use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let present = flag(v, "present", true)? != 0;
    let dynamic = v.get("dynamic_name").and_then(Value::as_str);
    ensure!(
        dynamic.is_none() || v.get("entity_ref").is_none(),
        "choose dynamic_name or entity_ref"
    );
    let reference = if let Some(name) = dynamic {
        dynamic_entity(name)
    } else {
        actor_entity(v)?
    };
    let comparison=h.wrap(json!({"$type":"questComparisonParam","comparisonType":if present{"Greater"}else{"Equal"},"count":0,"entireCommunity":u8::from(dynamic.is_some())}));
    let kind=h.wrap(json!({"$type":"questCharacterSpawned_ConditionType","comparisonParams":comparison,"objectRef":reference}));
    let condition = h.wrap(json!({"$type":"questCharacterCondition","type":kind}));
    Ok(Command::new(
        "questPauseConditionNodeDefinition",
        json!({"condition":condition}),
    ))
}
