use crate::{
    graph::{Graph, NodeId},
    manifest::Stage,
    nodes as n,
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

pub(super) mod actor_lifecycle;
pub(super) mod bindings;
pub(super) mod conditions;
pub(in crate::compiler) mod content;
pub(in crate::compiler) mod meeting_activity;
pub(super) mod owned;
pub(in crate::compiler) mod personal_link;

#[derive(Clone)]
pub(super) struct Signal {
    pub(super) node: NodeId,
    pub(super) port: String,
}
impl Signal {
    pub(super) fn out(node: NodeId) -> Self {
        Self {
            node,
            port: "Out".into(),
        }
    }
    pub(super) fn port(&self, name: &str) -> Self {
        Self {
            node: self.node,
            port: name.into(),
        }
    }
}

pub(super) struct Block<'a> {
    pub(super) stage: &'a Stage,
    pub(super) g: Graph,
    pub(super) next: u32,
    pub(super) start: Signal,
    pub(super) outputs: BTreeMap<String, NodeId>,
    pub(super) used: HashSet<String>,
    pub(super) created: Vec<NodeId>,
}
impl<'a> Block<'a> {
    pub(super) fn new(stage: &'a Stage) -> Result<Self> {
        let inputs = stage.inputs()?;
        ensure!(
            inputs.len() == 1,
            "stage {} needs exactly one start input",
            stage.id
        );
        let mut g = Graph::default();
        let start = Signal::out(n::input(
            &mut g,
            inputs.get("start").context("stage start input")?,
        )?);
        let mut outputs = BTreeMap::new();
        for (i, (alias, name)) in stage.outcomes()?.into_iter().enumerate() {
            outputs.insert(alias, n::output(&mut g, u32::try_from(i)? + 1, &name)?);
        }
        let next = 10.max(u32::try_from(outputs.len())? + 1);
        Ok(Self {
            stage,
            g,
            next,
            start,
            outputs,
            used: HashSet::new(),
            created: Vec::new(),
        })
    }
    pub(super) fn node(
        &mut self,
        f: impl FnOnce(&mut Graph, u32) -> Result<NodeId>,
    ) -> Result<Signal> {
        let id = self.next;
        self.next += 1;
        let node = f(&mut self.g, id)?;
        self.created.push(node);
        Ok(Signal::out(node))
    }
    pub(super) fn append(
        &mut self,
        source: &Signal,
        target: Signal,
        input: &str,
    ) -> Result<Signal> {
        self.g
            .connect(source.node, &source.port, target.node, input)?;
        Ok(target)
    }
    pub(super) fn connect(&mut self, source: &Signal, target: &Signal, input: &str) -> Result<()> {
        self.g
            .connect(source.node, &source.port, target.node, input)
    }
    pub(super) fn fact(&mut self, source: Signal, name: Option<&str>) -> Result<Signal> {
        if let Some(name) = name {
            let target = self.node(|g, id| n::fact(g, id, name, 1))?;
            self.append(&source, target, "In")
        } else {
            Ok(source)
        }
    }
    pub(super) fn objective(&mut self, source: Signal, event: &str) -> Result<Signal> {
        let Some(path) = self.stage.optional("objective") else {
            return Ok(source);
        };
        let default = match event {
            "enter" => "activate",
            "success" => "succeed",
            "failure" => "fail",
            _ => "deactivate",
        };
        let key = format!("on_{event}");
        let action = self
            .stage
            .fields
            .get("objective_lifecycle")
            .and_then(|v| v.get(&key))
            .and_then(Value::as_str)
            .unwrap_or(default);
        if action == "retain" {
            return Ok(source);
        }
        let input = match action {
            "activate" => "Active",
            "succeed" => "Succeeded",
            "fail" => "Failed",
            _ => "Inactive",
        };
        let node = self.node(|g, id| n::objective(g, id, path, false))?;
        self.append(&source, node, input)
    }
    pub(super) fn enter(&mut self) -> Result<Signal> {
        let mut source = self.objective(self.start.clone(), "enter")?;
        if let Some(path) = self.stage.optional("description_entry") {
            let target =
                self.node(|g, id| n::journal(g, id, path, "gameJournalQuestDescription", 2))?;
            source = self.append(&source, target, "Active")?;
        }
        Ok(source)
    }
    pub(super) fn pin(&mut self, source: Signal, state: &str, disable: bool) -> Result<Signal> {
        if let Some(path) = self.stage.optional("mappin") {
            let node = self.node(|g, id| n::mappin(g, id, path, disable))?;
            self.append(&source, node, state)
        } else {
            Ok(source)
        }
    }
    pub(super) fn message(&mut self, source: Signal, path: &str) -> Result<Signal> {
        let target = self.node(|g, id| n::journal(g, id, path, "gameJournalPhoneMessage", 1))?;
        self.append(&source, target, "Active")
    }
    pub(super) fn finish(&mut self, source: Signal, outcome: &str) -> Result<()> {
        let end = *self
            .outputs
            .get(outcome)
            .with_context(|| format!("stage {} has no outcome {outcome}", self.stage.id))?;
        self.used.insert(outcome.into());
        self.g.connect(source.node, &source.port, end, "In")
    }
    pub(super) fn join(&mut self, signals: &[Signal], all: bool) -> Result<Signal> {
        ensure!(!signals.is_empty(), "cannot join zero signals");
        let target = self
            .node(|g, id| n::join(g, id, signals.len(), all))?
            .port("Out1");
        for (i, signal) in signals.iter().enumerate() {
            self.connect(signal, &target, &format!("In{}", i + 1))?;
        }
        Ok(target)
    }
    pub(super) fn document(self) -> Result<Value> {
        for name in self.outputs.keys() {
            ensure!(
                self.used.contains(name),
                "stage {} never emits declared outcome {name}",
                self.stage.id
            );
        }
        self.g.document(&self.stage.phase_resource, &[])
    }

    pub(super) fn race(
        &mut self,
        routes: BTreeMap<String, Signal>,
        owned: &[NodeId],
    ) -> Result<BTreeMap<String, Signal>> {
        let mut gates = BTreeMap::new();
        for (name, signal) in routes {
            gates.insert(name, self.join(&[signal], false)?);
        }
        let mut cuts = BTreeMap::new();
        for (name, signal) in &gates {
            let cut = self.node(|g, id| {
                g.node(
                    id,
                    "questCutControlNodeDefinition",
                    &["In"],
                    &["Out", "CutSource"],
                    serde_json::json!({"permanent":1}),
                )
            })?;
            cuts.insert(name.clone(), self.append(signal, cut, "In")?);
        }
        for (name, cut) in &cuts {
            let mut seen = HashSet::new();
            let targets = owned
                .iter()
                .copied()
                .chain(
                    gates
                        .iter()
                        .filter(|(key, _)| *key != name)
                        .map(|(_, v)| v.node),
                )
                .chain(
                    cuts.iter()
                        .filter(|(key, _)| *key != name)
                        .map(|(_, v)| v.node),
                )
                .filter(|target| *target != cut.node && seen.insert(*target))
                .collect::<Vec<_>>();
            for target in targets {
                self.g
                    .connect(cut.node, "CutSource", target, "CutDestination")?;
            }
        }
        Ok(cuts)
    }
}

pub(super) fn threshold(b: &mut Block<'_>, signals: &[Signal], required: usize) -> Result<Signal> {
    ensure!(
        required > 0 && required <= signals.len(),
        "required_count must be between one and clue count"
    );
    let mut levels = BTreeMap::<usize, Signal>::new();
    for (index, signal) in signals.iter().enumerate() {
        let mut next = BTreeMap::new();
        for count in 1..=required.min(index + 1) {
            let acquired = if count == 1 {
                signal.clone()
            } else {
                b.join(&[levels[&(count - 1)].clone(), signal.clone()], true)?
            };
            let acquired = if let Some(previous) = levels.get(&count) {
                b.join(&[previous.clone(), acquired], false)?
            } else {
                acquired
            };
            next.insert(count, acquired);
        }
        levels = next;
    }
    levels.remove(&required).context("distinct clue threshold")
}

pub(super) fn fractional_delay(b: &mut Block<'_>, value: f64) -> Result<Signal> {
    ensure!(
        value.is_finite() && (0. ..4_294_967.).contains(&value),
        "invalid real-time delay"
    );
    let millis = (value * 1000.).round_ties_even() as u64;
    b.node(|g, id| n::real_delay(g, id, millis / 1000, millis % 1000))
}
