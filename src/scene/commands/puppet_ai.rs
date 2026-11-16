use super::*;
pub(super) fn build(v: &Value, _h: &mut Handles) -> Result<Command> {
    Ok(Command::new(
        "questPuppetAIManagerNodeDefinition",
        json!({"entries":[{"$type":"questPuppetAIManagerNodeDefinitionEntry","aiTier":v.get("ai_tier").and_then(Value::as_str).unwrap_or("Cinematic"),"entityReference":actor_entity(v)?}]}),
    ))
}
