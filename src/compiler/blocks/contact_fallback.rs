//! Native contact fallback stage generation.

use super::shared::owned::{Scope, array, s};
use crate::manifest::Stage;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut scope = Scope::new(stage)?;
    let data = json!(stage.fields);
    let running = scope.begin()?;
    let mut routes = BTreeMap::new();

    let mut calling = running.clone();
    let mut fallback = Vec::new();
    for fact in array(&data, "unavailable_facts")? {
        let (unavailable, remaining) = scope.snapshot(calling, s(fact)?)?;
        fallback.push(unavailable);
        calling = remaining;
    }
    let mut results = scope.child(calling, &data["call"], true)?;
    let connected = results
        .remove("connected")
        .context("call requires connected exit")?;
    fallback.extend(results.into_values());
    routes.insert("connected".into(), connected);
    routes.insert("fallback".into(), scope.block.join(&fallback, false)?);
    let routes = scope.with_cancel(routes)?;
    for (outcome, signal) in scope.race(routes)? {
        let tail = match outcome.as_str() {
            "fallback" => {
                let signal =
                    scope.actions(signal, &[json!({"kind":"message","path":data["message"]})])?;
                scope
                    .block
                    .fact(signal, Some(stage.string("message_fact")?))?
            }
            "connected" => scope
                .block
                .fact(signal, Some(stage.string("connected_fact")?))?,
            _ => signal,
        };
        let tail = scope.actions(tail, array(&data, "restore")?)?;
        let tail = scope.actions(tail, array(&data["effects"], &outcome)?)?;
        scope.block.finish(tail, &outcome)?;
    }
    scope.block.document()
}
