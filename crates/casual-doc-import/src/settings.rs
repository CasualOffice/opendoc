//! Settings-part parsing: `word/settings.xml` -> v1 `DocumentSettings`.
//!
//! The load-bearing settings are modeled (font-embedding flags, header parity,
//! default tab stop, revision tracking, proof state, document/write protection,
//! default table style, view, zoom, the theme font languages, and the
//! `w:compatSetting` triples). Every OTHER
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
use casual_doc_model::v1::{DocumentView, ThemeFontLanguages};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::attribute_value;
use crate::report::Reporter;

/// Parses the settings part, returning the modeled subset (default when none of
/// the recognized settings appear). Every unmodeled top-level setting — and every
/// non-`compatSetting` child of `w:compat` — is reported.
pub(crate) fn parse(
    xml: &[u8],
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

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| ImportError::MalformedXml)?;
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
                match (level, local.as_ref()) {
                    (1, b"compat") => in_compat = true,
                    (1, b"footnotePr") => note_scope = Some(NoteScope::Footnote),
                    (1, b"endnotePr") => note_scope = Some(NoteScope::Endnote),
                    _ => on_setting(
                        level,
                        in_compat,
                        note_scope,
                        &element,
                        false,
                        &mut settings,
                        reporter,
                    ),
                }
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                on_setting(
                    depth,
                    in_compat,
                    note_scope,
                    &element,
                    true,
                    &mut settings,
                    reporter,
                );
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
    level: u64,
    in_compat: bool,
    note_scope: Option<NoteScope>,
    element: &BytesStart<'_>,
    self_closing: bool,
    settings: &mut DocumentSettings,
    reporter: &mut Reporter,
) {
    let local = element.local_name();
    let local = local.as_ref();
    if in_compat && level == 2 {
        match local {
            b"adjustLineHeightInTable" => {
                settings.adjust_line_height_in_table = on_off(element);
            }
            b"compatSetting" => {
                if !push_compat_setting(element, settings) {
                    reporter.report(local);
                }
            }
            _ => reporter.report_element(local, element, self_closing),
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
    if apply_setting(local, element, settings) {
        report_unmodeled_attributes(reporter, local, element);
    } else {
        reporter.report_element(local, element, self_closing);
    }
}

/// The `AG_Password` and `AG_TransitionalPassword` attribute groups, which
/// `w:documentProtection` (`CT_DocProtect`) and `w:writeProtection`
/// (`CT_WriteProtection`) both carry and this model represents nowhere.
///
/// Sorted by name, so the two groups are interleaved rather than in blocks.
/// `AG_Password` is the legacy twelve Word has always written (`w:hash`,
/// `w:salt`, `w:cryptProviderType`, `w:cryptAlgorithmClass`,
/// `w:cryptAlgorithmType`, `w:cryptAlgorithmSid`, `w:cryptSpinCount`,
/// `w:cryptProvider`, `w:algIdExt`, `w:algIdExtSource`,
/// `w:cryptProviderTypeExt`, `w:cryptProviderTypeExtSource`).
/// `AG_TransitionalPassword` is the Office-2010 ISO verifier form Word writes
/// *instead* when `UseIsoPasswordVerifier` is set — `w:algorithmName`,
/// `w:hashValue`, `w:saltValue`, `w:spinCount` — so a modern file's password
/// material may be entirely in those four. ADR-052 and `docs/160` §7 item 5 each
/// enumerated only the legacy five or seven until this landed; both now name the
/// sixteen.
const PASSWORD_ATTRIBUTES: &[&[u8]] = &[
    b"algIdExt",
    b"algIdExtSource",
    b"algorithmName",
    b"cryptAlgorithmClass",
    b"cryptAlgorithmSid",
    b"cryptAlgorithmType",
    b"cryptProvider",
    b"cryptProviderType",
    b"cryptProviderTypeExt",
    b"cryptProviderTypeExtSource",
    b"cryptSpinCount",
    b"hash",
    b"hashValue",
    b"salt",
    b"saltValue",
    b"spinCount",
];

/// Reports the **attributes** of an otherwise-modeled settings element whose
/// meaning the model does not carry — today, exactly the password groups on the
/// two protection elements.
///
/// This function exists because `apply_setting` returns *handled* for
/// `w:documentProtection` and `w:writeProtection`, and the catch-all at the end
/// of [`on_setting`] fires only on the `false` branch. "Handled" marked the whole
/// element consumed, so its unread attributes fell through the only reporter in
/// reach and were dropped **in total silence**: `word/settings.xml` is a consumed
/// part that the semantic writer regenerates from the model, and retained parts
/// are extra opaque parts rather than an override for a generated one, so there
/// is no byte floor behind these either. A password-protected document therefore
/// saved password-less while the restriction survived, and nothing anywhere said
/// so. That is `AGENTS.md`'s no-silent-data-loss rule, and `SKILL.md` §1
/// advantage 2 — verbatim retention is only an advantage if the loss is
/// *reported*.
///
/// It is a report rather than a round trip on purpose. ADR-052 decided opendoc
/// will **not** verify password material as a security boundary (the legacy hash
/// is removable by editing one attribute, and Word documents it as a deterrent),
/// and re-emitting a hash the engine cannot verify is a separate decision an
/// owner has to make. Reporting it needs no decision at all.
///
/// The shape mirrors `numbering.rs`'s `report_unmodeled_attributes` (`docs/142`
/// LST-31), which exists for the same reason in the same position: a parser that
/// matches on element names cannot see an attribute on an element it recognises.
///
/// Empty values are skipped. `docs/160` §3's reduction is that an attribute whose
/// value says nothing is not a loss — a producer may legitimately write
/// `w:hash=""` — and a false finding is the failure mode HF-174 put 621 of in
/// front of the owner.
///
/// Complexity: O(A) in the attributes of the one element being opened, with a
/// 16-name comparison each, and there is at most one of each protection element
/// per document. No document walk.
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
    for attribute in PASSWORD_ATTRIBUTES {
        if attribute_value(element, attribute).is_some_and(|value| !value.is_empty()) {
            reporter.report_attribute(local, attribute);
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
fn apply_setting(local: &[u8], element: &BytesStart<'_>, settings: &mut DocumentSettings) -> bool {
    match local {
        b"embedTrueTypeFonts" => settings.embed_true_type_fonts = on_off(element),
        b"embedSystemFonts" => settings.embed_system_fonts = on_off(element),
        b"saveSubsetFonts" => settings.save_subset_fonts = on_off(element),
        b"evenAndOddHeaders" => settings.even_and_odd_headers = on_off(element),
        b"mirrorMargins" => settings.mirror_margins = on_off(element),
        b"trackChanges" => settings.track_changes = on_off(element),
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
            });
        }
        b"writeProtection" => {
            settings.write_protection = Some(WriteProtection {
                recommended: attr_flag(element, b"recommended"),
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
