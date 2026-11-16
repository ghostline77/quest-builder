use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let selector=h.wrap(json!({"$type":"questRecordSelector","characterRecordID":tweak("0"),"deviceRecordID":tweak("0"),"isCharacter":1,"isDevice":0,"isItem":0,"itemRecordID":tweak("0")}));
    let subtype=h.wrap(json!({"$type":"questCharacterManagerParameters_SetStatusEffect","isPlayer":1,"isPlayerStatusEffectSource":1,"puppetRef":n::entity(None,&[]),"recordSelector":selector,"set":flag(v,"set",true)?,"statusEffectID":tweak(string(v,"status_effect")?),"statusEffectSourceObject":n::entity(None,&[])}));
    let kind =
        h.wrap(json!({"$type":"questCharacterManagerParameters_NodeType","subtype":subtype}));
    Ok(Command::new(
        "questCharacterManagerNodeDefinition",
        json!({"type":kind}),
    ))
}
