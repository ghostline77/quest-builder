use super::{Block, Signal};
use crate::nodes as n;
use anyhow::{Result, ensure};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

pub(in crate::compiler::blocks) fn actor(
    b: &mut Block<'_>,
    mut tail: Signal,
    event: &str,
    default: &str,
) -> Result<Signal> {
    let stage = b.stage;
    let key = format!("on_{event}");
    let action = stage
        .fields
        .get("actor_lifecycle")
        .and_then(|v| v.get(key))
        .and_then(Value::as_str)
        .unwrap_or(default);
    if action == "retain" {
        return Ok(tail);
    }
    ensure!(
        matches!(action, "assign_follower" | "release"),
        "unsupported actor lifecycle policy {action}"
    );
    if action == "assign_follower" {
        let ai = b.node(|g, id| {
            n::gameplay_ai(g, id, stage.string("community")?, stage.string("entry")?)
        })?;
        tail = b.append(&tail, ai, "In")?;
    }
    let role = b
        .node(|g, id| {
            n::follower(
                g,
                id,
                stage.string("community")?,
                stage.string("entry")?,
                action == "assign_follower",
            )
        })?
        .port("Success");
    b.append(&tail, role, "In")
}
pub(in crate::compiler::blocks) fn resolution(
    b: &mut Block<'_>,
    running: Signal,
    success: Signal,
) -> Result<BTreeMap<String, Signal>> {
    let stage = b.stage;
    let killed = b.node(|g, id| {
        n::character_outcome(
            g,
            id,
            stage.string("community")?,
            stage.string("entry")?,
            true,
            true,
            true,
        )
    })?;
    b.connect(&running, &killed, "In")?;
    let mut failures = vec![killed];
    let combat_failure = stage.kind == "escort_npc"
        && stage
            .fields
            .get("allow_combat_interrupt")
            .and_then(Value::as_bool)
            == Some(false);
    if combat_failure {
        let fighting=b.node(|g,id|{let kind=g.wrap(serde_json::json!({"$type":"questCharacterCombat_ConditionType","objectRef":n::entity(Some(stage.string("community")?),&[stage.string("entry")?]),"isPlayer":0,"inverted":0}));let condition=g.wrap(serde_json::json!({"$type":"questCharacterCondition","type":kind}));g.node(id,"questPauseConditionNodeDefinition",&["In"],&["Out"],serde_json::json!({"condition":condition}))})?;
        b.connect(&running, &fighting, "In")?;
        failures.push(fighting);
    }
    let failure = b.join(&failures, false)?;
    let mut routes = BTreeMap::from([("failure".into(), failure), ("success".into(), success)]);
    if let Some(fact) = stage.optional("cancellation_fact") {
        let cancelled = b.node(|g, id| n::fact_condition(g, id, fact, "Greater", 0, false))?;
        b.connect(&running, &cancelled, "In")?;
        routes.insert("cancelled".into(), cancelled);
    }
    let owned = b.created.clone();
    b.race(routes, &owned)
}
pub(in crate::compiler::blocks) fn cleanup(
    b: &mut Block<'_>,
    routes: BTreeMap<String, Signal>,
    pins: &[String],
    attackers: Option<&str>,
) -> Result<()> {
    let stage = b.stage;
    for (outcome, mut tail) in routes {
        let mut seen = HashSet::new();
        for pin in pins {
            if seen.insert(pin) {
                let node = b.node(|g, id| n::mappin(g, id, pin, false))?;
                tail = b.append(&tail, node, "Inactive")?;
            }
        }
        if let Some(community) = attackers {
            let node = b.node(|g, id| n::community(g, id, community, "Deactivate", None, None))?;
            tail = b.append(&tail, node, "In")?;
        }
        tail = actor(b, tail, &outcome, "release")?;
        tail = b.objective(tail, &outcome)?;
        let field = match outcome.as_str() {
            "failure" => "failure_fact",
            "cancelled" => "cancelled_fact",
            _ => "completion_fact",
        };
        tail = b.fact(tail, stage.optional(field))?;
        if outcome == "failure" && stage.flag("block_on_failure") {
            ensure!(
                !b.outputs.contains_key("failure"),
                "blocking failure cannot emit a terminal failure port"
            );
            continue;
        }
        if b.outputs.contains_key(&outcome) {
            b.finish(tail, &outcome)?;
        } else if outcome == "failure" && stage.kind == "defend_target" {
            b.finish(tail, "success")?;
        }
    }
    Ok(())
}
