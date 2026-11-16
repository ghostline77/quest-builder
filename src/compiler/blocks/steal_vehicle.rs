//! Native steal vehicle stage generation.

use super::Block;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let mut tail = b.enter()?;
    tail = b.pin(tail, "Active", false)?;

    let node = b.node(|g, id| {
        n::mount(
            g,
            id,
            n::entity(
                Some(stage.string("vehicle_community")?),
                &[stage.string("vehicle_entry")?],
            ),
            n::entity(None, &[]),
            false,
            true,
            stage.optional("player_role").unwrap_or("Invalid"),
            "OnMount",
            false,
        )
    })?;
    tail = b.append(&tail, node, "In")?;

    tail = b.objective(tail, "success")?;
    tail = b.pin(tail, "Inactive", false)?;

    tail = b.fact(tail, Some(stage.string("completion_fact")?))?;

    b.finish(tail, "success")?;

    b.document()
}
