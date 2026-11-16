//! Native reach area stage generation.

use super::Block;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let mut tail = b.enter()?;
    tail = b.pin(
        tail,
        "Active",
        stage
            .fields
            .get("disable_previous_mappins")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    )?;

    tail = b.fact(tail, stage.optional("start_fact"))?;
    if let Some(encounter) = stage.fields.get("encounter") {
        let community = encounter["community"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("encounter community"))?;
        let activate = b.node(|g, id| n::community(g, id, community, "Activate", None, None))?;
        tail = b.append(&tail, activate, "In")?;
        let ready = b.node(|g, id| n::spawned(g, id, community, None))?;
        tail = b.append(&tail, ready, "In")?;
        if let Some(entries) = encounter.get("entries").and_then(Value::as_array) {
            for entry in entries {
                tail = super::shared::content::attitude(
                    &mut b,
                    tail,
                    community,
                    entry
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("encounter entry"))?,
                    encounter
                        .get("attitude_group")
                        .and_then(Value::as_str)
                        .unwrap_or("hostile"),
                )?;
            }
        }
    }

    let node = b.node(|g, id| n::trigger(g, id, stage.string("trigger")?, "IsInside"))?;
    tail = b.append(&tail, node, "In")?;
    tail = b.objective(tail, "success")?;
    tail = b.pin(tail, "Inactive", false)?;
    if let Some(community) = stage.optional("cleanup_community") {
        let release = b.node(|g, id| n::community(g, id, community, "Deactivate", None, None))?;
        tail = b.append(&tail, release, "In")?;
    }

    b.finish(tail, "success")?;

    b.document()
}
