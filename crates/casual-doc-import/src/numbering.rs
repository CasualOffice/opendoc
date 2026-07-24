//! Numbering-part parsing: OOXML abstractNum/num string ids -> deterministic v1
//! ids, and w:numPr resolution. Mirrors the styles pattern.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{
    AbstractNumbering, AbstractNumberingId, DefinitionMap, NumberingInstance, NumberingInstanceId,
    NumberingLevel, NumberingRef,
};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::attribute_value;
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
            });
        }
        b"start" if state.current_level.is_some() => {
            if let Some(level) = state.current_level.as_mut() {
                level.start = attribute_value(element, b"val")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1);
            }
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
        // Unmapped numbering detail (numFmt, lvlText, pPr, rPr, ...) is reported.
        _ if state.current_abstract.is_some() || state.current_num.is_some() => {
            reporter.report(local);
        }
        _ => {}
    }
}

fn on_end(
    state: &mut NumberingState,
    local: &[u8],
    abstracts: &mut Vec<RawAbstract>,
    nums: &mut Vec<RawNum>,
) {
    match local {
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
