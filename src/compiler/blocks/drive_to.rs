//! Native drive to stage generation.

use super::Block;
use super::shared::bindings::{list, s};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Context, Result};
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let mut tail = b.enter()?;
    tail = b.pin(tail, "Active", false)?;

    for waypoint in list(stage, "waypoints")?.iter().chain(std::iter::once(
        stage
            .fields
            .get("destination")
            .context("drive destination")?,
    )) {
        let node = b.node(|g, id| {
            n::actor_inside(
                g,
                id,
                s(waypoint)?,
                n::entity(
                    Some(stage.string("vehicle_community")?),
                    &[stage.string("vehicle_entry")?],
                ),
            )
        })?;
        tail = b.append(&tail, node, "In")?;
    }

    tail = b.objective(tail, "success")?;
    tail = b.pin(tail, "Inactive", false)?;

    tail = b.fact(tail, Some(stage.string("completion_fact")?))?;

    b.finish(tail, "success")?;

    b.document()
}
