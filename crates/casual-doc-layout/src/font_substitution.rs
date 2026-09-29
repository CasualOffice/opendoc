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
//! sans-serif (the default) → Liberation Sans. Three signals are consulted, in
//! this order:
//!
//! 1. the known-family name list plus the CSS generic names (`serif` /
//!    `sans-serif` / `monospace`), which also carries the metric partners;
//! 2. **the kind the document itself declares** for the face in
//!    `word/fontTable.xml` (`<w:family w:val="roman|swiss|modern|…"/>`,
//!    [`casual_doc_model::v1::FontFamilyKind`]) — see [`GenericFamily`];
//! 3. the `mono` / `sans` / `serif` substring heuristic on the name.
//!
//! Signal 2 is why this module exists in its current shape. The owner's NDA sets
//! its `Normal` style in **Bookman Old Style** 12pt, a serif face that is not
//! installed, is not in the name list, and whose name does not contain the
//! substring `serif` — so it fell through to the sans-serif default. LibreOffice
//! substitutes Times New Roman (metric-partner of the bundled Liberation Serif);
//! a sans face at the same size sets wider, one paragraph wrapped to four lines
//! instead of three, the following heading moved to the next page and the
//! document never recovered — 16 pages against LibreOffice's 15. The font table
//! declared `<w:family w:val="roman"/>` for that face all along; import parsed it
//! into [`casual_doc_model::v1::FontDescriptor::family`] and layout never read
//! it. Classifying by what the DOCUMENT says a face is beats guessing from a
//! substring of its name, and it fixes the family of defects rather than the one
//! name: no entry for `bookman old style` is needed, and none was added.
//!
//! This keeps a missing font on a face with plausible Latin metrics rather than
//! an arbitrary default.
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
//! the same bundled family. Both are handed the SAME declared kind — the resolver
//! through [`crate::resolve::FaceRequest::declared`], the shaper through
//! [`crate::text::StyledRun::requested_family_kind`], both filled from one
//! [`DeclaredFamilies`] index built per layout — because a kind that reached only
//! one of them would make the shaped and the outlined face diverge, which is a
//! worse defect than the one being fixed. An *installed* requested face (real OS Arial under
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

use std::cmp::Ordering;

use casual_doc_model::v1::FontDescriptor;
use casual_doc_model::v1::FontFamilyKind;

use crate::fonts::{
    BundledFamily, CALADEA, CARLITO, LIBERATION_MONO, LIBERATION_SANS, LIBERATION_SERIF, ROBOTO,
};

/// The generic class of a face — the only thing whole-face substitution can act
/// on once a name has no metric partner.
///
/// This is the layout-side reading of the kind a document declares for a face in
/// `word/fontTable.xml` (`<w:family w:val="…"/>`,
/// [`casual_doc_model::v1::FontFamilyKind`]). It is deliberately a separate,
/// smaller type: the OOXML enum carries two values that name no generic class at
/// all, and substitution must not be able to pretend otherwise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenericFamily {
    /// A serif face (`roman`) — Liberation Serif, metric partner of Times New
    /// Roman, which is what LibreOffice substitutes for a missing serif.
    Serif,
    /// A sans-serif face (`swiss`) — Liberation Sans, metric partner of Arial.
    SansSerif,
    /// A fixed-pitch face (`modern`) — Liberation Mono, metric partner of
    /// Courier New.
    Monospace,
}

impl GenericFamily {
    /// The generic class an OOXML `w:family` value names, or `None` when it names
    /// none.
    ///
    /// - `roman` / `swiss` / `modern` map straight across.
    /// - `auto` means *unspecified* in the schema: it carries no signal, so it
    ///   must not displace the name heuristic.
    /// - `script` (handwriting) and `decorative` (display/ornamental) name a
    ///   visual genre, not a text-metric class. There is no bundled counterpart
    ///   for either, and calling one "serif" or "sans" would be an invention —
    ///   an assertion about advances the document never made. They therefore
    ///   yield `None` and fall through to the name heuristic, which can still
    ///   recognise a `… Mono` or `… Serif` in the name. This is a deliberate
    ///   difference from Word, which has faces for these; we report the
    ///   substitution instead of guessing well.
    ///
    /// `w:pitch="fixed"` is *not* consumed as a fourth signal. It is a plausible
    /// one, but a misdeclared pitch on a proportional face would force every run
    /// onto a monospace face — a far larger visible error than the sans/serif
    /// swap this fixes — and `w:family="modern"` already covers the honest case.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub const fn from_declared(kind: FontFamilyKind) -> Option<Self> {
        match kind {
            FontFamilyKind::Roman => Some(Self::Serif),
            FontFamilyKind::Swiss => Some(Self::SansSerif),
            FontFamilyKind::Modern => Some(Self::Monospace),
            FontFamilyKind::Auto | FontFamilyKind::Script | FontFamilyKind::Decorative => None,
        }
    }

    /// The bundled family this generic class substitutes with.
    #[must_use]
    pub const fn bundled(self) -> &'static BundledFamily {
        match self {
            Self::Serif => &LIBERATION_SERIF,
            Self::SansSerif => &LIBERATION_SANS,
            Self::Monospace => &LIBERATION_MONO,
        }
    }
}

/// The generic class each face in a document's `word/fontTable.xml` declares,
/// indexed by family name.
///
/// Built **once per layout** from [`casual_doc_model::v1::Definitions::font_table`]
/// and then queried once per run, by both substitution consumers. Keys are stored
/// pre-lowercased and sorted; lookup is a case-insensitive binary search that
/// allocates nothing, so it can sit on the layout path.
///
/// Complexity: O(f log f) to build and O(log f) per lookup, over the *font table*
/// (tens of entries), with no allocation per lookup and nothing per glyph.
#[derive(Clone, Debug, Default)]
pub struct DeclaredFamilies {
    /// `(lowercased family name, declared class)`, sorted by the name.
    kinds: Vec<(Box<str>, GenericFamily)>,
}

impl DeclaredFamilies {
    /// Indexes the faces in a document's font table that declare a generic class.
    /// Faces declaring `auto`, `script`, `decorative` or nothing at all are left
    /// out, so a lookup misses exactly when the document said nothing useful.
    #[must_use]
    pub fn from_font_table(table: &[FontDescriptor]) -> Self {
        let mut kinds: Vec<(Box<str>, GenericFamily)> = table
            .iter()
            .filter_map(|font| {
                let generic = GenericFamily::from_declared(font.family?)?;
                let key = font.name.trim().to_ascii_lowercase();
                (!key.is_empty()).then(|| (key.into_boxed_str(), generic))
            })
            .collect();
        kinds.sort_by(|(a, _), (b, _)| a.cmp(b));
        kinds.dedup_by(|(a, _), (b, _)| a == b);
        Self { kinds }
    }

    /// Whether the document declared a usable class for any face.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// The class the document declared for `family`, if any. Case- and
    /// surrounding-whitespace-insensitive, matching [`substitute`].
    ///
    /// Complexity: O(log f) comparisons over the font table, no allocation. This
    /// runs once per run on the layout path, never per glyph, so the per-run cost
    /// must not become linear in the font table — `search` counts its comparisons
    /// so a test can prove that by doubling the table rather than by timing it.
    #[must_use]
    pub fn kind_of(&self, family: &str) -> Option<GenericFamily> {
        self.search(family, &mut 0)
    }

    /// [`kind_of`](Self::kind_of), reporting the number of name comparisons it
    /// took. The count is the measurable form of the complexity claim: a binary
    /// search over `f` entries answers in `⌊log2 f⌋ + 1` comparisons, so doubling
    /// the table costs one more — a linear scan would cost `f`.
    fn search(&self, family: &str, comparisons: &mut usize) -> Option<GenericFamily> {
        let name = family.trim();
        self.kinds
            .binary_search_by(|(key, _)| {
                *comparisons += 1;
                ascii_lowercase_cmp(key, name)
            })
            .ok()
            .map(|at| self.kinds[at].1)
    }
}

/// Orders two names by their ASCII-lowercased bytes without allocating, so a
/// pre-lowercased key can be binary-searched against a verbatim request.
fn ascii_lowercase_cmp(key: &str, request: &str) -> Ordering {
    key.bytes()
        .map(|b| b.to_ascii_lowercase())
        .cmp(request.bytes().map(|b| b.to_ascii_lowercase()))
}

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
    /// (serif/sans/mono) — from the class the document declared for it, else from
    /// its name. Layout may shift relative to the true font, so this is reported
    /// as a [`crate::resolve::Disposition::Fallback`] like any other loss: a
    /// substitution driven by the declared class is a better guess, not a
    /// correct answer, and must not go silent.
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
/// `declared` is the class the *document* gave this face in its font table
/// (`<w:family w:val="…"/>`, looked up through [`DeclaredFamilies::kind_of`]);
/// pass `None` where no font table is in scope.
///
/// Matching is case- and surrounding-whitespace-insensitive. A named family
/// always yields a substitute, from the first of three signals that answers:
///
/// 1. the known-family table — a bundled family used directly, or a metric
///    partner whose advances match, which is strictly better than any class;
/// 2. `declared` — what the document says the face *is*;
/// 3. the name substring heuristic, defaulting to [`LIBERATION_SANS`].
///
/// A declared class deliberately beats the substring heuristic (a name is a
/// guess, the declaration is data) and deliberately loses to a metric partner (a
/// document that declares Calibri `swiss` is right, but Carlito is still the
/// better answer than Liberation Sans).
///
/// Complexity: O(1) — a `match` over a fixed table plus at most three substring
/// scans of one family name. Nothing here is per glyph.
#[must_use]
pub fn substitute(family: &str, declared: Option<GenericFamily>) -> Option<Substitute> {
    let key = family.trim().to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }
    Some(known_family(&key).unwrap_or_else(|| Substitute {
        family: declared.map_or_else(|| classify_generic(&key), GenericFamily::bundled),
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
        let s = substitute(name, None).expect("a named family always substitutes");
        (s.family.name, s.kind)
    }

    fn declared(name: &str, kind: GenericFamily) -> (&'static str, SubstituteKind) {
        let s = substitute(name, Some(kind)).expect("a named family always substitutes");
        (s.family.name, s.kind)
    }

    fn font(name: &str, family: Option<FontFamilyKind>) -> FontDescriptor {
        FontDescriptor {
            name: name.to_owned(),
            alt_name: None,
            panose1: None,
            charset: None,
            family,
            pitch: None,
            sig: casual_doc_model::v1::FontSig::default(),
            not_true_type: false,
            embedded: casual_doc_model::v1::EmbeddedFontSet::default(),
        }
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
        assert!(substitute("", None).is_none());
        assert!(substitute("   ", None).is_none());
        assert!(substitute("  ", Some(GenericFamily::Serif)).is_none());
    }

    #[test]
    fn a_declared_roman_face_is_serif_even_when_its_name_says_nothing() {
        // The NDA's shape: an uninstalled serif whose name contains no generic
        // substring, so the heuristic alone reads it as sans and sets it wider.
        assert_eq!(sub("Bookman Old Style").0, "Liberation Sans");
        assert_eq!(
            declared("Bookman Old Style", GenericFamily::Serif),
            ("Liberation Serif", SubstituteKind::Generic)
        );
        assert_eq!(
            declared("Bookman Old Style", GenericFamily::Monospace).0,
            "Liberation Mono"
        );
        assert_eq!(
            declared("Bookman Old Style", GenericFamily::SansSerif).0,
            "Liberation Sans"
        );
    }

    #[test]
    fn a_metric_partner_outranks_the_declared_class() {
        // A font table may declare Calibri `swiss`, and be right — but Carlito
        // matches its advances, which is a stronger guarantee than a class.
        assert_eq!(
            declared("Calibri", GenericFamily::SansSerif),
            ("Carlito", SubstituteKind::MetricCompatible)
        );
        assert_eq!(
            declared("Times New Roman", GenericFamily::SansSerif),
            ("Liberation Serif", SubstituteKind::MetricCompatible)
        );
    }

    #[test]
    fn the_declared_class_outranks_the_name_substring() {
        // "Foo Serif" declared swiss is sans: the declaration is data, the
        // substring is a guess.
        assert_eq!(sub("Unlisted Serif").0, "Liberation Serif");
        assert_eq!(
            declared("Unlisted Serif", GenericFamily::SansSerif).0,
            "Liberation Sans"
        );
    }

    #[test]
    fn only_the_three_metric_classes_are_read_off_a_declaration() {
        assert_eq!(
            GenericFamily::from_declared(FontFamilyKind::Roman),
            Some(GenericFamily::Serif)
        );
        assert_eq!(
            GenericFamily::from_declared(FontFamilyKind::Swiss),
            Some(GenericFamily::SansSerif)
        );
        assert_eq!(
            GenericFamily::from_declared(FontFamilyKind::Modern),
            Some(GenericFamily::Monospace)
        );
        // `auto` is "unspecified"; script/decorative name a genre, not metrics.
        for kind in [
            FontFamilyKind::Auto,
            FontFamilyKind::Script,
            FontFamilyKind::Decorative,
        ] {
            assert_eq!(GenericFamily::from_declared(kind), None, "{kind:?}");
        }
    }

    #[test]
    fn the_font_table_index_is_case_insensitive_and_skips_classless_faces() {
        let table = [
            font("Bookman Old Style", Some(FontFamilyKind::Roman)),
            font("Some Display", Some(FontFamilyKind::Decorative)),
            font("Unclassified", None),
            font("Fixed Face", Some(FontFamilyKind::Modern)),
        ];
        let index = DeclaredFamilies::from_font_table(&table);
        assert!(!index.is_empty());
        assert_eq!(
            index.kind_of("bookman old style"),
            Some(GenericFamily::Serif)
        );
        assert_eq!(
            index.kind_of("  BOOKMAN OLD STYLE "),
            Some(GenericFamily::Serif)
        );
        assert_eq!(index.kind_of("Fixed Face"), Some(GenericFamily::Monospace));
        // A face that declares no metric class is absent, so the caller falls
        // through to the name heuristic rather than to a guess.
        assert_eq!(index.kind_of("Some Display"), None);
        assert_eq!(index.kind_of("Unclassified"), None);
        assert_eq!(index.kind_of("Never Declared"), None);
        assert!(DeclaredFamilies::default().is_empty());
        assert_eq!(DeclaredFamilies::default().kind_of("anything"), None);
    }

    /// Complexity is a gate (SKILL §8): this lookup sits on the layout path, once
    /// per run, so its cost must not grow with the size of the document's font
    /// table. Asserted by DOUBLING the table, not by timing: a binary search
    /// answers in one more comparison when the table doubles, a linear scan in
    /// twice as many.
    #[test]
    fn doubling_the_font_table_costs_one_more_comparison_not_twice_as_many() {
        fn comparisons(entries: usize) -> usize {
            let table: Vec<_> = (0..entries)
                .map(|i| font(&format!("Face {i:06}"), Some(FontFamilyKind::Roman)))
                .collect();
            let index = DeclaredFamilies::from_font_table(&table);
            // The worst case over every present key, plus one absent key (a miss
            // costs the same search).
            let mut worst = 0;
            for i in 0..entries {
                let mut count = 0;
                assert!(
                    index.search(&format!("Face {i:06}"), &mut count).is_some(),
                    "entry {i} of {entries} must be found"
                );
                worst = worst.max(count);
            }
            let mut miss = 0;
            assert!(index.search("Absent Face", &mut miss).is_none());
            worst.max(miss)
        }

        let small = comparisons(64);
        let double = comparisons(128);
        assert_eq!(
            double,
            small + 1,
            "doubling a {small}-comparison table must cost exactly one more \
             comparison ({small} -> {double}); a per-run linear scan over the font \
             table would cost tens and would scale with the document"
        );
        // And the absolute cost is nowhere near the table size, which is the
        // property a linear scan would break even if the delta looked small.
        assert!(
            double <= 8,
            "{double} comparisons over 128 entries is not a binary search"
        );
    }

    #[test]
    fn the_font_table_index_binary_search_finds_every_entry() {
        // Sorting plus case-insensitive comparison is easy to get subtly wrong
        // in a way that only loses SOME entries, so every entry is looked up.
        let names = [
            "Zapfino",
            "alpha",
            "Beta",
            "gamma face",
            "AAA",
            "m",
            "Bookman Old Style",
            "zz",
        ];
        let table: Vec<_> = names
            .iter()
            .map(|name| font(name, Some(FontFamilyKind::Roman)))
            .collect();
        let index = DeclaredFamilies::from_font_table(&table);
        for name in names {
            assert_eq!(
                index.kind_of(name),
                Some(GenericFamily::Serif),
                "{name} must be found"
            );
            assert_eq!(
                index.kind_of(&name.to_ascii_uppercase()),
                Some(GenericFamily::Serif)
            );
        }
    }
}
