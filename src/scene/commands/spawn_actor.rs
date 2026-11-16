use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let action = v
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("Activate");
    ensure!(
        ["Activate", "Deactivate"].contains(&action),
        "invalid spawn action"
    );
    let kind=h.wrap(json!({"$type":"questScene_NodeType","action":action,"entityReference":dynamic_entity(string(v,"dynamic_name")?)}));
    Ok(Command::new(
        "questSpawnManagerNodeDefinition",
        json!({"actions":[{"$type":"questSpawnManagerNodeActionEntry","type":kind}]}),
    ))
}
