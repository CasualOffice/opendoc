//! Bounded semantic projection of retained Office Math Markup Language.

use casual_doc_model::v1::{MAX_MATH_BYTES, MAX_MATH_DEPTH, MAX_MATH_NODES, MathExpression};
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
        // Property containers affect advanced typography and are intentionally
        // ignored for this first common-construct projection.
        name if name.ends_with(b"Pr") || name == b"ctrlPr" => return Some(None),
        // Wrappers are consumed by their owning construct, never independently.
        b"t" | b"e" | b"num" | b"den" | b"sub" | b"sup" | b"deg" => {
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
        assert!(
            parse_math_expression(
                "<m:oMath><m:m><m:mr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:mr></m:m></m:oMath>"
            )
            .is_none()
        );
    }
}
