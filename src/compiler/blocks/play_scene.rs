//! Play a generated scene, optionally after entering a placed trigger.
use super::{Block, shared::content};
use crate::{manifest::Stage, nodes as n};
use anyhow::{Context, Result, ensure};
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;
    let mut tail = b.start.clone();
    if let Some(trigger) = stage.optional("wait_trigger") {
        let wait = b.node(|g, id| n::trigger(g, id, trigger, "IsInside"))?;
        tail = b.append(&tail, wait, "In")?;
    }
    let spawners = stage
        .fields
        .get("spawners")
        .map(|value| {
            value
                .as_array()
                .context("NPC spawners must be an array")?
                .iter()
                .map(|reference| reference.as_str().context("NPC spawner reference"))
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    ensure!(
        !stage.flag("despawn_after") || !spawners.is_empty(),
        "despawn_after requires NPC spawners"
    );
    for spawner in &spawners {
        let spawn = b.node(|g, id| n::population(g, id, spawner, "Activate"))?;
        tail = b.append(&tail, spawn, "In")?;
        let ready = b.node(|g, id| n::population_npc_present(g, id, spawner, true))?;
        tail = b.append(&tail, ready, "In")?;
    }
    tail = content::scene(
        &mut b,
        tail,
        &serde_json::json!({
            "scene":stage.string("scene")?, "origin":stage.string("origin")?,
            "entry":stage.optional("entry").unwrap_or("start"),
            "exit":stage.optional("exit").unwrap_or("end")
        }),
    )?;
    if stage.flag("despawn_after") {
        for spawner in &spawners {
            let despawn = b.node(|g, id| n::population(g, id, spawner, "Deactivate"))?;
            tail = b.append(&tail, despawn, "In")?;
            let absent = b.node(|g, id| n::population_npc_present(g, id, spawner, false))?;
            tail = b.append(&tail, absent, "In")?;
        }
    }
    tail = b.fact(tail, stage.optional("completion_fact"))?;
    b.finish(tail, "success")?;
    b.document()
}
