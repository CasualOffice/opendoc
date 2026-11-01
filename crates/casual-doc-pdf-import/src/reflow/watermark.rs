//! Watermarks: a stamp the PDF painted on its pages ("DRAFT",
//! "CONFIDENTIAL", a faint logo) becomes the section's [`Watermark`], the
//! way Word's Design ▸ Watermark writes one (a behind-text shape in every
//! header, `casual-doc-export`), instead of a heading in the body of every
//! page or a stray picture (`docs/182` §18.12).
//!
//! **Text.** `record` keeps every run that may be a stamp out of the text
//! blocks ([`Stamp`]: turned 20 to 70 degrees off level, or light or
//! translucent and 24 pt or larger). Per page, pieces of one stamp (a
//! producer that writes each letter as its own run) are joined: the same
//! angle, size and fill. A stamp is a watermark when
//!
//! - it repeats: the same text, angle and size, centred within
//!   [`SAME_PLACE`] of the page in the same place, on at least two pages and
//!   half of them; or
//! - on a document of one page (or a stamp on one page only): it is
//!   diagonal, light, at least [`SINGLE_SIZE`] and centred within
//!   [`CENTRED`] of the page's middle. A stamp with some of these but not
//!   all stays text and is reported `watermark_inferred` as degraded: the
//!   reader is unsure.
//!
//! **Pictures.** A picture drawn at most [`FAINT`] opaque, covering at least
//! [`PICTURE_AREA`] of its page, its smaller side at least [`PICTURE_SIDE`]
//! of the page's, centred within [`CENTRED`], on at least two pages and
//! half of them in the same place (or alone on a one-page document) is a
//! picture watermark, washed out. Shadow tiles and thin faint bands (a UI
//! mockup's) are none: they are neither large nor centred.
//!
//! One watermark per document, as Word's dialog sets one for every section:
//! the stamp found on the most pages. Everything not taken goes back where
//! it was, so a document with no watermark reflows exactly as before.
//!
//! O(stamps log stamps + pictures log pictures).

use std::borrow::Cow;
use std::collections::BTreeMap;

use casual_doc_model::v1::{Rgba, Watermark, WatermarkContent, WatermarkLayout, WatermarkText};
// Own line: the picture watermark.
use casual_doc_model::v1::FontName;

use super::record::{self, PdfSourceState, SourceImage, Stamp};
use crate::report::{self, Findings, PdfModelOutcome as Outcome};

/// Stamps on two pages are in the same place when their centres are within
/// this share of the page's width and height.
pub const SAME_PLACE: f64 = 0.03;
/// A lone stamp or picture is centred when its centre is within this share
/// of the page's width and height of the middle.
pub const CENTRED: f64 = 0.15;
/// A lone stamp's size floor, in points.
pub const SINGLE_SIZE: f64 = 36.0;
/// A picture watermark is drawn at most this opaque.
pub const FAINT: f32 = 0.6;
/// A picture watermark covers at least this share of its page.
pub const PICTURE_AREA: f64 = 0.05;
/// A picture watermark's smaller side is at least this share of the page's.
pub const PICTURE_SIDE: f64 = 0.15;

/// What the watermark is, before emission gives a picture its media entry.
#[derive(Clone, Debug)]
pub enum Found {
    /// Stamped words.
    Text(Watermark),
    /// A stamped picture (washed out).
    Picture(SourceImage),
}

/// One page's stamp, its pieces joined.
#[derive(Clone, Debug)]
struct Joined {
    page: usize,
    text: String,
    rotation: f64,
    size: f64,
    fill: Option<[u8; 4]>,
    /// The centre, as shares of the page's width and height.
    at: (f64, f64),
    font: usize,
    pieces: Vec<usize>,
}

/// A stamp's angle (degrees), size (half points) and fill.
type StampKey = (i64, i64, Option<[u8; 4]>);

/// The page's stamps joined into whole ones: the same angle (to a degree),
/// size (to half a point) and fill, ordered along their baseline. O(s log s).
fn join(page: usize, stamps: &[Stamp], width: f64, height: f64) -> Vec<Joined> {
    let mut groups: BTreeMap<StampKey, Vec<usize>> = BTreeMap::new();
    for (i, s) in stamps.iter().enumerate() {
        groups
            .entry((
                s.rotation.round() as i64,
                (s.size * 2.0).round() as i64,
                s.fill,
            ))
            .or_default()
            .push(i);
    }
    groups
        .into_values()
        .map(|mut pieces| {
            let first = &stamps[pieces[0]];
            let (sin, cos) = first.rotation.to_radians().sin_cos();
            let along = |s: &Stamp| s.origin.x * cos + s.origin.y * sin;
            pieces.sort_by(|&a, &b| along(&stamps[a]).total_cmp(&along(&stamps[b])));
            let text: String = pieces
                .iter()
                .map(|&i| stamps[i].text.as_str())
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let n = pieces.len() as f64;
            let cx = pieces.iter().map(|&i| stamps[i].center.x).sum::<f64>() / n;
            let cy = pieces.iter().map(|&i| stamps[i].center.y).sum::<f64>() / n;
            Joined {
                page,
                text,
                rotation: first.rotation,
                size: first.size,
                fill: first.fill,
                at: (cx / width.max(1.0), cy / height.max(1.0)),
                font: first.font,
                pieces,
            }
        })
        .filter(|j| !j.text.is_empty())
        .collect()
}

fn centred(at: (f64, f64)) -> bool {
    (at.0 - 0.5).abs() <= CENTRED && (at.1 - 0.5).abs() <= CENTRED
}

fn same_place(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() <= SAME_PLACE && (a.1 - b.1).abs() <= SAME_PLACE
}

/// The text watermark a stamp is.
fn text_watermark(state: &PdfSourceState, stamp: &Joined) -> Watermark {
    let [r, g, b, a] = stamp.fill.unwrap_or([192, 192, 192, 255]);
    // Word's "Semitransparent" is half alpha; a stamp drawn translucent is
    // that, and its colour is the opaque ink.
    let semi = a < 230;
    let (family, bold, italic) =
        crate::fixed::run_font(stamp.font, &state.fonts, &state.font_family);
    let font = (!family.is_empty()).then_some(FontName { name: family });
    Watermark {
        content: WatermarkContent::Text(WatermarkText {
            text: truncate(&stamp.text),
            font,
            size_half_points: Some(((stamp.size * 2.0).round() as u32).clamp(2, 3_276)),
            color: Rgba {
                r,
                g,
                b,
                a: if semi { 255 } else { a },
            },
            bold,
            italic,
        }),
        layout: if record::diagonal(stamp.rotation) {
            WatermarkLayout::Diagonal
        } else {
            WatermarkLayout::Horizontal
        },
        semi_transparent: semi,
    }
}

/// The model's bound on a watermark's text, at a character boundary.
fn truncate(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if out.len() + c.len_utf8() > casual_doc_model::v1::MAX_WATERMARK_TEXT_BYTES {
            break;
        }
        out.push(c);
    }
    out
}

/// A picture that may be a watermark: faint, large and centred on its page.
fn faint_picture(image: &SourceImage, width: f64, height: f64) -> bool {
    let b = image.bounds;
    let (w, h) = (b.x1 - b.x0, b.y1 - b.y0);
    image.opacity <= FAINT
        && w * h >= PICTURE_AREA * width * height
        && w.min(h) >= PICTURE_SIDE * width.min(height)
        && centred((
            (b.x0 + b.x1) / 2.0 / width.max(1.0),
            (b.y0 + b.y1) / 2.0 / height.max(1.0),
        ))
}

/// Finds the document's watermark and returns the state without it: the
/// stamps that are no watermark back in their pages' text blocks, the
/// watermark's own stamps and pictures gone. The state is borrowed
/// unchanged when no page had a stamp or a faint picture.
/// O(stamps log stamps + pictures log pictures).
pub(crate) fn extract<'a>(
    state: &'a PdfSourceState,
    findings: &mut Findings,
) -> (Cow<'a, PdfSourceState>, Option<Found>) {
    let pages = state.pages.len();
    let any_stamp = state.pages.iter().any(|p| !p.stamps.is_empty());
    let faint: Vec<(usize, usize)> = state
        .pages
        .iter()
        .enumerate()
        .flat_map(|(p, page)| {
            page.images
                .iter()
                .enumerate()
                .filter(|(_, image)| faint_picture(image, page.width, page.height))
                .map(move |(i, _)| (p, i))
        })
        .collect();
    if !any_stamp && faint.is_empty() {
        return (Cow::Borrowed(state), None);
    }
    let joined: Vec<Joined> = state
        .pages
        .iter()
        .enumerate()
        .flat_map(|(p, page)| join(p, &page.stamps, page.width, page.height))
        .collect();
    // Repeats: stamps of the same text, angle and size in the same place.
    let mut best: Option<(usize, Vec<usize>)> = None;
    for (i, a) in joined.iter().enumerate() {
        let alike: Vec<usize> = joined
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                b.text == a.text
                    && (b.rotation - a.rotation).abs() <= 1.0
                    && (b.size - a.size).abs() <= 0.5
                    && same_place(a.at, b.at)
            })
            .map(|(k, _)| k)
            .collect();
        let mut on: Vec<usize> = alike.iter().map(|&k| joined[k].page).collect();
        on.dedup();
        let repeated = on.len() >= 2 && 2 * on.len() >= pages;
        let lone = on.len() == 1
            && record::diagonal(a.rotation)
            && record::light(a.fill)
            && a.size >= SINGLE_SIZE
            && centred(a.at);
        if (repeated || lone) && best.as_ref().is_none_or(|(_, b)| alike.len() > b.len()) {
            best = Some((i, alike));
        }
    }
    // A faint picture in the same place on the pages, when no text is one.
    let picture: Option<Vec<(usize, usize)>> = if best.is_none() {
        faint.iter().find_map(|&(p, i)| {
            let a = state.pages[p].images[i].bounds;
            let alike: Vec<(usize, usize)> = faint
                .iter()
                .copied()
                .filter(|&(q, k)| {
                    let b = state.pages[q].images[k].bounds;
                    (a.x0 - b.x0).abs() <= 2.0
                        && (a.y0 - b.y0).abs() <= 2.0
                        && (a.x1 - b.x1).abs() <= 2.0
                        && (a.y1 - b.y1).abs() <= 2.0
                })
                .collect();
            let mut on: Vec<usize> = alike.iter().map(|x| x.0).collect();
            on.dedup();
            ((on.len() >= 2 && 2 * on.len() >= pages) || (pages == 1 && on.len() == 1))
                .then_some(alike)
        })
    } else {
        None
    };
    let mut owned = state.clone();
    let mut found = None;
    let mut taken: Vec<(usize, usize)> = Vec::new();
    if let Some((i, alike)) = &best {
        found = Some(Found::Text(text_watermark(state, &joined[*i])));
        for &k in alike {
            for &piece in &joined[k].pieces {
                taken.push((joined[k].page, piece));
            }
        }
        findings.record(
            report::WATERMARK_INFERRED,
            Outcome::Mapped,
            alike.len() as u32,
            Some(joined[*i].page),
        );
    }
    // Stamps that were nearly a watermark (diagonal or light, large and
    // centred, but not all of it): the reader is unsure, and says so.
    for j in &joined {
        let kept = j.pieces.iter().any(|&s| taken.contains(&(j.page, s)));
        if !kept && j.size >= SINGLE_SIZE && centred(j.at) {
            findings.record(
                report::WATERMARK_INFERRED,
                Outcome::Degraded,
                1,
                Some(j.page),
            );
        }
    }
    for (p, page) in owned.pages.iter_mut().enumerate() {
        let stamps = std::mem::take(&mut page.stamps);
        for (s, stamp) in stamps.into_iter().enumerate() {
            if taken.contains(&(p, s)) {
                continue;
            }
            page.rotated_runs += u32::from(stamp.rotated);
            page.blocks.extend(stamp.blocks);
        }
    }
    if let Some(alike) = picture {
        if let Some(&(p, i)) = alike.first() {
            found = Some(Found::Picture(state.pages[p].images[i].clone()));
            findings.record(
                report::WATERMARK_INFERRED,
                Outcome::Mapped,
                alike.len() as u32,
                Some(p),
            );
        }
        for (p, page) in owned.pages.iter_mut().enumerate() {
            let mut k = 0;
            page.images.retain(|_| {
                let keep = !alike.contains(&(p, k));
                k += 1;
                keep
            });
        }
    }
    (Cow::Owned(owned), found)
}
