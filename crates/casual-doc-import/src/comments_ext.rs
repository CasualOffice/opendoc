//! Comment companion parts: `commentsExtended.xml` (reply threading + resolved
//! state), `commentsIds.xml` (durable ids), and `people.xml` (collaborator
//! identity). All three join to the base `word/comments.xml` on `paraId` — the
//! `w14:paraId` of a comment's last paragraph — so a threaded review survives a
//! semantic edit->save instead of collapsing to flat comments (P1F-10).
//!
//! Elements and attributes are matched by local name (namespace-agnostic), so a
//! producer's exact `w14`/`w15`/`w16cid` prefixes do not matter. Every parser is
//! bounded by the shared element/depth ceilings.
//!
//! These parts are *regenerated* by the semantic writer, never retained, so
//! anything a parser skips is gone for good. Every element that reaches neither
//! the model nor the writer is therefore reported (FID-R-04): the part roots and
//! the fully mapped entries raise no finding, an entry dropped for a missing
//! join key or an unmodeled element does.

use std::collections::BTreeMap;

use casual_doc_model::v1::{Person, PresenceInfo};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::{attribute_value, is_true};
use crate::report::Reporter;

/// Maximum byte length of a durable hex token (`paraId`/`paraIdParent`/
/// `durableId`); matches the model's `comment.threadId` domain.
const MAX_TOKEN_BYTES: usize = 64;
/// Maximum byte length of an identity string (author/provider/user id); matches
/// the model's `person.*` domain.
const MAX_IDENTITY_BYTES: usize = 255;

/// A `commentsExtended.xml` `w15:commentEx` entry (keyed by `paraId`): the parent
/// comment's `paraId` when this is a reply, plus the resolved/done flag.
pub(crate) struct CommentExtended {
    pub parent_para_id: Option<String>,
    pub done: bool,
}

/// Reads one bounded token attribute (`<= MAX_TOKEN_BYTES`, non-empty).
fn token(element: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    attribute_value(element, name)
        .filter(|value| !value.is_empty() && value.len() <= MAX_TOKEN_BYTES)
}

/// Reads one bounded identity attribute (`<= MAX_IDENTITY_BYTES`, non-empty).
fn identity(element: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    attribute_value(element, name)
        .filter(|value| !value.is_empty() && value.len() <= MAX_IDENTITY_BYTES)
}

fn bump(count: &mut u64, max: u64) -> Result<(), ImportError> {
    *count += 1;
    if *count > max {
        return Err(ImportError::LimitExceeded {
            limit: "xml_elements",
        });
    }
    Ok(())
}

/// Scans `word/comments.xml` for each comment's threading key: the `w14:paraId`
/// of its last direct-child paragraph, keyed by the comment's `w:id`. Only
/// direct-child `w:p` elements are considered (matching the writer, which stamps
/// the id on the comment's last top-level paragraph), so nested table paragraphs
/// do not shadow it.
///
/// Deliberately reports nothing: this is a second, key-only pass over
/// `word/comments.xml`, whose every element is already dispositioned by the body
/// parser in `build_comments`. Reporting here would double-count that part.
pub(crate) fn scan_comment_para_ids(
    xml: &[u8],
    config: ImportConfig,
) -> Result<BTreeMap<String, String>, ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut out = BTreeMap::new();
    let mut elements = 0_u64;
    let mut depth = 0_u64;
    // The open comment: its `w:id` and the nesting level at which `w:comment`
    // opened; a direct-child `w:p` opens at `level + 1`.
    let mut current: Option<(String, u64)> = None;
    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| ImportError::MalformedXml)?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(ImportError::MalformedXml),
            Event::Start(element) => {
                let level = depth;
                depth += 1;
                if depth > config.max_depth {
                    return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                }
                bump(&mut elements, config.max_elements)?;
                match element.local_name().as_ref() {
                    b"comment" => {
                        if let Some(id) = attribute_value(&element, b"id") {
                            current = Some((id, level));
                        }
                    }
                    b"p" => record_para(&current, level, &element, &mut out),
                    _ => {}
                }
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                if element.local_name().as_ref() == b"p" {
                    record_para(&current, depth, &element, &mut out);
                }
            }
            Event::End(element) => {
                depth = depth.saturating_sub(1);
                if element.local_name().as_ref() == b"comment" {
                    current = None;
                }
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(out)
}

/// Records a paragraph's `w14:paraId` for the open comment when the paragraph is
/// a direct child of `w:comment` (its level is the comment's level + 1). A later
/// direct-child paragraph overwrites an earlier one, so the last wins.
fn record_para(
    current: &Option<(String, u64)>,
    level: u64,
    element: &BytesStart<'_>,
    out: &mut BTreeMap<String, String>,
) {
    if let Some((id, comment_level)) = current
        && level == comment_level + 1
        && let Some(para_id) = token(element, b"paraId")
    {
        out.insert(id.clone(), para_id);
    }
}

/// Parses `commentsExtended.xml` into `paraId -> {parent, done}`. A `w15:commentEx`
/// is fully mapped (`paraId`, `paraIdParent` and `done` are its whole content),
/// so a well-formed entry raises no finding; one without a usable `paraId` has no
/// comment to join to and is reported as dropped, as is any other element.
pub(crate) fn parse_comments_extended(
    xml: &[u8],
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<BTreeMap<String, CommentExtended>, ImportError> {
    let mut out = BTreeMap::new();
    each_element(xml, config, |element| {
        match element.local_name().as_ref() {
            // The part root carries no data of its own.
            b"commentsEx" => {}
            b"commentEx" => match token(element, b"paraId") {
                Some(para_id) => {
                    let parent_para_id = token(element, b"paraIdParent");
                    let done = attribute_value(element, b"done")
                        .map(|value| is_true(Some(value.as_str())))
                        .unwrap_or(false);
                    out.insert(
                        para_id,
                        CommentExtended {
                            parent_para_id,
                            done,
                        },
                    );
                }
                // No usable `paraId` means there is no comment to join this
                // entry to and nothing to rewrite it as: it is structurally
                // unusable, which doc 35 calls `rejected` — refused and
                // reported — not `not-retained`, which is a deliberate drop of
                // something that could have been carried.
                None => reporter.report_invalid(b"commentEx"),
            },
            other => reporter.report(other),
        }
        Ok(())
    })?;
    Ok(out)
}

/// Parses `commentsIds.xml` into `paraId -> durableId`. Both attributes are the
/// whole of a `w16cid:commentId`, so a complete pair is fully mapped; an entry
/// missing either half cannot be joined or rewritten and is reported.
pub(crate) fn parse_comments_ids(
    xml: &[u8],
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<BTreeMap<String, String>, ImportError> {
    let mut out = BTreeMap::new();
    each_element(xml, config, |element| {
        match element.local_name().as_ref() {
            // The part root carries no data of its own.
            b"commentsIds" => {}
            b"commentId" => match (token(element, b"paraId"), token(element, b"durableId")) {
                (Some(para_id), Some(durable_id)) => {
                    out.insert(para_id, durable_id);
                }
                // Half a durable-id pair can neither be joined nor rewritten.
                _ => reporter.report_invalid(b"commentId"),
            },
            other => reporter.report(other),
        }
        Ok(())
    })?;
    Ok(out)
}

/// Parses `people.xml` into the collaborator identity table, in document order.
/// `w15:person` (its `author`) and `w15:presenceInfo` (`providerId`/`userId`) are
/// fully mapped; a person with no usable `author`, a presence record with no
/// person to attach to, and any other element are dropped and reported.
pub(crate) fn parse_people(
    xml: &[u8],
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<Vec<Person>, ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut people = Vec::new();
    let mut pending: Option<Person> = None;
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
                match element.local_name().as_ref() {
                    // The part root carries no data of its own.
                    b"people" => {}
                    b"person" => {
                        pending = identity(&element, b"author").map(|author| Person {
                            author,
                            presence: None,
                        });
                        if pending.is_none() {
                            // A collaborator with no usable author name cannot be
                            // identified or rewritten: unusable, not merely dropped.
                            reporter.report_invalid(b"person");
                        }
                    }
                    b"presenceInfo" => match pending.as_mut() {
                        Some(person) => person.presence = Some(presence(&element)),
                        // Presence with no open person has nothing to attach to.
                        None => reporter.report_invalid(b"presenceInfo"),
                    },
                    other => reporter.report(other),
                }
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                match element.local_name().as_ref() {
                    // A childless `<w15:people/>` carries no data of its own.
                    b"people" => {}
                    b"person" => match identity(&element, b"author") {
                        Some(author) => people.push(Person {
                            author,
                            presence: None,
                        }),
                        None => reporter.report_invalid(b"person"),
                    },
                    b"presenceInfo" => match pending.as_mut() {
                        Some(person) => person.presence = Some(presence(&element)),
                        None => reporter.report_invalid(b"presenceInfo"),
                    },
                    other => reporter.report(other),
                }
            }
            Event::End(element) => {
                depth = depth.saturating_sub(1);
                if element.local_name().as_ref() == b"person"
                    && let Some(person) = pending.take()
                {
                    people.push(person);
                }
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(people)
}

/// Reads a `w15:presenceInfo` element's provider/user ids (empty when absent).
fn presence(element: &BytesStart<'_>) -> PresenceInfo {
    PresenceInfo {
        provider_id: identity(element, b"providerId").unwrap_or_default(),
        user_id: identity(element, b"userId").unwrap_or_default(),
    }
}

/// Walks the flat companion parts (`commentsExtended`/`commentsIds`), invoking
/// `visit` for each element (`Start` and `Empty`) under the shared ceilings.
/// Both parts are two levels deep by schema, so a visitor that reports an
/// unmodeled element sees it once rather than once per descendant.
fn each_element(
    xml: &[u8],
    config: ImportConfig,
    mut visit: impl FnMut(&BytesStart<'_>) -> Result<(), ImportError>,
) -> Result<(), ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
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
                visit(&element)?;
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                visit(&element)?;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
        buffer.clear();
    }
    Ok(())
}
