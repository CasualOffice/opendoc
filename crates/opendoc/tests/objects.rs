// SPDX-License-Identifier: Apache-2.0

//! Pictures, headers and footers, notes and sections by edit (`docs/176`
//! §12.12): each one applied, read back, saved and reopened, undone, and
//! refused as a whole batch. These drive the public API only; the bindings run
//! the same operations through the JSON commands (`edit_commands`).

use opendoc::view::{Block, ParagraphView};
use opendoc::{Document, EditCommand, Format, PageKind, PageSetup, Region, RunFormat, SaveMode};

/// A real PNG, drawn by the engine itself (a blank page at 4 dpi).
fn png() -> Vec<u8> {
    Document::create().unwrap().render_page_png(0, 4.0).unwrap()
}

/// A second, different PNG.
fn other_png() -> Vec<u8> {
    Document::create().unwrap().render_page_png(0, 6.0).unwrap()
}

fn reopen(doc: &Document) -> Document {
    let (bytes, report) = doc.save(Format::Docx, SaveMode::PreserveWhenSafe).unwrap();
    assert!(
        report.losses.is_empty(),
        "the save lost {:?}",
        report.losses
    );
    Document::open(&bytes).unwrap()
}

fn images(doc: &Document) -> Vec<(String, opendoc::view::ImageView)> {
    doc.paragraphs()
        .into_iter()
        .flat_map(|p| p.runs)
        .filter_map(|r| r.image.map(|image| (r.id, image)))
        .collect()
}

fn snapshot(doc: &Document) -> Vec<u8> {
    doc.save(Format::Json, SaveMode::Semantic).unwrap().0
}

fn blocks_text(blocks: &[Block]) -> String {
    opendoc::view::blocks_text(blocks)
}

#[test]
fn a_picture_is_inserted_resized_described_replaced_and_deleted() {
    let mut doc = Document::create().unwrap();
    let ((p, picture), _) = doc
        .edit(|e| {
            let p = e.append_paragraph("Logo: ", None)?;
            let picture = e.insert_picture(&p, 6, &png(), Some(72.0), None)?;
            Ok((p, picture))
        })
        .unwrap();
    let found = images(&doc);
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].0, picture,
        "the read view shows the picture by the id returned"
    );
    assert!((found[0].1.width_pt.unwrap() - 72.0).abs() < 0.01);
    // Only the width was given: the height keeps the image's own proportions.
    let image = png();
    let pixels = |at: usize| u32::from_be_bytes(image[at..at + 4].try_into().unwrap());
    let natural = f64::from(pixels(20)) / f64::from(pixels(16));
    let ratio = found[0].1.height_pt.unwrap() / found[0].1.width_pt.unwrap();
    assert!(
        (ratio - natural).abs() < 0.01,
        "aspect kept: {ratio} against {natural}"
    );
    assert!(
        doc.media_bytes(&found[0].1.part).is_some(),
        "the bytes are in the package"
    );
    // It survives a save, with its bytes.
    let reopened = reopen(&doc);
    let again = images(&reopened);
    assert_eq!(again.len(), 1);
    assert_eq!(
        reopened.media_bytes(&again[0].1.part),
        Some(png().as_slice())
    );

    doc.edit(|e| {
        e.resize_picture(&picture, 144.0, 36.0)?;
        e.set_alt_text(&picture, Some("Company logo"))
    })
    .unwrap();
    let image = &images(&doc)[0].1;
    assert_eq!((image.width_pt, image.height_pt), (Some(144.0), Some(36.0)));
    assert_eq!(image.description.as_deref(), Some("Company logo"));

    let before_replace = images(&doc)[0].1.part.clone();
    doc.edit(|e| e.replace_picture(&picture, &other_png()))
        .unwrap();
    let (id, image) = images(&doc).remove(0);
    assert_eq!(id, picture, "a replaced picture keeps its id");
    assert_ne!(image.part, before_replace);
    assert_eq!(
        image.description.as_deref(),
        Some("Company logo"),
        "and its alt text"
    );
    assert!(
        image.width_pt.unwrap() <= 144.0 + 0.01 && image.height_pt.unwrap() <= 36.0 + 0.01,
        "the new image fits the old frame"
    );
    assert_eq!(doc.media_bytes(&image.part), Some(other_png().as_slice()));
    assert_eq!(
        reopen(&doc).media_bytes(&image.part),
        Some(other_png().as_slice())
    );

    let (_, undo) = doc.edit(|e| e.delete_picture(&picture)).unwrap();
    assert!(images(&doc).is_empty());
    assert_eq!(doc.paragraph(&p).unwrap().text, "Logo: ");
    doc.undo(&undo).unwrap();
    assert_eq!(images(&doc).len(), 1, "undo restores the picture");
}

#[test]
fn a_refused_batch_takes_its_picture_with_it() {
    let mut doc = Document::create().unwrap();
    let ((p, _), _) = doc
        .edit(|e| Ok((e.append_paragraph("x", None)?, ())))
        .unwrap();
    let before = snapshot(&doc);
    let media_before = doc.media().len();
    let refused = doc.edit(|e| {
        e.insert_picture(&p, 0, &png(), None, None)?;
        e.insert_text(&p, 99, "past the end")
    });
    assert!(refused.is_err());
    assert_eq!(snapshot(&doc), before, "the document is exactly as it was");
    assert_eq!(doc.media().len(), media_before);
    // Not an image at all is refused with the reason.
    let error = doc
        .edit(|e| e.insert_picture(&p, 0, b"not a picture", None, None))
        .unwrap_err();
    assert!(error.message().contains("PNG, JPEG"), "{}", error.message());
}

fn section_header_text(doc: &Document, section: usize, region: &str, kind: &str) -> Option<String> {
    let s = &doc.sections()[section];
    let refs = if region == "header" {
        &s.headers
    } else {
        &s.footers
    };
    let id = &refs.iter().find(|r| r.kind == kind)?.id;
    doc.headers_footers()
        .into_iter()
        .find(|h| &h.id == id)
        .map(|h| blocks_text(&h.blocks))
}

#[test]
fn headers_and_footers_default_first_and_even_with_fields() {
    let mut doc = Document::create().unwrap();
    doc.edit(|e| {
        e.append_paragraph("Body", None)?;
        let header = e.header_footer(0, Region::Header, PageKind::Default)?;
        e.insert_text(&header, 0, "Quarterly report")?;
        let footer = e.header_footer(0, Region::Footer, PageKind::Default)?;
        e.insert_text(&footer, 0, "Page ")?;
        e.insert_field(&footer, 5, "page", "")?;
        e.append_run(&footer, " of ", &RunFormat::default())?;
        e.insert_field(&footer, 9, "numpages", "")?;
        e.set_different_first_page(0, true)?;
        let first = e.header_footer(0, Region::Header, PageKind::First)?;
        e.insert_text(&first, 0, "Cover")?;
        e.set_different_odd_even(true)?;
        let even = e.header_footer(0, Region::Header, PageKind::Even)?;
        e.insert_text(&even, 0, "Even page")?;
        // A second paragraph in a header, after the first.
        let second =
            e.insert_paragraph(&opendoc::Position::after(even.clone()), "Second line", None)?;
        assert_ne!(second, even);
        Ok(())
    })
    .unwrap();
    for d in [&doc, &reopen(&doc)] {
        assert_eq!(
            section_header_text(d, 0, "header", "default").as_deref(),
            Some("Quarterly report")
        );
        assert_eq!(
            section_header_text(d, 0, "header", "first").as_deref(),
            Some("Cover")
        );
        assert_eq!(
            section_header_text(d, 0, "header", "even").as_deref(),
            Some("Even page\nSecond line")
        );
        assert!(d.sections()[0].title_page);
        let footer = d
            .headers_footers()
            .into_iter()
            .find(|h| h.part == "footer")
            .unwrap();
        let fields: Vec<_> = footer
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph(ParagraphView { runs, .. }) => Some(runs.clone()),
                _ => None,
            })
            .flatten()
            .filter_map(|r| r.field)
            .collect();
        let names: Vec<_> = fields
            .iter()
            .map(|f| f.split_whitespace().next().unwrap_or_default())
            .collect();
        assert_eq!(names, ["PAGE", "NUMPAGES"]);
    }
    // The layout prints the first-page header on page 1: emptying it changes
    // that page's pixels (page text covers the body only).
    let with_cover = doc.render_page(0, 36.0).unwrap();
    let ((first, _), _) = doc
        .edit(|e| Ok((e.header_footer(0, Region::Header, PageKind::First)?, ())))
        .unwrap();
    let (_, undo) = doc.edit(|e| e.delete_text(&first, 0, 5)).unwrap();
    assert_ne!(doc.render_page(0, 36.0).unwrap(), with_cover);
    doc.undo(&undo).unwrap();
    assert_eq!(doc.render_page(0, 36.0).unwrap(), with_cover);

    // Asking again returns the existing band; removing it empties the section.
    let ((again, _), _) = doc
        .edit(|e| Ok((e.header_footer(0, Region::Header, PageKind::Default)?, ())))
        .unwrap();
    assert_eq!(doc.paragraph(&again).unwrap().text, "Quarterly report");
    doc.edit(|e| e.remove_header_footer(0, Region::Header, PageKind::Default))
        .unwrap();
    assert_eq!(section_header_text(&doc, 0, "header", "default"), None);
}

#[test]
fn notes_are_inserted_edited_and_deleted() {
    let mut doc = Document::create().unwrap();
    let ((p, foot, end), _) = doc
        .edit(|e| {
            let p = e.append_paragraph("Claim one. Claim two.", None)?;
            let foot = e.insert_note("footnote", &p, 10, "Source A")?;
            let end = e.insert_note("endnote", &p, 21, "Source B")?;
            Ok((p, foot, end))
        })
        .unwrap();
    let notes = |d: &Document| -> Vec<(String, String, String)> {
        d.notes()
            .into_iter()
            .map(|n| (n.id, n.kind.to_owned(), n.text))
            .collect()
    };
    assert!(notes(&doc).contains(&(foot.clone(), "footnote".into(), "Source A".into())));
    assert!(notes(&doc).contains(&(end.clone(), "endnote".into(), "Source B".into())));
    let references: Vec<_> = doc
        .paragraph(&p)
        .unwrap()
        .runs
        .into_iter()
        .filter_map(|r| r.reference)
        .collect();
    assert_eq!(references, [foot.clone(), end.clone()]);
    let texts = |d: &Document| -> Vec<String> { d.notes().into_iter().map(|n| n.text).collect() };
    assert!(texts(&reopen(&doc)).contains(&"Source A".to_owned()));

    doc.edit(|e| e.set_note_text(&foot, "Source A, page 4"))
        .unwrap();
    assert!(texts(&doc).contains(&"Source A, page 4".to_owned()));
    let (_, undo) = doc.edit(|e| e.delete_note(&end)).unwrap();
    assert!(doc.notes().iter().all(|n| n.id != end));
    assert!(
        doc.paragraph(&p)
            .unwrap()
            .runs
            .iter()
            .all(|r| r.reference.as_deref() != Some(&end))
    );
    doc.undo(&undo).unwrap();
    assert!(
        doc.notes().iter().any(|n| n.id == end),
        "undo restores the note"
    );
    assert!(doc.edit(|e| e.delete_note("123456")).is_err());
}

#[test]
fn sections_are_added_typed_and_set_up() {
    let mut doc = Document::create().unwrap();
    doc.edit(|e| {
        e.append_paragraph("Portrait part", None)?;
        let index = e.add_section("nextPage")?;
        assert_eq!(index, 1);
        e.append_paragraph("Landscape part", None)?;
        e.set_page_setup(
            1,
            &PageSetup {
                orientation: Some("landscape".into()),
                top_pt: Some(36.0),
                left_pt: Some(54.0),
                columns: Some(2),
                column_spacing_pt: Some(18.0),
                ..PageSetup::default()
            },
        )?;
        e.set_section_start(1, "oddPage")?;
        Ok(())
    })
    .unwrap();
    for d in [&doc, &reopen(&doc)] {
        let sections = d.sections();
        assert_eq!(sections.len(), 2);
        let (first, second) = (&sections[0], &sections[1]);
        assert_eq!(first.orientation, "portrait");
        assert_eq!(second.orientation, "landscape");
        assert!(
            second.page_width_pt > second.page_height_pt,
            "turning swaps the size, as Word does"
        );
        assert_eq!(
            (second.page_width_pt, second.page_height_pt),
            (first.page_height_pt, first.page_width_pt)
        );
        assert_eq!(second.margins_pt.top, 36.0);
        assert_eq!(second.margins_pt.left, 54.0);
        assert_eq!(second.columns, 2);
        assert_eq!(second.start, "oddPage");
    }
    // The landscape part is on its own (landscape) page.
    let sizes = doc.page_sizes();
    assert!(sizes.len() >= 2);
    assert!(sizes.last().unwrap().width_pt > sizes.last().unwrap().height_pt);
    // A section break mid-paragraph, by position, continuous.
    let p = doc
        .paragraphs()
        .into_iter()
        .find(|p| p.text == "Portrait part")
        .unwrap()
        .id;
    doc.edit(|e| e.insert_section_break(&p, 8, "continuous"))
        .unwrap();
    assert_eq!(doc.sections().len(), 3);
    assert_eq!(doc.sections()[1].start, "continuous");
    // Out of range is refused, the document unchanged.
    let before = snapshot(&doc);
    assert!(
        doc.edit(|e| e.set_page_setup(9, &PageSetup::default()))
            .is_err()
    );
    assert!(doc.edit(|e| e.set_section_start(0, "sideways")).is_err());
    assert_eq!(snapshot(&doc), before);
}

#[test]
fn the_json_commands_reach_every_new_edit() {
    // What every binding sends. One batch, with results.
    let mut doc = Document::create().unwrap();
    let ((p, _), _) = doc
        .edit(|e| Ok((e.append_paragraph("Hello", None)?, ())))
        .unwrap();
    let image = opendoc::base64::encode(&png());
    let commands: Vec<EditCommand> = serde_json::from_value(serde_json::json!([
        {"op": "insertPicture", "paragraph": p, "offset": 5, "image": image, "widthPt": 50},
        {"op": "headerFooter", "section": 0, "region": "footer"},
        {"op": "insertNote", "kind": "footnote", "paragraph": p, "offset": 2, "text": "n"},
        {"op": "addSection", "start": "continuous"},
        {"op": "setPageSetup", "section": 1, "setup": {"orientation": "landscape", "bottomPt": 40}},
        {"op": "setDifferentOddEven", "on": true}
    ]))
    .unwrap();
    let (results, _) = doc.edit_commands(&commands).unwrap();
    assert!(results[0].is_string() && results[1].is_string() && results[2].is_string());
    assert_eq!(results[3], 1);
    assert_eq!(doc.sections()[1].margins_pt.bottom, 40.0);
    assert!(
        serde_json::from_value::<Vec<EditCommand>>(
            serde_json::json!([{"op": "setPageSetup", "section": 0, "setup": {"sideways": true}}])
        )
        .is_err(),
        "an unknown page-setup field is an error, not a silent skip"
    );
}
