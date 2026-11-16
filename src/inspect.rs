//! Focused resource inspection for humans, agents and research indexes.
use anyhow::{Context, Result, ensure};
use ghostline_red::{codec, schema::RedSchema};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

/// Reads an authoring resource document or decodes native CR2W with an explicit schema.
pub fn read(path: &Path, schema: Option<&RedSchema>) -> Result<Value> {
    let bytes = fs::read(path)?;
    decode(&bytes, schema)
}

/// Inspects structural imports/embedded payload identities alongside the decoded resource.
pub fn read_summary(path: &Path, schema: Option<&RedSchema>) -> Result<Value> {
    summarize_bytes(&fs::read(path)?, schema)
}

/// Byte-backed summaries retain native dependencies even when a value is not a ResourcePath field.
pub fn summarize_bytes(bytes: &[u8], schema: Option<&RedSchema>) -> Result<Value> {
    let mut summary = summarize(&decode(bytes, schema)?)?;
    if bytes.starts_with(b"CR2W") {
        let native = ghostline_red::cr2w::inspect_bytes(bytes)?;
        let mut paths = summary["resources"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        paths.extend(
            native
                .imports
                .iter()
                .map(|import| import.depot_path.clone()),
        );
        summary["resources"] = json!(paths);
        summary["native"] = json!({"version":native.header.version,"imports":native.imports,"exports":native.exports,"buffers":native.buffers,"embedded":native.embedded});
    }
    Ok(summary)
}
/// Decodes bytes without resolving imports or accessing another repository.
pub fn decode(bytes: &[u8], schema: Option<&RedSchema>) -> Result<Value> {
    if bytes.starts_with(b"CR2W") {
        let automatic;
        let schema = if let Some(schema) = schema {
            schema
        } else {
            automatic = RedSchema::bundled()?;
            &automatic
        };
        codec::decode_wkit_from_bytes(bytes, schema).map_err(Into::into)
    } else {
        serde_json::from_slice(bytes).context("resource JSON")
    }
}
fn name(v: &Value) -> Option<&str> {
    v.as_str()
        .or_else(|| v.get("$value").and_then(Value::as_str))
}
fn walk<'a>(v: &'a Value, pointer: &str, all: &mut Vec<(String, &'a Value)>) {
    if v.is_object() {
        all.push((pointer.into(), v));
    }
    match v {
        Value::Object(v) => {
            for (key, child) in v {
                walk(
                    child,
                    &format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1")),
                    all,
                );
            }
        }
        Value::Array(v) => {
            for (index, child) in v.iter().enumerate() {
                walk(child, &format!("{pointer}/{index}"), all);
            }
        }
        _ => {}
    }
}
struct View<'a> {
    handles: BTreeMap<&'a str, &'a Value>,
}
impl<'a> View<'a> {
    fn resolve(&self, value: &'a Value) -> &'a Value {
        value
            .get("HandleId")
            .or_else(|| value.get("HandleRefId"))
            .and_then(Value::as_str)
            .and_then(|id| self.handles.get(id).copied())
            .unwrap_or(value)
    }
}

/// Returns identities, graph wiring, actors, journal paths, assets and technical counts.
/// Large geometry, animation and texture buffers are represented by counts, not dumped.
pub fn summarize(document: &Value) -> Result<Value> {
    let root = document
        .pointer("/Data/RootChunk")
        .context("resource root chunk")?;
    let mut all = Vec::new();
    walk(root, "/Data/RootChunk", &mut all);
    let mut handles = BTreeMap::new();
    for (_, value) in &all {
        if let Some(id) = value.get("HandleId").and_then(Value::as_str) {
            ensure!(
                handles.insert(id, &value["Data"]).is_none(),
                "duplicate resource handle {id}"
            );
        }
    }
    let view = View { handles };
    let mut socket_owners = BTreeMap::new();
    for (_, node) in &all {
        if let Some(sockets) = node["sockets"].as_array() {
            for socket in sockets {
                if let Some(handle) = socket
                    .get("HandleId")
                    .or_else(|| socket.get("HandleRefId"))
                    .and_then(Value::as_str)
                {
                    socket_owners.insert(handle, node["id"].clone());
                }
            }
        }
    }
    let mut types = BTreeMap::<String, usize>::new();
    let mut assets = BTreeSet::new();
    let mut world_refs = BTreeSet::new();
    let mut facts_read = BTreeSet::new();
    let mut facts_written = BTreeSet::new();
    let mut journal_paths = BTreeSet::new();
    let mut graph = Vec::new();
    let mut actors = Vec::new();
    let mut components = Vec::new();
    let mut events = Vec::new();
    let mut localization = Vec::new();
    let mut occurrences = Vec::new();
    let mut animation = Vec::new();
    let mut appearances = Vec::new();
    for (pointer, value) in &all {
        if value.get("stringId").is_some() {
            localization.push(json!({"pointer":pointer,"string_id":value["stringId"],"female":value.get("femaleVariant"),"male":value.get("maleVariant"),"female_audio":value.get("femaleResPath"),"male_audio":value.get("maleResPath")}));
        }
        let Some(kind) = value.get("$type").and_then(Value::as_str) else {
            continue;
        };
        *types.entry(kind.into()).or_default() += 1;
        if kind == "appearanceAppearanceDefinition" {
            appearances.push(json!({"pointer":pointer,"name":name(&value["name"]),"components":value["components"].as_array().map(Vec::len),"parts":value.get("partsValues"),"overrides":value.get("partsOverrides")}));
        }
        if matches!(
            kind,
            "ResourcePath"
                | "NodeRef"
                | "questSetVar_NodeType"
                | "questVarComparison_ConditionType"
                | "gameJournalPath"
        ) {
            occurrences.push(json!({"pointer":pointer,"type":kind,"value":value}));
        }
        let fields = [
            "animationName",
            "animName",
            "animation",
            "actorComponent",
            "componentName",
            "animationSet",
            "animSet",
            "ridAnimSet",
            "animSets",
            "trackNames",
        ];
        let details = fields
            .into_iter()
            .filter_map(|key| value.get(key).map(|v| (key, v)))
            .collect::<BTreeMap<_, _>>();
        if !details.is_empty() {
            animation.push(json!({"pointer":pointer,"type":kind,"fields":details}));
        }
        match kind {
            "ResourcePath" => {
                if let Some(path) = name(value).filter(|v| !v.is_empty() && *v != "0") {
                    assets.insert(path);
                }
            }
            "NodeRef" => {
                if let Some(path) = name(value).filter(|v| !v.is_empty() && *v != "0") {
                    world_refs.insert(path);
                }
            }
            "questSetVar_NodeType" => {
                if let Some(fact) = name(&value["factName"]) {
                    facts_written.insert(fact);
                }
            }
            "questVarComparison_ConditionType" => {
                if let Some(fact) = name(&value["factName"]) {
                    facts_read.insert(fact);
                }
            }
            "gameJournalPath" => {
                if let Some(path) = value["realPath"].as_str() {
                    journal_paths.insert(path);
                }
            }
            "scnActorDef" | "scnPlayerActorDef" => {
                actors.push(json!({"id":value["actorId"]["id"],"name":value.get("actorName").or_else(||value.get("playerName")),"acquisition":value["acquisitionPlan"],"community":name(&value["communityParams"]["reference"]),"entry":name(&value["communityParams"]["entryName"]),"record":name(&value["specCharacterRecordId"])}));
            }
            "scnDialogLineEvent"
            | "scnPlaySkAnimEvent"
            | "scnPlayRidAnimEvent"
            | "scneventsCameraEvent"
            | "scneventsPlayRidCameraAnimEvent"
            | "scnChangeIdleAnimEvent"
            | "scnLookAtEvent"
            | "scnPoseCorrectionEvent"
            | "scneventsSocket" => {
                events.push(json!({"pointer":pointer,"type":kind,"id":value["id"]["id"],"start_ms":value["startTime"],"duration_ms":value["duration"],"performer":value.get("performer").or_else(||value.get("performerId")),"screenplay_item":value["screenplayLineId"]["id"]}));
            }
            "localizationPersistenceOnScreenEntry" => {
                localization.push(json!({"primary_key":value["primaryKey"],"secondary_key":value["secondaryKey"],"female":value["femaleVariant"],"male":value["maleVariant"]}));
            }
            _ => {}
        }
        if value.get("sockets").is_some() && value.get("id").is_some() {
            let sockets = value["sockets"].as_array().context("node sockets")?;
            let mut ports = Vec::new();
            for socket in sockets {
                let socket = view.resolve(socket);
                let mut connections = Vec::new();
                for connection in socket["connections"].as_array().into_iter().flatten() {
                    let connection = view.resolve(connection);
                    let endpoint = |key| {
                        let wrapper = &connection[key];
                        let node = wrapper
                            .get("HandleId")
                            .or_else(|| wrapper.get("HandleRefId"))
                            .and_then(Value::as_str)
                            .and_then(|id| socket_owners.get(id));
                        json!({"node":node,"port":name(&view.resolve(wrapper)["name"])})
                    };
                    connections.push(
                        json!({"source":endpoint("source"),"destination":endpoint("destination")}),
                    );
                }
                ports.push(json!({"name":name(&socket["name"]),"direction":socket["type"],"connections":connections}));
            }
            graph.push(json!({"pointer":pointer,"id":value["id"],"type":kind,"ports":ports,"scene":name(&value["sceneFile"]["DepotPath"]),"phase":name(&value["phaseResource"]["DepotPath"])}));
        }
        if value.get("nodeId").is_some() && value.get("outputSockets").is_some() {
            graph.push(json!({"pointer":pointer,"id":value["nodeId"]["id"],"type":kind,"outputs":value["outputSockets"],"duration":value["sectionDuration"]["stu"]}));
        }
        if (kind.ends_with("Component")
            || kind.ends_with("ComponentDefinition")
            || kind.ends_with("Controller"))
            && value.get("name").is_some()
        {
            components.push(json!({"type":kind,"name":name(&value["name"]),"id":value["id"],"mesh":name(&value["mesh"]["DepotPath"]),"mesh_appearance":name(&value["meshAppearance"]),"chunk_mask":value["chunkMask"],"enabled":value["isEnabled"]}));
        }
    }
    let mut journal_entries = Vec::new();
    fn entries(
        value: &Value,
        prefix: &str,
        view: &View<'_>,
        result: &mut Vec<Value>,
        active: &mut BTreeSet<usize>,
    ) -> Result<()> {
        let value = view.resolve(value);
        let identity = std::ptr::from_ref(value) as usize;
        ensure!(active.insert(identity), "journal contains an entry cycle");
        let Some(kind) = value["$type"].as_str() else {
            active.remove(&identity);
            return Ok(());
        };
        if !kind.starts_with("gameJournal") {
            active.remove(&identity);
            return Ok(());
        }
        let id = name(&value["id"]).unwrap_or("");
        let path = if prefix.is_empty() {
            id.to_owned()
        } else if id.is_empty() {
            prefix.into()
        } else {
            format!("{prefix}/{id}")
        };
        result.push(json!({"path":path,"type":kind,"title":value.get("title").or_else(||value.get("name")),"optional":value["optional"]}));
        for child in value["entries"].as_array().into_iter().flatten() {
            entries(child, &path, view, result, active)?;
        }
        active.remove(&identity);
        Ok(())
    }
    if let Some(entry) = root.get("entry") {
        entries(entry, "", &view, &mut journal_entries, &mut BTreeSet::new())?;
    }
    let sectors = root.get("descriptors").cloned().unwrap_or(json!([]));
    let world_nodes=root.get("nodes").and_then(Value::as_array).map(|v|v.iter().map(|node|{let node=view.resolve(node);json!({"type":node["$type"],"name":name(&node["debugName"]),"ref":name(&node["sourceObjectId"]),"entity":name(&node["entityTemplate"]["DepotPath"])})}).collect::<Vec<_>>()).unwrap_or_default();
    Ok(
        json!({"kind":root["$type"],"depot":document["Header"]["ArchiveFileName"],"handles":view.handles.len(),"types":types,"resources":assets,"world_refs":world_refs,"facts":{"reads":facts_read,"writes":facts_written},"journal_paths":journal_paths,"graph":graph,"actors":actors,"appearances":appearances,"components":components,"events":events,"animation":animation,"occurrences":occurrences,"journal_entries":journal_entries,"localization":localization,"entry_points":root.get("entryPoints"),"exit_points":root.get("exitPoints"),"world_nodes":world_nodes,"sectors":sectors}),
    )
}

/// Searches an inspection result without exposing the resource's opaque buffers.
pub fn query(summary: &Value, query: &str) -> Value {
    let query = query.to_lowercase();
    let mut result = serde_json::Map::new();
    for (key, value) in summary.as_object().into_iter().flat_map(|v| v.iter()) {
        if let Some(rows) = value.as_array() {
            let found = rows
                .iter()
                .filter(|v| v.to_string().to_lowercase().contains(&query))
                .cloned()
                .collect::<Vec<_>>();
            if !found.is_empty() {
                result.insert(key.clone(), json!(found));
            }
        } else if value.to_string().to_lowercase().contains(&query) {
            result.insert(key.clone(), value.clone());
        }
    }
    Value::Object(result)
}
