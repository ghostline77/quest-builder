//! Lootable world placements and their project-owned loot tables.
use super::{Handles, nodes};
use crate::{compiler::Artifacts, nodes as n};
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};

pub(super) fn node(
    spec: &Value,
    reference: &str,
    namespace: &str,
    handles: &mut Handles,
    artifacts: &mut Artifacts,
    records: &mut Map<String, Value>,
) -> Result<Value> {
    let key = spec
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_else(|| reference.trim_start_matches('#'));
    ensure!(
        crate::manifest::identifier(key),
        "loot container needs a valid id"
    );
    let supplied = spec
        .get("loot_tables")
        .map(|v| v.as_array().context("loot_tables must be an array"))
        .transpose()?;
    let items = nodes::list(spec, "items")?;
    ensure!(
        !items.is_empty() || supplied.is_some_and(|v| !v.is_empty()),
        "loot container requires items or loot_tables"
    );
    let mut tables = Vec::new();
    if let Some(supplied) = supplied {
        for table in supplied {
            let table = table.as_str().context("loot table record ID")?;
            record_id(table)?;
            tables.push(n::tweak_id(table));
        }
    }
    if !items.is_empty() {
        let table = format!("LootTables.{namespace}_{key}");
        let mut entries = Vec::new();
        let mut guaranteed = 0;
        for (index, item) in items.iter().enumerate() {
            let record = item
                .get("id")
                .and_then(Value::as_str)
                .context("loot item id")?;
            record_id(record)?;
            let count = item.get("quantity").map_or(Ok(1), |v| {
                v.as_u64().context("loot quantity must be an integer")
            })?;
            ensure!(
                (1..=i32::MAX as u64).contains(&count),
                "loot quantity is out of range"
            );
            let chance = item.get("chance").map_or(Ok(1.), |v| {
                v.as_f64().context("loot chance must be numeric")
            })?;
            ensure!(
                chance.is_finite() && (0. ..=1.).contains(&chance),
                "loot chance must be 0..1"
            );
            guaranteed += usize::from(chance == 1.);
            let id = format!("{table}_item_{index:03}");
            ensure!(records.insert(id.clone(), json!({"$type":"gamedataLootItem_Record","itemID":record,"dropChance":chance,"dropCountMin":count,"dropCountMax":count})).is_none(), "duplicate loot item record");
            entries.push(id);
        }
        ensure!(records.insert(table.clone(), json!({"$type":"gamedataLootTable_Record","lootGenerationType":"dropChance","lootItems":entries,"minItemsToLoot":guaranteed,"maxItemsToLoot":items.len()})).is_none(), "duplicate loot table {table}");
        tables.push(n::tweak_id(&table));
    }
    let state = handles.wrap(json!({"$type":"LootContainerObjectAnimatedByTransformPS","isLocked":u8::from(nodes::flag(spec,"locked",false)?),"isDisabled":u8::from(nodes::flag(spec,"disabled",false)?)}));
    let root = json!({"$type":"LootContainerObjectAnimatedByTransform","lootTables":tables,"persistentState":state,"markAsQuest":u8::from(nodes::flag(spec,"quest",false)?)});
    let mut placement = spec.clone();
    if let Some(mesh) = spec.get("mesh").and_then(Value::as_str) {
        ensure!(
            spec.get("entity_template").is_none() && spec.get("entity").is_none(),
            "choose a mesh or an entity template for a container"
        );
        crate::project::depot_path(mesh)?;
        ensure!(mesh.ends_with(".mesh"), "container mesh must end in .mesh");
        let path = format!("mod\\{namespace}\\containers\\{key}.ent");
        let mut chunks = vec![root.clone()];
        let mut component = |kind: &str, name: &str, fields: Value| {
            let mut c = json!({"$type":kind,"id":super::geometry::hash(&format!("{namespace}:{key}:{name}")).to_string(),"name":n::cname(name)});
            c.as_object_mut()
                .expect("component")
                .extend(fields.as_object().expect("fields").clone());
            chunks.push(c);
        };
        let mut mesh_resource = nodes::resource(mesh);
        mesh_resource["Flags"] = json!("Obligatory");
        component(
            "entMeshComponent",
            "mesh",
            json!({"mesh":mesh_resource,"meshAppearance":n::cname(spec.get("mesh_appearance").and_then(Value::as_str).unwrap_or("default"))}),
        );
        component(
            "gameInventory",
            "inventory",
            json!({"persistentState":handles.wrap(json!({"$type":"gameInventoryPS"}))}),
        );
        component("entPlaceholderComponent", "root", json!({}));
        component(
            "gameinteractionsComponent",
            "interactions",
            json!({"definitionResource":{"DepotPath":{"$type":"ResourcePath","$storage":"string","$value":"base\\gameplay\\containers\\generic_container_interaction.interaction"},"Flags":"Obligatory"},"parentTransform":handles.wrap(json!({"$type":"entHardTransformBinding","bindName":n::cname("root")}))}),
        );
        component(
            "GameplayRoleComponent",
            "GameplayRole",
            json!({"gameplayRole":"Loot"}),
        );
        component("gameTargetingComponent", "targeting", json!({}));
        component(
            "entSlotComponent",
            "UI_Slots",
            json!({"slots":(["UI_Interaction","Nameplate","roleMappin","questDeviceMappin","uploadBar","NetworkLink"].iter().map(|name|json!({"$type":"entSlot","slotName":n::cname(name)})).collect::<Vec<_>>())}),
        );
        let mut cruids = json!({});
        for (index, chunk) in chunks.iter().enumerate() {
            cruids[index.to_string()] = chunk.get("id").cloned().unwrap_or(json!("0"));
        }
        let doc = json!({"Data":{"Version":195,"BuildVersion":0,"RootChunk":{"$type":"entEntityTemplate","cookingPlatform":"PLATFORM_PC","defaultAppearance":n::cname("default"),"compiledData":{"BufferId":"0","Flags":7471104,"Type":"WolvenKit.RED4.Archive.Buffer.RedPackage, WolvenKit.RED4, Version=8.17.4.0, Culture=neutral, PublicKeyToken=null","Data":{"Version":4,"Sections":7,"CruidIndex":0,"CruidDict":cruids,"Chunks":chunks}}}}});
        ensure!(
            artifacts.insert(path.clone(), doc).is_none(),
            "container resource collision"
        );
        placement
            .as_object_mut()
            .context("container")?
            .remove("mesh");
        placement["entity_template"] = json!(path);
    } else if spec.get("entity_template").is_none() && spec.get("entity").is_none() {
        placement["entity_template"] =
            json!("base\\gameplay\\loot\\containers\\weapon_cases\\weapon_case_small.ent");
    }
    let mut node = nodes::entity(&placement, reference, handles)?;
    if spec.get("mesh").is_none() {
        node["Data"]["instanceData"] = handles.wrap(json!({"$type":"entEntityInstanceData","buffer":{"BufferId":spec["buffer_id"].to_string(),"Flags":4063232,"Type":"WolvenKit.RED4.Archive.Buffer.RedPackage, WolvenKit.RED4, Version=8.17.4.0, Culture=neutral, PublicKeyToken=null","Data":{"Version":4,"Sections":7,"CruidIndex":0,"CruidDict":{"0":"0"},"Chunks":[root]}}}));
    }
    Ok(node)
}

fn record_id(value: &str) -> Result<()> {
    ensure!(
        value.contains('.')
            && value.split('.').all(|part| {
                let mut bytes = part.bytes();
                bytes
                    .next()
                    .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                    && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
            }),
        "invalid loot record ID {value}"
    );
    Ok(())
}
