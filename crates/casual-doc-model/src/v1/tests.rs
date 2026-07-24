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
    let BlockNode::Paragraph(result) = &migrated.body()[0] else {
        panic!("expected a paragraph");
    };
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
    let BlockNode::Paragraph(result) = &migrated.body()[0] else {
        panic!("expected a paragraph");
    };
    let InlineNode::Run(run) = &result.inlines[0] else {
        panic!("expected a run");
    };
    // Candidate (4,1) collides with the preserved paragraph id, so the run
    // receives (4,2); output re-validates and is deterministic.
    assert_eq!(run.id, NodeId::from_parts(4, 2).unwrap());
    migrated.validate().unwrap();
}

const MEDIA_DEFS: &str = r#""definitions":{"media":{"0000000000000000000000000000000d":{"relationshipId":"rId1","mediaType":"image/png","partName":"word/media/x.png"}}}"#;

fn roundtrips(json: &[u8]) {
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
fn drawing_with_and_without_extent_round_trips() {
    let json = format!(
        r#"{{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{{"type":"paragraph","id":"00000000000000030000000000000002","properties":{{}},
              "inlines":[
                {{"type":"drawing","id":"00000000000000030000000000000003","media":"0000000000000000000000000000000d","extent":{{"widthEmu":9525,"heightEmu":19050}}}},
                {{"type":"drawing","id":"00000000000000030000000000000004","media":"0000000000000000000000000000000d"}}
              ]}}],
            {MEDIA_DEFS}}}"#
    );
    roundtrips(json.as_bytes());
    // An absent extent emits no key.
    let document = Document::from_json(json.as_bytes(), SnapshotLimits::default()).unwrap();
    let text = String::from_utf8(document.to_json().unwrap()).unwrap();
    assert!(text.contains(r#""media":"0000000000000000000000000000000d""#));
    assert_eq!(text.matches("extent").count(), 1);
}

#[test]
fn drawing_referencing_absent_media_is_rejected() {
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[{"type":"drawing","id":"00000000000000030000000000000003","media":"0000000000000000000000000000000e"}]}],
            "definitions":{}}"#;
    assert!(matches!(
        expect_invalid(json),
        ModelError::DanglingMediaRef(_)
    ));
}

#[test]
fn drawing_extent_out_of_domain_is_rejected() {
    let json = format!(
        r#"{{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{{"type":"paragraph","id":"00000000000000030000000000000002","properties":{{}},
              "inlines":[{{"type":"drawing","id":"00000000000000030000000000000003","media":"0000000000000000000000000000000d","extent":{{"widthEmu":27273042316901,"heightEmu":1}}}}]}}],
            {MEDIA_DEFS}}}"#
    );
    assert!(matches!(
        expect_invalid(json.as_bytes()),
        ModelError::PropertyValueOutOfDomain {
            property: "drawing.extent.width"
        }
    ));
}

#[test]
fn drawing_between_equal_runs_is_accepted() {
    let json = format!(
        r#"{{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{{"type":"paragraph","id":"00000000000000030000000000000002","properties":{{}},
              "inlines":[
                {{"type":"run","id":"00000000000000030000000000000003","properties":{{}},"text":"a"}},
                {{"type":"drawing","id":"00000000000000030000000000000005","media":"0000000000000000000000000000000d"}},
                {{"type":"run","id":"00000000000000030000000000000004","properties":{{}},"text":"b"}}
              ]}}],
            {MEDIA_DEFS}}}"#
    );
    roundtrips(json.as_bytes());
}

#[test]
fn hyperlink_targets_round_trip() {
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[
                {"type":"hyperlink","id":"00000000000000030000000000000003","target":{"type":"external","url":"https://example.com/docs"},"tooltip":"docs","inlines":[
                  {"type":"run","id":"00000000000000030000000000000004","properties":{},"text":"the "},
                  {"type":"run","id":"00000000000000030000000000000005","properties":{"bold":true},"text":"docs"}
                ]},
                {"type":"hyperlink","id":"00000000000000030000000000000006","target":{"type":"internal","anchor":"top"},"inlines":[
                  {"type":"run","id":"00000000000000030000000000000007","properties":{},"text":"top"}
                ]}
              ]}],
            "definitions":{}}"#;
    roundtrips(json);
    // An absent tooltip emits no key (only the external link declares one).
    let document = Document::from_json(json, SnapshotLimits::default()).unwrap();
    let text = String::from_utf8(document.to_json().unwrap()).unwrap();
    assert_eq!(text.matches("tooltip").count(), 1);
}

#[test]
fn empty_hyperlink_is_rejected() {
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[{"type":"hyperlink","id":"00000000000000030000000000000003","target":{"type":"internal","anchor":"top"},"inlines":[]}]}],
            "definitions":{}}"#;
    assert!(matches!(
        expect_invalid(json),
        ModelError::EmptyHyperlink(_)
    ));
}

#[test]
fn nested_hyperlink_is_rejected() {
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[{"type":"hyperlink","id":"00000000000000030000000000000003","target":{"type":"internal","anchor":"a"},"inlines":[
                {"type":"hyperlink","id":"00000000000000030000000000000004","target":{"type":"internal","anchor":"b"},"inlines":[
                  {"type":"run","id":"00000000000000030000000000000005","properties":{},"text":"x"}
                ]}
              ]}]}],
            "definitions":{}}"#;
    assert!(matches!(
        expect_invalid(json),
        ModelError::NestedHyperlink(_)
    ));
}

#[test]
fn hyperlink_child_id_collision_is_rejected() {
    // A child run id equal to the paragraph id: proves record_inline_ids recurses.
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[{"type":"hyperlink","id":"00000000000000030000000000000003","target":{"type":"internal","anchor":"a"},"inlines":[
                {"type":"run","id":"00000000000000030000000000000002","properties":{},"text":"x"}
              ]}]}],
            "definitions":{}}"#;
    assert!(matches!(
        expect_invalid(json),
        ModelError::DuplicateNodeId(_)
    ));
}

#[test]
fn over_domain_hyperlink_url_is_rejected() {
    let long = "h".repeat(2049);
    let json = format!(
        r#"{{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{{"type":"paragraph","id":"00000000000000030000000000000002","properties":{{}},
              "inlines":[{{"type":"hyperlink","id":"00000000000000030000000000000003","target":{{"type":"external","url":"{long}"}},"inlines":[
                {{"type":"run","id":"00000000000000030000000000000004","properties":{{}},"text":"x"}}
              ]}}]}}],
            "definitions":{{}}}}"#
    );
    assert!(matches!(
        expect_invalid(json.as_bytes()),
        ModelError::PropertyValueOutOfDomain {
            property: "hyperlink.external.url"
        }
    ));
}

// ---- schema v1 tables ----------------------------------------------------

/// A distinct node id in namespace 1 (block ids); document ids use namespace 9.
fn tid(counter: u64) -> NodeId {
    NodeId::from_parts(1, counter).unwrap()
}

fn paragraph_block(id: NodeId) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id,
        properties: ParagraphProperties::default(),
        inlines: Vec::new(),
    })
}

fn cell(id: NodeId, properties: TableCellProperties, blocks: Vec<BlockNode>) -> TableCell {
    TableCell {
        id,
        properties,
        blocks,
    }
}

fn table_document(body: Vec<BlockNode>) -> Result<Document, ModelError> {
    Document::new(
        NodeId::from_parts(9, 1).unwrap(),
        body,
        Definitions::default(),
    )
}

#[test]
fn valid_table_with_merges_validates_and_round_trips_json() {
    // A 2x2 table: top-left spans two grid columns (gridSpan), and the
    // right column vertically merges (Restart over Continue).
    let table = BlockNode::Table(Table {
        id: tid(10),
        grid: vec![
            GridColumn {
                width_twips: Some(2_880),
            },
            GridColumn {
                width_twips: Some(2_880),
            },
        ],
        rows: vec![
            TableRow {
                id: tid(11),
                cells: vec![
                    cell(
                        tid(12),
                        TableCellProperties {
                            grid_span: Some(2),
                            ..TableCellProperties::default()
                        },
                        vec![paragraph_block(tid(13))],
                    ),
                    cell(
                        tid(14),
                        TableCellProperties {
                            vertical_merge: Some(VerticalMerge::Restart),
                            ..TableCellProperties::default()
                        },
                        vec![paragraph_block(tid(15))],
                    ),
                ],
            },
            TableRow {
                id: tid(16),
                cells: vec![
                    cell(
                        tid(17),
                        TableCellProperties::default(),
                        vec![paragraph_block(tid(18))],
                    ),
                    cell(
                        tid(19),
                        TableCellProperties {
                            vertical_merge: Some(VerticalMerge::Continue),
                            ..TableCellProperties::default()
                        },
                        vec![paragraph_block(tid(20))],
                    ),
                ],
            },
        ],
    });

    let document = table_document(vec![table]).unwrap();
    let json = document.to_json().unwrap();
    let reloaded = Document::from_json(&json, SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);

    let BlockNode::Table(table) = &document.body()[0] else {
        panic!("expected a table");
    };
    assert_eq!(table.grid.len(), 2);
    assert_eq!(table.rows[0].cells[0].properties.grid_span, Some(2));
    assert_eq!(
        table.rows[0].cells[1].properties.vertical_merge,
        Some(VerticalMerge::Restart)
    );
    assert_eq!(
        table.rows[1].cells[1].properties.vertical_merge,
        Some(VerticalMerge::Continue)
    );
}

#[test]
fn empty_table_is_rejected() {
    let table = BlockNode::Table(Table {
        id: tid(10),
        grid: Vec::new(),
        rows: Vec::new(),
    });
    assert!(matches!(
        table_document(vec![table]),
        Err(ModelError::EmptyTable(_))
    ));
}

#[test]
fn table_row_without_cells_is_rejected() {
    let table = BlockNode::Table(Table {
        id: tid(10),
        grid: Vec::new(),
        rows: vec![TableRow {
            id: tid(11),
            cells: Vec::new(),
        }],
    });
    assert!(matches!(
        table_document(vec![table]),
        Err(ModelError::EmptyTableRow(_))
    ));
}

#[test]
fn table_cell_without_blocks_is_rejected() {
    let table = BlockNode::Table(Table {
        id: tid(10),
        grid: Vec::new(),
        rows: vec![TableRow {
            id: tid(11),
            cells: vec![cell(tid(12), TableCellProperties::default(), Vec::new())],
        }],
    });
    assert!(matches!(
        table_document(vec![table]),
        Err(ModelError::EmptyTableCell(_))
    ));
}

#[test]
fn grid_span_out_of_domain_is_rejected() {
    let table = BlockNode::Table(Table {
        id: tid(10),
        grid: Vec::new(),
        rows: vec![TableRow {
            id: tid(11),
            cells: vec![cell(
                tid(12),
                TableCellProperties {
                    grid_span: Some(0),
                    ..TableCellProperties::default()
                },
                vec![paragraph_block(tid(13))],
            )],
        }],
    });
    assert!(matches!(
        table_document(vec![table]),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "table.cell.grid_span"
        })
    ));
}

#[test]
fn duplicate_id_inside_a_cell_is_rejected() {
    // The cell id collides with a nested paragraph id.
    let table = BlockNode::Table(Table {
        id: tid(10),
        grid: Vec::new(),
        rows: vec![TableRow {
            id: tid(11),
            cells: vec![cell(
                tid(12),
                TableCellProperties::default(),
                vec![paragraph_block(tid(12))],
            )],
        }],
    });
    assert!(matches!(
        table_document(vec![table]),
        Err(ModelError::DuplicateNodeId(_))
    ));
}

fn wrap_in_tables(depth: u32, counter: &mut u64) -> BlockNode {
    if depth == 0 {
        *counter += 1;
        return paragraph_block(tid(*counter));
    }
    let inner = wrap_in_tables(depth - 1, counter);
    *counter += 1;
    let table_id = tid(*counter);
    *counter += 1;
    let row_id = tid(*counter);
    *counter += 1;
    let cell_id = tid(*counter);
    BlockNode::Table(Table {
        id: table_id,
        grid: Vec::new(),
        rows: vec![TableRow {
            id: row_id,
            cells: vec![cell(cell_id, TableCellProperties::default(), vec![inner])],
        }],
    })
}

#[test]
fn table_nesting_within_bound_validates() {
    let mut counter = 0;
    let block = wrap_in_tables(MAX_TABLE_DEPTH, &mut counter);
    assert!(table_document(vec![block]).is_ok());
}

#[test]
fn table_nesting_beyond_bound_is_rejected() {
    let mut counter = 0;
    let block = wrap_in_tables(MAX_TABLE_DEPTH + 1, &mut counter);
    assert!(matches!(
        table_document(vec![block]),
        Err(ModelError::TableNestingTooDeep(_))
    ));
}

#[test]
fn nested_table_block_count_is_bounded() {
    // A table (1) + row (1) + cell (1) + nested paragraph (1) = 4 blocks; a
    // max_blocks of 3 must reject via the snapshot limit, not silently pass.
    let table = BlockNode::Table(Table {
        id: tid(10),
        grid: Vec::new(),
        rows: vec![TableRow {
            id: tid(11),
            cells: vec![cell(
                tid(12),
                TableCellProperties::default(),
                vec![paragraph_block(tid(13))],
            )],
        }],
    });
    let document = table_document(vec![table]).unwrap();
    let json = document.to_json().unwrap();
    let limits = SnapshotLimits {
        max_blocks: 3,
        ..SnapshotLimits::default()
    };
    assert!(matches!(
        Document::from_json(&json, limits),
        Err(SnapshotError::LimitExceeded {
            limit: "body_blocks",
            ..
        })
    ));
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

// ---- schema v1 fields ----------------------------------------------------

fn run_inline(id: NodeId, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id,
        properties: RunProperties::default(),
        text: text.to_owned(),
    })
}

fn field_paragraph(field: Field) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: tid(1),
        properties: ParagraphProperties::default(),
        inlines: vec![InlineNode::Field(field)],
    })
}

#[test]
fn field_with_cached_result_validates_and_round_trips_json() {
    let field = Field {
        id: tid(10),
        instruction: " PAGE ".to_owned(),
        inlines: vec![run_inline(tid(11), "7")],
    };
    let document = table_document(vec![field_paragraph(field)]).unwrap();
    let json = document.to_json().unwrap();
    let reloaded = Document::from_json(&json, SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);

    let BlockNode::Paragraph(paragraph) = &document.body()[0] else {
        panic!("expected a paragraph");
    };
    let InlineNode::Field(field) = &paragraph.inlines[0] else {
        panic!("expected a field");
    };
    assert_eq!(field.instruction, " PAGE ");
    assert_eq!(field.inlines.len(), 1);
}

#[test]
fn field_with_empty_cached_result_is_valid() {
    let field = Field {
        id: tid(10),
        instruction: " TIME ".to_owned(),
        inlines: Vec::new(),
    };
    assert!(table_document(vec![field_paragraph(field)]).is_ok());
}

#[test]
fn empty_field_instruction_is_rejected() {
    let field = Field {
        id: tid(10),
        instruction: String::new(),
        inlines: Vec::new(),
    };
    assert!(matches!(
        table_document(vec![field_paragraph(field)]),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "field.instruction"
        })
    ));
}

#[test]
fn field_inside_a_hyperlink_is_rejected() {
    let inner_field = InlineNode::Field(Field {
        id: tid(12),
        instruction: " PAGE ".to_owned(),
        inlines: Vec::new(),
    });
    let link = InlineNode::Hyperlink(Hyperlink {
        id: tid(10),
        target: HyperlinkTarget::Internal(InternalTarget {
            anchor: "top".to_owned(),
        }),
        tooltip: None,
        inlines: vec![inner_field],
    });
    let block = BlockNode::Paragraph(Paragraph {
        id: tid(1),
        properties: ParagraphProperties::default(),
        inlines: vec![link],
    });
    assert!(matches!(
        table_document(vec![block]),
        Err(ModelError::NestedField(_))
    ));
}

#[test]
fn hyperlink_inside_a_field_is_rejected() {
    let inner_link = InlineNode::Hyperlink(Hyperlink {
        id: tid(12),
        target: HyperlinkTarget::Internal(InternalTarget {
            anchor: "top".to_owned(),
        }),
        tooltip: None,
        inlines: vec![run_inline(tid(13), "x")],
    });
    let field = Field {
        id: tid(10),
        instruction: " REF a ".to_owned(),
        inlines: vec![inner_link],
    };
    assert!(matches!(
        table_document(vec![field_paragraph(field)]),
        Err(ModelError::NestedHyperlink(_))
    ));
}

#[test]
fn nested_field_inside_a_field_is_rejected() {
    let inner = InlineNode::Field(Field {
        id: tid(12),
        instruction: " PAGE ".to_owned(),
        inlines: Vec::new(),
    });
    let field = Field {
        id: tid(10),
        instruction: " = ".to_owned(),
        inlines: vec![inner],
    };
    assert!(matches!(
        table_document(vec![field_paragraph(field)]),
        Err(ModelError::NestedField(_))
    ));
}

#[test]
fn duplicate_id_inside_a_field_result_is_rejected() {
    let field = Field {
        id: tid(10),
        instruction: " PAGE ".to_owned(),
        inlines: vec![run_inline(tid(10), "7")], // run id collides with field id
    };
    assert!(matches!(
        table_document(vec![field_paragraph(field)]),
        Err(ModelError::DuplicateNodeId(_))
    ));
}
