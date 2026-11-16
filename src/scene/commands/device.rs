//! Native device actions and acknowledged device conditions inside a scene.
use super::*;

fn properties(v: &Value, condition: bool) -> Result<Vec<Value>> {
    let Some(properties) = v.get("properties") else {
        return Ok(Vec::new());
    };
    properties.as_object().context("device properties must be an object")?
        .iter().map(|(name, property)| {
            let kind = string(property,"type")?;
            let value = property.get("value").context("device property value")?;
            ensure!(match kind {
                "Int32" => value.as_i64().is_some_and(|v| i32::try_from(v).is_ok()),
                "Bool" => value.is_boolean(),
                "Float" => value.is_number(),
                "CName" | "String" => value.is_string(),
                _ => false,
            }, "invalid {kind} device property {name}");
            Ok(json!({
                "$type":if condition {"questDevice_ConditionFunctionParameter"} else {"questDeviceManager_ActionProperty"},
                "name":n::cname(name),
                "value":{"$type":"Variant","value_type":kind,"value":value}
            }))
        }).collect()
}

pub(super) fn action(v: &Value, h: &mut Handles) -> Result<Command> {
    let params = h.wrap(json!({
        "$type":"questDeviceManager_NodeTypeParams",
        "objectRef":n::node_ref(string(v,"device_ref")?),
        "deviceControllerClass":n::cname(string(v,"controller_class")?),
        "deviceAction":n::cname(string(v,"action")?),
        "actionProperties":properties(v,false)?
    }));
    let kind = h.wrap(json!({"$type":"questDeviceManager_NodeType","params":[params]}));
    Ok(Command::new(
        "questInteractiveObjectManagerNodeDefinition",
        json!({"type":kind}),
    ))
}

pub(super) fn condition(v: &Value, h: &mut Handles) -> Result<Command> {
    let kind = h.wrap(json!({
        "$type":"questDevice_ConditionType",
        "objectRef":n::node_ref(string(v,"device_ref")?),
        "deviceControllerClass":n::cname(string(v,"controller_class")?),
        "deviceConditionFunction":n::cname(string(v,"condition")?),
        "functionParameters":properties(v,true)?
    }));
    let condition = h.wrap(json!({"$type":"questObjectCondition","type":kind}));
    let snapshot = flag(v, "snapshot", false)? != 0;
    let mut command = Command::new(
        if snapshot { "questConditionNodeDefinition" } else { "questPauseConditionNodeDefinition" },
        json!({"condition":condition}),
    );
    if snapshot {
        command.outputs = vec![("True".into(), "on_true".into()), ("False".into(), "on_false".into())];
    }
    Ok(command)
}
