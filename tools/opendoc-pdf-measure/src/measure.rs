//! Runs hayro over one PDF and records what docs/182 §10 P0 asks for.
//!
//! The measuring [`Device`] counts; it does not keep evidence. Images are
//! decoded (that is part of the cost the importer will pay) unless they declare
//! more pixels than [`MEASURE_IMAGE_PIXEL_CAP`], in which case the declaration
//! is recorded and the decode skipped, because measuring a hostile file must not
//! take down the harness.
//!
//! Complexity: O(document): one interpretation of every page.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use hayro_interpret::font::{Glyph, GlyphRun};
use hayro_interpret::hayro_cmap::BfString;
use hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask, TransformExt, interpret_page,
};
use hayro_syntax::object::Stream;
use hayro_syntax::{Filter, LoadPdfError, Pdf};
use kurbo::{BezPath, Rect};
use serde_json::{Value, json};
use skrifa::MetadataProvider;

use crate::PEAK;

/// Images declaring more pixels than this are not decoded by the harness.
pub const MEASURE_IMAGE_PIXEL_CAP: u64 = 100_000_000;

/// Deterministic per-file counts (compared by the artifact test).
#[derive(Debug, Default)]
struct Counts {
    glyphs: u64,
    unicode_glyphs: u64,
    type3_glyphs: u64,
    paths: u64,
    even_odd_fills: u64,
    images: u64,
    stencil_images: u64,
    images_over_cap: u64,
    marked_content: u64,
    codecs: BTreeSet<&'static str>,
    max_image_pixels: u64,
    /// Font cache key -> (program kind, chars used).
    fonts: BTreeMap<u128, FontSeen>,
}

#[derive(Debug, Default)]
struct FontSeen {
    program: &'static str,
    data: Option<Vec<u8>>,
    chars: BTreeSet<char>,
}

struct Measuring<'c> {
    counts: &'c mut Counts,
}

fn codec(filter: &Filter) -> &'static str {
    match filter {
        Filter::AsciiHexDecode => "ASCIIHex",
        Filter::Ascii85Decode => "ASCII85",
        Filter::LzwDecode => "LZW",
        Filter::FlateDecode => "Flate",
        Filter::RunLengthDecode => "RunLength",
        Filter::CcittFaxDecode => "CCITTFax",
        Filter::Jbig2Decode => "JBIG2",
        Filter::DctDecode => "DCT",
        Filter::JpxDecode => "JPX",
        Filter::Crypt => "Crypt",
    }
}

fn record_codecs(counts: &mut Counts, stream: &Stream<'_>) {
    let filters = stream.filters();
    if filters.is_empty() {
        counts.codecs.insert("raw");
    }
    for filter in &filters {
        counts.codecs.insert(codec(filter));
    }
}

/// What an embedded program is, from its first bytes.
fn program_kind(bytes: &[u8]) -> &'static str {
    match bytes.first_chunk::<4>() {
        Some(b"\x00\x01\x00\x00" | b"true") => "truetype",
        Some(b"OTTO") => "opentype-cff",
        Some(b"ttcf") => "collection",
        Some([1, 0, _, _]) => "bare-cff",
        _ => "other",
    }
}

impl<'a> Device<'a> for Measuring<'_> {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, mode: &DrawMode) {
        self.counts.paths += 1;
        if matches!(
            mode,
            DrawMode::Fill(hayro_interpret::FillRule::EvenOdd)
                | DrawMode::FillAndStroke(hayro_interpret::FillRule::EvenOdd, _)
        ) {
            self.counts.even_odd_fills += 1;
        }
    }
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, _: DrawProps<'a>, _: &DrawMode) {
        for glyph in run.glyphs() {
            self.counts.glyphs += 1;
            let unicode = glyph.as_unicode();
            if unicode.is_some() {
                self.counts.unicode_glyphs += 1;
            }
            match &**glyph {
                Glyph::Outline(outline) => {
                    let seen = self
                        .counts
                        .fonts
                        .entry(outline.font_cache_key())
                        .or_insert_with(|| {
                            let data = outline.font_data().map(|d| (*d.data).as_ref().to_vec());
                            FontSeen {
                                program: data.as_deref().map_or("not-extractable", program_kind),
                                data,
                                chars: BTreeSet::new(),
                            }
                        });
                    match unicode {
                        Some(BfString::Char(c)) => {
                            seen.chars.insert(c);
                        }
                        Some(BfString::String(s)) => seen.chars.extend(s.chars()),
                        None => {}
                    }
                }
                Glyph::Type3(_) => self.counts.type3_glyphs += 1,
            }
        }
    }
    fn draw_image(&mut self, image: Image<'a, '_>, _: ImageDrawProps<'a>) {
        let pixels = u64::from(image.width()) * u64::from(image.height());
        self.counts.max_image_pixels = self.counts.max_image_pixels.max(pixels);
        match image {
            Image::Raster(raster) => {
                self.counts.images += 1;
                record_codecs(self.counts, raster.stream());
                if pixels > MEASURE_IMAGE_PIXEL_CAP {
                    self.counts.images_over_cap += 1;
                } else {
                    raster.with_rgba(|_, _| {}, None);
                }
            }
            Image::Stencil(stencil) => {
                self.counts.stencil_images += 1;
                if pixels > MEASURE_IMAGE_PIXEL_CAP {
                    self.counts.images_over_cap += 1;
                } else {
                    stencil.with_stencil(|_, _| {}, None);
                }
            }
        }
    }
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
    fn begin_marked_content(&mut self, _: &[u8], _: Option<i32>) {
        self.counts.marked_content += 1;
    }
}

/// Whether an embedded sfnt maps every character the document drew with it,
/// i.e. whether registering it for editing would shape the same text.
fn covers(font: &FontSeen) -> bool {
    let Some(data) = &font.data else {
        return false;
    };
    let Ok(face) = skrifa::FontRef::new(data) else {
        return false;
    };
    let charmap = face.charmap();
    charmap.has_map() && font.chars.iter().all(|c| charmap.map(*c).is_some())
}

/// Measures one file. `name` labels it; the bytes never leave this function.
pub fn measure(name: &str, bytes: &[u8]) -> Value {
    let started = Instant::now();
    // A sentinel password: an unencrypted file ignores it, an encrypted one
    // (even with an empty user password) fails decryption, which is the
    // first-release refusal (docs/182 Q7).
    let pdf = match Pdf::new_with_password(bytes.to_vec(), "\u{1}opendoc-refuses-encryption") {
        Ok(pdf) => pdf,
        Err(error) => {
            let reason = match error {
                LoadPdfError::Decryption(_) => "encrypted",
                LoadPdfError::Invalid => "invalid",
            };
            return json!({
                "name": name,
                "bytes": bytes.len(),
                "deterministic": { "outcome": format!("refused:{reason}") },
                "timing": { "open_us": started.elapsed().as_micros() as u64 },
            });
        }
    };
    let open_us = started.elapsed().as_micros() as u64;
    let pages = pdf.pages();
    let settings = InterpreterSettings {
        render_annotations: false,
        ..InterpreterSettings::default()
    };
    let cache = InterpreterCache::new();
    let mut counts = Counts::default();
    let mut page_us = Vec::with_capacity(pages.len());
    let mut page_peak = Vec::with_capacity(pages.len());
    let mut operators: u64 = 0;
    let mut max_page_operators: u64 = 0;
    let mut decoded_content: u64 = 0;
    let mut max_page_side: f32 = 0.0;
    for page in pages.iter() {
        let (w, h) = page.render_dimensions();
        max_page_side = max_page_side.max(w).max(h);
        let baseline = PEAK.current_usage();
        PEAK.reset_peak_usage();
        let started = Instant::now();
        let stream_len = page.page_stream().map_or(0, <[u8]>::len) as u64;
        decoded_content += stream_len;
        let mut iter = page.operations();
        let mut ops: u64 = 0;
        while iter.next().is_some() {
            ops += 1;
        }
        operators += ops;
        max_page_operators = max_page_operators.max(ops);
        let mut context = Context::new(
            page.initial_transform(true).to_kurbo(),
            Rect::new(0.0, 0.0, f64::from(w), f64::from(h)),
            &cache,
            pdf.xref(),
            settings.clone(),
        );
        interpret_page(
            page,
            &mut context,
            &mut Measuring {
                counts: &mut counts,
            },
        );
        page_us.push(started.elapsed().as_micros() as u64);
        page_peak.push(PEAK.peak_usage().saturating_sub(baseline) as u64);
    }
    let mut programs: BTreeMap<&str, u64> = BTreeMap::new();
    let mut registrable = 0u64;
    for font in counts.fonts.values() {
        *programs.entry(font.program).or_default() += 1;
        if covers(font) {
            registrable += 1;
        }
    }
    page_us.sort_unstable();
    page_peak.sort_unstable();
    let median = |v: &[u64]| v.get(v.len() / 2).copied().unwrap_or(0);
    json!({
        "name": name,
        "bytes": bytes.len(),
        "deterministic": {
            "outcome": "parsed",
            "pages": pages.len(),
            "objects": pdf.len(),
            "operators": operators,
            "max_page_operators": max_page_operators,
            "decoded_content_bytes": decoded_content,
            "max_page_side_pt": max_page_side.round() as u64,
            "glyphs": counts.glyphs,
            "unicode_glyphs": counts.unicode_glyphs,
            "type3_glyphs": counts.type3_glyphs,
            "paths": counts.paths,
            "even_odd_fills": counts.even_odd_fills,
            "images": counts.images,
            "stencil_images": counts.stencil_images,
            "images_over_measurement_cap": counts.images_over_cap,
            "max_image_pixels": counts.max_image_pixels,
            "marked_content_sequences": counts.marked_content,
            "codecs": counts.codecs.iter().collect::<Vec<_>>(),
            "fonts": counts.fonts.len(),
            "font_programs": programs,
            "fonts_registrable_for_editing": registrable,
        },
        "timing": {
            "open_us": open_us,
            "median_page_us": median(&page_us),
            "max_page_us": page_us.last().copied().unwrap_or(0),
            "median_page_peak_heap_bytes": median(&page_peak),
            "max_page_peak_heap_bytes": page_peak.last().copied().unwrap_or(0),
        },
    })
}
