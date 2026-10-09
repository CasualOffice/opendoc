//! The walk: one pass over the body in document order, resolving every node
//! through the same cascade the page is painted with.
//!
//! # Shape
//!
//! Two buffers. The body is written first, because only the body knows which
//! styles it used; the head — whose stylesheet holds one class per paragraph
//! style the body reached — is written last and placed in front. Each element
//! then carries only where it differs from its class (`css::Declarations::delta`),
//! which is the *normalization* Word's and LibreOffice's own HTML filters use:
//! a style's formatting is stated once, not on every paragraph.
//!
//! # Vertical spacing is Word's, not CSS's
//!
//! CSS collapses adjacent margins to the larger; Word adds a paragraph's space
//! after to the next one's space before. So no paragraph writes a bottom
//! margin except the last in its container: each writes, as its top margin,
//! its own space before plus the space after of the one above it, with
//! `w:contextualSpacing` suppressing both between paragraphs of one style.
//!
//! # Complexity
//!
//! O(nodes) for the walk. Style resolution walks a style's `basedOn` chain,
//! bounded by the cascade; a table's per-cell layers and border candidates are
//! computed once per table. Media are one B-tree lookup per picture.

use std::collections::BTreeMap;

use casual_doc_layout::cascade::{StyleCascade, TableStyleLayer, requested_font_family};
use casual_doc_layout::font_substitution::DeclaredFamilies;
use casual_doc_layout::numbering::NumberingState;
use casual_doc_layout::paint_values::{
    NoteLabels, PaintPalette, cell_margins, list_indent, marker_glyphs, note_labels,
    table_cell_borders, table_cell_layers,
};
use casual_doc_model::v1::{
    BlockNode, BorderEdge, BreakKind, CellVerticalAlignment, Definitions, Document, DrawingAnchor,
    EmbeddedKind, Extent, GroupChild, HeightRule, HorizontalAlign, HorizontalPosition,
    HyperlinkTarget, InlineNode, LevelSuffix, MediaId, NoteId, NoteKind, NumberFormat, Paragraph,
    ParagraphProperties, RevisionKind, RunProperties, SectionBoundary, SectionType, StyleId, Table,
    TableWidth, TextBoxVerticalAnchor, TextDirection, VerticalAlignment, VerticalMerge, WidthType,
    WordprocessingGroup, WrapMode,
};

use super::css::{
    self, DEFAULT_SIZE_HALF_POINTS, Declarations, Fonts, css_string, hex, paragraph_css,
    paragraph_space, points, run_css, twips,
};
use super::{HtmlLimits, Losses, base64, enforce, escape_attribute, escape_text};
use crate::{AdapterError, DocumentResources, ModelOutcome};

/// The fixed part of the stylesheet: resets that let the document's own
/// formatting decide, instead of the browser's defaults for `<h1>`, `<th>`,
/// `<a>` and lists. Every visual fact beyond these comes from the document.
const BASE_STYLESHEET: &str = "\
*{box-sizing:border-box}\
body{margin:0 auto;padding:24pt 16pt;overflow-wrap:break-word}\
p,h1,h2,h3,h4,h5,h6,li{margin:0;font-size:inherit;font-weight:inherit}\
ul.list,ol.list{margin:0;padding:0}\
li{list-style-position:outside}\
table{border-collapse:collapse;border-spacing:0}\
td,th{padding:0;vertical-align:top;text-align:inherit;font-weight:inherit;border:none}\
img{max-width:100%;height:auto;vertical-align:baseline}\
a{color:inherit;text-decoration:none}\
sup,sub{font-size:inherit;vertical-align:baseline;line-height:0}\
.tab{white-space:pre}\
.note-ref{font-size:66.67%;vertical-align:.5em;line-height:0}\
.note-ref a,.note-back{color:inherit}\
.notes{margin-top:24pt}\
hr{border:0;border-top:1px solid #999}\
@media print{body{max-width:none;padding:0}}";

/// Where the walk is: the nesting it has descended through and the table
/// style layer that applies to the paragraphs it is in.
#[derive(Clone, Copy)]
struct Cx<'t> {
    depth: usize,
    layer: Option<&'t TableStyleLayer>,
}

/// The vertical-spacing state of one container (`body`, a cell, a text box).
#[derive(Default)]
struct Flow {
    /// The space after of the previous paragraph, owed to the next one.
    pending_after: i64,
    /// The previous paragraph's style and whether it asked for contextual
    /// spacing.
    previous: Option<(Option<StyleId>, bool)>,
    /// A section break owes the next block a page break.
    page_break: bool,
}

/// A list level the walk has open.
struct OpenList {
    level: u8,
    tag: &'static str,
    /// Where this list's items are measured from: the content edge of the item
    /// that contains it, in twips from the container's edge.
    origin: i64,
    /// The start indent of the item open in this list, if one is.
    item: Option<i64>,
}

/// The single-file HTML writer.
pub(super) struct Writer<'d> {
    body: String,
    limits: HtmlLimits,
    document: &'d Document,
    definitions: &'d Definitions,
    resources: &'d DocumentResources,
    cascade: StyleCascade<'d>,
    palette: PaintPalette,
    fonts: Fonts<'d>,
    numbering: NumberingState,
    note_labels: NoteLabels,
    /// The notes the body referenced, in order of first reference.
    notes: Vec<(NoteKind, NoteId)>,
    /// The class each paragraph style became, and each class's declarations.
    class_of: BTreeMap<Option<StyleId>, String>,
    rules: BTreeMap<String, Declarations>,
    /// The note being written, while its blocks are walked: its anchor index
    /// and label, for the number at its head.
    current_note: Option<(usize, NoteKind)>,
    document_language: Option<String>,
    losses: Losses,
}

impl<'d> Writer<'d> {
    pub(super) fn new(
        document: &'d Document,
        resources: &'d DocumentResources,
        limits: HtmlLimits,
    ) -> Self {
        let definitions = document.definitions();
        let cascade = StyleCascade::new(definitions);
        let document_language = cascade
            .resolve_run(
                cascade.paragraph_style(&ParagraphProperties::default()),
                &RunProperties::default(),
            )
            .language
            .and_then(|language| language.value)
            .and_then(|tag| language_tag(&tag));
        Self {
            body: String::new(),
            limits,
            document,
            definitions,
            resources,
            cascade,
            palette: PaintPalette::new(document),
            fonts: Fonts {
                scheme: definitions.font_scheme.as_ref(),
                declared: DeclaredFamilies::from_font_table(&definitions.font_table),
            },
            numbering: NumberingState::new(),
            note_labels: note_labels(document),
            notes: Vec::new(),
            class_of: BTreeMap::new(),
            rules: BTreeMap::new(),
            current_note: None,
            document_language,
            losses: Losses::default(),
        }
    }

    fn push(&mut self, value: &str) -> Result<(), AdapterError> {
        let observed = self.body.len().saturating_add(value.len());
        enforce("html_output_bytes", observed, self.limits.max_output_bytes)?;
        self.body.push_str(value);
        Ok(())
    }

    /// Writes the whole document and returns its bytes and what it lost.
    pub(super) fn write(mut self) -> Result<(Vec<u8>, Losses), AdapterError> {
        let sections = &self.definitions.sections;
        let first = sections.first();
        self.report_sections(sections);

        let mut flow = Flow::default();
        let cx = Cx {
            depth: 0,
            layer: None,
        };
        let body = self.document.body();
        let mut section = 0;
        let mut start = 0;
        // The body is cut at each section break so a multi-column section can
        // be wrapped in the columns it is laid out in.
        for (index, block) in body.iter().enumerate() {
            let ends_section = matches!(block, BlockNode::Paragraph(paragraph)
                if paragraph.properties.section_break.is_some());
            if ends_section || index + 1 == body.len() {
                self.section(&body[start..=index], sections.get(section), cx, &mut flow)?;
                start = index + 1;
                if ends_section {
                    section += 1;
                    let next_type = sections.get(section).and_then(|next| next.section_type);
                    flow.page_break = !matches!(
                        next_type,
                        Some(SectionType::Continuous | SectionType::NextColumn)
                    );
                }
            }
        }
        self.write_notes()?;

        if self
            .document
            .properties()
            .is_some_and(|properties| properties.core.creator.is_some())
        {
            // An author name in a file that gets published is a privacy
            // decision, not a formatting one; it is not smuggled into `<meta>`.
            self.losses
                .record("html.document_author", ModelOutcome::Omitted);
        }

        let head = self.head(first)?;
        let total = head.len() + self.body.len() + "</body>\n</html>\n".len();
        enforce("html_output_bytes", total, self.limits.max_output_bytes)?;
        let mut out = head;
        out.push_str(&self.body);
        out.push_str("</body>\n</html>\n");
        Ok((out.into_bytes(), self.losses))
    }

    /// Records what the sections carry that a single web page cannot.
    fn report_sections(&mut self, sections: &[SectionBoundary]) {
        if sections
            .iter()
            .any(|section| !section.headers.is_empty() || !section.footers.is_empty())
        {
            // A web page has no pages to repeat a header on. The header is
            // still in the document; it is not in this file, and that is said.
            self.losses
                .record("html.header_footer", ModelOutcome::Omitted);
        }
        if sections.iter().any(|section| section.watermark.is_some()) {
            self.losses.record("html.watermark", ModelOutcome::Omitted);
        }
        if let Some(first) = sections.first()
            && sections.iter().skip(1).any(|section| {
                section.page_size != first.page_size || section.page_margins != first.page_margins
            })
        {
            // One page size per file: `@page` takes the first section's.
            self.losses
                .record("html.section_layout", ModelOutcome::Degraded);
        }
    }

    /// One section's blocks, in its columns when it has more than one.
    fn section(
        &mut self,
        blocks: &[BlockNode],
        section: Option<&SectionBoundary>,
        cx: Cx<'_>,
        flow: &mut Flow,
    ) -> Result<(), AdapterError> {
        let columns = section.filter(|section| section.columns.count > 1);
        if let Some(section) = columns {
            let gap = section.columns.space_twips.unwrap_or(720);
            let mut css = format!(
                "column-count:{};column-gap:{}",
                section.columns.count,
                twips(i64::from(gap))
            );
            if section.columns.separator == Some(true) {
                css.push_str(";column-rule:1px solid #000");
            }
            if flow.page_break {
                css.push_str(";break-before:page");
                flow.page_break = false;
            }
            self.push("<div style=\"")?;
            self.push(&css)?;
            self.push("\">\n")?;
        }
        self.blocks(blocks, cx, flow)?;
        if columns.is_some() {
            self.push("</div>\n")?;
        }
        Ok(())
    }

    fn head(&self, first: Option<&SectionBoundary>) -> Result<String, AdapterError> {
        let mut head = String::new();
        head.push_str("<!DOCTYPE html>\n<html");
        if let Some(language) = &self.document_language {
            head.push_str(" lang=\"");
            head.push_str(&escape_attribute(language));
            head.push('"');
        }
        head.push_str(">\n<head>\n<meta charset=\"utf-8\">\n");
        head.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
        let title = self
            .document
            .properties()
            .and_then(|properties| properties.core.title.clone())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "Document".to_owned());
        head.push_str("<title>");
        head.push_str(&escape_text(&title));
        head.push_str("</title>\n<style>\n");
        head.push_str(BASE_STYLESHEET);
        head.push('\n');

        // The page: what a browser prints on, and the measure the screen
        // shows the text in — the first section's text column.
        let mut body = Declarations::default();
        if let Some(section) = first {
            let size = section.page_size;
            let margins = section.page_margins;
            let measure = i64::from(size.width_twips)
                - i64::from(margins.start_twips)
                - i64::from(margins.end_twips);
            if measure > 0 {
                body.set("max-width", twips(measure));
            }
            head.push_str(&format!(
                "@page{{size:{} {};margin:{} {} {} {}}}\n",
                twips(i64::from(size.width_twips)),
                twips(i64::from(size.height_twips)),
                twips(i64::from(margins.top_twips).max(0)),
                twips(i64::from(margins.end_twips).max(0)),
                twips(i64::from(margins.bottom_twips).max(0)),
                twips(i64::from(margins.start_twips).max(0)),
            ));
        }
        let default_tab = self.definitions.settings.default_tab_stop.unwrap_or(720);
        body.set("tab-size", twips(i64::from(default_tab.max(1))));
        let defaults = run_css(
            &self.cascade.resolve_run(None, &RunProperties::default()),
            &self.fonts,
            &self.palette,
        );
        body.extend(&defaults.inherited);
        if let Some(background) = self.document.background() {
            body.set(
                "background-color",
                hex([background.r, background.g, background.b, 255]),
            );
        }
        head.push_str("body{");
        head.push_str(&body.to_css());
        head.push_str("}\n");
        for (class, rule) in &self.rules {
            if rule.is_empty() {
                continue;
            }
            head.push('.');
            head.push_str(class);
            head.push('{');
            head.push_str(&rule.to_css());
            head.push_str("}\n");
        }
        head.push_str("</style>\n</head>\n<body>\n");
        enforce(
            "html_output_bytes",
            head.len(),
            self.limits.max_output_bytes,
        )?;
        Ok(head)
    }

    // ---- blocks ------------------------------------------------------------

    fn blocks(
        &mut self,
        blocks: &[BlockNode],
        cx: Cx<'_>,
        flow: &mut Flow,
    ) -> Result<(), AdapterError> {
        let mut index = 0;
        while index < blocks.len() {
            if self.list_item(&blocks[index], cx).is_some() {
                let mut end = index + 1;
                while end < blocks.len() && self.list_item(&blocks[end], cx).is_some() {
                    end += 1;
                }
                self.list(&blocks[index..end], cx, flow, end == blocks.len())?;
                index = end;
            } else {
                self.block(&blocks[index], cx, flow, index + 1 == blocks.len())?;
                index += 1;
            }
        }
        Ok(())
    }

    fn block(
        &mut self,
        block: &BlockNode,
        cx: Cx<'_>,
        flow: &mut Flow,
        last: bool,
    ) -> Result<(), AdapterError> {
        match block {
            BlockNode::Paragraph(paragraph) => self.paragraph(paragraph, cx, flow, last, None),
            BlockNode::Table(table) => self.table(table, cx, flow),
            BlockNode::Sdt(sdt) => {
                // A content control is a container, and `<div>` is the honest
                // HTML for one: the control's behaviour does not survive, the
                // grouping does. Its paragraphs keep their neighbours' spacing.
                self.losses
                    .record("html.content_control", ModelOutcome::Degraded);
                self.push("<div>\n")?;
                self.blocks(&sdt.blocks, cx, flow)?;
                self.push("</div>\n")
            }
            BlockNode::AltChunk(_) => {
                self.losses.record("html.alt_chunk", ModelOutcome::Omitted);
                Ok(())
            }
        }
    }

    /// The effective paragraph properties: document defaults, the table style
    /// layer, the paragraph style chain, then direct formatting.
    fn resolve(&self, paragraph: &Paragraph, cx: Cx<'_>) -> ParagraphProperties {
        self.cascade
            .resolve_paragraph_in_table(&paragraph.properties, cx.layer)
    }

    /// Whether a block is a list item — a numbered, non-heading paragraph whose
    /// numbering resolves — and its level.
    fn list_item(&self, block: &BlockNode, cx: Cx<'_>) -> Option<u8> {
        let BlockNode::Paragraph(paragraph) = block else {
            return None;
        };
        let effective = self.resolve(paragraph, cx);
        if effective.numbering_none || self.heading_level(&effective).is_some() {
            return None;
        }
        let reference = effective.numbering?;
        self.definitions
            .numbering_resolver()
            .level(reference)
            .map(|_| reference.level)
    }

    /// The heading level a paragraph's style names, 1-6, or `None`: the
    /// style's outline level, which is what Word's navigation pane reads, with
    /// the name as the fallback. HTML stops at six.
    fn heading_level(&self, effective: &ParagraphProperties) -> Option<u8> {
        let style_id = self.cascade.paragraph_style(effective)?;
        let style = self.definitions.styles.get(&style_id)?;
        let from_outline = effective
            .outline_level
            .or_else(|| {
                style
                    .paragraph
                    .as_ref()
                    .and_then(|properties| properties.outline_level)
            })
            .filter(|level| *level < 9)
            .map(|level| level.saturating_add(1));
        let from_name = style.name.as_deref().and_then(heading_level_from_name);
        let level = from_outline.or(from_name)?;
        (1..=9).contains(&level).then(|| level.min(6))
    }

    /// The class for a paragraph style: its resolved paragraph box and the run
    /// formatting it gives every run, stated once in the stylesheet.
    fn class(&mut self, style: Option<StyleId>) -> String {
        if let Some(name) = self.class_of.get(&style) {
            return name.clone();
        }
        let base_name = style
            .and_then(|id| self.definitions.styles.get(&id))
            .map_or_else(
                || "default".to_owned(),
                |style| {
                    style
                        .name
                        .as_deref()
                        .map_or_else(|| "style".to_owned(), class_slug)
                },
            );
        let mut name = format!("s-{base_name}");
        let mut suffix = 2;
        while self.rules.contains_key(&name) {
            name = format!("s-{base_name}-{suffix}");
            suffix += 1;
        }
        let direct = ParagraphProperties {
            style_ref: style,
            ..ParagraphProperties::default()
        };
        let mut rule = paragraph_css(&self.cascade.resolve_paragraph(&direct), &self.palette);
        rule.extend(&self.base_run(style, None).inherited);
        self.rules.insert(name.clone(), rule);
        self.class_of.insert(style, name.clone());
        name
    }

    /// What a paragraph's runs inherit from it: the paragraph style's (and the
    /// table style's) run formatting with no character formatting of their own.
    fn base_run(&self, style: Option<StyleId>, layer: Option<&TableStyleLayer>) -> css::RunCss {
        let properties = self
            .cascade
            .resolve_run_in_table(style, &RunProperties::default(), layer);
        run_css(&properties, &self.fonts, &self.palette)
    }

    /// The top margin a paragraph owes Word's additive spacing, and records
    /// what the next one is owed.
    fn space(&self, effective: &ParagraphProperties, flow: &mut Flow, last: bool) -> (i64, i64) {
        let style = self.cascade.paragraph_style(effective);
        let contextual = effective.contextual_spacing == Some(true);
        let (before, after) = paragraph_space(effective);
        let same_style = flow.previous.is_some_and(|(previous, _)| previous == style);
        let previous_contextual = flow.previous.is_some_and(|(_, contextual)| contextual);
        let owed = if previous_contextual && same_style {
            0
        } else {
            flow.pending_after
        };
        let own = if contextual && same_style { 0 } else { before };
        flow.pending_after = after;
        flow.previous = Some((style, contextual));
        (owed + own, if last { after } else { 0 })
    }

    /// Writes one paragraph — as a `<p>`, an `<hN>`, or, inside a list, an
    /// `<li>` whose marker and indentation `item` supplies.
    fn paragraph(
        &mut self,
        paragraph: &Paragraph,
        cx: Cx<'_>,
        flow: &mut Flow,
        last: bool,
        item: Option<ListItem>,
    ) -> Result<(), AdapterError> {
        let mut effective = self.resolve(paragraph, cx);
        let style = self.cascade.paragraph_style(&effective);
        let class = self.class(style);
        let heading = self.heading_level(&effective);

        // A numbered paragraph outside a list (a numbered heading) keeps its
        // number as text at its head, so it stays a heading.
        let mut marker = None;
        if item.is_none()
            && !effective.numbering_none
            && let Some(reference) = effective.numbering
            && let Some(resolved) = self.numbering.resolve(self.definitions, &reference)
        {
            if let Some(level) = resolved.level_indent {
                effective.indentation = Some(list_indent(level, effective.indentation));
            }
            let family = self.marker_family(resolved.run_properties.as_ref(), style, cx);
            let (text, _) = marker_glyphs(&resolved.text, family.as_deref());
            if !text.is_empty() {
                marker = Some(match resolved.suffix {
                    LevelSuffix::Nothing => text,
                    LevelSuffix::Space | LevelSuffix::Tab => format!("{text}\u{2002}"),
                });
            }
        }

        let mut declarations = paragraph_css(&effective, &self.palette);
        declarations.extend(&self.base_run(style, cx.layer).inherited);
        if let Some(item) = &item {
            declarations.set("margin-inline-start", twips(item.relative_start));
            declarations.remove("text-indent");
            declarations.set("list-style-type", item.marker.clone());
        }
        let class_rule = self.rules.get(&class).cloned().unwrap_or_default();
        let mut inline = declarations.delta(&class_rule);
        if item.is_some() && class_rule.get("text-indent").is_some() {
            inline.set("text-indent", "0");
        }
        let (top, bottom) = self.space(&effective, flow, last);
        if top > 0 {
            inline.set("margin-top", twips(top));
        }
        if bottom > 0 {
            inline.set("margin-bottom", twips(bottom));
        }
        if std::mem::take(&mut flow.page_break) || effective.page_break_before == Some(true) {
            inline.set("break-before", "page");
        }
        match explicit_break(&paragraph.inlines) {
            Some(BreakPlace::Before(kind)) => {
                inline.set("break-before", kind);
            }
            Some(BreakPlace::After(kind)) => {
                inline.set("break-after", kind);
            }
            Some(BreakPlace::Within(kind)) => {
                inline.set("break-before", kind);
                self.losses
                    .record("html.break_within_paragraph", ModelOutcome::Degraded);
            }
            None => {}
        }
        if has_tab(&paragraph.inlines) && !effective.tabs.is_empty() {
            // A custom tab stop — right-aligned, centred, with a leader — has
            // no CSS equivalent; the tab advances to the default grid instead.
            self.losses.record("html.tab_stop", ModelOutcome::Degraded);
        }

        // A `<p>` or `<hN>` may hold only phrasing content, and a browser
        // closes it early at a nested block — leaving a stray empty paragraph
        // behind. A paragraph carrying a text box or a group is a `<div>`.
        let holds_blocks = holds_blocks(&paragraph.inlines);
        let tag = match (&item, heading) {
            (Some(_), _) => "li".to_owned(),
            (None, _) if holds_blocks => "div".to_owned(),
            (None, Some(level)) => format!("h{level}"),
            (None, None) => "p".to_owned(),
        };
        self.push("<")?;
        self.push(&tag)?;
        self.push(" class=\"")?;
        self.push(&class)?;
        self.push("\"")?;
        if !inline.is_empty() {
            self.push(" style=\"")?;
            self.push(&escape_attribute(&inline.to_css()))?;
            self.push("\"")?;
        }
        if effective.bidi == Some(true) {
            self.push(" dir=\"rtl\"")?;
        }
        self.push(">")?;
        if let Some(marker) = marker {
            self.push("<span class=\"marker\">")?;
            self.push(&escape_text(&marker))?;
            self.push("</span>")?;
        }
        let context = InlineContext {
            style,
            layer: cx.layer,
            base: self.base_run(style, cx.layer).inherited,
        };
        self.inlines(&paragraph.inlines, cx, &context)?;
        if !has_visible_content(&paragraph.inlines) {
            // An empty paragraph is a blank line on the page; an empty `<p>`
            // has no height at all. The break gives it its line.
            self.push("<br>")?;
        }
        if item.is_none() {
            self.push("</")?;
            self.push(&tag)?;
            self.push(">\n")?;
        }
        Ok(())
    }

    /// The family a list marker is drawn in: its level's run formatting over
    /// the paragraph's, as the page resolves it.
    fn marker_family(
        &self,
        level: Option<&RunProperties>,
        style: Option<StyleId>,
        cx: Cx<'_>,
    ) -> Option<String> {
        let level = level.cloned().unwrap_or_default();
        let effective = self.cascade.resolve_run_in_table(style, &level, cx.layer);
        requested_font_family(&effective, self.fonts.scheme)
    }

    /// A run of list paragraphs as nested `<ul>`/`<ol>`, each item's marker
    /// the label the page prints (`1.1.`, `a)`, `•`) and its indentation the
    /// level's, measured from the list that contains it.
    fn list(
        &mut self,
        items: &[BlockNode],
        cx: Cx<'_>,
        flow: &mut Flow,
        last: bool,
    ) -> Result<(), AdapterError> {
        let mut stack: Vec<OpenList> = Vec::new();
        for (index, block) in items.iter().enumerate() {
            let BlockNode::Paragraph(paragraph) = block else {
                continue;
            };
            let effective = self.resolve(paragraph, cx);
            let Some(reference) = effective.numbering else {
                continue;
            };
            let level = reference.level;
            let ordered = self
                .definitions
                .numbering_resolver()
                .level(reference)
                .is_some_and(|level| {
                    !matches!(
                        level.num_fmt.as_ref(),
                        Some(NumberFormat::Bullet | NumberFormat::None) | None
                    )
                });
            let tag = if ordered { "ol" } else { "ul" };
            let resolved = self.numbering.resolve(self.definitions, &reference);
            let indentation = match resolved.as_ref().and_then(|marker| marker.level_indent) {
                Some(level_indent) => list_indent(level_indent, effective.indentation),
                None => effective.indentation.unwrap_or_default(),
            };
            let start = i64::from(indentation.start_twips.unwrap_or(0));
            let style = self.cascade.paragraph_style(&effective);
            let marker = match &resolved {
                Some(resolved) => {
                    let family = self.marker_family(resolved.run_properties.as_ref(), style, cx);
                    let (text, _) = marker_glyphs(&resolved.text, family.as_deref());
                    if text.is_empty() {
                        "none".to_owned()
                    } else {
                        let suffix = match resolved.suffix {
                            LevelSuffix::Nothing => "",
                            LevelSuffix::Space => "\u{a0}",
                            LevelSuffix::Tab => "\u{2002}",
                        };
                        css_string(&format!("{text}{suffix}"))
                    }
                }
                None => "none".to_owned(),
            };

            // Close what this item is not inside: deeper levels, and this
            // level when it changes between numbered and bulleted.
            while let Some(top) = stack.last() {
                if top.level > level || (top.level == level && top.tag != tag) {
                    if top.item.is_some() {
                        self.push("</li>\n")?;
                    }
                    self.push("</")?;
                    self.push(top.tag)?;
                    self.push(">\n")?;
                    stack.pop();
                } else {
                    break;
                }
            }
            match stack.last_mut() {
                Some(top) if top.level == level => {
                    if top.item.take().is_some() {
                        self.push("</li>\n")?;
                    }
                }
                _ => {
                    let origin = stack.last().and_then(|top| top.item).unwrap_or(0);
                    self.push("<")?;
                    self.push(tag)?;
                    self.push(" class=\"list\">\n")?;
                    stack.push(OpenList {
                        level,
                        tag,
                        origin,
                        item: None,
                    });
                }
            }
            let origin = stack.last().map_or(0, |top| top.origin);
            self.paragraph(
                paragraph,
                cx,
                flow,
                last && index + 1 == items.len(),
                Some(ListItem {
                    relative_start: start - origin,
                    marker,
                }),
            )?;
            if let Some(top) = stack.last_mut() {
                top.item = Some(start);
            }
        }
        while let Some(top) = stack.pop() {
            if top.item.is_some() {
                self.push("</li>\n")?;
            }
            self.push("</")?;
            self.push(top.tag)?;
            self.push(">\n")?;
        }
        Ok(())
    }

    // ---- tables ------------------------------------------------------------

    /// A real HTML table: the grid's column widths, merged cells as `colspan`
    /// and `rowspan`, each cell's borders, fill, padding and alignment as the
    /// page resolves them — table style, banding and conflict rules included.
    fn table(&mut self, table: &Table, cx: Cx<'_>, flow: &mut Flow) -> Result<(), AdapterError> {
        if cx.depth >= self.limits.max_nesting_depth {
            return Err(AdapterError::new(
                "limit html_nesting_depth exceeded while walking nested tables",
            ));
        }
        let layers = table_cell_layers(table, &self.cascade);
        let borders = table_cell_borders(table, &layers);
        let columns = grid_columns(table);

        let mut css = Declarations::default();
        let properties = &table.properties;
        if let Some(width) = properties.width.as_ref().and_then(width_css) {
            css.set("width", width);
        }
        if properties.layout == Some(casual_doc_model::v1::TableLayout::Fixed) {
            css.set("table-layout", "fixed");
        }
        if let Some(indent) = properties.indent_twips.filter(|indent| *indent != 0) {
            css.set("margin-inline-start", twips(i64::from(indent)));
        }
        match properties.alignment {
            Some(casual_doc_model::v1::Alignment::Center) => {
                css.set("margin-inline-start", "auto");
                css.set("margin-inline-end", "auto");
            }
            Some(casual_doc_model::v1::Alignment::End) => {
                css.set("margin-inline-start", "auto");
            }
            _ => {}
        }
        if let Some(spacing) = properties.cell_spacing_twips.filter(|spacing| *spacing > 0) {
            css.set("border-collapse", "separate");
            css.set("border-spacing", twips(i64::from(spacing) * 2));
        }
        if flow.pending_after > 0 {
            css.set("margin-top", twips(flow.pending_after));
        }
        if std::mem::take(&mut flow.page_break) {
            css.set("break-before", "page");
        }
        flow.pending_after = 0;
        flow.previous = None;
        if properties.float_position.is_some() {
            // A floating table's page position has no meaning on a web page;
            // it flows where it is anchored.
            self.losses
                .record("html.table_float_position", ModelOutcome::Degraded);
        }

        self.push("<table")?;
        if !css.is_empty() {
            self.push(" style=\"")?;
            self.push(&escape_attribute(&css.to_css()))?;
            self.push("\"")?;
        }
        self.push(">\n")?;
        if table.grid.iter().any(|column| column.width_twips.is_some()) {
            self.push("<colgroup>")?;
            for column in &table.grid {
                match column.width_twips {
                    Some(width) => {
                        self.push("<col style=\"width:")?;
                        self.push(&twips(i64::from(width.max(0))))?;
                        self.push("\">")?;
                    }
                    None => self.push("<col>")?,
                }
            }
            self.push("</colgroup>\n")?;
        }

        let deeper = Cx {
            depth: cx.depth + 1,
            layer: None,
        };
        // Header rows — those Word repeats on every page — are the table's
        // head, and their cells are header cells. A first row that is not one
        // is an ordinary row.
        let header_rows = table
            .rows
            .iter()
            .take_while(|row| row.properties.header)
            .count();
        for (row_index, row) in table.rows.iter().enumerate() {
            if row_index == 0 && header_rows > 0 {
                self.push("<thead>\n")?;
            }
            if row_index == header_rows {
                if header_rows > 0 {
                    self.push("</thead>\n")?;
                }
                self.push("<tbody>\n")?;
            }
            let header = row_index < header_rows;
            self.push("<tr")?;
            if let Some(height) = row
                .properties
                .height
                .value_twips
                .filter(|height| *height > 0)
                && row.properties.height.rule != Some(HeightRule::Auto)
            {
                self.push(" style=\"height:")?;
                self.push(&twips(i64::from(height)))?;
                self.push("\"")?;
            }
            self.push(">\n")?;
            for (cell_index, cell) in row.cells.iter().enumerate() {
                if cell.properties.vertical_merge == Some(VerticalMerge::Continue) {
                    continue;
                }
                let layer = &layers[row_index][cell_index];
                let tag = if header { "th" } else { "td" };
                self.push("<")?;
                self.push(tag)?;
                if let Some(span) = cell.properties.grid_span.filter(|span| *span > 1) {
                    self.push(&format!(" colspan=\"{span}\""))?;
                }
                let rows_spanned = vertical_span(table, &columns, row_index, cell_index);
                if rows_spanned > 1 {
                    self.push(&format!(" rowspan=\"{rows_spanned}\""))?;
                }
                let mut cell_css = Declarations::default();
                let candidates = &borders[row_index][cell_index];
                for (property, edge) in [
                    ("border-top", candidates.top.as_ref()),
                    ("border-bottom", candidates.bottom.as_ref()),
                    ("border-inline-start", candidates.start.as_ref()),
                    ("border-inline-end", candidates.end.as_ref()),
                ] {
                    if let Some(value) = edge.and_then(|edge| self.cell_edge(edge)) {
                        cell_css.set(property, value);
                    }
                }
                if let Some(fill) = self.palette.cell_fill(table, cell, layer) {
                    cell_css.set("background-color", hex(fill));
                }
                let margins = cell_margins(&cell.properties, &table.properties);
                for (property, value) in [
                    ("padding-top", margins.top.raw()),
                    ("padding-bottom", margins.bottom.raw()),
                    ("padding-inline-start", margins.start.raw()),
                    ("padding-inline-end", margins.end.raw()),
                ] {
                    if value > 0 {
                        cell_css.set(property, twips(i64::from(value)));
                    }
                }
                match cell.properties.vertical_alignment {
                    Some(CellVerticalAlignment::Center) => cell_css.set("vertical-align", "middle"),
                    Some(CellVerticalAlignment::Bottom) => cell_css.set("vertical-align", "bottom"),
                    _ => {}
                }
                if let Some(width) = cell.properties.width.as_ref().and_then(width_css) {
                    cell_css.set("width", width);
                }
                match cell.properties.text_direction {
                    Some(TextDirection::TbRl) => cell_css.set("writing-mode", "vertical-rl"),
                    Some(TextDirection::BtLr) => {
                        cell_css.set("writing-mode", "vertical-rl");
                        cell_css.set("transform", "rotate(180deg)");
                    }
                    _ => {}
                }
                if !cell_css.is_empty() {
                    self.push(" style=\"")?;
                    self.push(&escape_attribute(&cell_css.to_css()))?;
                    self.push("\"")?;
                }
                self.push(">\n")?;
                let mut cell_flow = Flow::default();
                let in_cell = Cx {
                    layer: Some(layer),
                    ..deeper
                };
                self.blocks(&cell.blocks, in_cell, &mut cell_flow)?;
                self.push("</")?;
                self.push(tag)?;
                self.push(">\n")?;
            }
            self.push("</tr>\n")?;
        }
        if header_rows == table.rows.len() && header_rows > 0 {
            self.push("</thead>\n")?;
        } else if !table.rows.is_empty() {
            self.push("</tbody>\n")?;
        }
        self.push("</table>\n")?;
        Ok(())
    }

    /// One cell side as CSS: the winning edge as the page draws it, `hidden`
    /// for an explicit `nil` (which suppresses the shared edge on both sides,
    /// as it does on the page), nothing for no border.
    fn cell_edge(&self, edge: &BorderEdge) -> Option<String> {
        if edge.style == "nil" {
            return Some("hidden".to_owned());
        }
        css::border_css(&self.palette, edge)
    }

    // ---- inlines -----------------------------------------------------------

    fn inlines(
        &mut self,
        inlines: &[InlineNode],
        cx: Cx<'_>,
        context: &InlineContext<'_>,
    ) -> Result<(), AdapterError> {
        for inline in inlines {
            self.inline(inline, cx, context)?;
        }
        Ok(())
    }

    fn inline(
        &mut self,
        inline: &InlineNode,
        cx: Cx<'_>,
        context: &InlineContext<'_>,
    ) -> Result<(), AdapterError> {
        match inline {
            InlineNode::Run(run) => self.run(&run.properties, &run.text, context),
            InlineNode::Tab(_) | InlineNode::PositionalTab(_) => {
                // A real tab, advancing to the next stop on the document's
                // default grid (`tab-size` on the body).
                self.push("<span class=\"tab\">\t</span>")
            }
            InlineNode::Break(node) => match node.kind {
                BreakKind::Line => self.push("<br>"),
                // A page or column break becomes the paragraph's own
                // `break-before`/`break-after` (see `explicit_break`).
                _ => Ok(()),
            },
            InlineNode::Hyperlink(link) => {
                let target = link_target(&link.target);
                self.push("<a href=\"")?;
                self.push(&escape_attribute(&target))?;
                self.push("\"")?;
                if let Some(tooltip) = &link.tooltip {
                    self.push(" title=\"")?;
                    self.push(&escape_attribute(tooltip))?;
                    self.push("\"")?;
                }
                self.push(">")?;
                self.inlines(&link.inlines, cx, context)?;
                self.push("</a>")
            }
            InlineNode::Drawing(drawing) => {
                let link = drawing
                    .hyperlink
                    .as_ref()
                    .map(|link| link_target(&link.target));
                self.picture(
                    &Picture {
                        media: drawing.media,
                        description: drawing.descr.as_deref(),
                        extent: drawing.extent,
                        crop: drawing.crop,
                        rotation: drawing.rotation,
                        flip_h: drawing.flip_h,
                        flip_v: drawing.flip_v,
                        opacity: drawing.opacity,
                        anchor: None,
                        link,
                    },
                    cx,
                )
            }
            InlineNode::AnchoredDrawing(drawing) => {
                // A page position has no meaning on a web page; the picture
                // floats to the side it is anchored to, or sits in the text.
                self.losses
                    .record("html.anchor_position", ModelOutcome::Degraded);
                let link = drawing
                    .hyperlink
                    .as_ref()
                    .map(|link| link_target(&link.target));
                self.picture(
                    &Picture {
                        media: drawing.media,
                        description: drawing.descr.as_deref(),
                        extent: Some(drawing.extent),
                        crop: drawing.crop,
                        rotation: drawing.rotation,
                        flip_h: drawing.flip_h,
                        flip_v: drawing.flip_v,
                        opacity: drawing.opacity,
                        anchor: Some(&drawing.anchor),
                        link,
                    },
                    cx,
                )
            }
            InlineNode::HorizontalRule(_) => self.push("<hr>"),
            InlineNode::Field(field) => {
                // The field's last computed result is what a reader sees; its
                // instruction does not survive.
                self.losses
                    .record("html.field_instruction", ModelOutcome::Degraded);
                self.inlines(&field.inlines, cx, context)
            }
            InlineNode::TextBox(text_box) => {
                // The box keeps its size, fill, outline and inner margins; what
                // a web page cannot keep is a page position (it floats to its
                // side instead) and Word's shrink-text-on-overflow.
                self.losses.record("html.text_box", ModelOutcome::Degraded);
                let mut css = Declarations::default();
                if let Some(extent) = text_box
                    .extent
                    .filter(|extent| extent.width_emu > 0 && extent.height_emu > 0)
                {
                    css.set("width", emu(extent.width_emu));
                    css.set("max-width", "100%");
                    css.set("min-height", emu(extent.height_emu));
                }
                if let Some(fill) = &text_box.fill {
                    css.set("background-color", rgba_css(fill.flat_color()));
                }
                if let Some(border) = text_box
                    .border
                    .as_ref()
                    .filter(|border| border.width_emu > 0)
                {
                    css.set("border", stroke_css(border));
                }
                let insets = text_box.body_properties.insets;
                css.set(
                    "padding",
                    format!(
                        "{} {} {} {}",
                        emu(i64::from(insets.top_emu)),
                        emu(i64::from(insets.right_emu)),
                        emu(i64::from(insets.bottom_emu)),
                        emu(i64::from(insets.left_emu))
                    ),
                );
                match text_box.body_properties.vertical_anchor {
                    TextBoxVerticalAnchor::Center => {
                        css.set("display", "flex");
                        css.set("flex-direction", "column");
                        css.set("justify-content", "center");
                    }
                    TextBoxVerticalAnchor::Bottom => {
                        css.set("display", "flex");
                        css.set("flex-direction", "column");
                        css.set("justify-content", "flex-end");
                    }
                    TextBoxVerticalAnchor::Top => {}
                }
                match &text_box.anchor {
                    Some(anchor) => anchor_css(anchor, &mut css),
                    None => {
                        if css.get("display").is_none() {
                            css.set("display", "inline-block");
                        }
                        css.set("vertical-align", "top");
                    }
                }
                self.nested_blocks_in(&text_box.blocks, cx, &css)
            }
            InlineNode::Group(group) => self.group(group, cx),
            InlineNode::Revision(revision) => {
                // `<ins>` and `<del>` are the HTML for exactly this, so a
                // tracked change survives as a tracked change.
                let tag = match revision.kind {
                    RevisionKind::Insertion | RevisionKind::MoveTo => "ins",
                    RevisionKind::Deletion | RevisionKind::MoveFrom => "del",
                };
                self.push("<")?;
                self.push(tag)?;
                self.push(">")?;
                self.inlines(&revision.inlines, cx, context)?;
                self.push("</")?;
                self.push(tag)?;
                self.push(">")
            }
            InlineNode::Sdt(sdt) => {
                self.losses
                    .record("html.content_control", ModelOutcome::Degraded);
                self.inlines(&sdt.inlines, cx, context)
            }
            InlineNode::Math(math) => {
                // Not MathML: the model carries OMML, and translating it is its
                // own piece of work (`docs/134`). The fallback text is what a
                // reader sees, and the loss says the markup went.
                self.losses
                    .record("html.math_markup", ModelOutcome::Degraded);
                self.push(&escape_text(&math.text))
            }
            InlineNode::Symbol(symbol) => {
                // A Symbol or Wingdings code point is a glyph in that font, not
                // a character; the map gives the Unicode the page draws.
                let character =
                    casual_doc_layout::symbol_map::map_symbol(&symbol.font, symbol.char)
                        .or_else(|| char::from_u32(symbol.char).filter(|c| !is_private_use(*c)));
                let Some(character) = character else {
                    self.losses
                        .record("html.symbol_font", ModelOutcome::Degraded);
                    return Ok(());
                };
                self.run(&symbol.properties, &character.to_string(), context)
            }
            InlineNode::NoBreakHyphen(_) => self.push("&#8209;"),
            InlineNode::SoftHyphen(_) => self.push("&shy;"),
            InlineNode::EmbeddedObject(object) => match object.preview {
                // A chart, SmartArt diagram or OLE object carries the picture
                // Word drew of it. That picture, at the object's size, is what
                // a reader without the application sees — in Word as well.
                Some(preview) => {
                    self.losses
                        .record("html.embedded_object_as_picture", ModelOutcome::Degraded);
                    let description = match &object.kind {
                        EmbeddedKind::Chart => "Chart",
                        EmbeddedKind::Diagram => "Diagram",
                        EmbeddedKind::OleObject | EmbeddedKind::Other(_) => "Embedded object",
                    };
                    self.picture(
                        &Picture {
                            media: preview,
                            description: Some(description),
                            extent: Some(object.extent),
                            crop: None,
                            rotation: None,
                            flip_h: false,
                            flip_v: false,
                            opacity: None,
                            anchor: None,
                            link: None,
                        },
                        cx,
                    )
                }
                None => {
                    self.losses
                        .record("html.embedded_object", ModelOutcome::Omitted);
                    Ok(())
                }
            },
            InlineNode::NoteReference(reference) => {
                self.note_reference(reference.kind, reference.note)
            }
            InlineNode::NoteNumberMark(_) => self.note_number_mark(),
            InlineNode::BookmarkStart(marker) => {
                // A bookmark is an anchor, and an internal link's `#name` needs
                // one to land on. The NAME lives in `definitions.bookmarks`.
                let Some(bookmark) = self.definitions.bookmarks.get(&marker.bookmark) else {
                    self.losses
                        .record("html.dangling_bookmark", ModelOutcome::Omitted);
                    return Ok(());
                };
                let name = escape_attribute(&bookmark.name);
                self.push("<a id=\"")?;
                self.push(&name)?;
                self.push("\"></a>")
            }
            InlineNode::BookmarkEnd(_) => Ok(()),
            InlineNode::CommentReference(_)
            | InlineNode::CommentRangeStart(_)
            | InlineNode::CommentRangeEnd(_)
            | InlineNode::FieldRangeStart(_)
            | InlineNode::FieldRangeEnd(_)
            | InlineNode::MoveRangeStart(_)
            | InlineNode::MoveRangeEnd(_) => {
                self.losses
                    .record("html.range_or_comment_marker", ModelOutcome::Omitted);
                Ok(())
            }
        }
    }

    /// One run of text: semantic `<strong>`/`<em>` where it is bolder or more
    /// italic than its paragraph, `<sup>`/`<sub>` for its baseline, and a
    /// `<span>` carrying only where its resolved formatting differs from what
    /// it inherits.
    fn run(
        &mut self,
        direct: &RunProperties,
        text: &str,
        context: &InlineContext<'_>,
    ) -> Result<(), AdapterError> {
        if text.is_empty() {
            return Ok(());
        }
        let effective = self
            .cascade
            .resolve_run_in_table(context.style, direct, context.layer);
        let css = run_css(&effective, &self.fonts, &self.palette);
        let mut inherited = context.base.clone();
        let mut tags: Vec<&'static str> = Vec::new();
        if css.inherited.get("font-weight") == Some("700")
            && inherited.get("font-weight") != Some("700")
        {
            tags.push("strong");
            inherited.set("font-weight", "700");
        }
        if css.inherited.get("font-style") == Some("italic")
            && inherited.get("font-style") != Some("italic")
        {
            tags.push("em");
            inherited.set("font-style", "italic");
        }
        match effective.vertical_alignment {
            Some(VerticalAlignment::Superscript) => tags.push("sup"),
            Some(VerticalAlignment::Subscript) => tags.push("sub"),
            _ => {}
        }
        let mut span = css.inherited.delta(&inherited);
        span.extend(&css.own);
        // The page draws super- and subscript at two thirds of the size,
        // raised by a third or lowered by a sixth of it (`flow::run_metrics`);
        // `<sup>` and `<sub>` are neutral in the stylesheet so these numbers
        // are the only ones that apply.
        let size = f64::from(
            effective
                .size_half_points
                .unwrap_or(DEFAULT_SIZE_HALF_POINTS),
        ) / 2.0;
        let position = f64::from(effective.position_half_points.unwrap_or(0)) / 2.0;
        match effective.vertical_alignment {
            Some(VerticalAlignment::Superscript) => {
                span.set("font-size", points(size * 2.0 / 3.0));
                span.set("vertical-align", points(size / 3.0 + position));
            }
            Some(VerticalAlignment::Subscript) => {
                span.set("font-size", points(size * 2.0 / 3.0));
                span.set("vertical-align", points(-size / 6.0 + position));
            }
            _ => {}
        }
        let language = effective
            .language
            .as_ref()
            .and_then(|language| language.value.as_deref())
            .and_then(language_tag)
            .filter(|tag| Some(tag) != self.document_language.as_ref());
        let hidden = effective.hidden == Some(true);
        let rtl = effective.rtl == Some(true);

        for tag in &tags {
            self.push("<")?;
            self.push(tag)?;
            self.push(">")?;
        }
        let wrapped = !span.is_empty() || language.is_some() || hidden || rtl;
        if wrapped {
            self.push("<span")?;
            if !span.is_empty() {
                self.push(" style=\"")?;
                self.push(&escape_attribute(&span.to_css()))?;
                self.push("\"")?;
            }
            if let Some(language) = &language {
                self.push(" lang=\"")?;
                self.push(&escape_attribute(language))?;
                self.push("\"")?;
            }
            if rtl {
                self.push(" dir=\"rtl\"")?;
            }
            if hidden {
                // Hidden text is not on the page; it is still in the document,
                // so it is in the file, hidden.
                self.push(" hidden")?;
            }
            self.push(">")?;
        }
        self.push(&escape_text(text))?;
        if wrapped {
            self.push("</span>")?;
        }
        for tag in tags.iter().rev() {
            self.push("</")?;
            self.push(tag)?;
            self.push(">")?;
        }
        Ok(())
    }

    /// Blocks inside an inline container (a text box): a nested flow.
    fn nested_blocks(&mut self, blocks: &[BlockNode], cx: Cx<'_>) -> Result<(), AdapterError> {
        self.nested_blocks_in(blocks, cx, &Declarations::default())
    }

    /// [`Self::nested_blocks`] in a box with its own declarations.
    fn nested_blocks_in(
        &mut self,
        blocks: &[BlockNode],
        cx: Cx<'_>,
        css: &Declarations,
    ) -> Result<(), AdapterError> {
        if cx.depth >= self.limits.max_nesting_depth {
            return Err(AdapterError::new(
                "limit html_nesting_depth exceeded while walking a text box",
            ));
        }
        let deeper = Cx {
            depth: cx.depth + 1,
            layer: None,
        };
        if css.is_empty() {
            self.push("<div>\n")?;
        } else {
            self.push("<div style=\"")?;
            self.push(&escape_attribute(&css.to_css()))?;
            self.push("\">\n")?;
        }
        let mut flow = Flow::default();
        self.blocks(blocks, deeper, &mut flow)?;
        self.push("</div>\n")
    }

    fn group(&mut self, group: &WordprocessingGroup, cx: Cx<'_>) -> Result<(), AdapterError> {
        if cx.depth >= self.limits.max_nesting_depth {
            return Err(AdapterError::new(
                "limit html_nesting_depth exceeded while walking a grouped drawing",
            ));
        }
        self.losses
            .record("html.grouped_drawing", ModelOutcome::Degraded);
        let deeper = Cx {
            depth: cx.depth + 1,
            layer: None,
        };
        for child in &group.children {
            match child {
                GroupChild::Picture(picture) => {
                    self.picture(
                        &Picture {
                            media: picture.media,
                            description: picture.descr.as_deref(),
                            extent: None,
                            crop: None,
                            rotation: None,
                            flip_h: false,
                            flip_v: false,
                            opacity: None,
                            anchor: None,
                            link: None,
                        },
                        deeper,
                    )?;
                }
                GroupChild::TextBox(text_box) => self.nested_blocks(&text_box.blocks, deeper)?,
                GroupChild::Shape(_) => {
                    self.losses
                        .record("html.group_shape", ModelOutcome::Omitted);
                }
                GroupChild::Group(inner) => self.group(inner, deeper)?,
            }
        }
        Ok(())
    }

    /// One picture, embedded as a `data:` URI when its bytes are present and
    /// within the per-picture ceiling, at the size the document gives it.
    fn picture(&mut self, picture: &Picture<'_>, _cx: Cx<'_>) -> Result<(), AdapterError> {
        let alt = picture.description.unwrap_or("");
        let reference = self.definitions.media.get(&picture.media);
        let bytes = reference.and_then(|entry| self.resources.get(&entry.part_name));
        let Some((entry, bytes)) = reference.zip(bytes) else {
            // A drawing whose bytes the host did not supply cannot be embedded,
            // and an `<img>` with no source is a broken-image icon. The alt text
            // is what is left.
            self.losses
                .record("html.picture_bytes_absent", ModelOutcome::Degraded);
            return self.alt_text(alt);
        };
        if bytes.len() > self.limits.max_embedded_bytes {
            self.losses
                .record("html.picture_over_embedding_limit", ModelOutcome::Degraded);
            return self.alt_text(alt);
        }

        let mut frame = Declarations::default();
        let mut image = Declarations::default();
        let size = picture
            .extent
            .filter(|extent| extent.width_emu > 0 && extent.height_emu > 0);
        if let Some(extent) = size {
            frame.set("width", emu(extent.width_emu));
            frame.set(
                "aspect-ratio",
                format!("{} / {}", extent.width_emu, extent.height_emu),
            );
        }
        let mut transforms = Vec::new();
        if let Some(rotation) = picture.rotation.filter(|rotation| *rotation != 0) {
            // `a:xfrm@rot` is in 60,000ths of a degree.
            transforms.push(format!("rotate({}deg)", f64::from(rotation) / 60_000.0));
        }
        if picture.flip_h {
            transforms.push("scaleX(-1)".to_owned());
        }
        if picture.flip_v {
            transforms.push("scaleY(-1)".to_owned());
        }
        if !transforms.is_empty() {
            frame.set("transform", transforms.join(" "));
        }
        if let Some(opacity) = picture.opacity.filter(|opacity| *opacity < 100_000) {
            frame.set("opacity", format!("{}", f64::from(opacity) / 100_000.0));
        }
        if let Some(anchor) = picture.anchor {
            anchor_css(anchor, &mut frame);
        }

        // A crop shows part of the source scaled to fill the box: the image is
        // drawn larger than the frame and offset, and the frame clips it.
        let crop = picture.crop.filter(|crop| !crop.is_identity());
        let cropped = crop.is_some() && size.is_some();
        if let Some(crop) = crop.filter(|_| cropped) {
            let full = f64::from(casual_doc_model::v1::CROP_FULL);
            let visible_w = (full - f64::from(crop.left) - f64::from(crop.right)).max(1.0);
            let visible_h = (full - f64::from(crop.top) - f64::from(crop.bottom)).max(1.0);
            image.set("display", "block");
            image.set("max-width", "none");
            image.set("width", format!("{}%", round3(full / visible_w * 100.0)));
            image.set("height", format!("{}%", round3(full / visible_h * 100.0)));
            image.set(
                "margin-left",
                format!("{}%", round3(-f64::from(crop.left) / visible_w * 100.0)),
            );
            image.set(
                "margin-top",
                format!("{}%", round3(-f64::from(crop.top) / visible_h * 100.0)),
            );
            frame.set("display", "inline-block");
            frame.set("overflow", "hidden");
            frame.set("max-width", "100%");
            frame.set("vertical-align", "baseline");
        } else if let Some(extent) = size {
            image.extend(&frame);
            image.set("width", emu(extent.width_emu));
            frame = Declarations::default();
        } else {
            image.extend(&frame);
            frame = Declarations::default();
        }

        if let Some(link) = &picture.link {
            self.push("<a href=\"")?;
            self.push(&escape_attribute(link))?;
            self.push("\">")?;
        }
        if cropped {
            self.push("<span style=\"")?;
            self.push(&escape_attribute(&frame.to_css()))?;
            self.push("\">")?;
        }
        self.push("<img alt=\"")?;
        self.push(&escape_attribute(alt))?;
        self.push("\"")?;
        if !image.is_empty() {
            self.push(" style=\"")?;
            self.push(&escape_attribute(&image.to_css()))?;
            self.push("\"")?;
        }
        self.push(" src=\"data:")?;
        self.push(&escape_attribute(&entry.media_type))?;
        self.push(";base64,")?;
        let encoded = base64(bytes);
        self.push(&encoded)?;
        self.push("\">")?;
        if cropped {
            self.push("</span>")?;
        }
        if picture.link.is_some() {
            self.push("</a>")?;
        }
        Ok(())
    }

    fn alt_text(&mut self, alt: &str) -> Result<(), AdapterError> {
        if alt.trim().is_empty() {
            return Ok(());
        }
        self.push("<span>")?;
        self.push(&escape_text(alt))?;
        self.push("</span>")
    }

    // ---- notes -------------------------------------------------------------

    /// A footnote or endnote reference: the label the page prints, linked to
    /// the note at the end of the file, and linked back from it.
    fn note_reference(&mut self, kind: NoteKind, note: NoteId) -> Result<(), AdapterError> {
        let exists = match kind {
            NoteKind::Footnote => self.definitions.footnotes.get(&note).is_some(),
            NoteKind::Endnote => self.definitions.endnotes.get(&note).is_some(),
        };
        if !exists {
            self.losses
                .record("html.dangling_note", ModelOutcome::Omitted);
            return Ok(());
        }
        let label = self
            .note_labels
            .label(kind, note)
            .map_or_else(|| "*".to_owned(), str::to_owned);
        let (index, first) = match self.notes.iter().position(|seen| *seen == (kind, note)) {
            Some(index) => (index, false),
            None => {
                self.notes.push((kind, note));
                (self.notes.len() - 1, true)
            }
        };
        let id = note_anchor(kind, index);
        self.push("<sup class=\"note-ref\"><a href=\"#")?;
        self.push(&id)?;
        self.push("\"")?;
        if first {
            self.push(" id=\"ref-")?;
            self.push(&id)?;
            self.push("\"")?;
        }
        self.push(">")?;
        self.push(&escape_text(&label))?;
        self.push("</a></sup>")
    }

    /// The number at the head of a note's own text, linking back to where the
    /// note is referenced.
    fn note_number_mark(&mut self) -> Result<(), AdapterError> {
        let Some((index, kind)) = self.current_note else {
            return Ok(());
        };
        let (note_kind, note) = self.notes[index];
        let label = self
            .note_labels
            .label(note_kind, note)
            .map_or_else(|| "*".to_owned(), str::to_owned);
        let id = note_anchor(kind, index);
        self.push("<sup class=\"note-ref\"><a class=\"note-back\" href=\"#ref-")?;
        self.push(&id)?;
        self.push("\">")?;
        self.push(&escape_text(&label))?;
        self.push("</a></sup>")
    }

    /// Every referenced note, after the body: footnotes, then endnotes.
    fn write_notes(&mut self) -> Result<(), AdapterError> {
        // A note may reference another note; the queue can grow while this
        // walks it, so it is walked by index.
        for kind in [NoteKind::Footnote, NoteKind::Endnote] {
            let mut opened = false;
            let mut index = 0;
            while index < self.notes.len() {
                let (note_kind, note) = self.notes[index];
                if note_kind != kind {
                    index += 1;
                    continue;
                }
                let blocks = match kind {
                    NoteKind::Footnote => self.definitions.footnotes.get(&note),
                    NoteKind::Endnote => self.definitions.endnotes.get(&note),
                }
                .map(|note| note.blocks.clone())
                .unwrap_or_default();
                if !opened {
                    self.push(match kind {
                        NoteKind::Footnote => "<section class=\"notes footnotes\">\n<hr>\n",
                        NoteKind::Endnote => "<section class=\"notes endnotes\">\n<hr>\n",
                    })?;
                    opened = true;
                }
                self.push("<div class=\"note\" id=\"")?;
                self.push(&note_anchor(kind, index))?;
                self.push("\">\n")?;
                self.current_note = Some((index, kind));
                let mut flow = Flow::default();
                let cx = Cx {
                    depth: 1,
                    layer: None,
                };
                self.blocks(&blocks, cx, &mut flow)?;
                self.current_note = None;
                self.push("</div>\n")?;
                index += 1;
            }
            if opened {
                self.push("</section>\n")?;
            }
        }
        Ok(())
    }
}

/// What a paragraph's inlines need to know about it.
struct InlineContext<'t> {
    style: Option<StyleId>,
    layer: Option<&'t TableStyleLayer>,
    /// The inherited declarations the paragraph element already carries.
    base: Declarations,
}

/// A list item's marker and its indentation relative to its list.
struct ListItem {
    relative_start: i64,
    marker: String,
}

/// A picture, wherever in the model it came from.
struct Picture<'p> {
    media: MediaId,
    description: Option<&'p str>,
    extent: Option<Extent>,
    crop: Option<casual_doc_model::v1::CropRect>,
    rotation: Option<i32>,
    flip_h: bool,
    flip_v: bool,
    opacity: Option<u32>,
    anchor: Option<&'p DrawingAnchor>,
    link: Option<String>,
}

/// How an anchored picture sits on a web page: beside the text on the side it
/// is anchored to when text wraps around it, on its own line when text sits
/// above and below it.
fn anchor_css(anchor: &DrawingAnchor, css: &mut Declarations) {
    let side = match anchor.horizontal.position {
        HorizontalPosition::Align(HorizontalAlign::Right | HorizontalAlign::Outside) => {
            Some("right")
        }
        HorizontalPosition::Align(HorizontalAlign::Center) => None,
        HorizontalPosition::Align(HorizontalAlign::Left | HorizontalAlign::Inside) => Some("left"),
        HorizontalPosition::Offset(_) => Some("left"),
    };
    let distances = anchor.wrap_distances;
    match anchor.wrap {
        WrapMode::Square | WrapMode::Tight | WrapMode::Through => {
            let side = side.unwrap_or("left");
            css.set("float", side);
            css.set("margin-top", emu(distances.top_emu));
            css.set("margin-bottom", emu(distances.bottom_emu));
            if side == "left" {
                css.set("margin-right", emu(distances.end_emu));
            } else {
                css.set("margin-left", emu(distances.start_emu));
            }
        }
        WrapMode::TopAndBottom | WrapMode::None => {
            css.set("display", "block");
            match side {
                None => {
                    css.set("margin-left", "auto");
                    css.set("margin-right", "auto");
                }
                Some("right") => css.set("margin-left", "auto"),
                Some(_) => {}
            }
        }
    }
}

/// A drawing colour, with its alpha when it has one.
fn rgba_css(color: casual_doc_model::v1::Rgba) -> String {
    if color.a == 255 {
        hex([color.r, color.g, color.b, 255])
    } else {
        format!(
            "rgba({},{},{},{})",
            color.r,
            color.g,
            color.b,
            round3(f64::from(color.a) / 255.0)
        )
    }
}

/// A shape outline as a CSS border: its width, a dash family, its colour.
fn stroke_css(stroke: &casual_doc_model::v1::ShapeStroke) -> String {
    use casual_doc_model::v1::DashStyle;
    let style = match stroke.dash {
        Some(DashStyle::Dot) => "dotted",
        Some(DashStyle::Solid) | None => "solid",
        Some(_) => "dashed",
    };
    format!(
        "{} {style} {}",
        emu(stroke.width_emu.max(1)),
        rgba_css(stroke.color)
    )
}

/// EMUs (914,400 to the inch, 12,700 to the point) as points.
fn emu(value: i64) -> String {
    points(value as f64 / 12_700.0)
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// A table or cell width as CSS, or `None` for automatic.
fn width_css(width: &TableWidth) -> Option<String> {
    match width.width_type {
        WidthType::Dxa if width.value > 0 => Some(twips(i64::from(width.value))),
        // Fiftieths of a percent: 5000 is the full width.
        WidthType::Pct if width.value > 0 => {
            Some(format!("{}%", round3(f64::from(width.value) / 50.0)))
        }
        _ => None,
    }
}

/// The grid column each cell starts in, `[row][cell]`: `w:gridBefore` plus
/// the spans before it. A vertical merge continues in the same *grid column*,
/// which is not the same cell index once a row has a span.
fn grid_columns(table: &Table) -> Vec<Vec<u32>> {
    table
        .rows
        .iter()
        .map(|row| {
            let mut column = row.properties.grid_before.unwrap_or(0);
            row.cells
                .iter()
                .map(|cell| {
                    let start = column;
                    column = column.saturating_add(cell.properties.grid_span.unwrap_or(1).max(1));
                    start
                })
                .collect()
        })
        .collect()
}

/// How many rows a cell's vertical merge spans, counting from `row_index`.
///
/// Complexity: O(rows below × cells per row) for a merged cell and O(1) for
/// the common case.
fn vertical_span(table: &Table, columns: &[Vec<u32>], row_index: usize, cell_index: usize) -> u32 {
    let start = table
        .rows
        .get(row_index)
        .and_then(|row| row.cells.get(cell_index));
    if start.map(|cell| cell.properties.vertical_merge) != Some(Some(VerticalMerge::Restart)) {
        return 1;
    }
    let column = columns[row_index][cell_index];
    let mut span = 1;
    for (offset, row) in table.rows.iter().enumerate().skip(row_index + 1) {
        let continues = columns[offset]
            .iter()
            .position(|start| *start == column)
            .and_then(|index| row.cells.get(index))
            .is_some_and(|cell| cell.properties.vertical_merge == Some(VerticalMerge::Continue));
        if !continues {
            break;
        }
        span += 1;
    }
    span
}

/// Where a paragraph's page or column break sits, which decides whether it
/// becomes `break-before` or `break-after` on the paragraph.
enum BreakPlace {
    Before(&'static str),
    After(&'static str),
    Within(&'static str),
}

fn explicit_break(inlines: &[InlineNode]) -> Option<BreakPlace> {
    let is_content = |inline: &InlineNode| match inline {
        InlineNode::Run(run) => !run.text.is_empty(),
        InlineNode::BookmarkStart(_)
        | InlineNode::BookmarkEnd(_)
        | InlineNode::CommentRangeStart(_)
        | InlineNode::CommentRangeEnd(_)
        | InlineNode::CommentReference(_)
        | InlineNode::FieldRangeStart(_)
        | InlineNode::FieldRangeEnd(_)
        | InlineNode::MoveRangeStart(_)
        | InlineNode::MoveRangeEnd(_) => false,
        InlineNode::Break(node) => node.kind == BreakKind::Line,
        _ => true,
    };
    let (index, kind) = inlines
        .iter()
        .enumerate()
        .find_map(|(index, inline)| match inline {
            InlineNode::Break(node) => match node.kind {
                BreakKind::Page => Some((index, "page")),
                BreakKind::Column => Some((index, "column")),
                _ => None,
            },
            _ => None,
        })?;
    let before = inlines[..index].iter().any(is_content);
    let after = inlines[index + 1..].iter().any(is_content);
    Some(match (before, after) {
        (false, _) => BreakPlace::Before(kind),
        (true, false) => BreakPlace::After(kind),
        (true, true) => BreakPlace::Within(kind),
    })
}

/// Whether a paragraph shows anything: text, a tab, a picture, a box. A
/// paragraph of bookmarks, comment anchors and range markers shows nothing.
fn has_visible_content(inlines: &[InlineNode]) -> bool {
    inlines.iter().any(|inline| match inline {
        InlineNode::Run(run) => !run.text.is_empty(),
        InlineNode::Hyperlink(link) => has_visible_content(&link.inlines),
        InlineNode::Field(field) => has_visible_content(&field.inlines),
        InlineNode::Sdt(sdt) => has_visible_content(&sdt.inlines),
        InlineNode::Revision(revision) => has_visible_content(&revision.inlines),
        InlineNode::Break(node) => node.kind == BreakKind::Line,
        InlineNode::BookmarkStart(_)
        | InlineNode::BookmarkEnd(_)
        | InlineNode::CommentReference(_)
        | InlineNode::CommentRangeStart(_)
        | InlineNode::CommentRangeEnd(_)
        | InlineNode::FieldRangeStart(_)
        | InlineNode::FieldRangeEnd(_)
        | InlineNode::MoveRangeStart(_)
        | InlineNode::MoveRangeEnd(_)
        | InlineNode::EmbeddedObject(_) => false,
        _ => true,
    })
}

/// Whether a paragraph's inlines include a container of blocks (a text box,
/// a group), which phrasing content cannot hold.
fn holds_blocks(inlines: &[InlineNode]) -> bool {
    inlines.iter().any(|inline| match inline {
        InlineNode::TextBox(_) | InlineNode::Group(_) => true,
        InlineNode::Hyperlink(link) => holds_blocks(&link.inlines),
        InlineNode::Field(field) => holds_blocks(&field.inlines),
        InlineNode::Sdt(sdt) => holds_blocks(&sdt.inlines),
        InlineNode::Revision(revision) => holds_blocks(&revision.inlines),
        _ => false,
    })
}

fn has_tab(inlines: &[InlineNode]) -> bool {
    inlines.iter().any(|inline| match inline {
        InlineNode::Tab(_) => true,
        InlineNode::Hyperlink(link) => has_tab(&link.inlines),
        InlineNode::Field(field) => has_tab(&field.inlines),
        InlineNode::Sdt(sdt) => has_tab(&sdt.inlines),
        InlineNode::Revision(revision) => has_tab(&revision.inlines),
        _ => false,
    })
}

fn link_target(target: &HyperlinkTarget) -> String {
    match target {
        HyperlinkTarget::External(external) => match &external.anchor {
            Some(anchor) => format!("{}#{anchor}", external.url),
            None => external.url.clone(),
        },
        HyperlinkTarget::Internal(internal) => format!("#{}", internal.anchor),
    }
}

fn note_anchor(kind: NoteKind, index: usize) -> String {
    match kind {
        NoteKind::Footnote => format!("fn{}", index + 1),
        NoteKind::Endnote => format!("en{}", index + 1),
    }
}

fn is_private_use(c: char) -> bool {
    ('\u{E000}'..='\u{F8FF}').contains(&c)
}

/// A language tag reduced to the characters a BCP 47 tag is made of. A value
/// that keeps nothing is no tag.
fn language_tag(value: &str) -> Option<String> {
    let kept: String = value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    (!kept.is_empty() && kept.len() <= 35).then_some(kept)
}

/// A style name as a class name: lower-case letters, digits and hyphens.
fn class_slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "style".to_owned()
    } else {
        out
    }
}

fn heading_level_from_name(name: &str) -> Option<u8> {
    let trimmed = name.trim();
    let rest = trimmed
        .strip_prefix("heading")
        .or_else(|| trimmed.strip_prefix("Heading"))?;
    rest.trim().parse::<u8>().ok()
}
