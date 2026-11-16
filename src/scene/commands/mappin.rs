use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let path = journal_path(v, h)?;
    let mut command = Command::new(
        "questMappinManagerNodeDefinition",
        json!({"disablePreviousMappins":flag(v,"disable_previous_mappins",false)?,"path":path}),
    );
    command.inputs = vec!["CutDestination".into(), "Active".into(), "Inactive".into()];
    Ok(command)
}
