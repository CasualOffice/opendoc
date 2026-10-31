#![no_main]

//! The C ABI (`crates/opendoc-ffi`) driven by arbitrary call sequences
//! (`docs/176` §12.14): the input is a little program. Each step picks an
//! export and its arguments from the bytes: documents opened from arbitrary
//! bytes with honest-but-wrong lengths (short, zero, a null pointer with a
//! length), JSON and non-JSON arguments, method names real and made up, page
//! numbers and resolutions in and out of range, and handles and buffers that
//! are null, live, freed already (stale) or of the wrong kind, freed out of
//! order and twice. Nothing may crash, hang or leak: every handle and buffer
//! still live at the end is freed.
//!
//! What the harness does NOT do is hand the library a pointer to fewer bytes
//! than the length it states, or a string without its NUL: those are the
//! caller's promise in any C ABI and no registry can check them.

use std::ffi::CString;

use libfuzzer_sys::fuzz_target;
use opendoc_ffi::*;

/// Reads the program's bytes.
struct Input<'a> {
    data: &'a [u8],
}

impl<'a> Input<'a> {
    fn byte(&mut self) -> Option<u8> {
        let (&first, rest) = self.data.split_first()?;
        self.data = rest;
        Some(first)
    }

    fn take(&mut self, n: usize) -> &'a [u8] {
        let n = n.min(self.data.len());
        let (head, rest) = self.data.split_at(n);
        self.data = rest;
        head
    }

    /// A short string, NUL bytes dropped so it can cross as a C string.
    fn text(&mut self) -> String {
        let len = usize::from(self.byte().unwrap_or(0) % 64);
        let raw: Vec<u8> = self.take(len).iter().copied().filter(|b| *b != 0).collect();
        String::from_utf8_lossy(&raw).into_owned()
    }
}

const METHODS: [&str; 26] = [
    "body",
    "paragraph",
    "table",
    "text",
    "sections",
    "styles",
    "comments",
    "revisions",
    "headersFooters",
    "notes",
    "media",
    "properties",
    "importReport",
    "sourceFormat",
    "isModified",
    "setAuthor",
    "edit",
    "undo",
    "renderTemplate",
    "templateTags",
    "pageCount",
    "pageSizes",
    "pageText",
    "outline",
    "fontNeeds",
    "lastSaveReport",
];

const FORMATS: [&str; 10] = [
    "docx", "dotx", "odt", "rtf", "txt", "json", "html", "md", "pdf", "nope",
];

/// Edit commands that reach the engine when given a real paragraph id.
const EDITS: [&str; 12] = [
    r#"[{"op":"insertText","paragraph":"ID","offset":N,"text":"x\ty"}]"#,
    r#"[{"op":"deleteText","paragraph":"ID","start":0,"end":N}]"#,
    r#"[{"op":"insertParagraph","after":"ID","text":"p"}]"#,
    r#"[{"op":"insertTable","rows":N,"columns":2}]"#,
    r#"[{"op":"headerFooter","section":N,"region":"header","kind":"first"}]"#,
    r#"[{"op":"insertNote","kind":"footnote","paragraph":"ID","offset":N,"text":"n"}]"#,
    r#"[{"op":"addSection","start":"continuous"}]"#,
    r#"[{"op":"setPageSetup","section":N,"setup":{"orientation":"landscape","columns":3}}]"#,
    r#"[{"op":"setSectionStart","section":N,"start":"oddPage"}]"#,
    r#"[{"op":"insertField","paragraph":"ID","offset":N,"kind":"page"}]"#,
    r#"[{"op":"replaceAll","find":"e","replace":"E"}]"#,
    r#"[{"op":"insertPicture","paragraph":"ID","offset":N,"image":"iVBORw0KGgo="}]"#,
];

/// Every pointer the library returned stays in its list after it is freed, so
/// later steps use it stale; one index past the end picks `NULL`.
fn pick<T>(slots: &[*mut T], choice: u8) -> *mut T {
    let index = usize::from(choice) % (slots.len() + 1);
    slots.get(index).copied().unwrap_or(std::ptr::null_mut())
}

fn read_buffer(buf: *mut OdBuffer) -> Vec<u8> {
    // SAFETY: the library checks the pointer against its registry.
    unsafe {
        let data = opendoc_buffer_data(buf);
        let len = opendoc_buffer_len(buf);
        if data.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(data, len).to_vec()
        }
    }
}

/// The first paragraph id in a `body` result, to aim edits at.
fn first_id(json: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(json).ok()?;
    let at = text.find("\"id\":\"")? + 6;
    let end = text[at..].find('"')?;
    Some(text[at..at + end].to_owned())
}

fuzz_target!(|data: &[u8]| {
    let mut input = Input { data };
    let mut docs: Vec<*mut OdDocument> = Vec::new();
    let mut bufs: Vec<*mut OdBuffer> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    let mut steps = 0;
    while let Some(op) = input.byte() {
        steps += 1;
        if steps > 48 {
            break;
        }
        let choice = input.byte().unwrap_or(0);
        let mut result: *mut OdBuffer = std::ptr::null_mut();
        // SAFETY: every pointer passed is null, live, stale or of the wrong
        // kind, never one to fewer bytes than stated; strings are CStrings.
        unsafe {
            match op % 16 {
                0 => {
                    let doc = opendoc_create();
                    if !doc.is_null() {
                        docs.push(doc);
                    }
                }
                1 => {
                    let len = usize::from(input.byte().unwrap_or(0)) * 4;
                    let bytes = input.take(len).to_vec();
                    // Honest lengths that are wrong for the content: all of it,
                    // a prefix, none of it, or a null pointer with a length.
                    let (ptr, n) = match choice % 4 {
                        0 => (bytes.as_ptr(), bytes.len()),
                        1 => (bytes.as_ptr(), bytes.len() / 2),
                        2 => (bytes.as_ptr(), 0),
                        _ => (std::ptr::null(), bytes.len()),
                    };
                    let options = CString::new(input.text()).unwrap_or_default();
                    let opts = if choice & 4 == 0 { std::ptr::null() } else { options.as_ptr() };
                    let doc = opendoc_open(ptr, n, opts);
                    if !doc.is_null() {
                        docs.push(doc);
                    }
                }
                2 => {
                    let method = METHODS[usize::from(choice) % METHODS.len()];
                    let selector = input.byte().unwrap_or(0);
                    let arg = if selector % 3 == 0 {
                        let id = ids.last().cloned().unwrap_or_default();
                        let n = input.byte().unwrap_or(0) % 8;
                        match method {
                            "edit" => EDITS[usize::from(selector / 3) % EDITS.len()]
                                .replace("ID", &id)
                                .replace('N', &n.to_string()),
                            "paragraph" | "table" => format!(r#"{{"id":"{id}"}}"#),
                            "pageText" => format!(r#"{{"index":{n}}}"#),
                            _ => "null".to_owned(),
                        }
                    } else {
                        input.text()
                    };
                    let method = CString::new(if selector % 7 == 6 { input.text() } else { method.to_owned() })
                        .unwrap_or_default();
                    let arg = CString::new(arg).unwrap_or_default();
                    let arg_ptr = if selector % 5 == 4 { std::ptr::null() } else { arg.as_ptr() };
                    result = opendoc_call(pick(&docs, input.byte().unwrap_or(0)), method.as_ptr(), arg_ptr);
                    if method.to_bytes() == b"body" && !result.is_null() {
                        if let Some(id) = first_id(&read_buffer(result)) {
                            ids.push(id);
                        }
                    }
                }
                3 => {
                    let format = CString::new(FORMATS[usize::from(choice) % FORMATS.len()]).unwrap_or_default();
                    let mode = CString::new(["exact", "preserve", "semantic", "x"][usize::from(choice >> 4) % 4])
                        .unwrap_or_default();
                    let mode_ptr = if choice & 8 == 0 { std::ptr::null() } else { mode.as_ptr() };
                    // PDF only now and then: it is the slowest export.
                    if FORMATS[usize::from(choice) % FORMATS.len()] != "pdf" || choice & 0x40 != 0 {
                        result = opendoc_save(pick(&docs, input.byte().unwrap_or(0)), format.as_ptr(), mode_ptr);
                    }
                }
                4 => {
                    let dpi = [f32::NAN, -1.0, 0.0, 1.0, 9.6, 24.0, f32::INFINITY, 1.0e9][usize::from(choice) % 8];
                    let page = u32::from(input.byte().unwrap_or(0) % 4);
                    result = opendoc_render_png(pick(&docs, input.byte().unwrap_or(0)), page, dpi);
                }
                5 => {
                    let page = u32::from(choice % 4);
                    result = opendoc_render_svg(pick(&docs, input.byte().unwrap_or(0)), page);
                }
                6 => {
                    opendoc_free(pick(&docs, choice));
                }
                7 => {
                    opendoc_buffer_free(pick(&bufs, choice));
                }
                8 => {
                    let _ = read_buffer(pick(&bufs, choice));
                }
                9 => {
                    result = opendoc_compare(pick(&docs, choice), pick(&docs, input.byte().unwrap_or(0)));
                }
                10 => {
                    let len = usize::from(input.byte().unwrap_or(0)) * 2;
                    let bytes = input.take(len).to_vec();
                    let to = CString::new(FORMATS[usize::from(choice) % FORMATS.len()]).unwrap_or_default();
                    if FORMATS[usize::from(choice) % FORMATS.len()] != "pdf" {
                        result = opendoc_convert(bytes.as_ptr(), bytes.len(), to.as_ptr());
                    }
                }
                11 => {
                    let spec = CString::new(input.text()).unwrap_or_default();
                    let doc = opendoc_from_spec(spec.as_ptr());
                    if !doc.is_null() {
                        docs.push(doc);
                    }
                }
                12 => {
                    // A buffer handed over as a document, and a document as a buffer.
                    let wrong: *mut OdDocument = pick(&bufs, choice).cast();
                    let method = CString::new("text").unwrap_or_default();
                    result = opendoc_call(wrong, method.as_ptr(), std::ptr::null());
                    let _ = read_buffer(pick(&docs, choice).cast());
                }
                13 => {
                    result = opendoc_last_error();
                }
                14 => {
                    result = opendoc_version();
                }
                _ => {
                    let _ = opendoc_abi_version();
                }
            }
        }
        if !result.is_null() {
            bufs.push(result);
        }
    }
    // Free what is still live, in an order the program chose; a freed slot is
    // freed again on purpose, which the library must refuse.
    // SAFETY: as above.
    unsafe {
        for buf in bufs.iter().rev() {
            opendoc_buffer_free(*buf);
        }
        for doc in &docs {
            opendoc_free(*doc);
        }
        let _ = opendoc_last_error_and_free();
    }
});

/// Clears this thread's last error, freeing its buffer.
///
/// # Safety
///
/// None beyond the ABI's own.
unsafe fn opendoc_last_error_and_free() {
    let buf = opendoc_last_error();
    // SAFETY: a buffer from the library, or null.
    unsafe { opendoc_buffer_free(buf) };
}
