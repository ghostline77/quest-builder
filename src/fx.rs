//! Native timed billboard effects and independent effect-spawner patches.
use crate::{compiler::Artifacts, nodes as n, project::depot_path};
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub struct Build {
    pub artifacts: Artifacts,
    pub registration: Value,
}
struct Handles(u64);
impl Handles {
    fn wrap(&mut self, data: Value) -> Value {
        let id = self.0;
        self.0 += 1;
        json!({"HandleId":id.to_string(),"Data":data})
    }
}
fn text<'a>(spec: &'a Value, field: &str) -> Result<&'a str> {
    spec[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .with_context(|| format!("effect field {field}"))
}
fn number(spec: &Value, field: &str) -> Result<f64> {
    spec[field]
        .as_f64()
        .filter(|v| v.is_finite() && v.abs() <= f64::from(f32::MAX))
        .with_context(|| format!("finite effect field {field}"))
}
fn reference(path: &str) -> Value {
    json!({"DepotPath":{"$type":"ResourcePath","$storage":"string","$value":path},"Flags":"Default"})
}
fn uid(namespace: &str, name: &str) -> String {
    let bytes = Sha256::digest(format!("ghostline.{namespace}.{name}"));
    u64::from_le_bytes(bytes[..8].try_into().expect("SHA-256 has eight bytes")).to_string()
}
fn document(path: &str, root: Value) -> Result<Value> {
    depot_path(path)?;
    crate::graph::order_handles(
        &json!({"Header":{"WolvenKitVersion":"8.17.4","WKitJsonVersion":"0.0.9","GameVersion":2310,"ExportedDateTime":"1970-01-01T00:00:00Z","DataType":"CR2W","ArchiveFileName":path},"Data":{"Version":195,"BuildVersion":0,"RootChunk":root,"EmbeddedFiles":[]}}),
    )
}
fn frames(spec: &Value) -> Result<Vec<(String, f64, f64)>> {
    let start = number(spec, "loop_start_seconds")?;
    let end = number(spec, "loop_end_seconds")?;
    let hold = spec["hold_frames"]
        .as_u64()
        .filter(|n| (1..=256).contains(n))
        .context("effect hold frame count")?;
    let outro = spec["outro_frames"]
        .as_u64()
        .filter(|n| (1..=256).contains(n))
        .context("effect outro frame count")?;
    let step = number(spec, "outro_frame_seconds")?;
    ensure!(
        start > 0. && end > start && step > 0.,
        "invalid effect timing"
    );
    let mut result = vec![("hearts".into(), 0., start)];
    let hold_step = (end - start) / hold as f64;
    result.extend((0..hold).map(|i| {
        (
            format!("hold_{i:02}"),
            start + i as f64 * hold_step,
            hold_step,
        )
    }));
    result.extend((0..outro).map(|i| (format!("burst_{i:02}"), end + i as f64 * step, step)));
    Ok(result)
}

fn particle(
    spec: &Value,
    variant: &str,
    layer: &str,
    suffix: &str,
    lifetime: f64,
) -> Result<Value> {
    let prefix = format!("mod\\{}", text(spec, "namespace")?);
    let size = number(
        spec,
        if variant == "v" {
            "v_screen_size"
        } else {
            "npc_billboard_size"
        },
    )?;
    ensure!(size > 0., "particle size must be positive");
    let mut h = Handles(0);
    let life = h.wrap(json!({"$type":"CEvaluatorFloatConst","value":lifetime}));
    let life = h.wrap(json!({"$type":"CParticleInitializerLifeTime","lifeTime":life}));
    let size_evaluator=h.wrap(json!({"$type":"CEvaluatorVectorConst","value":{"$type":"Vector4","X":size,"Y":size,"Z":size,"W":1}}));
    let size_initializer =
        h.wrap(json!({"$type":"CParticleInitializerSize","size":size_evaluator,"scale":1}));
    let velocity = h.wrap(json!({"$type":"CEvaluatorVectorConst"}));
    let velocity =
        h.wrap(json!({"$type":"CParticleInitializerVelocity","velocity":velocity,"worldSpace":0}));
    let alpha = h.wrap(json!({"$type":"CParticleModificatorAlphaOverEffect"}));
    let file = if layer == "bows" {
        format!("{suffix}_bows")
    } else {
        suffix.into()
    };
    let texture = format!("{prefix}\\textures\\{variant}_{file}_d.xbm");
    let settings = &spec["layers"][layer];
    let tint = spec["material_tint"]
        .as_array()
        .filter(|v| v.len() == 4)
        .context("effect material tint needs RGBA")?;
    for channel in tint {
        ensure!(
            channel.as_u64().is_some_and(|v| v <= 255),
            "effect tint channel outside byte range"
        );
    }
    let mut values = vec![
        json!({"$type":"rRef:ITexture","Diffuse":reference(&texture)}),
        json!({"$type":"rRef:ITexture","AlphaMask":reference(&texture.replace("_d.xbm","_m.xbm"))}),
        json!({"$type":"Color","Color":{"$type":"Color","Red":tint[0],"Green":tint[1],"Blue":tint[2],"Alpha":tint[3]}}),
    ];
    for (name, value) in [
        ("ColorMultiplier", number(settings, "color_multiplier")?),
        ("UseAlphaMask", 1.),
        ("SubUVWidth", 1.),
        ("SubUVHeight", 1.),
        ("AdditiveAlphaBlend", number(settings, "additive_blend")?),
        ("DistortAmount", 0.),
        ("AlphaGlobal", number(settings, "alpha_global")?),
        ("AlphaSoft", 0.5),
    ] {
        values.push(json!({"$type":"Float",name:value}));
    }
    values.push(json!({"$type":"Vector4","TexCoordSpeed":{"$type":"Vector4"}}));
    let material=h.wrap(json!({"$type":"CMaterialInstance","baseMaterial":reference("engine\\materials\\particles_generic.remt"),"resourceVersion":4,"values":values}));
    let drawer = h.wrap(
        json!({"$type":if variant=="v"{"CParticleDrawerScreen"}else{"CParticleDrawerBillboard"}}),
    );
    let rate = h.wrap(json!({"$type":"CEvaluatorFloatConst","value":0}));
    // Native v5 constant initializer records: lifetime, XY size, zero velocity.
    let mut bytes = Vec::with_capacity(61);
    for (record_size, values) in [
        (12_u16, vec![lifetime as f32; 2]),
        (20, vec![size as f32; 4]),
        (28, vec![0.; 6]),
    ] {
        bytes.extend(2_u16.to_le_bytes());
        bytes.extend(record_size.to_le_bytes());
        for value in values {
            bytes.extend(value.to_le_bytes());
        }
    }
    bytes.push(0);
    let blob=h.wrap(json!({"$type":"rendRenderParticleBlob","header":{"$type":"rendRenderParticleBlobHeader","version":5,"emitterInfo":{"$type":"rendRenderParticleBlobEmitterInfo","backLightingFactor":0.5,"maxParticles":1,"keepSimulationLocal":1,"vertexDrawerType":if variant=="v"{1}else{0},"renderObjectType":"ROT_Static","numModifiers":1,"modifierSetMask":"33554432","numInitializers":3,"initializerSetMask":"8454160","seeds":[0,0,0],"emitterLoops":1,"lods":[{"$type":"rendEmitterLOD","birthRate":[0,0],"burstList":[{"$type":"rendParticleBurst","spawnCount":1}],"isEnabled":1}],"volumetricParticleSize":1,"volumetricParticleDensity":1,"volumetricParticleFalloff":1}},"updaterData":{"$type":"rendRenderParticleUpdaterData","data":{"BufferId":"0","Flags":4063232,"Bytes":STANDARD.encode(bytes)},"modifOffset":61}}));
    let emitter=h.wrap(json!({"$type":"CParticleEmitter","editorName":format!("{variant} {layer} {suffix}"),"modules":[life,size_initializer,velocity,alpha],"material":reference("engine\\materials\\particles_generic.remt"),"localMaterialInstance":material,"maxParticles":1,"particleDrawer":drawer,"keepSimulationLocal":1,"emitterLoops":1,"lods":[{"$type":"SParticleEmitterLODLevel","birthRate":rate,"burstList":[{"$type":"ParticleBurst","spawnCount":1}]}],"renderResourceBlob":blob}));
    Ok(json!({"$type":"CParticleSystem","cookingPlatform":"PLATFORM_PC","emitters":[emitter]}))
}

fn effect(
    spec: &Value,
    variant: &str,
    frames: &[(String, f64, f64)],
) -> Result<(Value, Vec<Value>)> {
    let namespace = text(spec, "namespace")?;
    let prefix = format!("mod\\{namespace}");
    let mut h = Handles(0);
    let mut tracks = Vec::new();
    let mut events = Vec::new();
    let mut placements = Vec::new();
    let start = number(spec, "loop_start_seconds")?;
    let end = number(spec, "loop_end_seconds")?;
    let mut items = vec![
        json!({"$type":"effectTrackItemLoopMarker","timeBegin":start,"timeDuration":end-start,"ruid":uid(namespace,&format!("{variant}.loop"))}),
    ];
    for (suffix, begin, duration) in frames {
        for layer in ["hearts", "bows"] {
            let file = if layer == "bows" {
                format!("{suffix}_bows")
            } else {
                suffix.clone()
            };
            let mut item = json!({"$type":"effectTrackItemParticles","timeBegin":begin,"timeDuration":duration,"ruid":uid(namespace,&format!("{variant}.{file}")),"particleSystem":reference(&format!("{prefix}\\fx\\{variant}_{file}.particle")),"soundPositionName":n::cname("glitch")});
            for field in ["emissionScale", "alpha", "size", "velocity"] {
                item[field] = json!({"$type":"effectEffectParameterEvaluatorFloat","evaluator":h.wrap(json!({"$type":"CEvaluatorFloatConst","value":1}))});
            }
            items.push(item);
        }
    }
    let rtpc = h.wrap(json!({"$type":"CEvaluatorFloatConst","value":0}));
    items.push(json!({"$type":"effectTrackItemSound","timeBegin":end+0.005,"timeDuration":0.1,"ruid":uid(namespace,&format!("{variant}.sound")),"eventName":n::cname(text(spec,"sound_event")?),"positionName":n::cname("glitch"),"rtpcValue":rtpc}));
    for item in items {
        if item["$type"] != "effectTrackItemLoopMarker" {
            placements.push(json!({"$type":"worldCompiledEffectEventInfo","eventRUID":item["ruid"],"componentIndexMask":"0","flags":1,"placementIndexMask":"1"}));
        }
        let item = h.wrap(item);
        events.push(json!({"HandleRefId":item["HandleId"]}));
        tracks.push(h.wrap(json!({"$type":"effectTrack","items":[item]})));
    }
    placements.sort_by_key(|item| {
        item["eventRUID"]
            .as_str()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or_default()
    });
    let length = frames
        .last()
        .map(|(_, begin, duration)| begin + duration)
        .context("effect has no frames")?;
    Ok((
        json!({"$type":"worldEffect","cookingPlatform":"PLATFORM_PC","length":length,"trackRoot":h.wrap(json!({"$type":"effectTrackGroup","tracks":tracks})),"events":events,"effectLoops":[{"$type":"effectLoopData","startTime":start,"endTime":end}]}),
        placements,
    ))
}

fn spawner(spec: &Value, variant: &str, events: Vec<Value>) -> Result<Value> {
    let namespace = text(spec, "namespace")?;
    let effect = format!("mod\\{namespace}\\fx\\{namespace}_{variant}.effect");
    // entEffectDesc.effect is an async reference. In its embedded RedPackage,
    // Default encodes the async import (Soft/Obligatory sets the sync bit).
    // Do not repeat it in resolvedDependencies: that preloads the entire
    // particle/texture sequence whenever a patched NPC spawns.
    let effect_reference = reference(&effect);
    let component_id = uid(namespace, &format!("{variant}.component"));
    let descriptor = json!({"$type":"entEffectDesc","id":uid(namespace,&format!("{variant}.descriptor")),"effectName":n::cname(text(spec,"fx_name")?),"effect":effect_reference,"isAutoSpawn":false,"compiledEffectInfo":{"$type":"worldCompiledEffectInfo","placementTags":[n::cname("Head")],"relativePositions":[{"$type":"Vector3","X":0,"Y":0,"Z":if variant=="npc"{0.18}else{0.}}],"relativeRotations":[{"$type":"Quaternion","i":0,"j":0,"k":0,"r":1}],"placementInfos":[{"$type":"worldCompiledEffectPlacementInfo","flags":3,"placementTagIndex":0,"relativePositionIndex":0,"relativeRotationIndex":0}],"eventsSortedByRUID":events}});
    let component = json!({"$type":"entEffectSpawnerComponent","name":n::cname(&format!("ghostline_{namespace}_fx")),"id":component_id,"effectDescs":[{"HandleId":"0","Data":descriptor}]});
    Ok(
        json!({"$type":"entEntityTemplate","cookingPlatform":"PLATFORM_PC","compiledData":{"BufferId":"0","Flags":4063232,"Type":"WolvenKit.RED4.Archive.Buffer.RedPackage, WolvenKit.RED4, Version=8.17.4.0, Culture=neutral, PublicKeyToken=null","Data":{"Version":4,"Sections":7,"CruidIndex":0,"CruidDict":{"0":"0","1":component_id},"Chunks":[{"$type":"entEntity"},component]}},"resolvedDependencies":[]}),
    )
}

/// Generates fresh particles, tracks and spawner resources from a timed texture sequence.
pub fn compile(spec: &Value) -> Result<Build> {
    let namespace = text(spec, "namespace")?;
    ensure!(
        crate::manifest::identifier(namespace),
        "invalid effect namespace"
    );
    let sequence = frames(spec)?;
    let mut artifacts = Artifacts::new();
    let mut patches = serde_json::Map::new();
    for variant in ["v", "npc"] {
        for (suffix, _, duration) in &sequence {
            for layer in ["hearts", "bows"] {
                let file = if layer == "bows" {
                    format!("{suffix}_bows")
                } else {
                    suffix.clone()
                };
                let depot = format!("mod\\{namespace}\\fx\\{variant}_{file}.particle");
                artifacts.insert(
                    depot.clone(),
                    document(&depot, particle(spec, variant, layer, suffix, *duration)?)?,
                );
            }
        }
        let (root, events) = effect(spec, variant, &sequence)?;
        let depot = format!("mod\\{namespace}\\fx\\{namespace}_{variant}.effect");
        artifacts.insert(depot.clone(), document(&depot, root)?);
        let depot = format!("mod\\{namespace}\\registration\\{namespace}_{variant}.ent");
        artifacts.insert(
            depot.clone(),
            document(&depot, spawner(spec, variant, events)?)?,
        );
        let targets = spec["targets"][variant]
            .as_array()
            .context("effect patch targets")?;
        for target in targets {
            depot_path(target.as_str().context("effect target depot")?)?;
        }
        patches.insert(depot, json!(targets));
    }
    Ok(Build {
        artifacts,
        registration: json!({"resource":{"patch":patches}}),
    })
}
