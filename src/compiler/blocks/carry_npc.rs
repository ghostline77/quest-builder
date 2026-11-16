//! Native carry npc stage generation.

use super::Block;
use super::shared::bindings::{list, s};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Context, Result};
use serde_json::{Value, json};

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let mut tail = b.enter()?;
    tail = b.pin(tail, "Active", false)?;
    let mounted = b.node(|g, id| {
        n::mount(
            g,
            id,
            n::entity(None, &[]),
            n::entity(Some(stage.string("community")?), &[stage.string("entry")?]),
            true,
            false,
            "Invalid",
            "OnMount",
            false,
        )
    })?;
    tail = b.append(&tail, mounted, "In")?;
    for waypoint in list(stage, "waypoints")?.iter().chain(std::iter::once(
        stage
            .fields
            .get("destination")
            .context("carry destination")?,
    )) {
        let node = b.node(|g, id| {
            n::carried_inside(
                g,
                id,
                stage.string("community")?,
                stage.string("entry")?,
                s(waypoint)?,
            )
        })?;
        tail = b.append(&tail, node, "In")?;
    }
    if let Some(slot) = stage.optional("placement_slot") {
        let actor = n::entity(Some(stage.string("community")?), &[stage.string("entry")?]);
        let released = b.node(|g, id| {
            n::mount(
                g,
                id,
                n::entity(None, &[]),
                actor.clone(),
                true,
                false,
                "Invalid",
                "OnUnmount",
                false,
            )
        })?;
        b.connect(&tail, &released, "In")?;
        let release=b.node(|g,id|{let kind=g.wrap(json!({"$type":"questEntityManagerMountPuppet_NodeType","assign":0,"childRef":actor,"isInstant":1,"isParentPlayer":1,"parentRef":n::entity(None,&[]),"slotName":n::cname("None")}));g.node(id,"questEntityManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))})?;
        b.connect(&tail, &release, "In")?;
        let parent = stage
            .optional("placement_parent")
            .unwrap_or(stage.string("destination")?);
        let attached = b.node(|g, id| {
            n::mount(
                g,
                id,
                n::entity(Some(parent), &[]),
                actor.clone(),
                false,
                false,
                "Invalid",
                "OnMount",
                false,
            )
        })?;
        b.connect(&released, &attached, "In")?;
        let placed=b.node(|g,id|{let kind=g.wrap(json!({"$type":"questEntityManagerMountPuppet_NodeType","assign":1,"childRef":actor,"isInstant":1,"isParentPlayer":0,"parentRef":n::entity(Some(parent),&[]),"slotName":n::cname(slot),"removePitchRollRotation":1}));g.node(id,"questEntityManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))})?;
        b.connect(&released, &placed, "In")?;
        tail = attached;
    }
    tail = b.objective(tail, "success")?;
    tail = b.pin(tail, "Inactive", false)?;
    tail = b.fact(tail, stage.optional("completion_fact"))?;
    b.finish(tail, "success")?;

    b.document()
}
