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
fn dangling_next_style_reference_is_rejected() {
    // A `w:next` (like `w:link`) must resolve; a dangling one fails validation.
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"styles":{
              "0000000000000000000000000000000a":{"kind":"paragraph","next":"00000000000000000000000000000099"}
            }}}"#;
    assert!(matches!(
        Document::from_json(json, SnapshotLimits::default()),
        Err(SnapshotError::InvalidModel(ModelError::DanglingStyleRef(_)))
    ));
}

#[test]
fn table_style_conditional_formatting_snapshot_round_trips() {
    // A table style with style-level borders plus two `w:tblStylePr` regions
    // (a bold, shaded first row and a banded row) is a JSON snapshot fixed point,
    // proving the additive style shape serializes and re-validates cleanly.
    let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"styles":{
              "0000000000000000000000000000000a":{"kind":"table","isDefault":true,"name":"Grid","uiPriority":59,
                "table":{"borders":{"top":{"style":"single","sizeEighthPoints":4}}},
                "conditional":[
                  {"region":"firstRow","run":{"bold":true},
                    "tableCell":{"shading":{"fill":{"r":68,"g":114,"b":196}}}},
                  {"region":"band1Horizontal",
                    "tableCell":{"shading":{"fill":{"r":217,"g":226,"b":243}}}}
                ]}
            }}}"#;
    let document = Document::from_json(json, SnapshotLimits::default()).unwrap();
    let first = document.to_json().unwrap();
    let reloaded = Document::from_json(&first, SnapshotLimits::default()).unwrap();
    assert_eq!(reloaded.to_json().unwrap(), first);

    let (_, style) = document.definitions().styles.iter().next().unwrap();
    assert_eq!(style.kind, StyleKind::Table);
    assert_eq!(style.conditional.len(), 2);
    assert_eq!(style.conditional[0].region, TableStyleRegion::FirstRow);
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
        properties: TableProperties::default(),
        rows: vec![
            TableRow {
                id: tid(11),
                properties: TableRowProperties::default(),
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
                properties: TableRowProperties::default(),
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
fn table_alignment_justify_is_rejected() {
    // `both` (justify) is not a valid `ST_JcTable` value; an authored model must
    // not carry it (the writer would emit an invalid `w:jc`). Start/Center/End
    // remain valid.
    let table = |alignment| {
        BlockNode::Table(Table {
            id: tid(30),
            grid: vec![GridColumn {
                width_twips: Some(2_880),
            }],
            properties: TableProperties {
                alignment: Some(alignment),
                ..TableProperties::default()
            },
            rows: vec![TableRow {
                id: tid(31),
                properties: TableRowProperties::default(),
                cells: vec![cell(
                    tid(32),
                    TableCellProperties::default(),
                    vec![paragraph_block(tid(33))],
                )],
            }],
        })
    };
    assert!(matches!(
        table_document(vec![table(Alignment::Justify)]),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "table.alignment"
        })
    ));
    assert!(table_document(vec![table(Alignment::Center)]).is_ok());
}

#[test]
fn table_properties_round_trip_and_default_omits_the_key() {
    // A cell with a default (empty) properties still serializes to {} and a table
    // with default properties omits the "properties" key entirely — the
    // load-bearing backward-compat guard for the additive table-property fields.
    let styled_cell = cell(
        tid(3),
        TableCellProperties {
            vertical_alignment: Some(CellVerticalAlignment::Center),
            no_wrap: true,
            ..TableCellProperties::default()
        },
        vec![paragraph_block(tid(4))],
    );
    let table = Table {
        id: tid(1),
        grid: Vec::new(),
        properties: TableProperties {
            width_twips: Some(9000),
            layout: Some(TableLayout::Fixed),
            look: TableLook {
                first_row: true,
                ..TableLook::default()
            },
            shading: Shading {
                fill: Some(RgbColor { r: 1, g: 2, b: 3 }),
            },
            ..TableProperties::default()
        },
        rows: vec![TableRow {
            id: tid(2),
            properties: TableRowProperties {
                cant_split: true,
                ..TableRowProperties::default()
            },
            cells: vec![styled_cell],
        }],
    };
    let document = table_document(vec![BlockNode::Table(table)]).unwrap();
    let reloaded =
        Document::from_json(&document.to_json().unwrap(), SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);

    // Backward-compat: default table/row properties are omitted from the JSON.
    let plain = Table {
        id: tid(1),
        grid: Vec::new(),
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: tid(2),
            properties: TableRowProperties::default(),
            cells: vec![cell(
                tid(3),
                TableCellProperties::default(),
                vec![paragraph_block(tid(4))],
            )],
        }],
    };
    // The table and row objects omit their default `properties` key entirely
    // (byte-compat); the cell keeps its always-present `properties: {}`.
    let value: serde_json::Value = serde_json::to_value(&plain).unwrap();
    let table_obj = value.as_object().unwrap();
    assert!(
        !table_obj.contains_key("properties"),
        "default table properties omitted"
    );
    let row_obj = value["rows"][0].as_object().unwrap();
    assert!(
        !row_obj.contains_key("properties"),
        "default row properties omitted"
    );
    let cell_obj = value["rows"][0]["cells"][0].as_object().unwrap();
    assert_eq!(
        cell_obj["properties"],
        serde_json::json!({}),
        "cell properties still serialized as {{}}"
    );
}

#[test]
fn table_borders_and_margins_round_trip_and_reject_bad_style() {
    let borders = TableBorders {
        top: Some(BorderEdge {
            style: "single".to_owned(),
            size_eighth_points: Some(8),
            color: Some(RgbColor { r: 1, g: 2, b: 3 }),
            space_points: Some(4),
        }),
        ..TableBorders::default()
    };
    let table = Table {
        id: tid(1),
        grid: Vec::new(),
        properties: TableProperties {
            borders: borders.clone(),
            cell_margins: CellMargins {
                top_twips: Some(120),
                ..CellMargins::default()
            },
            ..TableProperties::default()
        },
        rows: vec![TableRow {
            id: tid(2),
            properties: TableRowProperties::default(),
            cells: vec![cell(
                tid(3),
                TableCellProperties {
                    borders,
                    ..TableCellProperties::default()
                },
                vec![paragraph_block(tid(4))],
            )],
        }],
    };
    let document = table_document(vec![BlockNode::Table(table)]).unwrap();
    let reloaded =
        Document::from_json(&document.to_json().unwrap(), SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);

    // An oversized border style token is rejected.
    let bad = Table {
        id: tid(1),
        grid: Vec::new(),
        properties: TableProperties {
            borders: TableBorders {
                top: Some(BorderEdge {
                    style: "x".repeat(33),
                    size_eighth_points: None,
                    color: None,
                    space_points: None,
                }),
                ..TableBorders::default()
            },
            ..TableProperties::default()
        },
        rows: vec![TableRow {
            id: tid(2),
            properties: TableRowProperties::default(),
            cells: vec![cell(
                tid(3),
                TableCellProperties::default(),
                vec![paragraph_block(tid(4))],
            )],
        }],
    };
    assert!(matches!(
        table_document(vec![BlockNode::Table(bad)]),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "table.borders"
        })
    ));
}

#[test]
fn over_range_table_width_is_rejected() {
    let table = Table {
        id: tid(1),
        grid: Vec::new(),
        properties: TableProperties {
            width_twips: Some(40_000),
            ..TableProperties::default()
        },
        rows: vec![TableRow {
            id: tid(2),
            properties: TableRowProperties::default(),
            cells: vec![cell(
                tid(3),
                TableCellProperties::default(),
                vec![paragraph_block(tid(4))],
            )],
        }],
    };
    assert!(matches!(
        table_document(vec![BlockNode::Table(table)]),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "table.width"
        })
    ));
}

#[test]
fn empty_table_is_rejected() {
    let table = BlockNode::Table(Table {
        id: tid(10),
        grid: Vec::new(),
        properties: TableProperties::default(),
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
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: tid(11),
            properties: TableRowProperties::default(),
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
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: tid(11),
            properties: TableRowProperties::default(),
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
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: tid(11),
            properties: TableRowProperties::default(),
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
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: tid(11),
            properties: TableRowProperties::default(),
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
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: row_id,
            properties: TableRowProperties::default(),
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
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: tid(11),
            properties: TableRowProperties::default(),
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

fn run_with_props(id: NodeId, properties: RunProperties) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: tid(1),
        properties: ParagraphProperties::default(),
        inlines: vec![InlineNode::Run(Run {
            id,
            properties,
            text: "x".to_owned(),
        })],
    })
}

#[test]
fn run_long_tail_properties_round_trip() {
    let properties = RunProperties {
        all_caps: Some(true),
        small_caps: Some(false),
        hidden: Some(true),
        double_strike: Some(true),
        font_ref_cs: Some(FontRef::Named(FontName {
            name: "Arial".to_owned(),
        })),
        font_ref_east_asia: Some(FontRef::Theme(ThemeFont {
            slot: ThemeFontRef::MinorEastAsia,
        })),
        font_hint: Some(RunFontHint::EastAsia),
        vertical_alignment: Some(VerticalAlignment::Superscript),
        highlight: Some(HighlightColor::Yellow),
        emphasis: Some(EmphasisMark::Dot),
        ..RunProperties::default()
    };
    let document = table_document(vec![run_with_props(tid(2), properties)]).unwrap();
    let reloaded =
        Document::from_json(&document.to_json().unwrap(), SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);
}

#[test]
fn empty_run_long_tail_font_name_is_rejected() {
    let properties = RunProperties {
        font_ref_h_ansi: Some(FontRef::Named(FontName {
            name: String::new(),
        })),
        ..RunProperties::default()
    };
    assert!(matches!(
        table_document(vec![run_with_props(tid(2), properties)]),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "run.font_ref.name"
        })
    ));
}

#[test]
fn default_run_properties_still_serialize_to_empty_object() {
    // Backward-compat guard: the additive long-tail fields must not appear when
    // unset, so a default RunProperties serializes to `{}` as before.
    let json = serde_json::to_string(&RunProperties::default()).unwrap();
    assert_eq!(json, "{}");
}

#[test]
fn run_metrics_and_language_round_trip_and_bound() {
    let properties = RunProperties {
        character_spacing_twips: Some(-40),
        kerning_half_points: Some(28),
        position_half_points: Some(6),
        language: Some(Language {
            value: Some("en-US".to_owned()),
            east_asia: Some("ja-JP".to_owned()),
            bidi: None,
        }),
        ..RunProperties::default()
    };
    let block = BlockNode::Paragraph(Paragraph {
        id: tid(1),
        properties: ParagraphProperties::default(),
        inlines: vec![InlineNode::Run(Run {
            id: tid(2),
            properties,
            text: "x".to_owned(),
        })],
    });
    let document = table_document(vec![block]).unwrap();
    let reloaded =
        Document::from_json(&document.to_json().unwrap(), SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);

    // Out-of-range character spacing is rejected.
    let bad = RunProperties {
        character_spacing_twips: Some(40_000),
        ..RunProperties::default()
    };
    let bad_block = BlockNode::Paragraph(Paragraph {
        id: tid(1),
        properties: ParagraphProperties::default(),
        inlines: vec![InlineNode::Run(Run {
            id: tid(2),
            properties: bad,
            text: "x".to_owned(),
        })],
    });
    assert!(matches!(
        table_document(vec![bad_block]),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "run.character_spacing"
        })
    ));
}

#[test]
fn default_paragraph_properties_still_serialize_to_empty_object() {
    // Same additive guard for the paragraph long-tail fields.
    let json = serde_json::to_string(&ParagraphProperties::default()).unwrap();
    assert_eq!(json, "{}");
}

#[test]
fn paragraph_long_tail_properties_round_trip() {
    let properties = ParagraphProperties {
        keep_next: true,
        page_break_before: true,
        contextual_spacing: true,
        suppress_line_numbers: true,
        outline_level: Some(3),
        ..ParagraphProperties::default()
    };
    let block = BlockNode::Paragraph(Paragraph {
        id: tid(1),
        properties,
        inlines: vec![run_inline(tid(2), "x")],
    });
    let document = table_document(vec![block]).unwrap();
    let reloaded =
        Document::from_json(&document.to_json().unwrap(), SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);
}

#[test]
fn paragraph_borders_shading_tabs_round_trip_and_bound() {
    let properties = ParagraphProperties {
        borders: ParagraphBorders {
            top: Some(BorderEdge {
                style: "single".to_owned(),
                size_eighth_points: Some(8),
                color: None,
                space_points: Some(4),
            }),
            ..ParagraphBorders::default()
        },
        shading: Shading {
            fill: Some(RgbColor { r: 1, g: 2, b: 3 }),
        },
        tabs: vec![TabStop {
            position_twips: 2160,
            alignment: TabAlignment::Center,
            leader: Some(TabLeader::Dot),
        }],
        ..ParagraphProperties::default()
    };
    let block = BlockNode::Paragraph(Paragraph {
        id: tid(1),
        properties,
        inlines: vec![run_inline(tid(2), "x")],
    });
    let document = table_document(vec![block]).unwrap();
    let reloaded =
        Document::from_json(&document.to_json().unwrap(), SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);

    // Too many tab stops is rejected.
    let bad = ParagraphProperties {
        tabs: (0..200)
            .map(|_| TabStop {
                position_twips: 100,
                alignment: TabAlignment::Start,
                leader: None,
            })
            .collect(),
        ..ParagraphProperties::default()
    };
    let bad_block = BlockNode::Paragraph(Paragraph {
        id: tid(1),
        properties: bad,
        inlines: vec![run_inline(tid(2), "x")],
    });
    assert!(matches!(
        table_document(vec![bad_block]),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "paragraph.tabs"
        })
    ));
}

#[test]
fn out_of_range_outline_level_is_rejected() {
    let block = BlockNode::Paragraph(Paragraph {
        id: tid(1),
        properties: ParagraphProperties {
            outline_level: Some(10),
            ..ParagraphProperties::default()
        },
        inlines: vec![run_inline(tid(2), "x")],
    });
    assert!(matches!(
        table_document(vec![block]),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "paragraph.outline_level"
        })
    ));
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

// ---- schema v1 text boxes ------------------------------------------------

fn textbox_paragraph(para_id: NodeId, text_box: TextBox) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: para_id,
        properties: ParagraphProperties::default(),
        inlines: vec![InlineNode::TextBox(text_box)],
    })
}

#[test]
fn text_box_with_block_content_validates_and_round_trips_json() {
    let text_box = TextBox {
        id: tid(10),
        blocks: vec![paragraph_block(tid(11))],
    };
    let document = table_document(vec![textbox_paragraph(tid(1), text_box)]).unwrap();
    let json = document.to_json().unwrap();
    let reloaded = Document::from_json(&json, SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);

    let BlockNode::Paragraph(paragraph) = &document.body()[0] else {
        panic!("expected a paragraph");
    };
    let InlineNode::TextBox(text_box) = &paragraph.inlines[0] else {
        panic!("expected a text box");
    };
    assert_eq!(text_box.blocks.len(), 1);
}

#[test]
fn empty_text_box_is_rejected() {
    let text_box = TextBox {
        id: tid(10),
        blocks: Vec::new(),
    };
    assert!(matches!(
        table_document(vec![textbox_paragraph(tid(1), text_box)]),
        Err(ModelError::EmptyTextBox(_))
    ));
}

#[test]
fn duplicate_id_inside_a_text_box_is_rejected() {
    let text_box = TextBox {
        id: tid(10),
        blocks: vec![paragraph_block(tid(10))], // inner paragraph id collides
    };
    assert!(matches!(
        table_document(vec![textbox_paragraph(tid(1), text_box)]),
        Err(ModelError::DuplicateNodeId(_))
    ));
}

fn wrap_in_textboxes(depth: u32, counter: &mut u64) -> BlockNode {
    if depth == 0 {
        *counter += 1;
        return paragraph_block(tid(*counter));
    }
    let inner = wrap_in_textboxes(depth - 1, counter);
    *counter += 1;
    let box_id = tid(*counter);
    *counter += 1;
    let para_id = tid(*counter);
    BlockNode::Paragraph(Paragraph {
        id: para_id,
        properties: ParagraphProperties::default(),
        inlines: vec![InlineNode::TextBox(TextBox {
            id: box_id,
            blocks: vec![inner],
        })],
    })
}

#[test]
fn text_box_nesting_within_bound_validates() {
    let mut counter = 0;
    let block = wrap_in_textboxes(MAX_TEXTBOX_DEPTH, &mut counter);
    assert!(table_document(vec![block]).is_ok());
}

#[test]
fn text_box_nesting_beyond_bound_is_rejected() {
    let mut counter = 0;
    let block = wrap_in_textboxes(MAX_TEXTBOX_DEPTH + 1, &mut counter);
    assert!(matches!(
        table_document(vec![block]),
        Err(ModelError::TextBoxNestingTooDeep(_))
    ));
}

// ---- schema v1 footnotes / endnotes --------------------------------------

fn note_reference(id: NodeId, kind: NoteKind, note: NoteId) -> InlineNode {
    InlineNode::NoteReference(NoteReference { id, kind, note })
}

fn document_with_footnote(
    reference_kind: NoteKind,
    ref_note: NoteId,
    footnote_id: NoteId,
) -> Result<Document, ModelError> {
    let mut definitions = Definitions::default();
    definitions.footnotes.insert(
        footnote_id,
        Note {
            blocks: vec![paragraph_block(tid(30))],
        },
    );
    let body = vec![BlockNode::Paragraph(Paragraph {
        id: tid(1),
        properties: ParagraphProperties::default(),
        inlines: vec![note_reference(tid(2), reference_kind, ref_note)],
    })];
    Document::new(NoteId::new(tid(99)).node_id(), body, definitions)
}

#[test]
fn footnote_reference_resolves_and_round_trips() {
    let note = NoteId::new(tid(20));
    let document = document_with_footnote(NoteKind::Footnote, note, note).unwrap();
    let json = document.to_json().unwrap();
    let reloaded = Document::from_json(&json, SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);
    assert_eq!(document.definitions().footnotes.len(), 1);
}

#[test]
fn dangling_note_reference_is_rejected() {
    let defined = NoteId::new(tid(20));
    let missing = NoteId::new(tid(21));
    assert!(matches!(
        document_with_footnote(NoteKind::Footnote, missing, defined),
        Err(ModelError::DanglingNoteRef(_))
    ));
}

#[test]
fn footnote_reference_does_not_resolve_against_endnotes() {
    // A footnote-kind reference must resolve in `footnotes`, not `endnotes`.
    let note = NoteId::new(tid(20));
    let mut definitions = Definitions::default();
    definitions.endnotes.insert(
        note,
        Note {
            blocks: vec![paragraph_block(tid(30))],
        },
    );
    let body = vec![BlockNode::Paragraph(Paragraph {
        id: tid(1),
        properties: ParagraphProperties::default(),
        inlines: vec![note_reference(tid(2), NoteKind::Footnote, note)],
    })];
    assert!(matches!(
        Document::new(tid(99), body, definitions),
        Err(ModelError::DanglingNoteRef(_))
    ));
}

#[test]
fn duplicate_id_inside_a_note_is_rejected() {
    let note = NoteId::new(tid(20));
    let mut definitions = Definitions::default();
    definitions.footnotes.insert(
        note,
        Note {
            blocks: vec![paragraph_block(tid(1))], // collides with body paragraph id
        },
    );
    let body = vec![BlockNode::Paragraph(Paragraph {
        id: tid(1),
        properties: ParagraphProperties::default(),
        inlines: vec![note_reference(tid(2), NoteKind::Footnote, note)],
    })];
    assert!(matches!(
        Document::new(tid(99), body, definitions),
        Err(ModelError::DuplicateNodeId(_))
    ));
}

// ---- schema v1 headers / footers -----------------------------------------

fn valid_section(
    id: NodeId,
    headers: Vec<HeaderFooterRef>,
    footers: Vec<HeaderFooterRef>,
) -> SectionBoundary {
    SectionBoundary {
        id: SectionId::new(id),
        page_size: PageSize {
            width_twips: 12_240,
            height_twips: 15_840,
        },
        page_margins: PageMargins {
            top_twips: 1_440,
            bottom_twips: 1_440,
            start_twips: 1_440,
            end_twips: 1_440,
        },
        columns: SectionColumns {
            count: 1,
            space_twips: None,
            separator: None,
        },
        headers,
        footers,
        section_type: None,
        title_page: None,
        vertical_alignment: None,
        page_numbering: PageNumbering::default(),
        doc_grid: DocGrid::default(),
    }
}

#[test]
fn header_reference_resolves_and_round_trips() {
    let header = HeaderFooterId::new(tid(20));
    let mut definitions = Definitions::default();
    definitions.headers.insert(
        header,
        HeaderFooter {
            blocks: vec![paragraph_block(tid(30))],
        },
    );
    definitions.sections.push(valid_section(
        tid(40),
        vec![HeaderFooterRef {
            kind: HeaderFooterKind::Default,
            reference: header,
        }],
        Vec::new(),
    ));
    let body = vec![paragraph_block(tid(1))];
    let document = Document::new(tid(99), body, definitions).unwrap();
    let json = document.to_json().unwrap();
    let reloaded = Document::from_json(&json, SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);
    assert_eq!(document.definitions().headers.len(), 1);
}

#[test]
fn dangling_header_reference_is_rejected() {
    let missing = HeaderFooterId::new(tid(21));
    let mut definitions = Definitions::default();
    definitions.sections.push(valid_section(
        tid(40),
        vec![HeaderFooterRef {
            kind: HeaderFooterKind::Default,
            reference: missing,
        }],
        Vec::new(),
    ));
    assert!(matches!(
        Document::new(tid(99), vec![paragraph_block(tid(1))], definitions),
        Err(ModelError::DanglingHeaderFooterRef(_))
    ));
}

#[test]
fn header_reference_does_not_resolve_against_footers() {
    // A header reference must resolve in `headers`, not `footers`.
    let id = HeaderFooterId::new(tid(20));
    let mut definitions = Definitions::default();
    definitions.footers.insert(
        id,
        HeaderFooter {
            blocks: vec![paragraph_block(tid(30))],
        },
    );
    definitions.sections.push(valid_section(
        tid(40),
        vec![HeaderFooterRef {
            kind: HeaderFooterKind::Default,
            reference: id,
        }],
        Vec::new(),
    ));
    assert!(matches!(
        Document::new(tid(99), vec![paragraph_block(tid(1))], definitions),
        Err(ModelError::DanglingHeaderFooterRef(_))
    ));
}

// ---- schema v1 comments --------------------------------------------------

fn comment_paragraph(reference: NodeId, comment: CommentId) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: tid(1),
        properties: ParagraphProperties::default(),
        inlines: vec![InlineNode::CommentReference(CommentReference {
            id: reference,
            comment,
        })],
    })
}

#[test]
fn comment_reference_resolves_and_round_trips() {
    let id = CommentId::new(tid(20));
    let mut definitions = Definitions::default();
    definitions.comments.insert(
        id,
        Comment {
            blocks: vec![paragraph_block(tid(30))],
            author: Some("Alice".to_owned()),
            initials: Some("AL".to_owned()),
            date: Some("2026-07-25T00:00:00Z".to_owned()),
        },
    );
    let document =
        Document::new(tid(99), vec![comment_paragraph(tid(2), id)], definitions).unwrap();
    let json = document.to_json().unwrap();
    let reloaded = Document::from_json(&json, SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);
    let comment = document.definitions().comments.get(&id).unwrap();
    assert_eq!(comment.author.as_deref(), Some("Alice"));
}

#[test]
fn dangling_comment_reference_is_rejected() {
    let defined = CommentId::new(tid(20));
    let missing = CommentId::new(tid(21));
    let mut definitions = Definitions::default();
    definitions.comments.insert(
        defined,
        Comment {
            blocks: vec![paragraph_block(tid(30))],
            ..Comment::default()
        },
    );
    assert!(matches!(
        Document::new(
            tid(99),
            vec![comment_paragraph(tid(2), missing)],
            definitions
        ),
        Err(ModelError::DanglingCommentRef(_))
    ));
}

#[test]
fn empty_comment_author_is_rejected() {
    let id = CommentId::new(tid(20));
    let mut definitions = Definitions::default();
    definitions.comments.insert(
        id,
        Comment {
            blocks: Vec::new(),
            author: Some(String::new()),
            ..Comment::default()
        },
    );
    assert!(matches!(
        Document::new(tid(99), vec![comment_paragraph(tid(2), id)], definitions),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "comment.metadata"
        })
    ));
}

// ---- tracked changes (revisions) -----------------------------------------

fn revision_paragraph(paragraph_id: NodeId, inline: InlineNode) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: paragraph_id,
        properties: ParagraphProperties::default(),
        inlines: vec![inline],
    })
}

/// Builds `depth` nested revision wrappers around a leaf run, each with a unique
/// id drawn from `counter`.
fn wrap_in_revisions(depth: u32, counter: &mut u64) -> InlineNode {
    *counter += 1;
    let id = tid(*counter);
    if depth == 0 {
        return run_inline(id, "leaf");
    }
    let inner = wrap_in_revisions(depth - 1, counter);
    InlineNode::Revision(Revision {
        id,
        kind: RevisionKind::Insertion,
        author: None,
        date: None,
        revision_id: None,
        inlines: vec![inner],
    })
}

#[test]
fn insertion_revision_round_trips_with_metadata() {
    let revision = InlineNode::Revision(Revision {
        id: tid(10),
        kind: RevisionKind::Insertion,
        author: Some("Bob".to_owned()),
        date: Some("2026-07-25T00:00:00Z".to_owned()),
        revision_id: Some("42".to_owned()),
        inlines: vec![run_inline(tid(11), "added")],
    });
    let document = table_document(vec![revision_paragraph(tid(1), revision)]).unwrap();
    let reloaded =
        Document::from_json(&document.to_json().unwrap(), SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);
}

#[test]
fn deletion_revision_preserves_deleted_text() {
    let revision = InlineNode::Revision(Revision {
        id: tid(10),
        kind: RevisionKind::Deletion,
        author: None,
        date: None,
        revision_id: None,
        inlines: vec![run_inline(tid(11), "gone")],
    });
    let document = table_document(vec![revision_paragraph(tid(1), revision)]).unwrap();
    let reloaded =
        Document::from_json(&document.to_json().unwrap(), SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);
    let BlockNode::Paragraph(paragraph) = &document.body()[0] else {
        panic!("expected a paragraph");
    };
    let InlineNode::Revision(revision) = &paragraph.inlines[0] else {
        panic!("expected a revision");
    };
    assert_eq!(revision.kind, RevisionKind::Deletion);
    let InlineNode::Run(run) = &revision.inlines[0] else {
        panic!("expected a run");
    };
    assert_eq!(run.text, "gone");
}

#[test]
fn empty_revision_is_rejected() {
    let revision = InlineNode::Revision(Revision {
        id: tid(10),
        kind: RevisionKind::Insertion,
        author: None,
        date: None,
        revision_id: None,
        inlines: Vec::new(),
    });
    assert!(matches!(
        table_document(vec![revision_paragraph(tid(1), revision)]),
        Err(ModelError::EmptyRevision(_))
    ));
}

#[test]
fn nested_insertion_around_deletion_is_accepted() {
    let inner = InlineNode::Revision(Revision {
        id: tid(12),
        kind: RevisionKind::Deletion,
        author: None,
        date: None,
        revision_id: None,
        inlines: vec![run_inline(tid(13), "x")],
    });
    let outer = InlineNode::Revision(Revision {
        id: tid(10),
        kind: RevisionKind::Insertion,
        author: None,
        date: None,
        revision_id: None,
        inlines: vec![inner],
    });
    assert!(table_document(vec![revision_paragraph(tid(1), outer)]).is_ok());
}

#[test]
fn revision_nesting_at_bound_is_accepted() {
    let mut counter = 100;
    let nested = wrap_in_revisions(MAX_REVISION_DEPTH, &mut counter);
    assert!(table_document(vec![revision_paragraph(tid(1), nested)]).is_ok());
}

#[test]
fn revision_nesting_beyond_bound_is_rejected() {
    let mut counter = 100;
    let nested = wrap_in_revisions(MAX_REVISION_DEPTH + 1, &mut counter);
    assert!(matches!(
        table_document(vec![revision_paragraph(tid(1), nested)]),
        Err(ModelError::RevisionNestingTooDeep(_))
    ));
}

#[test]
fn oversized_revision_date_is_rejected() {
    let revision = InlineNode::Revision(Revision {
        id: tid(10),
        kind: RevisionKind::Insertion,
        author: None,
        date: Some("0".repeat(65)),
        revision_id: None,
        inlines: vec![run_inline(tid(11), "t")],
    });
    assert!(matches!(
        table_document(vec![revision_paragraph(tid(1), revision)]),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "revision.date"
        })
    ));
}

#[test]
fn oversized_revision_id_is_rejected() {
    // The producer `w:id` grouping key is bounded at 64 bytes (the importer's
    // capture filter and the design contract), separate from author's 255.
    let revision = InlineNode::Revision(Revision {
        id: tid(10),
        kind: RevisionKind::Insertion,
        author: None,
        date: None,
        revision_id: Some("9".repeat(65)),
        inlines: vec![run_inline(tid(11), "t")],
    });
    assert!(matches!(
        table_document(vec![revision_paragraph(tid(1), revision)]),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "revision.date"
        })
    ));
}

#[test]
fn long_revision_author_within_bound_is_accepted() {
    // Author keeps its 255-byte bound (wider than id/date), so a 200-byte author
    // is valid.
    let revision = InlineNode::Revision(Revision {
        id: tid(10),
        kind: RevisionKind::Insertion,
        author: Some("a".repeat(200)),
        date: None,
        revision_id: None,
        inlines: vec![run_inline(tid(11), "t")],
    });
    assert!(table_document(vec![revision_paragraph(tid(1), revision)]).is_ok());
}

#[test]
fn revision_may_wrap_a_hyperlink_at_top_level() {
    // A revision is transparent to the wrapper leaf-only rule, so it may wrap a
    // hyperlink (an inserted link) even though a hyperlink cannot nest in a
    // hyperlink/field.
    let link = InlineNode::Hyperlink(Hyperlink {
        id: tid(12),
        target: HyperlinkTarget::Internal(InternalTarget {
            anchor: "a".to_owned(),
        }),
        tooltip: None,
        inlines: vec![run_inline(tid(13), "link")],
    });
    let revision = InlineNode::Revision(Revision {
        id: tid(10),
        kind: RevisionKind::Insertion,
        author: None,
        date: None,
        revision_id: None,
        inlines: vec![link],
    });
    assert!(table_document(vec![revision_paragraph(tid(1), revision)]).is_ok());
}

#[test]
fn revision_child_id_duplicating_the_wrapper_is_rejected() {
    let revision = InlineNode::Revision(Revision {
        id: tid(10),
        kind: RevisionKind::Insertion,
        author: None,
        date: None,
        revision_id: None,
        inlines: vec![run_inline(tid(10), "dup")],
    });
    assert!(matches!(
        table_document(vec![revision_paragraph(tid(1), revision)]),
        Err(ModelError::DuplicateNodeId(_))
    ));
}

// ---- schema v1 bookmarks -------------------------------------------------

fn bookmark_id(counter: u64) -> BookmarkId {
    BookmarkId::new(tid(counter))
}

fn bookmark_paragraph(paragraph_id: NodeId, inlines: Vec<InlineNode>) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: paragraph_id,
        properties: ParagraphProperties::default(),
        inlines,
    })
}

#[test]
fn bookmark_pair_resolves_and_round_trips() {
    let bm = bookmark_id(20);
    let mut definitions = Definitions::default();
    definitions.bookmarks.insert(
        bm,
        Bookmark {
            name: "_Toc1".to_owned(),
        },
    );
    let paragraph = bookmark_paragraph(
        tid(1),
        vec![
            InlineNode::BookmarkStart(BookmarkStart {
                id: tid(2),
                bookmark: bm,
            }),
            run_inline(tid(3), "anchored"),
            InlineNode::BookmarkEnd(BookmarkEnd {
                id: tid(4),
                bookmark: bm,
            }),
        ],
    );
    let document = Document::new(tid(99), vec![paragraph], definitions).unwrap();
    let json = document.to_json().unwrap();
    let reloaded = Document::from_json(&json, SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);
    assert_eq!(
        document.definitions().bookmarks.get(&bm).unwrap().name,
        "_Toc1"
    );
}

#[test]
fn empty_bookmarks_are_omitted_from_serialization() {
    let document = Document::new(
        tid(99),
        vec![paragraph_block(tid(1))],
        Definitions::default(),
    )
    .unwrap();
    let json = String::from_utf8(document.to_json().unwrap()).unwrap();
    assert!(!json.contains("bookmarks"));
}

#[test]
fn dangling_bookmark_reference_is_rejected() {
    let defined = bookmark_id(20);
    let missing = bookmark_id(21);
    let mut definitions = Definitions::default();
    definitions.bookmarks.insert(
        defined,
        Bookmark {
            name: "x".to_owned(),
        },
    );
    let paragraph = bookmark_paragraph(
        tid(1),
        vec![InlineNode::BookmarkStart(BookmarkStart {
            id: tid(2),
            bookmark: missing,
        })],
    );
    assert!(matches!(
        Document::new(tid(99), vec![paragraph], definitions),
        Err(ModelError::DanglingBookmarkRef(_))
    ));
}

#[test]
fn empty_bookmark_name_is_rejected() {
    let bm = bookmark_id(20);
    let mut definitions = Definitions::default();
    definitions.bookmarks.insert(
        bm,
        Bookmark {
            name: String::new(),
        },
    );
    let paragraph = bookmark_paragraph(
        tid(1),
        vec![InlineNode::BookmarkStart(BookmarkStart {
            id: tid(2),
            bookmark: bm,
        })],
    );
    assert!(matches!(
        Document::new(tid(99), vec![paragraph], definitions),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "bookmark.name"
        })
    ));
}

#[test]
fn oversized_bookmark_name_is_rejected() {
    let bm = bookmark_id(20);
    let mut definitions = Definitions::default();
    definitions.bookmarks.insert(
        bm,
        Bookmark {
            name: "a".repeat(256),
        },
    );
    let paragraph = bookmark_paragraph(
        tid(1),
        vec![InlineNode::BookmarkStart(BookmarkStart {
            id: tid(2),
            bookmark: bm,
        })],
    );
    assert!(matches!(
        Document::new(tid(99), vec![paragraph], definitions),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "bookmark.name"
        })
    ));
}

#[test]
fn bookmark_definition_id_colliding_with_a_node_is_rejected() {
    // The bookmark definition id equals the start marker's own node id.
    let bm = BookmarkId::new(tid(2));
    let mut definitions = Definitions::default();
    definitions.bookmarks.insert(
        bm,
        Bookmark {
            name: "x".to_owned(),
        },
    );
    let paragraph = bookmark_paragraph(
        tid(1),
        vec![InlineNode::BookmarkStart(BookmarkStart {
            id: tid(2),
            bookmark: bm,
        })],
    );
    assert!(matches!(
        Document::new(tid(99), vec![paragraph], definitions),
        Err(ModelError::DuplicateNodeId(_))
    ));
}

#[test]
fn bookmark_marker_separates_equivalent_runs() {
    // Two equal-property runs split by a marker must NOT be merge-flagged.
    let bm = bookmark_id(20);
    let mut definitions = Definitions::default();
    definitions.bookmarks.insert(
        bm,
        Bookmark {
            name: "x".to_owned(),
        },
    );
    let paragraph = bookmark_paragraph(
        tid(1),
        vec![
            run_inline(tid(2), "a"),
            InlineNode::BookmarkStart(BookmarkStart {
                id: tid(3),
                bookmark: bm,
            }),
            run_inline(tid(4), "b"),
        ],
    );
    assert!(Document::new(tid(99), vec![paragraph], definitions).is_ok());
}

#[test]
fn lone_bookmark_start_validates() {
    // Lax pairing: a start with no matching end is allowed.
    let bm = bookmark_id(20);
    let mut definitions = Definitions::default();
    definitions.bookmarks.insert(
        bm,
        Bookmark {
            name: "x".to_owned(),
        },
    );
    let paragraph = bookmark_paragraph(
        tid(1),
        vec![
            InlineNode::BookmarkStart(BookmarkStart {
                id: tid(2),
                bookmark: bm,
            }),
            run_inline(tid(3), "text"),
        ],
    );
    assert!(Document::new(tid(99), vec![paragraph], definitions).is_ok());
}

#[test]
fn bookmark_marker_inside_a_hyperlink_validates() {
    let bm = bookmark_id(20);
    let mut definitions = Definitions::default();
    definitions.bookmarks.insert(
        bm,
        Bookmark {
            name: "x".to_owned(),
        },
    );
    let link = InlineNode::Hyperlink(Hyperlink {
        id: tid(2),
        target: HyperlinkTarget::Internal(InternalTarget {
            anchor: "x".to_owned(),
        }),
        tooltip: None,
        inlines: vec![
            InlineNode::BookmarkStart(BookmarkStart {
                id: tid(3),
                bookmark: bm,
            }),
            run_inline(tid(4), "t"),
            InlineNode::BookmarkEnd(BookmarkEnd {
                id: tid(5),
                bookmark: bm,
            }),
        ],
    });
    let paragraph = bookmark_paragraph(tid(1), vec![link]);
    assert!(Document::new(tid(99), vec![paragraph], definitions).is_ok());
}
// ---- content controls (structured document tags) -------------------------

fn full_sdt_props() -> SdtProperties {
    SdtProperties {
        control_kind: Some(SdtControlKind::RichText),
        alias: Some("Full name".to_owned()),
        tag: Some("fullName".to_owned()),
        control_id: Some("1553275".to_owned()),
    }
}

fn block_sdt(id: NodeId, properties: SdtProperties, blocks: Vec<BlockNode>) -> BlockNode {
    BlockNode::Sdt(BlockSdt {
        id,
        properties,
        blocks,
    })
}

fn inline_sdt_paragraph(paragraph_id: NodeId, inline: InlineNode) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: paragraph_id,
        properties: ParagraphProperties::default(),
        inlines: vec![inline],
    })
}

#[test]
fn block_content_control_validates_and_round_trips_json() {
    let sdt = block_sdt(tid(10), full_sdt_props(), vec![paragraph_block(tid(11))]);
    let document = table_document(vec![sdt]).unwrap();
    let reloaded =
        Document::from_json(&document.to_json().unwrap(), SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);
    let BlockNode::Sdt(sdt) = &document.body()[0] else {
        panic!("expected a block sdt");
    };
    assert_eq!(sdt.properties.control_kind, Some(SdtControlKind::RichText));
    assert_eq!(sdt.properties.alias.as_deref(), Some("Full name"));
    assert_eq!(sdt.properties.tag.as_deref(), Some("fullName"));
    assert_eq!(sdt.properties.control_id.as_deref(), Some("1553275"));
    assert_eq!(sdt.blocks.len(), 1);
}

#[test]
fn inline_content_control_validates_and_round_trips_json() {
    let inline = InlineNode::Sdt(InlineSdt {
        id: tid(10),
        properties: full_sdt_props(),
        inlines: vec![run_inline(tid(11), "typed")],
    });
    let document = table_document(vec![inline_sdt_paragraph(tid(1), inline)]).unwrap();
    let reloaded =
        Document::from_json(&document.to_json().unwrap(), SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);
    let BlockNode::Paragraph(paragraph) = &document.body()[0] else {
        panic!("expected a paragraph");
    };
    let InlineNode::Sdt(sdt) = &paragraph.inlines[0] else {
        panic!("expected an inline sdt");
    };
    let InlineNode::Run(run) = &sdt.inlines[0] else {
        panic!("expected a run");
    };
    assert_eq!(run.text, "typed");
}

#[test]
fn every_sdt_control_kind_round_trips() {
    for kind in [
        SdtControlKind::RichText,
        SdtControlKind::PlainText,
        SdtControlKind::ComboBox,
        SdtControlKind::DropDownList,
        SdtControlKind::Date,
        SdtControlKind::Picture,
        SdtControlKind::Checkbox,
        SdtControlKind::Group,
        SdtControlKind::BuildingBlockGallery,
        SdtControlKind::RepeatingSection,
        SdtControlKind::Citation,
        SdtControlKind::Bibliography,
    ] {
        let properties = SdtProperties {
            control_kind: Some(kind),
            ..SdtProperties::default()
        };
        let sdt = block_sdt(tid(10), properties, vec![paragraph_block(tid(11))]);
        let document = table_document(vec![sdt]).unwrap();
        let reloaded =
            Document::from_json(&document.to_json().unwrap(), SnapshotLimits::default()).unwrap();
        assert_eq!(document, reloaded);
    }
}

#[test]
fn empty_sdt_properties_serialize_to_empty_object() {
    let sdt = block_sdt(
        tid(10),
        SdtProperties::default(),
        vec![paragraph_block(tid(11))],
    );
    let json = String::from_utf8(table_document(vec![sdt]).unwrap().to_json().unwrap()).unwrap();
    assert!(json.contains(r#""type":"sdt""#));
    assert!(json.contains(r#""properties":{},"blocks":"#));
}

#[test]
fn empty_block_content_control_is_rejected() {
    let sdt = block_sdt(tid(10), SdtProperties::default(), Vec::new());
    assert!(matches!(
        table_document(vec![sdt]),
        Err(ModelError::EmptySdt(_))
    ));
}

#[test]
fn empty_inline_content_control_is_rejected() {
    let inline = InlineNode::Sdt(InlineSdt {
        id: tid(10),
        properties: SdtProperties::default(),
        inlines: Vec::new(),
    });
    assert!(matches!(
        table_document(vec![inline_sdt_paragraph(tid(1), inline)]),
        Err(ModelError::EmptySdt(_))
    ));
}

fn wrap_in_block_sdts(depth: u32, counter: &mut u64) -> BlockNode {
    if depth == 0 {
        *counter += 1;
        return paragraph_block(tid(*counter));
    }
    let inner = wrap_in_block_sdts(depth - 1, counter);
    *counter += 1;
    block_sdt(tid(*counter), SdtProperties::default(), vec![inner])
}

fn wrap_in_inline_sdts(depth: u32, counter: &mut u64) -> InlineNode {
    *counter += 1;
    let id = tid(*counter);
    if depth == 0 {
        return run_inline(id, "leaf");
    }
    let inner = wrap_in_inline_sdts(depth - 1, counter);
    InlineNode::Sdt(InlineSdt {
        id,
        properties: SdtProperties::default(),
        inlines: vec![inner],
    })
}

#[test]
fn block_content_control_nesting_within_bound_validates() {
    let mut counter = 0;
    let block = wrap_in_block_sdts(MAX_SDT_DEPTH, &mut counter);
    assert!(table_document(vec![block]).is_ok());
}

#[test]
fn block_content_control_nesting_beyond_bound_is_rejected() {
    let mut counter = 0;
    let block = wrap_in_block_sdts(MAX_SDT_DEPTH + 1, &mut counter);
    assert!(matches!(
        table_document(vec![block]),
        Err(ModelError::SdtNestingTooDeep(_))
    ));
}

#[test]
fn inline_content_control_nesting_within_bound_validates() {
    let mut counter = 100;
    let nested = wrap_in_inline_sdts(MAX_SDT_DEPTH, &mut counter);
    assert!(table_document(vec![inline_sdt_paragraph(tid(1), nested)]).is_ok());
}

#[test]
fn inline_content_control_nesting_beyond_bound_is_rejected() {
    let mut counter = 100;
    let nested = wrap_in_inline_sdts(MAX_SDT_DEPTH + 1, &mut counter);
    assert!(matches!(
        table_document(vec![inline_sdt_paragraph(tid(1), nested)]),
        Err(ModelError::SdtNestingTooDeep(_))
    ));
}

#[test]
fn oversized_sdt_alias_is_rejected() {
    let properties = SdtProperties {
        alias: Some("a".repeat(256)),
        ..SdtProperties::default()
    };
    let sdt = block_sdt(tid(10), properties, vec![paragraph_block(tid(11))]);
    assert!(matches!(
        table_document(vec![sdt]),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "sdt.alias"
        })
    ));
}

#[test]
fn oversized_sdt_tag_is_rejected() {
    let properties = SdtProperties {
        tag: Some("t".repeat(256)),
        ..SdtProperties::default()
    };
    let sdt = block_sdt(tid(10), properties, vec![paragraph_block(tid(11))]);
    assert!(matches!(
        table_document(vec![sdt]),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "sdt.tag"
        })
    ));
}

#[test]
fn oversized_sdt_control_id_is_rejected() {
    let properties = SdtProperties {
        control_id: Some("9".repeat(65)),
        ..SdtProperties::default()
    };
    let sdt = block_sdt(tid(10), properties, vec![paragraph_block(tid(11))]);
    assert!(matches!(
        table_document(vec![sdt]),
        Err(ModelError::PropertyValueOutOfDomain { property: "sdt.id" })
    ));
}

#[test]
fn duplicate_id_inside_a_content_control_is_rejected() {
    let sdt = block_sdt(
        tid(10),
        SdtProperties::default(),
        vec![paragraph_block(tid(10))], // inner paragraph id collides with the sdt
    );
    assert!(matches!(
        table_document(vec![sdt]),
        Err(ModelError::DuplicateNodeId(_))
    ));
}

#[test]
fn inline_content_control_composes_with_a_hyperlink_either_way() {
    // A content control is transparent to the wrapper leaf-only rule, so it may
    // wrap a hyperlink AND may itself sit inside one.
    let link = InlineNode::Hyperlink(Hyperlink {
        id: tid(12),
        target: HyperlinkTarget::Internal(InternalTarget {
            anchor: "a".to_owned(),
        }),
        tooltip: None,
        inlines: vec![run_inline(tid(13), "link")],
    });
    let sdt_over_link = InlineNode::Sdt(InlineSdt {
        id: tid(10),
        properties: SdtProperties::default(),
        inlines: vec![link],
    });
    assert!(table_document(vec![inline_sdt_paragraph(tid(1), sdt_over_link)]).is_ok());

    let inner_sdt = InlineNode::Sdt(InlineSdt {
        id: tid(22),
        properties: SdtProperties::default(),
        inlines: vec![run_inline(tid(23), "x")],
    });
    let link_over_sdt = InlineNode::Hyperlink(Hyperlink {
        id: tid(20),
        target: HyperlinkTarget::Internal(InternalTarget {
            anchor: "b".to_owned(),
        }),
        tooltip: None,
        inlines: vec![inner_sdt],
    });
    assert!(table_document(vec![inline_sdt_paragraph(tid(2), link_over_sdt)]).is_ok());
}

fn nested_table(levels: u32, counter: &mut u64) -> BlockNode {
    *counter += 1;
    let table_id = tid(*counter);
    *counter += 1;
    let row_id = tid(*counter);
    *counter += 1;
    let cell_id = tid(*counter);
    let inner = if levels <= 1 {
        *counter += 1;
        paragraph_block(tid(*counter))
    } else {
        nested_table(levels - 1, counter)
    };
    BlockNode::Table(Table {
        id: table_id,
        grid: Vec::new(),
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: row_id,
            properties: TableRowProperties::default(),
            cells: vec![cell(cell_id, TableCellProperties::default(), vec![inner])],
        }],
    })
}

#[test]
fn deep_table_inside_a_block_content_control_validates() {
    // Regression (review-fix 1): a block sdt restarts the table-depth budget
    // (matching the importer's fresh table stack), so a full-depth table tower
    // INSIDE a control that itself sits in a table cell does not sum past
    // MAX_TABLE_DEPTH and reject the whole document.
    let mut counter = 100;
    let tower = nested_table(MAX_TABLE_DEPTH, &mut counter);
    let sdt = block_sdt(tid(1), SdtProperties::default(), vec![tower]);
    let outer = BlockNode::Table(Table {
        id: tid(2),
        grid: Vec::new(),
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: tid(3),
            properties: TableRowProperties::default(),
            cells: vec![cell(tid(4), TableCellProperties::default(), vec![sdt])],
        }],
    });
    assert!(table_document(vec![outer]).is_ok());
}

// ---- schema v1 document properties (docProps) ----------------------------

fn rich_properties() -> DocumentProperties {
    DocumentProperties {
        core: CoreProperties {
            title: Some("Quarterly Report".to_owned()),
            creator: Some("Ada Lovelace".to_owned()),
            keywords: Some("finance, q3".to_owned()),
            revision: Some("4".to_owned()),
            created: Some("2026-01-02T03:04:05Z".to_owned()),
            modified: Some("2026-07-25T10:11:12Z".to_owned()),
            language: Some("en-US".to_owned()),
            ..CoreProperties::default()
        },
        app: AppProperties {
            application: Some("OpenDoc".to_owned()),
            app_version: Some("16.0000".to_owned()),
            company: Some("Analytical Engines".to_owned()),
            total_time: Some(42),
            pages: Some(3),
            words: Some(1200),
            scale_crop: Some(false),
            links_up_to_date: Some(true),
            titles_of_parts: vec!["Report".to_owned(), "Appendix".to_owned()],
            heading_pairs: vec![HeadingPair {
                name: "Title".to_owned(),
                count: 2,
            }],
            ..AppProperties::default()
        },
        custom: vec![
            CustomProperty {
                name: "Editor".to_owned(),
                value: CustomValue::Text {
                    value: "Grace".to_owned(),
                },
            },
            CustomProperty {
                name: "Approved".to_owned(),
                value: CustomValue::Bool { value: true },
            },
            CustomProperty {
                name: "Rank".to_owned(),
                value: CustomValue::I4 { value: 7 },
            },
            CustomProperty {
                name: "Ratio".to_owned(),
                value: CustomValue::R8 {
                    value: "3.14".to_owned(),
                },
            },
        ],
    }
}

#[test]
fn document_properties_round_trip_json() {
    let document = table_document(vec![paragraph_block(tid(1))])
        .unwrap()
        .with_properties(rich_properties())
        .unwrap();
    let json = document.to_json().unwrap();
    let reloaded = Document::from_json(&json, SnapshotLimits::default()).unwrap();
    assert_eq!(document, reloaded);
    assert_eq!(reloaded.properties(), Some(&rich_properties()));
    // The reload is a byte-exact fixed point.
    assert_eq!(reloaded.to_json().unwrap(), json);
}

#[test]
fn empty_document_properties_are_dropped_and_omitted() {
    // Attaching all-empty metadata is equivalent to attaching none: the
    // accessor returns None and no `properties` key is serialized (backward
    // compat with pre-metadata snapshots).
    let document = table_document(vec![paragraph_block(tid(1))])
        .unwrap()
        .with_properties(DocumentProperties::default())
        .unwrap();
    assert_eq!(document.properties(), None);
    let value: serde_json::Value = serde_json::from_slice(&document.to_json().unwrap()).unwrap();
    assert!(
        !value.as_object().unwrap().contains_key("properties"),
        "empty metadata omitted from the document envelope"
    );
}

#[test]
fn over_long_custom_property_name_is_rejected() {
    let properties = DocumentProperties {
        custom: vec![CustomProperty {
            name: "x".repeat(256),
            value: CustomValue::I4 { value: 1 },
        }],
        ..DocumentProperties::default()
    };
    assert!(matches!(
        table_document(vec![paragraph_block(tid(1))])
            .unwrap()
            .with_properties(properties),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "custom.name"
        })
    ));
}

#[test]
fn negative_word_count_is_rejected() {
    let properties = DocumentProperties {
        app: AppProperties {
            words: Some(-1),
            ..AppProperties::default()
        },
        ..DocumentProperties::default()
    };
    assert!(matches!(
        table_document(vec![paragraph_block(tid(1))])
            .unwrap()
            .with_properties(properties),
        Err(ModelError::PropertyValueOutOfDomain {
            property: "app.words"
        })
    ));
}
