use ghostline_quest_builder::{evidence, inspect, install, manifest, scenario};
use serde_json::json;
use std::{fs, path::Path};

#[test]
fn routing_scenarios_replay_snapshots_and_reject_changed_event_identity() {
    let quest = json!({"schema_version":1,"id":"route","title":"Route","stages":[{"id":"one","type":"time_gate","phase_resource":"mod\\route\\one.questphase","seconds":1,"completion_fact":"one_done"},{"id":"two","type":"time_gate","phase_resource":"mod\\route\\two.questphase","seconds":1,"completion_fact":"two_done"}]});
    let checked = manifest::check(&quest).unwrap();
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let plan = checked.manifest.unwrap().plan().unwrap();
    let event = json!({"event_id":"a","stage":"one","outcome":"success"});
    let suite = json!({"schema_version":1,"scenarios":[{"id":"resume","steps":[{"event":event,"expect":{"active_stage":"two"}},{"save":"checkpoint"},{"event":{"event_id":"b","stage":"two","outcome":"success"},"expect":{"active_stage":null}},{"load":"checkpoint","expect":{"active_stage":"two"}},{"event":event,"expect":{"result":"duplicate"}},{"event":{"event_id":"c","stage":"two","outcome":"success"}}],"expect":{"terminal_outcome":"success"}}]});
    assert_eq!(scenario::run(&plan, &suite).unwrap()["passed"], true);
    let mut invalid = suite;
    invalid["scenarios"][0]["steps"][4]["event"]["stage"] = json!("two");
    assert_eq!(scenario::run(&plan, &invalid).unwrap()["passed"], false);
}

#[test]
fn parallel_graph_previews_branch_and_join_instead_of_showing_sequential_routes() {
    let stages=["a","b","c","d"].iter().map(|id|json!({"id":id,"type":"time_gate","phase_resource":format!("mod\\parallel\\{id}.questphase"),"seconds":1})).collect::<Vec<_>>();
    let checked=manifest::check(&json!({"schema_version":1,"id":"parallel","title":"Parallel","stages":stages,"parallel_groups":[{"id":"both","branches":[["a","b"],["c","d"]]}]})).unwrap();
    let plan = checked.manifest.unwrap().plan().unwrap();
    let graph = scenario::graph(&plan).unwrap();
    assert!(graph.contains("fork0 --> s0") && graph.contains("fork0 --> s2"));
    assert!(graph.contains("s1 --> join0") && graph.contains("s3 --> join0"));
    assert!(!graph.contains("s1 --> s2"));
}

#[test]
fn inspector_and_evidence_report_native_identities_and_stale_inputs() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input.json");
    let checked=manifest::check(&json!({"schema_version":1,"id":"inspect","title":"Inspect","stages":[{"id":"wait","type":"time_gate","phase_resource":"mod\\inspect\\wait.questphase","seconds":1,"completion_fact":"inspected"}]})).unwrap();
    let stage = &checked.manifest.unwrap().stages[0];
    let document = ghostline_quest_builder::compiler::compile_stage(stage).unwrap();
    let summary = inspect::summarize(&document).unwrap();
    assert_eq!(summary["facts"]["writes"], json!(["inspected"]));
    fs::write(&input, serde_json::to_vec(&document).unwrap()).unwrap();
    let receipt = evidence::record(
        std::slice::from_ref(&input),
        &[],
        &json!({"kind":"native_build","passed":true}),
    )
    .unwrap();
    assert_eq!(evidence::verify(&receipt).unwrap()["valid"], true);
    fs::write(input, b"changed").unwrap();
    assert_eq!(evidence::verify(&receipt).unwrap()["stale"], true);
}

fn package(directory: &Path, bytes: &[u8]) {
    let relative = "archive/pc/mod/sample.archive";
    fs::create_dir_all(directory.join("archive/pc/mod")).unwrap();
    fs::write(directory.join(relative), bytes).unwrap();
    fs::write(directory.join("verification.json"),serde_json::to_vec(&json!({"schema_version":1,"name":"sample","archive_verified":true,"zip_verified":true,"zip_sha256":"test_distribution_identity","files":{relative:evidence::digest(bytes)}})).unwrap()).unwrap();
}

#[test]
fn installation_updates_remove_only_owned_files_and_rollback_restores_original_bytes() {
    let scratch = tempfile::tempdir().unwrap();
    let game = scratch.path().join("game");
    fs::create_dir_all(game.join("bin/x64")).unwrap();
    fs::write(
        game.join("bin/x64/Cyberpunk2077.exe"),
        b"test marker, not an executable",
    )
    .unwrap();
    fs::create_dir_all(game.join("archive/pc/mod")).unwrap();
    let archive = game.join("archive/pc/mod/sample.archive");
    fs::write(&archive, b"original").unwrap();
    let package_dir = scratch.path().join("package");
    package(&package_dir, b"first");
    install::apply(&package_dir, &game).unwrap();
    assert_eq!(fs::read(&archive).unwrap(), b"first");
    assert_eq!(install::verify(&game, "sample").unwrap()["valid"], true);
    package(&package_dir, b"second");
    install::apply(&package_dir, &game).unwrap();
    install::rollback(&game, "sample").unwrap();
    assert_eq!(fs::read(&archive).unwrap(), b"first");
    install::rollback(&game, "sample").unwrap();
    assert_eq!(fs::read(&archive).unwrap(), b"original");
    install::apply(&package_dir, &game).unwrap();
    fs::write(&archive, b"independent edit").unwrap();
    assert_eq!(install::verify(&game, "sample").unwrap()["valid"], false);
    assert!(install::rollback(&game, "sample").is_err());
    assert_eq!(fs::read(&archive).unwrap(), b"independent edit");
}

#[test]
fn build_replacement_retains_a_verified_restore_point_and_rejects_changed_outputs() {
    use ghostline_quest_builder::{compiler, publication};
    use std::collections::BTreeMap;
    let scratch = tempfile::tempdir().unwrap();
    let first = scratch.path().join("first");
    let second = scratch.path().join("second");
    let current = scratch.path().join("current");
    let document = json!({"Header":{"ArchiveFileName":"mod\\example\\file.json"},"Data":{"RootChunk":{"$type":"JsonResource"}}});
    let docs = BTreeMap::from([("mod\\example\\file.json".into(), document)]);
    compiler::publish_raw(&first, &docs, false).unwrap();
    publication::replace(&first, &current).unwrap();
    compiler::publish_raw(&second, &docs, false).unwrap();
    let receipt = publication::replace(&second, &current).unwrap();
    let backup = Path::new(receipt["backup"].as_str().unwrap());
    publication::verify(backup).unwrap();
    publication::replace(backup, &current).unwrap();
    fs::write(current.join("notes.txt"), b"user owned data").unwrap();
    assert!(publication::verify(&current).is_err());
}

#[test]
fn research_indexes_reuse_exact_sources_and_bind_pointer_evidence() {
    use ghostline_quest_builder::{compiler, corpus};
    let scratch = tempfile::tempdir().unwrap();
    let file = scratch.path().join("phase.json");
    let checked=manifest::check(&json!({"schema_version":1,"id":"research","title":"Research","stages":[{"id":"wait","type":"time_gate","phase_resource":"mod\\research\\wait.questphase","seconds":1,"completion_fact":"research_done"}]})).unwrap();
    let doc = compiler::compile_stage(&checked.manifest.unwrap().stages[0]).unwrap();
    fs::write(&file, serde_json::to_vec(&doc).unwrap()).unwrap();
    let sources = vec![file.clone()];
    let first = corpus::build(&sources, None, None, &[]).unwrap();
    let second = corpus::build(&sources, None, Some(&first), &[]).unwrap();
    assert_eq!(second["counts"]["reused"], 1);
    let key = second["entries"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap();
    let entry = &second["entries"][key];
    let occurrence = entry["summary"]["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["type"] == "questSetVar_NodeType")
        .unwrap();
    let proof =
        corpus::evidence(&second, key, occurrence["pointer"].as_str().unwrap(), None).unwrap();
    assert_eq!(proof["value"]["factName"], "research_done");
    assert_eq!(
        corpus::analyze(&second).unwrap()["facts"]["research_done"]["writers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    fs::write(file, b"changed").unwrap();
    assert!(corpus::extract(&second, key).is_err());
}

#[test]
fn project_ownership_prefers_the_most_specific_declared_dependency() {
    use ghostline_quest_builder::project::Project;
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path();
    fs::create_dir(root.join("shared")).unwrap();
    fs::write(root.join("project.json"),serde_json::to_vec(&json!({"schema_version":1,"id":"main","depot_prefixes":["mod/example"],"dependencies":["shared"]})).unwrap()).unwrap();
    fs::write(
        root.join("shared/project.json"),
        serde_json::to_vec(
            &json!({"schema_version":1,"id":"shared","depot_prefixes":["mod/example/shared"]}),
        )
        .unwrap(),
    )
    .unwrap();
    let project = Project::open(root).unwrap();
    assert_eq!(
        project
            .owner("MOD/example/shared/actor.ent")
            .unwrap()
            .config
            .id,
        "shared"
    );
    assert_eq!(
        project
            .owner("mod/example/phases/start.questphase")
            .unwrap()
            .config
            .id,
        "main"
    );
    assert!(project.owner("mod/unowned/start.questphase").is_err());
}

#[test]
fn encounter_authoring_checks_supplied_world_and_explicit_boss_requirements() {
    use ghostline_quest_builder::{audit, manifest::Manifest};
    use std::collections::BTreeMap;
    let mut source: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/cyberpsycho_encounter.quest.json")).unwrap();
    source["stages"][0]["authoring"] =
        json!({"world_spec":"encounter.world.json","tweak_file":"boss.yaml"});
    let manifest: Manifest = serde_json::from_value(source).unwrap();
    let stage = &manifest.stages[0];
    let mut world = json!({"community":{"ref":stage.fields["community"],"entry":stage.fields["boss_entry"],"character":stage.fields["boss_character"],"active_on_start":false},"triggers":[{"ref":stage.fields["activation_trigger"]},{"ref":stage.fields["arena_trigger"]},{"ref":stage.fields["reveal"]["trigger"]},{"ref":stage.fields["cleanup"]["trigger"]}]});
    let yaml=b"Character.GhostlineGothBaddie:\n  rarity: NPCRarity.Boss\n  tags: !append [Cyberpsycho]\n  statModifiers: [Character.Cyberpsycho_ModGroup, Character.Cyberpsycho_HitReaction_Resistance]\n  target: TargetTracking.DefaultPreset\n  ui: UINameplate.CombatSettings\n  scanner: ScanningNPCPresets.ScannerPreset_NPCFull\n  entityTemplatePath: mod\\example\\boss.ent\n  displayName: Boss\n  fullDisplayName: Boss\n";
    let loose = BTreeMap::from([("r6/tweaks/boss.yaml".into(), yaml.to_vec())]);
    audit::encounters(&manifest, std::slice::from_ref(&world), &loose).unwrap();
    world["community"]["active_on_start"] = json!(true);
    assert!(audit::encounters(&manifest, &[world], &loose).is_err());
}
