//! Merge generated spots into the world's initial registration, not a later append.
use super::binary::{dword, field, put, word};
use anyhow::{Context, Result, ensure};
use ghostline_red::cr2w::{self, Cr2wInspection};
use serde_json::Value;
use std::{collections::BTreeSet, ops::Range};

fn properties<'a>(
    bytes: &[u8],
    file: &'a Cr2wInspection,
    start: usize,
    end: usize,
) -> Result<(Vec<(&'a str, Range<usize>)>, usize)> {
    ensure!(
        bytes.get(start) == Some(&0),
        "persistent spot struct marker"
    );
    let mut cursor = start + 1;
    let mut result = Vec::new();
    loop {
        ensure!(cursor + 2 <= end, "persistent spot struct bounds");
        let name = word(bytes, cursor)? as usize;
        if name == 0 {
            return Ok((result, cursor + 2));
        }
        let size = dword(bytes, cursor + 4)? as usize;
        ensure!(
            size >= 4 && cursor + 4 + size <= end,
            "persistent spot property bounds"
        );
        result.push((
            file.names
                .get(name)
                .context("persistent spot property name")?
                .value
                .as_str(),
            cursor + 8..cursor + 4 + size,
        ));
        cursor += 4 + size;
    }
}

fn id(bytes: &[u8], file: &Cr2wInspection, fields: &[(&str, Range<usize>)]) -> Result<u64> {
    let range = &fields
        .iter()
        .find(|(name, _)| *name == "globalNodeId")
        .context("spot globalNodeId")?
        .1;
    let (inner, consumed) = properties(bytes, file, range.start, range.end)?;
    ensure!(consumed == range.end, "spot ID trailing data");
    let range = &inner
        .iter()
        .find(|(name, _)| *name == "hash")
        .context("spot ID hash")?
        .1;
    ensure!(range.len() == 8, "spot ID hash width");
    Ok(u64::from_le_bytes(bytes[range.clone()].try_into()?))
}

fn property(file: &Cr2wInspection, name: &str, kind: &str, payload: &[u8]) -> Result<Vec<u8>> {
    let index = |name: &str| -> Result<u16> {
        Ok(u16::try_from(
            file.names
                .iter()
                .position(|entry| entry.value == name)
                .with_context(|| format!("native workspot name {name}"))?,
        )?)
    };
    let mut result = index(name)?.to_le_bytes().to_vec();
    result.extend_from_slice(&index(kind)?.to_le_bytes());
    result.extend_from_slice(&u32::try_from(payload.len() + 4)?.to_le_bytes());
    result.extend_from_slice(payload);
    Ok(result)
}
fn structure(fields: impl IntoIterator<Item = Vec<u8>>) -> Vec<u8> {
    let mut result = vec![0];
    for field in fields {
        result.extend(field);
    }
    result.extend_from_slice(&[0, 0]);
    result
}
fn encode_spot(file: &Cr2wInspection, spot: &Value) -> Result<(u64, Vec<u8>)> {
    let hash: u64 = spot["globalNodeId"]["hash"]
        .as_str()
        .context("generated spot ID")?
        .parse()?;
    ensure!(hash != 0, "zero generated spot ID");
    let mut coordinates = Vec::new();
    for axis in ["x", "y", "z"] {
        let bits = i32::try_from(
            spot["worldPosition"][axis]["Bits"]
                .as_i64()
                .context("fixed point coordinate")?,
        )?;
        let value = structure([property(file, "Bits", "Int32", &bits.to_le_bytes())?]);
        coordinates.push(property(file, axis, "FixedPoint", &value)?);
    }
    let position = structure(coordinates);
    let identifier = structure([property(file, "hash", "Uint64", &hash.to_le_bytes())?]);
    let yaw = spot["yaw"].as_f64().context("spot yaw")? as f32;
    ensure!(yaw.is_finite(), "invalid spot yaw");
    let enabled = spot["isEnabled"]
        .as_bool()
        .map(u8::from)
        .or_else(|| {
            spot["isEnabled"]
                .as_u64()
                .and_then(|v| u8::try_from(v).ok())
        })
        .context("spot enable flag")?;
    ensure!(enabled <= 1, "invalid spot enable flag");
    Ok((
        hash,
        structure([
            property(file, "globalNodeId", "worldGlobalNodeID", &identifier)?,
            property(file, "isEnabled", "Bool", &[enabled])?,
            property(file, "worldPosition", "WorldPosition", &position)?,
            property(file, "yaw", "Float", &yaw.to_le_bytes())?,
        ]),
    ))
}

pub(super) fn merge(original: &[u8], generated: &[Value]) -> Result<Vec<u8>> {
    if generated.is_empty() {
        return Ok(original.to_vec());
    }
    let file = cr2w::inspect_bytes(original)?;
    let mut candidates = Vec::new();
    for (index, export) in file.exports.iter().enumerate() {
        if export.class_name != "worldCommunityRegistryNode" {
            continue;
        }
        let range = field(original, &file, index, "workspotsPersistentData")?;
        if dword(original, range.start)? != 0 {
            candidates.push((index, range));
        }
    }
    ensure!(
        candidates.len() == 1,
        "expected one initial native workspot batch"
    );
    let (registry, range) = candidates.remove(0);
    let original_count = dword(original, range.start)? as usize;
    let mut records = Vec::new();
    let mut identifiers = BTreeSet::new();
    let mut cursor = range.start + 4;
    for _ in 0..original_count {
        let (fields, next) = properties(original, &file, cursor, range.end)?;
        let hash = id(original, &file, &fields)?;
        // Vanilla contains repeated IDs; retain every original record and its
        // relative order instead of treating the baseline as a unique-key map.
        identifiers.insert(hash);
        records.push((hash, original[cursor..next].to_vec()));
        cursor = next;
    }
    ensure!(
        cursor == range.end,
        "initial native workspot batch trailing bytes"
    );
    for spot in generated {
        let (hash, bytes) = encode_spot(&file, spot)?;
        ensure!(identifiers.insert(hash), "generated workspot ID collision {hash}");
        records.push((hash, bytes));
    }
    let mut batch = u32::try_from(records.len())?.to_le_bytes().to_vec();
    records.sort_by_key(|(hash, _)| *hash);
    for (_, bytes) in &records {
        batch.extend_from_slice(bytes);
    }
    ensure!(
        records.len() == original_count + generated.len() && batch.len() > range.len(),
        "initial workspot merge count"
    );
    let growth = batch.len() - range.len();
    let mut output = original[..range.start].to_vec();
    output.extend_from_slice(&batch);
    output.extend_from_slice(&original[range.end..]);
    put(
        &mut output,
        range.start - 4,
        u32::try_from(batch.len() + 4)?,
    );
    for (index, table) in file.header.tables.iter().enumerate() {
        if table.offset as usize >= range.end {
            put(
                &mut output,
                40 + index * 12,
                table.offset + u32::try_from(growth)?,
            );
        }
    }
    let table_at = |index: usize| {
        file.header.tables[index].offset as usize
            + if file.header.tables[index].offset as usize >= range.end {
                growth
            } else {
                0
            }
    };
    for (index, export) in file.exports.iter().enumerate() {
        let table = table_at(4) + index * 24;
        if export.data_offset as usize >= range.end {
            put(
                &mut output,
                table + 12,
                export.data_offset + u32::try_from(growth)?,
            );
        }
        if index == registry {
            let size = export.data_size + u32::try_from(growth)?;
            put(&mut output, table + 8, size);
            let crc = crc32fast::hash(
                &output[export.data_offset as usize..export.data_offset as usize + size as usize],
            );
            put(&mut output, table + 20, crc);
        }
    }
    for (index, buffer) in file.buffers.iter().enumerate() {
        if buffer.offset as usize >= range.end {
            put(
                &mut output,
                table_at(5) + index * 24 + 8,
                buffer.offset + u32::try_from(growth)?,
            );
        }
    }
    put(
        &mut output,
        24,
        file.header.objects_end + u32::try_from(growth)?,
    );
    put(
        &mut output,
        28,
        file.header.buffers_end + u32::try_from(growth)?,
    );
    for index in [4, 5] {
        let at = table_at(index);
        let crc =
            crc32fast::hash(&output[at..at + file.header.tables[index].item_count as usize * 24]);
        put(&mut output, 40 + index * 12 + 8, crc);
    }
    let mut header = output[..160].to_vec();
    put(&mut header, 32, 0xdeadbeef);
    put(&mut output, 32, crc32fast::hash(&header));
    let verified = cr2w::inspect_bytes(&output)?;
    ensure!(
        verified.exports.len() == file.exports.len()
            && verified.buffers.len() == file.buffers.len(),
        "merged workspot resource table counts"
    );
    for (index, (before, after)) in file.exports.iter().zip(&verified.exports).enumerate() {
        if index == registry {
            continue;
        }
        ensure!(
            original[before.data_offset as usize..(before.data_offset + before.data_size) as usize]
                == output
                    [after.data_offset as usize..(after.data_offset + after.data_size) as usize],
            "unrelated workspot merge export changed {index}"
        );
    }
    for (before, after) in file.buffers.iter().zip(&verified.buffers) {
        ensure!(
            original[before.offset as usize..(before.offset + before.disk_size) as usize]
                == output[after.offset as usize..(after.offset + after.disk_size) as usize],
            "workspot merge changed opaque native buffer"
        );
    }
    let merged = field(&output, &verified, registry, "workspotsPersistentData")?;
    ensure!(
        output[merged] == batch,
        "merged native workspot batch readback"
    );
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghostline_red::{codec, schema::RedSchema};
    use serde_json::json;

    #[test]
    #[ignore = "requires GHOSTLINE_NATIVE_REGISTRY pointing to a vanilla registry sector"]
    fn native_world_pool_preserves_existing_records() {
        let path = std::env::var_os("GHOSTLINE_NATIVE_REGISTRY").expect("native registry fixture");
        let original = std::fs::read(path).unwrap();
        let spot = json!({"globalNodeId":{"hash":"18369614221190020847"},"isEnabled":1,"worldPosition":{"x":{"Bits":0},"y":{"Bits":0},"z":{"Bits":0}},"yaw":0});
        let output = merge(&original, &[spot]).unwrap();
        assert!(output.len() > original.len());
    }

    #[test]
    fn initial_batch_merges_in_id_order_and_roundtrips_new_coordinates() {
        let types = json!({
            "worldCommunityRegistryNode":{"properties":{"workspotsPersistentData":{"cs_type":"CArray<AISpotPersistentData>","ordinal":0}}},
            "AISpotPersistentData":{"properties":{"globalNodeId":{"cs_type":"worldGlobalNodeID","ordinal":0},"isEnabled":{"cs_type":"CBool","ordinal":1},"worldPosition":{"cs_type":"WorldPosition","ordinal":2},"yaw":{"cs_type":"CFloat","ordinal":3}}},
            "worldGlobalNodeID":{"properties":{"hash":{"cs_type":"CUInt64","ordinal":0}}},
            "WorldPosition":{"properties":{"x":{"cs_type":"FixedPoint","ordinal":0},"y":{"cs_type":"FixedPoint","ordinal":1},"z":{"cs_type":"FixedPoint","ordinal":2}}},
            "FixedPoint":{"properties":{"Bits":{"cs_type":"CInt32","ordinal":0}}}
        });
        let schema = RedSchema::from_slice(&serde_json::to_vec(&types).unwrap()).unwrap();
        let spot = |id: u64, x: i32| json!({"$type":"AISpotPersistentData","globalNodeId":{"$type":"worldGlobalNodeID","hash":id.to_string()},"isEnabled":1,"worldPosition":{"$type":"WorldPosition","x":{"$type":"FixedPoint","Bits":x},"y":{"$type":"FixedPoint","Bits":-42},"z":{"$type":"FixedPoint","Bits":1024}},"yaw":90});
        let document = json!({"Header":{"ArchiveFileName":"mod\\fixture\\registry.streamingsector"},"Data":{"Version":195,"BuildVersion":0,"RootChunk":{"$type":"worldCommunityRegistryNode","workspotsPersistentData":[spot(20,200)]}}});
        let path = "mod\\fixture\\registry.streamingsector";
        let binary =
            crate::native::encode(&[(path.into(), document)].into_iter().collect(), &schema)
                .unwrap()
                .remove(path)
                .unwrap();
        let merged = merge(&binary, &[spot(30, 300), spot(10, 100)]).unwrap();
        let decoded = codec::decode_wkit_from_bytes(&merged, &schema).unwrap();
        let rows = decoded["Data"]["RootChunk"]["workspotsPersistentData"]
            .as_array()
            .unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row["globalNodeId"]["hash"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["10", "20", "30"]
        );
        assert_eq!(rows[0]["worldPosition"]["x"]["Bits"], 100);
        assert_eq!(rows[2]["worldPosition"]["y"]["Bits"], -42);
        assert_eq!(rows[2]["yaw"].as_f64(), Some(90.0));
        assert!(merge(&binary, &[spot(20, 123)]).is_err());
    }
}
