use super::*;
pub(super) fn build(spec: &Value) -> Result<Value> {
    let mut f = Flow::new()?;
    let objective = s(spec, "objective")?;
    let pin = s(spec, "mappin")?;
    let community = s(spec, "community")?;
    let contact = s(spec, "contact")?;
    let active = f.node(|g, id| n::objective(g, id, objective, false))?;
    let mut tail = f.append(&f.start.clone(), active, "Active")?;
    let description = f.node(|g, id| {
        n::journal(
            g,
            id,
            s(spec, "description_entry")?,
            "gameJournalQuestDescription",
            2,
        )
    })?;
    tail = f.append(&tail, description, "Active")?;
    let show = f.node(|g, id| n::mappin(g, id, pin, true))?;
    tail = f.append(&tail, show, "Active")?;
    let spawn =
        f.node(|g, id| n::community(g, id, community, "Activate", Some(contact), Some("default")))?;
    tail = f.append(&tail, spawn, "In")?;
    let ready = f.node(|g, id| n::spawned(g, id, community, None))?;
    tail = f.append(&tail, ready, "In")?;
    let friendly=f.node(|g,id|{let subtype=g.wrap(json!({"$type":"questCharacterManagerParameters_SetAttitudeGroupForPuppet","groupName":n::cname("friendly"),"isPlayer":0,"puppetRef":n::entity(Some(community),&[contact])}));let kind=g.wrap(json!({"$type":"questCharacterManagerParameters_NodeType","subtype":subtype}));g.node(id,"questCharacterManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))})?;
    tail = f.append(&tail, friendly, "In")?;
    let protect=f.node(|g,id|{let subtype=g.wrap(json!({"$type":"questCharacterManagerParameters_SetMortality","isPlayer":0,"puppetRef":n::entity(Some(community),&[contact]),"resetToDefault":0,"source":n::cname(s(spec,"protection_source")?),"state":"Invulnerable"}));let kind=g.wrap(json!({"$type":"questCharacterManagerParameters_NodeType","subtype":subtype}));g.node(id,"questCharacterManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))})?;
    tail = f.append(&tail, protect, "In")?;
    let setup = f.node(|g, id| n::trigger(g, id, s(spec, "setup_trigger")?, "IsInside"))?;
    f.connect(&tail, &setup, "In")?;
    let accept = spec
        .get("accept_exit")
        .and_then(Value::as_str)
        .unwrap_or("job_accept");
    let defer = spec
        .get("defer_exit")
        .and_then(Value::as_str)
        .unwrap_or("end");
    ensure!(accept != defer, "accept/defer exits must differ");
    let entry = spec.get("entry").and_then(Value::as_str).unwrap_or("start");
    let origin = s(spec, "scene_origin")?;
    let path = s(spec, "scene")?;
    let main = Scene {
        scene: path.into(),
        origin: origin.into(),
        entry: entry.into(),
        exit: accept.into(),
        completion_exits: vec![defer.into()],
    };
    let outside = f.node(|g, id| n::trigger(g, id, s(spec, "rearm_trigger")?, "IsOutside"))?;
    let accepted = if let Some(deferred) = spec.get("deferred_fact").and_then(Value::as_str) {
        let branch = f.node(|g, id| n::fact_condition(g, id, deferred, "Greater", 0, true))?;
        f.connect(&setup, &branch, "In")?;
        let first = f.scene(&branch.port("False"), &main)?;
        let resume = Scene {
            entry: spec
                .get("resume_entry")
                .and_then(Value::as_str)
                .unwrap_or("resume")
                .into(),
            ..main
        };
        let returned = f.scene(&branch.port("True"), &resume)?;
        let mark = f.node(|g, id| n::fact(g, id, deferred, 1))?;
        f.connect(&first.port(defer), &mark, "In")?;
        f.connect(&returned.port(defer), &mark, "In")?;
        f.connect(&mark, &outside, "In")?;
        let join = f.node(|g, id| n::join(g, id, 2, false))?.port("Out1");
        f.connect(&first, &join, "In1")?;
        f.connect(&returned, &join, "In2")?;
        join
    } else {
        let accepted = f.scene(&setup, &main)?;
        f.connect(&accepted.port(defer), &outside, "In")?;
        accepted
    };
    f.connect(&outside, &setup, "In")?;
    let done = f.node(|g, id| n::objective(g, id, objective, false))?;
    tail = f.append(&accepted, done, "Succeeded")?;
    let hide = f.node(|g, id| n::mappin(g, id, pin, false))?;
    tail = f.append(&tail, hide, "Inactive")?;
    let mark = f.node(|g, id| n::fact(g, id, s(spec, "success_fact")?, 1))?;
    tail = f.append(&tail, mark, "In")?;
    if let Some(item) = spec.get("reward_item").and_then(Value::as_str) {
        let reward = f.node(|g, id| n::item(g, id, item, u(spec, "reward_quantity", 1)?, false))?;
        tail = f.append(&tail, reward, "In")?;
    }
    f.finish(&tail)?;
    f.document(spec)
}
