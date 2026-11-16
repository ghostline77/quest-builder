//! Native entitlement reconcile stage generation.

use super::shared::owned::{Scope, array};
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

    let (owned, absent) = if let Some(record) = stage.optional("vehicle_record") {
        let test = scope
            .block
            .node(|g, id| n::vehicle_available(g, id, record))?;
        scope.block.connect(&running, &test, "In")?;
        (test.port("True"), test.port("False"))
    } else {
        scope.snapshot(running.clone(), stage.string("entitled_fact")?)?
    };
    routes.insert("owned".into(), owned);
    routes.insert("unowned".into(), absent);
    for (outcome, signal) in scope.race(routes)? {
        let mut tail = signal;
        if outcome == "owned" {
            tail = scope.block.fact(tail, Some(stage.string("owned_fact")?))?;
            tail = scope.actions(tail, array(&data, "retire")?)?;
        }
        tail = scope.actions(tail, array(&data, "restore")?)?;
        tail = scope.actions(tail, array(&data["effects"], &outcome)?)?;
        scope.block.finish(tail, &outcome)?;
    }
    scope.block.document()
}
