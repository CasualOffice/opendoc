// SPDX-License-Identifier: Apache-2.0

//! The MCP conformance guard of `docs/178` §8: the real binary, spoken to over
//! stdio by a scripted client. It lists the tools, calls every one, and holds
//! the profile filter, the edit grant and the path allowlist.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

/// The 20 tools at the widest profile, in `tools/list` order (`docs/178` §6.2).
const ALL_TOOLS: &[&str] = &[
    "open_document",
    "close_document",
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

    fn request(&mut self, method: &str, params: Value) -> Value {
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
    assert_eq!(names.len(), 19);
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
