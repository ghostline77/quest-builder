use crate::project::depot_path;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::HashSet;

pub(super) fn deletions(spec: &Value) -> Result<Vec<Value>> {
    let Some(patches) = spec.get("sector_patches") else {
        return Ok(Vec::new());
    };
    let mut sectors = HashSet::new();
    patches
        .as_array()
        .context("sector_patches must be an array")?
        .iter()
        .map(|patch| {
            let path = patch["path"].as_str().context("sector patch path")?;
            depot_path(path)?;
            ensure!(
                path.ends_with(".streamingsector"),
                "sector patch requires a streamingsector"
            );
            ensure!(
                sectors.insert(path.replace('/', "\\").to_lowercase()),
                "duplicate sector patch"
            );
            let expected = patch["expected_nodes"]
                .as_u64()
                .context("sector patch expected_nodes")?;
            ensure!(expected > 0, "sector patch expected_nodes must be positive");
            let mut indices = HashSet::new();
            let deletions = patch["node_deletions"]
                .as_array()
                .context("sector patch node_deletions")?
                .iter()
                .map(|node| {
                    let index = node["index"].as_u64().context("sector deletion index")?;
                    ensure!(
                        index < expected && indices.insert(index),
                        "duplicate or out-of-range sector deletion"
                    );
                    let kind = node["type"].as_str().context("sector deletion type")?;
                    ensure!(
                        kind.starts_with("world") && kind.ends_with("Node"),
                        "sector deletion requires a world node type"
                    );
                    Ok(json!({"index":index,"type":kind}))
                })
                .collect::<Result<Vec<_>>>()?;
            ensure!(
                !deletions.is_empty(),
                "sector patch requires node deletions"
            );
            Ok(json!({"path":path,"expectedNodes":expected,"nodeDeletions":deletions}))
        })
        .collect()
}
