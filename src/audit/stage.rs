//! Checks that declared runtime bindings survive stage generation and extension transforms.
use super::*;

/// Checks typed authoring identities independently of graph construction.
pub fn typed_fields(stage: &Stage, document: &Value) -> Result<()> {
    fn strings<'a>(value: &'a Value, out: &mut HashSet<&'a str>) {
        match value {
            Value::String(value) => {
                out.insert(value);
            }
            Value::Array(values) => {
                for value in values {
                    strings(value, out);
                }
            }
            Value::Object(values) => {
                for value in values.values() {
                    strings(value, out);
                }
            }
            _ => {}
        }
    }
    fn values<'a>(value: &'a Value, path: &[&str], out: &mut Vec<&'a str>) {
        if path.is_empty() {
            if let Some(value) = value.as_str() {
                out.push(value);
            } else if let Some(array) = value.as_array() {
                for value in array {
                    values(value, path, out);
                }
            }
            return;
        }
        if let Some(array) = value.as_array() {
            for value in array {
                values(value, path, out);
            }
        } else if let Some(value) = value.get(path[0]) {
            values(value, &path[1..], out);
        }
    }
    let fields = match stage.kind.as_str() {
        "play_scene" => &[
            "scene",
            "origin",
            "entry",
            "exit",
            "wait_trigger",
            "spawners",
        ][..],
        "meet_contact" => &["contact", "scene", "community"][..],
        "hack_access_point" => &["device", "success_fact", "guard_community", "grants"][..],
        "deliver_drop_point" => &[
            "drop_point",
            "deposit_fact",
            "item_branches.condition",
            "item_branches.item",
        ][..],
        "phone_conversation" => &[
            "choices.set_fact",
            "opening_branches.condition",
            "opening_branches.unless",
            "opening_branches.messages",
            "conditional_message_groups.branches.condition",
            "conditional_message_groups.branches.messages",
            "postscript_messages",
        ][..],
        "reach_area" => &["trigger", "objective", "description_entry", "mappin"][..],
        "interact_device" => &[
            "device",
            "controller_class",
            "completion_function",
            "outcome_branches.condition",
            "outcome_branches.set_fact",
            "outcome_branches.add_items",
            "outcome_branches.remove_items",
        ][..],
        "acquire_item" => &["item"][..],
        "combat_encounter" => &[
            "community",
            "entries",
            "trigger",
            "abort_trigger",
            "setup.trigger",
            "restore.trigger",
        ][..],
        "cyberpsycho_encounter" => &[
            "community",
            "boss_entry",
            "activation_trigger",
            "resolution.spared_fact",
            "resolution.killed_fact",
            "reveal.trigger",
            "arena_trigger",
            "alerted_path",
            "alerted_spots",
            "cleanup.trigger",
            "player_defeat_scene.completion_branches.set_fact",
        ][..],
        "leave_area" => &["trigger", "objective", "cleanup_community"][..],
        "investigate_clues" => &[
            "clues.object_ref",
            "clues.completion_fact",
            "clues.grant_item",
            "clues.grant_items",
            "clues.journal_entry",
            "clues.mappin",
        ][..],
        "optional_condition" => &["success_fact", "failure_fact", "condition.value"][..],
        "choice_gate" => &["branches.condition", "branches.set_fact"][..],
        "escort_npc" => &[
            "community",
            "entry",
            "objective",
            "completion_fact",
            "destinations",
            "route_mappins",
        ][..],
        "carry_npc" => &["community", "entry", "destination", "objective"][..],
        "deliver_vehicle" => &["vehicle", "destination", "objective"][..],
        "read_terminal_document" => &["objective", "completion_fact", "document_entry"][..],
        "stealth_monitor" => &["objective", "failure_fact", "success_fact", "stop_fact"][..],
        "plant_item" => &[
            "item",
            "device",
            "controller_class",
            "action",
            "completion_function",
            "completion_fact",
            "objective",
        ][..],
        "defend_target" => &[
            "community",
            "entry",
            "completion_fact",
            "failure_fact",
            "objective",
        ][..],
        "release_or_rescue_npc" => &[
            "community",
            "entry",
            "device",
            "controller_class",
            "action",
            "completion_function",
            "completion_fact",
            "objective",
        ][..],
        "enter_vehicle" | "steal_vehicle" => {
            &["vehicle_community", "vehicle_entry", "objective", "mappin"][..]
        }
        "ride_with_contact" => &[
            "vehicle_community",
            "vehicle_entry",
            "contact_community",
            "contact_entry",
            "objective",
        ][..],
        "drive_to" => &[
            "vehicle_community",
            "vehicle_entry",
            "destination",
            "completion_fact",
            "objective",
            "mappin",
        ][..],
        "vehicle_cleanup" => &["completion_fact"][..],
        "braindance_analysis" => &[
            "scene",
            "scene_origin",
            "player_anchor",
            "player_return",
            "completion_fact",
            "objective",
            "clue_facts",
        ][..],
        _ => &[][..],
    };
    let data = serde_json::to_value(&stage.fields)?;
    let mut emitted = HashSet::new();
    strings(document, &mut emitted);
    let mut expected = Vec::new();
    for field in fields {
        let mut found = Vec::new();
        values(&data, &field.split('.').collect::<Vec<_>>(), &mut found);
        expected.extend(found.into_iter().map(|v| ((*field).to_owned(), v)));
    }
    let mut additional = Vec::new();
    if stage.kind == "deliver_drop_point" && data.get("item_branches").is_none() {
        additional.push("item");
    }
    if stage.kind == "read_shard" {
        if data.get("acquisition_fact").is_some() {
            additional.push("acquisition_fact");
        } else if stage.flag("grant_item") {
            additional.push("item");
        }
        additional.push("journal_entry");
    }
    if matches!(stage.kind.as_str(), "interact_device" | "hack_access_point")
        && (stage.kind == "interact_device" || data.get("completion_function").is_some())
    {
        additional.extend(["controller_class", "completion_function"]);
        if data["send_action"] != false {
            additional.push("action");
        }
    }
    if ![
        "phone_job_offer",
        "meet_contact",
        "hack_access_point",
        "deliver_drop_point",
        "phone_conversation",
    ]
    .contains(&stage.kind.as_str())
    {
        additional.extend([
            "description_entry",
            "mappin",
            "completion_fact",
            "acquisition_fact",
            "success_fact",
            "failure_fact",
        ]);
    }
    for field in additional {
        if let Some(value) = stage.optional(field) {
            expected.push((field.into(), value));
        }
    }
    let policy = &data["objective_lifecycle"];
    let retained = policy["on_enter"] == "retain"
        && stage
            .outcomes()?
            .keys()
            .all(|outcome| policy[format!("on_{outcome}")] == "retain");
    let missing = expected
        .into_iter()
        .filter(|(field, value)| !(emitted.contains(value) || field == "objective" && retained))
        .map(|(field, value)| format!("{field}={value}"))
        .collect::<Vec<_>>();
    ensure!(
        missing.is_empty(),
        "stage {} lost declared runtime bindings: {}",
        stage.id,
        missing.join(", ")
    );
    Ok(())
}
