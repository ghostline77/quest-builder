//! Native investigate clues stage generation.

use super::{Block, threshold};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashSet;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;
    populate(&mut b)?;
    b.document()
}

fn populate(b: &mut Block<'_>) -> Result<()> {
    let stage = b.stage;
    let clues = stage.fields["clues"].as_array().context("clues")?;
    let required = usize::try_from(stage.number("required_count", u64::try_from(clues.len())?))?;
    let any = stage
        .optional("scan_order")
        .unwrap_or(if required < clues.len() {
            "any"
        } else {
            "ordered"
        })
        == "any";
    let running = b.enter()?;
    let mut previous = running.clone();
    let mut signals = Vec::new();
    let mut pins = Vec::new();
    for clue in clues {
        let mut tail = if any {
            running.clone()
        } else {
            previous.clone()
        };
        if let Some(pin) = clue.get("mappin").and_then(Value::as_str) {
            pins.push(pin);
            let node = b.node(|g, id| n::mappin(g, id, pin, false))?;
            tail = b.append(&tail, node, "Active")?;
        }
        let reference = clue["object_ref"]
            .as_str()
            .context("clue object reference")?;
        let scan = b.node(|g, id| n::scan(g, id, reference))?;
        tail = b.append(&tail, scan, "In")?;
        tail = b.join(&[tail], false)?;
        if let Some(pin) = clue.get("mappin").and_then(Value::as_str) {
            let node = b.node(|g, id| n::mappin(g, id, pin, false))?;
            tail = b.append(&tail, node, "Inactive")?;
        }
        if let Some(path) = clue.get("journal_entry").and_then(Value::as_str) {
            let node = b.node(|g, id| n::journal(g, id, path, "gameJournalOnscreen", 5))?;
            tail = b.append(&tail, node, "Active")?;
        }
        tail = b.fact(tail, clue.get("completion_fact").and_then(Value::as_str))?;
        for item in clue.get("grant_item").into_iter().chain(
            clue.get("grant_items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
        ) {
            let item = item.as_str().context("clue grant item")?;
            let node = b.node(|g, id| n::item(g, id, item, 1, false))?;
            tail = b.append(&tail, node, "In")?;
        }
        signals.push(tail.clone());
        previous = tail;
    }
    let mut tail = threshold(b, &signals, required)?;
    let mut seen = HashSet::new();
    for pin in pins {
        if seen.insert(pin) {
            let node = b.node(|g, id| n::mappin(g, id, pin, false))?;
            tail = b.append(&tail, node, "Inactive")?;
        }
    }
    tail = b.objective(tail, "success")?;
    tail = b.fact(tail, stage.optional("completion_fact"))?;
    b.finish(tail, "success")
}
