//! Bounded import of the first ODT page-layout geometry into schema-v1.

use casual_doc_model::v1::{PageMargins, PageOrientation, PageSize, TextDirection};
use quick_xml::Reader;
use quick_xml::events::Event;

/// The geometry found in one `style:page-layout-properties` element.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PageLayoutGeometry {
    pub size: PageSize,
    pub margins: PageMargins,
    pub orientation: Option<PageOrientation>,
    pub columns: u16,
    pub column_gap_twips: Option<i32>,
    pub column_separator: Option<bool>,
    pub text_direction: Option<TextDirection>,
}

pub(crate) fn parse_page_layout(bytes: &[u8]) -> Option<PageLayoutGeometry> {
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf).ok()? {
            Event::Start(start) | Event::Empty(start) => {
                let local = start.name();
                let local = local.as_ref().split(|byte| *byte == b':').next_back()?;
                if local != b"page-layout-properties" {
                    buf.clear();
                    continue;
                }
                let mut width = None;
                let mut height = None;
                let mut margins = [None; 4];
                let mut orientation = None;
                let mut columns = 1u16;
                let mut column_gap_twips = None;
                let mut column_separator = None;
                let mut text_direction = None;
                for attr in start.attributes().flatten() {
                    let key = attr.key.as_ref().split(|byte| *byte == b':').next_back()?;
                    let value = String::from_utf8_lossy(attr.value.as_ref());
                    match key {
                        b"page-width" => width = parse_twips(&value),
                        b"page-height" => height = parse_twips(&value),
                        b"margin-top" => margins[0] = parse_twips(&value),
                        b"margin-bottom" => margins[1] = parse_twips(&value),
                        b"margin-left" => margins[2] = parse_twips(&value),
                        b"margin-right" => margins[3] = parse_twips(&value),
                        b"print-orientation" => {
                            orientation = match value.as_ref() {
                                "landscape" => Some(PageOrientation::Landscape),
                                "portrait" => Some(PageOrientation::Portrait),
                                _ => None,
                            }
                        }
                        b"column-count" => {
                            columns = value
                                .parse()
                                .ok()
                                .filter(|value: &u16| *value > 0)
                                .unwrap_or(1)
                        }
                        b"column-gap" => column_gap_twips = parse_twips(&value),
                        b"column-sep" => column_separator = Some(value == "true" || value == "1"),
                        b"writing-mode" => {
                            text_direction = match value.as_ref() {
                                "lr-tb" => Some(TextDirection::LrTb),
                                "tb-rl" => Some(TextDirection::TbRl),
                                "bt-lr" => Some(TextDirection::BtLr),
                                _ => None,
                            }
                        }
                        _ => {}
                    }
                }
                return Some(PageLayoutGeometry {
                    size: PageSize {
                        width_twips: width.unwrap_or(12_240),
                        height_twips: height.unwrap_or(15_840),
                    },
                    margins: PageMargins {
                        top_twips: margins[0].unwrap_or(1_440),
                        bottom_twips: margins[1].unwrap_or(1_440),
                        start_twips: margins[2].unwrap_or(1_440),
                        end_twips: margins[3].unwrap_or(1_440),
                        header_twips: None,
                        footer_twips: None,
                        gutter_twips: None,
                    },
                    orientation,
                    columns,
                    column_gap_twips,
                    column_separator,
                    text_direction,
                });
            }
            Event::Eof => return None,
            _ => {}
        }
        buf.clear();
    }
}

fn parse_twips(value: &str) -> Option<i32> {
    let value = value.trim();
    let split = value
        .find(|character: char| character.is_ascii_alphabetic())
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split);
    let number = number.parse::<f64>().ok()?;
    let twips = match unit {
        "cm" => number * 1440.0 / 2.54,
        "mm" => number * 1440.0 / 25.4,
        "in" => number * 1440.0,
        "pt" => number * 20.0,
        _ => return None,
    };
    if !(0.0..=2_000_000.0).contains(&twips) {
        return None;
    }
    Some(twips.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_page_geometry_and_units() {
        let xml = br#"<style:page-layout-properties fo:page-width="21cm" fo:page-height="29.7cm" fo:margin-top="2cm" fo:margin-bottom="2cm" fo:margin-left="1.5cm" fo:margin-right="1.5cm" style:print-orientation="portrait"/>"#;
        let geometry = parse_page_layout(xml).unwrap();
        assert_eq!(geometry.size.width_twips, 11_906);
        assert_eq!(geometry.margins.top_twips, 1_134);
        assert_eq!(geometry.orientation, Some(PageOrientation::Portrait));
    }
}
