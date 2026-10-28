//! Links and annotations into the fixed-layout document (`docs/182` §5.1),
//! the way Word treats a PDF's when it opens one: a link becomes a hyperlink
//! over the text it covers, an in-document destination becomes a bookmark
//! the hyperlink points at, and a Text or FreeText note becomes a comment
//! with its author and date. Every other annotation (highlight, stamp, ink,
//! shapes) is reported `annotation_omitted`; popups are their note's window
//! and count with it, widgets and attachments are counted with forms and
//! files at admission.
//!
//! The covered text is inferred from geometry (a character belongs to a link
//! when its centre lies in the link's rectangle), so it is reported
//! (`hyperlink_inferred`, PDFR-I4); so is a comment's anchor
//! (`comment_inferred`, the frame nearest the note) and a bookmark's place
//! (`bookmark_inferred`, the frame nearest the destination's top).
//!
//! Complexity: O(annotations) to sort a page's annotations; a character's
//! link is a scan of the page's links (few per page in practice:
//! O(characters x links on the page)).

use casual_doc_pdf_read::{AnnotationEvidence, AnnotationKind, Bounds, LinkTarget};

use crate::report::{self, Findings, PdfModelOutcome as Outcome};

/// A note to become a comment.
#[derive(Clone, Debug)]
pub(crate) struct NoteAnnotation {
    /// Where it sits.
    pub rect: Bounds,
    /// `/T`.
    pub author: Option<String>,
    /// ISO 8601.
    pub date: Option<String>,
    /// `/Contents`.
    pub text: String,
}

/// One page's annotations, sorted by what they become.
#[derive(Clone, Debug, Default)]
pub(crate) struct PageAnnotations {
    /// Navigable links: rectangle and target.
    pub links: Vec<(Bounds, LinkTarget)>,
    /// Notes with text.
    pub notes: Vec<NoteAnnotation>,
}

/// Sorts a page's annotations and reports what is left out. O(annotations).
pub(crate) fn sort(
    annotations: &[AnnotationEvidence],
    findings: &mut Findings,
    page: usize,
) -> PageAnnotations {
    let mut out = PageAnnotations::default();
    for annotation in annotations {
        match &annotation.kind {
            AnnotationKind::Link(target @ (LinkTarget::Uri(_) | LinkTarget::Page { .. })) => {
                out.links.push((annotation.rect, target.clone()));
            }
            AnnotationKind::Link(_) => {
                findings.record(report::LINK_UNRESOLVED, Outcome::Omitted, 1, Some(page));
            }
            AnnotationKind::Note { .. } => match &annotation.contents {
                Some(text) if !text.trim().is_empty() => out.notes.push(NoteAnnotation {
                    rect: annotation.rect,
                    author: annotation.author.clone(),
                    date: annotation.date.clone(),
                    text: text.clone(),
                }),
                _ => findings.record(report::ANNOTATION_OMITTED, Outcome::Omitted, 1, Some(page)),
            },
            AnnotationKind::Popup | AnnotationKind::Widget | AnnotationKind::FileAttachment => {}
            AnnotationKind::Other => {
                findings.record(report::ANNOTATION_OMITTED, Outcome::Omitted, 1, Some(page));
            }
        }
    }
    out
}

/// The link whose rectangle holds a glyph's centre (`x0..x1` on `baseline`
/// at `size`), if any. O(links).
#[must_use]
pub(crate) fn link_at(
    links: &[(Bounds, LinkTarget)],
    x0: f64,
    x1: f64,
    baseline: f64,
    size: f64,
) -> Option<usize> {
    let (cx, cy) = ((x0 + x1) / 2.0, baseline - size * 0.3);
    links
        .iter()
        .position(|(r, _)| cx >= r.x0 && cx <= r.x1 && cy >= r.y0 - 1.0 && cy <= r.y1 + 1.0)
}

/// The squared distance from a rectangle to a point outside it (0 inside).
#[must_use]
pub(crate) fn distance(r: &Bounds, x: f64, y: f64) -> f64 {
    let dx = (r.x0 - x).max(0.0).max(x - r.x1);
    let dy = (r.y0 - y).max(0.0).max(y - r.y1);
    dx * dx + dy * dy
}
