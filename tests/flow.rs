use ghostline_quest_builder::manifest;
use serde_json::{Value, json};

fn handoff() -> Value {
    json!({"schema_version":1,"id":"handoff","title":"Handoff","stages":[
        {"id":"escort","type":"escort_npc","phase_resource":"mod\\handoff\\escort.questphase","community":"#allies","entry":"ally","objective":"quests/minor_quest/handoff/phase/escort","mappin":"quests/minor_quest/handoff/phase/escort/pin","destinations":["#gate_one","#gate_two","#gate_three"],"completion_fact":"escort_done","failure_fact":"escort_failed","actor_lifecycle":{"on_enter":"assign_follower","on_success":"retain","on_failure":"release"},"outcomes":{"success":"Out1","failure":"Failed"},"on":{"success":"defend","failure":"$end"}},
        {"id":"defend","type":"defend_target","phase_resource":"mod\\handoff\\defend.questphase","community":"#allies","entry":"ally","objective":"quests/minor_quest/handoff/phase/defend","completion_fact":"defend_done","failure_fact":"defend_failed","actor_lifecycle":{"on_enter":"retain","on_success":"release","on_failure":"release"},"outcomes":{"success":"Out1","failure":"Failed"},"on":{"success":"$end","failure":"$end"}}
    ]})
}

#[test]
fn actor_ownership_handoffs_require_an_owner_on_every_route_and_cleanup_at_end() {
    let raw = handoff();
    let checked = manifest::check(&raw).unwrap();
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let mut missing = raw.clone();
    missing["stages"][0]["actor_lifecycle"]["on_success"] = json!("release");
    let checked = manifest::check(&missing).unwrap();
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|v| v.code == "ownership_not_acquired"),
        "{:?}",
        checked.diagnostics
    );
    let mut leak = raw;
    leak["stages"][1]["actor_lifecycle"]["on_success"] = json!("retain");
    let checked = manifest::check(&leak).unwrap();
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|v| v.code == "ownership_leak"),
        "{:?}",
        checked.diagnostics
    );
}

#[test]
fn a_closed_cycle_is_reported_as_nonterminating() {
    let raw = json!({"schema_version":1,"id":"cycle","title":"Cycle","stages":[{"id":"wait","type":"time_gate","phase_resource":"mod\\cycle\\wait.questphase","seconds":1,"completion_fact":"cycle_waited","on":{"success":"wait"}}]});
    let checked = manifest::check(&raw).unwrap();
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|v| v.code == "nonterminating_flow"),
        "{:?}",
        checked.diagnostics
    );
}
