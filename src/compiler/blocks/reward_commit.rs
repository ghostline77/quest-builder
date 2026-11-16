//! Native reward commit stage generation.

use super::shared::owned::Scope;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut scope = Scope::new(stage)?;
    let data = json!(stage.fields);
    let running = scope.begin()?;
    let mut routes = BTreeMap::new();

    ensure!(
        stage.optional("commit_fact").is_some(),
        "reward_commit requires a durable commit_fact"
    );
    let ready = scope.watch(|g, id| {
        n::fact_condition(g, id, stage.string("reported_fact")?, "Greater", 0, false)
    })?;
    scope.block.connect(&running, &ready, "In")?;
    let test = scope.block.node(|g, id| {
        n::inventory(
            g,
            id,
            stage.string("item")?,
            stage.number("quantity", 1),
            true,
        )
    })?;
    scope.block.connect(&ready, &test, "In")?;
    let granted = scope.actions(
        test.port("False"),
        &[json!({"kind":"item","item":data["item"],"quantity":stage.number("quantity",1)})],
    )?;
    let verified = scope.watch(|g, id| {
        n::inventory(
            g,
            id,
            stage.string("item")?,
            stage.number("quantity", 1),
            false,
        )
    })?;
    scope.block.connect(&granted, &verified, "In")?;
    routes.insert(
        "success".into(),
        scope.block.join(&[test.port("True"), verified], false)?,
    );

    scope.finish(routes, &[], None)?;
    scope.block.document()
}
