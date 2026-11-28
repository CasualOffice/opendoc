// SPDX-License-Identifier: Apache-2.0

//! A picture, a text box or an equation on a story other than the main document is
//! written with every prefix declared.
//!
//! # What was broken, measured before the fix
//!
//! Every semantic save rewrites `header*.xml`, `footer*.xml`, `footnotes.xml`,
//! `endnotes.xml` and `comments.xml` from the model, through the same `write_block`
//! the main document uses. `document_xml` declared every prefix that writer can emit
//! (`wp a pic wps wpg w14 m c dgm v o`), but the other four roots declared only the
//! few their authors had needed (`w r`, `w14`, the fold namespaces). A header or
//! footer that held a picture therefore came out with `wp:inline`, `a:graphic` and
//! `pic:pic` unbound: not well-formed XML, which Word answers with its "unreadable
//! content" recovery prompt (dropping the logo) and LibreOffice does not open at all.
//!
//! Nothing noticed because the one guard that walks every written part with a
//! namespace-aware reader, `collapsed_heading_state.rs::
//! no_written_part_uses_an_undeclared_prefix`, ran over a fixture with no picture
//! anywhere but the body.
//!
//! # What these guards assert
//!
//! * the package written from [`common::story_pictures::story_pictures_source`] has no part that
//!   uses an unbound prefix (the shared guard, now also run by the guard's home file
//!   over the same package);
//! * the drawings really are in every story, so the walk above is not vacuous;
//! * the six story roots declare one identical set of prefixes, because the fix is
//!   one declaration site (`declare_story_namespaces`) and a root that drifts from
//!   the others is the next instance of this bug.
//!
//! # Complexity
//!
//! `O(package bytes)` per test. Test-time only.

mod common;

use std::collections::BTreeSet;

use quick_xml::events::Event;

/// The prefixes `write_block` can emit, which every story root must bind.
const EXPECTED_PREFIXES: [&str; 15] = [
    "w", "r", "wp", "a", "pic", "wps", "wpg", "w14", "m", "c", "dgm", "v", "o", "w15", "mc",
];

/// The text of one part of the written package.
fn part_text(package: &[u8], part: &str) -> String {
    let parts = common::xml_parts(package);
    let bytes = parts
        .get(part)
        .unwrap_or_else(|| panic!("the written package has no `{part}`"));
    String::from_utf8(bytes.clone()).expect("a written part is UTF-8")
}

/// The prefixes bound by `xmlns:*` attributes on a part's root element.
fn root_prefixes(package: &[u8], part: &str) -> BTreeSet<String> {
    let parts = common::xml_parts(package);
    let mut reader = quick_xml::Reader::from_reader(parts[part].as_slice());
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(root)) => {
                return root
                    .attributes()
                    .map(|attribute| {
                        String::from_utf8(attribute.expect("a valid attribute").key.as_ref().into())
                            .expect("an attribute name is UTF-8")
                    })
                    .filter_map(|name| name.strip_prefix("xmlns:").map(str::to_owned))
                    .collect();
            }
            Ok(Event::Eof) => panic!("`{part}` has no root element"),
            Ok(_) => {}
            Err(error) => panic!("`{part}` must be well-formed XML: {error}"),
        }
        buffer.clear();
    }
}

/// The load-bearing guard: a picture (inline and anchored), a text box, and the same
/// on a footnote, an endnote and a comment, are written with every prefix declared.
#[test]
fn pictures_and_text_boxes_on_every_story_are_written_with_declared_prefixes() {
    let written = common::story_pictures::write_story_pictures_package();

    // The drawings must be there, or "no unbound prefix" proves nothing. Counts are
    // per part and exact: header = inline + anchored picture + text box, footer =
    // inline + anchored picture, each note and the comment = one inline picture.
    for (part, inline, anchor, pictures, text_boxes) in [
        ("word/header1.xml", 1, 2, 2, 1),
        ("word/footer1.xml", 1, 1, 2, 0),
        ("word/footnotes.xml", 1, 0, 1, 0),
        ("word/endnotes.xml", 1, 0, 1, 0),
        ("word/comments.xml", 1, 0, 1, 0),
    ] {
        let text = part_text(&written, part);
        assert_eq!(text.matches("<wp:inline").count(), inline, "{part}: {text}");
        assert_eq!(text.matches("<wp:anchor").count(), anchor, "{part}: {text}");
        assert_eq!(text.matches("<pic:pic").count(), pictures, "{part}: {text}");
        assert_eq!(
            text.matches("<wps:txbx").count(),
            text_boxes,
            "{part}: {text}"
        );
    }

    let checked = common::assert_no_part_uses_an_undeclared_prefix("story-pictures", &written);
    assert!(
        checked > 0,
        "the walk found no elements, so it proved nothing"
    );
}

/// Every story root binds the same prefixes, and all of those `write_block` can emit.
///
/// Stated as an invariant of the roots rather than of the pictures: the fix is one
/// declaration site, so the property worth pinning is that no root is declared by
/// hand again. It also covers the prefixes no picture here uses (`wpg`, `m`, `c`,
/// `dgm`, `v`, `o`: a group, an equation, a chart or diagram, an OLE preview or a
/// watermark in a running part), which would otherwise wait for their own failure.
#[test]
fn every_story_root_declares_the_same_prefixes() {
    let written = common::story_pictures::write_story_pictures_package();
    let expected: BTreeSet<String> = EXPECTED_PREFIXES.iter().map(|p| (*p).to_owned()).collect();
    for (part, root) in common::story_pictures::STORY_PARTS {
        let declared = root_prefixes(&written, part);
        assert_eq!(
            declared, expected,
            "`{root}` in `{part}` must declare exactly the prefixes the shared body writer \
             can emit"
        );
    }
}
