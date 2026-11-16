//! Procedural performance recipes and explicit workspot lifecycle expansion.
use super::*;
mod events;
mod presets;

pub(super) fn expand(spec: &Value) -> Result<Value> {
    let mut expanded = spec.clone();
    let mut keys = HashSet::new();
    let mut generated = Vec::new();
    for block in list(spec, "performance_blocks")? {
        let key = string(block, "key")?;
        ensure!(keys.insert(key), "duplicate performance block {key}");
        let kind = string(block, "kind")?;
        if kind == "pose_beat" {
            continue;
        }
        ensure!(
            ["workspot_move", "workspot_exit", "workspot_idle"].contains(&kind),
            "unsupported performance block {kind}"
        );
        let actor = string(block, "actor")?;
        let actor = list(spec, "actors")?
            .iter()
            .find(|v| v["key"] == actor)
            .context("workspot block actor is missing")?;
        ensure!(
            actor["kind"] == "community",
            "workspot blocks need a community actor"
        );
        ensure!(
            block.get("node_id").is_some() && block.get("on_success").is_some(),
            "workspot block needs node_id and on_success"
        );
        integer(block, "node_id", 0)?;
        if kind != "workspot_idle" {
            let reference = string(block, "workspot_ref")?;
            ensure!(
                reference.starts_with('#') || reference.starts_with("$/"),
                "workspot reference must be explicit"
            );
        }
        if kind == "workspot_move" {
            ensure!(
                block.get("on_work_started").is_some()
                    && block
                        .get("is_workspot_infinite")
                        .is_some_and(Value::is_boolean),
                "movement requires work-started routing and an explicit infinite policy"
            );
        }
        if kind == "workspot_exit" {
            ensure!(
                block.get("exit_entry_id").is_some(),
                "workspot exit requires exit_entry_id"
            );
        }
        let mut node = json!({"key":key,"kind":if kind=="workspot_exit"{"stop_workspot"}else{"use_workspot"},"node_id":block["node_id"],"entity_ref":actor["community_ref"],"entry":actor["entry"],"workspot_ref":block.get("workspot_ref").cloned().unwrap_or(json!(0)),"teleport":false,"instant":false,"repeat_on_interrupt":false,"finish_animation":true,"movement_type":"Walk","is_workspot_infinite":block.get("is_workspot_infinite").cloned().unwrap_or(json!(false)),"on_success":block["on_success"],"on_work_started":block.get("on_work_started").cloned().unwrap_or(json!([]))});
        if kind == "workspot_idle" {
            node["function"] = json!("IdleOnlyMode");
            node["enable_idle_mode"] = json!(true);
            node["is_workspot_infinite"] = json!(true);
            node["repeat_on_interrupt"] = json!(true);
            node["teleport"] = json!(true);
        }
        for field in [
            "entry_id",
            "exit_entry_id",
            "exit_animation",
            "movement_type",
            "repeat_on_interrupt",
        ] {
            if let Some(value) = block.get(field) {
                node[field] = value.clone();
            }
        }
        let allowed = [
            "key",
            "kind",
            "actor",
            "node_id",
            "on_success",
            "workspot_ref",
            "on_work_started",
            "is_workspot_infinite",
            "entry_id",
            "exit_entry_id",
            "exit_animation",
            "movement_type",
            "repeat_on_interrupt",
        ];
        for key in block.as_object().context("performance declaration")?.keys() {
            ensure!(
                allowed.contains(&key.as_str()),
                "unsupported workspot performance field {key}"
            );
        }
        generated.push(node);
    }
    if !generated.is_empty() {
        if expanded.get("quest_nodes").is_none() {
            expanded["quest_nodes"] = json!([]);
        }
        expanded["quest_nodes"]
            .as_array_mut()
            .context("scene quest nodes")?
            .extend(generated);
    }
    Ok(expanded)
}

fn workspots(root: &mut Value, spec: &Value, h: &mut Handles) -> Result<HashSet<u32>> {
    let mut ids = HashSet::new();
    let mut data = Vec::new();
    let mut instances = Vec::new();
    for row in list(spec, "workspots")? {
        let index = integer(row, "id", u32::MAX)?;
        ensure!(
            index != u32::MAX && ids.insert(index),
            "invalid/duplicate scene workspot ID"
        );
        let path = string(row, "resource")?;
        depot_path(&path.replace('/', "\\"))?;
        data.push(h.wrap(json!({"$type":"scnWorkspotData_ExternalWorkspotResource","dataId":id("scnSceneWorkspotDataId",index),"workspotResource":resource(path,false)})));
        let origin = if let Some(actor) = row.get("origin_actor") {
            ensure!(row.get("origin").is_none(), "choose origin or origin_actor");
            let actor = list(spec, "actors")?.iter().find(|a| a["key"] == *actor)
                .context("workspot origin actor missing")?;
            let entity = if actor["kind"] == "player" {
                n::entity(Some("#player"), &[])
            } else {
                n::entity(Some(string(actor, "community_ref")?), &[string(actor, "entry")?])
            };
            json!({"$type":"scnMarker","type":"Entity","entityRef":entity,
                "nodeRef":n::node_ref(""),"slotName":n::cname("None"),
                "localMarkerId":n::cname("None"),"isMounted":1})
        } else {
            events::marker(string(row, "origin")?)
        };
        instances.push(json!({"$type":"scnWorkspotInstance","dataId":id("scnSceneWorkspotDataId",index),"workspotInstanceId":id("scnSceneWorkspotInstanceId",index),"localTransform":events::transform(row.get("offset"))?,"playAtActorLocation":flag(row,"play_at_actor_location",false)?,"originMarker":origin}));
    }
    root["workspots"] = json!(data);
    root["workspotInstances"] = json!(instances);
    Ok(ids)
}

pub(super) fn apply(root: &mut Value, spec: &Value, h: &mut Handles) -> Result<()> {
    let workspots = workspots(root, spec, h)?;
    let mut animation_refs = BTreeMap::<String, usize>::new();
    for block in list(spec, "performance_blocks")? {
        if block["kind"] != "pose_beat" {
            continue;
        }
        let key = string(block, "key")?;
        let recipe = presets::recipe(string(block, "preset")?)?;
        let actor_bindings = block["actors"]
            .as_object()
            .context("performance actor bindings")?;
        ensure!(
            actor_bindings.len() == recipe.roles.len(),
            "bind exactly the performance roles"
        );
        let mut bound = BTreeMap::new();
        let mut actor_ids = HashSet::new();
        for (role, player) in &recipe.roles {
            let name = actor_bindings
                .get(*role)
                .and_then(Value::as_str)
                .context("unbound performance role")?;
            let actor = list(spec, "actors")?
                .iter()
                .find(|v| v["key"] == name)
                .context("unknown performance actor")?;
            ensure!(
                (actor["kind"] == "player") == *player,
                "performance role {role} has wrong actor kind"
            );
            let aid = integer(actor, "id", u32::MAX)?;
            ensure!(
                actor_ids.insert(aid),
                "performance roles require distinct actors"
            );
            bound.insert(*role, (name, aid));
        }
        let section_name = string(block, "section")?;
        let section_id = list(spec, "sections")?
            .iter()
            .find(|v| v["key"] == section_name)
            .context("performance section missing")?;
        let section_id = integer(section_id, "node_id", u32::MAX)?;
        let graph = root["sceneGraph"]["Data"]["graph"]
            .as_array()
            .context("scene graph")?;
        let index = graph
            .iter()
            .position(|v| v["Data"]["nodeId"]["id"] == section_id)
            .context("performance section node missing")?;
        let mut offset = integer(block, "start_ms", 0)?;
        if let Some(anchor) = block.get("anchor_line") {
            let anchor = anchor.as_str().context("performance anchor key")?;
            let line_index = list(spec, "spoken_line_order")?
                .iter()
                .position(|v| v == anchor)
                .context("anchor line absent from screenplay")?;
            let item = 1 + u32::try_from(line_index)? * 256;
            let anchors = list(&graph[index]["Data"], "events")?
                .iter()
                .filter(|v| v["Data"]["screenplayLineId"]["id"] == item)
                .collect::<Vec<_>>();
            ensure!(anchors.len() == 1, "anchor line must occur once in section");
            offset = offset
                .checked_add(integer(&anchors[0]["Data"], "startTime", 0)?)
                .context("anchor timing overflow")?;
        }
        let empty = json!({});
        let ports = block
            .get("on")
            .unwrap_or(&empty)
            .as_object()
            .context("performance output bindings")?;
        let ignored = list(block, "ignore_ports")?
            .iter()
            .map(|v| v.as_str().context("ignored performance port"))
            .collect::<Result<HashSet<_>>>()?;
        ensure!(
            ignored.len() == list(block, "ignore_ports")?.len(),
            "duplicate ignored performance port"
        );
        ensure!(
            ports.len() + ignored.len() == recipe.ports.len()
                && recipe
                    .ports
                    .iter()
                    .all(|p| ports.contains_key(*p) ^ ignored.contains(p)),
            "bind or explicitly ignore every performance port"
        );
        for destinations in ports.values() {
            let destinations = destinations
                .as_array()
                .filter(|v| !v.is_empty())
                .context("performance port needs destinations")?;
            for dest in destinations {
                let target = integer(dest, "node_id", u32::MAX)?;
                let target = graph
                    .iter()
                    .find(|v| v["Data"]["nodeId"]["id"] == target)
                    .context("performance destination missing")?;
                let ordinal = integer(dest, "input_ordinal", 0)?;
                let name = integer(dest, "input_name", 0)?;
                ensure!(
                    name == 0
                        && if target["Data"]["$type"] == "scnQuestNode" {
                            ordinal > 0
                                && (ordinal as usize)
                                    < list(&target["Data"], "isockMappings")?.len()
                        } else {
                            ordinal == 0
                        },
                    "performance destination is not executable"
                );
            }
        }
        let mut emitted = Vec::new();
        let mut used_motion = HashSet::new();
        let mut outgoing = Vec::new();
        let mut next_socket = 2 + u32::try_from(
            list(&graph[index]["Data"], "outputSockets")?
                .iter()
                .filter(|v| v["stamp"]["name"].as_u64().is_some_and(|v| v >= 2))
                .count(),
        )?;
        for (i, beat) in recipe.beats.iter().enumerate() {
            if let presets::Beat::Signal { port, .. } = beat {
                if ignored.contains(port) {
                    continue;
                }
                let destinations = ports[*port].as_array().context("signal destinations")?;
                outgoing.push(graph::socket(next_socket, 0, destinations)?);
                let event =
                    events::signal(spec, key, i, beat, offset, next_socket, recipe.duration, h)?;
                emitted.push(h.wrap(event));
                next_socket += 1;
            } else {
                let event = events::build(
                    spec,
                    block,
                    key,
                    i,
                    beat,
                    offset,
                    &bound,
                    &workspots,
                    &mut used_motion,
                    h,
                )?;
                emitted.push(h.wrap(event));
            }
        }
        let motions = block
            .get("root_motion")
            .unwrap_or(&empty)
            .as_object()
            .context("root motion bindings")?;
        ensure!(
            motions.len() == used_motion.len()
                && motions.keys().all(|k| used_motion.contains(k.as_str())),
            "unused or missing root-motion bindings"
        );
        let section = &mut root["sceneGraph"]["Data"]["graph"][index]["Data"];
        for event in &emitted {
            if event["Data"]["$type"] == "scnPlaySkAnimEvent" {
                for other in list(section, "events")? {
                    let other = &other["Data"];
                    if other["$type"] == "scnPlaySkAnimEvent"
                        && other["performer"] == event["Data"]["performer"]
                        && other["actorComponent"] == event["Data"]["actorComponent"]
                    {
                        let a = integer(other, "startTime", 0)?;
                        let b = integer(&event["Data"], "startTime", 0)?;
                        ensure!(
                            a + integer(other, "duration", 0)? <= b
                                || b + integer(&event["Data"], "duration", 0)? <= a,
                            "overlapping full-body performance blocks"
                        );
                    }
                }
            }
        }
        section["events"]
            .as_array_mut()
            .context("performance events")?
            .extend(emitted);
        section["outputSockets"]
            .as_array_mut()
            .context("performance sockets")?
            .extend(outgoing);
        let end = offset
            .checked_add(recipe.duration)
            .context("performance timing overflow")?;
        section["sectionDuration"]["stu"] =
            json!(integer(&section["sectionDuration"], "stu", 0)?.max(end));
        for binding in &recipe.bindings {
            let path = binding.path();
            let refs = &mut root["resouresReferences"];
            let index = if let Some(index) = animation_refs.get(&path) {
                *index
            } else {
                let index = list(refs, "cinematicAnimSets")?.len();
                refs["cinematicAnimSets"].as_array_mut().context("cinematic references")?.push(json!({"$type":"scnCinematicAnimSetSRRef","asyncAnimSet":resource(&path,true),"isOverride":1,"priority":binding.priority}));
                refs["cinematicAnimNames"]
                    .as_array_mut()
                    .context("cinematic names")?
                    .push(json!({"$type":"scnAnimSetAnimNames","animationNames":[]}));
                animation_refs.insert(path, index);
                index
            };
            let names = refs["cinematicAnimNames"][index]["animationNames"]
                .as_array_mut()
                .context("clip names")?;
            for beat in &recipe.beats {
                if let presets::Beat::Body { role, clip, .. } = beat
                    && role == &binding.role
                {
                    let name = n::cname(clip);
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
            let aid = bound[binding.role].1;
            for collection in ["actors", "playerActors"] {
                for actor in root[collection].as_array_mut().context("cast")? {
                    if actor["actorId"]["id"] == aid {
                        let grant = id("scnCinematicAnimSetSRRefId", u32::try_from(index)?);
                        let sets = actor["bodyCinematicAnimSets"]
                            .as_array_mut()
                            .context("body animation sets")?;
                        if !sets.contains(&grant) {
                            sets.push(grant);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
