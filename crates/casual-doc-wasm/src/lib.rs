//! `casual-doc-wasm` — the `wasm-bindgen` façade the browser/webview viewer drives.
//!
//! This crate is a **bridge, not an engine** (doc 57 §1): it exposes the existing
//! Rust pipeline across the JS boundary. One handle, [`WasmDocument`], owns an
//! imported document, its [`PaginatedLayout`], and the shaper whose font registry
//! served that pagination — so rendering serves the exact faces layout shaped.
//!
//! P1G-001 surface (doc 57 §4.1–4.3): [`open`], [`WasmDocument::page_count`],
//! [`WasmDocument::page_size`], [`WasmDocument::render_page`]. The text layer,
//! hit-testing, and editing methods (§4.4–4.6) land in later milestones.
//!
//! Units are twips on the model side and device pixels only at the raster
//! boundary (doc 57 §3): `render_page(i, dpi)` rasterizes at `dpi`, where
//! `device_px = twip / 1440 * dpi`.

use casual_doc_edit::{
    FormatDelta, Operation, Pos, Range as EditRange, apply as apply_edit, caret_run_properties,
    cell_properties, find_paragraph, find_table, locate_cell, locate_table_cell, locate_table_row,
    paragraph_properties, run_properties_in_range,
};
use casual_doc_export::write_document;
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::cascade::{StyleCascade, requested_font_family};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::{
    document_page_config, paginate_document, paginate_document_cached,
};
use casual_doc_layout::flow::node_plain_text;
use casual_doc_layout::hittest::{Direction, HitZone, LayoutSnapshot};
use casual_doc_layout::incremental::{DirtySet, GalleyCache};
use casual_doc_layout::model::{ModelPos, ModelRange};
use casual_doc_layout::page::{Page, PaginatedLayout};
use casual_doc_layout::paginate::PageConfig;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::{Point, Rect, Size, Twip};
use casual_doc_model::v1::GridColumn;
use casual_doc_model::v1::{
    AbstractNumbering, AbstractNumberingId, Alignment, BlockNode, BookmarkId, BorderEdge,
    CellMargins, CellVerticalAlignment, Color, Document, ExternalTarget, FontName, FontRef,
    HeightRule, HighlightColor, Hyperlink, HyperlinkTarget, Indentation, InlineNode,
    InternalTarget, LevelJustification, LevelSuffix, NumberFormat, NumberingInstance,
    NumberingInstanceId, NumberingLevel, NumberingOverride, NumberingRef, PageMargins,
    PageOrientation, PageSize as SectionPageSize, Paragraph, ParagraphBorders, ParagraphProperties,
    RgbColor, RowHeight, Run, RunProperties, SectionId, Spacing, StyleId, StyleKind, TabAlignment,
    TabStop, Table, TableBorders, TableCell, TableCellProperties, TableLayout, TableProperties,
    TableRow, VerticalAlignment, VerticalMerge,
};
use casual_doc_model::{IdGenerator, NodeId};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_render::{MediaSource, RegistryFontSource, Surface, render};
use std::collections::BTreeMap;
use std::str::FromStr;
use wasm_bindgen::Clamped;
use wasm_bindgen::prelude::*;

/// Package admission limits for the viewer (64 MiB input, 256 MiB expanded) —
/// the same envelope the native `render_gallery` probe uses, so a document that
/// opens there opens here. All other bounds keep their [`PackageLimits::default`]
/// values.
fn viewer_limits() -> PackageLimits {
    PackageLimits {
        max_input_bytes: 64 * 1024 * 1024,
        max_total_expanded_bytes: 256 * 1024 * 1024,
        max_single_expanded_bytes: 64 * 1024 * 1024,
        ..PackageLimits::default()
    }
}

/// Host font batch admission: enough for several variable families while
/// preventing an accidental/untrusted JS caller from cloning an unbounded blob
/// into the shaper.
const MAX_HOST_FONT_FACES: usize = 16;
const MAX_HOST_FONT_BYTES: usize = 32 * 1024 * 1024;

/// An open document: the imported model, its current pagination, and the shaper
/// (and its font registry) that produced it, plus the media bytes rendering needs.
///
/// Held together because rendering must serve the faces the shaper actually
/// resolved during pagination — [`ParleyShaper::registry`] is only complete after
/// [`paginate_document`] has shaped every paragraph.
#[wasm_bindgen]
pub struct WasmDocument {
    document: Document,
    layout: PaginatedLayout,
    shaper: ParleyShaper,
    /// Inline-image bytes by package part name — served to the renderer (via
    /// [`BorrowedMedia`]) and to the semantic writer on export.
    media: BTreeMap<String, Vec<u8>>,
    /// The first-section page geometry — the fallback when a page's section id
    /// resolves to no boundary (a document with no `w:sectPr` falls back to
    /// US-Letter here, exactly as [`document_page_config`] defines).
    default_config: PageConfig,
    /// Mints run identities for edits, in a namespace distinct from the imported
    /// ids so new runs never collide with existing nodes.
    edit_ids: IdGenerator,
    /// Undo stack: each entry is one user action's inverse ops, stored in the
    /// order they must be re-applied to undo the action (reverse of how the
    /// forward ops ran), so a multi-op action (cross-paragraph delete, type-over)
    /// undoes in a single step.
    undo: Vec<HistoryEntry>,
    /// Redo stack — forward-op groups of undone actions; cleared on a fresh edit.
    redo: Vec<HistoryEntry>,
    /// The last incremental typing action eligible to absorb another adjacent
    /// keystroke. The host owns gesture boundaries through `session`; the engine
    /// additionally requires exact caret continuity before coalescing history.
    typing_history: Option<TypingHistory>,
    /// Monotonic model revision, bumped on every applied edit.
    revision: u32,
    /// Shaped-paragraph cache for the incremental edit path: an edit re-shapes only
    /// the paragraph(s) it touched (hash-based invalidation) and reuses the rest, so
    /// re-pagination is `O(edit)` rather than `O(document)`. Cleared whenever a font
    /// registration changes face resolution (see [`WasmDocument::repaginate`]).
    galley_cache: GalleyCache,
    /// The shared bullet / numbered list definitions this editor created, if any —
    /// allocated lazily on the first list toggle and reused for every later one, so
    /// the document grows at most one abstract+instance per list kind.
    bullet_list: Option<NumberingInstanceId>,
    numbered_list: Option<NumberingInstanceId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TypingHistory {
    session: u32,
    caret: Pos,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HistoryKind {
    Edit,
    Typing,
    Paste,
    Delete,
    Replace,
    ParagraphBreak,
    Formatting,
    ParagraphFormatting,
    ListFormatting,
    LinkChange,
    TableResize,
    TableFormatting,
    TableStructure,
    DocumentProperties,
    PageSetup,
}

impl HistoryKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Edit => "Edit",
            Self::Typing => "Typing",
            Self::Paste => "Paste",
            Self::Delete => "Delete",
            Self::Replace => "Replace",
            Self::ParagraphBreak => "Paragraph break",
            Self::Formatting => "Formatting",
            Self::ParagraphFormatting => "Paragraph formatting",
            Self::ListFormatting => "List formatting",
            Self::LinkChange => "Link change",
            Self::TableResize => "Table resize",
            Self::TableFormatting => "Table formatting",
            Self::TableStructure => "Table structure",
            Self::DocumentProperties => "Document properties",
            Self::PageSetup => "Page setup",
        }
    }
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    kind: HistoryKind,
    operations: Vec<Operation>,
}

fn history_kind_for_ops(operations: &[Operation]) -> HistoryKind {
    let Some(first) = operations.first() else {
        return HistoryKind::Edit;
    };
    match first {
        Operation::InsertText { .. } => HistoryKind::Typing,
        Operation::DeleteText { .. } => HistoryKind::Delete,
        Operation::SplitParagraph { .. } | Operation::JoinParagraphs { .. } => {
            HistoryKind::ParagraphBreak
        }
        Operation::FormatText { .. }
        | Operation::ClearFormatting { .. }
        | Operation::SetInlines { .. } => HistoryKind::Formatting,
        Operation::SetHyperlink { .. } => HistoryKind::LinkChange,
        Operation::SetParagraphProperties { .. } => HistoryKind::ParagraphFormatting,
        Operation::InsertRow { .. }
        | Operation::DeleteRow { .. }
        | Operation::InsertColumn { .. }
        | Operation::DeleteColumn { .. }
        | Operation::DeleteTable { .. }
        | Operation::InsertTable { .. } => HistoryKind::TableStructure,
        Operation::SetTableCellProperties { .. }
        | Operation::SetTableProperties { .. }
        | Operation::ReplaceTable { .. } => HistoryKind::TableFormatting,
        Operation::SetCoreProperties { .. } => HistoryKind::DocumentProperties,
        Operation::SetSectionGeometry { .. } => HistoryKind::PageSetup,
    }
}

impl core::fmt::Debug for WasmDocument {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `ParleyShaper` is opaque; report the shape a
        // caller can act on without dumping the whole model.
        f.debug_struct("WasmDocument")
            .field("pages", &self.layout.page_count())
            .field("media_parts", &self.document.definitions().media.len())
            .finish_non_exhaustive()
    }
}

/// Imports a `.docx` and paginates it. Returns a handle over the open document.
///
/// Errors (bad ZIP, admission failure, import failure) throw a JS `Error`.
///
// TODO(P1G-003+): surface the structured `SdkError` `code`/`severity` and the
// import compatibility report on the thrown error, per doc 57 §5.5, so the host
// can show "opened with N unsupported constructs" instead of only a message.
#[wasm_bindgen]
pub fn open(bytes: &[u8]) -> Result<WasmDocument, JsValue> {
    open_document(bytes).map_err(to_js)
}

#[wasm_bindgen]
impl WasmDocument {
    /// The number of laid-out pages.
    #[wasm_bindgen(getter, js_name = pageCount)]
    #[must_use]
    pub fn page_count(&self) -> u32 {
        // A paginated document never exceeds u32 pages within the admission
        // limits; the cast is saturating for defensiveness.
        u32::try_from(self.layout.page_count()).unwrap_or(u32::MAX)
    }

    /// Whether the document has at least one user action available to undo.
    #[wasm_bindgen(getter, js_name = canUndo)]
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether the document has at least one undone action available to redo.
    #[wasm_bindgen(getter, js_name = canRedo)]
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// User-facing label for the next undoable action, or an empty string when
    /// history is empty. The engine owns this vocabulary so the host never has to
    /// reverse-engineer a command from its inverse operations.
    #[wasm_bindgen(getter, js_name = undoLabel)]
    #[must_use]
    pub fn undo_label(&self) -> String {
        self.undo
            .last()
            .map_or_else(String::new, |entry| entry.kind.label().to_owned())
    }

    /// User-facing label for the next redoable action, or an empty string when
    /// the redo stack is empty.
    #[wasm_bindgen(getter, js_name = redoLabel)]
    #[must_use]
    pub fn redo_label(&self) -> String {
        self.redo
            .last()
            .map_or_else(String::new, |entry| entry.kind.label().to_owned())
    }

    /// The page box size of page `index` (0-based), in twips, resolved against
    /// that page's own section (`w:sectPr`) so multi-section documents report
    /// per-section geometry.
    ///
    /// Throws if `index` is out of range.
    #[wasm_bindgen(js_name = pageSize)]
    pub fn page_size(&self, index: u32) -> Result<PageSize, JsValue> {
        self.page_size_inner(index).map_err(to_js)
    }

    /// Rasterizes page `index` (0-based) at `dpi` device pixels per inch and
    /// returns raw premultiplied RGBA8888 the frontend blits via `putImageData`.
    ///
    /// Throws if `index` is out of range or the surface size is invalid.
    #[wasm_bindgen(js_name = renderPage)]
    pub fn render_page(&self, index: u32, dpi: f32) -> Result<PageBitmap, JsValue> {
        self.render_page_inner(index, dpi).map_err(to_js)
    }

    /// The code points the last pagination could **not** cover with any available
    /// face — i.e. what renders as `.notdef` tofu (▯). Returned as `u32` scalar
    /// values. A host queries this to decide which fallback fonts to fetch
    /// (typically CJK / complex scripts absent from the bundled Latin faces).
    #[wasm_bindgen(js_name = missingCoverage)]
    #[must_use]
    pub fn missing_coverage(&self) -> Vec<u32> {
        self.shaper
            .registry()
            .missing_coverage()
            .into_iter()
            .map(|c| c as u32)
            .collect()
    }

    /// Registers a host-provided font (e.g. a network-fetched Noto face) as a
    /// **coverage fallback** for the given ISO-15924 scripts (`"Hani"`, `"Hira"`,
    /// `"Kana"`, `"Hang"`, …), then re-paginates so runs the bundled faces miss now
    /// shape and render with it. This is the browser half of the font-provisioning
    /// strategy — the single host-populatable seam.
    ///
    /// `scripts` may be empty to register the face without wiring script fallback
    /// (see [`WasmDocument::register_font`]).
    #[wasm_bindgen(js_name = registerFallbackFont)]
    pub fn register_fallback_font(&mut self, bytes: &[u8], scripts: Vec<String>) {
        let refs: Vec<&str> = scripts.iter().map(String::as_str).collect();
        self.shaper.register_fallback_font(bytes.to_vec(), &refs);
        self.repaginate();
    }

    /// Registers a host-provided font by family (no script-fallback wiring), then
    /// re-paginates. Use when a document names a face the host can supply directly;
    /// for CJK / complex-script coverage prefer
    /// [`register_fallback_font`](Self::register_fallback_font).
    #[wasm_bindgen(js_name = registerFont)]
    pub fn register_font(&mut self, bytes: &[u8]) {
        self.shaper.register_font(bytes.to_vec());
        self.repaginate();
    }

    /// Registers a bounded batch of host-provided named-family fonts and
    /// re-paginates exactly once. `bytes` is the concatenation of every font blob;
    /// `lengths` gives each blob's byte length in the same order.
    ///
    /// The packed form keeps the JS↔WASM ABI simple and avoids one complete
    /// document reflow per face when the browser provisions the Roboto/Noto
    /// families before first paint.
    #[wasm_bindgen(js_name = registerFonts)]
    pub fn register_fonts(&mut self, bytes: &[u8], lengths: Vec<u32>) -> Result<(), JsValue> {
        self.register_fonts_inner(bytes, &lengths).map_err(to_js)
    }

    // ---- Selection & copy (P1G-003) ----------------------------------------
    //
    // The interaction pipeline of doc 58: a page-local point resolves to a model
    // anchor (`hitTest`); anchors form a `Selection` the frontend draws from
    // engine geometry (`caretRect`/`selectionRects`) so the highlight matches the
    // raster exactly; `copyText` is the first read-only action over a selection.
    // All positions are `NodeId` (32-hex string) + node-relative UTF-8 byte
    // offset — the layout anchor space (doc 58 §3).

    /// Resolves a page-local point (twips) on 1-based `page` to the nearest caret
    /// anchor. Returns `undefined` if the page has no lines. `zone` is `"content"`
    /// (inside a line) or `"outside"` (snapped in from a margin).
    #[wasm_bindgen(js_name = hitTest)]
    #[must_use]
    pub fn hit_test(&self, page: u32, x_twip: i32, y_twip: i32) -> Option<HitPayload> {
        let snapshot = LayoutSnapshot::new(&self.layout);
        let hit = snapshot.hit_test(page, Point::new(Twip(x_twip), Twip(y_twip)))?;
        Some(HitPayload {
            node: hit.pos.node.to_string(),
            offset: hit.pos.offset,
            zone: match hit.zone {
                HitZone::Content => "content",
                HitZone::Outside => "outside",
            },
        })
    }

    /// Resolves a direct content click to the hyperlink painted at that point.
    /// External targets are returned verbatim for the host to policy-check before
    /// opening. Internal targets additionally resolve their bookmark marker to a
    /// model caret and 1-based target page, enabling TOC/page navigation without
    /// making the runtime own browser navigation policy.
    #[wasm_bindgen(js_name = linkAt)]
    #[must_use]
    pub fn link_at(&self, page: u32, x_twip: i32, y_twip: i32) -> Option<LinkHit> {
        let point = Point::new(Twip(x_twip), Twip(y_twip));
        let snapshot = LayoutSnapshot::new(&self.layout);
        let hit = snapshot.hit_test(page, point)?;
        if hit.zone != HitZone::Content {
            return None;
        }
        let paragraph = find_paragraph(self.document.body(), hit.pos.node)?;
        let links = paragraph_links(&self.document, paragraph);
        let link = links.into_iter().find(|candidate| {
            snapshot
                .selection_rects(candidate.range)
                .into_iter()
                .any(|(candidate_page, rect)| candidate_page == page && rect.contains(point))
        })?;

        let (kind, url, anchor, target) = match &link.link.target {
            HyperlinkTarget::External(external) => {
                ("external", external.url.clone(), String::new(), None)
            }
            HyperlinkTarget::Internal(internal) => (
                "internal",
                String::new(),
                internal.anchor.clone(),
                resolve_bookmark(&self.document, &internal.anchor),
            ),
        };
        let (target_node, target_offset, target_page) = target.map_or_else(
            || (String::new(), 0, 0),
            |pos| {
                let target_page = snapshot.caret_rect(pos).map_or(0, |(page, _)| page);
                (pos.node.to_string(), pos.offset, target_page)
            },
        );
        Some(LinkHit {
            kind,
            url,
            anchor,
            tooltip: link.link.tooltip.clone().unwrap_or_default(),
            start_node: link.range.start.node.to_string(),
            start_offset: link.range.start.offset,
            end_node: link.range.end.node.to_string(),
            end_offset: link.range.end.offset,
            target_node,
            target_offset,
            target_page,
        })
    }

    /// The caret box for a model anchor, as `[page, xTwip, yTwip, wTwip, hTwip]`
    /// (page-local; `w` is 0). Empty if the anchor resolves to no line (e.g. a
    /// stale position) or `node` is malformed.
    #[wasm_bindgen(js_name = caretRect)]
    #[must_use]
    pub fn caret_rect(&self, node: &str, offset: u32) -> Vec<i32> {
        let Some(pos) = parse_pos(node, offset) else {
            return Vec::new();
        };
        LayoutSnapshot::new(&self.layout)
            .caret_rect(pos)
            .map(|(page, rect)| flat_rect(page, rect).to_vec())
            .unwrap_or_default()
    }

    /// Highlight rectangles for a selection, flattened as `[page, x, y, w, h, …]`
    /// (page-local twips), one 5-tuple per covered line-fragment. The range may
    /// span nodes and pages and be given in either direction (a backward/upward
    /// drag) — the endpoints are ordered in document order first.
    #[wasm_bindgen(js_name = selectionRects)]
    #[must_use]
    pub fn selection_rects(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> Vec<i32> {
        let Ok((start, end)) = self.order_endpoints(start_node, start_offset, end_node, end_offset)
        else {
            return Vec::new();
        };
        let range = ModelRange::new(
            ModelPos::new(start.node, start.offset),
            ModelPos::new(end.node, end.offset),
        );
        let mut out = Vec::new();
        for (page, rect) in LayoutSnapshot::new(&self.layout).selection_rects(range) {
            out.extend_from_slice(&flat_rect(page, rect));
        }
        out
    }

    /// The plain text a selection covers — the first (read-only) action of the
    /// interaction pipeline (doc 58 §4). Walks the model between the two anchors
    /// in document order, slicing each node's shaped text at the byte offsets and
    /// joining paragraphs with `\n`. Empty if either anchor is unknown.
    ///
    // Byte-exact for `Run`/`Tab`/wrapper content (the common case). Documents whose
    // paragraphs contain fields/symbols/inline objects that contribute shaped
    // glyphs may drift; hardening to a shaping-time extractor is a follow-up.
    #[wasm_bindgen(js_name = copyText)]
    #[must_use]
    pub fn copy_text(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> String {
        let Some(range) = parse_range(start_node, start_offset, end_node, end_offset) else {
            return String::new();
        };
        self.copy_text_inner(range)
    }

    /// The selection as a rich-run clipboard fragment, serialized as JSON (the
    /// internal payload doc 67's "rich clipboard" row calls for): paragraph
    /// breaks, run formatting (bold/italic/underline/strike/size/color/
    /// highlight/vertAlign/font), and hyperlink targets. Scoped to paragraph
    /// text — a `Table` block or any exotic inline (field, content control,
    /// drawing, …) intersecting the range falls back to its existing plain-text
    /// extraction, unstyled, so nothing is silently dropped, only unstyled.
    /// Returns `"[]"` if either anchor is unknown.
    #[wasm_bindgen(js_name = copyRichRuns)]
    #[must_use]
    pub fn copy_rich_runs(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> String {
        let Some(range) = parse_range(start_node, start_offset, end_node, end_offset) else {
            return "[]".to_string();
        };
        serde_json::to_string(&self.copy_rich_runs_inner(range))
            .unwrap_or_else(|_| "[]".to_string())
    }

    /// Replaces the selection (if any) with a rich-run clipboard fragment — the
    /// paste counterpart of [`copy_rich_runs`](Self::copy_rich_runs). Builds one
    /// `InsertText`/`FormatText`/`SetHyperlink`/`SplitParagraph` op per run or
    /// paragraph-break marker and commits them as a single undoable action (the
    /// same atomic multi-op batching `replaceSelection` and `insertStyledText`
    /// already use). `runs_json` must be a JSON array in the shape
    /// `copyRichRuns` produces.
    #[wasm_bindgen(js_name = pasteRichRuns)]
    pub fn paste_rich_runs(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        runs_json: String,
    ) -> Result<EditResult, JsValue> {
        let runs: Vec<ClipboardRun> = serde_json::from_str(&runs_json)
            .map_err(|e| to_js(format!("invalid clipboard payload: {e}")))?;
        let (start, _end) = self
            .order_endpoints(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let mut ops = self
            .selection_delete_ops(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;

        let mut cursor = start;
        for run in &runs {
            if run.paragraph_break {
                let new_id = self
                    .edit_ids
                    .next_id()
                    .map_err(|_| to_js("id space exhausted".into()))?;
                ops.push(Operation::SplitParagraph { at: cursor, new_id });
                cursor = Pos::new(new_id, 0);
                continue;
            }
            if run.text.is_empty() {
                continue;
            }
            let insert_start = cursor;
            let insert_end = Pos::new(cursor.node, cursor.offset + run.text.len() as u32);
            ops.push(Operation::InsertText {
                at: insert_start,
                text: run.text.clone(),
            });
            let delta = FormatDelta {
                bold: run.bold,
                italic: run.italic,
                underline: run.underline,
                strike: run.strike,
                color: run.color.as_deref().and_then(parse_hex_color),
                highlight: run.highlight.as_deref().map(parse_highlight),
                size_half_points: run.size_half_points,
                vertical_alignment: run.vert_align.as_deref().map(|v| match v {
                    "super" => VerticalAlignment::Superscript,
                    "sub" => VerticalAlignment::Subscript,
                    _ => VerticalAlignment::Baseline,
                }),
                font: run.font.clone(),
            };
            if delta != FormatDelta::default() {
                ops.push(Operation::FormatText {
                    range: EditRange {
                        start: insert_start,
                        end: insert_end,
                    },
                    delta,
                });
            }
            if let Some(href) = &run.href {
                let id = self
                    .edit_ids
                    .next_id()
                    .map_err(|_| to_js("id space exhausted".into()))?;
                let target = if let Some(anchor) = href.strip_prefix('#') {
                    HyperlinkTarget::Internal(InternalTarget {
                        anchor: anchor.to_owned(),
                    })
                } else {
                    HyperlinkTarget::External(ExternalTarget { url: href.clone() })
                };
                ops.push(Operation::SetHyperlink {
                    range: EditRange {
                        start: insert_start,
                        end: insert_end,
                    },
                    id,
                    target: Some(target),
                    tooltip: None,
                });
            }
            cursor = insert_end;
        }
        if ops.is_empty() {
            return Err(to_js("nothing to paste".into()));
        }
        self.apply_action_caret_as(ops, cursor, HistoryKind::Paste)
            .map_err(to_js)
    }

    /// Finds the next/previous literal text match from a model position. Matches
    /// are constrained to one text-bearing node; cross-paragraph phrases remain a
    /// later text-layer search increment.
    #[wasm_bindgen(js_name = findText)]
    #[must_use]
    pub fn find_text(
        &self,
        query: &str,
        start_node: &str,
        start_offset: u32,
        forward: bool,
        match_case: bool,
    ) -> TextMatch {
        if query.is_empty() {
            return TextMatch::none();
        }
        let mut nodes = Vec::new();
        collect_block_text(self.document.body(), &mut nodes);
        if nodes.is_empty() {
            return TextMatch::none();
        }
        let start_idx = NodeId::from_str(start_node)
            .ok()
            .and_then(|nid| nodes.iter().position(|(id, _)| *id == nid))
            .unwrap_or(0);
        let start_offset = start_offset as usize;
        find_text_match(&nodes, query, start_idx, start_offset, forward, match_case)
            .unwrap_or_else(TextMatch::none)
    }

    // ---- Editing (P1G-006) — the mutating side of the doc 58 pipeline ---------
    //
    // Edits enter ONLY through these semantic methods (I1); JS never constructs an
    // `Operation`. Each applies a closed-set op on `v1::Document` (I2), records its
    // inverse for undo, re-paginates, and returns where the caret should land.

    /// Inserts `text` at a caret anchor. Returns the new caret + revision.
    #[wasm_bindgen(js_name = insertText)]
    pub fn insert_text(
        &mut self,
        node: &str,
        offset: u32,
        text: String,
    ) -> Result<EditResult, JsValue> {
        let node = node_id(node)?;
        self.apply(Operation::InsertText {
            at: Pos::new(node, offset),
            text,
        })
        .map_err(to_js)
    }

    /// Deletes the text range `[start, end)` within one paragraph.
    #[wasm_bindgen(js_name = deleteRange)]
    pub fn delete_range(
        &mut self,
        node: &str,
        start: u32,
        end: u32,
    ) -> Result<EditResult, JsValue> {
        let node = node_id(node)?;
        self.apply(Operation::DeleteText {
            range: EditRange {
                start: Pos::new(node, start),
                end: Pos::new(node, end),
            },
        })
        .map_err(to_js)
    }

    /// Creates or updates a hyperlink over a non-empty same-paragraph selection.
    /// A target beginning with `#` is an internal bookmark anchor; all other
    /// targets are retained as external URLs. Activation policy remains host-owned.
    #[wasm_bindgen(js_name = setHyperlink)]
    pub fn set_hyperlink(
        &mut self,
        node: &str,
        start: u32,
        end: u32,
        target: String,
        tooltip: Option<String>,
    ) -> Result<EditResult, JsValue> {
        let node = node_id(node)?;
        let target = if let Some(anchor) = target.strip_prefix('#') {
            HyperlinkTarget::Internal(InternalTarget {
                anchor: anchor.to_owned(),
            })
        } else {
            HyperlinkTarget::External(ExternalTarget { url: target })
        };
        let id = self
            .edit_ids
            .next_id()
            .map_err(|_| to_js("id space exhausted".into()))?;
        self.apply(Operation::SetHyperlink {
            range: EditRange {
                start: Pos::new(node, start),
                end: Pos::new(node, end),
            },
            id,
            target: Some(target),
            tooltip,
        })
        .map_err(to_js)
    }

    /// Removes the hyperlink wrapper occupying exactly the supplied selection,
    /// preserving its linked text and formatting.
    #[wasm_bindgen(js_name = removeHyperlink)]
    pub fn remove_hyperlink(
        &mut self,
        node: &str,
        start: u32,
        end: u32,
    ) -> Result<EditResult, JsValue> {
        let node = node_id(node)?;
        self.apply(Operation::SetHyperlink {
            range: EditRange {
                start: Pos::new(node, start),
                end: Pos::new(node, end),
            },
            id: node,
            target: None,
            tooltip: None,
        })
        .map_err(to_js)
    }

    /// Authored bookmark names, sorted for host navigation surfaces.
    #[wasm_bindgen(js_name = listBookmarks)]
    #[must_use]
    pub fn list_bookmarks(&self) -> Vec<String> {
        let mut names = self
            .document
            .definitions()
            .bookmarks
            .iter()
            .map(|(_, bookmark)| bookmark.name.clone())
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        names
    }

    /// Resolves an authored bookmark to `node\toffset`, or an empty string.
    #[wasm_bindgen(js_name = bookmarkPosition)]
    #[must_use]
    pub fn bookmark_position(&self, name: &str) -> String {
        resolve_bookmark(&self.document, name)
            .map(|position| format!("{}\t{}", position.node, position.offset))
            .unwrap_or_default()
    }

    /// Splits the paragraph at the caret into two (Enter). The caret lands at the
    /// start of the new trailing paragraph.
    #[wasm_bindgen(js_name = splitParagraph)]
    pub fn split_paragraph(&mut self, node: &str, offset: u32) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let new_id = self
            .edit_ids
            .next_id()
            .map_err(|_| to_js("id space exhausted".into()))?;
        self.apply(Operation::SplitParagraph {
            at: Pos::new(nid, offset),
            new_id,
        })
        .map_err(to_js)
    }

    /// Backspace at a collapsed caret: deletes the character before `offset`, or —
    /// at a paragraph start — joins this paragraph into the previous one.
    #[wasm_bindgen(js_name = deleteBackward)]
    pub fn delete_backward(&mut self, node: &str, offset: u32) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        if offset == 0 {
            let paras = self.ordered_paragraphs();
            let idx = paras.iter().position(|(id, _)| *id == nid);
            return match idx {
                Some(i) if i > 0 => self
                    .apply_action_as(
                        vec![Operation::JoinParagraphs {
                            first: paras[i - 1].0,
                            second: nid,
                        }],
                        HistoryKind::Delete,
                    )
                    .map_err(to_js),
                _ => Err(to_js("at document start".into())),
            };
        }
        let text = self.paragraph_text(nid);
        let prev = prev_char_boundary(&text, offset as usize) as u32;
        self.apply(Operation::DeleteText {
            range: EditRange {
                start: Pos::new(nid, prev),
                end: Pos::new(nid, offset),
            },
        })
        .map_err(to_js)
    }

    /// Forward-delete at a collapsed caret: deletes the character at `offset`, or —
    /// at a paragraph end — joins the next paragraph into this one.
    #[wasm_bindgen(js_name = deleteForward)]
    pub fn delete_forward(&mut self, node: &str, offset: u32) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let text = self.paragraph_text(nid);
        if offset as usize >= text.len() {
            let paras = self.ordered_paragraphs();
            let idx = paras.iter().position(|(id, _)| *id == nid);
            return match idx {
                Some(i) if i + 1 < paras.len() => self
                    .apply_action_as(
                        vec![Operation::JoinParagraphs {
                            first: nid,
                            second: paras[i + 1].0,
                        }],
                        HistoryKind::Delete,
                    )
                    .map_err(to_js),
                _ => Err(to_js("at document end".into())),
            };
        }
        let next = next_char_boundary(&text, offset as usize) as u32;
        self.apply(Operation::DeleteText {
            range: EditRange {
                start: Pos::new(nid, offset),
                end: Pos::new(nid, next),
            },
        })
        .map_err(to_js)
    }

    /// Deletes the Unicode word before a collapsed caret as one undoable edit.
    /// At a paragraph boundary this has the same safe join behavior as ordinary
    /// Backspace.
    #[wasm_bindgen(js_name = deleteWordBackward)]
    pub fn delete_word_backward(&mut self, node: &str, offset: u32) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        if offset == 0 {
            return self.delete_backward(node, offset);
        }
        let text = self.paragraph_text(nid);
        let end = (offset as usize).min(text.len());
        let start = prev_word_boundary(&text, end).unwrap_or(0);
        if start == end {
            return Err(to_js("nothing to delete".into()));
        }
        self.apply(Operation::DeleteText {
            range: EditRange {
                start: Pos::new(nid, start as u32),
                end: Pos::new(nid, end as u32),
            },
        })
        .map_err(to_js)
    }

    /// Deletes the Unicode word after a collapsed caret as one undoable edit.
    /// At a paragraph boundary this has the same safe join behavior as ordinary
    /// Delete.
    #[wasm_bindgen(js_name = deleteWordForward)]
    pub fn delete_word_forward(&mut self, node: &str, offset: u32) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let text = self.paragraph_text(nid);
        let start = (offset as usize).min(text.len());
        if start == text.len() {
            return self.delete_forward(node, offset);
        }
        let end = next_word_boundary(&text, start).unwrap_or(text.len());
        if start == end {
            return Err(to_js("nothing to delete".into()));
        }
        self.apply(Operation::DeleteText {
            range: EditRange {
                start: Pos::new(nid, start as u32),
                end: Pos::new(nid, end as u32),
            },
        })
        .map_err(to_js)
    }

    /// Deletes a selection that may span paragraphs (a selection + Backspace/
    /// Delete). Same-paragraph → one delete; cross-paragraph → delete the end
    /// pieces + join, as one undoable action. Caret lands at the selection start.
    #[wasm_bindgen(js_name = deleteSelection)]
    pub fn delete_selection(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> Result<EditResult, JsValue> {
        let (start, _end) = self
            .order_endpoints(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let ops = self
            .selection_delete_ops(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        if ops.is_empty() {
            return Err(to_js("empty selection".into()));
        }
        self.apply_action_caret_as(ops, start, HistoryKind::Delete)
            .map_err(to_js)
    }

    /// Replaces a selection with `text` (type-over) as one undoable action.
    #[wasm_bindgen(js_name = replaceSelection)]
    pub fn replace_selection(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        text: String,
    ) -> Result<EditResult, JsValue> {
        let (start, _end) = self
            .order_endpoints(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let mut ops = self
            .selection_delete_ops(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        if !text.is_empty() {
            ops.push(Operation::InsertText { at: start, text });
        }
        if ops.is_empty() {
            return Err(to_js("nothing to do".into()));
        }
        self.apply_action_as(ops, HistoryKind::Replace)
            .map_err(to_js)
    }

    /// Replaces a selection (which may be collapsed) with normalized plain text
    /// as one atomic user action. Newlines become paragraph splits inside the
    /// same operation group, so paste, IME commit, and replace-with-Enter each
    /// consume exactly one undo step.
    #[wasm_bindgen(js_name = insertPlainText)]
    pub fn insert_plain_text(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        text: String,
    ) -> Result<EditResult, JsValue> {
        self.insert_plain_text_action(
            start_node,
            start_offset,
            end_node,
            end_offset,
            text,
            HistoryKind::Typing,
        )
    }

    /// The same atomic plain-text command with a closed semantic history kind.
    /// The host may distinguish paste and paragraph-break gestures, but cannot
    /// inject arbitrary history labels.
    #[wasm_bindgen(js_name = insertPlainTextAs)]
    #[allow(clippy::too_many_arguments)]
    pub fn insert_plain_text_as(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        text: String,
        action_kind: &str,
    ) -> Result<EditResult, JsValue> {
        let kind = match action_kind {
            "paste" => HistoryKind::Paste,
            "paragraphBreak" => HistoryKind::ParagraphBreak,
            "typing" => HistoryKind::Typing,
            _ => return Err(to_js("unknown plain-text action kind".into())),
        };
        self.insert_plain_text_action(start_node, start_offset, end_node, end_offset, text, kind)
    }

    fn insert_plain_text_action(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        text: String,
        kind: HistoryKind,
    ) -> Result<EditResult, JsValue> {
        let (start, _end) = self
            .order_endpoints(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let mut ops = self
            .selection_delete_ops(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let (mut insert_ops, caret) = self.plain_text_ops(start, &text).map_err(to_js)?;
        ops.append(&mut insert_ops);
        if ops.is_empty() {
            return Err(to_js("nothing to insert".into()));
        }
        self.apply_action_caret_as(ops, caret, kind).map_err(to_js)
    }

    /// Incremental keyboard typing with explicit host gesture identity. Each call
    /// still edits/reflows immediately; adjacent calls with the same `session`
    /// coalesce into one history entry only when the previous caret exactly
    /// matches this insertion start.
    #[wasm_bindgen(js_name = typeText)]
    pub fn type_text(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        text: String,
        session: u32,
    ) -> Result<EditResult, JsValue> {
        let (start, end) = self
            .order_endpoints(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let mut ops = self
            .selection_delete_ops(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let (mut insert_ops, caret) = self.plain_text_ops(start, &text).map_err(to_js)?;
        ops.append(&mut insert_ops);
        if ops.is_empty() {
            return Err(to_js("nothing to type".into()));
        }
        let one_line = !text.contains('\r') && !text.contains('\n');
        self.apply_typing_action(ops, caret, start, session, start == end, one_line)
            .map_err(to_js)
    }

    /// Moves a caret by one step in `dir` (`"left"`, `"right"`, `"up"`, `"down"`),
    /// crossing line, paragraph, and page boundaries. Pure navigation — no edit.
    #[wasm_bindgen(js_name = moveCaret)]
    pub fn move_caret(&self, node: &str, offset: u32, dir: &str) -> Result<Caret, JsValue> {
        let nid = node_id(node)?;
        let pos = self.moved_caret(nid, offset, dir);
        Ok(Caret {
            node: pos.0.to_string(),
            offset: pos.1,
        })
    }

    /// Returns the ordered start or end edge of a selection. Selection ordering
    /// belongs to the engine because anchors in different paragraphs cannot be
    /// compared correctly by the browser host.
    #[wasm_bindgen(js_name = selectionEdge)]
    pub fn selection_edge(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        toward_end: bool,
    ) -> Result<Caret, JsValue> {
        let (start, end) = self
            .order_endpoints(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let pos = if toward_end { end } else { start };
        Ok(Caret {
            node: pos.node.to_string(),
            offset: pos.offset,
        })
    }

    /// The byte length of paragraph `node`'s text (triple-click paragraph select).
    #[wasm_bindgen(js_name = paragraphLength)]
    #[must_use]
    pub fn paragraph_length(&self, node: &str) -> u32 {
        NodeId::from_str(node).map_or(0, |nid| self.paragraph_text(nid).len() as u32)
    }

    /// The first caret position in the document (⌘↑ / select-all anchor).
    #[wasm_bindgen(js_name = firstPosition)]
    #[must_use]
    pub fn first_position(&self) -> Caret {
        self.ordered_paragraphs().first().map_or_else(
            || Caret {
                node: self.document.id().to_string(),
                offset: 0,
            },
            |(id, _)| Caret {
                node: id.to_string(),
                offset: 0,
            },
        )
    }

    /// The last caret position in the document (⌘↓ / select-all focus).
    #[wasm_bindgen(js_name = lastPosition)]
    #[must_use]
    pub fn last_position(&self) -> Caret {
        self.ordered_paragraphs().last().map_or_else(
            || Caret {
                node: self.document.id().to_string(),
                offset: 0,
            },
            |(id, len)| Caret {
                node: id.to_string(),
                offset: *len,
            },
        )
    }

    /// The byte range `[start, end]` of the word at `offset` (double-click select),
    /// as `[start, end]`; empty if the offset is not within a word.
    #[wasm_bindgen(js_name = wordAt)]
    #[must_use]
    pub fn word_at(&self, node: &str, offset: u32) -> Vec<u32> {
        let Ok(nid) = NodeId::from_str(node) else {
            return Vec::new();
        };
        let text = self.paragraph_text(nid);
        word_bounds(&text, offset as usize)
            .map(|(s, e)| vec![s as u32, e as u32])
            .unwrap_or_default()
    }

    /// Applies a run-property change (bold/italic/underline/strike) over a range
    /// within one paragraph. Each argument is a tri-state: `true`/`false` sets the
    /// toggle, `undefined` leaves it unchanged. Runs straddling the range split so
    /// the change lands exactly on the selection. The selection is preserved by
    /// the frontend (formatting does not collapse it).
    #[wasm_bindgen(js_name = formatText)]
    #[allow(clippy::too_many_arguments)] // a flat JS signature is clearer than a bag struct
    pub fn format_text(
        &mut self,
        node: &str,
        start: u32,
        end: u32,
        bold: Option<bool>,
        italic: Option<bool>,
        underline: Option<bool>,
        strike: Option<bool>,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        self.apply(Operation::FormatText {
            range: EditRange {
                start: Pos::new(nid, start),
                end: Pos::new(nid, end),
            },
            delta: FormatDelta {
                bold,
                italic,
                underline,
                strike,
                ..FormatDelta::default()
            },
        })
        .map_err(to_js)
    }

    /// Clears direct character formatting over a range while preserving the
    /// paragraph's named style. The selection remains intact in the host.
    #[wasm_bindgen(js_name = clearFormatting)]
    pub fn clear_formatting(
        &mut self,
        node: &str,
        start: u32,
        end: u32,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        self.apply(Operation::ClearFormatting {
            range: EditRange {
                start: Pos::new(nid, start),
                end: Pos::new(nid, end),
            },
        })
        .map_err(to_js)
    }

    /// The uniform format state of a range (each toggle `true` only when every
    /// covered run sets it) — drives a toolbar's active state and toggle direction.
    #[wasm_bindgen(js_name = formatAt)]
    #[must_use]
    pub fn format_at(&self, node: &str, start: u32, end: u32) -> Format {
        let Ok(nid) = NodeId::from_str(node) else {
            return Format::default();
        };
        let properties = effective_run_properties_in_range(&self.document, nid, start, end);
        format_from_effective_runs(&properties)
    }

    /// The run formatting a collapsed caret inherits — what new typing there would
    /// carry (the run to the caret's left, Word's rule). Drives the toolbar's active
    /// state at a caret and the "type bold" toggle direction, where `formatAt` on an
    /// empty range reports all-false.
    #[wasm_bindgen(js_name = caretFormat)]
    #[must_use]
    pub fn caret_format(&self, node: &str, offset: u32) -> Format {
        let Ok(nid) = NodeId::from_str(node) else {
            return Format::default();
        };
        effective_caret_run_properties(&self.document, nid, offset)
            .map_or_else(Format::default, |properties| {
                format_from_effective_runs(&[properties])
            })
    }

    /// Inserts `text` at a collapsed caret carrying explicit run formatting — typing
    /// while Bold/Italic/… is *armed* at the caret. Inserts the text, then formats
    /// exactly the inserted range, as one atomic single-undo action; the caret lands
    /// after the inserted text. A toggle left `undefined` inherits the surrounding
    /// run, so this equals `insertText` when nothing is armed. Consecutive armed
    /// typing coalesces into one run (the second char inserts into the run the first
    /// created).
    #[wasm_bindgen(js_name = insertStyledText)]
    #[allow(clippy::too_many_arguments)] // a flat JS signature is clearer than a bag struct
    pub fn insert_styled_text(
        &mut self,
        node: &str,
        offset: u32,
        text: String,
        bold: Option<bool>,
        italic: Option<bool>,
        underline: Option<bool>,
        strike: Option<bool>,
        size_half_points: Option<u32>,
        color: Option<String>,
        highlight: Option<String>,
        vert_align: Option<String>,
        font: Option<String>,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        self.styled_text_action(
            nid,
            offset,
            text,
            bold,
            italic,
            underline,
            strike,
            size_half_points,
            color,
            highlight,
            vert_align,
            font,
            None,
        )
        .map_err(to_js)
    }

    /// Styled counterpart of [`type_text`](Self::type_text): preserves an armed
    /// run format while allowing adjacent keyboard calls in the same host typing
    /// session to undo/redo as one user action.
    #[wasm_bindgen(js_name = typeStyledText)]
    #[allow(clippy::too_many_arguments)] // a flat JS signature is clearer than a bag struct
    pub fn type_styled_text(
        &mut self,
        node: &str,
        offset: u32,
        text: String,
        bold: Option<bool>,
        italic: Option<bool>,
        underline: Option<bool>,
        strike: Option<bool>,
        size_half_points: Option<u32>,
        color: Option<String>,
        highlight: Option<String>,
        vert_align: Option<String>,
        font: Option<String>,
        session: u32,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        self.styled_text_action(
            nid,
            offset,
            text,
            bold,
            italic,
            underline,
            strike,
            size_half_points,
            color,
            highlight,
            vert_align,
            font,
            Some(session),
        )
        .map_err(to_js)
    }

    /// Like [`format_text`](Self::format_text) but across a selection that may span
    /// paragraphs — each covered paragraph's sub-range is formatted, as one
    /// undoable action.
    #[wasm_bindgen(js_name = formatSelection)]
    #[allow(clippy::too_many_arguments)] // a flat JS signature is clearer than a bag struct
    pub fn format_selection(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        bold: Option<bool>,
        italic: Option<bool>,
        underline: Option<bool>,
        strike: Option<bool>,
    ) -> Result<EditResult, JsValue> {
        let (start, end) = self
            .order_endpoints(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let delta = FormatDelta {
            bold,
            italic,
            underline,
            strike,
            ..FormatDelta::default()
        };
        let ops: Vec<Operation> = self
            .selection_subranges(start, end)
            .into_iter()
            .map(|(node, s, e)| Operation::FormatText {
                range: EditRange {
                    start: Pos::new(node, s),
                    end: Pos::new(node, e),
                },
                delta: delta.clone(),
            })
            .collect();
        if ops.is_empty() {
            return Err(to_js("empty selection".into()));
        }
        self.apply_action(ops).map_err(to_js)
    }

    /// The uniform format state across a (possibly multi-paragraph) selection — a
    /// toggle is `true` only when every covered run in every covered paragraph sets
    /// it.
    #[wasm_bindgen(js_name = selectionFormat)]
    #[must_use]
    pub fn selection_format(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> Format {
        let Ok((start, end)) = self.order_endpoints(start_node, start_offset, end_node, end_offset)
        else {
            return Format::default();
        };
        let subs = self.selection_subranges(start, end);
        if subs.is_empty() {
            return Format::default();
        }
        let mut properties = Vec::new();
        for (node, s, e) in subs {
            properties.extend(effective_run_properties_in_range(
                &self.document,
                node,
                s,
                e,
            ));
        }
        format_from_effective_runs(&properties)
    }

    // ---- Run properties over a selection (color / highlight / size / vertAlign) --

    /// Sets the text color over the selection to an explicit RGB.
    #[wasm_bindgen(js_name = setTextColor)]
    #[allow(clippy::too_many_arguments)] // flat JS signature (node/offsets + rgb)
    pub fn set_text_color(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        r: u8,
        g: u8,
        b: u8,
    ) -> Result<EditResult, JsValue> {
        self.apply_run_format(
            start_node,
            start_offset,
            end_node,
            end_offset,
            FormatDelta {
                color: Some(RgbColor { r, g, b }),
                ..FormatDelta::default()
            },
        )
    }

    /// Sets the highlight over the selection to a named color (`"yellow"`,
    /// `"green"`, … or `"none"` to clear).
    #[wasm_bindgen(js_name = setHighlight)]
    pub fn set_highlight(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        name: &str,
    ) -> Result<EditResult, JsValue> {
        self.apply_run_format(
            start_node,
            start_offset,
            end_node,
            end_offset,
            FormatDelta {
                highlight: Some(parse_highlight(name)),
                ..FormatDelta::default()
            },
        )
    }

    /// Sets the font size over the selection, in **points**.
    #[wasm_bindgen(js_name = setFontSize)]
    pub fn set_font_size(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        points: f32,
    ) -> Result<EditResult, JsValue> {
        if !points.is_finite() || !(1.0..=1638.0).contains(&points) || (points * 2.0).fract() != 0.0
        {
            return Err(to_js(
                "font size must be between 1 and 1638 points in 0.5 point steps".into(),
            ));
        }
        let half_points = (points * 2.0).round() as u32;
        self.apply_run_format(
            start_node,
            start_offset,
            end_node,
            end_offset,
            FormatDelta {
                size_half_points: Some(half_points),
                ..FormatDelta::default()
            },
        )
    }

    /// Sets the baseline alignment over the selection: `"super"`, `"sub"`, or
    /// `"baseline"`.
    #[wasm_bindgen(js_name = setVertAlign)]
    pub fn set_vert_align(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        which: &str,
    ) -> Result<EditResult, JsValue> {
        let value = match which {
            "super" => VerticalAlignment::Superscript,
            "sub" => VerticalAlignment::Subscript,
            _ => VerticalAlignment::Baseline,
        };
        self.apply_run_format(
            start_node,
            start_offset,
            end_node,
            end_offset,
            FormatDelta {
                vertical_alignment: Some(value),
                ..FormatDelta::default()
            },
        )
    }

    // ---- Paragraph properties over a selection --------------------------------

    /// Sets paragraph alignment over every paragraph the selection touches:
    /// `"start"`, `"center"`, `"end"`, or `"justify"`.
    #[wasm_bindgen(js_name = setAlignment)]
    pub fn set_alignment(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        align: &str,
    ) -> Result<EditResult, JsValue> {
        let value = match align {
            "center" => Alignment::Center,
            "end" | "right" => Alignment::End,
            "justify" => Alignment::Justify,
            _ => Alignment::Start,
        };
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            p.alignment = Some(value);
        })
    }

    /// Sets multiple line spacing (`w:lineRule="auto"`) as a percentage of single
    /// (100 = single, 150 = 1.5×, 200 = double) over the selection's paragraphs. Per
    /// the model, the `auto` rule leaves `line_rule` `None` and rides `line_percent`,
    /// so exact/atLeast values are cleared.
    #[wasm_bindgen(js_name = setLineSpacing)]
    pub fn set_line_spacing(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        percent: u16,
    ) -> Result<EditResult, JsValue> {
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            let mut spacing = p.spacing.unwrap_or_default();
            spacing.line_percent = Some(percent);
            spacing.line_rule = None; // `auto` is the implicit-default rule
            spacing.line_twips = None;
            p.spacing = Some(spacing);
        })
    }

    /// Sets a fixed line height in twips over the selection's paragraphs: `at_least`
    /// true → `w:lineRule="atLeast"` (grows for tall content), false → `"exact"`
    /// (clipped). Clears the `auto` percentage.
    #[wasm_bindgen(js_name = setLineSpacingExact)]
    pub fn set_line_spacing_exact(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        twips: i32,
        at_least: bool,
    ) -> Result<EditResult, JsValue> {
        let rule = if at_least {
            casual_doc_model::v1::LineRule::AtLeast
        } else {
            casual_doc_model::v1::LineRule::Exact
        };
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            let mut spacing = p.spacing.unwrap_or_default();
            spacing.line_rule = Some(rule);
            spacing.line_twips = Some(twips.max(0));
            spacing.line_percent = None;
            p.spacing = Some(spacing);
        })
    }

    /// Sets space before the paragraph (`w:spacing w:before`) in twips over the
    /// selection; a negative value clears it (back to the style default). Setting an
    /// explicit value also turns off `beforeAutospacing`.
    #[wasm_bindgen(js_name = setSpaceBefore)]
    pub fn set_space_before(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        twips: i32,
    ) -> Result<EditResult, JsValue> {
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            let mut spacing = p.spacing.unwrap_or_default();
            if twips < 0 {
                spacing.before_twips = None;
                spacing.before_auto = None;
            } else {
                spacing.before_twips = Some(twips);
                spacing.before_auto = Some(false);
            }
            p.spacing = Some(spacing);
        })
    }

    /// Sets space after the paragraph (`w:spacing w:after`) in twips over the
    /// selection; a negative value clears it. Setting an explicit value also turns
    /// off `afterAutospacing`.
    #[wasm_bindgen(js_name = setSpaceAfter)]
    pub fn set_space_after(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        twips: i32,
    ) -> Result<EditResult, JsValue> {
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            let mut spacing = p.spacing.unwrap_or_default();
            if twips < 0 {
                spacing.after_twips = None;
                spacing.after_auto = None;
            } else {
                spacing.after_twips = Some(twips);
                spacing.after_auto = Some(false);
            }
            p.spacing = Some(spacing);
        })
    }

    /// Adjusts the left (start) indent by `delta_twips` (positive indents,
    /// negative outdents, clamped at 0) over the selection's paragraphs.
    #[wasm_bindgen(js_name = adjustIndent)]
    pub fn adjust_indent(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        delta_twips: i32,
    ) -> Result<EditResult, JsValue> {
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            let mut indent = p.indentation.unwrap_or(Indentation {
                start_twips: None,
                end_twips: None,
                first_line_twips: None,
                hanging_twips: None,
            });
            let current = indent.start_twips.unwrap_or(0);
            indent.start_twips = Some((current + delta_twips).max(0));
            p.indentation = Some(indent);
        })
    }

    /// Sets the left (start) indent to an absolute `twips` (clamped ≥ 0) — the
    /// ruler's left-indent marker drag.
    #[wasm_bindgen(js_name = setLeftIndent)]
    pub fn set_left_indent(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        twips: i32,
    ) -> Result<EditResult, JsValue> {
        self.apply_indent_props(start_node, start_offset, end_node, end_offset, move |p| {
            let mut indent = p.indentation.unwrap_or(EMPTY_INDENT);
            indent.start_twips = Some(twips.max(0));
            p.indentation = Some(indent);
        })
    }

    /// Sets the right (end) indent to an absolute `twips` (clamped ≥ 0) — the ruler's
    /// right-indent marker drag.
    #[wasm_bindgen(js_name = setRightIndent)]
    pub fn set_right_indent(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        twips: i32,
    ) -> Result<EditResult, JsValue> {
        self.apply_indent_props(start_node, start_offset, end_node, end_offset, move |p| {
            let mut indent = p.indentation.unwrap_or(EMPTY_INDENT);
            indent.end_twips = Some(twips.max(0));
            p.indentation = Some(indent);
        })
    }

    /// Sets the first-line indent (relative to the left indent) to `twips` — the
    /// ruler's top marker drag. Positive is a first-line indent; negative becomes a
    /// hanging indent (mutually exclusive, as in Word); zero clears both.
    #[wasm_bindgen(js_name = setFirstLineIndent)]
    pub fn set_first_line_indent(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        twips: i32,
    ) -> Result<EditResult, JsValue> {
        self.apply_indent_props(start_node, start_offset, end_node, end_offset, move |p| {
            let mut indent = p.indentation.unwrap_or(EMPTY_INDENT);
            if twips > 0 {
                indent.first_line_twips = Some(twips);
                indent.hanging_twips = None;
            } else if twips < 0 {
                indent.hanging_twips = Some(-twips);
                indent.first_line_twips = None;
            } else {
                indent.first_line_twips = None;
                indent.hanging_twips = None;
            }
            p.indentation = Some(indent);
        })
    }

    /// Toggles a `"bullet"` or `"numbered"` list over the selection's paragraphs, as
    /// one undoable action. The shared list definition is created on first use and
    /// reused after; the toggle direction (on vs off) is decided by the first
    /// selected paragraph — if it already belongs to this list it is turned off,
    /// else every selected paragraph is turned on. Pressing Enter in a list item
    /// continues the list automatically (split copies the paragraph properties).
    #[wasm_bindgen(js_name = toggleList)]
    pub fn toggle_list(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        kind: &str,
    ) -> Result<EditResult, JsValue> {
        let numbered = kind == "numbered";
        let instance = self.ensure_list(numbered).map_err(to_js)?;
        // Direction: turn the list off iff the first selected paragraph already uses
        // exactly this list instance; otherwise turn it on for the whole selection.
        let (start, _end) = self
            .order_endpoints(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let already = paragraph_properties(&self.document, start.node)
            .and_then(|p| p.numbering)
            .is_some_and(|n| n.instance == instance);
        let target = (!already).then_some(NumberingRef { instance, level: 0 });
        self.apply_paragraph_props_as(
            start_node,
            start_offset,
            end_node,
            end_offset,
            HistoryKind::ListFormatting,
            move |p| {
                p.numbering = target;
            },
        )
    }

    /// Promotes or demotes list paragraphs by one numbering level, preserving
    /// their list instance. Non-list paragraphs are rejected so the host can
    /// retain ordinary indentation behavior.
    #[wasm_bindgen(js_name = adjustListLevel)]
    pub fn adjust_list_level(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        delta: i32,
    ) -> Result<EditResult, JsValue> {
        let (start, _) = self
            .order_endpoints(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        if paragraph_properties(&self.document, start.node)
            .and_then(|p| p.numbering)
            .is_none()
        {
            return Err(to_js(
                "list level adjustment requires a list paragraph".into(),
            ));
        }
        self.apply_paragraph_props_as(
            start_node,
            start_offset,
            end_node,
            end_offset,
            HistoryKind::ListFormatting,
            move |p| {
                if let Some(mut numbering) = p.numbering {
                    numbering.level = (i32::from(numbering.level) + delta).clamp(0, 8) as u8;
                    p.numbering = Some(numbering);
                }
            },
        )
    }

    /// Restarts a numbered list at the caret's paragraph and carries the new
    /// numbering instance through the contiguous following items at the same
    /// level. Bullets and non-list paragraphs are rejected.
    #[wasm_bindgen(js_name = restartList)]
    pub fn restart_list(&mut self, node: &str) -> Result<EditResult, JsValue> {
        let start_node = node_id(node)?;
        let Some(current) =
            paragraph_properties(&self.document, start_node).and_then(|p| p.numbering)
        else {
            return Err(to_js(
                "restart numbering requires a numbered list item".into(),
            ));
        };
        if self.list_format(current.instance) == Some(NumberFormat::Bullet) {
            return Err(to_js(
                "restart numbering requires a numbered list item".into(),
            ));
        }
        let abstract_ref = self
            .document
            .definitions()
            .numbering
            .get(&current.instance)
            .ok_or_else(|| to_js("numbering definition not found".into()))?
            .abstract_ref;
        let new_instance = NumberingInstanceId::new(
            self.edit_ids
                .next_id()
                .map_err(|_| to_js("id space exhausted".into()))?,
        );
        self.document.definitions_mut().numbering.insert(
            new_instance,
            NumberingInstance {
                abstract_ref,
                overrides: vec![NumberingOverride {
                    level: current.level,
                    start: Some(1),
                }],
            },
        );

        let ordered = self.ordered_paragraphs();
        let Some(index) = ordered.iter().position(|(id, _)| *id == start_node) else {
            return Err(to_js("paragraph not found".into()));
        };
        let mut ops = Vec::new();
        for (id, _) in ordered.into_iter().skip(index) {
            let Some(properties) = paragraph_properties(&self.document, id) else {
                continue;
            };
            let Some(numbering) = properties.numbering else {
                break;
            };
            if numbering.instance != current.instance || numbering.level != current.level {
                break;
            }
            let mut next = properties.clone();
            next.numbering = Some(NumberingRef {
                instance: new_instance,
                level: current.level,
            });
            ops.push(Operation::SetParagraphProperties {
                node: id,
                properties: Box::new(next),
            });
        }
        if ops.is_empty() {
            return Err(to_js("no contiguous numbered items to restart".into()));
        }
        self.apply_action_caret_as(ops, Pos::new(start_node, 0), HistoryKind::ListFormatting)
            .map_err(to_js)
    }

    /// The list kind the paragraph belongs to: `"bullet"`, `"numbered"`, or `""` —
    /// drives the toolbar's list-button active state.
    #[wasm_bindgen(js_name = listStyleAt)]
    #[must_use]
    pub fn list_style_at(&self, node: &str) -> String {
        let Ok(nid) = NodeId::from_str(node) else {
            return String::new();
        };
        let Some(reference) = paragraph_properties(&self.document, nid).and_then(|p| p.numbering)
        else {
            return String::new();
        };
        if Some(reference.instance) == self.numbered_list {
            "numbered".to_string()
        } else if Some(reference.instance) == self.bullet_list {
            "bullet".to_string()
        } else {
            // A list from the imported document (not one we created) — reflect its
            // format so the right button still lights up.
            self.list_format(reference.instance)
                .map_or_else(String::new, |f| {
                    if matches!(f, NumberFormat::Bullet) {
                        "bullet".to_string()
                    } else {
                        "numbered".to_string()
                    }
                })
        }
    }

    /// Inserts an empty table row above or below the row containing the caret's
    /// paragraph (`below` = after it). The new row mirrors the anchor row's column
    /// structure (cell count, widths, spans) with empty cells; the caret lands in
    /// its first cell. Errors if the caret is not inside a table.
    #[wasm_bindgen(js_name = insertRow)]
    pub fn insert_row(&mut self, node: &str, below: bool) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, index, template) = locate_table_row(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let row = self.empty_row_like(&template).map_err(to_js)?;
        // Caret rests in the new row's first cell's paragraph.
        let caret = first_paragraph_of_row(&row).map_or(Pos::new(nid, 0), |p| Pos::new(p, 0));
        let new_index = if below { index + 1 } else { index };
        self.apply_action_caret(
            vec![Operation::InsertRow {
                table,
                index: new_index,
                row: Box::new(row),
            }],
            caret,
        )
        .map_err(to_js)
    }

    /// Deletes the table row containing the caret's paragraph. Refuses to delete a
    /// table's only row (a table's rows are non-empty — delete the table instead).
    /// The caret lands in an adjacent surviving row. Errors if not inside a table.
    #[wasm_bindgen(js_name = deleteRow)]
    pub fn delete_row(&mut self, node: &str) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, index, _) = locate_table_row(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        // Caret target: the first paragraph of the row that survives at this spot
        // (the next row, or the previous one when deleting the last), computed
        // before the edit removes it.
        let caret = self
            .surviving_row_anchor(table, index)
            .map_or_else(|| Pos::new(nid, 0), |p| Pos::new(p, 0));
        self.apply_action_caret(vec![Operation::DeleteRow { table, index }], caret)
            .map_err(to_js)
    }

    /// Inserts an empty column left or right of the caret's cell (`after` = to its
    /// right) in a **regular** table (no merged cells; grid matches the cell count),
    /// adding one empty cell to every row and a grid column. The caret stays in the
    /// same row, moving to the new cell. Errors if the caret is not inside a regular
    /// table.
    #[wasm_bindgen(js_name = insertColumn)]
    pub fn insert_column(&mut self, node: &str, after: bool) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, col) = locate_table_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let (_, row_index, _) = locate_table_row(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let t = find_table(&self.document, table).ok_or_else(|| to_js("table not found".into()))?;
        let width = t.grid.get(col as usize).and_then(|g| g.width_twips);
        let cells = self.empty_column_cells(t.rows.len()).map_err(to_js)?;
        let new_index = if after { col + 1 } else { col };
        // Caret rests in the new cell of the caret's own row.
        let caret = cells
            .get(row_index as usize)
            .and_then(first_paragraph_of_cell)
            .map_or(Pos::new(nid, 0), |p| Pos::new(p, 0));
        self.apply_action_caret(
            vec![Operation::InsertColumn {
                table,
                index: new_index,
                width,
                cells,
            }],
            caret,
        )
        .map_err(to_js)
    }

    /// Deletes the column containing the caret's cell from a **regular** table
    /// (refusing a table's only column), removing that cell from every row. The
    /// caret lands in an adjacent surviving cell of the same row. Errors if the
    /// caret is not inside a regular table.
    #[wasm_bindgen(js_name = deleteColumn)]
    pub fn delete_column(&mut self, node: &str) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, col) = locate_table_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let (_, row_index, _) = locate_table_row(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        // Caret target: the cell that survives at this spot in the caret's row (the
        // next column, or the previous one when deleting the last), read before the
        // edit removes it.
        let caret = self
            .surviving_cell_anchor(table, row_index, col)
            .map_or_else(|| Pos::new(nid, 0), |p| Pos::new(p, 0));
        self.apply_action_caret(vec![Operation::DeleteColumn { table, index: col }], caret)
            .map_err(to_js)
    }

    /// Deletes the whole table the caret is in (the innermost table for a nested
    /// one). Refuses when the table is a cell's only content. The caret lands on the
    /// first top-level body paragraph. Errors if the caret is not inside a table.
    #[wasm_bindgen(js_name = deleteTable)]
    pub fn delete_table(&mut self, node: &str) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, _, _) = locate_table_row(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        // Caret rests on the first top-level body paragraph (always outside the
        // deleted table); falls back to the document node for a table-only body.
        let caret = self
            .first_body_paragraph()
            .map_or_else(|| Pos::new(self.document.id(), 0), |p| Pos::new(p, 0));
        self.apply_action_caret(vec![Operation::DeleteTable { table }], caret)
            .map_err(to_js)
    }

    /// The first top-level paragraph in the body, if any.
    fn first_body_paragraph(&self) -> Option<NodeId> {
        self.document.body().iter().find_map(|b| match b {
            BlockNode::Paragraph(p) => Some(p.id),
            _ => None,
        })
    }

    /// Inserts a fresh `rows`×`cols` table into the body right after the caret's
    /// top-level block, with equal columns spanning the content width and a thin
    /// single-line grid so it is visible. The caret lands in the first cell.
    #[wasm_bindgen(js_name = insertTable)]
    pub fn insert_table(
        &mut self,
        node: &str,
        rows: u32,
        cols: u32,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let rows = rows.clamp(1, 50) as usize;
        let cols = cols.clamp(1, 20) as usize;
        let exhausted = || to_js("id space exhausted".into());

        let c = &self.default_config;
        let content_w =
            (c.page_size.width.raw() - c.margin_start.raw() - c.margin_end.raw()).max(cols as i32);
        let col_w = content_w / cols as i32;

        let grid = (0..cols)
            .map(|_| GridColumn {
                width_twips: Some(col_w),
            })
            .collect();

        let mut table_rows = Vec::with_capacity(rows);
        for _ in 0..rows {
            let mut cells = Vec::with_capacity(cols);
            for _ in 0..cols {
                let cell_id = self.edit_ids.next_id().map_err(|_| exhausted())?;
                let para_id = self.edit_ids.next_id().map_err(|_| exhausted())?;
                cells.push(TableCell {
                    id: cell_id,
                    properties: TableCellProperties {
                        width_twips: Some(col_w),
                        ..TableCellProperties::default()
                    },
                    blocks: vec![BlockNode::Paragraph(Paragraph {
                        id: para_id,
                        properties: ParagraphProperties::default(),
                        inlines: Vec::new(),
                    })],
                });
            }
            let row_id = self.edit_ids.next_id().map_err(|_| exhausted())?;
            table_rows.push(TableRow {
                id: row_id,
                properties: Default::default(),
                cells,
            });
        }

        let mut properties = TableProperties::default();
        set_table_borders_preset(&mut properties.borders, "all", || border_edge(0, 0, 0, 4));
        let table_id = self.edit_ids.next_id().map_err(|_| exhausted())?;
        let table = Table {
            id: table_id,
            grid,
            grid_change: None,
            properties,
            rows: table_rows,
        };

        let index = self.body_index_after(nid) as u32;
        let caret = table
            .rows
            .first()
            .and_then(first_paragraph_of_row)
            .map_or_else(|| Pos::new(table_id, 0), |p| Pos::new(p, 0));
        self.apply_action_caret(
            vec![Operation::InsertTable {
                container: None,
                index,
                table: Box::new(table),
            }],
            caret,
        )
        .map_err(to_js)
    }

    /// The body index right after the top-level block that (recursively) contains
    /// `node` — where [`insert_table`](Self::insert_table) drops a new table. Falls
    /// back to the end of the body.
    fn body_index_after(&self, node: NodeId) -> usize {
        let blocks = self.document.body();
        blocks
            .iter()
            .position(|b| block_holds(b, node))
            .map_or(blocks.len(), |i| i + 1)
    }

    /// Sets the table's horizontal alignment on the page: `"left"`/`"center"`/
    /// `"right"` (the table containing `node`).
    #[wasm_bindgen(js_name = setTableAlignment)]
    pub fn set_table_alignment(&mut self, node: &str, align: &str) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, _cell) =
            locate_cell(&self.document, nid).ok_or_else(|| to_js("not in a table".into()))?;
        let mut props = find_table(&self.document, table)
            .map(|t| t.properties.clone())
            .ok_or_else(|| to_js("table not found".into()))?;
        props.alignment = Some(match align {
            "center" => Alignment::Center,
            "right" => Alignment::End,
            _ => Alignment::Start,
        });
        self.apply_action(vec![Operation::SetTableProperties {
            table,
            properties: Box::new(props),
        }])
        .map_err(to_js)
    }

    /// Whether the caret's paragraph is inside a table cell — drives the table
    /// controls' enabled state.
    #[wasm_bindgen(js_name = inTable)]
    #[must_use]
    pub fn in_table(&self, node: &str) -> bool {
        NodeId::from_str(node)
            .ok()
            .is_some_and(|nid| locate_table_row(&self.document, nid).is_some())
    }

    /// Metadata about the innermost table containing `node`, used by the editor's
    /// table-selection affordances. Returns an empty/not-found object when the
    /// node is outside a table.
    #[wasm_bindgen(js_name = tableInfo)]
    #[must_use]
    pub fn table_info(&self, node: &str) -> TableInfo {
        let Ok(nid) = NodeId::from_str(node) else {
            return TableInfo::none();
        };
        let Some((table, row, _)) = locate_table_row(&self.document, nid) else {
            return TableInfo::none();
        };
        let Some((_, col)) = locate_table_cell(&self.document, nid) else {
            return TableInfo::none();
        };
        let Some(t) = find_table(&self.document, table) else {
            return TableInfo::none();
        };
        TableInfo {
            found: true,
            table: table.to_string(),
            row,
            column: col,
            rows: t.rows.len() as u32,
            columns: table_column_count(t) as u32,
            regular: table_is_regular(t),
            header_row: t
                .rows
                .get(row as usize)
                .is_some_and(|r| r.properties.header),
            column_width_twips: active_column_width(t, col as usize).unwrap_or(-1),
            table_width_twips: t.properties.width_twips.unwrap_or(-1),
            table_indent_twips: t.properties.indent_twips.unwrap_or(0),
            fixed_layout: t.properties.layout == Some(TableLayout::Fixed),
            row_height_twips: t
                .rows
                .get(row as usize)
                .and_then(|r| r.properties.height.value_twips)
                .map_or(-1, |v| v as i32),
            row_height_rule: t
                .rows
                .get(row as usize)
                .and_then(|r| r.properties.height.rule)
                .map_or_else(String::new, height_rule_name),
            cell_margin_twips: uniform_margins(t.properties.cell_margins).unwrap_or(-1),
            cell_spacing_twips: t.properties.cell_spacing_twips.unwrap_or(-1),
            alignment: match t.properties.alignment {
                Some(Alignment::Center) => "center",
                Some(Alignment::End) => "right",
                _ => "left",
            }
            .to_owned(),
            caption: t.properties.caption.clone().unwrap_or_default(),
            description: t.properties.description.clone().unwrap_or_default(),
        }
    }

    /// Applies the table-properties inspector draft as one undoable action.
    ///
    /// Only fields present in the JSON patch are changed, so opening and applying
    /// the inspector does not materialize model defaults that were previously
    /// absent. Table-, active-row-, and active-column-level values are committed
    /// together through one [`Operation::ReplaceTable`].
    #[wasm_bindgen(js_name = applyTableProperties)]
    pub fn apply_table_properties(
        &mut self,
        node: &str,
        properties_json: &str,
    ) -> Result<EditResult, JsValue> {
        let patch: TablePropertiesPatch = serde_json::from_str(properties_json)
            .map_err(|err| to_js(format!("invalid table properties: {err}")))?;
        if patch.is_empty() {
            return Err(to_js("no table property changes".into()));
        }

        let nid = node_id(node)?;
        let (table, row, _) = locate_table_row(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let (_, column) = locate_table_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let mut replacement = find_table(&self.document, table)
            .ok_or_else(|| to_js("table not found".into()))?
            .clone();

        if let Some(alignment) = patch.alignment.as_deref() {
            replacement.properties.alignment = Some(match alignment {
                "left" | "start" => Alignment::Start,
                "center" => Alignment::Center,
                "right" | "end" => Alignment::End,
                _ => {
                    return Err(to_js(
                        "table alignment must be left, center, or right".into(),
                    ));
                }
            });
        }
        if let Some(width) = patch.table_width_twips {
            ensure_optional_twips("table width", width)?;
            replacement.properties.width_twips = (width >= 0).then_some(width);
        }
        if let Some(indent) = patch.table_indent_twips {
            ensure_signed_twips("table indent", indent)?;
            replacement.properties.indent_twips = Some(indent);
        }
        if let Some(fixed) = patch.fixed_layout {
            replacement.properties.layout = Some(if fixed {
                TableLayout::Fixed
            } else {
                TableLayout::Autofit
            });
        }
        if let Some(margin) = patch.cell_margin_twips {
            ensure_optional_twips("cell margin", margin)?;
            replacement.properties.cell_margins = if margin < 0 {
                CellMargins::default()
            } else {
                CellMargins {
                    top_twips: Some(margin),
                    start_twips: Some(margin),
                    bottom_twips: Some(margin),
                    end_twips: Some(margin),
                }
            };
        }
        if let Some(spacing) = patch.cell_spacing_twips {
            ensure_optional_twips("cell spacing", spacing)?;
            replacement.properties.cell_spacing_twips = (spacing >= 0).then_some(spacing);
        }
        if let Some(caption) = patch.caption {
            replacement.properties.caption = normalize_table_metadata("caption", caption)?;
        }
        if let Some(description) = patch.description {
            replacement.properties.description =
                normalize_table_metadata("description", description)?;
        }

        let active_row = replacement
            .rows
            .get_mut(row as usize)
            .ok_or_else(|| to_js("row is outside the table".into()))?;
        if let Some(header) = patch.header_row {
            active_row.properties.header = header;
        }
        if let Some(rule) = patch.row_height_rule.as_deref() {
            let height = patch.row_height_twips.unwrap_or(-1);
            active_row.properties.height = match rule {
                "auto" => RowHeight::default(),
                "atLeast" | "at_least" => RowHeight {
                    value_twips: Some(required_twips("row height", height)? as u32),
                    rule: Some(HeightRule::AtLeast),
                },
                "exact" => RowHeight {
                    value_twips: Some(required_twips("row height", height)? as u32),
                    rule: Some(HeightRule::Exact),
                },
                _ => {
                    return Err(to_js(
                        "row height rule must be auto, atLeast, or exact".into(),
                    ));
                }
            };
        } else if patch.row_height_twips.is_some() {
            return Err(to_js("row height requires a row height rule".into()));
        }

        if let Some(width) = patch.column_width_twips {
            ensure_optional_twips("column width", width)?;
            if !table_is_regular(&replacement) {
                return Err(to_js("column width requires a regular table".into()));
            }
            let column = column as usize;
            let columns = table_column_count(&replacement);
            if column >= columns {
                return Err(to_js("column is outside the table".into()));
            }
            if replacement.grid.len() < columns {
                replacement
                    .grid
                    .resize(columns, GridColumn { width_twips: None });
            }
            replacement.grid[column].width_twips = (width >= 0).then_some(width);
            for row in &mut replacement.rows {
                row.cells[column].properties.width_twips = (width >= 0).then_some(width);
            }
        }

        self.apply_action(vec![Operation::ReplaceTable {
            table,
            replacement: Box::new(replacement),
        }])
        .map_err(to_js)
    }

    /// Sets the active regular table column's preferred width in twips. Updates
    /// the shared grid and each cell in that visual column so DOCX round-tripping
    /// and layout stay in agreement.
    #[wasm_bindgen(js_name = setTableColumnWidth)]
    pub fn set_table_column_width(
        &mut self,
        node: &str,
        width_twips: i32,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (_, col) = locate_table_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        self.set_table_column_width_at(node, col, width_twips)
    }

    /// Preferred width for a specific regular-table column, or `-1` when unset.
    #[wasm_bindgen(js_name = tableColumnWidthAt)]
    #[must_use]
    pub fn table_column_width_at(&self, node: &str, column: u32) -> i32 {
        let Ok(nid) = NodeId::from_str(node) else {
            return -1;
        };
        let Some((table, _)) = locate_table_cell(&self.document, nid) else {
            return -1;
        };
        find_table(&self.document, table)
            .and_then(|t| active_column_width(t, column as usize))
            .unwrap_or(-1)
    }

    /// Sets a specific regular-table column's preferred width in twips.
    #[wasm_bindgen(js_name = setTableColumnWidthAt)]
    pub fn set_table_column_width_at(
        &mut self,
        node: &str,
        column: u32,
        width_twips: i32,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, _) = locate_table_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let mut replacement = find_table(&self.document, table)
            .ok_or_else(|| to_js("table not found".into()))?
            .clone();
        if !table_is_regular(&replacement) {
            return Err(to_js("column width requires a regular table".into()));
        }
        let col = column as usize;
        if col >= table_column_count(&replacement) {
            return Err(to_js("column is outside the table".into()));
        }
        let width_twips = width_twips.clamp(1, 31_680);
        let columns = table_column_count(&replacement);
        if replacement.grid.len() < columns {
            replacement
                .grid
                .resize(columns, GridColumn { width_twips: None });
        }
        if let Some(grid_col) = replacement.grid.get_mut(col) {
            grid_col.width_twips = Some(width_twips);
        }
        for row in &mut replacement.rows {
            if let Some(cell) = row.cells.get_mut(col) {
                cell.properties.width_twips = Some(width_twips);
            }
        }
        self.apply_action_as(
            vec![Operation::ReplaceTable {
                table,
                replacement: Box::new(replacement),
            }],
            HistoryKind::TableResize,
        )
        .map_err(to_js)
    }

    /// Distributes the current preferred width evenly across every regular
    /// table column, preserving the total width as one undoable resize.
    #[wasm_bindgen(js_name = distributeTableColumns)]
    pub fn distribute_table_columns(&mut self, node: &str) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, _) = locate_table_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let mut replacement = find_table(&self.document, table)
            .ok_or_else(|| to_js("table not found".into()))?
            .clone();
        if !table_is_regular(&replacement) {
            return Err(to_js("column distribution requires a regular table".into()));
        }
        let columns = table_column_count(&replacement);
        if columns < 2 {
            return Err(to_js(
                "column distribution requires at least two columns".into(),
            ));
        }
        let widths = replacement
            .grid
            .iter()
            .take(columns)
            .map(|column| column.width_twips)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| to_js("column distribution requires explicit column widths".into()))?;
        let total = widths.iter().map(|width| i64::from(*width)).sum::<i64>();
        let distributed = distribute_twips(total, columns)?;
        replacement
            .grid
            .resize(columns, GridColumn { width_twips: None });
        for (column, width) in distributed.iter().enumerate() {
            replacement.grid[column].width_twips = Some(*width);
            for row in &mut replacement.rows {
                row.cells[column].properties.width_twips = Some(*width);
            }
        }
        self.apply_action_as(
            vec![Operation::ReplaceTable {
                table,
                replacement: Box::new(replacement),
            }],
            HistoryKind::TableResize,
        )
        .map_err(to_js)
    }

    /// Distributes explicit row heights evenly across every regular table row.
    /// Auto-height rows are refused because assigning an arbitrary height can
    /// clip content without a deterministic layout measurement.
    #[wasm_bindgen(js_name = distributeTableRows)]
    pub fn distribute_table_rows(&mut self, node: &str) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, _, _) = locate_table_row(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let mut replacement = find_table(&self.document, table)
            .ok_or_else(|| to_js("table not found".into()))?
            .clone();
        if !table_is_regular(&replacement) {
            return Err(to_js("row distribution requires a regular table".into()));
        }
        let rows = replacement.rows.len();
        if rows < 2 {
            return Err(to_js("row distribution requires at least two rows".into()));
        }
        let rule = replacement.rows[0]
            .properties
            .height
            .rule
            .filter(|rule| matches!(rule, HeightRule::AtLeast | HeightRule::Exact))
            .ok_or_else(|| to_js("row distribution requires explicit row heights".into()))?;
        let heights = replacement
            .rows
            .iter()
            .map(|row| {
                let height = row.properties.height;
                if height.rule != Some(rule) {
                    return Err(to_js(
                        "all rows must use the same explicit height rule".into(),
                    ));
                }
                height
                    .value_twips
                    .filter(|value| *value > 0)
                    .map(i64::from)
                    .ok_or_else(|| to_js("row distribution requires explicit row heights".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let total = heights.iter().sum::<i64>();
        let distributed = distribute_twips(total, rows)?;
        for (row, height) in replacement.rows.iter_mut().zip(distributed) {
            row.properties.height = RowHeight {
                value_twips: Some(height as u32),
                rule: Some(rule),
            };
        }
        self.apply_action_as(
            vec![Operation::ReplaceTable {
                table,
                replacement: Box::new(replacement),
            }],
            HistoryKind::TableResize,
        )
        .map_err(to_js)
    }

    /// Sets or clears the current row's repeat-header flag.
    #[wasm_bindgen(js_name = setTableHeaderRow)]
    pub fn set_table_header_row(
        &mut self,
        node: &str,
        enabled: bool,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, row, _) = locate_table_row(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let mut replacement = find_table(&self.document, table)
            .ok_or_else(|| to_js("table not found".into()))?
            .clone();
        replacement
            .rows
            .get_mut(row as usize)
            .ok_or_else(|| to_js("row is outside the table".into()))?
            .properties
            .header = enabled;
        self.apply_action(vec![Operation::ReplaceTable {
            table,
            replacement: Box::new(replacement),
        }])
        .map_err(to_js)
    }

    /// Sets the active row height: `"auto"` clears the explicit value,
    /// `"atLeast"` stores a minimum, and `"exact"` stores an exact height.
    #[wasm_bindgen(js_name = setTableRowHeight)]
    pub fn set_table_row_height(
        &mut self,
        node: &str,
        height_twips: i32,
        rule: &str,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, row, _) = locate_table_row(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let mut replacement = find_table(&self.document, table)
            .ok_or_else(|| to_js("table not found".into()))?
            .clone();
        let height = match rule {
            "exact" => RowHeight {
                value_twips: Some(height_twips.clamp(0, 31_680) as u32),
                rule: Some(HeightRule::Exact),
            },
            "atLeast" | "at_least" => RowHeight {
                value_twips: Some(height_twips.clamp(0, 31_680) as u32),
                rule: Some(HeightRule::AtLeast),
            },
            _ => RowHeight::default(),
        };
        replacement
            .rows
            .get_mut(row as usize)
            .ok_or_else(|| to_js("row is outside the table".into()))?
            .properties
            .height = height;
        self.apply_action(vec![Operation::ReplaceTable {
            table,
            replacement: Box::new(replacement),
        }])
        .map_err(to_js)
    }

    /// The named table styles defined by the document, sorted for a gallery.
    #[wasm_bindgen(js_name = listTableStyles)]
    #[must_use]
    pub fn list_table_styles(&self) -> Vec<String> {
        let mut names = self
            .document
            .definitions()
            .styles
            .iter()
            .map(|(_, style)| style)
            .filter(|style| style.kind == StyleKind::Table && !style.hidden)
            .filter_map(|style| style.name.clone())
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        names
    }

    #[wasm_bindgen(js_name = tableStyleAt)]
    #[must_use]
    pub fn table_style_at(&self, node: &str) -> String {
        let Ok(nid) = NodeId::from_str(node) else {
            return String::new();
        };
        let Some((table, _)) = locate_table_cell(&self.document, nid) else {
            return String::new();
        };
        find_table(&self.document, table)
            .and_then(|table| table.properties.style_ref)
            .and_then(|id| self.document.definitions().styles.get(&id))
            .and_then(|style| style.name.clone())
            .unwrap_or_default()
    }

    /// Applies a named table style, or clears the table style when `name` is empty.
    #[wasm_bindgen(js_name = applyTableStyle)]
    pub fn apply_table_style(&mut self, node: &str, name: &str) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, _) = locate_table_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let style_ref = if name.is_empty() {
            None
        } else {
            let id = self
                .document
                .definitions()
                .styles
                .iter()
                .find(|(_, style)| {
                    style.kind == StyleKind::Table && style.name.as_deref() == Some(name)
                })
                .map(|(id, _)| *id)
                .ok_or_else(|| to_js(format!("no table style named {name:?}")))?;
            Some(id)
        };
        let mut properties = find_table(&self.document, table)
            .ok_or_else(|| to_js("table not found".into()))?
            .properties
            .clone();
        properties.style_ref = style_ref;
        self.apply_action(vec![Operation::SetTableProperties {
            table,
            properties: Box::new(properties),
        }])
        .map_err(to_js)
    }

    /// Sorts regular-table rows by the first cell's plain text as one undoable edit.
    #[wasm_bindgen(js_name = sortTable)]
    pub fn sort_table(&mut self, node: &str, direction: &str) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, _) = locate_table_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let mut replacement = find_table(&self.document, table)
            .ok_or_else(|| to_js("table not found".into()))?
            .clone();
        if !table_is_regular(&replacement) {
            return Err(to_js("sorting requires a regular table".into()));
        }
        let descending = match direction {
            "ascending" | "asc" => false,
            "descending" | "desc" => true,
            _ => {
                return Err(to_js(
                    "sort direction must be ascending or descending".into(),
                ));
            }
        };
        let header = replacement
            .rows
            .first()
            .is_some_and(|row| row.properties.header);
        let start = usize::from(header);
        if replacement.rows.len().saturating_sub(start) < 2 {
            return Err(to_js("sorting requires at least two data rows".into()));
        }
        let mut keyed = replacement.rows.split_off(start);
        for row in &keyed {
            first_cell_plain_text(row).map_err(to_js)?;
        }
        keyed.sort_by(|left, right| {
            let left_key = first_cell_plain_text(left)
                .unwrap_or_default()
                .to_lowercase();
            let right_key = first_cell_plain_text(right)
                .unwrap_or_default()
                .to_lowercase();
            let order = left_key.cmp(&right_key);
            if descending { order.reverse() } else { order }
        });
        replacement.rows.extend(keyed);
        self.apply_action_as(
            vec![Operation::ReplaceTable {
                table,
                replacement: Box::new(replacement),
            }],
            HistoryKind::TableStructure,
        )
        .map_err(to_js)
    }

    /// Calculates a bounded table formula and writes its numeric result into the
    /// active cell. Supported forms are `=SUM|AVERAGE|MIN|MAX(ABOVE|LEFT)`.
    #[wasm_bindgen(js_name = calculateTableFormula)]
    pub fn calculate_table_formula(
        &mut self,
        node: &str,
        formula: &str,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, row, _) = locate_table_row(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let (_, column) = locate_table_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let mut replacement = find_table(&self.document, table)
            .ok_or_else(|| to_js("table not found".into()))?
            .clone();
        if !table_is_regular(&replacement) {
            return Err(to_js("formulas require a regular table".into()));
        }
        let (operation, range) = parse_table_formula(formula)?;
        let values = formula_values(&replacement, row as usize, column as usize, &range)?;
        if values.is_empty() {
            return Err(to_js("formula range contains no numeric cells".into()));
        }
        let result = match operation.as_str() {
            "SUM" => values.iter().sum::<f64>(),
            "AVERAGE" => values.iter().sum::<f64>() / values.len() as f64,
            "MIN" => values.iter().copied().fold(f64::INFINITY, f64::min),
            "MAX" => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            _ => return Err(to_js("unsupported table formula operation".into())),
        };
        let cell = replacement
            .rows
            .get_mut(row as usize)
            .and_then(|r| r.cells.get_mut(column as usize))
            .ok_or_else(|| to_js("formula cell is outside the table".into()))?;
        let paragraph = cell
            .blocks
            .first_mut()
            .and_then(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph),
                _ => None,
            })
            .ok_or_else(|| to_js("formula cell must begin with a paragraph".into()))?;
        let run_id = paragraph
            .inlines
            .iter()
            .find_map(|inline| match inline {
                InlineNode::Run(run) => Some(run.id),
                _ => None,
            })
            .unwrap_or(
                self.edit_ids
                    .next_id()
                    .map_err(|_| to_js("id space exhausted".into()))?,
            );
        paragraph.inlines = vec![InlineNode::Run(Run {
            id: run_id,
            properties: RunProperties::default(),
            text: format_number(result),
        })];
        self.apply_action_as(
            vec![Operation::ReplaceTable {
                table,
                replacement: Box::new(replacement),
            }],
            HistoryKind::TableFormatting,
        )
        .map_err(to_js)
    }

    /// Sets the table's preferred width in twips, or clears it when negative.
    #[wasm_bindgen(js_name = setTableWidth)]
    pub fn set_table_width(&mut self, node: &str, width_twips: i32) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, _) = locate_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let mut props = find_table(&self.document, table)
            .map(|t| t.properties.clone())
            .ok_or_else(|| to_js("table not found".into()))?;
        props.width_twips = (width_twips >= 0).then_some(width_twips.clamp(0, 31_680));
        self.apply_action(vec![Operation::SetTableProperties {
            table,
            properties: Box::new(props),
        }])
        .map_err(to_js)
    }

    /// Sets the table leading indent in twips.
    #[wasm_bindgen(js_name = setTableIndent)]
    pub fn set_table_indent(
        &mut self,
        node: &str,
        indent_twips: i32,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, _) = locate_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let mut props = find_table(&self.document, table)
            .map(|t| t.properties.clone())
            .ok_or_else(|| to_js("table not found".into()))?;
        props.indent_twips = Some(indent_twips.clamp(-31_680, 31_680));
        self.apply_action(vec![Operation::SetTableProperties {
            table,
            properties: Box::new(props),
        }])
        .map_err(to_js)
    }

    /// Switches table layout between fixed-width and AutoFit-style behavior.
    #[wasm_bindgen(js_name = setTableFixedLayout)]
    pub fn set_table_fixed_layout(
        &mut self,
        node: &str,
        fixed: bool,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, _) = locate_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let mut props = find_table(&self.document, table)
            .map(|t| t.properties.clone())
            .ok_or_else(|| to_js("table not found".into()))?;
        props.layout = Some(if fixed {
            TableLayout::Fixed
        } else {
            TableLayout::Autofit
        });
        self.apply_action(vec![Operation::SetTableProperties {
            table,
            properties: Box::new(props),
        }])
        .map_err(to_js)
    }

    /// Sets uniform default cell margins on the active table; a negative value
    /// clears the table-level default margins.
    #[wasm_bindgen(js_name = setTableCellMargins)]
    pub fn set_table_cell_margins(
        &mut self,
        node: &str,
        margin_twips: i32,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, _) = locate_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let mut props = find_table(&self.document, table)
            .map(|t| t.properties.clone())
            .ok_or_else(|| to_js("table not found".into()))?;
        props.cell_margins = if margin_twips < 0 {
            CellMargins::default()
        } else {
            let twips = margin_twips.clamp(0, 31_680);
            CellMargins {
                top_twips: Some(twips),
                start_twips: Some(twips),
                bottom_twips: Some(twips),
                end_twips: Some(twips),
            }
        };
        self.apply_action(vec![Operation::SetTableProperties {
            table,
            properties: Box::new(props),
        }])
        .map_err(to_js)
    }

    /// Sets default cell spacing on the active table; a negative value clears it.
    #[wasm_bindgen(js_name = setTableCellSpacing)]
    pub fn set_table_cell_spacing(
        &mut self,
        node: &str,
        spacing_twips: i32,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, _) = locate_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let mut props = find_table(&self.document, table)
            .map(|t| t.properties.clone())
            .ok_or_else(|| to_js("table not found".into()))?;
        props.cell_spacing_twips = (spacing_twips >= 0).then_some(spacing_twips.clamp(0, 31_680));
        self.apply_action(vec![Operation::SetTableProperties {
            table,
            properties: Box::new(props),
        }])
        .map_err(to_js)
    }

    /// Moves the caret to the next/previous editable cell in the current innermost
    /// table. Pure navigation: no edit and no row creation at table boundaries.
    #[wasm_bindgen(js_name = moveTableCell)]
    pub fn move_table_cell(&self, node: &str, forward: bool) -> Result<Caret, JsValue> {
        let nid = node_id(node)?;
        let (table, row, _) = locate_table_row(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let (_, col) = locate_table_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let target = self
            .table_cell_anchor(table, row, col, forward)
            .ok_or_else(|| to_js("no adjacent table cell".into()))?;
        Ok(Caret {
            node: target.to_string(),
            offset: 0,
        })
    }

    /// The active cell's border box as a flat `[page, x, y, w, h]` in page-local
    /// twips (empty when the caret is not in a table) — the geometry the frontend
    /// draws the active-cell highlight from.
    #[wasm_bindgen(js_name = cellRect)]
    #[must_use]
    pub fn cell_rect(&self, node: &str) -> Vec<i32> {
        let Ok(nid) = NodeId::from_str(node) else {
            return Vec::new();
        };
        LayoutSnapshot::new(&self.layout)
            .cell_rect(nid)
            .map(|(page, rect)| flat_rect(page, rect).to_vec())
            .unwrap_or_default()
    }

    /// Cell rectangles for a table selection mode (`"row"`, `"column"`, `"table"`)
    /// around `node`, flattened as `[page, x, y, w, h, …]`. This deliberately
    /// reports cell boxes, not text ranges: table column selection is not a
    /// contiguous document text range.
    #[wasm_bindgen(js_name = tableSelectionRects)]
    #[must_use]
    pub fn table_selection_rects(&self, node: &str, mode: &str) -> Vec<i32> {
        let Ok(nid) = NodeId::from_str(node) else {
            return Vec::new();
        };
        let Some((table, row, _)) = locate_table_row(&self.document, nid) else {
            return Vec::new();
        };
        let Some((_, col)) = locate_table_cell(&self.document, nid) else {
            return Vec::new();
        };
        let Some(t) = find_table(&self.document, table) else {
            return Vec::new();
        };
        let layout = LayoutSnapshot::new(&self.layout);
        let mut out = Vec::new();
        for anchor in table_selection_anchors(t, row as usize, col as usize, mode) {
            if let Some((page, rect)) = layout.cell_rect(anchor) {
                out.extend_from_slice(&flat_rect(page, rect));
            }
        }
        out
    }

    /// Internal column-resize edges for the active regular table, flattened as
    /// `[page, x, y, h, column, …]` in page-local twips. `column` is the column to
    /// the left of the edge; resizing it preserves the rest of the table.
    #[wasm_bindgen(js_name = tableColumnResizeHandles)]
    #[must_use]
    pub fn table_column_resize_handles(&self, node: &str) -> Vec<i32> {
        let Ok(nid) = NodeId::from_str(node) else {
            return Vec::new();
        };
        let Some((table, _, _)) = locate_table_row(&self.document, nid) else {
            return Vec::new();
        };
        let Some(t) = find_table(&self.document, table) else {
            return Vec::new();
        };
        if !table_is_regular(t) {
            return Vec::new();
        }
        let cols = table_column_count(t);
        if cols < 2 {
            return Vec::new();
        }
        let layout = LayoutSnapshot::new(&self.layout);
        let mut edges: BTreeMap<(u32, usize), (i32, i32, i32)> = BTreeMap::new();
        for row in &t.rows {
            for col in 0..cols - 1 {
                let Some(anchor) = row.cells.get(col).and_then(first_paragraph_of_cell) else {
                    continue;
                };
                let Some((page, rect)) = layout.cell_rect(anchor) else {
                    continue;
                };
                let x = rect.origin.x.raw() + rect.size.width.raw();
                let y0 = rect.origin.y.raw();
                let y1 = y0 + rect.size.height.raw();
                edges
                    .entry((page, col))
                    .and_modify(|(edge_x, top, bottom)| {
                        *edge_x = x;
                        *top = (*top).min(y0);
                        *bottom = (*bottom).max(y1);
                    })
                    .or_insert((x, y0, y1));
            }
        }
        let mut out = Vec::with_capacity(edges.len() * 5);
        for ((page, col), (x, y0, y1)) in edges {
            out.extend_from_slice(&[page as i32, x, y0, y1 - y0, col as i32]);
        }
        out
    }

    /// Merges the active table selection mode (`"row"`, `"column"`, `"table"`) in
    /// a regular table. Selected-cell content is preserved by moving it into the
    /// top-left merged cell in row-major order.
    #[wasm_bindgen(js_name = mergeTableSelection)]
    pub fn merge_table_selection(&mut self, node: &str, mode: &str) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, row, _) = locate_table_row(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let (_, col) = locate_table_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let original = find_table(&self.document, table)
            .ok_or_else(|| to_js("table not found".into()))?
            .clone();
        if !table_is_regular(&original) {
            return Err(to_js("merge requires a regular table".into()));
        }
        let rows = original.rows.len();
        let cols = table_column_count(&original);
        let (r0, r1, c0, c1) = match mode {
            "row" => (row as usize, row as usize, 0, cols.saturating_sub(1)),
            "column" => (0, rows.saturating_sub(1), col as usize, col as usize),
            "table" => (0, rows.saturating_sub(1), 0, cols.saturating_sub(1)),
            _ => return Err(to_js("unknown table selection mode".into())),
        };
        if r0 == r1 && c0 == c1 {
            return Err(to_js("select at least two cells to merge".into()));
        }
        let replacement =
            merge_regular_table_selection(original, r0, r1, c0, c1, &mut self.edit_ids)
                .map_err(to_js)?;
        let caret = replacement
            .rows
            .get(r0)
            .and_then(|r| r.cells.get(c0))
            .and_then(first_paragraph_of_cell)
            .map_or(Pos::new(nid, 0), |p| Pos::new(p, 0));
        self.apply_action_caret(
            vec![Operation::ReplaceTable {
                table,
                replacement: Box::new(replacement),
            }],
            caret,
        )
        .map_err(to_js)
    }

    /// Splits the active merged cell in the current regularized merged table back
    /// into normal cells. The top-left cell keeps the merged content; recreated
    /// cells are empty but valid.
    #[wasm_bindgen(js_name = splitMergedCell)]
    pub fn split_merged_cell(
        &mut self,
        node: &str,
        requested_rows: Option<u32>,
        requested_columns: Option<u32>,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, row, _) = locate_table_row(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let (_, col) = locate_table_cell(&self.document, nid)
            .ok_or_else(|| to_js("caret is not inside a table".into()))?;
        let original = find_table(&self.document, table)
            .ok_or_else(|| to_js("table not found".into()))?
            .clone();
        let replacement = split_table_cell_counts(
            original,
            row as usize,
            col as usize,
            requested_rows.unwrap_or(0) as usize,
            requested_columns.unwrap_or(0) as usize,
            &mut self.edit_ids,
        )
        .map_err(to_js)?;
        let caret = replacement
            .rows
            .get(row as usize)
            .and_then(|r| r.cells.get(col as usize))
            .and_then(first_paragraph_of_cell)
            .map_or(Pos::new(nid, 0), |p| Pos::new(p, 0));
        self.apply_action_caret(
            vec![Operation::ReplaceTable {
                table,
                replacement: Box::new(replacement),
            }],
            caret,
        )
        .map_err(to_js)
    }

    /// Sets or clears the background shading fill of the cell containing `node`.
    #[wasm_bindgen(js_name = setCellShading)]
    pub fn set_cell_shading(
        &mut self,
        node: &str,
        r: u8,
        g: u8,
        b: u8,
        clear: bool,
    ) -> Result<EditResult, JsValue> {
        self.apply_cell_props(node, |p| {
            p.shading.fill = if clear {
                None
            } else {
                Some(RgbColor { r, g, b })
            };
        })
    }

    /// Sets the cell's vertical text alignment: `"top"` | `"center"` | `"bottom"`.
    #[wasm_bindgen(js_name = setCellVerticalAlign)]
    pub fn set_cell_vertical_align(
        &mut self,
        node: &str,
        align: &str,
    ) -> Result<EditResult, JsValue> {
        let va = match align {
            "center" => CellVerticalAlignment::Center,
            "bottom" => CellVerticalAlignment::Bottom,
            _ => CellVerticalAlignment::Top,
        };
        self.apply_cell_props(node, move |p| p.vertical_alignment = Some(va))
    }

    /// Applies a border `edges` preset to the cell containing `node`: `"none"`/`"box"`
    /// clear-or-set the four cell edges; `"top"`/`"bottom"`/`"left"`/`"right"` toggle
    /// one edge. Set edges use a single line of `size_eighth_points` in the RGB.
    #[wasm_bindgen(js_name = setCellBorder)]
    #[allow(clippy::too_many_arguments)] // flat JS signature (node + preset + rgb + size)
    pub fn set_cell_border(
        &mut self,
        node: &str,
        edges: &str,
        r: u8,
        g: u8,
        b: u8,
        size_eighth_points: u32,
    ) -> Result<EditResult, JsValue> {
        let edges = edges.to_string();
        self.apply_cell_props(node, move |p| {
            set_table_borders_preset(&mut p.borders, &edges, || {
                border_edge(r, g, b, size_eighth_points)
            });
        })
    }

    /// Applies a border `edges` preset to the whole table containing `node`:
    /// `"none"` clears all; `"box"` sets the four outer edges; `"all"` sets outer +
    /// inside gridlines; `"top"`/`"bottom"`/`"left"`/`"right"` toggle one outer edge.
    #[wasm_bindgen(js_name = setTableBorder)]
    #[allow(clippy::too_many_arguments)] // flat JS signature (node + preset + rgb + size)
    pub fn set_table_border(
        &mut self,
        node: &str,
        edges: &str,
        r: u8,
        g: u8,
        b: u8,
        size_eighth_points: u32,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (table, _cell) =
            locate_cell(&self.document, nid).ok_or_else(|| to_js("not in a table".into()))?;
        let mut props = find_table(&self.document, table)
            .map(|t| t.properties.clone())
            .ok_or_else(|| to_js("table not found".into()))?;
        set_table_borders_preset(&mut props.borders, edges, || {
            border_edge(r, g, b, size_eighth_points)
        });
        self.apply_action(vec![Operation::SetTableProperties {
            table,
            properties: Box::new(props),
        }])
        .map_err(to_js)
    }

    /// The shading fill of the cell containing `node` as packed `0xRRGGBB`, or `-1`.
    #[wasm_bindgen(js_name = cellShadingAt)]
    #[must_use]
    pub fn cell_shading_at(&self, node: &str) -> i32 {
        self.cell_props_of(node)
            .and_then(|p| p.shading.fill)
            .map_or(-1, |c| {
                (i32::from(c.r) << 16) | (i32::from(c.g) << 8) | i32::from(c.b)
            })
    }

    /// The vertical alignment of the cell containing `node`: `"top"`/`"center"`/
    /// `"bottom"` (defaults to `"top"`); `""` when not in a cell.
    #[wasm_bindgen(js_name = cellVerticalAlignAt)]
    #[must_use]
    pub fn cell_vertical_align_at(&self, node: &str) -> String {
        match self.cell_props_of(node) {
            Some(p) => match p.vertical_alignment {
                Some(CellVerticalAlignment::Center) => "center",
                Some(CellVerticalAlignment::Bottom) => "bottom",
                _ => "top",
            }
            .to_string(),
            None => String::new(),
        }
    }

    /// The cell's border edges as a bitmask (top=1, bottom=2, left=4, right=8) — for
    /// reflecting the active cell-border presets.
    #[wasm_bindgen(js_name = cellBorderEdges)]
    #[must_use]
    pub fn cell_border_edges(&self, node: &str) -> u8 {
        self.cell_props_of(node).map_or(0, |p| {
            let bd = &p.borders;
            u8::from(bd.top.is_some())
                | (u8::from(bd.bottom.is_some()) << 1)
                | (u8::from(bd.start.is_some()) << 2)
                | (u8::from(bd.end.is_some()) << 3)
        })
    }

    /// The first section's page geometry (width + side margins, in twips) — what the
    /// horizontal ruler draws its scale and margin zones from.
    #[wasm_bindgen(js_name = pageGeometry)]
    #[must_use]
    pub fn page_geometry(&self) -> RulerGeometry {
        let c = &self.default_config;
        RulerGeometry {
            width_twip: c.page_size.width.raw(),
            margin_start_twip: c.margin_start.raw(),
            margin_end_twip: c.margin_end.raw(),
        }
    }

    /// The first section's full page-setup geometry (page size, margins, and
    /// orientation) as a JSON object — the "Page Setup" dialog's read side.
    /// `null` if the document has no section (should not occur for an
    /// imported DOCX, which always carries at least one).
    #[wasm_bindgen(js_name = pageSetup)]
    #[must_use]
    pub fn page_setup(&self) -> String {
        let Some(section) = self.document.definitions().sections.first() else {
            return "null".to_string();
        };
        let payload = PageSetupJson {
            section: section.id.node_id().to_string(),
            page_size: section.page_size,
            page_margins: section.page_margins,
            orientation: section.orientation,
        };
        serde_json::to_string(&payload).unwrap_or_else(|_| "null".to_string())
    }

    /// Installs a new page-setup geometry from a JSON object in the shape
    /// [`page_setup`](Self::page_setup) returns — the dialog's write side. One
    /// undoable action.
    #[wasm_bindgen(js_name = setPageSetup)]
    pub fn set_page_setup(&mut self, page_setup_json: &str) -> Result<EditResult, JsValue> {
        let payload: PageSetupJson = serde_json::from_str(page_setup_json)
            .map_err(|e| to_js(format!("invalid page setup payload: {e}")))?;
        let section = NodeId::from_str(&payload.section)
            .map(SectionId::new)
            .map_err(|_| to_js("invalid section id".to_string()))?;
        let caret = Pos::new(self.document.id(), 0);
        self.apply_action_caret(
            vec![Operation::SetSectionGeometry {
                section,
                page_size: payload.page_size,
                page_margins: payload.page_margins,
                orientation: payload.orientation,
            }],
            caret,
        )
        .map_err(to_js)
    }

    /// The paragraph's indentation (left/right/first-line/hanging, in twips; 0 when
    /// unset) — for positioning the ruler's indent markers.
    #[wasm_bindgen(js_name = paragraphIndent)]
    #[must_use]
    pub fn paragraph_indent(&self, node: &str) -> Indents {
        let ind = NodeId::from_str(node)
            .ok()
            .and_then(|nid| paragraph_properties(&self.document, nid))
            .and_then(|p| p.indentation);
        match ind {
            Some(i) => Indents {
                start_twip: i.start_twips.unwrap_or(0),
                end_twip: i.end_twips.unwrap_or(0),
                first_line_twip: i.first_line_twips.unwrap_or(0),
                hanging_twip: i.hanging_twips.unwrap_or(0),
            },
            None => Indents::default(),
        }
    }

    /// The paragraph's spacing (space before/after in twips, and line spacing) — for
    /// the toolbar's line-&-paragraph-spacing menu to reflect current state. `-1`
    /// means unset for before/after; `line_rule` is `0` auto (percent), `1` atLeast,
    /// `2` exact (twips).
    #[wasm_bindgen(js_name = paragraphSpacing)]
    #[must_use]
    pub fn paragraph_spacing(&self, node: &str) -> ParagraphSpacing {
        let spacing = NodeId::from_str(node)
            .ok()
            .and_then(|nid| paragraph_properties(&self.document, nid))
            .and_then(|p| p.spacing);
        match spacing {
            Some(s) => ParagraphSpacing {
                before_twip: s.before_twips.unwrap_or(-1),
                after_twip: s.after_twips.unwrap_or(-1),
                line_percent: s.line_percent.map_or(0, u32::from),
                line_rule: match s.line_rule {
                    Some(casual_doc_model::v1::LineRule::AtLeast) => 1,
                    Some(casual_doc_model::v1::LineRule::Exact) => 2,
                    _ => 0,
                },
                line_twip: s.line_twips.unwrap_or(0),
            },
            None => ParagraphSpacing::default(),
        }
    }

    /// Uniform paragraph properties over every paragraph touched by the model
    /// selection. Each value has an explicit mixed flag; booleans use state
    /// 0=off, 1=on, 2=mixed.
    #[wasm_bindgen(js_name = selectionParagraphState)]
    #[must_use]
    pub fn selection_paragraph_state(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> ParagraphState {
        let Ok((start, end)) = self.order_endpoints(start_node, start_offset, end_node, end_offset)
        else {
            return ParagraphState::default();
        };
        let properties: Vec<ParagraphProperties> = self
            .paragraphs_in_selection(start, end)
            .into_iter()
            .filter_map(|node| paragraph_properties(&self.document, node))
            .collect();
        if properties.is_empty() {
            return ParagraphState::default();
        }

        let alignment = uniform_slice(
            &properties
                .iter()
                .map(|p| p.alignment.unwrap_or(Alignment::Start))
                .collect::<Vec<_>>(),
        );
        let style = uniform_slice(
            &properties
                .iter()
                .map(|p| {
                    p.style_ref
                        .and_then(|id| self.document.definitions().styles.get(&id))
                        .and_then(|style| style.name.clone())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>(),
        );
        let indentation = |field: fn(Indentation) -> Option<i32>| {
            uniform_slice(
                &properties
                    .iter()
                    .map(|p| p.indentation.and_then(field).unwrap_or(0))
                    .collect::<Vec<_>>(),
            )
        };
        let spacing = |field: fn(Spacing) -> i32, default: i32| {
            uniform_slice(
                &properties
                    .iter()
                    .map(|p| p.spacing.map(field).unwrap_or(default))
                    .collect::<Vec<_>>(),
            )
        };
        let start_indent = indentation(|value| value.start_twips);
        let end_indent = indentation(|value| value.end_twips);
        let first_indent = indentation(|value| value.first_line_twips);
        let hanging_indent = indentation(|value| value.hanging_twips);
        let before = spacing(|value| value.before_twips.unwrap_or(-1), -1);
        let after = spacing(|value| value.after_twips.unwrap_or(-1), -1);
        let line_percent = spacing(
            |value| {
                if matches!(
                    value.line_rule,
                    None | Some(casual_doc_model::v1::LineRule::Auto)
                ) {
                    value.line_percent.map_or(0, i32::from)
                } else {
                    0
                }
            },
            0,
        );
        let keep_next = bool_selection_state(&properties, |p| p.keep_next);
        let keep_lines = bool_selection_state(&properties, |p| p.keep_lines);
        let page_break_before = bool_selection_state(&properties, |p| p.page_break_before);
        let shading = uniform_slice(
            &properties
                .iter()
                .map(|p| p.shading.fill)
                .collect::<Vec<_>>(),
        );
        let borders = uniform_slice(
            &properties
                .iter()
                .map(|p| paragraph_border_mask(&p.borders))
                .collect::<Vec<_>>(),
        );

        ParagraphState {
            count: properties.len() as u32,
            alignment: alignment.map(alignment_name).unwrap_or_default().to_owned(),
            alignment_mixed: alignment.is_none(),
            style: style.clone().unwrap_or_default(),
            style_mixed: style.is_none(),
            start_twip: start_indent.unwrap_or_default(),
            start_mixed: start_indent.is_none(),
            end_twip: end_indent.unwrap_or_default(),
            end_mixed: end_indent.is_none(),
            first_line_twip: first_indent.unwrap_or_default(),
            first_line_mixed: first_indent.is_none(),
            hanging_twip: hanging_indent.unwrap_or_default(),
            hanging_mixed: hanging_indent.is_none(),
            before_twip: before.unwrap_or(-1),
            before_mixed: before.is_none(),
            after_twip: after.unwrap_or(-1),
            after_mixed: after.is_none(),
            line_percent: line_percent.unwrap_or_default().max(0) as u32,
            line_mixed: line_percent.is_none(),
            keep_next,
            keep_lines,
            page_break_before,
            shading: shading.flatten().map_or(-1, |color| pack_rgb(color) as i32),
            shading_mixed: shading.is_none(),
            border_edges: borders.unwrap_or_default(),
            borders_mixed: borders.is_none(),
        }
    }

    /// Document statistics for the status footer: word count (whitespace-delimited
    /// tokens across every paragraph, body + table cells + content controls),
    /// paragraph count, and page count.
    #[wasm_bindgen(js_name = documentStats)]
    #[must_use]
    pub fn document_stats(&self) -> DocStats {
        let mut nodes = Vec::new();
        collect_block_text(self.document.body(), &mut nodes);
        DocStats {
            words: nodes
                .iter()
                .map(|(_, t)| t.split_whitespace().count() as u32)
                .sum(),
            paragraphs: nodes.len() as u32,
            pages: self.layout.page_count() as u32,
        }
    }

    /// The document's core properties (`docProps/core.xml` — title, author,
    /// subject, …) as a JSON object matching the model's own camelCase field
    /// names (e.g. `{"title":"...","creator":"...","lastModifiedBy":null,...}`).
    /// A field is `null` when unset. Empty (`{}`) when the document carries no
    /// core properties at all.
    #[wasm_bindgen(js_name = documentProperties)]
    #[must_use]
    pub fn document_properties(&self) -> String {
        let core = self
            .document
            .properties()
            .map(|p| p.core.clone())
            .unwrap_or_default();
        serde_json::to_string(&core).unwrap_or_else(|_| "{}".to_string())
    }

    /// All modeled DOCX metadata (`docProps/core.xml`, `app.xml`, and
    /// `custom.xml`) as one read-only JSON object. This complements
    /// [`document_properties`](Self::document_properties), whose core-only
    /// payload remains the stable mutation shape used by
    /// [`set_document_properties`](Self::set_document_properties).
    #[wasm_bindgen(js_name = documentMetadata)]
    #[must_use]
    pub fn document_metadata(&self) -> String {
        let metadata = self.document.properties().cloned().unwrap_or_default();
        serde_json::to_string(&serde_json::json!({
            "core": metadata.core,
            "app": metadata.app,
            "custom": metadata.custom,
        }))
        .unwrap_or_else(|_| r#"{"core":{},"app":{},"custom":[]}"#.to_string())
    }

    /// Replaces the document's core properties from a JSON object in the same
    /// shape [`document_properties`](Self::document_properties) returns — an
    /// absent/`null` field clears that property. One undoable action.
    #[wasm_bindgen(js_name = setDocumentProperties)]
    pub fn set_document_properties(
        &mut self,
        properties_json: &str,
    ) -> Result<EditResult, JsValue> {
        let core: casual_doc_model::v1::CoreProperties = serde_json::from_str(properties_json)
            .map_err(|e| to_js(format!("invalid document properties payload: {e}")))?;
        let caret = Pos::new(self.document.id(), 0);
        self.apply_action_caret(
            vec![Operation::SetCoreProperties {
                properties: Box::new(core),
            }],
            caret,
        )
        .map_err(to_js)
    }

    /// Sets the paragraph background shading (`w:shd` fill) over the selection to
    /// an RGB, or clears it when `clear` is true.
    #[wasm_bindgen(js_name = setParagraphShading)]
    #[allow(clippy::too_many_arguments)] // flat JS signature (node/offsets + rgb + clear)
    pub fn set_paragraph_shading(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        r: u8,
        g: u8,
        b: u8,
        clear: bool,
    ) -> Result<EditResult, JsValue> {
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            p.shading.fill = if clear {
                None
            } else {
                Some(RgbColor { r, g, b })
            };
        })
    }

    /// The paragraph's shading fill as a packed `0xRRGGBB` int, or `-1` when unset —
    /// for the paragraph-properties inspector's shading swatch.
    #[wasm_bindgen(js_name = paragraphShadingAt)]
    #[must_use]
    pub fn paragraph_shading_at(&self, node: &str) -> i32 {
        NodeId::from_str(node)
            .ok()
            .and_then(|nid| paragraph_properties(&self.document, nid))
            .and_then(|p| p.shading.fill)
            .map_or(-1, |c| {
                (i32::from(c.r) << 16) | (i32::from(c.g) << 8) | i32::from(c.b)
            })
    }

    /// Sets "keep with next" (`w:keepNext`) over the selection's paragraphs.
    #[wasm_bindgen(js_name = setKeepWithNext)]
    pub fn set_keep_with_next(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        on: bool,
    ) -> Result<EditResult, JsValue> {
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            p.keep_next = on;
        })
    }

    /// Sets "keep lines together" (`w:keepLines`) over the selection's paragraphs.
    #[wasm_bindgen(js_name = setKeepLinesTogether)]
    pub fn set_keep_lines_together(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        on: bool,
    ) -> Result<EditResult, JsValue> {
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            p.keep_lines = on;
        })
    }

    /// Sets "page break before" (`w:pageBreakBefore`) over the selection's paragraphs.
    #[wasm_bindgen(js_name = setPageBreakBefore)]
    pub fn set_page_break_before(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        on: bool,
    ) -> Result<EditResult, JsValue> {
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            p.page_break_before = on;
        })
    }

    /// The paragraph's line-and-page-break flags (keep-with-next, keep-lines,
    /// page-break-before) for reflecting the paragraph-properties inspector.
    #[wasm_bindgen(js_name = paragraphFlags)]
    #[must_use]
    pub fn paragraph_flags(&self, node: &str) -> ParagraphFlags {
        NodeId::from_str(node)
            .ok()
            .and_then(|nid| paragraph_properties(&self.document, nid))
            .map_or(ParagraphFlags::default(), |p| ParagraphFlags {
                keep_next: p.keep_next,
                keep_lines: p.keep_lines,
                page_break_before: p.page_break_before,
            })
    }

    /// Applies a paragraph border `edges` preset over the selection: `"none"` clears
    /// all four edges; `"box"` sets all four; `"top"`/`"bottom"`/`"left"`/`"right"`
    /// toggle that single edge (start = left, end = right). Set edges use a single
    /// line of `size_eighth_points` (eighth-points) in the given RGB.
    #[wasm_bindgen(js_name = setParagraphBorder)]
    #[allow(clippy::too_many_arguments)] // flat JS signature (node/offsets + preset + rgb + size)
    pub fn set_paragraph_border(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        edges: &str,
        r: u8,
        g: u8,
        b: u8,
        size_eighth_points: u32,
    ) -> Result<EditResult, JsValue> {
        let edge = || BorderEdge {
            style: "single".to_string(),
            size_eighth_points: Some(size_eighth_points.clamp(2, 96)),
            color: Some(RgbColor { r, g, b }),
            space_points: None,
        };
        let edges = edges.to_string();
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            let bd = &mut p.borders;
            // Toggle a single edge: on if currently absent, else cleared.
            let toggle = |slot: &mut Option<BorderEdge>| {
                *slot = if slot.is_none() { Some(edge()) } else { None };
            };
            match edges.as_str() {
                "none" => {
                    bd.top = None;
                    bd.bottom = None;
                    bd.start = None;
                    bd.end = None;
                }
                "box" => {
                    bd.top = Some(edge());
                    bd.bottom = Some(edge());
                    bd.start = Some(edge());
                    bd.end = Some(edge());
                }
                "top" => toggle(&mut bd.top),
                "bottom" => toggle(&mut bd.bottom),
                "left" => toggle(&mut bd.start),
                "right" => toggle(&mut bd.end),
                _ => {}
            }
        })
    }

    /// The paragraph's border edges as a bitmask — top=1, bottom=2, left=4, right=8 —
    /// for reflecting which edge presets are active in the paragraph menu.
    #[wasm_bindgen(js_name = paragraphBorderEdges)]
    #[must_use]
    pub fn paragraph_border_edges(&self, node: &str) -> u8 {
        NodeId::from_str(node)
            .ok()
            .and_then(|nid| paragraph_properties(&self.document, nid))
            .map_or(0, |p| {
                let bd = &p.borders;
                u8::from(bd.top.is_some())
                    | (u8::from(bd.bottom.is_some()) << 1)
                    | (u8::from(bd.start.is_some()) << 2)
                    | (u8::from(bd.end.is_some()) << 3)
            })
    }

    /// Adds or replaces a tab stop at `position_twips` (from the leading margin) with
    /// alignment `align_code` (0 start, 1 center, 2 end, 3 decimal) over the
    /// selection's paragraphs. Any existing stop at the same position is replaced;
    /// stops stay sorted by position.
    #[wasm_bindgen(js_name = setTabStop)]
    pub fn set_tab_stop(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        position_twips: i32,
        align_code: u8,
    ) -> Result<EditResult, JsValue> {
        let alignment = tab_alignment_from_code(align_code);
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            let leader = p
                .tabs
                .iter()
                .find(|t| t.position_twips == position_twips)
                .and_then(|t| t.leader);
            p.tabs.retain(|t| t.position_twips != position_twips);
            p.tabs.push(TabStop {
                position_twips,
                alignment,
                leader,
            });
            p.tabs.sort_by_key(|t| t.position_twips);
        })
    }

    /// Removes the tab stop at exactly `position_twips` over the selection.
    #[wasm_bindgen(js_name = removeTabStop)]
    pub fn remove_tab_stop(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        position_twips: i32,
    ) -> Result<EditResult, JsValue> {
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            p.tabs.retain(|t| t.position_twips != position_twips);
        })
    }

    /// Moves the tab stop at `from_twips` to `to_twips` (keeping its alignment) over
    /// the selection — one undoable action, for a ruler drag.
    #[wasm_bindgen(js_name = moveTabStop)]
    pub fn move_tab_stop(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        from_twips: i32,
        to_twips: i32,
    ) -> Result<EditResult, JsValue> {
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            if let Some(t) = p.tabs.iter_mut().find(|t| t.position_twips == from_twips) {
                t.position_twips = to_twips;
            }
            p.tabs.retain(|t| t.position_twips >= 0);
            p.tabs.sort_by_key(|t| t.position_twips);
        })
    }

    /// Clears every explicit tab stop over the selection.
    #[wasm_bindgen(js_name = clearTabStops)]
    pub fn clear_tab_stops(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> Result<EditResult, JsValue> {
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, |p| {
            p.tabs.clear();
        })
    }

    /// The paragraph's explicit tab stops as a flat `[pos0, code0, pos1, code1, …]`
    /// (position in twips, alignment code 0 start / 1 center / 2 end / 3 decimal /
    /// 4 bar) — what the ruler renders its tab glyphs from.
    #[wasm_bindgen(js_name = paragraphTabs)]
    #[must_use]
    pub fn paragraph_tabs(&self, node: &str) -> Vec<i32> {
        NodeId::from_str(node)
            .ok()
            .and_then(|nid| paragraph_properties(&self.document, nid))
            .map(|p| {
                p.tabs
                    .iter()
                    .flat_map(|t| [t.position_twips, tab_alignment_code(t.alignment)])
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The uniform effective run styling of the selection
    /// (size/color/authored-font/vert-align) for reflecting the current values in
    /// the toolbar. Blank/zero means mixed or unset.
    #[wasm_bindgen(js_name = selectionRunStyle)]
    #[must_use]
    pub fn selection_run_style(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> RunStyle {
        let Ok((start, end)) = self.order_endpoints(start_node, start_offset, end_node, end_offset)
        else {
            return RunStyle::default();
        };
        let mut properties = Vec::new();
        for (node, range_start, range_end) in self.selection_subranges(start, end) {
            properties.extend(effective_run_properties_in_range(
                &self.document,
                node,
                range_start,
                range_end,
            ));
        }
        run_style_from_effective_runs(&self.document, &properties)
    }

    /// The effective run styling a **collapsed caret** inherits — size / authored
    /// font / color / super-sub after document defaults and style inheritance.
    /// Renderer substitutions and glyph-coverage fallbacks are intentionally not
    /// exposed as authored formatting.
    #[wasm_bindgen(js_name = caretRunStyle)]
    #[must_use]
    pub fn caret_run_style(&self, node: &str, offset: u32) -> RunStyle {
        let Ok(nid) = NodeId::from_str(node) else {
            return RunStyle::default();
        };
        effective_caret_run_properties(&self.document, nid, offset)
            .map_or_else(RunStyle::default, |properties| {
                run_style_from_effective_runs(&self.document, &[properties])
            })
    }

    /// The line-spacing percentage of the paragraph at `node` (0 if unset) — for
    /// the toolbar's spacing dropdown.
    #[wasm_bindgen(js_name = lineSpacingAt)]
    #[must_use]
    pub fn line_spacing_at(&self, node: &str) -> u32 {
        NodeId::from_str(node)
            .ok()
            .and_then(|nid| paragraph_properties(&self.document, nid))
            .and_then(|p| p.spacing)
            .and_then(|s| s.line_percent)
            .map_or(0, u32::from)
    }

    /// The alignment of the first paragraph the selection touches (`"start"`,
    /// `"center"`, `"end"`, `"justify"`), or `"start"` if unset — for toolbar
    /// state.
    #[wasm_bindgen(js_name = alignmentAt)]
    #[must_use]
    pub fn alignment_at(&self, node: &str, offset: u32) -> String {
        let _ = offset;
        let Ok(nid) = NodeId::from_str(node) else {
            return "start".into();
        };
        match paragraph_properties(&self.document, nid).and_then(|p| p.alignment) {
            Some(Alignment::Center) => "center",
            Some(Alignment::End) => "end",
            Some(Alignment::Justify) => "justify",
            _ => "start",
        }
        .into()
    }

    /// Sets the font family over the selection (`w:rFonts`).
    #[wasm_bindgen(js_name = setFont)]
    pub fn set_font(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        family: String,
    ) -> Result<EditResult, JsValue> {
        if family.is_empty() {
            return Err(to_js("empty font family".into()));
        }
        self.apply_run_format(
            start_node,
            start_offset,
            end_node,
            end_offset,
            FormatDelta {
                font: Some(family),
                ..FormatDelta::default()
            },
        )
    }

    /// Sets the paragraph style (`w:pStyle`) over the selection's paragraphs to the
    /// style with the given name (e.g. `"Heading 1"`, `"Title"`, `"Normal"`), or
    /// clears it when `style_name` is empty. Throws if no such paragraph style
    /// exists in the document.
    #[wasm_bindgen(js_name = setParagraphStyle)]
    pub fn set_paragraph_style(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        style_name: &str,
    ) -> Result<EditResult, JsValue> {
        let style_ref = self.style_id_by_name(style_name);
        if !style_name.is_empty() && style_ref.is_none() {
            return Err(to_js(format!("no paragraph style named {style_name:?}")));
        }
        self.apply_paragraph_props(start_node, start_offset, end_node, end_offset, move |p| {
            p.style_ref = style_ref;
        })
    }

    /// The paragraph style names defined in the document (for a style dropdown),
    /// sorted and de-duplicated.
    #[wasm_bindgen(js_name = listStyles)]
    #[must_use]
    pub fn list_styles(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .document
            .definitions()
            .styles
            .iter()
            .filter(|(_, style)| style.kind == StyleKind::Paragraph)
            .filter_map(|(_, style)| style.name.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// The name of the paragraph style applied at `node` (empty if none) — for
    /// reflecting the current style in a dropdown.
    #[wasm_bindgen(js_name = paragraphStyleAt)]
    #[must_use]
    pub fn paragraph_style_at(&self, node: &str) -> String {
        let Ok(nid) = NodeId::from_str(node) else {
            return String::new();
        };
        paragraph_properties(&self.document, nid)
            .and_then(|p| p.style_ref)
            .and_then(|id| self.document.definitions().styles.get(&id).cloned())
            .and_then(|style| style.name)
            .unwrap_or_default()
    }

    /// The document's heading outline as flat `"{level}\t{node}\t{text}"` rows (in
    /// document order) — what the left Outline panel renders and navigates from. A
    /// paragraph is a heading if it carries an `outlineLvl` or a Title/Heading N
    /// style; `level` is 1-based (1 = top). Empty-text headings are skipped.
    #[wasm_bindgen(js_name = documentOutline)]
    #[must_use]
    pub fn document_outline(&self) -> Vec<String> {
        let mut nodes = Vec::new();
        collect_block_text(self.document.body(), &mut nodes);
        // Build the style cascade once; heading level comes from the *effective*
        // paragraph properties (a Heading/Title style's outlineLvl is inherited,
        // not written on the paragraph), so headings in real documents are found.
        let cascade = StyleCascade::new(self.document.definitions());
        nodes
            .into_iter()
            .filter_map(|(id, text)| {
                let level = self.heading_level_of(id, &cascade)?;
                let t = text.trim();
                (!t.is_empty()).then(|| format!("{level}\t{id}\t{}", t.replace('\t', " ")))
            })
            .collect()
    }

    /// The heading level of paragraph `node` (1-based; 1 = top), or `None` if it is
    /// not a heading. Robust across how producers mark headings:
    /// 1. the **effective** `outlineLvl` (resolved through the whole style chain);
    /// 2. otherwise, walk the paragraph's style + its `basedOn` ancestors for a
    ///    style that carries its own `outlineLvl` or a `Title`/`Heading N` name
    ///    (so a custom style *based on* Heading 2 is still found).
    fn heading_level_of(&self, node: NodeId, cascade: &StyleCascade) -> Option<u8> {
        let direct = paragraph_properties(&self.document, node)?;
        if let Some(level) = cascade
            .resolve_paragraph(&direct)
            .outline_level
            .filter(|l| *l <= 8)
        {
            return Some(level + 1);
        }
        let defs = self.document.definitions();
        let mut style_id = direct.style_ref;
        for _ in 0..24 {
            let Some(style) = style_id.and_then(|id| defs.styles.get(&id)) else {
                break;
            };
            if let Some(level) = style
                .paragraph
                .as_ref()
                .and_then(|p| p.outline_level)
                .filter(|l| *l <= 8)
            {
                return Some(level + 1);
            }
            if let Some(level) = heading_level_from_name(style.name.as_deref()) {
                return Some(level);
            }
            style_id = style.based_on;
        }
        None
    }

    /// Undoes the last user action (one or many ops), returning the restored
    /// caret + revision.
    #[wasm_bindgen(js_name = undo)]
    pub fn undo(&mut self) -> Result<EditResult, JsValue> {
        self.typing_history = None;
        let entry = self
            .undo
            .pop()
            .ok_or_else(|| to_js("nothing to undo".into()))?;
        // `group` is stored in the order it must be re-applied to undo the action.
        let (caret, redo_group) = self
            .apply_group(entry.operations)
            .map_err(|e| to_js(format!("undo failed: {e}")))?;
        self.redo.push(HistoryEntry {
            kind: entry.kind,
            operations: redo_group,
        });
        Ok(self.finish_edit(caret))
    }

    /// Redoes the last undone action.
    #[wasm_bindgen(js_name = redo)]
    pub fn redo(&mut self) -> Result<EditResult, JsValue> {
        self.typing_history = None;
        let entry = self
            .redo
            .pop()
            .ok_or_else(|| to_js("nothing to redo".into()))?;
        let (caret, undo_group) = self
            .apply_group(entry.operations)
            .map_err(|e| to_js(format!("redo failed: {e}")))?;
        self.undo.push(HistoryEntry {
            kind: entry.kind,
            operations: undo_group,
        });
        Ok(self.finish_edit(caret))
    }

    /// Serializes the current (edited) document to a `.docx` package the host can
    /// save. Uses the semantic writer (`v1::Document` → WordprocessingML), so all
    /// modeled content and edits are written; unmodeled opaque parts from the
    /// original import are not carried through in this first cut (a follow-up wires
    /// `write_document_with_retained_parts`).
    #[wasm_bindgen(js_name = exportDocx)]
    pub fn export_docx(&self) -> Result<Vec<u8>, JsValue> {
        write_document(&self.document, &self.media)
            .map_err(|e| to_js(format!("export failed: {e:?}")))
    }
}

/// A [`MediaSource`] that borrows the document's media map — lets the renderer and
/// the semantic writer share one owned copy of the image bytes.
struct BorrowedMedia<'a>(&'a BTreeMap<String, Vec<u8>>);

impl MediaSource for BorrowedMedia<'_> {
    fn media_bytes(&self, media: &str) -> Option<&[u8]> {
        self.0.get(media).map(Vec::as_slice)
    }
}

/// Internal engine calls, returning plain `Result<_, String>`. The `#[wasm_bindgen]`
/// wrappers above convert the error to a thrown JS `Error` at the boundary — so
/// these run under `cargo test` on native targets, where constructing a `JsValue`
/// would panic ("cannot call wasm-bindgen imported functions on non-wasm targets").
impl WasmDocument {
    /// See [`WasmDocument::page_size`].
    fn page_size_inner(&self, index: u32) -> Result<PageSize, String> {
        let page = self.page(index)?;
        let size = self.page_box(page);
        Ok(PageSize {
            width_twip: size.width.raw(),
            height_twip: size.height.raw(),
        })
    }

    /// See [`WasmDocument::render_page`].
    fn render_page_inner(&self, index: u32, dpi: f32) -> Result<PageBitmap, String> {
        let page = self.page(index)?;
        let size = self.page_box(page);
        let width_px = size.width.to_device_px(dpi).ceil() as u32;
        let height_px = size.height.to_device_px(dpi).ceil() as u32;

        // The page background (`w:background`) fills the page behind everything,
        // matching the native renderer.
        let mut surface = match self.document.background() {
            Some(c) => Surface::with_background(width_px, height_px, [c.r, c.g, c.b]),
            None => Surface::new(width_px, height_px),
        }
        .map_err(|e| format!("allocate surface: {e:?}"))?;

        // Serve bundled (+ any host-registered) faces from the shaper's registry,
        // taken after pagination shaped every paragraph — so the renderer outlines
        // the same face layout measured.
        let registry = self.shaper.registry();
        let fonts = RegistryFontSource::new(&registry);
        render(
            &compose_page(page),
            &mut surface,
            dpi,
            &fonts,
            &BorrowedMedia(&self.media),
        );

        Ok(PageBitmap {
            width_px,
            height_px,
            rgba: surface.data().to_vec(),
        })
    }

    /// Re-runs pagination against the current document and shaper. Called after a
    /// font registration so the new face participates in shaping + coverage. The
    /// page geometry (`default_config`) is font-independent and unchanged.
    fn repaginate(&mut self) {
        // A newly registered face can change how existing runs resolve, so every
        // cached fragment is potentially stale — drop the whole cache and re-shape.
        self.galley_cache = GalleyCache::new();
        self.layout = paginate_document(&self.document, &self.shaper);
    }

    fn register_fonts_inner(&mut self, bytes: &[u8], lengths: &[u32]) -> Result<(), String> {
        if lengths.is_empty() {
            return Ok(());
        }
        if lengths.len() > MAX_HOST_FONT_FACES {
            return Err(format!(
                "font batch has {} faces; limit is {MAX_HOST_FONT_FACES}",
                lengths.len()
            ));
        }
        if bytes.len() > MAX_HOST_FONT_BYTES {
            return Err(format!(
                "font batch has {} bytes; limit is {MAX_HOST_FONT_BYTES}",
                bytes.len()
            ));
        }
        let mut total = 0usize;
        for &length in lengths {
            let length = usize::try_from(length).map_err(|_| "font length is too large")?;
            if length == 0 {
                return Err("font batch contains an empty face".into());
            }
            total = total
                .checked_add(length)
                .ok_or_else(|| "font batch length overflow".to_owned())?;
        }
        if total != bytes.len() {
            return Err(format!(
                "font batch lengths total {total} bytes, but payload has {}",
                bytes.len()
            ));
        }

        let mut start = 0usize;
        for &length in lengths {
            let end = start + length as usize;
            self.shaper.register_font(bytes[start..end].to_vec());
            start = end;
        }
        self.repaginate();
        Ok(())
    }

    /// Builds the closed operation group for inserting normalized plain text at
    /// `start`. Line breaks are structural paragraph splits, never literal
    /// newline characters inside a run.
    fn plain_text_ops(&mut self, start: Pos, text: &str) -> Result<(Vec<Operation>, Pos), String> {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut ops = Vec::new();
        let mut cursor = start;
        for (index, line) in normalized.split('\n').enumerate() {
            if index > 0 {
                let new_id = self
                    .edit_ids
                    .next_id()
                    .map_err(|_| "id space exhausted".to_owned())?;
                ops.push(Operation::SplitParagraph { at: cursor, new_id });
                cursor = Pos::new(new_id, 0);
            }
            if line.is_empty() {
                continue;
            }
            let byte_len =
                u32::try_from(line.len()).map_err(|_| "inserted text is too large".to_owned())?;
            ops.push(Operation::InsertText {
                at: cursor,
                text: line.to_owned(),
            });
            cursor.offset = cursor
                .offset
                .checked_add(byte_len)
                .ok_or_else(|| "text offset overflow".to_owned())?;
        }
        Ok((ops, cursor))
    }

    /// Shared implementation for ordinary and gesture-grouped styled typing.
    #[allow(clippy::too_many_arguments)]
    fn styled_text_action(
        &mut self,
        node: NodeId,
        offset: u32,
        text: String,
        bold: Option<bool>,
        italic: Option<bool>,
        underline: Option<bool>,
        strike: Option<bool>,
        size_half_points: Option<u32>,
        color: Option<String>,
        highlight: Option<String>,
        vert_align: Option<String>,
        font: Option<String>,
        typing_session: Option<u32>,
    ) -> Result<EditResult, String> {
        if text.is_empty() {
            return Err("nothing to type".into());
        }
        let text_len =
            u32::try_from(text.len()).map_err(|_| "inserted text is too large".to_owned())?;
        let end = offset
            .checked_add(text_len)
            .ok_or_else(|| "text offset overflow".to_owned())?;
        let delta = FormatDelta {
            bold,
            italic,
            underline,
            strike,
            color: color.as_deref().and_then(parse_hex_color),
            highlight: highlight.as_deref().map(parse_highlight),
            size_half_points,
            vertical_alignment: vert_align.as_deref().map(|v| match v {
                "super" => VerticalAlignment::Superscript,
                "sub" => VerticalAlignment::Subscript,
                _ => VerticalAlignment::Baseline,
            }),
            font,
        };
        let start = Pos::new(node, offset);
        let caret = Pos::new(node, end);
        let one_line = !text.contains('\r') && !text.contains('\n');
        let mut ops = vec![Operation::InsertText { at: start, text }];
        if delta != FormatDelta::default() {
            ops.push(Operation::FormatText {
                range: EditRange { start, end: caret },
                delta,
            });
        }
        match typing_session {
            Some(session) => self.apply_typing_action(ops, caret, start, session, true, one_line),
            None => self.apply_action_caret_as(ops, caret, HistoryKind::Typing),
        }
    }

    /// Applies one forward op as a single undoable action, with the caret at that
    /// op's natural resting place.
    fn apply(&mut self, op: Operation) -> Result<EditResult, String> {
        let kind = history_kind_for_ops(core::slice::from_ref(&op));
        self.apply_action_as(vec![op], kind)
    }

    /// Applies a batch of forward ops as one undoable action. The caret rests at
    /// the last op's natural place. A multi-op batch is atomic: if any op fails the
    /// document is rolled back, so a partially-applied cross-paragraph edit never
    /// corrupts. A single op needs no snapshot (it validates before mutating), so
    /// typing stays clone-free.
    fn apply_action(&mut self, ops: Vec<Operation>) -> Result<EditResult, String> {
        let kind = history_kind_for_ops(&ops);
        self.apply_action_as(ops, kind)
    }

    fn apply_action_as(
        &mut self,
        ops: Vec<Operation>,
        kind: HistoryKind,
    ) -> Result<EditResult, String> {
        self.typing_history = None;
        if ops.is_empty() {
            return Err("empty edit".into());
        }
        let snapshot = (ops.len() > 1).then(|| self.document.clone());
        match self.apply_group(ops) {
            Ok((caret, inverses)) => {
                self.undo.push(HistoryEntry {
                    kind,
                    operations: inverses,
                });
                self.redo.clear();
                Ok(self.finish_edit(caret))
            }
            Err(e) => {
                if let Some(snapshot) = snapshot {
                    self.document = snapshot;
                }
                Err(e)
            }
        }
    }

    /// Like [`apply_action`](Self::apply_action) but rests the caret at `caret`
    /// instead of the last op's natural place — for a batch whose final op does not
    /// define the caret the user expects (e.g. `InsertText` then `FormatText`, where
    /// the caret should sit after the inserted text, not at the format range start).
    fn apply_action_caret(
        &mut self,
        ops: Vec<Operation>,
        caret: Pos,
    ) -> Result<EditResult, String> {
        let kind = history_kind_for_ops(&ops);
        self.apply_action_caret_as(ops, caret, kind)
    }

    fn apply_action_caret_as(
        &mut self,
        ops: Vec<Operation>,
        caret: Pos,
        kind: HistoryKind,
    ) -> Result<EditResult, String> {
        self.typing_history = None;
        if ops.is_empty() {
            return Err("empty edit".into());
        }
        let snapshot = (ops.len() > 1).then(|| self.document.clone());
        match self.apply_group(ops) {
            Ok((_, inverses)) => {
                self.undo.push(HistoryEntry {
                    kind,
                    operations: inverses,
                });
                self.redo.clear();
                Ok(self.finish_edit(caret))
            }
            Err(e) => {
                if let Some(snapshot) = snapshot {
                    self.document = snapshot;
                }
                Err(e)
            }
        }
    }

    /// Applies one incremental typing tick and optionally folds it into the
    /// previous tick's inverse group. `may_merge` is false while replacing a
    /// range; that action can begin a session, but an unexpected second range
    /// cannot be folded into it. `keep_open` is false for structural text such as
    /// a newline.
    #[allow(clippy::too_many_arguments)]
    fn apply_typing_action(
        &mut self,
        ops: Vec<Operation>,
        caret: Pos,
        start: Pos,
        session: u32,
        may_merge: bool,
        keep_open: bool,
    ) -> Result<EditResult, String> {
        if ops.is_empty() {
            return Err("empty typing edit".into());
        }
        let snapshot = (ops.len() > 1).then(|| self.document.clone());
        match self.apply_group(ops) {
            Ok((_, mut inverses)) => {
                let merge = may_merge
                    && self.redo.is_empty()
                    && self.undo.last().is_some()
                    && self.typing_history.is_some_and(|previous| {
                        previous.session == session && previous.caret == start
                    });
                if merge {
                    let mut previous = self.undo.pop().expect("history entry checked above");
                    inverses.append(&mut previous.operations);
                }
                self.undo.push(HistoryEntry {
                    kind: HistoryKind::Typing,
                    operations: inverses,
                });
                self.redo.clear();
                self.typing_history = keep_open.then_some(TypingHistory { session, caret });
                Ok(self.finish_edit(caret))
            }
            Err(error) => {
                if let Some(snapshot) = snapshot {
                    self.document = snapshot;
                }
                self.typing_history = None;
                Err(error)
            }
        }
    }

    /// Applies each op in `ops`, returning the caret (from the last op) and the
    /// inverse group ordered so that applying it in turn undoes the whole action.
    fn apply_group(&mut self, ops: Vec<Operation>) -> Result<(Pos, Vec<Operation>), String> {
        let mut inverses = Vec::with_capacity(ops.len());
        let mut caret = Pos::new(self.document.id(), 0);
        for op in &ops {
            let inverse = apply_edit(&mut self.document, &mut self.edit_ids, op)
                .map_err(|e| format!("{e:?}"))?;
            caret = caret_after(op, &inverse, &self.document);
            inverses.push(inverse);
        }
        // To undo, apply the inverses in reverse order of the forward ops.
        inverses.reverse();
        Ok((caret, inverses))
    }

    /// Bumps the revision, re-paginates, and reports the caret + revision + the
    /// **dirty page set** (indices whose layout changed) so the frontend re-rasters
    /// only those pages, not the whole document. Infallible — the mutation already
    /// succeeded.
    fn finish_edit(&mut self, caret: Pos) -> EditResult {
        self.revision += 1;
        // Incremental re-pagination: reuse the shaped lines of every paragraph the
        // edit did not touch (hash-based invalidation inside the cache re-shapes any
        // paragraph whose content changed), turning a keystroke from `O(document)`
        // into `O(edit)`. An empty dirty set is correct — the cache's content hash
        // already forces a re-shape of the edited paragraph(s); it is only the
        // belt-and-suspenders override, unnecessary here.
        let new_layout = paginate_document_cached(
            &self.document,
            &self.shaper,
            &mut self.galley_cache,
            &DirtySet::new(),
        );
        let dirty = dirty_pages(&self.layout, &new_layout);
        self.layout = new_layout;
        EditResult {
            node: caret.node.to_string(),
            offset: caret.offset,
            revision: self.revision,
            page_count: self.page_count(),
            dirty,
        }
    }

    /// The shaped plain text of paragraph `node` (empty if it is not a paragraph),
    /// used to resolve character boundaries for backspace/forward-delete.
    fn paragraph_text(&self, node: NodeId) -> String {
        let mut nodes: Vec<(NodeId, String)> = Vec::new();
        collect_block_text(self.document.body(), &mut nodes);
        nodes
            .into_iter()
            .find(|(id, _)| *id == node)
            .map(|(_, text)| text)
            .unwrap_or_default()
    }

    /// Every text-bearing paragraph in document order, with its byte length —
    /// the ordering caret navigation and cross-paragraph edits traverse.
    fn ordered_paragraphs(&self) -> Vec<(NodeId, u32)> {
        let mut nodes: Vec<(NodeId, String)> = Vec::new();
        collect_block_text(self.document.body(), &mut nodes);
        nodes
            .into_iter()
            .map(|(id, text)| (id, text.len() as u32))
            .collect()
    }

    /// Maps every paragraph to an opaque id identifying which top-level
    /// `Vec<BlockNode>` directly owns it — the body, or one specific table
    /// cell's `blocks`, or one specific content-control's `blocks`. Two
    /// paragraphs can be joined (`casual_doc_edit::join_paragraphs`) only
    /// when they share this id *and* are adjacent within it, so this map
    /// lets a cross-paragraph delete decide, upfront, every container
    /// boundary it must not try to cross — mirrors `join_paragraphs`'s own
    /// recursive same-vec search (body first, else recurse into `Table`
    /// cells / `Sdt`), minting a fresh id per cell/`Sdt` rather than reusing
    /// the parent's.
    fn paragraph_container_map(&self) -> BTreeMap<NodeId, usize> {
        fn walk(
            blocks: &[BlockNode],
            container: usize,
            next_id: &mut usize,
            out: &mut BTreeMap<NodeId, usize>,
        ) {
            for block in blocks {
                match block {
                    BlockNode::Paragraph(p) => {
                        out.insert(p.id, container);
                    }
                    BlockNode::Table(t) => {
                        for row in &t.rows {
                            for cell in &row.cells {
                                let cell_container = *next_id;
                                *next_id += 1;
                                walk(&cell.blocks, cell_container, next_id, out);
                            }
                        }
                    }
                    BlockNode::Sdt(sdt) => {
                        let sdt_container = *next_id;
                        *next_id += 1;
                        walk(&sdt.blocks, sdt_container, next_id, out);
                    }
                    BlockNode::AltChunk(_) => {}
                }
            }
        }
        let mut out = BTreeMap::new();
        let mut next_id = 1usize; // 0 is the body root.
        walk(self.document.body(), 0, &mut next_id, &mut out);
        out
    }

    /// Orders two selection endpoints into `(start, end)` by document position.
    fn order_endpoints(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> Result<(Pos, Pos), String> {
        let s_node = NodeId::from_str(start_node).map_err(|_| "invalid start node".to_string())?;
        let e_node = NodeId::from_str(end_node).map_err(|_| "invalid end node".to_string())?;
        let paras = self.ordered_paragraphs();
        let si = paras
            .iter()
            .position(|(id, _)| *id == s_node)
            .ok_or("start node not found")?;
        let ei = paras
            .iter()
            .position(|(id, _)| *id == e_node)
            .ok_or("end node not found")?;
        let a = Pos::new(s_node, start_offset);
        let b = Pos::new(e_node, end_offset);
        if (si, start_offset) <= (ei, end_offset) {
            Ok((a, b))
        } else {
            Ok((b, a))
        }
    }

    /// The op sequence that deletes an ordered selection: one `DeleteText` within a
    /// paragraph, or (across paragraphs) delete the start tail + each whole middle
    /// paragraph + the end head, then join same-container paragraphs together
    /// (never across a table-cell/content-control boundary — see
    /// `paragraph_container_map`).
    fn selection_delete_ops(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> Result<Vec<Operation>, String> {
        let (start, end) = self.order_endpoints(start_node, start_offset, end_node, end_offset)?;
        if start.node == end.node {
            if start.offset == end.offset {
                return Ok(Vec::new());
            }
            return Ok(vec![Operation::DeleteText {
                range: EditRange { start, end },
            }]);
        }
        let paras = self.ordered_paragraphs();
        let si = paras
            .iter()
            .position(|(id, _)| *id == start.node)
            .ok_or("start not found")?;
        let ei = paras
            .iter()
            .position(|(id, _)| *id == end.node)
            .ok_or("end not found")?;
        let start_len = paras[si].1;

        let mut ops = Vec::new();
        if start.offset < start_len {
            ops.push(Operation::DeleteText {
                range: EditRange {
                    start,
                    end: Pos::new(start.node, start_len),
                },
            });
        }
        for (id, len) in paras.iter().take(ei).skip(si + 1) {
            if *len > 0 {
                ops.push(Operation::DeleteText {
                    range: EditRange {
                        start: Pos::new(*id, 0),
                        end: Pos::new(*id, *len),
                    },
                });
            }
        }
        if end.offset > 0 {
            ops.push(Operation::DeleteText {
                range: EditRange {
                    start: Pos::new(end.node, 0),
                    end,
                },
            });
        }
        // Join every following paragraph (now empty in the middle) up to and
        // including the end paragraph into its nearest preceding paragraph —
        // but never across a container boundary (body vs. a table cell's own
        // `blocks`, or a content control's): a generic delete must never
        // dissolve a table/cell by trying to merge a body paragraph with one
        // living inside it. A boundary paragraph is left in place (already
        // emptied by the `DeleteText` ops above) and becomes the new join
        // anchor for any further same-container run after it.
        let containers = self.paragraph_container_map();
        let mut anchor = start.node;
        let mut anchor_container = containers.get(&start.node).copied().unwrap_or(0);
        for (id, _) in paras.iter().take(ei + 1).skip(si + 1) {
            let container = containers.get(id).copied().unwrap_or(0);
            if container == anchor_container {
                ops.push(Operation::JoinParagraphs {
                    first: anchor,
                    second: *id,
                });
            } else {
                anchor = *id;
                anchor_container = container;
            }
        }
        Ok(ops)
    }

    /// The per-paragraph sub-ranges an ordered selection covers: `(node, start,
    /// end)` byte ranges, one per paragraph the selection touches (the start
    /// paragraph's tail, whole middles, the end paragraph's head). Same-paragraph
    /// selections yield a single entry.
    fn selection_subranges(&self, start: Pos, end: Pos) -> Vec<(NodeId, u32, u32)> {
        if start.node == end.node {
            return if end.offset > start.offset {
                vec![(start.node, start.offset, end.offset)]
            } else {
                Vec::new()
            };
        }
        let paras = self.ordered_paragraphs();
        let (Some(si), Some(ei)) = (
            paras.iter().position(|(id, _)| *id == start.node),
            paras.iter().position(|(id, _)| *id == end.node),
        ) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let start_len = paras[si].1;
        if start.offset < start_len {
            out.push((start.node, start.offset, start_len));
        }
        for (id, len) in paras.iter().take(ei).skip(si + 1) {
            if *len > 0 {
                out.push((*id, 0, *len));
            }
        }
        if end.offset > 0 {
            out.push((end.node, 0, end.offset));
        }
        out
    }

    /// Applies a run-property `delta` across the selection (one `FormatText` per
    /// covered paragraph sub-range) as one undoable action.
    fn apply_run_format(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        delta: FormatDelta,
    ) -> Result<EditResult, JsValue> {
        let (start, end) = self
            .order_endpoints(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let ops: Vec<Operation> = self
            .selection_subranges(start, end)
            .into_iter()
            .map(|(node, s, e)| Operation::FormatText {
                range: EditRange {
                    start: Pos::new(node, s),
                    end: Pos::new(node, e),
                },
                delta: delta.clone(),
            })
            .collect();
        if ops.is_empty() {
            return Err(to_js("empty selection".into()));
        }
        self.apply_action(ops).map_err(to_js)
    }

    /// Applies a mutation `f` to the properties of every paragraph the selection
    /// touches, as one undoable action.
    fn apply_paragraph_props(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        f: impl Fn(&mut ParagraphProperties),
    ) -> Result<EditResult, JsValue> {
        self.apply_paragraph_props_as(
            start_node,
            start_offset,
            end_node,
            end_offset,
            HistoryKind::ParagraphFormatting,
            f,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_paragraph_props_as(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        kind: HistoryKind,
        f: impl Fn(&mut ParagraphProperties),
    ) -> Result<EditResult, JsValue> {
        let (start, end) = self
            .order_endpoints(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let mut ops = Vec::new();
        for node in self.paragraphs_in_selection(start, end) {
            if let Some(mut props) = paragraph_properties(&self.document, node) {
                f(&mut props);
                ops.push(Operation::SetParagraphProperties {
                    node,
                    properties: Box::new(props),
                });
            }
        }
        if ops.is_empty() {
            return Err(to_js("no paragraph in selection".into()));
        }
        self.apply_action_as(ops, kind).map_err(to_js)
    }

    /// Reads the current properties of the cell containing `node`, applies `f`, and
    /// installs them via `SetTableCellProperties` (one undoable action). Errors when
    /// `node` is not inside a table cell.
    fn apply_cell_props(
        &mut self,
        node: &str,
        f: impl FnOnce(&mut TableCellProperties),
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        let (_table, cell) =
            locate_cell(&self.document, nid).ok_or_else(|| to_js("not in a table cell".into()))?;
        let mut props =
            cell_properties(&self.document, cell).ok_or_else(|| to_js("cell not found".into()))?;
        f(&mut props);
        self.apply_action(vec![Operation::SetTableCellProperties {
            cell,
            properties: Box::new(props),
        }])
        .map_err(to_js)
    }

    /// A clone of the current properties of the cell containing `node`, or `None` when
    /// not in a cell — the read side of the cell-formatting reflect getters.
    fn cell_props_of(&self, node: &str) -> Option<TableCellProperties> {
        let nid = NodeId::from_str(node).ok()?;
        let (_table, cell) = locate_cell(&self.document, nid)?;
        cell_properties(&self.document, cell)
    }

    /// Ruler-indent applier: like [`apply_paragraph_props`](Self::apply_paragraph_props)
    /// but skips paragraphs inside table cells. A body-scale indent applied to a
    /// narrow cell paragraph overflows the cell and mangles the table, so the ruler
    /// never touches them (a cell-relative ruler is a follow-up). If the selection
    /// covers only table paragraphs the drag is a no-op (reported as an error the
    /// caller ignores), leaving the table intact.
    fn apply_indent_props(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        f: impl Fn(&mut ParagraphProperties),
    ) -> Result<EditResult, JsValue> {
        let (start, end) = self
            .order_endpoints(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let mut ops = Vec::new();
        for node in self.paragraphs_in_selection(start, end) {
            if locate_table_row(&self.document, node).is_some() {
                continue; // never indent a table-cell paragraph from the ruler
            }
            if let Some(mut props) = paragraph_properties(&self.document, node) {
                f(&mut props);
                ops.push(Operation::SetParagraphProperties {
                    node,
                    properties: Box::new(props),
                });
            }
        }
        if ops.is_empty() {
            return Err(to_js("no body paragraph in selection".into()));
        }
        self.apply_action(ops).map_err(to_js)
    }

    /// Ensures the shared bullet (or numbered) list definition exists in the
    /// document and returns its instance id, creating the abstract definition +
    /// instance on first use (ids from the edit namespace). Idempotent — every later
    /// toggle of the same kind reuses it. The definition is document infrastructure,
    /// not a body edit, so it is not part of undo; an unreferenced numbering instance
    /// is valid and harmless.
    fn ensure_list(&mut self, numbered: bool) -> Result<NumberingInstanceId, String> {
        if let Some(id) = if numbered {
            self.numbered_list
        } else {
            self.bullet_list
        } {
            return Ok(id);
        }
        let exhausted = || "id space exhausted".to_string();
        let abs_id = AbstractNumberingId::new(self.edit_ids.next_id().map_err(|_| exhausted())?);
        let inst_id = NumberingInstanceId::new(self.edit_ids.next_id().map_err(|_| exhausted())?);
        let defs = self.document.definitions_mut();
        defs.abstract_numbering.insert(
            abs_id,
            AbstractNumbering {
                levels: (0..=8).map(|level| list_level(numbered, level)).collect(),
            },
        );
        defs.numbering.insert(
            inst_id,
            NumberingInstance {
                abstract_ref: abs_id,
                overrides: Vec::new(),
            },
        );
        if numbered {
            self.numbered_list = Some(inst_id);
        } else {
            self.bullet_list = Some(inst_id);
        }
        Ok(inst_id)
    }

    /// The number format of an imported list instance's first level, if it resolves —
    /// lets [`list_style_at`](Self::list_style_at) light the right button for a list
    /// the document already carried (not one this editor created).
    fn list_format(&self, instance: NumberingInstanceId) -> Option<NumberFormat> {
        let defs = self.document.definitions();
        let inst = defs.numbering.get(&instance)?;
        let abstract_num = defs.abstract_numbering.get(&inst.abstract_ref)?;
        abstract_num.levels.first().and_then(|l| l.num_fmt.clone())
    }

    /// Builds an empty row matching `template`'s column structure: one empty
    /// paragraph per cell, cell properties (width / span / borders) preserved, and
    /// fresh ids for the row, every cell, and every paragraph (from the edit
    /// namespace, so they never collide with imported nodes).
    fn empty_row_like(&mut self, template: &TableRow) -> Result<TableRow, String> {
        let exhausted = || "id space exhausted".to_string();
        let mut cells = Vec::with_capacity(template.cells.len());
        for cell in &template.cells {
            let cell_id = self.edit_ids.next_id().map_err(|_| exhausted())?;
            let para_id = self.edit_ids.next_id().map_err(|_| exhausted())?;
            cells.push(TableCell {
                id: cell_id,
                properties: cell.properties.clone(),
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: para_id,
                    properties: ParagraphProperties::default(),
                    inlines: Vec::new(),
                })],
            });
        }
        let row_id = self.edit_ids.next_id().map_err(|_| exhausted())?;
        Ok(TableRow {
            id: row_id,
            properties: template.properties.clone(),
            cells,
        })
    }

    /// The first paragraph of the row that will occupy `index`'s neighbourhood after
    /// the row at `index` is removed — the next row, or the previous one when the
    /// last row is deleted — for placing the caret. `None` if the table has one row.
    fn surviving_row_anchor(&self, table: NodeId, index: u32) -> Option<NodeId> {
        let t = find_table(&self.document, table)?;
        if t.rows.len() <= 1 {
            return None;
        }
        let idx = index as usize;
        let target = if idx + 1 < t.rows.len() {
            idx + 1
        } else {
            idx.saturating_sub(1)
        };
        first_paragraph_of_row(t.rows.get(target)?)
    }

    /// `count` empty cells (one per row) for a new column: default cell properties,
    /// a single empty paragraph each, all ids fresh from the edit namespace.
    fn empty_column_cells(&mut self, count: usize) -> Result<Vec<TableCell>, String> {
        let exhausted = || "id space exhausted".to_string();
        let mut cells = Vec::with_capacity(count);
        for _ in 0..count {
            let cell_id = self.edit_ids.next_id().map_err(|_| exhausted())?;
            let para_id = self.edit_ids.next_id().map_err(|_| exhausted())?;
            cells.push(TableCell {
                id: cell_id,
                properties: TableCellProperties::default(),
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: para_id,
                    properties: ParagraphProperties::default(),
                    inlines: Vec::new(),
                })],
            });
        }
        Ok(cells)
    }

    /// The paragraph of the cell that survives at `col`'s spot in `row` after the
    /// column at `col` is removed — the next column, or the previous one when the
    /// last column is deleted — for placing the caret. `None` if the row has one cell.
    fn surviving_cell_anchor(&self, table: NodeId, row: u32, col: u32) -> Option<NodeId> {
        let t = find_table(&self.document, table)?;
        let row = t.rows.get(row as usize)?;
        if row.cells.len() <= 1 {
            return None;
        }
        let idx = col as usize;
        let target = if idx + 1 < row.cells.len() {
            idx + 1
        } else {
            idx.saturating_sub(1)
        };
        first_paragraph_of_cell(row.cells.get(target)?)
    }

    /// The first paragraph of the adjacent cell in row-major order.
    fn table_cell_anchor(
        &self,
        table: NodeId,
        row: u32,
        col: u32,
        forward: bool,
    ) -> Option<NodeId> {
        let t = find_table(&self.document, table)?;
        let row_idx = row as usize;
        let col_idx = col as usize;
        let target = if forward {
            let row = t.rows.get(row_idx)?;
            if col_idx + 1 < row.cells.len() {
                Some((row_idx, col_idx + 1))
            } else if row_idx + 1 < t.rows.len() {
                Some((row_idx + 1, 0))
            } else {
                None
            }
        } else if col_idx > 0 {
            Some((row_idx, col_idx - 1))
        } else if row_idx > 0 {
            let prev_row = t.rows.get(row_idx - 1)?;
            prev_row
                .cells
                .len()
                .checked_sub(1)
                .map(|last| (row_idx - 1, last))
        } else {
            None
        }?;
        t.rows
            .get(target.0)
            .and_then(|r| r.cells.get(target.1))
            .and_then(first_paragraph_of_cell)
    }

    /// The id of the paragraph style with `name`, if one exists.
    fn style_id_by_name(&self, name: &str) -> Option<StyleId> {
        if name.is_empty() {
            return None;
        }
        self.document
            .definitions()
            .styles
            .iter()
            .find(|(_, style)| {
                style.kind == StyleKind::Paragraph && style.name.as_deref() == Some(name)
            })
            .map(|(id, _)| *id)
    }

    /// The paragraph node ids the selection touches, in document order.
    fn paragraphs_in_selection(&self, start: Pos, end: Pos) -> Vec<NodeId> {
        if start.node == end.node {
            return vec![start.node];
        }
        let paras = self.ordered_paragraphs();
        let (Some(si), Some(ei)) = (
            paras.iter().position(|(id, _)| *id == start.node),
            paras.iter().position(|(id, _)| *id == end.node),
        ) else {
            return Vec::new();
        };
        paras[si..=ei].iter().map(|(id, _)| *id).collect()
    }

    /// The caret position one step in `dir` from `(nid, offset)` — crossing line/
    /// page boundaries (up/down) and paragraph boundaries (left/right).
    fn moved_caret(&self, nid: NodeId, offset: u32, dir: &str) -> (NodeId, u32) {
        match dir {
            "up" | "down" => {
                let direction = if dir == "up" {
                    Direction::Up
                } else {
                    Direction::Down
                };
                LayoutSnapshot::new(&self.layout)
                    .move_vertical(ModelPos::new(nid, offset), direction)
                    .map_or((nid, offset), |p| (p.node, p.offset))
            }
            "left" => {
                if offset > 0 {
                    let text = self.paragraph_text(nid);
                    (nid, prev_char_boundary(&text, offset as usize) as u32)
                } else {
                    let paras = self.ordered_paragraphs();
                    match paras.iter().position(|(id, _)| *id == nid) {
                        Some(i) if i > 0 => (paras[i - 1].0, paras[i - 1].1),
                        _ => (nid, offset),
                    }
                }
            }
            "right" => {
                let text = self.paragraph_text(nid);
                if (offset as usize) < text.len() {
                    (nid, next_char_boundary(&text, offset as usize) as u32)
                } else {
                    let paras = self.ordered_paragraphs();
                    match paras.iter().position(|(id, _)| *id == nid) {
                        Some(i) if i + 1 < paras.len() => (paras[i + 1].0, 0),
                        _ => (nid, offset),
                    }
                }
            }
            // Line start/end: probe the same visual line at its far left/right via
            // hit-testing (reuses the exact line geometry).
            "lineStart" | "lineEnd" => {
                let snapshot = LayoutSnapshot::new(&self.layout);
                let Some((page, rect)) = snapshot.caret_rect(ModelPos::new(nid, offset)) else {
                    return (nid, offset);
                };
                let y = Twip(rect.origin.y.raw() + rect.size.height.raw() / 2);
                let x = if dir == "lineStart" {
                    Twip(0)
                } else {
                    Twip(i32::MAX)
                };
                snapshot
                    .hit_test(page, Point::new(x, y))
                    .map_or((nid, offset), |h| (h.pos.node, h.pos.offset))
            }
            "wordRight" => {
                let text = self.paragraph_text(nid);
                match next_word_boundary(&text, offset as usize) {
                    Some(o) => (nid, o as u32),
                    None => self.moved_caret(nid, offset.max(text.len() as u32), "right"),
                }
            }
            "wordLeft" => {
                let text = self.paragraph_text(nid);
                match prev_word_boundary(&text, offset as usize) {
                    Some(o) => (nid, o as u32),
                    None if offset == 0 => self.moved_caret(nid, 0, "left"),
                    None => (nid, 0),
                }
            }
            "paragraphUp" => {
                let paras = self.ordered_paragraphs();
                match paras.iter().position(|(id, _)| *id == nid) {
                    Some(_) if offset > 0 => (nid, 0),
                    Some(i) if i > 0 => (paras[i - 1].0, 0),
                    _ => (nid, 0),
                }
            }
            "paragraphDown" => {
                let paras = self.ordered_paragraphs();
                match paras.iter().position(|(id, _)| *id == nid) {
                    Some(i) if i + 1 < paras.len() => (paras[i + 1].0, 0),
                    Some(i) => (nid, paras[i].1),
                    None => (nid, offset),
                }
            }
            _ => (nid, offset),
        }
    }

    /// The page at `index`, or an out-of-range message.
    fn page(&self, index: u32) -> Result<&Page, String> {
        self.layout.pages.get(index as usize).ok_or_else(|| {
            format!(
                "page index {index} out of range (0..{})",
                self.layout.page_count()
            )
        })
    }

    /// The page box (width/height) for a page, from its own section geometry,
    /// falling back to the first-section config when the section id is unknown.
    fn page_box(&self, page: &Page) -> Size {
        self.document
            .definitions()
            .sections
            .iter()
            .find(|s| s.id == page.section)
            .map_or(self.default_config.page_size, |s| {
                Size::new(
                    Twip(s.page_size.width_twips),
                    Twip(s.page_size.height_twips),
                )
            })
    }

    /// Plain text for a model range: slice the start/end nodes' shaped text at the
    /// byte offsets and join the paragraphs the range spans with `\n`.
    fn copy_text_inner(&self, range: ModelRange) -> String {
        // Text-bearing nodes in document order — the order hit-testing traverses.
        let mut nodes: Vec<(NodeId, String)> = Vec::new();
        collect_block_text(self.document.body(), &mut nodes);

        let start = nodes.iter().position(|(id, _)| *id == range.start.node);
        let end = nodes.iter().position(|(id, _)| *id == range.end.node);
        let (Some(mut si), Some(mut ei)) = (start, end) else {
            return String::new();
        };
        let (mut so, mut eo) = (range.start.offset as usize, range.end.offset as usize);
        // Order the endpoints (a backward drag selects the same text).
        if si > ei || (si == ei && so > eo) {
            std::mem::swap(&mut si, &mut ei);
            std::mem::swap(&mut so, &mut eo);
        }

        if si == ei {
            return slice_bytes(&nodes[si].1, so, eo);
        }
        let mut parts = Vec::with_capacity(ei - si + 1);
        parts.push(slice_bytes(&nodes[si].1, so, nodes[si].1.len()));
        for (_, text) in &nodes[si + 1..ei] {
            parts.push(text.clone());
        }
        parts.push(slice_bytes(&nodes[ei].1, 0, eo));
        parts.join("\n")
    }

    fn copy_rich_runs_inner(&self, range: ModelRange) -> Vec<ClipboardRun> {
        let mut nodes: Vec<(NodeId, String)> = Vec::new();
        collect_block_text(self.document.body(), &mut nodes);

        let start = nodes.iter().position(|(id, _)| *id == range.start.node);
        let end = nodes.iter().position(|(id, _)| *id == range.end.node);
        let (Some(mut si), Some(mut ei)) = (start, end) else {
            return Vec::new();
        };
        let (mut so, mut eo) = (range.start.offset as usize, range.end.offset as usize);
        if si > ei || (si == ei && so > eo) {
            std::mem::swap(&mut si, &mut ei);
            std::mem::swap(&mut so, &mut eo);
        }

        let mut out = Vec::new();
        for (idx, (node, text)) in nodes.iter().enumerate().take(ei + 1).skip(si) {
            if idx > si {
                out.push(ClipboardRun {
                    paragraph_break: true,
                    ..ClipboardRun::default()
                });
            }
            let (node, full_len) = (*node, text.len());
            let lo = if idx == si { so } else { 0 };
            let hi = if idx == ei { eo } else { full_len };
            if lo >= hi {
                continue;
            }
            let Some(paragraph) = find_paragraph(self.document.body(), node) else {
                continue;
            };
            paragraph_rich_runs(&self.document, paragraph, lo as u32, hi as u32, &mut out);
        }
        out
    }
}

/// Walks one paragraph's inlines in the same byte-anchor space
/// `paragraph_links`/`inline_anchor_len` use, emitting a [`ClipboardRun`] for
/// each `Run` (styled) or `Hyperlink`-wrapped `Run` (styled + `href`) that
/// intersects `[lo, hi)`, clipped to the intersection. Any other inline kind
/// intersecting the range falls back to its slice of the paragraph's existing
/// plain-text extraction (`node_plain_text` — the same source `copyText`
/// uses), unstyled — never silently dropped.
/// One element of a rich-clipboard fragment: either a formatted text run
/// (`paragraph_break: false`) or a paragraph-break marker (`paragraph_break:
/// true`, every other field ignored). Crosses the JS boundary as JSON — a
/// bridge payload, not a new engine type — so `copyRichRuns` and
/// `pasteRichRuns` share exactly this shape with no separate wasm-bindgen
/// struct to keep in sync. Field names match [`FormatDelta`]'s vocabulary.
/// The "Page Setup" dialog's bridge payload — crosses the JS boundary as
/// JSON, mirrors [`page_setup`](WasmDocument::page_setup)/
/// [`set_page_setup`](WasmDocument::set_page_setup). `section` is the
/// section's `NodeId` as a hex string, opaque to JS, passed back unchanged.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageSetupJson {
    section: String,
    page_size: SectionPageSize,
    page_margins: PageMargins,
    orientation: Option<PageOrientation>,
}

/// Sparse draft emitted by the host's table-properties inspector. Optional fields
/// mean "leave the current model value untouched"; numeric `-1` values clear an
/// optional measurement.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct TablePropertiesPatch {
    alignment: Option<String>,
    table_width_twips: Option<i32>,
    table_indent_twips: Option<i32>,
    fixed_layout: Option<bool>,
    header_row: Option<bool>,
    column_width_twips: Option<i32>,
    row_height_twips: Option<i32>,
    row_height_rule: Option<String>,
    cell_margin_twips: Option<i32>,
    cell_spacing_twips: Option<i32>,
    caption: Option<String>,
    description: Option<String>,
}

impl TablePropertiesPatch {
    fn is_empty(&self) -> bool {
        self.alignment.is_none()
            && self.table_width_twips.is_none()
            && self.table_indent_twips.is_none()
            && self.fixed_layout.is_none()
            && self.header_row.is_none()
            && self.column_width_twips.is_none()
            && self.row_height_twips.is_none()
            && self.row_height_rule.is_none()
            && self.cell_margin_twips.is_none()
            && self.cell_spacing_twips.is_none()
            && self.caption.is_none()
            && self.description.is_none()
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ClipboardRun {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    underline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    strike: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_half_points: Option<u32>,
    /// `#rrggbb`.
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    /// A [`highlight_name`] value (e.g. `"yellow"`, `"none"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    highlight: Option<String>,
    /// `"super"` or `"sub"` (baseline is `None`).
    #[serde(skip_serializing_if = "Option::is_none")]
    vert_align: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    font: Option<String>,
    /// An external URL, or `#anchor` for an internal bookmark target —
    /// [`set_hyperlink`](WasmDocument::set_hyperlink)'s existing convention.
    #[serde(skip_serializing_if = "Option::is_none")]
    href: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    paragraph_break: bool,
}

fn paragraph_rich_runs(
    document: &Document,
    paragraph: &Paragraph,
    lo: u32,
    hi: u32,
    out: &mut Vec<ClipboardRun>,
) {
    let full_text = node_plain_text(&paragraph.inlines);
    let mut offset = 0u32;
    walk_inlines_rich(
        document,
        &paragraph.inlines,
        None,
        &mut offset,
        lo,
        hi,
        &full_text,
        out,
    );
}

#[allow(clippy::too_many_arguments)]
fn walk_inlines_rich(
    document: &Document,
    inlines: &[InlineNode],
    href: Option<&str>,
    offset: &mut u32,
    lo: u32,
    hi: u32,
    full_text: &str,
    out: &mut Vec<ClipboardRun>,
) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => {
                let start = *offset;
                let end = start.saturating_add(run.text.len() as u32);
                *offset = end;
                let (a, b) = (start.max(lo), end.min(hi));
                if a < b {
                    let text = slice_bytes(&run.text, (a - start) as usize, (b - start) as usize);
                    out.push(clipboard_run_from(run, href, text));
                }
            }
            InlineNode::Hyperlink(link) => {
                let href = hyperlink_href(&link.target);
                walk_inlines_rich(
                    document,
                    &link.inlines,
                    Some(&href),
                    offset,
                    lo,
                    hi,
                    full_text,
                    out,
                );
            }
            InlineNode::Revision(revision) => {
                walk_inlines_rich(
                    document,
                    &revision.inlines,
                    href,
                    offset,
                    lo,
                    hi,
                    full_text,
                    out,
                );
            }
            InlineNode::Sdt(sdt) => {
                walk_inlines_rich(document, &sdt.inlines, href, offset, lo, hi, full_text, out);
            }
            other => {
                let start = *offset;
                let end = start.saturating_add(inline_anchor_len(document, other));
                *offset = end;
                let (a, b) = (start.max(lo), end.min(hi));
                if a < b {
                    let text = slice_bytes(full_text, a as usize, b as usize);
                    if !text.is_empty() {
                        out.push(ClipboardRun {
                            text,
                            ..ClipboardRun::default()
                        });
                    }
                }
            }
        }
    }
}

fn clipboard_run_from(run: &Run, href: Option<&str>, text: String) -> ClipboardRun {
    let props = &run.properties;
    ClipboardRun {
        text,
        bold: props.bold,
        italic: props.italic,
        underline: props.underline,
        strike: props.strike,
        size_half_points: props.size_half_points,
        color: match &props.color {
            Some(Color::Rgb(c)) => Some(format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)),
            _ => None,
        },
        highlight: props.highlight.map(highlight_name),
        vert_align: match props.vertical_alignment {
            Some(VerticalAlignment::Superscript) => Some("super".to_string()),
            Some(VerticalAlignment::Subscript) => Some("sub".to_string()),
            _ => None,
        },
        font: match &props.font_ref {
            Some(FontRef::Named(FontName { name })) => Some(name.clone()),
            _ => None,
        },
        href: href.map(str::to_string),
        paragraph_break: false,
    }
}

fn hyperlink_href(target: &HyperlinkTarget) -> String {
    match target {
        HyperlinkTarget::External(external) => external.url.clone(),
        HyperlinkTarget::Internal(internal) => format!("#{}", internal.anchor),
    }
}

fn highlight_name(color: HighlightColor) -> String {
    match color {
        HighlightColor::None => "none",
        HighlightColor::Black => "black",
        HighlightColor::Blue => "blue",
        HighlightColor::Cyan => "cyan",
        HighlightColor::DarkBlue => "darkblue",
        HighlightColor::DarkCyan => "darkcyan",
        HighlightColor::DarkGray => "darkgray",
        HighlightColor::DarkGreen => "darkgreen",
        HighlightColor::DarkMagenta => "darkmagenta",
        HighlightColor::DarkRed => "darkred",
        HighlightColor::DarkYellow => "darkyellow",
        HighlightColor::Green => "green",
        HighlightColor::LightGray => "lightgray",
        HighlightColor::Magenta => "magenta",
        HighlightColor::Red => "red",
        HighlightColor::White => "white",
        HighlightColor::Yellow => "yellow",
    }
    .to_string()
}

/// Collects every text-bearing node (paragraphs, including those inside tables and
/// block content controls) in document order, paired with its shaped plain text —
/// the byte space hit-testing addresses (doc 58 §3).
fn collect_block_text(blocks: &[BlockNode], out: &mut Vec<(NodeId, String)>) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(p) => out.push((p.id, node_plain_text(&p.inlines))),
            BlockNode::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        collect_block_text(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => collect_block_text(&sdt.blocks, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

fn find_text_match(
    nodes: &[(NodeId, String)],
    query: &str,
    start_idx: usize,
    start_offset: usize,
    forward: bool,
    match_case: bool,
) -> Option<TextMatch> {
    if forward {
        if let Some(hit) = find_in_node_after(
            nodes[start_idx].0,
            &nodes[start_idx].1,
            query,
            start_offset,
            match_case,
        ) {
            return Some(hit);
        }
        for (node, text) in nodes.iter().skip(start_idx + 1) {
            if let Some(hit) = find_in_node_after(*node, text, query, 0, match_case) {
                return Some(hit);
            }
        }
        for (node, text) in nodes.iter().take(start_idx) {
            if let Some(hit) = find_in_node_after(*node, text, query, 0, match_case) {
                return Some(hit);
            }
        }
        find_in_node_before(
            nodes[start_idx].0,
            &nodes[start_idx].1,
            query,
            start_offset,
            match_case,
        )
    } else {
        if let Some(hit) = find_in_node_before(
            nodes[start_idx].0,
            &nodes[start_idx].1,
            query,
            start_offset,
            match_case,
        ) {
            return Some(hit);
        }
        for (node, text) in nodes[..start_idx].iter().rev() {
            if let Some(hit) = find_in_node_before(*node, text, query, text.len(), match_case) {
                return Some(hit);
            }
        }
        for (node, text) in nodes.iter().skip(start_idx + 1).rev() {
            if let Some(hit) = find_in_node_before(*node, text, query, text.len(), match_case) {
                return Some(hit);
            }
        }
        find_in_node_after(
            nodes[start_idx].0,
            &nodes[start_idx].1,
            query,
            start_offset,
            match_case,
        )
    }
}

fn find_in_node_after(
    node: NodeId,
    text: &str,
    query: &str,
    min_offset: usize,
    match_case: bool,
) -> Option<TextMatch> {
    find_spans(text, query, match_case)
        .into_iter()
        .find(|(start, _)| *start >= min_offset)
        .map(|(start, end)| TextMatch::new(node, start, end))
}

fn find_in_node_before(
    node: NodeId,
    text: &str,
    query: &str,
    max_offset: usize,
    match_case: bool,
) -> Option<TextMatch> {
    find_spans(text, query, match_case)
        .into_iter()
        .take_while(|(_, end)| *end <= max_offset)
        .last()
        .map(|(start, end)| TextMatch::new(node, start, end))
}

fn find_spans(text: &str, query: &str, match_case: bool) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let qchars = query.chars().count();
    if qchars == 0 {
        return Vec::new();
    }
    let query_folded = if match_case {
        String::new()
    } else {
        query.to_lowercase()
    };
    let mut offsets: Vec<usize> = text.char_indices().map(|(idx, _)| idx).collect();
    offsets.push(text.len());
    if offsets.len() <= qchars {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..=(offsets.len() - qchars - 1) {
        let start = offsets[i];
        let end = offsets[i + qchars];
        let candidate = &text[start..end];
        let matched = if match_case {
            candidate == query
        } else {
            candidate.to_lowercase() == query_folded
        };
        if matched {
            out.push((start, end));
        }
    }
    out
}

/// A model anchor `NodeId` + byte offset, or `None` if `node` is not a valid id.
fn parse_pos(node: &str, offset: u32) -> Option<ModelPos> {
    Some(ModelPos::new(NodeId::from_str(node).ok()?, offset))
}

/// A [`ModelRange`] from two string-encoded anchors, or `None` if either id is
/// malformed.
fn parse_range(
    start_node: &str,
    start_offset: u32,
    end_node: &str,
    end_offset: u32,
) -> Option<ModelRange> {
    Some(ModelRange::new(
        parse_pos(start_node, start_offset)?,
        parse_pos(end_node, end_offset)?,
    ))
}

/// Resolves the direct properties of each run covered by one paragraph range
/// through the document's effective style cascade.
fn effective_run_properties_in_range(
    document: &Document,
    node: NodeId,
    start: u32,
    end: u32,
) -> Vec<RunProperties> {
    let Some(paragraph) = find_paragraph(document.body(), node) else {
        return Vec::new();
    };
    let cascade = StyleCascade::new(document.definitions());
    let paragraph_style = cascade.paragraph_style(&paragraph.properties);
    run_properties_in_range(
        document,
        EditRange {
            start: Pos::new(node, start),
            end: Pos::new(node, end),
        },
    )
    .into_iter()
    .map(|direct| cascade.resolve_run(paragraph_style, direct))
    .collect()
}

/// Resolves the run a caret inherits, including document defaults and paragraph /
/// character styles. Empty paragraphs still inherit their paragraph style.
fn effective_caret_run_properties(
    document: &Document,
    node: NodeId,
    offset: u32,
) -> Option<RunProperties> {
    let paragraph = find_paragraph(document.body(), node)?;
    let cascade = StyleCascade::new(document.definitions());
    let direct = caret_run_properties(document, node, offset)
        .cloned()
        .unwrap_or_default();
    Some(cascade.resolve_run(cascade.paragraph_style(&paragraph.properties), &direct))
}

/// Converts effective run properties into the uniform toggle state a toolbar
/// reflects.
fn format_from_effective_runs(properties: &[RunProperties]) -> Format {
    if properties.is_empty() {
        return Format::default();
    }
    let state = |f: fn(&RunProperties) -> Option<bool>| {
        let first = f(&properties[0]) == Some(true);
        if properties
            .iter()
            .skip(1)
            .all(|properties| (f(properties) == Some(true)) == first)
        {
            u8::from(first)
        } else {
            2
        }
    };
    Format {
        bold_state: state(|properties| properties.bold),
        italic_state: state(|properties| properties.italic),
        underline_state: state(|properties| properties.underline),
        strike_state: state(|properties| properties.strike),
    }
}

/// Converts effective properties into the WASM [`RunStyle`] the toolbar reads.
/// Font names are the authored/requested family, never the physical fallback
/// selected by the renderer.
fn run_style_from_effective_runs(document: &Document, properties: &[RunProperties]) -> RunStyle {
    if properties.is_empty() {
        return RunStyle::default();
    }
    let scheme = document.definitions().font_scheme.as_ref();
    let size = uniform_value(properties, |p| p.size_half_points);
    let color = uniform_value(properties, |p| p.color);
    let font = uniform_value(properties, |p| {
        requested_font_family(p, scheme)
            .unwrap_or_else(|| casual_doc_layout::fonts::ROBOTO.name.to_owned())
    });
    let highlight = uniform_value(properties, |p| p.highlight);
    let vertical_alignment = uniform_value(properties, |p| {
        p.vertical_alignment.unwrap_or(VerticalAlignment::Baseline)
    });
    let size_mixed = size.is_none();
    let color_mixed = color.is_none();
    let font_mixed = font.is_none();
    let highlight_mixed = highlight.is_none();
    let vertical_align_mixed = vertical_alignment.is_none();
    let vertical_align = vertical_alignment.map_or_else(String::new, |alignment| match alignment {
        VerticalAlignment::Superscript => "super".to_owned(),
        VerticalAlignment::Subscript => "sub".to_owned(),
        VerticalAlignment::Baseline => "baseline".to_owned(),
    });
    RunStyle {
        size_points: size
            .flatten()
            .map_or(0.0, |half_points| half_points as f32 / 2.0),
        size_mixed,
        color: color
            .flatten()
            .map_or_else(String::new, |color| match color {
                Color::Rgb(c) => format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b),
                Color::Theme(_) => String::new(),
            }),
        color_mixed,
        font: font.unwrap_or_default(),
        font_mixed,
        highlight: highlight.map_or_else(String::new, |value| {
            value.map_or_else(|| "none".to_owned(), highlight_name)
        }),
        highlight_mixed,
        superscript: vertical_align == "super",
        subscript: vertical_align == "sub",
        vertical_align,
        vertical_align_mixed,
    }
}

/// The common value across all effective runs, or `None` when values disagree.
/// The value itself may be an `Option<T>`, which preserves uniformly-unset state
/// instead of conflating it with mixed.
fn uniform_value<T: Clone + PartialEq>(
    properties: &[RunProperties],
    value: impl Fn(&RunProperties) -> T,
) -> Option<T> {
    let first = value(&properties[0]);
    properties
        .iter()
        .skip(1)
        .all(|properties| value(properties) == first)
        .then_some(first)
}

/// The common value in a non-empty slice, or `None` when values disagree.
fn uniform_slice<T: Clone + PartialEq>(values: &[T]) -> Option<T> {
    let first = values.first()?.clone();
    values
        .iter()
        .skip(1)
        .all(|value| value == &first)
        .then_some(first)
}

/// A boolean value over the selected paragraphs: 0=off, 1=on, 2=mixed.
fn bool_selection_state(
    properties: &[ParagraphProperties],
    value: impl Fn(&ParagraphProperties) -> bool,
) -> u8 {
    let first = value(&properties[0]);
    if properties
        .iter()
        .skip(1)
        .all(|properties| value(properties) == first)
    {
        u8::from(first)
    } else {
        2
    }
}

fn alignment_name(alignment: Alignment) -> &'static str {
    match alignment {
        Alignment::Start => "start",
        Alignment::Center => "center",
        Alignment::End => "end",
        Alignment::Justify => "justify",
    }
}

fn paragraph_border_mask(borders: &ParagraphBorders) -> u8 {
    u8::from(borders.top.is_some())
        | (u8::from(borders.bottom.is_some()) << 1)
        | (u8::from(borders.start.is_some()) << 2)
        | (u8::from(borders.end.is_some()) << 3)
}

fn pack_rgb(color: RgbColor) -> u32 {
    (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)
}

/// A page-local rectangle flattened to `[page, x, y, w, h]` twips.
fn flat_rect(page: u32, rect: Rect) -> [i32; 5] {
    [
        page as i32,
        rect.origin.x.raw(),
        rect.origin.y.raw(),
        rect.size.width.raw(),
        rect.size.height.raw(),
    ]
}

/// The heading level (1-based; 1 = top) implied by a style `name` — `Title` or
/// `Heading N` (case- and whitespace-insensitive), else `None`.
fn heading_level_from_name(name: Option<&str>) -> Option<u8> {
    let compact: String = name?
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if compact == "title" {
        return Some(1);
    }
    compact
        .strip_prefix("heading")
        .and_then(|n| n.parse::<u8>().ok())
        .filter(|n| (1..=9).contains(n))
}

/// Whether `block` (recursively, through nested tables / content controls) holds the
/// paragraph `node` — locates the top-level body block a caret belongs to.
fn block_holds(block: &BlockNode, node: NodeId) -> bool {
    match block {
        BlockNode::Paragraph(p) => p.id == node,
        BlockNode::Table(t) => t.rows.iter().any(|r| {
            r.cells
                .iter()
                .any(|c| c.blocks.iter().any(|b| block_holds(b, node)))
        }),
        BlockNode::Sdt(s) => s.blocks.iter().any(|b| block_holds(b, node)),
        _ => false,
    }
}

/// A single-line border edge in the given RGB at `size_eighth_points` eighth-points.
fn border_edge(r: u8, g: u8, b: u8, size_eighth_points: u32) -> BorderEdge {
    BorderEdge {
        style: "single".to_string(),
        size_eighth_points: Some(size_eighth_points.clamp(2, 96)),
        color: Some(RgbColor { r, g, b }),
        space_points: None,
    }
}

/// Applies a border preset to a [`TableBorders`] (cell or table): `"none"` clears
/// all; `"box"` sets the four outer edges; `"all"` adds the inside gridlines too;
/// `"top"`/`"bottom"`/`"left"`/`"right"` toggle one outer edge. `mk` builds the edge.
fn set_table_borders_preset(bd: &mut TableBorders, edges: &str, mk: impl Fn() -> BorderEdge) {
    let toggle = |slot: &mut Option<BorderEdge>| {
        *slot = if slot.is_none() { Some(mk()) } else { None };
    };
    match edges {
        "none" => {
            bd.top = None;
            bd.bottom = None;
            bd.start = None;
            bd.end = None;
            bd.inside_h = None;
            bd.inside_v = None;
        }
        "box" => {
            bd.top = Some(mk());
            bd.bottom = Some(mk());
            bd.start = Some(mk());
            bd.end = Some(mk());
        }
        "all" => {
            bd.top = Some(mk());
            bd.bottom = Some(mk());
            bd.start = Some(mk());
            bd.end = Some(mk());
            bd.inside_h = Some(mk());
            bd.inside_v = Some(mk());
        }
        "top" => toggle(&mut bd.top),
        "bottom" => toggle(&mut bd.bottom),
        "left" => toggle(&mut bd.start),
        "right" => toggle(&mut bd.end),
        _ => {}
    }
}

/// Ruler tab-alignment code (0 start / 1 center / 2 end / 3 decimal / 4 bar) → model.
fn tab_alignment_from_code(code: u8) -> TabAlignment {
    match code {
        1 => TabAlignment::Center,
        2 => TabAlignment::End,
        3 => TabAlignment::Decimal,
        4 => TabAlignment::Bar,
        _ => TabAlignment::Start,
    }
}

/// Model tab alignment → ruler code (inverse of [`tab_alignment_from_code`]).
fn tab_alignment_code(alignment: TabAlignment) -> i32 {
    match alignment {
        TabAlignment::Start => 0,
        TabAlignment::Center => 1,
        TabAlignment::End => 2,
        TabAlignment::Decimal => 3,
        TabAlignment::Bar => 4,
    }
}

/// `text[from..to]` clamped to the string's byte length and snapped to char
/// boundaries — never panics on a stale or off-boundary offset.
fn slice_bytes(text: &str, from: usize, to: usize) -> String {
    let clamp = |i: usize| {
        let mut i = i.min(text.len());
        while i < text.len() && !text.is_char_boundary(i) {
            i += 1;
        }
        i
    };
    let (a, b) = (clamp(from), clamp(to));
    if a >= b {
        String::new()
    } else {
        text[a..b].to_string()
    }
}

struct ParagraphLink<'a> {
    link: &'a Hyperlink,
    range: ModelRange,
}

/// Collects hyperlink wrappers with ranges in the same byte-anchor space used by
/// shaping and hit-testing. Revisions and inline content controls are transparent,
/// matching the layout flattener.
fn paragraph_links<'a>(document: &Document, paragraph: &'a Paragraph) -> Vec<ParagraphLink<'a>> {
    fn walk<'a>(
        document: &Document,
        inlines: &'a [InlineNode],
        node: NodeId,
        offset: &mut u32,
        out: &mut Vec<ParagraphLink<'a>>,
    ) {
        for inline in inlines {
            match inline {
                InlineNode::Hyperlink(link) => {
                    let start = *offset;
                    *offset = offset.saturating_add(inlines_anchor_len(document, &link.inlines));
                    out.push(ParagraphLink {
                        link,
                        range: ModelRange::new(
                            ModelPos::new(node, start),
                            ModelPos::new(node, *offset),
                        ),
                    });
                }
                InlineNode::Revision(revision) => {
                    walk(document, &revision.inlines, node, offset, out)
                }
                InlineNode::Sdt(sdt) => walk(document, &sdt.inlines, node, offset, out),
                _ => {
                    *offset = offset.saturating_add(inline_anchor_len(document, inline));
                }
            }
        }
    }

    let mut links = Vec::new();
    let mut offset = 0;
    walk(
        document,
        &paragraph.inlines,
        paragraph.id,
        &mut offset,
        &mut links,
    );
    links
}

/// Number of UTF-8 bytes an inline contributes to the layout anchor stream.
/// This mirrors `tabs::split_blocks`: ordinary/positional tabs affect geometry
/// but are zero-width in the current model-offset space, while synthetic display
/// values contribute the bytes the shaper assigns them.
fn inline_anchor_len(document: &Document, inline: &InlineNode) -> u32 {
    match inline {
        InlineNode::Run(run) => run.text.len() as u32,
        InlineNode::Tab(_) | InlineNode::PositionalTab(_) => 0,
        InlineNode::Symbol(symbol) => {
            char::from_u32(symbol.char).map_or(0, |ch| ch.len_utf8() as u32)
        }
        InlineNode::Hyperlink(link) => inlines_anchor_len(document, &link.inlines),
        InlineNode::Revision(revision) => inlines_anchor_len(document, &revision.inlines),
        InlineNode::Sdt(sdt) => inlines_anchor_len(document, &sdt.inlines),
        InlineNode::Field(field) => field_anchor_len(field),
        InlineNode::NoteReference(reference) => {
            let notes = match reference.kind {
                casual_doc_model::v1::NoteKind::Footnote => &document.definitions().footnotes,
                casual_doc_model::v1::NoteKind::Endnote => &document.definitions().endnotes,
            };
            let ordinal = notes
                .iter()
                .position(|(id, _)| *id == reference.note)
                .map_or_else(|| "?".to_owned(), |index| (index + 1).to_string());
            ordinal.len() as u32
        }
        InlineNode::CommentReference(_) => "[comment]".len() as u32,
        InlineNode::Math(math) => {
            if math.text.is_empty() {
                "[equation]".len() as u32
            } else {
                math.text.len().saturating_add(2) as u32
            }
        }
        InlineNode::NoBreakHyphen(_) => '\u{2011}'.len_utf8() as u32,
        InlineNode::SoftHyphen(_) => '\u{00ad}'.len_utf8() as u32,
        InlineNode::EmbeddedObject(object) if object.preview.is_none() => match &object.kind {
            casual_doc_model::v1::EmbeddedKind::Chart => "[chart]".len() as u32,
            casual_doc_model::v1::EmbeddedKind::Diagram => "[diagram]".len() as u32,
            casual_doc_model::v1::EmbeddedKind::OleObject
            | casual_doc_model::v1::EmbeddedKind::Other(_) => "[object]".len() as u32,
        },
        _ => 0,
    }
}

fn inlines_anchor_len(document: &Document, inlines: &[InlineNode]) -> u32 {
    inlines.iter().fold(0u32, |total, inline| {
        total.saturating_add(inline_anchor_len(document, inline))
    })
}

fn field_anchor_len(field: &casual_doc_model::v1::Field) -> u32 {
    let cached = field.inlines.iter().fold(0u32, |total, inline| {
        let len = match inline {
            InlineNode::Run(run) => run.text.len() as u32,
            InlineNode::Tab(_) => 1,
            _ => 0,
        };
        total.saturating_add(len)
    });
    if cached > 0 {
        return cached;
    }
    matches!(
        field
            .instruction
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase()
            .as_str(),
        "PAGE" | "NUMPAGES"
    ) as u32
}

/// Resolves a bookmark name to the zero-width `BookmarkStart` marker in document
/// flow. Anchor strings are resolved through the definition table; they are never
/// interpreted as paragraph ids.
fn resolve_bookmark(document: &Document, anchor: &str) -> Option<ModelPos> {
    let bookmark = document
        .definitions()
        .bookmarks
        .iter()
        .find_map(|(id, definition)| (definition.name == anchor).then_some(*id))?;

    fn inlines_pos(
        document: &Document,
        inlines: &[InlineNode],
        node: NodeId,
        bookmark: BookmarkId,
        offset: &mut u32,
    ) -> Option<ModelPos> {
        for inline in inlines {
            match inline {
                InlineNode::BookmarkStart(marker) if marker.bookmark == bookmark => {
                    return Some(ModelPos::new(node, *offset));
                }
                InlineNode::Hyperlink(link) => {
                    if let Some(pos) = inlines_pos(document, &link.inlines, node, bookmark, offset)
                    {
                        return Some(pos);
                    }
                }
                InlineNode::Revision(revision) => {
                    if let Some(pos) =
                        inlines_pos(document, &revision.inlines, node, bookmark, offset)
                    {
                        return Some(pos);
                    }
                }
                InlineNode::Sdt(sdt) => {
                    if let Some(pos) = inlines_pos(document, &sdt.inlines, node, bookmark, offset) {
                        return Some(pos);
                    }
                }
                InlineNode::Field(field) => {
                    if let Some(pos) = inlines_pos(document, &field.inlines, node, bookmark, offset)
                    {
                        return Some(pos);
                    }
                }
                _ => {
                    *offset = offset.saturating_add(inline_anchor_len(document, inline));
                }
            }
        }
        None
    }

    fn blocks_pos(
        document: &Document,
        blocks: &[BlockNode],
        bookmark: BookmarkId,
    ) -> Option<ModelPos> {
        for block in blocks {
            match block {
                BlockNode::Paragraph(paragraph) => {
                    let mut offset = 0;
                    if let Some(pos) = inlines_pos(
                        document,
                        &paragraph.inlines,
                        paragraph.id,
                        bookmark,
                        &mut offset,
                    ) {
                        return Some(pos);
                    }
                }
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            if let Some(pos) = blocks_pos(document, &cell.blocks, bookmark) {
                                return Some(pos);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(pos) = blocks_pos(document, &sdt.blocks, bookmark) {
                        return Some(pos);
                    }
                }
                BlockNode::AltChunk(_) => {}
            }
        }
        None
    }

    blocks_pos(document, document.body(), bookmark)
}

/// The model anchor a point resolved to (doc 58 §2 `TextCaret`/`Outside`): a
/// `NodeId` (32-hex string), a node-relative UTF-8 byte offset, and whether the
/// point was inside content or snapped in from outside.
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct HitPayload {
    node: String,
    offset: u32,
    zone: &'static str,
}

#[wasm_bindgen]
impl HitPayload {
    /// The anchor node id (32-hex string; compare/pass back, never arithmetic).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn node(&self) -> String {
        self.node.clone()
    }

    /// The node-relative UTF-8 byte offset.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// `"content"` (inside a line) or `"outside"` (snapped from a margin).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn zone(&self) -> String {
        self.zone.to_string()
    }
}

/// A hyperlink resolved at a page-local point. The runtime reports both the
/// authored target and, for internal links, the resolved bookmark caret/page;
/// the embedding host decides whether and how navigation is allowed.
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct LinkHit {
    kind: &'static str,
    url: String,
    anchor: String,
    tooltip: String,
    start_node: String,
    start_offset: u32,
    end_node: String,
    end_offset: u32,
    target_node: String,
    target_offset: u32,
    target_page: u32,
}

#[wasm_bindgen]
impl LinkHit {
    /// `"external"` or `"internal"`.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn kind(&self) -> String {
        self.kind.to_owned()
    }

    /// External target URL, or empty for an internal target.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// Internal bookmark name, or empty for an external target.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn anchor(&self) -> String {
        self.anchor.clone()
    }

    /// Optional authored screen tip (empty when absent).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn tooltip(&self) -> String {
        self.tooltip.clone()
    }

    /// Paragraph node containing the linked range.
    #[wasm_bindgen(getter, js_name = startNode)]
    #[must_use]
    pub fn start_node(&self) -> String {
        self.start_node.clone()
    }

    /// Start byte offset of the linked range.
    #[wasm_bindgen(getter, js_name = startOffset)]
    #[must_use]
    pub fn start_offset(&self) -> u32 {
        self.start_offset
    }

    /// Paragraph node ending the linked range (currently identical to startNode).
    #[wasm_bindgen(getter, js_name = endNode)]
    #[must_use]
    pub fn end_node(&self) -> String {
        self.end_node.clone()
    }

    /// End byte offset of the linked range.
    #[wasm_bindgen(getter, js_name = endOffset)]
    #[must_use]
    pub fn end_offset(&self) -> u32 {
        self.end_offset
    }

    /// Resolved internal-bookmark paragraph node, or empty if unresolved/external.
    #[wasm_bindgen(getter, js_name = targetNode)]
    #[must_use]
    pub fn target_node(&self) -> String {
        self.target_node.clone()
    }

    /// Resolved internal-bookmark byte offset.
    #[wasm_bindgen(getter, js_name = targetOffset)]
    #[must_use]
    pub fn target_offset(&self) -> u32 {
        self.target_offset
    }

    /// Resolved internal-bookmark 1-based page, or 0 if unresolved/external.
    #[wasm_bindgen(getter, js_name = targetPage)]
    #[must_use]
    pub fn target_page(&self) -> u32 {
        self.target_page
    }
}

/// Current table context for editor table-selection and table-property controls.
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct TableInfo {
    found: bool,
    table: String,
    row: u32,
    column: u32,
    rows: u32,
    columns: u32,
    regular: bool,
    header_row: bool,
    column_width_twips: i32,
    table_width_twips: i32,
    table_indent_twips: i32,
    fixed_layout: bool,
    row_height_twips: i32,
    row_height_rule: String,
    cell_margin_twips: i32,
    cell_spacing_twips: i32,
    alignment: String,
    caption: String,
    description: String,
}

impl TableInfo {
    fn none() -> Self {
        Self {
            found: false,
            table: String::new(),
            row: 0,
            column: 0,
            rows: 0,
            columns: 0,
            regular: false,
            header_row: false,
            column_width_twips: -1,
            table_width_twips: -1,
            table_indent_twips: 0,
            fixed_layout: false,
            row_height_twips: -1,
            row_height_rule: String::new(),
            cell_margin_twips: -1,
            cell_spacing_twips: -1,
            alignment: "left".to_owned(),
            caption: String::new(),
            description: String::new(),
        }
    }
}

#[wasm_bindgen]
impl TableInfo {
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn found(&self) -> bool {
        self.found
    }

    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn table(&self) -> String {
        self.table.clone()
    }

    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn row(&self) -> u32 {
        self.row
    }

    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn column(&self) -> u32 {
        self.column
    }

    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn rows(&self) -> u32 {
        self.rows
    }

    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn columns(&self) -> u32 {
        self.columns
    }

    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn regular(&self) -> bool {
        self.regular
    }

    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn caption(&self) -> String {
        self.caption.clone()
    }

    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn description(&self) -> String {
        self.description.clone()
    }

    #[wasm_bindgen(getter, js_name = headerRow)]
    #[must_use]
    pub fn header_row(&self) -> bool {
        self.header_row
    }

    #[wasm_bindgen(getter, js_name = columnWidthTwips)]
    #[must_use]
    pub fn column_width_twips(&self) -> i32 {
        self.column_width_twips
    }

    #[wasm_bindgen(getter, js_name = tableWidthTwips)]
    #[must_use]
    pub fn table_width_twips(&self) -> i32 {
        self.table_width_twips
    }

    #[wasm_bindgen(getter, js_name = tableIndentTwips)]
    #[must_use]
    pub fn table_indent_twips(&self) -> i32 {
        self.table_indent_twips
    }

    #[wasm_bindgen(getter, js_name = fixedLayout)]
    #[must_use]
    pub fn fixed_layout(&self) -> bool {
        self.fixed_layout
    }

    #[wasm_bindgen(getter, js_name = rowHeightTwips)]
    #[must_use]
    pub fn row_height_twips(&self) -> i32 {
        self.row_height_twips
    }

    #[wasm_bindgen(getter, js_name = rowHeightRule)]
    #[must_use]
    pub fn row_height_rule(&self) -> String {
        self.row_height_rule.clone()
    }

    #[wasm_bindgen(getter, js_name = cellMarginTwips)]
    #[must_use]
    pub fn cell_margin_twips(&self) -> i32 {
        self.cell_margin_twips
    }

    #[wasm_bindgen(getter, js_name = cellSpacingTwips)]
    #[must_use]
    pub fn cell_spacing_twips(&self) -> i32 {
        self.cell_spacing_twips
    }

    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn alignment(&self) -> String {
        self.alignment.clone()
    }
}

/// A page box size in twips (doc 57 §4.2).
#[wasm_bindgen(js_name = PageSize)]
#[derive(Clone, Copy, Debug)]
pub struct PageSize {
    width_twip: i32,
    height_twip: i32,
}

#[wasm_bindgen(js_class = PageSize)]
impl PageSize {
    /// Page box width in twips (1/1440 in).
    #[wasm_bindgen(getter, js_name = widthTwip)]
    #[must_use]
    pub fn width_twip(&self) -> i32 {
        self.width_twip
    }

    /// Page box height in twips (1/1440 in).
    #[wasm_bindgen(getter, js_name = heightTwip)]
    #[must_use]
    pub fn height_twip(&self) -> i32 {
        self.height_twip
    }
}

/// A rasterized page: RGBA8888 premultiplied pixels plus their device dimensions
/// (doc 57 §5.1). Blit with `new ImageData(rgba, widthPx, heightPx)`.
#[wasm_bindgen]
#[derive(Debug)]
pub struct PageBitmap {
    width_px: u32,
    height_px: u32,
    rgba: Vec<u8>,
}

#[wasm_bindgen]
impl PageBitmap {
    /// Bitmap width in device pixels.
    #[wasm_bindgen(getter, js_name = widthPx)]
    #[must_use]
    pub fn width_px(&self) -> u32 {
        self.width_px
    }

    /// Bitmap height in device pixels.
    #[wasm_bindgen(getter, js_name = heightPx)]
    #[must_use]
    pub fn height_px(&self) -> u32 {
        self.height_px
    }

    /// The premultiplied RGBA8888 pixels, row-major, as a `Uint8ClampedArray` —
    /// the exact backing an `ImageData` takes. Copies out of the surface.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn rgba(&self) -> Clamped<Vec<u8>> {
        Clamped(self.rgba.clone())
    }
}

/// Imports + paginates a `.docx` into a [`WasmDocument`], returning a plain
/// message on failure. Split from the `#[wasm_bindgen]` [`open`] so it runs
/// under native `cargo test`.
fn open_document(bytes: &[u8]) -> Result<WasmDocument, String> {
    let mut package =
        DocxPackage::open(bytes, viewer_limits()).map_err(|e| format!("open package: {e:?}"))?;
    let imported = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .map_err(|e| format!("import document: {e:?}"))?;
    let document = imported.document;

    // Snapshot the inline-image bytes rendering will need, so the handle owns
    // everything and the package can be dropped.
    let mut media = BTreeMap::new();
    for (_id, reference) in document.definitions().media.iter() {
        if let Ok(part_bytes) = package.read_part(&reference.part_name) {
            media.insert(reference.part_name.clone(), part_bytes);
        }
    }

    let shaper = ParleyShaper::new();
    // One call: per-section geometry, flowed headers/footers, anchored drawings,
    // and page-number fields — the same entry point the native renderer uses.
    let layout = paginate_document(&document, &shaper);
    let default_config = document_page_config(&document);

    // Edits allocate run ids in a namespace derived from — but distinct from —
    // the document's own, so a new run can never collide with an imported node.
    let edit_namespace = ((document.id().as_u128() >> 64) as u64) ^ 0xED17_ED17_ED17_ED17;

    Ok(WasmDocument {
        document,
        layout,
        shaper,
        media,
        default_config,
        edit_ids: IdGenerator::new(edit_namespace),
        undo: Vec::new(),
        redo: Vec::new(),
        typing_history: None,
        revision: 0,
        // Populated lazily on the first edit's incremental re-pagination; the open
        // above uses the full path since there is nothing yet to reuse.
        galley_cache: GalleyCache::new(),
        // List definitions are created on demand by the first bullet/numbered toggle.
        bullet_list: None,
        numbered_list: None,
    })
}

/// Converts an internal error message to a thrown JS `Error`. Only ever runs at
/// the `#[wasm_bindgen]` boundary (never under native tests, where constructing a
/// `JsValue` would panic). Placeholder until the structured `SdkError` model
/// carries `code`/`severity` across the boundary (doc 57 §5.5).
fn to_js(message: String) -> JsValue {
    JsError::new(&message).into()
}

/// Maps a highlight name (case-insensitive) to a [`HighlightColor`]; unknown
/// names fall back to `Yellow`, `"none"`/`""` clear the highlight.
/// Parses a `#rrggbb` (or `rrggbb`) hex color, or `None` if malformed.
fn parse_hex_color(hex: &str) -> Option<RgbColor> {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    if h.len() != 6 {
        return None;
    }
    Some(RgbColor {
        r: u8::from_str_radix(&h[0..2], 16).ok()?,
        g: u8::from_str_radix(&h[2..4], 16).ok()?,
        b: u8::from_str_radix(&h[4..6], 16).ok()?,
    })
}

fn parse_highlight(name: &str) -> HighlightColor {
    match name.to_ascii_lowercase().as_str() {
        "none" | "" => HighlightColor::None,
        "black" => HighlightColor::Black,
        "blue" => HighlightColor::Blue,
        "cyan" => HighlightColor::Cyan,
        "green" => HighlightColor::Green,
        "magenta" => HighlightColor::Magenta,
        "red" => HighlightColor::Red,
        "white" => HighlightColor::White,
        "darkgray" | "gray" | "grey" => HighlightColor::DarkGray,
        "lightgray" => HighlightColor::LightGray,
        _ => HighlightColor::Yellow,
    }
}

/// Parses a 32-hex node-id string, or a thrown JS error.
fn node_id(node: &str) -> Result<NodeId, JsValue> {
    NodeId::from_str(node).map_err(|_| to_js(format!("invalid node id: {node}")))
}

/// Where the caret lands after `op` is applied: end of an insertion, start of a
/// deletion, start of the new paragraph after a split, and the join seam after a
/// join (recovered from the inverse split's boundary).
/// The id of the first paragraph in a row's first cell (where a caret goes after a
/// row edit), if the row has a leading paragraph.
fn first_paragraph_of_row(row: &TableRow) -> Option<NodeId> {
    row.cells.first().and_then(first_paragraph_of_cell)
}

/// The id of a cell's first paragraph, if it has one.
fn first_paragraph_of_cell(cell: &TableCell) -> Option<NodeId> {
    cell.blocks.iter().find_map(|b| match b {
        BlockNode::Paragraph(p) => Some(p.id),
        _ => None,
    })
}

fn first_paragraph_of_cell_id(blocks: &[BlockNode], cell_id: NodeId) -> Option<NodeId> {
    for block in blocks {
        match block {
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        if cell.id == cell_id {
                            return first_paragraph_of_cell(cell);
                        }
                        if let Some(paragraph) = first_paragraph_of_cell_id(&cell.blocks, cell_id) {
                            return Some(paragraph);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(paragraph) = first_paragraph_of_cell_id(&sdt.blocks, cell_id) {
                    return Some(paragraph);
                }
            }
            _ => {}
        }
    }
    None
}

fn table_column_count(table: &Table) -> usize {
    if !table.grid.is_empty() {
        return table.grid.len();
    }
    table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.properties.grid_span.unwrap_or(1).max(1) as usize)
                .sum()
        })
        .max()
        .unwrap_or(0)
}

fn active_column_width(table: &Table, col_index: usize) -> Option<i32> {
    table
        .grid
        .get(col_index)
        .and_then(|col| col.width_twips)
        .or_else(|| {
            table
                .rows
                .iter()
                .find_map(|row| row.cells.get(col_index)?.properties.width_twips)
        })
}

fn height_rule_name(rule: HeightRule) -> String {
    match rule {
        HeightRule::Auto => "auto",
        HeightRule::AtLeast => "atLeast",
        HeightRule::Exact => "exact",
    }
    .to_owned()
}

fn uniform_margins(margins: CellMargins) -> Option<i32> {
    let top = margins.top_twips?;
    (margins.start_twips == Some(top)
        && margins.bottom_twips == Some(top)
        && margins.end_twips == Some(top))
    .then_some(top)
}

fn ensure_optional_twips(label: &str, value: i32) -> Result<(), JsValue> {
    if (-1..=31_680).contains(&value) {
        Ok(())
    } else {
        Err(to_js(format!(
            "{label} must be -1 or between 0 and 31680 twips"
        )))
    }
}

fn ensure_signed_twips(label: &str, value: i32) -> Result<(), JsValue> {
    if (-31_680..=31_680).contains(&value) {
        Ok(())
    } else {
        Err(to_js(format!(
            "{label} must be between -31680 and 31680 twips"
        )))
    }
}

fn required_twips(label: &str, value: i32) -> Result<i32, JsValue> {
    if (0..=31_680).contains(&value) {
        Ok(value)
    } else {
        Err(to_js(format!("{label} must be between 0 and 31680 twips")))
    }
}

fn normalize_table_metadata(label: &str, value: String) -> Result<Option<String>, JsValue> {
    let value = value.trim().to_owned();
    if value.len() > 255 {
        return Err(to_js(format!("{label} must be at most 255 bytes")));
    }
    Ok((!value.is_empty()).then_some(value))
}

fn distribute_twips(total: i64, parts: usize) -> Result<Vec<i32>, JsValue> {
    if parts == 0 || total < parts as i64 || total > i64::from(i32::MAX) {
        return Err(to_js(
            "table dimensions are outside the supported range".into(),
        ));
    }
    let base = total / parts as i64;
    let remainder = (total % parts as i64) as usize;
    (0..parts)
        .map(|index| {
            i32::try_from(base + i64::from((index < remainder) as u8))
                .map_err(|_| to_js("table dimensions are outside the supported range".into()))
        })
        .collect()
}

fn table_is_regular(table: &Table) -> bool {
    let cols = table_column_count(table);
    if cols == 0 {
        return false;
    }
    table.rows.iter().all(|row| {
        row.cells.len() == cols
            && row.cells.iter().all(|cell| {
                cell.properties.grid_span.is_none_or(|span| span <= 1)
                    && cell.properties.vertical_merge.is_none()
            })
    })
}

fn table_selection_anchors(
    table: &Table,
    row_index: usize,
    col_index: usize,
    mode: &str,
) -> Vec<NodeId> {
    let mut out = Vec::new();
    match mode {
        "row" => {
            if let Some(row) = table.rows.get(row_index) {
                for cell in &row.cells {
                    if let Some(anchor) = first_paragraph_of_cell(cell) {
                        out.push(anchor);
                    }
                }
            }
        }
        "column" => {
            if table_is_regular(table) {
                for row in &table.rows {
                    if let Some(anchor) = row.cells.get(col_index).and_then(first_paragraph_of_cell)
                    {
                        out.push(anchor);
                    }
                }
            }
        }
        "table" => {
            for row in &table.rows {
                for cell in &row.cells {
                    if let Some(anchor) = first_paragraph_of_cell(cell) {
                        out.push(anchor);
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn first_cell_plain_text(row: &TableRow) -> Result<String, String> {
    let cell = row
        .cells
        .first()
        .ok_or_else(|| "row has no first cell".to_owned())?;
    let paragraph = cell
        .blocks
        .first()
        .and_then(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .ok_or_else(|| "sorting requires paragraph text in the first cell".to_owned())?;
    Ok(node_plain_text(&paragraph.inlines))
}

fn parse_table_formula(formula: &str) -> Result<(String, String), JsValue> {
    let formula = formula.trim().to_ascii_uppercase();
    let (operation, rest) = formula
        .split_once('(')
        .ok_or_else(|| to_js("formula must look like =SUM(ABOVE)".into()))?;
    let range = rest
        .strip_suffix(')')
        .ok_or_else(|| to_js("formula must end with ')'".into()))?;
    let operation = operation.strip_prefix('=').unwrap_or(operation);
    if !matches!(operation, "SUM" | "AVERAGE" | "MIN" | "MAX") || !matches!(range, "ABOVE" | "LEFT")
    {
        return Err(to_js(
            "supported formulas are SUM, AVERAGE, MIN, or MAX over ABOVE or LEFT".into(),
        ));
    }
    // Leak-free static storage is unnecessary: the owned uppercase string is
    // represented by the caller's temporary only for this synchronous parse.
    // Reparse from the original string using slices after validation instead.
    let original = formula;
    let (operation, rest) = original.split_once('(').unwrap();
    Ok((
        operation.strip_prefix('=').unwrap_or(operation).to_owned(),
        rest.strip_suffix(')').unwrap().to_owned(),
    ))
}

fn formula_values(
    table: &Table,
    row: usize,
    column: usize,
    range: &str,
) -> Result<Vec<f64>, JsValue> {
    let mut values = Vec::new();
    let cells = match range {
        "ABOVE" => table.rows[..row]
            .iter()
            .map(|r| &r.cells[column])
            .collect::<Vec<_>>(),
        "LEFT" => table.rows[row].cells[..column].iter().collect::<Vec<_>>(),
        _ => return Err(to_js("unsupported formula range".into())),
    };
    for cell in cells {
        let text = cell
            .blocks
            .first()
            .and_then(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(node_plain_text(&paragraph.inlines)),
                _ => None,
            })
            .ok_or_else(|| to_js("formula range contains a non-paragraph cell".into()))?;
        if let Ok(value) = text.trim().parse::<f64>() {
            values.push(value);
        }
    }
    Ok(values)
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

fn merge_regular_table_selection(
    mut table: Table,
    r0: usize,
    r1: usize,
    c0: usize,
    c1: usize,
    ids: &mut IdGenerator,
) -> Result<Table, String> {
    if !table_is_regular(&table) || r0 > r1 || c0 > c1 || r1 >= table.rows.len() {
        return Err("selection is not a regular rectangular table range".into());
    }
    let cols = table_column_count(&table);
    if c1 >= cols {
        return Err("selection is outside the table".into());
    }
    let width = c1 - c0 + 1;
    let height = r1 - r0 + 1;
    let mut merged_blocks = Vec::new();
    for row in &table.rows[r0..=r1] {
        for cell in &row.cells[c0..=c1] {
            merged_blocks.extend(cell.blocks.clone());
        }
    }

    for r in r0..=r1 {
        let row = table
            .rows
            .get_mut(r)
            .ok_or_else(|| "selection row is outside the table".to_owned())?;
        if row.cells.len() < c1 + 1 {
            return Err("selection column is outside the row".into());
        }
        row.cells.drain(c0 + 1..=c1);
        let cell = row
            .cells
            .get_mut(c0)
            .ok_or_else(|| "selection target cell missing".to_owned())?;
        cell.properties.grid_span = (width > 1).then_some(width as u32);
        if height > 1 {
            cell.properties.vertical_merge = Some(if r == r0 {
                VerticalMerge::Restart
            } else {
                VerticalMerge::Continue
            });
        } else {
            cell.properties.vertical_merge = None;
        }
        if r == r0 {
            cell.blocks = merged_blocks.clone();
        } else {
            cell.blocks = vec![empty_paragraph_block(ids)?];
        }
    }
    Ok(table)
}

fn split_table_cell(
    mut table: Table,
    row_index: usize,
    col_index: usize,
    ids: &mut IdGenerator,
) -> Result<Table, String> {
    let row = table
        .rows
        .get(row_index)
        .ok_or_else(|| "cell row is outside the table".to_owned())?;
    let cell = row
        .cells
        .get(col_index)
        .ok_or_else(|| "cell column is outside the table".to_owned())?;
    if matches!(
        cell.properties.vertical_merge,
        Some(VerticalMerge::Continue)
    ) {
        return Err("split from the top-left merged cell".into());
    }
    let width = cell.properties.grid_span.unwrap_or(1).max(1) as usize;
    let mut height = 1usize;
    if matches!(cell.properties.vertical_merge, Some(VerticalMerge::Restart)) {
        for row in table.rows.iter().skip(row_index + 1) {
            let Some(next) = row.cells.get(col_index) else {
                break;
            };
            if next.properties.vertical_merge == Some(VerticalMerge::Continue)
                && next.properties.grid_span.unwrap_or(1).max(1) as usize == width
            {
                height += 1;
            } else {
                break;
            }
        }
    }
    if width == 1 && height == 1 {
        return Err("cell is not merged".into());
    }

    for r in row_index..row_index + height {
        let row = table
            .rows
            .get_mut(r)
            .ok_or_else(|| "merged row is outside the table".to_owned())?;
        let cell = row
            .cells
            .get_mut(col_index)
            .ok_or_else(|| "merged cell is outside the row".to_owned())?;
        cell.properties.grid_span = None;
        cell.properties.vertical_merge = None;
        if r != row_index {
            cell.blocks = vec![empty_paragraph_block(ids)?];
        }
        for offset in 1..width {
            row.cells
                .insert(col_index + offset, empty_table_cell(ids, None)?);
        }
    }
    Ok(table)
}

fn split_table_cell_counts(
    table: Table,
    row_index: usize,
    col_index: usize,
    requested_rows: usize,
    requested_columns: usize,
    ids: &mut IdGenerator,
) -> Result<Table, String> {
    if requested_rows == 0 && requested_columns == 0 {
        return split_table_cell(table, row_index, col_index, ids);
    }
    let mut table = table;
    let row = table
        .rows
        .get(row_index)
        .ok_or_else(|| "cell row is outside the table".to_owned())?;
    let cell = row
        .cells
        .get(col_index)
        .ok_or_else(|| "cell column is outside the table".to_owned())?;
    if cell.properties.vertical_merge.is_some() {
        return Err("custom split counts currently require a horizontal merged cell".into());
    }
    let width = cell.properties.grid_span.unwrap_or(1).max(1) as usize;
    if width == 1 {
        return Err("cell is not merged".into());
    }
    let columns = if requested_columns == 0 {
        width
    } else {
        requested_columns
    };
    let rows = if requested_rows == 0 {
        1
    } else {
        requested_rows
    };
    if rows != 1 || columns < width || columns > 20 {
        return Err("custom split supports one row and at least the current column span".into());
    }
    let grid_widths = table
        .grid
        .get(col_index..col_index + width)
        .ok_or_else(|| "merged cell is outside the table grid".to_owned())?
        .iter()
        .map(|column| column.width_twips.unwrap_or(1))
        .sum::<i32>();
    let widths = distribute_twips(i64::from(grid_widths), columns)
        .map_err(|_| "merged cell width is outside the supported range".to_owned())?;
    table.grid.splice(
        col_index..col_index + width,
        widths.iter().map(|width| GridColumn {
            width_twips: Some(*width),
        }),
    );
    for row in &mut table.rows {
        if row.cells.len() < col_index + 1 {
            return Err("merged cell is outside the row".into());
        }
        let merged = row.cells[col_index].properties.grid_span.unwrap_or(1) > 1;
        if merged {
            let anchor = row.cells.remove(col_index);
            let mut cells = Vec::with_capacity(columns);
            let mut first = anchor;
            first.properties.grid_span = None;
            first.properties.width_twips = Some(widths[0]);
            cells.push(first);
            for width in widths.iter().skip(1) {
                cells.push(empty_table_cell(ids, Some(*width))?);
            }
            row.cells.splice(col_index..col_index, cells);
        } else {
            for width in widths.iter().skip(width) {
                row.cells
                    .insert(col_index + 1, empty_table_cell(ids, Some(*width))?);
            }
        }
    }
    Ok(table)
}

fn empty_table_cell(ids: &mut IdGenerator, width_twips: Option<i32>) -> Result<TableCell, String> {
    let cell_id = ids.next_id().map_err(|_| "id space exhausted".to_owned())?;
    Ok(TableCell {
        id: cell_id,
        properties: TableCellProperties {
            width_twips,
            ..TableCellProperties::default()
        },
        blocks: vec![empty_paragraph_block(ids)?],
    })
}

fn empty_paragraph_block(ids: &mut IdGenerator) -> Result<BlockNode, String> {
    let para_id = ids.next_id().map_err(|_| "id space exhausted".to_owned())?;
    Ok(BlockNode::Paragraph(Paragraph {
        id: para_id,
        properties: ParagraphProperties::default(),
        inlines: Vec::new(),
    }))
}

/// One generated list level: a bullet glyph or a nested decimal marker,
/// indented so the marker hangs to the left of the body text. These are the
/// nine levels accepted by `adjustListLevel` (0..=8), so editor-created lists
/// remain valid when users promote items repeatedly and export the document.
fn list_level(numbered: bool, level: u8) -> NumberingLevel {
    let (num_fmt, lvl_text) = if numbered {
        let placeholders = (1..=level + 1)
            .map(|part| format!("%{part}"))
            .collect::<Vec<_>>()
            .join(".");
        (NumberFormat::Decimal, format!("{placeholders}."))
    } else {
        let glyph = ["\u{2022}", "\u{25e6}", "\u{25aa}", "\u{2013}"][usize::from(level) % 4];
        (NumberFormat::Bullet, glyph.to_string())
    };
    NumberingLevel {
        level,
        start: 1,
        num_fmt: Some(num_fmt),
        lvl_text: Some(lvl_text),
        lvl_jc: Some(LevelJustification::Start),
        suff: Some(LevelSuffix::Tab),
        is_lgl: false,
        paragraph_properties: Some(ParagraphProperties {
            indentation: Some(Indentation {
                start_twips: Some(720 + i32::from(level) * 360),
                end_twips: None,
                first_line_twips: None,
                hanging_twips: Some(360),
            }),
            ..ParagraphProperties::default()
        }),
        run_properties: None,
        style_ref: None,
    }
}

fn caret_after(op: &Operation, inverse: &Operation, document: &Document) -> Pos {
    let doc_id = document.id();
    let table_anchor = |table| {
        find_table(document, table)
            .and_then(|table| table.rows.first())
            .and_then(first_paragraph_of_row)
            .map_or_else(|| Pos::new(table, 0), |paragraph| Pos::new(paragraph, 0))
    };
    match op {
        Operation::InsertText { at, text } => Pos::new(at.node, at.offset + text.len() as u32),
        Operation::DeleteText { range } => range.start,
        Operation::SplitParagraph { new_id, .. } => Pos::new(*new_id, 0),
        Operation::JoinParagraphs { first, .. } => match inverse {
            Operation::SplitParagraph { at, .. } => *at,
            _ => Pos::new(*first, 0),
        },
        // Formatting keeps the selection; the frontend does not collapse to this.
        Operation::FormatText { range, .. } | Operation::ClearFormatting { range } => range.start,
        Operation::SetHyperlink { range, .. } => range.start,
        Operation::SetInlines { node, .. } | Operation::SetParagraphProperties { node, .. } => {
            Pos::new(*node, 0)
        }
        // Row ops go through `apply_action_caret`, which overrides the caret; these
        // arms only keep the match exhaustive with a sensible fallback.
        Operation::InsertRow { table, row, .. } => {
            first_paragraph_of_row(row).map_or_else(|| Pos::new(*table, 0), |p| Pos::new(p, 0))
        }
        Operation::DeleteRow { table, .. } => table_anchor(*table),
        Operation::InsertColumn { table, cells, .. } => cells
            .first()
            .and_then(first_paragraph_of_cell)
            .map_or_else(|| Pos::new(*table, 0), |p| Pos::new(p, 0)),
        Operation::DeleteColumn { table, .. } => table_anchor(*table),
        // Row/table ops go through `apply_action_caret`, which overrides the caret.
        Operation::DeleteTable { table } => Pos::new(*table, 0),
        Operation::InsertTable { table, .. } => table
            .rows
            .first()
            .and_then(first_paragraph_of_row)
            .map_or_else(|| Pos::new(table.id, 0), |p| Pos::new(p, 0)),
        // Cell/table formatting keeps the selection (the frontend does not collapse
        // to this); these arms only keep the match exhaustive.
        Operation::SetTableCellProperties { cell, .. } => {
            first_paragraph_of_cell_id(document.body(), *cell)
                .map_or_else(|| Pos::new(*cell, 0), |paragraph| Pos::new(paragraph, 0))
        }
        Operation::SetTableProperties { table, .. } | Operation::ReplaceTable { table, .. } => {
            table_anchor(*table)
        }
        // Document-global, not node-scoped — always routed through
        // `apply_action_caret` with the caller's own caret, so this natural
        // position is never actually used; the document's own root id is a
        // neutral placeholder (matches `apply_group`'s own default caret).
        Operation::SetCoreProperties { .. } => Pos::new(doc_id, 0),
        // Also document-global — see the SetCoreProperties comment above.
        Operation::SetSectionGeometry { .. } => Pos::new(doc_id, 0),
    }
}

/// The byte index of the character boundary at or before `offset - 1` — the start
/// of the character a backspace removes.
fn prev_char_boundary(text: &str, offset: usize) -> usize {
    let mut i = offset.min(text.len()).saturating_sub(1);
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// The byte index of the next character boundary after `offset` — the end of the
/// character a forward-delete removes.
fn next_char_boundary(text: &str, offset: usize) -> usize {
    let mut i = (offset + 1).min(text.len());
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// The result of an edit: the new caret, the revision, the page count, and the
/// **dirty page indices** (0-based) whose layout changed — the only pages the
/// frontend must re-raster.
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct EditResult {
    node: String,
    offset: u32,
    revision: u32,
    page_count: u32,
    dirty: Vec<u32>,
}

#[wasm_bindgen]
impl EditResult {
    /// The caret anchor node id (32-hex string).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn node(&self) -> String {
        self.node.clone()
    }

    /// The caret byte offset within the node.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// The model revision after the edit.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn revision(&self) -> u32 {
        self.revision
    }

    /// The page count after the edit (so the frontend can add/remove pages).
    #[wasm_bindgen(getter, js_name = pageCount)]
    #[must_use]
    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    /// The 0-based indices of pages whose layout changed — re-raster only these.
    #[wasm_bindgen(getter, js_name = dirtyPages)]
    #[must_use]
    pub fn dirty_pages(&self) -> Vec<u32> {
        self.dirty.clone()
    }
}

/// The uniform run-format state of a selection (each `true` only when every
/// covered run sets that toggle) — for a formatting toolbar.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Default)]
pub struct Format {
    bold_state: u8,
    italic_state: u8,
    underline_state: u8,
    strike_state: u8,
}

#[wasm_bindgen]
impl Format {
    /// Every covered run is bold.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn bold(&self) -> bool {
        self.bold_state == 1
    }

    /// Bold state: 0 off, 1 on, 2 mixed.
    #[wasm_bindgen(getter, js_name = boldState)]
    #[must_use]
    pub fn bold_state(&self) -> u8 {
        self.bold_state
    }

    /// Every covered run is italic.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn italic(&self) -> bool {
        self.italic_state == 1
    }

    /// Italic state: 0 off, 1 on, 2 mixed.
    #[wasm_bindgen(getter, js_name = italicState)]
    #[must_use]
    pub fn italic_state(&self) -> u8 {
        self.italic_state
    }

    /// Every covered run is underlined.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn underline(&self) -> bool {
        self.underline_state == 1
    }

    /// Underline state: 0 off, 1 on, 2 mixed.
    #[wasm_bindgen(getter, js_name = underlineState)]
    #[must_use]
    pub fn underline_state(&self) -> u8 {
        self.underline_state
    }

    /// Every covered run is struck through.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn strike(&self) -> bool {
        self.strike_state == 1
    }

    /// Strike-through state: 0 off, 1 on, 2 mixed.
    #[wasm_bindgen(getter, js_name = strikeState)]
    #[must_use]
    pub fn strike_state(&self) -> u8 {
        self.strike_state
    }
}

/// An all-unset indentation, the base a ruler drag mutates.
const EMPTY_INDENT: Indentation = Indentation {
    start_twips: None,
    end_twips: None,
    first_line_twips: None,
    hanging_twips: None,
};

/// A page's ruler geometry (twips).
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Default)]
pub struct RulerGeometry {
    width_twip: i32,
    margin_start_twip: i32,
    margin_end_twip: i32,
}

#[wasm_bindgen]
impl RulerGeometry {
    /// Page width in twips.
    #[wasm_bindgen(getter, js_name = widthTwip)]
    #[must_use]
    pub fn width_twip(&self) -> i32 {
        self.width_twip
    }

    /// Left (start) margin in twips.
    #[wasm_bindgen(getter, js_name = marginStartTwip)]
    #[must_use]
    pub fn margin_start_twip(&self) -> i32 {
        self.margin_start_twip
    }

    /// Right (end) margin in twips.
    #[wasm_bindgen(getter, js_name = marginEndTwip)]
    #[must_use]
    pub fn margin_end_twip(&self) -> i32 {
        self.margin_end_twip
    }
}

/// A paragraph's indentation (twips), for the ruler markers.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Default)]
pub struct Indents {
    start_twip: i32,
    end_twip: i32,
    first_line_twip: i32,
    hanging_twip: i32,
}

#[wasm_bindgen]
impl Indents {
    /// Left (start) indent in twips.
    #[wasm_bindgen(getter, js_name = startTwip)]
    #[must_use]
    pub fn start_twip(&self) -> i32 {
        self.start_twip
    }

    /// Right (end) indent in twips.
    #[wasm_bindgen(getter, js_name = endTwip)]
    #[must_use]
    pub fn end_twip(&self) -> i32 {
        self.end_twip
    }

    /// First-line indent in twips (relative to the left indent).
    #[wasm_bindgen(getter, js_name = firstLineTwip)]
    #[must_use]
    pub fn first_line_twip(&self) -> i32 {
        self.first_line_twip
    }

    /// Hanging indent in twips (mutually exclusive with first-line).
    #[wasm_bindgen(getter, js_name = hangingTwip)]
    #[must_use]
    pub fn hanging_twip(&self) -> i32 {
        self.hanging_twip
    }
}

/// A paragraph's spacing, for the toolbar's line-&-paragraph-spacing menu.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Default)]
pub struct ParagraphSpacing {
    before_twip: i32,
    after_twip: i32,
    line_percent: u32,
    line_rule: u8,
    line_twip: i32,
}

#[wasm_bindgen]
impl ParagraphSpacing {
    /// Space before the paragraph in twips, or `-1` when unset.
    #[wasm_bindgen(getter, js_name = beforeTwip)]
    #[must_use]
    pub fn before_twip(&self) -> i32 {
        self.before_twip
    }

    /// Space after the paragraph in twips, or `-1` when unset.
    #[wasm_bindgen(getter, js_name = afterTwip)]
    #[must_use]
    pub fn after_twip(&self) -> i32 {
        self.after_twip
    }

    /// Line-spacing percentage for the `auto` rule (0 when unset or a fixed rule).
    #[wasm_bindgen(getter, js_name = linePercent)]
    #[must_use]
    pub fn line_percent(&self) -> u32 {
        self.line_percent
    }

    /// Line rule: `0` auto (percent), `1` atLeast, `2` exact (twips).
    #[wasm_bindgen(getter, js_name = lineRule)]
    #[must_use]
    pub fn line_rule(&self) -> u8 {
        self.line_rule
    }

    /// Fixed line height in twips for the atLeast/exact rules (0 for auto).
    #[wasm_bindgen(getter, js_name = lineTwip)]
    #[must_use]
    pub fn line_twip(&self) -> i32 {
        self.line_twip
    }
}

/// Uniform paragraph properties over every paragraph touched by a selection.
///
/// Values carry explicit mixed flags so the host never has to infer
/// disagreement from a sentinel. Boolean states are 0=off, 1=on, 2=mixed.
#[wasm_bindgen]
#[derive(Clone, Debug, Default)]
pub struct ParagraphState {
    count: u32,
    alignment: String,
    alignment_mixed: bool,
    style: String,
    style_mixed: bool,
    start_twip: i32,
    start_mixed: bool,
    end_twip: i32,
    end_mixed: bool,
    first_line_twip: i32,
    first_line_mixed: bool,
    hanging_twip: i32,
    hanging_mixed: bool,
    before_twip: i32,
    before_mixed: bool,
    after_twip: i32,
    after_mixed: bool,
    line_percent: u32,
    line_mixed: bool,
    keep_next: u8,
    keep_lines: u8,
    page_break_before: u8,
    shading: i32,
    shading_mixed: bool,
    border_edges: u8,
    borders_mixed: bool,
}

#[wasm_bindgen]
impl ParagraphState {
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn count(&self) -> u32 {
        self.count
    }

    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn alignment(&self) -> String {
        self.alignment.clone()
    }

    #[wasm_bindgen(getter, js_name = alignmentMixed)]
    #[must_use]
    pub fn alignment_mixed(&self) -> bool {
        self.alignment_mixed
    }

    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn style(&self) -> String {
        self.style.clone()
    }

    #[wasm_bindgen(getter, js_name = styleMixed)]
    #[must_use]
    pub fn style_mixed(&self) -> bool {
        self.style_mixed
    }

    #[wasm_bindgen(getter, js_name = startTwip)]
    #[must_use]
    pub fn start_twip(&self) -> i32 {
        self.start_twip
    }

    #[wasm_bindgen(getter, js_name = startMixed)]
    #[must_use]
    pub fn start_mixed(&self) -> bool {
        self.start_mixed
    }

    #[wasm_bindgen(getter, js_name = endTwip)]
    #[must_use]
    pub fn end_twip(&self) -> i32 {
        self.end_twip
    }

    #[wasm_bindgen(getter, js_name = endMixed)]
    #[must_use]
    pub fn end_mixed(&self) -> bool {
        self.end_mixed
    }

    #[wasm_bindgen(getter, js_name = firstLineTwip)]
    #[must_use]
    pub fn first_line_twip(&self) -> i32 {
        self.first_line_twip
    }

    #[wasm_bindgen(getter, js_name = firstLineMixed)]
    #[must_use]
    pub fn first_line_mixed(&self) -> bool {
        self.first_line_mixed
    }

    #[wasm_bindgen(getter, js_name = hangingTwip)]
    #[must_use]
    pub fn hanging_twip(&self) -> i32 {
        self.hanging_twip
    }

    #[wasm_bindgen(getter, js_name = hangingMixed)]
    #[must_use]
    pub fn hanging_mixed(&self) -> bool {
        self.hanging_mixed
    }

    #[wasm_bindgen(getter, js_name = beforeTwip)]
    #[must_use]
    pub fn before_twip(&self) -> i32 {
        self.before_twip
    }

    #[wasm_bindgen(getter, js_name = beforeMixed)]
    #[must_use]
    pub fn before_mixed(&self) -> bool {
        self.before_mixed
    }

    #[wasm_bindgen(getter, js_name = afterTwip)]
    #[must_use]
    pub fn after_twip(&self) -> i32 {
        self.after_twip
    }

    #[wasm_bindgen(getter, js_name = afterMixed)]
    #[must_use]
    pub fn after_mixed(&self) -> bool {
        self.after_mixed
    }

    #[wasm_bindgen(getter, js_name = linePercent)]
    #[must_use]
    pub fn line_percent(&self) -> u32 {
        self.line_percent
    }

    #[wasm_bindgen(getter, js_name = lineMixed)]
    #[must_use]
    pub fn line_mixed(&self) -> bool {
        self.line_mixed
    }

    #[wasm_bindgen(getter, js_name = keepNextState)]
    #[must_use]
    pub fn keep_next_state(&self) -> u8 {
        self.keep_next
    }

    #[wasm_bindgen(getter, js_name = keepLinesState)]
    #[must_use]
    pub fn keep_lines_state(&self) -> u8 {
        self.keep_lines
    }

    #[wasm_bindgen(getter, js_name = pageBreakBeforeState)]
    #[must_use]
    pub fn page_break_before_state(&self) -> u8 {
        self.page_break_before
    }

    /// Packed `0xRRGGBB`, or `-1` when uniformly unset.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn shading(&self) -> i32 {
        self.shading
    }

    #[wasm_bindgen(getter, js_name = shadingMixed)]
    #[must_use]
    pub fn shading_mixed(&self) -> bool {
        self.shading_mixed
    }

    /// top=1, bottom=2, start=4, end=8.
    #[wasm_bindgen(getter, js_name = borderEdges)]
    #[must_use]
    pub fn border_edges(&self) -> u8 {
        self.border_edges
    }

    #[wasm_bindgen(getter, js_name = bordersMixed)]
    #[must_use]
    pub fn borders_mixed(&self) -> bool {
        self.borders_mixed
    }
}

/// A paragraph's line-and-page-break flags, for the paragraph-properties inspector.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Default)]
pub struct ParagraphFlags {
    keep_next: bool,
    keep_lines: bool,
    page_break_before: bool,
}

#[wasm_bindgen]
impl ParagraphFlags {
    /// Keep this paragraph on the same page as the next (`w:keepNext`).
    #[wasm_bindgen(getter, js_name = keepNext)]
    #[must_use]
    pub fn keep_next(&self) -> bool {
        self.keep_next
    }

    /// Keep all lines of this paragraph on one page (`w:keepLines`).
    #[wasm_bindgen(getter, js_name = keepLines)]
    #[must_use]
    pub fn keep_lines(&self) -> bool {
        self.keep_lines
    }

    /// Force a page break before this paragraph (`w:pageBreakBefore`).
    #[wasm_bindgen(getter, js_name = pageBreakBefore)]
    #[must_use]
    pub fn page_break_before(&self) -> bool {
        self.page_break_before
    }
}

/// Document statistics for the status footer.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Default)]
pub struct DocStats {
    words: u32,
    paragraphs: u32,
    pages: u32,
}

#[wasm_bindgen]
impl DocStats {
    /// Whitespace-delimited word count across every paragraph.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn words(&self) -> u32 {
        self.words
    }

    /// Paragraph count (body + table cells + content controls).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn paragraphs(&self) -> u32 {
        self.paragraphs
    }

    /// Page count.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn pages(&self) -> u32 {
        self.pages
    }
}

/// The uniform run styling of a selection, for reflecting current values in the
/// toolbar. Values and explicit mixed flags are separate so a uniformly unset
/// property is never conflated with disagreement across the selection.
#[wasm_bindgen]
#[derive(Clone, Debug, Default)]
pub struct RunStyle {
    size_points: f32,
    size_mixed: bool,
    color: String,
    color_mixed: bool,
    font: String,
    font_mixed: bool,
    highlight: String,
    highlight_mixed: bool,
    superscript: bool,
    subscript: bool,
    vertical_align: String,
    vertical_align_mixed: bool,
}

#[wasm_bindgen]
impl RunStyle {
    /// Common font size in points (0 if mixed/unset).
    #[wasm_bindgen(getter, js_name = sizePoints)]
    #[must_use]
    pub fn size_points(&self) -> f32 {
        self.size_points
    }

    /// Whether the selection contains more than one effective font size.
    #[wasm_bindgen(getter, js_name = sizeMixed)]
    #[must_use]
    pub fn size_mixed(&self) -> bool {
        self.size_mixed
    }

    /// Common text color as `#rrggbb` (empty if mixed/unset or a theme color).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn color(&self) -> String {
        self.color.clone()
    }

    /// Whether the selection contains more than one effective text color.
    #[wasm_bindgen(getter, js_name = colorMixed)]
    #[must_use]
    pub fn color_mixed(&self) -> bool {
        self.color_mixed
    }

    /// Common font family (empty if mixed/unset).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn font(&self) -> String {
        self.font.clone()
    }

    /// Whether the selection contains more than one effective font family.
    #[wasm_bindgen(getter, js_name = fontMixed)]
    #[must_use]
    pub fn font_mixed(&self) -> bool {
        self.font_mixed
    }

    /// Common highlight name, `"none"` when uniformly clear, or empty when mixed.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn highlight(&self) -> String {
        self.highlight.clone()
    }

    /// Whether the selection contains more than one effective highlight value.
    #[wasm_bindgen(getter, js_name = highlightMixed)]
    #[must_use]
    pub fn highlight_mixed(&self) -> bool {
        self.highlight_mixed
    }

    /// Every covered run is superscript.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn superscript(&self) -> bool {
        self.superscript
    }

    /// Every covered run is subscript.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn subscript(&self) -> bool {
        self.subscript
    }

    /// Common vertical alignment: `super`, `sub`, `baseline`, or empty when mixed.
    #[wasm_bindgen(getter, js_name = verticalAlign)]
    #[must_use]
    pub fn vertical_align(&self) -> String {
        self.vertical_align.clone()
    }

    /// Whether the selection mixes baseline, superscript, or subscript.
    #[wasm_bindgen(getter, js_name = verticalAlignMixed)]
    #[must_use]
    pub fn vertical_align_mixed(&self) -> bool {
        self.vertical_align_mixed
    }
}

/// A find result: either no match, or a same-node byte range that can become the
/// editor selection and feed replaceSelection.
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct TextMatch {
    found: bool,
    start_node: String,
    start_offset: u32,
    end_node: String,
    end_offset: u32,
}

impl TextMatch {
    fn none() -> Self {
        Self {
            found: false,
            start_node: String::new(),
            start_offset: 0,
            end_node: String::new(),
            end_offset: 0,
        }
    }

    fn new(node: NodeId, start: usize, end: usize) -> Self {
        let node = node.to_string();
        Self {
            found: true,
            start_node: node.clone(),
            start_offset: start as u32,
            end_node: node,
            end_offset: end as u32,
        }
    }
}

#[wasm_bindgen]
impl TextMatch {
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn found(&self) -> bool {
        self.found
    }

    #[wasm_bindgen(getter, js_name = startNode)]
    #[must_use]
    pub fn start_node(&self) -> String {
        self.start_node.clone()
    }

    #[wasm_bindgen(getter, js_name = startOffset)]
    #[must_use]
    pub fn start_offset(&self) -> u32 {
        self.start_offset
    }

    #[wasm_bindgen(getter, js_name = endNode)]
    #[must_use]
    pub fn end_node(&self) -> String {
        self.end_node.clone()
    }

    #[wasm_bindgen(getter, js_name = endOffset)]
    #[must_use]
    pub fn end_offset(&self) -> u32 {
        self.end_offset
    }
}

/// A caret position returned by navigation (no edit): node id + byte offset.
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct Caret {
    node: String,
    offset: u32,
}

#[wasm_bindgen]
impl Caret {
    /// The caret's paragraph node id (32-hex string).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn node(&self) -> String {
        self.node.clone()
    }

    /// The caret's byte offset within the node.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn offset(&self) -> u32 {
        self.offset
    }
}

/// The byte range `[start, end)` of the Unicode word containing (or ending at)
/// `offset`, or `None` if the offset is not within a word (e.g. whitespace).
fn word_bounds(text: &str, offset: usize) -> Option<(usize, usize)> {
    use unicode_segmentation::UnicodeSegmentation;
    for (start, word) in text.unicode_word_indices() {
        let end = start + word.len();
        if (start..=end).contains(&offset) {
            return Some((start, end));
        }
    }
    None
}

/// The end of the next Unicode word at or after `offset` (⌥→), or `None` past the
/// last word.
fn next_word_boundary(text: &str, offset: usize) -> Option<usize> {
    use unicode_segmentation::UnicodeSegmentation;
    text.unicode_word_indices()
        .map(|(start, word)| start + word.len())
        .find(|&end| end > offset)
}

/// The start of the previous Unicode word before `offset` (⌥←), or `None` before
/// the first word.
fn prev_word_boundary(text: &str, offset: usize) -> Option<usize> {
    use unicode_segmentation::UnicodeSegmentation;
    text.unicode_word_indices()
        .map(|(start, _)| start)
        .rfind(|&start| start < offset)
}

/// The indices of pages that differ between two layouts (changed, added, or
/// removed) — the set the frontend must repaint.
fn dirty_pages(old: &PaginatedLayout, new: &PaginatedLayout) -> Vec<u32> {
    let max = old.pages.len().max(new.pages.len());
    (0..max)
        .filter(|&i| old.pages.get(i) != new.pages.get(i))
        .map(|i| i as u32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::v1::TabLeader;

    const RICH_DOCX: &[u8] = include_bytes!("../../../fixtures/corpus/real-producer-rich.docx");
    const SAMPLE_DOCX: &[u8] = include_bytes!("../../../sample.docx");

    #[test]
    fn document_metadata_exposes_core_app_and_custom_groups() {
        let doc = open_document(SAMPLE_DOCX).expect("open sample docx");
        let metadata: serde_json::Value =
            serde_json::from_str(&doc.document_metadata()).expect("metadata JSON");

        assert_eq!(metadata["core"]["creator"], "CasualOffice");
        assert_eq!(metadata["core"]["created"], "2013-12-23T23:15:00Z");
        assert_eq!(metadata["core"]["modified"], "2013-12-23T23:15:00Z");
        assert_eq!(metadata["app"]["application"], "Microsoft Macintosh Word");
        assert_eq!(metadata["app"]["appVersion"], "14.0000");
        assert!(metadata["custom"].is_array());
    }

    /// The bridge paginates a corpus document to the **same** page count as the
    /// native pipeline — the façade adds no layout of its own (doc 57 §10,
    /// P1G-001 acceptance: "open → page_count matches native").
    #[test]
    fn page_count_matches_native() {
        let doc = open_document(RICH_DOCX).expect("open corpus docx");

        // Native reference: import + paginate directly, no bridge.
        let mut package = DocxPackage::open(RICH_DOCX, viewer_limits()).unwrap();
        let imported = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap();
        let shaper = ParleyShaper::new();
        let native = paginate_document(&imported.document, &shaper);

        assert!(
            native.page_count() > 0,
            "fixture should paginate to >=1 page"
        );
        assert_eq!(doc.page_count() as usize, native.page_count());
    }

    /// Every page reports a positive per-section page box, and rendering that page
    /// yields a device bitmap whose dimensions and RGBA length agree.
    #[test]
    fn render_page_dimensions_are_consistent() {
        let doc = open_document(RICH_DOCX).expect("open corpus docx");
        let dpi = 96.0;

        for i in 0..doc.page_count() {
            let size = doc.page_size(i).expect("page size");
            assert!(size.width_twip() > 0 && size.height_twip() > 0);

            let bitmap = doc.render_page(i, dpi).expect("render page");
            assert!(bitmap.width_px() > 0 && bitmap.height_px() > 0);
            // RGBA8888: four bytes per pixel, row-major.
            let expected = bitmap.width_px() as usize * bitmap.height_px() as usize * 4;
            assert_eq!(bitmap.rgba().0.len(), expected);
        }
    }

    /// A Latin corpus document has no missing coverage (the bundled faces cover
    /// it), and registering a fallback font runs the register → repaginate path
    /// without panicking and without disturbing a covered document's pagination.
    #[test]
    fn font_registration_repaginates_without_panic() {
        let mut doc = open_document(RICH_DOCX).expect("open corpus docx");
        assert!(
            doc.missing_coverage().is_empty(),
            "bundled faces cover this Latin document"
        );
        let before = doc.page_count();

        // A real face (bundled Roboto) wired as a Han fallback: exercises register
        // + repaginate. It adds no Han glyphs, so a Latin doc's pages are stable.
        doc.register_fallback_font(
            casual_doc_layout::fonts::ROBOTO_REGULAR,
            vec!["Hani".to_string()],
        );
        assert_eq!(doc.page_count(), before);
        assert!(doc.missing_coverage().is_empty());
    }

    #[test]
    fn named_font_batch_is_bounded_and_repaginates_once_after_admission() {
        let mut doc = open_document(RICH_DOCX).expect("open corpus docx");
        let faces = [
            casual_doc_layout::fonts::ROBOTO_REGULAR,
            casual_doc_layout::fonts::ROBOTO_ITALIC,
        ];
        let lengths: Vec<u32> = faces.iter().map(|face| face.len() as u32).collect();
        let bytes: Vec<u8> = faces.into_iter().flatten().copied().collect();
        let before = doc.page_count();

        doc.register_fonts_inner(&bytes, &lengths)
            .expect("valid host font batch");
        assert_eq!(doc.page_count(), before);
        assert!(
            doc.register_fonts_inner(&bytes, &[bytes.len() as u32 - 1])
                .is_err(),
            "declared lengths must cover the payload exactly"
        );
        assert!(
            doc.register_fonts_inner(&[0], &[0]).is_err(),
            "empty faces are rejected before registration"
        );
    }

    /// An out-of-range page index is a clean error, not a panic. Exercises the
    /// inner methods: the `#[wasm_bindgen]` wrappers would construct a `JsValue`
    /// on the error path, which panics off-wasm.
    #[test]
    fn out_of_range_page_is_an_error() {
        let doc = open_document(RICH_DOCX).expect("open corpus docx");
        assert!(doc.page_size_inner(doc.page_count()).is_err());
        assert!(doc.render_page_inner(doc.page_count(), 96.0).is_err());
    }

    /// hit-test → caret/selection geometry → copy round-trips over a real
    /// paragraph: copying a node's full range reproduces its plain text, the
    /// geometry queries return well-formed rects, and a point inside the caret's
    /// line resolves back to the same node (the doc 58 pipeline, read-only end).
    #[test]
    fn selection_and_copy_roundtrip() {
        let doc = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(doc.document.body(), &mut nodes);
        let (node_id, text) = nodes
            .iter()
            .find(|(_, t)| !t.is_empty())
            .expect("a non-empty paragraph");
        let node = node_id.to_string();
        let end = text.len() as u32;

        // Copying the whole node reproduces its shaped plain text exactly.
        assert_eq!(doc.copy_text(&node, 0, &node, end), *text);
        // A backward range copies the same text (endpoints are ordered).
        assert_eq!(doc.copy_text(&node, end, &node, 0), *text);

        // Caret geometry: [page, x, y, w=0, h>0].
        let caret = doc.caret_rect(&node, 0);
        assert_eq!(caret.len(), 5, "one flat rect");
        assert_eq!(caret[3], 0, "caret is zero-width");
        assert!(caret[4] > 0, "caret has line height");

        // Selection geometry: at least one 5-tuple rect.
        let rects = doc.selection_rects(&node, 0, &node, end);
        assert!(!rects.is_empty() && rects.len().is_multiple_of(5));

        // A point inside the caret's line resolves back to the same node.
        let (page, x, y) = (caret[0] as u32, caret[1], caret[2]);
        let hit = doc
            .hit_test(page, x + 10, y + 10)
            .expect("a hit on content");
        assert_eq!(hit.node(), node);
        assert_eq!(hit.zone(), "content");

        // A malformed node id is an empty result, not a panic.
        assert!(doc.caret_rect("not-a-node", 0).is_empty());
        assert!(doc.copy_text("bad", 0, "bad", 1).is_empty());
    }

    #[test]
    fn find_text_moves_through_document_order_and_wraps() {
        let doc = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(doc.document.body(), &mut nodes);
        let first_node = nodes.first().expect("document text").0.to_string();
        let nested_a = nodes
            .iter()
            .find(|(_, t)| t == "Nested A")
            .map(|(id, _)| id.to_string())
            .expect("nested A cell");
        let nested_b = nodes
            .iter()
            .find(|(_, t)| t == "Nested B")
            .map(|(id, _)| id.to_string())
            .expect("nested B cell");

        let first = doc.find_text("nested", &first_node, 0, true, false);
        assert!(first.found());
        assert_eq!(first.start_node(), nested_a);
        assert_eq!(first.start_offset(), 0);
        assert_eq!(first.end_offset(), "Nested".len() as u32);

        let next = doc.find_text("Nested", &first.end_node(), first.end_offset(), true, true);
        assert!(next.found());
        assert_eq!(next.start_node(), nested_b);

        let previous = doc.find_text(
            "Nested",
            &next.start_node(),
            next.start_offset(),
            false,
            true,
        );
        assert!(previous.found());
        assert_eq!(previous.start_node(), nested_a);

        let wrapped = doc.find_text("Nested", &next.end_node(), next.end_offset(), true, true);
        assert!(wrapped.found());
        assert_eq!(wrapped.start_node(), nested_a);

        assert!(
            doc.find_text("nESTED A", &first_node, 0, true, false)
                .found()
        );
        assert!(
            !doc.find_text("nESTED A", &first_node, 0, true, true)
                .found()
        );
        assert!(
            !doc.find_text("definitely absent", &first_node, 0, true, false)
                .found()
        );
    }

    #[test]
    fn hyperlink_authoring_hit_activation_undo_and_roundtrip() {
        fn editable_run(blocks: &[BlockNode]) -> Option<(NodeId, u32, u32)> {
            for block in blocks {
                match block {
                    BlockNode::Paragraph(paragraph) => {
                        let mut offset = 0;
                        for inline in &paragraph.inlines {
                            let len = node_plain_text(core::slice::from_ref(inline)).len() as u32;
                            if matches!(inline, InlineNode::Run(_)) && len > 0 {
                                return Some((paragraph.id, offset, offset + len));
                            }
                            offset += len;
                        }
                    }
                    BlockNode::Table(table) => {
                        for row in &table.rows {
                            for cell in &row.cells {
                                if let Some(found) = editable_run(&cell.blocks) {
                                    return Some(found);
                                }
                            }
                        }
                    }
                    BlockNode::Sdt(sdt) => {
                        if let Some(found) = editable_run(&sdt.blocks) {
                            return Some(found);
                        }
                    }
                    BlockNode::AltChunk(_) => {}
                }
            }
            None
        }

        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let (node_id, start, end) =
            editable_run(d.document.body()).expect("an editable top-level run");
        let node = node_id.to_string();
        d.set_hyperlink(
            &node,
            start,
            end,
            "https://example.com/document".to_owned(),
            Some("Example".to_owned()),
        )
        .expect("create link");
        d.document.validate().expect("link edit remains valid");

        let rects = d.selection_rects(&node, start, &node, end);
        assert!(rects.len() >= 5);
        let (page, x, y, width, height) = (rects[0] as u32, rects[1], rects[2], rects[3], rects[4]);
        let link = d
            .link_at(page, x + width / 2, y + height / 2)
            .expect("painted linked text is directly clickable");
        assert_eq!(link.kind(), "external");
        assert_eq!(link.url(), "https://example.com/document");
        assert_eq!(link.tooltip(), "Example");
        assert_eq!(link.start_node(), node);
        assert_eq!(link.start_offset(), start);
        assert_eq!(link.end_offset(), end);

        let bytes = d.export_docx().expect("export linked document");
        let reopened = open_document(&bytes).expect("re-open linked document");
        let paragraph = find_paragraph(reopened.document.body(), node_id).expect("same paragraph");
        let links = paragraph_links(&reopened.document, paragraph);
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].link.target,
            HyperlinkTarget::External(ExternalTarget {
                url: "https://example.com/document".to_owned(),
            })
        );

        d.undo().expect("undo link");
        assert!(
            paragraph_links(
                &d.document,
                find_paragraph(d.document.body(), node_id).unwrap()
            )
            .is_empty()
        );
        d.redo().expect("redo link");
        d.remove_hyperlink(&node, start, end).expect("remove link");
        assert!(
            paragraph_links(
                &d.document,
                find_paragraph(d.document.body(), node_id).unwrap()
            )
            .is_empty()
        );
        d.undo().expect("undo remove");
        assert_eq!(
            paragraph_links(
                &d.document,
                find_paragraph(d.document.body(), node_id).unwrap()
            )
            .len(),
            1
        );
    }

    #[test]
    fn internal_link_resolves_named_bookmark_marker() {
        use casual_doc_model::v1::{
            Bookmark, BookmarkStart, Definitions, Hyperlink, Run, RunProperties,
        };

        let bookmark_id = BookmarkId::new(NodeId::from_parts(70, 1).unwrap());
        let source_id = NodeId::from_parts(70, 2).unwrap();
        let target_id = NodeId::from_parts(70, 3).unwrap();
        let mut definitions = Definitions::default();
        definitions.bookmarks.insert(
            bookmark_id,
            Bookmark {
                name: "Heading_1".to_owned(),
            },
        );
        let document = Document::new(
            NodeId::from_parts(70, 10).unwrap(),
            vec![
                BlockNode::Paragraph(Paragraph {
                    id: source_id,
                    properties: ParagraphProperties::default(),
                    inlines: vec![InlineNode::Hyperlink(Hyperlink {
                        id: NodeId::from_parts(70, 4).unwrap(),
                        target: HyperlinkTarget::Internal(InternalTarget {
                            anchor: "Heading_1".to_owned(),
                        }),
                        tooltip: None,
                        inlines: vec![InlineNode::Run(Run {
                            id: NodeId::from_parts(70, 5).unwrap(),
                            properties: Default::default(),
                            text: "Go".to_owned(),
                        })],
                    })],
                }),
                BlockNode::Paragraph(Paragraph {
                    id: target_id,
                    properties: ParagraphProperties::default(),
                    inlines: vec![
                        InlineNode::Run(Run {
                            id: NodeId::from_parts(70, 6).unwrap(),
                            properties: RunProperties::default(),
                            text: "Before ".to_owned(),
                        }),
                        InlineNode::BookmarkStart(BookmarkStart {
                            id: NodeId::from_parts(70, 7).unwrap(),
                            bookmark: bookmark_id,
                        }),
                        InlineNode::Run(Run {
                            id: NodeId::from_parts(70, 8).unwrap(),
                            properties: RunProperties {
                                bold: Some(true),
                                ..RunProperties::default()
                            },
                            text: "Heading".to_owned(),
                        }),
                    ],
                }),
            ],
            definitions,
        )
        .expect("valid bookmarked document");

        assert_eq!(
            resolve_bookmark(&document, "Heading_1"),
            Some(ModelPos::new(target_id, 7))
        );
        assert_eq!(resolve_bookmark(&document, "missing"), None);
    }

    /// Type a character, confirm the model text changed, then undo and confirm it
    /// is restored — the mutating pipeline end-to-end through the WASM methods.
    #[test]
    fn insert_then_undo_round_trips() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let (node_id, original) = nodes
            .iter()
            .find(|(_, t)| !t.is_empty())
            .map(|(id, t)| (*id, t.clone()))
            .expect("a non-empty paragraph");
        let node = node_id.to_string();
        let end = original.len() as u32;

        let result = d.insert_text(&node, 0, "Z".to_string()).expect("insert");
        assert_eq!(result.offset(), 1, "caret advances past the inserted char");
        assert_eq!(result.revision(), 1);
        assert_eq!(
            d.copy_text(&node, 0, &node, end + 1),
            format!("Z{original}")
        );

        d.undo().expect("undo");
        assert_eq!(
            d.copy_text(&node, 0, &node, end),
            original,
            "undo restores text"
        );

        d.redo().expect("redo");
        assert_eq!(
            d.copy_text(&node, 0, &node, end + 1),
            format!("Z{original}")
        );
    }

    /// Rapid adjacent typing is one user action, while a different host gesture
    /// id starts a new history entry. History availability is exposed as engine
    /// truth for the shell's Undo/Redo buttons.
    #[test]
    fn typing_sessions_coalesce_only_adjacent_history() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let (node_id, original) = nodes
            .iter()
            .find(|(_, text)| !text.is_empty())
            .map(|(id, text)| (*id, text.clone()))
            .expect("a non-empty paragraph");
        let node = node_id.to_string();

        assert!(!d.can_undo());
        assert!(!d.can_redo());
        d.type_text(&node, 0, &node, 0, "A".to_owned(), 41)
            .expect("first typing tick");
        d.type_text(&node, 1, &node, 1, "B".to_owned(), 41)
            .expect("adjacent typing tick");
        assert_eq!(d.undo.len(), 1, "one host typing session is one action");
        assert_eq!(d.undo_label(), "Typing");
        assert!(d.can_undo());
        assert!(!d.can_redo());

        d.undo().expect("undo typing session");
        assert_eq!(
            d.copy_text(&node, 0, &node, original.len() as u32),
            original
        );
        assert!(!d.can_undo());
        assert!(d.can_redo());
        assert_eq!(d.redo_label(), "Typing");

        d.redo().expect("redo typing session");
        assert_eq!(
            d.copy_text(&node, 0, &node, original.len() as u32 + 2),
            format!("AB{original}")
        );
        assert!(d.can_undo());
        assert!(!d.can_redo());
        assert_eq!(d.undo_label(), "Typing");

        d.type_text(&node, 2, &node, 2, "C".to_owned(), 42)
            .expect("new typing gesture");
        assert_eq!(
            d.undo.len(),
            2,
            "a different host session starts a new action"
        );
        d.undo().expect("undo only the new gesture");
        assert_eq!(
            d.copy_text(&node, 0, &node, original.len() as u32 + 2),
            format!("AB{original}")
        );

        d.set_font_size(&node, 0, &node, 1, 12.5)
            .expect("format one character");
        assert_eq!(d.undo_label(), "Formatting");
        d.insert_plain_text_as(&node, 0, &node, 0, "P".to_owned(), "paste")
            .expect("paste through the closed action-kind API");
        assert_eq!(d.undo_label(), "Paste");
        d.undo().expect("undo paste");
        assert_eq!(d.redo_label(), "Paste");
    }

    #[test]
    fn styled_typing_session_undoes_insert_and_format_together() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let (node_id, original) = nodes
            .iter()
            .find(|(_, text)| !text.is_empty())
            .map(|(id, text)| (*id, text.clone()))
            .expect("a non-empty paragraph");
        let node = node_id.to_string();

        for (offset, text) in [(0, "A"), (1, "B")] {
            d.type_styled_text(
                &node,
                offset,
                text.to_owned(),
                Some(true),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                77,
            )
            .expect("styled typing tick");
        }
        assert_eq!(d.undo.len(), 1);
        assert!(d.format_at(&node, 0, 2).bold());
        d.undo().expect("undo styled typing session");
        assert_eq!(
            d.copy_text(&node, 0, &node, original.len() as u32),
            original
        );
    }

    /// Plain multiline insertion performs selection deletion, paragraph splits,
    /// and every inserted line in one atomic history group.
    #[test]
    fn plain_text_replacement_is_one_atomic_action() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let (node_id, original) = nodes
            .iter()
            .find(|(_, text)| text.len() >= 2)
            .map(|(id, text)| (*id, text.clone()))
            .expect("a paragraph with two bytes");
        let node = node_id.to_string();

        let result = d
            .insert_plain_text(&node, 0, &node, 2, "A\r\nB\nC".to_owned())
            .expect("replace selection with multiline text");
        assert_eq!(d.undo.len(), 1, "the complete replacement is one action");
        assert_eq!(
            d.copy_text(&node, 0, &result.node(), result.offset()),
            "A\nB\nC",
            "line endings become structural paragraph breaks"
        );

        d.undo().expect("undo multiline replacement");
        assert_eq!(
            d.copy_text(&node, 0, &node, original.len() as u32),
            original,
            "one undo restores both deleted text and paragraph structure"
        );
        assert!(!d.can_undo());
        assert!(d.can_redo());
    }

    /// The engine, not JS string comparison, resolves selection order across
    /// paragraphs for plain-arrow range collapse.
    #[test]
    fn selection_edge_orders_cross_paragraph_anchors() {
        let d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let (first, first_text) = nodes.first().expect("fixture needs a first paragraph");
        let (second, second_text) = nodes.get(1).expect("fixture needs a second paragraph");

        let start = d
            .selection_edge(
                &second.to_string(),
                second_text.len() as u32,
                &first.to_string(),
                0,
                false,
            )
            .expect("ordered start");
        assert_eq!(start.node(), first.to_string());
        assert_eq!(start.offset(), 0);

        let end = d
            .selection_edge(
                &second.to_string(),
                second_text.len() as u32,
                &first.to_string(),
                0,
                true,
            )
            .expect("ordered end");
        assert_eq!(end.node(), second.to_string());
        assert_eq!(end.offset(), second_text.len() as u32);
        assert!(!first_text.is_empty(), "fixture sanity");
    }

    /// An edit survives export → re-open: type text, export to `.docx`, re-open
    /// the exported bytes, and confirm the inserted text is present.
    #[test]
    fn edit_survives_export_and_reopen() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(_, t)| !t.is_empty())
            .map(|(id, _)| id.to_string())
            .expect("a non-empty paragraph");

        d.insert_text(&node, 0, "ZZTOP".to_string())
            .expect("insert");
        let bytes = d.export_docx().expect("export");
        assert!(!bytes.is_empty(), "exported a non-empty package");

        let reopened = open_document(&bytes).expect("re-open exported docx");
        let mut nodes2 = Vec::new();
        collect_block_text(reopened.document.body(), &mut nodes2);
        assert!(
            nodes2.iter().any(|(_, t)| t.starts_with("ZZTOP")),
            "the inserted text survived the export/re-open round trip"
        );
    }

    /// Bold a range, confirm the format query reflects it, and undo restores it.
    #[test]
    fn format_bold_and_query_and_undo() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(id, text)| text.len() >= 3 && !d.format_at(&id.to_string(), 0, 3).bold())
            .map(|(id, _)| id.to_string())
            .expect("a non-bold paragraph with >=3 chars");

        assert!(!d.format_at(&node, 0, 3).bold(), "not bold initially");
        d.format_text(&node, 0, 3, Some(true), None, None, None)
            .expect("bold");
        assert!(d.format_at(&node, 0, 3).bold(), "now bold over [0,3)");

        d.undo().expect("undo");
        assert!(!d.format_at(&node, 0, 3).bold(), "undo cleared bold");
    }

    #[test]
    fn toolbar_style_resolves_effective_paragraph_style_font_and_format() {
        use casual_doc_model::v1::{Definitions, RunProperties, Style};

        let style_id = StyleId::new(NodeId::from_parts(82, 1).unwrap());
        let paragraph_id = NodeId::from_parts(82, 2).unwrap();
        let mut definitions = Definitions::default();
        definitions.styles.insert(
            style_id,
            Style {
                kind: StyleKind::Paragraph,
                is_default: false,
                name: Some("Imported heading".to_owned()),
                aliases: None,
                based_on: None,
                next: None,
                link: None,
                hidden: false,
                ui_priority: None,
                semi_hidden: false,
                unhide_when_used: false,
                q_format: true,
                locked: false,
                paragraph: None,
                run: Some(RunProperties {
                    bold: Some(true),
                    size_half_points: Some(36),
                    font_ref: Some(FontRef::Named(FontName {
                        name: "Imported Display Face".to_owned(),
                    })),
                    ..RunProperties::default()
                }),
                table: None,
                table_row: None,
                table_cell: None,
                conditional: Vec::new(),
            },
        );
        let document = Document::new(
            NodeId::from_parts(82, 3).unwrap(),
            vec![BlockNode::Paragraph(Paragraph {
                id: paragraph_id,
                properties: ParagraphProperties {
                    style_ref: Some(style_id),
                    ..ParagraphProperties::default()
                },
                inlines: vec![InlineNode::Run(Run {
                    id: NodeId::from_parts(82, 4).unwrap(),
                    properties: RunProperties::default(),
                    text: "Styled".to_owned(),
                })],
            })],
            definitions,
        )
        .expect("valid style-driven document");

        let effective = effective_caret_run_properties(&document, paragraph_id, 3)
            .expect("caret inherits its paragraph style");
        let toolbar = run_style_from_effective_runs(&document, std::slice::from_ref(&effective));
        let format = format_from_effective_runs(std::slice::from_ref(&effective));
        assert_eq!(toolbar.font(), "Imported Display Face");
        assert_eq!(toolbar.size_points(), 18.0);
        assert!(format.bold(), "style-driven bold is reflected");

        let implicit = run_style_from_effective_runs(&document, &[RunProperties::default()]);
        assert_eq!(
            implicit.font(),
            "Roboto",
            "an undeclared family reflects the engine default"
        );
    }

    /// Typing with an armed format at a collapsed caret: `insertStyledText` inserts
    /// the character bold and `caretFormat` reflects what the caret carries; the
    /// whole thing undoes as one action.
    #[test]
    fn insert_styled_text_types_bold_and_undoes() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(id, text)| !text.is_empty() && !d.caret_format(&id.to_string(), 0).bold())
            .map(|(id, _)| id.to_string())
            .expect("a non-empty non-bold paragraph");

        assert!(!d.caret_format(&node, 0).bold(), "caret not bold initially");
        // Armed bold + 20pt: both land on the typed character.
        let res = d
            .insert_styled_text(
                &node,
                0,
                "B".to_string(),
                Some(true),
                None,
                None,
                None,
                Some(40),
                None,
                None,
                None,
                None,
            )
            .expect("styled insert");
        assert_eq!(res.offset(), 1, "caret rests after the inserted char");
        assert!(d.format_at(&node, 0, 1).bold(), "the typed char is bold");
        assert_eq!(
            d.selection_run_style(&node, 0, &node, 1).size_points(),
            20.0,
            "the typed char carries the armed 20pt size"
        );

        d.undo().expect("undo");
        assert!(
            !d.format_at(&node, 0, 1).bold(),
            "one undo reverts both the insert and its formatting"
        );
    }

    /// A delete whose range spans several runs (a formatted paragraph) succeeds —
    /// the bug that made a multi-paragraph selection Backspace a no-op ("edit
    /// ignored: Unsupported"). Formatting part of the paragraph splits it into runs;
    /// a range crossing that boundary must still delete, and undo must restore.
    #[test]
    fn delete_across_runs_succeeds_and_undoes() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let (node_id, text) = nodes
            .iter()
            .find(|(_, t)| t.is_ascii() && t.len() >= 6)
            .map(|(id, t)| (*id, t.clone()))
            .expect("an ASCII paragraph with >=6 chars");
        let node = node_id.to_string();
        let len = text.len() as u32;

        // Bold [0,3): the paragraph is now at least two runs. [2,5) spans the
        // bold/normal run boundary — the case that used to report Unsupported.
        d.format_text(&node, 0, 3, Some(true), None, None, None)
            .expect("bold");
        d.delete_range(&node, 2, 5)
            .expect("multi-run delete must succeed");
        let after = d.copy_text(&node, 0, &node, len - 3);
        assert_eq!(after, format!("{}{}", &text[..2], &text[5..]));

        // Undo the delete, then the format; the original text returns intact.
        d.undo().expect("undo delete");
        d.undo().expect("undo bold");
        assert_eq!(d.copy_text(&node, 0, &node, len), text);
    }

    /// Deleting a selection spanning several consecutive *plain* paragraphs
    /// (no formatting, no tables) — the everyday "select several paragraphs,
    /// press Delete" case. This succeeds today; the reported "multi-page
    /// delete doesn't work" bug turned out to be selections that cross into
    /// a table (see docs/67's "Cross-structure delete/selection" P0 row),
    /// not this same-container case — kept as a regression guard so the two
    /// don't get conflated again.
    #[test]
    fn delete_selection_spanning_several_plain_paragraphs_succeeds() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(_, t)| !t.is_empty())
            .map(|(id, _)| id.to_string())
            .expect("a non-empty paragraph");

        let mut current = node.clone();
        for label in ["p1", "p2", "p3", "p4", "p5"] {
            d.insert_text(&current, 0, label.to_string())
                .expect("insert label");
            let res = d
                .split_paragraph(&current, label.len() as u32)
                .expect("split after label");
            current = res.node();
        }
        // `current` is now the 6th paragraph (right after "p5"), at offset 0 —
        // the selection [node@0, current@0) spans exactly p1..p5.
        d.delete_selection(&node, 0, &current, 0)
            .expect("cross-paragraph delete of 5 plain paragraphs must succeed");
    }

    /// The actual reported bug: a selection starting in body text and ending
    /// inside a *nested* table's cell ("Outer" is the outer table's cell,
    /// "Nested A" is inside a table nested within it) must delete the
    /// covered text without trying to dissolve either table — and one undo
    /// restores everything exactly.
    #[test]
    fn delete_selection_crossing_into_a_table_clears_text_without_dissolving_it() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let (first_id, first_text) = nodes.first().cloned().expect("a first paragraph");
        let first = first_id.to_string();
        let (nested_a_id, nested_a_text) = nodes
            .iter()
            .find(|(_, t)| t == "Nested A")
            .cloned()
            .expect("the nested cell paragraph");
        let nested_a = nested_a_id.to_string();
        let nested_a_len = nested_a_text.len() as u32;

        d.delete_selection(&first, 0, &nested_a, nested_a_len)
            .expect("a body-into-table-cell delete must succeed, not `Unsupported`");

        // The table survived: "Nested A" is still its own paragraph (now
        // empty — its covered text was cleared) and "Nested B" (just past
        // the deleted range) is completely untouched.
        let mut after = Vec::new();
        collect_block_text(d.document.body(), &mut after);
        assert_eq!(
            after
                .iter()
                .find(|(id, _)| *id == nested_a_id)
                .map(|(_, t)| t.as_str()),
            Some(""),
            "the nested cell paragraph still exists, now empty"
        );
        assert!(
            after.iter().any(|(_, t)| t == "Nested B"),
            "an untouched cell past the deleted range is unaffected"
        );

        // One undo restores the original body text and the cell's text.
        d.undo().expect("undo the cross-structure delete");
        let mut restored = Vec::new();
        collect_block_text(d.document.body(), &mut restored);
        assert_eq!(
            restored
                .iter()
                .find(|(id, _)| *id == first_id)
                .map(|(_, t)| t.clone()),
            Some(first_text),
            "undo restores the first body paragraph verbatim"
        );
        assert_eq!(
            restored
                .iter()
                .find(|(id, _)| *id == nested_a_id)
                .map(|(_, t)| t.clone()),
            Some(nested_a_text),
            "undo restores the cell's text verbatim"
        );
    }

    /// Joining two paragraphs (Backspace at a paragraph start) keeps the
    /// *surviving* paragraph's own properties — a list item backspaced into
    /// a plain paragraph does not turn the result into a list item, matching
    /// Word's convention. Pins down existing `join_paragraphs` behavior
    /// (casual-doc-edit) so a future change can't silently regress it.
    #[test]
    fn join_paragraphs_keeps_the_surviving_paragraphs_properties() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(_, t)| !t.is_empty())
            .map(|(id, _)| id.to_string())
            .expect("a non-empty paragraph");

        // Split off a trailing paragraph and make *it* a bulleted list item;
        // the first paragraph stays plain.
        let split_at = d.copy_text(&node, 0, &node, 1).len() as u32;
        let res = d.split_paragraph(&node, split_at).expect("split");
        let second = res.node();
        d.toggle_list(&second, 0, &second, 1, "bullet")
            .expect("make the second paragraph a bullet item");
        assert_eq!(
            d.list_style_at(&second),
            "bullet",
            "second paragraph is now a list item"
        );
        assert_eq!(
            d.list_style_at(&node),
            "",
            "first paragraph was never a list item"
        );

        // Backspace at the second paragraph's start joins it into the first.
        d.delete_backward(&second, 0).expect("join via backspace");
        assert_eq!(
            d.list_style_at(&node),
            "",
            "the surviving (first) paragraph keeps its own non-list properties"
        );
    }

    /// `copyRichRuns` → `pasteRichRuns` round-trips a bold run, a hyperlinked
    /// run, and plain text as one undoable paste — the P0 "rich clipboard"
    /// gap (doc 67). Catches an export/import shape asymmetry directly,
    /// without a browser.
    #[test]
    fn rich_clipboard_round_trip_preserves_formatting_and_links() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(_, t)| !t.is_empty())
            .map(|(id, _)| id.to_string())
            .expect("a non-empty paragraph");

        // Build "Bold Link plain" at the paragraph start: "Bold " bold,
        // "Link" hyperlinked, " plain" unstyled.
        d.insert_styled_text(
            &node,
            0,
            "Bold ".to_string(),
            Some(true),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("insert bold");
        d.insert_text(&node, 5, "Link".to_string())
            .expect("insert link text");
        d.set_hyperlink(&node, 5, 9, "https://example.com".to_string(), None)
            .expect("set hyperlink");
        d.insert_text(&node, 9, " plain".to_string())
            .expect("insert plain text");

        let json = d.copy_rich_runs(&node, 0, &node, 15);
        let runs: Vec<ClipboardRun> = serde_json::from_str(&json).expect("valid clipboard json");
        assert_eq!(runs.len(), 3, "bold run + linked run + plain run");
        assert_eq!(runs[0].text, "Bold ");
        assert_eq!(runs[0].bold, Some(true));
        assert_eq!(runs[0].href, None);
        assert_eq!(runs[1].text, "Link");
        assert_eq!(runs[1].href.as_deref(), Some("https://example.com"));
        assert_eq!(runs[2].text, " plain");
        assert_eq!(runs[2].bold, None);
        assert_eq!(runs[2].href, None);

        // Paste the exact fragment back in at the paragraph's new end.
        let mut nodes2 = Vec::new();
        collect_block_text(d.document.body(), &mut nodes2);
        let end_offset = nodes2
            .iter()
            .find(|(id, _)| id.to_string() == node)
            .map(|(_, t)| t.len() as u32)
            .expect("node still present");

        d.paste_rich_runs(&node, end_offset, &node, end_offset, json)
            .expect("paste rich runs");

        let bold_end = end_offset + 5;
        assert!(
            d.format_at(&node, end_offset, bold_end).bold(),
            "pasted text is bold"
        );
        let link_start = bold_end;
        let link_end = link_start + 4;
        let paragraph = find_paragraph(
            d.document.body(),
            NodeId::from_str(&node).expect("valid node id"),
        )
        .expect("paragraph still present");
        let links = paragraph_links(&d.document, paragraph);
        assert!(
            links
                .iter()
                .any(|l| l.range.start.offset == link_start && l.range.end.offset == link_end),
            "pasted link lands at the expected range"
        );
        assert_eq!(
            d.copy_text(&node, link_end, &node, link_end + 6),
            " plain",
            "pasted plain text follows the link"
        );

        // One undo reverts the whole paste (delete-if-any + N inserts + format +
        // hyperlink), since it was built as a single `apply_action_caret` batch.
        d.undo().expect("undo paste");
        let mut nodes3 = Vec::new();
        collect_block_text(d.document.body(), &mut nodes3);
        let restored_len = nodes3
            .iter()
            .find(|(id, _)| id.to_string() == node)
            .map(|(_, t)| t.len() as u32)
            .expect("node still present after undo");
        assert_eq!(
            restored_len, end_offset,
            "undo removes the pasted fragment in one step"
        );
    }

    /// Editing works **inside a table cell** exactly as in the body: hit-testing
    /// resolves to the cell paragraph, and caret geometry, insert, delete, and
    /// formatting all operate on that node. (The rich fixture's nested table has a
    /// cell whose paragraph reads "Nested A".)
    #[test]
    fn edit_and_caret_inside_a_table_cell() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let (cell_id, text) = nodes
            .iter()
            .find(|(_, t)| t == "Nested A")
            .map(|(id, t)| (*id, t.clone()))
            .expect("the nested cell paragraph");
        let node = cell_id.to_string();

        // Caret geometry resolves for the cell paragraph (a non-empty flat rect).
        assert_eq!(
            d.caret_rect(&node, 0).len(),
            5,
            "caret rect resolves inside the cell"
        );

        // Insert, copy back, format, and delete — all on the cell node.
        d.insert_text(&node, 0, "X".to_string())
            .expect("insert into cell");
        assert_eq!(
            d.copy_text(&node, 0, &node, text.len() as u32 + 1),
            format!("X{text}")
        );
        d.format_text(&node, 0, 1, Some(true), None, None, None)
            .expect("bold in cell");
        assert!(d.format_at(&node, 0, 1).bold(), "cell text is bold");
        d.delete_range(&node, 0, 1).expect("delete in cell");
        assert_eq!(d.copy_text(&node, 0, &node, text.len() as u32), text);
    }

    /// Insert / delete a table row: the row count changes, the new row is editable,
    /// and both undo exactly.
    #[test]
    fn insert_and_delete_table_row_round_trip() {
        use casual_doc_edit::{find_table, locate_table_row};

        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let cell_id = nodes
            .iter()
            .find(|(_, t)| t == "Nested A")
            .map(|(id, _)| *id)
            .expect("the nested cell paragraph");
        let node = cell_id.to_string();

        let (table_id, _, _) =
            locate_table_row(&d.document, cell_id).expect("node is inside a table");
        let rows = |d: &WasmDocument| find_table(&d.document, table_id).unwrap().rows.len();
        let before = rows(&d);

        // Insert a row below and confirm its caret lands in a new, editable cell.
        let res = d.insert_row(&node, true).expect("insert row below");
        assert_eq!(rows(&d), before + 1);
        let new_para = res.node();
        d.insert_text(&new_para, 0, "NEW".to_string())
            .expect("type in the new row's cell");
        assert_eq!(d.copy_text(&new_para, 0, &new_para, 3), "NEW");

        // Undo the typing, then the insert; the row count returns.
        d.undo().expect("undo type");
        d.undo().expect("undo insert");
        assert_eq!(rows(&d), before);

        // Now there are >=2 rows only after inserting; insert once so delete is legal
        // (a table's last row cannot be deleted), then delete and undo.
        d.insert_row(&node, true).expect("insert for delete test");
        assert_eq!(rows(&d), before + 1);
        d.delete_row(&node).expect("delete the anchor row");
        assert_eq!(rows(&d), before);
        d.undo().expect("undo delete");
        assert_eq!(rows(&d), before + 1);
    }

    /// Insert / delete a table column on a regular table: the grid + every row's
    /// cell count change together, the new cell is editable, and both undo exactly.
    #[test]
    fn insert_and_delete_table_column_round_trip() {
        use casual_doc_edit::{find_table, locate_table_cell};

        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let cell_id = nodes
            .iter()
            .find(|(_, t)| t == "Nested A")
            .map(|(id, _)| *id)
            .expect("the nested cell paragraph");
        let node = cell_id.to_string();
        let (table_id, _) =
            locate_table_cell(&d.document, cell_id).expect("node is inside a table cell");

        // The nested "Nested A | Nested B" table is regular (2 columns, no merges).
        let cols = |d: &WasmDocument| find_table(&d.document, table_id).unwrap().grid.len();
        let cells0 = |d: &WasmDocument| {
            find_table(&d.document, table_id).unwrap().rows[0]
                .cells
                .len()
        };
        let before_cols = cols(&d);
        let before_cells = cells0(&d);

        // Insert a column to the right; grid and each row grow by one, new cell edits.
        let res = d.insert_column(&node, true).expect("insert column right");
        assert_eq!(cols(&d), before_cols + 1);
        assert_eq!(cells0(&d), before_cells + 1);
        let new_para = res.node();
        d.insert_text(&new_para, 0, "C".to_string())
            .expect("type in the new column's cell");
        assert_eq!(d.copy_text(&new_para, 0, &new_para, 1), "C");

        d.undo().expect("undo type");
        d.undo().expect("undo insert column");
        assert_eq!(cols(&d), before_cols);
        assert_eq!(cells0(&d), before_cells);

        // Delete the anchor's column; grid and each row shrink; undo restores.
        d.delete_column(&node).expect("delete column");
        assert_eq!(cols(&d), before_cols - 1);
        d.undo().expect("undo delete column");
        assert_eq!(cols(&d), before_cols);
    }

    #[test]
    fn table_distribution_preserves_totals_and_is_undoable() {
        use casual_doc_edit::{find_table, locate_table_cell};

        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let body = d
            .first_body_paragraph()
            .expect("body paragraph")
            .to_string();
        let inserted = d.insert_table(&body, 2, 3).expect("insert table");
        let anchor = inserted.node();
        let (table_id, _) = locate_table_cell(
            &d.document,
            NodeId::from_str(&anchor).expect("table anchor"),
        )
        .expect("table cell");
        let widths = |d: &WasmDocument| {
            find_table(&d.document, table_id)
                .unwrap()
                .grid
                .iter()
                .map(|column| column.width_twips.unwrap())
                .collect::<Vec<_>>()
        };
        let total = widths(&d).iter().sum::<i32>();
        d.set_table_row_height(&anchor, 600, "exact")
            .expect("first row height");
        let second = find_table(&d.document, table_id).unwrap().rows[1].cells[0]
            .blocks
            .first()
            .and_then(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id),
                _ => None,
            })
            .expect("second row paragraph")
            .to_string();
        d.set_table_row_height(&second, 1200, "exact")
            .expect("second row height");
        d.distribute_table_columns(&anchor)
            .expect("distribute columns");
        assert_eq!(widths(&d).iter().sum::<i32>(), total);
        assert!(widths(&d).windows(2).all(|pair| pair[0] == pair[1]));
        d.undo().expect("undo column distribution");
        assert_eq!(widths(&d).iter().sum::<i32>(), total);

        d.distribute_table_rows(&anchor).expect("distribute rows");
        let table = find_table(&d.document, table_id).unwrap();
        assert_eq!(
            table.rows[0].properties.height.value_twips,
            table.rows[1].properties.height.value_twips
        );
        d.undo().expect("undo row distribution");
    }

    /// Tab-style cell navigation moves row-major through the current innermost
    /// table without mutating it.
    #[test]
    fn table_cell_navigation_moves_row_major_without_editing() {
        let d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let nested_a = nodes
            .iter()
            .find(|(_, t)| t == "Nested A")
            .map(|(id, _)| *id)
            .expect("the first nested-table cell paragraph");
        let nested_b = nodes
            .iter()
            .find(|(_, t)| t == "Nested B")
            .map(|(id, _)| *id)
            .expect("the second nested-table cell paragraph");

        let next = d
            .move_table_cell(&nested_a.to_string(), true)
            .expect("next cell");
        assert_eq!(next.node(), nested_b.to_string());
        assert_eq!(next.offset(), 0);

        let prev = d
            .move_table_cell(&nested_b.to_string(), false)
            .expect("previous cell");
        assert_eq!(prev.node(), nested_a.to_string());
        assert_eq!(prev.offset(), 0);

        let (table, first_row, _) =
            locate_table_row(&d.document, nested_a).expect("nested A row location");
        let (_, first_col) = locate_table_cell(&d.document, nested_a).expect("nested A cell");
        assert_eq!(
            d.table_cell_anchor(table, first_row, first_col, false),
            None
        );

        let (table, last_row, _) =
            locate_table_row(&d.document, nested_b).expect("nested B row location");
        let (_, last_col) = locate_table_cell(&d.document, nested_b).expect("nested B cell");
        assert_eq!(d.table_cell_anchor(table, last_row, last_col, true), None);
    }

    #[test]
    fn table_info_reports_active_regular_table_context() {
        let d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let nested_b = nodes
            .iter()
            .find(|(_, t)| t == "Nested B")
            .map(|(id, _)| id.to_string())
            .expect("nested B cell");
        let body_para = nodes
            .iter()
            .find(|(id, _)| !d.in_table(&id.to_string()))
            .map(|(id, _)| id.to_string())
            .expect("body paragraph outside tables");

        let info = d.table_info(&nested_b);
        assert!(info.found());
        assert_eq!(info.row(), 0);
        assert_eq!(info.column(), 1);
        assert_eq!(info.rows(), 1);
        assert_eq!(info.columns(), 2);
        assert!(info.regular());
        assert!(!info.header_row());
        assert!(!info.table().is_empty());

        assert!(!d.table_info(&body_para).found());
    }

    #[test]
    fn table_selection_rects_report_cell_ranges() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let body_para = nodes
            .iter()
            .find(|(id, _)| !d.in_table(&id.to_string()))
            .map(|(id, _)| id.to_string())
            .expect("body paragraph outside tables");

        let res = d.insert_table(&body_para, 2, 3).expect("insert table");
        let caret = res.node();
        assert_eq!(d.table_selection_rects(&caret, "row").len(), 3 * 5);
        assert_eq!(d.table_selection_rects(&caret, "column").len(), 2 * 5);
        assert_eq!(d.table_selection_rects(&caret, "table").len(), 6 * 5);
        assert_eq!(d.table_column_resize_handles(&caret).len(), 2 * 5);
        assert!(d.table_selection_rects(&body_para, "table").is_empty());
        assert!(d.table_selection_rects(&caret, "unknown").is_empty());
    }

    #[test]
    fn table_caption_and_description_apply_reflect_and_undo() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let body = nodes
            .iter()
            .find(|(id, _)| !d.in_table(&id.to_string()))
            .map(|(id, _)| id.to_string())
            .expect("body paragraph");
        let anchor = d.insert_table(&body, 2, 2).expect("insert table").node();
        d.apply_table_properties(
            &anchor,
            r#"{"caption":"Sales","description":"Quarterly sales table"}"#,
        )
        .expect("metadata");
        let info = d.table_info(&anchor);
        assert_eq!(info.caption(), "Sales");
        assert_eq!(info.description(), "Quarterly sales table");
        d.undo().expect("undo metadata");
        let info = d.table_info(&anchor);
        assert_eq!(info.caption(), "");
        assert_eq!(info.description(), "");
    }

    #[test]
    fn table_formula_sum_above_writes_result_and_undoes() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let body = nodes
            .iter()
            .find(|(id, _)| !d.in_table(&id.to_string()))
            .map(|(id, _)| id.to_string())
            .expect("body paragraph");
        let anchor = d.insert_table(&body, 3, 2).expect("insert table").node();
        let first_cells = |d: &WasmDocument| {
            let (table, _) =
                locate_table_cell(&d.document, NodeId::from_str(&anchor).expect("anchor"))
                    .expect("table");
            find_table(&d.document, table)
                .unwrap()
                .rows
                .iter()
                .map(|row| match row.cells[0].blocks.first().unwrap() {
                    BlockNode::Paragraph(paragraph) => paragraph.id.to_string(),
                    _ => panic!("cell paragraph"),
                })
                .collect::<Vec<_>>()
        };
        let cells = first_cells(&d);
        d.insert_text(&cells[0], 0, "2".into()).expect("value 2");
        d.insert_text(&cells[1], 0, "3".into()).expect("value 3");
        d.calculate_table_formula(&cells[2], "=SUM(ABOVE)")
            .expect("sum above");
        assert_eq!(d.copy_text(&cells[2], 0, &cells[2], 1), "5");
        d.undo().expect("undo formula");
        assert_eq!(d.copy_text(&cells[2], 0, &cells[2], 1), "");
    }

    #[test]
    fn merge_and_split_selected_table_cells_round_trip() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let body_para = nodes
            .iter()
            .find(|(id, _)| !d.in_table(&id.to_string()))
            .map(|(id, _)| id.to_string())
            .expect("body paragraph outside tables");

        let res = d.insert_table(&body_para, 2, 3).expect("insert table");
        let caret = res.node();
        assert!(d.table_info(&caret).regular());

        let merged = d
            .merge_table_selection(&caret, "row")
            .expect("merge selected row");
        let merged_node = merged.node();
        let info = d.table_info(&merged_node);
        assert!(info.found());
        assert!(!info.regular(), "merged row is no longer a regular grid");
        assert_eq!(d.table_selection_rects(&merged_node, "row").len(), 5);

        let custom = d
            .split_merged_cell(&merged_node, Some(1), Some(4))
            .expect("split merged row into four columns");
        assert_eq!(d.table_info(&custom.node()).columns(), 4);
        assert!(d.table_info(&custom.node()).regular());
        d.undo().expect("undo custom split");

        let split = d
            .split_merged_cell(&merged_node, None, None)
            .expect("split merged row");
        let split_node = split.node();
        assert!(d.table_info(&split_node).regular());
        assert_eq!(d.table_selection_rects(&split_node, "table").len(), 6 * 5);

        d.undo().expect("undo split");
        assert!(!d.table_info(&merged_node).regular());
        d.undo().expect("undo merge");
        assert!(d.table_info(&caret).regular());
    }

    #[test]
    fn table_sort_reorders_rows_and_undoes_as_one_action() {
        use casual_doc_edit::{find_table, locate_table_cell};

        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let body = nodes
            .iter()
            .find(|(id, _)| !d.in_table(&id.to_string()))
            .map(|(id, _)| id.to_string())
            .expect("body paragraph");
        let inserted = d.insert_table(&body, 3, 2).expect("insert table");
        let anchor = inserted.node();
        let (table_id, _) = locate_table_cell(
            &d.document,
            NodeId::from_str(&anchor).expect("table anchor"),
        )
        .expect("table cell");
        let first_cell_paragraphs = |d: &WasmDocument| {
            find_table(&d.document, table_id)
                .unwrap()
                .rows
                .iter()
                .map(|row| match row.cells[0].blocks.first().unwrap() {
                    BlockNode::Paragraph(paragraph) => paragraph.id.to_string(),
                    _ => panic!("first cell paragraph"),
                })
                .collect::<Vec<_>>()
        };
        for (node, value) in first_cell_paragraphs(&d)
            .into_iter()
            .zip(["Bravo", "Alpha", "Charlie"])
        {
            d.insert_text(&node, 0, value.to_owned())
                .expect("cell text");
        }
        d.sort_table(&anchor, "ascending").expect("sort ascending");
        let sorted = first_cell_paragraphs(&d);
        assert_eq!(d.copy_text(&sorted[0], 0, &sorted[0], 5), "Alpha");
        assert_eq!(d.copy_text(&sorted[1], 0, &sorted[1], 6), "Bravo");
        d.undo().expect("undo sort");
        let restored = first_cell_paragraphs(&d);
        assert_eq!(d.copy_text(&restored[0], 0, &restored[0], 6), "Bravo");
    }

    #[test]
    fn table_properties_apply_reflect_and_undo() {
        use casual_doc_edit::{find_table, locate_table_cell};

        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let body_para = nodes
            .iter()
            .find(|(id, _)| !d.in_table(&id.to_string()))
            .map(|(id, _)| id.to_string())
            .expect("body paragraph outside tables");

        let res = d.insert_table(&body_para, 2, 3).expect("insert table");
        let caret = res.node();
        let (table_id, _cell_id) =
            locate_table_cell(&d.document, NodeId::from_str(&caret).unwrap()).unwrap();

        d.set_table_column_width(&caret, 2_160)
            .expect("set column width");
        let t = find_table(&d.document, table_id).unwrap();
        assert_eq!(t.grid[0].width_twips, Some(2_160));
        assert!(
            t.rows
                .iter()
                .all(|row| row.cells[0].properties.width_twips == Some(2_160))
        );
        assert_eq!(d.table_info(&caret).column_width_twips(), 2_160);

        d.set_table_header_row(&caret, true)
            .expect("set header row");
        assert!(d.table_info(&caret).header_row());

        d.set_table_width(&caret, 8_640).expect("set table width");
        d.set_table_indent(&caret, 720).expect("set table indent");
        d.set_table_fixed_layout(&caret, true)
            .expect("set fixed layout");
        d.set_table_row_height(&caret, 540, "exact")
            .expect("set row height");
        d.set_table_cell_margins(&caret, 144)
            .expect("set cell margins");
        d.set_table_cell_spacing(&caret, 72)
            .expect("set cell spacing");
        let info = d.table_info(&caret);
        assert_eq!(info.table_width_twips(), 8_640);
        assert_eq!(info.table_indent_twips(), 720);
        assert!(info.fixed_layout());
        assert_eq!(info.row_height_twips(), 540);
        assert_eq!(info.row_height_rule(), "exact");
        assert_eq!(info.cell_margin_twips(), 144);
        assert_eq!(info.cell_spacing_twips(), 72);

        d.undo().expect("undo cell spacing");
        assert_eq!(d.table_info(&caret).cell_spacing_twips(), -1);
        d.undo().expect("undo cell margins");
        assert_eq!(d.table_info(&caret).cell_margin_twips(), -1);
        d.undo().expect("undo row height");
        assert_eq!(d.table_info(&caret).row_height_twips(), -1);
        d.undo().expect("undo fixed layout");
        assert!(!d.table_info(&caret).fixed_layout());
        d.undo().expect("undo indent");
        assert_eq!(d.table_info(&caret).table_indent_twips(), 0);
        d.undo().expect("undo width");
        assert_eq!(d.table_info(&caret).table_width_twips(), -1);
        d.undo().expect("undo header");
        assert!(!d.table_info(&caret).header_row());
        d.undo().expect("undo column width");
        assert_ne!(d.table_info(&caret).column_width_twips(), 2_160);
    }

    #[test]
    fn table_properties_inspector_applies_as_one_undo_action() {
        use casual_doc_edit::{find_table, locate_table_cell};

        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let body_para = nodes
            .iter()
            .find(|(id, _)| !d.in_table(&id.to_string()))
            .map(|(id, _)| id.to_string())
            .expect("body paragraph outside tables");
        let caret = d
            .insert_table(&body_para, 2, 3)
            .expect("insert table")
            .node();
        let (table_id, _) =
            locate_table_cell(&d.document, NodeId::from_str(&caret).unwrap()).unwrap();
        let before = find_table(&d.document, table_id)
            .expect("inserted table")
            .clone();

        d.apply_table_properties(
            &caret,
            r#"{
                "alignment":"center",
                "tableWidthTwips":8640,
                "tableIndentTwips":720,
                "fixedLayout":true,
                "headerRow":true,
                "columnWidthTwips":2160,
                "rowHeightTwips":540,
                "rowHeightRule":"exact",
                "cellMarginTwips":144,
                "cellSpacingTwips":72
            }"#,
        )
        .expect("apply inspector draft");

        let info = d.table_info(&caret);
        assert_eq!(info.alignment(), "center");
        assert_eq!(info.table_width_twips(), 8_640);
        assert_eq!(info.table_indent_twips(), 720);
        assert!(info.fixed_layout());
        assert!(info.header_row());
        assert_eq!(info.column_width_twips(), 2_160);
        assert_eq!(info.row_height_twips(), 540);
        assert_eq!(info.row_height_rule(), "exact");
        assert_eq!(info.cell_margin_twips(), 144);
        assert_eq!(info.cell_spacing_twips(), 72);

        let undo = d.undo().expect("one undo reverses the complete inspector");
        assert!(
            d.in_table(&undo.node()),
            "undo should keep the caret in the restored table"
        );
        assert_eq!(
            find_table(&d.document, table_id).expect("table remains after undo"),
            &before
        );
    }

    /// Delete a whole table and undo it: the body's table count drops by one, the
    /// caret lands on an editable body paragraph, and undo restores the table.
    #[test]
    fn delete_table_and_undo_restores() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        // A paragraph directly in the first top-level table's first cell.
        let outer = d
            .document
            .body()
            .iter()
            .find_map(|b| match b {
                BlockNode::Table(t) => {
                    t.rows
                        .first()?
                        .cells
                        .first()?
                        .blocks
                        .iter()
                        .find_map(|bb| match bb {
                            BlockNode::Paragraph(p) => Some(p.id.to_string()),
                            _ => None,
                        })
                }
                _ => None,
            })
            .expect("a paragraph in the top-level table's first cell");

        let tables = |d: &WasmDocument| {
            d.document
                .body()
                .iter()
                .filter(|b| matches!(b, BlockNode::Table(_)))
                .count()
        };
        let before = tables(&d);
        assert!(before >= 1, "the fixture has a top-level table");

        let res = d.delete_table(&outer).expect("delete the table");
        assert_eq!(tables(&d), before - 1, "the table is gone");
        // The caret landed on a real, editable body paragraph.
        let caret = res.node();
        d.insert_text(&caret, 0, "Z".to_string())
            .expect("caret is a valid paragraph");

        d.undo().expect("undo type");
        d.undo().expect("undo delete");
        assert_eq!(tables(&d), before, "undo restored the table");
    }

    /// Toggling a bullet list on a paragraph: `listStyleAt` reflects it, the list
    /// definition is created once (a second toggle on another paragraph reuses it),
    /// the document still paginates, undo turns it off, and it survives export.
    #[test]
    fn toggle_bullet_list_reflects_creates_once_and_round_trips() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(_, t)| !t.is_empty())
            .map(|(id, _)| id.to_string())
            .expect("a non-empty paragraph");

        assert_eq!(d.list_style_at(&node), "", "not a list initially");
        d.toggle_list(&node, 0, &node, 0, "bullet")
            .expect("toggle bullet on");
        assert_eq!(d.list_style_at(&node), "bullet", "now a bullet list");
        assert!(d.page_count() >= 1, "still paginates with a list marker");

        // A second paragraph reuses the same instance (only one abstract+instance).
        let before_abstracts = d.document.definitions().abstract_numbering.len();
        if let Some((id2, _)) = nodes.iter().filter(|(_, t)| !t.is_empty()).nth(1) {
            let n2 = id2.to_string();
            d.toggle_list(&n2, 0, &n2, 0, "bullet")
                .expect("toggle second");
            assert_eq!(
                d.document.definitions().abstract_numbering.len(),
                before_abstracts,
                "the bullet definition is reused, not duplicated"
            );
        }

        d.undo().expect("undo second toggle");
        d.undo().expect("undo first toggle");
        assert_eq!(d.list_style_at(&node), "", "undo removed the list");

        // Re-apply and confirm the numbering survives export → re-open.
        d.toggle_list(&node, 0, &node, 0, "numbered")
            .expect("toggle numbered on");
        assert_eq!(d.list_style_at(&node), "numbered");
        for _ in 0..8 {
            d.adjust_list_level(&node, 0, &node, 0, 1)
                .expect("promote list level");
        }
        assert_eq!(
            paragraph_properties(&d.document, NodeId::from_str(&node).unwrap())
                .unwrap()
                .numbering
                .unwrap()
                .level,
            8
        );
        d.export_docx()
            .expect("deeply promoted editor list remains exportable");
        for _ in 0..8 {
            d.adjust_list_level(&node, 0, &node, 0, -1)
                .expect("demote list level");
        }
        assert_eq!(
            paragraph_properties(&d.document, NodeId::from_str(&node).unwrap())
                .unwrap()
                .numbering
                .unwrap()
                .level,
            0
        );
        let bytes = d.export_docx().expect("export");
        let reopened = open_document(&bytes).expect("re-open");
        assert!(
            !reopened.document.definitions().numbering.is_empty(),
            "the list's numbering definition survived export/re-open"
        );
    }

    #[test]
    fn toggling_an_empty_list_item_exits_the_list() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(_, text)| !text.is_empty())
            .map(|(id, _)| id.to_string())
            .expect("a non-empty paragraph");
        d.toggle_list(&node, 0, &node, 0, "bullet")
            .expect("toggle bullet on");
        let split = d
            .split_paragraph(&node, d.paragraph_length(&node))
            .expect("split list item");
        let empty = split.node();
        assert_eq!(d.paragraph_length(&empty), 0);
        assert_eq!(d.list_style_at(&empty), "bullet");
        d.toggle_list(&empty, 0, &empty, 0, "bullet")
            .expect("exit empty list item");
        assert_eq!(d.list_style_at(&empty), "");
    }

    #[test]
    fn restarting_numbered_list_carries_contiguous_items() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(_, text)| !text.is_empty())
            .map(|(id, _)| id.to_string())
            .expect("a non-empty paragraph");
        d.toggle_list(&node, 0, &node, 0, "numbered")
            .expect("toggle numbered on");
        let split = d
            .split_paragraph(&node, d.paragraph_length(&node))
            .expect("split numbered item");
        let second = split.node();
        let before = paragraph_properties(&d.document, NodeId::from_str(&node).unwrap())
            .unwrap()
            .numbering
            .unwrap()
            .instance;
        d.restart_list(&second).expect("restart numbered list");
        let first_after = paragraph_properties(&d.document, NodeId::from_str(&node).unwrap())
            .unwrap()
            .numbering
            .unwrap()
            .instance;
        let second_after = paragraph_properties(&d.document, NodeId::from_str(&second).unwrap())
            .unwrap()
            .numbering
            .unwrap()
            .instance;
        assert_eq!(first_after, before);
        assert_ne!(before, second_after);
        assert_eq!(
            d.document
                .definitions()
                .numbering
                .get(&second_after)
                .unwrap()
                .overrides[0]
                .start,
            Some(1)
        );
    }

    /// Paragraph and run properties apply and undo: alignment (with a query),
    /// plus line spacing / indent / shading / color / size / vert-align without
    /// error, all reversible, leaving the text intact.
    #[test]
    fn paragraph_and_run_properties_apply_and_undo() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let (node_id, text) = nodes
            .iter()
            .find(|(_, t)| t.len() >= 3)
            .map(|(id, t)| (*id, t.clone()))
            .expect("a paragraph with >=3 chars");
        let node = node_id.to_string();

        assert_eq!(d.alignment_at(&node, 0), "start");
        d.set_alignment(&node, 0, &node, 0, "center")
            .expect("align");
        assert_eq!(d.alignment_at(&node, 0), "center");

        d.set_line_spacing(&node, 0, &node, 0, 150)
            .expect("spacing");
        d.adjust_indent(&node, 0, &node, 0, 720).expect("indent");
        d.set_paragraph_shading(&node, 0, &node, 0, 255, 255, 0, false)
            .expect("shading");
        d.set_text_color(&node, 0, &node, 3, 255, 0, 0)
            .expect("color");
        d.set_font_size(&node, 0, &node, 3, 18.0).expect("size");
        d.set_vert_align(&node, 0, &node, 3, "super").expect("vert");

        // Undo every action; alignment returns to start and the text is intact.
        for _ in 0..7 {
            d.undo().expect("undo");
        }
        assert_eq!(d.alignment_at(&node, 0), "start");
        assert_eq!(d.copy_text(&node, 0, &node, text.len() as u32), text);
    }

    /// Line spacing (auto multiple vs exact/atLeast) and space before/after apply,
    /// reflect through `paragraphSpacing`, and undo. Guards the line-rule fidelity:
    /// the `auto` path must leave `line_rule` unset (0), not force it.
    #[test]
    fn paragraph_spacing_applies_reflects_and_undoes() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(_, t)| t.len() >= 3)
            .map(|(id, _)| id.to_string())
            .expect("a paragraph with >=3 chars");

        // Auto multiple: percent set, rule stays 0 (auto), no fixed twips.
        d.set_line_spacing(&node, 0, &node, 0, 150).expect("line %");
        let s = d.paragraph_spacing(&node);
        assert_eq!(
            (s.line_percent(), s.line_rule(), s.line_twip()),
            (150, 0, 0)
        );

        // Exact/atLeast: rule flips to atLeast(1), twips carried, percent cleared.
        d.set_line_spacing_exact(&node, 0, &node, 0, 360, true)
            .expect("line exact");
        let s = d.paragraph_spacing(&node);
        assert_eq!(
            (s.line_percent(), s.line_rule(), s.line_twip()),
            (0, 1, 360)
        );

        // Space before/after in twips; a negative value clears back to unset (-1).
        d.set_space_before(&node, 0, &node, 0, 240).expect("before");
        d.set_space_after(&node, 0, &node, 0, 160).expect("after");
        let s = d.paragraph_spacing(&node);
        assert_eq!((s.before_twip(), s.after_twip()), (240, 160));
        d.set_space_before(&node, 0, &node, 0, -1)
            .expect("clear before");
        assert_eq!(d.paragraph_spacing(&node).before_twip(), -1);

        // Undo the clear → space-before returns to 240.
        d.undo().expect("undo");
        assert_eq!(d.paragraph_spacing(&node).before_twip(), 240);
    }

    /// Line-and-page-break flags and paragraph shading apply, reflect, and undo.
    #[test]
    fn paragraph_flags_and_shading_apply_reflect_and_undo() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(_, t)| t.len() >= 3)
            .map(|(id, _)| id.to_string())
            .expect("a paragraph with >=3 chars");

        let f = d.paragraph_flags(&node);
        assert!(!f.keep_next() && !f.keep_lines() && !f.page_break_before());

        d.set_keep_with_next(&node, 0, &node, 0, true)
            .expect("keep");
        d.set_page_break_before(&node, 0, &node, 0, true)
            .expect("pbb");
        let f = d.paragraph_flags(&node);
        assert!(f.keep_next() && f.page_break_before() && !f.keep_lines());

        assert_eq!(d.paragraph_shading_at(&node), -1);
        d.set_paragraph_shading(&node, 0, &node, 0, 0xFF, 0xE0, 0x80, false)
            .expect("shade");
        assert_eq!(d.paragraph_shading_at(&node), 0x00FF_E080);
        d.set_paragraph_shading(&node, 0, &node, 0, 0, 0, 0, true)
            .expect("clear shade");
        assert_eq!(d.paragraph_shading_at(&node), -1);

        // Undo the clear → the fill returns.
        d.undo().expect("undo");
        assert_eq!(d.paragraph_shading_at(&node), 0x00FF_E080);
    }

    /// Paragraph borders: box sets all four edges, single-edge presets toggle, none
    /// clears, and the bitmask getter reflects it — all undoable.
    #[test]
    fn paragraph_borders_apply_reflect_and_undo() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(_, t)| t.len() >= 3)
            .map(|(id, _)| id.to_string())
            .expect("a paragraph with >=3 chars");

        assert_eq!(d.paragraph_border_edges(&node), 0);
        d.set_paragraph_border(&node, 0, &node, 0, "box", 0, 0, 0, 8)
            .expect("box");
        assert_eq!(d.paragraph_border_edges(&node), 0b1111);
        // Toggling the top edge off leaves bottom+left+right (0b1110).
        d.set_paragraph_border(&node, 0, &node, 0, "top", 0, 0, 0, 8)
            .expect("toggle top");
        assert_eq!(d.paragraph_border_edges(&node), 0b1110);
        d.set_paragraph_border(&node, 0, &node, 0, "none", 0, 0, 0, 8)
            .expect("none");
        assert_eq!(d.paragraph_border_edges(&node), 0);

        // Undo the clear → back to bottom+left+right.
        d.undo().expect("undo");
        assert_eq!(d.paragraph_border_edges(&node), 0b1110);
    }

    /// Tab stops: add (sorted, with alignment), move, cycle-by-replace, remove, and
    /// clear — reflected by `paragraphTabs` and undoable.
    #[test]
    fn tab_stops_apply_reflect_and_undo() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(_, t)| t.len() >= 3)
            .map(|(id, _)| id.to_string())
            .expect("a paragraph with >=3 chars");

        assert!(d.paragraph_tabs(&node).is_empty());
        d.set_tab_stop(&node, 0, &node, 0, 1440, 0)
            .expect("tab 1in L");
        d.set_tab_stop(&node, 0, &node, 0, 720, 2)
            .expect("tab .5in R");
        // Sorted by position: [720, End(2), 1440, Start(0)].
        assert_eq!(d.paragraph_tabs(&node), vec![720, 2, 1440, 0]);
        // Replace at 1440 with center (a "cycle").
        d.set_tab_stop(&node, 0, &node, 0, 1440, 1).expect("cycle");
        assert_eq!(d.paragraph_tabs(&node), vec![720, 2, 1440, 1]);
        d.apply_paragraph_props(&node, 0, &node, 0, |p| {
            p.tabs
                .iter_mut()
                .find(|t| t.position_twips == 1440)
                .expect("tab exists")
                .leader = Some(TabLeader::Dot);
        })
        .expect("add tab leader");
        d.set_tab_stop(&node, 0, &node, 0, 1440, 2)
            .expect("change alignment without dropping leader");
        assert_eq!(
            paragraph_properties(&d.document, NodeId::from_str(&node).unwrap())
                .unwrap()
                .tabs
                .iter()
                .find(|t| t.position_twips == 1440)
                .and_then(|t| t.leader),
            Some(TabLeader::Dot)
        );
        // Move 720 -> 2160.
        d.move_tab_stop(&node, 0, &node, 0, 720, 2160)
            .expect("move");
        assert_eq!(d.paragraph_tabs(&node), vec![1440, 2, 2160, 2]);
        // Remove 1440.
        d.remove_tab_stop(&node, 0, &node, 0, 1440).expect("remove");
        assert_eq!(d.paragraph_tabs(&node), vec![2160, 2]);
        d.clear_tab_stops(&node, 0, &node, 0).expect("clear");
        assert!(d.paragraph_tabs(&node).is_empty());

        // Undo the clear → the single remaining stop returns.
        d.undo().expect("undo");
        assert_eq!(d.paragraph_tabs(&node), vec![2160, 2]);
    }

    /// Cell shading / vertical alignment / borders and table borders apply through the
    /// caret's cell, reflect via the getters, and undo.
    #[test]
    fn table_cell_and_table_formatting_apply_reflect_and_undo() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let cell_para = nodes
            .iter()
            .find(|(id, _)| d.in_table(&id.to_string()))
            .map(|(id, _)| id.to_string())
            .expect("a paragraph inside a table cell");

        // Capture the corpus cell's initial state (it already carries some formatting)
        // and verify each edit changes it, then undo restores it exactly.
        let initial_shading = d.cell_shading_at(&cell_para);
        let initial_valign = d.cell_vertical_align_at(&cell_para);
        let initial_edges = d.cell_border_edges(&cell_para);

        d.set_cell_shading(&cell_para, 0xEE, 0xEE, 0x00, false)
            .expect("shade");
        assert_eq!(d.cell_shading_at(&cell_para), 0x00EE_EE00);

        d.set_cell_vertical_align(&cell_para, "bottom")
            .expect("valign");
        assert_eq!(d.cell_vertical_align_at(&cell_para), "bottom");

        // Clear then box the cell borders (independent of the corpus's initial edges).
        d.set_cell_border(&cell_para, "none", 0, 0, 0, 8)
            .expect("clear border");
        assert_eq!(d.cell_border_edges(&cell_para), 0);
        d.set_cell_border(&cell_para, "box", 0, 0, 0, 8)
            .expect("box border");
        assert_eq!(d.cell_border_edges(&cell_para), 0b1111);

        // A table-level border (outer + inside) applies without error.
        d.set_table_border(&cell_para, "all", 0, 0, 0, 8)
            .expect("table border");

        // Undo every edit in reverse → back to the initial state.
        d.undo().expect("u table border");
        d.undo().expect("u box");
        d.undo().expect("u clear");
        assert_eq!(d.cell_border_edges(&cell_para), initial_edges);
        d.undo().expect("u valign");
        assert_eq!(d.cell_vertical_align_at(&cell_para), initial_valign);
        d.undo().expect("u shading");
        assert_eq!(d.cell_shading_at(&cell_para), initial_shading);
    }

    /// Inserting a fresh table adds a 3×4 grid after the caret's block and lands the
    /// caret inside it; undo removes it. Table alignment applies and reflects.
    #[test]
    fn insert_table_and_align() {
        fn top_tables(blocks: &[BlockNode]) -> Vec<(usize, usize)> {
            blocks
                .iter()
                .filter_map(|b| match b {
                    BlockNode::Table(t) => {
                        Some((t.rows.len(), t.rows.first().map_or(0, |r| r.cells.len())))
                    }
                    _ => None,
                })
                .collect()
        }

        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let body_para = nodes
            .iter()
            .find(|(id, _)| !d.in_table(&id.to_string()))
            .map(|(id, _)| id.to_string())
            .expect("a top-level body paragraph");

        let before = top_tables(d.document.body());
        let res = d.insert_table(&body_para, 3, 4).expect("insert table");
        let caret = res.node();
        assert!(d.in_table(&caret), "caret lands inside the new table");
        let after = top_tables(d.document.body());
        assert_eq!(after.len(), before.len() + 1, "one more top-level table");
        assert!(
            after.contains(&(3, 4)),
            "a 3x4 table was inserted: {after:?}"
        );

        // Align that new table centered, then read it back.
        d.set_table_alignment(&caret, "center").expect("align");

        // Undo align + insert → back to the original table set.
        d.undo().expect("u align");
        d.undo().expect("u insert");
        assert_eq!(top_tables(d.document.body()), before);
    }

    /// The document outline lists heading paragraphs as `level\tnode\ttext`, in order.
    #[test]
    fn document_outline_lists_headings() {
        let d = open_document(RICH_DOCX).expect("open corpus docx");
        let outline = d.document_outline();
        // The corpus's "Rich Document" is a Heading 1 → a level-1 entry exists.
        assert!(
            outline.iter().any(|row| {
                let mut it = row.splitn(3, '\t');
                it.next() == Some("1") && it.nth(1).is_some_and(|t| t.contains("Rich Document"))
            }),
            "outline has the Heading 1 title: {outline:?}"
        );
        // Every row is well-formed: numeric level, a node id, non-empty text.
        for row in &outline {
            let parts: Vec<&str> = row.splitn(3, '\t').collect();
            assert_eq!(parts.len(), 3, "row has level\\tnode\\ttext: {row:?}");
            assert!(parts[0].parse::<u8>().is_ok(), "numeric level: {row:?}");
            assert!(!parts[2].trim().is_empty(), "non-empty text: {row:?}");
        }
    }

    /// A `pageBreakBefore` set through the live edit path must re-paginate and add a
    /// page (guards the incremental pagination honoring break control, not just the
    /// from-scratch paginator).
    #[test]
    fn page_break_before_repaginates_through_the_edit_path() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        // Second top-level (non-table) paragraph — forcing a break before it must
        // push it (and everything after) to a new page.
        let target = nodes
            .iter()
            .filter(|(id, _)| locate_table_row(&d.document, *id).is_none())
            .nth(1)
            .map(|(id, _)| id.to_string())
            .expect("a second body paragraph");

        let before = d.page_count();
        d.set_page_break_before(&target, 0, &target, 0, true)
            .expect("page break before");
        assert!(
            d.page_count() > before,
            "pageBreakBefore must force a new page through the edit path (before={before}, after={})",
            d.page_count()
        );

        // Undo restores the single page.
        d.undo().expect("undo");
        assert_eq!(d.page_count(), before);
    }

    /// Font family applies over a range; paragraph style applies, reads back, and
    /// undoes (when the document defines paragraph styles).
    #[test]
    fn font_family_and_paragraph_style() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(_, t)| t.len() >= 3)
            .map(|(id, _)| id.to_string())
            .expect("a paragraph with >=3 chars");

        d.set_font(&node, 0, &node, 3, "Arial".to_string())
            .expect("set font");

        let styles = d.list_styles();
        if let Some(name) = styles.first().cloned() {
            d.set_paragraph_style(&node, 0, &node, 0, &name)
                .expect("set style");
            assert_eq!(d.paragraph_style_at(&node), name);
            d.undo().expect("undo style");
            assert_ne!(d.paragraph_style_at(&node), name, "undo cleared the style");
        }
        // (A non-existent style name is rejected — verified in-browser, since the
        // error path constructs a JsValue which panics off-wasm.)
    }

    /// Formatting spans paragraphs: bold a selection across two paragraphs, and
    /// the combined query reports bold; undo clears it.
    #[test]
    fn format_selection_spans_paragraphs() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let pair = nodes
            .windows(2)
            .find(|w| !w[0].1.is_empty() && !w[1].1.is_empty())
            .expect("two adjacent non-empty paragraphs");
        let a = pair[0].0.to_string();
        let b = pair[1].0.to_string();

        assert!(!d.selection_format(&a, 0, &b, 1).bold());
        d.format_selection(&a, 0, &b, 1, Some(true), None, None, None)
            .expect("format across paragraphs");
        assert!(
            d.selection_format(&a, 0, &b, 1).bold(),
            "bold applied across both paragraphs"
        );

        d.undo().expect("undo");
        assert!(!d.selection_format(&a, 0, &b, 1).bold(), "undo cleared it");
    }

    #[test]
    fn formatting_queries_distinguish_uniform_unset_from_mixed() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes
            .iter()
            .find(|(id, text)| {
                text.is_ascii()
                    && text.len() >= 4
                    && d.selection_format(&id.to_string(), 0, &id.to_string(), 4)
                        .bold_state()
                        == 0
                    && {
                        let style = d.selection_run_style(&id.to_string(), 0, &id.to_string(), 4);
                        style.highlight() == "none" && !style.highlight_mixed()
                    }
            })
            .map(|(id, _)| id.to_string())
            .expect("four plain ASCII bytes");

        let initial = d.selection_run_style(&node, 0, &node, 4);
        assert_eq!(initial.highlight(), "none");
        assert!(!initial.highlight_mixed());

        d.format_selection(&node, 0, &node, 2, Some(true), None, None, None)
            .expect("bold first half");
        let format = d.selection_format(&node, 0, &node, 4);
        assert_eq!(format.bold_state(), 2, "part-bold selection is mixed");
        assert!(!format.bold(), "mixed is not reported as uniformly on");

        d.set_font_size(&node, 0, &node, 2, 12.5)
            .expect("half-point font size");
        d.set_highlight(&node, 0, &node, 2, "yellow")
            .expect("highlight first half");
        let mixed = d.selection_run_style(&node, 0, &node, 4);
        assert!(mixed.size_mixed());
        assert!(mixed.highlight_mixed());
        assert_eq!(mixed.highlight(), "");

        d.set_vert_align(&node, 0, &node, 4, "super")
            .expect("superscript selection");
        let raised = d.selection_run_style(&node, 0, &node, 4);
        assert_eq!(raised.vertical_align(), "super");
        assert!(raised.superscript());
        d.set_vert_align(&node, 0, &node, 4, "baseline")
            .expect("toggle back to baseline");
        let baseline = d.selection_run_style(&node, 0, &node, 4);
        assert_eq!(baseline.vertical_align(), "baseline");
        assert!(!baseline.superscript());
        assert!(!baseline.subscript());
    }

    #[test]
    fn paragraph_state_reports_every_selected_paragraph_and_mixed_values() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let pair = nodes
            .windows(2)
            .find(|window| !window[0].1.is_empty() && !window[1].1.is_empty())
            .expect("two adjacent non-empty paragraphs");
        let first = pair[0].0.to_string();
        let second = pair[1].0.to_string();

        d.set_left_indent(&first, 0, &first, 0, 720)
            .expect("indent only first paragraph");
        d.set_keep_with_next(&first, 0, &first, 0, true)
            .expect("flag only first paragraph");
        let mixed = d.selection_paragraph_state(&first, 0, &second, 1);
        assert_eq!(mixed.count(), 2);
        assert!(mixed.start_mixed());
        assert_eq!(mixed.keep_next_state(), 2);

        d.set_left_indent(&first, 0, &second, 1, 360)
            .expect("set both selected paragraphs");
        d.set_keep_with_next(&first, 0, &second, 1, true)
            .expect("set flag on both");
        let uniform = d.selection_paragraph_state(&first, 0, &second, 1);
        assert_eq!(uniform.start_twip(), 360);
        assert!(!uniform.start_mixed());
        assert_eq!(uniform.keep_next_state(), 1);
        assert_eq!(d.undo_label(), "Paragraph formatting");
    }

    /// Split (Enter) divides a paragraph and undo rejoins it; word selection and
    /// caret movement resolve sensibly.
    #[test]
    fn split_word_and_move_caret() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let (node_id, text) = nodes
            .iter()
            .find(|(_, t)| t.contains(' '))
            .map(|(id, t)| (*id, t.clone()))
            .expect("a paragraph with a space");
        let node = node_id.to_string();

        // Word at offset 0 spans the first word.
        let word = d.word_at(&node, 0);
        assert_eq!(word.len(), 2);
        assert_eq!(word[0], 0);
        assert!(word[1] > 0 && (word[1] as usize) <= text.len());

        // Move right by one (ASCII) byte.
        let caret = d.move_caret(&node, 0, "right").expect("move");
        assert_eq!(caret.offset(), 1);
        assert_eq!(caret.node(), node);

        // Split at the first space; the head keeps text[..sp]; undo rejoins.
        let sp = text.find(' ').unwrap() as u32;
        let result = d.split_paragraph(&node, sp).expect("split");
        assert_eq!(result.offset(), 0, "caret at the new paragraph start");
        assert_eq!(d.copy_text(&node, 0, &node, sp), text[..sp as usize]);
        d.undo().expect("undo split");
        assert_eq!(d.copy_text(&node, 0, &node, text.len() as u32), text);
    }

    /// Backspace deletes the character before the caret and undo restores it.
    #[test]
    fn backspace_deletes_previous_char() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let (node_id, original) = nodes
            .iter()
            .find(|(_, t)| t.len() >= 2)
            .map(|(id, t)| (*id, t.clone()))
            .expect("a paragraph with >=2 chars");
        let node = node_id.to_string();

        // Backspace at offset 1 removes the first character.
        d.delete_backward(&node, 1).expect("backspace");
        assert_eq!(
            d.copy_text(&node, 0, &node, (original.len() - 1) as u32),
            original[1..]
        );
        d.undo().expect("undo");
        assert_eq!(
            d.copy_text(&node, 0, &node, original.len() as u32),
            original
        );
    }

    #[test]
    fn word_delete_uses_unicode_boundaries_and_undo_restores_once() {
        let mut d = open_document(RICH_DOCX).expect("open corpus docx");
        let (node_id, _) = d
            .ordered_paragraphs()
            .into_iter()
            .find(|(_, len)| *len > 0)
            .expect("a non-empty paragraph");
        let node = node_id.to_string();
        let marker = "alpha café";
        d.insert_text(&node, 0, marker.to_owned())
            .expect("insert controlled words");

        d.delete_word_backward(&node, marker.len() as u32)
            .expect("delete previous Unicode word");
        assert!(
            d.paragraph_text(node_id).starts_with("alpha "),
            "backward deletion removes the whole multi-byte word"
        );
        d.undo().expect("undo word deletion");
        assert!(
            d.paragraph_text(node_id).starts_with(marker),
            "one undo restores the complete word"
        );

        d.delete_word_forward(&node, 0)
            .expect("delete following word");
        assert!(
            d.paragraph_text(node_id).starts_with(" café"),
            "forward deletion removes one word and preserves its following separator"
        );
    }

    #[test]
    fn paragraph_navigation_moves_to_model_paragraph_boundaries() {
        let d = open_document(RICH_DOCX).expect("open corpus docx");
        let paras = d.ordered_paragraphs();
        let pair = paras
            .windows(2)
            .find(|pair| pair[0].1 > 0 && pair[1].1 > 0)
            .expect("two adjacent non-empty paragraphs");
        let first = pair[0];
        let second = pair[1];

        let current_start = d
            .move_caret(&second.0.to_string(), 1, "paragraphUp")
            .expect("move to current paragraph start");
        assert_eq!(current_start.node(), second.0.to_string());
        assert_eq!(current_start.offset(), 0);

        let previous_start = d
            .move_caret(&second.0.to_string(), 0, "paragraphUp")
            .expect("move to previous paragraph start");
        assert_eq!(previous_start.node(), first.0.to_string());
        assert_eq!(previous_start.offset(), 0);

        let next_start = d
            .move_caret(&first.0.to_string(), 1, "paragraphDown")
            .expect("move to next paragraph start");
        assert_eq!(next_start.node(), second.0.to_string());
        assert_eq!(next_start.offset(), 0);
    }

    /// Copy across two paragraphs joins them with a newline.
    #[test]
    fn copy_spans_paragraphs() {
        let doc = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(doc.document.body(), &mut nodes);
        // Two *adjacent* text nodes, so exactly one newline joins them (no empty
        // paragraph in between contributing an extra break).
        let pair = nodes
            .windows(2)
            .find(|w| !w[0].1.is_empty() && !w[1].1.is_empty())
            .expect("two adjacent non-empty paragraphs");
        let (a_id, a_text) = &pair[0];
        let (b_id, b_text) = &pair[1];

        let copied = doc.copy_text(&a_id.to_string(), 0, &b_id.to_string(), b_text.len() as u32);
        assert_eq!(copied, format!("{a_text}\n{b_text}"));
    }

    #[test]
    fn tabbed_toc_link_uses_layout_offset_space_and_is_clickable() {
        use casual_doc_model::v1::{Bookmark, BookmarkStart, Definitions, Hyperlink, Run, Tab};

        let document_id = NodeId::from_parts(90, 1).unwrap();
        let source_id = NodeId::from_parts(90, 2).unwrap();
        let target_id = NodeId::from_parts(90, 3).unwrap();
        let bookmark_id = BookmarkId::new(NodeId::from_parts(90, 4).unwrap());
        let mut definitions = Definitions::default();
        definitions.bookmarks.insert(
            bookmark_id,
            Bookmark {
                name: "_Toc1".to_owned(),
            },
        );
        let document = Document::new(
            document_id,
            vec![
                BlockNode::Paragraph(Paragraph {
                    id: source_id,
                    properties: ParagraphProperties::default(),
                    inlines: vec![InlineNode::Hyperlink(Hyperlink {
                        id: NodeId::from_parts(90, 5).unwrap(),
                        target: HyperlinkTarget::Internal(InternalTarget {
                            anchor: "_Toc1".to_owned(),
                        }),
                        tooltip: None,
                        inlines: vec![
                            InlineNode::Run(Run {
                                id: NodeId::from_parts(90, 6).unwrap(),
                                properties: Default::default(),
                                text: "Tables".to_owned(),
                            }),
                            InlineNode::Tab(Tab {
                                id: NodeId::from_parts(90, 7).unwrap(),
                            }),
                            InlineNode::Run(Run {
                                id: NodeId::from_parts(90, 8).unwrap(),
                                properties: Default::default(),
                                text: "3".to_owned(),
                            }),
                        ],
                    })],
                }),
                BlockNode::Paragraph(Paragraph {
                    id: target_id,
                    properties: ParagraphProperties::default(),
                    inlines: vec![
                        InlineNode::BookmarkStart(BookmarkStart {
                            id: NodeId::from_parts(90, 9).unwrap(),
                            bookmark: bookmark_id,
                        }),
                        InlineNode::Run(Run {
                            id: NodeId::from_parts(90, 10).unwrap(),
                            properties: Default::default(),
                            text: "Tables heading".to_owned(),
                        }),
                    ],
                }),
            ],
            definitions,
        )
        .expect("valid TOC-like document");

        let shaper = ParleyShaper::new();
        let layout = paginate_document(&document, &shaper);
        let default_config = document_page_config(&document);
        let handle = WasmDocument {
            document,
            layout,
            shaper,
            media: BTreeMap::new(),
            default_config,
            edit_ids: IdGenerator::new(0x5a),
            undo: Vec::new(),
            redo: Vec::new(),
            typing_history: None,
            revision: 0,
            galley_cache: GalleyCache::new(),
            bullet_list: None,
            numbered_list: None,
        };

        let paragraph =
            find_paragraph(handle.document.body(), source_id).expect("source paragraph");
        let links = paragraph_links(&handle.document, paragraph);
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].range,
            ModelRange::new(ModelPos::new(source_id, 0), ModelPos::new(source_id, 7)),
            "tabs position paint but consume no byte in the layout anchor space"
        );
        let rects = LayoutSnapshot::new(&handle.layout).selection_rects(links[0].range);
        let (page, rect) = rects.first().copied().expect("linked TOC row geometry");
        let point = Point::new(
            rect.origin.x + Twip(rect.size.width.raw() / 2),
            rect.origin.y + Twip(rect.size.height.raw() / 2),
        );
        let hit = handle
            .link_at(page, point.x.raw(), point.y.raw())
            .expect("painted tabbed TOC row is clickable");
        assert_eq!(hit.kind(), "internal");
        assert_eq!(hit.anchor(), "_Toc1");
        assert_eq!(hit.target_node(), target_id.to_string());
        assert!(hit.target_page() > 0);
    }
}
