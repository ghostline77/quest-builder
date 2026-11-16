//! Native graph topology, emitted fact contracts and bound resource interfaces.
use crate::manifest::Stage;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
mod authoring;
mod stage;
pub use authoring::encounters;
pub use stage::typed_fields;

fn name(value: &Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value.get("$value").and_then(Value::as_str))
}
fn handle(value: &Value) -> Result<&str> {
    value
        .get("HandleId")
        .or_else(|| value.get("HandleRefId"))
        .and_then(Value::as_str)
        .context("native handle identity")
}
fn collect<'a>(value: &'a Value, definitions: &mut HashMap<&'a str, &'a Value>) -> Result<()> {
    if let Some(id) = value.get("HandleId").and_then(Value::as_str) {
        ensure!(
            definitions.insert(id, &value["Data"]).is_none(),
            "duplicate native handle {id}"
        );
    }
    match value {
        Value::Object(values) => {
            for value in values.values() {
                collect(value, definitions)?
            }
        }
        Value::Array(values) => {
            for value in values {
                collect(value, definitions)?
            }
        }
        _ => {}
    }
    Ok(())
}

/// An independently decoded view of executable phase connections.
pub struct Phase<'a> {
    definitions: HashMap<&'a str, &'a Value>,
    nodes: BTreeMap<&'a str, &'a Value>,
    edges: BTreeMap<&'a str, Vec<&'a str>>,
}
impl<'a> Phase<'a> {
    /// Validates socket ownership, connection direction and reciprocal endpoints.
    pub fn read(document: &'a Value) -> Result<Self> {
        let mut definitions = HashMap::new();
        collect(document, &mut definitions)?;
        let graph_ref = document
            .pointer("/Data/RootChunk/graph")
            .context("phase graph")?;
        let graph = definitions
            .get(handle(graph_ref)?)
            .context("undefined graph handle")?;
        ensure!(
            graph["$type"] == "questGraphDefinition",
            "resource does not contain a quest graph"
        );
        let mut nodes = BTreeMap::new();
        let mut owners = HashMap::new();
        let mut numeric_ids = HashSet::new();
        for wrapper in graph["nodes"].as_array().context("graph nodes")? {
            let id = handle(wrapper)?;
            let node = *definitions.get(id).context("undefined graph node")?;
            ensure!(
                nodes.insert(id, node).is_none(),
                "node appears twice in phase graph"
            );
            ensure!(
                numeric_ids.insert(node["id"].as_u64().context("numeric quest node id")?),
                "duplicate numeric node identity"
            );
            for wrapper in node["sockets"].as_array().context("node sockets")? {
                let socket_id = handle(wrapper)?;
                let socket = *definitions.get(socket_id).context("undefined socket")?;
                let kind = socket["type"].as_str().context("socket direction")?;
                ensure!(
                    matches!(kind, "Input" | "Output" | "CutDestination"),
                    "invalid socket direction {kind}"
                );
                ensure!(
                    owners.insert(socket_id, (id, kind)).is_none(),
                    "socket has more than one owner"
                );
            }
        }
        let mut edges = BTreeMap::<&str, Vec<&str>>::new();
        let mut checked = HashSet::new();
        for (socket_id, (owner, kind)) in &owners {
            let socket = definitions[socket_id];
            for wrapper in socket["connections"]
                .as_array()
                .context("socket connections")?
            {
                let connection_id = handle(wrapper)?;
                let connection = *definitions
                    .get(connection_id)
                    .context("undefined connection")?;
                ensure!(
                    connection["$type"] == "graphGraphConnectionDefinition",
                    "socket references a non-connection handle"
                );
                let source = handle(&connection["source"])?;
                let destination = handle(&connection["destination"])?;
                ensure!(
                    *socket_id == source || *socket_id == destination,
                    "connection listed on unrelated socket"
                );
                let (from, source_kind) = owners
                    .get(source)
                    .context("connection source is not a graph socket")?;
                let (to, destination_kind) = owners
                    .get(destination)
                    .context("connection destination is not a graph socket")?;
                ensure!(
                    *source_kind == "Output"
                        && matches!(*destination_kind, "Input" | "CutDestination"),
                    "connection directions are invalid"
                );
                let reciprocal = if *socket_id == source {
                    destination
                } else {
                    source
                };
                ensure!(
                    definitions[reciprocal]["connections"]
                        .as_array()
                        .context("reciprocal socket connections")?
                        .iter()
                        .any(|v| handle(v).ok() == Some(connection_id)),
                    "connection missing reciprocal endpoint"
                );
                if checked.insert(connection_id) && *destination_kind != "CutDestination" {
                    edges.entry(*from).or_default().push(*to);
                }
            }
            let _ = (owner, kind);
        }
        Ok(Self {
            definitions,
            nodes,
            edges,
        })
    }
    fn resolve(&self, value: &Value) -> Result<&'a Value> {
        self.definitions
            .get(handle(value)?)
            .copied()
            .context("undefined handle")
    }
    fn ports(&self, node: &Value, direction: &str) -> Result<HashSet<String>> {
        node["sockets"]
            .as_array()
            .context("native sockets")?
            .iter()
            .map(|v| self.resolve(v))
            .filter_map(|v| match v {
                Ok(v) if v["type"] == direction => {
                    Some(name(&v["name"]).map(str::to_owned).context("socket name"))
                }
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }
    fn reachable(
        &self,
        starts: impl IntoIterator<Item = &'a str>,
        stop: &HashSet<&str>,
    ) -> HashSet<&'a str> {
        let mut pending = starts.into_iter().collect::<Vec<_>>();
        let mut seen = HashSet::new();
        while let Some(node) = pending.pop() {
            if !seen.insert(node) || stop.contains(node) {
                continue;
            }
            if let Some(next) = self.edges.get(node) {
                pending.extend(next.iter().copied());
            }
        }
        seen
    }

    /// Checks declarations against typed fact nodes and every executable outcome.
    pub fn contracts(&self, stage: &Stage) -> Result<()> {
        let Some(contract) = stage.fields.get("contract") else {
            return Ok(());
        };
        let mut emitted_reads = HashSet::new();
        let mut emitted_writes = HashSet::new();
        for value in self.definitions.values() {
            let fact = name(&value["factName"]);
            match value["$type"].as_str() {
                Some("questSetVar_NodeType") => {
                    if let Some(fact) = fact {
                        emitted_writes.insert(fact);
                    }
                }
                Some("questVarComparison_ConditionType" | "questFactsDBCondition") => {
                    if let Some(fact) = fact {
                        emitted_reads.insert(fact);
                    }
                }
                _ => {}
            }
        }
        for (key, emitted) in [
            ("reads_facts", &emitted_reads),
            ("writes_facts", &emitted_writes),
        ] {
            for fact in contract
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let fact = fact.as_str().context("declared fact name")?;
                ensure!(
                    emitted.contains(fact),
                    "stage {} declares {key} {fact}, absent from typed fact nodes",
                    stage.id
                );
            }
        }
        let inputs = stage.inputs()?;
        let start = inputs.get("start").context("stage start input")?;
        let outcomes = stage.outcomes()?;
        let starts = self
            .nodes
            .iter()
            .filter(|(_, node)| {
                node["$type"] == "questInputNodeDefinition"
                    && name(&node["socketName"]) == Some(start)
            })
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        let ends = self
            .nodes
            .iter()
            .filter(|(_, node)| {
                node["$type"] == "questOutputNodeDefinition"
                    && name(&node["socketName"])
                        .is_some_and(|name| outcomes.values().any(|v| v == name))
            })
            .map(|(id, _)| *id)
            .collect::<HashSet<_>>();
        for fact in contract
            .get("writes_facts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let fact = fact.as_str().context("write fact")?;
            let mut writers = HashSet::new();
            for (id, node) in &self.nodes {
                if node["$type"] == "questFactsDBManagerNodeDefinition" {
                    let operation = self.resolve(&node["type"])?;
                    if operation["$type"] == "questSetVar_NodeType"
                        && name(&operation["factName"]) == Some(fact)
                    {
                        writers.insert(*id);
                    }
                }
            }
            let bypass = self.reachable(starts.iter().copied(), &writers);
            ensure!(
                bypass.is_disjoint(&ends),
                "stage {} writes {fact} on only some outcome paths",
                stage.id
            );
        }
        for (key, direction) in [("scene_entries", "Input"), ("scene_exits", "Output")] {
            for (scene, required) in contract
                .get(key)
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                let mut ports = HashSet::new();
                let mut found = false;
                for node in self.nodes.values() {
                    if node["$type"] == "questSceneNodeDefinition"
                        && node
                            .pointer("/sceneFile/DepotPath/$value")
                            .and_then(Value::as_str)
                            == Some(scene)
                    {
                        found = true;
                        ports.extend(self.ports(node, direction)?);
                    }
                }
                ensure!(
                    found,
                    "stage {} contract refers to absent scene {scene}",
                    stage.id
                );
                for port in required.as_array().context("scene contract ports")? {
                    let port = port.as_str().context("scene contract port name")?;
                    ensure!(
                        ports.contains(port),
                        "scene {scene} lacks required {direction} socket {port}"
                    );
                }
            }
        }
        Ok(())
    }
}

/// Verifies an explicitly bound phase or scene's actual public interface.
pub fn interface(
    document: &Value,
    entries: &[String],
    exits: &BTreeMap<String, String>,
    notifications: &HashSet<String>,
) -> Result<()> {
    match document
        .pointer("/Data/RootChunk/$type")
        .and_then(Value::as_str)
    {
        Some("questQuestPhaseResource") => {
            let phase = Phase::read(document)?;
            let inputs = phase
                .nodes
                .values()
                .filter(|n| n["$type"] == "questInputNodeDefinition")
                .filter_map(|n| name(&n["socketName"]))
                .collect::<HashSet<_>>();
            let outputs = phase
                .nodes
                .values()
                .filter(|n| n["$type"] == "questOutputNodeDefinition")
                .filter_map(|n| Some((name(&n["socketName"])?, n["type"].as_str()?)))
                .collect::<HashMap<_, _>>();
            for entry in entries {
                ensure!(
                    inputs.contains(entry.as_str()),
                    "bound phase lacks entry {entry}"
                );
            }
            for (alias, socket) in exits {
                let expected = if notifications.contains(alias) {
                    "NonTerminating"
                } else {
                    "Terminating"
                };
                ensure!(
                    outputs.get(socket.as_str()) == Some(&expected),
                    "phase output {socket} must be {expected}"
                );
            }
            Ok(())
        }
        Some("scnSceneResource") => {
            let root = &document["Data"]["RootChunk"];
            let graph = root
                .pointer("/sceneGraph/Data/graph")
                .and_then(Value::as_array)
                .context("scene executable graph")?;
            let mut nodes = HashMap::new();
            for wrapper in graph {
                let node = &wrapper["Data"];
                let id = node
                    .pointer("/nodeId/id")
                    .and_then(Value::as_u64)
                    .context("scene node ID")?;
                ensure!(
                    nodes.insert(id, node).is_none(),
                    "duplicate scene node ID {id}"
                );
            }
            for (table, required) in [
                ("entryPoints", entries.to_vec()),
                ("exitPoints", exits.values().cloned().collect()),
            ] {
                let mut names = HashSet::new();
                for entry in root[table].as_array().context("scene public registry")? {
                    let name = name(&entry["name"]).context("scene entry/exit name")?;
                    ensure!(names.insert(name), "duplicate scene registry name {name}");
                    let id = entry
                        .pointer("/nodeId/id")
                        .and_then(Value::as_u64)
                        .context("scene registry target ID")?;
                    let expected = if table == "entryPoints" {
                        "scnStartNode"
                    } else {
                        "scnEndNode"
                    };
                    ensure!(
                        nodes.get(&id).is_some_and(|node| node["$type"] == expected),
                        "scene {table} {name} must target {expected}"
                    );
                }
                for required in required {
                    // These lifecycle outputs belong to the scene invocation itself,
                    // rather than the scene's authored exit-point registry.
                    if table == "exitPoints"
                        && matches!(required.as_str(), "Default INT" | "Default RET")
                    {
                        continue;
                    }
                    ensure!(
                        names.contains(required.as_str()),
                        "scene {table} lacks {required}"
                    );
                }
            }
            Ok(())
        }
        kind => anyhow::bail!("resource has no supported phase/scene interface: {kind:?}"),
    }
}

/// Lists executable phase and scene calls without treating them as generation inputs.
pub fn called_resources(artifacts: &crate::compiler::Artifacts) -> Result<HashSet<String>> {
    let mut paths = HashSet::new();
    for document in artifacts.values() {
        if document
            .pointer("/Data/RootChunk/$type")
            .and_then(Value::as_str)
            != Some("questQuestPhaseResource")
        {
            continue;
        }
        for node in Phase::read(document)?.nodes.values() {
            let field = match node["$type"].as_str() {
                Some("questSceneNodeDefinition") => "sceneFile",
                Some("questPhaseNodeDefinition") => "phaseResource",
                _ => continue,
            };
            if let Some(path) = node[field]
                .pointer("/DepotPath/$value")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty() && *v != "0")
            {
                paths.insert(path.to_owned());
            }
        }
    }
    Ok(paths)
}

/// Audits calls against resources present in the same build or inspection set.
pub fn bound_interfaces(artifacts: &crate::compiler::Artifacts) -> Result<()> {
    for (depot, document) in artifacts {
        if document
            .pointer("/Data/RootChunk/$type")
            .and_then(Value::as_str)
            != Some("questQuestPhaseResource")
        {
            continue;
        }
        let phase = Phase::read(document)?;
        for node in phase.nodes.values() {
            let field = match node["$type"].as_str() {
                Some("questSceneNodeDefinition") => "sceneFile",
                Some("questPhaseNodeDefinition") => "phaseResource",
                _ => continue,
            };
            let path = node
                .get(field)
                .and_then(|v| v.pointer("/DepotPath/$value"))
                .and_then(Value::as_str)
                .context("called resource depot")?;
            if let Some(target) = artifacts
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(path))
                .map(|(_, v)| v)
            {
                let entries = phase
                    .ports(node, "Input")?
                    .into_iter()
                    .filter(|v| v != "CutDestination")
                    .collect::<Vec<_>>();
                let exits = phase
                    .ports(node, "Output")?
                    .into_iter()
                    .filter(|v| !matches!(v.as_str(), "Default INT" | "Default RET"))
                    .map(|v| (v.clone(), v))
                    .collect();
                let notifications = if field == "phaseResource" {
                    let target_phase = Phase::read(target)?;
                    target_phase
                        .nodes
                        .values()
                        .filter(|n| {
                            n["$type"] == "questOutputNodeDefinition"
                                && n["type"] == "NonTerminating"
                        })
                        .filter_map(|n| name(&n["socketName"]).map(str::to_owned))
                        .collect()
                } else {
                    HashSet::new()
                };
                interface(target, &entries, &exits, &notifications)
                    .with_context(|| format!("resource interface from {depot} to {path}"))?;
            }
        }
    }
    Ok(())
}

/// A declared child call preserves terminal versus notification semantics.
pub struct Binding {
    pub path: String,
    pub entry: String,
    pub exits: BTreeMap<String, String>,
    pub notifications: HashSet<String>,
}

/// Collects explicit nested phase/scene contracts, including outcome effects.
pub fn bindings(manifest: &crate::manifest::Manifest) -> Result<Vec<Binding>> {
    fn visit(value: &Value, notifications: &HashSet<String>, out: &mut Vec<Binding>) -> Result<()> {
        if let Some(object) = value.as_object() {
            if let Some(path) = object
                .get("phase")
                .or_else(|| object.get("scene"))
                .and_then(Value::as_str)
                && let Some(entry) = object.get("entry").and_then(Value::as_str)
            {
                let exits = if let Some(exits) = object.get("exits") {
                    serde_json::from_value(exits.clone()).context("bound resource exits")?
                } else if let Some(exit) = object.get("exit").and_then(Value::as_str) {
                    BTreeMap::from([("success".into(), exit.into())])
                } else {
                    BTreeMap::new()
                };
                out.push(Binding {
                    path: path.into(),
                    entry: entry.into(),
                    exits,
                    notifications: notifications.clone(),
                });
            }
            let empty = HashSet::new();
            for (key, child) in object {
                visit(
                    child,
                    if key == "child" {
                        notifications
                    } else {
                        &empty
                    },
                    out,
                )?;
            }
        } else if let Some(array) = value.as_array() {
            for child in array {
                visit(child, &HashSet::new(), out)?;
            }
        }
        Ok(())
    }
    let mut result = Vec::new();
    for stage in &manifest.stages {
        let notifications = stage
            .fields
            .get("notifications")
            .and_then(Value::as_object)
            .map(|v| v.keys().cloned().collect())
            .unwrap_or_default();
        visit(
            &serde_json::to_value(&stage.fields)?,
            &notifications,
            &mut result,
        )?;
    }
    Ok(result)
}

/// Checks declared call semantics once generated and published resources are decoded.
pub fn declared_interfaces(
    bindings: &[Binding],
    documents: &crate::compiler::Artifacts,
) -> Result<()> {
    for binding in bindings {
        if let Some((_, target)) = documents
            .iter()
            .find(|(path, _)| path.eq_ignore_ascii_case(&binding.path))
        {
            interface(
                target,
                std::slice::from_ref(&binding.entry),
                &binding.exits,
                &binding.notifications,
            )
            .with_context(|| format!("declared interface {}", binding.path))?;
        }
    }
    Ok(())
}
