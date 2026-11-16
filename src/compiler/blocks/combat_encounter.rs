//! Native combat encounter stage generation.

use super::shared::bindings::{list, s};
use super::shared::owned::Scope;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut scope = Scope::new(stage)?;
    let mut tail = scope.begin()?;
    let activation = tail.clone();
    let b = &mut scope.block;
    if let Some(trigger) = stage.optional("trigger") {
        let node = b.node(|g, id| n::trigger(g, id, trigger, "Entered"))?;
        tail = b.append(&tail, node, "In")?;
    }
    tail = b.objective(tail, "enter")?;
    if let Some(path) = stage.optional("description_entry") {
        let node = b.node(|g, id| n::journal(g, id, path, "gameJournalQuestDescription", 2))?;
        tail = b.append(&tail, node, "Active")?;
    }
    if stage.fields.get("activate") != Some(&Value::Bool(false)) {
        let active = b.node(|g, id| {
            n::community(g, id, stage.string("community")?, "Activate", None, None)
        })?;
        tail = b.append(&tail, active, "In")?;
    }
    let spawned = b.node(|g, id| n::spawned(g, id, stage.string("community")?, None))?;
    scope.owned.push(spawned.node);
    tail = b.append(&tail, spawned, "In")?;
    for entry in list(stage, "entries")? {
        let node = b
            .node(|g, id| {
                n::combat_threat(
                    g,
                    id,
                    stage.string("community")?,
                    s(entry)?,
                    stage.optional("threat_function"),
                    stage
                        .fields
                        .get("threat_duration_seconds")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.5),
                )
            })?
            .port("Success");
        b.g.set_nested(
            node.node,
            &["params", "dontForceHostileAttitude"],
            json!(u8::from(
                stage.optional("hostility") == Some("already_hostile")
            )),
        )?;
        scope.owned.push(node.node);
        tail = b.append(&tail, node, "In")?;
    }
    let nonlethal = stage.fields.get("nonlethal_allowed") != Some(&Value::Bool(false));
    match stage.optional("completion").unwrap_or("all_defeated") {
        "fact" => {
            let condition = b.node(|g, id| {
                n::fact_condition(g, id, stage.string("completion_fact")?, "Greater", 0, false)
            })?;
            scope.owned.push(condition.node);
            tail = b.append(&tail, condition, "In")?;
        }
        "named_defeated" => {
            let mut outcomes = Vec::new();
            for entry in list(stage, "entries")? {
                let resolved = b.node(|g, id| {
                    n::character_outcome(
                        g,
                        id,
                        stage.string("community")?,
                        s(entry)?,
                        true,
                        nonlethal,
                        nonlethal,
                    )
                })?;
                scope.owned.push(resolved.node);
                b.connect(&tail, &resolved, "In")?;
                outcomes.push(resolved);
            }
            tail = b.join(&outcomes, true)?;
        }
        "all_defeated" => {
            let defeated = b.node(|g, id| {
                n::community_defeated(g, id, stage.string("community")?, nonlethal)
            })?;
            scope.owned.push(defeated.node);
            tail = b.append(&tail, defeated, "In")?;
        }
        mode => anyhow::bail!("unknown combat completion {mode}"),
    }
    tail = b.objective(tail, "success")?;
    tail = b.fact(tail, stage.optional("completion_fact"))?;
    if stage.flag("cleanup_on_exit") {
        let node = b.node(|g, id| {
            n::community(g, id, stage.string("community")?, "Deactivate", None, None)
        })?;
        tail = b.append(&tail, node, "In")?;
    }
    let mut routes = BTreeMap::from([("success".into(), tail)]);
    if let Some(trigger) = stage.optional("abort_trigger") {
        let outside = scope.watch(|g, id| n::trigger(g, id, trigger, "IsOutside"))?;
        scope.block.connect(&activation, &outside, "In")?;
        routes.insert("interrupted".into(), outside);
    }
    scope.finish(routes, &[], None)?;
    scope.block.document()
}
