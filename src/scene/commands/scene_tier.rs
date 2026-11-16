use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let kind=h.wrap(json!({"$type":"questSetTier_NodeType","forceEmptyHands":flag(v,"force_empty_hands",false)?,"motionConstrainedTierDataParams":{"$type":"gameMotionConstrainedTierDataParams","adjustingDuration":0,"adjustingSpeed":0,"notificationBackwardIndex":0,"splineRef":n::node_ref(""),"travellingDuration":0,"travellingSpeed":0},"tier":string(v,"tier")?,"useEnterAnim":0,"useExitAnim":0,"usePlayerWorkspot":0}));
    Ok(Command::new(
        "questSceneManagerNodeDefinition",
        json!({"type":kind}),
    ))
}
