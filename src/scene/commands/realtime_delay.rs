use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    ensure!(
        v.get("duration_ms").is_none()
            || ["hours", "minutes", "seconds", "milliseconds"]
                .iter()
                .all(|key| v.get(key).is_none()),
        "delay must use duration_ms or time components"
    );
    let (hours, minutes, seconds, milliseconds) = if v.get("duration_ms").is_some() {
        let duration = integer(v, "duration_ms", 0)?;
        (
            duration / 3_600_000,
            (duration / 60_000) % 60,
            (duration / 1000) % 60,
            duration % 1000,
        )
    } else {
        (
            integer(v, "hours", 0)?,
            integer(v, "minutes", 0)?,
            integer(v, "seconds", 0)?,
            integer(v, "milliseconds", 0)?,
        )
    };
    let kind=h.wrap(json!({"$type":"questRealtimeDelay_ConditionType","hours":hours,"miliseconds":milliseconds,"minutes":minutes,"seconds":seconds}));
    let condition = h.wrap(json!({"$type":"questTimeCondition","type":kind}));
    Ok(Command::new(
        "questPauseConditionNodeDefinition",
        json!({"condition":condition}),
    ))
}
