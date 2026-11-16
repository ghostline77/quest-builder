use super::*;
use presets::Beat;

pub(super) fn marker(reference: &str) -> Value {
    json!({"$type":"scnMarker","entityRef":n::entity(None,&[]),"isMounted":1,"localMarkerId":n::cname("None"),"nodeRef":n::node_ref(reference),"slotName":n::cname("None"),"type":"Global"})
}
pub(super) fn transform(value: Option<&Value>) -> Result<Value> {
    if let Some(value) = value {
        for (field, axes) in [
            ("position", &["X", "Y", "Z", "W"][..]),
            ("orientation", &["i", "j", "k", "r"][..]),
        ] {
            for axis in axes {
                number(&value[field], axis, 0.)?;
                ensure!(
                    value[field].get(*axis).is_some(),
                    "transform requires {field}.{axis}"
                );
            }
        }
        let q = &value["orientation"];
        ensure!(
            ["i", "j", "k", "r"]
                .iter()
                .map(|a| q[*a].as_f64().unwrap_or(0.).powi(2))
                .sum::<f64>()
                > 0.,
            "root-motion quaternion is zero"
        );
        let mut result = value.clone();
        result["$type"] = json!("Transform");
        result["position"]["$type"] = json!("Vector4");
        result["orientation"]["$type"] = json!("Quaternion");
        Ok(result)
    } else {
        Ok(
            json!({"$type":"Transform","orientation":{"$type":"Quaternion","i":0,"j":0,"k":0,"r":1},"position":{"$type":"Vector4","X":0,"Y":0,"Z":0,"W":0}}),
        )
    }
}
fn event(
    spec: &Value,
    key: &str,
    index: usize,
    kind: &str,
    start: u32,
    duration: u32,
) -> Result<Value> {
    Ok(
        json!({"$type":kind,"duration":duration,"executionTagFlags":0,"id":event_id(&[string(spec,"name")?,key,"performance",&index.to_string()]),"scalingData":null,"startTime":start}),
    )
}
fn anim_data(scale: f64, clip_end: f64) -> Value {
    json!({"$type":"scneventsPlayAnimEventExData","basic":{"$type":"scneventsPlayAnimEventData","blendIn":0.5,"blendInCurve":"SinusoidalEaseInOut","blendOut":0.5,"blendOutCurve":"SinusoidalEaseInOut","clipEnd":clip_end,"clipFront":0,"stretch":scale},"bodyPartMask":n::cname("None"),"weight":1})
}
fn scaling(h: &mut Handles, offset: u32, duration: u32) -> Result<Value> {
    Ok(h.wrap(json!({"$type":"scnScalingData_KeepRelationWithOtherEvents","groupRfrncNdspaceEndtime":{"$type":"scnSceneTime","stu":offset.checked_add(duration).context("performance group end overflow")?},"groupRfrncNdspaceStarttime":{"$type":"scnSceneTime","stu":offset}})))
}

#[expect(
    clippy::too_many_arguments,
    reason = "all explicit bindings belong to a single performance compilation context"
)]
pub(super) fn build<'a>(
    spec: &Value,
    block: &Value,
    key: &str,
    index: usize,
    beat: &Beat,
    offset: u32,
    bound: &BTreeMap<&str, (&str, u32)>,
    workspots: &HashSet<u32>,
    used_motion: &mut HashSet<&'a str>,
    h: &mut Handles,
) -> Result<Value>
where
    Beat: 'a,
{
    match beat {
        Beat::Body {
            role,
            start,
            duration,
            clip,
            motion,
        } => {
            let start = offset.checked_add(*start).context("body start overflow")?;
            let mut data = event(spec, key, index, "scnPlaySkAnimEvent", start, *duration)?;
            let animation = h.wrap(
                json!({"$type":"scnAnimName","type":"direct","unk1":[n::cname(clip)],"unk2":[]}),
            );
            let gameplay = if *motion {
                if clip.starts_with("walk_") {
                    "fpp_idle_stand"
                } else {
                    "stand__2h_on_sides__01"
                }
            } else {
                "None"
            };
            let gameplay=h.wrap(json!({"$type":"scnAnimName","type":"direct","unk1":[n::cname(gameplay)],"unk2":[]}));
            data.as_object_mut().context("body event")?.extend(json!({"actorComponent":n::cname("body"),"animData":anim_data(1.,0.),"animName":animation,"blendOverride":"CopyPitch_CopyYaw","cameraBlendInDuration":1,"cameraBlendOutDuration":1,"cameraParallaxSpace":"Trajectory","cameraParallaxWeight":0,"cameraUseTrajectorySpace":1,"convertToAdditive":0,"enableWorldSpaceSmoothing":1,"eyesBlendAdditive":1,"FPPControlActive":0,"gameplayAnimName":gameplay,"genderSpecificParams":[],"idleIsMountedWorkspot":0,"isSceneCarrying":0,"lowerFaceBlendAdditive":1,"muteAnimEvents":"0","neckWeight":1,"performer":performer(bound[role].1)?,"pitchLimitBottom":0,"pitchLimitTop":0,"playerData":{"$type":"scnPlayerAnimData","isEndOfCarryingAnimation":0,"tierData":null,"unmountBodyCarry":1,"useZSnapping":0},"poseBlendOutWorkspot":null,"stayInScene":0,"upperFaceBlendAdditive":1,"vehicleProceduralCameraWeight":0,"yawLimitLeft":0,"yawLimitRight":0}).as_object().context("body fields")?.clone());
            let mut origin = marker("");
            let mut placement = transform(None)?;
            if *motion {
                let binding = block
                    .get("root_motion")
                    .and_then(|v| v.get(*role))
                    .with_context(|| format!("root-motion placement required for {role}"))?;
                used_motion.insert(role);
                let reference = string(binding, "origin")?;
                ensure!(
                    reference.starts_with('#') || reference.starts_with("$/"),
                    "root motion requires an explicit origin"
                );
                origin = marker(reference);
                placement = transform(Some(
                    binding
                        .get("offset")
                        .context("explicit root-motion transform required")?,
                ))?;
                let workspot = integer(binding, "workspot_id", u32::MAX)?;
                ensure!(
                    workspots.contains(&workspot),
                    "root-motion handoff must bind a generated scene workspot"
                );
                data["poseBlendOutWorkspot"]=h.wrap(json!({"$type":"scnEventBlendWorkspotSetupParameters","idleOnlyMode":1,"itemOverride":{"$type":"workWorkspotItemOverride","itemOverrides":[],"propOverrides":[]},"sequenceEntryId":{"$type":"workWorkEntryId","id":integer(binding,"sequence_entry_id",2)?},"workExcludedGestures":[],"workspotId":id("scnSceneWorkspotInstanceId",workspot)}));
                data["scalingData"] = scaling(h, offset, *duration)?;
                data["playerData"]["tierData"]=h.wrap(json!({"$type":"gameSceneTier4Data","emptyHands":1,"params":{"$type":"gameMotionConstrainedTierDataParams","adjustingDuration":0,"adjustingSpeed":0,"notificationBackwardIndex":0,"splineRef":n::node_ref(""),"travellingDuration":0,"travellingSpeed":0},"tier":"Tier4_FPPCinematic","userDebugInfo":format!("{key}: generated workspot handoff")}));
            }
            data["rootMotionData"] = json!({"$type":"scnPlaySkAnimRootMotionData","customBlendInCurve":"SinusoidalEaseInOut","customBlendInTime":-1,"enabled":u8::from(*motion),"meshDissolvingEnabled":1,"originMarker":origin,"originOffset":placement,"placementMode":"Blend","removePitchRollRotation":1,"snapToGround":0,"snapToGroundEnd":0,"snapToGroundStart":0,"trajectoryLOD":[],"vehicleChangePhysicsState":1,"vehicleEnabledPhysicsOnEnd":1});
            Ok(data)
        }
        Beat::Facial {
            start,
            duration,
            emotion,
            expression,
            scale,
            clip_end,
        } => {
            let mut data = event(
                spec,
                key,
                index,
                "scnChangeIdleAnimEvent",
                offset
                    .checked_add(*start)
                    .context("facial start overflow")?,
                *duration,
            )?;
            let facial_key = |gender| {
                if let Some(expression) = expression {
                    n::cname(&format!("idle__{emotion}__{gender}__{expression}"))
                } else {
                    n::cname("None")
                }
            };
            data.as_object_mut().context("facial event")?.extend(json!({"actorComponent":n::cname("head"),"addIdleAnimName":n::cname("None"),"animData":anim_data(*scale,*clip_end),"animName":n::cname("None"),"bakedFacialTransition":{"$type":"animFacialEmotionTransitionBaked","customTransitionAnim":n::cname("None"),"facialKey_Female":facial_key("female"),"facialKey_Male":facial_key("male"),"facialKeyWeight":1,"timeScale":scale,"toIdleFemale":n::cname(&format!("idle__{emotion}__female")),"toIdleMale":n::cname(&format!("idle__{emotion}__male")),"toIdleNeckWeight":0.75,"toIdleWeight":1,"transitionDuration":f64::from(*duration)/1000.,"transitionType":"Natural"},"convertToAdditive":0,"eyesBlendAdditive":1,"facialInstantTransition":0,"idleAnimName":n::cname("None"),"isEnabled":1,"lowerFaceBlendAdditive":1,"muteAnimEvents":"0","neckWeight":1,"performer":performer(bound["speaker"].1)?,"upperFaceBlendAdditive":1}).as_object().context("facial fields")?.clone());
            Ok(data)
        }
        Beat::Look {
            actor,
            target,
            start,
            chest,
        } => {
            let actors = bound
                .values()
                .map(|(name, id)| (name.to_lowercase(), *id))
                .collect();
            let row = json!({"actor":bound[actor].0,"target":bound[target].0,"start_ms":offset.checked_add(*start).context("gaze start overflow")?,"duration_ms":1000});
            let mut data =
                timeline::look_at(string(spec, "name")?, key, index, &row, &actors,
                    list(spec, "actors")?.iter().find(|actor| actor["kind"] == "player")
                        .map(|actor| integer(actor, "id", u32::MAX)).transpose()?, 1000)?;
            data["id"] = event_id(&[
                string(spec, "name")?,
                key,
                "performance",
                &index.to_string(),
            ]);
            let request = &mut data["basicData"]["requests"][0]["request"];
            request["limits"]["softLimitDegrees"] = json!(360);
            request["transitionSpeed"] = json!(if chest.is_some() { 80 } else { 60 });
            if let Some(chest) = chest {
                request["additionalParts"]["Elements"][0]["weight"] = json!(0.100000024);
                request["additionalParts"]["Elements"][1]["weight"] = json!(chest);
            } else {
                request["additionalParts"]["Elements"] = json!([]);
            }
            Ok(data)
        }
        Beat::Correction {
            start,
            duration,
            bones,
        } => {
            let mut data = event(
                spec,
                key,
                index,
                "scnPoseCorrectionEvent",
                offset
                    .checked_add(*start)
                    .context("correction start overflow")?,
                *duration,
            )?;
            let quaternion =
                |q: [f64; 4]| json!({"$type":"Quaternion","i":q[0],"j":q[1],"k":q[2],"r":q[3]});
            let corrections = if bones.is_empty() {
                vec![]
            } else {
                vec![
                    json!({"$type":"animPoseCorrection","boneCorrections":{"Elements":bones.iter().map(|b|json!({"$type":"animBoneCorrection","additiveCorrection":quaternion(b.rotation),"boneName":n::cname(b.name)})).collect::<Vec<_>>()},"compareBones":{"Elements":bones.iter().map(|b|json!({"$type":"animCompareBone","boneName":n::cname(b.name),"boneRotationLs":quaternion(b.reference)})).collect::<Vec<_>>()},"rbfCoefficient":3.5,"rbfPowValue":20}),
                ]
            };
            data["performerId"] = performer(bound["speaker"].1)?;
            data["poseCorrectionGroup"] = json!({"$type":"animPoseCorrectionGroup","poseCorrections":{"Elements":corrections}});
            Ok(data)
        }
        Beat::Signal { .. } => anyhow::bail!("signals need an output socket binding"),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "a timeline signal binds identity, time range and its output socket"
)]
pub(super) fn signal(
    spec: &Value,
    key: &str,
    index: usize,
    beat: &Beat,
    offset: u32,
    socket: u32,
    duration: u32,
    h: &mut Handles,
) -> Result<Value> {
    let Beat::Signal { start, .. } = beat else {
        anyhow::bail!("not a signal beat")
    };
    let mut data = event(
        spec,
        key,
        index,
        "scneventsSocket",
        offset
            .checked_add(*start)
            .context("signal start overflow")?,
        0,
    )?;
    data["osockStamp"] = json!({"$type":"scnOutputSocketStamp","name":socket,"ordinal":0});
    data["scalingData"] = scaling(h, offset, duration)?;
    Ok(data)
}
