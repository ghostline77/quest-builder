//! Native laptop instances and journal-backed terminal documents.
use super::{Handles, geometry, nodes};
use crate::nodes as n;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub(super) fn device(spec: &Value, reference: &str, h: &mut Handles) -> Result<Value> {
    let entity = spec
        .get("entity")
        .or_else(|| spec.get("entity_template"))
        .and_then(Value::as_str)
        .unwrap_or("base\\gameplay\\devices\\masters\\computers\\laptop_1.ent");
    let ids = spec
        .get("controller_component_ids")
        .cloned()
        .unwrap_or_else(|| json!(["1108512084555509772", "1131680419258347532"]));
    ensure!(
        entity == "base\\gameplay\\devices\\masters\\computers\\laptop_1.ent"
            || spec.get("controller_component_ids").is_some(),
        "custom computers need explicit native controller identities"
    );
    let ids = ids
        .as_array()
        .filter(|v| !v.is_empty())
        .context("computer needs native controller identities")?;
    let documents = nodes::list(spec, "documents")?;
    let mut chunks = Vec::new();
    let mut cruids = json!({});
    for (index, identity) in ids.iter().enumerate() {
        let identity = identity
            .as_str()
            .map(str::to_owned)
            .or_else(|| identity.as_u64().map(|v| v.to_string()))
            .context("computer component identity")?;
        ensure!(
            identity.parse::<u64>().is_ok_and(|id| id > 0),
            "computer controller identity must be nonzero uint64"
        );
        let mut files = Vec::new();
        for document in documents {
            let path = document["journal_entry"]
                .as_str()
                .context("terminal document journal path")?;
            let journal = if index == 0 && ids.len() > 1 {
                Value::Null
            } else {
                h.wrap(json!({"$type":"gameJournalPath","className":n::cname("gameJournalFile"),"editorPath":path,"fileEntryIndex":document.get("file_index").cloned().unwrap_or(json!(5)),"realPath":path}))
            };
            files.push(json!({"$type":"gamedeviceDataElement","content":document["text"],"date":"","documentName":n::cname(document["name"].as_str().context("terminal document name")?),"isEnabled":1,"isEncrypted":0,"journalPath":journal,"owner":"SYSTEM","questInfo":{"$type":"gamedeviceQuestInfo","factName":n::cname(document["read_fact"].as_str().context("terminal document read fact")?),"isHighlighted":u8::from(index!=0)},"title":document["title"],"wasRead":0}));
        }
        let setup = json!({"$type":"ComputerSetup","allowRemoteAuthorization":0,"animationState":"None","animationStateSetFactName":n::cname("None"),"filesMenu":u8::from(!files.is_empty()),"filesStructure":if files.is_empty(){json!([])}else{json!([{"$type":"gamedeviceGenericDataContent","content":files,"name":"FILES"}])},"mailsMenu":0,"mailsStructure":[],"internetMenu":0,"internetSubnet":{"$type":"SInternetData","startingPage":""},"newsFeedMenu":0,"newsFeed":[],"newsFeedInterval":5,"systemMenu":1,"hideTopNavigationBar":0,"ignoreSlaveAuthorizationModule":0,"ignoreSleepMode":0,"maxClearance":10,"minClearance":1,"shouldForceVirtualSystem":1,"startingMenu":"FILES","stylePreset":n::tweak_id("ComputerUIPresets.DefaultPreset")});
        let mut setup = setup;
        if let Some(address) = spec.get("starting_page").and_then(Value::as_str) {
            ensure!(
                address.starts_with("NETdir://"),
                "computer starting_page must be a NETdir address"
            );
            setup["internetMenu"] = json!(1);
            setup["internetSubnet"]["startingPage"] = json!(address);
            setup["startingMenu"] = json!("INTERNET");
        }
        if let Some(menu) = spec.get("starting_menu") {
            let menu = menu
                .as_str()
                .context("computer starting_menu must be a string")?;
            ensure!(
                matches!(menu, "FILES" | "INTERNET"),
                "computer starting_menu must be FILES or INTERNET"
            );
            setup["startingMenu"] = json!(menu);
        }
        let personal_link = nodes::flag(spec, "has_personal_link_slot", false)?;
        let interaction = if personal_link {
            n::tweak_id(
                spec.get("interaction_record")
                    .and_then(Value::as_str)
                    .unwrap_or("Interactions.StealData"),
            )
        } else {
            json!({"$type":"TweakDBID","$storage":"uint64","$value":"0"})
        };
        let state=h.wrap(json!({"$type":"ComputerControllerPS","computerSetup":setup,"deviceState":spec.get("initial_state").cloned().unwrap_or(json!("ON")),"contentScale":{"$type":"TweakDBID","$storage":"uint64","$value":"0"},"hasPersonalLinkSlot":u8::from(personal_link),"personalLinkCustomInteraction":interaction,"markAsQuest":1,"isKeyloggerInstalled":0,"isInteractive":1,"isInSleepMode":0}));
        chunks.push(json!({"$type":"ComputerController","id":identity,"isReplicable":0,"name":n::cname("controller"),"persistentState":state}));
        cruids[index.to_string()] = json!(identity);
    }
    let buffer = spec.get("buffer_id").and_then(Value::as_u64).unwrap_or(1);
    let instance=h.wrap(json!({"$type":"entEntityInstanceData","buffer":{"BufferId":buffer.to_string(),"Flags":4063232,"Type":"WolvenKit.RED4.Archive.Buffer.RedPackage, WolvenKit.RED4, Version=8.17.4.0, Culture=neutral, PublicKeyToken=null","Data":{"Version":4,"Sections":7,"CruidIndex":-1,"CruidDict":cruids,"Chunks":chunks}}}));
    let mut data = nodes::common(spec, reference, "worldDeviceNode");
    data.as_object_mut().context("computer node")?.extend(json!({"entityTemplate":nodes::resource(entity),"appearanceName":n::cname(spec.get("appearance").and_then(Value::as_str).unwrap_or("default")),"deviceClassName":n::cname("ComputerControllerPS"),"deviceConnections":[],"entityLod":0,"alphaHackStreamingDistanceOverride":geometry::scalar(spec,"alpha_hack_streaming_distance_override",0.)?,"instanceData":instance,"ioPriority":"Immediate"}).as_object().context("computer properties")?.clone());
    Ok(h.wrap(data))
}
