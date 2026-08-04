//! Semantic DOCX writer: serialize a v1 `Document` model back to a valid,
//! editable WordprocessingML package (the dual of the Retention byte-copy
//! writer). Written today: the core body (`w:document`/`w:body`, paragraphs,
//! runs, all mapped run/paragraph properties, tabs/breaks), tables, the inline
//! constructs (hyperlinks, fields, bookmarks, revisions, content controls,
//! note/comment references, drawings, text boxes), block content controls, the
//! definition parts (`fontTable.xml` incl. embedded fonts, `theme1.xml` font
//! scheme, `styles.xml`, `numbering.xml`, footnotes/endnotes, comments), and the
//! structural parts (body `w:sectPr`, headers/footers). Media/font binary bytes
//! are supplied by the caller; the model carries only the reference metadata.
//! Anything unsupported is skipped, never a malformed part.
//!
//! Output is byte-deterministic for a given model: parts in fixed order, a fixed
//! ZIP timestamp, ids/relationships re-minted in document order.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};

use casual_doc_import::{RelationshipOwner, RetainedParts};
use casual_doc_model::v1::{
    AbstractNumbering, AbstractNumberingId, Alignment, AltChunk, AnchorHorizontal, AnchorVertical,
    AnchoredDrawing, AppProperties, BlockNode, BorderEdge, BreakKind, CellMergeAnnotation,
    CellMergeRevision, CellVerticalAlignment, CnfStyle, Color, ColorScheme, Comment, CommentId,
    CoreProperties, CropRect, CustomProperty, CustomValue, DashStyle, DefinitionMap, Definitions,
    DocGridType, Document, DocumentDefaults, DocumentProtectionEdit, DocumentSettings,
    DropCapFrame, DropCapMode, EmbeddedKind, EmbeddedObject, EmbeddedPart, EmphasisMark, Extent,
    Fill, FontCollection, FontDescriptor, FontFamilyKind, FontPitch, FontRef, FontScheme,
    FormCheckBoxSize, FormFieldData, FormFieldKind, FormTextType, FrameHorizontalAlignment,
    FrameHorizontalAnchor, FrameVerticalAlignment, FrameVerticalAnchor, FrameWrap, GradientKind,
    GradientStop, GridColumn, GroupChild, GroupShape, GroupTextBox, GroupTransform, HeaderFooterId,
    HeaderFooterKind, HeightRule, HighlightColor, HorizontalAlign, HorizontalAnchor,
    HorizontalPosition, HorizontalRuleAlign, HyperlinkTarget, InlineNode, LatentStyles,
    LevelJustification, LevelSuffix, LineEnd, LineEndKind, LineEndSize, LineNumberRestart,
    LineRule, MarkRevision, MarkRevisionKind, MediaId, MediaReference, MoveKind, Note, NoteId,
    NoteKind, NoteNumberRestart, NotePosition, NoteProperties, NumberFormat, NumberingInstance,
    NumberingInstanceId, NumberingLevel, PageBorderDisplay, PageBorderOffset, PageOrientation,
    PageVerticalAlignment, ParagraphProperties, Person, PointEmu, PositionalTabAlignment,
    PositionalTabLeader, PositionalTabRelativeTo, ProofState, PropChange, RevisionKind, RgbColor,
    Rgba, RunFontHint, RunProperties, SchemeColor, SdtCheckbox, SdtCheckboxSymbol, SdtControlData,
    SdtControlKind, SdtDate, SdtListItem, SdtLock, SdtProperties, SectionBoundary, SectionType,
    ShapeAdjustment, ShapeGeometry, ShapeStroke, Style, StyleId, StyleKind, TabAlignment,
    TabLeader, Table, TableAnchor, TableBorders, TableCell, TableCellProperties,
    TableFloatPosition, TableLayout, TableOverlap, TableProperties, TableRow, TableRowProperties,
    TableStyleOverride, TableStyleRegion, TableWidth, TableXAlign, TableYAlign, TextBox,
    TextBoxAutoFit, TextBoxBodyProperties, TextBoxHorizontalOverflow, TextBoxVerticalAnchor,
    TextBoxVerticalOverflow, TextDirection, ThemeColorRef, ThemeFontRef, VerticalAlign,
    VerticalAlignment, VerticalAnchor, VerticalMerge, VerticalPosition, VerticalTextAlignment,
    WidthType, WordprocessingGroup, WrapMode, Zoom, ZoomMode,
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
// Comment companion parts (P1F-10): threading, durable ids, and identity.
const W14_NS: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const W15_NS: &str = "http://schemas.microsoft.com/office/word/2012/wordml";
const W16CID_NS: &str = "http://schemas.microsoft.com/office/word/2016/wordml/cid";
const COMMENTS_EXTENDED_REL_TYPE: &str =
    "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";
const COMMENTS_EXTENDED_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml";
const COMMENTS_IDS_REL_TYPE: &str =
    "http://schemas.microsoft.com/office/2016/relationships/commentsIds";
const COMMENTS_IDS_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsIds+xml";
const PEOPLE_REL_TYPE: &str = "http://schemas.microsoft.com/office/2011/relationships/people";
const PEOPLE_CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.people+xml";
const HEADER_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";
const HEADER_CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const FOOTER_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer";
const FOOTER_CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
const SETTINGS_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
const SETTINGS_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";
const A_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const WP_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const PIC_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const WPS_NS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const WPG_NS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup";
const M_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
/// VML shape namespace (prefix `v`), for a legacy `w:object` OLE preview shape.
const V_NS: &str = "urn:schemas-microsoft-com:vml";
/// VML office-drawing namespace (prefix `o`), for `o:OLEObject`.
const O_NS: &str = "urn:schemas-microsoft-com:office:office";
/// DrawingML chart namespace (prefix `c`), for a `c:chart` reference.
const C_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
/// DrawingML diagram namespace (prefix `dgm`), for a `dgm:relIds` reference.
const DGM_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
/// The `a:graphicData@uri` marking a chart payload.
const CHART_URI: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
/// The `a:graphicData@uri` marking a SmartArt diagram payload.
const DIAGRAM_URI: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
const IMAGE_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const FONT_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/font";
const OBFUSCATED_FONT_CT: &str = "application/vnd.openxmlformats-officedocument.obfuscatedFont";
const CORE_PROPS_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
const CORE_PROPS_CT: &str = "application/vnd.openxmlformats-package.core-properties+xml";
const APP_PROPS_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties";
const APP_PROPS_CT: &str = "application/vnd.openxmlformats-officedocument.extended-properties+xml";
const CUSTOM_PROPS_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties";
const CUSTOM_PROPS_CT: &str = "application/vnd.openxmlformats-officedocument.custom-properties+xml";
// Package namespaces for the docProps parts.
const CP_NS: &str = "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
const DCTERMS_NS: &str = "http://purl.org/dc/terms/";
const DCMITYPE_NS: &str = "http://purl.org/dc/dcmitype/";
const XSI_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";
const EXT_PROPS_NS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties";
const CUSTOM_PROPS_NS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties";
const VT_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes";
/// The standard OPC format id every custom property is stamped with.
const CUSTOM_PROPS_FMTID: &str = "{D5CDD505-2E9C-101B-9397-08002B2CF9AE}";

/// A `docProps/*` part: its (static) part name, content type, root relationship
/// type + target, and serialized bytes. Distinct from [`ExtraPart`] because a
/// docProps part is referenced from the package root `_rels/.rels`, not from
/// `document.xml.rels`, and carries a package-level content type.
struct DocPropPart {
    part_name: &'static str,
    content_type: &'static str,
    rel_type: &'static str,
    target: &'static str,
    bytes: Vec<u8>,
}

/// The lowercased file extension of a media part name (for the content-type
/// `Default`), e.g. `word/media/image1.PNG` -> `png`; `bin` when absent.
fn media_extension(part_name: &str) -> String {
    part_name
        .rsplit('/')
        .next()
        .and_then(|file| file.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .filter(|ext| !ext.is_empty())
        .unwrap_or_else(|| "bin".to_owned())
}

/// The `word/`-relative target for a media relationship, e.g.
/// `word/media/image1.png` -> `media/image1.png`.
fn media_target(part_name: &str) -> &str {
    part_name.strip_prefix("word/").unwrap_or(part_name)
}

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
    /// `rId`s already claimed by media relationships (emitted verbatim); minting
    /// skips these so a hyperlink cannot collide with a media relationship.
    reserved: BTreeSet<String>,
}

impl RelBuilder {
    fn new(reserved: BTreeSet<String>) -> Self {
        Self {
            next: 0,
            by_url: BTreeMap::new(),
            entries: Vec::new(),
            reserved,
        }
    }

    /// Mints the next free `rId`, skipping any reserved by media.
    fn mint(&mut self) -> String {
        loop {
            self.next += 1;
            let id = format!("rId{}", self.next);
            if !self.reserved.contains(&id) {
                return id;
            }
        }
    }

    /// Returns the `rId` for an external hyperlink URL, minting a new one (and
    /// recording the relationship) the first time each URL is seen.
    fn hyperlink(&mut self, url: &str) -> String {
        if let Some(id) = self.by_url.get(url) {
            return id.clone();
        }
        let id = self.mint();
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
///
/// This preserves no opaque (unmodeled) package parts; use
/// [`write_document_with_retained_parts`] to carry an import's side-table
/// through so unmodeled parts survive a semantic edit→save.
pub fn write_document(
    document: &Document,
    media: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, ExportError> {
    write_document_with_retained_parts(document, media, &RetainedParts::default())
}

/// Serializes a v1 `Document` to a DOCX package, additionally carrying an
/// import's opaque part side-table (P1F-2) verbatim: every admitted part the
/// semantic model did not consume (glossary, embeddings, charts, customXml,
/// webSettings, thumbnail, stylesWithEffects, comment companions, ...) is
/// re-emitted byte-for-byte, its content-type merged into the generated
/// `[Content_Types].xml`, its owned `_rels` preserved, and the root/document
/// relationship that targets it re-added (with a fresh id) so the part stays
/// reachable. Digital signatures are excluded upstream (they are not in the
/// side-table), because regenerating the body invalidates them.
///
/// A part referenced only from the document *body* (a chart/embedding a dropped
/// drawing pointed at) is preserved as bytes but may be **orphaned**: the body
/// is regenerated from the model without that reference, so its relationship
/// exists (keeping the part in the package graph) but no body id names it.
/// Re-linking such objects is a future object-node slice, out of scope here.
pub fn write_document_with_retained_parts(
    document: &Document,
    media: &BTreeMap<String, Vec<u8>>,
    retained_parts: &RetainedParts,
) -> Result<Vec<u8>, ExportError> {
    let definitions = document.definitions();
    // Embedded-object (chart/diagram/OLE) part relationships, emitted with their
    // verbatim ids so the body reference and the relationship agree. Collected
    // before the body is written so their ids are reserved against hyperlink
    // minting; the referenced part BYTES come from the side-table (P1F-2).
    let embedded_rels = collect_embedded_rels(document);
    // Media relationships are emitted with their verbatim ids so the model
    // round-trips; reserve them (and the embedded-object ids) so hyperlink/part
    // rids do not collide.
    let mut reserved_rel_ids: BTreeSet<String> = definitions
        .media
        .iter()
        .map(|(_, reference)| reference.relationship_id.clone())
        .collect();
    for (id, _, _) in &embedded_rels {
        reserved_rel_ids.insert(id.clone());
    }
    let (document_xml, rels) = document_xml(document, reserved_rel_ids)?;

    // Extra parts beyond document.xml, each carrying its content-type override
    // and a document relationship; they appear only when the model has the
    // content (byte-identical to the earlier slices otherwise).
    let mut extras: Vec<ExtraPart> = Vec::new();
    let mut font_rels: Vec<RelEntry> = Vec::new();
    if !definitions.font_table.is_empty() {
        let (bytes, rels) = font_table_xml(&definitions.font_table)?;
        font_rels = rels;
        extras.push(ExtraPart::new(
            "word/fontTable.xml",
            FONT_TABLE_CT,
            FONT_TABLE_REL_TYPE,
            "fontTable.xml",
            bytes,
        ));
    }
    let has_embedded_fonts = !font_rels.is_empty();
    if definitions.font_scheme.is_some()
        || definitions.color_scheme.is_some()
        || definitions.format_scheme_xml.is_some()
    {
        extras.push(ExtraPart::new(
            "word/theme/theme1.xml",
            THEME_CT,
            THEME_REL_TYPE,
            "theme/theme1.xml",
            theme_xml(
                definitions.font_scheme.as_ref(),
                definitions.color_scheme.as_ref(),
                definitions.format_scheme_xml.as_deref(),
            )?,
        ));
    }
    if !definitions.settings.is_default() {
        extras.push(ExtraPart::new(
            "word/settings.xml",
            SETTINGS_CT,
            SETTINGS_REL_TYPE,
            "settings.xml",
            settings_xml(&definitions.settings)?,
        ));
    }
    if !definitions.styles.is_empty()
        || definitions.document_defaults.is_some()
        || definitions.latent_styles.is_some()
    {
        extras.push(ExtraPart::new(
            "word/styles.xml",
            STYLES_CT,
            STYLES_REL_TYPE,
            "styles.xml",
            styles_xml(
                &definitions.styles,
                definitions.document_defaults.as_ref(),
                definitions.latent_styles.as_ref(),
            )?,
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
        // Comment companion parts (P1F-10), each emitted only when it carries
        // data so a package without threading/identity stays byte-identical.
        if let Some(bytes) = comments_extended_xml(&definitions.comments)? {
            extras.push(ExtraPart::new(
                "word/commentsExtended.xml",
                COMMENTS_EXTENDED_CT,
                COMMENTS_EXTENDED_REL_TYPE,
                "commentsExtended.xml",
                bytes,
            ));
        }
        if let Some(bytes) = comments_ids_xml(&definitions.comments)? {
            extras.push(ExtraPart::new(
                "word/commentsIds.xml",
                COMMENTS_IDS_CT,
                COMMENTS_IDS_REL_TYPE,
                "commentsIds.xml",
                bytes,
            ));
        }
    }
    if !definitions.people.is_empty() {
        extras.push(ExtraPart::new(
            "word/people.xml",
            PEOPLE_CT,
            PEOPLE_REL_TYPE,
            "people.xml",
            people_xml(&definitions.people)?,
        ));
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

    // Document metadata parts (`docProps/*`), referenced from the package root
    // relationships (not `document.xml.rels`). A group is emitted only when the
    // model carries it, so an unedited package without metadata is byte-identical
    // to the earlier slices.
    let docprops = docprop_parts(document)?;

    // Parts are emitted in a deterministic order so the package bytes are
    // reproducible.
    let mut parts: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".to_owned(),
            content_types_xml(
                &extras,
                &docprops,
                &definitions.media,
                has_embedded_fonts,
                retained_parts,
            )?,
        ),
        (
            "_rels/.rels".to_owned(),
            root_rels_xml(&docprops, retained_parts)?,
        ),
        ("word/document.xml".to_owned(), document_xml),
        (
            "word/_rels/document.xml.rels".to_owned(),
            document_rels_xml(
                &rels,
                &extras,
                &definitions.media,
                &embedded_rels,
                retained_parts,
            )?,
        ),
    ];
    for docprop in &docprops {
        parts.push((docprop.part_name.to_owned(), docprop.bytes.clone()));
    }
    for extra in extras {
        if !extra.own_rels.is_empty() {
            parts.push((
                rels_part_name(&extra.part_name),
                part_rels_xml(&extra.own_rels)?,
            ));
        }
        parts.push((extra.part_name, extra.bytes));
    }
    // Opaque preserved parts (P1F-2): each part verbatim, plus its owned `_rels`
    // companion verbatim (so parts it references stay reachable). Content types
    // and root/document referencing relationships are merged into the generated
    // manifests above.
    for retained in &retained_parts.parts {
        if let Some(rels) = &retained.rels {
            parts.push((rels.part_name.clone(), rels.bytes.clone()));
        }
        parts.push((retained.part_name.clone(), retained.bytes.clone()));
    }
    // Media parts (image bytes supplied by the caller; the model carries only
    // the reference metadata). An absent entry writes an empty part — the
    // reference still round-trips (bytes are Retention's concern).
    for (_, reference) in definitions.media.iter() {
        let bytes = media.get(&reference.part_name).cloned().unwrap_or_default();
        parts.push((reference.part_name.clone(), bytes));
    }
    // Embedded fonts: the fontTable's own rels (`/font`) plus the `.odttf` parts.
    if has_embedded_fonts {
        parts.push((
            "word/_rels/fontTable.xml.rels".to_owned(),
            font_rels_xml(&font_rels)?,
        ));
        for font in &definitions.font_table {
            for (_, face) in font.embedded.faces() {
                let bytes = media.get(&face.part_name).cloned().unwrap_or_default();
                parts.push((face.part_name.clone(), bytes));
            }
        }
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
fn content_types_xml(
    extras: &[ExtraPart],
    docprops: &[DocPropPart],
    media: &DefinitionMap<MediaId, MediaReference>,
    has_embedded_fonts: bool,
    retained_parts: &RetainedParts,
) -> Result<Vec<u8>, ExportError> {
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
    // A `Default` per distinct media extension, mapped to that media's content
    // type (so `content_type(part)` re-imports to the same `media_type`).
    let mut media_defaults: BTreeMap<String, &str> = BTreeMap::new();
    for (_, reference) in media.iter() {
        media_defaults
            .entry(media_extension(&reference.part_name))
            .or_insert(reference.media_type.as_str());
    }
    for (ext, ct) in media_defaults {
        let mut d = start("Default");
        d.push_attribute(("Extension", ext.as_str()));
        d.push_attribute(("ContentType", ct));
        w.write_event(Event::Empty(d)).map_err(pkg)?;
    }
    if has_embedded_fonts {
        let mut d = start("Default");
        d.push_attribute(("Extension", "odttf"));
        d.push_attribute(("ContentType", OBFUSCATED_FONT_CT));
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
    for docprop in docprops {
        let part_name = format!("/{}", docprop.part_name);
        let mut over = start("Override");
        over.push_attribute(("PartName", part_name.as_str()));
        over.push_attribute(("ContentType", docprop.content_type));
        w.write_event(Event::Empty(over)).map_err(pkg)?;
    }
    // Opaque preserved parts (P1F-2): merge each part's declared content type as
    // an `Override` so it re-imports to the same type. A part whose source type
    // came from a `Default` extension is still valid as an explicit `Override`.
    // A part with no declared type falls back to the extension `Default`s above
    // (e.g. `xml`, `rels`); it gets no `Override`.
    for retained in &retained_parts.parts {
        let Some(content_type) = &retained.content_type else {
            continue;
        };
        let part_name = format!("/{}", retained.part_name);
        let mut over = start("Override");
        over.push_attribute(("PartName", part_name.as_str()));
        over.push_attribute(("ContentType", content_type.as_str()));
        w.write_event(Event::Empty(over)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("Types")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits `_rels/.rels`: the main-document relationship (`rId1`) plus one root
/// relationship per emitted `docProps/*` part (`rId2`..), in core/app/custom
/// order, plus any root-owned opaque-part relationship (P1F-2) that keeps a
/// preserved part (customXml, thumbnail, ...) reachable. With no metadata and no
/// preserved parts this is byte-identical to the earlier slices.
fn root_rels_xml(
    docprops: &[DocPropPart],
    retained_parts: &RetainedParts,
) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut rels = start("Relationships");
    rels.push_attribute(("xmlns", REL_NS));
    w.write_event(Event::Start(rels)).map_err(pkg)?;
    let mut rel = start("Relationship");
    rel.push_attribute(("Id", "rId1"));
    rel.push_attribute(("Type", DOC_REL_TYPE));
    rel.push_attribute(("Target", "word/document.xml"));
    w.write_event(Event::Empty(rel)).map_err(pkg)?;
    for (index, docprop) in docprops.iter().enumerate() {
        let id = format!("rId{}", index + 2);
        let mut rel = start("Relationship");
        rel.push_attribute(("Id", id.as_str()));
        rel.push_attribute(("Type", docprop.rel_type));
        rel.push_attribute(("Target", docprop.target));
        w.write_event(Event::Empty(rel)).map_err(pkg)?;
    }
    write_retained_relationships(&mut w, retained_parts, RelationshipOwner::Root)?;
    w.write_event(Event::End(BytesEnd::new("Relationships")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits the opaque-part (P1F-2) relationships owned by `owner`, each with a
/// fresh `rIdOp{n}` id (the `rIdOp` prefix cannot collide with the writer's
/// numeric `rId{n}` ids). The id is not referenced from any regenerated part —
/// it exists only to keep the preserved target reachable in the package graph —
/// so its exact value is immaterial; only in-file uniqueness matters.
fn write_retained_relationships(
    w: &mut Writer<Cursor<Vec<u8>>>,
    retained_parts: &RetainedParts,
    owner: RelationshipOwner,
) -> Result<(), ExportError> {
    let mut next = 0_u32;
    for relationship in &retained_parts.relationships {
        if relationship.owner != owner {
            continue;
        }
        next += 1;
        let id = format!("rIdOp{next}");
        let mut rel = start("Relationship");
        rel.push_attribute(("Id", id.as_str()));
        rel.push_attribute(("Type", relationship.relationship_type.as_str()));
        rel.push_attribute(("Target", relationship.target.as_str()));
        if relationship.external {
            rel.push_attribute(("TargetMode", "External"));
        }
        w.write_event(Event::Empty(rel)).map_err(pkg)?;
    }
    Ok(())
}

/// Builds the `docProps/*` parts from the document's metadata, one per non-empty
/// group, in core/app/custom order. Returns an empty vector when the document
/// carries no metadata.
fn docprop_parts(document: &Document) -> Result<Vec<DocPropPart>, ExportError> {
    let mut parts = Vec::new();
    let Some(properties) = document.properties() else {
        return Ok(parts);
    };
    if !properties.core.is_empty() {
        parts.push(DocPropPart {
            part_name: "docProps/core.xml",
            content_type: CORE_PROPS_CT,
            rel_type: CORE_PROPS_REL_TYPE,
            target: "docProps/core.xml",
            bytes: core_properties_xml(&properties.core)?,
        });
    }
    if !properties.app.is_empty() {
        parts.push(DocPropPart {
            part_name: "docProps/app.xml",
            content_type: APP_PROPS_CT,
            rel_type: APP_PROPS_REL_TYPE,
            target: "docProps/app.xml",
            bytes: app_properties_xml(&properties.app)?,
        });
    }
    if !properties.custom.is_empty() {
        parts.push(DocPropPart {
            part_name: "docProps/custom.xml",
            content_type: CUSTOM_PROPS_CT,
            rel_type: CUSTOM_PROPS_REL_TYPE,
            target: "docProps/custom.xml",
            bytes: custom_properties_xml(&properties.custom)?,
        });
    }
    Ok(parts)
}

/// Emits a `<tag>value</tag>` text element (empty text yields `<tag></tag>`),
/// with the text XML-escaped.
fn write_text_element(
    w: &mut Writer<Cursor<Vec<u8>>>,
    tag: &str,
    value: &str,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start(tag))).map_err(pkg)?;
    if !value.is_empty() {
        w.write_event(Event::Text(BytesText::new(value)))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new(tag))).map_err(pkg)?;
    Ok(())
}

/// Emits `docProps/core.xml` (OPC core properties). Elements are omitted when
/// their field is `None`; created/modified carry the W3CDTF `xsi:type`.
fn core_properties_xml(core: &CoreProperties) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("cp:coreProperties");
    root.push_attribute(("xmlns:cp", CP_NS));
    root.push_attribute(("xmlns:dc", DC_NS));
    root.push_attribute(("xmlns:dcterms", DCTERMS_NS));
    root.push_attribute(("xmlns:dcmitype", DCMITYPE_NS));
    root.push_attribute(("xmlns:xsi", XSI_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    for (value, tag) in [
        (&core.title, "dc:title"),
        (&core.subject, "dc:subject"),
        (&core.creator, "dc:creator"),
        (&core.keywords, "cp:keywords"),
        (&core.description, "dc:description"),
        (&core.last_modified_by, "cp:lastModifiedBy"),
        (&core.revision, "cp:revision"),
    ] {
        if let Some(value) = value {
            write_text_element(&mut w, tag, value)?;
        }
    }
    // The two dcterms timestamps carry the W3CDTF xsi:type.
    for (value, tag) in [
        (&core.created, "dcterms:created"),
        (&core.modified, "dcterms:modified"),
    ] {
        if let Some(value) = value {
            let mut element = start(tag);
            element.push_attribute(("xsi:type", "dcterms:W3CDTF"));
            w.write_event(Event::Start(element)).map_err(pkg)?;
            if !value.is_empty() {
                w.write_event(Event::Text(BytesText::new(value)))
                    .map_err(pkg)?;
            }
            w.write_event(Event::End(BytesEnd::new(tag))).map_err(pkg)?;
        }
    }
    for (value, tag) in [
        (&core.last_printed, "cp:lastPrinted"),
        (&core.category, "cp:category"),
        (&core.content_status, "cp:contentStatus"),
        (&core.language, "dc:language"),
        (&core.version, "cp:version"),
    ] {
        if let Some(value) = value {
            write_text_element(&mut w, tag, value)?;
        }
    }
    w.write_event(Event::End(BytesEnd::new("cp:coreProperties")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits `docProps/app.xml` (extended properties) in the ECMA-376 CT_Properties
/// element order. Each field is omitted when unset.
fn app_properties_xml(app: &AppProperties) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("Properties");
    root.push_attribute(("xmlns", EXT_PROPS_NS));
    root.push_attribute(("xmlns:vt", VT_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    for (value, tag) in [
        (&app.template, "Template"),
        (&app.manager, "Manager"),
        (&app.company, "Company"),
    ] {
        if let Some(value) = value {
            write_text_element(&mut w, tag, value)?;
        }
    }
    for (value, tag) in [
        (app.pages, "Pages"),
        (app.words, "Words"),
        (app.characters, "Characters"),
        (app.lines, "Lines"),
        (app.paragraphs, "Paragraphs"),
        (app.total_time, "TotalTime"),
    ] {
        if let Some(value) = value {
            write_text_element(&mut w, tag, &value.to_string())?;
        }
    }
    if let Some(value) = app.doc_security {
        write_text_element(&mut w, "DocSecurity", &value.to_string())?;
    }
    if let Some(value) = app.scale_crop {
        write_text_element(&mut w, "ScaleCrop", bool_token(value))?;
    }
    if !app.heading_pairs.is_empty() {
        w.write_event(Event::Start(start("HeadingPairs")))
            .map_err(pkg)?;
        let mut vector = start("vt:vector");
        vector.push_attribute(("size", (app.heading_pairs.len() * 2).to_string().as_str()));
        vector.push_attribute(("baseType", "variant"));
        w.write_event(Event::Start(vector)).map_err(pkg)?;
        for pair in &app.heading_pairs {
            w.write_event(Event::Start(start("vt:variant")))
                .map_err(pkg)?;
            write_text_element(&mut w, "vt:lpstr", &pair.name)?;
            w.write_event(Event::End(BytesEnd::new("vt:variant")))
                .map_err(pkg)?;
            w.write_event(Event::Start(start("vt:variant")))
                .map_err(pkg)?;
            write_text_element(&mut w, "vt:i4", &pair.count.to_string())?;
            w.write_event(Event::End(BytesEnd::new("vt:variant")))
                .map_err(pkg)?;
        }
        w.write_event(Event::End(BytesEnd::new("vt:vector")))
            .map_err(pkg)?;
        w.write_event(Event::End(BytesEnd::new("HeadingPairs")))
            .map_err(pkg)?;
    }
    if !app.titles_of_parts.is_empty() {
        w.write_event(Event::Start(start("TitlesOfParts")))
            .map_err(pkg)?;
        let mut vector = start("vt:vector");
        vector.push_attribute(("size", app.titles_of_parts.len().to_string().as_str()));
        vector.push_attribute(("baseType", "lpstr"));
        w.write_event(Event::Start(vector)).map_err(pkg)?;
        for title in &app.titles_of_parts {
            write_text_element(&mut w, "vt:lpstr", title)?;
        }
        w.write_event(Event::End(BytesEnd::new("vt:vector")))
            .map_err(pkg)?;
        w.write_event(Event::End(BytesEnd::new("TitlesOfParts")))
            .map_err(pkg)?;
    }
    if let Some(value) = app.links_up_to_date {
        write_text_element(&mut w, "LinksUpToDate", bool_token(value))?;
    }
    if let Some(value) = app.characters_with_spaces {
        write_text_element(&mut w, "CharactersWithSpaces", &value.to_string())?;
    }
    if let Some(value) = app.shared_doc {
        write_text_element(&mut w, "SharedDoc", bool_token(value))?;
    }
    if let Some(value) = &app.hyperlink_base {
        write_text_element(&mut w, "HyperlinkBase", value)?;
    }
    if let Some(value) = &app.application {
        write_text_element(&mut w, "Application", value)?;
    }
    if let Some(value) = &app.app_version {
        write_text_element(&mut w, "AppVersion", value)?;
    }
    w.write_event(Event::End(BytesEnd::new("Properties")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits `docProps/custom.xml`. Each property is stamped with the standard
/// format id and a sequential `pid` starting at 2 (the OPC minimum).
fn custom_properties_xml(custom: &[CustomProperty]) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("Properties");
    root.push_attribute(("xmlns", CUSTOM_PROPS_NS));
    root.push_attribute(("xmlns:vt", VT_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    for (index, property) in custom.iter().enumerate() {
        let mut element = start("property");
        element.push_attribute(("fmtid", CUSTOM_PROPS_FMTID));
        element.push_attribute(("pid", (index + 2).to_string().as_str()));
        element.push_attribute(("name", property.name.as_str()));
        w.write_event(Event::Start(element)).map_err(pkg)?;
        let (tag, value) = custom_value_tag(&property.value);
        write_text_element(&mut w, &format!("vt:{tag}"), &value)?;
        w.write_event(Event::End(BytesEnd::new("property")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("Properties")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// The `vt:` local name and text form for a typed custom value. `Other`
/// preserves its original `vt:` local name so it round-trips exactly.
fn custom_value_tag(value: &CustomValue) -> (String, String) {
    match value {
        CustomValue::Text { value } => ("lpwstr".to_owned(), value.clone()),
        CustomValue::I4 { value } => ("i4".to_owned(), value.to_string()),
        CustomValue::R8 { value } => ("r8".to_owned(), value.clone()),
        CustomValue::Bool { value } => ("bool".to_owned(), bool_token(*value).to_owned()),
        CustomValue::FileTime { value } => ("filetime".to_owned(), value.clone()),
        CustomValue::Other { kind, value } => (kind.clone(), value.clone()),
    }
}

/// The docProps boolean token.
const fn bool_token(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

/// Emits `word/_rels/document.xml.rels`. With no entries it is the empty
/// `<Relationships/>` element (byte-identical to the earlier no-relationship
/// slices); with entries it carries one external hyperlink relationship each.
fn document_rels_xml(
    entries: &[RelEntry],
    extras: &[ExtraPart],
    media: &DefinitionMap<MediaId, MediaReference>,
    embedded_rels: &[EmbeddedRelEntry],
    retained_parts: &RetainedParts,
) -> Result<Vec<u8>, ExportError> {
    let has_retained_document_rels = retained_parts
        .relationships
        .iter()
        .any(|relationship| relationship.owner == RelationshipOwner::Document);
    let mut w = new_writer();
    let mut rels = start("Relationships");
    rels.push_attribute(("xmlns", REL_NS));
    if entries.is_empty()
        && extras.is_empty()
        && media.is_empty()
        && embedded_rels.is_empty()
        && !has_retained_document_rels
    {
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
    // Media relationships with their verbatim ids (so `MediaReference` round-
    // trips); these ids are reserved elsewhere so nothing else reuses them.
    let mut reserved: BTreeSet<String> = BTreeSet::new();
    for (_, reference) in media.iter() {
        reserved.insert(reference.relationship_id.clone());
        let mut rel = start("Relationship");
        rel.push_attribute(("Id", reference.relationship_id.as_str()));
        rel.push_attribute(("Type", IMAGE_REL_TYPE));
        rel.push_attribute(("Target", media_target(&reference.part_name)));
        w.write_event(Event::Empty(rel)).map_err(pkg)?;
    }
    // Embedded-object (chart/diagram/OLE) part relationships with their verbatim
    // ids, so the body's `r:id` and this relationship agree; the target part's
    // bytes are re-emitted from the side-table (P1F-2), which — coordinated on
    // import — does NOT also re-add this relationship as an orphan.
    for (id, relationship_type, target) in embedded_rels {
        reserved.insert(id.clone());
        let mut rel = start("Relationship");
        rel.push_attribute(("Id", id.as_str()));
        rel.push_attribute(("Type", relationship_type.as_str()));
        rel.push_attribute(("Target", target.as_str()));
        w.write_event(Event::Empty(rel)).map_err(pkg)?;
    }
    // Internal relationships after the hyperlink ids. A part with an explicit
    // `rel_id` (headers/footers) uses it; the rest get a fresh sequential `rId`
    // that skips any reserved media id. Headers/footers keep relationship order.
    let mut next = entries.len();
    for extra in extras {
        let id = match &extra.rel_id {
            Some(id) => id.clone(),
            None => loop {
                next += 1;
                let id = format!("rId{next}");
                if !reserved.contains(&id) {
                    break id;
                }
            },
        };
        let mut rel = start("Relationship");
        rel.push_attribute(("Id", id.as_str()));
        rel.push_attribute(("Type", extra.rel_type));
        rel.push_attribute(("Target", extra.target.as_str()));
        w.write_event(Event::Empty(rel)).map_err(pkg)?;
    }
    write_retained_relationships(&mut w, retained_parts, RelationshipOwner::Document)?;
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
        rels: RelBuilder::new(BTreeSet::new()),
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
/// `CommentId`, with author/initials/date attributes, wrapping its blocks. When
/// the comment carries a `para_id` (its companion-part join key), that `w14:paraId`
/// is stamped on the comment's last top-level paragraph so the threading in
/// `commentsExtended.xml`/`commentsIds.xml` resolves back to it.
fn comments_xml(
    comments: &DefinitionMap<CommentId, Comment>,
    defs: &Definitions,
) -> Result<(Vec<u8>, Vec<RelEntry>), ExportError> {
    let mut w = new_writer();
    let mut ctx = Ctx {
        defs,
        rels: RelBuilder::new(BTreeSet::new()),
    };
    let mut r = start("w:comments");
    r.push_attribute(("xmlns:w", W_NS));
    r.push_attribute(("xmlns:r", R_NS));
    r.push_attribute(("xmlns:w14", W14_NS));
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
        // The `w14:paraId` anchors on the comment's last top-level paragraph.
        let anchor = comment
            .para_id
            .as_deref()
            .and_then(|_| comment.blocks.iter().rposition(is_paragraph));
        for (index, block) in comment.blocks.iter().enumerate() {
            match (block, anchor) {
                (BlockNode::Paragraph(paragraph), Some(anchor)) if anchor == index => {
                    write_paragraph(
                        &mut w,
                        &paragraph.properties,
                        &paragraph.inlines,
                        &mut ctx,
                        comment.para_id.as_deref(),
                    )?;
                }
                _ => write_block(&mut w, block, &mut ctx)?,
            }
        }
        w.write_event(Event::End(BytesEnd::new("w:comment")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:comments")))
        .map_err(pkg)?;
    Ok((finish(w), ctx.rels.entries))
}

/// Whether a block is a paragraph (the anchor kind for a comment's `w14:paraId`).
fn is_paragraph(block: &BlockNode) -> bool {
    matches!(block, BlockNode::Paragraph(_))
}

/// Emits `word/commentsExtended.xml` from the comments that carry threading
/// state (a parent link or a done flag). Returns `None` when no comment does, so
/// the part (and its content-type/rels) is omitted entirely.
fn comments_extended_xml(
    comments: &DefinitionMap<CommentId, Comment>,
) -> Result<Option<Vec<u8>>, ExportError> {
    let has_threading = comments
        .iter()
        .any(|(_, comment)| comment.parent_para_id.is_some() || comment.done);
    if !has_threading {
        return Ok(None);
    }
    let mut w = new_writer();
    let mut root = start("w15:commentsEx");
    root.push_attribute(("xmlns:w15", W15_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    // An entry is written for every comment with a `para_id` (its key), so a
    // resolved root and its replies all round-trip.
    for (_, comment) in comments.iter() {
        let Some(para_id) = comment.para_id.as_deref() else {
            continue;
        };
        if comment.parent_para_id.is_none() && !comment.done {
            continue;
        }
        let mut el = start("w15:commentEx");
        el.push_attribute(("w15:paraId", para_id));
        if let Some(parent) = comment.parent_para_id.as_deref() {
            el.push_attribute(("w15:paraIdParent", parent));
        }
        if comment.done {
            el.push_attribute(("w15:done", "1"));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w15:commentsEx")))
        .map_err(pkg)?;
    Ok(Some(finish(w)))
}

/// Emits `word/commentsIds.xml` mapping each comment's `para_id` to its durable
/// id (falling back to the `para_id` itself when the model carries no distinct
/// durable id). Returns `None` when no comment has a `para_id`.
fn comments_ids_xml(
    comments: &DefinitionMap<CommentId, Comment>,
) -> Result<Option<Vec<u8>>, ExportError> {
    if comments
        .iter()
        .all(|(_, comment)| comment.para_id.is_none())
    {
        return Ok(None);
    }
    let mut w = new_writer();
    let mut root = start("w16cid:commentsIds");
    root.push_attribute(("xmlns:w16cid", W16CID_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    for (_, comment) in comments.iter() {
        let Some(para_id) = comment.para_id.as_deref() else {
            continue;
        };
        let durable_id = comment.durable_id.as_deref().unwrap_or(para_id);
        let mut el = start("w16cid:commentId");
        el.push_attribute(("w16cid:paraId", para_id));
        el.push_attribute(("w16cid:durableId", durable_id));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w16cid:commentsIds")))
        .map_err(pkg)?;
    Ok(Some(finish(w)))
}

/// Emits `word/people.xml`: the collaborator identity table, each `w15:person`
/// keyed by author name with optional `w15:presenceInfo`.
fn people_xml(people: &[Person]) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("w15:people");
    root.push_attribute(("xmlns:w15", W15_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    for person in people {
        let mut el = start("w15:person");
        el.push_attribute(("w15:author", person.author.as_str()));
        match &person.presence {
            Some(presence) => {
                w.write_event(Event::Start(el)).map_err(pkg)?;
                let mut info = start("w15:presenceInfo");
                info.push_attribute(("w15:providerId", presence.provider_id.as_str()));
                info.push_attribute(("w15:userId", presence.user_id.as_str()));
                w.write_event(Event::Empty(info)).map_err(pkg)?;
                w.write_event(Event::End(BytesEnd::new("w15:person")))
                    .map_err(pkg)?;
            }
            None => {
                w.write_event(Event::Empty(el)).map_err(pkg)?;
            }
        }
    }
    w.write_event(Event::End(BytesEnd::new("w15:people")))
        .map_err(pkg)?;
    Ok(finish(w))
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
        rels: RelBuilder::new(BTreeSet::new()),
    };
    let mut r = start(root);
    r.push_attribute(("xmlns:w", W_NS));
    r.push_attribute(("xmlns:r", R_NS));
    // `xmlns:w14` so a content-control checkbox's `w14:checkbox` detail is
    // well-formed when a block sdt lives in a header/footer.
    r.push_attribute(("xmlns:w14", W14_NS));
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
fn font_table_xml(fonts: &[FontDescriptor]) -> Result<(Vec<u8>, Vec<RelEntry>), ExportError> {
    let mut w = new_writer();
    let mut font_rels: Vec<RelEntry> = Vec::new();
    let mut root = start("w:fonts");
    root.push_attribute(("xmlns:w", W_NS));
    root.push_attribute(("xmlns:r", R_NS));
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
        // Embedded faces: emit each `w:embed*` with its verbatim relationship id
        // (into `fontTable.xml.rels`) and font key, and record the rel.
        for (name, face) in font.embedded.faces() {
            let mut child = start(name);
            child.push_attribute(("r:id", face.relationship_id.as_str()));
            child.push_attribute(("w:fontKey", face.font_key.as_str()));
            if face.subsetted {
                child.push_attribute(("w:subsetted", "true"));
            }
            w.write_event(Event::Empty(child)).map_err(pkg)?;
            font_rels.push((
                face.relationship_id.clone(),
                media_target(&face.part_name).to_owned(),
            ));
        }
        w.write_event(Event::End(BytesEnd::new("w:font")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:fonts")))
        .map_err(pkg)?;
    Ok((finish(w), font_rels))
}

/// Emits a `fontTable.xml.rels` carrying the embedded-font `/font` relationships
/// (internal targets, no `TargetMode`).
fn font_rels_xml(rels: &[RelEntry]) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("Relationships");
    root.push_attribute(("xmlns", REL_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    for (id, target) in rels {
        let mut rel = start("Relationship");
        rel.push_attribute(("Id", id.as_str()));
        rel.push_attribute(("Type", FONT_REL_TYPE));
        rel.push_attribute(("Target", target.as_str()));
        w.write_event(Event::Empty(rel)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("Relationships")))
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

/// Emits `word/theme/theme1.xml` from the modeled schemes. `themeElements`
/// children are emitted in the schema order `clrScheme`, `fontScheme`,
/// `fmtScheme`: the colour scheme (from the model), the font scheme (from the
/// model), then the format scheme (retained verbatim). Each is emitted only when
/// present so the round trip is exact.
fn theme_xml(
    font_scheme: Option<&FontScheme>,
    color_scheme: Option<&ColorScheme>,
    format_scheme_xml: Option<&str>,
) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("a:theme");
    root.push_attribute(("xmlns:a", A_NS));
    root.push_attribute(("name", "Office Theme"));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    w.write_event(Event::Start(start("a:themeElements")))
        .map_err(pkg)?;
    if let Some(scheme) = color_scheme {
        write_color_scheme(&mut w, scheme)?;
    }
    if let Some(scheme) = font_scheme {
        let mut font_scheme = start("a:fontScheme");
        font_scheme.push_attribute(("name", "Office"));
        w.write_event(Event::Start(font_scheme)).map_err(pkg)?;
        write_font_collection(&mut w, "a:majorFont", &scheme.major)?;
        write_font_collection(&mut w, "a:minorFont", &scheme.minor)?;
        w.write_event(Event::End(BytesEnd::new("a:fontScheme")))
            .map_err(pkg)?;
    }
    if let Some(xml) = format_scheme_xml {
        // Retained verbatim: write the captured subtree bytes directly (they are
        // already a serialized `a:fmtScheme` element with `a:`-prefixed names,
        // which the `xmlns:a` on the root resolves).
        w.get_mut().write_all(xml.as_bytes()).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("a:themeElements")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("a:theme")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits `a:clrScheme` with its twelve named slots in OOXML child order.
fn write_color_scheme(
    w: &mut Writer<Cursor<Vec<u8>>>,
    scheme: &ColorScheme,
) -> Result<(), ExportError> {
    let mut root = start("a:clrScheme");
    root.push_attribute(("name", scheme.name.as_str()));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    for (tag, color) in [
        ("a:dk1", &scheme.dark1),
        ("a:lt1", &scheme.light1),
        ("a:dk2", &scheme.dark2),
        ("a:lt2", &scheme.light2),
        ("a:accent1", &scheme.accent1),
        ("a:accent2", &scheme.accent2),
        ("a:accent3", &scheme.accent3),
        ("a:accent4", &scheme.accent4),
        ("a:accent5", &scheme.accent5),
        ("a:accent6", &scheme.accent6),
        ("a:hlink", &scheme.hyperlink),
        ("a:folHlink", &scheme.followed_hyperlink),
    ] {
        w.write_event(Event::Start(start(tag))).map_err(pkg)?;
        write_scheme_color(w, color)?;
        w.write_event(Event::End(BytesEnd::new(tag))).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("a:clrScheme")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a single scheme color value (`a:srgbClr` or `a:sysClr`).
fn write_scheme_color(
    w: &mut Writer<Cursor<Vec<u8>>>,
    color: &SchemeColor,
) -> Result<(), ExportError> {
    match color {
        SchemeColor::Srgb(rgb) => {
            let mut el = start("a:srgbClr");
            el.push_attribute(("val", rgb_hex(rgb).as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        SchemeColor::System(system) => {
            let mut el = start("a:sysClr");
            el.push_attribute(("val", system.value.as_str()));
            if let Some(last) = &system.last_color {
                el.push_attribute(("lastClr", rgb_hex(last).as_str()));
            }
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
    }
    Ok(())
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
fn styles_xml(
    styles: &DefinitionMap<StyleId, Style>,
    document_defaults: Option<&DocumentDefaults>,
    latent_styles: Option<&LatentStyles>,
) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("w:styles");
    root.push_attribute(("xmlns:w", W_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    // `w:docDefaults` precedes the styles (schema order): `w:rPrDefault` then
    // `w:pPrDefault`. A `Some(default)` run/paragraph still emits its (empty)
    // element so the importer sees the tag's presence across the round trip.
    if let Some(defaults) = document_defaults {
        w.write_event(Event::Start(start("w:docDefaults")))
            .map_err(pkg)?;
        w.write_event(Event::Start(start("w:rPrDefault")))
            .map_err(pkg)?;
        if let Some(run) = &defaults.run {
            if *run == RunProperties::default() {
                w.write_event(Event::Empty(start("w:rPr"))).map_err(pkg)?;
            } else {
                write_run_properties(&mut w, run)?;
            }
        }
        w.write_event(Event::End(BytesEnd::new("w:rPrDefault")))
            .map_err(pkg)?;
        w.write_event(Event::Start(start("w:pPrDefault")))
            .map_err(pkg)?;
        if let Some(paragraph) = &defaults.paragraph {
            if *paragraph == ParagraphProperties::default() {
                w.write_event(Event::Empty(start("w:pPr"))).map_err(pkg)?;
            } else {
                write_paragraph_properties(&mut w, paragraph, None)?;
            }
        }
        w.write_event(Event::End(BytesEnd::new("w:pPrDefault")))
            .map_err(pkg)?;
        w.write_event(Event::End(BytesEnd::new("w:docDefaults")))
            .map_err(pkg)?;
    }
    // `w:latentStyles` follows `w:docDefaults` and precedes the styles (schema
    // order). Its block-level defaults and `w:lsdException` children are
    // attribute-only.
    if let Some(latent) = latent_styles {
        let mut el = start("w:latentStyles");
        for (name, value) in [
            ("w:defLockedState", latent.default_locked_state),
            ("w:defSemiHidden", latent.default_semi_hidden),
            ("w:defUnhideWhenUsed", latent.default_unhide_when_used),
            ("w:defQFormat", latent.default_q_format),
        ] {
            if let Some(value) = value {
                el.push_attribute((name, if value { "1" } else { "0" }));
            }
        }
        if let Some(priority) = latent.default_ui_priority {
            el.push_attribute(("w:defUIPriority", priority.to_string().as_str()));
        }
        if let Some(count) = latent.count {
            el.push_attribute(("w:count", count.to_string().as_str()));
        }
        if latent.exceptions.is_empty() {
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        } else {
            w.write_event(Event::Start(el)).map_err(pkg)?;
            for exception in &latent.exceptions {
                let mut el = start("w:lsdException");
                el.push_attribute(("w:name", exception.name.as_str()));
                for (name, value) in [
                    ("w:locked", exception.locked),
                    ("w:semiHidden", exception.semi_hidden),
                    ("w:unhideWhenUsed", exception.unhide_when_used),
                    ("w:qFormat", exception.q_format),
                ] {
                    if let Some(value) = value {
                        el.push_attribute((name, if value { "1" } else { "0" }));
                    }
                }
                if let Some(priority) = exception.ui_priority {
                    el.push_attribute(("w:uiPriority", priority.to_string().as_str()));
                }
                w.write_event(Event::Empty(el)).map_err(pkg)?;
            }
            w.write_event(Event::End(BytesEnd::new("w:latentStyles")))
                .map_err(pkg)?;
        }
    }
    for (id, style) in styles.iter() {
        let style_id = style_id_token(*id);
        let mut el = start("w:style");
        el.push_attribute(("w:type", style_kind_token(style.kind)));
        el.push_attribute(("w:styleId", style_id.as_str()));
        if style.is_default {
            el.push_attribute(("w:default", "1"));
        }
        w.write_event(Event::Start(el)).map_err(pkg)?;
        // Metadata in `CT_Style` order. `w:name` is emitted only when modeled: a
        // style with no captured name re-imports to `None`, so emitting a
        // placeholder would break the fixed point.
        if let Some(name) = &style.name {
            let mut el = start("w:name");
            el.push_attribute(("w:val", name.as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        if let Some(aliases) = &style.aliases {
            let mut el = start("w:aliases");
            el.push_attribute(("w:val", aliases.as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        for (element, reference) in [
            ("w:basedOn", style.based_on),
            ("w:next", style.next),
            ("w:link", style.link),
        ] {
            if let Some(reference) = reference {
                let mut el = start(element);
                el.push_attribute(("w:val", style_id_token(reference).as_str()));
                w.write_event(Event::Empty(el)).map_err(pkg)?;
            }
        }
        if style.hidden {
            w.write_event(Event::Empty(start("w:hidden")))
                .map_err(pkg)?;
        }
        if let Some(priority) = style.ui_priority {
            let mut el = start("w:uiPriority");
            el.push_attribute(("w:val", priority.to_string().as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        for (on, element) in [
            (style.semi_hidden, "w:semiHidden"),
            (style.unhide_when_used, "w:unhideWhenUsed"),
            (style.q_format, "w:qFormat"),
            (style.locked, "w:locked"),
        ] {
            if on {
                w.write_event(Event::Empty(start(element))).map_err(pkg)?;
            }
        }
        write_style_properties(
            &mut w,
            &style.paragraph,
            &style.run,
            &style.table,
            &style.table_row,
            &style.table_cell,
        )?;
        for over in &style.conditional {
            write_conditional_format(&mut w, over)?;
        }
        w.write_event(Event::End(BytesEnd::new("w:style")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:styles")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// The `w:style/@w:type` token for a style kind.
fn style_kind_token(kind: StyleKind) -> &'static str {
    match kind {
        StyleKind::Paragraph => "paragraph",
        StyleKind::Character => "character",
        StyleKind::Table => "table",
        StyleKind::Numbering => "numbering",
    }
}

/// Emits the property containers a style (or a `w:tblStylePr` region) carries, in
/// `CT_Style`/`CT_TblStylePr` order (pPr, rPr, tblPr, trPr, tcPr). A `Some(default)`
/// value still emits its bare element: the importer keys on the tag's PRESENCE
/// (Some vs None), while the property writers elide an all-default value — so a
/// bare element preserves presence across the round trip.
fn write_style_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    paragraph: &Option<ParagraphProperties>,
    run: &Option<RunProperties>,
    table: &Option<TableProperties>,
    table_row: &Option<TableRowProperties>,
    table_cell: &Option<TableCellProperties>,
) -> Result<(), ExportError> {
    if let Some(paragraph) = paragraph {
        if *paragraph == ParagraphProperties::default() {
            w.write_event(Event::Empty(start("w:pPr"))).map_err(pkg)?;
        } else {
            // A style never carries a section break (that is a body concept).
            write_paragraph_properties(w, paragraph, None)?;
        }
    }
    if let Some(run) = run {
        if *run == RunProperties::default() {
            w.write_event(Event::Empty(start("w:rPr"))).map_err(pkg)?;
        } else {
            write_run_properties(w, run)?;
        }
    }
    if let Some(table) = table {
        if *table == TableProperties::default() {
            w.write_event(Event::Empty(start("w:tblPr"))).map_err(pkg)?;
        } else {
            write_table_properties(w, table)?;
        }
    }
    if let Some(row) = table_row {
        if *row == TableRowProperties::default() {
            w.write_event(Event::Empty(start("w:trPr"))).map_err(pkg)?;
        } else {
            write_row_properties(w, row)?;
        }
    }
    if let Some(cell) = table_cell {
        if *cell == TableCellProperties::default() {
            w.write_event(Event::Empty(start("w:tcPr"))).map_err(pkg)?;
        } else {
            write_cell_properties(w, cell)?;
        }
    }
    Ok(())
}

/// Emits one `w:tblStylePr` conditional-formatting block (region + overrides).
fn write_conditional_format(
    w: &mut Writer<Cursor<Vec<u8>>>,
    over: &TableStyleOverride,
) -> Result<(), ExportError> {
    let mut el = start("w:tblStylePr");
    el.push_attribute(("w:type", table_style_region_token(over.region)));
    w.write_event(Event::Start(el)).map_err(pkg)?;
    write_style_properties(
        w,
        &over.paragraph,
        &over.run,
        &over.table,
        &over.table_row,
        &over.table_cell,
    )?;
    w.write_event(Event::End(BytesEnd::new("w:tblStylePr")))
        .map_err(pkg)?;
    Ok(())
}

/// The `w:tblStylePr/@w:type` token for a table-style region.
fn table_style_region_token(region: TableStyleRegion) -> &'static str {
    match region {
        TableStyleRegion::WholeTable => "wholeTable",
        TableStyleRegion::FirstRow => "firstRow",
        TableStyleRegion::LastRow => "lastRow",
        TableStyleRegion::FirstColumn => "firstCol",
        TableStyleRegion::LastColumn => "lastCol",
        TableStyleRegion::Band1Horizontal => "band1Horz",
        TableStyleRegion::Band2Horizontal => "band2Horz",
        TableStyleRegion::Band1Vertical => "band1Vert",
        TableStyleRegion::Band2Vertical => "band2Vert",
        TableStyleRegion::NorthEastCell => "neCell",
        TableStyleRegion::NorthWestCell => "nwCell",
        TableStyleRegion::SouthEastCell => "seCell",
        TableStyleRegion::SouthWestCell => "swCell",
    }
}

/// Emits `word/settings.xml` with the modeled settings, in `CT_Settings` schema
/// order so the part is valid WordprocessingML. Each field is emitted only when
/// it departs from the default, and the importer reads the same shapes back — so
/// the round trip is a fixed point. Emitted only when `settings.is_default()` is
/// false, matching the importer.
fn settings_xml(settings: &DocumentSettings) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("w:settings");
    root.push_attribute(("xmlns:w", W_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    if let Some(protection) = &settings.write_protection {
        let mut el = start("w:writeProtection");
        if protection.recommended {
            el.push_attribute(("w:recommended", "1"));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    write_zoom(&mut w, &settings.zoom)?;
    // `w:displayBackgroundShape` (CT_Settings §17.15.1.29) precedes the embed-font
    // flags in schema order.
    if settings.display_background_shape {
        w.write_event(Event::Empty(start("w:displayBackgroundShape")))
            .map_err(pkg)?;
    }
    for (name, on) in [
        ("w:embedTrueTypeFonts", settings.embed_true_type_fonts),
        ("w:embedSystemFonts", settings.embed_system_fonts),
        ("w:saveSubsetFonts", settings.save_subset_fonts),
    ] {
        if on {
            w.write_event(Event::Empty(start(name))).map_err(pkg)?;
        }
    }
    if settings.mirror_margins {
        w.write_event(Event::Empty(start("w:mirrorMargins")))
            .map_err(pkg)?;
    }
    if !settings.proof_state.is_empty() {
        let mut el = start("w:proofState");
        if let Some(state) = settings.proof_state.spelling {
            el.push_attribute(("w:spelling", proof_state_token(state)));
        }
        if let Some(state) = settings.proof_state.grammar {
            el.push_attribute(("w:grammar", proof_state_token(state)));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if settings.track_changes {
        w.write_event(Event::Empty(start("w:trackChanges")))
            .map_err(pkg)?;
    }
    if let Some(protection) = &settings.document_protection {
        let mut el = start("w:documentProtection");
        el.push_attribute(("w:edit", protection_edit_token(protection.edit)));
        if protection.enforcement {
            el.push_attribute(("w:enforcement", "1"));
        }
        if protection.formatting {
            el.push_attribute(("w:formatting", "1"));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(value) = settings.default_tab_stop {
        let mut el = start("w:defaultTabStop");
        el.push_attribute(("w:val", value.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    // Hyphenation group (CT_Settings §17.15.1.10/22/48/32), in schema order:
    // autoHyphenation, consecutiveHyphenLimit, hyphenationZone, doNotHyphenateCaps.
    if settings.auto_hyphenation {
        w.write_event(Event::Empty(start("w:autoHyphenation")))
            .map_err(pkg)?;
    }
    if let Some(value) = settings.consecutive_hyphen_limit {
        let mut el = start("w:consecutiveHyphenLimit");
        el.push_attribute(("w:val", value.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(value) = settings.hyphenation_zone {
        let mut el = start("w:hyphenationZone");
        el.push_attribute(("w:val", value.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if settings.do_not_hyphenate_caps {
        w.write_event(Event::Empty(start("w:doNotHyphenateCaps")))
            .map_err(pkg)?;
    }
    if settings.even_and_odd_headers {
        w.write_event(Event::Empty(start("w:evenAndOddHeaders")))
            .map_err(pkg)?;
    }
    if settings.update_fields {
        w.write_event(Event::Empty(start("w:updateFields")))
            .map_err(pkg)?;
    }
    if let Some(style) = &settings.default_table_style {
        let mut el = start("w:defaultTableStyle");
        el.push_attribute(("w:val", style.as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    write_section_note_props(&mut w, "w:footnotePr", &settings.footnote_props)?;
    write_section_note_props(&mut w, "w:endnotePr", &settings.endnote_props)?;
    if !settings.compat.is_empty() {
        w.write_event(Event::Start(start("w:compat")))
            .map_err(pkg)?;
        for setting in &settings.compat {
            let mut el = start("w:compatSetting");
            el.push_attribute(("w:name", setting.name.as_str()));
            el.push_attribute(("w:uri", setting.uri.as_str()));
            el.push_attribute(("w:val", setting.val.as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        w.write_event(Event::End(BytesEnd::new("w:compat")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:settings")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits `w:zoom` when the model carries a mode and/or an explicit percent.
fn write_zoom(w: &mut Writer<Cursor<Vec<u8>>>, zoom: &Zoom) -> Result<(), ExportError> {
    if zoom.is_empty() {
        return Ok(());
    }
    let mut el = start("w:zoom");
    if let Some(mode) = zoom.mode {
        el.push_attribute(("w:val", zoom_mode_token(mode)));
    }
    let percent = zoom.percent.map(|value| value.to_string());
    if let Some(percent) = &percent {
        el.push_attribute(("w:percent", percent.as_str()));
    }
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
}

fn proof_state_token(state: ProofState) -> &'static str {
    match state {
        ProofState::Clean => "clean",
        ProofState::Dirty => "dirty",
    }
}

fn protection_edit_token(edit: DocumentProtectionEdit) -> &'static str {
    match edit {
        DocumentProtectionEdit::None => "none",
        DocumentProtectionEdit::ReadOnly => "readOnly",
        DocumentProtectionEdit::Comments => "comments",
        DocumentProtectionEdit::TrackedChanges => "trackedChanges",
        DocumentProtectionEdit::Forms => "forms",
    }
}

fn zoom_mode_token(mode: ZoomMode) -> &'static str {
    match mode {
        ZoomMode::None => "none",
        ZoomMode::FullPage => "fullPage",
        ZoomMode::BestFit => "bestFit",
        ZoomMode::TextFit => "textFit",
    }
}

/// The `w:styleId`/`w:val` string for a style, derived from its internal id so
/// references reproduce it deterministically.
fn style_id_token(id: StyleId) -> String {
    id.node_id().as_u128().to_string()
}

/// Emits `word/numbering.xml`: the abstract definitions (each with its levels'
/// format/text/justify detail) then the numbering instances. The
/// `w:abstractNumId`/`w:numId` strings derive from the internal ids so a
/// `w:num`'s `w:abstractNumId` and a body `w:numPr`'s `w:numId` reference the same
/// string and re-import to the same ids. Each level's modeled detail (start,
/// numFmt, isLgl, suff, lvlText, lvlJc, pPr, rPr) is emitted in schema order; the
/// importer reads the same shapes back, so the round trip is a fixed point.
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
        if let Some(kind) = abstract_num.multi_level_type {
            use casual_doc_model::v1::MultiLevelType;
            let mut mlt = start("w:multiLevelType");
            mlt.push_attribute((
                "w:val",
                match kind {
                    MultiLevelType::SingleLevel => "singleLevel",
                    MultiLevelType::Multilevel => "multilevel",
                    MultiLevelType::HybridMultilevel => "hybridMultilevel",
                },
            ));
            w.write_event(Event::Empty(mlt)).map_err(pkg)?;
        }
        // `w:styleLink` then `w:numStyleLink` in CT_AbstractNum schema order,
        // after multiLevelType and before the levels. Each emits the linked
        // paragraph style's id token so it re-imports to the same StyleId.
        if let Some(style_link) = abstract_num.style_link {
            let mut el = start("w:styleLink");
            el.push_attribute(("w:val", style_id_token(style_link).as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        if let Some(num_style_link) = abstract_num.num_style_link {
            let mut el = start("w:numStyleLink");
            el.push_attribute(("w:val", style_id_token(num_style_link).as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        for level in &abstract_num.levels {
            write_level(&mut w, level)?;
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
        // Per-instance start overrides (`w:lvlOverride/w:startOverride`) so a
        // restarted list round-trips its restart value.
        for over in &instance.overrides {
            if let Some(start_value) = over.start {
                let mut lo = start("w:lvlOverride");
                lo.push_attribute(("w:ilvl", over.level.to_string().as_str()));
                w.write_event(Event::Start(lo)).map_err(pkg)?;
                let mut so = start("w:startOverride");
                so.push_attribute(("w:val", start_value.to_string().as_str()));
                w.write_event(Event::Empty(so)).map_err(pkg)?;
                w.write_event(Event::End(BytesEnd::new("w:lvlOverride")))
                    .map_err(pkg)?;
            }
        }
        w.write_event(Event::End(BytesEnd::new("w:num")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:numbering")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits one `w:lvl` with its modeled detail in `CT_Lvl` schema order. A
/// `Some(default)` pPr/rPr emits a bare element to preserve presence across the
/// round trip (the property writers elide an all-default value), mirroring the
/// styles writer.
fn write_level(w: &mut Writer<Cursor<Vec<u8>>>, level: &NumberingLevel) -> Result<(), ExportError> {
    let mut lvl = start("w:lvl");
    lvl.push_attribute(("w:ilvl", level.level.to_string().as_str()));
    w.write_event(Event::Start(lvl)).map_err(pkg)?;
    let mut s = start("w:start");
    s.push_attribute(("w:val", level.start.to_string().as_str()));
    w.write_event(Event::Empty(s)).map_err(pkg)?;
    if let Some(format) = &level.num_fmt {
        let mut el = start("w:numFmt");
        el.push_attribute(("w:val", number_format_token(format)));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    // `w:lvlRestart` follows numFmt in CT_Lvl schema order, before isLgl.
    if let Some(restart) = level.lvl_restart {
        let mut el = start("w:lvlRestart");
        el.push_attribute(("w:val", restart.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    // `w:pStyle` (the level's paragraph-style binding) follows lvlRestart, before
    // isLgl. Emitted with the referenced style's id token so it re-imports to the
    // same StyleId.
    if let Some(pstyle) = level.pstyle {
        let mut el = start("w:pStyle");
        el.push_attribute(("w:val", style_id_token(pstyle).as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if level.is_lgl {
        w.write_event(Event::Empty(start("w:isLgl"))).map_err(pkg)?;
    }
    if let Some(suffix) = level.suff {
        let mut el = start("w:suff");
        el.push_attribute(("w:val", level_suffix_token(suffix)));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(text) = &level.lvl_text {
        let mut el = start("w:lvlText");
        el.push_attribute(("w:val", text.as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(justification) = level.lvl_jc {
        let mut el = start("w:lvlJc");
        el.push_attribute(("w:val", level_jc_token(justification)));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(paragraph) = &level.paragraph_properties {
        if *paragraph == ParagraphProperties::default() {
            w.write_event(Event::Empty(start("w:pPr"))).map_err(pkg)?;
        } else {
            // A level's pPr never carries a section break (a body concept).
            write_paragraph_properties(w, paragraph, None)?;
        }
    }
    if let Some(run) = &level.run_properties {
        if *run == RunProperties::default() {
            w.write_event(Event::Empty(start("w:rPr"))).map_err(pkg)?;
        } else {
            write_run_properties(w, run)?;
        }
    }
    w.write_event(Event::End(BytesEnd::new("w:lvl")))
        .map_err(pkg)?;
    Ok(())
}

/// The `w:numFmt/@w:val` token; `Other` returns its retained verbatim value.
fn number_format_token(format: &NumberFormat) -> &str {
    match format {
        NumberFormat::Decimal => "decimal",
        NumberFormat::Bullet => "bullet",
        NumberFormat::LowerRoman => "lowerRoman",
        NumberFormat::UpperRoman => "upperRoman",
        NumberFormat::LowerLetter => "lowerLetter",
        NumberFormat::UpperLetter => "upperLetter",
        NumberFormat::Ordinal => "ordinal",
        NumberFormat::CardinalText => "cardinalText",
        NumberFormat::OrdinalText => "ordinalText",
        NumberFormat::DecimalZero => "decimalZero",
        NumberFormat::None => "none",
        NumberFormat::Other(value) => value.as_str(),
    }
}

fn level_jc_token(justification: LevelJustification) -> &'static str {
    match justification {
        LevelJustification::Start => "left",
        LevelJustification::Center => "center",
        LevelJustification::End => "right",
    }
}

fn level_suffix_token(suffix: LevelSuffix) -> &'static str {
    match suffix {
        LevelSuffix::Tab => "tab",
        LevelSuffix::Space => "space",
        LevelSuffix::Nothing => "nothing",
    }
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
fn document_xml(
    document: &Document,
    media_rel_ids: BTreeSet<String>,
) -> Result<(Vec<u8>, Vec<RelEntry>), ExportError> {
    let mut w = new_writer();
    let mut doc = start("w:document");
    doc.push_attribute(("xmlns:w", W_NS));
    // `xmlns:r` is required so a hyperlink's/drawing's `r:id`/`r:embed` is
    // well-formed; the DrawingML prefixes are declared for inline drawings.
    doc.push_attribute(("xmlns:r", R_NS));
    doc.push_attribute(("xmlns:wp", WP_NS));
    doc.push_attribute(("xmlns:a", A_NS));
    doc.push_attribute(("xmlns:pic", PIC_NS));
    doc.push_attribute(("xmlns:wps", WPS_NS));
    doc.push_attribute(("xmlns:wpg", WPG_NS));
    // `xmlns:w14` binds the Office 2010 prefix so a content-control checkbox's
    // `w14:checkbox` detail is well-formed.
    doc.push_attribute(("xmlns:w14", W14_NS));
    // `xmlns:m` binds the OMML prefix so a retained `m:oMath` subtree is
    // well-formed (the captured markup carries no local namespace declaration).
    doc.push_attribute(("xmlns:m", M_NS));
    // Embedded-object prefixes: `c`/`dgm` for chart/diagram graphic references,
    // `v`/`o` for a legacy `w:object` OLE preview shape and its `o:OLEObject`.
    doc.push_attribute(("xmlns:c", C_NS));
    doc.push_attribute(("xmlns:dgm", DGM_NS));
    doc.push_attribute(("xmlns:v", V_NS));
    doc.push_attribute(("xmlns:o", O_NS));
    w.write_event(Event::Start(doc)).map_err(pkg)?;
    w.write_event(Event::Start(start("w:body"))).map_err(pkg)?;

    let mut ctx = Ctx {
        defs: document.definitions(),
        rels: RelBuilder::new(media_rel_ids),
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

/// Emits a `w:sectPr` (header/footer references, page geometry, columns). Used
/// both for the body-level trailing section and, nested in a paragraph's
/// `w:pPr`, for a per-paragraph section break.
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
    write_section_note_props(w, "w:footnotePr", &section.footnote_props)?;
    write_section_note_props(w, "w:endnotePr", &section.endnote_props)?;
    if let Some(section_type) = section.section_type {
        let mut el = start("w:type");
        el.push_attribute((
            "w:val",
            match section_type {
                SectionType::NextPage => "nextPage",
                SectionType::Continuous => "continuous",
                SectionType::EvenPage => "evenPage",
                SectionType::OddPage => "oddPage",
                SectionType::NextColumn => "nextColumn",
            },
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    let mut pg_sz = start("w:pgSz");
    pg_sz.push_attribute(("w:w", section.page_size.width_twips.to_string().as_str()));
    pg_sz.push_attribute(("w:h", section.page_size.height_twips.to_string().as_str()));
    if let Some(orientation) = section.orientation {
        pg_sz.push_attribute((
            "w:orient",
            match orientation {
                PageOrientation::Portrait => "portrait",
                PageOrientation::Landscape => "landscape",
            },
        ));
    }
    w.write_event(Event::Empty(pg_sz)).map_err(pkg)?;
    let mut pg_mar = start("w:pgMar");
    pg_mar.push_attribute(("w:top", section.page_margins.top_twips.to_string().as_str()));
    pg_mar.push_attribute((
        "w:bottom",
        section.page_margins.bottom_twips.to_string().as_str(),
    ));
    // `w:pgMar` uses the PHYSICAL `w:left`/`w:right` attributes (ECMA-376
    // `CT_PageMar`), not the logical `w:start`/`w:end` — Word/Pages ignore the
    // latter and fall back to default margins. Our model stores logical
    // start/end; for the LTR common case start=left, end=right. (RTL section
    // mirroring is a separate follow-up.)
    pg_mar.push_attribute((
        "w:left",
        section.page_margins.start_twips.to_string().as_str(),
    ));
    pg_mar.push_attribute((
        "w:right",
        section.page_margins.end_twips.to_string().as_str(),
    ));
    if let Some(header) = section.page_margins.header_twips {
        pg_mar.push_attribute(("w:header", header.to_string().as_str()));
    }
    if let Some(footer) = section.page_margins.footer_twips {
        pg_mar.push_attribute(("w:footer", footer.to_string().as_str()));
    }
    if let Some(gutter) = section.page_margins.gutter_twips {
        pg_mar.push_attribute(("w:gutter", gutter.to_string().as_str()));
    }
    w.write_event(Event::Empty(pg_mar)).map_err(pkg)?;
    if !section.paper_source.is_empty() {
        let mut el = start("w:paperSrc");
        if let Some(first) = section.paper_source.first {
            el.push_attribute(("w:first", first.to_string().as_str()));
        }
        if let Some(other) = section.paper_source.other {
            el.push_attribute(("w:other", other.to_string().as_str()));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if !section.page_borders.is_empty() {
        let borders = &section.page_borders;
        let mut el = start("w:pgBorders");
        if let Some(display) = borders.display {
            el.push_attribute((
                "w:display",
                match display {
                    PageBorderDisplay::AllPages => "allPages",
                    PageBorderDisplay::FirstPage => "firstPage",
                    PageBorderDisplay::NotFirstPage => "notFirstPage",
                },
            ));
        }
        if let Some(offset_from) = borders.offset_from {
            el.push_attribute((
                "w:offsetFrom",
                match offset_from {
                    PageBorderOffset::Page => "page",
                    PageBorderOffset::Text => "text",
                },
            ));
        }
        w.write_event(Event::Start(el)).map_err(pkg)?;
        // CT_PageBorders orders the edges top, left, bottom, right; the model's
        // start/end map back to the `w:left`/`w:right` page-border edge names.
        for (edge, name) in [
            (&borders.top, "w:top"),
            (&borders.start, "w:left"),
            (&borders.bottom, "w:bottom"),
            (&borders.end, "w:right"),
        ] {
            if let Some(edge) = edge {
                write_border_edge(w, name, edge)?;
            }
        }
        w.write_event(Event::End(BytesEnd::new("w:pgBorders")))
            .map_err(pkg)?;
    }
    if !section.line_numbering.is_empty() {
        let line = &section.line_numbering;
        let mut el = start("w:lnNumType");
        if let Some(count_by) = line.count_by {
            el.push_attribute(("w:countBy", count_by.to_string().as_str()));
        }
        if let Some(start_num) = line.start {
            el.push_attribute(("w:start", start_num.to_string().as_str()));
        }
        if let Some(distance) = line.distance {
            el.push_attribute(("w:distance", distance.to_string().as_str()));
        }
        if let Some(restart) = line.restart {
            el.push_attribute((
                "w:restart",
                match restart {
                    LineNumberRestart::NewPage => "newPage",
                    LineNumberRestart::NewSection => "newSection",
                    LineNumberRestart::Continuous => "continuous",
                },
            ));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if !section.page_numbering.is_empty() {
        let mut el = start("w:pgNumType");
        if let Some(format) = &section.page_numbering.format {
            el.push_attribute(("w:fmt", format.as_str()));
        }
        if let Some(start_num) = section.page_numbering.start {
            el.push_attribute(("w:start", start_num.to_string().as_str()));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    let mut cols = start("w:cols");
    cols.push_attribute(("w:num", section.columns.count.to_string().as_str()));
    if let Some(equal_width) = section.columns.equal_width {
        cols.push_attribute(("w:equalWidth", if equal_width { "1" } else { "0" }));
    }
    if let Some(space) = section.columns.space_twips {
        cols.push_attribute(("w:space", space.to_string().as_str()));
    }
    if let Some(separator) = section.columns.separator {
        cols.push_attribute(("w:sep", if separator { "1" } else { "0" }));
    }
    if section.columns.columns.is_empty() {
        w.write_event(Event::Empty(cols)).map_err(pkg)?;
    } else {
        // Explicit per-column geometry: `w:cols` wraps one `w:col` per column.
        w.write_event(Event::Start(cols)).map_err(pkg)?;
        for def in &section.columns.columns {
            let mut col = start("w:col");
            col.push_attribute(("w:w", def.width_twips.to_string().as_str()));
            if let Some(space) = def.space_twips {
                col.push_attribute(("w:space", space.to_string().as_str()));
            }
            w.write_event(Event::Empty(col)).map_err(pkg)?;
        }
        w.write_event(Event::End(BytesEnd::new("w:cols")))
            .map_err(pkg)?;
    }
    if let Some(alignment) = section.vertical_alignment {
        let mut el = start("w:vAlign");
        el.push_attribute((
            "w:val",
            match alignment {
                PageVerticalAlignment::Top => "top",
                PageVerticalAlignment::Center => "center",
                PageVerticalAlignment::Both => "both",
                PageVerticalAlignment::Bottom => "bottom",
            },
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(on) = section.title_page {
        let mut el = start("w:titlePg");
        if !on {
            el.push_attribute(("w:val", "0"));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(direction) = section.text_direction {
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
    if section.bidi {
        w.write_event(Event::Empty(start("w:bidi"))).map_err(pkg)?;
    }
    if !section.doc_grid.is_empty() {
        let mut el = start("w:docGrid");
        if let Some(grid_type) = section.doc_grid.grid_type {
            el.push_attribute((
                "w:type",
                match grid_type {
                    DocGridType::Default => "default",
                    DocGridType::Lines => "lines",
                    DocGridType::LinesAndChars => "linesAndChars",
                    DocGridType::SnapToChars => "snapToChars",
                },
            ));
        }
        if let Some(line_pitch) = section.doc_grid.line_pitch {
            el.push_attribute(("w:linePitch", line_pitch.to_string().as_str()));
        }
        if let Some(char_space) = section.doc_grid.char_space {
            el.push_attribute(("w:charSpace", char_space.to_string().as_str()));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:sectPr")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a per-section `w:footnotePr`/`w:endnotePr` when non-empty, in `CT_FtnProps`
/// order (`pos`, `numFmt`, `numStart`, `numRestart`).
fn write_section_note_props(
    w: &mut Writer<Cursor<Vec<u8>>>,
    tag: &str,
    props: &NoteProperties,
) -> Result<(), ExportError> {
    if props.is_empty() {
        return Ok(());
    }
    w.write_event(Event::Start(start(tag))).map_err(pkg)?;
    if let Some(position) = props.position {
        let mut el = start("w:pos");
        el.push_attribute((
            "w:val",
            match position {
                NotePosition::PageBottom => "pageBottom",
                NotePosition::BeneathText => "beneathText",
                NotePosition::SectionEnd => "sectEnd",
                NotePosition::DocumentEnd => "docEnd",
            },
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(format) = &props.number_format {
        let mut el = start("w:numFmt");
        el.push_attribute(("w:val", number_format_token(format)));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(start_num) = props.number_start {
        let mut el = start("w:numStart");
        el.push_attribute(("w:val", start_num.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(restart) = props.number_restart {
        let mut el = start("w:numRestart");
        el.push_attribute((
            "w:val",
            match restart {
                NoteNumberRestart::Continuous => "continuous",
                NoteNumberRestart::EachSection => "eachSect",
                NoteNumberRestart::EachPage => "eachPage",
            },
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new(tag))).map_err(pkg)?;
    Ok(())
}

/// Emits a legacy form field's `w:ffData` block (inside a complex field's
/// `fldChar begin`), in `CT_FFData` order: name, enabled, calcOnExit, entry/exit
/// macros, help/status text, then the one kind-specific payload.
fn write_form_field_data(
    w: &mut Writer<Cursor<Vec<u8>>>,
    form: &FormFieldData,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:ffData")))
        .map_err(pkg)?;
    write_ff_string(w, "w:name", form.name.as_deref())?;
    write_ff_on_off(w, "w:enabled", form.enabled)?;
    write_ff_on_off(w, "w:calcOnExit", form.calc_on_exit)?;
    write_ff_string(w, "w:entryMacro", form.entry_macro.as_deref())?;
    write_ff_string(w, "w:exitMacro", form.exit_macro.as_deref())?;
    write_ff_string(w, "w:helpText", form.help_text.as_deref())?;
    write_ff_string(w, "w:statusText", form.status_text.as_deref())?;
    match &form.kind {
        FormFieldKind::TextInput(text) => {
            w.write_event(Event::Start(start("w:textInput")))
                .map_err(pkg)?;
            if let Some(text_type) = text.text_type {
                let mut el = start("w:type");
                el.push_attribute(("w:val", form_text_type_token(text_type)));
                w.write_event(Event::Empty(el)).map_err(pkg)?;
            }
            write_ff_string(w, "w:default", text.default.as_deref())?;
            if let Some(max_length) = text.max_length {
                let mut el = start("w:maxLength");
                el.push_attribute(("w:val", max_length.to_string().as_str()));
                w.write_event(Event::Empty(el)).map_err(pkg)?;
            }
            write_ff_string(w, "w:format", text.format.as_deref())?;
            w.write_event(Event::End(BytesEnd::new("w:textInput")))
                .map_err(pkg)?;
        }
        FormFieldKind::CheckBox(check) => {
            w.write_event(Event::Start(start("w:checkBox")))
                .map_err(pkg)?;
            match check.size {
                Some(FormCheckBoxSize::Explicit(half_points)) => {
                    let mut el = start("w:size");
                    el.push_attribute(("w:val", half_points.to_string().as_str()));
                    w.write_event(Event::Empty(el)).map_err(pkg)?;
                }
                Some(FormCheckBoxSize::Auto) => {
                    w.write_event(Event::Empty(start("w:sizeAuto")))
                        .map_err(pkg)?;
                }
                None => {}
            }
            write_ff_on_off(w, "w:default", check.default)?;
            write_ff_on_off(w, "w:checked", check.checked)?;
            w.write_event(Event::End(BytesEnd::new("w:checkBox")))
                .map_err(pkg)?;
        }
        FormFieldKind::DropDown(list) => {
            w.write_event(Event::Start(start("w:ddList")))
                .map_err(pkg)?;
            if let Some(result) = list.result {
                let mut el = start("w:result");
                el.push_attribute(("w:val", result.to_string().as_str()));
                w.write_event(Event::Empty(el)).map_err(pkg)?;
            }
            for entry in &list.entries {
                let mut el = start("w:listEntry");
                el.push_attribute(("w:val", entry.as_str()));
                w.write_event(Event::Empty(el)).map_err(pkg)?;
            }
            w.write_event(Event::End(BytesEnd::new("w:ddList")))
                .map_err(pkg)?;
        }
    }
    w.write_event(Event::End(BytesEnd::new("w:ffData")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits an optional `w:val`-carrying `w:ffData` child (empty is a valid value).
fn write_ff_string(
    w: &mut Writer<Cursor<Vec<u8>>>,
    name: &'static str,
    value: Option<&str>,
) -> Result<(), ExportError> {
    if let Some(value) = value {
        let mut el = start(name);
        el.push_attribute(("w:val", value));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    Ok(())
}

/// Emits an optional `CT_OnOff` `w:ffData` child: bare for `true`, `w:val="0"`
/// for `false`, nothing when unset.
fn write_ff_on_off(
    w: &mut Writer<Cursor<Vec<u8>>>,
    name: &'static str,
    value: Option<bool>,
) -> Result<(), ExportError> {
    if let Some(value) = value {
        let mut el = start(name);
        if !value {
            el.push_attribute(("w:val", "0"));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    Ok(())
}

/// The `w:textInput/w:type@w:val` token (`ST_FFTextType`) for a form text type.
fn form_text_type_token(text_type: FormTextType) -> &'static str {
    match text_type {
        FormTextType::Regular => "regular",
        FormTextType::Number => "number",
        FormTextType::Date => "date",
        FormTextType::CurrentTime => "currentTime",
        FormTextType::CurrentDate => "currentDate",
        FormTextType::Calculation => "calculated",
    }
}

/// Emits a body/cell block: a paragraph, a table, or a block-level content
/// control (`w:sdt` wrapping block content).
fn write_block(
    w: &mut Writer<Cursor<Vec<u8>>>,
    block: &BlockNode,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    match block {
        BlockNode::Paragraph(paragraph) => {
            write_paragraph(w, &paragraph.properties, &paragraph.inlines, ctx, None)
        }
        BlockNode::Table(table) => write_table(w, table, ctx),
        BlockNode::Sdt(sdt) => {
            w.write_event(Event::Start(start("w:sdt"))).map_err(pkg)?;
            write_sdt_properties(w, &sdt.properties)?;
            w.write_event(Event::Start(start("w:sdtContent")))
                .map_err(pkg)?;
            for inner in &sdt.blocks {
                write_block(w, inner, ctx)?;
            }
            w.write_event(Event::End(BytesEnd::new("w:sdtContent")))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:sdt")))
                .map_err(pkg)?;
            Ok(())
        }
        // An aggregated external content chunk: an empty `w:altChunk` referencing
        // the preserved part by its verbatim `r:id` (the relationship is emitted in
        // `document.xml.rels`, see `collect_embedded_rels`), plus `w:altChunkPr`
        // when it carries `w:matchSrc`.
        BlockNode::AltChunk(chunk) => write_alt_chunk(w, chunk),
    }
}

fn write_alt_chunk(w: &mut Writer<Cursor<Vec<u8>>>, chunk: &AltChunk) -> Result<(), ExportError> {
    let mut el = start("w:altChunk");
    el.push_attribute(("r:id", chunk.part.relationship_id.as_str()));
    match chunk.properties.match_source {
        None => {
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        Some(match_source) => {
            w.write_event(Event::Start(el)).map_err(pkg)?;
            w.write_event(Event::Start(start("w:altChunkPr")))
                .map_err(pkg)?;
            let mut match_src = start("w:matchSrc");
            match_src.push_attribute(("w:val", if match_source { "true" } else { "false" }));
            w.write_event(Event::Empty(match_src)).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:altChunkPr")))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:altChunk")))
                .map_err(pkg)?;
        }
    }
    Ok(())
}

fn write_table(
    w: &mut Writer<Cursor<Vec<u8>>>,
    table: &Table,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:tbl"))).map_err(pkg)?;
    write_table_properties(w, &table.properties)?;
    // `w:tblGrid` is emitted when it has columns OR carries a grid change (whose
    // `w:tblGridChange` must nest inside it as its last child).
    if !table.grid.is_empty() || table.grid_change.is_some() {
        w.write_event(Event::Start(start("w:tblGrid")))
            .map_err(pkg)?;
        write_grid_columns(w, &table.grid)?;
        // `w:tblGridChange` is the last child of `w:tblGrid`; its `w:tblGrid` is
        // the prior column grid (CT_TblGridChange carries only a `w:id`).
        if let Some(change) = &table.grid_change {
            let mut el = start("w:tblGridChange");
            push_prop_change_attrs(&mut el, change);
            w.write_event(Event::Start(el)).map_err(pkg)?;
            w.write_event(Event::Start(start("w:tblGrid")))
                .map_err(pkg)?;
            write_grid_columns(w, change.prior.as_ref())?;
            w.write_event(Event::End(BytesEnd::new("w:tblGrid")))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:tblGridChange")))
                .map_err(pkg)?;
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

/// Emits the `w:gridCol` children of a `w:tblGrid` (each with its `w:w` width
/// when set). Shared by the current grid and a `w:tblGridChange` prior snapshot.
fn write_grid_columns(
    w: &mut Writer<Cursor<Vec<u8>>>,
    columns: &[GridColumn],
) -> Result<(), ExportError> {
    for column in columns {
        let mut col = start("w:gridCol");
        if let Some(width) = column.width_twips {
            col.push_attribute(("w:w", width.to_string().as_str()));
        }
        w.write_event(Event::Empty(col)).map_err(pkg)?;
    }
    Ok(())
}

/// Pushes a format-change revision's `w:author`/`w:date`/`w:id` attributes onto
/// the `w:*PrChange` element. Each is emitted only when present, mirroring the
/// `w:ins`/`w:del` metadata convention (a producer-tolerant, non-strict form).
fn push_prop_change_attrs<P>(el: &mut BytesStart<'_>, change: &PropChange<P>) {
    if let Some(author) = &change.author {
        el.push_attribute(("w:author", author.as_str()));
    }
    if let Some(date) = &change.date {
        el.push_attribute(("w:date", date.as_str()));
    }
    if let Some(id) = &change.revision_id {
        el.push_attribute(("w:id", id.as_str()));
    }
}

/// Writes a tracked mark insertion/deletion as an empty element carrying the
/// revision's author/date/id. The element names differ by container (a row uses
/// `w:ins`/`w:del`; a cell uses `w:cellIns`/`w:cellDel`), so the caller passes
/// them.
fn write_mark_revision(
    w: &mut Writer<Cursor<Vec<u8>>>,
    revision: &MarkRevision,
    insertion: &str,
    deletion: &str,
) -> Result<(), ExportError> {
    let mut el = start(match revision.kind {
        MarkRevisionKind::Insertion => insertion,
        MarkRevisionKind::Deletion => deletion,
    });
    if let Some(author) = &revision.author {
        el.push_attribute(("w:author", author.as_str()));
    }
    if let Some(date) = &revision.date {
        el.push_attribute(("w:date", date.as_str()));
    }
    if let Some(id) = &revision.revision_id {
        el.push_attribute(("w:id", id.as_str()));
    }
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
}

fn cell_merge_annotation_token(annotation: CellMergeAnnotation) -> &'static str {
    match annotation {
        CellMergeAnnotation::Cont => "cont",
        CellMergeAnnotation::Rest => "rest",
    }
}

/// Writes a tracked cell merge (`w:cellMerge`): author/date/id plus the current
/// and original vertical-merge annotations.
fn write_cell_merge(
    w: &mut Writer<Cursor<Vec<u8>>>,
    merge: &CellMergeRevision,
) -> Result<(), ExportError> {
    let mut el = start("w:cellMerge");
    if let Some(author) = &merge.author {
        el.push_attribute(("w:author", author.as_str()));
    }
    if let Some(date) = &merge.date {
        el.push_attribute(("w:date", date.as_str()));
    }
    if let Some(id) = &merge.revision_id {
        el.push_attribute(("w:id", id.as_str()));
    }
    if let Some(vmerge) = merge.vmerge {
        el.push_attribute(("w:vMerge", cell_merge_annotation_token(vmerge)));
    }
    if let Some(vmerge_orig) = merge.vmerge_orig {
        el.push_attribute(("w:vMergeOrig", cell_merge_annotation_token(vmerge_orig)));
    }
    w.write_event(Event::Empty(el)).map_err(pkg)?;
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
    // `w:tblStyle` is first in `CT_TblPrBase`.
    if let Some(style_ref) = properties.style_ref {
        let mut el = start("w:tblStyle");
        el.push_attribute(("w:val", style_id_token(style_ref).as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    // `w:tblpPr` follows `w:tblStyle` and precedes `w:tblOverlap`.
    if let Some(float) = &properties.float_position {
        write_table_float_position(w, float)?;
    }
    if let Some(overlap) = properties.overlap {
        let mut el = start("w:tblOverlap");
        el.push_attribute((
            "w:val",
            match overlap {
                TableOverlap::Never => "never",
                TableOverlap::Overlap => "overlap",
            },
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    // `w:bidiVisual` follows `w:tblOverlap` in `CT_TblPrBase`.
    if properties.tbl_bidi_visual {
        w.write_event(Event::Empty(start("w:bidiVisual")))
            .map_err(pkg)?;
    }
    // `w:tblStyleRowBandSize`/`w:tblStyleColBandSize` follow `w:bidiVisual` and
    // precede `w:tblW`/`w:jc` in `CT_TblPrBase`.
    if let Some(size) = properties.row_band_size {
        let mut el = start("w:tblStyleRowBandSize");
        el.push_attribute(("w:val", size.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(size) = properties.col_band_size {
        let mut el = start("w:tblStyleColBandSize");
        el.push_attribute(("w:val", size.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(alignment) = properties.alignment {
        let mut jc = start("w:jc");
        jc.push_attribute(("w:val", alignment_token(alignment)));
        w.write_event(Event::Empty(jc)).map_err(pkg)?;
    }
    if let Some(width) = properties.width {
        write_table_width(w, "w:tblW", width)?;
    }
    if let Some(spacing) = properties.cell_spacing_twips {
        let mut el = start("w:tblCellSpacing");
        el.push_attribute(("w:type", "dxa"));
        el.push_attribute(("w:w", spacing.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(indent) = properties.indent_twips {
        let mut el = start("w:tblInd");
        el.push_attribute(("w:type", "dxa"));
        el.push_attribute(("w:w", indent.to_string().as_str()));
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
    if let Some(caption) = &properties.caption {
        let mut el = start("w:tblCaption");
        el.push_attribute(("w:val", caption.as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(description) = &properties.description {
        let mut el = start("w:tblDescription");
        el.push_attribute(("w:val", description.as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    // `w:tblPrChange` is the last child of `w:tblPr`; its `w:tblPr` is the prior
    // snapshot (CT_TblPrBase — no nested change). An all-default prior still emits
    // a bare `<w:tblPr/>` so the required child is present.
    if let Some(change) = &properties.prop_change {
        let mut el = start("w:tblPrChange");
        push_prop_change_attrs(&mut el, change);
        w.write_event(Event::Start(el)).map_err(pkg)?;
        if change.prior.as_ref() == &TableProperties::default() {
            w.write_event(Event::Empty(start("w:tblPr"))).map_err(pkg)?;
        } else {
            write_table_properties(w, change.prior.as_ref())?;
        }
        w.write_event(Event::End(BytesEnd::new("w:tblPrChange")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:tblPr")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits `w:tblpPr` (`CT_TblPPr`) from a [`TableFloatPosition`], in the
/// attribute order of the schema declaration. Absent (`None`) attributes are
/// omitted; an all-default position still emits an empty `<w:tblpPr/>` so the
/// element round-trips as a fixed point.
fn write_table_float_position(
    w: &mut Writer<Cursor<Vec<u8>>>,
    float: &TableFloatPosition,
) -> Result<(), ExportError> {
    let anchor_token = |anchor: TableAnchor| match anchor {
        TableAnchor::Text => "text",
        TableAnchor::Margin => "margin",
        TableAnchor::Page => "page",
    };
    let mut el = start("w:tblpPr");
    if let Some(v) = float.left_from_text_twips {
        el.push_attribute(("w:leftFromText", v.to_string().as_str()));
    }
    if let Some(v) = float.right_from_text_twips {
        el.push_attribute(("w:rightFromText", v.to_string().as_str()));
    }
    if let Some(v) = float.top_from_text_twips {
        el.push_attribute(("w:topFromText", v.to_string().as_str()));
    }
    if let Some(v) = float.bottom_from_text_twips {
        el.push_attribute(("w:bottomFromText", v.to_string().as_str()));
    }
    if let Some(anchor) = float.vert_anchor {
        el.push_attribute(("w:vertAnchor", anchor_token(anchor)));
    }
    if let Some(anchor) = float.horz_anchor {
        el.push_attribute(("w:horzAnchor", anchor_token(anchor)));
    }
    if let Some(spec) = float.x_spec {
        el.push_attribute((
            "w:tblpXSpec",
            match spec {
                TableXAlign::Left => "left",
                TableXAlign::Center => "center",
                TableXAlign::Right => "right",
                TableXAlign::Inside => "inside",
                TableXAlign::Outside => "outside",
            },
        ));
    }
    if let Some(v) = float.tbl_px_twips {
        el.push_attribute(("w:tblpX", v.to_string().as_str()));
    }
    if let Some(spec) = float.y_spec {
        el.push_attribute((
            "w:tblpYSpec",
            match spec {
                TableYAlign::Inline => "inline",
                TableYAlign::Top => "top",
                TableYAlign::Center => "center",
                TableYAlign::Bottom => "bottom",
                TableYAlign::Inside => "inside",
                TableYAlign::Outside => "outside",
            },
        ));
    }
    if let Some(v) = float.tbl_py_twips {
        el.push_attribute(("w:tblpY", v.to_string().as_str()));
    }
    w.write_event(Event::Empty(el)).map_err(pkg)?;
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

/// Emits a `w:cnfStyle` selector as the canonical 12-bit `@w:val` binary string
/// (the same bit order the importer reads). An all-false selector is `None` and
/// never reaches here. `w:cnfStyle` is the first child of both `CT_TrPr` and
/// `CT_TcPr`.
fn write_cnf_style(
    w: &mut Writer<Cursor<Vec<u8>>>,
    conditional_format: Option<CnfStyle>,
) -> Result<(), ExportError> {
    let Some(cnf) = conditional_format else {
        return Ok(());
    };
    let mut val = String::with_capacity(12);
    for on in [
        cnf.first_row,
        cnf.last_row,
        cnf.first_column,
        cnf.last_column,
        cnf.odd_v_band,
        cnf.even_v_band,
        cnf.odd_h_band,
        cnf.even_h_band,
        cnf.first_row_first_column,
        cnf.first_row_last_column,
        cnf.last_row_first_column,
        cnf.last_row_last_column,
    ] {
        val.push(if on { '1' } else { '0' });
    }
    let mut el = start("w:cnfStyle");
    el.push_attribute(("w:val", val.as_str()));
    w.write_event(Event::Empty(el)).map_err(pkg)?;
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
    write_cnf_style(w, properties.conditional_format)?;
    // Short-row grid skips (`w:gridBefore`/`w:gridAfter`) and their preferred
    // widths (`w:wBefore`/`w:wAfter`) follow `w:cnfStyle` and precede
    // `w:cantSplit`/`w:trHeight` in `CT_TrPrBase`.
    if let Some(count) = properties.grid_before {
        let mut el = start("w:gridBefore");
        el.push_attribute(("w:val", count.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(count) = properties.grid_after {
        let mut el = start("w:gridAfter");
        el.push_attribute(("w:val", count.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(width) = properties.w_before {
        write_table_width(w, "w:wBefore", width)?;
    }
    if let Some(width) = properties.w_after {
        write_table_width(w, "w:wAfter", width)?;
    }
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
    if let Some(spacing) = properties.cell_spacing_twips {
        let mut el = start("w:tblCellSpacing");
        el.push_attribute(("w:type", "dxa"));
        el.push_attribute(("w:w", spacing.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(alignment) = properties.alignment {
        let mut el = start("w:jc");
        el.push_attribute(("w:val", alignment_token(alignment)));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    // A tracked row insertion/deletion (`w:ins`/`w:del`) follows the base row
    // properties and precedes `w:trPrChange` in CT_TrPr.
    if let Some(revision) = &properties.row_revision {
        write_mark_revision(w, revision, "w:ins", "w:del")?;
    }
    // `w:trPrChange` is the last child of `w:trPr`; its `w:trPr` is the prior
    // snapshot. An all-default prior still emits a bare `<w:trPr/>`.
    if let Some(change) = &properties.prop_change {
        let mut el = start("w:trPrChange");
        push_prop_change_attrs(&mut el, change);
        w.write_event(Event::Start(el)).map_err(pkg)?;
        if change.prior.as_ref() == &TableRowProperties::default() {
            w.write_event(Event::Empty(start("w:trPr"))).map_err(pkg)?;
        } else {
            write_row_properties(w, change.prior.as_ref())?;
        }
        w.write_event(Event::End(BytesEnd::new("w:trPrChange")))
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
    write_cnf_style(w, properties.conditional_format)?;
    if let Some(width) = properties.width {
        write_table_width(w, "w:tcW", width)?;
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
    if properties.fit_text {
        w.write_event(Event::Empty(start("w:tcFitText")))
            .map_err(pkg)?;
    }
    if properties.hide_mark {
        w.write_event(Event::Empty(start("w:hideMark")))
            .map_err(pkg)?;
    }
    // Tracked cell changes (`w:cellIns`/`w:cellDel`/`w:cellMerge`) form
    // EG_CellMarkupElements, following the base cell properties and preceding
    // `w:tcPrChange` in CT_TcPr.
    if let Some(revision) = &properties.cell_revision {
        write_mark_revision(w, revision, "w:cellIns", "w:cellDel")?;
    }
    if let Some(merge) = &properties.cell_merge {
        write_cell_merge(w, merge)?;
    }
    // `w:tcPrChange` is the last child of `w:tcPr`; its `w:tcPr` is the prior
    // snapshot. An all-default prior still emits a bare `<w:tcPr/>`.
    if let Some(change) = &properties.prop_change {
        let mut el = start("w:tcPrChange");
        push_prop_change_attrs(&mut el, change);
        w.write_event(Event::Start(el)).map_err(pkg)?;
        if change.prior.as_ref() == &TableCellProperties::default() {
            w.write_event(Event::Empty(start("w:tcPr"))).map_err(pkg)?;
        } else {
            write_cell_properties(w, change.prior.as_ref())?;
        }
        w.write_event(Event::End(BytesEnd::new("w:tcPrChange")))
            .map_err(pkg)?;
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
    if shading.is_empty() {
        return Ok(());
    }
    let mut el = start("w:shd");
    el.push_attribute(("w:val", "clear"));
    el.push_attribute(("w:color", "auto"));
    // Theme background fill (`w:themeFill` + optional tint/shade); the formatted
    // tint/shade strings must outlive the attribute pushes.
    let tint_str;
    let shade_str;
    if let Some(theme) = &shading.theme_fill {
        el.push_attribute(("w:themeFill", theme_color_token(theme.slot)));
        if let Some(tint) = theme.theme_tint {
            tint_str = format!("{tint:02X}");
            el.push_attribute(("w:themeFillTint", tint_str.as_str()));
        }
        if let Some(shade) = theme.theme_shade {
            shade_str = format!("{shade:02X}");
            el.push_attribute(("w:themeFillShade", shade_str.as_str()));
        }
    }
    let fill_str;
    if let Some(fill) = &shading.fill {
        fill_str = rgb_hex(fill);
        el.push_attribute(("w:fill", fill_str.as_str()));
    }
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
}

fn rgb_hex(color: &RgbColor) -> String {
    format!("{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

/// The `w:themeColor`/`w:themeFill` token (`ST_ThemeColor`) for a theme slot. The
/// twelve canonical slot spellings; the four mapped aliases Word also accepts
/// normalize to these on import.
fn theme_color_token(slot: ThemeColorRef) -> &'static str {
    match slot {
        ThemeColorRef::Dark1 => "dark1",
        ThemeColorRef::Light1 => "light1",
        ThemeColorRef::Dark2 => "dark2",
        ThemeColorRef::Light2 => "light2",
        ThemeColorRef::Accent1 => "accent1",
        ThemeColorRef::Accent2 => "accent2",
        ThemeColorRef::Accent3 => "accent3",
        ThemeColorRef::Accent4 => "accent4",
        ThemeColorRef::Accent5 => "accent5",
        ThemeColorRef::Accent6 => "accent6",
        ThemeColorRef::Hyperlink => "hyperlink",
        ThemeColorRef::FollowedHyperlink => "followedHyperlink",
    }
}

fn write_paragraph(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &ParagraphProperties,
    inlines: &[InlineNode],
    ctx: &mut Ctx,
    para_id: Option<&str>,
) -> Result<(), ExportError> {
    let mut p = start("w:p");
    // `para_id` is set only for a comment's anchor paragraph, carrying the
    // durable `w14:paraId` its companion parts join on.
    if let Some(para_id) = para_id {
        p.push_attribute(("w14:paraId", para_id));
    }
    w.write_event(Event::Start(p)).map_err(pkg)?;
    // A per-paragraph section break resolves to its boundary in the shared
    // section table; it is emitted as the last `w:pPr` child.
    let section = properties
        .section_break
        .and_then(|id| ctx.defs.sections.iter().find(|boundary| boundary.id == id));
    write_paragraph_properties(w, properties, section)?;
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
    section: Option<&SectionBoundary>,
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
        (properties.contextual_spacing, "w:contextualSpacing"),
        (properties.suppress_line_numbers, "w:suppressLineNumbers"),
    ] {
        if flag {
            w.write_event(Event::Empty(start(name))).map_err(pkg)?;
        }
    }
    if let Some(frame) = &properties.drop_cap_frame {
        write_drop_cap_frame(w, frame)?;
    }
    // Tri-state paragraph toggles (`CT_OnOff`): bare = on, `w:val="0"` = an
    // explicit off, nothing = absent — so a default-ON toggle turned off survives.
    for (value, name) in [
        (properties.widow_control, "w:widowControl"),
        (properties.bidi, "w:bidi"),
        (properties.word_wrap, "w:wordWrap"),
        (properties.kinsoku, "w:kinsoku"),
        (properties.snap_to_grid, "w:snapToGrid"),
        (properties.mirror_indents, "w:mirrorIndents"),
        (properties.adjust_right_ind, "w:adjustRightInd"),
        (properties.suppress_auto_hyphens, "w:suppressAutoHyphens"),
        (properties.overflow_punct, "w:overflowPunct"),
        (properties.top_line_punct, "w:topLinePunct"),
        (properties.auto_space_de, "w:autoSpaceDE"),
        (properties.auto_space_dn, "w:autoSpaceDN"),
    ] {
        if let Some(on) = value {
            let mut el = start(name);
            if !on {
                el.push_attribute(("w:val", "0"));
            }
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
    }
    if let Some(alignment) = properties.text_alignment {
        let mut el = start("w:textAlignment");
        el.push_attribute((
            "w:val",
            match alignment {
                VerticalTextAlignment::Auto => "auto",
                VerticalTextAlignment::Baseline => "baseline",
                VerticalTextAlignment::Bottom => "bottom",
                VerticalTextAlignment::Center => "center",
                VerticalTextAlignment::Top => "top",
            },
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
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
        if let Some(before_auto) = spacing.before_auto {
            el.push_attribute(("w:beforeAutospacing", if before_auto { "1" } else { "0" }));
        }
        if let Some(after) = spacing.after_twips {
            el.push_attribute(("w:after", after.to_string().as_str()));
        }
        if let Some(after_auto) = spacing.after_auto {
            el.push_attribute(("w:afterAutospacing", if after_auto { "1" } else { "0" }));
        }
        match spacing.line_rule {
            // `atLeast`/`exact`: `w:line` is a twip value carried verbatim.
            Some(rule) => {
                if let Some(twips) = spacing.line_twips {
                    let name = match rule {
                        LineRule::Auto => "auto",
                        LineRule::AtLeast => "atLeast",
                        LineRule::Exact => "exact",
                    };
                    el.push_attribute(("w:line", twips.to_string().as_str()));
                    el.push_attribute(("w:lineRule", name));
                }
            }
            // `auto` (the default rule): `w:line` is a multiple in 240ths. The
            // importer reads `line * 100 / 240`; round the twips up so integer
            // division recovers the exact percent.
            None => {
                if let Some(percent) = spacing.line_percent {
                    let line = (u64::from(percent) * 240).div_ceil(100);
                    el.push_attribute(("w:line", line.to_string().as_str()));
                    el.push_attribute(("w:lineRule", "auto"));
                }
            }
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
    // The paragraph-mark run properties (`w:pPr > w:rPr`) precede `w:sectPr` in
    // CT_PPr. A `Some(default)` still emits a bare `<w:rPr/>` so the mark's
    // presence round-trips (the property writer elides an all-default value).
    // A tracked paragraph-mark insertion/deletion (`w:ins`/`w:del`) is the FIRST
    // child of the mark's `w:rPr` (CT_ParaRPr order), so when it is present the
    // mark rPr is written explicitly: the change element, then the mark's own run
    // property children (if any). Otherwise the existing path is preserved
    // exactly: `Some(default)` emits a bare `<w:rPr/>`, `Some(non-default)` its
    // full rPr, `None` nothing.
    if let Some(revision) = &properties.mark_revision {
        w.write_event(Event::Start(start("w:rPr"))).map_err(pkg)?;
        let element = match revision.kind {
            MarkRevisionKind::Insertion => "w:ins",
            MarkRevisionKind::Deletion => "w:del",
        };
        let mut el = start(element);
        if let Some(author) = &revision.author {
            el.push_attribute(("w:author", author.as_str()));
        }
        if let Some(date) = &revision.date {
            el.push_attribute(("w:date", date.as_str()));
        }
        if let Some(id) = &revision.revision_id {
            el.push_attribute(("w:id", id.as_str()));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
        if let Some(mark_run) = &properties.mark_run {
            write_run_property_children(w, mark_run)?;
        }
        w.write_event(Event::End(BytesEnd::new("w:rPr")))
            .map_err(pkg)?;
    } else if let Some(mark_run) = &properties.mark_run {
        if **mark_run == RunProperties::default() {
            w.write_event(Event::Empty(start("w:rPr"))).map_err(pkg)?;
        } else {
            write_run_properties(w, mark_run)?;
        }
    }
    // The section break precedes `w:pPrChange` in CT_PPr; it marks this paragraph
    // as a section's end.
    if let Some(section) = section {
        write_section_properties(w, section)?;
    }
    // `w:pPrChange` is the last child of `w:pPr` (after `w:sectPr`); its `w:pPr`
    // is the prior snapshot (CT_PPrBase — no mark rPr, sectPr, or nested change,
    // so it is emitted with no section). An all-default prior still emits a bare
    // `<w:pPr/>` so the required child is present.
    if let Some(change) = &properties.prop_change {
        let mut el = start("w:pPrChange");
        push_prop_change_attrs(&mut el, change);
        w.write_event(Event::Start(el)).map_err(pkg)?;
        if change.prior.as_ref() == &ParagraphProperties::default() {
            w.write_event(Event::Empty(start("w:pPr"))).map_err(pkg)?;
        } else {
            write_paragraph_properties(w, change.prior.as_ref(), None)?;
        }
        w.write_event(Event::End(BytesEnd::new("w:pPrChange")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:pPr")))
        .map_err(pkg)?;
    Ok(())
}

fn write_drop_cap_frame(
    w: &mut Writer<Cursor<Vec<u8>>>,
    frame: &DropCapFrame,
) -> Result<(), ExportError> {
    let mut el = start("w:framePr");
    el.push_attribute((
        "w:dropCap",
        match frame.mode {
            DropCapMode::Drop => "drop",
            DropCapMode::Margin => "margin",
        },
    ));
    el.push_attribute(("w:lines", frame.lines.to_string().as_str()));
    if let Some(wrap) = frame.wrap {
        el.push_attribute((
            "w:wrap",
            match wrap {
                FrameWrap::Around => "around",
                FrameWrap::NotBeside => "notBeside",
                FrameWrap::Auto => "auto",
                FrameWrap::None => "none",
            },
        ));
    }
    if let Some(anchor) = frame.horizontal_anchor {
        el.push_attribute((
            "w:hAnchor",
            match anchor {
                FrameHorizontalAnchor::Margin => "margin",
                FrameHorizontalAnchor::Page => "page",
                FrameHorizontalAnchor::Text => "text",
            },
        ));
    }
    if let Some(anchor) = frame.vertical_anchor {
        el.push_attribute((
            "w:vAnchor",
            match anchor {
                FrameVerticalAnchor::Margin => "margin",
                FrameVerticalAnchor::Page => "page",
                FrameVerticalAnchor::Text => "text",
            },
        ));
    }
    if let Some(alignment) = frame.horizontal_alignment {
        el.push_attribute((
            "w:xAlign",
            match alignment {
                FrameHorizontalAlignment::Center => "center",
                FrameHorizontalAlignment::Inside => "inside",
                FrameHorizontalAlignment::Left => "left",
                FrameHorizontalAlignment::Outside => "outside",
                FrameHorizontalAlignment::Right => "right",
            },
        ));
    }
    if let Some(alignment) = frame.vertical_alignment {
        el.push_attribute((
            "w:yAlign",
            match alignment {
                FrameVerticalAlignment::Bottom => "bottom",
                FrameVerticalAlignment::Center => "center",
                FrameVerticalAlignment::Inline => "inline",
                FrameVerticalAlignment::Inside => "inside",
                FrameVerticalAlignment::Outside => "outside",
                FrameVerticalAlignment::Top => "top",
            },
        ));
    }
    if let Some(position) = frame.horizontal_position_twips {
        el.push_attribute(("w:x", position.to_string().as_str()));
    }
    if let Some(position) = frame.vertical_position_twips {
        el.push_attribute(("w:y", position.to_string().as_str()));
    }
    if let Some(space) = frame.horizontal_space_twips {
        el.push_attribute(("w:hSpace", space.to_string().as_str()));
    }
    if let Some(space) = frame.vertical_space_twips {
        el.push_attribute(("w:vSpace", space.to_string().as_str()));
    }
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
}

/// One embedded-object part relationship to emit in `document.xml.rels`:
/// (relationship id, relationship type URI, `word/`-relative target).
type EmbeddedRelEntry = (String, String, String);

/// Collects every embedded-object part relationship in document order, deduped
/// by relationship id (the first occurrence wins). Walked before the body is
/// written so the ids can be reserved against hyperlink minting.
fn collect_embedded_rels(document: &Document) -> Vec<EmbeddedRelEntry> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for block in document.body() {
        collect_block_embedded_rels(block, &mut out, &mut seen);
    }
    out
}

fn collect_block_embedded_rels(
    block: &BlockNode,
    out: &mut Vec<EmbeddedRelEntry>,
    seen: &mut BTreeSet<String>,
) {
    match block {
        BlockNode::Paragraph(paragraph) => {
            for inline in &paragraph.inlines {
                collect_inline_embedded_rels(inline, out, seen);
            }
        }
        BlockNode::Table(table) => {
            for row in &table.rows {
                for cell in &row.cells {
                    for block in &cell.blocks {
                        collect_block_embedded_rels(block, out, seen);
                    }
                }
            }
        }
        BlockNode::Sdt(sdt) => {
            for block in &sdt.blocks {
                collect_block_embedded_rels(block, out, seen);
            }
        }
        // An alt chunk references its preserved part exactly like an embedded
        // object; its relationship is emitted from the node (verbatim `r:id`).
        BlockNode::AltChunk(chunk) => push_embedded_part(&chunk.part, out, seen),
    }
}

fn collect_inline_embedded_rels(
    inline: &InlineNode,
    out: &mut Vec<EmbeddedRelEntry>,
    seen: &mut BTreeSet<String>,
) {
    match inline {
        InlineNode::EmbeddedObject(object) => {
            push_embedded_part(&object.part, out, seen);
            for part in &object.extra_parts {
                push_embedded_part(part, out, seen);
            }
        }
        InlineNode::Hyperlink(link) => {
            for child in &link.inlines {
                collect_inline_embedded_rels(child, out, seen);
            }
        }
        InlineNode::Field(field) => {
            for child in &field.inlines {
                collect_inline_embedded_rels(child, out, seen);
            }
        }
        InlineNode::Revision(revision) => {
            for child in &revision.inlines {
                collect_inline_embedded_rels(child, out, seen);
            }
        }
        InlineNode::Sdt(sdt) => {
            for child in &sdt.inlines {
                collect_inline_embedded_rels(child, out, seen);
            }
        }
        InlineNode::TextBox(text_box) => {
            for block in &text_box.blocks {
                collect_block_embedded_rels(block, out, seen);
            }
        }
        _ => {}
    }
}

fn push_embedded_part(
    part: &EmbeddedPart,
    out: &mut Vec<EmbeddedRelEntry>,
    seen: &mut BTreeSet<String>,
) {
    if seen.insert(part.relationship_id.clone()) {
        out.push((
            part.relationship_id.clone(),
            part.relationship_type.clone(),
            media_target(&part.part_name).to_owned(),
        ));
    }
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
                    // An in-target fragment rides alongside the `r:id` base URL.
                    if let Some(anchor) = &ext.anchor {
                        el.push_attribute(("w:anchor", anchor.as_str()));
                    }
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
        // A field. An ordinary field is a self-contained `w:fldSimple`. A legacy
        // form field carries a `w:ffData` block, which is only valid inside a
        // complex field's `fldChar begin` — so it is re-emitted as the four-run
        // complex-field sequence begin(+ffData) / instrText / separate / end,
        // with the cached result between separate and end.
        InlineNode::Field(field) => match &field.form {
            None => {
                let mut el = start("w:fldSimple");
                el.push_attribute(("w:instr", field.instruction.as_str()));
                w.write_event(Event::Start(el)).map_err(pkg)?;
                for child in &field.inlines {
                    write_inline(w, child, ctx, in_deletion)?;
                }
                w.write_event(Event::End(BytesEnd::new("w:fldSimple")))
                    .map_err(pkg)?;
            }
            Some(form) => {
                // `fldChar begin` carrying the ffData block.
                w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
                let mut begin = start("w:fldChar");
                begin.push_attribute(("w:fldCharType", "begin"));
                w.write_event(Event::Start(begin)).map_err(pkg)?;
                write_form_field_data(w, form)?;
                w.write_event(Event::End(BytesEnd::new("w:fldChar")))
                    .map_err(pkg)?;
                w.write_event(Event::End(BytesEnd::new("w:r")))
                    .map_err(pkg)?;
                // The field instruction (`w:instrText`, whitespace preserved).
                w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
                let mut instr = start("w:instrText");
                instr.push_attribute(("xml:space", "preserve"));
                w.write_event(Event::Start(instr)).map_err(pkg)?;
                w.write_event(Event::Text(BytesText::new(&field.instruction)))
                    .map_err(pkg)?;
                w.write_event(Event::End(BytesEnd::new("w:instrText")))
                    .map_err(pkg)?;
                w.write_event(Event::End(BytesEnd::new("w:r")))
                    .map_err(pkg)?;
                // `fldChar separate`, then the cached-result inlines.
                w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
                let mut separate = start("w:fldChar");
                separate.push_attribute(("w:fldCharType", "separate"));
                w.write_event(Event::Empty(separate)).map_err(pkg)?;
                w.write_event(Event::End(BytesEnd::new("w:r")))
                    .map_err(pkg)?;
                for child in &field.inlines {
                    write_inline(w, child, ctx, in_deletion)?;
                }
                // `fldChar end`.
                w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
                let mut end = start("w:fldChar");
                end.push_attribute(("w:fldCharType", "end"));
                w.write_event(Event::Empty(end)).map_err(pkg)?;
                w.write_event(Event::End(BytesEnd::new("w:r")))
                    .map_err(pkg)?;
            }
        },
        // A tracked-change or tracked-move run wrapper. Its own runs are deleted
        // text when this deletes (a `Deletion` or a move-source `MoveFrom`) or
        // when already inside a deletion; an insertion or move-destination keeps
        // the inherited flag. `w:moveFrom` runs carry `w:delText`, `w:moveTo` runs
        // carry `w:t` — exactly like `w:del`/`w:ins`.
        InlineNode::Revision(revision) => {
            let (name, deleted) = match revision.kind {
                RevisionKind::Insertion => ("w:ins", in_deletion),
                RevisionKind::Deletion => ("w:del", true),
                RevisionKind::MoveFrom => ("w:moveFrom", true),
                RevisionKind::MoveTo => ("w:moveTo", in_deletion),
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
        // A tracked-move range marker (zero-width). The pairing `w:id` and the
        // move `w:name` are re-emitted verbatim; `w:author`/`w:date` restore the
        // move metadata. The start/end pair is self-contained (the shared
        // `move_id`), so no definition table is consulted.
        InlineNode::MoveRangeStart(marker) => {
            let element = match marker.kind {
                MoveKind::From => "w:moveFromRangeStart",
                MoveKind::To => "w:moveToRangeStart",
            };
            let mut el = start(element);
            el.push_attribute(("w:id", marker.move_id.as_str()));
            el.push_attribute(("w:name", marker.name.as_str()));
            if let Some(author) = &marker.author {
                el.push_attribute(("w:author", author.as_str()));
            }
            if let Some(date) = &marker.date {
                el.push_attribute(("w:date", date.as_str()));
            }
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        InlineNode::MoveRangeEnd(marker) => {
            let element = match marker.kind {
                MoveKind::From => "w:moveFromRangeEnd",
                MoveKind::To => "w:moveToRangeEnd",
            };
            let mut el = start(element);
            el.push_attribute(("w:id", marker.move_id.as_str()));
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
        // A note's own auto-number mark (`w:footnoteRef`/`w:endnoteRef`), inside a
        // footnote/endnote body. It carries no id (the number is the enclosing
        // note's), and re-emits its enclosing run's formatting.
        InlineNode::NoteNumberMark(mark) => {
            let element = match mark.kind {
                NoteKind::Footnote => "w:footnoteRef",
                NoteKind::Endnote => "w:endnoteRef",
            };
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            write_run_properties(w, &mark.properties)?;
            w.write_event(Event::Empty(start(element))).map_err(pkg)?;
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
        // A comment range marker (zero-width). It brackets the commented span in
        // paragraph flow — a sibling of runs, not wrapped in `w:r` — and its
        // `w:id` derives from the shared `CommentId` (the same token the
        // `w:commentReference` and the comment part carry).
        InlineNode::CommentRangeStart(marker) => {
            let mut el = start("w:commentRangeStart");
            el.push_attribute(("w:id", comment_id_token(marker.comment).as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        InlineNode::CommentRangeEnd(marker) => {
            let mut el = start("w:commentRangeEnd");
            el.push_attribute(("w:id", comment_id_token(marker.comment).as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        // An inline drawing: the minimal `w:drawing`/`wp:inline`/`pic:pic`
        // scaffold whose one load-bearing attribute is `a:blip@r:embed`, the
        // media's (verbatim) relationship id. The importer discards the rest.
        InlineNode::Drawing(drawing) => {
            let Some(reference) = ctx.defs.media.get(&drawing.media) else {
                return Ok(());
            };
            let embed = reference.relationship_id.clone();
            write_drawing(
                w,
                &embed,
                drawing.extent.as_ref(),
                drawing.descr.as_deref(),
                drawing.crop.as_ref(),
                drawing.border,
                Xfrm2D {
                    rotation: drawing.rotation,
                    flip_h: drawing.flip_h,
                    flip_v: drawing.flip_v,
                },
            )?;
        }
        // An anchored (floating) drawing: a `w:drawing`/`wp:anchor` carrying the
        // picture's position, wrap, z-order, and alt text.
        InlineNode::AnchoredDrawing(drawing) => {
            let Some(reference) = ctx.defs.media.get(&drawing.media) else {
                return Ok(());
            };
            let embed = reference.relationship_id.clone();
            write_anchored_drawing(w, &embed, drawing)?;
        }
        // An embedded object (chart / SmartArt diagram / OLE): the drawing wrapper
        // (chart/diagram) or `w:object` (OLE) referencing the preserved part(s) by
        // their verbatim `r:id`; the relationships are emitted in
        // `document.xml.rels` (see `collect_embedded_rels`).
        InlineNode::EmbeddedObject(object) => {
            write_embedded_object(w, object, ctx)?;
        }
        // A DrawingML text box: preserve whether it is inline or floating along
        // with its extent, anchor, z key, fill, and outline.
        InlineNode::TextBox(text_box) => {
            write_text_box(w, text_box, ctx)?;
        }
        // A DrawingML group: a floating `w:drawing`/`wp:anchor` wrapping a
        // `wpg:wgp` of positioned children (pictures, text boxes, shapes, nested
        // groups). Round-trips through the importer's group path.
        InlineNode::Group(group) => {
            write_group(w, group, ctx)?;
        }
        // An opaque math object: write the retained OMML subtree verbatim (a
        // direct inline child of `w:p`, like a hyperlink). The `m:` prefix is
        // declared on the `w:document` root; the plain-text fallback is not
        // re-emitted (the OMML carries the authoritative `m:t` runs).
        InlineNode::Math(math) => {
            w.get_mut().write_all(math.omml.as_bytes()).map_err(pkg)?;
        }
        // A symbol glyph: an empty `w:sym` (font + hex code point) inside its own
        // `w:r`, mirroring the tab/break run-child shape. `w:char` is written as
        // uppercase, at least four hex digits (Word's canonical form, e.g.
        // `F0FC`); the importer parses it back case-insensitively.
        InlineNode::Symbol(symbol) => {
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            write_run_properties(w, &symbol.properties)?;
            let char = format!("{:04X}", symbol.char);
            let mut sym = start("w:sym");
            sym.push_attribute(("w:font", symbol.font.as_str()));
            sym.push_attribute(("w:char", char.as_str()));
            w.write_event(Event::Empty(sym)).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:r")))
                .map_err(pkg)?;
        }
        // A horizontal rule: a `w:pict` wrapping a `v:rect` with `o:hr="t"` (Word's
        // "Insert → Horizontal Line"). The rule spans the full content width, so a
        // `width:0` style is written (Word ignores it for an `o:hr`); `height` is the
        // thickness (points), `fillcolor` the color, `o:hralign` the alignment, and
        // `o:hrpct` the width fraction (per-mille) — omitted at full width, matching
        // Word. The `v:`/`o:` prefixes are declared on the `w:document` root.
        InlineNode::HorizontalRule(rule) => {
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            w.write_event(Event::Start(start("w:pict"))).map_err(pkg)?;
            let mut rect = start("v:rect");
            let thickness_pt = rule.thickness_emu as f64 / 12700.0;
            rect.push_attribute(("style", format!("width:0;height:{thickness_pt}pt").as_str()));
            rect.push_attribute(("o:hr", "t"));
            rect.push_attribute(("o:hrstd", "t"));
            rect.push_attribute((
                "o:hralign",
                match rule.align {
                    HorizontalRuleAlign::Left => "left",
                    HorizontalRuleAlign::Center => "center",
                    HorizontalRuleAlign::Right => "right",
                },
            ));
            if rule.width_permille < 1000 {
                rect.push_attribute(("o:hrpct", rule.width_permille.to_string().as_str()));
            }
            let fillcolor = format!(
                "#{:02X}{:02X}{:02X}",
                rule.color.r, rule.color.g, rule.color.b
            );
            rect.push_attribute(("fillcolor", fillcolor.as_str()));
            rect.push_attribute(("stroked", "f"));
            w.write_event(Event::Empty(rect)).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:pict")))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:r")))
                .map_err(pkg)?;
        }
        // A non-breaking / soft hyphen glyph: an empty run child, mirroring the
        // tab/break/symbol run-child shape. The importer reads them back inside the
        // enclosing `w:r`.
        InlineNode::NoBreakHyphen(_) => {
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            w.write_event(Event::Empty(start("w:noBreakHyphen")))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:r")))
                .map_err(pkg)?;
        }
        InlineNode::SoftHyphen(_) => {
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            w.write_event(Event::Empty(start("w:softHyphen")))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:r")))
                .map_err(pkg)?;
        }
        // An absolute-position tab: an empty `w:ptab` run child carrying its three
        // required attributes (alignment / relativeTo / leader).
        InlineNode::PositionalTab(tab) => {
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            let mut ptab = start("w:ptab");
            ptab.push_attribute((
                "w:alignment",
                match tab.alignment {
                    PositionalTabAlignment::Left => "left",
                    PositionalTabAlignment::Center => "center",
                    PositionalTabAlignment::Right => "right",
                },
            ));
            ptab.push_attribute((
                "w:relativeTo",
                match tab.relative_to {
                    PositionalTabRelativeTo::Margin => "margin",
                    PositionalTabRelativeTo::Indent => "indent",
                },
            ));
            ptab.push_attribute((
                "w:leader",
                match tab.leader {
                    PositionalTabLeader::None => "none",
                    PositionalTabLeader::Dot => "dot",
                    PositionalTabLeader::Hyphen => "hyphen",
                    PositionalTabLeader::Underscore => "underscore",
                    PositionalTabLeader::MiddleDot => "middleDot",
                },
            ));
            w.write_event(Event::Empty(ptab)).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:r")))
                .map_err(pkg)?;
        }
    }
    Ok(())
}

/// Emits an inline `w:drawing` (embedded picture) referencing `embed` — the
/// media relationship id the importer resolves through the media table. Only
/// `wp:extent` and `a:blip@r:embed` are read back; the rest is fixed scaffold.
fn write_drawing(
    w: &mut Writer<Cursor<Vec<u8>>>,
    embed: &str,
    extent: Option<&Extent>,
    descr: Option<&str>,
    crop: Option<&CropRect>,
    border: Option<ShapeStroke>,
    xfrm: Xfrm2D,
) -> Result<(), ExportError> {
    let (cx, cy) = extent.map_or((0, 0), |extent| (extent.width_emu, extent.height_emu));
    w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
    w.write_event(Event::Start(start("w:drawing")))
        .map_err(pkg)?;
    let mut inline = start("wp:inline");
    for name in ["distT", "distB", "distL", "distR"] {
        inline.push_attribute((name, "0"));
    }
    w.write_event(Event::Start(inline)).map_err(pkg)?;
    if let Some(extent) = extent {
        let mut el = start("wp:extent");
        el.push_attribute(("cx", extent.width_emu.to_string().as_str()));
        el.push_attribute(("cy", extent.height_emu.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    let mut doc_pr = start("wp:docPr");
    doc_pr.push_attribute(("id", "1"));
    doc_pr.push_attribute(("name", "Picture 1"));
    if let Some(descr) = descr {
        doc_pr.push_attribute(("descr", descr));
    }
    w.write_event(Event::Empty(doc_pr)).map_err(pkg)?;
    write_pic_graphic(w, embed, cx, cy, crop, border, xfrm)?;
    w.write_event(Event::End(BytesEnd::new("wp:inline")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:drawing")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:r")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits the `a:graphic`/`pic:pic` subtree shared by inline and anchored
/// drawings: the picture frame referencing `embed` (the media relationship id)
/// at the `cx`×`cy` EMU extent. The importer reads back `a:blip@r:embed` and the
/// enclosing `wp:extent`; the geometry here is fixed scaffold.
/// Writes an `a:srcRect` crop element (the four ST_Percentage edge fractions),
/// omitting any edge that is zero. Called inside a `pic:blipFill`, before the
/// `a:stretch`. A `None`/identity crop writes nothing.
fn write_src_rect(
    w: &mut Writer<Cursor<Vec<u8>>>,
    crop: Option<&CropRect>,
) -> Result<(), ExportError> {
    let Some(crop) = crop.filter(|crop| !crop.is_identity()) else {
        return Ok(());
    };
    let mut el = start("a:srcRect");
    for (name, value) in [
        ("l", crop.left),
        ("t", crop.top),
        ("r", crop.right),
        ("b", crop.bottom),
    ] {
        if value != 0 {
            el.push_attribute((name, value.to_string().as_str()));
        }
    }
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
}

fn write_pic_graphic(
    w: &mut Writer<Cursor<Vec<u8>>>,
    embed: &str,
    cx: i64,
    cy: i64,
    crop: Option<&CropRect>,
    border: Option<ShapeStroke>,
    xfrm: Xfrm2D,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("a:graphic")))
        .map_err(pkg)?;
    let mut graphic_data = start("a:graphicData");
    graphic_data.push_attribute(("uri", PIC_NS));
    w.write_event(Event::Start(graphic_data)).map_err(pkg)?;
    w.write_event(Event::Start(start("pic:pic"))).map_err(pkg)?;
    w.write_event(Event::Start(start("pic:nvPicPr")))
        .map_err(pkg)?;
    let mut c_nv_pr = start("pic:cNvPr");
    c_nv_pr.push_attribute(("id", "1"));
    c_nv_pr.push_attribute(("name", "Picture 1"));
    w.write_event(Event::Empty(c_nv_pr)).map_err(pkg)?;
    w.write_event(Event::Empty(start("pic:cNvPicPr")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("pic:nvPicPr")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("pic:blipFill")))
        .map_err(pkg)?;
    let mut blip = start("a:blip");
    blip.push_attribute(("r:embed", embed));
    w.write_event(Event::Empty(blip)).map_err(pkg)?;
    write_src_rect(w, crop)?;
    w.write_event(Event::Start(start("a:stretch")))
        .map_err(pkg)?;
    w.write_event(Event::Empty(start("a:fillRect")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("a:stretch")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("pic:blipFill")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("pic:spPr")))
        .map_err(pkg)?;
    w.write_event(Event::Start(xfrm_start(
        xfrm.rotation,
        xfrm.flip_h,
        xfrm.flip_v,
    )))
    .map_err(pkg)?;
    let mut off = start("a:off");
    off.push_attribute(("x", "0"));
    off.push_attribute(("y", "0"));
    w.write_event(Event::Empty(off)).map_err(pkg)?;
    let mut ext = start("a:ext");
    ext.push_attribute(("cx", cx.to_string().as_str()));
    ext.push_attribute(("cy", cy.to_string().as_str()));
    w.write_event(Event::Empty(ext)).map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("a:xfrm")))
        .map_err(pkg)?;
    let mut geom = start("a:prstGeom");
    geom.push_attribute(("prst", "rect"));
    w.write_event(Event::Start(geom)).map_err(pkg)?;
    w.write_event(Event::Empty(start("a:avLst"))).map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("a:prstGeom")))
        .map_err(pkg)?;
    // A framed picture keeps its `a:ln` outline (schema order: after the geometry).
    // Absent border = no `a:ln` (the default), so it is only written when present.
    if border.is_some() {
        write_outline(w, border)?;
    }
    w.write_event(Event::End(BytesEnd::new("pic:spPr")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("pic:pic")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("a:graphicData")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("a:graphic")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a floating `w:drawing`/`wp:anchor` for an [`AnchoredDrawing`]: the
/// position (`wp:positionH`/`wp:positionV`), size (`wp:extent`), wrap
/// (`wp:wrap*`), z-order (`@behindDoc`), and alt text (`wp:docPr@descr`), around
/// the shared picture frame. Round-trips through the importer's `wp:anchor` path.
fn write_anchored_drawing(
    w: &mut Writer<Cursor<Vec<u8>>>,
    embed: &str,
    drawing: &AnchoredDrawing,
) -> Result<(), ExportError> {
    let anchor = &drawing.anchor;
    let (cx, cy) = (drawing.extent.width_emu, drawing.extent.height_emu);
    w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
    w.write_event(Event::Start(start("w:drawing")))
        .map_err(pkg)?;
    let mut el = start("wp:anchor");
    let dist_t = anchor.wrap_distances.top_emu.to_string();
    let dist_b = anchor.wrap_distances.bottom_emu.to_string();
    let dist_l = anchor.wrap_distances.start_emu.to_string();
    let dist_r = anchor.wrap_distances.end_emu.to_string();
    el.push_attribute(("distT", dist_t.as_str()));
    el.push_attribute(("distB", dist_b.as_str()));
    el.push_attribute(("distL", dist_l.as_str()));
    el.push_attribute(("distR", dist_r.as_str()));
    el.push_attribute(("simplePos", "0"));
    // `relativeHeight` is written only when the model carries one, so an anchor
    // that omitted it round-trips as `None` (a write -> reopen fixed point); Word
    // and the importer both treat an absent value as the default (bottom) z.
    if let Some(relative_height) = drawing.relative_height {
        el.push_attribute(("relativeHeight", relative_height.to_string().as_str()));
    }
    el.push_attribute(("behindDoc", if anchor.behind_doc { "1" } else { "0" }));
    el.push_attribute(("locked", "0"));
    el.push_attribute(("layoutInCell", "1"));
    el.push_attribute(("allowOverlap", "1"));
    w.write_event(Event::Start(el)).map_err(pkg)?;
    // `simplePos` is a required child; `simplePos="0"` above means it is ignored
    // (the positionH/V pair drives placement), so a zero point suffices.
    let mut simple_pos = start("wp:simplePos");
    simple_pos.push_attribute(("x", "0"));
    simple_pos.push_attribute(("y", "0"));
    w.write_event(Event::Empty(simple_pos)).map_err(pkg)?;
    write_position_h(w, &anchor.horizontal)?;
    write_position_v(w, &anchor.vertical)?;
    let mut extent = start("wp:extent");
    extent.push_attribute(("cx", cx.to_string().as_str()));
    extent.push_attribute(("cy", cy.to_string().as_str()));
    w.write_event(Event::Empty(extent)).map_err(pkg)?;
    write_wrap(w, anchor.wrap, anchor.wrap_polygon.as_deref())?;
    let mut doc_pr = start("wp:docPr");
    doc_pr.push_attribute(("id", "1"));
    doc_pr.push_attribute(("name", "Picture 1"));
    if let Some(descr) = &drawing.descr {
        doc_pr.push_attribute(("descr", descr.as_str()));
    }
    w.write_event(Event::Empty(doc_pr)).map_err(pkg)?;
    write_pic_graphic(
        w,
        embed,
        cx,
        cy,
        drawing.crop.as_ref(),
        drawing.border,
        Xfrm2D {
            rotation: drawing.rotation,
            flip_h: drawing.flip_h,
            flip_v: drawing.flip_v,
        },
    )?;
    w.write_event(Event::End(BytesEnd::new("wp:anchor")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:drawing")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:r")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a floating `w:drawing`/`wp:anchor` wrapping a `wpg:wgp` for a
/// [`WordprocessingGroup`]: the anchor (position/extent/z-order), then the group
/// transform and each child in document order. Round-trips through the importer's
/// group path.
fn write_group(
    w: &mut Writer<Cursor<Vec<u8>>>,
    group: &WordprocessingGroup,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    let anchor = group.anchor.as_ref();
    w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
    w.write_event(Event::Start(start("w:drawing")))
        .map_err(pkg)?;
    let mut el = start("wp:anchor");
    let dist_t = anchor
        .map_or(0, |anchor| anchor.wrap_distances.top_emu)
        .to_string();
    let dist_b = anchor
        .map_or(0, |anchor| anchor.wrap_distances.bottom_emu)
        .to_string();
    let dist_l = anchor
        .map_or(0, |anchor| anchor.wrap_distances.start_emu)
        .to_string();
    let dist_r = anchor
        .map_or(0, |anchor| anchor.wrap_distances.end_emu)
        .to_string();
    el.push_attribute(("distT", dist_t.as_str()));
    el.push_attribute(("distB", dist_b.as_str()));
    el.push_attribute(("distL", dist_l.as_str()));
    el.push_attribute(("distR", dist_r.as_str()));
    el.push_attribute(("simplePos", "0"));
    if let Some(relative_height) = group.relative_height {
        el.push_attribute(("relativeHeight", relative_height.to_string().as_str()));
    }
    el.push_attribute((
        "behindDoc",
        if anchor.is_some_and(|a| a.behind_doc) {
            "1"
        } else {
            "0"
        },
    ));
    el.push_attribute(("locked", "0"));
    el.push_attribute(("layoutInCell", "1"));
    el.push_attribute(("allowOverlap", "1"));
    w.write_event(Event::Start(el)).map_err(pkg)?;
    let mut simple_pos = start("wp:simplePos");
    simple_pos.push_attribute(("x", "0"));
    simple_pos.push_attribute(("y", "0"));
    w.write_event(Event::Empty(simple_pos)).map_err(pkg)?;
    if let Some(anchor) = anchor {
        write_position_h(w, &anchor.horizontal)?;
        write_position_v(w, &anchor.vertical)?;
        write_wrap_after_extent(w, group, anchor.wrap, anchor.wrap_polygon.as_deref())?;
    } else {
        write_extent_only(w, group)?;
        write_wrap(w, WrapMode::None, None)?;
        write_group_body(w, group, ctx)?;
        close_group_drawing(w)?;
        return Ok(());
    }
    write_group_body(w, group, ctx)?;
    close_group_drawing(w)?;
    Ok(())
}

/// Emits `wp:extent` for the group (its `wp:extent`).
fn write_extent_only(
    w: &mut Writer<Cursor<Vec<u8>>>,
    group: &WordprocessingGroup,
) -> Result<(), ExportError> {
    let mut extent = start("wp:extent");
    extent.push_attribute(("cx", group.extent.width_emu.to_string().as_str()));
    extent.push_attribute(("cy", group.extent.height_emu.to_string().as_str()));
    w.write_event(Event::Empty(extent)).map_err(pkg)?;
    Ok(())
}

/// Emits `wp:extent` + wrap + `wp:docPr` after the position pair.
fn write_wrap_after_extent(
    w: &mut Writer<Cursor<Vec<u8>>>,
    group: &WordprocessingGroup,
    wrap: WrapMode,
    polygon: Option<&[PointEmu]>,
) -> Result<(), ExportError> {
    write_extent_only(w, group)?;
    write_wrap(w, wrap, polygon)?;
    let mut doc_pr = start("wp:docPr");
    doc_pr.push_attribute(("id", "1"));
    doc_pr.push_attribute(("name", "Group 1"));
    w.write_event(Event::Empty(doc_pr)).map_err(pkg)?;
    Ok(())
}

/// Emits `a:graphic > a:graphicData(wpg) > wpg:wgp` with the group's transform and
/// children.
fn write_group_body(
    w: &mut Writer<Cursor<Vec<u8>>>,
    group: &WordprocessingGroup,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("a:graphic")))
        .map_err(pkg)?;
    let mut graphic_data = start("a:graphicData");
    graphic_data.push_attribute(("uri", WPG_NS));
    w.write_event(Event::Start(graphic_data)).map_err(pkg)?;
    write_wgp(w, group, "wpg:wgp", ctx)?;
    w.write_event(Event::End(BytesEnd::new("a:graphicData")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("a:graphic")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a `wpg:wgp` (top-level) or `wpg:grpSp` (nested) with its `grpSpPr` xfrm
/// and children.
fn write_wgp(
    w: &mut Writer<Cursor<Vec<u8>>>,
    group: &WordprocessingGroup,
    tag: &str,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start(tag))).map_err(pkg)?;
    let mut c_nv_pr = start("wpg:cNvPr");
    c_nv_pr.push_attribute(("id", "0"));
    c_nv_pr.push_attribute(("name", "Group"));
    w.write_event(Event::Empty(c_nv_pr)).map_err(pkg)?;
    w.write_event(Event::Empty(start("wpg:cNvGrpSpPr")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("wpg:grpSpPr")))
        .map_err(pkg)?;
    write_group_xfrm(w, &group.transform)?;
    w.write_event(Event::End(BytesEnd::new("wpg:grpSpPr")))
        .map_err(pkg)?;
    for child in &group.children {
        match child {
            GroupChild::Picture(picture) => {
                let embed = ctx
                    .defs
                    .media
                    .get(&picture.media)
                    .map(|reference| reference.relationship_id.clone());
                if let Some(embed) = embed {
                    write_group_picture(
                        w,
                        &embed,
                        picture.offset,
                        picture.extent,
                        picture.crop.as_ref(),
                        picture.border,
                        Xfrm2D {
                            rotation: picture.rotation,
                            flip_h: picture.flip_h,
                            flip_v: picture.flip_v,
                        },
                    )?;
                }
            }
            GroupChild::TextBox(text_box) => write_group_text_box(w, text_box, ctx)?,
            GroupChild::Shape(shape) => write_group_shape(w, shape)?,
            GroupChild::Group(nested) => write_wgp(w, nested, "wpg:grpSp", ctx)?,
        }
    }
    w.write_event(Event::End(BytesEnd::new(tag))).map_err(pkg)?;
    Ok(())
}

/// Emits an `a:xfrm` for a group transform (off/ext/chOff/chExt).
fn write_group_xfrm(
    w: &mut Writer<Cursor<Vec<u8>>>,
    transform: &GroupTransform,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(xfrm_start(
        transform.rotation,
        transform.flip_h,
        transform.flip_v,
    )))
    .map_err(pkg)?;
    write_point(w, "a:off", transform.offset)?;
    write_ext(w, "a:ext", transform.extent)?;
    write_point(w, "a:chOff", transform.child_offset)?;
    write_ext(w, "a:chExt", transform.child_extent)?;
    w.write_event(Event::End(BytesEnd::new("a:xfrm")))
        .map_err(pkg)?;
    Ok(())
}

fn write_point(
    w: &mut Writer<Cursor<Vec<u8>>>,
    tag: &str,
    point: PointEmu,
) -> Result<(), ExportError> {
    let mut el = start(tag);
    el.push_attribute(("x", point.x_emu.to_string().as_str()));
    el.push_attribute(("y", point.y_emu.to_string().as_str()));
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
}

fn write_ext(w: &mut Writer<Cursor<Vec<u8>>>, tag: &str, ext: Extent) -> Result<(), ExportError> {
    let mut el = start(tag);
    el.push_attribute(("cx", ext.width_emu.to_string().as_str()));
    el.push_attribute(("cy", ext.height_emu.to_string().as_str()));
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
}

/// Emits a group child `pic:pic` positioned at `offset`, sized `extent`.
fn write_group_picture(
    w: &mut Writer<Cursor<Vec<u8>>>,
    embed: &str,
    offset: PointEmu,
    extent: Extent,
    crop: Option<&CropRect>,
    border: Option<ShapeStroke>,
    xfrm: Xfrm2D,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("pic:pic"))).map_err(pkg)?;
    w.write_event(Event::Start(start("pic:nvPicPr")))
        .map_err(pkg)?;
    let mut c_nv_pr = start("pic:cNvPr");
    c_nv_pr.push_attribute(("id", "1"));
    c_nv_pr.push_attribute(("name", "Picture 1"));
    w.write_event(Event::Empty(c_nv_pr)).map_err(pkg)?;
    w.write_event(Event::Empty(start("pic:cNvPicPr")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("pic:nvPicPr")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("pic:blipFill")))
        .map_err(pkg)?;
    let mut blip = start("a:blip");
    blip.push_attribute(("r:embed", embed));
    w.write_event(Event::Empty(blip)).map_err(pkg)?;
    write_src_rect(w, crop)?;
    w.write_event(Event::Start(start("a:stretch")))
        .map_err(pkg)?;
    w.write_event(Event::Empty(start("a:fillRect")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("a:stretch")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("pic:blipFill")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("pic:spPr")))
        .map_err(pkg)?;
    write_shape_xfrm(w, offset, extent, xfrm.rotation, xfrm.flip_h, xfrm.flip_v)?;
    write_prst_geom(w, "rect")?;
    // A framed grouped picture keeps its `a:ln` outline (only when present).
    if border.is_some() {
        write_outline(w, border)?;
    }
    w.write_event(Event::End(BytesEnd::new("pic:spPr")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("pic:pic")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a group child shape (`wps:wsp`) with geometry, fill, and outline.
fn write_group_shape(
    w: &mut Writer<Cursor<Vec<u8>>>,
    shape: &GroupShape,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("wps:wsp"))).map_err(pkg)?;
    let mut c_nv_pr = start("wps:cNvPr");
    c_nv_pr.push_attribute(("id", "0"));
    c_nv_pr.push_attribute(("name", "Shape"));
    w.write_event(Event::Empty(c_nv_pr)).map_err(pkg)?;
    w.write_event(Event::Empty(start("wps:cNvSpPr")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("wps:spPr")))
        .map_err(pkg)?;
    write_shape_xfrm(
        w,
        shape.offset,
        shape.extent,
        shape.rotation,
        shape.flip_h,
        shape.flip_v,
    )?;
    let preset = shape
        .preset
        .as_deref()
        .unwrap_or_else(|| geometry_prst(shape.geometry));
    write_prst_geom_with_adjustments(w, preset, &shape.adjustments)?;
    if let Some(fill) = &shape.fill {
        write_fill(w, fill)?;
    }
    write_outline(w, shape.stroke)?;
    w.write_event(Event::End(BytesEnd::new("wps:spPr")))
        .map_err(pkg)?;
    w.write_event(Event::Empty(start("wps:bodyPr")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("wps:wsp")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a group child text box (`wps:wsp` with a `wps:txbx`).
fn write_group_text_box(
    w: &mut Writer<Cursor<Vec<u8>>>,
    text_box: &GroupTextBox,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("wps:wsp"))).map_err(pkg)?;
    let mut c_nv_pr = start("wps:cNvPr");
    c_nv_pr.push_attribute(("id", "0"));
    c_nv_pr.push_attribute(("name", "Text Box"));
    w.write_event(Event::Empty(c_nv_pr)).map_err(pkg)?;
    w.write_event(Event::Empty(start("wps:cNvSpPr")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("wps:spPr")))
        .map_err(pkg)?;
    write_shape_xfrm(
        w,
        text_box.offset,
        text_box.extent,
        text_box.rotation,
        text_box.flip_h,
        text_box.flip_v,
    )?;
    write_prst_geom(w, "rect")?;
    if let Some(fill) = &text_box.fill {
        write_fill(w, fill)?;
    } else {
        w.write_event(Event::Empty(start("a:noFill")))
            .map_err(pkg)?;
    }
    write_outline(w, text_box.border)?;
    w.write_event(Event::End(BytesEnd::new("wps:spPr")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("wps:txbx")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("w:txbxContent")))
        .map_err(pkg)?;
    for block in &text_box.blocks {
        write_block(w, block, ctx)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:txbxContent")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("wps:txbx")))
        .map_err(pkg)?;
    write_text_box_body_properties(w, &text_box.body_properties)?;
    w.write_event(Event::End(BytesEnd::new("wps:wsp")))
        .map_err(pkg)?;
    Ok(())
}

/// The rotation + flip orientation of an `a:xfrm` (`@rot`/`@flipH`/`@flipV`),
/// carried through the picture/drawing writers so their argument lists stay small.
#[derive(Clone, Copy, Default)]
struct Xfrm2D {
    rotation: Option<i32>,
    flip_h: bool,
    flip_v: bool,
}

/// Builds an `a:xfrm` start element carrying the optional `@rot` rotation
/// (60000ths of a degree) and `@flipH`/`@flipV` mirror flags, in schema order
/// (`rot`, then `flipH`, then `flipV`). An identity transform adds no attributes.
fn xfrm_start(rotation: Option<i32>, flip_h: bool, flip_v: bool) -> BytesStart<'static> {
    let mut el = start("a:xfrm");
    if let Some(rotation) = rotation {
        el.push_attribute(("rot", rotation.to_string().as_str()));
    }
    if flip_h {
        el.push_attribute(("flipH", "1"));
    }
    if flip_v {
        el.push_attribute(("flipV", "1"));
    }
    el
}

/// Emits an `a:xfrm` for a shape/picture (off + ext), carrying its rotation and
/// flip flags on the `a:xfrm` element.
fn write_shape_xfrm(
    w: &mut Writer<Cursor<Vec<u8>>>,
    offset: PointEmu,
    extent: Extent,
    rotation: Option<i32>,
    flip_h: bool,
    flip_v: bool,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(xfrm_start(rotation, flip_h, flip_v)))
        .map_err(pkg)?;
    write_point(w, "a:off", offset)?;
    write_ext(w, "a:ext", extent)?;
    w.write_event(Event::End(BytesEnd::new("a:xfrm")))
        .map_err(pkg)?;
    Ok(())
}

fn write_prst_geom(w: &mut Writer<Cursor<Vec<u8>>>, prst: &str) -> Result<(), ExportError> {
    write_prst_geom_with_adjustments(w, prst, &[])
}

fn write_prst_geom_with_adjustments(
    w: &mut Writer<Cursor<Vec<u8>>>,
    prst: &str,
    adjustments: &[ShapeAdjustment],
) -> Result<(), ExportError> {
    let mut geom = start("a:prstGeom");
    geom.push_attribute(("prst", prst));
    w.write_event(Event::Start(geom)).map_err(pkg)?;
    if adjustments.is_empty() {
        w.write_event(Event::Empty(start("a:avLst"))).map_err(pkg)?;
    } else {
        w.write_event(Event::Start(start("a:avLst"))).map_err(pkg)?;
        for adjustment in adjustments {
            let mut guide = start("a:gd");
            guide.push_attribute(("name", adjustment.name.as_str()));
            guide.push_attribute(("fmla", adjustment.formula.as_str()));
            w.write_event(Event::Empty(guide)).map_err(pkg)?;
        }
        w.write_event(Event::End(BytesEnd::new("a:avLst")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("a:prstGeom")))
        .map_err(pkg)?;
    Ok(())
}

fn geometry_prst(geometry: ShapeGeometry) -> &'static str {
    match geometry {
        ShapeGeometry::Rectangle => "rect",
        ShapeGeometry::RoundRectangle => "roundRect",
        ShapeGeometry::Ellipse => "ellipse",
        ShapeGeometry::Triangle => "triangle",
        ShapeGeometry::RightTriangle => "rtTriangle",
        ShapeGeometry::Diamond => "diamond",
        ShapeGeometry::Line => "line",
        ShapeGeometry::Other => "rect",
    }
}

/// Emits an `a:srgbClr` element for a resolved color, carrying an `a:alpha` child
/// when the color is not fully opaque.
fn write_srgb_color(w: &mut Writer<Cursor<Vec<u8>>>, color: Rgba) -> Result<(), ExportError> {
    let mut srgb = start("a:srgbClr");
    srgb.push_attribute(("val", hex_rgb(color).as_str()));
    if color.a == u8::MAX {
        w.write_event(Event::Empty(srgb)).map_err(pkg)?;
    } else {
        w.write_event(Event::Start(srgb)).map_err(pkg)?;
        let mut alpha = start("a:alpha");
        let alpha_value = (u32::from(color.a) * 100_000 + 127) / 255;
        alpha.push_attribute(("val", alpha_value.to_string().as_str()));
        w.write_event(Event::Empty(alpha)).map_err(pkg)?;
        w.write_event(Event::End(BytesEnd::new("a:srgbClr")))
            .map_err(pkg)?;
    }
    Ok(())
}

/// Emits an `a:solidFill > a:srgbClr` for a resolved color.
fn write_solid_fill(w: &mut Writer<Cursor<Vec<u8>>>, color: Rgba) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("a:solidFill")))
        .map_err(pkg)?;
    write_srgb_color(w, color)?;
    w.write_event(Event::End(BytesEnd::new("a:solidFill")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a shape/text-box background fill: a flat `a:solidFill` or a multi-stop
/// `a:gradFill`.
fn write_fill(w: &mut Writer<Cursor<Vec<u8>>>, fill: &Fill) -> Result<(), ExportError> {
    match fill {
        Fill::Solid(color) => write_solid_fill(w, *color),
        Fill::Gradient { stops, kind } => write_grad_fill(w, stops, *kind),
    }
}

/// Emits an `a:gradFill` with its stops (`a:gsLst/a:gs`) and geometry (`a:lin` or
/// `a:path`) in `CT_GradientFillProperties` schema order.
fn write_grad_fill(
    w: &mut Writer<Cursor<Vec<u8>>>,
    stops: &[GradientStop],
    kind: GradientKind,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("a:gradFill")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("a:gsLst"))).map_err(pkg)?;
    for stop in stops {
        let mut gs = start("a:gs");
        gs.push_attribute(("pos", stop.position.to_string().as_str()));
        w.write_event(Event::Start(gs)).map_err(pkg)?;
        write_srgb_color(w, stop.color)?;
        w.write_event(Event::End(BytesEnd::new("a:gs")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("a:gsLst")))
        .map_err(pkg)?;
    match kind {
        GradientKind::Linear { angle } => {
            let mut lin = start("a:lin");
            lin.push_attribute(("ang", angle.to_string().as_str()));
            w.write_event(Event::Empty(lin)).map_err(pkg)?;
        }
        GradientKind::Radial => {
            let mut path = start("a:path");
            path.push_attribute(("path", "circle"));
            w.write_event(Event::Empty(path)).map_err(pkg)?;
        }
    }
    w.write_event(Event::End(BytesEnd::new("a:gradFill")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits an `a:ln` outline (width + solid fill, plus any dash/line-end
/// decorations in schema order), or `a:ln > a:noFill` when absent.
fn write_outline(
    w: &mut Writer<Cursor<Vec<u8>>>,
    stroke: Option<ShapeStroke>,
) -> Result<(), ExportError> {
    match stroke {
        Some(stroke) => {
            let mut ln = start("a:ln");
            ln.push_attribute(("w", stroke.width_emu.to_string().as_str()));
            w.write_event(Event::Start(ln)).map_err(pkg)?;
            // Schema order (`CT_LineProperties`): fill, then prstDash, then the
            // head/tail line-end decorations.
            write_solid_fill(w, stroke.color)?;
            if let Some(dash) = stroke.dash {
                let mut prst = start("a:prstDash");
                prst.push_attribute(("val", dash_style_token(dash)));
                w.write_event(Event::Empty(prst)).map_err(pkg)?;
            }
            write_line_end(w, "a:headEnd", stroke.head_end)?;
            write_line_end(w, "a:tailEnd", stroke.tail_end)?;
            w.write_event(Event::End(BytesEnd::new("a:ln")))
                .map_err(pkg)?;
        }
        None => {
            w.write_event(Event::Start(start("a:ln"))).map_err(pkg)?;
            w.write_event(Event::Empty(start("a:noFill")))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("a:ln")))
                .map_err(pkg)?;
        }
    }
    Ok(())
}

/// Emits an `a:headEnd`/`a:tailEnd` line-end decoration (`@type` plus any
/// `@w`/`@len` size tokens) when present.
fn write_line_end(
    w: &mut Writer<Cursor<Vec<u8>>>,
    tag: &str,
    line_end: Option<LineEnd>,
) -> Result<(), ExportError> {
    let Some(line_end) = line_end else {
        return Ok(());
    };
    let mut el = start(tag);
    el.push_attribute(("type", line_end_kind_token(line_end.kind)));
    if let Some(width) = line_end.width {
        el.push_attribute(("w", line_end_size_token(width)));
    }
    if let Some(length) = line_end.length {
        el.push_attribute(("len", line_end_size_token(length)));
    }
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
}

/// Maps a [`DashStyle`] to its `a:prstDash@val` (`ST_PresetLineDashVal`) token.
fn dash_style_token(dash: DashStyle) -> &'static str {
    match dash {
        DashStyle::Solid => "solid",
        DashStyle::Dot => "dot",
        DashStyle::Dash => "dash",
        DashStyle::LargeDash => "lgDash",
        DashStyle::DashDot => "dashDot",
        DashStyle::LargeDashDot => "lgDashDot",
        DashStyle::LargeDashDotDot => "lgDashDotDot",
        DashStyle::SystemDash => "sysDash",
        DashStyle::SystemDot => "sysDot",
        DashStyle::SystemDashDot => "sysDashDot",
        DashStyle::SystemDashDotDot => "sysDashDotDot",
    }
}

/// Maps a [`LineEndKind`] to its `@type` (`ST_LineEndType`) token.
fn line_end_kind_token(kind: LineEndKind) -> &'static str {
    match kind {
        LineEndKind::None => "none",
        LineEndKind::Triangle => "triangle",
        LineEndKind::Stealth => "stealth",
        LineEndKind::Diamond => "diamond",
        LineEndKind::Oval => "oval",
        LineEndKind::Arrow => "arrow",
    }
}

/// Maps a [`LineEndSize`] to its `@w`/`@len` (`ST_LineEndWidth`/`ST_LineEndLength`)
/// token.
fn line_end_size_token(size: LineEndSize) -> &'static str {
    match size {
        LineEndSize::Small => "sm",
        LineEndSize::Medium => "med",
        LineEndSize::Large => "lg",
    }
}

fn hex_rgb(color: Rgba) -> String {
    format!("{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

/// Closes the `wp:anchor`/`w:drawing`/`w:r` wrapping a group.
fn close_group_drawing(w: &mut Writer<Cursor<Vec<u8>>>) -> Result<(), ExportError> {
    w.write_event(Event::End(BytesEnd::new("wp:anchor")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:drawing")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:r")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits `wp:positionH` (reference edge + `wp:posOffset`/`wp:align`).
fn write_position_h(
    w: &mut Writer<Cursor<Vec<u8>>>,
    horizontal: &AnchorHorizontal,
) -> Result<(), ExportError> {
    let mut el = start("wp:positionH");
    el.push_attribute((
        "relativeFrom",
        horizontal_anchor_str(horizontal.relative_from),
    ));
    w.write_event(Event::Start(el)).map_err(pkg)?;
    match horizontal.position {
        HorizontalPosition::Offset(emu) => write_text_child(w, "wp:posOffset", &emu.to_string())?,
        HorizontalPosition::Align(align) => {
            write_text_child(w, "wp:align", horizontal_align_str(align))?;
        }
    }
    w.write_event(Event::End(BytesEnd::new("wp:positionH")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits `wp:positionV` (reference edge + `wp:posOffset`/`wp:align`).
fn write_position_v(
    w: &mut Writer<Cursor<Vec<u8>>>,
    vertical: &AnchorVertical,
) -> Result<(), ExportError> {
    let mut el = start("wp:positionV");
    el.push_attribute(("relativeFrom", vertical_anchor_str(vertical.relative_from)));
    w.write_event(Event::Start(el)).map_err(pkg)?;
    match vertical.position {
        VerticalPosition::Offset(emu) => write_text_child(w, "wp:posOffset", &emu.to_string())?,
        VerticalPosition::Align(align) => {
            write_text_child(w, "wp:align", vertical_align_str(align))?;
        }
    }
    w.write_event(Event::End(BytesEnd::new("wp:positionV")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a simple `<tag>text</tag>` element.
fn write_text_child(
    w: &mut Writer<Cursor<Vec<u8>>>,
    tag: &str,
    text: &str,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start(tag))).map_err(pkg)?;
    w.write_event(Event::Text(BytesText::new(text)))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new(tag))).map_err(pkg)?;
    Ok(())
}

/// Emits the wrap element for an anchor. `wrapNone` is an empty element; the
/// others carry a `wrapText="bothSides"` (the round-trip only reads the element
/// name, so the attribute is fixed scaffold).
fn write_wrap(
    w: &mut Writer<Cursor<Vec<u8>>>,
    wrap: WrapMode,
    polygon: Option<&[PointEmu]>,
) -> Result<(), ExportError> {
    match wrap {
        WrapMode::None => {
            w.write_event(Event::Empty(start("wp:wrapNone")))
                .map_err(pkg)?;
        }
        WrapMode::Square | WrapMode::Tight | WrapMode::Through => {
            let tag = match wrap {
                WrapMode::Square => "wp:wrapSquare",
                WrapMode::Tight => "wp:wrapTight",
                _ => "wp:wrapThrough",
            };
            let mut el = start(tag);
            el.push_attribute(("wrapText", "bothSides"));
            // A tight/through wrap can carry a `wp:wrapPolygon` contour; emit it
            // (in schema order, inside the wrap element) when the anchor has one.
            match polygon {
                Some(points) if matches!(wrap, WrapMode::Tight | WrapMode::Through) => {
                    w.write_event(Event::Start(el)).map_err(pkg)?;
                    write_wrap_polygon(w, points)?;
                    w.write_event(Event::End(BytesEnd::new(tag))).map_err(pkg)?;
                }
                _ => {
                    w.write_event(Event::Empty(el)).map_err(pkg)?;
                }
            }
        }
        WrapMode::TopAndBottom => {
            w.write_event(Event::Empty(start("wp:wrapTopAndBottom")))
                .map_err(pkg)?;
        }
    }
    Ok(())
}

/// Emits a `wp:wrapPolygon` contour: its ordered vertices as `wp:start` (the
/// first point) then `wp:lineTo` (the rest), each an EMU `@x`/`@y` `CT_Point2D`.
fn write_wrap_polygon(
    w: &mut Writer<Cursor<Vec<u8>>>,
    points: &[PointEmu],
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("wp:wrapPolygon")))
        .map_err(pkg)?;
    for (index, point) in points.iter().enumerate() {
        let tag = if index == 0 { "wp:start" } else { "wp:lineTo" };
        let mut el = start(tag);
        el.push_attribute(("x", point.x_emu.to_string().as_str()));
        el.push_attribute(("y", point.y_emu.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("wp:wrapPolygon")))
        .map_err(pkg)?;
    Ok(())
}

/// The `wp:positionH@relativeFrom` string for a horizontal reference.
fn horizontal_anchor_str(anchor: HorizontalAnchor) -> &'static str {
    match anchor {
        HorizontalAnchor::Page => "page",
        HorizontalAnchor::Margin => "margin",
        HorizontalAnchor::Column => "column",
        HorizontalAnchor::Character => "character",
        HorizontalAnchor::LeftMargin => "leftMargin",
        HorizontalAnchor::RightMargin => "rightMargin",
        HorizontalAnchor::InsideMargin => "insideMargin",
        HorizontalAnchor::OutsideMargin => "outsideMargin",
    }
}

/// The `wp:positionV@relativeFrom` string for a vertical reference.
fn vertical_anchor_str(anchor: VerticalAnchor) -> &'static str {
    match anchor {
        VerticalAnchor::Page => "page",
        VerticalAnchor::Margin => "margin",
        VerticalAnchor::Paragraph => "paragraph",
        VerticalAnchor::Line => "line",
        VerticalAnchor::TopMargin => "topMargin",
        VerticalAnchor::BottomMargin => "bottomMargin",
        VerticalAnchor::InsideMargin => "insideMargin",
        VerticalAnchor::OutsideMargin => "outsideMargin",
    }
}

/// The horizontal `wp:align` keyword.
fn horizontal_align_str(align: HorizontalAlign) -> &'static str {
    match align {
        HorizontalAlign::Left => "left",
        HorizontalAlign::Center => "center",
        HorizontalAlign::Right => "right",
        HorizontalAlign::Inside => "inside",
        HorizontalAlign::Outside => "outside",
    }
}

/// The vertical `wp:align` keyword.
fn vertical_align_str(align: VerticalAlign) -> &'static str {
    match align {
        VerticalAlign::Top => "top",
        VerticalAlign::Center => "center",
        VerticalAlign::Bottom => "bottom",
        VerticalAlign::Inside => "inside",
        VerticalAlign::Outside => "outside",
    }
}

/// Emits an inline embedded object. A chart/diagram is a `w:drawing`/`wp:inline`
/// whose `a:graphicData` carries a `c:chart`/`dgm:relIds` reference; an OLE object
/// is a `w:object` with the (optional) preview shape and the `o:OLEObject`. Every
/// `r:id` is the part's verbatim relationship id (emitted in `document.xml.rels`).
fn write_embedded_object(
    w: &mut Writer<Cursor<Vec<u8>>>,
    object: &EmbeddedObject,
    ctx: &Ctx,
) -> Result<(), ExportError> {
    match &object.kind {
        EmbeddedKind::Chart => write_graphic_object(w, &object.extent, CHART_URI, |w| {
            let mut chart = start("c:chart");
            chart.push_attribute(("r:id", object.part.relationship_id.as_str()));
            w.write_event(Event::Empty(chart)).map_err(pkg)
        }),
        EmbeddedKind::Diagram => write_graphic_object(w, &object.extent, DIAGRAM_URI, |w| {
            let mut rel_ids = start("dgm:relIds");
            for part in std::iter::once(&object.part).chain(object.extra_parts.iter()) {
                if let Some(attr) = diagram_rel_attr(&part.relationship_type) {
                    rel_ids.push_attribute((attr, part.relationship_id.as_str()));
                }
            }
            w.write_event(Event::Empty(rel_ids)).map_err(pkg)
        }),
        EmbeddedKind::OleObject => write_ole_object(w, object, ctx),
        // An unrecognized `a:graphicData` payload: emit the wrapper with its uri.
        // The referencing relationship is still emitted so the part stays
        // reachable (the importer does not produce this variant).
        EmbeddedKind::Other(uri) => write_graphic_object(w, &object.extent, uri, |_| Ok(())),
    }
}

/// The `dgm:relIds` attribute a diagram part maps to, by its relationship type.
fn diagram_rel_attr(relationship_type: &str) -> Option<&'static str> {
    match relationship_type.rsplit('/').next() {
        Some("diagramData") => Some("r:dm"),
        Some("diagramLayout") => Some("r:lo"),
        Some("diagramQuickStyle") => Some("r:qs"),
        Some("diagramColors") => Some("r:cs"),
        _ => None,
    }
}

/// Emits the `w:drawing`/`wp:inline`/`a:graphic`/`a:graphicData` frame around a
/// caller-written graphic reference (`body` writes the `c:chart`/`dgm:relIds`).
fn write_graphic_object(
    w: &mut Writer<Cursor<Vec<u8>>>,
    extent: &Extent,
    uri: &str,
    body: impl FnOnce(&mut Writer<Cursor<Vec<u8>>>) -> Result<(), ExportError>,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
    w.write_event(Event::Start(start("w:drawing")))
        .map_err(pkg)?;
    let mut inline = start("wp:inline");
    for name in ["distT", "distB", "distL", "distR"] {
        inline.push_attribute((name, "0"));
    }
    w.write_event(Event::Start(inline)).map_err(pkg)?;
    let mut ext = start("wp:extent");
    ext.push_attribute(("cx", extent.width_emu.to_string().as_str()));
    ext.push_attribute(("cy", extent.height_emu.to_string().as_str()));
    w.write_event(Event::Empty(ext)).map_err(pkg)?;
    let mut doc_pr = start("wp:docPr");
    doc_pr.push_attribute(("id", "1"));
    doc_pr.push_attribute(("name", "Object 1"));
    w.write_event(Event::Empty(doc_pr)).map_err(pkg)?;
    w.write_event(Event::Start(start("a:graphic")))
        .map_err(pkg)?;
    let mut graphic_data = start("a:graphicData");
    graphic_data.push_attribute(("uri", uri));
    w.write_event(Event::Start(graphic_data)).map_err(pkg)?;
    body(w)?;
    w.write_event(Event::End(BytesEnd::new("a:graphicData")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("a:graphic")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("wp:inline")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:drawing")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:r")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits an OLE object as a legacy `w:object`: the natural size (`w:dxaOrig`/
/// `w:dyaOrig`, EMU → twips), an optional `v:shape/v:imagedata` preview, and the
/// `o:OLEObject` naming the embedding part and its `ProgID`.
fn write_ole_object(
    w: &mut Writer<Cursor<Vec<u8>>>,
    object: &EmbeddedObject,
    ctx: &Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
    let mut el = start("w:object");
    let dxa = (object.extent.width_emu / 635).to_string();
    let dya = (object.extent.height_emu / 635).to_string();
    el.push_attribute(("w:dxaOrig", dxa.as_str()));
    el.push_attribute(("w:dyaOrig", dya.as_str()));
    w.write_event(Event::Start(el)).map_err(pkg)?;
    // The preview shape, when a preview image resolves in the media table.
    let preview_rel = object
        .preview
        .and_then(|id| ctx.defs.media.get(&id))
        .map(|reference| reference.relationship_id.clone());
    if let Some(rel_id) = &preview_rel {
        let mut shape = start("v:shape");
        shape.push_attribute(("id", "_x0000_i1025"));
        shape.push_attribute(("type", "#_x0000_t75"));
        let style = format!(
            "width:{:.2}pt;height:{:.2}pt",
            object.extent.width_emu as f64 / 12_700.0,
            object.extent.height_emu as f64 / 12_700.0,
        );
        shape.push_attribute(("style", style.as_str()));
        w.write_event(Event::Start(shape)).map_err(pkg)?;
        let mut imagedata = start("v:imagedata");
        imagedata.push_attribute(("r:id", rel_id.as_str()));
        imagedata.push_attribute(("o:title", ""));
        w.write_event(Event::Empty(imagedata)).map_err(pkg)?;
        w.write_event(Event::End(BytesEnd::new("v:shape")))
            .map_err(pkg)?;
    }
    let mut ole = start("o:OLEObject");
    ole.push_attribute(("Type", "Embed"));
    if let Some(prog_id) = &object.prog_id {
        ole.push_attribute(("ProgID", prog_id.as_str()));
    }
    ole.push_attribute(("ShapeID", "_x0000_i1025"));
    ole.push_attribute(("DrawAspect", "Content"));
    ole.push_attribute(("r:id", object.part.relationship_id.as_str()));
    w.write_event(Event::Empty(ole)).map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:object")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:r")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a DrawingML text box, preserving its inline/floating frame, extent,
/// anchor, z-order key, fill, outline, and block content.
fn write_text_box(
    w: &mut Writer<Cursor<Vec<u8>>>,
    text_box: &TextBox,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
    w.write_event(Event::Start(start("w:drawing")))
        .map_err(pkg)?;

    let frame_tag = if let Some(anchor) = &text_box.anchor {
        let mut frame = start("wp:anchor");
        let dist_t = anchor.wrap_distances.top_emu.to_string();
        let dist_b = anchor.wrap_distances.bottom_emu.to_string();
        let dist_l = anchor.wrap_distances.start_emu.to_string();
        let dist_r = anchor.wrap_distances.end_emu.to_string();
        frame.push_attribute(("distT", dist_t.as_str()));
        frame.push_attribute(("distB", dist_b.as_str()));
        frame.push_attribute(("distL", dist_l.as_str()));
        frame.push_attribute(("distR", dist_r.as_str()));
        frame.push_attribute(("simplePos", "0"));
        if let Some(relative_height) = text_box.relative_height {
            frame.push_attribute(("relativeHeight", relative_height.to_string().as_str()));
        }
        frame.push_attribute(("behindDoc", if anchor.behind_doc { "1" } else { "0" }));
        frame.push_attribute(("locked", "0"));
        frame.push_attribute(("layoutInCell", "1"));
        frame.push_attribute(("allowOverlap", "1"));
        w.write_event(Event::Start(frame)).map_err(pkg)?;

        let mut simple_pos = start("wp:simplePos");
        simple_pos.push_attribute(("x", "0"));
        simple_pos.push_attribute(("y", "0"));
        w.write_event(Event::Empty(simple_pos)).map_err(pkg)?;
        write_position_h(w, &anchor.horizontal)?;
        write_position_v(w, &anchor.vertical)?;
        write_text_box_extent(
            w,
            text_box.extent.unwrap_or(Extent {
                width_emu: 0,
                height_emu: 0,
            }),
        )?;
        write_wrap(w, anchor.wrap, anchor.wrap_polygon.as_deref())?;
        "wp:anchor"
    } else {
        let mut frame = start("wp:inline");
        for name in ["distT", "distB", "distL", "distR"] {
            frame.push_attribute((name, "0"));
        }
        w.write_event(Event::Start(frame)).map_err(pkg)?;
        if let Some(extent) = text_box.extent {
            write_text_box_extent(w, extent)?;
        }
        "wp:inline"
    };

    let mut doc_pr = start("wp:docPr");
    doc_pr.push_attribute(("id", "1"));
    doc_pr.push_attribute(("name", "Text Box 1"));
    w.write_event(Event::Empty(doc_pr)).map_err(pkg)?;
    w.write_event(Event::Start(start("a:graphic")))
        .map_err(pkg)?;
    let mut graphic_data = start("a:graphicData");
    graphic_data.push_attribute(("uri", WPS_NS));
    w.write_event(Event::Start(graphic_data)).map_err(pkg)?;
    w.write_event(Event::Start(start("wps:wsp"))).map_err(pkg)?;
    let mut c_nv_pr = start("wps:cNvPr");
    c_nv_pr.push_attribute(("id", "0"));
    c_nv_pr.push_attribute(("name", "Text Box"));
    w.write_event(Event::Empty(c_nv_pr)).map_err(pkg)?;
    w.write_event(Event::Empty(start("wps:cNvSpPr")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("wps:spPr")))
        .map_err(pkg)?;
    write_shape_xfrm(
        w,
        PointEmu { x_emu: 0, y_emu: 0 },
        text_box.extent.unwrap_or(Extent {
            width_emu: 0,
            height_emu: 0,
        }),
        None,
        false,
        false,
    )?;
    write_prst_geom(w, "rect")?;
    if let Some(fill) = &text_box.fill {
        write_fill(w, fill)?;
    }
    write_outline(w, text_box.border)?;
    w.write_event(Event::End(BytesEnd::new("wps:spPr")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("wps:txbx")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("w:txbxContent")))
        .map_err(pkg)?;
    for block in &text_box.blocks {
        write_block(w, block, ctx)?;
    }
    for tag in ["w:txbxContent", "wps:txbx"] {
        w.write_event(Event::End(BytesEnd::new(tag))).map_err(pkg)?;
    }
    write_text_box_body_properties(w, &text_box.body_properties)?;
    for tag in ["wps:wsp", "a:graphicData", "a:graphic"] {
        w.write_event(Event::End(BytesEnd::new(tag))).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new(frame_tag)))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:drawing")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:r")))
        .map_err(pkg)?;
    Ok(())
}

fn write_text_box_extent(
    w: &mut Writer<Cursor<Vec<u8>>>,
    extent: Extent,
) -> Result<(), ExportError> {
    let mut el = start("wp:extent");
    el.push_attribute(("cx", extent.width_emu.to_string().as_str()));
    el.push_attribute(("cy", extent.height_emu.to_string().as_str()));
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
}

/// Emits the supported `wps:bodyPr` attributes and its mutually-exclusive
/// DrawingML autofit child. Schema-default attributes stay omitted.
fn write_text_box_body_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &TextBoxBodyProperties,
) -> Result<(), ExportError> {
    let mut body = start("wps:bodyPr");
    let insets = properties.insets;
    let left = insets.left_emu.to_string();
    let top = insets.top_emu.to_string();
    let right = insets.right_emu.to_string();
    let bottom = insets.bottom_emu.to_string();
    if insets.left_emu != casual_doc_model::v1::TextBoxInsets::DEFAULT_HORIZONTAL_EMU {
        body.push_attribute(("lIns", left.as_str()));
    }
    if insets.top_emu != casual_doc_model::v1::TextBoxInsets::DEFAULT_VERTICAL_EMU {
        body.push_attribute(("tIns", top.as_str()));
    }
    if insets.right_emu != casual_doc_model::v1::TextBoxInsets::DEFAULT_HORIZONTAL_EMU {
        body.push_attribute(("rIns", right.as_str()));
    }
    if insets.bottom_emu != casual_doc_model::v1::TextBoxInsets::DEFAULT_VERTICAL_EMU {
        body.push_attribute(("bIns", bottom.as_str()));
    }
    match properties.vertical_anchor {
        TextBoxVerticalAnchor::Top => {}
        TextBoxVerticalAnchor::Center => body.push_attribute(("anchor", "ctr")),
        TextBoxVerticalAnchor::Bottom => body.push_attribute(("anchor", "b")),
    }
    if properties.horizontal_overflow == TextBoxHorizontalOverflow::Clip {
        body.push_attribute(("horzOverflow", "clip"));
    }
    match properties.vertical_overflow {
        TextBoxVerticalOverflow::Overflow => {}
        TextBoxVerticalOverflow::Clip => body.push_attribute(("vertOverflow", "clip")),
        TextBoxVerticalOverflow::Ellipsis => body.push_attribute(("vertOverflow", "ellipsis")),
    }
    match properties.auto_fit {
        TextBoxAutoFit::None => {
            w.write_event(Event::Empty(body)).map_err(pkg)?;
        }
        TextBoxAutoFit::Shape => {
            w.write_event(Event::Start(body)).map_err(pkg)?;
            w.write_event(Event::Empty(start("a:spAutoFit")))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("wps:bodyPr")))
                .map_err(pkg)?;
        }
        TextBoxAutoFit::Normal {
            font_scale,
            line_spacing_reduction,
        } => {
            w.write_event(Event::Start(body)).map_err(pkg)?;
            let mut normal = start("a:normAutofit");
            let font_scale_text = font_scale.to_string();
            let line_reduction_text = line_spacing_reduction.to_string();
            if font_scale != 100_000 {
                normal.push_attribute(("fontScale", font_scale_text.as_str()));
            }
            if line_spacing_reduction != 0 {
                normal.push_attribute(("lnSpcReduction", line_reduction_text.as_str()));
            }
            w.write_event(Event::Empty(normal)).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("wps:bodyPr")))
                .map_err(pkg)?;
        }
    }
    Ok(())
}

/// Emits `w:sdtPr` for a content control, or nothing when the control carries no
/// modeled properties. Children follow the order Word writes (alias/tag/id, then
/// lock, placeholder, temporary, showingPlcHdr, dataBinding, then the type marker
/// with its control-specific detail); the importer is order-independent.
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
            write_val_element(w, name, value)?;
        }
    }
    if let Some(lock) = properties.lock {
        write_val_element(w, "w:lock", sdt_lock_token(lock))?;
    }
    if let Some(placeholder) = &properties.placeholder {
        w.write_event(Event::Start(start("w:placeholder")))
            .map_err(pkg)?;
        write_val_element(w, "w:docPart", placeholder)?;
        w.write_event(Event::End(BytesEnd::new("w:placeholder")))
            .map_err(pkg)?;
    }
    if properties.temporary {
        w.write_event(Event::Empty(start("w:temporary")))
            .map_err(pkg)?;
    }
    if properties.showing_placeholder {
        w.write_event(Event::Empty(start("w:showingPlcHdr")))
            .map_err(pkg)?;
    }
    if let Some(binding) = &properties.data_binding {
        let mut el = start("w:dataBinding");
        el.push_attribute(("w:xpath", binding.xpath.as_str()));
        if let Some(store_item_id) = &binding.store_item_id {
            el.push_attribute(("w:storeItemID", store_item_id.as_str()));
        }
        if let Some(prefix_mappings) = &binding.prefix_mappings {
            el.push_attribute(("w:prefixMappings", prefix_mappings.as_str()));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    write_sdt_control(w, properties)?;
    w.write_event(Event::End(BytesEnd::new("w:sdtPr")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits the `w:sdtPr` type-marker element for the control kind, carrying its
/// control-specific detail (list entries, date detail, checkbox detail) when the
/// kind supports it. Nothing is written when the control has no kind.
fn write_sdt_control(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &SdtProperties,
) -> Result<(), ExportError> {
    let Some(kind) = properties.control_kind else {
        return Ok(());
    };
    match kind {
        SdtControlKind::ComboBox | SdtControlKind::DropDownList => {
            let element = sdt_kind_element(kind);
            let items = match &properties.data {
                Some(SdtControlData::List(items)) => items.as_slice(),
                _ => &[][..],
            };
            write_sdt_list(w, element, items)?;
        }
        SdtControlKind::Date => {
            let date = match &properties.data {
                Some(SdtControlData::Date(date)) => Some(date),
                _ => None,
            };
            write_sdt_date(w, date)?;
        }
        SdtControlKind::Checkbox => {
            let checkbox = match &properties.data {
                Some(SdtControlData::Checkbox(checkbox)) => Some(checkbox),
                _ => None,
            };
            write_sdt_checkbox(w, checkbox)?;
        }
        SdtControlKind::BuildingBlockGallery => {
            let element = sdt_kind_element(kind);
            if properties.gallery.is_none() && properties.category.is_none() {
                w.write_event(Event::Empty(start(element))).map_err(pkg)?;
            } else {
                w.write_event(Event::Start(start(element))).map_err(pkg)?;
                if let Some(gallery) = &properties.gallery {
                    write_val_element(w, "w:docPartGallery", gallery)?;
                }
                if let Some(category) = &properties.category {
                    write_val_element(w, "w:docPartCategory", category)?;
                }
                w.write_event(Event::End(BytesEnd::new(element)))
                    .map_err(pkg)?;
            }
        }
        _ => {
            w.write_event(Event::Empty(start(sdt_kind_element(kind))))
                .map_err(pkg)?;
        }
    }
    Ok(())
}

/// Emits a combo-box / drop-down-list type marker with its `w:listItem` choice
/// entries (an empty type marker when there are none).
fn write_sdt_list(
    w: &mut Writer<Cursor<Vec<u8>>>,
    element: &str,
    items: &[SdtListItem],
) -> Result<(), ExportError> {
    if items.is_empty() {
        w.write_event(Event::Empty(start(element))).map_err(pkg)?;
        return Ok(());
    }
    w.write_event(Event::Start(start(element))).map_err(pkg)?;
    for item in items {
        let mut el = start("w:listItem");
        if let Some(display) = &item.display {
            el.push_attribute(("w:displayText", display.as_str()));
        }
        el.push_attribute(("w:value", item.value.as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new(element)))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a `w:date` type marker with its detail children (in `CT_SdtDate` order:
/// dateFormat, lid, storeMappedDataAs, calendar). An empty/absent detail writes a
/// bare `<w:date/>`.
fn write_sdt_date(
    w: &mut Writer<Cursor<Vec<u8>>>,
    date: Option<&SdtDate>,
) -> Result<(), ExportError> {
    let mut el = start("w:date");
    if let Some(full_date) = date.and_then(|date| date.full_date.as_deref()) {
        el.push_attribute(("w:fullDate", full_date));
    }
    let has_children = date.is_some_and(|date| {
        date.date_format.is_some()
            || date.lid.is_some()
            || date.store_mapped_as.is_some()
            || date.calendar.is_some()
    });
    if !has_children {
        w.write_event(Event::Empty(el)).map_err(pkg)?;
        return Ok(());
    }
    w.write_event(Event::Start(el)).map_err(pkg)?;
    let date = date.expect("has_children implies a date detail");
    if let Some(date_format) = &date.date_format {
        write_val_element(w, "w:dateFormat", date_format)?;
    }
    if let Some(lid) = &date.lid {
        write_val_element(w, "w:lid", lid)?;
    }
    if let Some(store_mapped_as) = &date.store_mapped_as {
        write_val_element(w, "w:storeMappedDataAs", store_mapped_as)?;
    }
    if let Some(calendar) = &date.calendar {
        write_val_element(w, "w:calendar", calendar)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:date")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a `w14:checkbox` type marker with its `w14:` detail children (the state
/// glyphs). An absent detail writes a bare `<w14:checkbox/>`.
fn write_sdt_checkbox(
    w: &mut Writer<Cursor<Vec<u8>>>,
    checkbox: Option<&SdtCheckbox>,
) -> Result<(), ExportError> {
    let Some(checkbox) = checkbox else {
        w.write_event(Event::Empty(start("w14:checkbox")))
            .map_err(pkg)?;
        return Ok(());
    };
    w.write_event(Event::Start(start("w14:checkbox")))
        .map_err(pkg)?;
    let mut checked = start("w14:checked");
    checked.push_attribute(("w14:val", if checkbox.checked { "1" } else { "0" }));
    w.write_event(Event::Empty(checked)).map_err(pkg)?;
    for (symbol, name) in [
        (&checkbox.checked_state, "w14:checkedState"),
        (&checkbox.unchecked_state, "w14:uncheckedState"),
    ] {
        if let Some(symbol) = symbol {
            write_sdt_checkbox_symbol(w, name, symbol)?;
        }
    }
    w.write_event(Event::End(BytesEnd::new("w14:checkbox")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a checkbox state glyph (`w14:checkedState` / `w14:uncheckedState`).
fn write_sdt_checkbox_symbol(
    w: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    symbol: &SdtCheckboxSymbol,
) -> Result<(), ExportError> {
    let mut el = start(name);
    el.push_attribute(("w14:val", symbol.val.as_str()));
    if let Some(font) = &symbol.font {
        el.push_attribute(("w14:font", font.as_str()));
    }
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
}

/// Emits an empty element carrying a single `w:val` attribute.
fn write_val_element(
    w: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    value: &str,
) -> Result<(), ExportError> {
    let mut el = start(name);
    el.push_attribute(("w:val", value));
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
}

/// Maps an edit-lock behaviour to its `w:lock@w:val` token.
fn sdt_lock_token(lock: SdtLock) -> &'static str {
    match lock {
        SdtLock::Unlocked => "unlocked",
        SdtLock::SdtLocked => "sdtLocked",
        SdtLock::ContentLocked => "contentLocked",
        SdtLock::SdtContentLocked => "sdtContentLocked",
    }
}

/// Maps a content-control kind to its `w:sdtPr` type-marker element. The
/// importer matches by local name, so the `w:`-prefixed spelling round-trips.
/// Combo/dropdown, date, and checkbox controls carry detail and are emitted by
/// their dedicated writers; this maps the marker element only.
fn sdt_kind_element(kind: SdtControlKind) -> &'static str {
    match kind {
        SdtControlKind::RichText => "w:richText",
        SdtControlKind::PlainText => "w:text",
        SdtControlKind::ComboBox => "w:comboBox",
        SdtControlKind::DropDownList => "w:dropDownList",
        SdtControlKind::Date => "w:date",
        SdtControlKind::Picture => "w:picture",
        SdtControlKind::Checkbox => "w14:checkbox",
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
    write_run_property_children(w, properties)?;
    w.write_event(Event::End(BytesEnd::new("w:rPr")))
        .map_err(pkg)?;
    Ok(())
}

/// Writes the CHILDREN of a run's `w:rPr` (every property element, in CT_RPr
/// schema order) without the enclosing `w:rPr` start/end. Shared by
/// [`write_run_properties`] and the paragraph-mark rPr writer, which must
/// prepend the mark's tracked change (`w:ins`/`w:del`) before these children.
fn write_run_property_children(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &RunProperties,
) -> Result<(), ExportError> {
    if let Some(style_ref) = properties.style_ref {
        let mut el = start("w:rStyle");
        el.push_attribute(("w:val", style_id_token(style_ref).as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    // Fonts (`w:rFonts`): the four slots (each a named family or a `*Theme`
    // reference) and the `@hint`. The cs slot emits the standard `w:cstheme`
    // spelling (the importer also accepts the legacy `w:csTheme`).
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
        push_font_slot(&mut el, "w:cs", "w:cstheme", &properties.font_ref_cs);
        if let Some(hint) = properties.font_hint {
            el.push_attribute(("w:hint", font_hint_token(hint)));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    for (value, name) in [
        // CT_RPr schema order: each Latin toggle is immediately followed by its
        // complex-script counterpart (`w:b`→`w:bCs`, `w:i`→`w:iCs`).
        (properties.bold, "w:b"),
        (properties.bold_complex, "w:bCs"),
        (properties.italic, "w:i"),
        (properties.italic_complex, "w:iCs"),
        (properties.strike, "w:strike"),
        (properties.double_strike, "w:dstrike"),
        (properties.all_caps, "w:caps"),
        (properties.small_caps, "w:smallCaps"),
        (properties.hidden, "w:vanish"),
        (properties.web_hidden, "w:webHidden"),
        (properties.no_proof, "w:noProof"),
        (properties.outline, "w:outline"),
        (properties.shadow, "w:shadow"),
        (properties.emboss, "w:emboss"),
        (properties.imprint, "w:imprint"),
        (properties.snap_to_grid, "w:snapToGrid"),
        (properties.rtl, "w:rtl"),
        (properties.spec_vanish, "w:specVanish"),
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
        } else if let Some(style) = properties.underline_style {
            // `w:u@val` line style; `single` is the default and stays implicit.
            use casual_doc_model::v1::UnderlineStyle;
            let token = match style {
                UnderlineStyle::Single => "single",
                UnderlineStyle::Double => "double",
                UnderlineStyle::Thick => "thick",
                UnderlineStyle::Dotted => "dotted",
                UnderlineStyle::Dashed => "dash",
                UnderlineStyle::DotDash => "dotDash",
                UnderlineStyle::Wavy => "wave",
                UnderlineStyle::Words => "words",
            };
            if !matches!(style, UnderlineStyle::Single) {
                el.push_attribute(("w:val", token));
            }
        }
        // `w:u@color` round-trips the explicit underline color when present.
        let color;
        if let Some(rgb) = &properties.underline_color {
            color = format!("{:02X}{:02X}{:02X}", rgb.r, rgb.g, rgb.b);
            el.push_attribute(("w:color", color.as_str()));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    match &properties.color {
        Some(Color::Rgb(rgb)) => {
            let mut el = start("w:color");
            el.push_attribute((
                "w:val",
                format!("{:02X}{:02X}{:02X}", rgb.r, rgb.g, rgb.b).as_str(),
            ));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        Some(Color::Auto) => {
            let mut el = start("w:color");
            el.push_attribute(("w:val", "auto"));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        Some(Color::Theme(theme)) => {
            let mut el = start("w:color");
            // `CT_Color` requires `@w:val`; the concrete fallback Word writes beside
            // `@w:themeColor` is not modeled, so emit `auto` and let the theme
            // reference carry the palette slot (with any tint/shade).
            el.push_attribute(("w:val", "auto"));
            el.push_attribute(("w:themeColor", theme_color_token(theme.slot)));
            let tint_str;
            let shade_str;
            if let Some(tint) = theme.theme_tint {
                tint_str = format!("{tint:02X}");
                el.push_attribute(("w:themeTint", tint_str.as_str()));
            }
            if let Some(shade) = theme.theme_shade {
                shade_str = format!("{shade:02X}");
                el.push_attribute(("w:themeShade", shade_str.as_str()));
            }
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        None => {}
    }
    if let Some(size) = properties.size_half_points {
        let mut el = start("w:sz");
        el.push_attribute(("w:val", size.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    // Complex-script size (`w:szCs`) follows `w:sz` in CT_RPr schema order.
    if let Some(size) = properties.size_complex_half_points {
        let mut el = start("w:szCs");
        el.push_attribute(("w:val", size.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(edge) = &properties.border {
        write_border_edge(w, "w:bdr", edge)?;
    }
    write_shading(w, &properties.shading)?;
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
    // Typographic metrics in CT_RPr schema order.
    if let Some(spacing) = properties.character_spacing_twips {
        let mut el = start("w:spacing");
        el.push_attribute(("w:val", spacing.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(scale) = properties.character_scale_percent {
        let mut el = start("w:w");
        el.push_attribute(("w:val", scale.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(kern) = properties.kerning_half_points {
        let mut el = start("w:kern");
        el.push_attribute(("w:val", kern.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(position) = properties.position_half_points {
        let mut el = start("w:position");
        el.push_attribute(("w:val", position.to_string().as_str()));
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
    // `w:rPrChange` is the last child of `w:rPr` (CT_RPr places it after every
    // property element); its `w:rPr` is the prior snapshot (CT_RPrOriginal — no
    // nested change). An all-default prior still emits a bare `<w:rPr/>` so the
    // required child is present.
    if let Some(change) = &properties.prop_change {
        let mut el = start("w:rPrChange");
        push_prop_change_attrs(&mut el, change);
        w.write_event(Event::Start(el)).map_err(pkg)?;
        if change.prior.as_ref() == &RunProperties::default() {
            w.write_event(Event::Empty(start("w:rPr"))).map_err(pkg)?;
        } else {
            write_run_properties(w, change.prior.as_ref())?;
        }
        w.write_event(Event::End(BytesEnd::new("w:rPrChange")))
            .map_err(pkg)?;
    }
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
        TabAlignment::Clear => "clear",
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

/// The `w:type` token for a preferred table/cell width unit (`ST_TblWidth`).
fn width_type_token(width_type: WidthType) -> &'static str {
    match width_type {
        WidthType::Dxa => "dxa",
        WidthType::Pct => "pct",
        WidthType::Auto => "auto",
        WidthType::Nil => "nil",
    }
}

/// Writes a `CT_TblWidth` element (`w:tblW`/`w:tcW`) carrying both `@w:w` and
/// `@w:type` so the typed width round-trips.
fn write_table_width<W: std::io::Write>(
    w: &mut Writer<W>,
    name: &str,
    width: TableWidth,
) -> Result<(), ExportError> {
    let mut el = start(name);
    el.push_attribute(("w:type", width_type_token(width.width_type)));
    el.push_attribute(("w:w", width.value.to_string().as_str()));
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
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
