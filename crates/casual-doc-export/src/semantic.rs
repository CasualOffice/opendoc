//! Semantic DOCX writer: serialize a v1 `Document` model back to a valid,
//! editable WordprocessingML package (the dual of the Retention byte-copy
//! writer). Written today: the core body (`w:document`/`w:body`, paragraphs,
//! runs, the mapped run/paragraph properties, tabs/breaks), tables (grid +
//! table/row/cell properties, merges, nested tables), and the self-contained
//! inline constructs (hyperlinks with `document.xml.rels` generation, fields,
//! bookmarks, tracked-change revisions, inline content controls), and the
//! definition parts (`fontTable.xml`, `theme1.xml` font scheme, `styles.xml`
//! with `w:pStyle`/`w:rStyle`, `numbering.xml` with body `w:numPr`, footnotes/
//! endnotes and comments with their body references and per-part hyperlink
//! rels). The media-backed constructs (drawings, text boxes) and headers/footers
//! land in later slices; a model that uses them is written with what is
//! supported and the rest is skipped (never a malformed part).
//!
//! Output is byte-deterministic for a given model: parts in fixed order, a fixed
//! ZIP timestamp, ids/relationships re-minted in document order.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use casual_doc_model::v1::{
    AbstractNumbering, AbstractNumberingId, Alignment, BlockNode, BorderEdge, BreakKind,
    CellVerticalAlignment, Color, Comment, CommentId, DefinitionMap, Definitions, Document,
    EmphasisMark, FontCollection, FontDescriptor, FontFamilyKind, FontPitch, FontRef, FontScheme,
    HeaderFooterId, HeaderFooterKind, HeightRule, HighlightColor, HyperlinkTarget, InlineNode,
    Note, NoteId, NoteKind, NumberingInstance, NumberingInstanceId, ParagraphProperties,
    RevisionKind, RgbColor, RunFontHint, RunProperties, SdtControlKind, SdtProperties,
    SectionBoundary, Style, StyleId, StyleKind, TabAlignment, TabLeader, Table, TableBorders,
    TableCell, TableCellProperties, TableLayout, TableProperties, TableRow, TableRowProperties,
    TextDirection, ThemeFontRef, VerticalAlignment, VerticalMerge,
};
use quick_xml::Writer;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::ExportError;

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const CT_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const DOC_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const DOC_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const HYPERLINK_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
const FONT_TABLE_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable";
const FONT_TABLE_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml";
const THEME_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
const THEME_CT: &str = "application/vnd.openxmlformats-officedocument.theme+xml";
const STYLES_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const STYLES_CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
const NUMBERING_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
const NUMBERING_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml";
const FOOTNOTES_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes";
const FOOTNOTES_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml";
const ENDNOTES_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes";
const ENDNOTES_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml";
const COMMENTS_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const COMMENTS_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml";
const HEADER_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";
const HEADER_CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const FOOTER_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer";
const FOOTER_CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
const A_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

/// A package part beyond `document.xml` (fontTable, theme, styles, …): its part
/// name, content type, and the relationship (type + `word/`-relative target)
/// that references it from `document.xml.rels`. `rId`s are assigned after the
/// hyperlink relationships.
struct ExtraPart {
    part_name: String,
    content_type: &'static str,
    rel_type: &'static str,
    target: String,
    bytes: Vec<u8>,
    /// An explicit relationship id (headers/footers derive it from their id so a
    /// `w:sectPr` reference can name it without threading a map). `None` = assign
    /// a fresh `rId` after the hyperlink relationships.
    rel_id: Option<String>,
    /// The part's own external relationships (a hyperlink inside a note/comment/
    /// header resolves through `word/_rels/<part>.rels`, not the document's).
    own_rels: Vec<RelEntry>,
}

impl ExtraPart {
    fn new(
        part_name: &str,
        content_type: &'static str,
        rel_type: &'static str,
        target: &str,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            part_name: part_name.to_owned(),
            content_type,
            rel_type,
            target: target.to_owned(),
            bytes,
            rel_id: None,
            own_rels: Vec::new(),
        }
    }

    fn with_own_rels(mut self, own_rels: Vec<RelEntry>) -> Self {
        self.own_rels = own_rels;
        self
    }

    fn with_rel_id(mut self, rel_id: String) -> Self {
        self.rel_id = Some(rel_id);
        self
    }
}

/// A `document.xml.rels` entry: (relationship id, external target URL).
type RelEntry = (String, String);

/// Accumulates the `word/_rels/document.xml.rels` entries the body needs while
/// it is written. Today that is one external-target relationship per distinct
/// hyperlink URL; media/part relationships join in later slices. `rId`s are
/// assigned in document order so the package is byte-deterministic, and a URL
/// seen twice reuses its `rId` (harmless to the round-trip, which stores the URL
/// not the id).
struct RelBuilder {
    next: u32,
    by_url: BTreeMap<String, String>,
    entries: Vec<RelEntry>,
}

impl RelBuilder {
    fn new() -> Self {
        Self {
            next: 0,
            by_url: BTreeMap::new(),
            entries: Vec::new(),
        }
    }

    /// Returns the `rId` for an external hyperlink URL, minting a new one (and
    /// recording the relationship) the first time each URL is seen.
    fn hyperlink(&mut self, url: &str) -> String {
        if let Some(id) = self.by_url.get(url) {
            return id.clone();
        }
        self.next += 1;
        let id = format!("rId{}", self.next);
        self.by_url.insert(url.to_string(), id.clone());
        self.entries.push((id.clone(), url.to_string()));
        id
    }
}

/// Threaded write context: the bookmark-name source (bookmark markers carry only
/// a `BookmarkId`; the name lives in `Definitions`) and the relationship
/// accumulator (external hyperlinks).
struct Ctx<'a> {
    defs: &'a Definitions,
    rels: RelBuilder,
}

/// Serializes a v1 `Document` to a DOCX package. `media` supplies binary image
/// bytes by part name (the model carries `MediaReference` metadata, not bytes);
/// pass an empty map when the model uses no drawings.
pub fn write_document(
    document: &Document,
    _media: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, ExportError> {
    let (document_xml, rels) = document_xml(document)?;
    let definitions = document.definitions();

    // Extra parts beyond document.xml, each carrying its content-type override
    // and a document relationship; they appear only when the model has the
    // content (byte-identical to the earlier slices otherwise).
    let mut extras: Vec<ExtraPart> = Vec::new();
    if !definitions.font_table.is_empty() {
        extras.push(ExtraPart::new(
            "word/fontTable.xml",
            FONT_TABLE_CT,
            FONT_TABLE_REL_TYPE,
            "fontTable.xml",
            font_table_xml(&definitions.font_table)?,
        ));
    }
    if let Some(scheme) = &definitions.font_scheme {
        extras.push(ExtraPart::new(
            "word/theme/theme1.xml",
            THEME_CT,
            THEME_REL_TYPE,
            "theme/theme1.xml",
            theme_xml(scheme)?,
        ));
    }
    if !definitions.styles.is_empty() {
        extras.push(ExtraPart::new(
            "word/styles.xml",
            STYLES_CT,
            STYLES_REL_TYPE,
            "styles.xml",
            styles_xml(&definitions.styles)?,
        ));
    }
    if !definitions.abstract_numbering.is_empty() || !definitions.numbering.is_empty() {
        extras.push(ExtraPart::new(
            "word/numbering.xml",
            NUMBERING_CT,
            NUMBERING_REL_TYPE,
            "numbering.xml",
            numbering_xml(&definitions.abstract_numbering, &definitions.numbering)?,
        ));
    }
    if !definitions.footnotes.is_empty() {
        let (bytes, own_rels) = notes_xml(
            "w:footnotes",
            "w:footnote",
            &definitions.footnotes,
            definitions,
        )?;
        extras.push(
            ExtraPart::new(
                "word/footnotes.xml",
                FOOTNOTES_CT,
                FOOTNOTES_REL_TYPE,
                "footnotes.xml",
                bytes,
            )
            .with_own_rels(own_rels),
        );
    }
    if !definitions.endnotes.is_empty() {
        let (bytes, own_rels) = notes_xml(
            "w:endnotes",
            "w:endnote",
            &definitions.endnotes,
            definitions,
        )?;
        extras.push(
            ExtraPart::new(
                "word/endnotes.xml",
                ENDNOTES_CT,
                ENDNOTES_REL_TYPE,
                "endnotes.xml",
                bytes,
            )
            .with_own_rels(own_rels),
        );
    }
    if !definitions.comments.is_empty() {
        let (bytes, own_rels) = comments_xml(&definitions.comments, definitions)?;
        extras.push(
            ExtraPart::new(
                "word/comments.xml",
                COMMENTS_CT,
                COMMENTS_REL_TYPE,
                "comments.xml",
                bytes,
            )
            .with_own_rels(own_rels),
        );
    }
    // Headers then footers, each a part with an id-derived relationship id the
    // section's `w:sectPr` references. Emitted in ascending-id order so the
    // importer (which keys by relationship order) re-allocates matching ids.
    for (index, (id, header)) in definitions.headers.iter().enumerate() {
        let (bytes, own_rels) = header_footer_xml("w:hdr", &header.blocks, definitions)?;
        extras.push(
            ExtraPart::new(
                &format!("word/header{}.xml", index + 1),
                HEADER_CT,
                HEADER_REL_TYPE,
                &format!("header{}.xml", index + 1),
                bytes,
            )
            .with_rel_id(hf_rel_id(*id))
            .with_own_rels(own_rels),
        );
    }
    for (index, (id, footer)) in definitions.footers.iter().enumerate() {
        let (bytes, own_rels) = header_footer_xml("w:ftr", &footer.blocks, definitions)?;
        extras.push(
            ExtraPart::new(
                &format!("word/footer{}.xml", index + 1),
                FOOTER_CT,
                FOOTER_REL_TYPE,
                &format!("footer{}.xml", index + 1),
                bytes,
            )
            .with_rel_id(hf_rel_id(*id))
            .with_own_rels(own_rels),
        );
    }

    // Parts are emitted in a deterministic order so the package bytes are
    // reproducible.
    let mut parts: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".to_owned(),
            content_types_xml(&extras)?,
        ),
        ("_rels/.rels".to_owned(), root_rels_xml()?),
        ("word/document.xml".to_owned(), document_xml),
        (
            "word/_rels/document.xml.rels".to_owned(),
            document_rels_xml(&rels, &extras)?,
        ),
    ];
    for extra in extras {
        if !extra.own_rels.is_empty() {
            parts.push((
                rels_part_name(&extra.part_name),
                part_rels_xml(&extra.own_rels)?,
            ));
        }
        parts.push((extra.part_name, extra.bytes));
    }

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::default());
    for (name, bytes) in parts {
        writer
            .start_file(name, options)
            .map_err(|_| ExportError::Package)?;
        writer.write_all(&bytes).map_err(|_| ExportError::Package)?;
    }
    Ok(writer
        .finish()
        .map_err(|_| ExportError::Package)?
        .into_inner())
}

fn new_writer() -> Writer<Cursor<Vec<u8>>> {
    Writer::new(Cursor::new(Vec::new()))
}

fn finish(writer: Writer<Cursor<Vec<u8>>>) -> Vec<u8> {
    writer.into_inner().into_inner()
}

fn start<'a>(name: &'a str) -> BytesStart<'a> {
    BytesStart::new(name)
}

/// Emits `[Content_Types].xml` with the standard defaults plus the main-document
/// override.
fn content_types_xml(extras: &[ExtraPart]) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut types = start("Types");
    types.push_attribute(("xmlns", CT_NS));
    w.write_event(Event::Start(types)).map_err(pkg)?;
    for (ext, ct) in [
        (
            "rels",
            "application/vnd.openxmlformats-package.relationships+xml",
        ),
        ("xml", "application/xml"),
    ] {
        let mut d = start("Default");
        d.push_attribute(("Extension", ext));
        d.push_attribute(("ContentType", ct));
        w.write_event(Event::Empty(d)).map_err(pkg)?;
    }
    let mut over = start("Override");
    over.push_attribute(("PartName", "/word/document.xml"));
    over.push_attribute(("ContentType", DOC_CT));
    w.write_event(Event::Empty(over)).map_err(pkg)?;
    for extra in extras {
        let part_name = format!("/{}", extra.part_name);
        let mut over = start("Override");
        over.push_attribute(("PartName", part_name.as_str()));
        over.push_attribute(("ContentType", extra.content_type));
        w.write_event(Event::Empty(over)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("Types")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits `_rels/.rels` pointing at the main document.
fn root_rels_xml() -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut rels = start("Relationships");
    rels.push_attribute(("xmlns", REL_NS));
    w.write_event(Event::Start(rels)).map_err(pkg)?;
    let mut rel = start("Relationship");
    rel.push_attribute(("Id", "rId1"));
    rel.push_attribute(("Type", DOC_REL_TYPE));
    rel.push_attribute(("Target", "word/document.xml"));
    w.write_event(Event::Empty(rel)).map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("Relationships")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits `word/_rels/document.xml.rels`. With no entries it is the empty
/// `<Relationships/>` element (byte-identical to the earlier no-relationship
/// slices); with entries it carries one external hyperlink relationship each.
fn document_rels_xml(entries: &[RelEntry], extras: &[ExtraPart]) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut rels = start("Relationships");
    rels.push_attribute(("xmlns", REL_NS));
    if entries.is_empty() && extras.is_empty() {
        w.write_event(Event::Empty(rels)).map_err(pkg)?;
        return Ok(finish(w));
    }
    w.write_event(Event::Start(rels)).map_err(pkg)?;
    for (id, url) in entries {
        let mut rel = start("Relationship");
        rel.push_attribute(("Id", id.as_str()));
        rel.push_attribute(("Type", HYPERLINK_REL_TYPE));
        rel.push_attribute(("Target", url.as_str()));
        rel.push_attribute(("TargetMode", "External"));
        w.write_event(Event::Empty(rel)).map_err(pkg)?;
    }
    // Internal relationships after the hyperlink ids. A part with an explicit
    // `rel_id` (headers/footers) uses it; the rest get a fresh sequential `rId`.
    // The importer resolves single parts by relationship type; headers/footers
    // are keyed by the ORDER their `/header`/`/footer` relationships appear, so
    // emit them in ascending-id order (the extras are built that way).
    let mut next = entries.len();
    for extra in extras {
        let id = match &extra.rel_id {
            Some(id) => id.clone(),
            None => {
                next += 1;
                format!("rId{next}")
            }
        };
        let mut rel = start("Relationship");
        rel.push_attribute(("Id", id.as_str()));
        rel.push_attribute(("Type", extra.rel_type));
        rel.push_attribute(("Target", extra.target.as_str()));
        w.write_event(Event::Empty(rel)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("Relationships")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// The `_rels` part name for a part, e.g. `word/footnotes.xml` ->
/// `word/_rels/footnotes.xml.rels`.
fn rels_part_name(part_name: &str) -> String {
    match part_name.rfind('/') {
        Some(slash) => format!(
            "{}/_rels/{}.rels",
            &part_name[..slash],
            &part_name[slash + 1..]
        ),
        None => format!("_rels/{part_name}.rels"),
    }
}

/// Emits a part-own relationships file carrying external hyperlink targets (used
/// by a note/comment part whose content contains a hyperlink).
fn part_rels_xml(entries: &[RelEntry]) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut rels = start("Relationships");
    rels.push_attribute(("xmlns", REL_NS));
    w.write_event(Event::Start(rels)).map_err(pkg)?;
    for (id, url) in entries {
        let mut rel = start("Relationship");
        rel.push_attribute(("Id", id.as_str()));
        rel.push_attribute(("Type", HYPERLINK_REL_TYPE));
        rel.push_attribute(("Target", url.as_str()));
        rel.push_attribute(("TargetMode", "External"));
        w.write_event(Event::Empty(rel)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("Relationships")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits a footnotes/endnotes part: each note keyed by a `w:id` derived from its
/// `NoteId`, wrapping the note's blocks. A fresh per-part `Ctx` routes a note's
/// hyperlinks into the part's own rels (returned), not the document's.
fn notes_xml(
    root: &str,
    item: &str,
    notes: &DefinitionMap<NoteId, Note>,
    defs: &Definitions,
) -> Result<(Vec<u8>, Vec<RelEntry>), ExportError> {
    let mut w = new_writer();
    let mut ctx = Ctx {
        defs,
        rels: RelBuilder::new(),
    };
    let mut r = start(root);
    r.push_attribute(("xmlns:w", W_NS));
    r.push_attribute(("xmlns:r", R_NS));
    w.write_event(Event::Start(r)).map_err(pkg)?;
    for (id, note) in notes.iter() {
        let mut el = start(item);
        el.push_attribute(("w:id", note_id_token(*id).as_str()));
        w.write_event(Event::Start(el)).map_err(pkg)?;
        for block in &note.blocks {
            write_block(&mut w, block, &mut ctx)?;
        }
        w.write_event(Event::End(BytesEnd::new(item)))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new(root)))
        .map_err(pkg)?;
    Ok((finish(w), ctx.rels.entries))
}

/// Emits `word/comments.xml`: each comment keyed by a `w:id` derived from its
/// `CommentId`, with author/initials/date attributes, wrapping its blocks.
fn comments_xml(
    comments: &DefinitionMap<CommentId, Comment>,
    defs: &Definitions,
) -> Result<(Vec<u8>, Vec<RelEntry>), ExportError> {
    let mut w = new_writer();
    let mut ctx = Ctx {
        defs,
        rels: RelBuilder::new(),
    };
    let mut r = start("w:comments");
    r.push_attribute(("xmlns:w", W_NS));
    r.push_attribute(("xmlns:r", R_NS));
    w.write_event(Event::Start(r)).map_err(pkg)?;
    for (id, comment) in comments.iter() {
        let mut el = start("w:comment");
        el.push_attribute(("w:id", comment_id_token(*id).as_str()));
        if let Some(author) = &comment.author {
            el.push_attribute(("w:author", author.as_str()));
        }
        if let Some(initials) = &comment.initials {
            el.push_attribute(("w:initials", initials.as_str()));
        }
        if let Some(date) = &comment.date {
            el.push_attribute(("w:date", date.as_str()));
        }
        w.write_event(Event::Start(el)).map_err(pkg)?;
        for block in &comment.blocks {
            write_block(&mut w, block, &mut ctx)?;
        }
        w.write_event(Event::End(BytesEnd::new("w:comment")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:comments")))
        .map_err(pkg)?;
    Ok((finish(w), ctx.rels.entries))
}

fn note_id_token(id: NoteId) -> String {
    id.node_id().as_u128().to_string()
}

fn comment_id_token(id: CommentId) -> String {
    id.node_id().as_u128().to_string()
}

/// Emits a header or footer part (`w:hdr`/`w:ftr`) from its blocks. Uses a fresh
/// per-part `Ctx` so a hyperlink inside routes to the part's own rels (returned).
fn header_footer_xml(
    root: &str,
    blocks: &[BlockNode],
    defs: &Definitions,
) -> Result<(Vec<u8>, Vec<RelEntry>), ExportError> {
    let mut w = new_writer();
    let mut ctx = Ctx {
        defs,
        rels: RelBuilder::new(),
    };
    let mut r = start(root);
    r.push_attribute(("xmlns:w", W_NS));
    r.push_attribute(("xmlns:r", R_NS));
    w.write_event(Event::Start(r)).map_err(pkg)?;
    for block in blocks {
        write_block(&mut w, block, &mut ctx)?;
    }
    w.write_event(Event::End(BytesEnd::new(root)))
        .map_err(pkg)?;
    Ok((finish(w), ctx.rels.entries))
}

/// The relationship id a `w:sectPr` uses to reference a header/footer part,
/// derived from its id so the reference and the part's relationship agree
/// without threading a map. The `rIdHf` prefix avoids the `rId{n}` ids.
fn hf_rel_id(id: HeaderFooterId) -> String {
    format!("rIdHf{}", id.node_id().as_u128())
}

fn hf_kind_token(kind: HeaderFooterKind) -> &'static str {
    match kind {
        HeaderFooterKind::Default => "default",
        HeaderFooterKind::First => "first",
        HeaderFooterKind::Even => "even",
    }
}

/// Emits `word/fontTable.xml` from the model's font descriptors, in order.
fn font_table_xml(fonts: &[FontDescriptor]) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("w:fonts");
    root.push_attribute(("xmlns:w", W_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    for font in fonts {
        let mut el = start("w:font");
        el.push_attribute(("w:name", font.name.as_str()));
        w.write_event(Event::Start(el)).map_err(pkg)?;
        for (value, name) in [
            (&font.alt_name, "w:altName"),
            (&font.panose1, "w:panose1"),
            (&font.charset, "w:charset"),
        ] {
            if let Some(value) = value {
                let mut child = start(name);
                child.push_attribute(("w:val", value.as_str()));
                w.write_event(Event::Empty(child)).map_err(pkg)?;
            }
        }
        if let Some(family) = font.family {
            let mut child = start("w:family");
            child.push_attribute(("w:val", font_family_token(family)));
            w.write_event(Event::Empty(child)).map_err(pkg)?;
        }
        if let Some(pitch) = font.pitch {
            let mut child = start("w:pitch");
            child.push_attribute(("w:val", font_pitch_token(pitch)));
            w.write_event(Event::Empty(child)).map_err(pkg)?;
        }
        if !font.sig.is_empty() {
            let mut child = start("w:sig");
            for (value, name) in [
                (&font.sig.usb0, "w:usb0"),
                (&font.sig.usb1, "w:usb1"),
                (&font.sig.usb2, "w:usb2"),
                (&font.sig.usb3, "w:usb3"),
                (&font.sig.csb0, "w:csb0"),
                (&font.sig.csb1, "w:csb1"),
            ] {
                if let Some(value) = value {
                    child.push_attribute((name, value.as_str()));
                }
            }
            w.write_event(Event::Empty(child)).map_err(pkg)?;
        }
        if font.not_true_type {
            w.write_event(Event::Empty(start("w:notTrueType")))
                .map_err(pkg)?;
        }
        w.write_event(Event::End(BytesEnd::new("w:font")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:fonts")))
        .map_err(pkg)?;
    Ok(finish(w))
}

fn font_family_token(family: FontFamilyKind) -> &'static str {
    match family {
        FontFamilyKind::Auto => "auto",
        FontFamilyKind::Decorative => "decorative",
        FontFamilyKind::Modern => "modern",
        FontFamilyKind::Roman => "roman",
        FontFamilyKind::Script => "script",
        FontFamilyKind::Swiss => "swiss",
    }
}

fn font_pitch_token(pitch: FontPitch) -> &'static str {
    match pitch {
        FontPitch::Default => "default",
        FontPitch::Fixed => "fixed",
        FontPitch::Variable => "variable",
    }
}

/// Emits `word/theme/theme1.xml` carrying just the font scheme (the colour and
/// format schemes are the byte-floor's concern, not the semantic model's).
fn theme_xml(scheme: &FontScheme) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("a:theme");
    root.push_attribute(("xmlns:a", A_NS));
    root.push_attribute(("name", "Office Theme"));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    w.write_event(Event::Start(start("a:themeElements")))
        .map_err(pkg)?;
    let mut font_scheme = start("a:fontScheme");
    font_scheme.push_attribute(("name", "Office"));
    w.write_event(Event::Start(font_scheme)).map_err(pkg)?;
    write_font_collection(&mut w, "a:majorFont", &scheme.major)?;
    write_font_collection(&mut w, "a:minorFont", &scheme.minor)?;
    w.write_event(Event::End(BytesEnd::new("a:fontScheme")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("a:themeElements")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("a:theme")))
        .map_err(pkg)?;
    Ok(finish(w))
}

fn write_font_collection(
    w: &mut Writer<Cursor<Vec<u8>>>,
    tag: &str,
    collection: &FontCollection,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start(tag))).map_err(pkg)?;
    for (entry_tag, entry) in [
        ("a:latin", &collection.latin),
        ("a:ea", &collection.ea),
        ("a:cs", &collection.cs),
    ] {
        let mut el = start(entry_tag);
        el.push_attribute(("typeface", entry.typeface.as_str()));
        if let Some(value) = &entry.panose {
            el.push_attribute(("panose", value.as_str()));
        }
        if let Some(value) = &entry.pitch_family {
            el.push_attribute(("pitchFamily", value.as_str()));
        }
        if let Some(value) = &entry.charset {
            el.push_attribute(("charset", value.as_str()));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    for over in &collection.script_overrides {
        let mut el = start("a:font");
        el.push_attribute(("script", over.script.as_str()));
        el.push_attribute(("typeface", over.typeface.as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new(tag))).map_err(pkg)?;
    Ok(())
}

/// Emits `word/styles.xml` from the style definitions. The `w:styleId` string is
/// derived from the internal `StyleId` (as bookmarks derive `w:id`), so a body
/// `w:pStyle`/`w:rStyle` and a `w:basedOn` that reference the same id emit the
/// same string and re-import to the same `StyleId`.
fn styles_xml(styles: &DefinitionMap<StyleId, Style>) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("w:styles");
    root.push_attribute(("xmlns:w", W_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    for (id, style) in styles.iter() {
        let style_id = style_id_token(*id);
        let mut el = start("w:style");
        el.push_attribute((
            "w:type",
            match style.kind {
                StyleKind::Paragraph => "paragraph",
                StyleKind::Character => "character",
            },
        ));
        el.push_attribute(("w:styleId", style_id.as_str()));
        w.write_event(Event::Start(el)).map_err(pkg)?;
        // The importer ignores `w:name`, but Word requires one; emit the id.
        let mut name = start("w:name");
        name.push_attribute(("w:val", style_id.as_str()));
        w.write_event(Event::Empty(name)).map_err(pkg)?;
        if let Some(based_on) = style.based_on {
            let mut el = start("w:basedOn");
            el.push_attribute(("w:val", style_id_token(based_on).as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        // A `Some(default)` pPr/rPr must still emit its (empty) element: the
        // importer keys on the tag's PRESENCE (Some vs None), while the property
        // writers elide an all-default value — so emit a bare `<w:pPr/>`/`<w:rPr/>`
        // for the default case to preserve presence across the round trip.
        if let Some(paragraph) = &style.paragraph {
            if *paragraph == ParagraphProperties::default() {
                w.write_event(Event::Empty(start("w:pPr"))).map_err(pkg)?;
            } else {
                write_paragraph_properties(&mut w, paragraph)?;
            }
        }
        if let Some(run) = &style.run {
            if *run == RunProperties::default() {
                w.write_event(Event::Empty(start("w:rPr"))).map_err(pkg)?;
            } else {
                write_run_properties(&mut w, run)?;
            }
        }
        w.write_event(Event::End(BytesEnd::new("w:style")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:styles")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// The `w:styleId`/`w:val` string for a style, derived from its internal id so
/// references reproduce it deterministically.
fn style_id_token(id: StyleId) -> String {
    id.node_id().as_u128().to_string()
}

/// Emits `word/numbering.xml`: the abstract definitions (each with its levels'
/// start values) then the numbering instances. The `w:abstractNumId`/`w:numId`
/// strings derive from the internal ids so a `w:num`'s `w:abstractNumId` and a
/// body `w:numPr`'s `w:numId` reference the same string and re-import to the
/// same ids. Only the modeled level detail (index + start) is emitted; other
/// `w:lvl` detail is not modeled (and the importer does not read it back).
fn numbering_xml(
    abstracts: &DefinitionMap<AbstractNumberingId, AbstractNumbering>,
    instances: &DefinitionMap<NumberingInstanceId, NumberingInstance>,
) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("w:numbering");
    root.push_attribute(("xmlns:w", W_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    for (id, abstract_num) in abstracts.iter() {
        let mut el = start("w:abstractNum");
        el.push_attribute(("w:abstractNumId", abstract_id_token(*id).as_str()));
        w.write_event(Event::Start(el)).map_err(pkg)?;
        for level in &abstract_num.levels {
            let mut lvl = start("w:lvl");
            lvl.push_attribute(("w:ilvl", level.level.to_string().as_str()));
            w.write_event(Event::Start(lvl)).map_err(pkg)?;
            let mut s = start("w:start");
            s.push_attribute(("w:val", level.start.to_string().as_str()));
            w.write_event(Event::Empty(s)).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:lvl")))
                .map_err(pkg)?;
        }
        w.write_event(Event::End(BytesEnd::new("w:abstractNum")))
            .map_err(pkg)?;
    }
    for (id, instance) in instances.iter() {
        let mut el = start("w:num");
        el.push_attribute(("w:numId", num_id_token(*id).as_str()));
        w.write_event(Event::Start(el)).map_err(pkg)?;
        let mut a = start("w:abstractNumId");
        a.push_attribute(("w:val", abstract_id_token(instance.abstract_ref).as_str()));
        w.write_event(Event::Empty(a)).map_err(pkg)?;
        w.write_event(Event::End(BytesEnd::new("w:num")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:numbering")))
        .map_err(pkg)?;
    Ok(finish(w))
}

fn abstract_id_token(id: AbstractNumberingId) -> String {
    id.node_id().as_u128().to_string()
}

fn num_id_token(id: NumberingInstanceId) -> String {
    id.node_id().as_u128().to_string()
}

/// Emits `word/document.xml` from the model body, returning the bytes plus the
/// external hyperlink relationships collected while writing (for
/// `document.xml.rels`).
fn document_xml(document: &Document) -> Result<(Vec<u8>, Vec<RelEntry>), ExportError> {
    let mut w = new_writer();
    let mut doc = start("w:document");
    doc.push_attribute(("xmlns:w", W_NS));
    // `xmlns:r` is required so a hyperlink's `r:id` is well-formed; harmless when
    // no relationship-referencing construct is present.
    doc.push_attribute(("xmlns:r", R_NS));
    w.write_event(Event::Start(doc)).map_err(pkg)?;
    w.write_event(Event::Start(start("w:body"))).map_err(pkg)?;

    let mut ctx = Ctx {
        defs: document.definitions(),
        rels: RelBuilder::new(),
    };
    for block in document.body() {
        write_block(&mut w, block, &mut ctx)?;
    }
    // The body-level section (the last, in the common single-section case). Its
    // header/footer references land in a later slice.
    if let Some(section) = document.definitions().sections.last() {
        write_section_properties(&mut w, section)?;
    }

    w.write_event(Event::End(BytesEnd::new("w:body")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:document")))
        .map_err(pkg)?;
    Ok((finish(w), ctx.rels.entries))
}

/// Emits a body-level `w:sectPr` with page geometry. Header/footer references
/// are a later slice.
fn write_section_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    section: &SectionBoundary,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:sectPr")))
        .map_err(pkg)?;
    for (references, element) in [
        (&section.headers, "w:headerReference"),
        (&section.footers, "w:footerReference"),
    ] {
        for reference in references {
            let mut el = start(element);
            el.push_attribute(("w:type", hf_kind_token(reference.kind)));
            el.push_attribute(("r:id", hf_rel_id(reference.reference).as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
    }
    let mut pg_sz = start("w:pgSz");
    pg_sz.push_attribute(("w:w", section.page_size.width_twips.to_string().as_str()));
    pg_sz.push_attribute(("w:h", section.page_size.height_twips.to_string().as_str()));
    w.write_event(Event::Empty(pg_sz)).map_err(pkg)?;
    let mut pg_mar = start("w:pgMar");
    pg_mar.push_attribute(("w:top", section.page_margins.top_twips.to_string().as_str()));
    pg_mar.push_attribute((
        "w:bottom",
        section.page_margins.bottom_twips.to_string().as_str(),
    ));
    pg_mar.push_attribute((
        "w:start",
        section.page_margins.start_twips.to_string().as_str(),
    ));
    pg_mar.push_attribute(("w:end", section.page_margins.end_twips.to_string().as_str()));
    w.write_event(Event::Empty(pg_mar)).map_err(pkg)?;
    let mut cols = start("w:cols");
    cols.push_attribute(("w:num", section.columns.count.to_string().as_str()));
    w.write_event(Event::Empty(cols)).map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:sectPr")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a body/cell block. Content-control block wrappers (`BlockNode::Sdt`)
/// are a later slice — their inner blocks are emitted directly (no text loss).
fn write_block(
    w: &mut Writer<Cursor<Vec<u8>>>,
    block: &BlockNode,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    match block {
        BlockNode::Paragraph(paragraph) => {
            write_paragraph(w, &paragraph.properties, &paragraph.inlines, ctx)
        }
        BlockNode::Table(table) => write_table(w, table, ctx),
        BlockNode::Sdt(sdt) => {
            for inner in &sdt.blocks {
                write_block(w, inner, ctx)?;
            }
            Ok(())
        }
    }
}

fn write_table(
    w: &mut Writer<Cursor<Vec<u8>>>,
    table: &Table,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:tbl"))).map_err(pkg)?;
    write_table_properties(w, &table.properties)?;
    if !table.grid.is_empty() {
        w.write_event(Event::Start(start("w:tblGrid")))
            .map_err(pkg)?;
        for column in &table.grid {
            let mut col = start("w:gridCol");
            if let Some(width) = column.width_twips {
                col.push_attribute(("w:w", width.to_string().as_str()));
            }
            w.write_event(Event::Empty(col)).map_err(pkg)?;
        }
        w.write_event(Event::End(BytesEnd::new("w:tblGrid")))
            .map_err(pkg)?;
    }
    for row in &table.rows {
        write_row(w, row, ctx)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:tbl")))
        .map_err(pkg)?;
    Ok(())
}

fn write_table_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &TableProperties,
) -> Result<(), ExportError> {
    if *properties == TableProperties::default() {
        return Ok(());
    }
    w.write_event(Event::Start(start("w:tblPr"))).map_err(pkg)?;
    if let Some(alignment) = properties.alignment {
        let mut jc = start("w:jc");
        jc.push_attribute(("w:val", alignment_token(alignment)));
        w.write_event(Event::Empty(jc)).map_err(pkg)?;
    }
    if let Some(width) = properties.width_twips {
        let mut el = start("w:tblW");
        el.push_attribute(("w:type", "dxa"));
        el.push_attribute(("w:w", width.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(layout) = properties.layout {
        let mut el = start("w:tblLayout");
        el.push_attribute((
            "w:type",
            match layout {
                TableLayout::Fixed => "fixed",
                TableLayout::Autofit => "autofit",
            },
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if !properties.look.is_empty() {
        let look = &properties.look;
        let mut el = start("w:tblLook");
        for (on, name) in [
            (look.first_row, "w:firstRow"),
            (look.last_row, "w:lastRow"),
            (look.first_column, "w:firstColumn"),
            (look.last_column, "w:lastColumn"),
            (look.no_h_band, "w:noHBand"),
            (look.no_v_band, "w:noVBand"),
        ] {
            if on {
                el.push_attribute((name, "1"));
            }
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    write_borders(w, "w:tblBorders", &properties.borders)?;
    write_shading(w, &properties.shading)?;
    write_margins(w, "w:tblCellMar", &properties.cell_margins)?;
    w.write_event(Event::End(BytesEnd::new("w:tblPr")))
        .map_err(pkg)?;
    Ok(())
}

fn write_row(
    w: &mut Writer<Cursor<Vec<u8>>>,
    row: &TableRow,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:tr"))).map_err(pkg)?;
    write_row_properties(w, &row.properties)?;
    for cell in &row.cells {
        write_cell(w, cell, ctx)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:tr")))
        .map_err(pkg)?;
    Ok(())
}

fn write_row_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &TableRowProperties,
) -> Result<(), ExportError> {
    if *properties == TableRowProperties::default() {
        return Ok(());
    }
    w.write_event(Event::Start(start("w:trPr"))).map_err(pkg)?;
    if !properties.height.is_empty() {
        let mut el = start("w:trHeight");
        if let Some(value) = properties.height.value_twips {
            el.push_attribute(("w:val", value.to_string().as_str()));
        }
        if let Some(rule) = properties.height.rule {
            el.push_attribute((
                "w:hRule",
                match rule {
                    HeightRule::Auto => "auto",
                    HeightRule::AtLeast => "atLeast",
                    HeightRule::Exact => "exact",
                },
            ));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if properties.cant_split {
        w.write_event(Event::Empty(start("w:cantSplit")))
            .map_err(pkg)?;
    }
    if properties.header {
        w.write_event(Event::Empty(start("w:tblHeader")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:trPr")))
        .map_err(pkg)?;
    Ok(())
}

fn write_cell(
    w: &mut Writer<Cursor<Vec<u8>>>,
    cell: &TableCell,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:tc"))).map_err(pkg)?;
    write_cell_properties(w, &cell.properties)?;
    for block in &cell.blocks {
        write_block(w, block, ctx)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:tc")))
        .map_err(pkg)?;
    Ok(())
}

fn write_cell_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &TableCellProperties,
) -> Result<(), ExportError> {
    if *properties == TableCellProperties::default() {
        return Ok(());
    }
    w.write_event(Event::Start(start("w:tcPr"))).map_err(pkg)?;
    if let Some(width) = properties.width_twips {
        let mut el = start("w:tcW");
        el.push_attribute(("w:type", "dxa"));
        el.push_attribute(("w:w", width.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(span) = properties.grid_span {
        let mut el = start("w:gridSpan");
        el.push_attribute(("w:val", span.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(merge) = properties.vertical_merge {
        let mut el = start("w:vMerge");
        // `Continue` is the bare element; `Restart` carries `w:val="restart"`.
        if merge == VerticalMerge::Restart {
            el.push_attribute(("w:val", "restart"));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    write_borders(w, "w:tcBorders", &properties.borders)?;
    write_shading(w, &properties.shading)?;
    write_margins(w, "w:tcMar", &properties.margins)?;
    if let Some(alignment) = properties.vertical_alignment {
        let mut el = start("w:vAlign");
        el.push_attribute((
            "w:val",
            match alignment {
                CellVerticalAlignment::Top => "top",
                CellVerticalAlignment::Center => "center",
                CellVerticalAlignment::Bottom => "bottom",
            },
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if properties.no_wrap {
        w.write_event(Event::Empty(start("w:noWrap")))
            .map_err(pkg)?;
    }
    if let Some(direction) = properties.text_direction {
        let mut el = start("w:textDirection");
        el.push_attribute((
            "w:val",
            match direction {
                TextDirection::LrTb => "lrTb",
                TextDirection::TbRl => "tbRl",
                TextDirection::BtLr => "btLr",
            },
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:tcPr")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a border set (`w:tblBorders`/`w:tcBorders`) if any edge is present.
fn write_borders(
    w: &mut Writer<Cursor<Vec<u8>>>,
    container: &str,
    borders: &TableBorders,
) -> Result<(), ExportError> {
    if borders.is_empty() {
        return Ok(());
    }
    w.write_event(Event::Start(start(container))).map_err(pkg)?;
    for (edge, name) in [
        (&borders.top, "w:top"),
        (&borders.start, "w:start"),
        (&borders.bottom, "w:bottom"),
        (&borders.end, "w:end"),
        (&borders.inside_h, "w:insideH"),
        (&borders.inside_v, "w:insideV"),
    ] {
        if let Some(edge) = edge {
            write_border_edge(w, name, edge)?;
        }
    }
    w.write_event(Event::End(BytesEnd::new(container)))
        .map_err(pkg)?;
    Ok(())
}

fn write_border_edge(
    w: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    edge: &BorderEdge,
) -> Result<(), ExportError> {
    let mut el = start(name);
    el.push_attribute(("w:val", edge.style.as_str()));
    if let Some(size) = edge.size_eighth_points {
        el.push_attribute(("w:sz", size.to_string().as_str()));
    }
    if let Some(color) = &edge.color {
        el.push_attribute(("w:color", rgb_hex(color).as_str()));
    }
    if let Some(space) = edge.space_points {
        el.push_attribute(("w:space", space.to_string().as_str()));
    }
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
}

/// Emits cell margins (`w:tblCellMar`/`w:tcMar`) if any edge is present.
fn write_margins(
    w: &mut Writer<Cursor<Vec<u8>>>,
    container: &str,
    margins: &casual_doc_model::v1::CellMargins,
) -> Result<(), ExportError> {
    if margins.is_empty() {
        return Ok(());
    }
    w.write_event(Event::Start(start(container))).map_err(pkg)?;
    for (value, name) in [
        (margins.top_twips, "w:top"),
        (margins.start_twips, "w:start"),
        (margins.bottom_twips, "w:bottom"),
        (margins.end_twips, "w:end"),
    ] {
        if let Some(value) = value {
            let mut el = start(name);
            el.push_attribute(("w:type", "dxa"));
            el.push_attribute(("w:w", value.to_string().as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
    }
    w.write_event(Event::End(BytesEnd::new(container)))
        .map_err(pkg)?;
    Ok(())
}

fn write_shading(
    w: &mut Writer<Cursor<Vec<u8>>>,
    shading: &casual_doc_model::v1::Shading,
) -> Result<(), ExportError> {
    if let Some(fill) = &shading.fill {
        let mut el = start("w:shd");
        el.push_attribute(("w:val", "clear"));
        el.push_attribute(("w:color", "auto"));
        el.push_attribute(("w:fill", rgb_hex(fill).as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    Ok(())
}

fn rgb_hex(color: &RgbColor) -> String {
    format!("{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

fn write_paragraph(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &ParagraphProperties,
    inlines: &[InlineNode],
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:p"))).map_err(pkg)?;
    write_paragraph_properties(w, properties)?;
    for inline in inlines {
        write_inline(w, inline, ctx, false)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:p")))
        .map_err(pkg)?;
    Ok(())
}

fn write_paragraph_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &ParagraphProperties,
) -> Result<(), ExportError> {
    if *properties == ParagraphProperties::default() {
        return Ok(());
    }
    w.write_event(Event::Start(start("w:pPr"))).map_err(pkg)?;
    if let Some(style_ref) = properties.style_ref {
        let mut el = start("w:pStyle");
        el.push_attribute(("w:val", style_id_token(style_ref).as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(numbering) = &properties.numbering {
        w.write_event(Event::Start(start("w:numPr"))).map_err(pkg)?;
        let mut ilvl = start("w:ilvl");
        ilvl.push_attribute(("w:val", numbering.level.to_string().as_str()));
        w.write_event(Event::Empty(ilvl)).map_err(pkg)?;
        let mut num_id = start("w:numId");
        num_id.push_attribute(("w:val", num_id_token(numbering.instance).as_str()));
        w.write_event(Event::Empty(num_id)).map_err(pkg)?;
        w.write_event(Event::End(BytesEnd::new("w:numPr")))
            .map_err(pkg)?;
    }
    if let Some(alignment) = properties.alignment {
        let mut jc = start("w:jc");
        jc.push_attribute(("w:val", alignment_token(alignment)));
        w.write_event(Event::Empty(jc)).map_err(pkg)?;
    }
    if let Some(indent) = &properties.indentation {
        let mut el = start("w:ind");
        if let Some(v) = indent.start_twips {
            el.push_attribute(("w:start", v.to_string().as_str()));
        }
        if let Some(v) = indent.end_twips {
            el.push_attribute(("w:end", v.to_string().as_str()));
        }
        if let Some(v) = indent.first_line_twips {
            el.push_attribute(("w:firstLine", v.to_string().as_str()));
        }
        if let Some(v) = indent.hanging_twips {
            el.push_attribute(("w:hanging", v.to_string().as_str()));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    for (flag, name) in [
        (properties.keep_next, "w:keepNext"),
        (properties.keep_lines, "w:keepLines"),
        (properties.page_break_before, "w:pageBreakBefore"),
        (properties.widow_control, "w:widowControl"),
        (properties.contextual_spacing, "w:contextualSpacing"),
        (properties.suppress_line_numbers, "w:suppressLineNumbers"),
    ] {
        if flag {
            w.write_event(Event::Empty(start(name))).map_err(pkg)?;
        }
    }
    if let Some(level) = properties.outline_level {
        let mut el = start("w:outlineLvl");
        el.push_attribute(("w:val", level.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(spacing) = &properties.spacing {
        let mut el = start("w:spacing");
        if let Some(before) = spacing.before_twips {
            el.push_attribute(("w:before", before.to_string().as_str()));
        }
        if let Some(after) = spacing.after_twips {
            el.push_attribute(("w:after", after.to_string().as_str()));
        }
        if let Some(percent) = spacing.line_percent {
            // The importer reads `line * 100 / 240` (auto rule); round the twips
            // up so that integer division recovers the exact percent.
            let line = (u64::from(percent) * 240).div_ceil(100);
            el.push_attribute(("w:line", line.to_string().as_str()));
            el.push_attribute(("w:lineRule", "auto"));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    let borders = &properties.borders;
    if borders.top.is_some()
        || borders.bottom.is_some()
        || borders.start.is_some()
        || borders.end.is_some()
        || borders.between.is_some()
        || borders.bar.is_some()
    {
        w.write_event(Event::Start(start("w:pBdr"))).map_err(pkg)?;
        for (edge, name) in [
            (&borders.top, "w:top"),
            (&borders.start, "w:start"),
            (&borders.bottom, "w:bottom"),
            (&borders.end, "w:end"),
            (&borders.between, "w:between"),
            (&borders.bar, "w:bar"),
        ] {
            if let Some(edge) = edge {
                write_border_edge(w, name, edge)?;
            }
        }
        w.write_event(Event::End(BytesEnd::new("w:pBdr")))
            .map_err(pkg)?;
    }
    write_shading(w, &properties.shading)?;
    if !properties.tabs.is_empty() {
        w.write_event(Event::Start(start("w:tabs"))).map_err(pkg)?;
        for tab in &properties.tabs {
            let mut el = start("w:tab");
            el.push_attribute(("w:val", tab_alignment_token(tab.alignment)));
            el.push_attribute(("w:pos", tab.position_twips.to_string().as_str()));
            if let Some(leader) = tab.leader {
                el.push_attribute(("w:leader", tab_leader_token(leader)));
            }
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        w.write_event(Event::End(BytesEnd::new("w:tabs")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:pPr")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits one inline node. `in_deletion` is true when the node sits inside a
/// tracked-change deletion range, so a run's text is written as `w:delText`
/// (which the importer reads back into `run.text`, exactly like `w:t`).
fn write_inline(
    w: &mut Writer<Cursor<Vec<u8>>>,
    inline: &InlineNode,
    ctx: &mut Ctx,
    in_deletion: bool,
) -> Result<(), ExportError> {
    match inline {
        InlineNode::Run(run) => {
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            write_run_properties(w, &run.properties)?;
            let tag = if in_deletion { "w:delText" } else { "w:t" };
            let mut t = start(tag);
            t.push_attribute(("xml:space", "preserve"));
            w.write_event(Event::Start(t)).map_err(pkg)?;
            w.write_event(Event::Text(BytesText::new(&run.text)))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new(tag))).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:r")))
                .map_err(pkg)?;
        }
        InlineNode::Tab(_) => {
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            w.write_event(Event::Empty(start("w:tab"))).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:r")))
                .map_err(pkg)?;
        }
        InlineNode::Break(node) => {
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            let mut br = start("w:br");
            br.push_attribute(("w:type", break_token(node.kind)));
            w.write_event(Event::Empty(br)).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:r")))
                .map_err(pkg)?;
        }
        // A hyperlink is a direct inline child of `w:p` (never inside a `w:r`):
        // an external target resolves through an `r:id` relationship, an internal
        // one through a `w:anchor` bookmark name.
        InlineNode::Hyperlink(link) => {
            let mut el = start("w:hyperlink");
            match &link.target {
                HyperlinkTarget::External(ext) => {
                    let rid = ctx.rels.hyperlink(&ext.url);
                    el.push_attribute(("r:id", rid.as_str()));
                    if let Some(tip) = &link.tooltip {
                        el.push_attribute(("w:tooltip", tip.as_str()));
                    }
                    w.write_event(Event::Start(el)).map_err(pkg)?;
                }
                HyperlinkTarget::Internal(int) => {
                    el.push_attribute(("w:anchor", int.anchor.as_str()));
                    if let Some(tip) = &link.tooltip {
                        el.push_attribute(("w:tooltip", tip.as_str()));
                    }
                    w.write_event(Event::Start(el)).map_err(pkg)?;
                }
            }
            for child in &link.inlines {
                write_inline(w, child, ctx, in_deletion)?;
            }
            w.write_event(Event::End(BytesEnd::new("w:hyperlink")))
                .map_err(pkg)?;
        }
        // A simple field: the instruction inline, the cached result as children.
        InlineNode::Field(field) => {
            let mut el = start("w:fldSimple");
            el.push_attribute(("w:instr", field.instruction.as_str()));
            w.write_event(Event::Start(el)).map_err(pkg)?;
            for child in &field.inlines {
                write_inline(w, child, ctx, in_deletion)?;
            }
            w.write_event(Event::End(BytesEnd::new("w:fldSimple")))
                .map_err(pkg)?;
        }
        // A tracked-change range. Its own runs are deleted text when this is a
        // deletion (or when already inside one); insertions keep the flag.
        InlineNode::Revision(revision) => {
            let (name, deleted) = match revision.kind {
                RevisionKind::Insertion => ("w:ins", in_deletion),
                RevisionKind::Deletion => ("w:del", true),
            };
            let mut el = start(name);
            if let Some(author) = &revision.author {
                el.push_attribute(("w:author", author.as_str()));
            }
            if let Some(date) = &revision.date {
                el.push_attribute(("w:date", date.as_str()));
            }
            if let Some(id) = &revision.revision_id {
                el.push_attribute(("w:id", id.as_str()));
            }
            w.write_event(Event::Start(el)).map_err(pkg)?;
            for child in &revision.inlines {
                write_inline(w, child, ctx, deleted)?;
            }
            w.write_event(Event::End(BytesEnd::new(name)))
                .map_err(pkg)?;
        }
        // A bookmark range marker (zero-width). The pairing key `w:id` derives
        // from the shared `BookmarkId`; the name lives in `Definitions`.
        InlineNode::BookmarkStart(marker) => {
            if let Some(bookmark) = ctx.defs.bookmarks.get(&marker.bookmark) {
                let id = marker.bookmark.node_id().as_u128().to_string();
                let mut el = start("w:bookmarkStart");
                el.push_attribute(("w:id", id.as_str()));
                el.push_attribute(("w:name", bookmark.name.as_str()));
                w.write_event(Event::Empty(el)).map_err(pkg)?;
            }
        }
        InlineNode::BookmarkEnd(marker) => {
            let id = marker.bookmark.node_id().as_u128().to_string();
            let mut el = start("w:bookmarkEnd");
            el.push_attribute(("w:id", id.as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        // An inline content control: typed `w:sdtPr` (omitted when default) plus
        // `w:sdtContent` wrapping the children.
        InlineNode::Sdt(sdt) => {
            w.write_event(Event::Start(start("w:sdt"))).map_err(pkg)?;
            write_sdt_properties(w, &sdt.properties)?;
            w.write_event(Event::Start(start("w:sdtContent")))
                .map_err(pkg)?;
            for child in &sdt.inlines {
                write_inline(w, child, ctx, in_deletion)?;
            }
            w.write_event(Event::End(BytesEnd::new("w:sdtContent")))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:sdt")))
                .map_err(pkg)?;
        }
        // A footnote/endnote reference (run-level), by the id derived from the
        // note it points at.
        InlineNode::NoteReference(note_ref) => {
            let element = match note_ref.kind {
                NoteKind::Footnote => "w:footnoteReference",
                NoteKind::Endnote => "w:endnoteReference",
            };
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            let mut el = start(element);
            el.push_attribute(("w:id", note_id_token(note_ref.note).as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:r")))
                .map_err(pkg)?;
        }
        InlineNode::CommentReference(comment_ref) => {
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            let mut el = start("w:commentReference");
            el.push_attribute(("w:id", comment_id_token(comment_ref.comment).as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:r")))
                .map_err(pkg)?;
        }
        // Drawings and text boxes depend on the binary-media path (a later slice).
        InlineNode::Drawing(_) | InlineNode::TextBox(_) => {}
    }
    Ok(())
}

/// Emits `w:sdtPr` for an inline content control, or nothing when the control
/// carries no modeled properties. Children are ordered per `CT_SdtPr` (alias,
/// tag, id, type marker) for Word validity; the importer is order-independent.
fn write_sdt_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &SdtProperties,
) -> Result<(), ExportError> {
    if *properties == SdtProperties::default() {
        return Ok(());
    }
    w.write_event(Event::Start(start("w:sdtPr"))).map_err(pkg)?;
    for (value, name) in [
        (&properties.alias, "w:alias"),
        (&properties.tag, "w:tag"),
        (&properties.control_id, "w:id"),
    ] {
        if let Some(value) = value {
            let mut el = start(name);
            el.push_attribute(("w:val", value.as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
    }
    if let Some(kind) = properties.control_kind {
        w.write_event(Event::Empty(start(sdt_kind_element(kind))))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:sdtPr")))
        .map_err(pkg)?;
    Ok(())
}

/// Maps a content-control kind to its `w:sdtPr` type-marker element. The
/// importer matches by local name, so the `w:`-prefixed spelling round-trips
/// (`Checkbox` re-imports even though real OOXML uses `w14:checkbox`).
fn sdt_kind_element(kind: SdtControlKind) -> &'static str {
    match kind {
        SdtControlKind::RichText => "w:richText",
        SdtControlKind::PlainText => "w:text",
        SdtControlKind::ComboBox => "w:comboBox",
        SdtControlKind::DropDownList => "w:dropDownList",
        SdtControlKind::Date => "w:date",
        SdtControlKind::Picture => "w:picture",
        SdtControlKind::Checkbox => "w:checkbox",
        SdtControlKind::Group => "w:group",
        SdtControlKind::BuildingBlockGallery => "w:docPartObj",
        SdtControlKind::RepeatingSection => "w:repeatingSection",
        SdtControlKind::Citation => "w:citation",
        SdtControlKind::Bibliography => "w:bibliography",
    }
}

fn write_run_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &RunProperties,
) -> Result<(), ExportError> {
    if *properties == RunProperties::default() {
        return Ok(());
    }
    w.write_event(Event::Start(start("w:rPr"))).map_err(pkg)?;
    if let Some(style_ref) = properties.style_ref {
        let mut el = start("w:rStyle");
        el.push_attribute(("w:val", style_id_token(style_ref).as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    // Fonts (`w:rFonts`): the four slots (each a named family or a `*Theme`
    // reference) and the `@hint`. `w:cs` uses `w:csTheme` to match the importer.
    if properties.font_ref.is_some()
        || properties.font_ref_h_ansi.is_some()
        || properties.font_ref_cs.is_some()
        || properties.font_ref_east_asia.is_some()
        || properties.font_hint.is_some()
    {
        let mut el = start("w:rFonts");
        push_font_slot(&mut el, "w:ascii", "w:asciiTheme", &properties.font_ref);
        push_font_slot(
            &mut el,
            "w:hAnsi",
            "w:hAnsiTheme",
            &properties.font_ref_h_ansi,
        );
        push_font_slot(
            &mut el,
            "w:eastAsia",
            "w:eastAsiaTheme",
            &properties.font_ref_east_asia,
        );
        push_font_slot(&mut el, "w:cs", "w:csTheme", &properties.font_ref_cs);
        if let Some(hint) = properties.font_hint {
            el.push_attribute(("w:hint", font_hint_token(hint)));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    for (value, name) in [
        (properties.bold, "w:b"),
        (properties.italic, "w:i"),
        (properties.strike, "w:strike"),
        (properties.double_strike, "w:dstrike"),
        (properties.all_caps, "w:caps"),
        (properties.small_caps, "w:smallCaps"),
        (properties.hidden, "w:vanish"),
        (properties.web_hidden, "w:webHidden"),
    ] {
        if let Some(on) = value {
            let mut el = start(name);
            // A `CT_OnOff` toggle: present means on; `w:val="0"` is an explicit
            // off (the importer's `is_true`).
            if !on {
                el.push_attribute(("w:val", "0"));
            }
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
    }
    if let Some(on) = properties.underline {
        // Underline is NOT a plain toggle: the importer maps `w:u@val != "none"`
        // to on. Emit a bare `w:u` for on and `w:val="none"` for an explicit off.
        let mut el = start("w:u");
        if !on {
            el.push_attribute(("w:val", "none"));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(Color::Rgb(rgb)) = &properties.color {
        let mut el = start("w:color");
        el.push_attribute((
            "w:val",
            format!("{:02X}{:02X}{:02X}", rgb.r, rgb.g, rgb.b).as_str(),
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(size) = properties.size_half_points {
        let mut el = start("w:sz");
        el.push_attribute(("w:val", size.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(alignment) = properties.vertical_alignment {
        let mut el = start("w:vertAlign");
        el.push_attribute(("w:val", vertical_alignment_token(alignment)));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(highlight) = properties.highlight {
        let mut el = start("w:highlight");
        el.push_attribute(("w:val", highlight_token(highlight)));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(emphasis) = properties.emphasis {
        let mut el = start("w:em");
        el.push_attribute(("w:val", emphasis_token(emphasis)));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    // Typographic metrics, each a signed/unsigned integer `w:val`.
    for (value, name) in [
        (properties.character_spacing_twips, "w:spacing"),
        (properties.position_half_points, "w:position"),
    ] {
        if let Some(v) = value {
            let mut el = start(name);
            el.push_attribute(("w:val", v.to_string().as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
    }
    if let Some(kern) = properties.kerning_half_points {
        let mut el = start("w:kern");
        el.push_attribute(("w:val", kern.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(language) = &properties.language {
        let mut el = start("w:lang");
        if let Some(v) = &language.value {
            el.push_attribute(("w:val", v.as_str()));
        }
        if let Some(v) = &language.east_asia {
            el.push_attribute(("w:eastAsia", v.as_str()));
        }
        if let Some(v) = &language.bidi {
            el.push_attribute(("w:bidi", v.as_str()));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:rPr")))
        .map_err(pkg)?;
    Ok(())
}

/// Pushes one `w:rFonts` slot: a named family onto `named`, or a theme reference
/// onto `theme`; nothing when the slot is unset.
fn push_font_slot(el: &mut BytesStart<'_>, named: &str, theme: &str, slot: &Option<FontRef>) {
    match slot {
        Some(FontRef::Named(font)) => el.push_attribute((named, font.name.as_str())),
        Some(FontRef::Theme(reference)) => {
            el.push_attribute((theme, theme_font_token(reference.slot)))
        }
        None => {}
    }
}

fn theme_font_token(slot: ThemeFontRef) -> &'static str {
    match slot {
        ThemeFontRef::MajorAscii => "majorAscii",
        ThemeFontRef::MajorHAnsi => "majorHAnsi",
        ThemeFontRef::MajorEastAsia => "majorEastAsia",
        ThemeFontRef::MajorBidi => "majorBidi",
        ThemeFontRef::MinorAscii => "minorAscii",
        ThemeFontRef::MinorHAnsi => "minorHAnsi",
        ThemeFontRef::MinorEastAsia => "minorEastAsia",
        ThemeFontRef::MinorBidi => "minorBidi",
    }
}

fn font_hint_token(hint: RunFontHint) -> &'static str {
    match hint {
        RunFontHint::Default => "default",
        RunFontHint::EastAsia => "eastAsia",
        RunFontHint::Cs => "cs",
    }
}

fn vertical_alignment_token(alignment: VerticalAlignment) -> &'static str {
    match alignment {
        VerticalAlignment::Baseline => "baseline",
        VerticalAlignment::Superscript => "superscript",
        VerticalAlignment::Subscript => "subscript",
    }
}

fn highlight_token(highlight: HighlightColor) -> &'static str {
    match highlight {
        HighlightColor::None => "none",
        HighlightColor::Black => "black",
        HighlightColor::Blue => "blue",
        HighlightColor::Cyan => "cyan",
        HighlightColor::DarkBlue => "darkBlue",
        HighlightColor::DarkCyan => "darkCyan",
        HighlightColor::DarkGray => "darkGray",
        HighlightColor::DarkGreen => "darkGreen",
        HighlightColor::DarkMagenta => "darkMagenta",
        HighlightColor::DarkRed => "darkRed",
        HighlightColor::DarkYellow => "darkYellow",
        HighlightColor::Green => "green",
        HighlightColor::LightGray => "lightGray",
        HighlightColor::Magenta => "magenta",
        HighlightColor::Red => "red",
        HighlightColor::White => "white",
        HighlightColor::Yellow => "yellow",
    }
}

fn tab_alignment_token(alignment: TabAlignment) -> &'static str {
    match alignment {
        TabAlignment::Start => "start",
        TabAlignment::Center => "center",
        TabAlignment::End => "end",
        TabAlignment::Decimal => "decimal",
        TabAlignment::Bar => "bar",
    }
}

fn tab_leader_token(leader: TabLeader) -> &'static str {
    match leader {
        TabLeader::Dot => "dot",
        TabLeader::Hyphen => "hyphen",
        TabLeader::Underscore => "underscore",
        TabLeader::MiddleDot => "middleDot",
        TabLeader::Heavy => "heavy",
    }
}

fn emphasis_token(emphasis: EmphasisMark) -> &'static str {
    match emphasis {
        EmphasisMark::None => "none",
        EmphasisMark::Dot => "dot",
        EmphasisMark::Comma => "comma",
        EmphasisMark::Circle => "circle",
        EmphasisMark::UnderDot => "underDot",
    }
}

fn alignment_token(alignment: Alignment) -> &'static str {
    match alignment {
        Alignment::Start => "start",
        Alignment::Center => "center",
        Alignment::End => "end",
        Alignment::Justify => "both",
    }
}

fn break_token(kind: BreakKind) -> &'static str {
    match kind {
        BreakKind::Line => "textWrapping",
        BreakKind::Page => "page",
        BreakKind::Column => "column",
    }
}

fn pkg<E>(_: E) -> ExportError {
    ExportError::Package
}
