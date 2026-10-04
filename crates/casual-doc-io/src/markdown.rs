//! Built-in deterministic CommonMark exporter.
//!
//! # Why this exists, and what it is
//!
//! `docs/153` grades ONLYOFFICE's Download-as list against ours. Theirs offers
//! Markdown (`apps/documenteditor/main/app/view/FileMenuPanels.js`, the `MD`
//! row); ours offered DOCX, ODT, RTF, plain text, normalized JSON and PDF. This
//! closes `shell.export-markdown`.
//!
//! **The established shape, named before anything was invented** (`SKILL` §8):
//! this is a *serializer* over the normalized model, structurally the same as
//! the plain-text exporter beside it — one linear walk, a bounded output buffer,
//! and a compatibility report for everything the target format cannot carry.
//! Nothing here is a new kind of component.
//!
//! The dialect is **CommonMark**, plus GFM pipe tables, because that is the
//! subset every consumer agrees on and the subset ONLYOFFICE's own `MD` row
//! writes. Nothing outside it is emitted: no HTML fallback, no reference links,
//! no setext headings, no front matter.
//!
//! # Export only, deliberately
//!
//! There is no Markdown importer, and this crate does not pretend otherwise:
//! the descriptor says `can_import: false`, so the registry never offers
//! Markdown as something to open, and `detect` never probes it. Reading
//! Markdown needs a CommonMark parser, which is a separate piece of work with
//! its own security envelope. Declaring an importer we do not have would be the
//! "built is not reachable" defect in reverse — a promise the registry cannot
//! keep.
//!
//! `exact_if_unchanged` is likewise `false`. A document is never *opened* as
//! Markdown, so there are no original bytes to hand back; asking for that mode
//! is refused with a sentence rather than answered with a semantic export that
//! quietly is not the same thing.
//!
//! # No silent loss
//!
//! Markdown carries a small fraction of a word-processing document. Every
//! construct that cannot survive is recorded in the [`CompatibilityReport`]
//! with an occurrence count, which is what makes this export honest rather than
//! lossy-and-quiet (`SKILL` §12: unsupported data is preserved where safe or
//! reported explicitly). The features are named `markdown.*` so a host can tell
//! at a glance which export dropped what.
//!
//! # Complexity
//!
//! One pass over the body, so O(nodes) in the document, and O(1) extra space
//! per nesting level. Nothing here looks a node up by id, so there is no linear
//! scan inside a loop (`SKILL` §8).

use std::collections::BTreeMap;

use casual_doc_model::v1::{
    BlockNode, BreakKind, Definitions, Document, GroupChild, HyperlinkTarget, InlineNode,
    NumberFormat, Paragraph, ParagraphProperties, RevisionKind, RunProperties, Table,
    WordprocessingGroup,
};

use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, Disposition, ExportArtifact, ExportMode,
    ExportRequest, FeatureLocation, FormatDescriptor, FormatExporter, FormatId, FormatProfile,
    ModelOutcome, PreservationLedger, formats,
};

/// The media type registered for Markdown, and the one the artifact carries.
///
/// `text/markdown` is the IANA registration (RFC 7763). `webapp`'s
/// `format_io.test.mjs` reads media types back out of this crate by constant
/// name, so this is the name it looks for.
pub const MARKDOWN_MIME: &str = "text/markdown";

/// Host-configurable Markdown limits with non-bypassable hard ceilings.
///
/// Same shape and the same reason as [`PlainTextLimits`](crate::PlainTextLimits):
/// the browser build runs on wasm32 and a limit sized for a 64-bit host let a
/// small input exhaust linear memory (`docs/104` HF-158). An exporter cannot
/// refuse its input, but it CAN refuse to grow its output without bound, which
/// is the failure that actually reaches a browser here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkdownLimits {
    /// Maximum bytes of emitted Markdown.
    pub max_output_bytes: usize,
    /// Maximum block-nesting depth followed before the walk refuses.
    pub max_nesting_depth: usize,
}

impl MarkdownLimits {
    /// Hard maximum emitted bytes.
    pub const HARD_MAX_OUTPUT_BYTES: usize = 256 * 1024 * 1024;
    /// Hard maximum nesting depth.
    pub const HARD_MAX_NESTING_DEPTH: usize = 256;

    fn validate(self) -> Result<(), AdapterError> {
        for (name, value, ceiling) in [
            (
                "markdown_output_bytes",
                self.max_output_bytes,
                Self::HARD_MAX_OUTPUT_BYTES,
            ),
            (
                "markdown_nesting_depth",
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

impl Default for MarkdownLimits {
    fn default() -> Self {
        Self {
            max_output_bytes: 64 * 1024 * 1024,
            max_nesting_depth: 32,
        }
    }
}

/// Built-in bounded CommonMark exporter.
#[derive(Clone, Debug)]
pub struct MarkdownAdapter {
    descriptor: FormatDescriptor,
    limits: MarkdownLimits,
}

impl MarkdownAdapter {
    /// Creates an exporter with explicit Markdown limits.
    #[must_use]
    pub fn new(limits: MarkdownLimits) -> Self {
        Self {
            descriptor: FormatDescriptor {
                id: FormatId::new(formats::MARKDOWN).expect("built-in markdown id is valid"),
                display_name: "Markdown".to_owned(),
                mime_types: vec![MARKDOWN_MIME.to_owned()],
                extensions: vec!["md".to_owned(), "markdown".to_owned()],
                can_import: false,
                can_export: true,
                exact_if_unchanged: false,
                preserve_when_safe: false,
            },
            limits,
        }
    }
}

impl Default for MarkdownAdapter {
    fn default() -> Self {
        Self::new(MarkdownLimits::default())
    }
}

impl FormatExporter for MarkdownAdapter {
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
                "markdown has no importer, so there are no retained source bytes to return \
                 exactly; ask for the semantic mode",
            ));
        }

        let mut writer = Writer::new(self.limits);
        let mut losses = Losses::default();
        writer.document(request.document, &mut losses)?;
        if request.document.definitions().sections.len() > 1 {
            losses.record("markdown.sections", ModelOutcome::Omitted);
        }
        if request.document.properties().is_some() {
            losses.record("markdown.document_properties", ModelOutcome::Omitted);
        }
        if request.document.background().is_some() {
            losses.record("markdown.page_background", ModelOutcome::Omitted);
        }
        if !request.resources.is_empty() {
            losses.record_many(
                "markdown.binary_resources",
                ModelOutcome::Omitted,
                request.resources.as_map().len(),
            );
        }
        if request.source.is_some() {
            losses.record("markdown.source_envelope", ModelOutcome::Omitted);
        }

        Ok(ExportArtifact {
            bytes: writer.finish(),
            report: losses.finish(),
            ledger: PreservationLedger::default(),
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: Some("commonmark".to_owned()),
            },
            mime_type: MARKDOWN_MIME.to_owned(),
            suggested_extension: "md".to_owned(),
        })
    }
}

/// The bounded output buffer.
///
/// It owns the blank-line rule as well as the byte ceiling, because CommonMark's
/// block structure IS its blank lines: a heading that does not begin at the
/// start of a line is text, and two paragraphs with no blank line between them
/// are one paragraph. Keeping that rule in one place is why the walk below never
/// writes a `\n` of its own.
struct Writer {
    value: String,
    max_bytes: usize,
    max_depth: usize,
    /// Blank lines owed before the next block is written. `usize::MAX` is the
    /// document start, where none are owed and none may be emitted.
    pending: Option<usize>,
}

impl Writer {
    fn new(limits: MarkdownLimits) -> Self {
        Self {
            value: String::new(),
            max_bytes: limits.max_output_bytes,
            max_depth: limits.max_nesting_depth,
            pending: None,
        }
    }

    /// Opens a block: settles the blank lines owed to the previous one.
    fn open_block(&mut self, blank_lines: usize) -> Result<(), AdapterError> {
        if let Some(owed) = self.pending {
            for _ in 0..owed.max(blank_lines) + 1 {
                self.push('\n')?;
            }
        }
        self.pending = None;
        Ok(())
    }

    /// Closes a block, owing `blank_lines` before whatever comes next.
    fn close_block(&mut self, blank_lines: usize) {
        self.pending = Some(self.pending.unwrap_or(0).max(blank_lines));
    }

    fn push_str(&mut self, value: &str) -> Result<(), AdapterError> {
        let observed = self.value.len().saturating_add(value.len());
        enforce("markdown_output_bytes", observed, self.max_bytes)?;
        self.value.push_str(value);
        Ok(())
    }

    fn push(&mut self, value: char) -> Result<(), AdapterError> {
        let observed = self.value.len().saturating_add(value.len_utf8());
        enforce("markdown_output_bytes", observed, self.max_bytes)?;
        self.value.push(value);
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        let mut value = self.value;
        // A text file ends with exactly one newline. Anything else is a diff
        // every tool disagrees about.
        while value.ends_with('\n') {
            value.pop();
        }
        if !value.is_empty() {
            value.push('\n');
        }
        value.into_bytes()
    }

    fn document(&mut self, document: &Document, losses: &mut Losses) -> Result<(), AdapterError> {
        let context = Context {
            definitions: document.definitions(),
            depth: 0,
            max_depth: self.max_depth,
            in_table_cell: false,
        };
        self.blocks(document.body(), &context, losses)
    }

    fn blocks(
        &mut self,
        blocks: &[BlockNode],
        context: &Context<'_>,
        losses: &mut Losses,
    ) -> Result<(), AdapterError> {
        for block in blocks {
            self.block(block, context, losses)?;
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
                losses.record("markdown.content_control", ModelOutcome::Degraded);
                self.blocks(&sdt.blocks, context, losses)
            }
            BlockNode::AltChunk(_) => {
                losses.record("markdown.alt_chunk", ModelOutcome::Omitted);
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
        let marker = list_marker(paragraph, context.definitions);

        // An empty paragraph is a blank line in Word and nothing at all in
        // Markdown, where a blank line is punctuation rather than content. Owing
        // one extra blank line is the closest faithful reading, and it is
        // reported because a document that used empty paragraphs for spacing
        // will not look the same.
        if heading.is_none() && marker.is_none() && paragraph_is_empty(paragraph) {
            losses.record("markdown.empty_paragraph", ModelOutcome::Omitted);
            self.close_block(0);
            return Ok(());
        }

        report_paragraph_formatting(paragraph, heading.is_some(), marker.is_some(), losses);

        self.open_block(0)?;
        let indent = marker.as_ref().map_or(0, |m| m.level);
        for _ in 0..indent {
            self.push_str("  ")?;
        }
        if let Some(level) = heading {
            for _ in 0..level {
                self.push('#')?;
            }
            self.push(' ')?;
        } else if let Some(marker) = &marker {
            let text = marker.text.clone();
            self.push_str(&text)?;
        }
        self.inlines(&paragraph.inlines, context, losses)?;
        // A list item owes no blank line: a tight list is what a word processor
        // draws, and a blank line between items makes CommonMark render every
        // item as its own paragraph.
        self.close_block(usize::from(marker.is_none()));
        Ok(())
    }

    /// A GFM pipe table, when the table is rectangular and flat; otherwise the
    /// cells' text in reading order, reported as degraded.
    ///
    /// The refusal is the honest half. A pipe table has no vertical merges, no
    /// nested tables and no block content inside a cell, so a table that has any
    /// of those cannot be written as one — and writing a broken pipe table would
    /// produce a file that looks like a table and is not.
    fn table(
        &mut self,
        table: &Table,
        context: &Context<'_>,
        losses: &mut Losses,
    ) -> Result<(), AdapterError> {
        let columns = pipe_table_columns(table);
        let Some(columns) = columns else {
            losses.record("markdown.table_structure", ModelOutcome::Degraded);
            for row in &table.rows {
                for cell in &row.cells {
                    self.blocks(&cell.blocks, context, losses)?;
                }
            }
            return Ok(());
        };

        losses.record("markdown.table_formatting", ModelOutcome::Omitted);
        self.open_block(0)?;
        for (index, row) in table.rows.iter().enumerate() {
            if index != 0 {
                self.push('\n')?;
            }
            self.push_str("| ")?;
            for (cell_index, cell) in row.cells.iter().enumerate() {
                if cell_index != 0 {
                    self.push_str(" | ")?;
                }
                let inlines = single_paragraph_inlines(cell).unwrap_or(&[]);
                let cell_context = Context {
                    in_table_cell: true,
                    ..context.clone()
                };
                self.inlines(inlines, &cell_context, losses)?;
            }
            self.push_str(" |")?;
            // GFM requires the delimiter row, and requires it second. A table
            // whose first row is not a header still needs one, so the first row
            // becomes the header: that is the only shape the format has.
            if index == 0 {
                self.push('\n')?;
                for _ in 0..columns {
                    self.push_str("| --- ")?;
                }
                self.push('|')?;
            }
        }
        self.close_block(1);
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
                let emphasis = Emphasis::of(&run.properties);
                report_run_formatting(&run.properties, emphasis, losses);
                if run.text.is_empty() {
                    return Ok(());
                }
                self.push_str(emphasis.opening())?;
                let escaped = escape_text(&run.text, context.in_table_cell);
                self.push_str(&escaped)?;
                self.push_str(emphasis.closing())
            }
            InlineNode::Tab(_) => {
                // A tab is layout, and Markdown has none inside a paragraph; a
                // leading one would open an indented code block.
                losses.record("markdown.tab", ModelOutcome::Omitted);
                self.push(' ')
            }
            InlineNode::Break(node) => match node.kind {
                BreakKind::Line if context.in_table_cell => {
                    // A pipe table row is one line by definition.
                    losses.record("markdown.line_break_in_table", ModelOutcome::Degraded);
                    self.push(' ')
                }
                BreakKind::Line => {
                    // The backslash hard break, not two trailing spaces: an
                    // editor that trims trailing whitespace silently deletes the
                    // other spelling.
                    self.push_str("\\\n")
                }
                _ => {
                    losses.record("markdown.page_or_column_break", ModelOutcome::Omitted);
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
                if link.tooltip.is_some() {
                    losses.record("markdown.hyperlink_tooltip", ModelOutcome::Omitted);
                }
                self.push('[')?;
                self.inlines(&link.inlines, context, losses)?;
                self.push_str("](")?;
                let target = escape_link_target(&target);
                self.push_str(&target)?;
                self.push(')')
            }
            InlineNode::HorizontalRule(_) if !context.in_table_cell => {
                self.open_block(0)?;
                self.push_str("---")?;
                self.close_block(1);
                Ok(())
            }
            InlineNode::Drawing(drawing) => self.image_alt(
                drawing.descr.as_deref(),
                "markdown.drawing",
                context,
                losses,
            ),
            InlineNode::AnchoredDrawing(drawing) => self.image_alt(
                drawing.descr.as_deref(),
                "markdown.anchored_drawing",
                context,
                losses,
            ),
            InlineNode::Field(field) => {
                // The field's cached result is ordinary content; the instruction
                // is not expressible and the result is what a reader wants.
                losses.record("markdown.field_instruction", ModelOutcome::Degraded);
                self.inlines(&field.inlines, context, losses)
            }
            InlineNode::TextBox(text_box) => {
                losses.record("markdown.text_box", ModelOutcome::Degraded);
                self.blocks(&text_box.blocks, context, losses)
            }
            InlineNode::Group(group) => self.group(group, context, losses),
            InlineNode::Revision(revision) => {
                losses.record("markdown.revision", ModelOutcome::Degraded);
                if matches!(
                    revision.kind,
                    RevisionKind::Insertion | RevisionKind::MoveTo
                ) {
                    self.inlines(&revision.inlines, context, losses)?;
                }
                Ok(())
            }
            InlineNode::Sdt(sdt) => {
                losses.record("markdown.content_control", ModelOutcome::Degraded);
                self.inlines(&sdt.inlines, context, losses)
            }
            InlineNode::Math(math) => {
                losses.record("markdown.math_markup", ModelOutcome::Degraded);
                let escaped = escape_text(&math.text, context.in_table_cell);
                self.push_str(&escaped)
            }
            InlineNode::Symbol(symbol) => {
                losses.record("markdown.symbol_font", ModelOutcome::Degraded);
                match char::from_u32(symbol.char) {
                    Some(character) => {
                        let escaped = escape_text(&character.to_string(), context.in_table_cell);
                        self.push_str(&escaped)
                    }
                    None => Ok(()),
                }
            }
            InlineNode::NoBreakHyphen(_) => self.push('\u{2011}'),
            InlineNode::SoftHyphen(_) => self.push('\u{00ad}'),
            InlineNode::PositionalTab(_) => {
                losses.record("markdown.positional_tab", ModelOutcome::Omitted);
                self.push(' ')
            }
            InlineNode::EmbeddedObject(_) => {
                losses.record("markdown.embedded_object", ModelOutcome::Omitted);
                Ok(())
            }
            InlineNode::NoteReference(_) => {
                // CommonMark has no footnotes; GFM's are an extension this
                // exporter does not emit, and the note BODY lives in
                // `definitions`, which is not walked.
                losses.record("markdown.note_reference", ModelOutcome::Omitted);
                Ok(())
            }
            InlineNode::HorizontalRule(_) => {
                losses.record("markdown.horizontal_rule_in_table", ModelOutcome::Omitted);
                Ok(())
            }
            InlineNode::NoteNumberMark(_) => {
                losses.record("markdown.note_number_mark", ModelOutcome::Omitted);
                Ok(())
            }
            InlineNode::CommentReference(_)
            | InlineNode::CommentRangeStart(_)
            | InlineNode::CommentRangeEnd(_)
            | InlineNode::BookmarkStart(_)
            | InlineNode::BookmarkEnd(_)
            | InlineNode::FieldRangeStart(_)
            | InlineNode::FieldRangeEnd(_)
            | InlineNode::MoveRangeStart(_)
            | InlineNode::MoveRangeEnd(_) => {
                losses.record("markdown.range_or_comment_marker", ModelOutcome::Omitted);
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
                "limit markdown_nesting_depth exceeded while walking a grouped drawing",
            ));
        }
        losses.record("markdown.grouped_drawing", ModelOutcome::Degraded);
        let deeper = Context {
            depth: context.depth + 1,
            ..context.clone()
        };
        for child in &group.children {
            match child {
                GroupChild::Picture(picture) => {
                    self.image_alt(
                        picture.descr.as_deref(),
                        "markdown.group_picture",
                        &deeper,
                        losses,
                    )?;
                }
                GroupChild::TextBox(text_box) => {
                    self.blocks(&text_box.blocks, &deeper, losses)?;
                }
                GroupChild::Shape(_) => {
                    // A preset shape is geometry. Markdown has none, and the
                    // model's group shape carries no alt text to fall back to.
                    losses.record("markdown.group_shape", ModelOutcome::Omitted);
                }
                GroupChild::Group(inner) => self.group(inner, &deeper, losses)?,
            }
        }
        Ok(())
    }

    /// A drawing's alternative text, as a CommonMark image with no source.
    ///
    /// The bytes are not embedded: a Markdown file is text, and an exporter that
    /// silently base64'd a document's pictures into it would produce a file no
    /// reader wants. The alternative text is what survives, and the loss is
    /// reported.
    fn image_alt(
        &mut self,
        description: Option<&str>,
        feature: &'static str,
        context: &Context<'_>,
        losses: &mut Losses,
    ) -> Result<(), AdapterError> {
        losses.record(feature, ModelOutcome::Degraded);
        let Some(description) = description.filter(|value| !value.trim().is_empty()) else {
            return Ok(());
        };
        self.push_str("![")?;
        let escaped = escape_text(description, context.in_table_cell);
        self.push_str(&escaped)?;
        self.push_str("]()")
    }
}

/// What the walk needs to know about where it is.
///
/// Three facts, and each one changes what may be emitted: the definitions table
/// (headings and list markers are resolved through it), the nesting depth
/// against the limit, and whether the cursor is inside a pipe-table cell, where
/// a newline ends the row and a `|` ends the cell.
#[derive(Clone)]
struct Context<'a> {
    definitions: &'a Definitions,
    depth: usize,
    max_depth: usize,
    in_table_cell: bool,
}

fn enforce(name: &'static str, observed: usize, allowed: usize) -> Result<(), AdapterError> {
    if observed > allowed {
        return Err(AdapterError::new(format!(
            "limit {name} observed {observed} exceeds allowed {allowed}"
        )));
    }
    Ok(())
}

/// Which emphasis delimiters a run needs, if any.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Emphasis {
    bold: bool,
    italic: bool,
    strike: bool,
}

impl Emphasis {
    fn of(properties: &RunProperties) -> Self {
        Self {
            bold: properties.bold == Some(true),
            italic: properties.italic == Some(true),
            strike: properties.strike == Some(true) || properties.double_strike == Some(true),
        }
    }

    /// The opening delimiter run.
    ///
    /// Order matters and the closing run is NOT the same string: CommonMark
    /// nests by position, so `~~***text***~~` is the spelling every renderer
    /// agrees on and `~~***text~~***` is not. A guard caught exactly that.
    fn opening(self) -> &'static str {
        match (self.strike, self.bold, self.italic) {
            (false, false, false) => "",
            (false, false, true) => "*",
            (false, true, false) => "**",
            (false, true, true) => "***",
            (true, false, false) => "~~",
            (true, false, true) => "~~*",
            (true, true, false) => "~~**",
            (true, true, true) => "~~***",
        }
    }

    /// The closing delimiter run: the opening one's groups, innermost first.
    fn closing(self) -> &'static str {
        match (self.strike, self.bold, self.italic) {
            (false, false, false) => "",
            (false, false, true) => "*",
            (false, true, false) => "**",
            (false, true, true) => "***",
            (true, false, false) => "~~",
            (true, false, true) => "*~~",
            (true, true, false) => "**~~",
            (true, true, true) => "***~~",
        }
    }
}

/// Escapes a run of text so no character in it opens a CommonMark construct.
///
/// The rule is CommonMark's own: a backslash before any ASCII punctuation is a
/// literal. Rather than guess at context, every character that can *begin* an
/// inline construct is escaped unconditionally, and the three that only matter
/// at the start of a line are escaped there. That is more backslashes than a
/// human would type and is always correct; the alternative — a contextual
/// escaper — is where Markdown serializers grow their bugs.
fn escape_text(text: &str, in_table_cell: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' => {
                out.push('\\');
                out.push(character);
            }
            '|' if in_table_cell => {
                // A literal pipe inside a GFM cell is `\|`, and nothing else
                // works: the cell is split before inline parsing.
                out.push('\\');
                out.push('|');
            }
            '&' => out.push_str("&amp;"),
            // A newline inside a run would end the block. Runs should not carry
            // one — `Document::validate` rejects control characters — but a
            // space is the only safe answer if one arrives.
            '\n' | '\r' => out.push(' '),
            _ => out.push(character),
        }
    }
    out
}

/// Escapes a link destination for the `(…)` form.
fn escape_link_target(target: &str) -> String {
    let needs_angles = target
        .chars()
        .any(|c| c.is_whitespace() || c == '(' || c == ')');
    if !needs_angles {
        return target.to_owned();
    }
    let mut out = String::with_capacity(target.len() + 2);
    out.push('<');
    for character in target.chars() {
        if matches!(character, '<' | '>' | '\\') {
            out.push('\\');
        }
        if character.is_whitespace() {
            out.push_str("%20");
        } else {
            out.push(character);
        }
    }
    out.push('>');
    out
}

/// The heading level a paragraph's style names, 1-6, or `None`.
///
/// Read from the style's own `w:outlineLvl` first and from its NAME second.
/// The outline level is the property Word's navigation pane and TOC read, so it
/// is the more truthful answer; the name is the fallback for a document whose
/// heading styles carry no explicit level, which Word's own defaults do not.
///
/// Levels 7-9 exist in OOXML and not in CommonMark, which stops at 6. They are
/// emitted as level 6 and reported, because the alternative is a `#######` that
/// every renderer prints as literal hashes.
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

/// `heading 3`, `Heading 3`, `heading3` -> 3.
fn heading_level_from_name(name: &str) -> Option<u8> {
    let trimmed = name.trim();
    let rest = trimmed
        .strip_prefix("heading")
        .or_else(|| trimmed.strip_prefix("Heading"))?;
    rest.trim().parse::<u8>().ok()
}

/// The list marker a paragraph's numbering asks for.
struct Marker {
    text: String,
    level: usize,
}

fn list_marker(paragraph: &Paragraph, definitions: &Definitions) -> Option<Marker> {
    let reference = paragraph.properties.numbering?;
    let level = definitions.numbering_resolver().level(reference)?;
    let ordered = !matches!(
        level.num_fmt.as_ref(),
        Some(NumberFormat::Bullet) | Some(NumberFormat::None) | None
    );
    Some(Marker {
        // Every ordered item is written `1.`: CommonMark numbers a list from its
        // first item and ignores the rest, so emitting the real counter would
        // require running the numbering machine and would change nothing a
        // reader sees.
        text: if ordered {
            "1. ".to_owned()
        } else {
            "- ".to_owned()
        },
        level: usize::from(reference.level),
    })
}

fn paragraph_is_empty(paragraph: &Paragraph) -> bool {
    paragraph.inlines.iter().all(|inline| match inline {
        InlineNode::Run(run) => run.text.is_empty(),
        InlineNode::CommentReference(_)
        | InlineNode::CommentRangeStart(_)
        | InlineNode::CommentRangeEnd(_)
        | InlineNode::BookmarkStart(_)
        | InlineNode::BookmarkEnd(_) => true,
        _ => false,
    })
}

/// The column count a GFM pipe table would need, or `None` when the table
/// cannot be written as one.
fn pipe_table_columns(table: &Table) -> Option<usize> {
    let first = table.rows.first()?;
    let columns = first.cells.len();
    if columns == 0 {
        return None;
    }
    for row in &table.rows {
        if row.cells.len() != columns {
            return None;
        }
        for cell in &row.cells {
            if cell.properties.grid_span.is_some_and(|span| span > 1)
                || cell.properties.vertical_merge.is_some()
            {
                return None;
            }
            single_paragraph_inlines(cell)?;
        }
    }
    Some(columns)
}

/// A cell's inlines when it holds exactly one paragraph, else `None`.
fn single_paragraph_inlines(cell: &casual_doc_model::v1::TableCell) -> Option<&[InlineNode]> {
    match cell.blocks.as_slice() {
        [BlockNode::Paragraph(paragraph)] => Some(&paragraph.inlines),
        [] => Some(&[]),
        _ => None,
    }
}

fn report_paragraph_formatting(
    paragraph: &Paragraph,
    is_heading: bool,
    is_list: bool,
    losses: &mut Losses,
) {
    let properties: &ParagraphProperties = &paragraph.properties;
    if properties.alignment.is_some() {
        losses.record("markdown.paragraph_alignment", ModelOutcome::Omitted);
    }
    if properties.indentation.is_some() && !is_list {
        losses.record("markdown.paragraph_indentation", ModelOutcome::Omitted);
    }
    if properties.spacing.is_some() {
        losses.record("markdown.paragraph_spacing", ModelOutcome::Omitted);
    }
    if properties.style_ref.is_some() && !is_heading {
        losses.record("markdown.paragraph_style", ModelOutcome::Omitted);
    }
}

fn report_run_formatting(properties: &RunProperties, emphasis: Emphasis, losses: &mut Losses) {
    if properties.underline == Some(true) {
        // CommonMark has no underline, and `<u>` is raw HTML this exporter does
        // not emit.
        losses.record("markdown.underline", ModelOutcome::Omitted);
    }
    if properties.color.is_some() || properties.highlight.is_some() {
        losses.record("markdown.run_colour", ModelOutcome::Omitted);
    }
    if properties.size_half_points.is_some() || properties.font_ref.is_some() {
        losses.record("markdown.run_font", ModelOutcome::Omitted);
    }
    if properties.vertical_alignment.is_some() {
        losses.record("markdown.superscript_or_subscript", ModelOutcome::Omitted);
    }
    let _ = emphasis;
}

/// Every loss this exporter can report, so a reader can see the whole
/// vocabulary in one place and a test can assert nothing was invented outside
/// it.
///
/// A report entry whose feature is not here would be a name a host cannot look
/// up, which is the same defect as a refusal with no code.
#[cfg(test)]
const FEATURES: &[&str] = &[
    "markdown.alt_chunk",
    "markdown.anchored_drawing",
    "markdown.binary_resources",
    "markdown.content_control",
    "markdown.document_properties",
    "markdown.drawing",
    "markdown.empty_paragraph",
    "markdown.embedded_object",
    "markdown.field_instruction",
    "markdown.group_picture",
    "markdown.group_shape",
    "markdown.grouped_drawing",
    "markdown.horizontal_rule_in_table",
    "markdown.hyperlink_tooltip",
    "markdown.line_break_in_table",
    "markdown.math_markup",
    "markdown.note_number_mark",
    "markdown.note_reference",
    "markdown.page_background",
    "markdown.page_or_column_break",
    "markdown.paragraph_alignment",
    "markdown.paragraph_indentation",
    "markdown.paragraph_spacing",
    "markdown.paragraph_style",
    "markdown.positional_tab",
    "markdown.range_or_comment_marker",
    "markdown.revision",
    "markdown.run_colour",
    "markdown.run_font",
    "markdown.sections",
    "markdown.source_envelope",
    "markdown.superscript_or_subscript",
    "markdown.symbol_font",
    "markdown.tab",
    "markdown.table_formatting",
    "markdown.table_structure",
    "markdown.text_box",
    "markdown.underline",
];

#[derive(Default)]
struct Losses {
    entries: BTreeMap<&'static str, (u32, ModelOutcome)>,
}

impl Losses {
    fn record(&mut self, feature: &'static str, outcome: ModelOutcome) {
        self.record_many(feature, outcome, 1);
    }

    fn record_many(&mut self, feature: &'static str, outcome: ModelOutcome, count: usize) {
        let count = u32::try_from(count).unwrap_or(u32::MAX);
        self.entries
            .entry(feature)
            .and_modify(|entry| entry.0 = entry.0.saturating_add(count))
            .or_insert((count, outcome));
    }

    /// Builds the report, resolving each finding's model outcome into the one
    /// disposition that is honest for this target.
    ///
    /// The retention half is `not-retained` for every finding, and that is a
    /// property of the FORMAT rather than a per-mode constant of the kind
    /// FID-R-02 removed: this target is a flat text encoding with no sidecar, no
    /// side-table and no byte floor, so there is nowhere for an unconsumed
    /// remainder to be kept. A `mapped` finding is therefore skipped rather than
    /// paired: `mapped` + `not-retained` is one of the six combinations
    /// `35-DISPOSITION-TAXONOMY.md` refuses, and a construct this writer emitted
    /// in full is not a finding in the first place. No call site records one
    /// today, so nothing is skipped — the arm exists so that the type cannot be
    /// used to publish an illegal pair later.
    fn finish(self) -> CompatibilityReport {
        let mut report = CompatibilityReport {
            entries: self
                .entries
                .into_iter()
                .filter_map(|(feature, (occurrences, model_outcome))| {
                    let disposition = match model_outcome {
                        ModelOutcome::Mapped => return None,
                        ModelOutcome::Degraded => Disposition::DegradedNotRetained,
                        ModelOutcome::Omitted => Disposition::OmittedNotRetained,
                    };
                    Some(CompatibilityEntry {
                        feature: feature.to_owned(),
                        occurrences,
                        location: FeatureLocation {
                            element: Some(feature.to_owned()),
                            ..FeatureLocation::default()
                        },
                        disposition,
                        ledger_id: None,
                        part: None,
                    })
                })
                .collect(),
        };
        report.sort();
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DocumentResources, builtin_registry};

    /// Builds a document by importing WordprocessingML, so the tests exercise
    /// the same model shape a real `.docx` produces rather than a hand-built one
    /// that could agree with a broken exporter.
    fn document(body: &str) -> Document {
        let xml = format!(
            "<w:document xmlns:w=\"urn:w\" xmlns:r=\"urn:r\"><w:body>{body}</w:body></w:document>"
        );
        casual_doc_import::import_main_document_xml(xml.as_bytes(), Default::default())
            .expect("the body imports")
            .document
    }

    fn markdown(document: &Document) -> (String, CompatibilityReport) {
        let resources = DocumentResources::default();
        let artifact = MarkdownAdapter::default()
            .export(ExportRequest {
                document,
                resources: &resources,
                source: None,
                source_unchanged: false,
                mode: ExportMode::Semantic,
            })
            .expect("markdown export");
        assert_eq!(artifact.mime_type, MARKDOWN_MIME);
        assert_eq!(artifact.suggested_extension, "md");
        (
            String::from_utf8(artifact.bytes).expect("markdown is UTF-8"),
            artifact.report,
        )
    }

    fn features(report: &CompatibilityReport) -> Vec<String> {
        report
            .entries
            .iter()
            .map(|entry| entry.feature.clone())
            .collect()
    }

    /// A document whose one paragraph carries a style whose NAME says it is a
    /// heading, and one whose style is named something else.
    ///
    /// Built model-first rather than imported, because a heading is the one
    /// thing this exporter cannot learn from a body alone: `w:pStyle` is a
    /// reference into `styles.xml`, so a document imported without that part has
    /// an unresolvable style id and — correctly — produces no heading at all.
    fn document_with_styles(entries: &[(&str, Option<u8>, &str)]) -> Document {
        use casual_doc_model::IdGenerator;
        use casual_doc_model::v1::{
            BlockNode, Definitions, Paragraph, ParagraphProperties, Run, RunProperties, Style,
            StyleId, StyleKind,
        };

        let mut ids = IdGenerator::new(0x5eed);
        let mut definitions = Definitions::default();
        let mut body = Vec::new();
        for (style_name, outline_level, text) in entries {
            let style_id = StyleId::new(ids.next_id().expect("style id"));
            definitions.styles.insert(
                style_id,
                Style {
                    kind: StyleKind::Paragraph,
                    is_default: false,
                    name: Some((*style_name).to_owned()),
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
                    paragraph: outline_level.map(|level| ParagraphProperties {
                        outline_level: Some(level),
                        ..ParagraphProperties::default()
                    }),
                    run: None,
                    table: None,
                    table_row: None,
                    table_cell: None,
                    conditional: Vec::new(),
                },
            );
            body.push(BlockNode::Paragraph(Paragraph {
                id: ids.next_id().expect("paragraph id"),
                properties: ParagraphProperties {
                    style_ref: Some(style_id),
                    ..ParagraphProperties::default()
                }
                .into(),
                inlines: vec![InlineNode::Run(Run {
                    id: ids.next_id().expect("run id"),
                    properties: RunProperties::default().into(),
                    text: (*text).to_owned(),
                })],
            }));
        }
        Document::new(ids.next_id().expect("document id"), body, definitions)
            .expect("the constructed document is valid")
    }

    #[test]
    fn a_styled_heading_writes_atx_hashes_at_its_level() {
        let (text, report) = markdown(&document_with_styles(&[
            ("heading 1", None, "Title"),
            ("Heading 3", None, "Deeper"),
            ("Quote", None, "Not a heading"),
        ]));
        assert_eq!(text, "# Title\n\n### Deeper\n\nNot a heading\n");
        assert!(
            features(&report).contains(&"markdown.paragraph_style".to_owned()),
            "the non-heading style is dropped and must be reported: {:?}",
            features(&report)
        );
    }

    #[test]
    fn a_renamed_heading_style_is_recognised_by_its_outline_level() {
        // A document that renamed its heading styles still carries the outline
        // level Word's navigation pane and TOC read, so that is the signal with
        // priority. "Chapter" is not a heading name; level 0 is heading 1.
        let (text, _) = markdown(&document_with_styles(&[("Chapter", Some(0), "One")]));
        assert_eq!(text, "# One\n");
    }

    #[test]
    fn an_outline_level_below_commonmarks_floor_clamps_to_six() {
        // OOXML has nine outline levels and CommonMark has six. `#######` is not
        // a heading in any renderer — it prints as literal hashes — so level 8
        // becomes level 6 rather than becoming text.
        let (text, _) = markdown(&document_with_styles(&[("heading 8", Some(7), "Deep")]));
        assert_eq!(text, "###### Deep\n");
    }

    #[test]
    fn emphasis_writes_commonmark_delimiters() {
        let (text, _) = markdown(&document(concat!(
            "<w:p><w:r><w:t>Plain </w:t></w:r>",
            "<w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r>",
            "<w:r><w:t> and </w:t></w:r>",
            "<w:r><w:rPr><w:i/></w:rPr><w:t>italic</w:t></w:r>",
            "<w:r><w:t> and </w:t></w:r>",
            "<w:r><w:rPr><w:b/><w:i/><w:strike/></w:rPr><w:t>all three</w:t></w:r></w:p>",
        )));
        assert_eq!(
            text,
            "Plain **bold** and *italic* and ~~***all three***~~\n"
        );
    }

    #[test]
    fn a_hyperlink_writes_an_inline_link_and_reports_a_dropped_tooltip() {
        let (text, report) = markdown(&document(concat!(
            "<w:p><w:hyperlink w:anchor=\"top\" w:tooltip=\"go up\">",
            "<w:r><w:t>Up</w:t></w:r></w:hyperlink></w:p>",
        )));
        assert_eq!(text, "[Up](#top)\n");
        assert!(
            features(&report).contains(&"markdown.hyperlink_tooltip".to_owned()),
            "a dropped tooltip must be reported: {:?}",
            features(&report)
        );
    }

    #[test]
    fn a_rectangular_table_writes_a_gfm_pipe_table() {
        let (text, report) = markdown(&document(concat!(
            "<w:tbl>",
            "<w:tr><w:tc><w:p><w:r><w:t>Name</w:t></w:r></w:p></w:tc>",
            "<w:tc><w:p><w:r><w:t>Count</w:t></w:r></w:p></w:tc></w:tr>",
            "<w:tr><w:tc><w:p><w:r><w:t>Widgets</w:t></w:r></w:p></w:tc>",
            "<w:tc><w:p><w:r><w:t>12</w:t></w:r></w:p></w:tc></w:tr>",
            "</w:tbl>",
        )));
        assert_eq!(
            text, "| Name | Count |\n| --- | --- |\n| Widgets | 12 |\n",
            "the delimiter row is second and the first row becomes the header, \
             because that is the only shape GFM has"
        );
        assert!(features(&report).contains(&"markdown.table_formatting".to_owned()));
        assert!(
            !features(&report).contains(&"markdown.table_structure".to_owned()),
            "a table that DID become a pipe table must not report a structure loss"
        );
    }

    #[test]
    fn a_ragged_table_is_refused_as_a_pipe_table_and_says_so() {
        // A pipe table is rectangular. Writing one from rows of different widths
        // would produce a file that looks like a table and is not, so the cells'
        // text is written in reading order and the loss is reported.
        let (text, report) = markdown(&document(concat!(
            "<w:tbl>",
            "<w:tr><w:tc><w:p><w:r><w:t>Wide</w:t></w:r></w:p></w:tc></w:tr>",
            "<w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc>",
            "<w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr>",
            "</w:tbl>",
        )));
        assert!(!text.contains('|'), "no pipe table was written: {text:?}");
        assert!(features(&report).contains(&"markdown.table_structure".to_owned()));
    }

    #[test]
    fn a_column_span_is_refused_even_when_every_row_holds_the_same_cell_count() {
        // The row-width check cannot see this one: both rows carry two CELLS and
        // the first row's first cell spans two GRID COLUMNS, so the table is
        // three columns wide in one row and two in the other. A pipe table has
        // no span, and the first draft of this guard used a ragged table — which
        // the width check refused for the wrong reason, so removing the span
        // check left every test green. That is why this case is its own test.
        let (text, report) = markdown(&document(concat!(
            "<w:tbl>",
            "<w:tr><w:tc><w:tcPr><w:gridSpan w:val=\"2\"/></w:tcPr>",
            "<w:p><w:r><w:t>Wide</w:t></w:r></w:p></w:tc>",
            "<w:tc><w:p><w:r><w:t>Narrow</w:t></w:r></w:p></w:tc></w:tr>",
            "<w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc>",
            "<w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr>",
            "</w:tbl>",
        )));
        assert!(!text.contains('|'), "no pipe table was written: {text:?}");
        assert!(features(&report).contains(&"markdown.table_structure".to_owned()));
    }

    #[test]
    fn a_vertical_merge_is_refused_even_when_the_grid_is_rectangular() {
        // Same reasoning as the column span, for the other merge axis.
        let (text, report) = markdown(&document(concat!(
            "<w:tbl>",
            "<w:tr><w:tc><w:tcPr><w:vMerge w:val=\"restart\"/></w:tcPr>",
            "<w:p><w:r><w:t>Tall</w:t></w:r></w:p></w:tc>",
            "<w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr>",
            "<w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>",
            "<w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr>",
            "</w:tbl>",
        )));
        assert!(!text.contains('|'), "no pipe table was written: {text:?}");
        assert!(features(&report).contains(&"markdown.table_structure".to_owned()));
    }

    #[test]
    fn a_cell_holding_more_than_one_block_is_refused_as_a_pipe_table() {
        // A GFM cell is a single line of inline content. Two paragraphs in a
        // cell cannot be one, so the table degrades rather than losing a
        // paragraph silently.
        let (text, report) = markdown(&document(concat!(
            "<w:tbl>",
            "<w:tr><w:tc><w:p><w:r><w:t>one</w:t></w:r></w:p>",
            "<w:p><w:r><w:t>two</w:t></w:r></w:p></w:tc>",
            "<w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr>",
            "</w:tbl>",
        )));
        assert!(!text.contains('|'), "no pipe table was written: {text:?}");
        assert!(features(&report).contains(&"markdown.table_structure".to_owned()));
        assert!(
            text.contains("one") && text.contains("two") && text.contains('a'),
            "every cell's text must still be written: {text:?}"
        );
    }

    #[test]
    fn text_is_escaped_so_a_document_cannot_inject_markup() {
        // The whole point: a document that reads `*not emphasis*` must export to
        // a file that still reads `*not emphasis*`, not to one that renders
        // italic. Over-escaping is correct; a contextual escaper is where these
        // serializers grow bugs.
        let (text, _) = markdown(&document(
            "<w:p><w:r><w:t>a * b _ c [d] e ` f</w:t></w:r></w:p>",
        ));
        assert_eq!(text, "a \\* b \\_ c \\[d\\] e \\` f\n");
    }

    #[test]
    fn a_pipe_inside_a_table_cell_is_escaped() {
        // GFM splits a row on `|` BEFORE inline parsing, so an unescaped pipe in
        // a cell silently adds a column and shifts every value after it.
        let (text, _) = markdown(&document(
            "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>a|b</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
        ));
        assert!(
            text.starts_with("| a\\|b |"),
            "the pipe must be escaped: {text:?}"
        );
    }

    #[test]
    fn a_link_target_with_a_space_is_wrapped_in_angle_brackets() {
        let (text, _) = markdown(&document(
            "<w:p><w:hyperlink w:anchor=\"a b\"><w:r><w:t>x</w:t></w:r></w:hyperlink></w:p>",
        ));
        assert_eq!(text, "[x](<#a%20b>)\n");
    }

    #[test]
    fn a_line_break_uses_the_backslash_hard_break() {
        // Not two trailing spaces: an editor that trims trailing whitespace
        // silently deletes the other spelling of the same thing.
        let (text, _) = markdown(&document(
            "<w:p><w:r><w:t>one</w:t><w:br/><w:t>two</w:t></w:r></w:p>",
        ));
        assert_eq!(text, "one\\\ntwo\n");
    }

    #[test]
    fn formatting_markdown_cannot_carry_is_reported_rather_than_dropped() {
        let (_, report) = markdown(&document(concat!(
            "<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr>",
            "<w:r><w:rPr><w:u w:val=\"single\"/><w:color w:val=\"FF0000\"/>",
            "<w:sz w:val=\"48\"/><w:vertAlign w:val=\"superscript\"/></w:rPr>",
            "<w:t>x</w:t></w:r></w:p>",
        )));
        let found = features(&report);
        for expected in [
            "markdown.paragraph_alignment",
            "markdown.underline",
            "markdown.run_colour",
            "markdown.run_font",
            "markdown.superscript_or_subscript",
        ] {
            assert!(
                found.contains(&expected.to_owned()),
                "{expected} missing from {found:?}"
            );
        }
    }

    #[test]
    fn every_reported_feature_is_in_the_declared_vocabulary() {
        // A feature name a host cannot look up is the same defect as a refusal
        // with no code. This is the list, and nothing may report outside it.
        let (_, report) = markdown(&document(concat!(
            "<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr>",
            "<w:r><w:tab/><w:t>x</w:t></w:r></w:p>",
            "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>",
            "<w:p/>",
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
    fn the_output_is_deterministic_and_ends_in_exactly_one_newline() {
        let source = document(
            "<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p/><w:p><w:r><w:t>two</w:t></w:r></w:p>",
        );
        let (first, _) = markdown(&source);
        let (second, _) = markdown(&source);
        assert_eq!(first, second);
        assert!(first.ends_with('\n'));
        assert!(!first.ends_with("\n\n"));
    }

    #[test]
    fn the_output_ceiling_refuses_rather_than_growing() {
        let source = document("<w:p><w:r><w:t>plenty of text here</w:t></w:r></w:p>");
        let resources = DocumentResources::default();
        let error = MarkdownAdapter::new(MarkdownLimits {
            max_output_bytes: 4,
            ..MarkdownLimits::default()
        })
        .export(ExportRequest {
            document: &source,
            resources: &resources,
            source: None,
            source_unchanged: false,
            mode: ExportMode::Semantic,
        })
        .expect_err("a four-byte ceiling must refuse");
        assert!(
            format!("{error}").contains("markdown_output_bytes"),
            "the refusal must name the limit: {error}"
        );
    }

    #[test]
    fn a_limit_above_its_hard_ceiling_is_refused() {
        let source = document("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
        let resources = DocumentResources::default();
        assert!(
            MarkdownAdapter::new(MarkdownLimits {
                max_output_bytes: MarkdownLimits::HARD_MAX_OUTPUT_BYTES + 1,
                ..MarkdownLimits::default()
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
        let error = MarkdownAdapter::default()
            .export(ExportRequest {
                document: &source,
                resources: &resources,
                source: None,
                source_unchanged: true,
                mode: ExportMode::ExactIfUnchanged,
            })
            .expect_err("markdown has no source bytes to return");
        assert!(
            format!("{error}").contains("no importer"),
            "the refusal must say why: {error}"
        );
    }

    #[test]
    fn the_builtin_registry_offers_markdown_to_save_and_never_to_open() {
        let registry = builtin_registry();
        let exports: Vec<&str> = registry
            .export_formats()
            .into_iter()
            .map(FormatId::as_str)
            .collect();
        assert!(
            exports.contains(&formats::MARKDOWN),
            "Markdown must be a save target: {exports:?}"
        );
        let descriptor = registry
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.id.as_str() == formats::MARKDOWN)
            .expect("the Markdown descriptor is registered");
        assert!(
            !descriptor.can_import,
            "there is no Markdown parser, so the picker must never offer it to open"
        );
        assert!(!descriptor.exact_if_unchanged);
        assert_eq!(descriptor.display_name, "Markdown");
        assert_eq!(descriptor.extensions, ["md", "markdown"]);
    }
}
