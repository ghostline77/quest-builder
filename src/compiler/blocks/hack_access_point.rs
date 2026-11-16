//! Native hack access point stage generation.

use super::Block;
use super::shared::bindings::{list, s};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;
    if stage.fields.contains_key("transfer") {
        super::shared::personal_link::populate(&mut b)?;
        return b.document();
    }

    let mut tail = b.enter()?;
    tail = b.pin(tail, "Active", false)?;
    if let Some(community) = stage.optional("guard_community") {
        let guards = b.node(|g, id| n::community(g, id, community, "Activate", None, None))?;
        tail = b.append(&tail, guards, "In")?;
        let ready = b.node(|g, id| n::spawned(g, id, community, None))?;
        tail = b.append(&tail, ready, "In")?;
    }
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
                stage
                    .optional("controller_class")
                    .unwrap_or("AccessPointControllerPS"),
                stage.optional("action").unwrap_or("TogglePersonalLink"),
            )
        })?;
        tail = b.append(&tail, action, "In")?;
    }
    let completed = b.node(|g, id| {
        n::device_condition(
            g,
            id,
            stage.string("device")?,
            stage
                .optional("controller_class")
                .unwrap_or("AccessPointControllerPS"),
            stage
                .optional("completion_function")
                .unwrap_or("WasHackingMinigameSucceeded"),
        )
    })?;
    tail = b.append(&tail, completed, "In")?;

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

    for item in list(stage, "grants")? {
        let node = b.node(|g, id| n::item(g, id, s(item)?, 1, false))?;
        tail = b.append(&tail, node, "In")?;
    }

    tail = b.fact(tail, stage.optional("success_fact"))?;

    b.finish(tail, "success")?;

    b.document()
}
