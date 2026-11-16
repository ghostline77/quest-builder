use crate::{
    graph::{Graph, NodeId},
    manifest::StartGate,
};
use anyhow::Result;
use serde_json::json;

pub(super) fn build(graph: &mut Graph, id: u32, gate: StartGate) -> Result<NodeId> {
    let facts = match gate {
        // Playing for Time's cleanup commits q101_done after meeting Takemura.
        // Standalone expansion starts skip that route and publish their own fact.
        StartGate::AfterAct1 => ["q101_done", "ep1_standalone"],
    };
    let conditions = facts.map(|fact| {
        let comparison = graph.wrap(json!({"$type":"questVarComparison_ConditionType","comparisonType":"Greater","factName":fact,"value":0}));
        graph.wrap(json!({"$type":"questFactsDBCondition","type":comparison}))
    });
    let condition = graph
        .wrap(json!({"$type":"questLogicalCondition","operation":"OR","conditions":conditions}));
    graph.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}
