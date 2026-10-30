// SPDX-License-Identifier: Apache-2.0

//! The `opendoc` binary, run as a user runs it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../sample.docx");

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("opendoc-cli-{}-{name}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn opendoc(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_opendoc"))
        .args(args)
        .output()
        .expect("runs")
}

fn ok(args: &[&str]) -> String {
    let out = opendoc(args);
    assert!(
        out.status.success(),
        "opendoc {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn convert_writes_each_format_and_a_report() {
    let dir = scratch("convert");
    for ext in ["pdf", "odt", "txt", "html", "md", "json"] {
        let out = dir.join(format!("sample.{ext}"));
        let said = ok(&["convert", SAMPLE, s(&out)]);
        assert!(said.contains(&format!("({ext}, semantic)")), "{said}");
        assert!(std::fs::metadata(&out).unwrap().len() > 0);
    }
    let pdf = std::fs::read(dir.join("sample.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    let report = dir.join("report.json");
    ok(&[
        "convert",
        SAMPLE,
        s(&dir.join("same.docx")),
        "--mode",
        "exact",
        "--report",
        s(&report),
    ]);
    assert_eq!(
        std::fs::read(dir.join("same.docx")).unwrap(),
        std::fs::read(SAMPLE).unwrap()
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert_eq!(report["mode"], "exact");
    assert_eq!(report["schema"], 1);
}

#[test]
fn render_writes_deterministic_pngs_for_the_pages_asked() {
    let a = scratch("render-a");
    let b = scratch("render-b");
    let listed = ok(&[
        "render",
        SAMPLE,
        "--out",
        s(&a),
        "--pages",
        "1-2",
        "--dpi",
        "48",
    ]);
    assert_eq!(listed.lines().count(), 2);
    ok(&[
        "render",
        SAMPLE,
        "--out",
        s(&b),
        "--pages",
        "1-2",
        "--dpi",
        "48",
    ]);
    for page in ["sample-1.png", "sample-2.png"] {
        let first = std::fs::read(a.join(page)).unwrap();
        assert!(first.starts_with(b"\x89PNG"));
        assert_eq!(
            first,
            std::fs::read(b.join(page)).unwrap(),
            "{page} differs between runs"
        );
    }
    ok(&["render", SAMPLE, "--out", s(&a), "--format", "pdf"]);
    assert!(
        std::fs::read(a.join("sample.pdf"))
            .unwrap()
            .starts_with(b"%PDF-")
    );
    let past = opendoc(&["render", SAMPLE, "--out", s(&a), "--pages", "999"]);
    assert_eq!(past.status.code(), Some(2));
}

#[test]
fn info_reports_structure_as_text_and_json() {
    let text = ok(&["info", SAMPLE]);
    assert!(text.contains("format      docx"), "{text}");
    let json: serde_json::Value = serde_json::from_str(&ok(&["info", SAMPLE, "--json"])).unwrap();
    assert_eq!(json["format"], "docx");
    assert!(json["pages"].as_u64().unwrap() > 1);
    assert!(json["paragraphs"].as_u64().unwrap() > 0);
}

#[test]
fn build_template_and_diff_work_end_to_end() {
    let dir = scratch("build");
    let spec = dir.join("spec.json");
    std::fs::write(
        &spec,
        r#"{"sections":[{"children":[
            {"type":"paragraph","text":"Dear {name},"},
            {"type":"paragraph","text":"{#items}"},
            {"type":"paragraph","text":"- {.}"},
            {"type":"paragraph","text":"{/items}"}]}]}"#,
    )
    .unwrap();
    let template = dir.join("template.docx");
    ok(&["build", s(&spec), s(&template)]);
    let data = dir.join("data.json");
    std::fs::write(&data, r#"{"name":"Ada","items":["one","two"]}"#).unwrap();
    let filled = dir.join("filled.docx");
    ok(&["template", s(&template), s(&data), s(&filled)]);
    let filled_txt = dir.join("filled.txt");
    ok(&["convert", s(&filled), s(&filled_txt)]);
    let text = std::fs::read_to_string(&filled_txt).unwrap();
    assert!(
        text.contains("Dear Ada,") && text.contains("- one") && text.contains("- two"),
        "{text}"
    );
    let changes = ok(&["diff", s(&template), s(&filled)]);
    assert!(
        changes.lines().next().unwrap().contains("change(s)"),
        "{changes}"
    );
    assert!(changes.contains("\"Ada\""), "{changes}");
    let json: serde_json::Value =
        serde_json::from_str(&ok(&["diff", s(&template), s(&filled), "--json"])).unwrap();
    assert!(!json["changes"].as_array().unwrap().is_empty());
    // A template error is a document error (exit 1) naming the code.
    std::fs::write(&data, "{}").unwrap();
    let strict = opendoc(&["template", s(&template), s(&data), s(&filled), "--strict"]);
    assert_eq!(strict.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&strict.stderr).contains("template:"));
}

#[test]
fn usage_errors_exit_2_and_document_errors_exit_1() {
    assert_eq!(opendoc(&[]).status.code(), Some(2));
    assert_eq!(opendoc(&["nope"]).status.code(), Some(2));
    assert_eq!(opendoc(&["info", SAMPLE, "--bogus"]).status.code(), Some(2));
    let dir = scratch("errors");
    let junk = dir.join("junk.docx");
    std::fs::write(&junk, b"PK\x03\x04\xff\xfe\x00 truncated zip\x00\xff").unwrap();
    let failed = opendoc(&["info", s(&junk)]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("open:"));
    assert!(ok(&["formats"]).contains("pdf"));
}
