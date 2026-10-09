//! Settings-part parsing: `word/settings.xml` -> v1 `DocumentSettings`.
//!
//! The load-bearing settings are modeled (font-embedding flags, header parity,
//! default tab stop, revision tracking, proof state, document/write protection,
//! default table style, view, zoom, the theme font languages, and the
//! `w:compatSetting` triples), and so are the ones Word writes into every
//! document it saves (`109` FID-AT-10): the decimal symbol and list separator,
//! the preview-picture and picture-compression flags, `w:compat/w:useFELayout`,
//! the `w14`/`w15` document ids and default image resolution, and — retained
//! VERBATIM, because nothing here consumes them — `m:mathPr` and
//! `w:shapeDefaults`. A verbatim fragment is kept only when every name in it
//! resolves to a namespace the writer declares under the same prefix; one that
//! does not is reported as it always was. Every OTHER
//! top-level setting is REPORTED (never silently dropped), so an unmodeled
//! setting is auditable and — in Retention mode — preserved by the byte floor.
//! Elements are matched by local name (namespace-agnostic); each `CT_OnOff` flag
//! is present-means-true unless an explicit `w:val` says otherwise.
//!
//! Two rules in here are about the *attributes* of elements this parser already
//! recognises, which the element-name catch-all cannot see:
//! [`enforcement`] (absent means **enforced**, per MS-OI29500 §17.15.1.29) and
//! [`report_unmodeled_attributes`] (the password groups on the two protection
//! elements, reported rather than dropped in silence).

use casual_doc_model::v1::{
    CompatSetting, DocumentProtection, DocumentProtectionEdit, DocumentSettings, NoteNumberRestart,
    NotePosition, NoteProperties, ProofState, WriteProtection, Zoom, ZoomMode,
};
// Own line, kept out of any sorted block (the repo's parallel-PR rule).
use casual_doc_model::v1::{DocumentView, PasswordAttribute, PasswordVerifier, ThemeFontLanguages};
// Own line, kept out of any sorted block (the repo's parallel-PR rule).
use casual_doc_model::v1::{LEGACY_COMPAT_OPTIONS, MAX_ATTACHED_TEMPLATE_BYTES};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use std::collections::BTreeMap;

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::attribute_value;
use crate::report::Reporter;

/// Parses the settings part, returning the modeled subset (default when none of
/// the recognized settings appear). Every unmodeled top-level setting — and every
/// `w:compat` child that is not a known switch — is reported.
///
/// `templates` maps the part's `attachedTemplate` relationship ids to their
/// targets (`settings.xml.rels`), which is where `w:attachedTemplate` points.
pub(crate) fn parse(
    xml: &[u8],
    templates: &BTreeMap<String, String>,
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<DocumentSettings, ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut settings = DocumentSettings::default();
    let mut elements = 0_u64;
    let mut depth = 0_u64;
    // `level` is the nesting depth at which an element's own event fires (the root
    // `w:settings` is level 0, its direct setting children level 1, `w:compat`'s
    // children level 2). We act only on the settings themselves (level 1) and on
    // `w:compat`'s children (level 2); deeper markup inside an unmodeled setting is
    // subsumed by that setting's single report entry.
    let mut in_compat = false;
    // The document-default note-property container (`w:footnotePr`/`w:endnotePr`)
    // currently open, if any; its `w:pos`/`w:numFmt`/`w:numStart`/`w:numRestart`
    // children (level 2) route to the matching side of `settings`.
    let mut note_scope: Option<NoteScope> = None;
    // The namespace bindings the root declares, read once from `w:settings`, so a
    // `docId` can be told apart by namespace (`w14` and `w15` share the local
    // name) and a verbatim fragment can be checked to mean what it says when it
    // is written back under the writer's own declarations.
    let mut bindings = Bindings::default();
    // A `m:mathPr` or `w:shapeDefaults` being captured verbatim (`109` FID-AT-10).
    let mut capture: Option<Capture> = None;

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| ImportError::MalformedXml)?;
        if let Some(open) = capture.as_mut() {
            // Inside a captured fragment every event is the fragment's; the depth
            // and element ceilings still apply, so a hostile fragment is bounded
            // exactly as the rest of the part is.
            match &event {
                Event::Eof => return Err(ImportError::MalformedXml),
                Event::DocType(_) => return Err(ImportError::MalformedXml),
                Event::Start(element) => {
                    depth += 1;
                    if depth > config.max_depth {
                        return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                    }
                    bump(&mut elements, config.max_elements)?;
                    open.nesting += 1;
                    open.check(element, &bindings);
                }
                Event::Empty(element) => {
                    bump(&mut elements, config.max_elements)?;
                    open.check(element, &bindings);
                }
                Event::End(_) => {
                    depth = depth.saturating_sub(1);
                    open.nesting = open.nesting.saturating_sub(1);
                }
                _ => {}
            }
            open.write(&event);
            if open.nesting == 0 {
                let finished = capture.take().expect("a capture is open");
                finished.finish(&mut settings, reporter);
            }
            buffer.clear();
            continue;
        }
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(ImportError::MalformedXml),
            Event::Start(element) => {
                let level = depth;
                depth += 1;
                if depth > config.max_depth {
                    return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                }
                bump(&mut elements, config.max_elements)?;
                let local = element.local_name();
                let fragment = if level == 1 {
                    Fragment::of(&element, &bindings)
                } else {
                    None
                };
                match (level, local.as_ref(), fragment) {
                    (0, _, _) => bindings = Bindings::of(&element),
                    (1, b"compat", _) => in_compat = true,
                    (1, b"footnotePr", _) => note_scope = Some(NoteScope::Footnote),
                    (1, b"endnotePr", _) => note_scope = Some(NoteScope::Endnote),
                    (_, _, Some(kind)) => {
                        let mut open = Capture::new(kind);
                        open.check(&element, &bindings);
                        open.write(&Event::Start(element.borrow()));
                        capture = Some(open);
                    }
                    _ => on_setting(
                        Position {
                            level,
                            in_compat,
                            note_scope,
                        },
                        &element,
                        false,
                        &bindings,
                        templates,
                        &mut settings,
                        reporter,
                    ),
                }
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                match Fragment::of(&element, &bindings) {
                    Some(kind) if depth == 1 => {
                        let mut open = Capture::new(kind);
                        open.check(&element, &bindings);
                        open.write(&Event::Empty(element.borrow()));
                        open.finish(&mut settings, reporter);
                    }
                    _ => on_setting(
                        Position {
                            level: depth,
                            in_compat,
                            note_scope,
                        },
                        &element,
                        true,
                        &bindings,
                        templates,
                        &mut settings,
                        reporter,
                    ),
                }
            }
            Event::End(element) => {
                match element.local_name().as_ref() {
                    b"compat" => in_compat = false,
                    b"footnotePr" | b"endnotePr" => note_scope = None,
                    _ => {}
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(settings)
}

/// The namespace URIs a verbatim settings fragment may use, by the prefix the
/// writer declares for each on `w:settings`. A fragment naming anything else is
/// not retained — it is reported, as it was before it could be retained —
/// because written under the writer's declarations it would no longer mean what
/// it meant.
const FRAGMENT_NAMESPACES: [(&[u8], &[u8]); 4] = [
    (
        b"w",
        b"http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    ),
    (
        b"m",
        b"http://schemas.openxmlformats.org/officeDocument/2006/math",
    ),
    (b"o", b"urn:schemas-microsoft-com:office:office"),
    (b"v", b"urn:schemas-microsoft-com:vml"),
];

/// The Word 2010 and 2013 extension namespaces, which share the local name
/// `docId`.
const W14_NAMESPACE: &[u8] = b"http://schemas.microsoft.com/office/word/2010/wordml";
const W15_NAMESPACE: &[u8] = b"http://schemas.microsoft.com/office/word/2012/wordml";

/// The `xmlns:` declarations on the `w:settings` root.
#[derive(Default)]
struct Bindings {
    declared: Vec<(Vec<u8>, Vec<u8>)>,
}

impl Bindings {
    /// Reads every prefixed namespace declaration on `root`.
    fn of(root: &BytesStart<'_>) -> Self {
        let declared = root
            .attributes()
            .flatten()
            .filter_map(|attribute| {
                let key = attribute.key.as_ref();
                let prefix = key.strip_prefix(b"xmlns:")?;
                Some((prefix.to_vec(), attribute.value.into_owned()))
            })
            .collect();
        Self { declared }
    }

    /// The URI `prefix` is bound to on the root, if it is.
    fn uri(&self, prefix: &[u8]) -> Option<&[u8]> {
        self.declared
            .iter()
            .find(|(declared, _)| declared == prefix)
            .map(|(_, uri)| uri.as_slice())
    }

    /// The URI an element's own prefix is bound to on the root.
    fn uri_of(&self, element: &BytesStart<'_>) -> Option<&[u8]> {
        element
            .name()
            .prefix()
            .and_then(|prefix| self.uri(prefix.as_ref()))
    }
}

/// The settings kept as verbatim fragments.
#[derive(Clone, Copy)]
enum Fragment {
    /// `m:mathPr`.
    MathProperties,
    /// `w:shapeDefaults`.
    ShapeDefaults,
    /// `w:hdrShapeDefaults` — the same VML defaults, for headers and footers.
    HeaderShapeDefaults,
}

impl Fragment {
    /// Which fragment a top-level setting element is, if either. Matched by
    /// local name AND namespace, so an extension element sharing a local name
    /// is not mistaken for one.
    fn of(element: &BytesStart<'_>, bindings: &Bindings) -> Option<Self> {
        let uri = bindings.uri_of(element);
        match element.local_name().as_ref() {
            b"mathPr" if uri == Some(FRAGMENT_NAMESPACES[1].1) => Some(Self::MathProperties),
            b"shapeDefaults" if uri == Some(FRAGMENT_NAMESPACES[0].1) => Some(Self::ShapeDefaults),
            b"hdrShapeDefaults" if uri == Some(FRAGMENT_NAMESPACES[0].1) => {
                Some(Self::HeaderShapeDefaults)
            }
            _ => None,
        }
    }

    /// The element's local name, for the finding when it cannot be retained.
    const fn local(self) -> &'static [u8] {
        match self {
            Self::MathProperties => b"mathPr",
            Self::ShapeDefaults => b"shapeDefaults",
            Self::HeaderShapeDefaults => b"hdrShapeDefaults",
        }
    }
}

/// One verbatim fragment being captured: re-serialized event by event, as the
/// OMML capture in `body` does, so the bytes the writer emits are exactly the
/// events the source held.
struct Capture {
    kind: Fragment,
    writer: quick_xml::Writer<std::io::Cursor<Vec<u8>>>,
    /// Open elements inside the fragment, its own root included.
    nesting: u32,
    /// Whether every name in it resolves to a namespace the writer declares
    /// under the same prefix, and it stayed within the size bound.
    retainable: bool,
}

impl Capture {
    fn new(kind: Fragment) -> Self {
        Self {
            kind,
            writer: quick_xml::Writer::new(std::io::Cursor::new(Vec::new())),
            nesting: 1,
            retainable: true,
        }
    }

    /// Checks one element of the fragment: its own prefix and each attribute's
    /// must be one the writer declares, bound on the root (or on the element
    /// itself) to the same URI the writer will declare for it.
    fn check(&mut self, element: &BytesStart<'_>, bindings: &Bindings) {
        let declared_here: Vec<(Vec<u8>, Vec<u8>)> = element
            .attributes()
            .flatten()
            .filter_map(|attribute| {
                let key = attribute.key.as_ref();
                if key == b"xmlns" {
                    // A default namespace inside the fragment would rebind every
                    // unprefixed name beneath it; refuse rather than reason about it.
                    return Some((Vec::new(), Vec::new()));
                }
                let prefix = key.strip_prefix(b"xmlns:")?;
                Some((prefix.to_vec(), attribute.value.into_owned()))
            })
            .collect();
        let resolves = |prefix: &[u8]| {
            let Some((_, expected)) = FRAGMENT_NAMESPACES
                .iter()
                .find(|(allowed, _)| *allowed == prefix)
            else {
                return false;
            };
            let bound = declared_here
                .iter()
                .find(|(declared, _)| declared == prefix)
                .map(|(_, uri)| uri.as_slice())
                .or_else(|| bindings.uri(prefix));
            bound == Some(*expected)
        };
        let declarations_are_canonical = declared_here
            .iter()
            .all(|(prefix, uri)| !prefix.is_empty() && resolves(prefix) && !uri.is_empty());
        let element_resolves = element
            .name()
            .prefix()
            .is_some_and(|prefix| resolves(prefix.as_ref()));
        let attributes_resolve = element.attributes().flatten().all(|attribute| {
            let key = attribute.key;
            if key.as_ref() == b"xmlns" || key.as_ref().starts_with(b"xmlns:") {
                return true;
            }
            key.prefix().is_none_or(|prefix| resolves(prefix.as_ref()))
        });
        if !(declarations_are_canonical && element_resolves && attributes_resolve) {
            self.retainable = false;
        }
    }

    /// Re-serializes one event into the fragment, refusing it once it outgrows
    /// the model's bound.
    fn write(&mut self, event: &Event<'_>) {
        if !self.retainable {
            return;
        }
        if self.writer.write_event(event.borrow()).is_err()
            || self.writer.get_ref().get_ref().len()
                > casual_doc_model::v1::MAX_SETTINGS_FRAGMENT_BYTES
        {
            self.retainable = false;
        }
    }

    /// Stores the finished fragment, or reports the element as the unmodeled
    /// setting it was before it could be retained.
    fn finish(self, settings: &mut DocumentSettings, reporter: &mut Reporter) {
        let kind = self.kind;
        let fragment = self
            .retainable
            .then(|| String::from_utf8(self.writer.into_inner().into_inner()).ok())
            .flatten();
        let Some(fragment) = fragment else {
            reporter.report(kind.local());
            return;
        };
        match kind {
            Fragment::MathProperties => settings.math_properties_xml = Some(fragment),
            Fragment::ShapeDefaults => settings.shape_defaults_xml = Some(fragment),
            Fragment::HeaderShapeDefaults => settings.header_shape_defaults_xml = Some(fragment),
        }
    }
}

/// Where in the part an element's event fired: its nesting level, and the
/// container it sits in.
#[derive(Clone, Copy)]
struct Position {
    /// The root `w:settings` is level 0, a setting level 1, a child of
    /// `w:compat` or of a note container level 2.
    level: u64,
    /// Whether `w:compat` is open.
    in_compat: bool,
    /// Which document-default note container is open, if any.
    note_scope: Option<NoteScope>,
}

/// Which document-default note container is currently open.
#[derive(Clone, Copy)]
enum NoteScope {
    Footnote,
    Endnote,
}

/// Handles one element at its nesting `level`. Level 1 is a direct setting;
/// level 2 while `in_compat` is a `w:compat` child. Recognized settings mutate
/// `settings`; everything else is reported.
fn on_setting(
    position: Position,
    element: &BytesStart<'_>,
    self_closing: bool,
    bindings: &Bindings,
    templates: &BTreeMap<String, String>,
    settings: &mut DocumentSettings,
    reporter: &mut Reporter,
) {
    let Position {
        level,
        in_compat,
        note_scope,
    } = position;
    let local = element.local_name();
    let local = local.as_ref();
    if in_compat && level == 2 {
        match local {
            b"adjustLineHeightInTable" => {
                settings.adjust_line_height_in_table = on_off(element);
            }
            b"useFELayout" => settings.use_fe_layout = on_off(element),
            b"compatSetting" => {
                if !push_compat_setting(element, settings) {
                    reporter.report(local);
                }
            }
            _ => match core::str::from_utf8(local) {
                Ok(name) if DocumentSettings::is_compat_option(name) => {
                    if on_off(element) {
                        push_compat_option(name, settings);
                    }
                }
                _ => reporter.report_element(local, element, self_closing),
            },
        }
        return;
    }
    if let Some(scope) = note_scope
        && level == 2
    {
        if !apply_note_child(scope, local, element, settings) {
            reporter.report_element(local, element, self_closing);
        }
        return;
    }
    if level != 1 {
        // The root itself (level 0) and markup subsumed by an unmodeled setting.
        return;
    }
    // A self-closing `w:footnotePr`/`w:endnotePr` carries no children: recognized
    // but empty, so there is nothing to apply and nothing to report.
    if matches!(local, b"footnotePr" | b"endnotePr") {
        return;
    }
    if apply_setting(local, element, bindings, templates, settings) {
        report_unmodeled_attributes(reporter, local, element);
    } else {
        reporter.report_element(local, element, self_closing);
    }
}

/// A protection element's password verifier, verbatim (ADR-052, updated
/// 2026-10-09): every one of the sixteen `AG_Password` and
/// `AG_TransitionalPassword` attributes the element states, or `None` when it
/// states none.
///
/// Until 2026-10-09 these were reported and dropped, so a password-protected
/// restriction saved password-less — liftable in Word by anyone. They are now
/// kept and written back; nothing verifies them, and nothing claims the
/// restriction is a security boundary (the legacy hash is removable by editing
/// one attribute, and Word documents it as a deterrent). An empty value says
/// nothing and is skipped (a producer may write `w:hash=""`); an over-long one
/// is not stored and is reported by [`report_unmodeled_attributes`].
///
/// Complexity: O(16) attribute lookups on the one element.
fn password_verifier(element: &BytesStart<'_>) -> Option<PasswordVerifier> {
    let mut verifier = PasswordVerifier::default();
    for attribute in PasswordAttribute::ALL {
        if let Some(value) = attribute_value(element, attribute.local_name().as_bytes()) {
            verifier.set(attribute, value);
        }
    }
    (!verifier.is_empty()).then_some(verifier)
}

/// Reports the **attributes** of an otherwise-modeled settings element that the
/// model could not keep: an over-long `w:themeFontLang` language, and a password
/// attribute whose value is longer than [`PasswordVerifier::MAX_VALUE_LEN`].
///
/// This function exists because `apply_setting` returns *handled* for these
/// elements, and the catch-all at the end of [`on_setting`] fires only on the
/// `false` branch, so an attribute the model could not hold would otherwise be
/// dropped in silence: `word/settings.xml` is regenerated from the model on a
/// semantic save, and there is no byte floor behind it. The shape mirrors
/// `numbering.rs`'s `report_unmodeled_attributes` (`docs/142` LST-31).
///
/// Complexity: O(A) in the attributes of the one element being opened. No
/// document walk.
fn report_unmodeled_attributes(reporter: &mut Reporter, local: &[u8], element: &BytesStart<'_>) {
    if local == b"themeFontLang" {
        // A language the model's bound refuses is the one thing `w:themeFontLang`
        // can lose; an empty one says nothing and is not a loss.
        for attribute in THEME_FONT_LANGUAGE_ATTRIBUTES {
            if attribute_value(element, attribute).is_some_and(|value| value.len() > 255) {
                reporter.report_attribute(local, attribute);
            }
        }
        return;
    }
    if !matches!(local, b"documentProtection" | b"writeProtection") {
        return;
    }
    for attribute in PasswordAttribute::ALL {
        let name = attribute.local_name().as_bytes();
        if attribute_value(element, name)
            .is_some_and(|value| !value.is_empty() && !PasswordVerifier::is_storable(&value))
        {
            reporter.report_attribute(local, name);
        }
    }
}

/// Applies one child of an open `w:footnotePr`/`w:endnotePr` (the shared
/// `w:pos`/`w:numFmt`/`w:numStart`/`w:numRestart` vocabulary), returning whether it
/// was consumed.
fn apply_note_child(
    scope: NoteScope,
    local: &[u8],
    element: &BytesStart<'_>,
    settings: &mut DocumentSettings,
) -> bool {
    let props: &mut NoteProperties = match scope {
        NoteScope::Footnote => &mut settings.footnote_props,
        NoteScope::Endnote => &mut settings.endnote_props,
    };
    match local {
        b"pos" => match note_position(element) {
            Some(value) => props.position = Some(value),
            None => return false,
        },
        b"numFmt" => match crate::numbering::number_format(element) {
            Some(value) => props.number_format = Some(value),
            None => return false,
        },
        b"numStart" => match attribute_value(element, b"val").and_then(|v| v.parse::<i32>().ok()) {
            Some(value) => props.number_start = Some(value),
            None => return false,
        },
        b"numRestart" => match note_number_restart(element) {
            Some(value) => props.number_restart = Some(value),
            None => return false,
        },
        // `<w:footnote w:id="-1"/>` / `<w:endnote w:id="0"/>` inside the
        // document-default note properties is a REFERENCE to the stock separator
        // and continuation-separator notes by id. The notes themselves carry no
        // document meaning — this engine's layout draws the rule above the notes
        // for itself, so `body::close_note` skips them — and a pointer to
        // something that carries no meaning carries none either. Reporting it
        // described a loss that did not happen, twice per container across seven
        // documents of the owner's corpus (HF-174).
        b"footnote" | b"endnote" => {}
        _ => return false,
    }
    true
}

/// Maps `w:footnotePr`/`w:endnotePr` `w:pos/@w:val` to a placement.
fn note_position(element: &BytesStart<'_>) -> Option<NotePosition> {
    match attribute_value(element, b"val").as_deref() {
        Some("pageBottom") => Some(NotePosition::PageBottom),
        Some("beneathText") => Some(NotePosition::BeneathText),
        Some("sectEnd") => Some(NotePosition::SectionEnd),
        Some("docEnd") => Some(NotePosition::DocumentEnd),
        _ => None,
    }
}

/// Maps `w:numRestart/@w:val` to a restart policy.
fn note_number_restart(element: &BytesStart<'_>) -> Option<NoteNumberRestart> {
    match attribute_value(element, b"val").as_deref() {
        Some("continuous") => Some(NoteNumberRestart::Continuous),
        Some("eachSect") => Some(NoteNumberRestart::EachSection),
        Some("eachPage") => Some(NoteNumberRestart::EachPage),
        _ => None,
    }
}

/// Applies a recognized top-level setting, returning whether it was consumed.
fn apply_setting(
    local: &[u8],
    element: &BytesStart<'_>,
    bindings: &Bindings,
    templates: &BTreeMap<String, String>,
    settings: &mut DocumentSettings,
) -> bool {
    match local {
        b"embedTrueTypeFonts" => settings.embed_true_type_fonts = on_off(element),
        b"embedSystemFonts" => settings.embed_system_fonts = on_off(element),
        b"saveSubsetFonts" => settings.save_subset_fonts = on_off(element),
        b"evenAndOddHeaders" => settings.even_and_odd_headers = on_off(element),
        b"mirrorMargins" => settings.mirror_margins = on_off(element),
        // `w:trackRevisions` is the schema's element (ECMA-376 §17.15.1.89).
        // Only `trackChanges` — which is in no schema — was read until `109`
        // FID-AT-11, so a document Word saved with Track Changes on reported the
        // element and opened with it off. `trackChanges` stays readable, because
        // the writer emitted it until the same change.
        b"trackRevisions" | b"trackChanges" => settings.track_changes = on_off(element),
        b"updateFields" => settings.update_fields = on_off(element),
        b"defaultTabStop" => match tab_stop(element) {
            Some(value) => settings.default_tab_stop = Some(value),
            None => return false,
        },
        b"autoHyphenation" => settings.auto_hyphenation = on_off(element),
        b"doNotHyphenateCaps" => settings.do_not_hyphenate_caps = on_off(element),
        b"displayBackgroundShape" => settings.display_background_shape = on_off(element),
        // `w:hyphenationZone` is a non-negative twip measure (same bound as a tab
        // stop); `w:consecutiveHyphenLimit` is a non-negative count.
        b"hyphenationZone" => match bounded_int(element, 0..=31_680) {
            Some(value) => settings.hyphenation_zone = Some(value),
            None => return false,
        },
        b"consecutiveHyphenLimit" => match bounded_int(element, 0..=32_767) {
            Some(value) => settings.consecutive_hyphen_limit = Some(value),
            None => return false,
        },
        b"defaultTableStyle" => match table_style(element) {
            Some(value) => settings.default_table_style = Some(value),
            None => return false,
        },
        b"proofState" => {
            let spelling = proof_state(element, b"spelling");
            let grammar = proof_state(element, b"grammar");
            if spelling.is_none() && grammar.is_none() {
                return false;
            }
            settings.proof_state.spelling = spelling;
            settings.proof_state.grammar = grammar;
        }
        b"documentProtection" => {
            settings.document_protection = Some(DocumentProtection {
                edit: protection_edit(element),
                enforcement: enforcement(element),
                formatting: attr_flag(element, b"formatting"),
                password: password_verifier(element),
            });
        }
        b"writeProtection" => {
            settings.write_protection = Some(WriteProtection {
                recommended: attr_flag(element, b"recommended"),
                password: password_verifier(element),
            });
        }
        b"zoom" => {
            let zoom = Zoom {
                mode: zoom_mode(element),
                percent: zoom_percent(element),
            };
            if zoom.is_empty() {
                return false;
            }
            settings.zoom = zoom;
        }
        // An unknown `ST_View` token is not a view this model can carry, so it
        // falls to the catch-all and is reported rather than guessed at.
        b"view" => match document_view(element) {
            Some(view) => settings.view = Some(view),
            None => return false,
        },
        // Consumed whatever it says: every attribute it can carry is kept, an
        // empty one states nothing (LibreOffice writes all three empty), and an
        // over-long one is reported by `report_unmodeled_attributes`.
        b"themeFontLang" => settings.theme_font_languages = theme_font_languages(element),
        // `109` FID-AT-10: settings Word writes into every document it saves,
        // reported and dropped by every edited save until they were modelled.
        b"savePreviewPicture" => settings.save_preview_picture = on_off(element),
        b"doNotAutoCompressPictures" => settings.do_not_auto_compress_pictures = on_off(element),
        b"decimalSymbol" | b"listSeparator" => {
            let Some(token) = attribute_value(element, b"val")
                .filter(|value| DocumentSettings::is_valid_token(value))
            else {
                return false;
            };
            if local == b"decimalSymbol" {
                settings.decimal_symbol = Some(token);
            } else {
                settings.list_separator = Some(token);
            }
        }
        // `w14:docId` and `w15:docId` share a local name; the namespace the root
        // binds the element's prefix to decides which identity this is, and a
        // value of the wrong shape for it is reported rather than stored.
        b"docId" => {
            let Some(id) = attribute_value(element, b"val") else {
                return false;
            };
            match bindings.uri_of(element) {
                Some(W14_NAMESPACE) if DocumentSettings::is_valid_document_id_w14(&id) => {
                    settings.document_id_w14 = Some(id);
                }
                Some(W15_NAMESPACE) if DocumentSettings::is_valid_document_id_w15(&id) => {
                    settings.document_id_w15 = Some(id);
                }
                _ => return false,
            }
        }
        // Word's drawing grid (Layout ▸ Align ▸ Grid Settings), `109` FID-AT-15.
        b"drawingGridHorizontalSpacing"
        | b"drawingGridVerticalSpacing"
        | b"drawingGridHorizontalOrigin"
        | b"drawingGridVerticalOrigin" => {
            let Some(value) = bounded_int(element, 0..=31_680).and_then(|v| u32::try_from(v).ok())
            else {
                return false;
            };
            let grid = &mut settings.drawing_grid;
            *match local {
                b"drawingGridHorizontalSpacing" => &mut grid.horizontal_spacing,
                b"drawingGridVerticalSpacing" => &mut grid.vertical_spacing,
                b"drawingGridHorizontalOrigin" => &mut grid.horizontal_origin,
                _ => &mut grid.vertical_origin,
            } = Some(value);
        }
        b"displayHorizontalDrawingGridEvery" | b"displayVerticalDrawingGridEvery" => {
            let Some(value) = bounded_int(element, 0..=32_767).and_then(|v| u32::try_from(v).ok())
            else {
                return false;
            };
            if local == b"displayHorizontalDrawingGridEvery" {
                settings.drawing_grid.display_horizontal_every = Some(value);
            } else {
                settings.drawing_grid.display_vertical_every = Some(value);
            }
        }
        b"doNotUseMarginsForDrawingGridOrigin" => {
            settings.drawing_grid.do_not_use_margins_for_origin = on_off(element);
        }
        // The template's TARGET lives in `settings.xml.rels`; `parse` resolves
        // the id once the part has been read (`resolve_attached_template`).
        // The template's TARGET lives in `settings.xml.rels`, resolved here by
        // the element's `r:id`; an id the part's relationships do not name, or a
        // target over the bound, is reported rather than invented.
        b"attachedTemplate" => match attribute_value(element, b"id")
            .and_then(|id| templates.get(&id))
            .filter(|target| !target.is_empty() && target.len() <= MAX_ATTACHED_TEMPLATE_BYTES)
        {
            Some(target) => settings.attached_template = Some(target.clone()),
            None => return false,
        },
        b"defaultImageDpi" => match attribute_value(element, b"val")
            .and_then(|value| value.trim().parse::<u32>().ok())
            .filter(|dpi| DocumentSettings::is_valid_image_dpi(*dpi))
        {
            Some(dpi) => settings.default_image_dpi = Some(dpi),
            None => return false,
        },
        _ => return false,
    }
    true
}

/// `w:themeFontLang`'s three language attributes, in model field order.
const THEME_FONT_LANGUAGE_ATTRIBUTES: [&[u8]; 3] = [b"val", b"eastAsia", b"bidi"];

/// Reads `w:themeFontLang`: each attribute kept when non-empty and within the
/// model's 255-byte bound.
fn theme_font_languages(element: &BytesStart<'_>) -> ThemeFontLanguages {
    let [latin, east_asia, bidi] = THEME_FONT_LANGUAGE_ATTRIBUTES.map(|attribute| {
        attribute_value(element, attribute).filter(|value| !value.is_empty() && value.len() <= 255)
    });
    ThemeFontLanguages {
        latin,
        east_asia,
        bidi,
    }
}

/// Maps `w:view/@w:val` (`ST_View`) to a view.
fn document_view(element: &BytesStart<'_>) -> Option<DocumentView> {
    match attribute_value(element, b"val").as_deref() {
        Some("none") => Some(DocumentView::None),
        Some("print") => Some(DocumentView::Print),
        Some("outline") => Some(DocumentView::Outline),
        Some("masterPages") => Some(DocumentView::MasterPages),
        Some("normal") => Some(DocumentView::Normal),
        Some("web") => Some(DocumentView::Web),
        _ => None,
    }
}

/// Parses a `w:compatSetting` triple, returning whether it was well-formed and
/// pushed (name/uri present and bounded, val bounded).
fn push_compat_setting(element: &BytesStart<'_>, settings: &mut DocumentSettings) -> bool {
    let bounded = |name: &[u8]| attribute_value(element, name).filter(|v| v.len() <= 255);
    let (Some(name), Some(uri)) = (bounded(b"name"), bounded(b"uri")) else {
        return false;
    };
    if name.is_empty() || uri.is_empty() {
        return false;
    }
    let val = bounded(b"val").unwrap_or_default();
    settings.compat.push(CompatSetting { name, uri, val });
    true
}

/// Records a `w:compat` switch that is on, keeping
/// [`DocumentSettings::compat_options`] in schema order and free of repeats.
/// O(65).
fn push_compat_option(name: &str, settings: &mut DocumentSettings) {
    let rank = |option: &str| {
        LEGACY_COMPAT_OPTIONS
            .iter()
            .position(|known| *known == option)
    };
    if settings.compat_options.iter().any(|held| held == name) {
        return;
    }
    let at = settings
        .compat_options
        .iter()
        .position(|held| rank(held) > rank(name))
        .unwrap_or(settings.compat_options.len());
    settings.compat_options.insert(at, name.to_owned());
}

/// Reads an OOXML `CT_OnOff` element value: present means `true` unless its
/// `w:val` is one of the falsey tokens (`false`, `0`, `off`).
fn on_off(element: &BytesStart<'_>) -> bool {
    match attribute_value(element, b"val") {
        Some(value) => !matches!(value.as_str(), "false" | "0" | "off"),
        None => true,
    }
}

/// Reads an attribute-level `CT_OnOff` (e.g. `w:formatting`, `w:recommended`):
/// true only for an explicit truthy token; absent or falsey is `false`.
///
/// `w:enforcement` is deliberately **not** read through here — see
/// [`enforcement`], which has a different default and a documented reason for it.
fn attr_flag(element: &BytesStart<'_>, name: &[u8]) -> bool {
    matches!(
        attribute_value(element, name).as_deref(),
        Some("1" | "true" | "on")
    )
}

/// Reads `w:documentProtection/@w:enforcement`, whose **absence means enforced**.
///
/// The attribute has three states and they are not two:
///
/// | Source | Here |
/// | --- | --- |
/// | `w:enforcement="1"` (`true`/`on`) | enforced |
/// | `w:enforcement="0"` (`false`/`off`) | **not** enforced |
/// | the attribute is **absent** | **enforced** |
///
/// ECMA-376 says an omitted `w:enforcement` means the protection settings are
/// ignored. Word does the opposite, and Microsoft's own implementer notes say so:
///
/// > "The standard states that if the `enforcement` attribute is omitted, then
/// > protection settings are ignored by application. — **Word enforces protection
/// > when this attribute is missing.**"
/// > — MS-OI29500 Part 1 §17.15.1.29.
///
/// Word is the producer of essentially every protected document in the world, so
/// its behaviour is the compatible reading, and the two readings differ in the
/// unsafe direction: this parser used to go through [`attr_flag`], which treats
/// absent as `false`, so `<w:documentProtection w:edit="readOnly"/>` — a document
/// Word opens read-only — opened here **fully editable, with no banner and no
/// finding**. That is a document-safety defect, not a fidelity one, which is why
/// it does not wait on any decision about how protection is presented.
///
/// The `="0"` case is separate and was always right: Word writes an explicit zero
/// when an author set a restriction up and then switched it off, and
/// `casual-doc-edit`'s `protection.rs` relies on exactly that to leave such a
/// document editable. Keeping the two cases apart is the whole content of this
/// function, so it reuses `properties::is_true` for the token table rather than
/// spelling the truthy and falsey tokens a second time.
///
/// A present-but-unrecognised token (`w:enforcement="maybe"`) resolves to
/// enforced, with absent: the producer said something about enforcement that this
/// parser cannot read, and the document-safety direction is to honour the
/// restriction rather than to drop it.
fn enforcement(element: &BytesStart<'_>) -> bool {
    crate::properties::is_true(attribute_value(element, b"enforcement").as_deref())
}

/// The default tab stop in twips (`w:defaultTabStop/@w:val`), bounded 0..=31680.
fn tab_stop(element: &BytesStart<'_>) -> Option<i32> {
    attribute_value(element, b"val")
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|value| (0..=31_680).contains(value))
}

/// A `@w:val` decimal integer clamped to `range` (rejected — reported — otherwise).
fn bounded_int(element: &BytesStart<'_>, range: std::ops::RangeInclusive<i32>) -> Option<i32> {
    attribute_value(element, b"val")
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|value| range.contains(value))
}

/// The default table style name (`w:defaultTableStyle/@w:val`), non-empty and
/// bounded to 255 bytes.
fn table_style(element: &BytesStart<'_>) -> Option<String> {
    attribute_value(element, b"val").filter(|value| !value.is_empty() && value.len() <= 255)
}

/// A `w:proofState` dimension (`w:spelling`/`w:grammar`) mapped to its state.
fn proof_state(element: &BytesStart<'_>, name: &[u8]) -> Option<ProofState> {
    match attribute_value(element, name).as_deref() {
        Some("clean") => Some(ProofState::Clean),
        Some("dirty") => Some(ProofState::Dirty),
        _ => None,
    }
}

/// The `w:documentProtection/@w:edit` restriction (default `none`).
fn protection_edit(element: &BytesStart<'_>) -> DocumentProtectionEdit {
    match attribute_value(element, b"edit").as_deref() {
        Some("readOnly") => DocumentProtectionEdit::ReadOnly,
        Some("comments") => DocumentProtectionEdit::Comments,
        Some("trackedChanges") => DocumentProtectionEdit::TrackedChanges,
        Some("forms") => DocumentProtectionEdit::Forms,
        _ => DocumentProtectionEdit::None,
    }
}

/// The `w:zoom/@w:val` preset mode, if in the vocabulary.
fn zoom_mode(element: &BytesStart<'_>) -> Option<ZoomMode> {
    match attribute_value(element, b"val").as_deref() {
        Some("none") => Some(ZoomMode::None),
        Some("fullPage") => Some(ZoomMode::FullPage),
        Some("bestFit") => Some(ZoomMode::BestFit),
        Some("textFit") => Some(ZoomMode::TextFit),
        _ => None,
    }
}

/// The `w:zoom/@w:percent` magnification, bounded 1..=1000.
fn zoom_percent(element: &BytesStart<'_>) -> Option<u16> {
    attribute_value(element, b"percent")
        .and_then(|value| value.trim_end_matches('%').parse::<u16>().ok())
        .filter(|value| (1..=1_000).contains(value))
}

/// Counts an element against the configured ceiling.
fn bump(count: &mut u64, max: u64) -> Result<(), ImportError> {
    *count += 1;
    if *count > max {
        return Err(ImportError::LimitExceeded {
            limit: "xml_elements",
        });
    }
    Ok(())
}
