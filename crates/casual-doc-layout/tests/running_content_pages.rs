//! **Which pages a header/footer appears on** — the page × variant decision,
//! end to end through the real driver.
//!
//! The rules under test are ECMA-376 §17.10.5 (`headerReference` /
//! `footerReference`) and §17.10.6 (`titlePg`), plus §17.10.1
//! (`evenAndOddHeaders`), as Word implements them:
//!
//! 1. **Link to previous is reference absence.** A section that omits a reference
//!    of some type inherits the previous section's for that type — transitively
//!    back through sections, and independently *per variant*. Only when no earlier
//!    section declared that type does nothing show.
//! 2. **`w:titlePg` is section-local.** The `first` variant shows on the owning
//!    section's first page, not only on document page 1, and it outranks `even`.
//! 3. **A variant that is switched off is never selected**, and — the other half
//!    of the same rule — **a variant that is switched on is selected even when it
//!    is empty**, painting a blank band rather than falling back to `default`.
//! 4. **Band geometry is per section.** Page size, margins, and
//!    `w:pgMar/@w:header`/`@w:footer` come from the section owning the page, so an
//!    inherited header is re-flowed and re-placed at the inheriting section's
//!    width — an orientation change re-breaks its lines.
//! 5. **A blank page inserted for `evenPage`/`oddPage` parity still shows running
//!    content** — the *preceding* section's, at the preceding section's geometry.
//! 6. **An unreachable variant must not reserve body area.** The reservation is
//!    per section (one content area per section is what keeps incremental page
//!    reuse valid), but it only counts variants the section can actually select.
//!    The remaining deviation from Word — a reachable-but-first-page-only tall
//!    variant costing body area on every page of its section — is asserted here as
//!    the recorded behavior, not as the correct one; see
//!    `HeaderFooter::band_height`.
//!
//! `tests/section_geometry.rs` covers the parity-pad *page count* and body
//! ownership; this file covers which band each page ends up painting.

use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::flow::flow_header_footer;
use casual_doc_layout::page::{PaginatedLayout, PlacedFragment};
use casual_doc_layout::running::HeaderFooter;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, DefinitionMap, Definitions, Document, DocumentSettings,
    HeaderFooter as ModelHeaderFooter, HeaderFooterId, HeaderFooterKind, HeaderFooterRef,
    InlineNode, PageMargins, PageSize, Paragraph, ParagraphProperties, Run, RunProperties,
    SectionBoundary, SectionColumns, SectionId, SectionType,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

fn run(id: u64, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id: node(id),
        properties: RunProperties::default(),
        text: text.to_owned(),
    })
}

fn paragraph_with(id: u64, text: &str, properties: ParagraphProperties) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties,
        inlines: vec![run(id + 1, text)],
    })
}

fn paragraph(id: u64, text: &str) -> BlockNode {
    paragraph_with(id, text, ParagraphProperties::default())
}

/// A paragraph carrying `section`'s `w:sectPr` — how a DOCX body marks the end of
/// a non-final section.
fn section_break(id: u64, text: &str, section: SectionId) -> BlockNode {
    paragraph_with(
        id,
        text,
        ParagraphProperties {
            section_break: Some(section),
            ..ParagraphProperties::default()
        },
    )
}

/// A paragraph that forces a page break before it, so page counts are exact and
/// independent of shaped line heights.
fn page_break(id: u64, text: &str) -> BlockNode {
    paragraph_with(
        id,
        text,
        ParagraphProperties {
            page_break_before: true,
            ..ParagraphProperties::default()
        },
    )
}

/// US-Letter portrait.
const PORTRAIT: (i32, i32) = (12_240, 15_840);
/// US-Letter landscape (what Word writes for `w:orient="landscape"`: the page
/// size itself is swapped).
const LANDSCAPE: (i32, i32) = (15_840, 12_240);

fn margins(all: i32) -> PageMargins {
    PageMargins {
        top_twips: all,
        bottom_twips: all,
        start_twips: all,
        end_twips: all,
        header_twips: None,
        footer_twips: None,
        gutter_twips: None,
    }
}

/// A single-column section. `id` doubles as the section's node id, so a failure
/// message names the section that owns a page.
fn section(
    id: u64,
    size: (i32, i32),
    page_margins: PageMargins,
    headers: Vec<HeaderFooterRef>,
    footers: Vec<HeaderFooterRef>,
) -> SectionBoundary {
    SectionBoundary {
        id: SectionId::new(node(id)),
        page_size: PageSize {
            width_twips: size.0,
            height_twips: size.1,
        },
        page_margins,
        columns: SectionColumns {
            count: 1,
            space_twips: None,
            separator: None,
            equal_width: None,
            columns: Vec::new(),
        },
        headers,
        footers,
        section_type: None,
        title_page: None,
        vertical_alignment: None,
        page_numbering: Default::default(),
        doc_grid: Default::default(),
        orientation: None,
        paper_source: Default::default(),
        page_borders: Default::default(),
        line_numbering: Default::default(),
        footnote_props: Default::default(),
        endnote_props: Default::default(),
        text_direction: None,
        bidi: false,
        section_change: None,
    }
}

fn href(kind: HeaderFooterKind, id: u64) -> HeaderFooterRef {
    HeaderFooterRef {
        kind,
        reference: HeaderFooterId::new(node(id)),
    }
}

/// A header/footer part of `lines` one-line paragraphs, keyed by `id`. Its first
/// paragraph's node id is `id + 1`, which is how a page's band is identified
/// below. `lines = 0` is a part with no blocks at all — a producer's way of
/// writing "this variant is deliberately empty".
fn part(id: u64, lines: usize) -> (HeaderFooterId, ModelHeaderFooter) {
    (
        HeaderFooterId::new(node(id)),
        ModelHeaderFooter {
            blocks: (0..lines)
                .map(|i| paragraph(id + 1 + (i as u64) * 2, "Running content"))
                .collect(),
        },
    )
}

fn parts(specs: &[(u64, usize)]) -> DefinitionMap<HeaderFooterId, ModelHeaderFooter> {
    let mut map = DefinitionMap::default();
    for (id, lines) in specs {
        let (key, value) = part(*id, *lines);
        map.insert(key, value);
    }
    map
}

/// The node id of the first fragment in a band — `None` for a blank band. The
/// part ids above make this identify *which* variant of *which* section a page
/// painted.
fn band(placed: &[PlacedFragment]) -> Option<NodeId> {
    placed.first().map(|placed| placed.fragment.node_id())
}

fn headers_of(layout: &PaginatedLayout) -> Vec<Option<NodeId>> {
    layout.pages.iter().map(|page| band(&page.header)).collect()
}

fn footers_of(layout: &PaginatedLayout) -> Vec<Option<NodeId>> {
    layout.pages.iter().map(|page| band(&page.footer)).collect()
}

// ---------------------------------------------------------------------------
// Rule 1 — link to previous is reference absence
// ---------------------------------------------------------------------------

/// Sections two and three declare no reference of any type; both must inherit
/// section one's header *and* footer, through the silent middle section. This is
/// the transitive half of the rule: it fails for any implementation that only
/// looks one section back.
#[test]
fn running_content_inheritance_is_transitive_across_three_sections() {
    let shaper = ParleyShaper::new();
    let one = section(
        9,
        PORTRAIT,
        margins(1_440),
        vec![href(HeaderFooterKind::Default, 300)],
        vec![href(HeaderFooterKind::Default, 400)],
    );
    let two = section(19, PORTRAIT, margins(1_440), Vec::new(), Vec::new());
    let three = section(29, PORTRAIT, margins(1_440), Vec::new(), Vec::new());
    let (one_id, two_id) = (one.id, two.id);
    let doc = Document::new(
        node(1),
        vec![
            section_break(100, "Section one", one_id),
            section_break(200, "Section two", two_id),
            paragraph(300_000, "Section three"),
        ],
        Definitions {
            sections: vec![one, two, three],
            headers: parts(&[(300, 1)]),
            footers: parts(&[(400, 1)]),
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    assert_eq!(layout.pages.len(), 3, "one page per section");
    assert_eq!(
        headers_of(&layout),
        vec![Some(node(301)); 3],
        "sections two and three inherit section one's header transitively"
    );
    assert_eq!(
        footers_of(&layout),
        vec![Some(node(401)); 3],
        "footer inheritance is the same rule, resolved independently"
    );
}

/// Inheritance is per **variant**, not per section: section two declares only
/// `default`, so its first page must still show the `first` variant it inherits
/// from section one — not fall back to its own `default`.
#[test]
fn a_section_inherits_the_first_page_variant_it_does_not_declare() {
    let shaper = ParleyShaper::new();
    let mut one = section(
        9,
        PORTRAIT,
        margins(1_440),
        vec![
            href(HeaderFooterKind::Default, 300),
            href(HeaderFooterKind::First, 320),
        ],
        Vec::new(),
    );
    one.title_page = Some(true);
    let mut two = section(
        19,
        PORTRAIT,
        margins(1_440),
        vec![href(HeaderFooterKind::Default, 600)],
        Vec::new(),
    );
    two.title_page = Some(true);
    let one_id = one.id;
    let doc = Document::new(
        node(1),
        vec![
            section_break(100, "Section one", one_id),
            paragraph(200, "Section two page one"),
            page_break(210, "Section two page two"),
        ],
        Definitions {
            sections: vec![one, two],
            headers: parts(&[(300, 1), (320, 1), (600, 1)]),
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    assert_eq!(
        headers_of(&layout),
        vec![
            // Section one's own first-page variant.
            Some(node(321)),
            // Section two declared only `default`; its first page inherits
            // section one's `first`, which outranks its own `default`.
            Some(node(321)),
            // Its later pages use the `default` it did declare.
            Some(node(601)),
        ]
    );
}

/// The converse: an explicit reference replaces **only** its own variant. Section
/// two declares `first` alone, so its first page uses that and its later pages
/// keep the `default` inherited from section one.
#[test]
fn a_declared_variant_replaces_only_that_variant() {
    let shaper = ParleyShaper::new();
    let one = section(
        9,
        PORTRAIT,
        margins(1_440),
        vec![href(HeaderFooterKind::Default, 300)],
        Vec::new(),
    );
    let mut two = section(
        19,
        PORTRAIT,
        margins(1_440),
        vec![href(HeaderFooterKind::First, 620)],
        Vec::new(),
    );
    two.title_page = Some(true);
    let one_id = one.id;
    let doc = Document::new(
        node(1),
        vec![
            section_break(100, "Section one", one_id),
            paragraph(200, "Section two page one"),
            page_break(210, "Section two page two"),
        ],
        Definitions {
            sections: vec![one, two],
            headers: parts(&[(300, 1), (620, 1)]),
            ..Definitions::default()
        },
    )
    .unwrap();

    assert_eq!(
        headers_of(&paginate_document(&doc, &shaper)),
        vec![Some(node(301)), Some(node(621)), Some(node(301))]
    );
}

// ---------------------------------------------------------------------------
// Rule 2 — `titlePg` is section-local and outranks `even`
// ---------------------------------------------------------------------------

/// Section two's first page is document page **2** — an even page — with
/// `w:evenAndOddHeaders` on as well. `titlePg` must still win there, and the
/// `even` variant must take over from page 4.
#[test]
fn a_sections_first_page_variant_wins_even_on_an_even_document_page() {
    let shaper = ParleyShaper::new();
    let one = section(
        9,
        PORTRAIT,
        margins(1_440),
        vec![href(HeaderFooterKind::Default, 300)],
        Vec::new(),
    );
    let mut two = section(
        19,
        PORTRAIT,
        margins(1_440),
        vec![
            href(HeaderFooterKind::Default, 600),
            href(HeaderFooterKind::First, 620),
            href(HeaderFooterKind::Even, 640),
        ],
        Vec::new(),
    );
    two.title_page = Some(true);
    let one_id = one.id;
    let doc = Document::new(
        node(1),
        vec![
            section_break(100, "Section one", one_id),
            paragraph(200, "Section two page one"),
            page_break(210, "Section two page two"),
            page_break(220, "Section two page three"),
        ],
        Definitions {
            sections: vec![one, two],
            headers: parts(&[(300, 1), (600, 1), (620, 1), (640, 1)]),
            settings: DocumentSettings {
                even_and_odd_headers: true,
                ..DocumentSettings::default()
            },
            ..Definitions::default()
        },
    )
    .unwrap();

    assert_eq!(
        headers_of(&paginate_document(&doc, &shaper)),
        vec![
            // Page 1, odd, section one's only variant.
            Some(node(301)),
            // Page 2 is even, but it is section two's *first* page: `first` wins.
            Some(node(621)),
            // Page 3, odd.
            Some(node(601)),
            // Page 4, even.
            Some(node(641)),
        ]
    );
}

// ---------------------------------------------------------------------------
// Rule 3 — a switched-off variant never shows; a switched-on one shows blank
// ---------------------------------------------------------------------------

/// `w:titlePg` with no `first` reference anywhere in the inheritance chain:
/// ECMA-376 §17.10.5 says a blank header is created, **not** that the odd/default
/// header is reused. Falling back to `default` here would silently defeat
/// "different first page".
#[test]
fn title_page_with_no_first_reference_blanks_the_first_page() {
    let shaper = ParleyShaper::new();
    let mut only = section(
        9,
        PORTRAIT,
        margins(1_440),
        vec![href(HeaderFooterKind::Default, 300)],
        vec![href(HeaderFooterKind::Default, 400)],
    );
    only.title_page = Some(true);
    let doc = Document::new(
        node(1),
        vec![paragraph(100, "One"), page_break(110, "Two")],
        Definitions {
            sections: vec![only],
            headers: parts(&[(300, 1)]),
            footers: parts(&[(400, 1)]),
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    assert_eq!(
        headers_of(&layout),
        vec![None, Some(node(301))],
        "page 1 gets a blank header, not the default one"
    );
    assert_eq!(
        footers_of(&layout),
        vec![None, Some(node(401))],
        "the footer follows the same rule"
    );
}

/// The same rule for a reference that resolves to a part with **no blocks** — an
/// explicitly emptied first-page header. It must not resurrect the default.
#[test]
fn an_explicitly_empty_first_page_reference_stays_blank() {
    let shaper = ParleyShaper::new();
    let mut only = section(
        9,
        PORTRAIT,
        margins(1_440),
        vec![
            href(HeaderFooterKind::Default, 300),
            href(HeaderFooterKind::First, 320),
        ],
        Vec::new(),
    );
    only.title_page = Some(true);
    let doc = Document::new(
        node(1),
        vec![paragraph(100, "One"), page_break(110, "Two")],
        Definitions {
            sections: vec![only],
            headers: parts(&[(300, 1), (320, 0)]),
            ..Definitions::default()
        },
    )
    .unwrap();

    assert_eq!(
        headers_of(&paginate_document(&doc, &shaper)),
        vec![None, Some(node(301))]
    );
}

/// `w:evenAndOddHeaders` with no `even` reference: even pages go blank, by the
/// same clause. Without the flag the `default` covers every page (the
/// `even_and_odd_headers_setting_is_honored` case in `tests/document_layout.rs`),
/// so this is the "switched on but undefined" half.
#[test]
fn even_and_odd_headers_with_no_even_reference_blanks_even_pages() {
    let shaper = ParleyShaper::new();
    let only = section(
        9,
        PORTRAIT,
        margins(1_440),
        vec![href(HeaderFooterKind::Default, 300)],
        Vec::new(),
    );
    let doc = Document::new(
        node(1),
        vec![
            paragraph(100, "One"),
            page_break(110, "Two"),
            page_break(120, "Three"),
        ],
        Definitions {
            sections: vec![only],
            headers: parts(&[(300, 1)]),
            settings: DocumentSettings {
                even_and_odd_headers: true,
                ..DocumentSettings::default()
            },
            ..Definitions::default()
        },
    )
    .unwrap();

    assert_eq!(
        headers_of(&paginate_document(&doc, &shaper)),
        vec![Some(node(301)), None, Some(node(301))]
    );
}

/// The switched-**off** half, in the one place it is observable: a section that
/// declares a `first` reference but no `w:titlePg` must show `default` on page 1.
/// A `header2.xml` left behind by an earlier edit is exactly this shape.
#[test]
fn a_first_reference_without_title_page_is_never_selected() {
    let shaper = ParleyShaper::new();
    let only = section(
        9,
        PORTRAIT,
        margins(1_440),
        vec![
            href(HeaderFooterKind::Default, 300),
            href(HeaderFooterKind::First, 320),
        ],
        Vec::new(),
    );
    let doc = Document::new(
        node(1),
        vec![paragraph(100, "One"), page_break(110, "Two")],
        Definitions {
            sections: vec![only],
            headers: parts(&[(300, 1), (320, 1)]),
            ..Definitions::default()
        },
    )
    .unwrap();

    assert_eq!(
        headers_of(&paginate_document(&doc, &shaper)),
        vec![Some(node(301)), Some(node(301))]
    );
}

// ---------------------------------------------------------------------------
// Rule 4 — band geometry is per section (orientation change)
// ---------------------------------------------------------------------------

/// An inherited header crossing into a landscape section must be **re-flowed** at
/// that section's content width and placed at that section's
/// `w:pgMar/@w:header` — not laid out once at the first section's width and
/// stretched. The header text is long enough to wrap at the portrait width and
/// fit on one line at the landscape width, so the line count witnesses a genuine
/// re-flow rather than a resized rect.
#[test]
fn an_orientation_change_relays_the_inherited_band_at_the_new_width() {
    use casual_doc_layout::block::BlockFragment;

    let shaper = ParleyShaper::new();
    let long = "A running header long enough that it wraps onto a second line at \
                portrait width but fits on one line across a landscape page.";
    let mut headers = DefinitionMap::default();
    headers.insert(
        HeaderFooterId::new(node(300)),
        ModelHeaderFooter {
            blocks: vec![paragraph(301, long)],
        },
    );

    let one = section(
        9,
        PORTRAIT,
        margins(1_440),
        vec![href(HeaderFooterKind::Default, 300)],
        Vec::new(),
    );
    // Landscape, tighter margins, and its own header distance — every piece of
    // band geometry differs from section one's.
    let two = section(
        19,
        LANDSCAPE,
        PageMargins {
            top_twips: 1_440,
            bottom_twips: 1_440,
            start_twips: 720,
            end_twips: 720,
            header_twips: Some(1_000),
            footer_twips: None,
            gutter_twips: None,
        },
        Vec::new(),
        Vec::new(),
    );
    let one_id = one.id;
    let doc = Document::new(
        node(1),
        vec![
            section_break(100, "Portrait", one_id),
            paragraph(200, "Landscape"),
        ],
        Definitions {
            sections: vec![one, two],
            headers,
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    assert_eq!(layout.pages.len(), 2);
    assert_eq!(headers_of(&layout), vec![Some(node(301)); 2]);

    let portrait_band = layout.pages[0].header[0].rect;
    let landscape_band = layout.pages[1].header[0].rect;
    assert_eq!(
        portrait_band.size.width,
        Twip(12_240 - 2 * 1_440),
        "the portrait band spans that section's text width"
    );
    assert_eq!(
        landscape_band.size.width,
        Twip(15_840 - 2 * 720),
        "the band must be re-laid at the landscape section's text width"
    );
    assert_eq!(
        portrait_band.origin.x,
        Twip(1_440),
        "each band starts at its own section's inside margin"
    );
    assert_eq!(landscape_band.origin.x, Twip(720));
    assert_eq!(
        portrait_band.origin.y,
        Twip(720),
        "Word's default w:pgMar/@w:header"
    );
    assert_eq!(
        landscape_band.origin.y,
        Twip(1_000),
        "the landscape section's own header distance"
    );

    // The witness that the band was re-flowed, not merely re-framed.
    let lines = |placed: &PlacedFragment| match &placed.fragment {
        BlockFragment::Paragraph { lines, .. } => lines.lines.len(),
        BlockFragment::TableRow { .. } => 0,
    };
    let portrait_lines = lines(&layout.pages[0].header[0]);
    let landscape_lines = lines(&layout.pages[1].header[0]);
    assert!(
        portrait_lines > landscape_lines,
        "the inherited header must re-break at the new content width \
         (portrait {portrait_lines} lines, landscape {landscape_lines})"
    );
    assert_eq!(landscape_lines, 1);
}

// ---------------------------------------------------------------------------
// Rule 5 — a parity blank page still shows running content
// ---------------------------------------------------------------------------

/// A blank page inserted for an `oddPage` section belongs to the section it
/// follows, so it paints that section's band **at that section's geometry** —
/// portrait here, even though the section that triggered the pad is landscape.
/// `tests/section_geometry.rs` covers the page count and body ownership; this is
/// the band half.
#[test]
fn a_parity_blank_page_paints_the_preceding_sections_band_at_its_own_geometry() {
    let shaper = ParleyShaper::new();
    let one = section(
        9,
        PORTRAIT,
        margins(1_440),
        vec![href(HeaderFooterKind::Default, 300)],
        vec![href(HeaderFooterKind::Default, 400)],
    );
    let mut two = section(
        19,
        LANDSCAPE,
        margins(720),
        vec![href(HeaderFooterKind::Default, 600)],
        Vec::new(),
    );
    two.section_type = Some(SectionType::OddPage);
    let one_id = one.id;
    let doc = Document::new(
        node(1),
        vec![
            section_break(100, "Front matter", one_id),
            paragraph(200, "Chapter"),
        ],
        Definitions {
            sections: vec![one, two],
            headers: parts(&[(300, 1), (600, 1)]),
            footers: parts(&[(400, 1)]),
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    assert_eq!(layout.pages.len(), 3, "the parity blank page is missing");
    assert!(layout.pages[1].placed.is_empty(), "page 2 is the blank pad");

    assert_eq!(
        headers_of(&layout),
        vec![Some(node(301)), Some(node(301)), Some(node(601))],
        "the pad keeps the preceding section's header"
    );
    assert_eq!(
        footers_of(&layout),
        vec![Some(node(401)); 3],
        "the footer is on every page: the pad keeps section one's, and section \
         two declares none so it inherits the same one"
    );
    assert_eq!(
        layout.pages[1].header[0].rect.size.width,
        Twip(12_240 - 2 * 1_440),
        "the pad is a page of the portrait section, so its band is portrait"
    );
    assert_eq!(
        layout.pages[2].header[0].rect.size.width,
        Twip(15_840 - 2 * 720)
    );
}

/// The pad is a real page for odd/even purposes: with `w:evenAndOddHeaders` on,
/// the inserted page 2 is even and shows the preceding section's `even` variant.
#[test]
fn a_parity_blank_page_takes_the_even_variant_when_its_number_is_even() {
    let shaper = ParleyShaper::new();
    let one = section(
        9,
        PORTRAIT,
        margins(1_440),
        vec![
            href(HeaderFooterKind::Default, 300),
            href(HeaderFooterKind::Even, 340),
        ],
        Vec::new(),
    );
    let mut two = section(
        19,
        PORTRAIT,
        margins(1_440),
        vec![
            href(HeaderFooterKind::Default, 600),
            // Section two has its own even variant, so the pad's band names the
            // section it was charged to as well as the parity it selected.
            href(HeaderFooterKind::Even, 660),
        ],
        Vec::new(),
    );
    two.section_type = Some(SectionType::OddPage);
    let one_id = one.id;
    let doc = Document::new(
        node(1),
        vec![
            section_break(100, "Front matter", one_id),
            paragraph(200, "Chapter"),
        ],
        Definitions {
            sections: vec![one, two],
            headers: parts(&[(300, 1), (340, 1), (600, 1), (660, 1)]),
            settings: DocumentSettings {
                even_and_odd_headers: true,
                ..DocumentSettings::default()
            },
            ..Definitions::default()
        },
    )
    .unwrap();

    assert_eq!(
        headers_of(&paginate_document(&doc, &shaper)),
        vec![Some(node(301)), Some(node(341)), Some(node(601))]
    );
}

// ---------------------------------------------------------------------------
// Rule 6 — band reservation counts only selectable variants
// ---------------------------------------------------------------------------

/// A tall `first` variant that `w:titlePg` never switches on must not reserve
/// band height: with the flag off every page's body starts at the top margin,
/// with it on the band overflows the margin and pushes the body down. The two
/// layouts differ only in that flag.
#[test]
fn an_unselectable_first_variant_reserves_no_band() {
    let shaper = ParleyShaper::new();
    let build = |title_page: bool| {
        let mut only = section(
            9,
            PORTRAIT,
            margins(1_440),
            vec![
                href(HeaderFooterKind::Default, 300),
                // Tall enough that 720 (the header distance) + its height clears
                // the 1440 top margin, so reserving it is observable.
                href(HeaderFooterKind::First, 320),
            ],
            Vec::new(),
        );
        only.title_page = title_page.then_some(true);
        Document::new(
            node(1),
            vec![paragraph(100, "One"), page_break(110, "Two")],
            Definitions {
                sections: vec![only],
                headers: parts(&[(300, 1), (320, 8)]),
                ..Definitions::default()
            },
        )
        .unwrap()
    };

    let off = paginate_document(&build(false), &shaper);
    let on = paginate_document(&build(true), &shaper);

    assert_eq!(
        off.pages[0].content_area.origin.y,
        Twip(1_440),
        "with w:titlePg off the first-page variant can never be selected, so it \
         must not push the body down"
    );
    assert!(
        on.pages[0].content_area.origin.y > Twip(1_440),
        "with w:titlePg on the tall first-page band does overflow the top margin"
    );
}

/// The same for a tall `even` variant without `w:evenAndOddHeaders`.
#[test]
fn an_unselectable_even_variant_reserves_no_band() {
    let shaper = ParleyShaper::new();
    let build = |even_and_odd: bool| {
        let only = section(
            9,
            PORTRAIT,
            margins(1_440),
            vec![
                href(HeaderFooterKind::Default, 300),
                href(HeaderFooterKind::Even, 340),
            ],
            Vec::new(),
        );
        Document::new(
            node(1),
            vec![paragraph(100, "One"), page_break(110, "Two")],
            Definitions {
                sections: vec![only],
                headers: parts(&[(300, 1), (340, 8)]),
                settings: DocumentSettings {
                    even_and_odd_headers: even_and_odd,
                    ..DocumentSettings::default()
                },
                ..Definitions::default()
            },
        )
        .unwrap()
    };

    assert_eq!(
        paginate_document(&build(false), &shaper).pages[0]
            .content_area
            .origin
            .y,
        Twip(1_440),
        "without w:evenAndOddHeaders the even variant is unreachable and must \
         not reserve band height"
    );
    assert!(
        paginate_document(&build(true), &shaper).pages[0]
            .content_area
            .origin
            .y
            > Twip(1_440)
    );
}

/// **Recorded deviation, not a specification.** A *reachable* tall variant is
/// reserved across the whole section, so an odd page showing only the short
/// `default` still loses the body area the tall first page needs. Word reserves
/// per page. This test pins the current behavior so the deviation cannot change
/// silently; see `HeaderFooter::band_height` for why the reservation is
/// per-section (one content area per section is what keeps the incremental
/// paginator's page reuse valid).
#[test]
fn a_reachable_tall_first_variant_costs_body_area_on_every_page_of_its_section() {
    let shaper = ParleyShaper::new();
    let mut only = section(
        9,
        PORTRAIT,
        margins(1_440),
        vec![
            href(HeaderFooterKind::Default, 300),
            href(HeaderFooterKind::First, 320),
        ],
        Vec::new(),
    );
    only.title_page = Some(true);
    let doc = Document::new(
        node(1),
        vec![paragraph(100, "One"), page_break(110, "Two")],
        Definitions {
            sections: vec![only],
            headers: parts(&[(300, 1), (320, 8)]),
            ..Definitions::default()
        },
    )
    .unwrap();

    let layout = paginate_document(&doc, &shaper);
    let first_page_top = layout.pages[0].content_area.origin.y;
    assert!(first_page_top > Twip(1_440));
    assert_eq!(
        layout.pages[1].content_area.origin.y, first_page_top,
        "page 2 shows the one-line default header, yet keeps page 1's reservation \
         — the documented per-section deviation from Word's per-page band"
    );
}

// ---------------------------------------------------------------------------
// The decision matrix itself, exercised directly
// ---------------------------------------------------------------------------

/// The table on `HeaderFooter::select`, asserted cell by cell. The end-to-end
/// tests above prove the driver feeds `select` the right section, page number and
/// flags; this proves `select`'s own truth table, including the rows no
/// realistic document reaches (an empty variant that is switched on).
#[test]
fn the_page_by_variant_decision_matrix_holds() {
    let shaper = ParleyShaper::new();
    let doc = Document::new(
        node(1),
        vec![paragraph(100, "Body")],
        Definitions::default(),
    )
    .unwrap();
    let flowed =
        |id: u64| flow_header_footer(&doc, &[paragraph(id, "Variant")], &shaper, Twip(9_360));
    let variants = HeaderFooter {
        default: flowed(700),
        first: flowed(710),
        even: flowed(720),
    };
    let picked = |number: u32, is_section_first: bool, title_page: bool, even_and_odd: bool| {
        variants
            .select(number, is_section_first, title_page, even_and_odd)
            .first()
            .map(casual_doc_layout::block::BlockFragment::node_id)
    };

    let (default, first, even) = (Some(node(700)), Some(node(710)), Some(node(720)));

    // titlePg off, evenAndOddHeaders off: `default` everywhere.
    assert_eq!(picked(1, true, false, false), default);
    assert_eq!(picked(2, false, false, false), default);
    assert_eq!(picked(3, false, false, false), default);
    // titlePg off, evenAndOddHeaders on: parity alone decides, including on the
    // section's first page.
    assert_eq!(picked(1, true, false, true), default);
    assert_eq!(picked(2, true, false, true), even);
    assert_eq!(picked(2, false, false, true), even);
    assert_eq!(picked(3, false, false, true), default);
    // titlePg on, evenAndOddHeaders off: `first` on the section's first page only.
    assert_eq!(picked(1, true, true, false), first);
    assert_eq!(picked(2, false, true, false), default);
    assert_eq!(picked(5, true, true, false), first);
    // Both on: `first` outranks `even` on the section's first page.
    assert_eq!(picked(2, true, true, true), first);
    assert_eq!(picked(4, false, true, true), even);
    assert_eq!(picked(5, false, true, true), default);

    // A switched-on variant that is empty paints a blank band; it must not fall
    // back to `default` (ECMA-376 §17.10.5).
    let missing = HeaderFooter {
        default: flowed(700),
        ..HeaderFooter::default()
    };
    assert!(
        missing.select(1, true, true, false).is_empty(),
        "w:titlePg with no first reference blanks the first page"
    );
    assert!(
        missing.select(2, false, false, true).is_empty(),
        "w:evenAndOddHeaders with no even reference blanks even pages"
    );
    assert_eq!(
        missing
            .select(3, false, true, true)
            .first()
            .map(casual_doc_layout::block::BlockFragment::node_id),
        default,
        "an ordinary odd page is unaffected"
    );

    // And the reservation only counts what can be selected: `first` here is twice
    // the height of `default`, so switching `w:titlePg` on is what grows the band.
    let tall = HeaderFooter {
        default: flowed(700),
        first: [flowed(730), flowed(740)].concat(),
        even: flowed(720),
    };
    assert_eq!(
        tall.band_height(false, false),
        tall.band_height(false, true),
        "an unreachable even variant cannot grow the band"
    );
    assert!(
        tall.band_height(true, false) > tall.band_height(false, false),
        "a reachable taller first variant does"
    );
}
