//! Native audio-call boundaries used around ordinary quest stages.
use super::Block;
use crate::{manifest::Stage, nodes as n};
use anyhow::Result;
use serde_json::{Value, json};

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;
    let call=b.node(|g,id|{
        let caller=n::journal_path(g,stage.string("caller")?,"gameJournalContact",1);
        let addressee=n::journal_path(g,stage.optional("addressee").unwrap_or("contacts/player"),"gameJournalContact",1);
        let kind=g.wrap(json!({"$type":"questCallContact_NodeType","caller":caller,"addressee":addressee,"phase":stage.string("action")?,"mode":"Audio","prefabNodeRef":n::node_ref(stage.optional("prefab").unwrap_or("#holocalls_studio")),"applyPhoneRestriction":0,"isRejectable":0,"showAvatar":0,"visuals":"Default"}));
        g.node(id,"questPhoneManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))
    })?;
    let tail = b.append(&b.start.clone(), call, "In")?;
    b.finish(tail, "success")?;
    b.document()
}
