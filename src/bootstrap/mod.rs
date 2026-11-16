//! Native quest startup through a conversation request and persistent quest host.
//!
//! One bootstrap owns the registered conversation slot and vendor overrides in a
//! package. Aggregate child phases here rather than publishing competing overrides.
use crate::{compiler::Build, native::Binaries};
use anyhow::{Context, Result, ensure};
use ghostline_red::material::ArchiveSet;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

mod binary;
mod host;
mod sector;
mod vendor;
mod workspots;

/// A child questphase launched by the persistent bootstrap host.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Phase {
    pub resource: String,
    #[serde(default = "default_input")]
    pub input: String,
    /// Completion socket for a finite quest. None hosts a persistent controller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}
fn default_input() -> String {
    "In1".into()
}

/// Generated bootstrap ownership and its child phase interfaces.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub id: String,
    pub phases: Vec<Phase>,
    #[serde(skip)]
    workspots: Vec<Value>,
}

impl Spec {
    /// Creates a bootstrap; attachment collects its generated persistent workspots.
    pub fn new(id: String, phases: Vec<Phase>) -> Self {
        Self {
            id,
            phases,
            workspots: Vec::new(),
        }
    }
    /// Number of spots merged into the initial native registration batch.
    pub fn workspot_count(&self) -> usize {
        self.workspots.len()
    }
    /// Supplies the core's catalogued workspots before the initial native batch.
    pub fn with_workspots(mut self, workspots: Vec<Value>) -> Self {
        self.workspots = workspots;
        self
    }
}

const PARENT: &str = "$/03_night_city/c_watson/kabuki/wk_05/wk_05_env_prefabVP3Y3GA/wk_05_decoration_prefab7QFY6AQ/wk_05_deco_regular_prefabJDVO25Y/decoset_shop_ak_v1_005_prefab7DOEVAY";
/// Native conversation slot owned by packages using this bootstrap.
pub const REGISTERED_SCENE: &str =
    r"base\open_world\scenes\generic_vendors\versions\gold\chinese_food_store.scene";
const VENDOR_SECTOR: &str =
    r"base\worlds\03_night_city\_compiled\default\exterior_-10_16_0_1.streamingsector";

/// Adds generated startup resources and replaces registration for its child roots.
///
/// # Errors
/// Rejects duplicate ownership, missing child documents, invalid interfaces or
/// resource collisions. Native generation additionally requires official archives.
pub fn attach(build: &mut Build, mut spec: Spec) -> Result<()> {
    ensure!(
        build.bootstrap.is_none(),
        "a package must aggregate its phases in one bootstrap"
    );
    ensure!(
        crate::manifest::identifier(&spec.id) && !spec.phases.is_empty(),
        "bootstrap needs a valid ID and child phases"
    );
    let mut children = BTreeSet::new();
    ensure!(
        spec.phases.len() == 1 || spec.phases.iter().all(|phase| phase.output.is_none()),
        "aggregate finite quests inside a controller before bootstrapping"
    );
    for phase in &spec.phases {
        crate::project::depot_path(&phase.resource)?;
        ensure!(
            phase.resource.ends_with(".questphase")
                && build.artifacts.contains_key(&phase.resource),
            "bootstrap child {} is not a generated questphase",
            phase.resource
        );
        ensure!(
            !phase.input.is_empty() && phase.input != "CutDestination",
            "invalid bootstrap input"
        );
        ensure!(
            phase
                .output
                .as_ref()
                .is_none_or(|output| !output.is_empty()),
            "invalid bootstrap completion output"
        );
        let exits = phase
            .output
            .iter()
            .map(|output| ("completion".to_owned(), output.clone()))
            .collect();
        crate::audit::interface(
            &build.artifacts[&phase.resource],
            std::slice::from_ref(&phase.input),
            &exits,
            &Default::default(),
        )
        .with_context(|| format!("bootstrap child interface {}", phase.resource))?;
        ensure!(
            children.insert(phase.resource.clone()),
            "duplicate bootstrap child {}",
            phase.resource
        );
    }
    let root = format!("mod\\quest_bootstrap\\{}", spec.id);
    let scene_path = format!("{root}\\start.scene");
    let commands = vec![
        json!({"key":"started","kind":"fact","fact":format!("{}_bootstrap_started", spec.id),"value":1,"on_out":[{"node":"dispatch_guard"}]}),
        json!({"key":"dispatch_guard","kind":"fact_condition","fact":format!("{}_bootstrap_dispatched", spec.id),"on_true":[{"node":"end"}],"on_false":[{"node":"request"}]}),
        json!({"key":"request","kind":"fact","fact":format!("{}_bootstrap_requested", spec.id),"value":1,"on_out":[{"node":"end"}]}),
    ];
    let scene = crate::scene::compile(
        &json!({
            "name":format!("{}_bootstrap",spec.id),"archive_path":scene_path,"scene_category":"otherOpenWorld",
            "actors":[{"key":"context","kind":"context","context_actor_name":"~/../fluff_vendor_ws","lipsync":false},{"key":"player","kind":"player","lipsync":false}],
            "start_nodes":[{"key":"start_0","on_start":[{"node":"started"}]},{"key":"start_1","on_start":[{"node":"started"}]}],
            "entry_points":[{"name":"Start 0","node":"start_0"},{"name":"Start 1","node":"start_1"}],
            "quest_nodes":commands,"end_node":{"key":"end"},"exit_points":[{"name":"Out","node":"end"}]
        }),
        &json!({}),
    )?;
    let world = crate::world::compile(&json!({
        "name":format!("{}_bootstrap",spec.id),"prefab_root":PARENT,"origin":[-1173.06665,2096.36304,8.98351],
        "always_loaded_sector_path":format!("{root}\\world.streamingsector"),"block_path":format!("{root}\\all.streamingblock"),"streaming_box":"world",
        "always_loaded_sectors":[VENDOR_SECTOR],
        "conversations":[{"ref":format!("#quest_bootstrap_{}",spec.id),"position":[-1159.985,2116.031,12.063],"sector":"always_loaded","binding":"direct","node_data":{"time_of_day_mask":1},
            "outline":{"points":[[-50000,-50000,-5000],[50000,-50000,-5000],[50000,50000,-5000],[-50000,50000,-5000]],"height":10000},
            "scene":scene_path,"registered_scene":REGISTERED_SCENE,"workspots":["~/../fluff_vendor_ws"],"ignore_global_limit":true,"ignore_local_limit":true}]
    }))?;
    let mut generated = world.artifacts;
    generated.insert(scene_path, scene);
    crate::conversation_metadata::bind(&mut generated, &world.scene_replacements)?;
    for path in generated.keys() {
        ensure!(
            !build.artifacts.contains_key(path) && !build.assets.contains_key(path),
            "bootstrap resource collision {path}"
        );
    }
    let name = format!("{}.archive.xl", spec.id);
    let mut registration: Value = serde_json::from_slice(
        build
            .resources
            .get(&name)
            .context("bootstrap package registration")?,
    )?;
    if let Some(phases) = registration
        .pointer_mut("/quest/phases")
        .and_then(Value::as_array_mut)
    {
        phases.retain(|phase| !children.contains(phase["path"].as_str().unwrap_or_default()));
        if phases.is_empty() {
            registration
                .as_object_mut()
                .context("registration object")?
                .remove("quest");
        }
    }
    if registration.get("streaming").is_none() {
        registration["streaming"] = json!({"blocks":[]});
    }
    if registration["streaming"].get("blocks").is_none() {
        registration["streaming"]["blocks"] = json!([]);
    }
    let blocks = registration["streaming"]["blocks"]
        .as_array_mut()
        .context("bootstrap streaming blocks")?;
    for block in world.registration["streaming"]["blocks"]
        .as_array()
        .context("generated bootstrap block")?
    {
        ensure!(
            !blocks.contains(block),
            "duplicate bootstrap streaming block"
        );
        blocks.push(block.clone());
    }
    build
        .resources
        .insert(name, serde_json::to_vec_pretty(&registration)?);
    build.artifacts.extend(generated);
    crate::audit::bound_interfaces(&build.artifacts)?;
    fn collect(value: &mut Value, spots: &mut Vec<Value>) -> Result<()> {
        if value["$type"] == "worldCommunityRegistryNode" {
            let entries = value["workspotsPersistentData"]
                .as_array_mut()
                .context("community persistent workspots")?;
            spots.extend(std::mem::take(entries));
        }
        match value {
            Value::Object(fields) => {
                for child in fields.values_mut() {
                    collect(child, spots)?;
                }
            }
            Value::Array(rows) => {
                for child in rows {
                    collect(child, spots)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    for (path, document) in &mut build.artifacts {
        if path.starts_with("mod\\") {
            collect(document, &mut spec.workspots)?;
        }
    }
    build.bootstrap = Some(spec);
    Ok(())
}

/// Generates bounded native overrides from official archives, excluding mods.
///
/// # Errors
/// Fails on incompatible game layouts or ambiguous native bindings.
pub(crate) fn encode(game: &Path, spec: &Spec, schema: &ghostline_red::schema::RedSchema) -> Result<Binaries> {
    let mut pending = ["content", "ep1", "hot"]
        .map(|part| game.join("archive/pc").join(part))
        .to_vec();
    let mut paths = Vec::new();
    while let Some(directory) = pending.pop() {
        if !directory.exists() {
            continue;
        }
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else if entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("archive"))
            {
                paths.push(entry.path());
            }
        }
    }
    let archives = ArchiveSet::from_paths(paths)?;
    let host = host::build(&archives, spec, schema)?;
    let (vendor_path, vendor_bytes) = vendor::build(&archives)?;
    let vendor_bytes = workspots::merge(&vendor_bytes, &spec.workspots)?;
    Ok([sector::build(&archives)?, (vendor_path, vendor_bytes), host]
        .into_iter()
        .collect())
}
