use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let duration = number(v, "duration_seconds", 2.5)?;
    let max = number(v, "max_duration_seconds", duration)?;
    ensure!(
        duration >= 0. && max >= 0.,
        "look-at duration must be nonnegative"
    );
    let target = string(v, "target_ref")?;
    let names = v
        .get("target_entry")
        .and_then(Value::as_str)
        .into_iter()
        .collect::<Vec<_>>();
    let offset = v.get("offset").cloned().unwrap_or(json!({}));
    let kind=h.wrap(json!({"$type":"questPlayerLookAt_NodeType","adjustPitch":flag(v,"adjust_pitch",true)?,"adjustYaw":flag(v,"adjust_yaw",true)?,"cameraInputMagToBreak":number(v,"camera_input_magnitude_to_break",2.)?,"duration":duration,"easeIn":flag(v,"ease_in",true)?,"easeOut":flag(v,"ease_out",true)?,"endOnCameraInputApplied":flag(v,"end_on_camera_input",false)?,"endOnTargetReached":flag(v,"end_on_target_reached",true)?,"endOnTimeExceeded":flag(v,"end_on_time_exceeded",true)?,"maxDuration":max,"objectRef":n::entity(Some(target),&names),"offsetPos":{"$type":"Vector3","X":number(&offset,"x",0.)?,"Y":number(&offset,"y",0.)?,"Z":number(&offset,"z",0.)?},"precision":number(v,"precision",0.1)?,"slotName":n::cname(v.get("slot").and_then(Value::as_str).unwrap_or("Head")),"useOffsetToPlayer":0}));
    Ok(Command::new(
        "questSceneManagerNodeDefinition",
        json!({"type":kind}),
    ))
}
