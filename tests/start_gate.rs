use ghostline_quest_builder::{audit, compiler, manifest::Manifest};
use serde_json::{Value, json};
use std::collections::HashMap;

fn definitions<'a>(value: &'a Value, result: &mut HashMap<String, &'a Value>) {
    if let Some(id) = value["HandleId"].as_str() {
        result.insert(id.into(), &value["Data"]);
    }
    match value {
        Value::Object(fields) => fields.values().for_each(|value| definitions(value, result)),
        Value::Array(values) => values.iter().for_each(|value| definitions(value, result)),
        _ => {}
    }
}

fn resolve<'a>(value: &'a Value, handles: &HashMap<String, &'a Value>) -> &'a Value {
    let id = value
        .get("HandleId")
        .or_else(|| value.get("HandleRefId"))
        .unwrap()
        .as_str()
        .unwrap();
    handles[id]
}

fn ready(value: &Value, handles: &HashMap<String, &Value>, q101: i64, standalone: i64) -> bool {
    let value = resolve(value, handles);
    match value["$type"].as_str().unwrap() {
        "questLogicalCondition" => {
            assert_eq!(value["operation"], "OR");
            value["conditions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|condition| ready(condition, handles, q101, standalone))
        }
        "questFactsDBCondition" => ready(&value["type"], handles, q101, standalone),
        "questVarComparison_ConditionType" => {
            assert_eq!(value["comparisonType"], "Greater");
            let fact = match value["factName"].as_str().unwrap() {
                "q101_done" => q101,
                "ep1_standalone" => standalone,
                other => panic!("unexpected startup fact {other}"),
            };
            fact > value["value"].as_i64().unwrap()
        }
        other => panic!("unexpected startup condition {other}"),
    }
}

#[test]
fn shared_gate_precedes_quest_conditions_and_waits_through_a_new_game() {
    let mut gate_authoring: Value =
        serde_json::from_str(include_str!("../examples/signal.quest.json")).unwrap();
    gate_authoring["start_gate"] = json!("after_act_1");
    let checked = ghostline_quest_builder::manifest::check(&gate_authoring).unwrap();
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert!(checked.manifest.unwrap().start_gate.is_some());
    let manifest: Manifest = serde_json::from_value(json!({
        "schema_version":1,"id":"test","title":"Test","start_gate":"after_act_1",
        "start_condition":{"fact":"test_completed","comparison":"Equal","value":0},
        "stages":[{"id":"offer","type":"wait_fact","phase_resource":"mod\\test\\offer.questphase","fact":"test_offer"}]
    })).unwrap();
    let document = compiler::orchestrate(&manifest, "mod\\test\\root.questphase").unwrap();
    audit::Phase::read(&document).unwrap();
    let mut handles = HashMap::new();
    definitions(&document, &mut handles);
    let graph = resolve(&document["Data"]["RootChunk"]["graph"], &handles);
    let start = graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| resolve(v, &handles))
        .find(|v| v["$type"] == "questInputNodeDefinition")
        .unwrap();
    let output = start["sockets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| resolve(v, &handles))
        .find(|v| v["type"] == "Output")
        .unwrap();
    let edge = resolve(&output["connections"][0], &handles);
    let gate_socket = resolve(&edge["destination"], &handles);
    let gate = graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| resolve(v, &handles))
        .find(|node| {
            node["sockets"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| std::ptr::eq(resolve(v, &handles), gate_socket))
        })
        .unwrap();
    assert_eq!(gate["$type"], "questPauseConditionNodeDefinition");
    for (q101, standalone, expected) in [(0, 0, false), (1, 0, true), (0, 1, true), (1, 1, true)] {
        assert_eq!(
            ready(&gate["condition"], &handles, q101, standalone),
            expected
        );
    }
    assert!(handles.values().any(|v| v["factName"] == "test_completed"));
}
