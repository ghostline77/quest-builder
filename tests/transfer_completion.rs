use ghostline_quest_builder::lifecycle;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

// Follow the emitted native sockets, including CutDestination, rather than
// assuming that terminating a child immediately unregisters its listeners.
struct Transfer {
    definitions: HashMap<String, Value>,
    owners: HashMap<String, (String, String)>,
    cut: HashSet<String>,
    failed: bool,
    retried: bool,
}

impl Transfer {
    fn new(with_scene: bool) -> Self {
        let mut spec = json!({"kind":"personal_link_attempt","depot":"mod\\test\\attempt.questphase","device":"#terminal","label":"Transfer","duration_seconds":5});
        if with_scene {
            spec["connected_scene"] = json!({"scene":"mod\\test\\dialogue.scene","origin":"#origin","entry":"start","exit":"end"});
        }
        let mut result = Self {
            definitions: HashMap::new(),
            owners: HashMap::new(),
            cut: HashSet::new(),
            failed: false,
            retried: false,
        };
        result.collect(&lifecycle::compile(&spec).unwrap());
        for (id, node) in &result.definitions {
            if let Some(sockets) = node["sockets"].as_array() {
                for socket in sockets {
                    let socket_id = Self::id(socket);
                    let name = result.definitions[&socket_id]["name"]["$value"]
                        .as_str()
                        .unwrap();
                    result.owners.insert(socket_id, (id.clone(), name.into()));
                }
            }
        }
        result
    }

    fn id(value: &Value) -> String {
        value
            .get("HandleId")
            .or_else(|| value.get("HandleRefId"))
            .unwrap()
            .as_str()
            .unwrap()
            .into()
    }

    fn collect(&mut self, value: &Value) {
        if let Some(id) = value["HandleId"].as_str() {
            self.definitions.insert(id.into(), value["Data"].clone());
        }
        match value {
            Value::Object(values) => values.values().for_each(|v| self.collect(v)),
            Value::Array(values) => values.iter().for_each(|v| self.collect(v)),
            _ => {}
        }
    }

    fn event(&mut self, predicate: &str) {
        let node = self
            .definitions
            .iter()
            .find(|(_, node)| {
                node["$type"] == "questPauseConditionNodeDefinition"
                    && node.to_string().contains(predicate)
            })
            .unwrap()
            .0
            .clone();
        self.emit(&node, "Out");
    }

    fn emit(&mut self, node: &str, port: &str) {
        if self.cut.contains(node) {
            return;
        }
        let destinations = self
            .owners
            .iter()
            .filter(|(_, owner)| owner.0 == node && owner.1 == port)
            .flat_map(|(socket, _)| self.definitions[socket]["connections"].as_array().unwrap())
            .map(|connection| {
                let connection = &self.definitions[&Self::id(connection)];
                self.owners[&Self::id(&connection["destination"])].clone()
            })
            .collect::<Vec<_>>();
        for (target, input) in destinations {
            if input == "CutDestination" {
                self.cut.insert(target);
                continue;
            }
            if self.cut.contains(&target) {
                continue;
            }
            match self.definitions[&target]["$type"].as_str().unwrap() {
                "questCutControlNodeDefinition" => {
                    self.emit(&target, "CutSource");
                    self.emit(&target, "Out");
                }
                "questUIManagerNodeDefinition" => {
                    self.failed = true;
                    self.emit(&target, "Out");
                }
                "questOutputNodeDefinition" => {
                    self.retried = true;
                }
                _ => {}
            }
        }
    }
}

#[test]
fn completed_transfer_cannot_fail_on_late_disconnect() {
    for with_scene in [false, true] {
        let mut transfer = Transfer::new(with_scene);
        transfer.event("questRealtimeDelay_ConditionType");
        transfer.event("IsPersonalLinkDisconnected");
        assert!(
            !transfer.failed,
            "completed HUD must not play the failed outro"
        );
        assert!(!transfer.retried, "completed transfer must not retry");
    }
}

#[test]
fn disconnect_before_completion_still_fails_and_retries() {
    for with_scene in [false, true] {
        let mut transfer = Transfer::new(with_scene);
        transfer.event("IsPersonalLinkDisconnected");
        assert!(transfer.failed);
        assert!(transfer.retried);
        transfer.event("questRealtimeDelay_ConditionType");
        assert_eq!(transfer.cut.len(), if with_scene { 3 } else { 2 });
    }
}
