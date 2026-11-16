use super::binary::{dword, field, put, single_handle, word};
use anyhow::{Context, Result, ensure};
use ghostline_red::{cr2w, material::ArchiveSet};
use std::ops::Range;

fn fields(
    bytes: &[u8],
    file: &cr2w::Cr2wInspection,
    start: usize,
    end: usize,
) -> Result<(Vec<(String, Range<usize>)>, usize)> {
    ensure!(bytes.get(start) == Some(&0), "inline class marker");
    let mut cursor = start + 1;
    let mut result = Vec::new();
    loop {
        let name = word(bytes, cursor)? as usize;
        if name == 0 {
            return Ok((result, cursor + 2));
        }
        let size = dword(bytes, cursor + 4)? as usize;
        ensure!(
            size >= 4 && cursor + 4 + size <= end,
            "inline class property bounds"
        );
        result.push((
            file.names.get(name).context("inline name")?.value.clone(),
            cursor + 8..cursor + 4 + size,
        ));
        cursor += 4 + size;
    }
}

pub(super) fn build(archives: &ArchiveSet) -> Result<(String, Vec<u8>)> {
    let depot = r"base\worlds\03_night_city\_compiled\default\always_loaded_1.streamingsector";
    let original = archives.read_resource(depot)?;
    let file = cr2w::inspect_bytes(&original)?;
    let target = 7752692643803313782_u64.to_le_bytes();
    let mut matches = Vec::new();
    for (index, export) in file.exports.iter().enumerate() {
        if export.class_name != "worldCommunityRegistryNode" {
            continue;
        }
        let range = field(&original, &file, index, "communitiesData")?;
        let count = dword(&original, range.start)?;
        let mut cursor = range.start + 4;
        for _ in 0..count {
            let (properties, end) = fields(&original, &file, cursor, range.end)?;
            let id = &properties
                .iter()
                .find(|(key, _)| key == "communityId")
                .context("registry ID")?
                .1;
            if original[id.clone()].windows(8).any(|bytes| bytes == target) {
                let template = properties
                    .iter()
                    .find(|(key, _)| key == "template")
                    .context("registry template")?
                    .1
                    .clone();
                ensure!(template.len() == 4, "registry template handle");
                matches.push((index, template));
            }
            cursor = end;
        }
        ensure!(cursor == range.end, "registry array trailing data");
    }
    ensure!(
        matches.len() == 1,
        "expected exactly one vendor registry item"
    );
    let (registry, template_range) = matches.remove(0);
    let template = dword(&original, template_range.start)? as usize - 1;
    let entry = single_handle(&original, field(&original, &file, template, "entries")?)?;
    let phase = single_handle(&original, field(&original, &file, entry, "phases")?)?;
    ensure!(
        file.exports[template].class_name == "communityCommunityTemplateData"
            && file.exports[entry].class_name == "communitySpawnEntry"
            && file.exports[phase].class_name == "communitySpawnPhase",
        "vendor template classes"
    );
    ensure!(
        field(&original, &file, phase, "alwaysSpawned").is_err(),
        "expected native default alwaysSpawned"
    );
    let copy_export = |index: usize| {
        let export = &file.exports[index];
        original[export.data_offset as usize..(export.data_offset + export.data_size) as usize]
            .to_vec()
    };
    let count = file.exports.len();
    let mut template_bytes = copy_export(template);
    let entries = field(&original, &file, template, "entries")?;
    put(
        &mut template_bytes,
        entries.start - file.exports[template].data_offset as usize + 4,
        u32::try_from(count + 2)?,
    );
    let mut entry_bytes = copy_export(entry);
    let phases = field(&original, &file, entry, "phases")?;
    put(
        &mut entry_bytes,
        phases.start - file.exports[entry].data_offset as usize + 4,
        u32::try_from(count + 3)?,
    );
    let mut phase_bytes = copy_export(phase);
    let (_, end) = fields(&phase_bytes, &file, 0, phase_bytes.len())?;
    ensure!(end == phase_bytes.len(), "spawn phase appendix");
    phase_bytes.truncate(end - 2);
    let name_index = |name: &str| -> Result<u16> {
        u16::try_from(
            file.names
                .iter()
                .position(|entry| entry.value == name)
                .with_context(|| format!("missing native name {name}"))?,
        )
        .context("name index overflow")
    };
    phase_bytes.extend_from_slice(&name_index("alwaysSpawned")?.to_le_bytes());
    phase_bytes.extend_from_slice(&name_index("gameAlwaysSpawnedState")?.to_le_bytes());
    phase_bytes.extend_from_slice(&6_u32.to_le_bytes());
    phase_bytes.extend_from_slice(&name_index("true")?.to_le_bytes());
    phase_bytes.extend_from_slice(&0_u16.to_le_bytes());
    let clones = [
        (template, template_bytes),
        (entry, entry_bytes),
        (phase, phase_bytes),
    ];
    let insertion = file.header.tables[4].offset as usize + count * 24;
    let table_growth = clones.len() * 24;
    let data_growth: usize = clones.iter().map(|(_, bytes)| bytes.len()).sum();
    ensure!(
        file.exports
            .iter()
            .all(|export| export.data_offset as usize >= insertion),
        "unexpected export/table ordering"
    );
    let mut output = original[..insertion].to_vec();
    output.resize(output.len() + table_growth, 0);
    output.extend_from_slice(&original[insertion..file.header.objects_end as usize]);
    let new_data_start = output.len();
    for (_, bytes) in &clones {
        output.extend_from_slice(bytes);
    }
    output.extend_from_slice(&original[file.header.objects_end as usize..]);
    for (index, table) in file.header.tables.iter().enumerate() {
        if table.offset as usize >= insertion && table.offset != 0 {
            put(
                &mut output,
                40 + index * 12,
                table.offset + u32::try_from(table_growth)?,
            );
        }
    }
    put(&mut output, 40 + 4 * 12 + 4, u32::try_from(count + 3)?);
    put(
        &mut output,
        24,
        file.header.objects_end + u32::try_from(table_growth + data_growth)?,
    );
    put(
        &mut output,
        28,
        file.header.buffers_end + u32::try_from(table_growth + data_growth)?,
    );
    put(&mut output, 36, file.header.chunk_count + 3);
    let exports_at = file.header.tables[4].offset as usize;
    for (index, export) in file.exports.iter().enumerate() {
        put(
            &mut output,
            exports_at + index * 24 + 12,
            export.data_offset + u32::try_from(table_growth)?,
        );
    }
    put(
        &mut output,
        template_range.start + table_growth,
        u32::try_from(count + 1)?,
    );
    let registry_export = &file.exports[registry];
    let registry_start = registry_export.data_offset as usize + table_growth;
    let registry_crc = crc32fast::hash(
        &output[registry_start..registry_start + registry_export.data_size as usize],
    );
    put(&mut output, exports_at + registry * 24 + 20, registry_crc);
    let mut clone_at = new_data_start;
    for (index, (source, bytes)) in clones.iter().enumerate() {
        let old_entry = exports_at + source * 24;
        let entry = exports_at + (count + index) * 24;
        output[entry..entry + 24].copy_from_slice(&original[old_entry..old_entry + 24]);
        put(&mut output, entry + 8, u32::try_from(bytes.len())?);
        put(&mut output, entry + 12, u32::try_from(clone_at)?);
        put(&mut output, entry + 20, crc32fast::hash(bytes));
        clone_at += bytes.len();
    }
    let buffer_table = &file.header.tables[5];
    let buffers_at = buffer_table.offset as usize
        + if buffer_table.offset as usize >= insertion {
            table_growth
        } else {
            0
        };
    for (index, buffer) in file.buffers.iter().enumerate() {
        put(
            &mut output,
            buffers_at + index * 24 + 8,
            buffer.offset + u32::try_from(table_growth + data_growth)?,
        );
    }
    for (index, at, length) in [
        (4, exports_at, (count + 3) * 24),
        (5, buffers_at, file.buffers.len() * 24),
    ] {
        let crc = crc32fast::hash(&output[at..at + length]);
        put(&mut output, 40 + index * 12 + 8, crc);
    }
    let mut header = output[..160].to_vec();
    put(&mut header, 32, 0xdeadbeef);
    put(&mut output, 32, crc32fast::hash(&header));
    let verified = cr2w::inspect_bytes(&output)?;
    ensure!(
        verified.exports.len() == count + 3 && verified.buffers.len() == file.buffers.len(),
        "isolated template table counts"
    );
    for (index, (before, after)) in file.exports.iter().zip(&verified.exports).enumerate() {
        if index == registry {
            continue;
        }
        ensure!(
            original[before.data_offset as usize..(before.data_offset + before.data_size) as usize]
                == output
                    [after.data_offset as usize..(after.data_offset + after.data_size) as usize],
            "unrelated export changed: {index}"
        );
    }
    for (before, after) in file.buffers.iter().zip(&verified.buffers) {
        ensure!(
            original[before.offset as usize..(before.offset + before.disk_size) as usize]
                == output[after.offset as usize..(after.offset + after.disk_size) as usize],
            "native buffer changed"
        );
    }
    Ok((depot.into(), output))
}
