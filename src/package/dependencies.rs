use super::*;
use ghostline_red::{archive::depot_path_hash, cr2w};
use std::collections::VecDeque;

pub(super) struct Selection {
    pub selected: BTreeSet<String>,
    pub audit: Value,
}

fn resources(value: &Value, paths: &mut BTreeSet<String>, hashes: &mut BTreeSet<u64>) {
    match value {
        Value::String(value) => {
            let normalized = value.replace('/', "\\").to_lowercase();
            if ["base\\", "ep1\\", "engine\\", "mod\\"]
                .iter()
                .any(|prefix| normalized.starts_with(prefix))
                && normalized
                    .rsplit('\\')
                    .next()
                    .is_some_and(|v| v.contains('.'))
            {
                paths.insert(normalized);
            }
        }
        Value::Array(values) => {
            for value in values {
                resources(value, paths, hashes);
            }
        }
        Value::Object(values) => {
            if value["$type"] == "ResourcePath" && value["$storage"] == "uint64" {
                if let Some(hash) = value["$value"]
                    .as_str()
                    .and_then(|v| v.parse::<u64>().ok())
                    .filter(|v| *v != 0)
                {
                    hashes.insert(hash);
                }
            } else {
                for (key, value) in values {
                    resources(&Value::String(key.clone()), paths, hashes);
                    resources(value, paths, hashes);
                }
            }
        }
        _ => {}
    }
}
fn matching(path: &str, prefixes: &[String]) -> bool {
    prefixes
        .iter()
        .any(|p| path.starts_with(&p.replace('/', "\\").to_lowercase()))
}

fn yaml_resources(value: &serde_yaml::Value, paths: &mut BTreeSet<String>) {
    match value {
        serde_yaml::Value::String(value) => resources(&json!(value), paths, &mut BTreeSet::new()),
        serde_yaml::Value::Sequence(values) => {
            for value in values {
                yaml_resources(value, paths);
            }
        }
        serde_yaml::Value::Mapping(values) => {
            for (key, value) in values {
                yaml_resources(key, paths);
                yaml_resources(value, paths);
            }
        }
        serde_yaml::Value::Tagged(value) => yaml_resources(&value.value, paths),
        _ => {}
    }
}

pub(super) fn select(profile: &Profile, inventory: &Inventory) -> Result<Selection> {
    let mut selected = BTreeSet::new();
    let mut external = BTreeSet::new();
    let mut additions = BTreeSet::new();
    let mut unresolved = BTreeMap::<String, Vec<String>>::new();
    let mut fallbacks = BTreeMap::<String, String>::new();
    let mut localized_references = BTreeMap::<String, String>::new();
    let mut localized_targets = BTreeSet::new();
    let lipmaps = inventory
        .registration
        .pointer("/localization/lipmaps")
        .and_then(|v| v.get(&profile.locale));
    if let Some(lipmaps) = lipmaps {
        let mut paths = BTreeSet::new();
        resources(lipmaps, &mut paths, &mut BTreeSet::new());
        for path in paths {
            if let Some(resource) = inventory.resources.get(&path) {
                if let Some(raw) = &resource.raw {
                    resources(raw, &mut localized_targets, &mut BTreeSet::new());
                }
                if resource.bytes.starts_with(b"CR2W") {
                    for import in cr2w::inspect_bytes(&resource.bytes)?.imports {
                        localized_targets.insert(normalized(&import.depot_path)?);
                    }
                }
            }
        }
    }
    let mut pending = VecDeque::new();
    let known_hashes = inventory
        .resources
        .keys()
        .map(|p| (depot_path_hash(p), p))
        .collect::<BTreeMap<_, _>>();
    ensure!(
        known_hashes.len() == inventory.resources.len(),
        "owned resource hash collision"
    );
    for path in inventory.resources.keys() {
        if (profile.include_all
            || matching(path, &profile.include_prefixes)
            || inventory
                .character_bundles
                .iter()
                .any(|p| path.starts_with(&(p.replace('/', "\\").to_lowercase() + "\\"))))
            && !matching(path, &profile.exclude_prefixes)
        {
            pending.push_back(path.clone());
        }
    }
    for root in profile
        .active_roots
        .iter()
        .chain(&profile.support_roots)
        .chain(&profile.additional_roots)
    {
        pending.push_back(normalized(root)?);
    }
    let mut registration = BTreeSet::new();
    resources(
        &inventory.registration,
        &mut registration,
        &mut BTreeSet::new(),
    );
    let patch_targets = inventory
        .registration
        .pointer("/resource/patch")
        .cloned()
        .unwrap_or(json!({}));
    let mut possible_targets = BTreeSet::new();
    if let Some(patches) = patch_targets.as_object() {
        let sources = patches
            .keys()
            .map(|v| normalized(v))
            .collect::<Result<BTreeSet<_>>>()?;
        ensure!(
            profile
                .external_patch_targets
                .iter()
                .map(|v| normalized(v))
                .collect::<Result<BTreeSet<_>>>()?
                .is_disjoint(&sources),
            "external patch targets cannot be owned patch sources"
        );
        for targets in patches.values() {
            resources(targets, &mut possible_targets, &mut BTreeSet::new());
        }
    }
    let optional = profile
        .external_patch_targets
        .iter()
        .map(|v| normalized(v))
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        optional.is_subset(&possible_targets),
        "external patch targets must be registered optional targets"
    );
    registration.retain(|v| !optional.contains(v));
    pending.extend(registration);
    for (path, bytes) in &inventory.loose {
        if !profile.exclude_loose.contains(path)
            && (profile.include_all
                || profile.include_loose.contains(path)
                || inventory.required_loose.contains(path))
        {
            if [".yaml", ".yml", ".xl", ".json"]
                .iter()
                .any(|suffix| path.ends_with(suffix))
            {
                let value: serde_yaml::Value = serde_yaml::from_slice(bytes)
                    .with_context(|| format!("structured loose resource {path}"))?;
                let mut paths = BTreeSet::new();
                yaml_resources(&value, &mut paths);
                pending.extend(paths);
            } else if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
                let mut paths = BTreeSet::new();
                resources(&value, &mut paths, &mut BTreeSet::new());
                pending.extend(paths);
            } else if [".yaml", ".yml", ".xl", ".json", ".reds", ".lua", ".ini"]
                .iter()
                .any(|suffix| path.ends_with(suffix))
            {
                let text = std::str::from_utf8(bytes).context("loose resource text encoding")?;
                for token in text.split(['\n', '\r', '\t', ' ', '\'', '"', ',', '[', ']']) {
                    let mut paths = BTreeSet::new();
                    resources(&json!(token), &mut paths, &mut BTreeSet::new());
                    pending.extend(paths);
                }
            }
        }
    }
    let mut inspected = BTreeSet::new();
    while let Some(path) = pending.pop_front() {
        let mut path = normalized(&path)?;
        if !inventory.resources.contains_key(&path)
            && let Some((owner, suffix)) = path.split_once("\\scenes\\lipsync\\en\\")
        {
            let localized = format!(
                "base\\localization\\{}\\lipsync\\{owner}\\scenes\\{suffix}",
                profile.locale
            );
            if localized_targets.contains(&localized)
                && inventory.resources.contains_key(&localized)
            {
                localized_references.insert(path, localized.clone());
                path = localized;
            }
        }
        ensure!(
            !matching(&path, &profile.forbidden_prefixes),
            "forbidden package dependency {path}"
        );
        ensure!(
            !matching(&path, &profile.exclude_prefixes),
            "dependency belongs to an excluded package section: {path}"
        );
        let Some(resource) = inventory.resources.get(&path) else {
            if (path.starts_with("base\\")
                || path.starts_with("ep1\\")
                || path.starts_with("engine\\"))
                && !(path.starts_with("base\\localization\\") && path.contains("\\lipsync\\mod\\"))
            {
                external.insert(path);
                continue;
            }
            anyhow::bail!("missing owned package dependency {path}");
        };
        if !selected.insert(path.clone()) {
            continue;
        }
        if !profile
            .active_roots
            .iter()
            .chain(&profile.support_roots)
            .chain(&profile.additional_roots)
            .any(|p| p.replace('/', "\\").to_lowercase() == path)
            && !profile.include_all
            && !matching(&path, &profile.include_prefixes)
        {
            additions.insert(path.clone());
        }
        inspected.insert(path.clone());
        let mut dependencies = resource.declared.clone();
        let mut hashes = BTreeSet::new();
        let mut embedded = BTreeSet::new();
        if resource.bytes.starts_with(b"CR2W") {
            let native = cr2w::inspect_bytes(&resource.bytes)?;
            for import in native.imports {
                dependencies.insert(normalized(&import.depot_path)?);
            }
            for entry in native.embedded {
                if !entry.depot_path.is_empty() {
                    embedded.insert(normalized(&entry.depot_path)?);
                }
            }
        }
        if let Some(raw) = &resource.raw {
            resources(raw, &mut dependencies, &mut hashes);
        } else if path.ends_with(".mesh") {
            for prefix in profile
                .character_bundles
                .iter()
                .chain(&inventory.character_bundles)
            {
                let prefix = prefix.replace('/', "\\").to_lowercase();
                if path.starts_with(&prefix) {
                    fallbacks.insert(
                        prefix.clone(),
                        "Mesh has no authored dependency metadata; preserve explicit owning bundle"
                            .into(),
                    );
                    pending.extend(
                        inventory
                            .resources
                            .keys()
                            .filter(|p| p.starts_with(&prefix))
                            .cloned(),
                    );
                }
            }
        }
        for hash in hashes {
            if let Some(path) = known_hashes.get(&hash) {
                dependencies.insert((*path).clone());
            } else {
                unresolved
                    .entry(path.clone())
                    .or_default()
                    .push(hash.to_string());
            }
        }
        dependencies.retain(|p| !embedded.contains(p));
        pending.extend(dependencies);
    }
    ensure!(
        profile.allow_unresolved_hashes || unresolved.is_empty(),
        "unresolved resource hashes require resolution before packaging: {}",
        serde_json::to_string(&unresolved)?
    );
    ensure!(!selected.is_empty(), "package profile selects no resources");
    let excluded = inventory
        .resources
        .keys()
        .filter(|p| !selected.contains(*p))
        .collect::<Vec<_>>();
    let audit = json!({"schema_version":1,"selected_paths":selected,"dependency_additions":additions,"external_game_dependencies":external,"external_patch_targets":optional,"localized_lipsync_references":localized_references,"unresolved_numeric_resource_paths":unresolved,"character_bundle_fallbacks":fallbacks,"excluded_paths":excluded,"inspected_paths":inspected,"dependency_scope":"Native CR2W imports plus authored paths and inventory-resolved resource hashes; external game assets are reported separately"});
    Ok(Selection { selected, audit })
}
