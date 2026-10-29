// SPDX-License-Identifier: Apache-2.0

//! A floating Word chart, end to end (`docs/109` HF-260), driven through the
//! real open path (`fixtures/generated/floating-chart.docx`, built by
//! `fixtures/tools/make_floating_chart.py`).
//!
//! Before this, a chart in a `wp:anchor` opened IN LINE: the importer's chart
//! arm returned before it took the pending anchor, the model had nowhere to
//! keep one, layout had no float content for a chart, and the writer emitted
//! every chart as `wp:inline`. Each assertion below is one of those four.

use casual_doc_model::v1::{BlockNode, InlineNode, WrapMode};

use crate::{WasmDocument, open_document};

const FLOATING_CHART: &[u8] = include_bytes!("../../../fixtures/generated/floating-chart.docx");

fn floating() -> WasmDocument {
    open_document(FLOATING_CHART).expect("open the floating-chart fixture")
}

/// The fixture's one chart object, found by walking the model rather than
/// through any API under test.
fn chart(document: &WasmDocument) -> casual_doc_model::v1::EmbeddedObject {
    document
        .document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(&paragraph.inlines),
            _ => None,
        })
        .flatten()
        .find_map(|inline| match inline {
            InlineNode::EmbeddedObject(object) => Some((**object).clone()),
            _ => None,
        })
        .expect("the fixture holds a chart")
}

/// Opened, it keeps its anchor: square wrap, 3.5 in from the margin, its
/// stacking key. Mutation: removing `take_embedded_anchor` from the importer's
/// chart arm opens it with `anchor: None`.
#[test]
fn a_floating_word_chart_opens_floating() {
    let document = floating();
    let object = chart(&document);
    let anchor = object.anchor.expect("the chart keeps its wp:anchor");
    assert_eq!(anchor.wrap, WrapMode::Square);
    assert_eq!(
        anchor.horizontal.position,
        casual_doc_model::v1::HorizontalPosition::Offset(3 * 914_400 + 457_200)
    );
    assert_eq!(object.relative_height, Some(251_659_264));
}

/// Laid out, it is a float a reader can select: the object list reports it as
/// a chart, its position reads as floating, and a wrap change applies to it.
/// Mutation: dropping the `AnchorContent::Chart` arm from `resolve_object_boxes`
/// (or the float pass's chart arm) leaves it out of the object list.
#[test]
fn a_floating_chart_is_placed_selectable_and_rewrappable() {
    let mut document = floating();
    let node = chart(&document).id.to_string();
    let order: Vec<serde_json::Value> =
        serde_json::from_str(&document.object_order()).expect("objectOrder is JSON");
    let entry = order
        .iter()
        .find(|entry| entry["subject"] == serde_json::json!(node))
        .expect("the floating chart is in the object list");
    assert_eq!(entry["kind"], "chart");
    let rect = document.object_rect(&node);
    assert_eq!(rect.len(), 5, "it has a placed frame");
    // 3 x 2 in = 4320 x 2880 twips.
    assert_eq!((rect[3], rect[4]), (4_320, 2_880));

    let position: serde_json::Value =
        serde_json::from_str(&document.object_position(&node)).expect("position JSON");
    assert_eq!(position["floating"], true, "{position}");
    document
        .set_object_wrap(&node, "topAndBottom")
        .expect("a floating chart can be re-wrapped");
    assert_eq!(
        chart(&document).anchor.map(|anchor| anchor.wrap),
        Some(WrapMode::TopAndBottom)
    );
}

/// Saved and reopened, it is still floating where it was. Mutation: writing
/// every chart as `wp:inline` reopens it in line.
#[test]
fn a_floating_chart_survives_a_save() {
    let document = floating();
    let before = chart(&document);
    let reopened =
        open_document(&document.export_docx().expect("export")).expect("reopen the save");
    let after = chart(&reopened);
    assert_eq!(after.anchor, before.anchor, "the anchor round-trips");
    assert_eq!(after.relative_height, before.relative_height);
    assert_eq!(after.extent, before.extent);
}
