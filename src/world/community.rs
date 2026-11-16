use super::{
    Handles,
    geometry::{Anchor, hash},
    nodes::{self, list},
};
use crate::nodes as n;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub(super) struct Spot {
    pub spec: Value,
    pub reference: String,
    pub hash: u64,
    pub anchor: Anchor,
}
pub(super) struct Entry {
    pub spec: Value,
    pub spots: Vec<Spot>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AreaType {
    Regular,
    Streamable,
    Background,
}
impl AreaType {
    pub(super) fn read(spec: &Value) -> Result<Self> {
        match text(spec, "community_area_type", "Streamable") {
            "Regular" => Ok(Self::Regular),
            "Streamable" => Ok(Self::Streamable),
            "Background" => Ok(Self::Background),
            value => anyhow::bail!("unknown community area type {value}"),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Regular => "Regular",
            Self::Streamable => "Streamable",
            Self::Background => "Background",
        }
    }
}
fn text<'a>(value: &'a Value, key: &str, default: &'a str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or(default)
}
fn phases(spec: &Value) -> Result<Vec<&str>> {
    let mut phases = vec![text(spec, "phase", "default")];
    for phase in list(spec, "additional_phases")? {
        let phase = phase.as_str().context("community phase name")?;
        ensure!(
            !phases.contains(&phase),
            "duplicate community phase {phase}"
        );
        phases.push(phase);
    }
    Ok(phases)
}

pub(super) fn area(
    spec: &Value,
    reference: &str,
    source: u64,
    entries: &[Entry],
    h: &mut Handles,
) -> Result<Value> {
    let area_type = AreaType::read(spec)?;
    let mut data = nodes::common(
        spec,
        reference,
        if area_type == AreaType::Regular {
            "worldCompiledCommunityAreaNode"
        } else {
            "worldCompiledCommunityAreaNode_Streamable"
        },
    );
    let mut models = Vec::new();
    for entry in entries {
        let mut phase_data = Vec::new();
        let default = text(&entry.spec, "phase", "default");
        for phase in phases(&entry.spec)? {
            let ids = entry
                .spots
                .iter()
                .filter(|spot| text(&spot.spec, "phase", default) == phase)
                .map(|spot| json!({"$type":"worldGlobalNodeID","hash":spot.hash.to_string()}))
                .collect::<Vec<_>>();
            phase_data.push(json!({"$type":"communityCommunityEntryPhaseSpotsData","entryPhaseName":n::cname(phase),"timePeriodsData":[{"$type":"communityCommunityEntryPhaseTimePeriodData","isSequence":u8::from(nodes::flag(&entry.spec,"is_sequence",false)?),"periodName":n::cname(text(&entry.spec,"period","Day")),"spotNodeIds":ids}]}));
        }
        models.push(json!({"$type":"communityCommunityEntrySpotsData","entryName":n::cname(entry.spec["entry"].as_str().context("community entry name")?),"phasesData":phase_data}));
    }
    data["area"] = h.wrap(json!({"$type":"communityArea","entriesData":models}));
    data["sourceObjectId"] = json!({"$type":"entEntityID","hash":source.to_string()});
    if area_type != AreaType::Regular {
        let distance = super::geometry::scalar(spec, "streaming_distance", 250.)?;
        ensure!(
            distance > 0.,
            "streamable communities require a positive streaming distance"
        );
        data["streamingDistance"] = json!(distance);
    }
    Ok(h.wrap(data))
}
pub(super) fn registry(
    spec: &Value,
    source: u64,
    entries: &[Entry],
    h: &mut Handles,
) -> Result<Value> {
    let area_type = AreaType::read(spec)?;
    let mut initial = Vec::new();
    let mut spawn = Vec::new();
    let mut persistent = Vec::new();
    for entry in entries {
        let name = entry.spec["entry"].as_str().context("community entry")?;
        let default = text(&entry.spec, "phase", "default");
        let period = text(&entry.spec, "period", "Day");
        let spawn_policy = |key: &str, fallback: &str| -> Result<String> {
            let value = entry.spec.get(key).map_or(Ok(fallback), |v| {
                v.as_str().context("community spawn policy")
            })?;
            Ok(match value {
                "true_" => "true",
                "false_" => "false",
                value => value,
            }
            .to_owned())
        };
        let always_spawned = spawn_policy(
            "always_spawned",
            if area_type == AreaType::Regular {
                "true"
            } else {
                "default__false_"
            },
        )?;
        let spawn_in_view = spawn_policy("spawn_in_view", "default__true_")?;
        let mut initializers = Vec::new();
        if let Some(tag) = entry.spec.get("voice_tag").and_then(Value::as_str) {
            initializers.push(h.wrap(
                json!({"$type":"communityVoiceTagInitializer","voiceTagName":n::cname(tag)}),
            ));
        }
        let appearances = entry.spec.get("appearances").cloned().unwrap_or_else(|| {
            entry
                .spec
                .get("appearance")
                .map_or_else(|| json!(["default"]), |appearance| json!([appearance]))
        });
        let appearances = if let Some(values) = appearances.as_array() {
            ensure!(
                !values.is_empty(),
                "community entry {name} requires at least one appearance"
            );
            values
                .iter()
                .map(|v| Ok(n::cname(v.as_str().context("community appearance")?)))
                .collect::<Result<Vec<_>>>()?
        } else {
            vec![n::cname(
                appearances.as_str().context("community appearance")?,
            )]
        };
        initial.push(json!({"$type":"worldCommunityEntryInitialState","entryActiveOnStart":u8::from(nodes::flag(&entry.spec,"active_on_start",area_type != AreaType::Regular)?),"entryName":n::cname(name),"initialPhaseName":n::cname(default)}));
        let mut phase_data = Vec::new();
        for phase in phases(&entry.spec)? {
            let spots = entry
                .spots
                .iter()
                .filter(|spot| text(&spot.spec, "phase", default) == phase)
                .collect::<Vec<_>>();
            phase_data.push(h.wrap(json!({"$type":"communitySpawnPhase","alwaysSpawned":always_spawned,"appearances":appearances,"phaseName":n::cname(phase),"prefetchAppearance":u8::from(nodes::flag(&entry.spec,"prefetch_appearance",false)?),"timePeriods":[{"$type":"communityPhaseTimePeriod","categories":[],"hour":period,"isSequence":u8::from(nodes::flag(&entry.spec,"is_sequence",false)?),"markings":[],"quantity":entry.spec.get("quantity").cloned().unwrap_or(json!(spots.len())),"spotNodeRefs":spots.iter().map(|spot|n::node_ref(&spot.reference)).collect::<Vec<_>>()}]})));
        }
        let character = entry.spec["character"]
            .as_str()
            .context("community actor requires an explicit character record")?;
        spawn.push(h.wrap(json!({"$type":"communitySpawnEntry","characterRecordId":n::tweak_id(character),"entryName":n::cname(name),"initializers":initializers,"phases":phase_data,"spawnInView":spawn_in_view})));
        for spot in &entry.spots {
            fn fixed(value: f64) -> Result<Value> {
                let value = (value * 131072.).round_ties_even();
                ensure!(
                    value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX),
                    "community world position exceeds fixed-point range"
                );
                Ok(json!({"$type":"FixedPoint","Bits":value as i32}))
            }
            persistent.push(json!({"$type":"AISpotPersistentData","globalNodeId":{"$type":"worldGlobalNodeID","hash":spot.hash.to_string()},"isEnabled":1,"worldPosition":{"$type":"WorldPosition","x":fixed(spot.anchor.position.x)?,"y":fixed(spot.anchor.position.y)?,"z":fixed(spot.anchor.position.z)?},"yaw":spot.anchor.yaw}));
        }
    }
    let spawn_set = text(spec, "spawn_set_reference", "None");
    // Keep deterministic global-node-ID ordering, matching the native initial
    // registry. Quest-core add-ons export these records into that initial batch.
    persistent.sort_by_key(|spot| {
        spot["globalNodeId"]["hash"]
            .as_str()
            .and_then(|id| id.parse::<u64>().ok())
            .unwrap_or(0)
    });
    let identity = json!({"$type":"gameCommunityID","entityId":{"$type":"entEntityID","hash":source.to_string()}});
    let runtime=h.wrap(json!({"$type":"communityCommunityTemplateData","crowdEntries":[],"entries":spawn,"spawnSetReference":n::cname(spawn_set)}));
    let mut data = nodes::common(
        spec,
        text(spec, "registry_debug_name", "registry"),
        "worldCommunityRegistryNode",
    );
    data["sourcePrefabHash"] = spec
        .get("registry_source_prefab_hash")
        .cloned()
        .unwrap_or(json!("0"));
    let area_type_name = area_type.name();
    data.as_object_mut().context("community registry")?.extend(json!({"communitiesData":[{"$type":"worldCommunityRegistryItem","communityAreaType":area_type_name,"communityId":identity,"entriesInitialState":initial,"template":runtime}],"crowdCreationRegistry":null,"representsCrowd":0,"spawnSetNameToCommunityID":{"$type":"gameCommunitySpawnSetNameToID","entries":if spawn_set=="None"{Vec::new()}else{vec![json!({"$type":"gameCommunitySpawnSetNameToIDEntry","communityId":identity,"nameReference":n::cname(spawn_set)})]}},"workspotsPersistentData":persistent}).as_object().context("registry fields")?.clone());
    Ok(h.wrap(data))
}
pub(super) fn identity(spec: &Value, key: &str, reference: &str) -> Result<u64> {
    ensure!(
        spec.get(key).is_none(),
        "{key} is generated from the world reference"
    );
    let id = hash(reference);
    ensure!(id != 0, "world identity cannot be zero");
    Ok(id)
}
