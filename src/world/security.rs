//! Native security-system and security-area controllers and their device network.
use super::{
    Builder, Handles,
    geometry::{self, Anchor, Position},
    nodes,
};
use crate::nodes as n;
use anyhow::{Context, Result};
use serde_json::{Value, json};

fn instance(h: &mut Handles, buffer: u64, chunks: Vec<Value>, ids: Value) -> Value {
    h.wrap(json!({"$type":"entEntityInstanceData","buffer":{"BufferId":buffer.to_string(),"Flags":4063232,"Type":"WolvenKit.RED4.Archive.Buffer.RedPackage, WolvenKit.RED4, Version=8.17.4.0, Culture=neutral, PublicKeyToken=null","Data":{"Version":4,"Sections":7,"CruidIndex":-1,"CruidDict":ids,"Chunks":chunks}}}))
}
fn connection(class: &str, references: &[String]) -> Value {
    json!({"$type":"worldDeviceConnections","deviceClassName":n::cname(class),"nodeRefs":references.iter().map(|r|n::node_ref(r)).collect::<Vec<_>>()})
}
pub(super) fn add(builder: &mut Builder, world: &Value, buffer: u64) -> Result<()> {
    let Some(spec) = world.get("security") else {
        return Ok(());
    };
    let system = spec["system_ref"]
        .as_str()
        .context("security system reference")?;
    let area = spec["area_ref"]
        .as_str()
        .context("security area reference")?;
    let position = Position::read(&spec["position"])?;
    let anchor = Anchor {
        position,
        yaw: 0.,
        index: None,
    };
    let communities = if let Some(refs) = spec.get("communities") {
        refs.as_array()
            .context("security communities")?
            .iter()
            .map(|v| {
                v.as_str()
                    .context("security community")
                    .map(|v| geometry::full_ref(&builder.prefab, v))
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        world
            .get("community")
            .into_iter()
            .chain(
                world
                    .get("communities")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten(),
            )
            .map(|c| {
                c["ref"]
                    .as_str()
                    .context("security community reference")
                    .map(|v| geometry::full_ref(&builder.prefab, v))
            })
            .collect::<Result<Vec<_>>>()?
    };
    let h = &mut builder.quest.handles;
    let system_id = "1425553149548834824";
    let state=h.wrap(json!({"$type":"SecuritySystemControllerPS","deviceState":"ON","securitySystemState":"SAFE","attitudeChangeMode":"PERSISTENTLY","attitudeGroup":n::tweak_id("Attitudes.Group_Neutral"),"allowSecuritySystemToDisableItself":1,"hideAreasOnMinimap":1,"isInteractive":1,"fullDepth":1,"maxGlobalWarningsCount":4,"updateInterval":1,"tweakDBRecord":n::tweak_id("Devices.SecuritySystem")}));
    let package = instance(
        h,
        buffer,
        vec![
            json!({"$type":"SecuritySystemController","id":system_id,"isReplicable":0,"name":n::cname("Component"),"persistentState":state}),
        ],
        json!({"0":system_id}),
    );
    let mut node = nodes::common(spec, system, "worldDeviceNode");
    node.as_object_mut().context("security node")?.extend(json!({"entityTemplate":nodes::resource("base\\gameplay\\devices\\security_systems\\security_system.ent"),"appearanceName":n::cname("default"),"deviceClassName":n::cname("SecuritySystemControllerPS"),"deviceConnections":[connection("SecurityAreaControllerPS",&[geometry::full_ref(&builder.prefab,area)]),connection("CommunityProxyPS",&communities)],"instanceData":package,"entityLod":0,"ioPriority":"Immediate","alphaHackStreamingDistanceOverride":0}).as_object().context("security properties")?.clone());
    let system_node = h.wrap(node);
    builder.add(
        system,
        system_node,
        anchor,
        spec.get("node_data"),
        super::Placement::Quest,
    )?;
    let h = &mut builder.quest.handles;
    let trigger_id = "1508365491801698304";
    let controller_id = "1395159393761816576";
    let outline = h.wrap(geometry::package_outline(&spec["outline"])?);
    let transform = json!({"$type":"WorldTransform","Orientation":{"$type":"Quaternion","i":0,"j":0,"k":0,"r":1},"Position":{"$type":"WorldPosition","x":{"$type":"FixedPoint","Bits":0},"y":{"$type":"FixedPoint","Bits":0},"z":{"$type":"FixedPoint","Bits":0}}});
    let trigger = json!({"$type":"gameStaticTriggerAreaComponent","id":trigger_id,"name":n::cname("Component"),"isReplicable":0,"includeMask":1,"excludeMask":0,"isEnabled":1,"outline":outline,"color":{"$type":"Color","Alpha":0,"Blue":0,"Green":0,"Red":0},"localTransform":transform});
    let state=h.wrap(json!({"$type":"SecurityAreaControllerPS","securityAreaType":spec["type"],"deviceState":"ON","isInteractive":1,"fullDepth":1,"eventsFilters":{"$type":"EventsFilters","incomingEventsFilter":"ALLOW_ALL","outgoingEventsFilter":"ALLOW_ALL"}}));
    let controller = json!({"$type":"SecurityAreaController","id":controller_id,"isReplicable":0,"name":n::cname("Component"),"persistentState":state});
    let package = instance(
        h,
        buffer + 1,
        vec![trigger, controller],
        json!({"0":trigger_id,"1":controller_id}),
    );
    let mut node = nodes::common(spec, area, "worldDeviceNode");
    node.as_object_mut().context("security area node")?.extend(json!({"entityTemplate":nodes::resource("base\\gameplay\\devices\\security_systems\\security_area\\security_area_1.ent"),"appearanceName":n::cname("default"),"deviceClassName":n::cname("SecurityAreaControllerPS"),"deviceConnections":[connection("CommunityProxyPS",&communities)],"instanceData":package,"entityLod":0,"ioPriority":"Immediate","alphaHackStreamingDistanceOverride":0}).as_object().context("security area properties")?.clone());
    let area_node = h.wrap(node);
    builder.add(
        area,
        area_node,
        anchor,
        spec.get("node_data"),
        super::Placement::Quest,
    )?;
    for (reference, class) in [
        (system, "SecuritySystemControllerPS"),
        (area, "SecurityAreaControllerPS"),
    ] {
        builder.device_entries.push((
            geometry::full_ref(&builder.prefab, reference),
            class.into(),
            position,
        ));
    }
    Ok(())
}
