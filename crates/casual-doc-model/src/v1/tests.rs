use std::collections::BTreeSet;

use super::*;
use crate::{
    Document as V0Document, IdGenerator, Mark, ModelError, NodeId, SnapshotError, SnapshotLimits,
};

fn v0_blank() -> V0Document {
    V0Document::blank(
        NodeId::from_parts(7, 1).unwrap(),
        NodeId::from_parts(7, 2).unwrap(),
    )
    .unwrap()
}

#[test]
fn blank_v0_migrates_to_canonical_v1_bytes() {
    let source = v0_blank();
    let mut ids = IdGenerator::new(9);
    let migrated = Document::from_v0(&source, &mut ids).unwrap();
    let json = String::from_utf8(migrated.to_json().unwrap()).unwrap();
    assert_eq!(
        json,
        "{\"schemaVersion\":1,\
             \"documentId\":\"00000000000000070000000000000001\",\
             \"body\":[{\"type\":\"paragraph\",\
             \"id\":\"00000000000000070000000000000002\",\
             \"properties\":{},\"inlines\":[]}],\
             \"definitions\":{\"styles\":{},\"abstractNumbering\":{},\
             \"numbering\":{},\"sections\":[],\"media\":{}}}"
    );
}

#[test]
fn marks_migrate_to_run_properties() {
    let mut paragraph = crate::Paragraph::empty(NodeId::from_parts(1, 2).unwrap());
    let marks = BTreeSet::from([Mark::Bold, Mark::Strike]);
    paragraph.insert_text(0, "Hi".to_owned(), marks).unwrap();
    let source = document_with_paragraph(paragraph);

    let mut ids = IdGenerator::new(5);
    let migrated = Document::from_v0(&source, &mut ids).unwrap();
    let BlockNode::Paragraph(result) = &migrated.body()[0];
    let InlineNode::Run(run) = &result.inlines[0] else {
        panic!("expected a run");
    };
    assert_eq!(run.text, "Hi");
    assert_eq!(run.properties.bold, Some(true));
    assert_eq!(run.properties.strike, Some(true));
    assert_eq!(run.properties.italic, None);
}

#[test]
fn migration_is_deterministic_and_reload_is_a_fixed_point() {
    let source = v0_blank();
    let first = Document::from_v0(&source, &mut IdGenerator::new(9))
        .unwrap()
        .to_json()
        .unwrap();
    let second = Document::from_v0(&source, &mut IdGenerator::new(9))
        .unwrap()
        .to_json()
        .unwrap();
    assert_eq!(first, second);

    let reloaded = Document::from_json(&first, SnapshotLimits::default()).unwrap();
    assert_eq!(reloaded.to_json().unwrap(), first);
}

#[test]
fn populated_v0_extensions_are_rejected_not_dropped() {
    let json = br#"{
            "schemaVersion":0,
            "documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","inlines":[]}],
            "extensions":{"x":{"mediaType":"application/octet-stream","data":[1]}}
        }"#;
    let source = V0Document::from_json(json, SnapshotLimits::default()).unwrap();
    assert_eq!(
        Document::from_v0(&source, &mut IdGenerator::new(1)),
        Err(MigrationError::UnsupportedSourceExtensions)
    );
}

#[test]
fn strict_json_rejects_unknown_fields_and_v0_extensions_field() {
    let unknown = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{},"future":true}"#;
    let has_extensions = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{},"extensions":{}}"#;
    for invalid in [unknown.as_slice(), has_extensions] {
        assert_eq!(
            Document::from_json(invalid, SnapshotLimits::default()),
            Err(SnapshotError::MalformedJson)
        );
    }
}

#[test]
fn wrong_schema_version_is_rejected() {
    let json = br#"{"schemaVersion":2,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{}}"#;
    assert_eq!(
        Document::from_json(json, SnapshotLimits::default()),
        Err(SnapshotError::InvalidModel(
            ModelError::UnsupportedSchemaVersion(2)
        ))
    );
}

#[test]
fn dangling_style_reference_is_rejected() {
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002",
              "properties":{"styleRef":"000000000000000000000000000000ff"},"inlines":[]}],
            "definitions":{}}"#;
    assert!(matches!(
        Document::from_json(json, SnapshotLimits::default()),
        Err(SnapshotError::InvalidModel(ModelError::DanglingStyleRef(_)))
    ));
}

#[test]
fn based_on_cycle_and_kind_mismatch_are_rejected() {
    let cycle = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"styles":{
              "0000000000000000000000000000000a":{"kind":"paragraph","basedOn":"0000000000000000000000000000000b"},
              "0000000000000000000000000000000b":{"kind":"paragraph","basedOn":"0000000000000000000000000000000a"}
            }}}"#;
    assert!(matches!(
        Document::from_json(cycle, SnapshotLimits::default()),
        Err(SnapshotError::InvalidModel(ModelError::StyleBasedOnCycle(
            _
        )))
    ));

    let mismatch = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"styles":{
              "0000000000000000000000000000000a":{"kind":"paragraph","basedOn":"0000000000000000000000000000000b"},
              "0000000000000000000000000000000b":{"kind":"character"}
            }}}"#;
    assert!(matches!(
        Document::from_json(mismatch, SnapshotLimits::default()),
        Err(SnapshotError::InvalidModel(
            ModelError::StyleBasedOnKindMismatch { .. }
        ))
    ));
}

#[test]
fn numbering_reference_integrity_is_enforced() {
    let dangling = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002",
              "properties":{"numbering":{"instance":"000000000000000000000000000000aa","level":0}},"inlines":[]}],
            "definitions":{}}"#;
    assert!(matches!(
        Document::from_json(dangling, SnapshotLimits::default()),
        Err(SnapshotError::InvalidModel(
            ModelError::DanglingNumberingRef(_)
        ))
    ));
}

#[test]
fn out_of_domain_run_size_is_rejected() {
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[{"type":"run","id":"00000000000000030000000000000003",
                "properties":{"sizeHalfPoints":0},"text":"x"}]}],
            "definitions":{}}"#;
    assert!(matches!(
        Document::from_json(json, SnapshotLimits::default()),
        Err(SnapshotError::InvalidModel(
            ModelError::PropertyValueOutOfDomain {
                property: "run.size_half_points"
            }
        ))
    ));
}

#[test]
fn adjacent_equal_runs_are_rejected_but_a_tab_between_them_is_accepted() {
    let adjacent = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[
                {"type":"run","id":"00000000000000030000000000000003","properties":{},"text":"a"},
                {"type":"run","id":"00000000000000030000000000000004","properties":{},"text":"b"}
              ]}],
            "definitions":{}}"#;
    assert!(matches!(
        Document::from_json(adjacent, SnapshotLimits::default()),
        Err(SnapshotError::InvalidModel(
            ModelError::AdjacentEquivalentTextRuns(_)
        ))
    ));

    let with_tab = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[
                {"type":"run","id":"00000000000000030000000000000003","properties":{},"text":"a"},
                {"type":"tab","id":"00000000000000030000000000000005"},
                {"type":"run","id":"00000000000000030000000000000004","properties":{},"text":"b"}
              ]}],
            "definitions":{}}"#;
    assert!(Document::from_json(with_tab, SnapshotLimits::default()).is_ok());
}

#[test]
fn named_font_round_trips() {
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[{"type":"run","id":"00000000000000030000000000000003",
                "properties":{"fontRef":{"type":"named","name":"Arial"}},"text":"x"}]}],
            "definitions":{}}"#;
    let document = Document::from_json(json, SnapshotLimits::default()).unwrap();
    let reexport = document.to_json().unwrap();
    let reloaded = Document::from_json(&reexport, SnapshotLimits::default()).unwrap();
    assert_eq!(reloaded.to_json().unwrap(), reexport);
}

fn expect_invalid(json: &[u8]) -> ModelError {
    match Document::from_json(json, SnapshotLimits::default()) {
        Err(SnapshotError::InvalidModel(error)) => error,
        other => panic!("expected InvalidModel, got {other:?}"),
    }
}

#[test]
fn document_defaults_properties_are_validated() {
    let dangling = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"documentDefaults":{"paragraph":{"styleRef":"000000000000000000000000000000ff"}}}}"#;
    assert!(matches!(
        expect_invalid(dangling),
        ModelError::DanglingStyleRef(_)
    ));
    let out_of_domain = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"documentDefaults":{"run":{"sizeHalfPoints":0}}}}"#;
    assert!(matches!(
        expect_invalid(out_of_domain),
        ModelError::PropertyValueOutOfDomain {
            property: "run.size_half_points"
        }
    ));
}

#[test]
fn numbering_overrides_are_validated() {
    let base = |overrides: &str| {
        format!(
                "{{\"schemaVersion\":1,\"documentId\":\"00000000000000030000000000000001\",\
                 \"body\":[{{\"type\":\"paragraph\",\"id\":\"00000000000000030000000000000002\",\"properties\":{{}},\"inlines\":[]}}],\
                 \"definitions\":{{\"abstractNumbering\":{{\"0000000000000000000000000000000a\":{{\"levels\":[{{\"level\":0,\"start\":1}}]}}}},\
                 \"numbering\":{{\"0000000000000000000000000000000b\":{{\"abstractRef\":\"0000000000000000000000000000000a\",\"overrides\":{overrides}}}}}}}}}"
            ).into_bytes()
    };
    assert!(matches!(
        expect_invalid(&base("[{\"level\":9,\"start\":1}]")),
        ModelError::NumberingLevelUndefined { level: 9, .. }
    ));
    assert!(matches!(
        expect_invalid(&base("[{\"level\":0,\"start\":60000}]")),
        ModelError::PropertyValueOutOfDomain {
            property: "numbering.override.start"
        }
    ));
}

#[test]
fn undefined_numbering_level_reference_is_rejected() {
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002",
              "properties":{"numbering":{"instance":"0000000000000000000000000000000b","level":5}},"inlines":[]}],
            "definitions":{"abstractNumbering":{"0000000000000000000000000000000a":{"levels":[{"level":0,"start":1}]}},
              "numbering":{"0000000000000000000000000000000b":{"abstractRef":"0000000000000000000000000000000a"}}}}"#;
    assert!(matches!(
        expect_invalid(json),
        ModelError::NumberingLevelUndefined { level: 5, .. }
    ));
}

#[test]
fn section_geometry_domains_are_enforced() {
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"sections":[{"id":"0000000000000000000000000000000c",
              "pageSize":{"widthTwips":-1,"heightTwips":100},
              "pageMargins":{"topTwips":0,"bottomTwips":0,"startTwips":0,"endTwips":0},
              "columns":{"count":1}}]}}"#;
    assert!(matches!(
        expect_invalid(json),
        ModelError::PropertyValueOutOfDomain {
            property: "section.page_size.width"
        }
    ));
}

#[test]
fn media_reference_fields_are_validated() {
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"media":{"0000000000000000000000000000000d":{"relationshipId":"rId1","mediaType":"","partName":"word/media/x.png"}}}}"#;
    assert!(matches!(
        expect_invalid(json),
        ModelError::PropertyValueOutOfDomain {
            property: "media.media_type"
        }
    ));
}

#[test]
fn duplicate_definition_map_key_is_rejected() {
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"styles":{
              "0000000000000000000000000000000a":{"kind":"paragraph"},
              "0000000000000000000000000000000a":{"kind":"character"}
            }}}"#;
    assert_eq!(
        Document::from_json(json, SnapshotLimits::default()),
        Err(SnapshotError::MalformedJson)
    );
}

#[test]
fn cross_table_duplicate_node_id_is_rejected() {
    // A style key equal to the paragraph id.
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"styles":{"00000000000000030000000000000002":{"kind":"paragraph"}}}}"#;
    assert!(matches!(
        expect_invalid(json),
        ModelError::DuplicateNodeId(_)
    ));
}

#[test]
fn empty_run_text_is_rejected() {
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[{"type":"run","id":"00000000000000030000000000000003","properties":{},"text":""}]}],
            "definitions":{}}"#;
    assert!(matches!(expect_invalid(json), ModelError::EmptyTextRun));
}

#[test]
fn break_inlines_round_trip_and_separate_equal_runs() {
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[
                {"type":"run","id":"00000000000000030000000000000003","properties":{},"text":"a"},
                {"type":"break","id":"00000000000000030000000000000005","kind":"page"},
                {"type":"run","id":"00000000000000030000000000000004","properties":{},"text":"b"}
              ]}],
            "definitions":{}}"#;
    let document = Document::from_json(json, SnapshotLimits::default()).unwrap();
    let reexport = document.to_json().unwrap();
    assert_eq!(
        Document::from_json(&reexport, SnapshotLimits::default())
            .unwrap()
            .to_json()
            .unwrap(),
        reexport
    );
}

#[test]
fn migration_skips_ids_that_collide_with_preserved_paragraph_ids() {
    // Seed the IdGenerator in the same (namespace, counter) space as the
    // preserved paragraph id so the first candidate collides and is skipped.
    let mut paragraph = crate::Paragraph::empty(NodeId::from_parts(4, 1).unwrap());
    paragraph
        .insert_text(0, "x".to_owned(), BTreeSet::new())
        .unwrap();
    let source = document_with_paragraph_ids(NodeId::from_parts(4, 9).unwrap(), paragraph);

    let migrated = Document::from_v0(&source, &mut IdGenerator::new(4)).unwrap();
    let BlockNode::Paragraph(result) = &migrated.body()[0];
    let InlineNode::Run(run) = &result.inlines[0] else {
        panic!("expected a run");
    };
    // Candidate (4,1) collides with the preserved paragraph id, so the run
    // receives (4,2); output re-validates and is deterministic.
    assert_eq!(run.id, NodeId::from_parts(4, 2).unwrap());
    migrated.validate().unwrap();
}

fn document_with_paragraph_ids(document_id: NodeId, paragraph: crate::Paragraph) -> V0Document {
    let json = format!(
        "{{\"schemaVersion\":0,\"documentId\":\"{document_id}\",\"body\":[{}],\"extensions\":{{}}}}",
        serde_json::to_string(&crate::BlockNode::Paragraph(paragraph)).unwrap()
    );
    V0Document::from_json(json.as_bytes(), SnapshotLimits::default()).unwrap()
}

fn document_with_paragraph(paragraph: crate::Paragraph) -> V0Document {
    let json = format!(
        "{{\"schemaVersion\":0,\"documentId\":\"{}\",\"body\":[{}],\"extensions\":{{}}}}",
        NodeId::from_parts(1, 1).unwrap(),
        serde_json::to_string(&crate::BlockNode::Paragraph(paragraph)).unwrap()
    );
    V0Document::from_json(json.as_bytes(), SnapshotLimits::default()).unwrap()
}
