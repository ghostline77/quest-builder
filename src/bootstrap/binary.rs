//! Bounded access to native reflected properties used by bootstrap overrides.
use anyhow::{Context, Result, ensure};
use ghostline_red::cr2w;
use std::ops::Range;

pub(super) fn word(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .context("native word bounds")?
            .try_into()?,
    ))
}
pub(super) fn dword(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .context("native dword bounds")?
            .try_into()?,
    ))
}
pub(super) fn field(
    bytes: &[u8],
    file: &cr2w::Cr2wInspection,
    index: usize,
    key: &str,
) -> Result<Range<usize>> {
    let export = file.exports.get(index).context("native export index")?;
    let mut cursor = export.data_offset as usize + 1;
    let end = export.data_offset as usize + export.data_size as usize;
    while cursor + 2 <= end {
        let name = word(bytes, cursor)? as usize;
        if name == 0 {
            break;
        }
        let size = dword(bytes, cursor + 4)? as usize;
        ensure!(
            size >= 4 && cursor + 4 + size <= end,
            "native property bounds"
        );
        if file.names.get(name).context("native property name")?.value == key {
            return Ok(cursor + 8..cursor + 4 + size);
        }
        cursor += 4 + size;
    }
    anyhow::bail!("native export {index} has no {key}")
}
pub(super) fn single_handle(bytes: &[u8], range: Range<usize>) -> Result<usize> {
    ensure!(
        range.len() == 8 && dword(bytes, range.start)? == 1,
        "expected one native handle"
    );
    let handle = dword(bytes, range.start + 4)?;
    ensure!(handle > 0, "null native handle");
    Ok((handle - 1) as usize)
}
pub(super) fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
