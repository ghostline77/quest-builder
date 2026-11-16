use super::{Block, Signal};
use crate::{graph::NodeId, manifest::Stage, nodes as n};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

pub(in crate::compiler::blocks) fn s(v: &Value) -> Result<&str> {
    v.as_str().context("owned-scope string binding")
}
pub(in crate::compiler::blocks) fn array<'a>(v: &'a Value, key: &str) -> Result<&'a [Value]> {
    v.get(key).map_or(Ok(&[][..]), |v| {
        v.as_array()
            .map(Vec::as_slice)
            .with_context(|| format!("{key} must be array"))
    })
}

pub(in crate::compiler::blocks) struct Scope<'a> {
    pub(in crate::compiler::blocks) block: Block<'a>,
    pub(in crate::compiler::blocks) owned: Vec<NodeId>,
    completed_scenes: BTreeMap<String, HashSet<NodeId>>,
    pub(in crate::compiler::blocks) initial_cancel: Option<Signal>,
    pub(in crate::compiler::blocks) activation: Signal,
}
impl<'a> Scope<'a> {
    pub(in crate::compiler::blocks) fn new(stage: &'a Stage) -> Result<Self> {
        let block = Block::new(stage)?;
        let activation = block.start.clone();
        Ok(Self {
            block,
            owned: Vec::new(),
            completed_scenes: BTreeMap::new(),
            initial_cancel: None,
            activation,
        })
    }
    pub(in crate::compiler::blocks) fn watch(
        &mut self,
        f: impl FnOnce(&mut crate::graph::Graph, u32) -> Result<NodeId>,
    ) -> Result<Signal> {
        let signal = self.block.node(f)?;
        self.owned.push(signal.node);
        Ok(signal)
    }
    pub(in crate::compiler::blocks) fn snapshot(
        &mut self,
        source: Signal,
        fact: &str,
    ) -> Result<(Signal, Signal)> {
        let node = self
            .block
            .node(|g, id| n::fact_condition(g, id, fact, "Greater", 0, true))?;
        self.block.connect(&source, &node, "In")?;
        Ok((node.port("True"), node.port("False")))
    }
    pub(in crate::compiler::blocks) fn begin(&mut self) -> Result<Signal> {
        let stage = self.block.stage;
        let mut source = self.block.start.clone();
        if let Some(commit) = stage.optional("commit_fact") {
            let (done, remaining) = self.snapshot(source, commit)?;
            self.block.finish(done, "success")?;
            source = remaining;
        }
        if let Some(cancel) = stage.optional("cancellation_fact") {
            let (cancelled, remaining) = self.snapshot(source, cancel)?;
            self.initial_cancel = Some(cancelled);
            source = remaining;
        }
        self.activation = source.clone();
        self.actions(source, array(&json!(stage.fields), "setup")?)
    }
    pub(in crate::compiler::blocks) fn child(
        &mut self,
        source: Signal,
        spec: &Value,
        scene: bool,
    ) -> Result<BTreeMap<String, Signal>> {
        let entry = s(&spec["entry"])?;
        let exits = spec["exits"]
            .as_object()
            .context("bound terminal/notification exits")?;
        ensure!(!exits.is_empty(), "bound child needs exits");
        let names = exits.values().map(s).collect::<Result<Vec<_>>>()?;
        let node = if scene {
            self.watch(|g, id| {
                n::scene(
                    g,
                    id,
                    s(&spec["scene"])?,
                    s(&spec["origin"])?,
                    &[entry],
                    &names,
                )
            })?
        } else {
            self.watch(|g, id| n::phase(g, id, s(&spec["phase"])?, &[entry], &names))?
        };
        self.block.connect(&source, &node, entry)?;
        if scene {
            for (outcome, exit) in exits {
                if !matches!(s(exit)?, "Default RET" | "Default INT") {
                    self.completed_scenes
                        .entry(outcome.clone())
                        .or_default()
                        .insert(node.node);
                }
            }
        }
        exits
            .iter()
            .map(|(alias, name)| Ok((alias.clone(), node.port(s(name)?))))
            .collect::<Result<_>>()
    }
    pub(in crate::compiler::blocks) fn once(&mut self, source: Signal) -> Result<Signal> {
        self.block.join(&[source], false)
    }
    pub(in crate::compiler::blocks) fn race(
        &mut self,
        routes: BTreeMap<String, Signal>,
    ) -> Result<BTreeMap<String, Signal>> {
        let mut gates = BTreeMap::new();
        for (name, signal) in routes {
            gates.insert(name, self.once(signal)?);
        }
        let mut cuts = BTreeMap::new();
        for (name, signal) in &gates {
            let cut = self.block.node(|g, id| {
                g.node(
                    id,
                    "questCutControlNodeDefinition",
                    &["In"],
                    &["Out", "CutSource"],
                    json!({"permanent":1}),
                )
            })?;
            cuts.insert(name.clone(), self.block.append(signal, cut, "In")?);
        }
        for (name, cut) in &cuts {
            let targets = self
                .owned
                .iter()
                .copied()
                // A terminal scene exit has already completed. Cutting it again
                // cancels its outstanding commands during the next scene's handoff.
                .filter(|target| {
                    !self
                        .completed_scenes
                        .get(name)
                        .is_some_and(|completed| completed.contains(target))
                })
                .chain(
                    gates
                        .iter()
                        .filter(|(key, _)| *key != name)
                        .map(|(_, s)| s.node),
                )
                .chain(
                    cuts.iter()
                        .filter(|(key, _)| *key != name)
                        .map(|(_, s)| s.node),
                )
                .collect::<Vec<_>>();
            let mut seen = HashSet::new();
            for target in targets {
                if seen.insert(target) {
                    self.block
                        .g
                        .connect(cut.node, "CutSource", target, "CutDestination")?;
                }
            }
        }
        Ok(cuts)
    }
    pub(in crate::compiler::blocks) fn actions(
        &mut self,
        mut source: Signal,
        actions: &[Value],
    ) -> Result<Signal> {
        for action in actions {
            let kind = s(&action["kind"])?;
            if kind == "phase" {
                source=self.child(source,&json!({"phase":action["phase"],"entry":action["entry"],"exits":{"done":action["exit"]}}),false)?.remove("done").context("restoration acknowledgement")?;
                continue;
            }
            let (node, input) = match kind {
                "fact" => (
                    self.block.node(|g, id| {
                        n::fact(
                            g,
                            id,
                            s(&action["fact"])?,
                            action.get("value").and_then(Value::as_i64).unwrap_or(1),
                        )
                    })?,
                    "In",
                ),
                "render_fade" => {
                    let duration = action.get("duration_seconds").and_then(Value::as_f64).unwrap_or(0.);
                    ensure!(duration.is_finite() && duration >= 0., "invalid scope fade duration");
                    let fade_in = action["fade_in"].as_bool().context("scope fade_in boolean")?;
                    let node = self.block.node(|g, id| {
                        let kind = g.wrap(json!({"$type":"questSetFadeInOut_NodeType", "duration":duration,
                            "fadeColor":{"$type":"Color","Alpha":0,"Blue":0,"Green":0,"Red":0},"fadeIn":u8::from(fade_in)}));
                        g.node(id,"questRenderFxManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))
                    })?;
                    (node, "In")
                }
                "scene_tier" => {
                    let tier = s(&action["tier"])?;
                    ensure!(matches!(tier, "Tier1" | "Tier2" | "Tier3"), "invalid scope scene tier");
                    let node = self.block.node(|g, id| {
                        let kind = g.wrap(json!({"$type":"questSetTier_NodeType", "tier":tier,
                            "forceEmptyHands":0,"useEnterAnim":0,"useExitAnim":0,"usePlayerWorkspot":0}));
                        g.node(id,"questSceneManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))
                    })?;
                    (node, "In")
                }
                "item" => (
                    self.block.node(|g, id| {
                        n::item_for_actor(
                            g,
                            id,
                            s(&action["item"])?,
                            action.get("quantity").and_then(Value::as_u64).unwrap_or(1),
                            false,
                            action.get("community").and_then(Value::as_str).map(|community| (community, action.get("entry").and_then(Value::as_str).unwrap_or(""))),
                        )
                    })?,
                    "In",
                ),
                "objective" => (
                    self.block.node(|g, id| {
                        n::objective(
                            g,
                            id,
                            s(&action["path"])?,
                            action
                                .get("quest")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        )
                    })?,
                    s(&action["state"])?,
                ),
                "mappin" => (
                    self.block
                        .node(|g, id| n::mappin(g, id, s(&action["path"])?, false))?,
                    s(&action["state"])?,
                ),
                "message" => (
                    self.block.node(|g, id| {
                        n::journal(g, id, s(&action["path"])?, "gameJournalPhoneMessage", 1)
                    })?,
                    action
                        .get("state")
                        .and_then(Value::as_str)
                        .unwrap_or("Active"),
                ),
                "journal" => {
                    let node = self.block.node(|g, id| {
                        let index = action["file_entry_index"]
                            .as_u64()
                            .and_then(|v| u32::try_from(v).ok())
                            .context("journal file entry index")?;
                        let node = n::journal(
                            g,
                            id,
                            s(&action["path"])?,
                            s(&action["class_name"])?,
                            index,
                        )?;
                        g.set_nested(node, &["type", "sendNotification"], json!(0))?;
                        Ok(node)
                    })?;
                    (node, s(&action["state"])?)
                }
                "community" => (
                    self.block.node(|g, id| {
                        n::community(
                            g,
                            id,
                            s(&action["community"])?,
                            s(&action["action"])?,
                            Some(s(&action["entry"])?),
                            Some(
                                action
                                    .get("phase")
                                    .and_then(Value::as_str)
                                    .unwrap_or("default"),
                            ),
                        )
                    })?,
                    "In",
                ),
                "trigger_state" => (
                    self.block.node(|g, id| {
                        n::set_trigger_state(
                            g,
                            id,
                            s(&action["trigger"])?,
                            action["enabled"]
                                .as_bool()
                                .context("trigger enabled state")?,
                        )
                    })?,
                    "In",
                ),
                "device" => (
                    self.block.node(|g, id| {
                        n::device_action_properties(
                            g,
                            id,
                            s(&action["device"])?,
                            s(&action["controller_class"])?,
                            s(&action["action"])?,
                            action.get("properties").unwrap_or(&json!({})),
                        )
                    })?,
                    "In",
                ),
                "terminal_document" => (
                    self.block.node(|g, id| {
                        n::terminal_document_enabled(
                            g,
                            id,
                            s(&action["device"])?,
                            s(&action["document"])?,
                            action["enabled"].as_bool().context("document enabled state")?,
                        )
                    })?,
                    "In",
                ),
                "follower" => {
                    let community = s(&action["community"])?;
                    let entry = s(&action["entry"])?;
                    let assign = action["assign"].as_bool().context("follower assignment")?;
                    if assign {
                        let ai = self
                            .block
                            .node(|g, id| n::gameplay_ai(g, id, community, entry))?;
                        source = self.block.append(&source, ai, "In")?;
                    }
                    (
                        self.block
                            .node(|g, id| n::follower(g, id, community, entry, assign))?
                            .port("Success"),
                        "In",
                    )
                }
                "elevator_attachment" => {
                    let node = self.block.node(|g,id| {
                        let kind = g.wrap(json!({"$type":"questElevator_ManageNPCAttachment_NodeType","params":[{
                            "$type":"questElevator_ManageNPCAttachment_NodeTypeParams",
                            "elevatorRef":n::node_ref(s(&action["elevator"])?),
                            "npcRef":n::entity(Some(s(&action["community"])?),&[s(&action["entry"])?]),
                            "action":if action["attach"].as_bool().context("elevator attachment")? {"Attach"} else {"Detach"}
                        }]}));
                        g.node(id,"questInteractiveObjectManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))
                    })?;
                    (node, "In")
                }
                "stop_workspot" => {
                    let flag = |key: &str, default: bool| {
                        action.get(key).map_or(Ok(default), |value| {
                            value.as_bool().context("workspot release flag")
                        })
                    };
                    let stop = self.block.node(|g, id| {
                        let stop = n::stop_workspot(
                            g,
                            id,
                            action.get("community").and_then(Value::as_str),
                            action.get("entry").and_then(Value::as_str),
                        )?;
                        g.set_nested(
                            stop,
                            &["paramsV1", "instant"],
                            json!(u8::from(flag("instant", false)?)),
                        )?;
                        g.set_nested(
                            stop,
                            &["paramsV1", "finishAnimation"],
                            json!(u8::from(flag("finish_animation", true)?)),
                        )?;
                        Ok(stop)
                    })?;
                    if !flag("wait_for_completion", true)? {
                        self.block.connect(&source, &stop, "In")?;
                        continue;
                    }
                    (stop.port("Success"), "In")
                }
                "weather_reset" => {
                    let node=self.block.node(|g,id|{let kind=g.wrap(json!({"$type":"questPlayEnv_SetWeather","reset":1,"source":n::cname(s(&action["source"])?),"blendTime":action.get("blend_seconds").cloned().unwrap_or(json!(5)),"priority":0,"weatherID":{"$type":"TweakDBID","$storage":"uint64","$value":"0"}}));g.node(id,"questEnvironmentManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))})?;
                    (node, "In")
                }
                "prefab_variant" => {
                    let node=self.block.node(|g,id|{let kind=g.wrap(json!({"$type":"questTogglePrefabVariant_NodeType","params":[{"$type":"questTogglePrefabVariant_NodeTypeParams","prefabNodeRef":n::node_ref(s(&action["prefab"])?),"variantStates":[{"$type":"questVariantState","name":n::cname(s(&action["variant"])?),"show":u8::from(action["show"].as_bool().context("variant visibility")?)}]}]}));g.node(id,"questWorldDataManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))})?;
                    (node, "In")
                }
                _ => bail!("unsupported scope action {kind}"),
            };
            source = self.block.append(&source, node, input)?;
            if kind == "community" && action["action"] == "Activate" {
                let ready = self.block.node(|g, id| {
                    n::spawned(
                        g,
                        id,
                        s(&action["community"])?,
                        action.get("entry").and_then(Value::as_str),
                    )
                })?;
                source = self.block.append(&source, ready, "In")?;
            }
        }
        Ok(source)
    }
    pub(in crate::compiler::blocks) fn with_cancel(
        &mut self,
        mut routes: BTreeMap<String, Signal>,
    ) -> Result<BTreeMap<String, Signal>> {
        if let Some(fact) = self.block.stage.optional("cancellation_fact") {
            let node = self.watch(|g, id| n::fact_condition(g, id, fact, "Greater", 0, false))?;
            self.block.connect(&self.activation, &node, "In")?;
            let signal = if let Some(initial) = self.initial_cancel.clone() {
                self.block.join(&[initial, node], false)?
            } else {
                node
            };
            routes.insert("cancelled".into(), signal);
        }
        Ok(routes)
    }
    pub(in crate::compiler::blocks) fn finish(
        &mut self,
        routes: BTreeMap<String, Signal>,
        extra_restore: &[Value],
        reported: Option<&str>,
    ) -> Result<()> {
        let routes = self.with_cancel(routes)?;
        let routes = self.race(routes)?;
        let fields = json!(self.block.stage.fields);
        for (outcome, signal) in routes {
            let mut tail = self.actions(signal, extra_restore)?;
            tail = self.actions(tail, array(&fields, "restore")?)?;
            if outcome == "success" {
                tail = self.block.fact(tail, reported)?;
            }
            tail = self.actions(tail, array(&fields["effects"], &outcome)?)?;
            if outcome == "success" {
                tail = self
                    .block
                    .fact(tail, self.block.stage.optional("commit_fact"))?;
            }
            self.block.finish(tail, &outcome)?;
        }
        Ok(())
    }
}
