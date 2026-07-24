//! Styles-part parsing: OOXML string style ids -> deterministic v1 StyleIds,
//! basedOn inheritance (dangling/kind-mismatch/cycle broken and reported), and
//! reporting of unmapped style-part constructs.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{
    DefinitionMap, ParagraphProperties, RunProperties, Style, StyleId, StyleKind,
};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::{
    apply_paragraph_property, apply_run_property, attribute_value, style_kind_from,
};
use crate::report::Reporter;

/// Resolved style definitions and their name/kind index.
#[derive(Debug, Default)]
pub(crate) struct Styles {
    by_name: BTreeMap<String, (StyleId, StyleKind)>,
    definitions: DefinitionMap<StyleId, Style>,
}

impl Styles {
    /// Resolves a style name to its id, requiring the kind to match the site.
    pub(crate) fn resolve(&self, name: &str, expected: StyleKind) -> Option<StyleId> {
        self.by_name
            .get(name)
            .filter(|(_, kind)| *kind == expected)
            .map(|(id, _)| *id)
    }

    pub(crate) fn into_definitions(self) -> DefinitionMap<StyleId, Style> {
        self.definitions
    }
}

struct RawStyle {
    style_id: String,
    kind: Option<StyleKind>,
    based_on: Option<String>,
    paragraph: Option<ParagraphProperties>,
    run: Option<RunProperties>,
}

/// Parses the styles part into resolved styles, allocating ids from `ids`.
pub(crate) fn parse(
    xml: &[u8],
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<Styles, ImportError> {
    let raw = parse_raw(xml, reporter, config)?;

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
    for (id, kind, based_on, style) in candidates {
        let based_on = if cyclic.contains(&id) {
            reporter.report(b"basedOn");
            None
        } else {
            based_on
        };
        definitions.insert(
            id,
            Style {
                kind,
                based_on,
                paragraph: style.paragraph,
                run: style.run,
            },
        );
    }

    Ok(Styles {
        by_name,
        definitions,
    })
}

fn next_id(ids: &mut IdGenerator) -> Result<casual_doc_model::NodeId, ImportError> {
    ids.next_id()
        .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })
}

#[derive(Default)]
struct StyleState {
    style_id: String,
    kind: Option<StyleKind>,
    based_on: Option<String>,
    paragraph: ParagraphProperties,
    has_paragraph: bool,
    run: RunProperties,
    has_run: bool,
    ppr_depth: u32,
    rpr_depth: u32,
}

fn parse_raw(
    xml: &[u8],
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<Vec<RawStyle>, ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut styles = Vec::new();
    let mut in_style = false;
    let mut state = StyleState::default();
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
                bump_elements(&mut elements, config.max_elements)?;
                on_start(
                    &mut in_style,
                    &mut state,
                    reporter,
                    element.local_name().as_ref(),
                    &element,
                );
            }
            Event::Empty(element) => {
                bump_elements(&mut elements, config.max_elements)?;
                let local = element.local_name();
                on_start(
                    &mut in_style,
                    &mut state,
                    reporter,
                    local.as_ref(),
                    &element,
                );
                on_end(&mut in_style, &mut state, local.as_ref(), &mut styles);
            }
            Event::End(element) => {
                on_end(
                    &mut in_style,
                    &mut state,
                    element.local_name().as_ref(),
                    &mut styles,
                );
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(styles)
}

fn bump_elements(elements: &mut u64, max: u64) -> Result<(), ImportError> {
    *elements += 1;
    if *elements > max {
        return Err(ImportError::LimitExceeded {
            limit: "xml_elements",
        });
    }
    Ok(())
}

fn on_start(
    in_style: &mut bool,
    state: &mut StyleState,
    reporter: &mut Reporter,
    local: &[u8],
    element: &BytesStart<'_>,
) {
    match local {
        b"styles" | b"style" => {
            if local == b"style" {
                *in_style = true;
                *state = StyleState {
                    style_id: attribute_value(element, b"styleId").unwrap_or_default(),
                    kind: attribute_value(element, b"type")
                        .as_deref()
                        .and_then(style_kind_from),
                    ..StyleState::default()
                };
            }
        }
        b"name" if *in_style => {}
        b"basedOn" if *in_style => state.based_on = attribute_value(element, b"val"),
        b"pPr" if *in_style && state.rpr_depth == 0 => {
            state.ppr_depth += 1;
            state.has_paragraph = true;
        }
        b"rPr" if *in_style => {
            state.rpr_depth += 1;
            state.has_run = true;
        }
        _ if state.rpr_depth > 0 => {
            if !apply_run_property(&mut state.run, local, element) {
                reporter.report(local);
            }
        }
        _ if state.ppr_depth > 0 => {
            if !apply_paragraph_property(&mut state.paragraph, local, element) {
                reporter.report(local);
            }
        }
        _ if *in_style => reporter.report(local),
        _ => {}
    }
}

fn on_end(in_style: &mut bool, state: &mut StyleState, local: &[u8], styles: &mut Vec<RawStyle>) {
    match local {
        b"style" if *in_style => {
            *in_style = false;
            let finished = std::mem::take(state);
            styles.push(RawStyle {
                style_id: finished.style_id,
                kind: finished.kind,
                based_on: finished.based_on,
                paragraph: finished.has_paragraph.then_some(finished.paragraph),
                run: finished.has_run.then_some(finished.run),
            });
        }
        b"pPr" => state.ppr_depth = state.ppr_depth.saturating_sub(1),
        b"rPr" => state.rpr_depth = state.rpr_depth.saturating_sub(1),
        _ => {}
    }
}
