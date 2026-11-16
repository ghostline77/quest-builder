use ghostline_quest_builder::{fx, native};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({
        "namespace":"test_fx","fx_name":"test_fx_hit",
        "loop_start_seconds":0.2,"loop_end_seconds":1.0,
        "hold_frames":1,"outro_frames":1,"outro_frame_seconds":0.05,
        "v_screen_size":1.0,"npc_billboard_size":1.0,
        "material_tint":[255,255,255,255],"sound_event":"test_fx_sound",
        "layers":{
            "hearts":{"additive_blend":1.0,"alpha_global":1.0,"color_multiplier":1.0},
            "bows":{"additive_blend":0.0,"alpha_global":1.0,"color_multiplier":1.0}
        },
        "targets":{"v":["base\\player.ent"],"npc":["base\\npc.ent"]}
    })
}

#[test]
fn effect_spawners_do_not_eagerly_import_their_resource_chain() {
    let build = fx::compile(&spec()).unwrap();
    for variant in ["v", "npc"] {
        let depot = format!("mod\\test_fx\\registration\\test_fx_{variant}.ent");
        let root = &build.artifacts[&depot]["Data"]["RootChunk"];
        assert_eq!(root["resolvedDependencies"], json!([]));
        let descriptor = &root["compiledData"]["Data"]["Chunks"][1]["effectDescs"][0]["Data"];
        assert_eq!(descriptor["effect"]["Flags"], "Default");
        assert_eq!(descriptor["isAutoSpawn"], false);
        assert_eq!(descriptor["effectName"]["$value"], "test_fx_hit");
        assert_eq!(
            build.registration["resource"]["patch"][&depot],
            spec()["targets"][variant]
        );
    }
}

#[test]
#[ignore = "requires GHOSTLINE_TEST_SCHEMA and optional GHOSTLINE_TEST_GAME"]
fn compiled_effect_spawners_do_not_promote_the_async_lookup_to_a_dependency() {
    let path = std::env::var_os("GHOSTLINE_TEST_SCHEMA").expect("explicit schema");
    let path = std::path::Path::new(&path);
    let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
        ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), path)
            .unwrap()
    } else {
        ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap()
    };
    let artifacts = fx::compile(&spec())
        .unwrap()
        .artifacts
        .into_iter()
        .filter(|(path, _)| path.ends_with(".ent"))
        .collect();
    let binaries = native::encode(&artifacts, &schema).unwrap();
    assert_eq!(binaries.len(), 2);
    for (path, bytes) in binaries {
        let imports = ghostline_red::cr2w::inspect_bytes(&bytes).unwrap().imports;
        assert!(
            imports.is_empty(),
            "{path}: the timeline must not be a top-level dependency"
        );
    }
}
