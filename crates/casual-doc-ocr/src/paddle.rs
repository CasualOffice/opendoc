//! PaddleOCR PP-OCRv5 on `rten` (feature `paddle`, the default provider;
//! owner decision 2026-10-29, `docs/182` §20.2).
//!
//! The weights are PaddlePaddle's own Apache-2.0 releases
//! (`huggingface.co/PaddlePaddle/PP-OCRv5_mobile_det`,
//! `.../PP-OCRv5_mobile_rec` and the per-script `*_PP-OCRv5_mobile_rec`
//! language packs), converted by the host to ONNX with PaddlePaddle's
//! `paddle2onnx`. This module never fetches or converts anything: the host
//! passes the detection model's bytes and one or more [`RecognitionPack`]s
//! (recognition model bytes plus that model's character dictionary).
//!
//! The pipeline mirrors PaddleOCR's own pre- and post-processing, with the
//! parameters of the models' `inference.yml` fixed in code:
//!
//! - **Detection (DB).** The raster, BGR, normalised with ImageNet mean and
//!   std, its sides rounded to multiples of 32 (long side capped at
//!   [`PaddleOcr::MAX_DET_SIDE`]); the probability map is thresholded at
//!   0.3; each 8-connected component is a text candidate, kept when its mean
//!   probability is at least 0.6 (`box_thresh`) and its short side at least
//!   3 px; its box is expanded by `area x 1.5 / perimeter` (`unclip_ratio`,
//!   the polygon offset PaddleOCR applies with pyclipper, here on the
//!   component's axis-aligned box).
//! - **Recognition (CTC).** Each box cropped from the raster, resized to
//!   height 48 keeping its aspect, normalised to `[-1, 1]`, padded to at
//!   least 320 wide; the model's per-frame class probabilities are decoded
//!   greedily (blank 0, then the dictionary, then a space when the model has
//!   one more class than the dictionary), repeats collapsed. A character's
//!   confidence is its best frame's probability; a word's is the geometric
//!   mean of its characters'.
//!
//! **Several language packs.** Each line is read by every pack; the reading
//! with the highest mean character probability is kept (ties go to the pack
//! supplied first). The cost is one recognition run per pack per line.
//!
//! **Not built.** The text-line orientation classifier (`PP-LCNet_x1_0_textline_ori`):
//! an upside-down line is read upside down. Boxes are axis-aligned, so a scan
//! skewed by more than a few degrees reads worse; deskew is not built.
//!
//! Verified against onnxruntime 1.19.2 on the converted graphs (`docs/182`
//! §20.2): `rten` loads and runs all three with the largest absolute
//! difference 1.4e-4 on the detection map and 7.2e-6 on the recognition
//! probabilities.

use rten::Model;
use rten_tensor::prelude::*;
use rten_tensor::{NdTensor, Tensor};

use crate::digest::sha256_hex;
use crate::provider::check_pixels;
use crate::{
    Determinism, Interrupt, OcrBox, OcrError, OcrLimits, OcrLine, OcrPage, OcrProvider, OcrWord,
    ProviderManifest, Raster, Stop,
};

const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];
const DET_THRESH: f32 = 0.3;
const BOX_THRESH: f32 = 0.6;
const UNCLIP_RATIO: f32 = 1.5;
const MIN_SIZE: f32 = 3.0;
const REC_HEIGHT: usize = 48;
const REC_MIN_WIDTH: usize = 320;
const REC_MAX_WIDTH: usize = 3200;
const MAX_CANDIDATES: usize = 1000;

/// One decoded label: its text (`None` for a separator), its raster x
/// extent, its probability.
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
    models: Vec<(&'static str, String)>,
    names: Vec<String>,
}

impl std::fmt::Debug for PaddleOcr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaddleOcr")
            .field("packs", &self.names)
            .field("models", &self.models)
            .finish_non_exhaustive()
    }
}

fn model_error(error: impl std::fmt::Display) -> OcrError {
    OcrError::Model(error.to_string())
}

/// A text candidate in detector-input pixels.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

/// 8-connected components of `map > DET_THRESH`, each kept when its mean
/// probability reaches `BOX_THRESH`, as unclipped axis-aligned boxes.
/// O(pixels) with an explicit stack.
fn components(map: &[f32], width: usize, height: usize) -> Vec<Candidate> {
    let mut seen = vec![false; map.len()];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..map.len() {
        if seen[start] || map[start] <= DET_THRESH {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        let (mut sum, mut count) = (0.0f64, 0u64);
        while let Some(p) = stack.pop() {
            let (x, y) = (p % width, p / width);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            sum += f64::from(map[p]);
            count += 1;
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
        let (w, h) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
        if w.min(h) < MIN_SIZE || (sum / count as f64) < f64::from(BOX_THRESH) {
            continue;
        }
        let d = w * h * UNCLIP_RATIO / (2.0 * (w + h));
        out.push(Candidate {
            x0: x0 as f32 - d,
            y0: y0 as f32 - d,
            x1: x1 as f32 + 1.0 + d,
            y1: y1 as f32 + 1.0 + d,
        });
        if out.len() >= MAX_CANDIDATES {
            break;
        }
    }
    out.retain(|c| (c.x1 - c.x0).min(c.y1 - c.y0) >= MIN_SIZE + 2.0);
    out
}

/// Bilinear sample of channel `c` at `(fx, fy)` (pixel centres at `+0.5`).
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
            models,
            names,
        })
    }

    /// Detects text lines. Returns boxes in raster pixels, top to bottom.
    fn detect(&self, raster: &Raster) -> Result<Vec<Candidate>, OcrError> {
        let (w, h) = (raster.width() as usize, raster.height() as usize);
        let ratio = (Self::MAX_DET_SIDE as f32 / w.max(h) as f32).min(1.0);
        let round32 = |v: usize| ((v as f32 * ratio / 32.0).round() as usize).max(1) * 32;
        let (dw, dh) = (round32(w), round32(h));
        let (sx, sy) = (w as f32 / dw as f32, h as f32 / dh as f32);
        let mut input = vec![0f32; 3 * dw * dh];
        for y in 0..dh {
            for x in 0..dw {
                let (fx, fy) = ((x as f32 + 0.5) * sx, (y as f32 + 0.5) * sy);
                // BGR, as PaddleOCR decodes; the mean and std are applied in
                // that channel order, as its NormalizeImage does.
                for (out_c, rgb_c) in [(0usize, 2usize), (1, 1), (2, 0)] {
                    let v = sample(raster, fx, fy, rgb_c) / 255.0;
                    input[out_c * dw * dh + y * dw + x] = (v - MEAN[out_c]) / STD[out_c];
                }
            }
        }
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
        let mut boxes: Vec<Candidate> = components(&map[..dw * dh], dw, dh)
            .into_iter()
            .map(|c| Candidate {
                x0: (c.x0 * sx).max(0.0),
                y0: (c.y0 * sy).max(0.0),
                x1: (c.x1 * sx).min(w as f32),
                y1: (c.y1 * sy).min(h as f32),
            })
            .filter(|c| c.x1 > c.x0 && c.y1 > c.y0)
            .collect();
        boxes.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
        Ok(boxes)
    }

    /// Reads one box with one pack: characters with raster x extents and
    /// probabilities, and the mean probability.
    fn read(
        &self,
        raster: &Raster,
        b: &Candidate,
        recognizer: &Recognizer,
    ) -> Result<(Vec<Decoded>, f32), OcrError> {
        let (bw, bh) = (b.x1 - b.x0, b.y1 - b.y0);
        let resized = ((REC_HEIGHT as f32 * bw / bh).ceil() as usize).clamp(1, REC_MAX_WIDTH);
        let width = resized.max(REC_MIN_WIDTH);
        let mut input = vec![0f32; 3 * REC_HEIGHT * width];
        let (sx, sy) = (bw / resized as f32, bh / REC_HEIGHT as f32);
        for y in 0..REC_HEIGHT {
            for x in 0..resized {
                let (fx, fy) = (b.x0 + (x as f32 + 0.5) * sx, b.y0 + (y as f32 + 0.5) * sy);
                for (out_c, rgb_c) in [(0usize, 2usize), (1, 1), (2, 0)] {
                    let v = sample(raster, fx, fy, rgb_c) / 255.0;
                    input[out_c * REC_HEIGHT * width + y * width + x] = (v - 0.5) / 0.5;
                }
            }
        }
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
        for frame in 0..frames {
            let mut best = (0usize, f32::NEG_INFINITY);
            for class in 0..classes {
                let p = output[[0, frame, class]];
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
                    steps.push((label, frame, p));
                }
            }
            previous = label;
        }
        let frame_px = width as f32 / frames as f32;
        let to_raster = bw / resized as f32;
        let mut chars = Vec::with_capacity(steps.len());
        let mut sum = 0.0f32;
        for (i, &(label, frame, p)) in steps.iter().enumerate() {
            let start = frame as f32 * frame_px;
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
                b.x0 + start * to_raster,
                b.x0 + end * to_raster,
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
        let boxes = self.detect(raster)?;
        let total = boxes.len();
        for (index, b) in boxes.iter().enumerate() {
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
            let mut best: Option<(Vec<Decoded>, f32)> = None;
            for recognizer in &self.recognizers {
                let reading = self.read(raster, b, recognizer)?;
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
            let (y0, y1) = raster
                .ink_rows(b.x0, b.y0, b.x1, b.y1)
                .unwrap_or((b.y0, b.y1));
            let words = words(&chars, y0, y1);
            let Some(first) = words.first() else {
                continue;
            };
            let bounds = words
                .iter()
                .fold(first.bounds, |acc, w| acc.union(&w.bounds));
            page.lines.push(OcrLine { words, bounds });
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
        let found = components(&map, w, h);
        assert_eq!(found.len(), 1, "{found:?}");
        let c = found[0];
        // d = 20 * 6 * 1.5 / (2 * 26) = 3.46.
        assert!((c.x0 - (2.0 - 3.4615)).abs() < 1e-3, "{c:?}");
        assert!((c.y1 - (13.0 + 3.4615)).abs() < 1e-3, "{c:?}");
    }
}
