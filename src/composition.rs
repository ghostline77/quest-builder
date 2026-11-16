//! Whole-value authoring aliases, recipes, stable identities and localization.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};

/// Expanded authoring input and its independently addressable bindings.
#[derive(Debug, Clone)]
pub struct Composition {
    pub manifest: Value,
    pub bindings: Value,
    pub text: BTreeMap<String, String>,
    pub configuration: Option<Value>,
}

fn object(value: &Value) -> Result<&Map<String, Value>> {
    value.as_object().context("expected authoring object")
}
fn record(value: &Value, shorthand: &str) -> Result<Value> {
    if value.is_string() {
        Ok(json!({shorthand:value}))
    } else {
        object(value)?;
        Ok(value.clone())
    }
}
fn put(value: &mut Value, key: &str, default: Value) {
    value
        .as_object_mut()
        .expect("validated record")
        .entry(key)
        .or_insert(default);
}
fn text(value: &Value) -> Result<&str> {
    value.as_str().context("expected authoring string")
}
fn id(value: &Value) -> Result<&str> {
    let value = text(value)?;
    ensure!(
        crate::manifest::identifier(value),
        "invalid authoring identifier {value}"
    );
    Ok(value)
}
fn journal_id(value: &Value) -> Result<&str> {
    let value = text(value)?;
    ensure!(
        !value.is_empty()
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_'),
        "invalid journal id {value}"
    );
    Ok(value)
}
fn fields(value: &Value, allowed: &[&str]) -> Result<()> {
    for key in object(value)?.keys() {
        ensure!(
            allowed.contains(&key.as_str()),
            "unknown authoring field {key}"
        );
    }
    Ok(())
}
fn add_text(entries: &mut Vec<(Value, Value)>, key: &Value, value: &Value) -> Result<()> {
    ensure!(
        key.as_str().is_some_and(|s| !s.is_empty()) && value.is_string(),
        "localization keys and text must be strings"
    );
    entries.push((key.clone(), value.clone()));
    Ok(())
}
fn entries(value: &Value, key: &str) -> Result<Vec<(String, Value)>> {
    value.get(key).map_or_else(
        || Ok(Vec::new()),
        |v| {
            Ok(object(v)?
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        },
    )
}

/// Resolves whole values only, preserving types and detecting alias cycles.
pub fn resolve(value: &Value, bindings: &Value) -> Result<Value> {
    fn visit(value: &Value, bindings: &Value, trail: &mut Vec<String>) -> Result<Value> {
        match value {
            Value::String(s) if s.starts_with("@@") => Ok(Value::String(s[1..].to_owned())),
            Value::String(s) if s.starts_with('@') => {
                let name = &s[1..];
                ensure!(
                    !trail.iter().any(|t| t == name),
                    "alias cycle: {} -> {name}",
                    trail.join(" -> ")
                );
                let mut target = bindings;
                for part in name.split('.') {
                    target = target
                        .get(part)
                        .with_context(|| format!("unknown alias {s}"))?;
                }
                trail.push(name.to_owned());
                let result = visit(target, bindings, trail);
                trail.pop();
                result
            }
            Value::Array(values) => values.iter().map(|v| visit(v, bindings, trail)).collect(),
            Value::Object(values) => values
                .iter()
                .map(|(k, v)| Ok((k.clone(), visit(v, bindings, trail)?)))
                .collect(),
            _ => Ok(value.clone()),
        }
    }
    visit(value, bindings, &mut Vec::new())
}

fn phone_entries(
    raw: &Value,
    path: &str,
    prefix: &str,
    localization: &mut Vec<(Value, Value)>,
) -> Result<Value> {
    let mut result = Map::new();
    let mut seen = HashSet::new();
    for (alias, value) in object(raw)? {
        ensure!(
            crate::manifest::identifier(alias),
            "invalid phone entry alias {alias}"
        );
        let mut entry = value.clone();
        fields(
            &entry,
            &[
                "type",
                "id",
                "text",
                "localization_key",
                "delay",
                "entries",
                "sender",
                "important",
                "attachment",
                "attachment_class",
                "attachment_file_index",
            ],
        )?;
        put(&mut entry, "id", json!(alias));
        let entry_id = journal_id(&entry["id"])?.to_owned();
        ensure!(
            seen.insert(entry_id.clone()),
            "duplicate phone entry {entry_id}"
        );
        entry["path"] = json!(format!("{path}/{entry_id}"));
        match entry["type"].as_str() {
            Some("message") => {
                ensure!(
                    entry.get("entries").is_none(),
                    "phone message cannot have child entries"
                );
                put(
                    &mut entry,
                    "localization_key",
                    json!(format!("{prefix}_{alias}")),
                );
                add_text(localization, &entry["localization_key"], &entry["text"])?;
                if let Some(delay) = entry.get("delay") {
                    ensure!(
                        delay.as_f64().is_some_and(|n| n.is_finite() && n >= 0.),
                        "message delay must be finite and nonnegative"
                    );
                }
                if let Some(sender) = entry.get("sender") {
                    ensure!(
                        matches!(sender.as_str(), Some("NPC" | "Player")),
                        "message sender must be NPC or Player"
                    );
                }
                if let Some(important) = entry.get("important") {
                    ensure!(important.is_boolean(), "message important must be boolean");
                }
                if let Some(attachment) = entry.get("attachment") {
                    ensure!(attachment.is_string(), "attachment must be a journal path");
                }
            }
            Some("choice_group") => {
                ensure!(
                    ![
                        "text",
                        "localization_key",
                        "delay",
                        "sender",
                        "important",
                        "attachment"
                    ]
                    .iter()
                    .any(|k| entry.get(*k).is_some()),
                    "choice group cannot contain message fields"
                );
                let mut choices = Map::new();
                let mut ids = HashSet::new();
                for (alias, raw) in object(&entry["entries"])? {
                    ensure!(
                        crate::manifest::identifier(alias),
                        "invalid choice alias {alias}"
                    );
                    let mut choice = raw.clone();
                    fields(&choice, &["id", "text", "localization_key"])?;
                    put(&mut choice, "id", json!(alias));
                    let choice_id = journal_id(&choice["id"])?.to_owned();
                    ensure!(
                        ids.insert(choice_id.clone()),
                        "duplicate phone choice {choice_id}"
                    );
                    choice["path"] = json!(format!("{path}/{entry_id}/{choice_id}"));
                    put(
                        &mut choice,
                        "localization_key",
                        json!(format!("{prefix}_choice_{alias}")),
                    );
                    add_text(localization, &choice["localization_key"], &choice["text"])?;
                    choices.insert(alias.clone(), choice);
                }
                ensure!(!choices.is_empty(), "empty phone choice group");
                entry["entries"] = Value::Object(choices);
            }
            _ => bail!("unsupported phone entry type"),
        }
        result.insert(alias.clone(), entry);
    }
    Ok(Value::Object(result))
}

fn thread(
    mut thread: Value,
    namespace: &str,
    contact: &str,
    contact_alias: &str,
    alias: &str,
    title: &Value,
    localization: &mut Vec<(Value, Value)>,
) -> Result<Value> {
    fields(
        &thread,
        &[
            "id",
            "title",
            "title_key",
            "messages",
            "choices",
            "final_message",
            "entries",
        ],
    )?;
    put(&mut thread, "id", json!(format!("{namespace}_{alias}")));
    let thread_id = journal_id(&thread["id"])?.to_owned();
    let path = format!("contacts/{contact}/{thread_id}");
    let prefix = format!("gl_{namespace}_{contact_alias}_{alias}");
    thread["path"] = json!(path);
    put(&mut thread, "title_key", json!(format!("{prefix}_title")));
    thread["choice_group"] = json!(format!("{path}/choices"));
    add_text(
        localization,
        &thread["title_key"],
        thread.get("title").unwrap_or(title),
    )?;
    if let Some(raw) = thread.get("entries") {
        ensure!(
            !["messages", "choices", "final_message"]
                .iter()
                .any(|k| thread.get(*k).is_some()),
            "explicit phone entries conflict with shorthand"
        );
        let expanded = phone_entries(raw, &path, &prefix, localization)?;
        let messages = object(&expanded)?
            .values()
            .filter(|e| e["type"] == "message")
            .map(|e| e["path"].clone())
            .collect::<Vec<_>>();
        thread["messages"] = json!(messages);
        thread["message"] = messages.first().cloned().unwrap_or(json!(""));
        let group = object(&expanded)?
            .values()
            .find(|e| e["type"] == "choice_group");
        thread["choice_group"] = group.map_or(json!(""), |g| g["path"].clone());
        thread["accept_choice"] = group
            .and_then(|g| g["entries"].as_object()?.values().next())
            .map_or(json!(""), |c| c["path"].clone());
        thread["entries"] = expanded;
        return Ok(thread);
    }
    let messages = entries(&thread, "messages")?;
    let choices = entries(&thread, "choices")?;
    let mut ids = messages
        .iter()
        .map(|(k, _)| k.clone())
        .collect::<HashSet<_>>();
    ensure!(
        !ids.contains("choices") && !ids.contains("final"),
        "reserved phone message id"
    );
    ids.extend(["choices".into(), "final".into()]);
    for (key, value) in &messages {
        ensure!(crate::manifest::identifier(key), "invalid message id {key}");
        add_text(localization, &json!(format!("{prefix}_{key}")), value)?;
    }
    thread["message_text"] = Value::Object(messages.iter().cloned().collect());
    thread["choice_text"] = Value::Object(choices.iter().cloned().collect());
    let paths = messages
        .iter()
        .map(|(key, _)| json!(format!("{path}/{key}")))
        .collect::<Vec<_>>();
    thread["message"] = paths.first().cloned().unwrap_or(json!(""));
    thread["messages"] = json!(paths);
    let mut choice_paths = Vec::new();
    for (key, value) in choices {
        ensure!(crate::manifest::identifier(&key), "invalid choice id {key}");
        let choice = record(&value, "text")?;
        fields(&choice, &["text", "reply"])?;
        add_text(
            localization,
            &json!(format!("{prefix}_choice_{key}")),
            &choice["text"],
        )?;
        let mut paths = json!({"choice":format!("{path}/choices/{key}")});
        if let Some(reply) = choice.get("reply") {
            let name = format!("{key}_reply");
            ensure!(ids.insert(name.clone()), "duplicate phone reply {name}");
            add_text(localization, &json!(format!("{prefix}_{name}")), reply)?;
            paths["reply"] = json!(format!("{path}/{name}"));
        }
        choice_paths.push(paths);
    }
    thread["accept_choice"] = choice_paths
        .first()
        .map_or(json!(""), |p| p["choice"].clone());
    thread["choices"] = json!(choice_paths);
    if let Some(final_message) = thread.get("final_message") {
        add_text(
            localization,
            &json!(format!("{prefix}_final")),
            final_message,
        )?;
        thread["final_message"] = json!(format!("{path}/final"));
    }
    thread["text_prefix"] = json!(prefix);
    Ok(thread)
}

/// Expands all composition declarations and all three registered recipes.
/// Input data is never modified, and expansion requires no resource I/O.
pub fn prepare(raw: &Value) -> Result<Composition> {
    let mut manifest = raw.clone();
    let composition = manifest
        .as_object_mut()
        .context("quest must be an object")?
        .remove("composition");
    let Some(config) = composition else {
        return Ok(Composition {
            manifest,
            bindings: json!({}),
            text: BTreeMap::new(),
            configuration: None,
        });
    };
    fields(
        &config,
        &[
            "namespace",
            "quest",
            "objectives",
            "contacts",
            "locations",
            "clues",
            "facts",
            "text",
            "recipes",
            "points_of_interest",
            "readables",
        ],
    )?;
    let namespace = id(config.get("namespace").unwrap_or(&manifest["id"]))?.to_owned();
    let mut bindings = json!({"quest":{"id":namespace,"path":format!("quests/minor_quest/{namespace}"),"title_key":format!("gl_{namespace}_title")},"resources":{"journal":format!("mod\\{namespace}\\journal\\{namespace}.journal"),"onscreens":format!("mod\\{namespace}\\localization\\en-us\\onscreens\\{namespace}.json")},"objectives":{},"contacts":{},"locations":{},"clues":{},"facts":{},"points_of_interest":{},"readables":{}});
    let quest = config.get("quest").cloned().unwrap_or(json!({}));
    fields(&quest, &["title", "localization_key", "type"])?;
    if let Some(key) = quest.get("localization_key") {
        bindings["quest"]["title_key"] = key.clone();
    }
    if let Some(kind) = quest.get("type") {
        bindings["quest"]["type"] = kind.clone();
    }
    let mut localization = Vec::new();
    add_text(
        &mut localization,
        &bindings["quest"]["title_key"],
        quest.get("title").unwrap_or(&manifest["title"]),
    )?;
    for category in ["locations", "facts", "clues", "objectives", "contacts"] {
        for (alias, value) in entries(&config, category)? {
            ensure!(
                crate::manifest::identifier(&alias),
                "invalid {category} alias {alias}"
            );
            let result = match category {
                "facts" => {
                    if value.is_null() {
                        json!(format!("{namespace}_{alias}"))
                    } else {
                        value
                    }
                }
                "locations" => {
                    let mut r = record(&value, "ref")?;
                    put(&mut r, "ref", json!(format!("#{namespace}_{alias}")));
                    r
                }
                "clues" => {
                    let mut r = record(&value, "object_ref")?;
                    fields(
                        &r,
                        &[
                            "id",
                            "object_ref",
                            "completion_fact",
                            "grant_item",
                            "grant_items",
                            "journal_entry",
                            "mappin",
                        ],
                    )?;
                    put(&mut r, "id", json!(alias));
                    put(
                        &mut r,
                        "object_ref",
                        json!(format!("#{namespace}_clue_{alias}")),
                    );
                    put(
                        &mut r,
                        "completion_fact",
                        json!(format!("{namespace}_clue_{alias}_scanned")),
                    );
                    r
                }
                "objectives" => {
                    let mut r = record(&value, "text")?;
                    fields(
                        &r,
                        &[
                            "text",
                            "description",
                            "phase_id",
                            "id",
                            "localization_key",
                            "description_id",
                            "description_key",
                            "mappins",
                            "completion_fact",
                            "optional",
                            "counter",
                        ],
                    )?;
                    put(&mut r, "phase_id", json!(format!("{namespace}_{alias}")));
                    let phase = journal_id(&r["phase_id"])?.to_owned();
                    put(&mut r, "id", json!(format!("{phase}_obj_{alias}")));
                    put(
                        &mut r,
                        "description_id",
                        json!(format!("{phase}_desc_{alias}")),
                    );
                    let objective = journal_id(&r["id"])?.to_owned();
                    let description = journal_id(&r["description_id"])?.to_owned();
                    let path = format!("{}/{phase}/{objective}", text(&bindings["quest"]["path"])?);
                    r["path"] = json!(path);
                    r["description_entry"] = json!(format!("{path}/{description}"));
                    put(
                        &mut r,
                        "localization_key",
                        json!(format!("gl_{namespace}_objective_{alias}")),
                    );
                    put(
                        &mut r,
                        "description_key",
                        json!(format!("gl_{namespace}_description_{alias}")),
                    );
                    put(
                        &mut r,
                        "description",
                        manifest.get("description").cloned().unwrap_or(json!("")),
                    );
                    put(
                        &mut r,
                        "completion_fact",
                        json!(format!("{namespace}_{alias}_complete")),
                    );
                    if let Some(v) = r.get("optional") {
                        ensure!(v.is_boolean(), "objective optional must be boolean");
                    }
                    if let Some(v) = r.get("counter") {
                        ensure!(
                            v.as_u64().is_some(),
                            "objective counter must be nonnegative integer"
                        );
                    }
                    add_text(&mut localization, &r["localization_key"], &r["text"])?;
                    add_text(&mut localization, &r["description_key"], &r["description"])?;
                    let mut pins = Map::new();
                    for (pin_alias, value) in entries(&r, "mappins")? {
                        ensure!(
                            crate::manifest::identifier(&pin_alias),
                            "invalid mappin alias"
                        );
                        let mut pin = record(&value, "ref")?;
                        fields(
                            &pin,
                            &[
                                "ref",
                                "id",
                                "text",
                                "entry",
                                "localization_key",
                                "gps_disabled",
                                "debug_caption",
                                "variant",
                                "offset",
                                "slot_name",
                            ],
                        )?;
                        put(&mut pin, "id", json!(format!("{phase}_qmp_{pin_alias}")));
                        let pin_id = journal_id(&pin["id"])?.to_owned();
                        put(
                            &mut pin,
                            "localization_key",
                            json!(format!("gl_{namespace}_mappin_{alias}_{pin_alias}")),
                        );
                        put(&mut pin, "text", r["text"].clone());
                        pin["path"] = json!(format!("{path}/{pin_id}"));
                        add_text(&mut localization, &pin["localization_key"], &pin["text"])?;
                        pins.insert(pin_alias, pin);
                    }
                    r["mappin"] = pins
                        .values()
                        .next()
                        .map_or(json!(""), |p| p["path"].clone());
                    r["route_mappins"] =
                        json!(pins.values().map(|p| p["path"].clone()).collect::<Vec<_>>());
                    r["mappins"] = Value::Object(pins);
                    r
                }
                "contacts" => {
                    let mut r = value.clone();
                    fields(
                        &r,
                        &[
                            "id",
                            "name",
                            "localization_key",
                            "threads",
                            "community",
                            "entry",
                            "appearance",
                            "scene",
                            "avatar",
                            "callable",
                            "contact_type",
                            "flat_message_layout",
                        ],
                    )?;
                    put(&mut r, "id", json!(format!("{namespace}_{alias}")));
                    let contact = id(&r["id"])?.to_owned();
                    r["contact"] = json!(contact);
                    put(
                        &mut r,
                        "localization_key",
                        json!(format!("gl_{namespace}_contact_{alias}")),
                    );
                    if r.get("name").is_some() || value.get("localization_key").is_none() {
                        add_text(
                            &mut localization,
                            &r["localization_key"],
                            r.get("name").unwrap_or(&json!(alias)),
                        )?;
                    }
                    let mut threads = Map::new();
                    for (name, value) in entries(&r, "threads")? {
                        ensure!(crate::manifest::identifier(&name), "invalid thread alias");
                        threads.insert(
                            name.clone(),
                            thread(
                                value,
                                &namespace,
                                &contact,
                                &alias,
                                &name,
                                &manifest["title"],
                                &mut localization,
                            )?,
                        );
                    }
                    r["threads"] = Value::Object(threads);
                    r
                }
                _ => unreachable!(),
            };
            bindings[category][&alias] = result;
        }
    }
    for (alias, value) in entries(&config, "readables")? {
        ensure!(
            crate::manifest::identifier(&alias),
            "invalid readable alias"
        );
        let mut r = value;
        fields(
            &r,
            &[
                "kind",
                "group",
                "id",
                "title",
                "text",
                "title_key",
                "text_key",
                "tag",
            ],
        )?;
        ensure!(
            matches!(r["kind"].as_str(), Some("file" | "onscreen")),
            "readable kind must be file or onscreen"
        );
        let file = r["kind"] == "file";
        put(
            &mut r,
            "group",
            json!(if file { "files" } else { "shards" }),
        );
        put(&mut r, "id", json!(alias));
        let group = journal_id(&r["group"])?.to_owned();
        let readable = journal_id(&r["id"])?.to_owned();
        ensure!(
            !file || r.get("tag").is_none(),
            "file entries cannot have onscreen tags"
        );
        put(
            &mut r,
            "title_key",
            json!(format!("gl_{namespace}_{alias}_title")),
        );
        put(
            &mut r,
            "text_key",
            json!(format!("gl_{namespace}_{alias}_body")),
        );
        r["path"] = json!(format!(
            "onscreens/emails/quests/minor_quest/{namespace}/{group}/{readable}"
        ));
        add_text(&mut localization, &r["title_key"], &r["title"])?;
        add_text(&mut localization, &r["text_key"], &r["text"])?;
        bindings["readables"][&alias] = r;
    }
    for (key, value) in entries(&config, "text")? {
        add_text(&mut localization, &json!(key), &value)?;
    }
    for (alias, value) in entries(&config, "points_of_interest")? {
        ensure!(crate::manifest::identifier(&alias), "invalid POI alias");
        let mut r = value;
        fields(&r, &["id", "ref", "variant"])?;
        put(&mut r, "id", json!(format!("{namespace}_poi_{alias}")));
        id(&r["id"])?;
        bindings["points_of_interest"][&alias] = r;
    }
    let mut stages = manifest.get("stages").cloned().unwrap_or(json!([]));
    let stages = stages.as_array_mut().context("stages must be an array")?;
    for recipe in config.get("recipes").map_or(Ok(&[][..]), |v| {
        v.as_array()
            .map(Vec::as_slice)
            .context("recipes must be an array")
    })? {
        fields(recipe, &["name", "steps", "prefix", "reward"])?;
        let name = text(&recipe["name"])?;
        let prefix = recipe
            .get("prefix")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !prefix.is_empty() {
            ensure!(crate::manifest::identifier(prefix), "invalid recipe prefix");
        }
        let slots: &[(&str, &str)] = match name {
            "offer_meeting_investigation_decision_debrief" => &[
                ("offer", "phone_job_offer"),
                ("meeting", "meet_contact"),
                ("investigation", "investigate_clues"),
                ("decision", "choice_gate"),
                ("debrief", "phone_conversation"),
            ],
            "rescue_escort_defense_extraction" => &[
                ("rescue", "release_or_rescue_npc"),
                ("escort", "escort_npc"),
                ("defense", "defend_target"),
                ("extraction", "leave_area"),
            ],
            "encounter_evidence_report_reward" => &[
                ("encounter", "combat_encounter"),
                ("evidence", "investigate_clues"),
                ("report", "phone_conversation"),
            ],
            _ => bail!("unknown quest recipe {name}"),
        };
        let steps = object(&recipe["steps"])?;
        ensure!(
            steps.len() == slots.len() && slots.iter().all(|(s, _)| steps.contains_key(*s)),
            "recipe steps must exactly match {name}"
        );
        ensure!(
            (name == "encounter_evidence_report_reward") == recipe.get("reward").is_some(),
            "reward record must be provided only for encounter/report recipe"
        );
        for (slot, kind) in slots {
            let mut stage = json!({"id":if prefix.is_empty(){(*slot).to_owned()}else{format!("{prefix}_{slot}")},"type":kind});
            stage
                .as_object_mut()
                .context("stage")?
                .extend(object(&steps[*slot])?.clone());
            if *slot == "report"
                && let Some(reward) = recipe.get("reward")
            {
                put(&mut stage, "reward", reward.clone());
            }
            stages.push(stage);
        }
    }
    let mut seen = HashSet::new();
    for stage in stages.iter_mut() {
        let name = id(&stage["id"])?.to_owned();
        ensure!(seen.insert(name.clone()), "duplicate expanded stage {name}");
        put(
            stage,
            "phase_resource",
            json!(format!(
                "mod\\{namespace}\\phases\\{namespace}_{name}.questphase"
            )),
        );
    }
    manifest["stages"] = json!(stages);
    let expanded = resolve(&bindings, &bindings)?;
    for value in object(&expanded["facts"])?.values() {
        id(value)?;
    }
    for (category, key) in [
        ("objectives", "path"),
        ("readables", "path"),
        ("contacts", "id"),
        ("points_of_interest", "id"),
    ] {
        let mut seen = HashSet::new();
        for value in object(&expanded[category])?.values() {
            ensure!(
                seen.insert(text(&value[key])?),
                "duplicate {category} {key}"
            );
        }
    }
    let mut groups = BTreeMap::new();
    for r in object(&expanded["readables"])?.values() {
        let group = text(&r["group"])?;
        let kind = text(&r["kind"])?;
        if let Some(old) = groups.insert(group, kind) {
            ensure!(old == kind, "readable group mixes native kinds");
        }
    }
    for contact in object(&expanded["contacts"])?.values() {
        let mut seen = HashSet::new();
        for t in object(&contact["threads"])?.values() {
            ensure!(seen.insert(text(&t["id"])?), "duplicate thread id");
        }
    }
    let mut points = object(&expanded["points_of_interest"])?
        .values()
        .collect::<Vec<_>>();
    for objective in object(&expanded["objectives"])?.values() {
        let mut seen = HashSet::from([text(&objective["description_id"])?]);
        for pin in object(&objective["mappins"])?.values() {
            ensure!(
                seen.insert(text(&pin["id"])?),
                "duplicate objective child id"
            );
            points.push(pin);
        }
    }
    for point in points {
        let reference = text(&point["ref"])?;
        ensure!(
            reference.len() > 1 && (reference.starts_with('#') || reference.starts_with('$')),
            "map marker needs explicit NodeRef"
        );
    }
    let mut text_map = BTreeMap::new();
    for (key, value) in localization {
        let key = resolve(&key, &bindings)?;
        let value = resolve(&value, &bindings)?;
        let key = text(&key)?.to_owned();
        let value = text(&value)?.to_owned();
        ensure!(!key.is_empty(), "empty localization key");
        if let Some(old) = text_map.insert(key.clone(), value.clone()) {
            ensure!(old == value, "conflicting localization text for {key}");
        }
    }
    Ok(Composition {
        manifest: resolve(&manifest, &bindings)?,
        bindings: expanded,
        text: text_map,
        configuration: Some(config),
    })
}
