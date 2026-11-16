//! Meetings that restore a native ambient activity on defer, departure and completion.
use super::{Block, Signal, content};
use crate::{graph::Graph, manifest::Stage, nodes as n};
use anyhow::{Context, Result};
use serde_json::Value;

pub(in crate::compiler) fn attempt_path(stage: &Stage, resume: bool) -> String {
    format!(
        "{}_meeting_{}.questphase",
        stage.phase_resource.trim_end_matches(".questphase"),
        if resume { "resume" } else { "start" }
    )
}

pub(in crate::compiler) fn attempt(stage: &Stage, resume: bool) -> Result<Value> {
    let mut g = Graph::default();
    let start = n::input(&mut g, "In1")?;
    let complete = n::output(&mut g, 1, "Complete")?;
    let deferred = n::output(&mut g, 2, "Deferred")?;
    let away = n::output(&mut g, 3, "Away")?;
    let entry = stage
        .optional(if resume {
            "resume_entry"
        } else {
            "scene_entry"
        })
        .unwrap_or(if resume { "resume" } else { "start" });
    let accept = stage.optional("accept_exit").unwrap_or("job_accept");
    let defer = stage.optional("defer_exit").unwrap_or("end");
    let scene = n::scene(
        &mut g,
        10,
        stage.string("scene")?,
        stage.string("scene_origin")?,
        &[entry],
        &[accept, defer],
    )?;
    let outside = n::trigger(&mut g, 11, stage.string("setup_trigger")?, "IsOutside")?;
    g.connect(start, "Out", scene, entry)?;
    g.connect(start, "Out", outside, "In")?;
    g.connect(scene, accept, complete, "In")?;
    g.connect(scene, defer, deferred, "In")?;
    g.then(outside, away)?;
    g.document(&attempt_path(stage, resume), &[])
}

fn restore(b: &mut Block<'_>, mut source: Signal, spec: &Value) -> Result<Signal> {
    for _ in 0..2 {
        let tick = b.node(|g, id| n::real_delay(g, id, 0, 33))?;
        source = b.append(&source, tick, "In")?;
    }
    content::scene(b, source, &spec["restore_scene"])
}

pub(in crate::compiler::blocks) fn populate(b: &mut Block<'_>) -> Result<()> {
    let stage = b.stage;
    let activity = &stage.fields["activity"];
    let field = |key: &str| {
        activity[key]
            .as_str()
            .with_context(|| format!("meeting activity {key}"))
    };
    let fresh =
        b.node(|g, id| n::fact_condition(g, id, field("completed_fact")?, "Greater", 0, true))?;
    b.connect(&b.start.clone(), &fresh, "In")?;
    b.finish(fresh.port("True"), "success")?;
    let nearby = b.node(|g, id| n::trigger(g, id, field("activation_trigger")?, "IsInside"))?;
    let mut tail = b.append(&fresh.port("False"), nearby, "In")?;
    let activate = b.node(|g, id| {
        n::community(
            g,
            id,
            stage.string("community")?,
            "Activate",
            Some(stage.string("contact")?),
            Some(stage.optional("community_phase").unwrap_or("default")),
        )
    })?;
    tail = b.append(&tail, activate, "In")?;
    let ready = b.node(|g, id| n::spawned(g, id, stage.string("community")?, None))?;
    tail = b.append(&tail, ready, "In")?;
    if let Some(appearance) = stage.optional("appearance") {
        tail = content::appearance(
            b,
            tail,
            stage.string("community")?,
            stage.string("contact")?,
            appearance,
        )?;
    }
    if let Some(group) = stage.optional("attitude_group") {
        tail = content::attitude(
            b,
            tail,
            stage.string("community")?,
            stage.string("contact")?,
            group,
        )?;
    }
    if let Some(tag) = stage.optional("protection_source") {
        tail = content::protect(
            b,
            tail,
            stage.string("community")?,
            stage.string("contact")?,
            tag,
        )?;
    }
    tail = b.objective(tail, "enter")?;
    if let Some(description) = stage.optional("description_entry") {
        let node =
            b.node(|g, id| n::journal(g, id, description, "gameJournalQuestDescription", 2))?;
        tail = b.append(&tail, node, "Active")?;
    }
    tail = b.pin(tail, "Active", false)?;
    if let Some(message) = stage.optional("opening_message") {
        tail = b.message(tail, message)?;
    }
    tail = content::scene(b, tail, &activity["restore_scene"])?;
    let setup = b.node(|g, id| n::trigger(g, id, stage.string("setup_trigger")?, "IsInside"))?;
    tail = b.append(&tail, setup.clone(), "In")?;
    let checkpoint = b.node(|g, id| {
        n::checkpoint(
            g,
            id,
            stage.optional("meeting_checkpoint").unwrap_or(&stage.id),
            false,
        )
    })?;
    tail = b.append(&tail, checkpoint, "In")?;
    let returning =
        b.node(|g, id| n::fact_condition(g, id, field("deferred_fact")?, "Greater", 0, true))?;
    b.connect(&tail, &returning, "In")?;
    let mut complete = Vec::new();
    let mut deferred = Vec::new();
    let mut away = Vec::new();
    for (resume, port) in [(false, "False"), (true, "True")] {
        let child = b.node(|g, id| {
            n::phase(
                g,
                id,
                &attempt_path(stage, resume),
                &["In1"],
                &["Complete", "Deferred", "Away"],
            )
        })?;
        b.connect(&returning.port(port), &child, "In1")?;
        complete.push(child.port("Complete"));
        deferred.push(child.port("Deferred"));
        away.push(child.port("Away"));
    }
    let deferred = b.join(&deferred, false)?;
    let deferred = b.fact(deferred, Some(field("deferred_fact")?))?;
    let outside = b.node(|g, id| n::trigger(g, id, stage.string("rearm_trigger")?, "IsOutside"))?;
    // Subscribe before restoration so a rapid departure/return is retained.
    b.connect(&deferred, &outside, "In")?;
    let restored = restore(b, deferred, activity)?;
    let rearmed = b.join(&[restored, outside], true)?;
    b.connect(&rearmed, &setup, "In")?;
    let away = b.join(&away, false)?;
    let away = b.fact(away, Some(field("deferred_fact")?))?;
    let restored = restore(b, away, activity)?;
    b.connect(&restored, &setup, "In")?;
    let complete = b.join(&complete, false)?;
    tail = restore(b, complete, activity)?;
    tail = b.fact(tail, Some(field("restored_fact")?))?;
    tail = b.objective(tail, "success")?;
    tail = b.pin(tail, "Inactive", false)?;
    let quest = b.node(|g, id| n::objective(g, id, field("quest_path")?, true))?;
    tail = b.append(&tail, quest, "Succeeded")?;
    tail = b.fact(tail, Some(stage.string("accepted_fact")?))?;
    if let Some(item) = stage.optional("reward_item") {
        let grant =
            b.node(|g, id| n::item(g, id, item, stage.number("reward_quantity", 1), false))?;
        tail = b.append(&tail, grant, "In")?;
    }
    tail = b.fact(tail, Some(field("completed_fact")?))?;
    let outside = b.node(|g, id| n::trigger(g, id, field("cleanup_trigger")?, "IsOutside"))?;
    tail = b.append(&tail, outside, "In")?;
    let release =
        b.node(|g, id| n::community(g, id, stage.string("community")?, "Deactivate", None, None))?;
    tail = b.append(&tail, release, "In")?;
    tail = b.fact(tail, Some(field("cleaned_fact")?))?;
    b.finish(tail, "success")
}
