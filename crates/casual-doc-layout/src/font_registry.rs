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

use parley::fontique::Blob;

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
}
