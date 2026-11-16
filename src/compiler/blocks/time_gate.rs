//! Native time gate stage generation.

use super::Block;
use crate::manifest::Stage;
use crate::nodes as n;
use anyhow::{Result, ensure};
use serde_json::Value;

pub(super) fn build(stage: &Stage) -> Result<Value> {
    let mut b = Block::new(stage)?;

    ensure!(
        ["days", "hours", "minutes", "seconds"]
            .iter()
            .any(|k| stage.number(k, 0) > 0),
        "time gate requires a positive duration"
    );
    let target = b.node(|g, id| {
        if stage.optional("clock") == Some("real") {
            let seconds = stage
                .number("days", 0)
                .checked_mul(86400)
                .and_then(|v| v.checked_add(stage.number("hours", 0).checked_mul(3600)?))
                .and_then(|v| v.checked_add(stage.number("minutes", 0).checked_mul(60)?))
                .and_then(|v| v.checked_add(stage.number("seconds", 0)))
                .ok_or_else(|| anyhow::anyhow!("real-time delay overflow"))?;
            return n::real_delay(g, id, seconds, 0);
        }
        n::game_delay(
            g,
            id,
            stage.number("days", 0),
            stage.number("hours", 0),
            stage.number("minutes", 0),
            stage.number("seconds", 0),
        )
    })?;
    let start = b.start.clone();
    let tail = b.append(&start, target, "In")?;
    let tail = b.fact(tail, stage.optional("completion_fact"))?;
    b.finish(tail, "success")?;

    b.document()
}
