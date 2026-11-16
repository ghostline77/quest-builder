//! Native braindance analysis stage generation.

use super::{Block, fractional_delay};
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
    let mut tail = b.enter()?;
    let park = b.node(|g, id| n::teleport_player(g, id, stage.string("player_anchor")?, true))?;
    tail = b.append(&tail, park, "In")?;
    let hide = b.node(|g, id| n::player_visibility(g, id, false))?;
    let hidden = b.append(&tail, hide, "In")?;
    let exits = stage
        .fields
        .get("scene_exit_sockets")
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .map(|v| v.as_str().context("scene exit"))
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or(vec!["end", "complete", "Default INT", "Default RET"]);
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
    b.connect(&hidden, &scene, entry)?;
    let delay = fractional_delay(
        b,
        stage
            .fields
            .get("exit_enable_seconds")
            .and_then(Value::as_f64)
            .unwrap_or(0.5),
    )?;
    let delay = b.append(&hidden, delay, "In")?;
    let enable = b.node(|g, id| {
        let kind = g.wrap(json!({"$type":"questEnableBraindanceFinish_NodeType"}));
        g.node(
            id,
            "questUIManagerNodeDefinition",
            &["In"],
            &["Out"],
            json!({"type":kind}),
        )
    })?;
    b.connect(&delay, &enable, "In")?;
    let clues = stage.fields["clue_facts"]
        .as_array()
        .context("braindance clue facts")?;
    ensure!(!clues.is_empty(), "braindance requires clues");
    let mut counters = Vec::new();
    for fact in clues {
        let name = fact.as_str().context("braindance clue fact")?;
        let clue = b.node(|g, id| n::fact_condition(g, id, name, "Greater", 0, false))?;
        b.connect(&hidden, &clue, "In")?;
        let counter = b.node(|g, id| n::objective_counter(g, id, stage.string("objective")?))?;
        counters.push(b.append(&clue, counter, "Increment")?);
    }
    let completed = b.join(&counters, true)?;
    let completed = b.objective(completed, "success")?;
    b.fact(completed, Some(stage.string("completion_fact")?))?;
    let teardown = fractional_delay(
        b,
        stage
            .fields
            .get("teardown_seconds")
            .and_then(Value::as_f64)
            .unwrap_or(1.5),
    )?;
    for exit in exits {
        b.connect(&scene.port(exit), &teardown, "In")?;
    }
    let restore =
        b.node(|g, id| n::teleport_player(g, id, stage.string("player_return")?, false))?;
    tail = b.append(&teardown, restore, "In")?;
    let show = b.node(|g, id| n::player_visibility(g, id, true))?;
    tail = b.append(&tail, show, "In")?;
    let settle = fractional_delay(
        b,
        stage
            .fields
            .get("settle_seconds")
            .and_then(Value::as_f64)
            .unwrap_or(0.05),
    )?;
    tail = b.append(&tail, settle, "In")?;
    b.finish(tail, "success")
}
