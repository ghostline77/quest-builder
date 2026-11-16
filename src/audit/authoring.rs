//! Encounter validation against explicitly supplied world and character-builder outputs.
use super::*;
use serde_json::json;

fn yaml_json(value: &serde_yaml::Value) -> Result<Value> {
    match value {
        serde_yaml::Value::Tagged(value) => yaml_json(&value.value),
        serde_yaml::Value::Sequence(values) => values.iter().map(yaml_json).collect(),
        serde_yaml::Value::Mapping(values) => values
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.as_str().context("TweakXL string key")?.to_owned(),
                    yaml_json(v)?,
                ))
            })
            .collect(),
        value => Ok(serde_json::to_value(value)?),
    }
}
fn strings(value: &Value, all: &mut HashSet<String>) {
    match value {
        Value::String(v) => {
            all.insert(v.clone());
        }
        Value::Array(v) => {
            for v in v {
                strings(v, all);
            }
        }
        Value::Object(v) => {
            for v in v.values() {
                strings(v, all);
            }
        }
        _ => {}
    }
}
fn refs(value: &Value, all: &mut HashSet<String>) {
    match value {
        Value::Array(v) => {
            for v in v {
                refs(v, all);
            }
        }
        Value::Object(v) => {
            if let Some(reference) = v.get("ref").and_then(Value::as_str) {
                all.insert(reference.into());
            }
            for v in v.values() {
                refs(v, all);
            }
        }
        _ => {}
    }
}

/// Validates the same explicit boss/world requirements without reading a series repository.
pub fn encounters(
    manifest: &crate::manifest::Manifest,
    worlds: &[Value],
    loose: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    for stage in &manifest.stages {
        if stage.kind != "cyberpsycho_encounter" {
            continue;
        }
        let Some(authoring) = stage.fields.get("authoring") else {
            continue;
        };
        ensure!(
            authoring.as_object().is_some_and(|v| !v.is_empty()),
            "encounter authoring must declare at least one input"
        );
        if authoring.get("world_spec").is_some() {
            let community = stage.string("community")?;
            let entry = stage.string("boss_entry")?;
            let candidate = worlds
                .iter()
                .find_map(|world| {
                    let communities = world.get("community").into_iter().chain(
                        world
                            .get("communities")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten(),
                    );
                    communities
                        .filter(|c| c["ref"] == community)
                        .map(|c| (world, c))
                        .next()
                })
                .with_context(|| {
                    format!(
                        "stage {} needs its declared community in supplied worlds",
                        stage.id
                    )
                })?;
            let (world, community) = candidate;
            let boss = community
                .get("entries")
                .and_then(Value::as_array)
                .map_or_else(
                    || Some(community),
                    |entries| entries.iter().find(|e| e["entry"] == entry),
                )
                .context("world lacks the encounter boss entry")?;
            ensure!(
                boss["character"] == stage.string("boss_character")?,
                "encounter/world character record mismatch"
            );
            let active = boss
                .get("active_on_start")
                .or_else(|| community.get("active_on_start"));
            ensure!(
                stage.fields.get("activate") == Some(&json!(false))
                    || active.is_some_and(|v| v == false || v == 0),
                "quest-activated encounter community must be inactive on start"
            );
            let mut declared = HashSet::new();
            refs(world, &mut declared);
            let mut required = vec![stage.string("activation_trigger")?];
            for key in ["arena_trigger", "alerted_path"] {
                if let Some(value) = stage.optional(key) {
                    required.push(value);
                }
            }
            for value in stage
                .fields
                .get("alerted_spots")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                required.push(value);
            }
            for key in ["reveal", "cleanup"] {
                if let Some(value) = stage.fields.get(key).and_then(|v| v["trigger"].as_str()) {
                    required.push(value);
                }
            }
            ensure!(
                required
                    .iter()
                    .all(|reference| declared.contains(*reference)),
                "world is missing encounter references: {:?}",
                required
                    .iter()
                    .filter(|reference| !declared.contains(**reference))
                    .collect::<Vec<_>>()
            );
        }
        if authoring.get("tweak_file").is_some() {
            let record = stage.string("boss_character")?;
            let mut candidates = Vec::new();
            for (path, bytes) in loose {
                if path.ends_with(".yaml") || path.ends_with(".yml") {
                    let value = yaml_json(
                        &serde_yaml::from_slice(bytes)
                            .with_context(|| format!("TweakXL {path}"))?,
                    )?;
                    if let Some(record) = value.get(record) {
                        candidates.push(record.clone());
                    }
                }
            }
            ensure!(
                candidates.len() == 1,
                "encounter needs exactly one supplied TweakXL definition of {record}"
            );
            let record = &candidates[0];
            let mut values = HashSet::new();
            strings(record, &mut values);
            ensure!(
                record["rarity"] == "NPCRarity.Boss",
                "encounter requires Boss rarity"
            );
            for requirement in [
                "Cyberpsycho",
                "Character.Cyberpsycho_ModGroup",
                "Character.Cyberpsycho_HitReaction_Resistance",
                "TargetTracking.DefaultPreset",
                "UINameplate.CombatSettings",
                "ScanningNPCPresets.ScannerPreset_NPCFull",
            ] {
                ensure!(
                    values.contains(requirement),
                    "encounter TweakXL lacks {requirement}"
                );
            }
            for field in ["entityTemplatePath", "displayName", "fullDisplayName"] {
                ensure!(
                    record.get(field).is_some(),
                    "encounter TweakXL lacks {field}"
                );
            }
            ensure!(
                record["disableDefeatedState"] != true,
                "encounter must permit defeated state"
            );
        }
    }
    Ok(())
}
