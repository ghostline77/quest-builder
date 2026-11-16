//! Resolves authoring names into private native scene identities.
use super::*;

type Names = BTreeMap<String, u32>;

fn key(row: &Value) -> Result<&str> {
    let key = string(row, "key")?;
    ensure!(crate::manifest::identifier(key), "invalid scene key {key}");
    Ok(key)
}

fn lookup(names: &Names, value: &Value, context: &str) -> Result<u32> {
    let name = value
        .as_str()
        .with_context(|| format!("{context} must reference a name"))?;
    names
        .get(name)
        .copied()
        .with_context(|| format!("unknown {context} {name}"))
}

fn reject(row: &Value, fields: &[&str]) -> Result<()> {
    for field in fields {
        ensure!(
            row.get(*field).is_none(),
            "{field} is compiler-owned; use a named reference"
        );
    }
    Ok(())
}

fn table(spec: &mut Value, field: &str, id_field: Option<&str>) -> Result<Names> {
    let Some(rows) = spec.get_mut(field) else {
        return Ok(Names::new());
    };
    let rows = rows
        .as_array_mut()
        .with_context(|| format!("{field} must be an array"))?;
    let mut names = Names::new();
    for (index, row) in rows.iter_mut().enumerate() {
        let name = key(row)?.to_owned();
        let id = u32::try_from(index)?;
        ensure!(
            names.insert(name.clone(), id).is_none(),
            "duplicate {field} key {name}"
        );
        if let Some(field) = id_field {
            reject(row, &[field])?;
            row[field] = json!(id);
        }
    }
    Ok(names)
}

fn array_references(row: &mut Value, field: &str, names: &Names) -> Result<()> {
    if let Some(values) = row.get_mut(field) {
        for value in values
            .as_array_mut()
            .with_context(|| format!("{field} must list names"))?
        {
            *value = json!(lookup(names, value, field)?);
        }
    }
    Ok(())
}

fn reference(row: &mut Value, field: &str, native: &str, names: &Names) -> Result<()> {
    reject(row, &[native])?;
    let value = row
        .as_object_mut()
        .context("named reference object")?
        .remove(field)
        .with_context(|| format!("missing named {field} reference"))?;
    row[native] = json!(lookup(names, &value, field)?);
    Ok(())
}

fn node_rows(spec: &Value) -> Result<Vec<(String, Value)>> {
    let mut result = Vec::new();
    for field in ["start_node", "end_node"] {
        if let Some(row) = spec.get(field) {
            result.push((field.into(), row.clone()));
        }
    }
    for field in [
        "start_nodes",
        "end_nodes",
        "sections",
        "choices",
        "quest_nodes",
        "xor_nodes",
    ] {
        result.extend(
            list(spec, field)?
                .iter()
                .map(|row| (field.into(), row.clone())),
        );
    }
    result.extend(
        list(spec, "performance_blocks")?
            .iter()
            .filter(|v| v["kind"] != "pose_beat")
            .map(|row| ("performance_blocks".into(), row.clone())),
    );
    Ok(result)
}

fn destinations(
    value: &mut Value,
    nodes: &Names,
    inputs: &BTreeMap<String, Vec<String>>,
) -> Result<()> {
    match value {
        Value::Array(rows) => {
            for row in rows {
                destinations(row, nodes, inputs)?;
            }
        }
        Value::Object(row) => {
            ensure!(
                !row.contains_key("node_id") && !row.contains_key("target_node_id"),
                "numeric scene node references are compiler-owned"
            );
            if let Some(target) = row.remove("node") {
                let id = lookup(nodes, &target, "scene node")?;
                let name = target.as_str().context("node name")?;
                let ports = &inputs[name];
                ensure!(
                    !row.contains_key("input_ordinal") && !row.contains_key("input_name"),
                    "use an input socket name"
                );
                let port = row.remove("input");
                let port = port
                    .as_ref()
                    .map(|v| v.as_str().context("input socket name"))
                    .transpose()?
                    .unwrap_or("In");
                let ordinal = ports
                    .iter()
                    .position(|p| p == port)
                    .with_context(|| format!("node {name} has no input {port}"))?;
                ensure!(
                    port != "CutDestination",
                    "ordinary flow cannot enter CutDestination"
                );
                row.insert("node_id".into(), json!(id));
                row.insert("input_ordinal".into(), json!(ordinal));
            } else if row.contains_key("choice_key") {
                let target = row
                    .remove("target")
                    .context("choice option needs a named target")?;
                ensure!(
                    !row.contains_key("target_input_ordinal"),
                    "choice input ordinals are compiler-owned"
                );
                let target_name = target.as_str().context("choice target name")?;
                let input = row.remove("input");
                let input = input
                    .as_ref()
                    .map(|v| v.as_str().context("choice input name"))
                    .transpose()?
                    .unwrap_or("In");
                ensure!(
                    input != "CutDestination",
                    "ordinary choice flow cannot enter CutDestination"
                );
                let ordinal = inputs[target_name]
                    .iter()
                    .position(|name| name == input)
                    .with_context(|| format!("choice target {target_name} has no input {input}"))?;
                row.insert("target_input_ordinal".into(), json!(ordinal));
                row.insert(
                    "target_node_id".into(),
                    json!(lookup(nodes, &target, "choice target")?),
                );
            }
            for child in row.values_mut() {
                destinations(child, nodes, inputs)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Lowers symbolic scene authoring. Numeric identities are never author inputs.
pub(super) fn lower(source: &Value) -> Result<Value> {
    let mut spec = source.clone();
    for (single, multiple) in [
        ("start_node", "start_nodes"),
        ("end_node", "end_nodes"),
        ("entry_point", "entry_points"),
    ] {
        ensure!(
            spec.get(single).is_none() || spec.get(multiple).is_none(),
            "choose {single} or {multiple}"
        );
    }
    let rows = node_rows(&spec)?;
    let mut nodes = Names::new();
    let mut inputs = BTreeMap::new();
    for (field, row) in &rows {
        reject(row, &["node_id"])?;
        let name = key(row)?.to_owned();
        ensure!(
            nodes
                .insert(name.clone(), u32::try_from(nodes.len())? + 1)
                .is_none(),
            "duplicate scene node key {name}"
        );
        let ports = match field.as_str() {
            "quest_nodes" => commands::input_names(row)?,
            "performance_blocks" => vec!["CutDestination".into(), "In".into()],
            "end_node" | "end_nodes" => vec!["In".into()],
            _ => vec!["In".into()],
        };
        inputs.insert(name, ports);
    }
    // Resolve authoring references before adding private node_id fields.
    destinations(&mut spec, &nodes, &inputs)?;
    for field in ["start_node", "end_node"] {
        if let Some(row) = spec.get_mut(field) {
            row["node_id"] = json!(nodes[key(row)?]);
        }
    }
    for field in [
        "start_nodes",
        "end_nodes",
        "sections",
        "choices",
        "quest_nodes",
        "xor_nodes",
        "performance_blocks",
    ] {
        if let Some(rows) = spec.get_mut(field).and_then(Value::as_array_mut) {
            for row in rows {
                if field != "performance_blocks" || row["kind"] != "pose_beat" {
                    row["node_id"] = json!(nodes[key(row)?]);
                }
            }
        }
    }
    array_references(&mut spec, "graph_order", &nodes)?;
    for field in ["quest_nodes", "performance_blocks"] {
        if let Some(rows) = spec.get_mut(field).and_then(Value::as_array_mut) {
            for row in rows {
                ensure!(
                    row.get("workspot_ref") != Some(&json!(0)),
                    "use workspot_ref: current for the actor's current workspot"
                );
                if row["workspot_ref"] == "current" {
                    row["workspot_ref"] = json!(0);
                }
            }
        }
    }

    let mut actors = Names::new();
    let mut next = 0u32;
    let mut player = false;
    let player_id = u32::try_from(
        list(&spec, "actors")?
            .iter()
            .filter(|row| row["kind"] != "player")
            .count(),
    )?;
    let mut next_lipsync = 0u32;
    if let Some(rows) = spec.get_mut("actors").and_then(Value::as_array_mut) {
        for row in rows {
            reject(row, &["id"])?;
            let name = key(row)?.to_owned();
            let id = if row["kind"] == "player" {
                ensure!(!player, "scene may define only one real player");
                player = true;
                player_id
            } else {
                let id = next;
                next = next.checked_add(1).context("actor ID overflow")?;
                id
            };
            ensure!(
                actors.insert(name.clone(), id).is_none(),
                "duplicate actor key {name}"
            );
            row["id"] = json!(id);
            let lipsync = row
                .get("lipsync")
                .map(|v| v.as_bool().context("lipsync must be a boolean"))
                .transpose()?
                .unwrap_or(row["kind"] != "spawn");
            row["lipsync"] = json!(if lipsync {
                let slot = next_lipsync;
                next_lipsync = next_lipsync
                    .checked_add(1)
                    .context("lipsync slot overflow")?;
                slot
            } else {
                u32::MAX
            });
        }
    }
    table(&mut spec, "props", Some("id"))?;
    if let Some(rows) = spec.get_mut("choices").and_then(Value::as_array_mut) {
        for row in rows {
            if row.get("position").is_none() {
                reference(row, "actor", "actor_id", &actors)?;
            } else {
                ensure!(
                    row.get("actor").is_none(),
                    "choose an actor or a scene-local interaction position"
                );
            }
        }
    }
    let resources = table(&mut spec, "rid_resources", Some("resource_id"))?;
    let animations = table(&mut spec, "rid_animations", None)?;
    let cameras = table(&mut spec, "rid_camera_animations", None)?;
    let body_sets = table(&mut spec, "rid_anim_sets", None)?;
    let face_sets = table(&mut spec, "rid_facial_anim_sets", None)?;
    let workspots = table(&mut spec, "workspots", Some("id"))?;
    if let Some(commands) = spec.get_mut("quest_nodes").and_then(Value::as_array_mut) {
        for command in commands {
            if command["kind"] == "use_scene_workspot" {
                reference(command, "workspot", "workspot_id", &workspots)?;
            }
        }
    }
    for field in ["rid_animations", "rid_camera_animations"] {
        if let Some(rows) = spec.get_mut(field).and_then(Value::as_array_mut) {
            for row in rows {
                reference(row, "resource", "resource_id", &resources)?;
            }
        }
    }
    for field in ["rid_anim_sets", "rid_facial_anim_sets"] {
        if let Some(rows) = spec.get_mut(field).and_then(Value::as_array_mut) {
            for row in rows {
                array_references(row, "animations", &animations)?;
            }
        }
    }
    if let Some(rows) = spec.get_mut("actors").and_then(Value::as_array_mut) {
        for row in rows {
            array_references(row, "rid_anim_sets", &body_sets)?;
            array_references(row, "rid_facial_anim_sets", &face_sets)?;
        }
    }
    if let Some(rows) = spec.get_mut("sections").and_then(Value::as_array_mut) {
        for row in rows {
            if let Some(events) = row.get_mut("rid_events").and_then(Value::as_array_mut) {
                for event in events {
                    reference(event, "animation", "anim_ref_id", &animations)?;
                }
            }
            if let Some(camera) = row.get_mut("rid_camera") {
                reference(camera, "animation", "camera_ref_id", &cameras)?;
            }
        }
    }
    if let Some(rows) = spec
        .get_mut("performance_blocks")
        .and_then(Value::as_array_mut)
    {
        for row in rows {
            if let Some(motions) = row.get_mut("root_motion").and_then(Value::as_object_mut) {
                for motion in motions.values_mut() {
                    reference(motion, "workspot", "workspot_id", &workspots)?;
                }
            }
        }
    }
    Ok(spec)
}
