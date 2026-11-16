//! Native CR2W conversion with structural and requested-value readback checks.
use crate::compiler::Artifacts;
use anyhow::{Context, Result, ensure};
use ghostline_red::{codec, schema::RedSchema, writer};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
};

/// Compiled native resource payloads, keyed by exact game depot identity.
pub type Binaries = BTreeMap<String, Vec<u8>>;

/// Audits published binary inputs as interfaces; they never become generation bases.
pub fn encode_build(
    build: &crate::compiler::Build,
    schema: &RedSchema,
    game: Option<&std::path::Path>,
) -> Result<Binaries> {
    crate::conversation_metadata::validate_build(build, schema, game)?;
    let mut missing_assets = Vec::new();
    for (depot, document) in &build.artifacts {
        let mut references = std::collections::BTreeSet::new();
        resource_paths(&document["Data"], &mut references);
        for embedded in document["Data"]["EmbeddedFiles"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(path) = embedded["FileName"]["$value"].as_str() {
                references.remove(&path.replace('/', "\\").to_ascii_lowercase());
            }
        }
        for path in references {
            if path.starts_with("mod\\")
                && !build
                    .artifacts
                    .keys()
                    .chain(build.assets.keys())
                    .any(|key| key.eq_ignore_ascii_case(&path))
            {
                missing_assets.push(format!("{depot} -> {path}"));
            }
        }
    }
    ensure!(
        missing_assets.is_empty(),
        "missing owned resource inputs:\n{}",
        missing_assets.join("\n")
    );
    let mut documents = build.artifacts.clone();
    for (depot, bytes) in &build.assets {
        if depot.ends_with(".scene") || depot.ends_with(".questphase") {
            documents.insert(depot.clone(), crate::inspect::decode(bytes, Some(schema))?);
        }
    }
    let references = crate::audit::called_resources(&documents)?;
    let missing = references
        .iter()
        .filter(|p| !documents.keys().any(|k| k.eq_ignore_ascii_case(p)))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        for depot in &missing {
            let path = depot.replace('/', "\\").to_ascii_lowercase();
            ensure!(
                path.starts_with("base\\") || path.starts_with("ep1\\"),
                "called mod resource {depot} needs an explicitly supplied published artifact; installed mod outputs are not build inputs"
            );
        }
        let archives = game
            .map(|root| ghostline_red::material::ArchiveSet::open(&root.join("archive/pc")))
            .transpose()?;
        for depot in missing {
            let archives = archives.as_ref().with_context(||format!("called resource {depot} needs a published artifact or --game for native interface inspection"))?;
            let bytes = archives
                .read_resource(&depot)
                .with_context(|| format!("read called resource {depot}"))?;
            documents.insert(depot, crate::inspect::decode(&bytes, Some(schema))?);
        }
    }
    crate::audit::bound_interfaces(&documents)?;
    crate::audit::declared_interfaces(&build.bindings, &documents)?;
    let mut overrides = Binaries::new();
    if let Some(bootstrap) = &build.bootstrap {
        for (path, bytes) in crate::bootstrap::encode(
            game.context("native quest bootstrap requires --game")?,
            bootstrap,
            schema,
        )? {
            ensure!(
                !build.assets.contains_key(&path) && !build.artifacts.contains_key(&path) && overrides.insert(path.clone(), bytes).is_none(),
                "bootstrap native override collision {path}"
            );
        }
    }
    let mut binaries = encode(&build.artifacts, schema)?;
    binaries.extend(overrides);
    Ok(binaries)
}

fn resource_paths(value: &Value, paths: &mut std::collections::BTreeSet<String>) {
    if value["$type"] == "ResourcePath"
        && let Some(path) = value["$value"].as_str()
    {
        paths.insert(path.replace('/', "\\").to_ascii_lowercase());
    }
    match value {
        Value::Object(object) => {
            for child in object.values() {
                resource_paths(child, paths)
            }
        }
        Value::Array(array) => {
            for child in array {
                resource_paths(child, paths)
            }
        }
        _ => {}
    }
}

/// Converts all documents before any output directory is published.
pub fn encode(artifacts: &Artifacts, schema: &RedSchema) -> Result<Binaries> {
    let work = tempfile::tempdir()?;
    let mut result = BTreeMap::new();
    let mut failures = Vec::new();
    for (index, (depot, document)) in artifacts.iter().enumerate() {
        let converted = (|| -> Result<Vec<u8>> {
            let raw = work.path().join(format!("{index}.json"));
            let output = work.path().join(format!("{index}.cr2w"));
            fs::write(&raw, serde_json::to_vec(document)?)?;
            writer::write(&raw, &output, schema)
                .with_context(|| format!("native write {depot}"))?;
            let bytes = fs::read(&output)?;
            let decoded = codec::decode_wkit_from_bytes(&bytes, schema)
                .with_context(|| format!("native readback {depot}"))?;
            verify(document, &decoded)
                .with_context(|| format!("native semantic readback {depot}"))?;
            Ok(bytes)
        })();
        match converted {
            Ok(bytes) => {
                result.insert(depot.clone(), bytes);
            }
            Err(error) => failures.push(format!("{error:#}")),
        }
    }
    ensure!(
        failures.is_empty(),
        "native conversion failed:\n{}",
        failures.join("\n")
    );
    Ok(result)
}

fn canonical(value: &Value) -> Result<Value> {
    fn collect(v: &Value, defs: &mut HashMap<String, Value>) -> Result<()> {
        if let Some(id) = v.get("HandleId").and_then(Value::as_str) {
            ensure!(
                defs.insert(id.to_owned(), v["Data"].clone()).is_none(),
                "duplicate resource handle {id}"
            );
        }
        match v {
            Value::Object(v) => {
                for (key, child) in v {
                    if key == "File"
                        && child.get("RootChunk").is_some()
                        && child.get("Version").is_some()
                    {
                        continue;
                    }
                    collect(child, defs)?
                }
            }
            Value::Array(v) => {
                for child in v {
                    collect(child, defs)?
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn visit(
        v: &Value,
        defs: &HashMap<String, Value>,
        ids: &mut HashMap<String, usize>,
    ) -> Result<Value> {
        // Embedded widget files have their own handle and buffer tables.
        if v.get("RootChunk").is_some() && v.get("Version").is_some() {
            return canonical(&json!({"Data":v}));
        }
        if v.get("$type").and_then(Value::as_str) == Some("Variant")
            && v.get("value_type").and_then(Value::as_str).is_some()
        {
            // Name-table indices in the diagnostic bytes are resource-local.
            // Compare the decoded native type and value instead.
            return Ok(json!({"$type":"Variant", "value_type":v["value_type"],
                "value":visit(&v["value"], defs, ids)?}));
        }
        if v.as_object().is_some_and(|object| object.len() == 1)
            && let Some(elements) = v.get("Elements").filter(|v| v.is_array())
        {
            return visit(elements, defs, ids);
        }
        if v.get("$type").and_then(Value::as_str) == Some("NodeRef") {
            let text = v
                .get("$value")
                .and_then(Value::as_str)
                .context("NodeRef value")?;
            let id = if v.get("$storage").and_then(Value::as_str) == Some("uint64") {
                text.parse::<u64>().context("numeric NodeRef identity")?
            } else {
                crate::world::node_ref_hash(text)
            };
            return Ok(json!({"$type":"NodeRef","$value":id.to_string()}));
        }
        if v.get("$type").and_then(Value::as_str) == Some("TweakDBID") {
            let text = v
                .get("$value")
                .and_then(Value::as_str)
                .context("TweakDBID value")?;
            let id = text
                .parse::<u64>()
                .unwrap_or_else(|_| ghostline_red::tweakdb::TweakId::from_name(text).0);
            return Ok(json!({"$type":"TweakDBID","$value":id.to_string()}));
        }
        if matches!(
            v.get("$type").and_then(Value::as_str),
            Some("NodeRef" | "ResourcePath")
        ) && matches!(v.get("$value").and_then(Value::as_str), Some("0" | ""))
        {
            return Ok(json!({"$type":v["$type"],"$value":""}));
        }
        if let Some(id) = v
            .get("HandleId")
            .or_else(|| v.get("HandleRefId"))
            .and_then(Value::as_str)
        {
            if id == "-1" {
                return Ok(Value::Null);
            }
            if let Some(id) = ids.get(id) {
                return Ok(json!({"$ref":id}));
            }
            let data = defs
                .get(id)
                .with_context(|| format!("undefined resource handle {id}"))?;
            let next = ids.len();
            ids.insert(id.to_owned(), next);
            return Ok(json!({"$handle":next,"data":visit(data,defs,ids)?}));
        }
        match v {
            Value::Object(v) => v
                .iter()
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_iter()
                .filter(|(key, _)| key.as_str() != "$storage")
                .map(|(key, v)| {
                    let value = if matches!(key.as_str(), "BufferId" | "BufferRefId") {
                        let id = v.as_str().context("buffer identity")?;
                        if id == "-1" {
                            v.clone()
                        } else {
                            let next = ids.len();
                            json!(ids.entry(format!("buffer:{id}")).or_insert(next))
                        }
                    } else {
                        visit(v, defs, ids)?
                    };
                    Ok((key.clone(), value))
                })
                .collect(),
            Value::Array(v) => v.iter().map(|v| visit(v, defs, ids)).collect(),
            _ => Ok(v.clone()),
        }
    }
    let mut defs = HashMap::new();
    collect(value, &mut defs)?;
    let root = value
        .pointer("/Data/RootChunk")
        .context("resource root chunk")?;
    visit(root, &defs, &mut HashMap::new())
}

fn intrinsic_default(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(v) => !*v,
        Value::Number(v) => v.as_f64() == Some(0.),
        Value::String(v) => v.is_empty(),
        Value::Array(v) => v.is_empty(),
        Value::Object(v) => {
            if let Some(kind) = v.get("$type").and_then(Value::as_str) {
                matches!(kind, "CName" | "NodeRef" | "TweakDBID" | "ResourcePath")
                    && v.get("$value")
                        .and_then(Value::as_str)
                        .is_some_and(|v| v == "0" || v == "None" || v.is_empty())
            } else {
                false
            }
        }
    }
}

fn compare(expected: &Value, actual: &Value, path: &str) -> Result<()> {
    match (expected, actual) {
        (Value::Number(number), Value::Bool(flag)) | (Value::Bool(flag), Value::Number(number)) => {
            ensure!(
                number.as_u64() == Some(u64::from(*flag)),
                "boolean changed at {path}: {expected} -> {actual}"
            );
            Ok(())
        }
        (Value::Object(expected), Value::Object(actual)) => {
            for (key, value) in expected {
                if let Some(found) = actual.get(key) {
                    compare(value, found, &format!("{path}/{key}"))?
                } else {
                    ensure!(
                        intrinsic_default(value),
                        "readback lost requested value at {path}/{key}: {value}"
                    );
                }
            }
            for (key, value) in actual {
                if !expected.contains_key(key) {
                    ensure!(
                        intrinsic_default(value),
                        "unexpected readback value at {path}/{key}: {value}"
                    );
                }
            }
            Ok(())
        }
        (Value::Array(expected), Value::Array(actual)) => {
            ensure!(
                expected.len() == actual.len(),
                "readback array length differs at {path}"
            );
            for (i, (expected, actual)) in expected.iter().zip(actual).enumerate() {
                compare(expected, actual, &format!("{path}/{i}"))?;
            }
            Ok(())
        }
        (Value::Number(expected), Value::Number(actual)) => {
            if expected.is_i64() && actual.is_i64() {
                ensure!(
                    expected.as_i64() == actual.as_i64(),
                    "integer changed at {path}"
                );
            } else {
                let a = expected.as_f64().context("expected number")?;
                let b = actual.as_f64().context("decoded number")?;
                ensure!(
                    (a - b).abs() <= f64::from(f32::EPSILON) * a.abs().max(1.),
                    "number changed at {path}: {a} -> {b}"
                );
            }
            Ok(())
        }
        _ => {
            ensure!(
                expected == actual,
                "readback differs at {path}: {expected} -> {actual}"
            );
            Ok(())
        }
    }
}

/// Checks requested typed values, resource imports and graph wiring independently
/// of numeric handle and buffer allocation. Only intrinsic zero/empty omission
/// and float32 rounding are accepted for requested values.
pub fn verify(expected: &Value, actual: &Value) -> Result<()> {
    compare(
        &canonical(expected)?,
        &canonical(actual)?,
        "/Data/RootChunk",
    )
}

#[cfg(test)]
mod tests {
    use super::verify;
    use serde_json::{Value, json};

    #[test]
    fn semantic_handles_do_not_depend_on_property_order_or_numbers() {
        let a:Value=serde_json::from_str(r#"{"Data":{"RootChunk":{"$type":"Example","z":{"HandleId":"7","Data":{"$type":"Leaf","value":2}},"a":{"HandleId":"3","Data":{"$type":"Leaf","value":1}},"again":{"HandleRefId":"7"}}}}"#).unwrap();
        let b:Value=serde_json::from_str(r#"{"Data":{"RootChunk":{"$type":"Example","a":{"HandleId":"1","Data":{"value":1,"$type":"Leaf"}},"again":{"HandleRefId":"2"},"z":{"HandleId":"2","Data":{"value":2,"$type":"Leaf"}}}}}"#).unwrap();
        verify(&a, &b).unwrap();
        let mut changed = b;
        changed["Data"]["RootChunk"]["again"] = json!({"HandleRefId":"1"});
        assert!(verify(&a, &changed).is_err());
    }
}
