use ghostline_quest_builder::{native, ui};
use serde_json::{Value, json};

fn site() -> Value {
    json!({
        "namespace":"fixture", "site_id":"fixture_site", "name":"site",
        "site_name":"Fixture", "depot_root":"mod\\fixture\\ui", "home":"home",
        "size":[1920,1440], "atlas":"mod\\fixture\\ui\\icons.inkatlas", "icon_part":"logo",
        "fonts":{"sans":{"family":"mod\\fixture\\ui\\font.inkfontfamily","style":"Regular"}},
        "pages":{
            "home":{"address":"NETdir://fixture.pub", "read_fact":"fixture_page_read", "layout":[
                {"kind":"rectangle","name":"Background","x":0,"y":0,"w":1920,"h":1440,"color":"#291c2e"},
                {"kind":"text","name":"Title","x":84,"y":80,"w":800,"h":120,"size":72,"color":"#f0e8d6","font":"sans","text":"Open profile","link":"NETdir://fixture.pub/profile"},
                {"kind":"image","name":"Logo","x":84,"y":220,"w":200,"h":200,"part":"logo"}
            ]},
            "profile":{"address":"NETdir://fixture.pub/profile","layout":[
                {"kind":"text","name":"Title","x":84,"y":80,"w":800,"h":120,"size":72,"color":"#f0e8d6","font":"sans","text":"Profile"}
            ]}
        }
    })
}

#[test]
fn website_binds_links_read_facts_and_localized_text() {
    let build = ui::website(&site()).unwrap();
    assert_eq!(build.artifacts.len(), 4);
    let journal =
        serde_json::to_string(&build.artifacts["mod\\fixture\\journal\\site.journal"]).unwrap();
    assert!(journal.contains("fixture_page_read"));
    assert!(journal.contains("NETdir://fixture.pub/profile"));
    let site_record = &build.artifacts["mod\\fixture\\journal\\site.journal"]["Data"]["RootChunk"]
        ["entry"]["Data"]["entries"][0]["Data"]["entries"][0]["Data"];
    assert_eq!(site_record["mainPagePath"]["Data"]["fileEntryIndex"], 1);
    let localization = &build.artifacts["mod\\fixture\\localization\\en-us\\onscreens\\site.json"]
        ["Data"]["RootChunk"]["root"]["Data"]["entries"];
    let resolve = |reference: &Value| {
        let primary = reference["value"].as_str().unwrap().strip_prefix("LocKey#").unwrap();
        localization.as_array().unwrap().iter()
            .find(|entry| entry["primaryKey"] == primary)
            .expect("website reference must resolve to a generated primary key")["femaleVariant"].clone()
    };
    assert_eq!(resolve(&site_record["shortName"]), "Fixture");
    for page in site_record["entries"].as_array().unwrap() {
        for text in page["Data"]["texts"].as_array().unwrap() {
            let preserve = json!({"$type":"Color","Alpha":0,"Blue":255,"Green":255,"Red":255});
            assert_eq!(text["Data"]["tintColor"], preserve);
            assert_eq!(text["Data"]["hoverTintColor"], preserve);
            let resolved = resolve(&text["Data"]["text"]);
            assert!(resolved == "Open profile" || resolved == "Profile");
        }
    }
    assert!(
        site_record["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|page| { page["Data"]["activatedAtStart"] == 1 })
    );
    let text = serde_json::to_string(
        &build.artifacts["mod\\fixture\\localization\\en-us\\onscreens\\site.json"],
    )
    .unwrap();
    assert!(text.contains("gl_fixture_web_home_Title"));
    assert!(text.contains("Open profile"));
    let mut invalid = site();
    invalid["pages"]["home"]["layout"][1]["font"] = json!("missing");
    assert!(ui::website(&invalid).is_err());
    invalid = site();
    invalid["pages"]["home"]["read_fact"] = json!("invalid fact");
    assert!(ui::website(&invalid).is_err());
}

#[test]
#[ignore = "requires GHOSTLINE_TEST_SCHEMA and optional GHOSTLINE_TEST_GAME"]
fn native_website_atlas_and_font_readback() {
    let path = std::env::var_os("GHOSTLINE_TEST_SCHEMA").expect("explicit schema");
    let path = std::path::Path::new(&path);
    let schema = if let Some(game) = std::env::var_os("GHOSTLINE_TEST_GAME") {
        ghostline_red::schema::RedSchema::from_game_files(std::path::Path::new(&game), path)
            .unwrap()
    } else {
        ghostline_red::schema::RedSchema::from_slice(&std::fs::read(path).unwrap()).unwrap()
    };
    let mut build = ui::website(&site()).unwrap();
    build.artifacts.insert("mod\\fixture\\ui\\icons.inkatlas".into(), ui::atlas(&json!({
        "depot":"mod\\fixture\\ui\\icons.inkatlas",
        "textures":["mod\\fixture\\ui\\icons.xbm","mod\\fixture\\ui\\icons.xbm","mod\\fixture\\ui\\icons.xbm"],
        "parts":[{"name":"logo","uv":[0,0,0.5,1]}]
    })).unwrap());
    build.artifacts.insert(
        "mod\\fixture\\ui\\font.inkfontfamily".into(),
        ui::font_family(&json!({
            "depot":"mod\\fixture\\ui\\font.inkfontfamily","name":"Fixture",
            "styles":[{"name":"Regular","font":"mod\\fixture\\ui\\regular.fnt"}]
        }))
        .unwrap(),
    );
    assert_eq!(native::encode(&build.artifacts, &schema).unwrap().len(), 6);
}

#[test]
fn native_verification_preserves_buffer_aliases_across_numbering() {
    let expected = json!({"Data":{"RootChunk":{"a":{"BufferId":"7","Bytes":"AA=="},"b":{"BufferId":"3","Bytes":"AQ=="},"c":{"BufferRefId":"7"}}}});
    let mut actual = json!({"Data":{"RootChunk":{"a":{"BufferId":"0","Bytes":"AA=="},"b":{"BufferId":"1","Bytes":"AQ=="},"c":{"BufferRefId":"0"}}}});
    native::verify(&expected, &actual).unwrap();
    actual["Data"]["RootChunk"]["c"]["BufferRefId"] = json!("1");
    assert!(native::verify(&expected, &actual).is_err());
}
