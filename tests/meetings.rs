use ghostline_quest_builder::{compiler, manifest, native};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn meeting() -> Value {
    json!({"schema_version":1,"id":"meeting_options","title":"Meeting options","stages":[{
        "id":"meet","type":"meet_contact","phase_resource":"mod\\meeting_options\\meet.questphase",
        "contact":"contact","community":"#contact","scene":"mod\\meeting_options\\meet.scene",
        "objective":"quests/minor_quest/meeting_options/meet","description_entry":"quests/minor_quest/meeting_options/description",
        "mappin":"quests/minor_quest/meeting_options/pin","setup_trigger":"#nearby","scene_origin":"#origin","accepted_fact":"meeting_accepted"
    }]})
}

#[test]
fn validates_meeting_bindings_and_compiles_disabled_deferral_with_resume() {
    let mut source = meeting();
    source["stages"][0]
        .as_object_mut()
        .unwrap()
        .remove("scene_origin");
    let checked = manifest::check(&source).unwrap();
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|d| d.path.ends_with("scene_origin"))
    );

    let mut documents = BTreeMap::new();
    for allow_defer in [true, false] {
        let mut source = meeting();
        source["stages"][0]["allow_defer"] = json!(allow_defer);
        source["stages"][0]["deferred_fact"] = json!("meeting_deferred");
        let checked = manifest::check(&source).unwrap();
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let stage = &checked.manifest.unwrap().stages[0];
        let document = compiler::compile_stage(stage).unwrap();
        documents.insert(
            format!("mod\\meeting_options\\meet_{allow_defer}.questphase"),
            document,
        );
    }
    if let Some(path) = std::env::var_os("GHOSTLINE_TEST_SCHEMA") {
        let path = std::path::Path::new(&path);
        let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
            ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), path)
                .unwrap()
        } else {
            ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap()
        };
        assert_eq!(native::encode(&documents, &schema).unwrap().len(), 2);
    }
}
