use ghostline_quest_builder::{compiler, manifest::Stage};
use serde_json::{Value, json};

fn contains(value: &Value, kind: &str) -> bool {
    match value {
        Value::Object(fields) => {
            fields.get("$type").and_then(Value::as_str) == Some(kind)
                || fields.values().any(|value| contains(value, kind))
        }
        Value::Array(values) => values.iter().any(|value| contains(value, kind)),
        _ => false,
    }
}

#[test]
fn reading_a_shard_does_not_require_the_consumed_inventory_item() {
    let stage: Stage = serde_json::from_value(json!({
        "id":"read","type":"read_shard","phase_resource":"mod\\read_sequence\\read.questphase",
        "item":"Items.TestShard","journal_entry":"onscreens/emails/quests/minor_quest/read_sequence/files/data",
        "file_entry_index":0,"objective":"quests/minor_quest/read_sequence/read","grant_item":false
    })).unwrap();
    let phase = compiler::compile_stage(&stage).unwrap();
    assert!(contains(&phase, "questJournalEntryVisited_ConditionType"));
    assert!(
        !contains(&phase, "questInventory_ConditionType"),
        "reading may already have consumed the physical shard"
    );
    assert!(
        !contains(&phase, "questJournalEntry_NodeType"),
        "the first shard's read wait must not reactivate the existing entry"
    );
    let mut granted_here = stage;
    granted_here.fields.insert("grant_item".into(), json!(true));
    let phase = compiler::compile_stage(&granted_here).unwrap();
    assert!(
        contains(&phase, "questJournalEntry_NodeType"),
        "later shards are activated when this block grants them"
    );
    assert!(contains(&phase, "questJournalEntryVisited_ConditionType"));
}
