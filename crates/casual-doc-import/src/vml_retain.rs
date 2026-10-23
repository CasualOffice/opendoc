//! Preparing a `w:pict`'s source to be carried verbatim beside its DrawingML
//! projection (`docs/08` ADR-072, `docs/109` HF-395).
//!
//! The source must survive being written into a DIFFERENT document: the one
//! the semantic writer regenerates, whose root declares its own prefixes and
//! whose text-box content, styles and relationships are renumbered. Two things
//! make that safe, and both are decided here, once, at import:
//!
//! 1. **Self-contained namespaces.** Every prefix the fragment uses and does
//!    not declare itself is declared on the `w:pict` start tag, bound to the
//!    URI it had in the source. The fragment then means the same thing under
//!    any root.
//! 2. **Text-box content is a hole, not bytes.** The fragment is split at each
//!    outermost `w:txbxContent`'s content, and the writer fills each hole with
//!    the text box's current blocks — the only form in which the style,
//!    numbering, bookmark and relationship ids inside it agree with the rest
//!    of the written package.
//!
//! Every relationship id left in the fragment (outside the holes) is returned,
//! so the caller can refuse to retain a pict that names a relationship the
//! regenerated part would not declare.
//!
//! O(bytes) in the fragment.

use std::collections::BTreeSet;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// VML (`v:`).
const VML_NS: &[u8] = b"urn:schemas-microsoft-com:vml";
/// VML's Office extensions (`o:`).
const OFFICE_NS: &[u8] = b"urn:schemas-microsoft-com:office:office";
/// VML's Word extensions (`w10:`).
const WORD_VML_NS: &[u8] = b"urn:schemas-microsoft-com:office:word";
/// Office Open XML relationships (`r:`).
const RELATIONSHIPS_NS: &[u8] =
    b"http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// Whether `uri` is one of the three VML vocabularies.
#[must_use]
pub(crate) fn is_vml_namespace(uri: &[u8]) -> bool {
    uri == VML_NS || uri == OFFICE_NS || uri == WORD_VML_NS
}

/// A pict source ready to be stored: its pieces around the text-box holes, and
/// the relationship ids it names outside them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedSource {
    /// `holes + 1` pieces.
    pub pieces: Vec<String>,
    /// Every relationship id named outside the holes.
    pub relationship_ids: Vec<String>,
}

/// Prepares `fragment` (one `w:pict` element, as captured) for retention.
/// `resolve` answers the URI an undeclared prefix had in the source.
///
/// `None` when the fragment is not one well-formed element, or uses a prefix
/// that resolves nowhere — it then cannot be written back safely, and the
/// caller keeps the projection only.
#[must_use]
pub(crate) fn prepare_source(
    fragment: &str,
    resolve: impl Fn(&[u8]) -> Option<Vec<u8>>,
) -> Option<PreparedSource> {
    let bytes = fragment.as_bytes();
    let mut reader = Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    // Prefixes declared inside the fragment, and those used by it.
    let mut declared: BTreeSet<Vec<u8>> = BTreeSet::new();
    let mut used: BTreeSet<Vec<u8>> = BTreeSet::new();
    // (start, end) byte ranges of each outermost `w:txbxContent`'s content.
    let mut holes: Vec<(usize, usize)> = Vec::new();
    let mut txbx_depth = 0_usize;
    let mut hole_start = 0_usize;
    let mut relationship_ids = Vec::new();
    // The end of the `w:pict` start tag, where declarations are inserted.
    let mut root_tag_end: Option<usize> = None;
    let mut depth = 0_usize;
    loop {
        let before = usize::try_from(reader.buffer_position()).ok()?;
        let event = reader.read_event_into(&mut buffer).ok()?;
        let after = usize::try_from(reader.buffer_position()).ok()?;
        match &event {
            Event::Eof => break,
            Event::Start(element) | Event::Empty(element) => {
                let is_start = matches!(event, Event::Start(_));
                if depth == 0 && root_tag_end.is_none() {
                    // `after` is one past the tag's `>` (or `/>`).
                    root_tag_end = Some(after - if is_start { 1 } else { 2 });
                }
                note_names(element, &mut declared, &mut used);
                let is_txbx = element.local_name().as_ref() == b"txbxContent";
                if txbx_depth == 0 {
                    note_relationships(element, &resolve, &mut relationship_ids);
                }
                if is_start {
                    depth += 1;
                    if is_txbx {
                        if txbx_depth == 0 {
                            hole_start = after;
                        }
                        txbx_depth += 1;
                    }
                }
            }
            Event::End(element) => {
                depth = depth.checked_sub(1)?;
                if element.local_name().as_ref() == b"txbxContent" && txbx_depth > 0 {
                    txbx_depth -= 1;
                    if txbx_depth == 0 {
                        holes.push((hole_start, before));
                    }
                }
            }
            _ => {}
        }
        buffer.clear();
    }
    if depth != 0 || txbx_depth != 0 {
        return None;
    }
    let root_tag_end = root_tag_end?;
    let mut declarations = String::new();
    for prefix in used.difference(&declared) {
        if prefix.as_slice() == b"xml" {
            continue;
        }
        let uri = resolve(prefix)?;
        declarations.push_str(&format!(
            " xmlns:{}=\"{}\"",
            std::str::from_utf8(prefix).ok()?,
            quick_xml::escape::escape(std::str::from_utf8(&uri).ok()?)
        ));
    }
    let mut pieces = Vec::with_capacity(holes.len() + 1);
    let mut cursor = 0;
    for (start, end) in &holes {
        pieces.push(fragment.get(cursor..*start)?.to_owned());
        cursor = *end;
    }
    pieces.push(fragment.get(cursor..)?.to_owned());
    // The declarations go into the first piece, which holds the root tag (a
    // hole can only follow it).
    pieces[0].insert_str(root_tag_end, &declarations);
    Some(PreparedSource {
        pieces,
        relationship_ids,
    })
}

/// Records the prefixes an element declares and the prefixes it uses (its own
/// and its attributes').
fn note_names(
    element: &BytesStart<'_>,
    declared: &mut BTreeSet<Vec<u8>>,
    used: &mut BTreeSet<Vec<u8>>,
) {
    if let Some(prefix) = element.name().prefix() {
        used.insert(prefix.as_ref().to_vec());
    }
    for attribute in element.attributes().flatten() {
        let key = attribute.key;
        match key.prefix().map(|prefix| prefix.into_inner()) {
            Some(b"xmlns") => {
                declared.insert(key.local_name().as_ref().to_vec());
            }
            Some(prefix) => {
                used.insert(prefix.to_vec());
            }
            None => {}
        }
    }
}

/// Collects the values of the element's relationship-namespace attributes
/// (`r:id`, `r:pict`, `r:href`, …) and of `o:relid`, which names a
/// relationship without being in that namespace.
fn note_relationships(
    element: &BytesStart<'_>,
    resolve: &impl Fn(&[u8]) -> Option<Vec<u8>>,
    out: &mut Vec<String>,
) {
    // A prefix declared on this very element shadows the outer one; the VML a
    // `w:pict` carries never rebinds `r`, and a fragment that did would only
    // make this check stricter.
    for attribute in element.attributes().flatten() {
        let key = attribute.key;
        let Some(prefix) = key.prefix().map(|prefix| prefix.into_inner()) else {
            continue;
        };
        if prefix == b"xmlns" {
            continue;
        }
        let Some(uri) = resolve(prefix) else {
            continue;
        };
        let names_relationship = uri.as_slice() == RELATIONSHIPS_NS
            || (uri.as_slice() == OFFICE_NS && key.local_name().as_ref() == b"relid");
        if names_relationship && let Ok(value) = std::str::from_utf8(attribute.value.as_ref()) {
            out.push(value.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(prefix: &[u8]) -> Option<Vec<u8>> {
        Some(match prefix {
            b"w" => b"http://schemas.openxmlformats.org/wordprocessingml/2006/main".to_vec(),
            b"v" => VML_NS.to_vec(),
            b"o" => OFFICE_NS.to_vec(),
            b"w10" => WORD_VML_NS.to_vec(),
            b"r" => RELATIONSHIPS_NS.to_vec(),
            _ => return None,
        })
    }

    #[test]
    fn declarations_are_added_and_text_box_content_is_a_hole() {
        let fragment = r#"<w:pict><v:shape id="a" o:spid="x"><v:imagedata r:id="rId4"/><v:textbox><w:txbxContent><w:p><w:r><w:t>hi</w:t></w:r></w:p></w:txbxContent></v:textbox><w10:wrap type="none"/></v:shape></w:pict>"#;
        let prepared = prepare_source(fragment, resolve).expect("prepares");
        assert_eq!(prepared.pieces.len(), 2);
        assert!(prepared.pieces[0].starts_with("<w:pict xmlns:"));
        for prefix in ["o", "r", "v", "w", "w10"] {
            assert!(
                prepared.pieces[0].contains(&format!("xmlns:{prefix}=")),
                "{prefix} must be declared: {}",
                prepared.pieces[0]
            );
        }
        assert!(prepared.pieces[0].ends_with("<w:txbxContent>"));
        assert!(prepared.pieces[1].starts_with("</w:txbxContent>"));
        assert_eq!(prepared.relationship_ids, vec!["rId4".to_owned()]);
    }

    #[test]
    fn a_relationship_inside_text_box_content_is_not_the_picts_to_resolve() {
        let fragment = r#"<w:pict><v:shape><v:textbox><w:txbxContent><w:p><w:hyperlink r:id="rId9"/></w:p></w:txbxContent></v:textbox></v:shape></w:pict>"#;
        let prepared = prepare_source(fragment, resolve).expect("prepares");
        assert!(prepared.relationship_ids.is_empty());
    }

    #[test]
    fn an_unresolvable_prefix_refuses() {
        assert!(prepare_source(r#"<w:pict><zz:thing/></w:pict>"#, resolve).is_none());
    }
}
