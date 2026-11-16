use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    equipment::build(v, h, true, true)
}
