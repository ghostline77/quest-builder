use ghostline_quest_builder::{native, world};
use serde_json::json;

fn world_spec() -> serde_json::Value {
    json!({"name":"signal","prefab_root":"#signal","origin":{"x":10.,"y":20.,"z":30.},
        "markers":[{"ref":"arrival","position":{"x":11.,"y":21.,"z":31.}}],
        "triggers":[{"ref":"entry","position":{"x":12.,"y":21.,"z":31.},"outline":{"type":"box","width":4.,"depth":6.,"height":3.}}],
        "race_splines":[{"ref":"route","position":{"x":10.,"y":20.,"z":30.},"points":[[0.,0.,0.],[100.,0.,0.]],"width_default":5.}],
        "always_loaded_node_refs":["arrival"],"streaming_box":{"padding":10.}})
}

#[test]
fn world_sector_deletions_are_registered_and_reject_invalid_indices() {
    let mut spec = world_spec();
    spec["sector_patches"] = json!([{"path":"base\\worlds\\test.streamingsector","expected_nodes":2418,"node_deletions":[{"index":138,"type":"worldAISpotNode"}]}]);
    let build = world::compile(&spec).unwrap();
    assert_eq!(
        build.registration["streaming"]["sectors"],
        json!([{"path":"base\\worlds\\test.streamingsector","expectedNodes":2418,"nodeDeletions":[{"index":138,"type":"worldAISpotNode"}]}])
    );
    spec["sector_patches"][0]["node_deletions"][0]["index"] = json!(2418);
    assert!(world::compile(&spec).is_err());
}

#[test]
fn world_generates_resources_and_rejects_duplicate_identities() {
    let spec = world_spec();
    let build = world::compile(&spec).unwrap();
    assert_eq!(build.artifacts.len(), 3);
    assert_eq!(
        build.registration["streaming"]["blocks"][0],
        "mod\\signal\\world\\all.streamingblock"
    );
    let block = &build.artifacts["mod\\signal\\world\\all.streamingblock"];
    assert_eq!(
        block
            .pointer("/Data/RootChunk/descriptors/0/streamingBox/Max/X")
            .unwrap(),
        125.
    );
    let mut rotated = spec.clone();
    rotated["race_splines"][0]["yaw"] = json!(90.);
    rotated["race_splines"][0]["points"][1] =
        json!({"position":[100.,0.,0.],"tangents":[[150.,0.,0.],[0.,0.,0.]]});
    let rotated = world::compile(&rotated).unwrap();
    let block = &rotated.artifacts["mod\\signal\\world\\all.streamingblock"];
    let max_y = block
        .pointer("/Data/RootChunk/descriptors/0/streamingBox/Max/Y")
        .unwrap()
        .as_f64()
        .unwrap();
    assert!(
        (max_y - 285.).abs() < 1e-8,
        "rotated spline control hull omitted: {max_y}"
    );
    let mut duplicate = spec.clone();
    duplicate["markers"]
        .as_array_mut()
        .unwrap()
        .push(spec["markers"][0].clone());
    assert!(
        world::compile(&duplicate)
            .unwrap_err()
            .to_string()
            .contains("duplicate world NodeRef")
    );
}

#[test]
#[ignore = "requires GHOSTLINE_TEST_SCHEMA and optional GHOSTLINE_TEST_GAME"]
fn native_world_readback() {
    let schema_path =
        std::env::var_os("GHOSTLINE_TEST_SCHEMA").expect("explicit native schema fixture");
    let schema_path = std::path::Path::new(&schema_path);
    let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
        ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), schema_path)
            .unwrap()
    } else {
        ghostline_red::schema::RedSchema::from_slice(&std::fs::read(schema_path).unwrap()).unwrap()
    };
    let mut spec = world_spec();
    spec["communities"] = json!([{"ref":"actors","position":{"x":10.,"y":20.,"z":30.},"spawn_set_reference":"contact","entries":[{"entry":"contact","character":"Character.judy","voice_tag":"judy","appearances":["default"],"additional_phases":["waiting"],"spots":[{"ref":"contact_spawn","position":{"x":10.,"y":20.,"z":30.},"workspot":"base\\animations\\workspots\\common\\stand.workspot"},{"ref":"contact_wait","phase":"waiting","position":{"x":12.,"y":20.,"z":30.},"workspot":"base\\animations\\workspots\\common\\stand.workspot"}]}]}]);
    spec["devices"] =
        json!([{"ref":"terminal","position":{"x":15.,"y":20.,"z":30.},"buffer_id":1}]);
    spec["devices"][0]["personal_link"] = json!({"controller_component_id":"1368878671547998208","interaction_record":"Interactions.ConnectPersonalLink"});
    spec["entities"] = json!([{"ref":"prop","position":{"x":10.,"y":20.,"z":30.},"entity":"base\\test.ent"},{"ref":"population","position":{"x":10.,"y":20.,"z":30.},"population_record":"Character.judy"},{"ref":"ambient","position":{"x":10.,"y":20.,"z":30.},"ambient_event":"city","ambient_outline":{"type":"box","width":3.,"depth":3.,"height":3.}},{"ref":"vehicles","position":{"x":10.,"y":20.,"z":30.},"vehicle_forbidden_outline":{"type":"box","width":3.,"depth":3.,"height":3.}}]);
    spec["triggers"][0]["notifiers"] = json!(["quest", "interior", "prevention"]);
    let build = world::compile(&spec).unwrap();
    assert_eq!(native::encode(&build.artifacts, &schema).unwrap().len(), 4);
}
