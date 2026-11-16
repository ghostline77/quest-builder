//! Hash-bound build, package and playtest evidence.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// SHA-256 of bytes, shared by scenario contracts and evidence receipts.
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
/// Canonical object ordering preserves array order and every scalar value.
pub fn canonical_hash(value: &Value) -> Result<String> {
    fn canonical(v: &Value) -> Value {
        match v {
            Value::Object(values) => values
                .iter()
                .map(|(k, v)| (k.clone(), canonical(v)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
            Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
            _ => v.clone(),
        }
    }
    Ok(digest(&serde_json::to_vec(&canonical(value))?))
}

/// Records explicitly supplied evidence files against exact resource/tool inputs.
pub fn record(inputs: &[PathBuf], attachments: &[PathBuf], observations: &Value) -> Result<Value> {
    ensure!(!inputs.is_empty(), "evidence requires frozen inputs");
    let files = |paths: &[PathBuf]| -> Result<BTreeMap<String, String>> {
        let mut files = BTreeMap::new();
        for path in paths {
            let path = path.canonicalize()?;
            ensure!(path.is_file(), "evidence input is not a file");
            ensure!(
                files
                    .insert(path.display().to_string(), digest(&fs::read(path)?))
                    .is_none(),
                "duplicate evidence path"
            );
        }
        Ok(files)
    };
    let inputs = files(inputs)?;
    let attachments = files(attachments)?;
    Ok(
        json!({"schema_version":1,"input_fingerprint":canonical_hash(&json!(inputs))?,"inputs":inputs,"attachments":attachments,"observations":observations}),
    )
}

/// Detects changed, missing or failed evidence without promoting it to runtime proof.
pub fn verify(receipt: &Value) -> Result<Value> {
    ensure!(
        receipt["schema_version"] == 1,
        "unsupported evidence schema"
    );
    let mut problems = Vec::new();
    ensure!(
        receipt["inputs"].as_object().is_some_and(|v| !v.is_empty()),
        "evidence requires bound input files"
    );
    let mut packed_input = false;
    for group in ["inputs", "attachments"] {
        for (path, expected) in receipt[group]
            .as_object()
            .context("evidence file hash map")?
        {
            match fs::read(Path::new(path)) {
                Ok(bytes) => {
                    if group == "inputs" && (bytes.starts_with(b"CR2W") || bytes.starts_with(b"RDAR")) {
                        packed_input = true;
                    }
                    if Some(digest(&bytes).as_str()) != expected.as_str() {
                        problems.push(json!({"path":path,"problem":"changed"}));
                    }
                }
                Err(error) => problems.push(json!({"path":path,"problem":"missing_or_unreadable","error":error.to_string()})),
            }
        }
    }
    if receipt["input_fingerprint"] != canonical_hash(&receipt["inputs"])? {
        problems.push(json!({"problem":"input_fingerprint_mismatch"}));
    }
    let passed = receipt["observations"]["passed"].as_bool().unwrap_or(false);
    let attached = receipt["attachments"]
        .as_object()
        .is_some_and(|v| !v.is_empty());
    let gameplay = receipt["observations"]["kind"] == "gameplay";
    if gameplay && (!attached || !packed_input) {
        problems
            .push(json!({"problem":"gameplay_evidence_requires_packed_inputs_and_attachments"}));
    }
    Ok(
        json!({"valid":problems.is_empty()&&passed,"stale":!problems.is_empty(),"observed_pass":passed,"problems":problems,"runtime_verified":attached&&problems.is_empty()&&passed&&receipt["observations"]["kind"]=="gameplay"}),
    )
}
