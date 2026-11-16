use super::{
    Handles,
    geometry::{self, Position},
};
use crate::nodes as n;
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

pub(super) fn common(spec: &Value, reference: &str, kind: &str) -> Value {
    let mut node = json!({"$type":kind,"debugName":n::cname(spec.get("debug_name").and_then(Value::as_str).unwrap_or(reference.trim_start_matches('#'))),"isHostOnly":0,"isVisibleInGame":1,"sourcePrefabHash":spec.get("source_prefab_hash").cloned().unwrap_or(json!("0"))});
    for (source, target) in [("tag", "tag"), ("tag_ext", "tagExt")] {
        if let Some(value) = spec.get(source) {
            node[target] = value.clone();
        }
    }
    node
}
pub(super) fn resource(path: &str) -> Value {
    json!({"DepotPath":{"$type":"ResourcePath","$storage":"string","$value":path.replace('/',"\\")},"Flags":"Soft"})
}
pub(super) fn zero_ref() -> Value {
    json!({"$type":"NodeRef","$storage":"uint64","$value":"0"})
}
pub(super) fn flag(v: &Value, key: &str, default: bool) -> Result<bool> {
    v.get(key).map_or(Ok(default), |v| {
        v.as_bool()
            .or_else(|| v.as_u64().filter(|v| *v <= 1).map(|v| v == 1))
            .with_context(|| format!("{key} must be boolean"))
    })
}
pub(super) fn list<'a>(v: &'a Value, key: &str) -> Result<&'a [Value]> {
    v.get(key).map_or(Ok(&[][..]), |v| {
        v.as_array()
            .map(Vec::as_slice)
            .with_context(|| format!("{key} must be array"))
    })
}
fn notifier(spec: &Value, h: &mut Handles) -> Result<Value> {
    let kind = spec
        .as_str()
        .or_else(|| spec.get("type").and_then(Value::as_str))
        .unwrap_or("quest")
        .to_ascii_lowercase();
    let data = match kind.as_str() {
        "quest" => {
            json!({"$type":"questTriggerNotifier_Quest","excludeChannels":0,"includeChannels":"TC_Default","isEnabled":1})
        }
        "interior" => {
            json!({"$type":"worldInteriorAreaNotifier","excludeChannels":0,"gameRestrictionIDs":[],"includeChannels":"TC_Player","isEnabled":1,"setTier2":0,"treatAsInterior":1})
        }
        "prevention_deescalation" | "prevention" => {
            json!({"$type":"worldQuestPreventionNotifier","activation":"Always","excludeChannels":0,"includeChannels":"TC_Player","isEnabled":1,"type":"Deescalation"})
        }
        _ => {
            if let Some(raw) = spec.get("raw") {
                ensure!(
                    raw["$type"].is_string(),
                    "custom notifier needs a native type"
                );
                raw.clone()
            } else {
                bail!("unsupported trigger notifier {kind}")
            }
        }
    };
    Ok(h.wrap(data))
}
pub(super) fn marker(spec: &Value, reference: &str, h: &mut Handles) -> Result<Value> {
    let scene = flag(spec, "scene_marker", false)?;
    let marker = spec.get("marker_type").and_then(Value::as_str);
    ensure!(
        !scene || marker.is_none(),
        "scene marker cannot combine another marker type"
    );
    let entries = list(spec, "scene_marker_entries")?;
    ensure!(
        scene || entries.is_empty(),
        "scene_marker_entries requires a scene marker"
    );
    let mut data = common(spec, reference, "worldStaticMarkerNode");
    let content = if scene {
        Some(json!({"$type":"scnSceneMarker","markers":entries,"workspotMarkers":[]}))
    } else {
        match marker {
            None => None,
            Some("null") => Some(json!({"$type":"worldNullMarker"})),
            Some("spawn_point") => Some(json!({"$type":"worldSpawnPointMarker","type":0})),
            Some(kind) => bail!("unknown marker type {kind}"),
        }
    };
    if let Some(content) = content {
        data["data"] = h.wrap(content);
        data["isEnabled"] = json!(1);
        data["tags"] = json!({"$type":"redTagList","tags":[]});
    }
    Ok(h.wrap(data))
}
pub(super) fn trigger(
    spec: &Value,
    reference: &str,
    h: &mut Handles,
    crowd: bool,
) -> Result<Value> {
    let mut data = common(
        spec,
        reference,
        if crowd {
            "worldCrowdNullAreaNode"
        } else {
            "worldTriggerAreaNode"
        },
    );
    data["outline"] = h.wrap(geometry::outline(
        spec.get("outline").unwrap_or(&json!({})),
    )?);
    if crowd {
        data["IsForBlockade"] = json!(1);
        data["permanentlyEnabledByDefault"] = json!(0);
    } else {
        data["color"] = json!({"$type":"Color","Alpha":0,"Blue":0,"Green":0,"Red":0});
        let raw = spec.get("notifiers").cloned().unwrap_or(json!(["quest"]));
        let notifiers = if let Some(values) = raw.as_array() {
            values
                .iter()
                .map(|v| notifier(v, h))
                .collect::<Result<Vec<_>>>()?
        } else {
            vec![notifier(&raw, h)?]
        };
        data["notifiers"] = json!(notifiers);
    }
    Ok(h.wrap(data))
}

/// Generates a streamed scene area with direct or group-based scene selection.
pub(super) fn conversation(spec: &Value, reference: &str, h: &mut Handles) -> Result<Value> {
    let scene = spec["scene"].as_str().context("conversation scene path")?;
    ensure!(
        scene.ends_with(".scene"),
        "conversation requires a .scene resource"
    );
    crate::project::depot_path(scene)?;
    let mut data = common(spec, reference, "worldInterestingConversationsAreaNode");
    data["outline"] = h.wrap(geometry::outline(
        spec.get("outline").unwrap_or(&json!({})),
    )?);
    data["notifiers"] = json!([h.wrap(json!({"$type":"worldInterestingConversationsAreaNotifier","excludeChannels":0,"includeChannels":"TC_Player","isEnabled":1}))]);
    let binding = spec
        .get("binding")
        .and_then(Value::as_str)
        .unwrap_or("direct");
    if binding == "direct" {
        data["conversationGroups"] = json!([]);
        data["conversationResources"] = json!([]);
        data["conversations"] = json!([h.wrap(json!({"$type":"worldConversationData","sceneFilename":resource(scene),"condition":null,"interruptionOperations":[],"ignoreGlobalLimit":flag(spec,"ignore_global_limit",false)?,"ignoreLocalLimit":flag(spec,"ignore_local_limit",false)?}))]);
    } else {
        ensure!(binding == "group", "unknown conversation binding {binding}");
        let group = spec["group_path"]
            .as_str()
            .context("conversation group path")?;
        let mut reference = resource(group);
        reference["Flags"] = json!("Default");
        let ignore_global = flag(spec, "ignore_global_limit", false)?;
        let ignore_local = flag(spec, "ignore_local_limit", false)?;
        if ignore_global || ignore_local {
            data["conversationGroups"] = json!([]);
            data["conversationResources"] = json!([h.wrap(json!({"$type":"worldConversationGroupData","conversationGroup":reference,"interruptionOperations":[],"ignoreGlobalLimit":ignore_global,"ignoreLocalLimit":ignore_local}))]);
        } else {
            data["conversationGroups"] = json!([reference]);
            data["conversationResources"] = json!([]);
        }
        data["conversations"] = json!([]);
    }
    data["workspots"] = json!(
        list(spec, "workspots")?
            .iter()
            .map(|v| v
                .as_str()
                .map(n::node_ref)
                .context("conversation workspot NodeRef"))
            .collect::<Result<Vec<_>>>()?
    );
    data["savingStrategy"] = spec
        .get("saving_strategy")
        .cloned()
        .unwrap_or(json!("Default"));
    Ok(h.wrap(data))
}
pub(super) fn entity(spec: &Value, reference: &str, h: &mut Handles) -> Result<Value> {
    if let Some(outline) = spec.get("vehicle_forbidden_outline") {
        let mut data = common(spec, reference, "worldVehicleForbiddenAreaNode");
        data["outline"] = h.wrap(geometry::outline(outline)?);
        let inner = h.wrap(geometry::outline(
            spec.get("vehicle_forbidden_inner_outline")
                .unwrap_or(outline),
        )?);
        let notify=h.wrap(json!({"$type":"worldVehicleForbiddenAreaNotifier","innerAreaBoundToOuterArea":flag(spec,"vehicle_forbidden_inner_bound",true)?,"innerAreaOutline":inner,"areaSpeedLimit":geometry::scalar(spec,"vehicle_forbidden_speed_limit",0.)?,"enableNullArea":flag(spec,"vehicle_forbidden_null_area",false)?}));
        data["notifiers"] = json!([notify]);
        return Ok(h.wrap(data));
    }
    if let Some(event) = spec.get("ambient_event").and_then(Value::as_str) {
        let mut data = common(spec, reference, "worldAmbientAreaNode");
        data["outline"] = h.wrap(geometry::outline(&spec["ambient_outline"])?);
        let settings=h.wrap(json!({"$type":"audioAmbientAreaSettings","EventsOnActive":[{"$type":"audioAudEventStruct","event":n::cname(event)}],"outerDistance":20.,"verticalOuterDistance":20.}));
        data["notifiers"]=json!([h.wrap(json!({"$type":"audioAmbientAreaNotifier","includeChannels":"TC_SoundAmbientArea","Settings":settings}))]);
        return Ok(h.wrap(data));
    }
    if let Some(record) = spec.get("population_record") {
        let mut data = common(spec, reference, "worldPopulationSpawnerNode");
        let record = record
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| record.to_string());
        data["objectRecordId"] = if record.parse::<u64>().is_ok() {
            json!({"$type":"TweakDBID","$storage":"uint64","$value":record})
        } else {
            n::tweak_id(&record)
        };
        data["isVehicle"] = json!(flag(spec, "is_vehicle", false)?);
        for (source, target) in [
            ("spawn_on_start", "spawnOnStart"),
            ("prefetch_appearance", "prefetchAppearance"),
        ] {
            if spec.get(source).is_some() {
                data[target] = json!(flag(spec, source, false)?);
            }
        }
        for (source, target) in [
            ("always_spawned", "alwaysSpawned"),
            ("spawn_in_view", "spawnInView"),
        ] {
            if let Some(value) = spec.get(source) {
                let name = value
                    .as_str()
                    .with_context(|| format!("{source} must be a native enum name"))?;
                data[target] = json!(match name {
                    "true_" => "true",
                    "false_" => "false",
                    value => value,
                });
            }
        }
        if let Some(appearance) = spec.get("appearance").and_then(Value::as_str) {
            data["appearanceName"] = n::cname(appearance);
        }
        return Ok(h.wrap(data));
    }
    if let Some(path) = spec.get("mesh").and_then(Value::as_str) {
        crate::project::depot_path(&path.replace('/', "\\"))?;
        ensure!(
            path.ends_with(".mesh"),
            "placed mesh requires a .mesh resource"
        );
        let mut data = common(spec, reference, "worldStaticMeshNode");
        data["mesh"] = resource(path);
        data["meshAppearance"] = n::cname(
            spec.get("appearance")
                .and_then(Value::as_str)
                .unwrap_or("default"),
        );
        for key in [
            "castShadows",
            "castLocalShadows",
            "castRayTracedGlobalShadows",
            "castRayTracedLocalShadows",
        ] {
            if let Some(mode) = spec.get("shadow_casting") {
                data[key] = mode.clone();
            }
        }
        data["forceAutoHideDistance"] = json!(geometry::scalar(spec, "auto_hide_distance", 0.)?);
        return Ok(h.wrap(data));
    }
    let path = spec
        .get("entity")
        .or_else(|| spec.get("entity_template"))
        .and_then(Value::as_str)
        .context("placed entity requires an .ent resource identity")?;
    crate::project::depot_path(&path.replace('/', "\\"))?;
    let mut data = common(spec, reference, "worldEntityNode");
    data["appearanceName"] = n::cname(
        spec.get("appearance")
            .and_then(Value::as_str)
            .unwrap_or("default"),
    );
    data["entityLod"] = spec.get("entity_lod").cloned().unwrap_or(json!(0));
    data["entityTemplate"] = resource(path);
    data["instanceData"] = Value::Null;
    data["ioPriority"] = spec
        .get("io_priority")
        .cloned()
        .unwrap_or(json!("Immediate"));
    Ok(h.wrap(data))
}
pub(super) fn spot(spec: &Value, reference: &str, h: &mut Handles) -> Result<Value> {
    let vehicle = flag(spec, "vehicle_spawn", false)?;
    let mut data = common(spec, reference, "worldAISpotNode");
    data.as_object_mut().context("spot object")?.extend(json!({"crowdBlacklist":{"$type":"redTagList","tags":[]},"crowdWhitelist":{"$type":"redTagList","tags":[]},"disableBumps":0,"isWorkspotInfinite":u8::from(flag(spec,"is_workspot_infinite",true)?),"isWorkspotStatic":u8::from(flag(spec,"is_workspot_static",false)?),"lookAtTarget":spec.get("look_at_target").and_then(Value::as_str).map_or_else(zero_ref,n::node_ref),"markings":list(spec,"markings")?.iter().map(|v|Ok(n::cname(v.as_str().context("spot marking")?))).collect::<Result<Vec<_>>>()?,"spotDef":null,"useCrowdBlacklist":u8::from(flag(spec,"use_crowd_blacklist",!vehicle)?),"useCrowdWhitelist":u8::from(flag(spec,"use_crowd_whitelist",!vehicle)?)}).as_object().context("spot properties")?.clone());
    data["spot"] = if vehicle {
        Value::Null
    } else {
        let path = spec
            .get("workspot")
            .and_then(Value::as_str)
            .unwrap_or("base\\workspots\\common\\wall\\generic__stand_wall_lean_back_cigarette__smoke__01.workspot");
        h.wrap(json!({"$type":"AIActionSpot","ActorBodytypeE3":"Undefined","clippingSpaceOrientation":geometry::scalar(spec,"clipping_space_orientation",180.)?,"clippingSpaceRange":geometry::scalar(spec,"clipping_space_range",120.)?,"enabledWhenMasterOccupied":0,"masterNodeRef":zero_ref(),"resource":resource(path),"snapToGround":u8::from(flag(spec,"snap_to_ground",false)?),"useClippingSpace":u8::from(flag(spec,"use_clipping_space",false)?)}))
    };
    Ok(h.wrap(data))
}
pub(super) fn device(spec: &Value, reference: &str, h: &mut Handles) -> Result<Value> {
    if spec["kind"] == "computer" {
        return super::computer::device(spec, reference, h);
    }
    ensure!(
        spec.get("kind")
            .and_then(Value::as_str)
            .unwrap_or("access_point")
            == "access_point",
        "only native access-point device generation is defined"
    );
    let mut data = common(spec, reference, "worldDeviceNode");
    ensure!(
        spec.get("personal_link").is_none() || flag(spec, "instance_data", true)?,
        "personal-link settings require generated instance data"
    );
    let instance = if flag(spec, "instance_data", true)? {
        let buffer = spec.get("buffer_id").and_then(Value::as_u64).unwrap_or(1);
        ensure!(buffer > 0, "device buffer cannot reuse sector buffer zero");
        let state = spec.get("initial_state").cloned().unwrap_or(json!("OFF"));
        let mut chunks = vec![
            json!({"$type":"AccessPoint","contentScale":n::tweak_id(spec.get("content_scale").and_then(Value::as_str).unwrap_or("DeviceContentAssignment.Autoscaling")),"controllerTypeName":n::cname("AccessPointController"),"deviceState":state,"displayDescription":{"unk1":"0","value":""},"displayName":{"unk1":"0","value":""},"networkName":spec.get("network_name").cloned().unwrap_or(json!("Local Network 1")),"tags":{"$type":"redTagList","tags":[]}}),
        ];
        let mut cruids = json!({"0":"0"});
        if let Some(link) = spec.get("personal_link") {
            let fields = link
                .as_object()
                .context("personal_link requires explicit controller binding")?;
            ensure!(
                fields
                    .keys()
                    .all(|key| ["controller_component_id", "interaction_record"]
                        .contains(&key.as_str())),
                "unknown personal-link field"
            );
            let component = link["controller_component_id"]
                .as_u64()
                .or_else(|| {
                    link["controller_component_id"]
                        .as_str()
                        .and_then(|v| v.parse::<u64>().ok())
                })
                .filter(|v| *v != 0)
                .context("personal_link needs the selected entity's controller component ID")?;
            let interaction = link
                .get("interaction_record")
                .map_or(Ok("Interactions.ConnectPersonalLink"), |v| {
                    v.as_str().context("personal-link interaction record")
                })?;
            ensure!(
                interaction.starts_with("Interactions.")
                    && interaction.len() > "Interactions.".len(),
                "personal-link interaction must be an Interactions record"
            );
            let check=h.wrap(json!({"$type":"HackingSkillCheck","difficulty":"NONE","isActive":0,"isDynamic":0,"skillToCheck":"Hacking"}));
            let checks = h.wrap(json!({"$type":"HackingContainer","hackingCheck":check}));
            let persistent=h.wrap(json!({"$type":"AccessPointControllerPS","accessPointSkillChecks":checks,"deviceState":state,"hasPersonalLinkSlot":1,"isInteractive":1,"personalLinkCustomInteraction":n::tweak_id(interaction),"personalLinkForced":1,"personalLinkStatus":"NOT_CONNECTED","shouldSkipNetrunnerMinigame":1}));
            chunks.push(json!({"$type":"AccessPointController","id":component.to_string(),"isReplicable":1,"name":n::cname("controller"),"persistentState":persistent}));
            cruids["1"] = json!(component.to_string());
        }
        h.wrap(json!({"$type":"entEntityInstanceData","buffer":{"BufferId":buffer.to_string(),"Flags":4063232,"Type":"WolvenKit.RED4.Archive.Buffer.RedPackage, WolvenKit.RED4, Version=8.17.4.0, Culture=neutral, PublicKeyToken=null","Data":{"Version":4,"Sections":7,"CruidIndex":0,"CruidDict":cruids,"Chunks":chunks}}}))
    } else {
        Value::Null
    };
    data.as_object_mut().context("device")?.extend(json!({"alphaHackStreamingDistanceOverride":geometry::scalar(spec,"alpha_hack_streaming_distance_override",0.)?,"appearanceName":n::cname(spec.get("appearance").and_then(Value::as_str).unwrap_or("access_point_access_point_socket_f_neomil")),"deviceClassName":n::cname(spec.get("device_class_name").and_then(Value::as_str).unwrap_or("None")),"deviceConnections":[],"entityLod":spec.get("entity_lod").cloned().unwrap_or(json!(0)),"entityTemplate":resource(spec.get("entity").or_else(||spec.get("entity_template")).and_then(Value::as_str).unwrap_or("base\\gameplay\\devices\\masters\\access_points\\accesspoint.ent")),"instanceData":instance,"ioPriority":spec.get("io_priority").cloned().unwrap_or(json!("Immediate"))}).as_object().context("device fields")?.clone());
    Ok(h.wrap(data))
}
pub(super) fn registry(entries: &[(String, String, Position)]) -> Value {
    json!({"$type":"gameDeviceResource","data":{"HandleId":"0","Data":{"$type":"gameDeviceResourceData","unk1":entries.iter().map(|(reference,class,position)|json!({"$type":"gameDeviceResourceData_Cls1","children":[],"className":n::cname(class),"hash":geometry::hash(reference).to_string(),"nodePosition":position.vector(),"parents":[]})).collect::<Vec<_>>(),"version":2}}})
}
