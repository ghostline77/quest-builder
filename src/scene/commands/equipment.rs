use super::*;
pub(super) fn build(v: &Value, h: &mut Handles, player: bool, equip: bool) -> Result<Command> {
    let entity = if player {
        n::entity(None, &[])
    } else {
        actor_entity(v)?
    };
    let reference=h.wrap(json!({"$type":"questObservableUniversalRef","entityReference":entity,"mainPlayerObject":0,"refLocalPlayer":u8::from(player)}));
    let record = |key| -> Result<Value> {
        match v.get(key) {
            None => Ok(tweak("0")),
            Some(Value::String(s)) => Ok(tweak(s)),
            Some(value) => Ok(
                json!({"$type":"TweakDBID","$storage":"uint64","$value":value.as_u64().context("item/slot ID must be a record name or uint64")?.to_string()}),
            ),
        }
    };
    let params=h.wrap(json!({"$type":"questEquipItemParams","byItem":flag(v,"by_item",false)?,"equipDurationOverride":number(v,"equip_duration_override",-1.)?,"equipLastWeapon":0,"equipTypes":"LastWeaponEquipped","failIfItemNotFound":0,"forceFirstEquip":0,"ignoreStateMachine":0,"instant":flag(v,"instant",false)?,"isPlayer":u8::from(player),"itemId":record("item_id")?,"slotId":record("slot_id")?,"type":if equip{"Equip"}else{v.get("operation").and_then(Value::as_str).unwrap_or("Unequip")},"unequipDurationOverride":number(v,"unequip_duration_override",-1.)?,"unequipTypes":v.get("unequip_types").and_then(Value::as_str).unwrap_or("AllWeapons")}));
    Ok(Command::new(
        "questEquipItemNodeDefinition",
        json!({"entityReference":reference,"params":params}),
    ))
}
