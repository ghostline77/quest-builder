//! Generates subtitle and voiceover resources from reviewed text and published audio paths.
use crate::{compiler::Artifacts, nodes as n, project::depot_path};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Stable native onscreen identity derived from an authored symbolic key.
pub fn onscreen_id(key: &str) -> u64 {
    key.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

/// A primary-key reference understood without secondary-key runtime hooks.
pub fn reference(key: &str) -> Value {
    let value = if key.is_empty() || key.starts_with("LocKey#") {
        key.to_owned()
    } else {
        format!("LocKey#{}", onscreen_id(key))
    };
    json!({"unk1":"0","value":value})
}

/// Generates matching primary and descriptive secondary keys for native text.
pub fn onscreen_entry(key: &str, text: &str) -> Value {
    json!({"$type":"localizationPersistenceOnScreenEntry","primaryKey":onscreen_id(key).to_string(),"secondaryKey":key,"femaleVariant":text,"maleVariant":""})
}

/// Generates the engine's scene/voice/animation lookup from generated scenes.
/// Slot identities are taken from each actor's native scene definition.
pub fn lipsync_maps(scenes: &Artifacts, locale: &str) -> Result<Build> {
    let mut groups = BTreeMap::<String, Vec<(String, Value)>>::new();
    for (depot, document) in scenes {
        let root = &document["Data"]["RootChunk"];
        if root["$type"] != "scnSceneResource" {
            continue;
        }
        let mut tags = Vec::new();
        let mut sets = Vec::new();
        let references = root["resouresReferences"]["lipsyncAnimSets"]
            .as_array()
            .context("scene lipsync slots")?;
        for actor in root["actors"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(root["playerActors"].as_array().into_iter().flatten())
        {
            let tag = &actor["voicetagId"]["id"];
            let id = tag
                .as_u64()
                .or_else(|| tag.as_str().and_then(|v| v.parse().ok()))
                .context("actor voice identity")?;
            if id == 0 {
                continue;
            }
            let slot = actor["lipsyncAnimSet"]["id"]
                .as_u64()
                .context("actor lipsync slot")?;
            if slot == u64::from(u32::MAX) {
                continue;
            }
            let slot = usize::try_from(slot).context("lipsync slot range")?;
            let reference = references.get(slot).context("actor lipsync slot absent")?;
            let asset = &reference["asyncRefLipsyncAnimSet"];
            ensure!(
                asset["DepotPath"]["$value"]
                    .as_str()
                    .is_some_and(|v| !v.is_empty()),
                "empty actor lipsync resource"
            );
            ensure!(
                !tags.contains(&json!(id.to_string())),
                "duplicate actor voice tag in scene lipmap"
            );
            tags.push(json!(id.to_string()));
            let mut asset = asset.clone();
            asset["Flags"] = json!("Soft");
            sets.push(asset);
        }
        if tags.is_empty() {
            continue;
        }
        let mut parts = depot.split('\\');
        ensure!(
            parts.next() == Some("mod"),
            "generated scene lipmaps require a mod-owned namespace"
        );
        let namespace = parts.next().context("scene namespace")?;
        let path = format!("mod\\{namespace}\\localization\\{locale}\\{namespace}.lipmap");
        groups.entry(path).or_default().push((
            ghostline_red::archive::depot_path_hash(depot).to_string(),
            json!({"$type":"animLipsyncMappingSceneEntry","actorVoiceTags":tags,"animSets":sets}),
        ));
    }
    let paths = groups.keys().cloned().collect::<Vec<_>>();
    let mut artifacts = Artifacts::new();
    for (path, entries) in groups {
        let (scene_paths, scene_entries): (Vec<_>, Vec<_>) = entries.into_iter().unzip();
        artifacts.insert(path.clone(),json!({"Header":{"WolvenKitVersion":"8.17.4","WKitJsonVersion":"0.0.9","GameVersion":2310,"ExportedDateTime":"1970-01-01T00:00:00Z","DataType":"CR2W","ArchiveFileName":path},"Data":{"Version":195,"BuildVersion":0,"RootChunk":{"$type":"animLipsyncMapping","cookingPlatform":"PLATFORM_PC","languageCodeName":n::cname(locale),"scenePaths":scene_paths,"sceneEntries":scene_entries},"EmbeddedFiles":[]}}));
    }
    Ok(Build {
        artifacts,
        registration: if paths.is_empty() {
            json!({})
        } else {
            json!({"localization":{"lipmaps":{locale:paths}}})
        },
    })
}

/// Complete localization resources and ArchiveXL registration for one dialogue/locale.
pub struct Build {
    pub artifacts: Artifacts,
    pub registration: Value,
}

fn resource(depot: &str, kind: &str, entries: Vec<Value>) -> Value {
    json!({"Header":{"WolvenKitVersion":"8.17.4","WKitJsonVersion":"0.0.9","GameVersion":2310,"ExportedDateTime":"1970-01-01T00:00:00Z","DataType":"CR2W","ArchiveFileName":depot},"Data":{"Version":195,"BuildVersion":0,"RootChunk":{"$type":"JsonResource","cookingPlatform":"PLATFORM_PC","root":{"HandleId":"0","Data":{"$type":kind,"entries":entries}}},"EmbeddedFiles":[]}})
}

/// No WEM conversion, synthesis, audio model, base resource or donor is used.
pub fn compile(spec: &Value, manifest: &Value) -> Result<Build> {
    let object = spec.as_object().context("localization specification")?;
    for key in object.keys() {
        ensure!(
            [
                "namespace",
                "dialogue",
                "locale",
                "manifest",
                "subtitle_depot",
                "subtitle_map_depot",
                "voiceover_depot"
            ]
            .contains(&key.as_str()),
            "unknown localization field {key}"
        );
    }
    let namespace = spec["namespace"]
        .as_str()
        .context("localization namespace")?;
    ensure!(
        crate::manifest::identifier(namespace),
        "invalid localization namespace"
    );
    let dialogue = spec["dialogue"]
        .as_str()
        .filter(|v| {
            !v.is_empty()
                && v.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        })
        .context("dialogue must be a path atom")?;
    let locale = spec
        .get("locale")
        .map_or(Ok("en-us"), |v| v.as_str().context("localization locale"))?;
    ensure!(
        locale.len() == 5
            && locale.as_bytes()[2] == b'-'
            && locale
                .bytes()
                .enumerate()
                .all(|(i, c)| i == 2 || c.is_ascii_lowercase()),
        "locale must use xx-xx form"
    );
    let directory = format!("mod\\{namespace}\\localization\\{locale}");
    let path = |key: &str, default: String| -> Result<String> {
        let path = spec.get(key).map_or(Ok(default), |v| {
            Ok::<_, anyhow::Error>(
                v.as_str()
                    .context("localization depot path")?
                    .replace('/', "\\"),
            )
        })?;
        depot_path(&path)?;
        ensure!(
            path.ends_with(".json"),
            "localization resources must end in .json"
        );
        Ok(path)
    };
    let subtitles = path(
        "subtitle_depot",
        format!("{directory}\\subtitles\\{dialogue}.json"),
    )?;
    let subtitle_map = path(
        "subtitle_map_depot",
        format!("{directory}\\subtitles\\{dialogue}_subtitles_map.json"),
    )?;
    let voiceover = path(
        "voiceover_depot",
        format!("{directory}\\vo\\{dialogue}.json"),
    )?;
    ensure!(
        BTreeSet::from([
            subtitles.to_ascii_lowercase(),
            subtitle_map.to_ascii_lowercase(),
            voiceover.to_ascii_lowercase()
        ])
        .len()
            == 3,
        "localization output paths collide"
    );
    let lines = manifest["spoken_lines"]
        .as_array()
        .filter(|v| !v.is_empty())
        .context("localization needs spoken_lines")?;
    let mut ids = BTreeSet::new();
    let mut text = Vec::new();
    let mut audio = Vec::new();
    for line in lines {
        let id = line["string_id"]
            .as_u64()
            .or_else(|| {
                line["string_id"]
                    .as_str()
                    .and_then(|v| v.parse::<u64>().ok())
            })
            .context("dialogue string_id must be uint64")?;
        ensure!(
            id != 0 && ids.insert(id),
            "duplicate or zero dialogue string ID {id}"
        );
        let base = line["text"]
            .as_str()
            .filter(|v| !v.trim().is_empty())
            .context("spoken line needs text")?;
        let variant = |key| {
            line.get(key).map_or(Ok(base), |v| {
                v.as_str()
                    .filter(|v| !v.trim().is_empty())
                    .context("spoken text variant must not be empty")
            })
        };
        let female = variant("female_text")?;
        let male = variant("male_text")?;
        let female_audio = line["audio_path"]
            .as_str()
            .context("spoken line needs published audio_path")?
            .replace('/', "\\");
        let male_audio = line
            .get("male_audio_path")
            .map_or(Ok(female_audio.clone()), |v| {
                Ok::<_, anyhow::Error>(v.as_str().context("male audio path")?.replace('/', "\\"))
            })?;
        for path in [&female_audio, &male_audio] {
            depot_path(path)?;
            ensure!(
                path.to_ascii_lowercase().ends_with(".wem"),
                "published audio paths must reference WEM files"
            );
        }
        text.push(json!({"$type":"localizationPersistenceSubtitleEntry","femaleVariant":female,"maleVariant":male,"stringId":id.to_string()}));
        audio.push(json!({"$type":"locVoLineEntry","femaleResPath":n::resource(&female_audio),"maleResPath":n::resource(&male_audio),"stringId":id.to_string()}));
    }
    let mut subtitle_file = n::resource(&subtitles);
    subtitle_file["Flags"] = json!("Soft");
    let map = vec![
        json!({"$type":"localizationPersistenceSubtitleMapEntry","subtitleFile":subtitle_file,"subtitleGroup":n::cname("quest")}),
    ];
    let artifacts = BTreeMap::from([
        (
            subtitles.clone(),
            resource(&subtitles, "localizationPersistenceSubtitleEntries", text),
        ),
        (
            subtitle_map.clone(),
            resource(&subtitle_map, "localizationPersistenceSubtitleMap", map),
        ),
        (
            voiceover.clone(),
            resource(&voiceover, "locVoiceoverMap", audio),
        ),
    ]);
    Ok(Build {
        artifacts,
        registration: json!({"localization":{"subtitles":{locale:[subtitle_map]},"vomaps":{locale:[voiceover]}}}),
    })
}
