use super::*;
pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let kind=h.wrap(json!({"$type":"questShowWorldNode_NodeType","componentName":n::cname("None"),"isPlayer":1,"objectRef":n::node_ref(""),"show":flag(v,"show",true)?}));
    Ok(Command::new(
        "questWorldDataManagerNodeDefinition",
        json!({"type":kind}),
    ))
}
