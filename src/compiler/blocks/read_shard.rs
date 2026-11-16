//! Native read shard stage generation.

use super::Block;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let mut tail = b.enter()?;
    if stage
        .fields
        .get("activate_entry")
        .and_then(Value::as_bool)
        .unwrap_or(stage.flag("grant_item"))
    {
        let node = b.node(|g, id| {
            n::journal(
                g,
                id,
                stage.string("journal_entry")?,
                "gameJournalOnscreen",
                u32::try_from(stage.number("file_entry_index", 0))?,
            )
        })?;
        tail = b.append(&tail, node, "Active")?;
    }
    if stage.flag("grant_item") {
        let grant = b.node(|g, id| n::item(g, id, stage.string("item")?, 1, false))?;
        tail = b.append(&tail, grant, "In")?;
    }
    if let Some(fact) = stage.optional("acquisition_fact") {
        let node = b.node(|g, id| n::fact_condition(g, id, fact, "Greater", 0, false))?;
        tail = b.append(&tail, node, "In")?;
    }
    if stage.number("presentation_delay_seconds", 0) > 0 {
        let node =
            b.node(|g, id| n::real_delay(g, id, stage.number("presentation_delay_seconds", 0), 0))?;
        tail = b.append(&tail, node, "In")?;
    }
    let read = b.node(|g, id| {
        n::journal_condition(
            g,
            id,
            stage.string("journal_entry")?,
            "gameJournalOnscreen",
            u32::try_from(stage.number("file_entry_index", 0))?,
            false,
        )
    })?;
    tail = b.append(&tail, read, "In")?;
    tail = b.objective(tail, "success")?;
    tail = b.fact(tail, stage.optional("completion_fact"))?;
    if let Some(scene) = stage.fields.get("reaction_scene") {
        tail = super::shared::content::scene(&mut b, tail, scene)?;
    }
    b.finish(tail, "success")?;

    b.document()
}
