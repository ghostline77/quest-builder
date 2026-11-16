use super::*;

fn fields(value: &Value, allowed: &[&str], context: &str) -> Result<()> {
    for key in value
        .as_object()
        .with_context(|| format!("{context} must be an object"))?
        .keys()
    {
        ensure!(
            allowed.contains(&key.as_str()),
            "unsupported {context} field {key}"
        );
    }
    Ok(())
}
pub(super) fn specification(spec: &Value) -> Result<()> {
    fields(
        spec,
        &[
            "name",
            "title",
            "description",
            "archive_path",
            "raw_path",
            "manifest",
            "exported_datetime",
            "scene_category",
            "speaker_distance_interruptions",
            "actors",
            "props",
            "spoken_line_order",
            "holocall_speakers",
            "choice_line_order",
            "choice_locales",
            "entry_point",
            "entry_points",
            "exit_points",
            "start_node",
            "start_nodes",
            "graph_order",
            "sections",
            "choices",
            "quest_nodes",
            "performance_blocks",
            "player_couch_sits",
            "xor_nodes",
            "rid_resources",
            "rid_animations",
            "rid_camera_animations",
            "rid_anim_sets",
            "rid_facial_anim_sets",
            "end_node",
            "end_nodes",
            "lipsync_animset",
            "line_gap_ms",
            "default_line_duration_ms",
            "section_tail_padding_ms",
            "workspots",
        ],
        "scene",
    )?;
    let mut keys = HashSet::new();
    for section in list(spec, "sections")? {
        let key = string(section, "key")?;
        ensure!(keys.insert(key), "duplicate section key {key}");
        fields(
            section,
            &[
                "key",
                "node_id",
                "lines",
                "line_gap_ms",
                "rid_events",
                "rid_camera",
                "camera_event",
                "rid_duration_ms",
                "tail_padding_ms",
                "section_duration_ms",
                "look_at_events",
                "on_end",
                "on_cancel",
            ],
            "section",
        )?;
    }
    for block in list(spec, "performance_blocks")? {
        if block["kind"] == "pose_beat" {
            fields(
                block,
                &[
                    "key",
                    "kind",
                    "preset",
                    "section",
                    "actors",
                    "start_ms",
                    "anchor_line",
                    "root_motion",
                    "on",
                    "ignore_ports",
                ],
                "performance",
            )?;
        }
    }
    for command in list(spec, "quest_nodes")? {
        let common = [
            "key",
            "kind",
            "node_id",
            "on_out",
            "on_success",
            "on_work_started",
            "on_true",
            "on_false",
        ];
        let specific = match string(command, "kind")? {
            "fact" => &["fact", "value"][..],
            "fact_condition" => &["fact", "comparison", "value"],
            "wait_fact" => &["fact", "comparison", "value"],
            "use_scene_workspot" => &[
                "is_player",
                "player_params",
                "dynamic_name",
                "entity_ref",
                "entry",
                "workspot_id",
                "entry_id",
                "finish_animation",
                "jump_to_entry",
                "enable_idle_mode",
                "repeat_on_interrupt",
                "max_anim_time_limit",
            ],
            "phase" => &["phase_resource", "input", "output", "wait_for_completion"],
            "puppet_ai" => &["entity_ref", "entry", "ai_tier"][..],
            "move_on_spline" => &["entity_ref", "entry", "spline_ref", "reverse", "movement_type", "catch_up_with_companion", "match_player_speed", "with_companion"],
            "move_to" => &["entity_ref", "entry", "destination_ref", "movement_type", "distance"],
            "face_player" => &["entity_ref", "entry"],
            "device_action" => &["device_ref", "controller_class", "action", "properties"],
            "device_condition" => &["device_ref", "controller_class", "condition", "properties", "snapshot"],
            "ui_context" => &["state", "active"],
            "elevator_attachment" => &["entity_ref", "entry", "elevator_ref", "attach"],
            "appearance" => &["entity_ref", "entry", "appearance"],
            "use_workspot" | "stop_workspot" => &[
                "is_player",
                "entity_ref",
                "entry",
                "workspot_ref",
                "function",
                "enable_idle_mode",
                "entry_id",
                "entry_tag",
                "exit_animation",
                "exit_entry_id",
                "finish_animation",
                "force_entry_animation",
                "instant",
                "is_workspot_infinite",
                "jump_to_entry",
                "max_anim_time_limit",
                "movement_type",
                "repeat_on_interrupt",
                "teleport",
            ],
            "scene_tier" => &["tier", "force_empty_hands"],
            "equip_player" | "equip_actor" | "unequip_player" | "unequip_actor" => &[
                "entity_ref",
                "entry",
                "item_id",
                "slot_id",
                "by_item",
                "equip_duration_override",
                "instant",
                "operation",
                "unequip_duration_override",
                "unequip_types",
            ],
            "teleport_player" | "teleport_actor" => &[
                "entity_ref",
                "entry",
                "destination_ref",
                "heal",
                "look_at_ref",
                "look_at_entry",
            ],
            "realtime_delay" => &["hours", "minutes", "seconds", "milliseconds", "duration_ms"],
            "render_fade" => &["duration_seconds", "fade_in"],
            "set_time" => &["hours", "minutes", "seconds", "source"],
            "time_period" => &["begin_seconds", "end_seconds", "trigger_ref"],
            "player_look_at" => &[
                "duration_seconds",
                "max_duration_seconds",
                "target_ref",
                "target_entry",
                "offset",
                "adjust_pitch",
                "adjust_yaw",
                "camera_input_magnitude_to_break",
                "ease_in",
                "ease_out",
                "end_on_camera_input",
                "end_on_target_reached",
                "end_on_time_exceeded",
                "precision",
                "slot",
            ],
            "player_status_effect" => &["set", "status_effect"],
            "pause_condition" => &[
                "trigger_ref",
                "require_player_not_in_combat",
                "entity_ref",
                "entry",
            ],
            "character_gender" => &["gender", "is_player", "object_ref"],
            "spawn_actor" => &["dynamic_name", "action"],
            "wait_actor_spawned" => &["dynamic_name", "entity_ref", "entry", "present"],
            "show_player" => &["show"],
            "journal" => &[
                "path",
                "class_name",
                "file_entry_index",
                "journal_type",
                "optional",
                "send_notification",
                "track_quest",
                "version",
                "inputs",
            ],
            "mappin" => &[
                "path",
                "class_name",
                "file_entry_index",
                "disable_previous_mappins",
            ],
            kind => anyhow::bail!("unsupported scene command {kind}"),
        };
        let allowed = common
            .into_iter()
            .chain(specific.iter().copied())
            .collect::<Vec<_>>();
        fields(command, &allowed, "scene command")?;
    }
    Ok(())
}

pub(super) fn references(root: &Value) -> Result<()> {
    let references = &root["resouresReferences"];
    let resources = list(root, "ridResources")?
        .iter()
        .map(|v| integer(&v["id"], "id", u32::MAX))
        .collect::<Result<HashSet<_>>>()?;
    for key in ["ridAnimations", "ridCameraAnimations"] {
        for animation in list(references, key)? {
            let resource = integer(&animation["resourceId"], "id", u32::MAX)?;
            ensure!(
                resources.contains(&resource),
                "{key} refers to missing RID resource {resource}"
            );
        }
    }
    for (key, target) in [
        ("ridAnimSets", "ridAnimations"),
        ("ridFacialAnimSets", "ridAnimations"),
    ] {
        for set in list(references, key)? {
            for animation in list(set, "animations")? {
                let index = integer(animation, "id", u32::MAX)?;
                ensure!(
                    (index as usize) < list(references, target)?.len(),
                    "{key} contains missing animation reference {index}"
                );
            }
        }
    }
    for actor in list(root, "actors")?
        .iter()
        .chain(list(root, "playerActors")?)
    {
        for (field, table) in [
            ("animSets", "ridAnimSets"),
            ("facialAnimSets", "ridFacialAnimSets"),
            ("bodyCinematicAnimSets", "cinematicAnimSets"),
        ] {
            for reference in list(actor, field)? {
                let index = integer(reference, "id", u32::MAX)?;
                ensure!(
                    (index as usize) < list(references, table)?.len(),
                    "actor {field} contains missing reference {index}"
                );
            }
        }
    }
    let mut event_ids = HashSet::new();
    for node in list(&root["sceneGraph"]["Data"], "graph")? {
        for event in list(&node["Data"], "events")? {
            let event = string(&event["Data"]["id"], "id")?;
            ensure!(
                event_ids.insert(event),
                "duplicate generated scene event ID {event}"
            );
        }
    }
    Ok(())
}
