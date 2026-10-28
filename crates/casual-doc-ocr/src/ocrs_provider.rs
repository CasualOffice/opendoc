//! The `ocrs` recognition provider (feature `ocrs`).
//!
//! Detection and line finding are `ocrs`'s own. Recognition runs the same
//! model and the same line preprocessing (`OcrEngine::prepare_recognition_input`),
//! but this module runs the model and decodes it itself, one line at a time:
//!
//! - `ocrs` does not expose a confidence, and the importer must report
//!   low-confidence text (`pdf.import.ocr_low_confidence`). Greedy CTC over the
//!   model's log-probabilities gives each character the probability of its
//!   best frame; a word's confidence is the geometric mean of its characters'.
//! - one line per model run is the unit the [`Interrupt`] is polled at, so a
//!   Cancel lands within one line rather than one batch of twenty;
//! - lines run in order on the calling thread (no `rayon` batch re-sorting),
//!   so the output order is the detector's reading order by construction.
//!
//! The recognition model is therefore loaded twice (once inside the engine for
//! preprocessing, once here): about 10 MB of extra resident weights for the
//! published model, traded for not forking `ocrs`.
//!
//! Decoding is greedy, never sampled; ties go to the lower label (PDFR-I8).

use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;
use rten_imageproc::{RotatedRect, bounding_rect};
use rten_tensor::prelude::*;
use rten_tensor::{NdTensor, Tensor};

use crate::digest::sha256_hex;
use crate::provider::check_pixels;
use crate::{
    Determinism, Interrupt, OcrBox, OcrError, OcrLimits, OcrLine, OcrPage, OcrProvider, OcrWord,
    ProviderManifest, Raster, Stop,
};

/// The alphabet the published `ocrs` recognition model was trained with,
/// copied from `ocrs` 0.13.1 `src/lib.rs` `DEFAULT_ALPHABET` (MIT OR
/// Apache-2.0), where it is private. Label 0 is the CTC blank; label `i + 1`
/// is character `i`. (The `E` before `ABCDE` stands for the euro sign in the
/// training data; `ocrs` emits `E` and so does this.)
const ALPHABET: &str = " 0123456789!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~EABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// One decoded character: itself, its raster x extent, its log-probability.
type DecodedChar = (char, f32, f32, f32);

/// The `ocrs` engine over host-supplied `rten` models.
pub struct OcrsProvider {
    engine: OcrEngine,
    recognizer: Model,
    alphabet: Vec<char>,
    models: Vec<(&'static str, String)>,
}

impl std::fmt::Debug for OcrsProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OcrsProvider")
            .field("models", &self.models)
            .finish_non_exhaustive()
    }
}

fn model_error(error: impl std::fmt::Display) -> OcrError {
    OcrError::Model(error.to_string())
}

impl OcrsProvider {
    /// Builds the provider from model bytes the host obtained (an `.rten` or
    /// `.onnx` file each). This crate never fetches them. O(model bytes).
    ///
    /// # Errors
    ///
    /// [`OcrError::Model`] when either model does not load or has the wrong
    /// shape for its role.
    pub fn from_model_bytes(detection: Vec<u8>, recognition: Vec<u8>) -> Result<Self, OcrError> {
        let models = vec![
            ("detection", sha256_hex(&detection)),
            ("recognition", sha256_hex(&recognition)),
        ];
        let detection_model = Model::load(detection).map_err(model_error)?;
        let engine_recognizer = Model::load(recognition.clone()).map_err(model_error)?;
        let recognizer = Model::load(recognition).map_err(model_error)?;
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(detection_model),
            recognition_model: Some(engine_recognizer),
            ..OcrEngineParams::default()
        })
        .map_err(model_error)?;
        Ok(Self {
            engine,
            recognizer,
            alphabet: ALPHABET.chars().collect(),
            models,
        })
    }

    /// Reads both models from files (native hosts and tests). O(model bytes).
    ///
    /// # Errors
    ///
    /// [`OcrError::Model`] when a file cannot be read or does not load.
    #[cfg(not(target_family = "wasm"))]
    pub fn from_model_files(
        detection: &std::path::Path,
        recognition: &std::path::Path,
    ) -> Result<Self, OcrError> {
        let read = |path: &std::path::Path| {
            std::fs::read(path)
                .map_err(|error| OcrError::Model(format!("{}: {error}", path.display())))
        };
        Self::from_model_bytes(read(detection)?, read(recognition)?)
    }

    /// Recognises one detected line: its characters with their raster x
    /// extents and log-probabilities. `None` when nothing was read.
    fn recognize_line(
        &self,
        input: &ocrs::OcrInput,
        line: &[RotatedRect],
    ) -> Result<Option<Vec<DecodedChar>>, OcrError> {
        let Some(rect) = bounding_rect(line.iter()) else {
            return Ok(None);
        };
        let rect = rect.integral_bounding_rect();
        let image = self
            .engine
            .prepare_recognition_input(input, line)
            .map_err(model_error)?;
        let [height, width] = image.shape();
        if width == 0 || height == 0 {
            return Ok(None);
        }
        let batch = image.into_shape([1, 1, height, width]);
        let output = self
            .recognizer
            .run_one(batch.view().into(), None)
            .map_err(model_error)?;
        let output: Tensor<f32> = output.try_into().map_err(model_error)?;
        let output: NdTensor<f32, 3> = output.try_into().map_err(model_error)?;
        // [sequence, batch, class] log-probabilities.
        let [sequence, _, classes] = output.shape();
        if classes != self.alphabet.len() + 1 {
            return Err(OcrError::Model(format!(
                "recognition output has {classes} classes, the alphabet needs {}",
                self.alphabet.len() + 1
            )));
        }
        if sequence == 0 {
            return Ok(None);
        }
        // Greedy CTC: the best label per frame, repeats collapsed, blanks
        // dropped. A character keeps its first frame and its best
        // log-probability over the frames it spans.
        let mut steps: Vec<(usize, usize, f32)> = Vec::new();
        let mut previous = 0usize;
        for frame in 0..sequence {
            let mut best = (0usize, f32::NEG_INFINITY);
            for class in 0..classes {
                let value = output[[frame, 0, class]];
                if value > best.1 {
                    best = (class, value);
                }
            }
            let (label, log_p) = best;
            if label != 0 {
                if label == previous {
                    if let Some(last) = steps.last_mut() {
                        last.2 = last.2.max(log_p);
                    }
                } else {
                    steps.push((label, frame, log_p));
                }
            }
            previous = label;
        }
        let downsample = ((width as f32 / sequence as f32).round() as usize).max(1);
        let x_scale = rect.width() as f32 / width as f32;
        let left = rect.left() as f32;
        let right = rect.right() as f32;
        let mut chars = Vec::with_capacity(steps.len());
        for (i, &(label, frame, log_p)) in steps.iter().enumerate() {
            let start = frame * downsample;
            let end = steps
                .get(i + 1)
                .map_or(width, |next| (next.1 * downsample).min(width));
            let x0 = left + start as f32 * x_scale;
            let x1 = left + end as f32 * x_scale;
            if x0 >= right {
                continue;
            }
            let c = self.alphabet.get(label - 1).copied().unwrap_or('?');
            chars.push((c, x0, x1.min(right), log_p));
        }
        Ok((!chars.is_empty()).then_some(chars))
    }
}

/// Splits a recognised line at spaces into words with boxes and confidences.
fn words(chars: &[DecodedChar], y0: f32, y1: f32) -> Vec<OcrWord> {
    chars
        .split(|c| c.0 == ' ')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mean_log_p = w.iter().map(|c| c.3).sum::<f32>() / w.len() as f32;
            OcrWord {
                text: w.iter().map(|c| c.0).collect(),
                bounds: OcrBox {
                    x0: w[0].1,
                    y0,
                    x1: w[w.len() - 1].2,
                    y1,
                },
                confidence: mean_log_p.exp().clamp(0.0, 1.0),
            }
        })
        .collect()
}

impl OcrProvider for OcrsProvider {
    fn manifest(&self) -> ProviderManifest {
        ProviderManifest {
            provider: "ocrs",
            version: "0.13.1",
            models: self.models.clone(),
            decoding: "ctc-greedy",
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
        let source = ImageSource::from_bytes(raster.rgb(), (raster.width(), raster.height()))
            .map_err(model_error)?;
        let input = self.engine.prepare_input(source).map_err(model_error)?;
        let detected = self.engine.detect_words(&input).map_err(model_error)?;
        let lines = self.engine.find_text_lines(&input, &detected);
        let total = lines.len();
        for (index, line) in lines.iter().enumerate() {
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
            let Some(chars) = self.recognize_line(&input, line)? else {
                continue;
            };
            if chars.len() as u64 > u64::from(limits.max_chars_per_line) {
                page.lines_skipped = page.lines_skipped.saturating_add(1);
                continue;
            }
            let Some(rect) = bounding_rect(line.iter()) else {
                continue;
            };
            let (y0, y1) = (rect.top(), rect.bottom());
            let words = words(&chars, y0, y1);
            let Some(first) = words.first() else {
                continue;
            };
            let bounds = words.iter().fold(first.bounds, |b, w| b.union(&w.bounds));
            page.lines.push(OcrLine { words, bounds });
        }
        Ok(page)
    }
}
