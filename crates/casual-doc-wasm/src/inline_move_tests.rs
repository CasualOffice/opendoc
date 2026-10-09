// SPDX-License-Identifier: Apache-2.0

//! Dragging an in-line object to a new place in the text (`docs/109`
//! UX-OB-02), asserted as what a reader would see: where the picture is in the
//! words afterwards, that it is still the same picture, that one Undo puts it
//! back, and that a refusal says why.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, CropRect, Definitions, Document, Drawing, DrawingHyperlink, Extent, ExternalTarget,
    HeaderFooter, HeaderFooterId, Hyperlink, HyperlinkTarget, InlineNode, LockFlags, MediaId,
    MediaReference, ObjectLocks, ObjectName, Paragraph, ParagraphProperties, Rgba, Run,
    RunProperties, ShapeStroke, Table, TableCell, TableCellProperties, TableProperties, TableRow,
    TableRowProperties, TextBox, TextBoxBodyProperties,
};

use crate::WasmDocument;

/// The fixture's node ids, by role.
struct Ids {
    picture: NodeId,
    linked_picture: NodeId,
    first: NodeId,
    second: NodeId,
    cell: NodeId,
    linked: NodeId,
    boxed: NodeId,
    inside_box: NodeId,
    header: NodeId,
}

fn id(n: u64) -> NodeId {
    NodeId::from_parts(122, n).unwrap()
}

fn run(n: u64, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id: id(n),
        properties: RunProperties::default().into(),
        text: text.to_owned(),
    })
}

fn paragraph(n: u64, inlines: Vec<InlineNode>) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: id(n),
        properties: ParagraphProperties::default().into(),
        inlines,
    })
}

/// A picture carrying EVERY property the model holds for one, each set to a
/// non-default value, so a move that rebuilt the node and forgot one is caught.
fn full_picture(n: u64, media: MediaId) -> Drawing {
    Drawing {
        id: id(n),
        media,
        extent: Some(Extent {
            width_emu: 914_400,
            height_emu: 457_200,
        }),
        descr: Some("The company logo".to_owned()),
        crop: Some(CropRect {
            left: 1_000,
            top: 2_000,
            right: 3_000,
            bottom: 4_000,
        }),
        opacity: Some(50_000),
        hyperlink: Some(DrawingHyperlink {
            target: HyperlinkTarget::External(ExternalTarget {
                url: "https://example.com/".to_owned(),
                anchor: None,
            }),
            tooltip: Some("Visit".to_owned()),
        }),
        border: Some(ShapeStroke {
            color: Rgba {
                r: 0x33,
                g: 0x55,
                b: 0xc4,
                a: 255,
            },
            width_emu: 12_700,
            dash: None,
            head_end: None,
            tail_end: None,
        }),
        flip_h: true,
        flip_v: true,
        rotation: Some(5_400_000),
    }
}

/// `Alpha [picture] Beta` / `Gamma Delta Epsilon` / a one-cell table / a
/// paragraph whose link holds a second picture / an in-line text box / and a
/// header with a line of its own.
fn fixture() -> (WasmDocument, Ids) {
    let media = MediaId::new(id(900));
    let ids = Ids {
        picture: id(10),
        linked_picture: id(40),
        first: id(1),
        second: id(2),
        cell: id(23),
        linked: id(3),
        boxed: id(4),
        inside_box: id(51),
        header: id(60),
    };
    let body = vec![
        paragraph(
            1,
            vec![
                run(11, "Alpha "),
                InlineNode::Drawing(Box::new(full_picture(10, media))),
                run(12, " Beta"),
            ],
        ),
        paragraph(2, vec![run(13, "Gamma Delta Epsilon")]),
        BlockNode::Table(Box::new(Table {
            id: id(20),
            grid: Vec::new(),
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![TableRow {
                id: id(21),
                properties: TableRowProperties::default(),
                cells: vec![TableCell {
                    id: id(22),
                    properties: TableCellProperties::default(),
                    blocks: vec![paragraph(23, vec![run(24, "Cell")])],
                }],
            }],
        })),
        paragraph(
            3,
            vec![
                run(30, "See "),
                InlineNode::Hyperlink(Box::new(Hyperlink {
                    id: id(31),
                    target: HyperlinkTarget::External(ExternalTarget {
                        url: "https://example.com/site".to_owned(),
                        anchor: None,
                    }),
                    tooltip: None,
                    inlines: vec![
                        run(32, "the site"),
                        InlineNode::Drawing(Box::new(Drawing {
                            descr: Some("A linked logo".to_owned()),
                            ..full_picture(40, media)
                        })),
                    ],
                })),
                run(33, " now"),
            ],
        ),
        paragraph(
            4,
            vec![
                run(50, "Box: "),
                InlineNode::TextBox(Box::new(TextBox {
                    hyperlink: None,
                    id: id(52),
                    anchor: None,
                    relative_height: None,
                    extent: Some(Extent {
                        width_emu: 1_828_800,
                        height_emu: 457_200,
                    }),
                    fill: None,
                    border: None,
                    body_properties: TextBoxBodyProperties::default(),
                    blocks: vec![paragraph(51, vec![run(53, "Inside")])],
                })),
            ],
        ),
    ];
    let mut definitions = Definitions::default();
    definitions.media.insert(
        media,
        MediaReference {
            relationship_id: "rIdImg1".to_owned(),
            media_type: "image/png".to_owned(),
            part_name: "word/media/image1.png".to_owned(),
        },
    );
    // A side-table entry keyed by the picture's id: a move that changed the id
    // would orphan it and the picture would be renamed "Picture 1" on save. Every
    // field is populated — the picture-level name and the locks share the entry
    // (FID-AT-08, FID-AT-09), so a move that kept only the frame's name would
    // still fail the whole-entry comparison below.
    definitions.object_names.insert(
        ids.picture,
        ObjectName {
            name: Some("Logo".to_owned()),
            title: Some("Company logo".to_owned()),
            inner_name: Some("logo.png".to_owned()),
            inner_title: None,
            locks: ObjectLocks {
                frame: LockFlags {
                    no_change_aspect: true,
                    ..LockFlags::default()
                },
                object: LockFlags {
                    no_change_aspect: true,
                    ..LockFlags::default()
                },
            },
        },
    );
    definitions.headers.insert(
        HeaderFooterId::new(id(61)),
        HeaderFooter {
            blocks: vec![paragraph(60, vec![run(62, "Running head")])],
        },
    );
    let document = Document::new(id(100), body, definitions).expect("a valid fixture");
    (crate::tests::wasm_document(document), ids)
}

/// A paragraph as a reader sees it, with each object written `{its id}` where it
/// sits in the words and a link's content in `<…>`. Runs are concatenated, so
/// the text reads the same however the move split or coalesced them.
fn flow(d: &WasmDocument, paragraph: NodeId) -> String {
    fn write(inlines: &[InlineNode], out: &mut String) {
        for inline in inlines {
            match inline {
                InlineNode::Run(run) => out.push_str(&run.text),
                InlineNode::Hyperlink(link) => {
                    out.push('<');
                    write(&link.inlines, out);
                    out.push('>');
                }
                InlineNode::Drawing(_) | InlineNode::EmbeddedObject(_) | InlineNode::TextBox(_) => {
                    out.push_str(&format!("{{{}}}", inline.id()));
                }
                _ => {}
            }
        }
    }
    let paragraph = casual_doc_edit::find_paragraph_any(&d.document, paragraph)
        .expect("the paragraph is in the document");
    let mut out = String::new();
    write(&paragraph.inlines, &mut out);
    out
}

/// The single drawing node `object`, wherever it is.
fn drawing(d: &WasmDocument, object: NodeId) -> InlineNode {
    casual_doc_edit::inline_object_position(&d.document, object)
        .map(|position| position.node.clone())
        .or_else(|| linked_drawing(d, object))
        .expect("the object is in the document")
}

fn linked_drawing(d: &WasmDocument, object: NodeId) -> Option<InlineNode> {
    casual_doc_edit::surface_block_lists(&d.document)
        .into_iter()
        .flat_map(|blocks| blocks.iter())
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => {
                paragraph.inlines.iter().find_map(|inline| match inline {
                    InlineNode::Hyperlink(link) => {
                        link.inlines.iter().find(|i| i.id() == object).cloned()
                    }
                    _ => None,
                })
            }
            _ => None,
        })
}

fn code(refusal: &str) -> &str {
    casual_doc_edit::refusal::split(refusal)
        .1
        .unwrap_or_else(|| panic!("the refusal carries no routing code: {refusal:?}"))
}

/// Every paragraph's flow, for "nothing changed" comparisons.
fn whole(d: &WasmDocument, ids: &Ids) -> Vec<String> {
    [
        ids.first,
        ids.second,
        ids.cell,
        ids.linked,
        ids.boxed,
        ids.inside_box,
        ids.header,
    ]
    .into_iter()
    .map(|paragraph| flow(d, paragraph))
    .collect()
}

#[test]
fn a_dragged_picture_lands_after_the_word_it_was_dropped_after_and_is_the_same_picture() {
    let (mut d, ids) = fixture();
    let before = drawing(&d, ids.picture);
    let name = d
        .document
        .definitions()
        .object_names
        .get(&ids.picture)
        .cloned();
    let p = ids.picture;
    assert_eq!(flow(&d, ids.first), format!("Alpha {{{p}}} Beta"));

    // "Gamma Delta" is eleven bytes: the drop caret sits right after "Delta".
    d.move_inline_object_inner(&p.to_string(), &ids.second.to_string(), 11, false)
        .expect("the picture moves");

    assert_eq!(flow(&d, ids.first), "Alpha  Beta", "it left its old place");
    assert_eq!(
        flow(&d, ids.second),
        format!("Gamma Delta{{{p}}} Epsilon"),
        "it is now after the word it was dropped after"
    );
    assert_eq!(
        drawing(&d, p),
        before,
        "the picture that arrived is the picture that left — every property, \
         including its id, so its size, crop, border, rotation, flips, alt text, \
         opacity and link all travelled"
    );
    assert_eq!(
        d.document.definitions().object_names.get(&p).cloned(),
        name,
        "and its name, which a side table keys by that id"
    );
    assert_eq!(d.undo_label(), "Object move");
}

#[test]
fn one_undo_puts_the_picture_back_and_one_redo_moves_it_again() {
    let (mut d, ids) = fixture();
    let original = whole(&d, &ids);
    let p = ids.picture;
    d.move_inline_object_inner(&p.to_string(), &ids.second.to_string(), 11, false)
        .expect("the picture moves");
    let moved = whole(&d, &ids);
    assert_ne!(moved, original);

    d.undo_inner().expect("undo");
    assert_eq!(
        whole(&d, &ids),
        original,
        "ONE undo restores the whole move"
    );
    assert!(
        !d.can_undo(),
        "and it was one step: nothing else is left to undo on a fresh document"
    );
    d.redo_inner().expect("redo");
    assert_eq!(whole(&d, &ids), moved, "one redo moves it again");
}

#[test]
fn dropping_a_picture_back_where_it_came_from_changes_nothing() {
    let (mut d, ids) = fixture();
    let original = whole(&d, &ids);
    let revision = d.revision;
    // "Alpha " is six bytes; the picture is zero-width, so 6 is both the
    // position before it and the position after it.
    for copy in [false, true] {
        let result = d
            .move_inline_object_inner(&ids.picture.to_string(), &ids.first.to_string(), 6, copy)
            .expect("a drop in place is not an error");
        assert!(result.dirty.is_empty(), "nothing repaints");
        assert_eq!(result.placed_object, "", "and nothing was placed");
    }
    assert_eq!(whole(&d, &ids), original);
    assert_eq!(d.revision, revision, "the document is not marked changed");
    assert!(!d.can_undo(), "and there is nothing to undo");
}

#[test]
fn a_picture_can_be_dropped_into_a_table_cell() {
    let (mut d, ids) = fixture();
    let p = ids.picture;
    d.move_inline_object_inner(&p.to_string(), &ids.cell.to_string(), 4, false)
        .expect("a table cell is text in the same story");
    assert_eq!(flow(&d, ids.cell), format!("Cell{{{p}}}"));
    assert_eq!(flow(&d, ids.first), "Alpha  Beta");
}

#[test]
fn a_drop_in_another_story_or_outside_ordinary_text_is_refused_with_a_reason() {
    let (mut d, ids) = fixture();
    let original = whole(&d, &ids);
    let p = ids.picture.to_string();
    for (target, offset, expected) in [
        (ids.header, 0, "object.move-other-story"),
        // A table, not a paragraph: no caret can be there.
        (id(20), 0, "object.move-target-not-text"),
        // Inside the link's text: the insertion itself refuses, and the reader
        // is told about the spot rather than shown `Unsupported`.
        (ids.linked, 6, "object.move-target-refused"),
    ] {
        let refusal = d
            .move_inline_object_inner(&p, &target.to_string(), offset, false)
            .expect_err("the drop is refused");
        assert_eq!(code(&refusal), expected, "{refusal}");
        assert!(
            refusal.starts_with(casual_doc_edit::refusal::MARKER),
            "{refusal}"
        );
        assert_eq!(whole(&d, &ids), original, "a refused drop changes nothing");
    }
}

#[test]
fn a_picture_inside_a_link_says_it_cannot_be_dragged_out_and_the_capability_agrees() {
    let (mut d, ids) = fixture();
    let refusal = d
        .move_inline_object_inner(
            &ids.linked_picture.to_string(),
            &ids.second.to_string(),
            0,
            false,
        )
        .expect_err("a picture that is a link's content is not lifted out of it");
    assert_eq!(code(&refusal), "object.move-in-wrapper");

    let boxes = d.object_boxes();
    let capability = |node: NodeId| {
        boxes
            .iter()
            .find(|object| object.subject == node)
            .unwrap_or_else(|| panic!("{node} is a placed object"))
            .capabilities
    };
    assert!(
        capability(ids.picture).can_move_in_text,
        "a picture in the words can move"
    );
    assert!(
        !capability(ids.linked_picture).can_move_in_text,
        "a picture that is a link's content cannot, so the host must not offer the drag"
    );
    let reasons = crate::capability_refusals("image", false, capability(ids.linked_picture));
    assert_eq!(
        reasons.get("canMoveInText").copied(),
        Some(super::NOT_DIRECTLY_IN_TEXT),
        "and the capability says why in the command's own words"
    );
}

#[test]
fn a_floating_object_is_placed_freely_and_is_told_so() {
    let (mut d, ids) = fixture();
    d.insert_text_box(&ids.second.to_string(), 0)
        .expect("Insert ▸ Text Box places a floating box");
    let floating = d
        .object_boxes()
        .into_iter()
        .find(|object| object.anchored)
        .expect("the inserted text box floats");
    let refusal = d
        .move_inline_object_inner(&floating.root.to_string(), &ids.first.to_string(), 0, false)
        .expect_err("a float has no place in the words to move to");
    assert_eq!(code(&refusal), "object.floats-not-in-text");
    let reasons = crate::capability_refusals(floating.kind, true, floating.capabilities);
    assert_eq!(
        reasons.get("canMoveInText").copied(),
        Some(super::FLOATS_FREELY)
    );
    assert!(
        !reasons.contains_key("canMove"),
        "and its free move is not refused"
    );
}

#[test]
fn a_copy_leaves_two_pictures_that_differ_only_in_identity() {
    let (mut d, ids) = fixture();
    let p = ids.picture;
    let original = drawing(&d, p);
    let result = d
        .move_inline_object_inner(&p.to_string(), &ids.second.to_string(), 5, true)
        .expect("a Ctrl-drag copies");
    let copy: NodeId = result.placed_object.parse().expect("the copy is named");
    assert_ne!(copy, p, "the copy is a new object");
    assert_eq!(
        flow(&d, ids.first),
        format!("Alpha {{{p}}} Beta"),
        "the original stays"
    );
    assert_eq!(
        flow(&d, ids.second),
        format!("Gamma{{{copy}}} Delta Epsilon")
    );
    let InlineNode::Drawing(mut duplicate) = drawing(&d, copy) else {
        panic!("the copy is a picture");
    };
    duplicate.id = p;
    assert_eq!(
        InlineNode::Drawing(duplicate),
        original,
        "every property of the copy is the original's"
    );
    assert_eq!(d.undo_label(), "Copy object");
    d.undo_inner().expect("undo");
    assert_eq!(
        flow(&d, ids.second),
        "Gamma Delta Epsilon",
        "one undo removes the copy"
    );
    assert_eq!(
        flow(&d, ids.first),
        format!("Alpha {{{p}}} Beta"),
        "and only the copy"
    );
}

#[test]
fn a_chart_moves_with_its_data_and_refuses_to_be_copied() {
    let (mut d, ids) = fixture();
    d.insert_chart(&ids.second.to_string(), 0, "pie")
        .expect("insert a chart");
    let projection = |d: &WasmDocument| {
        d.document
            .definitions()
            .charts
            .iter()
            .map(|(_, chart)| (chart.object, chart.clone()))
            .next()
    };
    let (chart, data) = projection(&d).expect("the chart has its data");
    d.move_inline_object_inner(&chart.to_string(), &ids.first.to_string(), 0, false)
        .expect("a chart moves in the text");
    assert_eq!(
        flow(&d, ids.first),
        format!("{{{chart}}}Alpha {{{}}} Beta", ids.picture)
    );
    assert_eq!(
        projection(&d),
        Some((chart, data)),
        "the chart keeps its data: the move must not cascade a removal of the \
         projection for an object the same edit puts back"
    );
    let refusal = d
        .move_inline_object_inner(&chart.to_string(), &ids.second.to_string(), 0, true)
        .expect_err("a chart copy would share its data part");
    assert_eq!(code(&refusal), "object.copy-unsupported");
}

#[test]
fn an_in_line_text_box_moves_with_its_text_but_not_into_it() {
    let (mut d, ids) = fixture();
    let text_box = id(52);
    let original = whole(&d, &ids);
    let refusal = d
        .move_inline_object_inner(&text_box.to_string(), &ids.inside_box.to_string(), 2, false)
        .expect_err("a text box cannot land in its own text");
    assert_eq!(code(&refusal), "object.move-into-itself");
    assert_eq!(whole(&d, &ids), original);

    d.move_inline_object_inner(&text_box.to_string(), &ids.second.to_string(), 0, false)
        .expect("an in-line text box moves in the text");
    assert_eq!(
        flow(&d, ids.second),
        format!("{{{text_box}}}Gamma Delta Epsilon")
    );
    assert_eq!(
        flow(&d, ids.inside_box),
        "Inside",
        "its own text came with it"
    );
}

#[test]
fn a_picture_can_be_dropped_into_a_text_boxs_text() {
    let (mut d, ids) = fixture();
    let p = ids.picture;
    d.move_inline_object_inner(&p.to_string(), &ids.inside_box.to_string(), 6, false)
        .expect("a text box's own text is text in the same story, as in Word");
    assert_eq!(flow(&d, ids.inside_box), format!("Inside{{{p}}}"));
}

/// A picture in a table cell is picked up, and edited, exactly as one in the
/// body is (`docs/109` UX-OB-03).
///
/// The object-box correlation walked the page's top-level paragraph fragments
/// only, so a picture in a cell painted and had no object box: a click on it
/// selected nothing, and no command that starts from a selection — resize,
/// crop, alt text, wrap, the drag — could reach it. UX-OB-02 made that easy to
/// meet, because a drag could now put a picture into a cell it could not then
/// be picked up from. This asserts the click lands where the picture was
/// PAINTED — the box agrees with the caret geometry the hit test produces — and
/// then that every edit a selection offers applies.
#[test]
fn a_picture_in_a_table_cell_is_picked_up_and_edited_like_one_in_the_body() {
    let (mut d, ids) = fixture();
    let p = ids.picture;
    let node = p.to_string();
    d.move_inline_object_inner(&node, &ids.cell.to_string(), 4, false)
        .expect("a drag into the cell");
    assert_eq!(flow(&d, ids.cell), format!("Cell{{{p}}}"));

    let placed = d
        .object_boxes()
        .into_iter()
        .find(|object| object.subject == p)
        .expect("the picture in the cell has an object box (UX-OB-03)");
    assert!(!placed.anchored);
    let caps = placed.capabilities;
    assert!(
        caps.can_resize
            && caps.can_crop
            && caps.can_alt_text
            && caps.can_stroke
            && caps.can_delete
            && caps.can_move_in_text,
        "the in-cell picture offers what a body picture offers: {caps:?}"
    );

    // Where it was PAINTED, by the hit test's own geometry rather than this
    // walk's: the box sits inside the cell's border box (`cellRect`, the rect
    // the active-cell outline is drawn from), and a point at its centre
    // resolves, through the click hit test, into the cell's own paragraph.
    let cell = d.cell_rect(&ids.cell.to_string());
    assert_eq!(cell.len(), 5, "the cell is placed");
    assert_eq!(i64::from(cell[0]), i64::from(placed.page));
    let (x, y, w, h) = (
        placed.rect.origin.x.raw(),
        placed.rect.origin.y.raw(),
        placed.rect.size.width.raw(),
        placed.rect.size.height.raw(),
    );
    assert!(
        x >= cell[1] && x < cell[1] + cell[3] && y >= cell[2] && y + h <= cell[2] + cell[4],
        "the box [{x}, {y}, {w}, {h}] is inside the cell {cell:?}"
    );
    let under = d
        .hit_test(placed.page, x + w / 2, y + h / 2)
        .expect("the picture's centre is on a line");
    assert_eq!(
        under.node(),
        ids.cell.to_string(),
        "and on the cell's paragraph"
    );
    // And exactly where the picture is DRAWN: the page's own display list.
    let page = d
        .painted_layout()
        .pages
        .iter()
        .find(|page| page.number == placed.page)
        .expect("the page");
    let drawn: Vec<_> = casual_doc_layout::compose::compose_page(page)
        .items
        .into_iter()
        .filter_map(|item| match item {
            casual_doc_layout::display::PaintItem::Image { rect, .. } => Some(rect),
            _ => None,
        })
        .collect();
    assert!(
        drawn.contains(&placed.rect),
        "the object box {:?} is a rectangle the page paints a picture into: {drawn:?}",
        placed.rect
    );
    let hit = d
        .object_at(placed.page, x + w / 2, y + h / 2)
        .expect("a click on the picture where it is painted selects it");
    assert_eq!(hit.subject(), node);

    // And every edit a selected picture offers reaches it.
    let emu = |twips: i32| f64::from(twips) * crate::EMU_PER_TWIP;
    d.resize_object(&node, emu(x), emu(y), emu(w * 2), emu(h * 2))
        .expect("resized from its grips");
    let grown = d.object_rect(&node);
    assert!(grown[3] > w + w / 2, "it is wider: {grown:?}");
    d.set_image_crop(&node, Some(vec![0.25, 0.0, 0.0, 0.0]))
        .expect("cropped");
    assert_eq!(d.object_crop(&node).map(|c| c.len()), Some(4));
    d.set_object_descr(&node, Some("A logo in a table".to_owned()))
        .expect("given alt text");
    assert_eq!(d.object_descr(&node).as_deref(), Some("A logo in a table"));
    d.set_object_anchor_kind(&node, "floating")
        .expect("given a text wrap");
    assert!(
        d.object_boxes()
            .into_iter()
            .any(|object| object.subject == p && object.anchored && object.capabilities.can_move),
        "and once it floats it is a floating object, placed freely"
    );
}
