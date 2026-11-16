//! Native leave area stage generation.

use super::Block;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let mut tail = b.enter()?;
    tail = b.pin(tail, "Active", false)?;

    let node = b.node(|g, id| n::trigger(g, id, stage.string("trigger")?, "IsOutside"))?;
    tail = b.append(&tail, node, "In")?;
    tail = b.objective(tail, "success")?;
    tail = b.pin(tail, "Inactive", false)?;

    tail = b.fact(tail, stage.optional("completion_fact"))?;
    if let Some(community) = stage.optional("cleanup_community") {
        let target = b.node(|g, id| n::community(g, id, community, "Deactivate", None, None))?;
        tail = b.append(&tail, target, "In")?;
    }

    b.finish(tail, "success")?;

    b.document()
}
