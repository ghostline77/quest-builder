//! Reusable native quest lifecycles with explicit content and world bindings.
mod attempt;
mod meeting;
mod rewards;
mod root;
mod transfer;
use crate::{
    graph::{Graph, NodeId},
    nodes as n,
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone)]
struct Signal {
    node: NodeId,
    port: String,
}
impl Signal {
    fn port(&self, name: &str) -> Self {
        Self {
            node: self.node,
            port: name.into(),
        }
    }
}
struct Flow {
    g: Graph,
    next: u32,
    start: Signal,
    end: NodeId,
}
impl Flow {
    fn new() -> Result<Self> {
        let mut g = Graph::default();
        let start = n::input(&mut g, "In1")?;
        let end = n::output(&mut g, 1, "Out1")?;
        Ok(Self {
            g,
            next: 10,
            start: Signal {
                node: start,
                port: "Out".into(),
            },
            end,
        })
    }
    fn node(&mut self, f: impl FnOnce(&mut Graph, u32) -> Result<NodeId>) -> Result<Signal> {
        let id = self.next;
        self.next += 1;
        Ok(Signal {
            node: f(&mut self.g, id)?,
            port: "Out".into(),
        })
    }
    fn connect(&mut self, from: &Signal, to: &Signal, input: &str) -> Result<()> {
        self.g.connect(from.node, &from.port, to.node, input)
    }
    fn append(&mut self, from: &Signal, to: Signal, input: &str) -> Result<Signal> {
        self.connect(from, &to, input)?;
        Ok(to)
    }
    fn finish(&mut self, from: &Signal) -> Result<()> {
        self.g.connect(from.node, &from.port, self.end, "In")
    }
    fn scene(&mut self, source: &Signal, spec: &Scene) -> Result<Signal> {
        let mut exits = vec![spec.exit.as_str(), "Default INT", "Default RET"];
        exits.extend(spec.completion_exits.iter().map(String::as_str));
        let node =
            self.node(|g, id| n::scene(g, id, &spec.scene, &spec.origin, &[&spec.entry], &exits))?;
        self.connect(source, &node, &spec.entry)?;
        Ok(node.port(&spec.exit))
    }
    fn tick(&mut self) -> Result<Signal> {
        self.node(|g, id| {
            let kind = g.wrap(json!({"$type":"questTickDelay_ConditionType","tickCount":2}));
            let condition = g.wrap(json!({"$type":"questTimeCondition","type":kind}));
            g.node(
                id,
                "questPauseConditionNodeDefinition",
                &["In"],
                &["Out"],
                json!({"condition":condition}),
            )
        })
    }
    fn document(self, spec: &Value) -> Result<Value> {
        let depot = s(spec, "depot")?;
        crate::project::depot_path(depot)?;
        ensure!(
            depot.ends_with(".questphase"),
            "lifecycle output must be a quest phase"
        );
        let prefabs = spec
            .get("phase_prefabs")
            .cloned()
            .map(serde_json::from_value::<Vec<String>>)
            .transpose()?
            .unwrap_or_default();
        let doc = self.g.document(depot, &prefabs)?;
        crate::audit::Phase::read(&doc)?;
        Ok(doc)
    }
}
fn s<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .with_context(|| format!("lifecycle requires {key}"))
}
fn u(v: &Value, key: &str, default: u64) -> Result<u64> {
    v.get(key).map_or(Ok(default), |v| {
        v.as_u64()
            .with_context(|| format!("{key} must be an unsigned integer"))
    })
}
fn flag(v: &Value, key: &str, default: bool) -> Result<bool> {
    v.get(key).map_or(Ok(default), |v| {
        v.as_bool()
            .with_context(|| format!("{key} must be boolean"))
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scene {
    scene: String,
    origin: String,
    entry: String,
    exit: String,
    #[serde(default)]
    completion_exits: Vec<String>,
}
fn scene(v: &Value, key: &str) -> Result<Option<Scene>> {
    let scene = v
        .get(key)
        .map(|v| serde_json::from_value(v.clone()).with_context(|| format!("{key} scene flow")))
        .transpose()?;
    if let Some(scene) = &scene {
        let scene: &Scene = scene;
        crate::project::depot_path(&scene.scene)?;
        ensure!(
            scene.scene.ends_with(".scene"),
            "scene flow requires .scene resource"
        );
        ensure!(
            scene.origin.starts_with('#') || scene.origin.starts_with("$/"),
            "scene origin must be a NodeRef"
        );
        ensure!(
            !scene.entry.is_empty() && !scene.exit.is_empty(),
            "scene flow needs entry and exit names"
        );
        let mut exits =
            std::collections::HashSet::from([scene.exit.as_str(), "Default INT", "Default RET"]);
        for exit in &scene.completion_exits {
            ensure!(
                !exit.is_empty() && exits.insert(exit),
                "duplicate scene completion exit"
            );
        }
    }
    Ok(scene)
}

/// Generates a complete lifecycle phase without reading any resource snapshots.
pub fn compile(spec: &Value) -> Result<Value> {
    let common = ["kind", "depot", "phase_prefabs", "register", "parent"];
    let allowed = match s(spec, "kind")? {
        "meeting" => &[
            "objective",
            "mappin",
            "community",
            "contact",
            "description_entry",
            "protection_source",
            "setup_trigger",
            "accept_exit",
            "defer_exit",
            "entry",
            "scene_origin",
            "scene",
            "rearm_trigger",
            "deferred_fact",
            "resume_entry",
            "success_fact",
            "reward_item",
            "reward_quantity",
        ][..],
        "personal_link_attempt" => &[
            "device",
            "label",
            "bottom_text",
            "duration_seconds",
            "connected_scene",
        ][..],
        "personal_link_transfer" => &[
            "objective",
            "mappin",
            "device",
            "description_entry",
            "intro_scene",
            "attempt_resource",
            "retry_scene",
            "success_fact",
            "success_scene",
        ][..],
        "root" => &[
            "guard_completed",
            "completed_fact",
            "skipped_stages",
            "stages",
            "call",
            "cleanup_trigger",
            "cleanup_community",
        ][..],
        "finish_and_epilogue" => &[
            "debrief_fact",
            "guard_completed",
            "completed_fact",
            "reward_item",
            "reward_quantity",
            "verified_fact",
            "granted_fact",
            "quest_path",
            "epilogue_phase",
            "epilogue_delay_seconds",
        ][..],
        "reward_reconcile" => &[
            "completed_fact",
            "debrief_fact",
            "verified_fact",
            "reward_item",
            "reward_quantity",
            "granted_fact",
        ][..],
        kind => anyhow::bail!("unsupported lifecycle {kind}"),
    };
    let object = spec.as_object().context("lifecycle declaration object")?;
    for (key, value) in object {
        ensure!(
            common.contains(&key.as_str()) || allowed.contains(&key.as_str()),
            "unsupported lifecycle field {key}"
        );
        if key.ends_with("_fact") {
            ensure!(
                value.as_str().is_some_and(crate::manifest::identifier),
                "{key} requires a fact identifier"
            );
        }
        if [
            "community",
            "device",
            "setup_trigger",
            "rearm_trigger",
            "scene_origin",
            "cleanup_trigger",
            "cleanup_community",
        ]
        .contains(&key.as_str())
        {
            ensure!(
                value
                    .as_str()
                    .is_some_and(|v| v.starts_with('#') || v.starts_with("$/")),
                "{key} requires a NodeRef"
            );
        }
    }
    for field in [
        "intro_scene",
        "retry_scene",
        "success_scene",
        "connected_scene",
    ] {
        let _validated = scene(spec, field)?;
    }
    if spec.get("reward_quantity").is_some() {
        ensure!(
            u(spec, "reward_quantity", 1)? > 0,
            "reward quantity must be positive"
        );
    }
    for field in ["attempt_resource", "epilogue_phase"] {
        if let Some(path) = spec.get(field) {
            let path = path.as_str().context("lifecycle phase path")?;
            crate::project::depot_path(path)?;
            ensure!(
                path.ends_with(".questphase"),
                "{field} requires .questphase"
            );
        }
    }
    if let Some(register) = spec.get("register") {
        ensure!(register.is_boolean(), "register must be boolean");
    }
    if let Some(stages) = spec.get("stages") {
        let stages = stages
            .as_array()
            .filter(|v| !v.is_empty())
            .context("root needs nonempty stages")?;
        for stage in stages {
            let object = stage.as_object().context("root stage")?;
            ensure!(
                object.len() == 2
                    && object.contains_key("id")
                    && object.contains_key("phase_resource"),
                "root stage needs only id and phase_resource"
            );
            ensure!(
                crate::manifest::identifier(s(stage, "id")?),
                "root stage ID"
            );
            let path = s(stage, "phase_resource")?;
            crate::project::depot_path(path)?;
            ensure!(
                path.ends_with(".questphase"),
                "root child needs .questphase"
            );
        }
        if let Some(phone) = spec.get("call") {
            let object = phone.as_object().context("root phone call")?;
            for key in object.keys() {
                ensure!(
                    [
                        "caller",
                        "addressee",
                        "prefab",
                        "start_before",
                        "end_before"
                    ]
                    .contains(&key.as_str()),
                    "unsupported phone boundary field {key}"
                );
            }
            let start = stages
                .iter()
                .position(|v| v["id"] == phone["start_before"])
                .context("call start stage")?;
            let end = stages
                .iter()
                .position(|v| v["id"] == phone["end_before"])
                .context("call end stage")?;
            ensure!(start < end, "call must end after it starts");
            for skipped in spec
                .get("skipped_stages")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                ensure!(
                    skipped != &phone["start_before"] && skipped != &phone["end_before"],
                    "phone boundary cannot be skipped"
                );
            }
        }
    }
    match s(spec, "kind")? {
        "meeting" => meeting::build(spec),
        "personal_link_attempt" => attempt::build(spec),
        "personal_link_transfer" => transfer::build(spec),
        "root" => root::build(spec),
        "finish_and_epilogue" => rewards::finish(spec),
        "reward_reconcile" => rewards::reconcile(spec),
        kind => anyhow::bail!("unsupported lifecycle {kind}"),
    }
}
