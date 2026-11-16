//! The complete canonical authoring schema and native diagnostics.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::LazyLock;

/// Complete bundled authoring schema, including all registered blocks.
pub static SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/quest-schema-v1.json"))
        .expect("bundled authoring schema must parse")
});

/// A diagnostic tied to a manifest JSON pointer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub level: String,
    pub code: String,
    pub path: String,
    pub message: String,
}
impl Diagnostic {
    pub(crate) fn error(code: &str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: "error".into(),
            code: code.into(),
            path: path.into(),
            message: message.into(),
        }
    }
}

/// Validates every structural constraint using an offline draft 2020-12 validator.
pub fn validate(value: &Value) -> Result<Vec<Diagnostic>> {
    let validator = jsonschema::validator_for(&SCHEMA).context("invalid bundled quest schema")?;
    Ok(validator
        .iter_errors(value)
        .map(|e| Diagnostic::error("schema", e.instance_path().to_string(), e.to_string()))
        .collect())
}

/// Returns all block contracts for human and agent tooling.
pub fn catalog() -> Value {
    let blocks=SCHEMA["$defs"]["stage"]["oneOf"].as_array().into_iter().flatten().map(|branch|{
        let shape=&branch["allOf"][1];
        json!({"type":shape["properties"]["type"]["const"],"required":shape["required"],"properties":shape["properties"],"native_builder":crate::compiler::has_builder(shape["properties"]["type"]["const"].as_str().unwrap_or_default())})
    }).collect::<Vec<_>>();
    json!({"schema_version":1,"common_fields":SCHEMA["$defs"]["baseStage"],"definitions":SCHEMA["$defs"],"blocks":blocks})
}

/// Enriches registry contracts with explicitly supplied examples and current evidence.
pub fn catalog_with(examples: &[Value], receipts: &[Value]) -> Result<Value> {
    let mut catalog = catalog();
    let mut examples_by_type = std::collections::BTreeMap::<String, Vec<Value>>::new();
    let mut example_errors = Vec::new();
    for (index, example) in examples.iter().enumerate() {
        match crate::manifest::check(example) {
            Ok(checked) => {
                if let Some(manifest) = checked.manifest {
                    for stage in &manifest.stages {
                        examples_by_type
                            .entry(stage.kind.clone())
                            .or_default()
                            .push(json!({"manifest":manifest.id,"stage":stage}));
                    }
                } else {
                    example_errors.push(json!({"index":index,"diagnostics":checked.diagnostics}));
                }
            }
            Err(error) => example_errors.push(json!({"index":index,"error":format!("{error:#}")})),
        }
    }
    let evidence=receipts.iter().map(|receipt|json!({"receipt":receipt,"verification":match crate::evidence::verify(receipt){Ok(value)=>value,Err(error)=>json!({"valid":false,"error":format!("{error:#}")})}})).collect::<Vec<_>>();
    for block in catalog["blocks"]
        .as_array_mut()
        .context("catalogue blocks")?
    {
        let kind = block["type"].as_str().context("block type")?.to_owned();
        block["implementation"] =
            json!({"mode":"generated","source":format!("src/compiler/blocks/{kind}.rs")});
        let mut fields = SCHEMA["$defs"]["baseStage"]["properties"]
            .as_object()
            .context("common fields")?
            .clone();
        fields.extend(
            block["properties"]
                .as_object()
                .context("block fields")?
                .clone(),
        );
        let required = SCHEMA["$defs"]["baseStage"]["required"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(block["required"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        block["fields"] = json!(
            fields
                .iter()
                .map(|(key, schema)| (
                    key,
                    json!({"required":required.contains(key.as_str()),"schema":schema})
                ))
                .collect::<std::collections::BTreeMap<_, _>>()
        );
        block["ports"] = json!({"inputs":{"start":"In1"},"outcomes":{"success":"Out1"},"overrides":"Explicit aliases are validated against generated native entry/output nodes; owned blocks derive required outcomes from their child bindings"});
        block["examples"] = json!(examples_by_type.get(&kind).cloned().unwrap_or_default());
        let relevant = evidence
            .iter()
            .filter(|entry| {
                entry["receipt"]["observations"]["stage_types"]
                    .as_array()
                    .is_some_and(|v| v.iter().any(|v| v == &kind))
            })
            .cloned()
            .collect::<Vec<_>>();
        block["structural_build_recorded"] = json!(relevant.iter().any(
            |entry| entry["verification"]["valid"] == true
                && matches!(
                    entry["receipt"]["observations"]["kind"].as_str(),
                    Some("build" | "native_build")
                )
        ));
        block["in_game_pass_recorded"] = json!(
            relevant
                .iter()
                .any(|entry| entry["verification"]["runtime_verified"] == true)
        );
        block["evidence"] = json!(relevant);
        block["limits"] = json!({"scope":"A recorded build or playtest proves only its exact configuration and hashed resources","native_readback":"Verifies authored serialization; engine behavior needs gameplay evidence"});
    }
    catalog["example_errors"] = json!(example_errors);
    catalog["evidence_errors"] = json!(
        evidence
            .iter()
            .filter(|entry| entry["verification"]["valid"] != true)
            .collect::<Vec<_>>()
    );
    Ok(catalog)
}

/// Human-readable catalogue generated from exactly the same contract data.
pub fn catalog_markdown(catalog: &Value) -> Result<String> {
    let mut out = String::from(
        "# Quest blocks\n\nResources are generated directly. Recorded evidence applies to the listed configuration and hashes.\n\n",
    );
    for block in catalog["blocks"].as_array().context("catalogue blocks")? {
        out.push_str(&format!(
            "## `{}`\n\nSource: `{}`\n\n| Field | Required | Contract |\n| --- | --- | --- |\n",
            block["type"].as_str().unwrap_or_default(),
            block["implementation"]["source"]
                .as_str()
                .unwrap_or_default()
        ));
        for (field, details) in block["fields"].as_object().context("catalogue fields")? {
            let schema = details["schema"].to_string().replace('|', "&#124;");
            out.push_str(&format!(
                "| `{field}` | {} | `{schema}` |\n",
                details["required"].as_bool().unwrap_or(false)
            ));
        }
        out.push_str(&format!(
            "\nExamples: {}. Current structural evidence: {}. Current gameplay evidence: {}.\n\n",
            block["examples"].as_array().map_or(0, Vec::len),
            block["structural_build_recorded"],
            block["in_game_pass_recorded"]
        ));
    }
    Ok(out)
}
