//! Direct world sectors, communities, devices, activity spots and spline generation.
mod community;
mod computer;
mod conversation;
mod geometry;
mod loot;
mod nodes;
mod patches;
mod security;
mod spline;
mod streaming;

use crate::{compiler::Artifacts, nodes as native, project::depot_path};
use anyhow::{Context, Result, ensure};
pub use geometry::hash as node_ref_hash;
use geometry::{Anchor, Position};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

#[derive(Default)]
struct Handles {
    next: u64,
}
impl Handles {
    fn wrap(&mut self, data: Value) -> Value {
        let id = self.next;
        self.next += 1;
        json!({"HandleId":id.to_string(),"Data":data})
    }
}
#[derive(Default)]
struct Sector {
    handles: Handles,
    nodes: Vec<Value>,
    data: Vec<Value>,
    references: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Placement {
    Quest,
    AlwaysLoaded,
    Exterior,
}
impl Placement {
    fn read(spec: &Value, default: Self) -> Result<Self> {
        match spec.get("sector").map(Value::as_str) {
            None => Ok(default),
            Some(Some("quest")) => Ok(Self::Quest),
            Some(Some("always_loaded")) => Ok(Self::AlwaysLoaded),
            Some(Some("exterior")) => Ok(Self::Exterior),
            _ => anyhow::bail!("sector must be quest, always_loaded or exterior"),
        }
    }
}

/// World documents and the registration needed to make them available in game.
#[derive(Debug)]
pub struct Build {
    pub artifacts: Artifacts,
    pub registration: Value,
    /// Generated TweakXL records required by placed loot containers.
    pub loose: BTreeMap<String, Vec<u8>>,
    /// Registered scene paths mapped to generated scene sources.
    pub scene_replacements: BTreeMap<String, String>,
}

struct Builder {
    prefab: String,
    anchors: BTreeMap<String, Anchor>,
    identities: HashSet<String>,
    quest: Sector,
    always: Sector,
    exterior: Sector,
    positions: Vec<Position>,
    device_entries: Vec<(String, String, Position)>,
    registry_ids: HashSet<u64>,
}
impl Builder {
    fn sector_mut(&mut self, placement: Placement) -> &mut Sector {
        match placement {
            Placement::Quest => &mut self.quest,
            Placement::AlwaysLoaded => &mut self.always,
            Placement::Exterior => &mut self.exterior,
        }
    }
    fn anchor(&self, spec: &Value) -> Result<Anchor> {
        let mut anchor = geometry::position(spec.get("position"), &self.anchors)?;
        anchor.yaw = geometry::yaw(spec.get("yaw"), &self.anchors, anchor.yaw)?;
        Ok(anchor)
    }
    fn add(
        &mut self,
        reference: &str,
        node: Value,
        anchor: Anchor,
        overrides: Option<&Value>,
        placement: Placement,
    ) -> Result<()> {
        let full = geometry::full_ref(&self.prefab, reference);
        ensure!(
            self.identities.insert(full.clone()),
            "duplicate world NodeRef {full}"
        );
        let sector = self.sector_mut(placement);
        let index = sector.nodes.len();
        let kind = node["Data"]["$type"].as_str().context("world node type")?;
        let mut overrides = overrides.cloned().unwrap_or_else(|| json!({}));
        streaming::apply(&mut overrides, kind, placement == Placement::Quest)?;
        sector.nodes.push(node);
        sector.data.push(node_data(
            index,
            json!({"$type":"NodeRef","$storage":"string","$value":full}),
            anchor,
            Some(&overrides),
        )?);
        sector.references.push(full.clone());
        let anchor = Anchor {
            index: Some(index),
            ..anchor
        };
        self.anchors.insert(reference.to_owned(), anchor);
        self.anchors
            .insert(reference.trim_start_matches('#').to_owned(), anchor);
        self.anchors.insert(full, anchor);
        self.positions.push(anchor.position);
        Ok(())
    }
    fn communities(&mut self, spec: &Value) -> Result<()> {
        let mut communities = Vec::new();
        if let Some(community) = spec.get("community").filter(|v| !v.is_null()) {
            communities.push(community);
        }
        communities.extend(nodes::list(spec, "communities")?);
        for spec in communities {
            let regular = community::AreaType::read(spec)? == community::AreaType::Regular;
            let placement = Placement::read(
                spec,
                if regular {
                    Placement::AlwaysLoaded
                } else {
                    Placement::Quest
                },
            )?;
            ensure!(
                !regular || placement == Placement::AlwaysLoaded,
                "Regular communities must be hosted in an AlwaysLoaded sector"
            );
            let reference = spec["ref"].as_str().context("community reference")?;
            let full = geometry::full_ref(&self.prefab, reference);
            let source = community::identity(spec, "source_object_id", &full)?;
            let declarations = spec
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_else(|| vec![spec.clone()]);
            ensure!(!declarations.is_empty(), "community requires entries");
            let mut entries = Vec::new();
            let mut names = HashSet::new();
            for (entry_index, declaration) in declarations.iter().enumerate() {
                let mut entry = spec.as_object().context("community object")?.clone();
                entry.remove("entries");
                entry.extend(declaration.as_object().context("community entry")?.clone());
                entry
                    .entry("entry")
                    .or_insert(json!(format!("entry_{}", entry_index + 1)));
                let entry = Value::Object(entry);
                let name = entry["entry"].as_str().context("community entry name")?;
                ensure!(
                    names.insert(name.to_owned()),
                    "duplicate community entry {name}"
                );
                let spots = declaration
                    .get("spots")
                    .or_else(|| entry.get("spots"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_else(|| {
                        vec![
                            declaration
                                .get("spot")
                                .or_else(|| spec.get("spot"))
                                .cloned()
                                .unwrap_or(json!({})),
                        ]
                    });
                ensure!(
                    !spots.is_empty(),
                    "community entry needs spawn/activity spots"
                );
                let mut resolved = Vec::new();
                for (spot_index, spot) in spots.iter().enumerate() {
                    let mut merged = spec
                        .get("spot")
                        .and_then(Value::as_object)
                        .cloned()
                        .unwrap_or_default();
                    merged.extend(spot.as_object().context("community activity spot")?.clone());
                    let spot = Value::Object(merged);
                    let reference = spot
                        .get("ref")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| {
                            format!(
                                "#{}_{}_spot_{}",
                                reference.trim_start_matches('#'),
                                name,
                                spot_index + 1
                            )
                        });
                    let full = geometry::full_ref(&self.prefab, &reference);
                    let hash = community::identity(&spot, "global_node_id", &full)?;
                    let anchor = self.anchor(&spot)?;
                    let handles = &mut self.sector_mut(placement).handles;
                    let node = nodes::spot(&spot, &reference, handles)?;
                    self.add(&reference, node, anchor, spot.get("node_data"), placement)?;
                    resolved.push(community::Spot {
                        spec: spot,
                        reference: full,
                        hash,
                        anchor,
                    });
                }
                entries.push(community::Entry {
                    spec: entry,
                    spots: resolved,
                });
            }
            let registry_id =
                community::identity(spec, "registry_node_id", &format!("{full}_registry"))?;
            ensure!(
                registry_id != source
                    && entries
                        .iter()
                        .flat_map(|e| &e.spots)
                        .all(|s| s.hash != registry_id)
                    && self.registry_ids.insert(registry_id),
                "community registry identity collides with another owned identity"
            );
            let anchor = self.anchor(spec)?;
            let handles = &mut self.sector_mut(placement).handles;
            let area = community::area(spec, reference, source, &entries, handles)?;
            self.add(reference, area, anchor, spec.get("node_data"), placement)?;
            let registry = community::registry(spec, source, &entries, &mut self.always.handles)?;
            let index = self.always.nodes.len();
            self.always.nodes.push(registry);
            self.always.data.push(node_data(index,json!({"$type":"NodeRef","$storage":"uint64","$value":registry_id.to_string()}),Anchor{position:Position{x:0.,y:0.,z:0.},yaw:0.,index:None},Some(&json!({"max_streaming_distance":17.320507,"streaming_distance":100000000,"uk10":32})))?);
        }
        Ok(())
    }
}

fn document(depot: &str, root: Value) -> Value {
    json!({"Header":{"WolvenKitVersion":"8.17.4","WKitJsonVersion":"0.0.9","GameVersion":2310,"ExportedDateTime":"1970-01-01T00:00:00Z","DataType":"CR2W","ArchiveFileName":depot},"Data":{"Version":195,"BuildVersion":0,"RootChunk":root,"EmbeddedFiles":[]}})
}
fn node_data(
    index: usize,
    reference: Value,
    anchor: Anchor,
    overrides: Option<&Value>,
) -> Result<Value> {
    let empty = json!({});
    let o = overrides.unwrap_or(&empty);
    let orientation = if let Some(q) = o.get("orientation") {
        let mut norm = 0.;
        for axis in ["i", "j", "k", "r"] {
            let v = geometry::number(&q[axis])?;
            norm += v * v;
        }
        ensure!(
            norm.is_finite() && norm > 0.,
            "orientation must have nonzero finite norm"
        );
        json!({"$type":"Quaternion","i":q["i"],"j":q["j"],"k":q["k"],"r":q["r"]})
    } else {
        geometry::quaternion(anchor.yaw)
    };
    Ok(
        json!({"Id":"0","NodeIndex":index,"Position":anchor.position.vector4(0.),"Orientation":orientation,"Scale":{"$type":"Vector3","X":geometry::scalar(o,"scale_x",1.)?,"Y":geometry::scalar(o,"scale_y",1.)?,"Z":geometry::scalar(o,"scale_z",1.)?},"Pivot":anchor.position.vector(),"Bounds":{"$type":"Box","Max":anchor.position.vector4(0.),"Min":anchor.position.vector4(0.)},"QuestPrefabRefHash":reference,"UkHash1":nodes::zero_ref(),"CookedPrefabData":{"DepotPath":{"$type":"ResourcePath","$storage":"uint64","$value":"0"},"Flags":"Default"},"MaxStreamingDistance":geometry::scalar(o,"max_streaming_distance",120.)?,"UkFloat1":geometry::scalar(o,"streaming_distance",100.)?,"Uk10":o.get("uk10").cloned().unwrap_or(json!(1056)),"Uk11":o.get("uk11").cloned().unwrap_or(json!(256)),"Uk12":o.get("uk12").cloned().unwrap_or(json!(0)),"Uk13":o.get("uk13").cloned().unwrap_or(json!("0")),"Uk14":o.get("uk14").cloned().unwrap_or(json!("0"))}),
    )
}
fn sector(depot: &str, category: &str, level: u64, mut value: Sector) -> Result<Value> {
    for record in &mut value.data {
        streaming::current_record(record)?;
    }
    let mut result = document(
        depot,
        json!({"$type":"worldStreamingSector","category":category,"externInplaceResource":{"DepotPath":{"$type":"ResourcePath","$storage":"uint64","$value":"0"},"Flags":"Default"},"level":level,"localInplaceResource":[],"nodeData":{"BufferId":"0","Flags":4063232,"Type":"WolvenKit.RED4.Archive.Buffer.worldNodeDataBuffer, WolvenKit.RED4, Version=8.17.4.0, Culture=neutral, PublicKeyToken=null","Data":value.data},"nodeRefs":value.references.iter().map(|v|native::node_ref(v)).collect::<Vec<_>>(),"nodes":value.nodes,"persistentNodeIndex":0,"persistentNodes":[],"variantIndices":[0],"variantNodes":[],"version":62}),
    );
    result["Data"]["RootChunk"]["cookingPlatform"] = json!("PLATFORM_PC");
    result["Data"]["RootChunk"]["version"] = json!(68);
    result["Data"]["RootChunk"]["nodeData"]["Version"] = json!(68);
    Ok(result)
}
fn descriptor(
    category: &str,
    path: &str,
    level: u64,
    prefab: Option<&str>,
    min: Position,
    max: Position,
) -> Value {
    json!({"$type":"worldStreamingSectorDescriptor","blockIndex":{"$type":"worldStreamingBlockIndex","oup":"Base","rldGridCell":0},"category":category,"data":nodes::resource(path),"level":level,"numNodeRanges":1,"questPrefabNodeRef":prefab.map_or_else(nodes::zero_ref,native::node_ref),"streamingBox":{"$type":"Box","Max":max.vector4(f64::from(f32::MAX)),"Min":min.vector4(-f64::from(f32::MAX))},"variants":[]})
}

/// Generates world resources and registration without reading source snapshots.
pub fn compile(spec: &Value) -> Result<Build> {
    let name = spec
        .get("name")
        .and_then(Value::as_str)
        .context("world requires a namespace")?;
    ensure!(crate::manifest::identifier(name), "invalid world namespace");
    let prefab = spec["prefab_root"].as_str().context("world prefab_root")?;
    ensure!(
        prefab.starts_with('#') || prefab.starts_with("$/"),
        "prefab_root must be an explicit NodeRef"
    );
    let origin = Position::read(&spec["origin"])?;
    let yaw = geometry::scalar(&spec["origin"], "yaw", geometry::scalar(spec, "yaw", 0.)?)?;
    let mut builder = Builder {
        prefab: prefab.into(),
        anchors: BTreeMap::from([(
            "origin".into(),
            Anchor {
                position: origin,
                yaw,
                index: None,
            },
        )]),
        identities: HashSet::new(),
        quest: Sector::default(),
        always: Sector::default(),
        exterior: Sector::default(),
        positions: vec![origin],
        device_entries: Vec::new(),
        registry_ids: HashSet::new(),
    };
    let mut buffers = HashSet::new();
    let mut conversation_artifacts = BTreeMap::new();
    let mut loot_records = serde_json::Map::new();
    for key in [
        "markers",
        "triggers",
        "conversations",
        "entities",
        "crowd_null_areas",
        "devices",
        "loot_containers",
    ] {
        for item in nodes::list(spec, key)? {
            let reference = item["ref"].as_str().context("world node reference")?;
            let anchor = builder.anchor(item)?;
            let placement = Placement::read(item, Placement::Quest)?;
            let handles = &mut builder.sector_mut(placement).handles;
            let node = match key {
                "markers" => nodes::marker(item, reference, handles)?,
                "triggers" => nodes::trigger(item, reference, handles, false)?,
                "conversations" => {
                    let mut bound = item.clone();
                    if let Some(registered) = item.get("registered_scene") {
                        let registered = registered
                            .as_str()
                            .context("registered_scene must be a depot path")?;
                        depot_path(registered)?;
                        ensure!(
                            registered.ends_with(".scene"),
                            "registered_scene must end in .scene"
                        );
                        bound["scene"] = json!(registered.replace('/', "\\"));
                    }
                    let item = &bound;
                    if item
                        .get("binding")
                        .and_then(Value::as_str)
                        .unwrap_or("direct")
                        == "direct"
                    {
                        nodes::conversation(item, reference, handles)?
                    } else {
                        let full = geometry::full_ref(prefab, reference);
                        let (model, path, resource) = conversation::build(item, name, &full)?;
                        ensure!(
                            conversation_artifacts.insert(path, resource).is_none(),
                            "duplicate conversation group path"
                        );
                        nodes::conversation(&model, reference, handles)?
                    }
                }
                "crowd_null_areas" => nodes::trigger(item, reference, handles, true)?,
                "devices" => {
                    if nodes::flag(item, "instance_data", true)? {
                        let id = item.get("buffer_id").and_then(Value::as_u64).unwrap_or(1);
                        ensure!(
                            id > 0 && buffers.insert(id),
                            "device instance buffers must have distinct positive IDs"
                        );
                    }
                    nodes::device(item, reference, handles)?
                }
                "loot_containers" => {
                    let mut item = item.clone();
                    let buffer = item.get("buffer_id").map_or_else(
                        || {
                            buffers
                                .iter()
                                .max()
                                .copied()
                                .unwrap_or(0)
                                .checked_add(1)
                                .context("instance buffer IDs exhausted")
                        },
                        |v| {
                            v.as_u64()
                                .context("loot buffer_id must be an unsigned integer")
                        },
                    )?;
                    ensure!(
                        buffer > 0 && buffers.insert(buffer),
                        "duplicate loot instance buffer"
                    );
                    item["buffer_id"] = json!(buffer);
                    loot::node(
                        &item,
                        reference,
                        name,
                        handles,
                        &mut conversation_artifacts,
                        &mut loot_records,
                    )?
                }
                _ => nodes::entity(item, reference, handles)?,
            };
            builder.add(reference, node, anchor, item.get("node_data"), placement)?;
            if key == "devices" {
                builder.device_entries.push((
                    geometry::full_ref(prefab, reference),
                    item.get("controller_class")
                        .and_then(Value::as_str)
                        .unwrap_or(if item["kind"] == "computer" {
                            "ComputerControllerPS"
                        } else {
                            "AccessPointControllerPS"
                        })
                        .into(),
                    anchor.position,
                ));
            }
        }
    }
    builder.communities(spec)?;
    security::add(
        &mut builder,
        spec,
        buffers.iter().max().copied().unwrap_or(0) + 1,
    )?;
    for key in ["ai_spots", "splines", "patrol_splines", "race_splines"] {
        for item in nodes::list(spec, key)? {
            let reference = item["ref"].as_str().context("activity/spline reference")?;
            let anchor = builder.anchor(item)?;
            let placement = Placement::read(item, Placement::Quest)?;
            let handles = &mut builder.sector_mut(placement).handles;
            let node = if key == "ai_spots" {
                nodes::spot(item, reference, handles)?
            } else {
                spline::build(item, reference, handles, key == "race_splines")?
            };
            builder
                .positions
                .extend(spline::bounds(&node, item, anchor)?);
            builder.add(reference, node, anchor, item.get("node_data"), placement)?;
        }
    }
    let mut aliases = HashSet::new();
    for item in nodes::list(spec, "always_loaded_node_refs")? {
        let (reference, model) = if let Some(reference) = item.as_str() {
            (reference, json!({}))
        } else {
            (
                item["ref"].as_str().context("always-loaded reference")?,
                item.clone(),
            )
        };
        let full = geometry::full_ref(prefab, reference);
        ensure!(
            aliases.insert(full.clone()) && !builder.always.references.contains(&full),
            "duplicate always-loaded alias {full}"
        );
        let anchor = if model.get("position").is_none() {
            let mut anchor = *geometry::lookup(&builder.anchors, &full)?;
            anchor.yaw = geometry::yaw(model.get("yaw"), &builder.anchors, anchor.yaw)?;
            anchor
        } else {
            builder.anchor(&model)?
        };
        let mut overrides =
            json!({"max_streaming_distance":9513.75586,"streaming_distance":512,"uk10":32});
        if let Some(extra) = model.get("node_data") {
            overrides
                .as_object_mut()
                .context("alias data")?
                .extend(extra.as_object().context("node data overrides")?.clone());
        }
        let index = model
            .get("node_index")
            .and_then(Value::as_u64)
            .map(usize::try_from)
            .transpose()?
            .or(anchor.index)
            .unwrap_or(builder.always.data.len());
        builder.always.data.push(node_data(
            index,
            native::node_ref(&full),
            anchor,
            Some(&overrides),
        )?);
        builder.always.references.push(full);
    }
    for reference in builder
        .quest
        .references
        .iter()
        .chain(&builder.always.references)
        .chain(&builder.exterior.references)
    {
        ensure!(
            !builder.registry_ids.contains(&node_ref_hash(reference)),
            "registry ID collides with world NodeRef {reference}"
        );
    }
    let quest_path = spec
        .get("quest_sector_path")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("mod\\{name}\\world\\quest.streamingsector"))
        .replace('/', "\\");
    let block_path = spec
        .get("block_path")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("mod\\{name}\\world\\all.streamingblock"))
        .replace('/', "\\");
    depot_path(&quest_path)?;
    depot_path(&block_path)?;
    let world = f64::from(f32::MAX);
    let (min, max) = if let Some(bounds) = spec.get("streaming_box").filter(|v| v.is_object()) {
        if bounds.get("min").is_some() && bounds.get("max").is_some() {
            (
                Position::read(&bounds["min"])?,
                Position::read(&bounds["max"])?,
            )
        } else {
            let padding = geometry::scalar(bounds, "padding", 300.)?;
            ensure!(
                padding >= 0.,
                "streaming bounds padding must be nonnegative"
            );
            let mut min = origin;
            let mut max = origin;
            for p in &builder.positions {
                min.x = min.x.min(p.x);
                min.y = min.y.min(p.y);
                min.z = min.z.min(p.z);
                max.x = max.x.max(p.x);
                max.y = max.y.max(p.y);
                max.z = max.z.max(p.z);
            }
            (
                min.offset(-padding, -padding, -padding),
                max.offset(padding, padding, padding),
            )
        }
    } else {
        ensure!(
            spec.get("streaming_box").is_none_or(|v| v == "world"),
            "streaming_box must be world or numeric bounds"
        );
        (
            Position {
                x: -world,
                y: -world,
                z: -world,
            },
            Position {
                x: world,
                y: world,
                z: world,
            },
        )
    };
    ensure!(
        min.x <= max.x && min.y <= max.y && min.z <= max.z,
        "invalid streaming bounds"
    );
    let mut descriptors = Vec::new();
    let mut artifacts = conversation_artifacts;
    if !builder.exterior.nodes.is_empty() || !builder.exterior.data.is_empty() {
        let path = spec
            .get("exterior_sector_path")
            .and_then(Value::as_str)
            .map_or_else(
                || format!("mod\\{name}\\world\\exterior.streamingsector"),
                |path| path.replace('/', "\\"),
            );
        depot_path(&path)?;
        let level = spec
            .get("exterior_sector_level")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        descriptors.push(descriptor("Exterior", &path, level, None, min, max));
        artifacts.insert(
            path.clone(),
            sector(&path, "Exterior", level, builder.exterior)?,
        );
    }
    let quest_level = spec
        .get("quest_sector_level")
        .and_then(Value::as_u64)
        .unwrap_or(255);
    if !builder.quest.nodes.is_empty() || !builder.quest.data.is_empty() {
        descriptors.push(descriptor("Quest", &quest_path, 0, Some(prefab), min, max));
        artifacts.insert(
            quest_path.clone(),
            sector(&quest_path, "Quest", quest_level, builder.quest)?,
        );
    }
    if !builder.always.nodes.is_empty() || !builder.always.data.is_empty() {
        let path = spec
            .get("always_loaded_sector_path")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("mod\\{name}\\world\\always_loaded.streamingsector"))
            .replace('/', "\\");
        depot_path(&path)?;
        let level = spec
            .get("always_loaded_level")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        descriptors.push(descriptor(
            "AlwaysLoaded",
            &path,
            level,
            None,
            Position {
                x: -world,
                y: -world,
                z: -world,
            },
            Position {
                x: world,
                y: world,
                z: world,
            },
        ));
        artifacts.insert(
            path.clone(),
            sector(&path, "AlwaysLoaded", level, builder.always)?,
        );
    }
    let mut registration = json!({"streaming":{"blocks":[block_path]}});
    let mut scene_replacements = BTreeMap::new();
    let mut replaced_scenes = HashSet::new();
    for item in nodes::list(spec, "conversations")? {
        if let Some(registered) = item.get("registered_scene").and_then(Value::as_str) {
            let scene = item["scene"]
                .as_str()
                .context("replacement scene path")?
                .replace('/', "\\");
            let registered = registered.replace('/', "\\");
            depot_path(&scene)?;
            ensure!(
                scene.ends_with(".scene") && !scene.eq_ignore_ascii_case(&registered),
                "registered_scene must differ from the generated scene path"
            );
            ensure!(
                replaced_scenes.insert(registered.to_ascii_lowercase()),
                "registered scene is replaced more than once"
            );
            scene_replacements.insert(registered, scene);
        }
    }
    let patches = patches::deletions(spec)?;
    if !patches.is_empty() {
        registration["streaming"]["sectors"] = json!(patches);
    }
    if !builder.device_entries.is_empty() {
        let path = spec
            .get("device_registry_path")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("mod\\{name}\\world\\custom_devices.devices"))
            .replace('/', "\\");
        depot_path(&path)?;
        artifacts.insert(
            path.clone(),
            document(&path, nodes::registry(&builder.device_entries)),
        );
        if registration.get("resource").is_none() {
            registration["resource"] = json!({});
        }
        registration["resource"]["patch"] = json!({path:["base\\worlds\\03_night_city\\_compiled\\default\\03_night_city.devices"]});
    }
    let mut resident_sectors = HashSet::new();
    for sector in nodes::list(spec, "always_loaded_sectors")? {
        let path = sector
            .as_str()
            .context("always_loaded_sectors entries must be depot paths")?
            .replace('/', "\\");
        depot_path(&path)?;
        ensure!(
            path.ends_with(".streamingsector"),
            "always-loaded sector reference must end in .streamingsector"
        );
        ensure!(
            resident_sectors.insert(path.to_ascii_lowercase())
                && !artifacts
                    .keys()
                    .any(|owned| owned.eq_ignore_ascii_case(&path)),
            "duplicate or owned always-loaded sector reference {path}"
        );
        descriptors.push(descriptor(
            "AlwaysLoaded",
            &path,
            0,
            None,
            Position {
                x: -world,
                y: -world,
                z: -world,
            },
            Position {
                x: world,
                y: world,
                z: world,
            },
        ));
    }
    artifacts.insert(block_path.clone(),document(&block_path,json!({"$type":"worldStreamingBlock","cookingPlatform":"PLATFORM_PC","descriptors":descriptors,"index":{"$type":"worldStreamingBlockIndex","oup":"Base","rldGridCell":0}})));
    let mut loose = BTreeMap::new();
    if !loot_records.is_empty() {
        loose.insert(
            format!("r6/tweaks/{name}_loot.yaml"),
            serde_yaml::to_string(&Value::Object(loot_records))?.into_bytes(),
        );
    }
    Ok(Build {
        artifacts,
        registration,
        loose,
        scene_replacements,
    })
}
