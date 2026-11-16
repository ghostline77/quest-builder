use super::*;

pub(super) struct Text {
    pub screenplay: Value,
    pub localization: Value,
    pub lines: BTreeMap<String, u32>,
    pub choices: BTreeMap<String, u32>,
    pub spoken: BTreeMap<String, Value>,
}

fn records(dialogue: &Value, key: &str) -> Result<BTreeMap<String, Value>> {
    let mut result = BTreeMap::new();
    for line in list(dialogue, key)? {
        let key = string(line, "key")?;
        ensure!(
            result.insert(key.into(), line.clone()).is_none(),
            "duplicate dialogue key {key}"
        );
    }
    Ok(result)
}
fn loc_id(line: &Value) -> Result<u64> {
    let value = &line["string_id"];
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|v| v.parse().ok()))
        .context("dialogue string_id must be uint64")
}
pub(super) fn actor(actors: &BTreeMap<String, u32>, name: &str) -> Result<u32> {
    actors
        .get(&name.to_lowercase())
        .copied()
        .with_context(|| format!("unknown actor {name}"))
}

pub(super) fn build(
    spec: &Value,
    dialogue: &Value,
    actors: &BTreeMap<String, u32>,
) -> Result<Text> {
    let spoken = records(dialogue, "spoken_lines")?;
    let choices = records(dialogue, "choice_lines")?;
    let mut lines = Vec::new();
    let mut options = Vec::new();
    let mut line_ids = BTreeMap::new();
    let mut option_ids = BTreeMap::new();
    let mut reserved = HashSet::new();
    for line in spoken.values().chain(choices.values()) {
        let id = loc_id(line)?;
        ensure!(
            id != 0 && reserved.insert(id),
            "duplicate/zero dialogue string ID {id}"
        );
    }
    for (index, key) in list(spec, "spoken_line_order")?.iter().enumerate() {
        let key = key.as_str().context("spoken line key")?;
        let line = spoken
            .get(key)
            .with_context(|| format!("missing spoken line {key}"))?;
        let item = u32::try_from(index)?
            .checked_mul(256)
            .and_then(|v| v.checked_add(1))
            .context("screenplay ID overflow")?;
        ensure!(
            line_ids.insert(key.into(), item).is_none(),
            "duplicate spoken line order {key}"
        );
        let mut data = json!({"$type":"scnscreenplayDialogLine","addressee":id("scnActorId",actor(actors,string(line,"addressee")?)?),"speaker":id("scnActorId",actor(actors,string(line,"speaker")?)?),"itemId":id("scnscreenplayItemId",item),"locstringId":id("scnlocLocstringId",loc_id(line)?.to_string()),"usage":{"$type":"scnscreenplayLineUsage","playerGenderMask":{"$type":"scnGenderMask","mask":3}}});
        for (input, field) in [
            ("female_lipsync_animation", "femaleLipsyncAnimationName"),
            ("male_lipsync_animation", "maleLipsyncAnimationName"),
        ] {
            if let Some(name) = line.get(input) {
                data[field] = n::cname(name.as_str().context("lipsync animation name")?);
            }
        }
        lines.push(data);
    }
    let mut ordered = Vec::new();
    for (index, key) in list(spec, "choice_line_order")?.iter().enumerate() {
        let key = key.as_str().context("choice key")?;
        let choice = choices
            .get(key)
            .with_context(|| format!("missing choice {key}"))?;
        let item = u32::try_from(index)?
            .checked_mul(256)
            .and_then(|v| v.checked_add(2))
            .context("choice ID overflow")?;
        ensure!(
            option_ids.insert(key.into(), item).is_none(),
            "duplicate choice order {key}"
        );
        let loc = loc_id(choice)?;
        options.push(json!({"$type":"scnscreenplayChoiceOption","itemId":id("scnscreenplayItemId",item),"locstringId":id("scnlocLocstringId",loc.to_string()),"usage":{"$type":"scnscreenplayOptionUsage","playerGenderMask":{"$type":"scnGenderMask","mask":3}}}));
        ordered.push((loc, key, choice));
    }
    ordered.sort_by_key(|(id, key, _)| (*id, *key));
    let defaults = json!(["db_db", "pl_pl", "en_us"]);
    let locales = spec
        .get("choice_locales")
        .unwrap_or(&defaults)
        .as_array()
        .context("choice locales")?;
    let mut descriptors = Vec::new();
    let mut payloads = Vec::new();
    let mut locale_ids = HashSet::new();
    for locale in locales {
        let locale = locale.as_str().context("locale name")?;
        ensure!(
            locale_ids.insert(locale),
            "duplicate choice locale {locale}"
        );
        for (loc, key, choice) in &ordered {
            for variant in if locale == "db_db" {
                &["blank", "source"][..]
            } else {
                &["text"][..]
            } {
                let mut salt = 0;
                let mut variant_id = hash(&format!(
                    "{}:choice-loc:{key}:{locale}:{loc}:{variant}",
                    string(spec, "name")?
                ));
                while variant_id == i64::MAX as u64 || !reserved.insert(variant_id) {
                    salt += 1;
                    variant_id = hash(&format!(
                        "{}:choice-loc:{key}:{locale}:{loc}:{variant}:{salt}",
                        string(spec, "name")?
                    ));
                }
                let content = if *variant == "blank" {
                    ""
                } else {
                    choice
                        .get("translations")
                        .and_then(|v| v.get(locale))
                        .and_then(Value::as_str)
                        .unwrap_or(string(choice, "text")?)
                };
                descriptors.push(json!({"$type":"scnlocLocStoreEmbeddedVariantDescriptorEntry","localeId":locale,"locstringId":id("scnlocLocstringId",loc.to_string()),"signature":{"$type":"scnlocSignature","val":"3"},"variantId":id("scnlocVariantId",variant_id.to_string()),"vpeIndex":payloads.len()}));
                payloads.push(json!({"$type":"scnlocLocStoreEmbeddedVariantPayloadEntry","content":content,"variantId":id("scnlocVariantId",variant_id.to_string())}));
            }
        }
    }
    Ok(Text {
        screenplay: json!({"$type":"scnscreenplayStore","lines":lines,"options":options}),
        localization: json!({"$type":"scnlocLocStoreEmbedded","vdEntries":descriptors,"vpEntries":payloads}),
        lines: line_ids,
        choices: option_ids,
        spoken,
    })
}
