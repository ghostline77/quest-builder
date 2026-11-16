use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let mut spec = v.clone();
    spec["function"] = json!("StopWorkspot");
    spec["teleport"] = json!(false);
    use_workspot::build(&spec, h)
}
