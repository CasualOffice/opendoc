use std::fs;
use std::io::{Cursor, Write};
use std::path::PathBuf;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
const NOTE_REFERENCES_CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/><Override PartName="/word/endnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"/></Types>"#;
const VISUAL_CONTAINMENT_CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
const ROOT_RELATIONSHIPS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
const NOTE_REFERENCES_DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdFootnotes" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/><Relationship Id="rIdEndnotes" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes" Target="endnotes.xml"/></Relationships>"#;
const VISUAL_CONTAINMENT_DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdVisualFloat" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/visual-float.png"/></Relationships>"#;
const DOCUMENT: &[u8] = br#"<?xml version="1.0"?><w:document/>"#;
const MIXED_UNICODE_DOCUMENT: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
    "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    "<w:body><w:p><w:r><w:t xml:space=\"preserve\">",
    "Cafe\u{0301} | \u{0939}\u{093f}\u{0928}\u{094d}\u{0926}\u{0940} | ",
    "\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064a}\u{0629} | ",
    "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\u{200d}\u{1f466}",
    "</w:t></w:r></w:p></w:body></w:document>",
);
const NOTE_REFERENCES_DOCUMENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Text with footnote</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r></w:p><w:p><w:r><w:t>Text with endnote</w:t></w:r><w:r><w:endnoteReference w:id="2"/></w:r></w:p></w:body></w:document>"#;
const NOTE_REFERENCES_FOOTNOTES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?><w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:footnote w:id="-1" w:type="separator"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:id="0" w:type="continuationSeparator"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote><w:footnote w:id="1"><w:p><w:r><w:t>Generated footnote body.</w:t></w:r></w:p></w:footnote></w:footnotes>"#;
const NOTE_REFERENCES_ENDNOTES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?><w:endnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:endnote w:id="-1" w:type="separator"><w:p><w:r><w:separator/></w:r></w:p></w:endnote><w:endnote w:id="0" w:type="continuationSeparator"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:endnote><w:endnote w:id="2"><w:p><w:r><w:t>Generated endnote body.</w:t></w:r></w:p></w:endnote></w:endnotes>"#;
const UNKNOWN_SAFE_PART: &[u8] =
    br#"<custom xmlns="urn:opendoc:fixture"><value>preserve-me</value></custom>"#;
const VISUAL_FLOAT_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xb8, 0xe3, 0xe6, 0xf6,
    0x1f, 0x00, 0x05, 0xd2, 0x02, 0x68, 0x3b, 0x3a, 0xb3, 0x8b, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/generated");
    fs::create_dir_all(&output)?;

    fs::write(output.join("minimal-valid.docx"), minimal_package())?;

    fs::write(
        output.join("mixed-unicode.docx"),
        package(&entries_with_document(MIXED_UNICODE_DOCUMENT.as_bytes()))?,
    )?;

    fs::write(
        output.join("note-references.docx"),
        package(&note_reference_entries())?,
    )?;

    fs::write(
        output.join("visual-containment.docx"),
        package(&visual_containment_entries())?,
    )?;

    let mut unknown_safe = minimal_entries();
    unknown_safe.push((
        "customXml/item1.xml".to_owned(),
        UNKNOWN_SAFE_PART.to_vec(),
        CompressionMethod::Deflated,
    ));
    fs::write(
        output.join("unknown-safe-part.docx"),
        package(&unknown_safe)?,
    )?;

    let mut traversal = minimal_entries();
    traversal.push((
        "../outside.xml".to_owned(),
        b"unsafe".to_vec(),
        CompressionMethod::Stored,
    ));
    fs::write(output.join("path-traversal.docx"), package(&traversal)?)?;

    let mut expansion = required_stored_entries();
    expansion[2] = (
        "word/document.xml".to_owned(),
        vec![b'A'; 64 * 1024],
        CompressionMethod::Deflated,
    );
    fs::write(output.join("high-expansion.docx"), package(&expansion)?)?;

    let mut duplicate_entries = minimal_entries();
    duplicate_entries.push((
        "word/documenx.xml".to_owned(),
        b"duplicate".to_vec(),
        CompressionMethod::Stored,
    ));
    let mut duplicate = package(&duplicate_entries)?;
    patch_fourth_name_as_document(&mut duplicate)?;
    fs::write(output.join("duplicate-part.docx"), duplicate)?;

    fs::write(
        output.join("malformed-truncated.docx"),
        b"PK\x03\x04truncated",
    )?;
    Ok(())
}

fn minimal_package() -> Vec<u8> {
    package(&minimal_entries()).expect("fixture ZIP generation should succeed")
}

fn minimal_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    entries_with_document(DOCUMENT)
}

fn entries_with_document(document: &[u8]) -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            document.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
    ]
}

fn note_reference_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            NOTE_REFERENCES_DOCUMENT.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            NOTE_REFERENCES_CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            NOTE_REFERENCES_DOCUMENT_RELS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/footnotes.xml".to_owned(),
            NOTE_REFERENCES_FOOTNOTES.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/endnotes.xml".to_owned(),
            NOTE_REFERENCES_ENDNOTES.to_vec(),
            CompressionMethod::Deflated,
        ),
    ]
}

fn visual_containment_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            visual_containment_document(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            VISUAL_CONTAINMENT_CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            VISUAL_CONTAINMENT_DOCUMENT_RELS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/media/visual-float.png".to_owned(),
            VISUAL_FLOAT_PNG.to_vec(),
            CompressionMethod::Stored,
        ),
    ]
}

fn visual_containment_document() -> Vec<u8> {
    let drop_cap_body = "Drop-cap body text must begin beside the full initial, continue without clipping it, and return to the full measure after the initial ends. ".repeat(3);
    let float_anchor_text = "This paragraph starts beside a tall left-anchored picture. Every line whose vertical band crosses that picture must use the narrowed measure. ".to_owned();
    let float_following_text = "This following paragraph is intentionally still inside the picture band, so page-level exclusion must continue across the paragraph boundary before restoring the full measure below it. ".repeat(3);
    let split_row_text = "The first table row is deliberately long enough to split over page boundaries. Its cell content must remain inside each emitted row fragment and must never paint over either successor row. ".repeat(18);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
 xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
 xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
 xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
 xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">
 <w:body>
  <w:p>
   <w:pPr>
    <w:keepNext/>
    <w:framePr w:dropCap="drop" w:lines="3" w:wrap="around"
     w:hAnchor="text" w:vAnchor="text" w:xAlign="left" w:yAlign="top"
     w:hSpace="90" w:vSpace="0"/>
   </w:pPr>
   <w:r><w:rPr><w:sz w:val="117"/></w:rPr><w:t>D</w:t></w:r>
  </w:p>
  <w:p><w:r><w:t>{drop_cap_body}</w:t></w:r></w:p>
  <w:p>
   <w:r>
    <w:drawing>
     <wp:anchor behindDoc="0" relativeHeight="1" simplePos="0"
      distT="0" distB="0" distL="0" distR="91440">
      <wp:simplePos x="0" y="0"/>
      <wp:positionH relativeFrom="margin"><wp:align>left</wp:align></wp:positionH>
      <wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>
      <wp:extent cx="1600200" cy="2000250"/>
      <wp:wrapSquare wrapText="bothSides"/>
      <wp:docPr id="1" name="Visual containment float"
       descr="Generated visual containment fixture"/>
      <a:graphic>
       <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">
        <pic:pic>
         <pic:nvPicPr><pic:cNvPr id="1" name="visual-float.png"/><pic:cNvPicPr/></pic:nvPicPr>
         <pic:blipFill><a:blip r:embed="rIdVisualFloat"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>
         <pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1600200" cy="2000250"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>
        </pic:pic>
       </a:graphicData>
      </a:graphic>
     </wp:anchor>
    </w:drawing>
   </w:r>
   <w:r><w:t>{float_anchor_text}</w:t></w:r>
  </w:p>
  <w:p><w:r><w:t>{float_following_text}</w:t></w:r></w:p>
  <w:tbl>
   <w:tblPr>
    <w:tblW w:w="5000" w:type="pct"/>
    <w:tblBorders>
     <w:top w:val="single" w:sz="8" w:color="000000"/>
     <w:left w:val="single" w:sz="8" w:color="000000"/>
     <w:bottom w:val="single" w:sz="8" w:color="000000"/>
     <w:right w:val="single" w:sz="8" w:color="000000"/>
     <w:insideH w:val="single" w:sz="8" w:color="000000"/>
    </w:tblBorders>
    <w:tblCellMar>
     <w:top w:w="120" w:type="dxa"/><w:left w:w="120" w:type="dxa"/>
     <w:bottom w:w="120" w:type="dxa"/><w:right w:w="120" w:type="dxa"/>
    </w:tblCellMar>
   </w:tblPr>
   <w:tblGrid><w:gridCol w:w="6000"/></w:tblGrid>
   <w:tr><w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/></w:tcPr>
    <w:p><w:r><w:t>{split_row_text}</w:t></w:r></w:p>
   </w:tc></w:tr>
   <w:tr><w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/><w:shd w:val="clear" w:fill="DDEEFF"/></w:tcPr>
    <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>SUCCESSOR ROW ONE</w:t></w:r></w:p>
   </w:tc></w:tr>
   <w:tr><w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/><w:shd w:val="clear" w:fill="FFEEDD"/></w:tcPr>
    <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>SUCCESSOR ROW TWO</w:t></w:r></w:p>
   </w:tc></w:tr>
  </w:tbl>
  <w:sectPr>
   <w:pgSz w:w="7200" w:h="7200"/>
   <w:pgMar w:top="600" w:right="600" w:bottom="600" w:left="600" w:header="300" w:footer="300"/>
  </w:sectPr>
 </w:body>
</w:document>"#,
    )
    .into_bytes()
}

fn required_stored_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "[Content_Types].xml".to_owned(),
            CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "word/document.xml".to_owned(),
            DOCUMENT.to_vec(),
            CompressionMethod::Stored,
        ),
    ]
}

fn package(
    entries: &[(String, Vec<u8>, CompressionMethod)],
) -> Result<Vec<u8>, zip::result::ZipError> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes, compression) in entries {
        writer.start_file(
            name,
            SimpleFileOptions::default().compression_method(*compression),
        )?;
        writer.write_all(bytes)?;
    }
    Ok(writer.finish()?.into_inner())
}

fn patch_fourth_name_as_document(bytes: &mut [u8]) -> Result<(), &'static str> {
    const DOCUMENT_PART: &[u8] = b"word/document.xml";
    let central = central_record_positions(bytes)?;
    let duplicate_central = *central.get(3).ok_or("missing fourth central record")?;
    let duplicate_local =
        usize::try_from(read_u32(bytes, duplicate_central + 42)?).map_err(|_| "large offset")?;
    let central_name = duplicate_central + 46;
    let local_name = duplicate_local + 30;
    bytes
        .get_mut(central_name..central_name + DOCUMENT_PART.len())
        .ok_or("central name outside fixture")?
        .copy_from_slice(DOCUMENT_PART);
    bytes
        .get_mut(local_name..local_name + DOCUMENT_PART.len())
        .ok_or("local name outside fixture")?
        .copy_from_slice(DOCUMENT_PART);
    Ok(())
}

fn central_record_positions(bytes: &[u8]) -> Result<Vec<usize>, &'static str> {
    let eocd = bytes
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .ok_or("missing EOCD")?;
    let mut cursor = usize::try_from(read_u32(bytes, eocd + 16)?).map_err(|_| "large directory")?;
    let entries = usize::from(read_u16(bytes, eocd + 10)?);
    let mut positions = Vec::new();
    for _ in 0..entries {
        positions.push(cursor);
        let name = usize::from(read_u16(bytes, cursor + 28)?);
        let extra = usize::from(read_u16(bytes, cursor + 30)?);
        let comment = usize::from(read_u16(bytes, cursor + 32)?);
        cursor += 46 + name + extra + comment;
    }
    Ok(positions)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, &'static str> {
    let value = bytes.get(offset..offset + 2).ok_or("short fixture")?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, &'static str> {
    let value = bytes.get(offset..offset + 4).ok_or("short fixture")?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}
