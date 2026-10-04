// SPDX-License-Identifier: Apache-2.0

//! The request/result types are extensible from OUTSIDE the crate.
//!
//! An integration test and not a `#[cfg(test)]` module, deliberately:
//! `#[non_exhaustive]` has no effect inside the defining crate, so a unit test
//! would be asserting the one place the attribute does nothing. This file is a
//! separate crate, which is the position every *host* adapter and
//! `casual-doc-wasm` are in.
//!
//! What this guards is the *defaults*, which is the half a compiler cannot check.
//! `#[non_exhaustive]` makes a literal impossible — that is the compiler's job and
//! a test asserting a compile failure would need `trybuild`, a dev-dependency this
//! change deliberately does not add. But a builder whose default silently differs
//! from the literal it replaced changes behaviour while every call site still
//! compiles, and that is exactly what the 58 mechanical rewrites in this commit
//! could have got wrong: `retain_source` costs a second copy of the whole input,
//! `source_unchanged` decides whether `ExactIfUnchanged` hands back the ORIGINAL
//! bytes, and `source` decides whether source-native opaque data is re-emitted.
//! Each of those defaults is asserted below, so a flipped default is a red test
//! rather than a silent change of what an export writes.

use casual_doc_io::{
    DocumentResources, ExportMode, ExportRequest, FormatImporter, ImportArtifact, ImportRequest,
    PlainTextAdapter, ProbeConfidence, ProbeRequest, ProbeResult,
};

/// One real document, imported through the public adapter boundary rather than
/// built by hand, so the fixture is reached the same way a caller reaches it.
fn imported() -> ImportArtifact {
    PlainTextAdapter::default()
        .import(ImportRequest::new(b"one paragraph"))
        .expect("plain text imports")
}

#[test]
fn an_import_request_does_not_retain_the_source_unless_asked() {
    let bytes = b"hello".as_slice();
    let request = ImportRequest::new(bytes);
    assert!(
        std::ptr::eq(request.bytes, bytes),
        "the request must borrow the caller's bytes, not a copy of them"
    );
    assert!(
        !request.retain_source,
        "retention costs a second copy of the whole input, so it is opt-in"
    );
    assert!(
        ImportRequest::new(bytes).retain_source(true).retain_source,
        "and asking for it must actually set it"
    );
    assert!(
        !ImportRequest::new(bytes)
            .retain_source(true)
            .retain_source(false)
            .retain_source,
        "the setter is a setter, not a latch: the last call wins"
    );
}

#[test]
fn an_export_request_defaults_to_semantic_with_no_preservation_state() {
    let artifact = imported();
    let document = &artifact.document;
    let resources = DocumentResources::default();
    let request = ExportRequest::new(document, &resources);
    assert_eq!(
        request.mode,
        ExportMode::Semantic,
        "the default must be the mode that writes no source-native opaque data"
    );
    assert!(
        request.source.is_none(),
        "no envelope was supplied, so none may be claimed"
    );
    assert!(
        !request.source_unchanged,
        "unchanged means ExactIfUnchanged may return the ORIGINAL bytes; defaulting \
         it true would hand back a stale file after an edit"
    );

    let changed = ExportRequest::new(document, &resources)
        .source_unchanged(true)
        .mode(ExportMode::ExactIfUnchanged);
    assert!(changed.source_unchanged);
    assert_eq!(changed.mode, ExportMode::ExactIfUnchanged);
    assert!(
        ExportRequest::new(document, &resources)
            .source(Some(&artifact.source))
            .source
            .is_some_and(|envelope| envelope.format() == &artifact.format.format),
        "the envelope the caller attached must be the one the adapter is handed"
    );
}

#[test]
fn a_probe_request_borrows_the_bytes_it_is_given() {
    let bytes = b"PK\x03\x04".as_slice();
    assert!(std::ptr::eq(ProbeRequest::new(bytes).bytes, bytes));
}

#[test]
fn the_three_probe_constructors_carry_their_confidence_and_evidence() {
    for (result, confidence) in [
        (ProbeResult::no_match("e.no"), ProbeConfidence::NoMatch),
        (ProbeResult::possible("e.maybe"), ProbeConfidence::Possible),
        (ProbeResult::definite("e.yes"), ProbeConfidence::Definite),
    ] {
        assert_eq!(result.confidence, confidence);
        assert!(
            result.evidence.starts_with("e."),
            "the evidence code must be carried through verbatim: {:?}",
            result.evidence
        );
    }
}
