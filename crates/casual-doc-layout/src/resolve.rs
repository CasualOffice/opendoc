//! Font resolution and fallback (`P1C-002b`, `40-FONT-MANAGEMENT-DESIGN.md` §4).
//!
//! A run declares a font family (directly, or through the theme font scheme); the
//! engine must turn that name into a concrete bundled face. This module owns that
//! mapping as a pure, host-independent, WASM-safe function of the request and the
//! fixed bundled face set — no system/`fontconfig` discovery, so native and
//! `wasm32-unknown-unknown` resolve identically (design G2/G3).
//!
//! Two tiers of fallback, both deterministic (design §4):
//!
//! - **Whole-face substitution** — a requested family is mapped to a bundled
//!   face. Metric-compatible substitutes (same advances, so line breaks are
//!   preserved) are used where an Apache-2.0 partner exists: Cambria → Caladea.
//!   Families with no bundled metric-compatible partner (Calibri, Arial, Times
//!   New Roman, …) fall back visually to the bundled default (Roboto). Every
//!   non-exact outcome is recorded in a [`FontResolutionReport`].
//! - **Per-glyph coverage fallback** — if the resolved face lacks a glyph for a
//!   code point, the resolver walks the remaining families (preserving the
//!   bold/italic face) to the first that covers it, recording the substitution.
//!
//! Calibri's metric-compatible partner is Carlito, which ships only under the SIL
//! Open Font License — outside the repository's license allowlist — so it is not
//! bundled and Calibri resolves to the documented visual fallback.

use std::collections::BTreeMap;

use skrifa::{FontRef, MetadataProvider};

use crate::fonts::{self, BundledFamily, CALADEA, ROBOTO};
use crate::text::FontId;

/// Distinct-entry ceiling per report bucket; excess requests are counted against
/// existing entries but never grow the maps past this (bounded, like the
/// importer's compatibility report).
const MAX_REPORT_ENTRIES: usize = 4_096;

/// A request to resolve a declared font family to a concrete bundled face.
#[derive(Clone, Copy, Debug)]
pub struct FaceRequest<'a> {
    /// The declared family name (already theme-resolved to a concrete family).
    pub family: &'a str,
    /// Bold weight (`w:b`).
    pub bold: bool,
    /// Italic style (`w:i`).
    pub italic: bool,
}

/// How faithfully a resolved face matches the requested family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Disposition {
    /// The requested family is bundled and used directly.
    Exact,
    /// Substituted with a metric-compatible face: glyph advances match, so line
    /// breaking and pagination are preserved (only the glyph shapes differ).
    MetricCompatible,
    /// The requested family is unavailable and no metric-compatible bundled
    /// partner exists; a visual-only fallback face was chosen (layout may shift).
    Fallback,
}

impl Disposition {
    /// A stable, lowercase label for reports and diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Disposition::Exact => "exact",
            Disposition::MetricCompatible => "metric-compatible",
            Disposition::Fallback => "fallback",
        }
    }
}

/// The outcome of resolving a [`FaceRequest`] to a bundled face.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FaceMatch {
    /// The resolved bundled face.
    pub face: FontId,
    /// The resolved family's name.
    pub family: &'static str,
    /// How faithful the match is.
    pub disposition: Disposition,
}

/// Resolves a declared font family to a concrete bundled face over the fixed
/// bundled face set. Stateless: resolution is a pure function of the request, so
/// one resolver can be shared across a whole document.
#[derive(Clone, Copy, Debug, Default)]
pub struct FontResolver {
    _private: (),
}

impl FontResolver {
    /// Creates a resolver over the bundled face set.
    #[must_use]
    pub const fn new() -> Self {
        Self { _private: () }
    }

    /// Resolves a request to a bundled face and a disposition (design §4
    /// whole-face substitution). Total: always returns a face.
    #[must_use]
    pub fn resolve(&self, request: &FaceRequest<'_>) -> FaceMatch {
        let (family, disposition) = substitute(request.family);
        FaceMatch {
            face: family.face_id(request.bold, request.italic),
            family: family.name,
            disposition,
        }
    }

    /// Scans `text` for code points the resolved `face` cannot render and records
    /// each distinct one, with the first family in the fallback chain that
    /// covers it, into `report` (design §4 per-glyph coverage fallback). Control
    /// and whitespace code points are ignored (they never carry visible glyphs).
    pub fn record_coverage(&self, face: FontId, text: &str, report: &mut FontResolutionReport) {
        let bytes = fonts::face_bytes(face);
        let Ok(font) = FontRef::new(bytes) else {
            return;
        };
        let charmap = font.charmap();
        for ch in text.chars() {
            if ch.is_control() || ch.is_whitespace() {
                continue;
            }
            if charmap.map(ch).is_none()
                && let Some(fallback) = cover_fallback(face, ch)
            {
                report.note_coverage_fallback(ch, face, fallback);
            }
        }
    }
}

/// Whether the bundled `face` has a glyph for `ch`.
#[must_use]
pub fn covers(face: FontId, ch: char) -> bool {
    FontRef::new(fonts::face_bytes(face))
        .map(|font| font.charmap().map(ch).is_some())
        .unwrap_or(false)
}

/// Given a `primary` face that lacks `ch`, the first face in the fallback chain
/// (the other bundled families, preserving the bold/italic face) that covers it.
#[must_use]
pub fn cover_fallback(primary: FontId, ch: char) -> Option<FontId> {
    // The offset within a family block encodes the bold/italic face; preserve it
    // so a bold-italic run falls back to the next family's bold-italic face.
    let offset = primary.0 % 4;
    for family in fonts::FAMILIES {
        let candidate = family.face_id(offset & 1 == 1, offset & 2 == 2);
        if candidate != primary && covers(candidate, ch) {
            return Some(candidate);
        }
    }
    None
}

/// Maps a requested family name (case- and whitespace-insensitive) to a bundled
/// family and the fidelity of the substitution. The table covers the common Word
/// families; anything unknown falls back to the default family.
fn substitute(family: &str) -> (&'static BundledFamily, Disposition) {
    let key = family.trim().to_ascii_lowercase();
    match key.as_str() {
        // Bundled families requested directly.
        "roboto" => (&ROBOTO, Disposition::Exact),
        "caladea" => (&CALADEA, Disposition::Exact),
        // Metric-compatible Apache-2.0 substitute for Cambria.
        "cambria" => (&CALADEA, Disposition::MetricCompatible),
        // Common families with no bundled metric-compatible partner. Calibri's
        // partner (Carlito) is OFL-licensed and thus not bundled; Arial/Helvetica/
        // Times New Roman have Apache partners (Arimo/Tinos) that are not bundled
        // by this slice. All resolve to the documented visual fallback.
        "calibri" | "carlito" | "arial" | "helvetica" | "helvetica neue" | "times new roman"
        | "times" | "tinos" | "arimo" | "courier new" | "courier" | "cousine" => {
            (&ROBOTO, Disposition::Fallback)
        }
        _ => (&ROBOTO, Disposition::Fallback),
    }
}

/// One whole-face substitution the resolver performed, aggregated by requested
/// family name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubstitutionRecord {
    /// The requested family name (as first seen, verbatim).
    pub requested: String,
    /// The family the resolver chose.
    pub resolved_family: &'static str,
    /// How faithful the substitution is.
    pub disposition: Disposition,
    /// Bounded occurrence count.
    pub occurrences: u32,
}

/// One per-glyph coverage fallback, aggregated by code point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoverageRecord {
    /// The Unicode scalar value that the primary face could not render.
    pub codepoint: u32,
    /// The face that lacked the glyph.
    pub from: FontId,
    /// The fallback face that covered it.
    pub to: FontId,
    /// Bounded occurrence count.
    pub occurrences: u32,
}

/// A deterministic, bounded report of the font substitutions and coverage
/// fallbacks performed while laying out a document. Substitutions are keyed by
/// the normalized requested family and coverage fallbacks by code point, so both
/// iterate in a stable order (design §7 loss-awareness — substitution outcomes
/// are surfaced, never silently swapped).
#[derive(Clone, Debug, Default)]
pub struct FontResolutionReport {
    substitutions: BTreeMap<String, SubstitutionRecord>,
    coverage: BTreeMap<u32, CoverageRecord>,
}

impl FontResolutionReport {
    /// A fresh, empty report.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a resolution outcome. [`Disposition::Exact`] matches are not
    /// recorded — the report surfaces only deviations from the requested family.
    pub fn note_resolution(&mut self, requested: &str, outcome: &FaceMatch) {
        if outcome.disposition == Disposition::Exact {
            return;
        }
        let key = requested.trim().to_ascii_lowercase();
        if let Some(record) = self.substitutions.get_mut(&key) {
            record.occurrences = record.occurrences.saturating_add(1);
        } else if self.substitutions.len() < MAX_REPORT_ENTRIES {
            self.substitutions.insert(
                key,
                SubstitutionRecord {
                    requested: requested.trim().to_owned(),
                    resolved_family: outcome.family,
                    disposition: outcome.disposition,
                    occurrences: 1,
                },
            );
        }
    }

    /// Records a per-glyph coverage fallback for `ch`.
    pub fn note_coverage_fallback(&mut self, ch: char, from: FontId, to: FontId) {
        let codepoint = ch as u32;
        if let Some(record) = self.coverage.get_mut(&codepoint) {
            record.occurrences = record.occurrences.saturating_add(1);
        } else if self.coverage.len() < MAX_REPORT_ENTRIES {
            self.coverage.insert(
                codepoint,
                CoverageRecord {
                    codepoint,
                    from,
                    to,
                    occurrences: 1,
                },
            );
        }
    }

    /// Whether nothing was substituted or fell back (an all-exact document).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.substitutions.is_empty() && self.coverage.is_empty()
    }

    /// The whole-face substitutions, ordered by normalized family name.
    pub fn substitutions(&self) -> impl Iterator<Item = &SubstitutionRecord> {
        self.substitutions.values()
    }

    /// The per-glyph coverage fallbacks, ordered by code point.
    pub fn coverage_fallbacks(&self) -> impl Iterator<Item = &CoverageRecord> {
        self.coverage.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cambria_resolves_to_the_caladea_face_metric_compatible() {
        let resolver = FontResolver::new();
        let m = resolver.resolve(&FaceRequest {
            family: "Cambria",
            bold: false,
            italic: false,
        });
        assert_eq!(m.family, "Caladea");
        assert_eq!(m.face, CALADEA.face_id(false, false));
        assert_eq!(m.disposition, Disposition::MetricCompatible);
    }

    #[test]
    fn cambria_substitution_preserves_the_bold_italic_face() {
        let resolver = FontResolver::new();
        let m = resolver.resolve(&FaceRequest {
            family: "cambria",
            bold: true,
            italic: true,
        });
        assert_eq!(m.face, CALADEA.face_id(true, true));
    }

    #[test]
    fn calibri_falls_back_deterministically_and_is_reported() {
        let resolver = FontResolver::new();
        let mut report = FontResolutionReport::new();
        let m = resolver.resolve(&FaceRequest {
            family: "Calibri",
            bold: false,
            italic: false,
        });
        // No Apache-2.0 metric-compatible partner is bundled, so Calibri falls
        // back to the default family (deterministic).
        assert_eq!(m.family, "Roboto");
        assert_eq!(m.disposition, Disposition::Fallback);
        report.note_resolution("Calibri", &m);
        let subs: Vec<_> = report.substitutions().collect();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].requested, "Calibri");
        assert_eq!(subs[0].resolved_family, "Roboto");
        assert_eq!(subs[0].disposition, Disposition::Fallback);
    }

    #[test]
    fn an_unknown_family_falls_back_and_is_reported_as_substituted() {
        let resolver = FontResolver::new();
        let mut report = FontResolutionReport::new();
        let m = resolver.resolve(&FaceRequest {
            family: "Totally Made Up Font",
            bold: false,
            italic: false,
        });
        assert_eq!(m.family, "Roboto");
        assert_eq!(m.disposition, Disposition::Fallback);
        report.note_resolution("Totally Made Up Font", &m);
        report.note_resolution("Totally Made Up Font", &m);
        let subs: Vec<_> = report.substitutions().collect();
        assert_eq!(subs.len(), 1, "the same family aggregates");
        assert_eq!(subs[0].occurrences, 2);
    }

    #[test]
    fn exact_matches_are_not_reported() {
        let resolver = FontResolver::new();
        let mut report = FontResolutionReport::new();
        let m = resolver.resolve(&FaceRequest {
            family: "Roboto",
            bold: false,
            italic: false,
        });
        assert_eq!(m.disposition, Disposition::Exact);
        report.note_resolution("Roboto", &m);
        assert!(report.is_empty());
    }

    #[test]
    fn resolution_is_case_and_whitespace_insensitive() {
        let resolver = FontResolver::new();
        let a = resolver.resolve(&FaceRequest {
            family: "  CAMBRIA  ",
            bold: false,
            italic: false,
        });
        assert_eq!(a.family, "Caladea");
        assert_eq!(a.disposition, Disposition::MetricCompatible);
    }

    #[test]
    fn coverage_fallback_triggers_and_is_reported() {
        // Cyrillic Zhe (U+0416) is present in Roboto but absent from Caladea, so
        // a run resolved to Caladea must fall back to Roboto for that glyph and
        // report it. This exercises the real cmaps of the bundled faces.
        let resolver = FontResolver::new();
        let zhe = 'Ж';
        let caladea = CALADEA.face_id(false, false);
        let roboto = ROBOTO.face_id(false, false);
        assert!(!covers(caladea, zhe), "Caladea lacks Cyrillic Zhe");
        assert!(covers(roboto, zhe), "Roboto covers Cyrillic Zhe");
        assert_eq!(cover_fallback(caladea, zhe), Some(roboto));

        let mut report = FontResolutionReport::new();
        // 'a','b','c' are covered by Caladea; only 'Ж' is a coverage gap.
        resolver.record_coverage(caladea, "abcЖ", &mut report);
        let gaps: Vec<_> = report.coverage_fallbacks().collect();
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].codepoint, zhe as u32);
        assert_eq!(gaps[0].from, caladea);
        assert_eq!(gaps[0].to, roboto);
    }

    #[test]
    fn coverage_fallback_preserves_the_styled_face() {
        // A bold-italic run resolved to Caladea falls back to Roboto's
        // bold-italic face, not its regular face.
        let bold_italic_caladea = CALADEA.face_id(true, true);
        let fallback = cover_fallback(bold_italic_caladea, 'Ж');
        assert_eq!(fallback, Some(ROBOTO.face_id(true, true)));
    }

    #[test]
    fn latin_text_over_a_covering_face_reports_no_coverage_gap() {
        let resolver = FontResolver::new();
        let mut report = FontResolutionReport::new();
        resolver.record_coverage(ROBOTO.face_id(false, false), "Hello, world!", &mut report);
        assert!(report.coverage_fallbacks().next().is_none());
    }
}
