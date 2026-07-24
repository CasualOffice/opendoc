//! ZIP central-directory inspection and low-level byte readers.

use std::collections::BTreeSet;

use crate::error::PackageError;
use crate::limits::{PackageLimits, enforce_limit, usize_to_u64};
use crate::package::CancellationToken;
use crate::path::normalize_package_path;

const LOCAL_FILE_SIGNATURE: &[u8; 4] = b"PK\x03\x04";
const CENTRAL_FILE_SIGNATURE: &[u8; 4] = b"PK\x01\x02";
const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
const ZIP64_EOCD_SIGNATURE: &[u8; 4] = b"PK\x06\x06";
const ZIP64_LOCATOR_SIGNATURE: &[u8; 4] = b"PK\x06\x07";

#[derive(Debug)]
pub(crate) struct CentralDirectory {
    pub(crate) start: usize,
    pub(crate) entries: usize,
}

impl CentralDirectory {
    pub(crate) fn inspect(
        bytes: &[u8],
        limits: PackageLimits,
        cancellation: &CancellationToken,
    ) -> Result<Self, PackageError> {
        if !bytes.starts_with(LOCAL_FILE_SIGNATURE) {
            return Err(PackageError::MalformedArchive);
        }
        let eocd = find_eocd(bytes)?;
        let disk = read_u16(bytes, eocd + 4)?;
        let central_disk = read_u16(bytes, eocd + 6)?;
        if disk != 0 || central_disk != 0 {
            return Err(PackageError::MalformedArchive);
        }

        let entries_on_disk = read_u16(bytes, eocd + 8)?;
        let entries = read_u16(bytes, eocd + 10)?;
        let central_size = read_u32(bytes, eocd + 12)?;
        let central_offset = read_u32(bytes, eocd + 16)?;
        let uses_zip64 = entries_on_disk == u16::MAX
            || entries == u16::MAX
            || central_size == u32::MAX
            || central_offset == u32::MAX;
        let (entry_count, central_size, central_offset, central_end) = if uses_zip64 {
            read_zip64_directory(bytes, eocd)?
        } else {
            if entries_on_disk != entries {
                return Err(PackageError::MalformedArchive);
            }
            (
                u64::from(entries),
                u64::from(central_size),
                u64::from(central_offset),
                eocd,
            )
        };

        enforce_limit("zip_entries", entry_count, usize_to_u64(limits.max_entries))?;
        let entries = usize::try_from(entry_count).map_err(|_| PackageError::MalformedArchive)?;
        let start = usize::try_from(central_offset).map_err(|_| PackageError::MalformedArchive)?;
        let size = usize::try_from(central_size).map_err(|_| PackageError::MalformedArchive)?;
        let end = start
            .checked_add(size)
            .ok_or(PackageError::MalformedArchive)?;
        if end != central_end || end > bytes.len() {
            return Err(PackageError::MalformedArchive);
        }

        let mut cursor = start;
        let mut names = BTreeSet::new();
        for _ in 0..entries {
            cancellation.check()?;
            let fixed_end = cursor
                .checked_add(46)
                .ok_or(PackageError::MalformedArchive)?;
            if fixed_end > end || bytes.get(cursor..cursor + 4) != Some(CENTRAL_FILE_SIGNATURE) {
                return Err(PackageError::MalformedArchive);
            }
            let flags = read_u16(bytes, cursor + 8)?;
            if flags & 1 != 0 {
                return Err(PackageError::EncryptedEntry);
            }
            match read_u16(bytes, cursor + 10)? {
                0 | 8 => {}
                _ => return Err(PackageError::UnsupportedCompression),
            }
            let name_length = usize::from(read_u16(bytes, cursor + 28)?);
            let extra_length = usize::from(read_u16(bytes, cursor + 30)?);
            let comment_length = usize::from(read_u16(bytes, cursor + 32)?);
            let record_end = fixed_end
                .checked_add(name_length)
                .and_then(|value| value.checked_add(extra_length))
                .and_then(|value| value.checked_add(comment_length))
                .ok_or(PackageError::MalformedArchive)?;
            if record_end > end {
                return Err(PackageError::MalformedArchive);
            }
            let name = normalize_package_path(
                &bytes[fixed_end..fixed_end + name_length],
                limits.max_path_bytes,
            )?;
            if !names.insert(name) {
                return Err(PackageError::DuplicatePart);
            }
            cursor = record_end;
        }
        if cursor != end {
            return Err(PackageError::MalformedArchive);
        }
        Ok(Self { start, entries })
    }
}

pub(crate) fn find_eocd(bytes: &[u8]) -> Result<usize, PackageError> {
    if bytes.len() < 22 {
        return Err(PackageError::MalformedArchive);
    }
    let minimum = bytes.len().saturating_sub(22 + usize::from(u16::MAX));
    for position in (minimum..=bytes.len() - 22).rev() {
        if bytes.get(position..position + 4) != Some(EOCD_SIGNATURE) {
            continue;
        }
        let comment_length = usize::from(read_u16(bytes, position + 20)?);
        if position
            .checked_add(22)
            .and_then(|value| value.checked_add(comment_length))
            == Some(bytes.len())
        {
            return Ok(position);
        }
    }
    Err(PackageError::MalformedArchive)
}

fn read_zip64_directory(bytes: &[u8], eocd: usize) -> Result<(u64, u64, u64, usize), PackageError> {
    let locator = eocd.checked_sub(20).ok_or(PackageError::MalformedArchive)?;
    if bytes.get(locator..locator + 4) != Some(ZIP64_LOCATOR_SIGNATURE)
        || read_u32(bytes, locator + 4)? != 0
        || read_u32(bytes, locator + 16)? != 1
    {
        return Err(PackageError::MalformedArchive);
    }
    let zip64_position = usize::try_from(read_u64(bytes, locator + 8)?)
        .map_err(|_| PackageError::MalformedArchive)?;
    if zip64_position > bytes.len().saturating_sub(56) {
        return Err(PackageError::MalformedArchive);
    }
    if bytes.get(zip64_position..zip64_position + 4) != Some(ZIP64_EOCD_SIGNATURE) {
        return Err(PackageError::MalformedArchive);
    }
    let record_size = usize::try_from(read_u64(bytes, zip64_position + 4)?)
        .map_err(|_| PackageError::MalformedArchive)?;
    if zip64_position
        .checked_add(12)
        .and_then(|value| value.checked_add(record_size))
        != Some(locator)
        || record_size < 44
        || read_u32(bytes, zip64_position + 16)? != 0
        || read_u32(bytes, zip64_position + 20)? != 0
    {
        return Err(PackageError::MalformedArchive);
    }
    let entries_on_disk = read_u64(bytes, zip64_position + 24)?;
    let entries = read_u64(bytes, zip64_position + 32)?;
    if entries_on_disk != entries {
        return Err(PackageError::MalformedArchive);
    }
    Ok((
        entries,
        read_u64(bytes, zip64_position + 40)?,
        read_u64(bytes, zip64_position + 48)?,
        zip64_position,
    ))
}

pub(crate) fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, PackageError> {
    let end = offset
        .checked_add(2)
        .ok_or(PackageError::MalformedArchive)?;
    let value = bytes
        .get(offset..end)
        .ok_or(PackageError::MalformedArchive)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

pub(crate) fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, PackageError> {
    let end = offset
        .checked_add(4)
        .ok_or(PackageError::MalformedArchive)?;
    let value = bytes
        .get(offset..end)
        .ok_or(PackageError::MalformedArchive)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

pub(crate) fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, PackageError> {
    let end = offset
        .checked_add(8)
        .ok_or(PackageError::MalformedArchive)?;
    let value = bytes
        .get(offset..end)
        .ok_or(PackageError::MalformedArchive)?;
    Ok(u64::from_le_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}
