use super::*;

pub(super) struct Cast {
    pub actors: Vec<Value>,
    pub players: Vec<Value>,
    pub props: Vec<Value>,
    pub debug: Value,
    pub lookup: BTreeMap<String, u32>,
}

fn animation_ids(v: &Value, key: &str, kind: &str) -> Result<Vec<Value>> {
    list(v, key)?
        .iter()
        .map(|v| {
            Ok(id(
                kind,
                u32::try_from(v.as_u64().context("animation ID")?)?,
            ))
        })
        .collect()
}

pub(super) fn build(spec: &Value) -> Result<Cast> {
    let mut cast = Cast {
        actors: Vec::new(),
        players: Vec::new(),
        props: Vec::new(),
        debug: json!({"$type":"scnDebugSymbols","performersDebugSymbols":[],"sceneEventsDebugSymbols":[],"sceneNodesDebugSymbols":[],"workspotsDebugSymbols":[]}),
        lookup: BTreeMap::new(),
    };
    let mut actor_ids = HashSet::new();
    let player_id = u32::try_from(
        list(spec, "actors")?
            .iter()
            .filter(|row| row["kind"] != "player")
            .count(),
    )?;
    for raw in list(spec, "actors")? {
        let actor = integer(raw, "id", u32::MAX)?;
        ensure!(
            actor != u32::MAX && actor_ids.insert(actor),
            "invalid/duplicate actor ID"
        );
        let key = string(raw, "key")?;
        let name = raw.get("name").and_then(Value::as_str).unwrap_or(key);
        for alias in [key, name] {
            let alias = alias.to_lowercase();
            ensure!(
                cast.lookup.get(&alias).is_none_or(|v| *v == actor),
                "duplicate actor alias {alias}"
            );
            cast.lookup.insert(alias, actor);
        }
        let kind = string(raw, "kind")?;
        ensure!(
            ["player", "community", "context", "spawn", "spawner"].contains(&kind),
            "unknown actor kind {kind}"
        );
        let player = kind == "player";
        let expected_id = if player {
            player_id
        } else {
            u32::try_from(cast.actors.len())?
        };
        ensure!(
            actor == expected_id,
            "scene actor IDs must match their native actor table positions"
        );
        let mut data = json!({"$type":if player{"scnPlayerActorDef"}else{"scnActorDef"},"actorId":id("scnActorId",actor),"acquisitionPlan":match kind{"player"|"context"=>"findInContext","community"=>"community",_=>"spawnDespawn"},"animSets":animation_ids(raw,"rid_anim_sets","scnSRRefId")?,"bodyCinematicAnimSets":[],"cyberwareAnimSets":[],"cyberwareCinematicAnimSets":[],"deformationAnimSets":[],"dynamicAnimSets":[],"facialAnimSets":animation_ids(raw,"rid_facial_anim_sets","scnRidFacialAnimSetSRRefId")?,"facialCinematicAnimSets":[],"lipsyncAnimSet":id("scnLipsyncAnimSetSRRefId",integer(raw,"lipsync",if kind=="spawn"{u32::MAX}else{actor})?),"specAppearance":n::cname(raw.get("appearance").and_then(Value::as_str).unwrap_or("default")),"specCharacterRecordId":tweak("0"),"voicetagId":id("scnVoicetagId",raw.get("voicetag").map(|v|v.as_str().map(str::to_owned).unwrap_or_else(||v.to_string())).unwrap_or_else(||if player{"1103967280742240864"}else{"0"}.into()))});
        let debug_ref = match kind {
            "spawner" => {
                let reference = string(raw, "spawner_ref")?;
                data["actorName"] = json!(name);
                data["acquisitionPlan"] = json!("spawner");
                data["spawnerParams"] = json!({"$type":"scnSpawnerParams", "reference":n::node_ref(reference), "forceMaxVisibility":flag(raw,"force_max_visibility",true)?});
                n::entity(Some(reference), &[])
            }
            "player" => {
                let record = raw
                    .get("record")
                    .and_then(Value::as_str)
                    .unwrap_or("Character.Player_Puppet_Base");
                data["playerName"] = json!(name);
                data["findActorInContextParams"] = json!({"$type":"scnFindEntityInContextParams","contextualName":raw.get("contextual_name").and_then(Value::as_str).unwrap_or("Player"),"specRecordId":tweak(record)});
                data["specCharacterRecordId"] = tweak(record);
                data["specTemplate"] = n::cname("(None)");
                n::entity(Some("#player"), &[])
            }
            "context" => {
                let context = string(raw, "context_actor_name")?;
                data["actorName"] = json!(name);
                data["findActorInContextParams"] = json!({"$type":"scnFindEntityInContextParams","contextualName":"ContextActorName","contextActorName":n::cname(context),"specRecordId":tweak(raw.get("record").and_then(Value::as_str).unwrap_or("Character.Scene_Default_Record"))});
                json!({"$type":"gameEntityReference","type":"SceneActorContextName","sceneActorContextName":n::cname(context)})
            }
            "community" => {
                data["actorName"] = json!(name);
                data["communityParams"] = json!({"$type":"scnCommunityParams","entryName":n::cname(string(raw,"entry")?),"forceMaxVisibility":0,"reference":n::node_ref(string(raw,"community_ref")?)});
                n::entity(
                    Some(string(raw, "community_ref")?),
                    &[string(raw, "entry")?],
                )
            }
            _ => {
                let dynamic = string(raw, "dynamic_name")?;
                data["actorName"] = json!(name);
                data["spawnDespawnParams"] = spawn(raw, dynamic)?;
                dynamic_entity(dynamic)
            }
        };
        cast.debug["performersDebugSymbols"].as_array_mut().context("debug symbols")?.push(json!({"$type":"scnPerformerSymbol","editorPerformerId":"0","entityRef":debug_ref,"performerId":performer(actor)?}));
        if player {
            cast.players.push(data)
        } else {
            cast.actors.push(data)
        }
    }
    let mut prop_ids = HashSet::new();
    for raw in list(spec, "props")? {
        let prop = integer(raw, "id", u32::MAX)?;
        ensure!(
            prop != u32::MAX && prop_ids.insert(prop),
            "invalid/duplicate prop ID"
        );
        let mut data = json!({"$type":"scnPropDef","propId":id("scnPropId",prop),"propName":raw.get("name").and_then(Value::as_str).unwrap_or(string(raw,"key")?),"animSets":[],"cinematicAnimSets":[],"dynamicAnimSets":[],"specPropRecordId":tweak("0")});
        let entity_ref = if raw["kind"] == "spawn" {
            let dynamic = string(raw, "dynamic_name")?;
            data["entityAcquisitionPlan"] = json!("spawnDespawn");
            data["spawnDespawnParams"] = spawn(raw, dynamic)?;
            data["specPropRecordId"] = tweak(string(raw, "record")?);
            dynamic_entity(dynamic)
        } else {
            let reference = string(raw, "node_ref")?;
            data["entityAcquisitionPlan"] = json!("findInNode");
            data["findEntityInNodeParams"] = json!({"$type":"scnFindEntityInNodeParams","forceMaxVisibility":0,"nodeRef":n::node_ref(reference)});
            n::entity(Some(reference), &[])
        };
        cast.props.push(data);
        let pid = prop
            .checked_mul(256)
            .and_then(|v| v.checked_add(2))
            .context("prop performer overflow")?;
        cast.debug["performersDebugSymbols"].as_array_mut().context("prop debug symbols")?.push(json!({"$type":"scnPerformerSymbol","editorPerformerId":"0","entityRef":entity_ref,"performerId":id("scnPerformerId",pid)}));
    }
    Ok(cast)
}

fn spawn(v: &Value, name: &str) -> Result<Value> {
    Ok(
        json!({"$type":"scnSpawnDespawnEntityParams","alwaysSpawned":flag(v,"always_spawned",false)?,"appearance":n::cname(v.get("appearance").and_then(Value::as_str).unwrap_or("default")),"dynamicEntityUniqueName":n::cname(name),"findInWorld":0,"forceMaxVisibility":flag(v,"force_max_visibility",false)?,"isEnabled":1,"itemOwnerId":id("scnPerformerId",INVALID_PERFORMER_ID),"keepAlive":flag(v,"keep_alive",false)?,"prefetchAppearance":flag(v,"prefetch_appearance",false)?,"spawnMarker":n::cname("None"),"spawnMarkerNodeRef":n::node_ref(string(v,"spawn_marker")?),"spawnMarkerType":v.get("spawn_marker_type").and_then(Value::as_str).unwrap_or("Global"),"spawnOffset":{"$type":"Transform","orientation":{"$type":"Quaternion","i":0,"j":0,"k":0,"r":1},"position":{"$type":"Vector4","W":0,"X":0,"Y":0,"Z":0}},"spawnOnStart":flag(v,"spawn_on_start",true)?,"specRecordId":tweak(string(v,"record")?),"validateSpawnPostion":flag(v,"validate_spawn_position",true)?}),
    )
}

pub(super) fn resources(spec: &Value) -> Result<Value> {
    let path=spec.get("lipsync_animset").and_then(Value::as_str).unwrap_or("base\\animations\\facial\\generic\\interactive_scene\\generic_facial_lipsync_gestures.anims");
    depot_path(&path.replace('/', "\\"))?;
    let mut count = 0;
    for actor in list(spec, "actors")? {
        let slot = integer(
            actor,
            "lipsync",
            if actor["kind"] == "spawn" {
                u32::MAX
            } else {
                integer(actor, "id", 0)?
            },
        )?;
        if slot != u32::MAX {
            ensure!(slot < 4096, "lipsync slot exceeds scene limit");
            count = count.max(slot + 1);
        }
    }
    let mut refs = json!({"$type":"scnSRRefCollection","cinematicAnimNames":[],"cinematicAnimSets":[],"dynamicAnimNames":[],"dynamicAnimSets":[],"gameplayAnimNames":[],"gameplayAnimSets":[],"lipsyncAnimSets":(0..count).map(|_|json!({"$type":"scnLipsyncAnimSetSRRef","asyncRefLipsyncAnimSet":resource(path,true),"lipsyncAnimSet":resource("",false)})).collect::<Vec<_>>(),"ridAnimationContainers":[],"ridAnimations":[],"ridAnimSets":[],"ridCameraAnimations":[],"ridCyberwareAnimSets":[],"ridDeformationAnimSets":[],"ridFacialAnimSets":[]});
    for (key, out, kind) in [
        ("rid_animations", "ridAnimations", "scnRidAnimationSRRef"),
        (
            "rid_camera_animations",
            "ridCameraAnimations",
            "scnRidCameraAnimationSRRef",
        ),
    ] {
        let mut rows = Vec::new();
        for row in list(spec, key)? {
            rows.push(json!({"$type":kind,"animationSN":{"$type":"scnRidSerialNumber","serialNumber":integer(row,"serial_number",0)?},"resourceId":id("scnRidResourceId",integer(row,"resource_id",0)?)}));
        }
        refs[out] = json!(rows);
    }
    for (key, out) in [
        ("rid_anim_sets", "ridAnimSets"),
        ("rid_facial_anim_sets", "ridFacialAnimSets"),
    ] {
        refs[out]=json!(list(spec,key)?.iter().map(|v|Ok(json!({"$type":"scnRidAnimSetSRRef","animations":animation_ids(v,"animations","scnSRRefId")?}))).collect::<Result<Vec<_>>>()?);
    }
    Ok(refs)
}
