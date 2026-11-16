//! Sequential signal-routing scenarios and journal-validated snapshots.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Event {
    event_id: String,
    stage: String,
    outcome: String,
}
struct Model {
    contract: Value,
    fingerprint: String,
    active: Option<String>,
    terminal: Option<String>,
    events: Vec<Event>,
}

/// Preview covers sequential and parallel authored routes; it makes no runtime claim.
pub fn graph(plan: &Value) -> Result<String> {
    let stages = plan["stages"].as_array().context("graph plan stages")?;
    let routes = plan["routes"].as_object().context("graph plan routes")?;
    let mut out = format!(
        "# {}\n\n```mermaid\nflowchart TD\n  entry((Entry))\n  finish((Complete))\n",
        plan["title"].as_str().unwrap_or("Quest")
    );
    let ids = stages
        .iter()
        .enumerate()
        .map(|(i, s)| Ok((s["id"].as_str().context("graph stage ID")?, format!("s{i}"))))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let escape = |text: &str| {
        text.replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace(['\n', '\r'], " ")
    };
    for stage in stages {
        let id = stage["id"].as_str().context("graph stage ID")?;
        out.push_str(&format!(
            "  {}[\"{}: {}\"]\n",
            ids[id],
            escape(id),
            escape(stage["type"].as_str().unwrap_or_default())
        ));
    }
    if let Some(groups) = plan["parallel_groups"].as_array().filter(|v| !v.is_empty()) {
        let positions = stages
            .iter()
            .enumerate()
            .map(|(index, stage)| {
                Ok((stage["id"].as_str().context("graph stage identity")?, index))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let mut starts = BTreeMap::new();
        for (number, group) in groups.iter().enumerate() {
            let branches = group["branches"].as_array().context("parallel branches")?;
            let mut span = Vec::new();
            for branch in branches {
                for member in branch.as_array().context("parallel branch")? {
                    span.push(
                        *positions
                            .get(member.as_str().context("parallel member")?)
                            .context("unknown parallel member")?,
                    );
                }
            }
            let start = *span.iter().min().context("empty parallel group")?;
            let end = *span.iter().max().context("empty parallel group")?;
            ensure!(
                starts.insert(start, (number, end, group)).is_none(),
                "overlapping parallel groups"
            );
        }
        let mut index = 0;
        let mut tail = String::from("entry");
        while index < stages.len() {
            if let Some((number, end, group)) = starts.get(&index) {
                let fork = format!("fork{number}");
                let join = format!("join{number}");
                out.push_str(&format!("  {fork}[\"Parallel: {}\"]\n  {join}[\"All branches complete\"]\n  {tail} --> {fork}\n",escape(group["id"].as_str().context("parallel group ID")?)));
                for branch in group["branches"].as_array().context("parallel branches")? {
                    let mut previous = fork.clone();
                    for member in branch.as_array().context("parallel branch")? {
                        let node = ids
                            .get(member.as_str().context("parallel member")?)
                            .context("unknown parallel member")?;
                        out.push_str(&format!("  {previous} --> {node}\n"));
                        previous = node.clone();
                    }
                    ensure!(previous != fork, "empty parallel branch");
                    out.push_str(&format!("  {previous} --> {join}\n"));
                }
                tail = join;
                index = end + 1;
            } else {
                let node = ids
                    .get(
                        stages[index]["id"]
                            .as_str()
                            .context("graph stage identity")?,
                    )
                    .context("unknown graph stage")?;
                out.push_str(&format!("  {tail} --> {node}\n"));
                tail = node.clone();
                index += 1;
            }
        }
        out.push_str(&format!("  {tail} --> finish\n"));
    } else {
        let entry = plan["entry_stage"].as_str().context("graph entry")?;
        out.push_str(&format!(
            "  entry --> {}\n",
            ids.get(entry).context("unknown entry stage")?
        ));
        for (from, exits) in routes {
            for (outcome, to) in exits.as_object().context("stage routes")? {
                let target = to.as_str().context("route destination")?;
                let target = if target == "$end" {
                    "finish"
                } else {
                    ids.get(target).context("unknown graph route")?
                };
                out.push_str(&format!(
                    "  {} -->|\"{}\"| {target}\n",
                    ids.get(from.as_str()).context("unknown graph source")?,
                    escape(outcome)
                ));
            }
        }
    }
    out.push_str("```\n\n");
    if let Some(groups) = plan["parallel_groups"].as_array().filter(|v| !v.is_empty()) {
        out.push_str("Parallel all-of groups:\n\n");
        for group in groups {
            out.push_str(&format!(
                "- `{}`: `{}`\n",
                group["id"].as_str().unwrap_or_default(),
                group["branches"]
            ));
        }
    }
    Ok(out)
}
impl Model {
    fn new(plan: &Value) -> Result<Self> {
        ensure!(
            plan.get("parallel_groups")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty),
            "Sequential scenarios do not execute parallel groups"
        );
        ensure!(
            !plan["diagnostics"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|v| v["level"] == "error"),
            "plan has compiler errors"
        );
        let stages = plan["stages"].as_array().context("scenario plan stages")?;
        ensure!(!stages.is_empty(), "scenario plan is empty");
        let mut ids = BTreeSet::new();
        let mut ordered = Vec::new();
        let mut ports = BTreeMap::new();
        let transitions = plan["routes"]
            .as_object()
            .context("scenario transition map")?;
        for stage in stages {
            let id = stage["id"]
                .as_str()
                .filter(|v| !v.is_empty() && *v != "$end")
                .context("invalid scenario stage ID")?;
            ensure!(ids.insert(id.to_owned()), "duplicate stage ID");
            ordered.push(id);
            let outcomes = stage
                .get("outcomes")
                .or_else(|| stage.get("fields").and_then(|v| v.get("outcomes")));
            if let Some(outcomes) = outcomes {
                ensure!(
                    outcomes
                        .as_object()
                        .context("outcome map")?
                        .keys()
                        .collect::<BTreeSet<_>>()
                        == transitions[id]
                            .as_object()
                            .context("transition routes")?
                            .keys()
                            .collect::<BTreeSet<_>>(),
                    "routing and declared outcomes disagree"
                );
            }
            ports.insert(
                id,
                json!({"inputs":stage.get("inputs"),"outcomes":outcomes}),
            );
        }
        ensure!(
            transitions.keys().cloned().collect::<BTreeSet<_>>() == ids,
            "plan needs transitions for every stage"
        );
        for routes in transitions.values() {
            let routes = routes
                .as_object()
                .filter(|v| !v.is_empty())
                .context("empty scenario routes")?;
            for (outcome, target) in routes {
                ensure!(
                    !outcome.is_empty()
                        && target
                            .as_str()
                            .is_some_and(|v| v == "$end" || ids.contains(v)),
                    "invalid scenario route"
                );
            }
        }
        let entry = plan
            .get("entry_stage")
            .and_then(Value::as_str)
            .unwrap_or(ordered[0]);
        ensure!(ids.contains(entry), "unknown scenario entry");
        let contracts = stages
            .iter()
            .map(|stage| {
                (
                    stage["id"].as_str().unwrap_or_default().to_owned(),
                    stage
                        .get("contract")
                        .or_else(|| stage.get("fields").and_then(|v| v.get("contract")))
                        .cloned()
                        .unwrap_or(json!({})),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let contract = json!({"schema_version":1,"entry_stage":entry,"stages":ordered,"ports":ports,"routes":transitions,"contracts":contracts});
        let fingerprint = crate::evidence::canonical_hash(&contract)?;
        Ok(Self {
            contract,
            fingerprint,
            active: Some(entry.into()),
            terminal: None,
            events: Vec::new(),
        })
    }
    fn state(&self) -> Value {
        json!({"active_stage":self.active,"terminal_outcome":self.terminal})
    }
    fn signal(&mut self, event: &Value) -> Result<Value> {
        let event: Event = serde_json::from_value(event.clone())?;
        ensure!(
            !event.event_id.is_empty() && !event.stage.is_empty() && !event.outcome.is_empty(),
            "event fields must be nonempty"
        );
        let result =
            if let Some(previous) = self.events.iter().find(|v| v.event_id == event.event_id) {
                ensure!(*previous == event, "event ID reused with different content");
                "duplicate"
            } else {
                let target = self.contract["routes"][&event.stage][&event.outcome]
                    .as_str()
                    .context("unknown stage/outcome signal")?;
                let result = if self.active.is_none() {
                    "terminal"
                } else if self.active.as_deref() != Some(&event.stage) {
                    "inactive"
                } else {
                    if target == "$end" {
                        self.active = None;
                        self.terminal = Some(event.outcome.clone());
                    } else {
                        self.active = Some(target.into());
                    }
                    "advanced"
                };
                self.events.push(event);
                result
            };
        let mut state = self.state();
        state["result"] = json!(result);
        Ok(state)
    }
    fn snapshot(&self) -> Value {
        json!({"schema_version":1,"contract_sha256":self.fingerprint,"active_stage":self.active,"terminal_outcome":self.terminal,"processed_events":self.events})
    }
    fn restore(&mut self, snapshot: &Value) -> Result<()> {
        ensure!(
            snapshot["schema_version"] == 1 && snapshot["contract_sha256"] == self.fingerprint,
            "snapshot belongs to a different routing contract"
        );
        let events = snapshot["processed_events"]
            .as_array()
            .context("snapshot event journal")?;
        let mut replay = Self {
            contract: self.contract.clone(),
            fingerprint: self.fingerprint.clone(),
            active: Some(
                self.contract["entry_stage"]
                    .as_str()
                    .context("contract entry")?
                    .into(),
            ),
            terminal: None,
            events: Vec::new(),
        };
        let mut ids = BTreeSet::new();
        for event in events {
            ensure!(
                ids.insert(event["event_id"].as_str().context("event receipt ID")?),
                "duplicate event receipt in snapshot"
            );
            replay.signal(event)?;
        }
        ensure!(
            replay.state()
                == json!({"active_stage":snapshot["active_stage"],"terminal_outcome":snapshot["terminal_outcome"]}),
            "snapshot state disagrees with event journal"
        );
        self.active = replay.active;
        self.terminal = replay.terminal;
        self.events = replay.events;
        Ok(())
    }
}
fn expect(actual: &Value, expected: Option<&Value>) -> Result<()> {
    if let Some(expected) = expected {
        for (key, value) in expected
            .as_object()
            .context("expectation must be an object")?
        {
            ensure!(
                matches!(key.as_str(), "active_stage" | "terminal_outcome" | "result"),
                "unknown expectation field {key}"
            );
            ensure!(
                actual.get(key) == Some(value),
                "expected {key}={value}, got {}",
                actual.get(key).unwrap_or(&Value::Null)
            );
        }
    }
    Ok(())
}

/// Executes declared routing signals; it does not simulate engine conditions or saves.
pub fn run(plan: &Value, suite: &Value) -> Result<Value> {
    let contract = Model::new(plan)?.fingerprint;
    ensure!(suite["schema_version"] == 1, "unsupported scenario schema");
    let scenarios = suite["scenarios"]
        .as_array()
        .filter(|v| !v.is_empty())
        .context("scenario suite needs scenarios")?;
    let mut ids = BTreeSet::new();
    let mut reports = Vec::new();
    for scenario in scenarios {
        let id = scenario["id"]
            .as_str()
            .filter(|v| !v.is_empty())
            .context("scenario ID")?;
        ensure!(ids.insert(id), "duplicate scenario ID");
        let mut model = Model::new(plan)?;
        let mut checkpoints = BTreeMap::new();
        let mut trace = Vec::new();
        let result = (|| -> Result<()> {
            let steps = scenario["steps"]
                .as_array()
                .filter(|v| !v.is_empty())
                .context("scenario requires steps")?;
            for (index, step) in steps.iter().enumerate() {
                let object = step.as_object().context("scenario step")?;
                ensure!(
                    object
                        .keys()
                        .all(|k| matches!(k.as_str(), "event" | "save" | "load" | "expect"))
                        && ["event", "save", "load"]
                            .iter()
                            .filter(|k| object.contains_key(**k))
                            .count()
                            == 1,
                    "step needs one event/save/load operation"
                );
                let value = if let Some(event) = step.get("event") {
                    model.signal(event)?
                } else {
                    let save = object.contains_key("save");
                    let name = step[if save { "save" } else { "load" }]
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .context("checkpoint name")?;
                    if save {
                        ensure!(!checkpoints.contains_key(name), "checkpoint already exists");
                        checkpoints.insert(name.to_owned(), model.snapshot());
                    } else {
                        model
                            .restore(checkpoints.get(name).context("checkpoint does not exist")?)?;
                    }
                    let mut state = model.state();
                    state["result"] = json!(if save { "save" } else { "load" });
                    state
                };
                let mut entry = value.clone();
                entry["step"] = json!(index);
                trace.push(entry);
                expect(&value, step.get("expect"))?;
            }
            expect(&model.state(), scenario.get("expect"))
        })();
        reports.push(json!({"id":id,"passed":result.is_ok(),"error":result.err().map(|e|e.to_string()),"trace":trace,"final_state":model.state()}));
    }
    Ok(
        json!({"schema_version":1,"scope":"Structural sequential routing; no engine conditions, scenes, physics or game save execution","runtime_verified":false,"contract_sha256":contract,"passed":reports.iter().all(|v|v["passed"]==true),"scenarios":reports}),
    )
}
