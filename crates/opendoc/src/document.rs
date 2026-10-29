// SPDX-License-Identifier: Apache-2.0

//! [`Document`]: open or create, read, edit, template, save, render.

use std::cell::OnceCell;
use std::collections::{BTreeMap, HashMap};

use casual_doc_io::{
    DetectionRequest, DocumentResources, ExportMode, ExportRequest, FormatId, FormatRegistry,
    FormatSelection, PlainTextLimits, SourceEnvelope, builtin_registry_with_limits,
};
use casual_doc_model::v1::{CustomValue, Document as Model, node_plain_text};
use casual_doc_model::{IdGenerator, IdSpace, NodeId};
use casual_doc_review::body::{
    RevisionIdAllocator, build_review_comment_summaries, build_review_revision_summaries,
};
use opendoc_viewer::{ViewerDocument, ViewerOptions};

use crate::builder::{DocumentSpec, build_docx};
use crate::edit::{EditCommand, EditState, Editor, Undo};
use crate::error::{Error, ErrorCode, Result};
use crate::format::{Format, SaveMode};
use crate::report::{ImportReport, SaveReport};
use crate::template::{self, TemplateOptions};
use crate::view::{
    AnchorView, Block, CommentView, HeaderFooterView, MediaView, NoteView, ParagraphView,
    Projector, PropertiesView, RevisionView, SectionView, StyleView, TableView,
};

/// How to open a document.
#[derive(Clone, Debug, Default)]
pub struct OpenOptions {
    /// The format, when the caller knows it. Otherwise it is detected from the
    /// bytes (and `file_name`'s extension when the bytes are ambiguous).
    pub format: Option<Format>,
    /// The file name, as a detection hint.
    pub file_name: Option<String>,
    /// The author review edits (comments, suggestions) are attributed to.
    /// Default `"OpenDoc"`.
    pub author: Option<String>,
    /// Render options.
    pub render: RenderOptions,
}

/// How pages are laid out and drawn.
#[derive(Clone, Debug, Default)]
pub struct RenderOptions {
    /// Let layout fall back to the machine's fonts. Off by default: with only
    /// the bundled faces (and `fonts`), the same document renders the same
    /// pixels on every machine (`docs/176` §2.1).
    pub system_fonts: bool,
    /// Extra font files (TTF/OTF) to lay out with.
    pub fonts: Vec<Vec<u8>>,
}

/// A rendered page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Premultiplied RGBA, row-major.
    pub rgba: Vec<u8>,
}

/// A page's size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSize {
    /// Width in points.
    pub width_pt: f64,
    /// Height in points.
    pub height_pt: f64,
}

/// The input ceilings: the same envelope the editor and the viewer admit
/// (`docs/176` R1): a large DOCX is large because of its pictures.
fn package_limits() -> casual_doc_ooxml::PackageLimits {
    casual_doc_ooxml::PackageLimits {
        max_input_bytes: 200 * 1024 * 1024,
        max_total_expanded_bytes: 512 * 1024 * 1024,
        max_single_expanded_bytes: 200 * 1024 * 1024,
        ..casual_doc_ooxml::PackageLimits::default()
    }
}

/// The registry every open and save goes through.
fn registry() -> FormatRegistry {
    #[allow(unused_mut)]
    let mut registry = builtin_registry_with_limits(package_limits(), PlainTextLimits::default());
    #[cfg(feature = "pdf-import")]
    {
        // A failure here means the adapter is already registered.
        drop(casual_doc_io::enable_pdf_import(
            &mut registry,
            casual_doc_io::PdfReadLimits::default(),
        ));
    }
    registry
}

/// An open document.
///
/// Reads project the current state; [`Document::edit`] changes it through the
/// engine's operations; [`Document::save`] writes it with a loss report; the
/// render calls lay it out once (cached until the next edit) and draw from that
/// one layout, so the PNG, the page text and the PDF agree.
pub struct Document {
    state: EditState,
    resources: DocumentResources,
    source: Option<SourceEnvelope>,
    source_format: Option<Format>,
    import: ImportReport,
    changed: bool,
    author: String,
    render: RenderOptions,
    layout: OnceCell<ViewerDocument>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("source_format", &self.source_format)
            .field("blocks", &self.state.model.body().len())
            .field("changed", &self.changed)
            .finish_non_exhaustive()
    }
}

impl Document {
    /// Opens a document from bytes, detecting the format.
    ///
    /// **O(document)**: one import.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Open`] when the bytes are not a document this build opens.
    pub fn open(bytes: &[u8]) -> Result<Self> {
        Self::open_with(bytes, &OpenOptions::default())
    }

    /// [`Document::open`] with options.
    ///
    /// # Errors
    ///
    /// As [`Document::open`].
    pub fn open_with(bytes: &[u8], options: &OpenOptions) -> Result<Self> {
        let selection = match options.format {
            Some(format) => {
                if !format.can_open() {
                    return Err(Error::new(
                        ErrorCode::UnsupportedFormat,
                        format!("this build does not open {format}"),
                    ));
                }
                FormatSelection::Explicit(
                    FormatId::new(format.registry_id())
                        .map_err(|e| Error::invalid(e.to_string()))?,
                )
            }
            None => FormatSelection::Auto,
        };
        let imported = registry()
            .import(
                DetectionRequest {
                    bytes,
                    selection,
                    file_name_hint: options.file_name.as_deref(),
                    mime_hint: None,
                },
                true,
            )
            .map_err(|e| Error::new(ErrorCode::Open, e.to_string()))?;
        let source_format = Format::from_registry_id(imported.source.format().as_str());
        let import = ImportReport::new(source_format, &imported.report, &imported.recovery);
        let model = imported.document;
        let mut ids = IdGenerator::new(IdSpace::local(IdSpace::of_document(model.id())).get());
        ids.reserve_through(model.highest_counter_in(IdSpace::new(ids.namespace())));
        let revisions = RevisionIdAllocator::from_document(&model);
        Ok(Self {
            state: EditState {
                model,
                ids,
                revisions,
            },
            resources: imported.resources,
            source: Some(imported.source),
            source_format,
            import,
            changed: false,
            author: options
                .author
                .clone()
                .unwrap_or_else(|| "OpenDoc".to_owned()),
            render: options.render.clone(),
            layout: OnceCell::new(),
        })
    }

    /// A new, empty document: one A4 section, Word's built-in styles
    /// (Normal, Title, Heading 1-6, List Bullet, List Number, Quote, Caption,
    /// Table Grid, ...), Calibri 11 pt.
    ///
    /// # Errors
    ///
    /// Never in practice; the blank package is the SDK's own.
    pub fn create() -> Result<Self> {
        Self::from_spec(&DocumentSpec::default(), &[])
    }

    /// Builds a document from a docx-js-shaped [`DocumentSpec`]; `media` holds
    /// the bytes its images index.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Build`] naming the first invalid value.
    pub fn from_spec(spec: &DocumentSpec, media: &[Vec<u8>]) -> Result<Self> {
        let bytes = build_docx(spec, media)?;
        Self::open_with(
            &bytes,
            &OpenOptions {
                format: Some(Format::Docx),
                ..OpenOptions::default()
            },
        )
    }

    /// [`Document::from_spec`] from the spec's JSON (what every binding sends).
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Build`] for JSON that is not a spec.
    pub fn from_spec_json(json: &str, media: &[Vec<u8>]) -> Result<Self> {
        let spec: DocumentSpec = serde_json::from_str(json)
            .map_err(|e| Error::new(ErrorCode::Build, format!("not a document spec: {e}")))?;
        Self::from_spec(&spec, media)
    }

    /// Sets the author review edits are attributed to.
    pub fn set_author(&mut self, author: impl Into<String>) {
        self.author = author.into();
    }

    /// Sets the render options (drops the cached layout).
    pub fn set_render_options(&mut self, render: RenderOptions) {
        self.render = render;
        self.layout = OnceCell::new();
    }

    /// The format the document was opened from.
    #[must_use]
    pub fn source_format(&self) -> Option<Format> {
        self.source_format
    }

    /// Whether the document changed since it was opened.
    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.changed
    }

    /// What the open did not carry into the model, and what it repaired.
    #[must_use]
    pub fn import_report(&self) -> &ImportReport {
        &self.import
    }

    // --- read ----------------------------------------------------------------

    /// The body's blocks. **O(body)**.
    #[must_use]
    pub fn body(&self) -> Vec<Block> {
        Projector::new(&self.state.model).blocks(self.state.model.body())
    }

    /// The body's top-level paragraphs (python-docx `document.paragraphs`).
    #[must_use]
    pub fn paragraphs(&self) -> Vec<ParagraphView> {
        self.body()
            .into_iter()
            .filter_map(|block| match block {
                Block::Paragraph(p) => Some(p),
                _ => None,
            })
            .collect()
    }

    /// The body's top-level tables (python-docx `document.tables`).
    #[must_use]
    pub fn tables(&self) -> Vec<TableView> {
        self.body()
            .into_iter()
            .filter_map(|block| match block {
                Block::Table(t) => Some(t),
                _ => None,
            })
            .collect()
    }

    /// The body's plain text, one line per paragraph; table cells
    /// tab-separated. **O(body)**.
    #[must_use]
    pub fn text(&self) -> String {
        crate::view::blocks_text(&self.body())
    }

    /// Sections (page setup).
    #[must_use]
    pub fn sections(&self) -> Vec<SectionView> {
        Projector::new(&self.state.model).sections()
    }

    /// Style definitions.
    #[must_use]
    pub fn styles(&self) -> Vec<StyleView> {
        Projector::new(&self.state.model).styles()
    }

    /// Headers and footers.
    #[must_use]
    pub fn headers_footers(&self) -> Vec<HeaderFooterView> {
        Projector::new(&self.state.model).headers_footers()
    }

    /// Footnotes and endnotes.
    #[must_use]
    pub fn notes(&self) -> Vec<NoteView> {
        Projector::new(&self.state.model).notes()
    }

    /// Every paragraph's text by id, on every surface, with the segments that
    /// map the review crate's anchor space onto it. **O(document)**.
    ///
    /// Review anchors are measured in the layout anchor stream, which counts a
    /// note reference's printed number, an equation's label and a no-break
    /// hyphen; the read view's `text` (the space edits index) counts none of
    /// them (`casual_doc_review::body::inline_anchor_len` records the
    /// disagreement). Mapping here keeps every offset the SDK hands out in one
    /// space.
    fn paragraph_spaces(&self) -> HashMap<NodeId, ParagraphSpace> {
        let notes = casual_doc_review::body::NoteAnchorLengths::of(&self.state.model);
        let mut spaces = HashMap::new();
        casual_doc_edit::visit_paragraphs_all_surfaces(&self.state.model, &mut |p| {
            let mut segments = Vec::new();
            flatten_segments(&notes, &p.inlines, &mut segments);
            spaces.insert(
                p.id,
                ParagraphSpace {
                    text: node_plain_text(&p.inlines),
                    segments,
                },
            );
        });
        spaces
    }

    fn anchor(
        spaces: &HashMap<NodeId, ParagraphSpace>,
        node: &str,
        start: u32,
        end: u32,
    ) -> AnchorView {
        let space = node.parse::<NodeId>().ok().and_then(|id| spaces.get(&id));
        let chars = |offset: u32| {
            let Some(space) = space else { return offset };
            let bytes = space.plain_offset(offset) as usize;
            space
                .text
                .get(..bytes.min(space.text.len()))
                .map_or(offset, |prefix| {
                    u32::try_from(prefix.chars().count()).unwrap_or(offset)
                })
        };
        AnchorView {
            paragraph: node.to_owned(),
            start: chars(start),
            end: chars(end),
        }
    }

    /// Comments, with their anchors (character offsets). **O(document)**.
    #[must_use]
    pub fn comments(&self) -> Vec<CommentView> {
        let texts = self.paragraph_spaces();
        build_review_comment_summaries(&self.state.model)
            .into_iter()
            .map(|c| CommentView {
                id: c.id,
                author: c.author,
                initials: c.initials,
                date: c.date,
                resolved: c.resolved,
                text: c.text,
                anchor: c
                    .anchor
                    .map(|a| Self::anchor(&texts, &a.node, a.start, a.end)),
                reply_to: c.parent_para_id,
            })
            .collect()
    }

    /// Tracked changes (insertions, deletions, moves, formatting).
    /// **O(document)**.
    #[must_use]
    pub fn revisions(&self) -> Vec<RevisionView> {
        let texts = self.paragraph_spaces();
        build_review_revision_summaries(&self.state.model)
            .into_iter()
            .map(|r| RevisionView {
                id: r.id,
                kind: r.kind,
                author: r.author,
                date: r.date,
                text: r.text,
                anchor: Self::anchor(&texts, &r.anchor.node, r.anchor.start, r.anchor.end),
            })
            .collect()
    }

    /// Media parts.
    #[must_use]
    pub fn media(&self) -> Vec<MediaView> {
        self.state
            .model
            .definitions()
            .media
            .iter()
            .map(|(id, reference)| MediaView {
                id: id.node_id().to_string(),
                part: reference.part_name.clone(),
                content_type: reference.media_type.clone(),
                bytes: self
                    .resources
                    .get(&reference.part_name)
                    .map_or(0, <[u8]>::len),
            })
            .collect()
    }

    /// A media part's bytes.
    #[must_use]
    pub fn media_bytes(&self, part: &str) -> Option<&[u8]> {
        self.resources.get(part)
    }

    /// Document properties.
    #[must_use]
    pub fn properties(&self) -> PropertiesView {
        let Some(props) = self.state.model.properties() else {
            return PropertiesView::default();
        };
        let core = &props.core;
        PropertiesView {
            title: core.title.clone(),
            subject: core.subject.clone(),
            creator: core.creator.clone(),
            keywords: core.keywords.clone(),
            description: core.description.clone(),
            last_modified_by: core.last_modified_by.clone(),
            revision: core.revision.clone(),
            created: core.created.clone(),
            modified: core.modified.clone(),
            category: core.category.clone(),
            custom: props
                .custom
                .iter()
                .map(|p| {
                    let value = match &p.value {
                        CustomValue::Text { value } => value.clone(),
                        other => serde_json::to_value(other)
                            .ok()
                            .and_then(|v| v.get("value").map(ToString::to_string))
                            .unwrap_or_default(),
                    };
                    (p.name.clone(), value)
                })
                .collect::<BTreeMap<_, _>>(),
        }
    }

    // --- modify --------------------------------------------------------------

    /// Applies a batch of edits, all or nothing, and returns the undo.
    ///
    /// The batch runs on a copy of the model (**O(document)** to copy), so a
    /// refusal anywhere leaves the document unchanged.
    ///
    /// # Errors
    ///
    /// The first edit's error; the document is unchanged.
    pub fn edit<T>(
        &mut self,
        batch: impl FnOnce(&mut Editor<'_>) -> Result<T>,
    ) -> Result<(T, Undo)> {
        let mut state = EditState {
            model: self.state.model.clone(),
            ids: self.state.ids.clone(),
            revisions: self.state.revisions.clone(),
        };
        let mut editor = Editor::new(&mut state, self.author.clone());
        let value = batch(&mut editor)?;
        let undo = editor.finish();
        if !undo.is_empty() {
            self.state = state;
            self.changed = true;
            self.layout = OnceCell::new();
        }
        Ok((value, undo))
    }

    /// Runs [`EditCommand`]s as one batch; returns each command's result.
    ///
    /// # Errors
    ///
    /// The first failing command's error, prefixed with its index.
    pub fn edit_commands(
        &mut self,
        commands: &[EditCommand],
    ) -> Result<(Vec<serde_json::Value>, Undo)> {
        self.edit(|editor| {
            commands
                .iter()
                .enumerate()
                .map(|(index, command)| {
                    editor
                        .run(command)
                        .map_err(|e| Error::new(e.code(), format!("edit {index}: {}", e.message())))
                })
                .collect()
        })
    }

    /// Applies an undo returned by [`Document::edit`]; returns the redo.
    ///
    /// # Errors
    ///
    /// When the document changed in a way the undo no longer fits.
    pub fn undo(&mut self, undo: &Undo) -> Result<Undo> {
        self.edit(|editor| editor.apply(undo.operations.clone()))
            .map(|((), redo)| redo)
    }

    /// Fills a docxtemplater-style template with `data` (see [`crate::template`]).
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Template`] listing every bad tag; the document is unchanged.
    pub fn render_template(
        &mut self,
        data: &serde_json::Value,
        options: &TemplateOptions,
    ) -> Result<()> {
        self.edit(|editor| template::fill(editor, data, options))
            .map(|_| ())
    }

    /// The tags a template uses: `(name, "value" | "loop" | "inverted")`.
    #[must_use]
    pub fn template_tags(&self, options: &TemplateOptions) -> Vec<(String, &'static str)> {
        template::tags(&self.state.model, options)
    }

    // --- save ----------------------------------------------------------------

    /// Saves to `format`, with a report of what was lost.
    ///
    /// The mode applies to a same-format save; a save to another format is
    /// semantic by nature, and its report says so. PDF is written from the
    /// same layout [`Document::render_page_png`] draws.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::UnsupportedFormat`] for a format this build does not save;
    /// [`ErrorCode::Save`] when the writer fails, or for [`SaveMode::Exact`] on
    /// a changed document or a different format.
    pub fn save(&self, format: Format, mode: SaveMode) -> Result<(Vec<u8>, SaveReport)> {
        if !format.can_save() {
            return Err(Error::new(
                ErrorCode::UnsupportedFormat,
                format!("this build does not save {format}"),
            ));
        }
        let same_format = Some(format) == self.source_format;
        if format == Format::Pdf {
            if mode == SaveMode::Exact {
                return Err(Error::new(
                    ErrorCode::Save,
                    "an exact save is to the source format only",
                ));
            }
            let bytes = self.to_pdf()?;
            let report = SaveReport::new(
                format,
                SaveMode::Semantic,
                &casual_doc_io::CompatibilityReport::default(),
                &self.import.findings,
            );
            return Ok((bytes, report));
        }
        let used = if same_format {
            mode
        } else {
            SaveMode::Semantic
        };
        if mode == SaveMode::Exact && (!same_format || self.changed) {
            return Err(Error::new(
                ErrorCode::Save,
                if same_format {
                    "an exact save needs an unchanged document; it was edited"
                } else {
                    "an exact save is to the source format only"
                },
            ));
        }
        let format_id =
            FormatId::new(format.registry_id()).map_err(|e| Error::invalid(e.to_string()))?;
        let artifact = registry()
            .export(
                &format_id,
                ExportRequest {
                    document: &self.state.model,
                    resources: &self.resources,
                    source: if same_format {
                        self.source.as_ref()
                    } else {
                        None
                    },
                    source_unchanged: !self.changed,
                    mode: match used {
                        SaveMode::Exact => ExportMode::ExactIfUnchanged,
                        SaveMode::PreserveWhenSafe => ExportMode::PreserveWhenSafe,
                        SaveMode::Semantic => ExportMode::Semantic,
                    },
                },
            )
            .map_err(|e| Error::new(ErrorCode::Save, e.to_string()))?;
        let report = SaveReport::new(format, used, &artifact.report, &self.import.findings);
        Ok((artifact.bytes, report))
    }

    // --- layout and render -----------------------------------------------------

    fn layout(&self) -> &ViewerDocument {
        self.layout.get_or_init(|| {
            let options = self
                .render
                .fonts
                .iter()
                .cloned()
                .fold(ViewerOptions::default(), ViewerOptions::with_font)
                .with_system_fonts(self.render.system_fonts);
            ViewerDocument::from_document(
                self.state.model.clone(),
                self.resources.as_map().clone(),
                &options,
            )
        })
    }

    /// The number of laid-out pages. **O(document)** the first time after an
    /// edit (one pagination), O(1) after.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.layout().page_count()
    }

    /// Every page's size.
    #[must_use]
    pub fn page_sizes(&self) -> Vec<PageSize> {
        self.layout()
            .page_sizes_twips()
            .into_iter()
            .map(|(w, h)| PageSize {
                width_pt: f64::from(w) / 20.0,
                height_pt: f64::from(h) / 20.0,
            })
            .collect()
    }

    /// The text on one page, as laid out.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidArgument`] past the last page.
    pub fn page_text(&self, index: usize) -> Result<String> {
        self.layout()
            .page_text(index)
            .map_err(|e| Error::invalid(e.to_string()))
    }

    /// Draws one page to RGBA at `dpi`.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidArgument`] past the last page or for a bad DPI.
    pub fn render_page(&self, index: usize, dpi: f32) -> Result<PageImage> {
        let bitmap = self
            .layout()
            .render_page(index, dpi)
            .map_err(|e| Error::new(ErrorCode::Render, e.to_string()))?;
        Ok(PageImage {
            width: bitmap.width_px,
            height: bitmap.height_px,
            rgba: bitmap.rgba,
        })
    }

    /// Draws one page to PNG at `dpi`.
    ///
    /// # Errors
    ///
    /// As [`Document::render_page`].
    pub fn render_page_png(&self, index: usize, dpi: f32) -> Result<Vec<u8>> {
        self.layout()
            .render_page_png(index, dpi)
            .map_err(|e| Error::new(ErrorCode::Render, e.to_string()))
    }

    /// A real-text vector PDF of the current state, from the same layout.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Render`] when a face cannot be embedded or past the PDF
    /// page ceiling.
    pub fn to_pdf(&self) -> Result<Vec<u8>> {
        self.layout()
            .to_pdf()
            .map_err(|e| Error::new(ErrorCode::Render, e.to_string()))
    }

    /// The level, text and page of each body heading.
    #[must_use]
    pub fn outline(&self) -> Vec<(u8, String, Option<usize>)> {
        self.layout()
            .outline()
            .into_iter()
            .map(|entry| (entry.level, entry.text, entry.page))
            .collect()
    }

    /// Font families the document asked for that layout had to substitute.
    #[must_use]
    pub fn font_needs(&self) -> Vec<String> {
        self.layout()
            .font_needs()
            .iter()
            .map(|need| need.requested.clone())
            .collect()
    }

    /// The model, for crate-internal use (the C ABI's digest, tests).
    #[allow(dead_code)]
    pub(crate) fn model(&self) -> &Model {
        &self.state.model
    }
}

/// A paragraph's text and its anchor-to-text segments.
struct ParagraphSpace {
    text: String,
    /// `(anchor bytes, text bytes)` per leaf, in order.
    segments: Vec<(u32, u32)>,
}

impl ParagraphSpace {
    /// The text byte offset an anchor byte offset lands on. Inside a segment
    /// both spaces measure alike, the offset moves with it; inside one only
    /// the anchor space counts (a note number), it clamps to the segment's
    /// start. **O(inlines)**.
    fn plain_offset(&self, anchor: u32) -> u32 {
        let (mut a, mut t) = (0_u32, 0_u32);
        for &(anchor_len, text_len) in &self.segments {
            if anchor < a + anchor_len || (anchor == a + anchor_len && anchor_len == text_len) {
                let inside = anchor - a;
                return if anchor_len == text_len {
                    t + inside
                } else {
                    t + inside.min(text_len)
                };
            }
            a += anchor_len;
            t += text_len;
        }
        t + anchor.saturating_sub(a)
    }
}

/// Leaf segments of a paragraph's inlines, through the wrappers both spaces
/// descend (links, visible revisions, content controls).
fn flatten_segments(
    notes: &casual_doc_review::body::NoteAnchorLengths,
    inlines: &[casual_doc_model::v1::InlineNode],
    out: &mut Vec<(u32, u32)>,
) {
    use casual_doc_model::v1::{InlineNode, ReviewProjection};
    for inline in inlines {
        match inline {
            InlineNode::Hyperlink(link) => flatten_segments(notes, &link.inlines, out),
            InlineNode::Sdt(sdt) => flatten_segments(notes, &sdt.inlines, out),
            InlineNode::Revision(revision)
                if revision
                    .kind
                    .contributes_to(ReviewProjection::FinalWithMarkup) =>
            {
                flatten_segments(notes, &revision.inlines, out);
            }
            other => out.push((
                casual_doc_review::body::inline_anchor_len(notes, other),
                casual_doc_edit::inline_text_len(other),
            )),
        }
    }
}
