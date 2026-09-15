//! The dynamic font registry — the shared seam by which the shaper and the
//! renderer agree on the real bytes behind every face.
//!
//! The engine resolves three kinds of face through one registry (design goal:
//! the browser's network-font path reuses this seam with zero rework):
//!
//! 1. **Bundled** — the license-clean faces in [`crate::fonts`], keyed by their
//!    fixed [`FontId`] block (`0..=11`). These are *not* stored here; they are
//!    served directly from the bundled table (`fonts::face_bytes`).
//! 2. **System** — when the `system-fonts` feature is on (native only), `parley`
//!    /`fontique` shape a run's uncovered code points (CJK, symbols, complex
//!    scripts) with an installed OS font. The shaper [`interns`](FontRegistry::intern)
//!    the resolved blob here so the renderer can rasterize it.
//! 3. **Host-registered** — bytes a host hands in at runtime (e.g. a browser
//!    feeding network-fetched Noto CJK). Registered through
//!    [`crate::shape::ParleyShaper::register_font`], which registers the blob into
//!    the shaper's collection *and* interns it here.
//! 4. **Document-embedded** — the `.odttf` faces a DOCX carries in its own
//!    package ([`register_embedded_fonts`]). These are de-obfuscated here
//!    ([`deobfuscate_odttf`]) and registered under the family name the document
//!    declares, so the document's *own* face outranks any metric substitute
//!    (`40-FONT-MANAGEMENT-DESIGN.md` §3.2/§3.3: embedded → bundled →
//!    host-enumerated).
//!
//! Faces stored here are addressed by [`FontId`]s at or above [`DYNAMIC_FONT_BASE`],
//! an id space disjoint from the bundled block so the two never collide. The
//! registry is a cheap-to-clone `Arc` handle: the shaper populates it while
//! shaping; the renderer [`snapshots`](FontRegistry::snapshot) it to build a glyph
//! source. It also records the code points that shaped to `.notdef`
//! ([`missing_coverage`](FontRegistry::missing_coverage)) so a host can learn which
//! scripts it still needs to fetch a face for.
//!
//! The registry has no platform dependency — it is an in-memory blob store — so it
//! compiles on `wasm32` regardless of the `system-fonts` feature.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, Mutex};

use casual_doc_model::v1::Document;
use fontique::Blob;

use crate::shape::ParleyShaper;
use crate::text::FontId;

/// The first [`FontId`] handed to a dynamically resolved face (a system-resolved
/// or host-registered blob). Chosen far above the bundled block (`0..=11`) and any
/// resolver-assigned id so the bundled and dynamic id spaces never overlap.
pub const DYNAMIC_FONT_BASE: u32 = 0x1000_0000;

/// Shareable, read-only font-file bytes (an `Arc`-backed [`Blob`]). Derefs to the
/// raw bytes so the renderer can hand them straight to `skrifa` with no copy.
#[derive(Clone)]
pub struct FontBytes(Blob<u8>);

impl FontBytes {
    /// The raw font-file bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        self.0.data()
    }
}

impl core::ops::Deref for FontBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.0.data()
    }
}

impl fmt::Debug for FontBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontBytes")
            .field("len", &self.0.data().len())
            .finish()
    }
}

/// A dynamically resolved face: its file bytes plus the face index within a font
/// collection (`.ttc`); `0` for a single-face file. The renderer selects the face
/// with `skrifa::FontRef::from_index(bytes, index)`.
#[derive(Clone, Debug)]
pub struct DynFace {
    /// The font-file bytes.
    pub bytes: FontBytes,
    /// The face index within the file (`0` for a single-face `.ttf`/`.otf`).
    pub index: u32,
}

/// The shared dynamic font registry. Clone is cheap (an `Arc` handle over the same
/// store) — the shaper and the renderer hold the same underlying data.
#[derive(Clone, Default)]
pub struct FontRegistry {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    /// `(blob id, face index)` → the [`FontId`] assigned to it, so a face shaped
    /// many times is stored once (interning is idempotent).
    by_key: BTreeMap<(u64, u32), FontId>,
    /// `FontId.0` → the face bytes the renderer rasterizes.
    faces: BTreeMap<u32, DynFace>,
    /// Code points (as `u32`) that shaped to `.notdef` — the coverage gap.
    missing: BTreeSet<u32>,
    /// The next dynamic id offset above [`DYNAMIC_FONT_BASE`].
    next: u32,
}

impl FontRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `font` addresses a dynamically registered face (system or host),
    /// as opposed to a bundled face served from [`crate::fonts`].
    #[must_use]
    pub fn is_dynamic(font: FontId) -> bool {
        font.0 >= DYNAMIC_FONT_BASE
    }

    /// Interns a `(blob, face index)` pair, returning a stable [`FontId`] for it.
    /// Idempotent: the same blob+index always maps to the same id, so a face used
    /// across many runs is stored once. Used for system-resolved and
    /// host-registered faces alike.
    #[must_use]
    pub fn intern(&self, blob: Blob<u8>, index: u32) -> FontId {
        let key = (blob.id(), index);
        let mut inner = self.inner.lock().expect("font registry mutex poisoned");
        if let Some(id) = inner.by_key.get(&key) {
            return *id;
        }
        let id = FontId(DYNAMIC_FONT_BASE + inner.next);
        inner.next += 1;
        inner.by_key.insert(key, id);
        inner.faces.insert(
            id.0,
            DynFace {
                bytes: FontBytes(blob),
                index,
            },
        );
        id
    }

    /// The bytes+index for a dynamically resolved [`FontId`], or `None` for a
    /// bundled id (served from [`crate::fonts`]) or an unknown id.
    #[must_use]
    pub fn face(&self, font: FontId) -> Option<DynFace> {
        self.inner
            .lock()
            .expect("font registry mutex poisoned")
            .faces
            .get(&font.0)
            .cloned()
    }

    /// A snapshot of every dynamically registered face `(FontId, DynFace)` — the
    /// renderer builds its glyph source from this once per render.
    #[must_use]
    pub fn snapshot(&self) -> Vec<(FontId, DynFace)> {
        self.inner
            .lock()
            .expect("font registry mutex poisoned")
            .faces
            .iter()
            .map(|(id, face)| (FontId(*id), face.clone()))
            .collect()
    }

    /// Records a code point that shaped to `.notdef` — no bundled, system, or
    /// host-registered face covered it. A host consults
    /// [`missing_coverage`](Self::missing_coverage) to learn what to fetch.
    pub fn note_missing(&self, ch: char) {
        self.inner
            .lock()
            .expect("font registry mutex poisoned")
            .missing
            .insert(ch as u32);
    }

    /// The code points that shaped to `.notdef` so far (the coverage gap), sorted
    /// ascending. A host maps these to scripts to decide which faces to fetch and
    /// register (e.g. Han code points → fetch Noto Sans CJK).
    #[must_use]
    pub fn missing_coverage(&self) -> Vec<char> {
        self.inner
            .lock()
            .expect("font registry mutex poisoned")
            .missing
            .iter()
            .filter_map(|&c| char::from_u32(c))
            .collect()
    }
}

impl fmt::Debug for FontRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.inner.lock().expect("font registry mutex poisoned");
        f.debug_struct("FontRegistry")
            .field("faces", &inner.faces.len())
            .field("missing", &inner.missing.len())
            .finish()
    }
}

/// The number of leading bytes an OOXML embedded font obfuscates.
///
/// ECMA-376 Part 1 §17.8.1 obfuscates exactly the first 32 bytes of the font
/// stream; everything from byte 32 on is the untouched OpenType file.
const OBFUSCATED_PREFIX_BYTES: usize = 32;

/// The largest de-obfuscated embedded face the engine will register.
///
/// An `.odttf` is attacker-controlled: it arrives inside the package and is
/// copied into memory before a single byte is parsed. 32 MiB comfortably admits
/// a full unsubsetted CJK face while keeping a hostile package from turning one
/// `w:embedRegular` into an unbounded allocation. This mirrors the viewer's
/// host-font ceiling (`casual-doc-wasm` `MAX_HOST_FONT_BYTES`) and the house
/// style in `casual-doc-package`'s `limits.rs`: a named bound, enforced, not a
/// magic number at the call site.
pub const MAX_EMBEDDED_FACE_BYTES: usize = 32 * 1024 * 1024;

/// The largest aggregate of embedded faces one document may register.
///
/// Per-face bounding alone is not enough: `fontTable.xml` may declare an
/// arbitrary number of `w:font` entries, each with four faces. This caps the
/// whole document at four maximal faces' worth.
pub const MAX_EMBEDDED_FONT_TOTAL_BYTES: u64 = 128 * 1024 * 1024;

/// Why one embedded (`.odttf`) face could not be used, so the caller can report
/// it instead of silently substituting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EmbeddedFaceError {
    /// The `.odttf` package part the `r:id` resolves to could not be read.
    MissingPart,
    /// `w:fontKey` is not a 32-hex-digit GUID, so the stream cannot be decoded.
    MalformedFontKey,
    /// Shorter than the 32 obfuscated bytes — not a font stream at all.
    TooShort,
    /// Larger than [`MAX_EMBEDDED_FACE_BYTES`].
    TooLarge,
    /// Registering it would exceed [`MAX_EMBEDDED_FONT_TOTAL_BYTES`].
    BudgetExceeded,
    /// De-obfuscation produced no recognized sfnt signature — a wrong key or a
    /// corrupt stream.
    NotSfnt,
    /// The bytes are a plausible sfnt but the shaper's font collection refused
    /// them (unparseable tables).
    NotRegistrable,
}

impl EmbeddedFaceError {
    /// A stable, reportable identifier for this failure — the suffix a host
    /// appends to its compatibility-report feature id.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingPart => "missing-part",
            Self::MalformedFontKey => "malformed-font-key",
            Self::TooShort => "too-short",
            Self::TooLarge => "too-large",
            Self::BudgetExceeded => "budget-exceeded",
            Self::NotSfnt => "not-sfnt",
            Self::NotRegistrable => "not-registrable",
        }
    }
}

impl fmt::Display for EmbeddedFaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One embedded face the engine could not use. The run keeps its metric
/// substitute; the caller reports this so the loss is visible rather than silent
/// (`AGENTS.md`: unsupported data is preserved where safe or reported
/// explicitly).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddedFaceFailure {
    /// The `w:font/@w:name` family the face belongs to.
    pub family: String,
    /// The `w:embed*` element the face came from (`w:embedRegular`, …).
    pub slot: &'static str,
    /// The `.odttf` package part name.
    pub part_name: String,
    /// Why the face was rejected.
    pub error: EmbeddedFaceError,
}

/// What [`register_embedded_fonts`] did: the faces that became usable and the
/// ones that did not.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EmbeddedFontOutcome {
    /// The families with at least one face registered, in `fontTable.xml` order,
    /// deduplicated. A run naming one of these now shapes with the document's
    /// own face.
    pub families: Vec<String>,
    /// How many faces registered successfully.
    pub faces: usize,
    /// Faces that could not be used; each falls back to substitution.
    pub failures: Vec<EmbeddedFaceFailure>,
}

/// Reverses the OOXML embedded-font obfuscation of an `.odttf` stream.
///
/// **Algorithm (ECMA-376 Part 1 §17.8.1, "Embedded Font Obfuscation").** The
/// producer generates a GUID, writes it as `w:fontKey` (`{XXXXXXXX-XXXX-XXXX-
/// XXXX-XXXXXXXXXXXX}`), and XORs the first 32 bytes of the font file with the
/// 16-byte key twice:
///
/// 1. strip `{`, `}` and `-` from `font_key`, leaving 32 hexadecimal digits;
/// 2. read them as 16 bytes **in string order**, then **reverse** the array:
///    `key[i] = b[15 - i]`;
/// 3. for `i` in `0..32`, `font[i] ^= key[i % 16]`;
/// 4. bytes `32..` are the unmodified OpenType file.
///
/// Step 2 is plain hex-string reversal — *not* .NET `Guid.ToByteArray()`'s
/// mixed-endian layout, which byte-swaps the first three GUID groups. Both
/// produce the same first eight key bytes, so both yield a valid-looking sfnt
/// signature; only hex-string reversal decodes bytes 8..15 correctly. Verified
/// against a real Word-produced package (`demo.docx`, six embedded faces across
/// three families): hex-string reversal yields `00010000 0015 0100 0004 0050
/// 'DSIG'` — a coherent sfnt header and table directory — for every face, while
/// the .NET layout corrupts bytes 8..15 of all six. This closes open question 1
/// in `40-FONT-MANAGEMENT-DESIGN.md` §8.
///
/// The transform is its own inverse, so the same function re-obfuscates.
///
/// Returns the de-obfuscated OpenType bytes, or the reason they are unusable.
/// Never panics on hostile input: a malformed key, a truncated stream, an
/// oversized stream, and a stream that does not decode to a recognized sfnt
/// signature are all errors the caller reports.
pub fn deobfuscate_odttf(font_key: &str, obfuscated: &[u8]) -> Result<Vec<u8>, EmbeddedFaceError> {
    let key = font_key_bytes(font_key).ok_or(EmbeddedFaceError::MalformedFontKey)?;
    if obfuscated.len() < OBFUSCATED_PREFIX_BYTES {
        return Err(EmbeddedFaceError::TooShort);
    }
    if obfuscated.len() > MAX_EMBEDDED_FACE_BYTES {
        return Err(EmbeddedFaceError::TooLarge);
    }
    let mut font = obfuscated.to_vec();
    for (index, byte) in font.iter_mut().take(OBFUSCATED_PREFIX_BYTES).enumerate() {
        *byte ^= key[index % key.len()];
    }
    if !is_sfnt(&font) {
        return Err(EmbeddedFaceError::NotSfnt);
    }
    Ok(font)
}

/// The 16 de-obfuscation key bytes of a `w:fontKey`, in the order they are
/// applied (hex-string order, reversed). `None` for anything that is not exactly
/// 32 hexadecimal digits once `{`, `}` and `-` are removed.
fn font_key_bytes(font_key: &str) -> Option<[u8; 16]> {
    let mut nibbles = [0_u8; 32];
    let mut count = 0_usize;
    for ch in font_key.chars() {
        if matches!(ch, '{' | '}' | '-') {
            continue;
        }
        let digit = ch.to_digit(16)?;
        if count == nibbles.len() {
            return None;
        }
        // `to_digit(16)` yields 0..=15, which always fits a `u8`.
        nibbles[count] = u8::try_from(digit).ok()?;
        count += 1;
    }
    if count != nibbles.len() {
        return None;
    }
    let mut key = [0_u8; 16];
    for (index, byte) in key.iter_mut().enumerate() {
        // Reverse the string-order byte array: key[i] = b[15 - i].
        let source = 15 - index;
        *byte = (nibbles[source * 2] << 4) | nibbles[source * 2 + 1];
    }
    Some(key)
}

/// Whether `bytes` opens with a recognized sfnt signature — TrueType
/// (`0x00010000`), CFF/OpenType (`OTTO`), the legacy Apple `true` tag, or a
/// TrueType collection (`ttcf`). The cheap structural gate that keeps a stream
/// decoded with the wrong key (or an outright corrupt one) out of the font
/// collection (`40-FONT-MANAGEMENT-DESIGN.md` §3.3, G4).
fn is_sfnt(bytes: &[u8]) -> bool {
    matches!(
        bytes.first_chunk::<4>(),
        Some(b"\x00\x01\x00\x00" | b"OTTO" | b"true" | b"ttcf")
    )
}

/// De-obfuscates every `.odttf` face `document` embeds and registers it with
/// `shaper` under the family name the document declares, so the document's own
/// face outranks the bundled metric substitute for that family.
///
/// `part_bytes` resolves an `.odttf` package part name to its obfuscated bytes
/// (the host's resource map). A face whose part is missing, whose key is
/// malformed, or whose stream does not decode to an sfnt is *skipped* — the run
/// keeps its substitute — and recorded in
/// [`EmbeddedFontOutcome::failures`] for the caller to report. Nothing here
/// panics and nothing is silently dropped.
///
/// Registration is idempotent at the blob level (the registry interns by blob
/// id), but calling this twice on the same shaper registers the family twice;
/// call it once, at document open.
pub fn register_embedded_fonts<'bytes>(
    shaper: &ParleyShaper,
    document: &Document,
    part_bytes: impl FnMut(&str) -> Option<&'bytes [u8]>,
) -> EmbeddedFontOutcome {
    register_embedded_fonts_bounded(shaper, document, part_bytes, MAX_EMBEDDED_FONT_TOTAL_BYTES)
}

/// [`register_embedded_fonts`] with an explicit aggregate byte budget.
///
/// The budget is a parameter rather than only a constant so the exhaustion path
/// is reachable in a test without allocating
/// [`MAX_EMBEDDED_FONT_TOTAL_BYTES`] of fonts. Production callers should use
/// [`register_embedded_fonts`].
pub fn register_embedded_fonts_bounded<'bytes>(
    shaper: &ParleyShaper,
    document: &Document,
    mut part_bytes: impl FnMut(&str) -> Option<&'bytes [u8]>,
    total_budget_bytes: u64,
) -> EmbeddedFontOutcome {
    let mut outcome = EmbeddedFontOutcome::default();
    let mut budget = total_budget_bytes;
    for font in &document.definitions().font_table {
        for (slot, face) in font.embedded.faces() {
            let decoded = part_bytes(&face.part_name)
                .ok_or(EmbeddedFaceError::MissingPart)
                .and_then(|bytes| {
                    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > budget {
                        Err(EmbeddedFaceError::BudgetExceeded)
                    } else {
                        Ok(bytes)
                    }
                })
                .and_then(|bytes| deobfuscate_odttf(&face.font_key, bytes));
            let error = match decoded {
                Ok(bytes) => {
                    budget = budget.saturating_sub(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
                    let (bold, italic) = slot_attributes(slot);
                    let ids = shaper.register_embedded_face(bytes, &font.name, bold, italic);
                    if ids.is_empty() {
                        Some(EmbeddedFaceError::NotRegistrable)
                    } else {
                        outcome.faces += ids.len();
                        if !outcome.families.contains(&font.name) {
                            outcome.families.push(font.name.clone());
                        }
                        None
                    }
                }
                Err(error) => Some(error),
            };
            if let Some(error) = error {
                outcome.failures.push(EmbeddedFaceFailure {
                    family: font.name.clone(),
                    slot,
                    part_name: face.part_name.clone(),
                    error,
                });
            }
        }
    }
    outcome
}

/// The `(bold, italic)` a `w:embed*` slot name declares. The slot — not the
/// subset's own OS/2 table — is authoritative: a subsetted face often keeps the
/// base family's attributes, and Word selects by slot.
const fn slot_attributes(slot: &str) -> (bool, bool) {
    match slot.as_bytes() {
        b"w:embedBold" => (true, false),
        b"w:embedItalic" => (false, true),
        b"w:embedBoldItalic" => (true, true),
        // `w:embedRegular` and anything unrecognized.
        _ => (false, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn blob(bytes: &[u8]) -> Blob<u8> {
        Blob::new(Arc::new(bytes.to_vec()))
    }

    #[test]
    fn interning_is_idempotent_and_dynamic() {
        let registry = FontRegistry::new();
        let data = blob(crate::fonts::ROBOTO_REGULAR);
        let a = registry.intern(data.clone(), 0);
        let b = registry.intern(data, 0);
        assert_eq!(a, b, "the same blob+index interns to the same id");
        assert!(FontRegistry::is_dynamic(a), "interned ids are dynamic");
        assert!(a.0 >= DYNAMIC_FONT_BASE);
    }

    #[test]
    fn distinct_faces_get_distinct_ids_and_serve_their_bytes() {
        let registry = FontRegistry::new();
        let a = registry.intern(blob(crate::fonts::ROBOTO_REGULAR), 0);
        let b = registry.intern(blob(crate::fonts::CALADEA_REGULAR), 0);
        assert_ne!(a, b, "distinct blobs get distinct ids");
        assert_eq!(
            registry.face(a).unwrap().bytes.as_slice(),
            crate::fonts::ROBOTO_REGULAR
        );
        assert_eq!(
            registry.face(b).unwrap().bytes.as_slice(),
            crate::fonts::CALADEA_REGULAR
        );
        // A face index rides through.
        let c = registry.intern(blob(crate::fonts::CALADEA_REGULAR), 3);
        assert_eq!(registry.face(c).unwrap().index, 3);
    }

    #[test]
    fn bundled_ids_are_not_dynamic() {
        assert!(!FontRegistry::is_dynamic(FontId(0)));
        assert!(!FontRegistry::is_dynamic(FontId(11)));
    }

    #[test]
    fn missing_coverage_is_recorded_and_sorted() {
        let registry = FontRegistry::new();
        registry.note_missing('文');
        registry.note_missing('中');
        registry.note_missing('中'); // deduped
        let missing = registry.missing_coverage();
        assert_eq!(missing, vec!['中', '文'], "sorted, deduped code points");
    }

    #[test]
    fn snapshot_reflects_registered_faces() {
        let registry = FontRegistry::new();
        assert!(registry.snapshot().is_empty());
        let id = registry.intern(blob(crate::fonts::ROBOTO_REGULAR), 0);
        let snap = registry.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].0, id);
    }

    /// The first 32 bytes of `word/fonts/font1.odttf` from a real Word-produced
    /// package (six embedded faces over three families), with its `w:fontKey`.
    /// A 32-byte header excerpt, kept as a *convention* vector: it is the only
    /// evidence that distinguishes plain hex-string key reversal from .NET
    /// `Guid.ToByteArray()`'s mixed-endian layout, which the two candidate
    /// readings of ECMA-376 Part 1 §17.8.1 disagree on.
    const REAL_FONT_KEY: &str = "{3EEE3167-E5B8-4798-AE48-EA6B71E31D4D}";
    const REAL_OBFUSCATED_PREFIX: [u8; 32] = [
        0x4D, 0x1C, 0xE3, 0x71, 0x6B, 0xFF, 0x49, 0xAE, 0x98, 0x43, 0xB8, 0xB5, 0x23, 0x62, 0xA7,
        0x79, 0x15, 0x12, 0x90, 0x39, 0x6B, 0xEF, 0x04, 0x5E, 0x98, 0x47, 0xA1, 0xD5, 0x20, 0x61,
        0xA1, 0x6D,
    ];
    /// The sfnt header the real face must decode to: version `0x00010000`,
    /// `numTables = 21`, `searchRange = 0x0100`, `entrySelector = 4`,
    /// `rangeShift = 0x50`, then the first table directory entry (`DSIG`,
    /// checksum, offset, length) and the second tag (`GPOS`). Only a correctly
    /// ordered key produces a coherent directory like this.
    const REAL_PLAIN_PREFIX: [u8; 32] = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x15, 0x01, 0x00, 0x00, 0x04, 0x00, 0x50, 0x44, 0x53, 0x49,
        0x47, 0x58, 0x0F, 0x73, 0x48, 0x00, 0x05, 0x4C, 0xF0, 0x00, 0x00, 0x19, 0x30, 0x47, 0x50,
        0x4F, 0x53,
    ];

    /// Obfuscates `plain` with `font_key`, written independently of
    /// [`deobfuscate_odttf`] so a round-trip assertion is not the production
    /// function checked against itself.
    fn obfuscate(font_key: &str, plain: &[u8]) -> Vec<u8> {
        let digits: Vec<u32> = font_key.chars().filter_map(|c| c.to_digit(16)).collect();
        assert_eq!(digits.len(), 32, "a fontKey is 32 hex digits");
        let string_order: Vec<u8> = digits
            .chunks(2)
            .map(|pair| (pair[0] * 16 + pair[1]) as u8)
            .collect();
        let key: Vec<u8> = string_order.into_iter().rev().collect();
        let mut out = plain.to_vec();
        for (index, byte) in out.iter_mut().take(32).enumerate() {
            *byte ^= key[index % 16];
        }
        out
    }

    /// The de-obfuscation key is the `w:fontKey` GUID's hex-digit pairs in
    /// **reverse** order. Pins `40-FONT-MANAGEMENT-DESIGN.md` §3.3's worked
    /// example.
    #[test]
    fn the_font_key_is_the_guid_hex_bytes_reversed() {
        let key = font_key_bytes("{001B70DC-AA60-4AD5-90EC-18A0948E1EAE}")
            .expect("a well-formed fontKey parses");
        assert_eq!(
            key,
            [
                0xAE, 0x1E, 0x8E, 0x94, 0xA0, 0x18, 0xEC, 0x90, 0xD5, 0x4A, 0x60, 0xAA, 0xDC, 0x70,
                0x1B, 0x00,
            ],
        );
        // Braces and dashes are optional punctuation, not part of the key.
        assert_eq!(
            font_key_bytes("001B70DCAA604AD590EC18A0948E1EAE"),
            Some(key),
        );
    }

    /// The convention guard: a real Word-produced `.odttf` prefix must decode to
    /// its real sfnt header. This is what fails if the key bytes are built with
    /// .NET `Guid.ToByteArray()`'s mixed-endian layout instead of plain
    /// hex-string reversal — the two orders agree on the first eight key bytes
    /// (so both yield a plausible `0x00010000` signature and pass an sfnt check)
    /// and disagree on the rest, so only a full-prefix comparison catches it.
    #[test]
    fn a_real_word_produced_odttf_prefix_decodes_to_its_sfnt_header() {
        let decoded = deobfuscate_odttf(REAL_FONT_KEY, &REAL_OBFUSCATED_PREFIX)
            .expect("a real embedded face decodes");
        assert_eq!(
            decoded, REAL_PLAIN_PREFIX,
            "the de-obfuscated prefix must be the face's real sfnt header and \
             first table directory entries",
        );
    }

    /// The transform is its own inverse and touches only the first 32 bytes.
    #[test]
    fn deobfuscation_restores_the_whole_face_and_only_the_first_32_bytes_differ() {
        let plain = crate::fonts::LIBERATION_MONO_REGULAR;
        let obfuscated = obfuscate(REAL_FONT_KEY, plain);
        assert_ne!(
            &obfuscated[..OBFUSCATED_PREFIX_BYTES],
            &plain[..OBFUSCATED_PREFIX_BYTES],
            "the header is actually obfuscated",
        );
        assert_eq!(
            &obfuscated[OBFUSCATED_PREFIX_BYTES..],
            &plain[OBFUSCATED_PREFIX_BYTES..],
            "bytes 32.. are untouched by the obfuscation",
        );
        let decoded =
            deobfuscate_odttf(REAL_FONT_KEY, &obfuscated).expect("a valid stream decodes");
        assert_eq!(decoded, plain, "de-obfuscation restores the exact face");
    }

    /// Every hostile shape is an error, never a panic and never a face.
    #[test]
    fn malformed_keys_and_streams_are_rejected_without_panicking() {
        let plain = crate::fonts::LIBERATION_MONO_REGULAR;
        let obfuscated = obfuscate(REAL_FONT_KEY, plain);
        for bad_key in [
            "",
            "{}",
            "not-a-guid",
            "{3EEE3167-E5B8-4798-AE48-EA6B71E31D4}", // 31 digits
            "{3EEE3167-E5B8-4798-AE48-EA6B71E31D4DA}", // 33 digits
            "{3EEE3167-E5B8-4798-AE48-EA6B71E31D4G}", // non-hex digit
            "{3EEE3167 E5B8 4798 AE48 EA6B71E31D4D}", // spaces are not punctuation
        ] {
            assert_eq!(
                deobfuscate_odttf(bad_key, &obfuscated),
                Err(EmbeddedFaceError::MalformedFontKey),
                "{bad_key:?} is not a usable fontKey",
            );
        }
        assert_eq!(
            deobfuscate_odttf(REAL_FONT_KEY, &obfuscated[..31]),
            Err(EmbeddedFaceError::TooShort),
            "a stream shorter than the obfuscated prefix is not a font",
        );
        assert_eq!(
            deobfuscate_odttf(REAL_FONT_KEY, &[]),
            Err(EmbeddedFaceError::TooShort),
        );
    }

    /// A stream decoded with the wrong key is refused rather than handed to the
    /// font collection as if it were a face (`40` §3.3 G4, and the difference
    /// between a substituted face and painted tofu).
    #[test]
    fn a_wrong_key_is_refused_instead_of_yielding_a_bogus_face() {
        let obfuscated = obfuscate(REAL_FONT_KEY, crate::fonts::LIBERATION_MONO_REGULAR);
        assert_eq!(
            deobfuscate_odttf("{00000000-0000-0000-0000-000000000000}", &obfuscated),
            Err(EmbeddedFaceError::NotSfnt),
        );
        // A stream that is simply not a font, decoded with its own key, is the
        // same failure.
        let junk = obfuscate(REAL_FONT_KEY, &[0x7F; 64]);
        assert_eq!(
            deobfuscate_odttf(REAL_FONT_KEY, &junk),
            Err(EmbeddedFaceError::NotSfnt),
        );
    }

    /// The per-face resource bound is enforced before the copy, so an oversized
    /// embedded face cannot be turned into an unbounded allocation.
    #[test]
    fn an_oversized_embedded_face_is_refused() {
        // A sparse `Vec` one byte over the cap: the check must reject it on
        // length alone, without needing a valid font.
        let oversized = vec![0_u8; MAX_EMBEDDED_FACE_BYTES + 1];
        assert_eq!(
            deobfuscate_odttf(REAL_FONT_KEY, &oversized),
            Err(EmbeddedFaceError::TooLarge),
        );
    }

    /// Every recognized sfnt signature is admitted, and nothing else is.
    #[test]
    fn only_recognized_sfnt_signatures_are_admitted() {
        for good in [[0x00, 0x01, 0x00, 0x00], *b"OTTO", *b"true", *b"ttcf"] {
            let mut bytes = [0_u8; 40];
            bytes[..4].copy_from_slice(&good);
            assert!(is_sfnt(&bytes), "{good:?} is an sfnt signature");
        }
        for bad in [*b"wOFF", *b"wOF2", *b"ttf\0", [0x00, 0x02, 0x00, 0x00]] {
            let mut bytes = [0_u8; 40];
            bytes[..4].copy_from_slice(&bad);
            assert!(!is_sfnt(&bytes), "{bad:?} is not an sfnt signature");
        }
        assert!(
            !is_sfnt(&[0x00, 0x01, 0x00]),
            "a short buffer is not an sfnt"
        );
    }

    /// The `w:embed*` slot — not the subset's own OS/2 table — decides the face's
    /// weight and style.
    #[test]
    fn the_embed_slot_names_the_weight_and_style() {
        assert_eq!(slot_attributes("w:embedRegular"), (false, false));
        assert_eq!(slot_attributes("w:embedBold"), (true, false));
        assert_eq!(slot_attributes("w:embedItalic"), (false, true));
        assert_eq!(slot_attributes("w:embedBoldItalic"), (true, true));
    }

    #[test]
    fn failure_reasons_have_stable_report_ids() {
        for (error, id) in [
            (EmbeddedFaceError::MissingPart, "missing-part"),
            (EmbeddedFaceError::MalformedFontKey, "malformed-font-key"),
            (EmbeddedFaceError::TooShort, "too-short"),
            (EmbeddedFaceError::TooLarge, "too-large"),
            (EmbeddedFaceError::BudgetExceeded, "budget-exceeded"),
            (EmbeddedFaceError::NotSfnt, "not-sfnt"),
            (EmbeddedFaceError::NotRegistrable, "not-registrable"),
        ] {
            assert_eq!(error.as_str(), id);
            assert_eq!(error.to_string(), id);
        }
    }
}
