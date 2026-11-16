//! Native phone job offer stage generation.

use super::Block;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let mut tail = b.start.clone();
    let contact = stage.string("contact")?;
    let contact = if contact.starts_with("contacts/") {
        contact.to_owned()
    } else {
        format!("contacts/{contact}")
    };
    let activate = b.node(|g, id| n::journal(g, id, &contact, "gameJournalContact", 1))?;
    tail = b.append(&tail, activate, "Active")?;
    if let Some(fact) = stage.optional("prerequisite_fact") {
        let node = b.node(|g, id| n::fact_condition(g, id, fact, "Greater", 0, false))?;
        tail = b.append(&tail, node, "In")?;
    }
    let hours = stage.number("delay_game_hours", 0);
    if hours > 0 {
        let node = b.node(|g, id| n::game_delay(g, id, hours / 24, hours % 24, 0, 0))?;
        tail = b.append(&tail, node, "In")?;
    }
    if stage.number("delay_seconds", 0) > 0 {
        let delay = b.node(|g, id| n::real_delay(g, id, stage.number("delay_seconds", 0), 0))?;
        tail = b.append(&tail, delay, "In")?;
    }
    tail = b.message(tail, stage.string("message")?)?;
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
    tail = b.fact(tail, Some(stage.string("start_fact")?))?;
    let accepted = b.node(|g, id| {
        n::journal_condition(
            g,
            id,
            stage.string("accept_choice")?,
            "gameJournalPhoneChoiceEntry",
            1,
            true,
        )
    })?;
    tail = b.append(&tail, accepted, "In")?;
    tail = b.fact(tail, Some(stage.string("accepted_fact")?))?;
    b.finish(tail, "success")?;

    b.document()
}
