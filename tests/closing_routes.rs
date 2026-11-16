use ghostline_quest_builder::{compiler, manifest, native};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
#[ignore = "requires an explicit authored quest and game data"]
fn authored_route_phases_and_journal_serialize() {
    let path = std::env::var_os("GHOSTLINE_TEST_QUEST_MANIFEST").expect("quest manifest");
    let source: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let checked = manifest::check(&source).unwrap();
    let manifest = checked.manifest.expect("authored quest validates");
    let mut artifacts = ghostline_quest_builder::journal::compose(&checked.composition).unwrap();
    artifacts.insert("mod\\test\\authored_root.questphase".into(), compiler::orchestrate(&manifest, "mod\\test\\authored_root.questphase").unwrap());
    for stage in &manifest.stages {
        artifacts.insert(stage.phase_resource.clone(), compiler::compile_stage(stage)
            .unwrap_or_else(|error| panic!("stage {}: {error:#}", stage.id)));
    }
    let game = std::env::var_os("GHOSTLINE_TEST_GAME").expect("game data");
    let schema = ghostline_red::schema::RedSchema::load(Some(std::path::Path::new(&game)), None).unwrap();
    let expected = artifacts.len();
    assert_eq!(native::encode(&artifacts, &schema).unwrap().len(), expected);
}

#[test]
fn post_completion_cleanup_and_followup_start_together_and_join() {
    let source = json!({"schema_version":1,"id":"postquest","title":"Postquest",
      "entry_stage":"done",
      "forks":[{"stage":"done","outcome":"success","start":"cleanup","complete":"cleanup"}],
      "stages":[
        {"id":"done","type":"owned_scope","phase_resource":"mod\\test\\done.questphase","on":{"success":"followup"}},
        {"id":"followup","type":"time_gate","clock":"game","hours":1,"phase_resource":"mod\\test\\followup.questphase","on":{"success":"$end"}},
        {"id":"cleanup","type":"owned_scope","wait_trigger":"#outside","trigger_event":"IsOutside","phase_resource":"mod\\test\\cleanup.questphase","on":{"success":"$end"}}
      ]});
    let checked = manifest::check(&source).unwrap();
    let manifest = checked.manifest.expect("concurrent cleanup remains reachable");
    let document = compiler::orchestrate(&manifest, "mod\\test\\postquest.questphase").unwrap();
    ghostline_quest_builder::audit::Phase::read(&document).unwrap();
    let mut joins = Vec::new();
    values(&document, "questLogicalAndNodeDefinition", &mut joins);
    assert_eq!(joins.len(), 1, "root must wait for both completion branches");
    let game = std::env::var_os("GHOSTLINE_TEST_GAME");
    let schema = ghostline_red::schema::RedSchema::load(game.as_deref().map(std::path::Path::new), None).unwrap();
    native::encode(&BTreeMap::from([("mod\\test\\postquest.questphase".into(),document)]), &schema).unwrap();
}

#[test]
fn terminal_unlock_enables_the_named_file_with_a_native_computer_event() {
    let source = json!({"schema_version":1,"id":"relay","title":"Relay",
      "stages":[{"id":"unlock","type":"owned_scope","phase_resource":"mod\\relay\\unlock.questphase",
        "effects":{"success":[{"kind":"terminal_document","device":"#relay",
          "document":"relay_receipt","enabled":true}]}}]});
    let checked = manifest::check(&source).unwrap();
    assert!(checked.manifest.is_some(), "{:?}", checked.diagnostics);
    let document = compiler::compile_stage(&checked.manifest.unwrap().stages[0]).unwrap();
    let mut events = Vec::new();
    values(&document, "questEventManagerNodeDefinition", &mut events);
    assert_eq!(events.len(), 1);
    let event = events[0];
    assert_eq!(event["managerName"], "ComputerManager");
    assert_eq!(event["objectRef"]["reference"]["$value"], "#relay");
    assert_eq!(event["event"]["Data"]["$type"], "EnableDocumentEvent");
    assert_eq!(event["event"]["Data"]["documentName"]["$value"], "relay_receipt");
    assert_eq!(event["event"]["Data"]["enable"], 1);
    let game = std::env::var_os("GHOSTLINE_TEST_GAME");
    let schema = ghostline_red::schema::RedSchema::load(game.as_deref().map(std::path::Path::new), None).unwrap();
    native::encode(&BTreeMap::from([("mod\\relay\\unlock.questphase".into(), document)]), &schema).unwrap();
}

fn values<'a>(value: &'a Value, kind: &str, result: &mut Vec<&'a Value>) {
    if value["$type"] == kind {
        result.push(value);
    }
    match value {
        Value::Object(fields) => fields.values().for_each(|v| values(v, kind, result)),
        Value::Array(rows) => rows.iter().for_each(|v| values(v, kind, result)),
        _ => {}
    }
}

#[test]
fn interrupted_wait_restores_visibility_and_player_control() {
    let source = json!({"schema_version":1,"id":"wait_cleanup","title":"Wait cleanup",
      "stages":[{"id":"wait","type":"owned_scope","phase_resource":"mod\\test\\wait.questphase",
        "child":{"scene":"mod\\test\\wait.scene","origin":"#wait","entry":"start",
          "exits":{"success":"ready","interrupted":"Default INT","returned":"Default RET"}},
        "outcomes":{"success":"Out1","interrupted":"Out2","returned":"Out3"},
        "on":{"success":"$end","interrupted":"$end","returned":"$end"},
        "effects":{"interrupted":[{"kind":"render_fade","fade_in":true,"duration_seconds":0},
          {"kind":"scene_tier","tier":"Tier1"}],
          "returned":[{"kind":"render_fade","fade_in":true,"duration_seconds":0},
          {"kind":"scene_tier","tier":"Tier1"}]}}]});
    let checked = manifest::check(&source).unwrap();
    assert!(checked.manifest.is_some(), "{:?}", checked.diagnostics);
    let manifest = checked.manifest.unwrap();
    let document = compiler::compile_stage(&manifest.stages[0]).unwrap();
    ghostline_quest_builder::audit::Phase::read(&document).unwrap();
    let mut fades = Vec::new();
    values(&document, "questSetFadeInOut_NodeType", &mut fades);
    assert_eq!(fades.len(), 2, "both non-success exits restore visibility");
    assert!(fades.iter().all(|fade| fade["fadeIn"] == 1 && fade["duration"] == 0.0));
    let mut tiers = Vec::new();
    values(&document, "questSetTier_NodeType", &mut tiers);
    assert_eq!(tiers.len(), 2);
    assert!(tiers.iter().all(|tier| tier["tier"] == "Tier1"));
    let game = std::env::var_os("GHOSTLINE_TEST_GAME");
    let schema = ghostline_red::schema::RedSchema::load(game.as_deref().map(std::path::Path::new), None).unwrap();
    native::encode(&BTreeMap::from([("mod\\test\\wait.questphase".into(), document)]), &schema).unwrap();
}

#[test]
fn encounter_marker_binds_to_the_named_actor_instead_of_the_community_origin() {
    let source = json!({"schema_version":1,"id":"broker","title":"Broker",
      "composition":{"namespace":"broker","objectives":{"confront":{
        "text":"Confront Latch.","mappins":{"target":{"ref":"#broker","entry":"latch"}}
      }}},
      "stages":[{"id":"start","type":"owned_scope","phase_resource":"mod\\broker\\start.questphase"}]});
    let checked = manifest::check(&source).unwrap();
    assert!(checked.manifest.is_some(), "{:?}", checked.diagnostics);
    let artifacts = ghostline_quest_builder::journal::compose(&checked.composition).unwrap();
    let mut pins = Vec::new();
    for document in artifacts.values() {
        values(document, "gameJournalQuestMapPin", &mut pins);
    }
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0]["reference"]["reference"]["$value"], "#broker");
    assert_eq!(pins[0]["reference"]["names"][0]["$value"], "latch");
    assert_eq!(pins[0]["mappinData"]["mappinType"]["$value"], "Mappins.QuestDynamicMappinDefinition");
    let game = std::env::var_os("GHOSTLINE_TEST_GAME");
    let schema = ghostline_red::schema::RedSchema::load(game.as_deref().map(std::path::Path::new), None).unwrap();
    native::encode(&artifacts, &schema).unwrap();
}

#[test]
fn terminal_shutdown_waits_for_device_zoom_to_end() {
    let mut source = json!({"schema_version":1,"id":"terminal_exit","title":"Terminal exit",
      "stages":[{"id":"commit","type":"owned_scope","phase_resource":"mod\\test\\terminal_exit.questphase",
        "wait_ui_context":{"state":"DeviceZoom","active":false},
        "effects":{"success":[{"kind":"device","device":"#terminal",
          "controller_class":"ComputerControllerPS","action":"ForceOFF"}]}}]});
    let checked = manifest::check(&source).unwrap();
    assert!(checked.manifest.is_some(), "{:?}", checked.diagnostics);
    let manifest = checked.manifest.unwrap();
    let document = compiler::compile_stage(&manifest.stages[0]).unwrap();
    ghostline_quest_builder::audit::Phase::read(&document).unwrap();
    let mut gates = Vec::new();
    values(&document, "questUIContextState_ConditionType", &mut gates);
    assert_eq!(gates.len(), 1);
    assert_eq!(gates[0]["state"], "DeviceZoom");
    assert_eq!(gates[0]["active"], 0);
    let mut pauses = Vec::new();
    values(&document, "questPauseConditionNodeDefinition", &mut pauses);
    assert!(pauses.iter().any(|pause| pause["condition"]["Data"]["$type"] == "questUICondition"));
    let game = std::env::var_os("GHOSTLINE_TEST_GAME");
    let schema = ghostline_red::schema::RedSchema::load(game.as_deref().map(std::path::Path::new), None).unwrap();
    native::encode(&BTreeMap::from([("mod\\test\\terminal_exit.questphase".into(), document)]), &schema).unwrap();

    // Receipt-only recovery must not acquire a terminal dependency.
    source["stages"][0].as_object_mut().unwrap().remove("wait_ui_context");
    let checked = manifest::check(&source).unwrap();
    let document = compiler::compile_stage(&checked.manifest.unwrap().stages[0]).unwrap();
    let mut gates = Vec::new();
    values(&document, "questUIContextState_ConditionType", &mut gates);
    assert!(gates.is_empty());
}

#[test]
fn optional_single_reply_followup_has_a_once_guard_and_no_extra_message() {
    let mut source: Value = serde_json::from_str(include_str!("fixtures/phone_conversation.quest.json")).unwrap();
    let stage = &mut source["stages"][0];
    stage.as_object_mut().unwrap().remove("final_message");
    stage["choices"].as_array_mut().unwrap().truncate(1);
    stage["once_fact"] = json!("followup_sent");
    let checked = manifest::check(&source).unwrap();
    let manifest = checked.manifest.expect("single-reply conversations are valid");
    let doc = compiler::compile_stage(&manifest.stages[0]).unwrap();
    let mut paths = Vec::new();
    values(&doc, "gameJournalPath", &mut paths);
    assert!(!paths.iter().any(|p| p["realPath"] == "contacts/morrow/gq_phone_example/05_msg_more_work"));
    assert!(paths.iter().any(|p| p["realPath"] == "contacts/morrow/gq_phone_example/04a_msg_professional"));
    let mut predicates = Vec::new();
    values(&doc, "questVarComparison_ConditionType", &mut predicates);
    assert!(predicates.iter().any(|p| p["factName"] == "followup_sent"));
    let game = std::env::var_os("GHOSTLINE_TEST_GAME");
    let schema = ghostline_red::schema::RedSchema::load(game.as_deref().map(std::path::Path::new), None).unwrap();
    native::encode(&BTreeMap::from([("mod\\test\\followup.questphase".into(), doc)]), &schema).unwrap();
}

#[test]
fn actor_loot_and_separate_defeat_output_preserve_native_bindings() {
    let source = json!({"schema_version":1,"id":"recovery","title":"Recovery","entry_stage":"stock",
        "stages":[
            {"id":"stock","type":"owned_scope","phase_resource":"mod\\test\\stock.questphase",
             "effects":{"success":[{"kind":"item","item":"Items.RecoveredReceipt","quantity":1,"community":"#enemy","entry":"enemy"}]},
             "on":{"success":"encounter"}},
            {"id":"encounter","type":"owned_scope","phase_resource":"mod\\test\\encounter.questphase",
             "child":{"scene":"mod\\test\\talk.scene","origin":"#origin","entry":"start","exits":{"success":"peaceful"}},
             "defeat":{"community":"#enemy","entry":"enemy","outcome":"defeated"},
             "outcomes":{"success":"Out1","defeated":"Out2"},
             "effects":{"defeated":[{"kind":"scene_tier","tier":"Tier1"}]},
             "on":{"success":"$end","defeated":"$end"}}
        ]});
    let checked = manifest::check(&source).unwrap();
    let manifest = checked.manifest.expect("defeat is a separately owned outcome");
    let mut artifacts = BTreeMap::new();
    for stage in &manifest.stages {
        artifacts.insert(stage.phase_resource.clone(), compiler::compile_stage(stage).unwrap());
    }
    let stock = &artifacts["mod\\test\\stock.questphase"];
    let mut params = Vec::new();
    values(stock, "questUniversalRef", &mut params);
    assert!(params.iter().any(|p| p["refLocalPlayer"] == 0 && p["entityReference"]["reference"]["$value"] == "#enemy"));
    let game = std::env::var_os("GHOSTLINE_TEST_GAME");
    let schema = ghostline_red::schema::RedSchema::load(game.as_deref().map(std::path::Path::new), None).unwrap();
    native::encode(&artifacts, &schema).unwrap();
    let mut invalid = source;
    invalid["stages"][0]["effects"]["success"][0].as_object_mut().unwrap().remove("entry");
    assert!(manifest::check(&invalid).unwrap().manifest.is_none());
}
