//! Snapshot predicates for persistent scene branches.
use super::*;

pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let kind = h.wrap(json!({"$type":"questVarComparison_ConditionType","factName":string(v,"fact")?,"comparisonType":v.get("comparison").and_then(Value::as_str).unwrap_or("Greater"),"value":v.get("value").cloned().unwrap_or(json!(0))}));
    let condition = h.wrap(json!({"$type":"questFactsDBCondition","type":kind}));
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
