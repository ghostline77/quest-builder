//! Scene authoring and native resource construction.

mod actors;
mod commands;
mod graph;
mod names;
mod performance;
mod player_couch;
mod text;
mod timeline;
mod validate;

use crate::{nodes as n, project::depot_path};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

/// Native absent performer: invalid performer index with the type byte cleared.
pub const INVALID_PERFORMER_ID: u32 = 0xffff_ff00;

#[derive(Default)]
struct Handles(u32);
impl Handles {
    fn wrap(&mut self, data: Value) -> Value {
        self.0 += 1;
        json!({"HandleId":(self.0+2).to_string(),"Data":data})
    }
}
fn list<'a>(v: &'a Value, k: &str) -> Result<&'a [Value]> {
    v.get(k).map_or(Ok(&[][..]), |v| {
        v.as_array()
            .map(Vec::as_slice)
            .with_context(|| format!("{k} must be an array"))
    })
}
fn string<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v.get(k)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .with_context(|| format!("missing nonempty {k}"))
}
fn integer(v: &Value, k: &str, default: u32) -> Result<u32> {
    v.get(k).map_or(Ok(default), |v| {
        v.as_u64()
            .context("unsigned integer")
            .and_then(|v| u32::try_from(v).context("integer exceeds uint32"))
    })
}
fn number(v: &Value, k: &str, default: f64) -> Result<f64> {
    let value = v
        .get(k)
        .map_or(Ok(default), |v| v.as_f64().context("numeric value"))?;
    ensure!(value.is_finite(), "{k} must be finite");
    Ok(value)
}
fn flag(v: &Value, k: &str, default: bool) -> Result<u8> {
    Ok(u8::from(v.get(k).map_or(Ok(default), |v| {
        v.as_bool().context("boolean value")
    })?))
}
fn id(kind: &str, value: impl Into<Value>) -> Value {
    let key = if matches!(kind, "scnlocLocstringId" | "scnlocVariantId") {
        "ruid"
    } else {
        "id"
    };
    json!({"$type":kind,key:value.into()})
}
fn hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}
fn event_id(parts: &[&str]) -> Value {
    let mut h = hash(&parts.join(":"));
    if h == i64::MAX as u64 {
        h = h.wrapping_add(1);
    }
    id("scnSceneEventId", h.to_string())
}
fn resource(path: &str, soft: bool) -> Value {
    json!({"DepotPath":{"$type":"ResourcePath","$storage":if path.is_empty(){"uint64"}else{"string"},"$value":if path.is_empty(){"0"}else{path}},"Flags":if soft{"Soft"}else{"Default"}})
}
fn actor_entity(v: &Value) -> Result<Value> {
    let entries = v
        .get("entry")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .into_iter()
        .collect::<Vec<_>>();
    Ok(n::entity(Some(string(v, "entity_ref")?), &entries))
}
fn dynamic_entity(name: &str) -> Value {
    let mut entity = n::entity(None, &[]);
    entity["dynamicEntityUniqueName"] = n::cname(name);
    entity
}
fn performer(actor: u32) -> Result<Value> {
    Ok(id(
        "scnPerformerId",
        actor
            .checked_mul(256)
            .and_then(|v| v.checked_add(1))
            .context("actor performer ID overflow")?,
    ))
}
fn tweak(value: &str) -> Value {
    if value == "0" {
        json!({"$type":"TweakDBID","$storage":"uint64","$value":"0"})
    } else {
        n::tweak_id(value)
    }
}

/// Generates a scene from an authoring specification and its dialogue manifest.
/// The specification contains depot identities, never a base scene or donor path.
pub fn compile(spec: &Value, dialogue: &Value) -> Result<Value> {
    for field in ["section_node_ids", "choice_node_ids"] {
        ensure!(
            dialogue.get(field).is_none(),
            "{field} is compiler-owned; declare named scene nodes in the scene specification"
        );
    }
    let lowered = names::lower(spec)?;
    let expanded = performance::expand(&lowered)?;
    let spec = &expanded;
    validate::specification(spec)?;
    let object = spec.as_object().context("scene spec must be an object")?;
    for forbidden in ["base_scene", "choice_shell_node_id", "template", "donor"] {
        ensure!(
            !object.contains_key(forbidden),
            "{forbidden} is not a generation input; scenes are constructed directly"
        );
    }
    let name = string(spec, "name")?;
    let depot = string(spec, "archive_path")?.replace('/', "\\");
    depot_path(&depot)?;
    ensure!(
        depot.ends_with(".scene"),
        "scene resource must end in .scene"
    );
    let mut handles = Handles::default();
    let cast = actors::build(spec)?;
    let text = text::build(spec, dialogue, &cast.lookup)?;
    let graph = graph::build(spec, dialogue, &text, &cast.lookup, &mut handles)?;
    let refs = actors::resources(spec)?;
    let mut rid_resources = Vec::new();
    let mut ids = HashSet::new();
    for row in list(spec, "rid_resources")? {
        let resource_id = integer(row, "resource_id", 0)?;
        ensure!(ids.insert(resource_id), "duplicate RID resource ID");
        let path = string(row, "path")?;
        depot_path(&path.replace('/', "\\"))?;
        rid_resources.push(json!({"$type":"scnRidResourceHandler","id":id("scnRidResourceId",resource_id),"ridResource":resource(path,false)}));
    }
    let root = json!({"$type":"scnSceneResource","version":5,"cookingPlatform":"PLATFORM_PC","sceneCategoryTag":spec.get("scene_category").and_then(Value::as_str).unwrap_or("minorQuests"),"actors":cast.actors,"playerActors":cast.players,"props":cast.props,"debugSymbols":cast.debug,"entryPoints":graph.entries,"exitPoints":graph.exits,"screenplayStore":text.screenplay,"locStore":text.localization,"sceneGraph":{"HandleId":"2","Data":{"$type":"scnSceneGraph","endNodes":[],"graph":graph.nodes}},"effectDefinitions":[],"effectInstances":[],"executionTagEntries":[],"executionTags":[],"localMarkers":[],"notablePoints":list(spec,"choices")?.iter().map(|v|Ok(json!({"$type":"scnNotablePoint","nodeId":id("scnNodeId",integer(v,"node_id",0)?)}))).collect::<Result<Vec<_>>>()?,"referencePoints":[],"resouresReferences":refs,"ridResources":rid_resources,"voInfo":[],"workspotInstances":[],"workspots":[],"sceneSolutionHash":{"$type":"scnSceneSolutionHash","sceneSolutionHash":{"$type":"scnSceneSolutionHashHash","sceneSolutionHashDate":hash(name).to_string()}}});
    let mut root = root;
    root["sceneGraph"]["Data"]["startNodes"] = json!(graph.entries.iter().map(|entry|entry["nodeId"].clone()).collect::<Vec<_>>());
    root["sceneGraph"]["Data"]["endNodes"] = json!(graph.exits.iter().map(|exit|exit["nodeId"].clone()).collect::<Vec<_>>());
    let interrupt = handles.wrap(json!({"$type":"scnCheckSpeakersDistanceInterruptCondition"}));
    let resume = handles.wrap(json!({"$type":"scnCheckSpeakersDistanceReturnCondition"}));
    root["interruptionScenarios"] = if flag(spec,"speaker_distance_interruptions",true)? != 0 { json!([{
        "$type":"scnInterruptionScenario",
        "id":id("scnInterruptionScenarioId",0),
        "name":n::cname("Default"),
        "interruptConditions":[interrupt],
        "returnConditions":[resume]
    }]) } else { json!([]) };
    performance::apply(&mut root, spec, &mut handles)?;
    player_couch::apply(&mut root, spec, &mut handles)?;
    validate::references(&root)?;
    let document = json!({"Header":{"WolvenKitVersion":"8.17.4","WKitJsonVersion":"0.0.9","GameVersion":2000,"ExportedDateTime":spec.get("exported_datetime").and_then(Value::as_str).unwrap_or("1970-01-01T00:00:00Z"),"DataType":"CR2W","ArchiveFileName":depot},"Data":{"Version":195,"BuildVersion":0,"RootChunk":root,"EmbeddedFiles":[]}});
    crate::graph::order_handles(&document)
}
