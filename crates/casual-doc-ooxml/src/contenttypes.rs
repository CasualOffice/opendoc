//! `[Content_Types].xml` default and override resolution.

use std::collections::BTreeMap;

use crate::discovery::for_each_metadata_element;
use crate::error::PackageError;
use crate::package::CONTENT_TYPES_PART;

/// Parsed `[Content_Types].xml` default and override mappings.
#[derive(Debug, Default)]
pub(crate) struct ContentTypes {
    /// Lowercased extension to content type.
    defaults: BTreeMap<String, String>,
    /// Absolute part name (`/word/document.xml`) to content type.
    overrides: BTreeMap<String, String>,
}

impl ContentTypes {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, PackageError> {
        let mut this = Self::default();
        for_each_metadata_element(bytes, CONTENT_TYPES_PART, |name, attribute| {
            match name {
                b"Default" => {
                    let mut extension = None;
                    let mut content_type = None;
                    attribute(&mut |key, value| match key {
                        b"Extension" => extension = Some(value.to_ascii_lowercase()),
                        b"ContentType" => content_type = Some(value.to_owned()),
                        _ => {}
                    })?;
                    if let (Some(extension), Some(content_type)) = (extension, content_type) {
                        this.defaults.insert(extension, content_type);
                    }
                }
                b"Override" => {
                    let mut part_name = None;
                    let mut content_type = None;
                    attribute(&mut |key, value| match key {
                        b"PartName" => part_name = Some(value.to_owned()),
                        b"ContentType" => content_type = Some(value.to_owned()),
                        _ => {}
                    })?;
                    if let (Some(part_name), Some(content_type)) = (part_name, content_type) {
                        this.overrides.insert(part_name, content_type);
                    }
                }
                _ => {}
            }
            Ok(())
        })?;
        Ok(this)
    }

    /// Resolves the content type of a normalized package part, if declared.
    pub(crate) fn content_type_of(&self, part_name: &str) -> Option<&str> {
        let absolute = format!("/{part_name}");
        if let Some(content_type) = self.overrides.get(&absolute) {
            return Some(content_type);
        }
        let extension = part_name.rsplit_once('.').map(|(_, ext)| ext)?;
        self.defaults
            .get(&extension.to_ascii_lowercase())
            .map(String::as_str)
    }
}
