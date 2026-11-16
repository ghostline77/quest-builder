use super::*;
fn call(f: &mut Flow, spec: &Value, phase: &str) -> Result<Signal> {
    f.node(|g,id|{let caller=n::journal_path(g,s(spec,"caller")?,"gameJournalContact",1);let addressee=n::journal_path(g,spec.get("addressee").and_then(Value::as_str).unwrap_or("contacts/player"),"gameJournalContact",1);let kind=g.wrap(json!({"$type":"questCallContact_NodeType","caller":caller,"addressee":addressee,"phase":phase,"mode":"Audio","prefabNodeRef":n::node_ref(spec.get("prefab").and_then(Value::as_str).unwrap_or("#holocalls_studio")),"applyPhoneRestriction":0,"isRejectable":0,"showAvatar":0,"visuals":"Default"}));g.node(id,"questPhoneManagerNodeDefinition",&["In"],&["Out"],json!({"type":kind}))})
}
pub(super) fn build(spec: &Value) -> Result<Value> {
    let mut f = Flow::new()?;
    let guarded = flag(spec, "guard_completed", true)?;
    let fresh = f.node(|g, id| {
        n::fact_condition(
            g,
            id,
            s(spec, "completed_fact")?,
            if guarded { "Greater" } else { "Equal" },
            0,
            guarded,
        )
    })?;
    f.connect(&f.start.clone(), &fresh, "In")?;
    if guarded {
        f.finish(&fresh.port("True"))?;
    }
    let mut tail = if guarded { fresh.port("False") } else { fresh };
    let skipped = spec
        .get("skipped_stages")
        .cloned()
        .map(serde_json::from_value::<Vec<String>>)
        .transpose()?
        .unwrap_or_default();
    let stages = spec["stages"]
        .as_array()
        .context("root lifecycle requires stage bindings")?;
    let mut ids = std::collections::HashSet::new();
    for stage in stages {
        let id = s(stage, "id")?;
        ensure!(ids.insert(id), "duplicate lifecycle stage {id}");
        if skipped.iter().any(|v| v == id) {
            continue;
        }
        if let Some(phone) = spec.get("call") {
            for (key, phase) in [("start_before", "StartCall"), ("end_before", "EndCall")] {
                if phone[key] == id {
                    let call = call(&mut f, phone, phase)?;
                    tail = f.append(&tail, call, "In")?;
                }
            }
        }
        let child = f
            .node(|g, id| n::phase(g, id, s(stage, "phase_resource")?, &["In1"], &["Out1"]))?
            .port("Out1");
        tail = f.append(&tail, child, "In1")?;
    }
    for skipped in &skipped {
        ensure!(
            ids.contains(skipped.as_str()),
            "skipped stage {skipped} is not declared"
        );
    }
    if let Some(phone) = spec.get("call") {
        for key in ["start_before", "end_before"] {
            ensure!(
                ids.contains(s(phone, key)?),
                "phone boundary does not name a stage"
            );
        }
    }
    let outside = f.node(|g, id| n::trigger(g, id, s(spec, "cleanup_trigger")?, "IsOutside"))?;
    tail = f.append(&tail, outside, "In")?;
    let release = f.node(|g, id| {
        n::community(
            g,
            id,
            s(spec, "cleanup_community")?,
            "Deactivate",
            None,
            None,
        )
    })?;
    tail = f.append(&tail, release, "In")?;
    f.finish(&tail)?;
    f.document(spec)
}
