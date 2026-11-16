use super::*;

fn event(scene: &str, section: &str, key: &str, kind: &str, start: u32, duration: u32) -> Value {
    json!({"$type":kind,"duration":duration,"executionTagFlags":0,"id":event_id(&[scene,section,key]),"scalingData":null,"startTime":start})
}
fn marker(reference: &str) -> Value {
    json!({"$type":"scnMarker","entityRef":n::entity(None,&[]),"isMounted":1,"localMarkerId":n::cname("None"),"nodeRef":n::node_ref(reference),"slotName":n::cname("None"),"type":"Global"})
}
fn anim_data(curve: &str) -> Value {
    json!({"$type":"scneventsPlayAnimEventData","blendIn":0,"blendInCurve":curve,"blendOut":0,"blendOutCurve":curve,"clipEnd":0,"clipFront":0,"stretch":1})
}

pub(super) fn section(
    spec: &Value,
    raw: &Value,
    text: &text::Text,
    actors: &BTreeMap<String, u32>,
    h: &mut Handles,
) -> Result<Value> {
    let scene = string(spec, "name")?;
    let key = string(raw, "key")?;
    let mut events = Vec::new();
    let mut start = 0u32;
    let gap = integer(raw, "line_gap_ms", integer(spec, "line_gap_ms", 250)?)?;
    let phone_actors = list(spec, "holocall_speakers")?
        .iter()
        .map(|actor| text::actor(actors, actor.as_str().context("holocall speaker")?))
        .collect::<Result<Vec<_>>>()?;
    for row in list(raw, "lines")? {
        let line_key = row.as_str().context("section line key")?;
        let line = text
            .spoken
            .get(line_key)
            .with_context(|| format!("missing section line {line_key}"))?;
        let line_id = *text
            .lines
            .get(line_key)
            .context("spoken line absent from screenplay order")?;
        let duration = integer(
            line,
            "duration_ms",
            integer(spec, "default_line_duration_ms", 2500)?,
        )?;
        ensure!(duration > 0, "dialogue duration must be positive");
        let mut data = event(
            scene,
            key,
            &format!("{line_key}:{line_id}"),
            "scnDialogLineEvent",
            start,
            duration,
        );
        data["additionalSpeakers"] =
            json!({"$type":"scnAdditionalSpeakers","executionTag":0,"role":"Full","speakers":[]});
        data["screenplayLineId"] = id("scnscreenplayItemId", line_id);
        data["visualStyle"] = json!("regular");
        let phone = u8::from(
            flag(line, "holocall_speaker", false)? != 0
                || (!phone_actors.is_empty()
                    && phone_actors.contains(&text::actor(actors, string(line, "speaker")?)?)),
        );
        data["voParams"] = json!({"$type":"scnDialogLineVoParams","alwaysUseBrainGender":0,"customVoEvent":n::cname("None"),"disableHeadMovement":0,"ignoreSpeakerIncapacitation":0,"isHolocallSpeaker":phone,"voContext":"Vo_Context_Quest","voExpression":if phone==1{"Vo_Expression_Phone"}else{"Vo_Expression_Spoken"}});
        events.push(h.wrap(data));
        start = start
            .checked_add(duration)
            .and_then(|v| v.checked_add(gap))
            .context("section timing overflow")?;
    }
    let mut duration = integer(
        raw,
        "rid_duration_ms",
        if start > 0 {
            (start - gap)
                .checked_add(integer(
                    raw,
                    "tail_padding_ms",
                    integer(spec, "section_tail_padding_ms", 400)?,
                )?)
                .context("section tail timing overflow")?
        } else {
            integer(raw, "section_duration_ms", 1000)?
        },
    )?;
    ensure!(duration > 0, "section duration must be positive");
    for (index, row) in list(raw, "rid_events")?.iter().enumerate() {
        let actor = text::actor(actors, string(row, "actor")?)?;
        let anim = integer(row, "anim_ref_id", u32::MAX)?;
        ensure!(
            (anim as usize) < list(spec, "rid_animations")?.len(),
            "RID animation reference out of range"
        );
        let fpp = flag(row, "fpp", false)?;
        let mut data = event(
            scene,
            key,
            &format!("rid_{index}:{anim}"),
            "scnPlayRidAnimEvent",
            integer(row, "start_time_ms", 0)?,
            integer(row, "duration_ms", 0)?,
        );
        data.as_object_mut().context("RID event")?.extend(json!({"actorComponent":n::cname(row.get("component").and_then(Value::as_str).unwrap_or("body")),"actorHasCollision":1,"actorPlacement":"SceneOrigin","animData":{"$type":"scneventsPlayAnimEventExData","basic":anim_data("SinusoidalEaseInOut"),"bodyPartMask":n::cname("None"),"weight":1},"animOriginMarker":marker(string(row,"origin")?),"animResRefId":id("scnRidAnimationSRRefId",anim),"blendInTrajectoryBone":0,"blendOverride":"CopyPitch_CopyYaw","cameraBlendInDuration":0.5,"cameraBlendOutDuration":0.5,"cameraParallaxSpace":if fpp==1{"Camera"}else{"Trajectory"},"cameraParallaxWeight":0,"cameraUseTrajectorySpace":1,"convertToAdditive":0,"enableWorldSpaceSmoothing":1,"eyesBlendAdditive":1,"FPPControlActive":fpp,"gameplayAnimName":h.wrap(json!({"$type":"scnAnimName","type":"direct","unk1":[n::cname("None")],"unk2":[]})),"genderSpecificParams":gender(row)?,"idleIsMountedWorkspot":0,"isSceneCarrying":0,"lowerFaceBlendAdditive":1,"muteAnimEvents":"0","neckWeight":1,"performer":performer(actor)?,"pitchLimitBottom":0,"pitchLimitTop":0,"ridVersinon":5,"stayInScene":0,"upperFaceBlendAdditive":1,"vehicleProceduralCameraWeight":0,"yawLimitLeft":0,"yawLimitRight":0}).as_object().context("RID properties")?.clone());
        events.push(h.wrap(data));
    }
    ensure!(
        !(raw.get("rid_camera").is_some() && raw.get("camera_event").is_some()),
        "section cannot have two camera sources"
    );
    for (kind, field) in [
        ("scneventsPlayRidCameraAnimEvent", "rid_camera"),
        ("scneventsCameraEvent", "camera_event"),
    ] {
        if let Some(row) = raw.get(field) {
            let mut data = event(
                scene,
                key,
                field,
                kind,
                integer(row, "start_time_ms", 0)?,
                integer(row, "duration_ms", 0)?,
            );
            data["cameraRef"] = n::node_ref(string(row, "camera_ref")?);
            if field == "rid_camera" {
                let camera = integer(row, "camera_ref_id", u32::MAX)?;
                ensure!(
                    (camera as usize) < list(spec, "rid_camera_animations")?.len(),
                    "RID camera index out of range"
                );
                data["activateAsGameCamera"] = json!(1);
                data["animData"] = anim_data("Linear");
                data["animOriginMarker"] = marker(string(row, "origin")?);
                data["animSRRefId"] = id("scnRidCameraAnimationSRRefId", camera);
                data["cameraPlacement"] = json!("SceneOrigin");
                data["controlRenderToTextureState"] = json!(0);
                data["markCamerCut"] = json!(1);
            } else {
                let blend = number(row, "blend_time_seconds", 0.)?;
                ensure!(blend >= 0., "camera blend must be nonnegative");
                data["blendTime"] = json!(blend);
                data["isBlendIn"] = json!(flag(row, "blend_in", true)?);
            }
            events.push(h.wrap(data));
        }
    }
    if raw.get("rid_duration_ms").is_none() && raw.get("section_duration_ms").is_none() {
        for event in &events {
            let event = &event["Data"];
            duration = duration.max(
                integer(event, "startTime", 0)?
                    .checked_add(integer(event, "duration", 0)?)
                    .context("event timing overflow")?,
            );
        }
    }
    let player_id = list(spec, "actors")?
        .iter()
        .find(|actor| actor["kind"] == "player")
        .map(|actor| integer(actor, "id", u32::MAX))
        .transpose()?;
    for (index, row) in list(raw, "look_at_events")?.iter().enumerate() {
        events.push(h.wrap(look_at(
            scene, key, index, row, actors, player_id, duration,
        )?));
    }
    for e in &events {
        let data = &e["Data"];
        let end =
            u64::from(integer(data, "startTime", 0)?) + u64::from(integer(data, "duration", 0)?);
        ensure!(
            end <= u64::from(duration),
            "event extends past section {key} duration"
        );
    }
    let behaviors=list(spec,"actors")?.iter().map(|v|Ok(json!({"$type":"scnSectionInternalsActorBehavior","actorId":id("scnActorId",integer(v,"id",0)?),"behaviorMode":"OnlyIfAlive"}))).collect::<Result<Vec<_>>>()?;
    Ok(h.wrap(json!({"$type":"scnSectionNode","actorBehaviors":behaviors,"events":events,"ffStrategy":"automatic","isFocusClue":0,"nodeId":id("scnNodeId",integer(raw,"node_id",u32::MAX)?),"outputSockets":[graph::socket(0,0,list(raw,"on_end")?)?,graph::socket(1,0,list(raw,"on_cancel")?)?],"sectionDuration":{"$type":"scnSceneTime","stu":duration}})))
}

fn gender(row: &Value) -> Result<Vec<Value>> {
    let Some(params) = row.get("fpp_gender_params") else {
        return Ok(Vec::new());
    };
    let angles = |key| -> Result<Vec<Value>> {
        list(params,key)?.iter().map(|v|Ok(json!({"$type":"EulerAngles","Pitch":number(v,"pitch",0.)?,"Roll":number(v,"roll",0.)?,"Yaw":number(v,"yaw",0.)?}))).collect()
    };
    let masks = params.get("gender_masks").cloned().unwrap_or(json!([2, 1]));
    masks.as_array().context("gender masks")?.iter().map(|mask|Ok(json!({"$type":"scnfppGenderSpecificParams","genderMask":{"$type":"scnGenderMask","mask":mask.as_u64().context("gender mask")?},"idleCameraLs":{"$type":"EulerAngles","Pitch":0,"Roll":0,"Yaw":0},"idleControlCameraMs":{"$type":"EulerAngles","Pitch":0,"Roll":0,"Yaw":0},"transitionBlendInCameraSpace":[],"transitionBlendInTrajectorySpaceAngles":angles("transition_blend_in_trajectory_space_angles")?,"transitionEndInputAngles":angles("transition_end_input_angles")?}))).collect()
}

pub(super) fn look_at(
    scene: &str,
    key: &str,
    index: usize,
    row: &Value,
    actors: &BTreeMap<String, u32>,
    player_id: Option<u32>,
    duration: u32,
) -> Result<Value> {
    let actor = text::actor(actors, string(row, "actor")?)?;
    let target = text::actor(actors, string(row, "target")?)?;
    if Some(actor) == player_id {
        return player_look_at(scene, key, index, row, target, duration);
    }
    let mut data = event(
        scene,
        key,
        &format!("actor_look_at:{index}"),
        "scnLookAtEvent",
        integer(row, "start_ms", 0)?,
        integer(row, "duration_ms", duration)?,
    );
    data["basicData"] = json!({"$type":"scnLookAtBasicEventData","basic":{"$type":"scnAnimTargetBasicData","isStart":1,"performerId":performer(actor)?,"staticTarget":{"$type":"Vector4","W":1,"X":0,"Y":0,"Z":0},"targetActorId":id("scnActorId",u32::MAX),"targetOffsetEntitySpace":{"$type":"Vector4","W":0,"X":0,"Y":0,"Z":0},"targetPerformerId":performer(target)?,"targetPropId":id("scnPropId",u32::MAX),"targetSlot":n::cname(row.get("target_slot").and_then(Value::as_str).unwrap_or("pla_default_tgt")),"targetType":"Actor"},"removePreviousAdvancedLookAts":1,"requests":[{"$type":"animLookAtRequestForPart","attachLeftHandToRightHand":-1,"attachRightHandToLeftHand":-1,"bodyPart":n::cname("Eyes"),"request":{"$type":"animLookAtRequest","additionalParts":{"Elements":[{"$type":"animLookAtPartRequest","mode":0,"partName":n::cname("Head"),"suppress":0,"weight":1},{"$type":"animLookAtPartRequest","mode":0,"partName":n::cname("Chest"),"suppress":0,"weight":1}]},"calculatePositionInParentSpace":0,"debugInfo":format!("{scene}: actor {actor} looks at {target}"),"followingSpeedFactorOverride":-1,"hasOutTransition":0,"invalid":0,"limits":{"$type":"animLookAtLimits","backLimitDegrees":210,"hardLimitDegrees":270,"hardLimitDistance":1000000,"softLimitDegrees":140},"mode":0,"outTransitionSpeed":60,"priority":-1,"suppress":0,"transitionSpeed":140}}]});
    Ok(data)
}

// Player camera control uses a target performer, not an NPC skeletal request
// performed by the player. The player is implicit in this native event type.
fn player_look_at(
    scene: &str,
    key: &str,
    index: usize,
    row: &Value,
    target: u32,
    duration: u32,
) -> Result<Value> {
    let duration = integer(row, "duration_ms", duration)?;
    let mut data = event(
        scene,
        key,
        &format!("actor_look_at:{index}"),
        "scneventsPlayerLookAtEvent",
        integer(row, "start_ms", 0)?,
        // Native player look-at events issue a one-shot camera request. Its
        // lifetime belongs to lookAtParams, not a sustained timeline event.
        0,
    );
    data["performer"] = performer(target)?;
    data["lookAtParams"] = json!({
        "$type":"scneventsPlayerLookAtEventParams",
        "slotName":n::cname(row.get("target_slot").and_then(Value::as_str).unwrap_or("Head")),
        "offsetPos":{"$type":"Vector3","X":0,"Y":0,"Z":0},
        "duration":0.25,"maxDuration":f64::from(duration)/1000.,
        "adjustPitch":1,"adjustYaw":1,"easeIn":1,"easeOut":1,
        "endOnCameraInputApplied":1,"cameraInputMagToBreak":0.2,
        "endOnTargetReached":1,"endOnTimeExceeded":1,"precision":0.1
    });
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_gaze_uses_camera_event_and_npc_gaze_uses_skeletal_event() {
        let actors = BTreeMap::from([("mara".into(), 0), ("v".into(), 2), ("velvet".into(), 1)]);
        let player = look_at(
            "arrival",
            "opening",
            0,
            &json!({"actor":"v","target":"velvet","start_ms":1450,"duration_ms":2080}),
            &actors,
            Some(2),
            16000,
        )
        .unwrap();
        assert_eq!(player["$type"], "scneventsPlayerLookAtEvent");
        assert_eq!(
            player["performer"]["id"], 257,
            "player event performer identifies its target"
        );
        assert_eq!(player["startTime"], 1450);
        assert_eq!(player["duration"], 0);
        assert_eq!(player["lookAtParams"]["maxDuration"], 2.08);
        assert_eq!(player["lookAtParams"]["endOnTargetReached"], 1);
        assert_eq!(player["lookAtParams"]["endOnCameraInputApplied"], 1);
        assert_eq!(player["lookAtParams"]["endOnTimeExceeded"], 1);
        assert_eq!(player["lookAtParams"]["cameraInputMagToBreak"], 0.2);
        assert!(player.get("basicData").is_none());
        let npc = look_at(
            "arrival",
            "opening",
            1,
            &json!({"actor":"mara","target":"v","duration_ms":1200}),
            &actors,
            Some(2),
            16000,
        )
        .unwrap();
        assert_eq!(npc["$type"], "scnLookAtEvent");
        assert_eq!(npc["duration"], 1200);
        assert_eq!(npc["basicData"]["basic"]["performerId"]["id"], 1);
        assert_eq!(npc["basicData"]["basic"]["targetPerformerId"]["id"], 513);
        let second_npc = look_at(
            "arrival",
            "opening",
            2,
            &json!({"actor":"velvet","target":"v"}),
            &actors,
            Some(2),
            1200,
        )
        .unwrap();
        assert_eq!(second_npc["$type"], "scnLookAtEvent");
    }
}
