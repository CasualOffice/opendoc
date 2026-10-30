// SPDX-License-Identifier: Apache-2.0

//! The two facts the accessibility checker needs that the screen-reader
//! projection does not carry (`docs/109` HF-064): text whose colour is too close
//! to what is behind it, and the first caret position inside a table, so a table
//! finding has a Go to as every other finding does.
//!
//! # Low contrast, measured the way WCAG measures it
//!
//! Word's checker reports "Hard-to-read text contrast" against WCAG 2.1 §1.4.3:
//! 4.5:1 for body text, 3:1 for large text (18 pt, or 14 pt bold). This walks the
//! body's runs, resolves each one's colour through the same cascade layout uses
//! (`docDefaults`, the paragraph style chain, the character style, direct
//! formatting), and grades it against the nearest background that is actually
//! painted behind it: the run's highlight, then its shading, then the
//! paragraph's shading, then the table cell's, then the page's own colour, then
//! white.
//!
//! **Not graded, and said rather than implied:** automatic colour (Word draws it
//! black or white against whatever is behind it, so it is legible by
//! construction), theme colours (the theme palette is resolved inside the
//! layout crate and is not public to this one; a document that colours its text
//! from the theme is not checked), hidden text, and text inside headers,
//! footers, notes and text boxes. The checker's dialog names the same gaps.
//!
//! # Complexity
//!
//! O(body inlines), once per check, on request only — never per keystroke.

use std::str::FromStr;

use casual_doc_layout::cascade::StyleCascade;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Color, HighlightColor, InlineNode, Paragraph, RgbColor, RunProperties, Shading,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::WasmDocument;

/// WCAG 2.1 §1.4.3's floor for body text.
pub const TEXT_FLOOR: f64 = 4.5;
/// WCAG 2.1 §1.4.3's floor for large text.
pub const LARGE_TEXT_FLOOR: f64 = 3.0;

/// One run of text the checker grades as hard to read.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContrastFinding {
    /// The paragraph holding the run: where Go to puts the caret.
    pub node: String,
    /// The run's byte offset in that paragraph.
    pub offset: u32,
    /// The run's text, at most 40 characters, so the finding can be named.
    pub text: String,
    /// The text colour, six hex digits.
    pub foreground: String,
    /// The colour behind it, six hex digits.
    pub background: String,
    /// The contrast ratio, rounded to two places.
    pub ratio: f64,
    /// Whether WCAG counts the text as large (the 3:1 floor applies).
    pub large: bool,
}

/// WCAG 2.1 relative luminance of an sRGB colour.
fn luminance(color: RgbColor) -> f64 {
    let channel = |value: u8| {
        let v = f64::from(value) / 255.0;
        if v <= 0.039_28 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.0722_f64.mul_add(
        channel(color.b),
        0.2126_f64.mul_add(channel(color.r), 0.7152 * channel(color.g)),
    )
}

/// WCAG contrast ratio between two opaque colours, 1:1 to 21:1. Symmetric.
#[must_use]
pub fn contrast_ratio(a: RgbColor, b: RgbColor) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

const fn rgb(r: u8, g: u8, b: u8) -> RgbColor {
    RgbColor { r, g, b }
}

/// Word's sixteen highlight colours (`ST_HighlightColor`), as Word paints them.
const fn highlight_rgb(color: HighlightColor) -> Option<RgbColor> {
    Some(match color {
        HighlightColor::None => return None,
        HighlightColor::Black => rgb(0, 0, 0),
        HighlightColor::Blue => rgb(0, 0, 0xFF),
        HighlightColor::Cyan => rgb(0, 0xFF, 0xFF),
        HighlightColor::DarkBlue => rgb(0, 0, 0x80),
        HighlightColor::DarkCyan => rgb(0, 0x80, 0x80),
        HighlightColor::DarkGray => rgb(0x80, 0x80, 0x80),
        HighlightColor::DarkGreen => rgb(0, 0x80, 0),
        HighlightColor::DarkMagenta => rgb(0x80, 0, 0x80),
        HighlightColor::DarkRed => rgb(0x80, 0, 0),
        HighlightColor::DarkYellow => rgb(0x80, 0x80, 0),
        HighlightColor::Green => rgb(0, 0xFF, 0),
        HighlightColor::LightGray => rgb(0xC0, 0xC0, 0xC0),
        HighlightColor::Magenta => rgb(0xFF, 0, 0xFF),
        HighlightColor::Red => rgb(0xFF, 0, 0),
        HighlightColor::White => rgb(0xFF, 0xFF, 0xFF),
        HighlightColor::Yellow => rgb(0xFF, 0xFF, 0),
    })
}

/// A shading's painted fill, when it states an explicit one. A theme fill is
/// not resolved here (see the module comment) and reads as "no fill".
const fn shading_fill(shading: &Shading) -> Option<RgbColor> {
    shading.fill
}

fn hex(color: RgbColor) -> String {
    format!("{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

/// What a walk carries down: the cascade and the backgrounds above the run.
struct Walk<'a> {
    cascade: StyleCascade<'a>,
    page: RgbColor,
    out: Vec<ContrastFinding>,
}

impl Walk<'_> {
    fn blocks(&mut self, blocks: &[BlockNode], cell: Option<RgbColor>) {
        for block in blocks {
            match block {
                BlockNode::Paragraph(paragraph) => self.paragraph(paragraph, cell),
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for table_cell in &row.cells {
                            let fill = shading_fill(&table_cell.properties.shading).or(cell);
                            self.blocks(&table_cell.blocks, fill);
                        }
                    }
                }
                BlockNode::Sdt(sdt) => self.blocks(&sdt.blocks, cell),
                BlockNode::AltChunk(_) => {}
            }
        }
    }

    fn paragraph(&mut self, paragraph: &Paragraph, cell: Option<RgbColor>) {
        let direct = paragraph.properties.get();
        let style = self.cascade.paragraph_style(direct);
        let resolved = self.cascade.resolve_paragraph(direct);
        let behind = shading_fill(&resolved.shading)
            .or(cell)
            .unwrap_or(self.page);
        let mut offset = 0;
        // One finding per paragraph: the first hard-to-read run names the
        // problem, and twenty findings for one grey paragraph would bury the rest.
        if let Some(finding) = self.inlines(&paragraph.inlines, style, behind, &mut offset) {
            self.out.push(ContrastFinding {
                node: paragraph.id.to_string(),
                ..finding
            });
        }
    }

    fn inlines(
        &self,
        inlines: &[InlineNode],
        style: Option<casual_doc_model::v1::StyleId>,
        behind: RgbColor,
        offset: &mut u32,
    ) -> Option<ContrastFinding> {
        for inline in inlines {
            let start = *offset;
            match inline {
                InlineNode::Run(run) => {
                    let effective = self.cascade.resolve_run(style, run.properties.get());
                    if let Some(finding) = grade(&effective, behind) {
                        let text: String = run.text.chars().take(40).collect();
                        *offset += casual_doc_edit::inline_text_len(inline);
                        return Some(ContrastFinding {
                            offset: start,
                            text,
                            ..finding
                        });
                    }
                }
                InlineNode::Hyperlink(link) => {
                    if let Some(found) = self.inlines(&link.inlines, style, behind, offset) {
                        return Some(found);
                    }
                    continue;
                }
                InlineNode::Sdt(sdt) => {
                    if let Some(found) = self.inlines(&sdt.inlines, style, behind, offset) {
                        return Some(found);
                    }
                    continue;
                }
                _ => {}
            }
            *offset = start + casual_doc_edit::inline_text_len(inline);
        }
        None
    }
}

/// The finding for one run's resolved properties against `behind`, without its
/// position, or `None` when the run is legible or not graded.
fn grade(properties: &RunProperties, behind: RgbColor) -> Option<ContrastFinding> {
    if properties.hidden == Some(true) {
        return None;
    }
    let Some(Color::Rgb(foreground)) = properties.color else {
        return None;
    };
    let background = properties
        .highlight
        .and_then(highlight_rgb)
        .or_else(|| shading_fill(&properties.shading))
        .unwrap_or(behind);
    let size = properties.size_half_points.unwrap_or(22);
    let large = size >= 36 || (properties.bold == Some(true) && size >= 28);
    let ratio = contrast_ratio(foreground, background);
    let floor = if large { LARGE_TEXT_FLOOR } else { TEXT_FLOOR };
    (ratio < floor).then(|| ContrastFinding {
        node: String::new(),
        offset: 0,
        text: String::new(),
        foreground: hex(foreground),
        background: hex(background),
        ratio: (ratio * 100.0).round() / 100.0,
        large,
    })
}

impl WasmDocument {
    /// The body's hard-to-read runs, for the checker. O(body inlines).
    pub(crate) fn contrast_findings(&self) -> Vec<ContrastFinding> {
        let mut walk = Walk {
            cascade: StyleCascade::new(self.document.definitions()),
            page: self.document.background().unwrap_or(rgb(0xFF, 0xFF, 0xFF)),
            out: Vec::new(),
        };
        walk.blocks(self.document.body(), None);
        walk.out
    }

    /// The first paragraph inside the top-level body block `node` (a table, a
    /// content control or a paragraph), at any depth. O(body blocks).
    pub(crate) fn first_paragraph_in(&self, node: NodeId) -> Option<NodeId> {
        fn first(blocks: &[BlockNode]) -> Option<NodeId> {
            blocks.iter().find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id),
                BlockNode::Table(table) => table
                    .rows
                    .iter()
                    .flat_map(|row| row.cells.iter())
                    .find_map(|cell| first(&cell.blocks)),
                BlockNode::Sdt(sdt) => first(&sdt.blocks),
                BlockNode::AltChunk(_) => None,
            })
        }
        fn find(blocks: &[BlockNode], node: NodeId) -> Option<NodeId> {
            blocks.iter().find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => (paragraph.id == node).then_some(node),
                BlockNode::Table(table) if table.id == node => first(std::slice::from_ref(block)),
                BlockNode::Table(table) => table
                    .rows
                    .iter()
                    .flat_map(|row| row.cells.iter())
                    .find_map(|cell| find(&cell.blocks, node)),
                BlockNode::Sdt(sdt) if sdt.id == node => first(&sdt.blocks),
                BlockNode::Sdt(sdt) => find(&sdt.blocks, node),
                BlockNode::AltChunk(_) => None,
            })
        }
        find(self.document.body(), node)
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// The body's text that is hard to read against what is behind it, as a
    /// JSON array of `{node, offset, text, foreground, background, ratio,
    /// large}` — the accessibility checker's contrast rule (`docs/109` HF-064).
    /// What is and is not graded is this module's comment. O(body inlines).
    #[wasm_bindgen(js_name = accessibilityContrastFindings)]
    #[must_use]
    pub fn accessibility_contrast_findings(&self) -> String {
        serde_json::to_string(&self.contrast_findings()).unwrap_or_else(|_| "[]".to_owned())
    }

    /// The first paragraph inside the body block `node` — where Go to puts the
    /// caret for a finding about a table or a block content control — or
    /// `undefined` when `node` is not a body block. O(body blocks).
    #[wasm_bindgen(js_name = firstParagraphIn)]
    #[must_use]
    pub fn first_paragraph_in_js(&self, node: &str) -> Option<String> {
        let id = NodeId::from_str(node).ok()?;
        self.first_paragraph_in(id).map(|found| found.to_string())
    }
}
