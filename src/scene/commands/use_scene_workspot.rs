//! Workspots embedded in the owning scene, without a world community registry.
use super::*;

pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Command> {
    let mut params = h.wrap(json!({
        "$type":"scnUseSceneWorkspotParamsV1",
        "workspotInstanceId":id("scnSceneWorkspotInstanceId",integer(v,"workspot_id",u32::MAX)?),
        "entryId":id("workWorkEntryId",integer(v,"entry_id",2)?),
        "finishAnimation":flag(v,"finish_animation",false)?,
        "jumpToEntry":flag(v,"jump_to_entry",true)?,
        "enableIdleMode":flag(v,"enable_idle_mode",true)?,
        "repeatCommandOnInterrupt":flag(v,"repeat_on_interrupt",false)?,
        "maxAnimTimeLimit":number(v,"max_anim_time_limit",0.)?
    }));
    let actor = if flag(v, "is_player", false)? != 0 {
        params["Data"]["isPlayer"] = json!(1);
        params["Data"]["playerParams"] = v
            .get("player_params")
            .context("player scene workspot requires explicit player_params")?
            .clone();
        n::entity(Some("#player"), &[])
    } else if let Some(name) = v.get("dynamic_name").and_then(Value::as_str) {
        dynamic_entity(name)
    } else {
        actor_entity(v)?
    };
    let mut command = Command::new(
        "questUseWorkspotNodeDefinition",
        json!({"entityReference":actor,"paramsV1":params}),
    );
    command.outputs = vec![
        ("Success".into(), "on_success".into()),
        ("Work Started".into(), "on_work_started".into()),
    ];
    Ok(command)
}
