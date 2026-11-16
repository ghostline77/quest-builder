//! Native access routes stage generation.

use super::shared::owned::{Scope, array, s};
use crate::manifest::Stage;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut scope = Scope::new(stage)?;
    let data = json!(stage.fields);
    let running = scope.begin()?;

    let mut candidates = BTreeMap::new();
    for route in array(&data, "routes")? {
        let id = s(&route["id"])?;
        ensure!(!candidates.contains_key(id), "duplicate access route {id}");
        let success = scope
            .child(running.clone(), route, false)?
            .remove("success")
            .context("access route requires success exit")?;
        candidates.insert(id.to_owned(), success);
    }
    let winners = scope.race(candidates)?;
    let mut signals = Vec::new();
    for (name, signal) in winners {
        signals.push(scope.block.fact(
            signal,
            data["route_facts"].get(&name).and_then(Value::as_str),
        )?);
    }
    let converged = scope.block.join(&signals, false)?;

    let routes = scope.child(converged, &data["application"], false)?;

    scope.finish(routes, &[], None)?;
    scope.block.document()
}
