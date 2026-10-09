//! Built-in deterministic single-file HTML exporter.
//!
//! # Why this exists
//!
//! `docs/153` `shell.export-html`, rank 3. ONLYOFFICE's Download-as grid offers
//! **HTML (Zipped)** — a folder of files: the markup plus every picture as a
//! sibling, delivered as a `.zip` a reader has to unpack before anything
//! displays. Ours is **one file**: the pictures are `data:` URIs inside it, the
//! stylesheet is inline, and nothing it references is off the disk. That is the
//! same local-first argument the rest of this product rests on, applied to an
//! export: a file you can open in a browser with the network off, or attach to
//! a mail, is more useful than an archive.
//!
//! # It looks like the document, because it is resolved like the page
//!
//! Real documents put most of their formatting in **styles** — a Title, a
//! Heading 1, a table style with a banded header — and in the theme those
//! styles name. An exporter that reads only direct formatting writes a heading
//! as browser-default black serif and a table as the browser's grid, which is
//! what this one did until ADR-066 (`docs/167`). Now every paragraph, run and
//! table cell is resolved through the same `casual_doc_layout` cascade the
//! canvas paints with — document defaults, table style and its conditional
//! regions, paragraph style chain, character style, direct formatting — and
//! the theme's colours and fonts through the same palette and font scheme
//! (`casual_doc_layout::paint_values`). There is one resolver, so the page and
//! the export cannot disagree about what a document looks like.
//!
//! What the result carries: fonts (with the metric-compatible face the page
//! draws when the reader lacks the authored one), sizes, colours, weights,
//! decoration styles, highlight and shading, alignment, indents, Word's
//! additive paragraph spacing, line spacing, paragraph borders, real nested
//! lists with the labels the page prints, tables with column widths, merges,
//! resolved borders, fills and padding, pictures at their size (cropped,
//! rotated, floated to their side), footnotes and endnotes as linked notes,
//! bookmarks as anchors, tracked changes as `<ins>`/`<del>`, the first
//! section's page size and margins for printing, and multi-column sections as
//! CSS columns.
//!
//! # The shape, named first
//!
//! A *serializer* with the same skeleton as [`text`](crate::PlainTextAdapter)
//! and [`markdown`](crate::MarkdownAdapter): one linear walk of the normalized
//! model, a bounded output buffer, and a compatibility report for everything
//! the target cannot carry. Its stylesheet is *normalized*: one class per
//! paragraph style holding that style's resolved formatting, and each element
//! carrying only where it differs — the shape Word's and LibreOffice's own HTML
//! filters use. Nothing here is a new kind of component.
//!
//! # Safety
//!
//! Every text node and every attribute value is escaped, and no declaration is
//! copied from document text: each is re-serialized from a typed, resolved
//! model field. The two pieces of document text that do reach the stylesheet —
//! a font family name and a list label — are reduced to characters that cannot
//! end a string, a declaration, an attribute or the `<style>` element
//! (`css::font_name`, `css::css_string`). A document cannot inject markup into
//! its own export, which is the whole reason this file escapes rather than
//! templating.
//!
//! # Complexity
//!
//! One pass over the body, so O(nodes); a style's resolved class is computed
//! once; a picture is copied once into its `data:` URI. Media are resolved by
//! one B-tree lookup per drawing, never by a scan (`SKILL` §8).

mod css;
mod writer;

use std::collections::BTreeMap;

use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, ExportArtifact, ExportMode,
    ExportRequest, FeatureLocation, FormatDescriptor, FormatExporter, FormatId, FormatProfile,
    ModelOutcome, RetentionOutcome, formats,
};

/// The media type registered for HTML, and the one the artifact carries.
pub const HTML_MIME: &str = "text/html";

/// Host-configurable HTML limits with non-bypassable hard ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HtmlLimits {
    /// Maximum bytes of emitted HTML, `data:` URIs included.
    pub max_output_bytes: usize,
    /// Maximum bytes of one picture before it is replaced by its alt text.
    ///
    /// A `data:` URI costs four bytes for every three, so a large picture is
    /// where a single-file export actually becomes unusable. Past this the
    /// picture is reported rather than embedded, which is a smaller lie than a
    /// 200 MB HTML file.
    pub max_embedded_bytes: usize,
    /// Maximum block-nesting depth followed before the walk refuses.
    pub max_nesting_depth: usize,
}

impl HtmlLimits {
    /// Hard maximum emitted bytes.
    pub const HARD_MAX_OUTPUT_BYTES: usize = 256 * 1024 * 1024;
    /// Hard maximum embedded bytes for one picture.
    pub const HARD_MAX_EMBEDDED_BYTES: usize = 64 * 1024 * 1024;
    /// Hard maximum nesting depth.
    pub const HARD_MAX_NESTING_DEPTH: usize = 256;

    fn validate(self) -> Result<(), AdapterError> {
        for (name, value, ceiling) in [
            (
                "html_output_bytes",
                self.max_output_bytes,
                Self::HARD_MAX_OUTPUT_BYTES,
            ),
            (
                "html_embedded_bytes",
                self.max_embedded_bytes,
                Self::HARD_MAX_EMBEDDED_BYTES,
            ),
            (
                "html_nesting_depth",
                self.max_nesting_depth,
                Self::HARD_MAX_NESTING_DEPTH,
            ),
        ] {
            if value > ceiling {
                return Err(AdapterError::new(format!(
                    "limit {name} value {value} exceeds hard ceiling {ceiling}"
                )));
            }
        }
        Ok(())
    }
}

impl Default for HtmlLimits {
    fn default() -> Self {
        Self {
            max_output_bytes: 64 * 1024 * 1024,
            max_embedded_bytes: 8 * 1024 * 1024,
            max_nesting_depth: 32,
        }
    }
}

/// Built-in bounded single-file HTML exporter.
#[derive(Clone, Debug)]
pub struct HtmlAdapter {
    descriptor: FormatDescriptor,
    limits: HtmlLimits,
}

impl HtmlAdapter {
    /// Creates an exporter with explicit HTML limits.
    #[must_use]
    pub fn new(limits: HtmlLimits) -> Self {
        Self {
            descriptor: FormatDescriptor {
                id: FormatId::new(formats::HTML).expect("built-in html id is valid"),
                display_name: "Web Page".to_owned(),
                mime_types: vec![HTML_MIME.to_owned()],
                extensions: vec!["html".to_owned(), "htm".to_owned()],
                // Export only, for the same reason Markdown is: reading HTML
                // needs a parser and a sanitiser, and only the writer exists.
                can_import: false,
                can_export: true,
                exact_if_unchanged: false,
                preserve_when_safe: false,
            },
            limits,
        }
    }
}

impl Default for HtmlAdapter {
    fn default() -> Self {
        Self::new(HtmlLimits::default())
    }
}

impl FormatExporter for HtmlAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn export(&self, request: ExportRequest<'_>) -> Result<ExportArtifact, AdapterError> {
        self.limits.validate()?;
        request
            .document
            .validate()
            .map_err(|error| AdapterError::new(format!("normalized model: {error}")))?;
        if request.mode == ExportMode::ExactIfUnchanged {
            return Err(AdapterError::new(
                "html has no importer, so there are no retained source bytes to return \
                 exactly; ask for the semantic mode",
            ));
        }

        let (bytes, mut losses) =
            writer::Writer::new(request.document, request.resources, self.limits).write()?;
        if request.source.is_some() {
            losses.record("html.source_envelope", ModelOutcome::Omitted);
        }

        Ok(ExportArtifact {
            bytes,
            report: losses.finish(),
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: Some("html5".to_owned()),
            },
            mime_type: HTML_MIME.to_owned(),
            suggested_extension: "html".to_owned(),
        })
    }
}

fn enforce(name: &'static str, observed: usize, allowed: usize) -> Result<(), AdapterError> {
    if observed > allowed {
        return Err(AdapterError::new(format!(
            "limit {name} observed {observed} exceeds allowed {allowed}"
        )));
    }
    Ok(())
}

/// Standard base64, no line breaks, as a `data:` URI needs.
///
/// Vendored as twenty lines rather than as a dependency: one alphabet, one
/// caller, and `deny.toml` is a gate this project keeps deliberately narrow.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Escapes a text node. `<`, `>` and `&` end or begin markup; the rest is text.
fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(character),
        }
    }
    out
}

/// Escapes an attribute value, which additionally must not close its own quote.
fn escape_attribute(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(character),
        }
    }
    out
}

/// Every loss this exporter can report.
#[cfg(test)]
const FEATURES: &[&str] = &[
    "html.alt_chunk",
    "html.anchor_position",
    "html.break_within_paragraph",
    "html.content_control",
    "html.dangling_bookmark",
    "html.dangling_note",
    "html.document_author",
    "html.embedded_object",
    "html.field_instruction",
    "html.group_shape",
    "html.grouped_drawing",
    "html.header_footer",
    "html.math_markup",
    "html.picture_bytes_absent",
    "html.picture_over_embedding_limit",
    "html.range_or_comment_marker",
    "html.section_layout",
    "html.source_envelope",
    "html.symbol_font",
    "html.tab_stop",
    "html.table_float_position",
    "html.text_box",
    "html.watermark",
];

/// What the export could not carry, counted per feature.
#[derive(Debug, Default)]
struct Losses {
    entries: BTreeMap<&'static str, (u32, ModelOutcome)>,
}

impl Losses {
    fn record(&mut self, feature: &'static str, outcome: ModelOutcome) {
        let count = 1;
        self.entries
            .entry(feature)
            .and_modify(|entry| entry.0 = entry.0.saturating_add(count))
            .or_insert((count, outcome));
    }

    fn finish(self) -> CompatibilityReport {
        let mut report = CompatibilityReport {
            entries: self
                .entries
                .into_iter()
                .map(
                    |(feature, (occurrences, model_outcome))| CompatibilityEntry {
                        feature: feature.to_owned(),
                        occurrences,
                        location: FeatureLocation {
                            local_name: Some(feature.to_owned()),
                            ..FeatureLocation::default()
                        },
                        model_outcome,
                        retention_outcome: RetentionOutcome::NotRetained,
                    },
                )
                .collect(),
        };
        report.sort();
        report
    }
}

#[cfg(test)]
mod tests;
