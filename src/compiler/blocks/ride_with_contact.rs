//! Native ride with contact stage generation.

use super::Block;
use super::shared::bindings::{list, s};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Result, ensure};
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;
    ride(&mut b)?;
    b.document()
}

fn ride(b: &mut Block<'_>) -> Result<()> {
    let stage = b.stage;
    let mut tail = b.enter()?;
    let passengers = list(stage, "additional_passengers")?;
    if stage.flag("wait_for_contacts") {
        let ready = b.node(|g, id| {
            n::spawned(
                g,
                id,
                stage.string("contact_community")?,
                Some(stage.string("contact_entry")?),
            )
        })?;
        tail = b.append(&tail, ready, "In")?;
        for actor in passengers {
            let ready = b.node(|g, id| {
                n::spawned(g, id, s(&actor["community"])?, Some(s(&actor["entry"])?))
            })?;
            tail = b.append(&tail, ready, "In")?;
        }
    }
    let node = b.node(|g, id| {
        n::assign_vehicle(
            g,
            id,
            stage.string("contact_community")?,
            stage.string("contact_entry")?,
            stage.string("vehicle_community")?,
            stage.string("vehicle_entry")?,
            stage.optional("contact_slot").unwrap_or("seat_front_right"),
        )
    })?;
    tail = b.append(&tail, node, "In")?;
    for actor in passengers {
        let assigned = b.node(|g, id| {
            n::assign_vehicle(
                g,
                id,
                s(&actor["community"])?,
                s(&actor["entry"])?,
                stage.string("vehicle_community")?,
                stage.string("vehicle_entry")?,
                s(&actor["slot"])?,
            )
        })?;
        tail = b.append(&tail, assigned, "In")?;
    }
    let player = b.node(|g, id| {
        n::mount(
            g,
            id,
            n::entity(
                Some(stage.string("vehicle_community")?),
                &[stage.string("vehicle_entry")?],
            ),
            n::entity(None, &[]),
            false,
            true,
            stage.optional("player_role").unwrap_or("Driver"),
            "OnMount",
            false,
        )
    })?;
    let contact = b.node(|g, id| {
        n::mount(
            g,
            id,
            n::entity(
                Some(stage.string("vehicle_community")?),
                &[stage.string("vehicle_entry")?],
            ),
            n::entity(
                Some(stage.string("contact_community")?),
                &[stage.string("contact_entry")?],
            ),
            false,
            false,
            stage.optional("contact_role").unwrap_or("Passenger"),
            "OnMount",
            false,
        )
    })?;
    let mut mounted = vec![player, contact];
    for actor in passengers {
        mounted.push(b.node(|g, id| {
            n::mount(
                g,
                id,
                n::entity(
                    Some(stage.string("vehicle_community")?),
                    &[stage.string("vehicle_entry")?],
                ),
                n::entity(Some(s(&actor["community"])?), &[s(&actor["entry"])?]),
                false,
                false,
                "Passenger",
                "OnMount",
                false,
            )
        })?);
    }
    ensure!(mounted.len() >= 2, "ride needs player and contact");
    for actor in &mounted {
        b.connect(&tail, actor, "In")?;
    }
    tail = b.join(&mounted, true)?;
    tail = b.objective(tail, "success")?;
    b.finish(tail, "success")
}
