use ghostline_quest_builder::{native, world};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"name":"cache_test","prefab_root":"#cache_test","origin":{"x":10.,"y":20.,"z":30.},
        "loot_containers":[{"ref":"sword_cache","position":{"x":11.,"y":20.,"z":30.},
        "items":[{"id":"Items.Preset_Katana_Default","quantity":1}]}]})
}

#[test]
fn container_generates_inventory_override_and_owned_loot_records() {
    let build = world::compile(&spec()).unwrap();
    let yaml: Value =
        serde_yaml::from_slice(&build.loose["r6/tweaks/cache_test_loot.yaml"]).unwrap();
    assert_eq!(
        yaml["LootTables.cache_test_sword_cache_item_000"]["itemID"],
        "Items.Preset_Katana_Default"
    );
    let sector = build
        .artifacts
        .values()
        .find(|doc| doc["Data"]["RootChunk"]["$type"] == "worldStreamingSector")
        .unwrap();
    let serialized = serde_json::to_string(sector).unwrap();
    assert!(serialized.contains("LootContainerObjectAnimatedByTransformPS"));
    assert!(serialized.contains("LootTables.cache_test_sword_cache"));
    let mut invalid = spec();
    invalid["loot_containers"][0]["items"][0]["quantity"] = json!(0);
    assert!(world::compile(&invalid).is_err());
    invalid["loot_containers"][0]["items"][0]["quantity"] = json!(1);
    invalid["loot_containers"][0]["items"][0]["chance"] = json!(0.);
    let optional = world::compile(&invalid).unwrap();
    let optional: Value =
        serde_yaml::from_slice(&optional.loose["r6/tweaks/cache_test_loot.yaml"]).unwrap();
    assert_eq!(
        optional["LootTables.cache_test_sword_cache"]["minItemsToLoot"],
        0
    );
}

#[test]
fn quest_build_requires_generated_loot_tweaks_in_the_package() {
    use ghostline_quest_builder::{compiler, manifest};
    let source = json!({"schema_version":1,"id":"cache_test","title":"Cache test",
        "stages":[{"id":"wait","type":"time_gate","phase_resource":"mod\\cache_test\\wait.questphase","seconds":1,"completion_fact":"cache_test_done"}]});
    let checked = manifest::check(&source).unwrap();
    let mut inputs = compiler::Inputs::default();
    inputs.worlds.push(spec());
    let build = compiler::compile(
        checked.manifest.as_ref().unwrap(),
        &checked.composition,
        &inputs,
        None,
        false,
    )
    .unwrap();
    assert!(
        build
            .required_loose
            .contains("r6/tweaks/cache_test_loot.yaml")
    );
    assert!(
        build
            .resources
            .contains_key("r6/tweaks/cache_test_loot.yaml")
    );
}

#[test]
#[ignore = "requires GHOSTLINE_TEST_SCHEMA and GHOSTLINE_TEST_GAME"]
fn native_loot_container_readback() {
    let schema = ghostline_red::schema::RedSchema::from_game_files(
        std::path::Path::new(&std::env::var_os("GHOSTLINE_TEST_GAME").unwrap()),
        std::path::Path::new(&std::env::var_os("GHOSTLINE_TEST_SCHEMA").unwrap()),
    )
    .unwrap();
    let build = world::compile(&spec()).unwrap();
    assert_eq!(
        native::encode(&build.artifacts, &schema).unwrap().len(),
        build.artifacts.len()
    );
    let mut mesh = spec();
    mesh["loot_containers"][0]["mesh"] = json!("base\\test.mesh");
    let build = world::compile(&mesh).unwrap();
    assert_eq!(
        native::encode(&build.artifacts, &schema).unwrap().len(),
        build.artifacts.len()
    );
}
