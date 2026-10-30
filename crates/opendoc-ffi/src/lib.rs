// SPDX-License-Identifier: Apache-2.0

//! The OpenDoc Native SDK's C ABI (`docs/176` §6, §12.9).
//!
//! **Ownership, the one rule.** Every handle (`OdDocument *`) and every buffer
//! (`OdBuffer *`) this library returns is owned by the caller, who frees it
//! exactly once: `opendoc_free` for a document, `opendoc_buffer_free` for a
//! buffer. Inputs are borrowed for the duration of the call and copied before
//! it returns; nothing keeps a caller's pointer.
//!
//! **Errors.** A function that fails returns `NULL` (or `-1` for a count) and
//! records a message on the calling thread, which `opendoc_last_error` returns
//! as a buffer. The message starts with the SDK's stable code (`open: …`).
//!
//! **Data.** Structured values cross as UTF-8 JSON in buffers, the same JSON
//! the Rust read view serialises and the JavaScript and Python bindings read.
//! Bytes (documents, PNG, PDF) cross as buffers. Strings in are NUL-terminated
//! UTF-8.
//!
//! **Panics** never cross the boundary: each export catches an unwind and turns
//! it into an `engine` error.
//!
//! The ABI version (`opendoc_abi_version`) changes only when a symbol's
//! signature or ownership rule changes (`docs/176` §5.3); a binding refuses a
//! library whose version it was not written for.

use std::cell::RefCell;
use std::ffi::{CStr, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};

use opendoc::{
    Document, EditCommand, Error, Format, OpenOptions, RenderOptions, SaveMode, TemplateOptions,
    Undo,
};
use serde_json::{Value, json};

/// The ABI version. Bump on any signature or ownership change.
pub const ABI_VERSION: u32 = 1;

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn set_error(message: String) {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = Some(message));
}

/// An open document behind an opaque handle.
#[derive(Debug)]
pub struct OdDocument {
    doc: Document,
    last_report: String,
}

/// An owned byte buffer.
#[derive(Debug)]
pub struct OdBuffer {
    bytes: Vec<u8>,
}

fn buffer(bytes: Vec<u8>) -> *mut OdBuffer {
    Box::into_raw(Box::new(OdBuffer { bytes }))
}

/// Runs `body`, turning an error or a panic into `NULL` plus a recorded message.
fn guard<T>(body: impl FnOnce() -> Result<*mut T, String>) -> *mut T {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(pointer)) => pointer,
        Ok(Err(message)) => {
            set_error(message);
            std::ptr::null_mut()
        }
        Err(_) => {
            set_error("engine: the engine panicked; this is a defect, please report it".to_owned());
            std::ptr::null_mut()
        }
    }
}

fn sdk(error: Error) -> String {
    error.to_string()
}

/// # Safety
///
/// `data` is null with `len == 0`, or points to `len` readable bytes.
unsafe fn bytes_in<'a>(data: *const u8, len: usize) -> Result<&'a [u8], String> {
    if data.is_null() {
        return if len == 0 {
            Ok(&[])
        } else {
            Err("invalid-argument: a null pointer with a non-zero length".to_owned())
        };
    }
    // SAFETY: the caller promises `len` readable bytes at `data`.
    Ok(unsafe { std::slice::from_raw_parts(data, len) })
}

/// # Safety
///
/// `text` is null or a NUL-terminated string.
unsafe fn str_in(text: *const c_char) -> Result<Option<String>, String> {
    if text.is_null() {
        return Ok(None);
    }
    // SAFETY: the caller promises a NUL-terminated string.
    let raw = unsafe { CStr::from_ptr(text) };
    raw.to_str()
        .map(|s| Some(s.to_owned()))
        .map_err(|_| "invalid-argument: a string is not UTF-8".to_owned())
}

/// # Safety
///
/// `doc` is null or a live handle from this library.
unsafe fn doc_mut<'a>(doc: *mut OdDocument) -> Result<&'a mut OdDocument, String> {
    if doc.is_null() {
        return Err("invalid-argument: a null document handle".to_owned());
    }
    // SAFETY: the caller promises a live, unaliased handle.
    Ok(unsafe { &mut *doc })
}

fn handle(doc: Document) -> *mut OdDocument {
    Box::into_raw(Box::new(OdDocument {
        doc,
        last_report: "null".to_owned(),
    }))
}

/// The ABI version this library implements.
#[unsafe(no_mangle)]
pub extern "C" fn opendoc_abi_version() -> u32 {
    ABI_VERSION
}

/// The engine version, as a buffer of UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn opendoc_version() -> *mut OdBuffer {
    buffer(opendoc::VERSION.as_bytes().to_vec())
}

/// The calling thread's last error message, or `NULL` when there is none. The
/// message is cleared by the call.
#[unsafe(no_mangle)]
pub extern "C" fn opendoc_last_error() -> *mut OdBuffer {
    LAST_ERROR
        .with(|slot| slot.borrow_mut().take())
        .map_or(std::ptr::null_mut(), |m| buffer(m.into_bytes()))
}

/// Opens a document. `options_json` may be `NULL`, or
/// `{"format": "docx", "fileName": "...", "author": "...", "systemFonts": false}`.
///
/// # Safety
///
/// `data` points to `len` readable bytes (or is null with `len == 0`);
/// `options_json` is null or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendoc_open(
    data: *const u8,
    len: usize,
    options_json: *const c_char,
) -> *mut OdDocument {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let bytes = unsafe { bytes_in(data, len) }?;
        // SAFETY: forwarded caller contract.
        let options: Value = match unsafe { str_in(options_json) }? {
            Some(text) => serde_json::from_str(&text)
                .map_err(|e| format!("invalid-argument: options are not JSON: {e}"))?,
            None => Value::Null,
        };
        let format = options["format"]
            .as_str()
            .map(str::parse::<Format>)
            .transpose()
            .map_err(sdk)?;
        let open = OpenOptions {
            format,
            file_name: options["fileName"].as_str().map(str::to_owned),
            author: options["author"].as_str().map(str::to_owned),
            render: RenderOptions {
                system_fonts: options["systemFonts"].as_bool().unwrap_or(false),
                fonts: Vec::new(),
            },
        };
        Document::open_with(bytes, &open).map(handle).map_err(sdk)
    })
}

/// A blank document with Word's built-in styles.
#[unsafe(no_mangle)]
pub extern "C" fn opendoc_create() -> *mut OdDocument {
    guard(|| Document::create().map(handle).map_err(sdk))
}

/// Builds a document from a docx-js-shaped spec (images as base64 `data`).
///
/// # Safety
///
/// `spec_json` is NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendoc_from_spec(spec_json: *const c_char) -> *mut OdDocument {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let spec = unsafe { str_in(spec_json) }?.unwrap_or_else(|| "{}".to_owned());
        Document::from_spec_json(&spec, &[])
            .map(handle)
            .map_err(sdk)
    })
}

/// Frees a document handle. `NULL` is a no-op.
///
/// # Safety
///
/// `doc` is null or a handle from this library not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendoc_free(doc: *mut OdDocument) {
    if !doc.is_null() {
        // SAFETY: the caller promises a live handle, freed once.
        drop(unsafe { Box::from_raw(doc) });
    }
}

fn to<T: serde::Serialize>(value: &T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| format!("engine: {e}"))
}

fn call(od: &mut OdDocument, method: &str, arg: &Value) -> Result<Value, String> {
    let doc = &mut od.doc;
    let s = |key: &str| arg[key].as_str().unwrap_or_default().to_owned();
    Ok(match method {
        "body" => to(&doc.body())?,
        "paragraph" => to(&doc.paragraph(&s("id")))?,
        "table" => to(&doc.table(&s("id")))?,
        "text" => Value::String(doc.text()),
        "sections" => to(&doc.sections())?,
        "styles" => to(&doc.styles())?,
        "comments" => to(&doc.comments())?,
        "revisions" => to(&doc.revisions())?,
        "headersFooters" => to(&doc.headers_footers())?,
        "notes" => to(&doc.notes())?,
        "media" => to(&doc.media())?,
        "properties" => to(&doc.properties())?,
        "importReport" => to(doc.import_report())?,
        "sourceFormat" => to(&doc.source_format().map(Format::name))?,
        "isModified" => Value::Bool(doc.is_modified()),
        "setAuthor" => {
            doc.set_author(s("author"));
            Value::Null
        }
        "edit" => {
            let commands: Vec<EditCommand> = serde_json::from_value(arg.clone())
                .map_err(|e| format!("invalid-argument: not a list of edits: {e}"))?;
            let (results, undo) = doc.edit_commands(&commands).map_err(sdk)?;
            json!({ "results": results, "undo": undo.to_json() })
        }
        "undo" => {
            let undo = Undo::from_json(&s("undo")).map_err(sdk)?;
            Value::String(doc.undo(&undo).map_err(sdk)?.to_json())
        }
        "renderTemplate" => {
            let options = TemplateOptions {
                start: arg["start"].as_str().unwrap_or("{").to_owned(),
                end: arg["end"].as_str().unwrap_or("}").to_owned(),
                null_value: s("nullValue"),
                strict: arg["strict"].as_bool().unwrap_or(false),
            };
            doc.render_template(&arg["data"], &options).map_err(sdk)?;
            Value::Null
        }
        "templateTags" => {
            let options = TemplateOptions {
                start: arg["start"].as_str().unwrap_or("{").to_owned(),
                end: arg["end"].as_str().unwrap_or("}").to_owned(),
                ..TemplateOptions::default()
            };
            to(&doc.template_tags(&options))?
        }
        "pageCount" => Value::from(doc.page_count()),
        "pageSizes" => to(&doc
            .page_sizes()
            .iter()
            .map(|p| json!({"widthPt": p.width_pt, "heightPt": p.height_pt}))
            .collect::<Vec<_>>())?,
        "pageText" => {
            let index = usize::try_from(arg["index"].as_u64().unwrap_or(0)).unwrap_or(usize::MAX);
            Value::String(doc.page_text(index).map_err(sdk)?)
        }
        "outline" => to(&doc.outline())?,
        "fontNeeds" => to(&doc.font_needs())?,
        "lastSaveReport" => serde_json::from_str(&od.last_report).unwrap_or(Value::Null),
        other => return Err(format!("invalid-argument: no method \"{other}\"")),
    })
}

/// Calls a read or edit method by name with a JSON argument (`NULL` for none)
/// and returns its JSON result. Methods: `body`, `paragraph {id}`,
/// `table {id}`, `text`, `sections`, `styles`, `comments`, `revisions`,
/// `headersFooters`, `notes`, `media`, `properties`, `importReport`,
/// `sourceFormat`, `isModified`, `setAuthor {author}`, `edit [commands]`,
/// `undo {undo}`, `renderTemplate {data, start, end, nullValue, strict}`,
/// `templateTags {start, end}`, `pageCount`, `pageSizes`, `pageText {index}`,
/// `outline`, `fontNeeds`, `lastSaveReport`.
///
/// # Safety
///
/// `doc` is a live handle; `method` is NUL-terminated; `arg_json` is null or
/// NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendoc_call(
    doc: *mut OdDocument,
    method: *const c_char,
    arg_json: *const c_char,
) -> *mut OdBuffer {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let od = unsafe { doc_mut(doc) }?;
        // SAFETY: forwarded caller contract.
        let method = unsafe { str_in(method) }?.unwrap_or_default();
        // SAFETY: forwarded caller contract.
        let arg = match unsafe { str_in(arg_json) }? {
            Some(text) => serde_json::from_str(&text)
                .map_err(|e| format!("invalid-argument: the argument is not JSON: {e}"))?,
            None => Value::Null,
        };
        let value = call(od, &method, &arg)?;
        Ok(buffer(value.to_string().into_bytes()))
    })
}

/// Saves to `format` (`"docx"`, `"pdf"`, …) in `mode` (`NULL`, `"exact"`,
/// `"preserve"`, `"semantic"`). The report is `opendoc_call(doc,
/// "lastSaveReport")`.
///
/// # Safety
///
/// `doc` is a live handle; `format` is NUL-terminated; `mode` is null or
/// NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendoc_save(
    doc: *mut OdDocument,
    format: *const c_char,
    mode: *const c_char,
) -> *mut OdBuffer {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let od = unsafe { doc_mut(doc) }?;
        // SAFETY: forwarded caller contract.
        let format: Format = unsafe { str_in(format) }?
            .unwrap_or_else(|| "docx".to_owned())
            .parse()
            .map_err(sdk)?;
        // SAFETY: forwarded caller contract.
        let mode: SaveMode = match unsafe { str_in(mode) }? {
            Some(mode) => mode.parse().map_err(sdk)?,
            None => SaveMode::default(),
        };
        let (bytes, report) = od.doc.save(format, mode).map_err(sdk)?;
        od.last_report = serde_json::to_string(&report).unwrap_or_else(|_| "null".to_owned());
        Ok(buffer(bytes))
    })
}

/// One page as PNG.
///
/// # Safety
///
/// `doc` is a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendoc_render_png(
    doc: *mut OdDocument,
    page: u32,
    dpi: f32,
) -> *mut OdBuffer {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let od = unsafe { doc_mut(doc) }?;
        od.doc
            .render_page_png(page as usize, dpi)
            .map(buffer)
            .map_err(sdk)
    })
}

/// A real-text PDF.
///
/// # Safety
///
/// `doc` is a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendoc_to_pdf(doc: *mut OdDocument) -> *mut OdBuffer {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let od = unsafe { doc_mut(doc) }?;
        od.doc.to_pdf().map(buffer).map_err(sdk)
    })
}

/// The structural diff of `older` against `newer`, as JSON.
///
/// # Safety
///
/// Both are live handles (they may be the same handle).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendoc_compare(
    older: *mut OdDocument,
    newer: *mut OdDocument,
) -> *mut OdBuffer {
    guard(|| {
        if older.is_null() || newer.is_null() {
            return Err("invalid-argument: a null document handle".to_owned());
        }
        // SAFETY: the caller promises two live handles; both are only read.
        let (a, b) = unsafe { (&*older, &*newer) };
        a.doc
            .compare(&b.doc)
            .map(|v| buffer(v.to_string().into_bytes()))
            .map_err(sdk)
    })
}

/// Converts bytes to `to` in one call (open, then a semantic save).
///
/// # Safety
///
/// `data` points to `len` readable bytes; `to` is NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendoc_convert(
    data: *const u8,
    len: usize,
    to: *const c_char,
) -> *mut OdBuffer {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let bytes = unsafe { bytes_in(data, len) }?;
        // SAFETY: forwarded caller contract.
        let to: Format = unsafe { str_in(to) }?
            .unwrap_or_default()
            .parse()
            .map_err(sdk)?;
        opendoc::convert(bytes, to)
            .map(|(out, _)| buffer(out))
            .map_err(sdk)
    })
}

/// A buffer's bytes (valid until it is freed).
///
/// # Safety
///
/// `buf` is a live buffer from this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendoc_buffer_data(buf: *const OdBuffer) -> *const u8 {
    if buf.is_null() {
        return std::ptr::null();
    }
    // SAFETY: the caller promises a live buffer.
    unsafe { (*buf).bytes.as_ptr() }
}

/// A buffer's length in bytes.
///
/// # Safety
///
/// `buf` is a live buffer from this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendoc_buffer_len(buf: *const OdBuffer) -> usize {
    if buf.is_null() {
        return 0;
    }
    // SAFETY: the caller promises a live buffer.
    unsafe { (*buf).bytes.len() }
}

/// Frees a buffer. `NULL` is a no-op.
///
/// # Safety
///
/// `buf` is null or a buffer from this library not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendoc_buffer_free(buf: *mut OdBuffer) {
    if !buf.is_null() {
        // SAFETY: the caller promises a live buffer, freed once.
        drop(unsafe { Box::from_raw(buf) });
    }
}
