use anyhow::{Context, Result, ensure};
use ghostline_red::{archive, cr2w, material::ArchiveSet};
use std::ops::Range;

fn u16_at(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        bytes.get(at..at + 2).context("uint16 bounds")?.try_into()?,
    ))
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes.get(at..at + 4).context("uint32 bounds")?.try_into()?,
    ))
}
fn put(bytes: &mut [u8], at: usize, value: u32) -> Result<()> {
    bytes
        .get_mut(at..at + 4)
        .context("uint32 write bounds")?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}
fn properties<'a>(
    bytes: &[u8],
    file: &'a cr2w::Cr2wInspection,
    index: usize,
) -> Result<(Vec<(&'a str, Range<usize>)>, usize)> {
    let export = file.exports.get(index).context("export index")?;
    let mut cursor = export.data_offset as usize;
    let end = cursor + export.data_size as usize;
    ensure!(bytes.get(cursor) == Some(&0), "export property marker");
    cursor += 1;
    let mut properties = Vec::new();
    loop {
        let name = u16_at(bytes, cursor)? as usize;
        cursor += 2;
        if name == 0 {
            break;
        }
        let size = u32_at(bytes, cursor + 2)? as usize;
        ensure!(size >= 4, "property size");
        cursor += 6;
        let payload_end = cursor + size - 4;
        ensure!(payload_end <= end, "property bounds");
        properties.push((
            file.names
                .get(name)
                .context("property name")?
                .value
                .as_str(),
            cursor..payload_end,
        ));
        cursor = payload_end;
    }
    Ok((properties, cursor))
}
fn vlq(bytes: &[u8], start: usize) -> Result<(usize, usize)> {
    let first = *bytes.get(start).context("count bounds")?;
    ensure!(first & 0x80 == 0, "negative count");
    let mut count = (first & 0x3f) as usize;
    let mut cursor = start + 1;
    if first & 0x40 != 0 {
        let mut shift = 6;
        loop {
            let byte = *bytes.get(cursor).context("count continuation")?;
            cursor += 1;
            count |= ((byte & 0x7f) as usize) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            ensure!(shift <= 27, "count overflow");
        }
    }
    Ok((count, cursor))
}

pub(super) fn build(archives: &ArchiveSet) -> Result<(String, Vec<u8>)> {
    let depot = r"base\worlds\03_night_city\_compiled\default\exterior_-10_16_0_1.streamingsector";
    let original = archives.read_resource(depot)?;
    let file = cr2w::inspect_bytes(&original)?;
    let target = 7752692643803313782_u64.to_le_bytes();
    let mut matches = Vec::new();
    for (index, export) in file.exports.iter().enumerate() {
        if export.class_name != "worldCompiledCommunityAreaNode_Streamable" {
            continue;
        }
        let (fields, _) = properties(&original, &file, index)?;
        if fields.iter().any(|(name, range)| {
            *name == "sourceObjectId"
                && original[range.clone()]
                    .windows(8)
                    .any(|bytes| bytes == target)
        }) {
            let distance = fields
                .iter()
                .find(|(name, _)| *name == "streamingDistance")
                .context("community streaming distance")?
                .1
                .clone();
            ensure!(distance.len() == 4, "community distance size");
            matches.push((index, distance));
        }
    }
    ensure!(
        matches.len() == 1,
        "expected exactly one native vendor controller"
    );
    let (export_index, distance_range) = matches.remove(0);
    let old_distance = f32::from_bits(u32_at(&original, distance_range.start)?);
    ensure!(
        (old_distance - 200.086655).abs() < 0.01,
        "unexpected native distance {old_distance}"
    );
    let (_, appendix) = properties(&original, &file, 0)?;
    let version = u32_at(&original, appendix)?;
    ensure!(
        version == 68,
        "expected native sector version 68, got {version}"
    );
    let encoded_buffer = u32_at(&original, appendix + 8)?;
    ensure!(encoded_buffer > 0x80000000, "node buffer reference");
    let buffer_index = (encoded_buffer - 0x80000001) as usize;
    let (count, handles_at) = vlq(&original, appendix + 12)?;
    let indices = (0..count)
        .filter_map(|index| {
            (u32_at(&original, handles_at + index * 4).ok()? == export_index as u32 + 1)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    ensure!(indices.len() == 1, "expected one compiled node definition");
    let node_index = indices[0];
    let buffer = file
        .buffers
        .get(buffer_index)
        .context("world node buffer")?;
    let stored = &original[buffer.offset as usize..(buffer.offset + buffer.disk_size) as usize];
    let mut decoded = if stored.starts_with(b"KARK") {
        archive::decompress_payload(&stored[8..], u32_at(stored, 4)? as usize)?
    } else {
        stored.to_vec()
    };
    ensure!(
        decoded.len() == buffer.memory_size as usize && decoded.len().is_multiple_of(144),
        "compiled node buffer size"
    );
    let mut placements = Vec::new();
    for (index, record) in decoded.chunks_exact(144).enumerate() {
        let packed = u64::from_le_bytes(record[112..120].try_into()?);
        if (packed & 0x7ffff) as usize == node_index {
            placements.push(index);
        }
    }
    ensure!(placements.len() == 1, "expected one vendor placement");
    let placement_offset = placements[0] * 144 + 124;
    ensure!(
        (f32::from_bits(u32_at(&decoded, placement_offset)?) - old_distance).abs() < 0.01,
        "placement distance differs from controller"
    );
    put(&mut decoded, placement_offset, 100000.0_f32.to_bits())?;

    // Retain all exports and their opaque cooked data, names, imports and handles.
    // Only the two distance values and the necessary container metadata change.
    let mut output = original[..file.header.objects_end as usize].to_vec();
    put(&mut output, distance_range.start, 100000.0_f32.to_bits())?;
    let export = &file.exports[export_index];
    let export_crc = crc32fast::hash(
        &output[export.data_offset as usize..(export.data_offset + export.data_size) as usize],
    );
    put(
        &mut output,
        file.header.tables[4].offset as usize + export_index * 24 + 20,
        export_crc,
    )?;
    for (index, buffer) in file.buffers.iter().enumerate() {
        let bytes = if index == buffer_index {
            decoded.as_slice()
        } else {
            &original[buffer.offset as usize..(buffer.offset + buffer.disk_size) as usize]
        };
        let offset = u32::try_from(output.len())?;
        output.extend_from_slice(bytes);
        let entry = file.header.tables[5].offset as usize + index * 24;
        put(&mut output, entry + 8, offset)?;
        put(&mut output, entry + 12, u32::try_from(bytes.len())?)?;
        put(&mut output, entry + 20, crc32fast::hash(bytes))?;
    }
    let buffers_end = u32::try_from(output.len())?;
    output.extend_from_slice(&original[file.header.buffers_end as usize..]);
    put(&mut output, 28, buffers_end)?;
    for table_index in [4, 5] {
        let table = &file.header.tables[table_index];
        let crc = crc32fast::hash(
            &output[table.offset as usize..table.offset as usize + table.item_count as usize * 24],
        );
        put(&mut output, 40 + table_index * 12 + 8, crc)?;
    }
    let mut header = output[..160].to_vec();
    put(&mut header, 32, 0xdeadbeef)?;
    put(&mut output, 32, crc32fast::hash(&header))?;
    let verified = cr2w::inspect_bytes(&output)?;
    ensure!(
        verified.exports.len() == file.exports.len()
            && verified.imports.len() == file.imports.len()
            && verified.buffers.len() == file.buffers.len(),
        "native table counts changed"
    );
    for (index, (before, after)) in file.exports.iter().zip(&verified.exports).enumerate() {
        let old_bytes = &original
            [before.data_offset as usize..(before.data_offset + before.data_size) as usize];
        let new_bytes =
            &output[after.data_offset as usize..(after.data_offset + after.data_size) as usize];
        if index != export_index {
            ensure!(old_bytes == new_bytes, "unrelated native export changed");
        }
    }
    Ok((depot.into(), output))
}
