//! Native deliver drop point stage generation.

use super::Block;
use super::shared::bindings::{list, s};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let mut tail = b.enter()?;
    tail = b.pin(tail, "Active", !list(stage, "item_branches")?.is_empty())?;
    let branches = list(stage, "item_branches")?;
    let branches = if branches.is_empty() {
        vec![serde_json::json!({"item":stage.string("item")?})]
    } else {
        branches.to_vec()
    };
    let mut deposits = Vec::new();
    for branch in branches {
        let mut source = tail.clone();
        if let Some(fact) = branch.get("condition").and_then(Value::as_str) {
            let node = b.node(|g, id| n::fact_condition(g, id, fact, "Greater", 0, false))?;
            source = b.append(&source, node, "In")?;
        }
        let inventory = b.node(|g, id| n::inventory(g, id, s(&branch["item"])?, 1, false))?;
        source = b.append(&source, inventory, "In")?;
        let reserve = b.node(|g, id| {
            n::reserve_drop_point(g, id, s(&branch["item"])?, stage.string("drop_point")?)
        })?;
        b.connect(&source, &reserve, "In")?;
        let deposited = b.node(|g, id| {
            n::fact_condition(g, id, stage.string("deposit_fact")?, "Greater", 0, false)
        })?;
        deposits.push(b.append(&source, deposited, "In")?);
    }
    tail = b.join(&deposits, false)?;
    tail = b.objective(tail, "success")?;
    tail = b.pin(tail, "Inactive", false)?;
    b.finish(tail, "success")?;

    b.document()
}
