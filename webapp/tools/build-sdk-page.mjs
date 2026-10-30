#!/usr/bin/env node
// Generates the code panels, tables and numbers on `webapp/sdk.page.html`, the
// Native SDK's page, from the files they document.
//
// The same contract as `build-embed-docs.mjs` (`docs/126`, SKILL §9): every code
// panel is EXTRACTED from a committed file that a test runs, and its caption names
// that file; every number is generated from a committed artifact
// (`benchmarks/sdk/results.json`, `crates/opendoc/src/format.rs`); everything else
// is prose a person wrote and this tool never touches. `--check` fails when the
// committed page is not what a fresh run produces, so editing the page by hand and
// changing a source without regenerating are both build failures.
//
// A separate tool rather than a third page in `build-embed-docs.mjs`, because that
// file is the embed product's (its sources are the embed package and the host
// contract) and is near two thousand lines. The helpers below are the same shapes,
// on purpose: one panel markup, one table markup, one seam.
//
//   node webapp/tools/build-sdk-page.mjs           rewrite the page
//   node webapp/tools/build-sdk-page.mjs --check   fail if it is stale
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

import { formatRows as readmeFormatRows } from "./build-readme.mjs";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const REPO = join(WEBAPP, "..");
export const PAGE = join(WEBAPP, "sdk.page.html");

const read = (path) => readFileSync(join(REPO, path), "utf8");
const escape = (text) =>
  String(text).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
const said = (key, text) => `<span data-i18n="${key}">${escape(text)}</span>`;

/** Every file the page quotes or measures, by role. The examples are each run by
 *  a test (named in `sdk_page.test.mjs`), which is what makes quoting them honest. */
export const SOURCES = Object.freeze({
  jsReadme: "packages/opendoc/README.md",
  pyReadme: "bindings/python/README.md",
  javaReadme: "bindings/java/README.md",
  jsExample: "packages/opendoc/examples/read-modify-convert.mjs",
  docxExample: "packages/opendoc/examples/build-report.mjs",
  pyExample: "bindings/python/examples/quickstart.py",
  javaExample: "bindings/java/src/examples/java/org/casualoffice/opendoc/examples/Quickstart.java",
  rustExample: "crates/opendoc/examples/quickstart.rs",
  cli: "tools/opendoc-cli/src/main.rs",
  formats: "crates/opendoc/src/format.rs",
  registry: "crates/casual-doc-io/src",
  results: "benchmarks/sdk/results.json",
});

// ── Extraction ─────────────────────────────────────────────────────────────

/** The nth fenced block of a language in a Markdown file, verbatim. */
function fencedBlock(path, language, index = 0) {
  const blocks = [...read(path).matchAll(new RegExp("```" + language + "\\n([\\s\\S]*?)```", "g"))];
  if (!blocks[index]) throw new Error(`build-sdk-page: no \`\`\`${language} block #${index} in ${path}`);
  return blocks[index][1].replace(/\n+$/, "");
}

/** A whole file, trailing newlines trimmed. */
const wholeFile = (path) => read(path).replace(/\n+$/, "");

/** The CLI's `USAGE` constant, as the binary prints it (Rust's `\"` unescaped). */
export function cliUsage() {
  const match = read(SOURCES.cli).match(/const USAGE: &str = "([\s\S]*?)";\n/);
  if (!match) throw new Error(`build-sdk-page: no USAGE constant in ${SOURCES.cli}`);
  return match[1].replaceAll('\\"', '"');
}

/** Each format and what this SDK does with it, from the `FormatDescriptor` each
 *  `casual-doc-io` adapter declares: the registry `crates/opendoc` opens and saves
 *  through, read by the same parser the README format tables use. PDF import is a
 *  separate registration the CLI's build compiles (feature `pdf-import`), so its
 *  row says so rather than "yes". */
export function formatRows() {
  return readmeFormatRows().map((row) => ({
    id: row.id,
    name: row.name,
    exts: row.exts,
    opens: row.id === "PDF" ? "pdf-import" : row.canImport ? "yes" : "no",
    saves: row.canExport,
    whole: wholeFormats().has(row.id),
  }));
}

/** The formats a save carries whole (`Format::carries_document`): DOCX, DOTX, ODT
 *  and the JSON snapshot. The others are renditions, whose save report says
 *  `lossyByNature`. */
function wholeFormats() {
  const source = read(SOURCES.formats);
  const body = source.slice(source.indexOf("pub const fn carries_document"));
  const line = body.slice(0, body.indexOf("\n    }")).match(/matches!\(self, ([^)]*)\)/);
  if (!line) throw new Error(`build-sdk-page: Format::carries_document is not where it was in ${SOURCES.formats}`);
  const ids = { Docx: "DOCX", Dotx: "DOTX", Odt: "ODT", Json: "NORMALIZED_JSON", Txt: "TEXT", Html: "HTML", Markdown: "MARKDOWN", Rtf: "RTF", Pdf: "PDF" };
  return new Set([...line[1].matchAll(/Self::(\w+)/g)].map((m) => ids[m[1]] ?? m[1]));
}

// ── Markup ─────────────────────────────────────────────────────────────────

function codePanel(path, key, clause, code) {
  return [
    '<div class="code-panel">',
    `  <div class="code-panel-head"><span>${escape(path)}</span> — ${said(key, clause)}</div>`,
    `  <pre><code>${escape(code)}</code></pre>`,
    "</div>",
  ].join("\n");
}

/** A `doc-table` inside its scroller, the reference pages' table: it scrolls
 *  sideways in its own box on a phone and the page never does. Headings are
 *  [key, English]; cells are HTML. */
function docTable(head, rows) {
  return [
    '<div class="doc-table-scroll">',
    '  <table class="doc-table">',
    "    <thead>",
    "      <tr>",
    ...head.map(([key, text]) => `        <th data-i18n="${key}">${escape(text)}</th>`),
    "      </tr>",
    "    </thead>",
    "    <tbody>",
    ...rows.map((row) => ["      <tr>", ...row.map((cell) => `        <td>${cell}</td>`), "      </tr>"].join("\n")),
    "    </tbody>",
    "  </table>",
    "</div>",
  ].join("\n");
}

// ── The benchmark ──────────────────────────────────────────────────────────

/** The competitor each task is measured against, and what the task is. The
 *  task names are prose, so they go through the seam; the library names are
 *  proper nouns and do not. `benchmarks/sdk/report.mjs` holds the same mapping
 *  for docs/176; a task present in results.json and absent here fails the run. */
const TASKS = Object.freeze({
  generate: ["docx-js", "site.sdk.task.generate", "Generate a DOCX (the same docx-js program)"],
  template: ["docxtemplater", "site.sdk.task.template", "Fill a template (the same template and data)"],
  html: ["mammoth", "site.sdk.task.html", "DOCX to HTML"],
  text: ["mammoth", "site.sdk.task.text", "DOCX to plain text"],
  pdf: ["libreoffice", "site.sdk.task.pdf", "DOCX to PDF"],
  "py-generate": ["python-docx", "site.sdk.task.pyGenerate", "Generate a DOCX (the same python-docx program)"],
  "py-read": ["python-docx", "site.sdk.task.pyRead", "Open and read every paragraph (Python)"],
  "java-generate": ["poi", "site.sdk.task.javaGenerate", "Generate a DOCX (POI-style calls, Java)"],
  "java-read": ["poi", "site.sdk.task.javaRead", "Open and read every paragraph (Java)"],
});

const ms = (cell) => (cell && !cell.error && cell.ms != null ? `${cell.ms} ms` : "—");
const mb = (cell) => (cell && !cell.error && cell.peakMB != null ? `${cell.peakMB} MB` : "—");

/** "1.3× faster" / "1.7× slower", or null when either side is missing. */
export function ratio(ours, theirs) {
  if (!ours?.ms || !theirs?.ms) return null;
  const r = theirs.ms / ours.ms;
  return r >= 1 ? `${r.toFixed(1)}× faster` : `${(1 / r).toFixed(1)}× slower`;
}

function results() {
  return JSON.parse(read(SOURCES.results));
}

function benchRows() {
  return results().speed.map((row) => {
    const task = TASKS[row.task];
    if (!task) throw new Error(`build-sdk-page: results.json has a task "${row.task}" with no description`);
    const [competitor, key, text] = task;
    const theirs = row[competitor];
    return { row, competitor, key, text, theirs, ratio: ratio(row.opendoc, theirs) };
  });
}

function benchTable() {
  return docTable(
    [
      ["site.sdk.column.task", "Task"],
      ["site.sdk.column.input", "Input"],
      ["site.sdk.column.opendoc", "OpenDoc"],
      ["site.sdk.column.competitor", "Competitor"],
      ["site.sdk.column.speed", "OpenDoc speed"],
      ["site.sdk.column.opendocMemory", "OpenDoc peak memory"],
      ["site.sdk.column.competitorMemory", "Competitor peak memory"],
    ],
    benchRows().map(({ row, competitor, key, text, theirs, ratio: r }) => [
      said(key, text),
      escape(row.input),
      escape(ms(row.opendoc)),
      `<code>${escape(competitor)}</code> ${escape(ms(theirs))}`,
      r ? `<b>${escape(r)}</b>` : "—",
      escape(mb(row.opendoc)),
      escape(mb(theirs)),
    ]),
  );
}

/** Every row OpenDoc loses, in speed or in memory, generated so a loss cannot be
 *  left out of the page by a person choosing which rows to talk about. */
export function losses() {
  const out = [];
  for (const { row, competitor, key, text, theirs, ratio: r } of benchRows()) {
    if (r && r.endsWith("slower")) {
      out.push({ kind: "speed", key, text, input: row.input, competitor, value: r });
    }
    if (row.opendoc?.peakMB != null && theirs?.peakMB != null && row.opendoc.peakMB > theirs.peakMB) {
      out.push({
        kind: "memory",
        key,
        text,
        input: row.input,
        competitor,
        value: `${row.opendoc.peakMB} MB against ${theirs.peakMB} MB`,
      });
    }
  }
  return out;
}

function lossList() {
  const items = losses().map((loss) => {
    const what =
      loss.kind === "speed"
        ? said("site.sdk.loss.slower", "Slower:")
        : said("site.sdk.loss.memory", "More memory:");
    return `  <li>${what} ${said(loss.key, loss.text)}, ${escape(loss.input)}, <code>${escape(
      loss.competitor,
    )}</code> — ${escape(loss.value)}</li>`;
  });
  return ['<ul class="doc-body">', ...items, "</ul>"].join("\n");
}

// ── Formats ────────────────────────────────────────────────────────────────

function formatTable() {
  const answer = (value) =>
    value === "yes"
      ? said("site.sdk.format.yes", "Yes")
      : value === "no"
        ? said("site.sdk.format.no", "No")
        : said("site.sdk.format.pdfImport", "With the pdf-import feature (the CLI has it)");
  return docTable(
    [
      ["site.sdk.column.format", "Format"],
      ["site.sdk.column.open", "Open"],
      ["site.sdk.column.save", "Save"],
      ["site.sdk.column.wholeDocument", "Carries the whole document"],
    ],
    formatRows().map((f) => [
      `${escape(f.name)} ${f.exts.map((e) => `<code>.${escape(e)}</code>`).join(" ")}`,
      answer(f.opens),
      answer(f.saves ? "yes" : "no"),
      f.whole
        ? said("site.sdk.format.yes", "Yes")
        : f.saves
          ? said("site.sdk.format.rendition", "No: a rendition, and its save report says lossyByNature")
          : "—",
    ]),
  );
}

// ── The page ───────────────────────────────────────────────────────────────

function regions() {
  return {
    "sdk-install-js": codePanel(SOURCES.jsReadme, "site.sdk.panel.installJs", "build and pack it from a checkout", fencedBlock(SOURCES.jsReadme, "sh")),
    "sdk-install-python": codePanel(SOURCES.pyReadme, "site.sdk.panel.installPython", "build the wheel locally", fencedBlock(SOURCES.pyReadme, "sh")),
    "sdk-install-java": codePanel(SOURCES.javaReadme, "site.sdk.panel.installJava", "build the jar and its native library", fencedBlock(SOURCES.javaReadme, "sh")),
    "sdk-example-js": codePanel(SOURCES.jsExample, "site.sdk.panel.exampleJs", "run by packages/opendoc/test/examples.test.mjs", wholeFile(SOURCES.jsExample)),
    "sdk-example-docx": codePanel(SOURCES.docxExample, "site.sdk.panel.exampleDocx", "run by packages/opendoc/test/examples.test.mjs", wholeFile(SOURCES.docxExample)),
    "sdk-example-python": codePanel(SOURCES.pyExample, "site.sdk.panel.examplePython", "run by bindings/python/tests/test_opendoc.py", wholeFile(SOURCES.pyExample)),
    "sdk-example-java": codePanel(SOURCES.javaExample, "site.sdk.panel.exampleJava", "run by bindings/java/build.sh", wholeFile(SOURCES.javaExample)),
    "sdk-example-rust": codePanel(SOURCES.rustExample, "site.sdk.panel.exampleRust", "run by crates/opendoc/tests/sdk.rs", wholeFile(SOURCES.rustExample)),
    "sdk-example-cli": codePanel(SOURCES.cli, "site.sdk.panel.exampleCli", "the usage the binary prints", cliUsage()),
    "sdk-bench": benchTable(),
    "sdk-bench-losses": lossList(),
    "sdk-formats": formatTable(),
  };
}

function claims() {
  const r = results();
  const env = r.environment;
  const formats = formatRows();
  return {
    "bench-machine": `${env.cpu} (${env.cores} cores, ${env.memoryGB} GB), ${env.platform}`,
    "bench-node": env.node,
    "bench-iterations": String(env.iterations),
    "bench-row-count": String(r.speed.length),
    "bench-loss-count": String(losses().length),
    "format-open-count": String(formats.filter((f) => f.opens === "yes").length),
    "format-save-count": String(formats.filter((f) => f.saves).length),
  };
}

function indentHtml(block, indent) {
  let inPre = false;
  return block
    .split("\n")
    .map((line) => {
      const opens = line.includes("<pre") && !line.includes("</pre>");
      const closes = inPre && line.includes("</pre>");
      const indented = inPre || !line ? line : indent + line;
      if (opens) inPre = true;
      if (closes) inPre = false;
      return indented;
    })
    .join("\n");
}

export function render(page) {
  const generated = regions();
  const values = claims();
  const seen = new Set();
  let out = page.replace(
    /^([ \t]*)<!-- @generated ([\w-]+) -->\n[\s\S]*?^[ \t]*<!-- @end \2 -->/gm,
    (_whole, indent, name) => {
      if (generated[name] === undefined) throw new Error(`build-sdk-page: unknown region "${name}"`);
      seen.add(`region ${name}`);
      return `${indent}<!-- @generated ${name} -->\n${indentHtml(generated[name], indent)}\n${indent}<!-- @end ${name} -->`;
    },
  );
  out = out.replace(/(data-claim="([\w-]+)"[^>]*>)([^<]*)(<)/g, (_whole, open, name, _old, close) => {
    if (values[name] === undefined) throw new Error(`build-sdk-page: the page claims "${name}", which this tool cannot derive`);
    seen.add(`claim ${name}`);
    return `${open}${escape(values[name])}${close}`;
  });
  const unused = [
    ...Object.keys(generated).map((n) => `region ${n}`),
    ...Object.keys(values).map((n) => `claim ${n}`),
  ].filter((n) => !seen.has(n));
  if (unused.length) throw new Error(`build-sdk-page: generated but never used on the page: ${unused.join(", ")}`);
  return out;
}

if (process.argv[1] && relative(process.argv[1], fileURLToPath(import.meta.url)) === "") {
  const current = readFileSync(PAGE, "utf8");
  const fresh = render(current);
  if (process.argv.includes("--check")) {
    if (fresh !== current) {
      console.error(
        "build-sdk-page --check: webapp/sdk.page.html is stale.\n" +
          "Run `node webapp/tools/build-sdk-page.mjs`, then `webapp/build-site.py`, and commit both.",
      );
      process.exit(1);
    }
    console.log("build-sdk-page --check: webapp/sdk.page.html up to date.");
  } else if (fresh !== current) {
    writeFileSync(PAGE, fresh);
    console.log("build-sdk-page: wrote webapp/sdk.page.html");
  } else {
    console.log("build-sdk-page: webapp/sdk.page.html was already current.");
  }
}
