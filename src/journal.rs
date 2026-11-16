//! Direct native journal, phone, readable and localization generation.
use crate::{composition::Composition, nodes};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn s(value: &Value) -> Result<&str> {
    value.as_str().context("journal string binding")
}
fn values(value: &Value) -> Result<impl Iterator<Item = &Value>> {
    Ok(value
        .as_object()
        .context("journal declaration must be an object")?
        .values())
}
fn loc(value: &Value) -> Value {
    value
        .as_str()
        .map(crate::localization::reference)
        .unwrap_or_else(|| json!({"unk1":"0","value":value}))
}
fn zero_id() -> Value {
    json!({"$type":"TweakDBID","$storage":"uint64","$value":"0"})
}
fn zero_ref() -> Value {
    json!({"$type":"NodeRef","$storage":"uint64","$value":"0"})
}
fn vector(x: f64, y: f64, z: f64) -> Value {
    json!({"$type":"Vector3","X":x,"Y":y,"Z":z})
}

#[derive(Default)]
struct Journal {
    next: u64,
}
impl Journal {
    fn wrap(&mut self, data: Value) -> Value {
        let id = self.next;
        self.next += 1;
        json!({"HandleId":id.to_string(),"Data":data})
    }
    fn entry(&mut self, kind: &str, id: &Value, fields: Value) -> Value {
        let mut data = json!({"$type":kind,"id":id,"journalEntryOverrideDataList":[]});
        data.as_object_mut()
            .expect("native entry object")
            .extend(fields.as_object().expect("native entry fields").clone());
        self.wrap(data)
    }
    fn folder(&mut self, kind: &str, id: &str, entries: Vec<Value>) -> Value {
        self.entry(kind, &json!(id), json!({"entries":entries}))
    }
    fn pin(&mut self, model: &Value) -> Result<Value> {
        let offset = model
            .get("offset")
            .cloned()
            .unwrap_or_else(|| vector(0., 0., 0.5));
        let names = model.get("entry").and_then(Value::as_str).into_iter().collect::<Vec<_>>();
        let fields = json!({"enableGPS":u8::from(!model.get("gps_disabled").and_then(Value::as_bool).unwrap_or(false)),"mappinData":{"$type":"gamemappinsMappinData","active":0,"debugCaption":model.get("debug_caption").unwrap_or(&model["localization_key"]),"localizedCaption":loc(&model["localization_key"]),"mappinType":nodes::tweak_id(if names.is_empty() { "Mappins.QuestStaticMappinDefinition" } else { "Mappins.QuestDynamicMappinDefinition" }),"scriptData":null,"variant":model.get("variant").cloned().unwrap_or(json!("DefaultQuestVariant")),"visibleThroughWalls":1},"offset":offset,"reference":nodes::entity(Some(s(&model["ref"])?),&names),"slotName":nodes::cname(model.get("slot_name").and_then(Value::as_str).unwrap_or("UI_Interaction")),"uiAnimation":zero_id()});
        Ok(self.entry("gameJournalQuestMapPin", &model["id"], fields))
    }
    fn objective(&mut self, model: &Value) -> Result<Value> {
        let mut entries = Vec::new();
        for pin in values(&model["mappins"])? {
            entries.push(self.pin(pin)?);
        }
        entries.push(self.entry(
            "gameJournalQuestDescription",
            &model["description_id"],
            json!({"description":loc(&model["description_key"])}),
        ));
        Ok(self.entry("gameJournalQuestObjective",&model["id"],json!({"counter":model.get("counter").cloned().unwrap_or(json!(0)),"description":loc(&model["localization_key"]),"districtID":"","itemID":zero_id(),"locationPrefabRef":zero_ref(),"optional":u8::from(model.get("optional").and_then(Value::as_bool).unwrap_or(false)),"entries":entries})))
    }
    fn message(&mut self, model: &Value) -> Result<Value> {
        let attachment = if let Some(path) = model.get("attachment") {
            self.wrap(json!({"$type":"gameJournalPath","className":nodes::cname(model.get("attachment_class").and_then(Value::as_str).unwrap_or("gameJournalOnscreen")),"editorPath":"","fileEntryIndex":model.get("attachment_file_index").cloned().unwrap_or(json!(5)),"realPath":path}))
        } else {
            Value::Null
        };
        Ok(self.entry("gameJournalPhoneMessage",&model["id"],json!({"attachment":attachment,"delay":model.get("delay").cloned().unwrap_or(json!(1)),"imageId":zero_id(),"isQuestImportant":u8::from(model.get("important").and_then(Value::as_bool).unwrap_or(true)),"sender":model.get("sender").cloned().unwrap_or(json!("NPC")),"text":loc(&model["localization_key"])})))
    }
    fn choices(&mut self, id: &Value, models: &Value) -> Result<Value> {
        let mut entries = Vec::new();
        for model in values(models)? {
            entries.push(self.entry("gameJournalPhoneChoiceEntry",&model["id"],json!({"isQuestImportant":1,"questCondition":null,"text":loc(&model["localization_key"])})));
        }
        Ok(self.entry(
            "gameJournalPhoneChoiceGroup",
            id,
            json!({"entries":entries}),
        ))
    }
    fn thread(&mut self, model: &Value) -> Result<Value> {
        let mut entries = Vec::new();
        if let Some(explicit) = model.get("entries") {
            for entry in values(explicit)? {
                if entry["type"] == "message" {
                    entries.push(self.message(entry)?);
                } else {
                    entries.push(self.choices(&entry["id"], &entry["entries"])?);
                }
            }
        } else {
            let prefix = s(&model["text_prefix"])?;
            for (name, _) in model["message_text"]
                .as_object()
                .context("phone messages")?
            {
                entries.push(
                    self.message(
                        &json!({"id":name,"localization_key":format!("{prefix}_{name}")}),
                    )?,
                );
            }
            let mut choices = serde_json::Map::new();
            for (name, _) in model["choice_text"].as_object().context("phone choices")? {
                choices.insert(
                    name.clone(),
                    json!({"id":name,"localization_key":format!("{prefix}_choice_{name}")}),
                );
            }
            if !choices.is_empty() {
                entries.push(self.choices(&json!("choices"), &Value::Object(choices))?);
            }
            for (name, value) in model["choice_text"].as_object().context("phone choices")? {
                if value.get("reply").is_some() {
                    entries.push(self.message(&json!({"id":format!("{name}_reply"),"localization_key":format!("{prefix}_{name}_reply")}))?);
                }
            }
            if model.get("final_message").is_some() {
                entries.push(self.message(
                    &json!({"id":"final","localization_key":format!("{prefix}_final")}),
                )?);
            }
        }
        Ok(self.entry(
            "gameJournalPhoneConversation",
            &model["id"],
            json!({"title":loc(&model["title_key"]),"entries":entries}),
        ))
    }
    fn contacts(&mut self, models: &Value) -> Result<Option<Value>> {
        let mut entries = Vec::new();
        for model in values(models)? {
            if model["threads"].as_object().is_none_or(|v| v.is_empty()) {
                continue;
            }
            let mut threads = Vec::new();
            for thread in values(&model["threads"])? {
                threads.push(self.thread(thread)?);
            }
            entries.push(self.entry("gameJournalContact",&model["id"],json!({"avatarID":nodes::tweak_id(model.get("avatar").and_then(Value::as_str).unwrap_or("PhoneAvatars.Avatar_Unknown")),"isCallableDefault":u8::from(model.get("callable").and_then(Value::as_bool).unwrap_or(false)),"name":loc(&model["localization_key"]),"type":model.get("contact_type").cloned().unwrap_or(json!("Texter")),"useFlatMessageLayout":u8::from(model.get("flat_message_layout").and_then(Value::as_bool).unwrap_or(true)),"entries":threads})));
        }
        Ok((!entries.is_empty())
            .then(|| self.folder("gameJournalPrimaryFolderEntry", "contacts", entries)))
    }
    fn readables(&mut self, models: &Value, namespace: &str) -> Result<Value> {
        let mut groups = serde_json::Map::new();
        for model in values(models)? {
            let file = model["kind"] == "file";
            let fields = if file {
                json!({"content":loc(&model["text_key"]),"PictureFilename(legacy)":"","pictureTweak":zero_id(),"title":loc(&model["title_key"])})
            } else {
                json!({"description":loc(&model["text_key"]),"iconID":zero_id(),"tag":nodes::cname(model.get("tag").and_then(Value::as_str).unwrap_or("None")),"title":loc(&model["title_key"])})
            };
            let entry = self.entry(
                if file {
                    "gameJournalFile"
                } else {
                    "gameJournalOnscreen"
                },
                &model["id"],
                fields,
            );
            let group = s(&model["group"])?;
            if !groups.contains_key(group) {
                groups.insert(
                    group.to_owned(),
                    self.folder(
                        if file {
                            "gameJournalFileGroup"
                        } else {
                            "gameJournalOnscreenGroup"
                        },
                        group,
                        Vec::new(),
                    ),
                );
            }
            groups[group]["Data"]["entries"]
                .as_array_mut()
                .context("readable group entries")?
                .push(entry);
        }
        let mut branch = self.folder(
            "gameJournalFolderEntry",
            namespace,
            groups.into_values().collect(),
        );
        for name in ["minor_quest", "quests", "emails"] {
            branch = self.folder("gameJournalFolderEntry", name, vec![branch]);
        }
        Ok(self.folder("gameJournalPrimaryFolderEntry", "onscreens", vec![branch]))
    }
    fn points(&mut self, models: &Value, quest: &Value) -> Result<Value> {
        let mut entries = Vec::new();
        for model in values(models)? {
            let variant=self.wrap(json!({"$type":"gamemappinsCommonVariant","variant":model.get("variant").cloned().unwrap_or(json!("DefaultQuestVariant"))}));
            let path=self.wrap(json!({"$type":"gameJournalPath","className":nodes::cname("gameJournalQuest"),"editorPath":"","fileEntryIndex":2,"realPath":quest["path"]}));
            entries.push(self.entry("gameJournalPointOfInterestMappin",&model["id"],json!({"dynamicEntityRef":nodes::entity(None,&[]),"mappinData":{"$type":"gamemappinsPointOfInterestMappinData","active":1,"dynamicMappinDef":nodes::tweak_id("Mappins.DynamicPointOfInterestMappinDefinition"),"dynamicMappinRadius":30,"slotName":nodes::cname("UI_Interaction"),"slotOffset":vector(0.,0.,0.5),"staticMappinDef":nodes::tweak_id("Mappins.StaticPointOfInterestMappinDefinition"),"typedVariant":variant},"notificationTriggerAreaRef":zero_ref(),"offset":vector(0.,0.,0.),"questPath":path,"recommendedLevelID":zero_id(),"securityAreaRef":zero_ref(),"staticNodeRef":nodes::node_ref(s(&model["ref"])?)})));
        }
        let group = self.folder("gameJournalPointOfInterestGroup", "minor_quests", entries);
        Ok(self.folder(
            "gameJournalPrimaryFolderEntry",
            "points_of_interest",
            vec![group],
        ))
    }
    fn build(mut self, bindings: &Value) -> Result<Value> {
        let mut phases = serde_json::Map::new();
        for model in values(&bindings["objectives"])? {
            let id = s(&model["phase_id"])?;
            if !phases.contains_key(id) {
                let phase = self.entry(
                    "gameJournalQuestPhase",
                    &model["phase_id"],
                    json!({"locationPrefabRef":zero_ref(),"entries":[]}),
                );
                phases.insert(id.to_owned(), phase);
            }
            phases[id]["Data"]["entries"]
                .as_array_mut()
                .context("phase entries")?
                .push(self.objective(model)?);
        }
        let quest = &bindings["quest"];
        let quest_entry=self.entry("gameJournalQuest",&quest["id"],json!({"districtID":"","recommendedLevelID":zero_id(),"title":loc(&quest["title_key"]),"type":quest.get("type").cloned().unwrap_or(json!("MinorQuest")),"entries":phases.into_values().collect::<Vec<_>>()}));
        let folder = self.folder("gameJournalFolderEntry", "minor_quest", vec![quest_entry]);
        let mut branches =
            vec![self.folder("gameJournalPrimaryFolderEntry", "quests", vec![folder])];
        if !bindings["points_of_interest"]
            .as_object()
            .context("points of interest")?
            .is_empty()
        {
            branches.push(self.points(&bindings["points_of_interest"], quest)?);
        }
        if let Some(contacts) = self.contacts(&bindings["contacts"])? {
            branches.push(contacts);
        }
        if !bindings["readables"]
            .as_object()
            .context("readables")?
            .is_empty()
        {
            branches.push(self.readables(&bindings["readables"], s(&quest["id"])?)?);
        }
        let root=self.entry("gameJournalRootFolderEntry",&json!(""),json!({"descriptor":{"DepotPath":{"$type":"ResourcePath","$storage":"string","$value":"base\\journal\\descriptor.journaldesc"},"Flags":"Soft"},"entries":branches}));
        Ok(document(
            s(&bindings["resources"]["journal"])?,
            json!({"$type":"gameJournalResource","cookingPlatform":"PLATFORM_PC","entry":root}),
        ))
    }
}

fn document(depot: &str, root: Value) -> Value {
    json!({"Header":{"WolvenKitVersion":"8.17.4","WKitJsonVersion":"0.0.9","GameVersion":2310,"ExportedDateTime":"1970-01-01T00:00:00Z","DataType":"CR2W","ArchiveFileName":depot},"Data":{"Version":195,"BuildVersion":0,"RootChunk":root,"EmbeddedFiles":[]}})
}

/// Generates all journal and localization resources directly from declarations.
pub fn compose(composition: &Composition) -> Result<BTreeMap<String, Value>> {
    if composition.configuration.is_none() {
        return Ok(BTreeMap::new());
    }
    let journal = Journal::default().build(&composition.bindings)?;
    let journal_depot = s(&composition.bindings["resources"]["journal"])?.to_owned();
    let onscreen_depot = s(&composition.bindings["resources"]["onscreens"])?.to_owned();
    let entries = composition
        .text
        .iter()
        .map(|(key, text)| crate::localization::onscreen_entry(key, text))
        .collect::<Vec<_>>();
    let onscreens = document(
        &onscreen_depot,
        json!({"$type":"JsonResource","cookingPlatform":"PLATFORM_PC","root":{"HandleId":"0","Data":{"$type":"localizationPersistenceOnScreenEntries","entries":entries}}}),
    );
    Ok(BTreeMap::from([
        (journal_depot, journal),
        (onscreen_depot, onscreens),
    ]))
}
