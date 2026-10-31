//! `fixed::build`: page evidence -> a fixed-layout `v1::Document`
//! (`docs/182` §4.1).
//!
//! Every PDF page becomes one section (page size from the CropBox, rotation
//! applied, landscape when wide, zero margins) holding one anchor paragraph. That paragraph carries
//! the page's objects as page-anchored floats, in paint order, with a
//! monotonically increasing `relative_height` so z-order follows paint order:
//!
//! - each text block (`group.rs`) -> a `TextBox` with real paragraphs and runs
//!   (a ROTATED block -> a one-child group whose `GroupTextBox` is turned by
//!   the text's angle, `docs/182` G-3),
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
use casual_doc_model::v1::{
    Bookmark, BookmarkEnd, BookmarkId, BookmarkStart, Comment, CommentId, CommentRangeEnd,
    CommentRangeStart, CommentReference, ExternalTarget, Hyperlink, HyperlinkTarget,
    InternalTarget,
};
use casual_doc_model::v1::{EmbeddedFontProgram, FillRule, FontProgramFormat, FontProgramStyle};
// Own line: turned text frames and landscape pages (`docs/182` G-3).
use casual_doc_model::v1::{GroupTextBox, PageOrientation};
use casual_doc_model::{IdGenerator, NodeId};
use casual_doc_pdf_read::{
    ActiveContent, AnnotationEvidence, Bounds, DocumentFonts, FontEvidence, FontProgramKind,
    GlyphRunEvidence, ImageEvidence, ImageFormat, LinkTarget, PageEvidence, PaintEvidence,
    PathCommand, PathEvidence, RenderMode,
};

use crate::PdfImportError;
use crate::fonts;
use crate::group::{self, ASCENT, Block, LayoutParams};
use crate::links;
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

pub(crate) fn emu(points: f64) -> i64 {
    (points * EMU_PER_PT).round() as i64
}

fn twips(points: f64) -> i32 {
    (points * 20.0).round() as i32
}

/// `point` turned clockwise (y-down page space, the evidence's convention) by
/// `degrees` about `pivot`. O(1).
fn turn(
    point: casual_doc_pdf_read::Point,
    pivot: casual_doc_pdf_read::Point,
    degrees: f64,
) -> casual_doc_pdf_read::Point {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (dx, dy) = (point.x - pivot.x, point.y - pivot.y);
    casual_doc_pdf_read::Point {
        x: pivot.x + dx * cos - dy * sin,
        y: pivot.y + dx * sin + dy * cos,
    }
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
        allow_overlap: true,
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
    // A symbol font's codes were mapped to real characters by the reader
    // (`casual_doc_pdf_read::unicode`), which the symbol face does not draw:
    // a text face (and the shaper's fallback) does.
    if casual_doc_pdf_read::unicode::FontCodes::of(&font.name).symbolic() {
        return "Arial".to_owned();
    }
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

/// The family, bold and italic a run drawn with font `index` is given: the
/// registered program's family when there is one, else the substitute.
/// Shared with reflow so both outputs name the same faces. O(1).
pub(crate) fn run_font(
    index: usize,
    fonts: &DocumentFonts,
    registered: &BTreeMap<usize, String>,
) -> (String, bool, bool) {
    let font = fonts.fonts.get(index);
    let family = registered
        .get(&index)
        .cloned()
        .or_else(|| font.map(substitute_family))
        .unwrap_or_else(|| "Arial".to_owned());
    (
        family,
        font.is_some_and(|f| f.bold),
        font.is_some_and(|f| f.italic),
    )
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
    pub(crate) font_family: BTreeMap<usize, String>,
    /// Embedded fonts a later page drew characters outside of.
    font_partial: BTreeSet<usize>,
    /// Font index -> the resource part holding its registered program.
    font_part: BTreeMap<usize, String>,
    /// The programs registered (`Definitions::font_programs`).
    pub(crate) programs: Vec<EmbeddedFontProgram>,
    /// Every picture placed: page, bounds after the clip, its media
    /// reference (the reflow sidecar references these, never copies them).
    pub(crate) placed_images: Vec<(usize, Bounds, MediaReference)>,
    /// The previous page's anchor paragraph, waiting for the next page so it
    /// can carry the section break that ends its page.
    pending: Option<(Paragraph, SectionId)>,
    /// Line-against-block comparisons made by grouping (the work counter).
    pub(crate) comparisons: u64,
    /// This page's navigable links, and whether each covered any text.
    page_links: Vec<(Bounds, LinkTarget)>,
    page_links_used: Vec<bool>,
    /// In-document destinations, by bookmark: (bookmark, name, page, top).
    destinations: Vec<(BookmarkId, String, usize, Option<f64>)>,
    /// Comments from notes.
    comments: Vec<(CommentId, Comment)>,
    /// A note's object id -> its comment's thread id (`para_id`), so a reply
    /// (`/IRT`) threads onto it.
    comment_threads: BTreeMap<(i32, i32), String>,
    /// Where each page and frame came from (`docs/182` P4).
    pub(crate) sources: crate::SourceMap,
    /// The current page's centre: the pivot rotated text is turned about.
    centre: casual_doc_pdf_read::Point,
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
            font_part: BTreeMap::new(),
            programs: Vec::new(),
            placed_images: Vec::new(),
            pending: None,
            comparisons: 0,
            page_links: Vec::new(),
            page_links_used: Vec::new(),
            destinations: Vec::new(),
            comments: Vec::new(),
            comment_threads: BTreeMap::new(),
            sources: crate::SourceMap::default(),
            centre: casual_doc_pdf_read::Point { x: 0.0, y: 0.0 },
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
        annotations: &[AnnotationEvidence],
    ) -> Result<(), PdfImportError> {
        let section = self.section(page)?;
        self.sources.pages.push(crate::PageSource {
            object: page.object,
            shows: page.shows,
        });
        self.centre = casual_doc_pdf_read::Point {
            x: page.width / 2.0,
            y: page.height / 2.0,
        };
        let sorted = links::sort(annotations, &mut self.findings, page.index);
        self.page_links_used = vec![false; sorted.links.len()];
        self.page_links = sorted.links;
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
        // The reader made the text fit for editing (`casual_doc_pdf_read::
        // unicode`): reported, because the characters are not the PDF's.
        self.findings.record(
            report::LIGATURE_DECOMPOSED,
            Outcome::Mapped,
            notes.ligatures_decomposed,
            at,
        );
        self.findings.record(
            report::SYMBOL_FONT_MAPPED,
            Outcome::Mapped,
            notes.symbols_mapped,
            at,
        );
        self.findings.record(
            report::MARKS_COMPOSED,
            Outcome::Mapped,
            notes.marks_composed,
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
        let used = self.page_links_used.iter().filter(|u| **u).count() as u32;
        let unused = self.page_links_used.len() as u32 - used;
        self.findings.record(
            report::HYPERLINK_INFERRED,
            Outcome::Mapped,
            used,
            Some(page.index),
        );
        self.findings.record(
            report::LINK_UNRESOLVED,
            Outcome::Omitted,
            unused,
            Some(page.index),
        );
        self.page_links.clear();
        for note in sorted.notes {
            if let Some((size, colour)) = note.drawn {
                self.free_text(&note, size, colour, &mut inlines)?;
            }
            self.comment(note, &mut inlines, page.index)?;
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
            // A landscape page (a `/Rotate` 90/270 portrait sheet, or one
            // authored wide) says so, as Word's `w:orient` does: the size is
            // already the displayed one (`docs/182` G-3).
            orientation: (width > height).then_some(PageOrientation::Landscape),
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
        // The advance (in em) each single-character glyph was placed at, the
        // first time it was seen: the check that a wrapped program's glyph
        // names tell the truth (`fonts::advances_agree`).
        let mut advances: BTreeMap<usize, BTreeMap<char, f64>> = BTreeMap::new();
        // The glyph each single character was drawn with, for a CID-keyed
        // program's cmap (`fonts::wrap_cid_cff`); a character drawn with two
        // different glyphs makes the font's mapping untrustworthy.
        let mut drawn: BTreeMap<usize, BTreeMap<char, u16>> = BTreeMap::new();
        let mut ambiguous: BTreeSet<usize> = BTreeSet::new();
        for run in runs {
            let chars = used.entry(run.font).or_default();
            let placed = advances.entry(run.font).or_default();
            let glyph_of = drawn.entry(run.font).or_default();
            for glyph in &run.glyphs {
                if let (Some(text), Some(id)) = (&glyph.text, glyph.glyph) {
                    let mut it = text.chars();
                    if let (Some(c), None, Ok(id)) = (it.next(), it.next(), u16::try_from(id))
                        && !c.is_whitespace()
                        && *glyph_of.entry(c).or_insert(id) != id
                    {
                        ambiguous.insert(run.font);
                    }
                }
                if let Some(text) = &glyph.text {
                    chars.extend(
                        text.chars()
                            .filter(|c| !c.is_whitespace() && !c.is_control()),
                    );
                    let mut it = text.chars();
                    if let (Some(c), None) = (it.next(), it.next())
                        && !c.is_whitespace()
                        && run.size > 0.0
                    {
                        placed.entry(c).or_insert(glyph.advance / run.size);
                    }
                }
            }
        }
        for (index, chars) in used {
            let Some(font) = fonts.fonts.get(index) else {
                continue;
            };
            if let Some(embedded) = self.font_embedded.get(&index).copied() {
                // The registered program (a wrapped one, for a CFF or Type1
                // source), not the PDF's bytes, decides what it covers.
                if embedded
                    && !self.font_partial.contains(&index)
                    && let Some(bytes) = self
                        .font_part
                        .get(&index)
                        .and_then(|part| self.resources.get(part))
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
            // A bare CFF is wrapped and a Type1 program converted into an
            // `OTTO` font (`fonts.rs`); either is registered only when its
            // advances agree with where the PDF placed the characters.
            let converted = match &font.program {
                Some((FontProgramKind::BareCff, bytes)) => {
                    match fonts::wrap_bare_cff(bytes, &family, font.bold, font.italic) {
                        Err(fonts::WrapRefusal::CidKeyed) if !ambiguous.contains(&index) => {
                            Some(fonts::wrap_cid_cff(
                                bytes,
                                &family,
                                font.bold,
                                font.italic,
                                drawn.get(&index).unwrap_or(&BTreeMap::new()),
                            ))
                        }
                        other => Some(other),
                    }
                }
                Some((FontProgramKind::Other, bytes)) if fonts::is_type1(bytes) => {
                    Some(fonts::convert_type1(bytes, &family, font.bold, font.italic))
                }
                _ => None,
            }
            .and_then(Result::ok)
            .filter(|otf| {
                advances
                    .get(&index)
                    .is_none_or(|placed| fonts::advances_agree(otf, placed))
            });
            let candidate = match (&font.program, &converted) {
                (_, Some(otf)) => Some((FontProgramFormat::OpenTypeCff, "otf", otf)),
                (Some((FontProgramKind::TrueType, bytes)), _) => {
                    Some((FontProgramFormat::TrueType, "ttf", bytes))
                }
                (Some((FontProgramKind::OpenTypeCff, bytes)), _) => {
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
                    self.font_part.insert(index, part_name.clone());
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
        let (family, _, _) = run_font(style.font, fonts, &self.font_family);
        let named = Some(FontRef::Named(FontName { name: family }));
        RunProperties {
            font_ref: named.clone(),
            font_ref_h_ansi: named.clone(),
            font_ref_cs: named.clone(),
            font_ref_east_asia: named,
            size_half_points: Some(style.half_points),
            size_complex_half_points: Some(style.half_points),
            bold: font.is_some_and(|f| f.bold).then_some(true),
            italic: (style.oblique || font.is_some_and(|f| f.italic)).then_some(true),
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
        // Rotated runs, by angle (hundredths of a degree), so lines of one
        // turned paragraph — every line of a `/Rotate` page — group together.
        let mut turned: BTreeMap<i64, Vec<GlyphRunEvidence>> = BTreeMap::new();
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
                // G-3: a rotated run is turned back upright about the page's
                // centre, grouped like any text, and each block becomes a frame
                // turned by the run's angle — the text stays editable and
                // paints where the PDF drew it.
                self.findings
                    .record(report::TEXT_ROTATED, Outcome::Mapped, 1, Some(page));
                let key = (run.rotation_degrees * 100.0).round() as i64;
                turned.entry(key).or_default().push((*run).clone());
            }
        }
        for (key, mut runs) in turned {
            let degrees = key as f64 / 100.0;
            for run in &mut runs {
                for glyph in &mut run.glyphs {
                    glyph.origin = turn(glyph.origin, self.centre, -degrees);
                }
                run.rotation_degrees = 0.0;
            }
            let refs: Vec<&GlyphRunEvidence> = runs.iter().collect();
            let (blocks, comparisons) = group::blocks_counted(&refs, self.params);
            self.comparisons += comparisons;
            for block in &blocks {
                self.frame(block, fonts, inlines, Some(degrees))?;
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
            self.frame(block, fonts, inlines, None)?;
        }
        Ok(())
    }

    /// One text block as a frame. `turned` is the clockwise angle in degrees
    /// when the block was turned back upright about the page centre: the
    /// frame is then a one-child group whose `GroupTextBox` carries the angle
    /// (`a:xfrm@rot`), placed so that turning it about its own centre puts the
    /// text where the PDF drew it.
    fn frame(
        &mut self,
        block: &Block,
        fonts: &DocumentFonts,
        inlines: &mut Vec<InlineNode>,
        turned: Option<f64>,
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
            let link_of: Vec<Option<usize>> = line
                .chars
                .iter()
                .map(|c| links::link_at(&self.page_links, c.x0, c.x1, c.baseline, c.size))
                .collect();
            let mut at = 0;
            while at < line.chars.len() {
                let style = line.chars[at].style;
                let link = link_of[at];
                let mut text = String::new();
                while let Some(c) = line.chars.get(at)
                    && c.style == style
                    && link_of[at] == link
                {
                    // A soft hyphen the PDF drew is a hyphen on the page; in a
                    // frame whose lines are the PDF's it must stay visible.
                    if c.text.contains('\u{ad}') {
                        text.push_str(&c.text.replace('\u{ad}', "-"));
                    } else {
                        text.push_str(&c.text);
                    }
                    at += 1;
                }
                if text.is_empty() {
                    continue;
                }
                let properties = self.run_properties(style, fonts);
                let run = InlineNode::Run(Run {
                    id: self.id()?,
                    properties: properties.into(),
                    text,
                });
                match link {
                    Some(index) => {
                        self.page_links_used[index] = true;
                        let target = self.link_target(index)?;
                        match paragraph_inlines.last_mut() {
                            Some(InlineNode::Hyperlink(h)) if h.target == target => {
                                push_run(&mut h.inlines, run);
                            }
                            _ => {
                                paragraph_inlines.push(InlineNode::Hyperlink(Box::new(Hyperlink {
                                    id: self.id()?,
                                    history: matches!(target, HyperlinkTarget::External(_)),
                                    target,
                                    tooltip: None,
                                    inlines: vec![run],
                                })))
                            }
                        }
                    }
                    None => push_run(&mut paragraph_inlines, run),
                }
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
        let id = self.id()?;
        let chars = || block.lines.iter().flat_map(|line| &line.chars);
        let shows: BTreeSet<u32> = chars().filter_map(|c| c.show).collect();
        self.sources.frames.push(crate::FrameSource {
            page: self.sources.pages.len().saturating_sub(1),
            frame: id,
            shows: shows.into_iter().collect(),
            complete: chars().all(|c| c.show.is_some()),
        });
        let extent = Extent {
            width_emu: emu(width).max(1),
            height_emu: emu(height).max(1),
        };
        let body_properties = TextBoxBodyProperties {
            insets: TextBoxInsets {
                left_emu: 0,
                top_emu: 0,
                right_emu: 0,
                bottom_emu: 0,
            },
            auto_fit: TextBoxAutoFit::Shape,
            ..TextBoxBodyProperties::default()
        };
        let blocks = vec![BlockNode::Paragraph(paragraph)];
        let Some(degrees) = turned else {
            inlines.push(InlineNode::TextBox(Box::new(TextBox {
                hyperlink: None,
                id,
                anchor: Some(page_anchor(x, top)),
                relative_height: Some(z),
                extent: Some(extent),
                fill: None,
                border: None,
                body_properties,
                blocks,
                shadow: None,
            })));
            return Ok(());
        };
        // The frame's centre, turned back to where the PDF drew it; the
        // frame is placed upright around that point and turned about it.
        let centre = turn(
            casual_doc_pdf_read::Point {
                x: x + width / 2.0,
                y: top + height / 2.0,
            },
            self.centre,
            degrees,
        );
        let origin = PointEmu { x_emu: 0, y_emu: 0 };
        let child = GroupTextBox {
            id,
            offset: origin,
            extent,
            geometry: ShapeGeometry::Rectangle,
            preset: None,
            adjustments: Vec::new(),
            blocks,
            fill: None,
            border: None,
            body_properties,
            hyperlink: None,
            flip_h: false,
            flip_v: false,
            rotation: Some((degrees * 60_000.0).round() as i32),
            shadow: None,
        };
        inlines.push(InlineNode::Group(Box::new(WordprocessingGroup {
            id: self.id()?,
            anchor: Some(page_anchor(centre.x - width / 2.0, centre.y - height / 2.0)),
            relative_height: Some(z),
            extent,
            transform: GroupTransform {
                offset: origin,
                extent,
                child_offset: origin,
                child_extent: extent,
                flip_h: false,
                flip_v: false,
                rotation: None,
            },
            hyperlink: None,
            children: vec![GroupChild::TextBox(child)],
        })));
        Ok(())
    }

    /// The hyperlink target of this page's link `index`; an in-document
    /// destination gets a bookmark, placed at `finish`. O(1).
    fn link_target(&mut self, index: usize) -> Result<HyperlinkTarget, PdfImportError> {
        match self.page_links[index].1.clone() {
            LinkTarget::Uri(url) => Ok(HyperlinkTarget::External(ExternalTarget {
                url,
                anchor: None,
            })),
            LinkTarget::Page { page, top } => {
                let existing = self
                    .destinations
                    .iter()
                    .find(|d| d.2 == page && d.3 == top)
                    .map(|d| d.1.clone());
                let name = match existing {
                    Some(name) => name,
                    None => {
                        let name = format!("_PdfDest{}", self.destinations.len() + 1);
                        let id = BookmarkId::new(self.id()?);
                        self.destinations.push((id, name.clone(), page, top));
                        name
                    }
                };
                Ok(HyperlinkTarget::Internal(InternalTarget { anchor: name }))
            }
            LinkTarget::Unresolved | LinkTarget::Blocked => {
                unreachable!("links::sort keeps navigable links only")
            }
        }
    }

    /// Anchors a note's comment on the frame nearest it on this page: the
    /// frame's text becomes the comment's range. O(frames on the page).
    fn comment(
        &mut self,
        note: links::NoteAnnotation,
        inlines: &mut [InlineNode],
        page: usize,
    ) -> Result<(), PdfImportError> {
        let (cx, cy) = (
            (note.rect.x0 + note.rect.x1) / 2.0,
            (note.rect.y0 + note.rect.y1) / 2.0,
        );
        let nearest = inlines
            .iter_mut()
            .filter_map(|inline| match inline {
                InlineNode::TextBox(frame) => {
                    frame_bounds(frame).map(|b| (links::distance(&b, cx, cy), frame))
                }
                _ => None,
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, frame)) = nearest else {
            self.findings
                .record(report::ANNOTATION_OMITTED, Outcome::Omitted, 1, Some(page));
            return Ok(());
        };
        let Some(BlockNode::Paragraph(paragraph)) = frame.blocks.first_mut() else {
            self.findings
                .record(report::ANNOTATION_OMITTED, Outcome::Omitted, 1, Some(page));
            return Ok(());
        };
        let comment = CommentId::new(self.id()?);
        paragraph.inlines.insert(
            0,
            InlineNode::CommentRangeStart(CommentRangeStart {
                id: self.id()?,
                comment,
            }),
        );
        paragraph
            .inlines
            .push(InlineNode::CommentRangeEnd(CommentRangeEnd {
                id: self.id()?,
                comment,
            }));
        paragraph
            .inlines
            .push(InlineNode::CommentReference(CommentReference {
                id: self.id()?,
                comment,
            }));
        let mut body = Vec::new();
        for line in note.text.split('\n') {
            body.push(BlockNode::Paragraph(Paragraph {
                id: self.id()?,
                properties: ParagraphProperties::default().into(),
                inlines: vec![InlineNode::Run(Run {
                    id: self.id()?,
                    properties: RunProperties::default().into(),
                    text: line.to_owned(),
                })],
            }));
        }
        let author = note
            .author
            .map(|a| a.chars().take(255).collect::<String>())
            .filter(|a| !a.is_empty());
        let initials = author.as_ref().map(|a| {
            a.split_whitespace()
                .filter_map(|w| w.chars().next())
                .take(4)
                .collect::<String>()
        });
        // Thread ids are the `w14:paraId` tokens Word threads by: 8 hex
        // digits, non-zero, below 0x80000000.
        let para_id = format!("{:08X}", self.comments.len() + 1);
        let parent_para_id = note
            .reply_to
            .and_then(|parent| self.comment_threads.get(&parent).cloned());
        if let Some(object) = note.object {
            self.comment_threads.insert(object, para_id.clone());
        }
        self.comments.push((
            comment,
            Comment {
                blocks: body,
                author,
                initials: initials.filter(|i| !i.is_empty()),
                date: note.date,
                para_id: Some(para_id),
                parent_para_id,
                ..Comment::default()
            },
        ));
        self.findings
            .record(report::COMMENT_INFERRED, Outcome::Mapped, 1, Some(page));
        Ok(())
    }

    /// A FreeText note's visible text, drawn as a frame at its rectangle in
    /// its `/DA` size and colour (the annotation's appearance stream is not
    /// page content, so without this the text it shows would be lost from
    /// the page). Its comment then anchors on it. O(text).
    fn free_text(
        &mut self,
        note: &links::NoteAnnotation,
        size: f64,
        colour: [u8; 3],
        inlines: &mut Vec<InlineNode>,
    ) -> Result<(), PdfImportError> {
        let r = note.rect;
        let half_points = (size * 2.0).round().clamp(2.0, 3_276.0) as u32;
        let named = Some(FontRef::Named(FontName {
            name: "Arial".to_owned(),
        }));
        let mut paragraph_inlines = Vec::new();
        for (i, line) in note.text.split('\n').enumerate() {
            if i > 0 {
                paragraph_inlines.push(InlineNode::Break(Break {
                    id: self.id()?,
                    kind: BreakKind::Line,
                }));
            }
            if line.is_empty() {
                continue;
            }
            paragraph_inlines.push(InlineNode::Run(Run {
                id: self.id()?,
                properties: RunProperties {
                    font_ref: named.clone(),
                    font_ref_h_ansi: named.clone(),
                    font_ref_cs: named.clone(),
                    font_ref_east_asia: named.clone(),
                    size_half_points: Some(half_points),
                    size_complex_half_points: Some(half_points),
                    color: (colour != [0, 0, 0]).then(|| {
                        Color::Rgb(RgbColor {
                            r: colour[0],
                            g: colour[1],
                            b: colour[2],
                        })
                    }),
                    ..RunProperties::default()
                }
                .into(),
                text: line.to_owned(),
            }));
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
            inlines: paragraph_inlines,
        };
        let z = self.next_z();
        inlines.push(InlineNode::TextBox(Box::new(TextBox {
            hyperlink: None,
            id: self.id()?,
            anchor: Some(page_anchor(r.x0, r.y0)),
            relative_height: Some(z),
            extent: Some(Extent {
                width_emu: emu(r.width()).max(1),
                height_emu: emu(r.height()).max(1),
            }),
            fill: None,
            border: None,
            body_properties: TextBoxBodyProperties {
                insets: TextBoxInsets {
                    left_emu: 25_400,
                    top_emu: 12_700,
                    right_emu: 25_400,
                    bottom_emu: 0,
                },
                auto_fit: TextBoxAutoFit::Shape,
                ..TextBoxBodyProperties::default()
            },
            blocks: vec![BlockNode::Paragraph(paragraph)],
            shadow: None,
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
        self.resources
            .insert(part_name.clone(), image.bytes.clone());
        self.placed_images.push((
            page,
            placed,
            MediaReference {
                relationship_id: format!("rIdPdfImage{n}"),
                media_type: media_type.to_owned(),
                part_name,
            },
        ));
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
        let built = vector_children(paths, &mut || self.id())?;
        self.findings.record(
            report::CLIP_IGNORED,
            Outcome::Degraded,
            built.clipped,
            Some(page),
        );
        self.findings.record(
            report::VECTOR_SPLIT,
            Outcome::Degraded,
            built.split,
            Some(page),
        );
        let Some(bounds) = built.bounds else {
            return Ok(());
        };
        let z = self.next_z();
        let id = self.id()?;
        inlines.push(InlineNode::Group(Box::new(vector_group(
            id,
            built.children,
            bounds,
            page_anchor(bounds.0, bounds.1),
            z,
        ))));
        Ok(())
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
        definitions.font_programs = self.programs.clone();
        for (id, comment) in std::mem::take(&mut self.comments) {
            definitions.comments.insert(id, comment);
        }
        let destinations = std::mem::take(&mut self.destinations);
        let placed = destinations.len() as u32;
        for (bookmark, name, page, top) in destinations {
            definitions.bookmarks.insert(bookmark, Bookmark { name });
            let start = InlineNode::BookmarkStart(BookmarkStart {
                id: self.id()?,
                bookmark,
            });
            let end = InlineNode::BookmarkEnd(BookmarkEnd {
                id: self.id()?,
                bookmark,
            });
            place_bookmark(self.body.get_mut(page), top, start, end);
        }
        self.findings
            .record(report::BOOKMARK_INFERRED, Outcome::Mapped, placed, None);
        let document_id = self.id()?;
        let document = Document::new(document_id, std::mem::take(&mut self.body), definitions)
            .map_err(|error| PdfImportError::Model(error.to_string()))?;
        Ok((document, self.resources, self.findings.finish()))
    }
}

/// The page-space bounds of a page-anchored frame, in points.
fn frame_bounds(frame: &TextBox) -> Option<Bounds> {
    let anchor = frame.anchor.as_ref()?;
    let (HorizontalPosition::Offset(x), VerticalPosition::Offset(y)) =
        (&anchor.horizontal.position, &anchor.vertical.position)
    else {
        return None;
    };
    let extent = frame.extent?;
    let pt = |emu: i64| emu as f64 / EMU_PER_PT;
    Some(Bounds {
        x0: pt(*x),
        y0: pt(*y),
        x1: pt(*x + extent.width_emu),
        y1: pt(*y + extent.height_emu),
    })
}

/// Puts a bookmark at the start of the frame on the page nearest at or below
/// `top` (the first frame when the destination states no top), else at the
/// start of the page's anchor paragraph. O(frames on the page).
fn place_bookmark(
    page: Option<&mut BlockNode>,
    top: Option<f64>,
    start: InlineNode,
    end: InlineNode,
) {
    let Some(BlockNode::Paragraph(anchor)) = page else {
        return;
    };
    let target = anchor
        .inlines
        .iter_mut()
        .filter_map(|inline| match inline {
            InlineNode::TextBox(frame) => frame_bounds(frame).map(|b| (b.y0, b.y1, frame)),
            _ => None,
        })
        .filter(|(_, bottom, _)| top.is_none_or(|t| *bottom >= t))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    match target.and_then(|(_, _, frame)| match frame.blocks.first_mut() {
        Some(BlockNode::Paragraph(p)) => Some(p),
        _ => None,
    }) {
        Some(paragraph) => {
            paragraph.inlines.insert(0, end);
            paragraph.inlines.insert(0, start);
        }
        None => {
            anchor.inlines.insert(0, end);
            anchor.inlines.insert(0, start);
        }
    }
}

/// The shapes of a run of paths (`fixed::build`'s vector art, and reflow's),
/// in page-EMU child space, with what drawing them cost.
#[derive(Debug)]
pub(crate) struct VectorChildren {
    /// One shape per path, or per chunk of a path over the model's command
    /// bound, in paint order.
    pub(crate) children: Vec<GroupChild>,
    /// The union of the shapes' bounds `(x0, y0, x1, y1)` in points; `None`
    /// when there are no shapes.
    pub(crate) bounds: Option<(f64, f64, f64, f64)>,
    /// Paths an active clip cut into (painted unclipped: `clip_ignored`).
    pub(crate) clipped: u32,
    /// Paths split into several shapes (`vector_split`).
    pub(crate) split: u32,
}

/// The shapes of `paths`, each taking its id from `id` in paint order.
/// O(path commands).
pub(crate) fn vector_children(
    paths: &[&PathEvidence],
    id: &mut dyn FnMut() -> Result<NodeId, PdfImportError>,
) -> Result<VectorChildren, PdfImportError> {
    let mut out = VectorChildren {
        children: Vec::new(),
        bounds: None,
        clipped: 0,
        split: 0,
    };
    let (mut x0, mut y0, mut x1, mut y1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for path in paths {
        if path.clipped {
            out.clipped += 1;
        }
        let chunks = split(&path.commands);
        if chunks.len() > 1 {
            out.split += 1;
        }
        for commands in chunks {
            let b = path_bounds(&commands);
            x0 = x0.min(b.0);
            y0 = y0.min(b.1);
            x1 = x1.max(b.2);
            y1 = y1.max(b.3);
            let shape_id = id()?;
            out.children
                .push(GroupChild::Shape(shape(path, &commands, b, shape_id)));
        }
    }
    if !out.children.is_empty() {
        out.bounds = Some((x0, y0, x1, y1));
    }
    Ok(out)
}

/// A top-level group of `children` (page-EMU child space) at `bounds`,
/// placed by `anchor`. The anchor places the group; its own `a:off` is
/// relative to that (Word writes 0,0), and the child space starts at the
/// group's page origin so children keep page-EMU coordinates. O(1).
pub(crate) fn vector_group(
    id: NodeId,
    children: Vec<GroupChild>,
    bounds: (f64, f64, f64, f64),
    anchor: DrawingAnchor,
    z: u32,
) -> WordprocessingGroup {
    let width = (bounds.2 - bounds.0).max(1.0);
    let height = (bounds.3 - bounds.1).max(1.0);
    let offset = PointEmu {
        x_emu: emu(bounds.0),
        y_emu: emu(bounds.1),
    };
    let extent = Extent {
        width_emu: emu(width),
        height_emu: emu(height),
    };
    WordprocessingGroup {
        id,
        anchor: Some(anchor),
        relative_height: Some(z),
        extent,
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
    }
}

/// One path chunk as a custom-geometry shape at `bounds`. O(commands).
fn shape(
    path: &PathEvidence,
    commands: &[PathCommand],
    bounds: (f64, f64, f64, f64),
    id: NodeId,
) -> GroupShape {
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
    GroupShape {
        id,
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
        path: Some(
            CustomGeometry::from_paths(vec![shape_path]).with_fill_rule(if path.even_odd {
                FillRule::EvenOdd
            } else {
                FillRule::NonZero
            }),
        ),
        fill,
        stroke,
        flip_h: false,
        flip_v: false,
        rotation: None,
        hyperlink: None,
        shadow: None,
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

/// Appends `run`, merging it into the previous inline when that is a run with
/// equal properties: two PDF glyph styles can map to the same `RunProperties`
/// (two font programs substituted by one face, say), and the model forbids
/// adjacent equivalent runs (`AdjacentEquivalentTextRuns`). The merged run
/// keeps the first run's id. O(1).
fn push_run(inlines: &mut Vec<InlineNode>, run: InlineNode) {
    if let (Some(InlineNode::Run(previous)), InlineNode::Run(next)) = (inlines.last_mut(), &run)
        && previous.properties == next.properties
    {
        previous.text.push_str(&next.text);
        return;
    }
    inlines.push(run);
}

#[cfg(test)]
mod push_run_tests {
    use super::*;

    fn run(id: u128, text: &str, bold: bool) -> InlineNode {
        InlineNode::Run(Run {
            id: NodeId::new(id).expect("id"),
            properties: RunProperties {
                bold: bold.then_some(true),
                ..RunProperties::default()
            }
            .into(),
            text: text.to_owned(),
        })
    }

    /// Two styles that resolve to the same properties become one run, and a
    /// different run after them stays separate. Mutation: make `push_run` a bare
    /// `inlines.push(run)` and the first assertion sees two runs.
    #[test]
    fn equal_properties_merge_into_one_run_and_different_ones_do_not() {
        let mut inlines = Vec::new();
        push_run(&mut inlines, run(1, "Hello ", false));
        push_run(&mut inlines, run(2, "world", false));
        assert_eq!(inlines.len(), 1, "{inlines:?}");
        let InlineNode::Run(merged) = &inlines[0] else {
            panic!("a run: {inlines:?}")
        };
        assert_eq!(merged.text, "Hello world");
        assert_eq!(merged.id, NodeId::new(1).expect("id"));
        push_run(&mut inlines, run(3, "!", true));
        assert_eq!(inlines.len(), 2, "{inlines:?}");
    }
}
