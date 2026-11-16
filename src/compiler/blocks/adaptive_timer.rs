//! Native adaptive timer stage generation.

use super::shared::owned::{Scope, array, s};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut scope = Scope::new(stage)?;
    let data = json!(stage.fields);
    let running = scope.begin()?;
    let mut routes = BTreeMap::new();

    let mut remaining = running.clone();
    if let Some(fact) = stage.optional("eligibility_fact") {
        let eligible = scope.watch(|g, id| n::fact_condition(g, id, fact, "Greater", 0, false))?;
        remaining = scope.block.append(&remaining, eligible, "In")?;
    }
    let mut profiles = Vec::new();
    for profile in array(&data, "profiles")? {
        let (selected, next) = scope.snapshot(remaining, s(&profile["when_fact"])?)?;
        profiles.push((
            selected,
            profile["choices"]
                .as_array()
                .context("profile choices")?
                .as_slice(),
        ));
        remaining = next;
    }
    profiles.push((remaining, array(&data, "choices")?));
    let mut waits = Vec::new();
    for (selected, choices) in profiles {
        ensure!(
            !choices.is_empty(),
            "adaptive timer choices cannot be empty"
        );
        let names = (1..=choices.len())
            .map(|n| format!("Out{n}"))
            .collect::<Vec<_>>();
        let weights = choices
            .iter()
            .map(|v| v["weight"].clone())
            .collect::<Vec<_>>();
        ensure!(
            weights.iter().all(|v| v.as_f64().is_some_and(|v| v > 0.)),
            "timer weights must be positive"
        );
        let randomizer = scope.block.node(|g, id| {
            g.node(
                id,
                "questRandomizerNodeDefinition",
                &["In"],
                &names.iter().map(String::as_str).collect::<Vec<_>>(),
                json!({"mode":"Random","outputWeights":weights}),
            )
        })?;
        scope.block.connect(&selected, &randomizer, "In")?;
        for (index, choice) in choices.iter().enumerate() {
            let seconds = choice["seconds"]
                .as_u64()
                .context("positive timer seconds")?;
            ensure!(seconds > 0, "timer duration must be positive");
            let timer = scope.watch(|g, id| {
                if stage.string("clock")? == "realtime" {
                    n::real_delay(g, id, seconds, 0)
                } else {
                    n::game_delay(g, id, 0, 0, 0, seconds)
                }
            })?;
            scope
                .block
                .connect(&randomizer.port(&names[index]), &timer, "In")?;
            waits.push(timer);
        }
    }
    let mut ready = scope.block.join(&waits, false)?;
    if let Some(fact) = stage.optional("busy_fact") {
        let (busy, free) = scope.snapshot(ready, fact)?;
        let released = scope.watch(|g, id| n::fact_condition(g, id, fact, "Equal", 0, false))?;
        let timeout = scope
            .watch(|g, id| n::real_delay(g, id, stage.number("max_deferral_seconds", 0), 0))?;
        scope.block.connect(&busy, &released, "In")?;
        scope.block.connect(&busy, &timeout, "In")?;
        ready = scope.block.join(&[free, released, timeout], false)?;
    }
    routes.insert("success".into(), ready);

    scope.finish(routes, &[], None)?;
    scope.block.document()
}
