//! Native graph construction with deterministic handles and cyclic connections.

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Stable node identity within a graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(usize);

#[derive(Debug)]
struct Node {
    handle: usize,
    inputs: BTreeMap<String, usize>,
    outputs: BTreeMap<String, usize>,
    cut: usize,
}

/// A graph arena. Edges refer to handles until final first-use serialization.
#[derive(Debug)]
pub struct Graph {
    definitions: Vec<Value>,
    nodes: Vec<Node>,
    ids: HashSet<u32>,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            definitions: vec![json!({"$type":"questGraphDefinition", "nodes":[]})],
            nodes: Vec::new(),
            ids: HashSet::new(),
        }
    }
}

impl Graph {
    /// Allocates a native handle and returns a reference to it.
    pub fn wrap(&mut self, data: Value) -> Value {
        let id = self.definitions.len();
        self.definitions.push(data);
        reference(id)
    }

    fn socket(&mut self, name: &str, kind: &str) -> usize {
        let id = self.definitions.len();
        self.definitions.push(json!({"$type":"questSocketDefinition", "connections":[], "name": crate::nodes::cname(name), "type":kind}));
        id
    }

    /// Adds a native node. Duplicate numeric identities are rejected.
    pub fn node(
        &mut self,
        id: u32,
        class: &str,
        inputs: &[&str],
        outputs: &[&str],
        properties: Value,
    ) -> Result<NodeId> {
        ensure!(self.ids.insert(id), "duplicate native node id {id}");
        ensure!(properties.is_object(), "node properties must be an object");
        ensure!(
            inputs.iter().collect::<HashSet<_>>().len() == inputs.len(),
            "duplicate input socket"
        );
        ensure!(
            outputs.iter().collect::<HashSet<_>>().len() == outputs.len(),
            "duplicate output socket"
        );
        let handle = self.definitions.len();
        self.definitions.push(Value::Null);
        let cut = self.socket("CutDestination", "CutDestination");
        let mut sockets = vec![reference(cut)];
        let mut input_map = BTreeMap::new();
        let mut output_map = BTreeMap::new();
        for name in inputs {
            let socket = self.socket(name, "Input");
            input_map.insert((*name).to_owned(), socket);
            sockets.push(reference(socket));
        }
        for name in outputs {
            let socket = self.socket(name, "Output");
            output_map.insert((*name).to_owned(), socket);
            sockets.push(reference(socket));
        }
        let mut data = json!({"$type":class,"id":id,"sockets":sockets});
        data.as_object_mut()
            .context("node object")?
            .extend(properties.as_object().context("properties object")?.clone());
        self.definitions[handle] = data;
        self.definitions[0]["nodes"]
            .as_array_mut()
            .context("graph nodes")?
            .push(reference(handle));
        let index = NodeId(self.nodes.len());
        self.nodes.push(Node {
            handle,
            inputs: input_map,
            outputs: output_map,
            cut,
        });
        Ok(index)
    }

    /// Changes a property on an existing native node.
    pub fn set(&mut self, node: NodeId, key: &str, value: Value) -> Result<()> {
        let handle = self.nodes.get(node.0).context("unknown node")?.handle;
        self.definitions[handle][key] = value;
        Ok(())
    }

    /// Changes an unwired predicate to an immediate True/False snapshot.
    pub fn snapshot(&mut self, node: NodeId) -> Result<()> {
        let source = self.nodes.get(node.0).context("unknown predicate node")?;
        for output in source.outputs.values() {
            ensure!(
                self.definitions[*output]["connections"]
                    .as_array()
                    .is_some_and(Vec::is_empty),
                "cannot snapshot a wired predicate"
            );
        }
        let handle = source.handle;
        let old = source.outputs.values().copied().collect::<HashSet<_>>();
        self.definitions[handle]["$type"] = json!("questConditionNodeDefinition");
        self.definitions[handle]["sockets"]
            .as_array_mut()
            .context("predicate sockets")?
            .retain(|v| {
                !v.get("HandleRefId")
                    .and_then(Value::as_str)
                    .and_then(|v| v.parse::<usize>().ok())
                    .is_some_and(|v| old.contains(&v))
            });
        self.nodes[node.0].outputs.clear();
        for name in ["True", "False"] {
            let socket = self.socket(name, "Output");
            self.nodes[node.0].outputs.insert(name.to_owned(), socket);
            self.definitions[handle]["sockets"]
                .as_array_mut()
                .context("predicate sockets")?
                .push(reference(socket));
        }
        Ok(())
    }

    /// Updates a nested native property, following arena handles on the path.
    pub fn set_nested(&mut self, node: NodeId, keys: &[&str], value: Value) -> Result<()> {
        ensure!(!keys.is_empty(), "nested property path is empty");
        let mut handle = self.nodes.get(node.0).context("unknown node")?.handle;
        let mut pointer = String::new();
        for key in &keys[..keys.len() - 1] {
            pointer.push('/');
            pointer.push_str(&key.replace('~', "~0").replace('/', "~1"));
            let field = self.definitions[handle]
                .pointer(&pointer)
                .context("missing nested property")?;
            if let Some(id) = field.get("HandleRefId").and_then(Value::as_str) {
                handle = id.parse().context("invalid property handle")?;
                ensure!(handle < self.definitions.len(), "undefined property handle");
                pointer.clear();
            }
        }
        pointer.push('/');
        pointer.push_str(&keys[keys.len() - 1].replace('~', "~0").replace('/', "~1"));
        *self.definitions[handle]
            .pointer_mut(&pointer)
            .context("missing nested property target")? = value;
        Ok(())
    }

    /// Connects named output/input sockets; loops and shared inputs are supported.
    pub fn connect(
        &mut self,
        source: NodeId,
        output: &str,
        destination: NodeId,
        input: &str,
    ) -> Result<()> {
        let from = *self
            .nodes
            .get(source.0)
            .context("unknown source node")?
            .outputs
            .get(output)
            .with_context(|| format!("missing output {output}"))?;
        let node = self
            .nodes
            .get(destination.0)
            .context("unknown destination node")?;
        let to = if input == "CutDestination" {
            node.cut
        } else {
            *node
                .inputs
                .get(input)
                .with_context(|| format!("missing input {input}"))?
        };
        let connection = self.wrap(json!({"$type":"graphGraphConnectionDefinition", "destination":reference(to), "source":reference(from)}));
        self.definitions[from]["connections"]
            .as_array_mut()
            .context("output connections")?
            .push(connection.clone());
        self.definitions[to]["connections"]
            .as_array_mut()
            .context("input connections")?
            .push(connection);
        Ok(())
    }

    /// Chains the ordinary Out socket to the ordinary In socket.
    pub fn then(&mut self, source: NodeId, destination: NodeId) -> Result<()> {
        self.connect(source, "Out", destination, "In")
    }

    /// Returns a complete phase document with definitions before references.
    pub fn document(&self, depot: &str, prefabs: &[String]) -> Result<Value> {
        let graph = expand(&reference(0), &self.definitions, &mut HashSet::new())?;
        Ok(json!({
            "Header":{"WolvenKitVersion":"8.17.4","WKitJsonVersion":"0.0.9","GameVersion":2310,"ExportedDateTime":"1970-01-01T00:00:00Z","DataType":"CR2W","ArchiveFileName":depot},
            "Data":{"Version":195,"BuildVersion":0,"RootChunk":{"$type":"questQuestPhaseResource","cookingPlatform":"PLATFORM_PC","graph":graph,"inplacePhases":[],"phasePrefabs":prefabs.iter().map(|p|json!({"$type":"questQuestPrefabEntry","prefabNodeRef":crate::nodes::node_ref(p)})).collect::<Vec<_>>()},"EmbeddedFiles":[]}
        }))
    }
}

fn reference(id: usize) -> Value {
    json!({"HandleRefId":id.to_string()})
}

fn expand(value: &Value, definitions: &[Value], seen: &mut HashSet<usize>) -> Result<Value> {
    if let Some(id) = value.get("HandleRefId").and_then(Value::as_str) {
        let id: usize = id.parse().context("invalid handle identity")?;
        let data = definitions.get(id).context("undefined graph handle")?;
        if !seen.insert(id) {
            return Ok(reference(id));
        }
        return Ok(json!({"HandleId":id.to_string(),"Data":expand(data,definitions,seen)?}));
    }
    match value {
        Value::Array(values) => values
            .iter()
            .map(|v| expand(v, definitions, seen))
            .collect(),
        Value::Object(values) => values
            .iter()
            .map(|(k, v)| Ok((k.clone(), expand(v, definitions, seen)?)))
            .collect(),
        _ => Ok(value.clone()),
    }
}

/// Validates and orders arbitrary document handles without changing graph wiring.
pub fn order_handles(document: &Value) -> Result<Value> {
    fn collect(value: &Value, definitions: &mut HashMap<String, Value>) -> Result<()> {
        match value {
            Value::Object(values) => {
                if let Some(id) = values.get("HandleId").and_then(Value::as_str) {
                    ensure!(
                        definitions.insert(id.to_owned(), value.clone()).is_none(),
                        "duplicate handle definition {id}"
                    );
                }
                for child in values.values() {
                    collect(child, definitions)?;
                }
            }
            Value::Array(values) => {
                for child in values {
                    collect(child, definitions)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn visit(
        value: &Value,
        definitions: &HashMap<String, Value>,
        seen: &mut HashSet<String>,
    ) -> Result<Value> {
        if let Some(id) = value
            .get("HandleId")
            .or_else(|| value.get("HandleRefId"))
            .and_then(Value::as_str)
        {
            if id == "-1" {
                return Ok(value.clone());
            }
            let definition = definitions
                .get(id)
                .with_context(|| format!("undefined handle {id}"))?;
            if !seen.insert(id.to_owned()) {
                return Ok(json!({"HandleRefId":id}));
            }
            let object = definition.as_object().context("handle definition")?;
            return object
                .iter()
                .map(|(k, v)| Ok((k.clone(), visit(v, definitions, seen)?)))
                .collect();
        }
        match value {
            Value::Object(values) => values
                .iter()
                .map(|(k, v)| Ok((k.clone(), visit(v, definitions, seen)?)))
                .collect(),
            Value::Array(values) => values.iter().map(|v| visit(v, definitions, seen)).collect(),
            _ => Ok(value.clone()),
        }
    }
    let mut definitions = HashMap::new();
    collect(document, &mut definitions)?;
    ensure!(
        !definitions.is_empty(),
        "resource contains no graph handles"
    );
    visit(document, &definitions, &mut HashSet::new())
}
