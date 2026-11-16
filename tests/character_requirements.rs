use ghostline_quest_builder::{compiler, manifest};
use serde_json::json;

#[test]
fn character_requirements_survive_build_publication() {
    let source = json!({"schema_version":1,"id":"shared_cast","title":"Shared cast",
        "stages":[{"id":"wait","type":"time_gate","phase_resource":"mod\\shared_cast\\wait.questphase","seconds":1,"completion_fact":"shared_cast_done"}]});
    let checked = manifest::check(&source).unwrap();
    let mut inputs = compiler::Inputs::default();
    inputs
        .required_loose
        .insert("r6/tweaks/series/character_contact.yaml".into());
    inputs
        .character_bundles
        .insert("mod\\series\\characters\\contact".into());
    let build = compiler::compile(
        checked.manifest.as_ref().unwrap(),
        &checked.composition,
        &inputs,
        None,
        false,
    )
    .unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let output = scratch.path().join("build");
    compiler::publish_build(&output, &build, &Default::default(), false).unwrap();
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("build.json")).unwrap()).unwrap();
    assert_eq!(
        receipt["required_loose"],
        json!(["r6/tweaks/series/character_contact.yaml"])
    );
    assert_eq!(
        receipt["character_bundles"],
        json!(["mod\\series\\characters\\contact"])
    );
}
