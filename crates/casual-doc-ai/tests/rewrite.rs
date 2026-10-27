// SPDX-License-Identifier: Apache-2.0

//! `docs/181` §4.3, `rewrite_block`: an agent rewrites a paragraph and the
//! engine changes only the words that differ, so a suggestion is a small,
//! reviewable revision rather than a delete-all/insert-all, and formatting the
//! Markdown cannot express is kept.

use casual_doc_ai::plan::{self, EditPlan};
use casual_doc_ai::read::{self, ReadFormat};
use casual_doc_ai::{Clock, Profile, Session, SessionConfig};
use serde_json::{Value, json};

const TEXT: &str = "The notice period is thirty (30) days, and either party may terminate it.\n";

fn open(profile: Profile) -> Session {
    let config = SessionConfig {
        author: "Claude (AI)".to_owned(),
        profile,
        clock: Clock::Fixed("2026-10-28T00:00:00Z".to_owned()),
        ..SessionConfig::default()
    };
    Session::open(TEXT.as_bytes(), "contract.txt", config).expect("a text file opens")
}

fn first(session: &Session) -> Value {
    read::read_blocks(session, None, None, Some(20_000), ReadFormat::Json).expect("reads")["blocks"]
        [0]
    .clone()
}

fn apply(session: &mut Session, mode: &str, edits: Value) -> Result<Value, casual_doc_ai::AiError> {
    let plan: EditPlan = serde_json::from_value(json!({
        "base_revision": session.revision(),
        "mode": mode,
        "edits": edits,
    }))
    .expect("a well-formed plan");
    plan::apply(session, &plan)
}

#[test]
fn a_one_word_rewrite_is_one_small_revision_not_a_replaced_paragraph() {
    let mut session = open(Profile::Suggest);
    let node = first(&session)["id"].as_str().expect("id").to_owned();
    let rewritten = TEXT.trim_end().replace("thirty", "forty");
    apply(
        &mut session,
        "suggest",
        json!([{ "op": "rewrite_block", "target": { "node": node },
                 "markdown": format!("{{#{node}}} {rewritten}") }]),
    )
    .expect("the rewrite applies");
    let changes = read::list_tracked_changes(&session, None, Some(50)).expect("changes");
    let changes = changes["trackedChanges"].as_array().expect("a list");
    let texts: Vec<(&str, &str)> = changes
        .iter()
        .map(|change| {
            (
                change["kind"].as_str().unwrap_or_default(),
                change["text"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    // One replaced word: its deletion and its insertion, and nothing else of
    // the sentence is touched.
    assert_eq!(texts.len(), 2, "one word, one replacement: {texts:?}");
    assert!(
        texts.iter().any(|(_, text)| *text == "thirty"),
        "the old word is the deletion: {texts:?}"
    );
    assert!(
        texts.iter().any(|(_, text)| *text == "forty"),
        "the new word is the insertion: {texts:?}"
    );
    assert!(
        texts.iter().all(|(_, text)| text.len() <= "thirty".len()),
        "no revision carries the paragraph: {texts:?}"
    );
}

#[test]
fn a_direct_rewrite_keeps_formatting_the_markdown_cannot_express() {
    let mut session = open(Profile::Edit);
    let node = first(&session)["id"].as_str().expect("id").to_owned();
    apply(
        &mut session,
        "direct",
        json!([{ "op": "set_text_format",
                 "target": { "node": node, "exact": "either party" },
                 "format": { "bold": true } }]),
    )
    .expect("bold applies");
    let rewritten = TEXT.trim_end().replace("thirty (30)", "sixty (60)");
    apply(
        &mut session,
        "direct",
        json!([{ "op": "rewrite_block", "target": { "node": node }, "markdown": rewritten }]),
    )
    .expect("the rewrite applies");
    let block = first(&session);
    assert_eq!(block["text"], rewritten);
    let bold: Vec<&str> = block["runs"]
        .as_array()
        .expect("runs")
        .iter()
        .filter(|run| {
            run["marks"]
                .as_array()
                .is_some_and(|marks| marks.contains(&json!("bold")))
        })
        .filter_map(|run| run["text"].as_str())
        .collect();
    assert_eq!(
        bold,
        vec!["either party"],
        "bold the Markdown cannot say survives a rewrite of other words"
    );
}

#[test]
fn a_rewrite_names_its_own_block_and_one_paragraph() {
    let mut session = open(Profile::Edit);
    let node = first(&session)["id"].as_str().expect("id").to_owned();
    for markdown in ["{#9:9} Another block's text.", "Two\nparagraphs."] {
        let refused = apply(
            &mut session,
            "direct",
            json!([{ "op": "rewrite_block", "target": { "node": node }, "markdown": markdown }]),
        )
        .expect_err("refused");
        assert_eq!(
            refused.details["firstCode"], "invalid-argument",
            "{markdown}"
        );
    }
    let same = apply(
        &mut session,
        "direct",
        json!([{ "op": "rewrite_block", "target": { "node": node }, "markdown": TEXT.trim_end() }]),
    )
    .expect_err("an unchanged rewrite changes nothing");
    assert_eq!(same.details["firstCode"], "no-change");
}

/// The word diff `rewrite_block` lowers through is casual-doc-diff's anchored
/// aligner. A rewrite of one word in a paragraph twice as long must cost about
/// twice as much, not four times (SKILL §8: an n-versus-2n guard, not a clock).
#[test]
fn the_rewrite_diff_is_linear_in_the_paragraph_for_a_small_change() {
    let paragraph = |words: usize| {
        (0..words)
            .map(|index| format!("word{index}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let cost = |words: usize| {
        let before = paragraph(words);
        let after = before.replacen("word7 ", "changed ", 1);
        casual_doc_diff::inline::diff_text(&before, &after).comparisons
    };
    let (n, two_n) = (cost(2_000), cost(4_000));
    assert!(
        two_n <= n * 3,
        "doubling the paragraph must not quadruple the diff: {n} -> {two_n}"
    );
}
