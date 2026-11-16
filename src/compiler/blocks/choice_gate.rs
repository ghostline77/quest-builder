//! Native choice gate stage generation.

use super::Block;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;
    populate(&mut b)?;
    b.document()
}

fn populate(b: &mut Block<'_>) -> Result<()> {
    let stage = b.stage;
    let branches = stage.fields["branches"]
        .as_array()
        .context("choice branches")?;
    ensure!(!branches.is_empty(), "choice requires branches");
    ensure!(
        stage.flag("join"),
        "choice gate requires an authored joined route"
    );
    if stage.optional("gate_kind") == Some("scene_exit") {
        let exits = branches
            .iter()
            .map(|branch| branch["condition"].as_str().context("scene exit name"))
            .collect::<Result<Vec<_>>>()?;
        let entry = stage.optional("scene_entry").unwrap_or("start");
        let scene = b.node(|g, id| {
            n::scene(
                g,
                id,
                stage.string("scene")?,
                stage.string("scene_origin")?,
                &[entry],
                &exits,
            )
        })?;
        b.connect(&b.start.clone(), &scene, entry)?;
        for branch in branches {
            let tail = b.fact(
                scene.port(branch["condition"].as_str().context("scene exit")?),
                branch["set_fact"].as_str(),
            )?;
            let name = branch["id"].as_str().context("branch ID")?;
            b.finish(
                tail,
                if b.outputs.contains_key(name) {
                    name
                } else {
                    "success"
                },
            )?;
        }
        return Ok(());
    }
    let fallback = stage.optional("default_branch");
    let snapshot = stage
        .optional("evaluation")
        .unwrap_or(if fallback.is_some() {
            "on_entry"
        } else {
            "wait"
        })
        == "on_entry";
    let probe = |g: &mut crate::graph::Graph,
                 id: u32,
                 binding: &str,
                 snapshot: bool|
     -> Result<crate::graph::NodeId> {
        if stage.optional("gate_kind") == Some("phone_choice") {
            let node =
                n::journal_condition(g, id, binding, "gameJournalPhoneChoiceEntry", 1, true)?;
            if snapshot {
                g.snapshot(node)?;
            }
            Ok(node)
        } else {
            super::shared::conditions::predicate(
                g,
                id,
                &json!({"kind":"fact","value":binding}),
                snapshot,
            )
        }
    };
    let mut previous = b.start.clone();
    if !snapshot {
        let mut probes = Vec::new();
        for branch in branches {
            let fact = branch["condition"].as_str().context("choice fact")?;
            let node = b.node(|g, id| probe(g, id, fact, false))?;
            b.connect(&previous, &node, "In")?;
            probes.push(node);
        }
        previous = b.join(&probes, false)?;
    }
    for branch in branches {
        let fact = branch["condition"].as_str().context("choice fact")?;
        let test = b.node(|g, id| probe(g, id, fact, true))?;
        b.connect(&previous, &test, "In")?;
        let tail = b.fact(
            test.port("True"),
            branch.get("set_fact").and_then(Value::as_str),
        )?;
        let outcome = branch["id"].as_str().context("choice branch id")?;
        b.finish(
            tail,
            if b.outputs.contains_key(outcome) {
                outcome
            } else {
                "success"
            },
        )?;
        previous = test.port("False");
    }
    if let Some(fallback) = fallback {
        let branch = branches
            .iter()
            .find(|v| v["id"] == fallback)
            .context("unknown default branch")?;
        let tail = b.fact(previous, branch.get("set_fact").and_then(Value::as_str))?;
        b.finish(
            tail,
            if b.outputs.contains_key(fallback) {
                fallback
            } else {
                "success"
            },
        )?;
    }
    Ok(())
}
