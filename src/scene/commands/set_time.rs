//! Advances the native game clock to an authored time of day.
use super::*;

pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let hours = integer(v, "hours", 0)?;
    let minutes = integer(v, "minutes", 0)?;
    let seconds = integer(v, "seconds", 0)?;
    ensure!(
        hours < 24 && minutes < 60 && seconds < 60,
        "invalid time of day"
    );
    let kind = h.wrap(json!({"$type":"questSetTime_NodeType",
        "hours":hours,"minutes":minutes,"seconds":seconds,
        "source":n::cname(string(v,"source")?)}));
    Ok(Command::new(
        "questTimeManagerNodeDefinition",
        json!({"type":kind}),
    ))
}
