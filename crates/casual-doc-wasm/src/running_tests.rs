// SPDX-License-Identifier: Apache-2.0

//! The guards `docs/129` §6 says Link to Previous must arrive with, plus Remove
//! and the page-number positions (`docs/109` HF-192, RM-23).
//!
//! Every guard is about the BODY a page resolves to and the bytes a save writes,
//! never about a host flag reading back what was written — `docs/129` §6 is
//! explicit that `headerLinked === false` proves nothing. The fixture is the
//! committed `sample.docx`: three sections, each declaring its own default
//! header and footer, every footer carrying a `PAGE` field (the provenance
//! `docs/129` §6.4 prefers: what Word itself writes).

use casual_doc_edit::{Operation, Pos, RunningRegion};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{Alignment, BlockNode, HeaderFooterId, HeaderFooterKind, InlineNode};

use crate::{DocxPackage, WasmDocument, open_document, viewer_limits};
use casual_doc_command::sections::PageNumberText;

const SAMPLE_DOCX: &[u8] = include_bytes!("../../../sample.docx");
const DOCX: &str = casual_doc_io::formats::DOCX;

/// A page of section `index` (0-based) that applies the DEFAULT variant.
fn page_in_section(d: &WasmDocument, index: usize) -> u32 {
    (1..=d.page_count())
        .find(|page| {
            d.running_section(*page).map(|(at, _)| at) == Some(index)
                && d.running_kind_for_page(*page, index) == HeaderFooterKind::Default
        })
        .expect("sample.docx has a default-variant page in this section")
}

/// The body section `index` itself declares for `region`'s default variant.
fn own(d: &WasmDocument, index: usize, region: RunningRegion) -> Option<HeaderFooterId> {
    let section = &d.document.definitions().sections[index];
    let refs = match region {
        RunningRegion::Header => &section.headers,
        RunningRegion::Footer => &section.footers,
    };
    refs.iter()
        .find(|reference| reference.kind == HeaderFooterKind::Default)
        .map(|reference| reference.reference)
}

fn blocks(d: &WasmDocument, region: RunningRegion, id: HeaderFooterId) -> Vec<BlockNode> {
    let definitions = d.document.definitions();
    match region {
        RunningRegion::Header => definitions.headers.get(&id),
        RunningRegion::Footer => definitions.footers.get(&id),
    }
    .expect("the body exists")
    .blocks
    .clone()
}

/// `(paragraph id, text)` of every paragraph, in order.
fn paragraphs(blocks: &[BlockNode]) -> Vec<(NodeId, String)> {
    let mut out = Vec::new();
    casual_doc_review::body::collect_block_text(blocks, &mut out);
    out
}

fn text(blocks: &[BlockNode]) -> Vec<String> {
    paragraphs(blocks)
        .into_iter()
        .map(|(_, text)| text)
        .collect()
}

fn ids(blocks: &[BlockNode]) -> Vec<NodeId> {
    paragraphs(blocks).into_iter().map(|(id, _)| id).collect()
}

/// `PAGE` fields anywhere in `blocks` (not `NUMPAGES`).
fn page_fields(blocks: &[BlockNode]) -> usize {
    fn in_inlines(inlines: &[InlineNode]) -> usize {
        inlines
            .iter()
            .map(|inline| match inline {
                InlineNode::Field(field) => {
                    usize::from(field.instruction.split_whitespace().next() == Some("PAGE"))
                }
                InlineNode::Hyperlink(link) => in_inlines(&link.inlines),
                _ => 0,
            })
            .sum()
    }
    blocks
        .iter()
        .map(|block| match block {
            BlockNode::Paragraph(paragraph) => in_inlines(&paragraph.inlines),
            BlockNode::Table(table) => table
                .rows
                .iter()
                .flat_map(|row| row.cells.iter())
                .map(|cell| page_fields(&cell.blocks))
                .sum(),
            BlockNode::Sdt(sdt) => page_fields(&sdt.blocks),
            BlockNode::AltChunk(_) => 0,
        })
        .sum()
}

/// The ids of the body the PAGE resolves its band to — what the editor puts a
/// caret in and what the layout paints there (`running_story_matches_the_painted_band`
/// holds those two to each other).
fn shown_ids(d: &WasmDocument, region: RunningRegion, page: u32) -> Vec<NodeId> {
    ids(d
        .running_blocks_of(region, page)
        .expect("the page shows a band"))
}

fn footer_parts(d: &mut WasmDocument) -> usize {
    let bytes = d
        .export_as_inner(DOCX, "preserve_when_safe")
        .expect("the save exports")
        .bytes;
    let package = DocxPackage::open(&bytes, viewer_limits()).expect("open package");
    package
        .entries()
        .iter()
        .filter(|entry| entry.part_name.contains("footer") && !entry.part_name.contains("_rels"))
        .count()
}

/// Section 2 of `sample.docx`, re-linked so it inherits section 1's footer —
/// the state Link to Previous OFF starts from.
fn linked_sample() -> (WasmDocument, u32) {
    let mut d = open_document(SAMPLE_DOCX).expect("open sample.docx");
    let page = page_in_section(&d, 1);
    d.set_running_linked_inner("footer", page, true, "", 0)
        .expect("re-link section 2's footer");
    assert!(
        own(&d, 1, RunningRegion::Footer).is_none(),
        "it inherits now"
    );
    (d, page)
}

/// `docs/129` §6.1 — the guard that must be written first. Unlinking copies the
/// inherited footer INCLUDING its page number, and the page shows the copy.
///
/// Mutation: replace `clone_blocks_with_fresh_ids` in
/// `sections::set_linked_to_previous` with one empty paragraph (or route the copy
/// through the clipboard's old `sanitize_inlines`): the copy then has no `PAGE`
/// field and the first assertion on `page_fields` fails.
#[test]
fn unlinking_copies_the_inherited_footer_with_its_page_number() {
    let (mut d, page) = linked_sample();
    let inherited = own(&d, 0, RunningRegion::Footer).expect("section 1 owns a footer");
    let source = blocks(&d, RunningRegion::Footer, inherited);
    assert!(
        page_fields(&source) > 0,
        "sample.docx's footer carries a PAGE field"
    );
    assert_eq!(shown_ids(&d, RunningRegion::Footer, page), ids(&source));

    let result = d
        .set_running_linked_inner("footer", page, false, "", 0)
        .expect("unlink");
    let copy_id = own(&d, 1, RunningRegion::Footer).expect("section 2 owns a footer again");
    assert_ne!(
        copy_id, inherited,
        "a new body, not the inherited one re-pointed"
    );
    let copy = blocks(&d, RunningRegion::Footer, copy_id);
    assert_eq!(
        page_fields(&copy),
        page_fields(&source),
        "the copy keeps the page number"
    );
    assert_eq!(
        text(&copy),
        text(&source),
        "and reads exactly as the band did"
    );
    assert!(
        ids(&copy).iter().all(|id| !ids(&source).contains(id)),
        "every paragraph id is re-minted"
    );
    assert_eq!(
        shown_ids(&d, RunningRegion::Footer, page),
        ids(&copy),
        "the page shows the copy"
    );
    assert!(
        result.paste_loss.is_empty(),
        "nothing was lost: {:?}",
        result.paste_loss
    );
}

/// `docs/129` §6.3 — the `docs/104` T-01 class. Typing into the unlinked copy
/// leaves the previous section's footer exactly as it was.
///
/// Mutation: make the unlink point section 2 at the INHERITED body
/// (`reference: Some(inherited)`) instead of the copy: the typed text then lands
/// in section 1's footer and the final assertion fails.
#[test]
fn editing_the_unlinked_footer_does_not_touch_the_previous_section() {
    let (mut d, page) = linked_sample();
    let inherited = own(&d, 0, RunningRegion::Footer).expect("section 1 owns a footer");
    let before = text(&blocks(&d, RunningRegion::Footer, inherited));
    d.set_running_linked_inner("footer", page, false, "", 0)
        .expect("unlink");
    let copy_id = own(&d, 1, RunningRegion::Footer).expect("its own footer");
    let first = ids(&blocks(&d, RunningRegion::Footer, copy_id))[0];
    d.apply(Operation::InsertText {
        at: Pos::new(first, 0),
        text: "SECTION TWO ".to_owned(),
    })
    .expect("type into the copy");
    assert!(text(&blocks(&d, RunningRegion::Footer, copy_id))[0].starts_with("SECTION TWO "));
    assert_eq!(
        text(&blocks(&d, RunningRegion::Footer, inherited)),
        before,
        "section 1's footer is untouched"
    );
}

/// `docs/129` §6.5 — one press of Undo reverses the whole unlink: the create and
/// the re-point are one action.
///
/// Mutation: apply the two operations of the unlink as two separate actions
/// (two `apply_action_caret_as` calls): one undo then leaves an orphan body and
/// section 2 still pointing at it, and the assertion fails.
#[test]
fn one_undo_reverses_an_unlink() {
    let (mut d, page) = linked_sample();
    let revision = d.revision;
    d.set_running_linked_inner("footer", page, false, "", 0)
        .expect("unlink");
    assert_eq!(d.revision, revision + 1, "one edit");
    d.undo_inner().expect("undo");
    assert!(
        own(&d, 1, RunningRegion::Footer).is_none(),
        "linked again after one undo"
    );
}

/// `docs/129` §6.4 and §6.6 — the round trip writes section 2's own
/// `w:footerReference`, and an unlink/re-link cycle does not grow the package.
///
/// Mutation: drop the exporter's reachability pass (`referenced_running_bodies`
/// in `casual-doc-export/src/semantic.rs`): the cycle's orphaned copy is then
/// written as an extra footer part and the count assertion fails.
#[test]
fn an_unlinked_footer_round_trips_and_a_link_cycle_does_not_grow_the_file() {
    let (mut d, page) = linked_sample();
    let parts_linked = footer_parts(&mut d);
    d.set_running_linked_inner("footer", page, false, "", 0)
        .expect("unlink");
    let bytes = d
        .export_as_inner(DOCX, "preserve_when_safe")
        .expect("export")
        .bytes;
    let reopened = open_document(&bytes).expect("reopen");
    let copy_id = own(&reopened, 1, RunningRegion::Footer).expect("section 2 declares its own");
    assert!(
        own(&reopened, 0, RunningRegion::Footer).is_some(),
        "section 1 still does"
    );
    assert!(page_fields(&blocks(&reopened, RunningRegion::Footer, copy_id)) > 0);

    d.set_running_linked_inner("footer", page, true, "", 0)
        .expect("re-link");
    assert_eq!(
        footer_parts(&mut d),
        parts_linked,
        "the cycle left the package as it was"
    );
}

/// The first section has nothing to link to, and says so rather than doing
/// nothing; asking for the state a band is already in is UNCHANGED.
///
/// Mutation: drop the `None` arm's refusal in `set_linked_to_previous` (treat
/// the first section as unlinked): the first `is_err` assertion fails.
#[test]
fn the_first_section_refuses_and_a_no_op_is_reported_unchanged() {
    let mut d = open_document(SAMPLE_DOCX).expect("open");
    let first = page_in_section(&d, 0);
    assert!(
        d.set_running_linked_inner("header", first, true, "", 0)
            .is_err()
    );
    let state: serde_json::Value =
        serde_json::from_str(&d.running_link_state("header", first)).expect("json");
    assert!(state["linked"].is_null(), "no previous section: {state}");

    let second = page_in_section(&d, 1);
    let revision = d.revision;
    let result = d
        .set_running_linked_inner("header", second, false, "", 0)
        .expect("already unlinked");
    assert_eq!(
        result.revision, revision,
        "no edit for the state it is already in"
    );
    let state: serde_json::Value =
        serde_json::from_str(&d.running_link_state("header", second)).expect("json");
    assert_eq!(state["linked"], false);
    assert_eq!(state["kind"], "default");
    assert_eq!(state["section"], 2);
}

/// A caret in the band follows an unlink into the copy, at the same offset, so
/// the person keeps typing where they were — in the band the page now shows.
///
/// Mutation: return the host's caret unchanged from `set_running_linked_inner`:
/// it stays in section 1's body and the assertion fails.
#[test]
fn the_caret_follows_an_unlink_into_the_copy() {
    let (mut d, page) = linked_sample();
    let inherited = own(&d, 0, RunningRegion::Footer).expect("section 1 owns a footer");
    let source_first = ids(&blocks(&d, RunningRegion::Footer, inherited))[0];
    let result = d
        .set_running_linked_inner("footer", page, false, &source_first.to_string(), 3)
        .expect("unlink");
    let copy_id = own(&d, 1, RunningRegion::Footer).expect("its own footer");
    let copy_first = ids(&blocks(&d, RunningRegion::Footer, copy_id))[0];
    assert_eq!(result.node, copy_first.to_string());
    assert_eq!(result.offset, 3);
}

/// Remove takes the band off THIS section's pages and leaves the earlier
/// section's alone — the opposite of ONLYOFFICE, which empties a later section's
/// body in place and so empties every earlier section linked to it.
///
/// Mutation: lower Remove as `SetSectionRunningRef { None }` always: section 3
/// then inherits section 2's footer, its page still shows a page number, and the
/// `page_fields == 0` assertion fails.
#[test]
fn removing_a_later_sections_footer_leaves_the_earlier_ones() {
    let mut d = open_document(SAMPLE_DOCX).expect("open");
    let page = page_in_section(&d, 2);
    let earlier = own(&d, 1, RunningRegion::Footer).expect("section 2's footer");
    let earlier_text = text(&blocks(&d, RunningRegion::Footer, earlier));
    let revision = d.revision;
    d.remove_running_content_inner("footer", page, "", 0)
        .expect("remove");
    assert_eq!(d.revision, revision + 1, "one undoable action");
    let shown = d
        .running_blocks_of(RunningRegion::Footer, page)
        .expect("the section points at an empty body")
        .to_vec();
    assert_eq!(
        page_fields(&shown),
        0,
        "no page number on section 3's pages"
    );
    assert!(casual_doc_command::sections::blocks_are_blank(&shown));
    assert_eq!(
        text(&blocks(&d, RunningRegion::Footer, earlier)),
        earlier_text
    );
    assert!(
        d.remove_running_content_inner("footer", page, "", 0)
            .is_err(),
        "nothing left to remove, and it says so"
    );
}

/// On the first section nothing can show through, so Remove drops the
/// reference outright and the band is gone.
#[test]
fn removing_the_first_sections_header_drops_its_reference() {
    let mut d = open_document(SAMPLE_DOCX).expect("open");
    let page = page_in_section(&d, 0);
    d.remove_running_content_inner("header", page, "", 0)
        .expect("remove");
    assert!(own(&d, 0, RunningRegion::Header).is_none());
    assert!(
        own(&d, 1, RunningRegion::Header).is_some(),
        "section 2 keeps its own"
    );
}

/// A page number added to a band that already has content gets a paragraph of
/// its own, centred, and the content stays — where Word and ONLYOFFICE clear
/// the band first (`SetHdrFtrPageNum`, `DocumentContent.js:1961`).
///
/// Mutation: drop the `SplitParagraph` and insert into the edge paragraph: the
/// header then has one paragraph, not two, and the count assertion fails.
#[test]
fn a_page_number_in_a_header_with_content_keeps_the_content() {
    let mut d = open_document(SAMPLE_DOCX).expect("open");
    let page = page_in_section(&d, 0);
    let header = own(&d, 0, RunningRegion::Header).expect("section 1's header");
    let before = text(&blocks(&d, RunningRegion::Header, header));
    assert!(
        before.iter().any(|line| !line.is_empty()),
        "sample.docx's header has text"
    );
    assert_eq!(page_fields(&blocks(&d, RunningRegion::Header, header)), 0);

    d.insert_running_page_number_inner("header", page, "center", &PageNumberText::Number)
        .expect("insert");
    let after = blocks(&d, RunningRegion::Header, header);
    assert_eq!(
        paragraphs(&after).len(),
        before.len() + 1,
        "a paragraph of its own"
    );
    assert_eq!(page_fields(&after), 1, "one page number");
    let BlockNode::Paragraph(top) = &after[0] else {
        panic!("the number's paragraph is first in a header");
    };
    assert_eq!(top.properties.alignment, Some(Alignment::Center));
    assert_eq!(page_fields(&after[..1]), 1, "and it holds the number");
    assert_eq!(
        &text(&after)[1..],
        &before[..],
        "the content is all still there, below it"
    );
}

/// On a page with no band of that variant — section 1's first page, which has
/// `w:titlePg` and no first-page footer in `sample.docx` — the band is created
/// holding the number, right-aligned, and the section points at it.
///
/// Mutation: skip the `SetSectionRunningRef` of the no-band path: the body is
/// created but nothing shows it, and `running_blocks_of` returns `None`.
#[test]
fn a_page_number_on_a_page_with_no_band_creates_one() {
    let mut d = open_document(SAMPLE_DOCX).expect("open");
    assert_eq!(d.running_kind_for_page(1, 0), HeaderFooterKind::First);
    assert!(
        d.running_blocks_of(RunningRegion::Footer, 1).is_none(),
        "no first-page footer"
    );
    let result = d
        .insert_running_page_number_inner("footer", 1, "right", &PageNumberText::Number)
        .expect("insert");
    let shown = d
        .running_blocks_of(RunningRegion::Footer, 1)
        .expect("page 1 shows a footer now")
        .to_vec();
    assert_eq!(page_fields(&shown), 1);
    let BlockNode::Paragraph(only) = &shown[0] else {
        panic!("a paragraph");
    };
    assert_eq!(only.properties.alignment, Some(Alignment::End));
    assert_eq!(
        result.node,
        only.id.to_string(),
        "the caret is in the new band"
    );
    assert!(
        d.insert_running_page_number_inner("footer", 1, "middle", &PageNumberText::Number)
            .is_err(),
        "an unknown alignment is refused"
    );
}

/// The band label and the control agree on a first page whose section declares
/// only a default header: the first-page band is inherited, so it is linked.
///
/// Mutation: restore the old `|| reference.kind == HeaderFooterKind::Default`
/// clause in `running_is_linked`: `runningBands` then reports the band as the
/// section's own while `runningLinkState` reports it linked, and the equality
/// fails.
#[test]
fn the_band_label_and_the_link_control_agree_per_variant() {
    let mut d = open_document(SAMPLE_DOCX).expect("open");
    // Give section 2 a first page; it declares only a default header.
    let section = d.document.definitions().sections[1].id;
    d.apply(Operation::SetSectionTitlePage {
        section,
        title_page: Some(true),
    })
    .expect("different first page on section 2");
    let first = (1..=d.page_count())
        .find(|p| d.running_section(*p).map(|(at, _)| at) == Some(1))
        .expect("section 2's first page");
    assert_eq!(d.running_kind_for_page(first, 1), HeaderFooterKind::First);
    let bands: serde_json::Value = serde_json::from_str(&d.running_bands(first)).expect("json");
    let state: serde_json::Value =
        serde_json::from_str(&d.running_link_state("header", first)).expect("json");
    assert_eq!(bands["headerLinked"], state["linked"]);
    assert_eq!(state["linked"], true, "no first-page header of its own");
}

/// Word's "Page X of Y": the host's words around a `PAGE` and a `NUMPAGES`
/// field, in one paragraph of a footer that already has content, which stays.
///
/// Mutation: build `OfTotal` as the number alone in `page_number_inlines` — the
/// `NUMPAGES` field and the words are missing and the assertions fail.
#[test]
fn page_x_of_y_writes_the_hosts_words_around_two_fields() {
    let mut d = open_document(SAMPLE_DOCX).expect("open");
    let page = page_in_section(&d, 2);
    let footer = own(&d, 2, RunningRegion::Footer).expect("section 3's footer");
    let before = text(&blocks(&d, RunningRegion::Footer, footer));
    d.insert_running_page_number_inner(
        "footer",
        page,
        "center",
        &PageNumberText::OfTotal {
            before: "Seite ".to_owned(),
            between: " von ".to_owned(),
            after: String::new(),
        },
    )
    .expect("insert");
    let after = blocks(&d, RunningRegion::Footer, footer);
    assert_eq!(
        &text(&after)[..before.len()],
        &before[..],
        "the footer's content stays"
    );
    let BlockNode::Paragraph(last) = after.last().expect("a paragraph") else {
        panic!("the number's paragraph is last in a footer");
    };
    let kinds: Vec<String> = last
        .inlines
        .iter()
        .map(|inline| match inline {
            InlineNode::Run(run) => format!("run:{}", run.text),
            // The field's name only: Word writes `PAGE \* MERGEFORMAT`, and the
            // switch is formatting, not which field it is.
            InlineNode::Field(field) => format!(
                "field:{}",
                field.instruction.split_whitespace().next().unwrap_or("")
            ),
            _ => "other".to_owned(),
        })
        .collect();
    assert_eq!(
        kinds,
        ["run:Seite ", "field:PAGE", "run: von ", "field:NUMPAGES"]
    );
    assert_eq!(last.properties.alignment, Some(Alignment::Center));
}
