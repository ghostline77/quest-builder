use ghostline_quest_builder::{compiler, manifest, native};
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn remaining_builtin_options_generate_native_resources() {
    let mut cases = Vec::new();
    for kind in ["fact", "trigger", "detection", "alarm", "timer"] {
        for evaluation in ["on_entry", "at_exit", "continuous"] {
            let value = match kind {
                "fact" => json!("condition_met"),
                "timer" => json!(0.125),
                _ => json!("#observer"),
            };
            let mut condition = json!({"kind":kind,"value":value});
            if evaluation != "on_entry" {
                condition["stop_fact"] = json!("window_closed");
            }
            cases.push(json!({"type":"optional_condition","objective":"quests/minor_quest/options/optional","condition":condition,"success_fact":"optional_passed","failure_fact":"optional_failed","evaluation":evaluation}));
        }
    }
    cases.push(json!({"type":"carry_npc","community":"#contact","entry":"contact","destination":"#delivery_area","objective":"quests/minor_quest/options/carry","placement_slot":"seat_front_right","placement_parent":"#delivery_vehicle","completion_fact":"delivered"}));
    for allow in [true, false] {
        cases.push(json!({"type":"escort_npc","community":"#contact","entry":"contact","destinations":["#waypoint"],"objective":"quests/minor_quest/options/escort","mappin":"quests/minor_quest/options/escort/pin","completion_fact":"escorted","failure_fact":"escort_failed","allow_combat_interrupt":allow,"outcomes":{"success":"Out1","failure":"Failed"},"on":{"success":"$end","failure":"$end"}}));
    }
    cases.push(json!({"type":"read_terminal_document","computer":"#computer","scene":"mod\\options\\read.scene","output_socket":"read","document_entry":"onscreens/terminal/document","completion_fact":"read_complete","objective":"quests/minor_quest/options/read"}));
    cases.push(json!({"type":"vehicle_cleanup","player_vehicle_record":"Vehicle.v_standard2_archer_hella_player","completion_fact":"retired","cleanup_communities":["#vehicles"]}));
    cases.push(json!({"type":"choice_gate","gate_kind":"phone_choice","join":true,"branches":[{"id":"yes","condition":"contacts/fixer/thread/yes","set_fact":"yes_selected"},{"id":"no","condition":"contacts/fixer/thread/no","set_fact":"no_selected"}]}));
    cases.push(json!({"type":"choice_gate","gate_kind":"scene_exit","join":true,"scene":"mod\\options\\choice.scene","scene_origin":"#origin","branches":[{"id":"yes","condition":"accept","set_fact":"yes_selected"},{"id":"no","condition":"decline","set_fact":"no_selected"}]}));
    let mut documents = BTreeMap::new();
    for (index, mut case) in cases.into_iter().enumerate() {
        case["id"] = json!(format!("option_{index}"));
        case["phase_resource"] = json!(format!("mod\\options\\option_{index}.questphase"));
        let source = json!({"schema_version":1,"id":"options","title":"Options","stages":[case]});
        let checked = manifest::check(&source).unwrap();
        assert!(
            checked.manifest.is_some(),
            "case {index}: {:?}",
            checked.diagnostics
        );
        let stage = &checked.manifest.as_ref().unwrap().stages[0];
        let doc = compiler::compile_stage(stage).unwrap_or_else(|e| panic!("case {index}: {e:#}"));
        documents.insert(stage.phase_resource.clone(), doc);
    }
    if let Some(path) = std::env::var_os("GHOSTLINE_TEST_SCHEMA") {
        let path = std::path::Path::new(&path);
        let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
            ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), path)
                .unwrap()
        } else {
            ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap()
        };
        assert_eq!(
            native::encode(&documents, &schema).unwrap().len(),
            documents.len()
        );
    }
}
