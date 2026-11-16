//! Explicit standalone project paths and resource ownership.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

/// A standalone project's authored inputs and declared dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub schema_version: u32,
    pub id: String,
    pub depot_prefixes: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<PathBuf>,
}

/// Resolved paths for one project; game depot identities never depend on them.
#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub config: ProjectConfig,
}
impl Project {
    /// Opens an explicit project directory; no global repository registry is read.
    pub fn open(root: &Path) -> Result<Self> {
        let root = root
            .canonicalize()
            .with_context(|| format!("project directory {}", root.display()))?;
        let mut config: ProjectConfig =
            serde_json::from_slice(&std::fs::read(root.join("project.json"))?)?;
        ensure!(
            config.schema_version == 1,
            "unsupported project schema {}",
            config.schema_version
        );
        ensure!(
            crate::manifest::identifier(&config.id.replace('-', "_")),
            "invalid project id"
        );
        ensure!(
            !config.depot_prefixes.is_empty(),
            "project needs explicit depot ownership"
        );
        let mut prefixes = HashSet::new();
        for prefix in &mut config.depot_prefixes {
            *prefix = prefix
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_ascii_lowercase();
            depot_path(prefix)?;
            ensure!(
                prefixes.insert(prefix.clone()),
                "duplicate project depot prefix"
            );
        }
        Ok(Self { root, config })
    }
    /// Maps an owned depot identity to its editable JSON and packed destinations.
    pub fn paths(&self, depot: &str) -> Result<(PathBuf, PathBuf)> {
        let depot = depot.replace('/', "\\").to_ascii_lowercase();
        let relative = depot_path(&depot)?;
        ensure!(
            self.config
                .depot_prefixes
                .iter()
                .any(|p| depot == p.trim_end_matches('\\')
                    || depot.starts_with(&format!("{}\\", p.trim_end_matches('\\')))),
            "project {} does not own {depot}",
            self.config.id
        );
        Ok((
            self.root
                .join("source/raw")
                .join(format!("{}.json", relative.to_string_lossy())),
            self.root.join("source/archive").join(relative),
        ))
    }
    /// Selects the longest declared owner from this project's dependency closure.
    pub fn owner(&self, depot: &str) -> Result<Self> {
        let depot = depot.replace('/', "\\").to_ascii_lowercase();
        depot_path(&depot)?;
        let mut matches = Vec::new();
        for project in self.closure()? {
            if let Some(length) = project
                .config
                .depot_prefixes
                .iter()
                .filter(|p| depot == **p || depot.starts_with(&format!("{p}\\")))
                .map(String::len)
                .max()
            {
                matches.push((length, project));
            }
        }
        matches
            .into_iter()
            .max_by_key(|(length, _)| *length)
            .map(|(_, p)| p)
            .context("no declared project owns this resource")
    }
    /// Resolves dependency closure and rejects cycles or overlapping depot owners.
    pub fn closure(&self) -> Result<Vec<Self>> {
        fn visit(
            project: Project,
            active: &mut HashSet<PathBuf>,
            done: &mut HashSet<PathBuf>,
            out: &mut Vec<Project>,
        ) -> Result<()> {
            if done.contains(&project.root) {
                return Ok(());
            }
            ensure!(
                active.insert(project.root.clone()),
                "project dependency cycle at {}",
                project.root.display()
            );
            for dep in &project.config.dependencies {
                visit(Project::open(&project.root.join(dep))?, active, done, out)?;
            }
            active.remove(&project.root);
            done.insert(project.root.clone());
            out.push(project);
            Ok(())
        }
        let mut result = Vec::new();
        visit(
            self.clone(),
            &mut HashSet::new(),
            &mut HashSet::new(),
            &mut result,
        )?;
        let mut prefixes = HashSet::new();
        let mut ids = HashSet::new();
        for project in &result {
            ensure!(
                ids.insert(&project.config.id),
                "duplicate project identity {}",
                project.config.id
            );
            for prefix in &project.config.depot_prefixes {
                ensure!(
                    prefixes.insert(prefix.trim_end_matches('\\').to_ascii_lowercase()),
                    "duplicate depot owner for {prefix}"
                );
            }
        }
        Ok(result)
    }
}

/// Validates a native depot identity without permitting Windows path aliases.
pub fn depot_path(value: &str) -> Result<PathBuf> {
    let parts = value.split('\\').collect::<Vec<_>>();
    ensure!(
        parts.len() > 1 && matches!(parts[0], "base" | "ep1" | "engine" | "mod"),
        "invalid depot path {value}"
    );
    for part in &parts {
        ensure!(
            !part.is_empty() && *part != "." && *part != ".." && !part.ends_with(['.', ' ']),
            "invalid depot component in {value}"
        );
        ensure!(
            !part
                .chars()
                .any(|c| c.is_control() || "/:<>\"|?*".contains(c)),
            "invalid depot characters in {value}"
        );
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        ensure!(
            !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && stem.as_bytes()[3].is_ascii_digit()),
            "reserved Windows depot component {part}"
        );
    }
    Ok(parts.into_iter().collect())
}
