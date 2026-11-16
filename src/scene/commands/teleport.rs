use super::*;
pub(super) fn build(v: &Value, h: &mut Handles, player: bool) -> Result<Command> {
    let entity = if player {
        n::entity(None, &[])
    } else {
        actor_entity(v)?
    };
    let reference=h.wrap(json!({"$type":"questUniversalRef","entityReference":entity,"mainPlayerObject":0,"refLocalPlayer":u8::from(player)}));
    let destination=h.wrap(json!({"$type":"questUniversalRef","entityReference":n::entity(Some(string(v,"destination_ref")?),&[]),"mainPlayerObject":0,"refLocalPlayer":0}));
    let params=h.wrap(json!({"$type":"questTeleportPuppetParamsV1","destinationOffset":{"$type":"Vector3","X":0,"Y":0,"Z":0},"destinationRef":destination,"doNavTest":0,"healAtTeleport":flag(v,"heal",false)?,"useFastTravelMechanism":0}));
    let look = v.get("look_at_ref").and_then(Value::as_str);
    let names = v
        .get("look_at_entry")
        .and_then(Value::as_str)
        .into_iter()
        .collect::<Vec<_>>();
    let look_at=h.wrap(json!({"$type":"questPlayerLookAtParams","adjustPitch":1,"adjustYaw":1,"cameraInputMagToBreak":0.2,"duration":0.25,"easeIn":1,"easeOut":1,"endOnCameraInputApplied":1,"endOnTargetReached":1,"endOnTimeExceeded":1,"lookAtTarget":n::entity(look,&names),"maxDuration":2,"offset":{"$type":"Vector3","X":0,"Y":0,"Z":0},"precision":0.1,"slotName":n::cname("None"),"useOffsetToPlayer":u8::from(look.is_none())}));
    Ok(Command::new(
        "questTeleportPuppetNodeDefinition",
        json!({"entityReference":reference,"lookAtAction":if look.is_some(){"Set"}else{"Reset"},"params":params,"playerLookAt":look_at}),
    ))
}
