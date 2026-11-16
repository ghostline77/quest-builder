//! Native read terminal document stage generation.

use super::Block;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;
    let mut tail = b.enter()?;
    let power = b.node(|g, id| {
        n::device_action(
            g,
            id,
            stage.string("computer")?,
            "ComputerControllerPS",
            "ForceON",
        )
    })?;
    tail = b.append(&tail, power, "In")?;
    if let Some(path) = stage.optional("document_entry") {
        let node = b.node(|g, id| n::journal(g, id, path, "gameJournalFile", 5))?;
        tail = b.append(&tail, node, "Active")?;
    }
    if let Some(scene) = stage.optional("scene") {
        let exit = stage.optional("output_socket").unwrap_or("end");
        let entry = stage.optional("scene_entry").unwrap_or("start");
        let node = b.node(|g, id| {
            n::scene(
                g,
                id,
                scene,
                stage
                    .optional("scene_origin")
                    .unwrap_or(stage.string("computer")?),
                &[entry],
                &[exit, "Default INT", "Default RET"],
            )
        })?;
        b.connect(&tail, &node, entry)?;
        tail = node.port(exit);
        tail = b.fact(tail, Some(stage.string("completion_fact")?))?;
    } else if let Some(document) = stage.optional("document_entry") {
        let read =
            b.node(|g, id| n::journal_condition(g, id, document, "gameJournalFile", 5, false))?;
        tail = b.append(&tail, read, "In")?;
        tail = b.fact(tail, Some(stage.string("completion_fact")?))?;
    } else {
        let node = b.node(|g, id| {
            n::fact_condition(g, id, stage.string("completion_fact")?, "Greater", 0, false)
        })?;
        tail = b.append(&tail, node, "In")?;
    }
    tail = b.objective(tail, "success")?;
    b.finish(tail, "success")?;

    b.document()
}
