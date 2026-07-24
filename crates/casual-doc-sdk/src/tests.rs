use std::collections::BTreeSet;

use super::*;

fn initial_paragraph(snapshot: &DocumentSnapshot) -> NodeId {
    match &snapshot.body[0] {
        BlockSnapshot::Paragraph(paragraph) => paragraph.id.clone(),
    }
}

fn paragraph(snapshot: &DocumentSnapshot, index: usize) -> &ParagraphSnapshot {
    match &snapshot.body[index] {
        BlockSnapshot::Paragraph(paragraph) => paragraph,
    }
}

fn paragraph_text(snapshot: &DocumentSnapshot, index: usize) -> String {
    paragraph(snapshot, index)
        .inlines
        .iter()
        .map(|inline| match inline {
            InlineSnapshot::Text { text, .. } => text.as_str(),
        })
        .collect()
}

fn position(node: NodeId, grapheme_offset: u32, affinity: Affinity) -> Position {
    Position {
        node,
        grapheme_offset,
        affinity,
    }
}

#[test]
fn blank_insert_snapshot_is_end_to_end() {
    let engine = Engine::new(EngineConfig { id_namespace: 9 }).unwrap();
    let session = engine.create_blank().unwrap();
    let before = session.snapshot().unwrap();
    let paragraph = initial_paragraph(&before);

    let result = session
        .insert_text(InsertTextRequest {
            base_revision: before.revision,
            at: Position {
                node: paragraph,
                grapheme_offset: 0,
                affinity: Affinity::After,
            },
            text: "OpenDoc 👩🏽‍💻".to_owned(),
            marks: BTreeSet::new(),
        })
        .unwrap();

    assert_eq!(result.revision, Revision::new(1));
    assert!(matches!(
        result.position_map.steps()[0],
        MappingStep::Insert { graphemes: 9, .. }
    ));
    assert_eq!(
        serde_json::to_value(session.snapshot().unwrap()).unwrap(),
        serde_json::json!({
            "schemaVersion": 0,
            "documentId": "00000000000000090000000000000001",
            "revision": 1,
            "body": [{
                "type": "paragraph",
                "id": "00000000000000090000000000000002",
                "inlines": [{
                    "type": "text",
                    "text": "OpenDoc 👩🏽‍💻",
                    "marks": []
                }]
            }]
        })
    );
}

#[test]
fn selection_only_update_preserves_revision_and_redo_history() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let blank = session.snapshot().unwrap();
    let paragraph = initial_paragraph(&blank);
    let initial = session.selection().unwrap();
    assert!(initial.is_collapsed());
    assert_eq!(
        initial.anchor,
        position(paragraph.clone(), 0, Affinity::After)
    );

    session
        .insert_text(InsertTextRequest {
            base_revision: blank.revision,
            at: initial.anchor,
            text: "ab".to_owned(),
            marks: BTreeSet::new(),
        })
        .unwrap();
    assert_eq!(session.selection().unwrap().focus.grapheme_offset, 2);
    session.undo(Revision::new(1)).unwrap();

    let before_selection_change = session.snapshot().unwrap();
    let before_affinity = position(paragraph.clone(), 0, Affinity::Before);
    session
        .set_selection(SetSelectionRequest {
            base_revision: before_selection_change.revision,
            selection: SelectionSnapshot {
                anchor: before_affinity.clone(),
                focus: before_affinity.clone(),
            },
        })
        .unwrap();

    assert_eq!(session.snapshot().unwrap(), before_selection_change);
    assert_eq!(
        session.selection().unwrap(),
        SelectionSnapshot {
            anchor: before_affinity.clone(),
            focus: before_affinity,
        }
    );
    session.redo(Revision::new(2)).unwrap();
    assert_eq!(paragraph_text(&session.snapshot().unwrap(), 0), "ab");
    assert_eq!(session.selection().unwrap().focus.grapheme_offset, 0);
}

#[test]
fn stale_and_invalid_selection_updates_preserve_selection() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let snapshot = session.snapshot().unwrap();
    let paragraph = initial_paragraph(&snapshot);
    let before = session.selection().unwrap();
    let invalid = SelectionSnapshot {
        anchor: position(paragraph.clone(), 1, Affinity::After),
        focus: position(paragraph, 1, Affinity::Before),
    };

    let stale = session
        .set_selection(SetSelectionRequest {
            base_revision: Revision::new(1),
            selection: invalid.clone(),
        })
        .unwrap_err();
    assert_eq!(stale.code(), ErrorCode::StaleRevision);
    assert_eq!(session.selection().unwrap(), before);

    let invalid_position = session
        .set_selection(SetSelectionRequest {
            base_revision: snapshot.revision,
            selection: invalid,
        })
        .unwrap_err();
    assert_eq!(invalid_position.code(), ErrorCode::InvalidPosition);
    assert_eq!(session.selection().unwrap(), before);
    assert_eq!(session.snapshot().unwrap(), snapshot);
}

#[test]
fn directed_selection_maps_through_structural_edits_and_history() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let blank = session.snapshot().unwrap();
    let first = initial_paragraph(&blank);
    session
        .insert_text(InsertTextRequest {
            base_revision: blank.revision,
            at: position(first.clone(), 0, Affinity::After),
            text: "abCD".to_owned(),
            marks: BTreeSet::new(),
        })
        .unwrap();
    assert_eq!(session.selection().unwrap().focus.grapheme_offset, 4);

    session
        .set_selection(SetSelectionRequest {
            base_revision: Revision::new(1),
            selection: SelectionSnapshot {
                anchor: position(first.clone(), 1, Affinity::Before),
                focus: position(first.clone(), 3, Affinity::After),
            },
        })
        .unwrap();
    let split = session
        .split_paragraph(SplitParagraphRequest {
            base_revision: Revision::new(1),
            at: position(first.clone(), 2, Affinity::After),
        })
        .unwrap();
    let second = match &split.position_map.steps()[0] {
        MappingStep::Split { new_node, .. } => new_node.clone(),
        other => panic!("unexpected mapping step: {other:?}"),
    };
    let after_split = session.selection().unwrap();
    assert_eq!(
        after_split.anchor,
        position(first.clone(), 1, Affinity::Before)
    );
    assert_eq!(
        after_split.focus,
        position(second.clone(), 1, Affinity::After)
    );
    session
        .set_selection(SetSelectionRequest {
            base_revision: Revision::new(2),
            selection: after_split.clone(),
        })
        .unwrap();
    assert_eq!(session.selection().unwrap(), after_split);

    session
        .delete_range(DeleteRangeRequest {
            base_revision: Revision::new(2),
            range: Range {
                start: position(second.clone(), 0, Affinity::Before),
                end: position(second.clone(), 1, Affinity::After),
            },
        })
        .unwrap();
    assert_eq!(
        session.selection().unwrap().focus,
        position(second.clone(), 0, Affinity::After)
    );

    session
        .join_paragraphs(JoinParagraphRequest {
            base_revision: Revision::new(3),
            first: first.clone(),
            second: second.clone(),
        })
        .unwrap();
    assert_eq!(
        session.selection().unwrap().focus,
        position(first.clone(), 2, Affinity::After)
    );

    session.undo(Revision::new(4)).unwrap();
    assert_eq!(
        session.selection().unwrap().focus,
        position(second, 0, Affinity::After)
    );
    session.redo(Revision::new(5)).unwrap();
    let after_redo = session.selection().unwrap();
    assert_eq!(
        after_redo.anchor,
        position(first.clone(), 1, Affinity::Before)
    );
    assert_eq!(after_redo.focus, position(first, 2, Affinity::After));
}

#[test]
fn independent_subscriptions_receive_ordered_transaction_and_selection_events() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let blank = session.snapshot().unwrap();
    let mut first_subscription = session.subscribe().unwrap();
    let mut second_subscription = session.subscribe().unwrap();

    session
        .insert_text(InsertTextRequest {
            base_revision: blank.revision,
            at: position(initial_paragraph(&blank), 0, Affinity::After),
            text: "A".to_owned(),
            marks: BTreeSet::new(),
        })
        .unwrap();

    let first_batch = first_subscription.drain(8).unwrap();
    let second_batch = second_subscription.drain(8).unwrap();
    assert_eq!(first_batch, second_batch);
    assert_eq!(first_batch.dropped_events, 0);
    assert_eq!(first_batch.events.len(), 2);
    assert_eq!(first_batch.events[0].sequence.get(), 1);
    assert_eq!(first_batch.events[1].sequence.get(), 2);
    match &first_batch.events[0].event {
        RuntimeEvent::TransactionCommitted(event) => {
            assert_eq!(event.origin, TransactionOrigin::Forward);
            assert_eq!(event.result.revision, Revision::new(1));
            assert_eq!(event.result.operations_applied, 1);
        }
        other => panic!("unexpected first event: {other:?}"),
    }
    match &first_batch.events[1].event {
        RuntimeEvent::SelectionChanged(event) => {
            assert_eq!(event.reason, SelectionChangeReason::Transaction);
            assert_eq!(event.revision, Revision::new(1));
            assert_eq!(event.selection.focus.grapheme_offset, 1);
        }
        other => panic!("unexpected second event: {other:?}"),
    }
    assert!(first_subscription.drain(8).unwrap().events.is_empty());
    let mut late_subscription = session.subscribe().unwrap();
    assert!(late_subscription.drain(8).unwrap().events.is_empty());
}

#[test]
fn explicit_selection_events_preserve_revision_and_suppress_no_op_updates() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let snapshot = session.snapshot().unwrap();
    let paragraph = initial_paragraph(&snapshot);
    let mut subscription = session.subscribe().unwrap();
    let initial = session.selection().unwrap();

    session
        .set_selection(SetSelectionRequest {
            base_revision: snapshot.revision,
            selection: initial,
        })
        .unwrap();
    assert!(subscription.drain(8).unwrap().events.is_empty());

    let changed = SelectionSnapshot {
        anchor: position(paragraph.clone(), 0, Affinity::Before),
        focus: position(paragraph, 0, Affinity::Before),
    };
    session
        .set_selection(SetSelectionRequest {
            base_revision: snapshot.revision,
            selection: changed.clone(),
        })
        .unwrap();
    let batch = subscription.drain(8).unwrap();
    assert_eq!(batch.events.len(), 1);
    match &batch.events[0].event {
        RuntimeEvent::SelectionChanged(event) => {
            assert_eq!(event.reason, SelectionChangeReason::Explicit);
            assert_eq!(event.revision, Revision::new(0));
            assert_eq!(event.selection, changed);
        }
        other => panic!("unexpected event: {other:?}"),
    }
    assert_eq!(session.snapshot().unwrap().revision, Revision::new(0));
}

#[test]
fn history_events_identify_undo_and_redo_causes() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let blank = session.snapshot().unwrap();
    let mut subscription = session.subscribe().unwrap();
    session
        .insert_text(InsertTextRequest {
            base_revision: blank.revision,
            at: position(initial_paragraph(&blank), 0, Affinity::After),
            text: "history".to_owned(),
            marks: BTreeSet::new(),
        })
        .unwrap();
    subscription.drain(8).unwrap();

    session.undo(Revision::new(1)).unwrap();
    let undo = subscription.drain(8).unwrap();
    assert!(matches!(
        &undo.events[0].event,
        RuntimeEvent::TransactionCommitted(TransactionCommittedEvent {
            origin: TransactionOrigin::Undo,
            ..
        })
    ));
    assert!(matches!(
        &undo.events[1].event,
        RuntimeEvent::SelectionChanged(SelectionChangedEvent {
            reason: SelectionChangeReason::Undo,
            ..
        })
    ));

    session.redo(Revision::new(2)).unwrap();
    let redo = subscription.drain(8).unwrap();
    assert!(matches!(
        &redo.events[0].event,
        RuntimeEvent::TransactionCommitted(TransactionCommittedEvent {
            origin: TransactionOrigin::Redo,
            ..
        })
    ));
    assert!(matches!(
        &redo.events[1].event,
        RuntimeEvent::SelectionChanged(SelectionChangedEvent {
            reason: SelectionChangeReason::Redo,
            ..
        })
    ));
}

#[test]
fn slow_subscription_reports_exact_bounded_event_gap() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let snapshot = session.snapshot().unwrap();
    let paragraph = initial_paragraph(&snapshot);
    let mut subscription = session.subscribe().unwrap();

    for index in 0..300 {
        let affinity = if index % 2 == 0 {
            Affinity::Before
        } else {
            Affinity::After
        };
        let endpoint = position(paragraph.clone(), 0, affinity);
        session
            .set_selection(SetSelectionRequest {
                base_revision: snapshot.revision,
                selection: SelectionSnapshot {
                    anchor: endpoint.clone(),
                    focus: endpoint,
                },
            })
            .unwrap();
    }

    let batch = subscription.drain(usize::MAX).unwrap();
    assert_eq!(batch.dropped_events, 44);
    assert_eq!(batch.events.len(), EVENT_JOURNAL_CAPACITY);
    assert_eq!(batch.events[0].sequence.get(), 45);
    assert_eq!(batch.events[255].sequence.get(), 300);
}

#[test]
fn invalid_drain_and_event_sequence_exhaustion_are_atomic() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let blank = session.snapshot().unwrap();
    let paragraph = initial_paragraph(&blank);
    let mut subscription = session.subscribe().unwrap();

    let drain_error = subscription.drain(0).unwrap_err();
    assert_eq!(drain_error.code(), ErrorCode::InvalidArgument);

    {
        let mut state = session.lock_state().unwrap();
        state.events.next_sequence = u64::MAX - 1;
    }
    let before_selection = session.selection().unwrap();
    let error = session
        .insert_text(InsertTextRequest {
            base_revision: blank.revision,
            at: position(paragraph, 0, Affinity::After),
            text: "A".to_owned(),
            marks: BTreeSet::new(),
        })
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::Internal);
    assert_eq!(session.snapshot().unwrap(), blank);
    assert_eq!(session.selection().unwrap(), before_selection);
    let state = session.state.read().unwrap();
    assert!(state.events.retained.is_empty());
    assert_eq!(state.events.next_sequence, u64::MAX - 1);
}

#[test]
fn stale_revision_has_stable_code_and_preserves_state() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let before = session.snapshot().unwrap();
    let paragraph = initial_paragraph(&before);
    let request = || InsertTextRequest {
        base_revision: Revision::new(0),
        at: Position {
            node: paragraph.clone(),
            grapheme_offset: 0,
            affinity: Affinity::After,
        },
        text: "A".to_owned(),
        marks: BTreeSet::new(),
    };

    session.insert_text(request()).unwrap();
    let error = session.insert_text(request()).unwrap_err();

    assert_eq!(error.code().as_str(), "ODC-2001");
    assert_eq!(session.snapshot().unwrap().revision, Revision::new(1));
}

#[test]
fn invalid_position_has_stable_code() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let snapshot = session.snapshot().unwrap();
    let error = session
        .insert_text(InsertTextRequest {
            base_revision: snapshot.revision,
            at: Position {
                node: initial_paragraph(&snapshot),
                grapheme_offset: 1,
                affinity: Affinity::After,
            },
            text: "A".to_owned(),
            marks: BTreeSet::new(),
        })
        .unwrap_err();

    assert_eq!(error.code().as_str(), "ODC-2002");
    assert_eq!(session.snapshot().unwrap(), snapshot);
}

#[test]
fn split_delete_undo_and_redo_are_revisioned() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let blank = session.snapshot().unwrap();
    let first = initial_paragraph(&blank);
    session
        .insert_text(InsertTextRequest {
            base_revision: blank.revision,
            at: Position {
                node: first.clone(),
                grapheme_offset: 0,
                affinity: Affinity::After,
            },
            text: "abCD".to_owned(),
            marks: BTreeSet::new(),
        })
        .unwrap();

    let split = session
        .split_paragraph(SplitParagraphRequest {
            base_revision: Revision::new(1),
            at: Position {
                node: first.clone(),
                grapheme_offset: 2,
                affinity: Affinity::After,
            },
        })
        .unwrap();
    let second = match &split.position_map.steps()[0] {
        MappingStep::Split { new_node, .. } => new_node.clone(),
        other => panic!("unexpected mapping step: {other:?}"),
    };
    let after_split = session.snapshot().unwrap();
    assert_eq!(paragraph_text(&after_split, 0), "ab");
    assert_eq!(paragraph_text(&after_split, 1), "CD");

    session
        .delete_range(DeleteRangeRequest {
            base_revision: Revision::new(2),
            range: Range {
                start: Position {
                    node: second.clone(),
                    grapheme_offset: 0,
                    affinity: Affinity::Before,
                },
                end: Position {
                    node: second,
                    grapheme_offset: 1,
                    affinity: Affinity::After,
                },
            },
        })
        .unwrap();
    assert_eq!(paragraph_text(&session.snapshot().unwrap(), 1), "D");

    session.undo(Revision::new(3)).unwrap();
    assert_eq!(paragraph_text(&session.snapshot().unwrap(), 1), "CD");
    session.undo(Revision::new(4)).unwrap();
    let joined = session.snapshot().unwrap();
    assert_eq!(joined.body.len(), 1);
    assert_eq!(paragraph_text(&joined, 0), "abCD");

    session.redo(Revision::new(5)).unwrap();
    assert_eq!(session.snapshot().unwrap().body.len(), 2);
    session.redo(Revision::new(6)).unwrap();
    let redone = session.snapshot().unwrap();
    assert_eq!(paragraph_text(&redone, 0), "ab");
    assert_eq!(paragraph_text(&redone, 1), "D");
    assert_eq!(redone.revision, Revision::new(7));
}

#[test]
fn failed_history_action_preserves_state() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let before = session.snapshot().unwrap();

    let error = session.undo(before.revision).unwrap_err();

    assert_eq!(error.code().as_str(), "ODC-2006");
    assert_eq!(session.snapshot().unwrap(), before);
}

#[test]
fn stale_undo_does_not_consume_history() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let blank = session.snapshot().unwrap();
    session
        .insert_text(InsertTextRequest {
            base_revision: blank.revision,
            at: Position {
                node: initial_paragraph(&blank),
                grapheme_offset: 0,
                affinity: Affinity::After,
            },
            text: "history".to_owned(),
            marks: BTreeSet::new(),
        })
        .unwrap();

    let error = session.undo(Revision::new(0)).unwrap_err();
    assert_eq!(error.code().as_str(), "ODC-2001");
    session.undo(Revision::new(1)).unwrap();
    assert_eq!(paragraph_text(&session.snapshot().unwrap(), 0), "");
}

#[test]
fn reversed_join_is_rejected_atomically() {
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine.create_blank().unwrap();
    let blank = session.snapshot().unwrap();
    let first = initial_paragraph(&blank);
    let split = session
        .split_paragraph(SplitParagraphRequest {
            base_revision: blank.revision,
            at: Position {
                node: first.clone(),
                grapheme_offset: 0,
                affinity: Affinity::After,
            },
        })
        .unwrap();
    let second = match &split.position_map.steps()[0] {
        MappingStep::Split { new_node, .. } => new_node.clone(),
        other => panic!("unexpected mapping step: {other:?}"),
    };
    let before = session.snapshot().unwrap();

    let error = session
        .join_paragraphs(JoinParagraphRequest {
            base_revision: before.revision,
            first: second,
            second: first,
        })
        .unwrap_err();

    assert_eq!(error.code().as_str(), "ODC-2002");
    assert_eq!(session.snapshot().unwrap(), before);
}

#[test]
fn normalized_json_load_export_and_node_allocation_are_deterministic() {
    let json = br#"{"schemaVersion":0,"documentId":"00000000000000010000000000000001","body":[{"type":"paragraph","id":"00000000000000010000000000000002","inlines":[{"type":"text","text":"loaded","marks":[]}]}],"extensions":{}}"#;
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let session = engine
        .open_normalized_json(json, OpenNormalizedOptions::default())
        .unwrap();
    assert_eq!(session.export_normalized_json().unwrap(), json);
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.revision, Revision::new(0));
    assert_eq!(
        session.selection().unwrap(),
        SelectionSnapshot {
            anchor: position(initial_paragraph(&snapshot), 0, Affinity::After),
            focus: position(initial_paragraph(&snapshot), 0, Affinity::After),
        }
    );

    let split = session
        .split_paragraph(SplitParagraphRequest {
            base_revision: snapshot.revision,
            at: Position {
                node: initial_paragraph(&snapshot),
                grapheme_offset: 6,
                affinity: Affinity::After,
            },
        })
        .unwrap();
    let generated = match &split.position_map.steps()[0] {
        MappingStep::Split { new_node, .. } => new_node,
        other => panic!("unexpected mapping step: {other:?}"),
    };
    assert_eq!(generated.as_str(), "00000000000000010000000000000003");
}

#[test]
fn normalized_json_errors_are_stable_and_redacted() {
    let malformed = br#"{
        "schemaVersion":0,
        "documentId":"00000000000000010000000000000001",
        "body":[{"type":"paragraph","id":"00000000000000010000000000000002","inlines":[]}],
        "extensions":{},
        "secret":"do-not-expose"
    }"#;
    let engine = Engine::new(EngineConfig::default()).unwrap();
    let malformed_error = engine
        .open_normalized_json(malformed, OpenNormalizedOptions::default())
        .unwrap_err();
    assert_eq!(malformed_error.code().as_str(), "ODC-1001");
    assert!(!malformed_error.to_string().contains("do-not-expose"));

    let options = OpenNormalizedOptions {
        limits: NormalizedSnapshotLimits {
            max_input_bytes: malformed.len() - 1,
            ..NormalizedSnapshotLimits::default()
        },
    };
    let limit_error = engine.open_normalized_json(malformed, options).unwrap_err();
    assert_eq!(limit_error.code().as_str(), "ODC-1003");
    assert_eq!(
        limit_error.context().get("limit_name").map(String::as_str),
        Some("input_json_bytes")
    );

    let configuration_error = engine
        .open_normalized_json(
            b"{}",
            OpenNormalizedOptions {
                limits: NormalizedSnapshotLimits {
                    max_input_bytes: 256 * 1024 * 1024 + 1,
                    ..NormalizedSnapshotLimits::default()
                },
            },
        )
        .unwrap_err();
    assert_eq!(configuration_error.code().as_str(), "ODC-0002");
}
