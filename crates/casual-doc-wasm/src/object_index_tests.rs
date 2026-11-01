// SPDX-License-Identifier: Apache-2.0
//! A PDF opened in its exact layout is a thousand page-anchored frames and
//! shapes, and the object queries the editor makes on every keystroke in a
//! frame (`objectRect`, `objectFrame`, `objectHandles`) must cost one walk of
//! the document, not one walk per object (`docs/109` HF-416, `docs/182` §4.1).

use super::*;

/// A PDF of `pages` Letter pages, each with `blocks` one-line text blocks far
/// enough apart that the import makes every one its own frame, and a stroked
/// rule under each (a vector group's shapes). Written here, by this
/// repository, so the fixture carries no third-party text. O(pages × blocks).
fn many_frames_pdf(pages: usize, blocks: usize) -> Vec<u8> {
    let mut content = String::new();
    for block in 0..blocks {
        let y = 740 - block * 52;
        content.push_str(&format!(
            "BT /F1 10 Tf 72 {y} Td (Frame {block} of a synthetic page, written for this test.) Tj ET\n"
        ));
        content.push_str(&format!("0.5 w 72 {} m 540 {} l S\n", y - 8, y - 8));
    }
    let content = content.into_bytes();
    // 1 catalog, 2 page tree, 3 font, then a page and its content per page.
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {pages} >>",
            (0..pages)
                .map(|i| format!("{} 0 R", 4 + i * 2))
                .collect::<Vec<_>>()
                .join(" ")
        )
        .into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    for i in 0..pages {
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
                 /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                5 + i * 2
            )
            .into_bytes(),
        );
        let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        stream.extend_from_slice(&content);
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// The PDF opened in its exact layout, through the PDF module's own import.
fn fixed_layout(bytes: Vec<u8>) -> WasmDocument {
    let mut job = casual_doc_wasm_pdf::begin(bytes.clone(), None).expect("begin");
    while job.step(1) == "pending" {}
    let package = job.finish_package().expect("finish");
    crate::pdf_import::open(&package, bytes).expect("open")
}

/// Selectable objects, and the carrier visits that one `objectRect`,
/// `objectFrame` and `objectHandles` (what the selection chrome asks per
/// keystroke) cost, on a fixed-layout document of `pages` pages.
fn selection_chrome_cost(pages: usize) -> (usize, u64) {
    let doc = fixed_layout(many_frames_pdf(pages, 12));
    let boxes = doc.object_boxes();
    let last = boxes
        .last()
        .expect("the frames are objects")
        .subject
        .to_string();
    take_carrier_visits();
    assert!(
        !doc.object_rect(&last).is_empty(),
        "the last frame is placed"
    );
    assert!(!doc.object_frame(&last).is_empty());
    let _ = doc.object_handles(&last);
    (boxes.len(), take_carrier_visits())
}

/// The guarantee: doubling the objects roughly doubles the work of a
/// selection query. Before HF-416 every placed object paid three lookups by
/// id, each a walk of the document, so the work squared (ratio ~4) and a
/// 32-page PDF spent ~0.5 s per keystroke in the browser.
#[test]
fn doubling_a_fixed_layout_pdf_doubles_the_selection_query_not_squares_it() {
    let (objects, visits) = selection_chrome_cost(6);
    let (double_objects, double_visits) = selection_chrome_cost(12);
    // The precondition, explicit: the fixture really is many objects, and
    // doubling the pages really doubled them.
    assert!(
        objects >= 6 * 12,
        "{objects} objects on 6 pages of 12 frames"
    );
    assert_eq!(
        double_objects,
        2 * objects,
        "doubling the pages doubles the objects"
    );
    let ratio = double_visits as f64 / visits as f64;
    assert!(
        ratio < 2.6,
        "doubling the objects must roughly double a selection query's work, not \
         square it (ratio {ratio:.2}: {visits} -> {double_visits} carrier visits)"
    );
}

/// The index answers what the single-object lookups answer, for every object
/// carrier in a fixed-layout document (frames, the vector group and its
/// shapes) — one visitor behind both, kept that way.
#[test]
fn the_object_index_agrees_with_every_single_object_lookup() {
    let doc = fixed_layout(many_frames_pdf(2, 6));
    let index = ObjectIndex::build(&doc.document);
    let mut ids = Vec::new();
    for blocks in surface_block_lists(&doc.document) {
        visit_anchors_in_blocks(blocks, &mut |id, _| {
            ids.push(id);
            false
        });
    }
    visit_group_refs_in_blocks(doc.document.body(), &mut |id, _| {
        ids.push(id);
        false
    });
    assert!(
        ids.len() > 12,
        "the fixture has frames and shapes to compare"
    );
    assert!(
        !index.groups.is_empty(),
        "the rules import as a vector group"
    );
    for id in ids {
        assert_eq!(
            index.floating.contains(&id),
            object_anchor_any_surface(&doc.document, id).is_some(),
            "floating {id}"
        );
        assert_eq!(
            index.groups.get(&id),
            body_group_object_ref(doc.document.body(), id).as_ref(),
            "group reference {id}"
        );
        assert_eq!(
            index.frame(id),
            object_resize_handles(doc.document.body(), id),
            "frame {id}"
        );
    }
}
