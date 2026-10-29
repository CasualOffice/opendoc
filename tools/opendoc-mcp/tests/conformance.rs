// SPDX-License-Identifier: Apache-2.0

//! The MCP conformance guard of `docs/178` §8: the real binary, spoken to over
//! stdio by a scripted client. It lists the tools, calls every one, and holds
//! the profile filter, the edit grant and the path allowlist.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

/// The 23 tools at the widest profile, in `tools/list` order (`docs/178` §6.2, plus
/// the two discovery tools of `docs/181` §5.1 and `convert_pdf_to_flowing`,
/// `docs/182` §18).
const ALL_TOOLS: &[&str] = &[
    "open_document",
    "close_document",
    "convert_pdf_to_flowing",
    "get_outline",
    "read_blocks",
    "get_styles",
    "read_table",
    "list_comments",
    "list_tracked_changes",
    "search",
    "chunk_document",
    "preview_edits",
    "apply_edits",
    "add_comment",
    "undo_change",
    "review_tracked_changes",
    "export_document",
    "render_page",
    "compare_documents",
    "compatibility_report",
    "validate_document",
    "list_edit_kinds",
    "describe_edit_kind",
];

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Client {
    fn start(args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_opendoc-mcp"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the server starts");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            child,
            stdin,
            stdout,
            next: 0,
        }
    }

    /// A 2026-07-28 stateless request: `_meta` names the protocol version and
    /// the client's capabilities, as SEP-2575 requires of every request.
    /// `initialize` is the older lifecycle and is sent as it was.
    fn request(&mut self, method: &str, mut params: Value) -> Value {
        if method != "initialize"
            && let Some(object) = params.as_object_mut()
        {
            object.entry("_meta").or_insert_with(|| {
                json!({
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {},
                    "io.modelcontextprotocol/clientInfo": { "name": "conformance.rs", "version": "1" }
                })
            });
        }
        self.raw(method, params)
    }

    /// A request sent exactly as given.
    fn raw(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let message =
            json!({ "jsonrpc": "2.0", "id": self.next, "method": method, "params": params });
        writeln!(self.stdin, "{message}").expect("write");
        self.stdin.flush().expect("flush");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("a reply");
        let reply: Value = serde_json::from_str(&line).expect("JSON-RPC");
        assert_eq!(
            reply["id"], self.next,
            "replies come back in order with their id"
        );
        reply
    }

    fn result(&mut self, method: &str, params: Value) -> Value {
        let reply = self.request(method, params);
        assert!(reply.get("error").is_none(), "{method}: {reply}");
        reply["result"].clone()
    }

    /// A tool call that must succeed; returns its structured content.
    fn ok(&mut self, tool: &str, arguments: Value) -> Value {
        let result = self.result(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        );
        assert_eq!(result["isError"], false, "{tool} refused: {result}");
        assert!(result["content"].as_array().is_some_and(|c| !c.is_empty()));
        result["structuredContent"].clone()
    }

    /// A tool call that must be refused; returns the refusal.
    fn refused(&mut self, tool: &str, arguments: Value) -> Value {
        let result = self.result(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        );
        assert_eq!(result["isError"], true, "{tool} was not refused: {result}");
        result["structuredContent"]["error"].clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A fresh directory tree for one test: `root/`, `write/` and `outside/`.
fn sandbox(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("opendoc-mcp-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    for dir in ["root", "write", "outside"] {
        std::fs::create_dir_all(base.join(dir)).expect("mkdir");
    }
    std::fs::write(
        base.join("root/contract.txt"),
        "Agreement\nThe notice period is thirty (30) days.\nTermination for convenience applies.\n",
    )
    .expect("fixture");
    std::fs::write(
        base.join("root/contract-v2.txt"),
        "Agreement\nThe notice period is sixty (60) days.\nTermination for convenience applies.\n",
    )
    .expect("fixture");
    std::fs::write(base.join("root/table.docx"), table_docx()).expect("fixture");
    std::fs::write(base.join("outside/secret.txt"), "secret\n").expect("fixture");
    base
}

/// A minimal WordprocessingML package holding a paragraph and a 2x2 table.
fn table_docx() -> Vec<u8> {
    use zip::write::SimpleFileOptions;
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let parts: [(&str, &str); 3] = [
        (
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "word/document.xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Prices</w:t></w:r></w:p><w:tbl><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>Item</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Price</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Tea</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>3</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/><w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>"#,
        ),
    ];
    for (name, body) in parts {
        zip.start_file(name, SimpleFileOptions::default())
            .expect("zip entry");
        zip.write_all(body.as_bytes()).expect("zip write");
    }
    zip.finish().expect("zip finish");
    out.into_inner()
}

fn arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn the_server_lists_every_tool_and_every_tool_answers() {
    let base = sandbox("all");
    let (root, write) = (arg(&base.join("root")), arg(&base.join("write")));
    let mut client = Client::start(&[
        "--root",
        &root,
        "--write-root",
        &write,
        "--profile",
        "review",
        "--author",
        "Claude (AI)",
    ]);

    let init = client.result(
        "initialize",
        json!({ "protocolVersion": "2026-07-28", "capabilities": {} }),
    );
    assert_eq!(init["protocolVersion"], "2026-07-28");
    let older = client.result("initialize", json!({ "protocolVersion": "2025-11-25" }));
    assert_eq!(
        older["protocolVersion"], "2025-11-25",
        "Q12: an older client is answered"
    );
    let discover = client.result("server/discover", json!({}));
    assert_eq!(discover["serverInfo"]["name"], "opendoc-mcp");
    assert!(client.request("no/such/method", json!({}))["error"]["code"] == -32601);

    let listed = client.result("tools/list", json!({}));
    let names: Vec<&str> = listed["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(names, ALL_TOOLS);
    for tool in listed["tools"].as_array().expect("tools") {
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        assert!(tool["annotations"]["readOnlyHint"].is_boolean());
    }
    // `docs/181`: the published edit vocabulary is the capability registry,
    // not a hand-kept list.
    for name in ["preview_edits", "apply_edits"] {
        let tool = listed["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .find(|tool| tool["name"] == name)
            .expect("an edit tool");
        let items = &tool["inputSchema"]["properties"]["plan"]["properties"]["edits"]["items"];
        assert_eq!(
            casual_doc_command::schema::ops(items),
            casual_doc_command::ids(),
            "{name} publishes the registry's ops, each with its generated fields"
        );
    }

    // --- every tool, once ---
    let opened = client.ok("open_document", json!({ "path": "contract.txt" }));
    let doc = opened["document"].as_str().expect("a handle").to_owned();
    assert!(
        doc.starts_with("doc_") && doc.len() == 36,
        "an opaque 128-bit handle"
    );
    let v2 = client.ok("open_document", json!({ "path": "contract-v2.txt" }))["document"]
        .as_str()
        .expect("a handle")
        .to_owned();
    let tables = client.ok("open_document", json!({ "path": "table.docx" }))["document"]
        .as_str()
        .expect("a handle")
        .to_owned();

    let outline = client.ok("get_outline", json!({ "document": doc }));
    assert_eq!(outline["estimator"], "bytes/4");
    let blocks = client.ok(
        "read_blocks",
        json!({ "document": doc, "format": "markdown", "max_tokens": 5 }),
    );
    assert_eq!(
        blocks["truncated"], true,
        "a small ceiling truncates and says so"
    );
    assert!(blocks["nextCursor"].is_string());
    let more = client.ok(
        "read_blocks",
        json!({ "document": doc, "cursor": blocks["nextCursor"] }),
    );
    assert!(more["blocks"].as_array().is_some_and(|b| !b.is_empty()));
    client.ok("get_styles", json!({ "document": doc }));

    let table_outline = client.ok("get_outline", json!({ "document": tables }));
    let table_id = table_outline["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|e| e["kind"] == "table")
        .expect("the table is in the outline")["node"]
        .clone();
    let table = client.ok(
        "read_table",
        json!({ "document": tables, "table": table_id }),
    );
    assert_eq!(table["rows"][1]["cells"][0]["text"], "Tea");

    let hits = client.ok(
        "search",
        json!({ "document": doc, "query": "thirty (30) days" }),
    );
    let node = hits["hits"][0]["node"].clone();
    client.ok("chunk_document", json!({ "document": doc }));

    // `docs/181` R2: the `surface` argument reaches the library through the
    // server, for one surface and for all of them.
    let everywhere = client.ok(
        "search",
        json!({ "document": doc, "query": "thirty", "surface": "all" }),
    );
    assert_eq!(everywhere["surfaces"], "all", "{everywhere}");
    assert_eq!(everywhere["hits"][0]["surface"], "body");
    let body = client.ok(
        "read_blocks",
        json!({ "document": doc, "surface": "body", "max_tokens": 200 }),
    );
    assert_eq!(body["surface"], "body", "{body}");
    let chunks = client.ok(
        "chunk_document",
        json!({ "document": doc, "surface": "all" }),
    );
    assert!(chunks["chunks"].as_array().is_some_and(|c| !c.is_empty()));
    let refused = client.refused(
        "read_blocks",
        json!({ "document": doc, "surface": "header:not-an-id" }),
    );
    assert!(
        refused["code"].is_string(),
        "an unknown surface is refused: {refused}"
    );

    let revision = opened["revision"].clone();
    let plan = json!({
        "base_revision": revision,
        "edits": [{ "op": "replace_text", "target": { "node": node, "exact": "thirty (30) days" }, "text": "sixty (60) days" }],
    });
    let preview = client.ok("preview_edits", json!({ "document": doc, "plan": plan }));
    assert_eq!(preview["wouldApply"], true);
    let applied = client.ok("apply_edits", json!({ "document": doc, "plan": plan }));
    let changes = client.ok("list_tracked_changes", json!({ "document": doc }));
    assert_eq!(
        changes["total"], 2,
        "a replacement is a deletion and an insertion"
    );
    assert_eq!(changes["trackedChanges"][0]["author"], "Claude (AI)");

    let commented = client.ok(
        "add_comment",
        json!({ "document": doc, "base_revision": applied["revision"], "target": { "node": node, "exact": "notice" }, "text": "Check against the MSA." }),
    );
    assert_eq!(
        client.ok("list_comments", json!({ "document": doc }))["total"],
        1
    );
    client.ok(
        "undo_change",
        json!({ "document": doc, "change_id": commented["changeId"] }),
    );
    assert_eq!(
        client.ok("list_comments", json!({ "document": doc }))["total"],
        0
    );

    client.ok(
        "review_tracked_changes",
        json!({ "document": doc, "base_revision": applied["revision"].as_u64().expect("rev") + 2, "decision": "accept", "all": true }),
    );
    assert_eq!(
        client.ok("list_tracked_changes", json!({ "document": doc }))["total"],
        0
    );

    let exported = client.ok(
        "export_document",
        json!({ "document": doc, "format": "docx" }),
    );
    let uri = exported["uri"]
        .as_str()
        .expect("an export resource")
        .to_owned();
    let read = client.result("resources/read", json!({ "uri": uri }));
    assert!(
        read["contents"][0]["blob"]
            .as_str()
            .is_some_and(|b| b.starts_with("UEsDB")),
        "a zip, base64"
    );
    client.ok(
        "export_document",
        json!({ "document": doc, "format": "docx", "path": "out.docx" }),
    );
    assert!(base.join("write/out.docx").is_file());
    let again = client.refused(
        "export_document",
        json!({ "document": doc, "format": "docx", "path": "out.docx", "overwrite": true }),
    );
    assert_eq!(
        again["code"], "overwrite-refused",
        "no --allow-overwrite, no overwrite"
    );

    let page = client.result(
        "tools/call",
        json!({ "name": "render_page", "arguments": { "document": doc, "page": 1, "dpi": 40 } }),
    );
    assert_eq!(page["isError"], false, "{page}");
    assert_eq!(page["content"][1]["type"], "image");
    assert_eq!(page["content"][1]["mimeType"], "image/png");

    let diff = client.ok("compare_documents", json!({ "left": v2, "right": doc }));
    assert!(diff["total"].is_number());
    let report = client.ok(
        "compatibility_report",
        json!({ "document": doc, "format": "txt" }),
    );
    assert_eq!(report["lossyByNature"], true);
    let valid = client.ok("validate_document", json!({ "document": doc }));
    assert_eq!(valid["valid"], true, "{valid}");
    let kinds = client.ok("list_edit_kinds", json!({}));
    assert_eq!(
        kinds["withheld"],
        json!([]),
        "the review profile with no toolsets admits every op"
    );
    assert_eq!(
        client.refused("convert_pdf_to_flowing", json!({ "document": doc }))["code"],
        "not-a-pdf",
        "a text file is not a PDF"
    );
    let described = client.ok("describe_edit_kind", json!({ "op": "replace_text" }));
    assert_eq!(
        described["schema"]["properties"]["op"]["const"],
        "replace_text"
    );
    client.ok("close_document", json!({ "document": doc }));
    let gone = client.refused("get_outline", json!({ "document": doc }));
    assert_eq!(gone["code"], "not-found");

    // --- resources and prompts ---
    let resources = client.result("resources/list", json!({}));
    assert!(
        resources["resources"]
            .as_array()
            .is_some_and(|r| r.iter().any(|x| x["uri"] == "opendoc://documents"))
    );
    assert!(client.result("resources/templates/list", json!({}))["resourceTemplates"].is_array());
    let prompts = client.result("prompts/list", json!({}));
    assert_eq!(prompts["prompts"].as_array().map(Vec::len), Some(4));
    let prompt = client.result(
        "prompts/get",
        json!({ "name": "redline", "arguments": { "doc": "d", "instructions": "tighten" } }),
    );
    assert!(
        prompt["messages"][0]["content"]["text"]
            .as_str()
            .is_some_and(|t| t.contains("preview_edits"))
    );
}

#[test]
fn a_suggest_profile_hides_review_and_its_direct_edits_are_refused() {
    let base = sandbox("grant");
    let root = arg(&base.join("root"));
    let mut client = Client::start(&["--root", &root, "--profile", "suggest"]);
    let listed = client.result("tools/list", json!({}));
    let names: Vec<&str> = listed["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert_eq!(names.len(), ALL_TOOLS.len() - 1);
    assert!(
        !names.contains(&"review_tracked_changes"),
        "an ungranted tool is not listed"
    );

    let doc = client.ok("open_document", json!({ "path": "contract.txt" }))["document"].clone();
    let withheld = client.refused(
        "review_tracked_changes",
        json!({ "document": doc, "base_revision": 0, "decision": "accept", "all": true }),
    );
    assert_eq!(withheld["code"], "capability-withheld");

    let node =
        client.ok("search", json!({ "document": doc, "query": "thirty" }))["hits"][0]["node"]
            .clone();
    let direct = client.refused(
        "apply_edits",
        json!({ "document": doc, "plan": { "base_revision": 0, "mode": "direct",
            "edits": [{ "op": "insert_text", "at": { "node": node, "exact": "days" }, "text": " net" }] } }),
    );
    assert_eq!(direct["code"], "plan-refused");
    assert_eq!(direct["details"]["firstCode"], "capability-withheld");
    assert_eq!(
        client.ok("list_tracked_changes", json!({ "document": doc }))["total"],
        0,
        "nothing landed"
    );
}

#[test]
fn paths_outside_the_operator_roots_are_refused() {
    let base = sandbox("paths");
    let root = arg(&base.join("root"));
    let mut client = Client::start(&["--root", &root]);
    let secret = base.join("outside/secret.txt");
    for path in [
        "../outside/secret.txt".to_owned(),
        arg(&secret),
        format!("file://{}", arg(&secret)),
    ] {
        let refused = client.refused("open_document", json!({ "path": path }));
        assert_eq!(refused["code"], "path-outside-roots", "{path}");
        assert!(
            !refused["message"]
                .as_str()
                .unwrap_or("")
                .contains("outside/secret"),
            "a refusal never echoes the resolved target"
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&secret, base.join("root/link.txt")).expect("symlink");
        let refused = client.refused("open_document", json!({ "path": "link.txt" }));
        assert_eq!(
            refused["code"], "path-outside-roots",
            "a symlink out is resolved and refused"
        );
    }
    std::fs::write(base.join("root/macro.docm"), b"PK").expect("fixture");
    assert_eq!(
        client.refused("open_document", json!({ "path": "macro.docm" }))["code"],
        "unsupported-format"
    );
    let doc = client.ok("open_document", json!({ "path": "contract.txt" }))["document"].clone();
    assert_eq!(
        client.refused(
            "export_document",
            json!({ "document": doc, "format": "docx", "path": "x.docx" })
        )["code"],
        "no-write-root",
        "the default is to write nothing to disk"
    );
}

/// The names of the listed tools.
fn tool_names(client: &mut Client) -> Vec<String> {
    client.result("tools/list", json!({}))["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
        .collect()
}

/// `op` → the modes `list_edit_kinds` admits it in, for every admitted op.
fn admitted_modes(kinds: &Value) -> Vec<(String, Vec<String>)> {
    kinds["families"]
        .as_array()
        .expect("families")
        .iter()
        .flat_map(|family| family["ops"].as_array().expect("ops").iter())
        .map(|op| {
            let modes = op["modes"]
                .as_array()
                .expect("modes")
                .iter()
                .filter_map(|m| m.as_str().map(str::to_owned))
                .collect();
            (op["id"].as_str().expect("an id").to_owned(), modes)
        })
        .collect()
}

#[test]
fn discovery_describes_the_registry_and_the_profile_the_engine_enforces() {
    let base = sandbox("discovery");
    let root = arg(&base.join("root"));
    let mut client = Client::start(&["--root", &root, "--profile", "suggest"]);

    // `docs/181` §5.1. What the session admits is asked of the AI layer's own
    // capability check, so it is exactly what `apply_edits` would enforce:
    // content edits as suggestions only, table rows (edit grant, never
    // suggestible) not at all, comments in either mode, review not at all.
    let kinds = client.ok("list_edit_kinds", json!({}));
    assert_eq!(
        kinds["vocabularyVersion"],
        casual_doc_command::schema::VOCABULARY_VERSION
    );
    let suggest_only = vec!["suggest".to_owned()];
    let both = vec!["suggest".to_owned(), "direct".to_owned()];
    // Derived from the registry, so a new command is covered without editing
    // this test: under `suggest`, a trackable command is admitted as a
    // suggestion only; a comment command in either mode; everything that needs
    // `edit` or `review` (untrackable structure, review decisions) not at all.
    let expected: Vec<(String, Vec<String>)> = casual_doc_command::COMMANDS
        .iter()
        .filter_map(|command| match command.trackable {
            casual_doc_command::Trackable::Yes => Some((command.id, &suggest_only)),
            casual_doc_command::Trackable::NotApplicable
                if command.family == casual_doc_command::Family::Review
                    && command.id != "review" =>
            {
                Some((command.id, &both))
            }
            _ => None,
        })
        .map(|(op, modes)| (op.to_owned(), modes.clone()))
        .collect();
    let mut expected = expected;
    expected.sort();
    let mut admitted = admitted_modes(&kinds);
    admitted.sort();
    assert_eq!(admitted, expected, "{kinds}");
    let withheld: Vec<(&str, &str)> = kinds["withheld"]
        .as_array()
        .expect("withheld")
        .iter()
        .map(|w| {
            (
                w["op"].as_str().expect("op"),
                w["withheldBy"].as_str().expect("by"),
            )
        })
        .collect();
    // Everything not admitted is withheld, by the profile, in registry order.
    let admitted_ids: Vec<&str> = expected.iter().map(|(op, _)| op.as_str()).collect();
    let expected_withheld: Vec<(&str, &str)> = casual_doc_command::COMMANDS
        .iter()
        .filter(|command| !admitted_ids.contains(&command.id))
        .map(|command| (command.id, "profile"))
        .collect();
    assert_eq!(withheld, expected_withheld);
    assert!(withheld.contains(&("review", "profile")));
    assert!(withheld.contains(&("table_insert_row", "profile")));

    // The engine agrees: a mode the listing withholds is refused by the profile.
    let doc = client.ok("open_document", json!({ "path": "contract.txt" }))["document"].clone();
    let node =
        client.ok("search", json!({ "document": doc, "query": "thirty" }))["hits"][0]["node"]
            .clone();
    let refused = client.refused(
        "apply_edits",
        json!({ "document": doc, "plan": { "base_revision": 0, "mode": "direct",
            "edits": [{ "op": "replace_text", "target": { "node": node, "exact": "thirty" }, "text": "sixty" }] } }),
    );
    assert_eq!(refused["details"]["firstCode"], "capability-withheld");

    // One op: its generated schema, trackability and reason, and an example.
    let rows = client.ok("describe_edit_kind", json!({ "op": "table_insert_row" }));
    assert_eq!(
        rows["schema"],
        casual_doc_command::schema::command("table_insert_row").expect("registered")
    );
    assert_eq!(rows["trackable"], "no");
    assert!(
        rows["trackableReason"]
            .as_str()
            .is_some_and(|r| r.contains("Q11"))
    );
    assert_eq!(rows["example"]["op"], "table_insert_row");
    assert_eq!(rows["admitted"], false);
    assert_eq!(rows["withheldBy"], "profile");
    let unknown = client.refused("describe_edit_kind", json!({ "op": "insert_rocket" }));
    assert_eq!(unknown["code"], "unknown-edit");
    assert!(unknown["details"]["known"].is_array());

    // The per-family schema, as a resource template.
    let templates = client.result("resources/templates/list", json!({}));
    assert!(
        templates["resourceTemplates"]
            .as_array()
            .expect("templates")
            .iter()
            .any(|t| t["uriTemplate"] == "opendoc://schema/edits/{family}")
    );
    let tables = client.result(
        "resources/read",
        json!({ "uri": "opendoc://schema/edits/tables" }),
    );
    let schema: Value = serde_json::from_str(
        tables["contents"][0]["text"]
            .as_str()
            .expect("a text resource"),
    )
    .expect("JSON");
    assert_eq!(
        casual_doc_command::schema::ops(&schema),
        casual_doc_command::COMMANDS
            .iter()
            .filter(|command| command.family == casual_doc_command::Family::Tables)
            .map(|command| command.id)
            .collect::<Vec<_>>()
    );
    let missing = client.request(
        "resources/read",
        json!({ "uri": "opendoc://schema/edits/rockets" }),
    );
    assert!(missing["error"].is_object(), "{missing}");
}

#[test]
fn toolsets_limit_the_edit_families_and_the_read_tools_stay() {
    let base = sandbox("toolsets");
    let root = arg(&base.join("root"));
    let mut client = Client::start(&["--root", &root, "--profile", "edit", "--toolsets", "text"]);

    // `docs/181` §5.2: read tools always; a tool that only makes edits of an
    // excluded family is not listed.
    let names = tool_names(&mut client);
    for read in [
        "open_document",
        "get_outline",
        "read_blocks",
        "search",
        "list_edit_kinds",
    ] {
        assert!(names.iter().any(|n| n == read), "{read} is listed");
    }
    assert!(
        !names.iter().any(|n| n == "add_comment"),
        "add_comment makes review-family edits"
    );
    let listed = client.result("tools/list", json!({}));
    let apply = listed["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|t| t["name"] == "apply_edits")
        .expect("apply_edits is listed")
        .clone();
    assert_eq!(
        casual_doc_command::schema::ops(
            &apply["inputSchema"]["properties"]["plan"]["properties"]["edits"]["items"]
        ),
        casual_doc_command::COMMANDS
            .iter()
            .filter(|command| command.family == casual_doc_command::Family::Text)
            .map(|command| command.id)
            .collect::<Vec<_>>(),
        "the published plan schema holds only the admitted families"
    );
    let kinds = client.ok("list_edit_kinds", json!({}));
    assert_eq!(kinds["toolsets"], json!(["text"]));
    let families: Vec<Value> = kinds["families"]
        .as_array()
        .expect("families")
        .iter()
        .map(|f| f["name"].clone())
        .collect();
    assert_eq!(families, [json!("text")]);

    // Enforced, not only advertised: an op of another family is refused by
    // apply and by preview, and nothing lands.
    let tables = client.ok("open_document", json!({ "path": "table.docx" }))["document"].clone();
    let table = client.ok("get_outline", json!({ "document": tables }))["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|e| e["kind"] == "table")
        .expect("a table")["node"]
        .clone();
    let plan = json!({ "base_revision": 0, "mode": "direct",
        "edits": [{ "op": "table_insert_row", "table": table, "index": 1 }] });
    for tool in ["apply_edits", "preview_edits"] {
        let refused = client.refused(tool, json!({ "document": tables, "plan": plan }));
        assert_eq!(refused["code"], "toolset-withheld", "{tool}: {refused}");
        assert_eq!(refused["details"]["family"], "tables");
    }
    let read = client.ok("read_table", json!({ "document": tables, "table": table }));
    assert_eq!(
        read["rows"].as_array().map(Vec::len),
        Some(2),
        "nothing landed"
    );
    let comment = client.refused(
        "add_comment",
        json!({ "document": tables, "base_revision": 0, "target": { "node": table }, "text": "x" }),
    );
    assert_eq!(comment["code"], "toolset-withheld");

    // An admitted family still works.
    let doc = client.ok("open_document", json!({ "path": "contract.txt" }))["document"].clone();
    let node =
        client.ok("search", json!({ "document": doc, "query": "thirty" }))["hits"][0]["node"]
            .clone();
    client.ok(
        "apply_edits",
        json!({ "document": doc, "plan": { "base_revision": 0,
            "edits": [{ "op": "replace_text", "target": { "node": node, "exact": "thirty" }, "text": "sixty" }] } }),
    );
}

#[test]
fn read_only_overrides_the_profile_in_either_order_and_admits_no_edit() {
    let base = sandbox("readonly");
    let root = arg(&base.join("root"));
    for args in [
        ["--read-only", "--profile", "review"],
        ["--profile", "review", "--read-only"],
    ] {
        let mut client = Client::start(&["--root", &root, args[0], args[1], args[2]]);
        let names = tool_names(&mut client);
        for write in [
            "apply_edits",
            "add_comment",
            "undo_change",
            "review_tracked_changes",
        ] {
            assert!(
                !names.iter().any(|n| n == write),
                "{write} is hidden under --read-only ({args:?})"
            );
        }
        let kinds = client.ok("list_edit_kinds", json!({}));
        assert_eq!(kinds["readOnly"], true);
        assert_eq!(kinds["profile"], "read");
        assert_eq!(
            kinds["families"],
            json!([]),
            "no edit is admitted ({args:?})"
        );
        let doc = client.ok("open_document", json!({ "path": "contract.txt" }))["document"].clone();
        let refused = client.refused(
            "apply_edits",
            json!({ "document": doc, "plan": { "base_revision": 0, "edits": [] } }),
        );
        assert_eq!(refused["code"], "capability-withheld");
    }
}

#[test]
fn an_unknown_toolset_stops_the_server_at_launch() {
    let base = sandbox("badtoolset");
    let output = Command::new(env!("CARGO_BIN_EXE_opendoc-mcp"))
        .args([
            "--root",
            &arg(&base.join("root")),
            "--toolsets",
            "text,rockets",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("the server runs");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown toolset rockets") && stderr.contains("tables"),
        "{stderr}"
    );
}

#[test]
fn every_list_and_discover_result_carries_the_2026_07_28_cache_fields() {
    // Found by the official conformance suite's wire-schema check
    // (`docs/181` §12, lane generation): `ttlMs` and `cacheScope` are
    // required on these results in MCP 2026-07-28.
    let base = sandbox("cache");
    let root = arg(&base.join("root"));
    let mut client = Client::start(&["--root", &root]);
    for method in [
        "server/discover",
        "tools/list",
        "resources/list",
        "resources/templates/list",
        "prompts/list",
    ] {
        let result = client.result(method, json!({}));
        assert!(result["ttlMs"].is_u64(), "{method} carries ttlMs: {result}");
        assert!(
            result["cacheScope"].is_string(),
            "{method} carries cacheScope: {result}"
        );
    }
}

#[test]
fn a_stateless_request_must_carry_its_meta_and_a_spoken_version() {
    // MCP 2026-07-28 SEP-2575, as the official conformance suite's
    // `server-stateless` scenario checks it (`docs/181` §12, lane generation).
    let base = sandbox("stateless");
    let root = arg(&base.join("root"));
    let mut client = Client::start(&["--root", &root]);
    let version = "io.modelcontextprotocol/protocolVersion";
    let capabilities = "io.modelcontextprotocol/clientCapabilities";

    let bare = client.raw("server/discover", json!({}));
    assert_eq!(bare["error"]["code"], -32602, "no _meta: {bare}");
    let bare = client.raw("tools/list", json!({}));
    assert_eq!(
        bare["error"]["code"], -32602,
        "no initialize and no _meta: {bare}"
    );
    let no_version = client.raw("server/discover", json!({ "_meta": { capabilities: {} } }));
    assert_eq!(no_version["error"]["code"], -32602);
    let no_capabilities = client.raw(
        "server/discover",
        json!({ "_meta": { version: "2026-07-28" } }),
    );
    assert_eq!(no_capabilities["error"]["code"], -32602);
    let unknown = client.raw(
        "server/discover",
        json!({ "_meta": { version: "v999.0.0", capabilities: {} } }),
    );
    assert_eq!(unknown["error"]["code"], -32022);
    assert_eq!(unknown["error"]["data"]["requested"], "v999.0.0");
    assert!(
        unknown["error"]["data"]["supported"]
            .as_array()
            .is_some_and(|v| v.contains(&json!("2026-07-28")))
    );
    for removed in ["initialize", "ping", "resources/subscribe"] {
        let reply = client.request(
            removed,
            json!({ "_meta": { version: "2026-07-28", capabilities: {} } }),
        );
        assert_eq!(reply["error"]["code"], -32601, "{removed}: {reply}");
    }
    let discover = client.result("server/discover", json!({}));
    assert_eq!(
        discover["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "opendoc-mcp"
    );
    let missing = client.request(
        "resources/read",
        json!({ "uri": "opendoc://documents/doc_nope/outline" }),
    );
    assert_eq!(missing["error"]["code"], -32602, "SEP-2164: {missing}");
    let malformed = client.request(
        "tools/call",
        json!({ "name": "list_edit_kinds", "arguments": {}, "inputResponses": 5 }),
    );
    assert_eq!(
        malformed["error"]["code"], -32602,
        "SEP-2322: a malformed inputResponses is a protocol error: {malformed}"
    );
    assert_eq!(
        missing["error"]["data"]["uri"],
        "opendoc://documents/doc_nope/outline"
    );
}

#[test]
fn a_client_that_opens_with_initialize_keeps_the_older_lifecycle() {
    // `docs/178` Q12: a 2025-11-25 client never sends `_meta`.
    let base = sandbox("legacy");
    let root = arg(&base.join("root"));
    let mut client = Client::start(&["--root", &root]);
    let init = client.raw(
        "initialize",
        json!({ "protocolVersion": "2025-11-25", "capabilities": {} }),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    let listed = client.raw("tools/list", json!({}));
    assert!(listed["result"]["tools"].is_array(), "{listed}");
    assert_eq!(client.raw("ping", json!({}))["result"], json!({}));
}

/// `docs/182` P1 headless reach: `open_document` opens a PDF through the
/// registry, and an agent reads it and suggests into one of its frames with
/// the same tools it uses on a DOCX. No PDF-specific tool exists.
#[test]
fn an_agent_opens_a_pdf_reads_it_and_suggests_into_a_frame() {
    let base = sandbox("pdf");
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/pdf/synthetic-prose.pdf"
        ),
        base.join("root/prose.pdf"),
    )
    .expect("fixture");
    let root = arg(&base.join("root"));
    let mut client = Client::start(&["--root", &root, "--profile", "suggest"]);
    let opened = client.ok("open_document", json!({ "path": "prose.pdf" }));
    assert_eq!(opened["format"], "application.pdf", "{opened}");
    assert_eq!(opened["pageCount"], 2, "{opened}");
    let doc = opened["document"].clone();

    let hits = client.ok(
        "search",
        json!({ "document": doc, "query": "lazy dog", "surface": "all" }),
    );
    let node = hits["hits"][0]["node"].clone();
    assert!(node.is_string(), "the frame text is searchable: {hits}");
    assert!(
        opened["importSummary"]
            .to_string()
            .contains("pdf.import.font_substituted"),
        "the import's findings reach the agent: {opened}"
    );

    let plan = json!({
        "base_revision": opened["revision"],
        "edits": [{ "op": "replace_text", "target": { "node": node, "exact": "lazy dog" }, "text": "sleepy cat" }],
    });
    client.ok("apply_edits", json!({ "document": doc, "plan": plan }));
    assert_eq!(
        client.ok("list_tracked_changes", json!({ "document": doc }))["total"],
        2,
        "a suggestion inside a PDF frame is a tracked deletion and insertion"
    );
}

/// `docs/182` §18 reach: under the default `read` profile an agent converts a
/// PDF it opened into a NEW flowing document, whose prose is body paragraphs
/// (the fixed layout's is in frames, found only with `surface: all`), and
/// both handles stay open.
#[test]
fn a_read_only_agent_converts_a_pdf_into_a_new_flowing_document() {
    let base = sandbox("pdf-flowing");
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/pdf/synthetic-prose.pdf"
        ),
        base.join("root/prose.pdf"),
    )
    .expect("fixture");
    let root = arg(&base.join("root"));
    let mut client = Client::start(&["--root", &root]);
    assert!(
        tool_names(&mut client).contains(&"convert_pdf_to_flowing".to_owned()),
        "offered under read"
    );
    let pdf = client.ok("open_document", json!({ "path": "prose.pdf" }))["document"].clone();
    let flowing = client.ok("convert_pdf_to_flowing", json!({ "document": pdf }));
    let new = flowing["document"].clone();
    assert!(new.is_string() && new != pdf, "a new handle: {flowing}");
    assert_eq!(flowing["convertedFrom"], pdf);
    assert_eq!(flowing["name"], "prose.pdf (flowing)");
    // The summary lists what was lost or degraded; a reflow's inferences
    // (`paragraph_boundary_inferred`) are `Mapped` and stay out of it, as every
    // import's mapped findings do. All-prose input loses nothing.
    assert_eq!(
        flowing["importSummary"]["findings"],
        json!([]),
        "nothing in all-prose input is lost: {flowing}"
    );
    let hits = client.ok(
        "search",
        json!({ "document": new, "query": "lazy dog", "surface": "body" }),
    );
    assert!(
        hits["hits"][0]["node"].is_string(),
        "the prose is body text: {hits}"
    );
    client.ok("get_outline", json!({ "document": pdf }));
    client.ok("get_outline", json!({ "document": new }));
    assert_eq!(
        client.refused("convert_pdf_to_flowing", json!({ "document": new }))["code"],
        "not-a-pdf",
        "a conversion does not convert again"
    );
}

/// A one-page image-only PDF: a white 8 x 8 grey picture over the whole
/// page, uncompressed. A scan with nothing native on it.
fn blank_scan() -> Vec<u8> {
    let objects: [Vec<u8>; 5] = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>".to_vec(),
        [
            b"<< /Type /XObject /Subtype /Image /Width 8 /Height 8 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 64 >>\nstream\n".as_slice(),
            &[255u8; 64],
            b"\nendstream",
        ]
        .concat(),
        {
            let content = b"q 612 0 0 792 0 0 cm /Im0 Do Q";
            [
                format!("<< /Length {} >>\nstream\n", content.len()).as_bytes(),
                content,
                b"\nendstream",
            ]
            .concat()
        },
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// `--ocr-models` (`docs/178` §5.6, `docs/182` §20a): the directory must be
/// inside a `--root`; a missing or unloadable one leaves OCR off and a scan
/// reports `pdf.import.ocr_unavailable`. Nothing is fetched. The real models
/// are exercised by `casual-doc-pdf-import`'s model-gated tests.
#[test]
fn ocr_models_must_be_inside_the_roots_and_missing_ones_report_ocr_unavailable() {
    let base = sandbox("ocr-models");
    std::fs::write(base.join("root/scan.pdf"), blank_scan()).expect("fixture");
    std::fs::create_dir_all(base.join("outside/models/paddle")).expect("mkdir");
    let root = arg(&base.join("root"));

    // Outside every root: the server refuses to start.
    let outside = arg(&base.join("outside/models"));
    let output = Command::new(env!("CARGO_BIN_EXE_opendoc-mcp"))
        .args(["--root", &root, "--ocr-models", &outside])
        .stdin(Stdio::null())
        .output()
        .expect("runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("outside every --root"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Missing, then present but not loadable (bytes that are no model): the
    // server runs without OCR and says so in the import summary.
    std::fs::create_dir_all(base.join("root/models/paddle")).expect("mkdir");
    for (file, bytes) in [
        ("root/models/paddle/det.onnx", "not a model"),
        ("root/models/paddle/rec.onnx", "not a model"),
        ("root/models/paddle/rec.dict.txt", "a\n"),
    ] {
        std::fs::write(base.join(file), bytes).expect("fixture");
    }
    for dir in [
        arg(&base.join("root/absent")),
        arg(&base.join("root/models")),
    ] {
        let mut client = Client::start(&["--root", &root, "--ocr-models", &dir]);
        let opened = client.ok("open_document", json!({ "path": "scan.pdf" }));
        assert!(
            opened["importSummary"]
                .to_string()
                .contains("pdf.import.ocr_unavailable"),
            "{dir}: {opened}"
        );
        assert_eq!(opened["ocr"]["provider"], Value::Null, "{dir}: {opened}");
    }
}
