//! Dependency selection and verified native archive/ZIP publication.
mod dependencies;
mod distribution;
mod modio;

use crate::project::depot_path;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// A package profile consumes immutable build receipts from explicitly named builds.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub locale: String,
    pub builds: Vec<PathBuf>,
    pub active_roots: Vec<String>,
    pub support_roots: Vec<String>,
    pub additional_roots: Vec<String>,
    pub streaming_sectors: Vec<String>,
    pub include_prefixes: Vec<String>,
    pub exclude_prefixes: Vec<String>,
    pub forbidden_prefixes: Vec<String>,
    pub include_all: bool,
    pub include_loose: Vec<String>,
    pub exclude_loose: Vec<String>,
    pub external_patch_targets: Vec<String>,
    pub character_bundles: Vec<String>,
    pub deduplicate_meshes: bool,
    pub minimum_archivexl: Option<String>,
    pub allow_unresolved_hashes: bool,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            name: String::new(),
            locale: "en-us".into(),
            builds: Vec::new(),
            active_roots: Vec::new(),
            support_roots: Vec::new(),
            additional_roots: Vec::new(),
            streaming_sectors: Vec::new(),
            include_prefixes: Vec::new(),
            exclude_prefixes: Vec::new(),
            forbidden_prefixes: Vec::new(),
            include_all: false,
            include_loose: Vec::new(),
            exclude_loose: Vec::new(),
            external_patch_targets: Vec::new(),
            character_bundles: Vec::new(),
            deduplicate_meshes: true,
            minimum_archivexl: None,
            allow_unresolved_hashes: false,
        }
    }
}

struct Resource {
    bytes: Vec<u8>,
    raw: Option<Value>,
    declared: BTreeSet<String>,
}
struct Inventory {
    required_loose: BTreeSet<String>,
    character_bundles: BTreeSet<String>,
    resources: BTreeMap<String, Resource>,
    loose: BTreeMap<String, Vec<u8>>,
    registration: Value,
    receipts: BTreeMap<String, String>,
}

/// A frozen package with dependency and provenance evidence, ready for publication.
pub struct Package {
    pub name: String,
    pub payloads: BTreeMap<String, Vec<u8>>,
    pub loose: BTreeMap<String, Vec<u8>>,
    pub registration: Value,
    pub audit: Value,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn normalized(path: &str) -> Result<String> {
    let path = path.replace('/', "\\").to_lowercase();
    depot_path(&path)?;
    Ok(path)
}
fn loose_path(path: &str) -> Result<PathBuf> {
    let path = depot_path(&format!("mod\\{}", path.replace('/', "\\")))?;
    Ok(path.strip_prefix("mod").context("loose path")?.to_owned())
}
fn merge(target: &mut Value, source: &Value) -> Result<()> {
    match (target, source) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                if let Some(existing) = a.get_mut(k) {
                    merge(existing, v)?;
                } else {
                    a.insert(k.clone(), v.clone());
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            for v in b {
                ensure!(!a.contains(v), "duplicate package registration {v}");
                a.push(v.clone());
            }
        }
        (a, b) => ensure!(a == b, "conflicting package registration"),
    };
    Ok(())
}

fn inventory(profile: &Profile, base: &Path) -> Result<Inventory> {
    ensure!(
        !profile.builds.is_empty(),
        "package requires at least one generated build"
    );
    let mut result = Inventory {
        required_loose: BTreeSet::new(),
        character_bundles: BTreeSet::new(),
        resources: BTreeMap::new(),
        loose: BTreeMap::new(),
        registration: json!({}),
        receipts: BTreeMap::new(),
    };
    let mut builds = BTreeSet::new();
    for build in &profile.builds {
        let root = base
            .join(build)
            .canonicalize()
            .with_context(|| format!("resolve generated build {}", build.display()))?;
        ensure!(builds.insert(root.clone()), "build supplied twice");
        let receipt_bytes = fs::read(root.join("build.json"))?;
        let receipt: Value = serde_json::from_slice(&receipt_bytes)?;
        for name in ["required_loose", "character_bundles"] {
            let values = receipt
                .get(name)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|v| {
                    v.as_str()
                        .context("character dependency path")
                        .map(String::from)
                })
                .collect::<Result<BTreeSet<_>>>()?;
            if name == "required_loose" {
                result.required_loose.extend(values);
            } else {
                result.character_bundles.extend(values);
            }
        }
        ensure!(
            receipt["schema_version"] == 1
                && receipt["native_serialized"] == true
                && receipt["prototype"] == false,
            "only complete non-prototype native builds can be packaged"
        );
        result
            .receipts
            .insert(root.display().to_string(), digest(&receipt_bytes));
        let binaries = receipt["binaries"]
            .as_object()
            .context("native build payload hashes")?;
        let raw_hashes = receipt["resources"]
            .as_object()
            .context("raw build hashes")?;
        ensure!(
            binaries
                .keys()
                .all(|k| raw_hashes.contains_key(k) || receipt["binary_assets"].get(k).is_some())
                && raw_hashes.keys().all(|k| binaries.contains_key(k)),
            "build receipt has an incomplete native payload set"
        );
        for (depot, hash) in binaries {
            let canonical = normalized(depot)?;
            let relative = depot_path(&depot.replace('/', "\\"))?;
            let bytes = fs::read(root.join("source/archive").join(&relative))?;
            ensure!(
                hash.as_str() == Some(digest(&bytes).as_str()),
                "native build payload changed since receipt: {depot}"
            );
            let raw = if let Some(expected) = raw_hashes.get(depot) {
                let mut raw_path = relative.into_os_string();
                raw_path.push(".json");
                let raw_bytes = fs::read(root.join("source/raw").join(raw_path))?;
                ensure!(
                    expected.as_str() == Some(digest(&raw_bytes).as_str()),
                    "raw build declaration changed since receipt: {depot}"
                );
                Some(serde_json::from_slice(&raw_bytes)?)
            } else {
                ensure!(
                    receipt["binary_assets"][depot].as_str() == Some(digest(&bytes).as_str()),
                    "external artifact receipt mismatch"
                );
                None
            };
            let declared = receipt["declared_dependencies"][depot]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| normalized(v.as_str().context("declared dependency")?))
                .collect::<Result<BTreeSet<_>>>()?;
            if let Some(existing) = result.resources.get_mut(&canonical) {
                ensure!(
                    existing.bytes == bytes,
                    "conflicting owned resource across builds: {depot}"
                );
                existing.declared.extend(declared);
                if existing.raw.is_none() {
                    existing.raw = raw;
                }
            } else {
                result.resources.insert(
                    canonical,
                    Resource {
                        bytes,
                        raw,
                        declared,
                    },
                );
            }
        }
        for (name, hash) in receipt
            .get("loose_resources")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let bytes = fs::read(root.join("source/resources").join(loose_path(name)?))?;
            ensure!(
                hash.as_str() == Some(digest(&bytes).as_str()),
                "loose build resource changed since receipt: {name}"
            );
            if name.ends_with(".archive.xl") {
                let registration: Value = serde_yaml::from_slice(&bytes)
                    .context("generated ArchiveXL registration must be structured JSON/YAML")?;
                merge(&mut result.registration, &registration)?;
            } else {
                let path = name.replace('\\', "/");
                if let Some(previous) = result.loose.get(&path) {
                    ensure!(previous == &bytes, "conflicting loose resource {path}");
                } else {
                    result.loose.insert(path, bytes);
                }
            }
        }
    }
    Ok(result)
}

fn filter_registration(registration: &mut Value, profile: &Profile) -> Result<()> {
    let mut roots = BTreeSet::new();
    for path in profile.active_roots.iter().chain(&profile.additional_roots) {
        let path = normalized(path)?;
        ensure!(
            path.ends_with(".questphase"),
            "active/additional root must be a quest phase"
        );
        ensure!(roots.insert(path), "duplicate active/additional quest root");
    }
    if !roots.is_empty() {
        let phases = registration
            .pointer_mut("/quest/phases")
            .and_then(Value::as_array_mut)
            .context("active quest roots require phase registrations")?;
        phases.retain(|v| {
            v["path"]
                .as_str()
                .and_then(|v| normalized(v).ok())
                .is_some_and(|p| roots.contains(&p))
        });
        let mut registered = BTreeSet::new();
        for entry in phases {
            let path = normalized(entry["path"].as_str().context("phase registration path")?)?;
            ensure!(
                registered.insert(path),
                "quest root registered more than once"
            );
        }
        ensure!(
            registered == roots,
            "each active and additional quest root needs exactly one registration"
        );
    }
    if !profile.include_all {
        let namespaces = profile
            .active_roots
            .iter()
            .chain(&profile.support_roots)
            .chain(&profile.additional_roots)
            .filter_map(|v| {
                v.replace('/', "\\")
                    .split('\\')
                    .nth(1)
                    .map(str::to_lowercase)
            })
            .collect::<BTreeSet<_>>();
        let keep = |path: &str| -> bool {
            let path = path.replace('/', "\\").to_lowercase();
            path.split('\\')
                .nth(1)
                .is_some_and(|ns| namespaces.contains(ns))
                || profile
                    .include_prefixes
                    .iter()
                    .any(|prefix| path.starts_with(&prefix.replace('/', "\\").to_lowercase()))
        };
        if let Some(journals) = registration
            .get_mut("journal")
            .and_then(Value::as_array_mut)
        {
            journals.retain(|v| v.as_str().is_some_and(&keep));
        }
        if let Some(localization) = registration
            .get_mut("localization")
            .and_then(Value::as_object_mut)
        {
            for locales in localization.values_mut() {
                for entries in locales
                    .as_object_mut()
                    .context("localization locale registry")?
                    .values_mut()
                {
                    entries
                        .as_array_mut()
                        .context("localized registry paths")?
                        .retain(|v| v.as_str().is_some_and(&keep));
                }
            }
        }
        if let Some(blocks) = registration
            .pointer_mut("/streaming/blocks")
            .and_then(Value::as_array_mut)
        {
            blocks.retain(|v| v.as_str().is_some_and(&keep));
        }
        for field in ["patch", "link"] {
            if let Some(entries) = registration
                .pointer_mut(&format!("/resource/{field}"))
                .and_then(Value::as_object_mut)
            {
                entries.retain(|source, _| keep(source));
            }
        }
    }
    if let Some(sectors) = registration
        .pointer_mut("/streaming/sectors")
        .and_then(Value::as_array_mut)
    {
        let requested = profile
            .streaming_sectors
            .iter()
            .map(|v| normalized(v))
            .collect::<Result<BTreeSet<_>>>()?;
        ensure!(
            requested.len() == profile.streaming_sectors.len(),
            "duplicate streaming sector request"
        );
        sectors.retain(|v| {
            v["path"]
                .as_str()
                .and_then(|v| normalized(v).ok())
                .is_some_and(|v| requested.contains(&v))
        });
        let actual = sectors
            .iter()
            .map(|v| normalized(v["path"].as_str().context("streaming sector patch path")?))
            .collect::<Result<BTreeSet<_>>>()?;
        ensure!(
            actual == requested && sectors.len() == requested.len(),
            "each streaming sector patch requires exactly one registration"
        );
    } else {
        ensure!(
            profile.streaming_sectors.is_empty(),
            "requested streaming sector patch is not registered"
        );
    }
    Ok(())
}

/// Validates a portable archive and installation ownership name.
pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        && depot_path(&format!("mod\\{name}\\package.archive")).is_ok()
}

/// Resolves the selected dependency closure and freezes every byte before packing.
pub fn prepare(profile: &Profile, base: &Path) -> Result<Package> {
    ensure!(valid_name(&profile.name), "invalid package name");
    let mut inventory = inventory(profile, base)?;
    filter_registration(&mut inventory.registration, profile)?;
    let mut selection = dependencies::select(profile, &inventory)?;
    let mut payloads = selection
        .selected
        .iter()
        .map(|path| {
            Ok((
                path.clone(),
                inventory
                    .resources
                    .get(path)
                    .context("selected payload missing")?
                    .bytes
                    .clone(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut aliases = BTreeMap::<String, Vec<String>>::new();
    let mut removed = 0u64;
    if profile.deduplicate_meshes {
        let mut groups = BTreeMap::<String, Vec<String>>::new();
        for (path, bytes) in &payloads {
            if path.ends_with(".mesh") {
                groups.entry(digest(bytes)).or_default().push(path.clone());
            }
        }
        for (hash, paths) in groups {
            if paths.len() < 2 {
                continue;
            }
            let canonical = format!("mod\\{}\\shared\\{hash}.mesh", profile.name);
            let bytes = payloads[&paths[0]].clone();
            if let Some(existing) = payloads.get(&canonical) {
                ensure!(*existing == bytes, "deduplicated mesh identity collision");
            }
            for path in &paths {
                payloads.remove(path);
            }
            removed += u64::try_from(bytes.len())? * u64::try_from(paths.len() - 1)?;
            payloads.insert(canonical.clone(), bytes);
            let targets = paths
                .into_iter()
                .filter(|path| path != &canonical)
                .collect();
            aliases.insert(canonical, targets);
        }
    }
    if !aliases.is_empty() {
        merge(
            &mut inventory.registration,
            &json!({"resource":{"link":aliases}}),
        )?;
    }
    let mut minimum = profile.minimum_archivexl.clone();
    if let Some(version) = &minimum {
        ensure!(
            (2..=3).contains(&version.split('.').count())
                && version
                    .split('.')
                    .all(|v| !v.is_empty() && v.parse::<u32>().is_ok()),
            "minimum_archivexl must be a numeric version"
        );
    }
    if !aliases.is_empty() {
        let version = minimum
            .as_deref()
            .unwrap_or("0")
            .split('.')
            .map(|v| v.parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>();
        if version.as_slice() < [1, 14].as_slice() {
            minimum = Some("1.14".into());
        }
    }
    let loose = inventory
        .loose
        .into_iter()
        .filter(|(path, _)| {
            (!profile.exclude_loose.contains(path))
                && (profile.include_all
                    || profile.include_loose.contains(path)
                    || inventory.required_loose.contains(path))
        })
        .collect::<BTreeMap<_, _>>();
    for path in profile
        .include_loose
        .iter()
        .chain(&inventory.required_loose)
    {
        loose_path(path)?;
        ensure!(
            loose.contains_key(path),
            "missing or excluded explicit loose resource {path}"
        );
    }
    selection.audit["source_receipts"] = json!(inventory.receipts);
    selection.audit["required_loose"] = json!(inventory.required_loose);
    selection.audit["character_bundles"] = json!(inventory.character_bundles);
    selection.audit["mesh_aliases"] = json!(aliases);
    selection.audit["deduplicated_bytes"] = json!(removed);
    selection.audit["minimum_archivexl"] = json!(minimum);
    selection.audit["runtime_tested"] = json!(false);
    selection.audit["game_dependencies_verified"] = json!(false);
    Ok(Package {
        name: profile.name.clone(),
        payloads,
        loose,
        registration: inventory.registration,
        audit: selection.audit,
    })
}

/// Publishes an archive, installed-folder layout and verified distributable ZIP.
pub fn publish(package: &Package, output: &Path) -> Result<Value> {
    distribution::publish(package, output)
}

/// Publishes the mod.io folder layout, validating console resource types and loose data.
/// This does not replace the platform validator or a console playtest.
pub fn publish_modio(package: &Package, output: &Path) -> Result<Value> {
    distribution::publish_modio(package, output)
}

/// Checks whether the package uses supported console resources and loose-file formats.
/// Engine/platform validation is still required for the resulting mod.io package.
pub fn validate_modio(package: &Package) -> Result<()> {
    modio::validate(package)
}
