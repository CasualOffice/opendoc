//! The byte-preserving PDF save (`docs/182` §7, P4), end to end: import a PDF,
//! edit a frame, save with `PreserveWhenSafe`, and check the file with readers
//! we did not write the update with (hayro, for opening and rendering) and with
//! a cross-reference walker that refuses what hayro would silently repair.

#![cfg(feature = "pdf-import")]

use casual_doc_io::{
    CompatibilityReport, DocumentResources, ExportArtifact, ExportMode, ExportRequest,
    FormatExporter, FormatImporter, ImportArtifact, ImportRequest, PdfImportAdapter,
};
use casual_doc_model::v1::{BlockNode, Document, InlineNode, TextBox};
use casual_doc_pdf_read::revision::{PdfRevision, revision_tail};
use casual_doc_pdf_read::{PaintEvidence, PdfReadLimits, PdfSource};

const PROSE: &[u8] = include_bytes!("../../../fixtures/pdf/synthetic-prose.pdf");
const IMAGES: &[u8] = include_bytes!("../../../fixtures/pdf/synthetic-images.pdf");

fn import(bytes: &[u8]) -> (PdfImportAdapter, ImportArtifact) {
    let adapter = PdfImportAdapter::default();
    let artifact = adapter
        .import(ImportRequest {
            bytes,
            retain_source: true,
        })
        .expect("import");
    (adapter, artifact)
}

fn save(
    adapter: &PdfImportAdapter,
    artifact: &ImportArtifact,
    document: &Document,
    resources: &DocumentResources,
    mode: ExportMode,
) -> ExportArtifact {
    adapter
        .export(ExportRequest {
            document,
            resources,
            source: Some(&artifact.source),
            source_unchanged: false,
            mode,
        })
        .expect("export")
}

/// The text frames of page `page`'s anchor paragraph.
fn frames(document: &mut Document, page: usize) -> Vec<&mut TextBox> {
    let BlockNode::Paragraph(anchor) = &mut document.body_mut()[page] else {
        panic!("a page is one anchor paragraph");
    };
    anchor
        .inlines
        .iter_mut()
        .filter_map(|inline| match inline {
            InlineNode::TextBox(frame) => Some(&mut **frame),
            _ => None,
        })
        .collect()
}

/// Replaces a frame's text with one run of `text` in its first run's format.
fn retype(frame: &mut TextBox, text: &str) {
    let BlockNode::Paragraph(paragraph) = &mut frame.blocks[0] else {
        panic!("a frame holds a paragraph");
    };
    paragraph.inlines.truncate(1);
    let InlineNode::Run(run) = &mut paragraph.inlines[0] else {
        panic!("a frame starts with a run");
    };
    run.text = text.to_owned();
}

/// Page `index`'s text as hayro decodes it from `bytes`.
fn page_text(bytes: &[u8], index: usize) -> String {
    let mut source = PdfSource::open(bytes.to_vec(), PdfReadLimits::default()).expect("reopen");
    let page = source.page_evidence(index).expect("page");
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

/// Every page rendered by hayro, as RGBA.
fn render(bytes: &[u8]) -> Vec<Vec<u8>> {
    let pdf = hayro::hayro_syntax::Pdf::new(bytes.to_vec()).expect("hayro opens it");
    let white = hayro::vello_cpu::color::AlphaColor::from_rgba8(255, 255, 255, 255);
    pdf.pages()
        .iter()
        .map(|page| {
            hayro::render(
                page,
                &hayro::RenderCache::new(),
                &hayro::hayro_interpret::InterpreterSettings::default(),
                &hayro::RenderSettings::default(),
                &hayro::PixmapSettings {
                    x_scale: 0.5,
                    y_scale: 0.5,
                    bg_color: white,
                },
            )
            .data()
            .iter()
            .flat_map(|p| [p.r, p.g, p.b, p.a])
            .collect()
        })
        .collect()
}

fn find(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn number_after(text: &[u8], key: &[u8]) -> Option<usize> {
    let at = find(text, 0, key)? + key.len();
    let digits: String = text[at..]
        .iter()
        .skip_while(|b| b.is_ascii_whitespace())
        .take_while(|b| b.is_ascii_digit())
        .map(|b| char::from(*b))
        .collect();
    digits.parse().ok()
}

/// Walks the cross-reference chain from the last `startxref` through every
/// `/Prev`, table or stream, and checks every in-use entry points at its own
/// `n g obj` header. Returns the sections' offsets, newest first. A reader
/// that repairs (hayro does) would open a broken chain; this does not.
fn xref_chain(bytes: &[u8]) -> Vec<usize> {
    let mut offset = Some(revision_tail(bytes).expect("a tail").startxref);
    let mut chain = Vec::new();
    while let Some(at) = offset {
        assert!(!chain.contains(&at), "a /Prev loop at {at}");
        chain.push(at);
        let check = |number: usize, generation: usize, target: usize| {
            let header = format!("{number} {generation} obj");
            assert!(
                bytes[target..].starts_with(header.as_bytes()),
                "section at {at}: object {number} points at {target}, not at `{header}`"
            );
        };
        let dict: &[u8];
        if bytes[at..].starts_with(b"xref") {
            let trailer = find(bytes, at, b"trailer").expect("trailer");
            let table = std::str::from_utf8(&bytes[at + 4..trailer]).unwrap();
            let mut lines = table.lines().filter(|l| !l.trim().is_empty());
            while let Some(head) = lines.next() {
                let mut head = head.split_whitespace();
                let first: usize = head.next().unwrap().parse().unwrap();
                let count: usize = head.next().unwrap().parse().unwrap();
                for number in first..first + count {
                    let entry: Vec<&str> = lines.next().unwrap().split_whitespace().collect();
                    if entry[2] == "n" {
                        check(number, entry[1].parse().unwrap(), entry[0].parse().unwrap());
                    }
                }
            }
            let end = find(bytes, trailer, b"startxref").unwrap_or(bytes.len());
            dict = &bytes[trailer..end];
        } else {
            let start = find(bytes, at, b"<<").unwrap();
            let stream = find(bytes, start, b"stream").unwrap();
            dict = &bytes[start..stream];
            let header = std::str::from_utf8(&bytes[at..start]).unwrap_or("");
            let header: Vec<&str> = header.split_whitespace().collect();
            assert!(
                header.len() == 3
                    && header[2] == "obj"
                    && header[..2].iter().all(|n| n.parse::<u32>().is_ok())
                    && find(dict, 0, b"/Type/XRef").is_some(),
                "the section at {at} is neither a table nor a stream: {header:?}"
            );
            assert!(find(dict, 0, b"/W[1 4 2]").is_some(), "W");
            let index_at = find(dict, 0, b"/Index[").unwrap() + 7;
            let index_end = find(dict, index_at, b"]").unwrap();
            let index: Vec<usize> = std::str::from_utf8(&dict[index_at..index_end])
                .unwrap()
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            let mut data = stream + "stream\n".len();
            for pair in index.chunks(2) {
                for number in pair[0]..pair[0] + pair[1] {
                    let entry = &bytes[data..data + 7];
                    if entry[0] == 1 {
                        let target = u32::from_be_bytes(entry[1..5].try_into().unwrap()) as usize;
                        let generation = u16::from_be_bytes(entry[5..7].try_into().unwrap());
                        check(number, usize::from(generation), target);
                    }
                    data += 7;
                }
            }
        }
        offset = number_after(dict, b"/Prev");
    }
    chain
}

/// A node id no import mints (a namespace of its own).
fn node(counter: u64) -> casual_doc_model::NodeId {
    casual_doc_model::NodeId::from_parts(0x0dd0_c0de, counter).unwrap()
}

fn has(report: &CompatibilityReport, feature: &str) -> Option<u32> {
    report
        .entries
        .iter()
        .find(|e| e.feature == feature)
        .map(|e| e.occurrences)
}

fn fallbacks(report: &CompatibilityReport) -> Vec<&str> {
    report
        .entries
        .iter()
        .filter(|e| e.feature.starts_with("pdf.export.preserve_fallback"))
        .map(|e| e.feature.as_str())
        .collect()
}

#[test]
fn an_edited_frame_is_appended_as_an_update_and_the_unchanged_page_is_carried_over() {
    let (adapter, artifact) = import(PROSE);
    let mut document = artifact.document.clone();
    retype(
        frames(&mut document, 0)[1],
        "An edited sentence replaces the first paragraph.",
    );
    let saved = save(
        &adapter,
        &artifact,
        &document,
        &artifact.resources,
        ExportMode::PreserveWhenSafe,
    );
    assert_eq!(fallbacks(&saved.report), Vec::<&str>::new());
    assert_eq!(has(&saved.report, "pdf.export.incremental_update"), Some(1));
    assert_eq!(has(&saved.report, "pdf.export.page_carried_over"), Some(1));
    assert_eq!(
        has(&saved.report, "pdf.export.earlier_revision_retained"),
        Some(1)
    );

    // The original is the prefix, byte for byte: every earlier object stays.
    let bytes = &saved.bytes;
    assert!(bytes.starts_with(PROSE), "the original is not the prefix");
    let update = &bytes[PROSE.len()..];
    // Page 1 (object 4) is replaced; page 2 (object 6) and its content
    // (object 5) are not written again, nor is page 1's original content (3).
    assert!(find(update, 0, b"\n4 0 obj").is_some() || update.starts_with(b"4 0 obj"));
    for untouched in [b"\n3 0 obj", b"\n5 0 obj", b"\n6 0 obj"] {
        assert!(
            find(update, 0, untouched).is_none(),
            "{} rewritten",
            String::from_utf8_lossy(untouched)
        );
    }

    // The unchanged page resolves to the same dictionary and content bytes.
    let before = PdfRevision::open(std::sync::Arc::new(PROSE.to_vec())).unwrap();
    let after = PdfRevision::open(std::sync::Arc::new(bytes.clone())).unwrap();
    assert_eq!(after.page_count(), 2);
    assert_eq!(before.page_parts(1).unwrap(), after.page_parts(1).unwrap());
    assert_ne!(before.page_parts(0).unwrap(), after.page_parts(0).unwrap());

    // The new revision has the edit and not the replaced text; the frames that
    // were not edited still draw from their original operators.
    let text = page_text(bytes, 0);
    assert!(text.contains("An edited sentence replaces"), "{text}");
    assert!(!text.contains("quick brown fox"), "{text}");
    assert!(text.contains("Synthetic prose"), "{text}");
    assert!(text.contains("A second paragraph begins"), "{text}");
    assert_eq!(page_text(bytes, 1), page_text(PROSE, 1));

    // Structure: the chain resolves to the original's section, and hayro
    // renders every page; the carried-over page renders identically.
    assert_eq!(xref_chain(bytes), [PROSE.len() + update_xref(update), 1819]);
    let (old, new) = (render(PROSE), render(bytes));
    assert_eq!(new.len(), 2);
    assert_eq!(old[1], new[1], "the carried-over page renders differently");
    assert_ne!(old[0], new[0], "the edited page renders as before");
    assert!(
        new[0].chunks(4).any(|p| p[0] < 128),
        "the edited page is blank"
    );
}

/// The offset of the update's own cross-reference section within `update`.
fn update_xref(update: &[u8]) -> usize {
    let at = update.windows(9).rposition(|w| w == b"startxref").unwrap();
    number_after(&update[at..], b"startxref").unwrap() - PROSE.len()
}

/// A deterministic stand-in for a signature value: the bytes a `/ByteRange`
/// covers, hashed. A real PKCS#7 signature covers the same bytes the same way.
fn covered_digest(bytes: &[u8], range: [usize; 4]) -> String {
    let mut low: u64 = 0xcbf2_9ce4_8422_2325;
    let mut high: u64 = 0x9e37_79b9_7f4a_7c15;
    for byte in bytes[range[0]..range[0] + range[1]]
        .iter()
        .chain(&bytes[range[2]..range[2] + range[3]])
    {
        low = (low ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        high = (high ^ u64::from(byte.rotate_left(3))).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{low:016x}{high:016x}")
}

/// Signs `original` the way a signing tool does, as an incremental update: a
/// signature dictionary whose `/ByteRange` covers the whole file but its own
/// `/Contents`, a signature field on page 1, and `/SigFlags 3`. The value in
/// `/Contents` is [`covered_digest`], a rights-safe dummy. With `xref_stream`
/// the update ends in a cross-reference stream. Returns the signed file and its
/// byte range.
fn sign(original: &[u8], xref_stream: bool) -> (Vec<u8>, [usize; 4]) {
    let tail = revision_tail(original).unwrap();
    let mut out = original.to_vec();
    let mut offsets = Vec::new();
    let page_start = find(original, 0, b"4 0 obj").unwrap();
    let page_end = find(original, page_start, b"endobj").unwrap();
    let page = std::str::from_utf8(&original[page_start + 8..page_end])
        .unwrap()
        .trim_end();
    let page = format!(
        "{}/Annots[8 0 R]>>",
        page.strip_suffix(">>").expect("a dictionary")
    );
    let objects = [
        (
            7,
            format!(
                "<</Type/Sig/Filter/Adobe.PPKLite/SubFilter/adbe.pkcs7.detached\
                 /ByteRange[0 0000000000 0000000000 0000000000]/Contents<{}>>>",
                "0".repeat(64)
            ),
        ),
        (
            8,
            "<</Type/Annot/Subtype/Widget/FT/Sig/T(Signature1)/Rect[0 0 0 0]/F 132/V 7 0 R/P 4 0 R>>"
                .to_owned(),
        ),
        (
            1,
            "<</Type/Catalog/Pages 2 0 R/AcroForm<</Fields[8 0 R]/SigFlags 3>>>>".to_owned(),
        ),
        (4, page),
    ];
    for (number, body) in &objects {
        offsets.push((*number, out.len()));
        out.extend_from_slice(format!("{number} 0 obj\n{body}\nendobj\n").as_bytes());
    }
    offsets.sort_unstable();
    let xref = out.len();
    if xref_stream {
        offsets.push((9, xref));
        let mut data = Vec::new();
        for (_, offset) in &offsets {
            data.push(1u8);
            data.extend_from_slice(&(*offset as u32).to_be_bytes());
            data.extend_from_slice(&[0, 0]);
        }
        out.extend_from_slice(
            format!(
                "9 0 obj\n<</Type/XRef/Size 10/W[1 4 2]/Index[1 1 4 1 7 3]/Root 1 0 R/Prev {}/Length {}>>\nstream\n",
                tail.startxref,
                data.len()
            )
            .as_bytes(),
        );
        out.extend_from_slice(&data);
        out.extend_from_slice(b"\nendstream\nendobj\n");
    } else {
        out.extend_from_slice(b"xref\n1 1\n");
        out.extend_from_slice(format!("{:010} 00000 n \n", offsets[0].1).as_bytes());
        out.extend_from_slice(b"4 1\n");
        out.extend_from_slice(format!("{:010} 00000 n \n", offsets[1].1).as_bytes());
        out.extend_from_slice(b"7 2\n");
        for (_, offset) in &offsets[2..] {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!("trailer\n<</Size 9/Root 1 0 R/Prev {}>>\n", tail.startxref).as_bytes(),
        );
    }
    out.extend_from_slice(format!("startxref\n{xref}\n%%EOF\n").as_bytes());

    let contents = find(&out, 0, b"/Contents<").unwrap() + "/Contents".len();
    let after = contents + 66;
    let range = [0, contents, after, out.len() - after];
    let at = find(&out, 0, b"/ByteRange[0 ").unwrap() + "/ByteRange[0 ".len();
    let numbers = format!("{:010} {:010} {:010}", range[1], range[2], range[3]);
    out[at..at + numbers.len()].copy_from_slice(numbers.as_bytes());
    let digest = covered_digest(&out, range);
    out[contents + 1..contents + 1 + digest.len()].copy_from_slice(digest.as_bytes());
    (out, range)
}

/// The `/ByteRange` and `/Contents` value of the first signature in `bytes`.
fn signature_of(bytes: &[u8]) -> ([usize; 4], String) {
    let at = find(bytes, 0, b"/ByteRange[").unwrap() + "/ByteRange[".len();
    let end = find(bytes, at, b"]").unwrap();
    let numbers: Vec<usize> = std::str::from_utf8(&bytes[at..end])
        .unwrap()
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    let contents = find(bytes, 0, b"/Contents<").unwrap() + "/Contents<".len();
    let value = String::from_utf8(bytes[contents..contents + 32].to_vec()).unwrap();
    ([numbers[0], numbers[1], numbers[2], numbers[3]], value)
}

#[test]
fn a_signed_file_keeps_its_signed_revision_intact_when_a_page_is_edited() {
    for xref_stream in [false, true] {
        let (signed, range) = sign(PROSE, xref_stream);
        // The dummy signature verifies on the file it was made for.
        assert_eq!(
            signature_of(&signed),
            (range, covered_digest(&signed, range))
        );

        let (adapter, artifact) = import(&signed);
        let mut document = artifact.document.clone();
        retype(frames(&mut document, 0)[0], "Signed, then edited");
        let saved = save(
            &adapter,
            &artifact,
            &document,
            &artifact.resources,
            ExportMode::PreserveWhenSafe,
        );
        assert_eq!(
            fallbacks(&saved.report),
            Vec::<&str>::new(),
            "{xref_stream}"
        );
        assert_eq!(
            has(
                &saved.report,
                "pdf.export.signature_covers_earlier_revision"
            ),
            Some(1)
        );
        let bytes = &saved.bytes;
        // The signed revision is the prefix, so the byte range the signature
        // covers is unchanged and the signature still verifies for it.
        assert!(bytes.starts_with(&signed));
        let (found, value) = signature_of(bytes);
        assert_eq!(found, range);
        assert_eq!(
            found[2] + found[3],
            signed.len(),
            "covers the signed revision"
        );
        assert_eq!(
            covered_digest(bytes, found),
            value,
            "the signed bytes changed"
        );
        // The update is the same kind of section, chained to the signing one.
        let tail = revision_tail(bytes).unwrap();
        assert_eq!(tail.xref_stream, xref_stream);
        let chain = xref_chain(bytes);
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[2], 1819);
        // The edited page keeps its signature widget.
        let after = PdfRevision::open(std::sync::Arc::new(bytes.clone())).unwrap();
        let entries = String::from_utf8(after.page_parts(0).unwrap().entries).unwrap();
        assert!(entries.contains("/Annots [8 0 R]"), "{entries}");
        let text = page_text(bytes, 0);
        assert!(text.contains("Signed, then edited") && !text.contains("Synthetic prose"));
        assert_eq!(render(bytes).len(), 2);
    }
}

#[test]
fn an_undone_edit_writes_the_original_back_and_semantic_mode_regenerates() {
    let (adapter, artifact) = import(PROSE);
    // `source_unchanged: false` (the watermark moved) but the content is the
    // import's: the original comes back, not a re-render.
    let saved = save(
        &adapter,
        &artifact,
        &artifact.document,
        &artifact.resources,
        ExportMode::PreserveWhenSafe,
    );
    assert_eq!(saved.bytes, PROSE);
    assert_eq!(
        has(&saved.report, "pdf.export.original_written_back"),
        Some(1)
    );
    let semantic = save(
        &adapter,
        &artifact,
        &artifact.document,
        &artifact.resources,
        ExportMode::Semantic,
    );
    assert!(!semantic.bytes.starts_with(PROSE));
}

#[test]
fn what_cannot_be_preserved_falls_back_to_the_semantic_writer_and_says_why() {
    // A picture removed: the page's non-text content changed.
    let (adapter, artifact) = import(IMAGES);
    let mut document = artifact.document.clone();
    let BlockNode::Paragraph(anchor) = &mut document.body_mut()[0] else {
        panic!()
    };
    let at = anchor
        .inlines
        .iter()
        .position(|i| matches!(i, InlineNode::AnchoredDrawing(_)))
        .expect("a picture");
    anchor.inlines.remove(at);
    let saved = save(
        &adapter,
        &artifact,
        &document,
        &artifact.resources,
        ExportMode::PreserveWhenSafe,
    );
    assert_eq!(
        fallbacks(&saved.report),
        ["pdf.export.preserve_fallback.page_objects_changed"]
    );
    assert!(!saved.bytes.starts_with(IMAGES));

    // A frame given a border: the overlay draws text only, so it refuses.
    let (adapter, artifact) = import(PROSE);
    let mut document = artifact.document.clone();
    let frame = frames(&mut document, 0).remove(1);
    retype(frame, "Boxed");
    frame.fill = Some(casual_doc_model::v1::Fill::Solid(
        casual_doc_model::v1::Rgba {
            r: 200,
            g: 0,
            b: 0,
            a: 255,
        },
    ));
    let saved = save(
        &adapter,
        &artifact,
        &document,
        &artifact.resources,
        ExportMode::PreserveWhenSafe,
    );
    assert_eq!(
        fallbacks(&saved.report),
        ["pdf.export.preserve_fallback.frame_not_text_only"]
    );
    assert!(!saved.bytes.starts_with(PROSE));

    // Without a retained original there is nothing to preserve.
    let plain = PdfImportAdapter::default()
        .import(ImportRequest {
            bytes: PROSE,
            retain_source: false,
        })
        .unwrap();
    let saved = save(
        &adapter,
        &plain,
        &plain.document,
        &plain.resources,
        ExportMode::PreserveWhenSafe,
    );
    assert_eq!(
        fallbacks(&saved.report),
        ["pdf.export.preserve_fallback.no_retained_source"]
    );
}

#[test]
fn a_deleted_frame_loses_its_text_and_an_added_one_is_drawn() {
    let (adapter, artifact) = import(PROSE);
    let mut document = artifact.document.clone();
    // Delete the heading frame; add a copy of the last frame, retyped.
    let BlockNode::Paragraph(anchor) = &mut document.body_mut()[0] else {
        panic!()
    };
    let heading = anchor
        .inlines
        .iter()
        .position(|i| matches!(i, InlineNode::TextBox(_)))
        .unwrap();
    anchor.inlines.remove(heading);
    let mut added = anchor
        .inlines
        .iter()
        .rev()
        .find_map(|i| match i {
            InlineNode::TextBox(frame) => Some(frame.clone()),
            _ => None,
        })
        .unwrap();
    added.id = node(1);
    if let BlockNode::Paragraph(paragraph) = &mut added.blocks[0] {
        paragraph.id = node(2);
        for (n, inline) in paragraph.inlines.iter_mut().enumerate() {
            match inline {
                InlineNode::Run(run) => {
                    run.id = node(0x100 + n as u64);
                }
                InlineNode::Break(b) => {
                    b.id = node(0x100 + n as u64);
                }
                _ => {}
            }
        }
    }
    retype(&mut added, "A frame the user added");
    anchor.inlines.push(InlineNode::TextBox(added));
    let saved = save(
        &adapter,
        &artifact,
        &document,
        &artifact.resources,
        ExportMode::PreserveWhenSafe,
    );
    assert_eq!(fallbacks(&saved.report), Vec::<&str>::new());
    let text = page_text(&saved.bytes, 0);
    assert!(!text.contains("Synthetic prose"), "{text}");
    assert!(text.contains("A frame the user added"), "{text}");
    assert!(text.contains("quick brown fox"), "{text}");
}

/// A one-page PDF drawing `content` with Helvetica as `/F1`.
fn one_page(content: &str) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    let objects = [
        "<</Type/Catalog/Pages 2 0 R>>".to_owned(),
        "<</Type/Pages/Kids[3 0 R]/Count 1>>".to_owned(),
        "<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]/Contents 4 0 R\
         /Resources<</Font<</F1<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>>>>>>>"
            .to_owned(),
        format!(
            "<</Length {}>>\nstream\n{content}\nendstream",
            content.len() + 1
        ),
    ];
    for (at, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", at + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<</Size 5/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    out
}

/// Edits the frame holding `text` on page 1 and returns the save's fallbacks.
fn fallbacks_after_editing(bytes: &[u8], text: &str) -> Vec<String> {
    let (adapter, artifact) = import(bytes);
    let mut document = artifact.document.clone();
    let mut all = frames(&mut document, 0);
    assert_eq!(all.len(), 2, "two frames");
    let frame = all
        .iter_mut()
        .find(|frame| {
            let BlockNode::Paragraph(p) = &frame.blocks[0] else {
                return false;
            };
            p.inlines
                .iter()
                .any(|i| matches!(i, InlineNode::Run(r) if r.text.contains(text)))
        })
        .expect("the frame");
    retype(frame, "Edited");
    let saved = save(
        &adapter,
        &artifact,
        &document,
        &artifact.resources,
        ExportMode::PreserveWhenSafe,
    );
    fallbacks(&saved.report)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[test]
fn an_operator_that_also_paints_a_kept_frame_or_moves_one_is_not_removed() {
    // One `TJ` paints both columns: removing it for the left frame would also
    // remove the right one.
    let shared = one_page("BT /F1 12 Tf 72 700 Td [(Left column) -20000 (Right column)] TJ ET");
    assert_eq!(
        fallbacks_after_editing(&shared, "Left"),
        ["pdf.export.preserve_fallback.operator_shared_with_kept_frame"]
    );
    // Two shows on one line with no repositioning between: removing the first
    // would move the second (the text matrix advances by what a show draws).
    let flowing = one_page("BT /F1 12 Tf 72 700 Td (Left column) Tj [-20000] TJ (Right) Tj ET");
    assert_eq!(
        fallbacks_after_editing(&flowing, "Left"),
        ["pdf.export.preserve_fallback.kept_text_would_move"]
    );
    // Repositioned with `Td`: preserved.
    let positioned =
        one_page("BT /F1 12 Tf 72 700 Td (Left column) Tj 300 0 Td (Right column) Tj ET");
    assert_eq!(
        fallbacks_after_editing(&positioned, "Left"),
        Vec::<String>::new()
    );
}
