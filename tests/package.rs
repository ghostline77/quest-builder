use ghostline_quest_builder::{compiler, manifest, native, package};
use serde_json::json;

#[test]
#[ignore = "requires GHOSTLINE_TEST_SCHEMA and optional GHOSTLINE_TEST_GAME"]
fn native_archive_and_zip_verify_the_frozen_dependency_closure() {
    let schema_path = std::env::var_os("GHOSTLINE_TEST_SCHEMA").expect("explicit schema");
    let path = std::path::Path::new(&schema_path);
    let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
        ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), path)
            .unwrap()
    } else {
        ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap()
    };
    let quest = json!({"schema_version":1,"id":"delivery","title":"Delivery","stages":[{"id":"wait","type":"time_gate","phase_resource":"mod\\delivery\\phases\\wait.questphase","seconds":1,"completion_fact":"delivery_done"}]});
    let mut quest = quest;
    quest["stages"][0]["required_assets"] = json!(["mod\\delivery\\audio\\line.wem"]);
    let scratch = tempfile::tempdir().unwrap();
    let audio = scratch.path().join("line.wem");
    std::fs::write(&audio, b"published external fixture bytes").unwrap();
    let tweaks = scratch.path().join("tweaks.yaml");
    std::fs::write(
        &tweaks,
        b"Items.delivery:\n  paths: !append\n    - mod\\delivery\\audio\\line.wem\n",
    )
    .unwrap();
    let mut inputs = compiler::Inputs::default();
    inputs.assets.insert(
        "mod\\delivery\\audio\\line.wem".into(),
        compiler::FileInput {
            path: audio.clone(),
        },
    );
    inputs.loose.insert(
        "r6/tweaks/delivery.yaml".into(),
        compiler::FileInput {
            path: tweaks.clone(),
        },
    );
    let checked = manifest::check(&quest).unwrap();
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let build = compiler::compile(
        checked.manifest.as_ref().unwrap(),
        &checked.composition,
        &inputs,
        None,
        false,
    )
    .unwrap();
    let binaries = native::encode_build(&build, &schema, None).unwrap();
    let build_path = scratch.path().join("build");
    compiler::publish_build(&build_path, &build, &binaries, false).unwrap();
    let profile = package::Profile {
        name: "delivery".into(),
        builds: vec!["build".into()],
        active_roots: vec!["mod\\delivery\\phases\\delivery.questphase".into()],
        include_loose: vec!["r6/tweaks/delivery.yaml".into()],
        ..Default::default()
    };
    let package = package::prepare(&profile, scratch.path()).unwrap();
    assert_eq!(package.payloads.len(), 3);
    assert_eq!(
        package.audit["dependency_additions"],
        json!([
            "mod\\delivery\\audio\\line.wem",
            "mod\\delivery\\phases\\wait.questphase"
        ])
    );
    let output = scratch.path().join("package");
    let receipt = package::publish(&package, &output).unwrap();
    assert_eq!(receipt["archive_verified"], true);
    assert_eq!(receipt["zip_verified"], true);
    assert!(output.join("delivery.zip").is_file());
    assert!(output.join("archive/pc/mod/delivery.archive.xl").is_file());
    assert!(package::publish(&package, &output).is_err());
    std::fs::write(
        build_path.join("source/archive/mod/delivery/phases/wait.questphase"),
        b"changed",
    )
    .unwrap();
    assert!(
        package::prepare(&profile, scratch.path())
            .err()
            .unwrap()
            .to_string()
            .contains("changed since receipt")
    );
    // Published resources are inspected as interfaces, never used as generation bases.
    let mut graph = ghostline_quest_builder::graph::Graph::default();
    let start = ghostline_quest_builder::nodes::input(&mut graph, "In1").unwrap();
    let end = ghostline_quest_builder::nodes::output(&mut graph, 1, "Out1").unwrap();
    graph.then(start, end).unwrap();
    let depot = "mod\\shared\\child.questphase";
    let document = graph.document(depot, &[]).unwrap();
    let bytes = native::encode(
        &std::collections::BTreeMap::from([(depot.into(), document)]),
        &schema,
    )
    .unwrap()
    .remove(depot)
    .unwrap();
    let external = scratch.path().join("published.questphase");
    std::fs::write(&external, &bytes).unwrap();
    let mut inputs = compiler::Inputs::default();
    inputs
        .assets
        .insert(depot.into(), compiler::FileInput { path: external });
    let mut source = json!({"schema_version":1,"id":"bound","title":"Bound","stages":[{"id":"call","type":"owned_scope","phase_resource":"mod\\bound\\call.questphase","child":{"phase":depot,"entry":"In1","exits":{"success":"Out1"}}}]});
    let checked = manifest::check(&source).unwrap();
    let build = compiler::compile(
        checked.manifest.as_ref().unwrap(),
        &checked.composition,
        &inputs,
        None,
        false,
    )
    .unwrap();
    native::encode_build(&build, &schema, None).unwrap();
    source["stages"][0]["child"]["entry"] = json!("MissingEntry");
    let checked = manifest::check(&source).unwrap();
    let build = compiler::compile(
        checked.manifest.as_ref().unwrap(),
        &checked.composition,
        &inputs,
        None,
        false,
    )
    .unwrap();
    assert!(native::encode_build(&build, &schema, None).is_err());
}
