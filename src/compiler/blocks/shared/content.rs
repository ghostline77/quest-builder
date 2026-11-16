//! Shared native operations used by the domain block builders.
use super::{Block, Signal};
use crate::nodes as n;
use anyhow::{Context, Result};
use serde_json::{Value, json};

pub(in crate::compiler::blocks) fn scene(
    b: &mut Block<'_>,
    source: Signal,
    spec: &Value,
) -> Result<Signal> {
    let path = spec["scene"].as_str().context("scene resource")?;
    let origin = spec["origin"].as_str().context("scene origin")?;
    let entry = spec.get("entry").and_then(Value::as_str).unwrap_or("start");
    let exit = spec.get("exit").and_then(Value::as_str).unwrap_or("end");
    let node = b.node(|g, id| n::scene(g, id, path, origin, &[entry], &[exit]))?;
    b.connect(&source, &node, entry)?;
    Ok(node.port(exit))
}

pub(in crate::compiler::blocks) fn appearance(
    b: &mut Block<'_>,
    source: Signal,
    community: &str,
    entry: &str,
    appearance: &str,
) -> Result<Signal> {
    let changed=b.node(|g,id|{
        let subtype=g.wrap(json!({"$type":"questCharacterManagerVisuals_ChangeEntityAppearance","appearanceEntries":[{"$type":"questCharacterManagerVisuals_EntityAppearanceOperationBaseEntityAppearanceEntry","appearanceName":n::cname(appearance),"isPlayer":0,"puppetRef":n::entity(Some(community),&[entry])}]}));
        let kind=g.wrap(json!({"$type":"questCharacterManagerVisuals_NodeType","subtype":subtype}));
        g.node(id,"questCharacterManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))
    })?;
    b.append(&source, changed, "In")
}

pub(in crate::compiler::blocks) fn attitude(
    b: &mut Block<'_>,
    source: Signal,
    community: &str,
    entry: &str,
    group: &str,
) -> Result<Signal> {
    let node=b.node(|g,id|{
        let subtype=g.wrap(json!({"$type":"questCharacterManagerParameters_SetAttitudeGroupForPuppet","groupName":n::cname(group),"isPlayer":0,"puppetRef":n::entity(Some(community),&[entry])}));
        let kind=g.wrap(json!({"$type":"questCharacterManagerParameters_NodeType","subtype":subtype}));
        g.node(id,"questCharacterManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))
    })?;
    b.append(&source, node, "In")
}

pub(in crate::compiler::blocks) fn protect(
    b: &mut Block<'_>,
    source: Signal,
    community: &str,
    entry: &str,
    tag: &str,
) -> Result<Signal> {
    let node=b.node(|g,id|{
        let subtype=g.wrap(json!({"$type":"questCharacterManagerParameters_SetMortality","isPlayer":0,"puppetRef":n::entity(Some(community),&[entry]),"resetToDefault":0,"source":n::cname(tag),"state":"Invulnerable"}));
        let kind=g.wrap(json!({"$type":"questCharacterManagerParameters_NodeType","subtype":subtype}));
        g.node(id,"questCharacterManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))
    })?;
    b.append(&source, node, "In")
}

pub(in crate::compiler::blocks) fn effects(
    b: &mut Block<'_>,
    source: &Signal,
    effects: &Value,
) -> Result<()> {
    for effect in effects.as_array().context("completion effects")? {
        let community = effect["community"].as_str().context("effect community")?;
        for entry in effect["entries"].as_array().context("effect entries")? {
            let entry = entry.as_str().context("effect actor entry")?;
            for record in effect["status_effects"]
                .as_array()
                .context("status effects")?
            {
                let record = record.as_str().context("status effect record")?;
                let node=b.node(|g,id|{
                    let subtype=g.wrap(json!({"$type":"questCharacterManagerParameters_SetStatusEffect","isPlayer":0,"isPlayerStatusEffectSource":1,"set":1,"puppetRef":n::entity(Some(community),&[entry]),"statusEffectID":n::tweak_id(record),"statusEffectSourceObject":n::entity(Some("#player"),&[]),"recordSelector":null}));
                    let kind=g.wrap(json!({"$type":"questCharacterManagerParameters_NodeType","subtype":subtype}));
                    g.node(id,"questCharacterManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))
                })?;
                // Missing or defeated actors must not prevent quest completion.
                b.connect(source, &node, "In")?;
            }
        }
    }
    Ok(())
}
