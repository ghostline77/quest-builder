use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let kind=h.wrap(json!({"$type":"questCharacterGender_CondtionType","gender":n::cname(v.get("gender").and_then(Value::as_str).unwrap_or("Male")),"isPlayer":flag(v,"is_player",true)?,"objectRef":n::entity(v.get("object_ref").and_then(Value::as_str),&[])}));
    let condition = h.wrap(json!({"$type":"questCharacterCondition","type":kind}));
    let mut command = Command::new(
        "questConditionNodeDefinition",
        json!({"condition":condition}),
    );
    command.outputs = vec![
        ("True".into(), "on_true".into()),
        ("False".into(), "on_false".into()),
    ];
    Ok(command)
}
