//! Part-scoped picture-bullet resource resolution. The bounded streaming pass
//! is O(XML elements), with no image decoding and no document scans.

use std::collections::BTreeMap;

use casual_doc_model::v1::{MediaId, PictureBullet};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::attribute_value;
use crate::report::Reporter;

#[derive(Default)]
struct RawPicture {
    id: String,
    relationship: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    unsupported: bool,
}

/// Resolves VML shape/image and DrawingML extent/blip marker forms. Unsupported
/// crops, transforms, external resources and malformed dimensions are reported
/// as a single picture-bullet loss; the level retains its textual fallback.
pub(crate) fn parse(
    xml: &[u8],
    media: &BTreeMap<String, MediaId>,
    config: ImportConfig,
    reporter: &mut Reporter,
) -> Result<BTreeMap<String, PictureBullet>, ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut depth = 0_u64;
    let mut elements = 0_u64;
    let mut picture: Option<RawPicture> = None;
    let mut result = BTreeMap::new();
    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| ImportError::MalformedXml)?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(ImportError::MalformedXml),
            Event::Start(element) | Event::Empty(element) => {
                elements += 1;
                if elements > config.max_elements {
                    return Err(ImportError::LimitExceeded {
                        limit: "xml_elements",
                    });
                }
                let local = element.local_name();
                if local.as_ref() == b"numPicBullet" {
                    if picture.is_some() {
                        reporter.report(b"numPicBullet");
                    }
                    picture = Some(RawPicture {
                        id: attribute_value(&element, b"numPicBulletId").unwrap_or_default(),
                        ..RawPicture::default()
                    });
                } else if let Some(raw) = &mut picture {
                    read_element(raw, local.as_ref(), &element);
                }
                if empty {
                    if local.as_ref() == b"numPicBullet" {
                        picture = None;
                        reporter.report(b"numPicBullet");
                    }
                } else {
                    depth += 1;
                    if depth > config.max_depth {
                        return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                    }
                }
            }
            Event::End(element) => {
                depth = depth.saturating_sub(1);
                if element.local_name().as_ref() == b"numPicBullet"
                    && let Some(raw) = picture.take()
                {
                    let value = raw
                        .relationship
                        .as_ref()
                        .and_then(|id| media.get(id))
                        .copied()
                        .zip(raw.width)
                        .zip(raw.height)
                        .map(|((media, width), height)| PictureBullet {
                            media,
                            width,
                            height,
                        });
                    if raw.unsupported
                        || raw.id.is_empty()
                        || value.is_none()
                        || result.contains_key(&raw.id)
                    {
                        reporter.report(b"numPicBullet");
                    } else if let Some(value) = value {
                        result.insert(raw.id, value);
                    }
                }
            }
            _ => {}
        }
        buffer.clear();
    }
    if picture.is_some() {
        return Err(ImportError::MalformedXml);
    }
    Ok(result)
}

fn read_element(raw: &mut RawPicture, local: &[u8], element: &BytesStart<'_>) {
    match local {
        b"pict" | b"drawing" | b"inline" | b"anchor" | b"graphic" | b"graphicData" | b"pic"
        | b"nvPicPr" | b"cNvPr" | b"cNvPicPr" | b"blipFill" | b"spPr" | b"stretch"
        | b"fillRect" | b"prstGeom" | b"avLst" | b"shapetype" | b"path" | b"stroke"
        | b"formulas" | b"f" | b"handles" | b"h" | b"lock" | b"docPr" | b"cNvGraphicFramePr"
        | b"graphicFrameLocks" => {}
        b"shape" => {
            let style = attribute_value(element, b"style").unwrap_or_default();
            for item in style.split(';') {
                let Some((name, value)) = item.split_once(':') else {
                    continue;
                };
                match name.trim() {
                    "width" => raw.width = vml_twips(value),
                    "height" => raw.height = vml_twips(value),
                    "rotation" | "flip" => raw.unsupported = true,
                    _ => {}
                }
            }
        }
        b"imagedata" => {
            raw.relationship = attribute_value(element, b"id");
            for crop in [
                b"croptop".as_slice(),
                b"cropbottom",
                b"cropleft",
                b"cropright",
                b"gain",
                b"blacklevel",
                b"gamma",
                b"grayscale",
                b"bilevel",
                b"chromakey",
                b"emboss",
            ] {
                if attribute_value(element, crop).is_some() {
                    raw.unsupported = true;
                }
            }
        }
        b"extent" => {
            raw.width = attribute_value(element, b"cx").and_then(|v| emu_twips(&v));
            raw.height = attribute_value(element, b"cy").and_then(|v| emu_twips(&v));
        }
        b"blip" => {
            raw.relationship = attribute_value(element, b"embed");
            if attribute_value(element, b"link").is_some() {
                raw.unsupported = true;
            }
        }
        // A transform may size the image too, but rotation/flip are genuinely
        // unsupported marker effects, not harmless markup.
        b"xfrm" => {
            if [b"rot".as_slice(), b"flipH", b"flipV"]
                .iter()
                .any(|name| attribute_value(element, name).is_some())
            {
                raw.unsupported = true;
            }
        }
        b"off" | b"ext" => {}
        _ => raw.unsupported = true,
    }
}

fn emu_twips(value: &str) -> Option<i32> {
    let emu: i64 = value.parse().ok()?;
    let twips = i32::try_from(emu.checked_add(317)? / 635).ok()?;
    (1..=31_680).contains(&twips).then_some(twips)
}

fn vml_twips(value: &str) -> Option<i32> {
    let value = value.trim();
    let (number, scale) = if let Some(v) = value.strip_suffix("pt") {
        (v, 20.0)
    } else if let Some(v) = value.strip_suffix("in") {
        (v, 1440.0)
    } else if let Some(v) = value.strip_suffix("cm") {
        (v, 1440.0 / 2.54)
    } else if let Some(v) = value.strip_suffix("mm") {
        (v, 1440.0 / 25.4)
    } else if let Some(v) = value.strip_suffix("px") {
        (v, 15.0)
    } else {
        return None;
    };
    let twips = (number.trim().parse::<f64>().ok()? * scale).round();
    (twips.is_finite() && (1.0..=31_680.0).contains(&twips)).then_some(twips as i32)
}
