//! Standalone quest graph compilation and complete in-memory artifact collection.
use crate::{
    composition::Composition,
    graph::{self, Graph, NodeId},
    journal,
    manifest::{Manifest, Stage},
    nodes as n,
    project::depot_path,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashSet};

mod blocks;
mod start_gate;

/// Resource documents ready for conversion or an isolated raw build.
pub type Artifacts = BTreeMap<String, Value>;

/// Explicit resource inputs; none are resolved through an old repository registry.
#[derive(Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Inputs {
    /// Start through the native conversation bootstrap instead of quest registration.
    pub bootstrap: bool,
    /// Names from the owning series character registry, resolved before compilation.
    pub characters: Vec<String>,
    #[serde(skip)]
    pub required_loose: BTreeSet<String>,
    #[serde(skip)]
    pub character_bundles: BTreeSet<String>,
    /// Explicit root depot identity when the authoring ID differs from its namespace.
    pub root_resource: Option<String>,

    pub extra_resources: Artifacts,
    pub scenes: Vec<SceneInput>,
    pub worlds: Vec<Value>,
    pub atlases: Vec<Value>,
    pub font_families: Vec<Value>,
    pub websites: Vec<Value>,
    pub effects: Vec<Value>,
    pub workspots: Vec<Value>,
    pub lifecycles: Vec<Value>,
    pub localizations: Vec<LocalizationInput>,
    pub assets: BTreeMap<String, FileInput>,
    /// Native asset exports produced by the character, building or asset tools.
    pub archives: Vec<std::path::PathBuf>,
    /// Published animation, audio and texture inputs arranged by game depot path.
    pub media: Vec<std::path::PathBuf>,
    /// Authored TweakXL files mapped to their installed loose paths.
    pub tweaks: BTreeMap<String, std::path::PathBuf>,
    pub loose: BTreeMap<String, FileInput>,
}

/// An external artifact supplied by an authored source file.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FileInput {
    pub path: std::path::PathBuf,
}
impl FileInput {
    fn load(&self) -> Result<Vec<u8>> {
        std::fs::read(&self.path)
            .with_context(|| format!("external artifact source {}", self.path.display()))
    }
}

/// Scene declarations and dialogue are authoring inputs, not resource snapshots.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SceneInput {
    pub spec: Value,
    pub dialogue: Value,
}

/// Reviewed dialogue and explicit locale/output identities; media is supplied separately.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalizationInput {
    pub spec: Value,
    pub manifest: Value,
}

/// Complete native documents and loose registration files from one transaction.
#[derive(Default)]
pub struct Build {
    pub bootstrap: Option<crate::bootstrap::Spec>,
    pub required_loose: BTreeSet<String>,
    pub character_bundles: BTreeSet<String>,
    pub artifacts: Artifacts,
    pub resources: BTreeMap<String, Vec<u8>>,
    pub assets: BTreeMap<String, Vec<u8>>,
    pub dependencies: BTreeMap<String, BTreeSet<String>>,
    pub bindings: Vec<crate::audit::Binding>,
    pub authoring_sha256: String,
    pub integration_contracts: Vec<Value>,
}

/// Optional project-owned compilation logic, independent of Python hooks.
pub trait Extension {
    /// Generates a project-specific stage, or uses its registered builder.
    fn stage(&self, _stage: &Stage) -> Result<Option<Value>> {
        Ok(None)
    }
    /// Generates a project-specific orchestration root when required.
    fn root(&self, _manifest: &Manifest) -> Result<Option<Value>> {
        Ok(None)
    }
    /// Applies a project-owned stage transform before contract checking.
    fn transform(&self, _stage: &Stage, _document: &mut Value) -> Result<()> {
        Ok(())
    }
    /// Adds explicitly owned scene/world/device/animation resource documents.
    fn artifacts(&self, _manifest: &Manifest) -> Result<Artifacts> {
        Ok(BTreeMap::new())
    }
}

/// Reports actual built-in implementation availability, distinct from schema support.
pub fn has_builder(kind: &str) -> bool {
    blocks::supported(kind)
}

/// Compiles all resources first. An unsupported builder fails the whole build.
pub fn compile(
    manifest: &Manifest,
    composition: &Composition,
    inputs: &Inputs,
    extension: Option<&dyn Extension>,
    allow_planned: bool,
) -> Result<Build> {
    ensure!(
        inputs.characters.is_empty(),
        "resolve series character projects before compiling resources"
    );
    manifest.require_ready(allow_planned)?;
    let diagnostics = manifest.validate()?;
    ensure!(
        diagnostics.is_empty(),
        "invalid manifest: {}",
        serde_json::to_string(&diagnostics)?
    );
    let root_depot = inputs
        .root_resource
        .clone()
        .unwrap_or_else(|| format!("mod\\{}\\phases\\{}.questphase", manifest.id, manifest.id));
    depot_path(&root_depot)?;
    let custom_root = extension.map(|e| e.root(manifest)).transpose()?.flatten();
    let root = if let Some(root) = custom_root {
        graph::order_handles(&root)?
    } else {
        orchestrate(manifest, &root_depot)?
    };
    let mut artifacts = BTreeMap::from([(root_depot.clone(), root)]);
    let integration_contracts = Vec::new();
    let mut registration = json!({"quest":{"phases":[{"path":root_depot,"parent":"base\\quest\\cyberpunk2077.quest"}]}});
    for stage in &manifest.stages {
        let custom_stage = extension.map(|e| e.stage(stage)).transpose()?.flatten();
        let mut document = if let Some(value) = custom_stage {
            graph::order_handles(&value)?
        } else {
            compile_stage(stage)?
        };
        if let Some(extension) = extension {
            extension.transform(stage, &mut document)?;
        }
        let prefabs = if let Some(prefabs) = stage.fields.get("phase_prefabs") {
            prefabs.as_array().context("stage prefab array")?.clone()
        } else if stage
            .fields
            .get("inherit_phase_prefabs")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            manifest.phase_prefabs.iter().map(|p| json!(p)).collect()
        } else {
            Vec::new()
        };
        document["Data"]["RootChunk"]["phasePrefabs"]=json!(prefabs.iter().map(|p|json!({"$type":"questQuestPrefabEntry","prefabNodeRef":n::node_ref(p.as_str().unwrap_or_default())})).collect::<Vec<_>>());
        document = graph::order_handles(&document)?;
        validate_ports(stage, &document)?;
        let supporting = blocks::supporting(stage)?;
        crate::audit::typed_fields(
            stage,
            &json!({"stage":document,"supporting":supporting.iter().map(|(_,doc)|doc).collect::<Vec<_>>()}),
        )?;
        document["Header"]["ArchiveFileName"] = json!(stage.phase_resource);
        insert(&mut artifacts, stage.phase_resource.clone(), document)?;
        for (depot, mut document) in supporting {
            document["Data"]["RootChunk"]["phasePrefabs"]=json!(prefabs.iter().map(|p|json!({"$type":"questQuestPrefabEntry","prefabNodeRef":n::node_ref(p.as_str().unwrap_or_default())})).collect::<Vec<_>>());
            insert(&mut artifacts, depot, graph::order_handles(&document)?)?;
        }
    }
    for (depot, document) in journal::compose(composition)? {
        if document
            .pointer("/Data/RootChunk/$type")
            .and_then(Value::as_str)
            == Some("gameJournalResource")
        {
            merge_registration(&mut registration, &json!({"journal":[depot]}))?;
        } else if document
            .pointer("/Data/RootChunk/$type")
            .and_then(Value::as_str)
            == Some("JsonResource")
        {
            merge_registration(
                &mut registration,
                &json!({"localization":{"onscreens":{"en-us":[depot]}}}),
            )?;
        }
        insert(&mut artifacts, depot, document)?;
    }
    for input in &inputs.scenes {
        let document = crate::scene::compile(&input.spec, &input.dialogue)?;
        let depot = document
            .pointer("/Header/ArchiveFileName")
            .and_then(Value::as_str)
            .context("generated scene depot")?
            .to_owned();
        insert(&mut artifacts, depot, document)?;
    }
    for spec in &inputs.lifecycles {
        let document = crate::lifecycle::compile(spec)?;
        let depot = document["Header"]["ArchiveFileName"]
            .as_str()
            .context("lifecycle depot")?
            .to_owned();
        if spec.get("register").and_then(Value::as_bool) == Some(true) {
            merge_registration(
                &mut registration,
                &json!({"quest":{"phases":[{"path":depot,"parent":spec.get("parent").and_then(Value::as_str).unwrap_or("base\\quest\\cyberpunk2077.quest")}]}}),
            )?;
        }
        insert(&mut artifacts, depot, document)?;
    }

    let resolved_worlds = inputs
        .worlds
        .iter()
        .map(|spec| crate::composition::resolve(spec, &composition.bindings))
        .collect::<Result<Vec<_>>>()?;
    let mut world_loose = BTreeMap::new();
    let mut required_loose = inputs.required_loose.clone();
    for spec in &resolved_worlds {
        let world = crate::world::compile(spec)?;
        for (path, bytes) in world.loose {
            required_loose.insert(path.clone());
            ensure!(
                world_loose.insert(path.clone(), bytes).is_none(),
                "generated world tweak collision {path}"
            );
        }
        for (depot, document) in world.artifacts {
            insert(&mut artifacts, depot, document)?;
        }
        crate::conversation_metadata::bind(&mut artifacts, &world.scene_replacements)?;
        merge_registration(&mut registration, &world.registration)?;
    }
    for input in &inputs.localizations {
        let localization = crate::localization::compile(&input.spec, &input.manifest)?;
        for (depot, document) in localization.artifacts {
            insert(&mut artifacts, depot, document)?;
        }
        merge_registration(&mut registration, &localization.registration)?;
    }
    for spec in &inputs.atlases {
        let spec = crate::composition::resolve(spec, &composition.bindings)?;
        let document = crate::ui::atlas(&spec)?;
        insert(
            &mut artifacts,
            spec["depot"].as_str().context("atlas depot")?.into(),
            document,
        )?;
    }
    for spec in &inputs.font_families {
        let spec = crate::composition::resolve(spec, &composition.bindings)?;
        let document = crate::ui::font_family(&spec)?;
        insert(
            &mut artifacts,
            spec["depot"].as_str().context("font family depot")?.into(),
            document,
        )?;
    }
    for spec in &inputs.websites {
        let website =
            crate::ui::website(&crate::composition::resolve(spec, &composition.bindings)?)?;
        for (depot, document) in website.artifacts {
            insert(&mut artifacts, depot, document)?;
        }
        merge_registration(&mut registration, &website.registration)?;
    }
    for spec in &inputs.effects {
        let build = crate::fx::compile(&crate::composition::resolve(spec, &composition.bindings)?)?;
        for (depot, document) in build.artifacts {
            insert(&mut artifacts, depot, document)?;
        }
        merge_registration(&mut registration, &build.registration)?;
    }
    for spec in &inputs.workspots {
        let document =
            crate::workspot::compile(&crate::composition::resolve(spec, &composition.bindings)?)?;
        let depot = document["Header"]["ArchiveFileName"]
            .as_str()
            .context("workspot depot")?
            .to_owned();
        insert(&mut artifacts, depot, document)?;
    }
    for (depot, document) in &inputs.extra_resources {
        insert(&mut artifacts, depot.clone(), document.clone())?;
    }
    if let Some(extension) = extension {
        for (depot, document) in extension.artifacts(manifest)? {
            insert(&mut artifacts, depot, document)?;
        }
    }
    let lipmaps = crate::localization::lipsync_maps(&artifacts, "en-us")?;
    for (depot, document) in lipmaps.artifacts {
        insert(&mut artifacts, depot, document)?;
    }
    merge_registration(&mut registration, &lipmaps.registration)?;
    crate::audit::bound_interfaces(&artifacts)?;
    let bindings = crate::audit::bindings(manifest)?;
    crate::audit::declared_interfaces(&bindings, &artifacts)?;
    let mut resources = BTreeMap::from([(
        format!("{}.archive.xl", manifest.id),
        serde_json::to_vec_pretty(&registration)?,
    )]);
    resources.extend(world_loose);
    let mut assets = BTreeMap::new();
    for path in &inputs.archives {
        collect_archive(path, &mut assets)?;
    }
    for (destination, path) in &inputs.tweaks {
        depot_path(&format!("mod\\{}", destination.replace('/', "\\")))?;
        let bytes =
            std::fs::read(path).with_context(|| format!("TweakXL source {}", path.display()))?;
        let parsed: serde_yaml::Value = serde_yaml::from_slice(&bytes)?;
        ensure!(parsed.is_mapping(), "TweakXL source must contain records");
        ensure!(
            resources.insert(destination.clone(), bytes).is_none(),
            "TweakXL output collision {destination}"
        );
    }
    for root in &inputs.media {
        collect_media(root, root, &mut assets)?;
    }
    for (depot, input) in &inputs.assets {
        depot_path(depot)?;
        ensure!(
            !artifacts.keys().any(|key| key.eq_ignore_ascii_case(depot))
                && !assets
                    .keys()
                    .any(|key: &String| key.eq_ignore_ascii_case(depot)),
            "external artifact collides with generated resource {depot}"
        );
        ensure!(
            assets.insert(depot.clone(), input.load()?).is_none(),
            "published asset collision {depot}"
        );
    }
    for (path, input) in &inputs.loose {
        depot_path(&format!("mod\\{}", path.replace('/', "\\")))?;
        let bytes = input.load()?;
        if path.ends_with(".archive.xl") {
            let extra: Value = serde_yaml::from_slice(&bytes)?;
            merge_registration(&mut registration, &extra)?;
        } else {
            ensure!(
                !resources.keys().any(|key| key
                    .replace('\\', "/")
                    .eq_ignore_ascii_case(&path.replace('\\', "/"))),
                "loose resource collision {path}"
            );
            ensure!(
                resources.insert(path.clone(), bytes).is_none(),
                "loose resource collision {path}"
            );
        }
    }
    resources.insert(
        format!("{}.archive.xl", manifest.id),
        serde_json::to_vec_pretty(&registration)?,
    );
    crate::audit::encounters(manifest, &resolved_worlds, &resources)?;
    let mut dependencies = BTreeMap::new();
    for stage in &manifest.stages {
        let mut required = BTreeSet::new();
        for asset in stage
            .fields
            .get("required_assets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let asset = asset.as_str().context("required asset depot")?;
            depot_path(asset)?;
            required.insert(asset.into());
        }
        dependencies.insert(stage.phase_resource.clone(), required);
    }
    let media_hashes = assets
        .iter()
        .map(|(path, bytes)| (path, crate::evidence::digest(bytes)))
        .collect::<BTreeMap<_, _>>();
    let authoring_sha256 = crate::evidence::canonical_hash(
        &json!({"manifest":manifest,"inputs":inputs,"media":media_hashes,"loose":resources.iter().map(|(path,bytes)|(path,crate::evidence::digest(bytes))).collect::<BTreeMap<_,_>>(),"composition":{"bindings":composition.bindings,"text":composition.text,"configuration":composition.configuration}}),
    )?;
    let mut build = Build {
        bootstrap: None,
        required_loose,
        character_bundles: inputs.character_bundles.clone(),
        artifacts,
        resources,
        assets,
        dependencies,
        bindings,
        authoring_sha256,
        integration_contracts,
    };
    if inputs.bootstrap {
        crate::bootstrap::attach(
            &mut build,
            crate::bootstrap::Spec::new(
                manifest.id.clone(),
                vec![crate::bootstrap::Phase {
                    resource: root_depot,
                    input: "In1".into(),
                    output: Some("Out1".into()),
                }],
            ),
        )?;
    }
    Ok(build)
}

fn collect_archive(path: &std::path::Path, assets: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
    use ghostline_red::archive;
    let index = archive::read_archive(path)
        .with_context(|| format!("native asset export {}", path.display()))?;
    let mut paths = BTreeMap::new();
    for source in archive::read_archive_paths(path)? {
        let source = source.replace('/', "\\").to_ascii_lowercase();
        depot_path(&source)?;
        ensure!(
            source.starts_with("mod\\"),
            "asset exports must own their mod namespace: {source}"
        );
        let extension = source.rsplit('.').next().unwrap_or_default();
        ensure!(
            !["questphase", "scene", "journal", "lipmap"].contains(&extension),
            "quest resources must be generated by the compiler: {source}"
        );
        ensure!(
            paths
                .insert(archive::depot_path_hash(&source), source)
                .is_none(),
            "duplicate export depot identity"
        );
    }
    ensure!(!index.entries.is_empty(), "empty native asset export");
    for (entry_index, entry) in index.entries.iter().enumerate() {
        let depot = paths
            .get(&entry.name_hash)
            .context("asset export is missing a depot identity")?;
        let bytes = archive::extract_entry_bytes(path, &index, entry_index)?;
        if let Some(previous) = assets.get(depot) {
            ensure!(
                previous == &bytes,
                "conflicting native asset exports for {depot}"
            );
        } else {
            assets.insert(depot.clone(), bytes);
        }
    }
    Ok(())
}

fn collect_media(
    root: &std::path::Path,
    directory: &std::path::Path,
    assets: &mut BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("published media directory {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        ensure!(!kind.is_symlink(), "published media cannot contain links");
        if kind.is_dir() {
            collect_media(root, &path, assets)?;
            continue;
        }
        ensure!(kind.is_file(), "unsupported published media entry");
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        ensure!(
            ["wem", "anims", "xbm", "ttf", "fnt"].contains(&extension),
            "unsupported published media file {}",
            path.display()
        );
        let depot = path
            .strip_prefix(root)?
            .to_str()
            .context("media depot encoding")?
            .replace('/', "\\")
            .to_lowercase();
        depot_path(&depot)?;
        let bytes = std::fs::read(&path)?;
        if let Some(previous) = assets.get(&depot) {
            ensure!(previous == &bytes, "conflicting published media {depot}");
        } else {
            assets.insert(depot, bytes);
        }
    }
    Ok(())
}

fn merge_registration(target: &mut Value, source: &Value) -> Result<()> {
    match (target, source) {
        (Value::Object(target), Value::Object(source)) => {
            for (key, value) in source {
                if let Some(existing) = target.get_mut(key) {
                    merge_registration(existing, value)?;
                } else {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
        (Value::Array(target), Value::Array(source)) => {
            for value in source {
                ensure!(
                    !target.contains(value),
                    "duplicate ArchiveXL registration {value}"
                );
                target.push(value.clone());
            }
        }
        (target, source) => ensure!(
            target == source,
            "conflicting ArchiveXL registration values"
        ),
    }
    Ok(())
}

/// Generates one stage directly from its typed fields.
/// This API does not infer world, actor or scene bindings from a quest series.
pub fn compile_stage(stage: &Stage) -> Result<Value> {
    ensure!(
        !stage.fields.contains_key("phase_template")
            && !stage.fields.contains_key("template_bindings"),
        "stage {} must declare generated behaviour directly",
        stage.id
    );
    let document = blocks::build(stage)?;
    validate_ports(stage, &document)?;
    let supporting = blocks::supporting(stage)?;
    crate::audit::typed_fields(
        stage,
        &json!({"stage":document,"supporting":supporting.iter().map(|(_,doc)|doc).collect::<Vec<_>>()}),
    )?;
    crate::audit::Phase::read(&document)?.contracts(stage)?;
    Ok(document)
}

fn insert(artifacts: &mut Artifacts, depot: String, document: Value) -> Result<()> {
    depot_path(&depot)?;
    ensure!(
        !artifacts.keys().any(|key| key.eq_ignore_ascii_case(&depot)),
        "duplicate artifact {depot}"
    );
    ensure!(
        artifacts.insert(depot.clone(), document).is_none(),
        "duplicate artifact {depot}"
    );
    Ok(())
}

/// Checks each declared stage entry/outcome against actual native nodes.
pub fn validate_ports(stage: &Stage, document: &Value) -> Result<()> {
    fn walk(v: &Value, inputs: &mut HashSet<String>, outputs: &mut HashSet<String>) {
        if let Some(name) = v
            .get("socketName")
            .and_then(|n| n.get("$value"))
            .and_then(Value::as_str)
        {
            match v.get("$type").and_then(Value::as_str) {
                Some("questInputNodeDefinition") => {
                    inputs.insert(name.to_owned());
                }
                Some("questOutputNodeDefinition") => {
                    outputs.insert(name.to_owned());
                }
                _ => {}
            }
        }
        match v {
            Value::Object(v) => {
                for v in v.values() {
                    walk(v, inputs, outputs)
                }
            }
            Value::Array(v) => {
                for v in v {
                    walk(v, inputs, outputs)
                }
            }
            _ => {}
        }
    }
    ensure!(
        document
            .pointer("/Data/RootChunk/$type")
            .and_then(Value::as_str)
            == Some("questQuestPhaseResource"),
        "stage {} did not produce a quest phase",
        stage.id
    );
    let mut inputs = HashSet::new();
    let mut outputs = HashSet::new();
    walk(document, &mut inputs, &mut outputs);
    for name in stage.inputs()?.values() {
        ensure!(
            inputs.contains(name),
            "stage {} declares missing entry {name}",
            stage.id
        );
    }
    for name in stage.outcomes()?.values() {
        ensure!(
            outputs.contains(name),
            "stage {} declares missing exit {name}",
            stage.id
        );
    }
    Ok(())
}

#[derive(Clone)]
struct Entry {
    node: NodeId,
    socket: String,
}

fn prefix(
    g: &mut Graph,
    s: &Stage,
    manifest: &Manifest,
    index: usize,
    next: &mut u32,
) -> Result<Vec<Entry>> {
    let mut result = Vec::new();
    macro_rules! add {
        ($node:expr,$socket:expr) => {{
            let node = $node?;
            *next += 1;
            result.push(Entry {
                node,
                socket: $socket.into(),
            });
        }};
    }
    if s.kind == "meet_contact" {
        if let Some(path) = s.optional("opening_message") {
            add!(
                n::journal(g, *next, path, "gameJournalPhoneMessage", 1),
                "Active"
            );
        }
        if s.fields
            .get("objective_lifecycle")
            .and_then(|v| v.get("on_enter"))
            .and_then(Value::as_str)
            != Some("retain")
        {
            add!(
                n::journal(
                    g,
                    *next,
                    s.string("objective")?,
                    "gameJournalQuestObjective",
                    2
                ),
                "Active"
            );
        }
        add!(
            n::journal(
                g,
                *next,
                s.string("description_entry")?,
                "gameJournalQuestDescription",
                2
            ),
            "Active"
        );
        add!(n::mappin(g, *next, s.string("mappin")?, true), "Active");
    }
    if let Some(name) = s.optional("checkpoint") {
        add!(
            n::checkpoint(g, *next, name, s.flag("retry_checkpoint")),
            "In"
        );
    }
    if let Some(name) = &manifest.debug_fact {
        add!(
            n::fact(g, *next, name, i64::try_from(index + 1)? * 10),
            "In"
        );
    }
    Ok(result)
}

/// Builds sequential, explicit branch/loop or parallel all-of root orchestration.
pub fn orchestrate(manifest: &Manifest, depot: &str) -> Result<Value> {
    let mut g = Graph::default();
    let mut start = n::input(&mut g, "In1")?;
    let end = n::output(&mut g, 1, "Out1")?;
    let mut next = u32::try_from(manifest.stages.len())? + 10;
    if let Some(gate) = manifest.start_gate {
        let wait = start_gate::build(&mut g, next, gate)?;
        next += 1;
        g.then(start, wait)?;
        start = wait;
    }
    if let Some(condition) = &manifest.start_condition {
        let wait = n::fact_condition(
            &mut g,
            next,
            condition["fact"].as_str().context("start fact")?,
            condition["comparison"]
                .as_str()
                .context("start comparison")?,
            condition["value"].as_i64().context("start fact value")?,
            false,
        )?;
        next += 1;
        g.then(start, wait)?;
        start = wait;
    }
    let mut entries = BTreeMap::new();
    let mut phases = BTreeMap::new();
    for (index, s) in manifest.stages.iter().enumerate() {
        let inputs = s.inputs()?;
        let outputs = s.outcomes()?;
        let mut chain = prefix(&mut g, s, manifest, index, &mut next)?;
        let phase = n::phase(
            &mut g,
            u32::try_from(index)? + 10,
            &s.phase_resource,
            &inputs.values().map(String::as_str).collect::<Vec<_>>(),
            &outputs.values().map(String::as_str).collect::<Vec<_>>(),
        )?;
        chain.push(Entry {
            node: phase,
            socket: inputs.get("start").context("stage start input")?.clone(),
        });
        for pair in chain.windows(2) {
            g.connect(pair[0].node, "Out", pair[1].node, &pair[1].socket)?;
        }
        entries.insert(s.id.as_str(), chain[0].clone());
        phases.insert(s.id.as_str(), phase);
    }
    if !manifest.parallel_groups.is_empty() {
        let mut tail = start;
        let mut output = "Out".to_owned();
        let mut index = 0;
        while index < manifest.stages.len() {
            let group = manifest.parallel_groups.iter().find(|group| {
                group
                    .branches
                    .iter()
                    .flatten()
                    .filter_map(|id| manifest.stages.iter().position(|s| &s.id == id))
                    .min()
                    == Some(index)
            });
            if let Some(group) = group {
                let join = n::join(&mut g, next, group.branches.len(), true)?;
                next += 1;
                for (branch_index, branch) in group.branches.iter().enumerate() {
                    let mut from = tail;
                    let mut port = output.clone();
                    for name in branch {
                        let entry = entries.get(name.as_str()).context("parallel stage entry")?;
                        g.connect(from, &port, entry.node, &entry.socket)?;
                        from = phases[name.as_str()];
                        port = "Out1".into();
                    }
                    g.connect(from, &port, join, &format!("In{}", branch_index + 1))?;
                }
                tail = join;
                output = "Out1".into();
                index = group
                    .branches
                    .iter()
                    .flatten()
                    .filter_map(|id| manifest.stages.iter().position(|s| &s.id == id))
                    .max()
                    .context("parallel stages")?
                    + 1;
            } else {
                let s = &manifest.stages[index];
                let entry = &entries[s.id.as_str()];
                g.connect(tail, &output, entry.node, &entry.socket)?;
                tail = phases[s.id.as_str()];
                output = s
                    .outcomes()?
                    .get("success")
                    .context("sequential success output")?
                    .clone();
                index += 1;
            }
        }
        if let Some(policy) = &manifest.completion {
            let success = policy["outcomes"]
                .get("success")
                .context("parallel completion requires a success policy")?;
            let finish = n::objective(
                &mut g,
                next,
                policy["quest_path"]
                    .as_str()
                    .context("completion quest path")?,
                true,
            )?;
            next += 1;
            g.connect(
                tail,
                &output,
                finish,
                success["state"].as_str().context("completion state")?,
            )?;
            tail = finish;
            output = "Out".into();
            if let Some(fact) = success.get("fact").and_then(Value::as_str) {
                let committed = n::fact(&mut g, next, fact, 1)?;
                g.then(tail, committed)?;
                tail = committed;
            }
        }
        g.connect(tail, &output, end, "In")?;
    } else {
        let terminal_join = if manifest.forks.is_empty() { None } else {
            let join = n::join(&mut g, next, manifest.forks.len() + 1, true)?;
            next += 1;
            g.connect(join, "Out1", end, "In")?;
            Some(join)
        };
        for (index, fork) in manifest.forks.iter().enumerate() {
            let source = manifest.stages.iter().find(|s| s.id == fork.stage).context("fork source")?;
            let entry = entries.get(fork.start.as_str()).context("fork entry")?;
            g.connect(phases[fork.stage.as_str()], &source.outcomes()?[&fork.outcome], entry.node, &entry.socket)?;
            let complete = manifest.stages.iter().find(|s| s.id == fork.complete).context("fork completion")?;
            g.connect(phases[fork.complete.as_str()], &complete.outcomes()?["success"], terminal_join.context("fork join")?, &format!("In{}", index + 2))?;
        }
        let entry = manifest
            .entry_stage
            .as_deref()
            .unwrap_or(&manifest.stages[0].id);
        let first = &entries[entry];
        g.connect(start, "Out", first.node, &first.socket)?;
        let mut terminals = BTreeMap::new();
        if let Some(policy) = &manifest.completion {
            for (outcome, policy) in policy["outcomes"]
                .as_object()
                .context("completion policy")?
            {
                let finish = n::objective(
                    &mut g,
                    next,
                    manifest
                        .completion
                        .as_ref()
                        .and_then(|v| v["quest_path"].as_str())
                        .context("completion journal path")?,
                    true,
                )?;
                next += 1;
                let mut tail = finish;
                if let Some(fact) = policy.get("fact").and_then(Value::as_str) {
                    tail = n::fact(&mut g, next, fact, 1)?;
                    next += 1;
                    g.then(finish, tail)?;
                }
                g.then(tail, end)?;
                terminals.insert(
                    outcome.clone(),
                    Entry {
                        node: finish,
                        socket: policy["state"].as_str().context("completion state")?.into(),
                    },
                );
            }
        }
        for s in &manifest.stages {
            let ports = s.outcomes()?;
            for (outcome, target) in &manifest.routes()?[&s.id] {
                if target == "$end" && outcome == "success" && manifest.forks.iter().any(|fork| fork.complete == s.id) {
                    continue;
                }
                let entry = if target == "$end" {
                    terminals.get(outcome).cloned().unwrap_or(Entry {
                        node: terminal_join.unwrap_or(end),
                        socket: if terminal_join.is_some() { "In1".into() } else { "In".into() },
                    })
                } else {
                    entries
                        .get(target.as_str())
                        .with_context(|| format!("unknown route {target}"))?
                        .clone()
                };
                g.connect(
                    phases[s.id.as_str()],
                    &ports[outcome],
                    entry.node,
                    &entry.socket,
                )?;
            }
        }
    }
    g.document(depot, &manifest.phase_prefabs)
}

/// Converts an unsupported stage into an actionable error, never an empty phase.
fn unsupported(stage: &Stage) -> anyhow::Error {
    anyhow::anyhow!(
        "unknown native stage builder {} ({}); no resource was published",
        stage.kind,
        stage.id
    )
}

/// Publishes a complete raw build by renaming a staged directory into place.
/// Existing output is rejected so no authored input is silently overwritten.
pub fn publish_raw(output: &std::path::Path, artifacts: &Artifacts, prototype: bool) -> Result<()> {
    publish(output, artifacts, &BTreeMap::new(), prototype)
}

/// Atomically publishes documents and their complete verified native payload set.
pub fn publish(
    output: &std::path::Path,
    artifacts: &Artifacts,
    binaries: &crate::native::Binaries,
    prototype: bool,
) -> Result<()> {
    publish_complete(output, artifacts, binaries, &BTreeMap::new(), prototype)
}

/// Publishes generated native resources and their required loose registrations together.
pub fn publish_complete(
    output: &std::path::Path,
    artifacts: &Artifacts,
    binaries: &crate::native::Binaries,
    resources: &BTreeMap<String, Vec<u8>>,
    prototype: bool,
) -> Result<()> {
    publish_transaction(
        output,
        artifacts,
        binaries,
        resources,
        prototype,
        &json!({"binary_assets":{},"declared_dependencies":{}}),
    )
}

/// Publishes generated resources and frozen external outputs in a single transaction.
pub fn publish_build(
    output: &std::path::Path,
    build: &Build,
    binaries: &crate::native::Binaries,
    prototype: bool,
) -> Result<()> {
    ensure!(
        binaries.is_empty() || binaries.keys().eq(build.artifacts.keys()),
        "generated native payload set is incomplete"
    );
    let mut all = binaries.clone();
    for (path, bytes) in &build.assets {
        ensure!(
            all.insert(path.clone(), bytes.clone()).is_none(),
            "binary artifact collision {path}"
        );
    }
    let asset_hashes = build
        .assets
        .iter()
        .map(|(path, bytes)| (path, crate::evidence::digest(bytes)))
        .collect::<BTreeMap<_, _>>();
    publish_transaction(
        output,
        &build.artifacts,
        &all,
        &build.resources,
        prototype,
        &json!({"binary_assets":asset_hashes,"declared_dependencies":build.dependencies,"authoring_sha256":build.authoring_sha256,"integration_contracts":build.integration_contracts,"bootstrap":build.bootstrap,"required_loose":build.required_loose,"character_bundles":build.character_bundles}),
    )
}

fn publish_transaction(
    output: &std::path::Path,
    artifacts: &Artifacts,
    binaries: &crate::native::Binaries,
    resources: &BTreeMap<String, Vec<u8>>,
    prototype: bool,
    metadata: &Value,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::{fs, path::Path};
    ensure!(
        !output.exists(),
        "build destination already exists: {}",
        output.display()
    );
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    ensure!(
        binaries.is_empty()
            || (artifacts.keys().all(|k| binaries.contains_key(k))
                || binaries
                    .keys()
                    .all(|k| metadata["binary_assets"].get(k).is_some()))
                && binaries.keys().all(
                    |k| artifacts.contains_key(k) || metadata["binary_assets"].get(k).is_some()
                ),
        "native payload set must exactly match documents"
    );
    let stage = tempfile::Builder::new()
        .prefix(".quest-build-")
        .tempdir_in(parent)?;
    let mut hashes = BTreeMap::new();
    let mut binary_hashes = BTreeMap::new();
    let mut resource_hashes = BTreeMap::new();
    for (depot, document) in artifacts {
        let relative = depot_path(depot)?;
        let mut relative = relative.into_os_string();
        relative.push(".json");
        let path = stage.path().join("source/raw").join(relative);
        fs::create_dir_all(path.parent().context("resource parent")?)?;
        let bytes = serde_json::to_vec_pretty(document)?;
        hashes.insert(depot, format!("{:x}", Sha256::digest(&bytes)));
        fs::write(path, bytes)?;
    }
    for (depot, bytes) in binaries {
        let path = stage.path().join("source/archive").join(depot_path(depot)?);
        fs::create_dir_all(path.parent().context("binary parent")?)?;
        fs::write(path, bytes)?;
        binary_hashes.insert(depot, format!("{:x}", Sha256::digest(bytes)));
    }
    for (name, bytes) in resources {
        let relative = depot_path(&format!("mod\\{}", name.replace('/', "\\")))?;
        let relative = relative
            .strip_prefix("mod")
            .context("loose resource path")?;
        let path = stage.path().join("source/resources").join(relative);
        fs::create_dir_all(path.parent().context("registration parent")?)?;
        fs::write(path, bytes)?;
        resource_hashes.insert(name, format!("{:x}", Sha256::digest(bytes)));
    }
    fs::write(
        stage.path().join("build.json"),
        serde_json::to_vec_pretty(
            &json!({"schema_version":1,"prototype":prototype,"native_serialized":!binaries.is_empty()&&artifacts.keys().all(|k|binaries.contains_key(k)),"runtime_tested":false,"resources":hashes,"binaries":binary_hashes,"loose_resources":resource_hashes,"binary_assets":metadata["binary_assets"],"declared_dependencies":metadata["declared_dependencies"],"authoring_sha256":metadata["authoring_sha256"],"integration_contracts":metadata["integration_contracts"],"bootstrap":metadata["bootstrap"],"required_loose":metadata["required_loose"],"character_bundles":metadata["character_bundles"],"generator":{"version":env!("CARGO_PKG_VERSION"),"sha256":crate::evidence::digest(&fs::read(std::env::current_exe()?)?)}}),
        )?,
    )?;
    crate::publication::rename_new(stage.path(), output)
        .with_context(|| format!("publish complete build to {}", output.display()))?;
    Ok(())
}
