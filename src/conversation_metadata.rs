//! Native conversation admission metadata and registered scene replacements.

use crate::{compiler::Artifacts, nodes::cname, project::depot_path};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// The engine reads this registry before admitting an interesting conversation.
pub const DEPOT: &str = r"base\sound\metadata\cooked_metadata.audio_metadata";

fn text(value: &Value) -> Option<&str> {
    value.as_str().or_else(|| value["$value"].as_str())
}

fn path(value: &str) -> String {
    value.replace('/', "\\").to_ascii_lowercase()
}

/// Generates the admission record from the scene's actual context actors.
/// Editor-only character hints do not constrain runtime actor selection.
pub fn generate(scene: &str, document: &Value) -> Result<Value> {
    depot_path(scene)?;
    let root = &document["Data"]["RootChunk"];
    ensure!(
        root["$type"] == "scnSceneResource",
        "conversation needs a scene resource"
    );
    let mut conditions = BTreeMap::new();
    for actor in root["actors"].as_array().into_iter().flatten() {
        ensure!(
            actor["acquisitionPlan"] == "findInContext",
            "conversation actors must be acquired from context"
        );
        let params = &actor["findActorInContextParams"];
        ensure!(
            params["contextualName"] == "ContextActorName",
            "conversation actor needs ContextActorName acquisition"
        );
        let context = text(&params["contextActorName"])
            .filter(|name| !name.is_empty())
            .context("conversation actor has no context name")?;
        ensure!(
            !conditions.contains_key(context),
            "duplicate conversation context actor {context}"
        );
        conditions.insert(
            context.to_owned(),
            json!({
                "$type":"audioConversationCharacterCondition",
                "actorContextName":cname(context),
                "actorsInitialWorkspotNodeRefHash":crate::world::node_ref_hash(context).to_string()
            }),
        );
    }
    Ok(json!({
        "$type":"audioConversationItemMetadata",
        "sceneFile":cname(&path(scene)),
        "characterConditions":conditions.into_values().collect::<Vec<_>>(),
        "shouldSceneInstanceBeSaved":false
    }))
}

fn collect<'a>(value: &'a Value, kind: &str, result: &mut Vec<&'a Value>) {
    match value {
        Value::Object(object) => {
            if value["$type"] == kind {
                result.push(value);
            }
            for child in object.values() {
                collect(child, kind, result);
            }
        }
        Value::Array(array) => {
            for child in array {
                collect(child, kind, result);
            }
        }
        _ => {}
    }
}

fn conditions(entry: &Value) -> Result<BTreeMap<String, (u64, u64, u64)>> {
    let mut result = BTreeMap::new();
    for condition in entry["characterConditions"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let context = text(&condition["actorContextName"])
            .unwrap_or_default()
            .to_owned();
        let number = |key: &str| -> Result<u64> {
            match condition.get(key) {
                None => Ok(0),
                Some(v) => v.as_u64().map(Ok).unwrap_or_else(|| {
                    text(v)
                        .context("metadata uint64")?
                        .parse()
                        .context("metadata uint64")
                }),
            }
        };
        let voice = text(&condition["voiceTag"]).unwrap_or_default();
        let voice = if voice.is_empty() || voice == "None" {
            0
        } else {
            crate::world::node_ref_hash(voice)
        };
        ensure!(
            result
                .insert(
                    context,
                    (
                        number("characterRecordId")?,
                        number("actorsInitialWorkspotNodeRefHash")?,
                        voice
                    )
                )
                .is_none(),
            "duplicate metadata context actor"
        );
    }
    Ok(result)
}

/// Publishes generated scenes at explicitly selected registered paths.
/// Existing game scene data is never used as a generation base.
pub fn bind(artifacts: &mut Artifacts, replacements: &BTreeMap<String, String>) -> Result<()> {
    for (registered, source) in replacements {
        ensure!(
            registered.starts_with("base\\") || registered.starts_with("ep1\\"),
            "registered scene must be a game resource"
        );
        let mut scene = artifacts
            .get(source)
            .with_context(|| format!("missing generated conversation scene {source}"))?
            .clone();
        ensure!(
            scene["Data"]["RootChunk"]["$type"] == "scnSceneResource",
            "conversation replacement must be a scene"
        );
        scene["Header"]["ArchiveFileName"] = json!(registered);
        ensure!(
            !artifacts.contains_key(registered),
            "registered scene replacement collides with another resource: {registered}"
        );
        artifacts.insert(registered.clone(), scene);
    }
    Ok(())
}

/// Checks admission and actor bindings against the installed game's registry.
/// Replacements reuse a registration without changing global audio metadata.
pub fn validate(artifacts: &Artifacts, metadata: &Value) -> Result<Value> {
    let mut registries = Vec::new();
    collect(metadata, "audioConversationMetadata", &mut registries);
    let mut entries = BTreeMap::new();
    for registry in registries {
        for entry in registry["conversations"].as_array().into_iter().flatten() {
            let scene = text(&entry["sceneFile"]).context("registered conversation scene path")?;
            ensure!(
                entries.insert(path(scene), entry).is_none(),
                "duplicate registered conversation {scene}"
            );
        }
    }
    ensure!(
        !entries.is_empty(),
        "game has no conversation admission metadata"
    );
    let documents = artifacts
        .iter()
        .map(|(key, value)| (path(key), value))
        .collect::<BTreeMap<_, _>>();
    let mut references = Vec::new();
    for document in artifacts.values() {
        collect(document, "worldConversationData", &mut references);
        collect(document, "scnInterestingConversationData", &mut references);
    }
    let mut report = BTreeMap::new();
    for reference in references {
        let scene = text(&reference["sceneFilename"]["DepotPath"])
            .context("conversation scene reference")?;
        let scene = path(scene);
        let admitted = entries.get(&scene).with_context(|| format!("conversation {scene} has no native admission metadata; use registered_scene with an existing compatible registration; adding audio metadata has no verified console loader"))?;
        if let Some(document) = documents.get(&scene) {
            let generated = generate(&scene, document)?;
            ensure!(
                conditions(admitted)? == conditions(&generated)?,
                "registered conversation {scene} has different actor/workspot conditions from its generated replacement"
            );
            ensure!(
                !admitted["shouldSceneInstanceBeSaved"]
                    .as_bool()
                    .unwrap_or(false),
                "registered conversation {scene} saves scene instances and cannot provide a replayable entry"
            );
            let admission = json!({"replacement":scene,"generated_metadata":generated,"loading":"archive_replacement","registry":DEPOT});
            report.insert(scene, admission);
        }
    }
    Ok(json!(report))
}

/// Returns whether world resources require a native admission check.
pub fn required(artifacts: &Artifacts) -> bool {
    artifacts.values().any(|document| {
        let mut references = Vec::new();
        collect(document, "worldConversationData", &mut references);
        collect(document, "scnInterestingConversationData", &mut references);
        !references.is_empty()
    })
}

/// Audits conversation replacements against official game resources during a build.
/// Installed mod output is excluded from the registration baseline.
pub fn validate_build(
    build: &crate::compiler::Build,
    schema: &ghostline_red::schema::RedSchema,
    game: Option<&Path>,
) -> Result<()> {
    if !required(&build.artifacts) {
        return Ok(());
    }
    let game = game
        .context("interesting conversations require --game to verify native admission metadata")?;
    let mut pending = ["content", "ep1", "hot"]
        .map(|directory| game.join("archive/pc").join(directory))
        .to_vec();
    let mut paths = Vec::new();
    while let Some(directory) = pending.pop() {
        if !directory.exists() {
            continue;
        }
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("archive"))
            {
                paths.push(path);
            }
        }
    }
    let archives = ghostline_red::material::ArchiveSet::from_paths(paths)?;
    let bytes = archives.read_resource(DEPOT)?;
    let metadata = crate::inspect::decode(&bytes, Some(schema))?;
    validate(&build.artifacts, &metadata)?;
    Ok(())
}
