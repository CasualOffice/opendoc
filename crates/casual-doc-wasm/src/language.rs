// SPDX-License-Identifier: Apache-2.0

//! The language text is written in: setting it on a selection and reading it
//! back per run (`docs/153` `review.proofing-language-selection` and
//! `text.language-of-selection`, `docs/168` batch 27, `docs/109` RM-42).
//!
//! The model has carried `w:lang` since the importer started reading it, and
//! `languageAt` exposes the language a RANGE agrees on. Two things were
//! missing, and they are this module:
//!
//! - **Writing it.** Set Language is one more field of the character patch
//!   (`CharacterFormatDelta::language`), so it rides the existing `FormatText`
//!   operation — one undo step, tracked in Suggesting mode through the same
//!   `suggestCharacterFormat` lowering, and transformed by OT with every other
//!   character property. No new operation exists for it, so none needs a
//!   registry twin. ONLYOFFICE does the same: `asc_docs_api.put_TextPrLang` is
//!   an `AddToParagraph(ParaTextPr({Lang}))`, a text-property change like bold
//!   (`sdkjs/word/api.js:3569-3579`).
//! - **Reading it per run.** A paragraph that mixes languages answers `""` to
//!   `languageAt` by design, and the spell checker then fell back to the
//!   document default for every word in it — so the French phrase a user had
//!   just tagged French was still checked as English. `languageSpans` lists
//!   where each language applies, and the checker proofs each span with its
//!   own word list.

use super::*;

/// Whether `tag` is a well-formed BCP 47 language tag as far as this editor
/// needs: one or more ASCII alphanumeric subtags of 1–8 characters joined by
/// `-`, inside the model's 85-byte bound for `w:lang` (`Document::validate`).
/// Not a registry lookup: Word writes tags it does not itself know, and a
/// refusal here must only stop text that cannot be a tag at all.
pub(crate) fn is_language_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 85
        && tag.split('-').all(|part| {
            !part.is_empty() && part.len() <= 8 && part.bytes().all(|b| b.is_ascii_alphanumeric())
        })
}

/// One stretch of a paragraph in one language: byte offsets in the editing
/// text, and the effective Latin `w:lang` (`""` where none is declared).
#[derive(Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct LanguageSpan {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) language: String,
}

/// The paragraph's runs grouped into maximal same-language spans, through the
/// style cascade (a style's or the defaults' `w:lang` counts as the run's).
/// Adjacent runs in one language are one span even when a non-text inline sits
/// between them, so a spell checker sees one stretch per language change.
///
/// Complexity: O(inlines in the paragraph) plus one cascade resolution per
/// run; never O(document). Called once per paragraph in the proofing window,
/// and only for a paragraph whose range answer was `""`.
pub(crate) fn language_spans_inner(
    document: &Document,
    node: &str,
) -> Result<Vec<LanguageSpan>, String> {
    let id = NodeId::from_str(node).map_err(|_| format!("`{node}` is not a node id"))?;
    let paragraph =
        find_paragraph_any(document, id).ok_or_else(|| format!("`{node}` is not a paragraph"))?;
    let cascade = StyleCascade::new(document.definitions());
    let style = cascade.paragraph_style(&paragraph.properties);
    let mut spans: Vec<LanguageSpan> = Vec::new();
    for (start, end, direct) in casual_doc_edit::run_property_spans(paragraph) {
        let resolved = cascade.resolve_run(style, direct);
        let language = resolved
            .language
            .as_ref()
            .and_then(|language| language.value.clone())
            .unwrap_or_default();
        let last = spans.len().checked_sub(1);
        if let Some(last) = last.filter(|&i| spans[i].language == language) {
            spans[last].end = end;
        } else {
            spans.push(LanguageSpan {
                start,
                end,
                language,
            });
        }
    }
    Ok(spans)
}

#[wasm_bindgen]
impl WasmDocument {
    /// The paragraph's same-language spans as JSON
    /// `[{ "start", "end", "language" }]`, byte offsets in editing text; `[]`
    /// for an unknown node or an empty paragraph. See `language_spans_inner`
    /// for the rule and its cost.
    #[wasm_bindgen(js_name = languageSpans)]
    #[must_use]
    pub fn language_spans(&self, node: &str) -> String {
        language_spans_inner(&self.document, node)
            .ok()
            .and_then(|spans| serde_json::to_string(&spans).ok())
            .unwrap_or_else(|| "[]".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sets `tag` on `[start, end)` of `node` through the same lowering the
    /// chrome's Set Language uses (`setCharacterFormat` with `{"language"}`),
    /// without touching a `JsValue` path.
    fn set_language(
        d: &mut WasmDocument,
        node: NodeId,
        start: u32,
        end: u32,
        tag: Option<&str>,
    ) -> Result<(), String> {
        let json = serde_json::json!({ "language": tag }).to_string();
        let character: casual_doc_edit::CharacterFormatDelta =
            serde_json::from_str(&json).map_err(|e| e.to_string())?;
        let ops = casual_doc_command::format::format_ranges(
            &[(node, start, end)],
            &FormatDelta {
                character: Some(Box::new(character)),
                ..Default::default()
            },
        );
        d.apply_action(ops).map(|_| ())
    }

    fn first_paragraph(d: &WasmDocument) -> NodeId {
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        nodes[0].0
    }

    #[test]
    fn a_language_set_on_part_of_a_paragraph_is_its_own_span_and_survives_save() {
        let mut d = open_document(b"colour and couleur").unwrap();
        let node = first_paragraph(&d);
        let id = node.to_string();
        assert_eq!(
            language_spans_inner(&d.document, &id).unwrap(),
            vec![LanguageSpan {
                start: 0,
                end: 18,
                language: String::new()
            }]
        );
        set_language(&mut d, node, 11, 18, Some("fr-FR")).unwrap();
        let spans = language_spans_inner(&d.document, &id).unwrap();
        assert_eq!(
            spans,
            vec![
                LanguageSpan {
                    start: 0,
                    end: 11,
                    language: String::new()
                },
                LanguageSpan {
                    start: 11,
                    end: 18,
                    language: "fr-FR".into()
                },
            ]
        );
        // The range read the status bar uses agrees with the spans.
        assert_eq!(shared_language_in_range(&d.document, node, 11, 18), "fr-FR");
        assert_eq!(shared_language_in_range(&d.document, node, 0, 18), "");

        // `w:lang` reaches the file and comes back.
        let bytes = write_document(&d.document, d.resources.as_map()).unwrap();
        let reopened = open_document(&bytes).unwrap();
        let reopened_node = first_paragraph(&reopened).to_string();
        assert_eq!(
            language_spans_inner(&reopened.document, &reopened_node)
                .unwrap()
                .last()
                .map(|span| span.language.as_str()),
            Some("fr-FR")
        );

        // One undo step removes it again.
        d.undo_inner().unwrap();
        assert_eq!(language_spans_inner(&d.document, &id).unwrap().len(), 1);
    }

    #[test]
    fn clearing_a_language_merges_the_spans_back() {
        let mut d = open_document(b"alpha beta").unwrap();
        let node = first_paragraph(&d);
        set_language(&mut d, node, 0, 10, Some("de-DE")).unwrap();
        set_language(&mut d, node, 6, 10, None).unwrap();
        let spans = language_spans_inner(&d.document, &node.to_string()).unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].language, "de-DE");
        assert_eq!(spans[1].language, "");
        set_language(&mut d, node, 0, 6, None).unwrap();
        assert_eq!(
            language_spans_inner(&d.document, &node.to_string())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn an_unknown_node_is_an_error_not_a_panic() {
        let d = open_document(b"alpha").unwrap();
        assert!(language_spans_inner(&d.document, "not-a-node").is_err());
        assert!(language_spans_inner(&d.document, "9:9999").is_err());
    }

    #[test]
    fn language_tags_are_checked_for_shape_only() {
        for tag in ["en", "en-US", "zh-Hant-TW", "sr-Latn-RS", "x-klingon"] {
            assert!(is_language_tag(tag), "{tag}");
        }
        let long = "a".repeat(86);
        for tag in [
            "",
            "-",
            "en-",
            "en US",
            "en_US",
            "toolongsubtag",
            long.as_str(),
        ] {
            assert!(!is_language_tag(tag), "{tag:?}");
        }
    }
}
