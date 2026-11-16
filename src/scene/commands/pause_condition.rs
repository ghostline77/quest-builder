use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let actor = v.get("entity_ref").is_some();
    let trigger=h.wrap(json!({"$type":"questTriggerCondition","activatorRef":if actor {actor_entity(v)?} else {n::entity(None,&[])},"isPlayerActivator":u8::from(!actor),"triggerAreaRef":n::node_ref(string(v,"trigger_ref")?),"type":"IsInside"}));
    let condition = if flag(v, "require_player_not_in_combat", false)? == 1 {
        let kind=h.wrap(json!({"$type":"questCharacterCombat_ConditionType","inverted":1,"isPlayer":1,"objectRef":n::entity(None,&[])}));
        let combat = h.wrap(json!({"$type":"questCharacterCondition","type":kind}));
        h.wrap(json!({"$type":"questLogicalCondition","conditions":[trigger,combat],"operation":"AND"}))
    } else {
        trigger
    };
    Ok(Command::new(
        "questPauseConditionNodeDefinition",
        json!({"condition":condition}),
    ))
}
