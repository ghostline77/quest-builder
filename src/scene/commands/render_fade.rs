use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let duration = number(v, "duration_seconds", 0.)?;
    ensure!(duration >= 0., "fade duration must be nonnegative");
    let kind=h.wrap(json!({"$type":"questSetFadeInOut_NodeType","duration":duration,"fadeColor":{"$type":"Color","Alpha":0,"Blue":0,"Green":0,"Red":0},"fadeIn":flag(v,"fade_in",true)?}));
    Ok(Command::new(
        "questRenderFxManagerNodeDefinition",
        json!({"type":kind}),
    ))
}
