// SPDX-License-Identifier: Apache-2.0

//! Bounded, namespace-agnostic recursive descent over one package part.
//!
//! # Why a cursor and not a flat event loop
//!
//! A PresentationML part is a tree whose meaning is positional: `a:ext` is the
//! extent of a shape under `a:xfrm` and an *extension list entry* under
//! `a:extLst`, and the local name alone cannot tell them apart. A flat loop with
//! a mode flag per context is the version of this that misreads a file the first
//! time a producer nests something unexpectedly — so each element's children are
//! read by the function that understands that element, and anything no function
//! claims is skipped as a whole subtree.
//!
//! # What is matched, and what is deliberately not
//!
//! Elements are matched on their **local** name, with no namespace resolution.
//! That follows `casual-doc-ooxml`'s package-metadata reader and is sound here
//! for the same reason: within a known parent, PresentationML and DrawingML local
//! names do not collide, and a producer's choice of prefix (`a:`, `p:`, or a
//! default-bound namespace) is arbitrary. Where a local name genuinely does carry
//! two meanings, the ambiguity is resolved by *position* — which is what this
//! module exists to make possible.
//!
//! # Hardening
//!
//! A `DOCTYPE` fails the part unconditionally. An external entity is an
//! exfiltration and denial-of-service primitive and no presentation needs one;
//! `quick-xml` does not expand external entities, but refusing the declaration
//! outright means this does not depend on that staying true.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::ImportError;
use crate::limits::ImportLimits;

/// A bounded reader positioned inside one package part.
#[derive(Debug)]
pub(crate) struct Cursor<'a> {
    reader: Reader<&'a [u8]>,
    part: &'a str,
    limits: ImportLimits,
    elements: u64,
    depth: u32,
    /// Whether the document element was written self-closing (`<a:tblStyleLst
    /// def="…"/>`). Its children are then already read — there are none — and
    /// [`children`] at root level must return at once rather than read on to the
    /// end of the part looking for a closing tag that was never written.
    root_empty: bool,
}

impl<'a> Cursor<'a> {
    /// A cursor over `bytes`, charging every bound to `part` in diagnostics.
    pub(crate) fn new(bytes: &'a [u8], part: &'a str, limits: ImportLimits) -> Self {
        let mut reader = Reader::from_reader(bytes);
        // A presentation part's significant whitespace lives only inside `a:t`,
        // which is read through `read_text`; trimming elsewhere keeps indentation
        // a producer added for readability from becoming document content.
        reader.config_mut().trim_text(false);
        Self {
            reader,
            part,
            limits,
            elements: 0,
            depth: 0,
            root_empty: false,
        }
    }

    /// The part this cursor reads, for a bounded finding location.
    pub(crate) const fn part(&self) -> &'a str {
        self.part
    }

    /// The limits in force, so a reader can bound its own collection.
    pub(crate) const fn limits(&self) -> ImportLimits {
        self.limits
    }

    fn malformed(&self) -> ImportError {
        ImportError::MalformedPartXml {
            part: self.part.to_owned(),
        }
    }

    fn exceeded(&self, limit: &'static str, observed: u64, allowed: u64) -> ImportError {
        ImportError::PartLimitExceeded {
            part: self.part.to_owned(),
            limit,
            observed,
            allowed,
        }
    }

    /// Reads one event, charging it against the element budget.
    fn event(&mut self) -> Result<Event<'a>, ImportError> {
        let event = self.reader.read_event().map_err(|_| self.malformed())?;
        if matches!(event, Event::DocType(_)) {
            return Err(self.malformed());
        }
        if matches!(event, Event::Start(_) | Event::Empty(_)) {
            self.elements = self.elements.saturating_add(1);
            if self.elements > self.limits.max_part_elements {
                return Err(self.exceeded(
                    "part_xml_elements",
                    self.elements,
                    self.limits.max_part_elements,
                ));
            }
        }
        Ok(event)
    }

    fn push_depth(&mut self) -> Result<(), ImportError> {
        self.depth = self.depth.saturating_add(1);
        if self.depth > self.limits.max_part_depth {
            return Err(self.exceeded(
                "part_xml_depth",
                u64::from(self.depth),
                u64::from(self.limits.max_part_depth),
            ));
        }
        Ok(())
    }

    /// Advances to the document element, returning its start tag.
    ///
    /// The caller then reads its children with [`children`]. Returns the
    /// malformed error for a part with no element at all.
    pub(crate) fn root(&mut self) -> Result<BytesStart<'a>, ImportError> {
        loop {
            match self.event()? {
                Event::Start(element) => return Ok(element),
                // An empty root is a legal part, and not an unusual one:
                // PowerPoint writes `<a:tblStyleLst def="{GUID}"/>` for every deck
                // with no inserted table. It has no children, and the cursor
                // remembers that so the caller's `children` call reads nothing —
                // without it, `children` read to the end of the part and refused
                // the whole deck as malformed.
                Event::Empty(element) => {
                    self.root_empty = true;
                    return Ok(element);
                }
                Event::Eof => return Err(self.malformed()),
                _ => {}
            }
        }
    }

    /// Consumes the remainder of the subtree the cursor has just entered,
    /// including its end tag.
    ///
    /// Tracks its own balance rather than the cursor's depth, because the subtree
    /// being discarded may be arbitrarily deep while contributing nothing to the
    /// *modelled* nesting the depth bound protects.
    fn skip_subtree(&mut self) -> Result<(), ImportError> {
        let mut balance = 1_u64;
        loop {
            match self.event()? {
                Event::Start(_) => {
                    balance = balance.saturating_add(1);
                    if balance > u64::from(self.limits.max_part_depth) {
                        return Err(self.exceeded(
                            "part_xml_depth",
                            balance,
                            u64::from(self.limits.max_part_depth),
                        ));
                    }
                }
                Event::End(_) => {
                    balance -= 1;
                    if balance == 0 {
                        return Ok(());
                    }
                }
                Event::Eof => return Err(self.malformed()),
                _ => {}
            }
        }
    }

    /// Reads the character data of the element the cursor has just entered,
    /// consuming its end tag.
    ///
    /// Concatenates `Text` and `CDATA`, which is what a producer splitting an
    /// `a:t` across a CDATA section means, and refuses a nested element: nothing
    /// in the modelled subset puts markup inside character data, so admitting it
    /// would mean silently dropping it.
    pub(crate) fn read_text(&mut self, max_bytes: usize) -> Result<String, ImportError> {
        let mut text = String::new();
        loop {
            match self.event()? {
                Event::Text(bytes) => {
                    let decoded = bytes.decode().map_err(|_| self.malformed())?;
                    if text.len().saturating_add(decoded.len()) > max_bytes {
                        return Err(self.exceeded(
                            "text_bytes",
                            text.len().saturating_add(decoded.len()) as u64,
                            max_bytes as u64,
                        ));
                    }
                    text.push_str(decoded.as_ref());
                }
                Event::CData(bytes) => {
                    let decoded = core::str::from_utf8(bytes.as_ref())
                        .map_err(|_| self.malformed())?
                        .to_owned();
                    if text.len().saturating_add(decoded.len()) > max_bytes {
                        return Err(self.exceeded(
                            "text_bytes",
                            text.len().saturating_add(decoded.len()) as u64,
                            max_bytes as u64,
                        ));
                    }
                    text.push_str(&decoded);
                }
                Event::End(_) => return Ok(text),
                Event::Start(_) => return Err(self.malformed()),
                Event::Eof => return Err(self.malformed()),
                _ => {}
            }
        }
    }
}

/// Reads the children of the element the cursor has just entered.
///
/// `visit` is called with the cursor, each child's start tag, and whether the
/// source wrote it self-closing. It returns whether it **consumed** the child's
/// subtree — by calling [`children`] or [`Cursor::read_text`] on it. A `false`
/// return (or any `Empty` child) leaves the skipping to this function, so a
/// reader that understands three of a parent's twenty children cannot
/// accidentally interpret the other seventeen as its own siblings.
///
/// That `self_closing` flag is not bookkeeping: `<a:effectLst/>` means "no
/// effects" and `<a:effectLst>…</a:effectLst>` means a lost shadow, and only the
/// element can tell them apart (`casual-doc-import`'s no-op classification makes
/// the same distinction).
///
/// # Complexity
///
/// O(part), once, with no backtracking: every event is read exactly once whether
/// it is modelled or skipped.
pub(crate) fn children<'a, F>(cursor: &mut Cursor<'a>, mut visit: F) -> Result<(), ImportError>
where
    F: FnMut(&mut Cursor<'a>, &BytesStart<'a>, bool) -> Result<bool, ImportError>,
{
    // The document element's children, when it was written self-closing: there
    // are none, and the next event is the end of the part. This is `enter`'s
    // guard applied at the one level a caller cannot apply it, because the root's
    // own start tag is read by `Cursor::root`.
    if cursor.depth == 0 && cursor.root_empty {
        return Ok(());
    }
    cursor.push_depth()?;
    loop {
        match cursor.event()? {
            Event::Start(element) => {
                if !visit(cursor, &element, false)? {
                    cursor.skip_subtree()?;
                }
            }
            Event::Empty(element) => {
                visit(cursor, &element, true)?;
            }
            Event::End(_) => {
                cursor.depth = cursor.depth.saturating_sub(1);
                return Ok(());
            }
            Event::Eof => {
                return Err(ImportError::MalformedPartXml {
                    part: cursor.part.to_owned(),
                });
            }
            _ => {}
        }
    }
}

/// Reads a child's children when it has any, and reports back whether its
/// subtree was consumed — the whole contract of [`children`]'s callback in one
/// call.
///
/// # Why this exists
///
/// Calling [`children`] on an element the source wrote **self-closing** is a
/// silent, destructive bug: there is no matching end tag, so the call consumes
/// the following *sibling's* events as if they were the empty element's
/// children. It is destructive rather than noisy because the events it eats are
/// well-formed, so the reader keeps going and the shape that got eaten simply
/// disappears.
///
/// This happened during development. `<a:prstGeom prst="rect"><a:avLst/></…>` is
/// what PowerPoint writes for every unadjusted preset, and reading `a:avLst`'s
/// children without checking the flag swallowed the `a:solidFill` after it.
/// Making the guarded form the SHORT form is the fix: a call site that uses this
/// cannot forget, where one that writes `children(…)` and `Ok(true)` by hand can.
pub(crate) fn enter<'a, F>(
    cursor: &mut Cursor<'a>,
    empty: bool,
    visit: F,
) -> Result<bool, ImportError>
where
    F: FnMut(&mut Cursor<'a>, &BytesStart<'a>, bool) -> Result<bool, ImportError>,
{
    if empty {
        return Ok(false);
    }
    children(cursor, visit)?;
    Ok(true)
}

/// An element's local name, with any namespace prefix removed.
pub(crate) fn local_name<'a>(element: &'a BytesStart<'_>) -> &'a [u8] {
    let name = element.name();
    let raw = name.into_inner();
    match raw.iter().rposition(|byte| *byte == b':') {
        Some(colon) => &raw[colon + 1..],
        None => raw,
    }
}

/// One attribute's value by local name, unescaped.
///
/// Unescaping matters even here: a shape name is author-supplied text, so
/// `name="Q1 &amp; Q2"` is one shape called `Q1 & Q2` and not a producer bug.
/// Malformed UTF-8 or a bad entity fails the part closed rather than yielding a
/// lossy value, because a silently mangled relationship id resolves to the wrong
/// part.
pub(crate) fn attribute(
    element: &BytesStart<'_>,
    name: &[u8],
    part: &str,
) -> Result<Option<String>, ImportError> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| ImportError::MalformedPartXml {
            part: part.to_owned(),
        })?;
        if attribute.key.local_name().as_ref() != name {
            continue;
        }
        let raw = core::str::from_utf8(attribute.value.as_ref()).map_err(|_| {
            ImportError::MalformedPartXml {
                part: part.to_owned(),
            }
        })?;
        let value =
            quick_xml::escape::unescape(raw).map_err(|_| ImportError::MalformedPartXml {
                part: part.to_owned(),
            })?;
        return Ok(Some(value.into_owned()));
    }
    Ok(None)
}

/// An attribute parsed as a signed EMU-or-other integer.
///
/// `None` for an absent attribute and for one whose text is not an integer. The
/// two are deliberately indistinguishable to the caller *for the value*, and
/// distinguishable for reporting: a caller that needs to report an unparsable
/// value checks [`attribute`] first. Nothing here clamps — a value outside the
/// model's domain is refused by the model's own validation, which is the one
/// place that knows the domain.
pub(crate) fn integer_attribute(
    element: &BytesStart<'_>,
    name: &[u8],
    part: &str,
) -> Result<Option<i64>, ImportError> {
    Ok(attribute(element, name, part)?.and_then(|value| value.trim().parse::<i64>().ok()))
}

/// An attribute parsed as an OOXML `xsd:boolean`, which admits `1`/`0` as well as
/// `true`/`false`.
///
/// Reading only `1`/`0` is a real and common importer defect: Word and PowerPoint
/// both write `b="1"`, but a hand-authored or third-party package writes
/// `b="true"`, and a reader that understands one spelling silently loses the bold.
pub(crate) fn boolean_attribute(
    element: &BytesStart<'_>,
    name: &[u8],
    part: &str,
) -> Result<Option<bool>, ImportError> {
    Ok(
        attribute(element, name, part)?.and_then(|value| match value.trim() {
            "1" | "true" => Some(true),
            "0" | "false" => Some(false),
            _ => None,
        }),
    )
}
