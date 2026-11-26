// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: a reader of zip archives, for the session archive of another instrument (`tss.rs`, docs/tss.md; oos-plus
//! ADR 0075). It reads the list of entries at the end of the file (`Zip::open`) and the bytes of one entry, stored
//! or deflated, with its CRC-32 checked (`Zip::read`). It writes nothing and unpacks to no file: a name is only ever
//! compared. Refused, each with its reason: zip64, an archive in several parts, an encrypted entry, a method other
//! than stored and deflate, more than `ZIP_ENTRIES` entries, an entry larger than the limit of the caller, a name
//! that leaves the archive, and every length or offset that the bytes do not hold.

use super::*;

/// The most entries of an archive that is read. A session archive seen so far has 2 to 7, the archive inside it 1
/// to 14.
pub(super) const ZIP_ENTRIES: usize = 256;

/// Whether these bytes begin as a zip archive with an entry does.
pub fn looks_zip(b: &[u8]) -> bool {
    b.starts_with(b"PK\x03\x04")
}

/// One entry, as the list at the end of the archive describes it. The sizes are taken from there: the header in
/// front of the entry's bytes has zeros when the writer learned them only after it wrote the bytes.
pub(super) struct ZipEntry {
    pub name: String,
    flags: u16,
    method: u16,
    crc: u32,
    packed: u32,
    size: u32,
    /// where the header in front of the entry's bytes begins
    local: u32,
}

pub(super) struct Zip<'a> {
    bytes: &'a [u8],
    /// where the list of entries begins: the bytes of every entry end before it
    list: usize,
    pub entries: Vec<ZipEntry>,
}

fn le16(b: &[u8], at: usize) -> Option<usize> {
    b.get(at..at.checked_add(2)?).map(|x| u16::from_le_bytes([x[0], x[1]]) as usize)
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at.checked_add(4)?).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

fn cut() -> String {
    "the archive is cut short or damaged".into()
}

const ZIP64: &str = "zip64 archives are not read";

/// A name that stays inside the archive: not empty, without a NUL, not absolute (`/x`, `\x`, `c:x`) and without a
/// `..` step. Nothing is written under a name, but an archive with such a name is not one a program made by saving
/// its state, and it is refused as a whole.
fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('\0')
        && !name.starts_with(['/', '\\'])
        && name.as_bytes().get(1) != Some(&b':')
        && !name.split(['/', '\\']).any(|step| step == "..")
}

impl<'a> Zip<'a> {
    /// The entries of the archive `bytes`. The end record (22 bytes and a comment of at most 65535) is looked for
    /// from the end; it says where the list of entries is and how many it has.
    pub(super) fn open(bytes: &'a [u8]) -> Result<Zip<'a>, String> {
        let last = bytes.len().checked_sub(22).ok_or_else(cut)?;
        let end = (last.saturating_sub(0xFFFF)..=last)
            .rev()
            .find(|&at| {
                bytes[at..].starts_with(b"PK\x05\x06")
                    && le16(bytes, at + 20).is_some_and(|c| at + 22 + c <= bytes.len())
            })
            .ok_or_else(cut)?;
        let field = |at: usize| le16(bytes, end + at).ok_or_else(cut);
        let (disk, list_disk, here, count) = (field(4)?, field(6)?, field(8)?, field(10)?);
        let (size, start) = (le32(bytes, end + 12).ok_or_else(cut)?, le32(bytes, end + 16).ok_or_else(cut)?);
        // a zip64 archive says so with all ones in a field of this record, and has a locator in front of it
        let locator = end.checked_sub(20).is_some_and(|at| bytes[at..].starts_with(b"PK\x06\x07"));
        if locator || count == 0xFFFF || size == u32::MAX || start == u32::MAX {
            return Err(ZIP64.into());
        }
        if disk != 0 || list_disk != 0 || here != count {
            return Err("an archive in several parts is not read".into());
        }
        if count > ZIP_ENTRIES {
            return Err(format!("the archive has {count} entries; at most {ZIP_ENTRIES} are read"));
        }
        let list = start as usize;
        if list.checked_add(size as usize).is_none_or(|stop| stop > end) {
            return Err(cut());
        }
        let mut entries = Vec::with_capacity(count);
        let mut at = list;
        for _ in 0..count {
            // 46 bytes, then the name, an extra field and a comment
            let head =
                bytes.get(at..at + 46).filter(|h| h.starts_with(b"PK\x01\x02") && at + 46 <= end).ok_or_else(cut)?;
            let lengths = le16(head, 28).zip(le16(head, 30)).zip(le16(head, 32));
            let ((n, extra), comment) = lengths.ok_or_else(cut)?;
            let name = bytes.get(at + 46..at + 46 + n).ok_or_else(cut)?;
            at += 46 + n + extra + comment;
            if at > end {
                return Err(cut());
            }
            let name = String::from_utf8_lossy(name).into_owned();
            if !name_ok(&name) {
                return Err("the archive has an entry whose name leaves the archive".into());
            }
            let word = |at: usize| le32(head, at).ok_or_else(cut);
            entries.push(ZipEntry {
                name,
                flags: le16(head, 8).ok_or_else(cut)? as u16,
                method: le16(head, 10).ok_or_else(cut)? as u16,
                crc: word(16)?,
                packed: word(20)?,
                size: word(24)?,
                local: word(42)?,
            });
        }
        Ok(Zip { bytes, list, entries })
    }

    /// The bytes of the entry `e`, when they are at most `limit`: checked against the size the list gives before
    /// anything is allocated, inflated to exactly that size, and checked against the CRC-32 of the list.
    pub(super) fn read(&self, e: &ZipEntry, limit: usize) -> Result<Vec<u8>, String> {
        let name = base_name(&e.name);
        // bit 0: encrypted; bit 6: strong encryption
        if e.flags & 0x41 != 0 {
            return Err(format!("{name} is encrypted and cannot be read"));
        }
        if e.packed == u32::MAX || e.size == u32::MAX || e.local == u32::MAX {
            return Err(ZIP64.into());
        }
        let (packed, size, local) = (e.packed as usize, e.size as usize, e.local as usize);
        if size > limit {
            return Err(format!("{name} exceeds the {} MiB import limit", (limit >> 20).max(1)));
        }
        // the header in front of the bytes: 30 bytes, the name and an extra field of its own length
        let head = self.bytes.get(local..local.saturating_add(30)).filter(|h| h.starts_with(b"PK\x03\x04"));
        let head = head.ok_or_else(cut)?;
        let from = local.saturating_add(30) + le16(head, 26).ok_or_else(cut)? + le16(head, 28).ok_or_else(cut)?;
        let data = self.bytes.get(from..from.saturating_add(packed)).filter(|d| from + d.len() <= self.list);
        let data = data.ok_or_else(cut)?;
        let out = match e.method {
            0 if packed == size => data.to_vec(),
            0 => return Err(cut()),
            8 => inflate(data, size)
                .map(|(out, _)| out)
                .filter(|out| out.len() == size)
                .ok_or_else(|| format!("{name} is damaged: it does not unpack to its size"))?,
            other => return Err(format!("{name} is packed with method {other}; stored and deflated entries are read")),
        };
        if oos_dsp::decode::crc32(&out) != e.crc {
            return Err(format!("{name} is damaged: its check sum is wrong"));
        }
        Ok(out)
    }
}

/// The name of an entry without the folders in front of it, as the messages and the labels have it.
pub(super) fn base_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}
