use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let comparison = h.wrap(json!({"$type":"questVarComparison_ConditionType","factName":string(v,"fact")?,"comparisonType":v.get("comparison").and_then(Value::as_str).unwrap_or("Greater"),"value":v.get("value").and_then(Value::as_i64).unwrap_or(0)}));
    let condition = h.wrap(json!({"$type":"questFactsDBCondition","type":comparison}));
    Ok(Command::new(
        "questPauseConditionNodeDefinition",
        json!({"condition":condition}),
    ))
}
