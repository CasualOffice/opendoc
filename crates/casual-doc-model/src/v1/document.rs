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
}

impl Document {
    /// Builds and validates a v1 document from constructed parts.
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
        };
        document.validate()?;
        Ok(document)
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

    /// Returns the definition tables.
    #[must_use]
    pub const fn definitions(&self) -> &Definitions {
        &self.definitions
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
        self.validate_body()?;
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

    fn validate_sections(&self) -> Result<(), ModelError> {
        for section in &self.definitions.sections {
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
        for block in &self.body {
            let BlockNode::Paragraph(paragraph) = block;
            insert_id(&mut ids, paragraph.id)?;
            for inline in &paragraph.inlines {
                record_inline_ids(inline, &mut ids)?;
            }
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
            }
        }
        // Level domain: level start values.
        for (_, abstract_num) in self.definitions.abstract_numbering.iter() {
            for level in &abstract_num.levels {
                check_domain(level.start <= 32_767, "numbering.level.start")?;
                if let Some(style) = level.style_ref {
                    if !self.style_exists(style) {
                        return Err(ModelError::DanglingStyleRef(style.node_id()));
                    }
                }
            }
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
        if let Some(style) = properties.style_ref {
            if !self.style_exists(style) {
                return Err(ModelError::DanglingStyleRef(style.node_id()));
            }
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
        Ok(())
    }

    fn check_run_property_refs(&self, properties: &RunProperties) -> Result<(), ModelError> {
        if let Some(style) = properties.style_ref {
            if !self.style_exists(style) {
                return Err(ModelError::DanglingStyleRef(style.node_id()));
            }
        }
        if let Some(size) = properties.size_half_points {
            check_domain((1..=65_534).contains(&size), "run.size_half_points")?;
        }
        if let Some(FontRef::Named(font)) = &properties.font_ref {
            check_domain(
                !font.name.is_empty() && font.name.len() <= 255,
                "run.font_ref.name",
            )?;
        }
        Ok(())
    }

    fn validate_body(&self) -> Result<(), ModelError> {
        for block in &self.body {
            let BlockNode::Paragraph(paragraph) = block;
            self.check_paragraph_property_refs(&paragraph.properties)?;
            self.validate_inlines(&paragraph.inlines, paragraph.id, false)?;
        }
        Ok(())
    }

    /// Validates one inline sequence. A drawing or hyperlink is a hard merge
    /// boundary (it resets adjacent-run tracking, like a tab or break).
    /// `in_hyperlink` is set while validating a hyperlink's own children, so a
    /// nested hyperlink is rejected.
    fn validate_inlines(
        &self,
        inlines: &[InlineNode],
        owner: NodeId,
        in_hyperlink: bool,
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
                InlineNode::Hyperlink(link) => {
                    if in_hyperlink {
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
                    self.validate_inlines(&link.inlines, link.id, true)?;
                    previous_run_properties = None;
                }
                InlineNode::Tab(_) | InlineNode::Break(_) => {
                    previous_run_properties = None;
                }
            }
        }
        Ok(())
    }

    fn validate_snapshot_limits(&self, limits: SnapshotLimits) -> Result<(), SnapshotError> {
        enforce_limit("body_blocks", self.body.len(), limits.max_blocks)?;
        let mut scalar_values = 0_usize;
        for block in &self.body {
            let BlockNode::Paragraph(paragraph) = block;
            for inline in &paragraph.inlines {
                accumulate_inline_limits(inline, limits, &mut scalar_values)?;
            }
        }
        enforce_limit(
            "unicode_scalar_values",
            scalar_values,
            limits.max_unicode_scalar_values,
        )
    }
}

/// Accounts one inline node against the text limits, recursing into hyperlink
/// children so nested run text cannot smuggle past the bounds.
fn accumulate_inline_limits(
    inline: &InlineNode,
    limits: SnapshotLimits,
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
                accumulate_inline_limits(child, limits, scalar_values)?;
            }
        }
        InlineNode::Tab(_) | InlineNode::Break(_) | InlineNode::Drawing(_) => {}
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

/// Records an inline's id and, for a hyperlink, its children's ids recursively.
fn record_inline_ids(inline: &InlineNode, ids: &mut BTreeSet<NodeId>) -> Result<(), ModelError> {
    insert_id(ids, inline.id())?;
    if let InlineNode::Hyperlink(link) = inline {
        for child in &link.inlines {
            record_inline_ids(child, ids)?;
        }
    }
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

fn check_domain(condition: bool, property: &'static str) -> Result<(), ModelError> {
    if condition {
        Ok(())
    } else {
        Err(ModelError::PropertyValueOutOfDomain { property })
    }
}
