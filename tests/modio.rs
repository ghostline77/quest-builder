use ghostline_quest_builder::package::{self, Package};
use serde_json::json;
use std::{collections::BTreeMap, fs};

fn package() -> Package {
    Package {
        name: "street_series".into(),
        payloads: BTreeMap::from([(
            "mod\\races\\series.questphase".into(),
            b"frozen resource".to_vec(),
        )]),
        loose: BTreeMap::from([(
            "r6/tweaks/map.yaml".into(),
            b"Mappins.Example:\n  $base: Mappins.StaticPointOfInterestMappinDefinition\n".to_vec(),
        )]),
        registration: json!({"streaming":{"blocks":[]},"journal":[]}),
        audit: json!({}),
    }
}

#[test]
fn publishes_console_layout_and_converts_loose_tweaks_without_pc_install_paths() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("modio");
    let receipt = package::publish_modio(&package(), &output).unwrap();
    assert_eq!(receipt["archive_verified"], true);
    assert_eq!(receipt["zip_verified"], true);
    let zip =
        zip::ZipArchive::new(fs::File::open(output.join("street_series.zip")).unwrap()).unwrap();
    let paths = zip.file_names().collect::<Vec<_>>();
    assert!(paths.contains(&"street_series/street_series.archive"));
    assert!(paths.contains(&"street_series/street_series.archive.json"));
    assert!(paths.contains(&"tweaks/map.json"));
    assert!(
        !paths
            .iter()
            .any(|p| p.starts_with("archive/pc/") || p.starts_with("r6/") || p.ends_with(".yaml"))
    );
    assert!(!paths.contains(&"package.json"));
    let tweaks: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("tweaks/map.json")).unwrap()).unwrap();
    assert_eq!(
        tweaks["Mappins.Example"]["$base"],
        "Mappins.StaticPointOfInterestMappinDefinition"
    );
}

#[test]
fn rejects_pc_resources_source_scripts_and_resource_patches_before_publication() {
    let mut value = package();
    value.payloads.insert("mod\\custom.ent".into(), vec![]);
    assert!(
        package::validate_modio(&value)
            .unwrap_err()
            .to_string()
            .contains("custom.ent")
    );
    value.payloads.remove("mod\\custom.ent");
    value.loose.insert("r6/scripts/unsafe.reds".into(), vec![]);
    assert!(package::validate_modio(&value).is_err());
    value.loose.remove("r6/scripts/unsafe.reds");
    value.registration["resource"] = json!({"patch":[]});
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("rejected");
    assert!(package::publish_modio(&value, &output).is_err());
    assert!(!output.exists());
}
