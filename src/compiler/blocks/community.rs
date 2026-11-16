//! Community activation, actor readiness, attitudes and scoped cleanup.
use super::{Block, shared::content};
use crate::{manifest::Stage, nodes as n};
use anyhow::{Context, Result};
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;
    let mut tail = b.start.clone();
    if let Some(trigger) = stage.optional("wait_trigger") {
        let wait = b.node(|g, id| {
            n::trigger(
                g,
                id,
                trigger,
                stage.optional("trigger_event").unwrap_or("IsOutside"),
            )
        })?;
        tail = b.append(&tail, wait, "In")?;
    }
    let action = stage.string("action")?;
    let community = stage.string("community")?;
    let node = b.node(|g, id| {
        n::community(
            g,
            id,
            community,
            action,
            stage.optional("entry"),
            stage.optional("phase"),
        )
    })?;
    tail = b.append(&tail, node, "In")?;
    if action == "Activate" {
        let ready = b.node(|g, id| n::spawned(g, id, community, stage.optional("entry")))?;
        tail = b.append(&tail, ready, "In")?;
    }
    if let Some(group) = stage.optional("attitude_group") {
        for entry in stage.fields["entries"]
            .as_array()
            .context("community actor entries")?
        {
            let entry = entry.as_str().context("community actor entry")?;
            if let Some(previous) = stage.optional("previous_attitude_group") {
                tail = content::attitude(&mut b, tail, community, entry, previous)?;
            }
            tail = content::attitude(&mut b, tail, community, entry, group)?;
        }
    }
    if let Some(tag) = stage.optional("protection_source") {
        for entry in stage.fields["entries"]
            .as_array()
            .context("community protected actor entries")?
        {
            let entry = entry.as_str().context("community protected actor entry")?;
            tail = content::protect(&mut b, tail, community, entry, tag)?;
        }
    }
    tail = b.fact(tail, stage.optional("completion_fact"))?;
    b.finish(tail, "success")?;
    b.document()
}
