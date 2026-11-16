use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let function = v
        .get("function")
        .and_then(Value::as_str)
        .unwrap_or("UseWorkspot");
    let workspot = if v["workspot_ref"] == 0 {
        n::node_ref("")
    } else {
        n::node_ref(string(v, "workspot_ref")?)
    };
    let params=h.wrap(json!({"$type":"questUseWorkspotParamsV1","changeWorkspot":1,"continueInCombat":0,"dangleResetSimulation":0,"enableIdleMode":flag(v,"enable_idle_mode",false)?,"entryId":{"$type":"workWorkEntryId","id":integer(v,"entry_id",u32::MAX)?},"entryTag":n::cname(v.get("entry_tag").and_then(Value::as_str).unwrap_or("None")),"exitAnimName":n::cname(v.get("exit_animation").and_then(Value::as_str).unwrap_or("None")),"exitEntryId":{"$type":"workWorkEntryId","id":integer(v,"exit_entry_id",u32::MAX)?},"finishAnimation":flag(v,"finish_animation",true)?,"forceEntryAnimName":n::cname(v.get("force_entry_animation").and_then(Value::as_str).unwrap_or("None")),"function":function,"instant":flag(v,"instant",false)?,"isPlayer":0,"isWorkspotInfinite":flag(v,"is_workspot_infinite",true)?,"jumpToEntry":flag(v,"jump_to_entry",false)?,"maxAnimTimeLimit":number(v,"max_anim_time_limit",0.)?,"meshDissolvingEnabled":1,"movementType":v.get("movement_type").and_then(Value::as_str).unwrap_or("Walk"),"playerParams":{"$type":"questUseWorkspotPlayerParams","applyCameraParams":0,"cameraSettings":{"$type":"gameTier3CameraSettings","pitchBottomLimit":45,"pitchSpeedMultiplier":1,"pitchTopLimit":60,"yawLeftLimit":60,"yawRightLimit":60,"yawSpeedMultiplier":1},"cameraUseTrajectorySpace":1,"emptyHands":0,"parallaxSpace":"Trajectory","parallaxWeight":1,"tier":"Tier3","vehicleProceduralCameraWeight":1},"repeatCommandOnInterrupt":flag(v,"repeat_on_interrupt",true)?,"teleport":flag(v,"teleport",function!="StopWorkspot")?,"workExcludedGestures":[],"workspotNode":workspot}));
    let mut params = params;
    let player = flag(v, "is_player", false)?;
    params["Data"]["isPlayer"] = json!(player);
    params["Data"]["playerParams"]["applyCameraParams"] = json!(player);
    params["Data"]["playerParams"]["emptyHands"] = json!(player);
    let mut command = Command::new(
        "questUseWorkspotNodeDefinition",
        json!({"entityReference":if player == 1 { n::entity(Some("#player"), &[]) } else {actor_entity(v)?},"paramsV1":params}),
    );
    command.outputs = vec![("Success".into(), "on_success".into())];
    if function == "UseWorkspot" {
        command
            .outputs
            .push(("Work Started".into(), "on_work_started".into()));
    }
    Ok(command)
}
