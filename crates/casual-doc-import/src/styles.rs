//! Styles-part parsing: OOXML string style ids -> deterministic v1 StyleIds,
//! basedOn inheritance (dangling/kind-mismatch/cycle broken and reported),
//! per-style metadata (`w:name`/`w:next`/`w:link`/`w:uiPriority`/toggles),
//! table-style properties (`w:tblPr`/`w:trPr`/`w:tcPr`) with `w:tblStylePr`
//! conditional formatting, and reporting of unmapped style-part constructs.
//!
//! The style body is parsed by recursive descent so the nested `w:tblStylePr`
//! overrides (each carrying its own pPr/rPr/tblPr/trPr/tcPr) map cleanly.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{
    Alignment, BorderEdge, CellMargins, CellVerticalAlignment, DefinitionMap, DocumentDefaults,
    HeightRule, ParagraphBorders, ParagraphProperties, RgbColor, RowHeight, RunProperties, Shading,
    Style, StyleId, StyleKind, TabAlignment, TabLeader, TabStop, TableBorders, TableCellProperties,
    TableLayout, TableLook, TableOverlap, TableProperties, TableRowProperties, TableStyleOverride,
    TableStyleRegion, TextDirection, VerticalMerge,
};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::numbering::Numbering;
use crate::properties::{
    apply_paragraph_property, apply_run_property, attribute_value, is_true, parse_rgb,
    style_kind_from,
};
use crate::report::Reporter;

/// Resolved style definitions and their name/kind index.
#[derive(Debug, Default)]
pub(crate) struct Styles {
    by_name: BTreeMap<String, (StyleId, StyleKind)>,
    definitions: DefinitionMap<StyleId, Style>,
    document_defaults: Option<DocumentDefaults>,
    /// Styles whose `w:pPr/w:numPr` was captured raw during parse; resolved to
    /// each style's `paragraph.numbering` by [`Styles::resolve_numbering`] once the
    /// numbering part is parsed (the numbering `numId -> instance` map does not
    /// exist while the styles part is being parsed).
    pending_numbering: Vec<(StyleId, String, u8)>,
}

impl Styles {
    /// Resolves the deferred style-level `w:numPr` captures (a paragraph style's
    /// list membership, e.g. `ListBullet -> numId`) against the now-parsed
    /// numbering table, setting each style's `paragraph.numbering`. Must run after
    /// the numbering part is parsed. A `numId` with no instance (or an undefined
    /// level) is reported, matching the body parser.
    pub(crate) fn resolve_numbering(&mut self, numbering: &Numbering, reporter: &mut Reporter) {
        for (style_id, num_id, level) in std::mem::take(&mut self.pending_numbering) {
            let Some(mut style) = self.definitions.get(&style_id).cloned() else {
                continue;
            };
            match numbering.resolve(&num_id, level) {
                Some(reference) => {
                    style
                        .paragraph
                        .get_or_insert_with(ParagraphProperties::default)
                        .numbering = Some(reference);
                    self.definitions.insert(style_id, style);
                }
                None => reporter.report(b"numPr"),
            }
        }
    }
}

impl Styles {
    /// Resolves a style name to its id, requiring the kind to match the site.
    pub(crate) fn resolve(&self, name: &str, expected: StyleKind) -> Option<StyleId> {
        self.by_name
            .get(name)
            .filter(|(_, kind)| *kind == expected)
            .map(|(id, _)| *id)
    }

    /// The `w:docDefaults` run/paragraph defaults, if the part carried any.
    pub(crate) fn document_defaults(&self) -> Option<DocumentDefaults> {
        self.document_defaults.clone()
    }

    pub(crate) fn into_definitions(self) -> DefinitionMap<StyleId, Style> {
        self.definitions
    }
}

/// A style parsed but not yet id-resolved: `basedOn`/`next`/`link` are still the
/// producer's name strings, resolved to ids once every style has an id.
struct RawStyle {
    style_id: String,
    kind: Option<StyleKind>,
    is_default: bool,
    name: Option<String>,
    aliases: Option<String>,
    based_on: Option<String>,
    next: Option<String>,
    link: Option<String>,
    hidden: bool,
    ui_priority: Option<i32>,
    semi_hidden: bool,
    unhide_when_used: bool,
    q_format: bool,
    locked: bool,
    paragraph: Option<ParagraphProperties>,
    run: Option<RunProperties>,
    table: Option<TableProperties>,
    table_row: Option<TableRowProperties>,
    table_cell: Option<TableCellProperties>,
    conditional: Vec<TableStyleOverride>,
    /// A style's `w:pPr/w:numPr` (raw numId + ilvl), resolved to the style's
    /// `paragraph.numbering` in the deferred pass once numbering is parsed.
    pending_numbering: Option<(String, u8)>,
}

/// Parses the styles part into resolved styles, allocating ids from `ids`.
pub(crate) fn parse(
    xml: &[u8],
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<Styles, ImportError> {
    let (raw, document_defaults) = parse_raw(xml, reporter, config)?;

    let mut by_name: BTreeMap<String, (StyleId, StyleKind)> = BTreeMap::new();
    let mut assigned: Vec<(StyleId, StyleKind, Option<String>, RawStyle)> = Vec::new();
    for style in raw {
        let Some(kind) = style.kind else {
            reporter.report(b"style");
            continue;
        };
        if by_name.contains_key(&style.style_id) {
            reporter.report(b"style");
            continue;
        }
        let id = StyleId::new(next_id(ids)?);
        by_name.insert(style.style_id.clone(), (id, kind));
        assigned.push((id, kind, style.based_on.clone(), style));
    }

    // Resolve basedOn candidates (dangling / kind-mismatch dropped + reported).
    let mut candidates: Vec<(StyleId, StyleKind, Option<StyleId>, RawStyle)> = Vec::new();
    for (id, kind, based_on_name, style) in assigned {
        let based_on = match based_on_name {
            Some(name) => match by_name.get(&name) {
                Some((base, base_kind)) if *base_kind == kind => Some(*base),
                _ => {
                    reporter.report(b"basedOn");
                    None
                }
            },
            None => None,
        };
        candidates.push((id, kind, based_on, style));
    }

    // Break basedOn cycles by dropping the edge that closes each.
    let edges: BTreeMap<StyleId, StyleId> = candidates
        .iter()
        .filter_map(|(id, _, based_on, _)| based_on.map(|base| (*id, base)))
        .collect();
    let mut cyclic: BTreeSet<StyleId> = BTreeSet::new();
    for &start in edges.keys() {
        let mut visited = BTreeSet::new();
        let mut current = start;
        loop {
            if !visited.insert(current) {
                cyclic.insert(current);
                break;
            }
            match edges.get(&current) {
                Some(&next) if !cyclic.contains(&next) => current = next,
                _ => break,
            }
        }
    }

    let mut definitions = DefinitionMap::default();
    let mut pending_numbering: Vec<(StyleId, String, u8)> = Vec::new();
    for (id, kind, based_on, style) in candidates {
        if let Some((num_id, ilvl)) = style.pending_numbering.clone() {
            pending_numbering.push((id, num_id, ilvl));
        }
        let based_on = if cyclic.contains(&id) {
            reporter.report(b"basedOn");
            None
        } else {
            based_on
        };
        // `next`/`link` resolve by name to any style (no kind constraint —
        // `link` deliberately points at the companion style of the other kind);
        // a name that names no style is dropped and reported.
        let next = resolve_reference(&style.next, &by_name, reporter, b"next");
        let link = resolve_reference(&style.link, &by_name, reporter, b"link");
        definitions.insert(
            id,
            Style {
                kind,
                is_default: style.is_default,
                name: style.name,
                aliases: style.aliases,
                based_on,
                next,
                link,
                hidden: style.hidden,
                ui_priority: style.ui_priority,
                semi_hidden: style.semi_hidden,
                unhide_when_used: style.unhide_when_used,
                q_format: style.q_format,
                locked: style.locked,
                paragraph: style.paragraph,
                run: style.run,
                table: style.table,
                table_row: style.table_row,
                table_cell: style.table_cell,
                conditional: style.conditional,
            },
        );
    }

    Ok(Styles {
        by_name,
        definitions,
        document_defaults,
        pending_numbering,
    })
}

/// Resolves an optional style-name reference to its id, reporting a dangling one.
fn resolve_reference(
    name: &Option<String>,
    by_name: &BTreeMap<String, (StyleId, StyleKind)>,
    reporter: &mut Reporter,
    label: &[u8],
) -> Option<StyleId> {
    match name {
        Some(name) => match by_name.get(name) {
            Some((id, _)) => Some(*id),
            None => {
                reporter.report(label);
                None
            }
        },
        None => None,
    }
}

fn next_id(ids: &mut IdGenerator) -> Result<casual_doc_model::NodeId, ImportError> {
    ids.next_id()
        .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })
}

/// Bounded parse context threaded through the recursive readers.
struct Ctx<'a> {
    reporter: &'a mut Reporter,
    max_depth: u64,
    max_elements: u64,
    elements: u64,
}

impl Ctx<'_> {
    fn bump(&mut self) -> Result<(), ImportError> {
        self.elements += 1;
        if self.elements > self.max_elements {
            return Err(ImportError::LimitExceeded {
                limit: "xml_elements",
            });
        }
        Ok(())
    }

    fn check_depth(&self, depth: u64) -> Result<(), ImportError> {
        if depth > self.max_depth {
            return Err(ImportError::LimitExceeded { limit: "xml_depth" });
        }
        Ok(())
    }

    fn report(&mut self, name: &[u8]) {
        self.reporter.report(name);
    }
}

/// One structural step from the reader: an element open/empty (owned so the
/// shared buffer can be reused during recursion), a close, or end of input.
enum Node {
    Open(BytesStart<'static>),
    Empty(BytesStart<'static>),
    Close,
    Eof,
}

fn read_node(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
) -> Result<Node, ImportError> {
    loop {
        let node = match reader
            .read_event_into(buffer)
            .map_err(|_| ImportError::MalformedXml)?
        {
            Event::Eof => Node::Eof,
            Event::DocType(_) => return Err(ImportError::MalformedXml),
            Event::Start(element) => {
                ctx.bump()?;
                Node::Open(element.into_owned())
            }
            Event::Empty(element) => {
                ctx.bump()?;
                Node::Empty(element.into_owned())
            }
            Event::End(_) => Node::Close,
            _ => {
                buffer.clear();
                continue;
            }
        };
        buffer.clear();
        return Ok(node);
    }
}

/// Consumes the children of the just-opened element up to its matching close.
fn skip_subtree(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
) -> Result<(), ImportError> {
    let mut depth = 1_u64;
    loop {
        match read_node(reader, buffer, ctx)? {
            Node::Open(_) => depth += 1,
            Node::Empty(_) => {}
            Node::Close => {
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
            }
            Node::Eof => return Ok(()),
        }
    }
}

/// Accumulates the property containers a style (or a `w:tblStylePr` region)
/// may carry. `has_*` records the element's presence (`Some(default)` survives).
#[derive(Default)]
struct PropAcc {
    paragraph: ParagraphProperties,
    has_paragraph: bool,
    /// A style's `w:pPr/w:numPr` (numId + ilvl) captured raw, because the
    /// numbering part is parsed after the styles part so the `numId -> instance`
    /// map is not yet available; resolved to `paragraph.numbering` in a deferred
    /// pass ([`Styles::resolve_numbering`]) once numbering is parsed.
    pending_num_id: Option<String>,
    pending_ilvl: u8,
    run: RunProperties,
    has_run: bool,
    table: TableProperties,
    has_table: bool,
    table_row: TableRowProperties,
    has_table_row: bool,
    table_cell: TableCellProperties,
    has_table_cell: bool,
}

fn parse_raw(
    xml: &[u8],
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<(Vec<RawStyle>, Option<DocumentDefaults>), ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut ctx = Ctx {
        reporter,
        max_depth: config.max_depth,
        max_elements: config.max_elements,
        elements: 0,
    };
    let mut styles = Vec::new();
    let mut document_defaults = None;

    loop {
        match read_node(&mut reader, &mut buffer, &mut ctx)? {
            Node::Eof => break,
            Node::Close => {}
            Node::Empty(element) => {
                if element.local_name().as_ref() == b"style" {
                    styles.push(empty_style(&element));
                }
            }
            Node::Open(element) => match element.local_name().as_ref() {
                // The `w:styles` root: fall through so its children are read by
                // the same top-level loop.
                b"styles" => {}
                b"style" => {
                    styles.push(read_style(&mut reader, &mut buffer, &mut ctx, 1, &element)?);
                }
                b"docDefaults" => {
                    document_defaults =
                        Some(read_doc_defaults(&mut reader, &mut buffer, &mut ctx, 1)?);
                }
                _ => skip_subtree(&mut reader, &mut buffer, &mut ctx)?,
            },
        }
    }
    Ok((styles, document_defaults))
}

/// A `<w:style .../>` with no body: only its attributes are meaningful.
fn empty_style(element: &BytesStart<'_>) -> RawStyle {
    RawStyle {
        style_id: attribute_value(element, b"styleId").unwrap_or_default(),
        kind: attribute_value(element, b"type")
            .as_deref()
            .and_then(style_kind_from),
        is_default: style_default_attr(element),
        name: None,
        aliases: None,
        based_on: None,
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
        table_cell: None,
        conditional: Vec::new(),
        pending_numbering: None,
    }
}

fn style_default_attr(element: &BytesStart<'_>) -> bool {
    attribute_value(element, b"default")
        .as_deref()
        .map(|value| is_true(Some(value)))
        .unwrap_or(false)
}

fn read_style(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
    element: &BytesStart<'_>,
) -> Result<RawStyle, ImportError> {
    ctx.check_depth(depth)?;
    let mut raw = empty_style(element);
    let mut acc = PropAcc::default();
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        let mut consumed = false;
        match child.local_name().as_ref() {
            b"name" => raw.name = attribute_value(&child, b"val"),
            b"aliases" => raw.aliases = attribute_value(&child, b"val"),
            b"basedOn" => raw.based_on = attribute_value(&child, b"val"),
            b"next" => raw.next = attribute_value(&child, b"val"),
            b"link" => raw.link = attribute_value(&child, b"val"),
            b"uiPriority" => {
                raw.ui_priority =
                    attribute_value(&child, b"val").and_then(|value| value.parse::<i32>().ok());
            }
            b"hidden" => raw.hidden = is_true(attribute_value(&child, b"val").as_deref()),
            b"semiHidden" => raw.semi_hidden = is_true(attribute_value(&child, b"val").as_deref()),
            b"unhideWhenUsed" => {
                raw.unhide_when_used = is_true(attribute_value(&child, b"val").as_deref());
            }
            b"qFormat" => raw.q_format = is_true(attribute_value(&child, b"val").as_deref()),
            b"locked" => raw.locked = is_true(attribute_value(&child, b"val").as_deref()),
            b"pPr" => {
                acc.has_paragraph = true;
                if open {
                    read_paragraph_container(reader, buffer, ctx, depth + 1, &mut acc)?;
                    consumed = true;
                }
            }
            b"rPr" => {
                acc.has_run = true;
                if open {
                    read_run_container(reader, buffer, ctx, depth + 1, &mut acc.run)?;
                    consumed = true;
                }
            }
            b"tblPr" => {
                acc.has_table = true;
                if open {
                    read_table_container(reader, buffer, ctx, depth + 1, &mut acc.table)?;
                    consumed = true;
                }
            }
            b"trPr" => {
                acc.has_table_row = true;
                if open {
                    read_row_container(reader, buffer, ctx, depth + 1, &mut acc.table_row)?;
                    consumed = true;
                }
            }
            b"tcPr" => {
                acc.has_table_cell = true;
                if open {
                    read_cell_container(reader, buffer, ctx, depth + 1, &mut acc.table_cell)?;
                    consumed = true;
                }
            }
            b"tblStylePr" => {
                let region = attribute_value(&child, b"type")
                    .as_deref()
                    .and_then(region_from);
                if open {
                    match read_override(reader, buffer, ctx, depth + 1, region)? {
                        Some(over) if raw.conditional.len() < 128 => raw.conditional.push(over),
                        Some(_) => ctx.report(b"tblStylePr"),
                        None => {}
                    }
                    consumed = true;
                } else if let Some(region) = region {
                    if raw.conditional.len() < 128 {
                        raw.conditional.push(TableStyleOverride {
                            region,
                            paragraph: None,
                            run: None,
                            table: None,
                            table_row: None,
                            table_cell: None,
                        });
                    }
                } else {
                    ctx.report(b"tblStylePr");
                }
            }
            other => ctx.report(other),
        }
        if open && !consumed {
            skip_subtree(reader, buffer, ctx)?;
        }
    }
    raw.paragraph = acc.has_paragraph.then_some(acc.paragraph);
    raw.pending_numbering = acc
        .pending_num_id
        .take()
        .map(|num_id| (num_id, acc.pending_ilvl));
    raw.run = acc.has_run.then_some(acc.run);
    raw.table = acc.has_table.then_some(acc.table);
    raw.table_row = acc.has_table_row.then_some(acc.table_row);
    raw.table_cell = acc.has_table_cell.then_some(acc.table_cell);
    Ok(raw)
}

/// Reads a `w:tblStylePr` region's property overrides into a fresh accumulator.
fn read_override(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
    region: Option<TableStyleRegion>,
) -> Result<Option<TableStyleOverride>, ImportError> {
    ctx.check_depth(depth)?;
    let mut acc = PropAcc::default();
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        let mut consumed = false;
        match child.local_name().as_ref() {
            b"pPr" => {
                acc.has_paragraph = true;
                if open {
                    read_paragraph_container(reader, buffer, ctx, depth + 1, &mut acc)?;
                    consumed = true;
                }
            }
            b"rPr" => {
                acc.has_run = true;
                if open {
                    read_run_container(reader, buffer, ctx, depth + 1, &mut acc.run)?;
                    consumed = true;
                }
            }
            b"tblPr" => {
                acc.has_table = true;
                if open {
                    read_table_container(reader, buffer, ctx, depth + 1, &mut acc.table)?;
                    consumed = true;
                }
            }
            b"trPr" => {
                acc.has_table_row = true;
                if open {
                    read_row_container(reader, buffer, ctx, depth + 1, &mut acc.table_row)?;
                    consumed = true;
                }
            }
            b"tcPr" => {
                acc.has_table_cell = true;
                if open {
                    read_cell_container(reader, buffer, ctx, depth + 1, &mut acc.table_cell)?;
                    consumed = true;
                }
            }
            other => ctx.report(other),
        }
        if open && !consumed {
            skip_subtree(reader, buffer, ctx)?;
        }
    }
    let Some(region) = region else {
        ctx.report(b"tblStylePr");
        return Ok(None);
    };
    Ok(Some(TableStyleOverride {
        region,
        paragraph: acc.has_paragraph.then_some(acc.paragraph),
        run: acc.has_run.then_some(acc.run),
        table: acc.has_table.then_some(acc.table),
        table_row: acc.has_table_row.then_some(acc.table_row),
        table_cell: acc.has_table_cell.then_some(acc.table_cell),
    }))
}

/// Reads a `w:docDefaults` block: its `w:pPrDefault`/`w:rPrDefault` wrappers are
/// structural (skipped); the nested `w:pPr`/`w:rPr` reuse the property readers.
fn read_doc_defaults(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
) -> Result<DocumentDefaults, ImportError> {
    ctx.check_depth(depth)?;
    let mut acc = PropAcc::default();
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        match child.local_name().as_ref() {
            b"pPrDefault" | b"rPrDefault" => {
                if open {
                    read_default_wrapper(reader, buffer, ctx, depth + 1, &mut acc)?;
                }
            }
            _ => {
                if open {
                    skip_subtree(reader, buffer, ctx)?;
                }
            }
        }
    }
    Ok(DocumentDefaults {
        paragraph: acc.has_paragraph.then_some(acc.paragraph),
        run: acc.has_run.then_some(acc.run),
    })
}

fn read_default_wrapper(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
    acc: &mut PropAcc,
) -> Result<(), ImportError> {
    ctx.check_depth(depth)?;
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        match child.local_name().as_ref() {
            b"pPr" => {
                acc.has_paragraph = true;
                if open {
                    read_paragraph_container(reader, buffer, ctx, depth + 1, acc)?;
                }
            }
            b"rPr" => {
                acc.has_run = true;
                if open {
                    read_run_container(reader, buffer, ctx, depth + 1, &mut acc.run)?;
                }
            }
            _ => {
                if open {
                    skip_subtree(reader, buffer, ctx)?;
                }
            }
        }
    }
    Ok(())
}

/// Reads a `w:pPr` block. A nested `w:rPr` (the paragraph-mark run properties)
/// is folded into the same style's run properties, matching the flat parser this
/// replaced; other unmodeled children are reported.
fn read_paragraph_container(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
    acc: &mut PropAcc,
) -> Result<(), ImportError> {
    ctx.check_depth(depth)?;
    acc.has_paragraph = true;
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        let mut consumed = false;
        match child.local_name().as_ref() {
            b"rPr" => {
                acc.has_run = true;
                if open {
                    read_run_container(reader, buffer, ctx, depth + 1, &mut acc.run)?;
                    consumed = true;
                }
            }
            // `w:pBdr` is a container of edge children, so the flat
            // `apply_paragraph_property` (leaf elements only) cannot read it; a
            // style-sourced paragraph border (e.g. a heading rule) is captured
            // here, matching the body parser's edge handling.
            b"pBdr" => {
                if open {
                    read_paragraph_borders(
                        reader,
                        buffer,
                        ctx,
                        depth + 1,
                        &mut acc.paragraph.borders,
                    )?;
                    consumed = true;
                }
            }
            // `w:numPr` is a container of `w:numId`/`w:ilvl` leaves, so the flat
            // `apply_paragraph_property` cannot read it. A paragraph style that
            // carries list membership (e.g. ListBullet -> numId) must not lose it,
            // so capture the raw numId+ilvl here for the deferred resolve pass
            // (the numbering part is not parsed yet). Mirrors the body parser.
            b"numPr" => {
                if open {
                    read_style_num_pr(reader, buffer, ctx, depth + 1, acc)?;
                    consumed = true;
                }
            }
            // `w:tabs` is a container of `w:tab` leaves, so the flat
            // `apply_paragraph_property` cannot read it. A style that declares its
            // own tab stops (e.g. a TOC/index style's dot-leader stops) must not
            // lose them, so read the nested `w:tab` children here. Mirrors the
            // body parser's `apply_tab_stop`.
            b"tabs" => {
                if open {
                    read_style_tab_stops(reader, buffer, ctx, depth + 1, &mut acc.paragraph)?;
                    consumed = true;
                }
            }
            other => {
                if !apply_paragraph_property(&mut acc.paragraph, other, &child) {
                    ctx.report(other);
                }
            }
        }
        if open && !consumed {
            skip_subtree(reader, buffer, ctx)?;
        }
    }
    Ok(())
}

fn read_run_container(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
    run: &mut RunProperties,
) -> Result<(), ImportError> {
    ctx.check_depth(depth)?;
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        if !apply_run_property(run, child.local_name().as_ref(), &child) {
            ctx.report(child.local_name().as_ref());
        }
        if open {
            skip_subtree(reader, buffer, ctx)?;
        }
    }
    Ok(())
}

fn read_table_container(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
    props: &mut TableProperties,
) -> Result<(), ImportError> {
    ctx.check_depth(depth)?;
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        let mut consumed = false;
        match child.local_name().as_ref() {
            b"tblOverlap" => match attribute_value(&child, b"val").as_deref() {
                Some("never") => props.overlap = Some(TableOverlap::Never),
                Some("overlap") => props.overlap = Some(TableOverlap::Overlap),
                _ => ctx.report(b"tblOverlap"),
            },
            b"jc" => match table_alignment(&child) {
                Some(alignment) => props.alignment = Some(alignment),
                None => ctx.report(b"jc"),
            },
            b"tblW" => match dxa_twips(&child) {
                Some(width) => props.width_twips = Some(width.clamp(0, 31_680)),
                None => ctx.report(b"tblW"),
            },
            b"tblCellSpacing" => match dxa_twips(&child) {
                Some(width) => props.cell_spacing_twips = Some(width.clamp(0, 31_680)),
                None => ctx.report(b"tblCellSpacing"),
            },
            b"tblInd" => match dxa_twips(&child) {
                Some(width) => props.indent_twips = Some(width.clamp(-31_680, 31_680)),
                None => ctx.report(b"tblInd"),
            },
            b"tblLayout" => match attribute_value(&child, b"type").as_deref() {
                Some("fixed") => props.layout = Some(TableLayout::Fixed),
                Some("autofit") => props.layout = Some(TableLayout::Autofit),
                _ => ctx.report(b"tblLayout"),
            },
            b"tblLook" => apply_table_look(&child, &mut props.look),
            b"tblBorders" => {
                if open {
                    read_borders(
                        reader,
                        buffer,
                        ctx,
                        depth + 1,
                        &mut props.borders,
                        b"tblBorders",
                    )?;
                    consumed = true;
                }
            }
            b"shd" => {
                props.shading = Shading {
                    fill: shading_fill(&child, ctx),
                }
            }
            b"tblCellMar" => {
                if open {
                    read_margins(
                        reader,
                        buffer,
                        ctx,
                        depth + 1,
                        &mut props.cell_margins,
                        b"tblCellMar",
                    )?;
                    consumed = true;
                }
            }
            b"tblCaption" => match attribute_value(&child, b"val") {
                Some(value) if !value.is_empty() && value.len() <= 255 => {
                    props.caption = Some(value);
                }
                _ => ctx.report(b"tblCaption"),
            },
            b"tblDescription" => match attribute_value(&child, b"val") {
                Some(value) if !value.is_empty() && value.len() <= 255 => {
                    props.description = Some(value);
                }
                _ => ctx.report(b"tblDescription"),
            },
            other => ctx.report(other),
        }
        if open && !consumed {
            skip_subtree(reader, buffer, ctx)?;
        }
    }
    Ok(())
}

fn read_row_container(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
    props: &mut TableRowProperties,
) -> Result<(), ImportError> {
    ctx.check_depth(depth)?;
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        match child.local_name().as_ref() {
            b"trHeight" => {
                let value = attribute_value(&child, b"val")
                    .and_then(|value| value.parse::<u32>().ok())
                    .map(|value| value.min(31_680));
                let rule = match attribute_value(&child, b"hRule").as_deref() {
                    Some("auto") => Some(HeightRule::Auto),
                    Some("atLeast") => Some(HeightRule::AtLeast),
                    Some("exact") => Some(HeightRule::Exact),
                    _ => None,
                };
                props.height = RowHeight {
                    value_twips: value,
                    rule,
                };
            }
            b"cantSplit" => props.cant_split = is_true(attribute_value(&child, b"val").as_deref()),
            b"tblHeader" => props.header = is_true(attribute_value(&child, b"val").as_deref()),
            b"tblCellSpacing" => match dxa_twips(&child) {
                Some(width) => props.cell_spacing_twips = Some(width.clamp(0, 31_680)),
                None => ctx.report(b"tblCellSpacing"),
            },
            b"jc" => match table_alignment(&child) {
                Some(alignment) => props.alignment = Some(alignment),
                None => ctx.report(b"jc"),
            },
            other => ctx.report(other),
        }
        if open {
            skip_subtree(reader, buffer, ctx)?;
        }
    }
    Ok(())
}

fn read_cell_container(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
    props: &mut TableCellProperties,
) -> Result<(), ImportError> {
    ctx.check_depth(depth)?;
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        let mut consumed = false;
        match child.local_name().as_ref() {
            b"tcW" => match dxa_twips(&child) {
                Some(width) => props.width_twips = Some(width.clamp(0, 31_680)),
                None => ctx.report(b"tcW"),
            },
            b"gridSpan" => match attribute_value(&child, b"val")
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|span| (1..=16_384).contains(span))
            {
                Some(span) => props.grid_span = Some(span),
                None => ctx.report(b"gridSpan"),
            },
            b"vMerge" => {
                props.vertical_merge = match attribute_value(&child, b"val").as_deref() {
                    Some("restart") => Some(VerticalMerge::Restart),
                    _ => Some(VerticalMerge::Continue),
                };
            }
            b"tcBorders" => {
                if open {
                    read_borders(
                        reader,
                        buffer,
                        ctx,
                        depth + 1,
                        &mut props.borders,
                        b"tcBorders",
                    )?;
                    consumed = true;
                }
            }
            b"shd" => {
                props.shading = Shading {
                    fill: shading_fill(&child, ctx),
                }
            }
            b"tcMar" => {
                if open {
                    read_margins(reader, buffer, ctx, depth + 1, &mut props.margins, b"tcMar")?;
                    consumed = true;
                }
            }
            b"vAlign" => match attribute_value(&child, b"val").as_deref() {
                Some("top") => props.vertical_alignment = Some(CellVerticalAlignment::Top),
                Some("center") => props.vertical_alignment = Some(CellVerticalAlignment::Center),
                Some("bottom") => props.vertical_alignment = Some(CellVerticalAlignment::Bottom),
                _ => ctx.report(b"vAlign"),
            },
            b"noWrap" => props.no_wrap = is_true(attribute_value(&child, b"val").as_deref()),
            b"textDirection" => match attribute_value(&child, b"val").as_deref() {
                Some("lrTb") => props.text_direction = Some(TextDirection::LrTb),
                Some("tbRl") => props.text_direction = Some(TextDirection::TbRl),
                Some("btLr") => props.text_direction = Some(TextDirection::BtLr),
                _ => ctx.report(b"textDirection"),
            },
            b"tcFitText" => props.fit_text = is_true(attribute_value(&child, b"val").as_deref()),
            b"hideMark" => props.hide_mark = is_true(attribute_value(&child, b"val").as_deref()),
            other => ctx.report(other),
        }
        if open && !consumed {
            skip_subtree(reader, buffer, ctx)?;
        }
    }
    Ok(())
}

fn read_borders(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
    borders: &mut TableBorders,
    container: &[u8],
) -> Result<(), ImportError> {
    ctx.check_depth(depth)?;
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        let edge = border_edge(&child);
        let slot = match child.local_name().as_ref() {
            b"top" => Some(&mut borders.top),
            b"bottom" => Some(&mut borders.bottom),
            b"start" | b"left" => Some(&mut borders.start),
            b"end" | b"right" => Some(&mut borders.end),
            b"insideH" => Some(&mut borders.inside_h),
            b"insideV" => Some(&mut borders.inside_v),
            _ => None,
        };
        match slot {
            Some(slot) => match edge {
                Some(edge) => *slot = Some(edge),
                None => ctx.report(container),
            },
            None => ctx.report(container),
        }
        if open {
            skip_subtree(reader, buffer, ctx)?;
        }
    }
    Ok(())
}

/// Reads a `w:pBdr` (paragraph border) container into [`ParagraphBorders`]. The
/// six edges (`top`/`bottom`/`start`|`left`/`end`|`right`/`between`/`bar`) each
/// map to one slot; a `nil`/`none` edge is retained verbatim so it can override an
/// inherited border, matching the body parser.
/// Reads a style's `w:pPr/w:numPr`, capturing the raw `w:numId`/`w:ilvl` values
/// onto `acc` for the deferred numbering resolution ([`Styles::resolve_numbering`]).
fn read_style_num_pr(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
    acc: &mut PropAcc,
) -> Result<(), ImportError> {
    ctx.check_depth(depth)?;
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        match child.local_name().as_ref() {
            b"numId" => {
                if let Some(value) = attribute_value(&child, b"val") {
                    acc.pending_num_id = Some(value);
                }
            }
            b"ilvl" => {
                if let Some(level) = attribute_value(&child, b"val").and_then(|v| v.parse().ok()) {
                    acc.pending_ilvl = level;
                }
            }
            _ => {}
        }
        if open {
            skip_subtree(reader, buffer, ctx)?;
        }
    }
    Ok(())
}

/// Reads a style's `w:pPr/w:tabs` container, mapping each `w:tab` child into
/// `paragraph.tabs`. A `clear`/unknown alignment, a missing/out-of-range `w:pos`,
/// or overflow past the 128-stop bound drops that stop and is reported — matching
/// the body parser's `apply_tab_stop`.
fn read_style_tab_stops(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
    paragraph: &mut ParagraphProperties,
) -> Result<(), ImportError> {
    ctx.check_depth(depth)?;
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        if child.local_name().as_ref() == b"tab" {
            match tab_stop(&child) {
                Some(tab) if paragraph.tabs.len() < 128 => paragraph.tabs.push(tab),
                _ => ctx.report(b"tab"),
            }
        } else {
            ctx.report(child.local_name().as_ref());
        }
        if open {
            skip_subtree(reader, buffer, ctx)?;
        }
    }
    Ok(())
}

/// Builds a `TabStop` from a `w:tab` element. Returns `None` (caller reports) for
/// a `clear`/unknown alignment or a missing/out-of-range `w:pos`.
fn tab_stop(element: &BytesStart<'_>) -> Option<TabStop> {
    let alignment = match attribute_value(element, b"val").as_deref() {
        Some("start" | "left") => TabAlignment::Start,
        Some("center") => TabAlignment::Center,
        Some("end" | "right") => TabAlignment::End,
        Some("decimal") => TabAlignment::Decimal,
        Some("bar") => TabAlignment::Bar,
        _ => return None,
    };
    let position_twips = match attribute_value(element, b"pos").and_then(|v| v.parse::<i32>().ok())
    {
        Some(pos) if (-31_680..=31_680).contains(&pos) => pos,
        _ => return None,
    };
    let leader = match attribute_value(element, b"leader").as_deref() {
        Some("dot") => Some(TabLeader::Dot),
        Some("hyphen") => Some(TabLeader::Hyphen),
        Some("underscore") => Some(TabLeader::Underscore),
        Some("middleDot") => Some(TabLeader::MiddleDot),
        Some("heavy") => Some(TabLeader::Heavy),
        _ => None,
    };
    Some(TabStop {
        position_twips,
        alignment,
        leader,
    })
}

fn read_paragraph_borders(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
    borders: &mut ParagraphBorders,
) -> Result<(), ImportError> {
    ctx.check_depth(depth)?;
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        let slot = match child.local_name().as_ref() {
            b"top" => Some(&mut borders.top),
            b"bottom" => Some(&mut borders.bottom),
            b"start" | b"left" => Some(&mut borders.start),
            b"end" | b"right" => Some(&mut borders.end),
            b"between" => Some(&mut borders.between),
            b"bar" => Some(&mut borders.bar),
            _ => None,
        };
        match slot {
            Some(slot) => match border_edge(&child) {
                Some(edge) => *slot = Some(edge),
                None => ctx.report(b"pBdr"),
            },
            None => ctx.report(b"pBdr"),
        }
        if open {
            skip_subtree(reader, buffer, ctx)?;
        }
    }
    Ok(())
}

fn read_margins(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    ctx: &mut Ctx,
    depth: u64,
    margins: &mut CellMargins,
    container: &[u8],
) -> Result<(), ImportError> {
    ctx.check_depth(depth)?;
    loop {
        let (child, open) = match read_node(reader, buffer, ctx)? {
            Node::Open(child) => (child, true),
            Node::Empty(child) => (child, false),
            Node::Close | Node::Eof => break,
        };
        let slot = match child.local_name().as_ref() {
            b"top" => Some(&mut margins.top_twips),
            b"start" | b"left" => Some(&mut margins.start_twips),
            b"bottom" => Some(&mut margins.bottom_twips),
            b"end" | b"right" => Some(&mut margins.end_twips),
            // Margins have no inside edges; ignore rather than report.
            b"insideH" | b"insideV" => {
                if open {
                    skip_subtree(reader, buffer, ctx)?;
                }
                continue;
            }
            _ => None,
        };
        match slot {
            Some(slot) => match dxa_twips(&child) {
                Some(width) => *slot = Some(width.clamp(0, 31_680)),
                None => ctx.report(container),
            },
            None => ctx.report(container),
        }
        if open {
            skip_subtree(reader, buffer, ctx)?;
        }
    }
    Ok(())
}

/// Maps a `w:tblStylePr@w:type` region token to its modeled region.
fn region_from(value: &str) -> Option<TableStyleRegion> {
    Some(match value {
        "wholeTable" => TableStyleRegion::WholeTable,
        "firstRow" => TableStyleRegion::FirstRow,
        "lastRow" => TableStyleRegion::LastRow,
        "firstCol" => TableStyleRegion::FirstColumn,
        "lastCol" => TableStyleRegion::LastColumn,
        "band1Horz" => TableStyleRegion::Band1Horizontal,
        "band2Horz" => TableStyleRegion::Band2Horizontal,
        "band1Vert" => TableStyleRegion::Band1Vertical,
        "band2Vert" => TableStyleRegion::Band2Vertical,
        "neCell" => TableStyleRegion::NorthEastCell,
        "nwCell" => TableStyleRegion::NorthWestCell,
        "seCell" => TableStyleRegion::SouthEastCell,
        "swCell" => TableStyleRegion::SouthWestCell,
        _ => return None,
    })
}

/// Maps a table `w:jc@val` to an `Alignment`; only the horizontal placements are
/// modeled (justify is reported by the caller).
fn table_alignment(element: &BytesStart<'_>) -> Option<Alignment> {
    match attribute_value(element, b"val").as_deref() {
        Some("start" | "left") => Some(Alignment::Start),
        Some("center") => Some(Alignment::Center),
        Some("end" | "right") => Some(Alignment::End),
        _ => None,
    }
}

/// A `dxa`-typed width/indent attribute (`@w:w`); a non-`dxa` type yields `None`.
fn dxa_twips(element: &BytesStart<'_>) -> Option<i32> {
    let is_dxa = attribute_value(element, b"type")
        .map(|kind| kind == "dxa")
        .unwrap_or(true);
    let width = attribute_value(element, b"w").and_then(|value| value.parse::<i32>().ok());
    match width {
        Some(width) if is_dxa => Some(width),
        _ => None,
    }
}

/// Builds a `BorderEdge` from an edge element; `None` when the `w:val` style is
/// missing/empty/oversized (the caller reports the container).
fn border_edge(element: &BytesStart<'_>) -> Option<BorderEdge> {
    let style =
        attribute_value(element, b"val").filter(|value| !value.is_empty() && value.len() <= 32)?;
    let size_eighth_points = attribute_value(element, b"sz")
        .and_then(|value| value.parse::<u32>().ok())
        .map(|size| size.min(1024));
    let color = attribute_value(element, b"color")
        .filter(|value| value != "auto")
        .and_then(|value| parse_rgb(&value));
    let space_points = attribute_value(element, b"space")
        .and_then(|value| value.parse::<u32>().ok())
        .map(|space| space.min(31));
    Some(BorderEdge {
        style,
        size_eighth_points,
        color,
        space_points,
    })
}

/// Parses a `w:shd`'s background fill (explicit sRGB `@w:fill`); a real pattern,
/// a non-`auto` pattern color, or a theme fill/color is reported (degraded).
fn shading_fill(element: &BytesStart<'_>, ctx: &mut Ctx) -> Option<RgbColor> {
    let pattern_modeled = matches!(
        attribute_value(element, b"val").as_deref(),
        None | Some("clear") | Some("nil")
    );
    let pattern_color_default = matches!(
        attribute_value(element, b"color").as_deref(),
        None | Some("auto")
    );
    let has_theme = attribute_value(element, b"themeFill").is_some()
        || attribute_value(element, b"themeColor").is_some();
    if !pattern_modeled || !pattern_color_default || has_theme {
        ctx.report(b"shd");
    }
    attribute_value(element, b"fill")
        .filter(|value| value != "auto")
        .and_then(|value| parse_rgb(&value))
}

/// Applies a `w:tblLook`: explicit boolean attributes when present, else the
/// legacy hex `@w:val` bitmask.
fn apply_table_look(element: &BytesStart<'_>, look: &mut TableLook) {
    let mut any = false;
    for (name, field) in [
        (b"firstRow".as_slice(), &mut look.first_row),
        (b"lastRow".as_slice(), &mut look.last_row),
        (b"firstColumn".as_slice(), &mut look.first_column),
        (b"lastColumn".as_slice(), &mut look.last_column),
        (b"noHBand".as_slice(), &mut look.no_h_band),
        (b"noVBand".as_slice(), &mut look.no_v_band),
    ] {
        if let Some(value) = attribute_value(element, name) {
            *field = is_true(Some(&value));
            any = true;
        }
    }
    if any {
        return;
    }
    if let Some(mask) = attribute_value(element, b"val")
        .and_then(|value| u32::from_str_radix(value.trim_start_matches("0x"), 16).ok())
    {
        look.first_row = mask & 0x0020 != 0;
        look.last_row = mask & 0x0040 != 0;
        look.first_column = mask & 0x0080 != 0;
        look.last_column = mask & 0x0100 != 0;
        look.no_h_band = mask & 0x0200 != 0;
        look.no_v_band = mask & 0x0400 != 0;
    }
}
