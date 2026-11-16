//! Native observe lethal stage generation.

use super::shared::owned::{Scope, array, s};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut scope = Scope::new(stage)?;
    let data = json!(stage.fields);
    let running = scope.begin()?;
    let mut routes = BTreeMap::new();

    let (closed, observing) = scope.snapshot(running.clone(), stage.string("stop_fact")?)?;
    for target in array(&data, "targets")? {
        let killed = scope.watch(|g, id| {
            n::character_outcome(
                g,
                id,
                s(&target["community"])?,
                s(&target["entry"])?,
                true,
                false,
                false,
            )
        })?;
        scope.block.connect(&observing, &killed, "In")?;
        let killed = scope.once(killed)?;
        scope.block.fact(killed, Some(s(&target["killed_fact"])?))?;
    }
    let stop = scope
        .watch(|g, id| n::fact_condition(g, id, stage.string("stop_fact")?, "Greater", 0, false))?;
    scope.block.connect(&observing, &stop, "In")?;
    routes.insert("success".into(), scope.block.join(&[closed, stop], false)?);

    scope.finish(routes, &[], None)?;
    scope.block.document()
}
