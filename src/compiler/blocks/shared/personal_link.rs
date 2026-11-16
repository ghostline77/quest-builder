//! Timed personal-link transfers, including interruption and dialogue ordering.
use super::{Block, content};
use crate::{manifest::Stage, nodes as n};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub(in crate::compiler) fn attempt_path(stage: &Stage) -> String {
    format!(
        "{}_attempt.questphase",
        stage.phase_resource.trim_end_matches(".questphase")
    )
}

pub(in crate::compiler) fn attempt(stage: &Stage) -> Result<Value> {
    let transfer = &stage.fields["transfer"];
    let duration = transfer["duration_seconds"]
        .as_u64()
        .context("transfer duration")?;
    ensure!(duration > 0, "transfer duration must be positive");
    let mut spec = json!({"kind":"personal_link_attempt","depot":attempt_path(stage),"device":stage.string("device")?,"label":transfer["label"],"duration_seconds":duration});
    for key in ["connected_scene", "bottom_text"] {
        if let Some(value) = transfer.get(key) {
            spec[key] = value.clone();
        }
    }
    crate::lifecycle::compile(&spec)
}

pub(in crate::compiler::blocks) fn populate(b: &mut Block<'_>) -> Result<()> {
    let stage = b.stage;
    let transfer = &stage.fields["transfer"];
    let device = stage.string("device")?;
    let controller = stage
        .optional("controller_class")
        .unwrap_or("AccessPointControllerPS");
    let mut tail = b.enter()?;
    tail = b.pin(tail, "Active", true)?;
    let parallel = transfer
        .get("intro_parallel")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let intro = if let Some(spec) = transfer.get("intro_scene") {
        Some(content::scene(b, tail.clone(), spec)?)
    } else {
        None
    };
    if !parallel && let Some(intro) = &intro {
        tail = intro.clone();
    }
    let enable = b.node(|g, id| n::device_action(g, id, device, controller, "ForceON"))?;
    tail = b.append(&tail, enable, "In")?;
    let child =
        b.node(|g, id| n::phase(g, id, &attempt_path(stage), &["In1"], &["Out1", "Retry"]))?;
    b.connect(&tail, &child, "In1")?;
    let mut retry = child.port("Retry");
    if let Some(fact) = transfer.get("interrupted_fact").and_then(Value::as_str) {
        retry = b.fact(retry, Some(fact))?;
    }
    if parallel && let Some(intro) = &intro {
        let fact = format!(
            "qb_{}_{}_briefed",
            stage.phase_resource.split('\\').nth(1).unwrap_or("quest"),
            stage.id
        );
        b.fact(intro.clone(), Some(&fact))?;
        let heard = b.node(|g, id| n::fact_condition(g, id, &fact, "Greater", 0, false))?;
        retry = b.append(&retry, heard, "In")?;
    }
    if let Some(spec) = transfer.get("retry_scene") {
        retry = content::scene(b, retry, spec)?;
    }
    b.connect(&retry, &child, "In1")?;
    // The attempt has already committed success while connected. A disconnect
    // after the transfer completes must not block subsequent quest progression.
    tail = child.port("Out1");
    tail = b.fact(tail, stage.optional("success_fact"))?;
    if let Some(effects) = transfer.get("completion_effects") {
        content::effects(b, &tail, effects)?;
    }
    // Terminating the attempt removes its disconnect listener before we unplug.
    for _ in 0..2 {
        let tick = b.node(|g, id| n::real_delay(g, id, 0, 33))?;
        tail = b.append(&tail, tick, "In")?;
    }
    for action in ["QuestForceDisconnectPersonalLink", "ForceDisabled"] {
        let node = b.node(|g, id| n::device_action(g, id, device, controller, action))?;
        tail = b.append(&tail, node, "In")?;
    }
    for item in stage
        .fields
        .get("grants")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let node = b.node(|g, id| {
            n::item(
                g,
                id,
                item.as_str().context("transfer item grant")?,
                1,
                false,
            )
        })?;
        tail = b.append(&tail, node, "In")?;
    }
    tail = b.objective(tail, "success")?;
    tail = b.pin(tail, "Inactive", false)?;
    if parallel && let Some(intro) = intro {
        tail = b.join(&[tail, intro], true)?;
    }
    if let Some(spec) = transfer.get("success_scene") {
        tail = content::scene(b, tail, spec)?;
    }
    b.finish(tail, "success")
}
