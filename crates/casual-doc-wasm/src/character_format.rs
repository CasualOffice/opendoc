//! Bounded additive advanced character API, using existing history/policy paths.
use super::*;
use casual_doc_edit::CharacterFormatDelta;

fn parse_character(json: &str) -> Result<CharacterFormatDelta, String> {
    if json.len() > 8192 {
        return Err("character patch is too large".into());
    }
    let delta: CharacterFormatDelta = serde_json::from_str(json).map_err(|e| e.to_string())?;
    // Validate before the command changes any document or allocates revision ids.
    if delta
        .character_spacing_twips
        .flatten()
        .is_some_and(|v| !(-31680..=31680).contains(&v))
        || delta
            .position_half_points
            .flatten()
            .is_some_and(|v| !(-31680..=31680).contains(&v))
        || delta
            .character_scale_percent
            .flatten()
            .is_some_and(|v| !(1..=600).contains(&v))
        || delta
            .kerning_half_points
            .flatten()
            .is_some_and(|v| v > 65534)
    {
        return Err("character value outside the supported range".into());
    }
    if let Some(Some(border)) = &delta.border
        && (border.style.is_empty()
            || border.style.len() > 32
            || border.size_eighth_points.is_some_and(|v| v > 1024)
            || border.space_points.is_some_and(|v| v > 31))
    {
        return Err("character border outside the supported range".into());
    }
    Ok(delta)
}

fn character_values(props: &RunProperties) -> serde_json::Map<String, serde_json::Value> {
    serde_json::json!({
        "allCaps":props.all_caps,"smallCaps":props.small_caps,"doubleStrike":props.double_strike,
        "outline":props.outline,"shadow":props.shadow,"emboss":props.emboss,"imprint":props.imprint,
        "emphasis":props.emphasis,"border":props.border,"characterSpacingTwips":props.character_spacing_twips,
        "characterScalePercent":props.character_scale_percent,"positionHalfPoints":props.position_half_points,
        "kerningHalfPoints":props.kerning_half_points,"ligatures":props.ligatures,
    }).as_object().expect("character projection is an object").clone()
}

pub(super) fn append_review_changes(
    changes: &mut Vec<serde_json::Value>,
    current: &RunProperties,
    prior: &RunProperties,
) {
    let before = character_values(prior);
    for (property, after) in character_values(current) {
        if before[&property] != after {
            changes.push(
                serde_json::json!({"property":property,"before":before[&property],"after":after}),
            );
        }
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// Apply an advanced character patch across paragraphs as one undo action.
    #[wasm_bindgen(js_name = setCharacterFormat)]
    pub fn set_character_format(
        &mut self,
        sn: &str,
        so: u32,
        en: &str,
        eo: u32,
        json: &str,
    ) -> Result<EditResult, JsValue> {
        let character = parse_character(json).map_err(to_js)?;
        self.apply_run_format(
            sn,
            so,
            en,
            eo,
            FormatDelta {
                character: Some(Box::new(character)),
                ..Default::default()
            },
        )
    }

    /// Effective uniform properties, with mixed values listed separately.
    #[wasm_bindgen(js_name = selectionCharacterFormat)]
    pub fn selection_character_format(
        &self,
        sn: &str,
        so: u32,
        en: &str,
        eo: u32,
    ) -> Result<String, JsValue> {
        let (start, end) = self.order_endpoints(sn, so, en, eo).map_err(to_js)?;
        let mut values: Option<serde_json::Map<String, serde_json::Value>> = None;
        let mut mixed = std::collections::BTreeSet::new();
        let mut visit = |props: &RunProperties| {
            let next = character_values(props);
            if let Some(first) = &values {
                for (key, value) in &next {
                    if first.get(key) != Some(value) {
                        mixed.insert(key.clone());
                    }
                }
            } else {
                values = Some(next);
            }
        };
        if start == end {
            if let Some(props) =
                effective_caret_run_properties(&self.document, start.node, start.offset)
            {
                visit(&props);
            }
        } else {
            for (node, a, b) in self.selection_subranges(start, end) {
                // Aggregate each paragraph and release its resolved properties;
                // never serialize revision snapshots or accumulate JSON per run.
                for props in effective_run_properties_in_range(&self.document, node, a, b) {
                    visit(&props);
                }
            }
        }
        let mut values = values.unwrap_or_else(|| character_values(&RunProperties::default()));
        for key in &mixed {
            values.remove(key);
        }
        Ok(serde_json::json!({ "values": values, "mixed": mixed }).to_string())
    }

    /// Advanced character formatting tracked across paragraphs in one revision group.
    #[wasm_bindgen(js_name = suggestCharacterFormat)]
    #[allow(clippy::too_many_arguments)]
    pub fn suggest_character_format(
        &mut self,
        sn: &str,
        so: u32,
        en: &str,
        eo: u32,
        json: &str,
        date: Option<String>,
    ) -> Result<EditResult, JsValue> {
        let character = parse_character(json).map_err(to_js)?;
        let delta = FormatDelta {
            character: Some(Box::new(character)),
            ..Default::default()
        };
        let author = self.resolve_author(None);
        validate_authored_revision_author(author.as_deref()).map_err(to_js)?;
        let (start, end) = self.order_endpoints(sn, so, en, eo).map_err(to_js)?;
        let ranges = self.selection_subranges(start, end);
        if ranges.is_empty() {
            return Err(to_js(
                "tracked character formatting requires a range".into(),
            ));
        }
        let group = RevisionGroup {
            id: self
                .edit_ids
                .next_id()
                .map_err(|_| to_js("id space exhausted".into()))?,
            kind: RevisionGroupKind::Formatting,
        };
        let notes = NoteAnchorLengths::of(&self.document);
        let mut ops = Vec::new();
        for (node, a, b) in ranges {
            let mut body = review_paragraph_body(&self.document, node).map_err(to_js)?;
            if !apply_review_format_change(
                &notes,
                &mut body,
                node,
                a,
                b,
                delta.clone(),
                author.clone(),
                date.clone(),
                group,
                &mut self.revision_ids,
                &mut self.edit_ids,
            )
            .map_err(to_js)?
            {
                return Err(to_js("tracked formatting requires paragraph text".into()));
            }
            ops.push(update_review_operation(&self.document, &body, None).map_err(to_js)?);
        }
        self.apply_action_caret_as(ops, start, HistoryKind::Review)
            .map_err(to_js)
    }

    /// Armed typing with additive character formatting and typed underline.
    #[wasm_bindgen(js_name = typeCharacterText)]
    #[allow(clippy::too_many_arguments)]
    pub fn type_character_text(
        &mut self,
        node: &str,
        offset: u32,
        text: String,
        bold: Option<bool>,
        italic: Option<bool>,
        underline: Option<bool>,
        strike: Option<bool>,
        size: Option<u32>,
        color: Option<String>,
        highlight: Option<String>,
        vert: Option<String>,
        font: Option<String>,
        session: u32,
        underline_style: Option<String>,
        underline_color: Option<String>,
        json: &str,
        suggesting: bool,
        date: Option<String>,
    ) -> Result<EditResult, JsValue> {
        let mut delta = review_format_delta(
            bold, italic, underline, strike, size, color, highlight, vert, font,
        );
        delta.character = Some(Box::new(parse_character(json).map_err(to_js)?));
        delta.underline_style = underline_style
            .as_deref()
            .map(parse_underline_style)
            .transpose()
            .map_err(to_js)?;
        delta.underline_color = match underline_color.as_deref() {
            None => None,
            Some("") | Some("auto") => Some(None),
            Some(value) => Some(Some(
                parse_hex_color(value).ok_or_else(|| to_js("invalid underline color".into()))?,
            )),
        };
        if suggesting {
            self.suggest_delta_insert(node, offset, &text, delta, None, date, Some(session), true)
        } else {
            self.styled_delta_action(node_id(node)?, offset, text, delta, Some(session))
                .map_err(to_js)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advanced_review_changes_name_the_properties_and_preserve_units() {
        let prior = RunProperties::default();
        let mut current = prior.clone();
        parse_character(
            r#"{"outline":true,"characterSpacingTwips":25,"ligatures":"standardDiscretional"}"#,
        )
        .unwrap()
        .apply_to(&mut current);
        let delta = formatting_delta_json(&current, &prior);
        let changes = delta.as_array().unwrap();
        assert_eq!(changes.len(), 3);
        for (property, value) in [
            ("outline", serde_json::json!(true)),
            ("characterSpacingTwips", serde_json::json!(25)),
            ("ligatures", serde_json::json!("standardDiscretional")),
        ] {
            assert_eq!(
                changes
                    .iter()
                    .find(|change| change["property"] == property)
                    .unwrap()["after"],
                value
            );
        }
        assert_eq!(
            formatting_delta_json(&current, &current),
            serde_json::json!([])
        );
    }

    #[test]
    fn bounded_character_parser_refuses_bad_units_tokens_and_fields() {
        for json in [
            r#"{"characterScalePercent":0}"#,
            r#"{"characterSpacingTwips":31681}"#,
            r#"{"positionHalfPoints":-31681}"#,
            r#"{"kerningHalfPoints":65535}"#,
            r#"{"ligatures":"discretionary"}"#,
            r#"{"unknown":true}"#,
            r#"{"border":{"style":"single","spacePoints":32}}"#,
        ] {
            assert!(parse_character(json).is_err(), "{json}");
        }
        assert!(parse_character(&" ".repeat(8193)).is_err());
        assert!(parse_character(r#"{"characterScalePercent":600,"characterSpacingTwips":-31680,"positionHalfPoints":31680,"kerningHalfPoints":65534}"#).is_ok());
    }
    fn text_document() -> (WasmDocument, Vec<(NodeId, String)>) {
        let d = open_document(b"alpha\nbeta").unwrap();
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        (d, nodes)
    }
    fn values(d: &WasmDocument, n: NodeId, start: u32, end: u32) -> serde_json::Value {
        serde_json::from_str(
            &d.selection_character_format(&n.to_string(), start, &n.to_string(), end)
                .unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn inherited_ligatures_survive_save_and_direct_none_or_clear() {
        let (mut d, nodes) = text_document();
        let n = nodes[0].0;
        d.document.definitions_mut().document_defaults =
            Some(casual_doc_model::v1::DocumentDefaults {
                paragraph: None,
                run: Some(RunProperties {
                    ligatures: Some(casual_doc_model::v1::Ligatures::All),
                    ..Default::default()
                }),
            });
        assert_eq!(values(&d, n, 0, 5)["values"]["ligatures"], "all");
        d.set_character_format(
            &n.to_string(),
            0,
            &n.to_string(),
            5,
            r#"{"ligatures":"none"}"#,
        )
        .unwrap();
        assert_eq!(values(&d, n, 0, 5)["values"]["ligatures"], "none");
        d.set_character_format(
            &n.to_string(),
            0,
            &n.to_string(),
            5,
            r#"{"ligatures":null}"#,
        )
        .unwrap();
        assert_eq!(values(&d, n, 0, 5)["values"]["ligatures"], "all");
        let reopened = open_document(&d.export_docx().unwrap()).unwrap();
        let mut nodes = Vec::new();
        collect_block_text(reopened.document.body(), &mut nodes);
        assert_eq!(
            values(&reopened, nodes[0].0, 0, 5)["values"]["ligatures"],
            "all"
        );
    }
    #[test]
    fn character_transactions_cannot_bypass_viewer_or_document_protection() {
        let (mut d, nodes) = text_document();
        let n = nodes[0].0;
        let op = Operation::FormatText {
            range: EditRange {
                start: Pos::new(n, 0),
                end: Pos::new(n, 5),
            },
            delta: FormatDelta {
                character: Some(Box::new(parse_character(r#"{"outline":true}"#).unwrap())),
                ..Default::default()
            },
        };
        d.capabilities = Capabilities::viewer();
        let before = d.document.clone();
        assert!(d.apply(op.clone()).is_err());
        assert_eq!(before, d.document);
        d.capabilities = Capabilities::owner();
        d.set_document_protection_inner(Some("readOnly"), true, true)
            .unwrap();
        let before = d.document.clone();
        assert!(d.apply(op).is_err());
        assert_eq!(before, d.document);
    }

    #[test]
    fn character_range_preserves_text_undo_and_mixed_state() {
        let (mut d, nodes) = text_document();
        let first = nodes[0].0;
        let second = nodes[1].0;
        d.set_character_format(&first.to_string(),1,&second.to_string(),2,r#"{"allCaps":true,"smallCaps":true,"doubleStrike":true,"emphasis":"dot","characterSpacingTwips":20,"characterScalePercent":125,"positionHalfPoints":3,"kerningHalfPoints":24,"ligatures":"all","border":{"style":"double","sizeEighthPoints":8}}"#).unwrap();
        assert_eq!(
            d.copy_text(&first.to_string(), 0, &second.to_string(), 4),
            "alpha\nbeta"
        );
        let state = values(&d, first, 1, 5);
        assert_eq!(state["values"]["allCaps"], true);
        assert_eq!(state["values"]["ligatures"], "all");
        assert_eq!(state["values"]["border"]["style"], "double");
        assert!(
            values(&d, first, 0, 5)["mixed"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("allCaps"))
        );
        d.undo().unwrap();
        assert_eq!(
            values(&d, first, 1, 5)["values"]["allCaps"],
            serde_json::Value::Null
        );
        d.redo().unwrap();
        assert_eq!(
            values(&d, second, 0, 2)["values"]["characterScalePercent"],
            125
        );
        d.set_character_format(
            &first.to_string(),
            1,
            &first.to_string(),
            5,
            r#"{"characterScalePercent":null,"shadow":false}"#,
        )
        .unwrap();
        let state = values(&d, first, 1, 5);
        assert_eq!(
            state["values"]["characterScalePercent"],
            serde_json::Value::Null
        );
        assert_eq!(state["values"]["allCaps"], true);
    }
    #[test]
    fn every_ligature_mode_survives_docx_save_and_reopen() {
        for token in crate::character_format::tests::tokens() {
            let (mut d, nodes) = text_document();
            let n = nodes[0].0;
            d.set_character_format(
                &n.to_string(),
                0,
                &n.to_string(),
                5,
                &serde_json::json!({"ligatures":token}).to_string(),
            )
            .unwrap();
            let bytes = d.export_docx().unwrap();
            let reopened = open_document(&bytes).unwrap();
            let mut nodes = Vec::new();
            collect_block_text(reopened.document.body(), &mut nodes);
            assert_eq!(
                values(&reopened, nodes[0].0, 0, 5)["values"]["ligatures"],
                token,
                "{token}"
            );
        }
    }
    fn tokens() -> [&'static str; 16] {
        [
            "none",
            "standard",
            "contextual",
            "standardContextual",
            "historical",
            "standardHistorical",
            "contextualHistorical",
            "standardContextualHistorical",
            "discretional",
            "standardDiscretional",
            "contextualDiscretional",
            "standardContextualDiscretional",
            "historicalDiscretional",
            "standardHistoricalDiscretional",
            "contextualHistoricalDiscretional",
            "all",
        ]
    }
    #[test]
    fn armed_character_typing_is_one_undo_and_suggestions_reject_atomically() {
        let (mut d, nodes) = text_document();
        let n = nodes[0].0.to_string();
        d.type_character_text(
            &n,
            0,
            "X".into(),
            Some(true),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            1,
            None,
            None,
            r#"{"allCaps":true,"shadow":true}"#,
            false,
            None,
        )
        .unwrap();
        assert_eq!(values(&d, nodes[0].0, 0, 1)["values"]["shadow"], true);
        d.undo().unwrap();
        assert_eq!(d.copy_text(&n, 0, &n, 5), "alpha");
        d.set_active_author("Tester", None, None).unwrap();
        d.suggest_character_format(
            &n,
            0,
            &nodes[1].0.to_string(),
            4,
            r#"{"outline":true,"ligatures":"none"}"#,
            None,
        )
        .unwrap();
        assert_eq!(values(&d, nodes[0].0, 0, 5)["values"]["outline"], true);
        let summary: serde_json::Value = serde_json::from_str(&d.review_summary()).unwrap();
        let revisions = summary["revisions"].as_array().unwrap();
        assert_eq!(revisions.len(), 2);
        assert_eq!(
            revisions[0]["groupId"], revisions[1]["groupId"],
            "one cross-paragraph revision group"
        );
        d.decide_revision_group(revisions[0]["groupId"].as_str().unwrap(), false)
            .unwrap();
        assert_eq!(
            values(&d, nodes[0].0, 0, 5)["values"]["outline"],
            serde_json::Value::Null
        );
        d.undo().unwrap();
        // Undo restores both paragraphs without duplicating or deleting text.
        d.undo().unwrap();
        assert_eq!(
            values(&d, nodes[1].0, 0, 4)["values"]["outline"],
            serde_json::Value::Null
        );
    }
}
