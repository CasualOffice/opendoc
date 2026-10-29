//! PaddleOCR PP-OCRv5 on `rten` (feature `paddle`, the default provider;
//! owner decision 2026-10-29, `docs/182` §20.2 and §20a).
//!
//! The weights are PaddlePaddle's own Apache-2.0 releases
//! (`huggingface.co/PaddlePaddle/PP-OCRv5_mobile_det`,
//! `.../PP-OCRv5_mobile_rec`, the per-script `*_PP-OCRv5_mobile_rec`
//! language packs and `.../PP-LCNet_x1_0_textline_ori`), converted by the
//! host to ONNX with PaddlePaddle's `paddle2onnx`. This module never fetches
//! or converts anything: the host passes the detection model's bytes, one or
//! more [`RecognitionPack`]s (recognition model bytes plus that model's
//! character dictionary) and, optionally, the text-line orientation model.
//!
//! The pipeline mirrors PaddleOCR's own pre- and post-processing, with the
//! parameters of the models' `inference.yml` fixed in code:
//!
//! - **Detection (DB).** The raster, BGR, normalised with ImageNet mean and
//!   std ([`detection_input`]), its sides rounded to multiples of 32 (long
//!   side capped at [`PaddleOcr::MAX_DET_SIDE`]); the probability map is
//!   thresholded at 0.3; each 8-connected component is a text candidate,
//!   kept when its mean probability is at least 0.6 (`box_thresh`) and its
//!   short side at least 3 px.
//! - **Rotated boxes and deskew.** Each candidate's box is fitted along the
//!   principal axis of its pixels (second moments), PaddleOCR's minimum-area
//!   rectangle for the convex blobs DB produces. The raster's skew is the
//!   median angle of its elongated candidates (aspect at least 3); a short
//!   candidate, or one within a degree of that median, takes the median, so
//!   the lines of one skewed scan share one angle. Under a quarter of a degree
//!   is straight. The box is then expanded by `area x 1.5 / perimeter`
//!   (`unclip_ratio`, the polygon offset PaddleOCR applies with pyclipper).
//!   Recognition reads each box along its own angle (PaddleOCR's
//!   perspective crop of a rectangle), so a skewed scan reads as a straight
//!   one; nothing is resampled twice.
//! - **Orientation (optional, `PP-LCNet_x1_0_textline_ori`).** Each box,
//!   stretched to 160 x 80, BGR, ImageNet-normalised; a line the classifier
//!   gives `180_degree` with probability above 0.9 is read turned half way.
//! - **Recognition (CTC).** Each box resized to height 48 keeping its aspect,
//!   normalised to `[-1, 1]`, padded to at least 320 wide; the model's
//!   per-frame class probabilities are decoded greedily (blank 0, then the
//!   dictionary, then a space when the model has one more class than the
//!   dictionary), repeats collapsed. A character's confidence is its best
//!   frame's probability; a word's is the geometric mean of its characters'.
//!
//! A turned line is reported in its upright frame with its angle and pivot
//! ([`OcrLine::rotation_degrees`], [`OcrLine::pivot`]); an axis-aligned line
//! is reported exactly as before rotated boxes existed.
//!
//! **Several language packs.** Each line is read by every pack; the reading
//! with the highest mean character probability is kept (ties go to the pack
//! supplied first). The cost is one recognition run per pack per line.
//!
//! **Not built.** Vertical text (a candidate whose principal axis is nearer
//! vertical than horizontal is read as an axis-aligned box, as before), and
//! the 90/270 degree page orientation model (`PP-LCNet_x1_0_doc_ori`).
//!
//! Verified against onnxruntime 1.19.2 on the converted graphs (`docs/182`
//! §20.2, §20a): `rten` loads and runs all four.

use rten::Model;
use rten_tensor::prelude::*;
use rten_tensor::{NdTensor, Tensor};

use crate::digest::sha256_hex;
use crate::provider::check_pixels;
use crate::{
    Determinism, Interrupt, OcrBox, OcrError, OcrLimits, OcrLine, OcrPage, OcrProvider, OcrWord,
    ProviderManifest, Raster, Stop,
};

/// ImageNet mean, in R, G, B order (PaddleOCR's `NormalizeImage` applies it
/// to channels in the image's order, which is BGR: see [`detection_input`]).
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
/// ImageNet standard deviation, same order as [`MEAN`].
const STD: [f32; 3] = [0.229, 0.224, 0.225];
/// Output channel `i` reads RGB channel `BGR[i]`: PaddleOCR decodes images
/// with OpenCV, which yields BGR.
const BGR: [usize; 3] = [2, 1, 0];
const DET_THRESH: f32 = 0.3;
const BOX_THRESH: f32 = 0.6;
const UNCLIP_RATIO: f32 = 1.5;
const MIN_SIZE: f32 = 3.0;
const REC_HEIGHT: usize = 48;
const REC_MIN_WIDTH: usize = 320;
const REC_MAX_WIDTH: usize = 3200;
const MAX_CANDIDATES: usize = 1000;
/// The orientation classifier's input (`ResizeImage size: [160, 80]`).
const ORI_WIDTH: usize = 160;
const ORI_HEIGHT: usize = 80;
/// PaddleOCR's `cls_thresh`: a line is turned only on a confident call.
const ORI_THRESH: f32 = 0.9;
/// A candidate this elongated (long side over short) votes on the skew.
const ELONGATED: f32 = 3.0;
/// A line within this many degrees of the skew takes the skew.
const SNAP_DEGREES: f32 = 1.0;
/// A skew under this is straight.
const STRAIGHT_DEGREES: f32 = 0.25;
/// Beyond this a principal axis is nearer vertical: read axis-aligned.
const MAX_LINE_DEGREES: f32 = 45.0;

/// One decoded label: its text (`None` for a separator), its x extent in
/// the line's upright frame, its probability.
type Decoded = (Option<String>, f32, f32, f32);

/// One recognition model and its dictionary: a language pack.
#[derive(Clone)]
pub struct RecognitionPack {
    /// The host's name for it (e.g. `ch`, `latin`, `korean`), recorded in the
    /// manifest.
    pub name: String,
    /// The ONNX bytes.
    pub model: Vec<u8>,
    /// The character dictionary: one entry per line, in label order (the
    /// `character_dict` of the model's `inference.yml`, or PaddleOCR's
    /// `*_dict.txt`).
    pub dictionary: String,
}

impl std::fmt::Debug for RecognitionPack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecognitionPack")
            .field("name", &self.name)
            .field("model_bytes", &self.model.len())
            .finish_non_exhaustive()
    }
}

struct Recognizer {
    model: Model,
    /// Label `i` decodes to `labels[i - 1]`; `None` is a word separator.
    /// An entry may be several code points (PP-OCRv5's flag emoji).
    labels: Vec<Option<String>>,
}

/// PP-OCRv5 over host-supplied ONNX models.
pub struct PaddleOcr {
    detector: Model,
    recognizers: Vec<Recognizer>,
    orientation: Option<Model>,
    models: Vec<(&'static str, String)>,
    names: Vec<String>,
}

impl std::fmt::Debug for PaddleOcr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaddleOcr")
            .field("packs", &self.names)
            .field("orientation", &self.orientation.is_some())
            .field("models", &self.models)
            .finish_non_exhaustive()
    }
}

fn model_error(error: impl std::fmt::Display) -> OcrError {
    OcrError::Model(error.to_string())
}

/// A text line's box: a rectangle `w` x `h` centred on `(cx, cy)`, its long
/// side turned `angle` degrees clockwise (y down), read upside down when
/// `flip`. In raster pixels once [`PaddleOcr::detect`] has mapped it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Frame {
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    angle: f32,
    flip: bool,
}

impl Frame {
    /// The left edge of the upright frame.
    fn x0(&self) -> f32 {
        self.cx - self.w / 2.0
    }

    /// The top edge of the upright frame.
    fn y0(&self) -> f32 {
        self.cy - self.h / 2.0
    }

    /// The turn a reader of the raster sees: the angle, plus 180 when read
    /// upside down, in `(-180, 180]`.
    fn rotation(&self) -> f32 {
        let r = self.angle + if self.flip { 180.0 } else { 0.0 };
        if r > 180.0 { r - 360.0 } else { r }
    }

    /// The raster point at upright-frame offset `(u, v)` from the centre.
    fn point(&self, u: f32, v: f32) -> (f32, f32) {
        let (u, v) = if self.flip { (-u, -v) } else { (u, v) };
        if self.angle == 0.0 {
            return (self.cx + u, self.cy + v);
        }
        let (sin, cos) = self.angle.to_radians().sin_cos();
        (self.cx + u * cos - v * sin, self.cy + u * sin + v * cos)
    }

    /// Whether this frame is exactly the axis-aligned box of earlier
    /// releases (no turn, no flip).
    fn axis_aligned(&self) -> bool {
        self.angle == 0.0 && !self.flip
    }
}

/// One component of the thresholded map: its pixel indices (row-major).
struct Blob {
    pixels: Vec<u32>,
}

/// 8-connected components of `map > DET_THRESH` whose mean probability
/// reaches `BOX_THRESH`. O(pixels) with an explicit stack.
fn blobs(map: &[f32], width: usize, height: usize) -> Vec<Blob> {
    let mut seen = vec![false; map.len()];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..map.len() {
        if seen[start] || map[start] <= DET_THRESH {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let mut pixels = Vec::new();
        let mut sum = 0.0f64;
        while let Some(p) = stack.pop() {
            pixels.push(p as u32);
            sum += f64::from(map[p]);
            let (x, y) = (p % width, p / width);
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= width as i64 || ny >= height as i64 {
                        continue;
                    }
                    let q = ny as usize * width + nx as usize;
                    if !seen[q] && map[q] > DET_THRESH {
                        seen[q] = true;
                        stack.push(q);
                    }
                }
            }
        }
        if (sum / pixels.len() as f64) < f64::from(BOX_THRESH) {
            continue;
        }
        out.push(Blob { pixels });
        if out.len() >= MAX_CANDIDATES {
            break;
        }
    }
    out
}

/// The angle (degrees, clockwise, in `(-90, 90]`) of a blob's principal
/// axis, from its second moments. O(blob pixels).
fn principal_angle(blob: &Blob, width: usize) -> f32 {
    let n = blob.pixels.len() as f64;
    let (mut sx, mut sy) = (0.0f64, 0.0f64);
    for &p in &blob.pixels {
        sx += f64::from(p % width as u32);
        sy += f64::from(p / width as u32);
    }
    let (mx, my) = (sx / n, sy / n);
    let (mut xx, mut yy, mut xy) = (0.0f64, 0.0f64, 0.0f64);
    for &p in &blob.pixels {
        let dx = f64::from(p % width as u32) - mx;
        let dy = f64::from(p / width as u32) - my;
        xx += dx * dx;
        yy += dy * dy;
        xy += dx * dy;
    }
    (0.5 * (2.0 * xy).atan2(xx - yy)).to_degrees() as f32
}

/// The blob's rectangle along `angle`: pixel centres projected on the long
/// and short axes. O(blob pixels).
fn fit(blob: &Blob, width: usize, angle: f32) -> Frame {
    let (sin, cos) = f64::from(angle).to_radians().sin_cos();
    let (mut u0, mut u1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for &p in &blob.pixels {
        let x = f64::from(p % width as u32) + 0.5;
        let y = f64::from(p / width as u32) + 0.5;
        let (u, v) = (x * cos + y * sin, -x * sin + y * cos);
        u0 = u0.min(u);
        u1 = u1.max(u);
        v0 = v0.min(v);
        v1 = v1.max(v);
    }
    let (uc, vc) = ((u0 + u1) / 2.0, (v0 + v1) / 2.0);
    Frame {
        cx: (uc * cos - vc * sin) as f32,
        cy: (uc * sin + vc * cos) as f32,
        w: (u1 - u0 + 1.0) as f32,
        h: (v1 - v0 + 1.0) as f32,
        angle,
        flip: false,
    }
}

/// The median of `values` (which it sorts), or `None` when empty.
fn median(values: &mut [f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) / 2.0
    })
}

/// Text candidates in detector-input pixels, unclipped, and the skew.
/// O(map pixels).
fn components(map: &[f32], width: usize, height: usize) -> (Vec<Frame>, f32) {
    let found = blobs(map, width, height);
    // (raw angle, aspect along it) per blob.
    let raw: Vec<(f32, f32)> = found
        .iter()
        .map(|blob| {
            let angle = principal_angle(blob, width);
            let f = fit(blob, width, angle);
            (angle, f.w / f.h.max(1.0))
        })
        .collect();
    let mut votes: Vec<f32> = raw
        .iter()
        .filter(|(angle, aspect)| *aspect >= ELONGATED && angle.abs() <= MAX_LINE_DEGREES)
        .map(|(angle, _)| *angle)
        .collect();
    let skew = median(&mut votes)
        .filter(|s| s.abs() >= STRAIGHT_DEGREES)
        .unwrap_or(0.0);
    let mut out = Vec::with_capacity(found.len());
    for (blob, &(angle, aspect)) in found.iter().zip(&raw) {
        let own = aspect >= ELONGATED
            && angle.abs() <= MAX_LINE_DEGREES
            && (angle - skew).abs() > SNAP_DEGREES;
        let mut chosen = if own { angle } else { skew };
        if chosen.abs() < STRAIGHT_DEGREES {
            chosen = 0.0;
        }
        let mut f = fit(blob, width, chosen);
        if f.w.min(f.h) < MIN_SIZE {
            continue;
        }
        let d = f.w * f.h * UNCLIP_RATIO / (2.0 * (f.w + f.h));
        f.w += 2.0 * d;
        f.h += 2.0 * d;
        if f.w.min(f.h) < MIN_SIZE + 2.0 {
            continue;
        }
        out.push(f);
    }
    (out, skew)
}

/// Bilinear sample of RGB channel `c` at `(fx, fy)` (pixel centres at
/// `+0.5`), clamped to the edge.
fn sample(raster: &Raster, fx: f32, fy: f32, c: usize) -> f32 {
    let (w, h) = (raster.width() as usize, raster.height() as usize);
    let fx = (fx - 0.5).clamp(0.0, (w - 1) as f32);
    let fy = (fy - 0.5).clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let rgb = raster.rgb();
    let p = |x: usize, y: usize| f32::from(rgb[(y * w + x) * 3 + c]);
    let top = p(x0, y0) * (1.0 - tx) + p(x1, y0) * tx;
    let bottom = p(x0, y1) * (1.0 - tx) + p(x1, y1) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// PP-OCRv5's detector input for `raster`: `[3, dh, dw]` row-major, channel
/// 0 = blue, 1 = green, 2 = red (OpenCV's order, which PaddleOCR decodes
/// into), each `(v / 255 - mean) / std` with ImageNet's mean
/// `[0.485, 0.456, 0.406]` and std `[0.229, 0.224, 0.225]` applied **in
/// that channel order** (PaddleOCR's `NormalizeImage` indexes them by the
/// image's channel, so blue gets 0.485). Sides rounded to multiples of 32,
/// long side capped at `max_side`. Returns the data and `(dw, dh)`.
/// O(dw x dh).
pub(crate) fn detection_input(raster: &Raster, max_side: usize) -> (Vec<f32>, usize, usize) {
    let (w, h) = (raster.width() as usize, raster.height() as usize);
    let ratio = (max_side as f32 / w.max(h) as f32).min(1.0);
    let round32 = |v: usize| ((v as f32 * ratio / 32.0).round() as usize).max(1) * 32;
    let (dw, dh) = (round32(w), round32(h));
    let (sx, sy) = (w as f32 / dw as f32, h as f32 / dh as f32);
    let mut input = vec![0f32; 3 * dw * dh];
    for y in 0..dh {
        for x in 0..dw {
            let (fx, fy) = ((x as f32 + 0.5) * sx, (y as f32 + 0.5) * sy);
            for (out_c, &rgb_c) in BGR.iter().enumerate() {
                let v = sample(raster, fx, fy, rgb_c) / 255.0;
                input[out_c * dw * dh + y * dw + x] = (v - MEAN[out_c]) / STD[out_c];
            }
        }
    }
    (input, dw, dh)
}

/// How a sampled value in `0.0..=1.0` is normalised for a model input.
#[derive(Clone, Copy)]
enum Norm {
    /// ImageNet, BGR (detection, orientation).
    ImageNetBgr,
    /// `(v - 0.5) / 0.5`, BGR (recognition).
    Symmetric,
}

/// Fills a `[3, out_h, out_w]` input by bilinear sampling `frame`, its full
/// width stretched over `content_w` columns and its height over `out_h`
/// rows; columns past `content_w` stay zero (PaddleOCR's padding).
/// O(content_w x out_h).
fn frame_input(
    raster: &Raster,
    frame: &Frame,
    out_w: usize,
    out_h: usize,
    content_w: usize,
    norm: Norm,
) -> Vec<f32> {
    let mut input = vec![0f32; 3 * out_h * out_w];
    for y in 0..out_h {
        let v = ((y as f32 + 0.5) / out_h as f32 - 0.5) * frame.h;
        for x in 0..content_w {
            let u = ((x as f32 + 0.5) / content_w as f32 - 0.5) * frame.w;
            let (fx, fy) = frame.point(u, v);
            for (out_c, &rgb_c) in BGR.iter().enumerate() {
                let value = sample(raster, fx, fy, rgb_c) / 255.0;
                input[out_c * out_h * out_w + y * out_w + x] = match norm {
                    Norm::ImageNetBgr => (value - MEAN[out_c]) / STD[out_c],
                    Norm::Symmetric => (value - 0.5) / 0.5,
                };
            }
        }
    }
    input
}

/// The vertical extent of the ink in `frame`, in its upright frame: the
/// first and last rows (at the raster's resolution) holding a pixel darker
/// than mid-grey. An axis-aligned frame asks [`Raster::ink_rows`] directly.
/// O(frame pixels).
fn frame_ink_rows(raster: &Raster, frame: &Frame) -> Option<(f32, f32)> {
    if frame.axis_aligned() {
        return raster.ink_rows(
            frame.x0(),
            frame.y0(),
            frame.x0() + frame.w,
            frame.y0() + frame.h,
        );
    }
    let (cols, rows) = (
        frame.w.ceil().max(1.0) as usize,
        frame.h.ceil().max(1.0) as usize,
    );
    let (rw, rh) = (raster.width() as i64, raster.height() as i64);
    let rgb = raster.rgb();
    let inked = |row: usize| {
        let v = row as f32 + 0.5 - frame.h / 2.0;
        (0..cols).any(|col| {
            let (fx, fy) = frame.point(col as f32 + 0.5 - frame.w / 2.0, v);
            let (x, y) = (fx.floor() as i64, fy.floor() as i64);
            if x < 0 || y < 0 || x >= rw || y >= rh {
                return false;
            }
            let i = ((y * rw + x) * 3) as usize;
            u32::from(rgb[i]) + u32::from(rgb[i + 1]) + u32::from(rgb[i + 2]) < 3 * 128
        })
    };
    let top = (0..rows).find(|&r| inked(r))?;
    let bottom = (0..rows).rev().find(|&r| inked(r))?;
    Some((frame.y0() + top as f32, frame.y0() + (bottom + 1) as f32))
}

/// Whether the orientation classifier's `[0_degree, 180_degree]`
/// probabilities call a line upside down: only a confident call turns it.
pub(crate) fn upside_down(probabilities: [f32; 2]) -> bool {
    probabilities[1] > ORI_THRESH && probabilities[1] > probabilities[0]
}

impl PaddleOcr {
    /// The longest detector input side (the models' `trt_dynamic_shapes`
    /// maximum).
    pub const MAX_DET_SIDE: usize = 4000;

    /// Builds the provider from the host's detection model and one or more
    /// language packs (tried in order). O(model bytes + dictionary).
    ///
    /// # Errors
    ///
    /// [`OcrError::Model`] when a model does not load, no pack is given, or a
    /// dictionary's length does not fit its model's class count (checked on
    /// the first line read, when the class count is known).
    pub fn from_model_bytes(
        detection: Vec<u8>,
        packs: Vec<RecognitionPack>,
    ) -> Result<Self, OcrError> {
        if packs.is_empty() {
            return Err(OcrError::Model("no recognition pack supplied".to_owned()));
        }
        let mut models = vec![("detection", sha256_hex(&detection))];
        let detector = Model::load(detection).map_err(model_error)?;
        let mut recognizers = Vec::with_capacity(packs.len());
        let mut names = Vec::with_capacity(packs.len());
        for pack in packs {
            models.push(("recognition", sha256_hex(&pack.model)));
            models.push(("dictionary", sha256_hex(pack.dictionary.as_bytes())));
            let model = Model::load(pack.model).map_err(model_error)?;
            let labels = pack
                .dictionary
                .lines()
                .map(|line| line.trim_end_matches('\r'))
                // Whitespace entries (PP-OCRv5's first entry is U+3000)
                // separate words.
                .map(|entry| (!entry.trim().is_empty()).then(|| entry.to_owned()))
                .collect();
            recognizers.push(Recognizer { model, labels });
            names.push(pack.name);
        }
        Ok(Self {
            detector,
            recognizers,
            orientation: None,
            models,
            names,
        })
    }

    /// Adds the text-line orientation classifier
    /// (`PP-LCNet_x1_0_textline_ori`, converted to ONNX), so upside-down
    /// lines read the right way up. Its SHA-256 joins the manifest as
    /// `orientation`. O(model bytes).
    ///
    /// # Errors
    ///
    /// [`OcrError::Model`] when the model does not load.
    pub fn with_orientation(mut self, model: Vec<u8>) -> Result<Self, OcrError> {
        self.models.push(("orientation", sha256_hex(&model)));
        self.orientation = Some(Model::load(model).map_err(model_error)?);
        Ok(self)
    }

    /// Whether the orientation classifier is loaded.
    #[must_use]
    pub fn has_orientation(&self) -> bool {
        self.orientation.is_some()
    }

    /// Detects text lines. Returns frames in raster pixels, top to bottom,
    /// and the raster's skew.
    fn detect(&self, raster: &Raster) -> Result<(Vec<Frame>, f32), OcrError> {
        let (w, h) = (raster.width() as f32, raster.height() as f32);
        let (input, dw, dh) = detection_input(raster, Self::MAX_DET_SIDE);
        let (sx, sy) = (w / dw as f32, h / dh as f32);
        let input = NdTensor::from_data([1, 3, dh, dw], input);
        let output = self
            .detector
            .run_one(input.view().into(), None)
            .map_err(model_error)?;
        let output: Tensor<f32> = output.try_into().map_err(model_error)?;
        let shape = output.shape().to_vec();
        if shape.len() != 4 || shape[2] != dh || shape[3] != dw {
            return Err(OcrError::Model(format!(
                "detection output shape {shape:?}, expected [1, 1, {dh}, {dw}]"
            )));
        }
        let map: Vec<f32> = output.iter().copied().collect();
        let (found, skew) = components(&map[..dw * dh], dw, dh);
        let mut frames: Vec<Frame> = found
            .into_iter()
            .filter_map(|c| {
                if c.angle == 0.0 {
                    // Axis-aligned: clipped to the raster, as before.
                    let x0 = ((c.cx - c.w / 2.0) * sx).max(0.0);
                    let y0 = ((c.cy - c.h / 2.0) * sy).max(0.0);
                    let x1 = ((c.cx + c.w / 2.0) * sx).min(w);
                    let y1 = ((c.cy + c.h / 2.0) * sy).min(h);
                    return (x1 > x0 && y1 > y0).then_some(Frame {
                        cx: (x0 + x1) / 2.0,
                        cy: (y0 + y1) / 2.0,
                        w: x1 - x0,
                        h: y1 - y0,
                        angle: 0.0,
                        flip: false,
                    });
                }
                // The detector's input is the raster scaled by (1/sx, 1/sy),
                // nearly uniformly (sides rounded to 32): map the axes.
                let (sin, cos) = c.angle.to_radians().sin_cos();
                let angle = (sin * sy).atan2(cos * sx).to_degrees();
                Some(Frame {
                    cx: c.cx * sx,
                    cy: c.cy * sy,
                    w: c.w * ((cos * sx).powi(2) + (sin * sy).powi(2)).sqrt(),
                    h: c.h * ((sin * sx).powi(2) + (cos * sy).powi(2)).sqrt(),
                    angle,
                    flip: false,
                })
            })
            .collect();
        frames.sort_by(|a, b| a.y0().total_cmp(&b.y0()).then(a.x0().total_cmp(&b.x0())));
        Ok((frames, skew))
    }

    /// Asks the orientation classifier whether `frame` reads upside down.
    fn reads_upside_down(&self, raster: &Raster, frame: &Frame) -> Result<bool, OcrError> {
        let Some(model) = &self.orientation else {
            return Ok(false);
        };
        let input = frame_input(
            raster,
            frame,
            ORI_WIDTH,
            ORI_HEIGHT,
            ORI_WIDTH,
            Norm::ImageNetBgr,
        );
        let input = NdTensor::from_data([1, 3, ORI_HEIGHT, ORI_WIDTH], input);
        let output = model
            .run_one(input.view().into(), None)
            .map_err(model_error)?;
        let output: Tensor<f32> = output.try_into().map_err(model_error)?;
        let p: Vec<f32> = output.iter().copied().collect();
        if p.len() != 2 {
            return Err(OcrError::Model(format!(
                "orientation output has {} values, expected 2",
                p.len()
            )));
        }
        Ok(upside_down([p[0], p[1]]))
    }

    /// Reads one frame with one pack: characters with upright-frame x
    /// extents and probabilities, and the mean probability.
    fn read(
        &self,
        raster: &Raster,
        frame: &Frame,
        recognizer: &Recognizer,
    ) -> Result<(Vec<Decoded>, f32), OcrError> {
        let (bw, bh) = (frame.w, frame.h);
        let resized = ((REC_HEIGHT as f32 * bw / bh).ceil() as usize).clamp(1, REC_MAX_WIDTH);
        let width = resized.max(REC_MIN_WIDTH);
        let input = frame_input(raster, frame, width, REC_HEIGHT, resized, Norm::Symmetric);
        let input = NdTensor::from_data([1, 3, REC_HEIGHT, width], input);
        let output = recognizer
            .model
            .run_one(input.view().into(), None)
            .map_err(model_error)?;
        let output: Tensor<f32> = output.try_into().map_err(model_error)?;
        let output: NdTensor<f32, 3> = output.try_into().map_err(model_error)?;
        let [_, frames, classes] = output.shape();
        let dictionary = recognizer.labels.len();
        let has_space = match classes.checked_sub(dictionary) {
            Some(2) => true,
            Some(1) => false,
            _ => {
                return Err(OcrError::Model(format!(
                    "recognition output has {classes} classes for a {dictionary}-entry dictionary"
                )));
            }
        };
        if frames == 0 {
            return Ok((Vec::new(), 0.0));
        }
        let mut steps: Vec<(usize, usize, f32)> = Vec::new();
        let mut previous = 0usize;
        for step in 0..frames {
            let mut best = (0usize, f32::NEG_INFINITY);
            for class in 0..classes {
                let p = output[[0, step, class]];
                if p > best.1 {
                    best = (class, p);
                }
            }
            let (label, p) = best;
            if label != 0 {
                if label == previous {
                    if let Some(last) = steps.last_mut() {
                        last.2 = last.2.max(p);
                    }
                } else {
                    steps.push((label, step, p));
                }
            }
            previous = label;
        }
        let frame_px = width as f32 / frames as f32;
        let to_frame = bw / resized as f32;
        let x0 = frame.x0();
        let mut chars = Vec::with_capacity(steps.len());
        let mut sum = 0.0f32;
        for (i, &(label, step, p)) in steps.iter().enumerate() {
            let start = step as f32 * frame_px;
            if start >= resized as f32 {
                continue;
            }
            let end = steps
                .get(i + 1)
                .map_or(resized as f32, |next| next.1 as f32 * frame_px)
                .min(resized as f32);
            let c = if has_space && label == classes - 1 {
                None
            } else {
                recognizer.labels.get(label - 1).cloned().flatten()
            };
            sum += p;
            chars.push((
                c,
                x0 + start * to_frame,
                x0 + end * to_frame,
                p.clamp(f32::MIN_POSITIVE, 1.0),
            ));
        }
        let mean = if chars.is_empty() {
            0.0
        } else {
            sum / chars.len() as f32
        };
        Ok((chars, mean))
    }
}

/// Splits characters at separators into words.
fn words(chars: &[Decoded], y0: f32, y1: f32) -> Vec<OcrWord> {
    chars
        .split(|c| c.0.is_none())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mean_log = w.iter().map(|c| c.3.ln()).sum::<f32>() / w.len() as f32;
            OcrWord {
                text: w.iter().filter_map(|c| c.0.as_deref()).collect(),
                bounds: OcrBox {
                    x0: w[0].1,
                    y0,
                    x1: w[w.len() - 1].2,
                    y1,
                },
                confidence: mean_log.exp().clamp(0.0, 1.0),
            }
        })
        .collect()
}

impl OcrProvider for PaddleOcr {
    fn manifest(&self) -> ProviderManifest {
        ProviderManifest {
            provider: "pp-ocrv5",
            version: env!("CARGO_PKG_VERSION"),
            models: self.models.clone(),
            decoding: "db-ctc-greedy",
            determinism: Determinism::SameTarget,
        }
    }

    fn recognize(
        &self,
        raster: &Raster,
        limits: &OcrLimits,
        interrupt: &dyn Interrupt,
    ) -> Result<OcrPage, OcrError> {
        check_pixels(raster, limits)?;
        let mut page = OcrPage::default();
        match interrupt.poll() {
            Some(Stop::Cancelled) => return Err(OcrError::Cancelled),
            Some(Stop::OutOfTime) => {
                page.out_of_time = true;
                return Ok(page);
            }
            None => {}
        }
        let (frames, skew) = self.detect(raster)?;
        page.skew_degrees = skew;
        let total = frames.len();
        for (index, detected) in frames.iter().enumerate() {
            if index as u64 >= u64::from(limits.max_lines) {
                page.lines_skipped = u32::try_from(total - index).unwrap_or(u32::MAX);
                break;
            }
            match interrupt.poll() {
                Some(Stop::Cancelled) => return Err(OcrError::Cancelled),
                Some(Stop::OutOfTime) => {
                    page.out_of_time = true;
                    page.lines_skipped = u32::try_from(total - index).unwrap_or(u32::MAX);
                    break;
                }
                None => {}
            }
            let mut frame = *detected;
            frame.flip = self.reads_upside_down(raster, &frame)?;
            let mut best: Option<(Vec<Decoded>, f32)> = None;
            for recognizer in &self.recognizers {
                let reading = self.read(raster, &frame, recognizer)?;
                if best.as_ref().is_none_or(|(_, mean)| reading.1 > *mean) {
                    best = Some(reading);
                }
            }
            let Some((chars, _)) = best else {
                continue;
            };
            if chars.len() as u64 > u64::from(limits.max_chars_per_line) {
                page.lines_skipped = page.lines_skipped.saturating_add(1);
                continue;
            }
            let (y0, y1) =
                frame_ink_rows(raster, &frame).unwrap_or((frame.y0(), frame.y0() + frame.h));
            let words = words(&chars, y0, y1);
            let Some(first) = words.first() else {
                continue;
            };
            let bounds = words
                .iter()
                .fold(first.bounds, |acc, w| acc.union(&w.bounds));
            page.lines.push(OcrLine {
                words,
                bounds,
                rotation_degrees: frame.rotation(),
                pivot: [frame.cx, frame.cy],
            });
        }
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_component_under_the_box_threshold_is_dropped_and_one_over_it_unclipped() {
        let (w, h) = (40, 20);
        let mut map = vec![0.0f32; w * h];
        // A confident 20 x 6 bar and a faint 10 x 6 one.
        for y in 7..13 {
            for x in 2..22 {
                map[y * w + x] = 0.9;
            }
            for x in 26..36 {
                map[y * w + x] = 0.4;
            }
        }
        let (found, skew) = components(&map, w, h);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(skew, 0.0);
        let c = found[0];
        assert_eq!(c.angle, 0.0);
        // d = 20 * 6 * 1.5 / (2 * 26) = 3.46.
        assert!((c.x0() - (2.0 - 3.4615)).abs() < 1e-3, "{c:?}");
        assert!((c.y0() + c.h - (13.0 + 3.4615)).abs() < 1e-3, "{c:?}");
    }

    /// Paints a bar `len` x `thick` centred on `(cx, cy)`, turned `degrees`
    /// clockwise, into a probability map.
    fn bar(map: &mut [f32], w: usize, (cx, cy): (f32, f32), len: f32, thick: f32, degrees: f32) {
        let (sin, cos) = degrees.to_radians().sin_cos();
        for (i, p) in map.iter_mut().enumerate() {
            let (x, y) = ((i % w) as f32 + 0.5 - cx, (i / w) as f32 + 0.5 - cy);
            let (u, v) = (x * cos + y * sin, -x * sin + y * cos);
            if u.abs() <= len / 2.0 && v.abs() <= thick / 2.0 {
                *p = 0.9;
            }
        }
    }

    #[test]
    fn a_skewed_line_gets_a_box_along_its_own_angle_and_short_ones_take_the_skew() {
        let (w, h) = (400, 200);
        let mut map = vec![0.0f32; w * h];
        // Two long lines turned 3 degrees clockwise and a one-word stub whose
        // own principal axis says nothing.
        bar(&mut map, w, (200.0, 60.0), 300.0, 10.0, 3.0);
        bar(&mut map, w, (200.0, 110.0), 300.0, 10.0, 3.0);
        bar(&mut map, w, (60.0, 170.0), 16.0, 10.0, 0.0);
        let (found, skew) = components(&map, w, h);
        assert_eq!(found.len(), 3, "{found:?}");
        assert!((skew - 3.0).abs() < 0.3, "skew {skew}");
        for f in &found {
            assert!(
                (f.angle - skew).abs() < 1e-6,
                "every line shares the skew: {f:?}"
            );
        }
        // Along its angle the long box is the bar's thickness plus the
        // unclip margin, not the bar's axis-aligned extent (about 26 px).
        let long = found.iter().find(|f| f.w > 100.0).unwrap();
        let d = 300.0 * 10.0 * UNCLIP_RATIO / (2.0 * 310.0);
        assert!((long.h - (10.0 + 2.0 * d)).abs() < 2.5, "{long:?}");
    }

    #[test]
    fn a_line_far_from_the_skew_keeps_its_own_angle() {
        let (w, h) = (400, 300);
        let mut map = vec![0.0f32; w * h];
        bar(&mut map, w, (200.0, 50.0), 300.0, 10.0, 0.0);
        bar(&mut map, w, (200.0, 100.0), 300.0, 10.0, 0.0);
        bar(&mut map, w, (200.0, 200.0), 300.0, 10.0, 10.0);
        let (found, skew) = components(&map, w, h);
        assert_eq!(skew, 0.0);
        let angles: Vec<f32> = found.iter().map(|f| f.angle.round()).collect();
        assert_eq!(angles, vec![0.0, 0.0, 10.0], "{found:?}");
    }

    #[test]
    fn the_detector_input_is_bgr_with_imagenet_constants_in_that_order() {
        // A colour fixture: channel order and input scale are both visible.
        // (51, 102, 204) is (0.2, 0.4, 0.8) of full scale.
        let raster = Raster::from_rgb(1, 1, vec![51, 102, 204]).unwrap();
        let (input, dw, dh) = detection_input(&raster, PaddleOcr::MAX_DET_SIDE);
        assert_eq!((dw, dh), (32, 32));
        let plane = dw * dh;
        // Channel 0 is blue (0.8) normalised with the first mean and std.
        let expected = [
            (0.8 - 0.485) / 0.229,
            (0.4 - 0.456) / 0.224,
            (0.2 - 0.406) / 0.225,
        ];
        for (c, want) in expected.iter().enumerate() {
            for at in [0, plane - 1] {
                let got = input[c * plane + at];
                assert!(
                    (got - want).abs() < 1e-5,
                    "channel {c} pixel {at}: {got} != {want}"
                );
            }
        }
    }

    #[test]
    fn a_turned_frame_samples_upside_down_and_maps_its_points_back() {
        // Left half black, right half white.
        let (w, h) = (8u32, 4u32);
        let mut rgb = vec![255u8; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w / 2 {
                let i = ((y * w + x) * 3) as usize;
                rgb[i..i + 3].copy_from_slice(&[0, 0, 0]);
            }
        }
        let raster = Raster::from_rgb(w, h, rgb).unwrap();
        let upright = Frame {
            cx: 4.0,
            cy: 2.0,
            w: 8.0,
            h: 4.0,
            angle: 0.0,
            flip: false,
        };
        let turned = Frame {
            flip: true,
            ..upright
        };
        let first_column = |frame: &Frame| frame_input(&raster, frame, 8, 4, 8, Norm::Symmetric)[0];
        assert!(first_column(&upright) < 0.0, "upright: black on the left");
        assert!(first_column(&turned) > 0.0, "turned: white on the left");
        assert_eq!(turned.rotation(), 180.0);
        assert_eq!(turned.point(-4.0, 0.0), (8.0, 2.0));
        let line = OcrLine {
            words: Vec::new(),
            bounds: OcrBox::default(),
            rotation_degrees: turned.rotation(),
            pivot: [turned.cx, turned.cy],
        };
        let (x, y) = line.to_raster(0.0, 2.0);
        assert!((x - 8.0).abs() < 1e-4 && (y - 2.0).abs() < 1e-4, "{x} {y}");
    }

    #[test]
    fn only_a_confident_call_turns_a_line() {
        assert!(upside_down([0.02, 0.98]));
        assert!(!upside_down([0.15, 0.85]));
        assert!(!upside_down([0.99, 0.01]));
    }
}
