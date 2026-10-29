// SPDX-License-Identifier: Apache-2.0

//! Tables of contents and tables of figures: generating one with the settings
//! Word's and ONLYOFFICE's dialogs offer, the two updates, removing one as a
//! unit, and marking a paragraph at a contents level (`docs/153` RM-25).
//!
//! # The settings, and where each one lives
//!
//! Both rivals' dialogs carry the same seven choices — show page numbers, right
//! align them, the tab leader, hyperlink the entries, how many levels, build
//! from outline levels or from named styles, and a format (ONLYOFFICE's
//! `DE.Views.TableOfContentsSettings`, `web-apps/apps/documenteditor/main/app/
//! view/TableOfContentsSettings.js:157-247`). An update must keep every one of
//! them, so each is stored where Word stores it and read back from there:
//!
//! | Choice | Stored as | Read back from |
//! | --- | --- | --- |
//! | levels | `\o "1-n"` | the instruction |
//! | from styles | `\t "Style,level,…"` | the instruction |
//! | hyperlinks | `\h` | the instruction |
//! | no page numbers | `\n` | the instruction |
//! | captions of a label | `\c "Label"` / `\a "Label"` | the instruction |
//! | right align, leader | the entries' tab stop | the first entry |
//! | format | the `TOC n` style definitions | the style table |
//!
//! So nothing about a generated table lives anywhere but the document, and a
//! Word-authored table reopens with its own settings.
//!
//! # Hyperlinked entries, and what they cost
//!
//! `\h` wraps each entry in a link to a `_Toc` bookmark on its heading. A
//! heading that already carries one (every Word-authored contents leaves them)
//! is reused, so an update plants nothing. A heading that has none gets one
//! `CreateBookmark`, and each of those re-validates the document, so the first
//! hyperlinked insert costs **O(new bookmarks × document)**. That is the honest
//! price of the one encoding Word reads: no operation plants several bookmarks
//! at once, and adding one would widen the closed op set (ADR-030 I2) for a
//! one-off gesture. It is paid once, on a deliberate gesture, never on a
//! keystroke; the default stays unlinked, as before.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use casual_doc_edit::table_structure::MintId;
use casual_doc_edit::{Operation, Pos, body_field_range_blocks};
// Its own line (rustfmt is set to Preserve; a shared `use` list is where
// parallel lanes collide).
use casual_doc_edit::refused;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, BookmarkId, Color, Document, FieldKind, FieldRange, FieldRangeEnd, FieldRangeId,
    FieldRangeStart, FieldUpdateState, Hyperlink, HyperlinkTarget, Indentation, InlineNode,
    InternalTarget, Paragraph, ParagraphProperties, Run, RunProperties, Spacing, Style, StyleId,
    StyleKind, Tab, TabAlignment, TabLeader, TabStop, ThemeColor, ThemeColorRef, node_plain_text,
    visit_paragraphs,
};

use super::{
    LayoutFacts, Placed, display_text, next, paragraph_offsets, paragraph_style_named, sequence_of,
};

/// The deepest heading level a table of contents can show, as Word's dialog
/// offers.
pub const MAX_TOC_LEVEL: u8 = 9;

/// The default when the caller names no level — Word's own default.
pub const DEFAULT_TOC_LEVEL: u8 = 3;

/// The indent one contents level adds, in twips (0.15 in), matching the
/// built-in `TOC n` styles Word ships.
const TOC_LEVEL_INDENT_TWIPS: i32 = 220;

/// The prefix Word gives the bookmarks a hyperlinked contents entry points at.
pub const TOC_BOOKMARK_PREFIX: &str = "_Toc";

/// Word's built-in style for the entries of a table of figures.
pub const TABLE_OF_FIGURES_STYLE_NAME: &str = "table of figures";

/// The levels a request for `requested` shows: `0` is Word's default of 3,
/// anything else is clamped to 1..=9. O(1).
#[must_use]
pub fn toc_level(requested: u32) -> u8 {
    if requested == 0 {
        DEFAULT_TOC_LEVEL
    } else {
        (requested.min(u32::from(MAX_TOC_LEVEL)) as u8).max(1)
    }
}

/// The name of the paragraph style for contents level `level` (1-based).
/// `TOC 1`..`TOC 9` is Word's marking, and the one the editor's contents
/// navigation recognises. O(1).
fn toc_style_name(level: u8) -> String {
    format!("TOC {level}")
}

// ---- Settings ---------------------------------------------------------------

/// Where a table's entries come from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TocSource {
    /// Every paragraph whose outline level (direct, or through its style) is
    /// at or above the table's level — Word's `\o` with `\u`.
    OutlineLevels,
    /// Paragraphs styled with one of these styles, each listed at the level
    /// it is paired with — Word's `\t "Style,level,…"`, ONLYOFFICE's
    /// "Selected styles".
    Styles(Vec<(String, u8)>),
    /// A table of FIGURES: every caption of `label` (`\c`), shown with its
    /// label and number, or (`include_label: false`, Word's `\a`) the caption
    /// text alone.
    Captions {
        /// The caption sequence: `Figure`, `Table`, `Equation` or the
        /// document's own label.
        label: String,
        /// Whether each entry keeps "Figure 3" in front of the caption text.
        include_label: bool,
    },
}

/// The format a table of contents is drawn in: ONLYOFFICE's style gallery
/// (`TableOfContentsSettings.js:936-943`: Current, Simple, Standard, Modern,
/// Classic, Online), which rewrites the `TOC n` style definitions as Word's
/// "Formats" list does. `Current` keeps whatever the document defines.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TocStyle {
    /// Keep the document's own `TOC n` styles (create plain ones if absent).
    #[default]
    Current,
    /// Indented levels, no emphasis.
    Simple,
    /// Indented levels; the top two bold and larger.
    Standard,
    /// No indents; a bold, larger top level with more space after it.
    Modern,
    /// First-line indents per level; the top two bold.
    Classic,
    /// Underlined in the theme's hyperlink colour, like a web page's index.
    Online,
}

impl TocStyle {
    /// Every format, in the gallery's order.
    pub const ALL: [Self; 6] = [
        Self::Current,
        Self::Simple,
        Self::Standard,
        Self::Modern,
        Self::Classic,
        Self::Online,
    ];

    /// The format's wire token. O(1).
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Simple => "simple",
            Self::Standard => "standard",
            Self::Modern => "modern",
            Self::Classic => "classic",
            Self::Online => "online",
        }
    }

    /// The format a token names. O(1).
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|style| style.token() == token)
    }
}

/// Everything the settings dialog decides about one table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TocOptions {
    /// The deepest outline level listed (1..=9). Ignored for a table built
    /// from styles or captions.
    pub max_level: u8,
    /// Whether each entry ends in its page number.
    pub page_numbers: bool,
    /// Whether the page number sits on a right-aligned tab at the margin.
    pub right_align: bool,
    /// The leader that tab draws, or none.
    pub leader: Option<TabLeader>,
    /// Whether each entry links to its heading (`\h`).
    pub hyperlinks: bool,
    /// Where the entries come from.
    pub source: TocSource,
    /// The format the entry styles are given.
    pub style: TocStyle,
}

impl Default for TocOptions {
    /// What the editor generated before settings existed, so a caller that
    /// names none gets the same table it always did.
    fn default() -> Self {
        Self {
            max_level: DEFAULT_TOC_LEVEL,
            page_numbers: true,
            right_align: true,
            leader: Some(TabLeader::Dot),
            hyperlinks: false,
            source: TocSource::OutlineLevels,
            style: TocStyle::Current,
        }
    }
}

impl TocOptions {
    /// The options a `TOC` instruction records — levels, styles, captions,
    /// `\h` and `\n` — with the entry format (right align, leader) at its
    /// defaults, since that lives on the entries ([`entry_format_of`]).
    ///
    /// `\t` wins over `\o` only when there is no `\o`: Word merges the two,
    /// and a table that names both is listed by outline level here, which
    /// lists every paragraph the styles would have named at the level its
    /// style's outline gives it. **O(instruction length).**
    #[must_use]
    pub fn from_instruction(instruction: &str) -> Self {
        let tokens = switch_tokens(instruction);
        let mut options = Self::default();
        let argument = |switch: &str| {
            tokens
                .iter()
                .position(|token| token.eq_ignore_ascii_case(switch))
                .and_then(|index| tokens.get(index + 1))
                .filter(|value| !value.starts_with('\\'))
                .cloned()
        };
        let has = |switch: &str| {
            tokens
                .iter()
                .any(|token| token.eq_ignore_ascii_case(switch))
        };
        options.hyperlinks = has("\\h");
        options.page_numbers = !has("\\n");
        if let Some(range) = argument("\\o") {
            options.max_level = range
                .split('-')
                .nth(1)
                .and_then(|value| value.trim().parse::<u8>().ok())
                .map_or(DEFAULT_TOC_LEVEL, |level| level.clamp(1, MAX_TOC_LEVEL));
        }
        if let Some(label) = argument("\\c") {
            options.source = TocSource::Captions {
                label,
                include_label: true,
            };
        } else if let Some(label) = argument("\\a") {
            options.source = TocSource::Captions {
                label,
                include_label: false,
            };
        } else if !has("\\o")
            && let Some(list) = argument("\\t")
        {
            options.source = TocSource::Styles(parse_style_list(&list));
        }
        options
    }

    /// The instruction a table with these options carries. The default is
    /// byte-for-byte what [`toc_instruction`] wrote before settings existed.
    /// O(styles named).
    #[must_use]
    pub fn instruction(&self) -> String {
        let mut out = String::from(" TOC");
        match &self.source {
            TocSource::OutlineLevels => {
                out.push_str(&format!(" \\o \"1-{}\"", self.max_level));
            }
            TocSource::Styles(styles) => {
                let list: Vec<String> = styles
                    .iter()
                    .map(|(name, level)| format!("{name},{level}"))
                    .collect();
                out.push_str(&format!(" \\t \"{}\"", list.join(",")));
            }
            TocSource::Captions {
                label,
                include_label,
            } => {
                let switch = if *include_label { "\\c" } else { "\\a" };
                out.push_str(&format!(" {switch} \"{label}\""));
            }
        }
        if self.hyperlinks {
            out.push_str(" \\h");
        }
        out.push_str(" \\z");
        if matches!(self.source, TocSource::OutlineLevels) {
            out.push_str(" \\u");
        }
        if !self.page_numbers {
            out.push_str(" \\n");
        }
        out.push(' ');
        out
    }

    /// Whether this is a table of figures. O(1).
    #[must_use]
    pub const fn is_table_of_figures(&self) -> bool {
        matches!(self.source, TocSource::Captions { .. })
    }
}

/// An instruction's words, a quoted argument kept whole without its quotes.
/// O(instruction length).
fn switch_tokens(instruction: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for ch in instruction.chars() {
        match ch {
            '"' => {
                if quoted {
                    tokens.push(std::mem::take(&mut current));
                }
                quoted = !quoted;
            }
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// `"Heading 1,1,Quote,2"` as `[("Heading 1", 1), ("Quote", 2)]`. A pair
/// whose level does not read as 1..=9 is dropped. O(list length).
fn parse_style_list(list: &str) -> Vec<(String, u8)> {
    let parts: Vec<&str> = list.split(',').collect();
    parts
        .chunks(2)
        .filter_map(|pair| {
            let name = pair.first()?.trim();
            let level = pair.get(1)?.trim().parse::<u8>().ok()?;
            (!name.is_empty() && (1..=MAX_TOC_LEVEL).contains(&level))
                .then(|| (name.to_owned(), level))
        })
        .collect()
}

/// The deepest level a `TOC` instruction asks for: the upper bound of its
/// `\o "1-3"` switch, clamped to 1..=9; Word's 3 when the switch is absent or
/// unreadable. **O(instruction length).**
#[must_use]
pub fn toc_levels_of(instruction: &str) -> u8 {
    TocOptions::from_instruction(instruction).max_level
}

/// The instruction a generated table of contents carries with the default
/// settings. `\z` (hide in web layout) and `\u` (use outline levels) are
/// Word's own defaults; `\h` is absent unless asked for (see the module note).
/// O(1).
#[must_use]
pub fn toc_instruction(max_level: u8) -> String {
    TocOptions {
        max_level,
        ..TocOptions::default()
    }
    .instruction()
}

/// The right-align and leader choice the existing entries in `blocks` carry:
/// the first entry's end-aligned tab stop, or "not right aligned" when its
/// tab has no stop. `None` when no block is an entry with a tab (a table with
/// no page numbers), so the caller keeps its defaults. O(blocks + tabs).
#[must_use]
pub fn entry_format_of(blocks: &[BlockNode]) -> Option<(bool, Option<TabLeader>)> {
    blocks.iter().find_map(|block| match block {
        BlockNode::Paragraph(paragraph) if has_tab(&paragraph.inlines) => {
            let properties = paragraph.properties.get();
            Some(
                match properties
                    .tabs
                    .iter()
                    .find(|stop| stop.alignment == TabAlignment::End)
                {
                    Some(stop) => (true, stop.leader),
                    None => (false, None),
                },
            )
        }
        _ => None,
    })
}

/// The settings of the table of contents `field`: its instruction's switches
/// plus its entries' tab format — what the settings dialog opens on, and what
/// an update keeps. **O(body blocks)** for the field's extent.
///
/// # Errors
/// As [`toc_field`]; the markers are not at body top level.
pub fn toc_options(document: &Document, field: Option<FieldRangeId>) -> Result<TocOptions, String> {
    let (field, instruction) = toc_field(document, field)?;
    let (first, last) = body_field_range_blocks(document.body(), field)
        .map_err(|_| "the contents field's markers are not at body top level".to_owned())?;
    let mut options = TocOptions::from_instruction(&instruction);
    if let Some((right_align, leader)) = entry_format_of(&document.body()[first..=last]) {
        options.right_align = right_align;
        options.leader = leader;
    }
    Ok(options)
}

// ---- Entries ----------------------------------------------------------------

/// One entry the table names.
struct Entry {
    /// 1-based contents level (1 = top); always 1 in a table of figures.
    level: u8,
    /// The entry's text, tabs flattened.
    text: String,
    /// The page label printed on the page the source paragraph is laid out
    /// on. Empty when the layout placed no fragment for it (a heading in a
    /// text box) or page numbers are off; it is never invented.
    page: String,
    /// The source paragraph.
    node: NodeId,
    /// Its plain-text length — the end of a bookmark covering it.
    length: u32,
    /// A `_Toc` bookmark already starting in it, if any.
    bookmark: Option<String>,
}

/// The name of a `_Toc` bookmark whose start marker sits at the top level of
/// `inlines`. O(inlines).
fn toc_bookmark_in(document: &Document, inlines: &[InlineNode]) -> Option<String> {
    inlines.iter().find_map(|inline| match inline {
        InlineNode::BookmarkStart(marker) => document
            .definitions()
            .bookmarks
            .get(&marker.bookmark)
            .filter(|bookmark| bookmark.name.starts_with(TOC_BOOKMARK_PREFIX))
            .map(|bookmark| bookmark.name.clone()),
        _ => None,
    })
}

/// A caption's text after its label and number — Word's "Include label and
/// number" off (`\a`): the text after the top-level `SEQ` field, with the
/// separator (`:`, `.`, a dash or spaces) between the two trimmed. The whole
/// caption when no top-level `SEQ` is found. O(inlines).
fn caption_text_alone(paragraph: &Paragraph) -> String {
    let Some(index) = paragraph.inlines.iter().position(
        |inline| matches!(inline, InlineNode::Field(field) if matches!(field.kind, FieldKind::Seq { .. })),
    ) else {
        return display_text(paragraph);
    };
    node_plain_text(&paragraph.inlines[index + 1..])
        .trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, ':' | '.' | '-' | '\u{2013}' | '\u{2014}')
        })
        .trim_end()
        .replace('\t', " ")
}

/// Every entry `options` lists, in document order.
///
/// **O(document)**: one walk of the body carrying each paragraph (never a by-id
/// lookup per entry, `docs/116`), O(1) page lookups and O(1) style-name
/// lookups through a map built once.
///
/// # Errors
/// The layout cannot number the whole document and page numbers are on.
fn toc_entries(
    document: &Document,
    facts: &impl LayoutFacts,
    options: &TocOptions,
) -> Result<Vec<Entry>, String> {
    if options.page_numbers {
        facts.whole_pagination()?;
    }
    // Style id -> level, for `\t`: built once over the style table, so the walk
    // looks each paragraph's style up in O(1) rather than by name.
    let by_style: HashMap<StyleId, u8> = match &options.source {
        TocSource::Styles(styles) => document
            .definitions()
            .styles
            .iter()
            .filter_map(|(id, style)| {
                let name = style.name.as_deref()?;
                styles
                    .iter()
                    .find(|(wanted, _)| wanted.eq_ignore_ascii_case(name))
                    .map(|(_, level)| (*id, *level))
            })
            .collect(),
        _ => HashMap::new(),
    };
    let bookmarks = &document.definitions().bookmarks;
    let mut entries = Vec::new();
    visit_paragraphs(document.body(), &mut |paragraph| {
        let listed = match &options.source {
            TocSource::OutlineLevels => facts
                .heading_level(paragraph)
                .filter(|level| *level <= options.max_level)
                .map(|level| (level, display_text(paragraph))),
            TocSource::Styles(_) => paragraph
                .properties
                .get()
                .style_ref
                .and_then(|id| by_style.get(&id).copied())
                .map(|level| (level, display_text(paragraph))),
            TocSource::Captions {
                label,
                include_label,
            } => sequence_of(&paragraph.inlines)
                .filter(|(name, _)| name.eq_ignore_ascii_case(label))
                .map(|_| {
                    let text = if *include_label {
                        display_text(paragraph)
                    } else {
                        caption_text_alone(paragraph)
                    };
                    (1, text)
                }),
        };
        let Some((level, text)) = listed else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let page = if options.page_numbers {
            facts.page_label(paragraph.id).unwrap_or_default()
        } else {
            String::new()
        };
        let (length, bookmark) = if options.hyperlinks {
            (
                paragraph_offsets(paragraph, bookmarks).0,
                toc_bookmark_in(document, &paragraph.inlines),
            )
        } else {
            (0, None)
        };
        entries.push(Entry {
            level,
            text,
            page,
            node: paragraph.id,
            length,
            bookmark,
        });
    });
    Ok(entries)
}

/// The `CreateBookmark` operations that give every entry lacking one a fresh
/// `_Toc` bookmark over its whole source paragraph, writing the name into the
/// entry. Names never collide with the document's. **O(entries + bookmarks)**
/// to plan; see the module note for what applying them costs.
fn plant_bookmarks(
    document: &Document,
    entries: &mut [Entry],
    ids: MintId<'_>,
) -> Result<Vec<Operation>, String> {
    let taken: BTreeSet<&str> = document
        .definitions()
        .bookmarks
        .iter()
        .map(|(_, bookmark)| bookmark.name.as_str())
        .collect();
    let mut serial: u64 = 1;
    let mut operations = Vec::new();
    for entry in entries.iter_mut().filter(|entry| entry.bookmark.is_none()) {
        let name = loop {
            let candidate = format!("{TOC_BOOKMARK_PREFIX}{serial:09}");
            serial += 1;
            if !taken.contains(candidate.as_str()) {
                break candidate;
            }
        };
        operations.push(Operation::CreateBookmark {
            bookmark: BookmarkId::new(next(ids)?),
            name: name.clone(),
            start: Pos::new(entry.node, 0),
            start_id: next(ids)?,
            end: Pos::new(entry.node, entry.length),
            end_id: next(ids)?,
        });
        entry.bookmark = Some(name);
    }
    Ok(operations)
}

// ---- Styles -----------------------------------------------------------------

/// The paragraph and run formatting format `style` gives contents level
/// `level`. The shapes follow ONLYOFFICE's gallery (`sdkjs/word/Editor/
/// Styles.js:6750-7010`, `CStyle.prototype.CreateTOC`): which levels are bold
/// and larger, and whether levels step by a left or a first-line indent. The
/// measurements are this editor's (0.5 cm steps, 3 pt and 8.5 pt after).
/// `Current` is the plain shape the editor always created. O(1).
fn style_format(style: TocStyle, level: u8) -> (ParagraphProperties, Option<RunProperties>) {
    const STEP: i32 = 283; // 0.5 cm in twips.
    const AFTER: i32 = 57; // 2.85 pt.
    const AFTER_TOP: i32 = 170; // 8.5 pt.
    let depth = i32::from(level.saturating_sub(1));
    let spaced = |after: i32| {
        Some(Spacing {
            after_twips: Some(after),
            ..Spacing::default()
        })
    };
    let indent = |start: i32, first_line: i32| {
        Some(Indentation {
            start_twips: Some(start),
            first_line_twips: (first_line != 0).then_some(first_line),
            ..Indentation::default()
        })
    };
    let run = |bold: bool, points: u32| {
        Some(RunProperties {
            bold: bold.then_some(true),
            size_half_points: Some(points * 2),
            ..RunProperties::default()
        })
    };
    // The emphasis Standard, Modern and Classic share: bold 14 pt, bold 13 pt,
    // 13 pt, then 11 pt.
    let ranked = |level: u8| match level {
        1 => run(true, 14),
        2 => run(true, 13),
        3 => run(false, 13),
        _ => run(false, 11),
    };
    match style {
        TocStyle::Current => (
            ParagraphProperties {
                indentation: indent(depth * TOC_LEVEL_INDENT_TWIPS, 0),
                ..ParagraphProperties::default()
            },
            None,
        ),
        TocStyle::Simple => (
            ParagraphProperties {
                indentation: indent(depth * STEP, 0),
                spacing: spaced(AFTER),
                ..ParagraphProperties::default()
            },
            None,
        ),
        TocStyle::Standard => (
            ParagraphProperties {
                indentation: indent(depth * STEP, 0),
                spacing: spaced(AFTER),
                ..ParagraphProperties::default()
            },
            ranked(level),
        ),
        TocStyle::Modern => (
            ParagraphProperties {
                indentation: indent(0, 0),
                spacing: spaced(if level == 1 { AFTER_TOP } else { AFTER }),
                ..ParagraphProperties::default()
            },
            ranked(level),
        ),
        TocStyle::Classic => (
            ParagraphProperties {
                indentation: indent(0, (depth - 1).max(0) * STEP),
                spacing: spaced(if level == 1 { AFTER_TOP } else { AFTER }),
                ..ParagraphProperties::default()
            },
            ranked(level),
        ),
        TocStyle::Online => (
            ParagraphProperties {
                indentation: indent(depth * TOC_LEVEL_INDENT_TWIPS, 0),
                spacing: spaced(100),
                ..ParagraphProperties::default()
            },
            Some(RunProperties {
                underline: Some(true),
                color: Some(Color::Theme(ThemeColor {
                    slot: ThemeColorRef::Hyperlink,
                    theme_tint: None,
                    theme_shade: None,
                })),
                ..RunProperties::default()
            }),
        ),
    }
}

/// A fresh paragraph style named `name` with `format`. O(1).
fn new_entry_style(name: String, format: (ParagraphProperties, Option<RunProperties>)) -> Style {
    Style {
        kind: StyleKind::Paragraph,
        is_default: false,
        name: Some(name),
        aliases: None,
        based_on: None,
        next: None,
        link: None,
        hidden: false,
        ui_priority: Some(39),
        semi_hidden: false,
        unhide_when_used: true,
        q_format: false,
        locked: false,
        paragraph: Some(format.0),
        run: format.1,
        table: None,
        table_row: None,
        table_cell: None,
        conditional: Vec::new(),
        custom_style: false,
    }
}

/// The style id for an entry style named `name`, plus the operation that
/// creates it (when the document does not define it) or reformats it (when
/// `style` is a gallery format rather than `Current`). A reformat keeps the
/// style's identity and everything but its paragraph and run formatting.
/// **O(styles).**
fn entry_style(
    document: &Document,
    name: String,
    level: u8,
    style: TocStyle,
    ids: MintId<'_>,
) -> Result<(StyleId, Option<Operation>), String> {
    let format = style_format(style, level);
    if let Some(id) = paragraph_style_named(document, &name) {
        if style == TocStyle::Current {
            return Ok((id, None));
        }
        let mut restyled = document
            .definitions()
            .styles
            .get(&id)
            .cloned()
            .unwrap_or_else(|| new_entry_style(name, format.clone()));
        restyled.paragraph = Some(format.0);
        restyled.run = format.1;
        return Ok((
            id,
            Some(Operation::SetStyleDefinition {
                id,
                style: Some(Box::new(restyled)),
            }),
        ));
    }
    let id = StyleId::new(next(ids)?);
    Ok((
        id,
        Some(Operation::SetStyleDefinition {
            id,
            style: Some(Box::new(new_entry_style(name, format))),
        }),
    ))
}

/// The operations an entry-style pass needs, and the style each level uses.
type EntryStyles = (Vec<Operation>, BTreeMap<u8, StyleId>);

/// The entry style for every level `entries` uses — and, for a gallery
/// format, every level down to `max_level`, so a level the next update adds
/// arrives in the same format. **O(levels × styles)**, at most nine levels.
fn entry_styles(
    document: &Document,
    entries: &[Entry],
    options: &TocOptions,
    ids: MintId<'_>,
) -> Result<EntryStyles, String> {
    let mut levels: BTreeSet<u8> = entries.iter().map(|entry| entry.level).collect();
    if options.style != TocStyle::Current && !options.is_table_of_figures() {
        levels.extend(1..=options.max_level);
    }
    let mut operations = Vec::new();
    let mut styles = BTreeMap::new();
    for level in levels {
        let name = if options.is_table_of_figures() {
            TABLE_OF_FIGURES_STYLE_NAME.to_owned()
        } else {
            toc_style_name(level)
        };
        let (id, op) = entry_style(document, name, level, options.style, ids)?;
        styles.insert(level, id);
        operations.extend(op);
    }
    Ok((operations, styles))
}

// ---- Blocks -----------------------------------------------------------------

/// The entry paragraphs for `entries`, carrying the field range's two markers
/// on the first and last block, plus the style and bookmark operations they
/// need first. **O(entries)** plus the bounded style pass.
fn toc_blocks(
    document: &Document,
    entries: &mut [Entry],
    field: FieldRangeId,
    options: &TocOptions,
    tab_position_twips: i32,
    ids: MintId<'_>,
) -> Result<(Vec<Operation>, Vec<BlockNode>), String> {
    let (mut operations, styles) = entry_styles(document, entries, options, ids)?;
    if options.hyperlinks {
        operations.extend(plant_bookmarks(document, entries, ids)?);
    }
    let tabs = if options.page_numbers && options.right_align {
        vec![TabStop {
            position_twips: tab_position_twips.max(0),
            alignment: TabAlignment::End,
            leader: options.leader,
        }]
    } else {
        Vec::new()
    };

    let count = entries.len();
    let mut blocks = Vec::with_capacity(count);
    for (index, entry) in entries.iter().enumerate() {
        let mut content = Vec::with_capacity(3);
        content.push(InlineNode::Run(Run {
            id: next(ids)?,
            properties: RunProperties::default().into(),
            text: entry.text.clone(),
        }));
        if options.page_numbers {
            content.push(InlineNode::Tab(Tab { id: next(ids)? }));
            // A source the layout placed nowhere it can number (one in a text
            // box) has no page: the entry keeps its tab and shows none, because
            // an empty run is not a valid model node and would refuse the table.
            if !entry.page.is_empty() {
                content.push(InlineNode::Run(Run {
                    id: next(ids)?,
                    properties: RunProperties::default().into(),
                    text: entry.page.clone(),
                }));
            }
        }
        let content = match (&entry.bookmark, options.hyperlinks) {
            (Some(anchor), true) => vec![InlineNode::Hyperlink(Box::new(Hyperlink {
                id: next(ids)?,
                target: HyperlinkTarget::Internal(InternalTarget {
                    anchor: anchor.clone(),
                }),
                tooltip: None,
                inlines: content,
                history: true,
            }))],
            _ => content,
        };
        let mut inlines = Vec::with_capacity(content.len() + 2);
        if index == 0 {
            inlines.push(InlineNode::FieldRangeStart(FieldRangeStart {
                id: next(ids)?,
                field,
            }));
        }
        inlines.extend(content);
        if index + 1 == count {
            inlines.push(InlineNode::FieldRangeEnd(FieldRangeEnd {
                id: next(ids)?,
                field,
            }));
        }
        blocks.push(BlockNode::Paragraph(Paragraph {
            id: next(ids)?,
            properties: ParagraphProperties {
                style_ref: styles.get(&entry.level).copied(),
                tabs: tabs.clone(),
                ..ParagraphProperties::default()
            }
            .into(),
            inlines,
        }));
    }
    Ok((operations, blocks))
}

/// The refusal for a table with nothing to list, naming what it looked for.
/// O(1).
fn nothing_to_list(options: &TocOptions) -> String {
    match &options.source {
        TocSource::Captions { .. } => refused!(
            "toc.no-captions",
            "This document has no captions with that label to list. Insert a caption first."
        )
        .to_owned(),
        TocSource::Styles(_) => refused!(
            "toc.no-styled",
            "No paragraph uses any of the chosen styles, so there is nothing to list."
        )
        .to_owned(),
        TocSource::OutlineLevels => format!(
            "the document has no headings at level {} or above to list",
            options.max_level
        ),
    }
}

// ---- Insert -----------------------------------------------------------------

/// Inserts a table of contents before the top-level body block holding
/// `before`, listing every heading down to `max_level` (see [`toc_level`]),
/// with the default settings. See [`insert_table_of_contents_with`].
///
/// # Errors
/// As [`insert_table_of_contents_with`].
pub fn insert_table_of_contents(
    document: &Document,
    facts: &impl LayoutFacts,
    before: NodeId,
    max_level: u8,
    tab_position_twips: i32,
    ids: MintId<'_>,
) -> Result<Placed, String> {
    let options = TocOptions {
        max_level,
        ..TocOptions::default()
    };
    insert_contents(
        document,
        facts,
        before,
        &options,
        tab_position_twips,
        ids,
        Replace::Refuse,
    )
}

/// What an insert does when the caret is inside an existing table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Replace {
    /// Replace that table — the settings dialog on a generated table.
    Existing,
    /// Refuse: the plain insert (and the agent command) never deletes a table
    /// it was not asked to.
    Refuse,
}

/// Inserts a table of contents — or, for [`TocSource::Captions`], a table of
/// figures — before the top-level body block holding `before`, drawn with
/// `options`. When `before` is inside an existing table, that table is
/// REPLACED (one `RemoveFieldRange` first), which is how Word's dialog changes
/// a generated table's settings; [`insert_table_of_contents`] refuses there
/// instead.
///
/// Each entry is a paragraph in the level's `TOC n` style (or `table of
/// figures`) holding the source's text and, when page numbers are on, a tab and
/// the page label that page prints — on a right-aligned leader tab at
/// `tab_position_twips` (the content width) when right alignment is on. The
/// whole run of entries is one paragraph-spanning complex field (`docs/128`),
/// so an update finds it again. The caret goes to the start of the first entry.
///
/// **O(document)** for the walk, plus O(new bookmarks × document) the first
/// time a hyperlinked table is generated (see the module note). Never on a
/// keystroke path.
///
/// # Errors
/// `before` is not in the body; page numbers are on and the whole document is
/// not paginated ([`LayoutFacts::whole_pagination`]); nothing to list; ids ran
/// out.
pub fn insert_table_of_contents_with(
    document: &Document,
    facts: &impl LayoutFacts,
    before: NodeId,
    options: &TocOptions,
    tab_position_twips: i32,
    ids: MintId<'_>,
) -> Result<Placed, String> {
    insert_contents(
        document,
        facts,
        before,
        options,
        tab_position_twips,
        ids,
        Replace::Existing,
    )
}

/// The insert both entry points share. **O(document).**
fn insert_contents(
    document: &Document,
    facts: &impl LayoutFacts,
    before: NodeId,
    options: &TocOptions,
    tab_position_twips: i32,
    ids: MintId<'_>,
    replace: Replace,
) -> Result<Placed, String> {
    let index = document
        .body()
        .iter()
        .position(|block| crate::table::holds_paragraph(block, before))
        .ok_or_else(|| {
            refused!(
                "toc.body-only",
                "A table of contents goes in the document body, not in a header, footer or note."
            )
            .to_owned()
        })?;
    // Inside an existing contents field, Word asks to REPLACE it rather than
    // nest a second one in it: the settings dialog on a generated table is how
    // its settings are changed.
    let replacing = contents_field_covering(document, index);
    let (index, mut operations) = match (replacing, replace) {
        (Some((field, first)), Replace::Existing) => {
            (first, vec![Operation::RemoveFieldRange { field }])
        }
        (Some(_), Replace::Refuse) => {
            return Err(refused!(
                "toc.inside-contents",
                "The caret is inside a table of contents. Open its settings to change it, or \
                 move the caret out of it to insert another."
            )
            .to_owned());
        }
        (None, _) => (index, Vec::new()),
    };
    let mut entries = toc_entries(document, facts, options)?;
    if entries.is_empty() {
        return Err(nothing_to_list(options));
    }
    let field = FieldRangeId::new(next(ids)?);
    let (prelude, blocks) = toc_blocks(
        document,
        &mut entries,
        field,
        options,
        tab_position_twips,
        ids,
    )?;
    operations.extend(prelude);
    let caret = blocks
        .first()
        .and_then(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(Pos::new(paragraph.id, 0)),
            _ => None,
        })
        .ok_or_else(|| {
            refused!(
                "toc.empty",
                "This table of contents has no entries to go to."
            )
            .to_owned()
        })?;
    let instruction = options.instruction();
    operations.push(Operation::InsertFieldRange {
        field,
        definition: Box::new(FieldRange {
            kind: FieldKind::parse(&instruction),
            instruction,
            update: FieldUpdateState::default(),
        }),
        index: index as u32,
        blocks,
    });
    Ok(Placed { operations, caret })
}

/// The `TOC` field range whose blocks cover body block `index`, with the
/// index of its first block. **O(field ranges × body blocks)** in the worst
/// case; a document holds a handful of field ranges.
fn contents_field_covering(document: &Document, index: usize) -> Option<(FieldRangeId, usize)> {
    document
        .definitions()
        .field_ranges
        .iter()
        .filter(|(_, range)| range.kind == FieldKind::Toc)
        .find_map(|(id, _)| {
            let (first, last) = body_field_range_blocks(document.body(), *id).ok()?;
            (first..=last).contains(&index).then_some((*id, first))
        })
}

// ---- Update -----------------------------------------------------------------

/// Word's two ways to update a table of contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TocUpdate {
    /// Re-read the pagination and keep the entries.
    PageNumbers,
    /// Rebuild the entries from the heading outline (or the captions).
    Entire,
}

/// The field range a contents command acts on: `field`, or — when `None` — the
/// document's single `TOC` field range. Returns its id and instruction.
///
/// **O(field ranges).**
///
/// # Errors
/// `field` names no field range; with `None`, the document has no table of
/// contents or more than one.
pub fn toc_field(
    document: &Document,
    field: Option<FieldRangeId>,
) -> Result<(FieldRangeId, String), String> {
    let ranges = &document.definitions().field_ranges;
    if let Some(field) = field {
        let range = ranges
            .get(&field)
            .ok_or_else(|| "no such field range".to_owned())?;
        return Ok((field, range.instruction.clone()));
    }
    let mut found = ranges
        .iter()
        .filter(|(_, range)| range.kind == FieldKind::Toc)
        .map(|(id, range)| (*id, range.instruction.clone()));
    let first = found.next().ok_or_else(|| {
        refused!(
            "toc.none",
            "This document has no table of contents to update."
        )
        .to_owned()
    })?;
    if found.next().is_some() {
        return Err(
            "this document holds more than one table of contents; name the one to update"
                .to_owned(),
        );
    }
    Ok(first)
}

/// Updates a table of contents or of figures: [`TocUpdate::PageNumbers`]
/// rewrites only the page column from the current pagination;
/// [`TocUpdate::Entire`] rebuilds the entries with the table's own settings
/// ([`toc_options`]) — its levels or styles or caption label, its links, its
/// page numbers and its tab format. One `DeleteBlocks` + `InsertBlocks` pair,
/// plus any entry style or bookmark the rebuild needs.
///
/// "Page numbers only" on a table that shows none has nothing to renumber and
/// rebuilds the table instead, which is what it would have produced.
///
/// **O(document)** — one walk for the entries.
///
/// # Errors
/// As [`toc_field`]; the field's markers are not at body top level; page
/// numbers are on and the whole document is not paginated; for `Entire`,
/// nothing to list; for `PageNumbers`, the entry count no longer matches (the
/// outline changed, which needs a whole rebuild rather than numbers on the
/// wrong rows).
pub fn update_table_of_contents(
    document: &Document,
    facts: &impl LayoutFacts,
    field: Option<FieldRangeId>,
    mode: TocUpdate,
    tab_position_twips: i32,
    ids: MintId<'_>,
) -> Result<Vec<Operation>, String> {
    if field.is_none()
        && !document
            .definitions()
            .field_ranges
            .iter()
            .any(|(_, range)| range.kind == FieldKind::Toc)
        && let Some((index, instruction)) = inline_toc(document)
    {
        return update_inline_toc(
            document,
            facts,
            index,
            &instruction,
            mode,
            tab_position_twips,
            ids,
        );
    }
    let (field, _) = toc_field(document, field)?;
    let (first, last) = body_field_range_blocks(document.body(), field)
        .map_err(|_| "the contents field's markers are not at body top level".to_owned())?;
    let count = (last - first + 1) as u32;
    let options = toc_options(document, Some(field))?;
    let mut entries = toc_entries(document, facts, &options)?;

    let mode = if options.page_numbers {
        mode
    } else {
        TocUpdate::Entire
    };
    let (mut operations, blocks) = match mode {
        TocUpdate::Entire => {
            if entries.is_empty() {
                return Err(match options.source {
                    TocSource::OutlineLevels => refused!(
                        "toc.no-headings",
                        "This document has no headings to list. Apply a heading style to the \
                         text you want listed."
                    )
                    .to_owned(),
                    _ => nothing_to_list(&options),
                });
            }
            toc_blocks(
                document,
                &mut entries,
                field,
                &options,
                tab_position_twips,
                ids,
            )?
        }
        TocUpdate::PageNumbers => (
            Vec::new(),
            toc_renumbered_blocks(document, first, last, &entries, ids)?,
        ),
    };
    operations.push(Operation::DeleteBlocks {
        container: None,
        index: first as u32,
        count,
    });
    operations.push(Operation::InsertBlocks {
        container: None,
        index: first as u32,
        blocks,
    });
    Ok(operations)
}

/// The top-level body paragraph holding a contents as an INLINE `TOC` field, and
/// its instruction — `docs/128` §2c's encoding when the field's markers fall in one
/// paragraph, which is how a one-entry contents (Word's or ours) reopens from DOCX.
/// `None` when there is none, or more than one (the caller then reports the
/// ordinary "no table of contents" refusal). **O(top-level inlines).**
fn inline_toc(document: &Document) -> Option<(usize, String)> {
    fn find(inlines: &[InlineNode]) -> Option<String> {
        inlines.iter().find_map(|inline| match inline {
            InlineNode::Field(field) if FieldKind::parse(&field.instruction) == FieldKind::Toc => {
                Some(field.instruction.clone())
            }
            InlineNode::Hyperlink(link) => find(&link.inlines),
            InlineNode::Sdt(sdt) => find(&sdt.inlines),
            _ => None,
        })
    }
    let mut found = document
        .body()
        .iter()
        .enumerate()
        .filter_map(|(index, block)| match block {
            BlockNode::Paragraph(paragraph) => find(&paragraph.inlines).map(|i| (index, i)),
            _ => None,
        });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

/// Updates a contents held as an inline field: its one paragraph is replaced by
/// freshly built entries, inserted as a field RANGE with the same instruction,
/// because the updated result no longer fits in a paragraph (`docs/128` §2c: a
/// field that does not fit in a paragraph is a range). Word does the same when it
/// updates a `TOC` field. "Page numbers only" checks the entry count as the range
/// path does, then rebuilds the one paragraph, which is the whole of it.
///
/// **O(document)** for the entries, like the range path.
fn update_inline_toc(
    document: &Document,
    facts: &impl LayoutFacts,
    index: usize,
    instruction: &str,
    mode: TocUpdate,
    tab_position_twips: i32,
    ids: MintId<'_>,
) -> Result<Vec<Operation>, String> {
    let mut options = TocOptions::from_instruction(instruction);
    if let Some((right_align, leader)) = entry_format_of(&document.body()[index..=index]) {
        options.right_align = right_align;
        options.leader = leader;
    }
    let mut entries = toc_entries(document, facts, &options)?;
    if entries.is_empty() {
        return Err(refused!(
            "toc.no-headings",
            "This document has no headings to list. Apply a heading style to the text \
             you want listed."
        )
        .to_owned());
    }
    if mode == TocUpdate::PageNumbers && options.page_numbers {
        let listed = usize::from(
            matches!(&document.body()[index], BlockNode::Paragraph(p) if has_tab(&p.inlines)),
        );
        if listed != entries.len() {
            return Err(format!(
                "this table lists {listed} entries but the document now has {} headings; \
                 update the entire table instead of only its page numbers",
                entries.len()
            ));
        }
    }
    let field = FieldRangeId::new(next(ids)?);
    let (mut operations, blocks) = toc_blocks(
        document,
        &mut entries,
        field,
        &options,
        tab_position_twips,
        ids,
    )?;
    operations.push(Operation::DeleteBlocks {
        container: None,
        index: index as u32,
        count: 1,
    });
    operations.push(Operation::InsertFieldRange {
        field,
        definition: Box::new(FieldRange {
            kind: FieldKind::Toc,
            instruction: instruction.to_owned(),
            update: FieldUpdateState::default(),
        }),
        index: index as u32,
        blocks,
    });
    Ok(operations)
}

/// The contents blocks `first..=last` with only their page numbers rewritten
/// from `entries`: each entry keeps its own runs, formatting, link and field
/// markers, and only the text after its tab is replaced. Entries pair with
/// sources POSITIONALLY, which is what "update page numbers only" means.
///
/// **O(entries + inlines in them).**
fn toc_renumbered_blocks(
    document: &Document,
    first: usize,
    last: usize,
    entries: &[Entry],
    ids: MintId<'_>,
) -> Result<Vec<BlockNode>, String> {
    let blocks: Vec<BlockNode> = document.body()[first..=last].to_vec();
    let listed = blocks
        .iter()
        .filter(|block| matches!(block, BlockNode::Paragraph(p) if has_tab(&p.inlines)))
        .count();
    if listed != entries.len() {
        return Err(format!(
            "this table lists {listed} entries but the document now has {} headings; \
             update the entire table instead of only its page numbers",
            entries.len()
        ));
    }

    let mut out = Vec::with_capacity(blocks.len());
    let mut next_entry = 0_usize;
    for block in blocks {
        let BlockNode::Paragraph(mut paragraph) = block else {
            out.push(block);
            continue;
        };
        if !has_tab(&paragraph.inlines) {
            out.push(BlockNode::Paragraph(paragraph));
            continue;
        }
        let page = &entries[next_entry].page;
        let mut fresh = || {
            ids().map(|id| {
                InlineNode::Run(Run {
                    id,
                    properties: RunProperties::default().into(),
                    text: page.clone(),
                })
            })
        };
        // The tab sits at the top level, or inside the entry's hyperlink.
        let renumber = |inlines: &mut Vec<InlineNode>, run: Option<InlineNode>| {
            let Some(tab) = inlines
                .iter()
                .position(|inline| matches!(inline, InlineNode::Tab(_)))
            else {
                return false;
            };
            let end_marker = match inlines.last() {
                Some(InlineNode::FieldRangeEnd(marker)) => Some(InlineNode::FieldRangeEnd(*marker)),
                _ => None,
            };
            inlines.truncate(tab + 1);
            inlines.extend(run);
            inlines.extend(end_marker);
            true
        };
        let run = if page.is_empty() {
            None
        } else {
            Some(fresh().ok_or_else(|| "id space exhausted".to_owned())?)
        };
        if !renumber(&mut paragraph.inlines, run.clone()) {
            for inline in &mut paragraph.inlines {
                if let InlineNode::Hyperlink(link) = inline
                    && renumber(&mut link.inlines, run.clone())
                {
                    break;
                }
            }
        }
        next_entry += 1;
        out.push(BlockNode::Paragraph(paragraph));
    }
    Ok(out)
}

/// Whether an inline list holds a tab, at its top level or inside a link —
/// what makes a contents paragraph an ENTRY rather than a "Contents" title the
/// field happens to enclose. **O(inlines).**
fn has_tab(inlines: &[InlineNode]) -> bool {
    inlines.iter().any(|inline| match inline {
        InlineNode::Tab(_) => true,
        InlineNode::Hyperlink(link) => has_tab(&link.inlines),
        _ => false,
    })
}

// ---- Remove -----------------------------------------------------------------

/// Removes a table of contents (or of figures) as a unit — ONLYOFFICE's
/// Remove table of contents (`DE.Views.Links.textContentsRemove`), Word's
/// References ▸ Table of Contents ▸ Remove. `field` names it; `None` is the
/// document's only one. One `RemoveFieldRange`, which drops the definition and
/// every entry block and is exactly undoable. A contents held as an inline
/// field (a one-entry table reopened from DOCX) is its paragraph, deleted when
/// the body has another block to keep.
///
/// The bookmarks a hyperlinked table planted on its headings stay: they are
/// harmless, Word leaves them too, and a later table reuses them.
///
/// **O(field ranges + body blocks).**
///
/// # Errors
/// As [`toc_field`]; an inline contents that is the body's only block.
pub fn remove_table_of_contents(
    document: &Document,
    field: Option<FieldRangeId>,
) -> Result<Vec<Operation>, String> {
    if field.is_none()
        && !document
            .definitions()
            .field_ranges
            .iter()
            .any(|(_, range)| range.kind == FieldKind::Toc)
        && let Some((index, _)) = inline_toc(document)
    {
        if document.body().len() < 2 {
            return Err(refused!(
                "toc.only-block",
                "The table of contents is the whole document; delete its text instead."
            )
            .to_owned());
        }
        return Ok(vec![Operation::DeleteBlocks {
            container: None,
            index: index as u32,
            count: 1,
        }]);
    }
    let (field, _) = toc_field(document, field).map_err(|error| {
        if casual_doc_edit::refusal::split(&error).1 == Some("toc.none") {
            refused!(
                "toc.none-to-remove",
                "This document has no table of contents to remove."
            )
            .to_owned()
        } else {
            error
        }
    })?;
    Ok(vec![Operation::RemoveFieldRange { field }])
}

// ---- Add text ---------------------------------------------------------------

/// Marks every paragraph in `paragraphs` at contents level `level` (1..=9), or
/// — with `None` — as "Do not show in table of contents", by setting its
/// outline level (`w:outlineLvl`) and leaving its style and formatting alone.
///
/// Deliberately not ONLYOFFICE's behaviour: their Add text applies the default
/// `Heading n` style (`sdkjs/word/api.js:12183-12187`, `asc_AddParagraphToTOC`),
/// so adding a body paragraph to the contents also turns it into a heading on
/// the page. Word's Paragraph ▸ Outline level is the gesture that lists a
/// paragraph without restyling it, and it is what `TOC \u` reads; this follows
/// that. "Do not show" clears the direct level, and writes Word's explicit
/// body-text level (9) only when the paragraph's style would list it anyway.
///
/// **O(document)** for the one walk that finds the paragraphs; the style check
/// is O(style chain) per paragraph.
///
/// # Errors
/// `level` is outside 1..=9; a paragraph is not in the body.
pub fn add_paragraphs_to_contents(
    document: &Document,
    facts: &impl LayoutFacts,
    paragraphs: &[NodeId],
    level: Option<u8>,
) -> Result<Vec<Operation>, String> {
    if level.is_some_and(|level| !(1..=MAX_TOC_LEVEL).contains(&level)) {
        return Err("a contents level is 1 to 9".to_owned());
    }
    let mut operations = Vec::with_capacity(paragraphs.len());
    let mut found = 0_usize;
    visit_paragraphs(document.body(), &mut |paragraph| {
        if !paragraphs.contains(&paragraph.id) {
            return;
        }
        found += 1;
        let direct = paragraph.properties.get();
        let outline_level = match level {
            Some(level) => Some(level - 1),
            None => {
                let mut without = paragraph.clone();
                without.properties = ParagraphProperties {
                    outline_level: None,
                    ..direct.clone()
                }
                .into();
                facts.heading_level(&without).map(|_| 9)
            }
        };
        if direct.outline_level == outline_level {
            return;
        }
        operations.push(Operation::SetParagraphProperties {
            node: paragraph.id,
            properties: Box::new(ParagraphProperties {
                outline_level,
                ..direct.clone()
            }),
        });
    });
    if found < paragraphs.len() {
        return Err(refused!(
            "toc.add-text-body-only",
            "Only a paragraph in the document body can be listed in the table of contents."
        )
        .to_owned());
    }
    Ok(operations)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default is byte-for-byte the instruction the editor always wrote,
    /// so every table already in a document reads back as itself.
    #[test]
    fn the_default_instruction_is_unchanged() {
        assert_eq!(toc_instruction(3), " TOC \\o \"1-3\" \\z \\u ");
        assert_eq!(
            TocOptions::from_instruction(" TOC \\o \"1-3\" \\z \\u "),
            TocOptions::default()
        );
    }

    /// Every switch the settings write is read back. Mutation: drop the `\h`
    /// read in `from_instruction` and the hyperlinked round trips go red.
    #[test]
    fn every_setting_the_instruction_carries_round_trips() {
        let cases = [
            TocOptions {
                max_level: 5,
                hyperlinks: true,
                page_numbers: false,
                ..TocOptions::default()
            },
            TocOptions {
                source: TocSource::Styles(vec![
                    ("Heading 1".to_owned(), 1),
                    ("Callout".to_owned(), 2),
                ]),
                ..TocOptions::default()
            },
            TocOptions {
                source: TocSource::Captions {
                    label: "Figure".to_owned(),
                    include_label: true,
                },
                hyperlinks: true,
                ..TocOptions::default()
            },
            TocOptions {
                source: TocSource::Captions {
                    label: "Table".to_owned(),
                    include_label: false,
                },
                ..TocOptions::default()
            },
        ];
        for options in cases {
            let instruction = options.instruction();
            assert_eq!(
                TocOptions::from_instruction(&instruction),
                options,
                "{instruction}"
            );
        }
    }

    /// Word's own instructions read as Word meant them.
    #[test]
    fn word_written_instructions_read_back() {
        let word = TocOptions::from_instruction(r#" TOC \o "1-3" \h \z \u "#);
        assert!(word.hyperlinks);
        assert_eq!(word.max_level, 3);
        let figures = TocOptions::from_instruction(r#" TOC \h \z \c "Figure" "#);
        assert_eq!(
            figures.source,
            TocSource::Captions {
                label: "Figure".to_owned(),
                include_label: true
            }
        );
        assert_eq!(toc_levels_of(r#" TOC \o "1-5" \h "#), 5);
        assert_eq!(toc_levels_of(r#" TOC \o "1-99" \h "#), MAX_TOC_LEVEL);
        assert_eq!(
            parse_style_list("Heading 1,1,Bad,12,Quote,2"),
            [("Heading 1".to_owned(), 1), ("Quote".to_owned(), 2)]
        );
    }

    /// Every format token parses back to its format.
    #[test]
    fn every_gallery_format_has_a_token() {
        for style in TocStyle::ALL {
            assert_eq!(TocStyle::parse(style.token()), Some(style));
        }
        assert_eq!(TocStyle::parse("fancy"), None);
    }
}
