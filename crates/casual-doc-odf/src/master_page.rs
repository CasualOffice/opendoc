//! Bounded import of ODT master-page header/footer content into schema-v1.
//!
//! This is the styles.xml companion to [`crate::page_style`]. It scans the first
//! `style:master-page` and lifts its `style:header`/`style:footer` regions (and
//! the `-left` even-page variants) into a plain-text paragraph draft that
//! [`crate::package`] turns into schema-v1 `HeaderFooter` definitions.
//!
//! The first checkpoint is deliberately bounded: paragraphs made of plain runs,
//! explicit spaces (`text:s`), tabs (`text:tab`), and line breaks
//! (`text:line-break`). Run/paragraph formatting, headings, lists, tables,
//! links, notes, drawings, first-page regions, and additional master-pages are
//! reported as compatibility findings rather than silently dropped, matching the
//! crate's bounded-subset philosophy. The walk is lenient about
//! unsupported-but-valid constructs (never turns a currently-importable ODT into
//! a failure) and strict about malformed XML and resource limits (fails closed).

use std::collections::BTreeMap;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::content::{ModelOutcome, OdfImportLimits};
use crate::{OdfError, RetentionOutcome};

/// One inline fragment inside a bounded header/footer paragraph. Text fragments
/// are always maximal (adjacent text is merged) and separated by structural
/// leaves so the built model never has adjacent equivalent runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HeaderFooterInline {
    /// A plain (unformatted) run of text.
    Text(String),
    /// An explicit tab.
    Tab,
    /// An explicit line break.
    LineBreak,
}

/// One bounded header/footer paragraph (may be empty — a blank line).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct HeaderFooterParagraph {
    /// The paragraph's inline content in document order.
    pub inlines: Vec<HeaderFooterInline>,
}

/// The plain-text block content of one header or footer region.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct HeaderFooterRegion {
    /// The region's paragraphs in document order.
    pub paragraphs: Vec<HeaderFooterParagraph>,
}

/// The bounded master-page content lifted from styles.xml.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MasterPageContent {
    /// `style:header` content.
    pub default_header: Option<HeaderFooterRegion>,
    /// `style:header-left` content.
    pub even_header: Option<HeaderFooterRegion>,
    /// `style:footer` content.
    pub default_footer: Option<HeaderFooterRegion>,
    /// `style:footer-left` content.
    pub even_footer: Option<HeaderFooterRegion>,
    /// Deterministic compatibility findings for deferred constructs.
    pub findings: Vec<(String, ModelOutcome, RetentionOutcome)>,
}

/// Which region slot an open `style:header*`/`style:footer*` element targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RegionSlot {
    DefaultHeader,
    EvenHeader,
    DefaultFooter,
    EvenFooter,
}

/// Deterministic finding accumulator (deduplicates and sums occurrences).
#[derive(Default)]
struct Findings {
    entries: BTreeMap<(String, ModelOutcome, RetentionOutcome), u32>,
}

impl Findings {
    fn record(&mut self, feature: &str, model: ModelOutcome, retention: RetentionOutcome) {
        let counter = self
            .entries
            .entry((feature.to_owned(), model, retention))
            .or_insert(0);
        *counter = counter.saturating_add(1);
    }

    fn into_vec(self) -> Vec<(String, ModelOutcome, RetentionOutcome)> {
        self.entries.into_keys().collect()
    }
}

/// Mutable parse state shared across the single-pass walk.
struct State {
    content: MasterPageContent,
    findings: Findings,
    depth: usize,
    elements: usize,
    attributes: usize,
    attribute_bytes: usize,
    paragraphs: usize,
    inlines: usize,
    text_bytes: usize,
    master_page_open: bool,
    master_page_depth: usize,
    master_page_done: bool,
    /// When set, every element at or below this depth is skipped.
    skip_depth: Option<usize>,
    region_slot: Option<RegionSlot>,
    region_depth: usize,
    region: HeaderFooterRegion,
    para_open: bool,
    para_depth: usize,
    para: HeaderFooterParagraph,
    text: String,
}

/// Parses the first master-page's header/footer regions from styles.xml.
///
/// The same `OdfImportLimits` fields that bound content.xml bound the master-page
/// walk; no new limit surface is introduced.
pub(crate) fn parse_master_page(
    bytes: &[u8],
    limits: OdfImportLimits,
) -> Result<MasterPageContent, OdfError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut state = State {
        content: MasterPageContent::default(),
        findings: Findings::default(),
        depth: 0,
        elements: 0,
        attributes: 0,
        attribute_bytes: 0,
        paragraphs: 0,
        inlines: 0,
        text_bytes: 0,
        master_page_open: false,
        master_page_depth: 0,
        master_page_done: false,
        skip_depth: None,
        region_slot: None,
        region_depth: 0,
        region: HeaderFooterRegion::default(),
        para_open: false,
        para_depth: 0,
        para: HeaderFooterParagraph::default(),
        text: String::new(),
    };

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| OdfError::MalformedContent)?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(OdfError::MalformedContent),
            Event::Start(start) => {
                let element_depth = state
                    .depth
                    .checked_add(1)
                    .ok_or(OdfError::MalformedContent)?;
                count_element(&mut state, &start, element_depth, limits)?;
                open_element(&mut state, &start, element_depth, false, limits)?;
                state.depth = element_depth;
            }
            Event::Empty(start) => {
                let element_depth = state
                    .depth
                    .checked_add(1)
                    .ok_or(OdfError::MalformedContent)?;
                count_element(&mut state, &start, element_depth, limits)?;
                open_element(&mut state, &start, element_depth, true, limits)?;
                // Empty elements do not nest; `state.depth` is unchanged.
            }
            Event::Text(text) => {
                if state.skip_depth.is_none() && state.para_open {
                    let decoded = text.decode().map_err(|_| OdfError::MalformedContent)?;
                    let value = quick_xml::escape::unescape(&decoded)
                        .map_err(|_| OdfError::MalformedContent)?;
                    append_text(&mut state, &value, limits)?;
                }
            }
            Event::CData(text) => {
                if state.skip_depth.is_none() && state.para_open {
                    let value = text.decode().map_err(|_| OdfError::MalformedContent)?;
                    append_text(&mut state, &value, limits)?;
                }
            }
            Event::End(_) => close_element(&mut state, limits)?,
            _ => {}
        }
        buffer.clear();
    }

    // quick-xml does not error on elements left open at EOF; a non-zero depth
    // (or any still-open context) means the input was truncated, so fail closed
    // rather than silently dropping the unterminated region.
    if state.depth != 0
        || state.master_page_open
        || state.region_slot.is_some()
        || state.para_open
        || state.skip_depth.is_some()
    {
        return Err(OdfError::MalformedContent);
    }

    let mut content = state.content;
    content.findings = state.findings.into_vec();
    Ok(content)
}

/// Enforces per-element XML budgets (depth, element and attribute counts, and
/// aggregate attribute bytes) for every opened element, skipped or not.
fn count_element(
    state: &mut State,
    start: &BytesStart<'_>,
    element_depth: usize,
    limits: OdfImportLimits,
) -> Result<(), OdfError> {
    enforce("odf_content_xml_depth", element_depth, limits.max_xml_depth)?;
    state.elements = increment(state.elements)?;
    enforce(
        "odf_content_xml_elements",
        state.elements,
        limits.max_xml_elements,
    )?;
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        state.attributes = increment(state.attributes)?;
        enforce(
            "odf_content_xml_attributes",
            state.attributes,
            limits.max_xml_attributes,
        )?;
        state.attribute_bytes = state
            .attribute_bytes
            .checked_add(attribute.value.len())
            .ok_or(OdfError::MalformedContent)?;
        enforce(
            "odf_content_xml_attribute_bytes",
            state.attribute_bytes,
            limits.max_xml_attribute_bytes,
        )?;
    }
    Ok(())
}

/// Processes one opened element (`Start` or `Empty`) against the current context.
fn open_element(
    state: &mut State,
    start: &BytesStart<'_>,
    element_depth: usize,
    empty: bool,
    limits: OdfImportLimits,
) -> Result<(), OdfError> {
    // While skipping an unsupported subtree, non-empty children only deepen the
    // skip; empty children are inert. The skip is cleared in `close_element`.
    if state.skip_depth.is_some() {
        return Ok(());
    }
    let name = start.name();
    let local = local_name(name.as_ref());

    // Inside an open paragraph: map the bounded inline subset.
    if state.para_open && element_depth > state.para_depth {
        return open_paragraph_child(state, start, element_depth, empty, local, limits);
    }

    // Inside an open region but not yet in a paragraph: only paragraphs map.
    if state.region_slot.is_some() && !state.para_open {
        if local == b"p" && element_depth == state.region_depth + 1 {
            state.paragraphs = increment(state.paragraphs)?;
            enforce(
                "odf_content_paragraphs",
                state.paragraphs,
                limits.max_paragraphs,
            )?;
            if empty {
                state
                    .region
                    .paragraphs
                    .push(HeaderFooterParagraph::default());
            } else {
                state.para_open = true;
                state.para_depth = element_depth;
                state.para = HeaderFooterParagraph::default();
                state.text.clear();
            }
            return Ok(());
        }
        if local == b"h" && element_depth == state.region_depth + 1 {
            state.findings.record(
                "odf.master-page.heading",
                ModelOutcome::Omitted,
                RetentionOutcome::NotRetained,
            );
            skip_if_start(state, element_depth, empty);
            return Ok(());
        }
        // Any other region child (lists, tables, tracked content) is dropped.
        state.findings.record(
            "odf.master-page.unsupported-content",
            ModelOutcome::Omitted,
            RetentionOutcome::NotRetained,
        );
        skip_if_start(state, element_depth, empty);
        return Ok(());
    }

    // Master-page region openings.
    if state.master_page_open
        && state.region_slot.is_none()
        && element_depth == state.master_page_depth + 1
    {
        if let Some(slot) = region_slot(local) {
            if slot_filled(&state.content, slot) {
                state.findings.record(
                    "odf.master-page.duplicate-region",
                    ModelOutcome::Omitted,
                    RetentionOutcome::NotRetained,
                );
                skip_if_start(state, element_depth, empty);
                return Ok(());
            }
            if empty {
                store_region(&mut state.content, slot, HeaderFooterRegion::default());
            } else {
                state.region_slot = Some(slot);
                state.region_depth = element_depth;
                state.region = HeaderFooterRegion::default();
            }
            return Ok(());
        }
        if matches!(local, b"header-first" | b"footer-first") {
            state.findings.record(
                "odf.master-page.first-page-region",
                ModelOutcome::Omitted,
                RetentionOutcome::NotRetained,
            );
            skip_if_start(state, element_depth, empty);
            return Ok(());
        }
        // Non-header/footer master-page children (e.g. background styling) are
        // outside this checkpoint; ignore without a finding to avoid noise.
        skip_if_start(state, element_depth, empty);
        return Ok(());
    }

    // Master-page element itself.
    if local == b"master-page" {
        if state.master_page_done || state.master_page_open {
            state.findings.record(
                "odf.master-page.multiple",
                ModelOutcome::Omitted,
                RetentionOutcome::NotRetained,
            );
            skip_if_start(state, element_depth, empty);
            return Ok(());
        }
        if empty {
            state.master_page_done = true;
        } else {
            state.master_page_open = true;
            state.master_page_depth = element_depth;
        }
        return Ok(());
    }

    // Structural containers on the way to the first master-page
    // (`office:document-styles`, `office:master-styles`) are just traversed.
    Ok(())
}

/// Maps the bounded inline subset inside an open header/footer paragraph.
fn open_paragraph_child(
    state: &mut State,
    start: &BytesStart<'_>,
    element_depth: usize,
    empty: bool,
    local: &[u8],
    limits: OdfImportLimits,
) -> Result<(), OdfError> {
    match local {
        b"span" => {
            // Spans are transparent for text; their styling is not modeled yet.
            if span_is_styled(start) {
                state.findings.record(
                    "odf.master-page.run-formatting",
                    ModelOutcome::Degraded,
                    RetentionOutcome::NotRetained,
                );
            }
            Ok(())
        }
        b"s" => {
            let count = space_count(start)?;
            enforce("odf_content_space_repeat", count, limits.max_space_repeat)?;
            for _ in 0..count {
                state.text.push(' ');
            }
            state.text_bytes = state
                .text_bytes
                .checked_add(count)
                .ok_or(OdfError::MalformedContent)?;
            enforce(
                "odf_content_text_bytes",
                state.text_bytes,
                limits.max_text_bytes,
            )?;
            Ok(())
        }
        b"tab" => {
            flush_text(state, limits)?;
            push_inline(state, HeaderFooterInline::Tab, limits)
        }
        b"line-break" => {
            flush_text(state, limits)?;
            push_inline(state, HeaderFooterInline::LineBreak, limits)
        }
        _ => {
            // Links, notes, fields, drawings, and similar inline content drop to
            // a finding; their text is not salvaged in this checkpoint.
            state.findings.record(
                "odf.master-page.unsupported-content",
                ModelOutcome::Degraded,
                RetentionOutcome::NotRetained,
            );
            skip_if_start(state, element_depth, empty);
            Ok(())
        }
    }
}

/// Handles an `End` event, closing whichever open context matches the depth.
fn close_element(state: &mut State, limits: OdfImportLimits) -> Result<(), OdfError> {
    let closing_depth = state.depth;
    state.depth = state.depth.saturating_sub(1);

    if let Some(skip_depth) = state.skip_depth {
        if closing_depth <= skip_depth {
            state.skip_depth = None;
        }
        return Ok(());
    }

    if state.para_open && closing_depth == state.para_depth {
        // Charge the trailing text run against the inline-node budget, exactly
        // like a run terminated by a tab or line break.
        flush_text(state, limits)?;
        let para = std::mem::take(&mut state.para);
        state.region.paragraphs.push(para);
        state.para_open = false;
        return Ok(());
    }

    if let Some(slot) = state.region_slot
        && !state.para_open
        && closing_depth == state.region_depth
    {
        let region = std::mem::take(&mut state.region);
        store_region(&mut state.content, slot, region);
        state.region_slot = None;
        return Ok(());
    }

    if state.master_page_open && closing_depth == state.master_page_depth {
        state.master_page_open = false;
        state.master_page_done = true;
    }
    Ok(())
}

/// Flushes the pending text buffer into a maximal text inline, if any.
fn flush_text(state: &mut State, limits: OdfImportLimits) -> Result<(), OdfError> {
    if state.text.is_empty() {
        return Ok(());
    }
    let text = std::mem::take(&mut state.text);
    push_inline(state, HeaderFooterInline::Text(text), limits)
}

/// Appends raw character data to the current paragraph's text buffer.
fn append_text(state: &mut State, text: &str, limits: OdfImportLimits) -> Result<(), OdfError> {
    if text.is_empty() {
        return Ok(());
    }
    state.text.push_str(text);
    state.text_bytes = state
        .text_bytes
        .checked_add(text.len())
        .ok_or(OdfError::MalformedContent)?;
    enforce(
        "odf_content_text_bytes",
        state.text_bytes,
        limits.max_text_bytes,
    )
}

/// Pushes one inline node into the current paragraph, charging the inline bound.
fn push_inline(
    state: &mut State,
    inline: HeaderFooterInline,
    limits: OdfImportLimits,
) -> Result<(), OdfError> {
    state.inlines = increment(state.inlines)?;
    enforce(
        "odf_content_inline_nodes",
        state.inlines,
        limits.max_inline_nodes,
    )?;
    state.para.inlines.push(inline);
    Ok(())
}

/// Begins skipping an unsupported subtree opened by a non-empty element.
fn skip_if_start(state: &mut State, element_depth: usize, empty: bool) {
    if !empty {
        state.skip_depth = Some(element_depth);
    }
}

fn region_slot(local: &[u8]) -> Option<RegionSlot> {
    match local {
        b"header" => Some(RegionSlot::DefaultHeader),
        b"header-left" => Some(RegionSlot::EvenHeader),
        b"footer" => Some(RegionSlot::DefaultFooter),
        b"footer-left" => Some(RegionSlot::EvenFooter),
        _ => None,
    }
}

fn slot_filled(content: &MasterPageContent, slot: RegionSlot) -> bool {
    match slot {
        RegionSlot::DefaultHeader => content.default_header.is_some(),
        RegionSlot::EvenHeader => content.even_header.is_some(),
        RegionSlot::DefaultFooter => content.default_footer.is_some(),
        RegionSlot::EvenFooter => content.even_footer.is_some(),
    }
}

fn store_region(content: &mut MasterPageContent, slot: RegionSlot, region: HeaderFooterRegion) {
    match slot {
        RegionSlot::DefaultHeader => content.default_header = Some(region),
        RegionSlot::EvenHeader => content.even_header = Some(region),
        RegionSlot::DefaultFooter => content.default_footer = Some(region),
        RegionSlot::EvenFooter => content.even_footer = Some(region),
    }
}

/// Whether a `text:span` carries any style reference (formatting we drop).
fn span_is_styled(start: &BytesStart<'_>) -> bool {
    start.attributes().flatten().any(|attribute| {
        matches!(
            local_name(attribute.key.as_ref()),
            b"style-name" | b"class-names"
        )
    })
}

/// Reads the `text:c` repeat count on a `text:s` element (default 1).
fn space_count(start: &BytesStart<'_>) -> Result<usize, OdfError> {
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        if local_name(attribute.key.as_ref()) == b"c" {
            let value = String::from_utf8_lossy(attribute.value.as_ref());
            return value
                .trim()
                .parse::<usize>()
                .map_err(|_| OdfError::MalformedContent);
        }
    }
    Ok(1)
}

/// Returns the local part of a possibly-prefixed XML name.
fn local_name(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|byte| *byte == b':') {
        Some(index) => &name[index + 1..],
        None => name,
    }
}

fn increment(value: usize) -> Result<usize, OdfError> {
    value.checked_add(1).ok_or(OdfError::MalformedContent)
}

fn enforce(limit: &'static str, observed: usize, allowed: usize) -> Result<(), OdfError> {
    if observed > allowed {
        return Err(OdfError::LimitExceeded {
            limit,
            observed,
            allowed,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncated_input_fails_closed() {
        // quick-xml does not error on tags left open at EOF; the post-loop
        // balance check must reject the truncated region rather than drop it.
        let xml = b"<office:document-styles><office:master-styles><style:master-page><style:header><text:p>hi";
        assert!(parse_master_page(xml, OdfImportLimits::default()).is_err());
    }

    #[test]
    fn plain_header_and_footer_regions_map_to_text() {
        let xml = br#"<office:document-styles><office:master-styles><style:master-page><style:header><text:p>Head</text:p></style:header><style:footer><text:p>Foot</text:p></style:footer></style:master-page></office:master-styles></office:document-styles>"#;
        let content = parse_master_page(xml, OdfImportLimits::default()).unwrap();
        let header = content.default_header.unwrap();
        assert_eq!(header.paragraphs.len(), 1);
        assert_eq!(
            header.paragraphs[0].inlines,
            vec![HeaderFooterInline::Text("Head".to_owned())]
        );
        assert!(content.default_footer.is_some());
        assert!(content.even_header.is_none());
    }
}
