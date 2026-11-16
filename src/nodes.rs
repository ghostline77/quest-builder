//! Typed native quest node constructors shared by block compilers.
use crate::graph::{Graph, NodeId};
use anyhow::Result;
use serde_json::{Value, json};

/// Encodes an exact, case-sensitive native name.
pub fn cname(value: &str) -> Value {
    json!({"$type":"CName","$storage":"string","$value":value})
}
/// Encodes a native world reference.
pub fn node_ref(value: &str) -> Value {
    json!({"$type":"NodeRef","$storage":"string","$value":value})
}
/// Encodes an exact TweakDB record identity.
pub fn tweak_id(value: &str) -> Value {
    json!({"$type":"TweakDBID","$storage":"string","$value":value})
}
/// Encodes a synchronous native resource import.
pub fn resource(value: &str) -> Value {
    json!({"DepotPath":{"$type":"ResourcePath","$storage":"string","$value":value},"Flags":"Default"})
}
/// Encodes an actor reference, optionally scoped to named community entries.
pub fn entity(reference: Option<&str>, names: &[&str]) -> Value {
    json!({"$type":"gameEntityReference","dynamicEntityUniqueName":cname("None"),"names":names.iter().map(|n|cname(n)).collect::<Vec<_>>(),"reference":reference.map_or_else(||json!({"$type":"NodeRef","$storage":"uint64","$value":"0"}),node_ref),"sceneActorContextName":cname("None"),"slotName":cname("None"),"type":"EntityRef"})
}
/// Shows or hides an existing terminal document using the native computer event.
pub fn terminal_document_enabled(
    g: &mut Graph,
    id: u32,
    device: &str,
    document: &str,
    enabled: bool,
) -> Result<NodeId> {
    let event = g.wrap(json!({"$type":"EnableDocumentEvent",
        "documentAdress":{"$type":"SDocumentAdress","folderID":-1,"documentID":-1},
        "documentName":cname(document),"documentType":"FILE",
        "enable":u8::from(enabled),"entireFolder":0}));
    g.node(id, "questEventManagerNodeDefinition", &["In"], &["Out"],
        json!({"componentName":cname("None"),"event":event,
            "isObjectPlayer":0,"isUiEvent":0,"managerName":"ComputerManager",
            "objectRef":entity(Some(device),&[]),"PSClassName":cname("None")}))
}

/// Adds a phase entry with a caller-selected native socket name.
pub fn input(g: &mut Graph, name: &str) -> Result<NodeId> {
    g.node(
        0,
        "questInputNodeDefinition",
        &[],
        &["Out"],
        json!({"socketName":cname(name)}),
    )
}
/// Adds a terminating phase exit.
pub fn output(g: &mut Graph, id: u32, name: &str) -> Result<NodeId> {
    g.node(
        id,
        "questOutputNodeDefinition",
        &["In"],
        &[],
        json!({"socketName":cname(name),"type":"Terminating"}),
    )
}
/// Adds a phase invocation with explicit native ports.
pub fn phase(
    g: &mut Graph,
    id: u32,
    path: &str,
    inputs: &[&str],
    outputs: &[&str],
) -> Result<NodeId> {
    g.node(id,"questPhaseNodeDefinition",inputs,outputs,json!({"phaseGraph":null,"phaseInstancePrefabs":[],"phaseResource":resource(path),"saveLock":0,"unfreezingTriggerNodeRef":{"$type":"NodeRef","$storage":"uint64","$value":"0"}}))
}
/// Allocates a journal path handle.
pub fn journal_path(g: &mut Graph, path: &str, class: &str, index: u32) -> Value {
    g.wrap(json!({"$type":"gameJournalPath","className":cname(class),"editorPath":"","fileEntryIndex":index,"realPath":path}))
}
/// Adds activation/deactivation of an arbitrary journal entry.
pub fn journal(g: &mut Graph, id: u32, path: &str, class: &str, index: u32) -> Result<NodeId> {
    let path = journal_path(g, path, class, index);
    let kind =
        g.wrap(json!({"$type":"questJournalEntry_NodeType","path":path,"sendNotification":1}));
    g.node(
        id,
        "questJournalNodeDefinition",
        &["Active", "Inactive"],
        &["Out"],
        json!({"type":kind}),
    )
}
/// Adds an objective or complete-quest state transition.
pub fn objective(g: &mut Graph, id: u32, path: &str, whole_quest: bool) -> Result<NodeId> {
    objective_tracking(g, id, path, whole_quest, true)
}
/// Adds an objective transition with explicit automatic tracking policy.
pub fn objective_tracking(
    g: &mut Graph,
    id: u32,
    path: &str,
    whole_quest: bool,
    track: bool,
) -> Result<NodeId> {
    let path = journal_path(
        g,
        path,
        if whole_quest {
            "gameJournalQuest"
        } else {
            "gameJournalQuestObjective"
        },
        2,
    );
    let kind=g.wrap(json!({"$type":"questJournalQuestEntry_NodeType","optional":0,"path":path,"sendNotification":1,"trackQuest":u8::from(track),"version":"Initial"}));
    g.node(
        id,
        "questJournalNodeDefinition",
        &["Active", "Inactive", "Succeeded", "Failed"],
        &["Out"],
        json!({"type":kind}),
    )
}
/// Adds a map pin activation/deactivation node.
pub fn mappin(g: &mut Graph, id: u32, path: &str, disable_previous: bool) -> Result<NodeId> {
    let path = journal_path(g, path, "gameJournalQuestMapPin", 2);
    g.node(
        id,
        "questMappinManagerNodeDefinition",
        &["Active", "Inactive"],
        &["Out"],
        json!({"disablePreviousMappins":u8::from(disable_previous),"path":path}),
    )
}
/// Adds an exact fact assignment.
pub fn fact(g: &mut Graph, id: u32, name: &str, value: i64) -> Result<NodeId> {
    let kind = g.wrap(
        json!({"$type":"questSetVar_NodeType","factName":name,"setExactValue":1,"value":value}),
    );
    g.node(
        id,
        "questFactsDBManagerNodeDefinition",
        &["In"],
        &["Out"],
        json!({"type":kind}),
    )
}

/// Releases an actor or player from its current workspot, acknowledging completion.
pub fn stop_workspot(
    g: &mut Graph,
    id: u32,
    community: Option<&str>,
    entry: Option<&str>,
) -> Result<NodeId> {
    let player = community.is_none();
    let params = g.wrap(json!({"$type":"questUseWorkspotParamsV1","function":"StopWorkspot","isPlayer":u8::from(player),"instant":0,"teleport":0,"finishAnimation":1,"repeatCommandOnInterrupt":0,"entryId":{"$type":"workWorkEntryId","id":u32::MAX},"exitEntryId":{"$type":"workWorkEntryId","id":u32::MAX},"workspotNode":node_ref("")}));
    g.node(id, "questUseWorkspotNodeDefinition", &["In"], &["Success"], json!({"entityReference":entity(community,&entry.into_iter().collect::<Vec<_>>()),"paramsV1":params}))
}
/// Adds a native fact listener or instantaneous snapshot.
pub fn fact_condition(
    g: &mut Graph,
    id: u32,
    name: &str,
    comparison: &str,
    value: i64,
    snapshot: bool,
) -> Result<NodeId> {
    let kind=g.wrap(json!({"$type":"questVarComparison_ConditionType","comparisonType":comparison,"factName":name,"value":value}));
    let condition = g.wrap(json!({"$type":"questFactsDBCondition","type":kind}));
    if snapshot {
        g.node(
            id,
            "questConditionNodeDefinition",
            &["In"],
            &["True", "False"],
            json!({"condition":condition}),
        )
    } else {
        g.node(
            id,
            "questPauseConditionNodeDefinition",
            &["In"],
            &["Out"],
            json!({"condition":condition}),
        )
    }
}
/// Adds a trigger event/occupancy listener.
pub fn trigger(g: &mut Graph, id: u32, reference: &str, kind: &str) -> Result<NodeId> {
    let condition=g.wrap(json!({"$type":"questTriggerCondition","activatorRef":entity(None,&[]),"isPlayerActivator":1,"triggerAreaRef":node_ref(reference),"type":kind}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}
/// Adds an AND join or a one-shot XOR join.
pub fn join(g: &mut Graph, id: u32, count: usize, all: bool) -> Result<NodeId> {
    let inputs = (1..=count).map(|n| format!("In{n}")).collect::<Vec<_>>();
    g.node(
        id,
        if all {
            "questLogicalAndNodeDefinition"
        } else {
            "questLogicalXorNodeDefinition"
        },
        &inputs.iter().map(String::as_str).collect::<Vec<_>>(),
        &["Out1"],
        json!({"inputSocketCount":count,"outputSocketCount":1}),
    )
}
/// Adds a game-time delay.
pub fn game_delay(
    g: &mut Graph,
    id: u32,
    days: u64,
    hours: u64,
    minutes: u64,
    seconds: u64,
) -> Result<NodeId> {
    let kind=g.wrap(json!({"$type":"questGameTimeDelay_ConditionType","days":days,"hours":hours,"minutes":minutes,"seconds":seconds}));
    let condition = g.wrap(json!({"$type":"questTimeCondition","type":kind}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}
/// Adds a real-time delay; the native field intentionally spells miliseconds.
pub fn real_delay(g: &mut Graph, id: u32, seconds: u64, milliseconds: u64) -> Result<NodeId> {
    let kind=g.wrap(json!({"$type":"questRealtimeDelay_ConditionType","hours":0,"minutes":0,"seconds":seconds,"miliseconds":milliseconds}));
    let condition = g.wrap(json!({"$type":"questTimeCondition","type":kind}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}
/// Adds a native checkpoint, optionally allowing failure retry.
pub fn checkpoint(g: &mut Graph, id: u32, name: &str, retry: bool) -> Result<NodeId> {
    g.node(id,"questCheckpointNodeDefinition",&["In"],&["Out"],json!({"additionalEndGameRewardsTweak":[],"debugString":name,"endGameSave":0,"ignoreSaveLocks":0,"pointOfNoReturn":0,"retryOnFailure":u8::from(retry),"saveLock":0}))
}

/// Adds or removes a precise quantity from the local player's inventory.
pub fn item(g: &mut Graph, id: u32, item: &str, quantity: u64, remove: bool) -> Result<NodeId> {
    item_for_actor(g, id, item, quantity, remove, None)
}

/// Adds or removes inventory from a specific actor, preserving native loot handling.
pub fn item_for_actor(g: &mut Graph, id: u32, item: &str, quantity: u64, remove: bool, actor: Option<(&str, &str)>) -> Result<NodeId> {
    let reference = actor.map_or_else(|| entity(None,&[]), |(community, entry)| entity(Some(community), &[entry]));
    let player=g.wrap(json!({"$type":"questUniversalRef","entityReference":reference,"mainPlayerObject":0,"refLocalPlayer":u8::from(actor.is_none())}));
    let params=g.wrap(json!({"$type":"questAddRemoveItem_NodeTypeParams","entityRef":player,"flagItemAddedCallbackAsSilent":0,"isPlayer":0,"itemID":tweak_id(item),"itemIDsToIgnoreOnRemove":[],"nodeType":if remove{"RemoveItem"}else{"AddItem"},"objectRef":entity(None,&[]),"quantity":quantity,"removeAllQuantity":0,"sendNotification":1,"tagsToIgnoreOnRemove":[],"tagToRemove":cname("None")}));
    let kind = g.wrap(json!({"$type":"questAddRemoveItem_NodeType","params":[params]}));
    g.node(
        id,
        "questItemManagerNodeDefinition",
        &["In"],
        &["Out"],
        json!({"type":kind}),
    )
}
/// Waits for an inventory entitlement, or snapshots it without waiting.
pub fn inventory(
    g: &mut Graph,
    id: u32,
    item: &str,
    quantity: u64,
    snapshot: bool,
) -> Result<NodeId> {
    let kind=g.wrap(json!({"$type":"questInventory_ConditionType","comparisonType":"GreaterOrEqual","isPlayer":1,"itemID":tweak_id(item),"itemTag":cname("None"),"objectRef":entity(None,&[]),"quantity":quantity}));
    let condition = g.wrap(json!({"$type":"questObjectCondition","type":kind}));
    g.node(
        id,
        if snapshot {
            "questConditionNodeDefinition"
        } else {
            "questPauseConditionNodeDefinition"
        },
        &["In"],
        if snapshot {
            &["True", "False"]
        } else {
            &["Out"]
        },
        json!({"condition":condition}),
    )
}
/// Controls a native population spawner at its authored world transform.
pub fn population(g: &mut Graph, id: u32, reference: &str, action: &str) -> Result<NodeId> {
    anyhow::ensure!(
        ["Activate", "Deactivate"].contains(&action),
        "invalid population action"
    );
    let kind = g.wrap(json!({"$type":"questSpawner_NodeType","action":action,"spawnerReference":node_ref(reference)}));
    g.node(
        id,
        "questSpawnManagerNodeDefinition",
        &["In"],
        &["Out"],
        json!({"actions":[{"$type":"questSpawnManagerNodeActionEntry","type":kind}]}),
    )
}

/// Waits for a single NPC bound to a population spawner to appear or disappear.
pub fn population_npc_present(
    g: &mut Graph,
    id: u32,
    reference: &str,
    present: bool,
) -> Result<NodeId> {
    let comparison = g.wrap(json!({"$type":"questComparisonParam","comparisonType":if present{"Greater"}else{"Equal"},"count":0,"entireCommunity":false}));
    let kind = g.wrap(json!({"$type":"questCharacterSpawned_ConditionType","comparisonParams":comparison,"objectRef":entity(Some(reference),&[])}));
    let condition = g.wrap(json!({"$type":"questCharacterCondition","type":kind}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}

/// Activates/deactivates a community, optionally targeting a named entry/phase.
pub fn community(
    g: &mut Graph,
    id: u32,
    reference: &str,
    action: &str,
    entry: Option<&str>,
    phase: Option<&str>,
) -> Result<NodeId> {
    let action=g.wrap(json!({"$type":"questCommunityTemplate_NodeType","action":action,"communityEntryName":cname(entry.unwrap_or("None")),"communityEntryPhaseName":cname(phase.unwrap_or("None")),"spawnerReference":node_ref(reference)}));
    g.node(
        id,
        "questSpawnManagerNodeDefinition",
        &["In"],
        &["Out"],
        json!({"actions":[{"$type":"questSpawnManagerNodeActionEntry","type":action}]}),
    )
}
/// Waits for a completed scan of a specific object.
pub fn scan(g: &mut Graph, id: u32, reference: &str) -> Result<NodeId> {
    let kind=g.wrap(json!({"$type":"questScan_ConditionType","eventType":"Finished","objectRef":entity(Some(reference),&[])}));
    let condition = g.wrap(json!({"$type":"questObjectCondition","type":kind}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}
/// Waits for a visited journal entry or a selected phone choice.
pub fn journal_condition(
    g: &mut Graph,
    id: u32,
    path: &str,
    class: &str,
    index: u32,
    selected: bool,
) -> Result<NodeId> {
    let path = journal_path(g, path, class, index);
    let kind = if selected {
        g.wrap(json!({"$type":"questJournalEntryState_ConditionType","inverted":0,"path":path,"state":"Succeeded"}))
    } else {
        g.wrap(json!({"$type":"questJournalEntryVisited_ConditionType","path":path,"visited":1}))
    };
    let condition = g.wrap(json!({"$type":"questJournalCondition","type":kind}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}
/// Grants a named native reward record.
pub fn reward(g: &mut Graph, id: u32, record: &str) -> Result<NodeId> {
    let kind = g.wrap(json!({"$type":"questGiveReward_NodeType","rewards":[tweak_id(record)]}));
    g.node(
        id,
        "questRewardManagerNodeDefinition",
        &["In"],
        &["Out"],
        json!({"type":kind}),
    )
}

/// Invokes a scene with explicit entries, exits and native world placement.
pub fn scene(
    g: &mut Graph,
    id: u32,
    path: &str,
    origin: &str,
    entries: &[&str],
    exits: &[&str],
) -> Result<NodeId> {
    let mut scene_file = resource(path);
    scene_file["Flags"] = json!("Soft");
    g.node(id,"questSceneNodeDefinition",entries,exits,json!({"interruptionOperations":[],"notAllowedToBeFrozen":0,"reapplyInterruptionOperationsAfterGameLoad":0,"sceneFile":scene_file,"sceneLocation":{"$type":"scnWorldMarker","nodeRef":node_ref(origin),"tag":cname("None"),"type":"NodeRef"},"syncToMusic":0}))
}
/// Waits for a deliberately selected set of character resolution events.
pub fn character_outcome(
    g: &mut Graph,
    id: u32,
    community: &str,
    entry: &str,
    killed: bool,
    unconscious: bool,
    defeated: bool,
) -> Result<NodeId> {
    let comparison=g.wrap(json!({"$type":"questComparisonParam","comparisonType":"GreaterOrEqual","count":1,"entireCommunity":0}));
    let kind=g.wrap(json!({"$type":"questCharacterKilled_ConditionType","comparisonParams":comparison,"defeated":u8::from(defeated),"killed":u8::from(killed),"objectRef":entity(Some(community),&[entry]),"source":null,"unconscious":u8::from(unconscious)}));
    let condition = g.wrap(json!({"$type":"questCharacterCondition","type":kind}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}
/// Snapshots the engine's specific-vehicle availability predicate.
pub fn vehicle_available(g: &mut Graph, id: u32, record: &str) -> Result<NodeId> {
    let kind=g.wrap(json!({"$type":"questVehicleAvailable_ConditionType","vehicleName":record,"vehicleType":"SpecificVehicle"}));
    let condition = g.wrap(json!({"$type":"questVehicleCondition","type":kind}));
    g.node(
        id,
        "questConditionNodeDefinition",
        &["In"],
        &["True", "False"],
        json!({"condition":condition}),
    )
}

/// Enables or disables a native trigger area and its notifiers.
pub fn set_trigger_state(g: &mut Graph, id: u32, reference: &str, enabled: bool) -> Result<NodeId> {
    let params = json!({
        "$type": "questSetTriggerState_NodeTypeParams",
        "objectRef": node_ref(reference),
        "enable": u8::from(enabled)
    });
    let kind = g.wrap(json!({
        "$type": "questSetTriggerState_NodeType",
        "params": [params]
    }));
    g.node(
        id,
        "questTriggerManagerNodeDefinition",
        &["In"],
        &["Out"],
        json!({"type": kind}),
    )
}

/// Sends an explicit native device action.
pub fn device_action(
    g: &mut Graph,
    id: u32,
    device: &str,
    controller: &str,
    action: &str,
) -> Result<NodeId> {
    device_action_properties(g, id, device, controller, action, &json!({}))
}

/// Sends a native device action with typed action properties.
pub fn device_action_properties(
    g: &mut Graph,
    id: u32,
    device: &str,
    controller: &str,
    action: &str,
    properties: &Value,
) -> Result<NodeId> {
    let properties = properties
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("device properties must be an object"))?;
    let properties = properties
        .iter()
        .map(|(name, value)| {
            json!({
                "$type":"questDeviceManager_ActionProperty", "name":cname(name),
                "value":{"$type":"Variant","value_type":value["type"],"value":value["value"]}
            })
        })
        .collect::<Vec<_>>();
    let params = g.wrap(json!({"$type":"questDeviceManager_NodeTypeParams","actionProperties":properties,"deviceAction":cname(action),"deviceControllerClass":cname(controller),"entityRef":entity(None,&[]),"objectRef":node_ref(device),"slotName":cname("None")}));
    let kind = g.wrap(json!({"$type":"questDeviceManager_NodeType","params":[params]}));
    g.node(
        id,
        "questInteractiveObjectManagerNodeDefinition",
        &["In"],
        &["Out"],
        json!({"type":kind}),
    )
}
/// Waits for a native device controller function to report completion.
pub fn device_condition(
    g: &mut Graph,
    id: u32,
    device: &str,
    controller: &str,
    function: &str,
) -> Result<NodeId> {
    let kind=g.wrap(json!({"$type":"questDevice_ConditionType","deviceConditionFunction":cname(function),"deviceControllerClass":cname(controller),"functionParameters":[],"objectRef":node_ref(device)}));
    let condition = g.wrap(json!({"$type":"questObjectCondition","type":kind}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}
/// Waits for either any community member or one named actor to exist.
pub fn spawned(g: &mut Graph, id: u32, community: &str, entry: Option<&str>) -> Result<NodeId> {
    let comparison=g.wrap(json!({"$type":"questComparisonParam","comparisonType":"Greater","count":0,"entireCommunity":u8::from(entry.is_none())}));
    let names = entry.into_iter().collect::<Vec<_>>();
    let kind=g.wrap(json!({"$type":"questCharacterSpawned_ConditionType","comparisonParams":comparison,"objectRef":entity(Some(community),&names)}));
    let condition = g.wrap(json!({"$type":"questCharacterCondition","type":kind}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}
/// Waits for a whole encounter community to be resolved.
pub fn community_defeated(
    g: &mut Graph,
    id: u32,
    community: &str,
    nonlethal: bool,
) -> Result<NodeId> {
    let comparison=g.wrap(json!({"$type":"questComparisonParam","comparisonType":"GreaterOrEqual","count":0,"entireCommunity":1}));
    let kind=g.wrap(json!({"$type":"questCharacterKilled_ConditionType","comparisonParams":comparison,"defeated":u8::from(nonlethal),"killed":1,"objectRef":entity(Some(community),&[]),"source":null,"unconscious":u8::from(nonlethal)}));
    let condition = g.wrap(json!({"$type":"questCharacterCondition","type":kind}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}
/// Injects combat threat with explicit command acknowledgement.
pub fn combat_threat(
    g: &mut Graph,
    id: u32,
    community: &str,
    entry: &str,
    function: Option<&str>,
    duration: f64,
) -> Result<NodeId> {
    let params=g.wrap(json!({"$type":"AIInjectCombatThreatCommandParams","dontForceHostileAttitude":0,"duration":duration,"isPersistent":0,"targetNodeRef":{"$type":"NodeRef","$storage":"uint64","$value":"0"},"targetPuppetRef":entity(Some("#player"),&[])}));
    let mut properties =
        json!({"entityReference":entity(Some(community),&[entry]),"params":params});
    if let Some(function) = function {
        properties["function"] = cname(function);
    }
    g.node(
        id,
        "questCombatNodeDefinition",
        &["In"],
        &["Success"],
        properties,
    )
}
/// Waits for a specific actor/vehicle pair to occupy a trigger area.
pub fn actor_inside(g: &mut Graph, id: u32, trigger: &str, activator: Value) -> Result<NodeId> {
    let condition=g.wrap(json!({"$type":"questTriggerCondition","activatorRef":activator,"isPlayerActivator":0,"triggerAreaRef":node_ref(trigger),"type":"IsInside"}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}
/// Builds exact native mount predicates, including carried actors.
#[expect(
    clippy::too_many_arguments,
    reason = "Explicit parent, child and event bindings mirror the native predicate"
)]
pub fn mount(
    g: &mut Graph,
    id: u32,
    parent: Value,
    child: Value,
    parent_player: bool,
    child_player: bool,
    role: &str,
    event: &str,
    snapshot: bool,
) -> Result<NodeId> {
    let kind=g.wrap(json!({"$type":"questCharacterMount_ConditionType","anyChild":0,"anyParent":0,"childIsPlayer":u8::from(child_player),"childRef":child,"condition":event,"enterAnimationFinished":0,"parentIsPlayer":u8::from(parent_player),"parentRef":parent,"playerVehicleName":"","role":role,"usePlayersVehicle":0,"vehicleAfiliation":"Invalid","vehicleOrigin":"Any","vehicleType":"Any"}));
    let condition = g.wrap(json!({"$type":"questCharacterCondition","type":kind}));
    g.node(
        id,
        if snapshot {
            "questConditionNodeDefinition"
        } else {
            "questPauseConditionNodeDefinition"
        },
        &["In"],
        if snapshot {
            &["True", "False"]
        } else {
            &["Out"]
        },
        json!({"condition":condition}),
    )
}
/// Waits until the player carries the named actor within the target trigger.
pub fn carried_inside(
    g: &mut Graph,
    id: u32,
    community: &str,
    entry: &str,
    destination: &str,
) -> Result<NodeId> {
    let kind=g.wrap(json!({"$type":"questCharacterMount_ConditionType","anyChild":0,"anyParent":0,"childIsPlayer":0,"childRef":entity(Some(community),&[entry]),"condition":"OnMount","enterAnimationFinished":0,"parentIsPlayer":1,"parentRef":entity(None,&[]),"playerVehicleName":"","role":"Invalid","usePlayersVehicle":0,"vehicleAfiliation":"Invalid","vehicleOrigin":"Any","vehicleType":"Any"}));
    let mount = g.wrap(json!({"$type":"questCharacterCondition","type":kind}));
    let area=g.wrap(json!({"$type":"questTriggerCondition","activatorRef":entity(None,&[]),"isPlayerActivator":1,"triggerAreaRef":node_ref(destination),"type":"IsInside"}));
    let condition = g
        .wrap(json!({"$type":"questLogicalCondition","conditions":[mount,area],"operation":"AND"}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}
/// Waits for the named vehicle's speed to reach zero.
pub fn vehicle_stopped(g: &mut Graph, id: u32, vehicle: &str) -> Result<NodeId> {
    let kind=g.wrap(json!({"$type":"questVehicleSpeed_ConditionType","comparisonType":"CT_EQUAL","speed":0,"vehicleRef":entity(Some(vehicle),&[])}));
    let condition = g.wrap(json!({"$type":"questVehicleCondition","type":kind}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}
/// Assigns a named community actor to an explicit vehicle seat.
pub fn assign_vehicle(
    g: &mut Graph,
    id: u32,
    community: &str,
    entry: &str,
    vehicle: &str,
    vehicle_entry: &str,
    slot: &str,
) -> Result<NodeId> {
    let kind=g.wrap(json!({"$type":"questAssignCharacter_NodeType","assign":1,"characterRef":entity(Some(community),&[entry]),"clearAssignedVehicleIdWhenUnmounting":0,"entryAnimName":cname("None"),"entrySlotName":cname("default"),"isInstant":1,"isPlayer":0,"slotName":cname(slot),"vehicleRef":entity(Some(vehicle),&[vehicle_entry])}));
    g.node(
        id,
        "questVehicleNodeDefinition",
        &["In"],
        &["Out"],
        json!({"type":kind}),
    )
}

/// Teleports the local player while resetting look-at, with explicit healing policy.
pub fn teleport_player(g: &mut Graph, id: u32, destination: &str, heal: bool) -> Result<NodeId> {
    let destination=g.wrap(json!({"$type":"questUniversalRef","entityReference":entity(Some(destination),&[]),"mainPlayerObject":0,"refLocalPlayer":0}));
    let params=g.wrap(json!({"$type":"questTeleportPuppetParamsV1","destinationOffset":{"$type":"Vector3","X":0,"Y":0,"Z":0},"destinationRef":destination,"doNavTest":0,"healAtTeleport":u8::from(heal),"useFastTravelMechanism":0}));
    let look=g.wrap(json!({"$type":"questPlayerLookAtParams","adjustPitch":1,"adjustYaw":1,"cameraInputMagToBreak":0.200000003,"duration":0.25,"easeIn":1,"easeOut":1,"endOnCameraInputApplied":1,"endOnTargetReached":1,"endOnTimeExceeded":1,"lookAtTarget":entity(None,&[]),"maxDuration":2,"offset":{"$type":"Vector3","X":0,"Y":0,"Z":0},"precision":0.100000001,"slotName":cname("None"),"useOffsetToPlayer":1}));
    let player=g.wrap(json!({"$type":"questUniversalRef","entityReference":entity(None,&[]),"mainPlayerObject":0,"refLocalPlayer":1}));
    g.node(id,"questTeleportPuppetNodeDefinition",&["In"],&["Out"],json!({"entityReference":player,"lookAtAction":"Reset","params":params,"playerLookAt":look}))
}
/// Shows/hides the player's world representation for braindance playback.
pub fn player_visibility(g: &mut Graph, id: u32, show: bool) -> Result<NodeId> {
    let kind=g.wrap(json!({"$type":"questShowWorldNode_NodeType","componentName":cname("None"),"isPlayer":1,"objectRef":node_ref("#player"),"show":u8::from(show)}));
    g.node(
        id,
        "questWorldDataManagerNodeDefinition",
        &["In"],
        &["Out"],
        json!({"type":kind}),
    )
}
/// Changes a journal objective's displayed counter.
pub fn objective_counter(g: &mut Graph, id: u32, path: &str) -> Result<NodeId> {
    let path = journal_path(g, path, "gameJournalQuestObjective", 2);
    let kind = g.wrap(json!({"$type":"questJournalQuestObjectiveCounter_NodeType","path":path}));
    g.node(
        id,
        "questJournalNodeDefinition",
        &["Increment", "Decrement"],
        &["Out"],
        json!({"type":kind}),
    )
}

/// Reserves an item with the native drop-point controller and its inherited UI data.
pub fn reserve_drop_point(g: &mut Graph, id: u32, item: &str, drop_point: &str) -> Result<NodeId> {
    let zero = json!({"$type":"TweakDBID","$storage":"uint64","$value":"0"});
    let widget = json!({"$type":"SActionWidgetPackage","action":null,"bckgroundTextureID":zero,"customData":null,"dependendActions":[],"displayName":"","iconID":cname("None"),"iconTextureID":zero,"isValid":1,"isWidgetInactive":0,"libraryID":cname("None"),"libraryPath":{"$type":"redResourceReferenceScriptToken","resource":{"DepotPath":{"$type":"ResourcePath","$storage":"uint64","$value":"0"},"Flags":"Default"}},"orientation":"Horizontal","ownerID":{"$type":"gamePersistentID","componentName":cname("None"),"entityHash":"0"},"ownerIDClassName":cname("None"),"placement":"DOCKED","textData":null,"wasInitalized":0,"widget":null,"widgetName":"","widgetState":"DEFAULT","widgetTweakDBID":zero});
    let choice = json!({"$type":"gameinteractionsChoice","caption":"","captionParts":{"$type":"gameinteractionsChoiceCaption","parts":[]},"choiceMetaData":{"$type":"gameinteractionsChoiceMetaData","tweakDBID":zero,"tweakDBName":"","type":{"$type":"gameinteractionsChoiceTypeWrapper","properties":0}},"data":[],"doNotTurnOffPreventionSystem":0,"lookAtDescriptor":{"$type":"gameinteractionsChoiceLookAtDescriptor","offset":{"$type":"Vector3","X":0,"Y":0,"Z":0},"orbId":{"$type":"gameinteractionsOrbID","id":0},"slotName":cname("None"),"type":"Root"}});
    let event=g.wrap(json!({"$type":"ReserveItemToThisDropPoint","actionName":cname("None"),"actionWidgetPackage":widget,"activationTimeReduction":0,"activeStatusEffect":zero,"attachedProgram":zero,"calculatedBaseCost":0,"canSkipPayCost":0,"canTriggerStim":1,"clearanceLevel":0,"costComponents":[],"deviceActionQueue":null,"disableSpread":0,"duration":0,"executor":null,"hasInteraction":0,"inactiveReason":"","inkWidgetID":zero,"interactionChoice":choice,"interactionIconType":zero,"interactionLayer":cname("None"),"isActionQueueingUsed":0,"isActionRPGCheckDissabled":0,"IsAppliedByMonowire":0,"isInactive":0,"isQueuedAction":0,"isQuickHack":0,"isSpiderbotAction":0,"isTargetDead":0,"item":tweak_id(item),"localizedObjectName":"","objectActionID":zero,"objectActionRecord":null,"paymentQuantity":0,"prop":null,"proxyExecutor":null,"requesterID":{"$type":"entEntityID","hash":"0"},"shouldActivateDevice":0,"spiderbotActionLocationOverride":{"$type":"NodeRef","$storage":"uint64","$value":"0"},"wasPerformedOnOwner":0,"widgetStyle":"DarkBlue"}));
    g.node(id,"questEventManagerNodeDefinition",&["In"],&["Out"],json!({"componentName":cname("controller"),"event":event,"isObjectPlayer":0,"isUiEvent":0,"managerName":"DropPointManager","objectRef":entity(Some(drop_point),&[]),"PSClassName":cname("DropPointControllerPS")}))
}

/// Assigns the actor's gameplay AI tier before giving a follower role.
pub fn gameplay_ai(g: &mut Graph, id: u32, community: &str, entry: &str) -> Result<NodeId> {
    g.node(id,"questPuppetAIManagerNodeDefinition",&["In"],&["Out"],json!({"entries":[{"$type":"questPuppetAIManagerNodeDefinitionEntry","aiTier":"Gameplay","entityReference":entity(Some(community),&[entry])}]}))
}
/// Assigns or releases the native follower role, waiting for AI acknowledgement.
pub fn follower(
    g: &mut Graph,
    id: u32,
    community: &str,
    entry: &str,
    assign: bool,
) -> Result<NodeId> {
    let params = if assign {
        let role=g.wrap(json!({"$type":"AIFollowerRole","attitudeGroupName":cname("None"),"followerRef":entity(Some("#player"),&[]),"followTarget":null,"followTargetSquads":[],"friendlyTargetSlotListener":null,"isFriendMelee":0,"isOwnerSniper":0,"lastStealthLeaveTimeStamp":{"$type":"EngineTime"},"owner":null,"ownerTargetSlotListener":null,"playerCombatListener":null}));
        g.wrap(json!({"$type":"AIAssignRoleCommandParams","role":role}))
    } else {
        g.wrap(json!({"$type":"AIClearRoleCommandParams"}))
    };
    g.node(id,"questMiscAICommandNode",&["In"],&["Success"],json!({"entityReference":entity(Some(community),&[entry]),"function":cname(if assign {"AIAssignRoleCommandParams"} else {"AIClearRoleCommandParams"}),"params":params}))
}
