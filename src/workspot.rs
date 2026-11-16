//! Procedural workspot graphs from animation and entry specifications.
use crate::{graph, nodes as n, project::depot_path};
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    depot: String,
    rig: String,
    animsets: Vec<String>,
    #[serde(default)]
    animation_setup_hash: u64,
    root: Entry,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Entry {
    Sequence {
        key: String,
        #[serde(default)]
        idle: Option<String>,
        #[serde(default)]
        repeat: bool,
        entries: Vec<Entry>,
    },
    Pause {
        key: String,
        seconds: f32,
    },
    Clip {
        key: String,
        animation: String,
    },
    Random {
        key: String,
        min_clips: u32,
        max_clips: u32,
        pause_seconds: f32,
        pause_deviation: f32,
        weights: Vec<f32>,
        entries: Vec<Entry>,
    },
}

#[derive(Default)]
struct Builder {
    handle: u32,
    entries: BTreeSet<String>,
}
impl Builder {
    fn wrap(&mut self, data: Value) -> Value {
        let handle = self.handle;
        self.handle += 1;
        json!({"HandleId":handle.to_string(),"Data":data})
    }

    fn entry(&mut self, entry: Entry) -> Result<Value> {
        let key = match &entry {
            Entry::Sequence { key, .. }
            | Entry::Pause { key, .. }
            | Entry::Clip { key, .. }
            | Entry::Random { key, .. } => key,
        };
        ensure!(
            crate::manifest::identifier(key) && self.entries.insert(key.clone()),
            "invalid or duplicate workspot entry {key}"
        );
        let id = u32::try_from(self.entries.len())?;
        let mut data = match entry {
            Entry::Sequence {
                idle,
                repeat,
                entries,
                ..
            } => {
                ensure!(!entries.is_empty(), "empty workspot sequence");
                let children = entries
                    .into_iter()
                    .map(|e| self.entry(e))
                    .collect::<Result<Vec<_>>>()?;
                json!({"$type":"workSequence","category":"Any","disableAutoTransition":0,"flags":0,"idleAnim":n::cname(idle.as_deref().unwrap_or("None")),"list":children,"loopInfinitely":u8::from(repeat),"previousLoopInfinitely":0})
            }
            Entry::Pause { seconds, .. } => {
                ensure!(
                    seconds.is_finite() && seconds >= 0.,
                    "invalid workspot pause"
                );
                json!({"$type":"workPauseClip","flags":32,"blendOutTime":0.5,"timeMin":seconds,"timeMax":seconds})
            }
            Entry::Clip { animation, .. } => {
                ensure!(
                    !animation.trim().is_empty(),
                    "empty workspot animation name"
                );
                json!({"$type":"workAnimClip","flags":2,"blendOutTime":0.5,"animName":n::cname(&animation)})
            }
            Entry::Random {
                min_clips,
                max_clips,
                pause_seconds,
                pause_deviation,
                weights,
                entries,
                ..
            } => {
                ensure!(
                    !entries.is_empty() && weights.len() == entries.len(),
                    "workspot random weights must match its entries"
                );
                ensure!(
                    min_clips > 0 && max_clips >= min_clips,
                    "invalid workspot random clip count"
                );
                ensure!(
                    pause_seconds.is_finite()
                        && pause_seconds >= 0.
                        && pause_deviation.is_finite()
                        && pause_deviation >= 0.,
                    "invalid workspot random pause"
                );
                ensure!(
                    weights.iter().all(|w| w.is_finite() && *w >= 0.)
                        && weights.iter().any(|w| *w > 0.),
                    "invalid workspot random weights"
                );
                let children = entries
                    .into_iter()
                    .map(|e| self.entry(e))
                    .collect::<Result<Vec<_>>>()?;
                json!({"$type":"workRandomList","disableAutoTransition":0,"dontRepeatLastAnims":1,"flags":0,"idleAnim":n::cname("None"),"list":children,"minClips":min_clips,"maxClips":max_clips,"pauseBetweenLength":pause_seconds,"pauseLengthDeviation":pause_deviation,"pauseBlendOutTime":0.5,"weights":weights})
            }
        };
        data["id"] = json!({"$type":"workWorkEntryId","id":id});
        Ok(self.wrap(data))
    }
}

/// Generates a native animation hub and workspot tree without a base resource.
/// Allocates entry IDs and rejects duplicate keys, invalid timings and weights.
pub fn compile(value: &Value) -> Result<Value> {
    let spec: Spec = serde_json::from_value(value.clone())?;
    depot_path(&spec.depot)?;
    depot_path(&spec.rig)?;
    ensure!(
        spec.depot.starts_with("mod\\") && spec.depot.ends_with(".workspot"),
        "workspot needs a mod-owned .workspot depot"
    );
    ensure!(
        spec.rig.ends_with(".rig") && !spec.animsets.is_empty(),
        "workspot needs a rig and animation sets"
    );
    for path in &spec.animsets {
        depot_path(path)?;
        ensure!(
            path.ends_with(".anims"),
            "workspot animation set must end in .anims"
        );
    }
    let mut b = Builder::default();
    let root_entry = b.entry(spec.root)?;
    let id_counter = u32::try_from(b.entries.len())?;
    let hub = b.wrap(json!({"$type":"animAnimNode_WorkspotHub","id":u32::MAX,"facialIdleFemaleAnimation":n::cname("idle__neutral__female"),"facialIdleMaleAnimation":n::cname("idle__neutral__male"),"facialKeyWeight":1}));
    let hub_id = hub["HandleId"].clone();
    let output = b.wrap(json!({"$type":"animAnimNode_Output","id":u32::MAX,"node":{"$type":"animPoseLink","node":hub}}));
    let output_id = output["HandleId"].clone();
    let graph_root = b.wrap(json!({"$type":"animAnimNode_Root","id":u32::MAX,"nodes":[output]}));
    let root_id = graph_root["HandleId"].clone();
    let variables = b.wrap(json!({"$type":"animAnimVariableContainer"}));
    let animsets = spec.animsets.iter().map(|p|json!({"$type":"animAnimSetupEntry","animSet":n::resource(p),"priority":128,"variableNames":[]})).collect::<Vec<_>>();
    let mut rig = n::resource(&spec.rig);
    rig["Flags"] = json!("Soft");
    let tree = b.wrap(json!({"$type":"workWorkspotTree","animGraphSlotName":n::cname("WORKSPOT"),"autoTransitionBlendTime":1,"blendOutTime":0,"finalAnimsets":[{"$type":"workWorkspotAnimsetEntry","animations":{"$type":"animAnimSetup","cinematics":animsets,"gameplay":[],"hash":spec.animation_setup_hash.to_string()},"loadingHandles":spec.animsets.iter().map(|p|n::resource(p)).collect::<Vec<_>>(),"rig":rig}],"idCounter":id_counter,"inertializationDurationEnter":0.5,"inertializationDurationExitForced":0.2,"inertializationDurationExitNatural":0.5,"initialCanUseExits":1,"itemsPolicy":"ItemPolicy_SpawnItemOnIdleChange, ItemPolicy_DespawnItemOnIdleChange, ItemPolicy_DespawnItemOnReaction","rootEntry":root_entry,"sequencesTimeLimit":1,"unmountBodyCarry":1}));
    graph::order_handles(
        &json!({"Header":{"WolvenKitVersion":"8.17.4","WKitJsonVersion":"0.0.9","GameVersion":2310,"ExportedDateTime":"1970-01-01T00:00:00Z","DataType":"CR2W","ArchiveFileName":spec.depot},"Data":{"Version":195,"BuildVersion":0,"RootChunk":{"$type":"workWorkspotResource","cookingPlatform":"PLATFORM_PC","animFeatures":[{"$type":"animAnimFeatureEntry","className":n::cname("animAnimFeatureUpdateWorkspot"),"name":n::cname("UpdateWorkspot"),"debugEnabled":0,"forceAllocate":0}],"nodesToInit":[graph_root,{"HandleRefId":output_id},{"HandleRefId":hub_id}],"rootNode":{"HandleRefId":root_id},"timeDeltaMultiplier":1,"variables":variables,"workspotTree":tree},"EmbeddedFiles":[]}}),
    )
}
