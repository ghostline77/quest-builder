use super::{Block, Signal};
use crate::{
    graph::{Graph, NodeId},
    manifest::Stage,
    nodes as n,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
fn s(v: &Value) -> Result<&str> {
    v.as_str().context("cyberpsycho binding must be a string")
}

fn mortality(
    g: &mut Graph,
    id: u32,
    stage: &Stage,
    state: &str,
    player: bool,
    defeat: bool,
    reset: bool,
) -> Result<NodeId> {
    let subtype=g.wrap(json!({"$type":"questCharacterManagerParameters_SetMortality","isPlayer":u8::from(player),"puppetRef":if player{n::entity(None,&[])}else{n::entity(Some(stage.string("community")?),&[stage.string("boss_entry")?])},"resetToDefault":u8::from(reset),"source":n::cname(&if defeat{format!("{}_player_defeat",stage.id)}else{stage.id.clone()}),"state":state}));
    let kind =
        g.wrap(json!({"$type":"questCharacterManagerParameters_NodeType","subtype":subtype}));
    g.node(
        id,
        "questCharacterManagerNodeDefinition",
        &["In"],
        &["Out"],
        json!({"type":kind}),
    )
}
fn ai(g: &mut Graph, id: u32, stage: &Stage, tier: Option<&str>) -> Result<NodeId> {
    let mut entry = json!({"$type":"questPuppetAIManagerNodeDefinitionEntry","entityReference":n::entity(Some(stage.string("community")?),&[stage.string("boss_entry")?])});
    if let Some(tier) = tier {
        entry["aiTier"] = json!(tier);
    }
    g.node(
        id,
        "questPuppetAIManagerNodeDefinition",
        &["In"],
        &["Out"],
        json!({"entries":[entry]}),
    )
}
fn scene(b: &mut Block<'_>, flow: &Value) -> Result<Signal> {
    let mut exits = vec![s(&flow["exit"])?];
    for exit in flow
        .get("completion_exits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        exits.push(s(exit)?);
    }
    for branch in flow
        .get("completion_branches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        exits.push(s(&branch["exit"])?);
    }
    let mut entries = vec![s(&flow["entry"])?];
    for entry in flow
        .get("additional_entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        entries.push(s(entry)?);
    }
    b.node(|g, id| {
        n::scene(
            g,
            id,
            s(&flow["scene"])?,
            s(&flow["origin"])?,
            &entries,
            &exits,
        )
    })
}
fn restore_player(b: &mut Block<'_>, tail: Signal) -> Result<Signal> {
    let stage = b.stage;
    let node = b.node(|g, id| mortality(g, id, stage, "Mortal", true, true, true))?;
    b.append(&tail, node, "In")
}

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;
    let community = stage.string("community")?;
    let boss = stage.string("boss_entry")?;
    let activate = b.node(|g, id| {
        n::trigger(
            g,
            id,
            stage.string("activation_trigger")?,
            stage.optional("activation_mode").unwrap_or("Entered"),
        )
    })?;
    let start = b.start.clone();
    let mut tail = b.append(&start, activate, "In")?;
    tail = b.objective(tail, "enter")?;
    if let Some(path) = stage.optional("description_entry") {
        let node = b.node(|g, id| n::journal(g, id, path, "gameJournalQuestDescription", 2))?;
        tail = b.append(&tail, node, "Active")?;
    }
    tail = b.pin(tail, "Active", false)?;
    if stage
        .fields
        .get("activate")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        let node = b.node(|g, id| n::community(g, id, community, "Activate", None, None))?;
        tail = b.append(&tail, node, "In")?;
    }
    let ready = b.node(|g, id| n::spawned(g, id, community, Some(boss)))?;
    tail = b.append(&tail, ready, "In")?;
    let protect = b.node(|g, id| mortality(g, id, stage, "Invulnerable", false, false, false))?;
    tail = b.append(&tail, protect, "In")?;
    let reveal = &stage.fields["reveal"];
    let mut reveals = Vec::new();
    if let Some(trigger) = reveal.get("trigger").and_then(Value::as_str) {
        let node = b.node(|g, id| n::trigger(g, id, trigger, "Entered"))?;
        b.connect(&tail, &node, "In")?;
        reveals.push(node);
    }
    for key in [
        "scan",
        "attacked_by_boss",
        "boss_hit_by_player",
        "boss_sees_player",
    ] {
        if reveal.get(key).and_then(Value::as_bool) == Some(true) {
            let node=b.node(|g,id|{
        let boss=n::entity(Some(community),&[boss]);let (wrapper,condition)=match key{
            "scan"=>("questObjectCondition",json!({"$type":"questScan_ConditionType","objectRef":boss})),
            "attacked_by_boss"=>("questCharacterCondition",json!({"$type":"questCharacterAttack_ConditionType","attackerRef":boss,"isTargetPlayer":1})),
            "boss_hit_by_player"=>("questCharacterCondition",json!({"$type":"questCharacterHit_ConditionType","isAttackerPlayer":1,"targetRef":boss})),
            _=>("questSensesCondition",json!({"$type":"questVision_ConditionType","observerPuppetRef":boss}))};let kind=g.wrap(condition);let condition=g.wrap(json!({"$type":wrapper,"type":kind}));g.node(id,"questPauseConditionNodeDefinition",&["In"],&["Out"],json!({"condition":condition}))})?;
            b.connect(&tail, &node, "In")?;
            reveals.push(node);
        }
    }
    ensure!(
        !reveals.is_empty(),
        "cyberpsycho requires at least one explicit reveal route"
    );
    tail = if reveals.len() == 1 {
        reveals.remove(0)
    } else {
        b.join(&reveals, false)?
    };
    if let Some(trigger) = stage.optional("arena_trigger") {
        let node = b.node(|g, id| {
            n::trigger(
                g,
                id,
                trigger,
                stage.optional("arena_mode").unwrap_or("Entered"),
            )
        })?;
        tail = b.append(&tail, node, "In")?;
    }
    let gameplay = b.node(|g, id| ai(g, id, stage, None))?;
    tail = b.append(&tail, gameplay.clone(), "In")?;
    let mortal = b.node(|g, id| mortality(g, id, stage, "Mortal", false, false, false))?;
    tail = b.append(&tail, mortal, "In")?;
    if let Some(path) = stage.optional("alerted_path") {
        let spots = stage
            .fields
            .get("alerted_spots")
            .and_then(Value::as_array)
            .context("alerted patrol spots")?;
        if !spots.is_empty() {
            let delay = b.node(|g, id| n::real_delay(g, id, 0, 200))?;
            tail = b.append(&gameplay, delay, "In")?;
            let peaceful=b.node(|g,id|{let kind=g.wrap(json!({"$type":"questCharacterCombat_ConditionType","objectRef":n::entity(Some(community),&[boss]),"isPlayer":0,"inverted":1}));let condition=g.wrap(json!({"$type":"questCharacterCondition","type":kind}));g.node(id,"questPauseConditionNodeDefinition",&["In"],&["Out"],json!({"condition":condition}))})?;
            let peaceful = b.append(&tail, peaceful, "In")?;
            let patrol=b.node(|g,id|{let normal=g.wrap(json!({"$type":"AIPatrolPathParameters","movementType":"Sprint","patrolWithWeapon":1}));let alerted=g.wrap(json!({"$type":"AIPatrolPathParameters","path":n::node_ref(path),"movementType":"Sprint","patrolWithWeapon":1}));let workspots=g.wrap(json!({"$type":"AIbehaviorWorkspotList","spots":spots.iter().map(|v|Ok(n::node_ref(s(v)?))).collect::<Result<Vec<_>>>()?}));let role=g.wrap(json!({"$type":"AIPatrolRole","pathParams":normal,"alertedPathParams":alerted,"alertedSpots":workspots,"forceAlerted":1}));let params=g.wrap(json!({"$type":"AIAssignRoleCommandParams","role":role}));g.node(id,"questMiscAICommandNode",&["In"],&["Success"],json!({"entityReference":n::entity(Some(community),&[boss]),"params":params}))})?;
            b.connect(&peaceful, &patrol, "In")?;
        }
    }
    let target=b.node(|g,id|{let params=g.wrap(json!({"$type":"questCombatNodeParams_CombatTarget","duration":0.01,"immediately":1,"targetNode":{"$type":"NodeRef","$storage":"uint64","$value":"0"},"targetPuppet":n::entity(Some("#player"),&[])}));g.node(id,"questCombatNodeDefinition",&["In"],&["Success"],json!({"entityReference":n::entity(Some(community),&[boss]),"params":params}))})?.port("Success");
    tail = b.append(&tail, target, "In")?;
    let defeat = stage
        .fields
        .get("player_defeat_scene")
        .filter(|v| v.is_object());
    if defeat.is_some() {
        let immortal = b.node(|g, id| mortality(g, id, stage, "Immortal", true, true, false))?;
        tail = b.append(&tail, immortal, "In")?;
    }
    let threat = b.node(|g, id| n::combat_threat(g, id, community, boss, None, 0.5))?;
    b.connect(&tail, &threat, "In")?;
    let resolution = &stage.fields["resolution"];
    let killed =
        b.node(|g, id| n::character_outcome(g, id, community, boss, true, false, false))?;
    let spared = b.node(|g, id| n::character_outcome(g, id, community, boss, false, true, true))?;
    b.connect(&tail, &killed, "In")?;
    b.connect(&tail, &spared, "In")?;
    let killed = b.fact(killed, Some(s(&resolution["killed_fact"])?))?;
    let spared = b.fact(spared, Some(s(&resolution["spared_fact"])?))?;
    let player_defeat = defeat
        .map(|flow| defeat_route(&mut b, tail.clone(), flow, resolution))
        .transpose()?;
    let killed = if defeat.is_some() {
        restore_player(&mut b, killed)?
    } else {
        killed
    };
    let mut spared = spared;
    if let Some(flow) = stage
        .fields
        .get("postfight_scene")
        .filter(|v| v.is_object())
    {
        let node = scene(&mut b, flow)?;
        b.connect(&spared, &node, s(&flow["entry"])?)?;
        spared = node.port(s(&flow["exit"])?);
    }
    if defeat.is_some() {
        spared = restore_player(&mut b, spared)?;
    }
    let mut outcomes = vec![killed, spared];
    if let Some(defeated) = player_defeat {
        outcomes.push(defeated);
    }
    tail = b.join(&outcomes, false)?;
    tail = b.objective(tail, "success")?;
    tail = b.pin(tail, "Inactive", false)?;
    tail = b.fact(tail, stage.optional("completion_fact"))?;
    if let Some(cleanup) = stage.fields.get("cleanup").filter(|v| v.is_object()) {
        if let Some(trigger) = cleanup.get("trigger").and_then(Value::as_str) {
            let node = b.node(|g, id| n::trigger(g, id, trigger, "IsOutside"))?;
            tail = b.append(&tail, node, "In")?;
        }
        if cleanup
            .get("deactivate_community")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            let node = b.node(|g, id| n::community(g, id, community, "Deactivate", None, None))?;
            tail = b.append(&tail, node, "In")?;
        }
    }
    b.finish(tail, "success")?;
    b.document()
}

fn defeat_route(
    b: &mut Block<'_>,
    hostile: Signal,
    flow: &Value,
    resolution: &Value,
) -> Result<Signal> {
    let stage = b.stage;
    let community = stage.string("community")?;
    let boss = stage.string("boss_entry")?;
    let health=b.node(|g,id|{let kind=g.wrap(json!({"$type":"questCharacterHealth_ConditionType","comparisonType":"LessOrEqual","isPlayer":1,"objectRef":n::entity(None,&[]),"percent":flow.get("health_percent").cloned().unwrap_or(json!(5))}));let condition=g.wrap(json!({"$type":"questCharacterCondition","type":kind}));g.node(id,"questPauseConditionNodeDefinition",&["In"],&["Out"],json!({"condition":condition}))})?;
    let debug =
        b.node(|g, id| n::fact_condition(g, id, s(&flow["debug_fact"])?, "Greater", 0, false))?;
    b.connect(&hostile, &health, "In")?;
    b.connect(&hostile, &debug, "In")?;
    let mut tail = b.join(&[health, debug], false)?;
    let protect = b.node(|g, id| mortality(g, id, stage, "Invulnerable", false, true, false))?;
    tail = b.append(&tail, protect, "In")?;
    let friendly=b.node(|g,id|{let subtype=g.wrap(json!({"$type":"questCharacterManagerParameters_SetAttitudeGroupForPuppet","groupName":n::cname("friendly"),"isPlayer":0,"puppetRef":n::entity(Some(community),&[boss])}));let kind=g.wrap(json!({"$type":"questCharacterManagerParameters_NodeType","subtype":subtype}));g.node(id,"questCharacterManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))})?;
    tail = b.append(&tail, friendly, "In")?;
    let clear = b
        .node(|g, id| n::follower(g, id, community, boss, false))?
        .port("Success");
    tail = b.append(&tail, clear, "In")?;
    let cinematic = b.node(|g, id| ai(g, id, stage, Some("Cinematic")))?;
    tail = b.append(&tail, cinematic, "In")?;
    tail = b.fact(tail, Some(s(&flow["defeat_fact"])?))?;
    tail = b.fact(tail, Some(s(&resolution["spared_fact"])?))?;
    let delay = b.node(|g, id| n::real_delay(g, id, 0, 200))?;
    tail = b.append(&tail, delay, "In")?;
    let heal=b.node(|g,id|{let subtype=g.wrap(json!({"$type":"questCharacterManagerCombat_ModifyHealth","damageSourceRef":n::entity(None,&[]),"isPlayer":1,"noDamageIndicator":1,"percent":100,"puppetRef":n::entity(None,&[]),"setExactValue":1}));let kind=g.wrap(json!({"$type":"questCharacterManagerCombat_NodeType","subtype":subtype}));g.node(id,"questCharacterManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))})?;
    tail = b.append(&tail, heal, "In")?;
    let loss_scene = scene(b, flow)?;
    b.connect(&tail, &loss_scene, s(&flow["entry"])?)?;
    tail = loss_scene.port(s(&flow["exit"])?);
    for key in ["continuation_scene", "aftermath_scene"] {
        if let Some(following) = flow.get(key).filter(|v| v.is_object()) {
            let node = scene(b, following)?;
            b.connect(&tail, &node, s(&following["entry"])?)?;
            tail = node.port(s(&following["exit"])?);
        }
    }
    let mut completions = vec![tail];
    for exit in flow
        .get("completion_exits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        completions.push(loss_scene.port(s(exit)?));
    }
    for branch in flow
        .get("completion_branches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        completions.push(b.fact(
            loss_scene.port(s(&branch["exit"])?),
            Some(s(&branch["set_fact"])?),
        )?);
    }
    let tail = if completions.len() == 1 {
        completions.remove(0)
    } else {
        b.join(&completions, false)?
    };
    restore_player(b, tail)
}
