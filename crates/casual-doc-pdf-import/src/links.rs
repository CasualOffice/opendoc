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
    /// Its object id, and the note it replies to (`/IRT`).
    pub object: Option<(i32, i32)>,
    /// See [`NoteAnnotation::object`].
    pub reply_to: Option<(i32, i32)>,
    /// A FreeText note: its text is drawn on the page, at this size and
    /// colour (from `/DA`).
    pub drawn: Option<(f64, [u8; 3])>,
}

/// The size and colour of a `/DA` default appearance (`/Helv 12 Tf 0 0 1 rg`,
/// or `0.5 g`), defaulting to 12 pt black. O(string).
#[must_use]
pub(crate) fn appearance(da: Option<&str>) -> (f64, [u8; 3]) {
    let tokens: Vec<&str> = da.unwrap_or("").split_whitespace().collect();
    let number = |t: &str| t.parse::<f64>().ok();
    let mut size = 12.0;
    let mut colour = [0, 0, 0];
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    for (i, t) in tokens.iter().enumerate() {
        match *t {
            "Tf" => {
                if let Some(v) = i.checked_sub(1).and_then(|k| number(tokens[k]))
                    && v > 0.0
                {
                    size = v;
                }
            }
            "rg" if i >= 3 => {
                if let (Some(r), Some(g), Some(b)) = (
                    number(tokens[i - 3]),
                    number(tokens[i - 2]),
                    number(tokens[i - 1]),
                ) {
                    colour = [byte(r), byte(g), byte(b)];
                }
            }
            "g" if i >= 1 => {
                if let Some(v) = number(tokens[i - 1]) {
                    colour = [byte(v); 3];
                }
            }
            _ => {}
        }
    }
    (size.min(144.0), colour)
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
            AnnotationKind::Note { free_text, .. } => match &annotation.contents {
                Some(text) if !text.trim().is_empty() => out.notes.push(NoteAnnotation {
                    rect: annotation.rect,
                    author: annotation.author.clone(),
                    date: annotation.date.clone(),
                    text: text.clone(),
                    object: annotation.object,
                    reply_to: annotation.in_reply_to,
                    drawn: free_text.then(|| appearance(annotation.appearance.as_deref())),
                }),
                _ => findings.record(report::ANNOTATION_OMITTED, Outcome::Omitted, 1, Some(page)),
            },
            AnnotationKind::Popup | AnnotationKind::Widget | AnnotationKind::FileAttachment => {}
            AnnotationKind::Other => {
                findings.record(report::ANNOTATION_OMITTED, Outcome::Omitted, 1, Some(page));
            }
        }
    }
    // Roots before replies, so a reply finds the comment it threads onto; a
    // reply takes its root's rectangle (replies often carry none of their own).
    out.notes.sort_by_key(|n| n.reply_to.is_some());
    let roots: Vec<((i32, i32), Bounds)> = out
        .notes
        .iter()
        .filter_map(|n| n.object.map(|o| (o, n.rect)))
        .collect();
    for note in &mut out.notes {
        if let Some(parent) = note.reply_to
            && let Some((_, rect)) = roots.iter().find(|(o, _)| *o == parent)
        {
            note.rect = *rect;
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
