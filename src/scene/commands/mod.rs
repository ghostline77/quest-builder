//! Scene-local quest commands, each with its own native constructor.

use super::*;
mod appearance;
mod character_gender;
mod device;
mod elevator_attachment;
mod equip_actor;
mod equip_player;
mod equipment;
mod face_player;
mod fact;
mod fact_condition;
mod journal;
mod mappin;
mod move_on_spline;
mod move_to;
mod pause_condition;
mod phase;
mod player_look_at;
mod player_status_effect;
mod puppet_ai;
mod realtime_delay;
mod render_fade;
mod scene_tier;
mod set_time;
mod show_player;
mod spawn_actor;
mod stop_workspot;
mod teleport;
mod teleport_actor;
mod teleport_player;
pub(super) mod time_period;
mod unequip_actor;
mod unequip_player;
mod use_scene_workspot;
mod use_workspot;
mod wait_actor_spawned;
mod wait_fact;

struct Command {
    quest: Value,
    inputs: Vec<String>,
    outputs: Vec<(String, String)>,
}
impl Command {
    fn new(kind: &str, fields: Value) -> Self {
        let mut quest = fields;
        quest["$type"] = json!(kind);
        Self {
            quest,
            inputs: vec!["CutDestination".into(), "In".into()],
            outputs: vec![("Out".into(), "on_out".into())],
        }
    }
    fn wrap(mut self, v: &Value, h: &mut Handles) -> Result<Value> {
        let node = integer(v, "node_id", u32::MAX)?;
        ensure!(node != u32::MAX, "command needs node_id");
        self.quest["id"] = json!(node);
        let mut sockets = Vec::new();
        for name in &self.inputs {
            sockets.push(h.wrap(json!({"$type":"questSocketDefinition","connections":[],"name":n::cname(name),"type":if name=="CutDestination"{"CutDestination"}else{"Input"}})));
        }
        for (name, _) in &self.outputs {
            sockets.push(h.wrap(json!({"$type":"questSocketDefinition","connections":[],"name":n::cname(name),"type":"Output"})));
        }
        self.quest["sockets"] = json!(sockets);
        let output_sockets = self
            .outputs
            .iter()
            .enumerate()
            .map(|(ordinal, (_, key))| graph::socket(0, u32::try_from(ordinal)?, list(v, key)?))
            .collect::<Result<Vec<_>>>()?;
        let quest = h.wrap(self.quest);
        Ok(h.wrap(json!({"$type":"scnQuestNode","ffStrategy":"automatic","isockMappings":self.inputs.iter().map(|s|n::cname(s)).collect::<Vec<_>>(),"osockMappings":self.outputs.iter().map(|(s,_)|n::cname(s)).collect::<Vec<_>>(),"nodeId":id("scnNodeId",node),"outputSockets":output_sockets,"questNode":quest})))
    }
}
fn command(v: &Value, h: &mut Handles) -> Result<Command> {
    match string(v, "kind")? {
        "appearance" => appearance::build(v, h),
        "character_gender" => character_gender::build(v, h),
        "device_action" => device::action(v, h),
        "device_condition" => device::condition(v, h),
        "ui_context" => {
            let kind = h.wrap(json!({"$type":"questUIContextState_ConditionType", "state":string(v,"state")?, "active":flag(v,"active",false)?}));
            let condition = h.wrap(json!({"$type":"questUICondition", "type":kind}));
            Ok(Command::new("questPauseConditionNodeDefinition",json!({"condition":condition})))
        }
        "elevator_attachment" => elevator_attachment::build(v, h),
        "equip_actor" => equip_actor::build(v, h),
        "equip_player" => equip_player::build(v, h),
        "face_player" => face_player::build(v, h),
        "fact" => fact::build(v, h),
        "fact_condition" => fact_condition::build(v, h),
        "journal" => journal::build(v, h),
        "mappin" => mappin::build(v, h),
        "move_on_spline" => move_on_spline::build(v, h),
        "move_to" => move_to::build(v, h),
        "pause_condition" => pause_condition::build(v, h),
        "phase" => phase::build(v, h),
        "player_look_at" => player_look_at::build(v, h),
        "player_status_effect" => player_status_effect::build(v, h),
        "puppet_ai" => puppet_ai::build(v, h),
        "realtime_delay" => realtime_delay::build(v, h),
        "render_fade" => render_fade::build(v, h),
        "scene_tier" => scene_tier::build(v, h),
        "set_time" => set_time::build(v, h),
        "show_player" => show_player::build(v, h),
        "spawn_actor" => spawn_actor::build(v, h),
        "stop_workspot" => stop_workspot::build(v, h),
        "teleport_actor" => teleport_actor::build(v, h),
        "teleport_player" => teleport_player::build(v, h),
        "time_period" => time_period::build(v, h),
        "unequip_actor" => unequip_actor::build(v, h),
        "unequip_player" => unequip_player::build(v, h),
        "use_workspot" => use_workspot::build(v, h),
        "use_scene_workspot" => use_scene_workspot::build(v, h),
        "wait_actor_spawned" => wait_actor_spawned::build(v, h),
        "wait_fact" => wait_fact::build(v, h),
        kind => anyhow::bail!("unsupported scene command {kind}"),
    }
}

pub(super) fn input_names(v: &Value) -> Result<Vec<String>> {
    Ok(command(v, &mut Handles::default())?.inputs)
}

pub(super) fn build(v: &Value, h: &mut Handles) -> Result<Value> {
    command(v, h)?.wrap(v, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_lift_commands_keep_companion_and_boarding_contracts() {
        let actor = json!({"entity_ref":"#contact","entry":"contact"});
        let mut move_spec = actor.clone();
        move_spec["kind"] = json!("move_on_spline");
        move_spec["spline_ref"] = json!("#route");
        let movement = command(&move_spec, &mut Handles::default()).unwrap();
        let params = &movement.quest["nodeParams"]["Data"];
        assert_eq!(movement.quest["$type"], "questMovePuppetNodeDefinition");
        assert_eq!(params["useStart"], false);
        assert_eq!(params["useStop"], false);
        let additional = &params["additionalParams"]["Data"];
        assert_eq!(additional["type"], "WithCompanion");
        let companion = &additional["withCompanionParams"];
        assert_eq!(companion["companionDistancePreset"], "Close");
        assert_eq!(companion["catchUpWithCompanion"], 0);
        assert_eq!(companion["teleportToCompanion"], false);
        // The empty native universal reference uses its local-player default.
        assert_eq!(
            companion["companionRef"]["Data"],
            json!({"$type":"questUniversalRef"})
        );
        move_spec["reverse"] = json!(true);
        assert_eq!(
            command(&move_spec, &mut Handles::default()).unwrap().quest["nodeParams"]["Data"]["reverse"],
            1
        );

        for npc in [false, true] {
            let mut gate = json!({"kind":"pause_condition","trigger_ref":"#cabin"});
            if npc {
                gate["entity_ref"] = actor["entity_ref"].clone();
                gate["entry"] = actor["entry"].clone();
            }
            let gate = command(&gate, &mut Handles::default()).unwrap();
            let condition = &gate.quest["condition"]["Data"];
            assert_eq!(condition["isPlayerActivator"], u8::from(!npc));
            if npc {
                assert_eq!(condition["activatorRef"]["reference"]["$value"], "#contact");
            }
        }
        for floor in [0, 1] {
            let spec = json!({"kind":"device_condition","device_ref":"#lift","controller_class":"LiftControllerPS","condition":"IsAtFloor","properties":{"floorNumber":{"type":"Int32","value":floor}}});
            let gate = command(&spec, &mut Handles::default()).unwrap();
            let condition = &gate.quest["condition"]["Data"]["type"]["Data"];
            assert_eq!(condition["deviceConditionFunction"]["$value"], "IsAtFloor");
            assert_eq!(
                condition["functionParameters"][0]["value"],
                json!({"$type":"Variant","value_type":"Int32","value":floor})
            );
        }
        let invalid = json!({"kind":"device_action","device_ref":"#lift","controller_class":"LiftControllerPS","action":"GoToFloor","properties":{"floorNumber":{"type":"Int32","value":"1"}}});
        assert!(command(&invalid, &mut Handles::default()).is_err());
    }
}

fn journal_path(v: &Value, h: &mut Handles) -> Result<Value> {
    let path = string(v, "path")?;
    let parts = path.split('/').collect::<Vec<_>>();
    let expected = match parts.as_slice() {
        ["quests", "minor_quest" | "vehicle_metaquest", ..] => Some(2),
        ["quests", "street_stories", ..] => Some(4),
        [
            "briefings" | "codex" | "contacts" | "internet_sites" | "onscreens"
            | "points_of_interest" | "tarots",
            ..,
        ] => Some(1),
        _ => None,
    };
    let index = integer(
        v,
        "file_entry_index",
        expected
            .context("unknown journal root requires file_entry_index")
            .or_else(|e| {
                if v.get("file_entry_index").is_some() {
                    Ok(0)
                } else {
                    Err(e)
                }
            })?,
    )?;
    ensure!(
        expected.is_none_or(|e| e == index),
        "journal file_entry_index disagrees with native hierarchy"
    );
    Ok(h.wrap(json!({"$type":"gameJournalPath","className":n::cname(string(v,"class_name")?),"editorPath":"","fileEntryIndex":index,"realPath":path})))
}
