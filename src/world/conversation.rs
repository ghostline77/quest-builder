//! Generates the scene-selection resource referenced by a conversation area.
use super::{Handles, document, geometry, nodes};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub(super) fn build(spec: &Value, name: &str, reference: &str) -> Result<(Value, String, Value)> {
    let scene = spec["scene"].as_str().context("conversation scene path")?;
    ensure!(
        scene.ends_with(".scene"),
        "conversation requires a .scene resource"
    );
    crate::project::depot_path(scene)?;
    let path = spec.get("group_path").and_then(Value::as_str).map_or_else(
        || {
            format!(
                "mod\\{name}\\world\\conversations\\{:016x}.conversations",
                geometry::hash(reference)
            )
        },
        |value| value.replace('/', "\\"),
    );
    crate::project::depot_path(&path)?;
    ensure!(
        path.ends_with(".conversations"),
        "conversation group resource requires .conversations extension"
    );
    let mut h = Handles::default();
    let candidate = h.wrap(json!({"$type":"scnInterestingConversationData","sceneFilename":nodes::resource(scene),"interruptionOperations":[]}));
    let group = h.wrap(json!({"$type":"scnInterestingConversationsGroup","condition":null,"conversations":[candidate]}));
    let resource = document(
        &path,
        json!({"$type":"scnInterestingConversationsResource","cookingPlatform":"PLATFORM_PC","conversationGroups":[group]}),
    );
    let mut model = spec.clone();
    model["group_path"] = json!(path);
    Ok((model, path, resource))
}
