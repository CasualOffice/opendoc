//! Numbering-part parsing: OOXML abstractNum/num string ids -> deterministic v1
//! ids, and w:numPr resolution. Mirrors the styles pattern.
//!
//! Two rules this module does **not** own:
//!
//! - **Which levels a `w:numPr` may name.** That is
//!   `casual_doc_model::v1::NumberingResolver`, the one authority the model's
//!   validator and the layout engine also resolve through
//!   ([`Numbering::resolve`], `docs/142` LST-10/LST-34).
//! - **Whether an unmapped element is a loss.** That is
//!   `crate::noop::carries_no_meaning`, routed inside `Reporter::report`.
//!
//! What it does own, besides the mapping, is naming the *attribute*-level losses
//! the element catch-all cannot see ([`report_unmodeled_attributes`], LST-31).

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{
    AbstractNumbering, AbstractNumberingId, DefinitionMap, LevelJustification, LevelSuffix,
    MultiLevelType, NumberFormat, NumberingInstance, NumberingInstanceId, NumberingLevel,
    NumberingOverride, NumberingRef, ParagraphProperties, RunProperties, StyleKind,
};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::{apply_paragraph_property, apply_run_property, attribute_value};
// Separate `use` lines to minimize import-block merge conflicts.
use crate::properties::{MAX_TAB_STOPS, tab_stop_from};
use crate::report::Reporter;
use crate::styles::Styles;
// Separate `use` line (rustfmt Preserve) to minimize import-block merge
// conflicts: the ONE numbering resolver, which this module defers to instead of
// carrying a validity rule of its own.
use casual_doc_model::v1::NumberingResolver;

/// Resolved numbering definitions plus the numId -> instance index.
#[derive(Debug, Default)]
pub(crate) struct Numbering {
    by_num_id: BTreeMap<String, NumberingInstanceId>,
    abstract_numbering: DefinitionMap<AbstractNumberingId, AbstractNumbering>,
    instances: DefinitionMap<NumberingInstanceId, NumberingInstance>,
}

impl Numbering {
    /// Resolves a `w:numPr` (numId + ilvl) to a paragraph numbering reference,
    /// requiring the instance to exist and the level to **resolve**.
    ///
    /// "Resolve" is not this module's rule. It is
    /// [`NumberingResolver::level`](casual_doc_model::v1::NumberingResolver::level)
    /// in `casual-doc-model` — the same predicate `Document::validate` accepts a
    /// reference by and `casual-doc-layout` paints from. This function used to
    /// carry a third, stricter rule of its own (a per-instance set of the `w:lvl`
    /// children the named abstract declared), which is why a Word **List Style** —
    /// an abstract carrying `<w:numStyleLink/>` and no `w:lvl` at all, deferring
    /// its levels to a numbering paragraph style — lost the marker on *every*
    /// paragraph of the list, silently, while the renderer had followed that link
    /// correctly for months (`docs/142` LST-10; the import-side half of LST-34 is
    /// that admitting the reference without moving the model's validator would
    /// have turned the dropped marker into a rejected document).
    ///
    /// `styles` is threaded in because the `w:numStyleLink` indirection routes
    /// *through* a paragraph style's own `w:pPr/w:numPr`; the caller therefore has
    /// to have run [`Styles::resolve_numbering`](crate::styles::Styles::resolve_numbering)
    /// first for a link to be followable.
    ///
    /// Complexity: one B-tree lookup for the `numId`, then O(1) link hops and a
    /// scan of the effective abstract's levels (at most nine in OOXML) — see
    /// [`NumberingResolver::level`](casual_doc_model::v1::NumberingResolver::level).
    /// Called once per `w:numPr`, so import stays linear in paragraphs and is
    /// quadratic in neither abstracts nor levels.
    pub(crate) fn resolve(&self, styles: &Styles, num_id: &str, level: u8) -> Option<NumberingRef> {
        let instance = *self.by_num_id.get(num_id)?;
        let reference = NumberingRef { instance, level };
        self.resolver(styles)
            .level(reference)
            .is_some()
            .then_some(reference)
    }

    /// The model's numbering resolver over the definitions parsed so far.
    ///
    /// Complexity: O(1) — it borrows three maps.
    fn resolver<'a>(&'a self, styles: &'a Styles) -> NumberingResolver<'a> {
        NumberingResolver::new(
            styles.definitions(),
            &self.instances,
            &self.abstract_numbering,
        )
    }

    /// Whether `num_id` names an instance whose abstract defers its levels through
    /// a `w:numStyleLink` that leads nowhere: the link is present, and following it
    /// still reaches an abstract declaring no levels at all.
    ///
    /// This is the difference between "this document has no such list" and "this
    /// document has a List-Style list we could not follow", and the caller reports
    /// the second as its own finding rather than defaulting the list to unnumbered
    /// (a dangling link and a `numStyleLink`/`styleLink` cycle both land here).
    ///
    /// Complexity: as [`Numbering::resolve`].
    pub(crate) fn has_unfollowable_style_link(&self, styles: &Styles, num_id: &str) -> bool {
        let Some(instance_id) = self.by_num_id.get(num_id) else {
            return false;
        };
        let resolver = self.resolver(styles);
        let Some(instance) = resolver.instance(*instance_id) else {
            return false;
        };
        let Some(declared) = self.abstract_numbering.get(&instance.abstract_ref) else {
            return false;
        };
        declared.num_style_link.is_some() && resolver.effective_abstract(declared).levels.is_empty()
    }

    pub(crate) fn into_definitions(
        self,
    ) -> (
        DefinitionMap<AbstractNumberingId, AbstractNumbering>,
        DefinitionMap<NumberingInstanceId, NumberingInstance>,
    ) {
        (self.abstract_numbering, self.instances)
    }
}

#[derive(Default)]
struct RawLevel {
    level: u8,
    start: u16,
    num_fmt: Option<NumberFormat>,
    lvl_text: Option<String>,
    lvl_jc: Option<LevelJustification>,
    suff: Option<LevelSuffix>,
    is_lgl: bool,
    lvl_restart: Option<u8>,
    /// Raw `w:lvl/w:pStyle@val` (a style id token); resolved to a `StyleId`
    /// against the parsed styles in the assembly pass.
    pstyle: Option<String>,
    paragraph: ParagraphProperties,
    has_paragraph: bool,
    run: RunProperties,
    has_run: bool,
}

#[derive(Default)]
struct RawAbstract {
    id: String,
    levels: Vec<RawLevel>,
    multi_level_type: Option<MultiLevelType>,
    /// Raw `w:numStyleLink@val` (a style id token); resolved in the assembly pass.
    num_style_link: Option<String>,
    /// Raw `w:styleLink@val` (a style id token); resolved in the assembly pass.
    style_link: Option<String>,
}

struct RawNum {
    num_id: String,
    abstract_id: Option<String>,
    /// Per-instance `w:lvlOverride` captures, one per opened override element.
    overrides: Vec<RawOverride>,
}

/// A raw `w:lvlOverride`: its target level plus the optional `w:startOverride`
/// value and full `w:lvl` redefinition it carries.
#[derive(Default)]
struct RawOverride {
    ilvl: u8,
    start: Option<u16>,
    level: Option<RawLevel>,
}

/// Parses the numbering part, allocating ids from `ids`. `styles` (parsed first)
/// resolves the numbering <-> style links (`w:pStyle`, `w:numStyleLink`,
/// `w:styleLink`), which reference paragraph styles by their id token.
pub(crate) fn parse(
    xml: &[u8],
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
    config: ImportConfig,
    styles: &Styles,
) -> Result<Numbering, ImportError> {
    let (abstracts, nums) = parse_raw(xml, reporter, config)?;

    // Assign ids to abstract definitions; build the abstractNumId -> id map and
    // the definition table.
    let mut abstract_by_key: BTreeMap<String, AbstractNumberingId> = BTreeMap::new();
    let mut abstract_numbering = DefinitionMap::default();
    for raw in abstracts {
        if abstract_by_key.contains_key(&raw.id) {
            reporter.report(b"abstractNum");
            continue;
        }
        let id = AbstractNumberingId::new(next_id(ids)?);
        let mut levels = Vec::with_capacity(raw.levels.len());
        // `seen` exists only to drop a repeated `w:lvl@w:ilvl` (the first wins);
        // it is NOT a table of which levels are valid. Which levels a `w:numPr`
        // may name is `NumberingResolver::level`'s answer, and building a second
        // one here is the defect this file used to carry.
        let mut seen = BTreeSet::new();
        for level in raw.levels {
            if seen.insert(level.level) {
                levels.push(build_level(level, styles, reporter));
            }
        }
        abstract_by_key.insert(raw.id.clone(), id);
        abstract_numbering.insert(
            id,
            AbstractNumbering {
                levels,
                multi_level_type: raw.multi_level_type,
                num_style_link: resolve_style_link(
                    styles,
                    raw.num_style_link.as_deref(),
                    reporter,
                    b"numStyleLink",
                ),
                style_link: resolve_style_link(
                    styles,
                    raw.style_link.as_deref(),
                    reporter,
                    b"styleLink",
                ),
            },
        );
    }

    // Assign ids to instances; resolve their abstract reference.
    let mut by_num_id = BTreeMap::new();
    let mut instances = DefinitionMap::default();
    for raw in nums {
        if by_num_id.contains_key(&raw.num_id) {
            reporter.report(b"num");
            continue;
        }
        let Some(abstract_ref) = raw
            .abstract_id
            .as_deref()
            .and_then(|key| abstract_by_key.get(key))
        else {
            reporter.report(b"num");
            continue;
        };
        let id = NumberingInstanceId::new(next_id(ids)?);
        by_num_id.insert(raw.num_id, id);
        // Keep only the last override per level (a later `w:lvlOverride` for the
        // same ilvl wins, per field), preserving level order for deterministic
        // output.
        let mut overrides: Vec<NumberingOverride> = Vec::new();
        for raw_override in raw.overrides {
            let definition = raw_override
                .level
                .map(|level| build_level(level, styles, reporter));
            match overrides.iter_mut().find(|o| o.level == raw_override.ilvl) {
                Some(existing) => {
                    if let Some(start) = raw_override.start {
                        existing.start = Some(start);
                    }
                    if definition.is_some() {
                        existing.definition = definition;
                    }
                }
                None => overrides.push(NumberingOverride {
                    level: raw_override.ilvl,
                    start: raw_override.start,
                    definition,
                }),
            }
        }
        overrides.sort_by_key(|o| o.level);
        // A `w:lvlOverride` with neither a start nor a full level carries no
        // information; drop it so an empty override does not appear.
        overrides.retain(|o| o.start.is_some() || o.definition.is_some());
        instances.insert(
            id,
            NumberingInstance {
                abstract_ref: *abstract_ref,
                overrides,
            },
        );
    }

    Ok(Numbering {
        by_num_id,
        abstract_numbering,
        instances,
    })
}

fn next_id(ids: &mut IdGenerator) -> Result<casual_doc_model::NodeId, ImportError> {
    ids.next_id()
        .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })
}

/// Resolves a captured `w:pStyle`/`w:numStyleLink`/`w:styleLink` id token to a
/// paragraph `StyleId`. An absent link is `None`; a token that names no
/// paragraph style is dropped and reported (no silent loss), mirroring how the
/// styles parser handles dangling references.
fn resolve_style_link(
    styles: &Styles,
    name: Option<&str>,
    reporter: &mut Reporter,
    label: &[u8],
) -> Option<casual_doc_model::v1::StyleId> {
    let name = name?;
    match styles.resolve(name, StyleKind::Paragraph) {
        Some(id) => Some(id),
        None => {
            reporter.report(label);
            None
        }
    }
}

/// Converts a parsed raw level into the typed model level, resolving its
/// `w:pStyle` binding and clamping its start value. Shared by abstract levels
/// and per-instance `w:lvlOverride/w:lvl` redefinitions.
fn build_level(level: RawLevel, styles: &Styles, reporter: &mut Reporter) -> NumberingLevel {
    NumberingLevel {
        level: level.level,
        start: level.start.min(32_767),
        num_fmt: level.num_fmt,
        lvl_text: level.lvl_text,
        lvl_jc: level.lvl_jc,
        suff: level.suff,
        is_lgl: level.is_lgl,
        paragraph_properties: level.has_paragraph.then_some(level.paragraph),
        run_properties: level.has_run.then_some(level.run),
        style_ref: None,
        lvl_restart: level.lvl_restart,
        pstyle: resolve_style_link(styles, level.pstyle.as_deref(), reporter, b"pStyle"),
    }
}

#[derive(Default)]
struct NumberingState {
    current_abstract: Option<RawAbstract>,
    current_level: Option<RawLevel>,
    current_num: Option<RawNum>,
    /// The `w:ilvl` of the `w:lvlOverride` currently open inside a `w:num`, so a
    /// nested `w:startOverride` knows which level it restarts.
    current_override_ilvl: Option<u8>,
    /// Depth inside the current level's `w:pPr` / `w:rPr` (so their children route
    /// to the shared paragraph/run property parsers, mirroring the styles parser).
    ppr_depth: u32,
    rpr_depth: u32,
    /// Depth inside the current level's `w:pPr/w:tabs` (0 when outside one).
    /// `w:tabs` is a container of `w:tab` leaves, so the flat
    /// `apply_paragraph_property` cannot read it — mirroring the styles parser,
    /// the children are routed to the shared tab-stop mapper from here instead.
    /// Without this a numbering level's list tab was dropped entirely, and with
    /// it the level's `w:tab w:val="num"` reaches the flow engine's marker-tab
    /// union (`casual-doc-layout/src/flow.rs`, `prepare_list_marker`).
    tabs_depth: u32,
    /// Nesting level inside a `w:numPicBullet` (0 when outside one). Picture
    /// bullets are not modeled and the numbering part is regenerated, so the
    /// whole subtree is one reported loss rather than one finding per child.
    pic_bullet_depth: u32,
}

fn parse_raw(
    xml: &[u8],
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<(Vec<RawAbstract>, Vec<RawNum>), ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut abstracts = Vec::new();
    let mut nums = Vec::new();
    let mut state = NumberingState::default();
    let mut elements = 0_u64;
    let mut depth = 0_u64;

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| ImportError::MalformedXml)?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(ImportError::MalformedXml),
            Event::Start(element) => {
                depth += 1;
                if depth > config.max_depth {
                    return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                }
                bump(&mut elements, config.max_elements)?;
                on_start(
                    &mut state,
                    reporter,
                    element.local_name().into_inner().as_bytes(),
                    &element,
                );
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                let local = element.local_name();
                on_start(&mut state, reporter, local.into_inner().as_bytes(), &element);
                on_end(&mut state, local.into_inner().as_bytes(), &mut abstracts, &mut nums);
            }
            Event::End(element) => {
                on_end(
                    &mut state,
                    element.local_name().into_inner().as_bytes(),
                    &mut abstracts,
                    &mut nums,
                );
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok((abstracts, nums))
}

fn bump(elements: &mut u64, max: u64) -> Result<(), ImportError> {
    *elements += 1;
    if *elements > max {
        return Err(ImportError::LimitExceeded {
            limit: "xml_elements",
        });
    }
    Ok(())
}

fn on_start(
    state: &mut NumberingState,
    reporter: &mut Reporter,
    local: &[u8],
    element: &BytesStart<'_>,
) {
    // Inside a `w:numPicBullet`, the whole subtree belongs to the one loss
    // already reported on the container.
    if state.pic_bullet_depth > 0 {
        state.pic_bullet_depth += 1;
        return;
    }
    // Inside the current level's rPr/pPr, delegate to the shared property parsers;
    // an unmapped property child is reported (no silent loss).
    if state.rpr_depth > 0 {
        if let Some(level) = state.current_level.as_mut()
            && !apply_run_property(&mut level.run, local, element)
        {
            reporter.report(local);
        }
        return;
    }
    if state.ppr_depth > 0 {
        // `w:tabs` is a container, not a leaf property: open it and route its
        // `w:tab` children through the shared mapper. A list level's tab stop
        // (`<w:tabs><w:tab w:val="num" w:pos="709"/></w:tabs>`, which LibreOffice
        // and Word both emit) is where the marker's suffix tab lands the body
        // text, so dropping it put every list's text at the default grid instead.
        if local == b"tabs" {
            state.tabs_depth += 1;
            return;
        }
        if state.tabs_depth > 0 {
            if local != b"tab" {
                reporter.report(local);
                return;
            }
            match (state.current_level.as_mut(), tab_stop_from(element)) {
                // `has_paragraph` is already set by the enclosing `w:pPr`.
                (Some(level), Some(stop)) if level.paragraph.tabs.len() < MAX_TAB_STOPS => {
                    level.paragraph.tabs.push(stop);
                }
                _ => reporter.report(b"tab"),
            }
            return;
        }
        if let Some(level) = state.current_level.as_mut()
            && !apply_paragraph_property(&mut level.paragraph, local, element)
        {
            reporter.report(local);
        }
        return;
    }
    match local {
        b"numbering" => {}
        // A picture bullet (`w:numPicBullet`, a VML/DrawingML image used as the
        // list marker). Neither the image nor the level's `w:lvlPicBulletId`
        // reference is modeled, and the part is regenerated on save, so the
        // bullet is dropped: report it once and skip its subtree.
        b"numPicBullet" => {
            reporter.report(local);
            state.pic_bullet_depth = 1;
        }
        b"abstractNum" => {
            state.current_abstract = Some(RawAbstract {
                id: attribute_value(element, b"abstractNumId").unwrap_or_default(),
                ..RawAbstract::default()
            });
        }
        // A `w:lvl` opens either an abstract level or, inside a `w:num`, a full
        // `w:lvlOverride/w:lvl` redefinition; both feed the same `current_level`
        // machinery. The override's `w:lvl@ilvl` defaults to the override target.
        b"lvl" if state.current_abstract.is_some() || state.current_override_ilvl.is_some() => {
            report_unmodeled_attributes(reporter, local, element);
            state.current_level = Some(RawLevel {
                level: attribute_value(element, b"ilvl")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(state.current_override_ilvl.unwrap_or(0)),
                start: 1,
                ..RawLevel::default()
            });
        }
        b"start" if state.current_level.is_some() => {
            if let Some(level) = state.current_level.as_mut() {
                level.start = attribute_value(element, b"val")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1);
            }
        }
        // Level detail: number format/text/justify/suffix and the legal flag.
        b"numFmt" if state.current_level.is_some() => {
            report_unmodeled_attributes(reporter, local, element);
            match number_format(element) {
                Some(format) => set_level(state, |level| level.num_fmt = Some(format)),
                None => reporter.report(local),
            }
        }
        b"lvlText" if state.current_level.is_some() => {
            match attribute_value(element, b"val").filter(|value| value.len() <= 255) {
                Some(text) => set_level(state, |level| level.lvl_text = Some(text)),
                None => reporter.report(local),
            }
        }
        b"lvlJc" if state.current_level.is_some() => match level_justification(element) {
            Some(justification) => set_level(state, |level| level.lvl_jc = Some(justification)),
            None => reporter.report(local),
        },
        b"suff" if state.current_level.is_some() => match level_suffix(element) {
            Some(suffix) => set_level(state, |level| level.suff = Some(suffix)),
            None => reporter.report(local),
        },
        b"isLgl" if state.current_level.is_some() => {
            let on = on_off(element);
            set_level(state, |level| level.is_lgl = on);
        }
        // `w:lvlRestart@val`: the higher level whose advance restarts this one.
        b"lvlRestart" if state.current_level.is_some() => {
            match attribute_value(element, b"val").and_then(|v| v.parse::<u8>().ok()) {
                Some(restart) => set_level(state, |level| level.lvl_restart = Some(restart)),
                None => reporter.report(local),
            }
        }
        // `w:lvl/w:pStyle@val`: the paragraph style this level binds to (captured
        // raw; resolved once styles are threaded in). Only at level scope — a
        // `w:pStyle` inside the level's `w:pPr` is intercepted above by ppr_depth.
        b"pStyle" if state.current_level.is_some() => {
            match attribute_value(element, b"val").filter(|value| !value.is_empty()) {
                Some(name) => set_level(state, |level| level.pstyle = Some(name)),
                None => reporter.report(local),
            }
        }
        // `w:numStyleLink@val` / `w:styleLink@val` on the abstract definition: the
        // numbering <-> List-Style bindings (captured raw; resolved later). Only
        // at abstract scope, before any level opens.
        b"numStyleLink" if state.current_abstract.is_some() && state.current_level.is_none() => {
            match attribute_value(element, b"val").filter(|value| !value.is_empty()) {
                Some(name) => {
                    if let Some(abstract_num) = state.current_abstract.as_mut() {
                        abstract_num.num_style_link = Some(name);
                    }
                }
                None => reporter.report(local),
            }
        }
        b"styleLink" if state.current_abstract.is_some() && state.current_level.is_none() => {
            match attribute_value(element, b"val").filter(|value| !value.is_empty()) {
                Some(name) => {
                    if let Some(abstract_num) = state.current_abstract.as_mut() {
                        abstract_num.style_link = Some(name);
                    }
                }
                None => reporter.report(local),
            }
        }
        // `w:multiLevelType@val` on the abstract definition.
        b"multiLevelType" if state.current_abstract.is_some() => {
            match attribute_value(element, b"val")
                .as_deref()
                .and_then(multi_level_type_from)
            {
                Some(kind) => {
                    if let Some(abstract_num) = state.current_abstract.as_mut() {
                        abstract_num.multi_level_type = Some(kind);
                    }
                }
                None => reporter.report(local),
            }
        }
        b"pPr" if state.current_level.is_some() => {
            state.ppr_depth += 1;
            set_level(state, |level| level.has_paragraph = true);
        }
        b"rPr" if state.current_level.is_some() => {
            state.rpr_depth += 1;
            set_level(state, |level| level.has_run = true);
        }
        b"num" => {
            state.current_num = Some(RawNum {
                num_id: attribute_value(element, b"numId").unwrap_or_default(),
                abstract_id: None,
                overrides: Vec::new(),
            });
        }
        b"abstractNumId" if state.current_num.is_some() => {
            if let Some(num) = state.current_num.as_mut() {
                num.abstract_id = attribute_value(element, b"val");
            }
        }
        b"lvlOverride" if state.current_num.is_some() => {
            // Open one override entry per `w:lvlOverride`; its nested
            // `w:startOverride` and/or `w:lvl` fill it in. `w:ilvl` defaults to 0.
            let ilvl = attribute_value(element, b"ilvl")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            state.current_override_ilvl = Some(ilvl);
            if let Some(num) = state.current_num.as_mut() {
                num.overrides.push(RawOverride {
                    ilvl,
                    ..RawOverride::default()
                });
            }
        }
        b"startOverride" if state.current_num.is_some() => {
            // A per-instance restart (`<w:lvlOverride><w:startOverride w:val="N"/>`):
            // the ubiquitous "this list restarts at N" case. A `w:startOverride`
            // outside a `w:lvlOverride` defaults its level to 0, matching Word.
            if let (Some(num), Some(start)) = (
                state.current_num.as_mut(),
                attribute_value(element, b"val").and_then(|value| value.parse::<u16>().ok()),
            ) {
                let start = start.min(32_767);
                match num.overrides.last_mut() {
                    Some(over) if state.current_override_ilvl.is_some() => over.start = Some(start),
                    _ => num.overrides.push(RawOverride {
                        ilvl: state.current_override_ilvl.unwrap_or(0),
                        start: Some(start),
                        level: None,
                    }),
                }
            }
        }
        // Any still-unmapped numbering detail is reported (no silent loss).
        _ if state.current_abstract.is_some() || state.current_num.is_some() => {
            reporter.report(local);
        }
        _ => {}
    }
}

/// Reports the **attributes** of an otherwise-modeled numbering element whose
/// meaning the model does not carry (`docs/142` LST-31).
///
/// Until this existed, `numbering.rs` never called
/// [`Reporter::report_attribute`](crate::report::Reporter::report_attribute) at
/// all: the catch-all at the end of `on_start` sees **elements only**, so an
/// attribute on an element the parser *does* handle fell through it and was
/// dropped in silence. `word/numbering.xml` is in the consumed set and is
/// regenerated from the model on a semantic save, so there is no byte retention
/// behind these either — they are gone, and the no-silent-loss rule (`SKILL.md`
/// §12, and competitive advantage #2 in §1) says the report has to say so.
///
/// What is reported, and why each is a real loss:
///
/// - **`w:lvl@w:tplc`** — the level's list-template code. Word writes one on
///   nearly every authored level; 18 of them in one document of the owner's
///   sample set, 54 in another. It keys the level back to the entry in the user's
///   List Library, so a save drops the gallery association.
/// - **`w:lvl@w:tentative`** — this level was created as a placeholder and Word
///   may discard it if it is never used. Dropping it makes a tentative level
///   permanent on reopen, which is a behaviour difference, not bookkeeping.
/// - **`w:numFmt@w:format`** — the custom number-format picture used when
///   `w:val="custom"`. The typed model carries only the token, so the picture
///   that decides what the marker actually reads is lost.
///
/// What is deliberately **not** reported, so this does not become a source of
/// false losses (HF-174 put 621 of those in front of the owner):
///
/// - `w:lvl@w:ilvl`, `w:num@w:numId`, `w:abstractNum@w:abstractNumId` and
///   `w:lvlOverride@w:ilvl` — every one is consumed and modeled.
/// - `w:nsid` / `w:tmpl` — dropped **by policy** as List-Library gallery keys
///   (`crate::noop::carries_no_meaning`, and `docs/35`). They are elements, so
///   they never reached this function; the distinction is recorded here because
///   `w:tplc` is the same *kind* of identifier and is nonetheless reported: a
///   `tplc` sits on a level the user can still see and edit, and `35`'s policy
///   row covers only the two abstract-level GUIDs.
/// - Anything on an element this parser does not model at all: the element
///   itself is already one finding, and naming its attributes too would report
///   one loss twice.
///
/// Complexity: O(A) in the attributes of the one element being opened (three
/// name comparisons each), so O(1) per element and linear in the part overall.
fn report_unmodeled_attributes(reporter: &mut Reporter, local: &[u8], element: &BytesStart<'_>) {
    const UNMODELED: &[(&[u8], &[u8])] = &[
        (b"lvl", b"tplc"),
        (b"lvl", b"tentative"),
        (b"numFmt", b"format"),
    ];
    for (element_name, attribute) in UNMODELED {
        if *element_name == local && attribute_value(element, attribute).is_some() {
            reporter.report_attribute(local, attribute);
        }
    }
}

/// Applies `apply` to the current level if one is open (a no-op otherwise).
fn set_level(state: &mut NumberingState, apply: impl FnOnce(&mut RawLevel)) {
    if let Some(level) = state.current_level.as_mut() {
        apply(level);
    }
}

fn on_end(
    state: &mut NumberingState,
    local: &[u8],
    abstracts: &mut Vec<RawAbstract>,
    nums: &mut Vec<RawNum>,
) {
    // Closing out of the reported `w:numPicBullet` subtree (its own close
    // included); nothing inside it feeds the model.
    if state.pic_bullet_depth > 0 {
        state.pic_bullet_depth -= 1;
        return;
    }
    match local {
        b"tabs" if state.tabs_depth > 0 => state.tabs_depth -= 1,
        b"pPr" => state.ppr_depth = state.ppr_depth.saturating_sub(1),
        b"rPr" => state.rpr_depth = state.rpr_depth.saturating_sub(1),
        b"lvl" => {
            if let Some(level) = state.current_level.take() {
                if let Some(abstract_num) = state.current_abstract.as_mut() {
                    abstract_num.levels.push(level);
                } else if let Some(num) = state.current_num.as_mut() {
                    // A `w:lvlOverride/w:lvl` redefinition: attach it to the
                    // override entry opened by the enclosing `w:lvlOverride`.
                    if let Some(over) = num.overrides.last_mut() {
                        over.level = Some(level);
                    }
                }
            }
        }
        b"abstractNum" => {
            if let Some(abstract_num) = state.current_abstract.take() {
                abstracts.push(abstract_num);
            }
        }
        b"lvlOverride" => state.current_override_ilvl = None,
        b"num" => {
            if let Some(num) = state.current_num.take() {
                nums.push(num);
            }
        }
        _ => {}
    }
}

/// Reads an OOXML `CT_OnOff`: present means `true` unless `w:val` is falsey.
fn on_off(element: &BytesStart<'_>) -> bool {
    match attribute_value(element, b"val") {
        Some(value) => !matches!(value.as_str(), "false" | "0" | "off"),
        None => true,
    }
}

/// Maps `w:numFmt/@w:val` (`ST_NumberFormat`); an unknown-but-present token is
/// retained via `Other`, so nothing is lost. Absent/empty is unmapped.
/// Maps `w:multiLevelType@val` to the modeled list shape; an unknown token is
/// reported by the caller (returns `None`).
fn multi_level_type_from(value: &str) -> Option<MultiLevelType> {
    Some(match value {
        "singleLevel" => MultiLevelType::SingleLevel,
        "multilevel" => MultiLevelType::Multilevel,
        "hybridMultilevel" => MultiLevelType::HybridMultilevel,
        _ => return None,
    })
}

pub(crate) fn number_format(element: &BytesStart<'_>) -> Option<NumberFormat> {
    number_format_from_str(attribute_value(element, b"val")?)
}

/// Maps an `ST_NumberFormat` token to a [`NumberFormat`]. Used for `w:numFmt/@w:val`
/// (via [`number_format`]) and `w:pgNumType/@w:fmt`, which share the vocabulary. An
/// empty token, or an unknown one over the 64-byte retention bound, yields `None`.
pub(crate) fn number_format_from_str(value: String) -> Option<NumberFormat> {
    if value.is_empty() {
        return None;
    }
    Some(match value.as_str() {
        "decimal" => NumberFormat::Decimal,
        "bullet" => NumberFormat::Bullet,
        "lowerRoman" => NumberFormat::LowerRoman,
        "upperRoman" => NumberFormat::UpperRoman,
        "lowerLetter" => NumberFormat::LowerLetter,
        "upperLetter" => NumberFormat::UpperLetter,
        "ordinal" => NumberFormat::Ordinal,
        "cardinalText" => NumberFormat::CardinalText,
        "ordinalText" => NumberFormat::OrdinalText,
        "decimalZero" => NumberFormat::DecimalZero,
        "none" => NumberFormat::None,
        _ if value.len() <= 64 => NumberFormat::Other(value),
        // An oversized unknown token is out of the retention bound; report it.
        _ => return None,
    })
}

/// Maps `w:lvlJc/@w:val`; `left`/`start` and `right`/`end` are synonyms.
fn level_justification(element: &BytesStart<'_>) -> Option<LevelJustification> {
    match attribute_value(element, b"val").as_deref() {
        Some("left" | "start") => Some(LevelJustification::Start),
        Some("center") => Some(LevelJustification::Center),
        Some("right" | "end") => Some(LevelJustification::End),
        _ => None,
    }
}

/// Maps `w:suff/@w:val`.
fn level_suffix(element: &BytesStart<'_>) -> Option<LevelSuffix> {
    match attribute_value(element, b"val").as_deref() {
        Some("tab") => Some(LevelSuffix::Tab),
        Some("space") => Some(LevelSuffix::Space),
        Some("nothing") => Some(LevelSuffix::Nothing),
        _ => None,
    }
}
