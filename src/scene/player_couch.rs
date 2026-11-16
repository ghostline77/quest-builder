//! SQ028's player couch transition, rebound to an authored seat and idle command.
use super::*;

fn import(value: &Value, h: &mut Handles) -> Result<Value> {
    match value {
        Value::Object(object) if object.contains_key("Data") => {
            let data = import(&object["Data"], h)?;
            Ok(h.wrap(data))
        }
        Value::Object(object) => Ok(Value::Object(
            object
                .iter()
                .map(|(key, value)| Ok((key.clone(), import(value, h)?)))
                .collect::<Result<_>>()?,
        )),
        Value::Array(array) => Ok(Value::Array(
            array
                .iter()
                .map(|value| import(value, h))
                .collect::<Result<_>>()?,
        )),
        _ => Ok(value.clone()),
    }
}

pub(super) fn apply(root: &mut Value, spec: &Value, h: &mut Handles) -> Result<()> {
    let rows = list(spec, "player_couch_sits")?;
    if rows.is_empty() {
        return Ok(());
    }
    let template: Value = serde_json::from_str(include_str!("../../data/player-couch-sit.json"))?;
    let mut sections = HashSet::new();
    for row in rows {
        for key in row.as_object().context("player couch sit")?.keys() {
            ensure!(
                ["key", "section", "actor", "workspot", "hold"].contains(&key.as_str()),
                "unsupported player couch sit field {key}"
            );
        }
        let key = string(row, "key")?;
        let actor = list(spec, "actors")?
            .iter()
            .find(|v| v["key"] == row["actor"])
            .context("player couch actor missing")?;
        ensure!(
            actor["kind"] == "player",
            "couch transition requires a player actor"
        );
        let aid = integer(actor, "id", u32::MAX)?;
        let seat = list(spec, "workspots")?
            .iter()
            .find(|v| v["key"] == row["workspot"])
            .context("player couch workspot missing")?;
        ensure!(
            seat["resource"]
                == "base\\workspots\\player\\couch\\player__sit_couch__talk__01.workspot",
            "player couch transition requires its matching native workspot"
        );
        ensure!(
            seat.get("offset").is_none(),
            "player couch seat origin must be its final transform"
        );
        let wid = integer(seat, "id", u32::MAX)?;
        let hold = list(spec, "quest_nodes")?
            .iter()
            .find(|v| v["key"] == row["hold"])
            .context("player couch hold command missing")?;
        ensure!(
            hold["kind"] == "use_scene_workspot"
                && hold["is_player"] == true
                && hold["workspot_id"] == wid
                && hold["entry_id"] == 2
                && hold["jump_to_entry"] == true
                && hold["enable_idle_mode"] == true
                && hold["finish_animation"] == false,
            "player couch hold must target this seat's entry 2 in idle mode"
        );
        let section = list(spec, "sections")?
            .iter()
            .find(|v| v["key"] == row["section"])
            .context("player couch section missing")?;
        let sid = integer(section, "node_id", u32::MAX)?;
        ensure!(
            sections.insert(sid),
            "duplicate player couch transition section"
        );
        ensure!(
            list(section, "lines")?.is_empty() && list(section, "on_end")?.is_empty(),
            "player couch transition section must route through its hold command"
        );
        let mut event = import(&template["event"], h)?;
        event["id"] = event_id(&[string(spec, "name")?, key, "player_couch"]);
        event["performer"] = performer(aid)?;
        event["rootMotionData"]["originMarker"] = json!({"$type":"scnMarker","type":"Global",
            "nodeRef":n::node_ref(string(seat,"origin")?),"entityRef":n::entity(None,&[]),
            "slotName":n::cname("None"),"localMarkerId":n::cname("None"),"isMounted":1});
        event["poseBlendOutWorkspot"]["Data"]["workspotId"] = id("scnSceneWorkspotInstanceId", wid);
        let wrapped = h.wrap(event);
        let signal = h.wrap(json!({"$type":"scneventsSocket",
            "id":event_id(&[string(spec,"name")?,key,"hold"]), "startTime":3066,
            "osockStamp":{"$type":"scnOutputSocketStamp","name":2,"ordinal":0}}));
        let node = root["sceneGraph"]["Data"]["graph"]
            .as_array_mut()
            .context("scene graph")?
            .iter_mut()
            .find(|v| v["Data"]["nodeId"]["id"] == sid)
            .context("player couch section absent")?;
        ensure!(
            node["Data"]["events"]
                .as_array()
                .context("section events")?
                .is_empty(),
            "player couch section must not contain other events"
        );
        node["Data"]["events"] = json!([wrapped, signal]);
        node["Data"]["sectionDuration"]["stu"] = json!(3067);
        node["Data"]["outputSockets"]
            .as_array_mut()
            .context("section sockets")?
            .push(graph::socket(
                2,
                0,
                &[json!({"node_id":integer(hold,"node_id",u32::MAX)?,"input_ordinal":1})],
            )?);
        for binding in list(&template, "bindings")? {
            let refs = &mut root["resouresReferences"];
            let index = list(refs, "cinematicAnimSets")?.len();
            refs["cinematicAnimSets"]
                .as_array_mut()
                .context("animation sets")?
                .push(binding["set"].clone());
            refs["cinematicAnimNames"]
                .as_array_mut()
                .context("animation names")?
                .push(binding["names"].clone());
            let player = root["playerActors"]
                .as_array_mut()
                .context("player actors")?
                .iter_mut()
                .find(|v| v["actorId"]["id"] == aid)
                .context("player binding missing")?;
            player["bodyCinematicAnimSets"]
                .as_array_mut()
                .context("player body sets")?
                .push(id("scnCinematicAnimSetSRRefId", index));
        }
    }
    Ok(())
}
