//! Native predicates shared by optional objectives and choice gates.
use super::*;
use serde_json::json;

pub(in crate::compiler::blocks) fn predicate(
    g: &mut Graph,
    id: u32,
    spec: &Value,
    snapshot: bool,
) -> Result<NodeId> {
    let kind = spec["kind"].as_str().context("condition kind")?;
    let value = &spec["value"];
    let text = value.as_str();
    let reference = |v: &str| v.starts_with('#') || v.starts_with("$/");
    let node = match kind {
        "fact" => n::fact_condition(
            g,
            id,
            text.context("fact condition needs a fact name")?,
            "Greater",
            0,
            false,
        )?,
        "trigger" => n::trigger(
            g,
            id,
            text.filter(|v| reference(v))
                .context("trigger condition needs a NodeRef")?,
            "IsInside",
        )?,
        "timer" => {
            let seconds = value
                .as_f64()
                .or_else(|| text.and_then(|v| v.parse::<f64>().ok()))
                .filter(|v| v.is_finite() && *v >= 0.)
                .context("timer condition needs nonnegative seconds")?;
            ensure!(seconds <= f64::from(u32::MAX), "timer exceeds native range");
            let ms = (seconds * 1000.).round() as u64;
            n::real_delay(g, id, ms / 1000, ms % 1000)?
        }
        "detection" | "alarm" if text.is_some_and(|v| !reference(v)) => {
            n::fact_condition(g, id, text.unwrap(), "Greater", 0, false)?
        }
        "detection" | "alarm" => {
            let observer = spec
                .get("observer")
                .and_then(Value::as_str)
                .or(text)
                .filter(|v| reference(v))
                .context("detection/alarm needs an observer NodeRef or an explicit state fact")?;
            let entries = spec
                .get("entry")
                .and_then(Value::as_str)
                .into_iter()
                .collect::<Vec<_>>();
            if kind == "alarm" && spec.get("function").is_some() {
                n::device_condition(
                    g,
                    id,
                    observer,
                    spec["controller_class"]
                        .as_str()
                        .context("alarm device controller_class")?,
                    spec["function"].as_str().context("alarm device function")?,
                )?
            } else {
                let condition = if kind == "detection" {
                    json!({"$type":"questVision_ConditionType","observerPuppetRef":n::entity(Some(observer),&entries),"observedTargetRef":n::entity(spec.get("target").and_then(Value::as_str),&[]),"isObservedTargetPlayer":u8::from(spec.get("target").is_none()),"isInstant":1,"inverted":0})
                } else {
                    json!({"$type":"questCharacterTriggeredCombatInSecuritySystem_ConditionType","objectRef":n::entity(Some(observer),&entries)})
                };
                let inner = g.wrap(condition);
                let condition=g.wrap(json!({"$type":if kind=="detection"{"questSensesCondition"}else{"questCharacterCondition"},"type":inner}));
                g.node(
                    id,
                    "questPauseConditionNodeDefinition",
                    &["In"],
                    &["Out"],
                    json!({"condition":condition}),
                )?
            }
        }
        _ => anyhow::bail!("unknown condition kind {kind}"),
    };
    if snapshot {
        g.snapshot(node)?;
    }
    Ok(node)
}
