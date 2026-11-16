use ghostline_quest_builder::{localization, native};
use serde_json::json;

#[test]
fn localization_preserves_shared_gendered_audio_and_rejects_duplicate_ids() {
    let spec = json!({"namespace":"text","dialogue":"meeting","locale":"en-us"});
    let mut manifest = json!({"spoken_lines":[{"string_id":"9007199254740993","text":"Welcome","male_text":"Welcome, sir","audio_path":"mod\\text\\voice\\shared.wem"},{"string_id":"9007199254740994","text":"Goodbye","audio_path":"mod\\text\\voice\\female.wem","male_audio_path":"mod\\text\\voice\\male.wem"}]});
    let build = localization::compile(&spec, &manifest).unwrap();
    let voice = &build.artifacts["mod\\text\\localization\\en-us\\vo\\meeting.json"]["Data"]["RootChunk"]
        ["root"]["Data"]["entries"];
    assert_eq!(voice[0]["femaleResPath"], voice[0]["maleResPath"]);
    assert_ne!(voice[1]["femaleResPath"], voice[1]["maleResPath"]);
    let text = &build.artifacts["mod\\text\\localization\\en-us\\subtitles\\meeting.json"]["Data"]
        ["RootChunk"]["root"]["Data"]["entries"];
    assert_eq!(text[0]["stringId"], "9007199254740993");
    assert_eq!(text[0]["maleVariant"], "Welcome, sir");
    if let Some(path) = std::env::var_os("GHOSTLINE_TEST_SCHEMA") {
        let path = std::path::Path::new(&path);
        let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
            ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), path)
                .unwrap()
        } else {
            ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap()
        };
        assert_eq!(native::encode(&build.artifacts, &schema).unwrap().len(), 3);
    }
    manifest["spoken_lines"][1]["string_id"] = manifest["spoken_lines"][0]["string_id"].clone();
    assert!(localization::compile(&spec, &manifest).is_err());
}
