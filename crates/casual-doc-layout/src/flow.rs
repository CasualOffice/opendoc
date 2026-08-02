//! The block/flow engine — turning a v1 [`Document`] into a shaped galley.
//!
//! This is the bridge from the semantic model to layout: each body paragraph's
//! inline runs (and their run properties) become [`StyledRun`]s, which the
//! [`LineShaper`] turns into positioned lines, yielding a [`BlockFragment`]. The
//! resulting galley is what the paginator ([`crate::paginate`]) slices into pages
//! and the renderer paints — so this closes the loop from imported DOCX to a
//! rendered page.
//!
//! Scope: body paragraphs (runs with size/color/weight/decoration, recursing
//! through hyperlink/revision/content-control wrappers) and **tables** (rows and
//! nested tables, cells flowed at their grid-column width). Inline drawings,
//! fields, block content controls, indents/tabs (`P1C-003b`), and cross-page
//! table splitting (`P1D-003`) are the following slices; unmapped inline nodes
//! contribute no text yet (never panic).

use std::borrow::Cow;
use std::collections::{BTreeMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Alignment, AltChunk, BlockNode, BorderEdge, BreakKind, Color, ColorScheme, DefinitionMap,
    Definitions, Document, Drawing, DrawingAnchor, DropCapFrame, DropCapMode, EmbeddedKind,
    EmbeddedObject, Extent, FontScheme, FrameWrap, HeightRule, HighlightColor, HorizontalAlign,
    HorizontalAnchor, HorizontalPosition, HorizontalRule as ModelHorizontalRule,
    HorizontalRuleAlign, Indentation, InlineNode, InlineSdt, LevelSuffix, LineRule, MathExpression,
    MediaId, MediaReference, NoteKind, NoteReference, Paragraph, ParagraphProperties,
    ReviewProjection, Rgba, RunFontHint, RunProperties, SchemeColor, SdtControlData,
    SectionBoundary, SectionId, SectionType, ShapeStroke, StyleId, Symbol, TabAlignment, TabLeader,
    TabStop, Table, TableBorders, TableCell, TableLayout, TableRow, TableRowProperties, TextBox,
    TextBoxAutoFit, TextBoxBodyProperties, TextBoxHorizontalOverflow, TextBoxVerticalAnchor,
    TextBoxVerticalOverflow, ThemeColorRef, VerticalAlignment, VerticalAnchor, VerticalMerge,
    VerticalPosition, WrapMode,
};

use crate::block::{
    BlockBorders, BlockFragment, BorderPattern, BoxMetrics, BreakControl, CellBorders,
    CellBoxSpacing, CellContentMargins, CellFragment, CellVAlign, CellVerticalMerge,
    ParagraphDecor, ResolvedBorderSegment, ResolvedEdge,
};
use crate::cascade::{
    StyleCascade, TableStyleLayer, overlay_table_borders, requested_font_family,
    requested_font_family_for, union_cnf,
};
use crate::incremental::{DirtySet, GalleyCache};
use crate::model::{ModelPos, ModelRange};
use crate::numbering::{self, NumberingState, PreparedMarker};
use crate::resolve::{FaceRequest, FontResolutionReport, FontResolver};
use crate::script::{self, ScriptSlot};
use crate::tabs::{self, FlowItem};
use crate::text::{
    Decoration, FieldKind, FieldMarker, FieldStyle, FontId, Glyph, GlyphRun, InlineFloatSide,
    InlineFloatSpec, InlineImage, InlineImageSpec, InlineMathSpec, InlineRule, InlineTextBox, Line,
    LineBreak, LineConstraints, LineLayout, LineShaper, NoteMarker, StyledRun, TextAlignment,
    TextBoxContentLayout, TextBoxStroke,
};
use crate::units::{Point, Size, Twip};

/// One page-derived edge exclusion applied at the start of a body paragraph.
/// These are produced by the bounded cross-paragraph float pass after an initial
/// pagination and consumed only by the next fixed-point iteration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ParagraphFloatExclusion {
    pub(crate) side: InlineFloatSide,
    pub(crate) width: Twip,
    pub(crate) height: Twip,
}

pub(crate) type ParagraphFloatExclusions = BTreeMap<NodeId, Vec<ParagraphFloatExclusion>>;

/// Context threaded through the flow: the font resolver, the document's theme
/// font scheme (needed to turn `w:rFonts@*Theme` slots into concrete families),
/// and the running font-resolution report.
struct FlowCtx<'a> {
    resolver: &'a FontResolver,
    scheme: Option<&'a FontScheme>,
    report: &'a mut FontResolutionReport,
    /// The document's default tab-stop interval (`w:defaultTabStop`), already
    /// resolved to twips (falling back to the 720-twip standard).
    default_tab: Twip,
    /// The document's media table, so an inline drawing's [`MediaId`] resolves to
    /// its package part name (the display list's stable media key).
    media: &'a DefinitionMap<MediaId, MediaReference>,
    /// The document's theme color palette (`a:clrScheme`) resolved to RGBA per
    /// slot, so a run's `w:themeColor` resolves to the real theme color rather than
    /// silently falling back to black. `None` when the document declares no theme
    /// color scheme.
    palette: Option<&'a ResolvedPalette>,
    /// The style-hierarchy view used to resolve *effective* paragraph and run
    /// properties (docDefaults → style chain → direct), so style-driven formatting
    /// — font size, spacing, alignment, color — is honored, not just direct props.
    cascade: StyleCascade<'a>,
    /// The effective paragraph style of the paragraph currently being flowed, set
    /// before its inlines are collected. Threaded so each run's effective
    /// properties include the paragraph style's `rPr` in the cascade.
    para_style: Option<StyleId>,
    /// The resolved table-style layer for the current cell. `None` outside a
    /// table or inside an unstyled nested-table cell.
    table_style: Option<TableStyleLayer>,
    /// The document's ordered section boundaries (`w:sectPr`). A body paragraph
    /// whose `w:pPr` carries a section break ends a section; the *following*
    /// section's start type (`w:type`) decides whether a page break follows it.
    /// Empty for running content (header/footer), where section breaks are absent.
    sections: &'a [SectionBoundary],
    /// The document's definitions, so a paragraph's numbering reference resolves to
    /// its abstract/instance/level (the list marker engine, [`crate::numbering`]).
    definitions: &'a Definitions,
    /// The list-numbering counter state, advanced once per numbered paragraph in
    /// document order (including through tables/SDTs, which recurse this flow). A
    /// measurement (intrinsic-width) context threads a throwaway state so it never
    /// perturbs the real counters.
    numbering: NumberingState,
    /// Cumulative `a:normAutofit@fontScale` applied to runs in the current text
    /// body (`100000` = 100%).
    text_scale: u32,
    /// `a:normAutofit@lnSpcReduction`, in per-100000 units, scoped to the current
    /// text body.
    line_spacing_reduction: u32,
    /// Page-derived exclusions from body floats that intersect top-level body
    /// paragraphs. `None` on the initial pagination and in running content.
    paragraph_float_exclusions: Option<&'a ParagraphFloatExclusions>,
}

/// Builds a galley of block fragments from a document's body, shaped to fit
/// `content_width` (the page content-area width, in twips). The font-resolution
/// report is discarded; call [`build_galley_with_report`] to inspect which
/// families were substituted.
#[must_use]
pub fn build_galley(
    document: &Document,
    shaper: &dyn LineShaper,
    content_width: Twip,
) -> Vec<BlockFragment> {
    build_galley_with_report(document, shaper, content_width).0
}

/// Builds a galley and returns the [`FontResolutionReport`] alongside it: the
/// whole-face substitutions and per-glyph coverage fallbacks performed while
/// resolving each run's declared family (`P1C-002b`), mirroring the importer's
/// compatibility-report pattern so substitution is surfaced, never silent.
#[must_use]
pub fn build_galley_with_report(
    document: &Document,
    shaper: &dyn LineShaper,
    content_width: Twip,
) -> (Vec<BlockFragment>, FontResolutionReport) {
    let resolver = FontResolver::new();
    let mut report = FontResolutionReport::new();
    // Resolve the theme color palette once so every run's `w:themeColor` resolves
    // against the real scheme colors (not a black fallback).
    let palette = document
        .definitions()
        .color_scheme
        .as_ref()
        .map(resolve_palette);
    let mut ctx = FlowCtx {
        resolver: &resolver,
        scheme: document.definitions().font_scheme.as_ref(),
        report: &mut report,
        default_tab: tabs::default_tab_stop(document.definitions().settings.default_tab_stop),
        media: &document.definitions().media,
        palette: palette.as_ref(),
        cascade: StyleCascade::new(document.definitions()),
        para_style: None,
        table_style: None,
        sections: &document.definitions().sections,
        definitions: document.definitions(),
        numbering: NumberingState::new(),
        text_scale: 100_000,
        line_spacing_reduction: 0,
        paragraph_float_exclusions: None,
    };
    let galley = flow_blocks(document.body(), shaper, content_width, &mut ctx);
    (galley, report)
}

/// Flows an arbitrary slice of a document's **body** blocks into a galley at
/// `content_width` — the per-section building block the multi-column driver uses.
///
/// This is exactly [`build_galley`] restricted to `blocks` (a contiguous run of
/// the body belonging to one section) and laid out at that section's column
/// width, so each section flows at its own line-break width. The full section
/// list is threaded into the flow context, so a paragraph that carries a section
/// break still stamps the correct trailing page break (`nextPage` ⇒ break,
/// `continuous` ⇒ none) relative to the *next* section.
#[must_use]
pub fn build_galley_for_blocks(
    document: &Document,
    shaper: &dyn LineShaper,
    blocks: &[BlockNode],
    content_width: Twip,
) -> Vec<BlockFragment> {
    build_galley_for_blocks_inner(document, shaper, blocks, content_width, None)
}

pub(crate) fn build_galley_for_blocks_with_exclusions(
    document: &Document,
    shaper: &dyn LineShaper,
    blocks: &[BlockNode],
    content_width: Twip,
    exclusions: &ParagraphFloatExclusions,
) -> Vec<BlockFragment> {
    build_galley_for_blocks_inner(document, shaper, blocks, content_width, Some(exclusions))
}

fn build_galley_for_blocks_inner(
    document: &Document,
    shaper: &dyn LineShaper,
    blocks: &[BlockNode],
    content_width: Twip,
    exclusions: Option<&ParagraphFloatExclusions>,
) -> Vec<BlockFragment> {
    let resolver = FontResolver::new();
    let mut report = FontResolutionReport::new();
    let palette = document
        .definitions()
        .color_scheme
        .as_ref()
        .map(resolve_palette);
    let mut ctx = FlowCtx {
        resolver: &resolver,
        scheme: document.definitions().font_scheme.as_ref(),
        report: &mut report,
        default_tab: tabs::default_tab_stop(document.definitions().settings.default_tab_stop),
        media: &document.definitions().media,
        palette: palette.as_ref(),
        cascade: StyleCascade::new(document.definitions()),
        para_style: None,
        table_style: None,
        sections: &document.definitions().sections,
        definitions: document.definitions(),
        numbering: NumberingState::new(),
        text_scale: 100_000,
        line_spacing_reduction: 0,
        paragraph_float_exclusions: exclusions,
    };
    flow_blocks(blocks, shaper, content_width, &mut ctx)
}

/// Flows a header's or footer's block content into a galley of fragments at
/// `content_width` (the header/footer band width, normally the body content
/// width), reusing the document's font scheme, media table, and default tab stop.
///
/// This is the running-content counterpart to [`build_galley`]: the returned
/// fragments are laid out into the page's header/footer band by
/// [`crate::running::place_running_content`], and any `PAGE`/`NUMPAGES` field they
/// contain is resolved by [`crate::paginate::resolve_fields`]. `blocks` is a
/// [`casual_doc_model::v1::HeaderFooter::blocks`] list.
#[must_use]
pub fn flow_header_footer(
    document: &Document,
    blocks: &[BlockNode],
    shaper: &dyn LineShaper,
    content_width: Twip,
) -> Vec<BlockFragment> {
    flow_running_blocks(document, blocks, shaper, content_width, 100_000, 0)
}

/// Shared running-content/text-box flow constructor. A floating text box cannot
/// borrow the body's live [`FlowCtx`], so its authored normal-autofit adjustments
/// enter here while still using the same cascade, fonts, media, tables, and
/// recursive block flow as a header/footer.
fn flow_running_blocks(
    document: &Document,
    blocks: &[BlockNode],
    shaper: &dyn LineShaper,
    content_width: Twip,
    text_scale: u32,
    line_spacing_reduction: u32,
) -> Vec<BlockFragment> {
    let resolver = FontResolver::new();
    let mut report = FontResolutionReport::new();
    // Resolve the theme color palette once so every run's `w:themeColor` resolves
    // against the real scheme colors (not a black fallback).
    let palette = document
        .definitions()
        .color_scheme
        .as_ref()
        .map(resolve_palette);
    let mut ctx = FlowCtx {
        resolver: &resolver,
        scheme: document.definitions().font_scheme.as_ref(),
        report: &mut report,
        default_tab: tabs::default_tab_stop(document.definitions().settings.default_tab_stop),
        media: &document.definitions().media,
        palette: palette.as_ref(),
        cascade: StyleCascade::new(document.definitions()),
        para_style: None,
        table_style: None,
        // Running content has no section breaks of its own.
        sections: &[],
        definitions: document.definitions(),
        numbering: NumberingState::new(),
        text_scale,
        line_spacing_reduction,
        paragraph_float_exclusions: None,
    };
    flow_blocks(blocks, shaper, content_width, &mut ctx)
}

/// Builds the galley like [`build_galley`], but reuses the shaped lines of
/// unchanged paragraphs from `cache` instead of re-shaping them — the incremental
/// path (`43-…` §7.4 step 2). Only paragraphs whose content changed (their hash
/// differs) or whose node the transaction reported in `dirty` are handed to the
/// `shaper`; everything else is cloned from the cache. Shaping is the dominant
/// cost of building a galley, so this is what makes an edit `O(edit)` rather than
/// `O(document)`.
///
/// The result is identical to a fresh [`build_galley`] of the same document — the
/// cache changes only *how much shaping* happens, never the galley's content. The
/// cache is scoped to `content_width`; a width change transparently clears it.
///
/// Top-level body paragraphs are cached; tables (and their nested cell paragraphs)
/// are re-flowed each call, so a document dominated by large tables sees less
/// benefit — paragraph editing, the common case, is fully incremental.
#[must_use]
pub fn build_galley_cached(
    document: &Document,
    shaper: &dyn LineShaper,
    content_width: Twip,
    cache: &mut GalleyCache,
    dirty: &DirtySet,
) -> Vec<BlockFragment> {
    // A drop-cap paragraph and its following body paragraph are one coupled flow
    // unit. Until the cache key owns that adjacency, use the canonical fresh path
    // rather than serving either half under a stale independent paragraph key.
    if contains_drop_cap_pair(document.body(), &StyleCascade::new(document.definitions())) {
        return build_galley(document, shaper, content_width);
    }
    // The cache path resolves fonts exactly like the fresh path so a reused
    // fragment is byte-for-byte identical to a freshly built one; the resolved
    // face is folded into `paragraph_hash` (via `run.font`), so a cached fragment
    // can never carry a face resolved under different font inputs. The report is
    // discarded here, matching [`build_galley`]; callers wanting it use
    // [`build_galley_with_report`].
    let resolver = FontResolver::new();
    let mut report = FontResolutionReport::new();
    // Resolve the theme color palette once so every run's `w:themeColor` resolves
    // against the real scheme colors (not a black fallback).
    let palette = document
        .definitions()
        .color_scheme
        .as_ref()
        .map(resolve_palette);
    let mut ctx = FlowCtx {
        resolver: &resolver,
        scheme: document.definitions().font_scheme.as_ref(),
        report: &mut report,
        default_tab: tabs::default_tab_stop(document.definitions().settings.default_tab_stop),
        media: &document.definitions().media,
        palette: palette.as_ref(),
        cascade: StyleCascade::new(document.definitions()),
        para_style: None,
        table_style: None,
        sections: &document.definitions().sections,
        definitions: document.definitions(),
        numbering: NumberingState::new(),
        text_scale: 100_000,
        line_spacing_reduction: 0,
        paragraph_float_exclusions: None,
    };
    cache.begin_build(content_width);
    let mut galley = Vec::new();
    for block in document.body() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                // Resolve effective properties through the style cascade and record
                // the effective style so runs inherit the paragraph style's `rPr`.
                ctx.para_style = ctx.cascade.paragraph_style(&paragraph.properties);
                let mut props = ctx.cascade.resolve_paragraph(&paragraph.properties);
                let mut items = Vec::new();
                // Resolving here sets each run's resolved `font`, which
                // `paragraph_hash` hashes — so the cache key tracks the face.
                collect_items(
                    &paragraph.inlines,
                    &mut items,
                    shaper,
                    content_width,
                    &mut ctx,
                );
                prepend_paragraph_float_exclusions(paragraph.id, &mut items, &ctx);
                normalize_float_barriers(&mut items);
                // A numbered paragraph advances the document-order counter and gets a
                // marker; it bypasses the galley cache (the marker number is not in
                // the item hash, so a reused fragment could show a stale number after
                // an edit shifts the sequence). Numbered paragraphs are a minority, so
                // always reshaping them is the correct, simple choice.
                let numbered = props.numbering.is_some();
                let (constraints, marker) =
                    prepare_list_marker(paragraph.id, &mut props, content_width, &mut ctx, shaper);
                let shape = ShapeInputs {
                    items: &items,
                    tab_stops: &props.tabs,
                    default_tab: ctx.default_tab,
                    constraints,
                };
                let box_metrics = box_metrics(&props);
                let break_control = break_control(&props);
                let decor = paragraph_decor(&props, content_width);
                // A paragraph carrying an inline text box is never cached: the box's
                // flowed fragments are not folded into the paragraph hash, so a
                // reuse could serve stale nested content. Text boxes are rare, so
                // always reshaping them is the correct, simple choice.
                let has_text_box = items.iter().any(|i| matches!(i, FlowItem::TextBox { .. }));
                let uncacheable = has_text_box || numbered;
                // The paragraph-mark size + effective style feed the empty-paragraph
                // line height (synthesized below); folding them into the key keeps a
                // reused fragment correct when only the mark/style changes.
                let mark_size = props.mark_run.as_deref().and_then(|r| r.size_half_points);
                let hash = paragraph_hash(
                    paragraph.id,
                    &shape,
                    box_metrics,
                    break_control,
                    decor,
                    ctx.para_style,
                    mark_size,
                );

                if !uncacheable && let Some(fragment) = cache.reusable(paragraph.id, hash, dirty) {
                    // The cached fragment is section-break-pure (the break is not
                    // folded into the hash — a *neighboring* paragraph's section-type
                    // edit can flip this paragraph's break without dirtying it), so
                    // re-derive and stamp the break from the current section list.
                    let mut fragment = fragment.clone();
                    apply_section_break_to_fragment(&mut fragment, &paragraph.properties, &ctx);
                    galley.push(fragment);
                    continue;
                }
                let range = ModelRange::new(
                    ModelPos::new(paragraph.id, 0),
                    ModelPos::new(paragraph.id, 0),
                );
                let mut lines = shape_paragraph_items(
                    shaper,
                    shape.items,
                    shape.tab_stops,
                    shape.default_tab,
                    shape.constraints,
                    range,
                );
                if let Some(marker) = marker {
                    marker.inject(&mut lines, range);
                }
                ensure_nonempty_paragraph(
                    &mut lines,
                    &props,
                    &mut ctx,
                    shaper,
                    content_width,
                    range,
                );
                let fragment = BlockFragment::Paragraph {
                    id: paragraph.id,
                    lines,
                    box_metrics,
                    break_control,
                    decor,
                };
                // Cache the section-break-pure fragment, then stamp the break onto
                // the copy that enters the galley (see the cache-hit path above).
                if !uncacheable {
                    cache.store(paragraph.id, hash, fragment.clone());
                }
                let mut fragment = fragment;
                apply_section_break_to_fragment(&mut fragment, &paragraph.properties, &ctx);
                galley.push(fragment);
            }
            BlockNode::Table(table) => {
                flow_table(table, shaper, content_width, &mut galley, &mut ctx)
            }
            BlockNode::Sdt(sdt) => {
                // A block-level content control (`w:sdt`) is a transparent
                // wrapper: its child blocks flow exactly as if the wrapper
                // weren't there. Recurse through the shared block-flow path so
                // the children's fragments — carrying their own NodeIds, so
                // hit-testing and the editing bridge still resolve — reach the
                // galley. The wrapper itself contributes no box. (These recursed
                // children are not paragraph-cached, but block SDTs are rare.)
                galley.extend(flow_blocks(&sdt.blocks, shaper, content_width, &mut ctx));
            }
            // TODO(altchunk): the embedded part's actual content (HTML/RTF/nested
            // WordprocessingML) is not parsed or modeled — only an opaque part
            // reference (`P1F-28`) — so there is nothing to recurse and lay out
            // for real. A zero-space block would still let the reader lose track
            // of the chunk's presence, so a deterministic placeholder box claims
            // real layout space instead; see [`alt_chunk_fragment`]. This is a
            // visible approximation, not rendered altChunk content.
            BlockNode::AltChunk(chunk) => {
                galley.push(alt_chunk_fragment(chunk, shaper, content_width, &mut ctx));
            }
        }
    }
    galley
}

/// The inputs that determine a paragraph's shaped lines: its flattened item
/// stream (runs + tabs + breaks), the tab-stop context, and the wrap constraints.
/// Bundled so the hash and the shaper read the exact same values.
struct ShapeInputs<'a> {
    items: &'a [FlowItem<'a>],
    tab_stops: &'a [TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
}

/// A stable per-value key for a run of tab-stop alignment/leader/break enums that
/// do not derive `Hash` (so they can feed the cache key).
const fn tab_alignment_key(alignment: TabAlignment) -> u8 {
    match alignment {
        TabAlignment::Start => 0,
        TabAlignment::Center => 1,
        TabAlignment::End => 2,
        TabAlignment::Decimal => 3,
        TabAlignment::Bar => 4,
    }
}

/// A stable per-value key for a tab leader (see [`tab_alignment_key`]).
const fn tab_leader_key(leader: Option<TabLeader>) -> u8 {
    match leader {
        None => 0,
        Some(TabLeader::Dot) => 1,
        Some(TabLeader::Hyphen) => 2,
        Some(TabLeader::Underscore) => 3,
        Some(TabLeader::MiddleDot) => 4,
        Some(TabLeader::Heavy) => 5,
    }
}

/// A stable per-value key for a hard-break kind (see [`tab_alignment_key`]).
const fn break_kind_key(kind: BreakKind) -> u8 {
    match kind {
        BreakKind::Line => 0,
        BreakKind::Page => 1,
        BreakKind::Column => 2,
    }
}

/// Hashes every input that determines a paragraph's shaped fragment: its node
/// identity, each run's text and styling, the tab/break structure and tab-stop
/// context, the wrap constraints, and the box/break metrics. Because the hash is
/// computed from the exact values fed to the shaper and the fragment builder, a
/// matching hash guarantees an identical fragment — the [`GalleyCache`] reuse
/// condition.
fn paragraph_hash(
    id: casual_doc_model::NodeId,
    shape: &ShapeInputs<'_>,
    box_metrics: BoxMetrics,
    break_control: BreakControl,
    decor: ParagraphDecor,
    style: Option<StyleId>,
    mark_size: Option<u32>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    id.as_u128().hash(&mut hasher);
    // The effective style id + paragraph-mark size determine an empty paragraph's
    // synthesized line height, which is otherwise invisible to the item stream.
    style.map(|s| s.node_id().as_u128()).hash(&mut hasher);
    mark_size.hash(&mut hasher);
    for item in shape.items {
        match item {
            FlowItem::Run(run) => {
                0u8.hash(&mut hasher);
                run.text.hash(&mut hasher);
                // The requested family (not just the resolved `font`) is folded in:
                // under system fonts two runs can resolve to the same bundled
                // `font` yet shape with different installed faces by requested name.
                run.requested_family.hash(&mut hasher);
                run.font.0.hash(&mut hasher);
                run.size.0.hash(&mut hasher);
                run.character_scale_percent.hash(&mut hasher);
                run.bold.hash(&mut hasher);
                run.italic.hash(&mut hasher);
                run.letter_spacing.0.hash(&mut hasher);
                run.color.hash(&mut hasher);
                run.decoration.underline.hash(&mut hasher);
                run.decoration.strikethrough.hash(&mut hasher);
                run.highlight.hash(&mut hasher);
                run.baseline_shift.0.hash(&mut hasher);
            }
            FlowItem::Tab => 1u8.hash(&mut hasher),
            FlowItem::PositionalTab {
                alignment,
                relative_to,
                leader,
            } => {
                8u8.hash(&mut hasher);
                (*alignment as u8).hash(&mut hasher);
                (*relative_to as u8).hash(&mut hasher);
                (*leader as u8).hash(&mut hasher);
            }
            FlowItem::Break(kind) => {
                2u8.hash(&mut hasher);
                break_kind_key(*kind).hash(&mut hasher);
            }
            FlowItem::Image { media, size, crop } => {
                3u8.hash(&mut hasher);
                media.hash(&mut hasher);
                size.width.0.hash(&mut hasher);
                size.height.0.hash(&mut hasher);
                crop.map(|c| (c.left, c.top, c.right, c.bottom))
                    .hash(&mut hasher);
            }
            FlowItem::Math { size, runs, rules } => {
                11u8.hash(&mut hasher);
                size.width.0.hash(&mut hasher);
                size.height.0.hash(&mut hasher);
                for run in runs {
                    run.font.0.hash(&mut hasher);
                    run.size.0.hash(&mut hasher);
                    run.origin.x.0.hash(&mut hasher);
                    run.origin.y.0.hash(&mut hasher);
                    for glyph in &run.glyphs {
                        glyph.id.hash(&mut hasher);
                        glyph.advance.0.hash(&mut hasher);
                    }
                }
                for rule in rules {
                    rule.origin.x.0.hash(&mut hasher);
                    rule.origin.y.0.hash(&mut hasher);
                    rule.size.width.0.hash(&mut hasher);
                    rule.size.height.0.hash(&mut hasher);
                }
            }
            FlowItem::Field { kind, value, style } => {
                4u8.hash(&mut hasher);
                (*kind as u8).hash(&mut hasher);
                value.hash(&mut hasher);
                style.font.0.hash(&mut hasher);
                style.size.0.hash(&mut hasher);
                style.character_scale_percent.hash(&mut hasher);
                style.color.hash(&mut hasher);
                style.bold.hash(&mut hasher);
                style.italic.hash(&mut hasher);
                style.letter_spacing.0.hash(&mut hasher);
                style.decoration.underline.hash(&mut hasher);
                style.decoration.strikethrough.hash(&mut hasher);
            }
            // A text box's flowed fragments are not hashed here; a paragraph
            // carrying one bypasses the galley cache entirely (see
            // `build_galley_cached`), so this arm only keeps the match exhaustive.
            FlowItem::TextBox {
                size, border, fill, ..
            } => {
                5u8.hash(&mut hasher);
                size.width.0.hash(&mut hasher);
                size.height.0.hash(&mut hasher);
                border.hash(&mut hasher);
                fill.hash(&mut hasher);
            }
            FlowItem::HorizontalRule(rule) => {
                6u8.hash(&mut hasher);
                rule.origin.x.0.hash(&mut hasher);
                rule.size.width.0.hash(&mut hasher);
                rule.size.height.0.hash(&mut hasher);
                rule.color.hash(&mut hasher);
            }
            FlowItem::FloatBarrier { height } => {
                7u8.hash(&mut hasher);
                height.0.hash(&mut hasher);
            }
            FlowItem::FloatExclusion {
                side,
                width,
                height,
            } => {
                9u8.hash(&mut hasher);
                (*side as u8).hash(&mut hasher);
                width.0.hash(&mut hasher);
                height.0.hash(&mut hasher);
            }
            FlowItem::NoteReference(marker) => {
                10u8.hash(&mut hasher);
                (marker.kind as u8).hash(&mut hasher);
                marker.note.node_id().as_u128().hash(&mut hasher);
            }
        }
    }
    for stop in shape.tab_stops {
        stop.position_twips.hash(&mut hasher);
        tab_alignment_key(stop.alignment).hash(&mut hasher);
        tab_leader_key(stop.leader).hash(&mut hasher);
    }
    shape.default_tab.0.hash(&mut hasher);
    let constraints = &shape.constraints;
    constraints.max_width.0.hash(&mut hasher);
    constraints.margin_width.0.hash(&mut hasher);
    constraints.indent_start.0.hash(&mut hasher);
    constraints.rtl.hash(&mut hasher);
    (constraints.alignment as u8).hash(&mut hasher);
    constraints.line_height_percent.hash(&mut hasher);
    constraints.line_at_least.map(|t| t.0).hash(&mut hasher);
    constraints.line_exact.map(|t| t.0).hash(&mut hasher);
    constraints.first_line_indent.0.hash(&mut hasher);
    box_metrics.space_before.0.hash(&mut hasher);
    box_metrics.space_after.0.hash(&mut hasher);
    box_metrics.indent_start.0.hash(&mut hasher);
    box_metrics.indent_end.0.hash(&mut hasher);
    break_control.page_break_before.hash(&mut hasher);
    break_control.keep_next.hash(&mut hasher);
    break_control.keep_lines.hash(&mut hasher);
    break_control.widow_control.hash(&mut hasher);
    decor.shading.hash(&mut hasher);
    decor.width.0.hash(&mut hasher);
    for e in [
        decor.borders.top,
        decor.borders.bottom,
        decor.borders.start,
        decor.borders.end,
    ]
    .into_iter()
    .flatten()
    {
        e.color.hash(&mut hasher);
        e.width.0.hash(&mut hasher);
        e.pattern.hash(&mut hasher);
    }
    hasher.finish()
}

/// Flows a sequence of block nodes (a body or a table cell) into shaped
/// fragments at `width`. Paragraphs shape to lines; tables expand to their rows;
/// block content controls are laid out in a later slice.
fn flow_blocks(
    blocks: &[BlockNode],
    shaper: &dyn LineShaper,
    width: Twip,
    ctx: &mut FlowCtx,
) -> Vec<BlockFragment> {
    let mut galley = Vec::new();
    let mut index = 0;
    while index < blocks.len() {
        if let (BlockNode::Paragraph(drop_cap), Some(BlockNode::Paragraph(body))) =
            (&blocks[index], blocks.get(index + 1))
            && let Some(frame) =
                effective_drop_cap_frame(drop_cap, &ctx.cascade, ctx.table_style.as_ref())
            && is_single_character_drop_cap(drop_cap)
            && supports_drop_cap_layout(frame)
        {
            let mut drop_fragment = flow_paragraph(drop_cap, shaper, width, ctx, &[], true);
            let exclusion = collapse_drop_cap_fragment(&mut drop_fragment, frame);
            galley.push(drop_fragment);
            let exclusions = exclusion.into_iter().collect::<Vec<_>>();
            galley.push(flow_paragraph(body, shaper, width, ctx, &exclusions, false));
            index += 2;
            continue;
        }

        let block = &blocks[index];
        match block {
            BlockNode::Paragraph(paragraph) => {
                galley.push(flow_paragraph(paragraph, shaper, width, ctx, &[], false));
            }
            BlockNode::Table(table) => flow_table(table, shaper, width, &mut galley, ctx),
            BlockNode::Sdt(sdt) => {
                // Transparent wrapper (see the body loop): flow its children
                // through this same path so SDTs inside table cells / nested
                // contexts also flow. Nested SDTs recurse naturally.
                galley.extend(flow_blocks(&sdt.blocks, shaper, width, ctx));
            }
            // TODO(altchunk): same bounded placeholder as the body-flow site
            // above (no fallback blocks in the model to recurse into for real).
            BlockNode::AltChunk(chunk) => {
                galley.push(alt_chunk_fragment(chunk, shaper, width, ctx));
            }
        }
        index += 1;
    }
    galley
}

fn flow_paragraph(
    paragraph: &Paragraph,
    shaper: &dyn LineShaper,
    width: Twip,
    ctx: &mut FlowCtx,
    leading_exclusions: &[ParagraphFloatExclusion],
    natural_line_height: bool,
) -> BlockFragment {
    // Resolve the paragraph's *effective* properties through the style cascade,
    // and record its effective style so each run inherits the paragraph style's
    // `rPr`.
    ctx.para_style = ctx.cascade.paragraph_style(&paragraph.properties);
    let mut props = ctx
        .cascade
        .resolve_paragraph_in_table(&paragraph.properties, ctx.table_style.as_ref());
    if natural_line_height && let Some(spacing) = props.spacing.as_mut() {
        // Word's generated drop-cap paragraph commonly carries an exact line
        // height smaller than its large initial. That box positions the frame; it
        // must not become a glyph-ink clip in our coupled representation.
        spacing.line_rule = None;
        spacing.line_twips = None;
        spacing.line_percent = None;
    }
    let mut items = Vec::new();
    collect_items(&paragraph.inlines, &mut items, shaper, width, ctx);
    prepend_paragraph_float_exclusions(paragraph.id, &mut items, ctx);
    prepend_explicit_float_exclusions(leading_exclusions, &mut items);
    normalize_float_barriers(&mut items);
    let range = ModelRange::new(
        ModelPos::new(paragraph.id, 0),
        ModelPos::new(paragraph.id, 0),
    );
    // Resolve the list marker (if any) before shaping: it advances the numbering
    // counter, may merge the level's indent into `props`, and adjusts the body's
    // first-line indent.
    let (constraints, marker) = prepare_list_marker(paragraph.id, &mut props, width, ctx, shaper);
    let mut lines = shape_paragraph_items(
        shaper,
        &items,
        &props.tabs,
        ctx.default_tab,
        constraints,
        range,
    );
    if let Some(marker) = marker {
        marker.inject(&mut lines, range);
    }
    ensure_nonempty_paragraph(&mut lines, &props, ctx, shaper, width, range);
    apply_section_break(&mut lines, &paragraph.properties, ctx);
    BlockFragment::Paragraph {
        id: paragraph.id,
        lines,
        box_metrics: box_metrics(&props),
        break_control: break_control(&props),
        decor: paragraph_decor(&props, width),
    }
}

fn contains_drop_cap_pair(blocks: &[BlockNode], cascade: &StyleCascade<'_>) -> bool {
    blocks.windows(2).any(|pair| {
        let (BlockNode::Paragraph(drop_cap), BlockNode::Paragraph(_)) = (&pair[0], &pair[1]) else {
            return false;
        };
        effective_drop_cap_frame(drop_cap, cascade, None).is_some_and(supports_drop_cap_layout)
            && is_single_character_drop_cap(drop_cap)
    })
}

fn effective_drop_cap_frame(
    paragraph: &Paragraph,
    cascade: &StyleCascade<'_>,
    table_style: Option<&TableStyleLayer>,
) -> Option<DropCapFrame> {
    cascade
        .resolve_paragraph_in_table(&paragraph.properties, table_style)
        .drop_cap_frame
}

fn is_single_character_drop_cap(paragraph: &Paragraph) -> bool {
    let text = node_plain_text(&paragraph.inlines);
    let mut chars = text.chars();
    chars.next().is_some() && chars.next().is_none()
}

fn supports_drop_cap_layout(frame: DropCapFrame) -> bool {
    matches!(frame.wrap, None | Some(FrameWrap::Around | FrameWrap::Auto))
}

fn collapse_drop_cap_fragment(
    fragment: &mut BlockFragment,
    frame: DropCapFrame,
) -> Option<ParagraphFloatExclusion> {
    let BlockFragment::Paragraph {
        lines, box_metrics, ..
    } = fragment
    else {
        return None;
    };
    let natural_height = lines.height();
    let right = lines
        .lines
        .iter()
        .flat_map(|line| &line.runs)
        .map(|run| {
            run.origin.x
                + run
                    .glyphs
                    .iter()
                    .fold(Twip::ZERO, |advance, glyph| advance + glyph.advance)
        })
        .max()
        .unwrap_or(Twip::ZERO);
    let h_space = Twip(frame.horizontal_space_twips.unwrap_or(0) as i32);
    let v_space = Twip(frame.vertical_space_twips.unwrap_or(0) as i32);
    let width = (right + h_space).max(Twip(1));
    let authored_height = Twip(i32::from(frame.lines).saturating_mul(240));
    let height = natural_height.max(authored_height) + v_space;

    // The following paragraph starts at the same block origin. Preserve the
    // large run's baseline/ink, but make the frame paragraph flow-neutral and
    // remove the exact-line clip that caused the page-5 truncation.
    for line in &mut lines.lines {
        line.height = Twip::ZERO;
        line.clip = false;
        if frame.mode == DropCapMode::Margin {
            for run in &mut line.runs {
                run.origin.x = run.origin.x - width;
            }
        }
    }
    box_metrics.space_before = Twip::ZERO;
    box_metrics.space_after = Twip::ZERO;

    (frame.mode == DropCapMode::Drop).then_some(ParagraphFloatExclusion {
        side: InlineFloatSide::Left,
        width,
        height,
    })
}

/// Flows a table into one [`BlockFragment::TableRow`] per row at Word-grade
/// fidelity (`P1D-003`):
///
/// - **Column widths** come from the width solver ([`solve_column_widths`]):
///   preferred `w:tblW`/`w:tcW` widths, the fixed-vs-autofit algorithm,
///   content-driven minimum/preferred widths, and `w:gridSpan` distribution.
/// - **Table indent** (`w:tblInd`) offsets every cell's leading edge and reduces
///   the width available to the columns.
/// - **Row height** honors the `w:trHeight` rule (`atLeast` grows with content,
///   `exact` fixes the height and clips overflow).
/// - **Borders** are resolved by OOXML conflict precedence, preserving common
///   line patterns and independently styled horizontal span segments.
///
/// Cross-page row splitting and header repetition are the paginator's job
/// ([`crate::paginate`]); this produces the row fragments it slices.
fn flow_table(
    table: &Table,
    shaper: &dyn LineShaper,
    width: Twip,
    galley: &mut Vec<BlockFragment>,
    ctx: &mut FlowCtx,
) {
    let merge_roles = resolve_vertical_merge_roles(table);
    let style_layers = resolve_table_style_layers(table, &ctx.cascade);
    let border_candidates = resolve_table_border_candidates(table, &style_layers);
    let indent = table.properties.indent_twips.unwrap_or(0);
    let available = (width.raw() - indent).max(1);
    let widths = solve_table_columns(table, shaper, available, ctx, &merge_roles, &style_layers);

    // Logical grid edges are independent of physical alignment. `w:bidiVisual`
    // reflects a cell's logical range through the solved table box below.
    let ncols = widths.len();
    let mut edges = Vec::with_capacity(ncols + 1);
    let mut x = 0;
    for w in &widths {
        edges.push(x);
        x += w.raw();
    }
    edges.push(x);
    let table_width = x;
    let edge = |col: usize| Twip(edges[col.min(edges.len() - 1)]);

    let mut rows = Vec::with_capacity(table.rows.len());
    for (row_index, row) in table.rows.iter().enumerate() {
        let row_spacing = effective_cell_spacing(table, row);
        let row_origin = table_row_origin(
            row.properties.alignment.or(table.properties.alignment),
            table.properties.tbl_bidi_visual,
            width.raw(),
            table_width,
            indent,
        );
        let mut cells = Vec::new();
        let mut col = 0usize;
        for (index, cell) in row.cells.iter().enumerate() {
            let span = cell.properties.grid_span.unwrap_or(1).max(1) as usize;
            let logical_start = edge(col);
            let logical_end = edge(col + span);
            let slot_width = Twip((logical_end.raw() - logical_start.raw()).max(1));
            let slot_x = if table.properties.tbl_bidi_visual {
                Twip(
                    row_origin
                        .raw()
                        .saturating_add(table_width.saturating_sub(logical_end.raw())),
                )
            } else {
                row_origin + logical_start
            };
            let mut cell_spacing = cell_box_spacing(row_spacing, slot_width);
            let mut borders = if row_spacing > 0 {
                resolve_separated_cell_borders(&border_candidates[row_index][index])
            } else {
                resolve_cell_borders(&table.rows, &border_candidates, row_index, index, &edges)
            };
            let mut table_borders = if row_spacing > 0 {
                resolve_table_perimeter_borders(table, &style_layers, row_index, index)
            } else {
                CellBorders::default()
            };
            // A cell fill overrides table shading. When the cell omits `w:shd`,
            // the table-style layer applies next: the referenced style's (and
            // its `basedOn` ancestors') base cell/table shading, overlaid by
            // whichever conditional `w:tblStylePr` region the row+cell's
            // combined `w:cnfStyle` selects (gated by `w:tblLook`) — e.g. a
            // banded-row or header-row fill. Only then does the table's own
            // direct `w:shd` apply through the cell extent.
            let style_layer = &style_layers[row_index][index];
            let shading = cell
                .properties
                .shading
                .fill
                .or(style_layer.shading)
                .or(table.properties.shading.fill)
                .map(|c| [c.r, c.g, c.b, 255]);
            // Word insets a cell's content by `w:tcMar` (per-cell), falling back to
            // the table's `w:tblCellMar`, then to Word's built-in default. Content
            // therefore flows at the reduced inner width; composition offsets it by
            // the start/top margins (and the vertical-alignment slack).
            let mut margins =
                resolve_cell_margins(&cell.properties.margins, &table.properties.cell_margins);
            if table.properties.tbl_bidi_visual {
                std::mem::swap(&mut cell_spacing.start, &mut cell_spacing.end);
            }
            let cell_x = slot_x + cell_spacing.start;
            let cell_width = Twip(
                slot_width
                    .raw()
                    .saturating_sub(cell_spacing.start.raw())
                    .saturating_sub(cell_spacing.end.raw())
                    .max(1),
            );
            if table.properties.tbl_bidi_visual {
                mirror_cell_geometry(&mut margins, &mut borders, &mut table_borders, cell_width);
            }
            let inner_width =
                Twip((cell_width.raw() - margins.start.raw() - margins.end.raw()).max(1));
            let merge_role = merge_roles[row_index][index];
            // Replace, rather than inherit, the enclosing cell's table layer.
            // This also clears an outer style when a nested table is unstyled.
            let outer_table_style = std::mem::replace(
                &mut ctx.table_style,
                table
                    .properties
                    .style_ref
                    .is_some()
                    .then(|| style_layer.clone()),
            );
            let blocks = if matches!(merge_role, VerticalMergeRole::Continue) {
                Vec::new()
            } else {
                flow_blocks(&cell.blocks, shaper, inner_width, ctx)
            };
            ctx.table_style = outer_table_style;
            cells.push(CellFragment {
                id: cell.id,
                grid_span: span as u32,
                x: cell_x,
                width: cell_width,
                cell_spacing,
                blocks,
                margins,
                vertical_alignment: cell_vertical_alignment(&cell.properties),
                vertical_merge: if matches!(merge_role, VerticalMergeRole::Continue) {
                    CellVerticalMerge::Continue
                } else {
                    CellVerticalMerge::None
                },
                borders,
                table_borders,
                shading,
            });
            col += span;
        }
        let content_h = cells
            .iter()
            .enumerate()
            .filter(|(index, _)| match merge_roles[row_index][*index] {
                VerticalMergeRole::Continue => false,
                VerticalMergeRole::Restart { end_row, .. } => end_row == row_index,
                VerticalMergeRole::None => true,
            })
            .map(|(_, cell)| cell.occupied_height())
            .max()
            .unwrap_or(Twip::ZERO);
        let (height, clip) = resolve_row_height(&row.properties, content_h);
        for cell in &mut cells {
            clamp_vertical_cell_spacing(&mut cell.cell_spacing, height);
        }
        rows.push(FlowedTableRow {
            id: row.id,
            cells,
            height,
            can_split: !row.properties.cant_split,
            header: row.properties.header,
            clip,
            exact: row_has_exact_height(&row.properties),
            merge_keep_next: false,
        });
    }

    resolve_vertical_merge_geometry(&merge_roles, &mut rows);
    galley.extend(rows.into_iter().map(|row| BlockFragment::TableRow {
        id: row.id,
        table: table.id,
        cells: row.cells,
        height: row.height,
        can_split: row.can_split,
        header: row.header,
        merge_keep_next: row.merge_keep_next,
        clip: row.clip,
    }));
}

/// Resolves a row's logical `w:jc` to its physical grid origin. A row-direct
/// alignment has already been overlaid by the caller. `w:tblInd` is a logical
/// start inset, so it changes only start placement; center/end resolve against
/// the full containing block without applying that inset twice.
fn table_row_origin(
    alignment: Option<Alignment>,
    bidi_visual: bool,
    containing_width: i32,
    table_width: i32,
    indent: i32,
) -> Twip {
    let remaining = containing_width.saturating_sub(table_width);
    match alignment.unwrap_or(Alignment::Start) {
        Alignment::Center => Twip(remaining / 2),
        Alignment::End if !bidi_visual => Twip(remaining),
        Alignment::End => Twip::ZERO,
        Alignment::Start | Alignment::Justify if !bidi_visual => Twip(indent),
        Alignment::Start | Alignment::Justify => Twip(remaining.saturating_sub(indent)),
    }
}

/// Resolves direct row-over-table `w:tblCellSpacing`. Style-provided row
/// properties are intentionally outside this slice; model validation already
/// rejects negatives, and the clamp keeps manually constructed documents safe.
fn effective_cell_spacing(table: &Table, row: &TableRow) -> i32 {
    row.properties
        .cell_spacing_twips
        .or(table.properties.cell_spacing_twips)
        .unwrap_or(0)
        .max(0)
}

/// Splits one authored spacing value around the cell box while retaining at
/// least one twip for that box. Spanning cells get only the two outside halves;
/// covered internal grid lines do not introduce gaps.
fn cell_box_spacing(spacing: i32, slot_width: Twip) -> CellBoxSpacing {
    let spacing = spacing.max(0);
    let mut start = spacing / 2;
    let mut end = spacing - start;
    let available_gap = slot_width.raw().saturating_sub(1).max(0);
    if start.saturating_add(end) > available_gap {
        start = start.min(available_gap / 2);
        end = available_gap - start;
    }
    CellBoxSpacing {
        top: Twip(spacing / 2),
        start: Twip(start),
        bottom: Twip(spacing - spacing / 2),
        end: Twip(end),
    }
}

/// Keeps the separated border box inside an authored exact-height row. Auto and
/// at-least rows have already grown for the full spacing, so this is ordinarily
/// a no-op.
fn clamp_vertical_cell_spacing(spacing: &mut CellBoxSpacing, row_height: Twip) {
    let available_gap = row_height.raw().saturating_sub(1).max(0);
    if spacing.top.raw().saturating_add(spacing.bottom.raw()) > available_gap {
        let top = spacing.top.raw().min(available_gap / 2);
        spacing.top = Twip(top);
        spacing.bottom = Twip(available_gap - top);
    }
}

/// Reflects logical start/end geometry into physical left/right geometry for a
/// visually RTL table. Horizontal segment offsets are relative to the physical
/// left edge at composition time, so they are reflected and returned in
/// ascending physical order as well.
fn mirror_cell_geometry(
    margins: &mut CellContentMargins,
    borders: &mut CellBorders,
    table_borders: &mut CellBorders,
    width: Twip,
) {
    std::mem::swap(&mut margins.start, &mut margins.end);
    std::mem::swap(&mut borders.start, &mut borders.end);
    std::mem::swap(&mut table_borders.start, &mut table_borders.end);
    for segments in [&mut borders.top_segments, &mut borders.bottom_segments] {
        for segment in segments.iter_mut() {
            segment.offset = Twip(
                width
                    .raw()
                    .saturating_sub(segment.offset.raw())
                    .saturating_sub(segment.length.raw())
                    .max(0),
            );
        }
        segments.reverse();
    }
}

fn resolve_table_style_layers(
    table: &Table,
    cascade: &StyleCascade<'_>,
) -> Vec<Vec<TableStyleLayer>> {
    table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| {
                    let cnf = union_cnf(
                        row.properties.conditional_format.unwrap_or_default(),
                        cell.properties.conditional_format.unwrap_or_default(),
                    );
                    cascade.table_style_layer(
                        table.properties.style_ref,
                        table.properties.look,
                        cnf,
                    )
                })
                .collect()
        })
        .collect()
}

/// Materializes the border candidate on each physical cell side before shared-
/// edge conflict resolution. This lets conditional table borders vary by cell
/// while preserving the existing topology/segmentation pass.
fn resolve_table_border_candidates(
    table: &Table,
    layers: &[Vec<TableStyleLayer>],
) -> Vec<Vec<TableBorders>> {
    table
        .rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            row.cells
                .iter()
                .enumerate()
                .map(|(cell_index, cell)| {
                    let layer = &layers[row_index][cell_index];
                    let table_borders = effective_table_borders(table, layer);

                    let mut cell_borders = layer.cell_borders.clone();
                    overlay_table_borders(&mut cell_borders, &cell.properties.borders);

                    let first = cell_index == 0;
                    let last = cell_index + 1 == row.cells.len();
                    let top = row_index == 0;
                    let bottom = row_index + 1 == table.rows.len();
                    TableBorders {
                        top: effective_border(
                            cell_borders.top.as_ref(),
                            if top {
                                table_borders.top.as_ref()
                            } else {
                                table_borders.inside_h.as_ref()
                            },
                        )
                        .cloned(),
                        start: effective_border(
                            cell_borders.start.as_ref(),
                            if first {
                                table_borders.start.as_ref()
                            } else {
                                table_borders.inside_v.as_ref()
                            },
                        )
                        .cloned(),
                        bottom: effective_border(
                            cell_borders.bottom.as_ref(),
                            if bottom {
                                table_borders.bottom.as_ref()
                            } else {
                                table_borders.inside_h.as_ref()
                            },
                        )
                        .cloned(),
                        end: effective_border(
                            cell_borders.end.as_ref(),
                            if last {
                                table_borders.end.as_ref()
                            } else {
                                table_borders.inside_v.as_ref()
                            },
                        )
                        .cloned(),
                        inside_h: None,
                        inside_v: None,
                    }
                })
                .collect()
        })
        .collect()
}

fn effective_table_borders(table: &Table, layer: &TableStyleLayer) -> TableBorders {
    let mut borders = layer.table_borders.clone();
    overlay_table_borders(&mut borders, &table.properties.borders);
    borders
}

/// Retains the table perimeter as its own paint layer for separated-cell rows.
/// Each perimeter segment is attached to the cell whose logical grid slot owns
/// it; composition reconstructs that slot from the inset cell geometry.
fn resolve_table_perimeter_borders(
    table: &Table,
    layers: &[Vec<TableStyleLayer>],
    row_index: usize,
    cell_index: usize,
) -> CellBorders {
    let row = &table.rows[row_index];
    let borders = effective_table_borders(table, &layers[row_index][cell_index]);
    CellBorders {
        top: (row_index == 0)
            .then(|| resolve_edge(&[borders.top.as_ref()]))
            .flatten(),
        start: (cell_index == 0)
            .then(|| resolve_edge(&[borders.start.as_ref()]))
            .flatten(),
        bottom: (row_index + 1 == table.rows.len())
            .then(|| resolve_edge(&[borders.bottom.as_ref()]))
            .flatten(),
        end: (cell_index + 1 == row.cells.len())
            .then(|| resolve_edge(&[borders.end.as_ref()]))
            .flatten(),
        top_segments: Vec::new(),
        bottom_segments: Vec::new(),
    }
}

/// One table row while vertical-merge height constraints are being resolved.
struct FlowedTableRow {
    id: NodeId,
    cells: Vec<CellFragment>,
    height: Twip,
    can_split: bool,
    header: bool,
    clip: bool,
    exact: bool,
    merge_keep_next: bool,
}

/// A model cell's validated role in a vertical merge. Invalid/orphan
/// continuations remain `None` so their content stays visible.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum VerticalMergeRole {
    #[default]
    None,
    Restart {
        end_row: usize,
        end_cell: usize,
    },
    Continue,
}

/// Resolves `w:vMerge` runs by exact half-open grid-column range. A continuation
/// with no matching active restart is deliberately left ordinary.
fn resolve_vertical_merge_roles(table: &Table) -> Vec<Vec<VerticalMergeRole>> {
    let mut roles: Vec<Vec<VerticalMergeRole>> = table
        .rows
        .iter()
        .map(|row| vec![VerticalMergeRole::None; row.cells.len()])
        .collect();
    let mut active: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut next = BTreeMap::new();
        let mut column = 0usize;
        for (cell_index, cell) in row.cells.iter().enumerate() {
            let span = cell.properties.grid_span.unwrap_or(1).max(1) as usize;
            let key = (column, column.saturating_add(span));
            match cell.properties.vertical_merge {
                Some(VerticalMerge::Restart) => {
                    roles[row_index][cell_index] = VerticalMergeRole::Restart {
                        end_row: row_index,
                        end_cell: cell_index,
                    };
                    next.insert(key, (row_index, cell_index));
                }
                Some(VerticalMerge::Continue) => {
                    if let Some(&(start_row, start_cell)) = active.get(&key) {
                        roles[row_index][cell_index] = VerticalMergeRole::Continue;
                        roles[start_row][start_cell] = VerticalMergeRole::Restart {
                            end_row: row_index,
                            end_cell: cell_index,
                        };
                        next.insert(key, (start_row, start_cell));
                    }
                }
                None => {}
            }
            column = column.saturating_add(span);
        }
        active = next;
    }
    roles
}

/// Applies merged-cell minimum heights, closing-edge ownership, and pagination
/// keep boundaries after every physical row has its ordinary minimum height.
fn resolve_vertical_merge_geometry(roles: &[Vec<VerticalMergeRole>], rows: &mut [FlowedTableRow]) {
    // First solve every merged-height inequality. Multiple merges can overlap the
    // same physical rows, so no origin caches its final height until every
    // constraint has had a chance to grow those rows.
    for (row_index, row_roles) in roles.iter().enumerate() {
        for (cell_index, role) in row_roles.iter().copied().enumerate() {
            let VerticalMergeRole::Restart { end_row, end_cell } = role else {
                continue;
            };
            if end_row == row_index {
                continue;
            }

            let origin_cell = &rows[row_index].cells[cell_index];
            let closing_spacing_bottom = rows[end_row].cells[end_cell].cell_spacing.bottom.raw();
            let required = origin_cell
                .occupied_height()
                .raw()
                .saturating_sub(origin_cell.cell_spacing.bottom.raw())
                .saturating_add(closing_spacing_bottom);
            let current: i32 = rows[row_index..=end_row]
                .iter()
                .map(|row| row.height.raw())
                .sum();
            if required > current {
                if let Some(row) = rows[row_index..=end_row].iter_mut().find(|row| !row.exact) {
                    row.height = Twip(row.height.raw().saturating_add(required - current));
                } else {
                    rows[row_index].clip = true;
                }
            }

            for row in &mut rows[row_index..end_row] {
                row.merge_keep_next = true;
            }
            let header = rows[row_index].header;
            if rows[row_index..=end_row]
                .iter()
                .any(|row| row.header != header)
            {
                // A repeated header cannot reproduce only part of a merged box.
                // Disable repetition for the crossing group; a merge wholly
                // inside the header band remains repeatable as a unit.
                for row in &mut rows[row_index..=end_row] {
                    row.header = false;
                }
            }
        }
    }

    // Then materialize final merged boxes and closing borders from the stable
    // physical row heights.
    for (row_index, row_roles) in roles.iter().enumerate() {
        for (cell_index, role) in row_roles.iter().copied().enumerate() {
            let VerticalMergeRole::Restart { end_row, end_cell } = role else {
                continue;
            };
            if end_row == row_index {
                continue;
            }
            let merged_height = Twip(
                rows[row_index..=end_row]
                    .iter()
                    .map(|row| row.height.raw())
                    .sum(),
            );
            let (closing_bottom, closing_bottom_segments, table_bottom, closing_spacing_bottom) = {
                let closing = &rows[end_row].cells[end_cell].borders;
                let closing_cell = &rows[end_row].cells[end_cell];
                (
                    closing.bottom,
                    closing.bottom_segments.clone(),
                    closing_cell.table_borders.bottom,
                    closing_cell.cell_spacing.bottom,
                )
            };
            let origin = &mut rows[row_index].cells[cell_index];
            origin.vertical_merge = CellVerticalMerge::Restart {
                height: merged_height,
            };
            origin.borders.bottom = closing_bottom;
            origin.borders.bottom_segments = closing_bottom_segments;
            origin.table_borders.bottom = table_bottom;
            origin.cell_spacing.bottom = closing_spacing_bottom;
        }
    }
}

/// Whether a row has an authored exact height that merge constraints must not
/// grow.
fn row_has_exact_height(props: &TableRowProperties) -> bool {
    props.height.value_twips.is_some() && matches!(props.height.rule, Some(HeightRule::Exact))
}

/// Word's built-in default cell content margin for the leading/trailing edges
/// (`w:tblCellMar` when the document declares none): 108 twips (0.075"). The
/// top/bottom default is zero.
const DEFAULT_CELL_MARGIN_LR: i32 = 108;

/// Resolves a cell's effective content margins in twips: for each edge, the
/// cell's own `w:tcMar` wins, else the table's `w:tblCellMar`, else Word's default
/// (108 twips left/right, 0 top/bottom). This is the inset from each cell edge to
/// its content box (`docs/38-…#tables`).
fn resolve_cell_margins(
    cell: &casual_doc_model::v1::CellMargins,
    table: &casual_doc_model::v1::CellMargins,
) -> CellContentMargins {
    let pick =
        |c: Option<i32>, t: Option<i32>, default: i32| Twip(c.or(t).unwrap_or(default).max(0));
    CellContentMargins {
        top: pick(cell.top_twips, table.top_twips, 0),
        start: pick(cell.start_twips, table.start_twips, DEFAULT_CELL_MARGIN_LR),
        bottom: pick(cell.bottom_twips, table.bottom_twips, 0),
        end: pick(cell.end_twips, table.end_twips, DEFAULT_CELL_MARGIN_LR),
    }
}

/// Maps a cell's `w:vAlign` to the layout enum (`Top` when unset — Word's
/// default).
fn cell_vertical_alignment(props: &casual_doc_model::v1::TableCellProperties) -> CellVAlign {
    match props.vertical_alignment {
        Some(casual_doc_model::v1::CellVerticalAlignment::Center) => CellVAlign::Center,
        Some(casual_doc_model::v1::CellVerticalAlignment::Bottom) => CellVAlign::Bottom,
        Some(casual_doc_model::v1::CellVerticalAlignment::Top) | None => CellVAlign::Top,
    }
}

/// The resolved height of a table row and whether its content must be clipped,
/// per the `w:trHeight` rule. `atLeast`/`auto` grow to the content when it is
/// taller; `exact` pins the height and clips overflow (`docs/38-…#tables`).
fn resolve_row_height(props: &TableRowProperties, content: Twip) -> (Twip, bool) {
    let val = props.height.value_twips.map_or(0, |v| v as i32);
    match props.height.rule {
        Some(HeightRule::Exact) if props.height.value_twips.is_some() => {
            (Twip(val), content.raw() > val)
        }
        // `atLeast`, `auto`, and an `exact` with no value all resolve to "at
        // least this tall": the larger of the content height and the stated value.
        _ => (Twip(content.raw().max(val)), false),
    }
}

/// A table column's width constraints, the input to [`solve_column_widths`].
#[derive(Clone, Copy, Debug)]
struct ColumnConstraint {
    /// Declared grid width (`w:gridCol`), if any.
    grid: Option<i32>,
    /// Minimum content width — the widest run that cannot be broken (twips).
    min: i32,
    /// Preferred content width — the natural (unwrapped) width (twips).
    preferred: i32,
}

/// A table's preferred-width specification (`w:tblW`). The v1 model stores
/// `w:tblW` as `dxa` only, so the flow path emits `Auto`/`Dxa`; `Pct` (resolved
/// against the containing width) is supported and tested at the solver seam so
/// the algorithm is complete when the model gains percentage widths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WidthSpec {
    /// No preferred width — size to content within the available width.
    Auto,
    /// A fixed width in twips.
    Dxa(i32),
    /// A percentage of the containing width, in fiftieths of a percent
    /// (`5000` = 100%). The v1 model stores `w:tblW` as `dxa` only, so the flow
    /// path never emits this today; the solver implements it (and it is covered
    /// by tests) so percentage widths are ready the moment the model carries them.
    #[allow(dead_code)]
    Pct(u32),
}

/// Solves the grid column widths for a table (twips), measuring each cell's
/// content min/preferred widths with the shaper and distributing `w:gridSpan`
/// cells across the columns they cover.
fn solve_table_columns(
    table: &Table,
    shaper: &dyn LineShaper,
    available: i32,
    ctx: &FlowCtx,
    merge_roles: &[Vec<VerticalMergeRole>],
    style_layers: &[Vec<TableStyleLayer>],
) -> Vec<Twip> {
    let ncols = if table.grid.is_empty() {
        table
            .rows
            .iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| c.properties.grid_span.unwrap_or(1).max(1) as usize)
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(1)
            .max(1)
    } else {
        table.grid.len()
    };

    let mut cols: Vec<ColumnConstraint> = (0..ncols)
        .map(|i| ColumnConstraint {
            grid: table.grid.get(i).and_then(|g| g.width_twips),
            min: 0,
            preferred: 0,
        })
        .collect();

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut col = 0usize;
        for (cell_index, cell) in row.cells.iter().enumerate() {
            let span = cell.properties.grid_span.unwrap_or(1).max(1) as usize;
            if !matches!(
                merge_roles[row_index][cell_index],
                VerticalMergeRole::Continue
            ) {
                let table_style = table
                    .properties
                    .style_ref
                    .is_some()
                    .then_some(&style_layers[row_index][cell_index]);
                let (cmin, cmax) = block_intrinsic(&cell.blocks, shaper, ctx, table_style);
                let cpref = cmax.max(cell.properties.width_twips.unwrap_or(0));
                // A spanning cell's demand is shared over the columns it covers.
                let per_min = div_ceil(cmin, span as i32);
                let per_pref = div_ceil(cpref, span as i32);
                for c in cols.iter_mut().skip(col).take(span) {
                    c.min = c.min.max(per_min);
                    c.preferred = c.preferred.max(per_pref);
                }
            }
            col += span;
        }
    }
    for c in &mut cols {
        if let Some(g) = c.grid {
            c.preferred = c.preferred.max(g);
        }
        c.preferred = c.preferred.max(c.min);
    }

    let spec = table
        .properties
        .width_twips
        .map_or(WidthSpec::Auto, WidthSpec::Dxa);
    let layout = match table.properties.layout {
        Some(TableLayout::Fixed) => TableLayout::Fixed,
        _ => TableLayout::Autofit,
    };
    solve_column_widths(&cols, spec, available, layout)
        .into_iter()
        .map(Twip)
        .collect()
}

/// The column-width solver: resolves a table's target width from `spec`, seeds a
/// base width per column (the grid for fixed layout, the preferred content width
/// for autofit), then grows or shrinks the columns to hit the target while never
/// shrinking a column below its content minimum when the target allows.
fn solve_column_widths(
    cols: &[ColumnConstraint],
    spec: WidthSpec,
    available: i32,
    layout: TableLayout,
) -> Vec<i32> {
    let n = cols.len();
    if n == 0 {
        return Vec::new();
    }
    let available = available.max(1);
    let grid_sum: i32 = cols.iter().map(|c| c.grid.unwrap_or(0)).sum();
    let pref_sum: i32 = cols.iter().map(|c| c.preferred.max(1)).sum();
    let min_sum: i32 = cols.iter().map(|c| c.min.max(1)).sum();

    let target = match spec {
        WidthSpec::Dxa(v) => v.max(1),
        WidthSpec::Pct(p) => ((i64::from(available) * i64::from(p)) / 5000).max(1) as i32,
        WidthSpec::Auto => match layout {
            TableLayout::Fixed if grid_sum > 0 => grid_sum,
            TableLayout::Fixed => available,
            // Autofit sizes to the content, but never wider than the available
            // width nor narrower than the content minimum.
            TableLayout::Autofit => pref_sum.clamp(min_sum, available.max(min_sum)),
        },
    };

    let base: Vec<i32> = match layout {
        TableLayout::Fixed => {
            let undeclared = cols.iter().filter(|c| c.grid.is_none()).count();
            let each = if undeclared > 0 {
                ((available - grid_sum).max(0) / undeclared as i32).max(1)
            } else {
                0
            };
            cols.iter().map(|c| c.grid.unwrap_or(each).max(1)).collect()
        }
        TableLayout::Autofit => cols.iter().map(|c| c.preferred.max(1)).collect(),
    };
    let mins: Vec<i32> = cols.iter().map(|c| c.min.max(1)).collect();
    distribute_width(base, &mins, target)
}

/// Distributes `base` column widths to sum to `target`: extra space is shared in
/// proportion to each column's base width; a deficit is taken first from each
/// column's slack above its minimum (in proportion to that slack), and only if
/// that is not enough are columns shrunk below their minimums proportionally.
fn distribute_width(base: Vec<i32>, mins: &[i32], target: i32) -> Vec<i32> {
    let n = base.len();
    let sum: i32 = base.iter().sum();
    let mut out = base.clone();
    if n == 0 || sum == target {
        for v in &mut out {
            *v = (*v).max(1);
        }
        return out;
    }
    if sum < target {
        let extra = target - sum;
        let wsum: i64 = base.iter().map(|&b| i64::from(b.max(1))).sum();
        let mut given = 0;
        for i in 0..n {
            let add = if i == n - 1 {
                extra - given
            } else {
                ((i64::from(extra) * i64::from(base[i].max(1))) / wsum) as i32
            };
            out[i] += add;
            given += add;
        }
    } else {
        let deficit = sum - target;
        let slack: Vec<i32> = (0..n).map(|i| (base[i] - mins[i]).max(0)).collect();
        let slack_sum: i64 = slack.iter().map(|&s| i64::from(s)).sum();
        if slack_sum >= i64::from(deficit) && slack_sum > 0 {
            let last = (0..n).rev().find(|&i| slack[i] > 0).unwrap();
            let mut taken = 0;
            for i in 0..n {
                if slack[i] == 0 {
                    continue;
                }
                let sub = if i == last {
                    deficit - taken
                } else {
                    ((i64::from(deficit) * i64::from(slack[i])) / slack_sum) as i32
                };
                out[i] -= sub;
                taken += sub;
            }
        } else {
            let wsum: i64 = base.iter().map(|&b| i64::from(b.max(1))).sum();
            let mut taken = 0;
            for i in 0..n {
                let sub = if i == n - 1 {
                    deficit - taken
                } else {
                    ((i64::from(deficit) * i64::from(base[i].max(1))) / wsum) as i32
                };
                out[i] -= sub;
                taken += sub;
            }
        }
    }
    for v in &mut out {
        *v = (*v).max(1);
    }
    out
}

/// Ceiling of `a / b` for non-negative integers (`b >= 1`).
fn div_ceil(a: i32, b: i32) -> i32 {
    (a + b - 1) / b
}

/// A cell's intrinsic content widths `(min, preferred)` in twips: `min` is the
/// widest run or inline box that cannot be line-broken (measured at a 1-twip
/// width); `preferred` is the natural, unwrapped width (measured at an effectively
/// unbounded width). Nested tables contribute their declared grid width.
///
/// Runs are resolved to their concrete face and modeled inline objects use the
/// same `FlowItem` projection as final flow, but into a throwaway report —
/// measurement is internal and must not inflate the substitution counts surfaced
/// for the rendered galley, which the flow pass records once per run.
fn block_intrinsic(
    blocks: &[BlockNode],
    shaper: &dyn LineShaper,
    ctx: &FlowCtx,
    table_style: Option<&TableStyleLayer>,
) -> (i32, i32) {
    let mut scratch = FontResolutionReport::new();
    let mut mctx = FlowCtx {
        resolver: ctx.resolver,
        scheme: ctx.scheme,
        report: &mut scratch,
        default_tab: ctx.default_tab,
        media: ctx.media,
        palette: ctx.palette,
        cascade: ctx.cascade,
        para_style: ctx.para_style,
        table_style: table_style.cloned(),
        // Intrinsic width measurement never paginates, so section breaks are moot.
        sections: &[],
        definitions: ctx.definitions,
        // A throwaway counter state: measuring intrinsic widths must not advance the
        // document's real list counters.
        numbering: NumberingState::default(),
        text_scale: ctx.text_scale,
        line_spacing_reduction: ctx.line_spacing_reduction,
        paragraph_float_exclusions: None,
    };
    let mut min = 0;
    let mut preferred = 0;
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                mctx.para_style = mctx.cascade.paragraph_style(&paragraph.properties);
                let props = mctx
                    .cascade
                    .resolve_paragraph_in_table(&paragraph.properties, mctx.table_style.as_ref());
                let range = ModelRange::new(
                    ModelPos::new(paragraph.id, 0),
                    ModelPos::new(paragraph.id, 0),
                );
                let mut narrow_items = Vec::new();
                collect_items_with_measure(
                    &paragraph.inlines,
                    &mut narrow_items,
                    shaper,
                    Twip(1),
                    &mut mctx,
                    Some(IntrinsicPass::Minimum),
                );
                let narrow = shape_paragraph_items(
                    shaper,
                    &narrow_items,
                    &props.tabs,
                    mctx.default_tab,
                    LineConstraints {
                        max_width: Twip(1),
                        ..LineConstraints::default()
                    },
                    range,
                );
                let mut wide_items = Vec::new();
                collect_items_with_measure(
                    &paragraph.inlines,
                    &mut wide_items,
                    shaper,
                    Twip(1_000_000),
                    &mut mctx,
                    Some(IntrinsicPass::Preferred),
                );
                let wide = shape_paragraph_items(
                    shaper,
                    &wide_items,
                    &props.tabs,
                    mctx.default_tab,
                    LineConstraints {
                        max_width: Twip(1_000_000),
                        ..LineConstraints::default()
                    },
                    range,
                );
                min = min.max(max_line_width(&narrow));
                preferred = preferred.max(max_line_width(&wide));
            }
            BlockNode::Table(table) => {
                let grid: i32 = table.grid.iter().filter_map(|c| c.width_twips).sum();
                min = min.max(grid);
                preferred = preferred.max(grid);
            }
            BlockNode::Sdt(sdt) => {
                // Transparent wrapper: its children contribute their own
                // intrinsic widths, mirroring how they flow.
                let (m, p) = block_intrinsic(&sdt.blocks, shaper, ctx, table_style);
                min = min.max(m);
                preferred = preferred.max(p);
            }
            // TODO(altchunk): the chunk's real embedded content contributes no
            // measurable width (nothing is modeled to measure); the placeholder
            // box (see `alt_chunk_fragment`) contributes its own label's
            // intrinsic width instead, mirroring the paragraph branch above.
            BlockNode::AltChunk(chunk) => {
                mctx.para_style = None;
                let run = styled_run(
                    ALT_CHUNK_PLACEHOLDER_TEXT,
                    &RunProperties::default(),
                    &mut mctx,
                );
                let range = ModelRange::new(ModelPos::new(chunk.id, 0), ModelPos::new(chunk.id, 0));
                let narrow = shaper.shape_paragraph(
                    std::slice::from_ref(&run),
                    LineConstraints {
                        max_width: Twip(1),
                        ..LineConstraints::default()
                    },
                    range,
                );
                let wide = shaper.shape_paragraph(
                    &[run],
                    LineConstraints {
                        max_width: Twip(1_000_000),
                        ..LineConstraints::default()
                    },
                    range,
                );
                min = min.max(max_line_width(&narrow));
                preferred = preferred.max(max_line_width(&wide));
            }
        }
    }
    (min, preferred)
}

/// The widest line in a shaped paragraph (twips) — a run's right edge is its
/// origin plus the sum of its glyph advances.
fn max_line_width(layout: &crate::text::LineLayout) -> i32 {
    layout
        .lines
        .iter()
        .map(|line| {
            let run_edge = line
                .runs
                .iter()
                .map(|run| {
                    run.glyphs.iter().fold(run.origin.x.raw(), |edge, glyph| {
                        edge.saturating_add(glyph.advance.raw())
                    })
                })
                .max()
                .unwrap_or(0);
            let image_edge = line
                .images
                .iter()
                .map(|image| image.origin.x.raw().saturating_add(image.size.width.raw()))
                .max()
                .unwrap_or(0);
            let text_box_edge = line
                .text_boxes
                .iter()
                .map(|text_box| {
                    text_box
                        .origin
                        .x
                        .raw()
                        .saturating_add(text_box.size.width.raw())
                })
                .max()
                .unwrap_or(0);
            let rule_edge = line
                .rules
                .iter()
                .map(|rule| rule.origin.x.raw().saturating_add(rule.size.width.raw()))
                .max()
                .unwrap_or(0);
            run_edge.max(image_edge).max(text_box_edge).max(rule_edge)
        })
        .max()
        .unwrap_or(0)
}

/// Resolves a cell's four visible borders by OOXML conflict precedence
/// (ECMA-376 §17.4.66): first derive each cell side from its direct border or the
/// applicable table fallback, then resolve a zero-spacing conflict with the
/// abutting cell side. Interior edges use `w:insideH`/`w:insideV`; outer edges
/// use the table perimeter. `None` means no visible border.
///
/// A horizontally spanning cell can abut several cells in the preceding or
/// following row. The compact whole-side winner is retained for compatibility,
/// while top/bottom segment lists resolve each abutting grid interval
/// independently for composition.
fn resolve_cell_borders(
    rows: &[TableRow],
    effective: &[Vec<TableBorders>],
    row_index: usize,
    index: usize,
    column_edges: &[i32],
) -> CellBorders {
    let row = &rows[row_index].cells;
    let own = &effective[row_index][index];
    let left = index.checked_sub(1).map(|i| &effective[row_index][i]);
    let right = effective[row_index].get(index + 1);
    let (start_col, end_col) = cell_column_range(row, index);

    // Candidate order follows reading order so exact ties keep the first edge:
    // row above before current, current before row below; left before right.
    let mut top_candidates = Vec::new();
    if let Some(above) = row_index.checked_sub(1).and_then(|i| rows.get(i)) {
        push_overlapping_edges(
            &mut top_candidates,
            &above.cells,
            &effective[row_index - 1],
            start_col,
            end_col,
            |borders| borders.bottom.as_ref(),
        );
    }
    top_candidates.push(own.top.as_ref());

    let mut bottom_candidates = vec![own.bottom.as_ref()];
    if let Some(below) = rows.get(row_index + 1) {
        push_overlapping_edges(
            &mut bottom_candidates,
            &below.cells,
            &effective[row_index + 1],
            start_col,
            end_col,
            |borders| borders.top.as_ref(),
        );
    }

    let own_start = own.start.as_ref();
    let left_end = left.and_then(|borders| borders.end.as_ref());
    let own_end = own.end.as_ref();
    let right_start = right.and_then(|borders| borders.start.as_ref());

    let top_segments = row_index
        .checked_sub(1)
        .and_then(|i| rows.get(i))
        .map_or_else(Vec::new, |above| {
            resolve_horizontal_segments(
                own.top.as_ref(),
                &above.cells,
                &effective[row_index - 1],
                |borders| borders.bottom.as_ref(),
                start_col,
                end_col,
                column_edges,
                false,
            )
        });
    let bottom_segments = rows.get(row_index + 1).map_or_else(Vec::new, |below| {
        resolve_horizontal_segments(
            own.bottom.as_ref(),
            &below.cells,
            &effective[row_index + 1],
            |borders| borders.top.as_ref(),
            start_col,
            end_col,
            column_edges,
            true,
        )
    });

    CellBorders {
        top: resolve_edge(&top_candidates),
        bottom: resolve_edge(&bottom_candidates),
        start: resolve_edge(&[left_end, own_start]),
        end: resolve_edge(&[own_end, right_start]),
        top_segments,
        bottom_segments,
    }
}

/// Resolves one cell's already materialized sides without consulting adjacent
/// cells. This is the non-zero-spacing mode: each side remains visible in its
/// own inset cell box instead of collapsing to one shared winner.
fn resolve_separated_cell_borders(effective: &TableBorders) -> CellBorders {
    CellBorders {
        top: resolve_edge(&[effective.top.as_ref()]),
        start: resolve_edge(&[effective.start.as_ref()]),
        bottom: resolve_edge(&[effective.bottom.as_ref()]),
        end: resolve_edge(&[effective.end.as_ref()]),
        top_segments: Vec::new(),
        bottom_segments: Vec::new(),
    }
}

/// Resolves every independently abutting interval along one horizontal cell
/// side. Segment geometry is final twip geometry relative to the current cell.
#[allow(clippy::too_many_arguments)]
fn resolve_horizontal_segments<'a>(
    own: Option<&'a BorderEdge>,
    adjacent: &'a [TableCell],
    adjacent_borders: &'a [TableBorders],
    adjacent_side: impl Fn(&'a TableBorders) -> Option<&'a BorderEdge>,
    start_col: usize,
    end_col: usize,
    column_edges: &[i32],
    own_first: bool,
) -> Vec<ResolvedBorderSegment> {
    let mut breaks = vec![start_col, end_col];
    let mut adjacent_ranges = Vec::new();
    let mut cell_start = 0usize;
    for (cell, borders) in adjacent.iter().zip(adjacent_borders) {
        let span = cell.properties.grid_span.unwrap_or(1).max(1) as usize;
        let cell_end = cell_start.saturating_add(span);
        if cell_start < end_col && start_col < cell_end {
            let overlap_start = cell_start.max(start_col);
            let overlap_end = cell_end.min(end_col);
            breaks.push(overlap_start);
            breaks.push(overlap_end);
            adjacent_ranges.push((overlap_start, overlap_end, adjacent_side(borders)));
        }
        cell_start = cell_end;
        if cell_start >= end_col {
            break;
        }
    }
    breaks.sort_unstable();
    breaks.dedup();

    let coordinate = |column: usize| {
        column_edges
            .get(column)
            .copied()
            .or_else(|| column_edges.last().copied())
            .unwrap_or(0)
    };
    let cell_start_x = coordinate(start_col);
    let mut segments: Vec<ResolvedBorderSegment> = Vec::new();
    for window in breaks.windows(2) {
        let (segment_start, segment_end) = (window[0], window[1]);
        if segment_start >= segment_end {
            continue;
        }
        let mut candidates = Vec::new();
        if own_first {
            candidates.push(own);
        }
        candidates.extend(
            adjacent_ranges
                .iter()
                .filter(|(start, end, _)| *start < segment_end && segment_start < *end)
                .map(|(_, _, edge)| *edge),
        );
        if !own_first {
            candidates.push(own);
        }
        let Some(edge) = resolve_edge(&candidates) else {
            continue;
        };
        let offset = Twip(coordinate(segment_start) - cell_start_x);
        let length = Twip((coordinate(segment_end) - coordinate(segment_start)).max(0));
        if length.is_zero() {
            continue;
        }
        if let Some(previous) = segments.last_mut()
            && previous.edge == edge
            && previous.offset + previous.length == offset
        {
            previous.length = previous.length + length;
        } else {
            segments.push(ResolvedBorderSegment {
                offset,
                length,
                edge,
            });
        }
    }
    segments
}

/// Returns the half-open grid-column range occupied by `row[index]`.
fn cell_column_range(row: &[TableCell], index: usize) -> (usize, usize) {
    let start = row
        .iter()
        .take(index)
        .map(|cell| cell.properties.grid_span.unwrap_or(1).max(1) as usize)
        .sum::<usize>();
    let span = row[index].properties.grid_span.unwrap_or(1).max(1) as usize;
    (start, start.saturating_add(span))
}

/// Adds the effective side of every cell whose grid-column range overlaps the
/// current cell. This handles ordinary rows as well as different `w:gridSpan`
/// partitions above and below the edge.
fn push_overlapping_edges<'a>(
    candidates: &mut Vec<Option<&'a BorderEdge>>,
    row: &'a [TableCell],
    borders: &'a [TableBorders],
    start_col: usize,
    end_col: usize,
    side: impl Fn(&'a TableBorders) -> Option<&'a BorderEdge>,
) {
    let mut cell_start = 0usize;
    for (cell, borders) in row.iter().zip(borders) {
        let span = cell.properties.grid_span.unwrap_or(1).max(1) as usize;
        let cell_end = cell_start.saturating_add(span);
        if cell_start < end_col && start_col < cell_end {
            candidates.push(side(borders));
        }
        cell_start = cell_end;
        if cell_start >= end_col {
            break;
        }
    }
}

/// Derives one cell side before adjacent-cell conflict resolution. A direct
/// visible border (including `nil`, which explicitly suppresses the edge) wins;
/// omitted/`none` direct borders fall back to the applicable table edge.
fn effective_border<'a>(
    cell: Option<&'a BorderEdge>,
    table: Option<&'a BorderEdge>,
) -> Option<&'a BorderEdge> {
    match cell {
        Some(edge) if matches!(edge.style.as_str(), "" | "none") => table,
        Some(edge) => Some(edge),
        None => table,
    }
}

/// Picks the highest-precedence border among `candidates` and converts it to a
/// drawable [`ResolvedEdge`] (or `None` if none is visible). An explicit `nil`
/// suppresses the conflicting edge. Exact ranking ties keep the first candidate
/// in reading order.
pub(crate) fn resolve_edge(candidates: &[Option<&BorderEdge>]) -> Option<ResolvedEdge> {
    let mut winner: Option<&BorderEdge> = None;
    for edge in candidates.iter().filter_map(|candidate| *candidate) {
        if edge.style == "nil" {
            return None;
        }
        if !is_visible_border(edge) {
            continue;
        }
        if winner.is_none_or(|current| border_rank(edge) > border_rank(current)) {
            winner = Some(edge);
        }
    }
    let winner = winner?;
    let color = winner
        .color
        .map_or([0, 0, 0, 255], |c| [c.r, c.g, c.b, 255]);
    // `w:sz` is in eighths of a point; a point is 20 twips.
    let width = winner
        .size_eighth_points
        .map_or(Twip(10), |sz| Twip(((sz * 20) / 8).max(1) as i32));
    Some(ResolvedEdge {
        color,
        width,
        pattern: border_pattern(&winner.style),
    })
}

/// Maps the common OOXML line-style families to backend-independent patterns.
/// Producer-specific and art-border tokens intentionally use the solid fallback.
fn border_pattern(style: &str) -> BorderPattern {
    match style {
        "double" => BorderPattern::Double,
        "dotted" => BorderPattern::Dotted,
        "dashed" | "dashSmallGap" => BorderPattern::Dashed,
        "dotDash" | "dashDotStroked" => BorderPattern::DotDash,
        "dotDotDash" => BorderPattern::DotDotDash,
        _ => BorderPattern::Solid,
    }
}

/// Whether a border edge is a visible line (not `nil`/`none`/empty).
fn is_visible_border(edge: &BorderEdge) -> bool {
    !matches!(edge.style.as_str(), "" | "nil" | "none")
}

/// The border-conflict ranking key (higher wins): a visible border beats none,
/// then wider beats narrower, then a higher style rank, then a darker color.
fn border_rank(edge: &BorderEdge) -> (u32, u32, u32) {
    let width = edge.size_eighth_points.unwrap_or(0);
    let style: u32 = match edge.style.as_str() {
        "double" => 3,
        "single" => 2,
        "dashed" | "dotted" | "dotDash" | "dashDotStroked" => 1,
        _ => 0,
    };
    // Darker colors win ties: rank by inverse luminance (absent color = black).
    let luminance = edge
        .color
        .map_or(0, |c| u32::from(c.r) + u32::from(c.g) + u32::from(c.b));
    (width, style, 765 - luminance)
}

/// The plain text a paragraph's inline nodes contribute, in the **same byte
/// layout** the shaper indexes — so a UTF-8 byte offset from hit-testing
/// (`crate::hittest`, `ModelPos::offset`) slices this string at the caret the
/// user clicked. This is the authoritative source for copy-in-view: it mirrors
/// the text-bearing flow contributions exactly (`Run` text, `Tab` → `\t`,
/// `Symbol` → its code point, and recursion through visible
/// `Hyperlink`/`Revision`/`Sdt` wrappers); every other inline kind contributes
/// no shaped text bytes.
#[must_use]
pub fn node_plain_text(inlines: &[InlineNode]) -> String {
    node_plain_text_with_projection(inlines, ReviewProjection::FinalWithMarkup)
}

/// The plain text contributed by `inlines` under an explicit review projection.
///
/// Runtime editing uses [`ReviewProjection::FinalWithMarkup`]; the parameterized
/// form keeps the Original/Final contract testable without enabling an unsafe
/// view switcher before position mapping and editing policy are implemented.
#[must_use]
pub fn node_plain_text_with_projection(
    inlines: &[InlineNode],
    projection: ReviewProjection,
) -> String {
    let mut out = String::new();
    append_node_plain_text(inlines, projection, &mut out);
    out
}

fn append_node_plain_text(inlines: &[InlineNode], projection: ReviewProjection, out: &mut String) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => out.push_str(&run.text),
            InlineNode::Tab(_) => out.push('\t'),
            InlineNode::Symbol(symbol) => {
                if let Some(ch) = char::from_u32(symbol.char) {
                    out.push(ch);
                }
            }
            InlineNode::Hyperlink(hyperlink) => {
                append_node_plain_text(&hyperlink.inlines, projection, out);
            }
            InlineNode::Revision(revision) if revision.kind.contributes_to(projection) => {
                append_node_plain_text(&revision.inlines, projection, out);
            }
            InlineNode::Revision(_) => {}
            InlineNode::Sdt(sdt) => append_node_plain_text(&sdt.inlines, projection, out),
            _ => {}
        }
    }
}

/// Flattens a paragraph's inline nodes into a [`FlowItem`] stream — styled runs
/// interleaved with the explicit tabs (`w:tab`) and hard breaks (`w:br`/`w:cr`)
/// that control horizontal advance and forced lines. Tabs and breaks are
/// preserved as first-class items so the tab/break layer can resolve them;
/// recursion through wrappers matches final visible flow.
fn collect_items<'a>(
    inlines: &'a [InlineNode],
    out: &mut Vec<FlowItem<'a>>,
    shaper: &dyn LineShaper,
    width: Twip,
    ctx: &mut FlowCtx,
) {
    collect_items_with_measure(inlines, out, shaper, width, ctx, None);
}

#[derive(Clone, Copy)]
enum IntrinsicPass {
    Minimum,
    Preferred,
}

fn collect_items_with_measure<'a>(
    inlines: &'a [InlineNode],
    out: &mut Vec<FlowItem<'a>>,
    shaper: &dyn LineShaper,
    width: Twip,
    ctx: &mut FlowCtx,
    intrinsic: Option<IntrinsicPass>,
) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => {
                let mut styled = Vec::new();
                push_styled_runs(&run.text, &run.properties, ctx, &mut styled);
                out.extend(styled.into_iter().map(FlowItem::Run));
            }
            InlineNode::Tab(_) => out.push(FlowItem::Tab),
            InlineNode::PositionalTab(tab) => out.push(FlowItem::PositionalTab {
                alignment: tab.alignment,
                relative_to: tab.relative_to,
                leader: tab.leader,
            }),
            InlineNode::Symbol(symbol) => out.push(FlowItem::Run(symbol_glyph_run(symbol, ctx))),
            InlineNode::Break(node) => out.push(FlowItem::Break(node.kind)),
            InlineNode::Drawing(drawing) => {
                if let Some(item) = image_item(drawing, ctx) {
                    out.push(item);
                }
            }
            InlineNode::EmbeddedObject(object) => embedded_object_items(object, out, ctx),
            InlineNode::AnchoredDrawing(drawing) => {
                if intrinsic.is_none()
                    && let Some(item) = float_flow_item(&drawing.anchor, &drawing.extent)
                {
                    out.push(item);
                }
            }
            InlineNode::Field(field) => out.push(field_item(field, ctx)),
            InlineNode::HorizontalRule(rule) => {
                let rule_width = if intrinsic.is_some() { Twip(1) } else { width };
                out.push(hr_item(rule, rule_width));
            }
            // A floating text box (`anchor` set) is removed from the inline flow —
            // it is placed absolutely by the float layer ([`crate::anchor`]). Only
            // an inline text box flows here.
            InlineNode::TextBox(text_box) if text_box.anchor.is_none() => {
                let box_width = intrinsic
                    .map(|pass| intrinsic_text_box_width(text_box, shaper, ctx, pass))
                    .unwrap_or(width);
                out.push(textbox_item(text_box, shaper, box_width, ctx))
            }
            InlineNode::TextBox(text_box) => {
                if intrinsic.is_none()
                    && let (Some(anchor), Some(extent)) = (&text_box.anchor, &text_box.extent)
                    && let Some(item) = float_flow_item(anchor, extent)
                {
                    out.push(item);
                }
            }
            InlineNode::Group(group) => {
                if intrinsic.is_none()
                    && let Some(anchor) = &group.anchor
                    && let Some(item) = float_flow_item(anchor, &group.extent)
                {
                    out.push(item);
                }
            }
            InlineNode::Hyperlink(hyperlink) => {
                collect_items_with_measure(&hyperlink.inlines, out, shaper, width, ctx, intrinsic)
            }
            InlineNode::NoteReference(reference) => {
                out.push(FlowItem::NoteReference(NoteMarker {
                    kind: reference.kind,
                    note: reference.note,
                }));
                out.push(FlowItem::Run(note_reference_run(reference, ctx)));
            }
            // `w:commentReference` is a zero-width model marker. Its visible
            // affordance belongs to the host review UI; shaping placeholder
            // text here changes line wrapping and leaks a superscript
            // "[comment]" glyph into the document canvas.
            InlineNode::CommentReference(_) => {}
            InlineNode::Revision(revision)
                if revision
                    .kind
                    .contributes_to(ReviewProjection::FinalWithMarkup) =>
            {
                collect_items_with_measure(&revision.inlines, out, shaper, width, ctx, intrinsic)
            }
            InlineNode::Revision(_) => {}
            InlineNode::Sdt(sdt) => {
                // A native checkbox content control (`w14:checkbox`) declares its
                // checked/unchecked glyphs, but producers routinely leave the
                // cached `sdtContent` run at the unchecked box even when
                // `w14:checked=1`, so the state must drive the painted glyph
                // (docs/60 §8 "Control appearance"). Otherwise the SDT is a
                // transparent range wrapper and recurses.
                if let Some(glyph) = sdt_checkbox_glyph_run(sdt, ctx) {
                    out.push(FlowItem::Run(glyph));
                } else {
                    collect_items_with_measure(&sdt.inlines, out, shaper, width, ctx, intrinsic);
                }
            }
            InlineNode::Math(math) => {
                let base = styled_owned_run("x".to_owned(), &RunProperties::default(), ctx);
                if let Some(expression) = &math.expression
                    && let Some(math_box) = layout_math_expression(shaper, expression, &base, 1_000)
                {
                    out.push(FlowItem::Math {
                        size: math_box.size,
                        runs: math_box.runs,
                        rules: math_box.rules,
                    });
                } else {
                    let text = if math.text.is_empty() {
                        "[equation]".to_owned()
                    } else {
                        format!("[{}]", math.text)
                    };
                    out.push(FlowItem::Run(styled_owned_run(
                        text,
                        &RunProperties::default(),
                        ctx,
                    )));
                }
            }
            InlineNode::NoBreakHyphen(_) => {
                out.push(FlowItem::Run(styled_run(
                    "\u{2011}",
                    &RunProperties::default(),
                    ctx,
                )));
            }
            InlineNode::SoftHyphen(_) => {
                out.push(FlowItem::Run(styled_run(
                    "\u{00ad}",
                    &RunProperties::default(),
                    ctx,
                )));
            }
            _ => {}
        }
    }
}

fn embedded_object_items<'a>(
    object: &EmbeddedObject,
    out: &mut Vec<FlowItem<'a>>,
    ctx: &mut FlowCtx,
) {
    if let Some(preview) = object.preview
        && let Some(media) = ctx.media.get(&preview)
    {
        let size = extent_to_size(&object.extent);
        if size.width.raw() > 0 && size.height.raw() > 0 {
            out.push(FlowItem::Image {
                media: media.part_name.clone(),
                size,
                crop: None,
            });
            return;
        }
    }

    out.push(FlowItem::Run(styled_owned_run(
        embedded_object_label(object).to_owned(),
        &RunProperties::default(),
        ctx,
    )));
}

fn embedded_object_label(object: &EmbeddedObject) -> &'static str {
    match &object.kind {
        EmbeddedKind::Chart => "[chart]",
        EmbeddedKind::Diagram => "[diagram]",
        EmbeddedKind::OleObject => "[object]",
        EmbeddedKind::Other(_) => "[object]",
    }
}

fn note_reference_run(reference: &NoteReference, ctx: &mut FlowCtx) -> StyledRun<'static> {
    let ordinal = match reference.kind {
        NoteKind::Footnote => note_ordinal(&ctx.definitions.footnotes, reference.note),
        NoteKind::Endnote => note_ordinal(&ctx.definitions.endnotes, reference.note),
    };
    let text = ordinal.map_or_else(|| "?".to_owned(), |n| n.to_string());
    styled_owned_run(text, &note_reference_properties(), ctx)
}

fn note_ordinal<V>(
    notes: &DefinitionMap<casual_doc_model::v1::NoteId, V>,
    note: casual_doc_model::v1::NoteId,
) -> Option<usize> {
    notes
        .iter()
        .position(|(id, _)| *id == note)
        .map(|index| index + 1)
}

fn note_reference_properties() -> RunProperties {
    RunProperties {
        vertical_alignment: Some(VerticalAlignment::Superscript),
        ..RunProperties::default()
    }
}

/// Converts a paragraph-local anchored object into its non-painting flow marker.
/// Top-and-bottom wrapping reserves vertical space; left/right square-family
/// wrapping narrows only lines that intersect the object's vertical clearance.
/// Page-relative and cross-paragraph exclusions remain outside this local slice.
fn float_flow_item(anchor: &DrawingAnchor, extent: &Extent) -> Option<FlowItem<'static>> {
    if !matches!(
        anchor.vertical.relative_from,
        VerticalAnchor::Paragraph | VerticalAnchor::Line
    ) {
        return None;
    }
    let offset_emu = match anchor.vertical.position {
        VerticalPosition::Offset(offset) => offset,
        VerticalPosition::Align(casual_doc_model::v1::VerticalAlign::Top) => 0,
        _ => return None,
    };
    let clearance_emu = offset_emu
        .saturating_add(extent.height_emu)
        .saturating_add(anchor.wrap_distances.bottom_emu)
        .max(0);
    let height = emu_to_twip(clearance_emu);
    if height.raw() <= 0 {
        return None;
    }
    if anchor.wrap == WrapMode::TopAndBottom {
        return Some(FlowItem::FloatBarrier { height });
    }
    if !matches!(
        anchor.wrap,
        WrapMode::Square | WrapMode::Tight | WrapMode::Through
    ) || !matches!(
        anchor.horizontal.relative_from,
        HorizontalAnchor::Margin | HorizontalAnchor::Column
    ) {
        return None;
    }
    let side = match anchor.horizontal.position {
        HorizontalPosition::Align(HorizontalAlign::Left) => InlineFloatSide::Left,
        HorizontalPosition::Align(HorizontalAlign::Right) => InlineFloatSide::Right,
        _ => return None,
    };
    let exclusion_emu = anchor
        .wrap_distances
        .start_emu
        .saturating_add(extent.width_emu)
        .saturating_add(anchor.wrap_distances.end_emu)
        .max(0);
    let width = emu_to_twip(exclusion_emu);
    (width.raw() > 0).then_some(FlowItem::FloatExclusion {
        side,
        width,
        height,
    })
}

fn prepend_paragraph_float_exclusions<'a>(
    paragraph: NodeId,
    items: &mut Vec<FlowItem<'a>>,
    ctx: &FlowCtx<'_>,
) {
    let Some(exclusions) = ctx
        .paragraph_float_exclusions
        .and_then(|by_paragraph| by_paragraph.get(&paragraph))
    else {
        return;
    };
    prepend_explicit_float_exclusions(exclusions, items);
}

fn prepend_explicit_float_exclusions<'a>(
    exclusions: &[ParagraphFloatExclusion],
    items: &mut Vec<FlowItem<'a>>,
) {
    // Insert in reverse so the stable, page-derived order is preserved at byte 0.
    for exclusion in exclusions.iter().rev() {
        if exclusion.width.raw() > 0 && exclusion.height.raw() > 0 {
            items.insert(
                0,
                FlowItem::FloatExclusion {
                    side: exclusion.side,
                    width: exclusion.width,
                    height: exclusion.height,
                },
            );
        }
    }
}

/// Moves paragraph-local float exclusions to the paragraph start and coalesces
/// multiple overlapping top-and-bottom floats by maximum clearance. Summing
/// them would incorrectly stack objects that share the same anchor paragraph.
fn normalize_float_barriers(items: &mut Vec<FlowItem<'_>>) {
    let height = items
        .iter()
        .filter_map(|item| match item {
            FlowItem::FloatBarrier { height } => Some(*height),
            _ => None,
        })
        .max();
    let Some(height) = height else {
        return;
    };
    items.retain(|item| !matches!(item, FlowItem::FloatBarrier { .. }));
    items.insert(0, FlowItem::FloatBarrier { height });
}

/// Flows an inline text box (`wps:txbx` / `v:textbox`) into an [`FlowItem::TextBox`]:
/// its recursive block content is laid out through the **same** [`flow_blocks`]
/// pipeline the document body uses (so it supports paragraphs, tables incl. nested,
/// inline images, borders/shading — the uniform-flow-pipeline invariant), and it
/// works identically in headers/footers/cells because they share this flow.
///
/// A positive authored extent dimension wins. A missing or zero width falls back
/// to the available flow width; a missing or zero height falls back to flowed
/// content height plus the internal insets. Appearance is copied from the shape;
/// no border is invented when the document declares none.
fn textbox_item(
    text_box: &TextBox,
    shaper: &dyn LineShaper,
    width: Twip,
    ctx: &mut FlowCtx,
) -> FlowItem<'static> {
    let authored_size = text_box.extent.as_ref().map(extent_to_size);
    let outer_width = authored_size
        .map(|size| size.width)
        .filter(|width| width.raw() > 0)
        .unwrap_or(width)
        .max(Twip(1));
    let authored_height = authored_size
        .map(|size| size.height)
        .filter(|height| height.raw() > 0);
    let flowed = flow_text_box_with_ctx(
        &text_box.blocks,
        &text_box.body_properties,
        shaper,
        outer_width,
        authored_height,
        ctx,
    );
    FlowItem::TextBox {
        blocks: flowed.blocks,
        size: flowed.size,
        border: text_box.border.map(text_box_stroke),
        fill: text_box.fill.map(rgba),
        content_layout: flowed.content_layout,
    }
}

fn intrinsic_text_box_width(
    text_box: &TextBox,
    shaper: &dyn LineShaper,
    ctx: &mut FlowCtx,
    pass: IntrinsicPass,
) -> Twip {
    if let Some(authored) = text_box
        .extent
        .as_ref()
        .map(extent_to_size)
        .map(|size| size.width)
        .filter(|width| width.raw() > 0)
    {
        return authored;
    }

    let (local_scale, local_reduction) = text_box_text_adjustments(&text_box.body_properties);
    let previous_scale = ctx.text_scale;
    let previous_reduction = ctx.line_spacing_reduction;
    ctx.text_scale = ((u64::from(previous_scale) * u64::from(local_scale)) / 100_000)
        .min(u64::from(u32::MAX)) as u32;
    ctx.line_spacing_reduction = combine_percentage_reductions(previous_reduction, local_reduction);
    let (minimum, preferred) =
        block_intrinsic(&text_box.blocks, shaper, ctx, ctx.table_style.as_ref());
    ctx.text_scale = previous_scale;
    ctx.line_spacing_reduction = previous_reduction;
    let content = match pass {
        IntrinsicPass::Minimum => minimum,
        IntrinsicPass::Preferred => preferred,
    };
    let insets = text_box_insets(&text_box.body_properties);
    clamp_twip(
        i64::from(content)
            .saturating_add(i64::from(insets.left.raw()))
            .saturating_add(i64::from(insets.right.raw())),
    )
    .max(Twip(1))
}

/// A text box after recursive block flow and box-model resolution. Used by both
/// the inline paragraph path and the post-pagination float path.
pub(crate) struct FlowedTextBox {
    pub(crate) blocks: Vec<BlockFragment>,
    pub(crate) size: Size,
    pub(crate) content_layout: TextBoxContentLayout,
}

/// Flows a floating or grouped text box through a fresh running-content context.
/// `outer_size` is the anchor/group rectangle before shape autofit; the returned
/// height may grow for `a:spAutoFit`.
pub(crate) fn flow_anchored_text_box(
    document: &Document,
    blocks: &[BlockNode],
    shaper: &dyn LineShaper,
    outer_size: Size,
    properties: &TextBoxBodyProperties,
) -> FlowedTextBox {
    let outer_width = outer_size.width.max(Twip(1));
    let authored_height = (outer_size.height.raw() > 0).then_some(outer_size.height);
    let insets = text_box_insets(properties);
    let inner_width = inner_text_box_width(outer_width, insets);
    let (text_scale, line_spacing_reduction) = text_box_text_adjustments(properties);
    let blocks = flow_running_blocks(
        document,
        blocks,
        shaper,
        inner_width,
        text_scale,
        line_spacing_reduction,
    );
    finish_text_box(blocks, outer_width, authored_height, insets, properties)
}

fn flow_text_box_with_ctx(
    blocks: &[BlockNode],
    properties: &TextBoxBodyProperties,
    shaper: &dyn LineShaper,
    outer_width: Twip,
    authored_height: Option<Twip>,
    ctx: &mut FlowCtx,
) -> FlowedTextBox {
    let insets = text_box_insets(properties);
    let inner_width = inner_text_box_width(outer_width, insets);
    let (local_scale, local_reduction) = text_box_text_adjustments(properties);
    let previous_scale = ctx.text_scale;
    let previous_reduction = ctx.line_spacing_reduction;
    let previous_para_style = ctx.para_style;
    ctx.text_scale = ((u64::from(previous_scale) * u64::from(local_scale)) / 100_000)
        .min(u64::from(u32::MAX)) as u32;
    ctx.line_spacing_reduction = combine_percentage_reductions(previous_reduction, local_reduction);
    let flowed = flow_blocks(blocks, shaper, inner_width, ctx);
    ctx.text_scale = previous_scale;
    ctx.line_spacing_reduction = previous_reduction;
    ctx.para_style = previous_para_style;
    finish_text_box(flowed, outer_width, authored_height, insets, properties)
}

#[derive(Clone, Copy)]
struct ResolvedTextBoxInsets {
    left: Twip,
    top: Twip,
    right: Twip,
    bottom: Twip,
}

fn text_box_insets(properties: &TextBoxBodyProperties) -> ResolvedTextBoxInsets {
    let insets = properties.insets;
    ResolvedTextBoxInsets {
        left: emu_to_twip(i64::from(insets.left_emu)),
        top: emu_to_twip(i64::from(insets.top_emu)),
        right: emu_to_twip(i64::from(insets.right_emu)),
        bottom: emu_to_twip(i64::from(insets.bottom_emu)),
    }
}

fn inner_text_box_width(outer_width: Twip, insets: ResolvedTextBoxInsets) -> Twip {
    Twip(
        outer_width
            .raw()
            .saturating_sub(insets.left.raw())
            .saturating_sub(insets.right.raw())
            .max(1),
    )
}

fn text_box_text_adjustments(properties: &TextBoxBodyProperties) -> (u32, u32) {
    match properties.auto_fit {
        TextBoxAutoFit::Normal {
            font_scale,
            line_spacing_reduction,
        } => (font_scale, line_spacing_reduction),
        TextBoxAutoFit::None | TextBoxAutoFit::Shape => (100_000, 0),
    }
}

fn combine_percentage_reductions(outer: u32, inner: u32) -> u32 {
    let retained_outer = 100_000u64.saturating_sub(u64::from(outer.min(100_000)));
    let retained_inner = 100_000u64.saturating_sub(u64::from(inner.min(100_000)));
    let retained = retained_outer.saturating_mul(retained_inner) / 100_000;
    100_000u32.saturating_sub(retained as u32)
}

fn finish_text_box(
    blocks: Vec<BlockFragment>,
    outer_width: Twip,
    authored_height: Option<Twip>,
    insets: ResolvedTextBoxInsets,
    properties: &TextBoxBodyProperties,
) -> FlowedTextBox {
    let content_height = blocks
        .iter()
        .map(BlockFragment::height)
        .fold(Twip::ZERO, |a, h| a + h);
    let natural_height = clamp_twip(
        i64::from(insets.top.raw())
            .saturating_add(i64::from(content_height.raw()))
            .saturating_add(i64::from(insets.bottom.raw())),
    )
    .max(Twip(1));
    let outer_height = match (properties.auto_fit, authored_height) {
        (TextBoxAutoFit::Shape, Some(authored)) => authored.max(natural_height),
        (TextBoxAutoFit::Shape, None) | (_, None) => natural_height,
        (_, Some(authored)) => authored.max(Twip(1)),
    };
    let inner_height = i64::from(outer_height.raw())
        .saturating_sub(i64::from(insets.top.raw()))
        .saturating_sub(i64::from(insets.bottom.raw()));
    let free = inner_height
        .saturating_sub(i64::from(content_height.raw()))
        .max(0);
    let vertical_offset = match properties.vertical_anchor {
        TextBoxVerticalAnchor::Top => 0,
        TextBoxVerticalAnchor::Center => free / 2,
        TextBoxVerticalAnchor::Bottom => free,
    };
    let origin_y = i64::from(insets.top.raw()).saturating_add(vertical_offset);
    FlowedTextBox {
        blocks,
        size: Size::new(outer_width, outer_height),
        content_layout: TextBoxContentLayout {
            origin: Point::new(insets.left, clamp_twip(origin_y)),
            clip_horizontal: properties.horizontal_overflow == TextBoxHorizontalOverflow::Clip,
            clip_vertical: !matches!(
                properties.vertical_overflow,
                TextBoxVerticalOverflow::Overflow
            ),
        },
    }
}

fn clamp_twip(value: i64) -> Twip {
    Twip(value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

fn rgba(color: Rgba) -> [u8; 4] {
    [color.r, color.g, color.b, color.a]
}

fn text_box_stroke(stroke: ShapeStroke) -> TextBoxStroke {
    TextBoxStroke {
        color: rgba(stroke.color),
        width: emu_to_twip(stroke.width_emu),
    }
}

/// Maps an inline field to a [`FlowItem::Field`]: classifies its instruction
/// (`PAGE`/`NUMPAGES`/other), shapes a placeholder value from its cached result,
/// and captures the run styling so the post-pagination field pass can reshape a
/// recomputed value. A field is laid out inline like a run; page-dependent fields
/// carry only a marker through pagination (never a baked number).
fn field_item(field: &casual_doc_model::v1::Field, ctx: &mut FlowCtx) -> FlowItem<'static> {
    let kind = field_kind(&field.instruction);
    let cached = cached_result_text(&field.inlines);
    let value = if cached.is_empty() {
        match kind {
            // A placeholder so the flowed field has sensible width/height before the
            // field pass runs; `Passthrough` shows nothing when it has no result.
            FieldKind::Page | FieldKind::NumPages => "1".to_owned(),
            FieldKind::Passthrough => String::new(),
        }
    } else {
        cached
    };
    let style = field_style(&field.inlines, &value, ctx);
    FlowItem::Field { kind, value, style }
}

/// Classifies a field instruction by its leading keyword (case-insensitive):
/// `PAGE` and `NUMPAGES` are the page-dependent fields the field pass recomputes;
/// everything else passes its cached result through unchanged.
fn field_kind(instruction: &str) -> FieldKind {
    match instruction
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase()
        .as_str()
    {
        "PAGE" => FieldKind::Page,
        "NUMPAGES" => FieldKind::NumPages,
        _ => FieldKind::Passthrough,
    }
}

/// Concatenates the text of a field's cached-result runs (its `inlines`, which are
/// leaf-only) — the value shown before the field pass recomputes it.
fn cached_result_text(inlines: &[InlineNode]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => out.push_str(&run.text),
            InlineNode::Tab(_) => out.push('\t'),
            _ => {}
        }
    }
    out
}

/// The styling used to shape (and later reshape) a field's value: the first
/// cached-result run's resolved style, or the document default when the field has
/// no cached result.
fn field_style(inlines: &[InlineNode], value: &str, ctx: &mut FlowCtx) -> FieldStyle {
    let props = inlines.iter().find_map(|n| match n {
        InlineNode::Run(run) => Some(&run.properties),
        _ => None,
    });
    let styled = match props {
        Some(props) => styled_run(value, props, ctx),
        None => styled_run(value, &RunProperties::default(), ctx),
    };
    FieldStyle {
        font: styled.font,
        size: styled.size,
        character_scale_percent: styled.character_scale_percent,
        color: styled.color,
        bold: styled.bold,
        italic: styled.italic,
        letter_spacing: styled.letter_spacing,
        decoration: styled.decoration,
    }
}

/// Maps an inline drawing to an [`FlowItem::Image`]: resolves its media id to the
/// package part name (the display list's stable media key) and its EMU extent to a
/// twip box. Returns `None` — contributing nothing, never panicking — when the
/// drawing declares no extent (so it cannot be sized here) or its media id is
/// absent from the table. Anchored/floating placement is a later slice
/// (`P1F-28`); this is the inline case.
fn image_item(drawing: &Drawing, ctx: &FlowCtx) -> Option<FlowItem<'static>> {
    let part = ctx.media.get(&drawing.media)?.part_name.clone();
    let size = extent_to_size(drawing.extent.as_ref()?);
    (size.width.raw() > 0 && size.height.raw() > 0).then_some(FlowItem::Image {
        media: part,
        size,
        crop: drawing.crop,
    })
}

/// Maps an inline horizontal rule (`v:rect@o:hr`) to an [`FlowItem::HorizontalRule`],
/// resolving its width against the available content `width` (`width_permille` of
/// it) and its horizontal offset against its alignment. The rule box is `width`
/// wide (a fraction, for a partial-width rule) and `thickness_emu` tall; the box
/// origin's `y` is `0` (the box owns its line — [`stack_lines`] shifts it into
/// paragraph-absolute space).
fn hr_item(rule: &ModelHorizontalRule, width: Twip) -> FlowItem<'static> {
    let thickness = emu_to_twip(rule.thickness_emu).max(Twip(1));
    let permille = i64::from(rule.width_permille).clamp(1, 1000);
    let rule_width = Twip(
        ((i64::from(width.raw()) * permille) / 1000).clamp(1, i64::from(width.raw().max(1))) as i32,
    );
    let slack = Twip((width.raw() - rule_width.raw()).max(0));
    let left = match rule.align {
        HorizontalRuleAlign::Left => Twip::ZERO,
        HorizontalRuleAlign::Center => Twip(slack.raw() / 2),
        HorizontalRuleAlign::Right => slack,
    };
    FlowItem::HorizontalRule(InlineRule {
        origin: Point::new(left, Twip::ZERO),
        size: Size::new(rule_width, thickness),
        color: [rule.color.r, rule.color.g, rule.color.b, rule.color.a],
    })
}

/// Converts a drawing's EMU extent to a twip box size.
fn extent_to_size(extent: &Extent) -> Size {
    Size::new(
        emu_to_twip(extent.width_emu),
        emu_to_twip(extent.height_emu),
    )
}

/// EMU → twips: 914400 EMU/inch ÷ 1440 twips/inch = exactly 635 EMU per twip.
/// Clamped to a non-negative `i32` twip (the model bounds `Extent` to `MAX_EMU`).
fn emu_to_twip(emu: i64) -> Twip {
    Twip((emu / 635).clamp(0, i64::from(i32::MAX)) as i32)
}

/// Shapes a paragraph's [`FlowItem`] stream into lines. Images are handed to the
/// shaper as true in-flow boxes interleaved with text. Text boxes, horizontal
/// rules, and float barriers remain block-like standalone lines.
fn shape_paragraph_items(
    shaper: &dyn LineShaper,
    items: &[FlowItem<'_>],
    tab_stops: &[TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
    range: ModelRange,
) -> LineLayout {
    let note_positions = collect_note_positions(items, range.start.offset);
    let is_standalone = |item: &FlowItem<'_>| {
        matches!(
            item,
            FlowItem::TextBox { .. } | FlowItem::HorizontalRule(_) | FlowItem::FloatBarrier { .. }
        )
    };
    if !items.iter().any(is_standalone) {
        let mut layout =
            shape_inline_chunk(shaper, items, tab_stops, default_tab, constraints, range);
        attach_note_markers(&mut layout, &note_positions);
        return layout;
    }

    let mut out: Vec<Line> = Vec::new();
    let mut cursor_y = Twip::ZERO;
    let mut i = 0usize;
    while i < items.len() {
        match &items[i] {
            FlowItem::TextBox {
                blocks,
                size,
                border,
                fill,
                content_layout,
            } => {
                let line = textbox_line(
                    blocks.clone(),
                    *size,
                    *border,
                    *fill,
                    *content_layout,
                    range,
                );
                stack_lines(&mut out, vec![line], &mut cursor_y);
                i += 1;
            }
            FlowItem::HorizontalRule(rule) => {
                let line = hr_line(*rule, range);
                stack_lines(&mut out, vec![line], &mut cursor_y);
                i += 1;
            }
            FlowItem::FloatBarrier { height } => {
                let line = float_barrier_line(*height, range);
                stack_lines(&mut out, vec![line], &mut cursor_y);
                i += 1;
            }
            _ => {
                let start = i;
                while i < items.len() && !is_standalone(&items[i]) {
                    i += 1;
                }
                let chunk = shape_inline_chunk(
                    shaper,
                    &items[start..i],
                    tab_stops,
                    default_tab,
                    constraints,
                    range,
                );
                stack_lines(&mut out, chunk.lines, &mut cursor_y);
            }
        }
    }
    let mut layout = LineLayout { lines: out };
    attach_note_markers(&mut layout, &note_positions);
    layout
}

fn collect_note_positions(items: &[FlowItem<'_>], base: u32) -> Vec<(u32, NoteMarker)> {
    let mut byte = base;
    let mut notes = Vec::new();
    for item in items {
        match item {
            FlowItem::Run(run) => {
                byte = byte.saturating_add(run.text.len() as u32);
            }
            FlowItem::Field { value, .. } => {
                byte = byte.saturating_add(value.len() as u32);
            }
            FlowItem::NoteReference(marker) => notes.push((byte, *marker)),
            FlowItem::Tab
            | FlowItem::PositionalTab { .. }
            | FlowItem::Break(_)
            | FlowItem::Image { .. }
            | FlowItem::Math { .. }
            | FlowItem::TextBox { .. }
            | FlowItem::HorizontalRule(_)
            | FlowItem::FloatBarrier { .. }
            | FlowItem::FloatExclusion { .. } => {}
        }
    }
    notes
}

fn attach_note_markers(layout: &mut LineLayout, notes: &[(u32, NoteMarker)]) {
    if layout.lines.is_empty() {
        return;
    }
    for (byte, marker) in notes {
        let mut target = layout.lines.len() - 1;
        for (index, line) in layout.lines.iter().enumerate() {
            let start = line.range.start.offset;
            let end = line.range.end.offset;
            if start <= *byte && (start == end || *byte < end) {
                target = index;
                break;
            }
            if *byte <= end {
                target = index;
                break;
            }
        }
        layout.lines[target].notes.push(*marker);
    }
}

/// Shapes one standalone-box-free paragraph chunk, retaining the specialized
/// field path when that chunk contains PAGE/NUMPAGES or passthrough fields.
fn shape_inline_chunk(
    shaper: &dyn LineShaper,
    items: &[FlowItem<'_>],
    tab_stops: &[TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
    range: ModelRange,
) -> LineLayout {
    if items.iter().any(|item| {
        matches!(
            item,
            FlowItem::Image { .. } | FlowItem::Math { .. } | FlowItem::FloatExclusion { .. }
        )
    }) {
        let has_fields = items
            .iter()
            .any(|item| matches!(item, FlowItem::Field { .. }));
        if !has_fields && !tabs::needs_flow_layout(items, tab_stops) {
            return shape_text_with_objects(shaper, items, constraints, range);
        }
        // Fields and explicit tab/break assembly still own specialized marker and
        // positioning paths. Preserve their ordering safely until those paths can
        // carry boxes natively.
        return shape_complex_inline_with_objects(
            shaper,
            items,
            tab_stops,
            default_tab,
            constraints,
            range,
        );
    }
    if items
        .iter()
        .any(|item| matches!(item, FlowItem::Field { .. }))
    {
        shape_fielded_paragraph(shaper, items, tab_stops, default_tab, constraints, range)
    } else {
        shape_text_items(shaper, items, tab_stops, default_tab, constraints, range)
    }
}

/// Shapes ordinary runs, inline images, and paragraph-local floating exclusions
/// through the shaper's inline-object seam. Object byte indices are measured in
/// the same concatenated text string as the run ranges.
fn shape_text_with_objects(
    shaper: &dyn LineShaper,
    items: &[FlowItem<'_>],
    constraints: LineConstraints,
    range: ModelRange,
) -> LineLayout {
    let mut runs = Vec::new();
    let mut images = Vec::new();
    let mut maths = Vec::new();
    let mut floats = Vec::new();
    let mut byte = 0u32;
    for item in items {
        match item {
            FlowItem::Run(run) => {
                byte = byte.saturating_add(run.text.len() as u32);
                runs.push(run.clone());
            }
            FlowItem::Image { media, size, crop } => images.push(InlineImageSpec {
                media: media.clone(),
                index: byte,
                size: *size,
                crop: *crop,
            }),
            FlowItem::Math { size, runs, rules } => maths.push(InlineMathSpec {
                index: byte,
                size: *size,
                runs: runs.clone(),
                rules: rules.clone(),
            }),
            FlowItem::FloatExclusion {
                side,
                width,
                height,
            } => floats.push(InlineFloatSpec {
                index: byte,
                side: *side,
                width: *width,
                height: *height,
            }),
            _ => {}
        }
    }
    shaper.shape_paragraph_with_rich_inline_objects(
        &runs,
        &images,
        &maths,
        &floats,
        constraints,
        range,
    )
}

/// Compatibility path for uncommon field/tab + inline-object combinations:
/// specialized text chunks keep their semantics, images occupy an ordered line,
/// and floats conservatively become vertical barriers so content cannot collide.
fn shape_complex_inline_with_objects(
    shaper: &dyn LineShaper,
    items: &[FlowItem<'_>],
    tab_stops: &[TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
    range: ModelRange,
) -> LineLayout {
    let mut out = Vec::new();
    let mut cursor_y = Twip::ZERO;
    let mut start = 0usize;
    for (index, item) in items.iter().enumerate() {
        if !matches!(
            item,
            FlowItem::Image { .. } | FlowItem::Math { .. } | FlowItem::FloatExclusion { .. }
        ) {
            continue;
        }
        if start < index {
            let chunk = shape_inline_chunk(
                shaper,
                &items[start..index],
                tab_stops,
                default_tab,
                constraints,
                range,
            );
            stack_lines(&mut out, chunk.lines, &mut cursor_y);
        }
        let object_line = match item {
            FlowItem::Image { media, size, crop } => image_line(media.clone(), *size, *crop, range),
            FlowItem::Math { size, runs, rules } => {
                math_line(*size, runs.clone(), rules.clone(), range)
            }
            FlowItem::FloatExclusion { height, .. } => float_barrier_line(*height, range),
            _ => unreachable!(),
        };
        stack_lines(&mut out, vec![object_line], &mut cursor_y);
        start = index + 1;
    }
    if start < items.len() {
        let chunk = shape_inline_chunk(
            shaper,
            &items[start..],
            tab_stops,
            default_tab,
            constraints,
            range,
        );
        stack_lines(&mut out, chunk.lines, &mut cursor_y);
    }
    LineLayout { lines: out }
}

/// Shapes an image-free [`FlowItem`] slice. Ordinary text (no tab, no break, no
/// `bar` tab stop) takes the fast path — the base shaper alone, so its output is
/// byte-identical to before the flow layer existed; anything with a tab or break
/// is resolved by [`crate::tabs`].
fn shape_text_items(
    shaper: &dyn LineShaper,
    items: &[FlowItem<'_>],
    tab_stops: &[TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
    range: ModelRange,
) -> LineLayout {
    if !tabs::needs_flow_layout(items, tab_stops) {
        let runs: Vec<StyledRun<'_>> = items
            .iter()
            .filter_map(|item| match item {
                FlowItem::Run(run) => Some(run.clone()),
                _ => None,
            })
            .collect();
        return shaper.shape_paragraph(&runs, constraints, range);
    }
    tabs::shape_with_flow(shaper, items, tab_stops, default_tab, constraints, range)
}

/// A line holding a single inline image box at the paragraph's leading edge, its
/// height equal to the image height so following content stacks below it.
fn image_line(
    media: String,
    size: Size,
    crop: Option<casual_doc_model::v1::CropRect>,
    range: ModelRange,
) -> Line {
    Line {
        runs: Vec::new(),
        ascent: size.height,
        descent: Twip::ZERO,
        height: size.height,
        clip: false,
        range,
        line_break: LineBreak::Wrap,
        page_break_after: false,
        bars: Vec::new(),
        images: vec![InlineImage {
            media,
            origin: Point::new(Twip::ZERO, Twip::ZERO),
            size,
            crop,
        }],
        fields: Vec::new(),
        notes: Vec::new(),
        text_boxes: Vec::new(),
        rules: Vec::new(),
    }
}

/// Conservative compatibility line for equations combined with tabs/fields,
/// whose specialized assembly path cannot yet interleave arbitrary boxes.
fn math_line(size: Size, runs: Vec<GlyphRun>, rules: Vec<InlineRule>, range: ModelRange) -> Line {
    Line {
        runs,
        ascent: size.height,
        descent: Twip::ZERO,
        height: size.height,
        clip: false,
        range,
        line_break: LineBreak::Wrap,
        page_break_after: false,
        bars: Vec::new(),
        images: Vec::new(),
        fields: Vec::new(),
        notes: Vec::new(),
        text_boxes: Vec::new(),
        rules,
    }
}

/// A non-painting line whose height keeps visible paragraph content below a
/// local `wrapTopAndBottom` float. It participates in pagination and fragment
/// sizing but contributes no runs, images, fields, bars, or text boxes.
fn float_barrier_line(height: Twip, range: ModelRange) -> Line {
    Line {
        runs: Vec::new(),
        ascent: height,
        descent: Twip::ZERO,
        height,
        clip: false,
        range,
        line_break: LineBreak::Wrap,
        page_break_after: false,
        bars: Vec::new(),
        images: Vec::new(),
        fields: Vec::new(),
        notes: Vec::new(),
        text_boxes: Vec::new(),
        rules: Vec::new(),
    }
}

/// A line holding a single inline text box at the paragraph's leading edge, its
/// height equal to the box's outer height so following content stacks below it. The
/// box carries its already-flowed block fragments; composition paints the fill and
/// border and composes those fragments offset into the box.
fn textbox_line(
    blocks: Vec<BlockFragment>,
    size: Size,
    border: Option<TextBoxStroke>,
    fill: Option<[u8; 4]>,
    content_layout: TextBoxContentLayout,
    range: ModelRange,
) -> Line {
    Line {
        runs: Vec::new(),
        ascent: size.height,
        descent: Twip::ZERO,
        height: size.height,
        clip: false,
        range,
        line_break: LineBreak::Wrap,
        page_break_after: false,
        bars: Vec::new(),
        images: Vec::new(),
        fields: Vec::new(),
        notes: Vec::new(),
        text_boxes: vec![InlineTextBox {
            origin: Point::new(Twip::ZERO, Twip::ZERO),
            size,
            blocks,
            border,
            fill,
            content_layout,
        }],
        rules: Vec::new(),
    }
}

/// A line holding a single inline horizontal rule at the paragraph's leading edge,
/// its height equal to the rule's thickness so following content stacks below it.
/// The rule's own `origin.x` (already resolved for alignment) is preserved; its
/// `origin.y` is `0` (line-relative), shifted into paragraph-absolute space by
/// [`stack_lines`].
fn hr_line(rule: InlineRule, range: ModelRange) -> Line {
    Line {
        runs: Vec::new(),
        ascent: rule.size.height,
        descent: Twip::ZERO,
        height: rule.size.height,
        clip: false,
        range,
        line_break: LineBreak::Wrap,
        page_break_after: false,
        bars: Vec::new(),
        images: Vec::new(),
        fields: Vec::new(),
        notes: Vec::new(),
        text_boxes: Vec::new(),
        rules: vec![rule],
    }
}

/// Appends `lines` below the ones already in `out`, shifting each line's runs,
/// images, text boxes, and rules down by `cursor_y` (into paragraph-absolute y) and
/// advancing `cursor_y` past them.
fn stack_lines(out: &mut Vec<Line>, mut lines: Vec<Line>, cursor_y: &mut Twip) {
    let base = *cursor_y;
    for line in &mut lines {
        line.translate_contents_y(base);
    }
    *cursor_y = lines.iter().fold(base, |cursor, line| cursor + line.height);
    out.extend(lines);
}

// --- Inline math -----------------------------------------------------------

/// Resource ceiling for one equation's computed geometry. This is far larger
/// than a page while keeping all intermediate integer-twip additions safe.
const MAX_MATH_LAYOUT_TWIPS: i32 = 1 << 24;

#[derive(Clone)]
struct MathBox {
    size: Size,
    ascent: Twip,
    descent: Twip,
    runs: Vec<GlyphRun>,
    rules: Vec<InlineRule>,
}

fn layout_math_expression(
    shaper: &dyn LineShaper,
    expression: &MathExpression,
    base: &StyledRun<'_>,
    scale_permille: u16,
) -> Option<MathBox> {
    match expression {
        MathExpression::Text { value } => shape_math_text(shaper, value, base, scale_permille),
        MathExpression::Row { children } => {
            let children = children
                .iter()
                .map(|child| layout_math_expression(shaper, child, base, scale_permille))
                .collect::<Option<Vec<_>>>()?;
            math_row(children)
        }
        MathExpression::Fraction {
            numerator,
            denominator,
        } => {
            let child_scale = scaled_permille(scale_permille, 850);
            let numerator = layout_math_expression(shaper, numerator, base, child_scale)?;
            let denominator = layout_math_expression(shaper, denominator, base, child_scale)?;
            fraction_box(numerator, denominator, base.color, base.size)
        }
        MathExpression::Script {
            base: expression_base,
            subscript,
            superscript,
        } => {
            let expression_base =
                layout_math_expression(shaper, expression_base, base, scale_permille)?;
            let script_scale = scaled_permille(scale_permille, 700);
            let subscript = match subscript.as_deref() {
                Some(value) => Some(layout_math_expression(shaper, value, base, script_scale)?),
                None => None,
            };
            let superscript = match superscript.as_deref() {
                Some(value) => Some(layout_math_expression(shaper, value, base, script_scale)?),
                None => None,
            };
            script_box(expression_base, subscript, superscript, base.size)
        }
        MathExpression::Radical { degree, radicand } => {
            let radicand = layout_math_expression(shaper, radicand, base, scale_permille)?;
            let degree_scale = scaled_permille(scale_permille, 600);
            let degree = match degree.as_deref() {
                Some(value) => Some(layout_math_expression(shaper, value, base, degree_scale)?),
                None => None,
            };
            radical_box(shaper, degree, radicand, base, scale_permille)
        }
        MathExpression::Delimiter {
            open,
            close,
            content,
        } => {
            let mut children = Vec::new();
            if !open.is_empty() {
                children.push(shape_math_text(shaper, open, base, scale_permille)?);
            }
            children.push(layout_math_expression(
                shaper,
                content,
                base,
                scale_permille,
            )?);
            if !close.is_empty() {
                children.push(shape_math_text(shaper, close, base, scale_permille)?);
            }
            math_row(children)
        }
    }
}

fn shape_math_text(
    shaper: &dyn LineShaper,
    value: &str,
    base: &StyledRun<'_>,
    scale_permille: u16,
) -> Option<MathBox> {
    if value.is_empty() {
        return None;
    }
    let mut styled = base.clone();
    styled.text = Cow::Owned(value.to_owned());
    styled.size = Twip(
        (i64::from(base.size.raw()) * i64::from(scale_permille) / 1_000)
            .clamp(1, i64::from(MAX_MATH_LAYOUT_TWIPS)) as i32,
    );
    styled.baseline_shift = Twip::ZERO;
    let node = NodeId::from_parts(1, 1).expect("fixed valid math shaping id");
    let range = ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0));
    let layout = shaper.shape_paragraph(&[styled], tabs::unwrapped_constraints(), range);
    let line = layout.lines.first()?;
    let width = line
        .runs
        .iter()
        .map(|run| safe_add(run.origin.x, run_advance(run)))
        .max()
        .unwrap_or(Twip::ZERO);
    if width.raw() <= 0 || line.height.raw() <= 0 {
        return None;
    }
    let mut runs = line.runs.clone();
    for run in &mut runs {
        for glyph in &mut run.glyphs {
            glyph.cluster = 0;
        }
    }
    Some(MathBox {
        size: Size::new(width, line.height),
        ascent: line.ascent,
        descent: line.descent,
        runs,
        rules: Vec::new(),
    })
}

fn math_row(children: Vec<MathBox>) -> Option<MathBox> {
    if children.is_empty() {
        return None;
    }
    let ascent = children
        .iter()
        .map(|child| child.ascent)
        .max()
        .unwrap_or(Twip::ZERO);
    let descent = children
        .iter()
        .map(|child| child.descent)
        .max()
        .unwrap_or(Twip::ZERO);
    let height = safe_add(ascent, descent);
    let mut out = MathBox {
        size: Size::new(Twip::ZERO, height),
        ascent,
        descent,
        runs: Vec::new(),
        rules: Vec::new(),
    };
    let mut x = Twip::ZERO;
    for mut child in children {
        let y = ascent - child.ascent;
        translate_math_box(&mut child, x, y);
        out.runs.extend(child.runs);
        out.rules.extend(child.rules);
        x = safe_add(x, child.size.width);
    }
    out.size.width = x;
    Some(out)
}

fn fraction_box(
    mut numerator: MathBox,
    mut denominator: MathBox,
    color: [u8; 4],
    em: Twip,
) -> Option<MathBox> {
    let padding = Twip((em.raw() / 8).max(12));
    let gap = Twip((em.raw() / 10).max(10));
    let thickness = Twip((em.raw() / 24).max(8));
    let content_width = numerator.size.width.max(denominator.size.width);
    let width = safe_add(content_width, safe_add(padding, padding));
    let rule_y = safe_add(numerator.size.height, gap);
    let denominator_y = safe_add(rule_y, safe_add(thickness, gap));
    let height = safe_add(denominator_y, denominator.size.height);
    let numerator_x = Twip((width.raw() - numerator.size.width.raw()) / 2);
    let denominator_x = Twip((width.raw() - denominator.size.width.raw()) / 2);
    translate_math_box(&mut numerator, numerator_x, Twip::ZERO);
    translate_math_box(&mut denominator, denominator_x, denominator_y);
    let ascent = safe_add(rule_y, Twip(thickness.raw() / 2));
    Some(MathBox {
        size: Size::new(width, height),
        ascent,
        descent: height - ascent,
        runs: numerator.runs.into_iter().chain(denominator.runs).collect(),
        rules: numerator
            .rules
            .into_iter()
            .chain(denominator.rules)
            .chain(core::iter::once(InlineRule {
                origin: Point::new(padding, rule_y),
                size: Size::new(content_width, thickness),
                color,
            }))
            .collect(),
    })
}

fn script_box(
    mut base: MathBox,
    mut subscript: Option<MathBox>,
    mut superscript: Option<MathBox>,
    em: Twip,
) -> Option<MathBox> {
    let gap = Twip((em.raw() / 12).max(8));
    let sup_height = superscript
        .as_ref()
        .map_or(Twip::ZERO, |value| value.size.height);
    let ascent = base.ascent.max(safe_add(sup_height, gap));
    let base_y = ascent - base.ascent;
    let script_x = safe_add(base.size.width, gap);
    translate_math_box(&mut base, Twip::ZERO, base_y);
    let mut width = base.size.width;
    let mut height = safe_add(base_y, base.size.height);
    let mut runs = base.runs;
    let mut rules = base.rules;
    if let Some(superscript) = &mut superscript {
        translate_math_box(superscript, script_x, Twip::ZERO);
        width = width.max(safe_add(script_x, superscript.size.width));
        height = height.max(superscript.size.height);
        runs.append(&mut superscript.runs);
        rules.append(&mut superscript.rules);
    }
    if let Some(subscript) = &mut subscript {
        let y = safe_add(ascent, gap);
        translate_math_box(subscript, script_x, y);
        width = width.max(safe_add(script_x, subscript.size.width));
        height = height.max(safe_add(y, subscript.size.height));
        runs.append(&mut subscript.runs);
        rules.append(&mut subscript.rules);
    }
    Some(MathBox {
        size: Size::new(width, height),
        ascent,
        descent: height - ascent,
        runs,
        rules,
    })
}

fn radical_box(
    shaper: &dyn LineShaper,
    mut degree: Option<MathBox>,
    mut radicand: MathBox,
    base: &StyledRun<'_>,
    scale_permille: u16,
) -> Option<MathBox> {
    let mut radical = shape_math_text(shaper, "√", base, scale_permille)?;
    let gap = Twip((base.size.raw() / 16).max(8));
    let thickness = Twip((base.size.raw() / 24).max(8));
    let degree_width = degree
        .as_ref()
        .map_or(Twip::ZERO, |value| Twip(value.size.width.raw() / 2));
    let radical_x = degree_width;
    let radicand_x = safe_add(radical_x, radical.size.width);
    let ascent = safe_add(
        radical.ascent.max(radicand.ascent),
        safe_add(gap, thickness),
    );
    let radical_y = ascent - radical.ascent;
    let radicand_y = ascent - radicand.ascent;
    translate_math_box(&mut radical, radical_x, radical_y);
    translate_math_box(&mut radicand, radicand_x, radicand_y);
    let width = safe_add(radicand_x, radicand.size.width);
    let mut height = safe_add(ascent, radical.descent.max(radicand.descent));
    let mut runs: Vec<_> = radical.runs.into_iter().chain(radicand.runs).collect();
    let mut rules: Vec<_> = radical.rules.into_iter().chain(radicand.rules).collect();
    rules.push(InlineRule {
        origin: Point::new(radicand_x, Twip((radicand_y.raw() - gap.raw()).max(0))),
        size: Size::new(radicand.size.width, thickness),
        color: base.color,
    });
    if let Some(degree) = &mut degree {
        translate_math_box(degree, Twip::ZERO, Twip::ZERO);
        height = height.max(degree.size.height);
        runs.append(&mut degree.runs);
        rules.append(&mut degree.rules);
    }
    Some(MathBox {
        size: Size::new(width, height),
        ascent,
        descent: height - ascent,
        runs,
        rules,
    })
}

fn translate_math_box(math: &mut MathBox, x: Twip, y: Twip) {
    for run in &mut math.runs {
        run.origin = Point::new(safe_add(run.origin.x, x), safe_add(run.origin.y, y));
    }
    for rule in &mut math.rules {
        rule.origin = Point::new(safe_add(rule.origin.x, x), safe_add(rule.origin.y, y));
    }
}

fn scaled_permille(scale: u16, factor: u16) -> u16 {
    (u32::from(scale) * u32::from(factor) / 1_000).clamp(250, 1_000) as u16
}

fn safe_add(left: Twip, right: Twip) -> Twip {
    Twip((i64::from(left.raw()) + i64::from(right.raw())).clamp(
        -i64::from(MAX_MATH_LAYOUT_TWIPS),
        i64::from(MAX_MATH_LAYOUT_TWIPS),
    ) as i32)
}

// --- Inline fields ---------------------------------------------------------

/// A field's value shaped into a single glyph run at `origin` — the shared shaping
/// used both to lay a field out at flow time and to restamp it in the field pass
/// ([`crate::paginate::resolve_fields`]), so a resolved field is byte-identical
/// however it was produced. The glyphs of `value`'s shaped runs are concatenated
/// into one run (clusters zeroed — a field's value is not model text), and its
/// advance / metrics are returned so callers can position following content and
/// size the line.
#[must_use]
pub(crate) fn shape_field_run(
    shaper: &dyn LineShaper,
    value: &str,
    style: FieldStyle,
    origin: Point,
) -> FieldRunShape {
    let styled = StyledRun {
        text: value.into(),
        // A field style carries only a resolved face id, not the declared family,
        // so field glyphs shape with the bundled face (no system-face preference).
        requested_family: None,
        font: style.font,
        size: style.size,
        character_scale_percent: style.character_scale_percent,
        bold: style.bold,
        italic: style.italic,
        letter_spacing: style.letter_spacing,
        color: style.color,
        decoration: style.decoration,
        highlight: None,
        baseline_shift: Twip::ZERO,
    };
    let node = NodeId::from_parts(1, 1).expect("1/1 is a valid node id");
    let dummy = ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0));
    let layout = shaper.shape_paragraph(&[styled], tabs::unwrapped_constraints(), dummy);
    let mut glyphs: Vec<Glyph> = Vec::new();
    let (mut ascent, mut descent) = (Twip::ZERO, Twip::ZERO);
    if let Some(line) = layout.lines.first() {
        ascent = line.ascent;
        descent = line.descent;
        for run in &line.runs {
            for g in &run.glyphs {
                glyphs.push(Glyph {
                    id: g.id,
                    advance: g.advance,
                    cluster: 0,
                });
            }
        }
    }
    let advance = glyphs.iter().fold(Twip::ZERO, |a, g| a + g.advance);
    FieldRunShape {
        run: GlyphRun {
            is_marker: false,
            font: style.font,
            size: style.size,
            character_scale_percent: style.character_scale_percent,
            color: style.color,
            origin,
            bidi_level: 0,
            decoration: style.decoration,
            highlight: None,
            glyphs,
        },
        ascent,
        descent,
        advance,
    }
}

/// The result of [`shape_field_run`]: the shaped run plus the metrics a caller
/// needs to place following content and size the line.
pub(crate) struct FieldRunShape {
    /// The single glyph run for the field's value.
    pub run: GlyphRun,
    /// Ascent above the baseline.
    pub ascent: Twip,
    /// Descent below the baseline.
    pub descent: Twip,
    /// Total advance width.
    pub advance: Twip,
}

/// The total advance of a glyph run.
fn run_advance(run: &GlyphRun) -> Twip {
    run.glyphs.iter().fold(Twip::ZERO, |a, g| a + g.advance)
}

/// One tab-delimited segment of a fielded line: its shaped runs (local x from the
/// segment's left edge), the field markers pointing into those runs, and its box
/// metrics.
struct FieldedSegment {
    runs: Vec<GlyphRun>,
    fields: Vec<FieldPiece>,
    width: Twip,
    ascent: Twip,
    descent: Twip,
}

/// A field within a [`FieldedSegment`]: which of the segment's runs holds it, plus
/// the marker payload carried to the line.
struct FieldPiece {
    run: usize,
    kind: FieldKind,
    style: FieldStyle,
    value: String,
}

/// Shapes a paragraph whose inline stream contains fields. Runs, tabs, and fields
/// are laid out on a single horizontal line per hard-break block (fielded
/// paragraphs are not soft-wrapped — the target cases, header/footer and
/// page-number lines, are single-line in Word); each field becomes its own glyph
/// run tagged with a [`FieldMarker`] the post-pagination field pass resolves.
fn shape_fielded_paragraph(
    shaper: &dyn LineShaper,
    items: &[FlowItem<'_>],
    tab_stops: &[TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
    range: ModelRange,
) -> LineLayout {
    let ctx = FieldedCtx {
        shaper,
        node: range.start.node,
        base: range.start.offset,
        tab_stops,
        default_tab,
        constraints,
    };
    let bars: Vec<Twip> = tab_stops
        .iter()
        .filter(|t| t.alignment == TabAlignment::Bar)
        .map(|t| Twip(t.position_twips))
        .collect();

    // Split the stream into hard-break-delimited blocks (each is one line).
    let mut blocks: Vec<(Vec<&FlowItem<'_>>, Option<BreakKind>)> = Vec::new();
    let mut current: Vec<&FlowItem<'_>> = Vec::new();
    for item in items {
        match item {
            FlowItem::Break(kind) => {
                blocks.push((std::mem::take(&mut current), Some(*kind)));
            }
            other => current.push(other),
        }
    }
    blocks.push((current, None));

    let mut out: Vec<Line> = Vec::new();
    let mut cursor_y = Twip::ZERO;
    let last = blocks.len().saturating_sub(1);
    for (bi, (block_items, trailing)) in blocks.iter().enumerate() {
        let first_line_indent = if bi == 0 {
            constraints.first_line_indent
        } else {
            Twip::ZERO
        };
        let mut line = layout_fielded_line(&ctx, block_items, first_line_indent);
        for run in &mut line.runs {
            run.origin.y = run.origin.y + cursor_y;
        }
        line.bars = bars.clone();
        match trailing {
            Some(kind) => {
                line.line_break = match kind {
                    BreakKind::Line => LineBreak::Hard,
                    BreakKind::Page => LineBreak::Page,
                    BreakKind::Column => LineBreak::Column,
                };
                line.page_break_after = matches!(kind, BreakKind::Page | BreakKind::Column);
            }
            None if bi == last => line.line_break = LineBreak::ParagraphEnd,
            None => line.line_break = LineBreak::Hard,
        }
        cursor_y = cursor_y + line.height;
        out.push(line);
    }
    LineLayout { lines: out }
}

/// The invariant context threaded through a fielded paragraph's line layout: the
/// shaper, the node/byte anchor, and the paragraph's tab + wrap settings.
struct FieldedCtx<'a> {
    shaper: &'a dyn LineShaper,
    node: NodeId,
    base: u32,
    tab_stops: &'a [TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
}

/// Lays out one hard-break block of a fielded paragraph onto a single line: splits
/// it at tabs into segments, measures each, then positions segment 0 at the
/// (indented) start and each later segment at its resolved tab stop. Records a
/// [`FieldMarker`] per field with the run's absolute origin as its idempotent
/// reposition anchor.
fn layout_fielded_line(
    ctx: &FieldedCtx<'_>,
    items: &[&FlowItem<'_>],
    first_line_indent: Twip,
) -> Line {
    let shaper = ctx.shaper;
    let node = ctx.node;
    let base = ctx.base;
    let tab_stops = ctx.tab_stops;
    let default_tab = ctx.default_tab;
    let constraints = ctx.constraints;
    // Split at tabs into segments.
    let mut segments: Vec<Vec<&FlowItem<'_>>> = vec![Vec::new()];
    let mut tabs = Vec::new();
    for item in items {
        match item {
            FlowItem::Tab => {
                tabs.push(tabs::TabKind::Ordinary);
                segments.push(Vec::new());
            }
            FlowItem::PositionalTab {
                alignment,
                relative_to,
                leader,
            } => {
                tabs.push(tabs::TabKind::Positional {
                    alignment: *alignment,
                    relative_to: *relative_to,
                    leader: *leader,
                });
                segments.push(Vec::new());
            }
            _ => segments.last_mut().expect("non-empty").push(item),
        }
    }
    let has_tab = segments.len() > 1;
    let measured: Vec<FieldedSegment> = segments
        .iter()
        .map(|seg| measure_fielded_segment(shaper, node, seg))
        .collect();

    let ascent = measured
        .iter()
        .map(|s| s.ascent)
        .max()
        .unwrap_or(Twip::ZERO);
    let descent = measured
        .iter()
        .map(|s| s.descent)
        .max()
        .unwrap_or(Twip::ZERO);
    let baseline = ascent;

    let mut runs: Vec<GlyphRun> = Vec::new();
    let mut fields: Vec<FieldMarker> = Vec::new();
    let mut pen = first_line_indent.raw().max(0);

    for (i, seg) in measured.iter().enumerate() {
        let left = if i == 0 {
            pen
        } else {
            let stop = tabs::resolve_stop(tabs[i - 1], pen, tab_stops, default_tab, constraints);
            let mut l = match stop.alignment {
                TabAlignment::Start | TabAlignment::Bar => stop.position,
                TabAlignment::End | TabAlignment::Decimal => stop.position - seg.width.raw(),
                TabAlignment::Center => stop.position - seg.width.raw() / 2,
            };
            if l < pen {
                l = pen;
            }
            l
        };
        place_segment(seg, Twip(left), baseline, &mut runs, &mut fields);
        pen = left + seg.width.raw();
    }

    // A tab-free fielded line honors paragraph alignment (a centered / right page
    // number). Tabbed lines are positioned by their stops, as in Word.
    if !has_tab {
        let width = pen - first_line_indent.raw().max(0);
        let slack = constraints.max_width.raw() - width;
        let offset = match constraints.alignment {
            TextAlignment::Center => slack / 2,
            TextAlignment::End => slack,
            TextAlignment::Start | TextAlignment::Justify => 0,
        };
        if offset > 0 {
            for run in &mut runs {
                run.origin.x = run.origin.x + Twip(offset);
            }
            for field in &mut fields {
                field.base_x = field.base_x + Twip(offset);
            }
        }
    }

    let natural = ascent + descent;
    let (ascent, descent, height) =
        crate::shape::apply_line_rule(ascent, descent, natural, &constraints);
    let baseline_delta = ascent - baseline;
    if baseline_delta != Twip::ZERO {
        for run in &mut runs {
            run.origin.y = run.origin.y + baseline_delta;
        }
    }
    Line {
        runs,
        ascent,
        descent,
        height,
        clip: constraints.line_exact.is_some(),
        range: ModelRange::new(ModelPos::new(node, base), ModelPos::new(node, base)),
        line_break: LineBreak::ParagraphEnd,
        page_break_after: false,
        bars: Vec::new(),
        images: Vec::new(),
        fields,
        notes: Vec::new(),
        text_boxes: Vec::new(),
        rules: Vec::new(),
    }
}

/// Places a measured segment at absolute `left`/`baseline`, appending its runs to
/// `runs` and its field markers (with absolute `base_x`) to `fields`.
fn place_segment(
    seg: &FieldedSegment,
    left: Twip,
    baseline: Twip,
    runs: &mut Vec<GlyphRun>,
    fields: &mut Vec<FieldMarker>,
) {
    let base_index = runs.len();
    for run in &seg.runs {
        let mut placed = run.clone();
        placed.origin = Point::new(run.origin.x + left, baseline);
        runs.push(placed);
    }
    for piece in &seg.fields {
        let run_idx = base_index + piece.run;
        fields.push(FieldMarker {
            kind: piece.kind,
            run: run_idx as u32,
            base_x: runs[run_idx].origin.x,
            style: piece.style,
            value: piece.value.clone(),
        });
    }
}

/// Measures one tab-delimited segment of a fielded line: consecutive text runs are
/// shaped together (preserving intra-group kerning); each field is shaped on its
/// own. Runs carry local x from the segment's left edge; field pieces point at the
/// run holding each field.
fn measure_fielded_segment(
    shaper: &dyn LineShaper,
    node: NodeId,
    seg: &[&FlowItem<'_>],
) -> FieldedSegment {
    let mut runs: Vec<GlyphRun> = Vec::new();
    let mut fields: Vec<FieldPiece> = Vec::new();
    let mut pen = 0i32;
    let mut ascent = Twip::ZERO;
    let mut descent = Twip::ZERO;
    let range = ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0));

    let mut i = 0;
    while i < seg.len() {
        match seg[i] {
            FlowItem::Field { kind, value, style } => {
                let shape =
                    shape_field_run(shaper, value, *style, Point::new(Twip(pen), Twip::ZERO));
                ascent = ascent.max(shape.ascent);
                descent = descent.max(shape.descent);
                let idx = runs.len();
                runs.push(shape.run);
                fields.push(FieldPiece {
                    run: idx,
                    kind: *kind,
                    style: *style,
                    value: value.clone(),
                });
                pen += shape.advance.raw();
                i += 1;
            }
            FlowItem::Run(_) => {
                let start = i;
                while i < seg.len() && matches!(seg[i], FlowItem::Run(_)) {
                    i += 1;
                }
                let styled: Vec<StyledRun<'_>> = seg[start..i]
                    .iter()
                    .filter_map(|it| match it {
                        FlowItem::Run(r) => Some(r.clone()),
                        _ => None,
                    })
                    .collect();
                let layout = shaper.shape_paragraph(&styled, tabs::unwrapped_constraints(), range);
                if let Some(line) = layout.lines.first() {
                    ascent = ascent.max(line.ascent);
                    descent = descent.max(line.descent);
                    let mut group_w = 0i32;
                    for r in &line.runs {
                        let end = r.origin.x.raw() + run_advance(r).raw();
                        group_w = group_w.max(end);
                        let mut placed = r.clone();
                        placed.origin = Point::new(r.origin.x + Twip(pen), Twip::ZERO);
                        runs.push(placed);
                    }
                    pen += group_w;
                }
            }
            // Images/tabs/breaks do not reach here (tabs split the segment; images
            // and breaks are handled by their own paths).
            _ => i += 1,
        }
    }

    FieldedSegment {
        runs,
        fields,
        width: Twip(pen),
        ascent,
        descent,
    }
}

/// Maps a run's text + properties to a styled run, resolving its declared font
/// family (`w:rFonts`, direct or theme) to a concrete bundled face via the
/// [`FontResolver`] and recording any substitution/coverage fallback (`P1C-002b`).
///
/// Run appearance beyond the face is applied here too: `w:vertAlign` super/subscript
/// (a baseline shift plus a reduced shaped size) and `w:position` (a half-point
/// baseline raise/lower with no resize) fold into `size`/`baseline_shift`; `w:caps`
/// and `w:smallCaps` uppercase the text; and `w:color` resolves against the theme
/// palette. Small-caps per-letter size-splitting is done by [`push_styled_runs`],
/// so this single-run path uppercases without the per-letter size step.
fn styled_run<'a>(text: &'a str, properties: &RunProperties, ctx: &mut FlowCtx) -> StyledRun<'a> {
    let effective =
        ctx.cascade
            .resolve_run_in_table(ctx.para_style, properties, ctx.table_style.as_ref());
    build_styled_run(text, &effective, ctx)
}

fn styled_owned_run(
    text: String,
    properties: &RunProperties,
    ctx: &mut FlowCtx,
) -> StyledRun<'static> {
    let effective =
        ctx.cascade
            .resolve_run_in_table(ctx.para_style, properties, ctx.table_style.as_ref());
    let (size, baseline_shift) = scaled_run_metrics(&effective, ctx.text_scale);
    let text = case_transform(&text, &effective).into_owned();
    let bold = effective.bold.unwrap_or(false);
    let italic = effective.italic.unwrap_or(false);
    StyledRun {
        font: resolve_font(&text, &effective, bold, italic, ctx),
        requested_family: requested_family(&effective, ctx.scheme).map(Cow::Owned),
        text: Cow::Owned(text),
        size,
        character_scale_percent: effective.character_scale_percent.unwrap_or(100),
        bold,
        italic,
        letter_spacing: scale_twip(
            effective.character_spacing_twips.map_or(Twip::ZERO, Twip),
            ctx.text_scale,
        ),
        color: run_color(effective.color, ctx.palette),
        decoration: Decoration {
            underline: effective.underline.unwrap_or(false),
            strikethrough: effective.strike.unwrap_or(false),
        },
        highlight: effective.highlight.and_then(highlight_rgba),
        baseline_shift,
    }
}

/// Builds a [`StyledRun`] from already-resolved (effective) run properties. The
/// property cascade (style + docDefaults) is applied by [`styled_run`] /
/// [`push_styled_runs`] before this runs, so `properties` here is the value
/// actually in effect.
fn build_styled_run<'a>(
    text: &'a str,
    properties: &RunProperties,
    ctx: &mut FlowCtx,
) -> StyledRun<'a> {
    let (size, baseline_shift) = scaled_run_metrics(properties, ctx.text_scale);
    let text = case_transform(text, properties);
    let bold = properties.bold.unwrap_or(false);
    let italic = properties.italic.unwrap_or(false);
    StyledRun {
        // Resolve the declared family to a concrete face so the renderer outlines
        // the same face `parley` shapes with (measured against the shaped text).
        font: resolve_font(text.as_ref(), properties, bold, italic, ctx),
        // The declared family name, kept so the shaper can prefer a real installed
        // face of that name over the bundled fallback (`system-fonts`/host faces).
        requested_family: requested_family(properties, ctx.scheme).map(Cow::Owned),
        text,
        size,
        character_scale_percent: properties.character_scale_percent.unwrap_or(100),
        bold,
        italic,
        letter_spacing: scale_twip(
            properties.character_spacing_twips.map_or(Twip::ZERO, Twip),
            ctx.text_scale,
        ),
        color: run_color(properties.color, ctx.palette),
        decoration: Decoration {
            underline: properties.underline.unwrap_or(false),
            strikethrough: properties.strike.unwrap_or(false),
        },
        highlight: properties.highlight.and_then(highlight_rgba),
        baseline_shift,
    }
}

/// Builds the styled run for an inline `w:sym` symbol glyph. The legacy
/// symbol-font code point is mapped to a Unicode scalar an ordinary text face can
/// draw (see [`crate::symbol_map`]); an unmapped glyph falls back to a visible
/// placeholder so it is never silently dropped.
///
/// The symbol retains the owning run's properties, so form glyphs preserve their
/// authored size/color through the normal cascade. The declared symbol face
/// (Wingdings/Symbol/…) is intentionally **not**
/// requested — the mapped Unicode char must resolve against a covering text face,
/// not the (unbundled, glyph-incompatible) symbol font.
/// If `sdt` is a checkbox content control whose content is a single glyph run,
/// synthesize the state-driven glyph (`w14:checkedState` / `w14:uncheckedState`)
/// instead of trusting the cached `sdtContent` run — so a `w14:checked=1` box
/// paints its checked glyph even when the producer left the cached run unchecked
/// (docs/60 §8). Returns `None` for a non-checkbox SDT, a checkbox that declares
/// no glyph for the current state, or content that is not exactly one run (the
/// caller keeps the transparent recurse, so nothing is dropped). The cached
/// run's properties are reused so the glyph keeps its authored size/color, with
/// the checkbox symbol's own font.
fn sdt_checkbox_glyph_run(sdt: &InlineSdt, ctx: &mut FlowCtx) -> Option<StyledRun<'static>> {
    let Some(SdtControlData::Checkbox(checkbox)) = sdt.properties.data.as_ref() else {
        return None;
    };
    // Only replace a single-run content (the box glyph); anything richer keeps
    // its own recurse so no authored content is lost.
    let [InlineNode::Run(cached)] = sdt.inlines.as_slice() else {
        return None;
    };
    let state = if checkbox.checked {
        checkbox.checked_state.as_ref()
    } else {
        checkbox.unchecked_state.as_ref()
    }?;
    let code = u32::from_str_radix(state.val.trim(), 16).ok()?;
    let symbol = Symbol {
        id: cached.id,
        font: state.font.clone().unwrap_or_default(),
        char: code,
        properties: cached.properties.clone(),
    };
    Some(symbol_glyph_run(&symbol, ctx))
}

fn symbol_glyph_run(symbol: &Symbol, ctx: &mut FlowCtx) -> StyledRun<'static> {
    let glyph = crate::symbol_map::resolve_symbol(&symbol.font, symbol.char);
    let effective = ctx.cascade.resolve_run_in_table(
        ctx.para_style,
        &symbol.properties,
        ctx.table_style.as_ref(),
    );
    let (size, baseline_shift) = scaled_run_metrics(&effective, ctx.text_scale);
    let bold = effective.bold.unwrap_or(false);
    let italic = effective.italic.unwrap_or(false);
    let text = glyph.to_string();
    let font = resolve_font(&text, &effective, bold, italic, ctx);
    StyledRun {
        font,
        requested_family: requested_family(&effective, ctx.scheme).map(Cow::Owned),
        text: Cow::Owned(text),
        size,
        character_scale_percent: effective.character_scale_percent.unwrap_or(100),
        bold,
        italic,
        letter_spacing: scale_twip(
            effective.character_spacing_twips.map_or(Twip::ZERO, Twip),
            ctx.text_scale,
        ),
        color: run_color(effective.color, ctx.palette),
        decoration: Decoration {
            underline: effective.underline.unwrap_or(false),
            strikethrough: effective.strike.unwrap_or(false),
        },
        highlight: effective.highlight.and_then(highlight_rgba),
        baseline_shift,
    }
}

/// Remaps a list marker's glyphs through the symbol map: each character declared
/// in a legacy symbol face (`family`) that is a known symbol code point becomes
/// its Unicode equivalent, so a Wingdings/Symbol bullet renders as `●`/`○`/`▪`/…
/// in a covering text face rather than tofu. Returns the (possibly rewritten)
/// text and whether any glyph was remapped; ordinary bullet text (a literal `•`,
/// a number) passes through unchanged.
fn map_marker_glyphs(text: &str, family: Option<&str>) -> (String, bool) {
    let Some(family) = family else {
        return (text.to_owned(), false);
    };
    let mut out = String::with_capacity(text.len());
    let mut remapped = false;
    for ch in text.chars() {
        match crate::symbol_map::map_symbol(family, ch as u32) {
            Some(glyph) => {
                out.push(glyph);
                remapped = true;
            }
            None => out.push(ch),
        }
    }
    (out, remapped)
}

/// The down-scale factor for a list-marker glyph that is a full-em geometric
/// bullet (`●`/`○`/`■`/…). Those code points shape near cap-height, but Word and
/// LibreOffice draw list bullets much smaller — about the size of a `•` dot — so
/// the marker is scaled to match. Returns `None` for a normal marker (a number, a
/// `•`, or a multi-glyph marker) which shapes at its authored size.
fn bullet_scale(marker_text: &str) -> Option<f32> {
    let mut chars = marker_text.trim().chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        // More than one glyph: a numbered or multi-character marker, not a bullet.
        return None;
    }
    matches!(
        first,
        '\u{25CF}' // ● black circle
            | '\u{25CB}' // ○ white circle
            | '\u{25A0}' // ■ black square
            | '\u{25A1}' // □ white square
            | '\u{25C6}' // ◆ black diamond
            | '\u{25CA}' // ◊ lozenge
    )
    .then_some(0.62)
}

/// The size (twips) a run shapes at and its baseline shift (twips, positive =
/// raised toward the top of the line), from `w:vertAlign` and `w:position`.
/// Super/subscript raise (~1/3 of the size) / lower (~1/6) the baseline and shrink
/// the glyphs to ~2/3; `w:position` adds a half-point baseline raise(+)/lower(−)
/// with no resize. The two compose.
fn run_metrics(properties: &RunProperties) -> (Twip, Twip) {
    run_metrics_with_base(properties, properties.size_half_points)
}

/// [`run_metrics`] with the base size (in `w:sz`-style half-points) supplied
/// explicitly, so a complex-script slot can shape at `w:szCs`
/// ([`size_complex_half_points`](RunProperties::size_complex_half_points)) instead
/// of the Latin `w:sz`. The super/subscript and `w:position` shifts derive from
/// the same base, so they scale with the complex-script size too.
fn run_metrics_with_base(
    properties: &RunProperties,
    base_half_points: Option<u32>,
) -> (Twip, Twip) {
    // `w:sz` is in half-points; a half-point is 10 twips (a point is 20). Default
    // to 11pt (Word's default body size) when unset.
    let base = base_half_points.map_or(Twip::from_points(11), |hp| Twip(hp as i32 * 10));
    let (size, mut shift) = match properties.vertical_alignment {
        Some(VerticalAlignment::Superscript) => (Twip(base.raw() * 2 / 3), Twip(base.raw() / 3)),
        Some(VerticalAlignment::Subscript) => (Twip(base.raw() * 2 / 3), Twip(-base.raw() / 6)),
        Some(VerticalAlignment::Baseline) | None => (base, Twip::ZERO),
    };
    // `w:position` is a half-point baseline offset (positive raises), no resize.
    if let Some(hp) = properties.position_half_points {
        shift = Twip(shift.raw() + hp * 10);
    }
    (size, shift)
}

/// Applies a cumulative DrawingML normal-autofit font scale to the metrics that
/// affect shaping and baseline placement.
fn scaled_run_metrics(properties: &RunProperties, scale: u32) -> (Twip, Twip) {
    let (size, shift) = run_metrics(properties);
    let size = scale_twip(size, scale).max(Twip(1));
    (size, scale_twip(shift, scale))
}

fn scale_twip(value: Twip, scale: u32) -> Twip {
    let scaled = i64::from(value.raw()).saturating_mul(i64::from(scale)) / 100_000;
    Twip(scaled.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

/// Applies `w:caps`/`w:smallCaps` casing: both uppercase the run text (small-caps
/// per-letter sizing is layered on by [`push_styled_runs`]). Untransformed text is
/// borrowed, so the common case stays allocation-free.
fn case_transform<'a>(text: &'a str, properties: &RunProperties) -> Cow<'a, str> {
    if properties.all_caps == Some(true) || properties.small_caps == Some(true) {
        Cow::Owned(text.to_uppercase())
    } else {
        Cow::Borrowed(text)
    }
}

/// Pushes the styled run(s) for a model run. `w:smallCaps` splits the run so that
/// originally-lowercase letters are uppercased and shaped at ~3/4 size while the
/// rest keep full size (Word's small-caps look); every other run yields exactly one
/// styled run via [`styled_run`].
fn push_styled_runs<'a>(
    text: &'a str,
    properties: &RunProperties,
    ctx: &mut FlowCtx,
    out: &mut Vec<StyledRun<'a>>,
) {
    // Resolve the effective run properties once (cascade: docDefaults → paragraph
    // style → character style → direct), then build from the resolved value so
    // style-driven size/bold/caps/color are honored, not just direct props.
    let effective =
        ctx.cascade
            .resolve_run_in_table(ctx.para_style, properties, ctx.table_style.as_ref());
    if effective.hidden == Some(true) {
        return;
    }
    if effective.small_caps == Some(true) {
        push_small_caps_runs(text, &effective, ctx, out);
    } else if has_per_script_formatting(&effective) {
        // The run carries per-script font/formatting (`w:eastAsia`/`w:cs` fonts,
        // `w:bCs`/`w:iCs`/`w:szCs`, or an East-Asian/complex `w:rFonts@hint`), so
        // resolve each script span against its own slot (ECMA-376 §17.3.2.26)
        // instead of shaping the whole run through the Latin (ascii) slot. Latin
        // runs — and every run without these fields — never reach this arm, so
        // their single-run output is unchanged.
        for (span, slot) in script::partition_by_slot(text, effective.font_hint) {
            out.push(build_script_run(span, &effective, slot, ctx));
        }
    } else {
        out.push(build_styled_run(text, &effective, ctx));
    }
}

/// Whether a run declares any per-script formatting that makes the ascii-only slot
/// wrong for some of its code points: an explicit East-Asian/complex-script font
/// (`w:eastAsia`/`w:cs`), a complex-script bold/italic/size (`w:bCs`/`w:iCs`/
/// `w:szCs`), or an East-Asian/complex `w:rFonts@hint`. A run with none of these
/// keeps the single-slot fast path, so the existing Latin corpus is byte-for-byte
/// unchanged; only runs that actually carry per-script intent are re-slotted.
fn has_per_script_formatting(properties: &RunProperties) -> bool {
    properties.font_ref_east_asia.is_some()
        || properties.font_ref_cs.is_some()
        || properties.bold_complex.is_some()
        || properties.italic_complex.is_some()
        || properties.size_complex_half_points.is_some()
        || matches!(
            properties.font_hint,
            Some(RunFontHint::EastAsia | RunFontHint::Cs)
        )
}

/// Builds a [`StyledRun`] for one script span of a run, selecting the face (and,
/// for the complex-script slot, the bold/italic/size) from the matching
/// `w:rFonts` slot. The complex-script slot reads `w:bCs`/`w:iCs`/`w:szCs`
/// (falling back to the Latin `w:b`/`w:i`/`w:sz` when a complex field is unset);
/// the East-Asian and default slots use the Latin toggles/size. Every other run
/// attribute (color, decoration, letter spacing, scale, highlight, baseline
/// shift) is script-independent and shared across the run's spans.
///
/// For a `Default` span of a run that declares no complex fields this produces
/// exactly what [`build_styled_run`] does, so a run whose text is entirely
/// default-slot yields an identical single styled run.
fn build_script_run<'a>(
    text: &'a str,
    properties: &RunProperties,
    slot: ScriptSlot,
    ctx: &mut FlowCtx,
) -> StyledRun<'a> {
    let base_half_points = match slot {
        ScriptSlot::ComplexScript => properties
            .size_complex_half_points
            .or(properties.size_half_points),
        ScriptSlot::EastAsia | ScriptSlot::Default => properties.size_half_points,
    };
    let (raw_size, raw_shift) = run_metrics_with_base(properties, base_half_points);
    let size = scale_twip(raw_size, ctx.text_scale).max(Twip(1));
    let baseline_shift = scale_twip(raw_shift, ctx.text_scale);
    let (bold, italic) = match slot {
        ScriptSlot::ComplexScript => (
            properties.bold_complex.or(properties.bold).unwrap_or(false),
            properties
                .italic_complex
                .or(properties.italic)
                .unwrap_or(false),
        ),
        ScriptSlot::EastAsia | ScriptSlot::Default => (
            properties.bold.unwrap_or(false),
            properties.italic.unwrap_or(false),
        ),
    };
    let text = case_transform(text, properties);
    let family = requested_font_family_for(properties, ctx.scheme, slot);
    StyledRun {
        font: resolve_font_family(text.as_ref(), family.clone(), bold, italic, ctx),
        requested_family: family.map(Cow::Owned),
        text,
        size,
        character_scale_percent: properties.character_scale_percent.unwrap_or(100),
        bold,
        italic,
        letter_spacing: scale_twip(
            properties.character_spacing_twips.map_or(Twip::ZERO, Twip),
            ctx.text_scale,
        ),
        color: run_color(properties.color, ctx.palette),
        decoration: Decoration {
            underline: properties.underline.unwrap_or(false),
            strikethrough: properties.strike.unwrap_or(false),
        },
        highlight: properties.highlight.and_then(highlight_rgba),
        baseline_shift,
    }
}

/// Splits a small-caps run into uppercased spans, shaping originally-lowercase
/// spans at ~3/4 of the run size and every other span at full size. Each span's
/// text is owned (uppercased), so the pushed runs borrow nothing from `text`.
fn push_small_caps_runs<'a>(
    text: &str,
    properties: &RunProperties,
    ctx: &mut FlowCtx,
    out: &mut Vec<StyledRun<'a>>,
) {
    let (base, baseline_shift) = scaled_run_metrics(properties, ctx.text_scale);
    let bold = properties.bold.unwrap_or(false);
    let italic = properties.italic.unwrap_or(false);
    let letter_spacing = scale_twip(
        properties.character_spacing_twips.map_or(Twip::ZERO, Twip),
        ctx.text_scale,
    );
    let color = run_color(properties.color, ctx.palette);
    let decoration = Decoration {
        underline: properties.underline.unwrap_or(false),
        strikethrough: properties.strike.unwrap_or(false),
    };
    let highlight = properties.highlight.and_then(highlight_rgba);
    let family = requested_family(properties, ctx.scheme);
    for (span, was_lower) in small_caps_spans(text) {
        let size = if was_lower {
            Twip(base.raw() * 3 / 4)
        } else {
            base
        };
        let upper = span.to_uppercase();
        let font = resolve_font(&upper, properties, bold, italic, ctx);
        out.push(StyledRun {
            text: Cow::Owned(upper),
            requested_family: family.clone().map(Cow::Owned),
            font,
            size,
            character_scale_percent: properties.character_scale_percent.unwrap_or(100),
            bold,
            italic,
            letter_spacing,
            color,
            decoration,
            highlight,
            baseline_shift,
        });
    }
}

/// Splits `text` into maximal spans that are each either all originally-lowercase
/// letters or all other characters, in order, tagging each span with whether it was
/// the lowercase group (which small-caps shrinks).
fn small_caps_spans(text: &str) -> Vec<(&str, bool)> {
    let mut spans = Vec::new();
    let mut start = 0;
    let mut group: Option<bool> = None;
    for (i, ch) in text.char_indices() {
        let lower = ch.is_lowercase();
        match group {
            Some(g) if g == lower => {}
            Some(g) => {
                spans.push((&text[start..i], g));
                start = i;
            }
            None => {}
        }
        group = Some(lower);
    }
    if let Some(g) = group {
        spans.push((&text[start..], g));
    }
    spans
}

/// Resolves a run's `w:color` to opaque RGBA: an explicit `w:color@val` sRGB is
/// itself; a `w:themeColor` resolves against the document's theme palette (with any
/// tint/shade applied); an absent color (`w:color@val="auto"` / unset) is black.
fn run_color(color: Option<Color>, palette: Option<&ResolvedPalette>) -> [u8; 4] {
    match color {
        Some(Color::Rgb(rgb)) => [rgb.r, rgb.g, rgb.b, 255],
        Some(Color::Theme(theme)) => match palette {
            // The model carries no tint/shade on a theme color yet, so the factors
            // are `None` today; routing every theme color through `apply_tint_shade`
            // keeps the resolution complete for when the model gains them.
            Some(palette) => apply_tint_shade(palette.slot(theme.slot), None, None),
            None => [0, 0, 0, 255],
        },
        None => [0, 0, 0, 255],
    }
}

/// The document's theme color scheme (`a:clrScheme`) resolved to one opaque RGBA
/// per slot, so a `w:themeColor` reference resolves to the real color rather than
/// silently rendering black.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResolvedPalette {
    /// The twelve slots, in `ColorScheme` field order (see [`theme_slot_index`]).
    slots: [[u8; 4]; 12],
}

impl ResolvedPalette {
    /// The resolved RGBA for a theme color slot.
    fn slot(&self, slot: ThemeColorRef) -> [u8; 4] {
        self.slots[theme_slot_index(slot)]
    }
}

/// The [`ResolvedPalette`] index for a theme color slot (matching [`ColorScheme`]'s
/// OOXML child order: dk1, lt1, dk2, lt2, accent1..6, hlink, folHlink).
fn theme_slot_index(slot: ThemeColorRef) -> usize {
    match slot {
        ThemeColorRef::Dark1 => 0,
        ThemeColorRef::Light1 => 1,
        ThemeColorRef::Dark2 => 2,
        ThemeColorRef::Light2 => 3,
        ThemeColorRef::Accent1 => 4,
        ThemeColorRef::Accent2 => 5,
        ThemeColorRef::Accent3 => 6,
        ThemeColorRef::Accent4 => 7,
        ThemeColorRef::Accent5 => 8,
        ThemeColorRef::Accent6 => 9,
        ThemeColorRef::Hyperlink => 10,
        ThemeColorRef::FollowedHyperlink => 11,
    }
}

/// Resolves a document's [`ColorScheme`] to a [`ResolvedPalette`]: each slot's
/// `a:srgbClr` becomes its RGB and each `a:sysClr` resolves to its `lastClr` (or a
/// sensible default for the named system color when none was recorded).
fn resolve_palette(scheme: &ColorScheme) -> ResolvedPalette {
    ResolvedPalette {
        slots: [
            resolve_scheme_color(&scheme.dark1),
            resolve_scheme_color(&scheme.light1),
            resolve_scheme_color(&scheme.dark2),
            resolve_scheme_color(&scheme.light2),
            resolve_scheme_color(&scheme.accent1),
            resolve_scheme_color(&scheme.accent2),
            resolve_scheme_color(&scheme.accent3),
            resolve_scheme_color(&scheme.accent4),
            resolve_scheme_color(&scheme.accent5),
            resolve_scheme_color(&scheme.accent6),
            resolve_scheme_color(&scheme.hyperlink),
            resolve_scheme_color(&scheme.followed_hyperlink),
        ],
    }
}

/// Resolves one scheme color slot to opaque RGBA: an `a:srgbClr` is its RGB; an
/// `a:sysClr` uses its recorded `lastClr`, falling back to the conventional value
/// for the named system color (see [`default_system_color`]).
fn resolve_scheme_color(color: &SchemeColor) -> [u8; 4] {
    match color {
        SchemeColor::Srgb(rgb) => [rgb.r, rgb.g, rgb.b, 255],
        SchemeColor::System(sys) => match sys.last_color {
            Some(rgb) => [rgb.r, rgb.g, rgb.b, 255],
            None => default_system_color(&sys.value),
        },
    }
}

/// The conventional sRGB for a named `a:sysClr` token when no `lastClr` was
/// recorded: the light "surface" tokens are white, everything else (text-like) is
/// black — enough to keep an unrecorded system color legible.
fn default_system_color(value: &str) -> [u8; 4] {
    match value {
        "window" | "background" | "btnFace" | "menu" | "3dLight" => [255, 255, 255, 255],
        _ => [0, 0, 0, 255],
    }
}

/// Applies an OOXML `tint` (lightens toward white) and/or `shade` (darkens toward
/// black) to an RGB, each a `0.0..=1.0` factor; `None` leaves the channel
/// unchanged. Alpha is preserved. Ready for when the model carries theme
/// tint/shade — [`run_color`] routes every resolved theme color through it.
fn apply_tint_shade(rgb: [u8; 4], tint: Option<f32>, shade: Option<f32>) -> [u8; 4] {
    let adjust = |c: u8| -> u8 {
        let mut v = f32::from(c);
        if let Some(t) = tint {
            let t = t.clamp(0.0, 1.0);
            v = v * t + 255.0 * (1.0 - t);
        }
        if let Some(s) = shade {
            v *= s.clamp(0.0, 1.0);
        }
        v.round().clamp(0.0, 255.0) as u8
    };
    [adjust(rgb[0]), adjust(rgb[1]), adjust(rgb[2]), rgb[3]]
}

/// Resolves a run's declared font family to a bundled face, records any
/// substitution and per-glyph coverage fallback, and returns the chosen face. A
/// run with no declared family uses the bundled default (matching weight/style).
fn resolve_font(
    text: &str,
    properties: &RunProperties,
    bold: bool,
    italic: bool,
    ctx: &mut FlowCtx,
) -> FontId {
    resolve_font_family(
        text,
        requested_family(properties, ctx.scheme),
        bold,
        italic,
        ctx,
    )
}

/// Resolves an already-picked family name to a bundled face and records coverage.
/// Shared by the ascii-slot path ([`resolve_font`]) and the per-script-slot path
/// ([`build_script_run`]), which pass the family they selected for the run's
/// script so the renderer outlines the same face `parley` shapes with.
fn resolve_font_family(
    text: &str,
    family: Option<String>,
    bold: bool,
    italic: bool,
    ctx: &mut FlowCtx,
) -> FontId {
    let face = match family {
        Some(family) => {
            let outcome = ctx.resolver.resolve(&FaceRequest {
                family: &family,
                bold,
                italic,
            });
            ctx.report.note_resolution(&family, &outcome);
            outcome.face
        }
        None => crate::fonts::face_id(bold, italic),
    };
    ctx.resolver.record_coverage(face, text, ctx.report);
    face
}

/// The concrete family name a run requests through its ascii `w:rFonts` slot,
/// resolving a theme reference against the document's font scheme. `None` when the
/// run declares no family (it inherits the default).
fn requested_family(properties: &RunProperties, scheme: Option<&FontScheme>) -> Option<String> {
    requested_font_family(properties, scheme)
}

/// Maps model paragraph alignment to the layout alignment.
fn alignment(properties: &ParagraphProperties) -> TextAlignment {
    match properties.alignment {
        Some(Alignment::Start) | None => TextAlignment::Start,
        Some(Alignment::End) => TextAlignment::End,
        Some(Alignment::Center) => TextAlignment::Center,
        Some(Alignment::Justify) => TextAlignment::Justify,
    }
}

/// Maps paragraph break properties to the fragment's break control.
fn break_control(properties: &ParagraphProperties) -> BreakControl {
    BreakControl {
        page_break_before: properties.page_break_before,
        keep_next: properties.keep_next,
        keep_lines: properties.keep_lines,
        widow_control: properties.widow_control,
    }
}

/// Maps paragraph spacing/indent to the fragment's box metrics. `before`/`after`
/// honor `w:beforeAutospacing`/`w:afterAutospacing`: when the auto flag is set,
/// Word ignores the explicit twip value and uses a font-size-derived default
/// ([`auto_paragraph_space`]) instead.
fn box_metrics(properties: &ParagraphProperties) -> BoxMetrics {
    let spacing = properties.spacing.as_ref();
    let indent = properties.indentation.as_ref();
    let auto = auto_paragraph_space(properties);
    let before = match spacing {
        Some(s) if s.before_auto == Some(true) => auto,
        Some(s) => s.before_twips.map_or(Twip::ZERO, Twip),
        None => Twip::ZERO,
    };
    let after = match spacing {
        Some(s) if s.after_auto == Some(true) => auto,
        Some(s) => s.after_twips.map_or(Twip::ZERO, Twip),
        None => Twip::ZERO,
    };
    BoxMetrics {
        space_before: before,
        space_after: after,
        indent_start: indent.and_then(|i| i.start_twips).map_or(Twip::ZERO, Twip),
        indent_end: indent.and_then(|i| i.end_twips).map_or(Twip::ZERO, Twip),
    }
}

/// The font-size-derived "auto" paragraph space (`w:beforeAutospacing` /
/// `w:afterAutospacing`): Word derives a blank-line-sized gap from the paragraph
/// font. The paragraph-mark run size drives it (falling back to Word's 11pt body
/// default); one font-size worth of twips is a close, deterministic approximation.
fn auto_paragraph_space(properties: &ParagraphProperties) -> Twip {
    let half_points = properties
        .mark_run
        .as_deref()
        .and_then(|r| r.size_half_points)
        .unwrap_or(22);
    Twip((half_points as i32 * 10).max(0))
}

/// Resolves a paragraph's list marker (when it carries `w:numPr`), returning the
/// body line constraints and the positioned marker to inject into the first shaped
/// line. Advances the numbering counter exactly once (so this must be called once
/// per paragraph, in document order) and merges the numbering level's indentation
/// *below* the paragraph's direct/style indentation.
///
/// All counter and number/glyph-format logic lives in [`crate::numbering`]; this
/// hook only builds the marker's styled run through the flow engine's font cascade
/// and resolver — bullets frequently declare a Symbol/Wingdings face, which the
/// level's `w:rPr` carries — and shapes it to glyphs, then hands the geometry back.
///
/// The body's first-line indent is overridden to where the marker's suffix places
/// the text: the left indent for a hanging list (the marker protruding into the
/// hanging space), or past the marker for a space/no suffix.
fn prepare_list_marker(
    node: casual_doc_model::NodeId,
    props: &mut ParagraphProperties,
    width: Twip,
    ctx: &mut FlowCtx,
    shaper: &dyn LineShaper,
) -> (LineConstraints, Option<PreparedMarker>) {
    let Some(reference) = props.numbering else {
        return (
            line_constraints(props, width, ctx.line_spacing_reduction),
            None,
        );
    };
    let Some(resolved) = ctx.numbering.resolve(ctx.definitions, &reference) else {
        // A dangling numbering reference: flow the paragraph with no marker.
        return (
            line_constraints(props, width, ctx.line_spacing_reduction),
            None,
        );
    };
    // The numbering level's indentation is lower precedence than the paragraph's own
    // (direct + style, already folded into `props`), so the paragraph wins per field
    // and the level fills in what the paragraph leaves unset (the common case — the
    // list's indent/hanging live on the level).
    if let Some(level_indent) = resolved.level_indent {
        props.indentation = Some(merge_indent_over(level_indent, props.indentation));
    }
    let mut constraints = line_constraints(props, width, ctx.line_spacing_reduction);
    // The marker's left edge is the first-line indent: negative for a hanging indent
    // (the marker protrudes left of the body, into the hanging space).
    let marker_x = constraints.first_line_indent;

    // Build the marker run through the cascade so it inherits the paragraph's
    // size/color, with the level's own `w:rPr` (face/size/color of the number or
    // bullet) layered on top. A space suffix is folded into the run's text.
    let level_rpr = resolved.run_properties.unwrap_or_default();
    let mut effective =
        ctx.cascade
            .resolve_run_in_table(ctx.para_style, &level_rpr, ctx.table_style.as_ref());
    // Route the bullet glyph through the symbol map: a Wingdings/Symbol bullet code
    // point becomes its Unicode equivalent (●/○/▪/…). When a glyph is remapped, drop
    // the (unbundled, glyph-incompatible) symbol face so the mapped Unicode char
    // resolves against a covering text face instead of rendering as tofu.
    let family = requested_family(&effective, ctx.scheme);
    let (glyph_text, remapped) = map_marker_glyphs(&resolved.text, family.as_deref());
    if remapped {
        effective.font_ref = None;
    }
    let marker_text = match resolved.suffix {
        LevelSuffix::Space => format!("{glyph_text} "),
        LevelSuffix::Tab | LevelSuffix::Nothing => glyph_text,
    };
    if marker_text.is_empty() {
        // A `none`-format level renders no glyph, but the counter has advanced and
        // the body sits at the left indent (no hanging protrusion).
        constraints.first_line_indent =
            numbering::body_indent(resolved.suffix, marker_x, Twip::ZERO, ctx.default_tab);
        return (constraints, None);
    }
    let mut marker_run = build_styled_run(&marker_text, &effective, ctx);
    // A full-em geometric bullet (●/○/■/…) shapes near cap-height; Word/LibreOffice
    // draw list bullets much smaller. Scale the marker to match so a sub-list ○ is a
    // small ring, not a full-height circle.
    if let Some(scale) = bullet_scale(&marker_text) {
        let scaled = (marker_run.size.raw() as f32 * scale).round() as i32;
        marker_run.size = Twip(scaled.max(1));
    }
    let range = ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0));
    // Shape the marker unwrapped: it is a short prefix, never a wrapped block.
    let unwrapped = LineConstraints {
        max_width: Twip(1 << 28),
        ..LineConstraints::default()
    };
    let layout = shaper.shape_paragraph(std::slice::from_ref(&marker_run), unwrapped, range);
    let (runs, marker_width, ascent, descent) = match layout.lines.into_iter().next() {
        Some(line) => {
            let marker_width = line
                .runs
                .iter()
                .map(|r| r.origin.x + r.glyphs.iter().fold(Twip::ZERO, |a, g| a + g.advance))
                .max()
                .unwrap_or(Twip::ZERO);
            (line.runs, marker_width, line.ascent, line.descent)
        }
        None => (Vec::new(), Twip::ZERO, Twip::ZERO, Twip::ZERO),
    };
    constraints.first_line_indent =
        numbering::body_indent(resolved.suffix, marker_x, marker_width, ctx.default_tab);
    (
        constraints,
        Some(PreparedMarker::new(runs, marker_x, ascent, descent)),
    )
}

/// Merges a numbering level's indentation (`base`, lower precedence) under a
/// paragraph's own (`over`): each field the paragraph sets wins; the level fills the
/// rest. Mirrors the cascade's per-field indentation merge.
fn merge_indent_over(base: Indentation, over: Option<Indentation>) -> Indentation {
    let Some(over) = over else {
        return base;
    };
    Indentation {
        start_twips: over.start_twips.or(base.start_twips),
        end_twips: over.end_twips.or(base.end_twips),
        first_line_twips: over.first_line_twips.or(base.first_line_twips),
        hanging_twips: over.hanging_twips.or(base.hanging_twips),
    }
}

/// Builds the shaper constraints for a paragraph flowed into `width`: the wrap
/// width is the column width less the start/end indents (so lines wrap at the
/// indented column), and the first-line indent carries `w:ind@firstLine`
/// (positive) or `w:ind@hanging` (negative, protruding). Hanging wins when both
/// are present, matching Word.
fn line_constraints(
    properties: &ParagraphProperties,
    width: Twip,
    line_spacing_reduction: u32,
) -> LineConstraints {
    let spacing = properties.spacing.as_ref();
    let metrics = box_metrics(properties);
    let indent = properties.indentation.as_ref();
    let first_line_indent = match indent {
        Some(i) if i.hanging_twips.unwrap_or(0) != 0 => Twip(-i.hanging_twips.unwrap_or(0)),
        Some(i) => Twip(i.first_line_twips.unwrap_or(0)),
        None => Twip::ZERO,
    };
    let max_width =
        Twip((width.raw() - metrics.indent_start.raw() - metrics.indent_end.raw()).max(1));
    // Resolve the line-spacing rule into the shaper's line-box controls. The
    // `auto` multiple rides `line_height_percent` (parley's `MetricsRelative`); the
    // `atLeast`/`exact` rules carry an explicit twip box the shaper post-applies.
    let (line_height_percent, line_at_least, line_exact) = match spacing {
        Some(s) => match s.line_rule {
            Some(LineRule::Exact) => (None, None, s.line_twips.map(Twip)),
            Some(LineRule::AtLeast) => (None, s.line_twips.map(Twip), None),
            // An explicit `auto` rule (or none) uses the authored multiple,
            // including values below single spacing. OOXML stores this in 240ths
            // (`w:line="168"` = 0.70×); flooring it to 1.0 silently expands dense
            // forms/SDS paragraphs and changes pagination.
            Some(LineRule::Auto) | None => (
                s.line_percent.map(|p| {
                    let reduction =
                        ((line_spacing_reduction.saturating_add(500)) / 1_000).min(99) as u16;
                    p.saturating_sub(reduction).max(1)
                }),
                None,
                None,
            ),
        },
        None => (None, None, None),
    };
    LineConstraints {
        max_width,
        margin_width: width,
        indent_start: metrics.indent_start,
        // Paragraph base direction: `w:bidi` (right-to-left paragraph) sets the
        // shaper's base level to RTL so the line is laid out right-to-left and a
        // start/end alignment resolves against the correct edge. The effective
        // value already carries the section-level default, the paragraph style
        // chain, and the paragraph's own `w:bidi` (folded in by the cascade's
        // `overlay_paragraph`), so this reads the resolved paragraph intent. An
        // unset (or explicit `w:val="0"`) `w:bidi` stays LTR, unchanged from
        // before. Note: this flags the paragraph's *base direction*; the shaper's
        // per-run Unicode-bidi reordering within the line is a separate concern
        // (`docs/55` §7 — full visual reordering of mixed runs remains open).
        rtl: properties.bidi.unwrap_or(false),
        alignment: alignment(properties),
        line_height_percent,
        line_at_least,
        line_exact,
        first_line_indent,
    }
}

/// Ensures a paragraph occupies at least one line box. `parley` yields no line for
/// a paragraph with no text, so an empty paragraph would otherwise collapse to ~0
/// height (just its box spacing). Word instead gives it a full line, sized by the
/// **paragraph-mark** run's (cascade-resolved) font metrics and the paragraph's
/// line rule — a major vertical-rhythm and page-count factor, since documents use
/// blank paragraphs for spacing. The synthesized line carries no glyphs (nothing
/// is painted) but the correct height.
/// Whether a body paragraph that ends section `ended` (its `w:pPr` carried that
/// section's `w:sectPr`) forces a page break after its last line.
///
/// A `w:sectPr` in a paragraph's `w:pPr` marks the paragraph as the *last* of its
/// section. Whether the content that follows begins on a new page is governed, per
/// ECMA-376, by the *next* section's start type (`w:type`): `nextPage`/`evenPage`/
/// `oddPage` — and the unspecified default, which is `nextPage` — begin on a new
/// page, so a page break follows this paragraph; a `continuous` section (or, in
/// single-column flow, `nextColumn`) continues on the same page, so no break. The
/// document's final section is body-level (carried by no paragraph), so a
/// paragraph-carried section always has a following section; the defensive "no
/// next section → no break" keeps a malformed document from gaining a stray break.
fn section_break_forces_page(sections: &[SectionBoundary], ended: SectionId) -> bool {
    let Some(index) = sections.iter().position(|s| s.id == ended) else {
        return false;
    };
    let Some(next) = sections.get(index + 1) else {
        return false;
    };
    !matches!(
        next.section_type,
        Some(SectionType::Continuous | SectionType::NextColumn)
    )
}

/// Applies a paragraph-embedded section break to its shaped lines: if the paragraph
/// ends a section whose successor starts on a new page, force a page break after
/// the paragraph's last line (the same mechanism a trailing `w:br` type `page`
/// uses). No-op for a paragraph that carries no section break or whose successor is
/// continuous. Runs after [`ensure_nonempty_paragraph`], so an empty
/// section-terminating paragraph (e.g. a lone cover-page `sectPr`) has a line to
/// carry the break.
fn apply_section_break(lines: &mut LineLayout, properties: &ParagraphProperties, ctx: &FlowCtx) {
    if let Some(ended) = properties.section_break
        && section_break_forces_page(ctx.sections, ended)
        && let Some(last) = lines.lines.last_mut()
    {
        last.line_break = LineBreak::Page;
        last.page_break_after = true;
    }
}

/// [`apply_section_break`] on a built fragment (the incremental cache path stamps
/// the break onto the fragment rather than baking it into the cached lines). A
/// non-paragraph fragment (a table row) never carries a paragraph section break.
fn apply_section_break_to_fragment(
    fragment: &mut BlockFragment,
    properties: &ParagraphProperties,
    ctx: &FlowCtx,
) {
    if let BlockFragment::Paragraph { lines, .. } = fragment {
        apply_section_break(lines, properties, ctx);
    }
}

fn ensure_nonempty_paragraph(
    layout: &mut LineLayout,
    props: &ParagraphProperties,
    ctx: &mut FlowCtx,
    shaper: &dyn LineShaper,
    width: Twip,
    range: ModelRange,
) {
    if !layout.lines.is_empty() {
        return;
    }
    let mark = props.mark_run.as_deref().cloned().unwrap_or_default();
    let styled = styled_run(" ", &mark, ctx);
    let probe = shaper.shape_paragraph(
        &[styled],
        line_constraints(props, width, ctx.line_spacing_reduction),
        range,
    );
    if let Some(mut line) = probe.lines.into_iter().next() {
        line.runs.clear();
        line.images.clear();
        line.fields.clear();
        line.text_boxes.clear();
        line.bars.clear();
        line.range = range;
        line.line_break = LineBreak::ParagraphEnd;
        layout.lines.push(line);
    }
}

/// Builds the paint-only decoration (background shading + borders) of a paragraph
/// box flowed into `width`. Empty (serializes to nothing) for a plain paragraph.
fn paragraph_decor(properties: &ParagraphProperties, width: Twip) -> ParagraphDecor {
    let shading = properties.shading.fill.map(|c| [c.r, c.g, c.b, 255]);
    let b = &properties.borders;
    let borders = BlockBorders {
        top: single_edge(b.top.as_ref()),
        bottom: single_edge(b.bottom.as_ref()),
        start: single_edge(b.start.as_ref()),
        end: single_edge(b.end.as_ref()),
    };
    if shading.is_none() && borders.is_empty() {
        return ParagraphDecor::default();
    }
    ParagraphDecor {
        shading,
        borders,
        width,
    }
}

/// Converts a single model border edge to a drawable [`ResolvedEdge`] (no
/// conflict resolution — paragraph borders have a single candidate per edge),
/// returning `None` for an absent or invisible (`nil`/`none`) edge.
fn single_edge(edge: Option<&BorderEdge>) -> Option<ResolvedEdge> {
    let edge = edge?;
    if !is_visible_border(edge) {
        return None;
    }
    let color = edge.color.map_or([0, 0, 0, 255], |c| [c.r, c.g, c.b, 255]);
    // `w:sz` is in eighths of a point; a point is 20 twips.
    let width = edge
        .size_eighth_points
        .map_or(Twip(10), |sz| Twip(((sz * 20) / 8).max(1) as i32));
    Some(ResolvedEdge {
        color,
        width,
        pattern: border_pattern(&edge.style),
    })
}

/// Resolves a named `w:highlight` color to an opaque RGBA fill. `None`
/// (`ST_HighlightColor` `none`) yields no highlight.
fn highlight_rgba(color: HighlightColor) -> Option<[u8; 4]> {
    let (r, g, b) = match color {
        HighlightColor::None => return None,
        HighlightColor::Black => (0, 0, 0),
        HighlightColor::Blue => (0, 0, 255),
        HighlightColor::Cyan => (0, 255, 255),
        HighlightColor::DarkBlue => (0, 0, 139),
        HighlightColor::DarkCyan => (0, 139, 139),
        HighlightColor::DarkGray => (169, 169, 169),
        HighlightColor::DarkGreen => (0, 100, 0),
        HighlightColor::DarkMagenta => (139, 0, 139),
        HighlightColor::DarkRed => (139, 0, 0),
        HighlightColor::DarkYellow => (153, 153, 0),
        HighlightColor::Green => (0, 255, 0),
        HighlightColor::LightGray => (211, 211, 211),
        HighlightColor::Magenta => (255, 0, 255),
        HighlightColor::Red => (255, 0, 0),
        HighlightColor::White => (255, 255, 255),
        HighlightColor::Yellow => (255, 255, 0),
    };
    Some([r, g, b, 255])
}

/// Deterministic placeholder label for an unrendered `w:altChunk` block (see
/// [`alt_chunk_fragment`]). Not the chunk's actual content — just a fixed,
/// visible marker, in the spirit of [`crate::symbol_map::resolve_symbol`]'s `□`
/// fallback for an unmapped symbol glyph.
const ALT_CHUNK_PLACEHOLDER_TEXT: &str = "\u{2b1a} altChunk (embedded content not rendered)";

/// Builds a placeholder fragment for an unrendered `w:altChunk` (aggregated
/// external content chunk).
///
/// `P1F-28`: the semantic model carries only an opaque part reference for
/// `w:altChunk` — the referenced HTML/RTF/nested-WordprocessingML part's bytes
/// are byte-preserved by the opaque side-table, but never parsed into blocks —
/// so there is nothing to recurse into and lay out for real. Contributing zero
/// layout space (the prior behavior) would still leave the reader unable to
/// tell the chunk was ever there, and would under-count pagination relative to
/// Word. Instead, this reserves one deterministic, visible, dashed-bordered
/// line the full column width — a fixed placeholder box, **not** a rendering of
/// the chunk's actual embedded content. Full altChunk content flow (parsing the
/// embedded part and laying out its real blocks) remains a separate, larger,
/// tracked effort.
fn alt_chunk_fragment(
    chunk: &AltChunk,
    shaper: &dyn LineShaper,
    width: Twip,
    ctx: &mut FlowCtx,
) -> BlockFragment {
    // Document-default styling, independent of any preceding paragraph's
    // resolved style, so the placeholder's size/height is deterministic given
    // the document alone.
    ctx.para_style = None;
    let range = ModelRange::new(ModelPos::new(chunk.id, 0), ModelPos::new(chunk.id, 0));
    let run = styled_run(ALT_CHUNK_PLACEHOLDER_TEXT, &RunProperties::default(), ctx);
    let props = ParagraphProperties::default();
    let constraints = line_constraints(&props, width, ctx.line_spacing_reduction);
    let lines = shaper.shape_paragraph(&[run], constraints, range);
    BlockFragment::Paragraph {
        id: chunk.id,
        lines,
        box_metrics: BoxMetrics::default(),
        break_control: BreakControl::default(),
        decor: alt_chunk_decor(width),
    }
}

/// The [`alt_chunk_fragment`] placeholder's paint-only decoration: a dashed,
/// neutral-gray border on all four edges, spanning the flowed column width.
/// Dashed (rather than a plain solid rule) so the box reads as an
/// engine-drawn approximation, not authored `w:pBdr` content.
fn alt_chunk_decor(width: Twip) -> ParagraphDecor {
    let edge = ResolvedEdge {
        color: [128, 128, 128, 255],
        width: Twip(10),
        pattern: BorderPattern::Dashed,
    };
    ParagraphDecor {
        shading: None,
        borders: BlockBorders {
            top: Some(edge),
            bottom: Some(edge),
            start: Some(edge),
            end: Some(edge),
        },
        width,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::ParleyShaper;
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{
        BlockNode, CommentId, Definitions, Document, EmbeddedObject, EmbeddedPart, Extent,
        InlineNode, Math, MathExpression, MediaId, MediaReference, NoBreakHyphen, Note, NoteId,
        NoteReference, Paragraph, ParagraphProperties, Run, RunProperties, SoftHyphen, Spacing,
    };

    fn run_node(id: u64, text: &str, properties: RunProperties) -> InlineNode {
        InlineNode::Run(Run {
            id: NodeId::from_parts(id, 1).unwrap(),
            properties,
            text: text.to_owned(),
        })
    }

    fn paragraph(id: u64, inlines: Vec<InlineNode>) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(id, 1).unwrap(),
            properties: ParagraphProperties::default(),
            inlines,
        })
    }

    fn document(body: Vec<BlockNode>) -> Document {
        Document::new(
            NodeId::from_parts(1, 1).unwrap(),
            body,
            Definitions::default(),
        )
        .unwrap()
    }

    fn document_with_definitions(body: Vec<BlockNode>, definitions: Definitions) -> Document {
        Document::new(NodeId::from_parts(1, 1).unwrap(), body, definitions).unwrap()
    }

    fn collected_items<'a>(
        definitions: &'a Definitions,
        inlines: &'a [InlineNode],
    ) -> Vec<FlowItem<'a>> {
        let resolver = FontResolver::new();
        let shaper = ParleyShaper::new();
        let mut report = FontResolutionReport::new();
        let mut ctx = FlowCtx {
            resolver: &resolver,
            scheme: definitions.font_scheme.as_ref(),
            report: &mut report,
            default_tab: crate::tabs::DEFAULT_TAB_STOP,
            media: &definitions.media,
            palette: None,
            cascade: StyleCascade::new(definitions),
            para_style: None,
            table_style: None,
            sections: &[],
            definitions,
            numbering: NumberingState::new(),
            text_scale: 100_000,
            line_spacing_reduction: 0,
            paragraph_float_exclusions: None,
        };
        let mut items = Vec::new();
        collect_items(
            inlines,
            &mut items,
            &shaper,
            Twip::from_points(400),
            &mut ctx,
        );
        items
    }

    fn run_texts(items: &[FlowItem<'_>]) -> Vec<String> {
        items
            .iter()
            .filter_map(|item| match item {
                FlowItem::Run(run) => Some(run.text.to_string()),
                _ => None,
            })
            .collect()
    }

    /// The lines of the first paragraph fragment in a galley.
    fn first_paragraph_lines(galley: &[BlockFragment]) -> &crate::text::LineLayout {
        match &galley[0] {
            BlockFragment::Paragraph { lines, .. } => lines,
            BlockFragment::TableRow { .. } => panic!("expected a paragraph fragment"),
        }
    }

    /// The styled runs a single model run flows into, in document order, with the
    /// full flow cascade applied. Used to observe per-script slot selection.
    fn styled_runs_for(text: &str, properties: RunProperties) -> Vec<StyledRun<'static>> {
        let definitions = Definitions::default();
        let resolver = FontResolver::new();
        let mut report = FontResolutionReport::new();
        let mut ctx = FlowCtx {
            resolver: &resolver,
            scheme: definitions.font_scheme.as_ref(),
            report: &mut report,
            default_tab: crate::tabs::DEFAULT_TAB_STOP,
            media: &definitions.media,
            palette: None,
            cascade: StyleCascade::new(&definitions),
            para_style: None,
            table_style: None,
            sections: &[],
            definitions: &definitions,
            numbering: NumberingState::new(),
            text_scale: 100_000,
            line_spacing_reduction: 0,
            paragraph_float_exclusions: None,
        };
        let mut out = Vec::new();
        push_styled_runs(text, &properties, &mut ctx, &mut out);
        // Detach the borrow so the returned runs can outlive the borrowed `text`
        // (each run's text is owned once `case_transform`/partition copies it, but
        // borrowed spans must be cloned to `'static` here for the assertion site).
        out.into_iter()
            .map(|r| StyledRun {
                text: Cow::Owned(r.text.into_owned()),
                requested_family: r.requested_family.map(|f| Cow::Owned(f.into_owned())),
                ..r
            })
            .collect()
    }

    fn named_font_ref(name: &str) -> Option<casual_doc_model::v1::FontRef> {
        Some(casual_doc_model::v1::FontRef::Named(
            casual_doc_model::v1::FontName {
                name: name.to_owned(),
            },
        ))
    }

    #[test]
    fn line_constraints_derive_rtl_from_paragraph_bidi() {
        let width = Twip::from_points(400);

        // A default (LTR) paragraph stays left-to-right.
        let ltr = line_constraints(&ParagraphProperties::default(), width, 0);
        assert!(!ltr.rtl, "a paragraph without w:bidi is left-to-right");

        // A `w:bidi` paragraph is right-to-left.
        let rtl_props = ParagraphProperties {
            bidi: Some(true),
            ..ParagraphProperties::default()
        };
        let rtl = line_constraints(&rtl_props, width, 0);
        assert!(rtl.rtl, "a w:bidi paragraph is right-to-left");

        // An explicit `w:bidi w:val="0"` (off) stays left-to-right.
        let off_props = ParagraphProperties {
            bidi: Some(false),
            ..ParagraphProperties::default()
        };
        let off = line_constraints(&off_props, width, 0);
        assert!(
            !off.rtl,
            "an explicit w:bidi=off paragraph is left-to-right"
        );
    }

    #[test]
    fn mixed_script_run_selects_the_east_asian_face_for_cjk_only() {
        // One run, Latin ascii font + a distinct East-Asian font, mixing Latin and
        // CJK text. Each script must resolve against its own w:rFonts slot: the
        // Latin spans keep the ascii/hAnsi family, the CJK span uses eastAsia.
        let properties = RunProperties {
            font_ref: named_font_ref("Latin Font"),
            font_ref_east_asia: named_font_ref("EA Font"),
            ..RunProperties::default()
        };
        let runs = styled_runs_for("A中B", properties);

        assert_eq!(runs.len(), 3, "the run splits at each script boundary");
        assert_eq!(runs[0].text, "A");
        assert_eq!(
            runs[0].requested_family.as_deref(),
            Some("Latin Font"),
            "Latin code points keep the ascii/hAnsi face"
        );
        assert_eq!(runs[1].text, "中");
        assert_eq!(
            runs[1].requested_family.as_deref(),
            Some("EA Font"),
            "CJK code points use the eastAsia face"
        );
        assert_eq!(runs[2].text, "B");
        assert_eq!(
            runs[2].requested_family.as_deref(),
            Some("Latin Font"),
            "Latin code points after the CJK span keep the ascii/hAnsi face"
        );
    }

    #[test]
    fn complex_script_run_uses_the_cs_face_bold_and_size() {
        // An Arabic run with a complex-script font + complex-script bold and size
        // that differ from the Latin ones. The complex span must pick up the cs
        // face, w:bCs weight, and w:szCs size — not the Latin w:rFonts/w:b/w:sz.
        let properties = RunProperties {
            font_ref: named_font_ref("Latin Font"),
            font_ref_cs: named_font_ref("CS Font"),
            bold: Some(false),
            bold_complex: Some(true),
            size_half_points: Some(24),         // 12pt Latin
            size_complex_half_points: Some(40), // 20pt complex
            ..RunProperties::default()
        };
        let runs = styled_runs_for("العربية", properties);

        assert_eq!(runs.len(), 1, "an all-complex run is a single cs span");
        assert_eq!(
            runs[0].requested_family.as_deref(),
            Some("CS Font"),
            "complex code points use the cs face"
        );
        assert!(
            runs[0].bold,
            "complex code points use w:bCs (bold), not w:b"
        );
        assert_eq!(
            runs[0].size,
            Twip::from_points(20),
            "complex code points shape at w:szCs (20pt), not w:sz (12pt)"
        );
    }

    #[test]
    fn latin_run_without_per_script_fields_is_a_single_unchanged_run() {
        // The fast path: a run with no eastAsia/cs fonts, no bCs/iCs/szCs, and no
        // eastAsia/cs hint yields exactly one styled run through the original
        // single-slot path — the existing corpus is unaffected.
        let properties = RunProperties {
            font_ref: named_font_ref("Latin Font"),
            size_half_points: Some(24),
            ..RunProperties::default()
        };
        let runs = styled_runs_for("Hello world 123", properties);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hello world 123");
        assert_eq!(runs[0].requested_family.as_deref(), Some("Latin Font"));
        assert_eq!(runs[0].size, Twip::from_points(12));
    }

    #[test]
    fn stacking_a_multiline_batch_applies_its_base_once() {
        use crate::model::{ModelPos, ModelRange};
        use crate::text::{Decoration, FontId, Glyph, GlyphRun, LineBreak};

        let node = NodeId::from_parts(99, 1).unwrap();
        let range = ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 1));
        let line = |baseline| Line {
            runs: vec![GlyphRun {
                is_marker: false,
                font: FontId(0),
                size: Twip(100),
                character_scale_percent: 100,
                color: [0, 0, 0, 255],
                origin: Point::new(Twip::ZERO, Twip(baseline)),
                bidi_level: 0,
                decoration: Decoration::default(),
                highlight: None,
                glyphs: vec![Glyph {
                    id: 1,
                    advance: Twip(50),
                    cluster: 0,
                }],
            }],
            ascent: Twip(80),
            descent: Twip(20),
            height: Twip(100),
            clip: false,
            range,
            line_break: LineBreak::Wrap,
            page_break_after: false,
            bars: Vec::new(),
            images: Vec::new(),
            fields: Vec::new(),
            notes: Vec::new(),
            text_boxes: Vec::new(),
            rules: Vec::new(),
        };
        // The shaper has already made the second baseline paragraph-relative.
        let mut out = Vec::new();
        let mut cursor = Twip(500);
        stack_lines(&mut out, vec![line(80), line(180)], &mut cursor);

        assert_eq!(out[0].runs[0].origin.y, Twip(580));
        assert_eq!(
            out[1].runs[0].origin.y,
            Twip(680),
            "the preceding line height must not be added a second time"
        );
        assert_eq!(cursor, Twip(700));
    }

    #[test]
    fn a_block_sdt_flows_its_children_at_nonzero_height() {
        use casual_doc_model::v1::{BlockSdt, SdtProperties};
        // A block-level content control wrapping two paragraphs. It is a
        // transparent wrapper: layout must recurse into its children rather than
        // dropping the whole subtree at zero height (the TOC/form-control bug).
        let sdt = BlockNode::Sdt(BlockSdt {
            id: NodeId::from_parts(20, 1).unwrap(),
            properties: SdtProperties::default(),
            blocks: vec![
                paragraph(
                    21,
                    vec![run_node(22, "inside one", RunProperties::default())],
                ),
                paragraph(
                    23,
                    vec![run_node(24, "inside two", RunProperties::default())],
                ),
            ],
        });
        let shaper = ParleyShaper::new();
        let galley = build_galley(&document(vec![sdt]), &shaper, Twip::from_points(400));
        // Both child paragraphs produce fragments...
        assert_eq!(
            galley.len(),
            2,
            "the block SDT's two child paragraphs each flow to a fragment"
        );
        // ...each carrying its own NodeId (the wrapper contributes no box, so
        // hit-testing/editing still resolve to the child ids)...
        let ids: Vec<NodeId> = galley
            .iter()
            .map(|f| match f {
                BlockFragment::Paragraph { id, .. } => *id,
                BlockFragment::TableRow { .. } => panic!("expected paragraph fragments"),
            })
            .collect();
        assert_eq!(
            ids,
            vec![
                NodeId::from_parts(21, 1).unwrap(),
                NodeId::from_parts(23, 1).unwrap()
            ],
            "child NodeIds are preserved, not rewritten"
        );
        // ...at non-zero height (the fix: previously the whole SDT was dropped).
        let total: i32 = galley.iter().map(|f| f.height().raw()).sum();
        assert!(total > 0, "the SDT's children occupy real vertical space");
    }

    #[test]
    fn a_hard_line_break_splits_a_paragraph_into_two_lines() {
        use casual_doc_model::v1::{Break, BreakKind};
        let para = paragraph(
            10,
            vec![
                run_node(11, "before", RunProperties::default()),
                InlineNode::Break(Break {
                    id: NodeId::from_parts(12, 1).unwrap(),
                    kind: BreakKind::Line,
                }),
                run_node(13, "after", RunProperties::default()),
            ],
        );
        let shaper = ParleyShaper::new();
        let galley = build_galley(&document(vec![para]), &shaper, Twip::from_points(400));
        let lines = first_paragraph_lines(&galley);
        assert_eq!(lines.lines.len(), 2, "the hard break yields two lines");
        assert!(
            !lines.lines[0].page_break_after,
            "a line break is not a page break"
        );
    }

    #[test]
    fn a_page_break_threads_a_marker_to_the_paginator() {
        use casual_doc_model::v1::{Break, BreakKind};
        let para = paragraph(
            10,
            vec![
                run_node(11, "before", RunProperties::default()),
                InlineNode::Break(Break {
                    id: NodeId::from_parts(12, 1).unwrap(),
                    kind: BreakKind::Page,
                }),
                run_node(13, "after", RunProperties::default()),
            ],
        );
        let shaper = ParleyShaper::new();
        let galley = build_galley(&document(vec![para]), &shaper, Twip::from_points(400));
        let lines = first_paragraph_lines(&galley);
        assert_eq!(lines.lines.len(), 2);
        assert!(
            lines.lines[0].page_break_after,
            "the page break sets the paginator marker on the first line"
        );
    }

    #[test]
    fn a_tab_advances_to_the_paragraphs_tab_stop() {
        use casual_doc_model::v1::{Tab, TabAlignment, TabStop};
        let properties = ParagraphProperties {
            tabs: vec![TabStop {
                position_twips: 3000,
                alignment: TabAlignment::Start,
                leader: None,
            }],
            ..ParagraphProperties::default()
        };
        let para = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(10, 1).unwrap(),
            properties,
            inlines: vec![
                run_node(11, "A", RunProperties::default()),
                InlineNode::Tab(Tab {
                    id: NodeId::from_parts(12, 1).unwrap(),
                }),
                run_node(13, "B", RunProperties::default()),
            ],
        });
        let shaper = ParleyShaper::new();
        let galley = build_galley(&document(vec![para]), &shaper, Twip::from_points(400));
        let lines = first_paragraph_lines(&galley);
        assert_eq!(lines.lines.len(), 1);
        let b = lines.lines[0].runs.last().unwrap();
        assert!(
            (b.origin.x.raw() - 3000).abs() <= 20,
            "the tabbed run advances to the stop (3000), got {}",
            b.origin.x.raw()
        );
    }

    #[test]
    fn a_table_flows_to_a_row_fragment_with_positioned_cells() {
        use casual_doc_model::v1::{
            GridColumn, Table, TableCell, TableCellProperties, TableProperties, TableRow,
            TableRowProperties,
        };
        let cell = |id: u64, text: &str| TableCell {
            id: NodeId::from_parts(id, 1).unwrap(),
            properties: TableCellProperties::default(),
            blocks: vec![paragraph(
                id + 100,
                vec![run_node(id + 200, text, RunProperties::default())],
            )],
        };
        let table = BlockNode::Table(Table {
            id: NodeId::from_parts(50, 1).unwrap(),
            grid: vec![
                GridColumn {
                    width_twips: Some(3000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![TableRow {
                id: NodeId::from_parts(51, 1).unwrap(),
                properties: TableRowProperties::default(),
                cells: vec![cell(60, "left cell"), cell(61, "right cell")],
            }],
        });
        let shaper = ParleyShaper::new();
        let galley = build_galley(&document(vec![table]), &shaper, Twip::from_points(400));
        assert_eq!(galley.len(), 1, "the table flows to one row fragment");
        let BlockFragment::TableRow { cells, .. } = &galley[0] else {
            panic!("expected a table row");
        };
        assert_eq!(cells.len(), 2);
        // First cell at x=0 width 3000; second at x=3000.
        assert_eq!(cells[0].x, Twip::ZERO);
        assert_eq!(cells[0].width, Twip(3000));
        assert_eq!(cells[1].x, Twip(3000));
        // Each cell shaped its paragraph.
        assert!(!cells[0].blocks.is_empty() && !cells[1].blocks.is_empty());
    }

    #[test]
    fn builds_a_shaped_fragment_per_paragraph() {
        let doc = document(vec![
            paragraph(
                10,
                vec![run_node(11, "First paragraph.", RunProperties::default())],
            ),
            paragraph(
                20,
                vec![run_node(
                    21,
                    "Second one, a bit longer.",
                    RunProperties::default(),
                )],
            ),
        ]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        assert_eq!(galley.len(), 2, "one fragment per paragraph");
        for fragment in &galley {
            let BlockFragment::Paragraph { lines, .. } = fragment else {
                panic!("expected a paragraph fragment");
            };
            assert!(!lines.lines.is_empty(), "the paragraph shaped to lines");
            assert!(fragment.height().raw() > 0, "positive height");
        }
    }

    #[test]
    fn run_size_and_color_flow_into_the_shaped_run() {
        use casual_doc_model::v1::{Color, RgbColor};
        let props = RunProperties {
            size_half_points: Some(48), // 24pt
            color: Some(Color::Rgb(RgbColor {
                r: 200,
                g: 60,
                b: 20,
            })),
            ..RunProperties::default()
        };
        let doc = document(vec![paragraph(10, vec![run_node(11, "Big red", props)])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!();
        };
        let run = &lines.lines[0].runs[0];
        assert_eq!(run.color, [200, 60, 20, 255], "run color flows to layout");
        assert!(
            run.size.raw() >= Twip::from_points(20).raw(),
            "24pt size flows through"
        );
    }

    /// Regression: dense inline formatting must not overprint (P1F inline-overprint
    /// fix). A `parley` shaping run is split into one `GlyphRun` per contiguous
    /// brush/decoration span; the shaper must emit each `GlyphRun`'s *own* glyph
    /// slice, not the whole parent run's glyphs. Before the fix, adjacent same-face
    /// runs that differed only in brush (a highlighted run beside a recolored
    /// "inverse video" run) each re-emitted every glyph of the merged run, so their
    /// glyph clusters and x-ranges overlapped and the text drew on top of itself.
    ///
    /// The invariant asserted here is the one that was violated: on a line, the
    /// runs' glyph *cluster ranges* are pairwise disjoint (no byte is emitted by
    /// two runs), and each run's x-range starts at or after the previous run's end
    /// (allowing only `parley`'s <=1-twip per-run rounding, never a real backtrack).
    #[test]
    fn dense_inline_runs_do_not_overprint() {
        use casual_doc_model::v1::{Color, HighlightColor, RgbColor, VerticalAlignment};
        // Every run shares one face (no bold/italic split) so `parley` shapes them
        // as a single run split only by brush/decoration — the overprint trigger.
        let colored = |r, g, b| Some(Color::Rgb(RgbColor { r, g, b }));
        let inlines = vec![
            run_node(11, "Here is ", RunProperties::default()),
            run_node(
                12,
                "red",
                RunProperties {
                    color: colored(200, 0, 0),
                    ..RunProperties::default()
                },
            ),
            run_node(
                13,
                " strike",
                RunProperties {
                    strike: Some(true),
                    ..RunProperties::default()
                },
            ),
            run_node(
                14,
                "sup",
                RunProperties {
                    vertical_alignment: Some(VerticalAlignment::Superscript),
                    ..RunProperties::default()
                },
            ),
            run_node(
                15,
                "highlight",
                RunProperties {
                    highlight: Some(HighlightColor::Yellow),
                    ..RunProperties::default()
                },
            ),
            // "Inverse video": white glyphs, same face as the neighbours, so it
            // merges into the same shaping run as the highlighted span before it.
            run_node(
                16,
                "INVERSE",
                RunProperties {
                    color: colored(255, 255, 255),
                    ..RunProperties::default()
                },
            ),
            run_node(17, " tail", RunProperties::default()),
        ];
        let doc = document(vec![paragraph(10, inlines)]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(600));
        let lines = first_paragraph_lines(&galley);
        assert_eq!(lines.lines.len(), 1, "the text fits on one line");
        let runs = &lines.lines[0].runs;
        assert!(
            runs.len() >= 6,
            "the brush/decoration changes split the line into distinct runs (got {})",
            runs.len()
        );

        // 1) Cluster ranges are pairwise disjoint: no glyph byte offset is emitted
        //    by two runs. Before the fix, the highlighted and inverse runs both
        //    carried the whole merged run's clusters, so this set had duplicates.
        let mut seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for run in runs {
            for glyph in &run.glyphs {
                assert!(
                    seen.insert(glyph.cluster),
                    "cluster byte {} is emitted by two runs — runs overprint",
                    glyph.cluster
                );
            }
        }

        // 2) Runs advance in x: each run starts at or after the previous run's
        //    right edge. A tolerance of 1 twip absorbs `parley`'s independent
        //    per-run offset/advance rounding; the pre-fix bug backtracked by
        //    hundreds of twips (a whole span's width), far outside it.
        let mut prev_end = i32::MIN;
        for run in runs {
            let start = run.origin.x.raw();
            let end = start + run_advance(run).raw();
            assert!(
                start + 1 >= prev_end,
                "run at x={start} overprints the previous run ending at x={prev_end}"
            );
            prev_end = end;
        }
    }

    /// Regression: consecutive non-empty paragraphs must each have a positive
    /// height so composition stacks them at distinct, non-overlapping y ranges.
    #[test]
    fn consecutive_paragraphs_have_positive_distinct_heights() {
        let doc = document(vec![
            paragraph(
                10,
                vec![run_node(11, "First paragraph", RunProperties::default())],
            ),
            paragraph(
                20,
                vec![run_node(21, "Second paragraph", RunProperties::default())],
            ),
        ]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let heights: Vec<i32> = galley
            .iter()
            .filter_map(|f| match f {
                BlockFragment::Paragraph { lines, .. } => Some(lines.height().raw()),
                BlockFragment::TableRow { .. } => None,
            })
            .collect();
        assert_eq!(heights.len(), 2, "two paragraph fragments");
        assert!(
            heights.iter().all(|&h| h > 0),
            "each paragraph has positive height so the next stacks below it: {heights:?}"
        );
    }

    /// A run with the given size (half-points) and extra run properties.
    fn sized_run(id: u64, text: &str, half_points: u32, properties: RunProperties) -> InlineNode {
        run_node(
            id,
            text,
            RunProperties {
                size_half_points: Some(half_points),
                ..properties
            },
        )
    }

    #[test]
    fn superscript_raises_and_shrinks_the_run() {
        use casual_doc_model::v1::VerticalAlignment;
        // A baseline run and a superscript run share the line; the superscript is
        // raised (smaller screen-y, which grows downward) and shaped smaller (~2/3).
        let base = sized_run(11, "x", 24, RunProperties::default());
        let sup = sized_run(
            12,
            "2",
            24,
            RunProperties {
                vertical_alignment: Some(VerticalAlignment::Superscript),
                ..RunProperties::default()
            },
        );
        let doc = document(vec![paragraph(10, vec![base, sup])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let runs = &first_paragraph_lines(&galley).lines[0].runs;
        assert!(runs.len() >= 2, "base + superscript are distinct runs");
        let big = runs.iter().max_by_key(|r| r.size.raw()).unwrap();
        let small = runs.iter().min_by_key(|r| r.size.raw()).unwrap();
        assert!(
            small.size.raw() < big.size.raw(),
            "superscript shapes smaller ({} < {})",
            small.size.raw(),
            big.size.raw()
        );
        assert!(
            small.origin.y.raw() < big.origin.y.raw(),
            "superscript is raised above the baseline ({} < {})",
            small.origin.y.raw(),
            big.origin.y.raw()
        );
    }

    #[test]
    fn subscript_lowers_the_run() {
        use casual_doc_model::v1::VerticalAlignment;
        let base = sized_run(11, "x", 24, RunProperties::default());
        let sub = sized_run(
            12,
            "2",
            24,
            RunProperties {
                vertical_alignment: Some(VerticalAlignment::Subscript),
                ..RunProperties::default()
            },
        );
        let doc = document(vec![paragraph(10, vec![base, sub])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let runs = &first_paragraph_lines(&galley).lines[0].runs;
        let big = runs.iter().max_by_key(|r| r.size.raw()).unwrap();
        let small = runs.iter().min_by_key(|r| r.size.raw()).unwrap();
        assert!(
            small.size.raw() < big.size.raw(),
            "subscript shapes smaller"
        );
        assert!(
            small.origin.y.raw() > big.origin.y.raw(),
            "subscript is lowered below the baseline ({} > {})",
            small.origin.y.raw(),
            big.origin.y.raw()
        );
    }

    #[test]
    fn position_raises_the_baseline_without_resizing() {
        // `w:position` is a pure baseline offset — same size, only the origin moves.
        let base = sized_run(11, "x", 24, RunProperties::default());
        let raised = sized_run(
            12,
            "y",
            24,
            RunProperties {
                position_half_points: Some(6), // +6 half-points = +60 twips (raise)
                ..RunProperties::default()
            },
        );
        let doc = document(vec![paragraph(10, vec![base, raised])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let runs = &first_paragraph_lines(&galley).lines[0].runs;
        assert!(runs.len() >= 2, "base + positioned are distinct runs");
        assert!(
            runs.iter().all(|r| r.size == runs[0].size),
            "w:position must not resize the run"
        );
        let min_y = runs.iter().map(|r| r.origin.y.raw()).min().unwrap();
        let max_y = runs.iter().map(|r| r.origin.y.raw()).max().unwrap();
        assert!(
            min_y < max_y,
            "the positioned run is raised above the baseline"
        );
    }

    /// The glyph-id sequence of a galley's first paragraph (visual order).
    fn glyph_ids(galley: &[BlockFragment]) -> Vec<u32> {
        first_paragraph_lines(galley)
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .flat_map(|r| &r.glyphs)
            .map(|g| g.id)
            .collect()
    }

    #[test]
    fn all_caps_shapes_like_pre_uppercased_text() {
        let shaper = ParleyShaper::new();
        let caps = document(vec![paragraph(
            10,
            vec![run_node(
                11,
                "abc",
                RunProperties {
                    all_caps: Some(true),
                    ..RunProperties::default()
                },
            )],
        )]);
        let plain = document(vec![paragraph(
            20,
            vec![run_node(21, "ABC", RunProperties::default())],
        )]);
        let caps_glyphs = glyph_ids(&build_galley(&caps, &shaper, Twip::from_points(400)));
        let plain_glyphs = glyph_ids(&build_galley(&plain, &shaper, Twip::from_points(400)));
        assert!(!caps_glyphs.is_empty(), "the all-caps run shaped to glyphs");
        assert_eq!(
            caps_glyphs, plain_glyphs,
            "all-caps `abc` shapes to the same glyphs as literal `ABC`"
        );
    }

    #[test]
    fn small_caps_splits_lowercase_at_three_quarter_size() {
        // `aB`: the originally-lowercase `a` shapes at 3/4 size, the `B` at full
        // size, so the run splits into two spans of distinct size.
        let doc = document(vec![paragraph(
            10,
            vec![sized_run(
                11,
                "aB",
                24, // 12pt -> 240 twips; lowercase span shapes at 180 twips (3/4)
                RunProperties {
                    small_caps: Some(true),
                    ..RunProperties::default()
                },
            )],
        )]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let runs = &first_paragraph_lines(&galley).lines[0].runs;
        let sizes: std::collections::BTreeSet<i32> = runs.iter().map(|r| r.size.raw()).collect();
        assert_eq!(
            sizes,
            [180, 240].into_iter().collect(),
            "the lowercase span is 3/4 size (180) and the rest full size (240): {sizes:?}"
        );
    }

    #[test]
    fn a_theme_colored_run_resolves_to_the_scheme_slot_not_black() {
        use casual_doc_model::v1::{
            Color, ColorScheme, RgbColor, SchemeColor, ThemeColor, ThemeColorRef,
        };
        // A distinct accent-1 slot color, referenced by a run's `w:themeColor`.
        let scheme = ColorScheme {
            accent1: SchemeColor::Srgb(RgbColor {
                r: 0x11,
                g: 0x22,
                b: 0x33,
            }),
            ..ColorScheme::default()
        };
        let defs = Definitions {
            color_scheme: Some(scheme),
            ..Definitions::default()
        };
        let props = RunProperties {
            color: Some(Color::Theme(ThemeColor {
                slot: ThemeColorRef::Accent1,
            })),
            ..RunProperties::default()
        };
        let doc = Document::new(
            NodeId::from_parts(1, 1).unwrap(),
            vec![paragraph(10, vec![run_node(11, "themed", props)])],
            defs,
        )
        .unwrap();
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let run = &first_paragraph_lines(&galley).lines[0].runs[0];
        assert_eq!(
            run.color,
            [0x11, 0x22, 0x33, 255],
            "the theme slot resolves to its real RGB, flowed end to end"
        );
        assert_ne!(
            run.color,
            [0, 0, 0, 255],
            "a resolvable theme color is not black"
        );
    }

    #[test]
    fn a_syscolor_slot_resolves_to_its_last_color() {
        use casual_doc_model::v1::{
            ColorScheme, RgbColor, SchemeColor, SystemColor, ThemeColorRef,
        };
        let scheme = ColorScheme {
            dark1: SchemeColor::System(SystemColor {
                value: "windowText".to_owned(),
                last_color: Some(RgbColor { r: 5, g: 6, b: 7 }),
            }),
            ..ColorScheme::default()
        };
        let palette = resolve_palette(&scheme);
        assert_eq!(
            palette.slot(ThemeColorRef::Dark1),
            [5, 6, 7, 255],
            "a sysClr slot resolves to its recorded lastClr"
        );
    }

    #[test]
    fn tint_lightens_and_shade_darkens_a_theme_color() {
        // shade 0.5 halves each channel toward black; tint 0.5 blends halfway to
        // white (100*0.5 + 255*0.5 = 177.5 -> 178); no factors is the identity.
        assert_eq!(
            apply_tint_shade([100, 100, 100, 255], None, Some(0.5)),
            [50, 50, 50, 255]
        );
        assert_eq!(
            apply_tint_shade([100, 100, 100, 255], Some(0.5), None),
            [178, 178, 178, 255]
        );
        assert_eq!(
            apply_tint_shade([12, 34, 56, 255], None, None),
            [12, 34, 56, 255]
        );
    }

    #[test]
    fn an_auto_or_unresolvable_color_is_black() {
        use casual_doc_model::v1::{Color, ThemeColor, ThemeColorRef};
        // An unset (`auto`) color is black.
        assert_eq!(run_color(None, None), [0, 0, 0, 255]);
        // A theme color with no palette available also falls back to black.
        assert_eq!(
            run_color(
                Some(Color::Theme(ThemeColor {
                    slot: ThemeColorRef::Dark1,
                })),
                None,
            ),
            [0, 0, 0, 255]
        );
    }

    #[test]
    fn a_start_end_indent_reduces_the_wrap_width_and_carries_into_box_metrics() {
        use casual_doc_model::v1::{Indentation, Paragraph, ParagraphProperties};
        let text = "Hello world this is a longer paragraph that wraps onto lines";
        let plain = document(vec![paragraph(
            10,
            vec![run_node(11, text, RunProperties::default())],
        )]);
        let indented = document(vec![BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(10, 1).unwrap(),
            properties: ParagraphProperties {
                indentation: Some(Indentation {
                    start_twips: Some(2000),
                    end_twips: Some(2000),
                    ..Indentation::default()
                }),
                ..ParagraphProperties::default()
            },
            inlines: vec![run_node(11, text, RunProperties::default())],
        })]);
        let shaper = ParleyShaper::new();
        let width = Twip::from_points(300);
        let plain_lines = {
            let g = build_galley(&plain, &shaper, width);
            let BlockFragment::Paragraph { lines, .. } = &g[0] else {
                panic!()
            };
            lines.lines.len()
        };
        let g = build_galley(&indented, &shaper, width);
        let BlockFragment::Paragraph {
            lines, box_metrics, ..
        } = &g[0]
        else {
            panic!()
        };
        assert_eq!(
            box_metrics.indent_start,
            Twip(2000),
            "the start indent is carried"
        );
        assert_eq!(
            box_metrics.indent_end,
            Twip(2000),
            "the end indent is carried"
        );
        assert!(
            lines.lines.len() > plain_lines,
            "the 4000-twip indent narrows the column so the text wraps into more lines \
             ({} vs {plain_lines})",
            lines.lines.len()
        );
    }

    #[test]
    fn a_highlighted_run_carries_its_resolved_fill_into_the_shaped_run() {
        use casual_doc_model::v1::HighlightColor;
        let props = RunProperties {
            highlight: Some(HighlightColor::Yellow),
            ..RunProperties::default()
        };
        let doc = document(vec![paragraph(10, vec![run_node(11, "lit", props)])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!();
        };
        assert_eq!(
            lines.lines[0].runs[0].highlight,
            Some([255, 255, 0, 255]),
            "the yellow highlight resolves to RGBA and rides the shaped run"
        );
    }

    #[test]
    fn hyperlink_and_revision_text_is_collected() {
        use casual_doc_model::v1::{
            Hyperlink, HyperlinkTarget, InternalTarget, Revision, RevisionKind,
        };
        let link = InlineNode::Hyperlink(Hyperlink {
            id: NodeId::from_parts(30, 1).unwrap(),
            target: HyperlinkTarget::Internal(InternalTarget {
                anchor: "a".to_owned(),
            }),
            tooltip: None,
            inlines: vec![run_node(31, "linked", RunProperties::default())],
        });
        let rev = InlineNode::Revision(Revision {
            id: NodeId::from_parts(40, 1).unwrap(),
            kind: RevisionKind::Insertion,
            author: None,
            date: None,
            revision_id: None,
            editor_group: None,
            inlines: vec![run_node(41, " inserted", RunProperties::default())],
        });
        let doc = document(vec![paragraph(10, vec![link, rev])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!();
        };
        let glyphs: usize = lines
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .map(|r| r.glyphs.len())
            .sum();
        assert!(
            glyphs >= 12,
            "hyperlink + revision text both shaped (got {glyphs})"
        );
    }

    #[test]
    fn tracked_changes_share_one_projected_text_and_layout_byte_space() {
        use casual_doc_model::v1::{Revision, RevisionKind};

        let revision = |id, run_id, kind, text| {
            InlineNode::Revision(Revision {
                id: NodeId::from_parts(id, 1).unwrap(),
                kind,
                author: Some("Reviewer".to_owned()),
                date: None,
                revision_id: Some(id.to_string()),
                editor_group: None,
                inlines: vec![run_node(run_id, text, RunProperties::default())],
            })
        };
        let inlines = vec![
            run_node(11, "A", RunProperties::default()),
            revision(12, 13, RevisionKind::Deletion, "old"),
            revision(14, 15, RevisionKind::Insertion, "new"),
            revision(16, 17, RevisionKind::MoveFrom, "source"),
            revision(18, 19, RevisionKind::MoveTo, "target"),
            run_node(20, "Z", RunProperties::default()),
        ];
        assert_eq!(
            node_plain_text_with_projection(&inlines, ReviewProjection::Original),
            "AoldsourceZ"
        );
        assert_eq!(
            node_plain_text_with_projection(&inlines, ReviewProjection::Final),
            "AnewtargetZ"
        );
        assert_eq!(node_plain_text(&inlines), "AnewtargetZ");

        let doc = document(vec![paragraph(10, inlines)]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!();
        };
        assert_eq!(
            lines
                .lines
                .last()
                .expect("one shaped line")
                .range
                .end
                .offset,
            "AnewtargetZ".len() as u32,
            "hit-testing and editing offsets end at the projected byte length"
        );
    }

    #[test]
    fn visible_inline_leaf_floor_is_collected() {
        let note = NoteId::new(NodeId::from_parts(101, 1).unwrap());
        let mut definitions = Definitions::default();
        definitions.footnotes.insert(
            note,
            Note {
                blocks: vec![paragraph(
                    102,
                    vec![run_node(103, "note body", RunProperties::default())],
                )],
            },
        );

        let inlines = vec![
            InlineNode::Math(Math {
                id: NodeId::from_parts(10, 1).unwrap(),
                omml: "<m:oMath/>".to_owned(),
                text: "x+y".to_owned(),
                expression: None,
            }),
            InlineNode::NoBreakHyphen(NoBreakHyphen {
                id: NodeId::from_parts(11, 1).unwrap(),
            }),
            InlineNode::SoftHyphen(SoftHyphen {
                id: NodeId::from_parts(12, 1).unwrap(),
            }),
            InlineNode::NoteReference(NoteReference {
                id: NodeId::from_parts(13, 1).unwrap(),
                kind: NoteKind::Footnote,
                note,
            }),
            InlineNode::CommentReference(casual_doc_model::v1::CommentReference {
                id: NodeId::from_parts(14, 1).unwrap(),
                comment: CommentId::new(NodeId::from_parts(104, 1).unwrap()),
            }),
        ];

        let items = collected_items(&definitions, &inlines);
        assert_eq!(
            run_texts(&items),
            vec!["[x+y]", "\u{2011}", "\u{00ad}", "1"],
            "visible inline leaves are shaped while comment metadata stays zero-width"
        );
        let note_run = items.iter().find_map(|item| match item {
            FlowItem::Run(run) if run.text == "1" => Some(run),
            _ => None,
        });
        assert!(
            note_run.is_some_and(|run| run.baseline_shift > Twip::ZERO),
            "note references use a superscript marker style"
        );
        assert!(
            items
                .iter()
                .all(|item| !matches!(item, FlowItem::Run(run) if run.text.contains("comment"))),
            "comment references never emit an in-document glyph"
        );
    }

    #[test]
    fn typed_fraction_is_an_atomic_inline_box_with_a_painted_rule() {
        let math = InlineNode::Math(Math {
            id: NodeId::from_parts(12, 1).unwrap(),
            omml: "<m:oMath><m:f/></m:oMath>".to_owned(),
            text: "a/b".to_owned(),
            expression: Some(MathExpression::Fraction {
                numerator: Box::new(MathExpression::Text {
                    value: "a".to_owned(),
                }),
                denominator: Box::new(MathExpression::Text {
                    value: "b".to_owned(),
                }),
            }),
        });
        let doc = document(vec![paragraph(
            10,
            vec![
                run_node(11, "A", RunProperties::default()),
                math,
                run_node(13, "B", RunProperties::default()),
            ],
        )]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!("expected paragraph");
        };
        assert_eq!(
            lines.lines.len(),
            1,
            "inline math must not force a new line"
        );
        let line = &lines.lines[0];
        assert_eq!(line.rules.len(), 1, "fraction contributes one rule");
        assert!(
            line.runs.len() >= 4,
            "A, numerator, denominator, and B render"
        );
        let math_size = Twip::from_points(11).raw() * 85 / 100;
        assert!(
            line.runs
                .iter()
                .filter(|run| run.size.raw() == math_size)
                .all(|run| { run.glyphs.iter().all(|glyph| glyph.cluster == 1) }),
            "all equation glyphs map to the equation's atomic caret boundary"
        );

        let display = crate::compose::compose_paragraph(lines, Point::new(Twip::ZERO, Twip::ZERO));
        assert!(display.items.iter().any(|item| matches!(
            item,
            crate::display::PaintItem::Rect { rect, fill: Some(_), stroke: None }
                if rect.size.height == line.rules[0].size.height
        )));
    }

    #[test]
    fn typed_scripts_radicals_and_delimiters_build_nested_math_geometry() {
        let expression = MathExpression::Row {
            children: vec![
                MathExpression::Script {
                    base: Box::new(MathExpression::Text {
                        value: "x".to_owned(),
                    }),
                    subscript: Some(Box::new(MathExpression::Text {
                        value: "i".to_owned(),
                    })),
                    superscript: Some(Box::new(MathExpression::Text {
                        value: "2".to_owned(),
                    })),
                },
                MathExpression::Radical {
                    degree: Some(Box::new(MathExpression::Text {
                        value: "3".to_owned(),
                    })),
                    radicand: Box::new(MathExpression::Text {
                        value: "y".to_owned(),
                    }),
                },
                MathExpression::Delimiter {
                    open: "[".to_owned(),
                    close: "]".to_owned(),
                    content: Box::new(MathExpression::Text {
                        value: "z".to_owned(),
                    }),
                },
            ],
        };
        let definitions = Definitions::default();
        let inlines = [InlineNode::Math(Math {
            id: NodeId::from_parts(20, 1).unwrap(),
            omml: "<m:oMath/>".to_owned(),
            text: "xi2√y[z]".to_owned(),
            expression: Some(expression),
        })];
        let items = collected_items(&definitions, &inlines);
        let [FlowItem::Math { size, runs, rules }] = items.as_slice() else {
            panic!("supported expression should build one math box");
        };
        assert!(size.width > Twip::ZERO && size.height > Twip::ZERO);
        assert!(
            runs.len() >= 8,
            "all nested text and delimiter glyphs render"
        );
        assert_eq!(rules.len(), 1, "the radical contributes its overbar");
        assert!(
            runs.iter().any(|run| run.size < Twip::from_points(11)),
            "scripts and the radical degree use reduced deterministic sizing"
        );

        let doc = document(vec![paragraph(21, inlines.to_vec())]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!("expected math-only paragraph");
        };
        assert_eq!(lines.lines.len(), 1);
        assert_eq!(lines.lines[0].rules.len(), 1);
        assert!(!lines.lines[0].runs.is_empty());
    }

    #[test]
    fn note_reference_metadata_attaches_to_the_shaped_line() {
        let note = NoteId::new(NodeId::from_parts(201, 1).unwrap());
        let mut definitions = Definitions::default();
        definitions
            .footnotes
            .insert(note, Note { blocks: Vec::new() });
        let doc = document_with_definitions(
            vec![paragraph(
                10,
                vec![
                    run_node(11, "before", RunProperties::default()),
                    InlineNode::NoteReference(NoteReference {
                        id: NodeId::from_parts(12, 1).unwrap(),
                        kind: NoteKind::Footnote,
                        note,
                    }),
                    run_node(13, "after", RunProperties::default()),
                ],
            )],
            definitions,
        );

        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let lines = first_paragraph_lines(&galley);
        assert_eq!(
            lines.lines[0].notes,
            vec![NoteMarker {
                kind: NoteKind::Footnote,
                note
            }],
            "the visible note marker also carries pagination metadata"
        );
    }

    #[test]
    fn note_reference_metadata_flows_inside_a_table_cell() {
        use casual_doc_model::v1::{
            GridColumn, Table, TableCell, TableCellProperties, TableProperties, TableRow,
            TableRowProperties,
        };

        let note = NoteId::new(NodeId::from_parts(301, 1).unwrap());
        let mut definitions = Definitions::default();
        definitions
            .footnotes
            .insert(note, Note { blocks: Vec::new() });
        let cell = TableCell {
            id: NodeId::from_parts(20, 1).unwrap(),
            properties: TableCellProperties::default(),
            blocks: vec![paragraph(
                21,
                vec![
                    run_node(22, "cell", RunProperties::default()),
                    InlineNode::NoteReference(NoteReference {
                        id: NodeId::from_parts(23, 1).unwrap(),
                        kind: NoteKind::Footnote,
                        note,
                    }),
                ],
            )],
        };
        let table = BlockNode::Table(Table {
            id: NodeId::from_parts(30, 1).unwrap(),
            grid: vec![GridColumn {
                width_twips: Some(3000),
            }],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![TableRow {
                id: NodeId::from_parts(31, 1).unwrap(),
                properties: TableRowProperties::default(),
                cells: vec![cell],
            }],
        });
        let doc = document_with_definitions(vec![table], definitions);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::TableRow { cells, .. } = &galley[0] else {
            panic!("expected table row");
        };
        let BlockFragment::Paragraph { lines, .. } = &cells[0].blocks[0] else {
            panic!("expected flowed cell paragraph");
        };
        assert_eq!(
            lines.lines[0].notes,
            vec![NoteMarker {
                kind: NoteKind::Footnote,
                note
            }],
            "table-cell paragraphs expose note metadata through the shared flow path"
        );
    }

    #[test]
    fn embedded_object_uses_preview_image_or_visible_placeholder() {
        let media = MediaId::new(NodeId::from_parts(201, 1).unwrap());
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rIdPreview".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/preview.png".to_owned(),
            },
        );
        let part = EmbeddedPart {
            relationship_id: "rIdObject".to_owned(),
            relationship_type:
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart"
                    .to_owned(),
            part_name: "word/charts/chart1.xml".to_owned(),
        };
        let preview = InlineNode::EmbeddedObject(EmbeddedObject {
            id: NodeId::from_parts(202, 1).unwrap(),
            kind: EmbeddedKind::Chart,
            part: part.clone(),
            extra_parts: Vec::new(),
            preview: Some(media),
            extent: Extent {
                width_emu: 635_000,
                height_emu: 317_500,
            },
            prog_id: None,
        });
        let placeholder = InlineNode::EmbeddedObject(EmbeddedObject {
            id: NodeId::from_parts(203, 1).unwrap(),
            kind: EmbeddedKind::Chart,
            part,
            extra_parts: Vec::new(),
            preview: None,
            extent: Extent {
                width_emu: 635_000,
                height_emu: 317_500,
            },
            prog_id: None,
        });

        let preview_inlines = [preview];
        let preview_items = collected_items(&definitions, &preview_inlines);
        assert!(
            matches!(
                &preview_items[..],
                [FlowItem::Image { media, size, .. }]
                    if media == "word/media/preview.png"
                        && *size == Size::new(Twip(1_000), Twip(500))
            ),
            "an embedded object with a preview uses the existing image pipeline"
        );

        let placeholder_inlines = [placeholder];
        let placeholder_items = collected_items(&definitions, &placeholder_inlines);
        assert_eq!(
            run_texts(&placeholder_items),
            vec!["[chart]"],
            "an embedded object without a preview is visibly represented"
        );
    }

    #[test]
    fn a_vanished_run_is_not_collected_into_styled_runs() {
        // The shared item collector must drop a `w:vanish` run so it is never
        // measured, shaped, or painted.
        let hidden = RunProperties {
            hidden: Some(true),
            ..RunProperties::default()
        };
        let inlines = vec![
            run_node(1, "shown", RunProperties::default()),
            run_node(2, "secret", hidden),
        ];
        let definitions = Definitions::default();
        let items = collected_items(&definitions, &inlines);
        let runs: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                FlowItem::Run(run) => Some(run),
                _ => None,
            })
            .collect();
        assert_eq!(runs.len(), 1, "only the visible run is collected");
        assert_eq!(&*runs[0].text, "shown");
    }

    #[test]
    fn a_symbol_node_emits_a_mapped_glyph_run() {
        // A `w:sym` (the Medical form's Wingdings-2 checkbox, `F0A3`) must produce
        // a styled run bearing a visible box glyph — not be silently dropped as it
        // was before symbol layout existed.
        let checkbox = InlineNode::Symbol(Symbol {
            id: NodeId::from_parts(9, 1).unwrap(),
            font: "Wingdings 2".to_owned(),
            char: 0xF0A3,
            properties: RunProperties {
                size_half_points: Some(32),
                color: Some(Color::Rgb(casual_doc_model::v1::RgbColor {
                    r: 31,
                    g: 78,
                    b: 121,
                })),
                ..RunProperties::default()
            },
        });
        // An unmapped glyph in a non-bundled face still yields a visible placeholder
        // run rather than nothing.
        let unknown = InlineNode::Symbol(Symbol {
            id: NodeId::from_parts(10, 1).unwrap(),
            // A byte with no table entry (Wingdings 3 stops at 0xF0) → placeholder.
            font: "Wingdings 3".to_owned(),
            char: 0xF0FE,
            properties: RunProperties::default(),
        });
        let inlines = vec![checkbox, unknown];
        let definitions = Definitions::default();
        let items = collected_items(&definitions, &inlines);
        let runs: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                FlowItem::Run(run) => Some(run),
                _ => None,
            })
            .collect();
        assert_eq!(runs.len(), 2, "both symbols emit a glyph run");
        assert_eq!(
            runs[0].size,
            Twip::from_points(16),
            "the symbol keeps its owning run's authored size"
        );
        assert_eq!(runs[0].color, [31, 78, 121, 255]);
        let first = runs[0].text.chars().next().unwrap();
        assert!(
            matches!(first, '\u{25A1}' | '\u{2610}'),
            "checkbox maps to a box glyph, got {first:?}"
        );
        assert_eq!(
            &*runs[1].text, "\u{25A1}",
            "an unmapped symbol falls back to a visible placeholder, not nothing"
        );
    }

    #[test]
    fn a_native_sdt_checkbox_paints_its_state_glyph_not_the_cached_one() {
        use casual_doc_model::v1::{
            InlineSdt, SdtCheckbox, SdtCheckboxSymbol, SdtControlData, SdtControlKind,
            SdtProperties,
        };

        fn checkbox_sdt(
            checked: bool,
            checked_val: &str,
            font: &str,
            unchecked_val: Option<&str>,
            cached_glyph: &str,
        ) -> InlineNode {
            InlineNode::Sdt(InlineSdt {
                id: NodeId::from_parts(11, 1).unwrap(),
                properties: SdtProperties {
                    control_kind: Some(SdtControlKind::Checkbox),
                    data: Some(SdtControlData::Checkbox(SdtCheckbox {
                        checked,
                        checked_state: Some(SdtCheckboxSymbol {
                            val: checked_val.to_owned(),
                            font: Some(font.to_owned()),
                        }),
                        unchecked_state: unchecked_val.map(|v| SdtCheckboxSymbol {
                            val: v.to_owned(),
                            font: Some(font.to_owned()),
                        }),
                    })),
                    ..SdtProperties::default()
                },
                inlines: vec![InlineNode::Run(Run {
                    id: NodeId::from_parts(11, 2).unwrap(),
                    properties: RunProperties::default(),
                    text: cached_glyph.to_owned(),
                })],
            })
        }

        let definitions = Definitions::default();
        let glyph = |inline: InlineNode| -> String {
            let items = collected_items(&definitions, std::slice::from_ref(&inline));
            let runs: Vec<_> = items
                .iter()
                .filter_map(|item| match item {
                    FlowItem::Run(run) => Some(run.text.to_string()),
                    _ => None,
                })
                .collect();
            assert_eq!(runs.len(), 1, "a checkbox emits exactly one glyph run");
            runs.into_iter().next().unwrap()
        };

        // Checked → the `checkedState` glyph, even though the cached content run
        // is the unchecked box a producer left behind.
        assert_eq!(
            glyph(checkbox_sdt(
                true,
                "2612",
                "MS Gothic",
                Some("2610"),
                "\u{2610}"
            )),
            "\u{2612}",
            "a checked SDT checkbox paints its checked glyph, not the cached unchecked one"
        );
        // Unchecked → the `uncheckedState` glyph.
        assert_eq!(
            glyph(checkbox_sdt(
                false,
                "2612",
                "MS Gothic",
                Some("2610"),
                "\u{2612}"
            )),
            "\u{2610}",
            "an unchecked SDT checkbox paints its unchecked glyph"
        );
        // The same checkbox flips glyph purely on `checked`, independent of the
        // cached content run (here a stale checked box for the unchecked state).
        assert_eq!(
            glyph(checkbox_sdt(true, "2611", "MS Gothic", Some("2610"), "z")),
            "\u{2611}",
            "checked state selects checkedState even when the cached glyph is unrelated"
        );
    }

    #[test]
    fn an_sdt_checkbox_without_declared_glyphs_keeps_its_cached_content() {
        use casual_doc_model::v1::{
            InlineSdt, SdtCheckbox, SdtControlData, SdtControlKind, SdtProperties,
        };
        let definitions = Definitions::default();
        let sdt = InlineNode::Sdt(InlineSdt {
            id: NodeId::from_parts(12, 1).unwrap(),
            properties: SdtProperties {
                control_kind: Some(SdtControlKind::Checkbox),
                data: Some(SdtControlData::Checkbox(SdtCheckbox {
                    checked: true,
                    checked_state: None,
                    unchecked_state: None,
                })),
                ..SdtProperties::default()
            },
            inlines: vec![InlineNode::Run(Run {
                id: NodeId::from_parts(12, 2).unwrap(),
                properties: RunProperties::default(),
                text: "\u{2611}".to_owned(),
            })],
        });
        let items = collected_items(&definitions, std::slice::from_ref(&sdt));
        let runs: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                FlowItem::Run(run) => Some(run.text.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(
            runs,
            vec!["\u{2611}".to_string()],
            "a checkbox with no declared state glyph keeps its transparent recurse"
        );
    }

    #[test]
    fn a_style_sourced_bottom_border_reaches_the_paragraph_fragment_decor() {
        // Regression: a paragraph whose *style* (not its direct `w:pPr`) carries a
        // `w:pBdr` bottom border — a heading rule — must resolve through the cascade
        // onto the fragment's decoration so compose paints it. Direct paragraph
        // borders already worked; style-sourced ones were silently dropped when the
        // styles part was imported (a `w:pBdr` container is not a leaf property).
        use casual_doc_model::v1::{ParagraphBorders, Style, StyleKind};

        let sid = StyleId::new(NodeId::from_parts(7, 1).unwrap());
        let heading = Style {
            kind: StyleKind::Paragraph,
            is_default: false,
            name: None,
            aliases: None,
            based_on: None,
            next: None,
            link: None,
            hidden: false,
            ui_priority: None,
            semi_hidden: false,
            unhide_when_used: false,
            q_format: false,
            locked: false,
            paragraph: Some(ParagraphProperties {
                borders: ParagraphBorders {
                    bottom: Some(BorderEdge {
                        style: "single".to_owned(),
                        size_eighth_points: Some(8),
                        color: None,
                        space_points: Some(4),
                    }),
                    ..ParagraphBorders::default()
                },
                ..ParagraphProperties::default()
            }),
            run: None,
            table: None,
            table_row: None,
            table_cell: None,
            conditional: Vec::new(),
        };
        let mut definitions = Definitions::default();
        definitions.styles.insert(sid, heading);

        let para = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(30, 1).unwrap(),
            properties: ParagraphProperties {
                style_ref: Some(sid),
                ..ParagraphProperties::default()
            },
            inlines: vec![run_node(31, "Heading", RunProperties::default())],
        });
        let document =
            Document::new(NodeId::from_parts(1, 1).unwrap(), vec![para], definitions).unwrap();

        let shaper = ParleyShaper::new();
        let galley = build_galley(&document, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { decor, .. } = &galley[0] else {
            panic!("expected a paragraph fragment");
        };
        let bottom = decor
            .borders
            .bottom
            .expect("style-sourced bottom border reaches the fragment decor");
        // `w:sz` is in eighths of a point (20 twips/pt): 8/8 pt -> 20 twips.
        assert_eq!(bottom.width, Twip(20));
        assert!(
            decor.borders.top.is_none() && decor.borders.start.is_none(),
            "only the declared edge is present"
        );
    }

    #[test]
    fn a_vanished_run_produces_no_glyphs() {
        // End to end: a paragraph of one visible + one vanished run shapes to the
        // same glyphs as the visible run alone — the hidden text paints nothing.
        let shaper = ParleyShaper::new();

        let visible_only = document(vec![paragraph(
            10,
            vec![run_node(11, "shown", RunProperties::default())],
        )]);
        let g_visible = build_galley(&visible_only, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph {
            lines: visible_lines,
            ..
        } = &g_visible[0]
        else {
            panic!("expected a paragraph fragment");
        };
        let visible_glyphs: usize = visible_lines
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .map(|r| r.glyphs.len())
            .sum();

        let with_hidden = document(vec![paragraph(
            20,
            vec![
                run_node(21, "shown", RunProperties::default()),
                run_node(
                    22,
                    "invisible",
                    RunProperties {
                        hidden: Some(true),
                        ..RunProperties::default()
                    },
                ),
            ],
        )]);
        let g_hidden = build_galley(&with_hidden, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph {
            lines: hidden_lines,
            ..
        } = &g_hidden[0]
        else {
            panic!("expected a paragraph fragment");
        };
        let hidden_glyphs: usize = hidden_lines
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .map(|r| r.glyphs.len())
            .sum();

        assert!(visible_glyphs > 0, "the visible run shaped to glyphs");
        assert_eq!(
            hidden_glyphs, visible_glyphs,
            "the w:vanish run added no glyphs"
        );
    }

    #[test]
    fn inherited_vanish_is_suppressed_but_direct_false_restores_visibility() {
        use casual_doc_model::v1::DocumentDefaults;

        let definitions = Definitions {
            document_defaults: Some(DocumentDefaults {
                paragraph: None,
                run: Some(RunProperties {
                    hidden: Some(true),
                    ..RunProperties::default()
                }),
            }),
            ..Definitions::default()
        };
        let inherited_hidden = Document::new(
            NodeId::from_parts(1, 1).unwrap(),
            vec![paragraph(
                10,
                vec![run_node(11, "inherited secret", RunProperties::default())],
            )],
            definitions.clone(),
        )
        .unwrap();
        let direct_visible = Document::new(
            NodeId::from_parts(2, 1).unwrap(),
            vec![paragraph(
                20,
                vec![run_node(
                    21,
                    "shown",
                    RunProperties {
                        hidden: Some(false),
                        ..RunProperties::default()
                    },
                )],
            )],
            definitions,
        )
        .unwrap();
        let shaper = ParleyShaper::new();

        let glyph_count = |document: &Document| {
            build_galley(document, &shaper, Twip::from_points(400))
                .iter()
                .filter_map(|fragment| match fragment {
                    BlockFragment::Paragraph { lines, .. } => Some(lines),
                    BlockFragment::TableRow { .. } => None,
                })
                .flat_map(|lines| &lines.lines)
                .flat_map(|line| &line.runs)
                .map(|run| run.glyphs.len())
                .sum::<usize>()
        };

        assert_eq!(
            glyph_count(&inherited_hidden),
            0,
            "docDefaults w:vanish must suppress an otherwise unformatted run"
        );
        assert!(
            glyph_count(&direct_visible) > 0,
            "direct w:vanish=false must override the inherited hidden value"
        );
    }

    fn named_font(name: &str) -> RunProperties {
        use casual_doc_model::v1::{FontName, FontRef};
        RunProperties {
            font_ref: Some(FontRef::Named(FontName {
                name: name.to_owned(),
            })),
            ..RunProperties::default()
        }
    }

    // --- Table layout fidelity (P1D-003) --------------------------------------

    use casual_doc_model::v1::{
        BorderEdge, CellMargins, GridColumn, HeightRule, RgbColor, RowHeight, Shading, Table,
        TableBorders, TableCell, TableCellProperties, TableProperties, TableRow as ModelRow,
        TableRowProperties,
    };

    fn node(id: u64) -> NodeId {
        NodeId::from_parts(id, 1).unwrap()
    }

    fn text_cell(id: u64, props: TableCellProperties, text: &str) -> TableCell {
        TableCell {
            id: node(id),
            properties: props,
            blocks: vec![paragraph(
                id + 1000,
                vec![run_node(id + 2000, text, RunProperties::default())],
            )],
        }
    }

    fn edge(style: &str, sz: u32) -> BorderEdge {
        BorderEdge {
            style: style.to_owned(),
            size_eighth_points: Some(sz),
            color: None,
            space_points: None,
        }
    }

    fn colored_edge(style: &str, sz: u32, color: RgbColor) -> BorderEdge {
        BorderEdge {
            color: Some(color),
            ..edge(style, sz)
        }
    }

    /// Builds a single-row table galley and returns the row fragment's cells.
    fn flow_single_row(table: Table, width: Twip) -> BlockFragment {
        let shaper = ParleyShaper::new();
        let mut galley = build_galley(&document(vec![BlockNode::Table(table)]), &shaper, width);
        galley.remove(0)
    }

    fn flow_table_rows(table: Table, width: Twip) -> Vec<BlockFragment> {
        let shaper = ParleyShaper::new();
        build_galley(&document(vec![BlockNode::Table(table)]), &shaper, width)
    }

    // --- column-width solver (pure) ---

    #[test]
    fn solver_honors_a_preferred_table_width_and_distributes_the_extra() {
        let cols = [
            ColumnConstraint {
                grid: Some(2000),
                min: 100,
                preferred: 2000,
            },
            ColumnConstraint {
                grid: Some(2000),
                min: 100,
                preferred: 2000,
            },
        ];
        let w = solve_column_widths(&cols, WidthSpec::Dxa(8000), 10_000, TableLayout::Autofit);
        assert_eq!(
            w.iter().sum::<i32>(),
            8000,
            "columns sum to the preferred width"
        );
        assert_eq!(w, vec![4000, 4000], "the extra space is shared evenly");
    }

    #[test]
    fn solver_resolves_a_percentage_width_against_the_available_width() {
        let cols = [
            ColumnConstraint {
                grid: None,
                min: 1,
                preferred: 3000,
            },
            ColumnConstraint {
                grid: None,
                min: 1,
                preferred: 3000,
            },
        ];
        // 100% (5000 fiftieths) of a 10_000-twip content width.
        let w = solve_column_widths(&cols, WidthSpec::Pct(5000), 10_000, TableLayout::Autofit);
        assert_eq!(w.iter().sum::<i32>(), 10_000, "the table fills the width");
        // 50% resolves to half.
        let half = solve_column_widths(&cols, WidthSpec::Pct(2500), 10_000, TableLayout::Autofit);
        assert_eq!(half.iter().sum::<i32>(), 5000);
    }

    #[test]
    fn solver_keeps_a_column_at_its_minimum_content_width() {
        // Target narrower than the preferred sum: the deficit is taken from slack,
        // never shrinking a column below its content minimum.
        let cols = [
            ColumnConstraint {
                grid: None,
                min: 2500,
                preferred: 3000,
            },
            ColumnConstraint {
                grid: None,
                min: 200,
                preferred: 3000,
            },
        ];
        let w = solve_column_widths(&cols, WidthSpec::Dxa(4000), 10_000, TableLayout::Autofit);
        assert_eq!(w.iter().sum::<i32>(), 4000);
        assert!(
            w[0] >= 2500,
            "the narrow-min column keeps its minimum: {w:?}"
        );
    }

    #[test]
    fn solver_fixed_layout_uses_the_grid_verbatim() {
        let cols = [
            ColumnConstraint {
                grid: Some(3000),
                min: 100,
                preferred: 3000,
            },
            ColumnConstraint {
                grid: Some(5000),
                min: 100,
                preferred: 5000,
            },
        ];
        let w = solve_column_widths(&cols, WidthSpec::Auto, 12_000, TableLayout::Fixed);
        assert_eq!(w, vec![3000, 5000], "fixed layout keeps the declared grid");
    }

    // --- flow integration ---

    fn aligned_one_cell_table(alignment: Alignment, bidi_visual: bool) -> Table {
        Table {
            id: node(45),
            grid: vec![GridColumn {
                width_twips: Some(3000),
            }],
            grid_change: None,
            properties: TableProperties {
                tbl_bidi_visual: bidi_visual,
                alignment: Some(alignment),
                width_twips: Some(3000),
                layout: Some(TableLayout::Fixed),
                indent_twips: Some(600),
                ..TableProperties::default()
            },
            rows: vec![ModelRow {
                id: node(46),
                properties: TableRowProperties::default(),
                cells: vec![text_cell(47, TableCellProperties::default(), "cell")],
            }],
        }
    }

    #[test]
    fn table_alignment_maps_logical_edges_for_ltr_and_bidi_visual() {
        let x = |alignment, bidi_visual| {
            let BlockFragment::TableRow { cells, .. } =
                flow_single_row(aligned_one_cell_table(alignment, bidi_visual), Twip(9000))
            else {
                panic!("expected a table row");
            };
            cells[0].x
        };

        assert_eq!(x(Alignment::Start, false), Twip(600));
        assert_eq!(x(Alignment::Center, false), Twip(3000));
        assert_eq!(x(Alignment::End, false), Twip(6000));
        assert_eq!(x(Alignment::Start, true), Twip(5400));
        assert_eq!(x(Alignment::Center, true), Twip(3000));
        assert_eq!(x(Alignment::End, true), Twip(0));
    }

    #[test]
    fn row_alignment_overrides_the_table_alignment_without_changing_its_grid() {
        let mut table = aligned_one_cell_table(Alignment::End, false);
        table.rows.push(ModelRow {
            id: node(48),
            properties: TableRowProperties {
                alignment: Some(Alignment::Center),
                ..TableRowProperties::default()
            },
            cells: vec![text_cell(49, TableCellProperties::default(), "override")],
        });

        let rows = flow_table_rows(table, Twip(9000));
        let positions: Vec<(Twip, Twip)> = rows
            .iter()
            .map(|row| match row {
                BlockFragment::TableRow { cells, .. } => (cells[0].x, cells[0].width),
                BlockFragment::Paragraph { .. } => panic!("expected table rows"),
            })
            .collect();
        assert_eq!(
            positions,
            vec![(Twip(6000), Twip(3000)), (Twip(3000), Twip(3000))]
        );
    }

    fn two_column_spacing_table(spacing: i32, row_override: Option<i32>) -> Table {
        Table {
            id: node(50),
            grid: vec![
                GridColumn {
                    width_twips: Some(2000),
                },
                GridColumn {
                    width_twips: Some(4000),
                },
            ],
            grid_change: None,
            properties: TableProperties {
                width_twips: Some(6000),
                layout: Some(TableLayout::Fixed),
                cell_spacing_twips: Some(spacing),
                ..TableProperties::default()
            },
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties {
                    cell_spacing_twips: row_override,
                    ..TableRowProperties::default()
                },
                cells: vec![
                    text_cell(52, TableCellProperties::default(), "A"),
                    text_cell(53, TableCellProperties::default(), "B"),
                ],
            }],
        }
    }

    #[test]
    fn cell_spacing_is_carved_inside_fixed_grid_tracks_with_row_precedence() {
        let geometry = |table: Table| {
            let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
                panic!("expected a table row");
            };
            cells
                .iter()
                .map(|cell| (cell.x, cell.width, cell.cell_spacing))
                .collect::<Vec<_>>()
        };

        let table_spacing = geometry(two_column_spacing_table(240, None));
        assert_eq!(table_spacing[0].0, Twip(120));
        assert_eq!(table_spacing[0].1, Twip(1760));
        assert_eq!(table_spacing[1].0, Twip(2120));
        assert_eq!(table_spacing[1].1, Twip(3760));
        assert_eq!(
            table_spacing[1].0 - (table_spacing[0].0 + table_spacing[0].1),
            Twip(240)
        );
        assert_eq!(
            table_spacing[1].0 + table_spacing[1].1 + table_spacing[1].2.end,
            Twip(6000),
            "spacing does not enlarge the solved table"
        );

        let row_spacing = geometry(two_column_spacing_table(240, Some(480)));
        assert_eq!(row_spacing[0].0, Twip(240));
        assert_eq!(row_spacing[0].1, Twip(1520));
        assert_eq!(row_spacing[1].0, Twip(2240));
        assert_eq!(row_spacing[1].1, Twip(3520));
    }

    #[test]
    fn cell_spacing_contributes_to_row_height_but_not_the_cell_content_box() {
        let BlockFragment::TableRow {
            cells: plain,
            height: plain_height,
            ..
        } = flow_single_row(two_column_spacing_table(0, None), Twip(9000))
        else {
            panic!("expected a table row");
        };
        let BlockFragment::TableRow {
            cells: spaced,
            height: spaced_height,
            ..
        } = flow_single_row(two_column_spacing_table(240, None), Twip(9000))
        else {
            panic!("expected a table row");
        };

        assert_eq!(spaced_height - plain_height, Twip(240));
        assert_eq!(
            spaced[0].box_height(spaced_height),
            plain[0].box_height(plain_height)
        );
        assert_eq!(spaced[0].cell_spacing.top, Twip(120));
        assert_eq!(spaced[0].cell_spacing.bottom, Twip(120));
    }

    #[test]
    fn exact_row_clamps_excessive_vertical_spacing_inside_its_box() {
        let mut table = two_column_spacing_table(240, None);
        table.rows[0].properties.height = RowHeight {
            value_twips: Some(100),
            rule: Some(HeightRule::Exact),
        };
        let BlockFragment::TableRow {
            cells,
            height,
            clip,
            ..
        } = flow_single_row(table, Twip(9000))
        else {
            panic!("expected a table row");
        };
        assert_eq!(height, Twip(100));
        assert!(clip);
        assert_eq!(cells[0].box_height(height), Twip(1));
        assert_eq!(
            cells[0].cell_spacing.top + cells[0].box_height(height) + cells[0].cell_spacing.bottom,
            height
        );
    }

    #[test]
    fn separated_cells_keep_abutting_and_outer_table_borders_distinct() {
        let red = RgbColor { r: 255, g: 0, b: 0 };
        let blue = RgbColor { r: 0, g: 0, b: 255 };
        let green = RgbColor { r: 0, g: 128, b: 0 };
        let mut table = two_column_spacing_table(240, None);
        table.properties.borders = TableBorders {
            start: Some(colored_edge("single", 24, green)),
            end: Some(colored_edge("single", 24, green)),
            inside_v: Some(colored_edge("single", 8, green)),
            ..TableBorders::default()
        };
        table.rows[0].cells[0].properties.borders.end = Some(colored_edge("single", 16, red));
        table.rows[0].cells[1].properties.borders.start = Some(colored_edge("double", 24, blue));

        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a table row");
        };
        assert_eq!(
            cells[0].borders.end.map(|edge| edge.color),
            Some([255, 0, 0, 255])
        );
        assert_eq!(
            cells[1].borders.start.map(|edge| edge.color),
            Some([0, 0, 255, 255])
        );
        assert_eq!(
            cells[0].table_borders.start.map(|edge| edge.color),
            Some([0, 128, 0, 255])
        );
        assert_eq!(
            cells[1].table_borders.end.map(|edge| edge.color),
            Some([0, 128, 0, 255])
        );
    }

    #[test]
    fn bidi_visual_reflects_cell_spacing_around_a_grid_span() {
        let mut table = two_column_spacing_table(241, None);
        table.properties.tbl_bidi_visual = true;
        table.properties.alignment = Some(Alignment::End);
        table.grid = vec![
            GridColumn {
                width_twips: Some(1000),
            },
            GridColumn {
                width_twips: Some(2000),
            },
            GridColumn {
                width_twips: Some(3000),
            },
        ];
        table.rows[0].cells[0].properties.grid_span = Some(2);

        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a table row");
        };
        assert_eq!((cells[0].x, cells[0].width), (Twip(3121), Twip(2759)));
        assert_eq!((cells[1].x, cells[1].width), (Twip(121), Twip(2759)));
        assert_eq!(cells[0].x - (cells[1].x + cells[1].width), Twip(241));
        assert_eq!(cells[0].cell_spacing.start, Twip(121));
        assert_eq!(cells[0].cell_spacing.end, Twip(120));
    }

    #[test]
    fn excessive_cell_spacing_keeps_a_bounded_cell_box() {
        let BlockFragment::TableRow { cells, .. } =
            flow_single_row(two_column_spacing_table(10_000, None), Twip(9000))
        else {
            panic!("expected a table row");
        };
        assert_eq!(cells[0].width, Twip(1));
        assert_eq!(cells[1].width, Twip(1));
        assert!(cells[0].x.raw() >= 0);
        assert!(cells[1].x.raw() + cells[1].width.raw() <= 6000);
    }

    #[test]
    fn bidi_visual_mirrors_unequal_grid_ranges_margins_and_vertical_borders() {
        let leading = RgbColor { r: 255, g: 0, b: 0 };
        let trailing = RgbColor { r: 0, g: 0, b: 255 };
        let first = TableCellProperties {
            margins: CellMargins {
                start_twips: Some(111),
                end_twips: Some(222),
                ..CellMargins::default()
            },
            borders: TableBorders {
                start: Some(colored_edge("single", 8, leading)),
                end: Some(colored_edge("double", 24, trailing)),
                ..TableBorders::default()
            },
            ..TableCellProperties::default()
        };
        let table = Table {
            id: node(70),
            grid: vec![
                GridColumn {
                    width_twips: Some(1000),
                },
                GridColumn {
                    width_twips: Some(2000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            grid_change: None,
            properties: TableProperties {
                tbl_bidi_visual: true,
                alignment: Some(Alignment::End),
                layout: Some(TableLayout::Fixed),
                ..TableProperties::default()
            },
            rows: vec![ModelRow {
                id: node(71),
                properties: TableRowProperties::default(),
                cells: vec![
                    text_cell(72, first, "logical first"),
                    text_cell(73, TableCellProperties::default(), "middle"),
                    text_cell(74, TableCellProperties::default(), "logical last"),
                ],
            }],
        };

        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(8000)) else {
            panic!("expected a table row");
        };
        assert_eq!(
            cells
                .iter()
                .map(|cell| (cell.x, cell.width))
                .collect::<Vec<_>>(),
            vec![
                (Twip(5000), Twip(1000)),
                (Twip(3000), Twip(2000)),
                (Twip(0), Twip(3000)),
            ]
        );
        assert_eq!(
            (cells[0].margins.start, cells[0].margins.end),
            (Twip(222), Twip(111))
        );
        assert_eq!(
            cells[0].borders.end.map(|edge| edge.color),
            Some([255, 0, 0, 255])
        );
        assert_eq!(
            cells[0].borders.start.map(|edge| edge.color),
            Some([0, 0, 255, 255])
        );
    }

    #[test]
    fn bidi_visual_reflects_a_grid_span_as_one_physical_box() {
        let table = Table {
            id: node(80),
            grid: vec![
                GridColumn {
                    width_twips: Some(1000),
                },
                GridColumn {
                    width_twips: Some(2000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            grid_change: None,
            properties: TableProperties {
                tbl_bidi_visual: true,
                alignment: Some(Alignment::End),
                layout: Some(TableLayout::Fixed),
                ..TableProperties::default()
            },
            rows: vec![ModelRow {
                id: node(81),
                properties: TableRowProperties::default(),
                cells: vec![
                    text_cell(
                        82,
                        TableCellProperties {
                            grid_span: Some(2),
                            ..TableCellProperties::default()
                        },
                        "logical columns one and two",
                    ),
                    text_cell(83, TableCellProperties::default(), "logical column three"),
                ],
            }],
        };

        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(8000)) else {
            panic!("expected a table row");
        };
        assert_eq!(
            cells
                .iter()
                .map(|cell| (cell.x, cell.width))
                .collect::<Vec<_>>(),
            vec![(Twip(3000), Twip(3000)), (Twip(0), Twip(3000))]
        );
    }

    #[test]
    fn bidi_visual_reflects_segmented_horizontal_borders() {
        let thin = ResolvedEdge {
            color: [255, 0, 0, 255],
            width: Twip(10),
            pattern: BorderPattern::Solid,
        };
        let thick = ResolvedEdge {
            color: [0, 0, 255, 255],
            width: Twip(30),
            pattern: BorderPattern::Double,
        };
        let mut margins = CellContentMargins {
            start: Twip(100),
            end: Twip(200),
            ..CellContentMargins::default()
        };
        let mut borders = CellBorders {
            start: Some(thin),
            end: Some(thick),
            top_segments: vec![
                ResolvedBorderSegment {
                    offset: Twip(100),
                    length: Twip(200),
                    edge: thin,
                },
                ResolvedBorderSegment {
                    offset: Twip(400),
                    length: Twip(300),
                    edge: thick,
                },
            ],
            ..CellBorders::default()
        };
        let mut table_borders = CellBorders {
            start: Some(thin),
            end: Some(thick),
            ..CellBorders::default()
        };

        mirror_cell_geometry(&mut margins, &mut borders, &mut table_borders, Twip(1000));

        assert_eq!((margins.start, margins.end), (Twip(200), Twip(100)));
        assert_eq!((borders.start, borders.end), (Some(thick), Some(thin)));
        assert_eq!(
            (table_borders.start, table_borders.end),
            (Some(thick), Some(thin))
        );
        assert_eq!(
            borders.top_segments,
            vec![
                ResolvedBorderSegment {
                    offset: Twip(300),
                    length: Twip(300),
                    edge: thick,
                },
                ResolvedBorderSegment {
                    offset: Twip(700),
                    length: Twip(200),
                    edge: thin,
                },
            ]
        );
    }

    #[test]
    fn preferred_cell_width_grows_its_column() {
        // No grid (autofit derives the column count); cell 0 asks for a wide `w:tcW`.
        let table = Table {
            id: node(50),
            grid: Vec::new(),
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![
                    text_cell(
                        60,
                        TableCellProperties {
                            width_twips: Some(4000),
                            ..TableCellProperties::default()
                        },
                        "a",
                    ),
                    text_cell(61, TableCellProperties::default(), "b"),
                ],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        assert!(
            cells[0].width.raw() >= 3500,
            "the preferred tcW width is honored: {}",
            cells[0].width.raw()
        );
        assert!(
            cells[0].width.raw() > cells[1].width.raw(),
            "the wide-preferred column is wider than its neighbor"
        );
    }

    #[test]
    fn a_narrow_column_grows_to_fit_its_content() {
        // A tiny declared grid column whose cell holds a long unbreakable word:
        // autofit must grow the column to at least that word's width.
        let long_word = "Supercalifragilisticexpialidocious";
        let table = Table {
            id: node(50),
            grid: vec![
                GridColumn {
                    width_twips: Some(200),
                },
                GridColumn {
                    width_twips: Some(4000),
                },
            ],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![
                    text_cell(60, TableCellProperties::default(), long_word),
                    text_cell(61, TableCellProperties::default(), "x"),
                ],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        assert!(
            cells[0].width.raw() > 200,
            "the narrow column grew past its tiny declared width: {}",
            cells[0].width.raw()
        );
    }

    #[test]
    fn modeled_inline_boxes_contribute_to_cell_intrinsic_widths() {
        use casual_doc_model::v1::{
            Drawing, Field, InlineSdt, SdtProperties, TextBox, TextBoxBodyProperties, TextBoxInsets,
        };

        fn measure(definitions: &Definitions, inlines: Vec<InlineNode>) -> (i32, i32) {
            let resolver = FontResolver::new();
            let mut report = FontResolutionReport::new();
            let ctx = FlowCtx {
                resolver: &resolver,
                scheme: definitions.font_scheme.as_ref(),
                report: &mut report,
                default_tab: crate::tabs::DEFAULT_TAB_STOP,
                media: &definitions.media,
                palette: None,
                cascade: StyleCascade::new(definitions),
                para_style: None,
                table_style: None,
                sections: &definitions.sections,
                definitions,
                numbering: NumberingState::new(),
                text_scale: 100_000,
                line_spacing_reduction: 0,
                paragraph_float_exclusions: None,
            };
            block_intrinsic(&[paragraph(700, inlines)], &ParleyShaper::new(), &ctx, None)
        }

        let media = MediaId::new(node(701));
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rIdImage".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/intrinsic.png".to_owned(),
            },
        );
        let picture = InlineNode::Sdt(InlineSdt {
            id: node(714),
            properties: SdtProperties::default(),
            inlines: vec![InlineNode::Drawing(Drawing {
                id: node(702),
                media,
                extent: Some(Extent {
                    width_emu: 1_905_000,
                    height_emu: 635_000,
                }),
                descr: None,
                crop: None,
            })],
        });
        let preview = InlineNode::EmbeddedObject(EmbeddedObject {
            id: node(703),
            kind: EmbeddedKind::Chart,
            part: EmbeddedPart {
                relationship_id: "rIdChart".to_owned(),
                relationship_type: "chart".to_owned(),
                part_name: "word/charts/chart1.xml".to_owned(),
            },
            extra_parts: Vec::new(),
            preview: Some(media),
            extent: Extent {
                width_emu: 1_587_500,
                height_emu: 635_000,
            },
            prog_id: None,
        });
        let math = InlineNode::Math(Math {
            id: node(704),
            omml: "<m:oMath/>".to_owned(),
            text: "fraction".to_owned(),
            expression: Some(MathExpression::Fraction {
                numerator: Box::new(MathExpression::Text {
                    value: "numerator".to_owned(),
                }),
                denominator: Box::new(MathExpression::Text {
                    value: "denominator".to_owned(),
                }),
            }),
        });
        let field = InlineNode::Field(Field {
            id: node(705),
            instruction: "DOCPROPERTY Title".to_owned(),
            inlines: vec![run_node(
                706,
                "cached-field-result",
                RunProperties::default(),
            )],
            form: None,
        });
        let authored_box = InlineNode::TextBox(TextBox {
            id: node(707),
            anchor: None,
            relative_height: None,
            extent: Some(Extent {
                width_emu: 1_397_000,
                height_emu: 635_000,
            }),
            fill: None,
            border: None,
            body_properties: TextBoxBodyProperties::default(),
            blocks: vec![paragraph(
                708,
                vec![run_node(709, "boxed", RunProperties::default())],
            )],
        });
        let widthless_box = InlineNode::TextBox(TextBox {
            id: node(710),
            anchor: None,
            relative_height: None,
            extent: None,
            fill: None,
            border: None,
            body_properties: TextBoxBodyProperties {
                insets: TextBoxInsets {
                    left_emu: 63_500,
                    right_emu: 63_500,
                    top_emu: 0,
                    bottom_emu: 0,
                },
                ..TextBoxBodyProperties::default()
            },
            blocks: vec![paragraph(
                711,
                vec![run_node(
                    712,
                    "widthless-box-content",
                    RunProperties::default(),
                )],
            )],
        });

        assert!(measure(&definitions, vec![picture]).0 >= 3_000);
        assert!(measure(&definitions, vec![preview]).0 >= 2_500);
        let math_width = measure(&definitions, vec![math]);
        assert!(math_width.0 > 1 && math_width.1 >= math_width.0);
        let field_width = measure(&definitions, vec![field]);
        assert!(field_width.0 > 1 && field_width.1 >= field_width.0);
        let authored_width = measure(&definitions, vec![authored_box]);
        assert!(authored_width.0 >= 2_200 && authored_width.1 >= 2_200);
        let inner_width = measure(
            &definitions,
            vec![run_node(
                713,
                "widthless-box-content",
                RunProperties::default(),
            )],
        );
        let widthless_width = measure(&definitions, vec![widthless_box]);
        assert!(widthless_width.0 >= inner_width.0.saturating_add(200));
        assert!(widthless_width.1 >= inner_width.1.saturating_add(200));
    }

    #[test]
    fn inline_picture_intrinsic_width_grows_an_autofit_table_column() {
        use casual_doc_model::v1::Drawing;

        let media = MediaId::new(node(720));
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rIdImage".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/autofit.png".to_owned(),
            },
        );
        let object_cell = TableCell {
            id: node(721),
            properties: TableCellProperties::default(),
            blocks: vec![paragraph(
                722,
                vec![InlineNode::Drawing(Drawing {
                    id: node(723),
                    media,
                    extent: Some(Extent {
                        width_emu: 1_905_000,
                        height_emu: 635_000,
                    }),
                    descr: None,
                    crop: None,
                })],
            )],
        };
        let table = Table {
            id: node(724),
            grid: vec![
                GridColumn {
                    width_twips: Some(200),
                },
                GridColumn {
                    width_twips: Some(200),
                },
            ],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![ModelRow {
                id: node(725),
                properties: TableRowProperties::default(),
                cells: vec![
                    object_cell,
                    text_cell(726, TableCellProperties::default(), "x"),
                ],
            }],
        };
        let doc = document_with_definitions(vec![BlockNode::Table(table)], definitions);
        let galley = build_galley(&doc, &ParleyShaper::new(), Twip(6_000));
        let BlockFragment::TableRow { cells, .. } = &galley[0] else {
            panic!("expected a table row");
        };
        assert!(
            cells[0].width.raw() >= 3_000,
            "the 3000-twip picture must participate in auto-fit: {:?}",
            cells
                .iter()
                .map(|cell| cell.width.raw())
                .collect::<Vec<_>>()
        );
        assert!(cells[0].width > cells[1].width);
    }

    #[test]
    fn grid_span_spans_the_covered_columns() {
        let table = Table {
            id: node(50),
            grid: vec![
                GridColumn {
                    width_twips: Some(3000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![text_cell(
                    60,
                    TableCellProperties {
                        grid_span: Some(2),
                        ..TableCellProperties::default()
                    },
                    "spanning header",
                )],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].grid_span, 2);
        assert_eq!(cells[0].x, Twip::ZERO);
        assert_eq!(cells[0].width, Twip(6000), "the cell spans both columns");
    }

    #[test]
    fn table_indent_offsets_the_cells() {
        let table = Table {
            id: node(50),
            grid: vec![
                GridColumn {
                    width_twips: Some(3000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            grid_change: None,
            properties: TableProperties {
                indent_twips: Some(1000),
                ..TableProperties::default()
            },
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![
                    text_cell(60, TableCellProperties::default(), "a"),
                    text_cell(61, TableCellProperties::default(), "b"),
                ],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        assert_eq!(cells[0].x, Twip(1000), "w:tblInd offsets the first cell");
        assert_eq!(cells[1].x, Twip(4000), "the second cell follows the indent");
    }

    fn one_cell_table(props: TableRowProperties, text: &str) -> Table {
        Table {
            id: node(50),
            grid: vec![GridColumn {
                width_twips: Some(3000),
            }],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![ModelRow {
                id: node(51),
                properties: props,
                cells: vec![text_cell(60, TableCellProperties::default(), text)],
            }],
        }
    }

    #[test]
    fn a_cambria_run_resolves_to_the_caladea_face_and_is_reported() {
        use crate::resolve::Disposition;
        let doc = document(vec![paragraph(
            10,
            vec![run_node(11, "Body text", named_font("Cambria"))],
        )]);
        let shaper = ParleyShaper::new();
        let (galley, report) = build_galley_with_report(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!();
        };
        // The resolver's metric-compatible substitution (Cambria -> Caladea) is a
        // pure function of the bundled face set, so it is always recorded.
        let subs: Vec<_> = report.substitutions().collect();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].resolved_family, "Caladea");
        assert_eq!(subs[0].disposition, Disposition::MetricCompatible);
        // When no real Cambria face is available to the shaper — every deterministic
        // / WASM build, and any platform without the font — the run is *shaped* with
        // the bundled Caladea substitute. With `system-fonts` on a machine that has
        // Cambria (e.g. Windows CI), `pick_family` prefers the real installed face,
        // so the shaped FontId is intentionally the interned Cambria, not Caladea.
        #[cfg(not(feature = "system-fonts"))]
        assert_eq!(
            lines.lines[0].runs[0].font,
            crate::fonts::CALADEA.face_id(false, false),
            "Cambria shapes and renders as the Caladea face"
        );
        let _ = lines;
    }

    #[test]
    fn an_unknown_family_is_reported_as_a_fallback() {
        use crate::resolve::Disposition;
        let doc = document(vec![paragraph(
            10,
            vec![run_node(11, "text", named_font("No Such Font"))],
        )]);
        let shaper = ParleyShaper::new();
        let (_galley, report) = build_galley_with_report(&doc, &shaper, Twip::from_points(400));
        let subs: Vec<_> = report.substitutions().collect();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].requested, "No Such Font");
        assert_eq!(subs[0].disposition, Disposition::Fallback);
    }

    #[test]
    fn a_run_without_a_declared_family_reports_nothing() {
        let doc = document(vec![paragraph(
            10,
            vec![run_node(11, "plain", RunProperties::default())],
        )]);
        let shaper = ParleyShaper::new();
        let (_galley, report) = build_galley_with_report(&doc, &shaper, Twip::from_points(400));
        assert!(
            report.is_empty(),
            "an undeclared family is not a substitution"
        );
    }

    #[test]
    fn at_least_row_height_grows_to_the_stated_minimum() {
        let props = TableRowProperties {
            height: RowHeight {
                value_twips: Some(6000),
                rule: Some(HeightRule::AtLeast),
            },
            ..TableRowProperties::default()
        };
        let row = flow_single_row(one_cell_table(props, "short"), Twip(9000));
        assert_eq!(
            row.height(),
            Twip(6000),
            "atLeast holds the row to its stated height when content is shorter"
        );
    }

    #[test]
    fn exact_row_height_is_fixed_and_clips_overflow() {
        let props = TableRowProperties {
            height: RowHeight {
                value_twips: Some(300),
                rule: Some(HeightRule::Exact),
            },
            ..TableRowProperties::default()
        };
        // A long sentence wraps to several lines in a 3000-twip column, exceeding
        // the 300-twip exact height.
        let table = one_cell_table(
            props,
            "This is a fairly long sentence that wraps across several lines in a narrow column.",
        );
        let BlockFragment::TableRow { height, clip, .. } = flow_single_row(table, Twip(9000))
        else {
            panic!("expected a row");
        };
        assert_eq!(height, Twip(300), "exact pins the row height");
        assert!(clip, "content taller than an exact row is clipped");
    }

    #[test]
    fn vertical_merge_owns_content_height_and_closing_border_by_grid_range() {
        let blue = RgbColor { r: 0, g: 0, b: 255 };
        let red = RgbColor { r: 255, g: 0, b: 0 };
        let table = Table {
            id: node(500),
            grid: vec![
                GridColumn {
                    width_twips: Some(1200),
                },
                GridColumn {
                    width_twips: Some(1200),
                },
                GridColumn {
                    width_twips: Some(1200),
                },
            ],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![
                ModelRow {
                    id: node(501),
                    properties: TableRowProperties {
                        header: true,
                        ..TableRowProperties::default()
                    },
                    cells: vec![
                        text_cell(510, TableCellProperties::default(), "left top"),
                        text_cell(
                            511,
                            TableCellProperties {
                                grid_span: Some(2),
                                vertical_merge: Some(VerticalMerge::Restart),
                                ..TableCellProperties::default()
                            },
                            "The merged cell owns enough wrapped content to constrain the sum of both physical rows.",
                        ),
                    ],
                },
                ModelRow {
                    id: node(502),
                    properties: TableRowProperties::default(),
                    cells: vec![
                        text_cell(520, TableCellProperties::default(), "left bottom"),
                        text_cell(
                            521,
                            TableCellProperties {
                                grid_span: Some(2),
                                vertical_merge: Some(VerticalMerge::Continue),
                                borders: TableBorders {
                                    bottom: Some(edge("single", 4)),
                                    ..TableBorders::default()
                                },
                                ..TableCellProperties::default()
                            },
                            "continuation content must not render",
                        ),
                    ],
                },
                ModelRow {
                    id: node(503),
                    properties: TableRowProperties::default(),
                    cells: vec![
                        text_cell(530, TableCellProperties::default(), "left after"),
                        text_cell(
                            531,
                            TableCellProperties {
                                borders: TableBorders {
                                    top: Some(colored_edge("double", 24, blue)),
                                    ..TableBorders::default()
                                },
                                ..TableCellProperties::default()
                            },
                            "after one",
                        ),
                        text_cell(
                            532,
                            TableCellProperties {
                                borders: TableBorders {
                                    top: Some(colored_edge("dotted", 12, red)),
                                    ..TableBorders::default()
                                },
                                ..TableCellProperties::default()
                            },
                            "after two",
                        ),
                    ],
                },
            ],
        };

        let rows = flow_table_rows(table, Twip(3600));
        let [
            BlockFragment::TableRow {
                cells: top,
                height: top_height,
                header: top_header,
                merge_keep_next,
                ..
            },
            BlockFragment::TableRow {
                cells: bottom,
                height: bottom_height,
                header: bottom_header,
                ..
            },
            BlockFragment::TableRow { .. },
        ] = rows.as_slice()
        else {
            panic!("expected three table rows");
        };

        let merged_height = match top[1].vertical_merge {
            CellVerticalMerge::Restart { height } => height,
            other => panic!("expected a merge restart, got {other:?}"),
        };
        assert_eq!(merged_height, *top_height + *bottom_height);
        assert!(
            merged_height.raw() >= top[1].occupied_height().raw(),
            "the merged content constrains the sum of covered row heights"
        );
        assert_eq!(bottom[1].vertical_merge, CellVerticalMerge::Continue);
        assert!(
            bottom[1].blocks.is_empty(),
            "the continuation cell owns no independently rendered content"
        );
        assert_eq!((top[1].x, top[1].width), (bottom[1].x, bottom[1].width));
        assert!(*merge_keep_next, "the merge boundary is page-local");
        assert!(
            !*top_header && !*bottom_header,
            "a merge crossing from a header row into body rows cannot repeat partially"
        );
        assert_eq!(
            top[1].borders.bottom,
            Some(ResolvedEdge {
                color: [0, 0, 255, 255],
                width: Twip(60),
                pattern: BorderPattern::Double,
            }),
            "the merged box closes with the final continuation's bottom edge"
        );
        let [first, second] = top[1].borders.bottom_segments.as_slice() else {
            panic!("the closing side keeps two independently styled segments")
        };
        assert_eq!(first.offset, Twip::ZERO);
        assert_eq!(first.length, second.offset);
        assert_eq!(first.length + second.length, top[1].width);
        assert_eq!(
            (first.edge.color, first.edge.width, first.edge.pattern),
            ([0, 0, 255, 255], Twip(60), BorderPattern::Double)
        );
        assert_eq!(
            (second.edge.color, second.edge.width, second.edge.pattern),
            ([255, 0, 0, 255], Twip(30), BorderPattern::Dotted),
            "the restart copies the closing continuation's styled subsegments"
        );
    }

    #[test]
    fn a_range_mismatched_vertical_continuation_remains_an_ordinary_visible_cell() {
        let table = Table {
            id: node(600),
            grid: vec![
                GridColumn {
                    width_twips: Some(1200),
                },
                GridColumn {
                    width_twips: Some(1200),
                },
                GridColumn {
                    width_twips: Some(1200),
                },
            ],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![
                ModelRow {
                    id: node(601),
                    properties: TableRowProperties::default(),
                    cells: vec![
                        text_cell(610, TableCellProperties::default(), "left"),
                        text_cell(
                            611,
                            TableCellProperties {
                                grid_span: Some(2),
                                vertical_merge: Some(VerticalMerge::Restart),
                                ..TableCellProperties::default()
                            },
                            "restart over two columns",
                        ),
                    ],
                },
                ModelRow {
                    id: node(602),
                    properties: TableRowProperties::default(),
                    cells: vec![
                        text_cell(620, TableCellProperties::default(), "left"),
                        text_cell(
                            621,
                            TableCellProperties {
                                vertical_merge: Some(VerticalMerge::Continue),
                                ..TableCellProperties::default()
                            },
                            "mismatched continuation stays visible",
                        ),
                        text_cell(622, TableCellProperties::default(), "right"),
                    ],
                },
            ],
        };

        let rows = flow_table_rows(table, Twip(3600));
        let [
            BlockFragment::TableRow {
                cells: top,
                merge_keep_next,
                ..
            },
            BlockFragment::TableRow { cells: bottom, .. },
        ] = rows.as_slice()
        else {
            panic!("expected two table rows");
        };
        assert_eq!(top[1].vertical_merge, CellVerticalMerge::None);
        assert_eq!(bottom[1].vertical_merge, CellVerticalMerge::None);
        assert!(
            !bottom[1].blocks.is_empty(),
            "malformed producer content is not silently hidden"
        );
        assert!(!merge_keep_next);
    }

    #[test]
    fn overlapping_merge_constraints_publish_only_final_row_heights() {
        let merge = |id, role, text| {
            text_cell(
                id,
                TableCellProperties {
                    vertical_merge: Some(role),
                    ..TableCellProperties::default()
                },
                text,
            )
        };
        let table = Table {
            id: node(700),
            grid: vec![
                GridColumn {
                    width_twips: Some(1000),
                },
                GridColumn {
                    width_twips: Some(1000),
                },
            ],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![
                ModelRow {
                    id: node(701),
                    properties: TableRowProperties::default(),
                    cells: vec![
                        merge(710, VerticalMerge::Restart, "short"),
                        merge(
                            711,
                            VerticalMerge::Restart,
                            "A much taller second merge wraps over many lines and grows a physical row shared with the first merge.",
                        ),
                    ],
                },
                ModelRow {
                    id: node(702),
                    properties: TableRowProperties::default(),
                    cells: vec![
                        merge(720, VerticalMerge::Continue, "ignored"),
                        merge(721, VerticalMerge::Continue, "ignored"),
                    ],
                },
                ModelRow {
                    id: node(703),
                    properties: TableRowProperties::default(),
                    cells: vec![
                        text_cell(730, TableCellProperties::default(), "ordinary"),
                        merge(731, VerticalMerge::Continue, "ignored"),
                    ],
                },
            ],
        };

        let rows = flow_table_rows(table, Twip(2000));
        let heights: Vec<Twip> = rows.iter().map(BlockFragment::height).collect();
        let BlockFragment::TableRow { cells: origins, .. } = &rows[0] else {
            unreachable!()
        };
        assert_eq!(
            origins[0].vertical_merge,
            CellVerticalMerge::Restart {
                height: heights[0] + heights[1]
            },
            "the shorter merge observes growth caused by the overlapping taller merge"
        );
        assert_eq!(
            origins[1].vertical_merge,
            CellVerticalMerge::Restart {
                height: heights[0] + heights[1] + heights[2]
            }
        );
    }

    #[test]
    fn all_exact_rows_clip_merged_content_to_their_authored_total() {
        let exact = TableRowProperties {
            height: RowHeight {
                value_twips: Some(100),
                rule: Some(HeightRule::Exact),
            },
            ..TableRowProperties::default()
        };
        let table = Table {
            id: node(800),
            grid: vec![GridColumn {
                width_twips: Some(1200),
            }],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![
                ModelRow {
                    id: node(801),
                    properties: exact.clone(),
                    cells: vec![text_cell(
                        810,
                        TableCellProperties {
                            vertical_merge: Some(VerticalMerge::Restart),
                            ..TableCellProperties::default()
                        },
                        "This merged content is deliberately much taller than two exact one-hundred-twip rows.",
                    )],
                },
                ModelRow {
                    id: node(802),
                    properties: exact,
                    cells: vec![text_cell(
                        820,
                        TableCellProperties {
                            vertical_merge: Some(VerticalMerge::Continue),
                            ..TableCellProperties::default()
                        },
                        "ignored",
                    )],
                },
            ],
        };

        let rows = flow_table_rows(table, Twip(1200));
        let BlockFragment::TableRow {
            cells,
            height,
            clip,
            ..
        } = &rows[0]
        else {
            unreachable!()
        };
        assert_eq!(*height, Twip(100));
        assert!(*clip);
        assert_eq!(
            cells[0].vertical_merge,
            CellVerticalMerge::Restart { height: Twip(200) }
        );
        assert_eq!(rows[1].height(), Twip(100));
    }

    #[test]
    fn border_conflict_resolution_picks_the_higher_precedence_edge() {
        // Two adjacent cells share an edge: cell 0's end is a thin single line,
        // cell 1's start is a thick double line. The thicker/double border wins on
        // both sides of the shared edge.
        let cell0 = TableCell {
            id: node(60),
            properties: TableCellProperties {
                borders: TableBorders {
                    end: Some(edge("single", 4)),
                    ..TableBorders::default()
                },
                ..TableCellProperties::default()
            },
            blocks: vec![paragraph(
                160,
                vec![run_node(260, "a", RunProperties::default())],
            )],
        };
        let cell1 = TableCell {
            id: node(61),
            properties: TableCellProperties {
                borders: TableBorders {
                    start: Some(edge("double", 24)),
                    ..TableBorders::default()
                },
                ..TableCellProperties::default()
            },
            blocks: vec![paragraph(
                161,
                vec![run_node(261, "b", RunProperties::default())],
            )],
        };
        let table = Table {
            id: node(50),
            grid: vec![
                GridColumn {
                    width_twips: Some(3000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![cell0, cell1],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        // sz 24 eighth-points = 24*20/8 = 60 twips.
        let winner = ResolvedEdge {
            color: [0, 0, 0, 255],
            width: Twip(60),
            pattern: BorderPattern::Double,
        };
        assert_eq!(
            cells[0].borders.end,
            Some(winner),
            "the shared edge shows the double border on cell 0"
        );
        assert_eq!(
            cells[1].borders.start,
            Some(winner),
            "and the same winner on cell 1"
        );
    }

    #[test]
    fn common_border_tokens_map_to_typed_paint_patterns() {
        assert_eq!(border_pattern("single"), BorderPattern::Solid);
        assert_eq!(border_pattern("double"), BorderPattern::Double);
        assert_eq!(border_pattern("dotted"), BorderPattern::Dotted);
        assert_eq!(border_pattern("dashSmallGap"), BorderPattern::Dashed);
        assert_eq!(border_pattern("dotDash"), BorderPattern::DotDash);
        assert_eq!(border_pattern("dashDotStroked"), BorderPattern::DotDash);
        assert_eq!(border_pattern("dotDotDash"), BorderPattern::DotDotDash);
        assert_eq!(
            border_pattern("apples"),
            BorderPattern::Solid,
            "art-border tokens keep the documented deterministic fallback"
        );
    }

    #[test]
    fn paragraph_cache_hash_includes_the_resolved_border_pattern() {
        let shape = ShapeInputs {
            items: &[],
            tab_stops: &[],
            default_tab: Twip(720),
            constraints: LineConstraints::default(),
        };
        let decor = |pattern| ParagraphDecor {
            borders: BlockBorders {
                bottom: Some(ResolvedEdge {
                    color: [0, 0, 0, 255],
                    width: Twip(20),
                    pattern,
                }),
                ..BlockBorders::default()
            },
            width: Twip(5000),
            ..ParagraphDecor::default()
        };
        let solid = paragraph_hash(
            node(1),
            &shape,
            BoxMetrics::default(),
            BreakControl::default(),
            decor(BorderPattern::Solid),
            None,
            None,
        );
        let dashed = paragraph_hash(
            node(1),
            &shape,
            BoxMetrics::default(),
            BreakControl::default(),
            decor(BorderPattern::Dashed),
            None,
            None,
        );
        assert_ne!(
            solid, dashed,
            "a style-only border edit cannot reuse stale paragraph paint"
        );
    }

    #[test]
    fn a_stronger_table_border_overrides_a_missing_cell_border() {
        // No cell border, but a table outer border: the cell inherits it.
        let table = Table {
            id: node(50),
            grid: vec![GridColumn {
                width_twips: Some(3000),
            }],
            grid_change: None,
            properties: TableProperties {
                borders: TableBorders {
                    top: Some(edge("single", 8)),
                    ..TableBorders::default()
                },
                ..TableProperties::default()
            },
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![text_cell(60, TableCellProperties::default(), "a")],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        let top = cells[0].borders.top.expect("the table top border applies");
        assert_eq!(top.width, Twip(20), "sz 8 eighth-points = 20 twips");
    }

    #[test]
    fn a_direct_cell_border_is_derived_before_the_table_fallback() {
        let table = Table {
            id: node(50),
            grid: vec![GridColumn {
                width_twips: Some(3000),
            }],
            grid_change: None,
            properties: TableProperties {
                borders: TableBorders {
                    top: Some(edge("double", 24)),
                    ..TableBorders::default()
                },
                ..TableProperties::default()
            },
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![text_cell(
                    60,
                    TableCellProperties {
                        borders: TableBorders {
                            top: Some(edge("single", 4)),
                            ..TableBorders::default()
                        },
                        ..TableCellProperties::default()
                    },
                    "direct",
                )],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        assert_eq!(
            cells[0].borders.top.map(|border| border.width),
            Some(Twip(10)),
            "the direct cell edge wins before the table edge enters conflict resolution"
        );
    }

    #[test]
    fn table_outer_and_inside_horizontal_borders_apply_to_the_correct_rows() {
        let red = RgbColor { r: 255, g: 0, b: 0 };
        let blue = RgbColor { r: 0, g: 0, b: 255 };
        let green = RgbColor { r: 0, g: 128, b: 0 };
        let table = Table {
            id: node(50),
            grid: vec![GridColumn {
                width_twips: Some(3000),
            }],
            grid_change: None,
            properties: TableProperties {
                borders: TableBorders {
                    top: Some(colored_edge("single", 8, red)),
                    inside_h: Some(colored_edge("single", 4, blue)),
                    bottom: Some(colored_edge("single", 12, green)),
                    ..TableBorders::default()
                },
                ..TableProperties::default()
            },
            rows: vec![
                ModelRow {
                    id: node(51),
                    properties: TableRowProperties::default(),
                    cells: vec![text_cell(60, TableCellProperties::default(), "top")],
                },
                ModelRow {
                    id: node(52),
                    properties: TableRowProperties::default(),
                    cells: vec![text_cell(61, TableCellProperties::default(), "bottom")],
                },
            ],
        };

        let rows = flow_table_rows(table, Twip(9000));
        let [
            BlockFragment::TableRow {
                cells: top_cells, ..
            },
            BlockFragment::TableRow {
                cells: bottom_cells,
                ..
            },
        ] = rows.as_slice()
        else {
            panic!("expected two table rows");
        };

        assert_eq!(
            top_cells[0].borders.top,
            Some(ResolvedEdge {
                color: [255, 0, 0, 255],
                width: Twip(20),
                pattern: BorderPattern::Solid,
            }),
            "only the first row receives the table top border"
        );
        let inside = Some(ResolvedEdge {
            color: [0, 0, 255, 255],
            width: Twip(10),
            pattern: BorderPattern::Solid,
        });
        assert_eq!(top_cells[0].borders.bottom, inside);
        assert_eq!(
            bottom_cells[0].borders.top, inside,
            "both sides of the shared row boundary resolve to insideH"
        );
        assert_eq!(
            bottom_cells[0].borders.bottom,
            Some(ResolvedEdge {
                color: [0, 128, 0, 255],
                width: Twip(30),
                pattern: BorderPattern::Solid,
            }),
            "only the final row receives the table bottom border"
        );
    }

    #[test]
    fn horizontal_border_conflicts_compare_abutting_rows() {
        let red = RgbColor { r: 255, g: 0, b: 0 };
        let blue = RgbColor { r: 0, g: 0, b: 255 };
        let upper = text_cell(
            60,
            TableCellProperties {
                borders: TableBorders {
                    bottom: Some(colored_edge("single", 4, red)),
                    ..TableBorders::default()
                },
                ..TableCellProperties::default()
            },
            "upper",
        );
        let lower = text_cell(
            61,
            TableCellProperties {
                borders: TableBorders {
                    top: Some(colored_edge("double", 24, blue)),
                    ..TableBorders::default()
                },
                ..TableCellProperties::default()
            },
            "lower",
        );
        let table = Table {
            id: node(50),
            grid: vec![GridColumn {
                width_twips: Some(3000),
            }],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![
                ModelRow {
                    id: node(51),
                    properties: TableRowProperties::default(),
                    cells: vec![upper],
                },
                ModelRow {
                    id: node(52),
                    properties: TableRowProperties::default(),
                    cells: vec![lower],
                },
            ],
        };

        let rows = flow_table_rows(table, Twip(9000));
        let [
            BlockFragment::TableRow {
                cells: upper_cells, ..
            },
            BlockFragment::TableRow {
                cells: lower_cells, ..
            },
        ] = rows.as_slice()
        else {
            panic!("expected two table rows");
        };
        let winner = Some(ResolvedEdge {
            color: [0, 0, 255, 255],
            width: Twip(60),
            pattern: BorderPattern::Double,
        });
        assert_eq!(upper_cells[0].borders.bottom, winner);
        assert_eq!(lower_cells[0].borders.top, winner);
    }

    #[test]
    fn horizontal_conflicts_find_every_cell_overlapping_a_grid_span() {
        let spanning = text_cell(
            60,
            TableCellProperties {
                grid_span: Some(2),
                borders: TableBorders {
                    bottom: Some(edge("single", 4)),
                    ..TableBorders::default()
                },
                ..TableCellProperties::default()
            },
            "span",
        );
        let lower_left = text_cell(
            61,
            TableCellProperties {
                borders: TableBorders {
                    top: Some(edge("double", 24)),
                    ..TableBorders::default()
                },
                ..TableCellProperties::default()
            },
            "left",
        );
        let lower_right = text_cell(
            62,
            TableCellProperties {
                borders: TableBorders {
                    top: Some(edge("single", 8)),
                    ..TableBorders::default()
                },
                ..TableCellProperties::default()
            },
            "right",
        );
        let table = Table {
            id: node(50),
            grid: vec![
                GridColumn {
                    width_twips: Some(3000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![
                ModelRow {
                    id: node(51),
                    properties: TableRowProperties::default(),
                    cells: vec![spanning],
                },
                ModelRow {
                    id: node(52),
                    properties: TableRowProperties::default(),
                    cells: vec![lower_left, lower_right],
                },
            ],
        };

        let rows = flow_table_rows(table, Twip(9000));
        let [
            BlockFragment::TableRow {
                cells: upper_cells, ..
            },
            BlockFragment::TableRow {
                cells: lower_cells, ..
            },
        ] = rows.as_slice()
        else {
            panic!("expected two table rows");
        };
        assert_eq!(
            upper_cells[0].borders.bottom.map(|border| border.width),
            Some(Twip(60)),
            "the spanning edge inspects both abutting lower cells"
        );
        assert_eq!(
            upper_cells[0].borders.bottom_segments,
            vec![
                ResolvedBorderSegment {
                    offset: Twip(0),
                    length: Twip(3000),
                    edge: ResolvedEdge {
                        color: [0, 0, 0, 255],
                        width: Twip(60),
                        pattern: BorderPattern::Double,
                    },
                },
                ResolvedBorderSegment {
                    offset: Twip(3000),
                    length: Twip(3000),
                    edge: ResolvedEdge {
                        color: [0, 0, 0, 255],
                        width: Twip(20),
                        pattern: BorderPattern::Solid,
                    },
                },
            ],
            "each differently styled abutting half keeps its own conflict winner"
        );
        assert_eq!(
            lower_cells[0].borders.top.map(|border| border.width),
            Some(Twip(60))
        );
        assert_eq!(
            lower_cells[1].borders.top.map(|border| border.width),
            Some(Twip(20))
        );
    }

    #[test]
    fn equal_horizontal_segment_winners_are_coalesced() {
        let spanning = text_cell(
            60,
            TableCellProperties {
                grid_span: Some(2),
                borders: TableBorders {
                    bottom: Some(edge("single", 4)),
                    ..TableBorders::default()
                },
                ..TableCellProperties::default()
            },
            "span",
        );
        let same_top = || TableCellProperties {
            borders: TableBorders {
                top: Some(edge("dashed", 8)),
                ..TableBorders::default()
            },
            ..TableCellProperties::default()
        };
        let table = Table {
            id: node(50),
            grid: vec![
                GridColumn {
                    width_twips: Some(3000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![
                ModelRow {
                    id: node(51),
                    properties: TableRowProperties::default(),
                    cells: vec![spanning],
                },
                ModelRow {
                    id: node(52),
                    properties: TableRowProperties::default(),
                    cells: vec![
                        text_cell(61, same_top(), "left"),
                        text_cell(62, same_top(), "right"),
                    ],
                },
            ],
        };
        let rows = flow_table_rows(table, Twip(9000));
        let BlockFragment::TableRow { cells, .. } = &rows[0] else {
            panic!("expected a row")
        };
        assert_eq!(
            cells[0].borders.bottom_segments,
            vec![ResolvedBorderSegment {
                offset: Twip(0),
                length: Twip(6000),
                edge: ResolvedEdge {
                    color: [0, 0, 0, 255],
                    width: Twip(20),
                    pattern: BorderPattern::Dashed,
                },
            }]
        );
    }

    #[test]
    fn a_nil_cell_border_suppresses_the_abutting_visible_border() {
        let left = text_cell(
            60,
            TableCellProperties {
                borders: TableBorders {
                    end: Some(edge("single", 8)),
                    ..TableBorders::default()
                },
                ..TableCellProperties::default()
            },
            "left",
        );
        let right = text_cell(
            61,
            TableCellProperties {
                borders: TableBorders {
                    start: Some(edge("nil", 0)),
                    ..TableBorders::default()
                },
                ..TableCellProperties::default()
            },
            "right",
        );
        let table = Table {
            id: node(50),
            grid: vec![
                GridColumn {
                    width_twips: Some(3000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![left, right],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        assert_eq!(cells[0].borders.end, None);
        assert_eq!(cells[1].borders.start, None);
    }

    #[test]
    fn cell_shading_overrides_the_table_shading_fallback() {
        let table_fill = RgbColor {
            r: 240,
            g: 230,
            b: 220,
        };
        let cell_fill = RgbColor {
            r: 10,
            g: 20,
            b: 30,
        };
        let table = Table {
            id: node(50),
            grid: vec![
                GridColumn {
                    width_twips: Some(3000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            grid_change: None,
            properties: TableProperties {
                shading: Shading {
                    fill: Some(table_fill),
                },
                ..TableProperties::default()
            },
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![
                    text_cell(60, TableCellProperties::default(), "table fill"),
                    text_cell(
                        61,
                        TableCellProperties {
                            shading: Shading {
                                fill: Some(cell_fill),
                            },
                            ..TableCellProperties::default()
                        },
                        "cell fill",
                    ),
                ],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        assert_eq!(cells[0].shading, Some([240, 230, 220, 255]));
        assert_eq!(cells[1].shading, Some([10, 20, 30, 255]));
    }

    #[test]
    fn table_style_and_cnf_style_conditional_shading_differ_from_the_unstyled_default() {
        // Regression (table-style cascade, docs/46 "Table style and conditional
        // formatting"): a table referencing a `w:tblStyle` with a header-row
        // `w:tblStylePr` and a plain base cell fill must resolve the header row
        // to the region's fill and every other row to the style's base fill —
        // neither of which a cell/table direct `w:shd` provides. An identical
        // table with no `style_ref` (or with `tblLook` disabling the header
        // option) must fall back to no fill at all, proving the cascade — not a
        // hard-coded default — drives the difference.
        use casual_doc_model::v1::{
            CnfStyle, Style, StyleKind, TableLook, TableStyleOverride, TableStyleRegion,
        };

        let base_fill = RgbColor {
            r: 220,
            g: 220,
            b: 220,
        };
        let header_fill = RgbColor {
            r: 40,
            g: 70,
            b: 130,
        };
        let sid = StyleId::new(NodeId::from_parts(9, 1).unwrap());
        let table_style = Style {
            kind: StyleKind::Table,
            is_default: false,
            name: None,
            aliases: None,
            based_on: None,
            next: None,
            link: None,
            hidden: false,
            ui_priority: None,
            semi_hidden: false,
            unhide_when_used: false,
            q_format: false,
            locked: false,
            paragraph: None,
            run: None,
            table: None,
            table_row: None,
            table_cell: Some(TableCellProperties {
                shading: Shading {
                    fill: Some(base_fill),
                },
                ..TableCellProperties::default()
            }),
            conditional: vec![TableStyleOverride {
                region: TableStyleRegion::FirstRow,
                paragraph: None,
                run: None,
                table: None,
                table_row: None,
                table_cell: Some(TableCellProperties {
                    shading: Shading {
                        fill: Some(header_fill),
                    },
                    ..TableCellProperties::default()
                }),
            }],
        };
        let mut definitions = Definitions::default();
        definitions.styles.insert(sid, table_style);

        let build = |style_ref: Option<StyleId>, look: TableLook| Table {
            id: node(50),
            grid: vec![GridColumn {
                width_twips: Some(3000),
            }],
            grid_change: None,
            properties: TableProperties {
                style_ref,
                look,
                ..TableProperties::default()
            },
            rows: vec![
                ModelRow {
                    id: node(51),
                    properties: TableRowProperties {
                        conditional_format: Some(CnfStyle {
                            first_row: true,
                            ..CnfStyle::default()
                        }),
                        ..TableRowProperties::default()
                    },
                    cells: vec![text_cell(60, TableCellProperties::default(), "header")],
                },
                ModelRow {
                    id: node(52),
                    properties: TableRowProperties::default(),
                    cells: vec![text_cell(62, TableCellProperties::default(), "body")],
                },
            ],
        };

        let styled = build(
            Some(sid),
            TableLook {
                first_row: true,
                ..TableLook::default()
            },
        );
        let shaper = ParleyShaper::new();
        let document =
            document_with_definitions(vec![BlockNode::Table(styled)], definitions.clone());
        let galley = build_galley(&document, &shaper, Twip(9000));
        let BlockFragment::TableRow {
            cells: header_row, ..
        } = &galley[0]
        else {
            panic!("expected the header row fragment");
        };
        let BlockFragment::TableRow {
            cells: body_row, ..
        } = &galley[1]
        else {
            panic!("expected the body row fragment");
        };
        assert_eq!(
            header_row[0].shading,
            Some([40, 70, 130, 255]),
            "the first-row conditional region fill applies, not the style base"
        );
        assert_eq!(
            body_row[0].shading,
            Some([220, 220, 220, 255]),
            "a row matching no conditional region still gets the style's base fill"
        );

        // Same table, `tblLook` no longer enabling the header-row option: the
        // first row's `w:cnfStyle` bit is now ignored (Word's "Header Row"
        // checkbox unticked) and only the base fill remains for every row.
        let look_disabled = build(Some(sid), TableLook::default());
        let document_disabled =
            document_with_definitions(vec![BlockNode::Table(look_disabled)], definitions);
        let galley_disabled = build_galley(&document_disabled, &shaper, Twip(9000));
        let BlockFragment::TableRow {
            cells: header_row_disabled,
            ..
        } = &galley_disabled[0]
        else {
            panic!("expected the header row fragment");
        };
        assert_eq!(
            header_row_disabled[0].shading,
            Some([220, 220, 220, 255]),
            "disabling tblLook.first_row suppresses the conditional region"
        );

        // The unstyled default: no `style_ref` at all, and neither the table nor
        // the cells set a direct `w:shd` — no fill anywhere.
        let unstyled = build(None, TableLook::default());
        let BlockFragment::TableRow {
            cells: unstyled_row,
            ..
        } = flow_single_row(unstyled, Twip(9000))
        else {
            panic!("expected a row");
        };
        assert_eq!(
            unstyled_row[0].shading, None,
            "no style_ref means no table-style layer contributes a fill"
        );
    }

    #[test]
    fn conditional_table_style_text_and_borders_drive_measurement_and_final_flow() {
        use casual_doc_model::v1::{
            CnfStyle, Color, Style, StyleKind, TableLook, TableStyleOverride, TableStyleRegion,
        };

        let sid = StyleId::new(NodeId::from_parts(9, 2).unwrap());
        let green = RgbColor {
            r: 20,
            g: 140,
            b: 60,
        };
        let table_style = Style {
            kind: StyleKind::Table,
            is_default: false,
            name: None,
            aliases: None,
            based_on: None,
            next: None,
            link: None,
            hidden: false,
            ui_priority: None,
            semi_hidden: false,
            unhide_when_used: false,
            q_format: false,
            locked: false,
            paragraph: None,
            run: Some(RunProperties {
                size_half_points: Some(20),
                ..RunProperties::default()
            }),
            table: Some(TableProperties {
                borders: TableBorders {
                    top: Some(colored_edge(
                        "single",
                        8,
                        RgbColor {
                            r: 160,
                            g: 30,
                            b: 30,
                        },
                    )),
                    ..TableBorders::default()
                },
                ..TableProperties::default()
            }),
            table_row: None,
            table_cell: None,
            conditional: vec![TableStyleOverride {
                region: TableStyleRegion::FirstRow,
                paragraph: Some(ParagraphProperties {
                    spacing: Some(Spacing {
                        before_twips: Some(180),
                        after_twips: Some(180),
                        ..Spacing::default()
                    }),
                    ..ParagraphProperties::default()
                }),
                run: Some(RunProperties {
                    bold: Some(true),
                    size_half_points: Some(40),
                    color: Some(Color::Rgb(RgbColor {
                        r: 30,
                        g: 60,
                        b: 120,
                    })),
                    ..RunProperties::default()
                }),
                table: None,
                table_row: None,
                table_cell: Some(TableCellProperties {
                    borders: TableBorders {
                        top: Some(colored_edge("double", 16, green)),
                        ..TableBorders::default()
                    },
                    ..TableCellProperties::default()
                }),
            }],
        };
        let mut definitions = Definitions::default();
        definitions.styles.insert(sid, table_style);
        let build = |style_ref| Table {
            id: node(350),
            grid: Vec::new(),
            grid_change: None,
            properties: TableProperties {
                style_ref,
                look: TableLook {
                    first_row: true,
                    ..TableLook::default()
                },
                ..TableProperties::default()
            },
            rows: vec![
                ModelRow {
                    id: node(351),
                    properties: TableRowProperties {
                        conditional_format: Some(CnfStyle {
                            first_row: true,
                            ..CnfStyle::default()
                        }),
                        ..TableRowProperties::default()
                    },
                    cells: vec![text_cell(
                        352,
                        TableCellProperties::default(),
                        "conditional header metrics",
                    )],
                },
                ModelRow {
                    id: node(353),
                    properties: TableRowProperties::default(),
                    cells: vec![text_cell(
                        354,
                        TableCellProperties::default(),
                        "conditional header metrics",
                    )],
                },
            ],
        };

        let shaper = ParleyShaper::new();
        let styled_doc = document_with_definitions(
            vec![BlockNode::Table(build(Some(sid)))],
            definitions.clone(),
        );
        let styled = build_galley(&styled_doc, &shaper, Twip(9000));
        let unstyled_doc =
            document_with_definitions(vec![BlockNode::Table(build(None))], definitions);
        let unstyled = build_galley(&unstyled_doc, &shaper, Twip(9000));

        fn row(fragment: &BlockFragment) -> (&[CellFragment], Twip) {
            match fragment {
                BlockFragment::TableRow { cells, height, .. } => (cells, *height),
                BlockFragment::Paragraph { .. } => panic!("expected table row"),
            }
        }
        fn run(cell: &CellFragment) -> &GlyphRun {
            match &cell.blocks[0] {
                BlockFragment::Paragraph { lines, .. } => &lines.lines[0].runs[0],
                BlockFragment::TableRow { .. } => panic!("expected paragraph"),
            }
        }
        let (header, header_height) = row(&styled[0]);
        let (body, body_height) = row(&styled[1]);
        let (unstyled_header, _) = row(&unstyled[0]);

        assert_eq!(run(&header[0]).size, Twip(400));
        assert_eq!(run(&header[0]).color, [30, 60, 120, 255]);
        assert_eq!(run(&body[0]).size, Twip(200));
        assert!(
            header_height > body_height,
            "conditional pPr spacing grows the header row"
        );
        assert!(
            header[0].width > unstyled_header[0].width,
            "auto-fit measurement uses the conditional 20pt font"
        );
        assert_eq!(header[0].borders.top.unwrap().color, [20, 140, 60, 255]);
        assert_eq!(
            header[0].borders.top.unwrap().pattern,
            BorderPattern::Double
        );
    }

    #[test]
    fn an_unstyled_nested_table_does_not_inherit_the_outer_table_style() {
        use casual_doc_model::v1::{Style, StyleKind};

        let sid = StyleId::new(NodeId::from_parts(9, 3).unwrap());
        let style = Style {
            kind: StyleKind::Table,
            is_default: false,
            name: None,
            aliases: None,
            based_on: None,
            next: None,
            link: None,
            hidden: false,
            ui_priority: None,
            semi_hidden: false,
            unhide_when_used: false,
            q_format: false,
            locked: false,
            paragraph: None,
            run: Some(RunProperties {
                size_half_points: Some(40),
                ..RunProperties::default()
            }),
            table: None,
            table_row: None,
            table_cell: None,
            conditional: Vec::new(),
        };
        let nested = Table {
            id: node(420),
            grid: vec![GridColumn {
                width_twips: Some(1800),
            }],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![ModelRow {
                id: node(421),
                properties: TableRowProperties::default(),
                cells: vec![text_cell(422, TableCellProperties::default(), "nested")],
            }],
        };
        let outer = Table {
            id: node(410),
            grid: vec![GridColumn {
                width_twips: Some(4000),
            }],
            grid_change: None,
            properties: TableProperties {
                style_ref: Some(sid),
                ..TableProperties::default()
            },
            rows: vec![ModelRow {
                id: node(411),
                properties: TableRowProperties::default(),
                cells: vec![TableCell {
                    id: node(412),
                    properties: TableCellProperties::default(),
                    blocks: vec![
                        paragraph(413, vec![run_node(414, "before", RunProperties::default())]),
                        BlockNode::Table(nested),
                        paragraph(415, vec![run_node(416, "after", RunProperties::default())]),
                    ],
                }],
            }],
        };
        let mut definitions = Definitions::default();
        definitions.styles.insert(sid, style);
        let shaper = ParleyShaper::new();
        let galley = build_galley(
            &document_with_definitions(vec![BlockNode::Table(outer)], definitions),
            &shaper,
            Twip(9000),
        );
        let BlockFragment::TableRow { cells, .. } = &galley[0] else {
            panic!("expected outer row");
        };
        let run_size = |block: &BlockFragment| match block {
            BlockFragment::Paragraph { lines, .. } => lines.lines[0].runs[0].size,
            BlockFragment::TableRow { cells, .. } => match &cells[0].blocks[0] {
                BlockFragment::Paragraph { lines, .. } => lines.lines[0].runs[0].size,
                BlockFragment::TableRow { .. } => panic!("expected nested paragraph"),
            },
        };
        assert_eq!(run_size(&cells[0].blocks[0]), Twip(400));
        assert_eq!(run_size(&cells[0].blocks[1]), Twip(220));
        assert_eq!(run_size(&cells[0].blocks[2]), Twip(400));
    }

    #[test]
    fn an_inline_drawing_becomes_an_image_paint_item_with_the_extent_derived_rect() {
        use crate::compose::compose_paragraph;
        use crate::display::PaintItem;
        use casual_doc_model::v1::{Drawing, Extent, MediaId, MediaReference};

        // A media table with one embedded PNG, referenced by an inline drawing.
        let media_id = MediaId::new(NodeId::from_parts(70, 1).unwrap());
        let mut media = DefinitionMap::default();
        media.insert(
            media_id,
            MediaReference {
                relationship_id: "rId7".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/image1.png".to_owned(),
            },
        );
        let definitions = Definitions {
            media,
            ..Definitions::default()
        };
        let para = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(10, 1).unwrap(),
            properties: ParagraphProperties::default(),
            inlines: vec![
                run_node(12, "before ", RunProperties::default()),
                InlineNode::Drawing(Drawing {
                    id: NodeId::from_parts(11, 1).unwrap(),
                    media: media_id,
                    // 190500 × 127000 EMU (635 EMU/twip) → 300 × 200 twips.
                    extent: Some(Extent {
                        width_emu: 190_500,
                        height_emu: 127_000,
                    }),
                    descr: None,
                    crop: None,
                }),
                run_node(13, " after", RunProperties::default()),
            ],
        });
        let doc =
            Document::new(NodeId::from_parts(1, 1).unwrap(), vec![para], definitions).unwrap();

        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!("expected a paragraph fragment");
        };
        // The drawing became an inline image box, sized from the EMU extent and
        // keyed by its package part name.
        let image_line = lines
            .lines
            .iter()
            .find(|line| !line.images.is_empty())
            .expect("an inline image was placed");
        let image = &image_line.images[0];
        assert_eq!(image.media, "word/media/image1.png");
        assert_eq!(
            image.size,
            Size::new(Twip(300), Twip(200)),
            "190500×127000 EMU resolves to 300×200 twips"
        );
        assert!(
            image.origin.x > Twip::ZERO,
            "the image follows the leading text on the same line"
        );
        assert!(
            image_line.runs.iter().any(|run| {
                let right = run.origin.x
                    + run
                        .glyphs
                        .iter()
                        .fold(Twip::ZERO, |advance, glyph| advance + glyph.advance);
                run.origin.x >= image.origin.x + image.size.width
                    || right > image.origin.x + image.size.width
            }),
            "the trailing text continues after the image instead of moving to a standalone line"
        );
        assert!(
            image_line.height >= image.size.height,
            "the image contributes to the shared line box height"
        );

        // And it composes to a `PaintItem::Image` carrying that extent-derived rect.
        let list = compose_paragraph(lines, Point::new(Twip::ZERO, Twip::ZERO));
        let rect = list
            .items
            .iter()
            .find_map(|item| match item {
                PaintItem::Image { media, rect, .. } if media == "word/media/image1.png" => {
                    Some(*rect)
                }
                _ => None,
            })
            .expect("an image paint item");
        assert_eq!(
            rect.size,
            Size::new(Twip(300), Twip(200)),
            "the paint rect carries the extent-derived size"
        );
    }

    #[test]
    fn a_cropped_inline_drawing_carries_its_crop_into_the_image_paint_item() {
        use crate::compose::compose_paragraph;
        use crate::display::PaintItem;
        use casual_doc_model::v1::{CropRect, Drawing, Extent, MediaId, MediaReference};

        let media_id = MediaId::new(NodeId::from_parts(70, 1).unwrap());
        let mut media = DefinitionMap::default();
        media.insert(
            media_id,
            MediaReference {
                relationship_id: "rId7".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/image1.png".to_owned(),
            },
        );
        let definitions = Definitions {
            media,
            ..Definitions::default()
        };
        let crop = CropRect {
            left: 10_000,
            top: 20_000,
            right: 5_000,
            bottom: 15_000,
        };
        // Two paragraphs with the same image: one cropped, one not — so the paint
        // items differ only by their crop, proving the crop (not the box) is what
        // the display list carries.
        let drawing = |id: u64, crop: Option<CropRect>| {
            BlockNode::Paragraph(Paragraph {
                id: NodeId::from_parts(id, 1).unwrap(),
                properties: ParagraphProperties::default(),
                inlines: vec![InlineNode::Drawing(Drawing {
                    id: NodeId::from_parts(id + 100, 1).unwrap(),
                    media: media_id,
                    extent: Some(Extent {
                        width_emu: 190_500,
                        height_emu: 127_000,
                    }),
                    descr: None,
                    crop,
                })],
            })
        };
        let doc = Document::new(
            NodeId::from_parts(1, 1).unwrap(),
            vec![drawing(10, Some(crop)), drawing(20, None)],
            definitions,
        )
        .unwrap();

        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let paint_crop = |fragment: &BlockFragment| {
            let BlockFragment::Paragraph { lines, .. } = fragment else {
                panic!("expected a paragraph fragment");
            };
            let list = compose_paragraph(lines, Point::new(Twip::ZERO, Twip::ZERO));
            list.items
                .iter()
                .find_map(|item| match item {
                    PaintItem::Image { media, rect, crop } if media == "word/media/image1.png" => {
                        Some((*rect, *crop))
                    }
                    _ => None,
                })
                .expect("an image paint item")
        };

        let (cropped_rect, cropped_crop) = paint_crop(&galley[0]);
        let (plain_rect, plain_crop) = paint_crop(&galley[1]);

        // The crop rides the paint item so the backend samples only the visible
        // source sub-rectangle; the uncropped twin carries no crop.
        assert_eq!(cropped_crop, Some(crop), "the crop reaches the paint item");
        assert_eq!(plain_crop, None, "an uncropped image carries no crop");
        // The display box (destination rect) is the same either way — `a:srcRect`
        // changes which source pixels fill the box, not the box itself.
        assert_eq!(
            cropped_rect.size,
            Size::new(Twip(300), Twip(200)),
            "cropping does not change the extent-derived display box"
        );
        assert_eq!(cropped_rect.size, plain_rect.size);
    }

    /// The first inline text box placed anywhere in a paragraph fragment's lines.
    fn text_box_of(fragment: &BlockFragment) -> &InlineTextBox {
        let BlockFragment::Paragraph { lines, .. } = fragment else {
            panic!("expected a paragraph fragment");
        };
        lines
            .lines
            .iter()
            .flat_map(|l| &l.text_boxes)
            .next()
            .expect("a text box was placed inline")
    }

    #[test]
    fn a_text_box_flows_its_paragraph_and_composes_text_inside_a_bordered_box() {
        use crate::compose::compose_paragraph;
        use crate::display::PaintItem;
        use casual_doc_model::v1::{Extent, Rgba, ShapeStroke, TextBox};

        // A paragraph whose only inline is an authored-size text box holding one
        // paragraph, fill, and a 30-twip outline.
        let text_box = InlineNode::TextBox(TextBox {
            id: NodeId::from_parts(20, 1).unwrap(),
            anchor: None,
            relative_height: None,
            extent: Some(Extent {
                width_emu: 1_270_000,
                height_emu: 635_000,
            }),
            fill: Some(Rgba {
                r: 1,
                g: 2,
                b: 3,
                a: 255,
            }),
            border: Some(ShapeStroke {
                color: Rgba {
                    r: 4,
                    g: 5,
                    b: 6,
                    a: 255,
                },
                width_emu: 19_050,
            }),
            body_properties: TextBoxBodyProperties::default(),
            blocks: vec![paragraph(
                21,
                vec![run_node(22, "boxed", RunProperties::default())],
            )],
        });
        let shaper = ParleyShaper::new();
        let galley = build_galley(
            &document(vec![paragraph(10, vec![text_box])]),
            &shaper,
            Twip::from_points(400),
        );

        // The box was placed as an inline box on its own line, carrying the flowed
        // block content of the box (the uniform-flow pipeline).
        let tb = text_box_of(&galley[0]);
        assert_eq!(tb.blocks.len(), 1, "the box flowed its one inner paragraph");
        let BlockFragment::Paragraph { lines: inner, .. } = &tb.blocks[0] else {
            panic!("the box's content is a paragraph fragment");
        };
        assert!(
            inner.lines.iter().any(|l| !l.runs.is_empty()),
            "the inner paragraph shaped glyphs"
        );
        assert_eq!(
            tb.size,
            Size::new(Twip(2_000), Twip(1_000)),
            "positive authored dimensions win over flow fallbacks"
        );
        assert_eq!(tb.fill, Some([1, 2, 3, 255]));
        assert_eq!(
            tb.border,
            Some(TextBoxStroke {
                color: [4, 5, 6, 255],
                width: Twip(30),
            })
        );

        // Composition paints the border (a stroked rect) and the inner glyphs.
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            unreachable!()
        };
        let list = compose_paragraph(lines, Point::new(Twip::ZERO, Twip::ZERO));
        assert!(
            list.items.iter().any(|i| matches!(
                i,
                PaintItem::Rect {
                    stroke: Some(_),
                    ..
                }
            )),
            "the box border paints as a stroked rect"
        );
        assert!(
            list.items
                .iter()
                .any(|i| matches!(i, PaintItem::Glyphs { .. })),
            "the box's text paints inside it"
        );
    }

    #[test]
    fn a_text_box_renders_a_nested_table_through_the_shared_pipeline() {
        use crate::compose::compose_paragraph;
        use crate::display::PaintItem;
        use casual_doc_model::v1::{
            GridColumn, Table, TableCell, TableCellProperties, TableProperties, TableRow,
            TableRowProperties, TextBox,
        };

        let cell = |id: u64, text: &str| TableCell {
            id: NodeId::from_parts(id, 1).unwrap(),
            properties: TableCellProperties::default(),
            blocks: vec![paragraph(
                id + 100,
                vec![run_node(id + 200, text, RunProperties::default())],
            )],
        };
        let table = BlockNode::Table(Table {
            id: NodeId::from_parts(50, 1).unwrap(),
            grid: vec![
                GridColumn {
                    width_twips: Some(2000),
                },
                GridColumn {
                    width_twips: Some(2000),
                },
            ],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![TableRow {
                id: NodeId::from_parts(51, 1).unwrap(),
                properties: TableRowProperties::default(),
                cells: vec![cell(60, "a"), cell(61, "b")],
            }],
        });
        let text_box = InlineNode::TextBox(TextBox {
            id: NodeId::from_parts(20, 1).unwrap(),
            anchor: None,
            relative_height: None,
            extent: None,
            fill: None,
            border: None,
            body_properties: TextBoxBodyProperties::default(),
            blocks: vec![table],
        });
        let shaper = ParleyShaper::new();
        let galley = build_galley(
            &document(vec![paragraph(10, vec![text_box])]),
            &shaper,
            Twip::from_points(400),
        );

        // The nested table expanded to a row fragment inside the box — exactly what
        // the body pipeline produces for a table.
        let tb = text_box_of(&galley[0]);
        assert!(
            matches!(tb.blocks[0], BlockFragment::TableRow { .. }),
            "the nested table flowed to a table-row fragment"
        );
        assert_eq!(
            tb.size.width,
            Twip::from_points(400),
            "a missing authored width falls back to the available flow width"
        );
        assert!(
            tb.border.is_none(),
            "layout must not fabricate an outline when the shape has none"
        );

        // Composition emits both cells' text inside the box.
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            unreachable!()
        };
        let list = compose_paragraph(lines, Point::new(Twip::ZERO, Twip::ZERO));
        let glyph_runs = list
            .items
            .iter()
            .filter(|i| matches!(i, PaintItem::Glyphs { .. }))
            .count();
        assert!(
            glyph_runs >= 2,
            "both nested cells' text paints inside the box"
        );
    }

    #[test]
    fn a_text_box_renders_an_inline_image_through_the_shared_pipeline() {
        use crate::compose::compose_paragraph;
        use crate::display::PaintItem;
        use casual_doc_model::v1::{Drawing, Extent, MediaId, MediaReference, TextBox};

        let media_id = MediaId::new(NodeId::from_parts(70, 1).unwrap());
        let mut media = DefinitionMap::default();
        media.insert(
            media_id,
            MediaReference {
                relationship_id: "rId7".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/image1.png".to_owned(),
            },
        );
        let definitions = Definitions {
            media,
            ..Definitions::default()
        };
        let inner_para = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(21, 1).unwrap(),
            properties: ParagraphProperties::default(),
            inlines: vec![InlineNode::Drawing(Drawing {
                id: NodeId::from_parts(22, 1).unwrap(),
                media: media_id,
                extent: Some(Extent {
                    width_emu: 190_500,
                    height_emu: 127_000,
                }),
                descr: None,
                crop: None,
            })],
        });
        let para = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(10, 1).unwrap(),
            properties: ParagraphProperties::default(),
            inlines: vec![InlineNode::TextBox(TextBox {
                id: NodeId::from_parts(20, 1).unwrap(),
                anchor: None,
                relative_height: None,
                extent: None,
                fill: None,
                border: None,
                body_properties: TextBoxBodyProperties::default(),
                blocks: vec![inner_para],
            })],
        });
        let doc =
            Document::new(NodeId::from_parts(1, 1).unwrap(), vec![para], definitions).unwrap();
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));

        // The image inside the box resolved to its package part name.
        let tb = text_box_of(&galley[0]);
        let BlockFragment::Paragraph { lines: inner, .. } = &tb.blocks[0] else {
            panic!("the box's content is a paragraph fragment");
        };
        let image = inner
            .lines
            .iter()
            .flat_map(|l| &l.images)
            .next()
            .expect("an inline image was placed inside the box");
        assert_eq!(image.media, "word/media/image1.png");

        // And it composes to an image paint item inside the box.
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            unreachable!()
        };
        let list = compose_paragraph(lines, Point::new(Twip::ZERO, Twip::ZERO));
        assert!(
            list.items.iter().any(|i| matches!(
                i,
                PaintItem::Image { media, .. } if media == "word/media/image1.png"
            )),
            "the box's image paints inside it"
        );
    }

    #[test]
    fn text_box_body_properties_drive_insets_alignment_autofit_scaling_and_clip() {
        use crate::compose::compose_paragraph;
        use crate::display::PaintItem;
        use casual_doc_model::v1::{
            Extent, TextBox, TextBoxAutoFit, TextBoxBodyProperties, TextBoxHorizontalOverflow,
            TextBoxInsets, TextBoxVerticalAnchor, TextBoxVerticalOverflow,
        };

        let inner = vec![paragraph(
            21,
            vec![run_node(22, "body properties", RunProperties::default())],
        )];
        let fixed = InlineNode::TextBox(TextBox {
            id: NodeId::from_parts(20, 1).unwrap(),
            anchor: None,
            relative_height: None,
            extent: Some(Extent {
                width_emu: 2_000 * 635,
                height_emu: 2_000 * 635,
            }),
            fill: None,
            border: None,
            body_properties: TextBoxBodyProperties {
                insets: TextBoxInsets {
                    left_emu: 200 * 635,
                    top_emu: 100 * 635,
                    right_emu: 300 * 635,
                    bottom_emu: 200 * 635,
                },
                vertical_anchor: TextBoxVerticalAnchor::Center,
                horizontal_overflow: TextBoxHorizontalOverflow::Clip,
                vertical_overflow: TextBoxVerticalOverflow::Clip,
                auto_fit: TextBoxAutoFit::None,
            },
            blocks: inner.clone(),
        });
        let shaper = ParleyShaper::new();
        let galley = build_galley(
            &document(vec![paragraph(10, vec![fixed])]),
            &shaper,
            Twip(4_000),
        );
        let text_box = text_box_of(&galley[0]);
        let content_height = text_box
            .blocks
            .iter()
            .map(BlockFragment::height)
            .fold(Twip::ZERO, |a, h| a + h);
        assert_eq!(text_box.size, Size::new(Twip(2_000), Twip(2_000)));
        assert_eq!(text_box.content_layout.origin.x, Twip(200));
        assert_eq!(
            text_box.content_layout.origin.y,
            Twip(100 + (1_700 - content_height.raw()).max(0) / 2),
            "center anchoring uses the height inside top/bottom insets"
        );
        assert!(text_box.content_layout.clip_horizontal);
        assert!(text_box.content_layout.clip_vertical);
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            unreachable!()
        };
        let list = compose_paragraph(lines, Point::new(Twip::ZERO, Twip::ZERO));
        assert!(list.items.iter().any(|item| matches!(
            item,
            PaintItem::PushClip(rect)
                if rect.origin == Point::new(Twip::ZERO, Twip::ZERO)
                    && rect.size == Size::new(Twip(2_000), Twip(2_000))
        )));
        assert!(
            list.items
                .iter()
                .any(|item| matches!(item, PaintItem::PopClip))
        );

        let base_document = document(vec![paragraph(
            900,
            vec![run_node(901, "host", RunProperties::default())],
        )]);
        let shape_fit = flow_anchored_text_box(
            &base_document,
            &inner,
            &shaper,
            Size::new(Twip(2_000), Twip(1)),
            &TextBoxBodyProperties {
                insets: TextBoxInsets {
                    left_emu: 0,
                    top_emu: 50 * 635,
                    right_emu: 0,
                    bottom_emu: 70 * 635,
                },
                auto_fit: TextBoxAutoFit::Shape,
                ..TextBoxBodyProperties::default()
            },
        );
        let shape_content_height = shape_fit
            .blocks
            .iter()
            .map(BlockFragment::height)
            .fold(Twip::ZERO, |a, h| a + h);
        assert_eq!(
            shape_fit.size.height,
            Twip(shape_content_height.raw() + 120),
            "shape autofit grows the authored height to content plus insets"
        );

        let normal = flow_anchored_text_box(
            &base_document,
            &inner,
            &shaper,
            Size::new(Twip(2_000), Twip(2_000)),
            &TextBoxBodyProperties {
                auto_fit: TextBoxAutoFit::Normal {
                    font_scale: 50_000,
                    line_spacing_reduction: 20_000,
                },
                ..TextBoxBodyProperties::default()
            },
        );
        let BlockFragment::Paragraph { lines, .. } = &normal.blocks[0] else {
            unreachable!()
        };
        let run = lines
            .lines
            .iter()
            .flat_map(|line| &line.runs)
            .next()
            .expect("normal autofit shapes the inner run");
        assert_eq!(run.size, Twip(110));
    }

    #[test]
    fn a_text_box_inside_a_table_cell_uses_the_same_body_properties_path() {
        use casual_doc_model::v1::{
            GridColumn, Table, TableCell, TableCellProperties, TableProperties, TableRow,
            TableRowProperties, TextBox, TextBoxBodyProperties, TextBoxInsets,
            TextBoxVerticalAnchor,
        };

        let text_box = InlineNode::TextBox(TextBox {
            id: NodeId::from_parts(30, 1).unwrap(),
            anchor: None,
            relative_height: None,
            extent: Some(Extent {
                width_emu: 1_000 * 635,
                height_emu: 900 * 635,
            }),
            fill: None,
            border: None,
            body_properties: TextBoxBodyProperties {
                insets: TextBoxInsets {
                    left_emu: 40 * 635,
                    top_emu: 50 * 635,
                    right_emu: 60 * 635,
                    bottom_emu: 70 * 635,
                },
                vertical_anchor: TextBoxVerticalAnchor::Bottom,
                ..TextBoxBodyProperties::default()
            },
            blocks: vec![paragraph(
                31,
                vec![run_node(32, "cell box", RunProperties::default())],
            )],
        });
        let table = BlockNode::Table(Table {
            id: NodeId::from_parts(40, 1).unwrap(),
            grid: vec![GridColumn {
                width_twips: Some(2_000),
            }],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![TableRow {
                id: NodeId::from_parts(41, 1).unwrap(),
                properties: TableRowProperties::default(),
                cells: vec![TableCell {
                    id: NodeId::from_parts(42, 1).unwrap(),
                    properties: TableCellProperties::default(),
                    blocks: vec![paragraph(43, vec![text_box])],
                }],
            }],
        });
        let shaper = ParleyShaper::new();
        let galley = build_galley(&document(vec![table]), &shaper, Twip(3_000));
        let BlockFragment::TableRow { cells, .. } = &galley[0] else {
            unreachable!()
        };
        let nested = text_box_of(&cells[0].blocks[0]);
        assert_eq!(nested.content_layout.origin.x, Twip(40));
        assert!(
            nested.content_layout.origin.y.raw() >= 50,
            "bottom anchoring is resolved inside a table cell, not body-only"
        );
    }

    #[test]
    fn cell_margins_resolve_by_precedence() {
        use casual_doc_model::v1::CellMargins;

        // Word's built-in default when neither cell nor table declares margins:
        // 108 twips left/right, 0 top/bottom.
        let none = resolve_cell_margins(&CellMargins::default(), &CellMargins::default());
        assert_eq!(none.start, Twip(108));
        assert_eq!(none.end, Twip(108));
        assert_eq!(none.top, Twip::ZERO);
        assert_eq!(none.bottom, Twip::ZERO);

        // The table default (`w:tblCellMar`) is used when the cell is silent.
        let table = CellMargins {
            top_twips: Some(20),
            start_twips: Some(200),
            bottom_twips: Some(30),
            end_twips: Some(200),
        };
        let from_table = resolve_cell_margins(&CellMargins::default(), &table);
        assert_eq!(from_table.start, Twip(200));
        assert_eq!(from_table.top, Twip(20));

        // The cell's own `w:tcMar` wins per edge, falling back to the table for the
        // edges it does not set.
        let cell = CellMargins {
            start_twips: Some(50),
            top_twips: Some(15),
            ..CellMargins::default()
        };
        let effective = resolve_cell_margins(&cell, &table);
        assert_eq!(effective.start, Twip(50), "cell start wins");
        assert_eq!(effective.top, Twip(15), "cell top wins");
        assert_eq!(effective.end, Twip(200), "end falls back to the table");
        assert_eq!(effective.bottom, Twip(30), "bottom falls back to the table");
    }

    #[test]
    fn cell_vertical_alignment_slack_placement() {
        use crate::block::{CellContentMargins, CellFragment, CellVerticalMerge};

        // A cell 100 twips tall of content, inset 10 top / 10 bottom, inside a row
        // 200 twips tall: 80 twips of slack above/below the content box.
        let margins = CellContentMargins {
            top: Twip(10),
            bottom: Twip(10),
            start: Twip(108),
            end: Twip(108),
        };
        let make = |valign: CellVAlign| CellFragment {
            id: NodeId::from_parts(9, 1).unwrap(),
            grid_span: 1,
            x: Twip::ZERO,
            width: Twip(3000),
            cell_spacing: Default::default(),
            // A single 100-twip-tall empty paragraph line stands in for content.
            blocks: vec![BlockFragment::Paragraph {
                id: NodeId::from_parts(10, 1).unwrap(),
                lines: crate::text::LineLayout {
                    lines: vec![crate::text::Line {
                        runs: Vec::new(),
                        ascent: Twip(100),
                        descent: Twip::ZERO,
                        height: Twip(100),
                        clip: false,
                        range: ModelRange::new(
                            ModelPos::new(NodeId::from_parts(10, 1).unwrap(), 0),
                            ModelPos::new(NodeId::from_parts(10, 1).unwrap(), 0),
                        ),
                        line_break: LineBreak::Wrap,
                        page_break_after: false,
                        bars: Vec::new(),
                        images: Vec::new(),
                        fields: Vec::new(),
                        notes: Vec::new(),
                        text_boxes: Vec::new(),
                        rules: Vec::new(),
                    }],
                },
                box_metrics: BoxMetrics::default(),
                break_control: BreakControl::default(),
                decor: ParagraphDecor::default(),
            }],
            margins,
            vertical_alignment: valign,
            vertical_merge: CellVerticalMerge::None,
            borders: CellBorders::default(),
            table_borders: CellBorders::default(),
            shading: None,
        };
        let row = Twip(200);
        assert_eq!(
            make(CellVAlign::Top).content_y_offset(row),
            Twip(10),
            "top: just the top margin"
        );
        // slack = 200 - (10 + 100 + 10) = 80.
        assert_eq!(
            make(CellVAlign::Center).content_y_offset(row),
            Twip(10 + 40),
            "center: top margin + half the slack"
        );
        assert_eq!(
            make(CellVAlign::Bottom).content_y_offset(row),
            Twip(10 + 80),
            "bottom: top margin + all the slack"
        );
        // Occupied height counts the top+bottom margins, so it drives row height.
        assert_eq!(make(CellVAlign::Top).occupied_height(), Twip(120));
    }

    /// Builds a one-abstract, one-instance numbering definition set: level 0 with
    /// the given format/text/rPr, plus a paragraph referencing it. Returns the
    /// document and the numbering instance id.
    fn numbered_document(
        num_fmt: casual_doc_model::v1::NumberFormat,
        lvl_text: &str,
        level_rpr: Option<RunProperties>,
    ) -> Document {
        use casual_doc_model::v1::{
            AbstractNumbering, AbstractNumberingId, NumberingInstance, NumberingInstanceId,
            NumberingLevel, NumberingRef,
        };
        let abs_id = AbstractNumberingId::new(NodeId::from_parts(900, 1).unwrap());
        let inst_id = NumberingInstanceId::new(NodeId::from_parts(901, 1).unwrap());
        let mut definitions = Definitions::default();
        definitions.abstract_numbering.insert(
            abs_id,
            AbstractNumbering {
                levels: vec![NumberingLevel {
                    level: 0,
                    start: 1,
                    num_fmt: Some(num_fmt),
                    lvl_text: Some(lvl_text.to_owned()),
                    lvl_jc: None,
                    suff: Some(casual_doc_model::v1::LevelSuffix::Space),
                    is_lgl: false,
                    paragraph_properties: None,
                    run_properties: level_rpr,
                    style_ref: None,
                }],
            },
        );
        definitions.numbering.insert(
            inst_id,
            NumberingInstance {
                abstract_ref: abs_id,
                overrides: Vec::new(),
            },
        );
        let para = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(10, 1).unwrap(),
            properties: ParagraphProperties {
                numbering: Some(NumberingRef {
                    instance: inst_id,
                    level: 0,
                }),
                ..ParagraphProperties::default()
            },
            inlines: vec![run_node(11, "Body", RunProperties::default())],
        });
        Document::new(NodeId::from_parts(1, 1).unwrap(), vec![para], definitions).unwrap()
    }

    #[test]
    fn a_numbered_paragraph_prepends_a_marker_glyph_run_using_the_level_rpr_size() {
        // The level renders its number at a distinctive 20pt (400 twips), while the
        // 11pt body run is 220 twips — so the marker run is identifiable by its size.
        let level_rpr = RunProperties {
            size_half_points: Some(40),
            ..RunProperties::default()
        };
        let doc = numbered_document(
            casual_doc_model::v1::NumberFormat::Decimal,
            "%1.",
            Some(level_rpr),
        );
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!("expected a paragraph fragment");
        };
        let first = &lines.lines[0];
        // The first run is the marker, shaped at the level's rPr size (400 twips),
        // distinct from the 220-twip body run — proving the marker used the level's
        // run properties, not the body's.
        assert!(
            first.runs.iter().any(|r| r.size == Twip(400)),
            "a marker glyph run at the level's 20pt size is present"
        );
        assert!(
            first.runs.iter().any(|r| r.size == Twip(220)),
            "the 11pt body run is also present"
        );
        // The marker run leads the line (its glyphs sit at or before the body run).
        let marker_x = first
            .runs
            .iter()
            .find(|r| r.size == Twip(400))
            .map(|r| r.origin.x)
            .unwrap();
        let body_x = first
            .runs
            .iter()
            .find(|r| r.size == Twip(220))
            .map(|r| r.origin.x)
            .unwrap();
        assert!(marker_x <= body_x, "the marker precedes the body text");
    }

    #[test]
    fn a_bullet_paragraph_renders_a_marker_and_a_plain_paragraph_does_not() {
        let shaper = ParleyShaper::new();
        // A bullet list item: its glyph is the marker, prepended to the line.
        let bullet_doc =
            numbered_document(casual_doc_model::v1::NumberFormat::Bullet, "\u{2022}", None);
        let galley = build_galley(&bullet_doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!("expected a paragraph fragment");
        };
        let bullet_runs = lines.lines[0].runs.len();

        // The same paragraph text with no numbering: no marker, so strictly fewer
        // glyph runs on the first line (a stray marker on a non-list paragraph would
        // be a regression).
        let plain_para = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(10, 1).unwrap(),
            properties: ParagraphProperties::default(),
            inlines: vec![run_node(11, "Body", RunProperties::default())],
        });
        let plain_doc = Document::new(
            NodeId::from_parts(1, 1).unwrap(),
            vec![plain_para],
            Definitions::default(),
        )
        .unwrap();
        let plain_galley = build_galley(&plain_doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines: plain, .. } = &plain_galley[0] else {
            panic!("expected a paragraph fragment");
        };
        assert!(
            bullet_runs > plain.lines[0].runs.len(),
            "the bullet item has a marker run the plain paragraph lacks"
        );
    }

    fn hr_node(id: u64, align: HorizontalRuleAlign, width_permille: u16) -> InlineNode {
        InlineNode::HorizontalRule(ModelHorizontalRule {
            id: NodeId::from_parts(id, 1).unwrap(),
            align,
            width_permille,
            thickness_emu: 30 * 635,
            color: Rgba {
                r: 0xA0,
                g: 0xA0,
                b: 0xA0,
                a: 255,
            },
        })
    }

    #[test]
    fn a_full_width_horizontal_rule_paints_a_content_width_line() {
        let shaper = ParleyShaper::new();
        let doc = document(vec![paragraph(
            10,
            vec![hr_node(11, HorizontalRuleAlign::Center, 1000)],
        )]);
        let width = Twip::from_points(400);
        let galley = build_galley(&doc, &shaper, width);
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!("expected a paragraph fragment");
        };
        assert_eq!(lines.lines.len(), 1, "the rule owns a single line");
        let line = &lines.lines[0];
        assert_eq!(line.rules.len(), 1);
        let rule = &line.rules[0];
        // Full width spans the whole content column, at the leading edge.
        assert_eq!(rule.origin.x, Twip::ZERO);
        assert_eq!(rule.size.width, width);
        // 1.5pt thick (30 twips), and the line reserves exactly that height.
        assert_eq!(rule.size.height, Twip(30));
        assert_eq!(line.height, rule.size.height);
        assert_eq!(rule.color, [0xA0, 0xA0, 0xA0, 255]);
    }

    #[test]
    fn hr_item_positions_a_partial_width_rule_by_alignment() {
        let width = Twip(8000);
        let make = |align| {
            let FlowItem::HorizontalRule(rule) = hr_item(&hr_model(align, 500), width) else {
                panic!("expected a horizontal-rule flow item");
            };
            rule
        };
        // 500 per-mille == half the content width, so 4000 twips of slack.
        let left = make(HorizontalRuleAlign::Left);
        assert_eq!(left.origin.x, Twip::ZERO);
        assert_eq!(left.size.width, Twip(4000));
        let center = make(HorizontalRuleAlign::Center);
        assert_eq!(center.origin.x, Twip(2000));
        let right = make(HorizontalRuleAlign::Right);
        assert_eq!(right.origin.x, Twip(4000));
    }

    #[test]
    fn framed_initial_is_unclipped_and_excludes_following_lines() {
        let drop_cap = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(70, 1).unwrap(),
            properties: ParagraphProperties {
                keep_next: true,
                spacing: Some(Spacing {
                    line_rule: Some(LineRule::Exact),
                    line_twips: Some(700),
                    ..Spacing::default()
                }),
                drop_cap_frame: Some(DropCapFrame {
                    mode: DropCapMode::Drop,
                    lines: 3,
                    wrap: Some(FrameWrap::Around),
                    horizontal_anchor: None,
                    vertical_anchor: None,
                    horizontal_alignment: None,
                    vertical_alignment: None,
                    horizontal_position_twips: None,
                    vertical_position_twips: None,
                    horizontal_space_twips: Some(80),
                    vertical_space_twips: None,
                }),
                ..ParagraphProperties::default()
            },
            inlines: vec![run_node(
                71,
                "D",
                RunProperties {
                    size_half_points: Some(117),
                    ..RunProperties::default()
                },
            )],
        });
        let body = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(72, 1).unwrap(),
            properties: ParagraphProperties::default(),
            inlines: vec![run_node(
                73,
                &"rop cap body text wraps beside the initial ".repeat(40),
                RunProperties::default(),
            )],
        });
        let document = document(vec![drop_cap, body]);
        let shaper = ParleyShaper::new();
        let width = Twip(5_000);
        let galley = build_galley(&document, &shaper, width);
        let BlockFragment::Paragraph { lines: initial, .. } = &galley[0] else {
            panic!("drop cap paragraph");
        };
        assert_eq!(galley[0].height(), Twip::ZERO);
        assert_eq!(initial.lines.len(), 1);
        assert!(!initial.lines[0].clip, "the large initial is never clipped");
        assert_eq!(initial.lines[0].height, Twip::ZERO);
        assert!(initial.lines[0].runs[0].size >= Twip(1_170));

        let BlockFragment::Paragraph { lines: body, .. } = &galley[1] else {
            panic!("following paragraph");
        };
        let shifted = body
            .lines
            .iter()
            .take_while(|line| {
                line.runs
                    .first()
                    .is_some_and(|run| run.origin.x > Twip::ZERO)
            })
            .count();
        assert!(
            shifted >= 3,
            "the initial occupies at least three body lines"
        );
        assert!(
            body.lines.iter().skip(shifted).any(|line| line
                .runs
                .first()
                .is_some_and(|run| run.origin.x == Twip::ZERO)),
            "the paragraph returns to its full measure below the initial"
        );

        let mut cache = GalleyCache::new();
        assert_eq!(
            build_galley_cached(
                &document,
                &shaper,
                width,
                &mut cache,
                &DirtySet::everything()
            ),
            galley,
            "cached layout uses the coupled drop-cap flow path"
        );
    }

    #[test]
    fn unsupported_frame_wrap_does_not_acquire_drop_cap_layout() {
        let framed = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(74, 1).unwrap(),
            properties: ParagraphProperties {
                drop_cap_frame: Some(DropCapFrame {
                    mode: DropCapMode::Drop,
                    lines: 3,
                    wrap: Some(FrameWrap::None),
                    horizontal_anchor: None,
                    vertical_anchor: None,
                    horizontal_alignment: None,
                    vertical_alignment: None,
                    horizontal_position_twips: None,
                    vertical_position_twips: None,
                    horizontal_space_twips: None,
                    vertical_space_twips: None,
                }),
                ..ParagraphProperties::default()
            },
            inlines: vec![run_node(75, "D", RunProperties::default())],
        });
        let body = paragraph(
            76,
            vec![run_node(77, "ordinary body", RunProperties::default())],
        );
        let galley = build_galley(
            &document(vec![framed, body]),
            &ParleyShaper::new(),
            Twip(5_000),
        );
        assert!(galley[0].height() > Twip::ZERO);
        let BlockFragment::Paragraph { lines, .. } = &galley[1] else {
            panic!("body paragraph");
        };
        assert_eq!(lines.lines[0].runs[0].origin.x, Twip::ZERO);
    }

    #[test]
    fn a_margin_right_square_anchor_becomes_a_local_line_exclusion() {
        use casual_doc_model::v1::{AnchorHorizontal, AnchorVertical, WrapDistances};

        let anchor = DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: HorizontalAnchor::Margin,
                position: HorizontalPosition::Align(HorizontalAlign::Right),
            },
            vertical: AnchorVertical {
                relative_from: VerticalAnchor::Paragraph,
                position: VerticalPosition::Offset(0),
            },
            wrap: WrapMode::Square,
            wrap_distances: WrapDistances {
                start_emu: 180 * 635,
                ..WrapDistances::default()
            },
            behind_doc: false,
        };
        let item = float_flow_item(
            &anchor,
            &Extent {
                width_emu: 1500 * 635,
                height_emu: 1500 * 635,
            },
        )
        .expect("supported paragraph-local square wrap");
        assert!(matches!(
            item,
            FlowItem::FloatExclusion {
                side: InlineFloatSide::Right,
                width: Twip(1680),
                height: Twip(1500),
            }
        ));
    }

    fn hr_model(align: HorizontalRuleAlign, width_permille: u16) -> ModelHorizontalRule {
        ModelHorizontalRule {
            id: NodeId::from_parts(11, 1).unwrap(),
            align,
            width_permille,
            thickness_emu: 30 * 635,
            color: Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        }
    }
}
