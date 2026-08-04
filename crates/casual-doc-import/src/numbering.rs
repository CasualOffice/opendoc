//! Numbering-part parsing: OOXML abstractNum/num string ids -> deterministic v1
//! ids, and w:numPr resolution. Mirrors the styles pattern.

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
use crate::report::Reporter;
use crate::styles::Styles;

/// Resolved numbering definitions plus the numId -> instance index.
#[derive(Debug, Default)]
pub(crate) struct Numbering {
    by_num_id: BTreeMap<String, NumberingInstanceId>,
    valid_levels: BTreeMap<NumberingInstanceId, BTreeSet<u8>>,
    abstract_numbering: DefinitionMap<AbstractNumberingId, AbstractNumbering>,
    instances: DefinitionMap<NumberingInstanceId, NumberingInstance>,
}

impl Numbering {
    /// Resolves a `w:numPr` (numId + ilvl) to a paragraph numbering reference,
    /// requiring the instance to exist and the level to be defined.
    pub(crate) fn resolve(&self, num_id: &str, level: u8) -> Option<NumberingRef> {
        let instance = *self.by_num_id.get(num_id)?;
        if self.valid_levels.get(&instance)?.contains(&level) {
            Some(NumberingRef { instance, level })
        } else {
            None
        }
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
    /// Per-instance `w:lvlOverride/w:startOverride` captures: `(ilvl, start)`.
    overrides: Vec<(u8, u16)>,
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
    let mut abstract_by_key: BTreeMap<String, (AbstractNumberingId, BTreeSet<u8>)> =
        BTreeMap::new();
    let mut abstract_numbering = DefinitionMap::default();
    for raw in abstracts {
        if abstract_by_key.contains_key(&raw.id) {
            reporter.report(b"abstractNum");
            continue;
        }
        let id = AbstractNumberingId::new(next_id(ids)?);
        let mut levels = Vec::with_capacity(raw.levels.len());
        let mut defined = BTreeSet::new();
        for level in raw.levels {
            if defined.insert(level.level) {
                levels.push(NumberingLevel {
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
                    pstyle: resolve_style_link(
                        styles,
                        level.pstyle.as_deref(),
                        reporter,
                        b"pStyle",
                    ),
                });
            }
        }
        abstract_by_key.insert(raw.id.clone(), (id, defined));
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
    let mut valid_levels = BTreeMap::new();
    let mut instances = DefinitionMap::default();
    for raw in nums {
        if by_num_id.contains_key(&raw.num_id) {
            reporter.report(b"num");
            continue;
        }
        let Some((abstract_ref, levels)) = raw
            .abstract_id
            .as_deref()
            .and_then(|key| abstract_by_key.get(key))
        else {
            reporter.report(b"num");
            continue;
        };
        let id = NumberingInstanceId::new(next_id(ids)?);
        by_num_id.insert(raw.num_id, id);
        valid_levels.insert(id, levels.clone());
        // Keep only the last override per level (a later `w:lvlOverride` for the
        // same ilvl wins), preserving level order for deterministic output.
        let mut overrides: Vec<NumberingOverride> = Vec::new();
        for (level, start) in raw.overrides {
            match overrides.iter_mut().find(|o| o.level == level) {
                Some(existing) => existing.start = Some(start),
                None => overrides.push(NumberingOverride {
                    level,
                    start: Some(start),
                }),
            }
        }
        overrides.sort_by_key(|o| o.level);
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
        valid_levels,
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
                    element.local_name().as_ref(),
                    &element,
                );
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                let local = element.local_name();
                on_start(&mut state, reporter, local.as_ref(), &element);
                on_end(&mut state, local.as_ref(), &mut abstracts, &mut nums);
            }
            Event::End(element) => {
                on_end(
                    &mut state,
                    element.local_name().as_ref(),
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
        if let Some(level) = state.current_level.as_mut()
            && !apply_paragraph_property(&mut level.paragraph, local, element)
        {
            reporter.report(local);
        }
        return;
    }
    match local {
        b"numbering" => {}
        b"abstractNum" => {
            state.current_abstract = Some(RawAbstract {
                id: attribute_value(element, b"abstractNumId").unwrap_or_default(),
                ..RawAbstract::default()
            });
        }
        b"lvl" if state.current_abstract.is_some() => {
            state.current_level = Some(RawLevel {
                level: attribute_value(element, b"ilvl")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
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
        b"numFmt" if state.current_level.is_some() => match number_format(element) {
            Some(format) => set_level(state, |level| level.num_fmt = Some(format)),
            None => reporter.report(local),
        },
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
            // Track the level this override targets; a nested `w:startOverride`
            // (below) reads it. `w:ilvl` defaults to 0 when absent.
            state.current_override_ilvl = Some(
                attribute_value(element, b"ilvl")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
            );
        }
        b"startOverride" if state.current_num.is_some() => {
            // A per-instance restart (`<w:lvlOverride><w:startOverride w:val="N"/>`):
            // the ubiquitous "this list restarts at N" case. A `w:startOverride`
            // outside a `w:lvlOverride` defaults its level to 0, matching Word.
            if let (Some(num), Some(start)) = (
                state.current_num.as_mut(),
                attribute_value(element, b"val").and_then(|value| value.parse::<u16>().ok()),
            ) {
                let ilvl = state.current_override_ilvl.unwrap_or(0);
                num.overrides.push((ilvl, start.min(32_767)));
            }
        }
        // A `w:lvlOverride/w:lvl` full-format override is not representable in the
        // per-instance model (only start is); its children fall through and are
        // reported. Any still-unmapped numbering detail is too.
        _ if state.current_abstract.is_some() || state.current_num.is_some() => {
            reporter.report(local);
        }
        _ => {}
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
    match local {
        b"pPr" => state.ppr_depth = state.ppr_depth.saturating_sub(1),
        b"rPr" => state.rpr_depth = state.rpr_depth.saturating_sub(1),
        b"lvl" => {
            if let (Some(abstract_num), Some(level)) =
                (state.current_abstract.as_mut(), state.current_level.take())
            {
                abstract_num.levels.push(level);
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
    let value = attribute_value(element, b"val").filter(|value| !value.is_empty())?;
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
