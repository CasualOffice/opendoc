//! Deterministic bounded ODF 1.4 writing for the implemented ODT subset.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};

use casual_doc_model::v1::{
    Alignment, BlockNode, BreakKind, Color, Definitions, Document, GroupChild, InlineNode,
    LevelJustification, LevelSuffix, Note, NoteId, NoteKind, NoteReference, NumberFormat,
    NumberingInstanceId, Paragraph, ParagraphProperties, RevisionKind, RunProperties, Table,
    TableCell, TableCellProperties, TableRow, TableRowProperties, VerticalMerge,
};
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

use crate::{
    CompatibilityEntry, CompatibilityReport, MANIFEST_PART, MIMETYPE_PART, ModelOutcome, ODT_MIME,
    OdfError, RetentionOutcome,
};

const CONTENT_HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" office:version="1.4">"#;
const BODY_PREFIX: &str = "<office:body><office:text>";
const CONTENT_SUFFIX: &str = "</office:text></office:body></office:document-content>";
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.4"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text" manifest:version="1.4"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#;

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
        format!(
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
        )
    }
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
            reporter: Reporter::new(limits.max_report_features),
        };
        writer.push(BODY_PREFIX)?;
        Ok(writer)
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
                    self.reporter
                        .record("odt.export.hyperlink", ModelOutcome::Degraded);
                    self.write_inlines(&link.inlines, depth + 1)?;
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
                    self.write_alt(drawing.descr.as_deref(), "odt.export.drawing")?
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
                InlineNode::BookmarkStart(_) | InlineNode::BookmarkEnd(_) => self
                    .reporter
                    .record("odt.export.bookmark", ModelOutcome::Omitted),
                InlineNode::MoveRangeStart(_) | InlineNode::MoveRangeEnd(_) => self
                    .reporter
                    .record("odt.export.move_range", ModelOutcome::Omitted),
                InlineNode::HorizontalRule(_) => self
                    .reporter
                    .record("odt.export.horizontal_rule", ModelOutcome::Omitted),
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
        if let Some(bold) = style.bold {
            push_bounded(
                &mut xml,
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
                &mut xml,
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
                &mut xml,
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
                &mut xml,
                if strike {
                    " style:text-line-through-style=\"solid\""
                } else {
                    " style:text-line-through-style=\"none\""
                },
                max_content_bytes,
            )?;
        }
        if let Some((red, green, blue)) = style.color {
            push_bounded(&mut xml, " fo:color=\"#", max_content_bytes)?;
            push_bounded(
                &mut xml,
                &format!("{red:02x}{green:02x}{blue:02x}"),
                max_content_bytes,
            )?;
            push_bounded(&mut xml, "\"", max_content_bytes)?;
        }
        if let Some(size) = style.size_half_points {
            push_bounded(&mut xml, " fo:font-size=\"", max_content_bytes)?;
            push_bounded(&mut xml, &(size / 2).to_string(), max_content_bytes)?;
            if size % 2 != 0 {
                push_bounded(&mut xml, ".5", max_content_bytes)?;
            }
            push_bounded(&mut xml, "pt\"", max_content_bytes)?;
        }
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
    limits.validate()?;
    document.validate().map_err(|_| OdfError::InvalidModel)?;
    let mut writer = Writer::new(limits)?;
    writer.register_numbering(document.definitions());
    writer.register_notes(document.definitions());
    let mut definition_remainder = document.definitions().clone();
    definition_remainder.abstract_numbering = Default::default();
    definition_remainder.numbering = Default::default();
    definition_remainder.footnotes = Default::default();
    definition_remainder.endnotes = Default::default();
    if definition_remainder != Definitions::default() {
        writer
            .reporter
            .record("odt.export.definitions", ModelOutcome::Omitted);
    }
    if document.properties().is_some() {
        writer
            .reporter
            .record("odt.export.document_properties", ModelOutcome::Omitted);
    }
    if document.background().is_some() {
        writer
            .reporter
            .record("odt.export.background", ModelOutcome::Omitted);
    }
    writer.write_blocks(document.body(), 0)?;
    writer.report_unreferenced_notes();
    if writer.paragraphs_written == 0 {
        writer.push("<text:p/>")?;
    }
    writer.push(CONTENT_SUFFIX)?;
    let styles = automatic_styles_xml(
        &writer.paragraph_styles,
        &writer.run_styles,
        &writer.list_styles,
        limits.max_content_bytes,
    )?;
    let content_len = CONTENT_HEADER
        .len()
        .checked_add(styles.len())
        .and_then(|value| value.checked_add(writer.xml.len()))
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
    content.push_str(CONTENT_HEADER);
    content.push_str(&styles);
    content.push_str(&writer.xml);
    let content = content.into_bytes();
    let report = writer.reporter.finish();
    let bytes = package(&content, limits)?;
    Ok(OdtExport { bytes, report })
}

fn package(content: &[u8], limits: OdfExportLimits) -> Result<Vec<u8>, OdfError> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(
        MIMETYPE_PART,
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
    )
    .map_err(|_| OdfError::SerializationFailed)?;
    zip.write_all(ODT_MIME.as_bytes())
        .map_err(|_| OdfError::SerializationFailed)?;
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(crate::CONTENT_PART, deflated)
        .map_err(|_| OdfError::SerializationFailed)?;
    zip.write_all(content)
        .map_err(|_| OdfError::SerializationFailed)?;
    zip.start_file(MANIFEST_PART, deflated)
        .map_err(|_| OdfError::SerializationFailed)?;
    zip.write_all(MANIFEST.as_bytes())
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

#[cfg(test)]
mod tests {
    use casual_doc_model::v1::{BlockNode, InlineNode, RgbColor};

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
                }],
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
    fn unsupported_formatting_is_reported_and_limits_fail_atomically() {
        let mut document = core_document();
        let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0] else {
            panic!("paragraph")
        };
        let InlineNode::Run(run) = &mut paragraph.inlines[0] else {
            panic!("run")
        };
        run.properties.bold = Some(true);
        run.properties.all_caps = Some(true);
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
