//! Native daily clock-window conditions, optionally combined with an arrival trigger.
use super::*;

pub(in crate::scene) fn condition(v: &Value, h: &mut Handles) -> Result<Value> {
    let begin = integer(v, "begin_seconds", 0)?;
    let end = integer(v, "end_seconds", 86399)?;
    ensure!(
        begin <= end && end < 86400,
        "time period must fit within one day"
    );
    let kind = h.wrap(json!({"$type":"questTimePeriod_ConditionType",
        "begin":{"$type":"GameTime","seconds":begin},
        "end":{"$type":"GameTime","seconds":end}}));
    Ok(h.wrap(json!({"$type":"questTimeCondition","type":kind})))
}

pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let mut condition = condition(v, h)?;
    if let Some(trigger) = v.get("trigger_ref").and_then(Value::as_str) {
        let arrival = h.wrap(json!({"$type":"questTriggerCondition",
            "activatorRef":n::entity(None,&[]),"isPlayerActivator":1,
            "triggerAreaRef":n::node_ref(trigger),"type":"IsInside"}));
        condition = h.wrap(json!({"$type":"questLogicalCondition",
            "operation":"AND","conditions":[condition,arrival]}));
    }
    Ok(Command::new(
        "questPauseConditionNodeDefinition",
        json!({"condition":condition}),
    ))
}
