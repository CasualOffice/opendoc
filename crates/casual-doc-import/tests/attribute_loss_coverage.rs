// SPDX-License-Identifier: Apache-2.0

//! The attribute axis of the loss-coverage gate (`109` HF-243).
//!
//! # The hole this closes
//!
//! `crates/casual-doc-export/tests/source_element_coverage.rs` arms one axis:
//! **no element name in the source may vanish from the written package without a
//! compatibility-report entry naming it.** Its own doc comment stated the limit —
//! *"It is about names, not values. Attribute names are a separate axis"* — and
//! `35-DISPOSITION-TAXONOMY.md` recorded that axis as deliberately deferred. That
//! doc comment now points here (`109` HF-268).
//!
//! HF-242 is what the hole costs: `wp:wrapSquare@wrapText` was not modelled, so
//! every float wrapped on both sides whatever the author asked for, and **no
//! finding said so**. The element `wp:wrapSquare` was present in the output, so
//! the element gate was satisfied; the attribute was simply gone. Nothing said
//! HF-242 was the last one.
//!
//! # Why this is not the gate `35` deferred
//!
//! The deferred design was the element gate's own mechanism run over attribute
//! *names*: diff the source's attribute names against the written package's.
//! Measured, that yielded 52 names falling to 19, and `35` declined to arm it
//! because **the residue is dominated by spelling equivalence, not loss** —
//! `w:keepNext w:val="true"` against the bare `<w:keepNext/>` that means the same
//! thing, `w:ind w:left` against the logical `w:start`, `w:tab w:leader="none"`
//! against an omitted default. Each exception would have been a hand-checked claim
//! about an ECMA-376 implicit default, and the result would have been a gate
//! asserting *the writer's spelling conventions*.
//!
//! This guard asks a different question, and never looks at the writer's output at
//! all, so the whole spelling axis disappears:
//!
//! > **Substitute another value the corpus itself authored for this
//! > `element/@attribute`. Does anything about the import change?**
//!
//! If the model, the compatibility report, the preservation ledger and the
//! retained-part side-table are all exactly what they were, then the attribute as
//! the author wrote it reached nothing and was named by nothing. That is the
//! definition of a silent attribute loss, and it is independent of how any writer
//! spells anything.
//!
//! **Where the substitute comes from is the whole trick**, and two earlier drafts
//! of this file got it wrong in instructive ways.
//!
//! The first *invented* a replacement from the authored value — a flipped boolean,
//! a neighbouring number, a complemented colour, an unused token. For an
//! enumerated attribute that cannot work: there is no way to derive
//! `ST_Underline`'s vocabulary from the token `single`, an invented token is
//! invalid, and a parser that falls back to the schema default on invalid input is
//! indistinguishable from one that never read the attribute. Measured, that draft
//! accused `w:u@w:val`, `w:tab@w:leader`, `w:headerReference@w:type` and
//! `a:prstGeom@prst` — all of which the importer plainly reads — of silent loss.
//!
//! The second *deleted* the attribute instead, which needs no vocabulary but
//! answers a different question. "Nothing changed when it was removed" conflates
//! *unread* with *the authored value is the state the model already holds*, and the
//! second is `35`'s no-op rule rather than a loss: `w:u w:val="single"` deleted
//! changes nothing because `single` is the model's canonical line style. That draft
//! reported forty-five pairs, most of them `w:tcW@w:type="dxa"`-shaped — an
//! attribute written at its default value, in a corpus that never writes any other.
//!
//! Taking the substitute from the corpus fixes both, and makes the gate's own
//! limits explicit rather than silent: see [`is_testable`].
//!
//! # The known pattern
//!
//! **Metamorphic testing**, in its standard use: perturb an input and require an
//! observable change, to prove a declared input is actually consumed. Differential
//! testing cannot answer this (there is no second implementation to differ from)
//! and neither can a golden (a golden records what the code does, including
//! ignoring the attribute).
//!
//! The population under test is **derived** — every `element/@attribute` the gated
//! sources actually carry, read out of the packages at test time, which is what
//! HF-243 means by *not a per-construct `report_attribute` call added by hand each
//! time*. A fixture added to `fixtures/` joins the gate by existing. The second
//! half of the pattern is [`EXCEPTIONS`], a suppression list that requires a reason
//! per row, carrying the staleness rule that makes such a list survive contact with
//! time: a row no source exercises is itself a failure, the analogue of
//! `--report-unused-disable-directives`.
//!
//! **There is deliberately no baseline and no ratchet.** An earlier draft had one,
//! because arming a derived gate over an existing importer usually needs a list of
//! losses it starts out knowing about. Measured, the residue was seven pairs, all
//! one family — the DrawingML transform and non-visual properties of a *lone*
//! picture, which `is_drawing_scaffolding` silenced unconditionally — and all seven
//! were closable in this crate, so the list would have shipped empty. An empty
//! baseline with a regeneration switch is strictly worse than no baseline: it is an
//! escape hatch a future regression can be written into, and nothing in the gate
//! could tell that from a legitimate retrofit. If a later lane finds a loss it
//! genuinely cannot close, the ratchet is the right thing to add *then* — but it
//! must never share a list with [`EXCEPTIONS`], because an exception row claims
//! nothing was lost and parking an unfixed loss behind that claim launders it.
//!
//! # What it does and does not claim
//!
//! - **Corpus-scoped, and the scope is measured, not claimed.** A pair the corpus
//!   authors with only one value carries no difference to lose, so the gate says
//!   nothing about it; [`is_testable`] draws that line and
//!   [`the_gates_reach_is_measured_and_covers_the_defect_that_motivated_it`]
//!   publishes how many pairs fall inside it on every run.
//! - **A floor, not a fidelity measure.** A changed import proves *something* read
//!   the value, not that the value was modelled faithfully.
//! - **Package plumbing is out of scope**, as `35` already places it: the
//!   content-type manifest and `_rels/*` are not document content, and a writer
//!   emits its own correct values for them.
//!
//! # Complexity
//!
//! `O(pairs × package bytes)`. One baseline import per package, then perturbed
//! imports for each testable pair the report does not already name, stopping at the
//! first that moves — findings subsume their attributes before any import runs, so
//! a document full of reported losses costs almost nothing extra. Nothing here runs
//! at edit time.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use casual_doc_import::{
    Import, ImportConfig, ImportMode, MeaningfulMarkup, RSID_CLASS_FEATURE, import_package,
    meaningful_markup,
};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// Source `element/@attribute` pairs whose authored value legitimately reaches
/// nothing, each with the reason that is not a loss.
///
/// Adding a row is a **decision that nothing was lost**, and it must say why in
/// terms of the document rather than of the code. "We have not got to it yet" is
/// not such a reason: that is a real loss, it belongs in the report as a finding or
/// in the model, and parking it here would put a false claim in front of it. Every
/// row is exercised by at least one gated source
/// ([`no_attribute_exception_is_stale`]), so the list cannot accumulate rows that
/// no longer describe anything.
///
/// A documented *class* is not a row here: the revision-save-ID class is resolved
/// from the report's own class entry (see [`covered_by_the_rsid_class`]), so its
/// seven attribute names do not need seven rows going stale one at a time.
///
/// `"*"` as the element matches any, so one documented exclusion covers a
/// package-wide processing attribute instead of one row per part root.
const EXCEPTIONS: &[(&str, &str, &str)] = &[
    (
        "*",
        "Ignorable",
        "`35`: `mc:Ignorable` is a markup-compatibility processing directive \
         naming which namespace prefixes a consumer may ignore. `35` already \
         places it outside the taxonomy — it carries no document content, and a \
         writer emits its own correct value for the prefixes it declares. One row \
         for every part root that carries one, because the attribute means the \
         same thing on `w:document`, `w:styles`, `w:numbering`, `w:footnotes`, \
         `w:hdr` and `w:ftr`.",
    ),
    (
        "footnote",
        "type",
        "`w:footnote@w:type` distinguishes the body notes from the two STOCK \
         notes Word keeps at the head of the part: the `separator` rule drawn \
         above a footnote block and the `continuationSeparator` drawn above a \
         block continued from the previous page. This engine's layout draws both \
         rules for itself, so neither note's content reaches the model and \
         swapping one type for the other changes nothing — which is exactly why \
         `35` excludes the separator notes, and why the element gate excuses \
         `w:separator` and `w:continuationSeparator` themselves. A note with no \
         `@w:type` is a real note and is imported; that is the case this row does \
         NOT cover, because the gate only sees a value substituted for another \
         value the corpus authored, and the corpus authors only the two stock \
         types.",
    ),
    (
        "endnote",
        "type",
        "The `w:endnote@w:type` half of the stock-separator reasoning above.",
    ),
    (
        "docPr",
        "id",
        "`wp:docPr@id` is ECMA-376's unique identifier for a drawing object \
         within the document, and nothing in the package joins on it: a drawing's \
         image, hyperlink and chart are reached through `r:embed`/`r:id` \
         relationships, and a comment or bookmark anchors to a paragraph, never \
         to a drawing id. The model mints its own stable node identity, so the \
         number is a producer's bookkeeping. Distinct from `@name`, which is the \
         author-visible object name and is NOT excused.",
    ),
    (
        "cNvPr",
        "id",
        "The `pic:cNvPr`/`wps:cNvPr` flavour of the drawing-object id above, \
         same reasoning: a non-visual id nothing in the package references.",
    ),
    (
        "property",
        "pid",
        "`custom-properties/property@pid` is the schema-required ordinal of a \
         custom document property, starting at 2 and dense. The property is \
         identified by `@name` — which is what a consumer, a field code and this \
         importer all look it up by — so the ordinal says only where in the part \
         it was written. The writer renumbers from the model's own order.",
    ),
    (
        "vector",
        "baseType",
        "`vt:vector@baseType` declares the variant type of the vector's \
         children, which the child elements already state by name (`vt:lpstr`, \
         `vt:variant`, `vt:i4`). A `baseType` disagreeing with its children is \
         malformed, not a document fact; the importer reads the children.",
    ),
    (
        "tblW",
        "w",
        "`w:tblW@w:w` is ignored when `@w:type` is `auto` (ECMA-376 \
         `ST_TblWidth`: the width is computed from the content), and `auto` is \
         what both flagged fixtures author — `<w:tblW w:w=\"0\" w:type=\"auto\"/>`. \
         The gate proves `@w:w` is read where it means something: the \
         `real-producer-*` fixtures author `pct` and `dxa` widths and none of \
         them flags this pair. So what fires here is a value the document itself \
         declares inert.",
    ),
    (
        "style",
        "styleId",
        "`w:style@w:styleId` is a join key, consumed rather than stored: the \
         importer resolves `w:pStyle`/`w:rStyle`/`w:tblStyle` through it and the \
         writer re-mints ids from the model. The one fixture that flags it \
         declares a single style — the document default, identified by \
         `@w:default=\"1\"` rather than by its id — and no `w:pStyle` anywhere, so \
         there is no join for the id to carry. Every fixture with referenced \
         styles reads it, which is why none of them flags this pair.",
    ),
    (
        "vector",
        "size",
        "`vt:vector@size` restates the number of children the vector already \
         has, so it carries nothing the markup does not. A size disagreeing with \
         the child count is malformed, not a document fact; the importer counts \
         the children.",
    ),
    (
        "graphicData",
        "uri",
        "`a:graphicData@uri` declares which DrawingML graphic vocabulary the one \
         child element belongs to, and the child element's own name says the same \
         thing — the importer dispatches on the child (`pic:pic`, `wps:wsp`, \
         `wpg:wgp`, `c:chart`). A `uri` disagreeing with its child is malformed. \
         Not the same as a `uri` naming a vocabulary this engine does not model: \
         that case raises a finding on the child, or on `a:graphic` when there is \
         no recognised child at all.",
    ),
];

/// Fixtures the package reader or the importer refuses before any comparison can
/// run, with the refusal each one exists to exercise. Same list, same reason, as
/// the element gate: a fixture that silently stopped being refused would silently
/// stop being covered.
const REFUSED_BY_DESIGN: &[(&str, &str)] = &[
    ("duplicate-part.docx", "two entries for one part name"),
    ("high-expansion.docx", "a zip bomb's expansion ratio"),
    ("malformed-truncated.docx", "a truncated archive"),
    ("path-traversal.docx", "a `../` part name"),
];

/// One `element/@attribute` pair, by XML local name on both halves.
///
/// Local names only, for the reason `FeatureLocation` records them: a namespace
/// *prefix* is whatever the producer's `xmlns:` bound it to, so comparing prefixes
/// would compare writers rather than constructs.
type Pair = (String, String);

/// Imports a package and returns the whole observable surface of the load.
///
/// `Import` is `PartialEq` over all of it — the model, the compatibility report,
/// the preservation ledger, the retained-part side-table and the embedded-part
/// bookkeeping — which is exactly the surface a deletion has to move to prove the
/// attribute was read. A refusal is `None`, and differs from a success, because an
/// importer that *rejects* a document for a missing attribute has plainly read it.
fn observe(bytes: &[u8]) -> Option<Import> {
    let mut package = DocxPackage::open(bytes, PackageLimits::default()).ok()?;
    import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .ok()
}

/// The XML parts of a package, keyed by name. `.rels` and the content-type
/// manifest are **excluded**: `35` places package plumbing outside the taxonomy,
/// and a relationship id or a part name is not a value an author chose.
fn document_parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip package");
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let mut parts = BTreeMap::new();
    for name in names {
        if !name.ends_with(".xml") || name == "[Content_Types].xml" {
            continue;
        }
        let mut buffer = Vec::new();
        archive
            .by_name(&name)
            .expect("a listed entry")
            .read_to_end(&mut buffer)
            .expect("a readable entry");
        parts.insert(name, buffer);
    }
    parts
}

/// Rebuilds a package with some parts' bytes replaced, leaving every other entry
/// byte-identical. Stored (uncompressed) so the rebuild is cheap and the reader
/// sees exactly the bytes handed in.
fn repackage(bytes: &[u8], replacements: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    use std::io::Write;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip package");
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for name in names {
        let mut buffer = Vec::new();
        archive
            .by_name(&name)
            .expect("a listed entry")
            .read_to_end(&mut buffer)
            .expect("a readable entry");
        writer.start_file(&name, options).expect("a zip entry");
        writer
            .write_all(replacements.get(&name).unwrap_or(&buffer))
            .expect("a written entry");
    }
    writer.finish().expect("a finished zip").into_inner()
}

/// Substitutes `substitute` for every occurrence of `element/@attribute` in one
/// XML part, leaving every other byte of the part untouched.
///
/// Byte splicing rather than re-serialisation: a round trip through an XML writer
/// would normalise whitespace, attribute order and entity spelling, and the
/// importer would then be reading a differently-shaped document for reasons that
/// have nothing to do with the attribute under test.
///
/// `None` when nothing changed — the pair does not occur here, or every
/// occurrence already carries `substitute`.
fn substituted(xml: &[u8], (element, attribute): &Pair, substitute: &str) -> Option<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut out: Vec<u8> = Vec::with_capacity(xml.len());
    let mut cursor = 0_usize;
    let mut changed = false;
    loop {
        let before = usize::try_from(reader.buffer_position()).expect("a part within usize");
        let event = reader.read_event_into(&mut buffer);
        let after = usize::try_from(reader.buffer_position()).expect("a part within usize");
        let Ok(event) = event else { break };
        if matches!(event, Event::Eof) {
            break;
        }
        if let Event::Start(start) | Event::Empty(start) = &event
            && start.local_name().as_ref() == element.as_bytes()
            && let Some(spliced) = splice(&xml[before..after], start, attribute, substitute)
        {
            out.extend_from_slice(&xml[cursor..before]);
            out.extend_from_slice(&spliced);
            cursor = after;
            changed = true;
        }
        buffer.clear();
    }
    out.extend_from_slice(&xml[cursor..]);
    changed.then_some(out)
}

/// Rewrites one element's raw bytes with a different value for `attribute`, or
/// `None` when the element does not carry it or already carries `substitute`.
///
/// Works on the raw slice so the element's own spelling survives; the parsed
/// `BytesStart` is consulted only for the authored value and the exact key
/// spelling (`w:val`, `val`, `w14:paraId`) to find. XML forbids a duplicate
/// attribute, so the first `key` followed by `=` is the one.
fn splice(
    raw: &[u8],
    start: &BytesStart<'_>,
    attribute: &str,
    substitute: &str,
) -> Option<Vec<u8>> {
    let authored = start
        .attributes()
        .filter_map(Result::ok)
        .find(|candidate| candidate.key.local_name().as_ref() == attribute.as_bytes())?;
    if String::from_utf8_lossy(authored.value.as_ref()).trim() == substitute {
        return None;
    }
    let key = authored.key.as_ref().to_vec();
    let mut at = 0_usize;
    while at + key.len() <= raw.len() {
        let found = at
            + raw[at..]
                .windows(key.len())
                .position(|window| window == key)?;
        let rest = &raw[found + key.len()..];
        let equals = rest.iter().position(|byte| !byte.is_ascii_whitespace())?;
        if rest[equals] != b'=' {
            at = found + key.len();
            continue;
        }
        let open = found
            + key.len()
            + equals
            + 1
            + rest[equals + 1..].iter().position(|byte| *byte == b'"')?;
        let close = open + 1 + raw[open + 1..].iter().position(|byte| *byte == b'"')?;
        let mut spliced = Vec::with_capacity(raw.len() + substitute.len());
        spliced.extend_from_slice(&raw[..=open]);
        spliced.extend_from_slice(quick_xml::escape::escape(substitute).as_ref().as_bytes());
        spliced.extend_from_slice(&raw[close..]);
        return Some(spliced);
    }
    None
}

/// Every `element/@attribute` pair one package carries with the values authored
/// for it, plus the element ancestry a subsuming finding is judged against.
#[derive(Default)]
struct Authored {
    /// The values authored for each pair, by XML local name on both halves.
    values: BTreeMap<Pair, BTreeSet<String>>,
    /// The source's meaningful element markup, for ancestor subsumption.
    markup: MeaningfulMarkup,
}

impl Authored {
    /// Merges another package's pairs and values in — how the corpus-wide
    /// vocabulary is accumulated.
    fn absorb(&mut self, other: &Self) {
        for (pair, values) in &other.values {
            self.values
                .entry(pair.clone())
                .or_default()
                .extend(values.iter().cloned());
        }
    }
}

/// Reads one package's authored pairs and values out of its document parts.
fn authored(bytes: &[u8]) -> Authored {
    let mut authored = Authored::default();
    for xml in document_parts(bytes).values() {
        authored.markup.absorb(meaningful_markup(xml));
        let mut reader = Reader::from_reader(xml.as_slice());
        let mut buffer = Vec::new();
        while let Ok(event) = reader.read_event_into(&mut buffer) {
            match &event {
                Event::Eof => break,
                Event::Start(start) | Event::Empty(start) => {
                    let element = String::from_utf8_lossy(start.local_name().as_ref()).into_owned();
                    for attribute in start.attributes().filter_map(Result::ok) {
                        if attribute.key.as_ref().starts_with(b"xmlns") {
                            continue;
                        }
                        let name = String::from_utf8_lossy(attribute.key.local_name().as_ref())
                            .into_owned();
                        let value = String::from_utf8_lossy(attribute.value.as_ref())
                            .trim()
                            .to_owned();
                        authored
                            .values
                            .entry((element.clone(), name))
                            .or_default()
                            .insert(value);
                    }
                }
                _ => {}
            }
            buffer.clear();
        }
    }
    authored
}

/// Whether some baseline finding already names this pair, its element, or an
/// element enclosing every occurrence of its element.
///
/// A finding on the element subsumes its attributes: if the whole `a:custGeom` is
/// reported as geometry outside the modelled subset, its `a:pt@x` is not a second
/// loss to chase. Ancestor subsumption is the rule `35` already states for
/// elements — a whole-subtree loss is reported once on its outermost element —
/// applied to the attributes inside that subtree.
///
/// Only a finding with **no** attribute in its location subsumes an element's other
/// attributes, and that distinction is load-bearing. Counting an attribute finding
/// as an element-wide one makes a single row blanket the whole element: when
/// `wp:docPr@name` started reporting, it silently excused `wp:docPr@id`, which the
/// staleness check caught only because that pair happened to carry an exception
/// row. A `degraded` attribute finding says one *part* of the element's meaning was
/// lost, not all of it.
fn covered_by_a_finding(
    import: &Import,
    (element, attribute): &Pair,
    markup: &MeaningfulMarkup,
) -> bool {
    let wholly_named: BTreeSet<&str> = import
        .report
        .entries
        .iter()
        .filter(|entry| entry.location.attribute.is_none())
        .filter_map(|entry| entry.location.element.as_deref())
        .collect();
    if wholly_named.contains(element.as_str()) {
        return true;
    }
    if import.report.entries.iter().any(|entry| {
        entry.location.element.as_deref() == Some(element.as_str())
            && entry.location.attribute.as_deref() == Some(attribute.as_str())
    }) {
        return true;
    }
    markup
        .ancestors_of_every_occurrence(element)
        .is_some_and(|ancestors| ancestors.iter().any(|a| wholly_named.contains(a.as_str())))
}

/// Whether this attribute belongs to the revision-save-ID class the report
/// dispositions once per document.
///
/// `35` settles the aggregation: `w:rsid*` is the highest-volume attribute in
/// WordprocessingML, per-construct reporting would inflate every report by about
/// half with rows nothing can act on, and the class entry `docx.rsid` carries the
/// tally instead. So the class entry's presence is what covers the attribute —
/// not seven exception rows going stale one at a time.
fn covered_by_the_rsid_class(import: &Import, (_, attribute): &Pair) -> bool {
    attribute.starts_with("rsid")
        && import
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == RSID_CLASS_FEATURE)
}

/// Whether substituting another value the corpus authored for this pair moves
/// anything about the import.
///
/// Every alternative is tried, not just the first, because two spellings of one
/// meaning (`w:val="true"` against `w:val="1"`) are a real pair of corpus values
/// and substituting one for the other proves nothing. One changed import on any
/// alternative is enough.
fn is_read(
    bytes: &[u8],
    baseline: &Option<Import>,
    pair: &Pair,
    vocabulary: &BTreeSet<String>,
) -> bool {
    for (part, xml) in document_parts(bytes) {
        for substitute in vocabulary {
            let Some(perturbed) = substituted(&xml, pair, substitute) else {
                continue;
            };
            let replacements = [(part.clone(), perturbed)].into_iter().collect();
            if &observe(&repackage(bytes, &replacements)) != baseline {
                return true;
            }
        }
    }
    false
}

/// Pairs in one package whose authored value reached nothing and was named by
/// nothing. `corpus` supplies the cross-fixture vocabulary.
fn silent_attribute_losses(bytes: &[u8], corpus: &Authored) -> Vec<Pair> {
    let baseline = observe(bytes);
    let Some(import) = baseline.as_ref() else {
        return Vec::new();
    };
    let authored = authored(bytes);
    authored
        .values
        .keys()
        .filter(|pair| is_testable(pair, corpus))
        .filter(|pair| !covered_by_a_finding(import, pair, &authored.markup))
        .filter(|pair| !covered_by_the_rsid_class(import, pair))
        .filter(|pair| !is_read(bytes, &baseline, pair, &corpus.values[*pair]))
        .cloned()
        .collect()
}

/// Whether the corpus can say anything about this pair at all.
///
/// **This predicate is the gate's reach, and it is the reason the gate is honest
/// rather than noisy.** A pair the corpus authors with a single value is
/// untestable: substituting that value for itself is a no-op, and *deleting* it
/// cannot distinguish an unread attribute from one whose authored value is exactly
/// the state the model already holds. `w:u w:val="single"` is the example to keep
/// in mind — the importer plainly reads it (`properties.underline = Some(val !=
/// Some("none"))`), the corpus only ever writes `single`, and `single` is the
/// model's canonical line style, so no probe over this corpus can move the import
/// and no loss is there to find. An earlier draft of this file reported it, and
/// `w:tab@w:leader="none"` and `w:headerReference@w:type="default"` with it, as
/// silent losses; all three were the probe's limitation and not the importer's.
///
/// Two authored values mean a *difference* exists to carry, and the gate asks
/// whether the importer carries it.
/// [`the_gates_reach_is_measured_and_covers_the_defect_that_motivated_it`]
/// publishes how many pairs that is, so the reach is a number rather than a claim.
fn is_testable(pair: &Pair, corpus: &Authored) -> bool {
    corpus
        .values
        .get(pair)
        .is_some_and(|values| values.len() > 1)
}

/// Every `.docx` under `fixtures/`, sorted, so a new fixture joins the gate by
/// existing rather than by being remembered.
fn fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut paths: Vec<PathBuf> = ["corpus", "generated"]
        .iter()
        .flat_map(|sub| {
            std::fs::read_dir(root.join(sub))
                .expect("a fixture directory")
                .map(|entry| entry.expect("a directory entry").path())
        })
        .filter(|path| path.extension().is_some_and(|ext| ext == "docx"))
        .collect();
    paths.sort();
    paths
}

/// Every package the gate covers.
fn gated_sources() -> Vec<(String, Vec<u8>)> {
    fixtures()
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            (!REFUSED_BY_DESIGN
                .iter()
                .any(|(fixture, _)| *fixture == name))
            .then(|| (name, std::fs::read(&path).expect("a readable fixture")))
        })
        .collect()
}

/// Whether an [`EXCEPTIONS`] row excuses this pair.
fn excused((element, attribute): &Pair) -> bool {
    EXCEPTIONS
        .iter()
        .any(|(excused_element, excused_attribute, _)| {
            (*excused_element == "*" || excused_element == element)
                && excused_attribute == attribute
        })
}

/// The corpus-wide vocabulary: every authored value of every pair, over every
/// gated source. Read before any probe runs, because [`is_testable`] is a
/// statement about the corpus rather than about one document.
fn corpus(sources: &[(String, Vec<u8>)]) -> Authored {
    let mut corpus = Authored::default();
    for (_, bytes) in sources {
        corpus.absorb(&authored(bytes));
    }
    corpus
}

/// Every silent attribute loss across the gate, with the fixtures each was seen
/// in — before [`EXCEPTIONS`] is consulted.
fn firing() -> BTreeMap<Pair, BTreeSet<String>> {
    let sources = gated_sources();
    let corpus = corpus(&sources);
    let mut losses: BTreeMap<Pair, BTreeSet<String>> = BTreeMap::new();
    for (name, bytes) in &sources {
        for pair in silent_attribute_losses(bytes, &corpus) {
            losses.entry(pair).or_default().insert(name.clone());
        }
    }
    losses
}

#[test]
fn no_authored_attribute_value_vanishes_without_a_finding() {
    let failures: Vec<String> = firing()
        .into_iter()
        .filter(|(pair, _)| !excused(pair))
        .map(|((element, attribute), fixtures)| {
            format!(
                "  `{element}/@{attribute}` — every other value the corpus authors \
                 for it imports identically, and no finding names it ({})",
                fixtures.into_iter().collect::<Vec<_>>().join(", ")
            )
        })
        .collect();
    assert!(
        failures.is_empty(),
        "an attribute's authored value is discarded in silence — model it, add the \
         report site, or add a justified row to EXCEPTIONS saying why nothing was \
         lost:\n{}",
        failures.join("\n")
    );
}

#[test]
fn no_attribute_exception_is_stale() {
    // The `--report-unused-disable-directives` half of the pattern. An allowlist
    // nobody prunes stops being a statement about the code, and the row that goes
    // stale first is the one whose loss has just been closed — exactly the row
    // whose removal would tighten the gate.
    let firing = firing();
    let unused: Vec<String> = EXCEPTIONS
        .iter()
        .filter(|(element, attribute, _)| {
            !firing.keys().any(|(authored_element, authored_attribute)| {
                (*element == "*" || element == authored_element) && attribute == authored_attribute
            })
        })
        .map(|(element, attribute, _)| format!("{element}/@{attribute}"))
        .collect();
    assert!(
        unused.is_empty(),
        "these EXCEPTIONS rows no longer describe anything the gated sources do; \
         delete them so the gate keeps its teeth: {unused:?}"
    );
}

#[test]
fn the_gates_reach_is_measured_and_covers_the_defect_that_motivated_it() {
    // `SKILL.md` §9 rule 2: prose about a gate must name what is armed, and a test
    // must assert the gate is armed. The reach here is the count of TESTABLE pairs
    // — those the corpus authors with more than one value — and it is derived on
    // every run rather than quoted, so it cannot go stale into a false claim.
    let sources = gated_sources();
    let corpus = corpus(&sources);
    let testable: Vec<&Pair> = corpus
        .values
        .keys()
        .filter(|pair| is_testable(pair, &corpus))
        .collect();
    assert!(
        testable.len() * 4 > corpus.values.len(),
        "the gate can speak about {} of {} authored pairs; below a quarter it is \
         thin enough that something has narrowed the sweep (a part excluded, the \
         fixtures not read) rather than the corpus being poor",
        testable.len(),
        corpus.values.len()
    );

    // And the specific instance HF-243 was raised from. `wp:wrapSquare@wrapText`
    // is the attribute that vanished in silence and reached a user (HF-242): the
    // corpus authors `bothSides`, `left` and `right` for it, so the gate can see
    // it, and it must be read rather than baselined. If this pair ever stops being
    // testable or starts being baselined, the gate has lost the one case it is
    // known to catch.
    let wrap = ("wrapSquare".to_owned(), "wrapText".to_owned());
    assert!(
        is_testable(&wrap, &corpus),
        "the corpus no longer authors two wrap sides, so HF-242's own instance is \
         outside the gate"
    );
    assert!(
        !firing().contains_key(&wrap),
        "wrapSquare/@wrapText is unread again — this is HF-242, and the gate exists \
         because it reached a user once already"
    );
}
