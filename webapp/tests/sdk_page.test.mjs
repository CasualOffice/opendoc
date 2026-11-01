// `webapp/sdk.page.html` — the guard that keeps the SDK page honest.
//
// The page is held to the rule `embedding_page.test.mjs` holds the embedding
// guide to (`docs/126`, SKILL §9): examples extracted from code a test runs,
// numbers generated from a committed artifact, nothing typed and trusted. So this
// checks the ways the page could be false:
//
//   1. It drifted from its sources: `build-sdk-page.mjs --check` answers that.
//   2. A code panel was typed rather than copied: each panel's body must appear
//      VERBATIM in the file its caption names, and that file must be one a test
//      in this repository runs.
//   3. Its numbers are wrong: the benchmark ratios, the count of losses and the
//      format counts are RE-DERIVED here from the artifacts, independently of the
//      generator, because a guard that re-runs the code under test agrees with
//      its bugs.
//   4. The familiarity table names an API the binding does not have.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const WEBAPP = dirname(dirname(fileURLToPath(import.meta.url)));
const REPO = dirname(WEBAPP);
const read = (path) => readFileSync(join(REPO, path), "utf8");
const page = read("webapp/sdk.page.html");
const built = read("webapp/sdk.html");

const unescape = (html) =>
  html.replaceAll("&lt;", "<").replaceAll("&gt;", ">").replaceAll("&amp;", "&");

function claim(name) {
  const values = [...page.matchAll(new RegExp(`data-claim="${name}"[^>]*>([^<]*)<`, "g"))].map((m) =>
    unescape(m[1].trim()),
  );
  assert.equal(values.length, 1, `the page must carry data-claim="${name}" once`);
  return values[0];
}

function region(name) {
  const open = `<!-- @generated ${name} -->`;
  const from = built.indexOf(open);
  const to = built.indexOf(`<!-- @end ${name} -->`);
  assert.ok(from >= 0 && to > from, `the built page must carry the "${name}" region`);
  return built.slice(from + open.length, to);
}

test("the page is what its generator produces from today's sources", () => {
  execFileSync(process.execPath, [join(WEBAPP, "tools", "build-sdk-page.mjs"), "--check"], {
    stdio: "pipe",
  });
});

test("every code panel is copied verbatim from a file a test runs", () => {
  const panels = [
    ...built.matchAll(
      /<div class="code-panel-head"><span>([^<]+)<\/span> — <span[^>]*>([^<]*)<\/span><\/div>\s*<pre><code>([\s\S]*?)<\/code><\/pre>/g,
    ),
  ].map(([, path, clause, code]) => ({ path, clause, code: unescape(code) }));
  assert.ok(panels.length >= 9, `the page must carry its install and example panels, found ${panels.length}`);
  for (const { path, clause, code } of panels) {
    let source = read(path);
    // The CLI panel is the `USAGE` constant as the binary prints it, so the
    // source's Rust escapes are undone before comparing.
    if (path.endsWith(".rs") && path.startsWith("tools/opendoc-cli")) source = source.replaceAll('\\"', '"');
    assert.ok(source.includes(code), `the panel captioned ${path} is not verbatim in that file`);
    // "run by X": X must exist and mention the example by name, so the chain from
    // the panel to the test that executes it is checkable, not asserted.
    const runner = clause.match(/^run by (\S+)$/)?.[1];
    if (runner) {
      const name = path.slice(path.lastIndexOf("/") + 1).replace(/\.(rs|py|java|mjs)$/, "");
      assert.ok(read(runner).includes(name), `${runner} does not run ${path}`);
    }
  }
  // Every example panel names a runner; only the install blocks and the CLI's
  // usage text are allowed not to.
  const examples = panels.filter((p) => /examples?\//.test(p.path));
  assert.ok(examples.length >= 5, "every language's example must be on the page");
  for (const p of examples) assert.match(p.clause, /^run by /, `${p.path} names no test that runs it`);
});

test("the benchmark table and its losses are what results.json says", () => {
  const results = JSON.parse(read("benchmarks/sdk/results.json"));
  const env = results.environment;
  assert.equal(claim("bench-iterations"), String(env.iterations));
  assert.equal(claim("bench-node"), env.node);
  assert.ok(claim("bench-machine").includes(env.cpu) && claim("bench-machine").includes(env.platform));
  assert.equal(claim("bench-row-count"), String(results.speed.length));

  // Re-derived: for each row, OpenDoc's time and the competitor's (whichever
  // non-OpenDoc library the row measured), and the ratio between them.
  const table = region("sdk-bench");
  const rows = [...table.matchAll(/<tr>([\s\S]*?)<\/tr>/g)].slice(1);
  assert.equal(rows.length, results.speed.length);
  let losses = 0;
  results.speed.forEach((row, i) => {
    const [competitor] = Object.keys(row).filter((k) => !["task", "input", "opendoc"].includes(k));
    const theirs = row[competitor];
    const cells = rows[i][1];
    assert.ok(cells.includes(`${row.opendoc.ms} ms`), `row ${i}: OpenDoc's time`);
    if (theirs?.ms && row.opendoc?.ms) {
      const faster = theirs.ms >= row.opendoc.ms;
      const factor = (faster ? theirs.ms / row.opendoc.ms : row.opendoc.ms / theirs.ms).toFixed(1);
      assert.ok(cells.includes(`${factor}× ${faster ? "faster" : "slower"}`), `row ${i}: the ratio`);
      if (!faster) losses += 1;
    }
    if (theirs?.peakMB != null && row.opendoc.peakMB > theirs.peakMB) losses += 1;
  });
  assert.equal(claim("bench-loss-count"), String(losses));
  const listed = [...region("sdk-bench-losses").matchAll(/<li>/g)].length;
  assert.equal(listed, losses, "every loss is listed, and nothing else");
  // The losses the owner named are on the page, in so many words.
  const lossText = region("sdk-bench-losses");
  for (const named of ["docxtemplater", "poi", "MB against"]) {
    assert.ok(lossText.includes(named), `the losses must show ${named}`);
  }
});

test("the format counts are what the registry's descriptors declare", () => {
  // Independent of `build-readme.mjs`'s parser: count the flags directly.
  const dir = join(REPO, "crates", "casual-doc-io", "src");
  const files = [];
  const walk = (d) => {
    for (const entry of readdirSync(d)) {
      const path = join(d, entry);
      if (statSync(path).isDirectory()) walk(path);
      else if (entry.endsWith(".rs") && !entry.endsWith("tests.rs") && entry !== "tests.rs") files.push(path);
    }
  };
  walk(dir);
  let opens = 0;
  let saves = 0;
  for (const file of files) {
    for (const [, body] of readFileSync(file, "utf8").matchAll(/FormatDescriptor \{([\s\S]*?)\n\s*\}/g)) {
      const id = body.match(/formats::([A-Z_]+)/)?.[1];
      if (!id) continue;
      // PDF's descriptor says `can_import: false`; the page counts it apart.
      if (/can_import: true/.test(body) && id !== "PDF") opens += 1;
      if (/can_export: true/.test(body)) saves += 1;
    }
  }
  assert.equal(claim("format-open-count"), String(opens));
  assert.equal(claim("format-save-count"), String(saves));
});

test("every API the familiarity table offers exists in the file its row names", () => {
  const rows = [...page.matchAll(/<td data-source="([^"]+)">([\s\S]*?)<\/td>/g)];
  assert.ok(rows.length >= 5, "docx-js, docxtemplater, mammoth, python-docx and POI each have a row");
  for (const [, source, cell] of rows) {
    const full = join(REPO, source);
    const text = statSync(full).isDirectory()
      ? readdirSync(full).map((name) => readFileSync(join(full, name), "utf8")).join("\n")
      : readFileSync(full, "utf8");
    for (const [, code] of cell.matchAll(/<code>([^<]+)<\/code>/g)) {
      const name = code.split(".").pop();
      assert.match(text, new RegExp(`\\b${name}\\b`), `${code} is not in ${source}`);
    }
  }
});

test("every section is reachable from the page's own contents list", () => {
  const sections = [...page.matchAll(/<h2 id="([^"]+)"/g)].map((m) => m[1]);
  const toc = page.slice(page.indexOf('class="doc-toc-list"'));
  const listed = [...toc.slice(0, toc.indexOf("</ul>")).matchAll(/href="#([^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual(listed.filter((id) => sections.includes(id)), sections);
});
