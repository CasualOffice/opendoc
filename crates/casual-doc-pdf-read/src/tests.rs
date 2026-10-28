use super::*;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pdf")
            .join(name),
    )
    .expect(name)
}

fn text_of(page: &PageEvidence) -> String {
    let mut out = String::new();
    for item in &page.items {
        if let PaintEvidence::Glyphs(run) = item {
            for glyph in &run.glyphs {
                out.push_str(glyph.text.as_deref().unwrap_or("\u{fffd}"));
            }
        }
    }
    out
}

#[test]
fn prose_yields_positioned_unicode_glyphs_with_sizes_and_advances() {
    let mut source =
        PdfSource::open(fixture("synthetic-prose.pdf"), PdfReadLimits::default()).expect("admit");
    assert_eq!(source.page_count(), 2);
    let page = source.page_evidence(0).expect("page");
    assert_eq!((page.width, page.height), (612.0, 792.0));
    let text = text_of(&page);
    assert!(text.starts_with("Synthetic prose"), "{text}");
    let PaintEvidence::Glyphs(heading) = &page.items[0] else {
        panic!("first paint is the heading");
    };
    // `/F3 18 Tf 72 720 Td`: origin at x 72, y 792 - 720 = 72 in y-down space.
    assert!((heading.size - 18.0).abs() < 0.01, "{}", heading.size);
    let first = &heading.glyphs[0];
    assert!((first.origin.x - 72.0).abs() < 0.01 && (first.origin.y - 72.0).abs() < 0.01);
    // Consecutive origins differ by the advance (the widths drive both).
    let second = &heading.glyphs[1];
    assert!((second.origin.x - first.origin.x - first.advance).abs() < 0.01);
    assert_eq!(heading.rotation_degrees, 0.0);
    assert_eq!(source.fonts().fonts[heading.font].name, "Helvetica-Bold");
    assert!(source.fonts().fonts[heading.font].bold);
}

/// Unicode extraction cross-checked against an independent reader: the
/// writer's own inspector (`casual-doc-pdf/src/inspect.rs`) parses the same
/// files' text layer with its own ToUnicode parser, and both must agree on
/// every non-space character, in order, on every page of every writer file.
#[test]
fn unicode_matches_the_independent_inspector_on_every_writer_file() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pdf");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("writer-"))
        })
        .collect();
    files.sort();
    assert!(files.len() >= 9, "the writer corpus is present");
    let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let inspected = casual_doc_pdf::inspect::parse(&bytes).expect("inspector parses");
        let theirs: Vec<String> = inspected
            .pages()
            .into_iter()
            .map(|page| squash(&inspected.page_contents(page).plain_text()))
            .collect();
        let mut source = PdfSource::open(bytes, PdfReadLimits::default()).expect("admit");
        assert_eq!(source.page_count(), theirs.len());
        for (index, expected) in theirs.iter().enumerate() {
            let ours = squash(&text_of(&source.page_evidence(index).unwrap()));
            assert_eq!(&ours, expected, "{} page {index}", path.display());
        }
        // The embedded subsets are extracted as sfnt programs.
        assert!(
            source
                .fonts()
                .fonts
                .iter()
                .any(|font| matches!(font.program, Some((FontProgramKind::TrueType, _))))
        );
    }
}

#[test]
fn rotated_runs_report_their_rotation() {
    let mut source = PdfSource::open(
        fixture("synthetic-rotated-text.pdf"),
        PdfReadLimits::default(),
    )
    .unwrap();
    let page = source.page_evidence(0).unwrap();
    let rotations: Vec<i64> = page
        .items
        .iter()
        .filter_map(|item| match item {
            PaintEvidence::Glyphs(run) => Some(run.rotation_degrees.round() as i64),
            _ => None,
        })
        .collect();
    // y-down page space: a counter-clockwise PDF rotation reads as negative.
    assert_eq!(rotations, vec![0, -90, -30]);
}

#[test]
fn vectors_keep_fill_rule_stroke_and_merge_fill_then_stroke() {
    let mut source =
        PdfSource::open(fixture("synthetic-vector.pdf"), PdfReadLimits::default()).unwrap();
    let page = source.page_evidence(0).unwrap();
    let paths: Vec<&PathEvidence> = page
        .items
        .iter()
        .filter_map(|item| match item {
            PaintEvidence::Path(path) => Some(path),
            _ => None,
        })
        .collect();
    assert!(paths.iter().any(|p| p.even_odd), "the donut is even-odd");
    assert!(
        paths
            .iter()
            .any(|p| p.stroke.as_ref().is_some_and(|s| s.dashed))
    );
    // `B` on two rectangles: one object each with both fill and stroke.
    assert!(
        paths
            .iter()
            .filter(|p| p.fill.is_some() && p.stroke.is_some())
            .count()
            >= 1
    );
}

#[test]
fn images_are_decoded_placed_and_clipped() {
    let mut source =
        PdfSource::open(fixture("synthetic-images.pdf"), PdfReadLimits::default()).unwrap();
    let page = source.page_evidence(0).unwrap();
    let images: Vec<&ImageEvidence> = page
        .items
        .iter()
        .filter_map(|item| match item {
            PaintEvidence::Image(image) => Some(image),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 3);
    assert_eq!(images[0].format, ImageFormat::Png);
    assert!(images[0].bytes.starts_with(b"\x89PNG"));
    assert_eq!(
        images[2].format,
        ImageFormat::Jpeg,
        "plain DCT passes through"
    );
    // `144 0 0 144 72 560 cm`: 144 pt square whose top is at 792 - 704 = 88.
    let b = images[0].bounds;
    assert!(
        (b.x0 - 72.0).abs() < 0.01
            && (b.y0 - 88.0).abs() < 0.01
            && (b.width() - 144.0).abs() < 0.01
    );
    assert!(!images[0].flip_v);
    let clip = images[2]
        .clip
        .expect("the DCT picture is clipped to its left half");
    assert!((clip.width() - 72.0).abs() < 0.01);
    assert!(images[0].clip.is_none());
}

#[test]
fn marked_content_and_artifacts_are_carried() {
    let mut source =
        PdfSource::open(fixture("synthetic-tagged.pdf"), PdfReadLimits::default()).unwrap();
    assert!(source.active_content().tagged);
    let page = source.page_evidence(0).unwrap();
    let runs: Vec<&GlyphRunEvidence> = page
        .items
        .iter()
        .filter_map(|item| match item {
            PaintEvidence::Glyphs(run) => Some(run),
            _ => None,
        })
        .collect();
    assert_eq!(runs[0].mcid, Some(0));
    assert!(runs.last().unwrap().artifact);
}

#[test]
fn active_content_is_counted_and_never_run() {
    let source = PdfSource::open(
        fixture("synthetic-active-content.pdf"),
        PdfReadLimits::default(),
    )
    .unwrap();
    let active = source.active_content();
    assert_eq!(active.javascript, 1);
    assert_eq!(active.actions, 1, "the Launch action");
    assert_eq!(active.embedded_files, 1);
    assert_eq!(active.form_fields, 1);
    assert_eq!(active.links, 1);
}

#[test]
fn type3_glyphs_without_to_unicode_have_no_text() {
    let mut source =
        PdfSource::open(fixture("synthetic-type3.pdf"), PdfReadLimits::default()).unwrap();
    let page = source.page_evidence(0).unwrap();
    assert_eq!(page.notes.type3_glyphs, 6);
    assert_eq!(text_of(&page).matches('\u{fffd}').count(), 6);
}

#[test]
fn page_boxes_apply_crop_and_rotation() {
    let mut source = PdfSource::open(
        fixture("synthetic-page-boxes.pdf"),
        PdfReadLimits::default(),
    )
    .unwrap();
    let cropped = source.page_evidence(0).unwrap();
    assert_eq!((cropped.width, cropped.height), (540.0, 720.0));
    let rotated = source.page_evidence(1).unwrap();
    assert_eq!(rotated.rotation, 90);
    assert_eq!((rotated.width, rotated.height), (842.0, 595.0));
}

/// Every hostile file is refused, with the limit that refused it, and none
/// takes long: they are rejected before the work they would cause.
#[test]
fn the_hostile_corpus_is_refused_by_the_right_bound() {
    let expect: [(&str, &str); 9] = [
        ("hostile-encrypted.pdf", "encrypted"),
        ("hostile-flate-bomb.pdf", "max_stream_decoded_bytes"),
        ("hostile-operator-flood.pdf", "max_operators_per_page"),
        ("hostile-image-dimensions.pdf", "max_image_pixels"),
        ("hostile-recursive-form.pdf", "accepted"),
        ("hostile-truncated.pdf", "no-pages"),
        ("hostile-garbage.pdf", "invalid"),
        ("hostile-page-flood.pdf", "max_pages"),
        ("hostile-huge-page.pdf", "max_page_side_points"),
    ];
    for (name, verdict) in expect {
        let bytes = fixture(&format!("hostile/{name}"));
        let outcome = PdfSource::open(bytes, PdfReadLimits::default()).and_then(|mut source| {
            for index in 0..source.page_count() {
                source.page_evidence(index)?;
            }
            Ok(())
        });
        let got = match outcome {
            Ok(()) => "accepted",
            Err(PdfReadError::Encrypted) => "encrypted",
            Err(PdfReadError::Invalid) => "invalid",
            Err(PdfReadError::NoPages) => "no-pages",
            Err(PdfReadError::Limit { limit, .. }) => limit,
            Err(other) => panic!("{name}: unexpected {other}"),
        };
        // The self-drawing form is bounded by hayro's nesting cap (50): it
        // terminates and imports one small square, so it is accepted.
        assert_eq!(got, verdict, "{name}");
    }
}

#[test]
fn limits_above_the_hard_ceiling_are_refused() {
    let limits = PdfReadLimits {
        max_pages: PdfReadLimits::HARD_CEILING.max_pages + 1,
        ..PdfReadLimits::default()
    };
    assert!(matches!(
        limits.validate(),
        Err(PdfReadError::Limit {
            limit: "max_pages",
            ..
        })
    ));
}

#[test]
fn probe_finds_the_header() {
    assert!(probe_pdf(b"%PDF-1.7\n"));
    assert!(probe_pdf(b"junk before %PDF-1.4"));
    assert!(!probe_pdf(b"PK\x03\x04"));
}
