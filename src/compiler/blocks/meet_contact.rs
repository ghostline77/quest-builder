//! Native meet contact stage generation.

use super::Block;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;
    if stage.fields.contains_key("activity") {
        super::shared::meeting_activity::populate(&mut b)?;
        return b.document();
    }
    populate(&mut b)?;
    b.document()
}

fn populate(b: &mut Block<'_>) -> Result<()> {
    let stage = b.stage;
    let mut start = b.enter()?;
    start = b.pin(start, "Active", false)?;
    if let Some(message) = stage.optional("opening_message") {
        start = b.message(start, message)?;
    }
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
    let mut tail = b.append(&start, activate, "In")?;
    let ready = b.node(|g, id| n::spawned(g, id, stage.string("community")?, None))?;
    tail = b.append(&tail, ready, "In")?;
    if let Some(appearance) = stage.optional("appearance") {
        tail = super::shared::content::appearance(
            b,
            tail,
            stage.string("community")?,
            stage.string("contact")?,
            appearance,
        )?;
    }
    if let Some(group) = stage.optional("attitude_group") {
        tail = super::shared::content::attitude(
            b,
            tail,
            stage.string("community")?,
            stage.string("contact")?,
            group,
        )?;
    }
    if let Some(tag) = stage.optional("protection_source") {
        tail = super::shared::content::protect(
            b,
            tail,
            stage.string("community")?,
            stage.string("contact")?,
            tag,
        )?;
    }
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
    let accept = stage.optional("accept_exit").unwrap_or("job_accept");
    let defer = stage.optional("defer_exit").unwrap_or("end");
    let entry = stage.optional("scene_entry").unwrap_or("start");
    let allow_defer = stage.fields.get("allow_defer") != Some(&Value::Bool(false));
    let mut exits = vec![accept];
    for exit in [defer, "Default INT", "Default RET"] {
        if (exit != defer || allow_defer) && !exits.contains(&exit) {
            exits.push(exit);
        }
    }
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
    let (accepted, deferred) = if let Some(fact) = stage.optional("deferred_fact") {
        let branch = b.node(|g, id| n::fact_condition(g, id, fact, "Greater", 0, true))?;
        b.connect(&tail, &branch, "In")?;
        b.connect(&branch.port("False"), &scene, entry)?;
        let resume = stage.optional("resume_entry").unwrap_or("resume");
        let resumed = b.node(|g, id| {
            n::scene(
                g,
                id,
                stage.string("scene")?,
                stage.string("scene_origin")?,
                &[resume],
                &exits,
            )
        })?;
        b.connect(&branch.port("True"), &resumed, resume)?;
        let accepted = b.join(&[scene.port(accept), resumed.port(accept)], false)?;
        let deferred = if allow_defer {
            let deferred = b.join(&[scene.port(defer), resumed.port(defer)], false)?;
            Some(b.fact(deferred, Some(fact))?)
        } else {
            None
        };
        (accepted, deferred)
    } else {
        b.connect(&tail, &scene, entry)?;
        (scene.port(accept), allow_defer.then(|| scene.port(defer)))
    };
    let mut tail = b.objective(accepted, "success")?;
    tail = b.pin(tail, "Inactive", false)?;
    tail = b.fact(tail, Some(stage.string("accepted_fact")?))?;
    if let Some(item) = stage.optional("reward_item") {
        let grant =
            b.node(|g, id| n::item(g, id, item, stage.number("reward_quantity", 1), false))?;
        tail = b.append(&tail, grant, "In")?;
    }
    b.finish(tail, "success")?;
    if let Some(deferred) = deferred {
        if stage.optional("defer_policy") == Some("rearm") {
            let outside =
                b.node(|g, id| n::trigger(g, id, stage.string("rearm_trigger")?, "IsOutside"))?;
            b.connect(&deferred, &outside, "In")?;
            b.connect(&outside, &setup, "In")?;
        } else if b.outputs.contains_key("declined") {
            b.finish(deferred, "declined")?;
        } else {
            b.finish(deferred, "success")?;
        }
    }
    Ok(())
}
