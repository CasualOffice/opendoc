// SPDX-License-Identifier: Apache-2.0

//! Lists, lowered once (`docs/181` §4.2 step 3; `docs/76`, `docs/142`).
//!
//! The list definitions the editor creates, and the restart / continue
//! numbering rules, used to live inside the editor's wasm methods. They live
//! here now so an agent's `set_list`, `restart_numbering` and
//! `continue_numbering` produce the editor's operations.
//!
//! Every function takes the paragraphs **in document order** from the caller
//! (`ordered`), because which surfaces a caller walks, and in what order, is
//! the caller's: the editor walks every surface, an agent the body it reads.

use casual_doc_edit::table_structure::MintId;
use casual_doc_edit::{Operation, ParagraphIndex, refused};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AbstractNumbering, AbstractNumberingId, Document, Indentation, LevelJustification, LevelSuffix,
    NumberFormat, NumberingInstance, NumberingInstanceId, NumberingLevel, NumberingOverride,
    NumberingRef, ParagraphProperties,
};

/// One level of the bullet (`numbered == false`) or decimal list the editor
/// creates: `1.`, `1.1.`, … or `•`, `◦`, `▪`, `–`, indented 0.5" per level
/// with a 0.25" hanging indent. O(level).
#[must_use]
pub fn level(numbered: bool, level: u8) -> NumberingLevel {
    let (num_fmt, lvl_text) = if numbered {
        let placeholders = (1..=level + 1)
            .map(|part| format!("%{part}"))
            .collect::<Vec<_>>()
            .join(".");
        (NumberFormat::Decimal, format!("{placeholders}."))
    } else {
        let glyph = ["\u{2022}", "\u{25e6}", "\u{25aa}", "\u{2013}"][usize::from(level) % 4];
        (NumberFormat::Bullet, glyph.to_string())
    };
    NumberingLevel {
        picture_bullet: None,
        level,
        start: 1,
        num_fmt: Some(num_fmt),
        lvl_text: Some(lvl_text),
        lvl_jc: Some(LevelJustification::Start),
        suff: Some(LevelSuffix::Tab),
        is_lgl: false,
        paragraph_properties: Some(ParagraphProperties {
            indentation: Some(Indentation {
                start_twips: Some(720 + i32::from(level) * 360),
                end_twips: None,
                first_line_twips: None,
                hanging_twips: Some(360),
            }),
            ..ParagraphProperties::default()
        }),
        run_properties: None,
        style_ref: None,
        lvl_restart: None,
        pstyle: None,
        template_code: None,
        tentative: false,
    }
}

/// The two operations that install a list definition with `levels` (the
/// abstract first, then the instance naming it), and the instance id. Ids are
/// minted abstract first. O(levels).
///
/// # Errors
/// When ids run out.
pub fn install(
    levels: Vec<NumberingLevel>,
    ids: MintId<'_>,
) -> Result<(NumberingInstanceId, Vec<Operation>), String> {
    let exhausted = || "id space exhausted".to_owned();
    let abstract_id = AbstractNumberingId::new(ids().ok_or_else(exhausted)?);
    let instance = NumberingInstanceId::new(ids().ok_or_else(exhausted)?);
    Ok((
        instance,
        vec![
            Operation::SetAbstractNumbering {
                id: abstract_id,
                definition: Some(Box::new(AbstractNumbering {
                    levels,
                    multi_level_type: None,
                    num_style_link: None,
                    style_link: None,
                })),
            },
            Operation::SetNumberingInstance {
                id: instance,
                instance: Some(Box::new(NumberingInstance {
                    abstract_ref: abstract_id,
                    overrides: Vec::new(),
                })),
            },
        ],
    ))
}

/// A new bullet or numbered list definition: [`install`] of nine [`level`]s.
///
/// # Errors
/// When ids run out.
pub fn new_definition(
    numbered: bool,
    ids: MintId<'_>,
) -> Result<(NumberingInstanceId, Vec<Operation>), String> {
    install((0..=8).map(|n| level(numbered, n)).collect(), ids)
}

/// An instance already in `document` that is exactly the editor's bullet or
/// numbered list with no overrides, so a second agent edit joins the first
/// one's list rather than starting another (the editor reuses its own the
/// same way). O(numbering definitions).
#[must_use]
pub fn existing_definition(document: &Document, numbered: bool) -> Option<NumberingInstanceId> {
    let definitions = document.definitions();
    let first = level(numbered, 0);
    definitions.numbering.iter().find_map(|(id, instance)| {
        let abstract_numbering = definitions.abstract_numbering.get(&instance.abstract_ref)?;
        (instance.overrides.is_empty()
            && abstract_numbering.levels.len() == 9
            && abstract_numbering.levels.first() == Some(&first))
        .then_some(*id)
    })
}

/// The number format of an instance's first level, if it resolves. O(1).
#[must_use]
pub fn number_format(document: &Document, instance: NumberingInstanceId) -> Option<NumberFormat> {
    let definitions = document.definitions();
    let resolved = definitions.numbering.get(&instance)?;
    let abstract_numbering = definitions.abstract_numbering.get(&resolved.abstract_ref)?;
    abstract_numbering
        .levels
        .first()
        .and_then(|first| first.num_fmt.clone())
}

fn numbered_item(document: &Document, start: NodeId) -> Result<NumberingRef, String> {
    let not_numbered = || {
        refused!(
            "list.continue-not-numbered",
            "Continue numbering works on a numbered list item."
        )
        .to_owned()
    };
    let current = casual_doc_edit::paragraph_properties(document, start)
        .and_then(|p| p.numbering)
        .ok_or_else(not_numbered)?;
    if number_format(document, current.instance) == Some(NumberFormat::Bullet) {
        return Err(not_numbered());
    }
    Ok(current)
}

/// Restarts the numbered list at paragraph `start` at `value` (1 to 32767): a
/// new instance with a start override for the item's level, carried through
/// the contiguous following items of the same list at that level or deeper.
/// The instance is installed in the same step as the paragraphs naming it,
/// first. O(paragraphs) with one index.
///
/// # Errors
/// When `start` is not a numbered list item, `value` is out of range, there
/// is nothing to restart, or ids run out.
pub fn restart(
    document: &Document,
    ordered: &[NodeId],
    start: NodeId,
    value: u16,
    ids: MintId<'_>,
) -> Result<Vec<Operation>, String> {
    if !(1..=32767).contains(&value) {
        return Err("list.start-out-of-range: Choose a value from 1 to 32767.".to_owned());
    }
    let refused = || "restart numbering requires a numbered list item".to_owned();
    let current = casual_doc_edit::paragraph_properties(document, start)
        .and_then(|p| p.numbering)
        .ok_or_else(refused)?;
    if number_format(document, current.instance) == Some(NumberFormat::Bullet) {
        return Err(refused());
    }
    let original = document
        .definitions()
        .numbering
        .get(&current.instance)
        .ok_or_else(|| "numbering definition not found".to_owned())?;
    let abstract_ref = original.abstract_ref;
    let mut overrides = original.overrides.clone();
    if let Some(existing) = overrides.iter_mut().find(|o| o.level == current.level) {
        existing.start = Some(value);
    } else {
        overrides.push(NumberingOverride {
            level: current.level,
            start: Some(value),
            definition: None,
        });
    }
    let new_instance =
        NumberingInstanceId::new(ids().ok_or_else(|| "id space exhausted".to_owned())?);
    let by_id = ParagraphIndex::build(document);
    let index = ordered
        .iter()
        .position(|id| *id == start)
        .ok_or_else(|| "paragraph not found".to_owned())?;
    let mut operations = vec![Operation::SetNumberingInstance {
        id: new_instance,
        instance: Some(Box::new(NumberingInstance {
            abstract_ref,
            overrides,
        })),
    }];
    for id in &ordered[index..] {
        let Some(properties) = by_id.properties(*id) else {
            continue;
        };
        let Some(numbering) = properties.numbering else {
            break;
        };
        if numbering.instance != current.instance || numbering.level < current.level {
            break;
        }
        let mut next = properties.clone();
        next.numbering = Some(NumberingRef {
            instance: new_instance,
            level: numbering.level,
        });
        operations.push(Operation::SetParagraphProperties {
            node: *id,
            properties: Box::new(next),
        });
    }
    if operations.len() == 1 {
        return Err(refused!(
            "list.restart-none",
            "There are no numbered items here to restart."
        )
        .to_owned());
    }
    Ok(operations)
}

/// Continues the numbered list at paragraph `start` from the nearest earlier
/// numbered list at the same level: the item, and the contiguous following
/// items that shared its instance and level, adopt that list's instance.
/// O(paragraphs) with one index.
///
/// # Errors
/// When `start` is not a numbered list item, there is no earlier list at its
/// level, or it already continues the one above.
pub fn continue_numbering(
    document: &Document,
    ordered: &[NodeId],
    start: NodeId,
) -> Result<Vec<Operation>, String> {
    let current = numbered_item(document, start)?;
    let by_id = ParagraphIndex::build(document);
    let index = ordered
        .iter()
        .position(|id| *id == start)
        .ok_or_else(|| "paragraph not found".to_owned())?;
    let mut target = None;
    for id in ordered[..index].iter().rev() {
        let Some(numbering) = by_id.properties(*id).and_then(|p| p.numbering) else {
            continue;
        };
        if number_format(document, numbering.instance) == Some(NumberFormat::Bullet)
            || numbering.level != current.level
        {
            continue;
        }
        if numbering.instance == current.instance {
            return Err(refused!(
                "list.continue-already",
                "This item already continues the numbering above it."
            )
            .to_owned());
        }
        target = Some(numbering.instance);
        break;
    }
    let target = target.ok_or_else(|| {
        refused!(
            "list.continue-no-preceding",
            "There is no earlier numbered list at this level to continue from."
        )
        .to_owned()
    })?;
    let mut operations = Vec::new();
    for id in &ordered[index..] {
        let Some(properties) = by_id.properties(*id) else {
            continue;
        };
        let Some(numbering) = properties.numbering else {
            break;
        };
        if numbering.instance != current.instance || numbering.level != current.level {
            break;
        }
        let mut next = properties.clone();
        next.numbering = Some(NumberingRef {
            instance: target,
            level: current.level,
        });
        operations.push(Operation::SetParagraphProperties {
            node: *id,
            properties: Box::new(next),
        });
    }
    if operations.is_empty() {
        return Err(refused!(
            "list.continue-none",
            "There are no numbered items here to continue."
        )
        .to_owned());
    }
    Ok(operations)
}
