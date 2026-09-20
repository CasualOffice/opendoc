//! Embedding the faces the layout pass actually used.
//!
//! The parity rule from `docs/98` is that the PDF must carry *the same face,
//! the same glyph ids and the same advances* the shaper resolved. So this
//! module never re-resolves a font: it takes the `FontId` a [`GlyphRun`] was
//! shaped with, asks the caller's font source for those exact bytes, keeps the
//! glyph identity space intact through subsetting, and writes a `Type0` font
//! with `Identity-H` encoding whose CIDs *are* the shaper's glyph ids.
//!
//! [`GlyphRun`]: casual_doc_layout::text::GlyphRun

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use casual_doc_layout::text::FontId;
use skrifa::FontRef;
use skrifa::MetadataProvider;

use crate::subset::Face;
use crate::subset::SfntError;
use crate::subset::closure;
use crate::subset::extract_face;
use crate::subset::subset_truetype;
use crate::writer::Ref;
use crate::writer::Writer;
use crate::writer::name as pdf_name;
use crate::writer::num;

/// `OS/2.fsType` bit 1: the face may not be embedded in a document at all.
const FSTYPE_RESTRICTED: u16 = 0x0002;
/// `OS/2.fsType` bit 9: the face may be embedded whole but not subset.
const FSTYPE_NO_SUBSETTING: u16 = 0x0200;

/// Why a face could not be embedded. Every variant is reported to the caller;
/// none of them results in text being dropped silently.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FontError {
    /// The font source served no bytes for a `FontId` the display list used.
    Unavailable(u32),
    /// The face is structurally unreadable.
    Malformed(u32, SfntError),
    /// The face's own `OS/2.fsType` forbids embedding it in a document.
    EmbeddingRestricted(u32, String),
}

impl core::fmt::Display for FontError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unavailable(id) => {
                write!(formatter, "no font bytes were available for font {id}")
            }
            Self::Malformed(id, error) => write!(formatter, "font {id} is unusable: {error}"),
            Self::EmbeddingRestricted(id, face) => write!(
                formatter,
                "font {id} (`{face}`) forbids embedding: its OS/2 fsType is set to restricted licence"
            ),
        }
    }
}

/// Supplies the font-file bytes for a [`FontId`].
///
/// This is deliberately the same shape as `casual-doc-render`'s `GlyphSource`:
/// both backends must be served the *same* face for the same id, or the PDF and
/// the screen disagree about where text sits.
pub trait PdfFontSource {
    /// The font-file bytes for `font`, or `None` when the id is unknown.
    fn font_data(&self, font: FontId) -> Option<&[u8]>;

    /// The face index within a collection file (`.ttc`); `0` for a single-face
    /// file, which is the common case.
    fn face_index(&self, _font: FontId) -> u32 {
        0
    }
}

/// Everything one embedded face contributes to the file.
pub(crate) struct PlannedFont {
    /// The `/F…` resource name used in content streams.
    pub(crate) resource: String,
    /// The `Type0` font object every page resource dictionary points at.
    pub(crate) object: Ref,
    /// The glyph ids the document actually draws with this face.
    pub(crate) glyphs: BTreeSet<u16>,
    /// The face's design grid, needed to convert advances into text space.
    pub(crate) units_per_em: u16,
}

/// Collects the faces a document uses and writes them into the file.
pub(crate) struct FontTable {
    planned: BTreeMap<u32, PlannedFont>,
    /// The number of `/F…` names handed out so far.
    next: usize,
}

impl FontTable {
    pub(crate) fn new() -> Self {
        Self {
            planned: BTreeMap::new(),
            next: 0,
        }
    }

    /// Records that `font` draws `glyphs`, reserving its PDF objects the first
    /// time the face is seen. Returns the resource name to reference it by.
    pub(crate) fn use_face(
        &mut self,
        writer: &mut Writer,
        fonts: &dyn PdfFontSource,
        font: FontId,
        glyphs: impl IntoIterator<Item = u16>,
    ) -> Result<String, FontError> {
        if !self.planned.contains_key(&font.0) {
            let bytes = fonts
                .font_data(font)
                .filter(|bytes| !bytes.is_empty())
                .ok_or(FontError::Unavailable(font.0))?;
            let face = Face::parse(bytes, fonts.face_index(font))
                .map_err(|error| FontError::Malformed(font.0, error))?;
            let units_per_em = face
                .units_per_em()
                .map_err(|error| FontError::Malformed(font.0, error))?;
            self.next += 1;
            self.planned.insert(
                font.0,
                PlannedFont {
                    resource: format!("F{}", self.next),
                    object: writer.reserve(),
                    glyphs: BTreeSet::new(),
                    units_per_em,
                },
            );
        }
        let planned = self
            .planned
            .get_mut(&font.0)
            .expect("the face was just planned");
        planned.glyphs.extend(glyphs);
        Ok(planned.resource.clone())
    }

    /// The design grid of an already-planned face.
    pub(crate) fn units_per_em(&self, font: FontId) -> Option<u16> {
        self.planned
            .get(&font.0)
            .map(|planned| planned.units_per_em)
    }

    /// The `/Font` sub-dictionary entries for every planned face, in resource
    /// order. Pages share one font dictionary rather than each carrying a
    /// subset of it: a font object is referenced, not copied.
    pub(crate) fn resource_dictionary(&self) -> String {
        let mut entries = String::new();
        for planned in self.planned.values() {
            entries.push_str(&pdf_name(&planned.resource));
            entries.push(' ');
            entries.push_str(&planned.object.reference());
        }
        entries
    }

    /// Whether any face was used at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.planned.is_empty()
    }

    /// Writes every planned face: the `Type0` wrapper, its CID descendant, the
    /// descriptor, the embedded (subset) font program and the `ToUnicode` map.
    ///
    /// Returns one [`EmbeddedFace`] report per face so the caller can account
    /// for what was subset and what had to be embedded whole.
    pub(crate) fn write(
        self,
        writer: &mut Writer,
        fonts: &dyn PdfFontSource,
    ) -> Result<Vec<EmbeddedFace>, FontError> {
        let mut reports = Vec::with_capacity(self.planned.len());
        for (id, planned) in self.planned {
            let font = FontId(id);
            let bytes = fonts.font_data(font).ok_or(FontError::Unavailable(id))?;
            let index = fonts.face_index(font);
            let face =
                Face::parse(bytes, index).map_err(|error| FontError::Malformed(id, error))?;
            reports.push(write_face(writer, &face, bytes, index, id, &planned)?);
        }
        Ok(reports)
    }
}

/// What happened to one face on its way into the file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddedFace {
    /// The PostScript name written into `/BaseFont`, including the subset tag.
    pub base_font: String,
    /// The glyph count retained after subsetting.
    pub glyphs: usize,
    /// The byte length of the font program actually embedded.
    pub embedded_bytes: usize,
    /// The byte length of the face as the font source served it.
    pub source_bytes: usize,
    /// Whether the embedded program is a subset. `false` means the whole face
    /// was embedded, which happens for CFF (PostScript) outlines and for faces
    /// whose `OS/2.fsType` forbids subsetting.
    pub subset: bool,
    /// Glyph ids for which no character could be recovered, so copying that
    /// glyph out of the PDF yields nothing. See
    /// [`crate::PdfFinding`] code `pdf.font.unmapped_glyphs`.
    pub unmapped_glyphs: usize,
}

fn write_face(
    writer: &mut Writer,
    face: &Face<'_>,
    source: &[u8],
    index: u32,
    id: u32,
    planned: &PlannedFont,
) -> Result<EmbeddedFace, FontError> {
    let fail = |error: SfntError| FontError::Malformed(id, error);
    let postscript = postscript_name(face);
    if face
        .embedding_permissions()
        .is_some_and(|flags| flags & FSTYPE_RESTRICTED != 0)
    {
        return Err(FontError::EmbeddingRestricted(id, postscript));
    }

    let kept = closure(face, &planned.glyphs).or_else(|error| {
        // A CFF face has no `glyf`/`loca` to walk; its whole program is
        // embedded, so the requested set stands as-is.
        if face.is_cff() {
            Ok(planned.glyphs.clone())
        } else {
            Err(fail(error))
        }
    })?;
    let no_subsetting = face
        .embedding_permissions()
        .is_some_and(|flags| flags & FSTYPE_NO_SUBSETTING != 0);
    let subsettable = !face.is_cff() && !no_subsetting;
    let program = if subsettable {
        subset_truetype(face, &kept).map_err(fail)?
    } else if face.is_collection_member() || no_subsetting {
        extract_face(face).map_err(fail)?
    } else {
        source.to_vec()
    };

    let units = f32::from(planned.units_per_em);
    let scale = 1000.0 / units;
    let bbox = face.bounding_box().map_err(fail)?;
    let (ascent, descent) = face.vertical_metrics().map_err(fail)?;
    let italic = face.italic_angle();
    // A widely used descriptor estimate: the descriptor's `/StemV` is
    // advisory (viewers use it only for synthetic substitution), and no table
    // carries it, so it is derived from the weight class.
    let stem_v = 50.0 + (f32::from(face.weight_class()) / 65.0).powi(2);
    let mut flags = 0b100_u32; // Symbolic: the embedded program carries no `cmap`.
    if face.is_fixed_pitch() {
        flags |= 0b1;
    }
    if italic != 0.0 {
        flags |= 0b100_0000;
    }

    let tag = subset_tag(&postscript, &kept);
    let base_font = format!("{tag}+{postscript}");
    let descendant = writer.reserve();
    let descriptor = writer.reserve();
    let program_object = writer.reserve();
    let to_unicode = writer.reserve();

    let (cmap, unmapped) = to_unicode_cmap(source, index, &kept);
    writer.stream(to_unicode, "", cmap.as_bytes(), true);

    let embedded_bytes = if face.is_cff() {
        writer.stream(program_object, "/Subtype/OpenType", &program, true)
    } else {
        writer.stream(
            program_object,
            &format!("/Length1 {}", program.len()),
            &program,
            true,
        )
    };

    let font_file = if face.is_cff() {
        format!("/FontFile3 {}", program_object.reference())
    } else {
        format!("/FontFile2 {}", program_object.reference())
    };
    writer.object(
        descriptor,
        &format!(
            "<</Type/FontDescriptor/FontName{}/Flags {flags}/FontBBox[{} {} {} {}]/ItalicAngle {}/Ascent {}/Descent {}/CapHeight {}/StemV {}{font_file}>>",
            pdf_name(&base_font),
            num(f32::from(bbox[0]) * scale),
            num(f32::from(bbox[1]) * scale),
            num(f32::from(bbox[2]) * scale),
            num(f32::from(bbox[3]) * scale),
            num(italic),
            num(f32::from(ascent) * scale),
            num(f32::from(descent) * scale),
            num(cap_height(face, ascent) * scale),
            num(stem_v),
        ),
    );

    writer.object(
        descendant,
        &format!(
            "<</Type/Font/Subtype/{}/BaseFont{}/CIDSystemInfo<</Registry(Adobe)/Ordering(Identity)/Supplement 0>>/FontDescriptor {}/DW 1000/W[{}]/CIDToGIDMap/Identity>>",
            if face.is_cff() {
                "CIDFontType0"
            } else {
                "CIDFontType2"
            },
            pdf_name(&base_font),
            descriptor.reference(),
            widths(face, &kept, scale),
        ),
    );

    writer.object(
        planned.object,
        &format!(
            "<</Type/Font/Subtype/Type0/BaseFont{}/Encoding/Identity-H/DescendantFonts[{}]/ToUnicode {}>>",
            pdf_name(&base_font),
            descendant.reference(),
            to_unicode.reference(),
        ),
    );

    Ok(EmbeddedFace {
        base_font,
        glyphs: kept.len(),
        embedded_bytes,
        source_bytes: source.len(),
        subset: subsettable,
        unmapped_glyphs: unmapped,
    })
}

/// `OS/2.sCapHeight` when the face publishes one, else a conventional estimate
/// from the ascender (the descriptor entry is required and advisory).
fn cap_height(face: &Face<'_>, ascent: i16) -> f32 {
    face.table(b"OS/2")
        .filter(|os2| os2.len() >= 90 && u16::from_be_bytes([os2[0], os2[1]]) >= 2)
        .map(|os2| f32::from(i16::from_be_bytes([os2[88], os2[89]])))
        .filter(|height| *height > 0.0)
        .unwrap_or_else(|| f32::from(ascent) * 0.7)
}

/// The `/W` array body: runs of consecutive glyph ids with their advances, in
/// thousandths of an em.
fn widths(face: &Face<'_>, kept: &BTreeSet<u16>, scale: f32) -> String {
    let mut out = String::new();
    let mut run: Vec<String> = Vec::new();
    let mut start: Option<u16> = None;
    let mut previous: Option<u16> = None;
    let flush = |start: Option<u16>, run: &mut Vec<String>, out: &mut String| {
        if let Some(first) = start
            && !run.is_empty()
        {
            out.push_str(&format!("{first}[{}]", run.join(" ")));
            run.clear();
        }
    };
    for glyph in kept {
        if previous.is_some_and(|last| last + 1 != *glyph) {
            flush(start, &mut run, &mut out);
            start = None;
        }
        if start.is_none() {
            start = Some(*glyph);
        }
        run.push(num(f32::from(face.advance(*glyph).unwrap_or(0)) * scale));
        previous = Some(*glyph);
    }
    flush(start, &mut run, &mut out);
    out
}

/// A deterministic six-uppercase-letter subset tag.
///
/// ISO 32000-1 requires the tag to distinguish subsets of the same face, and
/// consumers key their font cache on it, so it is derived from the face name
/// **and** the retained glyph set: two different subsets of one face never
/// collide, and the same subset always produces the same tag (the export stays
/// byte-reproducible).
fn subset_tag(postscript: &str, kept: &BTreeSet<u16>) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in postscript.as_bytes() {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    for glyph in kept {
        for byte in glyph.to_be_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    let mut tag = String::with_capacity(6);
    for _ in 0..6 {
        tag.push(char::from(b'A' + u8::try_from(hash % 26).unwrap_or(0)));
        hash /= 26;
    }
    tag
}

/// The face's PostScript name from the `name` table (name id 6), falling back
/// to a stable placeholder when the face omits it.
fn postscript_name(face: &Face<'_>) -> String {
    let Some(table) = face.table(b"name") else {
        return "OpenDocFace".to_owned();
    };
    let read_u16 = |offset: usize| -> Option<u16> {
        table
            .get(offset..offset + 2)
            .map(|slice| u16::from_be_bytes([slice[0], slice[1]]))
    };
    let Some(count) = read_u16(2) else {
        return "OpenDocFace".to_owned();
    };
    let Some(storage) = read_u16(4).map(usize::from) else {
        return "OpenDocFace".to_owned();
    };
    let mut best: Option<String> = None;
    for record in 0..usize::from(count) {
        let base = 6 + record * 12;
        let (Some(platform), Some(name_id), Some(length), Some(offset)) = (
            read_u16(base),
            read_u16(base + 6),
            read_u16(base + 8).map(usize::from),
            read_u16(base + 10).map(usize::from),
        ) else {
            break;
        };
        if name_id != 6 {
            continue;
        }
        let Some(raw) = table.get(storage + offset..storage + offset + length) else {
            continue;
        };
        let decoded = if platform == 3 {
            raw.chunks_exact(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .filter_map(|unit| char::from_u32(u32::from(unit)))
                .collect::<String>()
        } else {
            raw.iter().map(|byte| char::from(*byte)).collect::<String>()
        };
        let cleaned: String = decoded
            .chars()
            .filter(|ch| {
                ch.is_ascii_graphic() && !matches!(ch, '(' | ')' | '/' | '<' | '>' | '[' | ']')
            })
            .collect();
        if !cleaned.is_empty() && (best.is_none() || platform == 3) {
            best = Some(cleaned);
        }
    }
    best.unwrap_or_else(|| "OpenDocFace".to_owned())
}

/// Builds the `ToUnicode` CMap that makes the text layer selectable,
/// searchable and copyable, and reports how many retained glyphs it could not
/// name.
///
/// The display list carries glyph ids and cluster offsets but not the source
/// text, so the mapping is recovered by **inverting the face's own character
/// map**. That is exact for the overwhelming majority of text. Where one glyph
/// stands for several characters (a ligature) or for none (a glyph reached only
/// by substitution), the inversion cannot name it; those glyphs are counted and
/// reported rather than guessed at. Carrying cluster text through the display
/// list would make the mapping exact and is recorded as remaining work in
/// `docs/98`.
fn to_unicode_cmap(source: &[u8], index: u32, kept: &BTreeSet<u16>) -> (String, usize) {
    let mut mapping: BTreeMap<u16, u32> = BTreeMap::new();
    if let Ok(font) = FontRef::from_index(source, index) {
        for (code, glyph) in font.charmap().mappings() {
            if let Ok(id) = u16::try_from(glyph.to_u32())
                && kept.contains(&id)
            {
                // The lowest code point wins, so the map is independent of the
                // order the `cmap` happens to list its subtables in.
                mapping.entry(id).or_insert(code);
            }
        }
    }

    let mut body = String::new();
    let mut count = 0_usize;
    let mut chunk = String::new();
    for (glyph, code) in &mapping {
        let Some(ch) = char::from_u32(*code) else {
            continue;
        };
        let mut utf16 = String::new();
        let mut buffer = [0_u16; 2];
        for unit in ch.encode_utf16(&mut buffer) {
            utf16.push_str(&format!("{unit:04X}"));
        }
        chunk.push_str(&format!("<{glyph:04X}> <{utf16}>\n"));
        count += 1;
        // A `bfchar` section may hold at most 100 entries.
        if count.is_multiple_of(100) {
            body.push_str(&format!("100 beginbfchar\n{chunk}endbfchar\n"));
            chunk.clear();
        }
    }
    let tail = count % 100;
    if tail != 0 {
        body.push_str(&format!("{tail} beginbfchar\n{chunk}endbfchar\n"));
    }

    let cmap = format!(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CIDSystemInfo <</Registry (Adobe) /Ordering (UCS) /Supplement 0>> def\n\
         /CMapName /Adobe-Identity-UCS def\n\
         /CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
         {body}endcmap\n\
         CMapName currentdict /CMap defineresource pop\n\
         end\nend\n"
    );
    // `.notdef` is never copied out, so it is not counted as an unmapped glyph.
    let unmapped = kept.len().saturating_sub(mapping.len()).saturating_sub(1);
    (cmap, unmapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_layout::fonts;

    #[test]
    fn a_subset_tag_is_six_uppercase_letters_and_tracks_the_glyph_set() {
        let one: BTreeSet<u16> = [1, 2, 3].into_iter().collect();
        let two: BTreeSet<u16> = [1, 2, 4].into_iter().collect();
        let tag = subset_tag("Roboto-Regular", &one);
        assert_eq!(tag.len(), 6);
        assert!(tag.chars().all(|ch| ch.is_ascii_uppercase()), "{tag}");
        assert_eq!(tag, subset_tag("Roboto-Regular", &one), "deterministic");
        assert_ne!(tag, subset_tag("Roboto-Regular", &two));
        assert_ne!(tag, subset_tag("Roboto-Bold", &one));
    }

    #[test]
    fn the_postscript_name_comes_from_the_face() {
        let bytes = fonts::face_bytes(FontId(0));
        let face = Face::parse(bytes, 0).expect("parse bundled face");
        assert!(
            postscript_name(&face).starts_with("Roboto"),
            "{}",
            postscript_name(&face)
        );
    }

    #[test]
    fn the_to_unicode_map_names_the_glyphs_a_run_actually_uses() {
        let bytes = fonts::face_bytes(FontId(0));
        let font = FontRef::new(bytes).expect("skrifa reads the bundled face");
        let charmap = font.charmap();
        let glyph = u16::try_from(charmap.map('A').expect("A is covered").to_u32()).unwrap();
        let kept: BTreeSet<u16> = [0, glyph].into_iter().collect();
        let (cmap, unmapped) = to_unicode_cmap(bytes, 0, &kept);
        assert!(cmap.contains(&format!("<{glyph:04X}> <0041>")), "{cmap}");
        assert_eq!(unmapped, 0);
    }
}
