use ghostline_quest_builder::{lifecycle, native};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn specs() -> Vec<Value> {
    let flow = json!({"scene":"mod\\fixture\\dialogue.scene","origin":"#origin","entry":"start","exit":"end"});
    vec![
        json!({"kind":"meeting","depot":"mod\\fixture\\meeting.questphase","objective":"quests/minor_quest/fixture/phase/objective","description_entry":"quests/minor_quest/fixture/phase/objective/description","mappin":"quests/minor_quest/fixture/phase/objective/pin","community":"#contact","contact":"contact","setup_trigger":"#setup","rearm_trigger":"#engage","scene":"mod\\fixture\\dialogue.scene","scene_origin":"#origin","protection_source":"fixture_protection","success_fact":"fixture_accepted","deferred_fact":"fixture_deferred","reward_item":"Items.money","reward_quantity":10}),
        json!({"kind":"personal_link_attempt","depot":"mod\\fixture\\attempt.questphase","device":"#terminal","label":"Transfer","duration_seconds":5,"connected_scene":flow}),
        json!({"kind":"personal_link_transfer","depot":"mod\\fixture\\transfer.questphase","device":"#terminal","attempt_resource":"mod\\fixture\\attempt.questphase","objective":"quests/minor_quest/fixture/phase/objective","description_entry":"quests/minor_quest/fixture/phase/objective/description","mappin":"quests/minor_quest/fixture/phase/objective/pin","success_fact":"fixture_copied","intro_scene":flow,"retry_scene":flow,"success_scene":flow}),
        json!({"kind":"root","depot":"mod\\fixture\\root.questphase","completed_fact":"fixture_completed","cleanup_trigger":"#away","cleanup_community":"#contact","stages":[{"id":"meeting","phase_resource":"mod\\fixture\\meeting.questphase"},{"id":"transfer","phase_resource":"mod\\fixture\\transfer.questphase"}],"call":{"caller":"contacts/fixture","start_before":"meeting","end_before":"transfer"}}),
        json!({"kind":"finish_and_epilogue","depot":"mod\\fixture\\finish.questphase","debrief_fact":"fixture_debriefed","reward_item":"Items.money","reward_quantity":10,"verified_fact":"fixture_verified","granted_fact":"fixture_granted","quest_path":"quests/minor_quest/fixture","completed_fact":"fixture_completed","epilogue_phase":"mod\\fixture\\epilogue.questphase"}),
        json!({"kind":"reward_reconcile","depot":"mod\\fixture\\reconcile.questphase","completed_fact":"fixture_completed","debrief_fact":"fixture_debriefed","verified_fact":"fixture_verified","granted_fact":"fixture_granted","reward_item":"Items.money","reward_quantity":10}),
    ]
}

#[test]
#[ignore = "requires GHOSTLINE_TEST_SCHEMA and optional GHOSTLINE_TEST_GAME"]
fn native_lifecycles_preserve_retry_resume_reward_and_cleanup_graphs() {
    let schema_path = std::env::var_os("GHOSTLINE_TEST_SCHEMA").expect("explicit schema");
    let path = std::path::Path::new(&schema_path);
    let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
        ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), path)
            .unwrap()
    } else {
        ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap()
    };
    let mut artifacts = BTreeMap::new();
    for spec in specs() {
        let doc = lifecycle::compile(&spec).unwrap();
        artifacts.insert(spec["depot"].as_str().unwrap().into(), doc);
    }
    assert_eq!(native::encode(&artifacts, &schema).unwrap().len(), 6);
}
