//! Native investigate reactions stage generation.

use super::shared::owned::{Scope, array, s};
use super::threshold;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut scope = Scope::new(stage)?;
    let data = json!(stage.fields);
    let running = scope.begin()?;
    let mut routes = BTreeMap::new();
    let mut restore = Vec::new();

    let mut completed = Vec::new();
    let mut pins = Vec::new();
    for clue in array(&data, "clues")? {
        let (done, mut remaining) =
            scope.snapshot(running.clone(), s(&clue["completion_fact"])?)?;
        if let Some(pin) = clue.get("mappin").and_then(Value::as_str) {
            if !pins.contains(&pin) {
                pins.push(pin);
            }
            let node = scope.block.node(|g, id| n::mappin(g, id, pin, false))?;
            remaining = scope.block.append(&remaining, node, "Active")?;
        }
        let (scanned, waiting) = scope.snapshot(remaining, s(&clue["scan_fact"])?)?;
        let scan = scope.watch(|g, id| n::scan(g, id, s(&clue["object_ref"])?))?;
        scope.block.connect(&waiting, &scan, "In")?;
        let scan = scope.once(scan)?;
        let scan = scope.block.fact(scan, Some(s(&clue["scan_fact"])?))?;
        let available = scope.block.join(&[scanned, scan], false)?;
        let reacted = if let Some(reaction) = clue.get("reaction") {
            scope
                .child(available, reaction, true)?
                .remove("success")
                .context("clue reaction success exit")?
        } else {
            available
        };
        let reacted = scope
            .block
            .fact(reacted, Some(s(&clue["completion_fact"])?))?;
        completed.push(scope.block.join(&[done, reacted], false)?);
    }
    let mut success = threshold(
        &mut scope.block,
        &completed,
        usize::try_from(stage.number("required_count", u64::try_from(completed.len())?))?,
    )?;
    if let Some(fact) = stage.optional("discovery_fact") {
        let discovery = scope.watch(|g, id| n::fact_condition(g, id, fact, "Greater", 0, false))?;
        scope.block.connect(&running, &discovery, "In")?;
        success = scope.block.join(&[success, discovery], false)?;
    }
    for pin in pins {
        restore.push(json!({"kind":"mappin","path":pin,"state":"Inactive"}));
    }
    routes.insert("success".into(), success);

    scope.finish(routes, &restore, None)?;
    scope.block.document()
}
