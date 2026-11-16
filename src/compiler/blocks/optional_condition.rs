//! Native optional condition stage generation.

use super::{Block, shared::conditions::predicate};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let mut start = b.enter()?;
    let spec = &stage.fields["condition"];
    let mut predicate_spec = spec.clone();
    if stage.optional("evaluation") == Some("at_exit") {
        let stop = spec.get("stop_fact").and_then(Value::as_str).context(
            "at_exit requires condition.stop_fact to identify the end of the evaluation window",
        )?;
        if spec["kind"] == "timer" {
            let elapsed = format!(
                "{}_{}_timer_elapsed",
                stage.phase_resource.split('\\').nth(1).unwrap_or("quest"),
                stage.id
            );
            let clear = b.node(|g, id| n::fact(g, id, &elapsed, 0))?;
            start = b.append(&start, clear, "In")?;
            let timer = b.node(|g, id| predicate(g, id, spec, false))?;
            b.connect(&start, &timer, "In")?;
            b.fact(timer, Some(&elapsed))?;
            predicate_spec = json!({"kind":"fact","value":elapsed});
        }
        let closed = b.node(|g, id| n::fact_condition(g, id, stop, "Greater", 0, false))?;
        start = b.append(&start, closed, "In")?;
    }
    let continuous = stage.optional("evaluation") == Some("continuous");
    let expected = spec
        .get("expected")
        .and_then(Value::as_bool)
        .or_else(|| spec["value"].as_bool())
        .unwrap_or(true);
    let probe = b.node(|g, id| predicate(g, id, &predicate_spec, !continuous || !expected))?;
    b.connect(&start, &probe, "In")?;
    let mut owned = vec![probe.node];
    let (yes, mut no) = if continuous && expected {
        (probe.clone(), None)
    } else if expected {
        (probe.port("True"), Some(probe.port("False")))
    } else {
        (
            probe.port("False"),
            if continuous {
                None
            } else {
                Some(probe.port("True"))
            },
        )
    };
    if continuous && !expected {
        let retry = b.node(|g, id| n::real_delay(g, id, 0, 100))?;
        b.connect(&probe.port("True"), &retry, "In")?;
        b.connect(&retry, &probe, "In")?;
        owned.push(retry.node);
    }
    if continuous && let Some(stop) = spec.get("stop_fact").and_then(Value::as_str) {
        let stopped = b.node(|g, id| n::fact_condition(g, id, stop, "Greater", 0, false))?;
        b.connect(&start, &stopped, "In")?;
        owned.push(stopped.node);
        no = Some(stopped);
    }
    let mut routes = BTreeMap::from([("success".into(), yes)]);
    if let Some(no) = no {
        routes.insert("failure".into(), no);
    }
    for (outcome, signal) in b.race(routes, &owned)? {
        let passed = outcome == "success";
        let tail = b.fact(
            signal,
            Some(stage.string(if passed {
                "success_fact"
            } else {
                "failure_fact"
            })?),
        )?;
        let clear = b.node(|g, id| {
            n::fact(
                g,
                id,
                stage.string(if passed {
                    "failure_fact"
                } else {
                    "success_fact"
                })?,
                0,
            )
        })?;
        let tail = b.append(&tail, clear, "In")?;
        let tail = b.objective(tail, if passed { "success" } else { "failure" })?;
        b.finish(tail, "success")?;
    }

    b.document()
}
