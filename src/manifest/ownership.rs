use super::*;
use std::collections::{BTreeSet, VecDeque};

fn lifecycle_claims(stage: &Stage) -> BTreeMap<String, &Value> {
    let mut result = BTreeMap::new();
    for (field, resource) in [
        (
            "actor_lifecycle",
            format!(
                "actor:{}:{}",
                stage.optional("community").unwrap_or(""),
                stage.optional("entry").unwrap_or("")
            ),
        ),
        (
            "objective_lifecycle",
            format!("objective:{}", stage.optional("objective").unwrap_or("")),
        ),
    ] {
        if let Some(policy) = stage
            .fields
            .get(field)
            .filter(|v| v.as_object().is_some_and(|v| !v.is_empty()))
        {
            result.insert(resource, policy);
        }
    }
    result
}
fn action_claim(action: &Value) -> Option<String> {
    let string = |key| action.get(key).and_then(Value::as_str).unwrap_or("");
    match string("kind") {
        "objective" | "mappin" | "message" => {
            Some(format!("{}:{}", string("kind"), string("path")))
        }
        "community" => Some(format!("actor:{}:{}", string("community"), string("entry"))),
        "trigger_state" => Some(format!("trigger:{}", string("trigger"))),
        "weather_reset" => Some(format!("weather:{}", string("source"))),
        "prefab_variant" => Some(format!("prefab:{}:{}", string("prefab"), string("variant"))),
        _ => None,
    }
}
fn claims(stage: &Stage) -> BTreeSet<String> {
    let mut result = lifecycle_claims(stage).into_keys().collect::<BTreeSet<_>>();
    for key in ["setup", "restore", "retire"] {
        for action in stage
            .fields
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            result.extend(action_claim(action));
        }
    }
    for actions in stage
        .fields
        .get("effects")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|v| v.values())
    {
        for action in actions.as_array().into_iter().flatten() {
            result.extend(action_claim(action));
        }
    }
    if stage.kind == "investigate_reactions" {
        for clue in stage
            .fields
            .get("clues")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(pin) = clue.get("mappin").and_then(Value::as_str) {
                result.insert(format!("mappin:{pin}"));
            }
        }
    }
    result
}

pub(super) fn validate(
    manifest: &Manifest,
    routes: &BTreeMap<String, BTreeMap<String, String>>,
    entry: &str,
    errors: &mut Vec<Diagnostic>,
) -> Result<()> {
    let stages = manifest
        .stages
        .iter()
        .map(|s| (s.id.as_str(), s))
        .collect::<BTreeMap<_, _>>();
    if !stages.contains_key(entry) {
        return Ok(());
    }
    for stage in &manifest.stages {
        let outcomes = stage.outcomes()?;
        if !lifecycle_claims(stage).is_empty()
            && (!outcomes.contains_key("success")
                || outcomes
                    .keys()
                    .any(|v| !matches!(v.as_str(), "success" | "failure" | "cancelled")))
        {
            errors.push(Diagnostic::error(
                "ambiguous_lifecycle_outcome",
                format!("/stages/{}", stage.id),
                "Lifecycle policies require success/failure/cancelled outcomes",
            ));
        }
        for event in stage
            .fields
            .get("contract")
            .and_then(|v| v.get("external_events"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let event = event.as_str().context("external event contract")?;
            if !manifest.external_events.iter().any(|v| v == event) {
                errors.push(Diagnostic::error(
                    "undeclared_external_event",
                    format!("/stages/{}/contract/external_events", stage.id),
                    format!("External event {event} is not declared by this quest"),
                ));
            }
        }
    }
    for group in &manifest.parallel_groups {
        let branches = group
            .branches
            .iter()
            .map(|branch| {
                branch
                    .iter()
                    .filter_map(|id| stages.get(id.as_str()))
                    .flat_map(|s| claims(s))
                    .collect::<BTreeSet<_>>()
            })
            .collect::<Vec<_>>();
        for (i, branch) in branches.iter().enumerate() {
            for other in &branches[i + 1..] {
                for resource in branch.intersection(other) {
                    errors.push(Diagnostic::error(
                        "parallel_ownership_conflict",
                        format!("/parallel_groups/{}", group.id),
                        format!("Concurrent branches compete for {resource}"),
                    ));
                }
            }
        }
    }
    let resources = manifest
        .stages
        .iter()
        .flat_map(lifecycle_claims)
        .map(|(resource, _)| resource)
        .collect::<BTreeSet<_>>();
    let mut reported = BTreeSet::new();
    for resource in resources {
        let mut pending = VecDeque::from([(entry.to_owned(), false)]);
        let mut seen = BTreeSet::new();
        while let Some((identifier, mut owned)) = pending.pop_front() {
            if !seen.insert((identifier.clone(), owned)) {
                continue;
            }
            let Some(stage) = stages.get(identifier.as_str()) else {
                continue;
            };
            let policy = lifecycle_claims(stage).get(&resource).copied();
            if let Some(policy) = policy {
                let actor = resource.starts_with("actor:");
                let acquire = if actor { "assign_follower" } else { "activate" };
                let default = if actor && stage.kind == "defend_target" {
                    "retain"
                } else {
                    acquire
                };
                let enter = policy
                    .get("on_enter")
                    .and_then(Value::as_str)
                    .unwrap_or(default);
                let problem = if enter == "retain" && !owned {
                    Some("ownership_not_acquired")
                } else if enter == acquire && owned {
                    Some("ownership_already_acquired")
                } else {
                    None
                };
                if let Some(problem) = problem
                    && reported.insert((problem, identifier.clone(), resource.clone()))
                {
                    errors.push(Diagnostic::error(
                        problem,
                        format!("/stages/{identifier}"),
                        format!("Invalid {enter} handoff for {resource}"),
                    ));
                }
                owned = true;
            }
            for (outcome, target) in &routes[&identifier] {
                let retained = policy.map_or(owned, |p| {
                    p.get(format!("on_{outcome}")).and_then(Value::as_str) == Some("retain")
                });
                if target == "$end" {
                    if retained
                        && reported.insert(("ownership_leak", identifier.clone(), resource.clone()))
                    {
                        errors.push(Diagnostic::error(
                            "ownership_leak",
                            format!("/stages/{identifier}"),
                            format!("Terminal {outcome} retains {resource} without an owner"),
                        ));
                    }
                } else {
                    pending.push_back((target.clone(), retained));
                }
            }
        }
    }
    if manifest.parallel_groups.is_empty() {
        let mut reachable = BTreeSet::new();
        let mut pending = vec![entry];
        while let Some(id) = pending.pop() {
            if id == "$end" || !reachable.insert(id) {
                continue;
            }
            if let Some(next) = routes.get(id) {
                pending.extend(next.values().map(String::as_str));
            }
        }
        let mut can_end = routes
            .iter()
            .filter(|(_, edges)| edges.values().any(|v| v == "$end"))
            .map(|(id, _)| id.as_str())
            .collect::<BTreeSet<_>>();
        loop {
            let before = can_end.len();
            for (id, edges) in routes {
                if edges.values().any(|next| can_end.contains(next.as_str())) {
                    can_end.insert(id);
                }
            }
            if can_end.len() == before {
                break;
            }
        }
        for id in reachable.difference(&can_end) {
            errors.push(Diagnostic::error(
                "nonterminating_flow",
                format!("/stages/{id}"),
                "No outcome route can reach $end",
            ));
        }
    }
    Ok(())
}
