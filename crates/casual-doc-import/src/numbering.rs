//! Numbering-part parsing: OOXML abstractNum/num string ids -> deterministic v1
//! ids, and w:numPr resolution. Mirrors the styles pattern.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{
    AbstractNumbering, AbstractNumberingId, DefinitionMap, LevelJustification, LevelSuffix,
    NumberFormat, NumberingInstance, NumberingInstanceId, NumberingLevel, NumberingRef,
    ParagraphProperties, RunProperties,
};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::{apply_paragraph_property, apply_run_property, attribute_value};
use crate::report::Reporter;

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
    paragraph: ParagraphProperties,
    has_paragraph: bool,
    run: RunProperties,
    has_run: bool,
}

#[derive(Default)]
struct RawAbstract {
    id: String,
    levels: Vec<RawLevel>,
}

struct RawNum {
    num_id: String,
    abstract_id: Option<String>,
}

/// Parses the numbering part, allocating ids from `ids`.
pub(crate) fn parse(
    xml: &[u8],
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
    config: ImportConfig,
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
                });
            }
        }
        abstract_by_key.insert(raw.id.clone(), (id, defined));
        abstract_numbering.insert(id, AbstractNumbering { levels });
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
        instances.insert(
            id,
            NumberingInstance {
                abstract_ref: *abstract_ref,
                overrides: Vec::new(),
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

#[derive(Default)]
struct NumberingState {
    current_abstract: Option<RawAbstract>,
    current_level: Option<RawLevel>,
    current_num: Option<RawNum>,
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
                levels: Vec::new(),
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
            });
        }
        b"abstractNumId" if state.current_num.is_some() => {
            if let Some(num) = state.current_num.as_mut() {
                num.abstract_id = attribute_value(element, b"val");
            }
        }
        // Unmapped numbering detail (lvlRestart, pStyle, lvlOverride, ...) is
        // reported.
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
fn number_format(element: &BytesStart<'_>) -> Option<NumberFormat> {
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
