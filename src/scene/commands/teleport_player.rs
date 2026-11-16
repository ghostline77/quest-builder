use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    teleport::build(v, h, true)
}
