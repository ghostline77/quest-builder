//! Native owned scope stage generation.

use super::shared::owned::{Scope, s};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Context, Result};
use serde_json::{Value, json};

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut scope = Scope::new(stage)?;
    let data = json!(stage.fields);
    let mut running = scope.begin()?;
    if let Some(fact) = stage.optional("wait_fact") {
        let gate = scope.watch(|g, id| n::fact_condition(g, id, fact, "Greater", 0, false))?;
        scope.block.connect(&running, &gate, "In")?;
        running = gate;
    }
    if let Some(trigger) = stage.optional("wait_trigger") {
        let gate = scope.watch(|g, id| {
            if let Some(actor) = data.get("trigger_actor") {
                let condition = g.wrap(serde_json::json!({
                    "$type": "questTriggerCondition",
                    "activatorRef": n::entity(
                        Some(actor["community"].as_str().context("trigger actor community")?),
                        &[actor["entry"].as_str().context("trigger actor entry")?],
                    ),
                    "isPlayerActivator": 0,
                    "triggerAreaRef": n::node_ref(trigger),
                    "type": stage.optional("trigger_event").unwrap_or("IsInside"),
                }));
                return g.node(
                    id,
                    "questPauseConditionNodeDefinition",
                    &["In"],
                    &["Out"],
                    serde_json::json!({"condition": condition}),
                );
            }
            n::trigger(
                g,
                id,
                trigger,
                stage.optional("trigger_event").unwrap_or("IsInside"),
            )
        })?;
        scope.block.connect(&running, &gate, "In")?;
        running = gate;
    }

    if let Some(context) = data.get("wait_ui_context") {
        let state = s(&context["state"])?;
        let active = context["active"].as_bool().context("UI context active boolean")?;
        let gate = scope.watch(|g, id| {
            let kind = g.wrap(json!({"$type":"questUIContextState_ConditionType",
                "state":state,"active":u8::from(active)}));
            let condition = g.wrap(json!({"$type":"questUICondition","type":kind}));
            g.node(id,"questPauseConditionNodeDefinition",&["In"],&["Out"],json!({"condition":condition}))
        })?;
        scope.block.connect(&running, &gate, "In")?;
        running = gate;
    }

    let mut routes = if let Some(child) = data.get("child") {
        let scene = child.get("scene").is_some();
        if let Some(fact) = stage.optional("resume_fact") {
            anyhow::ensure!(scene, "resume bindings require a scene child");
            let (resuming, first) = scope.snapshot(running.clone(), fact)?;
            let first = scope.block.fact(first, Some(fact))?;
            let mut resumed = child.clone();
            resumed["entry"] = json!(stage.string("resume_entry")?);
            let initial = scope.child(first, child, true)?;
            let resumed = scope.child(resuming, &resumed, true)?;
            initial
                .into_iter()
                .map(|(name, signal)| {
                    let joined = scope.block.join(&[signal, resumed[&name].clone()], false)?;
                    Ok((name, joined))
                })
                .collect::<Result<std::collections::BTreeMap<_, _>>>()?
        } else {
            scope.child(running.clone(), child, scene)?
        }
    } else {
        std::collections::BTreeMap::from([("success".into(), running.clone())])
    };
    if let Some(defeat) = data.get("defeat") {
        let defeated = scope.watch(|g, id| {
            n::character_outcome(
                g,
                id,
                s(&defeat["community"])?,
                s(&defeat["entry"])?,
                true,
                true,
                true,
            )
        })?;
        scope.block.connect(&running, &defeated, "In")?;
        let outcome = s(&defeat["outcome"])?;
        let signal = if let Some(previous) = routes.remove(outcome) {
            scope.block.join(&[previous, defeated], false)?
        } else {
            defeated
        };
        routes.insert(outcome.into(), signal);
    }
    if let Some(notifications) = data.get("notifications").and_then(Value::as_object) {
        for (name, fact) in notifications {
            let signal = routes.remove(name).context("unknown child notification")?;
            scope.block.fact(signal, Some(s(fact)?))?;
        }
    }

    scope.finish(routes, &[], None)?;
    scope.block.document()
}
