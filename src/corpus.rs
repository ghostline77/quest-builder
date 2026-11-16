//! Resumable native resource research indexes with explicit source provenance.
use anyhow::{Context, Result, ensure};
use ghostline_red::{archive, schema::RedSchema};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn supported(path: &Path) -> bool {
    path.extension().and_then(|v| v.to_str()).is_some_and(|e| {
        matches!(
            e.to_lowercase().as_str(),
            "json"
                | "questphase"
                | "scene"
                | "journal"
                | "ent"
                | "app"
                | "streamingblock"
                | "streamingsector"
                | "anims"
                | "lipmap"
                | "scenerid"
                | "devices"
        )
    })
}
fn files(root: &Path, out: &mut BTreeSet<PathBuf>) -> Result<()> {
    let metadata = fs::symlink_metadata(root)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "research traversal does not follow symlinks"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "research traversal does not follow reparse points"
        );
    }
    if metadata.is_dir() {
        let mut entries = fs::read_dir(root)?
            .map(|v| v.map(|v| v.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        entries.sort();
        for path in entries {
            files(&path, out)?;
        }
    } else if supported(root) || root.extension().is_some_and(|v| v == "archive") {
        out.insert(root.canonicalize()?);
    }
    Ok(())
}

/// Indexes explicit directories, resources or archives. Unchanged resources reuse summaries.
/// Decode failures are retained as searchable failures rather than silently omitted.
pub fn build(
    sources: &[PathBuf],
    schema: Option<&RedSchema>,
    previous: Option<&Value>,
    known_paths: &[String],
) -> Result<Value> {
    ensure!(!sources.is_empty(), "corpus needs explicit sources");
    let mut paths = BTreeSet::new();
    for source in sources {
        files(source, &mut paths)?;
    }
    let mut entries = BTreeMap::new();
    let mut reused = 0u64;
    let schema_fingerprint = schema.map(|s| crate::evidence::canonical_hash(&json!({"classes":s.classes,"enums":s.enums,"bitfields":s.bitfields,"aliases":s.aliases,"simple_types":s.simple_types}))).transpose()?;
    let decoder_fingerprint = crate::evidence::canonical_hash(
        &json!({"tool":crate::evidence::digest(&fs::read(std::env::current_exe()?)?),"schema":schema_fingerprint}),
    )?;
    let mut unresolved = BTreeMap::<String, Vec<String>>::new();
    let mut index = |key: String, bytes: Vec<u8>, provenance: Value| -> Result<()> {
        let hash = crate::evidence::digest(&bytes);
        let old = previous
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.get(&key));
        if let Some(old) = old.filter(|v| {
            v["sha256"] == hash
                && v["decoder_fingerprint"] == decoder_fingerprint
                && v["error"].is_null()
        }) {
            entries.insert(key, old.clone());
            reused += 1;
            return Ok(());
        }
        let decoded = crate::inspect::summarize_bytes(&bytes, schema);
        let (summary, error) = match decoded {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        entries.insert(key,json!({"sha256":hash,"decoder_version":env!("CARGO_PKG_VERSION"),"decoder_fingerprint":decoder_fingerprint,"provenance":provenance,"summary":summary,"error":error}));
        Ok(())
    };
    for path in paths {
        if path.extension().is_some_and(|v| v == "archive") {
            let native = archive::read_archive(&path)?;
            let names = native
                .custom_paths
                .iter()
                .chain(known_paths)
                .map(|p| (archive::depot_path_hash(p), p))
                .collect::<BTreeMap<_, _>>();
            for (i, entry) in native.entries.iter().enumerate() {
                let Some(depot) = names.get(&entry.name_hash) else {
                    unresolved
                        .entry(path.display().to_string())
                        .or_default()
                        .push(entry.name_hash.to_string());
                    continue;
                };
                if !supported(Path::new(depot)) {
                    continue;
                }
                let key = format!("{}::{depot}", path.display());
                match archive::extract_entry_bytes(&path, &native, i) {
                    Ok(bytes) => index(
                        key,
                        bytes,
                        json!({"archive":path,"depot":depot,"entry_hash":entry.name_hash.to_string()}),
                    )?,
                    Err(error) => {
                        index(
                            key,
                            Vec::new(),
                            json!({"archive":path,"depot":depot,"extraction_error":error.to_string()}),
                        )?;
                    }
                }
            }
        } else {
            index(
                path.display().to_string(),
                fs::read(&path)?,
                json!({"path":path}),
            )?;
        }
    }
    let failed = entries.values().filter(|v| !v["error"].is_null()).count();
    Ok(
        json!({"schema_version":1,"runtime_verified":false,"entries":entries,"unresolved_archive_hashes":unresolved,"counts":{"total":entries.len(),"failed":failed,"reused":reused,"unresolved_paths":unresolved.values().map(Vec::len).sum::<usize>()}}),
    )
}

/// Searches descriptions, resource/fact references, actor identities and decode errors.
pub fn query(index: &Value, text: &str, kind: Option<&str>, limit: usize) -> Result<Value> {
    ensure!(index["schema_version"] == 1, "unsupported corpus schema");
    let entries = index["entries"].as_object().context("corpus entries")?;
    let text = text.to_lowercase();
    let matches = entries
        .iter()
        .filter(|(_, v)| {
            kind.is_none_or(|kind| v["summary"]["kind"] == kind)
                && v.to_string().to_lowercase().contains(&text)
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"total":matches.len(),"returned":matches.len().min(limit),"matches":matches.into_iter().take(limit).map(|(key,entry)|json!({"source":key,"entry":entry})).collect::<Vec<_>>()}),
    )
}

/// Produces cross-resource fact and dependency indexes and cinematic event statistics.
pub fn analyze(index: &Value) -> Result<Value> {
    let mut kinds = BTreeMap::<String, u64>::new();
    let mut facts = BTreeMap::<String, Value>::new();
    let mut events = BTreeMap::<String, u64>::new();
    let mut dependencies = BTreeMap::new();
    let entries = index["entries"].as_object().context("corpus entries")?;
    let identity = |path: &str| path.replace('/', "\\").to_ascii_lowercase();
    let mut owners = BTreeMap::<String, Vec<String>>::new();
    for (source, entry) in entries {
        if let Some(depot) = entry["provenance"]["depot"]
            .as_str()
            .or_else(|| entry["summary"]["depot"].as_str())
        {
            owners
                .entry(identity(depot))
                .or_default()
                .push(source.clone());
        }
        for embedded in entry["summary"]["native"]["embedded"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(depot) = embedded["depot_path"].as_str().filter(|p| !p.is_empty()) {
                owners
                    .entry(identity(depot))
                    .or_default()
                    .push(source.clone());
            }
        }
    }
    let mut edges = BTreeMap::<String, BTreeSet<String>>::new();
    let mut missing = BTreeMap::<String, BTreeSet<String>>::new();
    for (source, entry) in index["entries"].as_object().context("corpus entries")? {
        let summary = &entry["summary"];
        if let Some(kind) = summary["kind"].as_str() {
            *kinds.entry(kind.into()).or_default() += 1;
        }
        for (field, role) in [("reads", "readers"), ("writes", "writers")] {
            for fact in summary["facts"][field].as_array().into_iter().flatten() {
                if let Some(fact) = fact.as_str() {
                    let references = facts
                        .entry(fact.into())
                        .or_insert(json!({"readers":[],"writers":[]}));
                    references[role]
                        .as_array_mut()
                        .context("fact graph")?
                        .push(json!(source));
                }
            }
        }
        for event in summary["events"].as_array().into_iter().flatten() {
            if let Some(kind) = event["type"].as_str() {
                *events.entry(kind.into()).or_default() += 1;
            }
        }
        dependencies.insert(source, summary["resources"].clone());
        edges.entry(source.clone()).or_default();
        for resource in summary["resources"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if let Some(targets) = owners.get(&identity(resource)) {
                for target in targets {
                    edges
                        .entry(source.clone())
                        .or_default()
                        .insert(target.clone());
                    edges
                        .entry(target.clone())
                        .or_default()
                        .insert(source.clone());
                }
            } else {
                missing
                    .entry(source.clone())
                    .or_default()
                    .insert(resource.into());
            }
        }
    }
    let mut handoffs = Vec::new();
    for (fact, usage) in &facts {
        for writer in usage["writers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            for reader in usage["readers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                handoffs.push(json!({"fact":fact,"writer":writer,"reader":reader}));
                edges
                    .entry(writer.into())
                    .or_default()
                    .insert(reader.into());
                edges
                    .entry(reader.into())
                    .or_default()
                    .insert(writer.into());
            }
        }
    }
    let mut visited = BTreeSet::new();
    let mut components = Vec::new();
    for source in entries.keys() {
        if visited.contains(source) {
            continue;
        }
        let mut pending = vec![source.clone()];
        let mut component = BTreeSet::new();
        while let Some(next) = pending.pop() {
            if !visited.insert(next.clone()) {
                continue;
            }
            component.insert(next.clone());
            if let Some(neighbors) = edges.get(&next) {
                pending.extend(neighbors.iter().cloned());
            }
        }
        components.push(component);
    }
    let failures = entries
        .iter()
        .filter(|(_, entry)| !entry["error"].is_null())
        .map(|(source, entry)| (source, entry["error"].clone()))
        .collect::<BTreeMap<_, _>>();
    Ok(
        json!({"resource_kinds":kinds,"facts":facts,"dependencies":dependencies,"resource_owners":owners,"unindexed_references":missing,"fact_handoffs":handoffs,"connected_components":components,"decode_failures":failures,"unresolved_archive_hashes":index["unresolved_archive_hashes"],"cinematic_event_kinds":events,"runtime_verified":false}),
    )
}

/// Re-extracts exact indexed source bytes and rejects changed resources before inspection.
pub fn extract(index: &Value, source: &str) -> Result<Vec<u8>> {
    ensure!(index["schema_version"] == 1, "unsupported corpus schema");
    let entry = index["entries"]
        .get(source)
        .context("unknown corpus source")?;
    ensure!(
        entry["error"].is_null(),
        "cannot extract a failed corpus entry"
    );
    let provenance = &entry["provenance"];
    let bytes = if let Some(path) = provenance["path"].as_str() {
        fs::read(path)?
    } else {
        let path = Path::new(provenance["archive"].as_str().context("source archive")?);
        let depot = provenance["depot"].as_str().context("source depot")?;
        let native = archive::read_archive(path)?;
        let hash = archive::depot_path_hash(depot);
        ensure!(
            provenance["entry_hash"]
                .as_str()
                .and_then(|value| value.parse::<u64>().ok())
                == Some(hash),
            "indexed archive identity changed"
        );
        let position = native
            .entries
            .iter()
            .position(|e| e.name_hash == hash)
            .context("source no longer exists in archive")?;
        archive::extract_entry_bytes(path, &native, position)?
    };
    ensure!(
        entry["sha256"] == crate::evidence::digest(&bytes),
        "indexed source bytes changed; refresh corpus before collecting evidence"
    );
    Ok(bytes)
}

/// Captures a source pointer and its exact value, bound to source and decoder identities.
pub fn evidence(
    index: &Value,
    source: &str,
    pointer: &str,
    schema: Option<&RedSchema>,
) -> Result<Value> {
    let bytes = extract(index, source)?;
    let doc = crate::inspect::decode(&bytes, schema)?;
    let value = doc
        .pointer(pointer)
        .context("source JSON pointer does not exist")?;
    let entry = &index["entries"][source];
    Ok(
        json!({"schema_version":1,"source":source,"source_sha256":entry["sha256"],"decoder_fingerprint":entry["decoder_fingerprint"],"pointer":pointer,"value":value,"value_sha256":crate::evidence::canonical_hash(value)?,"provenance":entry["provenance"],"runtime_verified":false}),
    )
}
