//! Docling's `docling-layout-heron` layout model on `rten` (feature
//! `layout-heron`, `docs/182` P6).
//!
//! The model is the Apache-2.0 ONNX export published at
//! `huggingface.co/docling-project/docling-layout-heron-onnx` (an RT-DETRv2
//! detector, 17 labels). Its graph takes `images: u8 [batch, 3, 640, 640]`
//! (the rescale and normalisation are inside the graph) and
//! `orig_target_sizes: [batch, 2]`, and returns `labels`, `boxes` (corner
//! form, in target-size pixels) and `scores` for 300 queries.
//!
//! The page raster is resized to 640 x 640 with a fixed bilinear filter (the
//! processor's `resample: 2`), the target size is given as 640 x 640 so the
//! boxes come back in the resized frame, and this module scales them to the
//! raster and then to page space itself — the box mapping does not depend on
//! the export's width/height order convention. Queries under
//! [`HeronLayout::THRESHOLD`] (Docling's own default) are dropped; the rest
//! are sorted by position with a total tie-break (PDFR-I8).
//!
//! Work per page is bounded by construction: one model run on a fixed-size
//! input, whatever the raster's size (the resize samples four source pixels
//! per output pixel). Measured on an Apple M-series CPU in a release build:
//! about 0.45 s per page.

use rten::Model;
use rten_tensor::prelude::*;
use rten_tensor::{NdTensor, Tensor};

use crate::digest::sha256_hex;
use crate::layout::{LayoutInput, LayoutProvider, LayoutRegion, RegionKind, sort_regions};
use crate::{Determinism, Interrupt, OcrBox, OcrError, ProviderManifest, Raster, Stop};

const SIDE: usize = 640;

/// The heron layout model over host-supplied ONNX bytes.
pub struct HeronLayout {
    model: Model,
    digest: String,
}

impl std::fmt::Debug for HeronLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeronLayout")
            .field("digest", &self.digest)
            .finish_non_exhaustive()
    }
}

fn model_error(error: impl std::fmt::Display) -> OcrError {
    OcrError::Model(error.to_string())
}

/// heron's `id2label` (its `config.json`), mapped to the kinds the model
/// knows how to hold.
fn kind_of(label: i32) -> RegionKind {
    match label {
        0 => RegionKind::Caption,
        1 => RegionKind::Footnote,
        2 => RegionKind::Formula,
        3 => RegionKind::ListItem,
        4 => RegionKind::PageFooter,
        5 => RegionKind::PageHeader,
        6 => RegionKind::Figure,
        7 => RegionKind::SectionHeader,
        8 => RegionKind::Table,
        9 => RegionKind::Text,
        10 => RegionKind::Title,
        12 => RegionKind::Code,
        13..=16 => RegionKind::Form,
        _ => RegionKind::Other,
    }
}

/// Bilinear resize of `raster` to `SIDE` x `SIDE`, CHW. Pixel centres are
/// aligned (`align_corners = false`), the convention PIL and torchvision use.
/// Integer-free float arithmetic in a fixed order: deterministic per target.
fn resize_chw(raster: &Raster) -> Vec<u8> {
    let (w, h) = (raster.width() as usize, raster.height() as usize);
    let rgb = raster.rgb();
    let mut out = vec![0u8; 3 * SIDE * SIDE];
    let sx = w as f32 / SIDE as f32;
    let sy = h as f32 / SIDE as f32;
    for y in 0..SIDE {
        let fy = ((y as f32 + 0.5) * sy - 0.5).clamp(0.0, (h - 1) as f32);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(h - 1);
        let ty = fy - y0 as f32;
        for x in 0..SIDE {
            let fx = ((x as f32 + 0.5) * sx - 0.5).clamp(0.0, (w - 1) as f32);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(w - 1);
            let tx = fx - x0 as f32;
            for c in 0..3 {
                let p = |xx: usize, yy: usize| f32::from(rgb[(yy * w + xx) * 3 + c]);
                let top = p(x0, y0) * (1.0 - tx) + p(x1, y0) * tx;
                let bottom = p(x0, y1) * (1.0 - tx) + p(x1, y1) * tx;
                let value = top * (1.0 - ty) + bottom * ty;
                out[c * SIDE * SIDE + y * SIDE + x] = value.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

impl HeronLayout {
    /// Docling's default score threshold for heron.
    pub const THRESHOLD: f32 = 0.3;

    /// Builds the provider from the host's `model.onnx` bytes. O(model bytes).
    ///
    /// # Errors
    ///
    /// [`OcrError::Model`] when the bytes do not load or the graph does not
    /// have heron's two inputs and three outputs.
    pub fn from_model_bytes(bytes: Vec<u8>) -> Result<Self, OcrError> {
        let digest = sha256_hex(&bytes);
        let model = Model::load(bytes).map_err(model_error)?;
        if model.input_ids().len() != 2 || model.output_ids().len() != 3 {
            return Err(OcrError::Model(
                "not a heron layout graph: expected 2 inputs and 3 outputs".to_owned(),
            ));
        }
        Ok(Self { model, digest })
    }

    fn node(&self, name: &str) -> Result<rten::NodeId, OcrError> {
        self.model
            .find_node(name)
            .ok_or_else(|| OcrError::Model(format!("heron graph has no node {name}")))
    }
}

impl LayoutProvider for HeronLayout {
    fn manifest(&self) -> ProviderManifest {
        ProviderManifest {
            provider: "docling-layout-heron",
            version: "rten-0.26.0",
            models: vec![("layout", self.digest.clone())],
            decoding: "threshold-0.3",
            determinism: Determinism::SameTarget,
        }
    }

    fn needs_raster(&self) -> bool {
        true
    }

    fn regions(
        &self,
        input: &LayoutInput<'_>,
        interrupt: &dyn Interrupt,
    ) -> Result<Vec<LayoutRegion>, OcrError> {
        match interrupt.poll() {
            Some(Stop::Cancelled) => return Err(OcrError::Cancelled),
            Some(Stop::OutOfTime) => return Ok(Vec::new()),
            None => {}
        }
        let Some((raster, scale)) = input.raster else {
            return Err(OcrError::Model("heron needs the page raster".to_owned()));
        };
        if scale.is_nan() || scale <= 0.0 {
            return Err(OcrError::InvalidImage);
        }
        let images = NdTensor::<u8, 4>::from_data([1, 3, SIDE, SIDE], resize_chw(raster));
        let sizes = NdTensor::<i32, 2>::from_data([1, 2], vec![SIDE as i32, SIDE as i32]);
        let outputs = self
            .model
            .run(
                vec![
                    (self.node("images")?, images.view().into()),
                    (self.node("orig_target_sizes")?, sizes.view().into()),
                ],
                &[
                    self.node("labels")?,
                    self.node("boxes")?,
                    self.node("scores")?,
                ],
                None,
            )
            .map_err(model_error)?;
        let [labels, boxes, scores]: [rten::Value; 3] = outputs
            .try_into()
            .map_err(|_| OcrError::Model("heron returned the wrong output count".to_owned()))?;
        let labels: Tensor<i32> = labels.try_into().map_err(model_error)?;
        let boxes: Tensor<f32> = boxes.try_into().map_err(model_error)?;
        let scores: Tensor<f32> = scores.try_into().map_err(model_error)?;
        let labels: NdTensor<i32, 2> = labels.try_into().map_err(model_error)?;
        let boxes: NdTensor<f32, 3> = boxes.try_into().map_err(model_error)?;
        let scores: NdTensor<f32, 2> = scores.try_into().map_err(model_error)?;
        let queries = scores.size(1);
        if labels.size(1) != queries || boxes.size(1) != queries || boxes.size(2) != 4 {
            return Err(OcrError::Model("heron output shapes disagree".to_owned()));
        }
        let to_page_x = raster.width() as f32 / SIDE as f32 / scale;
        let to_page_y = raster.height() as f32 / SIDE as f32 / scale;
        let mut regions = Vec::new();
        for q in 0..queries {
            let score = scores[[0, q]];
            if score.is_nan() || score < Self::THRESHOLD {
                continue;
            }
            let [x0, y0, x1, y1] = [0, 1, 2, 3].map(|i| boxes[[0, q, i]]);
            let bounds = OcrBox {
                x0: x0.min(x1).max(0.0) * to_page_x,
                y0: y0.min(y1).max(0.0) * to_page_y,
                x1: x0.max(x1).min(SIDE as f32) * to_page_x,
                y1: y0.max(y1).min(SIDE as f32) * to_page_y,
            };
            if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
                continue;
            }
            regions.push(LayoutRegion {
                kind: kind_of(labels[[0, q]]),
                bounds,
                confidence: score.min(1.0),
            });
        }
        sort_regions(&mut regions);
        Ok(regions)
    }
}
