//! Native escort npc stage generation.

use super::Block;
use super::shared::actor_lifecycle::{actor, cleanup, resolution};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;
    let active = b.enter()?;

    let running = actor(&mut b, active.clone(), "enter", "assign_follower")?;
    let gates = stage.fields["destinations"]
        .as_array()
        .context("escort destinations")?;
    ensure!(!gates.is_empty(), "escort needs destinations");
    let pins = if let Some(pins) = stage.fields.get("route_mappins") {
        pins.as_array()
            .context("route mappins")?
            .iter()
            .map(|p| p.as_str().map(str::to_owned).context("mappin path"))
            .collect::<Result<Vec<_>>>()?
    } else {
        vec![stage.string("mappin")?.to_owned(); gates.len()]
    };
    ensure!(
        pins.len() == gates.len(),
        "escort route_mappins count must match destinations"
    );
    let mut tail = running;
    for (gate, pin) in gates.iter().zip(&pins) {
        let show = b.node(|g, id| n::mappin(g, id, pin, false))?;
        tail = b.append(&tail, show, "Active")?;
        let arrived = b.node(|g, id| {
            let actor=n::entity(Some(stage.string("community")?),&[stage.string("entry")?]);
            let trigger=g.wrap(json!({"$type":"questTriggerCondition","activatorRef":actor,"isPlayerActivator":0,"triggerAreaRef":n::node_ref(gate.as_str().context("escort trigger")?),"type":"IsInside"}));
            let state=g.wrap(json!({"$type":"questCharacterCombat_ConditionType","objectRef":actor,"isPlayer":0,"inverted":1}));
            let peaceful=g.wrap(json!({"$type":"questCharacterCondition","type":state}));
            let condition=g.wrap(json!({"$type":"questLogicalCondition","operation":"AND","conditions":[trigger,peaceful]}));
            g.node(id,"questPauseConditionNodeDefinition",&["In"],&["Out"],json!({"condition":condition}))
        })?;
        tail = b.append(&tail, arrived, "In")?;
        let hide = b.node(|g, id| n::mappin(g, id, pin, false))?;
        tail = b.append(&tail, hide, "Inactive")?;
    }
    let routes = resolution(&mut b, active, tail)?;
    cleanup(&mut b, routes, &pins, None)?;

    b.document()
}
