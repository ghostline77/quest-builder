//! SQ017's companion movement, with quest-owned route geometry.
use super::*;

pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let movement = v.get("movement_type").and_then(Value::as_str);
    ensure!(movement.is_none_or(|value| matches!(value, "Walk" | "Run" | "Sprint")), "invalid spline movement_type");
    let facing = h.wrap(json!({"$type":"questUniversalRef"}));
    let companion = h.wrap(json!({"$type":"questUniversalRef"}));
    let mut additional = h.wrap(json!({
        "$type":"questMoveOnSplineAdditionalParams",
        "type":"WithCompanion",
        "simpleParams":{"$type":"questSimpleMoveOnSplineParams","facingTargetRef":facing},
        "withCompanionParams":{
            "$type":"questWithCompanionMoveOnSplineParams",
            "companionRef":companion,
            "companionDistancePreset":"Close",
            "catchUpWithCompanion":flag(v,"catch_up_with_companion",false)?,
            "teleportToCompanion":false
        }
    }));
    if let Some(movement) = movement {
        additional["Data"]["withCompanionParams"]["movementType"] = json!({
            "$type":"AIMovementTypeSpec", "useNPCMovementParams":false, "movementType":movement
        });
    }
    if let Some(value) = v.get("match_player_speed") {
        ensure!(value.is_boolean(), "match_player_speed must be boolean");
        additional["Data"]["withCompanionParams"]["useMatchForSpeedForPlayer"] = value.clone();
    }
    if flag(v, "with_companion", true)? == 0 {
        additional["Data"]["type"] = json!("Simple");
        additional["Data"]["simpleParams"]["movementType"] = json!(movement.unwrap_or("Walk"));
        additional["Data"]["simpleParams"]["rotateEntityTowardsFacingTarget"] = json!(false);
    }
    let params = h.wrap(json!({
        "$type":"questMoveOnSplineParams",
        "splineNodeRef":n::node_ref(string(v,"spline_ref")?),
        "useStart":false,
        "useStop":false,
        "reverse":flag(v,"reverse",false)?,
        "additionalParams":additional
    }));
    Ok(Command::new(
        "questMovePuppetNodeDefinition",
        json!({
            "entityReference":actor_entity(v)?,"nodeParams":params
        }),
    ))
}
