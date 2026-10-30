//! The one builder the text-markup importers (Markdown, HTML) drive.
//!
//! # The pattern, named first (`SKILL` §8)
//!
//! This is a **builder over an event stream**: a front end (the CommonMark
//! pull parser, the bounded HTML tokenizer) turns its source into a flat
//! sequence of "open a heading", "push text", "open a link", "close a table
//! cell"; this builder turns that sequence into the normalized model. It is the
//! same split a SAX consumer has, and the reason for it is that Markdown and
//! HTML describe the SAME vocabulary (headings, lists, tables, emphasis, links,
//! code, quotes, rules, pictures) with different spellings. Two importers that
//! each built model nodes would be two places for a list's numbering or a
//! heading's style to disagree; one builder is one.
//!
//! The vocabulary is the paste path's (`webapp/src/clipboard.mjs`
//! `htmlToStructured`, then `casual-doc-wasm`'s `pasteExternalStructured`):
//! a heading takes Word's "Heading N" style with the outline level and size the
//! paste path gives it (`clipboard_outline::pasted_heading_style`), a list item
//! is a real numbered or bulleted paragraph at its nesting level, a table is a
//! real table. What a file can carry that a clipboard rarely does (code, quotes,
//! rules, footnotes, task boxes) uses the style names Word itself creates when
//! it opens HTML: "HTML Preformatted" for a code block, "HTML Code" for inline
//! code, "Quote" for a block quote.
//!
//! # Complexity
//!
//! Every builder call is O(1) amortized except closing a block, which is
//! O(inlines in it) to fold open link frames; the whole import is therefore
//! O(input). Nothing looks a node up by id inside a loop.

use std::collections::BTreeMap;

use casual_doc_model::v1::{
    AbstractNumbering, AbstractNumberingId, Alignment, BlockNode, BorderEdge, Break, BreakKind,
    Color, Definitions, Document, Drawing, Extent, ExternalTarget, FontName, FontRef, GridColumn,
    Hyperlink, HyperlinkTarget, Indentation, InlineNode, InlineSdt, InternalTarget,
    LevelJustification, LevelSuffix, MediaId, MediaReference, Note, NoteId, NoteKind,
    NoteNumberMark, NoteReference, NumberFormat, NumberingInstance, NumberingInstanceId,
    NumberingLevel, NumberingRef, Paragraph, ParagraphBorders, ParagraphProperties, RgbColor, Run,
    RunProperties, SdtCheckbox, SdtControlData, SdtControlKind, SdtProperties, Shading, Spacing,
    Style, StyleId, StyleKind, Tab, Table, TableBorders, TableCell, TableCellProperties,
    TableProperties, TableRow, TableRowProperties, TableWidth, VerticalAlignment, WidthType,
};
use casual_doc_model::{IdGenerator, NodeId};

use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, DocumentResources, FeatureLocation,
    ModelOutcome, RetentionOutcome,
};

/// The deepest list level the model's numbering carries (`w:ilvl` 0-8).
const MAX_LIST_LEVEL: u8 = 8;

/// Word's heading sizes in half-points, level 1 first: the paste path's
/// (`clipboard_outline::PASTED_HEADING_HALF_POINTS`), so a heading opened from
/// a file and one pasted from a web page are the same heading.
const HEADING_HALF_POINTS: [u32; 6] = [32, 26, 24, 22, 22, 22];

/// The text width a picture is fitted into: 6.5 inches, US Letter's text
/// column at Word's default margins, in EMU.
const MAX_PICTURE_WIDTH_EMU: i64 = 6 * 914_400 + 457_200;

/// EMU per CSS pixel at 96 DPI.
const EMU_PER_PIXEL: i64 = 9_525;

/// What an import produces: a model, its picture bytes, and what was reported.
pub(crate) type Imported = (Document, DocumentResources, Findings);

/// Findings recorded during an import, aggregated per feature.
#[derive(Debug, Default)]
pub(crate) struct Findings {
    entries: BTreeMap<String, (u32, ModelOutcome)>,
}

impl Findings {
    /// Records one occurrence of `feature`.
    pub(crate) fn record(&mut self, feature: &str, outcome: ModelOutcome) {
        self.entries
            .entry(feature.to_owned())
            .and_modify(|entry| entry.0 = entry.0.saturating_add(1))
            .or_insert((1, outcome));
    }

    pub(crate) fn finish(self) -> CompatibilityReport {
        let mut report = CompatibilityReport {
            entries: self
                .entries
                .into_iter()
                .map(
                    |(feature, (occurrences, model_outcome))| CompatibilityEntry {
                        location: FeatureLocation {
                            local_name: Some(feature.clone()),
                            ..FeatureLocation::default()
                        },
                        feature,
                        occurrences,
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

/// Character formatting turned on by the markup around a run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Format {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub underline: bool,
    pub code: bool,
    pub superscript: bool,
    pub subscript: bool,
}

/// Where a link goes, already checked against the safe-scheme list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Link {
    External(String),
    Internal(String),
}

/// What a URL in a source file may become.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum UrlKind {
    /// `#anchor`: a link inside the document.
    Internal(String),
    /// A scheme a reader can follow safely (`http`, `https`, `mailto`, `ftp`,
    /// `tel`) or a relative reference.
    Safe(String),
    /// `javascript:`, `vbscript:`, `data:`, `file:` and any scheme not on the
    /// list: never a live link.
    Unsafe,
}

/// Classifies a link destination. Mirrors the paste path's
/// `safeLinkTarget` (`webapp/src/clipboard.mjs`): the scheme decides, and an
/// unknown one is refused rather than guessed at. O(url).
pub(crate) fn classify_url(raw: &str) -> UrlKind {
    let url: String = raw
        .trim()
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '\u{200b}' | '\u{feff}'))
        .collect();
    if url.is_empty() {
        return UrlKind::Unsafe;
    }
    if let Some(anchor) = url.strip_prefix('#') {
        return if anchor.is_empty() {
            UrlKind::Unsafe
        } else {
            UrlKind::Internal(anchor.to_owned())
        };
    }
    let scheme_end = url.find(':');
    let first_delimiter = url.find(['/', '?', '#']);
    let has_scheme = scheme_end
        .is_some_and(|colon| colon > 0 && first_delimiter.is_none_or(|delim| colon < delim));
    if !has_scheme {
        // A relative reference (`docs/a.md`, `//host/x` is protocol-relative
        // and refused below as remote-ambiguous is NOT needed for a link: a
        // reader's browser resolves it against wherever the file came from).
        return UrlKind::Safe(url);
    }
    let scheme = url[..scheme_end.unwrap_or(0)].to_ascii_lowercase();
    if matches!(scheme.as_str(), "http" | "https" | "mailto" | "ftp" | "tel") {
        UrlKind::Safe(url)
    } else {
        UrlKind::Unsafe
    }
}

/// Whether a picture source names a network resource (never fetched here).
pub(crate) fn is_remote(src: &str) -> bool {
    let lower = src.trim().to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("ftp://")
        || lower.starts_with("//")
}

/// A decoded `data:` URI: its media type and bytes, when it is a base64 (or
/// percent-encoded) picture of a type the renderer paints. O(uri).
pub(crate) fn decode_data_uri(src: &str) -> Option<(String, Vec<u8>)> {
    let rest = src.trim().strip_prefix("data:")?;
    let (header, payload) = rest.split_once(',')?;
    let mut parts = header.split(';');
    let media_type = parts.next()?.trim().to_ascii_lowercase();
    let base64 = parts.any(|part| part.trim().eq_ignore_ascii_case("base64"));
    let bytes = if base64 {
        decode_base64(payload)?
    } else {
        percent_decode(payload)
    };
    let sniffed = sniff_picture(&bytes)?;
    // The bytes decide, not the declared type: a `data:image/png` that holds
    // a JPEG is still a picture, and one that holds HTML is not.
    let _ = media_type;
    Some((sniffed.0.to_owned(), bytes))
}

fn decode_base64(payload: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(payload.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in payload.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

fn percent_decode(payload: &str) -> Vec<u8> {
    let bytes = payload.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            out.push(high * 16 + low);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    out
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// The media type, file extension and pixel size of a PNG, JPEG or GIF, read
/// from its header; `None` for anything else. O(header) for PNG and GIF,
/// O(bytes) worst case for a JPEG whose frame header comes late.
pub(crate) fn sniff_picture(bytes: &[u8]) -> Option<(&'static str, &'static str, u32, u32)> {
    if bytes.len() >= 24 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && &bytes[12..16] == b"IHDR" {
        let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        return Some(("image/png", "png", width, height));
    }
    if bytes.len() >= 10 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        let width = u32::from(u16::from_le_bytes([bytes[6], bytes[7]]));
        let height = u32::from(u16::from_le_bytes([bytes[8], bytes[9]]));
        return Some(("image/gif", "gif", width, height));
    }
    if bytes.len() >= 4 && bytes[0] == 0xff && bytes[1] == 0xd8 {
        let mut index = 2;
        while index + 9 < bytes.len() {
            if bytes[index] != 0xff {
                index += 1;
                continue;
            }
            let marker = bytes[index + 1];
            let length = usize::from(u16::from_be_bytes([bytes[index + 2], bytes[index + 3]]));
            let frame = matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf);
            if frame {
                let height = u32::from(u16::from_be_bytes([bytes[index + 5], bytes[index + 6]]));
                let width = u32::from(u16::from_be_bytes([bytes[index + 7], bytes[index + 8]]));
                return Some(("image/jpeg", "jpeg", width, height));
            }
            if length < 2 {
                return None;
            }
            index += 2 + length;
        }
        return Some(("image/jpeg", "jpeg", 0, 0));
    }
    None
}

/// The paragraph shapes the builder knows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BlockKind {
    Body,
    Heading(u8),
    Code,
}

/// One open hyperlink inside the paragraph being built.
struct Frame {
    link: Option<Link>,
    inlines: Vec<InlineNode>,
}

/// The paragraph being built.
struct OpenParagraph {
    properties: ParagraphProperties,
    frames: Vec<Frame>,
}

/// One list level open around the cursor.
struct ListState {
    ordered: bool,
    start: u32,
    instance: Option<NumberingInstanceId>,
    /// Whether the current item has had its numbered (first) paragraph.
    item_numbered: bool,
}

/// A table being built.
struct TableState {
    alignments: Vec<Option<Alignment>>,
    rows: Vec<TableRow>,
    cells: Vec<TableCell>,
    row_header: bool,
    cell_alignment: Option<Alignment>,
    cell_span: u32,
    column: usize,
    /// How many containers were open when the table began: its cells are the
    /// containers above that.
    container_depth: usize,
}

/// Where finished blocks go.
enum Container {
    Body(Vec<BlockNode>),
    Note(String, Vec<BlockNode>),
    Cell(Vec<BlockNode>),
}

impl Container {
    fn blocks(&mut self) -> &mut Vec<BlockNode> {
        match self {
            Self::Body(blocks) | Self::Note(_, blocks) | Self::Cell(blocks) => blocks,
        }
    }
}

/// The styles the importers create, each made the first time it is used.
#[derive(Default)]
struct Styles {
    normal: Option<StyleId>,
    heading: [Option<StyleId>; 6],
    quote: Option<StyleId>,
    code_block: Option<StyleId>,
    code_char: Option<StyleId>,
    hyperlink: Option<StyleId>,
}

/// The builder. See the module documentation.
pub(crate) struct FlowBuilder {
    ids: IdGenerator,
    definitions: Definitions,
    resources: DocumentResources,
    pub(crate) findings: Findings,
    /// The format's finding prefix (`markdown.import`, `html.import`).
    prefix: &'static str,
    styles: Styles,
    containers: Vec<Container>,
    tables: Vec<TableState>,
    lists: Vec<ListState>,
    formats: Vec<Format>,
    links: Vec<Link>,
    paragraph: Option<OpenParagraph>,
    block: BlockKind,
    quote_depth: u32,
    alignment: Option<Alignment>,
    /// The next paragraph opened is the first of a note body.
    note_mark_pending: bool,
    notes: BTreeMap<String, NoteId>,
    note_order: Vec<String>,
    note_bodies: BTreeMap<String, Vec<BlockNode>>,
    pictures: usize,
    /// Whether a block-level boundary was asked for since the last text: a
    /// text push then opens a fresh paragraph rather than extending one.
    break_pending: bool,
}

impl FlowBuilder {
    /// A builder whose ids are namespaced by `namespace` (a hash of the
    /// source, so the same file opens to the same ids).
    pub(crate) fn new(namespace: u64, prefix: &'static str) -> Self {
        Self {
            ids: IdGenerator::new(namespace),
            definitions: Definitions::default(),
            resources: DocumentResources::default(),
            findings: Findings::default(),
            prefix,
            styles: Styles::default(),
            containers: vec![Container::Body(Vec::new())],
            tables: Vec::new(),
            lists: Vec::new(),
            formats: Vec::new(),
            links: Vec::new(),
            paragraph: None,
            block: BlockKind::Body,
            quote_depth: 0,
            alignment: None,
            note_mark_pending: false,
            notes: BTreeMap::new(),
            note_order: Vec::new(),
            note_bodies: BTreeMap::new(),
            pictures: 0,
            break_pending: false,
        }
    }

    /// Records `{prefix}.{name}`.
    pub(crate) fn report(&mut self, name: &str, outcome: ModelOutcome) {
        let feature = format!("{}.{name}", self.prefix);
        self.findings.record(&feature, outcome);
    }

    fn id(&mut self) -> Result<NodeId, AdapterError> {
        self.ids
            .next_id()
            .map_err(|error| AdapterError::new(format!("{} identity: {error}", self.prefix)))
    }

    // ----- styles ------------------------------------------------------------

    fn normal_style(&mut self) -> Result<StyleId, AdapterError> {
        if let Some(id) = self.styles.normal {
            return Ok(id);
        }
        let id = StyleId::new(self.id()?);
        let mut style = style(StyleKind::Paragraph, "Normal");
        style.is_default = true;
        self.definitions.styles.insert(id, style);
        self.styles.normal = Some(id);
        Ok(id)
    }

    fn heading_style(&mut self, level: u8) -> Result<StyleId, AdapterError> {
        let level = level.clamp(1, 6);
        let slot = usize::from(level - 1);
        if let Some(id) = self.styles.heading[slot] {
            return Ok(id);
        }
        let normal = self.normal_style()?;
        let id = StyleId::new(self.id()?);
        let mut heading = style(StyleKind::Paragraph, &format!("Heading {level}"));
        heading.based_on = Some(normal);
        heading.next = Some(normal);
        heading.ui_priority = Some(9);
        heading.unhide_when_used = true;
        heading.paragraph = Some(ParagraphProperties {
            outline_level: Some(level - 1),
            keep_next: Some(true),
            spacing: Some(Spacing {
                before_twips: Some(240),
                after_twips: Some(80),
                ..Spacing::default()
            }),
            ..ParagraphProperties::default()
        });
        heading.run = Some(RunProperties {
            bold: Some(true),
            size_half_points: Some(HEADING_HALF_POINTS[slot]),
            ..RunProperties::default()
        });
        self.definitions.styles.insert(id, heading);
        self.styles.heading[slot] = Some(id);
        Ok(id)
    }

    fn quote_style(&mut self) -> Result<StyleId, AdapterError> {
        if let Some(id) = self.styles.quote {
            return Ok(id);
        }
        let normal = self.normal_style()?;
        let id = StyleId::new(self.id()?);
        // Word's built-in "Quote" is italic and grey; a Markdown or HTML quote
        // is a left-indented block, so the indentation is the start edge's
        // rather than Word's centred 0.6" on both sides.
        let mut quote = style(StyleKind::Paragraph, "Quote");
        quote.based_on = Some(normal);
        quote.ui_priority = Some(29);
        quote.paragraph = Some(ParagraphProperties {
            indentation: Some(Indentation {
                start_twips: Some(720),
                end_twips: None,
                first_line_twips: None,
                hanging_twips: None,
            }),
            ..ParagraphProperties::default()
        });
        quote.run = Some(RunProperties {
            italic: Some(true),
            color: Some(Color::Rgb(RgbColor {
                r: 0x40,
                g: 0x40,
                b: 0x40,
            })),
            ..RunProperties::default()
        });
        self.definitions.styles.insert(id, quote);
        self.styles.quote = Some(id);
        Ok(id)
    }

    fn code_block_style(&mut self) -> Result<StyleId, AdapterError> {
        if let Some(id) = self.styles.code_block {
            return Ok(id);
        }
        let normal = self.normal_style()?;
        let id = StyleId::new(self.id()?);
        // Word's own name for the style it gives `<pre>` when it opens HTML.
        let mut code = style(StyleKind::Paragraph, "HTML Preformatted");
        code.based_on = Some(normal);
        code.ui_priority = Some(99);
        code.paragraph = Some(ParagraphProperties {
            spacing: Some(Spacing {
                before_twips: Some(0),
                after_twips: Some(0),
                ..Spacing::default()
            }),
            ..ParagraphProperties::default()
        });
        code.run = Some(monospace());
        self.definitions.styles.insert(id, code);
        self.styles.code_block = Some(id);
        Ok(id)
    }

    fn code_char_style(&mut self) -> Result<StyleId, AdapterError> {
        if let Some(id) = self.styles.code_char {
            return Ok(id);
        }
        let id = StyleId::new(self.id()?);
        // Word's own name for the style it gives `<code>` when it opens HTML.
        let mut code = style(StyleKind::Character, "HTML Code");
        code.ui_priority = Some(99);
        code.paragraph = None;
        code.run = Some(monospace());
        self.definitions.styles.insert(id, code);
        self.styles.code_char = Some(id);
        Ok(id)
    }

    fn hyperlink_style(&mut self) -> Result<StyleId, AdapterError> {
        if let Some(id) = self.styles.hyperlink {
            return Ok(id);
        }
        let id = StyleId::new(self.id()?);
        let mut link = style(StyleKind::Character, "Hyperlink");
        link.ui_priority = Some(99);
        link.unhide_when_used = true;
        link.paragraph = None;
        link.run = Some(RunProperties {
            underline: Some(true),
            color: Some(Color::Rgb(RgbColor {
                r: 0x05,
                g: 0x63,
                b: 0xc1,
            })),
            ..RunProperties::default()
        });
        self.definitions.styles.insert(id, link);
        self.styles.hyperlink = Some(id);
        Ok(id)
    }

    // ----- formatting --------------------------------------------------------

    /// The formatting in force (the innermost markup's).
    fn format(&self) -> Format {
        self.formats.last().copied().unwrap_or_default()
    }

    /// Opens a formatting scope: `change` turns properties on over the one in
    /// force.
    pub(crate) fn push_format(&mut self, change: impl FnOnce(&mut Format)) {
        let mut next = self.format();
        change(&mut next);
        self.formats.push(next);
    }

    /// Closes the innermost formatting scope.
    pub(crate) fn pop_format(&mut self) {
        self.formats.pop();
    }

    fn run_properties(&mut self) -> Result<RunProperties, AdapterError> {
        let format = self.format();
        let mut properties = RunProperties {
            bold: format.bold.then_some(true),
            italic: format.italic.then_some(true),
            strike: format.strike.then_some(true),
            underline: format.underline.then_some(true),
            vertical_alignment: if format.superscript {
                Some(VerticalAlignment::Superscript)
            } else if format.subscript {
                Some(VerticalAlignment::Subscript)
            } else {
                None
            },
            ..RunProperties::default()
        };
        // Inline code inside a code block is the block's own font already.
        if format.code && self.block != BlockKind::Code {
            properties.style_ref = Some(self.code_char_style()?);
        } else if !self.links.is_empty() {
            properties.style_ref = Some(self.hyperlink_style()?);
        }
        Ok(properties)
    }

    // ----- blocks ------------------------------------------------------------

    /// Starts a block of `kind`: the next text opens a paragraph of it.
    pub(crate) fn begin_block(&mut self, kind: BlockKind) -> Result<(), AdapterError> {
        self.end_paragraph()?;
        self.block = kind;
        self.break_pending = false;
        Ok(())
    }

    /// Ends the block in force; the next text opens an ordinary paragraph.
    pub(crate) fn end_block(&mut self) -> Result<(), AdapterError> {
        self.end_paragraph()?;
        self.block = BlockKind::Body;
        Ok(())
    }

    /// A paragraph boundary with no change of kind (HTML's `<p>`, `<div>`).
    pub(crate) fn paragraph_break(&mut self) -> Result<(), AdapterError> {
        if self.paragraph.is_some() {
            self.end_paragraph()?;
        }
        self.break_pending = false;
        Ok(())
    }

    /// The alignment the next paragraphs take (a table column's, an HTML
    /// `align`), or `None` to inherit.
    pub(crate) fn set_alignment(&mut self, alignment: Option<Alignment>) {
        self.alignment = alignment;
    }

    /// The alignment in force, so a scope that sets one can restore it.
    pub(crate) fn alignment(&self) -> Option<Alignment> {
        self.alignment
    }

    pub(crate) fn begin_quote(&mut self) -> Result<(), AdapterError> {
        self.end_paragraph()?;
        self.quote_depth += 1;
        Ok(())
    }

    pub(crate) fn end_quote(&mut self) -> Result<(), AdapterError> {
        self.end_paragraph()?;
        self.quote_depth = self.quote_depth.saturating_sub(1);
        Ok(())
    }

    pub(crate) fn begin_list(&mut self, ordered: bool, start: u32) -> Result<(), AdapterError> {
        self.end_paragraph()?;
        if self.lists.len() > usize::from(MAX_LIST_LEVEL) {
            self.report("list_level_clamped", ModelOutcome::Degraded);
        }
        self.lists.push(ListState {
            ordered,
            start: start.clamp(0, 32_767),
            instance: None,
            item_numbered: false,
        });
        Ok(())
    }

    pub(crate) fn end_list(&mut self) -> Result<(), AdapterError> {
        self.end_paragraph()?;
        self.lists.pop();
        Ok(())
    }

    pub(crate) fn begin_item(&mut self) -> Result<(), AdapterError> {
        self.end_paragraph()?;
        self.block = BlockKind::Body;
        if let Some(list) = self.lists.last_mut() {
            list.item_numbered = false;
        }
        Ok(())
    }

    pub(crate) fn end_item(&mut self) -> Result<(), AdapterError> {
        self.end_paragraph()
    }

    /// A thematic break: an empty paragraph whose bottom border is the rule,
    /// which is how Word draws one (AutoFormat's `---` + Enter does the same).
    pub(crate) fn rule(&mut self) -> Result<(), AdapterError> {
        self.end_paragraph()?;
        let properties = ParagraphProperties {
            borders: ParagraphBorders {
                bottom: Some(BorderEdge {
                    style: "single".to_owned(),
                    size_eighth_points: Some(6),
                    color: None,
                    theme_color: None,
                    space_points: Some(1),
                }),
                ..ParagraphBorders::default()
            }
            .into(),
            ..ParagraphProperties::default()
        };
        let paragraph = Paragraph {
            id: self.id()?,
            properties: properties.into(),
            inlines: Vec::new(),
        };
        self.push_block(BlockNode::Paragraph(paragraph));
        Ok(())
    }

    fn push_block(&mut self, block: BlockNode) {
        if let Some(container) = self.containers.last_mut() {
            container.blocks().push(block);
        }
    }

    fn ensure_paragraph(&mut self) -> Result<(), AdapterError> {
        if self.break_pending {
            self.end_paragraph()?;
            self.break_pending = false;
        }
        if self.paragraph.is_some() {
            return Ok(());
        }
        let mut properties = ParagraphProperties::default();
        match self.block {
            BlockKind::Heading(level) => properties.style_ref = Some(self.heading_style(level)?),
            BlockKind::Code => properties.style_ref = Some(self.code_block_style()?),
            BlockKind::Body => {
                if self.quote_depth > 0 {
                    properties.style_ref = Some(self.quote_style()?);
                }
            }
        }
        if self.quote_depth > 1 {
            properties.indentation = Some(Indentation {
                start_twips: Some(720 * i32::try_from(self.quote_depth).unwrap_or(1).min(20)),
                end_twips: None,
                first_line_twips: None,
                hanging_twips: None,
            });
        }
        let depth = self.lists.len();
        if depth > 0 {
            let level = u8::try_from(depth - 1)
                .unwrap_or(MAX_LIST_LEVEL)
                .min(MAX_LIST_LEVEL);
            let needs_number = self.lists.last().is_some_and(|list| !list.item_numbered);
            if needs_number {
                let instance = self.list_instance(level)?;
                properties.numbering = Some(NumberingRef { instance, level });
                if let Some(list) = self.lists.last_mut() {
                    list.item_numbered = true;
                }
            } else {
                // A second paragraph in a list item continues it, unnumbered,
                // at the item's text indent (Word's Shift+Enter-free shape).
                properties.indentation = Some(Indentation {
                    start_twips: Some(720 + i32::from(level) * 360),
                    end_twips: None,
                    first_line_twips: None,
                    hanging_twips: None,
                });
            }
        }
        properties.alignment = self.alignment;
        let mut inlines = Vec::new();
        if self.note_mark_pending {
            self.note_mark_pending = false;
            inlines.push(InlineNode::NoteNumberMark(NoteNumberMark {
                id: self.id()?,
                kind: NoteKind::Footnote,
                properties: RunProperties {
                    vertical_alignment: Some(VerticalAlignment::Superscript),
                    ..RunProperties::default()
                }
                .into(),
            }));
            inlines.push(InlineNode::Run(Run {
                id: self.id()?,
                properties: RunProperties::default().into(),
                text: " ".to_owned(),
            }));
        }
        let mut frames = vec![Frame {
            link: None,
            inlines,
        }];
        // A link that spans a paragraph boundary (HTML allows `<a>` around
        // blocks) continues in the next paragraph rather than being lost.
        for link in self.links.clone() {
            frames.push(Frame {
                link: Some(link),
                inlines: Vec::new(),
            });
        }
        self.paragraph = Some(OpenParagraph { properties, frames });
        Ok(())
    }

    /// The numbering instance for the list at `level`, made on first use: one
    /// definition per list, so each list counts from its own start the way
    /// Word's "Restart at" does, and a nested list of the other kind is its
    /// own definition. O(levels).
    fn list_instance(&mut self, level: u8) -> Result<NumberingInstanceId, AdapterError> {
        if let Some(instance) = self.lists.last().and_then(|list| list.instance) {
            return Ok(instance);
        }
        let (ordered, start) = self
            .lists
            .last()
            .map_or((false, 1), |list| (list.ordered, list.start));
        let levels = (0..=MAX_LIST_LEVEL)
            .map(|n| {
                let mut built = list_level(ordered, n);
                if n == level {
                    built.start = u16::try_from(start).unwrap_or(1);
                }
                built
            })
            .collect();
        let abstract_id = AbstractNumberingId::new(self.id()?);
        let instance = NumberingInstanceId::new(self.id()?);
        self.definitions.abstract_numbering.insert(
            abstract_id,
            AbstractNumbering {
                levels,
                multi_level_type: None,
                num_style_link: None,
                style_link: None,
            },
        );
        self.definitions.numbering.insert(
            instance,
            NumberingInstance {
                abstract_ref: abstract_id,
                overrides: Vec::new(),
            },
        );
        if let Some(list) = self.lists.last_mut() {
            list.instance = Some(instance);
        }
        Ok(instance)
    }

    /// Closes the paragraph being built, folding any open link frames.
    fn end_paragraph(&mut self) -> Result<(), AdapterError> {
        let Some(mut open) = self.paragraph.take() else {
            return Ok(());
        };
        while open.frames.len() > 1 {
            let frame = open.frames.pop().expect("more than one frame");
            let node = self.wrap_link(frame)?;
            if let (Some(node), Some(parent)) = (node, open.frames.last_mut()) {
                parent.inlines.push(node);
            }
        }
        let inlines = open
            .frames
            .pop()
            .map(|frame| frame.inlines)
            .unwrap_or_default();
        let paragraph = Paragraph {
            id: self.id()?,
            properties: open.properties.into(),
            inlines,
        };
        self.push_block(BlockNode::Paragraph(paragraph));
        Ok(())
    }

    fn wrap_link(&mut self, frame: Frame) -> Result<Option<InlineNode>, AdapterError> {
        if frame.inlines.is_empty() {
            return Ok(None);
        }
        let Some(link) = frame.link else {
            return Ok(None);
        };
        let target = match link {
            Link::External(url) => HyperlinkTarget::External(ExternalTarget { url, anchor: None }),
            Link::Internal(anchor) => HyperlinkTarget::Internal(InternalTarget { anchor }),
        };
        Ok(Some(InlineNode::Hyperlink(Box::new(Hyperlink {
            id: self.id()?,
            target,
            tooltip: None,
            inlines: frame.inlines,
            history: true,
        }))))
    }

    // ----- inlines -----------------------------------------------------------

    fn push_inline(&mut self, node: InlineNode) -> Result<(), AdapterError> {
        self.ensure_paragraph()?;
        if let Some(frame) = self
            .paragraph
            .as_mut()
            .and_then(|open| open.frames.last_mut())
        {
            frame.inlines.push(node);
        }
        Ok(())
    }

    /// Text in the formatting in force. In a code block a newline ends the
    /// line's paragraph; elsewhere the front end has already folded newlines.
    /// Tabs become tab nodes; other control characters are dropped (the
    /// model refuses them in a run). O(text).
    pub(crate) fn text(&mut self, text: &str) -> Result<(), AdapterError> {
        if self.block == BlockKind::Code {
            // One paragraph per line, an empty line included: a blank line
            // inside a fence is content. The front end strips the fence's
            // final newline, so `a\nb` is two paragraphs, not three.
            for (index, line) in text.split('\n').enumerate() {
                if index > 0 {
                    self.end_paragraph()?;
                }
                self.ensure_paragraph()?;
                let line = line.strip_suffix('\r').unwrap_or(line);
                if !line.is_empty() {
                    self.plain(line)?;
                }
            }
            return Ok(());
        }
        self.plain(text)
    }

    fn plain(&mut self, text: &str) -> Result<(), AdapterError> {
        let mut segment = String::new();
        for character in text.chars() {
            match character {
                '\t' => {
                    self.flush_run(&mut segment)?;
                    let tab = InlineNode::Tab(Tab { id: self.id()? });
                    self.push_inline(tab)?;
                }
                '\n' | '\r' => segment.push(' '),
                c if c.is_control() => {}
                c => segment.push(c),
            }
        }
        self.flush_run(&mut segment)
    }

    fn flush_run(&mut self, segment: &mut String) -> Result<(), AdapterError> {
        if segment.is_empty() {
            return Ok(());
        }
        let text = std::mem::take(segment);
        let properties = self.run_properties()?;
        self.ensure_paragraph()?;
        // Adjacent text in the same formatting is one run, as Word would save it.
        if let Some(frame) = self
            .paragraph
            .as_mut()
            .and_then(|open| open.frames.last_mut())
            && let Some(InlineNode::Run(last)) = frame.inlines.last_mut()
            && *last.properties == properties
        {
            last.text.push_str(&text);
            return Ok(());
        }
        let run = InlineNode::Run(Run {
            id: self.id()?,
            properties: properties.into(),
            text,
        });
        self.push_inline(run)
    }

    /// A line break inside the paragraph.
    pub(crate) fn line_break(&mut self) -> Result<(), AdapterError> {
        let node = InlineNode::Break(Break {
            id: self.id()?,
            kind: BreakKind::Line,
        });
        self.push_inline(node)
    }

    /// Opens a hyperlink scope.
    pub(crate) fn begin_link(&mut self, link: Link) {
        self.links.push(link.clone());
        if let Some(open) = self.paragraph.as_mut() {
            open.frames.push(Frame {
                link: Some(link),
                inlines: Vec::new(),
            });
        }
    }

    /// Closes the innermost hyperlink scope.
    pub(crate) fn end_link(&mut self) -> Result<(), AdapterError> {
        self.links.pop();
        let Some(open) = self.paragraph.as_mut() else {
            return Ok(());
        };
        if open.frames.len() <= 1 {
            return Ok(());
        }
        let frame = open.frames.pop().expect("a link frame");
        if let Some(node) = self.wrap_link(frame)?
            && let Some(parent) = self
                .paragraph
                .as_mut()
                .and_then(|open| open.frames.last_mut())
        {
            parent.inlines.push(node);
        }
        Ok(())
    }

    /// A task-list box: a checkbox content control, which Word toggles with a
    /// click and saves as `w14:checkbox`.
    pub(crate) fn checkbox(&mut self, checked: bool) -> Result<(), AdapterError> {
        let glyph = if checked { "\u{2612}" } else { "\u{2610}" };
        let run = InlineNode::Run(Run {
            id: self.id()?,
            properties: RunProperties::default().into(),
            text: glyph.to_owned(),
        });
        let control = InlineNode::Sdt(Box::new(InlineSdt {
            id: self.id()?,
            properties: SdtProperties {
                control_kind: Some(SdtControlKind::Checkbox),
                data: Some(SdtControlData::Checkbox(SdtCheckbox {
                    checked,
                    checked_state: None,
                    unchecked_state: None,
                })),
                ..SdtProperties::default()
            },
            inlines: vec![run],
        }));
        self.push_inline(control)?;
        let mut space = " ".to_owned();
        self.flush_run(&mut space)
    }

    /// A picture from bytes the importer already holds (a `data:` URI or a
    /// host-resolved file). Returns `false`, having placed nothing, when the
    /// bytes are not a picture the renderer paints. O(bytes).
    pub(crate) fn picture(&mut self, bytes: Vec<u8>, alt: &str) -> Result<bool, AdapterError> {
        let Some((media_type, extension, width, height)) = sniff_picture(&bytes) else {
            return Ok(false);
        };
        self.pictures += 1;
        let part_name = format!("media/image{}.{extension}", self.pictures);
        let media = MediaId::new(self.id()?);
        self.definitions.media.insert(
            media,
            MediaReference {
                relationship_id: format!("rIdImage{}", self.pictures),
                media_type: media_type.to_owned(),
                part_name: part_name.clone(),
            },
        );
        self.resources.insert(part_name, bytes);
        let extent = (width > 0 && height > 0).then(|| {
            let mut width_emu = i64::from(width) * EMU_PER_PIXEL;
            let mut height_emu = i64::from(height) * EMU_PER_PIXEL;
            if width_emu > MAX_PICTURE_WIDTH_EMU {
                height_emu = (height_emu * MAX_PICTURE_WIDTH_EMU / width_emu).max(1);
                width_emu = MAX_PICTURE_WIDTH_EMU;
            }
            Extent {
                width_emu,
                height_emu,
            }
        });
        let alt = alt.trim();
        let drawing = InlineNode::Drawing(Box::new(Drawing {
            id: self.id()?,
            media,
            extent,
            descr: (!alt.is_empty()).then(|| alt.to_owned()),
            crop: None,
            opacity: None,
            hyperlink: None,
            border: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        }));
        self.push_inline(drawing)?;
        Ok(true)
    }

    // ----- footnotes ---------------------------------------------------------

    fn note_id(&mut self, label: &str) -> Result<NoteId, AdapterError> {
        if let Some(id) = self.notes.get(label) {
            return Ok(*id);
        }
        let id = NoteId::new(self.id()?);
        self.notes.insert(label.to_owned(), id);
        self.note_order.push(label.to_owned());
        Ok(id)
    }

    /// A footnote reference mark at the cursor.
    pub(crate) fn note_reference(&mut self, label: &str) -> Result<(), AdapterError> {
        let note = self.note_id(label)?;
        let node = InlineNode::NoteReference(NoteReference {
            id: self.id()?,
            kind: NoteKind::Footnote,
            note,
        });
        self.push_inline(node)
    }

    /// Opens a footnote body: blocks go into it until [`Self::end_note`].
    pub(crate) fn begin_note(&mut self, label: &str) -> Result<(), AdapterError> {
        self.end_paragraph()?;
        self.containers
            .push(Container::Note(label.to_owned(), Vec::new()));
        self.note_mark_pending = true;
        Ok(())
    }

    pub(crate) fn end_note(&mut self) -> Result<(), AdapterError> {
        self.end_paragraph()?;
        self.note_mark_pending = false;
        if matches!(self.containers.last(), Some(Container::Note(..)))
            && let Some(Container::Note(label, blocks)) = self.containers.pop()
        {
            self.note_bodies.entry(label).or_insert(blocks);
        }
        Ok(())
    }

    // ----- tables ------------------------------------------------------------

    pub(crate) fn begin_table(
        &mut self,
        alignments: Vec<Option<Alignment>>,
    ) -> Result<(), AdapterError> {
        self.end_paragraph()?;
        self.tables.push(TableState {
            alignments,
            rows: Vec::new(),
            cells: Vec::new(),
            row_header: false,
            cell_alignment: None,
            cell_span: 1,
            column: 0,
            container_depth: self.containers.len(),
        });
        Ok(())
    }

    pub(crate) fn in_table(&self) -> bool {
        !self.tables.is_empty()
    }

    pub(crate) fn in_cell(&self) -> bool {
        matches!(self.containers.last(), Some(Container::Cell(_)))
            && self
                .tables
                .last()
                .is_some_and(|table| table.container_depth + 1 == self.containers.len())
    }

    pub(crate) fn in_row(&self) -> bool {
        self.tables
            .last()
            .is_some_and(|table| !table.cells.is_empty())
    }

    pub(crate) fn begin_row(&mut self, header: bool) -> Result<(), AdapterError> {
        if let Some(table) = self.tables.last_mut() {
            table.row_header = header;
            table.column = 0;
        }
        Ok(())
    }

    /// Marks the row being built as the table's header (HTML's `<th>` row
    /// with no `<thead>`).
    pub(crate) fn mark_row_header(&mut self) {
        if let Some(table) = self.tables.last_mut() {
            table.row_header = true;
        }
    }

    pub(crate) fn end_row(&mut self) -> Result<(), AdapterError> {
        if self.in_cell() {
            self.end_cell()?;
        }
        let id = self.id()?;
        let Some(table) = self.tables.last_mut() else {
            return Ok(());
        };
        if table.cells.is_empty() {
            return Ok(());
        }
        let cells = std::mem::take(&mut table.cells);
        table.rows.push(TableRow {
            id,
            properties: TableRowProperties {
                header: table.row_header,
                ..TableRowProperties::default()
            },
            cells,
        });
        Ok(())
    }

    /// Opens a cell; `alignment` overrides the column's, `span` is a column
    /// span (HTML `colspan`).
    pub(crate) fn begin_cell(
        &mut self,
        alignment: Option<Alignment>,
        span: u32,
    ) -> Result<(), AdapterError> {
        self.end_paragraph()?;
        let Some(table) = self.tables.last_mut() else {
            return Ok(());
        };
        let column_alignment = table.alignments.get(table.column).copied().flatten();
        table.cell_alignment = alignment.or(column_alignment);
        table.cell_span = span.clamp(1, 63);
        let alignment = table.cell_alignment;
        self.containers.push(Container::Cell(Vec::new()));
        self.alignment = alignment;
        Ok(())
    }

    pub(crate) fn end_cell(&mut self) -> Result<(), AdapterError> {
        self.end_paragraph()?;
        if !self.in_cell() {
            return Ok(());
        }
        let Some(Container::Cell(mut blocks)) = self.containers.pop() else {
            return Ok(());
        };
        let alignment = self.tables.last().and_then(|table| table.cell_alignment);
        if blocks.is_empty() {
            let properties = ParagraphProperties {
                alignment,
                ..ParagraphProperties::default()
            };
            blocks.push(BlockNode::Paragraph(Paragraph {
                id: self.id()?,
                properties: properties.into(),
                inlines: Vec::new(),
            }));
        }
        let id = self.id()?;
        self.alignment = None;
        if let Some(table) = self.tables.last_mut() {
            let span = table.cell_span;
            table.column += span as usize;
            table.cells.push(TableCell {
                id,
                properties: TableCellProperties {
                    grid_span: (span > 1).then_some(span),
                    ..TableCellProperties::default()
                },
                blocks,
            });
        }
        Ok(())
    }

    pub(crate) fn end_table(&mut self) -> Result<(), AdapterError> {
        if self.in_cell() {
            self.end_cell()?;
        }
        if self.in_row() {
            self.end_row()?;
        }
        let Some(table) = self.tables.pop() else {
            return Ok(());
        };
        if table.rows.is_empty() {
            return Ok(());
        }
        let columns = table
            .rows
            .iter()
            .map(|row| {
                row.cells
                    .iter()
                    .map(|cell| cell.properties.grid_span.unwrap_or(1) as usize)
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(1)
            .max(1);
        let mut rows = table.rows;
        // A short row is padded to the grid, as GFM pads it and as the paste
        // path's engine half does: the model's table is rectangular.
        for row in &mut rows {
            let width: usize = row
                .cells
                .iter()
                .map(|cell| cell.properties.grid_span.unwrap_or(1) as usize)
                .sum();
            for _ in width..columns {
                let paragraph = Paragraph {
                    id: self.id()?,
                    properties: ParagraphProperties::default().into(),
                    inlines: Vec::new(),
                };
                row.cells.push(TableCell {
                    id: self.id()?,
                    properties: TableCellProperties::default(),
                    blocks: vec![BlockNode::Paragraph(paragraph)],
                });
            }
        }
        let column_twips = i32::try_from(9_360 / columns).unwrap_or(1).max(1);
        let border = BorderEdge {
            style: "single".to_owned(),
            size_eighth_points: Some(4),
            color: None,
            theme_color: None,
            space_points: None,
        };
        let table = Table {
            id: self.id()?,
            grid: (0..columns)
                .map(|_| GridColumn {
                    width_twips: Some(column_twips),
                })
                .collect(),
            grid_change: None,
            properties: TableProperties {
                width: Some(TableWidth {
                    value: 5_000,
                    width_type: WidthType::Pct,
                }),
                borders: TableBorders {
                    top: Some(border.clone()),
                    start: Some(border.clone()),
                    bottom: Some(border.clone()),
                    end: Some(border.clone()),
                    inside_h: Some(border.clone()),
                    inside_v: Some(border),
                },
                ..TableProperties::default()
            },
            rows,
        };
        self.push_block(BlockNode::Table(Box::new(table)));
        Ok(())
    }

    // ----- finish ------------------------------------------------------------

    /// Closes everything still open and validates the document.
    pub(crate) fn finish(mut self) -> Result<Imported, AdapterError> {
        loop {
            self.end_paragraph()?;
            let depth = self.containers.len();
            if self
                .tables
                .last()
                .is_some_and(|table| table.container_depth == depth)
            {
                self.end_table()?;
                continue;
            }
            match self.containers.last() {
                Some(Container::Cell(_)) => self.end_cell()?,
                Some(Container::Note(..)) => self.end_note()?,
                _ => break,
            }
        }
        let mut body = match self.containers.pop() {
            Some(Container::Body(blocks)) => blocks,
            _ => Vec::new(),
        };
        // Notes: every reference gets a body. A reference with no definition
        // gets an empty one (its number still prints, and the reader can fill
        // it), and a definition nobody references is reported, not invented
        // a reference for.
        for label in std::mem::take(&mut self.note_order) {
            let id = self.notes[&label];
            let blocks = match self.note_bodies.remove(&label) {
                Some(blocks) if !blocks.is_empty() => blocks,
                _ => {
                    self.report("footnote_undefined", ModelOutcome::Degraded);
                    let mark = InlineNode::NoteNumberMark(NoteNumberMark {
                        id: self.id()?,
                        kind: NoteKind::Footnote,
                        properties: RunProperties {
                            vertical_alignment: Some(VerticalAlignment::Superscript),
                            ..RunProperties::default()
                        }
                        .into(),
                    });
                    vec![BlockNode::Paragraph(Paragraph {
                        id: self.id()?,
                        properties: ParagraphProperties::default().into(),
                        inlines: vec![mark],
                    })]
                }
            };
            self.definitions.footnotes.insert(id, Note { blocks });
        }
        for _ in 0..self.note_bodies.len() {
            self.report("footnote_unreferenced", ModelOutcome::Omitted);
        }
        if body.is_empty() {
            body.push(BlockNode::Paragraph(Paragraph {
                id: self.id()?,
                properties: ParagraphProperties::default().into(),
                inlines: Vec::new(),
            }));
        }
        let id = self.id()?;
        let document = Document::new(id, body, self.definitions)
            .map_err(|error| AdapterError::new(format!("{} model: {error}", self.prefix)))?;
        Ok((document, self.resources, self.findings))
    }
}

/// A paragraph or character style with nothing set but its name.
fn style(kind: StyleKind, name: &str) -> Style {
    Style {
        kind,
        is_default: false,
        name: Some(name.to_owned()),
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
        paragraph: Some(ParagraphProperties::default()),
        run: None,
        table: None,
        table_row: None,
        table_cell: None,
        conditional: Vec::new(),
        custom_style: false,
    }
}

/// Word's code face: Courier New at 10pt, on every script slot.
fn monospace() -> RunProperties {
    let face = Some(FontRef::Named(FontName {
        name: "Courier New".to_owned(),
    }));
    RunProperties {
        font_ref: face.clone(),
        font_ref_h_ansi: face.clone(),
        font_ref_cs: face,
        size_half_points: Some(20),
        size_complex_half_points: Some(20),
        shading: Shading::default(),
        ..RunProperties::default()
    }
}

/// One level of the bullet or decimal list the editor creates — the same
/// shape as `casual_doc_command::lists::level`, so an imported list and one
/// made with the ribbon's list button indent and number identically. Copied,
/// not depended on: `casual-doc-io` is linked by the read-only viewer, which
/// must not carry the editing crates. O(level).
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
        picture_bullet: None,
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
        lvl_restart: None,
        pstyle: None,
        template_code: None,
        tentative: false,
    }
}

/// FNV-1a over the source, the id namespace (the plain-text importer's).
pub(crate) fn namespace_of(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
