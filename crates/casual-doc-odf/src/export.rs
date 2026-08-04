//! Deterministic bounded ODF 1.4 writing for the implemented ODT subset.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};

use casual_doc_model::v1::{
    Alignment, BlockNode, BookmarkId, BreakKind, Color, Definitions, Document, DocumentDefaults,
    Extent, FontRef, GroupChild, HeaderFooterKind, HyperlinkTarget, InlineNode, LevelJustification,
    LevelSuffix, MediaId, Note, NoteId, NoteKind, NoteReference, NumberFormat, NumberingInstanceId,
    Paragraph, ParagraphProperties, RevisionKind, RunProperties, Table, TableCell,
    TableCellProperties, TableRow, TableRowProperties, VerticalAlignment, VerticalMerge,
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
            "T_b{}_i{}_u{}_s{}_c{}_z{}",
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
    // fonts and the complex/east-asian/h-ansi slots stay in the remainder and are
    // reported, so nothing is silently lost.
    if let Some(FontRef::Named(name)) = &remainder.font_ref {
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
    (style, remainder)
}

struct Writer {
    xml: String,
    paragraph_styles: BTreeSet<OdtParagraphAlignment>,
    run_styles: BTreeSet<OdtRunStyle>,
    list_styles: BTreeMap<NumberingInstanceId, OdtListStyle>,
    emitted_lists: BTreeSet<NumberingInstanceId>,
    footnotes: BTreeMap<NoteId, Note>,
    endnotes: BTreeMap<NoteId, Note>,
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
    /// Bookmark id → name, so `BookmarkStart`/`BookmarkEnd` markers can re-emit
    /// their `text:bookmark-start`/`-end` elements.
    bookmarks: BTreeMap<BookmarkId, String>,
    reporter: Reporter,
}

impl Writer {
    fn new(limits: OdfExportLimits) -> Result<Self, OdfError> {
        let mut writer = Self {
            xml: String::new(),
            paragraph_styles: BTreeSet::new(),
            run_styles: BTreeSet::new(),
            list_styles: BTreeMap::new(),
            emitted_lists: BTreeSet::new(),
            footnotes: BTreeMap::new(),
            endnotes: BTreeMap::new(),
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
            bookmarks: BTreeMap::new(),
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
        if table.grid.iter().any(|column| column.width_twips.is_some()) {
            self.reporter
                .record("odt.export.table_grid_widths", ModelOutcome::Omitted);
        }
        if table.grid_change.is_some() {
            self.reporter
                .record("odt.export.table_grid_change", ModelOutcome::Omitted);
        }
        if table.properties != Default::default() {
            self.reporter
                .record("odt.export.table_properties", ModelOutcome::Omitted);
        }

        let merges = analyze_table_merges(table)?;
        self.push("<table:table>")?;
        self.push("<table:table-column")?;
        if columns > 1 {
            self.push(" table:number-columns-repeated=\"")?;
            self.push(&columns.to_string())?;
            self.push("\"")?;
        }
        self.push("/>")?;

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
        self.push("<table:table-row>")?;
        let row = &table.rows[row_index];
        for (cell_index, cell) in row.cells.iter().enumerate() {
            let coordinate = (row_index, cell_index);
            let span = cell.properties.grid_span.unwrap_or(1);
            let mut remainder = cell.properties.clone();
            remainder.grid_span = None;
            remainder.vertical_merge = None;
            if remainder != TableCellProperties::default() {
                self.reporter
                    .record("odt.export.table_cell_properties", ModelOutcome::Omitted);
            }
            if cell.properties.grid_span == Some(1) {
                self.reporter
                    .record("odt.export.table_cell_properties", ModelOutcome::Degraded);
            }
            if cell.properties.vertical_merge == Some(VerticalMerge::Continue)
                && merges.continuations.contains(&coordinate)
            {
                for _ in 0..span {
                    self.push("<table:covered-table-cell/>")?;
                }
                continue;
            }

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
        let alignment = remainder.alignment.take().map(OdtParagraphAlignment::from);
        if remainder.numbering.take().is_some() && !numbering_mapped {
            self.reporter
                .record("odt.export.numbering", ModelOutcome::Omitted);
        }
        if remainder != ParagraphProperties::default() {
            self.reporter
                .record("odt.export.paragraph_properties", ModelOutcome::Omitted);
        }
        if let Some(level) = outline {
            self.push("<text:h text:outline-level=\"")?;
            self.push(&(u16::from(level) + 1).to_string())?;
            if let Some(alignment) = alignment {
                self.paragraph_styles.insert(alignment);
                self.push("\" text:style-name=\"")?;
                self.push(alignment.name())?;
            }
            self.push("\">")?;
        } else {
            self.push("<text:p")?;
            if let Some(alignment) = alignment {
                self.paragraph_styles.insert(alignment);
                self.push(" text:style-name=\"")?;
                self.push(alignment.name())?;
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
                    let (style, remainder) = split_run_properties(&run.properties);
                    if remainder != RunProperties::default() {
                        self.reporter
                            .record("odt.export.run_properties", ModelOutcome::Omitted);
                    }
                    let styled = !style.is_empty();
                    if styled {
                        let name = style.name();
                        self.run_styles.insert(style);
                        self.push("<text:span text:style-name=\"")?;
                        self.push(&name)?;
                        self.push("\">")?;
                    }
                    self.write_text(&run.text)?;
                    if styled {
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
                InlineNode::Field(field) => {
                    self.reporter
                        .record("odt.export.field", ModelOutcome::Degraded);
                    self.write_inlines(&field.inlines, depth + 1)?;
                }
                InlineNode::Revision(revision) => {
                    self.reporter
                        .record("odt.export.revision", ModelOutcome::Degraded);
                    if matches!(
                        revision.kind,
                        RevisionKind::Insertion | RevisionKind::MoveTo
                    ) {
                        self.write_inlines(&revision.inlines, depth + 1)?;
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
                    self.write_alt(drawing.descr.as_deref(), "odt.export.anchored_drawing")?
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
                InlineNode::TextBox(_) => self
                    .reporter
                    .record("odt.export.text_box", ModelOutcome::Omitted),
                InlineNode::Group(group) => {
                    self.reporter
                        .record("odt.export.group", ModelOutcome::Omitted);
                    if group
                        .children
                        .iter()
                        .any(|child| matches!(child, GroupChild::TextBox(_) | GroupChild::Group(_)))
                    {
                        self.reporter
                            .record("odt.export.group_text", ModelOutcome::Omitted);
                    }
                }
                InlineNode::NoteReference(note) => self.write_note(note, depth + 1)?,
                InlineNode::CommentReference(_)
                | InlineNode::CommentRangeStart(_)
                | InlineNode::CommentRangeEnd(_) => self
                    .reporter
                    .record("odt.export.comment", ModelOutcome::Omitted),
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
        if let Some(descr) = descr {
            // svg:title is single-line: fold tab/CR/LF to a space and drop any
            // other non-XML character, degrading rather than aborting the export
            // (the semantic alt-text path degrades the same input).
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
        }
        self.push("</draw:frame>")
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

fn automatic_styles_xml(
    paragraph_styles: &BTreeSet<OdtParagraphAlignment>,
    run_styles: &BTreeSet<OdtRunStyle>,
    list_styles: &BTreeMap<NumberingInstanceId, OdtListStyle>,
    max_content_bytes: usize,
) -> Result<String, OdfError> {
    if paragraph_styles.is_empty() && run_styles.is_empty() && list_styles.is_empty() {
        return Ok(String::new());
    }
    let mut xml = String::new();
    push_bounded(&mut xml, "<office:automatic-styles>", max_content_bytes)?;
    for alignment in paragraph_styles {
        push_bounded(&mut xml, "<style:style style:name=\"", max_content_bytes)?;
        push_bounded(&mut xml, alignment.name(), max_content_bytes)?;
        push_bounded(
            &mut xml,
            "\" style:family=\"paragraph\"><style:paragraph-properties fo:text-align=\"",
            max_content_bytes,
        )?;
        push_bounded(&mut xml, alignment.value(), max_content_bytes)?;
        push_bounded(&mut xml, "\"/></style:style>", max_content_bytes)?;
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
    reporter: &mut Reporter,
    max_content_bytes: usize,
) -> Result<String, OdfError> {
    let Some(defaults) = defaults else {
        return Ok(String::new());
    };
    let mut body = String::new();
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
                &mut body,
                "<style:default-style style:family=\"paragraph\"><style:paragraph-properties fo:text-align=\"",
                max_content_bytes,
            )?;
            push_bounded(&mut body, alignment.value(), max_content_bytes)?;
            push_bounded(&mut body, "\"/></style:default-style>", max_content_bytes)?;
        }
    }
    if let Some(run) = &defaults.run {
        let (style, remainder) = split_run_properties(run);
        if remainder != RunProperties::default() {
            reporter.record("odt.export.document_defaults.run", ModelOutcome::Omitted);
        }
        if !style.is_empty() {
            push_bounded(
                &mut body,
                "<style:default-style style:family=\"text\"><style:text-properties",
                max_content_bytes,
            )?;
            push_run_text_properties(&mut body, &style, max_content_bytes)?;
            push_bounded(&mut body, "/></style:default-style>", max_content_bytes)?;
        }
    }
    if body.is_empty() {
        return Ok(String::new());
    }
    Ok(format!("<office:styles>{body}</office:styles>"))
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
    writer.register_numbering(document.definitions());
    writer.register_notes(document.definitions());
    writer.register_bookmarks(document.definitions());
    let mut definition_remainder = document.definitions().clone();
    definition_remainder.abstract_numbering = Default::default();
    definition_remainder.numbering = Default::default();
    definition_remainder.footnotes = Default::default();
    definition_remainder.endnotes = Default::default();
    // Document defaults are emitted into styles.xml, so they are not a loss.
    definition_remainder.document_defaults = Default::default();
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
        &mut writer.reporter,
        limits.max_content_bytes,
    )?;
    let styles = automatic_styles_xml(
        &writer.paragraph_styles,
        &writer.run_styles,
        &writer.list_styles,
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
    matches!(character as u32, 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff)
}

/// Whether every character of `value` can be serialized into ODF output. Used to
/// guard attribute values (bookmark names, hrefs, tooltips) so a value the model
/// permits but we cannot emit degrades gracefully rather than aborting the whole
/// export.
fn is_representable(value: &str) -> bool {
    value.chars().all(is_xml_character)
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
