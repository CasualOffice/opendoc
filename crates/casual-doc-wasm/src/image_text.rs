// SPDX-License-Identifier: Apache-2.0

//! Editing the text in a picture (`docs/182` §20b): the line boxes a scan or
//! a picture of text opens with, and what an edit of one does to the pixels
//! under it.
//!
//! An **image-text line box** is a text box inside a shape group, over a
//! picture of that group, whose text is laid out but not painted
//! (`RunProperties::no_text_fill`, `w14:textFill/w14:noFill`): the page shows
//! the picture's own pixels. The model needs no marker beyond that, so the
//! same rules hold for a box Word wrote, a box the PDF importer made, and a
//! box saved and opened again.
//!
//! Its states, and the one mechanism behind each:
//!
//! * **Unedited**: no patch, every run unpainted. The host asks
//!   [`caret_text_unpainted`](WasmDocument::caret_text_unpainted) before an
//!   edit at the caret (the cost of `caretFormat`, no walk).
//! * **Edited** ([`edit_image_text`](WasmDocument::edit_image_text) on an
//!   unedited box): the box's pixels are read out of the picture under it and
//!   `casual_doc_render::image_text::make_patch` makes the same box without
//!   its ink; the patch is a new picture of the group, directly under the
//!   text box, with the box's own offset, size and turn, so it sits exactly
//!   over the erased ink. The text is painted from then on, in the ink's
//!   measured colour, height and weight. The patch's alt text keeps the line
//!   as it was recognised, which is what Restore puts back. The original
//!   picture is never touched.
//! * **Edited, another fill** (the same call on an edited box): the patch's
//!   picture is made again from the original picture with the new mode.
//! * **Restored** ([`restore_image_text`](WasmDocument::restore_image_text)):
//!   the patch is removed and the line is its recognised text again,
//!   unpainted: the original pixels, exactly.
//!
//! Each is one `SetMediaReference` + `SetInlines` on the paragraph holding
//! the group, through the transaction path, so undo is exact. Saved as DOCX
//! the group is the picture, the patches and the text boxes, which Word opens
//! as they look here.
//!
//! # Complexity
//!
//! Finding the box is one walk of the top-level paragraphs, O(document), run
//! once per box edited (never per keystroke: the host gates it on the caret
//! query). The patch is O(box pixels) plus the picture's decode, bounded by
//! `MAX_PATCH_PIXELS`.

use casual_doc_edit::{Operation, Pos};
use casual_doc_model::NodeId;
// Own line: the edited line's face.
use casual_doc_model::v1::{
    BlockNode, Color, GroupChild, GroupPicture, GroupTextBox, InlineNode, MediaId, MediaReference,
    Paragraph, RgbColor, Run, RunProperties, WordprocessingGroup,
};
use casual_doc_model::v1::{FontName, FontRef};
use casual_doc_render::image_text::{self, BoxGeometry, FillMode};
use wasm_bindgen::prelude::*;

use crate::{EditResult, HistoryKind, WasmDocument, to_js};

/// The em of a line's text against its ink's height, ascender to
/// descender: the importer's `EM_PER_LINE_BOX`.
const EM_PER_INK: f64 = 1.1;

/// Layouts [`WasmDocument::fit_to_line`] may take.
const FIT_PASSES: usize = 4;
/// The smallest share of the measured size the fit shrinks a line to.
const MIN_FIT_SHARE: f64 = 0.6;

/// Why an image-text command refused, as a catalogue key the host shows.
pub(crate) const NOT_IMAGE_TEXT: &str = "imageText.refused.notImageText";
/// The picture under the box cannot be read.
pub(crate) const PICTURE_UNREADABLE: &str = "imageText.refused.pictureUnreadable";
/// An unknown fill mode.
pub(crate) const UNKNOWN_MODE: &str = "imageText.refused.unknownMode";

/// Where an image-text line box is: the top-level paragraph holding its
/// group, the group, and the box.
struct Located {
    anchor: NodeId,
    group: NodeId,
    text_box: NodeId,
}

/// Whether `paragraph` is a direct block of `text_box`.
fn box_holds(text_box: &GroupTextBox, paragraph: NodeId) -> bool {
    text_box
        .blocks
        .iter()
        .any(|b| matches!(b, BlockNode::Paragraph(p) if p.id == paragraph))
}

/// The group (at any nesting) holding a text box that holds `paragraph`.
fn group_of(group: &WordprocessingGroup, paragraph: NodeId) -> Option<(NodeId, NodeId)> {
    for child in &group.children {
        match child {
            GroupChild::TextBox(text_box) if box_holds(text_box, paragraph) => {
                return Some((group.id, text_box.id));
            }
            GroupChild::Group(nested) => {
                if let Some(found) = group_of(nested, paragraph) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_in_inlines(inlines: &[InlineNode], paragraph: NodeId) -> Option<(NodeId, NodeId)> {
    for inline in inlines {
        if let InlineNode::Group(group) = inline
            && let Some(found) = group_of(group, paragraph)
        {
            return Some(found);
        }
        if let Some(nested) = crate::contained_inlines(inline)
            && let Some(found) = find_in_inlines(nested, paragraph)
        {
            return Some(found);
        }
    }
    None
}

/// The group with id `id` in `inlines`, mutably, at any nesting.
fn group_mut(inlines: &mut [InlineNode], id: NodeId) -> Option<&mut WordprocessingGroup> {
    fn holds(group: &WordprocessingGroup, id: NodeId) -> bool {
        group.id == id
            || group
                .children
                .iter()
                .any(|child| matches!(child, GroupChild::Group(inner) if holds(inner, id)))
    }
    fn nested(group: &mut WordprocessingGroup, id: NodeId) -> Option<&mut WordprocessingGroup> {
        if group.id == id {
            return Some(group);
        }
        group.children.iter_mut().find_map(|child| match child {
            GroupChild::Group(inner) => nested(inner, id),
            _ => None,
        })
    }
    for inline in inlines.iter_mut() {
        if matches!(inline, InlineNode::Group(group) if holds(group, id)) {
            let InlineNode::Group(group) = inline else {
                return None;
            };
            return nested(group, id);
        }
        if let Some(inner) = crate::contained_inlines_mut(inline)
            && let Some(found) = group_mut(inner, id)
        {
            return Some(found);
        }
    }
    None
}

/// The rectangle a child covers in its group's child space, (x0, y0, x1, y1).
fn rect_of(offset: (i64, i64), extent: (i64, i64)) -> (i64, i64, i64, i64) {
    (offset.0, offset.1, offset.0 + extent.0, offset.1 + extent.1)
}

/// Whether `picture` is the patch of `text_box`: the same place, size and
/// turn. A patch is made that way, and only a patch is.
fn is_patch(picture: &GroupPicture, text_box: &GroupTextBox) -> bool {
    picture.offset == text_box.offset
        && picture.extent == text_box.extent
        && picture.rotation == text_box.rotation
}

/// The members a line box's state is read from: its index, its patch's
/// index (the picture directly under it, when it is its patch), and the
/// source picture (the largest picture of the group under the box's centre
/// that is no box's patch).
struct Members {
    text_box: usize,
    patch: Option<usize>,
    source: Option<usize>,
}

fn members(group: &WordprocessingGroup, text_box: NodeId) -> Option<Members> {
    let at = group
        .children
        .iter()
        .position(|c| matches!(c, GroupChild::TextBox(t) if t.id == text_box))?;
    let GroupChild::TextBox(boxed) = &group.children[at] else {
        return None;
    };
    let patch = at
        .checked_sub(1)
        .filter(|i| matches!(&group.children[*i], GroupChild::Picture(p) if is_patch(p, boxed)));
    let boxes: Vec<&GroupTextBox> = group
        .children
        .iter()
        .filter_map(|c| match c {
            GroupChild::TextBox(t) => Some(t),
            _ => None,
        })
        .collect();
    let (cx, cy) = (
        boxed.offset.x_emu + boxed.extent.width_emu / 2,
        boxed.offset.y_emu + boxed.extent.height_emu / 2,
    );
    let source = group
        .children
        .iter()
        .enumerate()
        .filter_map(|(i, c)| match c {
            GroupChild::Picture(p) if !boxes.iter().any(|t| is_patch(p, t)) => Some((i, p)),
            _ => None,
        })
        .filter(|(_, p)| {
            let r = rect_of(
                (p.offset.x_emu, p.offset.y_emu),
                (p.extent.width_emu, p.extent.height_emu),
            );
            cx > r.0 && cx < r.2 && cy > r.1 && cy < r.3
        })
        .max_by_key(|(_, p)| p.extent.width_emu.saturating_mul(p.extent.height_emu))
        .map(|(i, _)| i);
    Some(Members {
        text_box: at,
        patch,
        source,
    })
}

/// Every run of the box's paragraphs.
fn runs_of(text_box: &GroupTextBox) -> impl Iterator<Item = &Run> {
    text_box
        .blocks
        .iter()
        .filter_map(|b| match b {
            BlockNode::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| match i {
            InlineNode::Run(r) => Some(r),
            _ => None,
        })
}

/// The box's plain text, its paragraphs joined by newlines.
fn text_of(text_box: &GroupTextBox) -> String {
    text_box
        .blocks
        .iter()
        .filter_map(|b| match b {
            BlockNode::Paragraph(p) => Some(
                p.inlines
                    .iter()
                    .filter_map(|i| match i {
                        InlineNode::Run(r) => Some(r.text.as_str()),
                        _ => None,
                    })
                    .collect::<String>(),
            ),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A box's state: `unedited`, `edited`, or `None` when it is no image-text
/// line box.
fn state_of(group: &WordprocessingGroup, text_box: NodeId) -> Option<&'static str> {
    let m = members(group, text_box)?;
    let GroupChild::TextBox(boxed) = &group.children[m.text_box] else {
        return None;
    };
    m.source?;
    if m.patch.is_some() {
        return Some("edited");
    }
    let mut runs = runs_of(boxed).peekable();
    (runs.peek().is_some() && runs.all(|r| r.properties.no_text_fill == Some(true)))
        .then_some("unedited")
}

impl WasmDocument {
    /// Finds the line box holding `paragraph`. O(document).
    fn locate_image_text(&self, paragraph: NodeId) -> Option<Located> {
        let mut found = None;
        crate::visit_paragraphs_in(&self.document, &mut |p: &Paragraph| {
            if found.is_none()
                && let Some((group, text_box)) = find_in_inlines(&p.inlines, paragraph)
            {
                found = Some(Located {
                    anchor: p.id,
                    group,
                    text_box,
                });
            }
        });
        found
    }

    fn anchor_inlines(&self, anchor: NodeId) -> Option<Vec<InlineNode>> {
        casual_doc_edit::find_paragraph_any(&self.document, anchor).map(|p| p.inlines.clone())
    }

    /// The patch for the box at `members.text_box` from its source picture:
    /// the PNG and the line's measured style. O(box pixels + picture).
    fn patch_for(
        &self,
        group: &WordprocessingGroup,
        m: &Members,
        mode: FillMode,
    ) -> Result<(Vec<u8>, image_text::LineStyle, f64), String> {
        let (Some(source), GroupChild::TextBox(boxed)) = (m.source, &group.children[m.text_box])
        else {
            return Err(NOT_IMAGE_TEXT.to_owned());
        };
        let GroupChild::Picture(picture) = &group.children[source] else {
            return Err(NOT_IMAGE_TEXT.to_owned());
        };
        let reference = self
            .document
            .definitions()
            .media
            .get(&picture.media)
            .ok_or_else(|| PICTURE_UNREADABLE.to_owned())?;
        let bytes = self
            .resources
            .get(&reference.part_name)
            .ok_or_else(|| PICTURE_UNREADABLE.to_owned())?;
        let pixels = image_text::decode(bytes).map_err(|_| PICTURE_UNREADABLE.to_owned())?;
        // Pixels per EMU of the picture as the group places it.
        let sx = pixels.width as f64 / picture.extent.width_emu.max(1) as f64;
        let sy = pixels.height as f64 / picture.extent.height_emu.max(1) as f64;
        let cx = (boxed.offset.x_emu - picture.offset.x_emu) as f64
            + boxed.extent.width_emu as f64 / 2.0;
        let cy = (boxed.offset.y_emu - picture.offset.y_emu) as f64
            + boxed.extent.height_emu as f64 / 2.0;
        let geometry = BoxGeometry {
            cx: cx * sx,
            cy: cy * sy,
            width: boxed.extent.width_emu as f64 * sx,
            height: boxed.extent.height_emu as f64 * sy,
            rotation_degrees: f64::from(boxed.rotation.unwrap_or(0)) / 60_000.0,
        };
        let patch = image_text::make_patch(&pixels, &geometry, mode).map_err(|e| e.to_string())?;
        let png = image_text::encode_png(&patch.image).map_err(|e| e.to_string())?;
        Ok((png, patch.style, sy))
    }

    /// Registers `png` as a new editor media part. Returns its id and the
    /// operation that declares it.
    fn patch_media(&mut self, png: Vec<u8>) -> Result<(MediaId, Operation), String> {
        let seq = self
            .edit_ids
            .next_id()
            .map_err(|_| "id space exhausted".to_owned())?;
        let media = MediaId::new(seq);
        // The editor-owned part naming `insertImage` uses: never an imported
        // part's name.
        let part_name = format!("word/media/editor-{seq}.png");
        self.resources.insert(part_name.clone(), png);
        Ok((
            media,
            Operation::SetMediaReference {
                id: media,
                reference: Some(Box::new(MediaReference {
                    relationship_id: format!("rIdEditorImg{seq}"),
                    media_type: "image/png".to_owned(),
                    part_name,
                })),
            },
        ))
    }

    /// [`edit_image_text`](Self::edit_image_text) without `JsValue`.
    pub(crate) fn edit_image_text_inner(
        &mut self,
        paragraph: &str,
        offset: u32,
        mode: &str,
    ) -> Result<EditResult, String> {
        let mode = FillMode::from_token(mode).ok_or_else(|| UNKNOWN_MODE.to_owned())?;
        let paragraph: NodeId = paragraph.parse().map_err(|_| NOT_IMAGE_TEXT.to_owned())?;
        let located = self
            .locate_image_text(paragraph)
            .ok_or_else(|| NOT_IMAGE_TEXT.to_owned())?;
        let mut inlines = self
            .anchor_inlines(located.anchor)
            .ok_or_else(|| NOT_IMAGE_TEXT.to_owned())?;
        let group_snapshot = group_mut(&mut inlines, located.group)
            .ok_or_else(|| NOT_IMAGE_TEXT.to_owned())?
            .clone();
        state_of(&group_snapshot, located.text_box).ok_or_else(|| NOT_IMAGE_TEXT.to_owned())?;
        let m =
            members(&group_snapshot, located.text_box).ok_or_else(|| NOT_IMAGE_TEXT.to_owned())?;
        let (png, style, pixels_per_emu) = self.patch_for(&group_snapshot, &m, mode)?;
        let (media, declare) = self.patch_media(png)?;
        let group =
            group_mut(&mut inlines, located.group).ok_or_else(|| NOT_IMAGE_TEXT.to_owned())?;
        if let Some(patch) = m.patch {
            // Another fill for an edited box: the same patch, new pixels.
            if let GroupChild::Picture(picture) = &mut group.children[patch] {
                picture.media = media;
            }
        } else {
            let GroupChild::TextBox(boxed) = &mut group.children[m.text_box] else {
                return Err(NOT_IMAGE_TEXT.to_owned());
            };
            let recognised = text_of(boxed);
            // Painted from now on, as the ink it replaces looked.
            let size = (style.ink_height > 0).then(|| {
                let points = style.ink_height as f64 / pixels_per_emu / 12_700.0;
                ((points * EM_PER_INK * 2.0).round() as u32).clamp(2, 3276)
            });
            for block in &mut boxed.blocks {
                let BlockNode::Paragraph(p) = block else {
                    continue;
                };
                for inline in &mut p.inlines {
                    if let InlineNode::Run(run) = inline {
                        let mut properties: RunProperties = (*run.properties).clone();
                        properties.no_text_fill = None;
                        let [r, g, b] = style.ink;
                        properties.color = Some(Color::Rgb(RgbColor { r, g, b }));
                        if style.bold {
                            properties.bold = Some(true);
                        }
                        if let Some(size) = size {
                            properties.size_half_points = Some(size);
                        }
                        // The nearest bundled face to the ink's (stroke
                        // contrast for serif, an even pitch for mono).
                        let family = Some(FontRef::Named(FontName {
                            name: style.face.family().to_owned(),
                        }));
                        properties.font_ref.clone_from(&family);
                        properties.font_ref_h_ansi = family;
                        run.properties = properties.into();
                    }
                }
            }
            let patch = GroupPicture {
                id: self
                    .edit_ids
                    .next_id()
                    .map_err(|_| "id space exhausted".to_owned())?,
                media,
                offset: boxed.offset,
                extent: boxed.extent,
                // The line as it was recognised: what Restore puts back.
                descr: Some(recognised),
                crop: None,
                opacity: None,
                hyperlink: None,
                border: None,
                flip_h: false,
                flip_v: false,
                rotation: boxed.rotation,
            };
            group
                .children
                .insert(m.text_box, GroupChild::Picture(patch));
        }
        let revealing = m.patch.is_none();
        let result = self.apply_action_caret_as(
            vec![
                declare,
                Operation::SetInlines {
                    node: located.anchor,
                    inlines,
                },
            ],
            Pos::new(paragraph, offset),
            HistoryKind::ImageText,
        )?;
        if !revealing {
            return Ok(result);
        }
        Ok(self
            .fit_to_line(&located, paragraph, offset)?
            .unwrap_or(result))
    }

    /// The edited line set on one line, as the scan's was: while the laid-out
    /// text wraps, its size shrinks by the overflow (a substitute face sets
    /// wider than the scan's), down to [`MIN_FIT_SHARE`] of the measured
    /// size, in the same undo step as the edit. At most [`FIT_PASSES`]
    /// layouts. `None` when it already fits.
    fn fit_to_line(
        &mut self,
        located: &Located,
        paragraph: NodeId,
        offset: u32,
    ) -> Result<Option<EditResult>, String> {
        let mut last = None;
        let mut floor = None;
        for _ in 0..FIT_PASSES {
            let Some(text_box) = self.text_box(located) else {
                break;
            };
            let length: usize = runs_of(&text_box).map(|r| r.text.len()).sum();
            let id = paragraph.to_string();
            let rects =
                self.selection_rects(&id, 0, &id, u32::try_from(length).unwrap_or(u32::MAX));
            let lines: std::collections::BTreeSet<i32> =
                rects.chunks_exact(5).map(|r| r[2]).collect();
            if lines.len() <= 1 {
                break;
            }
            let used: i64 = rects.chunks_exact(5).map(|r| i64::from(r[3])).sum();
            let scale = self.group_scale(located).unwrap_or(1.0);
            let room = (text_box.extent.width_emu
                - i64::from(text_box.body_properties.insets.left_emu)
                - i64::from(text_box.body_properties.insets.right_emu))
                as f64
                / 635.0
                * scale;
            let share = (room / used.max(1) as f64 * 0.97).min(0.97);
            let mut inlines = self
                .anchor_inlines(located.anchor)
                .ok_or_else(|| NOT_IMAGE_TEXT.to_owned())?;
            let group =
                group_mut(&mut inlines, located.group).ok_or_else(|| NOT_IMAGE_TEXT.to_owned())?;
            let mut changed = false;
            for child in &mut group.children {
                let GroupChild::TextBox(boxed) = child else {
                    continue;
                };
                if boxed.id != located.text_box {
                    continue;
                }
                for block in &mut boxed.blocks {
                    let BlockNode::Paragraph(p) = block else {
                        continue;
                    };
                    for inline in &mut p.inlines {
                        if let InlineNode::Run(run) = inline {
                            let mut properties: RunProperties = (*run.properties).clone();
                            let size = f64::from(properties.size_half_points.unwrap_or(22));
                            let least = *floor.get_or_insert((size * MIN_FIT_SHARE).max(8.0));
                            let next = (size * share).floor().max(least);
                            if next < size {
                                properties.size_half_points = Some(next as u32);
                                run.properties = properties.into();
                                changed = true;
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
            let ops = vec![Operation::SetInlines {
                node: located.anchor,
                inlines,
            }];
            self.apply_group(
                &ops,
                HistoryKind::ImageText.label(),
                casual_doc_transaction::Coalesce::Continue,
                crate::Origin::Edit,
            )?;
            last =
                Some(self.finish_edit_with(Pos::new(paragraph, offset), &crate::damage_of(&ops)));
        }
        Ok(last)
    }

    /// The located text box, as it is now.
    fn text_box(&self, located: &Located) -> Option<GroupTextBox> {
        let mut inlines = self.anchor_inlines(located.anchor)?;
        let group = group_mut(&mut inlines, located.group)?;
        group.children.iter().find_map(|c| match c {
            GroupChild::TextBox(t) if t.id == located.text_box => Some(t.clone()),
            _ => None,
        })
    }

    /// How much the group scales its child space onto the page.
    fn group_scale(&self, located: &Located) -> Option<f64> {
        let mut inlines = self.anchor_inlines(located.anchor)?;
        let group = group_mut(&mut inlines, located.group)?;
        Some(group.extent.width_emu as f64 / group.transform.child_extent.width_emu.max(1) as f64)
    }

    /// [`restore_image_text`](Self::restore_image_text) without `JsValue`.
    pub(crate) fn restore_image_text_inner(
        &mut self,
        paragraph: &str,
    ) -> Result<EditResult, String> {
        let paragraph: NodeId = paragraph.parse().map_err(|_| NOT_IMAGE_TEXT.to_owned())?;
        let located = self
            .locate_image_text(paragraph)
            .ok_or_else(|| NOT_IMAGE_TEXT.to_owned())?;
        let mut inlines = self
            .anchor_inlines(located.anchor)
            .ok_or_else(|| NOT_IMAGE_TEXT.to_owned())?;
        let group =
            group_mut(&mut inlines, located.group).ok_or_else(|| NOT_IMAGE_TEXT.to_owned())?;
        let m = members(group, located.text_box).ok_or_else(|| NOT_IMAGE_TEXT.to_owned())?;
        let Some(patch_at) = m.patch else {
            return Err(NOT_IMAGE_TEXT.to_owned());
        };
        let GroupChild::Picture(patch) = group.children.remove(patch_at) else {
            return Err(NOT_IMAGE_TEXT.to_owned());
        };
        let recognised = patch.descr.unwrap_or_default();
        let GroupChild::TextBox(boxed) = &mut group.children[m.text_box - 1] else {
            return Err(NOT_IMAGE_TEXT.to_owned());
        };
        // The first run's look, unpainted, holding the recognised line.
        let first = runs_of(boxed).next().map(|r| (*r.properties).clone());
        let mut properties = first.unwrap_or_default();
        properties.no_text_fill = Some(true);
        let Some(BlockNode::Paragraph(first_paragraph)) = boxed
            .blocks
            .iter()
            .find(|b| matches!(b, BlockNode::Paragraph(_)))
            .cloned()
        else {
            return Err(NOT_IMAGE_TEXT.to_owned());
        };
        let run = InlineNode::Run(Run {
            id: self
                .edit_ids
                .next_id()
                .map_err(|_| "id space exhausted".to_owned())?,
            properties: properties.into(),
            text: recognised,
        });
        let restored = Paragraph {
            inlines: if matches!(&run, InlineNode::Run(r) if r.text.is_empty()) {
                Vec::new()
            } else {
                vec![run]
            },
            ..first_paragraph
        };
        boxed.blocks = vec![BlockNode::Paragraph(restored)];
        let caret = Pos::new(
            match &boxed.blocks[0] {
                BlockNode::Paragraph(p) => p.id,
                _ => paragraph,
            },
            0,
        );
        self.apply_action_caret_as(
            vec![Operation::SetInlines {
                node: located.anchor,
                inlines,
            }],
            caret,
            HistoryKind::ImageText,
        )
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// Whether the text at the caret is laid out but not painted
    /// (`w14:textFill/w14:noFill`): the host's gate before an edit there, so
    /// an unedited image-text line box is made editable first
    /// ([`edit_image_text`](Self::edit_image_text)). The cost of
    /// `caretFormat`: the caret's run, no document walk.
    #[wasm_bindgen(js_name = caretTextUnpainted)]
    #[must_use]
    pub fn caret_text_unpainted(&self, node: &str, offset: u32) -> bool {
        node.parse::<NodeId>().ok().is_some_and(|node| {
            crate::effective_caret_run_properties(&self.document, node, offset)
                .is_some_and(|p| p.no_text_fill == Some(true))
        })
    }

    /// The state of the image-text line box holding `paragraph`:
    /// `"unedited"` (its text unpainted over the picture's pixels),
    /// `"edited"` (a patch under it, its text painted), or `""` when the
    /// paragraph is in no such box. What enables the Picture text commands.
    /// O(document).
    #[wasm_bindgen(js_name = imageTextState)]
    #[must_use]
    pub fn image_text_state(&self, paragraph: &str) -> String {
        let Ok(paragraph) = paragraph.parse::<NodeId>() else {
            return String::new();
        };
        let Some(located) = self.locate_image_text(paragraph) else {
            return String::new();
        };
        let Some(inlines) = self.anchor_inlines(located.anchor) else {
            return String::new();
        };
        let mut inlines = inlines;
        group_mut(&mut inlines, located.group)
            .and_then(|g| state_of(g, located.text_box))
            .unwrap_or("")
            .to_owned()
    }

    /// Makes the line box holding `paragraph` editable, or gives an edited
    /// one another fill: erases the picture's ink under the box by `mode`
    /// (`fill`, `blur` or `pixelate`) in a patch picture placed under the
    /// box, and paints the box's text in the ink's colour, height and weight
    /// (`docs/182` §20b). The picture itself is never changed. One undoable
    /// action; the caret is left at `paragraph`/`offset`. O(document) to find
    /// the box, plus O(box pixels).
    ///
    /// # Errors
    ///
    /// A catalogue key: `imageText.refused.notImageText`,
    /// `imageText.refused.pictureUnreadable` or `imageText.refused.unknownMode`.
    #[wasm_bindgen(js_name = editImageText)]
    pub fn edit_image_text(
        &mut self,
        paragraph: &str,
        offset: u32,
        mode: &str,
    ) -> Result<EditResult, JsValue> {
        self.edit_image_text_inner(paragraph, offset, mode)
            .map_err(to_js)
    }

    /// Restores the original pixels under an edited line box: removes its
    /// patch and makes its text the recognised line again, unpainted. One
    /// undoable action. O(document).
    ///
    /// # Errors
    ///
    /// `imageText.refused.notImageText` when the box was never edited.
    #[wasm_bindgen(js_name = restoreImageText)]
    pub fn restore_image_text(&mut self, paragraph: &str) -> Result<EditResult, JsValue> {
        self.restore_image_text_inner(paragraph).map_err(to_js)
    }
}

/// The line boxes' text of `document`'s first image-text group, for tests.
#[cfg(test)]
fn first_group(document: &casual_doc_model::v1::Document) -> Option<WordprocessingGroup> {
    document.body().iter().find_map(|b| match b {
        BlockNode::Paragraph(p) => p.inlines.iter().find_map(|i| match i {
            InlineNode::Group(g) => Some((**g).clone()),
            _ => None,
        }),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::v1::{
        AnchorHorizontal, AnchorVertical, DrawingAnchor, Extent, GroupTransform, HorizontalAnchor,
        HorizontalPosition, ParagraphProperties, PointEmu, ShapeGeometry, TextBoxBodyProperties,
        VerticalAnchor, VerticalPosition, WrapDistances, WrapMode,
    };
    use casual_doc_render::image_text::Rgba;

    /// EMU per pixel at 96 dpi.
    const EMU: i64 = 9_525;

    fn id(n: u64) -> NodeId {
        NodeId::from_parts(7, n).unwrap()
    }

    /// A 600 x 400 "scan": warm paper with a gentle gradient and a line of
    /// dark stems at (50..350, 100..130), and a second line at (50..350,
    /// 200..230).
    fn scan() -> (Rgba, Rgba) {
        let mut paper = Rgba::filled(600, 400, [0, 0, 0, 255]);
        for y in 0..400 {
            for x in 0..600 {
                let v = (238.0 + 10.0 * x as f64 / 600.0) as u8;
                paper.set(x, y, [v, v - 3, v - 12, 255]);
            }
        }
        let mut page = paper.clone();
        for top in [104usize, 204] {
            let mut x = 60;
            while x < 340 {
                for y in top..top + 22 {
                    for xx in x..x + 3 {
                        page.set(xx, y, [25, 25, 40, 255]);
                    }
                }
                x += 12;
            }
        }
        (page, paper)
    }

    fn line_box(n: u64, y_px: i64, text: &str) -> GroupChild {
        GroupChild::TextBox(GroupTextBox {
            id: id(n),
            offset: PointEmu {
                x_emu: 50 * EMU,
                y_emu: y_px * EMU,
            },
            extent: Extent {
                width_emu: 300 * EMU,
                height_emu: 30 * EMU,
            },
            geometry: ShapeGeometry::Rectangle,
            preset: None,
            adjustments: Vec::new(),
            blocks: vec![BlockNode::Paragraph(Paragraph {
                id: id(n + 1),
                properties: ParagraphProperties::default().into(),
                inlines: vec![InlineNode::Run(Run {
                    id: id(n + 2),
                    properties: RunProperties {
                        no_text_fill: Some(true),
                        ..RunProperties::default()
                    }
                    .into(),
                    text: text.to_owned(),
                })],
            })],
            fill: None,
            border: None,
            shadow: None,
            body_properties: TextBoxBodyProperties::default(),
            hyperlink: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        })
    }

    fn document() -> WasmDocument {
        document_with("First recognised line")
    }

    fn document_with(first: &str) -> WasmDocument {
        let media = MediaId::new(id(900));
        let mut definitions = casual_doc_model::v1::Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rId9".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/scan.png".to_owned(),
            },
        );
        let extent = Extent {
            width_emu: 600 * EMU,
            height_emu: 400 * EMU,
        };
        let group = WordprocessingGroup {
            id: id(10),
            anchor: Some(DrawingAnchor {
                horizontal: AnchorHorizontal {
                    relative_from: HorizontalAnchor::Column,
                    position: HorizontalPosition::Offset(0),
                },
                vertical: AnchorVertical {
                    relative_from: VerticalAnchor::Paragraph,
                    position: VerticalPosition::Offset(0),
                },
                wrap: WrapMode::TopAndBottom,
                wrap_text: None,
                wrap_distances: WrapDistances::default(),
                wrap_polygon: None,
                behind_doc: false,
                allow_overlap: true,
            }),
            relative_height: Some(1),
            extent,
            transform: GroupTransform {
                offset: PointEmu { x_emu: 0, y_emu: 0 },
                extent,
                child_offset: PointEmu { x_emu: 0, y_emu: 0 },
                child_extent: extent,
                flip_h: false,
                flip_v: false,
                rotation: None,
            },
            hyperlink: None,
            children: vec![
                GroupChild::Picture(GroupPicture {
                    id: id(11),
                    media,
                    offset: PointEmu { x_emu: 0, y_emu: 0 },
                    extent,
                    descr: None,
                    crop: None,
                    opacity: None,
                    hyperlink: None,
                    border: None,
                    flip_h: false,
                    flip_v: false,
                    rotation: None,
                }),
                line_box(20, 100, first),
                line_box(30, 200, "Second line"),
            ],
        };
        let document = casual_doc_model::v1::Document::new(
            id(1),
            vec![BlockNode::Paragraph(Paragraph {
                id: id(2),
                properties: ParagraphProperties::default().into(),
                inlines: vec![InlineNode::Group(Box::new(group))],
            })],
            definitions,
        )
        .expect("valid");
        let mut doc = crate::tests::wasm_document(document);
        let (page, _) = scan();
        doc.resources.insert(
            "word/media/scan.png".to_owned(),
            image_text::encode_png(&page).unwrap(),
        );
        doc
    }

    /// The patch picture under the box holding paragraph `p`, decoded.
    fn patch_pixels(doc: &WasmDocument) -> Option<(GroupPicture, Rgba)> {
        let group = first_group(&doc.document)?;
        let picture = group.children.iter().find_map(|c| match c {
            GroupChild::Picture(p) if p.id != id(11) => Some(p.clone()),
            _ => None,
        })?;
        let part = &doc
            .document
            .definitions()
            .media
            .get(&picture.media)?
            .part_name;
        let pixels = image_text::decode(doc.resources.get(part)?).ok()?;
        Some((picture, pixels))
    }

    #[test]
    fn an_unedited_line_box_is_found_and_reads_unedited() {
        let doc = document();
        assert!(doc.caret_text_unpainted(&id(21).to_string(), 3));
        assert_eq!(doc.image_text_state(&id(21).to_string()), "unedited");
        assert_eq!(
            doc.image_text_state(&id(2).to_string()),
            "",
            "the anchor is no box"
        );
    }

    /// The owner's test: edit one line, and its glyphs are gone from under
    /// it (the patch is the paper within tolerance and holds no ink colour);
    /// the other line and the picture are untouched; undo is exact.
    #[test]
    fn editing_a_line_erases_its_ink_in_a_patch_and_leaves_the_rest() {
        let mut doc = document();
        let before = doc.document.clone();
        let picture_bytes = doc.resources.get("word/media/scan.png").unwrap().to_vec();
        doc.edit_image_text_inner(&id(21).to_string(), 5, "fill")
            .expect("edited");
        assert_eq!(doc.image_text_state(&id(21).to_string()), "edited");
        assert_eq!(
            doc.image_text_state(&id(31).to_string()),
            "unedited",
            "the other line is untouched"
        );
        let (patch, pixels) = patch_pixels(&doc).expect("a patch");
        assert_eq!(patch.descr.as_deref(), Some("First recognised line"));
        assert_eq!((pixels.width, pixels.height), (300, 30));
        let (_, paper) = scan();
        for y in 0..30 {
            for x in 0..300 {
                let p = pixels.get(x, y);
                let truth = paper.get(50 + x, 100 + y);
                for c in 0..3 {
                    assert!(
                        p[c].abs_diff(truth[c]) <= 8,
                        "patch ({x},{y}) {p:?} strays from the paper {truth:?}"
                    );
                }
            }
        }
        // The picture's bytes are never changed.
        assert_eq!(
            doc.resources.get("word/media/scan.png").unwrap(),
            picture_bytes
        );
        // The text is painted now, in the ink's colour.
        let group = first_group(&doc.document).unwrap();
        let GroupChild::TextBox(edited) = &group.children[2] else {
            panic!(
                "the patch sits directly under its box: {:?}",
                group.children[1]
            );
        };
        let run = runs_of(edited).next().unwrap();
        assert_eq!(run.properties.no_text_fill, None);
        let Some(Color::Rgb(ink)) = run.properties.color else {
            panic!("an ink colour");
        };
        assert!(ink.r < 60 && ink.b < 80, "{ink:?}");
        // Undo is exact.
        doc.undo_inner().expect("undo");
        assert_eq!(doc.document, before);
    }

    /// The edited line is set on one line, as the scan's was: a substitute
    /// face that sets wider than the ink shrinks, in the same undo step.
    #[test]
    fn an_edited_line_that_would_wrap_is_shrunk_onto_one_line() {
        let long = "A longer recognised line, wider";
        let mut doc = document_with(long);
        let before = doc.document.clone();
        doc.edit_image_text_inner(&id(21).to_string(), 0, "fill")
            .unwrap();
        let id21 = id(21).to_string();
        let rects = doc.selection_rects(&id21, 0, &id21, long.len() as u32);
        let lines: std::collections::BTreeSet<i32> = rects.chunks_exact(5).map(|r| r[2]).collect();
        assert_eq!(lines.len(), 1, "the line wraps: {rects:?}");
        let group = first_group(&doc.document).unwrap();
        let GroupChild::TextBox(boxed) = &group.children[2] else {
            panic!("the box");
        };
        let size = runs_of(boxed)
            .next()
            .unwrap()
            .properties
            .size_half_points
            .unwrap();
        assert!(size < 36, "shrunk from the measured 18 pt: {size}");
        // One undo takes the edit and the fit away together.
        doc.undo_inner().expect("undo");
        assert_eq!(doc.document, before);
    }

    /// The face is the nearest bundled one to the ink's: the synthetic
    /// scan's stems stand on an even pitch, so it is set in Liberation Mono.
    #[test]
    fn the_edited_line_takes_the_nearest_bundled_face() {
        let mut doc = document();
        doc.edit_image_text_inner(&id(21).to_string(), 0, "fill")
            .unwrap();
        let group = first_group(&doc.document).unwrap();
        let GroupChild::TextBox(boxed) = &group.children[2] else {
            panic!("the box");
        };
        let run = runs_of(boxed).next().unwrap();
        assert_eq!(
            run.properties.font_ref,
            Some(FontRef::Named(FontName {
                name: "Liberation Mono".to_owned()
            }))
        );
    }

    #[test]
    fn another_fill_remakes_the_patch_and_restore_brings_the_original_back() {
        let mut doc = document();
        doc.edit_image_text_inner(&id(21).to_string(), 0, "fill")
            .unwrap();
        let (_, filled) = patch_pixels(&doc).unwrap();
        doc.edit_image_text_inner(&id(21).to_string(), 0, "pixelate")
            .unwrap();
        let (_, pixelated) = patch_pixels(&doc).unwrap();
        assert_ne!(filled, pixelated, "the fill changed");
        let group = first_group(&doc.document).unwrap();
        assert_eq!(group.children.len(), 4, "one patch, not two");
        // Restore: no patch, the recognised line, unpainted.
        let result = doc.restore_image_text_inner(&id(21).to_string()).unwrap();
        drop(result);
        let group = first_group(&doc.document).unwrap();
        assert_eq!(group.children.len(), 3);
        let GroupChild::TextBox(boxed) = &group.children[1] else {
            panic!("the box");
        };
        assert_eq!(text_of(boxed), "First recognised line");
        assert!(runs_of(boxed).all(|r| r.properties.no_text_fill == Some(true)));
    }

    #[test]
    fn a_box_that_is_not_over_a_picture_is_refused_with_a_reason() {
        let mut doc = document();
        assert_eq!(
            doc.edit_image_text_inner(&id(2).to_string(), 0, "fill")
                .err()
                .as_deref(),
            Some(NOT_IMAGE_TEXT)
        );
        assert_eq!(
            doc.edit_image_text_inner(&id(21).to_string(), 0, "smudge")
                .err()
                .as_deref(),
            Some(UNKNOWN_MODE)
        );
        assert_eq!(
            doc.restore_image_text_inner(&id(21).to_string())
                .err()
                .as_deref(),
            Some(NOT_IMAGE_TEXT),
            "an unedited box has nothing to restore"
        );
    }

    /// Saved as DOCX and opened again, the edit is the picture, its patch
    /// and the painted text box; the unedited line is still unpainted.
    #[test]
    fn an_edited_box_round_trips_through_docx() {
        let mut doc = document();
        doc.edit_image_text_inner(&id(21).to_string(), 0, "fill")
            .unwrap();
        let bytes = doc.export_docx().expect("export");
        let reopened = crate::open_document(&bytes).expect("reopen");
        let group = first_group(&reopened.document).expect("the group");
        let pictures = group
            .children
            .iter()
            .filter(|c| matches!(c, GroupChild::Picture(_)))
            .count();
        assert_eq!(pictures, 2, "the scan and the patch");
        let boxes: Vec<(String, bool)> = group
            .children
            .iter()
            .filter_map(|c| match c {
                GroupChild::TextBox(t) => Some((
                    text_of(t),
                    runs_of(t).all(|r| r.properties.no_text_fill == Some(true)),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            boxes,
            [
                ("First recognised line".to_owned(), false),
                ("Second line".to_owned(), true)
            ]
        );
    }
}
