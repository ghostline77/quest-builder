use anyhow::{Context, Result, ensure};
use clap::Args;
use ghostline_quest_builder::compiler;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Local tools used to generate series-owned character resources.
#[derive(Args, Default)]
pub struct Options {
    #[arg(long)]
    pub character_builder: Option<PathBuf>,
    #[arg(long)]
    pub catalogue: Option<PathBuf>,
    /// Reuse unchanged character projects from a previous series character export directory.
    #[arg(long)]
    pub character_exports: Option<PathBuf>,
}

struct Export {
    archive: PathBuf,
    tweaks: PathBuf,
    namespace: String,
    built: bool,
}

/// One export per character for the entire build invocation.
pub struct Characters {
    root: PathBuf,
    definitions: BTreeMap<String, PathBuf>,
    exports: BTreeMap<String, Export>,
}

impl Characters {
    pub fn new(root: &Path, definitions: BTreeMap<String, PathBuf>) -> Result<Self> {
        let root = root.canonicalize()?;
        let mut paths = std::collections::BTreeSet::new();
        for (id, path) in &definitions {
            ensure!(
                ghostline_quest_builder::manifest::identifier(id),
                "invalid series character {id}"
            );
            let path = root
                .join(path)
                .canonicalize()
                .with_context(|| format!("series character {id}"))?;
            ensure!(
                path.starts_with(&root) && path.is_file(),
                "character project must belong to its series: {id}"
            );
            ensure!(
                paths.insert(path),
                "character project registered more than once: {id}"
            );
        }
        Ok(Self {
            root,
            definitions,
            exports: BTreeMap::new(),
        })
    }

    pub fn check(&self, ids: &[String]) -> Result<()> {
        for id in ids {
            ensure!(
                self.definitions.contains_key(id),
                "unknown series character {id}; declare it in series.json"
            );
        }
        Ok(())
    }

    pub fn bind(
        &mut self,
        inputs: &mut compiler::Inputs,
        output: &Path,
        options: &Options,
        game: Option<&Path>,
        schema: Option<&Path>,
    ) -> Result<()> {
        self.check(&inputs.characters)?;
        for id in std::mem::take(&mut inputs.characters) {
            if !self.exports.contains_key(&id)
                && let Some(root) = &options.character_exports
            {
                let mut source: Value =
                    serde_json::from_slice(&fs::read(self.root.join(&self.definitions[&id]))?)?;
                let directory = root.join(&id);
                let mut saved: Value = serde_json::from_slice(&fs::read(
                    directory.join(format!("{id}.character.json")),
                )?)?;
                for project in [&mut source, &mut saved] {
                    // Older authored projects omit the optional display name;
                    // exporting writes its empty default without changing the NPC.
                    if let Some(identity) = project["identity"].as_object_mut()
                        && identity.get("display_name").and_then(Value::as_str) == Some("")
                    {
                        identity.remove("display_name");
                    }
                    if let Some(appearances) = project["appearances"].as_array_mut() {
                        for appearance in appearances {
                            if appearance
                                .get("native_parts")
                                .and_then(Value::as_array)
                                .is_some_and(Vec::is_empty)
                            {
                                appearance
                                    .as_object_mut()
                                    .context("character appearance")?
                                    .remove("native_parts");
                            }
                        }
                    }
                }
                ensure!(
                    source == saved,
                    "saved character {id} differs from the authored project; rebuild the character export"
                );
                let namespace = source["namespace"]
                    .as_str()
                    .context("character namespace")?
                    .replace('/', "\\");
                ghostline_quest_builder::project::Project::open(&self.root)?.owner(&namespace)?;
                ensure!(
                    !self
                        .exports
                        .values()
                        .any(|export| export.namespace.eq_ignore_ascii_case(&namespace)),
                    "series characters share a resource namespace: {namespace}"
                );
                let archive = directory.join(format!("{id}.archive"));
                let tweaks = directory.join(format!("{id}.yaml"));
                ensure!(
                    archive.is_file() && tweaks.is_file(),
                    "incomplete saved character export: {id}"
                );
                eprintln!("Reusing unchanged series character {id}");
                self.exports.insert(
                    id.clone(),
                    Export {
                        archive,
                        tweaks,
                        namespace,
                        built: false,
                    },
                );
            }
            if !self.exports.contains_key(&id) {
                let game = game.context("character generation requires --game")?;
                let project_path = self.root.join(&self.definitions[&id]);
                let project: Value = serde_json::from_slice(&fs::read(&project_path)?)?;
                ensure!(
                    project["id"].as_str() == Some(&id),
                    "series character key must match project id: {id}"
                );
                let namespace = project["namespace"]
                    .as_str()
                    .context("character namespace")?
                    .replace('/', "\\");
                ghostline_quest_builder::project::Project::open(&self.root)?.owner(&namespace)?;
                ensure!(
                    !self
                        .exports
                        .values()
                        .any(|e| e.namespace.eq_ignore_ascii_case(&namespace)),
                    "series characters share a resource namespace: {namespace}"
                );
                let builder = options
                    .character_builder
                    .clone()
                    .unwrap_or_else(default_builder);
                let catalogue = options
                    .catalogue
                    .clone()
                    .or_else(|| std::env::var_os("GHOSTLINE_ITEMS_CATALOGUE").map(PathBuf::from))
                    .unwrap_or_else(|| self.root.join("catalogue"));
                let directory = output.join(&id);
                eprintln!("Building series character {id}");
                let result = Command::new(&builder)
                    .arg("build")
                    .arg(&project_path)
                    .arg("--catalogue")
                    .arg(&catalogue)
                    .arg("--game")
                    .arg(game)
                    .args(
                        schema
                            .into_iter()
                            .flat_map(|path| [std::ffi::OsStr::new("--schema"), path.as_os_str()]),
                    )
                    .arg("--out")
                    .arg(&directory)
                    .arg("--export")
                    .output()
                    .with_context(|| {
                        format!(
                            "run character builder {}; use --character-builder to select it",
                            builder.display()
                        )
                    })?;
                ensure!(
                    result.status.success(),
                    "character {id} failed:\n{}\n{}",
                    String::from_utf8_lossy(&result.stderr),
                    String::from_utf8_lossy(&result.stdout)
                );
                let archive = directory.join(format!("{id}.archive"));
                let tweaks = directory.join(format!("{id}.yaml"));
                ensure!(
                    archive.is_file() && tweaks.is_file(),
                    "character builder did not produce complete outputs for {id}"
                );
                self.exports.insert(
                    id.clone(),
                    Export {
                        archive,
                        tweaks,
                        namespace,
                        built: true,
                    },
                );
            }
            let export = &self.exports[&id];
            let series: Value = serde_json::from_slice(&fs::read(self.root.join("project.json"))?)?;
            let series = series["id"].as_str().context("series project id")?;
            let loose = format!("r6/tweaks/{series}/character_{id}.yaml");
            ensure!(
                !inputs.tweaks.contains_key(&loose),
                "character TweakXL is supplied twice: {id}"
            );
            inputs.tweaks.insert(loose.clone(), export.tweaks.clone());
            inputs.required_loose.insert(loose);
            inputs.character_bundles.insert(export.namespace.clone());
            if !inputs.archives.contains(&export.archive) {
                inputs.archives.push(export.archive.clone());
            }
        }
        Ok(())
    }

    pub fn report(&self) -> Value {
        json!(self.exports.iter().map(|(id, export)| (id, json!({
            "project": self.definitions[id], "namespace": export.namespace, "build_count": u8::from(export.built), "reused": !export.built
        }))).collect::<BTreeMap<_, _>>())
    }

    pub fn relocate_reports(&self, destination: &Path) -> Result<()> {
        for (id, export) in &self.exports {
            if !export.built {
                continue;
            }
            let path = export
                .archive
                .parent()
                .context("character output directory")?
                .join("export-report.json");
            let mut report: Value = serde_json::from_slice(&fs::read(&path)?)?;
            let directory = destination.join("characters").join(id);
            report["archive"] = json!(directory.join(format!("{id}.archive")));
            report["tweaks"] = json!(directory.join(format!("{id}.yaml")));
            fs::write(path, serde_json::to_vec_pretty(&report)?)?;
        }
        Ok(())
    }
}

fn default_builder() -> PathBuf {
    let name = format!(
        "ghostline-character-builder{}",
        std::env::consts::EXE_SUFFIX
    );
    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        candidates.push(parent.join(&name));
    }
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .unwrap_or_else(|| name.into())
}

pub fn for_manifest(path: &Path) -> Result<Characters> {
    let path = path.canonicalize()?;
    let root = path
        .ancestors()
        .skip(1)
        .find(|root| root.join("series.json").is_file())
        .context("character references require an owning series.json")?;
    let config: Value = serde_json::from_slice(&fs::read(root.join("series.json"))?)?;
    let definitions =
        serde_json::from_value(config.get("characters").cloned().unwrap_or(json!({})))?;
    Characters::new(root, definitions)
}
