use super::*;
use std::collections::VecDeque;

pub(super) struct Graph {
    pub nodes: Vec<Value>,
    pub entries: Vec<Value>,
    pub exits: Vec<Value>,
}

pub(super) fn socket(name: u32, ordinal: u32, destinations: &[Value]) -> Result<Value> {
    let destinations=destinations.iter().map(|v|Ok(json!({"$type":"scnInputSocketId","nodeId":id("scnNodeId",integer(v,"node_id",u32::MAX)?),"isockStamp":{"$type":"scnInputSocketStamp","name":integer(v,"input_name",0)?,"ordinal":integer(v,"input_ordinal",0)?}}))).collect::<Result<Vec<_>>>()?;
    Ok(
        json!({"$type":"scnOutputSocket","destinations":destinations,"stamp":{"$type":"scnOutputSocketStamp","name":name,"ordinal":ordinal}}),
    )
}
fn simple(raw: &Value, kind: &str, key: &str, h: &mut Handles) -> Result<Value> {
    let mut node = json!({"$type":kind,"ffStrategy":"automatic","nodeId":id("scnNodeId",integer(raw,"node_id",u32::MAX)?),"outputSockets":if kind=="scnEndNode"{vec![]}else{vec![socket(0,0,list(raw,key)?)?]}});
    if let Some(end_type) = raw.get("end_type") {
        ensure!(kind == "scnEndNode", "end_type requires an end node");
        ensure!(matches!(end_type.as_str(), Some("Terminating" | "NonTerminating")), "invalid scene end_type");
        node["type"] = end_type.clone();
    }
    Ok(h.wrap(node))
}
fn choice(raw: &Value, text: &text::Text, h: &mut Handles) -> Result<Value> {
    let mut options = Vec::new();
    let mut sockets = Vec::new();
    for (ordinal, row) in list(raw, "options")?.iter().enumerate() {
        let key = string(row, "choice_key")?;
        let option = *text
            .choices
            .get(key)
            .with_context(|| format!("choice {key} missing from screenplay order"))?;
        let tags = list(row, "icon_tags")?
            .iter()
            .map(|v| {
                Ok(tweak(
                    v.as_str()
                        .filter(|v| !v.is_empty())
                        .context("choice icon tag")?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        options.push(json!({"$type":"scnChoiceNodeOption","blueline":0,"bluelineCondition":null,"caption":n::cname(row.get("caption").and_then(Value::as_str).unwrap_or("None")),"emphasisCondition":null,"exDataFlags":0,"gameplayAction":tweak("0"),"iconCondition":null,"iconTagIds":tags,"isFixedAsRead":0,"isSingleChoice":flag(row,"single_choice",false)?,"mappinReferencePointId":id("scnReferencePointId",u32::MAX),"questCondition":null,"screenplayOptionId":id("scnscreenplayItemId",option),"timedCondition":null,"timedParams":null,"triggerCondition":null,"type":{"$type":"gameinteractionsChoiceTypeWrapper","properties":integer(row,"choice_type",1)?}}));
        sockets.push(socket(
            0,
            u32::try_from(ordinal)?,
            &[json!({"node_id":integer(row,"target_node_id",u32::MAX)?,"input_ordinal":integer(row,"target_input_ordinal",0)?})],
        )?);
        if let Some(period) = row.get("available_time") {
            options.last_mut().context("choice option")?["questCondition"] =
                commands::time_period::condition(period, h)?;
        }
        if let Some(predicates) = row.get("available_facts") {
            ensure!(row.get("available_time").is_none(), "choose available_time or available_facts");
            let predicates = predicates.as_array().context("available_facts array")?;
            ensure!(!predicates.is_empty(), "available_facts cannot be empty");
            let conditions = predicates.iter().map(|predicate| {
                let comparison = predicate.get("comparison").and_then(Value::as_str).unwrap_or("Greater");
                ensure!(matches!(comparison,"Equal"|"NotEqual"|"Greater"|"GreaterOrEqual"|"Less"|"LessOrEqual"),"invalid fact comparison");
                let value = predicate.get("value").and_then(Value::as_i64).unwrap_or(0);
                let kind = h.wrap(json!({"$type":"questVarComparison_ConditionType","factName":string(predicate,"fact")?,"comparisonType":comparison,"value":value}));
                Ok(h.wrap(json!({"$type":"questFactsDBCondition","type":kind})))
            }).collect::<Result<Vec<_>>>()?;
            options.last_mut().context("choice option")?["questCondition"] = h.wrap(json!({"$type":"questLogicalCondition","operation":"AND","conditions":conditions}));
        }
    }
    ensure!(!options.is_empty(), "choice must contain options");
    for name in 1..=6 {
        sockets.push(socket(name, 0, &[])?);
    }
    let actor = integer(raw, "actor_id", 0)?;
    let reminder=h.wrap(json!({"$type":"scnChoiceNodeNsActorReminderParams","reminderActor":id("scnActorId",actor),"waitTimeForReminderA":{"$type":"scnSceneTime","stu":10000},"waitTimeForReminderB":{"$type":"scnSceneTime","stu":20000},"waitTimeForReminderC":{"$type":"scnSceneTime","stu":30000},"waitTimeForLooping":{"$type":"scnSceneTime","stu":60000}}));
    let shape = h.wrap(json!({"$type":"scnInteractionShapeParams"}));
    let look_at = h.wrap(json!({"$type":"scnChoiceNodeNsAdaptiveLookAtParams","nearbySlotName":n::cname("Head"),"distantSlotName":n::cname("Chest"),"blendLimit":0.3}));
    let mappin = h.wrap(json!({"$type":"scnChoiceNodeNsMappinParams","locationType":"Nameplate","mappinSettings":tweak("167667357963")}));
    let mut node = h.wrap(json!({"$type":"scnChoiceNode","nodeId":id("scnNodeId",integer(raw,"node_id",u32::MAX)?),"mode":"attachToActor","ataParams":{"$type":"scnChoiceNodeNsAttachToActorParams","actorId":id("scnActorId",actor)},"reminderParams":reminder,"shapeParams":shape,"lookAtParams":look_at,"mappinParams":mappin,"persistentLineEvents":[],"options":options,"outputSockets":sockets}));
    if let Some(position) = raw.get("position") {
        ensure!(
            position.is_object(),
            "interaction position must be scene-local x/y/z"
        );
        let activation = number(raw, "activation_range", 2.)?;
        let indication = number(raw, "indication_range", 3.)?;
        ensure!(
            activation > 0. && indication >= activation,
            "invalid interaction ranges"
        );
        let data = &mut node["Data"];
        data["mode"] = json!("attachToWorld");
        if flag(raw, "disable_auto_look_at", false)? == 1 {
            data["lookAtParams"] = Value::Null;
        }
        data.as_object_mut()
            .context("choice node")?
            .remove("ataParams");
        data["atwParams"] = json!({"$type":"scnChoiceNodeNsAttachToWorldParams",
            "entityPosition":{"$type":"Vector3","X":number(position,"x",0.)?,"Y":number(position,"y",0.)?,"Z":number(position,"z",0.)?},
            "entityOrientation":{"$type":"Quaternion","i":0,"j":0,"k":0,"r":1},
            "customEntityRadius":0,"visualizerStyle":"onScreen"});
        data["shapeParams"]["Data"] = json!({"$type":"scnInteractionShapeParams","preset":"Interaction",
            "customActivationRange":activation,"customIndicationRange":indication,
            "activationYawLimit":360,"activationHeight":3,"activationBaseLength":1,
            "offset":{"$type":"Vector3","X":0,"Y":0,"Z":0},"rotation":{"$type":"Quaternion","i":0,"j":0,"k":0,"r":1}});
        data["mappinParams"]["Data"]["locationType"] = json!("Interaction");
        data["mappinParams"]["Data"]["mappinSettings"] = json!({"$type":"TweakDBID","$storage":"string","$value":"MappinUISettings.SceneDialogObjectSettings"});
    }
    Ok(node)
}

pub(super) fn build(
    spec: &Value,
    _dialogue: &Value,
    text: &text::Text,
    actors: &BTreeMap<String, u32>,
    h: &mut Handles,
) -> Result<Graph> {
    let starts = if spec.get("start_nodes").is_some() {
        list(spec, "start_nodes")?.to_vec()
    } else {
        vec![
            spec.get("start_node")
                .context("scene needs start_nodes")?
                .clone(),
        ]
    };
    ensure!(!starts.is_empty(), "scene needs a start node");
    let mut nodes = Vec::new();
    for row in &starts {
        nodes.push(simple(row, "scnStartNode", "on_start", h)?);
    }
    for row in list(spec, "sections")? {
        nodes.push(timeline::section(spec, row, text, actors, h)?);
    }
    for row in list(spec, "choices")? {
        let aid = integer(row, "actor_id", 0)?;
        if row.get("position").is_none() {
            ensure!(
                actors.values().any(|v| *v == aid),
                "choice targets missing actor {aid}"
            );
        }
        nodes.push(choice(row, text, h)?);
    }
    for row in list(spec, "quest_nodes")? {
        nodes.push(commands::build(row, h)?);
    }
    for row in list(spec, "xor_nodes")? {
        nodes.push(simple(row, "scnXorNode", "on_out", h)?);
    }
    let ends = if spec.get("end_nodes").is_some() {
        list(spec, "end_nodes")?.to_vec()
    } else {
        vec![
            spec.get("end_node")
                .context("scene needs end_nodes")?
                .clone(),
        ]
    };
    for row in &ends {
        nodes.push(simple(row, "scnEndNode", "", h)?);
    }
    let mut by_id = BTreeMap::new();
    for (i, node) in nodes.iter().enumerate() {
        let node_id = integer(&node["Data"]["nodeId"], "id", u32::MAX)?;
        ensure!(
            node_id != u32::MAX && by_id.insert(node_id, i).is_none(),
            "invalid/duplicate scene node ID {node_id}"
        );
    }
    let mut adjacency = BTreeMap::new();
    let mut event_ids = HashSet::new();
    for node in &nodes {
        let source = integer(&node["Data"]["nodeId"], "id", 0)?;
        let mut children = Vec::new();
        for socket in list(&node["Data"], "outputSockets")? {
            for dest in list(socket, "destinations")? {
                let target = integer(&dest["nodeId"], "id", u32::MAX)?;
                let target_node = &nodes[*by_id
                    .get(&target)
                    .with_context(|| format!("scene node {source} targets missing {target}"))?]["Data"];
                let ordinal = integer(&dest["isockStamp"], "ordinal", 0)?;
                let name = integer(&dest["isockStamp"], "name", 0)?;
                if target_node["$type"] == "scnQuestNode" {
                    let mappings = list(target_node, "isockMappings")?;
                    let mapping = mappings
                        .get(ordinal as usize)
                        .context("scene destination input ordinal is out of range")?;
                    ensure!(
                        name == 0 && mapping["$value"] != "CutDestination",
                        "ordinary scene flow must target an executable quest input"
                    );
                } else {
                    ensure!(
                        name == 0 && ordinal == 0,
                        "scene destination must use executable input 0:0"
                    );
                }
                children.push(target);
            }
        }
        for event in list(&node["Data"], "events")? {
            let event = string(&event["Data"]["id"], "id")?;
            ensure!(
                event_ids.insert(event.to_owned()),
                "duplicate scene event ID {event}"
            );
        }
        adjacency.insert(source, children);
    }
    let mut entries = Vec::new();
    let mut entry_names = HashSet::new();
    let entry_specs = if spec.get("entry_points").is_some() {
        list(spec, "entry_points")?.to_vec()
    } else {
        vec![
            spec.get("entry_point")
                .context("scene entry points")?
                .clone(),
        ]
    };
    for row in entry_specs {
        let name = string(&row, "name")?;
        ensure!(
            entry_names.insert(name.to_owned()),
            "duplicate scene entry {name}"
        );
        let node = integer(&row, "node_id", u32::MAX)?;
        ensure!(
            by_id
                .get(&node)
                .is_some_and(|i| nodes[*i]["Data"]["$type"] == "scnStartNode"),
            "entry {name} must target a start node"
        );
        entries.push(
            json!({"$type":"scnEntryPoint","name":n::cname(name),"nodeId":id("scnNodeId",node)}),
        );
    }
    let mut exits = Vec::new();
    let mut exit_names = HashSet::new();
    for row in list(spec, "exit_points")? {
        let name = string(row, "name")?;
        ensure!(exit_names.insert(name), "duplicate scene exit {name}");
        let node = integer(row, "node_id", u32::MAX)?;
        ensure!(
            by_id
                .get(&node)
                .is_some_and(|i| nodes[*i]["Data"]["$type"] == "scnEndNode"),
            "exit {name} must target an end node"
        );
        exits.push(
            json!({"$type":"scnExitPoint","name":n::cname(name),"nodeId":id("scnNodeId",node)}),
        );
    }
    let order = if spec.get("graph_order").is_some() {
        let order = list(spec, "graph_order")?
            .iter()
            .map(|v| Ok(u32::try_from(v.as_u64().context("graph order ID")?)?))
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            order.iter().collect::<HashSet<_>>().len() == nodes.len()
                && order.len() == nodes.len()
                && order.iter().all(|v| by_id.contains_key(v)),
            "graph_order must contain every node exactly once"
        );
        order
    } else {
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([integer(&starts[0], "node_id", 0)?]);
        let mut order = Vec::new();
        while let Some(node) = queue.pop_front() {
            if seen.insert(node) {
                order.push(node);
                queue.extend(adjacency.get(&node).into_iter().flatten());
            }
        }
        for node in &nodes {
            let node = integer(&node["Data"]["nodeId"], "id", 0)?;
            if seen.insert(node) {
                order.push(node);
            }
        }
        order
    };
    Ok(Graph {
        nodes: order.iter().map(|id| nodes[by_id[id]].clone()).collect(),
        entries,
        exits,
    })
}
