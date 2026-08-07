//! Deterministic bounded ODF 1.4 writing for the implemented ODT subset.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Alignment, AnchoredDrawing, BlockNode, BlockSdt, BookmarkId, BreakKind, CellMargins,
    CellVerticalAlignment, Color, Comment, CommentId, CommentReference, DefinitionMap, Definitions,
    Document, DocumentDefaults, DrawingAnchor, Extent, Field, FieldKind, Fill, FontRef,
    FormFieldKind, GroupChild, GroupShape, HeaderFooterKind, HeightRule, HorizontalAnchor,
    HorizontalPosition, HyperlinkTarget, Indentation, InlineNode, LevelJustification, LevelSuffix,
    MediaId, Note, NoteId, NoteKind, NoteReference, NumberFormat, NumberingInstanceId, Paragraph,
    ParagraphProperties, PointEmu, Revision, RevisionKind, RowHeight, RunProperties,
    SdtControlKind, ShapeGeometry, Spacing, Style, StyleId, StyleKind, Table, TableCell,
    TableCellProperties, TableRow, TableRowProperties, TableWidth, TextBox, VerticalAlignment,
    VerticalAnchor, VerticalMerge, VerticalPosition, WidthType, WordprocessingGroup, WrapMode,
};
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

use crate::{
    CompatibilityEntry, CompatibilityReport, MANIFEST_PART, META_PART, MIMETYPE_PART, ModelOutcome,
    ODT_MIME, OdfError, RetentionOutcome,
};

const CONTENT_HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" office:version="1.4">"#;
/// Content header for the preserving writer, which additionally emits
/// `draw:frame`/`draw:image` and therefore declares the drawing/svg/xlink
/// namespaces. Kept separate so plain semantic output stays byte-identical.
const CONTENT_HEADER_PRESERVING: &str = r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" office:version="1.4">"#;
const BODY_PREFIX: &str = "<office:body><office:text>";
const CONTENT_SUFFIX: &str = "</office:text></office:body></office:document-content>";

/// Resource limits for deterministic ODT semantic export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OdfExportLimits {
    /// Maximum generated `content.xml` bytes.
    pub max_content_bytes: usize,
    /// Maximum final ODT package bytes.
    pub max_package_bytes: usize,
    /// Maximum visited body blocks.
    pub max_blocks: usize,
    /// Maximum visited inline nodes.
    pub max_inline_nodes: usize,
    /// Maximum visited table rows.
    pub max_table_rows: usize,
    /// Maximum visited table cells.
    pub max_table_cells: usize,
    /// Maximum table grid columns.
    pub max_table_columns: usize,
    /// Maximum emitted footnote and endnote occurrences.
    pub max_notes: usize,
    /// Maximum nested transparent-wrapper depth.
    pub max_recursion_depth: usize,
    /// Maximum aggregate source text bytes projected into XML.
    pub max_text_bytes: usize,
    /// Maximum distinct compatibility feature buckets before overflow folding.
    pub max_report_features: usize,
}

impl OdfExportLimits {
    /// Compiled maximum content XML bytes.
    pub const HARD_MAX_CONTENT_BYTES: usize = 512 * 1024 * 1024;
    /// Compiled maximum final package bytes.
    pub const HARD_MAX_PACKAGE_BYTES: usize = 1024 * 1024 * 1024;
    /// Compiled maximum visited blocks.
    pub const HARD_MAX_BLOCKS: usize = 4_000_000;
    /// Compiled maximum visited inline nodes.
    pub const HARD_MAX_INLINE_NODES: usize = 16_000_000;
    /// Compiled maximum visited table rows.
    pub const HARD_MAX_TABLE_ROWS: usize = 2_000_000;
    /// Compiled maximum visited table cells.
    pub const HARD_MAX_TABLE_CELLS: usize = 16_000_000;
    /// Compiled maximum table grid columns, aligned with the import profile.
    pub const HARD_MAX_TABLE_COLUMNS: usize = 16_384;
    /// Compiled maximum emitted footnote and endnote occurrences.
    pub const HARD_MAX_NOTES: usize = 2_000_000;
    /// Compiled maximum wrapper depth.
    pub const HARD_MAX_RECURSION_DEPTH: usize = 256;
    /// Compiled maximum projected text bytes.
    pub const HARD_MAX_TEXT_BYTES: usize = 512 * 1024 * 1024;
    /// Compiled maximum report feature buckets.
    pub const HARD_MAX_REPORT_FEATURES: usize = 16_384;

    /// Validates configured limits against compiled safety ceilings.
    pub fn validate(self) -> Result<(), OdfError> {
        for (limit, value, hard_ceiling) in [
            (
                "odt_export_content_bytes",
                self.max_content_bytes,
                Self::HARD_MAX_CONTENT_BYTES,
            ),
            (
                "odt_export_package_bytes",
                self.max_package_bytes,
                Self::HARD_MAX_PACKAGE_BYTES,
            ),
            ("odt_export_blocks", self.max_blocks, Self::HARD_MAX_BLOCKS),
            (
                "odt_export_inline_nodes",
                self.max_inline_nodes,
                Self::HARD_MAX_INLINE_NODES,
            ),
            (
                "odt_export_table_rows",
                self.max_table_rows,
                Self::HARD_MAX_TABLE_ROWS,
            ),
            (
                "odt_export_table_cells",
                self.max_table_cells,
                Self::HARD_MAX_TABLE_CELLS,
            ),
            (
                "odt_export_table_columns",
                self.max_table_columns,
                Self::HARD_MAX_TABLE_COLUMNS,
            ),
            ("odt_export_notes", self.max_notes, Self::HARD_MAX_NOTES),
            (
                "odt_export_recursion_depth",
                self.max_recursion_depth,
                Self::HARD_MAX_RECURSION_DEPTH,
            ),
            (
                "odt_export_text_bytes",
                self.max_text_bytes,
                Self::HARD_MAX_TEXT_BYTES,
            ),
            (
                "odt_export_report_features",
                self.max_report_features,
                Self::HARD_MAX_REPORT_FEATURES,
            ),
        ] {
            if value > hard_ceiling {
                return Err(OdfError::InvalidLimitConfiguration {
                    limit,
                    value,
                    hard_ceiling,
                });
            }
        }
        Ok(())
    }
}

impl Default for OdfExportLimits {
    fn default() -> Self {
        Self {
            max_content_bytes: 128 * 1024 * 1024,
            max_package_bytes: 256 * 1024 * 1024,
            max_blocks: 500_000,
            max_inline_nodes: 4_000_000,
            max_table_rows: 1_000_000,
            max_table_cells: 4_000_000,
            max_table_columns: Self::HARD_MAX_TABLE_COLUMNS,
            max_notes: 250_000,
            max_recursion_depth: 64,
            max_text_bytes: 128 * 1024 * 1024,
            max_report_features: 4_096,
        }
    }
}

/// Successful deterministic semantic ODT export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OdtExport {
    /// Complete ODF 1.4 package bytes.
    pub bytes: Vec<u8>,
    /// Explicit findings for model semantics not fully represented.
    pub report: CompatibilityReport,
}

#[derive(Debug)]
struct Reporter {
    counts: BTreeMap<(String, ModelOutcome), u32>,
    overflow: u32,
    max_features: usize,
}

impl Reporter {
    fn new(max_features: usize) -> Self {
        Self {
            counts: BTreeMap::new(),
            overflow: 0,
            max_features,
        }
    }

    fn record(&mut self, feature: &'static str, outcome: ModelOutcome) {
        let key = (feature.to_owned(), outcome);
        if let Some(count) = self.counts.get_mut(&key) {
            *count = count.saturating_add(1);
        } else if self.counts.len() < self.max_features {
            self.counts.insert(key, 1);
        } else {
            self.overflow = self.overflow.saturating_add(1);
        }
    }

    fn finish(self) -> CompatibilityReport {
        let mut entries = self
            .counts
            .into_iter()
            .map(
                |((feature, model_outcome), occurrences)| CompatibilityEntry {
                    feature,
                    occurrences,
                    model_outcome,
                    retention_outcome: RetentionOutcome::NotRetained,
                },
            )
            .collect::<Vec<_>>();
        if self.overflow != 0 {
            entries.push(CompatibilityEntry {
                feature: "odt.export.report.overflow".to_owned(),
                occurrences: self.overflow,
                model_outcome: ModelOutcome::Omitted,
                retention_outcome: RetentionOutcome::NotRetained,
            });
        }
        entries.sort_by(|left, right| {
            left.feature
                .cmp(&right.feature)
                .then_with(|| left.model_outcome.cmp(&right.model_outcome))
        });
        CompatibilityReport { entries }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum OdtParagraphAlignment {
    Start,
    End,
    Center,
    Justify,
}

impl OdtParagraphAlignment {
    const fn name(self) -> &'static str {
        match self {
            Self::Start => "P_start",
            Self::End => "P_end",
            Self::Center => "P_center",
            Self::Justify => "P_justify",
        }
    }

    const fn value(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::Justify => "justify",
        }
    }
}

impl From<Alignment> for OdtParagraphAlignment {
    fn from(value: Alignment) -> Self {
        match value {
            Alignment::Start => Self::Start,
            Alignment::End => Self::End,
            Alignment::Center => Self::Center,
            Alignment::Justify => Self::Justify,
        }
    }
}

/// The supported paragraph-formatting subset, emitted as one deterministic
/// automatic `style:style`. Alignment keeps its historical bare name (`P_center`)
/// so alignment-only output is byte-identical to before; any other property
/// appends a deterministic hash suffix.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct OdtParagraphStyle {
    alignment: Option<OdtParagraphAlignment>,
    margin_left_twips: Option<i32>,
    margin_right_twips: Option<i32>,
    text_indent_twips: Option<i32>,
    margin_top_twips: Option<i32>,
    margin_bottom_twips: Option<i32>,
    line_percent: Option<u16>,
    keep_next: bool,
    keep_together: bool,
    break_before: bool,
}

impl OdtParagraphStyle {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    fn name(&self) -> String {
        let base = match self.alignment {
            Some(alignment) => alignment.name().to_owned(),
            None => "P".to_owned(),
        };
        let mut non_alignment = self.clone();
        non_alignment.alignment = None;
        if non_alignment.is_empty() {
            return base;
        }
        format!("{base}_{:016x}", font_family_hash(&format!("{self:?}")))
    }
}

/// The automatic paragraph-style base names minted by [`OdtParagraphStyle::name`]
/// (the no-alignment `P` plus each `OdtParagraphAlignment::name`).
const AUTOMATIC_PARAGRAPH_STYLE_BASES: [&str; 5] =
    ["P", "P_start", "P_end", "P_center", "P_justify"];

/// Whether `name` is a name that [`OdtParagraphStyle::name`] could mint for an
/// automatic paragraph style: one of the bases, optionally followed by the
/// `_{:016x}` lowercase-hex suffix it appends for non-alignment properties. Such a
/// name is reserved (a named paragraph style reusing it would collide with a minted
/// automatic style in the paragraph family's shared `style:name` space), mirroring
/// the `T_` guard for character styles. Precise rather than a blanket `P` prefix so
/// ordinary names like "Preformatted" are not needlessly re-minted.
fn is_automatic_paragraph_style_name(name: &str) -> bool {
    AUTOMATIC_PARAGRAPH_STYLE_BASES.iter().any(|base| {
        name == *base
            || name
                .strip_prefix(base)
                .and_then(|rest| rest.strip_prefix('_'))
                .is_some_and(|hex| {
                    hex.len() == 16
                        && hex
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                })
    })
}

/// Assigns each named style of `kind` the `style:name` it will carry in styles.xml
/// and be referenced by. The model's retained name is reused verbatim when it is a
/// valid NCName (the ODT round-trip case, where the name is the original
/// `style:name`) that is neither already taken nor `is_reserved` (colliding with an
/// automatic-style namespace); otherwise a deterministic `{mint_prefix}{n}` name is
/// minted. Order is by `StyleId`, so both the styles.xml emission and the body
/// references stay deterministic.
fn assign_named_style_names(
    definitions: &Definitions,
    kind: StyleKind,
    is_reserved: impl Fn(&str) -> bool,
    mint_prefix: &str,
) -> BTreeMap<StyleId, String> {
    let mut used = BTreeSet::new();
    let mut minted = 0usize;
    let mut assigned = BTreeMap::new();
    for (id, style) in definitions.styles.iter() {
        if style.kind != kind {
            continue;
        }
        let candidate = style
            .name
            .as_deref()
            .filter(|name| is_ncname(name) && !is_reserved(name) && !used.contains(*name));
        let name = match candidate {
            Some(name) => name.to_owned(),
            None => loop {
                minted += 1;
                let name = format!("{mint_prefix}{minted}");
                if !used.contains(&name) {
                    break name;
                }
            },
        };
        used.insert(name.clone());
        assigned.insert(*id, name);
    }
    assigned
}

/// Emits the supported `<style:paragraph-properties>` attributes for a paragraph
/// style. Lengths use the exactly-reversible `pt` form.
fn push_paragraph_properties(
    xml: &mut String,
    style: &OdtParagraphStyle,
    max_content_bytes: usize,
) -> Result<(), OdfError> {
    if let Some(alignment) = style.alignment {
        push_bounded(xml, " fo:text-align=\"", max_content_bytes)?;
        push_bounded(xml, alignment.value(), max_content_bytes)?;
        push_bounded(xml, "\"", max_content_bytes)?;
    }
    for (attr, twips) in [
        ("fo:margin-left", style.margin_left_twips),
        ("fo:margin-right", style.margin_right_twips),
        ("fo:text-indent", style.text_indent_twips),
        ("fo:margin-top", style.margin_top_twips),
        ("fo:margin-bottom", style.margin_bottom_twips),
    ] {
        if let Some(twips) = twips {
            push_bounded(xml, " ", max_content_bytes)?;
            push_bounded(xml, attr, max_content_bytes)?;
            push_bounded(xml, "=\"", max_content_bytes)?;
            push_bounded(xml, &twips_to_pt(twips), max_content_bytes)?;
            push_bounded(xml, "\"", max_content_bytes)?;
        }
    }
    if let Some(percent) = style.line_percent {
        push_bounded(xml, " fo:line-height=\"", max_content_bytes)?;
        push_bounded(xml, &percent.to_string(), max_content_bytes)?;
        push_bounded(xml, "%\"", max_content_bytes)?;
    }
    if style.keep_next {
        push_bounded(xml, " fo:keep-with-next=\"always\"", max_content_bytes)?;
    }
    if style.keep_together {
        push_bounded(xml, " fo:keep-together=\"always\"", max_content_bytes)?;
    }
    if style.break_before {
        push_bounded(xml, " fo:break-before=\"page\"", max_content_bytes)?;
    }
    Ok(())
}

/// The supported table-cell-formatting subset, emitted as one deterministic
/// automatic `table-cell` style referenced by a cell's `table:style-name`.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct OdtCellStyle {
    fill: Option<(u8, u8, u8)>,
    vertical_align: Option<OdtCellVAlign>,
    borders: OdtCellBorders,
    margins: OdtCellMargins,
}

/// The four physical cell-border edges (ODF has no cell inside-H/V borders).
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct OdtCellBorders {
    top: Option<OdtBorderEdge>,
    left: Option<OdtBorderEdge>,
    bottom: Option<OdtBorderEdge>,
    right: Option<OdtBorderEdge>,
}

/// The four physical cell content-padding edges (`fo:padding-*`), in twips.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct OdtCellMargins {
    top: Option<i32>,
    left: Option<i32>,
    bottom: Option<i32>,
    right: Option<i32>,
}

impl OdtCellMargins {
    fn from_model(margins: &CellMargins) -> Self {
        Self {
            top: margins.top_twips,
            left: margins.start_twips,
            bottom: margins.bottom_twips,
            right: margins.end_twips,
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct OdtBorderEdge {
    size_eighth_points: Option<u32>,
    style: String,
    color: Option<(u8, u8, u8)>,
}

impl OdtBorderEdge {
    /// The ODF `fo:border` compound value: `<width> <style> <color>`, omitting
    /// absent components. Exactly re-parsed by the importer's `parse_fo_border`.
    fn value(&self) -> String {
        let mut parts = Vec::new();
        if let Some(size) = self.size_eighth_points {
            parts.push(eighth_points_to_pt(size));
        }
        parts.push(self.style.clone());
        if let Some((red, green, blue)) = self.color {
            parts.push(format!("#{red:02x}{green:02x}{blue:02x}"));
        }
        parts.join(" ")
    }
}

impl From<&casual_doc_model::v1::BorderEdge> for OdtBorderEdge {
    fn from(edge: &casual_doc_model::v1::BorderEdge) -> Self {
        Self {
            size_eighth_points: edge.size_eighth_points,
            style: edge.style.clone(),
            color: edge.color.map(|color| (color.r, color.g, color.b)),
        }
    }
}

/// A cell border edge is representable as `fo:border` when it carries no text
/// padding. Zero padding is the ODF default (no `fo:padding` needed), so
/// `space_points` of `None` or `Some(0)` is representable — Word writes
/// `w:space="0"` on essentially every edge, so requiring `None` would drop the
/// borders of most Word-authored cells. Only genuine padding (`>= 1`) is left in
/// the model remainder to be reported rather than silently lost.
fn take_representable_border(
    edge: &mut Option<casual_doc_model::v1::BorderEdge>,
) -> Option<OdtBorderEdge> {
    if edge
        .as_ref()
        .is_some_and(|edge| edge.space_points.unwrap_or(0) == 0)
    {
        edge.take().as_ref().map(OdtBorderEdge::from)
    } else {
        None
    }
}

/// 1 eighth-point = 0.125pt; emits the minimal exact decimal `pt` string.
fn eighth_points_to_pt(eighths: u32) -> String {
    let thousandths = eighths * 125;
    let whole = thousandths / 1000;
    let frac = thousandths % 1000;
    if frac == 0 {
        format!("{whole}pt")
    } else {
        let mut digits = format!("{frac:03}");
        while digits.ends_with('0') {
            digits.pop();
        }
        format!("{whole}.{digits}pt")
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum OdtCellVAlign {
    Top,
    Middle,
    Bottom,
}

impl OdtCellVAlign {
    const fn value(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Middle => "middle",
            Self::Bottom => "bottom",
        }
    }
}

impl From<CellVerticalAlignment> for OdtCellVAlign {
    fn from(value: CellVerticalAlignment) -> Self {
        match value {
            CellVerticalAlignment::Top => Self::Top,
            CellVerticalAlignment::Center => Self::Middle,
            CellVerticalAlignment::Bottom => Self::Bottom,
        }
    }
}

impl OdtCellStyle {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    fn name(&self) -> String {
        let mut name = String::from("ce");
        if let Some((red, green, blue)) = self.fill {
            name.push_str(&format!("_c{red:02x}{green:02x}{blue:02x}"));
        }
        if let Some(valign) = self.vertical_align {
            name.push_str(match valign {
                OdtCellVAlign::Top => "_vt",
                OdtCellVAlign::Middle => "_vm",
                OdtCellVAlign::Bottom => "_vb",
            });
        }
        if self.borders != OdtCellBorders::default() {
            // Borders are compound strings, so identify them by a deterministic
            // hash rather than an unwieldy encoding.
            name.push_str(&format!(
                "_b{:016x}",
                font_family_hash(&format!("{:?}", self.borders))
            ));
        }
        if self.margins != OdtCellMargins::default() {
            name.push_str(&format!(
                "_p{:016x}",
                font_family_hash(&format!("{:?}", self.margins))
            ));
        }
        name
    }
}

/// Emits the supported `<style:table-cell-properties>` attributes for a cell style.
fn push_cell_properties(
    xml: &mut String,
    style: &OdtCellStyle,
    max_content_bytes: usize,
) -> Result<(), OdfError> {
    if let Some((red, green, blue)) = style.fill {
        push_bounded(xml, " fo:background-color=\"#", max_content_bytes)?;
        push_bounded(
            xml,
            &format!("{red:02x}{green:02x}{blue:02x}"),
            max_content_bytes,
        )?;
        push_bounded(xml, "\"", max_content_bytes)?;
    }
    if let Some(valign) = style.vertical_align {
        push_bounded(xml, " style:vertical-align=\"", max_content_bytes)?;
        push_bounded(xml, valign.value(), max_content_bytes)?;
        push_bounded(xml, "\"", max_content_bytes)?;
    }
    let borders = &style.borders;
    // Collapse four identical edges to the `fo:border` shorthand; otherwise emit
    // each present edge. Both forms re-import to the same model.
    if let (Some(top), Some(left), Some(bottom), Some(right)) =
        (&borders.top, &borders.left, &borders.bottom, &borders.right)
        && top == left
        && left == bottom
        && bottom == right
    {
        push_border_attribute(xml, "fo:border", top, max_content_bytes)?;
    } else {
        for (attr, edge) in [
            ("fo:border-top", &borders.top),
            ("fo:border-left", &borders.left),
            ("fo:border-bottom", &borders.bottom),
            ("fo:border-right", &borders.right),
        ] {
            if let Some(edge) = edge {
                push_border_attribute(xml, attr, edge, max_content_bytes)?;
            }
        }
    }
    let margins = &style.margins;
    // Collapse four identical padding edges to the `fo:padding` shorthand;
    // otherwise emit each present edge. Both re-import to the same model.
    if let (Some(top), Some(left), Some(bottom), Some(right)) =
        (margins.top, margins.left, margins.bottom, margins.right)
        && top == left
        && left == bottom
        && bottom == right
    {
        push_padding_attribute(xml, "fo:padding", top, max_content_bytes)?;
    } else {
        for (attr, edge) in [
            ("fo:padding-top", margins.top),
            ("fo:padding-left", margins.left),
            ("fo:padding-bottom", margins.bottom),
            ("fo:padding-right", margins.right),
        ] {
            if let Some(twips) = edge {
                push_padding_attribute(xml, attr, twips, max_content_bytes)?;
            }
        }
    }
    Ok(())
}

fn push_padding_attribute(
    xml: &mut String,
    attr: &str,
    twips: i32,
    max_content_bytes: usize,
) -> Result<(), OdfError> {
    push_bounded(xml, " ", max_content_bytes)?;
    push_bounded(xml, attr, max_content_bytes)?;
    push_bounded(xml, "=\"", max_content_bytes)?;
    push_bounded(xml, &twips_to_pt(twips), max_content_bytes)?;
    push_bounded(xml, "\"", max_content_bytes)?;
    Ok(())
}

fn push_border_attribute(
    xml: &mut String,
    attr: &str,
    edge: &OdtBorderEdge,
    max_content_bytes: usize,
) -> Result<(), OdfError> {
    push_bounded(xml, " ", max_content_bytes)?;
    push_bounded(xml, attr, max_content_bytes)?;
    push_bounded(xml, "=\"", max_content_bytes)?;
    push_escaped_attribute(xml, &edge.value(), max_content_bytes)?;
    push_bounded(xml, "\"", max_content_bytes)?;
    Ok(())
}

/// The supported graphic-formatting subset for a floating (anchored) frame that
/// cannot ride on the frame element alone: the wrap mode (`style:wrap` plus
/// `style:run-through` for the float-over-text z-band) and the text-exclusion
/// distances (`fo:margin-*`). It deliberately excludes the per-frame `svg:x`/`svg:y`
/// offset (which stays on the frame), so two identically-wrapped frames at different
/// positions share one style. An all-default value means the ODF-default `Square`
/// wrap with no distances — no graphic style is emitted, keeping a plain offset
/// frame byte-identical to the first-increment output.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct OdtGraphicStyle {
    /// `style:wrap` value, `None` for the default (`parallel`/Square).
    wrap: Option<&'static str>,
    /// `style:run-through` (`background`/`foreground`), only with `wrap="run-through"`.
    run_through: Option<&'static str>,
    /// `fo:margin-*` text-exclusion distances in EMU, `None` when zero.
    margin_top: Option<i64>,
    margin_bottom: Option<i64>,
    margin_left: Option<i64>,
    margin_right: Option<i64>,
    /// A shape's solid fill (`draw:fill="solid"` + `draw:fill-color`) as RGB; `None`
    /// leaves the fill unset (`draw:fill="none"` when `fill_none` is set).
    fill: Option<(u8, u8, u8)>,
    /// Emit `draw:fill="none"` (a shape with no fill), distinct from an unset fill.
    fill_none: bool,
    /// A shape's solid outline (`svg:stroke-color` + `svg:stroke-width` EMU).
    stroke: Option<((u8, u8, u8), i64)>,
    /// Emit `draw:stroke="none"` (a shape with no outline).
    stroke_none: bool,
}

impl OdtGraphicStyle {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// A deterministic, content-addressed NCName in the `gr` namespace (disjoint
    /// from every other minted family and from a foreign producer's `fr…` names,
    /// which the writer never re-emits). Identical content hashes to one name, so
    /// the `BTreeSet` dedups shared styles.
    fn name(&self) -> String {
        format!("gr{:016x}", font_family_hash(&format!("{self:?}")))
    }
}

/// Emits the supported `<style:graphic-properties>` attributes in a fixed order.
fn push_graphic_properties(
    xml: &mut String,
    style: &OdtGraphicStyle,
    max_content_bytes: usize,
) -> Result<(), OdfError> {
    if let Some(wrap) = style.wrap {
        push_bounded(xml, " style:wrap=\"", max_content_bytes)?;
        push_bounded(xml, wrap, max_content_bytes)?;
        push_bounded(xml, "\"", max_content_bytes)?;
    }
    if let Some(run_through) = style.run_through {
        push_bounded(xml, " style:run-through=\"", max_content_bytes)?;
        push_bounded(xml, run_through, max_content_bytes)?;
        push_bounded(xml, "\"", max_content_bytes)?;
    }
    for (attr, emu) in [
        ("fo:margin-top", style.margin_top),
        ("fo:margin-bottom", style.margin_bottom),
        ("fo:margin-left", style.margin_left),
        ("fo:margin-right", style.margin_right),
    ] {
        if let Some(emu) = emu {
            push_bounded(xml, " ", max_content_bytes)?;
            push_bounded(xml, attr, max_content_bytes)?;
            push_bounded(xml, "=\"", max_content_bytes)?;
            push_bounded(xml, &emu_to_cm(emu), max_content_bytes)?;
            push_bounded(xml, "\"", max_content_bytes)?;
        }
    }
    if let Some((red, green, blue)) = style.fill {
        push_bounded(
            xml,
            " draw:fill=\"solid\" draw:fill-color=\"#",
            max_content_bytes,
        )?;
        push_bounded(
            xml,
            &format!("{red:02x}{green:02x}{blue:02x}"),
            max_content_bytes,
        )?;
        push_bounded(xml, "\"", max_content_bytes)?;
    } else if style.fill_none {
        push_bounded(xml, " draw:fill=\"none\"", max_content_bytes)?;
    }
    if let Some(((red, green, blue), width_emu)) = style.stroke {
        push_bounded(
            xml,
            " draw:stroke=\"solid\" svg:stroke-width=\"",
            max_content_bytes,
        )?;
        push_bounded(xml, &emu_to_cm(width_emu), max_content_bytes)?;
        push_bounded(xml, "\" svg:stroke-color=\"#", max_content_bytes)?;
        push_bounded(
            xml,
            &format!("{red:02x}{green:02x}{blue:02x}"),
            max_content_bytes,
        )?;
        push_bounded(xml, "\"", max_content_bytes)?;
    } else if style.stroke_none {
        push_bounded(xml, " draw:stroke=\"none\"", max_content_bytes)?;
    }
    Ok(())
}

/// The supported table-row-formatting subset (row height), emitted as one
/// deterministic automatic `table-row` style.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct OdtRowStyle {
    height_twips: Option<u32>,
    /// `true` → `style:row-height` (exact); `false` → `style:min-row-height`.
    exact: bool,
}

impl OdtRowStyle {
    /// The representable height: an exact or at-least rule with a value. `auto`
    /// (or a bare value with no rule) is not representable and stays empty.
    fn from_height(height: &RowHeight) -> Self {
        match (height.value_twips, height.rule) {
            (Some(twips), Some(HeightRule::Exact)) => Self {
                height_twips: Some(twips),
                exact: true,
            },
            (Some(twips), Some(HeightRule::AtLeast)) => Self {
                height_twips: Some(twips),
                exact: false,
            },
            _ => Self::default(),
        }
    }

    fn is_empty(&self) -> bool {
        self.height_twips.is_none()
    }

    fn name(&self) -> String {
        match self.height_twips {
            Some(twips) => format!("ro{}{twips}", if self.exact { 'e' } else { 'm' }),
            None => "ro".to_owned(),
        }
    }
}

/// Emits the supported `<style:table-row-properties>` attributes for a row style.
fn push_row_properties(
    xml: &mut String,
    style: &OdtRowStyle,
    max_content_bytes: usize,
) -> Result<(), OdfError> {
    if let Some(twips) = style.height_twips {
        let attr = if style.exact {
            " style:row-height=\""
        } else {
            " style:min-row-height=\""
        };
        push_bounded(xml, attr, max_content_bytes)?;
        push_bounded(
            xml,
            &twips_to_pt(i32::try_from(twips).unwrap_or(0)),
            max_content_bytes,
        )?;
        push_bounded(xml, "\"", max_content_bytes)?;
    }
    Ok(())
}

/// The supported table-level formatting subset (alignment + width), emitted as
/// one deterministic automatic `table` style.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct OdtTableStyle {
    align: Option<OdtTableAlign>,
    /// Absolute width in twips (`style:width`, model `WidthType::Dxa`).
    width_twips: Option<i32>,
    /// Relative width in fiftieths of a percent (`style:rel-width`, `Pct`).
    rel_width_pct50: Option<i32>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum OdtTableAlign {
    Left,
    Center,
    Right,
}

impl OdtTableAlign {
    const fn value(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }

    const fn code(self) -> &'static str {
        match self {
            Self::Left => "l",
            Self::Center => "c",
            Self::Right => "r",
        }
    }

    /// Maps a model alignment to a table alignment. `Justify` has no table
    /// carrier (the model forbids it on tables), so it is unrepresentable and
    /// returns `None`.
    fn from_alignment(alignment: Alignment) -> Option<Self> {
        match alignment {
            Alignment::Start => Some(Self::Left),
            Alignment::Center => Some(Self::Center),
            Alignment::End => Some(Self::Right),
            Alignment::Justify => None,
        }
    }
}

impl OdtTableStyle {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    fn name(&self) -> String {
        let mut name = String::from("tb");
        if let Some(align) = self.align {
            name.push_str("_a");
            name.push_str(align.code());
        }
        if let Some(twips) = self.width_twips {
            name.push_str(&format!("_w{twips}"));
        }
        if let Some(pct50) = self.rel_width_pct50 {
            name.push_str(&format!("_r{pct50}"));
        }
        name
    }
}

/// Emits the supported `<style:table-properties>` attributes for a table style.
fn push_table_properties(
    xml: &mut String,
    style: &OdtTableStyle,
    max_content_bytes: usize,
) -> Result<(), OdfError> {
    if let Some(align) = style.align {
        push_bounded(xml, " table:align=\"", max_content_bytes)?;
        push_bounded(xml, align.value(), max_content_bytes)?;
        push_bounded(xml, "\"", max_content_bytes)?;
    }
    if let Some(twips) = style.width_twips {
        push_bounded(xml, " style:width=\"", max_content_bytes)?;
        push_bounded(xml, &twips_to_pt(twips), max_content_bytes)?;
        push_bounded(xml, "\"", max_content_bytes)?;
    }
    if let Some(pct50) = style.rel_width_pct50 {
        push_bounded(xml, " style:rel-width=\"", max_content_bytes)?;
        // Round fiftieths-of-a-percent to the nearest whole percent.
        push_bounded(xml, &((pct50 + 25) / 50).to_string(), max_content_bytes)?;
        push_bounded(xml, "%\"", max_content_bytes)?;
    }
    Ok(())
}

/// A deterministic, NCName-safe automatic style name for a table column of the
/// given width (twips). Negative widths (not model-valid, but defensive) use an
/// `n` marker instead of a bare minus.
fn column_style_name(twips: i32) -> String {
    if twips < 0 {
        format!("co_n{}", twips.unsigned_abs())
    } else {
        format!("co{twips}")
    }
}

/// Formats a twips length (1/1440 in) as an ODF `pt` string exactly reversible by
/// the importer's length parser: 1 twip = 0.05pt, so at most two decimals.
fn twips_to_pt(twips: i32) -> String {
    let sign = if twips < 0 { "-" } else { "" };
    let abs = twips.unsigned_abs();
    let whole = abs / 20;
    let hundredths = (abs % 20) * 5;
    if hundredths == 0 {
        format!("{sign}{whole}pt")
    } else if hundredths.is_multiple_of(10) {
        format!("{sign}{whole}.{}pt", hundredths / 10)
    } else {
        format!("{sign}{whole}.{hundredths:02}pt")
    }
}

/// Reserved `style:name` prefix for the automatic run styles minted by
/// [`OdtRunStyle::name`]. Named character styles must not reuse it (see
/// [`Writer::register_named_styles`]) because both share the text family's
/// document-wide name space.
const RUN_STYLE_PREFIX: &str = "T_";

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct OdtRunStyle {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    color: Option<(u8, u8, u8)>,
    size_half_points: Option<u32>,
    /// A named font family (`fo:font-family`). Theme fonts and the complex/
    /// east-asian font slots are not captured here and stay in the remainder.
    font_family: Option<String>,
    /// Superscript (`Some(true)`) or subscript (`Some(false)`) via
    /// `style:text-position`; baseline is the default and stays `None`.
    super_sub: Option<bool>,
    /// `fo:text-transform="uppercase"`.
    all_caps: Option<bool>,
    /// `fo:font-variant="small-caps"`.
    small_caps: Option<bool>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum OdtListLabel {
    Bullet(String),
    Number {
        format: &'static str,
        prefix: String,
        suffix: String,
    },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct OdtListLevel {
    level: u8,
    start: u16,
    label: OdtListLabel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OdtListStyle {
    name: String,
    levels: BTreeMap<u8, OdtListLevel>,
}

#[derive(Clone, Copy)]
struct ActiveVerticalMerge {
    anchor: (usize, usize),
    span: u32,
}

#[derive(Default)]
struct TableMergeAnalysis {
    row_spans: BTreeMap<(usize, usize), usize>,
    continuations: BTreeSet<(usize, usize)>,
}

impl OdtRunStyle {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    fn name(&self) -> String {
        // Base codes are always present; the newer families append a code only
        // when set, so a style using only the original subset keeps its exact
        // historical name and byte output.
        let mut name = format!(
            "{RUN_STYLE_PREFIX}b{}_i{}_u{}_s{}_c{}_z{}",
            tri_state(self.bold),
            tri_state(self.italic),
            tri_state(self.underline),
            tri_state(self.strike),
            self.color
                .map(|(red, green, blue)| format!("{red:02x}{green:02x}{blue:02x}"))
                .unwrap_or_else(|| "n".to_owned()),
            self.size_half_points
                .map(|size| size.to_string())
                .unwrap_or_else(|| "n".to_owned()),
        );
        if let Some(family) = &self.font_family {
            // A font family is not a valid NCName fragment (spaces, punctuation),
            // so identify it by a deterministic hash of its exact bytes.
            name.push_str(&format!("_f{:016x}", font_family_hash(family)));
        }
        if let Some(super_sub) = self.super_sub {
            name.push_str(if super_sub { "_pu" } else { "_pd" });
        }
        if let Some(all_caps) = self.all_caps {
            name.push_str(if all_caps { "_a1" } else { "_a0" });
        }
        if let Some(small_caps) = self.small_caps {
            name.push_str(if small_caps { "_k1" } else { "_k0" });
        }
        name
    }
}

/// Deterministic FNV-1a hash of a font family name, used only to mint a unique,
/// NCName-safe style name suffix. Not security-sensitive.
fn font_family_hash(value: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

const fn tri_state(value: Option<bool>) -> &'static str {
    match value {
        None => "n",
        Some(false) => "0",
        Some(true) => "1",
    }
}

fn split_run_properties(properties: &RunProperties) -> (OdtRunStyle, RunProperties) {
    let mut remainder = properties.clone();
    let mut style = OdtRunStyle {
        bold: remainder.bold.take(),
        italic: remainder.italic.take(),
        underline: remainder.underline.take(),
        strike: remainder.strike.take(),
        size_half_points: remainder.size_half_points.take(),
        ..OdtRunStyle::default()
    };
    if let Some(Color::Rgb(color)) = remainder.color {
        style.color = Some((color.r, color.g, color.b));
        remainder.color = None;
    }
    // Font family: only the primary Named slot maps to `fo:font-family`. Theme
    // fonts, the complex/east-asian/h-ansi slots, and a family carrying a
    // character we cannot serialize stay in the remainder and are reported, so
    // nothing is silently lost and an unrepresentable name never aborts the
    // whole export.
    if let Some(FontRef::Named(name)) = &remainder.font_ref
        && is_representable(&name.name)
    {
        style.font_family = Some(name.name.clone());
        remainder.font_ref = None;
    }
    match remainder.vertical_alignment {
        Some(VerticalAlignment::Superscript) => {
            style.super_sub = Some(true);
            remainder.vertical_alignment = None;
        }
        Some(VerticalAlignment::Subscript) => {
            style.super_sub = Some(false);
            remainder.vertical_alignment = None;
        }
        // Baseline is an explicit reset only DOCX produces; leave it reported.
        _ => {}
    }
    style.all_caps = remainder.all_caps.take();
    style.small_caps = remainder.small_caps.take();
    // A `style_ref` is projected to a named `text:style-name` by the caller, not to
    // this automatic-style subset, so drop it here to keep the remainder check (any
    // leftover field is a reported loss) from treating a styled run as lossy.
    remainder.style_ref = None;
    (style, remainder)
}

/// Extracts the supported paragraph-formatting subset out of `remainder` into an
/// `OdtParagraphStyle`, leaving every unsupported field (and the unrepresentable
/// residue of a partially-mapped sub-struct) in place so the caller's
/// `!= default` check still reports it.
fn split_paragraph_properties(remainder: &mut ParagraphProperties) -> OdtParagraphStyle {
    let mut style = OdtParagraphStyle {
        alignment: remainder.alignment.take().map(OdtParagraphAlignment::from),
        ..OdtParagraphStyle::default()
    };
    if let Some(indent) = remainder.indentation.take() {
        style.margin_left_twips = indent.start_twips;
        style.margin_right_twips = indent.end_twips;
        // ODF `fo:text-indent` is a single value: positive is a first-line
        // indent, negative is a hanging indent. A negative model `first_line` is
        // therefore intentionally canonicalized to `hanging` on the round trip.
        // `checked_neg` guards the i32::MIN case (its negation is unrepresentable).
        let hanging_indent = indent.hanging_twips.and_then(i32::checked_neg);
        style.text_indent_twips = indent.first_line_twips.or(hanging_indent);
        // Keep any hanging value that could not be represented as a single
        // text-indent (both first-line and hanging set, or an unnegatable MIN) so
        // it is reported rather than silently dropped.
        let hanging_dropped = if indent.first_line_twips.is_some() || hanging_indent.is_none() {
            indent.hanging_twips
        } else {
            None
        };
        let leftover = Indentation {
            start_twips: None,
            end_twips: None,
            first_line_twips: None,
            hanging_twips: hanging_dropped,
        };
        if leftover != Indentation::default() {
            remainder.indentation = Some(leftover);
        }
    }
    if let Some(spacing) = remainder.spacing.take() {
        style.margin_top_twips = spacing.before_twips;
        style.margin_bottom_twips = spacing.after_twips;
        style.line_percent = spacing.line_percent;
        let leftover = Spacing {
            before_twips: None,
            after_twips: None,
            line_percent: None,
            ..spacing
        };
        if leftover != Spacing::default() {
            remainder.spacing = Some(leftover);
        }
    }
    style.keep_next = std::mem::take(&mut remainder.keep_next);
    style.keep_together = std::mem::take(&mut remainder.keep_lines);
    style.break_before = std::mem::take(&mut remainder.page_break_before);
    style
}

struct Writer {
    xml: String,
    paragraph_styles: BTreeSet<OdtParagraphStyle>,
    /// Distinct table-column widths (twips) that need a `table-column` style.
    column_styles: BTreeSet<i32>,
    /// Distinct cell formatting that needs a `table-cell` style.
    cell_styles: BTreeSet<OdtCellStyle>,
    /// Distinct row formatting (height) that needs a `table-row` style.
    row_styles: BTreeSet<OdtRowStyle>,
    /// Distinct table-level formatting (align/width) that needs a `table` style.
    table_styles: BTreeSet<OdtTableStyle>,
    run_styles: BTreeSet<OdtRunStyle>,
    /// Distinct floating-frame graphic formatting (wrap + exclusion distances).
    graphic_styles: BTreeSet<OdtGraphicStyle>,
    list_styles: BTreeMap<NumberingInstanceId, OdtListStyle>,
    emitted_lists: BTreeSet<NumberingInstanceId>,
    footnotes: BTreeMap<NoteId, Note>,
    endnotes: BTreeMap<NoteId, Note>,
    comments: BTreeMap<CommentId, Comment>,
    /// Count of emitted table-of-contents, for minting a document-unique
    /// `text:name` when the model carries no tag.
    toc_count: usize,
    /// Every `text:name` already emitted for a TOC, so a minted or model-carried
    /// name is never duplicated across indexes (ODF requires unique index names).
    emitted_toc_names: BTreeSet<String>,
    /// Revision node id → its emitted `text:change-id`, from the pre-walk.
    revision_change_ids: BTreeMap<NodeId, String>,
    /// Ordered insertion regions to declare in the leading `text:tracked-changes`
    /// block (change-id + author/date).
    revision_regions: Vec<RevisionRegion>,
    /// Claimed change-ids (model-carried NCNames + minted) for uniqueness.
    used_change_ids: BTreeSet<String>,
    /// Monotonic counter for minting `ctN` change-ids.
    revision_mint: usize,
    /// Form-field node id → its emitted `form:id`, from the pre-walk.
    form_field_ids: BTreeMap<NodeId, String>,
    /// Ordered form controls to declare in `office:forms` (id + name + kind).
    form_controls: Vec<(String, Option<String>, FormControlOut)>,
    /// Monotonic counter for minting `ctrlN` form ids.
    form_mint: usize,
    emitted_footnotes: BTreeSet<NoteId>,
    emitted_endnotes: BTreeSet<NoteId>,
    footnote_occurrences: BTreeMap<NoteId, usize>,
    endnote_occurrences: BTreeMap<NoteId, usize>,
    limits: OdfExportLimits,
    blocks: usize,
    inlines: usize,
    table_rows: usize,
    table_cells: usize,
    notes: usize,
    note_depth: usize,
    text_bytes: usize,
    paragraphs_written: usize,
    /// Media whose bytes are available to repackage (media id → package part
    /// name). When a `Drawing`'s media is present, it is emitted as a
    /// `draw:frame`; otherwise it degrades to an alt-text projection. Empty on
    /// the plain semantic path.
    available_media: BTreeMap<MediaId, String>,
    /// Whether the content header in use declares the `draw:`/`svg:` namespaces
    /// (true only for `CONTENT_HEADER_PRESERVING`, i.e. `write_odt_with_retained_parts`).
    /// A standalone shape (`draw:rect`) needs these even though it has no media of
    /// its own, so its export gates on this rather than on `available_media` —
    /// otherwise the plain `write_odt` path would emit a namespace-invalid element.
    drawing_namespaces_available: bool,
    /// Bookmark id → name, so `BookmarkStart`/`BookmarkEnd` markers can re-emit
    /// their `text:bookmark-start`/`-end` elements.
    bookmarks: BTreeMap<BookmarkId, String>,
    /// Named character-style id → the `style:name` (an NCName) emitted for it in
    /// styles.xml and referenced by a run's `text:style-name`. Populated from the
    /// document's `Character` style definitions; empty on documents without any.
    named_styles: BTreeMap<StyleId, String>,
    /// Named paragraph-style id → the `style:name` emitted for it in styles.xml and
    /// referenced by a paragraph's `text:style-name`. The paragraph analogue of
    /// `named_styles`; kept separate because the two share no id space.
    named_paragraph_styles: BTreeMap<StyleId, String>,
    reporter: Reporter,
}

impl Writer {
    fn new(limits: OdfExportLimits) -> Result<Self, OdfError> {
        let mut writer = Self {
            xml: String::new(),
            paragraph_styles: BTreeSet::new(),
            column_styles: BTreeSet::new(),
            cell_styles: BTreeSet::new(),
            row_styles: BTreeSet::new(),
            table_styles: BTreeSet::new(),
            run_styles: BTreeSet::new(),
            graphic_styles: BTreeSet::new(),
            list_styles: BTreeMap::new(),
            emitted_lists: BTreeSet::new(),
            footnotes: BTreeMap::new(),
            endnotes: BTreeMap::new(),
            comments: BTreeMap::new(),
            toc_count: 0,
            emitted_toc_names: BTreeSet::new(),
            revision_change_ids: BTreeMap::new(),
            revision_regions: Vec::new(),
            used_change_ids: BTreeSet::new(),
            revision_mint: 0,
            form_field_ids: BTreeMap::new(),
            form_controls: Vec::new(),
            form_mint: 0,
            emitted_footnotes: BTreeSet::new(),
            emitted_endnotes: BTreeSet::new(),
            footnote_occurrences: BTreeMap::new(),
            endnote_occurrences: BTreeMap::new(),
            limits,
            blocks: 0,
            inlines: 0,
            table_rows: 0,
            table_cells: 0,
            notes: 0,
            note_depth: 0,
            text_bytes: 0,
            paragraphs_written: 0,
            available_media: BTreeMap::new(),
            drawing_namespaces_available: false,
            bookmarks: BTreeMap::new(),
            named_styles: BTreeMap::new(),
            named_paragraph_styles: BTreeMap::new(),
            reporter: Reporter::new(limits.max_report_features),
        };
        writer.push(BODY_PREFIX)?;
        Ok(writer)
    }

    fn register_bookmarks(&mut self, definitions: &Definitions) {
        self.bookmarks.extend(
            definitions
                .bookmarks
                .iter()
                .map(|(id, bookmark)| (*id, bookmark.name.clone())),
        );
    }

    /// Assigns each named `Character` and `Paragraph` style definition the
    /// `style:name` it will carry in styles.xml and be referenced by (via a run's
    /// or paragraph's `text:style-name`). See [`assign_named_style_names`] for how a
    /// name is chosen and why a name colliding with an automatic-style namespace is
    /// re-minted; the two families keep separate maps because they share no id space.
    fn register_named_styles(&mut self, definitions: &Definitions) {
        self.named_styles = assign_named_style_names(
            definitions,
            StyleKind::Character,
            |name| name.starts_with(RUN_STYLE_PREFIX),
            "Char",
        );
        self.named_paragraph_styles = assign_named_style_names(
            definitions,
            StyleKind::Paragraph,
            is_automatic_paragraph_style_name,
            "Para",
        );
    }

    fn register_numbering(&mut self, definitions: &Definitions) {
        for (instance_id, instance) in definitions.numbering.iter() {
            let Some(abstract_numbering) =
                definitions.abstract_numbering.get(&instance.abstract_ref)
            else {
                continue;
            };
            let overrides = instance
                .overrides
                .iter()
                .filter_map(|value| value.start.map(|start| (value.level, start)))
                .collect::<BTreeMap<_, _>>();
            let mut levels = BTreeMap::new();
            let mut supported = true;
            for level in &abstract_numbering.levels {
                let label = match odt_list_label(
                    level.level,
                    level.num_fmt.as_ref(),
                    level.lvl_text.as_deref(),
                ) {
                    Some(label) => label,
                    None => {
                        supported = false;
                        self.reporter
                            .record("odt.export.list_label", ModelOutcome::Omitted);
                        continue;
                    }
                };
                if level
                    .lvl_jc
                    .is_some_and(|value| value != LevelJustification::Start)
                    || level.suff.is_some_and(|value| value != LevelSuffix::Tab)
                    || level.is_lgl
                    || level.paragraph_properties.is_some()
                    || level.run_properties.is_some()
                    || level.style_ref.is_some()
                {
                    self.reporter
                        .record("odt.export.list_level_properties", ModelOutcome::Omitted);
                }
                levels.insert(
                    level.level,
                    OdtListLevel {
                        level: level.level,
                        start: overrides.get(&level.level).copied().unwrap_or(level.start),
                        label,
                    },
                );
            }
            if supported && !levels.is_empty() {
                let name = odt_list_style_name(&levels);
                self.list_styles
                    .insert(*instance_id, OdtListStyle { name, levels });
            }
        }
    }

    fn register_notes(&mut self, definitions: &Definitions) {
        self.footnotes.extend(
            definitions
                .footnotes
                .iter()
                .map(|(id, note)| (*id, note.clone())),
        );
        self.endnotes.extend(
            definitions
                .endnotes
                .iter()
                .map(|(id, note)| (*id, note.clone())),
        );
        self.comments.extend(
            definitions
                .comments
                .iter()
                .map(|(id, comment)| (*id, comment.clone())),
        );
    }

    /// Pre-walks every reachable inline (body + footnote/endnote/comment bodies)
    /// assigning a `text:change-id` to each insertion revision, so the leading
    /// `text:tracked-changes` block can declare them before the body references
    /// them via `text:change-start`/`-end`.
    fn register_revisions(&mut self, document: &Document) {
        self.collect_revisions_in_blocks(document.body());
        let definitions = document.definitions();
        for (_, note) in definitions.footnotes.iter() {
            self.collect_revisions_in_blocks(&note.blocks);
        }
        for (_, note) in definitions.endnotes.iter() {
            self.collect_revisions_in_blocks(&note.blocks);
        }
        for (_, comment) in definitions.comments.iter() {
            self.collect_revisions_in_blocks(&comment.blocks);
        }
    }

    fn collect_revisions_in_blocks(&mut self, blocks: &[BlockNode]) {
        for block in blocks {
            match block {
                BlockNode::Paragraph(paragraph) => {
                    self.collect_revisions_in_inlines(&paragraph.inlines);
                }
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            self.collect_revisions_in_blocks(&cell.blocks);
                        }
                    }
                }
                BlockNode::Sdt(sdt) => self.collect_revisions_in_blocks(&sdt.blocks),
                BlockNode::AltChunk(_) => {}
            }
        }
    }

    fn collect_revisions_in_inlines(&mut self, inlines: &[InlineNode]) {
        for inline in inlines {
            match inline {
                InlineNode::Revision(revision) => match revision.kind {
                    RevisionKind::Insertion => {
                        self.assign_revision(revision);
                        // An insertion's children ARE emitted in the body.
                        self.collect_revisions_in_inlines(&revision.inlines);
                    }
                    RevisionKind::Deletion => {
                        // A deletion's content is flattened into its region, not
                        // emitted in the body — so assign a region only when there
                        // is flattenable content (an empty flatten would emit an
                        // unresolvable marker), and do NOT recurse (a nested
                        // revision would orphan-declare its own region that no body
                        // marker references).
                        if !flatten_inline_text(&revision.inlines).is_empty() {
                            self.assign_revision(revision);
                        }
                    }
                    _ => self.collect_revisions_in_inlines(&revision.inlines),
                },
                InlineNode::Hyperlink(link) => self.collect_revisions_in_inlines(&link.inlines),
                InlineNode::Sdt(sdt) => self.collect_revisions_in_inlines(&sdt.inlines),
                InlineNode::TextBox(text_box) => {
                    // A text box's blocks are written (write_text_box), so pre-walk
                    // them too — else a revision/form field inside a box would be
                    // emitted without a declared region/control entry.
                    self.collect_revisions_in_blocks(&text_box.blocks);
                }
                InlineNode::Field(field) => {
                    // Every modeled form field is emitted as ONLY a draw:control
                    // anchor (its inlines are never written), so mint its control
                    // and do NOT pre-walk its inlines — recursing would declare an
                    // orphan region for a nested revision the writer drops.
                    let minted_form = field.form.is_some();
                    if minted_form {
                        self.assign_form_field(field);
                    } else if field_projects_inlines(field) {
                        // Only recurse into a field's projection inlines when the
                        // writer will actually emit them (the degraded `_` path).
                        self.collect_revisions_in_inlines(&field.inlines);
                    }
                }
                _ => {}
            }
        }
    }

    fn assign_revision(&mut self, revision: &Revision) {
        if self.revision_change_ids.contains_key(&revision.id) {
            return;
        }
        let change_id = match &revision.revision_id {
            Some(id) if is_ncname(id) && !self.used_change_ids.contains(id) => id.clone(),
            _ => loop {
                self.revision_mint += 1;
                let candidate = format!("ct{}", self.revision_mint);
                if !self.used_change_ids.contains(&candidate) {
                    break candidate;
                }
            },
        };
        self.used_change_ids.insert(change_id.clone());
        self.revision_change_ids
            .insert(revision.id, change_id.clone());
        // A deletion declares its content in the region (the body carries only a
        // point marker); an insertion's content stays in the body.
        let deleted_text = (revision.kind == RevisionKind::Deletion)
            .then(|| flatten_inline_text(&revision.inlines));
        self.revision_regions.push(RevisionRegion {
            change_id,
            author: revision.author.clone(),
            date: revision.date.clone(),
            deleted_text,
        });
    }

    /// Emits the leading `text:tracked-changes` block declaring every collected
    /// insertion region. Emitted only when there is at least one, so a
    /// revision-free document keeps identical bytes. `dc` is declared inline.
    fn write_tracked_changes(&mut self) -> Result<(), OdfError> {
        if self.revision_regions.is_empty() {
            return Ok(());
        }
        self.push("<text:tracked-changes xmlns:dc=\"http://purl.org/dc/elements/1.1/\">")?;
        let regions = std::mem::take(&mut self.revision_regions);
        for region in &regions {
            self.push("<text:changed-region text:id=\"")?;
            push_escaped_attribute(
                &mut self.xml,
                &region.change_id,
                self.limits.max_content_bytes,
            )?;
            let deletion = region.deleted_text.is_some();
            self.push(if deletion {
                "\"><text:deletion><office:change-info>"
            } else {
                "\"><text:insertion><office:change-info>"
            })?;
            if let Some(author) = &region.author
                && is_representable(author)
            {
                self.push("<dc:creator>")?;
                self.write_pcdata(author)?;
                self.push("</dc:creator>")?;
            }
            if let Some(date) = &region.date
                && is_representable(date)
            {
                self.push("<dc:date>")?;
                self.write_pcdata(date)?;
                self.push("</dc:date>")?;
            }
            self.push("</office:change-info>")?;
            if let Some(deleted) = &region.deleted_text {
                // The deleted content as one paragraph (line breaks become
                // text:line-break) — re-read by the importer's deletion capture.
                self.push("<text:p>")?;
                self.write_text(deleted)?;
                self.push("</text:p></text:deletion></text:changed-region>")?;
            } else {
                self.push("</text:insertion></text:changed-region>")?;
            }
        }
        self.revision_regions = regions;
        self.push("</text:tracked-changes>")
    }

    /// Records a text-input, checkbox, or drop-down form field, minting its
    /// `form:id` and its `office:forms` registry entry.
    fn assign_form_field(&mut self, field: &Field) {
        let Some(form) = &field.form else {
            return;
        };
        let out = match &form.kind {
            FormFieldKind::TextInput(_) => FormControlOut::Text,
            FormFieldKind::CheckBox(checkbox) => FormControlOut::CheckBox(checkbox.checked),
            FormFieldKind::DropDown(list) => FormControlOut::DropDown(list.entries.clone()),
        };
        if self.form_field_ids.contains_key(&field.id) {
            return;
        }
        self.form_mint += 1;
        let form_id = format!("ctrl{}", self.form_mint);
        self.form_field_ids.insert(field.id, form_id.clone());
        self.form_controls.push((form_id, form.name.clone(), out));
    }

    /// Emits the `office:forms` control registry (a single `form:form` holding one
    /// `form:text`/`form:checkbox`/`form:listbox` per collected control). Emitted
    /// only when non-empty, with `xmlns:form` declared inline so form-free
    /// documents stay byte-identical.
    fn write_forms(&mut self) -> Result<(), OdfError> {
        if self.form_controls.is_empty() {
            return Ok(());
        }
        self.push(
            "<office:forms xmlns:form=\"urn:oasis:names:tc:opendocument:xmlns:form:1.0\"><form:form form:name=\"Standard\">",
        )?;
        let controls = std::mem::take(&mut self.form_controls);
        for (form_id, name, kind) in &controls {
            self.push(match kind {
                FormControlOut::Text => "<form:text form:id=\"",
                FormControlOut::CheckBox(_) => "<form:checkbox form:id=\"",
                FormControlOut::DropDown(_) => "<form:listbox form:id=\"",
            })?;
            push_escaped_attribute(&mut self.xml, form_id, self.limits.max_content_bytes)?;
            self.push("\"")?;
            if let Some(name) = name
                && is_representable(name)
            {
                self.push(" form:name=\"")?;
                push_escaped_attribute(&mut self.xml, name, self.limits.max_content_bytes)?;
                self.push("\"")?;
            }
            match kind {
                FormControlOut::Text => self.push("/>")?,
                FormControlOut::CheckBox(checked) => {
                    if let Some(checked) = checked {
                        self.push(" form:current-state=\"")?;
                        self.push(if *checked { "checked" } else { "unchecked" })?;
                        self.push("\"")?;
                    }
                    self.push("/>")?;
                }
                FormControlOut::DropDown(entries) => {
                    // A listbox wraps one form:option per entry label.
                    self.push(">")?;
                    for entry in entries {
                        if is_representable(entry) {
                            self.push("<form:option form:label=\"")?;
                            push_escaped_attribute(
                                &mut self.xml,
                                entry,
                                self.limits.max_content_bytes,
                            )?;
                            self.push("\"/>")?;
                        }
                    }
                    self.push("</form:listbox>")?;
                }
            }
        }
        self.form_controls = controls;
        self.push("</form:form></office:forms>")
    }

    fn push(&mut self, value: &str) -> Result<(), OdfError> {
        let observed = self
            .xml
            .len()
            .checked_add(value.len())
            .ok_or(OdfError::LimitExceeded {
                limit: "odt_export_content_bytes",
                observed: usize::MAX,
                allowed: self.limits.max_content_bytes,
            })?;
        enforce(
            "odt_export_content_bytes",
            observed,
            self.limits.max_content_bytes,
        )?;
        self.xml.push_str(value);
        Ok(())
    }

    fn visit_block(&mut self) -> Result<(), OdfError> {
        self.blocks = checked_add(self.blocks, 1, "odt_export_blocks", self.limits.max_blocks)?;
        Ok(())
    }

    fn visit_inline(&mut self) -> Result<(), OdfError> {
        self.inlines = checked_add(
            self.inlines,
            1,
            "odt_export_inline_nodes",
            self.limits.max_inline_nodes,
        )?;
        Ok(())
    }

    fn check_depth(&self, depth: usize) -> Result<(), OdfError> {
        enforce(
            "odt_export_recursion_depth",
            depth,
            self.limits.max_recursion_depth,
        )
    }

    fn write_blocks(&mut self, blocks: &[BlockNode], depth: usize) -> Result<(), OdfError> {
        self.check_depth(depth)?;
        let mut index = 0_usize;
        while index < blocks.len() {
            if let BlockNode::Paragraph(paragraph) = &blocks[index]
                && let Some(numbering) = paragraph.properties.numbering
                && self.list_styles.contains_key(&numbering.instance)
            {
                let mut paragraphs = Vec::new();
                while let Some(BlockNode::Paragraph(paragraph)) = blocks.get(index) {
                    let Some(reference) = paragraph.properties.numbering else {
                        break;
                    };
                    if reference.instance != numbering.instance
                        || !self.list_styles.contains_key(&reference.instance)
                    {
                        break;
                    }
                    self.visit_block()?;
                    paragraphs.push(paragraph);
                    index += 1;
                }
                self.write_list(numbering.instance, &paragraphs, depth + 1)?;
                continue;
            }
            let block = &blocks[index];
            index += 1;
            self.visit_block()?;
            match block {
                BlockNode::Paragraph(paragraph) => {
                    self.write_paragraph(paragraph, depth + 1, false)?
                }
                BlockNode::Sdt(sdt) if is_toc_sdt(sdt) => self.write_toc(sdt, depth + 1)?,
                BlockNode::Sdt(sdt) => {
                    self.reporter
                        .record("odt.export.block_content_control", ModelOutcome::Degraded);
                    self.write_blocks(&sdt.blocks, depth + 1)?;
                }
                BlockNode::Table(table) => self.write_table(table, depth + 1)?,
                BlockNode::AltChunk(_) => self
                    .reporter
                    .record("odt.export.alt_chunk", ModelOutcome::Omitted),
            }
        }
        Ok(())
    }

    fn write_table(&mut self, table: &Table, depth: usize) -> Result<(), OdfError> {
        self.check_depth(depth)?;
        self.table_rows = checked_add(
            self.table_rows,
            table.rows.len(),
            "odt_export_table_rows",
            self.limits.max_table_rows,
        )?;
        let mut columns = table.grid.len();
        for row in &table.rows {
            self.table_cells = checked_add(
                self.table_cells,
                row.cells.len(),
                "odt_export_table_cells",
                self.limits.max_table_cells,
            )?;
            columns = columns.max(table_row_width(row)?);
        }
        enforce(
            "odt_export_table_columns",
            columns,
            self.limits.max_table_columns,
        )?;
        if table.grid.len() != columns {
            self.reporter
                .record("odt.export.table_grid", ModelOutcome::Degraded);
        }
        if table.grid_change.is_some() {
            self.reporter
                .record("odt.export.table_grid_change", ModelOutcome::Omitted);
        }
        // Extract the supported table-level formatting (alignment + width) into a
        // `table` style; the residue is the reported remainder.
        let mut table_remainder = table.properties.clone();
        let mut table_style = OdtTableStyle::default();
        // `Justify` has no table:align carrier; leave it in the remainder to be
        // reported rather than emitting an unrepresentable value.
        if let Some(align) = table_remainder
            .alignment
            .and_then(OdtTableAlign::from_alignment)
        {
            table_style.align = Some(align);
            table_remainder.alignment = None;
        }
        match table_remainder.width.take() {
            Some(TableWidth {
                value,
                width_type: WidthType::Dxa,
            }) => table_style.width_twips = Some(value),
            Some(TableWidth {
                value,
                width_type: WidthType::Pct,
            }) => {
                table_style.rel_width_pct50 = Some(value);
                // The percent is emitted rounded to a whole percent; a value not
                // on a 1% boundary loses sub-percent precision, so report it.
                if value % 50 != 0 {
                    self.reporter
                        .record("odt.export.table_properties", ModelOutcome::Omitted);
                }
            }
            // Auto/Nil carry no representable width; put it back to be reported.
            other => table_remainder.width = other,
        }
        if table_remainder != Default::default() {
            self.reporter
                .record("odt.export.table_properties", ModelOutcome::Omitted);
        }
        let table_style_name = (!table_style.is_empty()).then(|| {
            let name = table_style.name();
            self.table_styles.insert(table_style);
            name
        });

        let merges = analyze_table_merges(table)?;
        if let Some(name) = &table_style_name {
            self.push("<table:table table:style-name=\"")?;
            self.push(name)?;
            self.push("\">")?;
        } else {
            self.push("<table:table>")?;
        }
        // Per-column widths, padded with None where rows imply more columns than
        // the grid declares.
        let widths: Vec<Option<i32>> = (0..columns)
            .map(|index| table.grid.get(index).and_then(|column| column.width_twips))
            .collect();
        if widths.iter().all(Option::is_none) {
            // No widths: one width-less repeated column (byte-identical to before).
            self.push("<table:table-column")?;
            if columns > 1 {
                self.push(" table:number-columns-repeated=\"")?;
                self.push(&columns.to_string())?;
                self.push("\"")?;
            }
            self.push("/>")?;
        } else {
            // Emit a column group per run of equal widths.
            let mut index = 0;
            while index < columns {
                let width = widths[index];
                let mut run = 1;
                while index + run < columns && widths[index + run] == width {
                    run += 1;
                }
                self.push("<table:table-column")?;
                if let Some(width) = width {
                    self.column_styles.insert(width);
                    self.push(" table:style-name=\"")?;
                    self.push(&column_style_name(width))?;
                    self.push("\"")?;
                }
                if run > 1 {
                    self.push(" table:number-columns-repeated=\"")?;
                    self.push(&run.to_string())?;
                    self.push("\"")?;
                }
                self.push("/>")?;
                index += run;
            }
        }

        let header_rows = table
            .rows
            .iter()
            .take_while(|row| row.properties.header)
            .count();
        if header_rows != 0 {
            self.push("<table:table-header-rows>")?;
        }
        for (row_index, row) in table.rows.iter().enumerate() {
            if row_index == header_rows && header_rows != 0 {
                self.push("</table:table-header-rows>")?;
            }
            let header_mapped = row_index < header_rows;
            let mut remainder = row.properties.clone();
            remainder.header = false;
            // A representable row height is emitted as a table-row style, so it is
            // not part of the unsupported remainder.
            if !OdtRowStyle::from_height(&row.properties.height).is_empty() {
                remainder.height = RowHeight::default();
            }
            if remainder != TableRowProperties::default()
                || (row.properties.header && !header_mapped)
            {
                self.reporter
                    .record("odt.export.table_row_properties", ModelOutcome::Omitted);
            }
            self.write_table_row(table, row_index, &merges, depth + 1)?;
        }
        if header_rows == table.rows.len() {
            self.push("</table:table-header-rows>")?;
        }
        self.push("</table:table>")
    }

    fn write_table_row(
        &mut self,
        table: &Table,
        row_index: usize,
        merges: &TableMergeAnalysis,
        depth: usize,
    ) -> Result<(), OdfError> {
        self.check_depth(depth)?;
        let row = &table.rows[row_index];
        let row_style = OdtRowStyle::from_height(&row.properties.height);
        if row_style.is_empty() {
            self.push("<table:table-row>")?;
        } else {
            let name = row_style.name();
            self.row_styles.insert(row_style);
            self.push("<table:table-row table:style-name=\"")?;
            self.push(&name)?;
            self.push("\">")?;
        }
        for (cell_index, cell) in row.cells.iter().enumerate() {
            let coordinate = (row_index, cell_index);
            let span = cell.properties.grid_span.unwrap_or(1);
            if cell.properties.grid_span == Some(1) {
                self.reporter
                    .record("odt.export.table_cell_properties", ModelOutcome::Degraded);
            }
            // A vertically-merged continuation cell is written as covered markers
            // that carry no properties: report anything it would drop (so no
            // silent loss) and never register a style for it.
            if cell.properties.vertical_merge == Some(VerticalMerge::Continue)
                && merges.continuations.contains(&coordinate)
            {
                let mut remainder = cell.properties.clone();
                remainder.grid_span = None;
                remainder.vertical_merge = None;
                if remainder != TableCellProperties::default() {
                    self.reporter
                        .record("odt.export.table_cell_properties", ModelOutcome::Omitted);
                }
                for _ in 0..span {
                    self.push("<table:covered-table-cell/>")?;
                }
                continue;
            }

            // Primary/emitted cell: extract the supported cell style; only now is
            // it registered and referenced.
            let mut remainder = cell.properties.clone();
            remainder.grid_span = None;
            remainder.vertical_merge = None;
            let mut cell_style = OdtCellStyle::default();
            if let Some(fill) = remainder.shading.fill {
                cell_style.fill = Some((fill.r, fill.g, fill.b));
                remainder.shading.fill = None;
            }
            if let Some(valign) = remainder.vertical_alignment {
                cell_style.vertical_align = Some(valign.into());
                remainder.vertical_alignment = None;
            }
            // The four physical edges map to fo:border-*; inside-H/V and any edge
            // with text padding stay in the remainder and are reported.
            cell_style.borders.top = take_representable_border(&mut remainder.borders.top);
            cell_style.borders.left = take_representable_border(&mut remainder.borders.start);
            cell_style.borders.bottom = take_representable_border(&mut remainder.borders.bottom);
            cell_style.borders.right = take_representable_border(&mut remainder.borders.end);
            // Every in-domain cell margin maps to `fo:padding-*`; take them all.
            cell_style.margins = OdtCellMargins::from_model(&remainder.margins);
            remainder.margins = CellMargins::default();
            if remainder != TableCellProperties::default() {
                self.reporter
                    .record("odt.export.table_cell_properties", ModelOutcome::Omitted);
            }
            let cell_style_name = (!cell_style.is_empty()).then(|| {
                let name = cell_style.name();
                self.cell_styles.insert(cell_style);
                name
            });

            let row_span = merges.row_spans.get(&coordinate).copied().unwrap_or(1);
            if matches!(
                cell.properties.vertical_merge,
                Some(VerticalMerge::Continue)
            ) || (cell.properties.vertical_merge == Some(VerticalMerge::Restart)
                && row_span == 1)
            {
                self.reporter
                    .record("odt.export.table_merge", ModelOutcome::Degraded);
            }

            self.push("<table:table-cell")?;
            if let Some(name) = &cell_style_name {
                self.push(" table:style-name=\"")?;
                self.push(name)?;
                self.push("\"")?;
            }
            if span > 1 {
                self.push(" table:number-columns-spanned=\"")?;
                self.push(&span.to_string())?;
                self.push("\"")?;
            }
            if row_span > 1 {
                self.push(" table:number-rows-spanned=\"")?;
                self.push(&row_span.to_string())?;
                self.push("\"")?;
            }
            self.push(">")?;
            self.write_blocks(&cell.blocks, depth + 1)?;
            self.push("</table:table-cell>")?;
            for _ in 1..span {
                self.push("<table:covered-table-cell/>")?;
            }
        }
        self.push("</table:table-row>")
    }

    fn write_list(
        &mut self,
        instance: NumberingInstanceId,
        paragraphs: &[&Paragraph],
        depth: usize,
    ) -> Result<(), OdfError> {
        self.check_depth(depth)?;
        let style_name = self
            .list_styles
            .get(&instance)
            .map(|style| style.name.clone())
            .ok_or(OdfError::InvalidModel)?;
        let continued = !self.emitted_lists.insert(instance);
        let mut current_level = None::<u8>;
        for paragraph in paragraphs {
            let reference = paragraph
                .properties
                .numbering
                .ok_or(OdfError::InvalidModel)?;
            let target = reference.level;
            self.check_depth(depth + usize::from(target))?;
            match current_level {
                None => {
                    for level in 0..=target {
                        self.push("<text:list")?;
                        if level == 0 {
                            self.push(" text:style-name=\"")?;
                            self.push(&style_name)?;
                            self.push("\"")?;
                            if continued {
                                self.push(" text:continue-numbering=\"true\"")?;
                            }
                        }
                        self.push("><text:list-item>")?;
                    }
                }
                Some(current) if target == current => {
                    self.push("</text:list-item><text:list-item>")?;
                }
                Some(current) if target > current => {
                    for _ in current + 1..=target {
                        self.push("<text:list><text:list-item>")?;
                    }
                }
                Some(current) => {
                    for _ in target + 1..=current {
                        self.push("</text:list-item></text:list>")?;
                    }
                    self.push("</text:list-item><text:list-item>")?;
                }
            }
            self.write_paragraph(paragraph, depth + usize::from(target) + 1, true)?;
            current_level = Some(target);
        }
        if let Some(current) = current_level {
            for _ in 0..=current {
                self.push("</text:list-item></text:list>")?;
            }
        }
        Ok(())
    }

    fn write_paragraph(
        &mut self,
        paragraph: &Paragraph,
        depth: usize,
        numbering_mapped: bool,
    ) -> Result<(), OdfError> {
        self.check_depth(depth)?;
        self.paragraphs_written = self.paragraphs_written.saturating_add(1);
        let mut remainder = paragraph.properties.clone();
        let outline = remainder.outline_level.take();
        // A named paragraph style is emitted as its own `style:name`; resolve it and
        // clear the ref from the remainder so it is not counted as an unmapped
        // property. An unresolvable ref (a dropped definition) is reported.
        let named = remainder
            .style_ref
            .take()
            .and_then(|id| self.named_paragraph_styles.get(&id).cloned());
        if paragraph.properties.style_ref.is_some() && named.is_none() {
            self.reporter
                .record("odt.export.paragraph_style_ref", ModelOutcome::Omitted);
        }
        let style = split_paragraph_properties(&mut remainder);
        if remainder.numbering.take().is_some() && !numbering_mapped {
            self.reporter
                .record("odt.export.numbering", ModelOutcome::Omitted);
        }
        if remainder != ParagraphProperties::default() {
            self.reporter
                .record("odt.export.paragraph_properties", ModelOutcome::Omitted);
        }
        // A named style and direct paragraph properties cannot both be carried by a
        // single `text:style-name`; the named style wins and the direct subset is a
        // reported degrade (only reachable from a DOCX-shaped paragraph).
        if named.is_some() && !style.is_empty() {
            self.reporter.record(
                "odt.export.paragraph_style_ref_with_direct",
                ModelOutcome::Degraded,
            );
        }
        let style_name = if let Some(name) = named {
            Some(name)
        } else if style.is_empty() {
            None
        } else {
            let name = style.name();
            self.paragraph_styles.insert(style);
            Some(name)
        };
        if let Some(level) = outline {
            self.push("<text:h text:outline-level=\"")?;
            self.push(&(u16::from(level) + 1).to_string())?;
            if let Some(name) = &style_name {
                self.push("\" text:style-name=\"")?;
                self.push(name)?;
            }
            self.push("\">")?;
        } else {
            self.push("<text:p")?;
            if let Some(name) = &style_name {
                self.push(" text:style-name=\"")?;
                self.push(name)?;
                self.push("\"")?;
            }
            self.push(">")?;
        }
        self.write_inlines(&paragraph.inlines, depth + 1)?;
        self.push(if outline.is_some() {
            "</text:h>"
        } else {
            "</text:p>"
        })
    }

    /// Resolves the first section's header/footer references into deterministic
    /// styles.xml fragments. Content is limited to the bounded plain-text subset
    /// (paragraphs, plain runs, tabs, and line breaks); anything richer is a loss
    /// finding so no header/footer detail disappears silently.
    fn render_master_page(&mut self, document: &Document) -> Result<MasterPageXml, OdfError> {
        let mut parts = MasterPageXml::default();
        let Some(section) = document.definitions().sections.first() else {
            return Ok(parts);
        };
        for reference in &section.headers {
            match reference.kind {
                HeaderFooterKind::Default | HeaderFooterKind::Even => {
                    let Some(header_footer) =
                        document.definitions().headers.get(&reference.reference)
                    else {
                        continue;
                    };
                    let fragment = self.render_header_footer(&header_footer.blocks)?;
                    let slot = if matches!(reference.kind, HeaderFooterKind::Even) {
                        &mut parts.even_header
                    } else {
                        &mut parts.default_header
                    };
                    store_master_slot(slot, fragment, &mut self.reporter);
                }
                HeaderFooterKind::First => self.reporter.record(
                    "odt.export.header_footer.first_page",
                    ModelOutcome::Degraded,
                ),
            }
        }
        for reference in &section.footers {
            match reference.kind {
                HeaderFooterKind::Default | HeaderFooterKind::Even => {
                    let Some(header_footer) =
                        document.definitions().footers.get(&reference.reference)
                    else {
                        continue;
                    };
                    let fragment = self.render_header_footer(&header_footer.blocks)?;
                    let slot = if matches!(reference.kind, HeaderFooterKind::Even) {
                        &mut parts.even_footer
                    } else {
                        &mut parts.default_footer
                    };
                    store_master_slot(slot, fragment, &mut self.reporter);
                }
                HeaderFooterKind::First => self.reporter.record(
                    "odt.export.header_footer.first_page",
                    ModelOutcome::Degraded,
                ),
            }
        }
        Ok(parts)
    }

    /// Serializes one header/footer's blocks into a self-contained XML fragment by
    /// swapping the content buffer, so the reusable counters and loss reporting
    /// still aggregate while the emitted bytes are captured separately.
    fn render_header_footer(&mut self, blocks: &[BlockNode]) -> Result<String, OdfError> {
        let outer = std::mem::take(&mut self.xml);
        let result = self.render_header_footer_blocks(blocks);
        let fragment = std::mem::replace(&mut self.xml, outer);
        result.map(|()| fragment)
    }

    fn render_header_footer_blocks(&mut self, blocks: &[BlockNode]) -> Result<(), OdfError> {
        for block in blocks {
            self.visit_block()?;
            match block {
                BlockNode::Paragraph(paragraph) => {
                    self.render_header_footer_paragraph(paragraph)?
                }
                _ => self
                    .reporter
                    .record("odt.export.header_footer.block", ModelOutcome::Omitted),
            }
        }
        Ok(())
    }

    fn render_header_footer_paragraph(&mut self, paragraph: &Paragraph) -> Result<(), OdfError> {
        if paragraph.properties != ParagraphProperties::default() {
            self.reporter.record(
                "odt.export.header_footer.paragraph_properties",
                ModelOutcome::Omitted,
            );
        }
        self.push("<text:p>")?;
        for inline in &paragraph.inlines {
            self.visit_inline()?;
            match inline {
                InlineNode::Run(run) => {
                    if run.properties != RunProperties::default() {
                        self.reporter.record(
                            "odt.export.header_footer.run_properties",
                            ModelOutcome::Omitted,
                        );
                    }
                    self.write_text(&run.text)?;
                }
                InlineNode::Tab(_) => self.push("<text:tab/>")?,
                InlineNode::Break(node) => {
                    if node.kind != BreakKind::Line {
                        self.reporter
                            .record("odt.export.header_footer.break", ModelOutcome::Degraded);
                    }
                    self.push("<text:line-break/>")?;
                }
                _ => self
                    .reporter
                    .record("odt.export.header_footer.inline", ModelOutcome::Omitted),
            }
        }
        self.push("</text:p>")
    }

    fn write_inlines(&mut self, inlines: &[InlineNode], depth: usize) -> Result<(), OdfError> {
        self.check_depth(depth)?;
        for inline in inlines {
            self.visit_inline()?;
            match inline {
                InlineNode::Run(run) => {
                    // A run referencing a named character style emits that style's
                    // `text:style-name` directly (the ODT round-trip form). Any
                    // remaining direct properties belong to an automatic style.
                    let named = run
                        .properties
                        .style_ref
                        .and_then(|id| self.named_styles.get(&id).cloned());
                    if run.properties.style_ref.is_some() && named.is_none() {
                        // A dangling style ref is rejected by model validation, so
                        // this only guards a Character definition that was dropped;
                        // report rather than silently emit an unstyled run.
                        self.reporter
                            .record("odt.export.run_style_ref", ModelOutcome::Omitted);
                    }
                    let (style, remainder) = split_run_properties(&run.properties);
                    if remainder != RunProperties::default() {
                        self.reporter
                            .record("odt.export.run_properties", ModelOutcome::Omitted);
                    }
                    // A named style and direct run properties cannot both be carried
                    // by a single `text:style-name`; the named style wins and the
                    // direct subset is reported as a degrade (only reachable from a
                    // DOCX-shaped run carrying both `rStyle` and `rPr`).
                    if named.is_some() && !style.is_empty() {
                        self.reporter.record(
                            "odt.export.run_style_ref_with_direct",
                            ModelOutcome::Degraded,
                        );
                    }
                    let style_name = if let Some(name) = named {
                        Some(name)
                    } else if style.is_empty() {
                        None
                    } else {
                        let name = style.name();
                        self.run_styles.insert(style);
                        Some(name)
                    };
                    if let Some(name) = &style_name {
                        self.push("<text:span text:style-name=\"")?;
                        self.push(name)?;
                        self.push("\">")?;
                    }
                    self.write_text(&run.text)?;
                    if style_name.is_some() {
                        self.push("</text:span>")?;
                    }
                }
                InlineNode::Tab(_) => self.push("<text:tab/>")?,
                InlineNode::Break(node) => {
                    if node.kind != BreakKind::Line {
                        self.reporter
                            .record("odt.export.page_or_column_break", ModelOutcome::Degraded);
                    }
                    self.push("<text:line-break/>")?;
                }
                InlineNode::Hyperlink(link) => {
                    // A `text:a` wrapper round-trips both external and internal
                    // targets; the importer reads exactly this form
                    // (`xlink:type="simple" xlink:href=…`, `#anchor` for internal).
                    let href = match &link.target {
                        HyperlinkTarget::External(target) => match &target.anchor {
                            Some(anchor) => format!("{}#{anchor}", target.url),
                            None => target.url.clone(),
                        },
                        HyperlinkTarget::Internal(target) => format!("#{}", target.anchor),
                    };
                    let tooltip_ok = link.tooltip.as_deref().is_none_or(is_representable);
                    if crate::content::has_blocked_link_scheme(&href)
                        || !is_representable(&href)
                        || !tooltip_ok
                    {
                        // Mirror the importer's scheme allowlist so a blocked
                        // scheme is never re-emitted as a live link, and never
                        // abort the whole export on a value we cannot serialize:
                        // degrade to the inner text, dropping the link wrapper.
                        self.reporter
                            .record("odt.export.hyperlink", ModelOutcome::Degraded);
                        self.write_inlines(&link.inlines, depth + 1)?;
                    } else {
                        let max = self.limits.max_content_bytes;
                        self.push("<text:a xlink:type=\"simple\" xlink:href=\"")?;
                        push_escaped_attribute(&mut self.xml, &href, max)?;
                        if let Some(tooltip) = &link.tooltip {
                            self.push("\" office:title=\"")?;
                            push_escaped_attribute(&mut self.xml, tooltip, max)?;
                        }
                        self.push("\">")?;
                        self.write_inlines(&link.inlines, depth + 1)?;
                        self.push("</text:a>")?;
                    }
                }
                InlineNode::Field(field) if self.form_field_ids.contains_key(&field.id) => {
                    // A text-input form field: anchor it with draw:control (its
                    // office:forms entry was declared up front). xmlns:draw inline.
                    let form_id = self.form_field_ids[&field.id].clone();
                    self.push(
                        "<draw:control xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" text:anchor-type=\"as-char\" draw:control=\"",
                    )?;
                    push_escaped_attribute(&mut self.xml, &form_id, self.limits.max_content_bytes)?;
                    self.push("\"/>")?;
                }
                InlineNode::Field(field) => match field.kind {
                    // Modeled ODF field elements. The computed display is emitted
                    // empty (a renderer recomputes it), which is exactly what the
                    // importer reads back.
                    FieldKind::Page => self.push("<text:page-number/>")?,
                    FieldKind::NumPages => self.push("<text:page-count/>")?,
                    FieldKind::Date { .. } => self.push("<text:date/>")?,
                    FieldKind::Time { .. } => self.push("<text:time/>")?,
                    FieldKind::Ref { ref bookmark } if is_representable(bookmark) => {
                        self.push(
                            "<text:bookmark-ref text:reference-format=\"text\" text:ref-name=\"",
                        )?;
                        push_escaped_attribute(
                            &mut self.xml,
                            bookmark,
                            self.limits.max_content_bytes,
                        )?;
                        self.push("\"/>")?;
                    }
                    FieldKind::PageRef { ref bookmark } if is_representable(bookmark) => {
                        self.push(
                            "<text:bookmark-ref text:reference-format=\"page\" text:ref-name=\"",
                        )?;
                        push_escaped_attribute(
                            &mut self.xml,
                            bookmark,
                            self.limits.max_content_bytes,
                        )?;
                        self.push("\"/>")?;
                    }
                    FieldKind::Seq { ref name } if is_representable(name) => {
                        self.push("<text:sequence text:name=\"")?;
                        push_escaped_attribute(&mut self.xml, name, self.limits.max_content_bytes)?;
                        self.push("\"/>")?;
                    }
                    // Other field kinds (and an unserializable ref target) have no
                    // ODF element mapping yet: keep the cached display text as a
                    // degraded projection.
                    _ => {
                        self.reporter
                            .record("odt.export.field", ModelOutcome::Degraded);
                        self.write_inlines(&field.inlines, depth + 1)?;
                    }
                },
                InlineNode::Revision(revision) => {
                    // An insertion assigned a change-id in the pre-walk is wrapped
                    // in text:change-start/-end markers referencing the leading
                    // changed-region. Other kinds (deletions, moves) are not
                    // modeled in ODF here: their content degrades to plain inlines
                    // (insertion-like) or is dropped.
                    if let Some(change_id) = self.revision_change_ids.get(&revision.id).cloned() {
                        if revision.kind == RevisionKind::Deletion {
                            // A point marker; the deleted content lives in the
                            // region, so the body carries nothing but the marker.
                            self.push("<text:change text:change-id=\"")?;
                            push_escaped_attribute(
                                &mut self.xml,
                                &change_id,
                                self.limits.max_content_bytes,
                            )?;
                            self.push("\"/>")?;
                        } else {
                            self.push("<text:change-start text:change-id=\"")?;
                            push_escaped_attribute(
                                &mut self.xml,
                                &change_id,
                                self.limits.max_content_bytes,
                            )?;
                            self.push("\"/>")?;
                            self.write_inlines(&revision.inlines, depth + 1)?;
                            self.push("<text:change-end text:change-id=\"")?;
                            push_escaped_attribute(
                                &mut self.xml,
                                &change_id,
                                self.limits.max_content_bytes,
                            )?;
                            self.push("\"/>")?;
                        }
                    } else {
                        self.reporter
                            .record("odt.export.revision", ModelOutcome::Degraded);
                        if matches!(
                            revision.kind,
                            RevisionKind::Insertion | RevisionKind::MoveTo
                        ) {
                            self.write_inlines(&revision.inlines, depth + 1)?;
                        }
                    }
                }
                InlineNode::Sdt(sdt) => {
                    self.reporter
                        .record("odt.export.inline_content_control", ModelOutcome::Degraded);
                    self.write_inlines(&sdt.inlines, depth + 1)?;
                }
                InlineNode::Drawing(drawing) => {
                    if let Some(part_name) = self.available_media.get(&drawing.media).cloned() {
                        self.write_draw_frame(
                            &part_name,
                            drawing.extent,
                            drawing.descr.as_deref(),
                        )?;
                    } else {
                        self.write_alt(drawing.descr.as_deref(), "odt.export.drawing")?;
                    }
                }
                InlineNode::AnchoredDrawing(drawing) => {
                    if let Some(part_name) = self.available_media.get(&drawing.media).cloned() {
                        self.write_anchored_draw_frame(&part_name, drawing)?;
                    } else {
                        self.write_alt(drawing.descr.as_deref(), "odt.export.anchored_drawing")?;
                    }
                }
                InlineNode::Math(math) => {
                    self.reporter
                        .record("odt.export.math", ModelOutcome::Degraded);
                    self.write_text(&math.text)?;
                }
                InlineNode::Symbol(symbol) => {
                    self.reporter
                        .record("odt.export.symbol_font", ModelOutcome::Degraded);
                    if let Some(character) = char::from_u32(symbol.char) {
                        self.write_text(&character.to_string())?;
                    }
                }
                InlineNode::NoBreakHyphen(_) => self.write_text("\u{2011}")?,
                InlineNode::SoftHyphen(_) => self.write_text("\u{00ad}")?,
                InlineNode::PositionalTab(_) => {
                    self.reporter
                        .record("odt.export.positional_tab", ModelOutcome::Degraded);
                    self.push("<text:tab/>")?;
                }
                InlineNode::EmbeddedObject(_) => self
                    .reporter
                    .record("odt.export.embedded_object", ModelOutcome::Omitted),
                InlineNode::TextBox(text_box) => self.write_text_box(text_box, depth + 1)?,
                InlineNode::Group(group) => {
                    // A shape and its graphic style need the `draw:`/`svg:`
                    // namespaces, which only the preserving content header declares;
                    // the plain semantic path degrades the shape rather than emit
                    // namespace-invalid XML (matching inline images).
                    if self.drawing_namespaces_available
                        && let Some((shape, element)) = single_box_shape(group)
                    {
                        self.write_group_box_shape(group, shape, element)?;
                    } else if self.drawing_namespaces_available
                        && let Some(shape) = single_line_shape(group)
                    {
                        self.write_group_line(group, shape)?;
                    } else {
                        // A multi-child group, an unsupported geometry, a picture/
                        // text-box child, a transformed shape, or the plain path.
                        self.reporter
                            .record("odt.export.group", ModelOutcome::Omitted);
                    }
                }
                InlineNode::NoteReference(note) => self.write_note(note, depth + 1)?,
                InlineNode::CommentReference(reference) => {
                    self.write_comment(reference, depth + 1)?
                }
                InlineNode::CommentRangeStart(_) | InlineNode::CommentRangeEnd(_) => self
                    .reporter
                    .record("odt.export.comment_range", ModelOutcome::Omitted),
                InlineNode::BookmarkStart(start) => {
                    self.write_bookmark_marker("bookmark-start", start.bookmark)?
                }
                InlineNode::BookmarkEnd(end) => {
                    self.write_bookmark_marker("bookmark-end", end.bookmark)?
                }
                InlineNode::MoveRangeStart(_) | InlineNode::MoveRangeEnd(_) => self
                    .reporter
                    .record("odt.export.move_range", ModelOutcome::Omitted),
                InlineNode::HorizontalRule(_) => self
                    .reporter
                    .record("odt.export.horizontal_rule", ModelOutcome::Omitted),
                InlineNode::NoteNumberMark(_) => self
                    .reporter
                    .record("odt.export.note_number_mark", ModelOutcome::Omitted),
            }
        }
        Ok(())
    }

    fn write_note(&mut self, reference: &NoteReference, depth: usize) -> Result<(), OdfError> {
        self.check_depth(depth)?;
        if self.note_depth != 0 {
            self.reporter
                .record("odt.export.nested_note", ModelOutcome::Omitted);
            return Ok(());
        }
        self.notes = checked_add(self.notes, 1, "odt_export_notes", self.limits.max_notes)?;
        let definition = match reference.kind {
            NoteKind::Footnote => self.footnotes.get(&reference.note),
            NoteKind::Endnote => self.endnotes.get(&reference.note),
        }
        .cloned()
        .ok_or(OdfError::InvalidModel)?;
        let occurrence = match reference.kind {
            NoteKind::Footnote => self.footnote_occurrences.entry(reference.note).or_default(),
            NoteKind::Endnote => self.endnote_occurrences.entry(reference.note).or_default(),
        };
        let occurrence_index = *occurrence;
        *occurrence = occurrence.checked_add(1).ok_or(OdfError::InvalidModel)?;
        match reference.kind {
            NoteKind::Footnote => {
                self.emitted_footnotes.insert(reference.note);
            }
            NoteKind::Endnote => {
                self.emitted_endnotes.insert(reference.note);
            }
        }
        if occurrence_index != 0 {
            self.reporter
                .record("odt.export.shared_note_reference", ModelOutcome::Degraded);
        }

        self.push("<text:note text:id=\"note-")?;
        self.push(&reference.note.node_id().to_string())?;
        if occurrence_index != 0 {
            self.push("-")?;
            self.push(&(occurrence_index + 1).to_string())?;
        }
        self.push("\" text:note-class=\"")?;
        self.push(match reference.kind {
            NoteKind::Footnote => "footnote",
            NoteKind::Endnote => "endnote",
        })?;
        self.push("\"><text:note-citation/><text:note-body>")?;
        self.note_depth += 1;
        let result = self.write_blocks(&definition.blocks, depth + 1);
        self.note_depth -= 1;
        result?;
        self.push("</text:note-body></text:note>")
    }

    /// Emits an `office:annotation` (a point comment) for a `CommentReference`.
    /// The `dc` namespace is declared inline on the element so the shared content
    /// header stays unchanged for comment-free documents. The paired range is not
    /// modeled, so no `office:annotation-end` is written.
    fn write_comment(
        &mut self,
        reference: &CommentReference,
        depth: usize,
    ) -> Result<(), OdfError> {
        self.check_depth(depth)?;
        let definition = self
            .comments
            .get(&reference.comment)
            .cloned()
            .ok_or(OdfError::InvalidModel)?;
        self.push("<office:annotation xmlns:dc=\"http://purl.org/dc/elements/1.1/\">")?;
        if let Some(author) = &definition.author {
            if is_representable(author) {
                self.push("<dc:creator>")?;
                self.write_pcdata(author)?;
                self.push("</dc:creator>")?;
            } else {
                self.reporter
                    .record("odt.export.comment_author", ModelOutcome::Omitted);
            }
        }
        if let Some(date) = &definition.date {
            if is_representable(date) {
                self.push("<dc:date>")?;
                self.write_pcdata(date)?;
                self.push("</dc:date>")?;
            } else {
                self.reporter
                    .record("odt.export.comment_date", ModelOutcome::Omitted);
            }
        }
        self.write_blocks(&definition.blocks, depth + 1)?;
        self.push("</office:annotation>")
    }

    /// Emits a TOC-shaped block content control as `text:table-of-content`. The
    /// index-body carries the model's cached entry blocks; the level-template
    /// source is minimal (a single outline level). `text:name` comes from the
    /// model tag when representable, else a minted ordinal — and is then made
    /// document-unique (ODF requires distinct index names), since the model does
    /// not enforce tag uniqueness and a tag can even equal a minted pattern.
    fn write_toc(&mut self, sdt: &BlockSdt, depth: usize) -> Result<(), OdfError> {
        self.check_depth(depth)?;
        let mut name = match &sdt.properties.tag {
            Some(tag) if is_representable(tag) => tag.clone(),
            _ => {
                self.toc_count += 1;
                format!("Table of Contents{}", self.toc_count)
            }
        };
        while self.emitted_toc_names.contains(&name) {
            self.toc_count += 1;
            name = format!("Table of Contents{}", self.toc_count);
        }
        self.emitted_toc_names.insert(name.clone());
        self.push("<text:table-of-content text:name=\"")?;
        push_escaped_attribute(&mut self.xml, &name, self.limits.max_content_bytes)?;
        self.push("\"><text:table-of-content-source text:outline-level=\"10\"/><text:index-body>")?;
        self.write_blocks(&sdt.blocks, depth + 1)?;
        self.push("</text:index-body></text:table-of-content>")
    }

    /// Emits an inline `draw:frame`>`draw:text-box` carrying the box's block body.
    /// `xmlns:draw` is declared inline so text-box-free documents keep the
    /// unchanged content header. Floating/geometry/fill/border are not emitted;
    /// their presence is reported.
    fn write_text_box(&mut self, text_box: &TextBox, depth: usize) -> Result<(), OdfError> {
        self.check_depth(depth)?;
        if text_box.anchor.is_some() || text_box.fill.is_some() || text_box.border.is_some() {
            self.reporter
                .record("odt.export.text_box_properties", ModelOutcome::Degraded);
        }
        // `xmlns:svg` is declared inline alongside `xmlns:draw` only when the box
        // carries an extent, so extent-free boxes keep identical bytes.
        if let Some(extent) = &text_box.extent {
            self.push(
                "<draw:frame xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" svg:width=\"",
            )?;
            self.push(&emu_to_cm(extent.width_emu))?;
            self.push("\" svg:height=\"")?;
            self.push(&emu_to_cm(extent.height_emu))?;
            self.push("\"><draw:text-box>")?;
        } else {
            self.push(
                "<draw:frame xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\"><draw:text-box>",
            )?;
        }
        self.write_blocks(&text_box.blocks, depth + 1)?;
        self.push("</draw:text-box></draw:frame>")
    }

    fn report_unreferenced_notes(&mut self) {
        for id in self.footnotes.keys() {
            if !self.emitted_footnotes.contains(id) {
                self.reporter
                    .record("odt.export.unreferenced_note", ModelOutcome::Omitted);
            }
        }
        for id in self.endnotes.keys() {
            if !self.emitted_endnotes.contains(id) {
                self.reporter
                    .record("odt.export.unreferenced_note", ModelOutcome::Omitted);
            }
        }
    }

    /// Emits a `text:bookmark-start`/`text:bookmark-end` marker for a bookmark
    /// id, looking up its name in the registered bookmark table. A marker whose
    /// bookmark is unknown (a dangling id a validated document cannot contain) is
    /// reported and skipped rather than emitting a nameless element.
    fn write_bookmark_marker(
        &mut self,
        element: &str,
        bookmark: BookmarkId,
    ) -> Result<(), OdfError> {
        let Some(name) = self.bookmarks.get(&bookmark).cloned() else {
            self.reporter
                .record("odt.export.bookmark", ModelOutcome::Omitted);
            return Ok(());
        };
        if !is_representable(&name) {
            // A name with characters we cannot serialize (e.g. control chars a
            // validated model still permits) would otherwise abort the whole
            // export; drop the marker with a finding instead. Both the start and
            // end markers share this name, so they skip together and stay paired.
            self.reporter
                .record("odt.export.bookmark", ModelOutcome::Omitted);
            return Ok(());
        }
        let max = self.limits.max_content_bytes;
        self.push("<text:")?;
        self.push(element)?;
        self.push(" text:name=\"")?;
        push_escaped_attribute(&mut self.xml, &name, max)?;
        self.push("\"/>")?;
        Ok(())
    }

    /// Emits an inline `draw:frame` referencing a retained package image part.
    fn write_draw_frame(
        &mut self,
        part_name: &str,
        extent: Option<Extent>,
        descr: Option<&str>,
    ) -> Result<(), OdfError> {
        let max = self.limits.max_content_bytes;
        self.push("<draw:frame")?;
        if let Some(extent) = extent {
            self.push(" svg:width=\"")?;
            self.push(&emu_to_cm(extent.width_emu))?;
            self.push("\" svg:height=\"")?;
            self.push(&emu_to_cm(extent.height_emu))?;
            self.push("\"")?;
        }
        self.push("><draw:image xlink:href=\"")?;
        push_escaped_attribute(&mut self.xml, part_name, max)?;
        self.push("\"/>")?;
        self.write_frame_title(descr)?;
        self.push("</draw:frame>")
    }

    /// Writes a floating (anchored) embedded image as a positioned `draw:frame`.
    /// This increment emits the reversible core — `text:anchor-type`, `svg:x`/`svg:y`
    /// offsets, `svg:width`/`svg:height`, `draw:z-index`, and the image — with the
    /// ODF-default (`Square`) wrap carried implicitly (no graphic style). Every model
    /// state outside that core (a non-Square wrap, exclusion distances, alignment
    /// positioning, a negative offset, a contour, the picture transforms) is reported
    /// as a degrade and mapped to its nearest representable form, so the output stays
    /// a fixed point (import of this frame re-exports identically).
    fn write_anchored_draw_frame(
        &mut self,
        part_name: &str,
        drawing: &AnchoredDrawing,
    ) -> Result<(), OdfError> {
        let max = self.limits.max_content_bytes;
        let anchor = &drawing.anchor;
        // The wrap mode and text-exclusion distances ride on a graphic style; an
        // all-default (Square, no distances) frame mints none, staying byte-identical
        // to the first increment. A contour and the picture transforms are still
        // dropped with a finding.
        let graphic = self.build_graphic_style(anchor);
        if anchor.wrap_polygon.is_some() {
            self.reporter
                .record("odt.export.anchor_wrap_polygon", ModelOutcome::Degraded);
        }
        if drawing.crop.is_some()
            || drawing.border.is_some()
            || drawing.flip_h
            || drawing.flip_v
            || drawing.rotation.is_some()
        {
            self.reporter.record(
                "odt.export.anchor_picture_transform",
                ModelOutcome::Degraded,
            );
        }
        let graphic_name = (!graphic.is_empty()).then(|| {
            let name = graphic.name();
            self.graphic_styles.insert(graphic);
            name
        });
        let anchor_type = self.anchor_type_name(
            anchor.horizontal.relative_from,
            anchor.vertical.relative_from,
        );
        let x_emu = self.anchor_offset_emu(horizontal_position_offset(anchor.horizontal.position));
        let y_emu = self.anchor_offset_emu(vertical_position_offset(anchor.vertical.position));
        self.push("<draw:frame text:anchor-type=\"")?;
        self.push(anchor_type)?;
        self.push("\"")?;
        if let Some(name) = &graphic_name {
            self.push(" draw:style-name=\"")?;
            self.push(name)?;
            self.push("\"")?;
        }
        if let Some(z_index) = drawing.relative_height {
            self.push(" draw:z-index=\"")?;
            self.push(&z_index.to_string())?;
            self.push("\"")?;
        }
        self.push(" svg:x=\"")?;
        self.push(&emu_to_cm(x_emu))?;
        self.push("\" svg:y=\"")?;
        self.push(&emu_to_cm(y_emu))?;
        self.push("\" svg:width=\"")?;
        self.push(&emu_to_cm(drawing.extent.width_emu))?;
        self.push("\" svg:height=\"")?;
        self.push(&emu_to_cm(drawing.extent.height_emu))?;
        self.push("\"><draw:image xlink:href=\"")?;
        push_escaped_attribute(&mut self.xml, part_name, max)?;
        self.push("\"/>")?;
        self.write_frame_title(drawing.descr.as_deref())?;
        self.push("</draw:frame>")
    }

    /// Builds the graphic style carrying an anchor's wrap and exclusion distances.
    /// `Square` with no distances yields an empty (unemitted) style. `Tight`/`Through`
    /// have no distinct ODF form without a contour polygon (not emitted this
    /// increment), so they degrade to the default `Square` wrap with a finding.
    fn build_graphic_style(&mut self, anchor: &DrawingAnchor) -> OdtGraphicStyle {
        let (wrap, run_through) = match anchor.wrap {
            WrapMode::Square => (None, None),
            WrapMode::TopAndBottom => (Some("none"), None),
            WrapMode::None => (
                Some("run-through"),
                Some(if anchor.behind_doc {
                    "background"
                } else {
                    "foreground"
                }),
            ),
            WrapMode::Tight | WrapMode::Through => {
                self.reporter
                    .record("odt.export.anchor_wrap_contour", ModelOutcome::Degraded);
                (None, None)
            }
        };
        // `style:run-through` (the z-band) is only expressible with a run-through
        // wrap, so a `behind_doc` on any other wrap has no ODF form — report it
        // rather than dropping it silently.
        if anchor.behind_doc && anchor.wrap != WrapMode::None {
            self.reporter
                .record("odt.export.anchor_behind_doc", ModelOutcome::Degraded);
        }
        let distances = &anchor.wrap_distances;
        OdtGraphicStyle {
            wrap,
            run_through,
            margin_top: (distances.top_emu != 0).then_some(distances.top_emu),
            margin_bottom: (distances.bottom_emu != 0).then_some(distances.bottom_emu),
            margin_left: (distances.start_emu != 0).then_some(distances.start_emu),
            margin_right: (distances.end_emu != 0).then_some(distances.end_emu),
            // An anchored image carries no fill/outline; only a shape's caller
            // (`write_group_rectangle`) sets these on the value this returns.
            fill: None,
            fill_none: false,
            stroke: None,
            stroke_none: false,
        }
    }

    /// Maps the model's per-axis anchor references to the single ODF
    /// `text:anchor-type`. The two combinations this increment's importer produces
    /// round-trip exactly; any other pair is a reported degrade to the nearest of
    /// `page`/`paragraph` (idempotent — re-import yields the mapped pair).
    fn anchor_type_name(
        &mut self,
        horizontal: HorizontalAnchor,
        vertical: VerticalAnchor,
    ) -> &'static str {
        match (horizontal, vertical) {
            (HorizontalAnchor::Page, VerticalAnchor::Page) => "page",
            (HorizontalAnchor::Column, VerticalAnchor::Paragraph) => "paragraph",
            _ => {
                self.reporter
                    .record("odt.export.anchor_rel", ModelOutcome::Degraded);
                if matches!(horizontal, HorizontalAnchor::Page)
                    || matches!(vertical, VerticalAnchor::Page)
                {
                    "page"
                } else {
                    "paragraph"
                }
            }
        }
    }

    /// Clamps a position offset to the non-negative range this increment emits: the
    /// codec (`emu_to_cm`/`parse_emu`) is unsigned, so a negative offset would fail
    /// to re-import and break the fixed point. A negative offset is reported and
    /// clamped to 0. `None` (an alignment position, already reported) is also 0.
    fn anchor_offset_emu(&mut self, offset: Option<i64>) -> i64 {
        match offset {
            Some(value) if value >= 0 => value,
            Some(_) => {
                self.reporter
                    .record("odt.export.anchor_offset_negative", ModelOutcome::Degraded);
                0
            }
            None => {
                self.reporter
                    .record("odt.export.anchor_align", ModelOutcome::Degraded);
                0
            }
        }
    }

    /// Writes a standalone anchored box shape (a single-child group carrying one
    /// `GroupShape` of a box geometry) as a positioned `draw:rect`/`draw:ellipse`
    /// (`element`). Mirrors `write_anchored_draw_frame`'s anchor-type/offset/wrap
    /// handling; the shape's fill/outline ride on the same `graphic` automatic-style
    /// family. A picture transform (flip/rotation) on the shape or a group transform
    /// beyond the identity is reported and dropped — this increment emits an
    /// axis-aligned, untransformed shape only.
    fn write_group_box_shape(
        &mut self,
        group: &WordprocessingGroup,
        shape: &GroupShape,
        element: &'static str,
    ) -> Result<(), OdfError> {
        // The identity check gating this call already confirmed the transform is
        // trivial and the shape sits at the group origin; only the fill/outline and
        // the anchor's wrap/distances need a graphic style.
        let Some(anchor) = &group.anchor else {
            // A nested (non-anchored) group-of-one shape is not reachable from this
            // increment's importer (it only produces top-level anchored groups), but
            // guard defensively rather than emit an unanchored shape.
            self.reporter
                .record("odt.export.group", ModelOutcome::Omitted);
            return Ok(());
        };
        if shape.flip_h || shape.flip_v || shape.rotation.is_some() {
            self.reporter
                .record("odt.export.shape_transform", ModelOutcome::Degraded);
        }
        let mut graphic = self.build_graphic_style(anchor);
        match shape.fill {
            Some(Fill::Solid(color)) => {
                // Only the RGB channels map to `draw:fill-color`; a translucent fill
                // (a < 255, only reachable from a non-ODF-origin model) loses its
                // alpha, reported rather than dropped silently.
                if color.a != 255 {
                    self.reporter
                        .record("odt.export.shape_fill_opacity", ModelOutcome::Degraded);
                }
                graphic.fill = Some((color.r, color.g, color.b));
            }
            Some(Fill::Gradient { .. }) => {
                self.reporter
                    .record("odt.export.shape_fill_gradient", ModelOutcome::Degraded);
                graphic.fill_none = true;
            }
            None => graphic.fill_none = true,
        }
        if let Some(stroke) = &shape.stroke {
            if stroke.dash.is_some() || stroke.head_end.is_some() || stroke.tail_end.is_some() {
                self.reporter
                    .record("odt.export.shape_stroke_detail", ModelOutcome::Degraded);
            }
            if stroke.color.a != 255 {
                self.reporter
                    .record("odt.export.shape_stroke_opacity", ModelOutcome::Degraded);
            }
            graphic.stroke = Some((
                (stroke.color.r, stroke.color.g, stroke.color.b),
                stroke.width_emu,
            ));
        } else {
            graphic.stroke_none = true;
        }
        let graphic_name = (!graphic.is_empty()).then(|| {
            let name = graphic.name();
            self.graphic_styles.insert(graphic);
            name
        });
        let anchor_type = self.anchor_type_name(
            anchor.horizontal.relative_from,
            anchor.vertical.relative_from,
        );
        let x_emu = self.anchor_offset_emu(horizontal_position_offset(anchor.horizontal.position));
        let y_emu = self.anchor_offset_emu(vertical_position_offset(anchor.vertical.position));
        self.push("<")?;
        self.push(element)?;
        self.push(" text:anchor-type=\"")?;
        self.push(anchor_type)?;
        self.push("\"")?;
        if let Some(name) = &graphic_name {
            self.push(" draw:style-name=\"")?;
            self.push(name)?;
            self.push("\"")?;
        }
        if let Some(z_index) = group.relative_height {
            self.push(" draw:z-index=\"")?;
            self.push(&z_index.to_string())?;
            self.push("\"")?;
        }
        self.push(" svg:x=\"")?;
        self.push(&emu_to_cm(x_emu))?;
        self.push("\" svg:y=\"")?;
        self.push(&emu_to_cm(y_emu))?;
        self.push("\" svg:width=\"")?;
        self.push(&emu_to_cm(group.extent.width_emu))?;
        self.push("\" svg:height=\"")?;
        self.push(&emu_to_cm(group.extent.height_emu))?;
        self.push("\"/>")?;
        Ok(())
    }

    /// Writes a standalone anchored line (a single-child group carrying one
    /// `Line`-geometry `GroupShape`) as a positioned `draw:line`. The endpoints are
    /// reconstructed from the group's anchor offset, its extent, and the shape's
    /// flip pair: `flip_h`/`flip_v` select which corner of the bounding box each end
    /// sits at, the exact inverse of the importer's `min`/`|delta|`/`>` mapping. A
    /// line carries only an outline (no fill); a fill on the model shape is ignored.
    fn write_group_line(
        &mut self,
        group: &WordprocessingGroup,
        shape: &GroupShape,
    ) -> Result<(), OdfError> {
        let Some(anchor) = &group.anchor else {
            self.reporter
                .record("odt.export.group", ModelOutcome::Omitted);
            return Ok(());
        };
        let mut graphic = self.build_graphic_style(anchor);
        if let Some(stroke) = &shape.stroke {
            if stroke.dash.is_some() || stroke.head_end.is_some() || stroke.tail_end.is_some() {
                self.reporter
                    .record("odt.export.shape_stroke_detail", ModelOutcome::Degraded);
            }
            if stroke.color.a != 255 {
                self.reporter
                    .record("odt.export.shape_stroke_opacity", ModelOutcome::Degraded);
            }
            graphic.stroke = Some((
                (stroke.color.r, stroke.color.g, stroke.color.b),
                stroke.width_emu,
            ));
        } else {
            graphic.stroke_none = true;
        }
        // A line has no fill; a solid/gradient fill on the model shape is not
        // representable on a `draw:line` and is reported rather than emitted.
        if shape.fill.is_some() {
            self.reporter
                .record("odt.export.line_fill", ModelOutcome::Degraded);
        }
        let graphic_name = (!graphic.is_empty()).then(|| {
            let name = graphic.name();
            self.graphic_styles.insert(graphic);
            name
        });
        let anchor_type = self.anchor_type_name(
            anchor.horizontal.relative_from,
            anchor.vertical.relative_from,
        );
        let origin_x =
            self.anchor_offset_emu(horizontal_position_offset(anchor.horizontal.position));
        let origin_y = self.anchor_offset_emu(vertical_position_offset(anchor.vertical.position));
        let width = group.extent.width_emu;
        let height = group.extent.height_emu;
        // Invert the importer: the bounding box is [origin, origin+extent]; the flip
        // pair chooses which corner is (x1,y1) vs (x2,y2). `flip_h` ⇔ x1 > x2,
        // `flip_v` ⇔ y1 > y2, so re-import recovers the same offset/extent/flip.
        let (x1, x2) = if shape.flip_h {
            (origin_x + width, origin_x)
        } else {
            (origin_x, origin_x + width)
        };
        let (y1, y2) = if shape.flip_v {
            (origin_y + height, origin_y)
        } else {
            (origin_y, origin_y + height)
        };
        self.push("<draw:line text:anchor-type=\"")?;
        self.push(anchor_type)?;
        self.push("\"")?;
        if let Some(name) = &graphic_name {
            self.push(" draw:style-name=\"")?;
            self.push(name)?;
            self.push("\"")?;
        }
        if let Some(z_index) = group.relative_height {
            self.push(" draw:z-index=\"")?;
            self.push(&z_index.to_string())?;
            self.push("\"")?;
        }
        self.push(" svg:x1=\"")?;
        self.push(&emu_to_cm(x1))?;
        self.push("\" svg:y1=\"")?;
        self.push(&emu_to_cm(y1))?;
        self.push("\" svg:x2=\"")?;
        self.push(&emu_to_cm(x2))?;
        self.push("\" svg:y2=\"")?;
        self.push(&emu_to_cm(y2))?;
        self.push("\"/>")?;
        Ok(())
    }

    /// Emits a frame's alt text as `svg:title`, shared by the inline and anchored
    /// frame writers (single-line: tab/CR/LF folded to a space, other non-XML
    /// characters dropped).
    fn write_frame_title(&mut self, descr: Option<&str>) -> Result<(), OdfError> {
        let Some(descr) = descr else {
            return Ok(());
        };
        let sanitized: String = descr
            .chars()
            .filter_map(|character| {
                if matches!(character, '\t' | '\n' | '\r') {
                    Some(' ')
                } else if is_xml_character(character) {
                    Some(character)
                } else {
                    None
                }
            })
            .collect();
        let trimmed = sanitized.trim();
        if !trimmed.is_empty() {
            self.push("<svg:title>")?;
            self.push(&quick_xml::escape::escape(trimmed))?;
            self.push("</svg:title>")?;
        }
        Ok(())
    }

    fn write_alt(
        &mut self,
        description: Option<&str>,
        feature: &'static str,
    ) -> Result<(), OdfError> {
        if let Some(description) = description {
            self.reporter.record(feature, ModelOutcome::Degraded);
            self.write_text(description)
        } else {
            self.reporter.record(feature, ModelOutcome::Omitted);
            Ok(())
        }
    }

    fn write_text(&mut self, text: &str) -> Result<(), OdfError> {
        self.text_bytes = checked_add(
            self.text_bytes,
            text.len(),
            "odt_export_text_bytes",
            self.limits.max_text_bytes,
        )?;
        let mut plain = String::new();
        let mut characters = text.chars().peekable();
        while let Some(character) = characters.next() {
            match character {
                ' ' => {
                    self.flush_plain(&mut plain)?;
                    let mut count = 1_usize;
                    while characters.peek() == Some(&' ') {
                        characters.next();
                        count += 1;
                    }
                    if count == 1 {
                        self.push("<text:s/>")?;
                    } else {
                        self.push("<text:s text:c=\"")?;
                        self.push(&count.to_string())?;
                        self.push("\"/>")?;
                    }
                }
                '\t' => {
                    self.flush_plain(&mut plain)?;
                    self.push("<text:tab/>")?;
                }
                '\r' => {
                    self.flush_plain(&mut plain)?;
                    if characters.peek() == Some(&'\n') {
                        characters.next();
                    }
                    self.push("<text:line-break/>")?;
                }
                '\n' => {
                    self.flush_plain(&mut plain)?;
                    self.push("<text:line-break/>")?;
                }
                value if is_xml_character(value) => plain.push(value),
                _ => return Err(OdfError::InvalidXmlCharacter),
            }
        }
        self.flush_plain(&mut plain)
    }

    fn flush_plain(&mut self, plain: &mut String) -> Result<(), OdfError> {
        if plain.is_empty() {
            return Ok(());
        }
        let escaped = quick_xml::escape::escape(plain.as_str()).into_owned();
        plain.clear();
        self.push(&escaped)
    }

    /// Emits `text` as escaped element `#PCDATA` — `&`/`<`/`>` escaped, whitespace
    /// left literal. Unlike `write_text`, it does NOT re-encode spaces/tabs/breaks
    /// as `text:s`/`text:tab`/`text:line-break`: those ODF whitespace elements are
    /// not valid children of a Dublin Core `#PCDATA` element (`dc:creator`,
    /// `dc:date`), and the annotation importer only decodes them inside a body
    /// paragraph, so running them over author/date would emit schema-invalid
    /// output and drop every whitespace character on re-import (a fixed-point
    /// break). Callers must pre-check `is_representable`.
    fn write_pcdata(&mut self, text: &str) -> Result<(), OdfError> {
        self.text_bytes = checked_add(
            self.text_bytes,
            text.len(),
            "odt_export_text_bytes",
            self.limits.max_text_bytes,
        )?;
        let escaped = quick_xml::escape::escape(text).into_owned();
        self.push(&escaped)
    }
}

fn table_row_width(row: &TableRow) -> Result<usize, OdfError> {
    row.cells.iter().try_fold(0_usize, |width, cell| {
        width
            .checked_add(cell.properties.grid_span.unwrap_or(1) as usize)
            .ok_or(OdfError::SerializationFailed)
    })
}

fn analyze_table_merges(table: &Table) -> Result<TableMergeAnalysis, OdfError> {
    let mut analysis = TableMergeAnalysis::default();
    let mut active = BTreeMap::<usize, ActiveVerticalMerge>::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut previous = std::mem::take(&mut active);
        let mut column = 0_usize;
        for (cell_index, cell) in row.cells.iter().enumerate() {
            let span = cell.properties.grid_span.unwrap_or(1);
            let coordinate = (row_index, cell_index);
            match cell.properties.vertical_merge {
                Some(VerticalMerge::Restart) => {
                    analysis.row_spans.insert(coordinate, 1);
                    active.insert(
                        column,
                        ActiveVerticalMerge {
                            anchor: coordinate,
                            span,
                        },
                    );
                }
                Some(VerticalMerge::Continue) if canonical_continuation_cell(cell) => {
                    if let Some(previous_merge) = previous.remove(&column)
                        && previous_merge.span == span
                    {
                        let row_span = analysis
                            .row_spans
                            .get_mut(&previous_merge.anchor)
                            .ok_or(OdfError::SerializationFailed)?;
                        *row_span = row_span
                            .checked_add(1)
                            .ok_or(OdfError::SerializationFailed)?;
                        analysis.continuations.insert(coordinate);
                        active.insert(column, previous_merge);
                    }
                }
                Some(VerticalMerge::Continue) | None => {}
            }
            column = column
                .checked_add(span as usize)
                .ok_or(OdfError::SerializationFailed)?;
        }
    }
    Ok(analysis)
}

fn canonical_continuation_cell(cell: &TableCell) -> bool {
    let mut properties = cell.properties.clone();
    properties.grid_span = None;
    properties.vertical_merge = None;
    properties == TableCellProperties::default()
        && matches!(
            cell.blocks.as_slice(),
            [BlockNode::Paragraph(paragraph)]
                if paragraph.properties == ParagraphProperties::default()
                    && paragraph.inlines.is_empty()
        )
}

fn odt_list_label(
    level: u8,
    format: Option<&NumberFormat>,
    template: Option<&str>,
) -> Option<OdtListLabel> {
    match format {
        Some(NumberFormat::Bullet) => {
            let glyph = template.unwrap_or("•");
            let mut characters = glyph.chars();
            let character = characters.next()?;
            if characters.next().is_some() || !is_xml_character(character) {
                return None;
            }
            Some(OdtListLabel::Bullet(glyph.to_owned()))
        }
        Some(format) => {
            let format = match format {
                NumberFormat::Decimal => "1",
                NumberFormat::LowerLetter => "a",
                NumberFormat::UpperLetter => "A",
                NumberFormat::LowerRoman => "i",
                NumberFormat::UpperRoman => "I",
                _ => return None,
            };
            let placeholder = format!("%{}", u16::from(level) + 1);
            let template = template
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{placeholder}."));
            let (prefix, suffix) = template.split_once(&placeholder)?;
            if prefix.contains('%')
                || suffix.contains('%')
                || !prefix.chars().all(is_xml_character)
                || !suffix.chars().all(is_xml_character)
            {
                return None;
            }
            Some(OdtListLabel::Number {
                format,
                prefix: prefix.to_owned(),
                suffix: suffix.to_owned(),
            })
        }
        None => None,
    }
}

fn odt_list_style_name(levels: &BTreeMap<u8, OdtListLevel>) -> String {
    let mut hash = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128;
    for level in levels.values() {
        hash_bytes_128(&mut hash, &[level.level]);
        hash_bytes_128(&mut hash, &level.start.to_le_bytes());
        match &level.label {
            OdtListLabel::Bullet(glyph) => {
                hash_bytes_128(&mut hash, b"bullet");
                hash_bytes_128(&mut hash, glyph.as_bytes());
            }
            OdtListLabel::Number {
                format,
                prefix,
                suffix,
            } => {
                hash_bytes_128(&mut hash, b"number");
                hash_bytes_128(&mut hash, format.as_bytes());
                hash_bytes_128(&mut hash, prefix.as_bytes());
                hash_bytes_128(&mut hash, suffix.as_bytes());
            }
        }
    }
    format!("L_{hash:032x}")
}

fn hash_bytes_128(hash: &mut u128, bytes: &[u8]) {
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    for byte in bytes {
        *hash ^= u128::from(*byte);
        *hash = hash.wrapping_mul(PRIME);
    }
    *hash ^= 0xff;
    *hash = hash.wrapping_mul(PRIME);
}

#[allow(clippy::too_many_arguments)]
fn automatic_styles_xml(
    paragraph_styles: &BTreeSet<OdtParagraphStyle>,
    run_styles: &BTreeSet<OdtRunStyle>,
    list_styles: &BTreeMap<NumberingInstanceId, OdtListStyle>,
    column_styles: &BTreeSet<i32>,
    cell_styles: &BTreeSet<OdtCellStyle>,
    row_styles: &BTreeSet<OdtRowStyle>,
    table_styles: &BTreeSet<OdtTableStyle>,
    graphic_styles: &BTreeSet<OdtGraphicStyle>,
    max_content_bytes: usize,
) -> Result<String, OdfError> {
    if paragraph_styles.is_empty()
        && run_styles.is_empty()
        && list_styles.is_empty()
        && column_styles.is_empty()
        && cell_styles.is_empty()
        && row_styles.is_empty()
        && table_styles.is_empty()
        && graphic_styles.is_empty()
    {
        return Ok(String::new());
    }
    let mut xml = String::new();
    push_bounded(&mut xml, "<office:automatic-styles>", max_content_bytes)?;
    for &width in column_styles {
        push_bounded(&mut xml, "<style:style style:name=\"", max_content_bytes)?;
        push_bounded(&mut xml, &column_style_name(width), max_content_bytes)?;
        push_bounded(
            &mut xml,
            "\" style:family=\"table-column\"><style:table-column-properties style:column-width=\"",
            max_content_bytes,
        )?;
        push_bounded(&mut xml, &twips_to_pt(width), max_content_bytes)?;
        push_bounded(&mut xml, "\"/></style:style>", max_content_bytes)?;
    }
    for style in cell_styles {
        push_bounded(&mut xml, "<style:style style:name=\"", max_content_bytes)?;
        push_bounded(&mut xml, &style.name(), max_content_bytes)?;
        push_bounded(
            &mut xml,
            "\" style:family=\"table-cell\"><style:table-cell-properties",
            max_content_bytes,
        )?;
        push_cell_properties(&mut xml, style, max_content_bytes)?;
        push_bounded(&mut xml, "/></style:style>", max_content_bytes)?;
    }
    for style in row_styles {
        push_bounded(&mut xml, "<style:style style:name=\"", max_content_bytes)?;
        push_bounded(&mut xml, &style.name(), max_content_bytes)?;
        push_bounded(
            &mut xml,
            "\" style:family=\"table-row\"><style:table-row-properties",
            max_content_bytes,
        )?;
        push_row_properties(&mut xml, style, max_content_bytes)?;
        push_bounded(&mut xml, "/></style:style>", max_content_bytes)?;
    }
    for style in table_styles {
        push_bounded(&mut xml, "<style:style style:name=\"", max_content_bytes)?;
        push_bounded(&mut xml, &style.name(), max_content_bytes)?;
        push_bounded(
            &mut xml,
            "\" style:family=\"table\"><style:table-properties",
            max_content_bytes,
        )?;
        push_table_properties(&mut xml, style, max_content_bytes)?;
        push_bounded(&mut xml, "/></style:style>", max_content_bytes)?;
    }
    for style in paragraph_styles {
        push_bounded(&mut xml, "<style:style style:name=\"", max_content_bytes)?;
        push_bounded(&mut xml, &style.name(), max_content_bytes)?;
        push_bounded(
            &mut xml,
            "\" style:family=\"paragraph\"><style:paragraph-properties",
            max_content_bytes,
        )?;
        push_paragraph_properties(&mut xml, style, max_content_bytes)?;
        push_bounded(&mut xml, "/></style:style>", max_content_bytes)?;
    }
    for style in run_styles {
        push_bounded(&mut xml, "<style:style style:name=\"", max_content_bytes)?;
        push_bounded(&mut xml, &style.name(), max_content_bytes)?;
        push_bounded(
            &mut xml,
            "\" style:family=\"text\"><style:text-properties",
            max_content_bytes,
        )?;
        push_run_text_properties(&mut xml, style, max_content_bytes)?;
        push_bounded(&mut xml, "/></style:style>", max_content_bytes)?;
    }
    for style in graphic_styles {
        push_bounded(&mut xml, "<style:style style:name=\"", max_content_bytes)?;
        push_bounded(&mut xml, &style.name(), max_content_bytes)?;
        push_bounded(
            &mut xml,
            "\" style:family=\"graphic\"><style:graphic-properties",
            max_content_bytes,
        )?;
        push_graphic_properties(&mut xml, style, max_content_bytes)?;
        push_bounded(&mut xml, "/></style:style>", max_content_bytes)?;
    }
    let mut unique_list_styles = BTreeMap::<&str, &OdtListStyle>::new();
    for style in list_styles.values() {
        if let Some(previous) = unique_list_styles.insert(&style.name, style)
            && previous.levels != style.levels
        {
            return Err(OdfError::SerializationFailed);
        }
    }
    for style in unique_list_styles.into_values() {
        push_bounded(
            &mut xml,
            "<text:list-style style:name=\"",
            max_content_bytes,
        )?;
        push_bounded(&mut xml, &style.name, max_content_bytes)?;
        push_bounded(&mut xml, "\">", max_content_bytes)?;
        for level in style.levels.values() {
            match &level.label {
                OdtListLabel::Bullet(glyph) => {
                    push_bounded(
                        &mut xml,
                        "<text:list-level-style-bullet text:level=\"",
                        max_content_bytes,
                    )?;
                    push_bounded(
                        &mut xml,
                        &(u16::from(level.level) + 1).to_string(),
                        max_content_bytes,
                    )?;
                    push_bounded(&mut xml, "\" text:bullet-char=\"", max_content_bytes)?;
                    push_escaped_attribute(&mut xml, glyph, max_content_bytes)?;
                    push_bounded(&mut xml, "\"/>", max_content_bytes)?;
                }
                OdtListLabel::Number {
                    format,
                    prefix,
                    suffix,
                } => {
                    push_bounded(
                        &mut xml,
                        "<text:list-level-style-number text:level=\"",
                        max_content_bytes,
                    )?;
                    push_bounded(
                        &mut xml,
                        &(u16::from(level.level) + 1).to_string(),
                        max_content_bytes,
                    )?;
                    push_bounded(&mut xml, "\" style:num-format=\"", max_content_bytes)?;
                    push_bounded(&mut xml, format, max_content_bytes)?;
                    push_bounded(&mut xml, "\" style:num-prefix=\"", max_content_bytes)?;
                    push_escaped_attribute(&mut xml, prefix, max_content_bytes)?;
                    push_bounded(&mut xml, "\" style:num-suffix=\"", max_content_bytes)?;
                    push_escaped_attribute(&mut xml, suffix, max_content_bytes)?;
                    if level.start != 1 {
                        push_bounded(&mut xml, "\" text:start-value=\"", max_content_bytes)?;
                        push_bounded(&mut xml, &level.start.to_string(), max_content_bytes)?;
                    }
                    push_bounded(&mut xml, "\"/>", max_content_bytes)?;
                }
            }
        }
        push_bounded(&mut xml, "</text:list-style>", max_content_bytes)?;
    }
    push_bounded(&mut xml, "</office:automatic-styles>", max_content_bytes)?;
    Ok(xml)
}

/// Serializes the supported `<style:text-properties>` attribute subset for a run
/// style; the caller emits the enclosing element and its closing tag.
fn push_run_text_properties(
    xml: &mut String,
    style: &OdtRunStyle,
    max_content_bytes: usize,
) -> Result<(), OdfError> {
    if let Some(bold) = style.bold {
        push_bounded(
            xml,
            if bold {
                " fo:font-weight=\"bold\""
            } else {
                " fo:font-weight=\"normal\""
            },
            max_content_bytes,
        )?;
    }
    if let Some(italic) = style.italic {
        push_bounded(
            xml,
            if italic {
                " fo:font-style=\"italic\""
            } else {
                " fo:font-style=\"normal\""
            },
            max_content_bytes,
        )?;
    }
    if let Some(underline) = style.underline {
        push_bounded(
            xml,
            if underline {
                " style:text-underline-style=\"solid\""
            } else {
                " style:text-underline-style=\"none\""
            },
            max_content_bytes,
        )?;
    }
    if let Some(strike) = style.strike {
        push_bounded(
            xml,
            if strike {
                " style:text-line-through-style=\"solid\""
            } else {
                " style:text-line-through-style=\"none\""
            },
            max_content_bytes,
        )?;
    }
    if let Some((red, green, blue)) = style.color {
        push_bounded(xml, " fo:color=\"#", max_content_bytes)?;
        push_bounded(
            xml,
            &format!("{red:02x}{green:02x}{blue:02x}"),
            max_content_bytes,
        )?;
        push_bounded(xml, "\"", max_content_bytes)?;
    }
    if let Some(size) = style.size_half_points {
        push_bounded(xml, " fo:font-size=\"", max_content_bytes)?;
        push_bounded(xml, &(size / 2).to_string(), max_content_bytes)?;
        if size % 2 != 0 {
            push_bounded(xml, ".5", max_content_bytes)?;
        }
        push_bounded(xml, "pt\"", max_content_bytes)?;
    }
    if let Some(family) = &style.font_family {
        push_bounded(xml, " fo:font-family=\"", max_content_bytes)?;
        push_escaped_attribute(xml, family, max_content_bytes)?;
        push_bounded(xml, "\"", max_content_bytes)?;
    }
    if let Some(super_sub) = style.super_sub {
        push_bounded(
            xml,
            if super_sub {
                " style:text-position=\"super\""
            } else {
                " style:text-position=\"sub\""
            },
            max_content_bytes,
        )?;
    }
    if let Some(all_caps) = style.all_caps {
        push_bounded(
            xml,
            if all_caps {
                " fo:text-transform=\"uppercase\""
            } else {
                " fo:text-transform=\"none\""
            },
            max_content_bytes,
        )?;
    }
    if let Some(small_caps) = style.small_caps {
        push_bounded(
            xml,
            if small_caps {
                " fo:font-variant=\"small-caps\""
            } else {
                " fo:font-variant=\"normal\""
            },
            max_content_bytes,
        )?;
    }
    Ok(())
}

/// Serializes the `<office:styles>` document defaults for the supported subset
/// (paragraph alignment and the direct run subset). Returns an empty string when
/// nothing supported is present; unsupported default detail is reported.
fn default_styles_xml(
    defaults: Option<&DocumentDefaults>,
    styles: &DefinitionMap<StyleId, Style>,
    named_styles: &BTreeMap<StyleId, String>,
    named_paragraph_style_names: &BTreeMap<StyleId, String>,
    reporter: &mut Reporter,
    max_content_bytes: usize,
) -> Result<String, OdfError> {
    let mut body = String::new();
    if let Some(defaults) = defaults {
        default_style_defaults(defaults, &mut body, reporter, max_content_bytes)?;
    }
    named_character_styles(styles, named_styles, &mut body, reporter, max_content_bytes)?;
    named_paragraph_styles(
        styles,
        named_paragraph_style_names,
        &mut body,
        reporter,
        max_content_bytes,
    )?;
    if body.is_empty() {
        return Ok(String::new());
    }
    Ok(format!("<office:styles>{body}</office:styles>"))
}

/// Serializes the supported `<style:default-style>` subset (paragraph alignment
/// and the direct run subset) into `body`; unsupported default detail is reported.
fn default_style_defaults(
    defaults: &DocumentDefaults,
    body: &mut String,
    reporter: &mut Reporter,
    max_content_bytes: usize,
) -> Result<(), OdfError> {
    if let Some(paragraph) = &defaults.paragraph {
        let mut remainder = paragraph.clone();
        let alignment = remainder.alignment.take().map(OdtParagraphAlignment::from);
        if remainder != ParagraphProperties::default() {
            reporter.record(
                "odt.export.document_defaults.paragraph",
                ModelOutcome::Omitted,
            );
        }
        if let Some(alignment) = alignment {
            push_bounded(
                body,
                "<style:default-style style:family=\"paragraph\"><style:paragraph-properties fo:text-align=\"",
                max_content_bytes,
            )?;
            push_bounded(body, alignment.value(), max_content_bytes)?;
            push_bounded(body, "\"/></style:default-style>", max_content_bytes)?;
        }
    }
    if let Some(run) = &defaults.run {
        let (style, remainder) = split_run_properties(run);
        if remainder != RunProperties::default() {
            reporter.record("odt.export.document_defaults.run", ModelOutcome::Omitted);
        }
        if !style.is_empty() {
            push_bounded(
                body,
                "<style:default-style style:family=\"text\"><style:text-properties",
                max_content_bytes,
            )?;
            push_run_text_properties(body, &style, max_content_bytes)?;
            push_bounded(body, "/></style:default-style>", max_content_bytes)?;
        }
    }
    Ok(())
}

/// Serializes each `Character` style as a named `<style:style style:family="text">`
/// with its run subset, in `StyleId` order so both the emission and every run's
/// `text:style-name` reference stay deterministic. A style whose run properties fall
/// outside the direct subset, or that carries non-run detail (paragraph/table
/// properties, inheritance, flags), is reported so nothing is silently lost.
fn named_character_styles(
    styles: &DefinitionMap<StyleId, Style>,
    named_styles: &BTreeMap<StyleId, String>,
    body: &mut String,
    reporter: &mut Reporter,
    max_content_bytes: usize,
) -> Result<(), OdfError> {
    for (id, name) in named_styles.iter() {
        let Some(style) = styles.get(id) else {
            continue;
        };
        if style_has_common_unsupported_detail(style) || style.paragraph.is_some() {
            reporter.record("odt.export.character_style", ModelOutcome::Degraded);
        }
        let (run_style, remainder) = style
            .run
            .as_ref()
            .map(split_run_properties)
            .unwrap_or_default();
        if remainder != RunProperties::default() {
            reporter.record("odt.export.character_style.run", ModelOutcome::Omitted);
        }
        push_bounded(
            body,
            "<style:style style:family=\"text\" style:name=\"",
            max_content_bytes,
        )?;
        push_escaped_attribute(body, name, max_content_bytes)?;
        push_bounded(body, "\">", max_content_bytes)?;
        if !run_style.is_empty() {
            push_bounded(body, "<style:text-properties", max_content_bytes)?;
            push_run_text_properties(body, &run_style, max_content_bytes)?;
            push_bounded(body, "/>", max_content_bytes)?;
        }
        push_bounded(body, "</style:style>", max_content_bytes)?;
    }
    Ok(())
}

/// Serializes each named `Paragraph` style as a `<style:style style:family="paragraph">`
/// with its paragraph-property subset, in `StyleId` order. The paragraph analogue of
/// [`named_character_styles`]; a style whose paragraph properties fall outside the
/// supported subset, or that carries non-paragraph detail (run/table properties,
/// inheritance, flags), is reported so nothing is silently lost.
fn named_paragraph_styles(
    styles: &DefinitionMap<StyleId, Style>,
    named_paragraph_styles: &BTreeMap<StyleId, String>,
    body: &mut String,
    reporter: &mut Reporter,
    max_content_bytes: usize,
) -> Result<(), OdfError> {
    for (id, name) in named_paragraph_styles.iter() {
        let Some(style) = styles.get(id) else {
            continue;
        };
        if style_has_common_unsupported_detail(style) || style.run.is_some() {
            reporter.record("odt.export.paragraph_style", ModelOutcome::Degraded);
        }
        let mut remainder = style.paragraph.clone().unwrap_or_default();
        // A style does not reference another style; the projection maps only the
        // direct paragraph-formatting subset, so an outline level, numbering link,
        // or any other leftover property on the style is reported, not silently lost.
        remainder.style_ref = None;
        let dropped_outline = remainder.outline_level.take().is_some();
        let dropped_numbering = remainder.numbering.take().is_some();
        let paragraph_style = split_paragraph_properties(&mut remainder);
        if dropped_outline || dropped_numbering || remainder != ParagraphProperties::default() {
            reporter.record(
                "odt.export.paragraph_style.paragraph",
                ModelOutcome::Omitted,
            );
        }
        push_bounded(
            body,
            "<style:style style:family=\"paragraph\" style:name=\"",
            max_content_bytes,
        )?;
        push_escaped_attribute(body, name, max_content_bytes)?;
        push_bounded(body, "\">", max_content_bytes)?;
        if !paragraph_style.is_empty() {
            push_bounded(body, "<style:paragraph-properties", max_content_bytes)?;
            push_paragraph_properties(body, &paragraph_style, max_content_bytes)?;
            push_bounded(body, "/>", max_content_bytes)?;
        }
        push_bounded(body, "</style:style>", max_content_bytes)?;
    }
    Ok(())
}

/// Reports whether a style carries detail the named-style projection does not
/// represent, EXCLUDING the family's own property slot (a caller adds its own-slot
/// check — `run` for paragraph styles, `paragraph` for character styles). Covers
/// inheritance, the UI flags, and the table property slots.
fn style_has_common_unsupported_detail(style: &Style) -> bool {
    style.is_default
        || style.aliases.is_some()
        || style.based_on.is_some()
        || style.next.is_some()
        || style.link.is_some()
        || style.hidden
        || style.ui_priority.is_some()
        || style.semi_hidden
        || style.unhide_when_used
        || style.q_format
        || style.locked
        || style.table.is_some()
        || style.table_row.is_some()
        || style.table_cell.is_some()
        || !style.conditional.is_empty()
}

fn push_escaped_attribute(
    output: &mut String,
    value: &str,
    allowed: usize,
) -> Result<(), OdfError> {
    if !value.chars().all(is_xml_character) {
        return Err(OdfError::InvalidXmlCharacter);
    }
    let escaped = quick_xml::escape::escape(value);
    // quick_xml escapes the five predefined entities but not the whitespace
    // control chars, which XML attribute-value normalization would collapse to a
    // space on re-parse. Emit them as numeric character references so an
    // attribute value round-trips byte-exactly.
    if escaped.contains(['\t', '\n', '\r']) {
        let numeric = escaped
            .replace('\t', "&#9;")
            .replace('\n', "&#10;")
            .replace('\r', "&#13;");
        return push_bounded(output, &numeric, allowed);
    }
    push_bounded(output, escaped.as_ref(), allowed)
}

fn push_bounded(output: &mut String, value: &str, allowed: usize) -> Result<(), OdfError> {
    let observed = output
        .len()
        .checked_add(value.len())
        .ok_or(OdfError::LimitExceeded {
            limit: "odt_export_content_bytes",
            observed: usize::MAX,
            allowed,
        })?;
    enforce("odt_export_content_bytes", observed, allowed)?;
    output.push_str(value);
    Ok(())
}

/// Writes a validated normalized document as a deterministic ODF 1.4 package.
pub fn write_odt(document: &Document, limits: OdfExportLimits) -> Result<OdtExport, OdfError> {
    write_odt_impl(document, limits, BTreeMap::new(), None, CONTENT_HEADER)
}

/// Writes a preserving ODF 1.4 package: `Drawing` nodes whose source image bytes
/// are retained are re-emitted as `draw:frame`/`draw:image`, and those bytes are
/// repackaged with manifest entries. A drawing whose bytes are not retained still
/// degrades to the alt-text projection, so no dangling reference is written.
pub fn write_odt_with_retained_parts(
    document: &Document,
    retained: &crate::OdfRetainedParts,
    limits: OdfExportLimits,
) -> Result<OdtExport, OdfError> {
    // Only media still referenced by the (possibly edited) document and actually
    // retained are re-emitted and repackaged, and reserved/active-content names
    // are excluded even on this public path. The normalized part name is used
    // consistently for the draw:frame href, the ZIP entry, and the manifest, so
    // no orphan parts are written and the output is a byte fixed point.
    let used = referenced_retained_parts(document, retained);
    let mut available_media = BTreeMap::new();
    for (id, media) in document.definitions().media.iter() {
        let name = crate::package::normalized_part_path(&media.part_name);
        if used.parts.contains_key(&name) {
            available_media.insert(*id, name);
        }
    }
    write_odt_impl(
        document,
        limits,
        available_media,
        Some(&used),
        CONTENT_HEADER_PRESERVING,
    )
}

/// The subset of `retained` that `document`'s media actually references and that
/// is safe to repackage (excludes reserved/regenerated and active-content part
/// names), keyed by normalized part name. Exposed so a host can report exactly
/// how many parts a preserving export will carry.
pub fn referenced_retained_parts(
    document: &Document,
    retained: &crate::OdfRetainedParts,
) -> crate::OdfRetainedParts {
    let mut used = crate::OdfRetainedParts::default();
    // Media-referenced parts: only those a Drawing still references.
    for (_, media) in document.definitions().media.iter() {
        let name = crate::package::normalized_part_path(&media.part_name);
        if crate::package::is_unsafe_retained_name(&name) {
            continue;
        }
        if let Some(part) = retained.parts.get(&name) {
            used.parts.entry(name).or_insert_with(|| part.clone());
        }
    }
    // Unknown parts are carried verbatim regardless of edits. Skip a name a
    // caller may also have placed in `parts` (the two are disjoint on the capture
    // path but the fields are public), so a part is never written twice.
    for (name, part) in &retained.unknown {
        if !crate::package::is_unsafe_retained_name(name) && !used.parts.contains_key(name) {
            used.unknown.insert(name.clone(), part.clone());
        }
    }
    used
}

fn write_odt_impl(
    document: &Document,
    limits: OdfExportLimits,
    available_media: BTreeMap<MediaId, String>,
    retained: Option<&crate::OdfRetainedParts>,
    content_header: &str,
) -> Result<OdtExport, OdfError> {
    limits.validate()?;
    document.validate().map_err(|_| OdfError::InvalidModel)?;
    let mut writer = Writer::new(limits)?;
    writer.available_media = available_media;
    // Only the preserving writer's header declares `draw:`/`svg:` (see
    // CONTENT_HEADER_PRESERVING); `retained.is_some()` exactly identifies that path.
    writer.drawing_namespaces_available = retained.is_some();
    writer.register_numbering(document.definitions());
    writer.register_notes(document.definitions());
    writer.register_bookmarks(document.definitions());
    writer.register_named_styles(document.definitions());
    writer.register_revisions(document);
    let mut definition_remainder = document.definitions().clone();
    definition_remainder.abstract_numbering = Default::default();
    definition_remainder.numbering = Default::default();
    definition_remainder.footnotes = Default::default();
    definition_remainder.endnotes = Default::default();
    // Document defaults are emitted into styles.xml, so they are not a loss.
    definition_remainder.document_defaults = Default::default();
    // Character styles are emitted as named `style:style` elements in styles.xml
    // (any style whose non-run detail cannot be represented is reported there), so
    // clear them here rather than counting them as a definitions loss.
    definition_remainder.styles = Default::default();
    if definition_remainder != Definitions::default() {
        writer
            .reporter
            .record("odt.export.definitions", ModelOutcome::Omitted);
    }
    if document.background().is_some() {
        writer
            .reporter
            .record("odt.export.background", ModelOutcome::Omitted);
    }
    if let Some(properties) = document.properties() {
        if properties.custom.iter().any(|property| {
            matches!(
                property.value,
                casual_doc_model::v1::CustomValue::Other { .. }
            )
        }) || properties.app.company.is_some()
            || properties.app.manager.is_some()
            || properties.app.template.is_some()
            || properties.app.characters.is_some()
            || properties.app.paragraphs.is_some()
        {
            writer.reporter.record(
                "odt.export.document_properties.unsupported",
                ModelOutcome::Omitted,
            );
        }
        if properties.core.last_modified_by.is_some()
            || properties.core.revision.is_some()
            || properties.core.last_printed.is_some()
            || properties.core.category.is_some()
            || properties.core.content_status.is_some()
            || properties.core.version.is_some()
        {
            writer.reporter.record(
                "odt.export.document_properties.core_unsupported",
                ModelOutcome::Omitted,
            );
        }
    }
    // Emitted first inside office:text (nothing else is pushed to the body buffer
    // between BODY_PREFIX and here), so the change-regions are declared before the
    // body's change-start/-end markers reference them.
    writer.write_tracked_changes()?;
    writer.write_forms()?;
    writer.write_blocks(document.body(), 0)?;
    writer.report_unreferenced_notes();
    if writer.paragraphs_written == 0 {
        writer.push("<text:p/>")?;
    }
    writer.push(CONTENT_SUFFIX)?;
    // Render master-page header/footer fragments and document defaults before
    // finishing the reporter so their loss findings are captured; the content
    // buffer is restored intact.
    let master_page = writer.render_master_page(document)?;
    let default_styles = default_styles_xml(
        document.definitions().document_defaults.as_ref(),
        &document.definitions().styles,
        &writer.named_styles,
        &writer.named_paragraph_styles,
        &mut writer.reporter,
        limits.max_content_bytes,
    )?;
    let styles = automatic_styles_xml(
        &writer.paragraph_styles,
        &writer.run_styles,
        &writer.list_styles,
        &writer.column_styles,
        &writer.cell_styles,
        &writer.row_styles,
        &writer.table_styles,
        &writer.graphic_styles,
        limits.max_content_bytes,
    )?;
    let content_len = content_header
        .len()
        .checked_add(styles.len())
        .and_then(|value| value.checked_add(writer.xml.len()))
        // Header/footer and default-style markup live in styles.xml (built via a
        // buffer swap / separate string), so fold their bytes in here or they
        // would escape the content byte budget.
        .and_then(|value| value.checked_add(master_page.total_len()))
        .and_then(|value| value.checked_add(default_styles.len()))
        .ok_or(OdfError::LimitExceeded {
            limit: "odt_export_content_bytes",
            observed: usize::MAX,
            allowed: limits.max_content_bytes,
        })?;
    enforce(
        "odt_export_content_bytes",
        content_len,
        limits.max_content_bytes,
    )?;
    let mut content = String::with_capacity(content_len);
    content.push_str(content_header);
    content.push_str(&styles);
    content.push_str(&writer.xml);
    let content = content.into_bytes();
    let report = writer.reporter.finish();
    let metadata = document
        .properties()
        .filter(|properties| !properties.is_empty())
        .map(metadata_xml)
        .transpose()?;
    let page_styles = page_styles_xml(document, &master_page, &default_styles);
    let empty_retained = crate::OdfRetainedParts::default();
    let bytes = package(
        &content,
        page_styles.as_deref(),
        metadata.as_deref(),
        retained.unwrap_or(&empty_retained),
        limits,
    )?;
    Ok(OdtExport { bytes, report })
}

fn package(
    content: &[u8],
    page_styles: Option<&[u8]>,
    metadata: Option<&[u8]>,
    retained: &crate::OdfRetainedParts,
    limits: OdfExportLimits,
) -> Result<Vec<u8>, OdfError> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file(MIMETYPE_PART, stored)
        .map_err(|_| OdfError::SerializationFailed)?;
    zip.write_all(ODT_MIME.as_bytes())
        .map_err(|_| OdfError::SerializationFailed)?;
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(crate::CONTENT_PART, deflated)
        .map_err(|_| OdfError::SerializationFailed)?;
    zip.write_all(content)
        .map_err(|_| OdfError::SerializationFailed)?;
    if let Some(page_styles) = page_styles {
        zip.start_file(crate::STYLES_PART, deflated)
            .map_err(|_| OdfError::SerializationFailed)?;
        zip.write_all(page_styles)
            .map_err(|_| OdfError::SerializationFailed)?;
    }
    if let Some(metadata) = metadata {
        zip.start_file(META_PART, deflated)
            .map_err(|_| OdfError::SerializationFailed)?;
        zip.write_all(metadata)
            .map_err(|_| OdfError::SerializationFailed)?;
    }
    // Retained parts are opaque bytes; store them verbatim in deterministic
    // (sorted) order after the semantic parts, before the manifest. Media-
    // referenced parts precede unknown parts; the two key sets are disjoint.
    for (name, part) in retained.parts.iter().chain(retained.unknown.iter()) {
        zip.start_file(name.as_str(), stored)
            .map_err(|_| OdfError::SerializationFailed)?;
        zip.write_all(&part.bytes)
            .map_err(|_| OdfError::SerializationFailed)?;
    }
    zip.start_file(MANIFEST_PART, deflated)
        .map_err(|_| OdfError::SerializationFailed)?;
    zip.write_all(&build_manifest(
        page_styles.is_some(),
        metadata.is_some(),
        retained,
    ))
    .map_err(|_| OdfError::SerializationFailed)?;
    let bytes = zip
        .finish()
        .map_err(|_| OdfError::SerializationFailed)?
        .into_inner();
    enforce(
        "odt_export_package_bytes",
        bytes.len(),
        limits.max_package_bytes,
    )?;
    Ok(bytes)
}

/// The ODF `draw:` element name for a preset geometry this increment can re-emit
/// as a standalone shape: `rect` and `ellipse` share the box-geometry form
/// (`svg:x`/`y`/`width`/`height`). Every other preset returns `None` and falls back
/// to the reported `odt.export.group` degrade.
fn box_shape_element(geometry: ShapeGeometry) -> Option<&'static str> {
    match geometry {
        ShapeGeometry::Rectangle => Some("draw:rect"),
        ShapeGeometry::Ellipse => Some("draw:ellipse"),
        _ => None,
    }
}

/// Whether `group` is exactly a shape this increment can re-emit as a standalone
/// box shape (`draw:rect`/`draw:ellipse`): one supported-geometry `GroupShape`
/// child, no adjustments or retained preset, at the group's origin, with an
/// identity (untransformed, unscaled) group transform. Anything else (a
/// picture/text-box/nested-group child, more than one child, an unsupported
/// geometry, or a real transform) is not this increment's shape and falls back to
/// the reported `odt.export.group` degrade — never a wrong or lossy element.
fn single_box_shape(group: &WordprocessingGroup) -> Option<(&GroupShape, &'static str)> {
    let [GroupChild::Shape(shape)] = group.children.as_slice() else {
        return None;
    };
    let element = box_shape_element(shape.geometry)?;
    if shape.preset.is_some() || !shape.adjustments.is_empty() {
        return None;
    }
    let origin = PointEmu { x_emu: 0, y_emu: 0 };
    if shape.offset != origin || shape.extent != group.extent {
        return None;
    }
    let transform = &group.transform;
    let identity = transform.offset == origin
        && transform.extent == group.extent
        && transform.child_offset == origin
        && transform.child_extent == group.extent
        && !transform.flip_h
        && !transform.flip_v
        && transform.rotation.is_none();
    identity.then_some((shape, element))
}

/// Whether `group` is exactly a shape this increment can re-emit as a standalone
/// `draw:line`: one `Line`-geometry `GroupShape` child at the group's origin with an
/// identity group transform and no rotation. Unlike a box shape, the SHAPE's own
/// `flip_h`/`flip_v` are permitted — they encode which diagonal of the bounding box
/// the line's endpoints span, and the writer reconstructs the endpoints from them.
fn single_line_shape(group: &WordprocessingGroup) -> Option<&GroupShape> {
    let [GroupChild::Shape(shape)] = group.children.as_slice() else {
        return None;
    };
    if shape.geometry != ShapeGeometry::Line
        || shape.preset.is_some()
        || !shape.adjustments.is_empty()
        || shape.rotation.is_some()
    {
        return None;
    }
    let origin = PointEmu { x_emu: 0, y_emu: 0 };
    if shape.offset != origin || shape.extent != group.extent {
        return None;
    }
    let transform = &group.transform;
    let identity = transform.offset == origin
        && transform.extent == group.extent
        && transform.child_offset == origin
        && transform.child_extent == group.extent
        && !transform.flip_h
        && !transform.flip_v
        && transform.rotation.is_none();
    identity.then_some(shape)
}

/// The absolute offset of a horizontal placement, or `None` for an alignment
/// (which this increment cannot represent in ODF and reports).
fn horizontal_position_offset(position: HorizontalPosition) -> Option<i64> {
    match position {
        HorizontalPosition::Offset(value) => Some(value),
        HorizontalPosition::Align(_) => None,
    }
}

/// The absolute offset of a vertical placement, or `None` for an alignment.
fn vertical_position_offset(position: VerticalPosition) -> Option<i64> {
    match position {
        VerticalPosition::Offset(value) => Some(value),
        VerticalPosition::Align(_) => None,
    }
}

/// Formats an EMU length as centimetres for `svg:width`/`svg:height` (1 cm =
/// 360000 EMU), matching the deterministic geometry unit formatting.
fn emu_to_cm(emu: i64) -> String {
    // Floor to 4 decimals so the re-parsed value never rounds *up* past the
    // source — in particular a max-size extent stays within the model domain
    // instead of being dropped on re-import.
    let centimetres = (emu as f64 / 360_000.0 * 10_000.0).floor() / 10_000.0;
    format!("{centimetres:.4}cm")
}

/// Builds the manifest deterministically. With no styles/meta/retained parts this
/// is byte-identical to prior releases (the fixed `MANIFEST` shape).
fn build_manifest(
    has_styles: bool,
    has_metadata: bool,
    retained: &crate::OdfRetainedParts,
) -> Vec<u8> {
    let mut manifest = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.4"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text" manifest:version="1.4"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>"#,
    );
    if has_styles {
        manifest.push_str(
            r#"<manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>"#,
        );
    }
    if has_metadata {
        manifest.push_str(
            r#"<manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>"#,
        );
    }
    for (name, part) in retained.parts.iter().chain(retained.unknown.iter()) {
        manifest.push_str("<manifest:file-entry manifest:full-path=\"");
        manifest.push_str(&quick_xml::escape::escape(name));
        manifest.push_str("\" manifest:media-type=\"");
        manifest.push_str(&quick_xml::escape::escape(&part.media_type));
        manifest.push_str("\"/>");
    }
    manifest.push_str("</manifest:manifest>");
    manifest.into_bytes()
}

/// Deterministic styles.xml header/footer fragments keyed by page type.
#[derive(Default)]
struct MasterPageXml {
    default_header: Option<String>,
    even_header: Option<String>,
    default_footer: Option<String>,
    even_footer: Option<String>,
}

impl MasterPageXml {
    fn is_empty(&self) -> bool {
        self.default_header.is_none()
            && self.even_header.is_none()
            && self.default_footer.is_none()
            && self.even_footer.is_none()
    }

    /// Total serialized fragment bytes, folded into the content byte budget so
    /// header/footer markup cannot escape `max_content_bytes` via the buffer swap.
    fn total_len(&self) -> usize {
        [
            &self.default_header,
            &self.even_header,
            &self.default_footer,
            &self.even_footer,
        ]
        .into_iter()
        .flatten()
        .map(String::len)
        .sum()
    }
}

/// Stores a rendered fragment in its slot. An empty fragment (a header/footer
/// whose blocks produced no output) is dropped so it neither emits an empty
/// region nor forces the text namespace on; the block-level loss is already
/// reported by the renderer. A second fragment for the same page type is a
/// duplicate loss finding rather than a second region.
fn store_master_slot(slot: &mut Option<String>, fragment: String, reporter: &mut Reporter) {
    if fragment.is_empty() {
        return;
    }
    if slot.is_some() {
        reporter.record("odt.export.header_footer.duplicate", ModelOutcome::Omitted);
        return;
    }
    *slot = Some(fragment);
}

fn page_styles_xml(
    document: &Document,
    master: &MasterPageXml,
    default_styles: &str,
) -> Option<Vec<u8>> {
    let section = document.definitions().sections.first();
    // styles.xml exists when there is page geometry or document defaults. A
    // master-page (headers/footers) only ever accompanies a section.
    if section.is_none() && default_styles.is_empty() {
        return None;
    }
    let automatic_styles = section.map(page_layout_xml).unwrap_or_default();
    // The text namespace and master-styles are only emitted when a header/footer
    // is present, so geometry-only output stays byte-identical to prior releases.
    let text_ns = if master.is_empty() {
        " "
    } else {
        " xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" "
    };
    let master_styles = master_styles_xml(master);
    // ODF orders office:styles, then office:automatic-styles, then master-styles.
    Some(format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-styles xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\"{text_ns}office:version=\"1.4\">{default_styles}{automatic_styles}{master_styles}</office:document-styles>").into_bytes())
}

/// Builds the `<office:automatic-styles>` page-layout block for one section.
fn page_layout_xml(section: &casual_doc_model::v1::SectionBoundary) -> String {
    let cm = |twips: i32| format!("{:.4}cm", f64::from(twips) * 2.54 / 1440.0);
    let orientation = matches!(
        section.orientation,
        Some(casual_doc_model::v1::PageOrientation::Landscape)
    )
    .then_some("landscape")
    .unwrap_or("portrait");
    let gap = section
        .columns
        .space_twips
        .map(|value| format!(" fo:column-gap=\"{}\"", cm(value)))
        .unwrap_or_default();
    let separator = section
        .columns
        .separator
        .map(|value| {
            format!(
                " style:column-sep=\"{}\"",
                if value { "true" } else { "false" }
            )
        })
        .unwrap_or_default();
    let writing_mode = section
        .text_direction
        .map(|value| {
            format!(
                " style:writing-mode=\"{}\"",
                match value {
                    casual_doc_model::v1::TextDirection::LrTb => "lr-tb",
                    casual_doc_model::v1::TextDirection::TbRl => "tb-rl",
                    casual_doc_model::v1::TextDirection::BtLr => "bt-lr",
                }
            )
        })
        .unwrap_or_default();
    format!(
        "<office:automatic-styles><style:page-layout style:name=\"pm1\"><style:page-layout-properties fo:page-width=\"{}\" fo:page-height=\"{}\" fo:margin-top=\"{}\" fo:margin-bottom=\"{}\" fo:margin-left=\"{}\" fo:margin-right=\"{}\" style:print-orientation=\"{}\" style:column-count=\"{}\"{}{}{} /></style:page-layout></office:automatic-styles>",
        cm(section.page_size.width_twips),
        cm(section.page_size.height_twips),
        cm(section.page_margins.top_twips),
        cm(section.page_margins.bottom_twips),
        cm(section.page_margins.start_twips),
        cm(section.page_margins.end_twips),
        orientation,
        section.columns.count,
        gap,
        separator,
        writing_mode
    )
}

/// Builds the `<office:master-styles>` block binding the page-layout to the
/// header/footer regions. ODF orders header regions before footer regions.
fn master_styles_xml(master: &MasterPageXml) -> String {
    if master.is_empty() {
        return String::new();
    }
    let mut xml = String::from(
        "<office:master-styles><style:master-page style:name=\"Standard\" style:page-layout-name=\"pm1\">",
    );
    if let Some(fragment) = &master.default_header {
        xml.push_str("<style:header>");
        xml.push_str(fragment);
        xml.push_str("</style:header>");
    }
    if let Some(fragment) = &master.even_header {
        xml.push_str("<style:header-left>");
        xml.push_str(fragment);
        xml.push_str("</style:header-left>");
    }
    if let Some(fragment) = &master.default_footer {
        xml.push_str("<style:footer>");
        xml.push_str(fragment);
        xml.push_str("</style:footer>");
    }
    if let Some(fragment) = &master.even_footer {
        xml.push_str("<style:footer-left>");
        xml.push_str(fragment);
        xml.push_str("</style:footer-left>");
    }
    xml.push_str("</style:master-page></office:master-styles>");
    xml
}

fn metadata_xml(
    properties: &casual_doc_model::v1::DocumentProperties,
) -> Result<Vec<u8>, OdfError> {
    use quick_xml::escape::escape;
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" office:version="1.4"><office:meta>"#,
    );
    let mut field = |tag: &str, value: &Option<String>| {
        if let Some(value) = value {
            xml.push('<');
            xml.push_str(tag);
            xml.push('>');
            xml.push_str(&escape(value));
            xml.push_str("</");
            xml.push_str(tag);
            xml.push('>');
        }
    };
    field("dc:title", &properties.core.title);
    field("dc:subject", &properties.core.subject);
    field("dc:creator", &properties.core.creator);
    field("dc:description", &properties.core.description);
    field("dc:language", &properties.core.language);
    // ODF stores the creation timestamp as meta:creation-date and the last
    // modification as dc:date (dcterms:* is not part of the ODF meta schema and
    // is not read back by the importer). Emitting the native elements keeps the
    // round trip idempotent and interoperable with LibreOffice/Word.
    field("meta:creation-date", &properties.core.created);
    field("dc:date", &properties.core.modified);
    if let Some(value) = &properties.core.keywords {
        field("meta:keyword", &Some(value.clone()));
    }
    if let Some(value) = &properties.app.application {
        field("meta:generator", &Some(value.clone()));
    }
    if let Some(value) = properties.app.pages {
        xml.push_str(&format!(
            "<meta:document-statistic meta:page-count=\"{value}\"/>"
        ));
    }
    if let Some(value) = properties.app.words {
        xml.push_str(&format!(
            "<meta:document-statistic meta:word-count=\"{value}\"/>"
        ));
    }
    if let Some(value) = properties.app.total_time {
        let hours = value / 60;
        let minutes = value % 60;
        xml.push_str(&format!(
            "<meta:editing-duration>PT{hours}H{minutes}M</meta:editing-duration>"
        ));
    }
    for property in &properties.custom {
        let (kind, value) = match &property.value {
            casual_doc_model::v1::CustomValue::Text { value } => ("string", value.clone()),
            casual_doc_model::v1::CustomValue::I4 { value } => ("long", value.to_string()),
            casual_doc_model::v1::CustomValue::R8 { value } => ("float", value.clone()),
            casual_doc_model::v1::CustomValue::Bool { value } => ("boolean", value.to_string()),
            casual_doc_model::v1::CustomValue::FileTime { value } => ("date", value.clone()),
            casual_doc_model::v1::CustomValue::Other { value, .. } => ("string", value.clone()),
        };
        xml.push_str("<meta:user-defined meta:name=\"");
        xml.push_str(&escape(&property.name));
        xml.push_str("\" meta:value-type=\"");
        xml.push_str(kind);
        xml.push_str("\">");
        xml.push_str(&escape(&value));
        xml.push_str("</meta:user-defined>");
    }
    xml.push_str("</office:meta></office:document-meta>");
    Ok(xml.into_bytes())
}

fn checked_add(
    value: usize,
    add: usize,
    limit: &'static str,
    allowed: usize,
) -> Result<usize, OdfError> {
    let observed = value.checked_add(add).ok_or(OdfError::LimitExceeded {
        limit,
        observed: usize::MAX,
        allowed,
    })?;
    enforce(limit, observed, allowed)?;
    Ok(observed)
}

fn enforce(limit: &'static str, observed: usize, allowed: usize) -> Result<(), OdfError> {
    if observed > allowed {
        Err(OdfError::LimitExceeded {
            limit,
            observed,
            allowed,
        })
    } else {
        Ok(())
    }
}

fn is_xml_character(character: char) -> bool {
    // Tab, LF, and CR are legal XML 1.0 `Char`s alongside the printable ranges.
    matches!(
        character as u32,
        0x09 | 0x0a | 0x0d | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff
    )
}

/// Whether every character of `value` can be serialized into ODF output. Used to
/// guard attribute values (bookmark names, hrefs, tooltips) so a value the model
/// permits but we cannot emit degrades gracefully rather than aborting the whole
/// export.
fn is_representable(value: &str) -> bool {
    value.chars().all(is_xml_character)
}

/// Whether a block content control is a table-of-contents (a building-block
/// gallery named "Table of Contents") — the shape the ODT importer mints and the
/// only `BlockSdt` re-emitted as `text:table-of-content`.
fn is_toc_sdt(sdt: &BlockSdt) -> bool {
    sdt.properties.control_kind == Some(SdtControlKind::BuildingBlockGallery)
        && sdt.properties.gallery.as_deref() == Some("Table of Contents")
}

/// Whether the writer projects a field's inner inlines (the degraded path) rather
/// than emitting a mapped ODF field element. Must stay in lockstep with the
/// `write_inlines` `InlineNode::Field` arm, so the revision pre-walk visits
/// exactly the inlines that will be emitted.
fn field_projects_inlines(field: &Field) -> bool {
    match &field.kind {
        FieldKind::Page | FieldKind::NumPages | FieldKind::Date { .. } | FieldKind::Time { .. } => {
            false
        }
        FieldKind::Ref { bookmark } | FieldKind::PageRef { bookmark } => {
            !is_representable(bookmark)
        }
        FieldKind::Seq { name } => !is_representable(name),
        _ => true,
    }
}

/// One region to declare in `text:tracked-changes`. `deleted_text` is `Some` for
/// A form control to declare in `office:forms`.
#[derive(Clone, Debug)]
enum FormControlOut {
    /// `form:text` (FORMTEXT).
    Text,
    /// `form:checkbox` (FORMCHECKBOX) with its current checked state.
    CheckBox(Option<bool>),
    /// `form:listbox` (FORMDROPDOWN) with its option entry labels.
    DropDown(Vec<String>),
}

/// a deletion (whose content lives in the region, not the body) and `None` for an
/// insertion.
#[derive(Clone, Debug)]
struct RevisionRegion {
    change_id: String,
    author: Option<String>,
    date: Option<String>,
    deleted_text: Option<String>,
}

/// Flattens a revision's inline content to plain text (concatenating run text and
/// recursing into wrappers) — the deleted content projection written into a
/// `text:deletion` region.
fn flatten_inline_text(inlines: &[InlineNode]) -> String {
    let mut text = String::new();
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => text.push_str(&run.text),
            InlineNode::Tab(_) => text.push('\t'),
            InlineNode::Break(_) => text.push('\n'),
            InlineNode::Hyperlink(link) => text.push_str(&flatten_inline_text(&link.inlines)),
            InlineNode::Revision(revision) => {
                text.push_str(&flatten_inline_text(&revision.inlines))
            }
            InlineNode::Sdt(sdt) => text.push_str(&flatten_inline_text(&sdt.inlines)),
            _ => {}
        }
    }
    text
}

/// Whether `value` is a valid XML NCName (a usable `text:change-id`): non-empty,
/// no colon, first char a letter or `_`, the rest letters/digits/`_`/`-`/`.`. A
/// model `revision_id` that fails this (e.g. a DOCX `w:id` of `"5"`) is replaced
/// by a minted id rather than emitted as an invalid attribute.
fn is_ncname(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use casual_doc_model::v1::{BlockNode, FontName, InlineNode, RgbColor};

    use super::*;
    use crate::{OdfImportLimits, OdfPackageLimits, OdfVersion, OdtPackage, import_content_xml};

    const CORE: &[u8] = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.4"><office:body><office:text><text:p>one<text:s text:c="2"/>two<text:tab/>three<text:line-break/>four</text:p><text:h text:outline-level="2">Title</text:h></office:text></office:body></office:document-content>"#;

    fn core_document() -> Document {
        import_content_xml(CORE, OdfVersion::V1_4, OdfImportLimits::default())
            .unwrap()
            .document
    }

    #[test]
    fn core_subset_is_deterministic_valid_and_semantically_stable() {
        let document = core_document();
        let first = write_odt(&document, OdfExportLimits::default()).unwrap();
        let second = write_odt(&document, OdfExportLimits::default()).unwrap();
        assert_eq!(first.bytes, second.bytes);
        assert!(first.report.entries.is_empty());
        let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
        let reopened = package.import_document(OdfImportLimits::default()).unwrap();
        assert_eq!(reopened.document, document);
    }

    #[test]
    fn supported_direct_formatting_uses_deterministic_automatic_styles() {
        let mut document = core_document();
        let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0] else {
            panic!("paragraph")
        };
        paragraph.properties.alignment = Some(Alignment::Center);
        let InlineNode::Run(run) = &mut paragraph.inlines[0] else {
            panic!("run")
        };
        run.properties.bold = Some(true);
        run.properties.italic = Some(false);
        run.properties.underline = Some(true);
        run.properties.strike = Some(false);
        run.properties.color = Some(Color::Rgb(RgbColor {
            r: 0x1a,
            g: 0x2b,
            b: 0x3c,
        }));
        run.properties.size_half_points = Some(21);

        let first = write_odt(&document, OdfExportLimits::default()).unwrap();
        let second = write_odt(&document, OdfExportLimits::default()).unwrap();
        assert_eq!(first.bytes, second.bytes);
        assert!(first.report.entries.is_empty());

        let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
        let content = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
        assert!(content.contains(
            "<style:style style:name=\"P_center\" style:family=\"paragraph\"><style:paragraph-properties fo:text-align=\"center\"/></style:style>"
        ));
        assert!(content.contains(
            "<style:style style:name=\"T_b1_i0_u1_s0_c1a2b3c_z21\" style:family=\"text\"><style:text-properties fo:font-weight=\"bold\" fo:font-style=\"normal\" style:text-underline-style=\"solid\" style:text-line-through-style=\"none\" fo:color=\"#1a2b3c\" fo:font-size=\"10.5pt\"/></style:style>"
        ));
        assert!(content.contains("<text:p text:style-name=\"P_center\">"));
        assert!(content.contains("<text:span text:style-name=\"T_b1_i0_u1_s0_c1a2b3c_z21\">"));
        assert!(content.contains("</text:span>"));

        let reopened = package.import_document(OdfImportLimits::default()).unwrap();
        assert!(reopened.report.entries.is_empty());
        let BlockNode::Paragraph(reopened_paragraph) = &reopened.document.body()[0] else {
            panic!("reopened paragraph")
        };
        assert_eq!(
            reopened_paragraph.properties.alignment,
            Some(Alignment::Center)
        );
        assert!(reopened_paragraph.inlines.iter().any(|inline| {
            matches!(inline, InlineNode::Run(run)
                if run.properties.bold == Some(true)
                    && run.properties.italic == Some(false)
                    && run.properties.underline == Some(true)
                    && run.properties.strike == Some(false)
                    && run.properties.color == Some(Color::Rgb(RgbColor {
                        r: 0x1a,
                        g: 0x2b,
                        b: 0x3c,
                    }))
                    && run.properties.size_half_points == Some(21))
        }));
        let reexported = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
        let mut package = OdtPackage::open(&reexported.bytes, OdfPackageLimits::default()).unwrap();
        let reopened_again = package.import_document(OdfImportLimits::default()).unwrap();
        assert_eq!(reopened_again.document, reopened.document);
    }

    #[test]
    fn lists_are_deterministic_nested_and_semantically_stable() {
        let source = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" office:version="1.4"><office:automatic-styles><text:list-style style:name="Mixed"><text:list-level-style-bullet text:level="1" text:bullet-char="•"/><text:list-level-style-number text:level="2" style:num-format="a" style:num-prefix="(" style:num-suffix=")" text:start-value="3"/></text:list-style></office:automatic-styles><office:body><office:text><text:list text:style-name="Mixed"><text:list-item><text:p>outer</text:p><text:list><text:list-item><text:p>nested one</text:p></text:list-item><text:list-item><text:p>nested two</text:p></text:list-item></text:list></text:list-item><text:list-item><text:p>second</text:p></text:list-item></text:list></office:text></office:body></office:document-content>"#;
        let document = import_content_xml(
            source.as_bytes(),
            OdfVersion::V1_4,
            OdfImportLimits::default(),
        )
        .unwrap()
        .document;
        let first = write_odt(&document, OdfExportLimits::default()).unwrap();
        let second = write_odt(&document, OdfExportLimits::default()).unwrap();
        assert_eq!(first.bytes, second.bytes);
        assert!(first.report.entries.is_empty(), "{:?}", first.report);

        let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
        let content = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
        assert!(content.contains("<text:list-style style:name=\"L_"));
        assert!(
            content.contains(
                "<text:list-level-style-bullet text:level=\"1\" text:bullet-char=\"•\"/>"
            )
        );
        assert!(content.contains(
            "<text:list-level-style-number text:level=\"2\" style:num-format=\"a\" style:num-prefix=\"(\" style:num-suffix=\")\" text:start-value=\"3\"/>"
        ));
        assert!(content.contains(
            "<text:list-item><text:p>outer</text:p><text:list><text:list-item><text:p>nested<text:s/>one</text:p></text:list-item><text:list-item><text:p>nested<text:s/>two</text:p>"
        ));

        let reopened = package.import_document(OdfImportLimits::default()).unwrap();
        assert!(reopened.report.entries.is_empty(), "{:?}", reopened.report);
        let levels = reopened
            .document
            .body()
            .iter()
            .map(|block| match block {
                BlockNode::Paragraph(paragraph) => paragraph.properties.numbering.unwrap().level,
                _ => panic!("paragraph"),
            })
            .collect::<Vec<_>>();
        assert_eq!(levels, [0, 1, 1, 0]);
        let reexported = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
        assert_eq!(reexported.bytes, first.bytes);
    }

    #[test]
    fn tables_are_deterministic_nested_and_semantically_stable() {
        let source = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" office:version="1.4"><office:body><office:text><text:p>before</text:p><table:table><table:table-column table:number-columns-repeated="3"/><table:table-header-rows><table:table-row><table:table-cell><text:p>H1</text:p></table:table-cell><table:table-cell><text:p>H2</text:p></table:table-cell><table:table-cell><text:p>H3</text:p></table:table-cell></table:table-row></table:table-header-rows><table:table-row><table:table-cell table:number-columns-spanned="2" table:number-rows-spanned="2"><text:p>merged</text:p></table:table-cell><table:covered-table-cell/><table:table-cell><table:table><table:table-row><table:table-cell><text:p>nested</text:p></table:table-cell></table:table-row></table:table></table:table-cell></table:table-row><table:table-row><table:covered-table-cell table:number-columns-repeated="2"/><table:table-cell><text:p>lower</text:p></table:table-cell></table:table-row></table:table><text:p>after</text:p></office:text></office:body></office:document-content>"#;
        let imported = import_content_xml(
            source.as_bytes(),
            OdfVersion::V1_4,
            OdfImportLimits::default(),
        )
        .unwrap();
        assert!(imported.report.entries.is_empty(), "{:?}", imported.report);
        let document = imported.document;

        let first = write_odt(&document, OdfExportLimits::default()).unwrap();
        let second = write_odt(&document, OdfExportLimits::default()).unwrap();
        assert_eq!(first.bytes, second.bytes);
        assert!(first.report.entries.is_empty(), "{:?}", first.report);

        let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
        let content = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
        assert!(content.contains(
            "<table:table><table:table-column table:number-columns-repeated=\"3\"/><table:table-header-rows>"
        ));
        assert!(content.contains(
            "<table:table-cell table:number-columns-spanned=\"2\" table:number-rows-spanned=\"2\">"
        ));
        assert!(content.contains("<table:covered-table-cell/><table:covered-table-cell/>"));
        assert!(content.matches("<table:table>").count() >= 2);

        let reopened = package.import_document(OdfImportLimits::default()).unwrap();
        assert!(reopened.report.entries.is_empty(), "{:?}", reopened.report);
        assert_eq!(reopened.document, document);
        let reexported = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
        assert_eq!(reexported.bytes, first.bytes);

        for limits in [
            OdfExportLimits {
                max_table_rows: 3,
                ..OdfExportLimits::default()
            },
            OdfExportLimits {
                max_table_cells: 7,
                ..OdfExportLimits::default()
            },
            OdfExportLimits {
                max_table_columns: 2,
                ..OdfExportLimits::default()
            },
        ] {
            assert!(matches!(
                write_odt(&document, limits),
                Err(OdfError::LimitExceeded { .. })
            ));
        }
    }

    #[test]
    fn notes_are_deterministic_recursive_and_semantically_stable() {
        let source = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" office:version="1.4"><office:body><office:text><text:p>body<text:note text:id="source-footnote" text:note-class="footnote"><text:note-citation/><text:note-body><text:p>foot paragraph</text:p><table:table><table:table-row><table:table-cell><text:p>foot table</text:p></table:table-cell></table:table-row></table:table></text:note-body></text:note>middle<text:note text:id="source-endnote" text:note-class="endnote"><text:note-citation/><text:note-body><text:p>end paragraph</text:p></text:note-body></text:note>end</text:p></office:text></office:body></office:document-content>"#;
        let imported = import_content_xml(
            source.as_bytes(),
            OdfVersion::V1_4,
            OdfImportLimits::default(),
        )
        .unwrap();
        assert!(imported.report.entries.is_empty(), "{:?}", imported.report);
        let document = imported.document;

        let first = write_odt(&document, OdfExportLimits::default()).unwrap();
        let second = write_odt(&document, OdfExportLimits::default()).unwrap();
        assert_eq!(first.bytes, second.bytes);
        assert!(first.report.entries.is_empty(), "{:?}", first.report);

        let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
        let content = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
        assert_eq!(content.matches("<text:note ").count(), 2);
        assert!(content.contains("text:note-class=\"footnote\""));
        assert!(content.contains("text:note-class=\"endnote\""));
        assert!(content.contains("<text:note-citation/><text:note-body><text:p>foot"));
        assert!(content.contains("<table:table>"));

        let reopened = package.import_document(OdfImportLimits::default()).unwrap();
        assert!(reopened.report.entries.is_empty(), "{:?}", reopened.report);
        assert_eq!(reopened.document, document);
        let reexported = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
        assert_eq!(reexported.bytes, first.bytes);

        assert!(matches!(
            write_odt(
                &document,
                OdfExportLimits {
                    max_notes: 1,
                    ..OdfExportLimits::default()
                },
            ),
            Err(OdfError::LimitExceeded {
                limit: "odt_export_notes",
                observed: 2,
                allowed: 1,
            })
        ));
    }

    #[test]
    fn shared_and_unreferenced_notes_have_explicit_export_outcomes() {
        let source = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.4"><office:body><office:text><text:p>x<text:note text:id="n" text:note-class="footnote"><text:note-citation/><text:note-body><text:p>note</text:p></text:note-body></text:note></text:p></office:text></office:body></office:document-content>"#;
        let mut document = import_content_xml(source, OdfVersion::V1_4, OdfImportLimits::default())
            .unwrap()
            .document;
        let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0] else {
            panic!("paragraph")
        };
        let InlineNode::NoteReference(mut repeated) = paragraph.inlines[1].clone() else {
            panic!("note reference")
        };
        repeated.id = casual_doc_model::NodeId::new(u128::MAX).unwrap();
        paragraph.inlines.push(InlineNode::NoteReference(repeated));
        document.definitions_mut().footnotes.insert(
            NoteId::new(casual_doc_model::NodeId::new(u128::MAX - 1).unwrap()),
            Note { blocks: Vec::new() },
        );
        document.validate().unwrap();

        let exported = write_odt(&document, OdfExportLimits::default()).unwrap();
        assert!(exported.report.entries.iter().any(|entry| {
            entry.feature == "odt.export.shared_note_reference"
                && entry.model_outcome == ModelOutcome::Degraded
        }));
        assert!(exported.report.entries.iter().any(|entry| {
            entry.feature == "odt.export.unreferenced_note"
                && entry.model_outcome == ModelOutcome::Omitted
        }));
        let mut package = OdtPackage::open(&exported.bytes, OdfPackageLimits::default()).unwrap();
        let content = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
        assert_eq!(content.matches("<text:note ").count(), 2);
        assert!(content.contains("-2\" text:note-class=\"footnote\""));
        package
            .import_document(OdfImportLimits::default())
            .unwrap()
            .document
            .validate()
            .unwrap();
    }

    #[test]
    fn non_rectangular_vertical_merge_is_reported_and_content_is_kept() {
        let source = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" office:version="1.4"><office:body><office:text><table:table><table:table-row><table:table-cell table:number-rows-spanned="2"><text:p>top</text:p></table:table-cell></table:table-row><table:table-row><table:covered-table-cell/></table:table-row></table:table></office:text></office:body></office:document-content>"#;
        let mut document = import_content_xml(
            source.as_bytes(),
            OdfVersion::V1_4,
            OdfImportLimits::default(),
        )
        .unwrap()
        .document;
        let BlockNode::Table(table) = &mut document.body_mut()[0] else {
            panic!("table")
        };
        let BlockNode::Paragraph(paragraph) = &mut table.rows[1].cells[0].blocks[0] else {
            panic!("continuation paragraph")
        };
        paragraph.properties.alignment = Some(Alignment::Center);

        let exported = write_odt(&document, OdfExportLimits::default()).unwrap();
        assert!(exported.report.entries.iter().any(|entry| {
            entry.feature == "odt.export.table_merge"
                && entry.model_outcome == ModelOutcome::Degraded
        }));
        let mut package = OdtPackage::open(&exported.bytes, OdfPackageLimits::default()).unwrap();
        let content = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
        assert!(!content.contains("table:number-rows-spanned"));
        assert!(content.contains("<text:p text:style-name=\"P_center\"></text:p>"));
        let reopened = package.import_document(OdfImportLimits::default()).unwrap();
        let BlockNode::Table(table) = &reopened.document.body()[0] else {
            panic!("reopened table")
        };
        assert_eq!(table.rows[0].cells[0].properties.vertical_merge, None);
        assert_eq!(table.rows[1].cells[0].properties.vertical_merge, None);
        let BlockNode::Paragraph(paragraph) = &table.rows[1].cells[0].blocks[0] else {
            panic!("reopened paragraph")
        };
        assert_eq!(paragraph.properties.alignment, Some(Alignment::Center));
    }

    #[test]
    fn unsupported_list_labels_are_reported_and_projected_as_plain_paragraphs() {
        let mut document = core_document();
        let mut ids = casual_doc_model::IdGenerator::new(0x0d7);
        let abstract_id = casual_doc_model::v1::AbstractNumberingId::new(ids.next_id().unwrap());
        let instance_id = NumberingInstanceId::new(ids.next_id().unwrap());
        document.definitions_mut().abstract_numbering.insert(
            abstract_id,
            casual_doc_model::v1::AbstractNumbering {
                levels: vec![casual_doc_model::v1::NumberingLevel {
                    level: 0,
                    start: 1,
                    num_fmt: Some(NumberFormat::DecimalZero),
                    lvl_text: Some("%1".to_owned()),
                    lvl_jc: None,
                    suff: None,
                    is_lgl: false,
                    paragraph_properties: None,
                    run_properties: None,
                    style_ref: None,
                    pstyle: None,
                    lvl_restart: None,
                }],
                multi_level_type: None,
                num_style_link: None,
                style_link: None,
            },
        );
        document.definitions_mut().numbering.insert(
            instance_id,
            casual_doc_model::v1::NumberingInstance {
                abstract_ref: abstract_id,
                overrides: Vec::new(),
            },
        );
        let paragraph = match &mut document.body_mut()[0] {
            BlockNode::Paragraph(paragraph) => paragraph,
            _ => panic!("paragraph"),
        };
        paragraph.properties.numbering = Some(casual_doc_model::v1::NumberingRef {
            instance: instance_id,
            level: 0,
        });

        let exported = write_odt(&document, OdfExportLimits::default()).unwrap();
        for feature in ["odt.export.list_label", "odt.export.numbering"] {
            assert!(
                exported
                    .report
                    .entries
                    .iter()
                    .any(|entry| entry.feature == feature),
                "missing {feature}"
            );
        }
        let mut package = OdtPackage::open(&exported.bytes, OdfPackageLimits::default()).unwrap();
        let content = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
        assert!(!content.contains("<text:list"));
        assert!(content.contains("<text:p>one"));
    }

    #[test]
    fn extended_run_properties_round_trip_to_a_fixed_point() {
        let mut document = core_document();
        let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0] else {
            panic!("paragraph")
        };
        let InlineNode::Run(run) = &mut paragraph.inlines[0] else {
            panic!("run")
        };
        run.properties.font_ref = Some(FontRef::Named(FontName {
            name: "Times New Roman".to_owned(),
        }));
        run.properties.vertical_alignment = Some(VerticalAlignment::Superscript);
        run.properties.all_caps = Some(true);
        run.properties.small_caps = Some(true);

        let first = write_odt(&document, OdfExportLimits::default()).unwrap();
        let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
        let content = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
        assert!(content.contains("fo:font-family=\"Times New Roman\""));
        assert!(content.contains("style:text-position=\"super\""));
        assert!(content.contains("fo:text-transform=\"uppercase\""));
        assert!(content.contains("fo:font-variant=\"small-caps\""));
        // These are no longer reported as an unsupported remainder.
        assert!(
            !first
                .report
                .entries
                .iter()
                .any(|entry| entry.feature == "odt.export.run_properties")
        );

        let reopened = package.import_document(OdfImportLimits::default()).unwrap();
        reopened.document.validate().unwrap();
        let BlockNode::Paragraph(paragraph) = &reopened.document.body()[0] else {
            panic!("paragraph")
        };
        let InlineNode::Run(run) = &paragraph.inlines[0] else {
            panic!("run")
        };
        assert_eq!(
            run.properties.font_ref,
            Some(FontRef::Named(FontName {
                name: "Times New Roman".to_owned(),
            }))
        );
        assert_eq!(
            run.properties.vertical_alignment,
            Some(VerticalAlignment::Superscript)
        );
        assert_eq!(run.properties.all_caps, Some(true));
        assert_eq!(run.properties.small_caps, Some(true));

        let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
        assert_eq!(first.bytes, second.bytes);
    }

    #[test]
    fn font_family_with_control_char_round_trips_via_numeric_ref() {
        let mut document = core_document();
        let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0] else {
            panic!("paragraph")
        };
        let InlineNode::Run(run) = &mut paragraph.inlines[0] else {
            panic!("run")
        };
        // A tab is a legal XML char; the font family is emitted with the tab as a
        // numeric character reference so it round-trips, rather than being dropped.
        run.properties.font_ref = Some(FontRef::Named(FontName {
            name: "Ar\tial".to_owned(),
        }));
        let first = write_odt(&document, OdfExportLimits::default()).unwrap();
        let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
        let content = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
        assert!(
            content.contains(r#"fo:font-family="Ar&#9;ial""#),
            "{content}"
        );
        let reopened = package.import_document(OdfImportLimits::default()).unwrap();
        let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
        assert_eq!(first.bytes, second.bytes);
    }

    #[test]
    fn padded_font_family_round_trips_without_trimming() {
        let mut document = core_document();
        let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0] else {
            panic!("paragraph")
        };
        let InlineNode::Run(run) = &mut paragraph.inlines[0] else {
            panic!("run")
        };
        // Import must not trim, or a padded name drifts and breaks the fixed
        // point. The writer emits the name verbatim.
        run.properties.font_ref = Some(FontRef::Named(FontName {
            name: " Arial ".to_owned(),
        }));
        let first = write_odt(&document, OdfExportLimits::default()).unwrap();
        let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
        let reopened = package.import_document(OdfImportLimits::default()).unwrap();
        let BlockNode::Paragraph(paragraph) = &reopened.document.body()[0] else {
            panic!("paragraph")
        };
        let InlineNode::Run(run) = &paragraph.inlines[0] else {
            panic!("run")
        };
        assert_eq!(
            run.properties.font_ref,
            Some(FontRef::Named(FontName {
                name: " Arial ".to_owned(),
            }))
        );
        let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
        assert_eq!(first.bytes, second.bytes);
    }

    #[test]
    fn paragraph_properties_round_trip_to_a_fixed_point() {
        use casual_doc_model::v1::{Indentation, Spacing};
        let mut document = core_document();
        let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0] else {
            panic!("paragraph")
        };
        paragraph.properties.indentation = Some(Indentation {
            start_twips: Some(720),
            end_twips: Some(360),
            first_line_twips: Some(240),
            hanging_twips: None,
        });
        paragraph.properties.spacing = Some(Spacing {
            before_twips: Some(120),
            after_twips: Some(240),
            line_percent: Some(150),
            ..Spacing::default()
        });
        paragraph.properties.keep_next = true;
        paragraph.properties.keep_lines = true;
        paragraph.properties.page_break_before = true;

        let first = write_odt(&document, OdfExportLimits::default()).unwrap();
        let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
        let content = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
        for expected in [
            "fo:margin-left=\"36pt\"",
            "fo:margin-right=\"18pt\"",
            "fo:text-indent=\"12pt\"",
            "fo:margin-top=\"6pt\"",
            "fo:margin-bottom=\"12pt\"",
            "fo:line-height=\"150%\"",
            "fo:keep-with-next=\"always\"",
            "fo:keep-together=\"always\"",
            "fo:break-before=\"page\"",
        ] {
            assert!(
                content.contains(expected),
                "missing {expected} in {content}"
            );
        }
        assert!(
            !first
                .report
                .entries
                .iter()
                .any(|entry| entry.feature == "odt.export.paragraph_properties")
        );

        let reopened = package.import_document(OdfImportLimits::default()).unwrap();
        reopened.document.validate().unwrap();
        let BlockNode::Paragraph(paragraph) = &reopened.document.body()[0] else {
            panic!("paragraph")
        };
        assert_eq!(
            paragraph.properties.indentation,
            Some(Indentation {
                start_twips: Some(720),
                end_twips: Some(360),
                first_line_twips: Some(240),
                hanging_twips: None,
            })
        );
        assert_eq!(
            paragraph.properties.spacing,
            Some(Spacing {
                before_twips: Some(120),
                after_twips: Some(240),
                line_percent: Some(150),
                ..Spacing::default()
            })
        );
        assert!(paragraph.properties.keep_next);
        assert!(paragraph.properties.keep_lines);
        assert!(paragraph.properties.page_break_before);

        let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
        assert_eq!(first.bytes, second.bytes);
    }

    #[test]
    fn unsupported_formatting_is_reported_and_limits_fail_atomically() {
        let mut document = core_document();
        let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0] else {
            panic!("paragraph")
        };
        let InlineNode::Run(run) = &mut paragraph.inlines[0] else {
            panic!("run")
        };
        run.properties.bold = Some(true);
        // `hidden` (fo:display / text:display) is still outside the supported
        // run subset, so it must surface as a reported remainder.
        run.properties.hidden = Some(true);
        let exported = write_odt(&document, OdfExportLimits::default()).unwrap();
        assert!(
            exported
                .report
                .entries
                .iter()
                .any(|entry| entry.feature == "odt.export.run_properties")
        );

        let error = write_odt(
            &document,
            OdfExportLimits {
                max_content_bytes: 8,
                ..OdfExportLimits::default()
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            OdfError::LimitExceeded {
                limit: "odt_export_content_bytes",
                ..
            }
        ));
        let invalid = write_odt(
            &document,
            OdfExportLimits {
                max_blocks: usize::MAX,
                ..OdfExportLimits::default()
            },
        )
        .unwrap_err();
        assert!(matches!(
            invalid,
            OdfError::InvalidLimitConfiguration {
                limit: "odt_export_blocks",
                ..
            }
        ));
    }
}
