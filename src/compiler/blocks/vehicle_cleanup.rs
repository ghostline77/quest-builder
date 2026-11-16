//! Native vehicle cleanup stage generation.

use super::Block;
use super::shared::bindings::{list, s};
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::Result;
use serde_json::{Value, json};

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    let mut tail = b.start.clone();
    let retire=b.node(|g,id|{let kind=g.wrap(json!({"$type":"questEnablePlayerVehicle_NodeType","vehicle":stage.string("player_vehicle_record")?,"enable":0,"despawn":1,"makePlayerActiveVehicle":0}));g.node(id,"questVehicleNodeDefinition",&["In"],&["Out"],json!({"type":kind}))})?;
    tail = b.append(&tail, retire, "In")?;
    if let Some(description) = stage.optional("description_entry") {
        let node =
            b.node(|g, id| n::journal(g, id, description, "gameJournalQuestDescription", 2))?;
        tail = b.append(&tail, node, "Active")?;
    }
    for community in list(stage, "cleanup_communities")? {
        let node = b.node(|g, id| n::community(g, id, s(community)?, "Deactivate", None, None))?;
        tail = b.append(&tail, node, "In")?;
    }
    tail = b.fact(tail, Some(stage.string("completion_fact")?))?;
    b.finish(tail, "success")?;

    b.document()
}
