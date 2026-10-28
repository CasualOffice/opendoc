// SPDX-License-Identifier: Apache-2.0

//! What a paragraph holds besides its text (`docs/181` §2.2, G2): links and
//! their targets, fields with their codes and results, bookmarks, note and
//! comment references, inline content controls, and objects (pictures, charts,
//! diagrams, OLE objects, text boxes, shape groups, equations).
//!
//! One function, [`paragraph_facts`], reached through a `match` on
//! `InlineNode` **with no wildcard arm**: a new inline kind does not compile
//! until someone classifies it here. What is named but not shown is
//! [`Omitted`]: kind, id and a reason a person can read, counted in every read
//! result's `omitted` field, never silently dropped.
//!
//! Every string here (URLs, field codes, alt text, control tags) is document
//! content: untrusted data returned in structured fields, never instruction
//! (`docs/178` §5.8).

use casual_doc_layout::flow::node_plain_text;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Definitions, EmbeddedKind, Extent, GroupChild, HyperlinkTarget, InlineNode, NoteKind,
    Paragraph, SdtProperties,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::surface::Surface;
use crate::text::clip;

/// Most entries of one kind listed for one paragraph; the rest are counted.
const MAX_PER_KIND: usize = 50;

/// Longest projected string (URL, field code, alt text), in bytes.
const MAX_STRING: usize = 2_048;

/// Something a read names but does not show, and why.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Omitted {
    /// The node kind, e.g. `chart` or `alt_chunk`.
    pub kind: &'static str,
    /// The node's id.
    pub id: String,
    /// Why its content is not shown, in a sentence.
    pub reason: &'static str,
}

impl Omitted {
    pub(crate) fn new(kind: &'static str, id: NodeId, reason: &'static str) -> Self {
        Self {
            kind,
            id: id.to_string(),
            reason,
        }
    }
}

/// The reason an `altChunk` block is not read.
pub(crate) const ALT_CHUNK: &str = "an imported alt-chunk part (HTML, RTF, text or a nested \
     document) that the package preserves but the model does not parse";

/// Everything but the text that one paragraph holds. The lists are complete
/// (so counts are exact); what is written into a result is bounded by
/// `MAX_PER_KIND` (50) per kind, and `factsTruncated` says when one was cut.
#[derive(Debug, Default)]
pub(crate) struct Facts {
    pub links: Vec<Value>,
    pub fields: Vec<Value>,
    pub bookmarks: Vec<Value>,
    pub notes: Vec<Value>,
    pub comments: Vec<String>,
    pub controls: Vec<Value>,
    pub objects: Vec<Value>,
    pub omitted: Vec<Omitted>,
}

impl Facts {
    fn push(list: &mut Vec<Value>, value: Value) {
        list.push(value);
    }

    /// Adds the non-empty lists to a paragraph's JSON projection.
    pub(crate) fn write_into(&self, value: &mut Value) {
        let comments: Vec<Value> = self.comments.iter().map(|id| json!(id)).collect();
        let pairs: [(&str, &[Value]); 7] = [
            ("links", &self.links),
            ("fields", &self.fields),
            ("bookmarks", &self.bookmarks),
            ("notes", &self.notes),
            ("comments", &comments),
            ("contentControls", &self.controls),
            ("objects", &self.objects),
        ];
        let mut truncated = false;
        for (key, list) in pairs {
            if !list.is_empty() {
                truncated |= list.len() > MAX_PER_KIND;
                value[key] = json!(&list[..list.len().min(MAX_PER_KIND)]);
            }
        }
        if truncated {
            value["factsTruncated"] = json!(true);
        }
    }

    /// One Markdown line per fact, after the paragraph's text, so a model
    /// reading Markdown sees what JSON readers see.
    pub(crate) fn markdown(&self) -> String {
        let mut out = String::new();
        let line = |out: &mut String, text: String| {
            out.push_str("\n  - ");
            out.push_str(&text);
        };
        for link in self.links.iter().take(MAX_PER_KIND) {
            let target = link["url"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| link["bookmark"].as_str().map(|b| format!("#{b}")))
                .unwrap_or_default();
            line(
                &mut out,
                format!("link {:?} -> {target}", str_of(&link["text"])),
            );
        }
        for field in self.fields.iter().take(MAX_PER_KIND) {
            line(
                &mut out,
                format!(
                    "field {} {:?} shows {:?}",
                    str_of(&field["kind"]),
                    str_of(&field["instruction"]),
                    str_of(&field["result"])
                ),
            );
        }
        for bookmark in self.bookmarks.iter().take(MAX_PER_KIND) {
            line(
                &mut out,
                format!("bookmark {:?}", str_of(&bookmark["name"])),
            );
        }
        for note in self.notes.iter().take(MAX_PER_KIND) {
            line(
                &mut out,
                format!(
                    "{} reference -> {}",
                    str_of(&note["kind"]),
                    str_of(&note["surface"])
                ),
            );
        }
        for control in self.controls.iter().take(MAX_PER_KIND) {
            line(
                &mut out,
                format!(
                    "content control {} tag={:?} alias={:?}",
                    str_of(&control["type"]),
                    str_of(&control["tag"]),
                    str_of(&control["alias"])
                ),
            );
        }
        for object in self.objects.iter().take(MAX_PER_KIND) {
            let mut text = format!("{} {{#{}}}", str_of(&object["kind"]), str_of(&object["id"]));
            if let Some(alt) = object["alt"].as_str() {
                text.push_str(&format!(" alt={alt:?}"));
            }
            if let Some(surface) = object["surface"].as_str() {
                text.push_str(&format!(" -> {surface}"));
            }
            if let Some(math) = object["text"].as_str() {
                text.push_str(&format!(" {math:?}"));
            }
            line(&mut out, text);
        }
        for omitted in self.omitted.iter().take(MAX_PER_KIND) {
            line(
                &mut out,
                format!(
                    "[{} {} not shown: {}]",
                    omitted.kind, omitted.id, omitted.reason
                ),
            );
        }
        out
    }
}

fn str_of(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

fn extent(extent: Option<&Extent>) -> Value {
    extent.map_or(Value::Null, |e| json!([e.width_emu, e.height_emu]))
}

fn alt(descr: Option<&String>) -> Value {
    descr.map_or(Value::Null, |d| json!(clip(d, MAX_STRING)))
}

fn control(id: NodeId, properties: &SdtProperties, text: Option<String>) -> Value {
    let kind = properties
        .control_kind
        .and_then(|kind| serde_json::to_value(kind).ok())
        .unwrap_or_else(|| json!("rich_text"));
    let mut value = json!({
        "id": id.to_string(),
        "type": kind,
        "tag": properties.tag,
        "alias": properties.alias,
    });
    if let Some(lock) = properties
        .lock
        .and_then(|lock| serde_json::to_value(lock).ok())
    {
        value["lock"] = lock;
    }
    if properties.showing_placeholder {
        value["showingPlaceholder"] = json!(true);
    }
    if let Some(text) = text {
        value["text"] = json!(clip(&text, MAX_STRING));
    }
    value
}

/// A block-level content control's projection: type, tag, alias and lock, not
/// flattened away. O(1).
pub(crate) fn block_control(sdt: &casual_doc_model::v1::BlockSdt) -> Value {
    control(sdt.id, &sdt.properties, None)
}

/// A shape group's children, flattened, bounded by `MAX_PER_KIND`. A group text
/// box names its own surface. O(children).
fn group_children(children: &[GroupChild], out: &mut Vec<Value>) {
    for child in children {
        if out.len() >= MAX_PER_KIND {
            return;
        }
        match child {
            GroupChild::Picture(picture) => out.push(json!({
                "id": picture.id.to_string(),
                "kind": "picture",
                "alt": alt(picture.descr.as_ref()),
                "extentEmu": extent(Some(&picture.extent)),
            })),
            GroupChild::TextBox(text_box) => out.push(json!({
                "id": text_box.id.to_string(),
                "kind": "text_box",
                "extentEmu": extent(Some(&text_box.extent)),
                "surface": Surface::TextBox(text_box.id).to_string(),
            })),
            GroupChild::Shape(shape) => out.push(json!({
                "id": shape.id.to_string(),
                "kind": "shape",
                "extentEmu": extent(Some(&shape.extent)),
            })),
            GroupChild::Group(nested) => group_children(&nested.children, out),
        }
    }
}

/// What `paragraph` holds besides its text. **O(inlines)**, plus O(log n) per
/// bookmark or field-range lookup in the definitions.
pub(crate) fn paragraph_facts(definitions: &Definitions, paragraph: &Paragraph) -> Facts {
    let mut facts = Facts::default();
    walk(definitions, &paragraph.inlines, &mut facts);
    facts
}

const CHART: &str = "a chart's series and data are not projected yet (`docs/181` §4.2 step 7)";
const DIAGRAM: &str = "a SmartArt diagram's content is preserved, not projected";
const OLE: &str = "an OLE object is binary content from another program; only its type is known";
const OTHER_GRAPHIC: &str = "a graphic of a kind the model preserves but does not read";
const MATH: &str = "an equation with no linear text form";

#[allow(clippy::too_many_lines)]
fn walk(definitions: &Definitions, inlines: &[InlineNode], facts: &mut Facts) {
    for inline in inlines {
        // `docs/181` G2: every inline kind is classified here, with no wildcard.
        match inline {
            InlineNode::Hyperlink(link) => {
                let mut value = json!({
                    "id": link.id.to_string(),
                    "text": clip(&node_plain_text(&link.inlines), MAX_STRING),
                });
                match &link.target {
                    HyperlinkTarget::External(target) => {
                        value["url"] = json!(clip(&target.url, MAX_STRING));
                        if let Some(anchor) = &target.anchor {
                            value["fragment"] = json!(anchor);
                        }
                    }
                    HyperlinkTarget::Internal(target) => {
                        value["bookmark"] = json!(target.anchor);
                    }
                }
                if let Some(tooltip) = &link.tooltip {
                    value["tooltip"] = json!(tooltip);
                }
                Facts::push(&mut facts.links, value);
                walk(definitions, &link.inlines, facts);
            }
            InlineNode::Field(field) => {
                let kind = serde_json::to_value(&field.kind)
                    .ok()
                    .and_then(|v| v.get("type").cloned())
                    .unwrap_or_else(|| json!("other"));
                let mut value = json!({
                    "id": field.id.to_string(),
                    "kind": kind,
                    "instruction": clip(field.instruction.trim(), MAX_STRING),
                    "result": clip(&node_plain_text(&field.inlines), MAX_STRING),
                });
                if field.form.is_some() {
                    value["formField"] = json!(true);
                }
                Facts::push(&mut facts.fields, value);
                walk(definitions, &field.inlines, facts);
            }
            InlineNode::FieldRangeStart(start) => {
                let (instruction, kind) = definitions.field_ranges.get(&start.field).map_or(
                    (String::new(), json!("other")),
                    |range| {
                        (
                            range.instruction.trim().to_owned(),
                            serde_json::to_value(&range.kind)
                                .ok()
                                .and_then(|v| v.get("type").cloned())
                                .unwrap_or_else(|| json!("other")),
                        )
                    },
                );
                Facts::push(
                    &mut facts.fields,
                    json!({
                        "id": start.id.to_string(),
                        "kind": kind,
                        "instruction": clip(&instruction, MAX_STRING),
                        "result": Value::Null,
                        "spansParagraphs": true,
                    }),
                );
            }
            InlineNode::BookmarkStart(start) => {
                let name = definitions
                    .bookmarks
                    .get(&start.bookmark)
                    .map(|bookmark| bookmark.name.clone());
                Facts::push(
                    &mut facts.bookmarks,
                    json!({ "id": start.bookmark.node_id().to_string(), "name": name }),
                );
            }
            InlineNode::NoteReference(reference) => {
                let surface = match reference.kind {
                    NoteKind::Footnote => Surface::Footnote(reference.note.node_id()),
                    NoteKind::Endnote => Surface::Endnote(reference.note.node_id()),
                };
                Facts::push(
                    &mut facts.notes,
                    json!({
                        "id": reference.id.to_string(),
                        "kind": surface.kind(),
                        "note": reference.note.node_id().to_string(),
                        "surface": surface.to_string(),
                    }),
                );
            }
            InlineNode::CommentReference(reference) => {
                facts.comments.push(reference.comment.node_id().to_string());
            }
            InlineNode::Sdt(sdt) => {
                Facts::push(
                    &mut facts.controls,
                    control(sdt.id, &sdt.properties, Some(node_plain_text(&sdt.inlines))),
                );
                walk(definitions, &sdt.inlines, facts);
            }
            InlineNode::Revision(revision) => {
                walk(definitions, &revision.inlines, facts);
            }
            InlineNode::Drawing(drawing) => Facts::push(
                &mut facts.objects,
                json!({
                    "id": drawing.id.to_string(),
                    "kind": "picture",
                    "alt": alt(drawing.descr.as_ref()),
                    "extentEmu": extent(drawing.extent.as_ref()),
                }),
            ),
            InlineNode::AnchoredDrawing(drawing) => Facts::push(
                &mut facts.objects,
                json!({
                    "id": drawing.id.to_string(),
                    "kind": "floating_picture",
                    "alt": alt(drawing.descr.as_ref()),
                    "extentEmu": extent(Some(&drawing.extent)),
                }),
            ),
            InlineNode::EmbeddedObject(object) => {
                let (kind, reason) = match &object.kind {
                    EmbeddedKind::Chart => ("chart", CHART),
                    EmbeddedKind::Diagram => ("diagram", DIAGRAM),
                    EmbeddedKind::OleObject => ("ole_object", OLE),
                    EmbeddedKind::Other(_) => ("graphic", OTHER_GRAPHIC),
                };
                let mut value = json!({
                    "id": object.id.to_string(),
                    "kind": kind,
                    "extentEmu": extent(Some(&object.extent)),
                    "opaque": reason,
                });
                if let Some(prog_id) = &object.prog_id {
                    value["progId"] = json!(prog_id);
                }
                Facts::push(&mut facts.objects, value);
                facts.omitted.push(Omitted::new(kind, object.id, reason));
            }
            InlineNode::TextBox(text_box) => Facts::push(
                &mut facts.objects,
                json!({
                    "id": text_box.id.to_string(),
                    "kind": "text_box",
                    "floating": text_box.anchor.is_some(),
                    "extentEmu": extent(text_box.extent.as_ref()),
                    "surface": Surface::TextBox(text_box.id).to_string(),
                }),
            ),
            InlineNode::Group(group) => {
                let mut children = Vec::new();
                group_children(&group.children, &mut children);
                Facts::push(
                    &mut facts.objects,
                    json!({
                        "id": group.id.to_string(),
                        "kind": "group",
                        "extentEmu": extent(Some(&group.extent)),
                        "children": children,
                    }),
                );
            }
            InlineNode::Math(math) => {
                if math.text.is_empty() {
                    Facts::push(
                        &mut facts.objects,
                        json!({ "id": math.id.to_string(), "kind": "equation", "opaque": MATH }),
                    );
                    facts.omitted.push(Omitted::new("equation", math.id, MATH));
                } else {
                    Facts::push(
                        &mut facts.objects,
                        json!({
                            "id": math.id.to_string(),
                            "kind": "equation",
                            "text": clip(&math.text, MAX_STRING),
                        }),
                    );
                }
            }
            // Text, layout characters and zero-width markers: the paragraph's
            // `text` and `runs` already carry what a reader sees of them, and a
            // marker's meaning is listed through the node that owns it (a
            // comment range through `comments`, a bookmark end through its
            // start, a field range end through its start, a move range through
            // `list_tracked_changes`).
            InlineNode::Run(_)
            | InlineNode::Tab(_)
            | InlineNode::Break(_)
            | InlineNode::Symbol(_)
            | InlineNode::HorizontalRule(_)
            | InlineNode::NoBreakHyphen(_)
            | InlineNode::SoftHyphen(_)
            | InlineNode::PositionalTab(_)
            | InlineNode::NoteNumberMark(_)
            | InlineNode::CommentRangeStart(_)
            | InlineNode::CommentRangeEnd(_)
            | InlineNode::BookmarkEnd(_)
            | InlineNode::FieldRangeEnd(_)
            | InlineNode::MoveRangeStart(_)
            | InlineNode::MoveRangeEnd(_) => {}
        }
    }
}
