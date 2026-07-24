//! Main-document body parsing into v1 block nodes.

use std::collections::BTreeMap;

use casual_doc_model::v1::{
    BlockNode, Break, BreakKind, Drawing, Extent, ExternalTarget, Hyperlink, HyperlinkTarget,
    InlineNode, InternalTarget, MAX_EMU, MediaId, PageMargins, PageSize, Paragraph,
    ParagraphProperties, Run, RunProperties, SectionBoundary, SectionColumns, SectionId, StyleKind,
    Tab,
};
use casual_doc_model::{IdGenerator, NodeId};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::numbering::Numbering;
use crate::properties::{
    apply_paragraph_property, apply_run_property, attribute_value, break_kind,
};
use crate::report::Reporter;
use crate::styles::Styles;

/// A run/tab/break/drawing/hyperlink segment before ids and normalization.
enum Segment {
    Run {
        properties: RunProperties,
        text: String,
    },
    Tab,
    Break(BreakKind),
    Drawing {
        media: MediaId,
        extent: Option<Extent>,
    },
    Hyperlink {
        target: HyperlinkTarget,
        tooltip: Option<String>,
        children: Vec<Segment>,
    },
}

/// A hyperlink being accumulated while inside a `w:hyperlink`.
struct HyperlinkAccumulator {
    target: HyperlinkTarget,
    tooltip: Option<String>,
    segments: Vec<Segment>,
}

/// Raw section geometry accumulated while inside a `w:sectPr`.
#[derive(Default)]
struct SectionAccumulator {
    page_width: Option<i32>,
    page_height: Option<i32>,
    margin_top: Option<i32>,
    margin_bottom: Option<i32>,
    margin_start: Option<i32>,
    margin_end: Option<i32>,
    columns: Option<u16>,
}

struct BodyParser<'a> {
    ids: &'a mut IdGenerator,
    styles: &'a Styles,
    numbering: &'a Numbering,
    reporter: &'a mut Reporter,
    config: ImportConfig,
    /// Resolution index: image relationship id -> the media table entry.
    media_index: &'a BTreeMap<String, MediaId>,
    /// Resolution index: hyperlink relationship id -> external target URL.
    hyperlink_rels: &'a BTreeMap<String, String>,
    elements: u64,
    depth: u64,
    text_bytes: usize,
    in_document: bool,
    in_body: bool,
    paragraph_open: bool,
    paragraph_id: Option<NodeId>,
    paragraph_properties: ParagraphProperties,
    ppr_depth: u32,
    numpr_depth: u32,
    pending_num_id: Option<String>,
    pending_ilvl: u8,
    run_open: bool,
    run_properties: RunProperties,
    rpr_depth: u32,
    in_text: bool,
    text_buffer: String,
    drawing_depth: u32,
    blipfill_depth: u32,
    pending_embed: Option<String>,
    pending_extent: Option<Extent>,
    drawing_extra: bool,
    hyperlink: Option<HyperlinkAccumulator>,
    hyperlink_depth: u32,
    segments: Vec<Segment>,
    paragraphs: Vec<Paragraph>,
    section: Option<SectionAccumulator>,
    sections: Vec<SectionBoundary>,
}

/// Resolution tables the body parser consults while mapping constructs.
pub(crate) struct ParseInputs<'a> {
    pub styles: &'a Styles,
    pub numbering: &'a Numbering,
    pub media_index: &'a BTreeMap<String, MediaId>,
    pub hyperlink_rels: &'a BTreeMap<String, String>,
}

/// Parses main-document body bytes into ordered block nodes, allocating ids.
pub(crate) fn parse<'a>(
    xml: &[u8],
    ids: &'a mut IdGenerator,
    reporter: &'a mut Reporter,
    inputs: ParseInputs<'a>,
    config: ImportConfig,
) -> Result<(Vec<BlockNode>, Vec<SectionBoundary>), ImportError> {
    let mut parser = BodyParser {
        ids,
        styles: inputs.styles,
        numbering: inputs.numbering,
        reporter,
        config,
        media_index: inputs.media_index,
        hyperlink_rels: inputs.hyperlink_rels,
        elements: 0,
        depth: 0,
        text_bytes: 0,
        in_document: false,
        in_body: false,
        paragraph_open: false,
        paragraph_id: None,
        paragraph_properties: ParagraphProperties::default(),
        ppr_depth: 0,
        numpr_depth: 0,
        pending_num_id: None,
        pending_ilvl: 0,
        run_open: false,
        run_properties: RunProperties::default(),
        rpr_depth: 0,
        in_text: false,
        text_buffer: String::new(),
        drawing_depth: 0,
        blipfill_depth: 0,
        pending_embed: None,
        pending_extent: None,
        drawing_extra: false,
        hyperlink: None,
        hyperlink_depth: 0,
        segments: Vec::new(),
        paragraphs: Vec::new(),
        section: None,
        sections: Vec::new(),
    };
    parser.run(xml)?;
    let body = parser
        .paragraphs
        .into_iter()
        .map(BlockNode::Paragraph)
        .collect();
    Ok((body, parser.sections))
}

impl BodyParser<'_> {
    fn next_id(&mut self) -> Result<NodeId, ImportError> {
        self.ids
            .next_id()
            .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })
    }

    fn run(&mut self, xml: &[u8]) -> Result<(), ImportError> {
        let mut reader = Reader::from_reader(xml);
        let mut buffer = Vec::new();
        loop {
            let event = reader
                .read_event_into(&mut buffer)
                .map_err(|_| ImportError::MalformedXml)?;
            match event {
                Event::Eof => break,
                Event::DocType(_) => return Err(ImportError::MalformedXml),
                Event::Start(element) => {
                    self.depth += 1;
                    if self.depth > self.config.max_depth {
                        return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                    }
                    self.on_start(element.local_name().as_ref(), &element)?;
                }
                Event::Empty(element) => {
                    self.on_start(element.local_name().as_ref(), &element)?;
                    self.on_end(element.local_name().as_ref())?;
                }
                Event::End(element) => {
                    self.on_end(element.local_name().as_ref())?;
                    self.depth = self.depth.saturating_sub(1);
                }
                Event::Text(text) if self.in_text => {
                    let raw = text.into_inner();
                    let raw =
                        std::str::from_utf8(raw.as_ref()).map_err(|_| ImportError::MalformedXml)?;
                    let decoded =
                        quick_xml::escape::unescape(raw).map_err(|_| ImportError::MalformedXml)?;
                    self.push_text(decoded.as_ref())?;
                }
                Event::CData(cdata) if self.in_text => {
                    let raw = cdata.into_inner();
                    let text =
                        std::str::from_utf8(raw.as_ref()).map_err(|_| ImportError::MalformedXml)?;
                    self.push_text(text)?;
                }
                _ => {}
            }
            buffer.clear();
        }
        Ok(())
    }

    fn push_text(&mut self, text: &str) -> Result<(), ImportError> {
        self.text_bytes = self.text_bytes.saturating_add(text.len());
        if self.text_bytes > self.config.max_text_bytes {
            return Err(ImportError::LimitExceeded {
                limit: "text_bytes",
            });
        }
        self.text_buffer.push_str(text);
        Ok(())
    }

    fn on_start(&mut self, local: &[u8], element: &BytesStart<'_>) -> Result<(), ImportError> {
        self.elements += 1;
        if self.elements > self.config.max_elements {
            return Err(ImportError::LimitExceeded {
                limit: "xml_elements",
            });
        }
        match local {
            b"document" => self.in_document = true,
            b"body" if self.in_document => self.in_body = true,
            b"p" if self.in_body
                && !self.run_open
                && self.ppr_depth == 0
                && self.rpr_depth == 0 =>
            {
                self.paragraph_open = true;
                self.paragraph_id = Some(self.next_id()?);
                self.paragraph_properties = ParagraphProperties::default();
                self.numpr_depth = 0;
                self.pending_num_id = None;
                self.pending_ilvl = 0;
                self.segments.clear();
            }
            b"pPr" if self.paragraph_open && !self.run_open => self.ppr_depth += 1,
            b"pStyle" if self.ppr_depth > 0 => {
                match self.resolve_style(element, StyleKind::Paragraph) {
                    Some(style) => self.paragraph_properties.style_ref = Some(style),
                    None => self.reporter.report(local),
                }
            }
            b"numPr" if self.ppr_depth > 0 => self.numpr_depth += 1,
            b"numId" if self.numpr_depth > 0 => {
                self.pending_num_id = attribute_value(element, b"val");
            }
            b"ilvl" if self.numpr_depth > 0 => {
                self.pending_ilvl = attribute_value(element, b"val")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
            }
            b"r" if self.paragraph_open => {
                self.run_open = true;
                self.run_properties = RunProperties::default();
            }
            b"rPr" if self.run_open => self.rpr_depth += 1,
            b"rStyle" if self.rpr_depth > 0 => {
                match self.resolve_style(element, StyleKind::Character) {
                    Some(style) => self.run_properties.style_ref = Some(style),
                    None => self.reporter.report(local),
                }
            }
            b"t" if self.run_open => {
                self.in_text = true;
                self.text_buffer.clear();
            }
            b"tab" if self.run_open => self.push_segment(Segment::Tab),
            b"br" if self.run_open => {
                let kind = break_kind(element);
                self.push_segment(Segment::Break(kind));
            }
            b"hyperlink" if self.paragraph_open && !self.run_open => {
                self.hyperlink_depth += 1;
                if self.hyperlink_depth == 1 {
                    match self.resolve_hyperlink_target(element) {
                        Some((target, tooltip)) => {
                            self.hyperlink = Some(HyperlinkAccumulator {
                                target,
                                tooltip,
                                segments: Vec::new(),
                            });
                        }
                        None => self.reporter.report(b"hyperlink"),
                    }
                } else {
                    // A nested hyperlink is not modeled; its runs flatten into
                    // the outer link and the nesting is reported.
                    self.reporter.report(b"hyperlink");
                }
            }
            b"drawing" if self.run_open => {
                self.drawing_depth += 1;
                if self.drawing_depth == 1 {
                    self.pending_embed = None;
                    self.pending_extent = None;
                    self.drawing_extra = false;
                    self.blipfill_depth = 0;
                }
            }
            b"extent" if self.drawing_depth > 0 => {
                if let (Some(cx), Some(cy)) = (attr_i64(element, b"cx"), attr_i64(element, b"cy")) {
                    if (0..=MAX_EMU).contains(&cx) && (0..=MAX_EMU).contains(&cy) {
                        self.pending_extent = Some(Extent {
                            width_emu: cx,
                            height_emu: cy,
                        });
                    }
                }
            }
            b"blipFill" if self.drawing_depth > 0 => self.blipfill_depth += 1,
            b"blip" if self.blipfill_depth > 0 && self.pending_embed.is_none() => {
                self.pending_embed = attribute_value(element, b"embed");
            }
            // A floating anchor, alt text, click-link, or SVG dual-blip carries
            // detail the model does not capture: flag it so a resolved drawing
            // is still reported (degraded), never silently under-modeled.
            b"anchor" if self.drawing_depth > 0 => self.drawing_extra = true,
            b"docPr" if self.drawing_depth > 0 => {
                if attribute_value(element, b"descr").is_some() {
                    self.drawing_extra = true;
                }
            }
            b"hlinkClick" | b"svgBlip" if self.drawing_depth > 0 => self.drawing_extra = true,
            b"sectPr" if self.in_body && !self.paragraph_open && self.ppr_depth == 0 => {
                self.section = Some(SectionAccumulator::default());
            }
            b"pgSz" if self.section.is_some() => {
                if let Some(section) = self.section.as_mut() {
                    section.page_width = attr_i32(element, b"w");
                    section.page_height = attr_i32(element, b"h");
                }
            }
            b"pgMar" if self.section.is_some() => {
                if let Some(section) = self.section.as_mut() {
                    section.margin_top = attr_i32(element, b"top");
                    section.margin_bottom = attr_i32(element, b"bottom");
                    section.margin_start =
                        attr_i32(element, b"start").or_else(|| attr_i32(element, b"left"));
                    section.margin_end =
                        attr_i32(element, b"end").or_else(|| attr_i32(element, b"right"));
                }
            }
            b"cols" if self.section.is_some() => {
                if let Some(section) = self.section.as_mut() {
                    section.columns =
                        attribute_value(element, b"num").and_then(|value| value.parse().ok());
                }
            }
            _ if self.rpr_depth > 0 => {
                if !apply_run_property(&mut self.run_properties, local, element) {
                    self.reporter.report(local);
                }
            }
            _ if self.ppr_depth > 0 => {
                if !apply_paragraph_property(&mut self.paragraph_properties, local, element) {
                    self.reporter.report(local);
                }
            }
            // Known DrawingML scaffolding for an embedded picture is consumed
            // silently; any OTHER element inside a drawing (e.g. a text box)
            // still falls through to the report arm below — no silent loss.
            _ if self.drawing_depth > 0 && is_drawing_scaffolding(local) => {}
            _ if self.in_document => self.reporter.report(local),
            _ => {}
        }
        Ok(())
    }

    fn resolve_style(
        &self,
        element: &BytesStart<'_>,
        expected: StyleKind,
    ) -> Option<casual_doc_model::v1::StyleId> {
        let name = attribute_value(element, b"val")?;
        self.styles.resolve(&name, expected)
    }

    fn on_end(&mut self, local: &[u8]) -> Result<(), ImportError> {
        match local {
            b"document" => self.in_document = false,
            b"body" => self.in_body = false,
            b"p" if self.paragraph_open => self.finish_paragraph()?,
            b"pPr" => self.ppr_depth = self.ppr_depth.saturating_sub(1),
            b"numPr" => {
                self.numpr_depth = self.numpr_depth.saturating_sub(1);
                if self.numpr_depth == 0 {
                    if let Some(num_id) = self.pending_num_id.take() {
                        match self.numbering.resolve(&num_id, self.pending_ilvl) {
                            Some(reference) => {
                                self.paragraph_properties.numbering = Some(reference);
                            }
                            None => self.reporter.report(b"numPr"),
                        }
                    }
                    self.pending_ilvl = 0;
                }
            }
            b"r" => {
                self.run_open = false;
                self.rpr_depth = 0;
            }
            b"rPr" => self.rpr_depth = self.rpr_depth.saturating_sub(1),
            b"sectPr" => {
                if let Some(accumulator) = self.section.take() {
                    self.build_section(accumulator)?;
                }
            }
            b"t" if self.in_text => {
                self.in_text = false;
                let text = std::mem::take(&mut self.text_buffer);
                if !text.is_empty() {
                    let properties = self.run_properties.clone();
                    self.push_segment(Segment::Run { properties, text });
                }
            }
            b"blipFill" => self.blipfill_depth = self.blipfill_depth.saturating_sub(1),
            b"drawing" if self.drawing_depth > 0 => {
                self.drawing_depth -= 1;
                if self.drawing_depth == 0 {
                    self.commit_drawing();
                }
            }
            b"hyperlink" if self.hyperlink_depth > 0 => {
                if self.hyperlink_depth == 1 {
                    if let Some(accumulator) = self.hyperlink.take() {
                        let children = normalize_segments(accumulator.segments);
                        if children.is_empty() {
                            self.reporter.report(b"hyperlink");
                        } else {
                            // Commit to the parent stream: a hyperlink never nests.
                            self.segments.push(Segment::Hyperlink {
                                target: accumulator.target,
                                tooltip: accumulator.tooltip,
                                children,
                            });
                        }
                    }
                }
                self.hyperlink_depth = self.hyperlink_depth.saturating_sub(1);
            }
            _ => {}
        }
        Ok(())
    }

    /// Commits the top-level drawing that just closed. A resolved embed becomes
    /// a `Drawing` segment; an unresolved/dangling embed is reported and
    /// dropped. A resolved drawing carrying unmodeled detail is also reported.
    fn commit_drawing(&mut self) {
        let extent = self.pending_extent.take();
        let extra = self.drawing_extra;
        match self.pending_embed.take() {
            Some(embed) => match self.media_index.get(&embed) {
                Some(media) => {
                    if extra {
                        self.reporter.report(b"drawing");
                    }
                    self.push_segment(Segment::Drawing {
                        media: *media,
                        extent,
                    });
                }
                None => self.reporter.report(b"drawing"),
            },
            None => self.reporter.report(b"drawing"),
        }
    }

    fn build_section(&mut self, accumulator: SectionAccumulator) -> Result<(), ImportError> {
        let id = SectionId::new(self.next_id()?);
        let page_size = PageSize {
            width_twips: accumulator.page_width.unwrap_or(12_240).clamp(1, 31_680),
            height_twips: accumulator.page_height.unwrap_or(15_840).clamp(1, 31_680),
        };
        let page_margins = PageMargins {
            top_twips: accumulator.margin_top.unwrap_or(1_440).clamp(0, 31_680),
            bottom_twips: accumulator.margin_bottom.unwrap_or(1_440).clamp(0, 31_680),
            start_twips: accumulator.margin_start.unwrap_or(1_440).clamp(0, 31_680),
            end_twips: accumulator.margin_end.unwrap_or(1_440).clamp(0, 31_680),
        };
        let columns = SectionColumns {
            count: accumulator.columns.unwrap_or(1).clamp(1, 64),
        };
        self.sections.push(SectionBoundary {
            id,
            page_size,
            page_margins,
            columns,
        });
        Ok(())
    }

    /// Routes a segment into an open hyperlink if one is being accumulated, so
    /// content (runs, tabs, breaks, drawings) inside a `w:hyperlink` is captured
    /// by the link rather than the paragraph.
    fn push_segment(&mut self, segment: Segment) {
        match self.hyperlink.as_mut() {
            Some(accumulator) => accumulator.segments.push(segment),
            None => self.segments.push(segment),
        }
    }

    /// Resolves a `w:hyperlink`'s target: an external URL through the
    /// relationship graph (`r:id`) or an internal bookmark (`w:anchor`).
    /// Returns `None` (report + flatten) when neither resolves in domain.
    fn resolve_hyperlink_target(
        &self,
        element: &BytesStart<'_>,
    ) -> Option<(HyperlinkTarget, Option<String>)> {
        let tooltip = attribute_value(element, b"tooltip")
            .filter(|value| !value.is_empty() && value.len() <= 255);
        if let Some(relationship_id) = attribute_value(element, b"id") {
            let url = self.hyperlink_rels.get(&relationship_id)?;
            if url.is_empty() || url.len() > 2048 {
                return None;
            }
            return Some((
                HyperlinkTarget::External(ExternalTarget { url: url.clone() }),
                tooltip,
            ));
        }
        let anchor = attribute_value(element, b"anchor")?;
        if anchor.is_empty() || anchor.len() > 255 {
            return None;
        }
        Some((
            HyperlinkTarget::Internal(InternalTarget { anchor }),
            tooltip,
        ))
    }

    fn finish_paragraph(&mut self) -> Result<(), ImportError> {
        self.paragraph_open = false;
        self.ppr_depth = 0;
        self.run_open = false;
        // Robustness: a `w:p` that closes with an open hyperlink is malformed;
        // flush what was accumulated so nothing is dropped, then reset state.
        if let Some(accumulator) = self.hyperlink.take() {
            let children = normalize_segments(accumulator.segments);
            if children.is_empty() {
                self.reporter.report(b"hyperlink");
            } else {
                self.segments.push(Segment::Hyperlink {
                    target: accumulator.target,
                    tooltip: accumulator.tooltip,
                    children,
                });
            }
        }
        self.hyperlink_depth = 0;
        self.drawing_depth = 0;
        self.blipfill_depth = 0;
        let paragraph_id = self
            .paragraph_id
            .take()
            .expect("paragraph id was allocated");
        let normalized = normalize_segments(std::mem::take(&mut self.segments));
        let mut inlines = Vec::with_capacity(normalized.len());
        for segment in normalized {
            inlines.push(self.segment_to_inline(segment)?);
        }
        self.paragraphs.push(Paragraph {
            id: paragraph_id,
            properties: std::mem::take(&mut self.paragraph_properties),
            inlines,
        });
        Ok(())
    }

    /// Assigns ids in document order (an opening tag before its children) and
    /// builds the inline node. A hyperlink's own id precedes its children's.
    fn segment_to_inline(&mut self, segment: Segment) -> Result<InlineNode, ImportError> {
        match segment {
            Segment::Run { properties, text } => {
                let id = self.next_id()?;
                Ok(InlineNode::Run(Run {
                    id,
                    properties,
                    text,
                }))
            }
            Segment::Tab => {
                let id = self.next_id()?;
                Ok(InlineNode::Tab(Tab { id }))
            }
            Segment::Break(kind) => {
                let id = self.next_id()?;
                Ok(InlineNode::Break(Break { id, kind }))
            }
            Segment::Drawing { media, extent } => {
                let id = self.next_id()?;
                Ok(InlineNode::Drawing(Drawing { id, media, extent }))
            }
            Segment::Hyperlink {
                target,
                tooltip,
                children,
            } => {
                let id = self.next_id()?;
                let mut inlines = Vec::with_capacity(children.len());
                for child in children {
                    inlines.push(self.segment_to_inline(child)?);
                }
                Ok(InlineNode::Hyperlink(Hyperlink {
                    id,
                    target,
                    tooltip,
                    inlines,
                }))
            }
        }
    }
}

fn attr_i32(element: &BytesStart<'_>, name: &[u8]) -> Option<i32> {
    attribute_value(element, name).and_then(|value| value.parse().ok())
}

fn attr_i64(element: &BytesStart<'_>, name: &[u8]) -> Option<i64> {
    attribute_value(element, name).and_then(|value| value.parse().ok())
}

/// Whether a local element name is known DrawingML scaffolding for an embedded
/// picture (consumed silently while inside a `w:drawing`). Anything not listed
/// still reports, so genuinely unmodeled drawing content is never lost.
fn is_drawing_scaffolding(local: &[u8]) -> bool {
    matches!(
        local,
        b"inline"
            | b"anchor"
            | b"simplePos"
            | b"positionH"
            | b"positionV"
            | b"posOffset"
            | b"align"
            | b"wrapNone"
            | b"wrapSquare"
            | b"wrapTight"
            | b"wrapThrough"
            | b"wrapTopAndBottom"
            | b"wrapPolygon"
            | b"start"
            | b"lineTo"
            | b"effectExtent"
            | b"docPr"
            | b"cNvGraphicFramePr"
            | b"graphicFrameLocks"
            | b"graphic"
            | b"graphicData"
            | b"pic"
            | b"nvPicPr"
            | b"cNvPr"
            | b"cNvPicPr"
            | b"picLocks"
            | b"hlinkClick"
            | b"spPr"
            | b"xfrm"
            | b"off"
            | b"ext"
            | b"prstGeom"
            | b"avLst"
            | b"custGeom"
            | b"ln"
            | b"noFill"
            | b"solidFill"
            | b"srgbClr"
            | b"stretch"
            | b"fillRect"
            | b"srcRect"
            | b"blipFill"
            | b"blip"
            | b"extLst"
            | b"svgBlip"
    )
}

fn normalize_segments(segments: Vec<Segment>) -> Vec<Segment> {
    let mut normalized: Vec<Segment> = Vec::with_capacity(segments.len());
    for segment in segments {
        match segment {
            Segment::Run { text, .. } if text.is_empty() => {}
            Segment::Run { properties, text } => {
                if let Some(Segment::Run {
                    properties: previous_properties,
                    text: previous_text,
                }) = normalized.last_mut()
                {
                    if *previous_properties == properties {
                        previous_text.push_str(&text);
                        continue;
                    }
                }
                normalized.push(Segment::Run { properties, text });
            }
            other => normalized.push(other),
        }
    }
    normalized
}
