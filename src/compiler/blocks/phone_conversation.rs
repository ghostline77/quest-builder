//! Native phone conversation stage generation.

use super::Block;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Context, Result, ensure};
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;
    populate(&mut b)?;
    b.document()
}

fn populate(b: &mut Block<'_>) -> Result<()> {
    let stage = b.stage;
    if let Some(fact) = stage.optional("once_fact") {
        let test = b.node(|g, id| n::fact_condition(g, id, fact, "Greater", 0, true))?;
        b.connect(&b.start.clone(), &test, "In")?;
        b.finish(test.port("True"), "success")?;
        b.start = b.fact(test.port("False"), Some(fact))?;
    }
    let mut tail = b.enter()?;
    let contact = stage.string("contact")?;
    let contact = if contact.starts_with("contacts/") {
        contact.to_owned()
    } else {
        format!("contacts/{contact}")
    };
    let activate = b.node(|g, id| n::journal(g, id, &contact, "gameJournalContact", 1))?;
    tail = b.append(&tail, activate, "Active")?;
    let thread = b.node(|g, id| {
        n::journal(
            g,
            id,
            stage.string("thread")?,
            "gameJournalPhoneConversation",
            1,
        )
    })?;
    tail = b.append(&tail, thread, "Active")?;
    let delay = b.node(|g, id| n::real_delay(g, id, stage.number("delay_seconds", 1), 0))?;
    tail = b.append(&tail, delay, "In")?;
    for path in stage.fields["messages"]
        .as_array()
        .context("phone messages")?
    {
        tail = b.message(tail, path.as_str().context("message path")?)?;
    }
    let mut groups = Vec::new();
    if let Some(branches) = stage
        .fields
        .get("opening_branches")
        .and_then(Value::as_array)
        && !branches.is_empty()
    {
        groups.push(branches);
    }
    for group in stage
        .fields
        .get("conditional_message_groups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        groups.push(
            group["branches"]
                .as_array()
                .context("conditional message branches")?,
        );
    }
    for branches in groups {
        let mut tails = Vec::new();
        for branch in branches {
            let condition = branch["condition"]
                .as_str()
                .context("conditional phone fact")?;
            let probe = b.node(|g, id| n::fact_condition(g, id, condition, "Greater", 0, false))?;
            b.connect(&tail, &probe, "In")?;
            let mut previous = probe;
            if let Some(unless) = branch.get("unless").and_then(Value::as_str) {
                let probe =
                    b.node(|g, id| n::fact_condition(g, id, unless, "LessOrEqual", 0, false))?;
                b.connect(&tail, &probe, "In")?;
                previous = b.join(&[previous, probe], true)?;
            }
            for path in branch["messages"]
                .as_array()
                .context("conditional messages")?
            {
                previous = b.message(previous, path.as_str().context("conditional message")?)?;
            }
            tails.push(previous);
        }
        tail = b.join(&tails, false)?;
    }
    let group = b.node(|g, id| {
        n::journal(
            g,
            id,
            stage.string("choice_group")?,
            "gameJournalPhoneChoiceGroup",
            1,
        )
    })?;
    tail = b.append(&tail, group, "Active")?;
    let mut replies = Vec::new();
    let choices = stage.fields["choices"]
        .as_array()
        .context("phone choices")?;
    ensure!(
        !choices.is_empty(),
        "phone conversation requires at least one reply"
    );
    for choice in choices {
        let path = choice["choice"].as_str().context("choice path")?;
        let selected = b.node(|g, id| {
            n::journal_condition(g, id, path, "gameJournalPhoneChoiceEntry", 1, true)
        })?;
        let selected = b.append(&tail, selected, "In")?;
        let reply = b.message(
            selected,
            choice["reply"].as_str().context("phone reply path")?,
        )?;
        replies.push(b.fact(reply, choice.get("set_fact").and_then(Value::as_str))?);
    }
    tail = b.join(&replies, false)?;
    if let Some(message) = stage.optional("final_message") {
        tail = b.message(tail, message)?;
    }
    let delay = b.node(|g, id| n::real_delay(g, id, 1, 0))?;
    tail = b.append(&tail, delay, "In")?;
    for path in stage
        .fields
        .get("postscript_messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        tail = b.message(tail, path.as_str().context("postscript message")?)?;
    }
    tail = b.objective(tail, "success")?;
    if let Some(record) = stage.optional("reward") {
        let reward = b.node(|g, id| n::reward(g, id, record))?;
        tail = b.append(&tail, reward, "In")?;
    }
    tail = b.fact(tail, stage.optional("completion_fact"))?;
    if let Some(path) = stage.optional("complete_quest") {
        let node = b.node(|g, id| n::objective(g, id, path, true))?;
        tail = b.append(&tail, node, "Succeeded")?;
    }
    b.finish(tail, "success")
}
