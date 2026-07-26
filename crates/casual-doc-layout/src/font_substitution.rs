//! Metric-compatible substitution of a requested font family.
//!
//! When a run requests a family the host does not have installed (every
//! deterministic / WASM build, and any machine missing the font), the engine
//! must pick a bundled face to shape, paginate, *and* rasterize with. Shaping a
//! missing Arial or Times New Roman run with the bundled default (Roboto/Caladea)
//! gives it the *wrong* advances, so words-per-line — and therefore the page
//! count — diverges from Word/LibreOffice. This module maps the requested family
//! to a bundled face whose metrics *match* the substitute LibreOffice would use:
//!
//! - Arial / Helvetica → **Liberation Sans** (metric-compatible with Arial)
//! - Times New Roman / Times → **Liberation Serif** (metric-compatible with Times)
//! - Courier New / Courier → **Liberation Mono** (metric-compatible with Courier)
//! - Calibri → **Carlito**, Cambria → **Caladea** (the existing bundled partners)
//!
//! The Liberation set is LibreOffice's *own* metric-compatible substitute family,
//! so a document that names Arial/Times/Courier without those fonts installed
//! breaks lines and paginates the same way LibreOffice does — the keystone fix
//! for page-count and line-breaking divergence (`40-FONT-MANAGEMENT-DESIGN.md`).
//!
//! An *unknown* missing family (one with no listed metric partner — the demo's
//! `Ubuntu`, class notes' `PT Serif` / `Old Standard TT`) is classified by its
//! generic family — serif → Liberation Serif, monospace → Liberation Mono,
//! sans-serif (the default) → Liberation Sans — using a small known-family name
//! list plus the CSS generic names (`serif` / `sans-serif` / `monospace`) and a
//! `mono` / `sans` / `serif` substring heuristic. This keeps a missing font on a
//! face with plausible Latin metrics rather than an arbitrary default.
//!
//! This is the *single source of truth* for whole-face substitution, consulted by
//! two seams that must agree so a bundled face is shaped and rasterized as the
//! same font:
//!
//! - [`crate::resolve::FontResolver`] maps the requested name to the bundled
//!   [`crate::text::FontId`] a run carries (the id the renderer outlines with);
//! - [`crate::shape::ParleyShaper`]'s `pick_family` shapes a *missing* requested
//!   family with the substitute's family name.
//!
//! Because both read this table, the shaped face and the outlined face are always
//! the same bundled family. An *installed* requested face (real OS Arial under
//! `system-fonts`) still wins in `pick_family` and keeps its true metrics —
//! substitution only applies when the requested family is genuinely absent.
//!
//! The function is pure over the requested name — host-independent and WASM-safe,
//! so native and `wasm32-unknown-unknown` substitute identically.
//!
//! Note on `w:rFonts@hint` ([`casual_doc_model::v1::RunFontHint`]): the hint
//! disambiguates *which script slot* (`eastAsia` / `cs`) applies to an ambiguous
//! code point, not whether a face is serif, sans, or monospaced, so it carries no
//! signal for metric substitution and is deliberately not consumed here. The
//! classification relies on the family name alone, which is the signal the
//! resolver and shaper actually have for a run.

use crate::fonts::{
    BundledFamily, CALADEA, CARLITO, LIBERATION_MONO, LIBERATION_SANS, LIBERATION_SERIF, ROBOTO,
};

/// How faithfully a substitute matches the requested family — the fidelity a
/// caller reports (see [`crate::resolve::Disposition`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubstituteKind {
    /// The requested family *is* one of the bundled families; it is used directly.
    Bundled,
    /// A different bundled family whose advances match the requested one, so line
    /// breaking and pagination are preserved (only the glyph shapes differ).
    MetricCompatible,
    /// No metric-compatible partner; the family was classified by generic family
    /// (serif/sans/mono). Layout may shift relative to the true font.
    Generic,
}

/// The bundled family a requested family resolves to, and how faithful the match
/// is.
#[derive(Clone, Copy, Debug)]
pub struct Substitute {
    /// The bundled family to shape and rasterize the run with.
    pub family: &'static BundledFamily,
    /// The fidelity of the substitution.
    pub kind: SubstituteKind,
}

/// The bundled family a requested `family` should be shaped and rasterized with,
/// or `None` when the name is blank (nothing to substitute — the caller keeps its
/// default).
///
/// Matching is case- and surrounding-whitespace-insensitive. A named family
/// always yields a substitute: the family itself if bundled, a known metric
/// partner where one exists, else a generic-family classification defaulting to
/// [`LIBERATION_SANS`].
#[must_use]
pub fn substitute(family: &str) -> Option<Substitute> {
    let key = family.trim().to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }
    Some(known_family(&key).unwrap_or_else(|| Substitute {
        family: classify_generic(&key),
        kind: SubstituteKind::Generic,
    }))
}

/// The bundled family for a family name the table knows explicitly: the bundled
/// families themselves, the metric-compatible partners, and common families whose
/// generic class is fixed. `None` for a name to classify heuristically.
fn known_family(key: &str) -> Option<Substitute> {
    let bundled = |family| {
        Some(Substitute {
            family,
            kind: SubstituteKind::Bundled,
        })
    };
    let metric = |family| {
        Some(Substitute {
            family,
            kind: SubstituteKind::MetricCompatible,
        })
    };
    let generic = |family| {
        Some(Substitute {
            family,
            kind: SubstituteKind::Generic,
        })
    };
    match key {
        // The bundled families requested by their own name — used directly.
        "roboto" => bundled(&ROBOTO),
        "caladea" => bundled(&CALADEA),
        "carlito" => bundled(&CARLITO),
        "liberation sans" => bundled(&LIBERATION_SANS),
        "liberation serif" => bundled(&LIBERATION_SERIF),
        "liberation mono" => bundled(&LIBERATION_MONO),
        // Metric-compatible partners: matching advances preserve line breaks.
        "arial" | "arial narrow" | "arial black" | "helvetica" | "helvetica neue"
        | "nimbus sans" | "nimbus sans l" | "arimo" => metric(&LIBERATION_SANS),
        "times new roman" | "times" | "nimbus roman" | "nimbus roman no9 l" | "tinos" => {
            metric(&LIBERATION_SERIF)
        }
        "courier new" | "courier" | "nimbus mono" | "nimbus mono l" | "cousine" => {
            metric(&LIBERATION_MONO)
        }
        "calibri" => metric(&CARLITO),
        "cambria" => metric(&CALADEA),
        // Common families with no metric partner but a fixed generic class, so
        // they need not fall to the substring heuristic (which would misread a
        // name like "Old Standard TT" as sans).
        "ubuntu" | "tahoma" | "verdana" | "segoe" | "segoe ui" | "open sans" | "trebuchet ms"
        | "century gothic" | "sans-serif" | "sans serif" => generic(&LIBERATION_SANS),
        "georgia" | "pt serif" | "old standard tt" | "garamond" | "adobe garamond" | "minion"
        | "minion pro" | "book antiqua" | "palatino" | "palatino linotype" | "constantia"
        | "serif" => generic(&LIBERATION_SERIF),
        "consolas" | "menlo" | "monaco" | "sf mono" | "cascadia code" | "cascadia mono"
        | "andale mono" | "lucida console" | "monospace" => generic(&LIBERATION_MONO),
        _ => None,
    }
}

/// Classifies an unknown (unlisted), already-normalized family key by a
/// generic-family substring, defaulting to sans. `mono` wins first (a "… Mono"
/// face is monospaced regardless of "sans"/"serif" in the name); then `sans`
/// before `serif` so "sans serif" resolves to sans.
fn classify_generic(key: &str) -> &'static BundledFamily {
    if key.contains("mono") {
        &LIBERATION_MONO
    } else if key.contains("sans") {
        &LIBERATION_SANS
    } else if key.contains("serif") {
        &LIBERATION_SERIF
    } else {
        &LIBERATION_SANS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(name: &str) -> (&'static str, SubstituteKind) {
        let s = substitute(name).expect("a named family always substitutes");
        (s.family.name, s.kind)
    }

    #[test]
    fn arial_and_helvetica_map_to_liberation_sans_metric_compatible() {
        assert_eq!(
            sub("Arial"),
            ("Liberation Sans", SubstituteKind::MetricCompatible)
        );
        assert_eq!(
            sub("Helvetica"),
            ("Liberation Sans", SubstituteKind::MetricCompatible)
        );
        assert_eq!(sub("Arial Narrow").0, "Liberation Sans");
    }

    #[test]
    fn times_maps_to_liberation_serif_metric_compatible() {
        assert_eq!(
            sub("Times New Roman"),
            ("Liberation Serif", SubstituteKind::MetricCompatible)
        );
        assert_eq!(sub("Times").0, "Liberation Serif");
    }

    #[test]
    fn courier_maps_to_liberation_mono_metric_compatible() {
        assert_eq!(
            sub("Courier New"),
            ("Liberation Mono", SubstituteKind::MetricCompatible)
        );
        assert_eq!(sub("Courier").0, "Liberation Mono");
    }

    #[test]
    fn calibri_and_cambria_keep_their_bundled_partners() {
        assert_eq!(
            sub("Calibri"),
            ("Carlito", SubstituteKind::MetricCompatible)
        );
        assert_eq!(
            sub("Cambria"),
            ("Caladea", SubstituteKind::MetricCompatible)
        );
    }

    #[test]
    fn bundled_families_requested_by_name_are_used_directly() {
        assert_eq!(sub("Roboto"), ("Roboto", SubstituteKind::Bundled));
        assert_eq!(
            sub("Liberation Sans"),
            ("Liberation Sans", SubstituteKind::Bundled)
        );
        assert_eq!(sub("Liberation Serif").1, SubstituteKind::Bundled);
    }

    #[test]
    fn unknown_sans_defaults_to_liberation_sans() {
        // The demo's Ubuntu (listed sans) and any unclassified missing font.
        assert_eq!(sub("Ubuntu"), ("Liberation Sans", SubstituteKind::Generic));
        assert_eq!(
            sub("Totally Made Up Font"),
            ("Liberation Sans", SubstituteKind::Generic)
        );
    }

    #[test]
    fn known_serif_names_map_to_liberation_serif() {
        // Class-notes families with no metric partner, classified as serif.
        assert_eq!(sub("PT Serif").0, "Liberation Serif");
        assert_eq!(sub("Old Standard TT").0, "Liberation Serif");
        assert_eq!(sub("Georgia").0, "Liberation Serif");
    }

    #[test]
    fn unknown_serif_by_substring_maps_to_liberation_serif() {
        assert_eq!(sub("DejaVu Serif").0, "Liberation Serif");
        assert_eq!(sub("Some Unlisted Serif").0, "Liberation Serif");
    }

    #[test]
    fn unknown_mono_by_substring_maps_to_liberation_mono() {
        assert_eq!(sub("Cascadia Mono").0, "Liberation Mono");
        assert_eq!(sub("PT Sans Mono").0, "Liberation Mono"); // mono wins over sans
        assert_eq!(sub("Fira Code Mono").0, "Liberation Mono");
    }

    #[test]
    fn css_generic_families_map_to_their_liberation_face() {
        assert_eq!(sub("serif").0, "Liberation Serif");
        assert_eq!(sub("sans-serif").0, "Liberation Sans");
        assert_eq!(sub("monospace").0, "Liberation Mono");
    }

    #[test]
    fn matching_is_case_and_whitespace_insensitive() {
        assert_eq!(sub("  ARIAL  ").0, "Liberation Sans");
        assert_eq!(sub("times new roman").0, "Liberation Serif");
    }

    #[test]
    fn a_blank_name_has_no_substitute() {
        assert!(substitute("").is_none());
        assert!(substitute("   ").is_none());
    }
}
