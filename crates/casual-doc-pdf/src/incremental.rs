//! The byte-preserving save's writer half (`docs/182` §7, P4): an
//! **incremental update** (ISO 32000-1 §7.5.6) appended to an original file.
//!
//! The original bytes are the update's prefix, unchanged, so every object of
//! every earlier revision — and the byte range a signature on one covers —
//! stays exactly where it was. The update adds:
//!
//! - per edited page, the page object again under its own number, whose
//!   `/Contents` is `[kept, overlay]`: `kept` is the original content with the
//!   edited frames' text operators removed (the caller does that, bracketed in
//!   `q`/`Q` here), and `overlay` draws a Form XObject;
//! - that Form XObject: the edited frames' text, transcribed from the shared
//!   display list by this crate's own text layer (the same transcriber, fonts,
//!   `ToUnicode` and subsetting the whole-file writer uses), mapped into the
//!   page's user space by its `/Matrix`, with its **own** resources, so no new
//!   name can collide with a name the page already uses;
//! - the shared resources and font objects those forms need;
//! - a cross-reference section of the same kind as the original's last one,
//!   whose trailer chains to it with `/Prev`.
//!
//! Nothing else is written: an unchanged page has no object in the update, so
//! it resolves to its original objects byte for byte.
//!
//! Complexity: O(original bytes) for the prefix copy, plus O(edited pages'
//! content and overlay paint) for the rest.

use crate::writer::{IncrementalBase, Writer, num};
use crate::{
    EmbeddedFace, PdfError, PdfFinding, PdfFontSource, PdfMediaSource, PdfPage, PdfTextSource,
    Transcriber, write_resources,
};

/// One page the update rewrites.
#[derive(Debug)]
pub struct EditedPage<'a> {
    /// The page object's number and generation in the original.
    pub object: (u32, u16),
    /// The page dictionary's other entries, raw (`/Parent`, boxes, `/Annots`,
    /// …), carried unchanged.
    pub entries: Vec<u8>,
    /// The page's effective resources except `/XObject`, raw.
    pub resource_entries: Vec<u8>,
    /// The page's `/XObject` entries, raw.
    pub xobject_entries: Vec<u8>,
    /// The name the overlay form is registered under; the caller picks one the
    /// page does not already use.
    pub form_name: String,
    /// The kept original content (decoded), edited frames' text removed.
    pub kept_content: Vec<u8>,
    /// Displayed page space (y up) to the page's user space.
    pub user_from_display: [f64; 6],
    /// The edited frames' paint, in displayed page space.
    pub overlay: PdfPage<'a>,
}

/// A finished update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncrementalExport {
    /// The whole file: the original bytes, then the update.
    pub bytes: Vec<u8>,
    /// The original's length: `bytes[..original_len]` is the original.
    pub original_len: usize,
    /// One entry per embedded face.
    pub faces: Vec<EmbeddedFace>,
    /// Everything the caller must be told about the overlay text.
    pub findings: Vec<PdfFinding>,
}

/// Appends an incremental update to `original` that rewrites `pages`.
///
/// # Errors
///
/// [`PdfError::Font`] when a face the overlay uses cannot be embedded.
pub fn write_incremental_update(
    original: &[u8],
    base: &IncrementalBase,
    pages: &[EditedPage<'_>],
    fonts: &dyn PdfFontSource,
    media: &dyn PdfMediaSource,
    text: &dyn PdfTextSource,
) -> Result<IncrementalExport, PdfError> {
    let mut writer = Writer::append_to(original, base.size);
    let resources = writer.reserve();
    let mut transcriber = Transcriber::new(fonts, media);
    for page in pages {
        let overlay = transcriber.page(&mut writer, page.overlay)?;
        let width = num(points(page.overlay.width));
        let height = num(points(page.overlay.height));
        let matrix = page
            .user_from_display
            .iter()
            .map(|v| num(*v as f32))
            .collect::<Vec<_>>()
            .join(" ");
        let form = writer.reserve();
        writer.stream(
            form,
            &format!(
                "/Type/XObject/Subtype/Form/BBox[0 0 {width} {height}]/Matrix[{matrix}]/Resources {}",
                resources.reference()
            ),
            &overlay,
            true,
        );
        // The kept content is bracketed so a state it leaves set (a `cm`
        // without `q`) cannot reach the overlay.
        let mut kept = Vec::with_capacity(page.kept_content.len() + 8);
        kept.extend_from_slice(b"q\n");
        kept.extend_from_slice(&page.kept_content);
        kept.extend_from_slice(b"\nQ\n");
        let kept_ref = writer.reserve();
        writer.stream(kept_ref, "", &kept, true);
        let draw = writer.reserve();
        let form_name = crate::writer::name(&page.form_name);
        writer.stream(draw, "", format!("q {form_name} Do Q\n").as_bytes(), false);

        let mut body = Vec::new();
        body.extend_from_slice(b"<<");
        body.extend_from_slice(&page.entries);
        body.extend_from_slice(b"/Resources<<");
        body.extend_from_slice(&page.resource_entries);
        body.extend_from_slice(b"/XObject<<");
        body.extend_from_slice(&page.xobject_entries);
        body.extend_from_slice(format!("{form_name} {}", form.reference()).as_bytes());
        body.extend_from_slice(b">>>>");
        body.extend_from_slice(
            format!("/Contents[{} {}]>>", kept_ref.reference(), draw.reference()).as_bytes(),
        );
        writer.replace_object(page.object.0, page.object.1, &body);
    }
    let overlays: Vec<PdfPage<'_>> = pages.iter().map(|page| page.overlay).collect();
    let (findings, faces) =
        write_resources(&mut writer, transcriber, &overlays, fonts, text, resources)?;
    Ok(IncrementalExport {
        bytes: writer.finish_incremental(base),
        original_len: original.len(),
        faces,
        findings,
    })
}

fn points(value: casual_doc_layout::units::Twip) -> f32 {
    value.raw() as f32 / 20.0
}
