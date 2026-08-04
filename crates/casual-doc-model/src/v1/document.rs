//! The v1 document envelope, strict validation, and snapshot I/O.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use super::*;
use crate::{ModelError, NodeId, SnapshotError, SnapshotLimits, enforce_limit};

/// The schema version stamped on authored and migrated v1 documents.
pub const SCHEMA_VERSION_V1: u32 = 1;

/// A normalized schema v1 document.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Document {
    schema_version: u32,
    document_id: NodeId,
    body: Vec<BlockNode>,
    definitions: Definitions,
    /// Document metadata (`docProps/*`). Additive: omitted when absent so
    /// existing snapshots serialize byte-identically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    properties: Option<DocumentProperties>,
    /// The page background color (`w:background@w:color`), an sRGB fill painted
    /// behind the whole page. Additive: omitted when absent (the default white
    /// page) so existing snapshots serialize byte-identically. A theme/image
    /// background is not modeled here and is reported at import instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    background: Option<RgbColor>,
}

impl Document {
    /// Builds and validates a v1 document from constructed parts. The document
    /// carries no metadata; attach it with [`Document::with_properties`].
    pub fn new(
        document_id: NodeId,
        body: Vec<BlockNode>,
        definitions: Definitions,
    ) -> Result<Self, ModelError> {
        let document = Self {
            schema_version: SCHEMA_VERSION_V1,
            document_id,
            body,
            definitions,
            properties: None,
            background: None,
        };
        document.validate()?;
        Ok(document)
    }

    /// Attaches document metadata (`docProps/*`), re-validating the whole
    /// document. All-empty metadata is dropped so it is equivalent to a document
    /// with none (`docProps/*` parts are omitted on write).
    pub fn with_properties(mut self, properties: DocumentProperties) -> Result<Self, ModelError> {
        self.properties = (!properties.is_empty()).then_some(properties);
        self.validate()?;
        Ok(self)
    }

    /// Returns the document metadata, if any.
    #[must_use]
    pub const fn properties(&self) -> Option<&DocumentProperties> {
        self.properties.as_ref()
    }

    /// Mutable access to document metadata, lazily installing an empty group
    /// if none exists yet. The edit-crate seam for mutating `docProps/*`
    /// (mirrors `body_mut`/`definitions_mut`); an all-empty result is left in
    /// place rather than collapsed back to `None` — `DocumentProperties`'s own
    /// `skip_serializing_if` already omits an empty group on write.
    pub fn properties_mut(&mut self) -> &mut DocumentProperties {
        self.properties
            .get_or_insert_with(DocumentProperties::default)
    }

    /// Attaches the page background color (`w:background`), re-validating the
    /// document. The color paints behind the whole page.
    pub fn with_background(mut self, color: RgbColor) -> Result<Self, ModelError> {
        self.background = Some(color);
        self.validate()?;
        Ok(self)
    }

    /// Returns the page background color, if the document sets one.
    #[must_use]
    pub const fn background(&self) -> Option<RgbColor> {
        self.background
    }

    /// Returns the schema version (always 1 for a valid v1 document).
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns the document id.
    #[must_use]
    pub const fn id(&self) -> NodeId {
        self.document_id
    }

    /// Returns the body blocks.
    #[must_use]
    pub fn body(&self) -> &[BlockNode] {
        &self.body
    }

    /// Returns the body blocks for in-place editing. The editing op set
    /// (`casual-doc-edit`, doc 59) mutates the document through this; callers are
    /// responsible for preserving model invariants (validated on save/export).
    #[must_use]
    pub fn body_mut(&mut self) -> &mut Vec<BlockNode> {
        &mut self.body
    }

    /// Returns the definition tables.
    #[must_use]
    pub const fn definitions(&self) -> &Definitions {
        &self.definitions
    }

    /// Mutable access to the definition tables — the additive seam an editor uses
    /// to register document-level infrastructure an edit needs (e.g. the numbering
    /// definition a new bullet/numbered list references). Body edits go through the
    /// closed op set; this is for the definition side-tables those edits reference.
    #[must_use]
    pub fn definitions_mut(&mut self) -> &mut Definitions {
        &mut self.definitions
    }

    /// Parses one strict, bounded schema v1 JSON document.
    pub fn from_json(bytes: &[u8], limits: SnapshotLimits) -> Result<Self, SnapshotError> {
        limits.validate()?;
        enforce_limit("input_json_bytes", bytes.len(), limits.max_input_bytes)?;
        let document: Self =
            serde_json::from_slice(bytes).map_err(|_| SnapshotError::MalformedJson)?;
        document.validate().map_err(SnapshotError::InvalidModel)?;
        document.validate_snapshot_limits(limits)?;
        Ok(document)
    }

    /// Serializes a valid v1 document to deterministic compact JSON.
    pub fn to_json(&self) -> Result<Vec<u8>, SnapshotError> {
        self.validate().map_err(SnapshotError::InvalidModel)?;
        serde_json::to_vec(self).map_err(|_| SnapshotError::Serialization)
    }

    /// Validates every schema v1 invariant, first-failure-wins.
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.schema_version != SCHEMA_VERSION_V1 {
            return Err(ModelError::UnsupportedSchemaVersion(self.schema_version));
        }
        if self.body.is_empty() {
            return Err(ModelError::EmptyDocumentBody);
        }

        self.validate_unique_ids()?;
        self.validate_styles()?;
        self.validate_numbering()?;
        self.validate_sections()?;
        self.validate_media()?;
        self.validate_document_defaults()?;
        self.validate_latent_styles()?;
        self.validate_notes()?;
        self.validate_headers_footers()?;
        self.validate_comments()?;
        self.validate_people()?;
        self.validate_bookmarks()?;
        self.validate_font_table()?;
        self.validate_font_scheme()?;
        self.validate_color_scheme()?;
        self.validate_settings()?;
        self.validate_properties()?;
        self.validate_body()?;
        Ok(())
    }

    fn validate_settings(&self) -> Result<(), ModelError> {
        let settings = &self.definitions.settings;
        if let Some(value) = settings.default_tab_stop {
            check_domain((0..=31_680).contains(&value), "settings.defaultTabStop")?;
        }
        if let Some(percent) = settings.zoom.percent {
            check_domain((1..=1_000).contains(&percent), "settings.zoom.percent")?;
        }
        if let Some(style) = &settings.default_table_style {
            check_domain(
                !style.is_empty() && style.len() <= 255,
                "settings.defaultTableStyle",
            )?;
        }
        for setting in &settings.compat {
            check_domain(
                !setting.name.is_empty() && setting.name.len() <= 255,
                "settings.compatSetting.name",
            )?;
            check_domain(
                !setting.uri.is_empty() && setting.uri.len() <= 255,
                "settings.compatSetting.uri",
            )?;
            check_domain(setting.val.len() <= 255, "settings.compatSetting.val")?;
        }
        for props in [&settings.footnote_props, &settings.endnote_props] {
            check_note_props(props)?;
        }
        Ok(())
    }

    fn validate_properties(&self) -> Result<(), ModelError> {
        if let Some(properties) = &self.properties {
            properties.validate()?;
        }
        Ok(())
    }

    fn validate_font_scheme(&self) -> Result<(), ModelError> {
        let Some(scheme) = &self.definitions.font_scheme else {
            return Ok(());
        };
        for collection in [&scheme.major, &scheme.minor] {
            for entry in [&collection.latin, &collection.ea, &collection.cs] {
                check_domain(entry.typeface.len() <= 255, "fontScheme.typeface")?;
                for (value, field) in [
                    (&entry.panose, "fontScheme.panose"),
                    (&entry.pitch_family, "fontScheme.pitchFamily"),
                    (&entry.charset, "fontScheme.charset"),
                ] {
                    if let Some(value) = value {
                        check_domain(!value.is_empty() && value.len() <= 255, field)?;
                    }
                }
            }
            for over in &collection.script_overrides {
                check_domain(
                    !over.script.is_empty() && over.script.len() <= 32,
                    "fontScheme.script",
                )?;
                check_domain(over.typeface.len() <= 255, "fontScheme.override.typeface")?;
            }
        }
        Ok(())
    }

    fn validate_color_scheme(&self) -> Result<(), ModelError> {
        if let Some(scheme) = &self.definitions.color_scheme {
            check_domain(scheme.name.len() <= 255, "clrScheme.name")?;
            for slot in [
                &scheme.dark1,
                &scheme.light1,
                &scheme.dark2,
                &scheme.light2,
                &scheme.accent1,
                &scheme.accent2,
                &scheme.accent3,
                &scheme.accent4,
                &scheme.accent5,
                &scheme.accent6,
                &scheme.hyperlink,
                &scheme.followed_hyperlink,
            ] {
                if let SchemeColor::System(system) = slot {
                    check_domain(
                        !system.value.is_empty() && system.value.len() <= 32,
                        "clrScheme.sysClr.val",
                    )?;
                }
            }
        }
        // The format scheme is retained verbatim; bound its size so a hostile
        // theme cannot inflate the model unboundedly.
        if let Some(xml) = &self.definitions.format_scheme_xml {
            check_domain(!xml.is_empty() && xml.len() <= 1 << 20, "fmtScheme")?;
        }
        Ok(())
    }

    fn validate_font_table(&self) -> Result<(), ModelError> {
        for font in &self.definitions.font_table {
            check_domain(!font.name.is_empty() && font.name.len() <= 255, "font.name")?;
            for (value, field) in [
                (&font.alt_name, "font.altName"),
                (&font.panose1, "font.panose1"),
                (&font.charset, "font.charset"),
            ] {
                if let Some(value) = value {
                    check_domain(!value.is_empty() && value.len() <= 255, field)?;
                }
            }
            for (value, field) in [
                (&font.sig.usb0, "font.sig.usb0"),
                (&font.sig.usb1, "font.sig.usb1"),
                (&font.sig.usb2, "font.sig.usb2"),
                (&font.sig.usb3, "font.sig.usb3"),
                (&font.sig.csb0, "font.sig.csb0"),
                (&font.sig.csb1, "font.sig.csb1"),
            ] {
                if let Some(value) = value {
                    check_domain(!value.is_empty() && value.len() <= 32, field)?;
                }
            }
            for (_, face) in font.embedded.faces() {
                check_domain(
                    !face.font_key.is_empty() && face.font_key.len() <= 64,
                    "font.embed.fontKey",
                )?;
                check_domain(
                    !face.relationship_id.is_empty() && face.relationship_id.len() <= 255,
                    "font.embed.relationshipId",
                )?;
                check_domain(
                    !face.part_name.is_empty() && face.part_name.len() <= 1024,
                    "font.embed.partName",
                )?;
            }
        }
        Ok(())
    }

    fn validate_document_defaults(&self) -> Result<(), ModelError> {
        if let Some(defaults) = &self.definitions.document_defaults {
            if let Some(properties) = &defaults.paragraph {
                self.check_paragraph_property_refs(properties)?;
            }
            if let Some(properties) = &defaults.run {
                self.check_run_property_refs(properties)?;
            }
        }
        Ok(())
    }

    fn validate_latent_styles(&self) -> Result<(), ModelError> {
        if let Some(latent) = &self.definitions.latent_styles {
            check_domain(
                latent.exceptions.len() <= MAX_LATENT_STYLE_EXCEPTIONS,
                "latentStyles.exceptions",
            )?;
            for exception in &latent.exceptions {
                check_domain(
                    !exception.name.is_empty()
                        && exception.name.len() <= MAX_LATENT_STYLE_NAME_BYTES,
                    "latentStyles.exception.name",
                )?;
            }
        }
        Ok(())
    }

    fn validate_sections(&self) -> Result<(), ModelError> {
        for section in &self.definitions.sections {
            check_section_domains(section)?;
            for header in &section.headers {
                if !self.definitions.headers.contains_key(&header.reference) {
                    return Err(ModelError::DanglingHeaderFooterRef(
                        header.reference.node_id(),
                    ));
                }
            }
            for footer in &section.footers {
                if !self.definitions.footers.contains_key(&footer.reference) {
                    return Err(ModelError::DanglingHeaderFooterRef(
                        footer.reference.node_id(),
                    ));
                }
            }
            // The `w:sectPrChange` prior snapshot: bound its metadata, then the prior
            // section's own domains (a prior never carries a further change, and its
            // historical header/footer references are not re-validated here).
            if let Some(change) = &section.section_change {
                check_prop_change_meta(change, "section.sectPrChange")?;
                check_section_domains(&change.prior)?;
            }
        }
        Ok(())
    }

    fn validate_media(&self) -> Result<(), ModelError> {
        for (_, media) in self.definitions.media.iter() {
            check_domain(
                !media.relationship_id.is_empty() && media.relationship_id.len() <= 255,
                "media.relationship_id",
            )?;
            check_domain(
                !media.media_type.is_empty() && media.media_type.len() <= 255,
                "media.media_type",
            )?;
            check_domain(
                !media.part_name.is_empty() && media.part_name.len() <= 1024,
                "media.part_name",
            )?;
        }
        Ok(())
    }

    /// Bounds an embedded object's part pointer (relationship id, type URI, and
    /// package part name), matching the media-reference bounds.
    fn check_embedded_part(&self, part: &EmbeddedPart) -> Result<(), ModelError> {
        check_domain(
            !part.relationship_id.is_empty() && part.relationship_id.len() <= 255,
            "embeddedObject.part.relationshipId",
        )?;
        check_domain(
            !part.relationship_type.is_empty() && part.relationship_type.len() <= 2048,
            "embeddedObject.part.relationshipType",
        )?;
        check_domain(
            !part.part_name.is_empty() && part.part_name.len() <= 1024,
            "embeddedObject.part.partName",
        )?;
        Ok(())
    }

    fn validate_unique_ids(&self) -> Result<(), ModelError> {
        let mut ids = BTreeSet::new();
        insert_id(&mut ids, self.document_id)?;
        for (id, _) in self.definitions.styles.iter() {
            insert_id(&mut ids, id.node_id())?;
        }
        for (id, _) in self.definitions.abstract_numbering.iter() {
            insert_id(&mut ids, id.node_id())?;
        }
        for (id, _) in self.definitions.numbering.iter() {
            insert_id(&mut ids, id.node_id())?;
        }
        for section in &self.definitions.sections {
            insert_id(&mut ids, section.id.node_id())?;
        }
        for (id, _) in self.definitions.media.iter() {
            insert_id(&mut ids, id.node_id())?;
        }
        for (id, note) in self.definitions.footnotes.iter() {
            insert_id(&mut ids, id.node_id())?;
            for block in &note.blocks {
                record_block_ids(block, &mut ids)?;
            }
        }
        for (id, note) in self.definitions.endnotes.iter() {
            insert_id(&mut ids, id.node_id())?;
            for block in &note.blocks {
                record_block_ids(block, &mut ids)?;
            }
        }
        for (id, header_footer) in self
            .definitions
            .headers
            .iter()
            .chain(self.definitions.footers.iter())
        {
            insert_id(&mut ids, id.node_id())?;
            for block in &header_footer.blocks {
                record_block_ids(block, &mut ids)?;
            }
        }
        for (id, comment) in self.definitions.comments.iter() {
            insert_id(&mut ids, id.node_id())?;
            for block in &comment.blocks {
                record_block_ids(block, &mut ids)?;
            }
        }
        for (id, _) in self.definitions.bookmarks.iter() {
            insert_id(&mut ids, id.node_id())?;
        }
        for block in &self.body {
            record_block_ids(block, &mut ids)?;
        }
        Ok(())
    }

    fn style_exists(&self, id: StyleId) -> bool {
        self.definitions.styles.contains_key(&id)
    }

    fn validate_styles(&self) -> Result<(), ModelError> {
        for (id, style) in self.definitions.styles.iter() {
            if let Some(properties) = &style.paragraph {
                self.check_paragraph_property_refs(properties)?;
            }
            if let Some(properties) = &style.run {
                self.check_run_property_refs(properties)?;
            }
            // A `w:next` / `w:link` reference must resolve; unlike `basedOn` its
            // target need not share this style's kind (`link` deliberately points
            // at the companion style of the opposite kind).
            for reference in [style.next, style.link].into_iter().flatten() {
                if !self.style_exists(reference) {
                    return Err(ModelError::DanglingStyleRef(reference.node_id()));
                }
            }
            // Conditional-format overrides (`w:tblStylePr`) may carry paragraph /
            // run property style references, which must resolve too.
            for over in &style.conditional {
                if let Some(properties) = &over.paragraph {
                    self.check_paragraph_property_refs(properties)?;
                }
                if let Some(properties) = &over.run {
                    self.check_run_property_refs(properties)?;
                }
            }
            if let Some(based_on) = style.based_on {
                if !self.style_exists(based_on) {
                    return Err(ModelError::DanglingStyleRef(based_on.node_id()));
                }
                let base_kind = self.definitions.styles.get(&based_on).map(|base| base.kind);
                if base_kind != Some(style.kind) {
                    return Err(ModelError::StyleBasedOnKindMismatch {
                        style: id.node_id(),
                        based_on: based_on.node_id(),
                    });
                }
            }
            // Cycle detection: walk the based_on chain from this style.
            let mut visited = BTreeSet::new();
            visited.insert(*id);
            let mut current = style.based_on;
            while let Some(next) = current {
                if !visited.insert(next) {
                    return Err(ModelError::StyleBasedOnCycle(id.node_id()));
                }
                current = self
                    .definitions
                    .styles
                    .get(&next)
                    .and_then(|style| style.based_on);
            }
        }
        Ok(())
    }

    fn validate_numbering(&self) -> Result<(), ModelError> {
        for (id, instance) in self.definitions.numbering.iter() {
            let abstract_num = self
                .definitions
                .abstract_numbering
                .get(&instance.abstract_ref)
                .ok_or(ModelError::DanglingAbstractNumberingRef(
                    instance.abstract_ref.node_id(),
                ))?;
            for numbering_override in &instance.overrides {
                if let Some(start) = numbering_override.start {
                    check_domain(start <= 32_767, "numbering.override.start")?;
                }
                if !abstract_num
                    .levels
                    .iter()
                    .any(|level| level.level == numbering_override.level)
                {
                    return Err(ModelError::NumberingLevelUndefined {
                        reference: id.node_id(),
                        level: numbering_override.level,
                    });
                }
                // A full `w:lvlOverride/w:lvl` redefinition is bounded exactly
                // like an abstract level.
                if let Some(definition) = &numbering_override.definition {
                    self.validate_numbering_level(definition)?;
                }
            }
        }
        // Level domain: level start values, format/text bounds, and per-level
        // property references.
        for (_, abstract_num) in self.definitions.abstract_numbering.iter() {
            for level in &abstract_num.levels {
                self.validate_numbering_level(level)?;
            }
        }
        Ok(())
    }

    fn validate_numbering_level(&self, level: &NumberingLevel) -> Result<(), ModelError> {
        check_domain(level.start <= 32_767, "numbering.level.start")?;
        if let Some(NumberFormat::Other(token)) = &level.num_fmt {
            check_domain(
                !token.is_empty() && token.len() <= 64,
                "numbering.level.numFmt",
            )?;
        }
        if let Some(text) = &level.lvl_text {
            check_domain(text.len() <= 255, "numbering.level.lvlText")?;
        }
        if let Some(properties) = &level.paragraph_properties {
            self.check_paragraph_property_refs(properties)?;
        }
        if let Some(properties) = &level.run_properties {
            self.check_run_property_refs(properties)?;
        }
        if let Some(style) = level.style_ref
            && !self.style_exists(style)
        {
            return Err(ModelError::DanglingStyleRef(style.node_id()));
        }
        Ok(())
    }

    fn resolve_numbering_level(&self, reference: &NumberingRef) -> Result<(), ModelError> {
        let instance = self.definitions.numbering.get(&reference.instance).ok_or(
            ModelError::DanglingNumberingRef(reference.instance.node_id()),
        )?;
        let abstract_num = self
            .definitions
            .abstract_numbering
            .get(&instance.abstract_ref)
            .ok_or(ModelError::DanglingAbstractNumberingRef(
                instance.abstract_ref.node_id(),
            ))?;
        if abstract_num
            .levels
            .iter()
            .any(|level| level.level == reference.level)
        {
            Ok(())
        } else {
            Err(ModelError::NumberingLevelUndefined {
                reference: reference.instance.node_id(),
                level: reference.level,
            })
        }
    }

    fn check_paragraph_property_refs(
        &self,
        properties: &ParagraphProperties,
    ) -> Result<(), ModelError> {
        if let Some(style) = properties.style_ref
            && !self.style_exists(style)
        {
            return Err(ModelError::DanglingStyleRef(style.node_id()));
        }
        if let Some(section) = properties.section_break
            && !self
                .definitions
                .sections
                .iter()
                .any(|boundary| boundary.id == section)
        {
            return Err(ModelError::DanglingSectionRef(section.node_id()));
        }
        if let Some(numbering) = &properties.numbering {
            self.resolve_numbering_level(numbering)?;
        }
        if let Some(indentation) = &properties.indentation {
            for value in [
                indentation.start_twips,
                indentation.end_twips,
                indentation.first_line_twips,
                indentation.hanging_twips,
            ]
            .into_iter()
            .flatten()
            {
                check_domain((-31_680..=31_680).contains(&value), "paragraph.indentation")?;
            }
        }
        if let Some(spacing) = &properties.spacing {
            for value in [spacing.before_twips, spacing.after_twips]
                .into_iter()
                .flatten()
            {
                check_domain((0..=31_680).contains(&value), "paragraph.spacing")?;
            }
            if let Some(percent) = spacing.line_percent {
                check_domain(
                    (1..=10_000).contains(&percent),
                    "paragraph.spacing.line_percent",
                )?;
            }
        }
        if let Some(frame) = properties.drop_cap_frame {
            check_domain(frame.lines > 0, "paragraph.drop_cap_frame.lines")?;
            for position in [
                frame.horizontal_position_twips,
                frame.vertical_position_twips,
            ]
            .into_iter()
            .flatten()
            {
                check_domain(
                    (-31_680..=31_680).contains(&position),
                    "paragraph.drop_cap_frame.position",
                )?;
            }
            for space in [frame.horizontal_space_twips, frame.vertical_space_twips]
                .into_iter()
                .flatten()
            {
                check_domain(space <= 31_680, "paragraph.drop_cap_frame.spacing")?;
            }
        }
        if let Some(level) = properties.outline_level {
            check_domain(level <= 9, "paragraph.outline_level")?;
        }
        for edge in [
            &properties.borders.top,
            &properties.borders.bottom,
            &properties.borders.start,
            &properties.borders.end,
            &properties.borders.between,
            &properties.borders.bar,
        ]
        .into_iter()
        .flatten()
        {
            check_domain(
                !edge.style.is_empty() && edge.style.len() <= 32,
                "paragraph.borders",
            )?;
            if let Some(size) = edge.size_eighth_points {
                check_domain(size <= 1024, "paragraph.borders")?;
            }
            if let Some(space) = edge.space_points {
                check_domain(space <= 31, "paragraph.borders")?;
            }
        }
        check_domain(properties.tabs.len() <= 128, "paragraph.tabs")?;
        for tab in &properties.tabs {
            check_domain(
                (-31_680..=31_680).contains(&tab.position_twips),
                "paragraph.tabs",
            )?;
        }
        if let Some(mark_run) = &properties.mark_run {
            self.check_run_property_refs(mark_run)?;
        }
        // The `w:pPrChange` prior snapshot: bound its metadata, then validate the
        // prior properties with the same rules as the current ones.
        if let Some(change) = &properties.prop_change {
            check_prop_change_meta(change, "paragraph.propChange")?;
            check_domain(
                change.editor_group.is_none(),
                "paragraph.propChange.editorGroup",
            )?;
            self.check_paragraph_property_refs(&change.prior)?;
        }
        Ok(())
    }

    fn check_run_property_refs(&self, properties: &RunProperties) -> Result<(), ModelError> {
        if let Some(style) = properties.style_ref
            && !self.style_exists(style)
        {
            return Err(ModelError::DanglingStyleRef(style.node_id()));
        }
        if let Some(size) = properties.size_half_points {
            check_domain((1..=65_534).contains(&size), "run.size_half_points")?;
        }
        // Every named font slot (ascii + the three additive siblings) is bounded.
        for slot in [
            &properties.font_ref,
            &properties.font_ref_h_ansi,
            &properties.font_ref_cs,
            &properties.font_ref_east_asia,
        ] {
            if let Some(FontRef::Named(font)) = slot {
                check_domain(
                    !font.name.is_empty() && font.name.len() <= 255,
                    "run.font_ref.name",
                )?;
            }
        }
        if let Some(value) = properties.character_spacing_twips {
            check_domain((-31_680..=31_680).contains(&value), "run.character_spacing")?;
        }
        if let Some(value) = properties.character_scale_percent {
            check_domain((1..=600).contains(&value), "run.character_scale")?;
        }
        if let Some(value) = properties.kerning_half_points {
            check_domain(value <= 65_534, "run.kerning")?;
        }
        if let Some(value) = properties.position_half_points {
            check_domain((-31_680..=31_680).contains(&value), "run.position")?;
        }
        if let Some(language) = &properties.language {
            for tag in [&language.value, &language.east_asia, &language.bidi]
                .into_iter()
                .flatten()
            {
                check_domain(!tag.is_empty() && tag.len() <= 85, "run.language")?;
            }
        }
        if let Some(edge) = &properties.border {
            check_domain(
                !edge.style.is_empty() && edge.style.len() <= 32,
                "run.border",
            )?;
            if let Some(size) = edge.size_eighth_points {
                check_domain(size <= 1024, "run.border")?;
            }
            if let Some(space) = edge.space_points {
                check_domain(space <= 31, "run.border")?;
            }
        }
        // The `w:rPrChange` prior snapshot: bound its metadata, then validate the
        // prior properties with the same rules as the current ones.
        if let Some(change) = &properties.prop_change {
            check_prop_change_meta(change, "run.propChange")?;
            if let Some(group) = change.editor_group {
                check_domain(
                    group.kind == RevisionGroupKind::Formatting,
                    "run.propChange.editorGroup",
                )?;
            }
            self.check_run_property_refs(&change.prior)?;
        }
        Ok(())
    }

    fn validate_body(&self) -> Result<(), ModelError> {
        for block in &self.body {
            self.validate_block(block, 0, 0, 0)?;
        }
        Ok(())
    }

    /// Validates one block, recursing through table cells, text boxes, and content
    /// controls. `table_depth` bounds table nesting; `textbox_depth` bounds
    /// text-box nesting; `sdt_depth` bounds content-control (`w:sdt`) nesting.
    fn validate_block(
        &self,
        block: &BlockNode,
        table_depth: u32,
        textbox_depth: u32,
        sdt_depth: u32,
    ) -> Result<(), ModelError> {
        match block {
            BlockNode::Paragraph(paragraph) => {
                self.check_paragraph_property_refs(&paragraph.properties)?;
                self.validate_inlines(
                    &paragraph.inlines,
                    paragraph.id,
                    false,
                    textbox_depth,
                    0,
                    sdt_depth,
                )?;
            }
            BlockNode::Table(table) => {
                self.validate_table(table, table_depth, textbox_depth, sdt_depth)?
            }
            BlockNode::Sdt(sdt) => {
                if sdt_depth + 1 > MAX_SDT_DEPTH {
                    return Err(ModelError::SdtNestingTooDeep(sdt.id));
                }
                if sdt.blocks.is_empty() {
                    return Err(ModelError::EmptySdt(sdt.id));
                }
                check_sdt_properties(&sdt.properties)?;
                // A block sdt's content builds in a fresh block container in the
                // importer (a suspended frame with its own table stack), so the
                // table budget restarts at 0 here — threading the enclosing
                // `table_depth` would reject deep tables inside an sdt that the
                // importer accepts and fail the whole import. The text-box budget
                // passes through (an sdt is transparent to it); `sdt_depth` bounds
                // this recursion.
                for nested in &sdt.blocks {
                    self.validate_block(nested, 0, textbox_depth, sdt_depth + 1)?;
                }
            }
            // An alt chunk is an opaque leaf block: it only references a preserved
            // part (bounded like an embedded object's part). `matchSrc` is a typed
            // bool with no domain to check.
            BlockNode::AltChunk(chunk) => {
                self.check_embedded_part(&chunk.part)?;
            }
        }
        Ok(())
    }

    fn validate_table(
        &self,
        table: &Table,
        table_depth: u32,
        textbox_depth: u32,
        sdt_depth: u32,
    ) -> Result<(), ModelError> {
        if table_depth + 1 > MAX_TABLE_DEPTH {
            return Err(ModelError::TableNestingTooDeep(table.id));
        }
        if table.rows.is_empty() {
            return Err(ModelError::EmptyTable(table.id));
        }
        self.check_table_properties(&table.properties)?;
        for column in &table.grid {
            self.check_grid_column(column)?;
        }
        // The `w:tblGridChange` prior snapshot: bound its metadata, then validate
        // each prior column with the same width domain as the current grid.
        if let Some(change) = &table.grid_change {
            check_prop_change_meta(change, "table.gridChange")?;
            check_domain(
                change.editor_group.is_none(),
                "table.gridChange.editorGroup",
            )?;
            for column in change.prior.iter() {
                self.check_grid_column(column)?;
            }
        }
        for row in &table.rows {
            if row.cells.is_empty() {
                return Err(ModelError::EmptyTableRow(row.id));
            }
            self.check_row_properties(&row.properties)?;
            for cell in &row.cells {
                if cell.blocks.is_empty() {
                    return Err(ModelError::EmptyTableCell(cell.id));
                }
                self.check_cell_properties(&cell.properties)?;
                for nested in &cell.blocks {
                    self.validate_block(nested, table_depth + 1, textbox_depth, sdt_depth)?;
                }
            }
        }
        Ok(())
    }

    /// Validates table properties (`w:tblPr`): style-reference resolution, value
    /// domains, and — recursively — any `w:tblPrChange` prior snapshot. Shared by
    /// the current properties and every nested prior snapshot.
    fn check_table_properties(&self, properties: &TableProperties) -> Result<(), ModelError> {
        // The associated table style (`w:tblStyle`) must resolve to a defined
        // style, like paragraph/run style references elsewhere.
        if let Some(style) = properties.style_ref
            && !self.style_exists(style)
        {
            return Err(ModelError::DanglingStyleRef(style.node_id()));
        }
        if let Some(width) = properties.width {
            check_domain(width.is_valid(), "table.width")?;
        }
        // `both` (justify) is not a valid `ST_JcTable` value, and the table
        // importer never yields it (it maps `both` -> None). Reject it so an
        // authored model cannot make the writer emit an invalid `w:jc`.
        if let Some(alignment) = properties.alignment {
            check_domain(alignment != Alignment::Justify, "table.alignment")?;
        }
        if let Some(indent) = properties.indent_twips {
            check_domain((-31_680..=31_680).contains(&indent), "table.indent")?;
        }
        if let Some(spacing) = properties.cell_spacing_twips {
            check_domain((0..=31_680).contains(&spacing), "table.cell_spacing")?;
        }
        for value in [&properties.caption, &properties.description]
            .into_iter()
            .flatten()
        {
            check_domain(
                !value.is_empty() && value.len() <= 255,
                "table.accessibility",
            )?;
        }
        check_borders(&properties.borders, "table.borders")?;
        check_margins(&properties.cell_margins, "table.cell_margins")?;
        if let Some(change) = &properties.prop_change {
            check_prop_change_meta(change, "table.propChange")?;
            check_domain(
                change.editor_group.is_none(),
                "table.propChange.editorGroup",
            )?;
            self.check_table_properties(&change.prior)?;
        }
        Ok(())
    }

    /// Validates one grid column's width domain (`w:gridCol`).
    fn check_grid_column(&self, column: &GridColumn) -> Result<(), ModelError> {
        if let Some(width) = column.width_twips {
            check_domain((0..=31_680).contains(&width), "table.grid.column.width")?;
        }
        Ok(())
    }

    /// Validates table-row properties (`w:trPr`): value domains and — recursively
    /// — any `w:trPrChange` prior snapshot.
    fn check_row_properties(&self, properties: &TableRowProperties) -> Result<(), ModelError> {
        if let Some(height) = properties.height.value_twips {
            check_domain(height <= 31_680, "table.row.height")?;
        }
        for width in [properties.w_before, properties.w_after]
            .into_iter()
            .flatten()
        {
            check_domain(width.is_valid(), "table.row.short_width")?;
        }
        if let Some(spacing) = properties.cell_spacing_twips {
            check_domain((0..=31_680).contains(&spacing), "table.row.cell_spacing")?;
        }
        if let Some(alignment) = properties.alignment {
            check_domain(alignment != Alignment::Justify, "table.row.alignment")?;
        }
        if let Some(change) = &properties.prop_change {
            check_prop_change_meta(change, "table.row.propChange")?;
            check_domain(
                change.editor_group.is_none(),
                "table.row.propChange.editorGroup",
            )?;
            self.check_row_properties(&change.prior)?;
        }
        Ok(())
    }

    /// Validates table-cell properties (`w:tcPr`): value domains and — recursively
    /// — any `w:tcPrChange` prior snapshot.
    fn check_cell_properties(&self, properties: &TableCellProperties) -> Result<(), ModelError> {
        if let Some(span) = properties.grid_span {
            check_domain((1..=16_384).contains(&span), "table.cell.grid_span")?;
        }
        if let Some(width) = properties.width {
            check_domain(width.is_valid(), "table.cell.width")?;
        }
        check_borders(&properties.borders, "table.cell.borders")?;
        check_margins(&properties.margins, "table.cell.margins")?;
        if let Some(change) = &properties.prop_change {
            check_prop_change_meta(change, "table.cell.propChange")?;
            check_domain(
                change.editor_group.is_none(),
                "table.cell.propChange.editorGroup",
            )?;
            self.check_cell_properties(&change.prior)?;
        }
        Ok(())
    }

    /// Validates one inline sequence. A drawing, hyperlink, field, or text box is a
    /// hard merge boundary (it resets adjacent-run tracking, like a tab or break).
    /// `in_wrapper` is set while validating a hyperlink's or field's own children,
    /// so a nested wrapper (hyperlink or field inside either) is rejected — this
    /// bounds inline nesting to one wrapper level. `textbox_depth` carries the
    /// enclosing text-box nesting; a text box's blocks restart the table budget.
    /// `revision_depth` bounds tracked-change (`w:ins`/`w:del`) wrapper nesting; a
    /// revision is transparent to `in_wrapper` (it neither imposes nor clears the
    /// leaf-only rule), so only nested revisions bump it. `sdt_depth` bounds
    /// content-control (`w:sdt`) nesting; an inline sdt is likewise transparent to
    /// `in_wrapper`, so only nested sdts bump it.
    fn validate_inlines(
        &self,
        inlines: &[InlineNode],
        owner: NodeId,
        in_wrapper: bool,
        textbox_depth: u32,
        revision_depth: u32,
        sdt_depth: u32,
    ) -> Result<(), ModelError> {
        let mut previous_run_properties: Option<&RunProperties> = None;
        for inline in inlines {
            match inline {
                InlineNode::Run(run) => {
                    if run.text.is_empty() {
                        return Err(ModelError::EmptyTextRun);
                    }
                    self.check_run_property_refs(&run.properties)?;
                    let length = run.text.graphemes(true).count();
                    u32::try_from(length).map_err(|_| ModelError::GraphemeCountOverflow(run.id))?;
                    if previous_run_properties == Some(&run.properties) {
                        return Err(ModelError::AdjacentEquivalentTextRuns(owner));
                    }
                    previous_run_properties = Some(&run.properties);
                }
                InlineNode::Drawing(drawing) => {
                    if !self.definitions.media.contains_key(&drawing.media) {
                        return Err(ModelError::DanglingMediaRef(drawing.media.node_id()));
                    }
                    if let Some(extent) = &drawing.extent {
                        check_domain(
                            (0..=MAX_EMU).contains(&extent.width_emu),
                            "drawing.extent.width",
                        )?;
                        check_domain(
                            (0..=MAX_EMU).contains(&extent.height_emu),
                            "drawing.extent.height",
                        )?;
                    }
                    previous_run_properties = None;
                }
                InlineNode::AnchoredDrawing(drawing) => {
                    if !self.definitions.media.contains_key(&drawing.media) {
                        return Err(ModelError::DanglingMediaRef(drawing.media.node_id()));
                    }
                    check_domain(
                        (0..=MAX_EMU).contains(&drawing.extent.width_emu),
                        "anchoredDrawing.extent.width",
                    )?;
                    check_domain(
                        (0..=MAX_EMU).contains(&drawing.extent.height_emu),
                        "anchoredDrawing.extent.height",
                    )?;
                    // A `posOffset` is signed (`ST_PositionOffset`, xsd:int) but is
                    // bounded to the positive-coordinate magnitude so it cannot name
                    // a point unrepresentably far off the page.
                    check_anchor_offset(
                        &drawing.anchor.horizontal.position,
                        &drawing.anchor.vertical.position,
                    )?;
                    check_wrap_distances(&drawing.anchor.wrap_distances)?;
                    if let Some(descr) = &drawing.descr {
                        check_domain(
                            !descr.is_empty() && descr.len() <= MAX_DESCR_BYTES,
                            "anchoredDrawing.descr",
                        )?;
                    }
                    previous_run_properties = None;
                }
                InlineNode::EmbeddedObject(object) => {
                    self.check_embedded_part(&object.part)?;
                    for extra in &object.extra_parts {
                        self.check_embedded_part(extra)?;
                    }
                    if let EmbeddedKind::Other(uri) = &object.kind {
                        check_domain(
                            !uri.is_empty() && uri.len() <= 2048,
                            "embeddedObject.kind.uri",
                        )?;
                    }
                    if let Some(preview) = &object.preview
                        && !self.definitions.media.contains_key(preview)
                    {
                        return Err(ModelError::DanglingMediaRef(preview.node_id()));
                    }
                    check_domain(
                        (0..=MAX_EMU).contains(&object.extent.width_emu),
                        "embeddedObject.extent.width",
                    )?;
                    check_domain(
                        (0..=MAX_EMU).contains(&object.extent.height_emu),
                        "embeddedObject.extent.height",
                    )?;
                    if let Some(prog_id) = &object.prog_id {
                        check_domain(
                            !prog_id.is_empty() && prog_id.len() <= 255,
                            "embeddedObject.progId",
                        )?;
                    }
                    previous_run_properties = None;
                }
                InlineNode::Hyperlink(link) => {
                    if in_wrapper {
                        return Err(ModelError::NestedHyperlink(link.id));
                    }
                    check_hyperlink_target(&link.target)?;
                    if let Some(tooltip) = &link.tooltip {
                        check_domain(
                            !tooltip.is_empty() && tooltip.len() <= 255,
                            "hyperlink.tooltip",
                        )?;
                    }
                    if link.inlines.is_empty() {
                        return Err(ModelError::EmptyHyperlink(link.id));
                    }
                    self.validate_inlines(
                        &link.inlines,
                        link.id,
                        true,
                        textbox_depth,
                        revision_depth,
                        sdt_depth,
                    )?;
                    previous_run_properties = None;
                }
                InlineNode::Field(field) => {
                    if in_wrapper {
                        return Err(ModelError::NestedField(field.id));
                    }
                    check_domain(
                        !field.instruction.is_empty()
                            && field.instruction.len() <= MAX_FIELD_INSTRUCTION_BYTES,
                        "field.instruction",
                    )?;
                    validate_field_kind(&field.kind)?;
                    check_form_field(field)?;
                    // A field's cached result may be empty; when present it is
                    // validated as leaf inlines (in_wrapper rejects any wrapper).
                    self.validate_inlines(
                        &field.inlines,
                        field.id,
                        true,
                        textbox_depth,
                        revision_depth,
                        sdt_depth,
                    )?;
                    previous_run_properties = None;
                }
                InlineNode::TextBox(text_box) => {
                    if textbox_depth + 1 > MAX_TEXTBOX_DEPTH {
                        return Err(ModelError::TextBoxNestingTooDeep(text_box.id));
                    }
                    if text_box.blocks.is_empty() {
                        return Err(ModelError::EmptyTextBox(text_box.id));
                    }
                    // A floating text box carries an anchor + extent; validate them
                    // exactly like an anchored drawing's. An inline text box leaves
                    // them `None` and is unaffected.
                    if let Some(anchor) = &text_box.anchor {
                        check_anchor_offset(
                            &anchor.horizontal.position,
                            &anchor.vertical.position,
                        )?;
                        check_wrap_distances(&anchor.wrap_distances)?;
                    }
                    if let Some(extent) = &text_box.extent {
                        check_domain(
                            (0..=MAX_EMU).contains(&extent.width_emu),
                            "textBox.extent.width",
                        )?;
                        check_domain(
                            (0..=MAX_EMU).contains(&extent.height_emu),
                            "textBox.extent.height",
                        )?;
                    }
                    check_text_box_body_properties(
                        &text_box.body_properties,
                        "textBox.bodyProperties",
                    )?;
                    // A text box is a fresh block container: its table budget
                    // restarts at 0, matching the importer (which gives each box a
                    // fresh table stack). Threading the enclosing `table_depth`
                    // here would reject documents the importer accepts and fail the
                    // whole import. Text-box nesting bounds the recursion instead.
                    for block in &text_box.blocks {
                        self.validate_block(block, 0, textbox_depth + 1, sdt_depth)?;
                    }
                    previous_run_properties = None;
                }
                InlineNode::Group(group) => {
                    self.validate_group(group, 0, textbox_depth, sdt_depth)?;
                    previous_run_properties = None;
                }
                InlineNode::NoteReference(note) => {
                    let resolved = match note.kind {
                        NoteKind::Footnote => self.definitions.footnotes.contains_key(&note.note),
                        NoteKind::Endnote => self.definitions.endnotes.contains_key(&note.note),
                    };
                    if !resolved {
                        return Err(ModelError::DanglingNoteRef(note.note.node_id()));
                    }
                    previous_run_properties = None;
                }
                // A note auto-number mark is an inert leaf carrying only its own
                // run formatting (like a symbol); it prints the enclosing note's
                // number and resolves against no definition.
                InlineNode::NoteNumberMark(mark) => {
                    self.check_run_property_refs(&mark.properties)?;
                    previous_run_properties = None;
                }
                InlineNode::CommentReference(reference) => {
                    if !self.definitions.comments.contains_key(&reference.comment) {
                        return Err(ModelError::DanglingCommentRef(reference.comment.node_id()));
                    }
                    previous_run_properties = None;
                }
                InlineNode::Revision(revision) => {
                    if revision_depth + 1 > MAX_REVISION_DEPTH {
                        return Err(ModelError::RevisionNestingTooDeep(revision.id));
                    }
                    if revision.inlines.is_empty() {
                        return Err(ModelError::EmptyRevision(revision.id));
                    }
                    if let Some(author) = &revision.author {
                        check_domain(
                            !author.is_empty() && author.len() <= 255,
                            "revision.metadata",
                        )?;
                    }
                    // The `w:id` (a producer-local grouping key) and the date are
                    // short: both are bounded at 64 bytes, matching the importer's
                    // capture filter and the design contract.
                    for value in [&revision.date, &revision.revision_id]
                        .into_iter()
                        .flatten()
                    {
                        check_domain(!value.is_empty() && value.len() <= 64, "revision.date")?;
                    }
                    // A revision is a transparent range marker: `in_wrapper` passes
                    // through unchanged (it may wrap a hyperlink/field at top level,
                    // and may itself sit inside one), and only nested revisions bump
                    // `revision_depth`.
                    self.validate_inlines(
                        &revision.inlines,
                        revision.id,
                        in_wrapper,
                        textbox_depth,
                        revision_depth + 1,
                        sdt_depth,
                    )?;
                    previous_run_properties = None;
                }
                InlineNode::Sdt(sdt) => {
                    if sdt_depth + 1 > MAX_SDT_DEPTH {
                        return Err(ModelError::SdtNestingTooDeep(sdt.id));
                    }
                    if sdt.inlines.is_empty() {
                        return Err(ModelError::EmptySdt(sdt.id));
                    }
                    check_sdt_properties(&sdt.properties)?;
                    // An inline sdt is a transparent range wrapper (like a
                    // revision): `in_wrapper` passes through unchanged (it may wrap
                    // a hyperlink/field and may itself sit inside one), and only
                    // nested sdts bump `sdt_depth`.
                    self.validate_inlines(
                        &sdt.inlines,
                        sdt.id,
                        in_wrapper,
                        textbox_depth,
                        revision_depth,
                        sdt_depth + 1,
                    )?;
                    previous_run_properties = None;
                }
                // Bookmark markers are inert leaves (like a tab): transparent to
                // `in_wrapper`/`textbox_depth`/`revision_depth`. Each verifies its
                // definition resolves and forms a hard merge boundary so two equal
                // runs separated by a marker are not merged (position-preserving).
                // Two arms, not an or-pattern: `BookmarkStart`/`BookmarkEnd` are
                // distinct types, so one binding cannot cover both.
                InlineNode::BookmarkStart(marker) => {
                    if !self.definitions.bookmarks.contains_key(&marker.bookmark) {
                        return Err(ModelError::DanglingBookmarkRef(marker.bookmark.node_id()));
                    }
                    previous_run_properties = None;
                }
                InlineNode::BookmarkEnd(marker) => {
                    if !self.definitions.bookmarks.contains_key(&marker.bookmark) {
                        return Err(ModelError::DanglingBookmarkRef(marker.bookmark.node_id()));
                    }
                    previous_run_properties = None;
                }
                // Move range markers are inert leaves (like bookmark markers):
                // transparent to `in_wrapper`/`textbox_depth`/`revision_depth` and
                // a hard merge boundary. The pairing id and name are opaque bounded
                // tokens (id/date <= 64 bytes, name/author <= 255) carried verbatim
                // from the producer, mirroring `Revision` metadata. The start/end
                // pairing is self-contained (the shared `move_id` token), so no
                // definition-table lookup is required.
                InlineNode::MoveRangeStart(marker) => {
                    check_domain(
                        !marker.move_id.is_empty() && marker.move_id.len() <= 64,
                        "moveRange.moveId",
                    )?;
                    check_domain(
                        !marker.name.is_empty() && marker.name.len() <= 255,
                        "moveRange.name",
                    )?;
                    if let Some(author) = &marker.author {
                        check_domain(
                            !author.is_empty() && author.len() <= 255,
                            "moveRange.author",
                        )?;
                    }
                    if let Some(date) = &marker.date {
                        check_domain(!date.is_empty() && date.len() <= 64, "moveRange.date")?;
                    }
                    previous_run_properties = None;
                }
                InlineNode::MoveRangeEnd(marker) => {
                    check_domain(
                        !marker.move_id.is_empty() && marker.move_id.len() <= 64,
                        "moveRange.moveId",
                    )?;
                    previous_run_properties = None;
                }
                // Math is an inert inline leaf at the document tree level. Its
                // retained XML and optional typed projection are independently
                // bounded so hostile snapshots cannot build an unbounded AST.
                InlineNode::Math(math) => {
                    check_domain(
                        !math.omml.is_empty() && math.omml.len() <= MAX_MATH_BYTES,
                        "math.omml",
                    )?;
                    check_domain(math.text.len() <= MAX_MATH_BYTES, "math.text")?;
                    if let Some(expression) = &math.expression {
                        validate_math_expression(expression)?;
                    }
                    previous_run_properties = None;
                }
                // A symbol is an inert leaf: the font name is a non-empty,
                // length-bounded face name; the code point is unconstrained.
                InlineNode::Symbol(symbol) => {
                    check_domain(
                        !symbol.font.is_empty() && symbol.font.len() <= MAX_SYMBOL_FONT_LEN,
                        "symbol.font",
                    )?;
                    self.check_run_property_refs(&symbol.properties)?;
                    previous_run_properties = None;
                }
                // Comment range markers are inert leaves (like a tab): a zero-width
                // point that carries no validated payload. Each forms a hard merge
                // boundary so two equal runs separated by a marker are not merged
                // (position-preserving). The commented span's comment resolves
                // through the paired `CommentReference`, so no lookup is repeated
                // here.
                // The hyphen glyphs and a positional tab are likewise inert leaves:
                // a non-breaking/soft hyphen carries only its identity, and a
                // positional tab's alignment/relativeTo/leader are typed enums that
                // cannot hold an out-of-domain value. Each forms a hard merge
                // boundary (like a tab), so `previous_run_properties` resets.
                // A horizontal rule is an inert leaf whose fields are typed and
                // bounded (an align enum, a per-mille width, a positive thickness,
                // an RGBA color); the width is clamped to a valid fraction so it
                // cannot hold an out-of-domain value. It forms a hard merge boundary.
                InlineNode::HorizontalRule(rule) => {
                    check_domain(
                        rule.width_permille >= 1 && rule.width_permille <= HR_FULL_WIDTH_PERMILLE,
                        "horizontalRule.widthPermille",
                    )?;
                    check_domain(rule.thickness_emu > 0, "horizontalRule.thicknessEmu")?;
                    previous_run_properties = None;
                }
                InlineNode::Tab(_)
                | InlineNode::Break(_)
                | InlineNode::CommentRangeStart(_)
                | InlineNode::CommentRangeEnd(_)
                | InlineNode::NoBreakHyphen(_)
                | InlineNode::SoftHyphen(_)
                | InlineNode::PositionalTab(_) => {
                    previous_run_properties = None;
                }
            }
        }
        Ok(())
    }

    /// Validates a DrawingML group and its children recursively: extent/offset
    /// domains, media references, outline widths, and each text box's block
    /// content (a fresh block container, like an inline text box). `group_depth`
    /// bounds nesting; `textbox_depth`/`sdt_depth` thread the enclosing recursion
    /// budgets into a child text box's blocks.
    fn validate_group(
        &self,
        group: &WordprocessingGroup,
        group_depth: u32,
        textbox_depth: u32,
        sdt_depth: u32,
    ) -> Result<(), ModelError> {
        if group_depth > MAX_GROUP_DEPTH {
            return Err(ModelError::GroupNestingTooDeep(group.id));
        }
        if let Some(anchor) = &group.anchor {
            check_anchor_offset(&anchor.horizontal.position, &anchor.vertical.position)?;
            check_wrap_distances(&anchor.wrap_distances)?;
        }
        check_extent(&group.extent, "group.extent")?;
        check_extent(&group.transform.extent, "group.transform.extent")?;
        check_extent(&group.transform.child_extent, "group.transform.childExtent")?;
        for child in &group.children {
            match child {
                GroupChild::Picture(picture) => {
                    if !self.definitions.media.contains_key(&picture.media) {
                        return Err(ModelError::DanglingMediaRef(picture.media.node_id()));
                    }
                    check_extent(&picture.extent, "group.picture.extent")?;
                    if let Some(descr) = &picture.descr {
                        check_domain(
                            !descr.is_empty() && descr.len() <= MAX_DESCR_BYTES,
                            "group.picture.descr",
                        )?;
                    }
                }
                GroupChild::TextBox(text_box) => {
                    if textbox_depth + 1 > MAX_TEXTBOX_DEPTH {
                        return Err(ModelError::TextBoxNestingTooDeep(text_box.id));
                    }
                    if text_box.blocks.is_empty() {
                        return Err(ModelError::EmptyTextBox(text_box.id));
                    }
                    check_extent(&text_box.extent, "group.textBox.extent")?;
                    check_text_box_body_properties(
                        &text_box.body_properties,
                        "group.textBox.bodyProperties",
                    )?;
                    if let Some(border) = &text_box.border {
                        check_domain(
                            (0..=MAX_EMU).contains(&border.width_emu),
                            "group.textBox.border.width",
                        )?;
                    }
                    for block in &text_box.blocks {
                        self.validate_block(block, 0, textbox_depth + 1, sdt_depth)?;
                    }
                }
                GroupChild::Shape(shape) => {
                    check_extent(&shape.extent, "group.shape.extent")?;
                    if let Some(preset) = &shape.preset {
                        check_domain(
                            shape.geometry == ShapeGeometry::Other
                                && !preset.is_empty()
                                && preset.len() <= MAX_SHAPE_PRESET_BYTES,
                            "group.shape.preset",
                        )?;
                    }
                    check_domain(
                        shape.adjustments.len() <= MAX_SHAPE_ADJUSTMENTS,
                        "group.shape.adjustments",
                    )?;
                    for adjustment in &shape.adjustments {
                        check_domain(
                            !adjustment.name.is_empty()
                                && adjustment.name.len() <= MAX_SHAPE_GUIDE_NAME_BYTES,
                            "group.shape.adjustment.name",
                        )?;
                        check_domain(
                            !adjustment.formula.is_empty()
                                && adjustment.formula.len() <= MAX_SHAPE_FORMULA_BYTES,
                            "group.shape.adjustment.formula",
                        )?;
                    }
                    if let Some(stroke) = &shape.stroke {
                        check_domain(
                            (0..=MAX_EMU).contains(&stroke.width_emu),
                            "group.shape.stroke.width",
                        )?;
                    }
                }
                GroupChild::Group(nested) => {
                    self.validate_group(nested, group_depth + 1, textbox_depth, sdt_depth)?;
                }
            }
        }
        Ok(())
    }

    /// Validates every footnote and endnote's block content. A note is a fresh
    /// block container (table/text-box depth restart at 0).
    fn validate_notes(&self) -> Result<(), ModelError> {
        for (_, note) in self.definitions.footnotes.iter() {
            for block in &note.blocks {
                self.validate_block(block, 0, 0, 0)?;
            }
        }
        for (_, note) in self.definitions.endnotes.iter() {
            for block in &note.blocks {
                self.validate_block(block, 0, 0, 0)?;
            }
        }
        Ok(())
    }

    /// Validates every header and footer's block content (a fresh block
    /// container, like a note).
    fn validate_headers_footers(&self) -> Result<(), ModelError> {
        for (_, header_footer) in self
            .definitions
            .headers
            .iter()
            .chain(self.definitions.footers.iter())
        {
            for block in &header_footer.blocks {
                self.validate_block(block, 0, 0, 0)?;
            }
        }
        Ok(())
    }

    /// Validates every comment's block content and metadata (a fresh block
    /// container, like a note).
    fn validate_comments(&self) -> Result<(), ModelError> {
        for (_, comment) in self.definitions.comments.iter() {
            for value in [&comment.author, &comment.initials].into_iter().flatten() {
                check_domain(!value.is_empty() && value.len() <= 255, "comment.metadata")?;
            }
            if let Some(date) = &comment.date {
                check_domain(!date.is_empty() && date.len() <= 64, "comment.date")?;
            }
            // Threading/identity join ids are bounded hex tokens; the person link
            // is a bounded author-name key into the identity table.
            for value in [
                &comment.para_id,
                &comment.parent_para_id,
                &comment.durable_id,
            ]
            .into_iter()
            .flatten()
            {
                check_domain(!value.is_empty() && value.len() <= 64, "comment.threadId")?;
            }
            if let Some(person) = &comment.person {
                check_domain(!person.is_empty() && person.len() <= 255, "comment.person")?;
            }
            for block in &comment.blocks {
                self.validate_block(block, 0, 0, 0)?;
            }
        }
        Ok(())
    }

    /// Validates the collaborator identity table (`word/people.xml`): a non-empty
    /// bounded author name plus bounded presence-provider fields.
    fn validate_people(&self) -> Result<(), ModelError> {
        for person in &self.definitions.people {
            check_domain(
                !person.author.is_empty() && person.author.len() <= 255,
                "person.author",
            )?;
            if let Some(presence) = &person.presence {
                check_domain(presence.provider_id.len() <= 255, "person.providerId")?;
                check_domain(presence.user_id.len() <= 255, "person.userId")?;
            }
        }
        Ok(())
    }

    /// Validates every bookmark definition's name domain. Marker-to-definition
    /// integrity is checked in `validate_inlines`; internal-hyperlink anchor
    /// resolution remains lax (forward/cross-part/well-known targets).
    fn validate_bookmarks(&self) -> Result<(), ModelError> {
        for (_, bookmark) in self.definitions.bookmarks.iter() {
            check_domain(
                !bookmark.name.is_empty() && bookmark.name.len() <= 255,
                "bookmark.name",
            )?;
        }
        Ok(())
    }

    fn validate_snapshot_limits(&self, limits: SnapshotLimits) -> Result<(), SnapshotError> {
        let mut blocks = 0_usize;
        let mut scalar_values = 0_usize;
        for block in &self.body {
            accumulate_block_limits(block, limits, &mut blocks, &mut scalar_values)?;
        }
        for (_, note) in self.definitions.footnotes.iter() {
            for block in &note.blocks {
                accumulate_block_limits(block, limits, &mut blocks, &mut scalar_values)?;
            }
        }
        for (_, note) in self.definitions.endnotes.iter() {
            for block in &note.blocks {
                accumulate_block_limits(block, limits, &mut blocks, &mut scalar_values)?;
            }
        }
        for (_, header_footer) in self
            .definitions
            .headers
            .iter()
            .chain(self.definitions.footers.iter())
        {
            for block in &header_footer.blocks {
                accumulate_block_limits(block, limits, &mut blocks, &mut scalar_values)?;
            }
        }
        for (_, comment) in self.definitions.comments.iter() {
            for block in &comment.blocks {
                accumulate_block_limits(block, limits, &mut blocks, &mut scalar_values)?;
            }
        }
        enforce_limit("body_blocks", blocks, limits.max_blocks)?;
        enforce_limit(
            "unicode_scalar_values",
            scalar_values,
            limits.max_unicode_scalar_values,
        )
    }
}

/// Accounts one block against the block-count and text limits, recursing through
/// table cells so nested paragraphs and tables cannot smuggle past the bounds.
fn accumulate_block_limits(
    block: &BlockNode,
    limits: SnapshotLimits,
    blocks: &mut usize,
    scalar_values: &mut usize,
) -> Result<(), SnapshotError> {
    *blocks = blocks.checked_add(1).ok_or(SnapshotError::LimitExceeded {
        limit: "body_blocks",
        observed: usize::MAX,
        allowed: limits.max_blocks,
    })?;
    match block {
        BlockNode::Paragraph(paragraph) => {
            for inline in &paragraph.inlines {
                accumulate_inline_limits(inline, limits, blocks, scalar_values)?;
            }
        }
        BlockNode::Table(table) => {
            for row in &table.rows {
                *blocks = blocks.checked_add(1).ok_or(SnapshotError::LimitExceeded {
                    limit: "body_blocks",
                    observed: usize::MAX,
                    allowed: limits.max_blocks,
                })?;
                for cell in &row.cells {
                    *blocks = blocks.checked_add(1).ok_or(SnapshotError::LimitExceeded {
                        limit: "body_blocks",
                        observed: usize::MAX,
                        allowed: limits.max_blocks,
                    })?;
                    for nested in &cell.blocks {
                        accumulate_block_limits(nested, limits, blocks, scalar_values)?;
                    }
                }
            }
        }
        BlockNode::Sdt(sdt) => {
            for nested in &sdt.blocks {
                accumulate_block_limits(nested, limits, blocks, scalar_values)?;
            }
        }
        // An alt chunk is a leaf block (already counted above); it holds no inline
        // content or scalar values.
        BlockNode::AltChunk(_) => {}
    }
    Ok(())
}

/// Accounts one inline node against the text and block limits, recursing into
/// hyperlink/field children and text-box blocks so nested content cannot smuggle
/// past the bounds.
fn accumulate_inline_limits(
    inline: &InlineNode,
    limits: SnapshotLimits,
    blocks: &mut usize,
    scalar_values: &mut usize,
) -> Result<(), SnapshotError> {
    match inline {
        InlineNode::Run(run) => {
            enforce_limit("text_run_bytes", run.text.len(), limits.max_text_run_bytes)?;
            *scalar_values = scalar_values.checked_add(run.text.chars().count()).ok_or(
                SnapshotError::LimitExceeded {
                    limit: "unicode_scalar_values",
                    observed: usize::MAX,
                    allowed: limits.max_unicode_scalar_values,
                },
            )?;
        }
        InlineNode::Hyperlink(link) => {
            for child in &link.inlines {
                accumulate_inline_limits(child, limits, blocks, scalar_values)?;
            }
        }
        InlineNode::Field(field) => {
            enforce_limit(
                "field_instruction_bytes",
                field.instruction.len(),
                MAX_FIELD_INSTRUCTION_BYTES,
            )?;
            for child in &field.inlines {
                accumulate_inline_limits(child, limits, blocks, scalar_values)?;
            }
        }
        InlineNode::TextBox(text_box) => {
            for block in &text_box.blocks {
                accumulate_block_limits(block, limits, blocks, scalar_values)?;
            }
        }
        InlineNode::Group(group) => {
            accumulate_group_limits(group, limits, blocks, scalar_values)?;
        }
        InlineNode::Revision(revision) => {
            for child in &revision.inlines {
                accumulate_inline_limits(child, limits, blocks, scalar_values)?;
            }
        }
        InlineNode::Sdt(sdt) => {
            for child in &sdt.inlines {
                accumulate_inline_limits(child, limits, blocks, scalar_values)?;
            }
        }
        InlineNode::Math(math) => {
            enforce_limit("math_omml_bytes", math.omml.len(), MAX_MATH_BYTES)?;
            enforce_limit("math_text_bytes", math.text.len(), MAX_MATH_BYTES)?;
            if let Some(expression) = &math.expression {
                let (nodes, text_bytes) = math_expression_size(expression);
                enforce_limit("math_expression_nodes", nodes, MAX_MATH_NODES)?;
                enforce_limit("math_expression_text_bytes", text_bytes, MAX_MATH_BYTES)?;
            }
        }
        InlineNode::Tab(_)
        | InlineNode::Break(_)
        | InlineNode::Drawing(_)
        | InlineNode::AnchoredDrawing(_)
        | InlineNode::EmbeddedObject(_)
        | InlineNode::NoteReference(_)
        | InlineNode::NoteNumberMark(_)
        | InlineNode::CommentReference(_)
        | InlineNode::CommentRangeStart(_)
        | InlineNode::CommentRangeEnd(_)
        | InlineNode::BookmarkStart(_)
        | InlineNode::BookmarkEnd(_)
        | InlineNode::MoveRangeStart(_)
        | InlineNode::MoveRangeEnd(_)
        | InlineNode::Symbol(_)
        | InlineNode::HorizontalRule(_)
        | InlineNode::NoBreakHyphen(_)
        | InlineNode::SoftHyphen(_)
        | InlineNode::PositionalTab(_) => {}
    }
    Ok(())
}

fn insert_id(ids: &mut BTreeSet<NodeId>, id: NodeId) -> Result<(), ModelError> {
    if ids.insert(id) {
        Ok(())
    } else {
        Err(ModelError::DuplicateNodeId(id))
    }
}

/// Records a block's ids, recursing through table rows, cells, and nested blocks.
fn record_block_ids(block: &BlockNode, ids: &mut BTreeSet<NodeId>) -> Result<(), ModelError> {
    match block {
        BlockNode::Paragraph(paragraph) => {
            insert_id(ids, paragraph.id)?;
            for inline in &paragraph.inlines {
                record_inline_ids(inline, ids)?;
            }
        }
        BlockNode::Table(table) => {
            insert_id(ids, table.id)?;
            for row in &table.rows {
                insert_id(ids, row.id)?;
                for cell in &row.cells {
                    insert_id(ids, cell.id)?;
                    for nested in &cell.blocks {
                        record_block_ids(nested, ids)?;
                    }
                }
            }
        }
        BlockNode::Sdt(sdt) => {
            insert_id(ids, sdt.id)?;
            for nested in &sdt.blocks {
                record_block_ids(nested, ids)?;
            }
        }
        BlockNode::AltChunk(chunk) => {
            insert_id(ids, chunk.id)?;
        }
    }
    Ok(())
}

/// Records an inline's id and, for a wrapper (hyperlink or field) or a text box,
/// its children's ids recursively.
fn record_inline_ids(inline: &InlineNode, ids: &mut BTreeSet<NodeId>) -> Result<(), ModelError> {
    insert_id(ids, inline.id())?;
    match inline {
        InlineNode::Hyperlink(link) => {
            for child in &link.inlines {
                record_inline_ids(child, ids)?;
            }
        }
        InlineNode::Field(field) => {
            for child in &field.inlines {
                record_inline_ids(child, ids)?;
            }
        }
        InlineNode::TextBox(text_box) => {
            for block in &text_box.blocks {
                record_block_ids(block, ids)?;
            }
        }
        InlineNode::Group(group) => {
            record_group_ids(group, ids)?;
        }
        InlineNode::Revision(revision) => {
            for child in &revision.inlines {
                record_inline_ids(child, ids)?;
            }
        }
        InlineNode::Sdt(sdt) => {
            for child in &sdt.inlines {
                record_inline_ids(child, ids)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Records a group's children's ids recursively (a picture/text box/shape id, a
/// text box's block ids, and a nested group's own id + children). The group's OWN
/// id is recorded by the caller ([`record_inline_ids`] via `inline.id()`, or the
/// parent group for a nested one), so it is not re-inserted here.
fn record_group_ids(
    group: &WordprocessingGroup,
    ids: &mut BTreeSet<NodeId>,
) -> Result<(), ModelError> {
    for child in &group.children {
        match child {
            GroupChild::Picture(picture) => insert_id(ids, picture.id)?,
            GroupChild::TextBox(text_box) => {
                insert_id(ids, text_box.id)?;
                for block in &text_box.blocks {
                    record_block_ids(block, ids)?;
                }
            }
            GroupChild::Shape(shape) => insert_id(ids, shape.id)?,
            GroupChild::Group(nested) => {
                insert_id(ids, nested.id)?;
                record_group_ids(nested, ids)?;
            }
        }
    }
    Ok(())
}

/// Accounts a group's block/scalar content (its text boxes' blocks, recursively
/// through nested groups) against the snapshot limits.
fn accumulate_group_limits(
    group: &WordprocessingGroup,
    limits: SnapshotLimits,
    blocks: &mut usize,
    scalar_values: &mut usize,
) -> Result<(), SnapshotError> {
    for child in &group.children {
        match child {
            GroupChild::TextBox(text_box) => {
                for block in &text_box.blocks {
                    accumulate_block_limits(block, limits, blocks, scalar_values)?;
                }
            }
            GroupChild::Group(nested) => {
                accumulate_group_limits(nested, limits, blocks, scalar_values)?;
            }
            GroupChild::Shape(shape) => {
                if let Some(preset) = &shape.preset {
                    add_scalar_values(preset, limits, scalar_values)?;
                }
                for adjustment in &shape.adjustments {
                    add_scalar_values(&adjustment.name, limits, scalar_values)?;
                    add_scalar_values(&adjustment.formula, limits, scalar_values)?;
                }
            }
            GroupChild::Picture(_) => {}
        }
    }
    Ok(())
}

fn add_scalar_values(
    value: &str,
    limits: SnapshotLimits,
    scalar_values: &mut usize,
) -> Result<(), SnapshotError> {
    *scalar_values =
        scalar_values
            .checked_add(value.chars().count())
            .ok_or(SnapshotError::LimitExceeded {
                limit: "unicode_scalar_values",
                observed: usize::MAX,
                allowed: limits.max_unicode_scalar_values,
            })?;
    Ok(())
}

fn check_hyperlink_target(target: &HyperlinkTarget) -> Result<(), ModelError> {
    match target {
        HyperlinkTarget::External(external) => check_domain(
            !external.url.is_empty() && external.url.len() <= 2048,
            "hyperlink.external.url",
        ),
        HyperlinkTarget::Internal(internal) => check_domain(
            !internal.anchor.is_empty() && internal.anchor.len() <= 255,
            "hyperlink.internal.anchor",
        ),
    }
}

/// Validates a content control's typed properties: `alias`/`tag` are non-empty
/// and at most 255 bytes; the retained producer `control_id` is non-empty and at
/// most 64 bytes (its opaque grouping-key bound, matching the importer filter);
/// the placeholder ref, data binding, and control-specific data are bounded and
/// (for `data`) consistent with `control_kind`.
fn check_sdt_properties(properties: &SdtProperties) -> Result<(), ModelError> {
    if let Some(alias) = &properties.alias {
        check_domain(!alias.is_empty() && alias.len() <= 255, "sdt.alias")?;
    }
    if let Some(tag) = &properties.tag {
        check_domain(!tag.is_empty() && tag.len() <= 255, "sdt.tag")?;
    }
    if let Some(control_id) = &properties.control_id {
        check_domain(!control_id.is_empty() && control_id.len() <= 64, "sdt.id")?;
    }
    if let Some(placeholder) = &properties.placeholder {
        check_domain(
            !placeholder.is_empty() && placeholder.len() <= 255,
            "sdt.placeholder",
        )?;
    }
    if let Some(binding) = &properties.data_binding {
        check_domain(
            !binding.xpath.is_empty() && binding.xpath.len() <= 1024,
            "sdt.dataBinding.xpath",
        )?;
        check_opt_bound(
            binding.store_item_id.as_deref(),
            128,
            "sdt.dataBinding.storeItemID",
        )?;
        check_opt_bound(
            binding.prefix_mappings.as_deref(),
            1024,
            "sdt.dataBinding.prefixMappings",
        )?;
    }
    if let Some(data) = &properties.data {
        check_sdt_data(properties.control_kind, data)?;
    }
    Ok(())
}

/// Validates the control-specific `data`: it must agree with `control_kind`, and
/// every carried string is bounded.
fn check_sdt_data(kind: Option<SdtControlKind>, data: &SdtControlData) -> Result<(), ModelError> {
    match data {
        SdtControlData::List(items) => {
            check_domain(
                matches!(
                    kind,
                    Some(SdtControlKind::ComboBox | SdtControlKind::DropDownList)
                ),
                "sdt.data.list",
            )?;
            check_domain(items.len() <= 1024, "sdt.data.list")?;
            for item in items {
                check_opt_bound(item.display.as_deref(), 255, "sdt.data.list.display")?;
                check_domain(item.value.len() <= 255, "sdt.data.list.value")?;
            }
        }
        SdtControlData::Date(date) => {
            check_domain(kind == Some(SdtControlKind::Date), "sdt.data.date")?;
            check_opt_bound(date.full_date.as_deref(), 64, "sdt.data.date.fullDate")?;
            check_opt_bound(date.date_format.as_deref(), 255, "sdt.data.date.dateFormat")?;
            check_opt_bound(date.calendar.as_deref(), 64, "sdt.data.date.calendar")?;
            check_opt_bound(date.lid.as_deref(), 64, "sdt.data.date.lid")?;
            check_opt_bound(
                date.store_mapped_as.as_deref(),
                64,
                "sdt.data.date.storeMappedAs",
            )?;
        }
        SdtControlData::Checkbox(checkbox) => {
            check_domain(kind == Some(SdtControlKind::Checkbox), "sdt.data.checkbox")?;
            for symbol in [&checkbox.checked_state, &checkbox.unchecked_state]
                .into_iter()
                .flatten()
            {
                check_domain(
                    !symbol.val.is_empty() && symbol.val.len() <= 8,
                    "sdt.data.checkbox.val",
                )?;
                check_opt_bound(symbol.font.as_deref(), 64, "sdt.data.checkbox.font")?;
            }
        }
    }
    Ok(())
}

/// Bounds an optional string to non-empty and at most `max` bytes when present.
fn check_opt_bound(
    value: Option<&str>,
    max: usize,
    property: &'static str,
) -> Result<(), ModelError> {
    if let Some(value) = value {
        check_domain(!value.is_empty() && value.len() <= max, property)?;
    }
    Ok(())
}

/// Validates a legacy form field's configuration (`w:ffData`): every present
/// string is at most `MAX_FORM_FIELD_STRING_BYTES`, a drop-down carries at most
/// `MAX_FORM_FIELD_ENTRIES` entries, and the kind-specific payload agrees with
/// the field instruction's `FORM…` token (a `TextInput` payload only on a
/// FORMTEXT field, and so on). Absent (`None`) for an ordinary field.
/// Bounds the strings carried by a [`FieldKind`] projection. Each is derived
/// from the (already length-bounded) instruction, so this only guards against a
/// hand-built model whose kind strings exceed the instruction ceiling.
fn validate_field_kind(kind: &FieldKind) -> Result<(), ModelError> {
    let within = |value: &str| value.len() <= MAX_FIELD_INSTRUCTION_BYTES;
    let ok = match kind {
        FieldKind::Page | FieldKind::NumPages | FieldKind::Toc => true,
        FieldKind::Date { format } | FieldKind::Time { format } => {
            format.as_deref().is_none_or(within)
        }
        FieldKind::Ref { bookmark } | FieldKind::PageRef { bookmark } => within(bookmark),
        FieldKind::Seq { name } => within(name),
        FieldKind::StyleRef { style } => within(style),
        FieldKind::Hyperlink { target } => target.as_deref().is_none_or(within),
        FieldKind::Other { keyword } => within(keyword),
    };
    check_domain(ok, "field.kind")
}

fn check_form_field(field: &Field) -> Result<(), ModelError> {
    let Some(form) = &field.form else {
        return Ok(());
    };
    // Common strings are length-bounded; empty is permitted (an explicit empty
    // value round-trips, and a blank drop-down entry is legitimate).
    for value in [
        &form.name,
        &form.help_text,
        &form.status_text,
        &form.entry_macro,
        &form.exit_macro,
    ]
    .into_iter()
    .flatten()
    {
        check_domain(
            value.len() <= MAX_FORM_FIELD_STRING_BYTES,
            "field.form.string",
        )?;
    }
    // The payload variant must match the instruction's field token.
    let token = field
        .instruction
        .split_whitespace()
        .next()
        .unwrap_or_default();
    let expected = match form.kind {
        FormFieldKind::TextInput(_) => "FORMTEXT",
        FormFieldKind::CheckBox(_) => "FORMCHECKBOX",
        FormFieldKind::DropDown(_) => "FORMDROPDOWN",
    };
    check_domain(token.eq_ignore_ascii_case(expected), "field.form.kind")?;
    match &form.kind {
        FormFieldKind::TextInput(text) => {
            if let Some(default) = &text.default {
                check_domain(
                    default.len() <= MAX_FORM_FIELD_STRING_BYTES,
                    "field.form.textInput.default",
                )?;
            }
            if let Some(format) = &text.format {
                check_domain(
                    format.len() <= MAX_FORM_FIELD_STRING_BYTES,
                    "field.form.textInput.format",
                )?;
            }
        }
        FormFieldKind::CheckBox(_) => {}
        FormFieldKind::DropDown(list) => {
            check_domain(
                list.entries.len() <= MAX_FORM_FIELD_ENTRIES,
                "field.form.ddList.entries",
            )?;
            for entry in &list.entries {
                check_domain(
                    entry.len() <= MAX_FORM_FIELD_STRING_BYTES,
                    "field.form.ddList.entry",
                )?;
            }
        }
    }
    Ok(())
}

/// Validates a footnote/endnote property container's numbering bounds (shared by
/// per-section `w:footnotePr`/`w:endnotePr` and the document-default settings pair).
fn check_note_props(props: &NoteProperties) -> Result<(), ModelError> {
    if let Some(NumberFormat::Other(token)) = &props.number_format {
        check_domain(
            !token.is_empty() && token.len() <= 64,
            "note_props.number_format",
        )?;
    }
    if let Some(start) = props.number_start {
        check_domain((0..=1_000_000).contains(&start), "note_props.number_start")?;
    }
    Ok(())
}

/// Validates a section's own value domains (page geometry, columns, page numbering,
/// grid, borders, line numbering, paper source, note props) — the bounds that hold
/// for a real section AND for a `w:sectPrChange` prior snapshot. Header/footer
/// reference resolution is validated separately (only for real sections).
fn check_section_domains(section: &SectionBoundary) -> Result<(), ModelError> {
    check_domain(
        (1..=31_680).contains(&section.page_size.width_twips),
        "section.page_size.width",
    )?;
    check_domain(
        (1..=31_680).contains(&section.page_size.height_twips),
        "section.page_size.height",
    )?;
    for margin in [
        section.page_margins.top_twips,
        section.page_margins.bottom_twips,
        section.page_margins.start_twips,
        section.page_margins.end_twips,
    ] {
        check_domain((0..=31_680).contains(&margin), "section.page_margins")?;
    }
    check_domain(
        (1..=64).contains(&section.columns.count),
        "section.column_count",
    )?;
    if let Some(space) = section.columns.space_twips {
        check_domain((0..=31_680).contains(&space), "section.column_space")?;
    }
    if let Some(NumberFormat::Other(token)) = &section.page_numbering.format {
        check_domain(
            !token.is_empty() && token.len() <= 64,
            "section.page_numbering.format",
        )?;
    }
    if let Some(start) = section.page_numbering.start {
        check_domain(
            (0..=1_000_000).contains(&start),
            "section.page_numbering.start",
        )?;
    }
    for value in [section.doc_grid.line_pitch, section.doc_grid.char_space]
        .into_iter()
        .flatten()
    {
        check_domain((0..=31_680).contains(&value), "section.doc_grid")?;
    }
    check_page_borders(&section.page_borders)?;
    for value in [
        section.line_numbering.count_by,
        section.line_numbering.start,
    ]
    .into_iter()
    .flatten()
    {
        check_domain((0..=32_767).contains(&value), "section.line_numbering")?;
    }
    if let Some(distance) = section.line_numbering.distance {
        check_domain(
            (0..=31_680).contains(&distance),
            "section.line_numbering.distance",
        )?;
    }
    for value in [section.paper_source.first, section.paper_source.other]
        .into_iter()
        .flatten()
    {
        check_domain((0..=32_767).contains(&value), "section.paper_source")?;
    }
    for props in [&section.footnote_props, &section.endnote_props] {
        check_note_props(props)?;
    }
    Ok(())
}

fn check_domain(condition: bool, property: &'static str) -> Result<(), ModelError> {
    if condition {
        Ok(())
    } else {
        Err(ModelError::PropertyValueOutOfDomain { property })
    }
}

fn validate_math_expression(expression: &MathExpression) -> Result<(), ModelError> {
    fn visit(
        expression: &MathExpression,
        depth: usize,
        nodes: &mut usize,
        text_bytes: &mut usize,
    ) -> Result<(), ModelError> {
        check_domain(depth <= MAX_MATH_DEPTH, "math.expression.depth")?;
        *nodes = nodes.saturating_add(1);
        check_domain(*nodes <= MAX_MATH_NODES, "math.expression.nodes")?;
        match expression {
            MathExpression::Row { children } => {
                check_domain(!children.is_empty(), "math.expression.row.children")?;
                for child in children {
                    visit(child, depth + 1, nodes, text_bytes)?;
                }
            }
            MathExpression::Text { value } => {
                check_domain(!value.is_empty(), "math.expression.text")?;
                *text_bytes = text_bytes.saturating_add(value.len());
                check_domain(*text_bytes <= MAX_MATH_BYTES, "math.expression.textBytes")?;
            }
            MathExpression::Fraction {
                numerator,
                denominator,
            } => {
                visit(numerator, depth + 1, nodes, text_bytes)?;
                visit(denominator, depth + 1, nodes, text_bytes)?;
            }
            MathExpression::Script {
                base,
                subscript,
                superscript,
            } => {
                check_domain(
                    subscript.is_some() || superscript.is_some(),
                    "math.expression.script",
                )?;
                visit(base, depth + 1, nodes, text_bytes)?;
                if let Some(subscript) = subscript {
                    visit(subscript, depth + 1, nodes, text_bytes)?;
                }
                if let Some(superscript) = superscript {
                    visit(superscript, depth + 1, nodes, text_bytes)?;
                }
            }
            MathExpression::Radical { degree, radicand } => {
                if let Some(degree) = degree {
                    visit(degree, depth + 1, nodes, text_bytes)?;
                }
                visit(radicand, depth + 1, nodes, text_bytes)?;
            }
            MathExpression::Delimiter {
                open,
                close,
                content,
            } => {
                *text_bytes = text_bytes
                    .saturating_add(open.len())
                    .saturating_add(close.len());
                check_domain(*text_bytes <= MAX_MATH_BYTES, "math.expression.textBytes")?;
                visit(content, depth + 1, nodes, text_bytes)?;
            }
            MathExpression::Function { name, argument } => {
                visit(name, depth + 1, nodes, text_bytes)?;
                visit(argument, depth + 1, nodes, text_bytes)?;
            }
            MathExpression::Accent { accent, base } => {
                *text_bytes = text_bytes.saturating_add(accent.len());
                check_domain(*text_bytes <= MAX_MATH_BYTES, "math.expression.textBytes")?;
                visit(base, depth + 1, nodes, text_bytes)?;
            }
            MathExpression::Limit { base, limit, .. } => {
                visit(base, depth + 1, nodes, text_bytes)?;
                visit(limit, depth + 1, nodes, text_bytes)?;
            }
            MathExpression::Nary {
                operator,
                lower,
                upper,
                base,
            } => {
                *text_bytes = text_bytes.saturating_add(operator.len());
                check_domain(*text_bytes <= MAX_MATH_BYTES, "math.expression.textBytes")?;
                if let Some(lower) = lower {
                    visit(lower, depth + 1, nodes, text_bytes)?;
                }
                if let Some(upper) = upper {
                    visit(upper, depth + 1, nodes, text_bytes)?;
                }
                visit(base, depth + 1, nodes, text_bytes)?;
            }
            MathExpression::Matrix { rows } => {
                check_domain(!rows.is_empty(), "math.expression.matrix.rows")?;
                for row in rows {
                    check_domain(!row.cells.is_empty(), "math.expression.matrix.cells")?;
                    for cell in &row.cells {
                        visit(cell, depth + 1, nodes, text_bytes)?;
                    }
                }
            }
            MathExpression::EqArray { rows } => {
                check_domain(!rows.is_empty(), "math.expression.eqArray.rows")?;
                for row in rows {
                    visit(row, depth + 1, nodes, text_bytes)?;
                }
            }
            MathExpression::Bar { base, .. } => {
                visit(base, depth + 1, nodes, text_bytes)?;
            }
            MathExpression::GroupChar {
                character, base, ..
            } => {
                *text_bytes = text_bytes.saturating_add(character.len());
                check_domain(*text_bytes <= MAX_MATH_BYTES, "math.expression.textBytes")?;
                visit(base, depth + 1, nodes, text_bytes)?;
            }
        }
        Ok(())
    }

    let mut nodes = 0;
    let mut text_bytes = 0;
    visit(expression, 1, &mut nodes, &mut text_bytes)
}

fn math_expression_size(expression: &MathExpression) -> (usize, usize) {
    match expression {
        MathExpression::Row { children } => children.iter().fold((1, 0), |acc, child| {
            let child = math_expression_size(child);
            (acc.0.saturating_add(child.0), acc.1.saturating_add(child.1))
        }),
        MathExpression::Text { value } => (1, value.len()),
        MathExpression::Fraction {
            numerator,
            denominator,
        } => {
            let numerator = math_expression_size(numerator);
            let denominator = math_expression_size(denominator);
            (
                1usize
                    .saturating_add(numerator.0)
                    .saturating_add(denominator.0),
                numerator.1.saturating_add(denominator.1),
            )
        }
        MathExpression::Script {
            base,
            subscript,
            superscript,
        } => {
            let mut size = math_expression_size(base);
            size.0 = size.0.saturating_add(1);
            for script in [subscript, superscript].into_iter().flatten() {
                let child = math_expression_size(script);
                size.0 = size.0.saturating_add(child.0);
                size.1 = size.1.saturating_add(child.1);
            }
            size
        }
        MathExpression::Radical { degree, radicand } => {
            let mut size = math_expression_size(radicand);
            size.0 = size.0.saturating_add(1);
            if let Some(degree) = degree {
                let child = math_expression_size(degree);
                size.0 = size.0.saturating_add(child.0);
                size.1 = size.1.saturating_add(child.1);
            }
            size
        }
        MathExpression::Delimiter {
            open,
            close,
            content,
        } => {
            let content = math_expression_size(content);
            (
                content.0.saturating_add(1),
                content
                    .1
                    .saturating_add(open.len())
                    .saturating_add(close.len()),
            )
        }
        MathExpression::Function { name, argument } => {
            let name = math_expression_size(name);
            let argument = math_expression_size(argument);
            (
                1usize.saturating_add(name.0).saturating_add(argument.0),
                name.1.saturating_add(argument.1),
            )
        }
        MathExpression::Accent { accent, base } => {
            let base = math_expression_size(base);
            (
                base.0.saturating_add(1),
                base.1.saturating_add(accent.len()),
            )
        }
        MathExpression::Limit { base, limit, .. } => {
            let base = math_expression_size(base);
            let limit = math_expression_size(limit);
            (
                1usize.saturating_add(base.0).saturating_add(limit.0),
                base.1.saturating_add(limit.1),
            )
        }
        MathExpression::Nary {
            operator,
            lower,
            upper,
            base,
        } => {
            let mut size = math_expression_size(base);
            size.0 = size.0.saturating_add(1);
            size.1 = size.1.saturating_add(operator.len());
            for bound in [lower, upper].into_iter().flatten() {
                let child = math_expression_size(bound);
                size.0 = size.0.saturating_add(child.0);
                size.1 = size.1.saturating_add(child.1);
            }
            size
        }
        MathExpression::Matrix { rows } => {
            let mut size = (1usize, 0usize);
            for row in rows {
                for cell in &row.cells {
                    let child = math_expression_size(cell);
                    size.0 = size.0.saturating_add(child.0);
                    size.1 = size.1.saturating_add(child.1);
                }
            }
            size
        }
        MathExpression::EqArray { rows } => {
            let mut size = (1usize, 0usize);
            for row in rows {
                let child = math_expression_size(row);
                size.0 = size.0.saturating_add(child.0);
                size.1 = size.1.saturating_add(child.1);
            }
            size
        }
        MathExpression::Bar { base, .. } => {
            let base = math_expression_size(base);
            (base.0.saturating_add(1), base.1)
        }
        MathExpression::GroupChar {
            character, base, ..
        } => {
            let base = math_expression_size(base);
            (
                base.0.saturating_add(1),
                base.1.saturating_add(character.len()),
            )
        }
    }
}

/// Bounds an [`Extent`]'s width and height to the positive-coordinate domain
/// (`0..=MAX_EMU`). `property` names the site for the error.
fn check_extent(extent: &Extent, property: &'static str) -> Result<(), ModelError> {
    check_domain(
        (0..=MAX_EMU).contains(&extent.width_emu) && (0..=MAX_EMU).contains(&extent.height_emu),
        property,
    )
}

/// Bounds a format-change revision's opaque metadata: `author` is non-empty and
/// at most 255 bytes; `date`/`revision_id` are non-empty and at most 64 bytes —
/// the same bounds as `w:ins`/`w:del` revision metadata. `property` names the
/// change site (e.g. `"run.propChange"`).
fn check_prop_change_meta<P>(
    change: &PropChange<P>,
    property: &'static str,
) -> Result<(), ModelError> {
    if let Some(author) = &change.author {
        check_domain(!author.is_empty() && author.len() <= 255, property)?;
    }
    for value in [&change.date, &change.revision_id].into_iter().flatten() {
        check_domain(!value.is_empty() && value.len() <= 64, property)?;
    }
    Ok(())
}

/// Bounds an anchored drawing's horizontal and vertical offsets. A `posOffset`
/// is signed (`ST_PositionOffset`), so the magnitude — not a `0..` range — is
/// bounded to the positive-coordinate limit. An alignment carries no magnitude.
fn check_anchor_offset(
    horizontal: &HorizontalPosition,
    vertical: &VerticalPosition,
) -> Result<(), ModelError> {
    if let HorizontalPosition::Offset(offset) = horizontal {
        check_domain(
            offset.unsigned_abs() <= MAX_EMU as u64,
            "anchoredDrawing.offsetH",
        )?;
    }
    if let VerticalPosition::Offset(offset) = vertical {
        check_domain(
            offset.unsigned_abs() <= MAX_EMU as u64,
            "anchoredDrawing.offsetV",
        )?;
    }
    Ok(())
}

/// Bounds the four non-negative `wp:anchor` text-exclusion distances.
fn check_wrap_distances(distances: &WrapDistances) -> Result<(), ModelError> {
    for distance in [
        distances.top_emu,
        distances.bottom_emu,
        distances.start_emu,
        distances.end_emu,
    ] {
        check_domain(
            (0..=MAX_EMU).contains(&distance),
            "drawingAnchor.wrapDistances",
        )?;
    }
    Ok(())
}

/// Bounds the percentage values carried by `a:normAutofit`. Insets need no
/// separate check: their `i32` representation is exactly the
/// `ST_Coordinate32` domain.
fn check_text_box_body_properties(
    properties: &TextBoxBodyProperties,
    property: &'static str,
) -> Result<(), ModelError> {
    if let TextBoxAutoFit::Normal {
        font_scale,
        line_spacing_reduction,
    } = properties.auto_fit
    {
        check_domain((1_000..=100_000).contains(&font_scale), property)?;
        check_domain(line_spacing_reduction <= 100_000, property)?;
    }
    Ok(())
}

/// Bounds every present edge of a border set. `property` is the stable domain
/// name for the level (`"table.borders"` / `"table.cell.borders"`).
fn check_borders(borders: &TableBorders, property: &'static str) -> Result<(), ModelError> {
    for edge in [
        &borders.top,
        &borders.start,
        &borders.bottom,
        &borders.end,
        &borders.inside_h,
        &borders.inside_v,
    ]
    .into_iter()
    .flatten()
    {
        check_domain(!edge.style.is_empty() && edge.style.len() <= 32, property)?;
        if let Some(size) = edge.size_eighth_points {
            check_domain(size <= 1024, property)?;
        }
        if let Some(space) = edge.space_points {
            check_domain(space <= 31, property)?;
        }
    }
    Ok(())
}

/// Bounds a single border edge (style token length, size, and space) — the same
/// domain the table-border edges use, reused for page borders.
fn check_border_edge(edge: &BorderEdge, property: &'static str) -> Result<(), ModelError> {
    check_domain(!edge.style.is_empty() && edge.style.len() <= 32, property)?;
    if let Some(size) = edge.size_eighth_points {
        check_domain(size <= 1024, property)?;
    }
    if let Some(space) = edge.space_points {
        check_domain(space <= 31, property)?;
    }
    Ok(())
}

/// Bounds every present edge of a section's page borders.
fn check_page_borders(borders: &PageBorders) -> Result<(), ModelError> {
    for edge in [&borders.top, &borders.bottom, &borders.start, &borders.end]
        .into_iter()
        .flatten()
    {
        check_border_edge(edge, "section.page_borders")?;
    }
    Ok(())
}

/// Bounds every present cell margin (`0..=31_680` twips).
fn check_margins(margins: &CellMargins, property: &'static str) -> Result<(), ModelError> {
    for value in [
        margins.top_twips,
        margins.start_twips,
        margins.bottom_twips,
        margins.end_twips,
    ]
    .into_iter()
    .flatten()
    {
        check_domain((0..=31_680).contains(&value), property)?;
    }
    Ok(())
}
