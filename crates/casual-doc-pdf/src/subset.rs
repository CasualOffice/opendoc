//! Bounded SFNT parsing and TrueType glyph subsetting.
//!
//! Font bytes reaching this module may have come straight out of an imported
//! document, so every read is bounds-checked and every malformed structure is a
//! returned error rather than a panic or a silent truncation (`21-PARSER-LIMITS`
//! discipline). The module never allocates from an attacker-controlled length
//! without first checking it against the slice it was read from.
//!
//! # What the subsetter does
//!
//! It keeps the glyph **identity space** and empties the outlines of glyphs the
//! document does not use. `loca[i] == loca[i + 1]` for a dropped glyph, which is
//! the SFNT spelling of "this glyph has no outline". Keeping glyph ids fixed is
//! what lets the PDF use `/CIDToGIDMap /Identity` and emit the *same* glyph ids
//! the shaper chose, so the exported page is glyph-for-glyph what the editor
//! laid out. The saving is the whole of `glyf`, which is where essentially all
//! of a font's bytes live: a 24-glyph subset of Roboto is ~4 KB against ~170 KB
//! for the face.
//!
//! Composite glyphs are expanded transitively, so a kept composite keeps the
//! components it draws with.

use std::collections::BTreeSet;

/// Tables copied verbatim into the subset, beyond the ones it rebuilds.
///
/// `cvt `/`fpgm`/`prep` are the hinting programs. They are kept because the
/// retained glyphs may carry hinting instructions that call into them; dropping
/// them while keeping the instructions is what makes a subset render subtly
/// wrong in a hinting rasterizer. They are a few kilobytes at most.
///
/// `cmap`, `name`, `post` and `OS/2` are deliberately **not** copied: ISO
/// 32000-1 §9.9.2 lets a CIDFontType2 subset omit them, and they are pure
/// overhead once the PDF addresses glyphs by id.
const COPIED_TABLES: [&[u8; 4]; 3] = [b"cvt ", b"fpgm", b"prep"];

/// The largest font file this module will parse. A face is a document-supplied
/// blob; this bound is what stops a malformed or hostile one from driving an
/// allocation the size of the address space.
pub(crate) const MAX_FONT_BYTES: usize = 64 * 1024 * 1024;

/// The largest glyph count this module will build tables for. Real faces top
/// out around 65 535 by format; the bound is stated so the allocation is not
/// implied by an attacker-controlled `maxp`.
pub(crate) const MAX_GLYPHS: usize = 65_536;

/// A structural failure while reading or subsetting a face.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SfntError {
    /// The blob is larger than [`MAX_FONT_BYTES`].
    TooLarge(usize),
    /// The blob is not a recognizable SFNT, OpenType or TrueType collection.
    NotAFont,
    /// A collection index addressed a face the collection does not contain.
    NoSuchFace(u32),
    /// A required table is missing.
    MissingTable(&'static str),
    /// A table's declared extent lies outside the file, or its contents are
    /// shorter than the format requires.
    MalformedTable(&'static str),
    /// `maxp` declares more glyphs than [`MAX_GLYPHS`].
    TooManyGlyphs(usize),
}

impl core::fmt::Display for SfntError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooLarge(bytes) => {
                write!(formatter, "font file is {bytes} bytes, over the limit")
            }
            Self::NotAFont => formatter.write_str("not a TrueType or OpenType font"),
            Self::NoSuchFace(index) => write!(formatter, "font collection has no face {index}"),
            Self::MissingTable(tag) => write!(formatter, "font is missing the `{tag}` table"),
            Self::MalformedTable(tag) => write!(formatter, "font's `{tag}` table is malformed"),
            Self::TooManyGlyphs(count) => write!(formatter, "font declares {count} glyphs"),
        }
    }
}

/// One face's table directory, resolved against the whole file.
#[derive(Clone, Debug)]
pub(crate) struct Face<'a> {
    bytes: &'a [u8],
    /// `(tag, offset, length)` for each table, in the directory's order.
    tables: Vec<([u8; 4], usize, usize)>,
    /// The `sfntVersion` of this face (`0x00010000` for TrueType outlines,
    /// `OTTO` for CFF outlines).
    version: u32,
}

impl<'a> Face<'a> {
    /// Resolves face `index` of `bytes`, accepting a bare SFNT or a `ttcf`
    /// collection.
    pub(crate) fn parse(bytes: &'a [u8], index: u32) -> Result<Self, SfntError> {
        if bytes.len() > MAX_FONT_BYTES {
            return Err(SfntError::TooLarge(bytes.len()));
        }
        let directory =
            if read_u32(bytes, 0).ok_or(SfntError::NotAFont)? == u32::from_be_bytes(*b"ttcf") {
                let count = read_u32(bytes, 8).ok_or(SfntError::NotAFont)?;
                if index >= count {
                    return Err(SfntError::NoSuchFace(index));
                }
                let entry =
                    12 + 4 * usize::try_from(index).map_err(|_| SfntError::NoSuchFace(index))?;
                usize::try_from(read_u32(bytes, entry).ok_or(SfntError::NotAFont)?)
                    .map_err(|_| SfntError::NotAFont)?
            } else {
                0
            };
        let version = read_u32(bytes, directory).ok_or(SfntError::NotAFont)?;
        if !matches!(version, 0x0001_0000 | 0x7472_7565 /* "true" */)
            && version != u32::from_be_bytes(*b"OTTO")
        {
            return Err(SfntError::NotAFont);
        }
        let count = usize::from(read_u16(bytes, directory + 4).ok_or(SfntError::NotAFont)?);
        let mut tables = Vec::with_capacity(count);
        for slot in 0..count {
            let record = directory + 12 + slot * 16;
            let tag = read_tag(bytes, record).ok_or(SfntError::NotAFont)?;
            let offset = usize::try_from(read_u32(bytes, record + 8).ok_or(SfntError::NotAFont)?)
                .map_err(|_| SfntError::NotAFont)?;
            let length = usize::try_from(read_u32(bytes, record + 12).ok_or(SfntError::NotAFont)?)
                .map_err(|_| SfntError::NotAFont)?;
            // A table that runs past the end of the file is dropped rather than
            // trusted; a required one then surfaces as `MissingTable`.
            if offset
                .checked_add(length)
                .is_some_and(|end| end <= bytes.len())
            {
                tables.push((tag, offset, length));
            }
        }
        Ok(Self {
            bytes,
            tables,
            version,
        })
    }

    /// The bytes of one table, if the face has it.
    pub(crate) fn table(&self, tag: &[u8; 4]) -> Option<&'a [u8]> {
        self.tables
            .iter()
            .find(|(candidate, _, _)| candidate == tag)
            .map(|(_, offset, length)| &self.bytes[*offset..*offset + *length])
    }

    /// Whether this face draws with CFF (PostScript) outlines rather than
    /// `glyf` ones. Those are embedded whole rather than subset.
    pub(crate) fn is_cff(&self) -> bool {
        self.table(b"CFF ").is_some() || self.table(b"CFF2").is_some()
    }

    /// Whether the face was selected out of a `ttcf` collection or otherwise
    /// needs rebuilding into a standalone file before it can be embedded.
    pub(crate) fn is_collection_member(&self) -> bool {
        read_u32(self.bytes, 0) == Some(u32::from_be_bytes(*b"ttcf"))
    }

    /// The glyph count from `maxp`.
    pub(crate) fn glyph_count(&self) -> Result<usize, SfntError> {
        let maxp = self.table(b"maxp").ok_or(SfntError::MissingTable("maxp"))?;
        let count = usize::from(read_u16(maxp, 4).ok_or(SfntError::MalformedTable("maxp"))?);
        if count > MAX_GLYPHS {
            return Err(SfntError::TooManyGlyphs(count));
        }
        Ok(count)
    }

    /// `head.unitsPerEm`, the design grid the PDF `/W` array and font matrix are
    /// expressed against.
    pub(crate) fn units_per_em(&self) -> Result<u16, SfntError> {
        let head = self.table(b"head").ok_or(SfntError::MissingTable("head"))?;
        let units = read_u16(head, 18).ok_or(SfntError::MalformedTable("head"))?;
        if units == 0 {
            return Err(SfntError::MalformedTable("head"));
        }
        Ok(units)
    }

    /// The font bounding box in design units, from `head`.
    pub(crate) fn bounding_box(&self) -> Result<[i16; 4], SfntError> {
        let head = self.table(b"head").ok_or(SfntError::MissingTable("head"))?;
        Ok([
            read_i16(head, 36).ok_or(SfntError::MalformedTable("head"))?,
            read_i16(head, 38).ok_or(SfntError::MalformedTable("head"))?,
            read_i16(head, 40).ok_or(SfntError::MalformedTable("head"))?,
            read_i16(head, 42).ok_or(SfntError::MalformedTable("head"))?,
        ])
    }

    /// `hhea` ascender and descender in design units.
    pub(crate) fn vertical_metrics(&self) -> Result<(i16, i16), SfntError> {
        let hhea = self.table(b"hhea").ok_or(SfntError::MissingTable("hhea"))?;
        Ok((
            read_i16(hhea, 4).ok_or(SfntError::MalformedTable("hhea"))?,
            read_i16(hhea, 6).ok_or(SfntError::MalformedTable("hhea"))?,
        ))
    }

    /// The italic angle in degrees from `post`, or `0` when the face omits it.
    pub(crate) fn italic_angle(&self) -> f32 {
        let Some(fixed) = self.table(b"post").and_then(|post| read_u32(post, 4)) else {
            return 0.0;
        };
        // `post.italicAngle` is a signed 16.16 fixed-point value.
        let signed = i64::from(fixed) - if fixed >= 0x8000_0000 { 1_i64 << 32 } else { 0 };
        signed as f32 / 65536.0
    }

    /// `OS/2.fsType`, the embedding-permission bit field, or `None` when the
    /// face has no `OS/2` table (which the specification treats as installable).
    pub(crate) fn embedding_permissions(&self) -> Option<u16> {
        self.table(b"OS/2").and_then(|os2| read_u16(os2, 8))
    }

    /// `OS/2.usWeightClass`, used for the PDF `/StemV` estimate.
    pub(crate) fn weight_class(&self) -> u16 {
        self.table(b"OS/2")
            .and_then(|os2| read_u16(os2, 4))
            .unwrap_or(400)
    }

    /// Whether `post` marks the face as monospaced.
    pub(crate) fn is_fixed_pitch(&self) -> bool {
        self.table(b"post")
            .and_then(|post| read_u32(post, 12))
            .is_some_and(|flag| flag != 0)
    }

    /// The advance width of `glyph`, in design units, from `hmtx`.
    ///
    /// Glyphs past `numberOfHMetrics` share the last entry's advance, which is
    /// how a monospaced tail is encoded.
    pub(crate) fn advance(&self, glyph: u16) -> Result<u16, SfntError> {
        let hhea = self.table(b"hhea").ok_or(SfntError::MissingTable("hhea"))?;
        let hmtx = self.table(b"hmtx").ok_or(SfntError::MissingTable("hmtx"))?;
        let metrics = usize::from(read_u16(hhea, 34).ok_or(SfntError::MalformedTable("hhea"))?);
        if metrics == 0 {
            return Err(SfntError::MalformedTable("hhea"));
        }
        let slot = usize::from(glyph).min(metrics - 1);
        read_u16(hmtx, slot * 4).ok_or(SfntError::MalformedTable("hmtx"))
    }

    /// Reads `loca` into absolute `glyf` offsets, one per glyph plus a final
    /// end sentinel.
    fn glyph_offsets(&self) -> Result<Vec<u32>, SfntError> {
        let head = self.table(b"head").ok_or(SfntError::MissingTable("head"))?;
        let long = read_i16(head, 50).ok_or(SfntError::MalformedTable("head"))? != 0;
        let loca = self.table(b"loca").ok_or(SfntError::MissingTable("loca"))?;
        let glyphs = self.glyph_count()?;
        let mut offsets = Vec::with_capacity(glyphs + 1);
        for slot in 0..=glyphs {
            let value = if long {
                read_u32(loca, slot * 4).ok_or(SfntError::MalformedTable("loca"))?
            } else {
                u32::from(read_u16(loca, slot * 2).ok_or(SfntError::MalformedTable("loca"))?) * 2
            };
            offsets.push(value);
        }
        Ok(offsets)
    }

    /// The `glyf` bytes of one glyph, or an empty slice for a glyph with no
    /// outline (a space, or one this subset dropped).
    fn glyph_data(
        &self,
        glyph: usize,
        offsets: &[u32],
        glyf: &'a [u8],
    ) -> Result<&'a [u8], SfntError> {
        let (start, end) = (offsets[glyph], offsets[glyph + 1]);
        if end <= start {
            return Ok(&[]);
        }
        let start = usize::try_from(start).map_err(|_| SfntError::MalformedTable("loca"))?;
        let end = usize::try_from(end).map_err(|_| SfntError::MalformedTable("loca"))?;
        glyf.get(start..end)
            .ok_or(SfntError::MalformedTable("glyf"))
    }
}

/// Expands `requested` with every glyph reached through a composite glyph's
/// components, so a kept composite keeps the pieces it is assembled from.
///
/// Glyph `0` (`.notdef`) is always retained: a viewer substitutes it for any
/// code the content stream names that the subset does not cover, and a font
/// without it is malformed.
pub(crate) fn closure(
    face: &Face<'_>,
    requested: &BTreeSet<u16>,
) -> Result<BTreeSet<u16>, SfntError> {
    let glyphs = face.glyph_count()?;
    let offsets = face.glyph_offsets()?;
    let glyf = face.table(b"glyf").ok_or(SfntError::MissingTable("glyf"))?;
    let mut kept: BTreeSet<u16> = BTreeSet::new();
    kept.insert(0);
    let mut pending: Vec<u16> = requested.iter().copied().chain([0]).collect();
    while let Some(glyph) = pending.pop() {
        if usize::from(glyph) >= glyphs {
            continue;
        }
        if !kept.insert(glyph) && glyph != 0 {
            continue;
        }
        let data = face.glyph_data(usize::from(glyph), &offsets, glyf)?;
        for component in composite_components(data) {
            if !kept.contains(&component) {
                pending.push(component);
            }
        }
    }
    Ok(kept)
}

/// The component glyph ids of a composite glyph; empty for a simple one.
///
/// A truncated component record ends the walk instead of reading past the
/// glyph, so a malformed composite loses components rather than the process.
fn composite_components(data: &[u8]) -> Vec<u16> {
    const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
    const WE_HAVE_A_SCALE: u16 = 0x0008;
    const MORE_COMPONENTS: u16 = 0x0020;
    const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
    const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;

    let Some(contours) = read_i16(data, 0) else {
        return Vec::new();
    };
    if contours >= 0 {
        return Vec::new();
    }
    let mut components = Vec::new();
    let mut cursor = 10;
    while let (Some(flags), Some(glyph)) = (read_u16(data, cursor), read_u16(data, cursor + 2)) {
        components.push(glyph);
        cursor += 4;
        cursor += if flags & ARG_1_AND_2_ARE_WORDS != 0 {
            4
        } else {
            2
        };
        if flags & WE_HAVE_A_SCALE != 0 {
            cursor += 2;
        } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
            cursor += 4;
        } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
            cursor += 8;
        }
        if flags & MORE_COMPONENTS == 0 || cursor >= data.len() {
            break;
        }
    }
    components
}

/// Builds a standalone TrueType file carrying only `kept`'s outlines.
///
/// The returned font keeps the original glyph ids, so the content stream's
/// glyph ids need no remapping, and reports the same `unitsPerEm`, advances and
/// bounding box as the face the shaper measured with.
pub(crate) fn subset_truetype(face: &Face<'_>, kept: &BTreeSet<u16>) -> Result<Vec<u8>, SfntError> {
    let glyphs = face.glyph_count()?;
    let offsets = face.glyph_offsets()?;
    let glyf = face.table(b"glyf").ok_or(SfntError::MissingTable("glyf"))?;

    let mut new_glyf: Vec<u8> = Vec::new();
    let mut new_loca: Vec<u8> = Vec::with_capacity((glyphs + 1) * 4);
    for glyph in 0..glyphs {
        new_loca.extend_from_slice(
            &u32::try_from(new_glyf.len())
                .map_err(|_| SfntError::MalformedTable("glyf"))?
                .to_be_bytes(),
        );
        let keep = u16::try_from(glyph).is_ok_and(|id| kept.contains(&id));
        if keep {
            new_glyf.extend_from_slice(face.glyph_data(glyph, &offsets, glyf)?);
            // Glyph records are conventionally 4-byte aligned; a long `loca`
            // permits any offset, but padding keeps older consumers happy.
            while !new_glyf.len().is_multiple_of(4) {
                new_glyf.push(0);
            }
        }
    }
    new_loca.extend_from_slice(
        &u32::try_from(new_glyf.len())
            .map_err(|_| SfntError::MalformedTable("glyf"))?
            .to_be_bytes(),
    );

    let mut head = face
        .table(b"head")
        .ok_or(SfntError::MissingTable("head"))?
        .to_vec();
    if head.len() < 54 {
        return Err(SfntError::MalformedTable("head"));
    }
    // The rebuilt `loca` is always long-format, whatever the source used.
    head[50..52].copy_from_slice(&1_i16.to_be_bytes());
    // `checkSumAdjustment` is computed over the finished file, so it is zeroed
    // first and patched at the end.
    head[8..12].copy_from_slice(&0_u32.to_be_bytes());

    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"glyf", new_glyf),
        (*b"head", head),
        (
            *b"hhea",
            face.table(b"hhea")
                .ok_or(SfntError::MissingTable("hhea"))?
                .to_vec(),
        ),
        (
            *b"hmtx",
            face.table(b"hmtx")
                .ok_or(SfntError::MissingTable("hmtx"))?
                .to_vec(),
        ),
        (*b"loca", new_loca),
        (
            *b"maxp",
            face.table(b"maxp")
                .ok_or(SfntError::MissingTable("maxp"))?
                .to_vec(),
        ),
    ];
    for tag in COPIED_TABLES {
        if let Some(bytes) = face.table(tag) {
            tables.push((*tag, bytes.to_vec()));
        }
    }
    tables.sort_by_key(|(tag, _)| *tag);
    Ok(assemble(0x0001_0000, tables))
}

/// Rebuilds a face selected out of a collection into a standalone file, copying
/// every table verbatim. Used for CFF faces, which this module does not subset.
pub(crate) fn extract_face(face: &Face<'_>) -> Result<Vec<u8>, SfntError> {
    let mut tables: Vec<([u8; 4], Vec<u8>)> = face
        .tables
        .iter()
        .map(|(tag, offset, length)| (*tag, face.bytes[*offset..*offset + *length].to_vec()))
        .collect();
    tables.sort_by_key(|(tag, _)| *tag);
    if tables.is_empty() {
        return Err(SfntError::NotAFont);
    }
    Ok(assemble(face.version, tables))
}

/// Writes a table directory and its tables into one SFNT file, filling in every
/// checksum including `head.checkSumAdjustment`.
fn assemble(version: u32, tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    let count = u16::try_from(tables.len()).unwrap_or(u16::MAX);
    let entry_selector = 15_u16.saturating_sub(count.leading_zeros() as u16).min(15);
    let search_range = (1_u16 << entry_selector).saturating_mul(16);
    let range_shift = count.saturating_mul(16).saturating_sub(search_range);

    let directory_len = 12 + tables.len() * 16;
    let mut body_offset = directory_len;
    let mut directory = Vec::with_capacity(directory_len);
    directory.extend_from_slice(&version.to_be_bytes());
    directory.extend_from_slice(&count.to_be_bytes());
    directory.extend_from_slice(&search_range.to_be_bytes());
    directory.extend_from_slice(&entry_selector.to_be_bytes());
    directory.extend_from_slice(&range_shift.to_be_bytes());

    let mut body = Vec::new();
    let mut head_offset = None;
    for (tag, bytes) in &tables {
        if tag == b"head" {
            head_offset = Some(body_offset);
        }
        directory.extend_from_slice(tag);
        directory.extend_from_slice(&table_checksum(bytes).to_be_bytes());
        directory.extend_from_slice(&(body_offset as u32).to_be_bytes());
        directory.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        body.extend_from_slice(bytes);
        while !body.len().is_multiple_of(4) {
            body.push(0);
        }
        body_offset = directory_len + body.len();
    }

    let mut file = directory;
    file.extend_from_slice(&body);
    if let Some(head) = head_offset
        && file.len() >= head + 12
    {
        let adjustment = 0xB1B0_AFBA_u32.wrapping_sub(table_checksum(&file));
        file[head + 8..head + 12].copy_from_slice(&adjustment.to_be_bytes());
    }
    file
}

/// The SFNT checksum: the sum of the table's bytes read as big-endian `u32`s,
/// with the tail zero-padded.
fn table_checksum(bytes: &[u8]) -> u32 {
    let mut sum = 0_u32;
    for chunk in bytes.chunks(4) {
        let mut word = [0_u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum = sum.wrapping_add(u32::from_be_bytes(word));
    }
    sum
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    bytes
        .get(offset..offset + 2)
        .map(|slice| u16::from_be_bytes([slice[0], slice[1]]))
}

fn read_i16(bytes: &[u8], offset: usize) -> Option<i16> {
    read_u16(bytes, offset).map(|value| value as i16)
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|slice| u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_tag(bytes: &[u8], offset: usize) -> Option<[u8; 4]> {
    bytes
        .get(offset..offset + 4)
        .map(|slice| [slice[0], slice[1], slice[2], slice[3]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_layout::fonts;
    use casual_doc_layout::text::FontId;

    fn roboto() -> &'static [u8] {
        fonts::face_bytes(FontId(0))
    }

    #[test]
    fn a_subset_keeps_glyph_ids_and_drops_unused_outlines() {
        let face = Face::parse(roboto(), 0).expect("parse bundled face");
        let kept: BTreeSet<u16> = [3, 36, 37, 38].into_iter().collect();
        let subset = subset_truetype(&face, &kept).expect("subset");
        let reparsed = Face::parse(&subset, 0).expect("reparse subset");
        assert_eq!(reparsed.glyph_count().unwrap(), face.glyph_count().unwrap());
        assert_eq!(
            reparsed.units_per_em().unwrap(),
            face.units_per_em().unwrap()
        );
        // The kept glyph still has its outline; a dropped one is now empty.
        let offsets = reparsed.glyph_offsets().unwrap();
        assert!(offsets[37] > offsets[36], "glyph 36 kept its outline");
        assert_eq!(offsets[200], offsets[201], "glyph 200 was dropped");
        assert!(
            subset.len() * 4 < roboto().len(),
            "subset {} vs face {}",
            subset.len(),
            roboto().len()
        );
    }

    #[test]
    fn a_composite_glyph_keeps_the_components_it_draws_with() {
        let face = Face::parse(roboto(), 0).expect("parse bundled face");
        let glyphs = face.glyph_count().unwrap();
        let offsets = face.glyph_offsets().unwrap();
        let glyf = face.table(b"glyf").unwrap();
        // Find any composite glyph in the face and prove the closure pulls in
        // its components.
        let composite = (1..glyphs)
            .find(|glyph| {
                let data = face.glyph_data(*glyph, &offsets, glyf).unwrap_or_default();
                !composite_components(data).is_empty()
            })
            .expect("a bundled face has composite glyphs");
        let id = u16::try_from(composite).unwrap();
        let requested: BTreeSet<u16> = [id].into_iter().collect();
        let kept = closure(&face, &requested).unwrap();
        let components = composite_components(face.glyph_data(composite, &offsets, glyf).unwrap());
        for component in components {
            assert!(kept.contains(&component), "component {component} retained");
        }
    }

    #[test]
    fn a_truncated_font_is_an_error_not_a_panic() {
        let bytes = roboto();
        for cut in [0, 1, 4, 11, 12, 40, 200, 4096] {
            let outcome = Face::parse(&bytes[..cut.min(bytes.len())], 0);
            if let Ok(face) = outcome {
                // Parsing may succeed on a truncated directory; every later read
                // must still be a returned error.
                let _ = face.glyph_count();
                let _ = face.units_per_em();
                let _ = face.glyph_offsets();
                let _ = subset_truetype(&face, &BTreeSet::new());
            }
        }
    }

    #[test]
    fn a_collection_index_past_the_end_is_refused() {
        assert!(
            Face::parse(roboto(), 7).is_ok(),
            "a bare SFNT ignores the face index"
        );
        let mut collection = Vec::from(*b"ttcf");
        collection.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
        collection.extend_from_slice(&1_u32.to_be_bytes());
        collection.extend_from_slice(&16_u32.to_be_bytes());
        assert_eq!(
            Face::parse(&collection, 3).err(),
            Some(SfntError::NoSuchFace(3))
        );
    }
}
