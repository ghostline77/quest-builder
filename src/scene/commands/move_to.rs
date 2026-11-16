//! Native navigation to a world marker, retaining navigation and collision.
use super::*;

pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let movement = v.get("movement_type").and_then(Value::as_str).unwrap_or("Walk");
    ensure!(matches!(movement, "Walk" | "Run" | "Sprint"), "invalid move_to movement_type");
    let distance = number(v, "distance", 0.35)?;
    ensure!(distance >= 0.0 && distance.is_finite(), "invalid move_to distance");
    let target = h.wrap(json!({
        "$type":"questUniversalRef",
        "entityReference":n::entity(Some(string(v,"destination_ref")?), &[]),
        "refLocalPlayer":false
    }));
    let params = h.wrap(json!({
        "$type":"questMoveToParams", "movementTargetRef":target,
        "movementType":movement, "ignoreNavigation":false,
        "desiredDistanceFromTarget":distance, "finishWhenDestinationReached":true,
        "useStart":false, "useStop":false, "repeatCommandOnInterrupt":false
    }));
    Ok(Command::new("questMovePuppetNodeDefinition", json!({
        "entityReference":actor_entity(v)?, "nodeParams":params
    })))
}
