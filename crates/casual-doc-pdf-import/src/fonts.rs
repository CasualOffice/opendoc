//! Embedded PDF font programs the model cannot register as they are, made
//! into OpenType so they can (`docs/182` §5.4, closing the P1 gap of §16.4).
//!
//! - A **bare CFF** program (`FontFile3` / `Type1C`) is a complete CFF font
//!   with no sfnt around it. It is *wrapped*, not converted: its bytes become
//!   the `CFF ` table of an `OTTO` font whose other required tables (`head`,
//!   `hhea`, `maxp` 0.5, `OS/2`, `name`, `cmap`, `post` 3.0, `hmtx`) are built
//!   from the program itself. This is the container change the OpenType
//!   specification defines for CFF outlines, so outlines and hints are the
//!   producer's, byte for byte.
//! - A **Type1** program (`FontFile`, PostScript) is *converted*: read-fonts'
//!   Type1 interpreter (MIT/Apache, already in the tree under skrifa) evaluates
//!   each charstring, flattening `seac` accents and subroutines, and this
//!   module re-encodes the outline as an unhinted Type2 charstring in a CFF it
//!   writes. Outlines are exact (the same points, in font units); hints are
//!   dropped, which matters only for hinted rasterisation at small sizes.
//!
//! Both are **name-keyed**: the Unicode `cmap` is built from glyph names
//! through the Adobe Glyph List (`read_fonts::ps::charmap`), which is how a
//! PDF's simple fonts name their glyphs. A CID-keyed CFF has no names; its
//! code-to-glyph mapping lives in the PDF's CMap, which the evidence does not
//! carry, so it is refused here and stays substituted ([`WrapRefusal::CidKeyed`]).
//!
//! Correctness gate: a wrapped program is registered only if (a) its `cmap`
//! maps every character the PDF drew with it (the caller's existing coverage
//! test) and (b) its advance for each drawn character agrees with the advance
//! the PDF placed that character with ([`advances_agree`]), so a glyph name
//! that lies about its character cannot draw the wrong letter silently.
//!
//! Complexity: O(glyphs + charstring bytes) per program.

use std::collections::BTreeMap;

use skrifa::raw::ps::cff::CffFontRef;
use skrifa::raw::ps::charmap::Charmap;
use skrifa::raw::ps::cs::CommandSink;
use skrifa::raw::ps::string::Sid;
use skrifa::raw::ps::type1::Type1Font;
use skrifa::raw::types::{Fixed, GlyphId};

/// Glyphs a wrapped program may hold (the sfnt limit).
pub const MAX_GLYPHS: u32 = 65_535;
/// The fraction of drawn characters whose advance may disagree with the
/// program's before the name mapping is distrusted.
pub const ADVANCE_DISAGREEMENT: f64 = 0.1;
/// Relative tolerance of one advance comparison.
pub const ADVANCE_TOLERANCE: f64 = 0.05;

/// Why a program was not made into OpenType.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WrapRefusal {
    /// The bytes are not the program kind claimed.
    Unreadable,
    /// A CID-keyed CFF: no glyph names, so no `cmap` without the PDF's CMap.
    CidKeyed,
    /// No glyph name maps to a Unicode character.
    NoNames,
    /// More glyphs than an sfnt can index.
    TooManyGlyphs,
    /// A font matrix this writer does not reproduce (skewed, or not 1/upem).
    Matrix,
}

/// One glyph's metrics in font units.
#[derive(Clone, Copy, Debug, Default)]
struct GlyphMetric {
    advance: u16,
    x_min: i16,
    y_min: i16,
    x_max: i16,
    y_max: i16,
}

/// What the sfnt tables need besides the glyph data.
struct FaceInfo {
    family: String,
    postscript: String,
    bold: bool,
    italic: bool,
    upem: u16,
    italic_angle: Fixed,
    fixed_pitch: bool,
    underline_position: i16,
    underline_thickness: i16,
}

/// Collects a charstring's outline: absolute points in font units, plus the
/// control-point bounds.
#[derive(Default)]
struct Outline {
    commands: Vec<Command>,
    bounds: Option<(f64, f64, f64, f64)>,
}

#[derive(Clone, Copy)]
pub(crate) enum Command {
    Move(Fixed, Fixed),
    Line(Fixed, Fixed),
    Curve(Fixed, Fixed, Fixed, Fixed, Fixed, Fixed),
}

impl Outline {
    fn grow(&mut self, x: Fixed, y: Fixed) {
        let (x, y) = (f64::from(x.to_f32()), f64::from(y.to_f32()));
        self.bounds = Some(match self.bounds {
            None => (x, y, x, y),
            Some((a, b, c, d)) => (a.min(x), b.min(y), c.max(x), d.max(y)),
        });
    }

    fn metric(&self, advance: Option<Fixed>) -> GlyphMetric {
        let clamp = |v: f64| v.round().clamp(-32_768.0, 32_767.0) as i16;
        let (x0, y0, x1, y1) = self.bounds.unwrap_or_default();
        GlyphMetric {
            advance: advance.map_or(0, |w| {
                f64::from(w.to_f32()).round().clamp(0.0, 65_535.0) as u16
            }),
            x_min: clamp(x0),
            y_min: clamp(y0),
            x_max: clamp(x1),
            y_max: clamp(y1),
        }
    }
}

impl CommandSink for Outline {
    fn move_to(&mut self, x: Fixed, y: Fixed) {
        self.grow(x, y);
        self.commands.push(Command::Move(x, y));
    }
    fn line_to(&mut self, x: Fixed, y: Fixed) {
        self.grow(x, y);
        self.commands.push(Command::Line(x, y));
    }
    fn curve_to(&mut self, cx0: Fixed, cy0: Fixed, cx1: Fixed, cy1: Fixed, x: Fixed, y: Fixed) {
        self.grow(cx0, cy0);
        self.grow(cx1, cy1);
        self.grow(x, y);
        self.commands.push(Command::Curve(cx0, cy0, cx1, cy1, x, y));
    }
    fn close(&mut self) {}
}

/// The Unicode `cmap` from glyph names, variants (`a.alt`) left out. Sorted by
/// code point.
fn unicode_map<'a>(names: impl Iterator<Item = (GlyphId, &'a str)>) -> Vec<(u32, u16)> {
    let charmap = Charmap::from_glyph_names(names);
    let mut out: Vec<(u32, u16)> = charmap
        .iter()
        .filter(|(code, _)| *code < 0x11_0000)
        .filter_map(|(code, gid)| u16::try_from(gid.to_u32()).ok().map(|g| (code, g)))
        .filter(|(_, gid)| *gid != 0)
        .collect();
    out.sort_unstable();
    out.dedup_by_key(|(code, _)| *code);
    out
}

/// Wraps a bare, name-keyed CFF program as an `OTTO` font. O(glyphs).
///
/// # Errors
///
/// A [`WrapRefusal`] when the program cannot be wrapped correctly.
pub fn wrap_bare_cff(
    cff: &[u8],
    family: &str,
    bold: bool,
    italic: bool,
) -> Result<Vec<u8>, WrapRefusal> {
    let font = CffFontRef::new_cff(cff, 0, None).map_err(|_| WrapRefusal::Unreadable)?;
    if font.is_cid() {
        return Err(WrapRefusal::CidKeyed);
    }
    let glyphs = font.num_glyphs();
    if glyphs == 0 {
        return Err(WrapRefusal::Unreadable);
    }
    if glyphs > MAX_GLYPHS {
        return Err(WrapRefusal::TooManyGlyphs);
    }
    let upem = match font.matrix() {
        None => 1000,
        Some(m) => u16::try_from(m.scale)
            .ok()
            .filter(|s| (16..=16_384).contains(s))
            .ok_or(WrapRefusal::Matrix)?,
    };
    let charset = font.charset().ok_or(WrapRefusal::Unreadable)?;
    let mut names: Vec<(GlyphId, String)> = Vec::with_capacity(glyphs as usize);
    for gid in 0..glyphs {
        let gid = GlyphId::new(gid);
        if let Ok(sid) = charset.string_id(gid)
            && let Some(bytes) = font.string(sid)
        {
            names.push((gid, String::from_utf8_lossy(bytes).into_owned()));
        }
    }
    let cmap = unicode_map(names.iter().map(|(g, n)| (*g, n.as_str())));
    if cmap.is_empty() {
        return Err(WrapRefusal::NoNames);
    }
    let subfont = font.subfont(0, &[]).map_err(|_| WrapRefusal::Unreadable)?;
    let mut metrics = Vec::with_capacity(glyphs as usize);
    for gid in 0..glyphs {
        let mut outline = Outline::default();
        let advance = font
            .evaluate_charstring(&subfont, GlyphId::new(gid), &[], &mut outline)
            .map_err(|_| WrapRefusal::Unreadable)?;
        metrics.push(outline.metric(advance));
    }
    let metadata = font.metadata().unwrap_or_default();
    let postscript = font_name(cff).unwrap_or_else(|| family.replace(' ', ""));
    let info = FaceInfo {
        family: family.to_owned(),
        postscript,
        bold,
        italic,
        upem,
        italic_angle: metadata.italic_angle(),
        fixed_pitch: metadata.is_fixed_pitch(),
        underline_position: metadata
            .underline_position()
            .to_i32()
            .clamp(-32_768, 32_767) as i16,
        underline_thickness: metadata.underline_thickness().to_i32().clamp(0, 32_767) as i16,
    };
    Ok(sfnt(&info, &metrics, &cmap, cff))
}

/// The first name in a CFF's Name INDEX. O(1).
fn font_name(cff: &[u8]) -> Option<String> {
    use skrifa::raw::FontRead;
    let table = skrifa::raw::tables::cff::Cff::read(cff.into()).ok()?;
    let name = table.names().get(0).ok()?;
    let name = String::from_utf8_lossy(name).into_owned();
    (!name.is_empty()).then_some(name)
}

/// Converts a Type1 program to an `OTTO` font with unhinted Type2 outlines.
/// O(glyphs + charstring bytes).
///
/// # Errors
///
/// A [`WrapRefusal`] when the program cannot be converted correctly.
pub fn convert_type1(
    program: &[u8],
    family: &str,
    bold: bool,
    italic: bool,
) -> Result<Vec<u8>, WrapRefusal> {
    let font = Type1Font::new(program).map_err(|_| WrapRefusal::Unreadable)?;
    let glyphs = font.num_glyphs();
    if glyphs == 0 {
        return Err(WrapRefusal::Unreadable);
    }
    if glyphs > MAX_GLYPHS {
        return Err(WrapRefusal::TooManyGlyphs);
    }
    // The CFF written below states no FontMatrix, i.e. the default
    // [0.001 0 0 0.001 0 0]; any other matrix would change every outline.
    let matrix = font.matrix();
    // read-fonts stores the matrix multiplied by its scale (`upem`), so the
    // default matrix reads as the identity with a scale of 1000.
    let unit = Fixed::ONE;
    let near = |a: Fixed, b: Fixed| (a.to_f64() - b.to_f64()).abs() < 1e-6;
    if font.upem() != 1000
        || !(near(matrix.xx, unit)
            && near(matrix.yy, unit)
            && near(matrix.xy, Fixed::ZERO)
            && near(matrix.yx, Fixed::ZERO)
            && near(matrix.dx, Fixed::ZERO)
            && near(matrix.dy, Fixed::ZERO))
    {
        return Err(WrapRefusal::Matrix);
    }
    let names: Vec<(GlyphId, String)> = font
        .glyph_names()
        .map(|(gid, name)| (gid, name.to_owned()))
        .collect();
    if names.len() != glyphs as usize || names.first().map(|(_, n)| n.as_str()) != Some(".notdef") {
        return Err(WrapRefusal::Unreadable);
    }
    let cmap = unicode_map(names.iter().map(|(g, n)| (*g, n.as_str())));
    if cmap.is_empty() {
        return Err(WrapRefusal::NoNames);
    }
    let mut metrics = Vec::with_capacity(glyphs as usize);
    let mut charstrings = Vec::with_capacity(glyphs as usize);
    for gid in 0..glyphs {
        let mut outline = Outline::default();
        let advance = font
            .evaluate_charstring(GlyphId::new(gid), &mut outline)
            .map_err(|_| WrapRefusal::Unreadable)?;
        let metric = outline.metric(advance);
        charstrings.push(type2_charstring(&outline.commands, metric.advance));
        metrics.push(metric);
    }
    let postscript = font
        .name()
        .map(str::to_owned)
        .unwrap_or_else(|| family.replace(' ', ""));
    let bbox = font.bbox();
    let cff = write_cff(
        &postscript,
        &names[1..]
            .iter()
            .map(|(_, n)| n.as_str())
            .collect::<Vec<_>>(),
        &charstrings,
        [
            bbox.x_min.to_i32(),
            bbox.y_min.to_i32(),
            bbox.x_max.to_i32(),
            bbox.y_max.to_i32(),
        ],
    );
    let info = FaceInfo {
        family: family.to_owned(),
        postscript,
        bold,
        italic,
        upem: 1000,
        italic_angle: Fixed::from_i32(font.italic_angle()),
        fixed_pitch: font.is_fixed_pitch(),
        underline_position: font.underline_position().clamp(-32_768, 32_767) as i16,
        underline_thickness: font.underline_thickness().clamp(0, 32_767) as i16,
    };
    Ok(sfnt(&info, &metrics, &cmap, &cff))
}

/// Whether `bytes` look like a Type1 program (`%!PS-AdobeFont`/`%!FontType`,
/// or a PFB segment header). O(1).
#[must_use]
pub fn is_type1(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%!PS-AdobeFont")
        || bytes.starts_with(b"%!FontType")
        || bytes.starts_with(&[0x80, 0x01])
}

/// Whether a program's advances agree with the advances the PDF drew its
/// characters at: `observed` maps a character to its advance in em, `program`
/// is an sfnt. True when at most [`ADVANCE_DISAGREEMENT`] of the characters
/// differ by more than [`ADVANCE_TOLERANCE`]. O(characters).
#[must_use]
pub fn advances_agree(program: &[u8], observed: &BTreeMap<char, f64>) -> bool {
    use skrifa::MetadataProvider;
    let Ok(face) = skrifa::FontRef::new(program) else {
        return false;
    };
    let charmap = face.charmap();
    let size = skrifa::instance::Size::unscaled();
    let upem = f64::from(
        face.metrics(size, skrifa::instance::LocationRef::default())
            .units_per_em,
    );
    let metrics = face.glyph_metrics(size, skrifa::instance::LocationRef::default());
    if observed.is_empty() || upem <= 0.0 {
        return true;
    }
    let mut disagree = 0usize;
    for (c, em) in observed {
        let advance = charmap
            .map(*c)
            .and_then(|gid| metrics.advance_width(gid))
            .map(|units| f64::from(units) / upem);
        let ok = advance.is_some_and(|a| (a - em).abs() <= ADVANCE_TOLERANCE * em.max(0.1));
        if !ok {
            disagree += 1;
        }
    }
    (disagree as f64) <= ADVANCE_DISAGREEMENT * observed.len() as f64
}

// --- Type2 charstrings and the CFF writer -----------------------------------

/// Appends a CFF Type2 charstring operand.
fn push_operand(out: &mut Vec<u8>, value: Fixed) {
    let bits = value.to_bits();
    if bits & 0xFFFF == 0 {
        let v = bits >> 16;
        match v {
            -107..=107 => out.push((v + 139) as u8),
            108..=1131 => {
                let v = v - 108;
                out.push((v / 256 + 247) as u8);
                out.push((v % 256) as u8);
            }
            -1131..=-108 => {
                let v = -v - 108;
                out.push((v / 256 + 251) as u8);
                out.push((v % 256) as u8);
            }
            _ => {
                out.push(28);
                out.extend_from_slice(&(v as i16).to_be_bytes());
            }
        }
    } else {
        out.push(255);
        out.extend_from_slice(&bits.to_be_bytes());
    }
}

/// An unhinted Type2 charstring for an absolute outline, the width given
/// explicitly against a nominal width of 0. O(commands).
fn type2_charstring(commands: &[Command], advance: u16) -> Vec<u8> {
    let mut out = Vec::new();
    push_operand(&mut out, Fixed::from_i32(i32::from(advance)));
    let (mut x, mut y) = (Fixed::ZERO, Fixed::ZERO);
    let mut first = true;
    for command in commands {
        match *command {
            Command::Move(px, py) => {
                if !first {
                    // Type2 has no closepath; a moveto closes the subpath.
                }
                push_operand(&mut out, px - x);
                push_operand(&mut out, py - y);
                out.push(21);
                (x, y) = (px, py);
            }
            Command::Line(px, py) => {
                push_operand(&mut out, px - x);
                push_operand(&mut out, py - y);
                out.push(5);
                (x, y) = (px, py);
            }
            Command::Curve(ax, ay, bx, by, px, py) => {
                push_operand(&mut out, ax - x);
                push_operand(&mut out, ay - y);
                push_operand(&mut out, bx - ax);
                push_operand(&mut out, by - ay);
                push_operand(&mut out, px - bx);
                push_operand(&mut out, py - by);
                out.push(8);
                (x, y) = (px, py);
            }
        }
        first = false;
    }
    out.push(14);
    out
}

/// A CFF INDEX of `items` (offSize 4: simple and always wide enough).
fn cff_index(items: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(items.len() as u16).to_be_bytes());
    if items.is_empty() {
        return out;
    }
    out.push(4);
    let mut offset: u32 = 1;
    out.extend_from_slice(&offset.to_be_bytes());
    for item in items {
        offset += item.len() as u32;
        out.extend_from_slice(&offset.to_be_bytes());
    }
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

/// A DICT integer operand in the fixed five-byte form, so offsets can be
/// written before they are known.
fn dict_int5(out: &mut Vec<u8>, value: i32) {
    out.push(29);
    out.extend_from_slice(&value.to_be_bytes());
}

/// Writes a name-keyed CFF: one font, the given glyph names for glyphs 1..n
/// (glyph 0 is `.notdef`), charstrings for 0..n, and an empty Private DICT
/// apart from zero default and nominal widths. O(bytes).
fn write_cff(name: &str, glyph_names: &[&str], charstrings: &[Vec<u8>], bbox: [i32; 4]) -> Vec<u8> {
    let mut standard: BTreeMap<&'static [u8], u16> = BTreeMap::new();
    for sid in 0..391u16 {
        if let Ok(s) = Sid::new(sid).resolve_standard() {
            standard.entry(s).or_insert(sid);
        }
    }
    let mut strings: Vec<&[u8]> = Vec::new();
    let mut charset = vec![0u8];
    for glyph in glyph_names {
        let sid = match standard.get(glyph.as_bytes()) {
            Some(sid) => *sid,
            None => {
                strings.push(glyph.as_bytes());
                391 + (strings.len() as u16 - 1)
            }
        };
        charset.extend_from_slice(&sid.to_be_bytes());
    }
    let private = vec![139, 20, 139, 21];
    let charstring_refs: Vec<&[u8]> = charstrings.iter().map(Vec::as_slice).collect();
    let charstrings = cff_index(&charstring_refs);
    let names = cff_index(&[name.as_bytes()]);
    let string_index = cff_index(&strings);
    let global_subrs = cff_index(&[]);
    // Top DICT: FontBBox, charset, CharStrings, Private. The offsets are
    // five-byte ints, so its size is known before they are.
    let top_dict = |charset_at: i32, charstrings_at: i32, private_at: i32| {
        let mut d = Vec::new();
        for v in bbox {
            dict_int5(&mut d, v);
        }
        d.push(5);
        dict_int5(&mut d, charset_at);
        d.push(15);
        dict_int5(&mut d, charstrings_at);
        d.push(17);
        dict_int5(&mut d, private.len() as i32);
        dict_int5(&mut d, private_at);
        d.push(18);
        d
    };
    let top_len = cff_index(&[&top_dict(0, 0, 0)]).len();
    let header = [1u8, 0, 4, 4];
    let charset_at = header.len() + names.len() + top_len + string_index.len() + global_subrs.len();
    let charstrings_at = charset_at + charset.len();
    let private_at = charstrings_at + charstrings.len();
    let top = top_dict(charset_at as i32, charstrings_at as i32, private_at as i32);
    let mut out = header.to_vec();
    out.extend_from_slice(&names);
    out.extend_from_slice(&cff_index(&[&top]));
    out.extend_from_slice(&string_index);
    out.extend_from_slice(&global_subrs);
    out.extend_from_slice(&charset);
    out.extend_from_slice(&charstrings);
    out.extend_from_slice(&private);
    out
}

// --- The sfnt assembler -----------------------------------------------------

fn be16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}
fn bei16(out: &mut Vec<u8>, v: i16) {
    out.extend_from_slice(&v.to_be_bytes());
}
fn be32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn checksum(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for chunk in data.chunks(4) {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum = sum.wrapping_add(u32::from_be_bytes(word));
    }
    sum
}

/// `cmap` with a format 4 (BMP) and, when needed, a format 12 subtable, both
/// for the Windows Unicode platform. O(mappings).
fn cmap_table(map: &[(u32, u16)]) -> Vec<u8> {
    // Format 4 segments: runs of consecutive codes with consecutive glyphs.
    let bmp: Vec<(u32, u16)> = map.iter().copied().filter(|(c, _)| *c < 0xFFFF).collect();
    let mut segments: Vec<(u16, u16, i16)> = Vec::new();
    for (code, gid) in &bmp {
        let code = *code as u16;
        let delta = gid.wrapping_sub(code) as i16;
        match segments.last_mut() {
            Some((_, end, d)) if *end + 1 == code && *d == delta => *end = code,
            _ => segments.push((code, code, delta)),
        }
    }
    segments.push((0xFFFF, 0xFFFF, 1));
    let seg_count = segments.len() as u16;
    let mut f4 = Vec::new();
    let (mut power, mut entry) = (1u16, 0u16);
    while power * 2 <= seg_count {
        power *= 2;
        entry += 1;
    }
    let search = 2 * power;
    be16(&mut f4, 4);
    be16(&mut f4, 0); // length, patched
    be16(&mut f4, 0);
    be16(&mut f4, seg_count * 2);
    be16(&mut f4, search);
    be16(&mut f4, entry);
    be16(&mut f4, seg_count * 2 - search);
    for (_, end, _) in &segments {
        be16(&mut f4, *end);
    }
    be16(&mut f4, 0);
    for (start, _, _) in &segments {
        be16(&mut f4, *start);
    }
    for (_, _, delta) in &segments {
        bei16(&mut f4, *delta);
    }
    for _ in &segments {
        be16(&mut f4, 0);
    }
    let len = f4.len() as u16;
    f4[2..4].copy_from_slice(&len.to_be_bytes());

    let wide: Vec<(u32, u16)> = map.to_vec();
    let needs_12 = map.iter().any(|(c, _)| *c > 0xFFFF);
    let mut f12 = Vec::new();
    if needs_12 {
        let mut groups: Vec<(u32, u32, u32)> = Vec::new();
        for (code, gid) in &wide {
            match groups.last_mut() {
                Some((start, end, first))
                    if *end + 1 == *code && *first + (*code - *start) == u32::from(*gid) =>
                {
                    *end = *code;
                }
                _ => groups.push((*code, *code, u32::from(*gid))),
            }
        }
        be16(&mut f12, 12);
        be16(&mut f12, 0);
        be32(&mut f12, 16 + 12 * groups.len() as u32);
        be32(&mut f12, 0);
        be32(&mut f12, groups.len() as u32);
        for (start, end, gid) in groups {
            be32(&mut f12, start);
            be32(&mut f12, end);
            be32(&mut f12, gid);
        }
    }
    let tables = if needs_12 { 2 } else { 1 };
    let mut out = Vec::new();
    be16(&mut out, 0);
    be16(&mut out, tables);
    let first = 4 + 8 * u32::from(tables);
    be16(&mut out, 3);
    be16(&mut out, 1);
    be32(&mut out, first);
    if needs_12 {
        be16(&mut out, 3);
        be16(&mut out, 10);
        be32(&mut out, first + f4.len() as u32);
    }
    out.extend_from_slice(&f4);
    out.extend_from_slice(&f12);
    out
}

fn name_table(info: &FaceInfo) -> Vec<u8> {
    let subfamily = match (info.bold, info.italic) {
        (true, true) => "Bold Italic",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (false, false) => "Regular",
    };
    let full = format!("{} {subfamily}", info.family);
    let records: [(u16, String); 5] = [
        (1, info.family.clone()),
        (2, subfamily.to_owned()),
        (3, format!("opendoc-pdf:{}", info.postscript)),
        (4, full),
        (
            6,
            info.postscript
                .chars()
                .filter(|c| c.is_ascii_graphic() && !"[](){}<>/%".contains(*c))
                .take(63)
                .collect(),
        ),
    ];
    let encoded: Vec<Vec<u8>> = records
        .iter()
        .map(|(_, s)| s.encode_utf16().flat_map(u16::to_be_bytes).collect())
        .collect();
    let mut out = Vec::new();
    be16(&mut out, 0);
    be16(&mut out, records.len() as u16);
    be16(&mut out, 6 + 12 * records.len() as u16);
    let mut offset = 0u16;
    for ((id, _), bytes) in records.iter().zip(&encoded) {
        be16(&mut out, 3);
        be16(&mut out, 1);
        be16(&mut out, 0x0409);
        be16(&mut out, *id);
        be16(&mut out, bytes.len() as u16);
        be16(&mut out, offset);
        offset += bytes.len() as u16;
    }
    for bytes in encoded {
        out.extend_from_slice(&bytes);
    }
    out
}

/// Assembles an `OTTO` sfnt around `cff`. Deterministic: no timestamps.
/// O(glyphs + bytes).
fn sfnt(info: &FaceInfo, metrics: &[GlyphMetric], cmap: &[(u32, u16)], cff: &[u8]) -> Vec<u8> {
    let inked: Vec<&GlyphMetric> = metrics
        .iter()
        .filter(|m| m.x_max > m.x_min || m.y_max > m.y_min)
        .collect();
    let x_min = inked.iter().map(|m| m.x_min).min().unwrap_or(0);
    let y_min = inked.iter().map(|m| m.y_min).min().unwrap_or(0);
    let x_max = inked.iter().map(|m| m.x_max).max().unwrap_or(0);
    let y_max = inked.iter().map(|m| m.y_max).max().unwrap_or(0);
    let upem = i32::from(info.upem);
    let ascent = (i32::from(y_max)).max(upem * 4 / 5).min(32_767) as i16;
    let descent = (i32::from(y_min)).min(-(upem / 5)).max(-32_768) as i16;
    let advance_max = metrics.iter().map(|m| m.advance).max().unwrap_or(0);
    let glyphs = metrics.len() as u16;

    let mut head = Vec::new();
    be32(&mut head, 0x0001_0000);
    be32(&mut head, 0x0001_0000);
    be32(&mut head, 0); // checkSumAdjustment, patched
    be32(&mut head, 0x5F0F_3CF5);
    be16(&mut head, 0x000B);
    be16(&mut head, info.upem);
    head.extend_from_slice(&[0; 16]); // created, modified
    bei16(&mut head, x_min);
    bei16(&mut head, y_min);
    bei16(&mut head, x_max);
    bei16(&mut head, y_max);
    be16(
        &mut head,
        u16::from(info.bold) | (u16::from(info.italic) << 1),
    );
    be16(&mut head, 8);
    bei16(&mut head, 2);
    bei16(&mut head, 0);
    bei16(&mut head, 0);

    let mut hhea = Vec::new();
    be32(&mut hhea, 0x0001_0000);
    bei16(&mut hhea, ascent);
    bei16(&mut hhea, descent);
    bei16(&mut hhea, 0);
    be16(&mut hhea, advance_max);
    bei16(
        &mut hhea,
        metrics.iter().map(|m| m.x_min).min().unwrap_or(0),
    );
    bei16(
        &mut hhea,
        metrics
            .iter()
            .map(|m| (i32::from(m.advance) - i32::from(m.x_max)).clamp(-32_768, 32_767) as i16)
            .min()
            .unwrap_or(0),
    );
    bei16(&mut hhea, x_max);
    bei16(&mut hhea, 1);
    bei16(&mut hhea, 0);
    bei16(&mut hhea, 0);
    hhea.extend_from_slice(&[0; 8]);
    bei16(&mut hhea, 0);
    be16(&mut hhea, glyphs);

    let mut maxp = Vec::new();
    be32(&mut maxp, 0x0000_5000);
    be16(&mut maxp, glyphs);

    let first_char = cmap.first().map_or(0, |(c, _)| (*c).min(0xFFFF) as u16);
    let last_char = cmap.last().map_or(0, |(c, _)| (*c).min(0xFFFF) as u16);
    let mut os2 = Vec::new();
    be16(&mut os2, 4);
    let avg = if inked.is_empty() {
        0
    } else {
        (inked.iter().map(|m| u32::from(m.advance)).sum::<u32>() / inked.len() as u32) as i16
    };
    bei16(&mut os2, avg);
    be16(&mut os2, if info.bold { 700 } else { 400 });
    be16(&mut os2, 5);
    be16(&mut os2, 0x0008); // fsType: editable embedding (the session edits with it)
    let em = info.upem as i16;
    for v in [
        em * 13 / 20,
        em * 7 / 10,
        0,
        em * 7 / 50,
        em * 13 / 20,
        em * 7 / 10,
        0,
        em * 12 / 25,
    ] {
        bei16(&mut os2, v); // sub/superscript size and offset
    }
    bei16(&mut os2, em / 20); // strikeout size
    bei16(&mut os2, em / 4); // strikeout position
    bei16(&mut os2, 0); // family class
    os2.extend_from_slice(&[0; 10]); // panose
    os2.extend_from_slice(&[0; 16]); // unicode ranges
    os2.extend_from_slice(b"PDFI");
    let selection = if info.bold || info.italic {
        u16::from(info.italic) | (u16::from(info.bold) << 5)
    } else {
        1 << 6
    } | (1 << 7);
    be16(&mut os2, selection);
    be16(&mut os2, first_char);
    be16(&mut os2, last_char);
    bei16(&mut os2, ascent);
    bei16(&mut os2, descent);
    bei16(&mut os2, 0);
    be16(&mut os2, ascent.max(0) as u16);
    be16(&mut os2, (-i32::from(descent)).max(0) as u16);
    be32(&mut os2, 1); // Latin 1 code page
    be32(&mut os2, 0);
    bei16(&mut os2, em / 2); // x-height (unknown; nominal)
    bei16(&mut os2, em * 7 / 10); // cap height (unknown; nominal)
    be16(&mut os2, 0);
    be16(&mut os2, 32);
    be16(&mut os2, 1);

    let mut post = Vec::new();
    be32(&mut post, 0x0003_0000);
    be32(&mut post, info.italic_angle.to_bits() as u32);
    bei16(&mut post, info.underline_position);
    bei16(&mut post, info.underline_thickness);
    be32(&mut post, u32::from(info.fixed_pitch));
    post.extend_from_slice(&[0; 16]);

    let mut hmtx = Vec::new();
    for m in metrics {
        be16(&mut hmtx, m.advance);
        bei16(&mut hmtx, m.x_min);
    }

    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"CFF ", cff.to_vec()),
        (*b"OS/2", os2),
        (*b"cmap", cmap_table(cmap)),
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"hmtx", hmtx),
        (*b"maxp", maxp),
        (*b"name", name_table(info)),
        (*b"post", post),
    ];
    tables.sort_by_key(|(tag, _)| *tag);
    let count = tables.len() as u16;
    let mut power = 1u16;
    let mut selector = 0u16;
    while power * 2 <= count {
        power *= 2;
        selector += 1;
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"OTTO");
    be16(&mut out, count);
    be16(&mut out, power * 16);
    be16(&mut out, selector);
    be16(&mut out, count * 16 - power * 16);
    let mut offset = 12 + 16 * u32::from(count);
    let mut head_at = 0usize;
    for (tag, data) in &tables {
        out.extend_from_slice(tag);
        be32(&mut out, checksum(data));
        be32(&mut out, offset);
        be32(&mut out, data.len() as u32);
        if tag == b"head" {
            head_at = offset as usize;
        }
        offset += (data.len() as u32 + 3) & !3;
    }
    for (_, data) in &tables {
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    let adjustment = 0xB1B0_AFBA_u32.wrapping_sub(checksum(&out));
    out[head_at + 8..head_at + 12].copy_from_slice(&adjustment.to_be_bytes());
    out
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Rights-safe synthetic font programs: Caladea's own outlines (Apache-2.0,
    //! bundled in `casual-doc-layout/fonts`) re-encoded as a bare CFF and as a
    //! Type1 program, so the wrap and the conversion are tested against an
    //! outline source this module did not write.

    use skrifa::MetadataProvider;
    use skrifa::outline::{DrawSettings, OutlinePen};
    use skrifa::prelude::{LocationRef, Size};

    use super::{Command, write_cff};
    use skrifa::raw::types::Fixed;

    /// The bundled face.
    pub fn caladea() -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../casual-doc-layout/fonts/Caladea-Regular.ttf"),
        )
        .expect("bundled Caladea")
    }

    #[derive(Default)]
    struct Pen(Vec<Command>, (f32, f32));

    fn fx(v: f32) -> Fixed {
        Fixed::from_f64(f64::from(v).round())
    }

    impl OutlinePen for Pen {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.push(Command::Move(fx(x), fx(y)));
            self.1 = (x, y);
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.push(Command::Line(fx(x), fx(y)));
            self.1 = (x, y);
        }
        fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
            let (x0, y0) = self.1;
            let c1 = (x0 + 2.0 / 3.0 * (cx - x0), y0 + 2.0 / 3.0 * (cy - y0));
            let c2 = (x + 2.0 / 3.0 * (cx - x), y + 2.0 / 3.0 * (cy - y));
            self.0.push(Command::Curve(
                fx(c1.0),
                fx(c1.1),
                fx(c2.0),
                fx(c2.1),
                fx(x),
                fx(y),
            ));
            self.1 = (x, y);
        }
        fn curve_to(&mut self, a: f32, b: f32, c: f32, d: f32, x: f32, y: f32) {
            self.0
                .push(Command::Curve(fx(a), fx(b), fx(c), fx(d), fx(x), fx(y)));
            self.1 = (x, y);
        }
        fn close(&mut self) {}
    }

    /// `(name, unicode, advance in 1000 units, outline)` for `chars`, from
    /// Caladea scaled to a 1000-unit em, glyph 0 being `.notdef`.
    pub(crate) fn glyphs(chars: &str) -> Vec<(String, Option<char>, u16, Vec<Command>)> {
        let bytes = caladea();
        let face = skrifa::FontRef::new(&bytes).expect("sfnt");
        use skrifa::raw::TableProvider;
        let upem = f32::from(face.head().expect("head").units_per_em());
        let scale = 1000.0 / upem;
        let charmap = face.charmap();
        let outlines = face.outline_glyphs();
        let metrics = face.glyph_metrics(Size::unscaled(), LocationRef::default());
        let mut out = vec![(".notdef".to_owned(), None, 500, Vec::new())];
        for c in chars.chars() {
            let gid = charmap.map(c).expect("Caladea maps it");
            let mut pen = Pen::default();
            outlines
                .get(gid)
                .expect("outline")
                .draw(
                    DrawSettings::unhinted(Size::new(1000.0), LocationRef::default()),
                    &mut pen,
                )
                .expect("draws");
            let advance = (metrics.advance_width(gid).unwrap_or(0.0) * scale).round() as u16;
            let mut name = [0u8; 64];
            let name = skrifa::raw::ps::agl::char_to_name(c, &mut name)
                .expect("AGL names it")
                .to_owned();
            out.push((name, Some(c), advance, pen.0));
        }
        out
    }

    /// A bare name-keyed CFF holding `chars`.
    pub fn bare_cff(chars: &str) -> Vec<u8> {
        let glyphs = glyphs(chars);
        let charstrings: Vec<Vec<u8>> = glyphs
            .iter()
            .map(|(_, _, advance, commands)| super::type2_charstring(commands, *advance))
            .collect();
        let names: Vec<&str> = glyphs[1..].iter().map(|(n, ..)| n.as_str()).collect();
        write_cff(
            "SynthCaladea",
            &names,
            &charstrings,
            [-200, -250, 1200, 950],
        )
    }

    /// The Type1 charstring encryption (Adobe Type 1 Font Format §7.2).
    fn encrypt(data: &[u8], mut r: u16) -> Vec<u8> {
        let (c1, c2) = (52_845u16, 22_719u16);
        data.iter()
            .map(|p| {
                let c = p ^ (r >> 8) as u8;
                r = (u16::from(c).wrapping_add(r))
                    .wrapping_mul(c1)
                    .wrapping_add(c2);
                c
            })
            .collect()
    }

    fn t1_number(out: &mut Vec<u8>, v: i32) {
        match v {
            -107..=107 => out.push((v + 139) as u8),
            108..=1131 => {
                let v = v - 108;
                out.push((v / 256 + 247) as u8);
                out.push((v % 256) as u8);
            }
            -1131..=-108 => {
                let v = -v - 108;
                out.push((v / 256 + 251) as u8);
                out.push((v % 256) as u8);
            }
            _ => {
                out.push(255);
                out.extend_from_slice(&v.to_be_bytes());
            }
        }
    }

    /// A Type1 program (PFA layout with a binary eexec section, as a PDF's
    /// `FontFile` carries it) holding `chars`.
    pub fn type1(chars: &str) -> Vec<u8> {
        let glyphs = glyphs(chars);
        let mut private = b"dup /Private 8 dict dup begin\n/RD{string currentfile exch readstring pop}executeonly def\n/ND{noaccess def}executeonly def\n/NP{noaccess put}executeonly def\n/lenIV 4 def\n/MinFeature{16 16} def\n/password 5839 def\n/BlueValues [] def\n".to_vec();
        private.extend_from_slice(
            format!("2 index /CharStrings {} dict dup begin\n", glyphs.len()).as_bytes(),
        );
        for (name, _, advance, commands) in &glyphs {
            let mut cs = Vec::new();
            t1_number(&mut cs, 0);
            t1_number(&mut cs, i32::from(*advance));
            cs.push(13); // hsbw
            let (mut x, mut y) = (0i32, 0i32);
            let mut open = false;
            for command in commands {
                let i = |f: Fixed| f.to_i32();
                match *command {
                    Command::Move(px, py) => {
                        if open {
                            cs.push(9); // closepath
                        }
                        t1_number(&mut cs, i(px) - x);
                        t1_number(&mut cs, i(py) - y);
                        cs.push(21);
                        (x, y) = (i(px), i(py));
                        open = true;
                    }
                    Command::Line(px, py) => {
                        t1_number(&mut cs, i(px) - x);
                        t1_number(&mut cs, i(py) - y);
                        cs.push(5);
                        (x, y) = (i(px), i(py));
                    }
                    Command::Curve(ax, ay, bx, by, px, py) => {
                        for v in [
                            i(ax) - x,
                            i(ay) - y,
                            i(bx) - i(ax),
                            i(by) - i(ay),
                            i(px) - i(bx),
                            i(py) - i(by),
                        ] {
                            t1_number(&mut cs, v);
                        }
                        cs.push(8);
                        (x, y) = (i(px), i(py));
                    }
                }
            }
            if open {
                cs.push(9);
            }
            cs.push(14);
            let mut plain = vec![0u8; 4];
            plain.extend_from_slice(&cs);
            let encrypted = encrypt(&plain, 4330);
            private.extend_from_slice(format!("/{name} {} RD ", encrypted.len()).as_bytes());
            private.extend_from_slice(&encrypted);
            private.extend_from_slice(b" ND\n");
        }
        private.extend_from_slice(b"end\nend\nreadonly put\nnoaccess put\ndup /FontName get exch definefont pop\nmark currentfile closefile\n");
        let mut plain = vec![0u8; 4];
        plain.extend_from_slice(&private);
        let binary = encrypt(&plain, 55_665);
        let mut out = b"%!PS-AdobeFont-1.0: SynthCaladea 001.000\n11 dict begin\n/FontName /SynthCaladea def\n/FontType 1 def\n/PaintType 0 def\n/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n/FontBBox {-200 -250 1200 950} readonly def\n/FontInfo 3 dict dup begin\n/FamilyName (SynthCaladea) readonly def\n/ItalicAngle 0 def\n/isFixedPitch false def\nend readonly def\n/Encoding StandardEncoding def\ncurrentdict end\ncurrentfile eexec\n".to_vec();
        out.extend_from_slice(&binary);
        out.extend_from_slice(b"\n");
        for _ in 0..8 {
            out.extend_from_slice(&[b'0'; 64]);
            out.push(b'\n');
        }
        out.extend_from_slice(b"cleartomark\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use skrifa::MetadataProvider;
    use skrifa::outline::{DrawSettings, OutlinePen};
    use skrifa::prelude::{LocationRef, Size};

    use super::test_support::{bare_cff, glyphs, type1};
    use super::{WrapRefusal, advances_agree, convert_type1, wrap_bare_cff};

    #[derive(Default)]
    struct Points(Vec<(i32, i32)>);
    impl OutlinePen for Points {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.push((x.round() as i32, y.round() as i32));
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.push((x.round() as i32, y.round() as i32));
        }
        fn quad_to(&mut self, _: f32, _: f32, x: f32, y: f32) {
            self.0.push((x.round() as i32, y.round() as i32));
        }
        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, x: f32, y: f32) {
            self.0.push((x.round() as i32, y.round() as i32));
        }
        fn close(&mut self) {}
    }

    /// On-curve points of `c`'s outline at a 1000-unit em, through skrifa.
    fn on_curve(font: &[u8], c: char) -> Vec<(i32, i32)> {
        let face = skrifa::FontRef::new(font).expect("an sfnt skrifa reads");
        let gid = face.charmap().map(c).expect("the cmap maps it");
        let mut points = Points::default();
        face.outline_glyphs()
            .get(gid)
            .expect("an outline")
            .draw(
                DrawSettings::unhinted(Size::new(1000.0), LocationRef::default()),
                &mut points,
            )
            .expect("draws");
        points.0
    }

    fn expected(c: char) -> Vec<(i32, i32)> {
        let source = glyphs(&c.to_string());
        source[1]
            .3
            .iter()
            .map(|command| match *command {
                super::Command::Move(x, y) | super::Command::Line(x, y) => (x.to_i32(), y.to_i32()),
                super::Command::Curve(_, _, _, _, x, y) => (x.to_i32(), y.to_i32()),
            })
            .collect()
    }

    #[test]
    fn a_bare_cff_is_wrapped_as_an_otto_font_skrifa_reads_with_the_same_outlines() {
        let otf = wrap_bare_cff(&bare_cff("Habe"), "Synth Caladea", false, false)
            .expect("a name-keyed CFF wraps");
        assert_eq!(&otf[..4], b"OTTO");
        for c in "Habe".chars() {
            assert_eq!(on_curve(&otf, c), expected(c), "outline of {c:?}");
        }
        let face = skrifa::FontRef::new(&otf).unwrap();
        assert!(
            face.charmap().map('z').is_none(),
            "only the program's glyphs are mapped"
        );
        // The checksum adjustment makes the whole font sum to the magic.
        let sum = otf.chunks(4).fold(0u32, |s, c| {
            let mut w = [0u8; 4];
            w[..c.len()].copy_from_slice(c);
            s.wrapping_add(u32::from_be_bytes(w))
        });
        assert_eq!(sum, 0xB1B0_AFBA);
    }

    #[test]
    fn a_type1_program_is_converted_with_the_same_outlines_and_advances() {
        let otf = convert_type1(&type1("Habe"), "Synth Caladea", false, false)
            .expect("a Type1 program converts");
        assert_eq!(&otf[..4], b"OTTO");
        for c in "Habe".chars() {
            assert_eq!(on_curve(&otf, c), expected(c), "outline of {c:?}");
        }
        let advances: BTreeMap<char, f64> = glyphs("Habe")[1..]
            .iter()
            .map(|(_, c, a, _)| (c.unwrap(), f64::from(*a) / 1000.0))
            .collect();
        assert!(advances_agree(&otf, &advances));
    }

    #[test]
    fn advances_that_disagree_distrust_the_names() {
        let otf = wrap_bare_cff(&bare_cff("Habe"), "Synth Caladea", false, false).unwrap();
        let wrong: BTreeMap<char, f64> = "Habe".chars().map(|c| (c, 0.2)).collect();
        assert!(!advances_agree(&otf, &wrong));
    }

    #[test]
    fn a_cid_keyed_cff_is_refused_and_garbage_is_unreadable() {
        let cjk = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/fonts/NotoSansCJK-TestSubset.otf"),
        )
        .expect("the CJK test subset");
        let face = skrifa::FontRef::new(&cjk).unwrap();
        let cff = face
            .table_data(skrifa::raw::types::Tag::new(b"CFF "))
            .expect("a CFF table");
        assert_eq!(
            wrap_bare_cff(cff.as_bytes(), "Noto", false, false),
            Err(WrapRefusal::CidKeyed)
        );
        assert_eq!(
            wrap_bare_cff(b"\x01\x00\x04\x04garbage", "x", false, false),
            Err(WrapRefusal::Unreadable)
        );
        assert_eq!(
            convert_type1(b"%!PS-AdobeFont garbage", "x", false, false),
            Err(WrapRefusal::Unreadable)
        );
    }
}
