use super::*;
fn mark(f: &mut Flow, tail: Signal, fact: &str) -> Result<Signal> {
    let node = f.node(|g, id| n::fact(g, id, fact, 1))?;
    f.append(&tail, node, "In")
}
pub(super) fn finish(spec: &Value) -> Result<Value> {
    let mut f = Flow::new()?;
    let debrief =
        f.node(|g, id| n::fact_condition(g, id, s(spec, "debrief_fact")?, "Equal", 1, false))?;
    if flag(spec, "guard_completed", true)? {
        let completed = f.node(|g, id| {
            n::fact_condition(g, id, s(spec, "completed_fact")?, "Greater", 0, true)
        })?;
        f.connect(&f.start.clone(), &completed, "In")?;
        f.finish(&completed.port("True"))?;
        f.connect(&completed.port("False"), &debrief, "In")?;
    } else {
        f.connect(&f.start.clone(), &debrief, "In")?;
    }
    let settled = f.tick()?;
    let mut tail = f.append(&debrief, settled, "In")?;
    let owns = f.node(|g, id| {
        n::inventory(
            g,
            id,
            s(spec, "reward_item")?,
            u(spec, "reward_quantity", 1)?,
            true,
        )
    })?;
    f.connect(&tail, &owns, "In")?;
    let wait = f.node(|g, id| {
        n::inventory(
            g,
            id,
            s(spec, "reward_item")?,
            u(spec, "reward_quantity", 1)?,
            false,
        )
    })?;
    f.connect(&owns.port("False"), &wait, "In")?;
    let verified = f.node(|g, id| n::fact(g, id, s(spec, "verified_fact")?, 1))?;
    f.connect(&owns.port("True"), &verified, "In")?;
    f.connect(&wait, &verified, "In")?;
    tail = mark(&mut f, verified, s(spec, "granted_fact")?)?;
    let succeeded =
        f.node(|g, id| n::objective_tracking(g, id, s(spec, "quest_path")?, true, false))?;
    tail = f.append(&tail, succeeded, "Succeeded")?;
    tail = mark(&mut f, tail, s(spec, "completed_fact")?)?;
    if let Some(phase) = spec.get("epilogue_phase").and_then(Value::as_str) {
        let delay =
            f.node(|g, id| n::real_delay(g, id, u(spec, "epilogue_delay_seconds", 45)?, 0))?;
        tail = f.append(&tail, delay, "In")?;
        let epilogue = f
            .node(|g, id| n::phase(g, id, phase, &["In1"], &["Out1"]))?
            .port("Out1");
        tail = f.append(&tail, epilogue, "In1")?;
    }
    f.finish(&tail)?;
    f.document(spec)
}

pub(super) fn reconcile(spec: &Value) -> Result<Value> {
    let mut f = Flow::new()?;
    let completed =
        f.node(|g, id| n::fact_condition(g, id, s(spec, "completed_fact")?, "Equal", 1, false))?;
    let mut tail = f.append(&f.start.clone(), completed, "In")?;
    let debrief =
        f.node(|g, id| n::fact_condition(g, id, s(spec, "debrief_fact")?, "Equal", 1, false))?;
    tail = f.append(&tail, debrief, "In")?;
    let verified =
        f.node(|g, id| n::fact_condition(g, id, s(spec, "verified_fact")?, "Equal", 1, true))?;
    f.connect(&tail, &verified, "In")?;
    f.finish(&verified.port("True"))?;
    let owns = f.node(|g, id| {
        n::inventory(
            g,
            id,
            s(spec, "reward_item")?,
            u(spec, "reward_quantity", 1)?,
            true,
        )
    })?;
    f.connect(&verified.port("False"), &owns, "In")?;
    let grant = f.node(|g, id| {
        n::item(
            g,
            id,
            s(spec, "reward_item")?,
            u(spec, "reward_quantity", 1)?,
            false,
        )
    })?;
    let grant = f.append(&owns.port("False"), grant, "In")?;
    let confirm = f.node(|g, id| {
        n::inventory(
            g,
            id,
            s(spec, "reward_item")?,
            u(spec, "reward_quantity", 1)?,
            false,
        )
    })?;
    let confirm = f.append(&grant, confirm, "In")?;
    let mark = f.node(|g, id| n::fact(g, id, s(spec, "verified_fact")?, 1))?;
    f.connect(&owns.port("True"), &mark, "In")?;
    f.connect(&confirm, &mark, "In")?;
    tail = self::mark(&mut f, mark, s(spec, "granted_fact")?)?;
    f.finish(&tail)?;
    f.document(spec)
}
