//! The header tables an RTF body refers to: fonts, colours, lists, metadata,
//! and the picture destination.
//!
//! Each is a small state machine driven by the same control-word stream as the
//! body, kept here so the body dispatcher stays about text.

use std::collections::BTreeMap;

use casual_doc_model::v1::AppProperties;
use casual_doc_model::v1::CoreProperties;
use casual_doc_model::v1::FontFamilyKind;
use casual_doc_model::v1::FontPitch;
use casual_doc_model::v1::LevelJustification;
use casual_doc_model::v1::NumberFormat;
use casual_doc_model::v1::RgbColor;

use crate::limits::enforce;
use crate::{RtfError, RtfLimits};

/// One `\fonttbl` entry.
#[derive(Clone, Debug, Default)]
pub(crate) struct FontEntry {
    /// Typeface name, with the trailing `;` and any `{\*\falt …}` removed.
    pub(crate) name: String,
    /// `\fcharset` value, which overrides `\ansicpg` for text in this font.
    pub(crate) charset: Option<i32>,
    /// Family from `\froman`, `\fswiss`, and friends.
    pub(crate) family: Option<FontFamilyKind>,
    /// Pitch from `\fprq`.
    pub(crate) pitch: Option<FontPitch>,
}

/// Accumulates `\fonttbl` entries.
#[derive(Debug, Default)]
pub(crate) struct FontTable {
    entries: BTreeMap<i32, FontEntry>,
    order: Vec<i32>,
    current_index: Option<i32>,
    current: FontEntry,
}

impl FontTable {
    pub(crate) fn select(&mut self, index: i32, limits: RtfLimits) -> Result<(), RtfError> {
        self.commit(limits)?;
        self.current_index = Some(index);
        Ok(())
    }

    pub(crate) fn set_charset(&mut self, charset: i32) {
        self.current.charset = Some(charset);
    }

    pub(crate) fn set_family(&mut self, family: FontFamilyKind) {
        self.current.family = Some(family);
    }

    pub(crate) fn set_pitch(&mut self, pitch: FontPitch) {
        self.current.pitch = Some(pitch);
    }

    pub(crate) fn push_name(&mut self, text: &str) {
        // Names are bounded by the model's own 255-byte font-name domain; stop
        // appending past it rather than building a string only to be rejected.
        for character in text.chars() {
            if self.current.name.len() + character.len_utf8() > 255 {
                break;
            }
            self.current.name.push(character);
        }
    }

    /// Commits the entry being built, if any. Called on `;` and on group end.
    pub(crate) fn commit(&mut self, limits: RtfLimits) -> Result<(), RtfError> {
        let Some(index) = self.current_index.take() else {
            self.current = FontEntry::default();
            return Ok(());
        };
        let mut entry = std::mem::take(&mut self.current);
        entry.name = entry.name.trim().trim_end_matches(';').trim().to_owned();
        if !self.entries.contains_key(&index) {
            enforce("rtf_fonts", self.entries.len() + 1, limits.max_fonts)?;
            self.order.push(index);
        }
        self.entries.insert(index, entry);
        Ok(())
    }

    pub(crate) fn get(&self, index: i32) -> Option<&FontEntry> {
        self.entries.get(&index)
    }

    /// Entries in declaration order, for `Definitions.font_table`.
    pub(crate) fn in_declaration_order(&self) -> impl Iterator<Item = &FontEntry> {
        self.order
            .iter()
            .filter_map(|index| self.entries.get(index))
    }
}

/// Accumulates `\colortbl` entries. Index 0 is conventionally "auto".
#[derive(Debug, Default)]
pub(crate) struct ColorTable {
    entries: Vec<Option<RgbColor>>,
    red: Option<u8>,
    green: Option<u8>,
    blue: Option<u8>,
}

impl ColorTable {
    pub(crate) fn set_component(&mut self, component: u8, value: i32) {
        let clamped = value.clamp(0, 255) as u8;
        match component {
            b'r' => self.red = Some(clamped),
            b'g' => self.green = Some(clamped),
            _ => self.blue = Some(clamped),
        }
    }

    /// Commits one entry at a `;` separator.
    ///
    /// An entry with no component at all is RTF's "auto" colour — the leading
    /// `;` in `{\colortbl ;\red0…}` — and must stay distinguishable from black,
    /// or every automatic-coloured run would import as explicitly black and
    /// stop following the theme.
    pub(crate) fn commit(&mut self, limits: RtfLimits) -> Result<(), RtfError> {
        enforce("rtf_colors", self.entries.len() + 1, limits.max_colors)?;
        let color = match (self.red.take(), self.green.take(), self.blue.take()) {
            (None, None, None) => None,
            (red, green, blue) => Some(RgbColor {
                r: red.unwrap_or(0),
                g: green.unwrap_or(0),
                b: blue.unwrap_or(0),
            }),
        };
        self.entries.push(color);
        Ok(())
    }

    pub(crate) fn get(&self, index: i32) -> Option<Option<RgbColor>> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.entries.get(index))
            .copied()
    }
}

/// One `\listlevel` in a `\listtable` definition.
#[derive(Clone, Debug, Default)]
pub(crate) struct ListLevelDefinition {
    pub(crate) number_format: Option<NumberFormat>,
    pub(crate) start_at: u16,
    pub(crate) justification: Option<LevelJustification>,
    /// Decoded `\leveltext`: a length-prefix character, then a mix of
    /// placeholder indices (`\'00`..`\'08`) and literal characters.
    pub(crate) level_text: String,
}

/// One `{\list …}` definition.
#[derive(Clone, Debug, Default)]
pub(crate) struct ListDefinition {
    pub(crate) list_id: Option<i32>,
    pub(crate) levels: Vec<ListLevelDefinition>,
}

/// `\listtable` plus `\listoverridetable`.
#[derive(Debug, Default)]
pub(crate) struct ListTables {
    pub(crate) definitions: Vec<ListDefinition>,
    /// `\ls` number → `\listid`.
    pub(crate) overrides: BTreeMap<i32, i32>,
    current_list: Option<ListDefinition>,
    current_level: Option<ListLevelDefinition>,
    current_override_list_id: Option<i32>,
    current_override_ls: Option<i32>,
}

impl ListTables {
    pub(crate) fn begin_list(&mut self, limits: RtfLimits) -> Result<(), RtfError> {
        self.end_list(limits)?;
        self.current_list = Some(ListDefinition::default());
        Ok(())
    }

    pub(crate) fn end_list(&mut self, limits: RtfLimits) -> Result<(), RtfError> {
        self.end_level();
        if let Some(list) = self.current_list.take() {
            enforce(
                "rtf_list_definitions",
                self.definitions.len() + 1,
                limits.max_list_definitions,
            )?;
            self.definitions.push(list);
        }
        Ok(())
    }

    pub(crate) fn begin_level(&mut self) {
        self.end_level();
        self.current_level = Some(ListLevelDefinition {
            start_at: 1,
            ..ListLevelDefinition::default()
        });
    }

    pub(crate) fn end_level(&mut self) {
        let Some(level) = self.current_level.take() else {
            return;
        };
        if let Some(list) = self.current_list.as_mut()
            && list.levels.len() < 9
        {
            list.levels.push(level);
        }
    }

    pub(crate) fn level(&mut self) -> Option<&mut ListLevelDefinition> {
        self.current_level.as_mut()
    }

    pub(crate) fn set_list_id(&mut self, id: i32) {
        if let Some(list) = self.current_list.as_mut() {
            list.list_id = Some(id);
        }
    }

    pub(crate) fn begin_override(&mut self) {
        self.end_override();
        self.current_override_list_id = None;
        self.current_override_ls = None;
    }

    pub(crate) fn set_override_list_id(&mut self, id: i32) {
        self.current_override_list_id = Some(id);
    }

    pub(crate) fn set_override_ls(&mut self, ls: i32) {
        self.current_override_ls = Some(ls);
    }

    pub(crate) fn end_override(&mut self) {
        if let (Some(ls), Some(list_id)) = (
            self.current_override_ls.take(),
            self.current_override_list_id.take(),
        ) {
            self.overrides.entry(ls).or_insert(list_id);
        }
    }

    /// Resolves a body `\lsN` to the `\listtable` definition it names.
    pub(crate) fn resolve(&self, ls: i32) -> Option<(usize, &ListDefinition)> {
        let list_id = self.overrides.get(&ls).copied()?;
        self.definitions
            .iter()
            .enumerate()
            .find(|(_, definition)| definition.list_id == Some(list_id))
    }
}

/// `\levelnfc` → the normalized number format.
#[must_use]
pub(crate) fn level_number_format(nfc: i32) -> Option<NumberFormat> {
    Some(match nfc {
        0 => NumberFormat::Decimal,
        1 => NumberFormat::UpperRoman,
        2 => NumberFormat::LowerRoman,
        3 => NumberFormat::UpperLetter,
        4 => NumberFormat::LowerLetter,
        5 => NumberFormat::Ordinal,
        6 => NumberFormat::CardinalText,
        7 => NumberFormat::OrdinalText,
        22 => NumberFormat::DecimalZero,
        23 => NumberFormat::Bullet,
        255 => NumberFormat::None,
        _ => return None,
    })
}

/// Which `\info` field the current destination is collecting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InfoField {
    /// `\title`
    Title,
    /// `\subject`
    Subject,
    /// `\author`
    Author,
    /// `\operator` — maps to `cp:lastModifiedBy`.
    Operator,
    /// `\keywords`
    Keywords,
    /// `\doccomm` — maps to `dc:description`.
    Comment,
    /// `\category`
    Category,
    /// `\company`
    Company,
    /// `\manager`
    Manager,
    /// `\*\generator` — maps to the app-properties application name.
    Generator,
}

/// Accumulates `{\info …}` and `{\*\generator …}` into document metadata.
#[derive(Debug, Default)]
pub(crate) struct InfoCollector {
    pub(crate) core: CoreProperties,
    pub(crate) app: AppProperties,
    buffer: String,
    field: Option<InfoField>,
}

impl InfoCollector {
    pub(crate) fn begin(&mut self, field: InfoField) {
        self.finish_field();
        self.field = Some(field);
    }

    pub(crate) fn push(&mut self, text: &str) {
        if self.field.is_none() {
            return;
        }
        for character in text.chars() {
            // The model bounds every metadata string; stop at the bound rather
            // than build a megabyte string a hostile `\info` could supply.
            if self.buffer.len() + character.len_utf8() > 255 {
                break;
            }
            self.buffer.push(character);
        }
    }

    pub(crate) fn finish_field(&mut self) {
        let Some(field) = self.field.take() else {
            self.buffer.clear();
            return;
        };
        // Producers terminate `\*\generator` with a `;` that is a separator,
        // not part of the name ("Riched20 10.0;").
        let value = std::mem::take(&mut self.buffer)
            .trim()
            .trim_end_matches(';')
            .trim()
            .to_owned();
        if value.is_empty() {
            return;
        }
        match field {
            InfoField::Title => self.core.title = Some(value),
            InfoField::Subject => self.core.subject = Some(value),
            InfoField::Author => self.core.creator = Some(value),
            InfoField::Operator => self.core.last_modified_by = Some(value),
            InfoField::Keywords => self.core.keywords = Some(value),
            InfoField::Comment => self.core.description = Some(value),
            InfoField::Category => self.core.category = Some(value),
            InfoField::Company => self.app.company = Some(value),
            InfoField::Manager => self.app.manager = Some(value),
            InfoField::Generator => self.app.application = Some(value),
        }
    }
}

/// Which binary form a `\pict` carries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PictureKind {
    /// `\pngblip`
    Png,
    /// `\jpegblip`
    Jpeg,
    /// `\emfblip`
    Emf,
    /// `\wmetafile`
    Wmf,
    /// `\dibitmap` / `\wbitmap`
    Bitmap,
}

impl PictureKind {
    pub(crate) const fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Emf => "image/x-emf",
            Self::Wmf => "image/x-wmf",
            Self::Bitmap => "image/bmp",
        }
    }

    pub(crate) const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Emf => "emf",
            Self::Wmf => "wmf",
            Self::Bitmap => "bmp",
        }
    }

    /// Whether the renderer can actually draw this form today.
    pub(crate) const fn is_renderable(self) -> bool {
        matches!(self, Self::Png | Self::Jpeg)
    }
}

/// Accumulates one `{\pict …}` destination.
#[derive(Debug, Default)]
pub(crate) struct PictureState {
    pub(crate) kind: Option<PictureKind>,
    pub(crate) goal_width_twips: i32,
    pub(crate) goal_height_twips: i32,
    pub(crate) raw_width: i32,
    pub(crate) raw_height: i32,
    pub(crate) scale_x_percent: i32,
    pub(crate) scale_y_percent: i32,
    bytes: Vec<u8>,
    half_byte: Option<u8>,
}

impl PictureState {
    pub(crate) fn new() -> Self {
        Self {
            scale_x_percent: 100,
            scale_y_percent: 100,
            ..Self::default()
        }
    }

    /// Accumulates hex-encoded payload text.
    pub(crate) fn push_hex(&mut self, text: &[u8], limits: RtfLimits) -> Result<(), RtfError> {
        for byte in text {
            let Some(value) = hex_value(*byte) else {
                // Whitespace and line wrapping inside the payload are normal;
                // anything else is not, and is skipped rather than folded into
                // the image as a wrong nibble.
                continue;
            };
            match self.half_byte.take() {
                None => self.half_byte = Some(value),
                Some(high) => {
                    enforce(
                        "rtf_picture_bytes",
                        self.bytes.len() + 1,
                        limits.max_picture_bytes,
                    )?;
                    self.bytes.push((high << 4) | value);
                }
            }
        }
        Ok(())
    }

    /// Accumulates a `\binN` payload.
    pub(crate) fn push_binary(
        &mut self,
        payload: &[u8],
        limits: RtfLimits,
    ) -> Result<(), RtfError> {
        enforce(
            "rtf_picture_bytes",
            self.bytes.len() + payload.len(),
            limits.max_picture_bytes,
        )?;
        self.bytes.extend_from_slice(payload);
        Ok(())
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.bytes.len()
    }
}

const fn hex_value(byte: u8) -> Option<u8> {
    Some(match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => return None,
    })
}
