// SPDX-License-Identifier: Apache-2.0

//! Erasing a line of text from a picture, for "edit the text in a picture"
//! (`docs/182` §20b): when an image-text line box over a scan is edited, the
//! scan's own glyphs under it must go, or the new text is drawn over the old.
//!
//! The picture is never changed. [`make_patch`] reads the box's pixels out of
//! the picture and returns a small picture of the same box **without its
//! ink**: the editor places it under the line's text box and over the scan, so
//! removing it (undo, "Restore original") shows the original pixels exactly.
//!
//! # The method, and the pattern it is
//!
//! It is inpainting, with a background model chosen per box:
//!
//! 1. **Ink from paper.** Otsu's threshold over the box's luminance splits it
//!    in two classes; the smaller is the ink (a line box is mostly paper,
//!    whichever is darker, so light text on a dark band works too). The ink
//!    mask is dilated by 1 or 2 pixels so the anti-aliased rim of each glyph
//!    counts as ink. **No ink pixel ever enters a background average**: that
//!    is the property the tests guard.
//! 2. **A smooth background ([`Method::Tiles`]).** The box is cut into tiles of
//!    4 to 8 pixels; each tile's value is the mean of its paper pixels only, a
//!    tile with none borrows its neighbours' (spread outward until every tile
//!    has one), and every pixel is the bilinear interpolation of the four tile
//!    centres around it, so no tile edge shows. Small tiles follow a gradient,
//!    a shadow or a stain closely; that is why they are small.
//! 3. **A textured background ([`Method::Inpaint`]).** When the paper pixels
//!    stray from that smooth model (a texture, a halftone, paper grain), a
//!    smooth patch would read as a flat sticker on the page. Then the paper
//!    pixels are kept as they are and only the ink pixels are solved, by
//!    iterated Laplacian relaxation (each ink pixel becomes the mean of its
//!    four neighbours, the paper around it held fixed): harmonic inpainting,
//!    diffusing inward from the ink's border, started from step 2's estimate
//!    so a few dozen sweeps converge.
//! 4. **The edge.** A tile-filled patch is feathered over its outer two pixels
//!    (alpha ramps up), so it fades into the picture under it rather than
//!    stopping at a line; ink pixels are never feathered, so no trace of a
//!    glyph shows through.
//!
//! [`FillMode::Blur`] and [`FillMode::Pixelate`] are the other two choices a
//! user has (Acrobat and Google Photos offer the same): a box blur, or a block
//! average, of the box *without its ink* (steps 1-3 first, exactly as Fill),
//! for a softer or blockier background. No mode reads an ink pixel's own
//! colour, so the text's colour never enters any average.
//!
//! [`make_patch`] also measures the line ([`LineStyle`]): the ink's colour
//! (the median of its core pixels), its height (for the font size), and its
//! stroke width against that height (for bold), so the edited text is set in
//! the colour and weight of the text it replaces.
//!
//! # Complexity
//!
//! Every step is O(box pixels), except the relaxation, which is
//! O(ink pixels × [`INPAINT_SWEEPS`]). Nothing reads outside the box; a box is
//! refused above [`MAX_PATCH_PIXELS`]. No dependency beyond `image`, which
//! this crate already decodes pictures with.

// Pixels are indexed per channel (`p[c]`) across several buffers at once;
// an iterator per buffer reads worse than the index it replaces.
#![allow(clippy::needless_range_loop)]

use image::ImageEncoder;

/// The largest box, in source pixels, a patch is made for (a 4000 × 1000
/// line). Past it the request is not a line of text.
pub const MAX_PATCH_PIXELS: usize = 4_000_000;

/// Relaxation sweeps of [`Method::Inpaint`]. Started from the tile estimate,
/// the residual stops moving well before this on a 6-pixel-wide stroke.
pub const INPAINT_SWEEPS: usize = 48;

/// Paper pixels whose luminance strays from the tile model by more than this
/// (root mean square, 0-255) make the box textured: [`Method::Inpaint`].
pub const TEXTURE_RMS: f64 = 9.0;

/// Stem width against the line's ink height from which a face is bold.
pub const BOLD_STEM: f64 = 0.18;

/// Feathered rim of a tile-filled patch, in pixels.
const FEATHER: usize = 2;

/// How the box's text is taken away.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FillMode {
    /// The background reconstructed without the ink (the default).
    #[default]
    Fill,
    /// The ink-free background (as [`FillMode::Fill`]) blurred.
    Blur,
    /// The ink-free background (as [`FillMode::Fill`]) as coarse blocks,
    /// each the mean of that background, never of ink.
    Pixelate,
}

impl FillMode {
    /// The mode named `token` (`fill`, `blur`, `pixelate`), if any.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "fill" => Some(Self::Fill),
            "blur" => Some(Self::Blur),
            "pixelate" => Some(Self::Pixelate),
            _ => None,
        }
    }

    /// The token [`FillMode::from_token`] reads.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Fill => "fill",
            Self::Blur => "blur",
            Self::Pixelate => "pixelate",
        }
    }
}

/// How a [`FillMode::Fill`] patch was made.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Method {
    /// Bilinear interpolation of tile means of the paper pixels.
    Tiles,
    /// Paper kept, ink solved by Laplacian relaxation.
    Inpaint,
    /// Not a fill: [`FillMode::Blur`] or [`FillMode::Pixelate`].
    Filter,
}

/// A decoded picture, 8-bit RGBA, row-major.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rgba {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

impl Rgba {
    /// A picture of one colour.
    #[must_use]
    pub fn filled(width: usize, height: usize, color: [u8; 4]) -> Self {
        let mut pixels = Vec::with_capacity(width * height * 4);
        for _ in 0..width * height {
            pixels.extend_from_slice(&color);
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    /// The pixel at `(x, y)`.
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.width + x) * 4;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    /// Sets the pixel at `(x, y)`.
    pub fn set(&mut self, x: usize, y: usize, color: [u8; 4]) {
        let i = (y * self.width + x) * 4;
        self.pixels[i..i + 4].copy_from_slice(&color);
    }

    /// Bilinear sample at `(x, y)` in pixel-centre coordinates, clamped to the
    /// picture. O(1).
    #[must_use]
    pub fn sample(&self, x: f64, y: f64) -> [f64; 4] {
        let max_x = self.width.saturating_sub(1) as f64;
        let max_y = self.height.saturating_sub(1) as f64;
        let x = (x - 0.5).clamp(0.0, max_x);
        let y = (y - 0.5).clamp(0.0, max_y);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (x - x0 as f64, y - y0 as f64);
        let mut out = [0.0; 4];
        for (c, value) in out.iter_mut().enumerate() {
            let at = |px: usize, py: usize| f64::from(self.pixels[(py * self.width + px) * 4 + c]);
            let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
            let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
            *value = top * (1.0 - fy) + bottom * fy;
        }
        out
    }
}

/// A line box in the source picture's pixels: its centre, its size along its
/// own axes, and its clockwise rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxGeometry {
    /// Centre, x, in source pixels.
    pub cx: f64,
    /// Centre, y, in source pixels.
    pub cy: f64,
    /// Width along the box's own x axis, in source pixels.
    pub width: f64,
    /// Height along the box's own y axis, in source pixels.
    pub height: f64,
    /// Clockwise rotation, in degrees (a deskewed scan's line, at most a few).
    pub rotation_degrees: f64,
}

/// What the ink of a line says about the text it was.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineStyle {
    /// The ink's colour: the per-channel median of its core pixels.
    pub ink: [u8; 3],
    /// The ink's height, top of the highest glyph to bottom of the lowest,
    /// in box pixels. Zero when the box has no ink.
    pub ink_height: usize,
    /// The ink's top row in the box, when it has ink.
    pub ink_top: usize,
    /// The median horizontal stroke width, in box pixels.
    pub stroke: f64,
    /// Stroke against height says the face is bold.
    pub bold: bool,
    /// The kind of face the ink looks set in.
    pub face: Face,
}

/// The kind of face a line's ink looks set in: what picks the nearest
/// bundled face (Liberation Sans, Serif or Mono) for the edited text.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Face {
    /// Even strokes, no serifs (Helvetica, Arial).
    #[default]
    Sans,
    /// Stroke contrast: vertical stems markedly heavier than the horizontal
    /// strokes (Times), which is where serifs and bracketed joins come from.
    Serif,
    /// Glyphs on an even pitch (Courier, a typewriter).
    Mono,
}

impl Face {
    /// The bundled family that draws this kind of face.
    #[must_use]
    pub const fn family(self) -> &'static str {
        match self {
            Self::Sans => "Liberation Sans",
            Self::Serif => "Liberation Serif",
            Self::Mono => "Liberation Mono",
        }
    }
}

/// Vertical stems heavier than horizontal strokes by this much make a
/// serif face (Times measures about 2.3, Helvetica about 1.1).
pub const SERIF_CONTRAST: f64 = 1.6;
/// Glyph starts this even (coefficient of variation of their pitch) make a
/// monospaced face; proportional text varies by a third or more.
pub const MONO_PITCH_CV: f64 = 0.12;

/// A patch: the box without its text, and what its text looked like.
#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    /// The patch's pixels, box-aligned (the box's own axes).
    pub image: Rgba,
    /// How it was made.
    pub method: Method,
    /// The line's measured style.
    pub style: LineStyle,
    /// Ink pixels found (before dilation).
    pub ink_pixels: usize,
}

/// Why no patch was made.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatchError {
    /// The box is empty or lies outside the picture.
    Empty,
    /// The box is over [`MAX_PATCH_PIXELS`].
    TooLarge,
    /// The picture's bytes do not decode.
    Undecodable,
}

impl core::fmt::Display for PatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Empty => "the box holds no pixels of the picture",
            Self::TooLarge => "the box is too large to be a line of text",
            Self::Undecodable => "the picture cannot be decoded",
        })
    }
}

impl std::error::Error for PatchError {}

/// Decodes a picture's bytes (any format this crate paints). O(pixels).
///
/// # Errors
///
/// [`PatchError::Undecodable`].
pub fn decode(bytes: &[u8]) -> Result<Rgba, PatchError> {
    let decoded = image::load_from_memory(bytes).map_err(|_| PatchError::Undecodable)?;
    let rgba = decoded.to_rgba8();
    Ok(Rgba {
        width: rgba.width() as usize,
        height: rgba.height() as usize,
        pixels: rgba.into_raw(),
    })
}

/// Encodes `image` as PNG. O(pixels).
///
/// # Errors
///
/// [`PatchError::Undecodable`] if the encoder refuses (never, for a valid
/// [`Rgba`]).
pub fn encode_png(image: &Rgba) -> Result<Vec<u8>, PatchError> {
    let mut out = Vec::new();
    let width = u32::try_from(image.width).map_err(|_| PatchError::TooLarge)?;
    let height = u32::try_from(image.height).map_err(|_| PatchError::TooLarge)?;
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(
            &image.pixels,
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|_| PatchError::Undecodable)?;
    Ok(out)
}

/// The box's pixels, resampled along its own axes (so a skewed line is
/// upright in the result). O(box pixels).
///
/// # Errors
///
/// [`PatchError::Empty`] or [`PatchError::TooLarge`].
pub fn sample_box(source: &Rgba, geometry: &BoxGeometry) -> Result<Rgba, PatchError> {
    let width = geometry.width.round();
    let height = geometry.height.round();
    if !(width >= 1.0 && height >= 1.0) || source.width == 0 || source.height == 0 {
        return Err(PatchError::Empty);
    }
    if width * height > MAX_PATCH_PIXELS as f64 {
        return Err(PatchError::TooLarge);
    }
    let (w, h) = (width as usize, height as usize);
    let (sin, cos) = geometry.rotation_degrees.to_radians().sin_cos();
    let mut out = Rgba::filled(w, h, [0, 0, 0, 0]);
    let mut inside = 0usize;
    for y in 0..h {
        for x in 0..w {
            // The pixel centre in box coordinates about the box's centre,
            // turned clockwise into the picture.
            let bx = x as f64 + 0.5 - width / 2.0;
            let by = y as f64 + 0.5 - height / 2.0;
            let sx = geometry.cx + bx * cos - by * sin;
            let sy = geometry.cy + bx * sin + by * cos;
            if sx >= 0.0 && sy >= 0.0 && sx <= source.width as f64 && sy <= source.height as f64 {
                inside += 1;
            }
            let v = source.sample(sx, sy);
            out.set(x, y, v.map(|c| c.round().clamp(0.0, 255.0) as u8));
        }
    }
    if inside == 0 {
        return Err(PatchError::Empty);
    }
    Ok(out)
}

/// Rec. 601 luma of a pixel, alpha-composited on white (a transparent pixel
/// of a PNG is paper).
fn luma(p: [u8; 4]) -> u8 {
    let a = f64::from(p[3]) / 255.0;
    let over = |c: u8| f64::from(c) * a + 255.0 * (1.0 - a);
    (0.299 * over(p[0]) + 0.587 * over(p[1]) + 0.114 * over(p[2]))
        .round()
        .clamp(0.0, 255.0) as u8
}

/// Otsu's threshold of `values`: the level that maximises the between-class
/// variance. Values at or below it are one class. O(n + 256).
#[must_use]
pub fn otsu(values: &[u8]) -> u8 {
    let mut histogram = [0u64; 256];
    for v in values {
        histogram[usize::from(*v)] += 1;
    }
    let total = values.len() as f64;
    let sum_all: f64 = histogram
        .iter()
        .enumerate()
        .map(|(i, n)| i as f64 * *n as f64)
        .sum();
    let (mut weight_low, mut sum_low) = (0.0, 0.0);
    let (mut best, mut best_variance) = (0u8, -1.0);
    for (level, n) in histogram.iter().enumerate() {
        weight_low += *n as f64;
        if weight_low == 0.0 {
            continue;
        }
        let weight_high = total - weight_low;
        if weight_high == 0.0 {
            break;
        }
        sum_low += level as f64 * *n as f64;
        let mean_low = sum_low / weight_low;
        let mean_high = (sum_all - sum_low) / weight_high;
        let variance = weight_low * weight_high * (mean_low - mean_high).powi(2);
        if variance > best_variance {
            best_variance = variance;
            best = level as u8;
        }
    }
    best
}

/// Which pixels are ink: Otsu's smaller class, or none when the box is one
/// tone (the two classes' means closer than 24 levels: no text there).
/// O(box pixels).
#[must_use]
pub fn ink_mask(image: &Rgba) -> Vec<bool> {
    let lumas: Vec<u8> = image
        .pixels
        .chunks_exact(4)
        .map(|p| luma([p[0], p[1], p[2], p[3]]))
        .collect();
    let threshold = otsu(&lumas);
    let (mut low, mut high) = ((0u64, 0u64), (0u64, 0u64));
    for v in &lumas {
        if *v <= threshold {
            low = (low.0 + 1, low.1 + u64::from(*v));
        } else {
            high = (high.0 + 1, high.1 + u64::from(*v));
        }
    }
    if low.0 == 0 || high.0 == 0 {
        return vec![false; lumas.len()];
    }
    let mean_low = low.1 as f64 / low.0 as f64;
    let mean_high = high.1 as f64 / high.0 as f64;
    if mean_high - mean_low < 24.0 {
        return vec![false; lumas.len()];
    }
    let ink_is_dark = low.0 <= high.0;
    lumas
        .iter()
        .map(|v| {
            if ink_is_dark {
                *v <= threshold
            } else {
                *v > threshold
            }
        })
        .collect()
}

/// `mask` grown by `radius` pixels (a square structuring element, done as two
/// separable passes). O(pixels × radius).
#[must_use]
pub fn dilate(mask: &[bool], width: usize, height: usize, radius: usize) -> Vec<bool> {
    if radius == 0 {
        return mask.to_vec();
    }
    let mut rows = vec![false; mask.len()];
    for y in 0..height {
        for x in 0..width {
            let (a, b) = (x.saturating_sub(radius), (x + radius).min(width - 1));
            rows[y * width + x] = (a..=b).any(|i| mask[y * width + i]);
        }
    }
    let mut out = vec![false; mask.len()];
    for y in 0..height {
        for x in 0..width {
            let (a, b) = (y.saturating_sub(radius), (y + radius).min(height - 1));
            out[y * width + x] = (a..=b).any(|j| rows[j * width + x]);
        }
    }
    out
}

/// The tile side for a box `height` pixels tall: a quarter of it, within
/// 4 to 8 pixels.
fn tile_side(height: usize) -> usize {
    (height / 4).clamp(4, 8)
}

/// The smooth background of `image` from its non-`ink` pixels only: tile means
/// of paper, spread into tiles with none, bilinearly interpolated between tile
/// centres. Every output pixel is opaque. O(pixels).
#[must_use]
pub fn tile_background(image: &Rgba, ink: &[bool]) -> Rgba {
    let (w, h) = (image.width, image.height);
    let side = tile_side(h);
    let (tw, th) = (w.div_ceil(side), h.div_ceil(side));
    // Sums of paper pixels per tile.
    let mut sums = vec![[0.0f64; 4]; tw * th];
    let mut counts = vec![0u32; tw * th];
    for y in 0..h {
        for x in 0..w {
            if ink[y * w + x] {
                continue;
            }
            let t = (y / side) * tw + x / side;
            let p = image.get(x, y);
            for c in 0..4 {
                sums[t][c] += f64::from(p[c]);
            }
            counts[t] += 1;
        }
    }
    let mut tiles: Vec<Option<[f64; 4]>> = sums
        .iter()
        .zip(&counts)
        .map(|(s, n)| (*n > 0).then(|| s.map(|v| v / f64::from(*n))))
        .collect();
    // A tile with no paper borrows from its known neighbours, ring by ring.
    // Each pass is O(tiles) and fills at least one tile while any is known.
    if tiles.iter().all(Option::is_none) {
        // All ink: nothing to model. White paper is the honest default.
        tiles.iter_mut().for_each(|t| *t = Some([255.0; 4]));
    }
    while tiles.iter().any(Option::is_none) {
        let snapshot = tiles.clone();
        for ty in 0..th {
            for tx in 0..tw {
                if snapshot[ty * tw + tx].is_some() {
                    continue;
                }
                let mut acc = [0.0; 4];
                let mut n = 0.0;
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (tx as i64 + dx, ty as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= tw as i64 || ny >= th as i64 {
                        continue;
                    }
                    if let Some(v) = snapshot[ny as usize * tw + nx as usize] {
                        for c in 0..4 {
                            acc[c] += v[c];
                        }
                        n += 1.0;
                    }
                }
                if n > 0.0 {
                    tiles[ty * tw + tx] = Some(acc.map(|v| v / n));
                }
            }
        }
    }
    let tiles: Vec<[f64; 4]> = tiles.into_iter().map(|t| t.unwrap_or([255.0; 4])).collect();
    let mut out = Rgba::filled(w, h, [0, 0, 0, 255]);
    let half = side as f64 / 2.0;
    for y in 0..h {
        // Position among tile centres.
        let fy = ((y as f64 + 0.5 - half) / side as f64).clamp(0.0, (th - 1) as f64);
        let (y0, y1) = (fy.floor() as usize, (fy.floor() as usize + 1).min(th - 1));
        let wy = fy - y0 as f64;
        for x in 0..w {
            let fx = ((x as f64 + 0.5 - half) / side as f64).clamp(0.0, (tw - 1) as f64);
            let (x0, x1) = (fx.floor() as usize, (fx.floor() as usize + 1).min(tw - 1));
            let wx = fx - x0 as f64;
            let mut p = [0u8; 4];
            for c in 0..4 {
                let top = tiles[y0 * tw + x0][c] * (1.0 - wx) + tiles[y0 * tw + x1][c] * wx;
                let bottom = tiles[y1 * tw + x0][c] * (1.0 - wx) + tiles[y1 * tw + x1][c] * wx;
                p[c] = (top * (1.0 - wy) + bottom * wy).round().clamp(0.0, 255.0) as u8;
            }
            p[3] = 255;
            out.set(x, y, p);
        }
    }
    out
}

/// How far `image`'s paper pixels stray from `model` (luma RMS). O(pixels).
fn paper_residual(image: &Rgba, model: &Rgba, ink: &[bool]) -> f64 {
    let (mut sum, mut n) = (0.0, 0.0);
    for (i, is_ink) in ink.iter().enumerate() {
        if *is_ink {
            continue;
        }
        let a = f64::from(luma(pixel(image, i)));
        let b = f64::from(luma(pixel(model, i)));
        sum += (a - b).powi(2);
        n += 1.0;
    }
    if n == 0.0 { 0.0 } else { (sum / n).sqrt() }
}

fn pixel(image: &Rgba, i: usize) -> [u8; 4] {
    let p = &image.pixels[i * 4..i * 4 + 4];
    [p[0], p[1], p[2], p[3]]
}

/// `image` with its `ink` pixels solved from the paper around them by
/// Laplacian relaxation, started from `start`; paper pixels are kept.
/// O(ink pixels × [`INPAINT_SWEEPS`]).
#[must_use]
pub fn inpaint(image: &Rgba, ink: &[bool], start: &Rgba) -> Rgba {
    let (w, h) = (image.width, image.height);
    let mut values: Vec<[f64; 4]> = (0..w * h)
        .map(|i| {
            let p = if ink[i] {
                pixel(start, i)
            } else {
                pixel(image, i)
            };
            p.map(f64::from)
        })
        .collect();
    let holes: Vec<usize> = (0..w * h).filter(|i| ink[*i]).collect();
    for _ in 0..INPAINT_SWEEPS {
        for &i in &holes {
            let (x, y) = (i % w, i / w);
            let mut acc = [0.0; 4];
            let mut n = 0.0;
            if x > 0 {
                add(&mut acc, values[i - 1]);
                n += 1.0;
            }
            if x + 1 < w {
                add(&mut acc, values[i + 1]);
                n += 1.0;
            }
            if y > 0 {
                add(&mut acc, values[i - w]);
                n += 1.0;
            }
            if y + 1 < h {
                add(&mut acc, values[i + w]);
                n += 1.0;
            }
            if n > 0.0 {
                values[i] = acc.map(|v| v / n);
            }
        }
    }
    let mut out = Rgba::filled(w, h, [0, 0, 0, 255]);
    for (i, v) in values.iter().enumerate() {
        let mut p = v.map(|c| c.round().clamp(0.0, 255.0) as u8);
        p[3] = 255;
        out.set(i % w, i / w, p);
    }
    out
}

fn add(acc: &mut [f64; 4], v: [f64; 4]) {
    for c in 0..4 {
        acc[c] += v[c];
    }
}

/// A box blur of `image` with `radius` (separable, prefix sums): O(pixels).
#[must_use]
pub fn box_blur(image: &Rgba, radius: usize) -> Rgba {
    let (w, h) = (image.width, image.height);
    let pass = |src: &Rgba, horizontal: bool| {
        let mut out = src.clone();
        let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
        for o in 0..outer {
            let at = |i: usize| if horizontal { (i, o) } else { (o, i) };
            let mut prefix = vec![[0.0f64; 4]; inner + 1];
            for i in 0..inner {
                let (x, y) = at(i);
                let p = src.get(x, y);
                for c in 0..4 {
                    prefix[i + 1][c] = prefix[i][c] + f64::from(p[c]);
                }
            }
            for i in 0..inner {
                let (a, b) = (i.saturating_sub(radius), (i + radius + 1).min(inner));
                let n = (b - a) as f64;
                let mut p = [0u8; 4];
                for c in 0..4 {
                    p[c] = ((prefix[b][c] - prefix[a][c]) / n).round() as u8;
                }
                let (x, y) = at(i);
                out.set(x, y, p);
            }
        }
        out
    };
    // Three box passes approximate a Gaussian.
    let mut out = image.clone();
    for _ in 0..3 {
        out = pass(&pass(&out, true), false);
    }
    out
}

/// `image` as blocks of `side` pixels, each its mean. O(pixels).
#[must_use]
pub fn pixelate(image: &Rgba, side: usize) -> Rgba {
    let side = side.max(1);
    let (w, h) = (image.width, image.height);
    let mut out = image.clone();
    for by in (0..h).step_by(side) {
        for bx in (0..w).step_by(side) {
            let (x1, y1) = ((bx + side).min(w), (by + side).min(h));
            let mut acc = [0.0; 4];
            let mut n = 0.0;
            for y in by..y1 {
                for x in bx..x1 {
                    add(&mut acc, image.get(x, y).map(f64::from));
                    n += 1.0;
                }
            }
            let p = acc.map(|v| (v / n).round() as u8);
            for y in by..y1 {
                for x in bx..x1 {
                    out.set(x, y, p);
                }
            }
        }
    }
    out
}

/// What the line's ink says: colour, height, stroke and weight.
/// O(box pixels).
#[must_use]
pub fn line_style(image: &Rgba, ink: &[bool]) -> LineStyle {
    let (w, h) = (image.width, image.height);
    let mut rows = (usize::MAX, 0usize);
    let mut lumas = Vec::new();
    for (i, is_ink) in ink.iter().enumerate() {
        if *is_ink {
            let y = i / w;
            rows = (rows.0.min(y), rows.1.max(y));
            lumas.push(luma(pixel(image, i)));
        }
    }
    if lumas.is_empty() {
        return LineStyle {
            ink: [0, 0, 0],
            ink_height: 0,
            ink_top: 0,
            stroke: 0.0,
            bold: false,
            face: Face::Sans,
        };
    }
    // The core of the ink, not its anti-aliased rim: the half of the ink
    // pixels furthest from the paper.
    let mut sorted = lumas.clone();
    sorted.sort_unstable();
    let paper_luma = {
        let paper: Vec<u8> = (0..w * h)
            .filter(|i| !ink[*i])
            .map(|i| luma(pixel(image, i)))
            .collect();
        paper.iter().map(|v| f64::from(*v)).sum::<f64>() / paper.len().max(1) as f64
    };
    let paper_is_light = paper_luma >= f64::from(sorted[sorted.len() / 2]);
    let cut = if paper_is_light {
        sorted[sorted.len() / 2]
    } else {
        sorted[(sorted.len() - 1) / 2]
    };
    let mut channels = [Vec::new(), Vec::new(), Vec::new()];
    for (i, is_ink) in ink.iter().enumerate() {
        if !*is_ink {
            continue;
        }
        let p = pixel(image, i);
        let core = if paper_is_light {
            luma(p) <= cut
        } else {
            luma(p) >= cut
        };
        if core {
            for c in 0..3 {
                channels[c].push(p[c]);
            }
        }
    }
    let median = |v: &mut Vec<u8>| {
        v.sort_unstable();
        v.get(v.len() / 2).copied().unwrap_or(0)
    };
    let ink_color = [
        median(&mut channels[0]),
        median(&mut channels[1]),
        median(&mut channels[2]),
    ];
    let ink_height = rows.1 + 1 - rows.0;
    // A stroke is measured on the pixels at least half covered (luma past
    // the midpoint of ink and paper), so a scanner's soft rim does not widen
    // every stem by a pixel and make regular text read as bold.
    let ink_luma = f64::from(cut);
    let half = (ink_luma + paper_luma) / 2.0;
    let covered = |i: usize| {
        let v = f64::from(luma(pixel(image, i)));
        ink[i] && if paper_is_light { v <= half } else { v >= half }
    };
    // Horizontal runs of ink shorter than half the line: stems and bowls,
    // not the bars and rules.
    let mut runs = Vec::new();
    for y in 0..h {
        let mut run = 0usize;
        for x in 0..=w {
            if x < w && covered(y * w + x) {
                run += 1;
            } else {
                if run > 0 && run * 2 < ink_height {
                    runs.push(run);
                }
                run = 0;
            }
        }
    }
    runs.sort_unstable();
    let stroke = runs.get(runs.len() / 2).copied().unwrap_or(0) as f64;
    // Horizontal strokes: vertical runs of ink shorter than a third of the
    // line (bars, serifs and bowls' tops, not the stems).
    let mut bars = Vec::new();
    for x in 0..w {
        let mut run = 0usize;
        for y in 0..=h {
            if y < h && covered(y * w + x) {
                run += 1;
            } else {
                if run > 0 && run * 3 < ink_height {
                    bars.push(run);
                }
                run = 0;
            }
        }
    }
    bars.sort_unstable();
    let bar = bars.get(bars.len() / 2).copied().unwrap_or(0) as f64;
    // Glyph starts: the first column of each run of inked columns.
    let mut starts = Vec::new();
    let mut inked_before = false;
    for x in 0..w {
        let inked = (0..h).any(|y| covered(y * w + x));
        if inked && !inked_before {
            starts.push(x as f64);
        }
        inked_before = inked;
    }
    let pitches: Vec<f64> = starts.windows(2).map(|p| p[1] - p[0]).collect();
    let cv = if pitches.len() >= 5 {
        let mean = pitches.iter().sum::<f64>() / pitches.len() as f64;
        let var = pitches.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / pitches.len() as f64;
        if mean > 0.0 {
            var.sqrt() / mean
        } else {
            f64::INFINITY
        }
    } else {
        f64::INFINITY
    };
    let face = if cv < MONO_PITCH_CV {
        Face::Mono
    } else if bar > 0.0 && stroke / bar >= SERIF_CONTRAST {
        Face::Serif
    } else {
        Face::Sans
    };
    LineStyle {
        face,
        ink: ink_color,
        ink_height,
        ink_top: rows.0,
        stroke,
        // Measured on scanner and browser rasters, a regular sans line's
        // median stem reads up to about 0.15 of its ink height once bowls and
        // anti-aliasing are counted; only a clearly heavier stem is bold, as a
        // wrong bold reads worse than a missed one.
        bold: ink_height > 0 && stroke / ink_height as f64 >= BOLD_STEM,
    }
}

/// The patch for `geometry` in `source`: the box without its text by `mode`,
/// and the text's measured style. O(box pixels), plus
/// O(ink × [`INPAINT_SWEEPS`]) for a textured box.
///
/// # Errors
///
/// As [`sample_box`].
pub fn make_patch(
    source: &Rgba,
    geometry: &BoxGeometry,
    mode: FillMode,
) -> Result<Patch, PatchError> {
    let boxed = sample_box(source, geometry)?;
    let (w, h) = (boxed.width, boxed.height);
    let core = ink_mask(&boxed);
    let ink_pixels = core.iter().filter(|v| **v).count();
    let style = line_style(&boxed, &core);
    // The anti-aliased rim: 1 pixel for a small line, 2 from 24 pixels tall.
    let radius = if h >= 24 { 2 } else { 1 };
    let ink = dilate(&core, w, h, radius);
    // Every mode starts from the box without its ink: no mode reads an ink
    // pixel's own colour, so the text's colour never smears into a blur or a
    // block mean (owner rule, 2026-11-01).
    let (clean, fill_method) = ink_free(&boxed, &ink);
    let (mut image, method) = match mode {
        FillMode::Fill => (clean, fill_method),
        FillMode::Blur => (box_blur(&clean, (h / 4).max(2)), Method::Filter),
        FillMode::Pixelate => (pixelate(&clean, (h / 3).max(3)), Method::Filter),
    };
    if method == Method::Tiles {
        feather(&mut image, &ink);
    }
    Ok(Patch {
        image,
        method,
        style,
        ink_pixels,
    })
}

/// `boxed` with its `ink` pixels replaced from the paper alone: the tile
/// model of the paper pixels, or, on textured paper, the paper kept and the
/// ink inpainted from it. The base of every [`FillMode`]. O(box pixels), plus
/// O(ink × [`INPAINT_SWEEPS`]) when inpainted.
fn ink_free(boxed: &Rgba, ink: &[bool]) -> (Rgba, Method) {
    let model = tile_background(boxed, ink);
    if paper_residual(boxed, &model, ink) > TEXTURE_RMS {
        (inpaint(boxed, ink, &model), Method::Inpaint)
    } else {
        (model, Method::Tiles)
    }
}

/// Ramps the alpha of the patch's outer [`FEATHER`] pixels, where they are
/// paper, so the patch fades into the picture under it. O(pixels).
fn feather(image: &mut Rgba, ink: &[bool]) {
    let (w, h) = (image.width, image.height);
    if w <= 2 * FEATHER + 1 || h <= 2 * FEATHER + 1 {
        return;
    }
    for y in 0..h {
        for x in 0..w {
            let d = x.min(y).min(w - 1 - x).min(h - 1 - y);
            if d >= FEATHER || ink[y * w + x] {
                continue;
            }
            let i = (y * w + x) * 4 + 3;
            image.pixels[i] = (255 * (d + 1) / (FEATHER + 1)) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page-like picture: a horizontal gradient of paper (a scanner's
    /// uneven light) with dark "glyphs" (vertical stems and a bar) drawn on it.
    fn gradient_with_text(width: usize, height: usize, ink: [u8; 4], stem: usize) -> (Rgba, Rgba) {
        let mut paper = Rgba::filled(width, height, [0, 0, 0, 255]);
        for y in 0..height {
            for x in 0..width {
                let v = 236.0 + 14.0 * x as f64 / width as f64 - 6.0 * y as f64 / height as f64;
                let v = v.round() as u8;
                paper.set(x, y, [v, v.saturating_sub(4), v.saturating_sub(10), 255]);
            }
        }
        let mut page = paper.clone();
        let (top, bottom) = (height / 4, height * 3 / 4);
        let mut x = width / 10;
        while x + stem < width * 9 / 10 {
            for yy in top..bottom {
                for xx in x..x + stem {
                    page.set(xx, yy, ink);
                }
            }
            // A soft rim, as a scanner leaves.
            for yy in top..bottom {
                let rim = page.get(x + stem, yy);
                let mix = |a: u8, b: u8| ((2 * u16::from(a) + u16::from(b)) / 3) as u8;
                page.set(
                    x + stem,
                    yy,
                    [
                        mix(rim[0], ink[0]),
                        mix(rim[1], ink[1]),
                        mix(rim[2], ink[2]),
                        255,
                    ],
                );
            }
            x += stem * 4;
        }
        (page, paper)
    }

    fn geometry_of(width: usize, height: usize) -> BoxGeometry {
        BoxGeometry {
            cx: width as f64 / 2.0,
            cy: height as f64 / 2.0,
            width: width as f64,
            height: height as f64,
            rotation_degrees: 0.0,
        }
    }

    fn max_channel_error(a: &Rgba, b: &Rgba) -> u8 {
        a.pixels
            .chunks_exact(4)
            .zip(b.pixels.chunks_exact(4))
            .map(|(p, q)| (0..3).map(|c| p[c].abs_diff(q[c])).max().unwrap_or(0))
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn otsu_splits_two_tones_between_them() {
        let mut values = vec![30u8; 100];
        values.extend(vec![220u8; 400]);
        let t = otsu(&values);
        assert!((30..220).contains(&t), "threshold {t}");
    }

    #[test]
    fn a_filled_patch_is_the_true_background_within_tolerance_and_holds_no_ink() {
        let (page, paper) = gradient_with_text(320, 40, [20, 24, 90, 255], 4);
        let patch = make_patch(&page, &geometry_of(320, 40), FillMode::Fill).expect("a patch");
        assert!(patch.ink_pixels > 1000, "the stems were found as ink");
        // Within 6 levels of the true paper everywhere.
        let error = max_channel_error(&patch.image, &paper);
        assert!(
            error <= 6,
            "the patch strays {error} levels from the true background"
        );
        // No pixel anywhere near the ink's colour.
        for p in patch.image.pixels.chunks_exact(4) {
            assert!(
                p[2] > 150 && p[0] > 150,
                "an ink-coloured pixel survived in the patch: {p:?}"
            );
        }
        // Plain paper is the smooth model's, not the textured fallback's.
        assert_eq!(patch.method, Method::Tiles);
    }

    /// The owner's rule, on the model itself: no ink pixel enters a tile's
    /// average. With the ink averaged in, every tile across a stem darkens.
    #[test]
    fn the_tile_model_averages_paper_pixels_only() {
        let (page, paper) = gradient_with_text(320, 40, [0, 0, 0, 255], 4);
        let boxed = sample_box(&page, &geometry_of(320, 40)).unwrap();
        let ink = dilate(&ink_mask(&boxed), 320, 40, 2);
        let model = tile_background(&boxed, &ink);
        let error = max_channel_error(&model, &paper);
        assert!(
            error <= 6,
            "the tile model strays {error} levels from the paper"
        );
    }

    #[test]
    fn a_line_box_on_a_dark_band_takes_the_light_text_as_ink() {
        let (page, _) = gradient_with_text(200, 32, [250, 250, 250, 255], 3);
        // Invert: dark paper, light text.
        let mut dark = page.clone();
        for p in dark.pixels.chunks_exact_mut(4) {
            for c in 0..3 {
                p[c] = 255 - p[c];
            }
        }
        let patch = make_patch(&dark, &geometry_of(200, 32), FillMode::Fill).expect("a patch");
        for p in patch.image.pixels.chunks_exact(4) {
            assert!(p[0] < 60, "light ink left in a dark patch: {p:?}");
        }
    }

    #[test]
    fn a_textured_background_is_inpainted_keeping_its_grain() {
        // Paper grain: a checker of ±12 levels, too rough for the tile model.
        let mut paper = Rgba::filled(160, 32, [0, 0, 0, 255]);
        for y in 0..32 {
            for x in 0..160 {
                let v = if (x / 2 + y / 2) % 2 == 0 { 217 } else { 193 };
                paper.set(x, y, [v, v, v, 255]);
            }
        }
        let mut page = paper.clone();
        for y in 8..24 {
            for x in (40..44).chain(80..84).chain(110..114) {
                page.set(x, y, [10, 10, 10, 255]);
            }
        }
        let patch = make_patch(&page, &geometry_of(160, 32), FillMode::Fill).expect("a patch");
        assert_eq!(patch.method, Method::Inpaint);
        // Far from the stroke the grain is the page's own.
        assert_eq!(patch.image.get(120, 10), paper.get(120, 10));
        // The stroke is gone: nothing there is near black.
        for y in 8..24 {
            for x in 40..44 {
                assert!(patch.image.get(x, y)[0] > 150, "the stroke survived");
            }
        }
    }

    #[test]
    fn a_skewed_box_is_sampled_along_its_own_axes() {
        // A tilted bar: sampled in its own frame it is a horizontal band.
        let mut page = Rgba::filled(200, 200, [240, 240, 240, 255]);
        let angle = 4f64.to_radians();
        for y in 0..200 {
            for x in 0..200 {
                let (dx, dy) = (x as f64 - 100.0, y as f64 - 100.0);
                let along = -dx * angle.sin() + dy * angle.cos();
                if along.abs() < 3.0 && dx.abs() < 80.0 {
                    page.set(x, y, [0, 0, 0, 255]);
                }
            }
        }
        let geometry = BoxGeometry {
            cx: 100.0,
            cy: 100.0,
            width: 150.0,
            height: 20.0,
            rotation_degrees: 4.0,
        };
        let boxed = sample_box(&page, &geometry).expect("sampled");
        // The band sits on the box's middle rows from end to end.
        for x in [10, 75, 140] {
            assert!(
                boxed.get(x, 10)[0] < 80,
                "the bar is not on the middle row at {x}"
            );
            assert!(boxed.get(x, 2)[0] > 200, "the bar strays to the top at {x}");
        }
    }

    #[test]
    fn blur_and_pixelate_keep_the_box_size_and_soften_the_ink() {
        let (page, _) = gradient_with_text(120, 24, [0, 0, 0, 255], 3);
        for mode in [FillMode::Blur, FillMode::Pixelate] {
            let patch = make_patch(&page, &geometry_of(120, 24), mode).expect("a patch");
            assert_eq!((patch.image.width, patch.image.height), (120, 24));
            assert_eq!(patch.method, Method::Filter);
            let darkest = patch
                .image
                .pixels
                .chunks_exact(4)
                .map(|p| p[0])
                .min()
                .unwrap_or(0);
            assert!(darkest > 40, "{mode:?} left solid ink ({darkest})");
        }
    }

    /// The owner's rule for every mode: no ink pixel's colour enters any
    /// average. On a gradient paper with dark-blue stems, the patch is the
    /// true paper within `tolerance`, holds no pixel near the ink's colour,
    /// and is no darker on average than the paper (the text adds no
    /// darkness). Fed the box with its ink, a blur or a block mean darkens by
    /// a quarter of the ink's contrast and fails all but the first.
    fn assert_ink_never_averaged(mode: FillMode, tolerance: u8) {
        let ink = [20, 24, 90, 255];
        let (page, paper) = gradient_with_text(320, 40, ink, 4);
        let patch = make_patch(&page, &geometry_of(320, 40), mode).expect("a patch");
        assert!(
            patch.ink_pixels > 1000,
            "{mode:?}: the stems were found as ink"
        );
        for p in patch.image.pixels.chunks_exact(4) {
            let near = (0..3).map(|c| p[c].abs_diff(ink[c])).max().unwrap_or(0);
            assert!(
                near > 100,
                "{mode:?}: a pixel within {near} levels of the ink: {p:?}"
            );
        }
        let error = max_channel_error(&patch.image, &paper);
        assert!(
            error <= tolerance,
            "{mode:?}: the patch strays {error} levels from the true background"
        );
        let mean_luma = |image: &Rgba| {
            let n = (image.width * image.height) as f64;
            image
                .pixels
                .chunks_exact(4)
                .map(|p| f64::from(luma([p[0], p[1], p[2], 255])))
                .sum::<f64>()
                / n
        };
        let (got, truth) = (mean_luma(&patch.image), mean_luma(&paper));
        assert!(
            got >= truth - 2.0,
            "{mode:?}: the text darkened the patch (mean luma {got:.1}, paper {truth:.1})"
        );
    }

    #[test]
    fn fill_averages_no_ink() {
        assert_ink_never_averaged(FillMode::Fill, 6);
    }

    #[test]
    fn blur_blurs_the_ink_free_background_only() {
        assert_ink_never_averaged(FillMode::Blur, 6);
    }

    #[test]
    fn pixelate_averages_the_ink_free_background_only() {
        assert_ink_never_averaged(FillMode::Pixelate, 6);
    }

    #[test]
    fn the_line_style_reads_colour_height_and_weight() {
        let (regular, _) = gradient_with_text(400, 50, [180, 20, 20, 255], 2);
        let style = line_style(&sample_box(&regular, &geometry_of(400, 50)).unwrap(), &{
            let b = sample_box(&regular, &geometry_of(400, 50)).unwrap();
            ink_mask(&b)
        });
        assert!(
            style.ink[0] > 150 && style.ink[1] < 60,
            "ink colour {:?}",
            style.ink
        );
        assert_eq!(style.ink_height, 25);
        assert!(!style.bold, "2-pixel stems on a 25-pixel line are regular");
        let (heavy, _) = gradient_with_text(400, 50, [0, 0, 0, 255], 5);
        let b = sample_box(&heavy, &geometry_of(400, 50)).unwrap();
        let style = line_style(&b, &ink_mask(&b));
        assert!(
            style.bold,
            "5-pixel stems on a 25-pixel line are bold ({})",
            style.stroke
        );
    }

    /// Glyph-like marks at `starts`: a stem `stem` wide and a bar across the
    /// top `bar` thick, on white.
    fn glyphs(starts: &[usize], stem: usize, bar: usize) -> Rgba {
        let mut page = Rgba::filled(400, 40, [250, 250, 250, 255]);
        for &x in starts {
            for y in 10..30 {
                for xx in x..x + stem {
                    page.set(xx, y, [0, 0, 0, 255]);
                }
            }
            for y in 10..10 + bar {
                for xx in x..x + 9 {
                    page.set(xx, y, [0, 0, 0, 255]);
                }
            }
        }
        page
    }

    #[test]
    fn the_face_is_read_from_pitch_and_stroke_contrast() {
        let uneven = [10, 24, 33, 51, 60, 81, 92, 113, 121, 140];
        let style = |page: Rgba| {
            let b = sample_box(&page, &geometry_of(400, 40)).unwrap();
            line_style(&b, &ink_mask(&b))
        };
        assert_eq!(
            style(glyphs(&uneven, 4, 1)).face,
            Face::Serif,
            "heavy stems, hairline bars"
        );
        assert_eq!(
            style(glyphs(&uneven, 3, 3)).face,
            Face::Sans,
            "even strokes"
        );
        let even: Vec<usize> = (0..10).map(|i| 10 + i * 16).collect();
        assert_eq!(style(glyphs(&even, 3, 3)).face, Face::Mono, "an even pitch");
        assert_eq!(Face::Serif.family(), "Liberation Serif");
    }

    #[test]
    fn a_blank_box_has_no_ink_and_fills_as_itself() {
        let paper = Rgba::filled(64, 16, [250, 248, 240, 255]);
        let patch = make_patch(&paper, &geometry_of(64, 16), FillMode::Fill).expect("a patch");
        assert_eq!(patch.ink_pixels, 0);
        assert!(max_channel_error(&patch.image, &paper) <= 1);
    }

    #[test]
    fn a_patch_round_trips_through_png() {
        let (page, _) = gradient_with_text(60, 20, [0, 0, 0, 255], 2);
        let patch = make_patch(&page, &geometry_of(60, 20), FillMode::Fill).expect("a patch");
        let png = encode_png(&patch.image).expect("encoded");
        assert_eq!(decode(&png).expect("decoded"), patch.image);
    }

    #[test]
    fn a_box_outside_the_picture_or_too_large_is_refused() {
        let page = Rgba::filled(10, 10, [255; 4]);
        let outside = BoxGeometry {
            cx: 500.0,
            cy: 500.0,
            width: 10.0,
            height: 10.0,
            rotation_degrees: 0.0,
        };
        assert_eq!(
            make_patch(&page, &outside, FillMode::Fill),
            Err(PatchError::Empty)
        );
        let huge = BoxGeometry {
            width: 5000.0,
            height: 5000.0,
            ..outside
        };
        assert_eq!(
            make_patch(&page, &huge, FillMode::Fill),
            Err(PatchError::TooLarge)
        );
    }

    /// Complexity guard (SKILL §8): the work is linear in box area. Doubling
    /// the box doubles the ink found and the pixels made, nothing more.
    #[test]
    fn the_work_scales_with_the_box_area() {
        let (small, _) = gradient_with_text(200, 30, [0, 0, 0, 255], 3);
        let (large, _) = gradient_with_text(400, 30, [0, 0, 0, 255], 3);
        let a = make_patch(&small, &geometry_of(200, 30), FillMode::Fill).unwrap();
        let b = make_patch(&large, &geometry_of(400, 30), FillMode::Fill).unwrap();
        assert_eq!(b.image.pixels.len(), 2 * a.image.pixels.len());
        let ratio = b.ink_pixels as f64 / a.ink_pixels as f64;
        assert!((1.8..2.3).contains(&ratio), "ink ratio {ratio}");
    }
}
