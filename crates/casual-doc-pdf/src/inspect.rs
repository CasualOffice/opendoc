//! Reading a produced PDF back.
//!
//! This is a small, deliberately independent PDF *consumer*: it walks the
//! cross-reference table, the page tree and the content streams the way a
//! viewer does, and recovers the text layer through each font's `ToUnicode`
//! CMap. Nothing here shares code with the writer.
//!
//! It exists because "a PDF was produced" proves nothing about the defect this
//! backend fixes. The guarantee is that the text is *extractable*, in the right
//! place, from an embedded subset font — and the only way to assert a guarantee
//! about a file is to read the file. The same entry points are useful to a host
//! that wants to verify an export before handing it to a user.
//!
//! # Scope
//!
//! It understands what this crate writes: classic cross-reference tables,
//! direct-length Flate or raw streams, `Tf`/`Tm`/`Tz`/`TJ`/`Tj` text, `Do`
//! image placements, and `bfchar`-style `ToUnicode` maps. It is not a general
//! PDF reader: cross-reference streams, object streams, encryption and
//! `bfrange` mappings are out of scope and surface as missing data rather than
//! as wrong data.

use std::collections::BTreeMap;
use std::io::Read;

use flate2::read::ZlibDecoder;

/// A parsed PDF object.
#[derive(Clone, Debug, PartialEq)]
pub enum Object {
    /// The null object.
    Null,
    /// A boolean.
    Bool(bool),
    /// A numeric object.
    Number(f64),
    /// A string object's bytes, unescaped.
    String(Vec<u8>),
    /// A name, without its leading slash.
    Name(String),
    /// An array.
    Array(Vec<Object>),
    /// A dictionary.
    Dict(BTreeMap<String, Object>),
    /// An indirect reference.
    Reference(u32),
    /// A stream: its dictionary and its raw (still encoded) bytes.
    Stream(BTreeMap<String, Object>, Vec<u8>),
}

impl Object {
    /// The dictionary of a dictionary or stream object.
    #[must_use]
    pub fn dict(&self) -> Option<&BTreeMap<String, Object>> {
        match self {
            Self::Dict(dict) | Self::Stream(dict, _) => Some(dict),
            _ => None,
        }
    }

    /// This object as a number.
    #[must_use]
    pub fn number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    /// This object as a name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Name(value) => Some(value),
            _ => None,
        }
    }

    /// This object as an array.
    #[must_use]
    pub fn array(&self) -> Option<&[Object]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }
}

/// A whole PDF file, indexed by object number.
#[derive(Clone, Debug)]
pub struct Pdf {
    objects: BTreeMap<u32, Object>,
    trailer: BTreeMap<String, Object>,
}

/// A run of text recovered from a content stream.
#[derive(Clone, Debug, PartialEq)]
pub struct TextItem {
    /// The recovered characters.
    pub text: String,
    /// The x coordinate of the run's origin, in PDF points from the page's
    /// left edge.
    pub x: f64,
    /// The y coordinate of the run's origin, in PDF points from the page's
    /// bottom edge.
    pub y: f64,
    /// The font resource name the run was drawn with.
    pub font: String,
    /// The type size in points.
    pub size: f64,
}

/// What one page contains.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageContents {
    /// The text runs, in the order the content stream draws them.
    pub text: Vec<TextItem>,
    /// The XObject names the page places with `Do`, in draw order.
    pub images: Vec<String>,
    /// The page's `/MediaBox`, as `[x0, y0, x1, y1]` points.
    pub media_box: [f64; 4],
}

impl PageContents {
    /// The page's text, runs joined with single spaces.
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.text
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// One embedded font, as a reader sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddedFontInfo {
    /// The `/BaseFont` name.
    pub base_font: String,
    /// The `/Subtype` of the descendant font.
    pub subtype: String,
    /// The decoded length of the embedded font program, or `None` when the
    /// descriptor embeds no program at all.
    pub program_bytes: Option<usize>,
    /// Whether a `ToUnicode` CMap is present.
    pub has_to_unicode: bool,
}

/// Parses a PDF file.
///
/// # Errors
///
/// Returns a message describing the first structure that could not be read.
pub fn parse(bytes: &[u8]) -> Result<Pdf, String> {
    if !bytes.starts_with(b"%PDF-") {
        return Err("not a PDF: no %PDF- header".to_owned());
    }
    let start = find_last(bytes, b"startxref").ok_or("no startxref")?;
    let mut lexer = Lexer::new(bytes, start + b"startxref".len());
    let offset = match lexer.object()? {
        Object::Number(value) if value >= 0.0 => value as usize,
        other => return Err(format!("startxref is not an offset: {other:?}")),
    };
    let (offsets, trailer) = read_xref(bytes, offset)?;
    let mut objects = BTreeMap::new();
    for (number, at) in offsets {
        let mut lexer = Lexer::new(bytes, at);
        // `N G obj <object> endobj`
        let _ = lexer.object()?;
        let _ = lexer.object()?;
        lexer.expect_keyword("obj")?;
        objects.insert(number, lexer.indirect_body()?);
    }
    Ok(Pdf { objects, trailer })
}

impl Pdf {
    /// Resolves an object, following one indirect reference.
    #[must_use]
    pub fn resolve<'a>(&'a self, object: &'a Object) -> &'a Object {
        match object {
            Object::Reference(number) => self.objects.get(number).unwrap_or(&Object::Null),
            other => other,
        }
    }

    /// Looks one key up in a dictionary and resolves it.
    #[must_use]
    pub fn get<'a>(&'a self, dict: &'a BTreeMap<String, Object>, key: &str) -> Option<&'a Object> {
        dict.get(key).map(|value| self.resolve(value))
    }

    /// The document catalog.
    #[must_use]
    pub fn catalog(&self) -> Option<&BTreeMap<String, Object>> {
        self.get(&self.trailer, "Root")?.dict()
    }

    /// The trailer dictionary.
    #[must_use]
    pub fn trailer(&self) -> &BTreeMap<String, Object> {
        &self.trailer
    }

    /// The `/Info` dictionary, when the file has one.
    #[must_use]
    pub fn info(&self) -> Option<&BTreeMap<String, Object>> {
        self.get(&self.trailer, "Info")?.dict()
    }

    /// Every page object, in document order.
    #[must_use]
    pub fn pages(&self) -> Vec<&BTreeMap<String, Object>> {
        let Some(catalog) = self.catalog() else {
            return Vec::new();
        };
        let Some(tree) = self.get(catalog, "Pages").and_then(Object::dict) else {
            return Vec::new();
        };
        let Some(kids) = self.get(tree, "Kids").and_then(Object::array) else {
            return Vec::new();
        };
        kids.iter()
            .filter_map(|kid| self.resolve(kid).dict())
            .collect()
    }

    /// Decodes a stream object's payload.
    #[must_use]
    pub fn stream_data(&self, object: &Object) -> Option<Vec<u8>> {
        let Object::Stream(dict, raw) = object else {
            return None;
        };
        match self.get(dict, "Filter").and_then(Object::name) {
            None => Some(raw.clone()),
            Some("FlateDecode") => {
                let mut out = Vec::new();
                ZlibDecoder::new(raw.as_slice())
                    .read_to_end(&mut out)
                    .ok()?;
                Some(out)
            }
            // A pass-through filter (an untouched JPEG, say) is returned as-is;
            // the caller knows from the dictionary what it is holding.
            Some(_) => Some(raw.clone()),
        }
    }

    /// Reads one page's text runs and image placements.
    #[must_use]
    pub fn page_contents(&self, page: &BTreeMap<String, Object>) -> PageContents {
        let media_box = self
            .get(page, "MediaBox")
            .and_then(Object::array)
            .map(|values| {
                let mut box_ = [0.0; 4];
                for (slot, value) in box_.iter_mut().zip(values) {
                    *slot = self.resolve(value).number().unwrap_or(0.0);
                }
                box_
            })
            .unwrap_or_default();
        let Some(content) = self.get(page, "Contents") else {
            return PageContents {
                media_box,
                ..PageContents::default()
            };
        };
        let Some(stream) = self.stream_data(content) else {
            return PageContents {
                media_box,
                ..PageContents::default()
            };
        };
        let fonts = self.page_fonts(page);
        let mut contents = PageContents {
            media_box,
            ..PageContents::default()
        };
        self.walk_content(&stream, &fonts, &mut contents);
        contents
    }

    /// The page's `/Font` resources, each with its `ToUnicode` map.
    fn page_fonts(
        &self,
        page: &BTreeMap<String, Object>,
    ) -> BTreeMap<String, BTreeMap<u16, String>> {
        let mut out = BTreeMap::new();
        let Some(resources) = self.get(page, "Resources").and_then(Object::dict) else {
            return out;
        };
        let Some(fonts) = self.get(resources, "Font").and_then(Object::dict) else {
            return out;
        };
        for (name, font) in fonts {
            let Some(font) = self.resolve(font).dict() else {
                continue;
            };
            let map = self
                .get(font, "ToUnicode")
                .and_then(|object| self.stream_data(object))
                .map(|bytes| parse_to_unicode(&bytes))
                .unwrap_or_default();
            out.insert(name.clone(), map);
        }
        out
    }

    /// Every embedded font in the file, in `/BaseFont` order.
    #[must_use]
    pub fn embedded_fonts(&self) -> Vec<EmbeddedFontInfo> {
        let mut out = Vec::new();
        for object in self.objects.values() {
            let Some(dict) = object.dict() else { continue };
            if self.get(dict, "Subtype").and_then(Object::name) != Some("Type0") {
                continue;
            }
            let base_font = self
                .get(dict, "BaseFont")
                .and_then(Object::name)
                .unwrap_or_default()
                .to_owned();
            let has_to_unicode = self.get(dict, "ToUnicode").is_some();
            let descendant = self
                .get(dict, "DescendantFonts")
                .and_then(Object::array)
                .and_then(|array| array.first())
                .map(|entry| self.resolve(entry))
                .and_then(Object::dict);
            let subtype = descendant
                .and_then(|descendant| self.get(descendant, "Subtype"))
                .and_then(Object::name)
                .unwrap_or_default()
                .to_owned();
            let program_bytes = descendant
                .and_then(|descendant| self.get(descendant, "FontDescriptor"))
                .and_then(Object::dict)
                .and_then(|descriptor| {
                    self.get(descriptor, "FontFile2")
                        .or_else(|| self.get(descriptor, "FontFile3"))
                })
                .and_then(|program| self.stream_data(program))
                .map(|bytes| bytes.len());
            out.push(EmbeddedFontInfo {
                base_font,
                subtype,
                program_bytes,
                has_to_unicode,
            });
        }
        out.sort_by(|a, b| a.base_font.cmp(&b.base_font));
        out
    }

    fn walk_content(
        &self,
        stream: &[u8],
        fonts: &BTreeMap<String, BTreeMap<u16, String>>,
        out: &mut PageContents,
    ) {
        let mut lexer = Lexer::new(stream, 0);
        let mut operands: Vec<Object> = Vec::new();
        let mut font = String::new();
        let mut size = 0.0_f64;
        let mut origin = (0.0_f64, 0.0_f64);
        while let Some(token) = lexer.next_content_token() {
            match token {
                Token::Object(object) => {
                    // A runaway operand stack means the stream is not what this
                    // reader understands; drop the oldest rather than grow.
                    if operands.len() > 64 {
                        operands.remove(0);
                    }
                    operands.push(object);
                }
                Token::Operator(operator) => {
                    match operator.as_str() {
                        "Tf" => {
                            size = operands.last().and_then(Object::number).unwrap_or(0.0);
                            font = operands
                                .iter()
                                .rev()
                                .find_map(|operand| operand.name())
                                .unwrap_or_default()
                                .to_owned();
                        }
                        "Tm" => {
                            let numbers: Vec<f64> =
                                operands.iter().filter_map(Object::number).collect();
                            if numbers.len() >= 6 {
                                origin = (numbers[numbers.len() - 2], numbers[numbers.len() - 1]);
                            }
                        }
                        "Tj" | "TJ" => {
                            let map = fonts.get(&font);
                            let mut text = String::new();
                            for operand in &operands {
                                match operand {
                                    Object::String(bytes) => push_decoded(&mut text, bytes, map),
                                    Object::Array(items) => {
                                        for item in items {
                                            if let Object::String(bytes) = item {
                                                push_decoded(&mut text, bytes, map);
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            if !text.is_empty() {
                                out.text.push(TextItem {
                                    text,
                                    x: origin.0,
                                    y: origin.1,
                                    font: font.clone(),
                                    size,
                                });
                            }
                        }
                        "Do" => {
                            if let Some(name) = operands.iter().rev().find_map(Object::name) {
                                out.images.push(name.to_owned());
                            }
                        }
                        _ => {}
                    }
                    operands.clear();
                }
            }
        }
    }
}

/// Decodes a `Identity-H` string: two bytes per CID, mapped through the font's
/// `ToUnicode` CMap. A CID the map does not name contributes nothing, which is
/// exactly what a viewer's copy does.
fn push_decoded(out: &mut String, bytes: &[u8], map: Option<&BTreeMap<u16, String>>) {
    for pair in bytes.chunks_exact(2) {
        let cid = u16::from_be_bytes([pair[0], pair[1]]);
        if let Some(text) = map.and_then(|map| map.get(&cid)) {
            out.push_str(text);
        }
    }
}

/// Reads the `bfchar` entries of a `ToUnicode` CMap.
fn parse_to_unicode(bytes: &[u8]) -> BTreeMap<u16, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut map = BTreeMap::new();
    for section in text.split("beginbfchar").skip(1) {
        let Some(body) = section.split("endbfchar").next() else {
            continue;
        };
        let mut entries = body.split('<').skip(1);
        while let (Some(code), Some(value)) = (entries.next(), entries.next()) {
            let (Some(code), Some(value)) = (code.split('>').next(), value.split('>').next())
            else {
                continue;
            };
            let Ok(cid) = u16::from_str_radix(code.trim(), 16) else {
                continue;
            };
            let units: Vec<u16> = value
                .trim()
                .as_bytes()
                .chunks(4)
                .filter_map(|chunk| u16::from_str_radix(&String::from_utf8_lossy(chunk), 16).ok())
                .collect();
            if let Ok(decoded) = String::from_utf16(&units) {
                map.insert(cid, decoded);
            }
        }
    }
    map
}

/// An object-number-to-offset index and the trailer that named it.
type Xref = (BTreeMap<u32, usize>, BTreeMap<String, Object>);

/// Reads a classic cross-reference table and its trailer.
fn read_xref(
    bytes: &[u8],
    offset: usize,
) -> Result<Xref, String> {
    let mut lexer = Lexer::new(bytes, offset);
    lexer.expect_keyword("xref")?;
    let mut offsets = BTreeMap::new();
    loop {
        lexer.skip_whitespace();
        if lexer.peek_keyword("trailer") {
            break;
        }
        let first = lexer.object()?.number().ok_or("xref subsection start")? as u32;
        let count = lexer.object()?.number().ok_or("xref subsection count")? as usize;
        for index in 0..count {
            lexer.skip_whitespace();
            let at = lexer.object()?.number().ok_or("xref entry offset")? as usize;
            let _generation = lexer.object()?;
            lexer.skip_whitespace();
            let kind = lexer.byte().ok_or("xref entry kind")?;
            if kind == b'n' {
                offsets.insert(first + index as u32, at);
            }
        }
    }
    lexer.expect_keyword("trailer")?;
    let trailer = match lexer.object()? {
        Object::Dict(dict) => dict,
        other => return Err(format!("trailer is not a dictionary: {other:?}")),
    };
    Ok((offsets, trailer))
}

fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

/// A content-stream token: either an object or an operator.
enum Token {
    Object(Object),
    Operator(String),
}

/// A minimal PDF tokenizer.
struct Lexer<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Lexer<'a> {
    fn new(bytes: &'a [u8], at: usize) -> Self {
        Self { bytes, at }
    }

    fn byte(&mut self) -> Option<u8> {
        let byte = self.bytes.get(self.at).copied()?;
        self.at += 1;
        Some(byte)
    }

    fn skip_whitespace(&mut self) {
        while let Some(byte) = self.bytes.get(self.at) {
            if byte.is_ascii_whitespace() || *byte == 0 {
                self.at += 1;
            } else if *byte == b'%' {
                while self
                    .bytes
                    .get(self.at)
                    .is_some_and(|byte| *byte != b'\n' && *byte != b'\r')
                {
                    self.at += 1;
                }
            } else {
                break;
            }
        }
    }

    fn peek_keyword(&mut self, keyword: &str) -> bool {
        self.skip_whitespace();
        self.bytes[self.at.min(self.bytes.len())..].starts_with(keyword.as_bytes())
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<(), String> {
        if !self.peek_keyword(keyword) {
            return Err(format!("expected `{keyword}` at byte {}", self.at));
        }
        self.at += keyword.len();
        Ok(())
    }

    /// Parses one object, resolving `N G R` reference triples.
    fn object(&mut self) -> Result<Object, String> {
        self.skip_whitespace();
        let byte = *self.bytes.get(self.at).ok_or("unexpected end of file")?;
        match byte {
            b'<' if self.bytes.get(self.at + 1) == Some(&b'<') => self.dictionary(),
            b'<' => self.hex_string(),
            b'(' => self.literal_string(),
            b'/' => Ok(Object::Name(self.name())),
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_whitespace();
                    match self.bytes.get(self.at) {
                        None => return Err("unterminated array".to_owned()),
                        Some(b']') => {
                            self.at += 1;
                            break;
                        }
                        Some(_) => items.push(self.object()?),
                    }
                }
                Ok(Object::Array(items))
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => Ok(self.number_or_reference()),
            _ => {
                let word = self.keyword();
                match word.as_str() {
                    "true" => Ok(Object::Bool(true)),
                    "false" => Ok(Object::Bool(false)),
                    "null" => Ok(Object::Null),
                    "" => Err(format!("unreadable object at byte {}", self.at)),
                    other => Ok(Object::Name(other.to_owned())),
                }
            }
        }
    }

    /// Parses the body of an indirect object, attaching stream bytes when the
    /// dictionary is followed by a `stream` keyword.
    fn indirect_body(&mut self) -> Result<Object, String> {
        let object = self.object()?;
        self.skip_whitespace();
        let Object::Dict(dict) = object else {
            return Ok(object);
        };
        if !self.peek_keyword("stream") {
            return Ok(Object::Dict(dict));
        }
        self.at += "stream".len();
        if self.bytes.get(self.at) == Some(&b'\r') {
            self.at += 1;
        }
        if self.bytes.get(self.at) == Some(&b'\n') {
            self.at += 1;
        }
        let length = match dict.get("Length") {
            Some(Object::Number(value)) if *value >= 0.0 => *value as usize,
            _ => return Err("stream has no direct /Length".to_owned()),
        };
        let end = self
            .at
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or("stream /Length runs past the file")?;
        let data = self.bytes[self.at..end].to_vec();
        self.at = end;
        Ok(Object::Stream(dict, data))
    }

    fn dictionary(&mut self) -> Result<Object, String> {
        self.at += 2;
        let mut dict = BTreeMap::new();
        loop {
            self.skip_whitespace();
            if self.bytes.get(self.at) == Some(&b'>') && self.bytes.get(self.at + 1) == Some(&b'>')
            {
                self.at += 2;
                return Ok(Object::Dict(dict));
            }
            if self.bytes.get(self.at) != Some(&b'/') {
                return Err(format!("dictionary key expected at byte {}", self.at));
            }
            let key = self.name();
            let value = self.object()?;
            dict.insert(key, value);
        }
    }

    fn name(&mut self) -> String {
        self.at += 1;
        let mut out = String::new();
        while let Some(byte) = self.bytes.get(self.at).copied() {
            if byte.is_ascii_whitespace()
                || matches!(byte, b'/' | b'[' | b']' | b'<' | b'>' | b'(' | b')' | b'%')
            {
                break;
            }
            self.at += 1;
            if byte == b'#' {
                let hex = self.bytes.get(self.at..self.at + 2).unwrap_or_default();
                if let Ok(value) = u8::from_str_radix(&String::from_utf8_lossy(hex), 16) {
                    out.push(char::from(value));
                    self.at += 2;
                    continue;
                }
            }
            out.push(char::from(byte));
        }
        out
    }

    fn keyword(&mut self) -> String {
        let mut out = String::new();
        while let Some(byte) = self.bytes.get(self.at).copied() {
            if byte.is_ascii_alphabetic() || byte == b'*' || byte == b'\'' || byte == b'"' {
                out.push(char::from(byte));
                self.at += 1;
            } else {
                break;
            }
        }
        if out.is_empty() {
            // Skip the offending byte so a caller cannot spin on it.
            self.at += 1;
        }
        out
    }

    fn number_or_reference(&mut self) -> Object {
        let start = self.at;
        while self
            .bytes
            .get(self.at)
            .is_some_and(|byte| matches!(byte, b'+' | b'-' | b'.' | b'0'..=b'9'))
        {
            self.at += 1;
        }
        let text = String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned();
        let value = text.parse::<f64>().unwrap_or(0.0);
        // Look ahead for the `G R` that turns `N` into a reference.
        let save = self.at;
        if value >= 0.0 && value.fract() == 0.0 {
            self.skip_whitespace();
            let generation_start = self.at;
            while self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
                self.at += 1;
            }
            if self.at > generation_start {
                self.skip_whitespace();
                if self.bytes.get(self.at) == Some(&b'R')
                    && !self
                        .bytes
                        .get(self.at + 1)
                        .is_some_and(|byte| byte.is_ascii_alphanumeric())
                {
                    self.at += 1;
                    return Object::Reference(value as u32);
                }
            }
        }
        self.at = save;
        Object::Number(value)
    }

    fn literal_string(&mut self) -> Result<Object, String> {
        self.at += 1;
        let mut out = Vec::new();
        let mut depth = 1_usize;
        while let Some(byte) = self.byte() {
            match byte {
                b'\\' => {
                    let Some(escaped) = self.byte() else { break };
                    out.push(match escaped {
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        other => other,
                    });
                }
                b'(' => {
                    depth += 1;
                    out.push(byte);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(Object::String(out));
                    }
                    out.push(byte);
                }
                other => out.push(other),
            }
        }
        Err("unterminated literal string".to_owned())
    }

    fn hex_string(&mut self) -> Result<Object, String> {
        self.at += 1;
        let mut digits = String::new();
        while let Some(byte) = self.byte() {
            if byte == b'>' {
                if digits.len() % 2 == 1 {
                    digits.push('0');
                }
                let bytes = digits
                    .as_bytes()
                    .chunks(2)
                    .filter_map(|pair| u8::from_str_radix(&String::from_utf8_lossy(pair), 16).ok())
                    .collect();
                return Ok(Object::String(bytes));
            }
            if byte.is_ascii_hexdigit() {
                digits.push(char::from(byte));
            }
        }
        Err("unterminated hex string".to_owned())
    }

    fn next_content_token(&mut self) -> Option<Token> {
        self.skip_whitespace();
        let byte = *self.bytes.get(self.at)?;
        if matches!(
            byte,
            b'<' | b'(' | b'/' | b'[' | b'+' | b'-' | b'.' | b'0'..=b'9'
        ) {
            return self.object().ok().map(Token::Object);
        }
        if byte == b']' || byte == b'>' || byte == b')' || byte == b'{' || byte == b'}' {
            self.at += 1;
            return self.next_content_token();
        }
        let word = self.keyword();
        if word.is_empty() {
            return self.next_content_token();
        }
        match word.as_str() {
            "true" => Some(Token::Object(Object::Bool(true))),
            "false" => Some(Token::Object(Object::Bool(false))),
            "null" => Some(Token::Object(Object::Null)),
            other => Some(Token::Operator(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lexer_reads_the_object_forms_this_crate_writes() {
        let mut lexer = Lexer::new(b"<</A 1/B[2 3]/C/Name/D 4 0 R/E(text)>>", 0);
        let Object::Dict(dict) = lexer.object().expect("dictionary") else {
            panic!("expected a dictionary");
        };
        assert_eq!(dict["A"], Object::Number(1.0));
        assert_eq!(
            dict["B"],
            Object::Array(vec![Object::Number(2.0), Object::Number(3.0)])
        );
        assert_eq!(dict["C"], Object::Name("Name".to_owned()));
        assert_eq!(dict["D"], Object::Reference(4));
        assert_eq!(dict["E"], Object::String(b"text".to_vec()));
    }

    #[test]
    fn a_to_unicode_map_is_read_back_from_its_bfchar_entries() {
        let cmap = b"1 beginbfchar\n<0024> <0041>\nendbfchar\n";
        let map = parse_to_unicode(cmap);
        assert_eq!(map.get(&0x24).map(String::as_str), Some("A"));
    }

    #[test]
    fn a_truncated_file_is_an_error_not_a_panic() {
        assert!(parse(b"").is_err());
        assert!(parse(b"%PDF-1.7\n").is_err());
        assert!(parse(b"%PDF-1.7\nstartxref\n999999\n%%EOF").is_err());
    }
}
