//! Main-document body parsing into v1 block nodes.

use casual_doc_model::v1::{
    BlockNode, Break, BreakKind, InlineNode, Paragraph, ParagraphProperties, Run, RunProperties,
    StyleKind, Tab,
};
use casual_doc_model::{IdGenerator, NodeId};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::{
    apply_paragraph_property, apply_run_property, attribute_value, break_kind,
};
use crate::report::Reporter;
use crate::styles::Styles;

/// A run/tab/break segment before ids and normalization are assigned.
enum Segment {
    Run {
        properties: RunProperties,
        text: String,
    },
    Tab,
    Break(BreakKind),
}

struct BodyParser<'a> {
    ids: &'a mut IdGenerator,
    styles: &'a Styles,
    reporter: &'a mut Reporter,
    config: ImportConfig,
    elements: u64,
    depth: u64,
    text_bytes: usize,
    in_document: bool,
    in_body: bool,
    paragraph_open: bool,
    paragraph_id: Option<NodeId>,
    paragraph_properties: ParagraphProperties,
    ppr_depth: u32,
    run_open: bool,
    run_properties: RunProperties,
    rpr_depth: u32,
    in_text: bool,
    text_buffer: String,
    segments: Vec<Segment>,
    paragraphs: Vec<Paragraph>,
}

/// Parses main-document body bytes into ordered block nodes, allocating ids.
pub(crate) fn parse(
    xml: &[u8],
    ids: &mut IdGenerator,
    styles: &Styles,
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<Vec<BlockNode>, ImportError> {
    let mut parser = BodyParser {
        ids,
        styles,
        reporter,
        config,
        elements: 0,
        depth: 0,
        text_bytes: 0,
        in_document: false,
        in_body: false,
        paragraph_open: false,
        paragraph_id: None,
        paragraph_properties: ParagraphProperties::default(),
        ppr_depth: 0,
        run_open: false,
        run_properties: RunProperties::default(),
        rpr_depth: 0,
        in_text: false,
        text_buffer: String::new(),
        segments: Vec::new(),
        paragraphs: Vec::new(),
    };
    parser.run(xml)?;
    Ok(parser
        .paragraphs
        .into_iter()
        .map(BlockNode::Paragraph)
        .collect())
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
                self.segments.clear();
            }
            b"pPr" if self.paragraph_open && !self.run_open => self.ppr_depth += 1,
            b"pStyle" if self.ppr_depth > 0 => {
                match self.resolve_style(element, StyleKind::Paragraph) {
                    Some(style) => self.paragraph_properties.style_ref = Some(style),
                    None => self.reporter.report(local),
                }
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
            b"tab" if self.run_open => self.segments.push(Segment::Tab),
            b"br" if self.run_open => self.segments.push(Segment::Break(break_kind(element))),
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
            b"r" => {
                self.run_open = false;
                self.rpr_depth = 0;
            }
            b"rPr" => self.rpr_depth = self.rpr_depth.saturating_sub(1),
            b"t" if self.in_text => {
                self.in_text = false;
                let text = std::mem::take(&mut self.text_buffer);
                if !text.is_empty() {
                    self.segments.push(Segment::Run {
                        properties: self.run_properties.clone(),
                        text,
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn finish_paragraph(&mut self) -> Result<(), ImportError> {
        self.paragraph_open = false;
        self.ppr_depth = 0;
        self.run_open = false;
        let paragraph_id = self
            .paragraph_id
            .take()
            .expect("paragraph id was allocated");
        let normalized = normalize_segments(std::mem::take(&mut self.segments));
        let mut inlines = Vec::with_capacity(normalized.len());
        for segment in normalized {
            let id = self.next_id()?;
            inlines.push(match segment {
                Segment::Run { properties, text } => InlineNode::Run(Run {
                    id,
                    properties,
                    text,
                }),
                Segment::Tab => InlineNode::Tab(Tab { id }),
                Segment::Break(kind) => InlineNode::Break(Break { id, kind }),
            });
        }
        self.paragraphs.push(Paragraph {
            id: paragraph_id,
            properties: std::mem::take(&mut self.paragraph_properties),
            inlines,
        });
        Ok(())
    }
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
