//! Direct native website, widget, atlas and font-family resource generation.
use crate::{compiler::Artifacts, nodes as n, project::depot_path};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub struct Build {
    pub artifacts: Artifacts,
    pub registration: Value,
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|v| !v.is_empty())
        .with_context(|| format!("UI field {key}"))
}
fn rows<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    value[key]
        .as_array()
        .map(Vec::as_slice)
        .with_context(|| format!("UI list {key}"))
}
fn document(depot: &str, root: Value) -> Result<Value> {
    depot_path(depot)?;
    Ok(
        json!({"Header":{"WolvenKitVersion":"8.17.4","WKitJsonVersion":"0.0.9","GameVersion":2310,"ExportedDateTime":"1970-01-01T00:00:00Z","DataType":"CR2W","ArchiveFileName":depot},"Data":{"Version":195,"BuildVersion":0,"RootChunk":root,"EmbeddedFiles":[]}}),
    )
}
fn colour(hex: &str) -> Result<Value> {
    let hex = hex
        .strip_prefix('#')
        .context("UI colour must use #RRGGBB")?;
    ensure!(
        hex.len() == 6 && hex.is_ascii(),
        "UI colour must use #RRGGBB"
    );
    let byte = |i| u8::from_str_radix(&hex[i..i + 2], 16).context("UI colour channel");
    Ok(
        json!({"$type":"HDRColor","Red":f64::from(byte(0)?)/255.,"Green":f64::from(byte(2)?)/255.,"Blue":f64::from(byte(4)?)/255.,"Alpha":1.}),
    )
}
fn numeric(value: &Value, key: &str) -> Result<f64> {
    value[key]
        .as_f64()
        .filter(|v| v.is_finite())
        .with_context(|| format!("finite UI coordinate {key}"))
}

/// Builds resolution slots and texture-part UVs from authored atlas metadata.
pub fn atlas(spec: &Value) -> Result<Value> {
    let textures = rows(spec, "textures")?;
    ensure!(
        textures.len() == 3,
        "ink atlases require three resolution slots"
    );
    let mut names = BTreeSet::new();
    let mut parts = Vec::new();
    for part in rows(spec, "parts")? {
        let name = string(part, "name")?;
        ensure!(names.insert(name), "duplicate atlas part");
        let uv = rows(part, "uv")?;
        ensure!(uv.len() == 4, "atlas UV rectangle needs four values");
        let uv = uv
            .iter()
            .map(|v| {
                v.as_f64()
                    .filter(|v| v.is_finite() && (0. ..=1.).contains(v))
                    .context("atlas UV outside 0..1")
            })
            .collect::<Result<Vec<_>>>()?;
        ensure!(uv[0] < uv[2] && uv[1] < uv[3], "empty atlas part");
        parts.push(json!({"$type":"inkTextureAtlasMapper","partName":n::cname(name),"clippingRectInUVCoords":{"$type":"RectF","Left":uv[0],"Top":uv[1],"Right":uv[2],"Bottom":uv[3]}}));
    }
    let mut slots = Vec::new();
    for texture in textures {
        let path = texture.as_str().context("atlas texture path")?;
        depot_path(path)?;
        slots.push(json!({"$type":"inkTextureSlot","texture":n::resource(path),"parts":parts}));
    }
    document(
        string(spec, "depot")?,
        json!({"$type":"inkTextureAtlas","cookingPlatform":"PLATFORM_PC","slots":{"Elements":slots}}),
    )
}

/// Links named styles to published font resources.
pub fn font_family(spec: &Value) -> Result<Value> {
    let mut styles = Vec::new();
    let mut names = BTreeSet::new();
    for style in rows(spec, "styles")? {
        let name = string(style, "name")?;
        ensure!(names.insert(name), "duplicate font style");
        let path = string(style, "font")?;
        depot_path(path)?;
        let mut resource = n::resource(path);
        resource["Flags"] = json!("Default");
        styles.push(json!({"$type":"inkFontStyle","styleName":n::cname(name),"font":resource}));
    }
    document(
        string(spec, "depot")?,
        json!({"$type":"inkFontFamilyResource","cookingPlatform":"PLATFORM_PC","familyName":n::cname(string(spec,"name")?),"fontStyles":styles}),
    )
}

struct Handles(u32);
impl Handles {
    fn wrap(&mut self, data: Value) -> Value {
        let id = self.0;
        self.0 += 1;
        json!({"HandleId":id.to_string(),"Data":data})
    }
}

fn widget(spec: &Value, page: &Value, depot: &str) -> Result<Value> {
    let mut h = Handles(2);
    let mut children = Vec::new();
    let mut texts = Vec::new();
    let mut dependencies = BTreeSet::new();
    let mut names = BTreeSet::new();
    for node in rows(page, "layout")? {
        let name = string(node, "name")?;
        ensure!(names.insert(name), "duplicate website widget name");
        let width = numeric(node, "w")?;
        let height = numeric(node, "h")?;
        ensure!(width > 0. && height > 0., "widget size must be positive");
        let link = node.get("link").and_then(Value::as_str).unwrap_or("");
        let mut data = json!({"name":n::cname(name),"parentWidget":{"HandleRefId":"0"},"visible":1,"fitToContent":0,"isInteractive":u8::from(!link.is_empty()),"size":{"$type":"Vector2","X":width,"Y":height},"layout":{"$type":"inkWidgetLayout","anchor":"TopLeft","margin":{"$type":"inkMargin","left":numeric(node,"x")?,"top":numeric(node,"y")?}}});
        match string(node, "kind")? {
            "text" => {
                let font = &spec["fonts"][string(node, "font")?];
                let family = string(font, "family")?;
                depot_path(family)?;
                dependencies.insert(family.to_owned());
                data["$type"] = json!("inkTextWidget");
                data["fontFamily"] = n::resource(family);
                data["fontStyle"] = n::cname(string(font, "style")?);
                let size = node["size"]
                    .as_u64()
                    .filter(|v| *v > 0)
                    .context("font size must be a positive integer")?;
                data["fontSize"] = json!(size);
                data["tintColor"] = colour(string(node, "color")?)?;
                data["text"] = json!(string(node, "text")?);
                data["wrappingInfo"] = json!({"$type":"textWrappingInfo","autoWrappingEnabled":1,"wrappingAtPosition":width});
                if !link.is_empty() {
                    data["logicController"]=h.wrap(json!({"$type":"LinkController","defaultColor":colour(string(node,"color")?)?,"hoverColor":colour(node.get("hover_color").and_then(Value::as_str).unwrap_or(string(node,"color")?))?}));
                }
            }
            "rectangle" => {
                data["$type"] = json!("inkRectangleWidget");
                data["tintColor"] = colour(string(node, "color")?)?;
            }
            "image" => {
                let atlas = string(spec, "atlas")?;
                depot_path(atlas)?;
                dependencies.insert(atlas.to_owned());
                data["$type"] = json!("inkImageWidget");
                data["textureAtlas"] = n::resource(atlas);
                data["texturePart"] = n::cname(string(node, "part")?);
                data["tintColor"] = colour("#ffffff")?;
            }
            kind => anyhow::bail!("unsupported native widget kind {kind}"),
        }
        let item = h.wrap(data);
        if node["kind"] == "text" {
            texts.push(
                json!({"$type":"inkTextWidgetReference","widget":{"HandleRefId":item["HandleId"]}}),
            );
        }
        children.push(item);
    }
    let size = spec
        .get("size")
        .and_then(Value::as_array)
        .filter(|v| v.len() == 2)
        .context("website canvas needs width and height")?;
    ensure!(
        size.iter()
            .all(|v| v.as_f64().is_some_and(|v| v.is_finite() && v > 0.)),
        "website canvas dimensions must be positive and finite"
    );
    let controller = h.wrap(json!({"$type":"WebPage","textList":texts}));
    let identity = h.wrap(json!({"$type":"inkRootWidgetIdentifier"}));
    let root = json!({"$type":"inkCanvasWidget","name":n::cname("Root"),"isInteractive":1,"size":{"$type":"Vector2","X":size[0],"Y":size[1]},"children":{"HandleId":"1","Data":{"$type":"inkMultiChildren","children":children}},"logicController":controller,"userData":[identity]});
    let inner = json!({"Version":195,"BuildVersion":0,"RootChunk":{"$type":"inkWidgetLibraryItemInstance","rootResolution":"UltraHD_3840_2160","rootWidget":{"HandleId":"0","Data":root}}});
    document(
        depot,
        json!({"$type":"inkWidgetLibraryResource","cookingPlatform":"PLATFORM_PC","rootResolution":"UltraHD_3840_2160","rootDefinitionIndex":0,"externalDependenciesForInternalItems":dependencies.iter().map(|p|n::resource(p)).collect::<Vec<_>>(),"libraryItems":[{"$type":"inkWidgetLibraryItem","name":n::cname("Root"),"package":{"Flags":0,"Type":"WolvenKit.RED4.Archive.Buffer.CR2WWrapper, WolvenKit.RED4, Version=8.17.4.0, Culture=neutral, PublicKeyToken=null","Data":{"File":inner}}}]}),
    )
}

/// Constructs website registration, journal page bindings, localization and widgets.
pub fn website(spec: &Value) -> Result<Build> {
    let namespace = spec
        .get("namespace")
        .or_else(|| spec.get("quest"))
        .and_then(Value::as_str)
        .context("website namespace")?;
    ensure!(
        crate::manifest::identifier(namespace),
        "invalid website namespace"
    );
    let site = string(spec, "site_id")?;
    ensure!(crate::manifest::identifier(site), "invalid website ID");
    let depot = string(spec, "depot_root")?;
    depot_path(depot)?;
    let pages = spec["pages"]
        .as_object()
        .filter(|p| !p.is_empty())
        .context("website pages")?;
    let home = string(spec, "home")?;
    ensure!(pages.contains_key(home), "website home page absent");
    let mut artifacts = Artifacts::new();
    let mut entries = Vec::new();
    let mut localization = BTreeMap::new();
    let mut h = Handles(0);
    let site_key = format!("gl_{namespace}_web_site");
    localization.insert(site_key.clone(), string(spec, "site_name")?.to_owned());
    for (id, page) in pages {
        ensure!(crate::manifest::identifier(id), "invalid website page ID");
        let widget_path = format!("{depot}\\{id}.inkwidget");
        artifacts.insert(widget_path.clone(), widget(spec, page, &widget_path)?);
        let mut texts = Vec::new();
        for node in rows(page, "layout")?.iter().filter(|n| n["kind"] == "text") {
            let name = string(node, "name")?;
            let key = format!("gl_{namespace}_web_{id}_{name}");
            localization.insert(key.clone(), string(node, "text")?.to_owned());
            // WebPage.FillPageFromJournal preserves the widget's authored tint
            // only for white with zero alpha. Transparent black hides the text.
            let preserve_tint = json!({"$type":"Color","Alpha":0,"Blue":255,"Green":255,"Red":255});
            texts.push(h.wrap(json!({"$type":"gameJournalInternetText","hoverTintColor":preserve_tint,"tintColor":preserve_tint,"name":n::cname(name),"text":crate::localization::reference(&key),"linkAddress":node.get("link").and_then(Value::as_str).unwrap_or("")})));
        }
        let facts = if page.get("read_fact").is_some() {
            let fact = string(page, "read_fact")?;
            ensure!(crate::manifest::identifier(fact), "invalid page-read fact");
            json!([{"$type":"gameJournalFactNameValue","factName":n::cname(fact),"factValue":1}])
        } else {
            json!([])
        };
        entries.push(h.wrap(json!({"$type":"gameJournalInternetPage","id":id,"activatedAtStart":1,"additionallyFilledFromScripts":0,"address":string(page,"address")?,"canvases":[],"factsToSet":facts,"images":[],"journalEntryOverrideDataList":[],"rectangles":[],"reloadOnZoomIn":1,"scale":1,"texts":texts,"videos":[],"widgetFile":n::resource(&widget_path)})));
    }
    let main=h.wrap(json!({"$type":"gameJournalPath","className":n::cname("gameJournalInternetPage"),"editorPath":"","fileEntryIndex":1,"realPath":format!("internet_sites/{site}/{home}")}));
    let site=h.wrap(json!({"$type":"gameJournalInternetSite","id":site,"entries":entries,"ignoredAtDesktop":0,"journalEntryOverrideDataList":[],"mainPagePath":main,"shortName":crate::localization::reference(&site_key),"textureAtlas":spec.get("atlas").and_then(Value::as_str).map(n::resource).unwrap_or_else(||n::resource("")),"texturePart":n::cname(spec.get("icon_part").and_then(Value::as_str).unwrap_or("None"))}));
    let primary=h.wrap(json!({"$type":"gameJournalPrimaryFolderEntry","id":"internet_sites","entries":[site],"journalEntryOverrideDataList":[]}));
    let root=h.wrap(json!({"$type":"gameJournalRootFolderEntry","id":"","descriptor":n::resource("base\\journal\\descriptor.journaldesc"),"entries":[primary],"journalEntryOverrideDataList":[]}));
    let journal = format!(
        "mod\\{namespace}\\journal\\{}.journal",
        string(spec, "name")?
    );
    artifacts.insert(
        journal.clone(),
        crate::graph::order_handles(&document(
            &journal,
            json!({"$type":"gameJournalResource","cookingPlatform":"PLATFORM_PC","entry":root}),
        )?)?,
    );
    let text_path = format!(
        "mod\\{namespace}\\localization\\en-us\\onscreens\\{}.json",
        string(spec, "name")?
    );
    let text=localization.into_iter().map(|(key,value)|crate::localization::onscreen_entry(&key,&value)).collect::<Vec<_>>();
    artifacts.insert(text_path.clone(),document(&text_path,json!({"$type":"JsonResource","cookingPlatform":"PLATFORM_PC","root":{"HandleId":"0","Data":{"$type":"localizationPersistenceOnScreenEntries","entries":text}}}))?);
    Ok(Build {
        artifacts,
        registration: json!({"journal":[journal],"localization":{"onscreens":{"en-us":[text_path]}}}),
    })
}
