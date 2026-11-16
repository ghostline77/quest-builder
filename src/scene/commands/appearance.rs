use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let subtype=h.wrap(json!({"$type":"questCharacterManagerVisuals_ChangeEntityAppearance","appearanceEntries":[{"$type":"questCharacterManagerVisuals_EntityAppearanceOperationBaseEntityAppearanceEntry","appearanceName":n::cname(string(v,"appearance")?),"isPlayer":0,"puppetRef":actor_entity(v)?}]}));
    let kind = h.wrap(json!({"$type":"questCharacterManagerVisuals_NodeType","subtype":subtype}));
    Ok(Command::new(
        "questCharacterManagerNodeDefinition",
        json!({"type":kind}),
    ))
}
