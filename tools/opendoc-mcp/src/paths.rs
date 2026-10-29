// SPDX-License-Identifier: Apache-2.0

//! The operator's path allowlist (`docs/178` §5.6). Every path is canonicalised
//! (symlinks resolved) and then prefix-checked against roots the operator named
//! on the command line. A client can never widen them, and a refusal never
//! echoes the resolved target.

use std::path::{Path, PathBuf};

use casual_doc_ai::AiError;

/// Extensions `open_document` accepts. `.docm` is refused, as it is in the
/// editor (`SKILL` §11).
pub const OPENABLE: &[&str] = &["docx", "dotx", "odt", "rtf", "txt", "json", "pdf"];

/// Canonical read and write roots.
#[derive(Clone, Debug, Default)]
pub struct Roots {
    read: Vec<PathBuf>,
    write: Vec<PathBuf>,
}

fn outside() -> AiError {
    AiError::new(
        "path-outside-roots",
        "That path is outside the directories this server was started with.",
    )
}

impl Roots {
    /// Canonicalises the operator's roots. A root that does not exist is an
    /// error at launch rather than a silent narrowing.
    pub fn new(read: &[PathBuf], write: &[PathBuf]) -> Result<Self, String> {
        let canonical = |paths: &[PathBuf]| -> Result<Vec<PathBuf>, String> {
            paths
                .iter()
                .map(|path| {
                    path.canonicalize()
                        .map_err(|error| format!("root {}: {error}", path.display()))
                })
                .collect()
        };
        let write = canonical(write)?;
        let mut read = canonical(read)?;
        // A write root is readable too: what the server wrote, it may reopen.
        for root in &write {
            if !read.contains(root) {
                read.push(root.clone());
            }
        }
        Ok(Self { read, write })
    }

    /// The read roots.
    pub fn read_roots(&self) -> &[PathBuf] {
        &self.read
    }

    /// Whether a canonical path lies inside a read root.
    pub fn contains(&self, canonical: &Path) -> bool {
        self.read.iter().any(|root| canonical.starts_with(root))
    }

    /// A file the server may read, canonical. Relative paths resolve against
    /// the first read root, never against the process's working directory.
    pub fn readable(&self, requested: &str) -> Result<PathBuf, AiError> {
        let path = self.anchor(requested, &self.read)?;
        let canonical = path.canonicalize().map_err(|_| {
            AiError::new(
                "not-found",
                "No readable file is at that path inside the allowed directories.",
            )
        })?;
        if !self.read.iter().any(|root| canonical.starts_with(root)) {
            return Err(outside());
        }
        if !canonical.is_file() {
            return Err(AiError::new("not-found", "That path is not a file."));
        }
        Ok(canonical)
    }

    /// A file the server may create or replace, canonical. The parent
    /// directory must exist inside a write root; an existing target must not
    /// be a symlink.
    pub fn writable(&self, requested: &str) -> Result<PathBuf, AiError> {
        if self.write.is_empty() {
            return Err(AiError::new(
                "no-write-root",
                "This server was started without a write root; export to a resource instead \
                 (omit `path`).",
            ));
        }
        let path = self.anchor(requested, &self.write)?;
        let name = path
            .file_name()
            .ok_or_else(|| AiError::invalid("The path names no file."))?
            .to_owned();
        let parent = path
            .parent()
            .ok_or_else(outside)?
            .canonicalize()
            .map_err(|_| outside())?;
        if !self.write.iter().any(|root| parent.starts_with(root)) {
            return Err(outside());
        }
        let target = parent.join(name);
        if let Ok(meta) = std::fs::symlink_metadata(&target)
            && meta.file_type().is_symlink()
        {
            return Err(outside());
        }
        Ok(target)
    }

    fn anchor(&self, requested: &str, roots: &[PathBuf]) -> Result<PathBuf, AiError> {
        if requested.is_empty() || requested.contains('\0') {
            return Err(AiError::invalid("The path is empty."));
        }
        let requested = requested.strip_prefix("file://").unwrap_or(requested);
        let path = Path::new(requested);
        if path.is_absolute() {
            return Ok(path.to_owned());
        }
        roots
            .first()
            .map(|root| root.join(path))
            .ok_or_else(outside)
    }
}

/// Whether `path` has an extension `open_document` accepts.
pub fn openable(path: &Path) -> Result<(), AiError> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if extension == "docm" {
        return Err(AiError::new(
            "unsupported-format",
            "Macro-enabled documents (.docm) are refused, as they are in the editor.",
        ));
    }
    if !OPENABLE.contains(&extension.as_str()) {
        return Err(AiError::new(
            "unsupported-format",
            format!("Only {} files open.", OPENABLE.join(", ")),
        ));
    }
    Ok(())
}
