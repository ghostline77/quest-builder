//! Native defend target stage generation.

use super::shared::actor_lifecycle::{actor, cleanup, resolution};
use super::{Block, fractional_delay};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Context, Result};
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;
    let active = b.enter()?;

    let mut running = actor(&mut b, active.clone(), "enter", "retain")?;
    let attackers = stage.fields.get("attackers");
    if let Some(attackers) = attackers {
        let community = attackers["community"]
            .as_str()
            .context("attacker community")?;
        let activate = b.node(|g, id| n::community(g, id, community, "Activate", None, None))?;
        running = b.append(&running, activate, "In")?;
        let ready = b.node(|g, id| n::spawned(g, id, community, None))?;
        running = b.append(&running, ready, "In")?;
        for attacker in attackers["entries"]
            .as_array()
            .context("attacker entries")?
        {
            let node = b
                .node(|g, id| {
                    n::combat_threat(
                        g,
                        id,
                        community,
                        attacker["entry"].as_str().context("attacker entry")?,
                        Some("questCombatNodeParams_ShootAt"),
                        0.,
                    )
                })?
                .port("Success");
            if attacker.get("target").and_then(Value::as_str) == Some("protected") {
                b.g.set_nested(
                    node.node,
                    &["params", "targetPuppetRef"],
                    n::entity(Some(stage.string("community")?), &[stage.string("entry")?]),
                )?;
            }
            running = b.append(&running, node, "In")?;
        }
    }
    let success = if let Some(duration) = stage.fields.get("duration_seconds") {
        fractional_delay(&mut b, duration.as_f64().context("defense duration")?)?
    } else {
        b.node(|g, id| {
            n::fact_condition(g, id, stage.string("completion_fact")?, "Greater", 0, false)
        })?
    };
    b.connect(&running, &success, "In")?;
    let routes = resolution(&mut b, active, success)?;
    let cleanup_community = attackers
        .filter(|a| a.get("cleanup").and_then(Value::as_bool).unwrap_or(true))
        .and_then(|a| a["community"].as_str());
    cleanup(&mut b, routes, &[], cleanup_community)?;

    b.document()
}
