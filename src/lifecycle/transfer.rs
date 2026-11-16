use super::*;
pub(super) fn build(spec: &Value) -> Result<Value> {
    let mut f = Flow::new()?;
    let objective = s(spec, "objective")?;
    let pin = s(spec, "mappin")?;
    let device = s(spec, "device")?;
    let activate = f.node(|g, id| n::objective(g, id, objective, false))?;
    let mut tail = f.append(&f.start.clone(), activate, "Active")?;
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
    if let Some(scene) = scene(spec, "intro_scene")? {
        tail = f.scene(&tail, &scene)?;
    }
    let enable =
        f.node(|g, id| n::device_action(g, id, device, "AccessPointControllerPS", "ForceON"))?;
    tail = f.append(&tail, enable, "In")?;
    let child = f
        .node(|g, id| {
            n::phase(
                g,
                id,
                s(spec, "attempt_resource")?,
                &["In1"],
                &["Out1", "Retry"],
            )
        })?
        .port("Out1");
    f.connect(&tail, &child, "In1")?;
    if let Some(scene) = scene(spec, "retry_scene")? {
        let retried = f.scene(&child.port("Retry"), &scene)?;
        f.connect(&retried, &child, "In1")?;
    } else {
        f.connect(&child.port("Retry"), &child, "In1")?;
    }
    tail = child;
    let fact = f.node(|g, id| n::fact(g, id, s(spec, "success_fact")?, 1))?;
    tail = f.append(&tail, fact, "In")?;
    let tick = f.tick()?;
    tail = f.append(&tail, tick, "In")?;
    for action in ["QuestForceDisconnectPersonalLink", "ForceDisabled"] {
        let node =
            f.node(|g, id| n::device_action(g, id, device, "AccessPointControllerPS", action))?;
        tail = f.append(&tail, node, "In")?;
    }
    let done = f.node(|g, id| n::objective(g, id, objective, false))?;
    tail = f.append(&tail, done, "Succeeded")?;
    let hide = f.node(|g, id| n::mappin(g, id, pin, false))?;
    tail = f.append(&tail, hide, "Inactive")?;
    if let Some(scene) = scene(spec, "success_scene")? {
        tail = f.scene(&tail, &scene)?;
    }
    f.finish(&tail)?;
    f.document(spec)
}
