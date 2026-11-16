//! Cross-field authoring rules; structural types come from the canonical schema.
use super::*;
use std::collections::BTreeSet;

fn rows<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}
fn distinct(rows: &[Value], key: &str) -> bool {
    rows.iter()
        .filter_map(|r| r.get(key))
        .map(Value::to_string)
        .collect::<BTreeSet<_>>()
        .len()
        == rows.len()
}
fn action_resource(action: &Value) -> Option<String> {
    let s = |k| action.get(k).and_then(Value::as_str).unwrap_or("");
    match s("kind") {
        "objective" | "mappin" | "message" => Some(format!("{}:{}", s("kind"), s("path"))),
        "community" => Some(format!("actor:{}:{}", s("community"), s("entry"))),
        "trigger_state" => Some(format!("trigger:{}", s("trigger"))),
        "weather_reset" => Some(format!("weather:{}", s("source"))),
        "prefab_variant" => Some(format!("prefab:{}:{}", s("prefab"), s("variant"))),
        _ => None,
    }
}

pub(super) fn validate(manifest: &Manifest, errors: &mut Vec<Diagnostic>) -> Result<()> {
    for (index, stage) in manifest.stages.iter().enumerate() {
        let data = json!(stage.fields);
        let mut require = |condition: bool, key: &str, message: &str| {
            if !condition {
                errors.push(Diagnostic::error(
                    "stage_semantics",
                    format!("/stages/{index}/{key}"),
                    message,
                ));
            }
        };
        let inputs = stage.inputs()?;
        require(
            inputs.len() == 1 && inputs.contains_key("start"),
            "inputs",
            "Generated blocks accept exactly one start input",
        );
        match stage.kind.as_str() {
            "time_gate" => require(
                ["days", "hours", "minutes", "seconds"]
                    .iter()
                    .any(|k| stage.number(k, 0) > 0),
                "seconds",
                "A time gate needs a nonzero duration",
            ),
            "phone_job_offer" => require(
                stage.number("delay_game_hours", 0) <= i32::MAX as u64,
                "delay_game_hours",
                "Game-hour delay exceeds the native range",
            ),
            "choice_gate" => {
                if stage.optional("gate_kind") == Some("scene_exit") {
                    require(
                        stage.optional("scene").is_some()
                            && stage.optional("scene_origin").is_some(),
                        "scene",
                        "Scene-exit choices need a scene and origin",
                    );
                    require(
                        stage.optional("evaluation") != Some("on_entry")
                            && stage.optional("default_branch").is_none(),
                        "evaluation",
                        "Scene exits are event outcomes, not entry snapshots",
                    );
                }
                let branches = rows(&data, "branches");
                require(
                    distinct(branches, "id") && distinct(branches, "set_fact"),
                    "branches",
                    "Choice IDs and outcome facts must be distinct",
                );
                let fallback = stage.optional("default_branch");
                let evaluation = stage
                    .optional("evaluation")
                    .unwrap_or(if fallback.is_some() {
                        "on_entry"
                    } else {
                        "wait"
                    });
                require(
                    (evaluation == "on_entry") == fallback.is_some(),
                    "evaluation",
                    "Entry evaluation needs a default branch; waiting evaluation cannot have one",
                );
                if let Some(default) = fallback {
                    require(
                        branches.iter().any(|b| b["id"] == default),
                        "default_branch",
                        "Default branch must name an authored branch",
                    );
                }
            }
            "optional_condition" => {
                require(
                    stage.optional("evaluation") != Some("at_exit")
                        || data["condition"].get("stop_fact").is_some(),
                    "condition/stop_fact",
                    "at_exit needs an explicit closing fact for its evaluation window",
                );
                require(
                    data["success_fact"] != data["failure_fact"],
                    "failure_fact",
                    "Success and failure facts must differ",
                );
                let condition = &data["condition"];
                match condition["kind"].as_str() {
                    Some("fact") => require(
                        condition["value"]
                            .as_str()
                            .is_some_and(crate::manifest::identifier),
                        "condition/value",
                        "Fact predicates need a fact identifier",
                    ),
                    Some("trigger") => require(
                        condition["value"]
                            .as_str()
                            .is_some_and(|v| v.starts_with('#') || v.starts_with("$/")),
                        "condition/value",
                        "Trigger predicates need a NodeRef",
                    ),
                    Some("timer") => require(
                        condition["value"]
                            .as_f64()
                            .or_else(|| {
                                condition["value"]
                                    .as_str()
                                    .and_then(|v| v.parse::<f64>().ok())
                            })
                            .is_some_and(|v| v.is_finite() && v >= 0.),
                        "condition/value",
                        "Timer predicates need nonnegative seconds",
                    ),
                    Some("detection" | "alarm") => require(
                        condition["value"].is_string() || condition.get("observer").is_some(),
                        "condition/observer",
                        "Boolean senses/security predicates require an explicit observer",
                    ),
                    _ => {}
                }
            }
            "read_terminal_document" => require(
                stage.optional("output_socket").is_none() || stage.optional("scene").is_some(),
                "output_socket",
                "A terminal scene output socket requires a scene",
            ),
            "investigate_clues" | "investigate_reactions" => {
                let clues = rows(&data, "clues");
                let count = stage.number("required_count", clues.len() as u64);
                require(
                    count > 0 && count <= clues.len() as u64,
                    "required_count",
                    "Required clue count must fit the declared clues",
                );
                for key in ["id", "object_ref"] {
                    require(
                        distinct(clues, key),
                        "clues",
                        "Clue IDs and objects must be distinct so one scan cannot satisfy multiple clues",
                    );
                }
                if stage.kind == "investigate_reactions" {
                    let facts = clues
                        .iter()
                        .flat_map(|c| [c["scan_fact"].clone(), c["completion_fact"].clone()])
                        .collect::<Vec<_>>();
                    require(
                        facts
                            .iter()
                            .map(Value::to_string)
                            .collect::<BTreeSet<_>>()
                            .len()
                            == facts.len(),
                        "clues",
                        "Scan and reaction completion facts must all be distinct",
                    );
                    for clue in clues {
                        if let Some(reaction) = clue.get("reaction") {
                            require(
                                reaction["exits"]
                                    .as_object()
                                    .is_some_and(|v| v.len() == 1 && v.contains_key("success")),
                                "clues",
                                "Clue reactions expose exactly one success exit",
                            );
                        }
                    }
                }
            }
            "interact_device" | "hack_access_point" | "plant_item" | "release_or_rescue_npc" => {
                let branches = rows(&data, "outcome_branches");
                for key in ["id", "condition", "set_fact"] {
                    require(
                        distinct(branches, key),
                        "outcome_branches",
                        "Device outcome IDs, predicates and result facts must be distinct",
                    );
                }
            }
            "deliver_drop_point" => {
                let branches = rows(&data, "item_branches");
                require(
                    stage.optional("item").is_some() || !branches.is_empty(),
                    "item",
                    "Delivery needs an item or item branches",
                );
                for key in ["id", "condition", "item"] {
                    require(
                        distinct(branches, key),
                        "item_branches",
                        "Delivery branch IDs, predicates and items must be distinct",
                    );
                }
            }
            "escort_npc" if data.get("route_mappins").is_some() => {
                require(
                    rows(&data, "route_mappins").len() == rows(&data, "destinations").len(),
                    "route_mappins",
                    "Supply one map pin per escort destination",
                );
            }
            "defend_target" => {
                if let Some(duration) = data.get("duration_seconds") {
                    require(
                        duration
                            .as_f64()
                            .is_some_and(|v| v.is_finite() && v >= 0.001),
                        "duration_seconds",
                        "Defense duration must be finite and at least one millisecond",
                    );
                }
                require(
                    distinct(rows(&data["attackers"], "entries"), "entry"),
                    "attackers",
                    "Attacker entries must be distinct",
                );
                if stage.flag("block_on_failure") {
                    require(
                        stage.flag("retry_checkpoint"),
                        "retry_checkpoint",
                        "Blocking defense failure requires checkpoint recovery",
                    );
                    require(
                        !stage.outcomes()?.contains_key("failure"),
                        "outcomes",
                        "Blocking defense failure cannot also expose a failure port",
                    );
                }
            }
            "combat_encounter" => {
                if stage.optional("hostility") == Some("neutral_to_hostile")
                    || stage.optional("completion") == Some("named_defeated")
                {
                    require(
                        !rows(&data, "entries").is_empty(),
                        "entries",
                        "Named resolution and forced hostility need explicit actor entries",
                    );
                }
                if data.get("threat_function").is_some()
                    || data.get("threat_duration_seconds").is_some()
                {
                    require(
                        !rows(&data, "entries").is_empty(),
                        "entries",
                        "Explicit threat configuration requires named combat entries",
                    );
                }
                if let Some(duration) = data.get("threat_duration_seconds") {
                    require(
                        duration.as_f64().is_some_and(|v| v.is_finite() && v >= 0.),
                        "threat_duration_seconds",
                        "Threat duration must be finite and nonnegative",
                    );
                }
            }
            "meet_contact" => {
                if stage.status == "ready" {
                    for key in ["setup_trigger", "scene_origin", "accepted_fact"] {
                        require(
                            stage.optional(key).is_some(),
                            key,
                            "Ready meetings require explicit world and completion bindings",
                        );
                    }
                }
                require(
                    data["allow_defer"] == false
                        || stage.optional("accept_exit").unwrap_or("job_accept")
                            != stage.optional("defer_exit").unwrap_or("end"),
                    "defer_exit",
                    "Accept and defer exits must differ",
                );
                if stage.optional("defer_policy") == Some("rearm") {
                    require(
                        stage.optional("rearm_trigger").is_some(),
                        "rearm_trigger",
                        "Rearming a meeting requires an explicit re-entry gate",
                    );
                }
            }
            "phone_conversation" => {
                require(
                    !rows(&data, "messages").is_empty()
                        || !rows(&data, "opening_branches").is_empty()
                        || !rows(&data, "conditional_message_groups").is_empty(),
                    "messages",
                    "Phone conversation needs unconditional or conditional opening messages",
                );
                require(
                    distinct(rows(&data, "choices"), "choice"),
                    "choices",
                    "Phone choices must address distinct entries",
                );
                require(
                    distinct(rows(&data, "opening_branches"), "condition"),
                    "opening_branches",
                    "Opening conditions must be distinct",
                );
                let groups = rows(&data, "conditional_message_groups");
                require(
                    distinct(groups, "id"),
                    "conditional_message_groups",
                    "Conditional groups need distinct IDs",
                );
                for group in groups {
                    require(
                        distinct(rows(group, "branches"), "condition"),
                        "conditional_message_groups",
                        "Each group needs distinct branch conditions",
                    );
                }
            }
            "ride_with_contact" => {
                let mut actors = BTreeSet::from([(
                    data["contact_community"].to_string(),
                    data["contact_entry"].to_string(),
                )]);
                let mut slots = BTreeSet::from([stage
                    .optional("contact_slot")
                    .unwrap_or("seat_front_right")
                    .to_owned()]);
                for passenger in rows(&data, "additional_passengers") {
                    require(
                        actors.insert((
                            passenger["community"].to_string(),
                            passenger["entry"].to_string(),
                        )) && slots.insert(passenger["slot"].as_str().unwrap_or("").into()),
                        "additional_passengers",
                        "Passengers require distinct actors and seat slots",
                    );
                }
                require(
                    !(stage.optional("player_role").unwrap_or("Driver") == "Driver"
                        && stage.optional("contact_role").unwrap_or("Passenger") == "Driver"),
                    "contact_role",
                    "Player and contact cannot both require the driver role",
                );
            }
            "cyberpsycho_encounter" => {
                let reveal = &data["reveal"];
                require(
                    reveal.get("trigger").is_some()
                        || [
                            "scan",
                            "attacked_by_boss",
                            "boss_hit_by_player",
                            "boss_sees_player",
                        ]
                        .iter()
                        .any(|k| reveal[*k] == true),
                    "reveal",
                    "Enable at least one reveal route",
                );
                require(
                    data["resolution"]["spared_fact"] != data["resolution"]["killed_fact"],
                    "resolution",
                    "Spared and killed facts must differ",
                );
                if let Some(defeat) = data.get("player_defeat_scene") {
                    let health = defeat
                        .get("health_percent")
                        .and_then(Value::as_f64)
                        .unwrap_or(5.);
                    require(
                        (1. ..=50.).contains(&health),
                        "player_defeat_scene/health_percent",
                        "Defeat health threshold must be between 1 and 50 percent",
                    );
                    require(
                        distinct(rows(defeat, "completion_branches"), "exit"),
                        "player_defeat_scene/completion_branches",
                        "Completion branch exits must be distinct",
                    );
                }
            }
            _ => {}
        }
        let owned = matches!(
            stage.kind.as_str(),
            "owned_scope"
                | "acquire_or_use"
                | "access_routes"
                | "resolve_and_report"
                | "observe_lethal"
                | "reward_commit"
                | "contact_fallback"
                | "adaptive_timer"
                | "entitlement_reconcile"
                | "investigate_reactions"
        );
        if owned {
            let mut expected = BTreeSet::from(["success".to_owned()]);
            let child = match stage.kind.as_str() {
                "owned_scope" if data.get("child").is_some() => Some("child"),
                "acquire_or_use" | "access_routes" => Some("application"),
                "resolve_and_report" => Some("report"),
                _ => None,
            };
            if let Some(child) = child {
                expected = data[child]["exits"]
                    .as_object()
                    .into_iter()
                    .flat_map(|v| v.keys().cloned())
                    .collect();
                if stage.kind == "owned_scope" {
                    for notification in data["notifications"]
                        .as_object()
                        .into_iter()
                        .flat_map(|v| v.keys())
                    {
                        require(
                            expected.remove(notification),
                            "notifications",
                            "Notifications must be declared child exits",
                        );
                    }
                    require(
                        !expected.is_empty(),
                        "child/exits",
                        "A scope needs a terminal child exit",
                    );
                } else {
                    require(
                        expected.contains("success"),
                        child,
                        "Application/report requires a success exit",
                    );
                }
            }
            if stage.kind == "contact_fallback" {
                expected = BTreeSet::from(["connected".into(), "fallback".into()]);
                require(
                    data["call"]["exits"]
                        .as_object()
                        .is_some_and(|v| v.len() >= 2 && v.contains_key("connected")),
                    "call/exits",
                    "Call needs connected and at least one fallback exit",
                );
            }
            if stage.kind == "entitlement_reconcile" {
                expected = BTreeSet::from(["owned".into(), "unowned".into()]);
                require(
                    data.get("vehicle_record").is_some() != data.get("entitled_fact").is_some(),
                    "vehicle_record",
                    "Choose one native vehicle predicate or entitlement fact",
                );
                require(
                    !rows(&data, "retire").is_empty(),
                    "retire",
                    "Entitlement reconciliation needs explicit offer retirement",
                );
                for action in rows(&data, "retire") {
                    require(
                        matches!(
                            action["kind"].as_str(),
                            Some("objective" | "message" | "mappin" | "fact")
                        ) && action.get("state").is_none_or(|v| v == "Inactive"),
                        "retire",
                        "Retirement may only deactivate journal/map entries or set facts",
                    );
                }
            }
            if let Some(defeat) = data.get("defeat") {
                require(
                    stage.kind == "owned_scope" && data["child"].get("scene").is_some(),
                    "defeat",
                    "A defeat watcher requires an owned scene",
                );
                if let Some(outcome) = defeat["outcome"].as_str() {
                    expected.insert(outcome.into());
                }
            }
            if stage.optional("cancellation_fact").is_some() {
                require(
                    !expected.contains("cancelled"),
                    "cancellation_fact",
                    "Cancelled is owned by the scope rather than the child",
                );
                expected.insert("cancelled".into());
            }
            require(
                stage.outcomes()?.into_keys().collect::<BTreeSet<_>>() == expected,
                "outcomes",
                "Declared outcomes must match terminal child/scope exits",
            );
            for effect in data["effects"]
                .as_object()
                .into_iter()
                .flat_map(|v| v.keys())
            {
                require(
                    expected.contains(effect),
                    "effects",
                    "Effects must name a declared outcome",
                );
            }
            if stage.optional("commit_fact").is_some() {
                require(
                    expected.contains("success"),
                    "commit_fact",
                    "A commit fact needs a success outcome",
                );
            }
            require(
                stage.optional("commit_fact").is_none()
                    || stage.optional("commit_fact") != stage.optional("cancellation_fact"),
                "commit_fact",
                "Commit and cancellation facts must differ",
            );
            let restored = rows(&data, "restore")
                .iter()
                .filter_map(action_resource)
                .collect::<BTreeSet<_>>();
            for action in rows(&data, "restore") {
                require(
                    action["kind"] != "item",
                    "restore",
                    "Restoration cannot grant items; use success effects or reward_commit",
                );
            }
            for action in rows(&data, "setup") {
                if let Some(resource) = action_resource(action) {
                    require(
                        restored.contains(&resource),
                        "restore",
                        "Every acquired setup resource requires shared restoration",
                    );
                }
            }
            for child in ["child", "application", "report", "call"] {
                if let Some(exits) = data[child]["exits"].as_object() {
                    require(
                        exits
                            .values()
                            .map(Value::to_string)
                            .collect::<BTreeSet<_>>()
                            .len()
                            == exits.len(),
                        child,
                        "Child exit sockets must be distinct",
                    );
                }
            }
            if stage.kind == "access_routes" {
                let routes = rows(&data, "routes");
                require(
                    distinct(routes, "id"),
                    "routes",
                    "Access route IDs must be distinct",
                );
                for route in routes {
                    require(
                        route["exits"]
                            .as_object()
                            .is_some_and(|v| v.len() == 1 && v.contains_key("success")),
                        "routes",
                        "Access attempts expose exactly one success outcome",
                    );
                }
                for id in data["route_facts"]
                    .as_object()
                    .into_iter()
                    .flat_map(|v| v.keys())
                {
                    require(
                        routes.iter().any(|v| v["id"] == id.as_str()),
                        "route_facts",
                        "Route facts must name declared access routes",
                    );
                }
            }
            if matches!(stage.kind.as_str(), "resolve_and_report" | "observe_lethal") {
                let targets = rows(&data, "targets");
                require(
                    targets
                        .iter()
                        .map(|v| (v["community"].to_string(), v["entry"].to_string()))
                        .collect::<BTreeSet<_>>()
                        .len()
                        == targets.len(),
                    "targets",
                    "Target actors must be distinct",
                );
                if stage.kind == "resolve_and_report" {
                    for target in targets {
                        require(
                            target["resolution"]
                                .as_object()
                                .is_some_and(|v| v.values().any(|v| v == true)),
                            "targets",
                            "Each target needs at least one explicit resolution state",
                        );
                    }
                }
            }
            if stage.kind == "adaptive_timer" {
                require(
                    stage.optional("busy_fact").is_some()
                        == data.get("max_deferral_seconds").is_some(),
                    "busy_fact",
                    "Busy fact and maximum deferral must be supplied together",
                );
            }
        }
    }
    Ok(())
}
