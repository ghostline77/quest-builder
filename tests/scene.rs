use ghostline_quest_builder::{compiler, graph::Graph, manifest, nodes};
use ghostline_quest_builder::{native, scene};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn spec() -> Value {
    let mut commands = json!([
        {
            "kind": "puppet_ai",
            "entity_ref": "#contact",
            "entry": "contact"
        },
        {
            "kind": "use_workspot",
            "entity_ref": "#contact",
            "entry": "contact",
            "workspot_ref": "#idle"
        },
        {
            "kind": "stop_workspot",
            "entity_ref": "#contact",
            "entry": "contact",
            "workspot_ref": "#idle"
        },
        {
            "kind": "appearance",
            "entity_ref": "#contact",
            "entry": "contact",
            "appearance": "default"
        },
        {
            "kind": "scene_tier",
            "tier": "Tier2"
        },
        {
            "kind": "unequip_player"
        },
        {
            "kind": "unequip_actor",
            "entity_ref": "#contact",
            "entry": "contact"
        },
        {
            "kind": "equip_player",
            "item_id": "Items.Preset_Liberty_Default"
        },
        {
            "kind": "equip_actor",
            "entity_ref": "#contact",
            "entry": "contact",
            "item_id": "Items.Preset_Liberty_Default"
        },
        {
            "kind": "teleport_player",
            "destination_ref": "#arrival"
        },
        {
            "kind": "teleport_actor",
            "entity_ref": "#contact",
            "entry": "contact",
            "destination_ref": "#arrival"
        },
        {
            "kind": "realtime_delay",
            "milliseconds": 250
        },
        {
            "kind": "render_fade",
            "duration_seconds": 0.2
        },
        {
            "kind": "player_look_at",
            "target_ref": "#contact",
            "target_entry": "contact"
        },
        {
            "kind": "player_status_effect",
            "status_effect": "BaseStatusEffect.NoCombat"
        },
        {
            "kind": "pause_condition",
            "trigger_ref": "#engage",
            "require_player_not_in_combat": true
        },
        {
            "kind": "character_gender",
            "gender": "Male"
        },
        {
            "kind": "spawn_actor",
            "dynamic_name": "replica"
        },
        {
            "kind": "wait_actor_spawned",
            "dynamic_name": "replica"
        },
        {
            "kind": "show_player",
            "show": true
        },
        {
            "kind": "journal",
            "path": "quests/minor_quest/signal/phase/objective",
            "class_name": "gameJournalQuestObjective"
        },
        {
            "kind": "mappin",
            "path": "quests/minor_quest/signal/phase/objective/pin",
            "class_name": "gameJournalQuestMapPin"
        }
    ]);
    let kinds = commands
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["kind"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    for (index, command) in commands.as_array_mut().unwrap().iter_mut().enumerate() {
        command["key"] = json!(kinds[index]);
        let target = kinds
            .get(index + 1)
            .map(String::as_str)
            .unwrap_or("greeting");
        let dest = if matches!(target, "journal" | "mappin") {
            json!([{"node":target,"input":"Active"}])
        } else {
            json!([{"node":target}])
        };
        match command["kind"].as_str().unwrap() {
            "use_workspot" | "stop_workspot" => command["on_success"] = dest,
            "character_gender" => {
                command["on_true"] = dest.clone();
                command["on_false"] = dest;
            }
            _ => command["on_out"] = dest,
        }
    }
    json!({"name":"signal","archive_path":"mod\\signal\\scenes\\signal.scene",
        "actors":[{"key":"contact","kind":"community","community_ref":"#contact","entry":"contact"},{"key":"player","kind":"player"},{"key":"replica","kind":"spawn","dynamic_name":"replica","record":"Character.Player_Puppet_Base","spawn_marker":"#replica_spawn"}],
        "props":[{"key":"camera","name":"camera","node_ref":"#camera"}],
        "spoken_line_order":["greeting"],"choice_line_order":["accept","decline"],
        "entry_point":{"name":"start","node":"entry_start"},"exit_points":[{"name":"accepted","node":"accepted"},{"name":"declined","node":"declined"}],
        "start_node":{"key":"entry_start","on_start":[{"node":"puppet_ai"}]},
        "sections":[{"key":"greeting","lines":["greeting"],"on_end":[{"node":"reply"}],"camera_event":{"duration_ms":1000,"camera_ref":"#camera"},"look_at_events":[{"actor":"contact","target":"player"},{"actor":"player","target":"contact"}]}],
        "choices":[{"key":"reply","actor":"contact","options":[{"choice_key":"accept","target":"accepted"},{"choice_key":"decline","target":"declined","choice_type":0}]}],
        "quest_nodes":commands,"end_nodes":[{"key":"accepted"},{"key":"declined"}]})
}
#[test]
fn player_couch_transition_blends_into_the_matching_seated_idle() {
    let preset: Value = serde_json::from_str(include_str!("../data/player-couch-sit.json")).unwrap();
    let input = json!({
        "name":"player_couch", "archive_path":"mod\\test\\player_couch.scene",
        "actors":[{"key":"v","kind":"player"}],
        "spoken_line_order":[], "choice_line_order":[],
        "entry_point":{"name":"start","node":"start"},
        "start_node":{"key":"start","on_start":[{"node":"sit"}]},
        "sections":[{"key":"sit","lines":[],"section_duration_ms":3067,"on_end":[]}],
        "quest_nodes":[{"key":"hold","kind":"use_scene_workspot","is_player":true,
            "workspot":"couch","entry_id":2,"jump_to_entry":true,"enable_idle_mode":true,
            "finish_animation":false,"player_params":preset["player_params"],
            "on_work_started":[{"node":"end"}]}],
        "workspots":[{"key":"couch","resource":"base\\workspots\\player\\couch\\player__sit_couch__talk__01.workspot","origin":"#seat"}],
        "player_couch_sits":[{"key":"sit","section":"sit","actor":"v","workspot":"couch","hold":"hold"}],
        "exit_points":[{"name":"end","node":"end"}],"end_nodes":[{"key":"end"}]
    });
    let doc = scene::compile(&input, &json!({"spoken_lines":[],"choice_lines":[]})).unwrap();
    let root = &doc["Data"]["RootChunk"];
    let graph = root["sceneGraph"]["Data"]["graph"].as_array().unwrap();
    let section = graph.iter().find(|n|n["Data"]["$type"]=="scnSectionNode").unwrap();
    let animation = section["Data"]["events"][0]["Data"].clone();
    assert_eq!(animation["$type"],"scnPlaySkAnimEvent");
    assert_eq!(animation["duration"],3067);
    assert_eq!(animation["genderSpecificParams"],preset["event"]["genderSpecificParams"]);
    assert_eq!(animation["rootMotionData"]["originMarker"]["nodeRef"]["$value"],"#seat");
    assert_eq!(section["Data"]["events"][1]["Data"]["startTime"],3066);
    let workspot = &root["workspotInstances"][0]["workspotInstanceId"];
    assert_eq!(&animation["poseBlendOutWorkspot"]["Data"]["workspotId"],workspot);
    assert_eq!(animation["poseBlendOutWorkspot"]["Data"]["sequenceEntryId"]["id"],2);
    let hold = graph.iter().find(|n|n["Data"]["questNode"]["Data"]["$type"]=="questUseWorkspotNodeDefinition").unwrap();
    let params = &hold["Data"]["questNode"]["Data"]["paramsV1"]["Data"];
    assert_eq!(&params["workspotInstanceId"],workspot);
    assert_eq!(params["isPlayer"],1);
    assert_eq!(params["playerParams"],preset["player_params"]);
    assert_eq!(root["resouresReferences"]["cinematicAnimSets"].as_array().unwrap().len(),3);
    if let Some(path) = std::env::var_os("GHOSTLINE_TEST_SCHEMA") {
        let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
            ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game),std::path::Path::new(&path)).unwrap()
        } else { ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap() };
        assert_eq!(native::encode(&BTreeMap::from([("mod\\test\\player_couch.scene".into(),doc)]),&schema).unwrap().len(),1);
    }
}

fn dialogue() -> Value {
    json!({"spoken_lines":[{"key":"greeting","speaker":"contact","addressee":"player","string_id":"1234","text":"Hello.","duration_ms":1500}],"choice_lines":[{"key":"accept","string_id":"1299","text":"Yes."},{"key":"decline","string_id":"1250","text":"No."}]})
}

#[test]
fn seated_handoff_and_companion_navigation_survive_native_encoding() {
    let source = json!({
        "name":"handoff_navigation", "archive_path":"mod\\test\\handoff_navigation.scene",
        "actors":[{"key":"contact","kind":"community","community_ref":"#contact","entry":"contact"}],
        "spoken_line_order":[],"choice_line_order":[],"sections":[],"choices":[],
        "entry_point":{"name":"start","node":"start"},
        "start_node":{"key":"start","on_start":[{"node":"walk"}]},
        "quest_nodes":[
            {"key":"walk","kind":"move_on_spline","entity_ref":"#contact","entry":"contact",
             "spline_ref":"#captured_route","movement_type":"Run","catch_up_with_companion":true,
             "match_player_speed":true,"on_out":[{"node":"stairs"}]},
            {"key":"stairs","kind":"move_to","entity_ref":"#contact","entry":"contact",
             "destination_ref":"#relay_marker","movement_type":"Run","on_out":[{"node":"face"}]},
            {"key":"face","kind":"face_player","entity_ref":"#contact","entry":"contact",
             "on_out":[{"node":"independent_walk"}]},
            {"key":"independent_walk","kind":"move_on_spline","entity_ref":"#contact","entry":"contact",
             "spline_ref":"#captured_route","movement_type":"Run","with_companion":false,
             "on_out":[{"node":"end"}]}
        ],
        "end_nodes":[{"key":"end","end_type":"NonTerminating"}],
        "exit_points":[{"name":"continue","node":"end"}]
    });
    let document = scene::compile(&source, &json!({"spoken_lines":[],"choice_lines":[]})).unwrap();
    let graph = document["Data"]["RootChunk"]["sceneGraph"]["Data"]["graph"].as_array().unwrap();
    let end = graph.iter().find(|n| n["Data"]["$type"] == "scnEndNode").unwrap();
    assert_eq!(end["Data"]["type"], "NonTerminating");
    let commands = graph.iter().filter_map(|n| n["Data"].get("questNode")).collect::<Vec<_>>();
    let companion = &commands[0]["Data"]["nodeParams"]["Data"]["additionalParams"]["Data"]["withCompanionParams"];
    assert_eq!(companion["movementType"]["movementType"], "Run");
    assert_eq!(companion["catchUpWithCompanion"], 1);
    assert_eq!(companion["useMatchForSpeedForPlayer"], true);
    assert_eq!(companion["teleportToCompanion"], false);
    let navigation = &commands[1]["Data"]["nodeParams"]["Data"];
    assert_eq!(navigation["ignoreNavigation"], false);
    assert_eq!(navigation["finishWhenDestinationReached"], true);
    assert_eq!(navigation["movementTargetRef"]["Data"]["refLocalPlayer"], false);
    assert_eq!(navigation["movementTargetRef"]["Data"]["entityReference"]["reference"]["$value"], "#relay_marker");
    let rotation = &commands[2]["Data"];
    assert_eq!(rotation["$type"], "questRotateToNodeDefinition");
    assert_eq!(rotation["entityReference"]["names"][0]["$value"], "contact");
    assert_eq!(rotation["params"]["Data"]["facingTargetRef"]["Data"]["refLocalPlayer"], true);
    let independent = &commands[3]["Data"]["nodeParams"]["Data"]["additionalParams"]["Data"];
    assert_eq!(independent["type"], "Simple");
    assert_eq!(independent["simpleParams"]["movementType"], "Run");
    let game = std::env::var_os("GHOSTLINE_TEST_GAME");
    let schema = ghostline_red::schema::RedSchema::load(game.as_deref().map(std::path::Path::new), None).unwrap();
    native::encode(&BTreeMap::from([("mod\\test\\handoff_navigation.scene".into(), document)]), &schema).unwrap();
    let mut invalid = source;
    invalid["end_nodes"][0]["end_type"] = json!("invalid");
    assert!(scene::compile(&invalid, &json!({"spoken_lines":[],"choice_lines":[]})).is_err());
}

#[test]
#[ignore = "requires GHOSTLINE_TEST_SCENE_SPECS (semicolon-separated authoring paths)"]
fn native_authored_scene_readback() {
    let paths = std::env::var("GHOSTLINE_TEST_SCENE_SPECS").expect("explicit scene paths");
    let game = std::env::var_os("GHOSTLINE_TEST_GAME");
    let schema = ghostline_red::schema::RedSchema::load(game.as_deref().map(std::path::Path::new), None).unwrap();
    let mut artifacts = BTreeMap::new();
    for path in paths.split(';') {
        let path = std::path::Path::new(path);
        let source: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let manifest = path.parent().unwrap().join(source["manifest"].as_str().unwrap());
        let dialogue: Value = serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
        let document = scene::compile(&source, &dialogue).unwrap();
        let depot = source["archive_path"].as_str().unwrap().to_owned();
        assert!(artifacts.insert(depot, document).is_none());
    }
    native::encode(&artifacts, &schema).unwrap();
}

#[test]
fn scene_names_reject_ambiguous_and_missing_bindings() {
    let mut legacy = dialogue();
    legacy["section_node_ids"] = json!({"greeting":2});
    assert!(
        scene::compile(&spec(), &legacy)
            .unwrap_err()
            .to_string()
            .contains("compiler-owned")
    );
    let mut source = spec();
    source["sections"][0]["key"] = json!("entry_start");
    assert!(
        scene::compile(&source, &dialogue())
            .unwrap_err()
            .to_string()
            .contains("duplicate scene node")
    );
    let mut source = spec();
    source["start_node"]["on_start"][0]["node"] = json!("missing");
    assert!(
        scene::compile(&source, &dialogue())
            .unwrap_err()
            .to_string()
            .contains("unknown scene node missing")
    );
    let mut source = spec();
    source["actors"][0]["id"] = json!(7);
    assert!(
        scene::compile(&source, &dialogue())
            .unwrap_err()
            .to_string()
            .contains("compiler-owned")
    );
    let mut source = spec();
    source["rid_resources"] =
        json!([{"key":"performance","path":"mod\\fixtures\\performance.scenerid"}]);
    source["rid_animations"] = json!([{"key":"pose","resource":"missing","serial_number":7}]);
    assert!(
        scene::compile(&source, &dialogue())
            .unwrap_err()
            .to_string()
            .contains("unknown resource missing")
    );
}

fn performance_scene(preset: &str) -> Value {
    let mut spec = spec();
    let actors = if preset == "group_side_exchange" {
        json!({"smoker":"contact","table_listener":"replica"})
    } else {
        json!({"speaker":"contact","listener":"player"})
    };
    let mut block = json!({"key":"performance","kind":"pose_beat","preset":preset,"section":"greeting","actors":actors,"start_ms":2000});
    if matches!(preset, "arrive_and_sit" | "stand_from_chair") {
        block["root_motion"] = json!({"speaker":{"origin":"#chair_origin","offset":{"position":{"X":0,"Y":0,"Z":0,"W":0},"orientation":{"i":0,"j":0,"k":0,"r":1}},"workspot":"chair"}});
        block["ignore_ports"] = if preset == "arrive_and_sit" {
            json!(["settled"])
        } else {
            json!(["idle_only", "standing"])
        };
        spec["workspots"] = json!([{"key":"chair","resource":"mod\\fixtures\\chair.workspot","origin":"#chair_origin"}]);
    }
    spec["performance_blocks"] = json!([block]);
    spec
}

#[test]
fn workspot_lifecycle_blocks_generate_executable_scene_nodes() {
    let mut spec = spec();
    spec["quest_nodes"] = json!([]);
    spec["start_node"]["on_start"] = json!([{"node":"walk"}]);
    spec["performance_blocks"] = json!([
        {"key":"walk","kind":"workspot_move","actor":"contact","workspot_ref":"#chair","is_workspot_infinite":false,"on_success":[{"node":"idle"}],"on_work_started":[]},
        {"key":"idle","kind":"workspot_idle","actor":"contact","on_success":[{"node":"exit"}]},
        {"key":"exit","kind":"workspot_exit","actor":"contact","workspot_ref":"#chair","exit_entry_id":2,"on_success":[{"node":"greeting"}]}
    ]);
    let document = scene::compile(&spec, &dialogue()).unwrap();
    let graph = document["Data"]["RootChunk"]["sceneGraph"]["Data"]["graph"]
        .as_array()
        .unwrap();
    let node = graph
        .iter()
        .find(|v| v["Data"]["questNode"]["Data"]["paramsV1"]["Data"]["function"] == "IdleOnlyMode")
        .unwrap();
    assert_eq!(
        node["Data"]["questNode"]["Data"]["paramsV1"]["Data"]["function"],
        "IdleOnlyMode"
    );
}

#[test]
#[ignore = "requires GHOSTLINE_TEST_SCHEMA and optional GHOSTLINE_TEST_GAME"]
fn native_rid_and_fpp_camera_readback() {
    let schema_path = std::env::var_os("GHOSTLINE_TEST_SCHEMA").expect("explicit schema");
    let path = std::path::Path::new(&schema_path);
    let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
        ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), path)
            .unwrap()
    } else {
        ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap()
    };
    let mut spec = spec();
    spec["rid_resources"] =
        json!([{"key":"performance","path":"mod\\fixtures\\performance.scenerid"}]);
    spec["rid_animations"] =
        json!([{"key":"player_pose","resource":"performance","serial_number":7}]);
    spec["rid_camera_animations"] =
        json!([{"key":"camera_move","resource":"performance","serial_number":8}]);
    spec["rid_anim_sets"] = json!([{"key":"body","animations":["player_pose"]}]);
    spec["rid_facial_anim_sets"] = json!([{"key":"face","animations":["player_pose"]}]);
    spec["actors"][1]["rid_anim_sets"] = json!(["body"]);
    spec["actors"][1]["rid_facial_anim_sets"] = json!(["face"]);
    let section = &mut spec["sections"][0];
    section.as_object_mut().unwrap().remove("camera_event");
    section["rid_camera"] = json!({"origin":"#scene_origin","camera_ref":"#camera","animation":"camera_move","duration_ms":1800});
    section["rid_events"] = json!([{"actor":"player","origin":"#scene_origin","animation":"player_pose","duration_ms":1800,"fpp":true,"fpp_gender_params":{"gender_masks":[2,1],"transition_blend_in_trajectory_space_angles":[{"yaw":15}],"transition_end_input_angles":[{"pitch":10}]}}]);
    let document = scene::compile(&spec, &dialogue()).unwrap();
    let artifacts = BTreeMap::from([("mod\\signal\\scenes\\rid.scene".into(), document)]);
    native::encode(&artifacts, &schema).unwrap();
}

#[test]
#[ignore = "requires GHOSTLINE_TEST_SCHEMA and optional GHOSTLINE_TEST_GAME"]
fn native_performance_readback() {
    let schema_path = std::env::var_os("GHOSTLINE_TEST_SCHEMA").expect("explicit schema");
    let path = std::path::Path::new(&schema_path);
    let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
        ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), path)
            .unwrap()
    } else {
        ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap()
    };
    let mut artifacts = BTreeMap::new();
    for &(name, expected) in PERFORMANCE_EVENTS {
        let doc = checked_performance_scene(name, expected);
        artifacts.insert(format!("mod\\fixtures\\{name}.scene"), doc);
    }
    assert_eq!(native::encode(&artifacts, &schema).unwrap().len(), 5);
}

const PERFORMANCE_EVENTS: &[(&str, &[(&str, u64)])] = &[
    (
        "seated_sad_reply",
        &[
            ("scnPlaySkAnimEvent", 2444),
            ("scnLookAtEvent", 2795),
            ("scnChangeIdleAnimEvent", 4862),
            ("scnChangeIdleAnimEvent", 6840),
            ("scnPlaySkAnimEvent", 9388),
            ("scnChangeIdleAnimEvent", 9717),
            ("scnChangeIdleAnimEvent", 15163),
        ],
    ),
    (
        "seated_explanation",
        &[
            ("scnLookAtEvent", 2000),
            ("scnPlaySkAnimEvent", 7149),
            ("scnChangeIdleAnimEvent", 19322),
            ("scnPlaySkAnimEvent", 24675),
        ],
    ),
    (
        "arrive_and_sit",
        &[
            ("scnPlaySkAnimEvent", 2000),
            ("scnChangeIdleAnimEvent", 2233),
            ("scnPoseCorrectionEvent", 2882),
            ("scnLookAtEvent", 3700),
            ("scnPoseCorrectionEvent", 4087),
            ("scnPoseCorrectionEvent", 4748),
        ],
    ),
    (
        "stand_from_chair",
        &[
            ("scnPlaySkAnimEvent", 2000),
            ("scnChangeIdleAnimEvent", 2824),
            ("scnLookAtEvent", 3102),
        ],
    ),
    (
        "group_side_exchange",
        &[
            ("scnPlaySkAnimEvent", 2000),
            ("scnLookAtEvent", 2806),
            ("scnPlaySkAnimEvent", 4353),
            ("scnPlaySkAnimEvent", 7443),
        ],
    ),
];

fn section_events(document: &Value) -> Vec<&Value> {
    document["Data"]["RootChunk"]["sceneGraph"]["Data"]["graph"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["Data"]["$type"] == "scnSectionNode")
        .unwrap()["Data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| &event["Data"])
        .collect()
}

fn checked_performance_scene(name: &str, expected: &[(&str, u64)]) -> Value {
    let mut input = performance_scene(name);
    input.as_object_mut().unwrap().remove("performance_blocks");
    let baseline = scene::compile(&input, &dialogue()).unwrap();
    let document = scene::compile(&performance_scene(name), &dialogue()).unwrap();
    let events = section_events(&document);
    let authored = section_events(&baseline);
    let signature = |event: &Value| {
        json!([
            event["$type"],
            event["startTime"],
            event["duration"],
            event["screenplayLineId"],
            event["performer"],
            event["cameraRef"]
        ])
    };
    for event in &authored {
        let matches: Vec<_> = events
            .iter()
            .filter(|actual| actual["id"] == event["id"])
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "{name}: authored event lost or duplicated"
        );
        assert_eq!(
            signature(matches[0]),
            signature(event),
            "{name}: authored event changed"
        );
    }
    let added: Vec<_> = events
        .iter()
        .filter(|event| !authored.iter().any(|base| base["id"] == event["id"]))
        .map(|event| {
            (
                event["$type"].as_str().unwrap(),
                event["startTime"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(added, expected, "{name}: incorrect preset choreography");
    document
}

#[test]
fn performance_presets_preserve_authored_events_and_match_choreography() {
    for &(name, expected) in PERFORMANCE_EVENTS {
        checked_performance_scene(name, expected);
    }
}

#[test]
fn complete_build_collects_scene_world_registration_and_checks_bound_interfaces() {
    let quest = json!({"schema_version":1,"id":"bundle","title":"Bundle","stages":[{"id":"wait","type":"time_gate","phase_resource":"mod\\bundle\\phases\\wait.questphase","seconds":1,"completion_fact":"bundle_wait_done"}]});
    let checked = manifest::check(&quest).unwrap();
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let mut graph = Graph::default();
    let start = nodes::input(&mut graph, "In1").unwrap();
    let call = nodes::scene(
        &mut graph,
        10,
        "mod\\signal\\scenes\\signal.scene",
        "#scene_origin",
        &["start"],
        &["accepted", "declined", "Default INT", "Default RET"],
    )
    .unwrap();
    let end = nodes::output(&mut graph, 1, "Out1").unwrap();
    graph.connect(start, "Out", call, "start").unwrap();
    graph.connect(call, "accepted", end, "In").unwrap();
    graph.connect(call, "declined", end, "In").unwrap();
    let phase = graph
        .document("mod\\bundle\\phases\\caller.questphase", &[])
        .unwrap();
    let inputs = compiler::Inputs {
        lifecycles: Vec::new(),
        localizations: Vec::new(),
        assets: BTreeMap::new(),
        loose: BTreeMap::new(),
        scenes: vec![compiler::SceneInput {
            spec: spec(),
            dialogue: dialogue(),
        }],
        worlds: vec![
            json!({"name":"bundle","prefab_root":"#bundle","origin":[0,0,0],"markers":[{"ref":"scene_origin","position":[0,0,0]}]}),
        ],
        extra_resources: BTreeMap::from([("mod\\bundle\\phases\\caller.questphase".into(), phase)]),
        ..Default::default()
    };
    let build = compiler::compile(
        checked.manifest.as_ref().unwrap(),
        &checked.composition,
        &inputs,
        None,
        false,
    )
    .unwrap();
    assert_eq!(build.artifacts.len(), 7);
    assert!(
        build
            .artifacts
            .contains_key("mod\\signal\\localization\\en-us\\signal.lipmap")
    );
    let registration: Value =
        serde_json::from_slice(&build.resources["bundle.archive.xl"]).unwrap();
    assert_eq!(
        registration["streaming"]["blocks"][0],
        "mod\\bundle\\world\\all.streamingblock"
    );
    assert_eq!(
        registration["quest"]["phases"][0]["parent"],
        "base\\quest\\cyberpunk2077.quest"
    );
    let mut broken = inputs;
    broken.scenes[0].spec["exit_points"][0]["name"] = json!("different");
    let error = compiler::compile(
        checked.manifest.as_ref().unwrap(),
        &checked.composition,
        &broken,
        None,
        false,
    )
    .err()
    .unwrap();
    assert!(format!("{error:#}").contains("lacks accepted"));
}

#[test]
fn scene_constructs_all_commands_and_sorts_choice_localization() {
    let document = scene::compile(&spec(), &dialogue()).unwrap();
    let mut explicit = spec();
    explicit["start_node"]["on_start"][0]["input"] = json!("In");
    assert_eq!(document, scene::compile(&explicit, &dialogue()).unwrap());
    let mut missing_non_default = spec();
    missing_non_default["start_node"]["on_start"] = json!([{"node":"mappin"}]);
    assert!(
        scene::compile(&missing_non_default, &dialogue())
            .unwrap_err()
            .to_string()
            .contains("no input In")
    );
    let root = &document["Data"]["RootChunk"];
    assert_eq!(
        root["sceneGraph"]["Data"]["graph"]
            .as_array()
            .unwrap()
            .len(),
        27
    );
    let loc = &root["locStore"]["vdEntries"];
    assert_eq!(loc[0]["locstringId"]["ruid"], "1250");
    assert_eq!(loc[2]["locstringId"]["ruid"], "1299");
    assert_eq!(
        root["debugSymbols"]["performersDebugSymbols"][1]["performerId"]["id"],
        513
    );
    let mut invalid = spec();
    invalid["start_node"]["on_start"][0]["input"] = json!("CutDestination");
    assert!(
        scene::compile(&invalid, &dialogue())
            .unwrap_err()
            .to_string()
            .contains("CutDestination")
    );
}

#[test]
fn multiple_npcs_and_player_share_contiguous_actor_indices() {
    for player_first in [false, true] {
        let mut source = spec();
        if player_first {
            source["actors"].as_array_mut().unwrap().swap(0, 1);
        }
        let document = scene::compile(&source, &dialogue()).unwrap();
        let root = &document["Data"]["RootChunk"];
        let ids = root["actors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|actor| actor["actorId"]["id"].as_u64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![0, 1],
            "NPC IDs must match their actor table positions"
        );
        assert_eq!(root["playerActors"][0]["actorId"]["id"], 2);
        assert_eq!(root["screenplayStore"]["lines"][0]["speaker"]["id"], 0);
        assert_eq!(root["screenplayStore"]["lines"][0]["addressee"]["id"], 2);
    }
}

#[test]
fn seat_choice_is_a_world_interaction_without_an_actor_attachment() {
    let mut source = spec();
    let choice = &mut source["choices"][0];
    choice.as_object_mut().unwrap().remove("actor");
    choice["position"] = json!({"x":-0.35,"y":2.26,"z":0.9});
    let document = scene::compile(&source, &dialogue()).unwrap();
    let nodes = document["Data"]["RootChunk"]["sceneGraph"]["Data"]["graph"]
        .as_array()
        .unwrap();
    let choice = nodes
        .iter()
        .map(|node| &node["Data"])
        .find(|node| node["$type"] == "scnChoiceNode")
        .unwrap();
    assert_eq!(choice["mode"], "attachToWorld");
    assert!(choice.get("ataParams").is_none());
    assert_eq!(
        choice["lookAtParams"]["Data"]["$type"],
        "scnChoiceNodeNsAdaptiveLookAtParams"
    );
    assert_eq!(choice["atwParams"]["entityPosition"]["Y"], 2.26);
    assert_eq!(choice["shapeParams"]["Data"]["customActivationRange"], 2.);
    source["choices"][0]["disable_auto_look_at"] = json!(true);
    let wait_document = scene::compile(&source, &dialogue()).unwrap();
    let wait_choice = wait_document["Data"]["RootChunk"]["sceneGraph"]["Data"]["graph"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| &node["Data"])
        .find(|node| node["$type"] == "scnChoiceNode")
        .unwrap();
    assert!(wait_choice["lookAtParams"].is_null());
}

#[test]
fn player_workspot_targets_the_player_entity() {
    let mut source = spec();
    let workspot = source["quest_nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|node| node["kind"] == "use_workspot")
        .unwrap();
    workspot["is_player"] = json!(true);
    let document = scene::compile(&source, &dialogue()).unwrap();
    let nodes = document["Data"]["RootChunk"]["sceneGraph"]["Data"]["graph"]
        .as_array()
        .unwrap();
    let command = nodes
        .iter()
        .map(|node| &node["Data"]["questNode"]["Data"])
        .find(|node| node["$type"] == "questUseWorkspotNodeDefinition")
        .unwrap();
    assert_eq!(command["entityReference"]["reference"]["$value"], "#player");
    assert_eq!(command["paramsV1"]["Data"]["isPlayer"], 1);
}

#[test]
#[ignore = "requires GHOSTLINE_TEST_SCHEMA and optional GHOSTLINE_TEST_GAME"]
fn native_scene_readback() {
    let schema_path = std::env::var_os("GHOSTLINE_TEST_SCHEMA").expect("explicit schema");
    let path = std::path::Path::new(&schema_path);
    let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
        ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), path)
            .unwrap()
    } else {
        ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap()
    };
    let doc = scene::compile(&spec(), &dialogue()).unwrap();
    let mut seat_spec = spec();
    seat_spec["name"] = json!("seat");
    seat_spec["archive_path"] = json!("mod\\signal\\scenes\\seat.scene");
    seat_spec["choices"][0]
        .as_object_mut()
        .unwrap()
        .remove("actor");
    seat_spec["choices"][0]["position"] = json!({"x":-0.35,"y":2.26,"z":0.9});
    let seat = scene::compile(&seat_spec, &dialogue()).unwrap();
    let artifacts = BTreeMap::from([
        ("mod\\signal\\scenes\\signal.scene".into(), doc),
        ("mod\\signal\\scenes\\seat.scene".into(), seat),
    ]);
    assert_eq!(native::encode(&artifacts, &schema).unwrap().len(), 2);
}
