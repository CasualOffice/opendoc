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
    FormatDelta, Operation, Pos, Range as EditRange, apply as apply_edit, format_state,
    paragraph_properties, run_style_state,
};
use casual_doc_export::write_document;
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::{document_page_config, paginate_document};
use casual_doc_layout::flow::node_plain_text;
use casual_doc_layout::hittest::{Direction, HitZone, LayoutSnapshot};
use casual_doc_layout::model::{ModelPos, ModelRange};
use casual_doc_layout::page::{Page, PaginatedLayout};
use casual_doc_layout::paginate::PageConfig;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::{Point, Rect, Size, Twip};
use casual_doc_model::v1::{
    Alignment, BlockNode, Document, HighlightColor, Indentation, ParagraphProperties, RgbColor,
    StyleId, StyleKind, VerticalAlignment,
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
    undo: Vec<Vec<Operation>>,
    /// Redo stack — forward-op groups of undone actions; cleared on a fresh edit.
    redo: Vec<Vec<Operation>>,
    /// Monotonic model revision, bumped on every applied edit.
    revision: u32,
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
                    .apply(Operation::JoinParagraphs {
                        first: paras[i - 1].0,
                        second: nid,
                    })
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
                    .apply(Operation::JoinParagraphs {
                        first: nid,
                        second: paras[i + 1].0,
                    })
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
        let ops = self
            .selection_delete_ops(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        if ops.is_empty() {
            return Err(to_js("empty selection".into()));
        }
        self.apply_action(ops).map_err(to_js)
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
        self.apply_action(ops).map_err(to_js)
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

    /// The uniform format state of a range (each toggle `true` only when every
    /// covered run sets it) — drives a toolbar's active state and toggle direction.
    #[wasm_bindgen(js_name = formatAt)]
    #[must_use]
    pub fn format_at(&self, node: &str, start: u32, end: u32) -> Format {
        let Ok(nid) = NodeId::from_str(node) else {
            return Format::default();
        };
        let state = format_state(
            &self.document,
            EditRange {
                start: Pos::new(nid, start),
                end: Pos::new(nid, end),
            },
        );
        Format {
            bold: state.bold,
            italic: state.italic,
            underline: state.underline,
            strike: state.strike,
        }
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
        let mut acc = Format {
            bold: true,
            italic: true,
            underline: true,
            strike: true,
        };
        for (node, s, e) in subs {
            let st = format_state(
                &self.document,
                EditRange {
                    start: Pos::new(node, s),
                    end: Pos::new(node, e),
                },
            );
            acc.bold &= st.bold;
            acc.italic &= st.italic;
            acc.underline &= st.underline;
            acc.strike &= st.strike;
        }
        acc
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
        let half_points = (points * 2.0).round().max(2.0) as u32;
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

    /// Sets line spacing (as a percentage of single: 100/150/200) over the
    /// selection's paragraphs.
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
            spacing.line_rule = Some(casual_doc_model::v1::LineRule::Auto);
            spacing.line_twips = None;
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

    /// The uniform run styling of the selection (size/color/font/vert-align) for
    /// reflecting the current values in the toolbar. Blank/zero for a mixed or
    /// cross-paragraph selection.
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
        if start.node != end.node {
            return RunStyle::default();
        }
        let state = run_style_state(&self.document, EditRange { start, end });
        RunStyle {
            size_points: state.size_half_points.map_or(0.0, |h| h as f32 / 2.0),
            color: state.color_rgb.map_or_else(String::new, |c| {
                format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
            }),
            font: state.font.unwrap_or_default(),
            superscript: state.superscript,
            subscript: state.subscript,
        }
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

    /// Undoes the last user action (one or many ops), returning the restored
    /// caret + revision.
    #[wasm_bindgen(js_name = undo)]
    pub fn undo(&mut self) -> Result<EditResult, JsValue> {
        let group = self
            .undo
            .pop()
            .ok_or_else(|| to_js("nothing to undo".into()))?;
        // `group` is stored in the order it must be re-applied to undo the action.
        let (caret, redo_group) = self
            .apply_group(group)
            .map_err(|e| to_js(format!("undo failed: {e}")))?;
        self.redo.push(redo_group);
        Ok(self.finish_edit(caret))
    }

    /// Redoes the last undone action.
    #[wasm_bindgen(js_name = redo)]
    pub fn redo(&mut self) -> Result<EditResult, JsValue> {
        let group = self
            .redo
            .pop()
            .ok_or_else(|| to_js("nothing to redo".into()))?;
        let (caret, undo_group) = self
            .apply_group(group)
            .map_err(|e| to_js(format!("redo failed: {e}")))?;
        self.undo.push(undo_group);
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
        self.layout = paginate_document(&self.document, &self.shaper);
    }

    /// Applies one forward op as a single undoable action, with the caret at that
    /// op's natural resting place.
    fn apply(&mut self, op: Operation) -> Result<EditResult, String> {
        self.apply_action(vec![op])
    }

    /// Applies a batch of forward ops as one undoable action. The caret rests at
    /// the last op's natural place. A multi-op batch is atomic: if any op fails the
    /// document is rolled back, so a partially-applied cross-paragraph edit never
    /// corrupts. A single op needs no snapshot (it validates before mutating), so
    /// typing stays clone-free.
    fn apply_action(&mut self, ops: Vec<Operation>) -> Result<EditResult, String> {
        if ops.is_empty() {
            return Err("empty edit".into());
        }
        let snapshot = (ops.len() > 1).then(|| self.document.clone());
        match self.apply_group(ops) {
            Ok((caret, inverses)) => {
                self.undo.push(inverses);
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

    /// Applies each op in `ops`, returning the caret (from the last op) and the
    /// inverse group ordered so that applying it in turn undoes the whole action.
    fn apply_group(&mut self, ops: Vec<Operation>) -> Result<(Pos, Vec<Operation>), String> {
        let mut inverses = Vec::with_capacity(ops.len());
        let mut caret = Pos::new(self.document.id(), 0);
        for op in &ops {
            let inverse = apply_edit(&mut self.document, &mut self.edit_ids, op)
                .map_err(|e| format!("{e:?}"))?;
            caret = caret_after(op, &inverse);
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
        let new_layout = paginate_document(&self.document, &self.shaper);
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
    /// paragraph + the end head, then join them all into the start paragraph.
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
        // including the end paragraph into the start paragraph.
        for (id, _) in paras.iter().take(ei + 1).skip(si + 1) {
            ops.push(Operation::JoinParagraphs {
                first: start.node,
                second: *id,
            });
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
        self.apply_action(ops).map_err(to_js)
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
        revision: 0,
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
fn caret_after(op: &Operation, inverse: &Operation) -> Pos {
    match op {
        Operation::InsertText { at, text } => Pos::new(at.node, at.offset + text.len() as u32),
        Operation::DeleteText { range } => range.start,
        Operation::SplitParagraph { new_id, .. } => Pos::new(*new_id, 0),
        Operation::JoinParagraphs { first, .. } => match inverse {
            Operation::SplitParagraph { at, .. } => *at,
            _ => Pos::new(*first, 0),
        },
        // Formatting keeps the selection; the frontend does not collapse to this.
        Operation::FormatText { range, .. } => range.start,
        Operation::SetInlines { node, .. } | Operation::SetParagraphProperties { node, .. } => {
            Pos::new(*node, 0)
        }
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
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

#[wasm_bindgen]
impl Format {
    /// Every covered run is bold.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn bold(&self) -> bool {
        self.bold
    }

    /// Every covered run is italic.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn italic(&self) -> bool {
        self.italic
    }

    /// Every covered run is underlined.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn underline(&self) -> bool {
        self.underline
    }

    /// Every covered run is struck through.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn strike(&self) -> bool {
        self.strike
    }
}

/// The uniform run styling of a selection, for reflecting current values in the
/// toolbar. `sizePoints` is 0 and `color`/`font` are empty for a mixed selection.
#[wasm_bindgen]
#[derive(Clone, Debug, Default)]
pub struct RunStyle {
    size_points: f32,
    color: String,
    font: String,
    superscript: bool,
    subscript: bool,
}

#[wasm_bindgen]
impl RunStyle {
    /// Common font size in points (0 if mixed/unset).
    #[wasm_bindgen(getter, js_name = sizePoints)]
    #[must_use]
    pub fn size_points(&self) -> f32 {
        self.size_points
    }

    /// Common text color as `#rrggbb` (empty if mixed/unset or a theme color).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn color(&self) -> String {
        self.color.clone()
    }

    /// Common font family (empty if mixed/unset).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn font(&self) -> String {
        self.font.clone()
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

    const RICH_DOCX: &[u8] = include_bytes!("../../../fixtures/corpus/real-producer-rich.docx");

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
            .find(|(_, t)| t.len() >= 3)
            .map(|(id, _)| id.to_string())
            .expect("a paragraph with >=3 chars");

        assert!(!d.format_at(&node, 0, 3).bold(), "not bold initially");
        d.format_text(&node, 0, 3, Some(true), None, None, None)
            .expect("bold");
        assert!(d.format_at(&node, 0, 3).bold(), "now bold over [0,3)");

        d.undo().expect("undo");
        assert!(!d.format_at(&node, 0, 3).bold(), "undo cleared bold");
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
}
