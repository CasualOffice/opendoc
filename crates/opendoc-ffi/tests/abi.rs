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
