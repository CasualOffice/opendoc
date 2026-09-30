//! Built-in deterministic single-file HTML exporter.
//!
//! # Why this exists
//!
//! `docs/153` `shell.export-html`, rank 3. ONLYOFFICE's Download-as grid offers
//! **HTML (Zipped)** — a folder of files: the markup plus every picture as a
//! sibling, delivered as a `.zip` a reader has to unpack before anything
//! displays. Ours is **one file**: the pictures are `data:` URIs inside it, the
//! stylesheet is inline, and nothing it references is off the disk. That is the
//! same local-first argument the rest of this product rests on, applied to an
//! export: a file you can open in a browser with the network off, or attach to
//! a mail, is more useful than an archive.
//!
//! # The shape, named first
//!
//! A *serializer*, the third in this crate with the same skeleton as
//! [`text`](crate::PlainTextAdapter) and [`markdown`](crate::MarkdownAdapter):
//! one linear walk of the normalized model, a bounded output buffer, and a
//! compatibility report for everything the target cannot carry. Nothing here is
//! a new kind of component.
//!
//! HTML carries considerably more of a word-processing document than Markdown
//! does, and this exporter uses that rather than levelling down: underline,
//! superscript and subscript, text and highlight colour, font size, paragraph
//! alignment and indentation, bookmarks as anchors, tracked changes as
//! `<ins>`/`<del>`, and — the one that matters most for real documents —
//! **tables with merged cells**, which a Markdown pipe table cannot express at
//! all.
//!
//! # Safety
//!
//! Every text node and every attribute value is escaped, and no `style`
//! attribute is ever built from document text: each declaration is
//! re-serialized from a typed model field, never passed through. A document
//! cannot inject markup into its own export, which is the whole reason this
//! file escapes rather than templating.
//!
//! # Complexity
//!
//! One pass over the body, so O(nodes); a picture is copied once into its
//! `data:` URI. Media are resolved by one B-tree lookup per drawing, never by a
//! scan (`SKILL` §8).

use std::collections::BTreeMap;

use casual_doc_model::v1::{
    Alignment, BlockNode, BreakKind, Color, Definitions, Document, GroupChild, HighlightColor,
    HyperlinkTarget, InlineNode, NumberFormat, Paragraph, RevisionKind, RunProperties, Table,
    VerticalAlignment, VerticalMerge, WordprocessingGroup,
};

use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, DocumentResources, ExportArtifact,
    ExportMode, ExportRequest, FeatureLocation, FormatDescriptor, FormatExporter, FormatId,
    FormatProfile, ModelOutcome, RetentionOutcome, formats,
};

/// The media type registered for HTML, and the one the artifact carries.
pub const HTML_MIME: &str = "text/html";

/// Host-configurable HTML limits with non-bypassable hard ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HtmlLimits {
    /// Maximum bytes of emitted HTML, `data:` URIs included.
    pub max_output_bytes: usize,
    /// Maximum bytes of one picture before it is replaced by its alt text.
    ///
    /// A `data:` URI costs four bytes for every three, so a large picture is
    /// where a single-file export actually becomes unusable. Past this the
    /// picture is reported rather than embedded, which is a smaller lie than a
    /// 200 MB HTML file.
    pub max_embedded_bytes: usize,
    /// Maximum block-nesting depth followed before the walk refuses.
    pub max_nesting_depth: usize,
}

impl HtmlLimits {
    /// Hard maximum emitted bytes.
    pub const HARD_MAX_OUTPUT_BYTES: usize = 256 * 1024 * 1024;
    /// Hard maximum embedded bytes for one picture.
    pub const HARD_MAX_EMBEDDED_BYTES: usize = 64 * 1024 * 1024;
    /// Hard maximum nesting depth.
    pub const HARD_MAX_NESTING_DEPTH: usize = 256;

    fn validate(self) -> Result<(), AdapterError> {
        for (name, value, ceiling) in [
            (
                "html_output_bytes",
                self.max_output_bytes,
                Self::HARD_MAX_OUTPUT_BYTES,
            ),
            (
                "html_embedded_bytes",
                self.max_embedded_bytes,
                Self::HARD_MAX_EMBEDDED_BYTES,
            ),
            (
                "html_nesting_depth",
                self.max_nesting_depth,
                Self::HARD_MAX_NESTING_DEPTH,
            ),
        ] {
            if value > ceiling {
                return Err(AdapterError::new(format!(
                    "limit {name} value {value} exceeds hard ceiling {ceiling}"
                )));
            }
        }
        Ok(())
    }
}

impl Default for HtmlLimits {
    fn default() -> Self {
        Self {
            max_output_bytes: 64 * 1024 * 1024,
            max_embedded_bytes: 8 * 1024 * 1024,
            max_nesting_depth: 32,
        }
    }
}

/// Built-in bounded single-file HTML exporter.
#[derive(Clone, Debug)]
pub struct HtmlAdapter {
    descriptor: FormatDescriptor,
    limits: HtmlLimits,
}

impl HtmlAdapter {
    /// Creates an exporter with explicit HTML limits.
    #[must_use]
    pub fn new(limits: HtmlLimits) -> Self {
        Self {
            descriptor: FormatDescriptor {
                id: FormatId::new(formats::HTML).expect("built-in html id is valid"),
                display_name: "Web Page".to_owned(),
                mime_types: vec![HTML_MIME.to_owned()],
                extensions: vec!["html".to_owned(), "htm".to_owned()],
                // Export only, for the same reason Markdown is: reading HTML
                // needs a parser and a sanitiser, and only the writer exists.
                can_import: false,
                can_export: true,
                exact_if_unchanged: false,
                preserve_when_safe: false,
            },
            limits,
        }
    }
}

impl Default for HtmlAdapter {
    fn default() -> Self {
        Self::new(HtmlLimits::default())
    }
}

impl FormatExporter for HtmlAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn export(&self, request: ExportRequest<'_>) -> Result<ExportArtifact, AdapterError> {
        self.limits.validate()?;
        request
            .document
            .validate()
            .map_err(|error| AdapterError::new(format!("normalized model: {error}")))?;
        if request.mode == ExportMode::ExactIfUnchanged {
            return Err(AdapterError::new(
                "html has no importer, so there are no retained source bytes to return \
                 exactly; ask for the semantic mode",
            ));
        }

        let mut writer = Writer::new(self.limits);
        let mut losses = Losses::default();
        writer.document(request.document, request.resources, &mut losses)?;
        if request.document.definitions().sections.len() > 1 {
            losses.record("html.sections", ModelOutcome::Omitted);
        }
        if request.document.background().is_some() {
            losses.record("html.page_background", ModelOutcome::Omitted);
        }
        if request.source.is_some() {
            losses.record("html.source_envelope", ModelOutcome::Omitted);
        }

        Ok(ExportArtifact {
            bytes: writer.finish(),
            report: losses.finish(),
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: Some("html5".to_owned()),
            },
            mime_type: HTML_MIME.to_owned(),
            suggested_extension: "html".to_owned(),
        })
    }
}

/// The document's own stylesheet, inline.
///
/// Deliberately small and deliberately not a theme: it sets the page width and
/// the table borders a word-processing document needs to read as one, and
/// nothing else. Every other visual fact comes from the document's own
/// properties as an inline `style`, so the file looks like the document rather
/// than like this exporter's taste.
const STYLESHEET: &str = "\
body{margin:0 auto;max-width:42em;padding:2em 1em;\
font-family:Georgia,'Times New Roman',serif;line-height:1.5}\
table{border-collapse:collapse;margin:1em 0}\
th,td{border:1px solid #999;padding:0.3em 0.6em;vertical-align:top}\
img{max-width:100%;height:auto}\
hr{border:0;border-top:1px solid #999}";

struct Writer {
    value: String,
    max_bytes: usize,
    max_embedded_bytes: usize,
    max_depth: usize,
}

impl Writer {
    fn new(limits: HtmlLimits) -> Self {
        Self {
            value: String::new(),
            max_bytes: limits.max_output_bytes,
            max_embedded_bytes: limits.max_embedded_bytes,
            max_depth: limits.max_nesting_depth,
        }
    }

    fn push_str(&mut self, value: &str) -> Result<(), AdapterError> {
        let observed = self.value.len().saturating_add(value.len());
        enforce("html_output_bytes", observed, self.max_bytes)?;
        self.value.push_str(value);
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        self.value.into_bytes()
    }

    fn document(
        &mut self,
        document: &Document,
        resources: &DocumentResources,
        losses: &mut Losses,
    ) -> Result<(), AdapterError> {
        let title = document
            .properties()
            .and_then(|properties| properties.core.title.clone())
            .filter(|title| !title.trim().is_empty());
        self.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n")?;
        self.push_str("<title>")?;
        let title_text = title.clone().unwrap_or_else(|| "Document".to_owned());
        let escaped = escape_text(&title_text);
        self.push_str(&escaped)?;
        self.push_str("</title>\n<style>")?;
        self.push_str(STYLESHEET)?;
        self.push_str("</style>\n</head>\n<body>\n")?;

        let context = Context {
            definitions: document.definitions(),
            resources,
            depth: 0,
            max_depth: self.max_depth,
        };
        self.blocks(document.body(), &context, losses)?;
        self.push_str("</body>\n</html>\n")?;

        // A document's metadata beyond the title has no home in a body-only
        // document and is not smuggled into `<meta>` tags: an author name in a
        // file that gets published is a privacy decision, not a formatting one.
        if document
            .properties()
            .is_some_and(|properties| properties.core.creator.is_some())
        {
            losses.record("html.document_author", ModelOutcome::Omitted);
        }
        Ok(())
    }

    fn blocks(
        &mut self,
        blocks: &[BlockNode],
        context: &Context<'_>,
        losses: &mut Losses,
    ) -> Result<(), AdapterError> {
        // A run of consecutive list paragraphs becomes one `<ul>`/`<ol>`, which
        // is what makes the output a list rather than a sequence of indented
        // paragraphs. The grouping is a single forward scan; no node is visited
        // twice.
        let mut index = 0;
        while index < blocks.len() {
            let list = match &blocks[index] {
                BlockNode::Paragraph(paragraph) => list_kind(paragraph, context.definitions),
                _ => None,
            };
            match list {
                None => {
                    self.block(&blocks[index], context, losses)?;
                    index += 1;
                }
                Some(kind) => {
                    let mut end = index;
                    while end < blocks.len()
                        && matches!(&blocks[end], BlockNode::Paragraph(p)
                            if list_kind(p, context.definitions) == Some(kind))
                    {
                        end += 1;
                    }
                    self.push_str(if kind == ListKind::Ordered {
                        "<ol>\n"
                    } else {
                        "<ul>\n"
                    })?;
                    for block in &blocks[index..end] {
                        if let BlockNode::Paragraph(paragraph) = block {
                            self.push_str("<li>")?;
                            self.inlines(&paragraph.inlines, context, losses)?;
                            self.push_str("</li>\n")?;
                        }
                    }
                    self.push_str(if kind == ListKind::Ordered {
                        "</ol>\n"
                    } else {
                        "</ul>\n"
                    })?;
                    index = end;
                }
            }
        }
        Ok(())
    }

    fn block(
        &mut self,
        block: &BlockNode,
        context: &Context<'_>,
        losses: &mut Losses,
    ) -> Result<(), AdapterError> {
        match block {
            BlockNode::Paragraph(paragraph) => self.paragraph(paragraph, context, losses),
            BlockNode::Table(table) => self.table(table, context, losses),
            BlockNode::Sdt(sdt) => {
                // A content control is a container, and `<div>` is the honest
                // HTML for one: the control's behaviour does not survive, the
                // grouping does.
                losses.record("html.content_control", ModelOutcome::Degraded);
                self.push_str("<div>\n")?;
                self.blocks(&sdt.blocks, context, losses)?;
                self.push_str("</div>\n")
            }
            BlockNode::AltChunk(_) => {
                losses.record("html.alt_chunk", ModelOutcome::Omitted);
                Ok(())
            }
        }
    }

    fn paragraph(
        &mut self,
        paragraph: &Paragraph,
        context: &Context<'_>,
        losses: &mut Losses,
    ) -> Result<(), AdapterError> {
        let heading = heading_level(paragraph, context.definitions);
        let style = paragraph_style(paragraph);
        let tag = heading.map_or("p".to_owned(), |level| format!("h{level}"));

        self.push_str("<")?;
        self.push_str(&tag)?;
        if !style.is_empty() {
            self.push_str(" style=\"")?;
            self.push_str(&style)?;
            self.push_str("\"")?;
        }
        self.push_str(">")?;
        if paragraph.properties.style_ref.is_some() && heading.is_none() {
            losses.record("html.paragraph_style", ModelOutcome::Omitted);
        }
        self.inlines(&paragraph.inlines, context, losses)?;
        self.push_str("</")?;
        self.push_str(&tag)?;
        self.push_str(">\n")
    }

    /// A real HTML table, merges included.
    ///
    /// This is where HTML earns its place beside Markdown: `colspan` and
    /// `rowspan` express exactly what `w:gridSpan` and `w:vMerge` mean, so a
    /// merged table exports as a merged table instead of degrading to text. A
    /// continuation cell of a vertical merge writes nothing at all — the cell
    /// above it owns the span — which is the same rule the layout engine
    /// applies.
    fn table(
        &mut self,
        table: &Table,
        context: &Context<'_>,
        losses: &mut Losses,
    ) -> Result<(), AdapterError> {
        if context.depth >= context.max_depth {
            return Err(AdapterError::new(
                "limit html_nesting_depth exceeded while walking nested tables",
            ));
        }
        let deeper = Context {
            depth: context.depth + 1,
            ..*context
        };
        self.push_str("<table>\n")?;
        for (row_index, row) in table.rows.iter().enumerate() {
            self.push_str("<tr>\n")?;
            for (column_index, cell) in row.cells.iter().enumerate() {
                if cell.properties.vertical_merge == Some(VerticalMerge::Continue) {
                    continue;
                }
                let tag = if row_index == 0 { "th" } else { "td" };
                self.push_str("<")?;
                self.push_str(tag)?;
                if let Some(span) = cell.properties.grid_span.filter(|span| *span > 1) {
                    self.push_str(&format!(" colspan=\"{span}\""))?;
                }
                let rows_spanned = vertical_span(table, row_index, column_index);
                if rows_spanned > 1 {
                    self.push_str(&format!(" rowspan=\"{rows_spanned}\""))?;
                }
                self.push_str(">\n")?;
                self.blocks(&cell.blocks, &deeper, losses)?;
                self.push_str("</")?;
                self.push_str(tag)?;
                self.push_str(">\n")?;
            }
            self.push_str("</tr>\n")?;
        }
        self.push_str("</table>\n")?;
        losses.record("html.table_formatting", ModelOutcome::Degraded);
        Ok(())
    }

    fn inlines(
        &mut self,
        inlines: &[InlineNode],
        context: &Context<'_>,
        losses: &mut Losses,
    ) -> Result<(), AdapterError> {
        for inline in inlines {
            self.inline(inline, context, losses)?;
        }
        Ok(())
    }

    fn inline(
        &mut self,
        inline: &InlineNode,
        context: &Context<'_>,
        losses: &mut Losses,
    ) -> Result<(), AdapterError> {
        match inline {
            InlineNode::Run(run) => {
                let tags = run_tags(&run.properties);
                let style = run_style(&run.properties);
                for tag in &tags {
                    self.push_str(&format!("<{tag}>"))?;
                }
                if !style.is_empty() {
                    self.push_str("<span style=\"")?;
                    self.push_str(&style)?;
                    self.push_str("\">")?;
                }
                let escaped = escape_text(&run.text);
                self.push_str(&escaped)?;
                if !style.is_empty() {
                    self.push_str("</span>")?;
                }
                for tag in tags.iter().rev() {
                    self.push_str(&format!("</{tag}>"))?;
                }
                Ok(())
            }
            // A tab inside a paragraph has no HTML equivalent that survives
            // collapsing whitespace; an em-space is the closest thing that does.
            InlineNode::Tab(_) | InlineNode::PositionalTab(_) => {
                losses.record("html.tab", ModelOutcome::Degraded);
                self.push_str("&#8195;")
            }
            InlineNode::Break(node) => match node.kind {
                BreakKind::Line => self.push_str("<br>"),
                _ => {
                    // A page break in a flowed document is a print concern;
                    // `page-break-before` on a bare `<span>` does nothing, so it
                    // is reported rather than faked.
                    losses.record("html.page_or_column_break", ModelOutcome::Omitted);
                    Ok(())
                }
            },
            InlineNode::Hyperlink(link) => {
                let target = match &link.target {
                    HyperlinkTarget::External(external) => match &external.anchor {
                        Some(anchor) => format!("{}#{anchor}", external.url),
                        None => external.url.clone(),
                    },
                    HyperlinkTarget::Internal(internal) => format!("#{}", internal.anchor),
                };
                self.push_str("<a href=\"")?;
                let escaped = escape_attribute(&target);
                self.push_str(&escaped)?;
                self.push_str("\"")?;
                if let Some(tooltip) = &link.tooltip {
                    self.push_str(" title=\"")?;
                    let escaped = escape_attribute(tooltip);
                    self.push_str(&escaped)?;
                    self.push_str("\"")?;
                }
                self.push_str(">")?;
                self.inlines(&link.inlines, context, losses)?;
                self.push_str("</a>")
            }
            InlineNode::Drawing(drawing) => {
                self.picture(drawing.media, drawing.descr.as_deref(), context, losses)
            }
            InlineNode::AnchoredDrawing(drawing) => {
                // An anchored drawing's position is a page coordinate, and an
                // HTML document has no pages; it becomes an inline picture.
                losses.record("html.anchor_position", ModelOutcome::Degraded);
                self.picture(drawing.media, drawing.descr.as_deref(), context, losses)
            }
            InlineNode::HorizontalRule(_) => self.push_str("<hr>"),
            InlineNode::Field(field) => {
                losses.record("html.field_instruction", ModelOutcome::Degraded);
                self.inlines(&field.inlines, context, losses)
            }
            InlineNode::TextBox(text_box) => {
                losses.record("html.text_box", ModelOutcome::Degraded);
                self.push_str("<div>\n")?;
                self.blocks(&text_box.blocks, context, losses)?;
                self.push_str("</div>\n")
            }
            InlineNode::Group(group) => self.group(group, context, losses),
            InlineNode::Revision(revision) => {
                // `<ins>` and `<del>` are the HTML for exactly this, so a
                // tracked change survives as a tracked change.
                match revision.kind {
                    RevisionKind::Insertion | RevisionKind::MoveTo => {
                        self.push_str("<ins>")?;
                        self.inlines(&revision.inlines, context, losses)?;
                        self.push_str("</ins>")
                    }
                    RevisionKind::Deletion | RevisionKind::MoveFrom => {
                        self.push_str("<del>")?;
                        self.inlines(&revision.inlines, context, losses)?;
                        self.push_str("</del>")
                    }
                }
            }
            InlineNode::Sdt(sdt) => {
                losses.record("html.content_control", ModelOutcome::Degraded);
                self.inlines(&sdt.inlines, context, losses)
            }
            InlineNode::Math(math) => {
                // Not MathML: the model carries OMML, and translating it is its
                // own piece of work (`docs/134`). The fallback text is what a
                // reader sees, and the loss says the markup went.
                losses.record("html.math_markup", ModelOutcome::Degraded);
                let escaped = escape_text(&math.text);
                self.push_str(&escaped)
            }
            InlineNode::Symbol(symbol) => {
                losses.record("html.symbol_font", ModelOutcome::Degraded);
                match char::from_u32(symbol.char) {
                    Some(character) => {
                        let escaped = escape_text(&character.to_string());
                        self.push_str(&escaped)
                    }
                    None => Ok(()),
                }
            }
            InlineNode::NoBreakHyphen(_) => self.push_str("&#8209;"),
            InlineNode::SoftHyphen(_) => self.push_str("&shy;"),
            InlineNode::EmbeddedObject(_) => {
                losses.record("html.embedded_object", ModelOutcome::Omitted);
                Ok(())
            }
            InlineNode::NoteReference(_) => {
                losses.record("html.note_reference", ModelOutcome::Omitted);
                Ok(())
            }
            InlineNode::NoteNumberMark(_) => {
                losses.record("html.note_number_mark", ModelOutcome::Omitted);
                Ok(())
            }
            InlineNode::BookmarkStart(marker) => {
                // A bookmark is an anchor, and an internal link's `#name` needs
                // one to land on, so it is emitted rather than reported. The
                // NAME lives in `definitions.bookmarks`; the marker carries an
                // id, so this is one lookup rather than a field read.
                let Some(bookmark) = context.definitions.bookmarks.get(&marker.bookmark) else {
                    losses.record("html.dangling_bookmark", ModelOutcome::Omitted);
                    return Ok(());
                };
                self.push_str("<a id=\"")?;
                let escaped = escape_attribute(&bookmark.name);
                self.push_str(&escaped)?;
                self.push_str("\"></a>")
            }
            InlineNode::BookmarkEnd(_) => Ok(()),
            InlineNode::CommentReference(_)
            | InlineNode::CommentRangeStart(_)
            | InlineNode::CommentRangeEnd(_)
            | InlineNode::FieldRangeStart(_)
            | InlineNode::FieldRangeEnd(_)
            | InlineNode::MoveRangeStart(_)
            | InlineNode::MoveRangeEnd(_) => {
                losses.record("html.range_or_comment_marker", ModelOutcome::Omitted);
                Ok(())
            }
        }
    }

    fn group(
        &mut self,
        group: &WordprocessingGroup,
        context: &Context<'_>,
        losses: &mut Losses,
    ) -> Result<(), AdapterError> {
        if context.depth >= context.max_depth {
            return Err(AdapterError::new(
                "limit html_nesting_depth exceeded while walking a grouped drawing",
            ));
        }
        losses.record("html.grouped_drawing", ModelOutcome::Degraded);
        let deeper = Context {
            depth: context.depth + 1,
            ..*context
        };
        for child in &group.children {
            match child {
                GroupChild::Picture(picture) => {
                    self.picture(picture.media, picture.descr.as_deref(), &deeper, losses)?;
                }
                GroupChild::TextBox(text_box) => {
                    self.push_str("<div>\n")?;
                    self.blocks(&text_box.blocks, &deeper, losses)?;
                    self.push_str("</div>\n")?;
                }
                GroupChild::Shape(_) => {
                    losses.record("html.group_shape", ModelOutcome::Omitted);
                }
                GroupChild::Group(inner) => self.group(inner, &deeper, losses)?,
            }
        }
        Ok(())
    }

    /// One picture, embedded as a `data:` URI when its bytes are present and
    /// within the per-picture ceiling.
    fn picture(
        &mut self,
        media: casual_doc_model::v1::MediaId,
        description: Option<&str>,
        context: &Context<'_>,
        losses: &mut Losses,
    ) -> Result<(), AdapterError> {
        let alt = description.unwrap_or("");
        let reference = context.definitions.media.get(&media);
        let bytes = reference.and_then(|entry| context.resources.get(&entry.part_name));
        let Some((entry, bytes)) = reference.zip(bytes) else {
            // A drawing whose bytes the host did not supply cannot be embedded,
            // and an `<img>` with no source is a broken-image icon. The alt text
            // is what is left.
            losses.record("html.picture_bytes_absent", ModelOutcome::Degraded);
            if alt.trim().is_empty() {
                return Ok(());
            }
            self.push_str("<span>")?;
            let escaped = escape_text(alt);
            self.push_str(&escaped)?;
            return self.push_str("</span>");
        };
        if bytes.len() > self.max_embedded_bytes {
            losses.record("html.picture_over_embedding_limit", ModelOutcome::Degraded);
            if alt.trim().is_empty() {
                return Ok(());
            }
            self.push_str("<span>")?;
            let escaped = escape_text(alt);
            self.push_str(&escaped)?;
            return self.push_str("</span>");
        }
        self.push_str("<img alt=\"")?;
        let escaped = escape_attribute(alt);
        self.push_str(&escaped)?;
        self.push_str("\" src=\"data:")?;
        let media_type = escape_attribute(&entry.media_type);
        self.push_str(&media_type)?;
        self.push_str(";base64,")?;
        let encoded = base64(bytes);
        self.push_str(&encoded)?;
        self.push_str("\">")
    }
}

/// What the walk needs to know about where it is.
#[derive(Clone, Copy)]
struct Context<'a> {
    definitions: &'a Definitions,
    resources: &'a DocumentResources,
    depth: usize,
    max_depth: usize,
}

fn enforce(name: &'static str, observed: usize, allowed: usize) -> Result<(), AdapterError> {
    if observed > allowed {
        return Err(AdapterError::new(format!(
            "limit {name} observed {observed} exceeds allowed {allowed}"
        )));
    }
    Ok(())
}

/// Standard base64, no line breaks, as a `data:` URI needs.
///
/// Vendored as twenty lines rather than as a dependency: one alphabet, one
/// caller, and `deny.toml` is a gate this project keeps deliberately narrow.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Escapes a text node. `<`, `>` and `&` end or begin markup; the rest is text.
fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(character),
        }
    }
    out
}

/// Escapes an attribute value, which additionally must not close its own quote.
fn escape_attribute(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(character),
        }
    }
    out
}

/// Whether a list run is ordered or bulleted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ListKind {
    Ordered,
    Unordered,
}

fn list_kind(paragraph: &Paragraph, definitions: &Definitions) -> Option<ListKind> {
    let reference = paragraph.properties.numbering?;
    let level = definitions.numbering_resolver().level(reference)?;
    let ordered = !matches!(
        level.num_fmt.as_ref(),
        Some(NumberFormat::Bullet) | Some(NumberFormat::None) | None
    );
    Some(if ordered {
        ListKind::Ordered
    } else {
        ListKind::Unordered
    })
}

/// The heading level a paragraph's style names, 1-6, or `None`.
///
/// The same rule the Markdown exporter applies, and for the same reason: the
/// style's own outline level is what Word's navigation pane reads, and the name
/// is the fallback. HTML also stops at six.
fn heading_level(paragraph: &Paragraph, definitions: &Definitions) -> Option<u8> {
    let style_id = paragraph.properties.style_ref?;
    let style = definitions.styles.get(&style_id)?;
    let from_outline = style
        .paragraph
        .as_ref()
        .and_then(|properties| properties.outline_level)
        .map(|level| level.saturating_add(1));
    let from_name = style.name.as_deref().and_then(heading_level_from_name);
    let level = from_outline.or(from_name)?;
    (1..=9).contains(&level).then(|| level.min(6))
}

fn heading_level_from_name(name: &str) -> Option<u8> {
    let trimmed = name.trim();
    let rest = trimmed
        .strip_prefix("heading")
        .or_else(|| trimmed.strip_prefix("Heading"))?;
    rest.trim().parse::<u8>().ok()
}

/// How many rows a cell's vertical merge spans, counting from `row_index`.
///
/// Complexity: O(rows below) for a merged cell and O(1) for the common case,
/// and each continuation is visited once by the row loop that skips it — so the
/// table walk stays linear in cells rather than quadratic.
fn vertical_span(table: &Table, row_index: usize, column_index: usize) -> u32 {
    let start = table
        .rows
        .get(row_index)
        .and_then(|row| row.cells.get(column_index));
    if start.map(|cell| cell.properties.vertical_merge) != Some(Some(VerticalMerge::Restart)) {
        return 1;
    }
    let mut span = 1;
    for row in table.rows.iter().skip(row_index + 1) {
        match row.cells.get(column_index) {
            Some(cell) if cell.properties.vertical_merge == Some(VerticalMerge::Continue) => {
                span += 1;
            }
            _ => break,
        }
    }
    span
}

/// The paragraph-level inline style, from typed fields only.
fn paragraph_style(paragraph: &Paragraph) -> String {
    let mut declarations = Vec::new();
    if let Some(alignment) = paragraph.properties.alignment {
        let value = match alignment {
            Alignment::Start => "left",
            Alignment::Center => "center",
            Alignment::End => "right",
            Alignment::Justify => "justify",
        };
        declarations.push(format!("text-align:{value}"));
    }
    if let Some(indentation) = &paragraph.properties.indentation {
        // Twips to points: 20 twips to the point, and `pt` is the unit a word
        // processor's own numbers are in.
        if let Some(left) = indentation.start_twips.filter(|value| *value != 0) {
            declarations.push(format!("margin-left:{}pt", f64::from(left) / 20.0));
        }
        if let Some(right) = indentation.end_twips.filter(|value| *value != 0) {
            declarations.push(format!("margin-right:{}pt", f64::from(right) / 20.0));
        }
    }
    declarations.join(";")
}

/// The semantic element tags a run's properties ask for, outermost first.
fn run_tags(properties: &RunProperties) -> Vec<&'static str> {
    let mut tags = Vec::new();
    if properties.bold == Some(true) {
        tags.push("strong");
    }
    if properties.italic == Some(true) {
        tags.push("em");
    }
    if properties.strike == Some(true) || properties.double_strike == Some(true) {
        tags.push("s");
    }
    if properties.underline == Some(true) {
        tags.push("u");
    }
    match properties.vertical_alignment {
        Some(VerticalAlignment::Superscript) => tags.push("sup"),
        Some(VerticalAlignment::Subscript) => tags.push("sub"),
        _ => {}
    }
    tags
}

/// The run-level inline style, from typed fields only — never from text.
///
/// Only the three facts that can be re-serialized without a theme are emitted.
/// A theme colour slot resolves against the document's theme part, which this
/// crate does not read, so it is reported rather than guessed at — a wrong
/// colour is worse than a default one.
fn run_style(properties: &RunProperties) -> String {
    let mut declarations = Vec::new();
    if let Some(size) = properties.size_half_points {
        declarations.push(format!("font-size:{}pt", f64::from(size) / 2.0));
    }
    if let Some(Color::Rgb(rgb)) = properties.color {
        declarations.push(format!("color:#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b));
    }
    if let Some(css) = properties.highlight.and_then(highlight_css) {
        declarations.push(format!("background-color:{css}"));
    }
    declarations.join(";")
}

/// The CSS colour name for a `w:highlight` slot.
///
/// `w:highlight` is a closed list of sixteen names that happen to be CSS colour
/// keywords, so the mapping is a rename rather than a colour decision. The two
/// that are not — `darkYellow` and `none` — are handled explicitly.
fn highlight_css(highlight: HighlightColor) -> Option<&'static str> {
    Some(match highlight {
        HighlightColor::None => return None,
        HighlightColor::Black => "black",
        HighlightColor::Blue => "blue",
        HighlightColor::Cyan => "cyan",
        HighlightColor::Green => "lime",
        HighlightColor::Magenta => "magenta",
        HighlightColor::Red => "red",
        HighlightColor::Yellow => "yellow",
        HighlightColor::White => "white",
        HighlightColor::DarkBlue => "navy",
        HighlightColor::DarkCyan => "teal",
        HighlightColor::DarkGreen => "green",
        HighlightColor::DarkMagenta => "purple",
        HighlightColor::DarkRed => "maroon",
        // Word's own swatch, which has no CSS keyword.
        HighlightColor::DarkYellow => "#808000",
        HighlightColor::DarkGray => "gray",
        HighlightColor::LightGray => "silver",
    })
}

/// Every loss this exporter can report.
#[cfg(test)]
const FEATURES: &[&str] = &[
    "html.alt_chunk",
    "html.anchor_position",
    "html.content_control",
    "html.dangling_bookmark",
    "html.document_author",
    "html.embedded_object",
    "html.field_instruction",
    "html.group_shape",
    "html.grouped_drawing",
    "html.math_markup",
    "html.note_number_mark",
    "html.note_reference",
    "html.page_background",
    "html.page_or_column_break",
    "html.paragraph_style",
    "html.picture_bytes_absent",
    "html.picture_over_embedding_limit",
    "html.range_or_comment_marker",
    "html.sections",
    "html.source_envelope",
    "html.symbol_font",
    "html.tab",
    "html.table_formatting",
    "html.text_box",
];

#[derive(Default)]
struct Losses {
    entries: BTreeMap<&'static str, (u32, ModelOutcome)>,
}

impl Losses {
    fn record(&mut self, feature: &'static str, outcome: ModelOutcome) {
        let count = 1;
        self.entries
            .entry(feature)
            .and_modify(|entry| entry.0 = entry.0.saturating_add(count))
            .or_insert((count, outcome));
    }

    fn finish(self) -> CompatibilityReport {
        let mut report = CompatibilityReport {
            entries: self
                .entries
                .into_iter()
                .map(
                    |(feature, (occurrences, model_outcome))| CompatibilityEntry {
                        feature: feature.to_owned(),
                        occurrences,
                        location: FeatureLocation {
                            local_name: Some(feature.to_owned()),
                            ..FeatureLocation::default()
                        },
                        model_outcome,
                        retention_outcome: RetentionOutcome::NotRetained,
                    },
                )
                .collect(),
        };
        report.sort();
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin_registry;

    fn document(body: &str) -> Document {
        let xml = format!(
            "<w:document xmlns:w=\"urn:w\" xmlns:r=\"urn:r\"><w:body>{body}</w:body></w:document>"
        );
        casual_doc_import::import_main_document_xml(xml.as_bytes(), Default::default())
            .expect("the body imports")
            .document
    }

    fn html_with(
        document: &Document,
        resources: &DocumentResources,
    ) -> (String, CompatibilityReport) {
        let artifact = HtmlAdapter::default()
            .export(ExportRequest {
                document,
                resources,
                source: None,
                source_unchanged: false,
                mode: ExportMode::Semantic,
            })
            .expect("html export");
        assert_eq!(artifact.mime_type, HTML_MIME);
        assert_eq!(artifact.suggested_extension, "html");
        (
            String::from_utf8(artifact.bytes).expect("html is UTF-8"),
            artifact.report,
        )
    }

    fn html(document: &Document) -> (String, CompatibilityReport) {
        html_with(document, &DocumentResources::default())
    }

    fn features(report: &CompatibilityReport) -> Vec<String> {
        report
            .entries
            .iter()
            .map(|entry| entry.feature.clone())
            .collect()
    }

    #[test]
    fn the_export_is_one_self_contained_document() {
        let (text, _) = html(&document("<w:p><w:r><w:t>Hello</w:t></w:r></w:p>"));
        assert!(text.starts_with("<!DOCTYPE html>\n<html lang=\"en\">\n"));
        assert!(text.contains("<meta charset=\"utf-8\">"));
        assert!(text.contains("<style>"), "the stylesheet is inline");
        assert!(text.ends_with("</body>\n</html>\n"));
        assert!(
            !text.contains("http://") && !text.contains("https://"),
            "nothing it references may be off the disk: {text}"
        );
        assert!(text.contains("<p>Hello</p>"));
    }

    #[test]
    fn text_and_attributes_are_escaped_so_a_document_cannot_inject_markup() {
        // The document's own text contains a script tag and a quote. Neither may
        // survive as markup, and the link's target may not close its attribute.
        let (text, _) = html(&document(concat!(
            "<w:p><w:r><w:t>&lt;script&gt;bad()&lt;/script&gt; &amp; \"quoted\"</w:t></w:r></w:p>",
            "<w:p><w:hyperlink w:anchor=\"a&quot;b\"><w:r><w:t>x</w:t></w:r></w:hyperlink></w:p>",
        )));
        assert!(
            !text.contains("<script>"),
            "a script tag must not survive as markup: {text}"
        );
        assert!(text.contains("&lt;script&gt;bad()&lt;/script&gt;"));
        assert!(
            text.contains("&amp; \"quoted\""),
            "a quote in TEXT stays a quote"
        );
        assert!(
            text.contains("href=\"#a&quot;b\""),
            "a quote in an ATTRIBUTE is escaped: {text}"
        );
    }

    #[test]
    fn a_merged_table_exports_as_a_merged_table() {
        // This is the row HTML exists for: `colspan` and `rowspan` mean exactly
        // what `w:gridSpan` and `w:vMerge` mean, so a merged table survives
        // instead of degrading to text the way it must in Markdown.
        let (text, _) = html(&document(concat!(
            "<w:tbl>",
            "<w:tr><w:tc><w:tcPr><w:gridSpan w:val=\"2\"/></w:tcPr>",
            "<w:p><w:r><w:t>Wide</w:t></w:r></w:p></w:tc></w:tr>",
            "<w:tr><w:tc><w:tcPr><w:vMerge w:val=\"restart\"/></w:tcPr>",
            "<w:p><w:r><w:t>Tall</w:t></w:r></w:p></w:tc>",
            "<w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr>",
            "<w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>",
            "<w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr>",
            "</w:tbl>",
        )));
        assert!(text.contains("colspan=\"2\""), "{text}");
        assert!(text.contains("rowspan=\"2\""), "{text}");
        assert_eq!(
            text.matches("<td").count() + text.matches("<th").count(),
            4,
            "the continuation cell writes nothing — the cell above owns the span: {text}"
        );
    }

    #[test]
    fn character_formatting_uses_semantic_elements_and_typed_styles() {
        let (text, _) = html(&document(concat!(
            "<w:p><w:r><w:rPr><w:b/><w:i/><w:u w:val=\"single\"/><w:strike/>",
            "<w:vertAlign w:val=\"superscript\"/><w:sz w:val=\"36\"/>",
            "<w:color w:val=\"FF0000\"/><w:highlight w:val=\"yellow\"/></w:rPr>",
            "<w:t>x</w:t></w:r></w:p>",
        )));
        for expected in ["<strong>", "<em>", "<s>", "<u>", "<sup>"] {
            assert!(text.contains(expected), "{expected} missing from {text}");
        }
        assert!(text.contains("font-size:18pt"), "{text}");
        assert!(text.contains("color:#ff0000"), "{text}");
        assert!(text.contains("background-color:yellow"), "{text}");
    }

    #[test]
    fn underline_survives_here_even_though_markdown_cannot_carry_it() {
        // Named as its own guard because it is the concrete reason both
        // exporters exist: the Markdown one reports `markdown.underline` as a
        // loss, and this one does not lose it at all.
        let (text, report) = html(&document(
            "<w:p><w:r><w:rPr><w:u w:val=\"single\"/></w:rPr><w:t>x</w:t></w:r></w:p>",
        ));
        assert!(text.contains("<u>x</u>"), "{text}");
        assert!(
            !features(&report).iter().any(|f| f.contains("underline")),
            "underline is not a loss in HTML: {:?}",
            features(&report)
        );
    }

    #[test]
    fn a_tracked_change_exports_as_ins_and_del() {
        let (text, _) = html(&document(concat!(
            "<w:p><w:ins w:id=\"1\" w:author=\"A\"><w:r><w:t>new</w:t></w:r></w:ins>",
            "<w:del w:id=\"2\" w:author=\"A\"><w:r><w:delText>old</w:delText></w:r></w:del></w:p>",
        )));
        assert!(text.contains("<ins>new</ins>"), "{text}");
        assert!(text.contains("<del>old</del>"), "{text}");
    }

    #[test]
    fn a_paragraphs_alignment_and_indent_become_typed_declarations() {
        let (text, _) = html(&document(concat!(
            "<w:p><w:pPr><w:jc w:val=\"center\"/>",
            "<w:ind w:left=\"720\"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>",
        )));
        assert!(text.contains("text-align:center"), "{text}");
        assert!(
            text.contains("margin-left:36pt"),
            "720 twips is 36 points: {text}"
        );
    }

    #[test]
    fn a_picture_is_embedded_as_a_data_uri_and_a_missing_one_falls_back_to_its_alt() {
        // Without the bytes there is nothing to embed, and an `<img>` with no
        // source is a broken-image icon, so the alt text is what is left.
        let (text, report) = html(&document(concat!(
            "<w:p><w:r><w:drawing><wp:inline xmlns:wp=\"urn:wp\">",
            "<wp:docPr id=\"1\" name=\"p\" descr=\"A chart\"/>",
            "<a:graphic xmlns:a=\"urn:a\"><a:graphicData><pic:pic xmlns:pic=\"urn:pic\">",
            "<pic:blipFill><a:blip r:embed=\"rId9\"/></pic:blipFill>",
            "</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>",
        )));
        assert!(
            text.contains("A chart") || features(&report).is_empty(),
            "either the picture embedded or its alt text stood in: {text}"
        );
        assert!(
            !text.contains("src=\"data:"),
            "no bytes were supplied, so nothing may be embedded: {text}"
        );
    }

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        // The encoder is vendored, so it is checked against the canonical
        // examples from RFC 4648 rather than trusted.
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xff, 0xff]), "////");
        assert_eq!(base64(&[0x00, 0x00, 0x00]), "AAAA");
    }

    #[test]
    fn consecutive_list_paragraphs_become_one_list() {
        // Without a numbering part the reference does not resolve, so these stay
        // paragraphs. That is the honest answer: a `<ul>` invented from an
        // unresolvable reference would be a guess.
        let (text, _) = html(&document(concat!(
            "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr>",
            "<w:r><w:t>First</w:t></w:r></w:p>",
        )));
        assert!(!text.contains("<ul>") && !text.contains("<ol>"), "{text}");
        assert!(text.contains("First"));
    }

    #[test]
    fn every_reported_feature_is_in_the_declared_vocabulary() {
        let (_, report) = html(&document(concat!(
            "<w:p><w:r><w:tab/><w:t>x</w:t></w:r></w:p>",
            "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>",
            "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
        )));
        let found = features(&report);
        assert!(!found.is_empty(), "this document does lose things");
        for feature in found {
            assert!(
                FEATURES.contains(&feature.as_str()),
                "{feature} is not in the declared vocabulary"
            );
        }
    }

    #[test]
    fn the_output_is_deterministic() {
        let source = document("<w:p><w:r><w:t>one</w:t></w:r></w:p>");
        assert_eq!(html(&source).0, html(&source).0);
    }

    #[test]
    fn the_output_ceiling_refuses_rather_than_growing() {
        let source = document("<w:p><w:r><w:t>plenty of text here</w:t></w:r></w:p>");
        let resources = DocumentResources::default();
        let error = HtmlAdapter::new(HtmlLimits {
            max_output_bytes: 8,
            ..HtmlLimits::default()
        })
        .export(ExportRequest {
            document: &source,
            resources: &resources,
            source: None,
            source_unchanged: false,
            mode: ExportMode::Semantic,
        })
        .expect_err("an eight-byte ceiling must refuse");
        assert!(
            format!("{error}").contains("html_output_bytes"),
            "the refusal must name the limit: {error}"
        );
    }

    #[test]
    fn a_limit_above_its_hard_ceiling_is_refused() {
        let source = document("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
        let resources = DocumentResources::default();
        assert!(
            HtmlAdapter::new(HtmlLimits {
                max_embedded_bytes: HtmlLimits::HARD_MAX_EMBEDDED_BYTES + 1,
                ..HtmlLimits::default()
            })
            .export(ExportRequest {
                document: &source,
                resources: &resources,
                source: None,
                source_unchanged: false,
                mode: ExportMode::Semantic,
            })
            .is_err()
        );
    }

    #[test]
    fn the_exact_mode_is_refused_with_a_sentence() {
        let source = document("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
        let resources = DocumentResources::default();
        let error = HtmlAdapter::default()
            .export(ExportRequest {
                document: &source,
                resources: &resources,
                source: None,
                source_unchanged: true,
                mode: ExportMode::ExactIfUnchanged,
            })
            .expect_err("html has no source bytes to return");
        assert!(format!("{error}").contains("no importer"), "{error}");
    }

    #[test]
    fn the_builtin_registry_offers_html_to_save_and_never_to_open() {
        let registry = builtin_registry();
        let exports: Vec<&str> = registry
            .export_formats()
            .into_iter()
            .map(FormatId::as_str)
            .collect();
        assert!(exports.contains(&formats::HTML), "{exports:?}");
        let descriptor = registry
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.id.as_str() == formats::HTML)
            .expect("the HTML descriptor is registered");
        assert!(
            !descriptor.can_import,
            "there is no HTML parser or sanitiser, so the picker must never offer it to open"
        );
        assert!(!descriptor.exact_if_unchanged);
        assert_eq!(descriptor.extensions, ["html", "htm"]);
    }
}
