//! `fixed::build`: page evidence -> a fixed-layout `v1::Document`
//! (`docs/182` §4.1).
//!
//! Every PDF page becomes one section (page size from the CropBox, rotation
//! applied, zero margins) holding one anchor paragraph. That paragraph carries
//! the page's objects as page-anchored floats, in paint order, with a
//! monotonically increasing `relative_height` so z-order follows paint order:
//!
//! - each text block (`group.rs`) -> a `TextBox` with real paragraphs and runs,
//!   `LineRule::Exact` from the measured baselines, zero insets, and
//!   grow-on-overflow (`TextBoxAutoFit::Shape`, Acrobat's behaviour);
//! - each image -> an `AnchoredDrawing`, its clip as a `CropRect`;
//! - each maximal run of consecutive paths -> one `WordprocessingGroup` of
//!   `GroupShape { CustomGeometry }`, in page-EMU child space.
//!
//! Floats are placed on the page their anchoring paragraph lands on
//! (`casual-doc-layout/src/anchor.rs`), so a page's frames cannot drift.
//!
//! Complexity: O(evidence) per page; the whole build is O(document).

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::v1::{
    Alignment, AnchorHorizontal, AnchorVertical, AnchoredDrawing, BlockNode, Break, BreakKind,
    Color, CropRect, CustomGeometry, DashStyle, Definitions, Document, DrawingAnchor, Extent, Fill,
    FontName, FontRef, GeometryPoint, GeometryValue, GroupChild, GroupShape, GroupTransform,
    HorizontalAnchor, HorizontalPosition, Indentation, InlineNode, LineRule,
    MAX_SHAPE_PATH_COMMANDS, MediaId, MediaReference, PageMargins, PageSize, Paragraph,
    ParagraphProperties, PathFill, PointEmu, RgbColor, Rgba, Run, RunProperties, SectionBoundary,
    SectionColumns, SectionId, ShapeGeometry, ShapePath, ShapePathCommand, ShapeStroke, Spacing,
    TextBox, TextBoxAutoFit, TextBoxBodyProperties, TextBoxInsets, VerticalAnchor,
    VerticalPosition, WordprocessingGroup, WrapDistances, WrapMode,
};
use casual_doc_model::v1::{EmbeddedFontProgram, FillRule, FontProgramFormat, FontProgramStyle};
use casual_doc_model::{IdGenerator, NodeId};
use casual_doc_pdf_read::{
    ActiveContent, DocumentFonts, FontEvidence, FontProgramKind, GlyphRunEvidence, ImageEvidence,
    ImageFormat, PageEvidence, PaintEvidence, PathCommand, PathEvidence, RenderMode,
};

use crate::PdfImportError;
use crate::group::{self, ASCENT, Block, LayoutParams};
use crate::report::{self, Findings, PdfModelOutcome as Outcome};

/// EMU per point.
const EMU_PER_PT: f64 = 12_700.0;
/// The model's section page-side bound in twips (22 in).
const MAX_PAGE_TWIPS: i32 = 31_680;
/// A frame is widened by this fraction so a substituted face whose advances
/// run a little wider than the PDF's does not rewrap an unedited line.
const FRAME_SLACK_FRACTION: f64 = 0.04;
/// And by this many points.
const FRAME_SLACK_POINTS: f64 = 2.0;
/// Rotation (degrees) below which text counts as upright.
const UPRIGHT_DEGREES: f64 = 0.5;

fn emu(points: f64) -> i64 {
    (points * EMU_PER_PT).round() as i64
}

fn twips(points: f64) -> i32 {
    (points * 20.0).round() as i32
}

fn rgba(color: [u8; 4]) -> Rgba {
    Rgba {
        r: color[0],
        g: color[1],
        b: color[2],
        a: color[3],
    }
}

fn page_anchor(x: f64, y: f64) -> DrawingAnchor {
    DrawingAnchor {
        horizontal: AnchorHorizontal {
            relative_from: HorizontalAnchor::Page,
            position: HorizontalPosition::Offset(emu(x)),
        },
        vertical: AnchorVertical {
            relative_from: VerticalAnchor::Page,
            position: VerticalPosition::Offset(emu(y)),
        },
        wrap: WrapMode::None,
        wrap_text: None,
        wrap_distances: WrapDistances::default(),
        wrap_polygon: None,
        behind_doc: false,
    }
}

/// The family an embedded program registers under and its runs name: the
/// descriptor's family, else the base name before any style suffix.
fn embedded_family(font: &FontEvidence) -> String {
    font.family
        .clone()
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| {
            let name = font.name.as_str();
            let cut = name.find(['-', ',']).unwrap_or(name.len());
            name[..cut].to_owned()
        })
}

/// Whether the sfnt `bytes` maps every character in `chars` through a cmap.
/// O(characters).
fn covers(bytes: &[u8], chars: &BTreeSet<char>) -> bool {
    use skrifa::MetadataProvider;
    let Ok(face) = skrifa::FontRef::new(bytes) else {
        return false;
    };
    let charmap = face.charmap();
    charmap.has_map() && chars.iter().all(|c| charmap.map(*c).is_some())
}

/// The face the run is drawn with when its program is not registered embedded programs can be registered
/// (`docs/182` G-1): the standard 14 map to their metric-compatible Office
/// names; anything else keeps its family name with the style suffix removed.
fn substitute_family(font: &FontEvidence) -> String {
    let base = font
        .family
        .clone()
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| {
            let name = font.name.as_str();
            let cut = name.find(['-', ',']).unwrap_or(name.len());
            name[..cut].to_owned()
        });
    let lower = base.to_ascii_lowercase();
    if lower.starts_with("helvetica") || lower == "arialmt" {
        "Arial".to_owned()
    } else if lower.starts_with("times") {
        "Times New Roman".to_owned()
    } else if lower.starts_with("courier") {
        "Courier New".to_owned()
    } else if base.is_empty() || font.subtype == "Type3" {
        if font.monospace {
            "Courier New"
        } else if font.serif {
            "Times New Roman"
        } else {
            "Arial"
        }
        .to_owned()
    } else {
        base
    }
}

/// A finished build: the document, its media and the report.
pub(crate) type Built = (Document, BTreeMap<String, Vec<u8>>, crate::PdfImportReport);

pub(crate) struct Builder {
    ids: IdGenerator,
    params: LayoutParams,
    body: Vec<BlockNode>,
    sections: Vec<SectionBoundary>,
    media: Vec<(MediaId, MediaReference)>,
    resources: BTreeMap<String, Vec<u8>>,
    findings: Findings,
    z: u32,
    /// Font index -> whether its program was registered.
    font_embedded: BTreeMap<usize, bool>,
    /// Font index -> the family its runs name, when embedded.
    font_family: BTreeMap<usize, String>,
    /// Embedded fonts a later page drew characters outside of.
    font_partial: BTreeSet<usize>,
    /// The programs registered (`Definitions::font_programs`).
    programs: Vec<EmbeddedFontProgram>,
    /// The previous page's anchor paragraph, waiting for the next page so it
    /// can carry the section break that ends its page.
    pending: Option<(Paragraph, SectionId)>,
    /// Line-against-block comparisons made by grouping (the work counter).
    pub(crate) comparisons: u64,
}

impl Builder {
    pub(crate) fn new(namespace: u64, active: &ActiveContent) -> Self {
        let mut findings = Findings::default();
        findings.record(
            report::ACTIVE_CONTENT_BLOCKED,
            Outcome::Omitted,
            active.javascript + active.actions,
            None,
        );
        findings.record(
            report::ATTACHMENT_OMITTED,
            Outcome::Omitted,
            active.embedded_files,
            None,
        );
        findings.record(
            report::FORM_OMITTED,
            Outcome::Omitted,
            active.form_fields,
            None,
        );
        findings.record(
            report::ANNOTATION_OMITTED,
            Outcome::Omitted,
            active.annotations + active.links,
            None,
        );
        findings.record(
            report::TAG_STRUCTURE_IGNORED,
            Outcome::Degraded,
            u32::from(active.tagged),
            None,
        );
        findings.record(
            report::OPTIONAL_CONTENT_FLATTENED,
            Outcome::Degraded,
            u32::from(active.optional_content),
            None,
        );
        Self {
            ids: IdGenerator::new(namespace),
            params: LayoutParams::PDFMINER,
            body: Vec::new(),
            sections: Vec::new(),
            media: Vec::new(),
            resources: BTreeMap::new(),
            findings,
            z: 0,
            font_embedded: BTreeMap::new(),
            font_family: BTreeMap::new(),
            font_partial: BTreeSet::new(),
            programs: Vec::new(),
            pending: None,
            comparisons: 0,
        }
    }

    /// The findings, for the OCR stage that runs before each page is built.
    pub(crate) fn findings_mut(&mut self) -> &mut Findings {
        &mut self.findings
    }

    fn id(&mut self) -> Result<NodeId, PdfImportError> {
        self.ids
            .next_id()
            .map_err(|error| PdfImportError::Model(error.to_string()))
    }

    fn next_z(&mut self) -> u32 {
        self.z = self.z.saturating_add(1);
        self.z
    }

    /// Adds one page. O(page evidence).
    pub(crate) fn page(
        &mut self,
        page: &PageEvidence,
        fonts: &DocumentFonts,
    ) -> Result<(), PdfImportError> {
        let section = self.section(page)?;
        let notes = &page.notes;
        let at = Some(page.index);
        self.findings.record(
            report::PATTERN_FLATTENED,
            Outcome::Degraded,
            notes.pattern_paints,
            at,
        );
        self.findings.record(
            report::TRANSPARENCY_FLATTENED,
            Outcome::Degraded,
            notes.transparency,
            at,
        );
        self.findings.record(
            report::CLIP_IGNORED,
            Outcome::Degraded,
            notes.clipped_text,
            at,
        );
        self.findings.record(
            report::TYPE3_GLYPHS_SUBSTITUTED,
            Outcome::Degraded,
            notes.type3_glyphs,
            at,
        );
        self.findings.record(
            report::IMAGE_UNDECODABLE,
            Outcome::Omitted,
            notes.undecodable_images,
            at,
        );

        let mut inlines = Vec::new();
        let mut index = 0;
        let items = &page.items;
        while index < items.len() {
            match &items[index] {
                PaintEvidence::Glyphs(_) => {
                    let mut runs = Vec::new();
                    while let Some(PaintEvidence::Glyphs(run)) = items.get(index) {
                        runs.push(run);
                        index += 1;
                    }
                    self.text(&runs, fonts, page.index, &mut inlines)?;
                }
                PaintEvidence::Path(_) => {
                    let mut paths = Vec::new();
                    while let Some(PaintEvidence::Path(path)) = items.get(index) {
                        paths.push(path);
                        index += 1;
                    }
                    self.paths(&paths, page.index, &mut inlines)?;
                }
                PaintEvidence::Image(image) => {
                    index += 1;
                    self.image(image, page.index, &mut inlines)?;
                }
            }
        }
        let paragraph = Paragraph {
            id: self.id()?,
            properties: ParagraphProperties {
                spacing: Some(Spacing {
                    before_twips: Some(0),
                    after_twips: Some(0),
                    ..Spacing::default()
                }),
                ..ParagraphProperties::default()
            }
            .into(),
            inlines,
        };
        self.flush_pending(true);
        self.pending = Some((paragraph, section));
        Ok(())
    }

    /// Moves the waiting paragraph into the body; `ends_section` marks it as
    /// the end of its page's section (every page but the last).
    fn flush_pending(&mut self, ends_section: bool) {
        if let Some((mut paragraph, section)) = self.pending.take() {
            if ends_section {
                let mut properties = (*paragraph.properties).clone();
                properties.section_break = Some(section);
                paragraph.properties = properties.into();
            }
            self.body.push(BlockNode::Paragraph(paragraph));
        }
    }

    fn section(&mut self, page: &PageEvidence) -> Result<SectionId, PdfImportError> {
        let (width, height) = (twips(page.width), twips(page.height));
        let clamped = |v: i32| v.clamp(1, MAX_PAGE_TWIPS);
        if clamped(width) != width || clamped(height) != height {
            self.findings.record(
                report::PAGE_SIZE_CLAMPED,
                Outcome::Degraded,
                1,
                Some(page.index),
            );
        }
        let id = SectionId::new(self.id()?);
        self.sections.push(SectionBoundary {
            id,
            page_size: PageSize {
                width_twips: clamped(width),
                height_twips: clamped(height),
            },
            page_margins: PageMargins {
                top_twips: 0,
                bottom_twips: 0,
                start_twips: 0,
                end_twips: 0,
                header_twips: Some(0),
                footer_twips: Some(0),
                gutter_twips: None,
            },
            columns: SectionColumns {
                count: 1,
                space_twips: None,
                separator: None,
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
            orientation: None,
            paper_source: casual_doc_model::v1::PaperSource::default(),
            page_borders: casual_doc_model::v1::PageBorders::default(),
            line_numbering: casual_doc_model::v1::LineNumbering::default(),
            watermark: None,
            footnote_props: casual_doc_model::v1::NoteProperties::default(),
            endnote_props: casual_doc_model::v1::NoteProperties::default(),
            text_direction: None,
            bidi: false,
            section_change: None,
        });
        Ok(id)
    }

    /// Decides, once per font, whether its embedded program draws the text
    /// (`docs/182` G-1) or a substitute does, and reports which.
    ///
    /// A program is registered only when it is an sfnt whose cmap maps every
    /// character this page drew with it, and no earlier program took the same
    /// family and style: registering a subset that cannot shape its own text
    /// would paint `.notdef` boxes where a substitute paints the letters. A
    /// registered subset that a later page draws characters outside of is
    /// reported as partly substituted (those characters fall back).
    /// O(characters on the page).
    fn decide_fonts(&mut self, runs: &[&GlyphRunEvidence], fonts: &DocumentFonts, page: usize) {
        let mut used: BTreeMap<usize, BTreeSet<char>> = BTreeMap::new();
        for run in runs {
            let chars = used.entry(run.font).or_default();
            for glyph in &run.glyphs {
                if let Some(text) = &glyph.text {
                    chars.extend(
                        text.chars()
                            .filter(|c| !c.is_whitespace() && !c.is_control()),
                    );
                }
            }
        }
        for (index, chars) in used {
            let Some(font) = fonts.fonts.get(index) else {
                continue;
            };
            if let Some(embedded) = self.font_embedded.get(&index).copied() {
                if embedded
                    && !self.font_partial.contains(&index)
                    && let Some((_, bytes)) = &font.program
                    && !covers(bytes, &chars)
                {
                    self.font_partial.insert(index);
                    self.findings.record(
                        report::FONT_SUBSTITUTED,
                        Outcome::Degraded,
                        1,
                        Some(page),
                    );
                }
                continue;
            }
            let family = embedded_family(font);
            let style = match (font.bold, font.italic) {
                (true, true) => FontProgramStyle::BoldItalic,
                (true, false) => FontProgramStyle::Bold,
                (false, true) => FontProgramStyle::Italic,
                (false, false) => FontProgramStyle::Regular,
            };
            let candidate = match &font.program {
                Some((FontProgramKind::TrueType, bytes)) => {
                    Some((FontProgramFormat::TrueType, "ttf", bytes))
                }
                Some((FontProgramKind::OpenTypeCff, bytes)) => {
                    Some((FontProgramFormat::OpenTypeCff, "otf", bytes))
                }
                _ => None,
            };
            let slot_free = !self
                .programs
                .iter()
                .any(|p| p.family == family && p.style == style);
            match candidate {
                Some((format, extension, bytes))
                    if slot_free && !family.is_empty() && covers(bytes, &chars) =>
                {
                    let part_name =
                        format!("pdf/fonts/font{}.{extension}", self.programs.len() + 1);
                    self.resources.insert(part_name.clone(), bytes.clone());
                    let mut program =
                        EmbeddedFontProgram::new(family.clone(), style, format, part_name)
                            .with_subsetted(true);
                    if !font.name.is_empty() {
                        program = program.with_source_name(font.name.clone());
                    }
                    self.programs.push(program);
                    self.font_family.insert(index, family);
                    self.font_embedded.insert(index, true);
                    self.findings
                        .record(report::FONT_EMBEDDED, Outcome::Mapped, 1, Some(page));
                }
                _ => {
                    self.font_embedded.insert(index, false);
                    self.findings.record(
                        report::FONT_SUBSTITUTED,
                        Outcome::Degraded,
                        1,
                        Some(page),
                    );
                }
            }
        }
    }

    fn run_properties(&mut self, style: group::Style, fonts: &DocumentFonts) -> RunProperties {
        let font = fonts.fonts.get(style.font);
        let family = self
            .font_family
            .get(&style.font)
            .cloned()
            .or_else(|| font.map(substitute_family))
            .unwrap_or_else(|| "Arial".to_owned());
        let named = Some(FontRef::Named(FontName { name: family }));
        RunProperties {
            font_ref: named.clone(),
            font_ref_h_ansi: named.clone(),
            font_ref_cs: named.clone(),
            font_ref_east_asia: named,
            size_half_points: Some(style.half_points),
            size_complex_half_points: Some(style.half_points),
            bold: font.is_some_and(|f| f.bold).then_some(true),
            italic: font.is_some_and(|f| f.italic).then_some(true),
            color: style
                .fill
                .map(|[r, g, b, _]| Color::Rgb(RgbColor { r, g, b })),
            hidden: (style.mode == RenderMode::Invisible).then_some(true),
            ..RunProperties::default()
        }
    }

    fn text(
        &mut self,
        runs: &[&GlyphRunEvidence],
        fonts: &DocumentFonts,
        page: usize,
        inlines: &mut Vec<InlineNode>,
    ) -> Result<(), PdfImportError> {
        self.decide_fonts(runs, fonts, page);
        let mut upright = Vec::new();
        for run in runs {
            let unmapped = run.glyphs.iter().filter(|g| g.text.is_none()).count();
            self.findings.record(
                report::NATIVE_TEXT_INVALID,
                Outcome::Degraded,
                unmapped as u32,
                Some(page),
            );
            match run.mode {
                RenderMode::Invisible => {
                    self.findings
                        .record(report::INVISIBLE_TEXT, Outcome::Degraded, 1, Some(page));
                }
                RenderMode::Stroke => self.findings.record(
                    report::TEXT_RENDER_MODE_DEGRADED,
                    Outcome::Degraded,
                    1,
                    Some(page),
                ),
                RenderMode::Fill | RenderMode::FillStroke => {}
            }
            if run.rotation_degrees.abs() <= UPRIGHT_DEGREES {
                upright.push(*run);
            } else {
                // G-3: text boxes cannot rotate their text yet. The run gets
                // its own frame at its first glyph, upright, and is reported.
                self.findings
                    .record(report::TEXT_ROTATED, Outcome::Degraded, 1, Some(page));
                let mut flat = (*run).clone();
                let origin = flat.glyphs.first().map(|g| g.origin).unwrap_or_default();
                let mut x = origin.x;
                for glyph in &mut flat.glyphs {
                    glyph.origin = casual_doc_pdf_read::Point { x, y: origin.y };
                    x += glyph.advance;
                }
                flat.rotation_degrees = 0.0;
                for block in group::blocks(&[&flat], self.params) {
                    self.frame(&block, fonts, inlines)?;
                }
            }
        }
        let (blocks, comparisons) = group::blocks_counted(&upright, self.params);
        self.comparisons += comparisons;
        let multi_line = blocks.iter().filter(|b| b.lines.len() > 1).count();
        self.findings.record(
            report::PARAGRAPH_BOUNDARY_INFERRED,
            Outcome::Mapped,
            multi_line as u32,
            Some(page),
        );
        if blocks.len() > 1 {
            self.findings.record(
                report::READING_ORDER_INFERRED,
                Outcome::Mapped,
                1,
                Some(page),
            );
        }
        for block in &blocks {
            self.frame(block, fonts, inlines)?;
        }
        Ok(())
    }

    fn frame(
        &mut self,
        block: &Block,
        fonts: &DocumentFonts,
        inlines: &mut Vec<InlineNode>,
    ) -> Result<(), PdfImportError> {
        let first = &block.lines[0];
        let mut deltas: Vec<f64> = block
            .lines
            .windows(2)
            .map(|pair| pair[1].baseline - pair[0].baseline)
            .filter(|d| *d > 0.0)
            .collect();
        deltas.sort_by(f64::total_cmp);
        let pitch = deltas
            .get(deltas.len() / 2)
            .copied()
            .unwrap_or(first.size * 1.2)
            .max(first.size * 0.5);
        // Alignment from the line edges: shared left edge, else shared right
        // edge, else shared centre; a first line that alone starts further
        // right is a first-line indent.
        let lines = &block.lines;
        let rest_x0 = lines
            .iter()
            .skip(1)
            .map(|l| l.x0)
            .fold(f64::INFINITY, f64::min);
        let same = |values: &mut dyn Iterator<Item = f64>| {
            let v: Vec<f64> = values.collect();
            v.iter().all(|x| (x - v[0]).abs() < 1.0)
        };
        let mut indent = None;
        let alignment = if lines.len() < 2 || same(&mut lines.iter().skip(1).map(|l| l.x0)) {
            if lines.len() >= 2 && first.x0 - rest_x0 > 1.0 {
                indent = Some(twips(first.x0 - rest_x0));
            }
            None
        } else if same(&mut lines.iter().map(|l| l.x1)) {
            Some(Alignment::End)
        } else if same(&mut lines.iter().map(|l| (l.x0 + l.x1) / 2.0)) {
            Some(Alignment::Center)
        } else {
            None
        };
        let left = if indent.is_some() { rest_x0 } else { block.x0 };
        let width = (block.x1 - left) * (1.0 + FRAME_SLACK_FRACTION) + FRAME_SLACK_POINTS;
        let x = match alignment {
            // Widening a right- or centre-aligned frame must not move its text.
            Some(Alignment::End) => left - (width - (block.x1 - left)),
            Some(Alignment::Center) => left - (width - (block.x1 - left)) / 2.0,
            _ => left,
        };
        let top = first.baseline - first.size * ASCENT;
        let height = pitch * lines.len() as f64;

        let mut paragraph_inlines = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                paragraph_inlines.push(InlineNode::Break(Break {
                    id: self.id()?,
                    kind: BreakKind::Line,
                }));
            }
            let mut at = 0;
            while at < line.chars.len() {
                let style = line.chars[at].style;
                let mut text = String::new();
                while let Some(c) = line.chars.get(at)
                    && c.style == style
                {
                    text.push_str(&c.text);
                    at += 1;
                }
                if text.is_empty() {
                    continue;
                }
                let properties = self.run_properties(style, fonts);
                paragraph_inlines.push(InlineNode::Run(Run {
                    id: self.id()?,
                    properties: properties.into(),
                    text,
                }));
            }
        }
        let paragraph = Paragraph {
            id: self.id()?,
            properties: ParagraphProperties {
                alignment,
                indentation: indent.map(|first_line| Indentation {
                    first_line_twips: Some(first_line),
                    ..Indentation::default()
                }),
                spacing: Some(Spacing {
                    before_twips: Some(0),
                    after_twips: Some(0),
                    line_rule: Some(LineRule::Exact),
                    line_twips: Some(twips(pitch).max(1)),
                    ..Spacing::default()
                }),
                ..ParagraphProperties::default()
            }
            .into(),
            inlines: paragraph_inlines,
        };
        let z = self.next_z();
        inlines.push(InlineNode::TextBox(Box::new(TextBox {
            hyperlink: None,
            id: self.id()?,
            anchor: Some(page_anchor(x, top)),
            relative_height: Some(z),
            extent: Some(Extent {
                width_emu: emu(width).max(1),
                height_emu: emu(height).max(1),
            }),
            fill: None,
            border: None,
            body_properties: TextBoxBodyProperties {
                insets: TextBoxInsets {
                    left_emu: 0,
                    top_emu: 0,
                    right_emu: 0,
                    bottom_emu: 0,
                },
                auto_fit: TextBoxAutoFit::Shape,
                ..TextBoxBodyProperties::default()
            },
            blocks: vec![BlockNode::Paragraph(paragraph)],
        })));
        Ok(())
    }

    fn image(
        &mut self,
        image: &ImageEvidence,
        page: usize,
        inlines: &mut Vec<InlineNode>,
    ) -> Result<(), PdfImportError> {
        if image
            .codecs
            .iter()
            .any(|c| matches!(*c, "JBIG2" | "JPX" | "CCITTFax"))
        {
            self.findings
                .record(report::IMAGE_REENCODED, Outcome::Mapped, 1, Some(page));
        }
        let upright = image.rotation_degrees.abs() <= UPRIGHT_DEGREES;
        let (mut placed, mut crop) = (image.bounds, None);
        if let Some(clip) = image.clip {
            if upright && clip.width() > 0.0 && clip.height() > 0.0 {
                let b = image.bounds;
                let fraction = |part: f64, whole: f64| {
                    ((part / whole) * f64::from(casual_doc_model::v1::CROP_FULL)).round() as i32
                };
                let (mut top, mut bottom) = (
                    fraction(clip.y0 - b.y0, b.height()),
                    fraction(b.y1 - clip.y1, b.height()),
                );
                if image.flip_v {
                    std::mem::swap(&mut top, &mut bottom);
                }
                crop = Some(
                    CropRect {
                        left: fraction(clip.x0 - b.x0, b.width()),
                        top,
                        right: fraction(b.x1 - clip.x1, b.width()),
                        bottom,
                    }
                    .clamped(),
                );
                placed = clip;
            } else {
                self.findings
                    .record(report::IMAGE_CLIP_IGNORED, Outcome::Degraded, 1, Some(page));
            }
        }
        let n = self.media.len() + 1;
        let (extension, media_type) = match image.format {
            ImageFormat::Png => ("png", "image/png"),
            ImageFormat::Jpeg => ("jpeg", "image/jpeg"),
        };
        let part_name = format!("word/media/pdf-image{n}.{extension}");
        let media = MediaId::new(self.id()?);
        self.media.push((
            media,
            MediaReference {
                relationship_id: format!("rIdPdfImage{n}"),
                media_type: media_type.to_owned(),
                part_name: part_name.clone(),
            },
        ));
        self.resources.insert(part_name, image.bytes.clone());
        let z = self.next_z();
        let rotation = (!upright).then(|| (image.rotation_degrees * 60_000.0).round() as i32);
        inlines.push(InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
            id: self.id()?,
            media,
            extent: Extent {
                width_emu: emu(placed.width()).max(1),
                height_emu: emu(placed.height()).max(1),
            },
            anchor: page_anchor(placed.x0, placed.y0),
            descr: None,
            relative_height: Some(z),
            crop: crop.filter(|c| !c.is_identity()),
            opacity: None,
            hyperlink: None,
            border: None,
            flip_h: false,
            flip_v: image.flip_v,
            rotation,
        })));
        Ok(())
    }

    fn paths(
        &mut self,
        paths: &[&PathEvidence],
        page: usize,
        inlines: &mut Vec<InlineNode>,
    ) -> Result<(), PdfImportError> {
        let mut children = Vec::new();
        let (mut x0, mut y0, mut x1, mut y1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for path in paths {
            if path.clipped {
                self.findings
                    .record(report::CLIP_IGNORED, Outcome::Degraded, 1, Some(page));
            }
            let chunks = split(&path.commands);
            if chunks.len() > 1 {
                self.findings
                    .record(report::VECTOR_SPLIT, Outcome::Degraded, 1, Some(page));
            }
            for commands in chunks {
                let b = path_bounds(&commands);
                x0 = x0.min(b.0);
                y0 = y0.min(b.1);
                x1 = x1.max(b.2);
                y1 = y1.max(b.3);
                children.push(GroupChild::Shape(self.shape(path, &commands, b)?));
            }
        }
        if children.is_empty() {
            return Ok(());
        }
        let width = (x1 - x0).max(1.0);
        let height = (y1 - y0).max(1.0);
        let offset = PointEmu {
            x_emu: emu(x0),
            y_emu: emu(y0),
        };
        let extent = Extent {
            width_emu: emu(width),
            height_emu: emu(height),
        };
        let z = self.next_z();
        inlines.push(InlineNode::Group(Box::new(WordprocessingGroup {
            id: self.id()?,
            anchor: Some(page_anchor(x0, y0)),
            relative_height: Some(z),
            extent,
            // The anchor places a top-level group; its own `a:off` is relative
            // to that (Word writes 0,0), and the child space starts at the
            // group's page origin so children keep page-EMU coordinates.
            transform: GroupTransform {
                offset: PointEmu { x_emu: 0, y_emu: 0 },
                extent,
                child_offset: offset,
                child_extent: extent,
                flip_h: false,
                flip_v: false,
                rotation: None,
            },
            hyperlink: None,
            children,
        })));
        Ok(())
    }

    fn shape(
        &mut self,
        path: &PathEvidence,
        commands: &[PathCommand],
        bounds: (f64, f64, f64, f64),
    ) -> Result<GroupShape, PdfImportError> {
        let (bx, by) = (bounds.0, bounds.1);
        let point = |p: casual_doc_pdf_read::Point| GeometryPoint {
            x: GeometryValue::Literal(emu(p.x - bx)),
            y: GeometryValue::Literal(emu(p.y - by)),
        };
        let mut out = Vec::with_capacity(commands.len());
        for command in commands {
            out.push(match *command {
                PathCommand::MoveTo(p) => ShapePathCommand::MoveTo { point: point(p) },
                PathCommand::LineTo(p) => ShapePathCommand::LineTo { point: point(p) },
                PathCommand::QuadTo(c, p) => ShapePathCommand::QuadBezTo {
                    control: point(c),
                    point: point(p),
                },
                PathCommand::CubicTo(c1, c2, p) => ShapePathCommand::CubicBezTo {
                    control1: point(c1),
                    control2: point(c2),
                    point: point(p),
                },
                PathCommand::Close => ShapePathCommand::Close,
            });
        }
        let filled = path.fill.is_some();
        let mut shape_path = ShapePath::new(out);
        shape_path.fill = if filled {
            PathFill::Norm
        } else {
            PathFill::None
        };
        shape_path.stroke = path.stroke.is_some();
        let fill = path.fill.map(|color| {
            // A pattern fill (`Some(None)`) is flattened to mid grey, reported
            // by the reader's pattern count.
            Fill::Solid(rgba(color.unwrap_or([128, 128, 128, 255])))
        });
        let stroke = path.stroke.as_ref().map(|stroke| ShapeStroke {
            color: rgba(stroke.color.unwrap_or([128, 128, 128, 255])),
            width_emu: emu(stroke.width).max(0),
            dash: stroke.dashed.then_some(DashStyle::Dash),
            head_end: None,
            tail_end: None,
        });
        Ok(GroupShape {
            id: self.id()?,
            offset: PointEmu {
                x_emu: emu(bounds.0),
                y_emu: emu(bounds.1),
            },
            extent: Extent {
                width_emu: emu(bounds.2 - bounds.0).max(0),
                height_emu: emu(bounds.3 - bounds.1).max(0),
            },
            geometry: ShapeGeometry::Other,
            preset: None,
            adjustments: Vec::new(),
            path: Some(CustomGeometry::from_paths(vec![shape_path]).with_fill_rule(
                if path.even_odd {
                    FillRule::EvenOdd
                } else {
                    FillRule::NonZero
                },
            )),
            fill,
            stroke,
            flip_h: false,
            flip_v: false,
            rotation: None,
            hyperlink: None,
        })
    }

    /// Finishes the document. O(document) for the model's validation.
    pub(crate) fn finish(mut self) -> Result<Built, PdfImportError> {
        // The last page's section is the body-level trailing section, which no
        // paragraph references.
        self.flush_pending(false);
        let mut definitions = Definitions::default();
        for (id, reference) in std::mem::take(&mut self.media) {
            definitions.media.insert(id, reference);
        }
        definitions.sections = std::mem::take(&mut self.sections);
        definitions.font_programs = std::mem::take(&mut self.programs);
        let document_id = self.id()?;
        let document = Document::new(document_id, std::mem::take(&mut self.body), definitions)
            .map_err(|error| PdfImportError::Model(error.to_string()))?;
        Ok((document, self.resources, self.findings.finish()))
    }
}

/// Splits a path into chunks within the model's per-geometry command bound,
/// at subpath starts where possible; a single oversized subpath is cut and
/// continued with a `MoveTo` at the cut point (exact for strokes, approximate
/// for fills, and reported as a split).
fn split(commands: &[PathCommand]) -> Vec<Vec<PathCommand>> {
    let cap = MAX_SHAPE_PATH_COMMANDS;
    if commands.len() <= cap {
        return vec![commands.to_vec()];
    }
    let mut chunks: Vec<Vec<PathCommand>> = vec![Vec::new()];
    let mut last_point = None;
    for command in commands {
        let current = chunks.last_mut().expect("non-empty");
        if current.len() >= cap {
            let mut next = Vec::with_capacity(cap);
            if !matches!(command, PathCommand::MoveTo(_))
                && let Some(p) = last_point
            {
                next.push(PathCommand::MoveTo(p));
            }
            chunks.push(next);
        }
        let current = chunks.last_mut().expect("non-empty");
        if current.is_empty()
            && !matches!(command, PathCommand::MoveTo(_) | PathCommand::Close)
            && let Some(p) = last_point
        {
            current.push(PathCommand::MoveTo(p));
        }
        if !(current.is_empty() && matches!(command, PathCommand::Close)) {
            current.push(*command);
        }
        last_point = match *command {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => Some(p),
            PathCommand::QuadTo(_, p) | PathCommand::CubicTo(_, _, p) => Some(p),
            PathCommand::Close => last_point,
        };
    }
    chunks.retain(|chunk| matches!(chunk.first(), Some(PathCommand::MoveTo(_))));
    chunks
}

fn path_bounds(commands: &[PathCommand]) -> (f64, f64, f64, f64) {
    let mut b = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    let mut grow = |p: &casual_doc_pdf_read::Point| {
        b.0 = b.0.min(p.x);
        b.1 = b.1.min(p.y);
        b.2 = b.2.max(p.x);
        b.3 = b.3.max(p.y);
    };
    for command in commands {
        match command {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => grow(p),
            PathCommand::QuadTo(c, p) => {
                grow(c);
                grow(p);
            }
            PathCommand::CubicTo(c1, c2, p) => {
                grow(c1);
                grow(c2);
                grow(p);
            }
            PathCommand::Close => {}
        }
    }
    b
}
