//! The effective-property cascade — resolving a paragraph's and its runs'
//! *effective* formatting from the style hierarchy, not just the direct
//! properties on the node.
//!
//! Real documents put most formatting in **styles**: a paragraph names a
//! `pStyle`, a run may name an `rStyle`, and both inherit from the document
//! defaults (`w:docDefaults`). The flow engine must resolve the value actually in
//! effect — a Heading paragraph shapes at the heading style's font size, a
//! list item inherits the list style's spacing — or every style-driven document
//! renders with the wrong sizes, spacing, alignment, and color.
//!
//! Word's cascade (ECMA-376 §17.7.2), lowest precedence first:
//! `docDefaults` → (table style) → numbering → paragraph style (up its `basedOn`
//! chain) → character style (runs only) → **direct** formatting. Each layer
//! overlays the one below it, property by property. This module implements the
//! `docDefaults → paragraph-style-chain → character-style-chain → direct` core of
//! that cascade (table-style and numbering layers are a later slice); the
//! `basedOn` chain is walked root-first so a child style overrides its parent, and
//! walking is depth-bounded and cycle-guarded (the model validates against cycles,
//! but the resolver never trusts that).

use casual_doc_model::v1::{
    CnfStyle, DocumentDefaults, FontRef, FontScheme, Indentation, ParagraphProperties, RgbColor,
    RunProperties, Spacing, Style, StyleId, StyleKind, TableLook, TableStyleRegion, ThemeFontRef,
};
use casual_doc_model::v1::{DefinitionMap, Definitions};

/// The maximum `basedOn` chain depth walked. The model rejects cycles, but the
/// resolver is defensive: it never loops, and a pathological chain is simply
/// truncated (the deepest ancestors, which contribute least, are dropped).
const MAX_CHAIN_DEPTH: usize = 64;

/// A read-only view of the document's style hierarchy used to resolve effective
/// properties. Cheap to construct (borrows the definitions) and cheap to query.
#[derive(Clone, Copy, Debug)]
pub struct StyleCascade<'a> {
    styles: &'a DefinitionMap<StyleId, Style>,
    defaults: Option<&'a DocumentDefaults>,
    /// The default paragraph style (`w:style@w:default="1"` of paragraph kind),
    /// applied to a paragraph that names no `pStyle` (Word's implicit `Normal`).
    default_paragraph_style: Option<StyleId>,
}

impl<'a> StyleCascade<'a> {
    /// Builds a cascade view over a document's definitions.
    #[must_use]
    pub fn new(definitions: &'a Definitions) -> Self {
        let default_paragraph_style = definitions
            .styles
            .iter()
            .find(|(_, style)| style.is_default && style.kind == StyleKind::Paragraph)
            .map(|(id, _)| *id);
        Self {
            styles: &definitions.styles,
            defaults: definitions.document_defaults.as_ref(),
            default_paragraph_style,
        }
    }

    /// The style actually in effect for a paragraph: its explicit `pStyle`, or the
    /// document's default paragraph style when it names none.
    #[must_use]
    pub fn paragraph_style(&self, direct: &ParagraphProperties) -> Option<StyleId> {
        direct.style_ref.or(self.default_paragraph_style)
    }

    /// The `basedOn` chain of a style, ordered **root first** (most ancestral) to
    /// the style itself last, so overlaying in order lets a child override its
    /// parent. Depth-bounded and cycle-guarded.
    fn style_chain(&self, start: Option<StyleId>) -> Vec<StyleId> {
        let mut chain: Vec<StyleId> = Vec::new();
        let mut current = start;
        while let Some(id) = current {
            if chain.contains(&id) || chain.len() >= MAX_CHAIN_DEPTH {
                break;
            }
            chain.push(id);
            current = self.styles.get(&id).and_then(|style| style.based_on);
        }
        chain.reverse();
        chain
    }

    /// The effective paragraph properties for a paragraph: `docDefaults.pPr`
    /// overlaid by the paragraph style chain (root → leaf) overlaid by the direct
    /// `w:pPr`. The result carries the same `style_ref` the paragraph declared.
    #[must_use]
    pub fn resolve_paragraph(&self, direct: &ParagraphProperties) -> ParagraphProperties {
        let mut effective = self
            .defaults
            .and_then(|d| d.paragraph.clone())
            .unwrap_or_default();
        for id in self.style_chain(self.paragraph_style(direct)) {
            if let Some(style) = self.styles.get(&id)
                && let Some(ppr) = &style.paragraph
            {
                overlay_paragraph(&mut effective, ppr);
            }
        }
        overlay_paragraph(&mut effective, direct);
        effective
    }

    /// The effective run properties for a run in a paragraph whose effective style
    /// is `paragraph_style`: `docDefaults.rPr` overlaid by the paragraph style
    /// chain's `rPr`, then the run's character-style (`rStyle`) chain's `rPr`, then
    /// the direct `w:rPr`.
    #[must_use]
    pub fn resolve_run(
        &self,
        paragraph_style: Option<StyleId>,
        direct: &RunProperties,
    ) -> RunProperties {
        let mut effective = self
            .defaults
            .and_then(|d| d.run.clone())
            .unwrap_or_default();
        // The paragraph style contributes its run properties to every run.
        for id in self.style_chain(paragraph_style) {
            if let Some(style) = self.styles.get(&id)
                && let Some(rpr) = &style.run
            {
                overlay_run(&mut effective, rpr);
            }
        }
        // The run's own character style chain overrides the paragraph style.
        for id in self.style_chain(direct.style_ref) {
            if let Some(style) = self.styles.get(&id)
                && let Some(rpr) = &style.run
            {
                overlay_run(&mut effective, rpr);
            }
        }
        overlay_run(&mut effective, direct);
        effective
    }

    /// The effective cell background fill contributed by the **table-style**
    /// layer (`w:tblStyle` → its `basedOn` chain, root → leaf): each style's own
    /// region-less base cell/table shading, overlaid by whichever conditional
    /// `w:tblStylePr` regions `cnf` selects (gated by `look`, Word's "Table Style
    /// Options" checkboxes), applied in Word's region precedence — banding
    /// lowest, header/footer row and first/last column next, corner cells
    /// highest — so a cell that matches more than one region (e.g. the header
    /// row's first column) resolves to the more specific one.
    ///
    /// Returns `None` when nothing in the style chain contributes a fill; the
    /// caller falls back further (the table's own direct `w:shd`, then no fill).
    /// This is the table-style layer described in `docs/46` (§ table findings,
    /// "Table style and conditional formatting"): only cell shading is resolved
    /// here — the style's paragraph/run emphasis (e.g. bold header text) and
    /// table/cell border regions are a separate, still-open slice.
    #[must_use]
    pub fn table_style_cell_shading(
        &self,
        style_ref: Option<StyleId>,
        look: TableLook,
        cnf: CnfStyle,
    ) -> Option<RgbColor> {
        let regions = active_table_regions(look, cnf);
        let mut fill = None;
        for id in self.style_chain(style_ref) {
            let Some(style) = self.styles.get(&id) else {
                continue;
            };
            // The style's own region-less base: cell-level shading wins over the
            // style's whole-table shading when both are set.
            if let Some(table_cell) = &style.table_cell
                && table_cell.shading.fill.is_some()
            {
                fill = table_cell.shading.fill;
            } else if let Some(table) = &style.table
                && table.shading.fill.is_some()
            {
                fill = table.shading.fill;
            }
            // Matching conditional regions overlay the base, in precedence order.
            for region in &regions {
                let Some(over) = style.conditional.iter().find(|c| c.region == *region) else {
                    continue;
                };
                if let Some(cell) = &over.table_cell
                    && cell.shading.fill.is_some()
                {
                    fill = cell.shading.fill;
                } else if let Some(table) = &over.table
                    && table.shading.fill.is_some()
                {
                    fill = table.shading.fill;
                }
            }
        }
        fill
    }
}

/// Combines a row's and a cell's `w:cnfStyle` selector bits: Word tolerates a
/// flag being set on either node (banding/header-row flags are typically
/// authored on the row, first/last-column and corner flags on the cell), so the
/// effective selector is the bitwise union.
#[must_use]
pub fn union_cnf(row: CnfStyle, cell: CnfStyle) -> CnfStyle {
    CnfStyle {
        first_row: row.first_row || cell.first_row,
        last_row: row.last_row || cell.last_row,
        first_column: row.first_column || cell.first_column,
        last_column: row.last_column || cell.last_column,
        odd_v_band: row.odd_v_band || cell.odd_v_band,
        even_v_band: row.even_v_band || cell.even_v_band,
        odd_h_band: row.odd_h_band || cell.odd_h_band,
        even_h_band: row.even_h_band || cell.even_h_band,
        first_row_first_column: row.first_row_first_column || cell.first_row_first_column,
        first_row_last_column: row.first_row_last_column || cell.first_row_last_column,
        last_row_first_column: row.last_row_first_column || cell.last_row_first_column,
        last_row_last_column: row.last_row_last_column || cell.last_row_last_column,
    }
}

/// The table-style regions a row/cell's combined `w:cnfStyle` selects, gated by
/// `w:tblLook` and ordered **lowest precedence first** (a later region in this
/// list overlays an earlier one when both match the same cell): vertical then
/// horizontal banding, last/first column, last/first row, then the four corner
/// cells (which require both the row and column flag enabled).
fn active_table_regions(look: TableLook, cnf: CnfStyle) -> Vec<TableStyleRegion> {
    let mut regions = Vec::new();
    if !look.no_v_band {
        if cnf.even_v_band {
            regions.push(TableStyleRegion::Band2Vertical);
        }
        if cnf.odd_v_band {
            regions.push(TableStyleRegion::Band1Vertical);
        }
    }
    if !look.no_h_band {
        if cnf.even_h_band {
            regions.push(TableStyleRegion::Band2Horizontal);
        }
        if cnf.odd_h_band {
            regions.push(TableStyleRegion::Band1Horizontal);
        }
    }
    if look.last_column && cnf.last_column {
        regions.push(TableStyleRegion::LastColumn);
    }
    if look.first_column && cnf.first_column {
        regions.push(TableStyleRegion::FirstColumn);
    }
    if look.last_row && cnf.last_row {
        regions.push(TableStyleRegion::LastRow);
    }
    if look.first_row && cnf.first_row {
        regions.push(TableStyleRegion::FirstRow);
    }
    if look.first_row && look.first_column && cnf.first_row_first_column {
        regions.push(TableStyleRegion::NorthWestCell);
    }
    if look.first_row && look.last_column && cnf.first_row_last_column {
        regions.push(TableStyleRegion::NorthEastCell);
    }
    if look.last_row && look.first_column && cnf.last_row_first_column {
        regions.push(TableStyleRegion::SouthWestCell);
    }
    if look.last_row && look.last_column && cnf.last_row_last_column {
        regions.push(TableStyleRegion::SouthEastCell);
    }
    regions
}

/// The concrete authored family requested by an effective run's Latin font
/// slots. Named values are returned unchanged; theme references resolve through
/// the document's font scheme. This deliberately reports document intent, not
/// the physical face selected later by the renderer's substitution/coverage
/// fallback.
#[must_use]
pub fn requested_font_family(
    properties: &RunProperties,
    scheme: Option<&FontScheme>,
) -> Option<String> {
    let reference = properties
        .font_ref
        .as_ref()
        .or(properties.font_ref_h_ansi.as_ref())?;
    match reference {
        FontRef::Named(name) => Some(name.name.clone()),
        FontRef::Theme(theme) => theme_font_family(theme.slot, scheme),
    }
}

/// Which per-script entry of a theme font collection a slot resolves against.
enum ThemeAxis {
    Latin,
    EastAsia,
    ComplexScript,
}

/// Resolves a `w:rFonts@*Theme` slot to a concrete typeface via the theme font
/// scheme. Empty East-Asian/complex-script entries inherit the Latin entry.
fn theme_font_family(slot: ThemeFontRef, scheme: Option<&FontScheme>) -> Option<String> {
    let scheme = scheme?;
    let (collection, axis) = match slot {
        ThemeFontRef::MajorAscii | ThemeFontRef::MajorHAnsi => (&scheme.major, ThemeAxis::Latin),
        ThemeFontRef::MajorEastAsia => (&scheme.major, ThemeAxis::EastAsia),
        ThemeFontRef::MajorBidi => (&scheme.major, ThemeAxis::ComplexScript),
        ThemeFontRef::MinorAscii | ThemeFontRef::MinorHAnsi => (&scheme.minor, ThemeAxis::Latin),
        ThemeFontRef::MinorEastAsia => (&scheme.minor, ThemeAxis::EastAsia),
        ThemeFontRef::MinorBidi => (&scheme.minor, ThemeAxis::ComplexScript),
    };
    let entry = match axis {
        ThemeAxis::Latin => &collection.latin,
        ThemeAxis::EastAsia => &collection.ea,
        ThemeAxis::ComplexScript => &collection.cs,
    };
    let typeface = if entry.typeface.is_empty() {
        &collection.latin.typeface
    } else {
        &entry.typeface
    };
    (!typeface.is_empty()).then(|| typeface.clone())
}

/// Overlays `over`'s set fields onto `base` (a higher-precedence run layer): every
/// `Some`/present property in `over` replaces `base`'s. Toggle booleans (`bold`
/// etc. are `Option`) follow the same rule; the effect-carrying `shading` and the
/// format-change revision are not layout-relevant but are overlaid when present so
/// the resolved value is complete.
fn overlay_run(base: &mut RunProperties, over: &RunProperties) {
    macro_rules! set {
        ($field:ident) => {
            if over.$field.is_some() {
                base.$field = over.$field.clone();
            }
        };
    }
    set!(style_ref);
    set!(bold);
    set!(bold_complex);
    set!(italic);
    set!(italic_complex);
    set!(underline);
    set!(strike);
    set!(color);
    set!(size_half_points);
    set!(size_complex_half_points);
    set!(font_ref);
    set!(font_ref_h_ansi);
    set!(font_ref_cs);
    set!(font_ref_east_asia);
    set!(font_hint);
    set!(all_caps);
    set!(small_caps);
    set!(hidden);
    set!(web_hidden);
    set!(double_strike);
    set!(vertical_alignment);
    set!(highlight);
    set!(emphasis);
    set!(character_spacing_twips);
    set!(character_scale_percent);
    set!(kerning_half_points);
    set!(position_half_points);
    set!(language);
    set!(outline);
    set!(shadow);
    set!(emboss);
    set!(imprint);
    set!(rtl);
    set!(snap_to_grid);
    set!(spec_vanish);
    set!(border);
    if !over.shading.is_empty() {
        base.shading = over.shading;
    }
}

/// Overlays `over`'s set fields onto `base` (a higher-precedence paragraph layer).
/// `spacing` and `indentation` are **deep-merged** field by field so a paragraph
/// that sets only `w:before` directly still inherits the style's line rule; the
/// `Option` scalars replace when set, and the toggle booleans OR (a style that
/// enables `w:contextualSpacing` keeps it enabled — the model cannot represent an
/// explicit re-disable, so the enabling layer wins).
fn overlay_paragraph(base: &mut ParagraphProperties, over: &ParagraphProperties) {
    if over.style_ref.is_some() {
        base.style_ref = over.style_ref;
    }
    if over.numbering.is_some() {
        base.numbering = over.numbering;
    }
    if over.alignment.is_some() {
        base.alignment = over.alignment;
    }
    base.indentation = merge_indentation(base.indentation, over.indentation);
    base.spacing = merge_spacing(base.spacing, over.spacing);
    if over.drop_cap_frame.is_some() {
        base.drop_cap_frame = over.drop_cap_frame;
    }
    base.keep_next |= over.keep_next;
    base.keep_lines |= over.keep_lines;
    base.page_break_before |= over.page_break_before;
    base.widow_control |= over.widow_control;
    base.contextual_spacing |= over.contextual_spacing;
    if over.outline_level.is_some() {
        base.outline_level = over.outline_level;
    }
    // Custom tab stops: a paragraph's own set replaces the inherited one (Word
    // merges/clears per position, but the common case is a full replacement).
    if !over.tabs.is_empty() {
        base.tabs = over.tabs.clone();
    }
    if !over.borders.is_empty() {
        base.borders = over.borders.clone();
    }
    if !over.shading.is_empty() {
        base.shading = over.shading;
    }
    if over.mark_run.is_some() {
        base.mark_run = over.mark_run.clone();
    }
}

/// Deep-merges two optional [`Indentation`] values (over wins per field).
fn merge_indentation(base: Option<Indentation>, over: Option<Indentation>) -> Option<Indentation> {
    match (base, over) {
        (None, over) => over,
        (base, None) => base,
        (Some(mut base), Some(over)) => {
            if over.start_twips.is_some() {
                base.start_twips = over.start_twips;
            }
            if over.end_twips.is_some() {
                base.end_twips = over.end_twips;
            }
            if over.first_line_twips.is_some() {
                base.first_line_twips = over.first_line_twips;
            }
            if over.hanging_twips.is_some() {
                base.hanging_twips = over.hanging_twips;
            }
            Some(base)
        }
    }
}

/// Deep-merges two optional [`Spacing`] values (over wins per field). The line
/// rule and its value move together: if `over` specifies any line rule/value/
/// percent, it fully replaces `base`'s line spacing (mixing an `over` percent with
/// a `base` exact rule would be meaningless), while `before`/`after` merge
/// independently.
fn merge_spacing(base: Option<Spacing>, over: Option<Spacing>) -> Option<Spacing> {
    match (base, over) {
        (None, over) => over,
        (base, None) => base,
        (Some(mut base), Some(over)) => {
            if over.before_twips.is_some() {
                base.before_twips = over.before_twips;
            }
            if over.after_twips.is_some() {
                base.after_twips = over.after_twips;
            }
            if over.before_auto.is_some() {
                base.before_auto = over.before_auto;
            }
            if over.after_auto.is_some() {
                base.after_auto = over.after_auto;
            }
            // Line spacing is a unit: any line specification in `over` replaces
            // `base`'s wholesale so a rule never desyncs from its value.
            if over.line_percent.is_some() || over.line_rule.is_some() || over.line_twips.is_some()
            {
                base.line_percent = over.line_percent;
                base.line_rule = over.line_rule;
                base.line_twips = over.line_twips;
            }
            Some(base)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{
        Alignment, Definitions, FontCollection, FontName, LineRule, RgbColor, Shading,
        TableCellProperties, TableStyleOverride, ThemeFont, ThemeFontEntry,
    };

    fn style_id(n: u64) -> StyleId {
        StyleId::new(NodeId::from_parts(n, 1).unwrap())
    }

    fn paragraph_style(_id: u64, based_on: Option<u64>, ppr: ParagraphProperties) -> Style {
        Style {
            kind: StyleKind::Paragraph,
            is_default: false,
            name: None,
            aliases: None,
            based_on: based_on.map(style_id),
            next: None,
            link: None,
            hidden: false,
            ui_priority: None,
            semi_hidden: false,
            unhide_when_used: false,
            q_format: false,
            locked: false,
            paragraph: Some(ppr),
            run: None,
            table: None,
            table_row: None,
            table_cell: None,
            conditional: Vec::new(),
        }
    }

    fn table_style(
        based_on: Option<u64>,
        table_cell: Option<TableCellProperties>,
        conditional: Vec<TableStyleOverride>,
    ) -> Style {
        Style {
            kind: StyleKind::Table,
            is_default: false,
            name: None,
            aliases: None,
            based_on: based_on.map(style_id),
            next: None,
            link: None,
            hidden: false,
            ui_priority: None,
            semi_hidden: false,
            unhide_when_used: false,
            q_format: false,
            locked: false,
            paragraph: None,
            run: None,
            table: None,
            table_row: None,
            table_cell,
            conditional,
        }
    }

    fn rgb(r: u8, g: u8, b: u8) -> RgbColor {
        RgbColor { r, g, b }
    }

    fn shaded_cell(fill: RgbColor) -> TableCellProperties {
        TableCellProperties {
            shading: Shading { fill: Some(fill) },
            ..TableCellProperties::default()
        }
    }

    fn defs_with(styles: Vec<(u64, Style)>, defaults: Option<DocumentDefaults>) -> Definitions {
        let mut definitions = Definitions::default();
        for (id, style) in styles {
            definitions.styles.insert(style_id(id), style);
        }
        definitions.document_defaults = defaults;
        definitions
    }

    #[test]
    fn direct_overrides_style_overrides_docdefaults_for_size() {
        // docDefaults sz=22 (11pt); style sz=48 (24pt); direct sz=20 (10pt).
        let defaults = DocumentDefaults {
            run: Some(RunProperties {
                size_half_points: Some(22),
                ..RunProperties::default()
            }),
            paragraph: None,
        };
        let heading = Style {
            run: Some(RunProperties {
                size_half_points: Some(48),
                bold: Some(true),
                ..RunProperties::default()
            }),
            ..paragraph_style(10, None, ParagraphProperties::default())
        };
        let definitions = defs_with(vec![(10, heading)], Some(defaults));
        let cascade = StyleCascade::new(&definitions);

        // A run in a Heading paragraph with no direct size inherits sz=48 + bold.
        let para = ParagraphProperties {
            style_ref: Some(style_id(10)),
            ..ParagraphProperties::default()
        };
        let inherited =
            cascade.resolve_run(cascade.paragraph_style(&para), &RunProperties::default());
        assert_eq!(inherited.size_half_points, Some(48));
        assert_eq!(inherited.bold, Some(true));

        // A direct size overrides the style.
        let direct = RunProperties {
            size_half_points: Some(20),
            ..RunProperties::default()
        };
        let overridden = cascade.resolve_run(cascade.paragraph_style(&para), &direct);
        assert_eq!(overridden.size_half_points, Some(20));
        assert_eq!(overridden.bold, Some(true), "bold still inherited");
    }

    #[test]
    fn requested_font_reports_authored_named_or_theme_family() {
        let named = RunProperties {
            font_ref: Some(FontRef::Named(FontName {
                name: "Document Sans".to_owned(),
            })),
            ..RunProperties::default()
        };
        assert_eq!(
            requested_font_family(&named, None).as_deref(),
            Some("Document Sans")
        );

        let themed = RunProperties {
            font_ref_h_ansi: Some(FontRef::Theme(ThemeFont {
                slot: ThemeFontRef::MinorHAnsi,
            })),
            ..RunProperties::default()
        };
        let scheme = FontScheme {
            minor: FontCollection {
                latin: ThemeFontEntry {
                    typeface: "Aptos".to_owned(),
                    ..ThemeFontEntry::default()
                },
                ..FontCollection::default()
            },
            ..FontScheme::default()
        };
        assert_eq!(
            requested_font_family(&themed, Some(&scheme)).as_deref(),
            Some("Aptos")
        );
    }

    #[test]
    fn based_on_chain_resolves_child_over_parent() {
        // Base sets alignment=center + before=100; child overrides alignment=end.
        let base = paragraph_style(
            1,
            None,
            ParagraphProperties {
                alignment: Some(Alignment::Center),
                spacing: Some(Spacing {
                    before_twips: Some(100),
                    ..Spacing::default()
                }),
                ..ParagraphProperties::default()
            },
        );
        let child = paragraph_style(
            2,
            Some(1),
            ParagraphProperties {
                alignment: Some(Alignment::End),
                ..ParagraphProperties::default()
            },
        );
        let definitions = defs_with(vec![(1, base), (2, child)], None);
        let cascade = StyleCascade::new(&definitions);
        let para = ParagraphProperties {
            style_ref: Some(style_id(2)),
            ..ParagraphProperties::default()
        };
        let eff = cascade.resolve_paragraph(&para);
        assert_eq!(
            eff.alignment,
            Some(Alignment::End),
            "child overrides parent"
        );
        assert_eq!(
            eff.spacing.and_then(|s| s.before_twips),
            Some(100),
            "inherited from parent"
        );
    }

    #[test]
    fn spacing_deep_merges_direct_before_over_style_line_rule() {
        // Style sets an exact line rule; the paragraph sets only `before` directly.
        let styled = paragraph_style(
            5,
            None,
            ParagraphProperties {
                spacing: Some(Spacing {
                    line_rule: Some(LineRule::Exact),
                    line_twips: Some(260),
                    ..Spacing::default()
                }),
                ..ParagraphProperties::default()
            },
        );
        let definitions = defs_with(vec![(5, styled)], None);
        let cascade = StyleCascade::new(&definitions);
        let para = ParagraphProperties {
            style_ref: Some(style_id(5)),
            spacing: Some(Spacing {
                before_twips: Some(40),
                ..Spacing::default()
            }),
            ..ParagraphProperties::default()
        };
        let eff = cascade.resolve_paragraph(&para).spacing.unwrap();
        assert_eq!(eff.before_twips, Some(40), "direct before applied");
        assert_eq!(eff.line_rule, Some(LineRule::Exact), "style line rule kept");
        assert_eq!(eff.line_twips, Some(260));
    }

    #[test]
    fn based_on_cycle_is_broken_without_looping() {
        // A self-referential (and mutually-referential) chain must terminate.
        let a = paragraph_style(1, Some(2), ParagraphProperties::default());
        let b = paragraph_style(2, Some(1), ParagraphProperties::default());
        let definitions = defs_with(vec![(1, a), (2, b)], None);
        let cascade = StyleCascade::new(&definitions);
        // Must not hang; returns a bounded chain.
        assert!(cascade.style_chain(Some(style_id(1))).len() <= 2);
    }

    #[test]
    fn default_paragraph_style_applies_without_explicit_pstyle() {
        let mut normal = paragraph_style(
            1,
            None,
            ParagraphProperties {
                alignment: Some(Alignment::Justify),
                ..ParagraphProperties::default()
            },
        );
        normal.is_default = true;
        let definitions = defs_with(vec![(1, normal)], None);
        let cascade = StyleCascade::new(&definitions);
        // A paragraph with no pStyle still inherits the default style's alignment.
        let eff = cascade.resolve_paragraph(&ParagraphProperties::default());
        assert_eq!(eff.alignment, Some(Alignment::Justify));
    }

    #[test]
    fn table_style_base_cell_shading_applies_with_no_cnf() {
        // The style's own (region-less) base cell shading applies to every cell,
        // even one that carries no `w:cnfStyle` selector at all.
        let style = table_style(None, Some(shaded_cell(rgb(200, 200, 200))), Vec::new());
        let definitions = defs_with(vec![(1, style)], None);
        let cascade = StyleCascade::new(&definitions);
        let fill = cascade.table_style_cell_shading(
            Some(style_id(1)),
            TableLook::default(),
            CnfStyle::default(),
        );
        assert_eq!(fill, Some(rgb(200, 200, 200)));
    }

    #[test]
    fn first_row_conditional_region_overrides_the_base_when_tbl_look_enables_it() {
        let style = table_style(
            None,
            Some(shaded_cell(rgb(200, 200, 200))),
            vec![TableStyleOverride {
                region: TableStyleRegion::FirstRow,
                paragraph: None,
                run: None,
                table: None,
                table_row: None,
                table_cell: Some(shaded_cell(rgb(50, 90, 160))),
            }],
        );
        let definitions = defs_with(vec![(1, style)], None);
        let cascade = StyleCascade::new(&definitions);
        let cnf = CnfStyle {
            first_row: true,
            ..CnfStyle::default()
        };

        // `tblLook` must enable the header-row option or the region is ignored
        // and the plain base fill remains in effect (the "Header Row" checkbox
        // unticked in Word).
        let look_disabled = TableLook::default();
        assert_eq!(
            cascade.table_style_cell_shading(Some(style_id(1)), look_disabled, cnf),
            Some(rgb(200, 200, 200)),
            "first-row region ignored when tblLook.first_row is unset"
        );

        let look_enabled = TableLook {
            first_row: true,
            ..TableLook::default()
        };
        assert_eq!(
            cascade.table_style_cell_shading(Some(style_id(1)), look_enabled, cnf),
            Some(rgb(50, 90, 160)),
            "first-row region overrides the base fill once tblLook enables it"
        );
    }

    #[test]
    fn banded_row_regions_alternate_by_odd_even_cnf_bit() {
        let style = table_style(
            None,
            None,
            vec![
                TableStyleOverride {
                    region: TableStyleRegion::Band1Horizontal,
                    paragraph: None,
                    run: None,
                    table: None,
                    table_row: None,
                    table_cell: Some(shaded_cell(rgb(255, 255, 255))),
                },
                TableStyleOverride {
                    region: TableStyleRegion::Band2Horizontal,
                    paragraph: None,
                    run: None,
                    table: None,
                    table_row: None,
                    table_cell: Some(shaded_cell(rgb(230, 230, 230))),
                },
            ],
        );
        let definitions = defs_with(vec![(1, style)], None);
        let cascade = StyleCascade::new(&definitions);
        let look = TableLook::default(); // no_h_band=false: banding is active.

        let odd = CnfStyle {
            odd_h_band: true,
            ..CnfStyle::default()
        };
        let even = CnfStyle {
            even_h_band: true,
            ..CnfStyle::default()
        };
        assert_eq!(
            cascade.table_style_cell_shading(Some(style_id(1)), look, odd),
            Some(rgb(255, 255, 255))
        );
        assert_eq!(
            cascade.table_style_cell_shading(Some(style_id(1)), look, even),
            Some(rgb(230, 230, 230))
        );

        // Suppressing horizontal banding (`w:tblLook@noHBand`) turns both off.
        let no_band = TableLook {
            no_h_band: true,
            ..TableLook::default()
        };
        assert_eq!(
            cascade.table_style_cell_shading(Some(style_id(1)), no_band, odd),
            None
        );
    }

    #[test]
    fn corner_cell_region_wins_over_first_row_and_first_column() {
        let style = table_style(
            None,
            None,
            vec![
                TableStyleOverride {
                    region: TableStyleRegion::FirstRow,
                    paragraph: None,
                    run: None,
                    table: None,
                    table_row: None,
                    table_cell: Some(shaded_cell(rgb(50, 90, 160))),
                },
                TableStyleOverride {
                    region: TableStyleRegion::FirstColumn,
                    paragraph: None,
                    run: None,
                    table: None,
                    table_row: None,
                    table_cell: Some(shaded_cell(rgb(90, 50, 160))),
                },
                TableStyleOverride {
                    region: TableStyleRegion::NorthWestCell,
                    paragraph: None,
                    run: None,
                    table: None,
                    table_row: None,
                    table_cell: Some(shaded_cell(rgb(20, 20, 20))),
                },
            ],
        );
        let definitions = defs_with(vec![(1, style)], None);
        let cascade = StyleCascade::new(&definitions);
        let look = TableLook {
            first_row: true,
            first_column: true,
            ..TableLook::default()
        };
        let cnf = CnfStyle {
            first_row: true,
            first_column: true,
            first_row_first_column: true,
            ..CnfStyle::default()
        };
        assert_eq!(
            cascade.table_style_cell_shading(Some(style_id(1)), look, cnf),
            Some(rgb(20, 20, 20)),
            "the north-west corner region outranks first-row and first-column"
        );
    }

    #[test]
    fn table_style_shading_inherits_up_the_based_on_chain() {
        let base = table_style(None, Some(shaded_cell(rgb(10, 20, 30))), Vec::new());
        let child = table_style(Some(1), None, Vec::new());
        let definitions = defs_with(vec![(1, base), (2, child)], None);
        let cascade = StyleCascade::new(&definitions);
        let fill = cascade.table_style_cell_shading(
            Some(style_id(2)),
            TableLook::default(),
            CnfStyle::default(),
        );
        assert_eq!(
            fill,
            Some(rgb(10, 20, 30)),
            "child style inherits the ancestor's base fill"
        );
    }

    #[test]
    fn union_cnf_combines_row_and_cell_bits() {
        let row = CnfStyle {
            first_row: true,
            odd_h_band: true,
            ..CnfStyle::default()
        };
        let cell = CnfStyle {
            first_column: true,
            ..CnfStyle::default()
        };
        let combined = union_cnf(row, cell);
        assert!(combined.first_row);
        assert!(combined.odd_h_band);
        assert!(combined.first_column);
        assert!(!combined.last_row);
    }
}
