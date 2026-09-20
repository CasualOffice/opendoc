//! Host-configurable RTF admission bounds with non-bypassable hard ceilings.

use crate::RtfError;

/// Bounded admission policy for one RTF import.
///
/// RTF is not a package, so none of the ZIP-shaped defences apply. Its hostile
/// axes are group nesting, stream length, declared binary length, and the
/// expansion counts a small file can produce. Each is bounded independently
/// here, checked *during* the single parsing pass rather than after it, and
/// clamped by a compiled ceiling a host cannot raise. See
/// `docs/110-RTF-IMPORT-PROFILE.md` §7 for the rationale behind each default.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RtfLimits {
    /// Maximum admitted input bytes.
    pub max_input_bytes: usize,
    /// Maximum `{` nesting depth.
    pub max_group_depth: usize,
    /// Maximum paragraphs produced, across body, table cells, and every other
    /// block sink.
    pub max_paragraphs: usize,
    /// Maximum inline nodes produced.
    pub max_inline_nodes: usize,
    /// Maximum decoded Unicode scalar values.
    pub max_text_scalar_values: usize,
    /// Maximum `\row` count.
    pub max_table_rows: usize,
    /// Maximum `\cell` count.
    pub max_table_cells: usize,
    /// Maximum `\fonttbl` entries.
    pub max_fonts: usize,
    /// Maximum `\colortbl` entries.
    pub max_colors: usize,
    /// Maximum `\listtable` definitions.
    pub max_list_definitions: usize,
    /// Maximum `\pict` count.
    pub max_pictures: usize,
    /// Maximum decoded bytes in one picture.
    pub max_picture_bytes: usize,
    /// Maximum decoded picture bytes across the document.
    pub max_total_picture_bytes: usize,
    /// Maximum distinct compatibility findings.
    pub max_findings: usize,
}

impl RtfLimits {
    /// Hard maximum admitted input bytes.
    pub const HARD_MAX_INPUT_BYTES: usize = 256 * 1024 * 1024;
    /// Hard maximum group nesting depth.
    pub const HARD_MAX_GROUP_DEPTH: usize = 1024;
    /// Hard maximum paragraphs.
    pub const HARD_MAX_PARAGRAPHS: usize = 4_000_000;
    /// Hard maximum inline nodes.
    pub const HARD_MAX_INLINE_NODES: usize = 32_000_000;
    /// Hard maximum decoded Unicode scalar values.
    pub const HARD_MAX_TEXT_SCALAR_VALUES: usize = 200_000_000;
    /// Hard maximum table rows.
    pub const HARD_MAX_TABLE_ROWS: usize = 1_000_000;
    /// Hard maximum table cells.
    pub const HARD_MAX_TABLE_CELLS: usize = 4_000_000;
    /// Hard maximum font-table entries.
    pub const HARD_MAX_FONTS: usize = 65_536;
    /// Hard maximum colour-table entries.
    pub const HARD_MAX_COLORS: usize = 65_536;
    /// Hard maximum list definitions.
    pub const HARD_MAX_LIST_DEFINITIONS: usize = 65_536;
    /// Hard maximum pictures.
    pub const HARD_MAX_PICTURES: usize = 65_536;
    /// Hard maximum bytes in one picture.
    pub const HARD_MAX_PICTURE_BYTES: usize = 128 * 1024 * 1024;
    /// Hard maximum picture bytes across the document.
    pub const HARD_MAX_TOTAL_PICTURE_BYTES: usize = 256 * 1024 * 1024;
    /// Hard maximum distinct compatibility findings.
    pub const HARD_MAX_FINDINGS: usize = 65_536;

    /// The RTF specification's own limit on a control word's letters.
    ///
    /// Not configurable: a longer run of letters is not a large control word,
    /// it is a malformed stream, and admitting it would let a hostile file
    /// spend unbounded time in a single token.
    pub const MAX_CONTROL_WORD_BYTES: usize = 32;
    /// Maximum digits in a control-word parameter.
    ///
    /// Ten digits covers the whole `i32` range with room for the sign handled
    /// separately; an eleventh digit is refused rather than silently wrapped,
    /// because a wrapped `\binN` length is a memory-safety-shaped bug.
    pub const MAX_PARAMETER_DIGITS: usize = 10;

    /// Rejects a configuration that exceeds any compiled ceiling.
    pub fn validate(self) -> Result<(), RtfError> {
        for (limit, configured, ceiling) in [
            (
                "rtf_input_bytes",
                self.max_input_bytes,
                Self::HARD_MAX_INPUT_BYTES,
            ),
            (
                "rtf_group_depth",
                self.max_group_depth,
                Self::HARD_MAX_GROUP_DEPTH,
            ),
            (
                "rtf_paragraphs",
                self.max_paragraphs,
                Self::HARD_MAX_PARAGRAPHS,
            ),
            (
                "rtf_inline_nodes",
                self.max_inline_nodes,
                Self::HARD_MAX_INLINE_NODES,
            ),
            (
                "rtf_text_scalar_values",
                self.max_text_scalar_values,
                Self::HARD_MAX_TEXT_SCALAR_VALUES,
            ),
            (
                "rtf_table_rows",
                self.max_table_rows,
                Self::HARD_MAX_TABLE_ROWS,
            ),
            (
                "rtf_table_cells",
                self.max_table_cells,
                Self::HARD_MAX_TABLE_CELLS,
            ),
            ("rtf_fonts", self.max_fonts, Self::HARD_MAX_FONTS),
            ("rtf_colors", self.max_colors, Self::HARD_MAX_COLORS),
            (
                "rtf_list_definitions",
                self.max_list_definitions,
                Self::HARD_MAX_LIST_DEFINITIONS,
            ),
            ("rtf_pictures", self.max_pictures, Self::HARD_MAX_PICTURES),
            (
                "rtf_picture_bytes",
                self.max_picture_bytes,
                Self::HARD_MAX_PICTURE_BYTES,
            ),
            (
                "rtf_total_picture_bytes",
                self.max_total_picture_bytes,
                Self::HARD_MAX_TOTAL_PICTURE_BYTES,
            ),
            ("rtf_findings", self.max_findings, Self::HARD_MAX_FINDINGS),
        ] {
            if configured > ceiling {
                return Err(RtfError::LimitAboveCeiling {
                    limit,
                    configured: configured as u64,
                    ceiling: ceiling as u64,
                });
            }
        }
        Ok(())
    }
}

impl Default for RtfLimits {
    /// Defaults sized so the **browser** is safe without the host having to
    /// remember to pass smaller ones.
    ///
    /// `max_paragraphs` is the block ceiling measured for the wasm viewer in
    /// `docs/104` HF-158, not a native-host number. A registry built with
    /// [`RtfLimits::default`] in a 32-bit wasm module therefore cannot admit a
    /// document that would exhaust linear memory, which is exactly the failure
    /// the plain-text adapter shipped before its limits were split.
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024 * 1024,
            max_group_depth: 128,
            max_paragraphs: 262_144,
            max_inline_nodes: 2_000_000,
            max_text_scalar_values: 20_000_000,
            max_table_rows: 100_000,
            max_table_cells: 500_000,
            max_fonts: 4_096,
            max_colors: 4_096,
            max_list_definitions: 4_096,
            max_pictures: 4_096,
            max_picture_bytes: 32 * 1024 * 1024,
            max_total_picture_bytes: 64 * 1024 * 1024,
            max_findings: 4_096,
        }
    }
}

/// Refuses an observed count that has passed its bound.
pub(crate) fn enforce(
    limit: &'static str,
    observed: usize,
    allowed: usize,
) -> Result<(), RtfError> {
    if observed > allowed {
        return Err(RtfError::LimitExceeded {
            limit,
            observed: observed as u64,
            allowed: allowed as u64,
        });
    }
    Ok(())
}
