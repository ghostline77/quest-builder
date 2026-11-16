//! Signal a generated quest from the existing global persistent quest host.
use super::Spec;
use crate::{graph::Graph, nodes as n};
use anyhow::{Context, Result, ensure};
use ghostline_red::{codec, material::ArchiveSet, schema::RedSchema};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

const DEPOT: &str = r"base\quest\persistent_content.questphase";

#[derive(Default)]
struct Arena {
    definitions: BTreeMap<String, Value>,
    next: u64,
}

fn handle(value: &Value) -> Result<String> {
    value
        .get("HandleId")
        .or_else(|| value.get("HandleRefId"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .context("native handle")
}
fn reference(id: &str) -> Value {
    json!({"HandleRefId":id})
}

impl Arena {
    fn import(&mut self, value: &Value, offset: u64) -> Result<Value> {
        if let Some(id) = value.get("HandleId").or_else(|| value.get("HandleRefId")) {
            let id = id.as_str().context("handle identity")?;
            if id == "-1" {
                return Ok(reference(id));
            }
            let number = id
                .parse::<u64>()?
                .checked_add(offset)
                .context("handle overflow")?;
            self.next = self
                .next
                .max(number.checked_add(1).context("handle overflow")?);
            let id = number.to_string();
            if let Some(data) = value.get("Data") {
                ensure!(
                    !self.definitions.contains_key(&id),
                    "duplicate native handle {id}"
                );
                self.definitions.insert(id.clone(), Value::Null);
                let data = self.import(data, offset)?;
                self.definitions.insert(id.clone(), data);
            }
            return Ok(reference(&id));
        }
        match value {
            Value::Object(fields) => fields
                .iter()
                .map(|(k, v)| Ok((k.clone(), self.import(v, offset)?)))
                .collect(),
            Value::Array(rows) => rows.iter().map(|v| self.import(v, offset)).collect(),
            _ => Ok(value.clone()),
        }
    }
    fn add(&mut self, data: Value) -> Value {
        let id = self.next.to_string();
        self.next += 1;
        self.definitions.insert(id.clone(), data);
        reference(&id)
    }
    fn data(&self, value: &Value) -> Result<&Value> {
        self.definitions
            .get(&handle(value)?)
            .context("undefined native handle")
    }
    fn graph_node(&self, graph: &Value, id: u64) -> Result<Value> {
        self.data(graph)?["nodes"]
            .as_array()
            .context("native graph nodes")?
            .iter()
            .find(|v| self.data(v).is_ok_and(|n| n["id"].as_u64() == Some(id)))
            .cloned()
            .with_context(|| format!("native host node {id}"))
    }
    fn socket(&self, node: &Value, name: &str) -> Result<Value> {
        self.data(node)?["sockets"]
            .as_array()
            .context("native sockets")?
            .iter()
            .find(|v| self.data(v).is_ok_and(|s| s["name"]["$value"] == name))
            .cloned()
            .with_context(|| format!("native socket {name}"))
    }
    fn edge(&mut self, source: &Value, destination: &Value) -> Result<()> {
        let connection=self.add(json!({"$type":"graphGraphConnectionDefinition","source":source,"destination":destination}));
        for socket in [source, destination] {
            let data = self
                .definitions
                .get_mut(&handle(socket)?)
                .context("edge socket")?;
            if data.get("connections").is_none() {
                data["connections"] = json!([]);
            }
            data["connections"]
                .as_array_mut()
                .context("connections")?
                .push(connection.clone());
        }
        Ok(())
    }
    fn condition(&self, value: &Value, graph: &mut Graph) -> Result<Value> {
        if value.get("HandleRefId").is_some() {
            let data = self.condition(self.data(value)?, graph)?;
            return Ok(graph.wrap(data));
        }
        match value {
            Value::Object(fields) => fields
                .iter()
                .map(|(k, v)| Ok((k.clone(), self.condition(v, graph)?)))
                .collect(),
            Value::Array(rows) => rows.iter().map(|v| self.condition(v, graph)).collect(),
            _ => Ok(value.clone()),
        }
    }
    fn expand(&self, value: &Value, seen: &mut HashSet<String>) -> Result<Value> {
        if value.get("HandleRefId").is_some() {
            let id = handle(value)?;
            if id == "-1" || !seen.insert(id.clone()) {
                return Ok(value.clone());
            }
            return Ok(json!({"HandleId":id,"Data":self.expand(self.data(value)?,seen)?}));
        }
        match value {
            Value::Object(fields) => fields
                .iter()
                .map(|(k, v)| Ok((k.clone(), self.expand(v, seen)?)))
                .collect(),
            Value::Array(rows) => rows.iter().map(|v| self.expand(v, seen)).collect(),
            _ => Ok(value.clone()),
        }
    }
}

pub(super) fn patch(document: &Value, spec: &Spec) -> Result<Value> {
    let mut arena = Arena::default();
    let document = arena.import(document, 0)?;
    for definition in arena.definitions.values_mut() {
        if definition["$type"] == "questSocketDefinition" && definition.get("connections").is_none()
        {
            definition["connections"] = json!([]);
        }
    }
    let mut scope = document["Data"]["RootChunk"]["graph"].clone();
    // This native branch is persistent for both normal and standalone starts.
    // Its original fact listener and all original node IDs remain in place.
    for id in [152, 3] {
        let phase = arena.graph_node(&scope, id)?;
        let data = arena.data(&phase)?;
        ensure!(
            data["$type"] == "questPhaseNodeDefinition",
            "native persistent host layout changed"
        );
        scope = data["phaseGraph"].clone();
    }
    let wait = arena.graph_node(&scope, 1)?;
    let original = arena.data(&wait)?["condition"].clone();
    ensure!(
        arena.data(&wait)?["$type"] == "questPauseConditionNodeDefinition",
        "native host listener changed"
    );
    let condition = arena.data(&original)?;
    ensure!(
        condition["$type"] == "questFactsDBCondition"
            && arena.data(&condition["type"])?["factName"] == "johnny_on",
        "native host predicate changed"
    );
    let wait_in = arena.socket(&wait, "In")?;
    let wait_out = arena.socket(&wait, "Out")?;
    let mut graph = Graph::default();
    let mut id = arena.data(&scope)?["nodes"]
        .as_array()
        .context("host nodes")?
        .iter()
        .map(|v| arena.data(v).map(|n| n["id"].as_u64().unwrap_or(0)))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .max()
        .unwrap_or(0)
        + 1;
    let mut next = || -> Result<u32> {
        let result = u32::try_from(id)?;
        id += 1;
        Ok(result)
    };
    let request = format!("{}_bootstrap_requested", spec.id);
    let dispatched = format!("{}_bootstrap_dispatched", spec.id);
    let entry_id = next()?;
    let entry = n::fact(
        &mut graph,
        entry_id,
        &format!("{}_bootstrap_host_wake", spec.id),
        1,
    )?;
    let check = n::fact_condition(&mut graph, next()?, &request, "Greater", 0, true)?;
    let clear = n::fact(&mut graph, next()?, &request, 0)?;
    let guard = n::fact_condition(&mut graph, next()?, &dispatched, "Equal", 0, true)?;
    let claim = n::fact(&mut graph, next()?, &dispatched, 1)?;
    let original_condition = arena.condition(&original, &mut graph)?;
    let resume = graph.node(
        next()?,
        "questConditionNodeDefinition",
        &["In"],
        &["True", "False"],
        json!({"condition":original_condition}),
    )?;
    let native_id = next()?;
    let native = n::fact(
        &mut graph,
        native_id,
        &format!("{}_bootstrap_native_resumed", spec.id),
        1,
    )?;
    let rearm_id = next()?;
    let rearm = n::fact(
        &mut graph,
        rearm_id,
        &format!("{}_bootstrap_rearmed", spec.id),
        1,
    )?;
    graph.then(entry, check)?;
    graph.connect(check, "True", clear, "In")?;
    graph.connect(check, "False", resume, "In")?;
    graph.then(clear, guard)?;
    graph.connect(guard, "True", claim, "In")?;
    graph.connect(guard, "False", resume, "In")?;
    graph.then(claim, resume)?;
    graph.connect(resume, "True", native, "In")?;
    graph.connect(resume, "False", rearm, "In")?;
    for phase in &spec.phases {
        let outputs = phase.output.iter().map(String::as_str).collect::<Vec<_>>();
        let call = n::phase(
            &mut graph,
            next()?,
            &phase.resource,
            &[&phase.input],
            &outputs,
        )?;
        graph.connect(claim, "Out", call, &phase.input)?;
        if let Some(output) = &phase.output {
            let completed = n::fact(
                &mut graph,
                next()?,
                &format!("{}_bootstrap_completed", spec.id),
                1,
            )?;
            graph.connect(call, output, completed, "In")?;
        }
    }
    let branch = graph.document("bootstrap-host", &[])?;
    let offset = arena.next;
    let branch = arena.import(&branch, offset)?;
    let branch_scope = branch["Data"]["RootChunk"]["graph"].clone();
    let nodes = arena.data(&branch_scope)?["nodes"]
        .as_array()
        .context("bridge nodes")?
        .clone();
    arena
        .definitions
        .get_mut(&handle(&scope)?)
        .context("host graph")?["nodes"]
        .as_array_mut()
        .context("host nodes")?
        .extend(nodes);
    let entry = arena.graph_node(&scope, u64::from(entry_id))?;
    let native = arena.graph_node(&scope, u64::from(native_id))?;
    let rearm = arena.graph_node(&scope, u64::from(rearm_id))?;
    let native_out = arena.socket(&native, "Out")?;
    let connections = arena.data(&wait_out)?["connections"]
        .as_array()
        .context("native listener edges")?
        .clone();
    for edge in &connections {
        arena
            .definitions
            .get_mut(&handle(edge)?)
            .context("native connection")?["source"] = native_out.clone();
    }
    arena
        .definitions
        .get_mut(&handle(&native_out)?)
        .context("bridge output")?["connections"] = json!(connections);
    arena
        .definitions
        .get_mut(&handle(&wait_out)?)
        .context("native output")?["connections"] = json!([]);
    arena.edge(&wait_out, &arena.socket(&entry, "In")?)?;
    arena.edge(&arena.socket(&rearm, "Out")?, &wait_in)?;
    let kind=arena.add(json!({"$type":"questVarComparison_ConditionType","factName":request,"comparisonType":"Greater","value":0}));
    let requested = arena.add(json!({"$type":"questFactsDBCondition","type":kind}));
    let either = arena.add(
        json!({"$type":"questLogicalCondition","operation":"OR","conditions":[original,requested]}),
    );
    arena
        .definitions
        .get_mut(&handle(&wait)?)
        .context("native listener")?["condition"] = either;
    let expanded = arena.expand(&document, &mut HashSet::new())?;
    let host_graph = arena.expand(&scope, &mut HashSet::new())?;
    crate::audit::Phase::read(&json!({"Data":{"RootChunk":{"graph":host_graph}}}))?;
    crate::audit::Phase::read(&expanded)?;
    Ok(expanded)
}

pub(super) fn build(
    archives: &ArchiveSet,
    spec: &Spec,
    schema: &RedSchema,
) -> Result<(String, Vec<u8>)> {
    let original = archives.read_resource(DEPOT)?;
    let document = codec::decode_wkit_from_bytes(&original, schema)?;
    let patched = patch(&document, spec)?;
    let binaries = crate::native::encode(&BTreeMap::from([(DEPOT.to_owned(), patched)]), schema)?;
    Ok((DEPOT.to_owned(), binaries[DEPOT].clone()))
}
