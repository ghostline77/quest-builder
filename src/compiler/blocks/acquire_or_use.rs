//! Native acquire or use stage generation.

use super::shared::owned::Scope;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::{Value, json};

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut scope = Scope::new(stage)?;
    let data = json!(stage.fields);
    let running = scope.begin()?;

    let test = scope.block.node(|g, id| {
        n::inventory(
            g,
            id,
            stage.string("item")?,
            stage.number("quantity", 1),
            true,
        )
    })?;
    scope.block.connect(&running, &test, "In")?;
    let existing = scope
        .block
        .fact(test.port("True"), stage.optional("existing_fact"))?;
    let wait = scope.watch(|g, id| {
        n::inventory(
            g,
            id,
            stage.string("item")?,
            stage.number("quantity", 1),
            false,
        )
    })?;
    scope.block.connect(&test.port("False"), &wait, "In")?;
    let acquired = scope.block.fact(wait, stage.optional("acquired_fact"))?;
    let available = scope.block.join(&[existing, acquired], false)?;

    let routes = scope.child(available, &data["application"], false)?;

    scope.finish(routes, &[], None)?;
    scope.block.document()
}
