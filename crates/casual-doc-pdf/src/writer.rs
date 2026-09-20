//! The PDF container: indirect objects, streams, the cross-reference table and
//! the trailer.
//!
//! Everything here is byte-deterministic. Object numbers are assigned in
//! reservation order, the file `/ID` is derived from a content hash rather than
//! from the clock or a random source, and no `/CreationDate` is written unless
//! the document itself supplies one. The same document therefore exports to the
//! same bytes on every machine and every run, which is the discipline the rest
//! of the repository already holds its writers to.

use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::io::Write;

/// A reference to an indirect object. Generation is always `0`: this writer
/// never rewrites an object in place, so no other generation can exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct Ref(pub(crate) u32);

impl Ref {
    /// The `N 0 R` reference form used inside dictionaries.
    pub(crate) fn reference(self) -> String {
        format!("{} 0 R", self.0)
    }
}

/// Streams below this many bytes are written uncompressed: the zlib header plus
/// the Flate dictionary costs more than the saving, and an uncompressed short
/// stream stays readable in a hex dump when something needs debugging.
const MIN_COMPRESSED_STREAM_BYTES: usize = 128;

/// The incremental PDF file builder.
pub(crate) struct Writer {
    bytes: Vec<u8>,
    /// Byte offset of each reserved object, indexed by `object number - 1`.
    /// `None` until the object body is written.
    offsets: Vec<Option<usize>>,
}

impl Writer {
    /// Starts a PDF 1.7 file, including the binary comment that marks the file
    /// as containing 8-bit data (required so naive transports do not mangle it).
    pub(crate) fn new() -> Self {
        let mut bytes = Vec::with_capacity(8 * 1024);
        bytes.extend_from_slice(b"%PDF-1.7\n");
        bytes.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");
        Self {
            bytes,
            offsets: Vec::new(),
        }
    }

    /// Reserves the next object number without writing its body, so a
    /// dictionary can reference an object that is emitted later.
    pub(crate) fn reserve(&mut self) -> Ref {
        self.offsets.push(None);
        // Object numbering starts at 1; object 0 is the free-list head.
        Ref(u32::try_from(self.offsets.len()).expect("object count fits in u32"))
    }

    /// Writes a complete indirect object whose body is `body`.
    pub(crate) fn object(&mut self, id: Ref, body: &str) {
        self.start_object(id);
        self.bytes.extend_from_slice(body.as_bytes());
        self.bytes.extend_from_slice(b"\nendobj\n");
    }

    /// Writes an indirect stream object. `dict_entries` are the caller's extra
    /// dictionary entries (without the enclosing `<<`/`>>`); `/Length` and, when
    /// the payload is compressed, `/Filter` are appended by this writer.
    ///
    /// Returns the number of bytes actually stored, which is what the size
    /// accounting in the export report reports.
    pub(crate) fn stream(
        &mut self,
        id: Ref,
        dict_entries: &str,
        data: &[u8],
        compress: bool,
    ) -> usize {
        let compressed = if compress && data.len() >= MIN_COMPRESSED_STREAM_BYTES {
            deflate(data)
        } else {
            None
        };
        let payload = compressed.as_deref().unwrap_or(data);
        self.start_object(id);
        self.bytes.extend_from_slice(b"<<");
        self.bytes.extend_from_slice(dict_entries.as_bytes());
        if compressed.is_some() {
            self.bytes.extend_from_slice(b"/Filter/FlateDecode");
        }
        self.bytes
            .extend_from_slice(format!("/Length {}>>\nstream\n", payload.len()).as_bytes());
        self.bytes.extend_from_slice(payload);
        self.bytes.extend_from_slice(b"\nendstream\nendobj\n");
        payload.len()
    }

    /// Writes a stream whose payload is already encoded by `filter` (for
    /// instance a JPEG passed through as `DCTDecode`), so the bytes are stored
    /// verbatim with no re-encode.
    pub(crate) fn preencoded_stream(
        &mut self,
        id: Ref,
        dict_entries: &str,
        filter: &str,
        data: &[u8],
    ) -> usize {
        self.start_object(id);
        self.bytes.extend_from_slice(b"<<");
        self.bytes.extend_from_slice(dict_entries.as_bytes());
        self.bytes.extend_from_slice(
            format!("/Filter/{filter}/Length {}>>\nstream\n", data.len()).as_bytes(),
        );
        self.bytes.extend_from_slice(data);
        self.bytes.extend_from_slice(b"\nendstream\nendobj\n");
        data.len()
    }

    fn start_object(&mut self, id: Ref) {
        let index = usize::try_from(id.0).expect("object number fits in usize") - 1;
        debug_assert!(
            self.offsets[index].is_none(),
            "an object number is written exactly once"
        );
        self.offsets[index] = Some(self.bytes.len());
        self.bytes
            .extend_from_slice(format!("{} 0 obj\n", id.0).as_bytes());
    }

    /// Writes the cross-reference table and trailer and returns the file bytes.
    ///
    /// Any object that was reserved but never written is emitted as a free
    /// entry rather than a dangling offset, so a caller bug produces a readable
    /// file rather than a corrupt one.
    pub(crate) fn finish(mut self, catalog: Ref, info: Option<Ref>) -> Vec<u8> {
        let id = content_id(&self.bytes);
        let xref_offset = self.bytes.len();
        let count = self.offsets.len() + 1;
        self.bytes
            .extend_from_slice(format!("xref\n0 {count}\n").as_bytes());
        self.bytes.extend_from_slice(b"0000000000 65535 f \n");
        for offset in &self.offsets {
            match offset {
                Some(offset) => self
                    .bytes
                    .extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes()),
                None => self.bytes.extend_from_slice(b"0000000000 65535 f \n"),
            }
        }
        let info_entry = info.map_or(String::new(), |id| format!("/Info {}", id.reference()));
        self.bytes.extend_from_slice(
            format!(
                "trailer\n<</Size {count}/Root {}{info_entry}/ID[<{id}><{id}>]>>\nstartxref\n{xref_offset}\n%%EOF\n",
                catalog.reference()
            )
            .as_bytes(),
        );
        self.bytes
    }
}

/// Flate-compresses `data`, returning `None` when the result is not smaller than
/// the input (a stream of incompressible bytes is stored as-is).
fn deflate(data: &[u8]) -> Option<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
    encoder.write_all(data).ok()?;
    let out = encoder.finish().ok()?;
    (out.len() < data.len()).then_some(out)
}

/// A deterministic 128-bit content fingerprint, rendered as 32 hex digits, used
/// for the file `/ID`.
///
/// This is FNV-1a run twice with different offset bases. It is a
/// **non-cryptographic** identity: the `/ID` array exists so a consumer can tell
/// two revisions of a file apart, not to authenticate anything. Deriving it from
/// the content rather than from the clock is what keeps the export
/// byte-reproducible.
fn content_id(bytes: &[u8]) -> String {
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut low: u64 = 0xcbf2_9ce4_8422_2325;
    let mut high: u64 = 0x9e37_79b9_7f4a_7c15;
    for byte in bytes {
        low = (low ^ u64::from(*byte)).wrapping_mul(PRIME);
        high = (high ^ u64::from(byte.rotate_left(3))).wrapping_mul(PRIME);
    }
    format!("{low:016x}{high:016x}")
}

/// Formats a coordinate for a content stream or dictionary.
///
/// PDF numbers are decimal with no exponent, so `{}` on an `f32` is not safe
/// (it can emit `1e-7`). Four fractional digits is finer than a thousandth of a
/// point, well past what any consumer resolves, and trailing zeros are trimmed
/// so the output stays compact and stable.
pub(crate) fn num(value: f32) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    let mut text = format!("{value:.4}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" { "0".to_owned() } else { text }
}

/// Formats an sRGB channel as a PDF colour component in `0..=1`.
pub(crate) fn channel(value: u8) -> String {
    num(f32::from(value) / 255.0)
}

/// Encodes a PDF text string.
///
/// Pure-ASCII text is written in literal form with the three characters that
/// must be escaped handled; anything else becomes a UTF-16BE hex string with a
/// byte-order mark, which is the only form ISO 32000-1 defines for non-ASCII
/// text strings.
pub(crate) fn text_string(value: &str) -> String {
    if value.is_ascii() && !value.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
        let mut out = String::with_capacity(value.len() + 2);
        out.push('(');
        for ch in value.chars() {
            if matches!(ch, '(' | ')' | '\\') {
                out.push('\\');
            }
            out.push(ch);
        }
        out.push(')');
        return out;
    }
    let mut out = String::from("<FEFF");
    for unit in value.encode_utf16() {
        out.push_str(&format!("{unit:04X}"));
    }
    out.push('>');
    out
}

/// Encodes a PDF name, escaping every byte a name may not carry literally.
pub(crate) fn name(value: &str) -> String {
    let mut out = String::from("/");
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'+') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("#{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_never_use_exponent_notation() {
        // `format!("{}", 1e-7f32)` yields "0.0000001" but `1e-30` yields
        // "0.000000000000000000000000000001"; a content stream must never carry
        // an exponent, and four-digit rounding is what keeps it bounded.
        assert_eq!(num(1e-30), "0");
        assert_eq!(num(12.5), "12.5");
        assert_eq!(num(-0.00001), "0");
        assert_eq!(num(1.0), "1");
        assert!(!num(f32::NAN).contains("NaN"));
    }

    #[test]
    fn text_strings_escape_delimiters_and_promote_non_ascii() {
        assert_eq!(text_string("a(b)c\\d"), "(a\\(b\\)c\\\\d)");
        assert_eq!(text_string("é"), "<FEFF00E9>");
    }

    #[test]
    fn the_file_identifier_is_derived_from_content_not_the_clock() {
        assert_eq!(content_id(b"abc"), content_id(b"abc"));
        assert_ne!(content_id(b"abc"), content_id(b"abd"));
        assert_eq!(content_id(b"abc").len(), 32);
    }

    #[test]
    fn a_reserved_but_unwritten_object_becomes_a_free_entry() {
        let mut writer = Writer::new();
        let catalog = writer.reserve();
        let orphan = writer.reserve();
        writer.object(catalog, "<</Type/Catalog>>");
        let bytes = writer.finish(catalog, None);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("xref\n0 3\n"), "{text}");
        // Two free entries: object 0 and the orphan.
        assert_eq!(text.matches("65535 f").count(), 2);
        let _ = orphan;
    }
}
