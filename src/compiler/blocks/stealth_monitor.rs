//! Native stealth monitor stage generation.

use super::Block;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let tail = b.enter()?;
    let failed = b.node(|g, id| {
        n::fact_condition(g, id, stage.string("failure_fact")?, "Greater", 0, false)
    })?;
    let stopped =
        b.node(|g, id| n::fact_condition(g, id, stage.string("stop_fact")?, "Greater", 0, false))?;
    b.connect(&tail, &failed, "In")?;
    b.connect(&tail, &stopped, "In")?;
    let mut failures = vec![failed];
    for condition in stage
        .fields
        .get("failure_conditions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let expected = condition
            .get("expected")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let node =
            b.node(|g, id| super::shared::conditions::predicate(g, id, condition, !expected))?;
        b.connect(&tail, &node, "In")?;
        if expected {
            failures.push(node);
        } else {
            failures.push(node.port("False"));
            let poll = b.node(|g, id| n::real_delay(g, id, 0, 100))?;
            b.connect(&node.port("True"), &poll, "In")?;
            b.connect(&poll, &node, "In")?;
        }
    }
    for fact in stage
        .fields
        .get("additional_failure_facts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let fact = fact.as_str().context("failure fact")?;
        let node = b.node(|g, id| n::fact_condition(g, id, fact, "Greater", 0, false))?;
        b.connect(&tail, &node, "In")?;
        failures.push(node);
    }
    let failed = b.join(&failures, false)?;
    let owned = b.created.clone();
    let routes = b.race(
        BTreeMap::from([("failure".into(), failed), ("success".into(), stopped)]),
        &owned,
    )?;
    for (outcome, signal) in routes {
        let mut tail = b.objective(signal, &outcome)?;
        tail = b.fact(
            tail,
            Some(stage.string(if outcome == "success" {
                "success_fact"
            } else {
                "failure_fact"
            })?),
        )?;
        let clear = b.node(|g, id| {
            n::fact(
                g,
                id,
                stage.string(if outcome == "success" {
                    "failure_fact"
                } else {
                    "success_fact"
                })?,
                0,
            )
        })?;
        tail = b.append(&tail, clear, "In")?;
        b.finish(tail, "success")?;
    }

    b.document()
}
