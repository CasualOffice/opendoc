//! The RTF semantic importer: one pass from control words to schema v1.

use std::collections::BTreeMap;

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::AbstractNumbering;
use casual_doc_model::v1::AbstractNumberingId;
use casual_doc_model::v1::Alignment;
use casual_doc_model::v1::Break;
use casual_doc_model::v1::BreakKind;
use casual_doc_model::v1::Color;
use casual_doc_model::v1::Definitions;
use casual_doc_model::v1::Document;
use casual_doc_model::v1::DocumentProperties;
use casual_doc_model::v1::Drawing;
use casual_doc_model::v1::Extent;
use casual_doc_model::v1::FontDescriptor;
use casual_doc_model::v1::FontFamilyKind;
use casual_doc_model::v1::FontName;
use casual_doc_model::v1::FontPitch;
use casual_doc_model::v1::FontRef;
use casual_doc_model::v1::HighlightColor;
use casual_doc_model::v1::Indentation;
use casual_doc_model::v1::InlineNode;
use casual_doc_model::v1::LevelJustification;
use casual_doc_model::v1::LineRule;
use casual_doc_model::v1::MediaId;
use casual_doc_model::v1::MediaReference;
use casual_doc_model::v1::NoBreakHyphen;
use casual_doc_model::v1::NumberingInstance;
use casual_doc_model::v1::NumberingInstanceId;
use casual_doc_model::v1::NumberingLevel;
use casual_doc_model::v1::NumberingRef;
use casual_doc_model::v1::PageMargins;
use casual_doc_model::v1::PageOrientation;
use casual_doc_model::v1::PageSize;
use casual_doc_model::v1::ParagraphProperties;
use casual_doc_model::v1::RgbColor;
use casual_doc_model::v1::RunProperties;
use casual_doc_model::v1::SectionBoundary;
use casual_doc_model::v1::SectionColumns;
use casual_doc_model::v1::SectionId;
use casual_doc_model::v1::SoftHyphen;
use casual_doc_model::v1::Spacing;
use casual_doc_model::v1::Tab;
use casual_doc_model::v1::TabAlignment;
use casual_doc_model::v1::TabLeader;
use casual_doc_model::v1::TabStop;
use casual_doc_model::v1::TableCellProperties;
use casual_doc_model::v1::TableRowProperties;
use casual_doc_model::v1::UnderlineStyle;
use casual_doc_model::v1::VerticalAlignment;

use crate::builder::{BodyBuilder, CellDefinition, next_id};
use crate::code_page::{self, CodePage};
use crate::lexer::{Lexer, Token, is_rtf};
use crate::limits::enforce;
use crate::report::{Losses, RtfCompatibilityReport, RtfModelOutcome};
use crate::tables::{
    ColorTable, FontTable, InfoCollector, InfoField, ListTables, PictureKind, PictureState,
    level_number_format,
};
use crate::{RtfError, RtfLimits};

/// One twip in English Metric Units.
const EMU_PER_TWIP: i64 = 635;
/// One pixel at 96 DPI in English Metric Units.
const EMU_PER_PIXEL_96DPI: i64 = 9_525;
/// The largest extent schema v1 accepts.
const MAX_EMU: i64 = 27_273_042_316_900;

/// The complete result of a successful RTF import.
#[derive(Debug)]
pub struct RtfImport {
    /// The normalized, validated document.
    pub document: Document,
    /// Decoded picture bytes keyed by the part name the model references.
    pub resources: BTreeMap<String, Vec<u8>>,
    /// Every construct that was degraded or dropped.
    pub report: RtfCompatibilityReport,
    /// The `\rtfN` version the stream declared.
    pub version: i32,
}

/// Returns whether the bytes carry an RTF signature, without importing them.
#[must_use]
pub fn probe_rtf(bytes: &[u8]) -> bool {
    is_rtf(bytes)
}

/// Imports one RTF byte stream under an explicit admission policy.
///
/// Import is atomic: on any lexical, bound, or model-validation failure no
/// document, resource, or partial report escapes.
///
/// # Errors
///
/// Returns [`RtfError::NotRtf`] when the bytes carry no signature,
/// [`RtfError::LimitExceeded`] when an admission bound is reached,
/// [`RtfError::Malformed`] for a structurally broken stream, and
/// [`RtfError::Model`] when the mapped content violates a model invariant.
pub fn import_rtf(bytes: &[u8], limits: RtfLimits) -> Result<RtfImport, RtfError> {
    limits.validate()?;
    enforce("rtf_input_bytes", bytes.len(), limits.max_input_bytes)?;
    if !is_rtf(bytes) {
        return Err(RtfError::NotRtf);
    }
    Importer::new(bytes, limits).run()
}

/// Where the bytes and control words of the current group are routed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Destination {
    /// Ordinary body content.
    Body,
    /// Consumed and discarded; the reason was reported when the group opened.
    Discard,
    /// `\fonttbl`
    FontTable,
    /// `\colortbl`
    ColorTable,
    /// `\info` and its fields.
    Info,
    /// `\pict`
    Picture,
    /// `\listtable`.
    ListTable,
    /// One `{\list …}` inside `\listtable`.
    ListEntry,
    /// One `{\listlevel …}` inside a `{\list …}`.
    ListLevel,
    /// `\leveltext` inside a `\listlevel`.
    LevelText,
    /// `\listoverridetable`
    ListOverrideTable,
    /// One `{\listoverride …}` inside `\listoverridetable`.
    ListOverrideEntry,
    /// `\*\generator`
    Generator,
}

/// Group-scoped formatting state: RTF restores all of it at `}`.
#[derive(Clone, Debug)]
struct State {
    destination: Destination,
    run: RunProperties,
    paragraph: ParagraphProperties,
    font_index: Option<i32>,
    unicode_skip: u32,
}

/// Page setup for the section currently being accumulated.
#[derive(Clone, Copy, Debug)]
struct SectionSetup {
    width_twips: i32,
    height_twips: i32,
    left_twips: i32,
    right_twips: i32,
    top_twips: i32,
    bottom_twips: i32,
    header_twips: Option<i32>,
    footer_twips: Option<i32>,
    gutter_twips: Option<i32>,
    landscape: bool,
    column_count: u16,
    column_space: Option<i32>,
    column_separator: bool,
    declared: bool,
}

impl Default for SectionSetup {
    fn default() -> Self {
        Self {
            width_twips: 12_240,
            height_twips: 15_840,
            left_twips: 1_440,
            right_twips: 1_440,
            top_twips: 1_440,
            bottom_twips: 1_440,
            header_twips: None,
            footer_twips: None,
            gutter_twips: None,
            landscape: false,
            column_count: 1,
            column_space: None,
            column_separator: false,
            declared: false,
        }
    }
}

/// Which cell border edge a `\clbrdrX` opened.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BorderEdgeSlot {
    Top,
    Start,
    Bottom,
    End,
}

use control_words::BorderTarget;

struct Importer<'a> {
    limits: RtfLimits,
    lexer: Lexer<'a>,
    ids: IdGenerator,
    losses: Losses,
    states: Vec<State>,
    body: BodyBuilder,
    fonts: FontTable,
    colors: ColorTable,
    lists: ListTables,
    info: InfoCollector,
    picture: Option<PictureState>,
    resources: BTreeMap<String, Vec<u8>>,
    media: Vec<(MediaId, MediaReference)>,
    sections: Vec<SectionBoundary>,
    setup: SectionSetup,
    default_code_page: u16,
    declared_code_page: Option<u16>,
    version: i32,
    default_font: Option<i32>,
    scalar_values: usize,
    picture_bytes_total: usize,
    picture_count: usize,
    pending_skip: u32,
    pending_high_surrogate: Option<u16>,
    at_group_start: bool,
    next_is_destination: bool,
    // Table row state, which RTF writes at body level rather than in a group.
    row_definitions: Vec<CellDefinition>,
    row_properties: TableRowProperties,
    pending_cell: TableCellProperties,
    pending_merge_continue: bool,
    pending_border: Option<(BorderTarget, BorderEdgeSlot)>,
    row_left_twips: i32,
    cell_cursor: usize,
    used_numbering: BTreeMap<usize, NumberingInstanceId>,
    numbering: Vec<(AbstractNumberingId, AbstractNumbering)>,
    instances: Vec<(NumberingInstanceId, NumberingInstance)>,
    pending_tab_alignment: TabAlignment,
    pending_tab_leader: Option<TabLeader>,
    line_spacing_value: Option<i32>,
    line_spacing_multiple: bool,
    current_ls: Option<i32>,
    current_ilvl: u8,
}

impl<'a> Importer<'a> {
    fn new(bytes: &'a [u8], limits: RtfLimits) -> Self {
        Self {
            limits,
            lexer: Lexer::new(bytes),
            ids: IdGenerator::new(namespace_of(bytes)),
            losses: Losses::default(),
            states: vec![State {
                destination: Destination::Body,
                run: RunProperties::default(),
                paragraph: ParagraphProperties::default(),
                font_index: None,
                unicode_skip: 1,
            }],
            body: BodyBuilder::default(),
            fonts: FontTable::default(),
            colors: ColorTable::default(),
            lists: ListTables::default(),
            info: InfoCollector::default(),
            picture: None,
            resources: BTreeMap::new(),
            media: Vec::new(),
            sections: Vec::new(),
            setup: SectionSetup::default(),
            default_code_page: 1252,
            declared_code_page: None,
            version: 1,
            default_font: None,
            scalar_values: 0,
            picture_bytes_total: 0,
            picture_count: 0,
            pending_skip: 0,
            pending_high_surrogate: None,
            at_group_start: false,
            next_is_destination: false,
            row_definitions: Vec::new(),
            row_properties: TableRowProperties::default(),
            pending_cell: TableCellProperties::default(),
            pending_merge_continue: false,
            pending_border: None,
            row_left_twips: 0,
            cell_cursor: 0,
            used_numbering: BTreeMap::new(),
            numbering: Vec::new(),
            instances: Vec::new(),
            pending_tab_alignment: TabAlignment::Start,
            pending_tab_leader: None,
            line_spacing_value: None,
            line_spacing_multiple: false,
            current_ls: None,
            current_ilvl: 0,
        }
    }

    fn state(&self) -> &State {
        self.states.last().expect("state stack is never empty")
    }

    fn state_mut(&mut self) -> &mut State {
        self.states.last_mut().expect("state stack is never empty")
    }

    fn run(mut self) -> Result<RtfImport, RtfError> {
        while let Some(token) = self.lexer.next_token()? {
            match token {
                Token::GroupStart => self.open_group()?,
                Token::GroupEnd => self.close_group()?,
                Token::ControlWord { name, parameter } => {
                    let at_start = std::mem::take(&mut self.at_group_start);
                    self.control_word(name, parameter, at_start)?;
                }
                Token::ControlSymbol(symbol) => {
                    let at_start = std::mem::take(&mut self.at_group_start);
                    self.control_symbol(symbol, at_start)?;
                }
                Token::HexByte(byte) => {
                    self.at_group_start = false;
                    if self.consume_skip(1) {
                        continue;
                    }
                    self.emit_source_byte(byte)?;
                }
                Token::Text(text) => {
                    self.at_group_start = false;
                    self.emit_text(text)?;
                }
            }
        }
        self.finish()
    }

    fn open_group(&mut self) -> Result<(), RtfError> {
        enforce(
            "rtf_group_depth",
            self.states.len() + 1,
            self.limits.max_group_depth,
        )?;
        // A group boundary cancels any pending `\uc` fallback run: the skip
        // counts characters, and swallowing a whole structural group because a
        // producer wrote `\u` before one would lose real content.
        self.pending_skip = 0;
        let mut inherited = self.state().clone();
        if inherited.destination == Destination::Picture {
            // A nested group inside `\pict` (`{\*\blipuid …}`) is metadata, not
            // payload; routing it to the picture would corrupt the bytes.
            inherited.destination = Destination::Discard;
        }
        self.states.push(inherited);
        self.at_group_start = true;
        self.next_is_destination = false;
        Ok(())
    }

    fn close_group(&mut self) -> Result<(), RtfError> {
        self.at_group_start = false;
        self.pending_skip = 0;
        let closing = self
            .states
            .pop()
            .ok_or(RtfError::Malformed {
                reason: "a closing brace has no matching opening brace",
            })?
            .destination;
        if self.states.is_empty() {
            // The root group closed. Push a fresh root so a stream with extra
            // trailing content still has somewhere to put it; the trailing
            // content itself is reported below.
            self.states.push(State {
                destination: Destination::Body,
                run: RunProperties::default(),
                paragraph: ParagraphProperties::default(),
                font_index: None,
                unicode_skip: 1,
            });
        }
        match closing {
            Destination::FontTable => self.fonts.commit(self.limits)?,
            Destination::Info | Destination::Generator => self.info.finish_field(),
            Destination::Picture => self.finish_picture()?,
            // Only the group that OPENED the list may end it. Ending it when
            // any descendant group closes discarded every `\listid`, which is
            // written after the `{\listlevel …}` groups, so no body `\ls`
            // could ever resolve.
            Destination::ListLevel => self.lists.end_level(),
            Destination::ListEntry | Destination::ListTable => {
                self.lists.end_list(self.limits)?;
            }
            Destination::ListOverrideEntry | Destination::ListOverrideTable => {
                self.lists.end_override();
            }
            Destination::LevelText => {}
            _ => {}
        }
        Ok(())
    }

    fn control_symbol(&mut self, symbol: u8, at_group_start: bool) -> Result<(), RtfError> {
        if symbol == b'*' {
            if at_group_start {
                self.next_is_destination = true;
                self.at_group_start = true;
            }
            return Ok(());
        }
        if self.consume_skip(1) {
            return Ok(());
        }
        match symbol {
            b'\\' | b'{' | b'}' => self.push_char(char::from(symbol)),
            b'~' => self.push_char('\u{00a0}'),
            b'_' => {
                let inline = InlineNode::NoBreakHyphen(NoBreakHyphen {
                    id: next_id(&mut self.ids)?,
                });
                self.push_inline(inline)
            }
            b'-' => {
                let inline = InlineNode::SoftHyphen(SoftHyphen {
                    id: next_id(&mut self.ids)?,
                });
                self.push_inline(inline)
            }
            b':' | b'|' => {
                // `\|` (formula character) and `\:` (subentry index) carry no
                // text in the normalized model.
                self.losses
                    .record("rtf.index-entry", RtfModelOutcome::Omitted, self.limits)
            }
            b';' => self.separator(),
            _ => self.losses.record_named(
                "rtf.control-symbol.unknown",
                Some(&char::from(symbol).to_string()),
                RtfModelOutcome::Omitted,
                self.limits,
            ),
        }
    }

    /// A `;` ends an entry in the font and colour tables and is text elsewhere.
    fn separator(&mut self) -> Result<(), RtfError> {
        match self.state().destination {
            Destination::FontTable => self.fonts.commit(self.limits),
            Destination::ColorTable => self.colors.commit(self.limits),
            _ => self.push_char(';'),
        }
    }

    fn emit_text(&mut self, text: &[u8]) -> Result<(), RtfError> {
        let mut rest = text;
        while self.pending_skip > 0 {
            let Some((_, tail)) = rest.split_first() else {
                return Ok(());
            };
            self.pending_skip -= 1;
            rest = tail;
        }
        for byte in rest {
            if *byte == b';' {
                self.separator()?;
                continue;
            }
            self.emit_source_byte(*byte)?;
        }
        Ok(())
    }

    /// Decodes one source byte through the code page in force and emits it.
    fn emit_source_byte(&mut self, byte: u8) -> Result<(), RtfError> {
        if self.state().destination == Destination::Picture {
            let limits = self.limits;
            if let Some(picture) = self.picture.as_mut() {
                picture.push_hex(&[byte], limits)?;
            }
            return Ok(());
        }
        if byte < 0x80 {
            return self.push_char(char::from(byte));
        }
        let page = self.active_code_page();
        match page {
            CodePageChoice::Single(page) => match page.decode(byte) {
                Some(character) => self.push_char(character),
                None => {
                    self.losses.record(
                        "rtf.text.undecodable-byte",
                        RtfModelOutcome::Degraded,
                        self.limits,
                    )?;
                    self.push_char('\u{fffd}')
                }
            },
            CodePageChoice::MultiByte => {
                self.losses.record(
                    "rtf.codepage.multibyte-unsupported",
                    RtfModelOutcome::Degraded,
                    self.limits,
                )?;
                self.push_char('\u{fffd}')
            }
            CodePageChoice::Unknown => {
                self.losses.record(
                    "rtf.codepage.unsupported",
                    RtfModelOutcome::Degraded,
                    self.limits,
                )?;
                self.push_char('\u{fffd}')
            }
        }
    }

    fn active_code_page(&self) -> CodePageChoice {
        let from_font = self
            .state()
            .font_index
            .or(self.default_font)
            .and_then(|index| self.fonts.get(index))
            .and_then(|font| font.charset)
            .and_then(charset_code_page);
        let number = from_font
            .or(self.declared_code_page)
            .unwrap_or(self.default_code_page);
        if let Some(page) = code_page::code_page(number) {
            return CodePageChoice::Single(page);
        }
        if is_multi_byte_code_page(number) {
            return CodePageChoice::MultiByte;
        }
        CodePageChoice::Unknown
    }

    fn push_char(&mut self, character: char) -> Result<(), RtfError> {
        match self.state().destination {
            Destination::Body => {
                self.scalar_values = self.scalar_values.saturating_add(1);
                enforce(
                    "rtf_text_scalar_values",
                    self.scalar_values,
                    self.limits.max_text_scalar_values,
                )?;
                let mut buffer = [0_u8; 4];
                let text = character.encode_utf8(&mut buffer);
                let properties = self.state().run.clone();
                self.body
                    .push_text(text, &properties, &mut self.ids, self.limits)
            }
            Destination::FontTable => {
                let mut buffer = [0_u8; 4];
                self.fonts.push_name(character.encode_utf8(&mut buffer));
                Ok(())
            }
            Destination::Info | Destination::Generator => {
                let mut buffer = [0_u8; 4];
                self.info.push(character.encode_utf8(&mut buffer));
                Ok(())
            }
            Destination::LevelText => {
                if let Some(level) = self.lists.level()
                    && level.level_text.len() + character.len_utf8() <= 255
                {
                    level.level_text.push(character);
                }
                Ok(())
            }
            Destination::Discard
            | Destination::ColorTable
            | Destination::Picture
            | Destination::ListTable
            | Destination::ListEntry
            | Destination::ListLevel
            | Destination::ListOverrideTable
            | Destination::ListOverrideEntry => Ok(()),
        }
    }

    fn push_inline(&mut self, inline: InlineNode) -> Result<(), RtfError> {
        if self.state().destination != Destination::Body {
            return Ok(());
        }
        self.body.push_inline(inline, &mut self.ids, self.limits)
    }

    /// Consumes one unit of a pending `\ucN` fallback run.
    fn consume_skip(&mut self, units: u32) -> bool {
        if self.pending_skip == 0 {
            return false;
        }
        self.pending_skip = self.pending_skip.saturating_sub(units);
        true
    }

    fn finish(mut self) -> Result<RtfImport, RtfError> {
        // A stream that ends on a lone high surrogate has one character the
        // reader was promised and never received. Dropping it here would be a
        // silent loss; it becomes a replacement character and a finding.
        if self.pending_high_surrogate.take().is_some() {
            self.losses.record(
                "rtf.text.unpaired-surrogate",
                RtfModelOutcome::Degraded,
                self.limits,
            )?;
            self.push_char('\u{fffd}')?;
        }
        let trailing = self.state().paragraph.clone();
        self.emit_section()?;
        self.info.finish_field();
        let mut definitions = Definitions::default();

        let blocks = self.body.finish(trailing, &mut self.ids, self.limits)?;

        for (id, reference) in self.media {
            definitions.media.insert(id, reference);
        }
        for (id, abstract_numbering) in self.numbering {
            definitions
                .abstract_numbering
                .insert(id, abstract_numbering);
        }
        for (id, instance) in self.instances {
            definitions.numbering.insert(id, instance);
        }
        for font in self.fonts.in_declaration_order() {
            if font.name.is_empty() {
                continue;
            }
            definitions.font_table.push(FontDescriptor {
                name: font.name.clone(),
                alt_name: None,
                panose1: None,
                charset: font.charset.map(|charset| format!("{charset:02X}")),
                family: font.family,
                pitch: font.pitch,
                sig: casual_doc_model::v1::FontSig::default(),
                not_true_type: false,
                embedded: casual_doc_model::v1::EmbeddedFontSet::default(),
            });
        }
        definitions.sections = std::mem::take(&mut self.sections);

        let document_id = next_id(&mut self.ids)?;
        let mut document =
            Document::new(document_id, blocks, definitions).map_err(|error| RtfError::Model {
                reason: error.to_string(),
            })?;
        let properties = DocumentProperties {
            core: self.info.core.clone(),
            app: self.info.app.clone(),
            custom: Vec::new(),
        };
        if !properties.is_empty() {
            document = document
                .with_properties(properties)
                .map_err(|error| RtfError::Model {
                    reason: error.to_string(),
                })?;
        }
        Ok(RtfImport {
            document,
            resources: self.resources,
            report: self.losses.finish(),
            version: self.version,
        })
    }

    fn emit_section(&mut self) -> Result<SectionId, RtfError> {
        if !self.setup.declared {
            self.losses.record(
                "rtf.section.defaulted",
                RtfModelOutcome::Degraded,
                self.limits,
            )?;
        }
        let setup = self.setup;
        let id = SectionId::new(next_id(&mut self.ids)?);
        let boundary = SectionBoundary {
            id,
            page_size: PageSize {
                width_twips: setup.width_twips.clamp(1, 31_680),
                height_twips: setup.height_twips.clamp(1, 31_680),
            },
            page_margins: PageMargins {
                top_twips: setup.top_twips.clamp(0, 31_680),
                bottom_twips: setup.bottom_twips.clamp(0, 31_680),
                start_twips: setup.left_twips.clamp(0, 31_680),
                end_twips: setup.right_twips.clamp(0, 31_680),
                header_twips: setup.header_twips.map(|value| value.clamp(0, 31_680)),
                footer_twips: setup.footer_twips.map(|value| value.clamp(0, 31_680)),
                gutter_twips: setup.gutter_twips.map(|value| value.clamp(0, 31_680)),
            },
            columns: SectionColumns {
                count: setup.column_count.clamp(1, 64),
                space_twips: setup.column_space.map(|value| value.clamp(0, 31_680)),
                separator: setup.column_separator.then_some(true),
                equal_width: None,
                columns: Vec::new(),
            },
            headers: Vec::new(),
            footers: Vec::new(),
            section_type: None,
            title_page: None,
            vertical_alignment: None,
            page_numbering: casual_doc_model::v1::PageNumbering::default(),
            doc_grid: casual_doc_model::v1::DocGrid::default(),
            orientation: setup
                .landscape
                .then_some(PageOrientation::Landscape)
                .or(Some(PageOrientation::Portrait)),
            paper_source: casual_doc_model::v1::PaperSource::default(),
            page_borders: casual_doc_model::v1::PageBorders::default(),
            line_numbering: casual_doc_model::v1::LineNumbering::default(),
            footnote_props: casual_doc_model::v1::NoteProperties::default(),
            endnote_props: casual_doc_model::v1::NoteProperties::default(),
            text_direction: None,
            bidi: false,
            section_change: None,
        };
        self.sections.push(boundary);
        Ok(id)
    }
}

/// Which decoder, if any, a declared code-page number selects.
#[derive(Clone, Copy, Debug)]
enum CodePageChoice {
    Single(CodePage),
    MultiByte,
    Unknown,
}

/// Maps a `\fcharset` value to the code page it implies.
const fn charset_code_page(charset: i32) -> Option<u16> {
    Some(match charset {
        0 => 1252,
        77 => 10_000,
        128 => 932,
        129 => 949,
        130 => 1361,
        134 => 936,
        136 => 950,
        161 => 1253,
        162 => 1254,
        163 => 1258,
        177 => 1255,
        178 => 1256,
        186 => 1257,
        204 => 1251,
        222 => 874,
        238 => 1250,
        254 => 437,
        255 => 850,
        // 1 (default) and 2 (symbol) deliberately do not override the
        // document code page: `\fcharset2` fonts carry glyph indices, not
        // characters, and guessing a text table for them invents characters.
        _ => return None,
    })
}

const fn is_multi_byte_code_page(number: u16) -> bool {
    matches!(number, 932 | 936 | 949 | 950 | 1361 | 54_936)
}

/// FNV-1a over the admitted bytes: the same document always gets the same ids,
/// and no host filename, clock, or iteration order participates.
fn namespace_of(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

mod control_words;
