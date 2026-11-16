use ghostline_quest_builder::workspot;
use serde_json::json;

#[test]
fn rejects_ambiguous_entries_and_invalid_random_choices() {
    let mut spec = json!({"depot":"mod\\fixture\\rest.workspot","rig":"base\\characters\\base_entities\\woman_base\\woman_base.rig","animsets":["mod\\fixture\\rest.anims"],"root":{"kind":"sequence","key":"root","entries":[{"kind":"clip","key":"root","animation":"rest"}]}});
    assert!(
        workspot::compile(&spec)
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    spec["root"]["entries"][0]["key"] = json!("rest");
    assert!(workspot::compile(&spec).is_ok());
    spec["root"] = json!({"kind":"random","key":"root","entries":[{"kind":"clip","key":"rest","animation":"rest"}],"min_clips":1,"max_clips":2,"pause_seconds":1,"pause_deviation":0,"weights":[0]});
    assert!(
        workspot::compile(&spec)
            .unwrap_err()
            .to_string()
            .contains("weights")
    );
    spec["root"]["weights"] = json!([1]);
    assert!(workspot::compile(&spec).is_ok());
    spec["root"]["entries"][0]["animation"] = json!("");
    assert!(workspot::compile(&spec).is_err());
}
