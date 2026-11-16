use super::*;

pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let target = h.wrap(json!({"$type":"questUniversalRef", "refLocalPlayer":true}));
    let params = h.wrap(json!({"$type":"questRotateToParams", "facingTargetRef":target,
        "angleOffset":0.0, "speed":180.0}));
    Ok(Command::new("questRotateToNodeDefinition", json!({
        "entityReference":actor_entity(v)?, "params":params
    })))
}
