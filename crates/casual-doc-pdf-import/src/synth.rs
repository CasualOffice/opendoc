//! Rights-safe synthetic PDFs for the reflow, link and font tests, written
//! from constants the way P0's corpus is (`tools/opendoc-pdf-measure`), so
//! no third-party or user content is involved and every file regenerates
//! byte for byte. Generated in the test rather than committed, so the P0
//! measurement artifact (`fixtures/pdf/measurement.json`, which covers that
//! directory) is not changed by this lane.
//!
//! The writer is the smallest correct PDF 1.7 serializer: numbered objects, a
//! classic cross-reference table with exact offsets, a trailer. Streams are
//! uncompressed. O(total bytes).

/// A PDF under construction: objects numbered from 1 in insertion order.
#[derive(Debug, Default)]
pub struct Pdf {
    objects: Vec<Option<Vec<u8>>>,
}

impl Pdf {
    /// Reserves an object number to be filled later.
    pub fn reserve(&mut self) -> u32 {
        self.objects.push(None);
        u32::try_from(self.objects.len()).expect("fits")
    }

    /// Sets a reserved object's body.
    pub fn set(&mut self, id: u32, body: impl Into<Vec<u8>>) {
        self.objects[id as usize - 1] = Some(body.into());
    }

    /// Adds an object.
    pub fn add(&mut self, body: impl Into<Vec<u8>>) -> u32 {
        let id = self.reserve();
        self.set(id, body);
        id
    }

    /// Adds a stream with `extra` dictionary entries.
    pub fn stream(&mut self, extra: &str, data: &[u8]) -> u32 {
        let mut body = format!("<< /Length {} {extra} >>\nstream\n", data.len()).into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        self.add(body)
    }

    /// Serializes with `root` as the catalog.
    pub fn finish(self, root: u32) -> Vec<u8> {
        let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = Vec::with_capacity(self.objects.len());
        for (index, body) in self.objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            out.extend_from_slice(body.as_deref().unwrap_or(b"null"));
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", self.objects.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root {root} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                self.objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }
}

/// Standard-14 fonts: F1 Helvetica, F2 Times-Roman, F3 Helvetica-Bold,
/// F4 Times-Italic, F5 Times-Bold, F6 Courier.
pub const FONTS: &str = "/Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >> /F2 << /Type /Font /Subtype /Type1 /BaseFont /Times-Roman /Encoding /WinAnsiEncoding >> /F3 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >> /F4 << /Type /Font /Subtype /Type1 /BaseFont /Times-Italic /Encoding /WinAnsiEncoding >> /F5 << /Type /Font /Subtype /Type1 /BaseFont /Times-Bold /Encoding /WinAnsiEncoding >> /F6 << /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >> >>";

/// Escapes a literal string.
#[must_use]
pub fn literal(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// Lines at `leading` from (`x`, `y`) (PDF space, y up).
#[must_use]
pub fn text(font: &str, size: f64, x: f64, y: f64, leading: f64, lines: &[&str]) -> String {
    let mut out = format!("BT /{font} {size} Tf {leading} TL {x} {y} Td\n");
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            out.push_str("T*\n");
        }
        out.push_str(&format!("({}) Tj\n", literal(line)));
    }
    out.push_str("ET\n");
    out
}

/// One page: content and extra page-dictionary entries (annotations, ...).
#[derive(Clone, Debug, Default)]
pub struct Page {
    /// The content stream.
    pub content: String,
    /// Annotation dictionaries (`/Annots` entries, inline); `{page}` in them
    /// is replaced with this page's object reference and `{pageN}` with page
    /// N's (zero-based).
    pub annots: Vec<String>,
    /// Extra page entries.
    pub extra: String,
}

impl Page {
    /// A page holding `content`.
    #[must_use]
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ..Self::default()
        }
    }
}

/// A document of `pages` with standard fonts plus `resources`, and
/// `catalog_extra` in the catalog (`{pageN}` substituted there too).
#[must_use]
pub fn document(pages: &[Page], resources: &str, catalog_extra: &str) -> Vec<u8> {
    let mut pdf = Pdf::default();
    document_in(&mut pdf, pages, resources, catalog_extra, |_| String::new())
}

/// [`document`] in an existing writer (so callers can add objects first);
/// `page_extra(i)` adds entries to page `i`.
pub fn document_in(
    pdf: &mut Pdf,
    pages: &[Page],
    resources: &str,
    catalog_extra: &str,
    page_extra: impl Fn(usize) -> String,
) -> Vec<u8> {
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let ids: Vec<u32> = pages.iter().map(|_| pdf.reserve()).collect();
    let substitute = |text: &str, this: u32| {
        let mut out = text.replace("{page}", &format!("{this} 0 R"));
        for (i, id) in ids.iter().enumerate() {
            out = out.replace(&format!("{{page{i}}}"), &format!("{id} 0 R"));
        }
        out
    };
    for (i, page) in pages.iter().enumerate() {
        let content = pdf.stream("", page.content.as_bytes());
        // `{annotK}` names this page's K-th annotation (a reply's `/IRT`).
        let annot_ids: Vec<u32> = page.annots.iter().map(|_| pdf.reserve()).collect();
        let annots: Vec<String> = page
            .annots
            .iter()
            .zip(&annot_ids)
            .map(|(a, &id)| {
                let mut body = substitute(a, ids[i]);
                for (k, other) in annot_ids.iter().enumerate() {
                    body = body.replace(&format!("{{annot{k}}}"), &format!("{other} 0 R"));
                }
                pdf.set(id, body);
                format!("{id} 0 R")
            })
            .collect();
        let annots = if annots.is_empty() {
            String::new()
        } else {
            format!("/Annots [{}]", annots.join(" "))
        };
        // Resources naming their own `/Font` replace the standard set.
        let fonts = if resources.trim_start().starts_with("/Font") {
            ""
        } else {
            FONTS
        };
        pdf.set(
            ids[i],
            format!(
                "<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 612 792] /Contents {content} 0 R /Resources << {fonts} {resources} >> {annots} {} {} >>",
                page.extra,
                page_extra(i)
            ),
        );
    }
    let kids: Vec<String> = ids.iter().map(|id| format!("{id} 0 R")).collect();
    pdf.set(
        tree,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            kids.len()
        ),
    );
    let catalog_extra = substitute(catalog_extra, ids[0]);
    pdf.set(
        catalog,
        format!("<< /Type /Catalog /Pages {tree} 0 R {catalog_extra} >>"),
    );
    // Objects the caller added first (a structure tree) may name pages too.
    for body in pdf.objects.iter_mut().flatten() {
        for (i, id) in ids.iter().enumerate() {
            let key = format!("{{page{i}}}").into_bytes();
            let value = format!("{id} 0 R").into_bytes();
            while let Some(at) = body.windows(key.len()).position(|w| w == key.as_slice()) {
                body.splice(at..at + key.len(), value.iter().copied());
            }
        }
    }
    std::mem::take(pdf).finish(catalog)
}

/// A stroked segment (y up).
#[must_use]
pub fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    format!("0.5 w {x0} {y0} m {x1} {y1} l S\n")
}

/// The prose the fixtures use, written by this repository.
pub const PROSE: [&str; 8] = [
    "The quick brown fox jumps over the lazy dog while the committee",
    "reviews the quarterly figures. Every sentence in this synthetic",
    "corpus is written by the repository, so the measurement carries",
    "no third-party text and regenerates byte for byte on any machine.",
    "A second paragraph begins after a blank line so the grouping",
    "parameters see a paragraph gap that is larger than the leading",
    "between lines, which is what separates two blocks in a frame.",
    "Numbers such as 3.14159 and 2,718 sit beside punctuation; ok?",
];
