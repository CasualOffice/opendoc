//! Control-word dispatch.
//!
//! Three buckets, and every control word is in exactly one:
//!
//! 1. **mapped** — it changes the normalized model and produces no finding;
//! 2. **consumed** — it carries no content this model can lose (`\rtf`,
//!    `\ansi`, `\intbl`), so it produces no finding either;
//! 3. **everything else** — reported, because a construct that vanishes
//!    without a finding is silent data loss.

use super::*;

/// Whether a border control word is decorating a cell or a paragraph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BorderTarget {
    Cell,
    Paragraph,
}

impl Importer<'_> {
    pub(super) fn control_word(
        &mut self,
        name: &str,
        parameter: Option<i32>,
        at_group_start: bool,
    ) -> Result<(), RtfError> {
        // `\bin` is lexical before it is semantic: its payload must leave the
        // token stream whatever destination we are in, or the raw bytes get
        // re-lexed as control words.
        if name == "bin" {
            return self.binary_payload(parameter);
        }
        if at_group_start {
            if self.begin_destination(name)? {
                self.next_is_destination = false;
                return Ok(());
            }
            if std::mem::take(&mut self.next_is_destination) {
                self.state_mut().destination = Destination::Discard;
                return self.losses.record_named(
                    "rtf.destination.ignored",
                    Some(name),
                    RtfModelOutcome::Omitted,
                    self.limits,
                );
            }
        }
        match self.state().destination {
            Destination::Discard | Destination::LevelText => Ok(()),
            Destination::Picture => self.picture_word(name, parameter),
            Destination::FontTable => self.font_table_word(name, parameter),
            Destination::ColorTable => self.color_table_word(name, parameter),
            Destination::ListTable | Destination::ListEntry | Destination::ListLevel => {
                self.list_table_word(name, parameter)
            }
            Destination::ListOverrideTable | Destination::ListOverrideEntry => {
                self.list_override_word(name, parameter)
            }
            Destination::Info | Destination::Generator => Ok(()),
            Destination::Body => self.body_word(name, parameter),
        }
    }

    /// Opens a destination when `name` names one. Returns whether it did.
    fn begin_destination(&mut self, name: &str) -> Result<bool, RtfError> {
        if let Some(field) = info_field(name) {
            self.info.begin(field);
            self.state_mut().destination = Destination::Info;
            return Ok(true);
        }
        let destination = match name {
            "fonttbl" => Destination::FontTable,
            "colortbl" => Destination::ColorTable,
            "info" => Destination::Info,
            "generator" => {
                self.info.begin(InfoField::Generator);
                Destination::Generator
            }
            "pict" => {
                self.picture_count = self.picture_count.saturating_add(1);
                enforce("rtf_pictures", self.picture_count, self.limits.max_pictures)?;
                self.picture = Some(PictureState::new());
                Destination::Picture
            }
            "listtable" => Destination::ListTable,
            "listoverridetable" => Destination::ListOverrideTable,
            // `\shppict` wraps the picture a Word-compatible reader should use;
            // it is a container, not a destination, so body routing continues.
            "shppict" => return Ok(true),
            // `\nonshppict` is the *duplicate* of the `\shppict` above it,
            // provided for readers that cannot read the first. Dropping it
            // loses nothing, which is why it is the one discard here with no
            // finding attached.
            "nonshppict" => {
                self.state_mut().destination = Destination::Discard;
                return Ok(true);
            }
            // `\fldrslt` holds the field's visible text; routing it to the body
            // is what keeps a field's content out of the loss column.
            "fldrslt" => Destination::Body,
            _ => {
                if let Some(feature) = reported_discard(name) {
                    self.state_mut().destination = Destination::Discard;
                    self.losses
                        .record(feature, RtfModelOutcome::Omitted, self.limits)?;
                    return Ok(true);
                }
                return Ok(false);
            }
        };
        self.state_mut().destination = destination;
        Ok(true)
    }

    fn binary_payload(&mut self, parameter: Option<i32>) -> Result<(), RtfError> {
        let length = usize::try_from(parameter.unwrap_or(0).max(0)).unwrap_or(0);
        // The slice is borrowed from the already-admitted input, so a
        // `\bin2000000000` in a small file fails on the bounds check without
        // ever allocating.
        let payload = self.lexer.take_binary(length)?;
        if self.state().destination == Destination::Picture {
            let limits = self.limits;
            let total = self.picture_bytes_total.saturating_add(payload.len());
            enforce(
                "rtf_total_picture_bytes",
                total,
                limits.max_total_picture_bytes,
            )?;
            if let Some(picture) = self.picture.as_mut() {
                picture.push_binary(payload, limits)?;
            }
            return Ok(());
        }
        if length > 0 {
            self.losses
                .record("rtf.binary-data", RtfModelOutcome::Omitted, self.limits)?;
        }
        Ok(())
    }

    // ---- header tables -------------------------------------------------

    fn font_table_word(&mut self, name: &str, parameter: Option<i32>) -> Result<(), RtfError> {
        match name {
            "f" => self.fonts.select(parameter.unwrap_or(0), self.limits)?,
            "fcharset" => self.fonts.set_charset(parameter.unwrap_or(0)),
            "froman" => self.fonts.set_family(FontFamilyKind::Roman),
            "fswiss" => self.fonts.set_family(FontFamilyKind::Swiss),
            "fmodern" => self.fonts.set_family(FontFamilyKind::Modern),
            "fscript" => self.fonts.set_family(FontFamilyKind::Script),
            "fdecor" => self.fonts.set_family(FontFamilyKind::Decorative),
            "fnil" => self.fonts.set_family(FontFamilyKind::Auto),
            "fprq" => self.fonts.set_pitch(match parameter.unwrap_or(0) {
                1 => FontPitch::Fixed,
                2 => FontPitch::Variable,
                _ => FontPitch::Default,
            }),
            _ => {}
        }
        Ok(())
    }

    fn color_table_word(&mut self, name: &str, parameter: Option<i32>) -> Result<(), RtfError> {
        match name {
            "red" => self.colors.set_component(b'r', parameter.unwrap_or(0)),
            "green" => self.colors.set_component(b'g', parameter.unwrap_or(0)),
            "blue" => self.colors.set_component(b'b', parameter.unwrap_or(0)),
            _ => {}
        }
        Ok(())
    }

    fn list_table_word(&mut self, name: &str, parameter: Option<i32>) -> Result<(), RtfError> {
        match name {
            "list" => {
                self.lists.begin_list(self.limits)?;
                self.state_mut().destination = Destination::ListEntry;
            }
            "listlevel" => {
                self.lists.begin_level();
                self.state_mut().destination = Destination::ListLevel;
            }
            "leveltext" => self.state_mut().destination = Destination::LevelText,
            "listid" => self.lists.set_list_id(parameter.unwrap_or(0)),
            "levelnfc" | "levelnfcn" => {
                let format = level_number_format(parameter.unwrap_or(0));
                if let Some(level) = self.lists.level() {
                    level.number_format = format;
                }
            }
            "levelstartat" => {
                let start = parameter.unwrap_or(1).clamp(0, 32_767) as u16;
                if let Some(level) = self.lists.level() {
                    level.start_at = start;
                }
            }
            "leveljc" | "leveljcn" => {
                let justification = match parameter.unwrap_or(0) {
                    1 => LevelJustification::Center,
                    2 => LevelJustification::End,
                    _ => LevelJustification::Start,
                };
                if let Some(level) = self.lists.level() {
                    level.justification = Some(justification);
                }
            }
            "levelnumbers" | "listname" | "listpicture" => {
                self.state_mut().destination = Destination::Discard;
            }
            _ => {}
        }
        Ok(())
    }

    fn list_override_word(&mut self, name: &str, parameter: Option<i32>) -> Result<(), RtfError> {
        match name {
            "listoverride" => {
                self.lists.begin_override();
                self.state_mut().destination = Destination::ListOverrideEntry;
            }
            "listid" => self.lists.set_override_list_id(parameter.unwrap_or(0)),
            "ls" => self.lists.set_override_ls(parameter.unwrap_or(0)),
            "lfolevel" => self.state_mut().destination = Destination::Discard,
            _ => {}
        }
        Ok(())
    }

    fn picture_word(&mut self, name: &str, parameter: Option<i32>) -> Result<(), RtfError> {
        let Some(picture) = self.picture.as_mut() else {
            return Ok(());
        };
        match name {
            "pngblip" => picture.kind = Some(PictureKind::Png),
            "jpegblip" => picture.kind = Some(PictureKind::Jpeg),
            "emfblip" => picture.kind = Some(PictureKind::Emf),
            "wmetafile" => picture.kind = Some(PictureKind::Wmf),
            "dibitmap" | "wbitmap" => picture.kind = Some(PictureKind::Bitmap),
            "picw" => picture.raw_width = parameter.unwrap_or(0),
            "pich" => picture.raw_height = parameter.unwrap_or(0),
            "picwgoal" => picture.goal_width_twips = parameter.unwrap_or(0),
            "pichgoal" => picture.goal_height_twips = parameter.unwrap_or(0),
            "picscalex" => picture.scale_x_percent = parameter.unwrap_or(100),
            "picscaley" => picture.scale_y_percent = parameter.unwrap_or(100),
            _ => {}
        }
        Ok(())
    }

    // ---- body ----------------------------------------------------------

    #[allow(clippy::too_many_lines)]
    fn body_word(&mut self, name: &str, parameter: Option<i32>) -> Result<(), RtfError> {
        match name {
            // -- header facts ------------------------------------------
            "rtf" => {
                self.version = parameter.unwrap_or(1);
                if self.version != 1 {
                    self.losses.record(
                        "rtf.version.unexpected",
                        RtfModelOutcome::Mapped,
                        self.limits,
                    )?;
                }
            }
            "ansi" => self.default_code_page = 1252,
            "mac" => self.default_code_page = 10_000,
            "pc" => self.default_code_page = 437,
            "pca" => self.default_code_page = 850,
            "ansicpg" => {
                let number = u16::try_from(parameter.unwrap_or(1252).max(0)).unwrap_or(1252);
                self.declared_code_page = Some(number);
            }
            "deff" => self.default_font = parameter,

            // -- Unicode ------------------------------------------------
            "uc" => {
                let skip = u32::try_from(parameter.unwrap_or(1).max(0)).unwrap_or(1);
                self.state_mut().unicode_skip = skip;
            }
            "u" => self.unicode_escape(parameter.unwrap_or(0))?,

            // -- literal glyphs -----------------------------------------
            "tab" => {
                let inline = InlineNode::Tab(Tab {
                    id: next_id(&mut self.ids)?,
                });
                self.push_inline(inline)?;
            }
            "line" => self.push_break(BreakKind::Line)?,
            "page" => self.push_break(BreakKind::Page)?,
            "column" => self.push_break(BreakKind::Column)?,
            "emspace" => self.push_char('\u{2003}')?,
            "enspace" => self.push_char('\u{2002}')?,
            "qmspace" => self.push_char('\u{2005}')?,
            "bullet" => self.push_char('\u{2022}')?,
            "endash" => self.push_char('\u{2013}')?,
            "emdash" => self.push_char('\u{2014}')?,
            "lquote" => self.push_char('\u{2018}')?,
            "rquote" => self.push_char('\u{2019}')?,
            "ldblquote" => self.push_char('\u{201c}')?,
            "rdblquote" => self.push_char('\u{201d}')?,
            "zwj" => self.push_char('\u{200d}')?,
            "zwnj" => self.push_char('\u{200c}')?,
            "ltrmark" => self.push_char('\u{200e}')?,
            "rtlmark" => self.push_char('\u{200f}')?,

            // -- paragraphs ---------------------------------------------
            "par" => self.end_paragraph()?,
            "pard" => self.reset_paragraph(),
            "ql" => self.state_mut().paragraph.alignment = Some(Alignment::Start),
            "qc" => self.state_mut().paragraph.alignment = Some(Alignment::Center),
            "qr" => self.state_mut().paragraph.alignment = Some(Alignment::End),
            "qj" => self.state_mut().paragraph.alignment = Some(Alignment::Justify),
            "li" | "lin" => self.indentation().start_twips = clamp_indent(parameter),
            "ri" | "rin" => self.indentation().end_twips = clamp_indent(parameter),
            "fi" => {
                let value = parameter.unwrap_or(0).clamp(-31_680, 31_680);
                let indentation = self.indentation();
                if value < 0 {
                    indentation.hanging_twips = Some(-value);
                    indentation.first_line_twips = None;
                } else {
                    indentation.first_line_twips = Some(value);
                    indentation.hanging_twips = None;
                }
            }
            "sb" => self.spacing().before_twips = Some(parameter.unwrap_or(0).clamp(0, 31_680)),
            "sa" => self.spacing().after_twips = Some(parameter.unwrap_or(0).clamp(0, 31_680)),
            "sbauto" => self.spacing().before_auto = Some(parameter.unwrap_or(1) != 0),
            "saauto" => self.spacing().after_auto = Some(parameter.unwrap_or(1) != 0),
            "sl" => {
                self.line_spacing_value = Some(parameter.unwrap_or(0));
                self.apply_line_spacing();
            }
            "slmult" => {
                self.line_spacing_multiple = parameter.unwrap_or(0) != 0;
                self.apply_line_spacing();
            }
            "keep" => self.state_mut().paragraph.keep_lines = true,
            "keepn" => self.state_mut().paragraph.keep_next = true,
            "pagebb" => self.state_mut().paragraph.page_break_before = true,
            "widctlpar" => self.state_mut().paragraph.widow_control = Some(true),
            "nowidctlpar" => self.state_mut().paragraph.widow_control = Some(false),
            "noline" => self.state_mut().paragraph.suppress_line_numbers = true,
            "contextualspace" => self.state_mut().paragraph.contextual_spacing = true,
            "outlinelevel" => {
                let level = u8::try_from(parameter.unwrap_or(0).clamp(0, 9)).unwrap_or(0);
                self.state_mut().paragraph.outline_level = Some(level);
            }
            "rtlpar" => self.state_mut().paragraph.bidi = Some(true),
            "ltrpar" => self.state_mut().paragraph.bidi = Some(false),

            // -- tab stops ----------------------------------------------
            "tqr" => self.pending_tab_alignment = TabAlignment::End,
            "tqc" => self.pending_tab_alignment = TabAlignment::Center,
            "tqdec" => self.pending_tab_alignment = TabAlignment::Decimal,
            "tldot" => self.pending_tab_leader = Some(TabLeader::Dot),
            "tlhyph" => self.pending_tab_leader = Some(TabLeader::Hyphen),
            "tlul" => self.pending_tab_leader = Some(TabLeader::Underscore),
            "tlth" => self.pending_tab_leader = Some(TabLeader::Heavy),
            "tleq" => self.pending_tab_leader = Some(TabLeader::MiddleDot),
            "tx" | "tb" => self.push_tab_stop(name, parameter),

            // -- runs ----------------------------------------------------
            "plain" => self.state_mut().run = RunProperties::default(),
            "b" => self.state_mut().run.bold = Some(flag(parameter)),
            "i" => self.state_mut().run.italic = Some(flag(parameter)),
            "strike" => self.state_mut().run.strike = Some(flag(parameter)),
            "striked" => self.state_mut().run.double_strike = Some(flag(parameter)),
            "caps" => self.state_mut().run.all_caps = Some(flag(parameter)),
            "scaps" => self.state_mut().run.small_caps = Some(flag(parameter)),
            "v" => self.state_mut().run.hidden = Some(flag(parameter)),
            "outl" => self.state_mut().run.outline = Some(flag(parameter)),
            "shad" => self.state_mut().run.shadow = Some(flag(parameter)),
            "embo" => self.state_mut().run.emboss = Some(flag(parameter)),
            "impr" => self.state_mut().run.imprint = Some(flag(parameter)),
            "rtlch" => self.state_mut().run.rtl = Some(true),
            "ltrch" => self.state_mut().run.rtl = Some(false),
            "nosupersub" => self.state_mut().run.vertical_alignment = None,
            "super" => {
                self.state_mut().run.vertical_alignment = Some(VerticalAlignment::Superscript);
            }
            "sub" => {
                self.state_mut().run.vertical_alignment = Some(VerticalAlignment::Subscript);
            }
            "up" => {
                let value = parameter.unwrap_or(6).clamp(-31_680, 31_680);
                self.state_mut().run.position_half_points = Some(value);
            }
            "dn" => {
                let value = parameter.unwrap_or(6).clamp(-31_680, 31_680);
                self.state_mut().run.position_half_points = Some(-value);
            }
            "fs" => {
                let size = parameter.unwrap_or(24).clamp(1, 65_534);
                self.state_mut().run.size_half_points = Some(u32::try_from(size).unwrap_or(24));
            }
            "expnd" | "expndtw" => {
                // `\expnd` is quarter-points, `\expndtw` twips; the model wants
                // twips, so only the twip form maps exactly.
                let twips = if name == "expndtw" {
                    parameter.unwrap_or(0)
                } else {
                    parameter.unwrap_or(0).saturating_mul(5)
                };
                self.state_mut().run.character_spacing_twips = Some(twips.clamp(-31_680, 31_680));
            }
            "charscalex" => {
                let percent = parameter.unwrap_or(100).clamp(1, 600);
                self.state_mut().run.character_scale_percent =
                    Some(u16::try_from(percent).unwrap_or(100));
            }
            "kerning" => {
                let value = parameter.unwrap_or(0).clamp(0, 65_534);
                self.state_mut().run.kerning_half_points = Some(u32::try_from(value).unwrap_or(0));
            }
            "f" => self.select_font(parameter.unwrap_or(0)),
            "cf" => self.select_color(parameter.unwrap_or(0)),
            "highlight" | "chcbpat" | "cb" => {
                self.select_highlight(name, parameter.unwrap_or(0))?
            }
            "ulnone" => {
                let run = &mut self.state_mut().run;
                run.underline = Some(false);
                run.underline_style = None;
            }
            "ulc" => {
                let color = self.colors.get(parameter.unwrap_or(0)).flatten();
                self.state_mut().run.underline_color = color;
            }

            // -- sections -------------------------------------------------
            "sect" => self.end_section()?,
            "sectd" => self.setup = SectionSetup::default(),
            "paperw" => self.declare_page(|setup, value| setup.width_twips = value, parameter),
            "paperh" => self.declare_page(|setup, value| setup.height_twips = value, parameter),
            "margl" => self.declare_page(|setup, value| setup.left_twips = value, parameter),
            "margr" => self.declare_page(|setup, value| setup.right_twips = value, parameter),
            "margt" => self.declare_page(|setup, value| setup.top_twips = value, parameter),
            "margb" => self.declare_page(|setup, value| setup.bottom_twips = value, parameter),
            "headery" => {
                self.declare_page(|setup, value| setup.header_twips = Some(value), parameter);
            }
            "footery" => {
                self.declare_page(|setup, value| setup.footer_twips = Some(value), parameter);
            }
            "gutter" => {
                self.declare_page(|setup, value| setup.gutter_twips = Some(value), parameter);
            }
            "landscape" | "lndscpsxn" => {
                self.setup.landscape = true;
                self.setup.declared = true;
            }
            "cols" => {
                let count = parameter.unwrap_or(1).clamp(1, 64);
                self.declare_page(
                    |setup, value| setup.column_count = u16::try_from(value).unwrap_or(1),
                    Some(count),
                );
            }
            "colsx" => {
                self.declare_page(|setup, value| setup.column_space = Some(value), parameter)
            }
            "linebetcol" => {
                self.setup.column_separator = true;
                self.setup.declared = true;
            }

            // -- lists ----------------------------------------------------
            "ls" => {
                self.current_ls = parameter;
                self.apply_numbering()?;
            }
            "ilvl" => {
                self.current_ilvl = u8::try_from(parameter.unwrap_or(0).clamp(0, 8)).unwrap_or(0);
                self.apply_numbering()?;
            }

            // -- tables ---------------------------------------------------
            "trowd" => self.reset_row(),
            "cellx" => self.push_cell_definition(parameter.unwrap_or(0)),
            "cell" => self.end_cell()?,
            "nestcell" | "nestrow" | "nesttableprops" => {
                self.losses
                    .record("rtf.table.nested", RtfModelOutcome::Omitted, self.limits)?;
            }
            "row" => self.end_row()?,
            "trleft" => self.row_left_twips = parameter.unwrap_or(0),
            "trhdr" => self.row_properties.header = true,
            "trkeep" => self.row_properties.cant_split = true,
            "trql" => self.row_properties.alignment = Some(Alignment::Start),
            "trqc" => self.row_properties.alignment = Some(Alignment::Center),
            "trqr" => self.row_properties.alignment = Some(Alignment::End),
            "trrh" => {
                let value = parameter.unwrap_or(0);
                self.row_properties.height = casual_doc_model::v1::RowHeight {
                    value_twips: Some(u32::try_from(value.abs().min(31_680)).unwrap_or(0)),
                    rule: Some(if value < 0 {
                        casual_doc_model::v1::HeightRule::Exact
                    } else {
                        casual_doc_model::v1::HeightRule::AtLeast
                    }),
                };
            }
            // `\clmgf` only announces that the *next* `\clmrg` cells belong
            // to it. The fold is driven entirely by `\clmrg`, so there is
            // nothing to record here — but it must still be consumed, or it
            // would be reported as an unknown control word.
            "tcelld" => {
                self.pending_cell = TableCellProperties::default();
                self.pending_border = None;
            }
            "trgaph" | "trpaddl" | "trpaddr" | "trpaddt" | "trpaddb" | "trpaddfl" | "trpaddfr"
            | "trpaddft" | "trpaddfb" => {
                self.losses.record(
                    "rtf.table.cell-padding",
                    RtfModelOutcome::Omitted,
                    self.limits,
                )?;
            }
            "clmgf" => {}
            "clmrg" => self.pending_merge_continue = true,
            "clvmgf" => {
                self.pending_cell.vertical_merge =
                    Some(casual_doc_model::v1::VerticalMerge::Restart);
            }
            "clvmrg" => {
                self.pending_cell.vertical_merge =
                    Some(casual_doc_model::v1::VerticalMerge::Continue);
            }
            "clvertalt" => {
                self.pending_cell.vertical_alignment =
                    Some(casual_doc_model::v1::CellVerticalAlignment::Top);
            }
            "clvertalc" => {
                self.pending_cell.vertical_alignment =
                    Some(casual_doc_model::v1::CellVerticalAlignment::Center);
            }
            "clvertalb" => {
                self.pending_cell.vertical_alignment =
                    Some(casual_doc_model::v1::CellVerticalAlignment::Bottom);
            }
            "clcbpat" => {
                let fill = self.colors.get(parameter.unwrap_or(0)).flatten();
                self.pending_cell.shading.fill = fill;
            }
            "clNoWrap" | "clnowrap" => self.pending_cell.no_wrap = true,
            "clbrdrt" => self.open_border(BorderTarget::Cell, BorderEdgeSlot::Top),
            "clbrdrl" => self.open_border(BorderTarget::Cell, BorderEdgeSlot::Start),
            "clbrdrb" => self.open_border(BorderTarget::Cell, BorderEdgeSlot::Bottom),
            "clbrdrr" => self.open_border(BorderTarget::Cell, BorderEdgeSlot::End),
            "brdrt" => self.open_border(BorderTarget::Paragraph, BorderEdgeSlot::Top),
            "brdrl" => self.open_border(BorderTarget::Paragraph, BorderEdgeSlot::Start),
            "brdrb" => self.open_border(BorderTarget::Paragraph, BorderEdgeSlot::Bottom),
            "brdrr" => self.open_border(BorderTarget::Paragraph, BorderEdgeSlot::End),
            "brdrw" => self.set_border_width(parameter.unwrap_or(0)),
            "brdrcf" => {
                let color = self.colors.get(parameter.unwrap_or(0)).flatten();
                self.set_border_color(color);
            }

            // -- reported, not modeled -------------------------------------
            "field" => {
                self.losses.record(
                    "rtf.field.instruction",
                    RtfModelOutcome::Degraded,
                    self.limits,
                )?;
            }
            "s" | "cs" | "ds" | "ts" => {
                self.losses
                    .record("rtf.stylesheet", RtfModelOutcome::Omitted, self.limits)?;
            }
            "pn" | "pntext" | "pnseclvl" => {
                self.losses
                    .record("rtf.list.legacy-pn", RtfModelOutcome::Omitted, self.limits)?;
            }
            "revised" | "deleted" | "revauth" | "revdttm" => {
                self.losses
                    .record("rtf.revision", RtfModelOutcome::Omitted, self.limits)?;
            }
            "chftn" | "chatn" | "chpgn" | "chdate" | "chtime" | "sectnum" => {
                self.losses.record(
                    "rtf.field.instruction",
                    RtfModelOutcome::Degraded,
                    self.limits,
                )?;
            }
            "box" | "brdrbtw" | "brdrbar" => {
                self.losses.record(
                    "rtf.paragraph.border",
                    RtfModelOutcome::Omitted,
                    self.limits,
                )?;
            }
            "absw" | "absh" | "posx" | "posy" | "phpg" | "pvpg" | "dxfrtext" => {
                self.losses
                    .record("rtf.frame", RtfModelOutcome::Omitted, self.limits)?;
            }
            "lang" | "langfe" | "langnp" | "langfenp" | "alang" => {
                self.losses
                    .record("rtf.run.language", RtfModelOutcome::Omitted, self.limits)?;
            }

            _ => {
                if name.starts_with("ul") {
                    self.set_underline(name, parameter)?;
                } else if is_border_style(name) {
                    self.set_border_style(name);
                } else if !is_consumed_without_loss(name) {
                    self.losses.record_named(
                        "rtf.control-word.unknown",
                        Some(name),
                        RtfModelOutcome::Omitted,
                        self.limits,
                    )?;
                }
            }
        }
        Ok(())
    }

    // ---- body helpers ---------------------------------------------------

    fn push_break(&mut self, kind: BreakKind) -> Result<(), RtfError> {
        let inline = InlineNode::Break(Break {
            id: next_id(&mut self.ids)?,
            kind,
        });
        self.push_inline(inline)
    }

    fn end_paragraph(&mut self) -> Result<(), RtfError> {
        let properties = self.state().paragraph.clone();
        self.body
            .end_paragraph(properties, &mut self.ids, self.limits)?;
        Ok(())
    }

    fn reset_paragraph(&mut self) {
        self.state_mut().paragraph = ParagraphProperties::default();
        self.pending_tab_alignment = TabAlignment::Start;
        self.pending_tab_leader = None;
        self.line_spacing_value = None;
        self.line_spacing_multiple = false;
        self.current_ls = None;
        self.current_ilvl = 0;
    }

    fn end_section(&mut self) -> Result<(), RtfError> {
        self.end_paragraph()?;
        let id = self.emit_section()?;
        self.body.amend_last_paragraph(|properties| {
            properties.section_break = Some(id);
        });
        self.setup = SectionSetup::default();
        Ok(())
    }

    fn declare_page(&mut self, apply: impl FnOnce(&mut SectionSetup, i32), parameter: Option<i32>) {
        let Some(value) = parameter else {
            return;
        };
        apply(&mut self.setup, value);
        self.setup.declared = true;
    }

    fn indentation(&mut self) -> &mut Indentation {
        self.state_mut()
            .paragraph
            .indentation
            .get_or_insert_with(Indentation::default)
    }

    fn spacing(&mut self) -> &mut Spacing {
        self.state_mut()
            .paragraph
            .spacing
            .get_or_insert_with(Spacing::default)
    }

    /// `\sl` alone is "at least N twips"; a negative `\sl` is "exactly N"; with
    /// `\slmult1` it is a multiple of single spacing in 240ths.
    fn apply_line_spacing(&mut self) {
        let Some(value) = self.line_spacing_value else {
            return;
        };
        let multiple = self.line_spacing_multiple;
        let spacing = self.spacing();
        if multiple {
            let percent = i64::from(value).saturating_mul(100) / 240;
            spacing.line_percent = Some(u16::try_from(percent.clamp(0, 65_535)).unwrap_or(100));
            spacing.line_rule = Some(LineRule::Auto);
            spacing.line_twips = None;
        } else if value < 0 {
            spacing.line_twips = Some((-value).clamp(0, 31_680));
            spacing.line_rule = Some(LineRule::Exact);
            spacing.line_percent = None;
        } else if value > 0 {
            spacing.line_twips = Some(value.clamp(0, 31_680));
            spacing.line_rule = Some(LineRule::AtLeast);
            spacing.line_percent = None;
        } else {
            spacing.line_twips = None;
            spacing.line_rule = None;
            spacing.line_percent = None;
        }
    }

    fn push_tab_stop(&mut self, name: &str, parameter: Option<i32>) {
        let alignment = if name == "tb" {
            TabAlignment::Bar
        } else {
            self.pending_tab_alignment
        };
        let leader = self.pending_tab_leader;
        let position = parameter.unwrap_or(0).clamp(-31_680, 31_680);
        let tabs = &mut self.state_mut().paragraph.tabs;
        if tabs.len() < 128 {
            tabs.push(TabStop {
                position_twips: position,
                alignment,
                leader,
            });
        }
        self.pending_tab_alignment = TabAlignment::Start;
        self.pending_tab_leader = None;
    }

    fn select_font(&mut self, index: i32) {
        self.state_mut().font_index = Some(index);
        let name = self
            .fonts
            .get(index)
            .map(|font| font.name.clone())
            .filter(|name| !name.is_empty());
        self.state_mut().run.font_ref = name.map(|name| FontRef::Named(FontName { name }));
    }

    fn select_color(&mut self, index: i32) {
        // An index the colour table does not define, and index 0 (RTF's
        // "auto"), both mean "follow the default", which is `Color::Auto` —
        // not black. Importing them as black would silently repaint every
        // automatic run.
        let color = match self.colors.get(index) {
            Some(Some(rgb)) => Some(Color::Rgb(rgb)),
            Some(None) => Some(Color::Auto),
            None => None,
        };
        self.state_mut().run.color = color;
    }

    fn select_highlight(&mut self, name: &str, index: i32) -> Result<(), RtfError> {
        let rgb = self.colors.get(index).flatten();
        if name == "cb" || name == "chcbpat" {
            self.state_mut().run.shading.fill = rgb;
            return Ok(());
        }
        let Some(rgb) = rgb else {
            self.state_mut().run.highlight = Some(HighlightColor::None);
            return Ok(());
        };
        match highlight_for(rgb) {
            Some(highlight) => self.state_mut().run.highlight = Some(highlight),
            None => {
                // The model's highlight vocabulary is Word's sixteen names, so
                // an arbitrary RGB highlight cannot be represented exactly;
                // carry it as run shading and say so rather than pick a
                // nearest-neighbour colour the user never chose.
                self.state_mut().run.shading.fill = Some(rgb);
                self.losses.record(
                    "rtf.run.highlight-not-standard",
                    RtfModelOutcome::Degraded,
                    self.limits,
                )?;
            }
        }
        Ok(())
    }

    fn set_underline(&mut self, name: &str, parameter: Option<i32>) -> Result<(), RtfError> {
        // Every `\ul*` word takes a parameter, and `0` turns it OFF — `\ul0`
        // is how producers end an underline run without `\ulnone`. Treating it
        // as "on" underlines the rest of the paragraph.
        if parameter == Some(0) {
            let run = &mut self.state_mut().run;
            run.underline = Some(false);
            run.underline_style = None;
            return Ok(());
        }
        let (style, exact) = match name {
            "ul" => (UnderlineStyle::Single, true),
            "uld" => (UnderlineStyle::Dotted, true),
            "uldb" => (UnderlineStyle::Double, true),
            "uldash" => (UnderlineStyle::Dashed, true),
            "uldashd" => (UnderlineStyle::DotDash, true),
            "ulth" => (UnderlineStyle::Thick, true),
            "ulw" => (UnderlineStyle::Words, true),
            "ulwave" => (UnderlineStyle::Wavy, true),
            "uldashdd" | "ulhwave" | "ululdbwave" => (UnderlineStyle::Wavy, false),
            "ulthd" | "ulthdash" | "ulthdashd" | "ulthdashdd" | "ulthldash" => {
                (UnderlineStyle::Thick, false)
            }
            _ => (UnderlineStyle::Single, false),
        };
        let run = &mut self.state_mut().run;
        run.underline = Some(true);
        run.underline_style = Some(style);
        if !exact {
            self.losses.record_named(
                "rtf.run.underline-approximated",
                Some(name),
                RtfModelOutcome::Degraded,
                self.limits,
            )?;
        }
        Ok(())
    }

    fn unicode_escape(&mut self, parameter: i32) -> Result<(), RtfError> {
        // `\uN` is a signed 16-bit value; a code point above 0x7FFF is written
        // negative and must be read back modulo 65536.
        let scalar = if parameter < 0 {
            u32::try_from(parameter + 65_536).unwrap_or(0xfffd)
        } else {
            u32::try_from(parameter).unwrap_or(0xfffd)
        };
        let skip = self.state().unicode_skip;
        if (0xd800..=0xdbff).contains(&scalar) {
            if self.pending_high_surrogate.is_some() {
                self.losses.record(
                    "rtf.text.unpaired-surrogate",
                    RtfModelOutcome::Degraded,
                    self.limits,
                )?;
                self.push_char('\u{fffd}')?;
            }
            self.pending_high_surrogate = u16::try_from(scalar).ok();
            self.pending_skip = skip;
            return Ok(());
        }
        if (0xdc00..=0xdfff).contains(&scalar) {
            match self.pending_high_surrogate.take() {
                Some(high) => {
                    let combined =
                        0x1_0000 + ((u32::from(high) - 0xd800) << 10) + (scalar - 0xdc00);
                    let character = char::from_u32(combined).unwrap_or('\u{fffd}');
                    self.push_char(character)?;
                }
                None => {
                    self.losses.record(
                        "rtf.text.unpaired-surrogate",
                        RtfModelOutcome::Degraded,
                        self.limits,
                    )?;
                    self.push_char('\u{fffd}')?;
                }
            }
            self.pending_skip = skip;
            return Ok(());
        }
        if self.pending_high_surrogate.take().is_some() {
            self.losses.record(
                "rtf.text.unpaired-surrogate",
                RtfModelOutcome::Degraded,
                self.limits,
            )?;
            self.push_char('\u{fffd}')?;
        }
        let character = char::from_u32(scalar).unwrap_or('\u{fffd}');
        self.push_char(character)?;
        self.pending_skip = skip;
        Ok(())
    }

    // ---- tables ---------------------------------------------------------

    fn reset_row(&mut self) {
        self.row_definitions.clear();
        self.row_properties = TableRowProperties::default();
        self.pending_cell = TableCellProperties::default();
        self.pending_merge_continue = false;
        self.pending_border = None;
        self.row_left_twips = 0;
        self.cell_cursor = 0;
    }

    fn push_cell_definition(&mut self, boundary: i32) {
        self.row_definitions.push(CellDefinition {
            right_boundary_twips: boundary,
            properties: std::mem::take(&mut self.pending_cell),
            merge_continue: std::mem::take(&mut self.pending_merge_continue),
        });
        self.pending_border = None;
    }

    fn end_cell(&mut self) -> Result<(), RtfError> {
        let index = self.cell_cursor;
        let definition = self.row_definitions.get(index).cloned().unwrap_or_default();
        let previous = if index == 0 {
            self.row_left_twips
        } else {
            self.row_definitions
                .get(index - 1)
                .map_or(self.row_left_twips, |cell| cell.right_boundary_twips)
        };
        self.body
            .end_cell(&definition, previous, &mut self.ids, self.limits)?;
        self.cell_cursor = index + 1;
        Ok(())
    }

    fn end_row(&mut self) -> Result<(), RtfError> {
        let properties = self.row_properties.clone();
        self.body.end_row(properties, &mut self.ids, self.limits)?;
        self.cell_cursor = 0;
        Ok(())
    }

    fn open_border(&mut self, target: BorderTarget, slot: BorderEdgeSlot) {
        self.pending_border = Some((target, slot));
        self.set_border_style("brdrs");
    }

    fn with_border(&mut self, apply: impl FnOnce(&mut casual_doc_model::v1::BorderEdge)) {
        let Some((target, slot)) = self.pending_border else {
            return;
        };
        let borders: &mut dyn BorderSlots = match target {
            BorderTarget::Cell => &mut self.pending_cell.borders,
            BorderTarget::Paragraph => {
                // `ParagraphProperties::borders` is a `BoxedParagraphBorders`,
                // which holds nothing until something asks to write one. Deref
                // through it: reaching this arm means the document carried an
                // explicit border control word, so allocating the set here is
                // the allocation the newtype exists to defer, not an extra one.
                &mut *self
                    .states
                    .last_mut()
                    .expect("root state")
                    .paragraph
                    .borders
            }
        };
        apply(borders.edge(slot));
    }

    fn set_border_style(&mut self, name: &str) {
        let style = border_style_token(name).unwrap_or("single").to_owned();
        self.with_border(|edge| edge.style = style);
    }

    fn set_border_width(&mut self, twips: i32) {
        // Twips to eighth-points: one twip is 1/20 pt, so 8/20 of an eighth.
        let eighths = u32::try_from(twips.max(0)).unwrap_or(0) * 2 / 5;
        self.with_border(|edge| edge.size_eighth_points = Some(eighths.min(1_024)));
    }

    fn set_border_color(&mut self, color: Option<RgbColor>) {
        self.with_border(|edge| edge.color = color);
    }

    // ---- lists ----------------------------------------------------------

    fn apply_numbering(&mut self) -> Result<(), RtfError> {
        let Some(ls) = self.current_ls else {
            return Ok(());
        };
        let Some((index, _)) = self.lists.resolve(ls) else {
            self.state_mut().paragraph.numbering = None;
            return self.losses.record(
                "rtf.list.unresolved",
                RtfModelOutcome::Omitted,
                self.limits,
            );
        };
        let instance = self.ensure_numbering(index)?;
        let levels = self.abstract_level_count(index);
        let level = self.current_ilvl.min(levels.saturating_sub(1));
        self.state_mut().paragraph.numbering = Some(NumberingRef { instance, level });
        Ok(())
    }

    fn abstract_level_count(&self, index: usize) -> u8 {
        self.lists
            .definitions
            .get(index)
            .map_or(1, |definition| definition.levels.len().clamp(1, 9) as u8)
    }

    fn ensure_numbering(&mut self, index: usize) -> Result<NumberingInstanceId, RtfError> {
        if let Some(instance) = self.used_numbering.get(&index) {
            return Ok(*instance);
        }
        let definition = self
            .lists
            .definitions
            .get(index)
            .cloned()
            .unwrap_or_default();
        let mut levels: Vec<NumberingLevel> = definition
            .levels
            .iter()
            .enumerate()
            .take(9)
            .map(|(level_index, level)| NumberingLevel {
                level: u8::try_from(level_index).unwrap_or(0),
                start: level.start_at,
                num_fmt: level.number_format.clone(),
                lvl_text: decode_level_text(&level.level_text),
                lvl_jc: level.justification,
                suff: None,
                is_lgl: false,
                paragraph_properties: None,
                run_properties: None,
                style_ref: None,
                lvl_restart: None,
                pstyle: None,
            })
            .collect();
        if levels.is_empty() {
            // A `\ls` that resolves to a definition with no `\listlevel` still
            // has to produce a level, or the numbering reference would dangle
            // and the whole import would fail validation.
            levels.push(NumberingLevel {
                level: 0,
                start: 1,
                num_fmt: Some(casual_doc_model::v1::NumberFormat::Decimal),
                lvl_text: None,
                lvl_jc: None,
                suff: None,
                is_lgl: false,
                paragraph_properties: None,
                run_properties: None,
                style_ref: None,
                lvl_restart: None,
                pstyle: None,
            });
            self.losses.record(
                "rtf.list.level-defaulted",
                RtfModelOutcome::Degraded,
                self.limits,
            )?;
        }
        let abstract_id = AbstractNumberingId::new(next_id(&mut self.ids)?);
        self.numbering.push((
            abstract_id,
            AbstractNumbering {
                levels,
                multi_level_type: None,
                num_style_link: None,
                style_link: None,
            },
        ));
        let instance_id = NumberingInstanceId::new(next_id(&mut self.ids)?);
        self.instances.push((
            instance_id,
            NumberingInstance {
                abstract_ref: abstract_id,
                overrides: Vec::new(),
            },
        ));
        self.used_numbering.insert(index, instance_id);
        Ok(instance_id)
    }

    // ---- pictures ---------------------------------------------------------

    pub(super) fn finish_picture(&mut self) -> Result<(), RtfError> {
        let Some(picture) = self.picture.take() else {
            return Ok(());
        };
        if picture.byte_len() == 0 {
            return self
                .losses
                .record("rtf.pict.empty", RtfModelOutcome::Omitted, self.limits);
        }
        let Some(kind) = picture.kind else {
            return self.losses.record(
                "rtf.pict.unknown-format",
                RtfModelOutcome::Omitted,
                self.limits,
            );
        };
        let total = self.picture_bytes_total.saturating_add(picture.byte_len());
        enforce(
            "rtf_total_picture_bytes",
            total,
            self.limits.max_total_picture_bytes,
        )?;
        self.picture_bytes_total = total;
        if !kind.is_renderable() {
            self.losses.record(
                "rtf.pict.not-renderable",
                RtfModelOutcome::Degraded,
                self.limits,
            )?;
        }
        let extent = picture_extent(&picture);
        let index = self.media.len() + 1;
        let part_name = format!("media/image{index}.{}", kind.extension());
        let media_id = MediaId::new(next_id(&mut self.ids)?);
        self.media.push((
            media_id,
            MediaReference {
                relationship_id: format!("rIdRtfImage{index}"),
                media_type: kind.media_type().to_owned(),
                part_name: part_name.clone(),
            },
        ));
        self.resources.insert(part_name, picture.into_bytes());
        let drawing = InlineNode::Drawing(Drawing {
            id: next_id(&mut self.ids)?,
            media: media_id,
            extent,
            descr: None,
            crop: None,
            border: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        });
        self.push_inline(drawing)
    }
}

/// Lets one helper write into either a cell's or a paragraph's border set.
trait BorderSlots {
    fn edge(&mut self, slot: BorderEdgeSlot) -> &mut casual_doc_model::v1::BorderEdge;
}

impl BorderSlots for casual_doc_model::v1::TableBorders {
    fn edge(&mut self, slot: BorderEdgeSlot) -> &mut casual_doc_model::v1::BorderEdge {
        let target = match slot {
            BorderEdgeSlot::Top => &mut self.top,
            BorderEdgeSlot::Start => &mut self.start,
            BorderEdgeSlot::Bottom => &mut self.bottom,
            BorderEdgeSlot::End => &mut self.end,
        };
        target.get_or_insert_with(default_border_edge)
    }
}

impl BorderSlots for casual_doc_model::v1::ParagraphBorders {
    fn edge(&mut self, slot: BorderEdgeSlot) -> &mut casual_doc_model::v1::BorderEdge {
        let target = match slot {
            BorderEdgeSlot::Top => &mut self.top,
            BorderEdgeSlot::Start => &mut self.start,
            BorderEdgeSlot::Bottom => &mut self.bottom,
            BorderEdgeSlot::End => &mut self.end,
        };
        target.get_or_insert_with(default_border_edge)
    }
}

fn default_border_edge() -> casual_doc_model::v1::BorderEdge {
    casual_doc_model::v1::BorderEdge {
        style: "single".to_owned(),
        size_eighth_points: None,
        color: None,
        space_points: None,
    }
}

/// `\bN` with no parameter is on; `\b0` is off.
const fn flag(parameter: Option<i32>) -> bool {
    !matches!(parameter, Some(0))
}

fn clamp_indent(parameter: Option<i32>) -> Option<i32> {
    Some(parameter.unwrap_or(0).clamp(-31_680, 31_680))
}

/// Word's sixteen highlight names, by their exact sRGB values.
const fn highlight_for(color: RgbColor) -> Option<HighlightColor> {
    Some(match (color.r, color.g, color.b) {
        (0x00, 0x00, 0x00) => HighlightColor::Black,
        (0x00, 0x00, 0xff) => HighlightColor::Blue,
        (0x00, 0xff, 0xff) => HighlightColor::Cyan,
        (0x00, 0xff, 0x00) => HighlightColor::Green,
        (0xff, 0x00, 0xff) => HighlightColor::Magenta,
        (0xff, 0x00, 0x00) => HighlightColor::Red,
        (0xff, 0xff, 0x00) => HighlightColor::Yellow,
        (0xff, 0xff, 0xff) => HighlightColor::White,
        (0x00, 0x00, 0x80) => HighlightColor::DarkBlue,
        (0x00, 0x80, 0x80) => HighlightColor::DarkCyan,
        (0x00, 0x80, 0x00) => HighlightColor::DarkGreen,
        (0x80, 0x00, 0x80) => HighlightColor::DarkMagenta,
        (0x80, 0x00, 0x00) => HighlightColor::DarkRed,
        (0x80, 0x80, 0x00) => HighlightColor::DarkYellow,
        (0x80, 0x80, 0x80) => HighlightColor::DarkGray,
        (0xc0, 0xc0, 0xc0) => HighlightColor::LightGray,
        _ => return None,
    })
}

fn is_border_style(name: &str) -> bool {
    border_style_token(name).is_some()
}

/// Maps an RTF border-style control word to the model's style token.
fn border_style_token(name: &str) -> Option<&'static str> {
    Some(match name {
        "brdrs" | "brdrsh" | "brdrhair" | "brdrengrave" | "brdremboss" => "single",
        "brdrth" => "thick",
        "brdrdb" => "double",
        "brdrdot" => "dotted",
        "brdrdash" | "brdrdashsm" => "dashed",
        "brdrdashd" => "dotDash",
        "brdrdashdd" => "dotDotDash",
        "brdrtriple" => "triple",
        "brdrwavy" => "wave",
        "brdrwavydb" => "doubleWave",
        "brdrnone" => "none",
        _ => return None,
    })
}

/// `\leveltext` is a length byte followed by placeholder indices and literals.
fn decode_level_text(raw: &str) -> Option<String> {
    let mut characters = raw.chars();
    characters.next()?;
    let mut text = String::new();
    for character in characters {
        if (character as u32) < 9 {
            text.push('%');
            text.push(char::from(b'1' + character as u8));
        } else {
            text.push(character);
        }
        if text.len() > 200 {
            break;
        }
    }
    // `{\leveltext …;}` is terminated by a semicolon that belongs to the
    // group, not to the rendered label.
    let text = text.trim_end_matches(';').to_owned();
    (!text.is_empty()).then_some(text)
}

/// Destinations consumed wholesale, each with the finding it produces.
fn reported_discard(name: &str) -> Option<&'static str> {
    Some(match name {
        "header" | "headerl" | "headerr" | "headerf" | "footer" | "footerl" | "footerr"
        | "footerf" => "rtf.section.header-footer",
        "footnote" => "rtf.note",
        "annotation" | "atnid" | "atnauthor" | "atndate" | "atnref" | "atnparent" | "atrfstart"
        | "atrfend" => "rtf.annotation",
        "revtbl" => "rtf.revision",
        "bkmkstart" | "bkmkend" => "rtf.bookmark",
        "fldinst" => "rtf.field.instruction",
        "stylesheet" => "rtf.stylesheet",
        "object" | "objdata" | "objclass" | "objname" | "objalias" | "objsect" | "objtime"
        | "result" => "rtf.object",
        "shp" | "shpinst" | "shptxt" | "shpgrp" | "shprslt" | "background" => "rtf.shape",
        "mmath" | "mmathPr" | "mmathPict" | "eqn" => "rtf.math",
        "pntext" | "pn" | "pntxta" | "pntxtb" | "pnseclvl" => "rtf.list.legacy-pn",
        "ud" => "rtf.upr.unicode-alternate",
        "datafield" | "datastore" | "docvar" | "userprops" | "template" | "filetbl"
        | "protusertbl" | "xmlnstbl" | "xmlopen" => "rtf.metadata-store",
        "themedata" | "colorschememapping" | "latentstyles" | "rsidtbl" | "generatorinfo"
        | "expandedcolortbl" | "listtextoverride" | "panose" | "falt" | "fname"
        | "wgrffmtfilter" | "pgptbl" | "defchp" | "defpap" | "flymaincnt" => "rtf.producer-table",
        _ => return None,
    })
}

/// Control words consumed with no loss to report.
///
/// Deliberately short. A word belongs here only when it states a fact the
/// model already holds another way; anything that carries formatting we do not
/// map is reported instead, because a construct that vanishes without a
/// finding is silent data loss however small it is.
fn is_consumed_without_loss(name: &str) -> bool {
    matches!(
        name,
        // The paragraph-in-table marker and its nesting depth: honoured
        // structurally through `\cell`/`\row`, and `\itap` above zero is
        // separately reported as `rtf.table.nested`.
        "intbl" | "itap" | "lastrow"
            // Left-to-right top-to-bottom cell text flow: the model's default.
            | "cltxlrtb"
    )
}

/// `\info` sub-destinations, plus `\*\generator`.
fn info_field(name: &str) -> Option<InfoField> {
    Some(match name {
        "title" => InfoField::Title,
        "subject" => InfoField::Subject,
        "author" => InfoField::Author,
        "operator" => InfoField::Operator,
        "keywords" => InfoField::Keywords,
        "doccomm" => InfoField::Comment,
        "category" => InfoField::Category,
        "company" => InfoField::Company,
        "manager" => InfoField::Manager,
        _ => return None,
    })
}

/// Chooses the image extent, preferring the `\pic*goal` twip size a producer
/// states over the raw pixel size, and applying `\picscale*`.
fn picture_extent(picture: &PictureState) -> Option<Extent> {
    let scaled = |base: i64, percent: i32| -> i64 {
        base.saturating_mul(i64::from(percent.clamp(1, 10_000))) / 100
    };
    let width = if picture.goal_width_twips > 0 {
        scaled(
            i64::from(picture.goal_width_twips) * EMU_PER_TWIP,
            picture.scale_x_percent,
        )
    } else if picture.raw_width > 0 {
        scaled(
            i64::from(picture.raw_width) * EMU_PER_PIXEL_96DPI,
            picture.scale_x_percent,
        )
    } else {
        0
    };
    let height = if picture.goal_height_twips > 0 {
        scaled(
            i64::from(picture.goal_height_twips) * EMU_PER_TWIP,
            picture.scale_y_percent,
        )
    } else if picture.raw_height > 0 {
        scaled(
            i64::from(picture.raw_height) * EMU_PER_PIXEL_96DPI,
            picture.scale_y_percent,
        )
    } else {
        0
    };
    if width <= 0 || height <= 0 {
        return None;
    }
    Some(Extent {
        width_emu: width.clamp(0, MAX_EMU),
        height_emu: height.clamp(0, MAX_EMU),
    })
}
