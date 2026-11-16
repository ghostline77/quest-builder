//! Persistent scene-owned story and topic facts.
use super::*;

pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let kind = h.wrap(json!({"$type":"questSetVar_NodeType","factName":string(v,"fact")?,"setExactValue":1,"value":v.get("value").and_then(Value::as_i64).unwrap_or(1)}));
    Ok(Command::new(
        "questFactsDBManagerNodeDefinition",
        json!({"type":kind}),
    ))
}
