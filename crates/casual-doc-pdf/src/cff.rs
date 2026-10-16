//! Making a CID-keyed CFF face's CIDs equal its glyph ids.
//!
//! The content stream draws with `Identity-H`, so each character code is a CID,
//! and the writer emits the shaper's **glyph ids** as those codes. For a
//! TrueType descendant `/CIDToGIDMap/Identity` makes that exact. A
//! `CIDFontType0` descendant has no such map: ISO 32000-2 §9.7.4.2 has a reader
//! turn a CID into a glyph through the embedded CFF program's own `charset`,
//! and readers disagree in practice. MuPDF follows the charset; Poppler uses the
//! code as a glyph id. Noto Sans CJK as published numbers its CIDs like its
//! glyphs, so both agree on it. Any subset of it — every `pyftsubset` output,
//! including the committed test face — keeps the source CIDs, and there MuPDF
//! drew the Hangul as nothing while Poppler drew it correctly; translating the
//! codes to CIDs merely swapped which reader lost it.
//!
//! So the embedded copy's charset is rewritten to the identity — glyph *n* is
//! CID *n* — and every reader agrees with the shaper. The rewrite is in place
//! and in the charset's own format, which never needs more bytes than the
//! original: format 0 lists the same count of entries, a format 1 or 2 charset
//! already spends at least one range per 256 (or 65 536) glyphs. Nothing else in
//! a CID-keyed program is indexed by CID (`FDSelect`, `CharStrings` and every
//! OpenType layout table are indexed by glyph id), and `CIDCount` already
//! exceeds the largest new CID because the old ones were distinct and
//! increasing from 1.
//!
//! Only what that needs is read: the header, the Name INDEX (skipped), the first
//! Top DICT for `ROS` and `charset`, and the charset itself. Every read is
//! bounds-checked and the glyph count is the caller's (already capped at
//! `MAX_GLYPHS` by the face reader), so a hostile face costs O(glyphs) at most
//! and is refused as malformed rather than misread.

use crate::subset::SfntError;

/// `ROS`, the operator that makes a Top DICT CID-keyed (`12 30`).
const OP_ROS: u16 = 0x0c1e;
/// `charset`, the offset of the glyph → SID/CID table (`15`).
const OP_CHARSET: u16 = 15;

const MALFORMED: SfntError = SfntError::MalformedTable("CFF ");

/// How many objects of an INDEX are kept. The rest are validated and skipped.
const MAX_INDEX_OBJECTS: usize = 16;

/// A copy of the `CFF ` table `cff` whose charset is the identity, or `None`
/// when it needs no rewrite: a name-keyed face (readers use the glyph id) or a
/// CID-keyed face whose charset already is the identity.
///
/// `glyphs` is the face's glyph count (`maxp.numGlyphs`). Cost: O(`glyphs`)
/// plus one copy of the table when it is rewritten.
pub(crate) fn with_identity_charset(
    cff: &[u8],
    glyphs: usize,
) -> Result<Option<Vec<u8>>, SfntError> {
    let Some(offset) = cid_charset_offset(cff)? else {
        return Ok(None);
    };
    let charset = read_charset(cff, offset, glyphs)?;
    if charset.is_identity() {
        return Ok(None);
    }
    let rewritten = identity_charset(charset.format, glyphs);
    if rewritten.len() > charset.end - offset {
        // Cannot happen for a well-formed charset (see the module docs); refused
        // rather than written over whatever follows it.
        return Err(MALFORMED);
    }
    let mut table = cff.to_vec();
    table[offset..offset + rewritten.len()].copy_from_slice(&rewritten);
    Ok(Some(table))
}

/// The charset offset of a CID-keyed program, or `None` for a name-keyed one.
fn cid_charset_offset(cff: &[u8]) -> Result<Option<usize>, SfntError> {
    let header_size = usize::from(*cff.get(2).ok_or(MALFORMED)?);
    let (_, after_names) = index(cff, header_size)?;
    let (top_dicts, _) = index(cff, after_names)?;
    let Some(top) = top_dicts.first() else {
        return Err(MALFORMED);
    };
    let mut cid_keyed = false;
    let mut charset = None;
    dict(top, |operator, operands| match operator {
        OP_ROS => cid_keyed = true,
        OP_CHARSET => charset = operands.last().copied(),
        _ => {}
    })?;
    if !cid_keyed {
        return Ok(None);
    }
    // Offsets 0..=2 name the predefined Latin charsets, which a CID-keyed font
    // cannot use.
    charset
        .and_then(|value| usize::try_from(value).ok())
        .filter(|offset| *offset > 2)
        .map(Some)
        .ok_or(MALFORMED)
}

/// A charset as read: its format, the CID of each glyph, and the offset just
/// past it.
struct Charset {
    format: u8,
    cids: Vec<u16>,
    end: usize,
}

impl Charset {
    fn is_identity(&self) -> bool {
        self.cids
            .iter()
            .enumerate()
            .all(|(glyph, cid)| usize::from(*cid) == glyph)
    }
}

/// The charset at `offset`: glyph 0 is always CID 0, and formats 0, 1 and 2
/// list the rest.
fn read_charset(cff: &[u8], offset: usize, glyphs: usize) -> Result<Charset, SfntError> {
    let format = *cff.get(offset).ok_or(MALFORMED)?;
    let mut cids = Vec::with_capacity(glyphs);
    let mut at = offset.checked_add(1).ok_or(MALFORMED)?;
    if glyphs > 0 {
        cids.push(0);
    }
    match format {
        0 => {
            while cids.len() < glyphs {
                cids.push(u16_at(cff, at)?);
                at += 2;
            }
        }
        1 | 2 => {
            while cids.len() < glyphs {
                let first = u16_at(cff, at)?;
                let left = if format == 1 {
                    u16::from(
                        *cff.get(at.checked_add(2).ok_or(MALFORMED)?)
                            .ok_or(MALFORMED)?,
                    )
                } else {
                    u16_at(cff, at.checked_add(2).ok_or(MALFORMED)?)?
                };
                // A range running past the largest CID is malformed, and is
                // refused before any of it is taken.
                first.checked_add(left).ok_or(MALFORMED)?;
                at += if format == 1 { 3 } else { 4 };
                for step in 0..=left {
                    if cids.len() == glyphs {
                        break;
                    }
                    cids.push(first + step);
                }
            }
        }
        _ => return Err(MALFORMED),
    }
    Ok(Charset {
        format,
        cids,
        end: at,
    })
}

/// The identity charset for `glyphs` glyphs, in `format`.
fn identity_charset(format: u8, glyphs: usize) -> Vec<u8> {
    let mut out = vec![format];
    // Glyph 0 is implicit; the rest are 1..glyphs.
    let rest = glyphs.saturating_sub(1);
    match format {
        0 => {
            for cid in 1..=rest {
                out.extend_from_slice(&(cid as u16).to_be_bytes());
            }
        }
        _ => {
            let span = if format == 1 { 256 } else { 65_536 };
            let mut first = 1;
            while first <= rest {
                let count = span.min(rest + 1 - first);
                out.extend_from_slice(&(first as u16).to_be_bytes());
                if format == 1 {
                    out.push((count - 1) as u8);
                } else {
                    out.extend_from_slice(&((count - 1) as u16).to_be_bytes());
                }
                first += count;
            }
        }
    }
    out
}

/// The first [`MAX_INDEX_OBJECTS`] objects of the INDEX at `at`, and the
/// offset just past it.
///
/// Every offset is validated, not only the retained ones: the first is 1, each
/// is no smaller than the one before, and the last stays inside `cff`. All the
/// arithmetic is checked, because the offsets are up to four attacker-chosen
/// bytes and `usize` is 32 bits on `wasm32`. Cost: O(`count`), `count` ≤ 65 535.
fn index(cff: &[u8], at: usize) -> Result<(Vec<&[u8]>, usize), SfntError> {
    let count = usize::from(u16_at(cff, at)?);
    if count == 0 {
        return Ok((Vec::new(), at.checked_add(2).ok_or(MALFORMED)?));
    }
    let size = usize::from(
        *cff.get(at.checked_add(2).ok_or(MALFORMED)?)
            .ok_or(MALFORMED)?,
    );
    if !(1..=4).contains(&size) {
        return Err(MALFORMED);
    }
    let offsets_at = at.checked_add(3).ok_or(MALFORMED)?;
    let offsets_len = count
        .checked_add(1)
        .and_then(|slots| slots.checked_mul(size))
        .ok_or(MALFORMED)?;
    let offsets = cff
        .get(offsets_at..offsets_at.checked_add(offsets_len).ok_or(MALFORMED)?)
        .ok_or(MALFORMED)?;
    // Offsets are 1-based from the byte before the data.
    let data_base = offsets_at + offsets_len - 1;
    let data_len = cff.len() - data_base;
    let mut objects = Vec::with_capacity(count.min(MAX_INDEX_OBJECTS));
    let mut previous = 0_usize;
    for (slot, bytes) in offsets.chunks_exact(size).enumerate() {
        let next = bytes
            .iter()
            .fold(0_usize, |value, byte| (value << 8) | usize::from(*byte));
        let valid = if slot == 0 {
            next == 1
        } else {
            next >= previous && next <= data_len
        };
        if !valid {
            return Err(MALFORMED);
        }
        // Only the first object of each INDEX is ever wanted (the Name and Top
        // DICT INDEXes hold one font each).
        if slot > 0 && objects.len() < MAX_INDEX_OBJECTS {
            objects.push(&cff[data_base + previous..data_base + next]);
        }
        previous = next;
    }
    Ok((objects, data_base + previous))
}

/// Walks a DICT, calling `visit` with each operator and its integer operands.
/// Real operands are skipped over (reported as 0): neither operator read here
/// takes one.
fn dict(data: &[u8], mut visit: impl FnMut(u16, &[i64])) -> Result<(), SfntError> {
    let mut operands: Vec<i64> = Vec::new();
    let mut at = 0;
    while let Some(&b0) = data.get(at) {
        at += 1;
        match b0 {
            0..=11 | 13..=21 => {
                visit(u16::from(b0), &operands);
                operands.clear();
            }
            12 => {
                let b1 = *data.get(at).ok_or(MALFORMED)?;
                at += 1;
                visit(0x0c00 | u16::from(b1), &operands);
                operands.clear();
            }
            28 => {
                operands.push(i64::from(i16::from_be_bytes([
                    *data.get(at).ok_or(MALFORMED)?,
                    *data.get(at + 1).ok_or(MALFORMED)?,
                ])));
                at += 2;
            }
            29 => {
                let bytes = data.get(at..at + 4).ok_or(MALFORMED)?;
                operands.push(i64::from(i32::from_be_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3],
                ])));
                at += 4;
            }
            30 => {
                // A real: nibbles until one is 0xf.
                loop {
                    let byte = *data.get(at).ok_or(MALFORMED)?;
                    at += 1;
                    if byte & 0x0f == 0x0f || byte >> 4 == 0x0f {
                        break;
                    }
                }
                operands.push(0);
            }
            32..=246 => operands.push(i64::from(b0) - 139),
            247..=250 => {
                let b1 = *data.get(at).ok_or(MALFORMED)?;
                at += 1;
                operands.push((i64::from(b0) - 247) * 256 + i64::from(b1) + 108);
            }
            251..=254 => {
                let b1 = *data.get(at).ok_or(MALFORMED)?;
                at += 1;
                operands.push(-(i64::from(b0) - 251) * 256 - i64::from(b1) - 108);
            }
            _ => return Err(MALFORMED),
        }
        if operands.len() > 48 {
            // The CFF limit on a DICT's operand stack.
            return Err(MALFORMED);
        }
    }
    Ok(())
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, SfntError> {
    bytes
        .get(at..at.checked_add(2).ok_or(MALFORMED)?)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .ok_or(MALFORMED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subset::Face;

    const CJK_TEST_FACE: &[u8] =
        include_bytes!("../../../fixtures/fonts/NotoSansCJK-TestSubset.otf");

    fn cff_and_glyphs(bytes: &[u8]) -> (&[u8], usize) {
        let face = Face::parse(bytes, 0).expect("parse");
        let cff = face.table(b"CFF ").expect("a CFF face");
        (cff, face.glyph_count().expect("glyph count"))
    }

    fn charset_of(cff: &[u8], glyphs: usize) -> Vec<u16> {
        let offset = cid_charset_offset(cff)
            .expect("top dict")
            .expect("CID-keyed");
        read_charset(cff, offset, glyphs).expect("charset").cids
    }

    #[test]
    fn a_subset_keeps_its_source_cids_and_the_rewrite_makes_them_its_glyph_ids() {
        let (cff, glyphs) = cff_and_glyphs(CJK_TEST_FACE);
        // fontTools names a CID-keyed glyph by its CID: glyph 1 is `cid01461`.
        let before = charset_of(cff, glyphs);
        assert_eq!(before.len(), glyphs);
        assert_eq!((before[0], before[1]), (0, 1461));

        let rewritten = with_identity_charset(cff, glyphs)
            .expect("rewrite")
            .expect("a non-identity charset is rewritten");
        assert_eq!(rewritten.len(), cff.len(), "rewritten in place");
        let after = charset_of(&rewritten, glyphs);
        assert_eq!(after, (0..glyphs as u16).collect::<Vec<_>>());
        // Only the charset changed: the outlines and dictionaries are untouched.
        let offset = cid_charset_offset(cff).unwrap().unwrap();
        assert_eq!(rewritten[..offset], cff[..offset]);
        // And an identity charset is left alone.
        assert_eq!(with_identity_charset(&rewritten, glyphs), Ok(None));
    }

    #[test]
    fn every_charset_format_is_rewritten_within_its_own_bytes() {
        // The worst cases for each format: the fewest bytes the original can
        // spend on a given glyph count.
        for glyphs in [1, 2, 3, 257, 258, 513, 65_535] {
            let rest = glyphs - 1;
            assert_eq!(identity_charset(0, glyphs).len(), 1 + 2 * rest, "{glyphs}");
            // A format 1 range covers at most 256 glyphs.
            assert!(identity_charset(1, glyphs).len() <= 1 + 3 * rest.div_ceil(256).max(1));
            assert!(identity_charset(2, glyphs).len() <= 5);
            for format in [0, 1, 2] {
                let mut table = identity_charset(format, glyphs);
                table.extend_from_slice(&[0; 4]);
                let read = read_charset(&table, 0, glyphs).expect("reads back");
                assert!(read.is_identity(), "format {format}, {glyphs} glyphs");
            }
        }
    }

    /// An INDEX of `count` objects with `size`-byte `offsets`, then `data`.
    fn index_bytes(size: u8, offsets: &[u32], data: &[u8]) -> Vec<u8> {
        let count = u16::try_from(offsets.len() - 1).unwrap();
        let mut out = count.to_be_bytes().to_vec();
        out.push(size);
        for offset in offsets {
            out.extend_from_slice(&offset.to_be_bytes()[4 - usize::from(size)..]);
        }
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn a_well_formed_index_reads_its_objects_and_its_end() {
        let bytes = index_bytes(1, &[1, 3, 6], b"abcde!");
        let (objects, end) = index(&bytes, 0).expect("well formed");
        assert_eq!(objects, [&b"ab"[..], &b"cde"[..]]);
        assert_eq!(end, bytes.len() - 1, "the byte after the last object");
    }

    #[test]
    fn hostile_index_offsets_are_refused_without_overflow() {
        // On `wasm32` `usize` is 32 bits, so `0xFFFF_FFFF` plus any base wraps:
        // every one of these must be an error, never a panic or a misread.
        let cases: [(&str, u8, &[u32]); 7] = [
            ("the largest four-byte offset", 4, &[1, 0xFFFF_FFFF]),
            ("a first offset of 0xFFFF_FFFF", 4, &[0xFFFF_FFFF, 2]),
            ("a zero first offset", 1, &[0, 2]),
            ("a first offset past 1", 1, &[2, 3]),
            ("a descending offset", 1, &[1, 4, 2]),
            ("a zero offset after the first", 2, &[1, 0]),
            ("an offset past the data", 1, &[1, 9]),
        ];
        for (case, size, offsets) in cases {
            let bytes = index_bytes(size, offsets, b"abcd");
            assert_eq!(index(&bytes, 0), Err(MALFORMED), "{case}");
        }
        // A descending offset AFTER the retained objects is still found.
        let mut offsets = vec![1_u32; 20];
        offsets.push(0);
        let bytes = index_bytes(1, &offsets, b"");
        assert_eq!(
            index(&bytes, 0),
            Err(MALFORMED),
            "validated past the kept 16"
        );
        // An INDEX that starts past the end of the table.
        assert_eq!(index(b"ab", usize::MAX - 1), Err(MALFORMED));
    }

    #[test]
    fn a_charset_range_past_the_largest_cid_is_refused() {
        // Format 1 range starting at 65 535 with 1 more: 65 536 is not a CID.
        assert_eq!(
            read_charset(&[1, 0xFF, 0xFF, 1], 0, 3).map(|charset| charset.cids),
            Err(MALFORMED)
        );
        // Format 2, the same with a two-byte count.
        assert_eq!(
            read_charset(&[2, 0xFF, 0x00, 0x01, 0x00], 0, 3).map(|charset| charset.cids),
            Err(MALFORMED)
        );
        // An unknown format.
        assert_eq!(
            read_charset(&[3, 0, 1], 0, 2).map(|charset| charset.cids),
            Err(MALFORMED)
        );
    }

    #[test]
    fn a_truncated_table_is_refused_rather_than_misread() {
        let (cff, glyphs) = cff_and_glyphs(CJK_TEST_FACE);
        for cut in [3, 8, 64] {
            assert_eq!(
                with_identity_charset(&cff[..cut], glyphs),
                Err(MALFORMED),
                "cut at {cut}"
            );
        }
    }
}
