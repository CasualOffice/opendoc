// SPDX-License-Identifier: Apache-2.0

//! Resource bounds for PresentationML import.
//!
//! The ZIP container is already bounded by `casual_doc_package::PackageLimits`,
//! which is the admission boundary and is not duplicated here. What these bound
//! is the second cost a package can impose once its bytes are admitted: the XML
//! inside a part, and how many parts a deck may chain together. A 2 KB slide
//! part can still declare a million-deep element nest, and a presentation part
//! can still name ten thousand slides.

/// Host-configurable PresentationML import limits.
///
/// Separate from [`casual_doc_package::PackageLimits`] rather than folded into
/// it: those bound the container for every format, these bound one format's
/// document graph. A host that raises the ZIP ceiling for a large media payload
/// has not asked for a deeper element nest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImportLimits {
    /// Maximum slides admitted from `p:sldIdLst`.
    pub max_slides: usize,
    /// Maximum slide layouts admitted across every master.
    pub max_layouts: usize,
    /// Maximum slide masters admitted from `p:sldMasterIdLst`.
    pub max_masters: usize,
    /// Maximum elements read from one part's XML.
    pub max_part_elements: u64,
    /// Maximum element nesting depth in one part's XML.
    pub max_part_depth: u32,
    /// Maximum shapes admitted from one `p:spTree`, counting nested groups'
    /// children.
    pub max_shapes_per_tree: usize,
    /// Maximum paragraphs admitted from one `a:txBody`.
    pub max_paragraphs_per_body: usize,
    /// Maximum runs, breaks and fields admitted from one `a:p`.
    pub max_runs_per_paragraph: usize,
    /// Maximum UTF-8 bytes admitted from one `a:t`.
    pub max_run_bytes: usize,
    /// Maximum `a:tr` rows admitted from one `a:tbl`.
    ///
    /// Separate from [`Self::max_shapes_per_tree`] because a table is not a
    /// shape tree: one `p:graphicFrame` counts as one shape there while
    /// carrying rows x columns cells, each with its own `a:txBody`. So the
    /// shape bound does not bound a table at all, and a 2 KB part can declare a
    /// grid with a hundred million cells.
    pub max_table_rows: usize,
    /// Maximum `a:gridCol` columns admitted from one `a:tblGrid`.
    pub max_table_columns: usize,
    /// Maximum `a:tblStyle` entries admitted from `tableStyles.xml`.
    pub max_table_styles: usize,
}

impl ImportLimits {
    /// Hard maximum slides, not bypassable by a host.
    pub const HARD_MAX_SLIDES: usize = 10_000;
    /// Hard maximum layouts.
    pub const HARD_MAX_LAYOUTS: usize = 10_000;
    /// Hard maximum masters.
    pub const HARD_MAX_MASTERS: usize = 1_000;
    /// Hard maximum elements per part.
    pub const HARD_MAX_PART_ELEMENTS: u64 = 2_000_000;
    /// Hard maximum element depth per part.
    pub const HARD_MAX_PART_DEPTH: u32 = 256;
    /// Hard maximum shapes per shape tree.
    pub const HARD_MAX_SHAPES_PER_TREE: usize = 20_000;
    /// Hard maximum paragraphs per text body.
    pub const HARD_MAX_PARAGRAPHS_PER_BODY: usize = 20_000;
    /// Hard maximum runs per paragraph.
    pub const HARD_MAX_RUNS_PER_PARAGRAPH: usize = 20_000;
    /// Hard maximum bytes per text run.
    pub const HARD_MAX_RUN_BYTES: usize = 1_000_000;
    /// Hard maximum rows per table — `casual_pres_model::MAX_TABLE_ROWS`, so the
    /// importer cannot admit a table the model would then refuse.
    pub const HARD_MAX_TABLE_ROWS: usize = casual_pres_model::MAX_TABLE_ROWS;
    /// Hard maximum grid columns per table —
    /// `casual_pres_model::MAX_TABLE_GRID_COLUMNS`, for the same reason.
    pub const HARD_MAX_TABLE_COLUMNS: usize = casual_pres_model::MAX_TABLE_GRID_COLUMNS;
    /// Hard maximum `a:tblStyle` entries.
    pub const HARD_MAX_TABLE_STYLES: usize = 10_000;

    /// Clamps every field to its hard ceiling.
    ///
    /// Clamped rather than refused, unlike `PackageLimits::validate`. The
    /// difference is deliberate: a container limit above its ceiling is a host
    /// configuration error worth failing loudly, whereas these are per-document
    /// shape bounds whose only effect is where import stops reporting detail —
    /// so silently honouring the ceiling costs a caller nothing and refusing to
    /// open the deck would cost them everything.
    ///
    /// # Complexity
    ///
    /// O(1).
    #[must_use]
    pub const fn clamped(self) -> Self {
        Self {
            max_slides: min_usize(self.max_slides, Self::HARD_MAX_SLIDES),
            max_layouts: min_usize(self.max_layouts, Self::HARD_MAX_LAYOUTS),
            max_masters: min_usize(self.max_masters, Self::HARD_MAX_MASTERS),
            max_part_elements: min_u64(self.max_part_elements, Self::HARD_MAX_PART_ELEMENTS),
            max_part_depth: min_u32(self.max_part_depth, Self::HARD_MAX_PART_DEPTH),
            max_shapes_per_tree: min_usize(
                self.max_shapes_per_tree,
                Self::HARD_MAX_SHAPES_PER_TREE,
            ),
            max_paragraphs_per_body: min_usize(
                self.max_paragraphs_per_body,
                Self::HARD_MAX_PARAGRAPHS_PER_BODY,
            ),
            max_runs_per_paragraph: min_usize(
                self.max_runs_per_paragraph,
                Self::HARD_MAX_RUNS_PER_PARAGRAPH,
            ),
            max_run_bytes: min_usize(self.max_run_bytes, Self::HARD_MAX_RUN_BYTES),
            max_table_rows: min_usize(self.max_table_rows, Self::HARD_MAX_TABLE_ROWS),
            max_table_columns: min_usize(self.max_table_columns, Self::HARD_MAX_TABLE_COLUMNS),
            max_table_styles: min_usize(self.max_table_styles, Self::HARD_MAX_TABLE_STYLES),
        }
    }
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            max_slides: 2_000,
            max_layouts: 2_000,
            max_masters: 100,
            max_part_elements: 500_000,
            max_part_depth: 64,
            max_shapes_per_tree: 5_000,
            max_paragraphs_per_body: 5_000,
            max_runs_per_paragraph: 5_000,
            max_run_bytes: 100_000,
            // PowerPoint's own UI stops at 75 columns and has no row ceiling; a
            // thousand rows is far past anything a slide shows and still refuses
            // a part built to exhaust memory.
            max_table_rows: 1_000,
            max_table_columns: 256,
            max_table_styles: 1_000,
        }
    }
}

const fn min_usize(left: usize, right: usize) -> usize {
    if left < right { left } else { right }
}

const fn min_u64(left: u64, right: u64) -> u64 {
    if left < right { left } else { right }
}

const fn min_u32(left: u32, right: u32) -> u32 {
    if left < right { left } else { right }
}
