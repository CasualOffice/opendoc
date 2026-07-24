use std::collections::BTreeSet;

use super::*;

#[test]
fn node_id_uses_fixed_lowercase_hex() {
    let id = NodeId::from_parts(1, 2).unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"00000000000000010000000000000002\"");
    assert_eq!(serde_json::from_str::<NodeId>(&json).unwrap(), id);
    assert!(
        "0000000000000001000000000000000A"
            .parse::<NodeId>()
            .is_err()
    );
}

#[test]
fn blank_document_is_valid_and_deterministic() {
    let document = Document::blank(
        NodeId::from_parts(7, 1).unwrap(),
        NodeId::from_parts(7, 2).unwrap(),
    )
    .unwrap();

    assert_eq!(document.schema_version(), 0);
    assert_eq!(document.body().len(), 1);
    document.validate().unwrap();
    assert_eq!(
        serde_json::to_string(&document).unwrap(),
        "{\"schemaVersion\":0,\"documentId\":\"00000000000000070000000000000001\",\
         \"body\":[{\"type\":\"paragraph\",\"id\":\"00000000000000070000000000000002\",\
         \"inlines\":[]}],\"extensions\":{}}"
            .replace(' ', "")
    );
}

#[test]
fn insertion_respects_grapheme_boundaries() {
    let id = NodeId::from_parts(1, 1).unwrap();
    let mut paragraph = Paragraph::empty(id);
    paragraph
        .insert_text(0, "A👨‍👩‍👧‍👦B".to_owned(), BTreeSet::new())
        .unwrap();
    paragraph
        .insert_text(2, "X".to_owned(), BTreeSet::new())
        .unwrap();

    assert_eq!(paragraph.grapheme_len(), 4);
    assert_eq!(paragraph.plain_text(), "A👨‍👩‍👧‍👦XB");
    assert_eq!(paragraph.inlines().len(), 1);
}

#[test]
fn normalized_json_round_trip_is_byte_deterministic() {
    let document = Document::blank(
        NodeId::from_parts(3, 1).unwrap(),
        NodeId::from_parts(3, 2).unwrap(),
    )
    .unwrap();
    let first = document.to_json().unwrap();
    let loaded = Document::from_json(&first, SnapshotLimits::default()).unwrap();
    let second = loaded.to_json().unwrap();

    assert_eq!(first, second);
    assert_eq!(loaded, document);
}

#[test]
fn strict_json_rejects_unknown_and_duplicate_values() {
    let unknown = br#"{
        "schemaVersion":0,
        "documentId":"00000000000000030000000000000001",
        "body":[{"type":"paragraph","id":"00000000000000030000000000000002","inlines":[]}],
        "extensions":{},
        "future":true
    }"#;
    let duplicate_mark = br#"{
        "schemaVersion":0,
        "documentId":"00000000000000030000000000000001",
        "body":[{
            "type":"paragraph",
            "id":"00000000000000030000000000000002",
            "inlines":[{"type":"text","text":"x","marks":["bold","bold"]}]
        }],
        "extensions":{}
    }"#;
    let duplicate_extension = br#"{
        "schemaVersion":0,
        "documentId":"00000000000000030000000000000001",
        "body":[{"type":"paragraph","id":"00000000000000030000000000000002","inlines":[]}],
        "extensions":{
            "same":{"mediaType":"application/octet-stream","data":[1]},
            "same":{"mediaType":"application/octet-stream","data":[2]}
        }
    }"#;

    for invalid in [unknown.as_slice(), duplicate_mark, duplicate_extension] {
        assert_eq!(
            Document::from_json(invalid, SnapshotLimits::default()),
            Err(SnapshotError::MalformedJson)
        );
    }
}

#[test]
fn snapshot_limits_reject_before_and_after_parse() {
    let json = br#"{
        "schemaVersion":0,
        "documentId":"00000000000000030000000000000001",
        "body":[{
            "type":"paragraph",
            "id":"00000000000000030000000000000002",
            "inlines":[{"type":"text","text":"secret","marks":[]}]
        }],
        "extensions":{}
    }"#;
    let byte_limits = SnapshotLimits {
        max_input_bytes: json.len() - 1,
        ..SnapshotLimits::default()
    };
    assert!(matches!(
        Document::from_json(json, byte_limits),
        Err(SnapshotError::LimitExceeded {
            limit: "input_json_bytes",
            ..
        })
    ));

    let text_limits = SnapshotLimits {
        max_unicode_scalar_values: 5,
        ..SnapshotLimits::default()
    };
    let error = Document::from_json(json, text_limits).unwrap_err();
    assert!(matches!(
        error,
        SnapshotError::LimitExceeded {
            limit: "unicode_scalar_values",
            ..
        }
    ));
    assert!(!error.to_string().contains("secret"));
}

#[test]
fn every_snapshot_limit_has_a_stable_boundary_name() {
    let text_json = br#"{
        "schemaVersion":0,
        "documentId":"00000000000000030000000000000001",
        "body":[{
            "type":"paragraph",
            "id":"00000000000000030000000000000002",
            "inlines":[{"type":"text","text":"abcdef","marks":[]}]
        }],
        "extensions":{}
    }"#;
    let cases = [
        (
            SnapshotLimits {
                max_blocks: 0,
                ..SnapshotLimits::default()
            },
            "body_blocks",
        ),
        (
            SnapshotLimits {
                max_unicode_scalar_values: 5,
                ..SnapshotLimits::default()
            },
            "unicode_scalar_values",
        ),
        (
            SnapshotLimits {
                max_text_run_bytes: 5,
                ..SnapshotLimits::default()
            },
            "text_run_bytes",
        ),
    ];
    for (limits, expected_name) in cases {
        assert!(matches!(
            Document::from_json(text_json, limits),
            Err(SnapshotError::LimitExceeded { limit, .. }) if limit == expected_name
        ));
    }

    let extension_json = br#"{
        "schemaVersion":0,
        "documentId":"00000000000000030000000000000001",
        "body":[{"type":"paragraph","id":"00000000000000030000000000000002","inlines":[]}],
        "extensions":{"x":{"mediaType":"x","data":[1,2]}}
    }"#;
    assert!(matches!(
        Document::from_json(
            extension_json,
            SnapshotLimits {
                max_extension_entries: 0,
                ..SnapshotLimits::default()
            }
        ),
        Err(SnapshotError::LimitExceeded {
            limit: "extension_entries",
            ..
        })
    ));
    assert!(matches!(
        Document::from_json(
            extension_json,
            SnapshotLimits {
                max_extension_bytes: 2,
                ..SnapshotLimits::default()
            }
        ),
        Err(SnapshotError::LimitExceeded {
            limit: "extension_payload_bytes",
            ..
        })
    ));

    assert!(
        Document::from_json(
            text_json,
            SnapshotLimits {
                max_input_bytes: text_json.len(),
                max_blocks: 1,
                max_unicode_scalar_values: 6,
                max_text_run_bytes: 6,
                ..SnapshotLimits::default()
            }
        )
        .is_ok()
    );
}

#[test]
fn hard_ceiling_is_not_host_bypassable() {
    let limits = SnapshotLimits {
        max_input_bytes: SnapshotLimits::HARD_MAX_INPUT_BYTES + 1,
        ..SnapshotLimits::default()
    };
    assert!(matches!(
        Document::from_json(b"{}", limits),
        Err(SnapshotError::InvalidLimitConfiguration {
            limit: "input_json_bytes",
            ..
        })
    ));
}

#[test]
fn duplicate_node_ids_fail_invariant_validation() {
    let json = br#"{
        "schemaVersion":0,
        "documentId":"00000000000000030000000000000001",
        "body":[{
            "type":"paragraph",
            "id":"00000000000000030000000000000001",
            "inlines":[]
        }],
        "extensions":{}
    }"#;
    assert!(matches!(
        Document::from_json(json, SnapshotLimits::default()),
        Err(SnapshotError::InvalidModel(ModelError::DuplicateNodeId(_)))
    ));
}
