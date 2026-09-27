//! Cross-language parity: this crate's host vocabulary against the editor's.
//!
//! `docs/126` phase 2's rule is "one schema, two transports", and the reason it is
//! a rule is that this repository has the receipts: the `shortcut:` labels and the
//! key bindings were two tables that disagreed until `109` UX-006 made them one.
//! A THIRD copy of the vocabulary — a Rust one that nothing checks — would be the
//! same defect in a new language, which is why `host.rs` exists together with this
//! file and not without it.
//!
//! So this reads `webapp/src/host_contract.mjs`, the file both JavaScript
//! transports are built from, and compares it with [`crate::host`] in BOTH
//! directions: a name in one and not the other fails, whichever side added it.
//! It runs in `cargo test`, which is a CI gate, so the drift is a failed build
//! rather than a discovery.
//!
//! It parses rather than executes, because the alternative is a JavaScript runtime
//! in a Rust test. That parse is a real dependency and it is guarded from the other
//! side too: `webapp/tests/host_contract.test.mjs` asserts every event name and
//! refusal code stays in the line-oriented form this parser reads, so a reshuffle
//! that hid them from this file fails there instead of passing silently here.

use std::fs;
use std::path::PathBuf;

use crate::host::{HOST_CONTRACT_VERSION, HostEvent, HostRefusal, HostRequest};

/// The editor's schema file. Absent is a FAILURE and never a skip: a parity test
/// that silently passes when it cannot find the other side is worse than no test,
/// because it gets cited as evidence (`docs/105` CQ-003).
fn contract_source() -> String {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "..",
        "webapp",
        "src",
        "host_contract.mjs",
    ]
    .iter()
    .collect();
    fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "the host contract at {} could not be read ({err}). This test compares this \
             crate's host vocabulary with the editor's; it must fail rather than skip.",
            path.display()
        )
    })
}

/// The text of a `Object.freeze([ … ]);` declaration introduced by `marker`.
///
/// Bounded by the first `]);` at the start of a line, and then CHECKED: if the
/// span swallowed another `export`, the closer was indented or missing and this
/// says so, rather than reporting the next declaration's contents as if they
/// belonged to this one. That is not hypothetical — indenting the `]);` of
/// `REFUSAL_CODES` by two spaces made the refusal scan return the whole event
/// table, which fails, but for a reason nobody would have understood.
fn declaration<'a>(source: &'a str, marker: &str) -> &'a str {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("the host contract no longer declares `{marker}`"));
    let rest = &source[start + marker.len()..];
    let end = rest
        .find("\n]);")
        .unwrap_or_else(|| panic!("`{marker}` has no closing `]);` at the start of a line"));
    let body = &rest[..end];
    assert!(
        !body.contains("\nexport "),
        "`{marker}` runs past the next `export` before any `]);` at the start of a line, so its \
         closing bracket is indented or missing. This parser is line-oriented on purpose \
         (webapp/tests/host_contract.test.mjs pins the form); put the `]);` back at column zero."
    );
    body
}

/// Every double-quoted string in `text`, in order.
///
/// A scanner and not a parser, deliberately: the inputs are a frozen array of
/// literals and a list of `name: "…"` fields, and a JavaScript parser in a Rust
/// test would be a second implementation of JavaScript to maintain.
fn quoted(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut chars = text.char_indices();
    while let Some((index, character)) = chars.next() {
        if character != '"' {
            continue;
        }
        let rest = &text[index + 1..];
        let Some(close) = rest.find('"') else { break };
        found.push(rest[..close].to_owned());
        // Resume after the closing quote.
        for (next, _) in chars.by_ref() {
            if next >= index + 1 + close {
                break;
            }
        }
    }
    found
}

/// The `name: "…"` values inside a declaration.
fn field_values(text: &str, field: &str) -> Vec<String> {
    let needle = format!("{field}: \"");
    text.match_indices(&needle)
        .filter_map(|(at, _)| {
            let rest = &text[at + needle.len()..];
            rest.find('"').map(|end| rest[..end].to_owned())
        })
        .collect()
}

/// Reports both directions of a set difference, so neither side is privileged.
fn assert_same(kind: &str, rust: &[&str], editor: &[String]) {
    let editor_refs: Vec<&str> = editor.iter().map(String::as_str).collect();
    let missing_in_rust: Vec<&&str> = editor_refs
        .iter()
        .filter(|name| !rust.contains(name))
        .collect();
    let missing_in_editor: Vec<&&str> = rust
        .iter()
        .filter(|name| !editor_refs.contains(name))
        .collect();
    assert!(
        missing_in_rust.is_empty(),
        "the editor declares {kind} this crate does not: {missing_in_rust:?}. Add them to \
         casual-doc-sdk's host module — one vocabulary, or the two drift."
    );
    assert!(
        missing_in_editor.is_empty(),
        "this crate declares {kind} the editor does not: {missing_in_editor:?}. Add them to \
         webapp/src/host_contract.mjs, or take them out of here."
    );
    assert_eq!(
        rust, editor_refs,
        "the {kind} agree but their ORDER differs. The order is part of the contract: it is what \
         the generated documentation tables and the `ALL` arrays publish."
    );
}

#[test]
fn the_editor_and_this_crate_declare_the_same_host_events() {
    let source = contract_source();
    let declared = field_values(
        declaration(&source, "export const HOST_EVENTS = Object.freeze(["),
        "name",
    );
    let rust: Vec<&str> = HostEvent::ALL.iter().map(|event| event.as_str()).collect();
    assert_same("host events", &rust, &declared);
}

#[test]
fn the_editor_and_this_crate_declare_the_same_refusal_codes() {
    let source = contract_source();
    let declared = quoted(declaration(
        &source,
        "export const REFUSAL_CODES = Object.freeze([",
    ));
    let rust: Vec<&str> = HostRefusal::ALL
        .iter()
        .map(|refusal| refusal.as_str())
        .collect();
    assert_same("refusal codes", &rust, &declared);
}

#[test]
fn the_editor_and_this_crate_declare_the_same_request_verbs() {
    let source = contract_source();
    let line = source
        .lines()
        .find(|line| line.contains("requests: Object.freeze(["))
        .expect("the host contract no longer declares the protocol's request verbs");
    let declared = quoted(line);
    let rust: Vec<&str> = HostRequest::ALL
        .iter()
        .map(|request| request.as_str())
        .collect();
    assert_same("request verbs", &rust, &declared);
}

#[test]
fn the_editor_and_this_crate_agree_on_the_contract_version() {
    let source = contract_source();
    let marker = "export const CONTRACT_VERSION = ";
    let start = source
        .find(marker)
        .expect("the host contract no longer declares CONTRACT_VERSION");
    let rest = &source[start + marker.len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let declared: u32 = digits
        .parse()
        .expect("CONTRACT_VERSION is not a plain integer this test can read");
    assert_eq!(
        declared, HOST_CONTRACT_VERSION,
        "the editor is on contract version {declared} and this crate declares \
         {HOST_CONTRACT_VERSION}. A host pins against one number, so there is one number."
    );
}

#[test]
fn a_runtime_event_maps_onto_a_host_event_rather_than_a_new_name() {
    use std::collections::BTreeSet;

    use crate::config::{Engine, EngineConfig};
    use crate::selection::Affinity;
    use crate::snapshot::BlockSnapshot;

    // `docs/125` §8: host events are DERIVED from `RuntimeEvent`, not invented
    // beside it. Driven through a real session rather than hand-built values, so
    // the events mapped here are the ones the journal actually produces — a
    // hand-built variant would prove the `match` arms exist and nothing else.
    let engine = Engine::new(EngineConfig::default()).expect("an engine");
    let session = engine.create_blank().expect("a blank document");
    let blank = session.snapshot().expect("a snapshot");
    let BlockSnapshot::Paragraph(paragraph) = &blank.body[0];
    let mut subscription = session.subscribe().expect("a subscription");
    session
        .insert_text(crate::command::InsertTextRequest {
            base_revision: blank.revision,
            at: crate::selection::Position {
                node: paragraph.id.clone(),
                grapheme_offset: 0,
                affinity: Affinity::After,
            },
            text: "A".to_owned(),
            marks: BTreeSet::new(),
        })
        .expect("the insertion commits");
    let batch = subscription.drain(8).expect("the journal drains");
    let mapped: Vec<HostEvent> = batch
        .events
        .iter()
        .map(|sequenced| HostEvent::from(&sequenced.event))
        .collect();
    // One insertion produces a commit and a selection mapping, and a host hears
    // exactly `change` then `selection` — the two names the editor emits from its
    // own `noteChange` and `noteSelection` hooks.
    assert_eq!(
        mapped,
        vec![HostEvent::Change, HostEvent::Selection],
        "an insertion must reach a host as change then selection"
    );
    let distinct: BTreeSet<HostEvent> = mapped.into_iter().collect();
    assert_eq!(
        distinct.len(),
        2,
        "the two runtime variants must not collapse into one host event"
    );
}

#[test]
fn every_error_code_reaches_exactly_one_refusal_a_host_can_branch_on() {
    use crate::error::ErrorCode;

    // The whole point of `for_error` is that a native host and a browser host read
    // the SAME refusal vocabulary. So every code maps, and the mapping is checked
    // against the taxonomy rather than against itself: a `for_error` that returned
    // a code the editor does not declare would pass a self-comparison.
    let declared: Vec<&str> = HostRefusal::ALL
        .iter()
        .map(|refusal| refusal.as_str())
        .collect();
    let codes = [
        ErrorCode::InvalidArgument,
        ErrorCode::InvalidConfiguration,
        ErrorCode::MalformedDocument,
        ErrorCode::ResourceLimit,
        ErrorCode::StaleRevision,
        ErrorCode::InvalidPosition,
        ErrorCode::EmptyTransaction,
        ErrorCode::InvalidTextInput,
        ErrorCode::InvariantViolation,
        ErrorCode::HistoryEmpty,
        ErrorCode::Internal,
    ];
    for code in codes {
        let refusal = HostRefusal::for_error(code);
        assert!(
            declared.contains(&refusal.as_str()),
            "{} maps to {:?}, which is not a refusal the contract declares",
            code.as_str(),
            refusal
        );
    }
    // A bad argument never reached the document, so it is not an engine refusal —
    // the distinction a host acts on, and the one a single blanket mapping loses.
    assert_eq!(
        HostRefusal::for_error(ErrorCode::InvalidArgument),
        HostRefusal::BadRequest
    );
    assert_eq!(
        HostRefusal::for_error(ErrorCode::StaleRevision),
        HostRefusal::EngineRefused
    );
    assert_eq!(
        HostRefusal::for_error(ErrorCode::Internal),
        HostRefusal::Threw
    );
}
