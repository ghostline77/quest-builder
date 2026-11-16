use ghostline_quest_builder::{
    audit, compiler, composition, contracts,
    graph::Graph,
    journal,
    manifest::{self, Stage},
    native, nodes,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn emitted_contract_cannot_claim_a_branch_only_fact() {
    let stage:Stage=serde_json::from_value(json!({"id":"decision","type":"choice_gate","phase_resource":"mod\\fixtures\\decision.questphase","gate_kind":"fact","join":true,"branches":[{"id":"yes","condition":"choose_yes","set_fact":"only_yes"},{"id":"no","condition":"choose_no","set_fact":"only_no"}],"contract":{"writes_facts":["only_yes"]}})).unwrap();
    let error = compiler::compile_stage(&stage).unwrap_err().to_string();
    assert!(error.contains("only some outcome paths"), "{error}");
}

#[test]
fn native_topology_rejects_an_unowned_connection_endpoint() {
    let mut graph = Graph::default();
    let start = nodes::input(&mut graph, "In1").unwrap();
    let end = nodes::output(&mut graph, 1, "Out1").unwrap();
    graph.then(start, end).unwrap();
    let mut document = graph
        .document("mod\\fixtures\\end.questphase", &[])
        .unwrap();
    fn corrupt(value: &mut Value) -> bool {
        if value.get("$type").and_then(Value::as_str) == Some("graphGraphConnectionDefinition") {
            value["destination"] = json!({"HandleRefId":"unowned"});
            return true;
        }
        match value {
            Value::Object(v) => v.values_mut().any(corrupt),
            Value::Array(v) => v.iter_mut().any(corrupt),
            _ => false,
        }
    }
    assert!(corrupt(&mut document));
    assert!(audit::Phase::read(&document).is_err());
}

#[test]
fn website_pages_activate_before_the_separate_visit_fact_gate() {
    let activation = json!({
        "id":"activate", "type":"owned_scope", "status":"ready",
        "phase_resource":"mod\\website\\activate.questphase",
        "setup":[
            {"kind":"journal","path":"internet_sites/fixture","class_name":"gameJournalInternetSite","file_entry_index":1,"state":"Active"},
            {"kind":"journal","path":"internet_sites/fixture/profile","class_name":"gameJournalInternetPage","file_entry_index":1,"state":"Active"}
        ],
        "on":{"success":"read_page"}
    });
    let waiting = json!({
        "id":"read_page", "type":"owned_scope", "status":"ready",
        "phase_resource":"mod\\website\\read.questphase",
        "wait_fact":"fixture_profile_read", "on":{"success":"$end"}
    });
    let source =
        json!({"schema_version":1,"id":"website","title":"Website","stages":[activation,waiting]});
    let checked = manifest::check(&source).unwrap();
    assert!(checked.manifest.is_some(), "{:?}", checked.diagnostics);
    let mut paths = Vec::new();
    fn find(value: &Value, paths: &mut Vec<Value>) {
        if value["$type"] == "gameJournalPath" {
            paths.push(value.clone());
        }
        match value {
            Value::Object(object) => object.values().for_each(|v| find(v, paths)),
            Value::Array(array) => array.iter().for_each(|v| find(v, paths)),
            _ => {}
        }
    }
    let manifest = checked.manifest.unwrap();
    let document = compiler::compile_stage(&manifest.stages[0]).unwrap();
    find(&document, &mut paths);
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().all(|p| p["fileEntryIndex"] == 1));
    assert_eq!(paths[1]["realPath"], "internet_sites/fixture/profile");
    assert_eq!(paths[1]["className"]["$value"], "gameJournalInternetPage");
    let summary = ghostline_quest_builder::inspect::summarize(&document).unwrap();
    assert_eq!(summary["types"]["questJournalEntry_NodeType"], 2);
    assert!(summary["types"].get("questSetVar_NodeType").is_none());
    let gate = compiler::compile_stage(&manifest.stages[1]).unwrap();
    let gate = serde_json::to_string(&gate).unwrap();
    assert!(gate.contains("fixture_profile_read"));
    assert!(gate.contains("Greater"));
}

#[test]
fn community_activation_sets_friendly_attitude_and_invulnerability() {
    let source = json!({
        "schema_version":1, "id":"protection", "title":"Protection",
        "stages":[{
            "id":"spawn", "type":"community", "status":"ready",
            "phase_resource":"mod\\protection\\spawn.questphase",
            "community":"#protected_actor", "entry":"contact", "entries":["contact"],
            "action":"Activate", "phase":"default", "attitude_group":"friendly",
            "protection_source":"contact_protection"
        }]
    });
    let checked = manifest::check(&source).unwrap();
    assert!(checked.manifest.is_some(), "{:?}", checked.diagnostics);
    let document = compiler::compile_stage(&checked.manifest.unwrap().stages[0]).unwrap();
    audit::Phase::read(&document).unwrap();
    let mut operations = Vec::new();
    fn collect(value: &Value, operations: &mut Vec<Value>) {
        if matches!(value["$type"].as_str(), Some(
            "questCharacterManagerParameters_SetAttitudeGroupForPuppet"
            | "questCharacterManagerParameters_SetMortality"
        )) {
            operations.push(value.clone());
        }
        match value {
            Value::Object(object) => object.values().for_each(|v| collect(v, operations)),
            Value::Array(array) => array.iter().for_each(|v| collect(v, operations)),
            _ => {}
        }
    }
    collect(&document, &mut operations);
    assert_eq!(operations.len(), 2);
    for operation in &operations {
        let target = operation["puppetRef"].to_string();
        assert!(target.contains("#protected_actor") && target.contains("contact"));
    }
    let attitude = operations.iter().find(|v| v.get("groupName").is_some()).unwrap();
    assert_eq!(attitude["groupName"]["$value"], "friendly");
    let mortality = operations.iter().find(|v| v.get("state").is_some()).unwrap();
    assert_eq!(mortality["state"], "Invulnerable");
    assert_eq!(mortality["source"]["$value"], "contact_protection");
    assert_eq!(mortality["resetToDefault"], 0);
}

const FIXTURES: &[(&str, &str)] = &[
    (
        "generated",
        include_str!("fixtures/generated-building-blocks.quest.json"),
    ),
    (
        "variants",
        include_str!("fixtures/generated-block-variants.quest.json"),
    ),
    (
        "owned",
        include_str!("fixtures/owned-lifecycle-blocks.quest.json"),
    ),
    (
        "owned-variants",
        include_str!("fixtures/owned-lifecycle-variants.quest.json"),
    ),
    (
        "direct",
        include_str!("fixtures/direct_building_blocks.quest.json"),
    ),
    (
        "cyberpsycho",
        include_str!("fixtures/cyberpsycho_encounter.quest.json"),
    ),
    (
        "phone",
        include_str!("fixtures/phone_conversation.quest.json"),
    ),
    (
        "play-scene",
        include_str!("../examples/blocks/play_scene.quest.json"),
    ),
];

#[test]
fn combat_completion_and_activation_variants_generate_the_requested_conditions() {
    let mut documents = BTreeMap::new();
    for (completion, nonlethal, count) in [
        ("all_defeated", false, 1),
        ("named_defeated", true, 2),
        ("fact", true, 0),
    ] {
        let stage:Stage=serde_json::from_value(json!({"id":"combat","type":"combat_encounter","phase_resource":format!("mod\\combat\\{completion}.questphase"),"community":"#guards","hostility":"neutral_to_hostile","activate":false,"entries":["one","two"],"completion":completion,"nonlethal_allowed":nonlethal,"completion_fact":"combat_done"})).unwrap();
        let document = compiler::compile_stage(&stage).unwrap();
        let summary = ghostline_quest_builder::inspect::summarize(&document).unwrap();
        assert_eq!(
            summary["types"]["questCharacterKilled_ConditionType"]
                .as_u64()
                .unwrap_or(0),
            count
        );
        assert!(summary["types"].get("questCommunityAction").is_none());
        documents.insert(stage.phase_resource, document);
    }
    if let Some(path) = std::env::var_os("GHOSTLINE_TEST_SCHEMA") {
        let path = std::path::Path::new(&path);
        let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
            ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), path)
                .unwrap()
        } else {
            ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap()
        };
        assert_eq!(native::encode(&documents, &schema).unwrap().len(), 3);
    }
}

#[test]
fn compile_existing_stage_configurations() {
    let mut covered = BTreeSet::new();
    let mut documents = BTreeMap::new();
    let mut failures = Vec::new();
    let mut configurations = 0;
    for (name, source) in FIXTURES {
        let raw: Value = serde_json::from_str(source).unwrap();
        let expanded = composition::prepare(&raw).unwrap();
        for value in expanded.manifest["stages"].as_array().unwrap() {
            let stage: Stage = serde_json::from_value(value.clone()).unwrap();
            configurations += 1;
            covered.insert(stage.kind.clone());
            match compiler::compile_stage(&stage) {
                Ok(document) => {
                    documents.insert(format!("{name}/{}", stage.id), document);
                }
                Err(e) => failures.push(format!("{name}/{}: {e:#}", stage.id)),
            }
        }
    }
    let additional = json!([
        {"id":"activate_community","type":"community","phase_resource":"mod\\fixtures\\community.questphase","community":"#guards","action":"Activate","entries":["guard"],"attitude_group":"hostile","completion_fact":"community_ready"},
        {"id":"start_call","type":"phone_call","phase_resource":"mod\\fixtures\\call.questphase","caller":"contacts/fixer","action":"StartCall"},
        {"id":"offer","type":"phone_job_offer","phase_resource":"mod\\fixtures\\offer.questphase","contact":"fixer","message":"contacts/fixer/offer/message","choice_group":"contacts/fixer/offer/choices","accept_choice":"contacts/fixer/offer/choices/accept","start_fact":"offer_started","accepted_fact":"offer_accepted"},
        {"id":"wait","type":"time_gate","phase_resource":"mod\\fixtures\\wait.questphase","seconds":1,"completion_fact":"wait_done"},
        {"id":"read","type":"read_terminal_document","phase_resource":"mod\\fixtures\\read.questphase","computer":"#terminal","completion_fact":"document_read","objective":"quests/minor_quest/fixtures/read/objective"},
        {"id":"device","type":"interact_device","phase_resource":"mod\\fixtures\\device.questphase","device":"#terminal","controller_class":"ComputerControllerPS","action":"QuestForceEnable","completion_function":"IsEnabled","send_action":false},
        {"id":"combat","type":"combat_encounter","phase_resource":"mod\\fixtures\\combat.questphase","community":"#guards","hostility":"already_hostile","completion":"all_defeated","entries":["guard_one","guard_two"],"completion_fact":"guards_defeated"},
        {"id":"clues","type":"investigate_clues","phase_resource":"mod\\fixtures\\clues.questphase","objective":"quests/minor_quest/fixtures/clues/objective","description_entry":"quests/minor_quest/fixtures/clues/objective/description","clues":[{"id":"one","object_ref":"#clue_one","completion_fact":"clue_one_scanned"},{"id":"two","object_ref":"#clue_two","completion_fact":"clue_two_scanned"}],"required_count":1,"scan_order":"any","completion_fact":"investigated"}
    ]);
    for value in additional.as_array().unwrap() {
        let stage: Stage = serde_json::from_value(value.clone()).unwrap();
        configurations += 1;
        covered.insert(stage.kind.clone());
        match compiler::compile_stage(&stage) {
            Ok(document) => {
                documents.insert(format!("additional/{}", stage.id), document);
            }
            Err(e) => failures.push(format!("additional/{}: {e:#}", stage.id)),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let registered = contracts::SCHEMA["$defs"]["baseStage"]["properties"]["type"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        covered, registered,
        "fixture coverage must include every registered builder"
    );
    eprintln!(
        "Compiled {configurations} existing stage configurations across {} kinds",
        covered.len()
    );
    if let Some(schema_path) = std::env::var_os("GHOSTLINE_TEST_SCHEMA") {
        let path = std::path::Path::new(&schema_path);
        let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
            ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), path)
                .unwrap()
        } else {
            ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap()
        };
        assert_eq!(
            native::encode(&documents, &schema).unwrap().len(),
            configurations
        );
        eprintln!("Verified native write/readback for all {configurations} stage configurations");
    }
}

#[test]
fn aliases_preserve_types_and_reject_cycles() {
    let bindings =
        json!({"facts":{"a":"@facts.b","b":"@facts.a"},"objectives":{"one":{"counter":3}}});
    assert!(composition::resolve(&json!("@facts.a"), &bindings).is_err());
    assert_eq!(
        composition::resolve(&json!("@objectives.one.counter"), &bindings).unwrap(),
        json!(3)
    );
    assert_eq!(
        composition::resolve(&json!("@@facts.a"), &bindings).unwrap(),
        json!("@facts.a")
    );
    assert_eq!(
        composition::resolve(&json!("literal @facts.a"), &bindings).unwrap(),
        json!("literal @facts.a")
    );
}

fn composed_quest() -> Value {
    json!({
        "schema_version":1,"id":"signal","title":"Follow the signal","description":"Trace its source.",
        "composition":{
            "facts":{"accepted":null},
            "objectives":{"reach":{"text":"Reach the relay.","mappins":{"relay":{"ref":"#signal_relay"}}}},
            "contacts":{"fixer":{"name":"Fixer","threads":{"offer":{"title":"A job","messages":{"offer":"Can you help?"},"choices":{"accept":"Yes"}}}}}
        },
        "stages":[{"id":"reach","type":"reach_area","objective":"@objectives.reach.path","description_entry":"@objectives.reach.description_entry","mappin":"@objectives.reach.mappin","trigger":"#signal_trigger"}]
    })
}

#[test]
fn complete_build_contains_journal_localization_and_scoped_phases() {
    let mut raw = composed_quest();
    raw["phase_prefabs"] = json!(["#signal_prefab"]);
    let checked = manifest::check(&raw).unwrap();
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let build = compiler::compile(
        checked.manifest.as_ref().unwrap(),
        &checked.composition,
        &compiler::Inputs::default(),
        None,
        false,
    )
    .unwrap();
    let artifacts = &build.artifacts;
    assert_eq!(artifacts.len(), 4);
    assert!(artifacts.contains_key("mod\\signal\\journal\\signal.journal"));
    let child = &artifacts["mod\\signal\\phases\\signal_reach.questphase"];
    assert_eq!(
        child
            .pointer("/Data/RootChunk/phasePrefabs/0/prefabNodeRef/$value")
            .unwrap(),
        "#signal_prefab"
    );
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("build");
    compiler::publish_complete(&path, artifacts, &BTreeMap::new(), &build.resources, false)
        .unwrap();
    assert!(path.join("source/resources/signal.archive.xl").is_file());
    assert!(path.join("build.json").is_file());
    assert!(compiler::publish_raw(&path, artifacts, false).is_err());
}

#[test]
fn cyclic_graph_serializes_without_forward_or_duplicate_handles() {
    let mut graph = Graph::default();
    let input = nodes::input(&mut graph, "In1").unwrap();
    let wait = nodes::fact_condition(&mut graph, 10, "signal", "Greater", 0, false).unwrap();
    graph.then(input, wait).unwrap();
    graph.then(wait, wait).unwrap();
    let doc = graph.document("mod\\test\\loop.questphase", &[]).unwrap();
    let reordered = ghostline_quest_builder::graph::order_handles(&doc).unwrap();
    assert_eq!(doc, reordered);
    let mut changed = doc.clone();
    fn corrupt(v: &mut Value) -> bool {
        if v.get("factName") == Some(&json!("signal")) {
            v["factName"] = json!("other");
            return true;
        }
        match v {
            Value::Object(v) => v.values_mut().any(corrupt),
            Value::Array(v) => v.iter_mut().any(corrupt),
            _ => false,
        }
    }
    assert!(corrupt(&mut changed));
    assert!(native::verify(&doc, &changed).is_err());
}

#[test]
fn branch_facts_require_a_producer_on_every_route() {
    let raw = json!({"schema_version":1,"id":"flow","title":"Flow","entry_stage":"a","stages":[
        {"id":"a","type":"time_gate","phase_resource":"mod\\flow\\a.questphase","seconds":1,"outcomes":{"yes":"Yes","no":"No"},"on":{"yes":"b","no":"c"}},
        {"id":"b","type":"time_gate","phase_resource":"mod\\flow\\b.questphase","seconds":1,"contract":{"writes_facts":["received"]},"on":{"success":"c"}},
        {"id":"c","type":"time_gate","phase_resource":"mod\\flow\\c.questphase","seconds":1,"contract":{"reads_facts":["received"]},"on":{"success":"$end"}}
    ]});
    let checked = manifest::check(&raw).unwrap();
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|d| d.code == "missing_fact_producer")
    );
}

#[test]
#[ignore = "requires GHOSTLINE_TEST_SCHEMA and optional GHOSTLINE_TEST_GAME"]
fn native_compilation_readback() {
    let path = std::env::var_os("GHOSTLINE_TEST_SCHEMA").expect("explicit native schema fixture");
    let path = std::path::Path::new(&path);
    let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
        ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), path)
            .unwrap()
    } else {
        ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap()
    };
    let checked = manifest::check(&composed_quest()).unwrap();
    let build = compiler::compile(
        checked.manifest.as_ref().unwrap(),
        &checked.composition,
        &compiler::Inputs::default(),
        None,
        false,
    )
    .unwrap();
    let binaries = native::encode(&build.artifacts, &schema).unwrap();
    assert_eq!(binaries.len(), build.artifacts.len());
    assert!(!journal::compose(&checked.composition).unwrap().is_empty());
}
