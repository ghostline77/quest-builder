//! Procedural stage builders, one module for each authoring block.

mod shared;
use crate::manifest::Stage;
use anyhow::Result;
use serde_json::Value;
use shared::{Block, Signal, fractional_delay, threshold};

mod access_routes;
mod acquire_item;
mod acquire_or_use;
mod adaptive_timer;
mod braindance_analysis;
mod carry_npc;
mod choice_gate;
mod combat_encounter;
mod community;
mod contact_fallback;
mod cyberpsycho_encounter;
mod defend_target;
mod deliver_drop_point;
mod deliver_vehicle;
mod drive_to;
mod enter_vehicle;
mod entitlement_reconcile;
mod escort_npc;
mod hack_access_point;
mod interact_device;
mod investigate_clues;
mod investigate_reactions;
mod leave_area;
mod meet_contact;
mod observe_lethal;
mod optional_condition;
mod owned_scope;
mod phone_call;
mod phone_conversation;
mod phone_job_offer;
mod plant_item;
mod play_scene;
mod reach_area;
mod read_shard;
mod read_terminal_document;
mod release_or_rescue_npc;
mod resolve_and_report;
mod reward_commit;
mod ride_with_contact;
mod steal_vehicle;
mod stealth_monitor;
mod time_gate;
mod vehicle_cleanup;

pub(super) fn supported(kind: &str) -> bool {
    if matches!(kind, "community" | "phone_call" | "play_scene") {
        return true;
    }
    matches!(
        kind,
        "access_routes"
            | "acquire_item"
            | "acquire_or_use"
            | "adaptive_timer"
            | "braindance_analysis"
            | "carry_npc"
            | "choice_gate"
            | "combat_encounter"
            | "contact_fallback"
            | "cyberpsycho_encounter"
            | "defend_target"
            | "deliver_drop_point"
            | "deliver_vehicle"
            | "drive_to"
            | "enter_vehicle"
            | "entitlement_reconcile"
            | "escort_npc"
            | "hack_access_point"
            | "interact_device"
            | "investigate_clues"
            | "investigate_reactions"
            | "leave_area"
            | "meet_contact"
            | "observe_lethal"
            | "optional_condition"
            | "owned_scope"
            | "phone_conversation"
            | "phone_job_offer"
            | "plant_item"
            | "reach_area"
            | "read_shard"
            | "read_terminal_document"
            | "release_or_rescue_npc"
            | "resolve_and_report"
            | "reward_commit"
            | "ride_with_contact"
            | "steal_vehicle"
            | "stealth_monitor"
            | "time_gate"
            | "vehicle_cleanup"
    )
}

pub(super) fn build(stage: &Stage) -> Result<Value> {
    match stage.kind.as_str() {
        "play_scene" => play_scene::build(stage),
        "access_routes" => access_routes::build(stage),
        "acquire_item" => acquire_item::build(stage),
        "acquire_or_use" => acquire_or_use::build(stage),
        "adaptive_timer" => adaptive_timer::build(stage),
        "braindance_analysis" => braindance_analysis::build(stage),
        "carry_npc" => carry_npc::build(stage),
        "choice_gate" => choice_gate::build(stage),
        "combat_encounter" => combat_encounter::build(stage),
        "community" => community::build(stage),
        "contact_fallback" => contact_fallback::build(stage),
        "cyberpsycho_encounter" => cyberpsycho_encounter::build(stage),
        "defend_target" => defend_target::build(stage),
        "deliver_drop_point" => deliver_drop_point::build(stage),
        "deliver_vehicle" => deliver_vehicle::build(stage),
        "drive_to" => drive_to::build(stage),
        "enter_vehicle" => enter_vehicle::build(stage),
        "entitlement_reconcile" => entitlement_reconcile::build(stage),
        "escort_npc" => escort_npc::build(stage),
        "hack_access_point" => hack_access_point::build(stage),
        "interact_device" => interact_device::build(stage),
        "investigate_clues" => investigate_clues::build(stage),
        "investigate_reactions" => investigate_reactions::build(stage),
        "leave_area" => leave_area::build(stage),
        "meet_contact" => meet_contact::build(stage),
        "observe_lethal" => observe_lethal::build(stage),
        "optional_condition" => optional_condition::build(stage),
        "owned_scope" => owned_scope::build(stage),
        "phone_conversation" => phone_conversation::build(stage),
        "phone_call" => phone_call::build(stage),
        "phone_job_offer" => phone_job_offer::build(stage),
        "plant_item" => plant_item::build(stage),
        "reach_area" => reach_area::build(stage),
        "read_shard" => read_shard::build(stage),
        "read_terminal_document" => read_terminal_document::build(stage),
        "release_or_rescue_npc" => release_or_rescue_npc::build(stage),
        "resolve_and_report" => resolve_and_report::build(stage),
        "reward_commit" => reward_commit::build(stage),
        "ride_with_contact" => ride_with_contact::build(stage),
        "steal_vehicle" => steal_vehicle::build(stage),
        "stealth_monitor" => stealth_monitor::build(stage),
        "time_gate" => time_gate::build(stage),
        "vehicle_cleanup" => vehicle_cleanup::build(stage),
        _ => Err(super::unsupported(stage)),
    }
}

/// Supporting phases are a compiler detail of their owning block.
pub(super) fn supporting(stage: &Stage) -> Result<Vec<(String, Value)>> {
    let mut resources = Vec::new();
    if stage.fields.contains_key("transfer") {
        resources.push((
            shared::personal_link::attempt_path(stage),
            shared::personal_link::attempt(stage)?,
        ));
    }
    if stage.fields.contains_key("activity") {
        for resume in [false, true] {
            resources.push((
                shared::meeting_activity::attempt_path(stage, resume),
                shared::meeting_activity::attempt(stage, resume)?,
            ));
        }
    }
    Ok(resources)
}
