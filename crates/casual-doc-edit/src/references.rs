//! Captions and cross-references: the OOXML field markup, and the model nodes
//! that carry it (`docs/105` OO-005).
//!
//! # The prior art, before any of our own invention
//!
//! Neither of these is a new problem, and OOXML already specifies both. This
//! module builds exactly what a real producer writes, and nothing else.
//!
//! **A caption is a paragraph with a numbered sequence field.** Word writes one
//! paragraph styled `Caption` holding a literal label run, a `SEQ` field, and the
//! author's own trailing text:
//!
//! ```xml
//! <w:p>
//!   <w:pPr><w:pStyle w:val="Caption"/></w:pPr>
//!   <w:r><w:t xml:space="preserve">Figure </w:t></w:r>
//!   <w:fldSimple w:instr=" SEQ Figure \* ARABIC "><w:r><w:t>1</w:t></w:r></w:fldSimple>
//!   <w:r><w:t xml:space="preserve">: Wiring diagram</w:t></w:r>
//! </w:p>
//! ```
//!
//! With *include chapter number* the same paragraph gains a `STYLEREF` for the
//! chapter, a literal separator, and the `SEQ` restarts at that heading level —
//! `STYLEREF 1 \s` then `-` then ` SEQ Figure \* ARABIC \s 1 `. That `\s`
//! argument is the whole mechanism behind "2-3" numbering; there is no second
//! counter.
//!
//! **A cross-reference is a `REF` field pointing at a bookmark.** Word bookmarks
//! the target (its own names are `_Ref` plus a decimal, which is why
//! [`REFERENCE_BOOKMARK_PREFIX`] matches) and writes one of a small closed set of
//! instructions — the bookmark's text, its page (`PAGEREF`), its paragraph number
//! (`\r`/`\n`/`\w`), or the word *above*/*below* (`\p`). `\h` makes it a
//! hyperlink. [`ReferenceTo`] is that closed set and nothing more.
//!
//! # What this module is, and is not
//!
//! Pure construction. Every function here is O(1) in document size: it takes a
//! spec plus already-resolved facts (the sequence number, the target's text, the
//! target's page) and returns model nodes. Nothing here reads a document, so
//! nothing here can hide a scan — finding the sequence number and the caption
//! list is the caller's job and is done with a single walk (see
//! `casual-doc-wasm`'s `references` module).
//!
//! # Deliberate divergence from Word, recorded here because it is a choice
//!
//! Word leaves a field's cached result **stale** until the user presses F9. We
//! compute the result at insertion (so a freshly inserted caption or
//! cross-reference is never blank or wrong) and expose an explicit update path
//! for the rest. A `SEQ` number is therefore correct the moment it is inserted,
//! and the numbers *after* it are renumbered by the same action — which Word also
//! does. `PAGEREF` results are layout-derived and can go stale after reflow; that
//! is the one place we match Word's stale-until-updated behaviour, because
//! recomputing it is O(document) and must not ride a keystroke.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Field, FieldKind, InlineNode, Paragraph, ParagraphProperties, Run, RunProperties, Style,
    StyleId, StyleKind,
};

use crate::EditError;

/// The paragraph-style name Word applies to a caption, and which we create when
/// the document has not got it yet.
pub const CAPTION_STYLE_NAME: &str = "Caption";

/// The prefix Word gives an automatically created cross-reference bookmark
/// (`_Ref` plus a decimal). Reusing it means a document we write and a document
/// Word writes are indistinguishable to a reader, and a `REF` field we emit
/// resolves in Word without it having to learn anything.
pub const REFERENCE_BOOKMARK_PREFIX: &str = "_Ref";

/// The built-in caption labels Word offers. A document may use any label; these
/// are the ones offered before the document has taught us any others.
pub const BUILT_IN_CAPTION_LABELS: [&str; 3] = ["Figure", "Table", "Equation"];

/// The number format a caption's `SEQ` field renders in (`\*` picture switch).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CaptionNumberFormat {
    /// `1, 2, 3` — `\* ARABIC`.
    #[default]
    Arabic,
    /// `I, II, III` — `\* ROMAN`.
    UpperRoman,
    /// `i, ii, iii` — `\* roman`.
    LowerRoman,
    /// `A, B, C` — `\* ALPHABETIC`.
    UpperLetter,
    /// `a, b, c` — `\* alphabetic`.
    LowerLetter,
}

impl CaptionNumberFormat {
    /// The `\*` picture switch Word writes for this format, without surrounding
    /// spaces. O(1).
    #[must_use]
    pub const fn switch(self) -> &'static str {
        match self {
            Self::Arabic => "\\* ARABIC",
            Self::UpperRoman => "\\* ROMAN",
            Self::LowerRoman => "\\* roman",
            Self::UpperLetter => "\\* ALPHABETIC",
            Self::LowerLetter => "\\* alphabetic",
        }
    }

    /// The stable wire name the host uses for this format (matches ONLYOFFICE's
    /// `CaptionNumberingFormat` vocabulary so a host that speaks one speaks
    /// both). O(1).
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Arabic => "arabic",
            Self::UpperRoman => "upperRoman",
            Self::LowerRoman => "lowerRoman",
            Self::UpperLetter => "upperLetter",
            Self::LowerLetter => "lowerLetter",
        }
    }

    /// Parses a [`wire_name`](Self::wire_name) (case-insensitive), or `None`.
    /// O(1).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        [
            Self::Arabic,
            Self::UpperRoman,
            Self::LowerRoman,
            Self::UpperLetter,
            Self::LowerLetter,
        ]
        .into_iter()
        .find(|format| format.wire_name().eq_ignore_ascii_case(name))
    }

    /// Renders `number` (1-based) in this format — the cached result text the
    /// `SEQ` field displays. A zero or a number past the alphabetic/roman range
    /// falls back to decimal rather than producing an empty run, because an empty
    /// run is not a legal model value and a silently missing number is worse than
    /// an unexpected one. O(number) at worst (roman), O(1) in document size.
    #[must_use]
    pub fn render(self, number: u32) -> String {
        if number == 0 {
            return "0".to_owned();
        }
        match self {
            Self::Arabic => number.to_string(),
            Self::UpperRoman => roman_numeral(number, true),
            Self::LowerRoman => roman_numeral(number, false),
            Self::UpperLetter => alphabetic(number, b'A'),
            Self::LowerLetter => alphabetic(number, b'a'),
        }
    }
}

/// `1 -> I`, `4 -> IV`, … Above 3,999 the additive notation runs out of symbols,
/// so the decimal is returned instead of an unbounded run of `M`s.
fn roman_numeral(number: u32, upper: bool) -> String {
    const VALUES: [(u32, &str, &str); 13] = [
        (1000, "M", "m"),
        (900, "CM", "cm"),
        (500, "D", "d"),
        (400, "CD", "cd"),
        (100, "C", "c"),
        (90, "XC", "xc"),
        (50, "L", "l"),
        (40, "XL", "xl"),
        (10, "X", "x"),
        (9, "IX", "ix"),
        (5, "V", "v"),
        (4, "IV", "iv"),
        (1, "I", "i"),
    ];
    if number > 3_999 {
        return number.to_string();
    }
    let mut left = number;
    let mut out = String::new();
    for (value, up, low) in VALUES {
        while left >= value {
            out.push_str(if upper { up } else { low });
            left -= value;
        }
    }
    out
}

/// `1 -> A`, `26 -> Z`, `27 -> AA` — Word's spreadsheet-style alphabetic
/// sequence. O(log_26 number); O(1) in document size.
fn alphabetic(number: u32, first: u8) -> String {
    let mut left = number;
    let mut letters = Vec::new();
    while left > 0 {
        let index = (left - 1) % 26;
        letters.push(first + u8::try_from(index).unwrap_or(0));
        left = (left - 1) / 26;
    }
    letters.reverse();
    String::from_utf8(letters).unwrap_or_else(|_| number.to_string())
}

/// Where a caption paragraph goes relative to the object it captions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CaptionPosition {
    /// Above the selected item (Word: "Above selected item").
    Above,
    /// Below the selected item (Word's default for figures).
    #[default]
    Below,
}

/// The literal character Word puts between the chapter number and the caption
/// number when *include chapter number* is on.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CaptionSeparator {
    /// `-` (Word's default).
    #[default]
    Hyphen,
    /// `.`
    Period,
    /// `:`
    Colon,
    /// `—`
    EmDash,
    /// `–`
    EnDash,
}

impl CaptionSeparator {
    /// The literal text of this separator. O(1).
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Self::Hyphen => "-",
            Self::Period => ".",
            Self::Colon => ":",
            Self::EmDash => "\u{2014}",
            Self::EnDash => "\u{2013}",
        }
    }

    /// The stable wire name the host uses (ONLYOFFICE's `CaptionSep`
    /// vocabulary). O(1).
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Hyphen => "hyphen",
            Self::Period => "period",
            Self::Colon => "colon",
            Self::EmDash => "emDash",
            Self::EnDash => "enDash",
        }
    }

    /// Parses a [`wire_name`](Self::wire_name) (case-insensitive), or `None`.
    /// O(1).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        [
            Self::Hyphen,
            Self::Period,
            Self::Colon,
            Self::EmDash,
            Self::EnDash,
        ]
        .into_iter()
        .find(|separator| separator.wire_name().eq_ignore_ascii_case(name))
    }
}

/// *Include chapter number*: the heading level the chapter counter comes from,
/// and the separator between it and the caption number.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptionChapter {
    /// The 1-based heading level (`1` = Heading 1). Word's dialog says "Chapter
    /// starts with style Heading N"; the field markup writes the same number
    /// into both `STYLEREF N \s` and `SEQ … \s N`.
    pub heading_level: u8,
    /// The literal separator between the chapter number and the caption number.
    pub separator: CaptionSeparator,
}

/// Everything the Insert Caption dialog collects.
#[derive(Clone, Debug)]
pub struct CaptionSpec {
    /// The caption label, e.g. `"Figure"`. Also the `SEQ` sequence name, which
    /// is why two labels never share a counter.
    pub label: String,
    /// The author's own trailing text, inserted verbatim after the number (Word
    /// and ONLYOFFICE both call this the *additional* text).
    pub text: String,
    /// Above or below the captioned item.
    pub position: CaptionPosition,
    /// *Exclude label from caption*: the number is inserted without the literal
    /// `"Figure "` run. The `SEQ` sequence name is unchanged, so the counter is
    /// still per-label.
    pub exclude_label: bool,
    /// The number format.
    pub number_format: CaptionNumberFormat,
    /// *Include chapter number*, or `None`.
    pub chapter: Option<CaptionChapter>,
}

impl CaptionSpec {
    /// The `SEQ` field instruction this caption carries, with Word's leading and
    /// trailing spaces — e.g. `" SEQ Figure \* ARABIC \s 1 "`. A label
    /// containing whitespace is quoted, as a producer must. O(label length).
    #[must_use]
    pub fn sequence_instruction(&self) -> String {
        let mut instruction = format!(
            " SEQ {} {}",
            quote_field_argument(&self.label),
            self.number_format.switch()
        );
        if let Some(chapter) = self.chapter {
            instruction.push_str(&format!(" \\s {}", chapter.heading_level));
        }
        instruction.push(' ');
        instruction
    }

    /// The `STYLEREF` instruction for the chapter number, when *include chapter
    /// number* is on. `STYLEREF N \s` is Word's exact markup: the nearest
    /// paragraph of the numbered heading style, showing its number rather than
    /// its text. O(1).
    #[must_use]
    pub fn chapter_instruction(&self) -> Option<String> {
        self.chapter
            .map(|chapter| format!(" STYLEREF {} \\s ", chapter.heading_level))
    }
}

/// Quotes a field-instruction argument if it needs it. Word quotes any argument
/// containing whitespace or a quote, and leaves a bare word bare — matching that
/// exactly is what makes a diff against a Word-written file empty.
/// O(argument length).
#[must_use]
pub fn quote_field_argument(argument: &str) -> String {
    if argument.is_empty()
        || argument
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\')
    {
        format!("\"{}\"", argument.replace('"', ""))
    } else {
        argument.to_owned()
    }
}

/// What a cross-reference displays. Word's *Insert reference to* list, restricted
/// to the entries that are meaningful for a given reference type; the mapping
/// from a type to its legal subset is the host's (it is UI policy, not document
/// semantics).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceTo {
    /// The whole bookmarked range: label, number and caption text
    /// (`REF name`). Word's "Entire caption".
    EntireCaption,
    /// Only the label and number (`REF name`, over a bookmark that covers only
    /// the label-and-number span). The instruction is the same as
    /// [`Self::EntireCaption`]; what differs is the *extent of the bookmark*,
    /// which is the caller's choice — recorded here so the two are not confused.
    LabelAndNumber,
    /// Only the caption text after the number (`REF name`, over the text span).
    /// Same instruction, different bookmark extent.
    CaptionTextOnly,
    /// The page the target is on (`PAGEREF name`).
    PageNumber,
    /// The word *above* or *below* (`REF name \p`).
    AboveBelow,
    /// The target paragraph's text (`REF name`) — the entry Word offers for a
    /// heading or a bookmark.
    ParagraphText,
    /// The target paragraph's number, in the shortest form that is
    /// unambiguous from the reference's position (`REF name \r`).
    ParagraphNumber,
    /// The target paragraph's number with no surrounding context
    /// (`REF name \n`).
    ParagraphNumberNoContext,
    /// The target paragraph's number in full context (`REF name \w`).
    ParagraphNumberFullContext,
}

impl ReferenceTo {
    /// The stable wire name the host uses. O(1).
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::EntireCaption => "entireCaption",
            Self::LabelAndNumber => "labelAndNumber",
            Self::CaptionTextOnly => "captionText",
            Self::PageNumber => "pageNumber",
            Self::AboveBelow => "aboveBelow",
            Self::ParagraphText => "paragraphText",
            Self::ParagraphNumber => "paragraphNumber",
            Self::ParagraphNumberNoContext => "paragraphNumberNoContext",
            Self::ParagraphNumberFullContext => "paragraphNumberFullContext",
        }
    }

    /// Parses a [`wire_name`](Self::wire_name) (case-insensitive), or `None`.
    /// O(1).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        [
            Self::EntireCaption,
            Self::LabelAndNumber,
            Self::CaptionTextOnly,
            Self::PageNumber,
            Self::AboveBelow,
            Self::ParagraphText,
            Self::ParagraphNumber,
            Self::ParagraphNumberNoContext,
            Self::ParagraphNumberFullContext,
        ]
        .into_iter()
        .find(|to| to.wire_name().eq_ignore_ascii_case(name))
    }

    /// Whether this reference's value comes from **layout** rather than from the
    /// model. Only two do, and both are therefore the ones whose cached result
    /// can go stale after a reflow. O(1).
    #[must_use]
    pub const fn is_layout_derived(self) -> bool {
        matches!(self, Self::PageNumber | Self::AboveBelow)
    }

    /// The field instruction for a reference of this kind to `bookmark`, with
    /// Word's leading and trailing spaces. `hyperlink` adds `\h`, which is what
    /// Word's *Insert as hyperlink* checkbox writes. O(bookmark length).
    #[must_use]
    pub fn instruction(self, bookmark: &str, hyperlink: bool) -> String {
        let keyword = if self == Self::PageNumber {
            "PAGEREF"
        } else {
            "REF"
        };
        let mut instruction = format!(" {keyword} {}", quote_field_argument(bookmark));
        match self {
            Self::AboveBelow => instruction.push_str(" \\p"),
            Self::ParagraphNumber => instruction.push_str(" \\r"),
            Self::ParagraphNumberNoContext => instruction.push_str(" \\n"),
            Self::ParagraphNumberFullContext => instruction.push_str(" \\w"),
            Self::EntireCaption
            | Self::LabelAndNumber
            | Self::CaptionTextOnly
            | Self::PageNumber
            | Self::ParagraphText => {}
        }
        if hyperlink {
            instruction.push_str(" \\h");
        }
        instruction.push(' ');
        instruction
    }
}

/// The inline nodes of a caption paragraph, in document order.
///
/// `number` is the already-resolved sequence number's display text and
/// `chapter_number` the already-resolved chapter number (both computed by the
/// caller's single document walk), so this function does no lookup of any kind.
/// O(1) in document size; O(text length) in the caption's own text.
///
/// `next_id` allocates the fresh node ids. It is called at most six times.
pub fn caption_inlines(
    spec: &CaptionSpec,
    number: &str,
    chapter_number: &str,
    next_id: &mut impl FnMut() -> Result<NodeId, EditError>,
) -> Result<Vec<InlineNode>, EditError> {
    let mut inlines = Vec::new();
    // The literal label run: "Figure " — omitted by *exclude label from caption*,
    // which leaves the number (and so the SEQ field) in place. Word writes the
    // trailing space inside this run, not as a separate one.
    if !spec.exclude_label {
        inlines.push(text_run(next_id()?, &format!("{} ", spec.label)));
    }
    // The chapter number and its separator, when *include chapter number* is on:
    // a STYLEREF field for the chapter, then a literal separator run.
    if let (Some(chapter), Some(instruction)) = (spec.chapter, spec.chapter_instruction()) {
        inlines.push(InlineNode::Field(Box::new(field_node(
            next_id()?,
            next_id()?,
            instruction,
            chapter_number,
        )?)));
        inlines.push(text_run(next_id()?, chapter.separator.text()));
    }
    // The SEQ field: the caption's number.
    inlines.push(InlineNode::Field(Box::new(field_node(
        next_id()?,
        next_id()?,
        spec.sequence_instruction(),
        number,
    )?)));
    // The author's own text, verbatim.
    if !spec.text.is_empty() {
        inlines.push(text_run(next_id()?, &spec.text));
    }
    Ok(inlines)
}

/// A whole caption paragraph: [`caption_inlines`] under `style`, with Word's
/// `keepNext` so a caption above its figure cannot be orphaned by a page break.
/// O(1) in document size.
pub fn caption_paragraph(
    id: NodeId,
    style: Option<StyleId>,
    spec: &CaptionSpec,
    number: &str,
    chapter_number: &str,
    next_id: &mut impl FnMut() -> Result<NodeId, EditError>,
) -> Result<Paragraph, EditError> {
    let properties = ParagraphProperties {
        style_ref: style,
        // A caption above its item must stay with it; Word's own Caption style
        // carries `keepNext` for exactly this reason, and a document whose
        // Caption style predates us may not.
        keep_next: Some(spec.position == CaptionPosition::Above),
        ..ParagraphProperties::default()
    };
    Ok(Paragraph {
        id,
        properties: properties.into(),
        inlines: caption_inlines(spec, number, chapter_number, next_id)?,
    })
}

/// The cross-reference field to insert at the caret: a `REF`/`PAGEREF` whose
/// cached result is `result_text` (the target's text, its page, or
/// *above*/*below*, resolved by the caller). O(1) in document size.
pub fn reference_field(
    id: NodeId,
    result_id: NodeId,
    to: ReferenceTo,
    bookmark: &str,
    hyperlink: bool,
    result_text: &str,
) -> Result<Field, EditError> {
    field_node(
        id,
        result_id,
        to.instruction(bookmark, hyperlink),
        result_text,
    )
}

/// The word Word's `REF … \p` renders: the target is *above* the reference when
/// it comes earlier in document order, *below* when later. Taking two
/// already-known document-order indices rather than two `NodeId`s is deliberate:
/// resolving a `NodeId` to its position is a whole-document walk, and this is
/// called once per inserted reference from a caller that already walked. O(1).
#[must_use]
pub fn above_below(target_order: usize, reference_order: usize) -> &'static str {
    if target_order <= reference_order {
        "above"
    } else {
        "below"
    }
}

/// Word's built-in `Caption` paragraph style, for a document that has not got
/// one. Word creates it on first caption insertion, so we do too — a caption
/// whose style silently did not exist would render as body text, which is the
/// "modeled is not shipped" failure in miniature.
///
/// The values are Word's own built-in definition: bold, 9 pt, `keepNext`, and
/// `basedOn` whatever the document's default paragraph style is (passed in, since
/// this module reads no document). O(1).
#[must_use]
pub fn caption_style(based_on: Option<StyleId>) -> Style {
    let run = RunProperties {
        bold: Some(true),
        size_half_points: Some(18),
        ..RunProperties::default()
    };
    let paragraph = ParagraphProperties {
        keep_next: Some(true),
        ..ParagraphProperties::default()
    };
    Style {
        kind: StyleKind::Paragraph,
        is_default: false,
        name: Some(CAPTION_STYLE_NAME.to_owned()),
        aliases: None,
        based_on,
        next: None,
        link: None,
        hidden: false,
        ui_priority: Some(35),
        semi_hidden: false,
        unhide_when_used: true,
        q_format: true,
        locked: false,
        paragraph: Some(paragraph),
        run: Some(run),
        table: None,
        table_row: None,
        table_cell: None,
        conditional: Vec::new(),
    }
}

/// A default-styled text run. `text` must be non-empty (the model forbids an
/// empty run); every caller here checks.
fn text_run(id: NodeId, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id,
        properties: RunProperties::default().into(),
        text: text.to_owned(),
    })
}

/// O(1) in document size, O(instruction + result length) in its arguments.
///
/// A field node carrying `instruction` and a single cached-result run. An empty
/// `result` yields a field with no cached inlines rather than an illegal empty
/// run — a field whose result a reader has not computed yet is legal OOXML and is
/// exactly what a producer writes before its first update.
fn field_node(
    id: NodeId,
    result_id: NodeId,
    instruction: String,
    result: &str,
) -> Result<Field, EditError> {
    if instruction.len() > casual_doc_model::v1::MAX_FIELD_INSTRUCTION_BYTES {
        return Err(EditError::ValueTooLarge);
    }
    let kind = FieldKind::parse(&instruction);
    let inlines = if result.is_empty() {
        Vec::new()
    } else {
        vec![text_run(result_id, result)]
    };
    Ok(Field {
        id,
        instruction,
        kind,
        inlines,
        form: None,
        // Neither locked nor dirty: the cached result was just computed, and
        // freezing a field is a later author decision.
        update: casual_doc_model::v1::FieldUpdateState::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> impl FnMut() -> Result<NodeId, EditError> {
        let mut next = 1u64;
        move || {
            next += 1;
            NodeId::from_parts(70, next).map_err(|_| EditError::IdExhausted)
        }
    }

    fn spec(label: &str) -> CaptionSpec {
        CaptionSpec {
            label: label.to_owned(),
            text: ": Wiring diagram".to_owned(),
            position: CaptionPosition::Below,
            exclude_label: false,
            number_format: CaptionNumberFormat::Arabic,
            chapter: None,
        }
    }

    #[test]
    fn a_caption_writes_words_own_seq_instruction() {
        assert_eq!(
            spec("Figure").sequence_instruction(),
            " SEQ Figure \\* ARABIC "
        );
    }

    #[test]
    fn a_label_with_a_space_is_quoted_like_word_quotes_it() {
        assert_eq!(
            spec("Code Listing").sequence_instruction(),
            " SEQ \"Code Listing\" \\* ARABIC "
        );
    }

    #[test]
    fn include_chapter_number_writes_the_restart_switch_and_a_styleref() {
        let mut spec = spec("Figure");
        spec.chapter = Some(CaptionChapter {
            heading_level: 1,
            separator: CaptionSeparator::Hyphen,
        });
        assert_eq!(spec.sequence_instruction(), " SEQ Figure \\* ARABIC \\s 1 ");
        assert_eq!(
            spec.chapter_instruction().as_deref(),
            Some(" STYLEREF 1 \\s ")
        );
    }

    #[test]
    fn a_caption_paragraph_is_label_then_seq_then_text() {
        let mut next = ids();
        let inlines = caption_inlines(&spec("Figure"), "1", "", &mut next).expect("inlines");
        assert_eq!(inlines.len(), 3);
        match &inlines[0] {
            InlineNode::Run(run) => assert_eq!(run.text, "Figure "),
            other => panic!("expected a label run, got {other:?}"),
        }
        match &inlines[1] {
            InlineNode::Field(field) => {
                assert_eq!(
                    field.kind,
                    FieldKind::Seq {
                        name: "Figure".to_owned()
                    }
                );
                match &field.inlines[..] {
                    [InlineNode::Run(run)] => assert_eq!(run.text, "1"),
                    other => panic!("expected one cached run, got {other:?}"),
                }
            }
            other => panic!("expected a SEQ field, got {other:?}"),
        }
        match &inlines[2] {
            InlineNode::Run(run) => assert_eq!(run.text, ": Wiring diagram"),
            other => panic!("expected the author's text, got {other:?}"),
        }
    }

    #[test]
    fn exclude_label_drops_the_label_run_but_keeps_the_counter_per_label() {
        let mut spec = spec("Figure");
        spec.exclude_label = true;
        let mut next = ids();
        let inlines = caption_inlines(&spec, "1", "", &mut next).expect("inlines");
        assert!(
            !inlines.iter().any(|inline| matches!(
                inline,
                InlineNode::Run(run) if run.text.contains("Figure")
            )),
            "the literal label run must be gone"
        );
        match &inlines[0] {
            InlineNode::Field(field) => assert_eq!(
                field.kind,
                FieldKind::Seq {
                    name: "Figure".to_owned()
                }
            ),
            other => panic!("the SEQ field must still name the label, got {other:?}"),
        }
    }

    #[test]
    fn chapter_numbering_orders_styleref_separator_seq() {
        let mut spec = spec("Figure");
        spec.chapter = Some(CaptionChapter {
            heading_level: 2,
            separator: CaptionSeparator::Period,
        });
        let mut next = ids();
        let inlines = caption_inlines(&spec, "3", "2", &mut next).expect("inlines");
        let kinds: Vec<&str> = inlines
            .iter()
            .map(|inline| match inline {
                InlineNode::Run(_) => "run",
                InlineNode::Field(_) => "field",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["run", "field", "run", "field", "run"]);
        match (&inlines[1], &inlines[2]) {
            (InlineNode::Field(styleref), InlineNode::Run(separator)) => {
                assert_eq!(styleref.instruction, " STYLEREF 2 \\s ");
                assert_eq!(separator.text, ".");
            }
            other => panic!("expected STYLEREF then a separator, got {other:?}"),
        }
    }

    #[test]
    fn every_reference_kind_writes_the_instruction_word_writes() {
        let cases = [
            (ReferenceTo::EntireCaption, " REF _Ref1 "),
            (ReferenceTo::LabelAndNumber, " REF _Ref1 "),
            (ReferenceTo::CaptionTextOnly, " REF _Ref1 "),
            (ReferenceTo::PageNumber, " PAGEREF _Ref1 "),
            (ReferenceTo::AboveBelow, " REF _Ref1 \\p "),
            (ReferenceTo::ParagraphText, " REF _Ref1 "),
            (ReferenceTo::ParagraphNumber, " REF _Ref1 \\r "),
            (ReferenceTo::ParagraphNumberNoContext, " REF _Ref1 \\n "),
            (ReferenceTo::ParagraphNumberFullContext, " REF _Ref1 \\w "),
        ];
        for (to, expected) in cases {
            assert_eq!(
                to.instruction("_Ref1", false),
                expected,
                "instruction for {}",
                to.wire_name()
            );
        }
    }

    #[test]
    fn insert_as_hyperlink_appends_the_h_switch_last() {
        assert_eq!(
            ReferenceTo::PageNumber.instruction("_Ref1", true),
            " PAGEREF _Ref1 \\h "
        );
        assert_eq!(
            ReferenceTo::AboveBelow.instruction("_Ref1", true),
            " REF _Ref1 \\p \\h "
        );
    }

    #[test]
    fn a_reference_field_projects_to_the_typed_kind_the_model_knows() {
        let field = reference_field(
            NodeId::from_parts(70, 1).expect("a fresh id"),
            NodeId::from_parts(70, 2).expect("a fresh id"),
            ReferenceTo::PageNumber,
            "_Ref1",
            true,
            "7",
        )
        .expect("field");
        assert_eq!(
            field.kind,
            FieldKind::PageRef {
                bookmark: "_Ref1".to_owned()
            }
        );
        match &field.inlines[..] {
            [InlineNode::Run(run)] => assert_eq!(run.text, "7"),
            other => panic!("expected the cached page number, got {other:?}"),
        }
    }

    #[test]
    fn a_field_with_no_computed_result_has_no_cached_run_rather_than_an_empty_one() {
        let field = reference_field(
            NodeId::from_parts(70, 1).expect("a fresh id"),
            NodeId::from_parts(70, 2).expect("a fresh id"),
            ReferenceTo::PageNumber,
            "_Ref1",
            false,
            "",
        )
        .expect("field");
        assert!(field.inlines.is_empty());
    }

    #[test]
    fn number_formats_render_the_sequences_word_renders() {
        assert_eq!(CaptionNumberFormat::Arabic.render(12), "12");
        assert_eq!(CaptionNumberFormat::UpperRoman.render(4), "IV");
        assert_eq!(CaptionNumberFormat::LowerRoman.render(9), "ix");
        assert_eq!(CaptionNumberFormat::UpperLetter.render(1), "A");
        assert_eq!(CaptionNumberFormat::UpperLetter.render(27), "AA");
        assert_eq!(CaptionNumberFormat::LowerLetter.render(26), "z");
        // Out of range rather than an unbounded run of M's or an empty run.
        assert_eq!(CaptionNumberFormat::UpperRoman.render(4_000), "4000");
        assert_eq!(CaptionNumberFormat::Arabic.render(0), "0");
    }

    #[test]
    fn wire_names_round_trip_for_every_variant() {
        for format in [
            CaptionNumberFormat::Arabic,
            CaptionNumberFormat::UpperRoman,
            CaptionNumberFormat::LowerRoman,
            CaptionNumberFormat::UpperLetter,
            CaptionNumberFormat::LowerLetter,
        ] {
            assert_eq!(CaptionNumberFormat::parse(format.wire_name()), Some(format));
        }
        for separator in [
            CaptionSeparator::Hyphen,
            CaptionSeparator::Period,
            CaptionSeparator::Colon,
            CaptionSeparator::EmDash,
            CaptionSeparator::EnDash,
        ] {
            assert_eq!(
                CaptionSeparator::parse(separator.wire_name()),
                Some(separator)
            );
        }
        for to in [
            ReferenceTo::EntireCaption,
            ReferenceTo::LabelAndNumber,
            ReferenceTo::CaptionTextOnly,
            ReferenceTo::PageNumber,
            ReferenceTo::AboveBelow,
            ReferenceTo::ParagraphText,
            ReferenceTo::ParagraphNumber,
            ReferenceTo::ParagraphNumberNoContext,
            ReferenceTo::ParagraphNumberFullContext,
        ] {
            assert_eq!(ReferenceTo::parse(to.wire_name()), Some(to));
        }
    }

    #[test]
    fn only_the_layout_derived_reference_kinds_say_so() {
        assert!(ReferenceTo::PageNumber.is_layout_derived());
        assert!(ReferenceTo::AboveBelow.is_layout_derived());
        assert!(!ReferenceTo::EntireCaption.is_layout_derived());
        assert!(!ReferenceTo::ParagraphNumber.is_layout_derived());
    }

    #[test]
    fn above_below_follows_document_order() {
        assert_eq!(above_below(2, 9), "above");
        assert_eq!(above_below(9, 2), "below");
        // A reference inside its own target reads "above", which is what Word
        // renders and is the only non-arbitrary answer.
        assert_eq!(above_below(4, 4), "above");
    }

    #[test]
    fn a_caption_above_its_item_keeps_with_the_next_block() {
        let mut above = spec("Table");
        above.position = CaptionPosition::Above;
        let mut next = ids();
        let paragraph = caption_paragraph(
            NodeId::from_parts(70, 1).expect("a fresh id"),
            None,
            &above,
            "1",
            "",
            &mut next,
        )
        .expect("paragraph");
        assert_eq!(paragraph.properties.get().keep_next, Some(true));
    }
}
