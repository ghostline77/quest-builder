//! Native deliver vehicle stage generation.

use super::Block;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let mut tail = b.enter()?;
    tail = b.pin(tail, "Active", false)?;
    let arrived = b.node(|g, id| {
        n::actor_inside(
            g,
            id,
            stage.string("destination")?,
            n::entity(Some(stage.string("vehicle")?), &[]),
        )
    })?;
    tail = b.append(&tail, arrived, "In")?;
    if stage
        .fields
        .get("require_stop")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        let stopped = b.node(|g, id| n::vehicle_stopped(g, id, stage.string("vehicle")?))?;
        tail = b.append(&tail, stopped, "In")?;
    }
    if stage.flag("require_player_exit") {
        let mounted = b.node(|g, id| {
            n::mount(
                g,
                id,
                n::entity(Some(stage.string("vehicle")?), &[]),
                n::entity(None, &[]),
                false,
                true,
                "Invalid",
                "OnMount",
                true,
            )
        })?;
        b.connect(&tail, &mounted, "In")?;
        let unmounted = b.node(|g, id| {
            n::mount(
                g,
                id,
                n::entity(Some(stage.string("vehicle")?), &[]),
                n::entity(None, &[]),
                false,
                true,
                "Invalid",
                "OnUnmount",
                false,
            )
        })?;
        b.connect(&mounted.port("True"), &unmounted, "In")?;
        tail = b.join(&[mounted.port("False"), unmounted], false)?;
    }
    tail = b.objective(tail, "success")?;
    tail = b.pin(tail, "Inactive", false)?;
    tail = b.fact(tail, stage.optional("completion_fact"))?;
    b.finish(tail, "success")?;

    b.document()
}
