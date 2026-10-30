//! HTML file import: a bounded tokenizer driving the shared
//! [`FlowBuilder`](crate::flow_builder::FlowBuilder).
//!
//! # The mapping is the paste path's
//!
//! Pasting HTML already maps it (`webapp/src/clipboard.mjs` `htmlToRuns` and
//! `htmlToStructured`, then `pasteExternalStructured`): `<h1>`..`<h6>` take the
//! Heading styles, `<ul>`/`<ol>` become real lists at their nesting level,
//! `<table>` becomes a table, `<b>`/`<i>`/`<u>`/`<s>`/`<sup>`/`<sub>` and the
//! same inline `style` declarations (`font-weight`, `font-style`,
//! `text-decoration`) set run properties, `<a href>` becomes a link only for a
//! safe scheme, and `<script>`, `<style>`, `<head>` and their kin are skipped.
//! That mapping runs in the browser over a `DOMParser` tree; a file opened by
//! the SDK, MCP's `open_document` or the native viewer has no DOM, so the same
//! vocabulary is read here by a tokenizer and built through the builder the
//! Markdown importer uses. A paste and an opened file therefore land as the
//! same structure, and so does a Markdown file that says the same thing.
//!
//! # Sanitised by construction
//!
//! Nothing is executed and nothing is fetched. A `<script>`, `<style>`,
//! `<template>`, `<iframe>` or `<object>` body is skipped without being parsed
//! as markup (raw-text elements are scanned to their end tag, as the HTML
//! tokenizer specifies), event-handler attributes are never read, a link whose
//! scheme is not `http`, `https`, `mailto`, `ftp` or `tel` is text, and a
//! remote picture is a reported finding with its alt text kept as a link.
//!
//! # Bounds and complexity
//!
//! The input is refused over [`HtmlLimits::max_input_bytes`]; the open-element
//! stack is refused past [`HtmlLimits::max_input_nesting_depth`]. The walk is
//! iterative (no recursion), one pass, O(input).

use std::borrow::Cow;

use casual_doc_model::v1::Alignment;

use crate::flow_builder::{
    BlockKind, FlowBuilder, Imported, Link, UrlKind, classify_url, decode_data_uri, is_remote,
};

use crate::{AdapterError, HtmlLimits, ModelOutcome};

/// Elements whose content is raw text: scanned to the matching end tag and
/// never parsed as markup. `script` and `style` are the reason this exists.
const RAW_TEXT: &[&str] = &[
    "script",
    "style",
    "textarea",
    "title",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "noscript",
    "plaintext",
];

/// Elements skipped with everything inside them (`clipboard.mjs` SKIP_TAGS and
/// the elements a document file carries that a clipboard does not).
const SKIP: &[&str] = &[
    "head", "template", "svg", "math", "object", "select", "datalist", "canvas", "video", "audio",
    "map", "button", "applet",
];

/// Elements that never have content or an end tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr", "keygen",
];

/// Elements that are a paragraph boundary (`clipboard.mjs` BLOCK_TAGS).
const BLOCK: &[&str] = &[
    "p",
    "div",
    "section",
    "article",
    "header",
    "footer",
    "main",
    "nav",
    "aside",
    "figure",
    "figcaption",
    "address",
    "center",
    "dl",
    "dt",
    "dd",
    "details",
    "summary",
    "form",
    "fieldset",
    "legend",
    "caption",
    "body",
    "hgroup",
];

#[derive(Debug)]
enum Token<'a> {
    Text(&'a str),
    Start {
        name: String,
        attributes: Vec<(String, String)>,
        self_closing: bool,
    },
    End(String),
}

/// The bounded HTML tokenizer: text, start tags with attributes, end tags;
/// comments, doctypes and processing instructions are consumed and dropped.
struct Tokenizer<'a> {
    source: &'a str,
    position: usize,
    /// Inside a raw-text element: its name, scanned to `</name`.
    raw: Option<String>,
}

impl<'a> Tokenizer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            position: 0,
            raw: None,
        }
    }

    fn rest(&self) -> &'a str {
        &self.source[self.position..]
    }

    fn next_token(&mut self) -> Option<Token<'a>> {
        loop {
            if self.position >= self.source.len() {
                return None;
            }
            if let Some(name) = self.raw.take() {
                // Scanned, never tokenized: `<script>a < b && "</p>"</script>`
                // is one text token, and the mapper drops it (every raw-text
                // element is skipped). The `</name` that ends it is parsed as
                // an ordinary end tag on the next call.
                let rest = self.rest();
                let end =
                    find_ascii_case_insensitive(rest, &format!("</{name}")).unwrap_or(rest.len());
                self.position += end;
                if end > 0 {
                    return Some(Token::Text(&rest[..end]));
                }
                continue;
            }
            let rest = self.rest();
            if !rest.starts_with('<') {
                let end = rest.find('<').unwrap_or(rest.len());
                self.position += end;
                return Some(Token::Text(&rest[..end]));
            }
            if let Some(comment) = rest.strip_prefix("<!--") {
                let end = comment.find("-->").map_or(rest.len(), |at| at + 7);
                self.position += end;
                continue;
            }
            if rest.starts_with("<!") || rest.starts_with("<?") {
                let end = rest.find('>').map_or(rest.len(), |at| at + 1);
                self.position += end;
                continue;
            }
            if let Some(after) = rest.strip_prefix("</")
                && after.starts_with(|c: char| c.is_ascii_alphabetic())
            {
                let name_end = after
                    .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
                    .unwrap_or(after.len());
                let name = after[..name_end].to_ascii_lowercase();
                let end = rest.find('>').map_or(rest.len(), |at| at + 1);
                self.position += end;
                return Some(Token::End(name));
            }
            if let Some(after) = rest.strip_prefix('<')
                && after.starts_with(|c: char| c.is_ascii_alphabetic())
            {
                return Some(self.start_tag());
            }
            // A `<` that opens nothing is text.
            self.position += 1;
            return Some(Token::Text(&rest[..1]));
        }
    }

    fn start_tag(&mut self) -> Token<'a> {
        let bytes = self.source.as_bytes();
        let mut index = self.position + 1;
        let name_start = index;
        while index < bytes.len()
            && !bytes[index].is_ascii_whitespace()
            && bytes[index] != b'>'
            && bytes[index] != b'/'
        {
            index += 1;
        }
        let name = self.source[name_start..index].to_ascii_lowercase();
        let mut attributes = Vec::new();
        let mut self_closing = false;
        loop {
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index >= bytes.len() {
                break;
            }
            match bytes[index] {
                b'>' => {
                    index += 1;
                    break;
                }
                b'/' => {
                    index += 1;
                    if bytes.get(index) == Some(&b'>') {
                        self_closing = true;
                        index += 1;
                        break;
                    }
                    continue;
                }
                _ => {}
            }
            let attribute_start = index;
            while index < bytes.len()
                && !bytes[index].is_ascii_whitespace()
                && !matches!(bytes[index], b'=' | b'>' | b'/')
            {
                index += 1;
            }
            let attribute = self.source[attribute_start..index].to_ascii_lowercase();
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            let mut value = String::new();
            if bytes.get(index) == Some(&b'=') {
                index += 1;
                while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                match bytes.get(index) {
                    Some(&quote @ (b'"' | b'\'')) => {
                        index += 1;
                        let value_start = index;
                        while index < bytes.len() && bytes[index] != quote {
                            index += 1;
                        }
                        value = decode_entities(&self.source[value_start..index]).into_owned();
                        index = (index + 1).min(bytes.len());
                    }
                    _ => {
                        let value_start = index;
                        while index < bytes.len()
                            && !bytes[index].is_ascii_whitespace()
                            && bytes[index] != b'>'
                        {
                            index += 1;
                        }
                        value = decode_entities(&self.source[value_start..index]).into_owned();
                    }
                }
            }
            if !attribute.is_empty() {
                attributes.push((attribute, value));
            }
        }
        self.position = index;
        if RAW_TEXT.contains(&name.as_str()) && !self_closing {
            self.raw = Some(name.clone());
        }
        Token::Start {
            name,
            attributes,
            self_closing,
        }
    }
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|&start| haystack[start..start + needle.len()].eq_ignore_ascii_case(needle))
}

/// Decodes character references: numeric ones and the named ones a document
/// file actually uses. An unknown name is left as written. O(text).
pub(crate) fn decode_entities(text: &str) -> Cow<'_, str> {
    if !text.contains('&') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest[1..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '#'))
            .map_or(rest.len(), |offset| offset + 1);
        let name = &rest[1..end];
        let decoded = if let Some(number) = name.strip_prefix('#') {
            let value = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => number.parse::<u32>().ok(),
            };
            value
                .and_then(char::from_u32)
                .filter(|c| *c != '\0')
                .map(String::from)
        } else {
            named_entity(name).map(String::from)
        };
        match decoded {
            Some(value) => {
                out.push_str(&value);
                let consumed = if rest[end..].starts_with(';') {
                    end + 1
                } else {
                    end
                };
                rest = &rest[consumed..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

fn named_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{a0}",
        "copy" => "\u{a9}",
        "reg" => "\u{ae}",
        "trade" => "\u{2122}",
        "hellip" => "\u{2026}",
        "mdash" => "\u{2014}",
        "ndash" => "\u{2013}",
        "lsquo" => "\u{2018}",
        "rsquo" => "\u{2019}",
        "ldquo" => "\u{201c}",
        "rdquo" => "\u{201d}",
        "bull" => "\u{2022}",
        "middot" => "\u{b7}",
        "deg" => "\u{b0}",
        "times" => "\u{d7}",
        "divide" => "\u{f7}",
        "euro" => "\u{20ac}",
        "pound" => "\u{a3}",
        "yen" => "\u{a5}",
        "cent" => "\u{a2}",
        "sect" => "\u{a7}",
        "para" => "\u{b6}",
        "laquo" => "\u{ab}",
        "raquo" => "\u{bb}",
        "shy" => "\u{ad}",
        "larr" => "\u{2190}",
        "rarr" => "\u{2192}",
        "uarr" => "\u{2191}",
        "darr" => "\u{2193}",
        "plusmn" => "\u{b1}",
        "frac12" => "\u{bd}",
        "frac14" => "\u{bc}",
        "frac34" => "\u{be}",
        "ensp" => "\u{2002}",
        "emsp" => "\u{2003}",
        "thinsp" => "\u{2009}",
        "zwj" => "\u{200d}",
        "zwnj" => "\u{200c}",
        _ => return None,
    })
}

/// What an open element did when it opened, so its end undoes exactly that.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Transparent,
    Block,
    Heading,
    Format,
    Link(bool),
    List,
    Item,
    Quote,
    Pre,
    Table,
    Section,
    Row,
    Cell,
    Skip,
    Aligned(Option<Alignment>),
}

struct Open {
    name: String,
    action: Action,
}

struct Mapper {
    builder: FlowBuilder,
    stack: Vec<Open>,
    max_depth: usize,
    /// Depth of skipped elements around the cursor (content dropped).
    skipping: usize,
    pre: usize,
    in_head_section: bool,
    rows_in_table: Vec<usize>,
    pending_space: bool,
    line_has_text: bool,
}

/// Imports an HTML document. O(input).
pub(crate) fn import_html(
    source: &str,
    limits: HtmlLimits,
    namespace: u64,
) -> Result<Imported, AdapterError> {
    let mut mapper = Mapper {
        builder: FlowBuilder::new(namespace, "html.import"),
        stack: Vec::new(),
        max_depth: limits.max_input_nesting_depth,
        skipping: 0,
        pre: 0,
        in_head_section: false,
        rows_in_table: Vec::new(),
        pending_space: false,
        line_has_text: false,
    };
    let mut tokenizer = Tokenizer::new(source);
    while let Some(token) = tokenizer.next_token() {
        match token {
            Token::Text(text) => mapper.text(text)?,
            Token::Start {
                name,
                attributes,
                self_closing,
            } => mapper.start(name, &attributes, self_closing)?,
            Token::End(name) => mapper.end(&name)?,
        }
    }
    while let Some(open) = mapper.stack.pop() {
        mapper.close(&open)?;
    }
    mapper.builder.finish()
}

fn attribute<'a>(attributes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// The few inline-style declarations the paste path reads (`applyInlineStyle`).
fn declarations(style: &str) -> impl Iterator<Item = (String, String)> + '_ {
    style.split(';').filter_map(|declaration| {
        let (property, value) = declaration.split_once(':')?;
        Some((
            property.trim().to_ascii_lowercase(),
            value.trim().to_ascii_lowercase(),
        ))
    })
}

fn alignment_of(attributes: &[(String, String)]) -> Option<Alignment> {
    let from_style = attribute(attributes, "style").and_then(|style| {
        declarations(style)
            .find(|(property, _)| property == "text-align")
            .map(|(_, value)| value)
    });
    let value =
        from_style.or_else(|| attribute(attributes, "align").map(str::to_ascii_lowercase))?;
    match value.trim_end_matches("!important").trim() {
        "left" | "start" => Some(Alignment::Start),
        "right" | "end" => Some(Alignment::End),
        "center" => Some(Alignment::Center),
        "justify" => Some(Alignment::Justify),
        _ => None,
    }
}

impl Mapper {
    fn text(&mut self, text: &str) -> Result<(), AdapterError> {
        if self.skipping > 0 {
            return Ok(());
        }
        let decoded = decode_entities(text);
        if self.pre > 0 {
            self.line_has_text = true;
            return self.builder.text(&decoded);
        }
        let mut word = String::new();
        for character in decoded.chars() {
            if character.is_ascii_whitespace() {
                if !word.is_empty() {
                    self.word(&word)?;
                    word.clear();
                }
                if self.line_has_text {
                    self.pending_space = true;
                }
            } else {
                word.push(character);
            }
        }
        if !word.is_empty() {
            self.word(&word)?;
        }
        Ok(())
    }

    fn word(&mut self, word: &str) -> Result<(), AdapterError> {
        // Text between table cells has nowhere to go in a table; it is
        // written before the table, as a browser's foster parenting does.
        if self.pending_space {
            self.builder.text(" ")?;
            self.pending_space = false;
        }
        self.line_has_text = true;
        self.builder.text(word)
    }

    fn boundary(&mut self) -> Result<(), AdapterError> {
        self.pending_space = false;
        self.line_has_text = false;
        self.builder.paragraph_break()
    }

    fn push(&mut self, name: String, action: Action) -> Result<(), AdapterError> {
        if self.stack.len() >= self.max_depth {
            return Err(AdapterError::new(format!(
                "limit html_input_nesting_depth observed {} exceeds allowed {}",
                self.stack.len() + 1,
                self.max_depth
            )));
        }
        self.stack.push(Open { name, action });
        Ok(())
    }

    /// Closes open elements down to and including the innermost `name`, but
    /// not past a `boundary` element (a list for `li`, a table for `td`).
    fn close_open(&mut self, names: &[&str], boundaries: &[&str]) -> Result<(), AdapterError> {
        let Some(at) = self.stack.iter().rposition(|open| {
            names.contains(&open.name.as_str()) || boundaries.contains(&open.name.as_str())
        }) else {
            return Ok(());
        };
        if boundaries.contains(&self.stack[at].name.as_str()) {
            return Ok(());
        }
        while self.stack.len() > at {
            let open = self.stack.pop().expect("an open element");
            self.close(&open)?;
        }
        Ok(())
    }

    fn start(
        &mut self,
        name: String,
        attributes: &[(String, String)],
        self_closing: bool,
    ) -> Result<(), AdapterError> {
        if self.skipping > 0 {
            if !VOID.contains(&name.as_str()) && !self_closing {
                self.skipping += 1;
                self.push(name, Action::Skip)?;
            }
            return Ok(());
        }
        let tag = name.as_str();
        if SKIP.contains(&tag) || RAW_TEXT.contains(&tag) {
            if !matches!(tag, "head" | "title" | "style" | "noscript" | "template") {
                self.builder
                    .report(&format!("skipped.{tag}"), ModelOutcome::Omitted);
            }
            if !self_closing {
                self.skipping += 1;
                self.push(name, Action::Skip)?;
            }
            return Ok(());
        }
        // A block-level start closes an open `<p>`, as the HTML parser does.
        let closes_paragraph = BLOCK.contains(&tag)
            || matches!(
                tag,
                "h1" | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "ul"
                    | "ol"
                    | "pre"
                    | "blockquote"
                    | "table"
                    | "hr"
            );
        if closes_paragraph {
            self.close_open(&["p"], &["li", "td", "th", "blockquote", "table"])?;
        }
        // The space before an inline element belongs to the text outside it:
        // `a <b>b</b>` is "a " then a bold "b", not "a" then a bold " b".
        let structural = closes_paragraph
            || matches!(
                tag,
                "li" | "tr" | "td" | "th" | "thead" | "tbody" | "tfoot" | "br"
            );
        if !structural && self.pending_space {
            self.builder.text(" ")?;
            self.pending_space = false;
        }
        match tag {
            "br" => {
                if self.pre > 0 {
                    self.builder.text("\n")?;
                } else {
                    self.builder.line_break()?;
                    self.pending_space = false;
                    self.line_has_text = false;
                }
                return Ok(());
            }
            "hr" => {
                self.boundary()?;
                return self.builder.rule();
            }
            "img" => return self.image(attributes),
            "input" => {
                if attribute(attributes, "type")
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("checkbox"))
                {
                    let checked = attribute(attributes, "checked").is_some();
                    self.line_has_text = true;
                    return self.builder.checkbox(checked);
                }
                self.builder.report("skipped.input", ModelOutcome::Omitted);
                return Ok(());
            }
            _ if VOID.contains(&tag) => return Ok(()),
            _ => {}
        }
        let action = match tag {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.boundary()?;
                let level = tag.as_bytes()[1] - b'0';
                self.builder.begin_block(BlockKind::Heading(level))?;
                Action::Heading
            }
            "ul" | "ol" => {
                self.boundary()?;
                let start = attribute(attributes, "start")
                    .and_then(|value| value.trim().parse::<u32>().ok())
                    .unwrap_or(1);
                self.builder.begin_list(tag == "ol", start)?;
                Action::List
            }
            "li" => {
                self.close_open(&["li"], &["ul", "ol"])?;
                self.boundary()?;
                self.builder.begin_item()?;
                Action::Item
            }
            "blockquote" => {
                self.boundary()?;
                self.builder.begin_quote()?;
                Action::Quote
            }
            "pre" | "listing" => {
                self.boundary()?;
                self.builder.begin_block(BlockKind::Code)?;
                self.pre += 1;
                Action::Pre
            }
            "table" => {
                self.boundary()?;
                self.builder.begin_table(Vec::new())?;
                self.rows_in_table.push(0);
                Action::Table
            }
            "thead" | "tbody" | "tfoot" => {
                self.in_head_section = tag == "thead";
                Action::Section
            }
            "tr" => {
                if !self.builder.in_table() {
                    return Ok(());
                }
                self.close_open(&["tr"], &["table"])?;
                self.builder.begin_row(self.in_head_section)?;
                if let Some(rows) = self.rows_in_table.last_mut() {
                    *rows += 1;
                }
                Action::Row
            }
            "td" | "th" => {
                if !self.builder.in_table() {
                    self.boundary()?;
                    self.push(name, Action::Block)?;
                    return self.styled(attributes);
                }
                self.close_open(&["td", "th"], &["tr", "table"])?;
                let span = attribute(attributes, "colspan")
                    .and_then(|value| value.trim().parse::<u32>().ok())
                    .unwrap_or(1);
                if attribute(attributes, "rowspan")
                    .and_then(|value| value.trim().parse::<u32>().ok())
                    .is_some_and(|rows| rows > 1)
                {
                    // The paste path degrades a vertical merge the same way:
                    // a cell per row, the content in the first.
                    self.builder.report("rowspan", ModelOutcome::Degraded);
                }
                // A header cell in the table's first row makes that row the
                // header, as `<th>` cells without a `<thead>` mean.
                if tag == "th" && self.rows_in_table.last().is_some_and(|rows| *rows <= 1) {
                    self.builder.mark_row_header();
                }
                self.pending_space = false;
                self.line_has_text = false;
                self.builder.begin_cell(alignment_of(attributes), span)?;
                Action::Cell
            }
            "b" | "strong" => self.format(|format| format.bold = true),
            "i" | "em" | "cite" | "var" | "dfn" => self.format(|format| format.italic = true),
            "u" | "ins" => self.format(|format| format.underline = true),
            "s" | "strike" | "del" => self.format(|format| format.strike = true),
            "sup" => self.format(|format| format.superscript = true),
            "sub" => self.format(|format| format.subscript = true),
            "code" | "kbd" | "samp" | "tt" => self.format(|format| format.code = true),
            "a" => match attribute(attributes, "href") {
                Some(href) => match classify_url(href) {
                    UrlKind::Internal(anchor) => {
                        self.builder.begin_link(Link::Internal(anchor));
                        Action::Link(true)
                    }
                    UrlKind::Safe(url) => {
                        self.builder.begin_link(Link::External(url));
                        Action::Link(true)
                    }
                    UrlKind::Unsafe => {
                        self.builder
                            .report("link_unsafe_scheme", ModelOutcome::Degraded);
                        Action::Link(false)
                    }
                },
                None => Action::Link(false),
            },
            _ if BLOCK.contains(&tag) => {
                self.boundary()?;
                let alignment = alignment_of(attributes);
                if alignment.is_some() && !self.builder.in_cell() {
                    let previous = self.builder.alignment();
                    self.builder.set_alignment(alignment);
                    self.push(name, Action::Aligned(previous))?;
                    return self.styled(attributes);
                }
                Action::Block
            }
            _ => Action::Transparent,
        };
        self.push(name, action)?;
        if !matches!(action, Action::Skip) {
            self.styled(attributes)?;
        }
        Ok(())
    }

    /// An element's inline `style` turns run properties on for its content,
    /// as `applyInlineStyle` does on paste. Each element gets a format scope
    /// of its own (pushed even when the style changes nothing), closed by
    /// `close`.
    fn styled(&mut self, attributes: &[(String, String)]) -> Result<(), AdapterError> {
        let style = attribute(attributes, "style").unwrap_or("");
        let mut bold = false;
        let mut italic = false;
        let mut underline = false;
        let mut strike = false;
        let mut superscript = false;
        let mut subscript = false;
        for (property, value) in declarations(style) {
            match property.as_str() {
                "font-weight" => {
                    bold = value.starts_with("bold")
                        || value.parse::<u32>().is_ok_and(|weight| weight >= 600);
                }
                "font-style" => {
                    italic = value.starts_with("italic") || value.starts_with("oblique")
                }
                "text-decoration" | "text-decoration-line" => {
                    underline |= value.contains("underline");
                    strike |= value.contains("line-through");
                }
                "vertical-align" => {
                    superscript |= value.starts_with("super");
                    subscript |= value.starts_with("sub");
                }
                _ => {}
            }
        }
        self.builder.push_format(|format| {
            format.bold |= bold;
            format.italic |= italic;
            format.underline |= underline;
            format.strike |= strike;
            format.superscript |= superscript;
            format.subscript |= subscript;
        });
        Ok(())
    }

    fn format(&mut self, change: impl FnOnce(&mut crate::flow_builder::Format)) -> Action {
        self.builder.push_format(change);
        Action::Format
    }

    fn end(&mut self, name: &str) -> Result<(), AdapterError> {
        let Some(at) = self.stack.iter().rposition(|open| open.name == name) else {
            // `</p>` with no `<p>` is an empty paragraph boundary in HTML;
            // every other stray end tag is ignored, as a browser ignores it.
            if name == "p" && self.skipping == 0 {
                self.boundary()?;
            }
            return Ok(());
        };
        while self.stack.len() > at {
            let open = self.stack.pop().expect("an open element");
            self.close(&open)?;
        }
        Ok(())
    }

    fn close(&mut self, open: &Open) -> Result<(), AdapterError> {
        match open.action {
            Action::Skip => {
                self.skipping = self.skipping.saturating_sub(1);
                return Ok(());
            }
            Action::Transparent | Action::Section => {
                if open.action == Action::Section {
                    self.in_head_section = false;
                }
            }
            Action::Block => self.boundary()?,
            Action::Aligned(previous) => {
                self.boundary()?;
                self.builder.set_alignment(previous);
            }
            Action::Heading => {
                self.pending_space = false;
                self.line_has_text = false;
                self.builder.end_block()?;
            }
            Action::Format => {
                // The element's own tag format and its style scope.
                self.builder.pop_format();
            }
            Action::Link(opened) => {
                if opened {
                    self.builder.end_link()?;
                }
            }
            Action::List => {
                self.boundary()?;
                self.builder.end_list()?;
            }
            Action::Item => {
                self.boundary()?;
                self.builder.end_item()?;
            }
            Action::Quote => {
                self.boundary()?;
                self.builder.end_quote()?;
            }
            Action::Pre => {
                self.pre = self.pre.saturating_sub(1);
                self.pending_space = false;
                self.line_has_text = false;
                self.builder.end_block()?;
            }
            Action::Table => {
                self.rows_in_table.pop();
                self.builder.end_table()?;
                self.pending_space = false;
                self.line_has_text = false;
            }
            Action::Row => self.builder.end_row()?,
            Action::Cell => {
                self.builder.end_cell()?;
                self.pending_space = false;
                self.line_has_text = false;
            }
        }
        // Every non-skip element pushed one style scope in `styled`.
        self.builder.pop_format();
        Ok(())
    }

    fn image(&mut self, attributes: &[(String, String)]) -> Result<(), AdapterError> {
        let source = attribute(attributes, "src").unwrap_or("").trim().to_owned();
        let alt = attribute(attributes, "alt").unwrap_or("").to_owned();
        if self.pending_space {
            self.builder.text(" ")?;
            self.pending_space = false;
        }
        self.line_has_text = true;
        if source.to_ascii_lowercase().starts_with("data:") {
            if let Some((_, bytes)) = decode_data_uri(&source)
                && self.builder.picture(bytes, &alt)?
            {
                return Ok(());
            }
            self.builder
                .report("image_unsupported", ModelOutcome::Degraded);
            return self.builder.text(&alt);
        }
        let feature = if is_remote(&source) {
            "image_remote"
        } else {
            "image_unresolved"
        };
        self.builder.report(feature, ModelOutcome::Degraded);
        let label = if alt.trim().is_empty() {
            source.as_str()
        } else {
            alt.as_str()
        };
        if label.is_empty() {
            return Ok(());
        }
        match classify_url(&source) {
            UrlKind::Safe(url) => {
                self.builder.begin_link(Link::External(url));
                self.builder.text(label)?;
                self.builder.end_link()
            }
            _ => self.builder.text(label),
        }
    }
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
