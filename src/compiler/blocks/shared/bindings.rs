use crate::manifest::Stage;
use anyhow::{Context, Result};
use serde_json::Value;

pub(in crate::compiler::blocks) fn s(v: &Value) -> Result<&str> {
    v.as_str().context("native equipment binding")
}

pub(in crate::compiler::blocks) fn list<'a>(stage: &'a Stage, key: &str) -> Result<&'a [Value]> {
    stage.fields.get(key).map_or(Ok(&[][..]), |v| {
        v.as_array()
            .map(Vec::as_slice)
            .with_context(|| format!("{key} must be array"))
    })
}
