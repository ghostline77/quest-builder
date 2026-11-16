use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let path = journal_path(v, h)?;
    let kind = if v.get("journal_type").and_then(Value::as_str) == Some("quest_entry") {
        h.wrap(json!({"$type":"questJournalQuestEntry_NodeType","optional":flag(v,"optional",false)?,"path":path,"sendNotification":flag(v,"send_notification",true)?,"trackQuest":flag(v,"track_quest",false)?,"version":v.get("version").and_then(Value::as_str).unwrap_or("Initial")}))
    } else {
        h.wrap(json!({"$type":"questJournalEntry_NodeType","path":path,"sendNotification":flag(v,"send_notification",true)?}))
    };
    let mut command = Command::new("questJournalNodeDefinition", json!({"type":kind}));
    command.inputs = vec!["CutDestination".into()];
    if v.get("inputs").is_some() {
        for input in list(v, "inputs")? {
            command
                .inputs
                .push(input.as_str().context("journal input name")?.into());
        }
    } else {
        command.inputs.extend(["Active".into(), "Inactive".into()]);
    }
    Ok(command)
}
