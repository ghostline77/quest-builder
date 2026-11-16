//! Native node streaming recipes and current compiled setup records.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

const FORCE: u32 = 1 << 1;
const QUEST: u32 = 1 << 4;
const BUILDING: u32 = 1 << 6;
const CULLABLE: u32 = 1 << 13;
const PERIPHERAL: u32 = 1 << 15;
const FALLBACK: u32 = 1 << 18;

pub(super) fn current_record(data: &mut Value) -> Result<()> {
    let integer = |key: &str, max: u64| -> Result<u64> {
        let value = data[key]
            .as_u64()
            .with_context(|| format!("world node {key}"))?;
        ensure!(value <= max, "world node {key} exceeds native range");
        Ok(value)
    };
    let low = integer("Uk10", u64::from(u16::MAX))?;
    let high = integer("Uk11", u64::from(u16::MAX))?;
    let index = integer("NodeIndex", 0x7_ffff)? | ((low & 15) << 16);
    ensure!(index <= 0x7_ffff, "world node index exceeds native range");
    let mask = integer("Uk12", u64::from(u16::MAX))?;
    let group: u64 = data["Uk13"]
        .as_str()
        .context("node data group word")?
        .parse()?;
    let padding: u64 = data["Uk14"]
        .as_str()
        .context("node padding word")?
        .parse()?;
    ensure!(
        group <= 255 && padding == 0,
        "nonzero legacy node padding has no native meaning"
    );
    data["NodeIndex"] = json!(index);
    data["NodeFlags"] = json!((low >> 4) | ((high & 255) << 12));
    data["StreamingPriority"] = json!(high >> 8);
    data["TimeOfDayMask"] = json!(mask);
    data["InstanceIndex"] = json!(u32::MAX);
    data["DataGroup"] = json!(group);
    data["Padding"] = json!(vec![0_u8; 11]);
    let object = data.as_object_mut().context("world node record")?;
    for key in ["Uk10", "Uk11", "Uk12", "Uk13", "Uk14"] {
        object.remove(key);
    }
    Ok(())
}

pub(super) fn apply(data: &mut Value, kind: &str, quest: bool) -> Result<()> {
    let (mut flags, priority, community) = match kind {
        "worldCompiledCommunityAreaNode"
        | "worldCompiledCommunityAreaNode_Streamable"
        | "worldCommunityRegistryNode" => (FORCE, 1, true),
        "worldTriggerAreaNode"
        | "worldInterestingConversationsAreaNode"
        | "worldAmbientAreaNode"
        | "worldAudioAttractAreaNode"
        | "gameKillTriggerNode" => (FORCE | BUILDING, 255, false),
        "worldEntityNode" | "worldStaticMeshNode" => {
            (FORCE | BUILDING | CULLABLE | PERIPHERAL, 21, false)
        }
        "worldDeviceNode" => (BUILDING | CULLABLE | PERIPHERAL, 21, false),
        "worldAISpotNode" => (
            FORCE | BUILDING | CULLABLE | PERIPHERAL | FALLBACK,
            1,
            false,
        ),
        "worldStaticMarkerNode"
        | "worldSplineNode"
        | "worldRaceSplineNode"
        | "worldSpeedSplineNode"
        | "worldPatrolSplineNode"
        | "worldGuardAreaNode"
        | "worldCrowdNullAreaNode"
        | "worldPopulationSpawnerNode" => (FORCE | BUILDING, 1, false),
        "worldVehicleForbiddenAreaNode" => (FORCE | BUILDING, 1, false),
        _ => anyhow::bail!("Missing native streaming recipe for world node {kind}"),
    };
    if quest && !community {
        flags = if kind == "worldAmbientAreaNode" {
            flags | QUEST
        } else {
            (flags & !BUILDING) | QUEST
        };
    }
    let object = data.as_object_mut().context("node data overrides")?;
    let number = |key: &str, default: u64, maximum: u64| -> Result<u64> {
        let value = object.get(key).map_or(Ok(default), |v| {
            v.as_u64()
                .with_context(|| format!("{key} must be an unsigned integer"))
        })?;
        ensure!(value <= maximum, "{key} exceeds native storage range");
        Ok(value)
    };
    let flags = number("node_flags", u64::from(flags), (1 << 20) - 1)?;
    let priority = number("streaming_priority", priority, u64::from(u8::MAX))?;
    let time_mask = number("time_of_day_mask", 0, u64::from(u16::MAX))?;
    // Bits 0..19 are the node index; these words cover bits 16..63.
    object.entry("uk10").or_insert(json!((flags & 0xfff) << 4));
    object
        .entry("uk11")
        .or_insert(json!((flags >> 12) | (priority << 8)));
    object.entry("uk12").or_insert(json!(time_mask));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_native_area_and_quest_flag_words() {
        let mut area = json!({"time_of_day_mask":1});
        apply(&mut area, "worldInterestingConversationsAreaNode", false).unwrap();
        assert_eq!(
            (
                area["uk10"].as_u64(),
                area["uk11"].as_u64(),
                area["uk12"].as_u64()
            ),
            (Some(1056), Some(65280), Some(1))
        );
        let mut quest = json!({});
        apply(&mut quest, "worldTriggerAreaNode", true).unwrap();
        assert_eq!(
            (quest["uk10"].as_u64(), quest["uk11"].as_u64()),
            (Some(288), Some(65280))
        );
    }
}
