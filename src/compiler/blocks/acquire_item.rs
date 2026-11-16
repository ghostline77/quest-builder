//! Native acquire item stage generation.

use super::Block;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Result, ensure};
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let mut tail = b.enter()?;
    tail = b.pin(tail, "Active", false)?;
    let quantity = stage.number("quantity", 1);
    ensure!(quantity > 0, "item quantity must be positive");
    if stage.string("source")? == "grant" {
        let node = b.node(|g, id| n::item(g, id, stage.string("item")?, quantity, false))?;
        tail = b.append(&tail, node, "In")?;
    }
    let node = b.node(|g, id| n::inventory(g, id, stage.string("item")?, quantity, false))?;
    tail = b.append(&tail, node, "In")?;
    tail = b.objective(tail, "success")?;
    tail = b.pin(tail, "Inactive", false)?;
    tail = b.fact(tail, stage.optional("acquisition_fact"))?;
    b.finish(tail, "success")?;

    b.document()
}
