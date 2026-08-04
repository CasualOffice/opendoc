//! Bounded semantic projection of retained Office Math Markup Language.

use casual_doc_model::v1::{
    BarPosition, GroupPosition, LimitPosition, MAX_MATH_BYTES, MAX_MATH_DEPTH, MAX_MATH_NODES,
    MathExpression, MathMatrixRow,
};
// Separate `use` line (kept out of the sorted block above) to avoid import-list
// merge collisions with other agents editing this file.
use casual_doc_model::v1::MathBorderBox;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

#[derive(Debug)]
struct Element {
    name: Vec<u8>,
    attributes: Vec<(Vec<u8>, String)>,
    children: Vec<Element>,
    text: String,
}

/// Derives the supported semantic subset from a retained OMML fragment.
///
/// `None` is a safe and expected result for unsupported, malformed, or
/// over-limit input. The caller retains the original fragment independently.
pub(crate) fn parse_math_expression(omml: &str) -> Option<MathExpression> {
    if omml.is_empty() || omml.len() > MAX_MATH_BYTES {
        return None;
    }
    let root = parse_tree(omml)?;
    let expression = expression_from_root(&root)?;
    let mut nodes = 0;
    if expression_within_bounds(&expression, 1, &mut nodes) {
        Some(expression)
    } else {
        None
    }
}

fn parse_tree(xml: &str) -> Option<Element> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = true;
    let mut stack = Vec::new();
    let mut root = None;
    let mut element_count = 0usize;

    loop {
        match reader.read_event().ok()? {
            Event::Start(start) => {
                element_count = element_count.checked_add(1)?;
                if element_count > MAX_MATH_NODES.saturating_mul(8)
                    || stack.len() >= MAX_MATH_DEPTH.saturating_mul(4)
                {
                    return None;
                }
                stack.push(element(&reader, &start)?);
            }
            Event::Empty(start) => {
                element_count = element_count.checked_add(1)?;
                if element_count > MAX_MATH_NODES.saturating_mul(8) {
                    return None;
                }
                append_element(&mut stack, &mut root, element(&reader, &start)?)?;
            }
            Event::End(_) => {
                let closed = stack.pop()?;
                append_element(&mut stack, &mut root, closed)?;
            }
            Event::Text(text) => {
                let decoded = text.decode().ok()?;
                let decoded = quick_xml::escape::unescape(&decoded).ok()?;
                let current = stack.last_mut()?;
                if current.text.len().saturating_add(decoded.len()) > MAX_MATH_BYTES {
                    return None;
                }
                current.text.push_str(&decoded);
            }
            Event::GeneralRef(reference) => {
                let name = reference.decode().ok()?;
                let encoded = format!("&{name};");
                let decoded = quick_xml::escape::unescape(&encoded).ok()?;
                let current = stack.last_mut()?;
                if current.text.len().saturating_add(decoded.len()) > MAX_MATH_BYTES {
                    return None;
                }
                current.text.push_str(&decoded);
            }
            Event::Eof => break,
            Event::Decl(_) | Event::Comment(_) | Event::CData(_) | Event::PI(_) => {}
            Event::DocType(_) => return None,
        }
    }
    if stack.is_empty() { root } else { None }
}

fn element(reader: &Reader<&[u8]>, start: &BytesStart<'_>) -> Option<Element> {
    let name = omml_local_name(start.name().as_ref())?.to_vec();
    let mut attributes = Vec::new();
    for attribute in start.attributes().with_checks(true) {
        let attribute = attribute.ok()?;
        let raw_name = attribute.key.as_ref();
        if raw_name == b"xmlns" || raw_name.starts_with(b"xmlns:") {
            continue;
        }
        let local = raw_name.rsplit(|byte| *byte == b':').next()?.to_vec();
        let value = attribute
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
            .ok()?
            .into_owned();
        attributes.push((local, value));
    }
    Some(Element {
        name,
        attributes,
        children: Vec::new(),
        text: String::new(),
    })
}

fn append_element(
    stack: &mut [Element],
    root: &mut Option<Element>,
    element: Element,
) -> Option<()> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(element);
    } else if root.is_none() {
        *root = Some(element);
    } else {
        return None;
    }
    Some(())
}

/// Only the conventional `m:` qualified name is admitted by this projection.
/// The outer document capture recognizes the OMML root; requiring the math
/// prefix here prevents WordprocessingML lookalikes from becoming semantics.
fn omml_local_name(name: &[u8]) -> Option<&[u8]> {
    name.strip_prefix(b"m:").filter(|local| !local.is_empty())
}

fn expression_from_root(root: &Element) -> Option<MathExpression> {
    match root.name.as_slice() {
        b"oMath" => row_from_children(&root.children),
        b"oMathPara" => {
            let maths = root
                .children
                .iter()
                .filter(|child| child.name.as_slice() == b"oMath")
                .map(expression_from_root)
                .collect::<Option<Vec<_>>>()?;
            row(maths)
        }
        _ => None,
    }
}

fn convert(element: &Element) -> Option<Option<MathExpression>> {
    let expression = match element.name.as_slice() {
        b"r" => {
            let value = element
                .children
                .iter()
                .filter(|child| child.name.as_slice() == b"t")
                .map(|child| child.text.as_str())
                .collect::<String>();
            if value.is_empty() {
                return Some(None);
            }
            MathExpression::Text { value }
        }
        b"f" => MathExpression::Fraction {
            numerator: Box::new(wrapper_expression(element, b"num")?),
            denominator: Box::new(wrapper_expression(element, b"den")?),
        },
        b"sSub" => MathExpression::Script {
            base: Box::new(wrapper_expression(element, b"e")?),
            subscript: Some(Box::new(wrapper_expression(element, b"sub")?)),
            superscript: None,
        },
        b"sSup" => MathExpression::Script {
            base: Box::new(wrapper_expression(element, b"e")?),
            subscript: None,
            superscript: Some(Box::new(wrapper_expression(element, b"sup")?)),
        },
        b"sSubSup" => MathExpression::Script {
            base: Box::new(wrapper_expression(element, b"e")?),
            subscript: Some(Box::new(wrapper_expression(element, b"sub")?)),
            superscript: Some(Box::new(wrapper_expression(element, b"sup")?)),
        },
        b"rad" => MathExpression::Radical {
            degree: wrapper_expression_optional(element, b"deg").map(Box::new),
            radicand: Box::new(wrapper_expression(element, b"e")?),
        },
        b"d" => {
            let properties = child(element, b"dPr");
            let open = properties
                .and_then(|properties| child(properties, b"begChr"))
                .and_then(|character| attribute(character, b"val"))
                .unwrap_or("(")
                .to_owned();
            let close = properties
                .and_then(|properties| child(properties, b"endChr"))
                .and_then(|character| attribute(character, b"val"))
                .unwrap_or(")")
                .to_owned();
            MathExpression::Delimiter {
                open,
                close,
                content: Box::new(wrapper_expression(element, b"e")?),
            }
        }
        b"func" => MathExpression::Function {
            name: Box::new(wrapper_expression(element, b"fName")?),
            argument: Box::new(wrapper_expression(element, b"e")?),
        },
        b"acc" => MathExpression::Accent {
            accent: property_character(element, b"accPr", "\u{0302}"),
            base: Box::new(wrapper_expression(element, b"e")?),
        },
        b"limLow" => MathExpression::Limit {
            base: Box::new(wrapper_expression(element, b"e")?),
            limit: Box::new(wrapper_expression(element, b"lim")?),
            position: LimitPosition::Lower,
        },
        b"limUpp" => MathExpression::Limit {
            base: Box::new(wrapper_expression(element, b"e")?),
            limit: Box::new(wrapper_expression(element, b"lim")?),
            position: LimitPosition::Upper,
        },
        b"nary" => MathExpression::Nary {
            operator: property_character(element, b"naryPr", "\u{222B}"),
            lower: wrapper_expression_optional(element, b"sub").map(Box::new),
            upper: wrapper_expression_optional(element, b"sup").map(Box::new),
            base: Box::new(wrapper_expression(element, b"e")?),
        },
        b"m" => {
            let mut rows = Vec::new();
            for mr in element
                .children
                .iter()
                .filter(|c| c.name.as_slice() == b"mr")
            {
                let mut cells = Vec::new();
                for cell in mr.children.iter().filter(|c| c.name.as_slice() == b"e") {
                    cells.push(row_from_children(&cell.children)?);
                }
                if cells.is_empty() {
                    return None;
                }
                rows.push(MathMatrixRow { cells });
            }
            if rows.is_empty() {
                return None;
            }
            MathExpression::Matrix { rows }
        }
        b"eqArr" => {
            let mut rows = Vec::new();
            for row in element
                .children
                .iter()
                .filter(|c| c.name.as_slice() == b"e")
            {
                rows.push(row_from_children(&row.children)?);
            }
            if rows.is_empty() {
                return None;
            }
            MathExpression::EqArray { rows }
        }
        b"bar" => MathExpression::Bar {
            position: match position_character(element, b"barPr") {
                Some("bot") => BarPosition::Bottom,
                _ => BarPosition::Top,
            },
            base: Box::new(wrapper_expression(element, b"e")?),
        },
        b"groupChr" => MathExpression::GroupChar {
            character: property_character(element, b"groupChrPr", "\u{23DE}"),
            position: match position_character(element, b"groupChrPr") {
                Some("bot") => GroupPosition::Bottom,
                _ => GroupPosition::Top,
            },
            base: Box::new(wrapper_expression(element, b"e")?),
        },
        // Pre-scripts (`m:sPre`): the sub/sup precede the base (left scripts).
        b"sPre" => MathExpression::PreScript {
            base: Box::new(wrapper_expression(element, b"e")?),
            subscript: wrapper_expression_optional(element, b"sub").map(Box::new),
            superscript: wrapper_expression_optional(element, b"sup").map(Box::new),
        },
        // A border/strike box around a formula (`m:borderBox`).
        b"borderBox" => MathExpression::BorderBox {
            content: Box::new(wrapper_expression(element, b"e")?),
            borders: border_box_edges(element),
        },
        // A logical grouping box (`m:box`): a transparent content wrapper.
        b"box" => MathExpression::Box {
            content: Box::new(wrapper_expression(element, b"e")?),
        },
        // Property containers affect advanced typography and are intentionally
        // ignored for this first common-construct projection.
        name if name.ends_with(b"Pr") || name == b"ctrlPr" => return Some(None),
        // Wrappers are consumed by their owning construct, never independently.
        b"t" | b"e" | b"num" | b"den" | b"sub" | b"sup" | b"deg" | b"fName" | b"lim" | b"mr" => {
            return Some(None);
        }
        _ => return None,
    };
    Some(Some(expression))
}

fn row_from_children(children: &[Element]) -> Option<MathExpression> {
    let mut expressions = Vec::new();
    for child in children {
        if let Some(expression) = convert(child)? {
            expressions.push(expression);
        }
    }
    row(expressions)
}

fn row(mut expressions: Vec<MathExpression>) -> Option<MathExpression> {
    match expressions.len() {
        0 => None,
        1 => expressions.pop(),
        _ => Some(MathExpression::Row {
            children: expressions,
        }),
    }
}

fn wrapper_expression(element: &Element, name: &[u8]) -> Option<MathExpression> {
    row_from_children(&child(element, name)?.children)
}

fn wrapper_expression_optional(element: &Element, name: &[u8]) -> Option<MathExpression> {
    let wrapper = child(element, name)?;
    row_from_children(&wrapper.children)
}

/// Reads a property container's `m:chr@m:val` character, falling back to the
/// OOXML default glyph when the property (or its container) is absent.
fn property_character(element: &Element, container: &[u8], default: &str) -> String {
    child(element, container)
        .and_then(|properties| child(properties, b"chr"))
        .and_then(|character| attribute(character, b"val"))
        .unwrap_or(default)
        .to_owned()
}

/// Reads a property container's `m:pos@m:val` (`top`/`bot`), if present.
fn position_character<'a>(element: &'a Element, container: &[u8]) -> Option<&'a str> {
    child(element, container)
        .and_then(|properties| child(properties, b"pos"))
        .and_then(|position| attribute(position, b"val"))
}

/// Reads a `m:borderBox`'s `m:borderBoxPr` edge/strike flags. Each flag is a
/// `CT_OnOff`-style `m:hide*`/`m:strike*` child; absent means default (all borders
/// drawn, no strikes).
fn border_box_edges(element: &Element) -> MathBorderBox {
    let properties = child(element, b"borderBoxPr");
    let flag = |name: &[u8]| properties.is_some_and(|pr| bool_property(pr, name));
    MathBorderBox {
        hide_top: flag(b"hideTop"),
        hide_bottom: flag(b"hideBot"),
        hide_left: flag(b"hideLeft"),
        hide_right: flag(b"hideRight"),
        strike_horizontal: flag(b"strikeH"),
        strike_vertical: flag(b"strikeV"),
        strike_bltr: flag(b"strikeBLTR"),
        strike_tlbr: flag(b"strikeTLBR"),
    }
}

/// Reads a `CT_OnOff`-style boolean OMML property child: present means on unless its
/// `m:val` is a falsy token; absent means off.
fn bool_property(container: &Element, name: &[u8]) -> bool {
    match child(container, name) {
        None => false,
        Some(flag) => !matches!(attribute(flag, b"val"), Some("0" | "false" | "off")),
    }
}

fn child<'a>(element: &'a Element, name: &[u8]) -> Option<&'a Element> {
    element.children.iter().find(|child| child.name == name)
}

fn attribute<'a>(element: &'a Element, name: &[u8]) -> Option<&'a str> {
    element
        .attributes
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.as_str())
}

fn expression_within_bounds(expression: &MathExpression, depth: usize, nodes: &mut usize) -> bool {
    if depth > MAX_MATH_DEPTH {
        return false;
    }
    *nodes = nodes.saturating_add(1);
    if *nodes > MAX_MATH_NODES {
        return false;
    }
    let mut check = |child: &MathExpression| expression_within_bounds(child, depth + 1, nodes);
    match expression {
        MathExpression::Row { children } => !children.is_empty() && children.iter().all(&mut check),
        MathExpression::Text { value } => !value.is_empty() && value.len() <= MAX_MATH_BYTES,
        MathExpression::Fraction {
            numerator,
            denominator,
        } => check(numerator) && check(denominator),
        MathExpression::Script {
            base,
            subscript,
            superscript,
        } => {
            (subscript.is_some() || superscript.is_some())
                && check(base)
                && subscript.as_deref().is_none_or(&mut check)
                && superscript.as_deref().is_none_or(&mut check)
        }
        MathExpression::Radical { degree, radicand } => {
            degree.as_deref().is_none_or(&mut check) && check(radicand)
        }
        MathExpression::Delimiter { content, .. } => check(content),
        MathExpression::Function { name, argument } => check(name) && check(argument),
        MathExpression::Accent { base, .. } => check(base),
        MathExpression::Limit { base, limit, .. } => check(base) && check(limit),
        MathExpression::Nary {
            lower, upper, base, ..
        } => {
            lower.as_deref().is_none_or(&mut check)
                && upper.as_deref().is_none_or(&mut check)
                && check(base)
        }
        MathExpression::Matrix { rows } => {
            !rows.is_empty()
                && rows
                    .iter()
                    .all(|row| !row.cells.is_empty() && row.cells.iter().all(&mut check))
        }
        MathExpression::EqArray { rows } => !rows.is_empty() && rows.iter().all(&mut check),
        MathExpression::Bar { base, .. } => check(base),
        MathExpression::GroupChar { base, .. } => check(base),
        MathExpression::PreScript {
            base,
            subscript,
            superscript,
        } => {
            (subscript.is_some() || superscript.is_some())
                && check(base)
                && subscript.as_deref().is_none_or(&mut check)
                && superscript.as_deref().is_none_or(&mut check)
        }
        MathExpression::BorderBox { content, .. } => check(content),
        MathExpression::Box { content } => check(content),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_common_constructs() {
        let xml = concat!(
            "<m:oMath>",
            "<m:f><m:num><m:r><m:t>a</m:t></m:r></m:num>",
            "<m:den><m:sSup><m:e><m:r><m:t>b</m:t></m:r></m:e>",
            "<m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:den></m:f>",
            "</m:oMath>"
        );
        assert!(matches!(
            parse_math_expression(xml),
            Some(MathExpression::Fraction { .. })
        ));
    }

    #[test]
    fn rejects_non_math_qualified_lookalikes() {
        assert!(parse_math_expression("<w:oMath><w:r><w:t>x</w:t></w:r></w:oMath>").is_none());
    }

    #[test]
    fn unsupported_structure_has_no_projection() {
        // `m:phant` (phantom spacing) is intentionally excluded from the projected
        // subset; an equation containing it has no typed projection (the raw OMML
        // is retained and remains authoritative for export).
        assert!(
            parse_math_expression(
                "<m:oMath><m:phant><m:e><m:r><m:t>x</m:t></m:r></m:e></m:phant></m:oMath>"
            )
            .is_none()
        );
    }

    #[test]
    fn projects_pre_script() {
        let pre_sub_sup = parse_math_expression(concat!(
            "<m:oMath><m:sPre><m:sub><m:r><m:t>i</m:t></m:r></m:sub>",
            "<m:sup><m:r><m:t>j</m:t></m:r></m:sup>",
            "<m:e><m:r><m:t>X</m:t></m:r></m:e></m:sPre></m:oMath>"
        ));
        assert!(matches!(
            pre_sub_sup,
            Some(MathExpression::PreScript {
                subscript: Some(_),
                superscript: Some(_),
                ..
            })
        ));

        // Only a pre-superscript present (empty sub omitted).
        let pre_sup_only = parse_math_expression(concat!(
            "<m:oMath><m:sPre><m:sub/><m:sup><m:r><m:t>2</m:t></m:r></m:sup>",
            "<m:e><m:r><m:t>X</m:t></m:r></m:e></m:sPre></m:oMath>"
        ));
        assert!(matches!(
            pre_sup_only,
            Some(MathExpression::PreScript {
                subscript: None,
                superscript: Some(_),
                ..
            })
        ));
    }

    #[test]
    fn projects_border_box_edges() {
        // Default borderBox (no borderBoxPr): all borders drawn, no strikes.
        let plain = parse_math_expression(
            "<m:oMath><m:borderBox><m:e><m:r><m:t>x</m:t></m:r></m:e></m:borderBox></m:oMath>",
        );
        assert!(matches!(
            plain,
            Some(MathExpression::BorderBox { borders, .. }) if borders == MathBorderBox::default()
        ));

        // A hidden top border plus a horizontal strike are captured.
        let decorated = parse_math_expression(concat!(
            "<m:oMath><m:borderBox><m:borderBoxPr><m:hideTop m:val=\"1\"/>",
            "<m:strikeH m:val=\"on\"/></m:borderBoxPr>",
            "<m:e><m:r><m:t>x</m:t></m:r></m:e></m:borderBox></m:oMath>"
        ));
        let Some(MathExpression::BorderBox { borders, .. }) = decorated else {
            panic!("expected a border box");
        };
        assert!(borders.hide_top);
        assert!(borders.strike_horizontal);
        assert!(!borders.hide_bottom);
        assert!(!borders.strike_vertical);
    }

    #[test]
    fn projects_grouping_box() {
        let boxed = parse_math_expression(
            "<m:oMath><m:box><m:e><m:r><m:t>x</m:t></m:r></m:e></m:box></m:oMath>",
        );
        assert!(matches!(
            boxed,
            Some(MathExpression::Box { content }) if matches!(*content, MathExpression::Text { .. })
        ));
    }

    #[test]
    fn projects_function_accent_limit_nary_matrix_and_eqarray() {
        let function = parse_math_expression(concat!(
            "<m:oMath><m:func><m:fName><m:r><m:t>sin</m:t></m:r></m:fName>",
            "<m:e><m:r><m:t>x</m:t></m:r></m:e></m:func></m:oMath>"
        ));
        assert!(matches!(function, Some(MathExpression::Function { .. })));

        let accent = parse_math_expression(concat!(
            "<m:oMath><m:acc><m:accPr><m:chr m:val=\"^\"/></m:accPr>",
            "<m:e><m:r><m:t>x</m:t></m:r></m:e></m:acc></m:oMath>"
        ));
        assert!(matches!(
            accent,
            Some(MathExpression::Accent { accent, .. }) if accent == "^"
        ));

        let limit = parse_math_expression(concat!(
            "<m:oMath><m:limLow><m:e><m:r><m:t>lim</m:t></m:r></m:e>",
            "<m:lim><m:r><m:t>n</m:t></m:r></m:lim></m:limLow></m:oMath>"
        ));
        assert!(matches!(
            limit,
            Some(MathExpression::Limit {
                position: LimitPosition::Lower,
                ..
            })
        ));

        let nary = parse_math_expression(concat!(
            "<m:oMath><m:nary><m:naryPr><m:chr m:val=\"\u{2211}\"/></m:naryPr>",
            "<m:sub><m:r><m:t>i</m:t></m:r></m:sub><m:sup><m:r><m:t>n</m:t></m:r></m:sup>",
            "<m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>"
        ));
        assert!(matches!(
            nary,
            Some(MathExpression::Nary { operator, lower: Some(_), upper: Some(_), .. })
                if operator == "\u{2211}"
        ));

        let matrix = parse_math_expression(concat!(
            "<m:oMath><m:m><m:mr><m:e><m:r><m:t>a</m:t></m:r></m:e>",
            "<m:e><m:r><m:t>b</m:t></m:r></m:e></m:mr></m:m></m:oMath>"
        ));
        assert!(matches!(
            matrix,
            Some(MathExpression::Matrix { rows }) if rows.len() == 1 && rows[0].cells.len() == 2
        ));

        let eq_array = parse_math_expression(concat!(
            "<m:oMath><m:eqArr><m:e><m:r><m:t>a</m:t></m:r></m:e>",
            "<m:e><m:r><m:t>b</m:t></m:r></m:e></m:eqArr></m:oMath>"
        ));
        assert!(matches!(
            eq_array,
            Some(MathExpression::EqArray { rows }) if rows.len() == 2
        ));
    }

    #[test]
    fn projects_bar_and_group_character() {
        let overbar = parse_math_expression(concat!(
            "<m:oMath><m:bar><m:barPr><m:pos m:val=\"top\"/></m:barPr>",
            "<m:e><m:r><m:t>x</m:t></m:r></m:e></m:bar></m:oMath>"
        ));
        assert!(matches!(
            overbar,
            Some(MathExpression::Bar {
                position: BarPosition::Top,
                ..
            })
        ));

        let underbar = parse_math_expression(concat!(
            "<m:oMath><m:bar><m:barPr><m:pos m:val=\"bot\"/></m:barPr>",
            "<m:e><m:r><m:t>y</m:t></m:r></m:e></m:bar></m:oMath>"
        ));
        assert!(matches!(
            underbar,
            Some(MathExpression::Bar {
                position: BarPosition::Bottom,
                ..
            })
        ));

        // An absent `m:pos` defaults to `Top` (overline).
        let default_bar = parse_math_expression(
            "<m:oMath><m:bar><m:e><m:r><m:t>z</m:t></m:r></m:e></m:bar></m:oMath>",
        );
        assert!(matches!(
            default_bar,
            Some(MathExpression::Bar {
                position: BarPosition::Top,
                ..
            })
        ));

        let overbrace = parse_math_expression(concat!(
            "<m:oMath><m:groupChr><m:groupChrPr><m:chr m:val=\"\u{23DE}\"/>",
            "<m:pos m:val=\"top\"/></m:groupChrPr>",
            "<m:e><m:r><m:t>x</m:t></m:r></m:e></m:groupChr></m:oMath>"
        ));
        assert!(matches!(
            overbrace,
            Some(MathExpression::GroupChar { character, position: GroupPosition::Top, .. })
                if character == "\u{23DE}"
        ));

        let underbrace = parse_math_expression(concat!(
            "<m:oMath><m:groupChr><m:groupChrPr><m:chr m:val=\"\u{23DF}\"/>",
            "<m:pos m:val=\"bot\"/></m:groupChrPr>",
            "<m:e><m:r><m:t>x</m:t></m:r></m:e></m:groupChr></m:oMath>"
        ));
        assert!(matches!(
            underbrace,
            Some(MathExpression::GroupChar { character, position: GroupPosition::Bottom, .. })
                if character == "\u{23DF}"
        ));
    }
}
