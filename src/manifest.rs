//! Parsed manifests, explicit routing and cross-stage dataflow checks.
mod ownership;
mod semantics;
use crate::{
    composition,
    contracts::{self, Diagnostic},
    project,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};

/// A normalized stage retaining its complete canonical field set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub phase_resource: String,
    #[serde(default = "ready")]
    pub status: String,
    #[serde(flatten)]
    pub fields: Map<String, Value>,
}
fn ready() -> String {
    "ready".into()
}
impl Stage {
    /// Reads a required string after structural validation.
    pub fn string(&self, key: &str) -> Result<&str> {
        self.fields
            .get(key)
            .and_then(Value::as_str)
            .with_context(|| format!("stage {} needs string {key}", self.id))
    }
    /// Reads an optional string.
    pub fn optional(&self, key: &str) -> Option<&str> {
        self.fields.get(key).and_then(Value::as_str)
    }
    /// Returns explicit input aliases or the ordinary native entry.
    pub fn inputs(&self) -> Result<BTreeMap<String, String>> {
        self.ports("inputs", "start", "In1")
    }
    /// Returns explicit outcome aliases or the ordinary success exit.
    pub fn outcomes(&self) -> Result<BTreeMap<String, String>> {
        self.ports("outcomes", "success", "Out1")
    }
    fn ports(&self, key: &str, alias: &str, socket: &str) -> Result<BTreeMap<String, String>> {
        self.fields.get(key).map_or_else(
            || Ok(BTreeMap::from([(alias.into(), socket.into())])),
            |v| serde_json::from_value(v.clone()).context("invalid port mapping"),
        )
    }
    /// Returns true only for an explicitly true native authoring flag.
    pub fn flag(&self, key: &str) -> bool {
        self.fields.get(key).and_then(Value::as_bool) == Some(true)
    }
    /// Returns an integer duration/quantity or a supplied default.
    pub fn number(&self, key: &str, default: u64) -> u64 {
        self.fields
            .get(key)
            .and_then(Value::as_u64)
            .unwrap_or(default)
    }
}

/// A parallel all-of group with explicit ordered stage branches.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParallelGroup {
    pub id: String,
    pub branches: Vec<Vec<String>>,
}

/// A concurrent branch whose completion is joined before the root retires.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fork {
    pub stage: String,
    pub outcome: String,
    pub start: String,
    pub complete: String,
}

/// A normalized quest with independently owned resource identities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub phase_prefabs: Vec<String>,
    #[serde(default)]
    pub parallel_groups: Vec<ParallelGroup>,
    #[serde(default)]
    pub forks: Vec<Fork>,
    #[serde(default)]
    pub debug_fact: Option<String>,
    pub stages: Vec<Stage>,
    #[serde(default)]
    pub entry_stage: Option<String>,
    #[serde(default)]
    pub external_facts: Vec<String>,
    #[serde(default)]
    pub external_events: Vec<String>,
    #[serde(default)]
    pub completion: Option<Value>,
    /// Generated resources and published inputs owned by this quest authoring file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_condition: Option<Value>,
    /// Optional shared progression gate; also applies before phone offers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_gate: Option<StartGate>,
}

/// Shared game-progression prerequisites, evaluated before quest activation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum StartGate {
    #[serde(rename = "after_act_1")]
    AfterAct1,
}

/// Expanded manifest and diagnostics, including source authoring bindings.
#[derive(Debug)]
pub struct Checked {
    pub composition: composition::Composition,
    pub manifest: Option<Manifest>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Checks an identifier without allocating or invoking an external tool.
pub fn identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Expands composition and checks the complete structural schema and flow model.
pub fn check(raw: &Value) -> Result<Checked> {
    let composition = composition::prepare(raw)?;
    let mut diagnostics = contracts::validate(&composition.manifest)?;
    if diagnostics.iter().any(|d| d.level == "error") {
        return Ok(Checked {
            composition,
            manifest: None,
            diagnostics,
        });
    }
    let manifest: Manifest = serde_json::from_value(composition.manifest.clone())
        .context("normalized quest manifest")?;
    diagnostics.extend(manifest.validate()?);
    let valid = !diagnostics.iter().any(|d| d.level == "error");
    Ok(Checked {
        composition,
        manifest: valid.then_some(manifest),
        diagnostics,
    })
}

impl Manifest {
    /// Calculates explicit or sequential routing for every stage.
    pub fn routes(&self) -> Result<BTreeMap<String, BTreeMap<String, String>>> {
        self.stages
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let following = self.stages.get(i + 1).map_or("$end", |s| s.id.as_str());
                let routes = s.fields.get("on").map_or_else(
                    || Ok(BTreeMap::from([("success".into(), following.to_owned())])),
                    |v| serde_json::from_value(v.clone()).context("invalid stage routes"),
                )?;
                Ok((s.id.clone(), routes))
            })
            .collect()
    }
    /// Validates ownership, declared routing, reachability and fact availability.
    pub fn validate(&self) -> Result<Vec<Diagnostic>> {
        let mut errors = Vec::new();
        let mut ids = HashSet::new();
        let mut resources = HashSet::new();
        semantics::validate(self, &mut errors)?;
        for (i, s) in self.stages.iter().enumerate() {
            let path = format!("/stages/{i}");
            if !ids.insert(s.id.as_str()) {
                errors.push(Diagnostic::error(
                    "duplicate_stage",
                    &path,
                    format!("duplicate stage {}", s.id),
                ));
            }
            if let Err(e) = project::depot_path(&s.phase_resource) {
                errors.push(Diagnostic::error(
                    "depot_path",
                    format!("{path}/phase_resource"),
                    e.to_string(),
                ));
            }
            if !resources.insert(&s.phase_resource) {
                errors.push(Diagnostic::error(
                    "duplicate_resource",
                    &path,
                    "stages share an output resource",
                ));
            }
            if let Some(value) = s.optional("scene")
                && let Err(e) = project::depot_path(value)
            {
                errors.push(Diagnostic::error(
                    "depot_path",
                    format!("{path}/scene"),
                    e.to_string(),
                ));
            }
            let inputs = s.inputs()?;
            let outcomes = s.outcomes()?;
            if !inputs.contains_key("start") {
                errors.push(Diagnostic::error(
                    "missing_start_port",
                    &path,
                    "input aliases require start",
                ));
            }
            for ports in [&inputs, &outcomes] {
                if ports.values().collect::<HashSet<_>>().len() != ports.len() {
                    errors.push(Diagnostic::error(
                        "duplicate_socket",
                        &path,
                        "port aliases must name distinct native sockets",
                    ));
                }
            }
            if s.flag("retry_checkpoint") && s.optional("checkpoint").is_none() {
                errors.push(Diagnostic::error(
                    "missing_checkpoint",
                    &path,
                    "retry_checkpoint requires checkpoint",
                ));
            }
            if s.fields.contains_key("phase_prefabs")
                && s.fields.contains_key("inherit_phase_prefabs")
            {
                errors.push(Diagnostic::error(
                    "conflicting_prefabs",
                    &path,
                    "phase_prefabs and inheritance cannot be combined",
                ));
            }
            if let Some(prefabs) = s.fields.get("phase_prefabs").and_then(Value::as_array) {
                for p in prefabs {
                    if !self
                        .phase_prefabs
                        .iter()
                        .any(|v| Some(v.as_str()) == p.as_str())
                    {
                        errors.push(Diagnostic::error(
                            "unknown_prefab",
                            &path,
                            format!("stage prefab {p} is not declared by quest"),
                        ));
                    }
                }
            }
        }
        if self.stages.is_empty() {
            errors.push(Diagnostic::error(
                "missing_stages",
                "/stages",
                "quest requires at least one stage",
            ));
            return Ok(errors);
        }
        let routes = self.routes()?;
        let entry = self.entry_stage.as_deref().unwrap_or(&self.stages[0].id);
        if !ids.contains(entry) {
            errors.push(Diagnostic::error(
                "unknown_entry",
                "/entry_stage",
                format!("unknown entry stage {entry}"),
            ));
        }
        for s in &self.stages {
            let outcomes = s.outcomes()?;
            let routes = &routes[&s.id];
            for name in outcomes.keys() {
                if !routes.contains_key(name) {
                    errors.push(Diagnostic::error(
                        "unrouted_outcome",
                        format!("/stages/{}/on", s.id),
                        format!("outcome {name} has no route"),
                    ));
                }
            }
            for (name, target) in routes {
                if !outcomes.contains_key(name) {
                    errors.push(Diagnostic::error(
                        "unknown_outcome",
                        format!("/stages/{}/on", s.id),
                        format!("unknown outcome {name}"),
                    ));
                }
                if target != "$end" && !ids.contains(target.as_str()) {
                    errors.push(Diagnostic::error(
                        "unknown_destination",
                        format!("/stages/{}/on", s.id),
                        format!("unknown destination {target}"),
                    ));
                }
            }
        }
        let explicit = self.entry_stage.is_some()
            || self.stages.iter().any(|s| {
                ["on", "inputs", "outcomes"]
                    .iter()
                    .any(|k| s.fields.contains_key(*k))
            });
        if !self.parallel_groups.is_empty() && explicit {
            errors.push(Diagnostic::error(
                "parallel_explicit_flow",
                "/parallel_groups",
                "parallel groups cannot combine with explicit routing",
            ));
        }
        if !self.parallel_groups.is_empty() && self.debug_fact.is_some() {
            errors.push(Diagnostic::error(
                "parallel_debug",
                "/debug_fact",
                "parallel groups cannot combine with debug_fact",
            ));
        }
        let mut grouped = HashSet::new();
        let mut group_ids = HashSet::new();
        for group in &self.parallel_groups {
            if !group_ids.insert(&group.id) {
                errors.push(Diagnostic::error(
                    "duplicate_parallel_group",
                    "/parallel_groups",
                    format!("duplicate group {}", group.id),
                ));
            }
            let mut indices = Vec::new();
            for member in group.branches.iter().flatten() {
                if !grouped.insert(member) {
                    errors.push(Diagnostic::error(
                        "overlapping_parallel_stage",
                        "/parallel_groups",
                        format!("stage {member} belongs to multiple branches/groups"),
                    ));
                }
                if let Some((index, s)) = self
                    .stages
                    .iter()
                    .enumerate()
                    .find(|(_, s)| s.id == *member)
                {
                    indices.push(index);
                    if s.kind == "meet_contact" || s.optional("checkpoint").is_some() {
                        errors.push(Diagnostic::error(
                            "unsupported_parallel_stage",
                            "/parallel_groups",
                            format!("meeting/checkpoint stage {member} cannot be parallel"),
                        ));
                    }
                } else {
                    errors.push(Diagnostic::error(
                        "unknown_parallel_stage",
                        "/parallel_groups",
                        format!("unknown stage {member}"),
                    ));
                }
            }
            indices.sort_unstable();
            if indices.windows(2).any(|w| w[1] != w[0] + 1) {
                errors.push(Diagnostic::error(
                    "noncontiguous_parallel_group",
                    "/parallel_groups",
                    format!("group {} must occupy a contiguous stage span", group.id),
                ));
            }
        }
        let mut flow_routes = routes.clone();
        for (index, fork) in self.forks.iter().enumerate() {
            ensure!(self.parallel_groups.is_empty(), "forks cannot combine with sequential parallel groups");
            ensure!(ids.contains(fork.stage.as_str()) && ids.contains(fork.start.as_str()) && ids.contains(fork.complete.as_str()), "unknown fork stage");
            ensure!(routes[&fork.stage].contains_key(&fork.outcome), "unknown fork outcome");
            ensure!(routes[&fork.complete].get("success").is_some_and(|target| target == "$end"), "fork completion must route success to $end");
            flow_routes.get_mut(&fork.stage).context("fork source")?.insert(format!("fork_{index}"), fork.start.clone());
        }
        let mut visited = HashSet::new();
        let mut pending = vec![entry];
        while let Some(name) = pending.pop() {
            if name == "$end" || !visited.insert(name) {
                continue;
            }
            if let Some(next) = flow_routes.get(name) {
                pending.extend(next.values().map(String::as_str));
            }
        }
        if self.parallel_groups.is_empty() {
            for s in &self.stages {
                if !visited.contains(s.id.as_str()) {
                    errors.push(Diagnostic::error(
                        "unreachable_stage",
                        format!("/stages/{}", s.id),
                        "stage cannot be reached from entry",
                    ));
                }
            }
        }
        if let Some(completion) = &self.completion {
            let policies = completion["outcomes"]
                .as_object()
                .context("completion outcomes")?;
            for (stage, edges) in &routes {
                for (outcome, target) in edges {
                    if target == "$end" && !policies.contains_key(outcome) {
                        errors.push(Diagnostic::error(
                            "missing_terminal_policy",
                            format!("/stages/{stage}/on/{outcome}"),
                            "terminal outcome has no quest completion policy",
                        ));
                    }
                }
            }
        }
        self.validate_fact_flow(&flow_routes, entry, &mut errors)?;
        ownership::validate(self, &flow_routes, entry, &mut errors)?;
        Ok(errors)
    }

    fn validate_fact_flow(
        &self,
        routes: &BTreeMap<String, BTreeMap<String, String>>,
        entry: &str,
        errors: &mut Vec<Diagnostic>,
    ) -> Result<()> {
        fn facts(s: &Stage, key: &str) -> HashSet<String> {
            s.fields
                .get("contract")
                .and_then(|v| v.get(key))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        }
        if !self.parallel_groups.is_empty() {
            let mut available = self.external_facts.iter().cloned().collect::<HashSet<_>>();
            let mut index = 0;
            while index < self.stages.len() {
                let group = self.parallel_groups.iter().find(|group| {
                    group
                        .branches
                        .iter()
                        .flatten()
                        .filter_map(|id| self.stages.iter().position(|s| &s.id == id))
                        .min()
                        == Some(index)
                });
                let branches = group.map_or_else(
                    || vec![vec![self.stages[index].id.clone()]],
                    |group| group.branches.clone(),
                );
                let mut completed = Vec::new();
                for branch in branches {
                    let mut known = available.clone();
                    for id in branch {
                        if let Some(stage) = self.stages.iter().find(|s| s.id == id) {
                            for fact in facts(stage, "reads_facts") {
                                if !known.contains(&fact) {
                                    errors.push(Diagnostic::error("missing_fact_producer",format!("/stages/{id}/contract"),format!("fact {fact} needs a prior writer in this branch or an external declaration")));
                                }
                            }
                            known.extend(facts(stage, "writes_facts"));
                        }
                    }
                    completed.push(known);
                }
                for known in completed {
                    available.extend(known);
                }
                index = group
                    .and_then(|group| {
                        group
                            .branches
                            .iter()
                            .flatten()
                            .filter_map(|id| self.stages.iter().position(|s| &s.id == id))
                            .max()
                    })
                    .map_or(index + 1, |last| last + 1);
            }
            return Ok(());
        }
        let external = self.external_facts.iter().cloned().collect::<HashSet<_>>();
        let universe = self
            .stages
            .iter()
            .flat_map(|s| facts(s, "writes_facts"))
            .chain(external.iter().cloned())
            .collect::<HashSet<_>>();
        let mut available = self
            .stages
            .iter()
            .map(|s| {
                (
                    s.id.clone(),
                    if s.id == entry {
                        external.clone()
                    } else {
                        universe.clone()
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        loop {
            let old = available.clone();
            for s in &self.stages {
                let mut incoming = if s.id == entry {
                    vec![external.clone()]
                } else {
                    Vec::new()
                };
                for predecessor in &self.stages {
                    if routes[&predecessor.id].values().any(|t| t == &s.id) {
                        let mut supplied = old[&predecessor.id].clone();
                        supplied.extend(facts(predecessor, "writes_facts"));
                        incoming.push(supplied);
                    }
                }
                let mut intersection = incoming
                    .first()
                    .cloned()
                    .unwrap_or_else(|| external.clone());
                for set in incoming.iter().skip(1) {
                    intersection.retain(|f| set.contains(f));
                }
                available.insert(s.id.clone(), intersection);
            }
            if old == available {
                break;
            }
        }
        for s in &self.stages {
            for fact in facts(s, "reads_facts") {
                if !available[&s.id].contains(&fact) {
                    errors.push(Diagnostic::error(
                        "missing_fact_producer",
                        format!("/stages/{}/contract", s.id),
                        format!("fact {fact} is not produced on every incoming path"),
                    ));
                }
            }
            for event in s
                .fields
                .get("contract")
                .and_then(|v| v.get("external_events"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !self.external_events.iter().any(|e| e == event) {
                    errors.push(Diagnostic::error(
                        "undeclared_external_event",
                        format!("/stages/{}/contract", s.id),
                        format!("external event {event} is not declared"),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Produces a machine-readable plan without writing resources.
    pub fn plan(&self) -> Result<Value> {
        Ok(
            json!({"schema_version":1,"id":self.id,"title":self.title,"entry_stage":self.entry_stage.as_deref().unwrap_or(&self.stages[0].id),"phase_prefabs":self.phase_prefabs,"parallel_groups":self.parallel_groups,"completion":self.completion,"stages":self.stages,"routes":self.routes()?}),
        )
    }

    /// Rejects planned stages unless prototype generation was explicitly selected.
    pub fn require_ready(&self, allow_planned: bool) -> Result<()> {
        ensure!(
            allow_planned || self.stages.iter().all(|s| s.status == "ready"),
            "planned stages require explicit --allow-planned prototype output"
        );
        Ok(())
    }
}
