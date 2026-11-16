use ghostline_quest_builder::{
    bootstrap::{self, Phase, Spec},
    compiler::Build,
    graph::Graph,
    nodes,
};
use serde_json::{Value, json};

fn build(output: Option<&str>) -> Build {
    let mut graph = Graph::default();
    let start = nodes::input(&mut graph, "In1").unwrap();
    if let Some(output) = output {
        let end = nodes::output(&mut graph, 1, output).unwrap();
        graph.then(start, end).unwrap();
    }
    let mut build = Build::default();
    let path = "mod\\fixture\\root.questphase";
    build
        .artifacts
        .insert(path.into(), graph.document(path, &[]).unwrap());
    build.resources.insert("fixture.archive.xl".into(), serde_json::to_vec(&json!({"quest":{"phases":[{"parent":"base\\quest\\cyberpunk2077.quest","path":path}]},"journal":["mod\\fixture\\journal.journal"]})).unwrap());
    build
}
fn spec(output: Option<&str>) -> Spec {
    Spec::new(
        "fixture".into(),
        vec![Phase {
            resource: "mod\\fixture\\root.questphase".into(),
            input: "In1".into(),
            output: output.map(str::to_owned),
        }],
    )
}

#[test]
fn finite_bootstrap_signals_host_and_uses_native_scene_registration() {
    let mut build = build(Some("Out1"));
    bootstrap::attach(&mut build, spec(Some("Out1"))).unwrap();
    let registration: Value =
        serde_json::from_slice(&build.resources["fixture.archive.xl"]).unwrap();
    assert!(registration.get("quest").is_none());
    assert_eq!(registration["journal"][0], "mod\\fixture\\journal.journal");
    assert_eq!(
        registration["streaming"]["blocks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let scene = &build.artifacts["base\\open_world\\scenes\\generic_vendors\\versions\\gold\\chinese_food_store.scene"];
    let text = scene.to_string();
    assert!(text.contains("fixture_bootstrap_requested"));
    assert!(text.contains("fixture_bootstrap_dispatched"));
    assert!(text.contains("~/../fluff_vendor_ws"));
    assert!(!text.contains("race_interaction_probe"));
    assert!(!text.contains("mq000"));
    assert!(!text.contains("questPhaseNodeDefinition"));
    ghostline_quest_builder::graph::order_handles(scene).unwrap();
    assert!(bootstrap::attach(&mut build, spec(Some("Out1"))).is_err());
}

#[test]
fn persistent_bootstrap_requires_the_real_child_input() {
    let mut build = build(None);
    let mut invalid = spec(None);
    invalid.phases[0].input = "Missing".into();
    assert!(bootstrap::attach(&mut build, invalid).is_err());
}
