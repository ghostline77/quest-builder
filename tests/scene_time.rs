use ghostline_quest_builder::scene;
use serde_json::{Value, json};

fn typed<'a>(value: &'a Value, name: &str, found: &mut Vec<&'a Value>) {
    match value {
        Value::Object(fields) => {
            if fields.get("$type").and_then(Value::as_str) == Some(name) {
                found.push(value);
            }
            for child in fields.values() {
                typed(child, name, found);
            }
        }
        Value::Array(items) => {
            for child in items {
                typed(child, name, found);
            }
        }
        _ => {}
    }
}

#[test]
fn evening_wait_uses_native_clock_and_arrival_conditions() {
    let spec = json!({
        "name":"evening", "archive_path":"mod\\test\\evening.scene",
        "actors":[{"key":"v","name":"V","kind":"player"}],
        "entry_point":{"name":"start","node":"start"},
        "start_node":{"key":"start","on_start":[{"node":"wait"},{"node":"arrival"}]},
        "exit_points":[{"name":"ready","node":"end"}],
        "end_node":{"key":"end"},
        "choice_line_order":["wait"],
        "choices":[{"key":"wait","position":{"x":0,"y":0,"z":0},"options":[{
            "choice_key":"wait","target":"advance",
            "available_time":{"begin_seconds":0,"end_seconds":64799}
        }]}],
        "quest_nodes":[
            {"key":"arrival","kind":"time_period","begin_seconds":64800,
             "end_seconds":86399,"trigger_ref":"#bar","on_out":[{"node":"end"}]},
            {"key":"advance","kind":"set_time","hours":18,"source":"test_wait",
             "on_out":[{"node":"end"}]}
        ]
    });
    let dialogue = json!({"spoken_lines":[],"choice_lines":[
        {"key":"wait","string_id":"762002000001","text":"Wait until 18:00"}
    ]});
    let resource = scene::compile(&spec, &dialogue).unwrap();
    let mut periods = Vec::new();
    typed(&resource, "questTimePeriod_ConditionType", &mut periods);
    assert!(
        periods
            .iter()
            .any(|p| p["begin"]["seconds"] == 64800 && p["end"]["seconds"] == 86399)
    );
    assert!(
        periods
            .iter()
            .any(|p| p["begin"]["seconds"] == 0 && p["end"]["seconds"] == 64799)
    );
    let mut arrivals = Vec::new();
    typed(&resource, "questTriggerCondition", &mut arrivals);
    assert_eq!(arrivals[0]["triggerAreaRef"]["$value"], "#bar");
    let mut clock = Vec::new();
    typed(&resource, "questSetTime_NodeType", &mut clock);
    assert_eq!(clock[0]["hours"], 18);
    let mut invalid = spec;
    invalid["quest_nodes"][0]["end_seconds"] = json!(86400);
    assert!(scene::compile(&invalid, &dialogue).is_err());
}
