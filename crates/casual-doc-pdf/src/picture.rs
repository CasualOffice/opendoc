//! Embedded pictures as PDF image XObjects.
//!
//! An image is embedded **once** and referenced from every page that places it,
//! which is the structural difference from the raster print path this replaces:
//! there, a picture was flattened into each page bitmap at the print
//! resolution, so the same logo cost eight megabytes a page.
//!
//! JPEG data is passed straight through as `DCTDecode` — the stored bytes are
//! the document's own bytes, with no decode and no re-encode, so a photographic
//! page costs what it cost in the source file. Everything else is decoded once
//! and stored as `FlateDecode`d 8-bit samples, with any alpha channel split off
//! into an `/SMask`, because PDF has no packed-alpha image form.

use std::collections::BTreeMap;
use std::io::Cursor;

use crate::writer::Ref;
use crate::writer::Writer;
use crate::writer::name as pdf_name;

/// Bounds on a decoded picture, matching `casual-doc-render`'s so the two
/// backends admit exactly the same set of images (`21-PARSER-LIMITS`).
const MAX_DECODED_IMAGE_DIMENSION: u32 = 32_768;
/// The most pixels any single picture may decode to.
const MAX_DECODED_IMAGE_PIXELS: u64 = 100_000_000;
/// The most bytes the decoder may allocate for one picture.
const MAX_DECODED_IMAGE_BYTES: u64 = MAX_DECODED_IMAGE_PIXELS * 4;

/// Supplies the encoded bytes of an embedded picture.
///
/// The key is the display list's media reference, which is the package part
/// name the importer recorded — the same key `casual-doc-render`'s
/// `MediaSource` is asked for.
pub trait PdfMediaSource {
    /// The encoded bytes for `media`, or `None` when the host cannot serve it.
    fn media_bytes(&self, media: &str) -> Option<&[u8]>;
}

/// A media source that serves nothing; every picture then reports as missing.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoMediaSource;

impl PdfMediaSource for NoMediaSource {
    fn media_bytes(&self, _media: &str) -> Option<&[u8]> {
        None
    }
}

/// A media source over an in-memory map from media key to encoded bytes.
#[derive(Clone, Debug, Default)]
pub struct MapMediaSource {
    parts: BTreeMap<String, Vec<u8>>,
}

impl MapMediaSource {
    /// An empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces one part's bytes.
    pub fn insert(&mut self, media: impl Into<String>, bytes: Vec<u8>) {
        self.parts.insert(media.into(), bytes);
    }
}

impl PdfMediaSource for MapMediaSource {
    fn media_bytes(&self, media: &str) -> Option<&[u8]> {
        self.parts.get(media).map(Vec::as_slice)
    }
}

/// One picture prepared for writing.
struct PlannedImage {
    resource: String,
    object: Ref,
    payload: Payload,
}

/// How a picture's samples are stored.
enum Payload {
    /// JPEG bytes passed through verbatim as `DCTDecode`.
    Jpeg {
        bytes: Vec<u8>,
        width: u32,
        height: u32,
        components: u8,
    },
    /// Decoded 8-bit RGB samples, with an optional 8-bit alpha plane.
    Raw {
        rgb: Vec<u8>,
        alpha: Option<Vec<u8>>,
        width: u32,
        height: u32,
    },
}

/// Collects the pictures a document places and writes them into the file.
#[derive(Default)]
pub(crate) struct ImageTable {
    planned: BTreeMap<String, PlannedImage>,
    next: usize,
    /// Media keys the source could not serve or the decoder refused, in first
    /// -seen order, so the caller can report them.
    unresolved: Vec<String>,
}

impl ImageTable {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Records a placement of `media`, decoding and reserving its object the
    /// first time it is seen. Returns the resource name, or `None` when the
    /// picture cannot be embedded — in which case it is recorded as
    /// unresolved and the caller reports it rather than drawing something
    /// wrong.
    pub(crate) fn use_image(
        &mut self,
        writer: &mut Writer,
        media: &dyn PdfMediaSource,
        key: &str,
    ) -> Option<String> {
        if let Some(planned) = self.planned.get(key) {
            return Some(planned.resource.clone());
        }
        if self.unresolved.iter().any(|seen| seen == key) {
            return None;
        }
        let Some(payload) = media.media_bytes(key).and_then(prepare) else {
            self.unresolved.push(key.to_owned());
            return None;
        };
        self.next += 1;
        let resource = format!("Im{}", self.next);
        self.planned.insert(
            key.to_owned(),
            PlannedImage {
                resource: resource.clone(),
                object: writer.reserve(),
                payload,
            },
        );
        Some(resource)
    }

    /// Media keys that could not be embedded.
    pub(crate) fn unresolved(&self) -> &[String] {
        &self.unresolved
    }

    /// Whether any picture was placed.
    pub(crate) fn is_empty(&self) -> bool {
        self.planned.is_empty()
    }

    /// The `/XObject` sub-dictionary entries for every planned picture.
    pub(crate) fn resource_dictionary(&self) -> String {
        let mut entries = String::new();
        for planned in self.planned.values() {
            entries.push_str(&pdf_name(&planned.resource));
            entries.push(' ');
            entries.push_str(&planned.object.reference());
        }
        entries
    }

    /// Writes every planned picture, returning the total stored bytes.
    pub(crate) fn write(self, writer: &mut Writer) -> usize {
        let mut total = 0;
        for planned in self.planned.into_values() {
            total += match planned.payload {
                Payload::Jpeg {
                    bytes,
                    width,
                    height,
                    components,
                } => {
                    let space = match components {
                        1 => "/DeviceGray",
                        4 => "/DeviceCMYK",
                        _ => "/DeviceRGB",
                    };
                    writer.preencoded_stream(
                        planned.object,
                        &format!(
                            "/Type/XObject/Subtype/Image/Width {width}/Height {height}/ColorSpace{space}/BitsPerComponent 8"
                        ),
                        "DCTDecode",
                        &bytes,
                    )
                }
                Payload::Raw {
                    rgb,
                    alpha,
                    width,
                    height,
                } => {
                    let mask = alpha.map(|alpha| {
                        let object = writer.reserve();
                        writer.stream(
                            object,
                            &format!(
                                "/Type/XObject/Subtype/Image/Width {width}/Height {height}/ColorSpace/DeviceGray/BitsPerComponent 8"
                            ),
                            &alpha,
                            true,
                        );
                        object
                    });
                    let mask_entry =
                        mask.map_or(String::new(), |id| format!("/SMask {}", id.reference()));
                    writer.stream(
                        planned.object,
                        &format!(
                            "/Type/XObject/Subtype/Image/Width {width}/Height {height}/ColorSpace/DeviceRGB/BitsPerComponent 8{mask_entry}"
                        ),
                        &rgb,
                        true,
                    )
                }
            };
        }
        total
    }
}

/// Classifies and prepares one picture's bytes, or returns `None` when it is
/// unreadable or over the decode limits.
fn prepare(bytes: &[u8]) -> Option<Payload> {
    if let Some((width, height, components)) = jpeg_geometry(bytes)
        && within_limits(width, height)
        && matches!(components, 1 | 3)
    {
        return Some(Payload::Jpeg {
            bytes: bytes.to_vec(),
            width,
            height,
            components,
        });
    }
    decode(bytes)
}

/// Decodes any format the codec set covers into 8-bit RGB plus optional alpha.
fn decode(bytes: &[u8]) -> Option<Payload> {
    let format = image::guess_format(bytes).ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODED_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_DECODED_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODED_IMAGE_BYTES);
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let (width, height) = (decoded.width(), decoded.height());
    if !within_limits(width, height) {
        return None;
    }
    let rgba = decoded.into_rgba8();
    let pixels = usize::try_from(u64::from(width) * u64::from(height)).ok()?;
    let mut rgb = Vec::with_capacity(pixels * 3);
    let mut alpha = Vec::with_capacity(pixels);
    let mut opaque = true;
    for pixel in rgba.pixels() {
        rgb.extend_from_slice(&pixel.0[..3]);
        alpha.push(pixel.0[3]);
        opaque &= pixel.0[3] == 255;
    }
    Some(Payload::Raw {
        rgb,
        alpha: (!opaque).then_some(alpha),
        width,
        height,
    })
}

fn within_limits(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= MAX_DECODED_IMAGE_DIMENSION
        && height <= MAX_DECODED_IMAGE_DIMENSION
        && u64::from(width)
            .checked_mul(u64::from(height))
            .is_some_and(|pixels| pixels <= MAX_DECODED_IMAGE_PIXELS)
}

/// Reads a baseline or progressive JPEG's frame header for its geometry, so the
/// bytes can be stored without decoding them.
///
/// Returns `None` for anything that is not a JPEG this writer will pass
/// through, including arithmetic-coded and hierarchical frames, which
/// `DCTDecode` does not cover.
fn jpeg_geometry(bytes: &[u8]) -> Option<(u32, u32, u8)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut cursor = 2_usize;
    while cursor + 4 <= bytes.len() {
        if bytes[cursor] != 0xFF {
            cursor += 1;
            continue;
        }
        let marker = bytes[cursor + 1];
        // Padding and the standalone markers carry no length field.
        if marker == 0xFF {
            cursor += 1;
            continue;
        }
        if matches!(marker, 0x01 | 0xD0..=0xD9) {
            cursor += 2;
            continue;
        }
        let length = usize::from(u16::from_be_bytes([bytes[cursor + 2], bytes[cursor + 3]]));
        if length < 2 {
            return None;
        }
        // SOF0/1/2/9/10 are the frame headers `DCTDecode` reads; SOF3/5-7/11-15
        // are lossless, differential or arithmetic and are not passed through.
        if matches!(marker, 0xC0 | 0xC1 | 0xC2 | 0xC9 | 0xCA) {
            let frame = bytes.get(cursor + 4..cursor + 4 + length - 2)?;
            if frame.len() < 6 {
                return None;
            }
            let height = u32::from(u16::from_be_bytes([frame[1], frame[2]]));
            let width = u32::from(u16::from_be_bytes([frame[3], frame[4]]));
            return Some((width, height, frame[5]));
        }
        if marker == 0xDA {
            // Start of scan: no frame header was found before the entropy data.
            return None;
        }
        cursor = cursor.checked_add(2)?.checked_add(length)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x2 opaque red PNG.
    fn png() -> Vec<u8> {
        let mut buffer = Vec::new();
        let image = image::RgbaImage::from_fn(2, 2, |_, _| image::Rgba([255, 0, 0, 255]));
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Png)
            .expect("encode test png");
        buffer
    }

    /// A 3x2 JPEG.
    fn jpeg() -> Vec<u8> {
        let mut buffer = Vec::new();
        let image = image::RgbImage::from_fn(3, 2, |x, _| image::Rgb([x as u8 * 40, 10, 20]));
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Jpeg)
            .expect("encode test jpeg");
        buffer
    }

    #[test]
    fn a_jpeg_is_stored_without_being_re_encoded() {
        let bytes = jpeg();
        assert_eq!(jpeg_geometry(&bytes), Some((3, 2, 3)));
        let Some(Payload::Jpeg { bytes: stored, .. }) = prepare(&bytes) else {
            panic!("a JPEG must take the passthrough path");
        };
        assert_eq!(stored, bytes, "the source bytes are stored verbatim");
    }

    #[test]
    fn a_png_with_alpha_is_split_into_samples_and_a_soft_mask() {
        let mut buffer = Vec::new();
        let image = image::RgbaImage::from_fn(2, 1, |x, _| image::Rgba([1, 2, 3, x as u8 * 255]));
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Png)
            .expect("encode test png");
        let Some(Payload::Raw { rgb, alpha, .. }) = prepare(&buffer) else {
            panic!("a PNG decodes to raw samples");
        };
        assert_eq!(rgb, vec![1, 2, 3, 1, 2, 3]);
        assert_eq!(alpha, Some(vec![0, 255]));
    }

    #[test]
    fn an_opaque_picture_carries_no_soft_mask() {
        let Some(Payload::Raw { alpha, .. }) = prepare(&png()) else {
            panic!("a PNG decodes to raw samples");
        };
        assert_eq!(alpha, None);
    }

    #[test]
    fn unreadable_bytes_are_refused_rather_than_guessed_at() {
        assert!(prepare(b"not an image at all").is_none());
        assert!(prepare(&[]).is_none());
        // A JPEG header with no frame must not be passed through.
        assert!(jpeg_geometry(&[0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02]).is_none());
    }

    #[test]
    fn the_same_picture_is_embedded_once_however_often_it_is_placed() {
        let mut writer = Writer::new();
        let mut table = ImageTable::new();
        let mut media = MapMediaSource::new();
        media.insert("word/media/image1.png", png());
        let first = table.use_image(&mut writer, &media, "word/media/image1.png");
        let second = table.use_image(&mut writer, &media, "word/media/image1.png");
        assert_eq!(first, second);
        assert_eq!(table.planned.len(), 1);
    }

    #[test]
    fn a_missing_picture_is_reported_not_silently_skipped() {
        let mut writer = Writer::new();
        let mut table = ImageTable::new();
        let media = NoMediaSource;
        assert_eq!(
            table.use_image(&mut writer, &media, "word/media/gone.png"),
            None
        );
        assert_eq!(table.unresolved(), ["word/media/gone.png"]);
    }
}
