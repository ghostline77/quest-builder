//! Shared native quest bootstrap and independently owned quest entry slots.
use crate::{compiler::Build, graph::Graph, nodes as n};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const ID: &str = "ghostline_quest_core";
pub const SLOT_COUNT: u16 = 64;
pub const WORKSPOTS_PER_SLOT: u16 = 2048;
const ROOT: &str = r"mod\ghostline_quest_core";

/// A registry allocation is an ownership declaration, not an activation condition.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Allocation {
    pub slot: u16,
    pub owner: String,
}

/// Generated native workspot declarations supplied by a registered quest pack.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub schema_version: u32,
    pub allocation: Allocation,
    pub workspots: Vec<Value>,
    #[serde(default)]
    pub reserved_workspots: Vec<u16>,
}

pub fn slot_resource(slot: u16) -> Result<String> {
    ensure!(
        (1..=SLOT_COUNT).contains(&slot),
        "quest core slot must be 1..={SLOT_COUNT}"
    );
    Ok(format!(r"{ROOT}\slots\slot_{slot:03}.questphase"))
}

/// Stable world identity reserved in the initial workspot-manager batch.
pub fn workspot_reference(slot: u16, index: u16) -> Result<String> {
    slot_resource(slot)?;
    ensure!(
        index < WORKSPOTS_PER_SLOT,
        "workspot index must be below {WORKSPOTS_PER_SLOT}"
    );
    Ok(format!(
        "$/03_night_city/#ghostline_quest_core/workspots/slot_{slot:03}/#spot_{index:04}"
    ))
}

fn reserved_workspots() -> Result<Vec<Value>> {
    let mut spots = Vec::with_capacity(usize::from(SLOT_COUNT) * usize::from(WORKSPOTS_PER_SLOT));
    for slot in 1..=SLOT_COUNT {
        for index in 0..WORKSPOTS_PER_SLOT {
            let reference = workspot_reference(slot, index)?;
            let coordinate = json!({"$type":"FixedPoint","Bits":0});
            spots.push(json!({"$type":"AISpotPersistentData","globalNodeId":{"$type":"worldGlobalNodeID","hash":crate::world::node_ref_hash(&reference).to_string()},"isEnabled":true,"worldPosition":{"$type":"WorldPosition","x":coordinate,"y":coordinate,"z":coordinate},"yaw":0.}));
        }
    }
    Ok(spots)
}

/// Reject conflicting allocations before packaging any quest content.
pub fn validate_allocations(allocations: &[Allocation]) -> Result<()> {
    let mut slots = std::collections::BTreeSet::new();
    let mut owners = std::collections::BTreeSet::new();
    for allocation in allocations {
        slot_resource(allocation.slot)?;
        ensure!(
            crate::manifest::identifier(&allocation.owner),
            "invalid slot owner"
        );
        ensure!(
            slots.insert(allocation.slot),
            "duplicate quest core slot {}",
            allocation.slot
        );
        ensure!(
            owners.insert(&allocation.owner),
            "duplicate quest core owner {}",
            allocation.owner
        );
    }
    Ok(())
}

fn vacant(slot: u16) -> Result<Value> {
    let mut g = Graph::default();
    let start = n::input(&mut g, "In1")?;
    let out = n::output(&mut g, 1, "Vacant")?;
    // Both outputs are part of the stable slot interface, including empty slots.
    n::output(&mut g, 2, "Done")?;
    g.then(start, out)?;
    g.document(&slot_resource(slot)?, &[])
}

/// Attach a quest pack to its allocated slot without generating global overrides.
/// The root's persistent state is owned by that pack; the core owns dispatch only.
pub fn attach(
    build: &mut Build,
    allocation: &Allocation,
    phase: &crate::bootstrap::Phase,
) -> Result<Registration> {
    validate_allocations(std::slice::from_ref(allocation))?;
    ensure!(
        build.bootstrap.is_none(),
        "slot add-ons must not own a global bootstrap"
    );
    let document = build
        .artifacts
        .get(&phase.resource)
        .context("slot root is not generated")?;
    let exits = phase
        .output
        .iter()
        .map(|out| ("completion".to_owned(), out.clone()))
        .collect();
    crate::audit::interface(
        document,
        std::slice::from_ref(&phase.input),
        &exits,
        &Default::default(),
    )?;
    let path = slot_resource(allocation.slot)?;
    let mut g = Graph::default();
    let start = n::input(&mut g, "In1")?;
    let vacant = n::output(&mut g, 1, "Vacant")?;
    let done = n::output(&mut g, 2, "Done")?;
    let finished = format!("{}_core_completed", allocation.owner);
    let disabled = n::fact_condition(
        &mut g,
        9,
        &format!("{}_core_disabled", allocation.owner),
        "Greater",
        0,
        true,
    )?;
    let guard = n::fact_condition(&mut g, 10, &finished, "Equal", 0, true)?;
    let entered = n::fact(&mut g, 11, &format!("{}_core_entered", allocation.owner), 1)?;
    let call = n::phase(
        &mut g,
        12,
        &phase.resource,
        &[&phase.input],
        &phase.output.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    g.then(start, disabled)?;
    g.connect(disabled, "True", vacant, "In")?;
    g.connect(disabled, "False", guard, "In")?;
    g.connect(guard, "False", done, "In")?;
    g.connect(guard, "True", entered, "In")?;
    g.connect(entered, "Out", call, &phase.input)?;
    if let Some(output) = &phase.output {
        let complete = n::fact(&mut g, 13, &finished, 1)?;
        g.connect(call, output, complete, "In")?;
        g.then(complete, done)?;
    }
    ensure!(
        build
            .artifacts
            .insert(path.clone(), g.document(&path, &[])?)
            .is_none(),
        "slot resource collision"
    );
    for bytes in build.resources.values_mut() {
        let Ok(mut registration) = serde_json::from_slice::<Value>(bytes) else {
            continue;
        };
        if let Some(phases) = registration
            .pointer_mut("/quest/phases")
            .and_then(Value::as_array_mut)
        {
            phases.retain(|entry| entry["path"] != phase.resource);
            if phases.is_empty() {
                registration
                    .as_object_mut()
                    .context("registration")?
                    .remove("quest");
            }
            *bytes = serde_json::to_vec_pretty(&registration)?;
        }
    }
    build.integration_contracts.push(json!({"kind":"quest-core-slot","core":ID,"version":1,"slot":allocation.slot,"owner":allocation.owner,"resource":path,"input":"In1","outputs":["Vacant","Done"]}));
    fn collect(value: &mut Value, workspots: &mut Vec<Value>) -> Result<()> {
        if value["$type"] == "worldCommunityRegistryNode" {
            workspots.extend(std::mem::take(
                value["workspotsPersistentData"]
                    .as_array_mut()
                    .context("persistent workspot batch")?,
            ));
        }
        match value {
            Value::Object(fields) => {
                for child in fields.values_mut() {
                    collect(child, workspots)?;
                }
            }
            Value::Array(rows) => {
                for child in rows {
                    collect(child, workspots)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut workspots = Vec::new();
    for (path, document) in &mut build.artifacts {
        if path.starts_with("mod\\") {
            collect(document, &mut workspots)?;
        }
    }
    let mut reserved_workspots = Vec::new();
    if !workspots.is_empty() {
        let mut bank_ids = std::collections::BTreeMap::new();
        for slot in 1..=SLOT_COUNT {
            for index in 0..WORKSPOTS_PER_SLOT {
                bank_ids.insert(
                    crate::world::node_ref_hash(&workspot_reference(slot, index)?),
                    (slot, index),
                );
            }
        }
        let mut legacy = Vec::new();
        for spot in workspots {
            let hash = spot["globalNodeId"]["hash"]
                .as_str()
                .context("workspot ID")?
                .parse::<u64>()?;
            if let Some(&(slot, index)) = bank_ids.get(&hash) {
                ensure!(
                    slot == allocation.slot,
                    "quest pack {} uses another slot's workspot bank ({slot})",
                    allocation.owner
                );
                ensure!(
                    !reserved_workspots.contains(&index),
                    "duplicate reserved workspot {index}"
                );
                reserved_workspots.push(index);
            }
            legacy.push(spot);
        }
        workspots = legacy;
    }
    Ok(Registration {
        schema_version: 1,
        allocation: allocation.clone(),
        workspots,
        reserved_workspots,
    })
}

/// Generate the standalone core, its health message, and vacant slot resources.
pub fn documents(registrations: &[Registration]) -> Result<Build> {
    validate_allocations(
        &registrations
            .iter()
            .map(|r| r.allocation.clone())
            .collect::<Vec<_>>(),
    )?;
    let mut workspots = reserved_workspots()?;
    let mut ids = std::collections::BTreeSet::new();
    let mut bank_positions = std::collections::BTreeMap::new();
    for (index, spot) in workspots.iter().enumerate() {
        let hash = spot["globalNodeId"]["hash"].as_str().context("bank ID")?.parse::<u64>()?;
        ids.insert(hash);
        bank_positions.insert(hash, index);
    }
    for registration in registrations {
        ensure!(
            registration.schema_version == 1,
            "unsupported workspot registration version"
        );
        let mut indices = std::collections::BTreeSet::new();
        for &index in &registration.reserved_workspots {
            workspot_reference(registration.allocation.slot, index)?;
            ensure!(
                indices.insert(index),
                "duplicate reserved workspot index {index}"
            );
        }
        for spot in &registration.workspots {
            let id = spot["globalNodeId"]["hash"]
                .as_str()
                .context("registered workspot ID")?
                .parse::<u64>()?;
            if let Some(&position) = bank_positions.get(&id) {
                ensure!(
                    registration.reserved_workspots.iter().any(|&index|
                        workspot_reference(registration.allocation.slot, index)
                            .is_ok_and(|reference| crate::world::node_ref_hash(&reference) == id)),
                    "workspot {id} is outside the pack's declared reserved allocation"
                );
                workspots[position] = spot.clone();
                continue;
            }
            ensure!(
                id != 0 && ids.insert(id),
                "workspot registration collision {id}"
            );
            workspots.push(spot.clone());
        }
    }
    let mut build = Build::default();
    let phase = format!(r"{ROOT}\controller.questphase");
    let mut g = Graph::default();
    let start = n::input(&mut g, "In1")?;
    let ready = n::fact(&mut g, 10, "ghostline_quest_core_active", 1)?;
    g.then(start, ready)?;
    let health = format!(r"{ROOT}\health.questphase");
    let call = n::phase(&mut g, 11, &health, &["In1"], &["Done"])?;
    g.connect(ready, "Out", call, "In1")?;
    for slot in 1..=SLOT_COUNT {
        let path = slot_resource(slot)?;
        build.artifacts.insert(path.clone(), vacant(slot)?);
        let id = 100 + u32::from(slot) * 10;
        let check = n::fact_condition(
            &mut g,
            id,
            &format!("ghostline_quest_core_slot_{slot:03}_done"),
            "Equal",
            0,
            true,
        )?;
        let call = n::phase(&mut g, id + 1, &path, &["In1"], &["Vacant", "Done"])?;
        let retry = n::real_delay(&mut g, id + 2, 15, 0)?;
        let done = n::fact(
            &mut g,
            id + 3,
            &format!("ghostline_quest_core_slot_{slot:03}_done"),
            1,
        )?;
        g.connect(ready, "Out", check, "In")?;
        g.connect(check, "True", call, "In1")?;
        g.connect(call, "Vacant", retry, "In")?;
        g.then(retry, check)?;
        g.connect(call, "Done", done, "In")?;
    }
    build
        .artifacts
        .insert(phase.clone(), g.document(&phase, &[])?);
    let (journal, texts, health_document) = health_documents(&health)?;
    let journal_path = journal.0.clone();
    let texts_path = texts.0.clone();
    build.artifacts.insert(journal.0, journal.1);
    build.artifacts.insert(texts.0, texts.1);
    build.artifacts.insert(health, health_document);
    build.resources.insert(format!("{ID}.archive.xl"), serde_json::to_vec_pretty(&json!({"journal":[journal_path],"localization":{"onscreens":{"en-us":[texts_path]}},"quest":{"phases":[{"parent":"base\\quest\\cyberpunk2077.quest","path":phase}]}}))?);
    crate::bootstrap::attach(
        &mut build,
        crate::bootstrap::Spec::new(
            ID.to_owned(),
            vec![crate::bootstrap::Phase {
                resource: phase,
                input: "In1".into(),
                output: None,
            }],
        )
        .with_workspots(workspots),
    )?;
    Ok(build)
}

type Resource = (String, Value);
fn health_documents(path: &str) -> Result<(Resource, Resource, Value)> {
    let mut next = 1_u64;
    let mut entry = |class: &str, id: &str, entries: Vec<Value>, fields: Value| {
        let mut data = json!({"$type":class,"id":id,"journalEntryOverrideDataList":[]});
        if class != "gameJournalPhoneMessage" {
            data["entries"] = json!(entries);
        }
        data.as_object_mut()
            .expect("object literal")
            .extend(fields.as_object().expect("object literal").clone());
        let id = next.to_string();
        next += 1;
        json!({"HandleId":id,"Data":data})
    };
    let message = entry(
        "gameJournalPhoneMessage",
        "active",
        vec![],
        json!({"attachment":null,"delay":1,"imageId":{"$type":"TweakDBID","$storage":"uint64","$value":"0"},"isQuestImportant":1,"sender":"NPC","text":crate::localization::reference("ghostline_quest_core_active_message")}),
    );
    let thread = entry(
        "gameJournalPhoneConversation",
        "status",
        vec![message],
        json!({"title":crate::localization::reference("ghostline_quest_core_name")}),
    );
    let contact = entry(
        "gameJournalContact",
        ID,
        vec![thread],
        json!({"avatarID":n::tweak_id("PhoneAvatars.Avatar_Unknown"),"isCallableDefault":0,"name":crate::localization::reference("ghostline_quest_core_name"),"type":"Texter","useFlatMessageLayout":1}),
    );
    let contacts = entry(
        "gameJournalPrimaryFolderEntry",
        "contacts",
        vec![contact],
        json!({}),
    );
    let root = entry(
        "gameJournalRootFolderEntry",
        "",
        vec![contacts],
        json!({"descriptor":n::resource("base\\journal\\descriptor.journaldesc")}),
    );
    let journal_path = format!(r"{ROOT}\core.journal");
    let texts_path = format!(r"{ROOT}\localization\en-us\core.json");
    let document = |root: Value| json!({"Data":{"Version":195,"BuildVersion":0,"RootChunk":root}});
    let journal = document(
        json!({"$type":"gameJournalResource","cookingPlatform":"PLATFORM_PC","entry":root}),
    );
    let texts = document(
        json!({"$type":"JsonResource","cookingPlatform":"PLATFORM_PC","root":{"HandleId":"0","Data":{"$type":"localizationPersistenceOnScreenEntries","entries":[crate::localization::onscreen_entry("ghostline_quest_core_name","Ghostline Quest Core"),crate::localization::onscreen_entry("ghostline_quest_core_active_message","Ghostline Quest Core active. Installed quest packs can now start.")]}}}),
    );
    let mut g = Graph::default();
    let start = n::input(&mut g, "In1")?;
    let guard = n::fact_condition(
        &mut g,
        10,
        "ghostline_quest_core_notified",
        "Equal",
        0,
        true,
    )?;
    let done = n::output(&mut g, 1, "Done")?;
    g.then(start, guard)?;
    g.connect(guard, "False", done, "In")?;
    let mut previous = guard;
    let mut port = "True";
    for (index, (path, class)) in [
        (format!("contacts/{ID}"), "gameJournalContact"),
        (
            format!("contacts/{ID}/status"),
            "gameJournalPhoneConversation",
        ),
        (
            format!("contacts/{ID}/status/active"),
            "gameJournalPhoneMessage",
        ),
    ]
    .iter()
    .enumerate()
    {
        let message = n::journal(&mut g, 20 + index as u32, path, class, 1)?;
        g.connect(previous, port, message, "Active")?;
        previous = message;
        port = "Out";
    }
    let notified = n::fact(&mut g, 30, "ghostline_quest_core_notified", 1)?;
    g.connect(previous, port, notified, "In")?;
    g.then(notified, done)?;
    Ok((
        (journal_path, journal),
        (texts_path, texts),
        g.document(path, &[])?,
    ))
}
