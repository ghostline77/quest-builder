//! Native resolve and report stage generation.

use super::shared::owned::{Scope, array, s};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Context, Result};
use serde_json::{Value, json};

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut scope = Scope::new(stage)?;
    let data = json!(stage.fields);
    let running = scope.begin()?;

    let mut signals = Vec::new();
    for target in array(&data, "targets")? {
        let policy = &target["resolution"];
        let signal = scope.watch(|g, id| {
            n::character_outcome(
                g,
                id,
                s(&target["community"])?,
                s(&target["entry"])?,
                policy["killed"].as_bool().context("resolution killed")?,
                policy["unconscious"]
                    .as_bool()
                    .context("resolution unconscious")?,
                policy["defeated"]
                    .as_bool()
                    .context("resolution defeated")?,
            )
        })?;
        scope.block.connect(&running, &signal, "In")?;
        signals.push(scope.once(signal)?);
    }
    let resolved = scope.block.join(&signals, true)?;
    let resolved = scope
        .block
        .fact(resolved, Some(stage.string("resolved_fact")?))?;

    let routes = scope.child(resolved, &data["report"], false)?;
    let reported = Some(stage.string("reported_fact")?);

    scope.finish(routes, &[], reported)?;
    scope.block.document()
}
