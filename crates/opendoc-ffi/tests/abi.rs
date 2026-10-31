// SPDX-License-Identifier: Apache-2.0

//! The C ABI, called the way a C host calls it: through raw pointers, with
//! every returned handle and buffer freed exactly once.

use std::ffi::CString;

use opendoc_ffi::*;

const SAMPLE: &[u8] = include_bytes!("../../../sample.docx");

/// Takes a buffer's bytes and frees it.
fn take(buf: *mut OdBuffer) -> Vec<u8> {
    assert!(!buf.is_null(), "unexpected NULL: {}", last_error());
    // SAFETY: a live buffer from the library, freed once here.
    unsafe {
        let bytes =
            std::slice::from_raw_parts(opendoc_buffer_data(buf), opendoc_buffer_len(buf)).to_vec();
        opendoc_buffer_free(buf);
        bytes
    }
}

fn last_error() -> String {
    let buf = opendoc_last_error();
    if buf.is_null() {
        return String::new();
    }
    String::from_utf8(take(buf)).unwrap()
}

fn call(doc: *mut OdDocument, method: &str, arg: Option<&str>) -> serde_json::Value {
    let method = CString::new(method).unwrap();
    let arg = arg.map(|a| CString::new(a).unwrap());
    // SAFETY: a live handle and NUL-terminated strings.
    let buf = unsafe {
        opendoc_call(
            doc,
            method.as_ptr(),
            arg.as_ref().map_or(std::ptr::null(), |a| a.as_ptr()),
        )
    };
    serde_json::from_slice(&take(buf)).unwrap()
}

#[test]
fn open_read_edit_save_render_and_free() {
    assert_eq!(opendoc_abi_version(), ABI_VERSION);
    // SAFETY: SAMPLE is a live slice; options are NULL.
    let doc = unsafe { opendoc_open(SAMPLE.as_ptr(), SAMPLE.len(), std::ptr::null()) };
    assert!(!doc.is_null(), "{}", last_error());
    let body = call(doc, "body", None);
    assert!(body.as_array().unwrap().len() > 10);
    let report = call(doc, "importReport", None);
    assert_eq!(report["format"], "docx");
    let edited = call(
        doc,
        "edit",
        Some(r#"[{"op":"replaceAll","find":"the","replace":"THE"}]"#),
    );
    assert!(edited["results"][0].as_u64().unwrap() > 0);
    assert!(call(doc, "text", None).as_str().unwrap().contains("THE"));
    let format = CString::new("pdf").unwrap();
    // SAFETY: a live handle and NUL-terminated strings.
    let pdf = take(unsafe { opendoc_save(doc, format.as_ptr(), std::ptr::null()) });
    assert!(pdf.starts_with(b"%PDF-"));
    assert_eq!(call(doc, "lastSaveReport", None)["format"], "pdf");
    // SAFETY: a live handle.
    let png = take(unsafe { opendoc_render_png(doc, 0, 24.0) });
    assert!(png.starts_with(b"\x89PNG"));
    // SAFETY: freed once.
    unsafe { opendoc_free(doc) };
}

#[test]
fn errors_return_null_and_a_coded_message() {
    let junk = b"PK\x03\x04 not a package";
    // SAFETY: a live slice.
    let doc = unsafe { opendoc_open(junk.as_ptr(), junk.len(), std::ptr::null()) };
    assert!(doc.is_null());
    assert!(last_error().starts_with("open:"));
    assert_eq!(last_error(), "", "the message is cleared once read");
    // A null pointer with a length is refused, not read.
    // SAFETY: the function checks the pointer before reading.
    let doc = unsafe { opendoc_open(std::ptr::null(), 10, std::ptr::null()) };
    assert!(doc.is_null());
    assert!(last_error().starts_with("invalid-argument:"));
    let created = opendoc_create();
    let method = CString::new("nope").unwrap();
    // SAFETY: a live handle and NUL-terminated strings.
    let buf = unsafe { opendoc_call(created, method.as_ptr(), std::ptr::null()) };
    assert!(buf.is_null());
    assert!(last_error().contains("no method"));
    // SAFETY: freed once; NULL frees are no-ops.
    unsafe {
        opendoc_free(created);
        opendoc_free(std::ptr::null_mut());
        opendoc_buffer_free(std::ptr::null_mut());
    }
}

#[test]
fn convert_and_compare() {
    let to = CString::new("txt").unwrap();
    // SAFETY: a live slice and a NUL-terminated string.
    let text = take(unsafe { opendoc_convert(SAMPLE.as_ptr(), SAMPLE.len(), to.as_ptr()) });
    assert!(!text.is_empty());
    let spec =
        CString::new(r#"{"sections":[{"children":[{"type":"paragraph","text":"one"}]}]}"#).unwrap();
    let spec2 =
        CString::new(r#"{"sections":[{"children":[{"type":"paragraph","text":"two"}]}]}"#).unwrap();
    // SAFETY: NUL-terminated specs.
    let (a, b) = unsafe {
        (
            opendoc_from_spec(spec.as_ptr()),
            opendoc_from_spec(spec2.as_ptr()),
        )
    };
    // SAFETY: two live handles.
    let diff: serde_json::Value =
        serde_json::from_slice(&take(unsafe { opendoc_compare(a, b) })).unwrap();
    assert!(!diff["changes"].as_array().unwrap().is_empty());
    // SAFETY: freed once each.
    unsafe {
        opendoc_free(a);
        opendoc_free(b);
    }
}

#[test]
fn the_header_declares_every_export() {
    let source = include_str!("../src/lib.rs");
    let header = include_str!("../include/opendoc.h");
    let mut exports = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if let Some(rest) = line
            .strip_prefix("pub extern \"C\" fn ")
            .or_else(|| line.strip_prefix("pub unsafe extern \"C\" fn "))
        {
            exports.push(rest.split('(').next().unwrap().to_owned());
        }
    }
    assert!(exports.len() >= 15, "{exports:?}");
    for name in &exports {
        assert!(
            header.contains(&format!(" {name}(")) || header.contains(&format!("*{name}(")),
            "opendoc.h does not declare {name}"
        );
    }
    let declared = header.matches("opendoc_").count();
    assert!(declared >= exports.len());
    assert!(header.contains(&format!("#define OPENDOC_ABI_VERSION {ABI_VERSION}")));
}

#[test]
fn pictures_headers_notes_and_sections_are_edits_through_the_abi() {
    // docs/176 §12.12: the C ABI reaches the new edits through `edit`, the same
    // JSON vocabulary every binding sends.
    let png = opendoc::Document::create()
        .unwrap()
        .render_page_png(0, 4.0)
        .unwrap();
    let doc = opendoc_create();
    assert!(!doc.is_null(), "{}", last_error());
    let added = call(
        doc,
        "edit",
        Some(r#"[{"op":"insertParagraph","text":"Hello"}]"#),
    );
    let p = added["results"][0].as_str().unwrap().to_owned();
    let batch = serde_json::json!([
        {"op": "insertPicture", "paragraph": p, "offset": 5,
         "image": opendoc::base64::encode(&png), "widthPt": 40},
        {"op": "headerFooter", "section": 0, "region": "header"},
        {"op": "insertNote", "kind": "footnote", "paragraph": p, "offset": 1, "text": "n1"},
        {"op": "addSection", "start": "continuous"},
        {"op": "setPageSetup", "section": 1, "setup": {"orientation": "landscape"}}
    ]);
    let results = call(doc, "edit", Some(&batch.to_string()));
    let picture = results["results"][0].as_str().unwrap().to_owned();
    let header = results["results"][1].as_str().unwrap().to_owned();
    let follow = serde_json::json!([
        {"op": "insertText", "paragraph": header, "offset": 0, "text": "Top"},
        {"op": "setAltText", "picture": picture, "text": "Logo"}
    ]);
    call(doc, "edit", Some(&follow.to_string()));
    let sections = call(doc, "sections", None);
    assert_eq!(sections[1]["orientation"], "landscape");
    assert_eq!(sections[1]["start"], "continuous");
    assert_eq!(call(doc, "notes", None)[0]["text"], "n1");
    let headers = call(doc, "headersFooters", None);
    assert!(
        headers
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["text"] == "Top")
    );
    let paragraph = call(
        doc,
        "paragraph",
        Some(&serde_json::json!({ "id": p }).to_string()),
    );
    assert!(
        paragraph["runs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == picture.as_str() && r["image"]["description"] == "Logo")
    );
    // A refused batch is refused whole, with a coded error.
    let method = CString::new("edit").unwrap();
    let arg = CString::new(
        r#"[{"op":"addSection"},{"op":"setSectionStart","section":9,"start":"oddPage"}]"#,
    )
    .unwrap();
    // SAFETY: a live handle and NUL-terminated strings.
    let refused = unsafe { opendoc_call(doc, method.as_ptr(), arg.as_ptr()) };
    assert!(refused.is_null());
    assert!(last_error().contains("section 9"));
    assert_eq!(
        call(doc, "sections", None).as_array().unwrap().len(),
        2,
        "nothing applied"
    );
    // SAFETY: freed once.
    unsafe { opendoc_free(doc) };
}

#[test]
fn a_page_renders_as_svg_through_the_abi() {
    // SAFETY: SAMPLE is a live slice; options are NULL.
    let doc = unsafe { opendoc_open(SAMPLE.as_ptr(), SAMPLE.len(), std::ptr::null()) };
    assert!(!doc.is_null(), "{}", last_error());
    // SAFETY: a live handle.
    let svg = take(unsafe { opendoc_render_svg(doc, 0) });
    assert!(svg.starts_with(b"<svg "));
    // SAFETY: a live handle; a page past the end is a coded error, not a crash.
    let past = unsafe { opendoc_render_svg(doc, 999) };
    assert!(past.is_null());
    assert!(!last_error().is_empty());
    // SAFETY: freed once.
    unsafe { opendoc_free(doc) };
}

#[test]
fn stale_double_freed_and_foreign_handles_are_refused_not_dereferenced() {
    // docs/176 §12.14: the registry turns a use-after-free into a coded error.
    // Without it each line below is undefined behaviour; the fuzz target
    // `ffi_calls` drives these sequences at random.
    let doc = opendoc_create();
    assert!(!doc.is_null());
    let text = CString::new("text").unwrap();
    // SAFETY: each call is made through the ABI's own checks; the handle and
    // buffer may be stale on purpose, which is what is under test.
    unsafe {
        opendoc_free(doc);
        assert!(opendoc_call(doc, text.as_ptr(), std::ptr::null()).is_null());
        assert!(last_error().contains("not a live document handle"));
        opendoc_free(doc); // a double free: refused, not freed again
        assert!(last_error().contains("not live"));
        assert!(opendoc_render_svg(doc, 0).is_null());
        assert!(opendoc_compare(doc, doc).is_null());

        let buf = opendoc_version();
        assert!(opendoc_buffer_len(buf) > 0);
        opendoc_buffer_free(buf);
        assert_eq!(opendoc_buffer_len(buf), 0, "a freed buffer reads as empty");
        assert!(opendoc_buffer_data(buf).is_null());
        opendoc_buffer_free(buf); // a double free: a no-op

        // A buffer passed where a document belongs.
        let other = opendoc_version();
        assert!(opendoc_call(other.cast(), text.as_ptr(), std::ptr::null()).is_null());
        assert!(last_error().contains("not a live document handle"));
        opendoc_buffer_free(other);
    }
}

#[test]
fn the_abi_fuzz_target_is_armed_in_ci() {
    // SKILL §9.2: a gate the docs call CI-enforced must be seen armed. The
    // `ffi_calls` target exists in the fuzz workspace, drives this crate, and
    // CI's `fuzz-build` job both builds and runs it.
    let manifest = include_str!("../../../fuzz/Cargo.toml");
    let target = include_str!("../../../fuzz/fuzz_targets/ffi_calls.rs");
    let ci = include_str!("../../../.github/workflows/ci.yml");
    assert!(manifest.contains("name = \"ffi_calls\""));
    assert!(manifest.contains("opendoc-ffi = { path = \"../crates/opendoc-ffi\""));
    assert!(target.contains("opendoc_free(") && target.contains("opendoc_buffer_free("));
    let start = ci.find("\n  fuzz-build:").expect("the fuzz-build job");
    let end = ci[start + 1..]
        .find("\n  docs:")
        .map_or(ci.len(), |at| start + 1 + at);
    let job = &ci[start..end];
    assert!(
        job.contains("fuzz build ffi_calls"),
        "fuzz-build does not build ffi_calls"
    );
    assert!(
        job.contains("fuzz run ffi_calls"),
        "fuzz-build does not run ffi_calls"
    );
}
