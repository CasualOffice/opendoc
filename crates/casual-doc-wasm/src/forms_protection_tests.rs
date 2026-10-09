// SPDX-License-Identifier: Apache-2.0

//! Forms protection is per SECTION (`docs/165` M6, `docs/109` FID-AT-06):
//! ECMA-376 §17.18.29 protects "form fields in sections where the `formProt`
//! element has a value of true [and puts] no restrictions in sections where
//! `formProt` is false", and a section that states nothing is protected. A
//! section break copies the split section's value, as Word copies `w:sectPr`.

use casual_doc_edit::{Operation, Pos};
use casual_doc_model::v1::SectionId;

use casual_doc_edit::find_paragraph_any;
use casual_doc_layout::flow::node_plain_text;

use crate::{WasmDocument, open_document};

const PROTECTED: &[u8] = include_bytes!("../../../fixtures/generated/forms-protected.docx");

/// The body paragraph whose text contains `needle`.
fn paragraph_with(d: &WasmDocument, needle: &str) -> casual_doc_model::NodeId {
    d.ordered_paragraphs()
        .into_iter()
        .map(|(id, _)| id)
        .find(|id| {
            find_paragraph_any(&d.document, *id)
                .is_some_and(|paragraph| node_plain_text(&paragraph.inlines).contains(needle))
        })
        .unwrap_or_else(|| panic!("the fixture has a paragraph with {needle:?}"))
}

fn sections(d: &WasmDocument) -> Vec<SectionId> {
    d.document
        .definitions()
        .sections
        .iter()
        .map(|boundary| boundary.id)
        .collect()
}

fn type_at(d: &mut WasmDocument, paragraph: casual_doc_model::NodeId) -> Result<(), String> {
    d.apply(Operation::InsertText {
        at: Pos::new(paragraph, 0),
        text: "QZX".to_owned(),
    })
    .map(|_| ())
}

/// The fixture is forms-protected with one section; lift the protection, break
/// the section before "End of form.", and hand back the two sections.
fn two_sections() -> (WasmDocument, SectionId, SectionId) {
    let mut d = open_document(PROTECTED).expect("open the forms fixture");
    d.set_document_protection_inner(None, false, false)
        .expect("lift the protection to author the sections");
    let end = paragraph_with(&d, "End of form");
    d.insert_section_break_inner(&end.to_string(), 0, "continuous")
        .expect("a section break before the last paragraph");
    let ids = sections(&d);
    assert_eq!(ids.len(), 2, "two sections now");
    (d, ids[0], ids[1])
}

#[test]
fn forms_protection_leaves_a_section_that_says_formprot_false_editable_and_no_other() {
    let (mut d, first, second) = two_sections();
    // The second section (the one "End of form." is in) states `false`; the
    // first states nothing, which is NOT the same statement.
    d.apply(Operation::SetSectionFormProtection {
        section: second,
        protected: Some(false),
    })
    .expect("open the second section");
    assert_eq!(
        d.document.definitions().section_form_protection(first),
        None
    );
    d.set_document_protection_inner(Some("forms"), true, false)
        .expect("protect the forms again");

    let open = paragraph_with(&d, "End of form");
    type_at(&mut d, open).expect("a section whose formProt is false has no restriction");

    let protected = paragraph_with(&d, "Tick one");
    let refusal =
        type_at(&mut d, protected).expect_err("a section that states nothing is protected");
    assert!(refusal.contains("protected"), "and it says so: {refusal}");

    // `true` protects as absent does.
    d.set_document_protection_inner(None, false, false).unwrap();
    d.apply(Operation::SetSectionFormProtection {
        section: second,
        protected: Some(true),
    })
    .unwrap();
    d.set_document_protection_inner(Some("forms"), true, false)
        .unwrap();
    type_at(&mut d, open).expect_err("formProt true protects");
}

#[test]
fn a_section_break_carries_the_split_sections_formprot_and_its_undo_takes_it_back() {
    let mut d = open_document(PROTECTED).expect("open the forms fixture");
    d.set_document_protection_inner(None, false, false).unwrap();
    let only = sections(&d)[0];
    d.apply(Operation::SetSectionFormProtection {
        section: only,
        protected: Some(false),
    })
    .unwrap();
    let digest = d.content_digest();

    let end = paragraph_with(&d, "End of form");
    d.insert_section_break_inner(&end.to_string(), 0, "continuous")
        .expect("break the open section");
    let ids = sections(&d);
    assert_eq!(ids.len(), 2);
    for section in &ids {
        assert_eq!(
            d.document.definitions().section_form_protection(*section),
            Some(false),
            "both halves of an open section are open, as Word copies w:sectPr"
        );
    }

    d.undo_inner().expect("undo the break");
    assert_eq!(sections(&d), vec![only]);
    assert_eq!(
        d.document.definitions().form_protection.len(),
        1,
        "the new section's entry went with it"
    );
    assert_eq!(
        d.content_digest(),
        digest,
        "the undo restores the document exactly"
    );
}
