use super::*;
fn progress(f: &mut Flow, spec: &Value, show: bool, duration: u64) -> Result<Signal> {
    f.node(|g,id|{let kind=g.wrap(json!({"$type":"questProgressBar_NodeType","duration":duration,"show":u8::from(show),"text":{"unk1":"0","value":s(spec,"label")?},"bottomText":{"unk1":"0","value":spec.get("bottom_text").and_then(Value::as_str).unwrap_or("Verifying transfer")},"type":"Connection"}));g.node(id,"questUIManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))})
}
pub(super) fn build(spec: &Value) -> Result<Value> {
    let duration = u(spec, "duration_seconds", 5)?;
    ensure!(duration > 0, "transfer duration must be positive");
    let mut f = Flow::new()?;
    let retry = n::output(&mut f.g, 2, "Retry")?;
    let device = s(spec, "device")?;
    let arm = f.node(|g, id| {
        n::device_action(
            g,
            id,
            device,
            "AccessPointControllerPS",
            "QuestForcePersonalLinkUnderStrictQuestControl",
        )
    })?;
    let mut tail = f.append(&f.start.clone(), arm, "In")?;
    let connected = f.node(|g, id| {
        n::device_condition(
            g,
            id,
            device,
            "AccessPointControllerPS",
            "IsPersonalLinkConnected",
        )
    })?;
    tail = f.append(&tail, connected, "In")?;
    let bar = progress(&mut f, spec, true, duration)?;
    tail = f.append(&tail, bar, "In")?;
    let timer = f.node(|g, id| n::real_delay(g, id, duration, 0))?;
    f.connect(&tail, &timer, "In")?;
    let broken = f.node(|g, id| {
        n::device_condition(
            g,
            id,
            device,
            "AccessPointControllerPS",
            "IsPersonalLinkDisconnected",
        )
    })?;
    f.connect(&tail, &broken, "In")?;
    // The HUD finishes at the timer boundary, even if connected dialogue is
    // still playing. Retire the failure branch at that same boundary.
    let complete = f.node(|g, id| {
        g.node(
            id,
            "questCutControlNodeDefinition",
            &["In"],
            &["Out", "CutSource"],
            json!({"permanent":1}),
        )
    })?;
    f.connect(&timer, &complete, "In")?;
    let abort = f.node(|g, id| {
        g.node(
            id,
            "questCutControlNodeDefinition",
            &["In"],
            &["Out", "CutSource"],
            json!({"permanent":1}),
        )
    })?;
    f.connect(&broken, &abort, "In")?;
    let hide = progress(&mut f, spec, false, duration)?;
    for target in [&broken, &abort, &hide] {
        f.connect(&complete.port("CutSource"), target, "CutDestination")?;
    }
    for target in [&timer, &complete] {
        f.connect(&abort.port("CutSource"), target, "CutDestination")?;
    }
    let finished = if let Some(scene) = scene(spec, "connected_scene")? {
        let heard = f.scene(&tail, &scene)?;
        f.connect(&abort.port("CutSource"), &heard, "CutDestination")?;
        let join = f.node(|g, id| n::join(g, id, 2, true))?.port("Out1");
        f.connect(&complete, &join, "In1")?;
        f.connect(&heard, &join, "In2")?;
        join
    } else {
        complete
    };
    let tick = f.tick()?;
    let finished = f.append(&finished, tick, "In")?;
    f.finish(&finished)?;
    let hide = f.append(&abort, hide, "In")?;
    f.g.connect(hide.node, &hide.port, retry, "In")?;
    f.document(spec)
}
