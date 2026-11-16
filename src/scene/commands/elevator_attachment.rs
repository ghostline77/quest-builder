use super::*;

pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let params = json!({
        "$type":"questElevator_ManageNPCAttachment_NodeTypeParams",
        "elevatorRef":n::node_ref(string(v,"elevator_ref")?),
        "npcRef":actor_entity(v)?,
        "action":if flag(v,"attach",true)? == 1 {"Attach"} else {"Detach"}
    });
    let kind =
        h.wrap(json!({"$type":"questElevator_ManageNPCAttachment_NodeType","params":[params]}));
    Ok(Command::new(
        "questInteractiveObjectManagerNodeDefinition",
        json!({"type":kind}),
    ))
}
