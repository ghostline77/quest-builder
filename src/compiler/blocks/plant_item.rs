//! Native plant item stage generation.

use super::Block;
use super::shared::bindings::{list, s};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::{Value, json};

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let mut tail = b.enter()?;
    tail = b.pin(tail, "Active", false)?;

    if stage
        .fields
        .get("send_action")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        let action = b.node(|g, id| {
            n::device_action(
                g,
                id,
                stage.string("device")?,
                stage.string("controller_class")?,
                stage.string("action")?,
            )
        })?;
        tail = b.append(&tail, action, "In")?;
    }
    let completed = b.node(|g, id| {
        n::device_condition(
            g,
            id,
            stage.string("device")?,
            stage.string("controller_class")?,
            stage.string("completion_function")?,
        )
    })?;
    tail = b.append(&tail, completed, "In")?;
    if let Some(progress) = stage.fields.get("progress") {
        let seconds = progress["duration_seconds"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("installation duration"))?;
        let show=b.node(|g,id|{
            let kind=g.wrap(json!({"$type":"questProgressBar_NodeType","duration":seconds,"show":1,"text":{"unk1":"0","value":progress["label"]},"bottomText":{"unk1":"0","value":progress["bottom_text"]},"type":"Connection"}));
            g.node(id,"questUIManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))
        })?;
        tail = b.append(&tail, show, "In")?;
        let elapsed = b.node(|g, id| n::real_delay(g, id, seconds, 0))?;
        tail = b.append(&tail, elapsed, "In")?;
    }
    if let Some(action) = stage.optional("completion_action") {
        let node = b.node(|g, id| {
            n::device_action(
                g,
                id,
                stage.string("device")?,
                stage.string("controller_class")?,
                action,
            )
        })?;
        tail = b.append(&tail, node, "In")?;
    }

    let node = if stage
        .fields
        .get("consume_item")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        b.node(|g, id| n::item(g, id, stage.string("item")?, 1, true))?
    } else {
        b.node(|g, id| n::inventory(g, id, stage.string("item")?, 1, false))?
    };
    tail = b.append(&tail, node, "In")?;

    let branches = list(stage, "outcome_branches")?;
    if !branches.is_empty() {
        let mut tails = Vec::new();
        for branch in branches {
            let condition = b.node(|g, id| {
                n::fact_condition(g, id, s(&branch["condition"])?, "Greater", 0, false)
            })?;
            let mut path = b.append(&tail, condition, "In")?;
            for (key, remove) in [("remove_items", true), ("add_items", false)] {
                for item in branch
                    .get(key)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let node = b.node(|g, id| n::item(g, id, s(item)?, 1, remove))?;
                    path = b.append(&path, node, "In")?;
                }
            }
            tails.push(b.fact(path, Some(s(&branch["set_fact"])?))?);
        }
        tail = b.join(&tails, false)?;
    }
    tail = b.objective(tail, "success")?;
    tail = b.pin(tail, "Inactive", false)?;

    tail = b.fact(tail, stage.optional("success_fact"))?;

    tail = b.fact(tail, Some(stage.string("completion_fact")?))?;

    b.finish(tail, "success")?;

    b.document()
}
