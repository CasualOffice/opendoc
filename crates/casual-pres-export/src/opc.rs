// SPDX-License-Identifier: Apache-2.0

//! The OPC half of a written deck: part names, content types, and the
//! relationship graph a reader walks back.
//!
//! # Why the graph is built, not stamped
//!
//! A `.pptx` is not a bag of parts. `p:sldIdLst` names its slides by `r:id`, and
//! that list **is** the deck's order — so the writer has to mint a relationship id
//! per slide, write it into both the relationship part and the id list, and keep
//! the two in step. The same holds one level down: a slide names its layout by
//! `r:id`, a layout names its master, a master names each layout it offers, and a
//! `a:blip` names its image. Getting any of those pairs out of step produces a
//! package that opens and shows the wrong thing, which is worse than one that
//! refuses to open.
//!
//! So ids are assigned here, once, from the model's own ordering, and every writer
//! takes the id it needs from this module rather than guessing a convention.
//!
//! # Determinism
//!
//! Parts are written in sorted order with a fixed timestamp and stored (not
//! deflated) entries, so writing the same deck twice produces the same bytes.
//! `casual-doc-export::write_package` does this for DOCX for the same reason: a
//! reproducible package is what makes a byte-level diff of two exports meaningful.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::ExportError;

/// The package-relationship type URIs this writer emits.
pub(crate) mod rel {
    /// `/ppt/presentation.xml`, from the package root.
    pub(crate) const OFFICE_DOCUMENT: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
    /// A slide, from the presentation part.
    pub(crate) const SLIDE: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
    /// A slide master, from the presentation part.
    pub(crate) const SLIDE_MASTER: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
    /// A slide layout, from a master or a slide.
    pub(crate) const SLIDE_LAYOUT: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
    /// An image, from any part that paints one.
    pub(crate) const IMAGE: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
}

/// The content types this writer declares.
pub(crate) mod content_type {
    pub(crate) const PRESENTATION: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml";
    pub(crate) const SLIDE: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
    pub(crate) const SLIDE_LAYOUT: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml";
    pub(crate) const SLIDE_MASTER: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml";
    pub(crate) const RELATIONSHIPS: &str =
        "application/vnd.openxmlformats-package.relationships+xml";
}

/// One relationship, as written into a `_rels` part.
#[derive(Clone, Debug)]
pub(crate) struct Relationship {
    /// The `Id` attribute, minted by [`Relationships::add`].
    pub(crate) id: String,
    /// The relationship type URI.
    pub(crate) relationship_type: &'static str,
    /// The `Target`, **relative to the source part's own folder** — which is the
    /// form every producer writes and the form a reader resolves against.
    pub(crate) target: String,
}

/// One part's relationships, in the order they were added.
///
/// Ordered rather than keyed, because the ids are minted sequentially and the
/// written part must list them in a stable order for the package to be
/// reproducible.
#[derive(Debug, Default)]
pub(crate) struct Relationships {
    entries: Vec<Relationship>,
}

impl Relationships {
    /// Adds a relationship and returns the id minted for it.
    ///
    /// Ids are `rId1`, `rId2`, … in insertion order. The caller keeps the returned
    /// id and writes it into whatever element references the target, so the
    /// reference and the relationship part cannot drift apart.
    pub(crate) fn add(&mut self, relationship_type: &'static str, target: &str) -> String {
        let id = format!("rId{}", self.entries.len() + 1);
        self.entries.push(Relationship {
            id: id.clone(),
            relationship_type,
            target: target.to_owned(),
        });
        id
    }

    /// Whether nothing references anything, in which case no `_rels` part is
    /// written at all — an empty relationship part is legal but no producer writes
    /// one, and writing it would be a diff in every package.
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serializes to a `_rels` part.
    pub(crate) fn to_xml(&self) -> Vec<u8> {
        let mut xml = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        );
        for entry in &self.entries {
            xml.push_str(&format!(
                r#"<Relationship Id="{}" Type="{}" Target="{}"/>"#,
                entry.id, entry.relationship_type, entry.target
            ));
        }
        xml.push_str("</Relationships>");
        xml.into_bytes()
    }
}

/// `[Content_Types].xml`, accumulated as the parts are written.
///
/// Overrides are kept per part rather than collapsed to extension defaults,
/// because every PresentationML part is `.xml` and a single `xml` default cannot
/// distinguish a slide from a master. The `rels` default IS safe to use, since
/// every relationship part shares one type.
#[derive(Debug, Default)]
pub(crate) struct ContentTypes {
    /// extension -> content type.
    defaults: BTreeMap<String, &'static str>,
    /// `/absolute/part/name.xml` -> content type.
    overrides: BTreeMap<String, &'static str>,
}

impl ContentTypes {
    /// Declares the default for an extension, lowercased as OPC matches it.
    pub(crate) fn default_for(&mut self, extension: &str, content_type: &'static str) {
        self.defaults
            .insert(extension.to_ascii_lowercase(), content_type);
    }

    /// Declares one part's own type. `part` is the package-relative name, without
    /// a leading slash; the slash is added here so there is one place that knows
    /// the override form.
    pub(crate) fn override_for(&mut self, part: &str, content_type: &'static str) {
        self.overrides.insert(format!("/{part}"), content_type);
    }

    /// Serializes to `[Content_Types].xml`.
    pub(crate) fn to_xml(&self) -> Vec<u8> {
        let mut xml = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        );
        for (extension, content_type) in &self.defaults {
            xml.push_str(&format!(
                r#"<Default Extension="{extension}" ContentType="{content_type}"/>"#
            ));
        }
        for (part, content_type) in &self.overrides {
            xml.push_str(&format!(
                r#"<Override PartName="{part}" ContentType="{content_type}"/>"#
            ));
        }
        xml.push_str("</Types>");
        xml.into_bytes()
    }
}

/// The parts of a deck being assembled, keyed by package-relative name.
///
/// A `BTreeMap` so iteration — and therefore the ZIP's entry order — is sorted and
/// reproducible.
#[derive(Debug, Default)]
pub(crate) struct Parts {
    entries: BTreeMap<String, Vec<u8>>,
}

impl Parts {
    /// Adds a part. A duplicate name is a writer bug rather than bad input, so it
    /// is refused rather than silently overwriting: two parts at one name means
    /// one of them is unreachable in the package that gets written.
    pub(crate) fn add(&mut self, name: &str, bytes: Vec<u8>) -> Result<(), ExportError> {
        if self.entries.contains_key(name) {
            return Err(ExportError::DuplicatePart(name.to_owned()));
        }
        self.entries.insert(name.to_owned(), bytes);
        Ok(())
    }

    /// Replaces a part's bytes, for the one part whose content is only known once
    /// every other part has been written.
    ///
    /// Separate from [`Parts::add`] so the duplicate-name refusal there stays
    /// strict: `[Content_Types].xml` reserves its name up front and fills it in at
    /// the end, and that is the only name this is used for.
    pub(crate) fn replace(&mut self, name: &str, bytes: Vec<u8>) {
        self.entries.insert(name.to_owned(), bytes);
    }

    /// Zips the parts into a package.
    ///
    /// Stored rather than deflated, and with a fixed timestamp, so the bytes are
    /// reproducible for a given deck — which is what lets a round-trip guard
    /// compare two exports directly instead of reopening both.
    pub(crate) fn into_package(self) -> Result<Vec<u8>, ExportError> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(DateTime::default());
        for (name, bytes) in &self.entries {
            writer
                .start_file(name, options)
                .map_err(|_| ExportError::Package)?;
            writer.write_all(bytes).map_err(|_| ExportError::Package)?;
        }
        Ok(writer
            .finish()
            .map_err(|_| ExportError::Package)?
            .into_inner())
    }
}

/// The `_rels` part name for a part: `folder/_rels/name.rels`.
pub(crate) fn rels_name(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((folder, file)) => format!("{folder}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}
