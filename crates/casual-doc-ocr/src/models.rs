//! The model set a host supplies (`docs/182` §20a): bytes it obtained under
//! its own policy, typed by role, turned into providers on demand.
//!
//! This crate never fetches a model. A host builds an [`OcrModels`] from
//! bytes ([`OcrModels::new`] and the `with_` builders) or, natively, from a
//! directory laid out as `fetch-test-models.sh` writes it
//! ([`OcrModels::from_dir`]). With none, nothing is built and the importer
//! reports `pdf.import.ocr_unavailable`.

use std::fmt;

#[cfg(any(feature = "paddle", feature = "layout-heron"))]
use crate::OcrError;
#[cfg(feature = "paddle")]
use crate::{PaddleOcr, RecognitionPack};

/// One recognition model and its character dictionary, by name.
#[derive(Clone)]
pub struct RecognitionModel {
    /// The pack's name (`rec`, `latin_rec`, ...), recorded in the manifest.
    pub name: String,
    /// The ONNX bytes.
    pub model: Vec<u8>,
    /// The dictionary, one entry per line in label order.
    pub dictionary: String,
}

impl fmt::Debug for RecognitionModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecognitionModel")
            .field("name", &self.name)
            .field("model_bytes", &self.model.len())
            .finish_non_exhaustive()
    }
}

/// The PP-OCRv5 models a host supplies: detection, one or more recognition
/// packs, and optionally the text-line orientation classifier
/// (`PP-LCNet_x1_0_textline_ori`) and the `docling-layout-heron` layout
/// model. Bytes only; identity (SHA-256 per model) is recorded by the
/// providers' manifests when they are built.
#[derive(Clone)]
pub struct OcrModels {
    detection: Vec<u8>,
    recognition: Vec<RecognitionModel>,
    orientation: Option<Vec<u8>>,
    layout: Option<Vec<u8>>,
}

impl fmt::Debug for OcrModels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OcrModels")
            .field("detection_bytes", &self.detection.len())
            .field("recognition", &self.recognition)
            .field("orientation", &self.orientation.as_ref().map(Vec::len))
            .field("layout", &self.layout.as_ref().map(Vec::len))
            .finish()
    }
}

/// Why a model directory could not be read. Names a file, never its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OcrModelsError {
    /// A required file is absent (the detection model, or every
    /// recognition pack).
    Missing(String),
    /// A file could not be read, is not UTF-8 where a dictionary must be,
    /// or is over [`OcrModels::MAX_MODEL_BYTES`].
    Unreadable(String),
    /// A file resolves (through a symbolic link) outside the directory.
    OutsideDirectory(String),
}

impl fmt::Display for OcrModelsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(what) => write!(f, "OCR model missing: {what}"),
            Self::Unreadable(what) => write!(f, "OCR model unreadable: {what}"),
            Self::OutsideDirectory(what) => {
                write!(f, "OCR model resolves outside its directory: {what}")
            }
        }
    }
}

impl std::error::Error for OcrModelsError {}

impl OcrModels {
    /// The largest model file [`OcrModels::from_dir`] reads (the heron
    /// layout model is 171 MB; the PP-OCRv5 graphs are under 20 MB).
    pub const MAX_MODEL_BYTES: u64 = 512 * 1024 * 1024;

    /// Detection plus the first recognition pack.
    #[must_use]
    pub fn new(detection: Vec<u8>, recognition: RecognitionModel) -> Self {
        Self {
            detection,
            recognition: vec![recognition],
            orientation: None,
            layout: None,
        }
    }

    /// One more recognition pack, read after the ones before it.
    #[must_use]
    pub fn with_recognition(mut self, pack: RecognitionModel) -> Self {
        self.recognition.push(pack);
        self
    }

    /// The text-line orientation classifier.
    #[must_use]
    pub fn with_orientation(mut self, model: Vec<u8>) -> Self {
        self.orientation = Some(model);
        self
    }

    /// The layout model (`docling-layout-heron`, ONNX).
    #[must_use]
    pub fn with_layout(mut self, model: Vec<u8>) -> Self {
        self.layout = Some(model);
        self
    }

    /// The recognition packs' names, in reading order.
    #[must_use]
    pub fn recognition_names(&self) -> Vec<&str> {
        self.recognition.iter().map(|r| r.name.as_str()).collect()
    }

    /// Whether the orientation classifier was supplied.
    #[must_use]
    pub fn has_orientation(&self) -> bool {
        self.orientation.is_some()
    }

    /// Whether a layout model was supplied.
    #[must_use]
    pub fn has_layout(&self) -> bool {
        self.layout.is_some()
    }

    /// Reads a directory laid out as `crates/casual-doc-ocr/fetch-test-models.sh`
    /// writes it:
    ///
    /// ```text
    /// <dir>/paddle/det.onnx                  detection (required)
    /// <dir>/paddle/<name>.onnx + <name>.dict.txt   a recognition pack (one or more)
    /// <dir>/paddle/textline_ori.onnx         orientation (optional)
    /// <dir>/heron.onnx                       layout (optional)
    /// ```
    ///
    /// Packs are read in name order, except that `rec` (the multilingual
    /// pack) comes first. Every file is canonicalised and must stay inside
    /// `dir`; none may exceed [`OcrModels::MAX_MODEL_BYTES`]. Nothing is
    /// fetched. O(model bytes).
    ///
    /// # Errors
    ///
    /// [`OcrModelsError`] naming the file.
    #[cfg(not(target_family = "wasm"))]
    pub fn from_dir(dir: &std::path::Path) -> Result<Self, OcrModelsError> {
        use std::path::Path;
        let root = dir
            .canonicalize()
            .map_err(|_| OcrModelsError::Missing(dir.display().to_string()))?;
        let read = |path: &Path| -> Result<Vec<u8>, OcrModelsError> {
            // Named relative to the model directory with `/` on every platform, so
            // an operator's message and the tests read the same on Windows.
            let shown = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let canonical = path
                .canonicalize()
                .map_err(|_| OcrModelsError::Missing(shown.clone()))?;
            if !canonical.starts_with(&root) {
                return Err(OcrModelsError::OutsideDirectory(shown));
            }
            let meta = std::fs::metadata(&canonical)
                .map_err(|_| OcrModelsError::Unreadable(shown.clone()))?;
            if !meta.is_file() || meta.len() > Self::MAX_MODEL_BYTES {
                return Err(OcrModelsError::Unreadable(shown));
            }
            std::fs::read(&canonical).map_err(|_| OcrModelsError::Unreadable(shown))
        };
        let paddle = root.join("paddle");
        let detection = read(&paddle.join("det.onnx"))?;
        let mut names: Vec<String> = std::fs::read_dir(&paddle)
            .map_err(|_| OcrModelsError::Missing("paddle".to_owned()))?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                name.strip_suffix(".dict.txt").map(str::to_owned)
            })
            .filter(|name| paddle.join(format!("{name}.onnx")).is_file())
            .collect();
        names.sort_by(|a, b| (a != "rec", a).cmp(&(b != "rec", b)));
        if names.is_empty() {
            return Err(OcrModelsError::Missing(
                "paddle/<name>.onnx with paddle/<name>.dict.txt".to_owned(),
            ));
        }
        let mut recognition = Vec::with_capacity(names.len());
        for name in names {
            let model = read(&paddle.join(format!("{name}.onnx")))?;
            let dictionary = String::from_utf8(read(&paddle.join(format!("{name}.dict.txt")))?)
                .map_err(|_| OcrModelsError::Unreadable(format!("paddle/{name}.dict.txt")))?;
            recognition.push(RecognitionModel {
                name,
                model,
                dictionary,
            });
        }
        let optional = |path: std::path::PathBuf| -> Result<Option<Vec<u8>>, OcrModelsError> {
            if path.exists() {
                read(&path).map(Some)
            } else {
                Ok(None)
            }
        };
        Ok(Self {
            detection,
            recognition,
            orientation: optional(paddle.join("textline_ori.onnx"))?,
            layout: optional(root.join("heron.onnx"))?,
        })
    }

    /// The PP-OCRv5 recognition provider over these models (with the
    /// orientation classifier when supplied). O(model bytes).
    ///
    /// # Errors
    ///
    /// [`OcrError::Model`] when a model does not load.
    #[cfg(feature = "paddle")]
    pub fn recognizer(&self) -> Result<PaddleOcr, OcrError> {
        let packs = self
            .recognition
            .iter()
            .map(|r| RecognitionPack {
                name: r.name.clone(),
                model: r.model.clone(),
                dictionary: r.dictionary.clone(),
            })
            .collect();
        let provider = PaddleOcr::from_model_bytes(self.detection.clone(), packs)?;
        match &self.orientation {
            Some(model) => provider.with_orientation(model.clone()),
            None => Ok(provider),
        }
    }

    /// The heron layout provider, when a layout model was supplied.
    ///
    /// # Errors
    ///
    /// [`OcrError::Model`] when the model does not load.
    #[cfg(feature = "layout-heron")]
    pub fn layout(&self) -> Option<Result<crate::HeronLayout, OcrError>> {
        self.layout
            .as_ref()
            .map(|model| crate::HeronLayout::from_model_bytes(model.clone()))
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "casual-doc-ocr-models-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("paddle")).unwrap();
        dir
    }

    #[test]
    fn a_directory_reads_by_role_with_the_multilingual_pack_first() {
        let dir = scratch("roles");
        for (file, bytes) in [
            ("paddle/det.onnx", "D"),
            ("paddle/latin_rec.onnx", "L"),
            ("paddle/latin_rec.dict.txt", "a\nb\n"),
            ("paddle/rec.onnx", "R"),
            ("paddle/rec.dict.txt", "x\n"),
            // A dictionary with no model next to it is not a pack.
            ("paddle/orphan.dict.txt", "z\n"),
            ("paddle/textline_ori.onnx", "O"),
        ] {
            std::fs::write(dir.join(file), bytes).unwrap();
        }
        let models = OcrModels::from_dir(&dir).unwrap();
        assert_eq!(models.recognition_names(), vec!["rec", "latin_rec"]);
        assert!(models.has_orientation());
        assert!(!models.has_layout());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_directory_without_detection_or_packs_is_refused_by_name() {
        let dir = scratch("missing");
        assert_eq!(
            OcrModels::from_dir(&dir).unwrap_err(),
            OcrModelsError::Missing("paddle/det.onnx".to_owned())
        );
        std::fs::write(dir.join("paddle/det.onnx"), "D").unwrap();
        assert!(matches!(
            OcrModels::from_dir(&dir).unwrap_err(),
            OcrModelsError::Missing(_)
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_model_linked_from_outside_the_directory_is_refused() {
        let dir = scratch("link");
        let outside = scratch("link-target");
        std::fs::write(outside.join("det.onnx"), "D").unwrap();
        std::os::unix::fs::symlink(outside.join("det.onnx"), dir.join("paddle/det.onnx")).unwrap();
        assert_eq!(
            OcrModels::from_dir(&dir).unwrap_err(),
            OcrModelsError::OutsideDirectory("paddle/det.onnx".to_owned())
        );
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }
}
