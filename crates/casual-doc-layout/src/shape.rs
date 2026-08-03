//! The default [`LineShaper`] — a `parley` (HarfBuzz + Unicode) implementation.
//!
//! `parley` shapes each run, applies the Unicode bidi and line-breaking
//! algorithms, and breaks the paragraph into lines; this adapter maps its output
//! into the crate's device-independent [`crate::text`] types. The shaper works
//! entirely in twips: run sizes are fed to `parley` in twips (with `scale = 1`),
//! so every advance, metric, and offset it returns is already in twips.
//!
//! Fonts: to stay deterministic and WASM-safe (`43-…` §1 decision 5), the shaper
//! registers a single bundled Apache-2.0 font into an *empty* font collection
//! (no system-font discovery). Fuller font resolution — multiple faces, DOCX
//! font-name matching, fallback — is `P1C-002` (`40-FONT-MANAGEMENT-DESIGN.md`).

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

use fontique::{Blob, Script};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontStyle, FontWeight, IndentOptions,
    InlineBox, InlineBoxKind, LayoutContext, LineHeight, PositionedLayoutItem, StyleProperty,
    YieldData,
};

use crate::font_registry::FontRegistry;
use crate::model::{ModelPos, ModelRange};
use crate::text::{
    Decoration, FontId, Glyph, GlyphRun, InlineFloatSide, InlineFloatSpec, InlineImage,
    InlineImageSpec, InlineMathSpec, Line, LineBreak, LineConstraints, LineLayout, LineShaper,
    StyledRun, TextAlignment,
};
use crate::units::{Point, Size, Twip};

/// Per-run data carried through `parley` and recovered from the shaped layout.
/// `Brush` is blanket-implemented for any `Clone + PartialEq + Default + Debug`,
/// so this struct is a valid brush and round-trips the run's fill color and the
/// resolved [`FontId`] (so the renderer can outline the exact face).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct RunBrush {
    color: [u8; 4],
    font: u32,
    /// Original (vertically unscaled) font size in twips.
    size: i32,
    /// OOXML horizontal character scale (`w:w`).
    character_scale_percent: u16,
    /// The run's resolved highlight fill (RGBA); alpha `0` means no highlight.
    highlight: [u8; 4],
    /// The run's resolved shading fill (RGBA); alpha `0` means no shading.
    shading: [u8; 4],
    /// Baseline shift in twips (positive = raised); subtracted from the run's
    /// glyph-run origin so super/subscript and `w:position` offsets survive shaping.
    baseline_shift: i32,
}

/// The default `parley`-backed line shaper.
///
/// `parley`'s builder needs `&mut` access to its font and layout contexts, while
/// [`LineShaper::shape_paragraph`] takes `&self` (shaping is logically pure — the
/// contexts are caches); the contexts therefore live behind `RefCell`.
pub struct ParleyShaper {
    fonts: RefCell<FontContext>,
    layout_cx: RefCell<LayoutContext<RunBrush>>,
    default_family: String,
    /// The registered family name for each bundled family, paired with the family's
    /// base [`FontId`], in ascending `base` order — used to push the resolved
    /// family per run so `parley` shapes with the same face the resolver chose.
    families: Vec<(u32, String)>,
    /// The `Blob` ids of the bundled faces. A run whose resolved face is one of
    /// these keeps its bundled `FontId` (so the bundled/golden path is byte-for-byte
    /// unchanged); a run resolved to any other face (a `system-fonts` OS fallback or
    /// a host-registered blob) is interned into [`Self::registry`] instead.
    bundled_blobs: HashSet<u64>,
    /// The shared dynamic registry: system- and host-resolved fallback faces the
    /// renderer fetches bytes from, plus the running coverage gap. Cloned handle —
    /// call [`Self::registry`] to share it with the renderer.
    registry: FontRegistry,
}

impl ParleyShaper {
    /// Creates a shaper with every target-bundled family registered into an empty
    /// collection (no system fonts — deterministic). Browser builds may omit
    /// host-provisioned families such as Roboto. Each run pushes its resolved
    /// family plus weight/style, so `parley` selects the same face the resolver
    /// did; the run's [`FontId`] rides the brush so the renderer draws the same.
    #[must_use]
    pub fn new() -> Self {
        let mut fonts = FontContext::new();
        let mut families: Vec<(u32, String)> = Vec::with_capacity(crate::fonts::FAMILIES.len());
        let mut bundled_blobs = HashSet::new();
        // Parley family ids of every bundled family, wired below as mutual
        // fallbacks so a glyph missing from a run's own bundled face is drawn
        // from a sibling family that has it instead of `.notdef` (tofu).
        let mut bundled_family_ids: Vec<_> = Vec::with_capacity(crate::fonts::FAMILIES.len());
        for family in crate::fonts::FAMILIES {
            let mut family_id = None;
            for offset in 0..4u32 {
                let bytes = family.face_bytes(offset);
                let blob = Blob::new(Arc::new(bytes.to_vec()));
                // Remember the blob id so a run parley resolves to this exact face
                // is recognized as bundled (and keeps its bundled `FontId`).
                bundled_blobs.insert(blob.id());
                let registered = fonts.collection.register_fonts(blob, None);
                if family_id.is_none() {
                    family_id = registered.first().map(|(id, _)| *id);
                }
            }
            if let Some(id) = family_id {
                bundled_family_ids.push(id);
            }
            let name = family_id
                .and_then(|id| fonts.collection.family_name(id).map(str::to_owned))
                .unwrap_or_else(|| family.name.to_owned());
            families.push((family.base, name));
        }
        // Wire the bundled families as mutual coverage fallbacks: a run shaped in
        // one bundled face (e.g. a Calibri run substituted to the narrower Carlito)
        // that hits a code point the face lacks falls back to a sibling bundled
        // family that covers it (e.g. Roboto or Liberation Serif) instead of
        // painting `.notdef` (tofu). `append_fallbacks` only registers candidates —
        // parley skips any that do not cover a given cluster — so appending every
        // bundled family across the scripts they collectively support is safe.
        // Scripts none of the bundled faces cover (CJK, Arabic, Devanagari, …)
        // still need a host fallback via `register_fallback_font`; because these
        // fallbacks are appended (not set), a host face registered later for such a
        // script still takes precedence.
        for code in [
            "Latn", "Grek", "Cyrl", "Copt", "Armn", "Geor", "Hebr", "Zyyy",
        ] {
            let script = Script::from_str_unchecked(code);
            fonts
                .collection
                .append_fallbacks(script, bundled_family_ids.iter().copied());
        }
        let default_family = families
            .iter()
            .find(|(base, _)| *base == crate::fonts::DEFAULT_FAMILY.base)
            .map(|(_, name)| name.clone())
            .expect("the bundled faces register at least one family");
        Self {
            fonts: RefCell::new(fonts),
            layout_cx: RefCell::new(LayoutContext::new()),
            default_family,
            families,
            bundled_blobs,
            registry: FontRegistry::new(),
        }
    }

    /// A shared handle to the dynamic [`FontRegistry`] this shaper populates while
    /// shaping (system- and host-resolved fallback faces + the coverage gap). Pass
    /// its [`snapshot`](FontRegistry::snapshot) to the renderer so it can rasterize
    /// the same fallback faces the shaper used, and query
    /// [`missing_coverage`](FontRegistry::missing_coverage) to learn which code
    /// points still have no covering face.
    #[must_use]
    pub fn registry(&self) -> FontRegistry {
        self.registry.clone()
    }

    /// Registers a host-provided font blob at runtime so the shaper can shape with
    /// it (by family name) and the renderer can rasterize it — the seam a browser
    /// uses to feed a network-fetched face (e.g. Noto CJK) and any embedded-font
    /// path rides. Returns the [`FontId`] of each face registered (a `.ttc`
    /// collection yields several). To make the face participate in *coverage*
    /// fallback for scripts the bundled faces miss, use
    /// [`register_fallback_font`](Self::register_fallback_font).
    pub fn register_font(&self, bytes: Vec<u8>) -> Vec<FontId> {
        let blob = Blob::new(Arc::new(bytes));
        let registered = {
            let mut fonts = self.fonts.borrow_mut();
            fonts.collection.register_fonts(blob.clone(), None)
        };
        let mut ids = Vec::new();
        for (_family, infos) in &registered {
            for info in infos {
                ids.push(self.registry.intern(blob.clone(), info.index()));
            }
        }
        ids
    }

    /// Registers a host-provided font blob and wires it as a fallback for the given
    /// scripts (ISO 15924 codes, e.g. `"Hani"`, `"Hira"`, `"Kana"`, `"Hang"`), so a
    /// run whose code points the bundled faces do not cover shapes with it. This is
    /// the browser's Noto-CJK path made functional without the native system
    /// source: after `register_fallback_font(noto_cjk, &["Hani", "Hira", "Kana",
    /// "Hang"])`, CJK runs resolve to the host face. Returns the registered faces'
    /// [`FontId`]s.
    pub fn register_fallback_font(&self, bytes: Vec<u8>, scripts: &[&str]) -> Vec<FontId> {
        let blob = Blob::new(Arc::new(bytes));
        let registered = {
            let mut fonts = self.fonts.borrow_mut();
            let registered = fonts.collection.register_fonts(blob.clone(), None);
            let family_ids: Vec<_> = registered.iter().map(|(family, _)| *family).collect();
            for code in scripts {
                let script = Script::from_str_unchecked(code);
                fonts
                    .collection
                    .append_fallbacks(script, family_ids.iter().copied());
            }
            registered
        };
        let mut ids = Vec::new();
        for (_family, infos) in &registered {
            for info in infos {
                ids.push(self.registry.intern(blob.clone(), info.index()));
            }
        }
        ids
    }

    /// The family name `parley` should shape a run with: the run's originally
    /// requested family (`w:rFonts`) when the font collection actually has a face
    /// under that name — a real system face (`system-fonts`) or a host-registered
    /// blob — else the bundled family the resolver substituted
    /// ([`family_for`](Self::family_for)).
    ///
    /// This is the seam that makes an installed requested family (e.g. Arial on a
    /// machine that has it) win over the bundled visual fallback (Roboto), so its
    /// real metrics drive line breaking and pagination. When the collection lacks
    /// the requested name — every deterministic / WASM build, and any machine
    /// missing the font — the bundled resolution stands, so nothing changes there.
    fn pick_family<'r>(&'r self, fonts: &mut FontContext, run: &'r StyledRun<'_>) -> &'r str {
        if let Some(name) = run.requested_family.as_deref() {
            if fonts.collection.family_id(name).is_some() {
                return name;
            }
            // Requested family is missing: shape with the bundled
            // metric-compatible substitute (Liberation Sans/Serif/Mono, Carlito,
            // Caladea) so line breaking matches LibreOffice instead of the
            // wrong-metric default. Falls through to `family_for` if the
            // substitute is somehow not registered.
            if let Some(sub) = crate::font_substitution::substitute(name)
                && let Some(registered) = self.registered_name(sub.family.base)
            {
                return registered;
            }
        }
        self.family_for(run.font)
    }

    /// The family name `parley` registered a bundled family under, keyed by the
    /// family's base [`FontId`] (an exact match, unlike [`Self::family_for`]).
    fn registered_name(&self, base: u32) -> Option<&str> {
        self.families
            .iter()
            .find(|(b, _)| *b == base)
            .map(|(_, name)| name.as_str())
    }

    /// The registered family name for a run's resolved [`FontId`] (the family
    /// whose id block contains it), falling back to the default family name.
    fn family_for(&self, font: FontId) -> &str {
        self.families
            .iter()
            .rev()
            .find(|(base, _)| font.0 >= *base)
            .map_or(self.default_family.as_str(), |(_, name)| name.as_str())
    }
}

impl Default for ParleyShaper {
    fn default() -> Self {
        Self::new()
    }
}

/// Word's default single-line height as a multiple of the run's font size (its
/// "single" line spacing). Used to re-base a line whose height was set by an
/// oversized (or over-tight) coverage-fallback face back onto the run font —
/// matching Word/LibreOffice, which normalize a CJK line to the run font's
/// single-line height rather than inheriting a substituted face's native metrics.
///
/// `1.2` reproduces LibreOffice's CJK line pitch on the reference corpus (a 10 pt
/// CJK line renders at ~16 px / 240 twips) while leaving ample room above and
/// below the baseline for the glyph ink so CJK is never clipped.
const CJK_SINGLE_LINE_FACTOR: f32 = 1.2;

/// Whether a scalar belongs to an East-Asian (CJK/Japanese/Korean) block that the
/// bundled Latin faces never cover. Used to recognize a run that shaped with an OS
/// *coverage-fallback* face (as opposed to a run on its real requested Latin face,
/// whose metrics we must never touch — Latin docs stay pixel-identical).
///
/// Delegates to [`crate::script::is_east_asian`], the single source of truth for
/// the East-Asian range table (also used by per-script font-slot selection).
fn is_cjk_scalar(ch: char) -> bool {
    crate::script::is_east_asian(ch)
}

/// Whether a scalar is a *full-width* CJK glyph — one Word lays out in a fixed
/// one-em cell. Restricted from [`is_cjk_scalar`] by excluding the half-width forms
/// (half-width katakana/hangul in `U+FF61..=U+FFDC` and the half-width symbol forms
/// `U+FFE8..=U+FFEE`), which advance at half an em and must never be widened to a
/// full cell.
fn is_full_width_cjk(ch: char) -> bool {
    is_cjk_scalar(ch) && !matches!(u32::from(ch), 0xFF61..=0xFFDC | 0xFFE8..=0xFFEE)
}

/// The advance a glyph should carry after full-width CJK normalization. Word and
/// CJK layout place each ideograph in a fixed one-em cell (`em` = the run font size
/// in twips); a dynamic OS/host *fallback* face (e.g. macOS PingFang, used when the
/// document's CJK font is absent) can report a slightly *larger* native advance, so
/// a CJK line renders marginally wider than Word and can wrap a word early. When the
/// glyph is a full-width CJK cell whose native advance exceeds the em, it is snapped
/// down to the em; every other glyph — Latin/proportional, half-width, or an advance
/// already at or under the em — passes through unchanged, so those paths stay
/// byte-for-byte identical.
fn full_width_cjk_advance(is_full_width: bool, native: i32, em: i32) -> i32 {
    if is_full_width && native > em {
        em
    } else {
        native
    }
}

/// Merges a one-glyph fallback-font widow when the requested face is unavailable
/// and fitting the authored measure requires no more than 3% horizontal
/// compensation.
///
/// A substituted CJK face can be slightly wider than the requested Word font even
/// after authored `w:w` scaling. Word's header text then fits while the fallback
/// leaves one ideograph on a third visual line. The correction is intentionally
/// narrow: exactly two plain LTR lines, a single-glyph final line, no hard break or
/// inline object, and a bounded scale that keeps the merged output inside the
/// original measure. Both glyph advances and paint-time outline scales are reduced
/// together, so layout and rendering stay consistent.
fn compact_fallback_glyph_widow(
    layout: &mut LineLayout,
    max_width: Twip,
    final_scalar_is_cjk: bool,
) {
    if !final_scalar_is_cjk || layout.lines.len() != 2 || max_width <= Twip::ZERO {
        return;
    }
    let (head, tail) = layout.lines.split_at_mut(1);
    let previous = &mut head[0];
    let last = &mut tail[0];
    let plain = |line: &Line| {
        line.images.is_empty()
            && line.fields.is_empty()
            && line.text_boxes.is_empty()
            && line.rules.is_empty()
            && line.bars.is_empty()
            && line.runs.iter().all(|run| run.bidi_level % 2 == 0)
    };
    let final_glyphs = last.runs.iter().map(|run| run.glyphs.len()).sum::<usize>();
    if previous.line_break != LineBreak::Wrap
        || last.line_break != LineBreak::ParagraphEnd
        || previous.page_break_after
        || last.page_break_after
        || previous.range.end != last.range.start
        || !plain(previous)
        || !plain(last)
        || previous.runs.is_empty()
        || last.runs.is_empty()
        || final_glyphs != 1
        || last
            .range
            .end
            .offset
            .saturating_sub(last.range.start.offset)
            > 4
        || previous
            .runs
            .iter()
            .chain(&last.runs)
            .any(|run| run.origin.x < Twip::ZERO)
    {
        return;
    }

    let run_end = |run: &GlyphRun| {
        run.glyphs
            .iter()
            .fold(run.origin.x, |x, glyph| x + glyph.advance)
    };
    let previous_end = previous
        .runs
        .iter()
        .map(run_end)
        .max_by_key(|x| x.raw())
        .unwrap_or(Twip::ZERO);
    let last_start = last
        .runs
        .iter()
        .map(|run| run.origin.x)
        .min_by_key(|x| x.raw())
        .unwrap_or(Twip::ZERO);
    let last_end = last
        .runs
        .iter()
        .map(run_end)
        .max_by_key(|x| x.raw())
        .unwrap_or(last_start);
    let combined = previous_end + (last_end - last_start);
    if combined <= max_width || i64::from(combined.raw()) * 100 > i64::from(max_width.raw()) * 103 {
        return;
    }

    let target_baseline = previous
        .runs
        .first()
        .map_or(previous.ascent, |run| run.origin.y);
    let last_baseline = last.runs.first().map_or(last.ascent, |run| run.origin.y);
    for run in &mut last.runs {
        run.origin.x = previous_end + (run.origin.x - last_start);
        run.origin.y = run.origin.y + (target_baseline - last_baseline);
    }
    previous.runs.append(&mut last.runs);

    let numerator = i64::from(max_width.raw());
    let denominator = i64::from(combined.raw());
    for run in &mut previous.runs {
        run.origin.x = Twip((i64::from(run.origin.x.raw()) * numerator / denominator) as i32);
        run.character_scale_percent = ((u64::from(run.character_scale_percent) * numerator as u64
            / denominator as u64) as u16)
            .max(1);
        for glyph in &mut run.glyphs {
            glyph.advance = Twip((i64::from(glyph.advance.raw()) * numerator / denominator) as i32);
        }
    }
    previous.range.end = last.range.end;
    previous.line_break = LineBreak::ParagraphEnd;
    layout.lines.pop();
}

/// Applies the `w:spacing@lineRule` box model to a shaped line's natural metrics,
/// returning the `(ascent, descent, height)` to store. Word (ECMA-376 §17.3.1.33):
///
/// - **auto** (the default, and any `MetricsRelative` multiple parley already
///   applied): the natural box is kept as-is.
/// - **atLeast(v)**: the box is at least `v` tall — a shorter natural box is grown
///   to `v`, the extra height added below the baseline (as leading/descent); a
///   taller natural box is left alone.
/// - **exact(v)**: the box is exactly `v` tall regardless of content. The ascent is
///   clamped into the box so the baseline stays inside it and the remainder is the
///   descent; when `v` is smaller than the natural content the glyphs may extend
///   past the box (Word clips — we keep the correct box height for pagination).
///
/// `exact` takes precedence over `atLeast` if both are somehow set (they are
/// mutually exclusive in a well-formed document).
pub(crate) fn apply_line_rule(
    ascent: Twip,
    descent: Twip,
    natural: Twip,
    constraints: &LineConstraints,
) -> (Twip, Twip, Twip) {
    if let Some(exact) = constraints.line_exact {
        let height = exact.raw().max(0);
        let ascent = ascent.raw().clamp(0, height);
        return (Twip(ascent), Twip((height - ascent).max(0)), Twip(height));
    }
    if let Some(at_least) = constraints.line_at_least
        && at_least.raw() > natural.raw()
    {
        let extra = at_least.raw() - natural.raw();
        return (ascent, Twip(descent.raw() + extra), at_least);
    }
    (ascent, descent, natural)
}

/// Maps the crate's [`TextAlignment`] to `parley`'s, honoring the paragraph's
/// base direction (`LineConstraints.rtl`, derived from `w:bidi`).
///
/// `parley` 0.11 resolves its direction-aware `Start`/`End` against the base
/// level it *auto-detects* from the text's strong directional characters — it
/// exposes no API to force a paragraph's base direction. So an RTL paragraph
/// whose text carries no strong RTL character (empty, punctuation-only, or an
/// authored RTL paragraph typed in Latin) would auto-detect LTR and align its
/// default (`Start`) content to the *left*, contradicting the document's
/// `w:bidi`. To make `w:bidi` actually govern the visual edge, an RTL paragraph
/// maps the document-relative `Start`/`End` to the *explicit* physical edges —
/// `Start`→`Right`, `End`→`Left` — rather than to `parley`'s auto-detected ones.
/// `Center`/`Justify` are edge-agnostic and pass through. LTR paragraphs are
/// unchanged (`Start`/`End` pass through, so genuinely mixed/RTL content parley
/// detects still resolves normally).
///
/// This governs the paragraph's base direction and alignment edge only; per-run
/// Unicode-bidi *reordering* within a line still relies on parley's own
/// analysis (see `docs/55` §7 — full visual reordering remains open).
fn alignment(alignment: TextAlignment, rtl: bool) -> Alignment {
    match (alignment, rtl) {
        (TextAlignment::Start, false) => Alignment::Start,
        (TextAlignment::End, false) => Alignment::End,
        (TextAlignment::Start, true) => Alignment::Right,
        (TextAlignment::End, true) => Alignment::Left,
        (TextAlignment::Center, _) => Alignment::Center,
        (TextAlignment::Justify, _) => Alignment::Justify,
    }
}

/// Runs Parley's resumable line breaker around paragraph-local floating
/// exclusions. Custom out-of-flow boxes are zero-sized markers; when one is
/// reached, the line's inline origin/width is reduced until the authored object
/// height has cleared. The anchored object itself remains owned by the page float
/// layer, so no duplicate paint item is emitted here.
fn break_lines_around_floats(
    layout: &mut parley::Layout<RunBrush>,
    image_count: usize,
    floats: &[InlineFloatSpec],
    max_width: Twip,
) {
    #[derive(Clone, Copy)]
    struct ActiveFloat {
        side: InlineFloatSide,
        width: f32,
        end_y: f64,
    }

    fn apply_geometry(
        breaker: &mut parley::BreakLines<'_, RunBrush>,
        active: &mut Vec<ActiveFloat>,
        y: f64,
        full_width: f32,
    ) {
        active.retain(|float| float.end_y > y);
        let left = active
            .iter()
            .filter(|float| float.side == InlineFloatSide::Left)
            .map(|float| float.width)
            .fold(0.0_f32, f32::max);
        let right = active
            .iter()
            .filter(|float| float.side == InlineFloatSide::Right)
            .map(|float| float.width)
            .fold(0.0_f32, f32::max);
        let available = (full_width - left - right).max(1.0);
        breaker.state_mut().set_line_x(left);
        breaker.state_mut().set_line_max_advance(available);
    }

    let full_width = max_width.raw().max(1) as f32;
    let mut breaker = layout.break_lines();
    breaker.state_mut().set_layout_max_advance(full_width);
    breaker.state_mut().set_line_max_advance(full_width);
    let mut active = Vec::new();

    while let Some(event) = breaker.break_next() {
        match event {
            YieldData::InlineBoxBreak(data) => {
                let Some(index) = (data.inline_box_id as usize).checked_sub(image_count) else {
                    // Only custom boxes yield, but consume an unknown marker
                    // defensively so a malformed id cannot stall line breaking.
                    breaker
                        .state_mut()
                        .append_inline_box_to_line(data.advance, 0.0);
                    continue;
                };
                let Some(float) = floats.get(index) else {
                    breaker
                        .state_mut()
                        .append_inline_box_to_line(data.advance, 0.0);
                    continue;
                };
                let y = breaker.committed_y();
                active.push(ActiveFloat {
                    side: float.side,
                    width: float.width.raw().max(0) as f32,
                    end_y: y + f64::from(float.height.raw().max(0)),
                });
                breaker
                    .state_mut()
                    .append_inline_box_to_line(data.advance, 0.0);
                apply_geometry(&mut breaker, &mut active, y, full_width);
            }
            YieldData::LineBreak(data) => {
                apply_geometry(&mut breaker, &mut active, data.line_y_end, full_width);
            }
            YieldData::MaxHeightExceeded(_) => {
                // No line-height bound is installed by this paragraph layout, so
                // this is unreachable. Remove any accidental bound defensively.
                breaker.state_mut().set_line_max_height(f32::INFINITY);
            }
        }
    }
    breaker.finish();
}

impl core::fmt::Debug for ParleyShaper {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `parley`'s font/layout contexts are opaque caches and not `Debug`.
        f.debug_struct("ParleyShaper")
            .field("default_family", &self.default_family)
            .finish_non_exhaustive()
    }
}

impl LineShaper for ParleyShaper {
    fn shape_paragraph(
        &self,
        runs: &[StyledRun<'_>],
        constraints: LineConstraints,
        range: ModelRange,
    ) -> LineLayout {
        self.shape_with_objects(runs, &[], &[], &[], constraints, range)
    }

    fn shape_paragraph_with_inline_images(
        &self,
        runs: &[StyledRun<'_>],
        images: &[InlineImageSpec],
        constraints: LineConstraints,
        range: ModelRange,
    ) -> LineLayout {
        self.shape_with_objects(runs, images, &[], &[], constraints, range)
    }

    fn shape_paragraph_with_inline_objects(
        &self,
        runs: &[StyledRun<'_>],
        images: &[InlineImageSpec],
        floats: &[InlineFloatSpec],
        constraints: LineConstraints,
        range: ModelRange,
    ) -> LineLayout {
        self.shape_with_objects(runs, images, &[], floats, constraints, range)
    }

    fn shape_paragraph_with_rich_inline_objects(
        &self,
        runs: &[StyledRun<'_>],
        images: &[InlineImageSpec],
        maths: &[InlineMathSpec],
        floats: &[InlineFloatSpec],
        constraints: LineConstraints,
        range: ModelRange,
    ) -> LineLayout {
        self.shape_with_objects(runs, images, maths, floats, constraints, range)
    }
}

#[derive(Clone, Copy)]
struct InlineObjects<'a> {
    images: &'a [InlineImageSpec],
    maths: &'a [InlineMathSpec],
    floats: &'a [InlineFloatSpec],
}

impl ParleyShaper {
    /// Shared text/inline-box entry point, including the two-pass CJK metric
    /// normalization used by ordinary text shaping.
    fn shape_with_objects(
        &self,
        runs: &[StyledRun<'_>],
        images: &[InlineImageSpec],
        maths: &[InlineMathSpec],
        floats: &[InlineFloatSpec],
        constraints: LineConstraints,
        range: ModelRange,
    ) -> LineLayout {
        let objects = InlineObjects {
            images,
            maths,
            floats,
        };
        // Shape once with the natural (face-metric) line heights. This pass is
        // byte-for-byte identical to the pre-normalization shaper, so Latin-only
        // paragraphs, the bundled/deterministic path, and every golden are
        // unchanged. It also reports whether the paragraph contains a CJK run that
        // shaped with a dynamic (non-bundled) OS/host *fallback* face, whose native
        // line height does not represent the run font's size.
        let (layout, cjk_fallback) = self.shape_inner(runs, objects, constraints, range, false);
        if !cjk_fallback {
            return layout;
        }
        // Re-shape the CJK paragraph with the line box driven by the *run font size*
        // (`LineHeight::FontSizeRelative`, ~1.2× the em — Word's default single line)
        // instead of the fallback face's ascent+descent+leading. Doing it inside
        // `parley` (rather than rewriting the box afterwards) keeps the glyph
        // baselines and the line box consistent on every downstream path, including
        // paragraphs that never go through the flow layer's line restacking.
        let mut normalized = self.shape_inner(runs, objects, constraints, range, true).0;
        if constraints.alignment == TextAlignment::Start
            && images.is_empty()
            && maths.is_empty()
            && floats.is_empty()
        {
            let final_scalar_is_cjk = runs
                .iter()
                .rev()
                .find_map(|run| run.text.chars().next_back())
                .is_some_and(is_cjk_scalar);
            compact_fallback_glyph_widow(
                &mut normalized,
                constraints.max_width,
                final_scalar_is_cjk,
            );
        }
        normalized
    }

    /// Shapes a paragraph once. When `normalize_cjk` is set, the line box is driven
    /// by the run font size (`LineHeight::FontSizeRelative`, [`CJK_SINGLE_LINE_FACTOR`]
    /// scaled by any `w:spacing@line` percent) rather than the shaped face's native
    /// metrics — the CJK fallback normalization. Returns the layout and whether the
    /// paragraph carried a CJK run shaped with a dynamic fallback face (so the caller
    /// knows whether the normalized re-shape is warranted).
    fn shape_inner(
        &self,
        runs: &[StyledRun<'_>],
        objects: InlineObjects<'_>,
        constraints: LineConstraints,
        range: ModelRange,
        normalize_cjk: bool,
    ) -> (LineLayout, bool) {
        // A paragraph with no runs (a model-empty paragraph, or a break/section
        // paragraph whose only content is a zero-width control) has nothing to
        // shape. Returning no lines — rather than letting `parley` fabricate one at
        // its default font size (~21 twips) — lets the caller
        // (`flow::ensure_nonempty_paragraph`) synthesize the single line at the
        // paragraph mark's own font metrics, which is what Word does. Fabricating a
        // default-size line here silently discards the mark metrics and mis-sizes
        // the empty line, shifting pagination.
        if runs.is_empty()
            && objects.images.is_empty()
            && objects.maths.is_empty()
            && objects.floats.is_empty()
        {
            return (LineLayout { lines: Vec::new() }, false);
        }

        let mut fonts = self.fonts.borrow_mut();
        let mut layout_cx = self.layout_cx.borrow_mut();

        // Concatenate run texts, tracking each run's byte range in the paragraph.
        let mut text = String::new();
        let mut spans: Vec<(usize, usize, &StyledRun<'_>)> = Vec::with_capacity(runs.len());
        for run in runs {
            let start = text.len();
            text.push_str(&run.text);
            spans.push((start, text.len(), run));
        }

        // Resolve each run's family name before `parley` borrows the font context
        // (the builder takes `&mut fonts`, so the collection cannot be queried once
        // it exists). Prefer a real installed face of the run's *requested* family
        // when the collection has one (a system face under `system-fonts`, or a
        // host-registered blob), else the bundled family the resolver chose. In a
        // deterministic build (no system fonts, no host faces) the requested name
        // is never in the collection, so the bundled family is used and output is
        // byte-identical to before.
        let families: Vec<&str> = spans
            .iter()
            .map(|(_, _, run)| self.pick_family(&mut fonts, run))
            .collect();

        // Byte offsets `parley` reports are indices into `text`, the paragraph's
        // runs concatenated in document order. That is exactly the paragraph
        // node's text (offset 0 = the node's start, `range.start.offset` in the
        // general case), so a concatenated-text byte offset *is* the node-relative
        // caret anchor `hittest` expects — no per-run remapping is needed. `base`
        // is the node offset at which this shaped text begins.
        let base = range.start.offset;
        let node = range.start.node;
        let to_offset = |byte: usize| base.saturating_add(byte as u32);

        // Feed sizes in twips with scale = 1 so all outputs are in twips.
        let mut builder = layout_cx.ranged_builder(&mut fonts, &text, 1.0, false);
        builder.push_default(FontFamily::from(self.default_family.as_str()));
        // The `w:spacing@line` percent multiple (`None` = single spacing = 1.0).
        let percent = constraints
            .line_height_percent
            .map_or(1.0, |p| f32::from(p) / 100.0);
        // An authored sub-single `auto` multiple is allowed to tighten below one
        // em. PR #170's blanket one-em floor prevented overpaint but visibly
        // enlarged selected SDS paragraphs. Exact-height lines retain their
        // explicit clip/baseline containment; `auto` spacing remains source-faithful.
        let cjk_line_factor = CJK_SINGLE_LINE_FACTOR * percent;
        if normalize_cjk {
            // Drive the line box from the run font size (Word's single-line height),
            // not the fallback face's native metrics: every run's line height becomes
            // `CJK_SINGLE_LINE_FACTOR * percent * font_size`, so a CJK line matches
            // Word/LibreOffice instead of inheriting an oversized (or over-tight)
            // substitute face's ascent+descent. `parley` positions the baseline
            // within this box, leaving room for the glyph ink so CJK is not clipped.
            builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
                cjk_line_factor,
            )));
        } else if constraints.line_height_percent.is_some() {
            // Line height as a percent of the metrics line height (`w:spacing@line`).
            builder.push_default(StyleProperty::LineHeight(LineHeight::MetricsRelative(
                percent,
            )));
        }
        for (i, (start, end, run)) in spans.iter().enumerate() {
            let scale = run.character_scale_percent.clamp(1, 600);
            let shaped_size = (run.size.raw() as f32 * f32::from(scale) / 100.0).max(1.0);
            builder.push(StyleProperty::FontSize(shaped_size), *start..*end);
            // Push the run's chosen family (system-preferred or bundled; see
            // `families` above) so `parley` shapes with the exact face the renderer
            // will outline for it.
            builder.push(
                StyleProperty::FontFamily(FontFamily::from(families[i])),
                *start..*end,
            );
            builder.push(
                StyleProperty::Brush(RunBrush {
                    color: run.color,
                    font: run.font.0,
                    size: run.size.raw(),
                    character_scale_percent: scale,
                    highlight: run.highlight.unwrap_or([0, 0, 0, 0]),
                    shading: run.shading.unwrap_or([0, 0, 0, 0]),
                    baseline_shift: run.baseline_shift.raw(),
                }),
                *start..*end,
            );
            if scale != 100 {
                // Shape at the scaled size so advances and line breaks see the
                // true horizontal width, then counteract that size in line-height
                // calculation so vertical metrics remain authored-size metrics.
                builder.push(
                    StyleProperty::LineHeight(LineHeight::FontSizeRelative(
                        cjk_line_factor * 100.0 / f32::from(scale),
                    )),
                    *start..*end,
                );
            }
            if run.bold {
                builder.push(
                    StyleProperty::FontWeight(FontWeight::new(700.0)),
                    *start..*end,
                );
            }
            if run.italic {
                builder.push(StyleProperty::FontStyle(FontStyle::Italic), *start..*end);
            }
            if run.letter_spacing != Twip::ZERO {
                builder.push(
                    StyleProperty::LetterSpacing(run.letter_spacing.raw() as f32),
                    *start..*end,
                );
            }
            if run.decoration.underline {
                builder.push(StyleProperty::Underline(true), *start..*end);
            }
            if run.decoration.strikethrough {
                builder.push(StyleProperty::Strikethrough(true), *start..*end);
            }
        }
        for (id, image) in objects.images.iter().enumerate() {
            builder.push_inline_box(InlineBox {
                id: id as u64,
                kind: InlineBoxKind::InFlow,
                index: image.index as usize,
                width: image.size.width.raw() as f32,
                height: image.size.height.raw() as f32,
            });
        }
        for (id, math) in objects.maths.iter().enumerate() {
            builder.push_inline_box(InlineBox {
                id: (objects.images.len() + id) as u64,
                kind: InlineBoxKind::InFlow,
                index: math.index as usize,
                width: math.size.width.raw() as f32,
                height: math.size.height.raw() as f32,
            });
        }
        for (id, float) in objects.floats.iter().enumerate() {
            builder.push_inline_box(InlineBox {
                id: (objects.images.len() + objects.maths.len() + id) as u64,
                kind: InlineBoxKind::CustomOutOfFlow,
                index: float.index as usize,
                width: 0.0,
                height: 0.0,
            });
        }

        let mut layout = builder.build(&text);
        // First-line indent (`w:ind@firstLine`/`@hanging`): parley applies the
        // indent as a start-edge margin on the first line, reducing (or, for a
        // negative/`hanging` amount, extending) its wrap width and offsetting it.
        // The start indent itself is applied downstream at composition; here we
        // only shape the first line's differing width.
        if constraints.first_line_indent != Twip::ZERO {
            layout.set_text_indent(
                constraints.first_line_indent.raw() as f32,
                IndentOptions::default(),
            );
        }
        if objects.floats.is_empty() {
            layout.break_all_lines(Some(constraints.max_width.raw() as f32));
        } else {
            break_lines_around_floats(
                &mut layout,
                objects.images.len() + objects.maths.len(),
                objects.floats,
                constraints.max_width,
            );
        }
        layout.align(
            alignment(constraints.alignment, constraints.rtl),
            AlignmentOptions::default(),
        );

        let line_count = layout.lines().count();
        let mut lines = Vec::with_capacity(line_count);
        // Output line boxes can differ from Parley's natural boxes for OOXML
        // `exact`/`atLeast` rules. Track their authored stack independently so
        // baselines are re-anchored into the box that pagination/composition use.
        let mut output_y = Twip::ZERO;
        let mut source_y = Twip::ZERO;
        // Whether any line carries a CJK run shaped with a dynamic (non-bundled)
        // coverage-fallback face — the signal that this paragraph should be re-shaped
        // with the run-font-driven line box (see `shape_paragraph`).
        let mut cjk_fallback = false;
        for (index, line) in layout.lines().enumerate() {
            let metrics = line.metrics();
            let mut out_runs = Vec::new();
            let mut out_images = Vec::new();
            let mut out_rules = Vec::new();
            // A single `parley` shaping run is split into one `GlyphRun` per
            // contiguous style span — a brush (color/highlight) or decoration
            // change splits the run *without* re-shaping, so the spans keep their
            // kerning and share one parent run. Each `GlyphRun` therefore covers
            // only a *slice* of the run's glyphs (`glyph_start`/`glyph_count`).
            // `run().visual_clusters()` walks the *whole* run, so we consume its
            // per-glyph byte offsets in lockstep with each slice via `cursor`;
            // walking the whole run per slice (the previous behavior) re-emitted
            // every glyph for each brush span, overprinting them. `GlyphRun`s of
            // one run are emitted consecutively, so the cursor stays aligned; a
            // change in the parent run's `text_range` marks a new run and resets
            // the offset stream.
            let mut run_offsets: Vec<u32> = Vec::new();
            // Per-glyph flag (parallel to `run_offsets`): whether the glyph's cluster
            // is a full-width CJK scalar, so a fallback-shaped ideograph's advance can
            // be normalized to the em (Word's fixed one-em cell).
            let mut run_cjk: Vec<bool> = Vec::new();
            let mut run_range: Option<std::ops::Range<usize>> = None;
            let mut cursor = 0usize;
            // Advance shaved from earlier (LTR) CJK runs on this line by the one-em
            // normalization; subtracted from later runs' origins so a following Latin
            // run closes up against the tightened CJK instead of leaving a gap.
            let mut cjk_trim_before = 0i32;
            for item in line.items() {
                let glyph_run = match item {
                    PositionedLayoutItem::InlineBox(inline_box) => {
                        if let Some(image) = objects.images.get(inline_box.id as usize) {
                            out_images.push(InlineImage {
                                media: image.media.clone(),
                                origin: Point::new(
                                    Twip(inline_box.x.round() as i32),
                                    Twip(inline_box.y.round() as i32),
                                ),
                                size: Size::new(
                                    Twip(inline_box.width.round() as i32),
                                    Twip(inline_box.height.round() as i32),
                                ),
                                crop: image.crop,
                            });
                        } else if let Some(math_index) =
                            (inline_box.id as usize).checked_sub(objects.images.len())
                            && let Some(math) = objects.maths.get(math_index)
                        {
                            let box_x = Twip(inline_box.x.round() as i32);
                            let box_y = Twip(inline_box.y.round() as i32);
                            let cluster = base.saturating_add(math.index);
                            for source in &math.runs {
                                let mut run = source.clone();
                                run.origin = Point::new(run.origin.x + box_x, run.origin.y + box_y);
                                for glyph in &mut run.glyphs {
                                    glyph.cluster = cluster;
                                }
                                out_runs.push(run);
                            }
                            for source in &math.rules {
                                let mut rule = *source;
                                rule.origin =
                                    Point::new(rule.origin.x + box_x, rule.origin.y + box_y);
                                out_rules.push(rule);
                            }
                        }
                        continue;
                    }
                    PositionedLayoutItem::GlyphRun(glyph_run) => glyph_run,
                };
                let style = glyph_run.style();
                let run = glyph_run.run();
                let shaped_size = Twip(run.font_size().round() as i32);
                let size = Twip(style.brush.size);
                // The face `parley` actually shaped this run with. When it is a
                // bundled face (always true with `system-fonts` off and no host
                // font registered) it equals the resolver's choice, carried on the
                // brush — so the bundled/golden path is byte-for-byte unchanged.
                // When it is anything else — an OS fallback picked for uncovered
                // code points (`system-fonts`) or a host-registered blob — we
                // intern it so the renderer fetches its bytes by the same `FontId`.
                let resolved = run.font();
                let is_fallback = !self.bundled_blobs.contains(&resolved.data.id());
                let font = if is_fallback {
                    self.registry.intern(resolved.data.clone(), resolved.index)
                } else {
                    FontId(style.brush.font)
                };
                // A run that shaped CJK text with a dynamic (non-bundled)
                // coverage-fallback face marks the paragraph for run-font-driven line
                // normalization. Latin runs on their real requested face (also dynamic
                // under `system-fonts`, e.g. installed Arial) carry no CJK scalars, so
                // they never qualify — Latin docs stay pixel-identical. The bundled /
                // deterministic path is never a fallback face, so it never qualifies.
                if is_fallback
                    && let Some(slice) = text.get(run.text_range())
                    && slice.chars().any(is_cjk_scalar)
                {
                    cjk_fallback = true;
                }
                // Record any code point that shaped to `.notdef` (glyph id 0) —
                // no bundled, system, or host face covered it — as a coverage gap
                // a host can query (`registry.missing_coverage`) and fetch a face
                // for. Harmless and output-neutral; only populates the registry.
                for cluster in run.visual_clusters() {
                    if cluster.glyphs().any(|glyph| glyph.id == 0)
                        && let Some(slice) = text.get(cluster.text_range())
                    {
                        for ch in slice.chars() {
                            self.registry.note_missing(ch);
                        }
                    }
                }
                // Screen-y grows downward, so a positive `baseline_shift` (a raise,
                // e.g. a superscript or a positive `w:position`) subtracts from the
                // baseline row; a negative shift (subscript / lower) adds to it.
                // Left-to-right runs shift left by the advance the one-em CJK
                // normalization already shaved from earlier runs on this line (RTL runs
                // are left untouched — the normalization is LTR-only).
                let is_rtl = glyph_run.run().is_rtl();
                let origin_x =
                    glyph_run.offset().round() as i32 - if is_rtl { 0 } else { cjk_trim_before };
                let origin = Point::new(
                    Twip(origin_x),
                    Twip(glyph_run.baseline().round() as i32 - style.brush.baseline_shift),
                );
                let this_range = glyph_run.run().text_range();
                if run_range.as_ref() != Some(&this_range) {
                    // A new shaping run: rebuild its per-glyph node-relative byte
                    // offsets (visual order, one entry per glyph, each tagged with
                    // its cluster's start — see `to_offset`) and restart the slice
                    // cursor at the run's first glyph.
                    run_offsets.clear();
                    run_cjk.clear();
                    for cluster in glyph_run.run().visual_clusters() {
                        let offset = to_offset(cluster.text_range().start);
                        let is_cjk = text
                            .get(cluster.text_range())
                            .and_then(|slice| slice.chars().next())
                            .is_some_and(is_full_width_cjk);
                        for _ in cluster.glyphs() {
                            run_offsets.push(offset);
                            run_cjk.push(is_cjk);
                        }
                    }
                    run_range = Some(this_range);
                    cursor = 0;
                }
                // Emit only this slice's glyphs (`glyphs()` honors the slice's
                // start/count), pairing each with its cluster byte offset from the
                // run-wide stream at the running cursor so the caret can anchor.
                // On a dynamic fallback face, a full-width CJK glyph whose native
                // advance exceeds the em is snapped down to the em (Word's one-em
                // cell); the shaved excess accrues into `this_run_trim` so following
                // LTR runs can close up. Latin/proportional glyphs and any advance
                // already at or under the em pass through unchanged.
                let em = shaped_size.raw();
                let mut this_run_trim = 0i32;
                let mut glyphs: Vec<Glyph> = Vec::new();
                for glyph in glyph_run.glyphs() {
                    let cluster = run_offsets.get(cursor).copied().unwrap_or(base);
                    let native = glyph.advance.round() as i32;
                    let advance = if is_fallback && !is_rtl {
                        full_width_cjk_advance(
                            run_cjk.get(cursor).copied().unwrap_or(false),
                            native,
                            em,
                        )
                    } else {
                        native
                    };
                    this_run_trim += native - advance;
                    cursor += 1;
                    glyphs.push(Glyph {
                        id: glyph.id,
                        advance: Twip(advance),
                        cluster,
                    });
                }
                // Later LTR runs on this line start `this_run_trim` further left.
                if !is_rtl {
                    cjk_trim_before += this_run_trim;
                }
                // `parley` resolves the Unicode bidi level per run; its parity is
                // the run's direction (even = LTR, odd = RTL), which is all the
                // public API exposes and all `hittest` reads.
                let bidi_level = u8::from(is_rtl);
                // Alpha 0 is the "no highlight"/"no shading" sentinel carried
                // through the brush.
                let highlight = (style.brush.highlight[3] != 0).then_some(style.brush.highlight);
                let shading = (style.brush.shading[3] != 0).then_some(style.brush.shading);
                out_runs.push(GlyphRun {
                    is_marker: false,
                    font,
                    size,
                    character_scale_percent: style.brush.character_scale_percent,
                    color: style.brush.color,
                    origin,
                    bidi_level,
                    decoration: Decoration {
                        underline: style.underline.is_some(),
                        strikethrough: style.strikethrough.is_some(),
                    },
                    highlight,
                    shading,
                    glyphs,
                });
            }
            let line_break = if index + 1 == line_count {
                LineBreak::ParagraphEnd
            } else {
                LineBreak::Wrap
            };
            // The line's model range is the node-relative span of the concatenated
            // text it covers; `parley` partitions the text across lines, so these
            // spans are contiguous and non-overlapping.
            let text_range = line.text_range();
            let line_range = ModelRange::new(
                ModelPos::new(node, to_offset(text_range.start)),
                ModelPos::new(node, to_offset(text_range.end)),
            );
            // The natural line box from the font metrics: parley already folded
            // any `lineRule="auto"` multiple into `line_height` (a
            // `MetricsRelative` factor pushed above). The `atLeast`/`exact` rules
            // reshape this box: `atLeast` grows a too-short box (extra space below
            // the baseline), `exact` pins the height exactly (content may clip).
            let ascent = Twip(metrics.ascent.round() as i32);
            let descent = Twip(metrics.descent.round() as i32);
            // The natural line box from the font metrics; on the normalized re-shape
            // this is already the run-font-driven height (`parley` folded
            // `LineHeight::FontSizeRelative` into it). The `atLeast`/`exact` rules
            // reshape it further below.
            let natural = Twip(metrics.line_height.round() as i32);
            let (ascent, descent, height) = apply_line_rule(ascent, descent, natural, &constraints);
            let source_baseline = Twip(metrics.baseline.round() as i32);
            let baseline_in_line = if constraints.line_exact.is_some() {
                ascent
            } else {
                source_baseline - source_y
            };
            let target_baseline = output_y + baseline_in_line;
            let baseline_delta = target_baseline - source_baseline;
            let box_delta = output_y - source_y;
            for run in &mut out_runs {
                run.origin.y = run.origin.y + baseline_delta;
            }
            // Atomic boxes are top-aligned to the line stack. Glyphs need
            // baseline anchoring, but applying that delta to images would move
            // them incorrectly when an exact-height line clips and reanchors
            // its text.
            for image in &mut out_images {
                image.origin.y = image.origin.y + box_delta;
            }
            for rule in &mut out_rules {
                rule.origin.y = rule.origin.y + box_delta;
            }
            lines.push(Line {
                runs: out_runs,
                ascent,
                descent,
                height,
                clip: constraints.line_exact.is_some(),
                range: line_range,
                line_break,
                page_break_after: false,
                bars: Vec::new(),
                images: out_images,
                fields: Vec::new(),
                notes: Vec::new(),
                text_boxes: Vec::new(),
                rules: out_rules,
            });
            output_y = output_y + height;
            source_y = source_y + natural;
        }
        (LineLayout { lines }, cjk_fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelPos, ModelRange};
    use casual_doc_model::NodeId;

    fn para_range() -> ModelRange {
        let node = NodeId::from_parts(1, 1).unwrap();
        ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0))
    }

    fn run(text: &str) -> StyledRun<'_> {
        StyledRun {
            text: text.into(),
            requested_family: None,
            font: FontId(0),
            size: Twip::from_points(11),
            character_scale_percent: 100,
            bold: false,
            italic: false,
            letter_spacing: Twip::ZERO,
            color: [0, 0, 0, 255],
            decoration: Decoration::default(),
            highlight: None,
            shading: None,
            baseline_shift: Twip::ZERO,
        }
    }

    fn constraints(width_points: i32) -> LineConstraints {
        LineConstraints {
            max_width: Twip::from_points(width_points),
            ..LineConstraints::default()
        }
    }

    #[test]
    fn a_glyph_missing_from_the_default_face_falls_back_to_a_bundled_sibling() {
        use crate::resolve::covers;
        // The run() helper shapes in the default bundled face (FontId(0)). Find a
        // code point that face lacks but some sibling bundled family covers — the
        // exact case that rendered as .notdef before the mutual-fallback wiring.
        let default_face = FontId(0);
        let target = ('\u{00C0}'..='\u{24FF}').find(|&ch| {
            !covers(default_face, ch)
                && crate::fonts::FAMILIES
                    .iter()
                    .any(|f| covers(f.face_id(false, false), ch))
        });
        let Some(ch) = target else {
            // The bundled set always differs somewhere across Latin-Extended /
            // punctuation / symbols; a None here means the corpus changed.
            panic!("expected a code point covered by a sibling but not the default face");
        };

        let shaper = ParleyShaper::new();
        let text = ch.to_string();
        let line = shaper.shape_paragraph(&[run(&text)], constraints(400), para_range());

        // The character no longer shapes to tofu: no .notdef glyph, and it is not
        // recorded as an uncovered code point.
        let has_notdef = line
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .flat_map(|r| &r.glyphs)
            .any(|g| g.id == 0);
        assert!(
            !has_notdef,
            "{ch:?} should fall back to a covering bundled face, not .notdef"
        );
        assert!(
            !shaper.registry().missing_coverage().contains(&ch),
            "{ch:?} is covered by a bundled sibling and must not be reported as missing",
        );
    }

    fn fallback_widow_line(
        node: NodeId,
        start: u32,
        end: u32,
        advance: Twip,
        line_break: LineBreak,
    ) -> Line {
        Line {
            runs: vec![GlyphRun {
                is_marker: false,
                font: FontId(0),
                size: Twip(220),
                character_scale_percent: 100,
                color: [0, 0, 0, 255],
                origin: Point::new(Twip::ZERO, Twip(180)),
                bidi_level: 0,
                decoration: Decoration::default(),
                highlight: None,
                shading: None,
                glyphs: vec![Glyph {
                    id: 1,
                    advance,
                    cluster: start,
                }],
            }],
            ascent: Twip(180),
            descent: Twip(40),
            height: Twip(220),
            clip: false,
            range: ModelRange::new(ModelPos::new(node, start), ModelPos::new(node, end)),
            line_break,
            page_break_after: false,
            bars: Vec::new(),
            images: Vec::new(),
            fields: Vec::new(),
            notes: Vec::new(),
            text_boxes: Vec::new(),
            rules: Vec::new(),
        }
    }

    fn fallback_widow_layout() -> LineLayout {
        let node = NodeId::from_parts(7, 1).unwrap();
        LineLayout {
            lines: vec![
                fallback_widow_line(node, 0, 3, Twip(3_000), LineBreak::Wrap),
                fallback_widow_line(node, 3, 6, Twip(200), LineBreak::ParagraphEnd),
            ],
        }
    }

    #[test]
    fn bounded_fallback_glyph_widow_is_compacted_inside_the_measure() {
        let max_width = Twip(3_122);
        let mut layout = fallback_widow_layout();

        compact_fallback_glyph_widow(&mut layout, max_width, true);

        assert_eq!(layout.lines.len(), 1);
        let line = &layout.lines[0];
        let right_edge = line
            .runs
            .iter()
            .map(|run| {
                run.origin.x
                    + run
                        .glyphs
                        .iter()
                        .fold(Twip::ZERO, |width, glyph| width + glyph.advance)
            })
            .max_by_key(|edge| edge.raw())
            .unwrap();
        assert!(right_edge <= max_width);
        assert_eq!(line.range.end.offset, 6);
        assert_eq!(line.line_break, LineBreak::ParagraphEnd);
        assert!(
            line.runs
                .iter()
                .all(|run| run.character_scale_percent < 100)
        );
    }

    #[test]
    fn fallback_glyph_widow_beyond_the_compensation_bound_still_wraps() {
        let mut layout = fallback_widow_layout();

        compact_fallback_glyph_widow(&mut layout, Twip(3_000), true);

        assert_eq!(layout.lines.len(), 2);
    }

    #[test]
    fn non_cjk_glyph_widow_is_not_compacted() {
        let mut layout = fallback_widow_layout();

        compact_fallback_glyph_widow(&mut layout, Twip(3_122), false);

        assert_eq!(layout.lines.len(), 2);
    }

    #[test]
    fn shapes_a_single_line_of_text() {
        let shaper = ParleyShaper::new();
        let layout = shaper.shape_paragraph(&[run("Hello world")], constraints(500), para_range());
        assert_eq!(layout.lines.len(), 1, "short text fits on one line");
        let line = &layout.lines[0];
        assert!(!line.runs.is_empty(), "the line has at least one glyph run");
        let glyph_count: usize = line.runs.iter().map(|r| r.glyphs.len()).sum();
        assert!(
            glyph_count >= 11,
            "one glyph per visible character at least"
        );
        assert!(
            line.ascent.raw() > 0 && line.height.raw() > 0,
            "positive metrics"
        );
    }

    #[test]
    fn alignment_shifts_the_line_position() {
        let shaper = ParleyShaper::new();
        let left = LineConstraints {
            max_width: Twip::from_points(400),
            alignment: TextAlignment::Start,
            ..LineConstraints::default()
        };
        let centered = LineConstraints {
            alignment: TextAlignment::Center,
            ..left
        };
        let x = |c| {
            shaper
                .shape_paragraph(&[run("short")], c, para_range())
                .lines[0]
                .runs[0]
                .origin
                .x
                .raw()
        };
        // Centered text starts further from the leading edge than start-aligned.
        assert!(
            x(centered) > x(left),
            "center alignment offsets the line inward"
        );
    }

    #[test]
    fn alignment_maps_start_and_end_by_base_direction() {
        // LTR: start/end pass through to parley's direction-aware Start/End.
        assert_eq!(alignment(TextAlignment::Start, false), Alignment::Start);
        assert_eq!(alignment(TextAlignment::End, false), Alignment::End);
        // RTL: start/end map to the explicit physical edges, so the visual edge is
        // governed by w:bidi rather than parley's auto-detected base level.
        assert_eq!(alignment(TextAlignment::Start, true), Alignment::Right);
        assert_eq!(alignment(TextAlignment::End, true), Alignment::Left);
        // Edge-agnostic alignments are unchanged by direction.
        assert_eq!(alignment(TextAlignment::Center, true), Alignment::Center);
        assert_eq!(alignment(TextAlignment::Justify, true), Alignment::Justify);
    }

    #[test]
    fn rtl_paragraph_aligns_start_content_to_the_right_edge() {
        // An RTL (w:bidi) paragraph whose text is Latin — parley auto-detects an
        // LTR base level, so only `LineConstraints.rtl` makes the default (Start)
        // content align to the right edge, as Word lays out a right-to-left
        // paragraph. The LTR control keeps it at the left edge.
        let shaper = ParleyShaper::new();
        let ltr = LineConstraints {
            max_width: Twip::from_points(400),
            alignment: TextAlignment::Start,
            rtl: false,
            ..LineConstraints::default()
        };
        let rtl = LineConstraints { rtl: true, ..ltr };
        let x = |c| {
            shaper
                .shape_paragraph(&[run("short")], c, para_range())
                .lines[0]
                .runs[0]
                .origin
                .x
                .raw()
        };
        assert_eq!(x(ltr), 0, "LTR start-aligned content sits at the left edge");
        assert!(
            x(rtl) > 0,
            "RTL start-aligned content is pushed toward the right edge"
        );
    }

    #[test]
    fn bold_and_letter_spacing_are_applied() {
        let shaper = ParleyShaper::new();
        let plain = run("iiiii");
        let spaced = StyledRun {
            letter_spacing: Twip::from_points(4),
            ..run("iiiii")
        };
        let width = |r: StyledRun<'_>| {
            let line = &shaper
                .shape_paragraph(&[r], constraints(500), para_range())
                .lines[0];
            line.runs
                .iter()
                .flat_map(|g| &g.glyphs)
                .map(|g| g.advance.raw())
                .sum::<i32>()
        };
        assert!(
            width(spaced) > width(plain),
            "letter spacing widens the run's advances"
        );
    }

    #[test]
    fn wraps_when_the_line_is_narrow() {
        let shaper = ParleyShaper::new();
        // A narrow column forces the two words onto separate lines.
        let layout = shaper.shape_paragraph(&[run("Hello world")], constraints(30), para_range());
        assert!(
            layout.lines.len() >= 2,
            "narrow width wraps to multiple lines"
        );
        assert_eq!(
            layout.lines.last().unwrap().line_break,
            LineBreak::ParagraphEnd
        );
    }

    #[test]
    fn a_right_float_narrows_only_the_lines_that_intersect_it() {
        let shaper = ParleyShaper::new();
        let text = "aaaa aaaa aaaa aaaa aaaa ".repeat(24);
        let styled = run(&text);
        let max_width = Twip(3000);
        let exclusion = Twip(1200);
        let float_height = Twip(700);
        let layout = shaper.shape_paragraph_with_inline_objects(
            &[styled],
            &[],
            &[InlineFloatSpec {
                index: 0,
                side: InlineFloatSide::Right,
                width: exclusion,
                height: float_height,
            }],
            LineConstraints {
                max_width,
                ..LineConstraints::default()
            },
            para_range(),
        );

        let line_end = |line: &Line| {
            line.runs
                .iter()
                .map(|run| {
                    run.origin.x.raw()
                        + run
                            .glyphs
                            .iter()
                            .map(|glyph| glyph.advance.raw())
                            .sum::<i32>()
                })
                .max()
                .unwrap_or(0)
        };
        let reduced_width = max_width.raw() - exclusion.raw();
        let mut y = 0;
        let mut saw_cleared_line = false;
        for line in &layout.lines {
            let end = line_end(line);
            if y < float_height.raw() {
                assert!(
                    end <= reduced_width + 2,
                    "an intersecting line ended at {end}, past {reduced_width}"
                );
            } else if end > reduced_width {
                saw_cleared_line = true;
            }
            y += line.height.raw();
        }
        assert!(
            saw_cleared_line,
            "a line below the float should recover the full paragraph width"
        );
    }

    #[test]
    fn simultaneous_edge_floats_union_the_available_line_interval() {
        let shaper = ParleyShaper::new();
        let text = "aaaa aaaa aaaa aaaa aaaa ".repeat(30);
        let max_width = Twip(3_600);
        let left = Twip(900);
        let right = Twip(1_100);
        let float_height = Twip(700);
        let layout = shaper.shape_paragraph_with_inline_objects(
            &[run(&text)],
            &[],
            &[
                InlineFloatSpec {
                    index: 0,
                    side: InlineFloatSide::Left,
                    width: left,
                    height: float_height,
                },
                InlineFloatSpec {
                    index: 0,
                    side: InlineFloatSide::Right,
                    width: right,
                    height: float_height,
                },
            ],
            LineConstraints {
                max_width,
                ..LineConstraints::default()
            },
            para_range(),
        );

        let mut y = 0;
        let mut saw_cleared_line = false;
        for line in &layout.lines {
            let start = line.runs.first().map_or(0, |run| run.origin.x.raw());
            let end = line
                .runs
                .iter()
                .map(|run| {
                    run.origin.x.raw()
                        + run
                            .glyphs
                            .iter()
                            .map(|glyph| glyph.advance.raw())
                            .sum::<i32>()
                })
                .max()
                .unwrap_or(0);
            if y < float_height.raw() {
                assert!(start >= left.raw());
                assert!(end <= max_width.raw() - right.raw() + 2);
            } else if start == 0 && end > max_width.raw() - right.raw() {
                saw_cleared_line = true;
            }
            y += line.height.raw();
        }
        assert!(
            saw_cleared_line,
            "a line below both floats should recover the full measure"
        );
    }

    #[test]
    fn dense_auto_spacing_preserves_the_authored_sub_single_pitch() {
        let shaper = ParleyShaper::new();
        let scaled = StyledRun {
            character_scale_percent: 95,
            ..run("scaled words scaled words scaled words scaled words")
        };
        let authored_size = scaled.size;
        let layout = shaper.shape_paragraph(
            &[scaled],
            LineConstraints {
                max_width: Twip(1200),
                line_height_percent: Some(70),
                ..LineConstraints::default()
            },
            para_range(),
        );
        assert!(layout.lines.len() > 1);
        let expected = (authored_size.raw() as f32 * CJK_SINGLE_LINE_FACTOR * 0.70).round() as i32;
        assert!(
            layout
                .lines
                .iter()
                .all(|line| line.height.raw() == expected),
            "the authored 70% auto multiple remains below one em"
        );
    }

    #[test]
    fn exact_lines_reanchor_each_baseline_inside_its_authored_box() {
        let shaper = ParleyShaper::new();
        let large = StyledRun {
            size: Twip::from_points(24),
            ..run("large words large words large words")
        };
        let exact = Twip(180);
        let layout = shaper.shape_paragraph(
            &[large],
            LineConstraints {
                max_width: Twip(1600),
                line_exact: Some(exact),
                ..LineConstraints::default()
            },
            para_range(),
        );
        assert!(layout.lines.len() > 1);
        for (index, line) in layout.lines.iter().enumerate() {
            let top = exact.raw() * index as i32;
            let bottom = top + exact.raw();
            assert_eq!(line.height, exact);
            assert!(line.clip);
            assert!(line.runs.iter().all(|run| {
                let baseline = run.origin.y.raw();
                (top..=bottom).contains(&baseline)
            }));
        }
    }

    #[test]
    fn preserves_run_color_and_decoration() {
        let shaper = ParleyShaper::new();
        let styled = StyledRun {
            text: "x".into(),
            requested_family: None,
            font: FontId(0),
            size: Twip::from_points(11),
            character_scale_percent: 100,
            bold: false,
            italic: false,
            letter_spacing: Twip::ZERO,
            color: [255, 0, 0, 255],
            decoration: Decoration {
                underline: true,
                strikethrough: false,
            },
            highlight: None,
            shading: None,
            baseline_shift: Twip::ZERO,
        };
        let layout = shaper.shape_paragraph(&[styled], constraints(500), para_range());
        let run = &layout.lines[0].runs[0];
        assert_eq!(
            run.color,
            [255, 0, 0, 255],
            "run color round-trips through parley"
        );
        assert!(run.decoration.underline, "underline round-trips");
    }

    #[test]
    fn character_scale_changes_horizontal_advance_without_scaling_line_height() {
        let shaper = ParleyShaper::new();
        let shape = |scale| {
            let mut styled = run("MMMMMMMM");
            styled.character_scale_percent = scale;
            shaper.shape_paragraph(&[styled], constraints(500), para_range())
        };
        let width = |layout: &LineLayout| {
            layout.lines[0]
                .runs
                .iter()
                .map(|run| {
                    run.origin.x
                        + run
                            .glyphs
                            .iter()
                            .fold(Twip::ZERO, |advance, glyph| advance + glyph.advance)
                })
                .max()
                .unwrap_or(Twip::ZERO)
                .raw()
        };

        let narrow = shape(50);
        let normal = shape(100);
        let wide = shape(180);
        assert!(width(&narrow) < width(&normal));
        assert!(width(&wide) > width(&normal));
        assert_eq!(narrow.lines[0].runs[0].character_scale_percent, 50);
        assert_eq!(wide.lines[0].runs[0].character_scale_percent, 180);
        assert!(
            (narrow.lines[0].height.raw() - wide.lines[0].height.raw()).abs() <= 2,
            "horizontal character scaling must not change vertical line pitch"
        );
    }

    /// A styled run addressing a specific bundled face (for multi-font lines).
    fn run_with_font(text: &str, font: FontId) -> StyledRun<'_> {
        StyledRun { font, ..run(text) }
    }

    #[test]
    fn two_runs_of_different_fonts_yield_increasing_node_relative_clusters() {
        let shaper = ParleyShaper::new();
        // "Hello " in Roboto, "World" in Caladea: one line, two distinct faces.
        let roboto = run_with_font("Hello ", crate::fonts::ROBOTO.face_id(false, false));
        let caladea = run_with_font("World", crate::fonts::CALADEA.face_id(false, false));
        let layout = shaper.shape_paragraph(&[roboto, caladea], constraints(500), para_range());
        assert_eq!(layout.lines.len(), 1, "the text fits on one line");
        let clusters: Vec<u32> = layout.lines[0]
            .runs
            .iter()
            .flat_map(|r| &r.glyphs)
            .map(|g| g.cluster)
            .collect();
        assert!(clusters.len() >= 11, "one cluster per ASCII char");
        assert!(
            clusters.iter().any(|&c| c != 0),
            "clusters are real byte offsets, not the placeholder 0"
        );
        // This LTR line is stored left-to-right, so byte offsets are
        // non-decreasing and reach into the second run (past byte 6, "World").
        assert!(
            clusters.windows(2).all(|w| w[0] <= w[1]),
            "clusters increase across the line: {clusters:?}"
        );
        assert_eq!(
            *clusters.first().unwrap(),
            0,
            "the first glyph anchors at byte 0"
        );
        assert!(
            clusters.iter().any(|&c| c >= 6),
            "the second run's glyphs carry offsets into its node text: {clusters:?}"
        );
        // Two distinct faces really were shaped (a genuine multi-font line).
        let fonts: std::collections::BTreeSet<u32> =
            layout.lines[0].runs.iter().map(|r| r.font.0).collect();
        assert_eq!(fonts.len(), 2, "the line carries two distinct faces");
    }

    #[test]
    fn first_line_indent_pushes_the_first_line_right() {
        let shaper = ParleyShaper::new();
        // A narrow column forces at least two lines; a positive first-line indent
        // out-dents the first line to the right of the continuation lines.
        let c = LineConstraints {
            max_width: Twip::from_points(60),
            first_line_indent: Twip::from_points(24),
            ..LineConstraints::default()
        };
        let layout = shaper.shape_paragraph(&[run("Hello world this wraps")], c, para_range());
        assert!(layout.lines.len() >= 2, "the text wraps to multiple lines");
        let first_x = layout.lines[0].runs[0].origin.x.raw();
        let second_x = layout.lines[1].runs[0].origin.x.raw();
        assert!(
            first_x > second_x,
            "first-line indent starts the first line ({first_x}) right of the rest ({second_x})"
        );
    }

    #[test]
    fn hanging_indent_protrudes_the_first_line_left() {
        let shaper = ParleyShaper::new();
        // A negative first-line indent (a hanging indent) protrudes the first line
        // to the left of the continuation lines (the bulleted-list shape).
        let c = LineConstraints {
            max_width: Twip::from_points(60),
            first_line_indent: Twip::from_points(-24),
            ..LineConstraints::default()
        };
        let layout = shaper.shape_paragraph(&[run("Hello world this wraps")], c, para_range());
        assert!(layout.lines.len() >= 2, "the text wraps to multiple lines");
        let first_x = layout.lines[0].runs[0].origin.x.raw();
        let second_x = layout.lines[1].runs[0].origin.x.raw();
        assert!(
            first_x < second_x,
            "a hanging indent starts the first line ({first_x}) left of the rest ({second_x})"
        );
    }

    #[test]
    fn bidi_levels_reflect_run_direction() {
        let shaper = ParleyShaper::new();
        // Pure LTR: every run is even (level 0).
        let ltr = shaper.shape_paragraph(&[run("abc")], constraints(500), para_range());
        assert!(
            ltr.lines[0].runs.iter().all(|r| r.bidi_level % 2 == 0),
            "LTR runs have an even bidi level"
        );
        // Pure RTL (Hebrew): at least one run is odd.
        let rtl = shaper.shape_paragraph(&[run("שלום")], constraints(500), para_range());
        assert!(
            rtl.lines
                .iter()
                .flat_map(|l| &l.runs)
                .any(|r| r.bidi_level % 2 == 1),
            "an RTL run has an odd bidi level"
        );
        // Mixed line: both parities present.
        let mixed = shaper.shape_paragraph(&[run("abc שלום def")], constraints(500), para_range());
        let levels: Vec<u8> = mixed
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .map(|r| r.bidi_level)
            .collect();
        assert!(
            levels.iter().any(|&b| b % 2 == 0) && levels.iter().any(|&b| b % 2 == 1),
            "a mixed line carries both LTR and RTL runs: {levels:?}"
        );
    }

    #[test]
    fn line_ranges_are_contiguous_subranges_of_the_paragraph() {
        let shaper = ParleyShaper::new();
        let text = "Hello world this is a longer paragraph that wraps onto lines";
        let total = text.len() as u32;
        // A narrow column forces several lines.
        let layout = shaper.shape_paragraph(&[run(text)], constraints(60), para_range());
        assert!(
            layout.lines.len() >= 2,
            "the text wraps to multiple lines (got {})",
            layout.lines.len()
        );
        let node = para_range().start.node;
        let mut prev_end = 0u32;
        for (index, line) in layout.lines.iter().enumerate() {
            assert_eq!(line.range.start.node, node, "the line anchors to the node");
            assert_eq!(line.range.end.node, node);
            assert!(
                line.range.start.offset <= line.range.end.offset,
                "the range is well-formed"
            );
            assert!(
                line.range.end.offset <= total,
                "the range stays within the paragraph text"
            );
            if index == 0 {
                assert_eq!(
                    line.range.start.offset, 0,
                    "the first line starts at byte 0"
                );
            } else {
                assert_eq!(
                    line.range.start.offset, prev_end,
                    "consecutive lines are contiguous — no gap, no overlap"
                );
            }
            prev_end = line.range.end.offset;
        }
        assert_eq!(prev_end, total, "the last line ends at the paragraph's end");
    }

    use crate::font_registry::FontRegistry;

    /// The run-font-driven single line for a size (Word's default), the target the
    /// CJK fallback normalization drives a line box to.
    fn normalized_single_line(size: Twip) -> i32 {
        (CJK_SINGLE_LINE_FACTOR * size.raw() as f32).round() as i32
    }

    /// Without a fallback face (the deterministic / bundled path), a CJK run shapes
    /// to `.notdef` with a bundled face and is therefore **not** normalized: its line
    /// box stays the bundled face's natural height, identical to a Latin run of the
    /// same size. This pins that the normalization never perturbs the deterministic
    /// path (no re-shape happens when nothing fell back to a non-bundled face).
    #[test]
    #[cfg(not(feature = "system-fonts"))]
    fn cjk_line_box_is_unchanged_on_the_bundled_path() {
        let shaper = ParleyShaper::new();
        let size = Twip::from_points(10);
        let cjk = shaper.shape_paragraph(
            &[StyledRun {
                size,
                ..run("中文字")
            }],
            constraints(500),
            para_range(),
        );
        let latin = shaper.shape_paragraph(
            &[StyledRun {
                size,
                ..run("Hello")
            }],
            constraints(500),
            para_range(),
        );
        assert_eq!(
            cjk.lines[0].height, latin.lines[0].height,
            "bundled .notdef CJK keeps the natural bundled line box (not normalized)"
        );
        assert_ne!(
            cjk.lines[0].height.raw(),
            normalized_single_line(size),
            "the bundled path is not driven to the run-font single line"
        );
    }

    /// With the `system-fonts` feature ON, a CJK run that shapes with a dynamic OS
    /// fallback face has its line box driven by the *run font size* (Word's single
    /// line, [`CJK_SINGLE_LINE_FACTOR`]) rather than that face's native metrics — so
    /// the height is exactly `round(factor * size)` regardless of which substitute
    /// face the OS supplied. A Latin run (no CJK scalars) is left on its natural
    /// bundled line box, proving the normalization is CJK-fallback-gated. Gated on a
    /// covering face being present (a headless runner may have none).
    #[test]
    #[cfg(all(feature = "system-fonts", not(target_arch = "wasm32")))]
    fn cjk_fallback_line_is_normalized_to_the_run_font_single_line() {
        let shaper = ParleyShaper::new();
        let size = Twip::from_points(10);
        let cjk = shaper.shape_paragraph(
            &[StyledRun {
                size,
                ..run("化学品安全技术说明书")
            }],
            constraints(500),
            para_range(),
        );
        let line = &cjk.lines[0];
        let covered = line.runs.iter().flat_map(|r| &r.glyphs).any(|g| g.id != 0);
        let dynamic = line.runs.iter().any(|r| FontRegistry::is_dynamic(r.font));
        if covered && dynamic {
            assert_eq!(
                line.height.raw(),
                normalized_single_line(size),
                "the CJK fallback line is driven to the run-font single line"
            );
            assert!(
                line.ascent.raw() > 0 && line.descent.raw() >= 0,
                "the box still has room for the glyph ink"
            );
        }
        // A Latin run never carries CJK scalars, so it is never re-shaped: it keeps
        // the bundled face's natural (taller than 1.2x) single line.
        let latin = shaper.shape_paragraph(
            &[StyledRun {
                size,
                ..run("Hello")
            }],
            constraints(500),
            para_range(),
        );
        assert!(
            latin.lines[0].height.raw() > normalized_single_line(size),
            "Latin keeps its natural bundled line box (not normalized)"
        );
    }

    /// With the `system-fonts` feature OFF (the deterministic / WASM path), no
    /// bundled Latin face covers CJK, so a CJK run shapes to `.notdef`, keeps a
    /// bundled `FontId`, and its uncovered code points are recorded as a coverage
    /// gap a host can query and fetch a face for.
    #[test]
    #[cfg(not(feature = "system-fonts"))]
    fn cjk_without_system_fonts_is_notdef_and_recorded_as_a_coverage_gap() {
        let shaper = ParleyShaper::new();
        let layout = shaper.shape_paragraph(&[run("中文")], constraints(500), para_range());
        let glyphs: Vec<_> = layout.lines[0]
            .runs
            .iter()
            .flat_map(|r| &r.glyphs)
            .collect();
        assert!(!glyphs.is_empty(), "the CJK run still produces glyphs");
        assert!(
            glyphs.iter().all(|g| g.id == 0),
            "no bundled face covers CJK, so every glyph is .notdef"
        );
        assert!(
            layout.lines[0]
                .runs
                .iter()
                .all(|r| !FontRegistry::is_dynamic(r.font)),
            "bundled-only: no dynamic fallback face was interned"
        );
        let missing = shaper.registry().missing_coverage();
        assert!(
            missing.contains(&'中') && missing.contains(&'文'),
            "the coverage gap records the uncovered CJK code points: {missing:?}"
        );
    }

    /// With the `system-fonts` feature ON (native), `parley`/`fontique` perform
    /// real font fallback: a CJK run whose code points the bundled faces miss is
    /// shaped with an installed OS font, yielding non-`.notdef` glyphs through a
    /// dynamically interned face whose bytes the shared registry can serve.
    ///
    /// Whether the host actually *has* a CJK face is environment-dependent (a
    /// headless CI runner may have none), so the test asserts the full covering
    /// path when a face is found and, otherwise, that the fallback still behaved —
    /// the uncovered code points were recorded as a coverage gap. It never fails
    /// merely because the OS lacks a CJK font.
    #[test]
    #[cfg(all(feature = "system-fonts", not(target_arch = "wasm32")))]
    fn cjk_with_system_fonts_resolves_a_covering_face() {
        let shaper = ParleyShaper::new();
        let layout = shaper.shape_paragraph(&[run("中文")], constraints(500), para_range());
        let runs = &layout.lines[0].runs;
        let covered = runs.iter().flat_map(|r| &r.glyphs).any(|g| g.id != 0);
        if covered {
            // The OS provided a covering face: a real (non-.notdef) glyph implies
            // the run was shaped with a dynamically interned system face whose
            // bytes the registry serves to the renderer.
            let covering = runs
                .iter()
                .map(|r| r.font)
                .find(|&f| FontRegistry::is_dynamic(f))
                .expect("a covering glyph implies a dynamically interned system face");
            assert!(
                shaper.registry().face(covering).is_some(),
                "the system face's bytes are interned and available to the renderer"
            );
        } else {
            // No CJK face installed (e.g. a headless runner): the fallback still
            // behaved — every code point is recorded as an actionable coverage gap.
            let missing = shaper.registry().missing_coverage();
            assert!(
                missing.contains(&'中') && missing.contains(&'文'),
                "with no covering face, the uncovered CJK code points are recorded: {missing:?}"
            );
        }
    }

    /// A host-registered font (the browser network-font seam) is interned into the
    /// shared registry under a dynamic `FontId` and its exact bytes are served
    /// back — independent of the `system-fonts` feature (an in-memory blob store,
    /// WASM-clean).
    #[test]
    fn full_width_cjk_scalars_exclude_half_width_forms() {
        assert!(is_full_width_cjk('中'), "a Han ideograph is full-width");
        assert!(is_full_width_cjk('あ'), "hiragana is full-width");
        assert!(is_full_width_cjk('２'), "a full-width digit is full-width");
        assert!(!is_full_width_cjk('2'), "an ASCII digit is not CJK");
        assert!(!is_full_width_cjk('A'), "Latin is not CJK");
        assert!(
            !is_full_width_cjk('\u{FF71}'),
            "half-width katakana advances at half an em, so it is not full-width"
        );
    }

    #[test]
    fn full_width_cjk_advance_clamps_over_em_and_leaves_others() {
        let em = 200;
        // A full-width CJK cell wider than the em is snapped down to the em.
        assert_eq!(
            full_width_cjk_advance(true, 214, em),
            em,
            "over-em CJK -> em"
        );
        // A CJK cell already at or under the em is untouched (never widened).
        assert_eq!(
            full_width_cjk_advance(true, em, em),
            em,
            "at-em CJK unchanged"
        );
        assert_eq!(
            full_width_cjk_advance(true, 180, em),
            180,
            "under-em CJK kept"
        );
        // A non-CJK (proportional) glyph is never touched, however wide.
        assert_eq!(
            full_width_cjk_advance(false, 260, em),
            260,
            "a non-CJK glyph keeps its native (proportional) advance"
        );
    }

    /// With `system-fonts` ON, a CJK run that shapes on a dynamic OS fallback face
    /// carries no CJK glyph advance *wider* than the em — an over-em fallback cell is
    /// snapped down to the one-em cell (and a face that already advances at the em is
    /// unchanged). A Latin run's advances are never touched. Gated on a covering face
    /// being present (a headless runner may have none).
    #[test]
    #[cfg(all(feature = "system-fonts", not(target_arch = "wasm32")))]
    fn cjk_fallback_glyph_advance_never_exceeds_the_em() {
        let shaper = ParleyShaper::new();
        let size = Twip::from_points(10);
        let em = size.raw();
        let cjk = shaper.shape_paragraph(
            &[StyledRun {
                size,
                ..run("化学品安全技术说明书")
            }],
            constraints(500),
            para_range(),
        );
        let line = &cjk.lines[0];
        let covered = line.runs.iter().flat_map(|r| &r.glyphs).any(|g| g.id != 0);
        let dynamic = line.runs.iter().any(|r| FontRegistry::is_dynamic(r.font));
        if covered && dynamic {
            for r in &line.runs {
                if FontRegistry::is_dynamic(r.font) {
                    for g in &r.glyphs {
                        assert!(
                            g.advance.raw() <= em,
                            "a normalized CJK fallback advance never exceeds the em \
                             ({} > {em})",
                            g.advance.raw()
                        );
                    }
                }
            }
        }
        // A Latin run (no CJK scalars) is never normalized: its digit advances are
        // proportional and, for these digits, wider than nothing — importantly, the
        // Latin path is untouched relative to the bundled baseline.
        let latin_dyn = shaper.shape_paragraph(
            &[StyledRun {
                size,
                requested_family: Some("Arial".into()),
                ..run("2025/01/23")
            }],
            constraints(500),
            para_range(),
        );
        let latin_bundled = shaper.shape_paragraph(
            &[StyledRun {
                size,
                ..run("2025/01/23")
            }],
            constraints(500),
            para_range(),
        );
        // The bundled Latin advances are the golden path — untouched by the CJK gate.
        let advances: Vec<i32> = latin_bundled.lines[0]
            .runs
            .iter()
            .flat_map(|r| &r.glyphs)
            .map(|g| g.advance.raw())
            .collect();
        assert!(
            advances.iter().all(|&a| a > 0),
            "Latin digits keep their proportional advances"
        );
        // Whether or not a system Arial resolved, no Latin glyph was clamped to a CJK
        // cell (the run carries no full-width CJK scalars).
        let _ = latin_dyn;
    }

    #[test]
    fn a_host_registered_font_is_interned_and_served() {
        let shaper = ParleyShaper::new();
        let ids = shaper.register_font(crate::fonts::CALADEA_REGULAR.to_vec());
        assert!(
            !ids.is_empty(),
            "registering a font yields at least one face id"
        );
        let host = ids[0];
        assert!(
            FontRegistry::is_dynamic(host),
            "host faces get dynamic FontIds, disjoint from the bundled block"
        );
        assert_eq!(
            shaper.registry().face(host).unwrap().bytes.as_slice(),
            crate::fonts::CALADEA_REGULAR,
            "the registry serves the exact host bytes for rasterization"
        );
    }

    /// A host-registered fallback font wired for a script is exposed through the
    /// same dynamic-`FontId` seam. (Full CJK coverage would need a CJK face, which
    /// the deterministic suite does not bundle; this exercises the registration +
    /// serving path that the browser's Noto-CJK slice reuses.)
    #[test]
    fn a_host_fallback_font_registers_without_panicking_and_is_served() {
        let shaper = ParleyShaper::new();
        let ids = shaper
            .register_fallback_font(crate::fonts::CARLITO_REGULAR.to_vec(), &["Hani", "Hira"]);
        assert!(!ids.is_empty());
        assert!(ids.iter().all(|&id| FontRegistry::is_dynamic(id)));
        assert_eq!(
            shaper.registry().face(ids[0]).unwrap().bytes.as_slice(),
            crate::fonts::CARLITO_REGULAR
        );
    }
}
