// SPDX-License-Identifier: Apache-2.0

//! Picture and object lowerings the editor and agents share (`docs/181` §4.2,
//! step 5): inserting a picture, alt text, size, wrap, crop, deletion, and a
//! shape's fill and outline.
//!
//! The editor's methods resolve the selected object and call these; an agent
//! names the object by the id `read_blocks` lists under `objects`.
//!
//! # How an agent supplies a picture's bytes
//!
//! Inline, as base64 in the edit (`data`), with its MIME type, bounded by
//! [`MAX_IMAGE_BYTES`] decoded. The bytes are checked to be the format the MIME
//! type names (its magic number), so a mislabelled or hostile payload is
//! refused before anything is written. No resource reference scheme exists
//! yet for MCP clients to point at, and base64 is what every client can send;
//! the ceiling keeps one edit bounded. The engine owns no image codec
//! (`docs/85` Q8), so the natural size is read from the header of the formats
//! whose header states it (PNG, GIF, JPEG, BMP) at 96 DPI, or given in points.

use casual_doc_edit::table_structure::MintId;
use casual_doc_edit::{Operation, Pos, find_shape, refused, surface_block_lists};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, CROP_FULL, CropRect, Document, Drawing, DrawingAnchor, Extent, Fill, InlineNode,
    LockFlags, MAX_EMU, MediaId, MediaReference, ObjectLocks, Rgba, ShapeStroke, WrapMode,
    contained_inlines,
};
use casual_doc_model::v1::{DashStyle, LineEnd};

fn next(ids: MintId<'_>) -> Result<NodeId, String> {
    ids().ok_or_else(|| "id space exhausted".to_owned())
}

/// Changing the wrap of something that has no wrap.
pub const NOT_FLOATING_WRAP: &str = refused!(
    "object.not-floating-wrap",
    "Only a floating object has text wrapping to change."
);

/// The largest picture an agent may insert in one edit, decoded: 8 MiB.
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// EMU per point.
pub const EMU_PER_POINT: i64 = 12_700;

/// The package media type and file extension of an image MIME type the
/// renderer and the writer both accept. O(1).
#[must_use]
pub fn image_part_type(mime: &str) -> Option<(&'static str, &'static str)> {
    Some(match mime.to_ascii_lowercase().as_str() {
        "image/png" => ("image/png", "png"),
        "image/jpeg" | "image/jpg" => ("image/jpeg", "jpeg"),
        "image/gif" => ("image/gif", "gif"),
        "image/bmp" | "image/x-ms-bmp" => ("image/bmp", "bmp"),
        "image/tiff" => ("image/tiff", "tiff"),
        "image/webp" => ("image/webp", "webp"),
        _ => return None,
    })
}

/// A lowered picture insertion.
#[derive(Clone, Debug)]
pub struct Picture {
    /// The operations, one step: the media registration, the drawing, its
    /// aspect lock.
    pub operations: Vec<Operation>,
    /// The package part the caller writes the bytes to, in the same step: the
    /// model holds the reference, the resource map holds the stream.
    pub part_name: String,
    /// The new drawing.
    pub drawing: NodeId,
}

/// Inserts a picture of `mime` at `at`, `width` × `height` EMU. The media
/// sequence id and the drawing id are minted in that order. The registration
/// is an operation in the same step as the drawing that names it (undo takes
/// both), and the picture carries Word's `noChangeAspect` frame lock, so a
/// corner drag keeps its proportions here and in Word. O(1).
///
/// # Errors
/// An unsupported MIME type, a size outside 1..=`MAX_EMU`, or ids running out.
pub fn insert_picture(
    at: Pos,
    mime: &str,
    width: i64,
    height: i64,
    ids: MintId<'_>,
) -> Result<Picture, String> {
    let (media_type, ext) =
        image_part_type(mime).ok_or_else(|| format!("unsupported image type {mime:?}"))?;
    for (value, label) in [(width, "width"), (height, "height")] {
        if !(1..=MAX_EMU).contains(&value) {
            return Err(format!("{label} is out of bounds"));
        }
    }
    let media_seq = next(ids)?;
    let drawing_id = next(ids)?;
    let media = MediaId::new(media_seq);
    // An editor-authored part name, unique by construction, so it can never
    // collide with an imported part.
    let part_name = format!("word/media/editor-{media_seq}.{ext}");
    let reference = MediaReference {
        relationship_id: format!("rIdEditorImg{media_seq}"),
        media_type: media_type.to_owned(),
        part_name: part_name.clone(),
    };
    let drawing = Drawing {
        // An inserted picture is not linked (`docs/109` HF-179).
        hyperlink: None,
        opacity: None,
        id: drawing_id,
        media,
        extent: Some(Extent {
            width_emu: width,
            height_emu: height,
        }),
        descr: None,
        crop: None,
        border: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    };
    let locks = ObjectLocks {
        frame: LockFlags {
            no_change_aspect: true,
            ..LockFlags::default()
        },
        object: LockFlags::default(),
    };
    Ok(Picture {
        operations: vec![
            // Registration first: an operation is never ordered before what it
            // references.
            Operation::SetMediaReference {
                id: media,
                reference: Some(Box::new(reference)),
            },
            Operation::InsertInlineObject {
                at,
                node: Box::new(InlineNode::Drawing(Box::new(drawing))),
            },
            // After the drawing: the lock names it.
            Operation::SetObjectLocks {
                object: drawing_id,
                locks,
            },
        ],
        part_name,
        drawing: drawing_id,
    })
}

/// Decodes standard base64 (RFC 4648 §4, padding optional, ASCII whitespace
/// ignored), refusing anything else and anything that decodes past `limit`
/// bytes before decoding it. O(input).
///
/// # Errors
/// A character outside the alphabet, bad padding, or a payload over `limit`.
pub fn decode_base64(input: &str, limit: usize) -> Result<Vec<u8>, String> {
    let clean: Vec<u8> = input
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let trimmed = clean
        .strip_suffix(b"==")
        .or_else(|| clean.strip_suffix(b"="))
        .unwrap_or(&clean);
    let padded = clean.len() != trimmed.len();
    if trimmed.len() % 4 == 1 || (padded && !clean.len().is_multiple_of(4)) {
        return Err("The base64 data is truncated or wrongly padded.".to_owned());
    }
    if trimmed.len() / 4 * 3 > limit {
        return Err(format!(
            "A picture is at most {} MiB.",
            limit / (1024 * 1024)
        ));
    }
    let value = |byte: u8| -> Result<u32, String> {
        Ok(u32::from(match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("The data is not standard base64.".to_owned()),
        }))
    };
    let mut out = Vec::with_capacity(trimmed.len() / 4 * 3 + 2);
    for chunk in trimmed.chunks(4) {
        let mut acc = 0u32;
        for (index, byte) in chunk.iter().enumerate() {
            acc |= value(*byte)? << (18 - 6 * index);
        }
        let bytes = acc.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    if out.len() > limit {
        return Err(format!(
            "A picture is at most {} MiB.",
            limit / (1024 * 1024)
        ));
    }
    Ok(out)
}

/// Whether `bytes` start with the signature of `media_type` (one of
/// [`image_part_type`]'s). O(1).
#[must_use]
pub fn bytes_are(media_type: &str, bytes: &[u8]) -> bool {
    match media_type {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "image/bmp" => bytes.starts_with(b"BM"),
        "image/tiff" => bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*"),
        "image/webp" => bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        _ => false,
    }
}

/// The pixel size a picture's header states, for PNG, GIF, JPEG and BMP;
/// `None` for the formats whose size needs a decoder. O(header), never more
/// than one pass over the JPEG marker chain.
#[must_use]
pub fn pixel_size(media_type: &str, bytes: &[u8]) -> Option<(u32, u32)> {
    let be16 = |at: usize| {
        Some(u32::from(u16::from_be_bytes([
            *bytes.get(at)?,
            *bytes.get(at + 1)?,
        ])))
    };
    let le16 = |at: usize| {
        Some(u32::from(u16::from_le_bytes([
            *bytes.get(at)?,
            *bytes.get(at + 1)?,
        ])))
    };
    let be32 = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let le32 = |at: usize| Some(i32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let size = match media_type {
        "image/png" => (be32(16)?, be32(20)?),
        "image/gif" => (le16(6)?, le16(8)?),
        "image/bmp" => (le32(18)?.unsigned_abs(), le32(22)?.unsigned_abs()),
        "image/jpeg" => {
            let mut at = 2usize;
            loop {
                if *bytes.get(at)? != 0xFF {
                    return None;
                }
                let marker = *bytes.get(at + 1)?;
                let length = be16(at + 2)? as usize;
                // SOF0..SOF15, except DHT (C4), JPG (C8) and DAC (CC).
                if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
                    break (be16(at + 7)?, be16(at + 5)?);
                }
                at = at.checked_add(2 + length)?;
            }
        }
        _ => return None,
    };
    (size.0 > 0 && size.1 > 0).then_some(size)
}

/// EMU for `pixels` at 96 DPI (9525 EMU per pixel). O(1).
#[must_use]
pub fn pixels_to_emu(pixels: u32) -> i64 {
    i64::from(pixels) * 9_525
}

/// EMU for `points`, if it is a positive, finite size inside the model's
/// bound. O(1).
#[must_use]
pub fn points_to_emu(points: f64) -> Option<i64> {
    let emu = (points * EMU_PER_POINT as f64).round();
    #[allow(clippy::cast_possible_truncation)] // bounded just below
    (emu.is_finite() && emu >= 1.0 && emu <= MAX_EMU as f64).then_some(emu as i64)
}

/// The anchor of the floating object `object` in `blocks` (paragraphs, table
/// cells, content controls and text-box bodies, and inline wrappers),
/// if it is one. O(blocks).
#[must_use]
pub fn anchor_in_blocks(blocks: &[BlockNode], object: NodeId) -> Option<DrawingAnchor> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if let Some(anchor) = anchor_in_inlines(&paragraph.inlines, object) {
                    return Some(anchor);
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        if let Some(anchor) = anchor_in_blocks(&cell.blocks, object) {
                            return Some(anchor);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(anchor) = anchor_in_blocks(&sdt.blocks, object) {
                    return Some(anchor);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

fn anchor_in_inlines(inlines: &[InlineNode], object: NodeId) -> Option<DrawingAnchor> {
    for inline in inlines {
        match inline {
            InlineNode::AnchoredDrawing(drawing) if drawing.id == object => {
                return Some(drawing.anchor.clone());
            }
            InlineNode::TextBox(text_box) => {
                if text_box.id == object {
                    return text_box.anchor.clone();
                }
                if let Some(anchor) = anchor_in_blocks(&text_box.blocks, object) {
                    return Some(anchor);
                }
            }
            InlineNode::Group(group) if group.id == object => return group.anchor.clone(),
            // Every inline wrapper in the declared container set (`Hyperlink`,
            // `Field`, `Revision`, `Sdt`): a float inside a field result or an
            // inline content control is still a float (`109` HF-166, HF-214).
            other => {
                if let Some(nested) = contained_inlines(other)
                    && let Some(anchor) = anchor_in_inlines(nested, object)
                {
                    return Some(anchor);
                }
            }
        }
    }
    None
}

/// The anchor of the floating object `object` on any surface. O(document).
#[must_use]
pub fn anchor_of(document: &Document, object: NodeId) -> Option<DrawingAnchor> {
    surface_block_lists(document)
        .into_iter()
        .find_map(|blocks| anchor_in_blocks(blocks, object))
}

/// The wrap tokens, in menu order.
pub const WRAPS: [&str; 6] = [
    "square",
    "tight",
    "through",
    "topAndBottom",
    "behind",
    "front",
];

/// `anchor` with its wrap set to `mode` (one of [`WRAPS`]; `behind` and
/// `front` are wrap-none behind or over the text). O(1).
///
/// # Errors
/// An unknown mode.
pub fn wrapped(mut anchor: DrawingAnchor, mode: &str) -> Result<DrawingAnchor, String> {
    match mode {
        "square" => anchor.wrap = WrapMode::Square,
        "tight" => anchor.wrap = WrapMode::Tight,
        "through" => anchor.wrap = WrapMode::Through,
        "topAndBottom" | "top_and_bottom" => anchor.wrap = WrapMode::TopAndBottom,
        "behind" => {
            anchor.wrap = WrapMode::None;
            anchor.behind_doc = true;
        }
        "front" => {
            anchor.wrap = WrapMode::None;
            anchor.behind_doc = false;
        }
        _ => return Err(format!("unknown wrap mode: {mode}")),
    }
    Ok(anchor)
}

/// Sets a floating object's text wrap. One `SetAnchor`. O(document) to find
/// the anchor.
///
/// # Errors
/// [`NOT_FLOATING_WRAP`] when `object` is not floating, or an unknown mode.
pub fn set_wrap(document: &Document, object: NodeId, mode: &str) -> Result<Vec<Operation>, String> {
    let anchor = anchor_of(document, object).ok_or_else(|| NOT_FLOATING_WRAP.to_owned())?;
    Ok(vec![Operation::SetAnchor {
        object,
        anchor: Box::new(wrapped(anchor, mode)?),
    }])
}

/// Resizes an object to `width` × `height` EMU. O(1).
///
/// # Errors
/// A size outside 1..=`MAX_EMU`.
pub fn set_extent(object: NodeId, width: i64, height: i64) -> Result<Vec<Operation>, String> {
    for (value, label) in [(width, "width"), (height, "height")] {
        if !(1..=MAX_EMU).contains(&value) {
            return Err(format!("{label} is out of bounds"));
        }
    }
    Ok(vec![Operation::SetExtent {
        object,
        extent: Some(Extent {
            width_emu: width,
            height_emu: height,
        }),
    }])
}

/// A crop edge from the fraction (0..=1) of the source it hides. O(1).
#[must_use]
#[allow(clippy::cast_possible_truncation)] // a fraction of CROP_FULL fits in i32
pub fn crop_edge(fraction: f64) -> i32 {
    (fraction * f64::from(CROP_FULL)).round() as i32
}

/// A crop from four fractions `[left, top, right, bottom]`. O(1).
#[must_use]
pub fn crop(insets: [f64; 4]) -> CropRect {
    CropRect {
        left: crop_edge(insets[0]),
        top: crop_edge(insets[1]),
        right: crop_edge(insets[2]),
        bottom: crop_edge(insets[3]),
    }
}

/// Sets or clears (`None`) a picture's crop. O(1).
#[must_use]
pub fn set_crop(object: NodeId, crop: Option<CropRect>) -> Vec<Operation> {
    vec![Operation::SetImageCrop { object, crop }]
}

/// Sets or clears (`None`) an object's alt text. O(1); the engine refuses an
/// empty or over-long text and an object that carries none.
#[must_use]
pub fn set_alt_text(object: NodeId, descr: Option<String>) -> Vec<Operation> {
    vec![Operation::SetObjectDescr { object, descr }]
}

/// Removes an object; undo restores it where it was. O(1).
#[must_use]
pub fn delete_object(object: NodeId) -> Vec<Operation> {
    vec![Operation::DeleteObject { object }]
}

/// An opaque colour from a parsed `#RRGGBB`. O(1).
#[must_use]
pub fn opaque(rgb: casual_doc_model::v1::RgbColor) -> Rgba {
    Rgba {
        r: rgb.r,
        g: rgb.g,
        b: rgb.b,
        a: 255,
    }
}

/// Sets a shape's fill to a solid colour, or clears it (`None`). O(1).
#[must_use]
pub fn set_shape_fill(shape: NodeId, color: Option<Rgba>) -> Vec<Operation> {
    vec![Operation::SetShapeFill {
        shape,
        fill: color.map(Fill::Solid),
    }]
}

/// The stroke a colour or weight change starts from: the shape's own (its
/// dash and line ends survive), else Word's black 0.75 pt hairline, with the
/// weight replaced when one is given (clamped to 0..=`MAX_EMU`). O(1).
#[must_use]
pub fn stroke_base(current: Option<ShapeStroke>, width_emu: Option<f64>) -> ShapeStroke {
    const HAIRLINE_EMU: i64 = 9_525;
    let base = current.unwrap_or(ShapeStroke {
        color: Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
        width_emu: HAIRLINE_EMU,
        dash: None,
        head_end: None,
        tail_end: None,
    });
    ShapeStroke {
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // clamped
        width_emu: width_emu.map_or(base.width_emu, |w| w.clamp(0.0, MAX_EMU as f64) as i64),
        ..base
    }
}

/// Sets a shape's (or a picture's border's) outline colour and/or weight,
/// inheriting whatever is not given from the outline it has; `None` for both
/// removes it. O(document) to read the current outline.
#[must_use]
pub fn set_shape_outline(
    document: &Document,
    shape: NodeId,
    color: Option<Rgba>,
    width_emu: Option<f64>,
) -> Vec<Operation> {
    // A picture's border is the same stroke on a different carrier.
    let current = match find_shape(document, shape) {
        Some(found) => found.stroke,
        None => casual_doc_edit::picture_border(document, shape).flatten(),
    };
    let stroke = match color {
        Some(color) => Some(ShapeStroke {
            color,
            ..stroke_base(current, width_emu)
        }),
        // A weight with no colour changes only the weight (Word's Weight
        // submenu); on an unoutlined shape it gives it one, as Word does.
        None if width_emu.is_some() => Some(stroke_base(current, width_emu)),
        None => None,
    };
    vec![Operation::SetShapeStroke { shape, stroke }]
}

/// Word's Shape Outline ▸ Dashes and ▸ Arrows (ONLYOFFICE's shape settings
/// line type and begin/end arrow styles): rewrites the dash pattern and/or the
/// two line ends of a shape's outline (or a picture's border), INHERITING the
/// colour and the weight. Each argument is `None` to keep what the outline has,
/// or `Some(value)` to set it, where `Some(None)` clears it (a solid line, no
/// arrowhead). An unoutlined shape is given Word's black 0.75 pt line first, so
/// choosing a dash is never a no-op (`docs/109` RM-30).
///
/// O(document) to read the current outline.
#[must_use]
pub fn set_shape_outline_style(
    document: &Document,
    shape: NodeId,
    dash: Option<Option<DashStyle>>,
    head_end: Option<Option<LineEnd>>,
    tail_end: Option<Option<LineEnd>>,
) -> Vec<Operation> {
    let current = match find_shape(document, shape) {
        Some(found) => found.stroke,
        None => casual_doc_edit::picture_border(document, shape).flatten(),
    };
    let base = stroke_base(current, None);
    let stroke = ShapeStroke {
        dash: dash.unwrap_or(base.dash),
        head_end: head_end.unwrap_or(base.head_end),
        tail_end: tail_end.unwrap_or(base.tail_end),
        ..base
    };
    vec![Operation::SetShapeStroke {
        shape,
        stroke: Some(stroke),
    }]
}

#[cfg(test)]
mod tests {
    use super::{bytes_are, decode_base64, pixel_size};

    const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";

    #[test]
    fn base64_decodes_strictly_and_within_its_ceiling() {
        assert_eq!(decode_base64("aGVsbG8=", 100).as_deref(), Ok(&b"hello"[..]));
        assert_eq!(decode_base64("aGVsbG8", 100).as_deref(), Ok(&b"hello"[..]));
        assert_eq!(
            decode_base64("aGVs\nbG8=", 100).as_deref(),
            Ok(&b"hello"[..])
        );
        assert!(decode_base64("aGVsbG8*", 100).is_err(), "not the alphabet");
        assert!(decode_base64("aGVsbG8==", 100).is_err(), "wrong padding");
        assert!(decode_base64("a", 100).is_err(), "truncated");
        assert!(
            decode_base64("aGVsbG8=", 4).is_err(),
            "refused past the ceiling"
        );
    }

    #[test]
    fn a_picture_is_the_format_it_claims_and_states_its_size() {
        let png = decode_base64(PNG_1X1, 1024).expect("base64");
        assert!(bytes_are("image/png", &png));
        assert!(!bytes_are("image/jpeg", &png), "a PNG is not a JPEG");
        assert_eq!(pixel_size("image/png", &png), Some((1, 1)));
        let gif = b"GIF89a\x03\x00\x02\x00";
        assert_eq!(pixel_size("image/gif", gif), Some((3, 2)));
        // A JPEG's size is in its first start-of-frame marker, after an APP0.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00,
            0x20, 0x00, 0x40, 0x01, 0x01, 0x11, 0x00,
        ];
        assert_eq!(pixel_size("image/jpeg", &jpeg), Some((64, 32)));
        assert_eq!(pixel_size("image/webp", b"RIFF\0\0\0\0WEBP"), None);
        assert_eq!(pixel_size("image/jpeg", &[0xFF, 0xD8]), None, "truncated");
    }
}
