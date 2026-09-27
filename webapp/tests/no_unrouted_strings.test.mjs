// "Nothing should be left" — the gate that makes it survive the next commit
// (docs/124 §4, `109` HF-081).
//
// The owner asked for eighteen languages and for nothing to be left out. The
// first half is a list of files. The second half cannot be delivered by
// translating what exists today, because tomorrow somebody adds an English
// literal to a dialog and the product is partly untranslated again. So it is
// delivered the way `main.js`'s line count is: a RATCHET. Every unrouted
// user-facing string in the tree is counted, per file, and the count may never
// rise. Routing a string through `t()` or a `data-i18n` attribute lowers it.
//
// Lowering a ceiling is the point of the exercise. Raising one is refused here
// rather than in review.
import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";

const { codeBlocks, scanMarkup, scanScript, scanTree, totalSites, unroutableStrings } =
  await import("../tools/string_sites.mjs");

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");

/** Every number is a debt, and the only legal direction is down.
 *
 *  1,323 when the seam landed on 2026-09-24. 473 on 2026-09-25, when the
 *  markup went through it: `editor.html` fell from 853 to 8, which is the
 *  whole of the chrome routed in one pass. What is left is almost all
 *  `main.js` — strings a script builds, where the English has to move into
 *  `en_strings.mjs` one surface at a time. */
const CEILINGS = new Map([
  ["editor.html", 16],
  // ---- The SITE (`109` HF-190) --------------------------------------------
  // These are not new debt. They are debt that was invisible: `scanTree` read
  // `editor.html` and `src/*.{js,mjs}` and nothing else, so every `*.page.html`
  // and every shared partial sat outside the gate entirely — about five hundred
  // unrouted strings, against an `editor.html` ceiling of sixteen, with no number
  // anywhere that would have said so.
  //
  // MEASURED, each one, by running this scanner over the file. Nothing here is
  // arithmetic: a report handed over approximate figures (embedding ~202, index
  // ~159, fidelity ~84, docs ~56, ~501 site-wide) and every one of them is off by
  // a little, which is exactly why the rule in this file is that a ceiling is a
  // measurement and never a calculation. The real numbers are below; part of the
  // gap is the `<pre><code>` exemption landing with them.
  //
  // No translation work ships with these. The point is that the debt becomes a
  // number that can only go down. Note that the site is not localised in any other
  // sense either — no per-language pages, no `hreflang` — so routing these strings
  // is the first half of a larger piece of work, not a loose end.
  ["docs.page.html", 54],
  ["embedding.page.html", 204],
  ["fidelity.page.html", 84],
  ["index.page.html", 157],
  // The shared header and footer, counted where they are AUTHORED. The generated
  // `*.html` pages inline them, so counting those would charge the same fourteen
  // strings once per page and make one edit to a partial move four ceilings.
  ["_partials/site-footer.html", 4],
  ["_partials/site-header.html", 10],
  // ---- The editor's scripts -------------------------------------------------
  ["src/a11y_mirror.mjs", 1],
  ["src/blank_document.mjs", 8],
  ["src/bookmark_manager.mjs", 6],
  ["src/command_menu.mjs", 6],
  ["src/command_taxonomy.mjs", 8],
  ["src/compact_toolbar.mjs", 18],
  ["src/export_commands.mjs", 6],
  // The six field-kind labels and their picker notes ("Page number", "Today's
  // date", …). They were six of `main.js`'s 348 and moved here with the table,
  // so this is the same debt relocated, not a new one — main.js came down by
  // exactly six in the same commit.
  ["src/field_kinds.mjs", 6],
  ["src/fidelity.js", 5],
  ["src/file_pane.mjs", 12],
  ["src/format_io.mjs", 6],
  ["src/home-embed.js", 4],
  ["src/keyboard.mjs", 7],
  // 342 → 338 in `109` UX-005: the table-style chooser's "No table style" became
  // one `NO_TABLE_STYLE` constant serving both the chooser row and the new palette
  // row, so one meaning stopped being two literals.
  //
  // 338 → the number below in OO-005 (captions and cross-references). Out: the
  // object right-click menu's nine labels, which moved with the menu itself (next
  // entry); the Outline panel's empty sentence and the cross-reference command's
  // "not possible yet" reason, both routed now; and the right-click menu's own
  // Increase/Decrease indent rows, which now call the Home band's existing keys —
  // those rows are built on right-click, long after the catalogue is installed, so
  // unlike the surface table they CAN call `t()`. In: the References palette rows'
  // "Insert caption" and "Update caption numbers", because that table's labels are
  // read at import, before a catalogue exists, so English there is the same
  // deliberate debt every other row in it carries.
  //
  // MEASURED from the rebased file. The first attempt at this number was 325,
  // reached by subtracting both lanes' reclaims from 342 — and this gate rejected
  // it: the real count is 327, because `main` also brought in two draft-recovery
  // template literals (the "Restore …" and "Delete the recovered draft of …"
  // confirmations) that neither branch's arithmetic knew about. That is the merge
  // trap `module_seams.test.mjs` records, caught by the guard rather than by CI,
  // and it is why this number is a measurement and never a calculation.
  //
  // 327 is still a real lowering: `main` had 338 and this branch had 328. The two
  // draft-recovery strings are pre-existing debt that became visible here, not debt
  // this change added, and they are left for whoever owns that dialog.
  // 325 after the section-properties round: the two running-content variant
  // toggles moved into `header_footer_settings.mjs` and took their four English
  // sentences through the catalogue on the way — the two palette switch labels
  // ("Different first page: on") became `headerFooter.*Switch` with a translated
  // on/off, and the failure message and the two status lines became keys. The new
  // module therefore carries ZERO unrouted strings and needs no entry here, which
  // is the direction this table is for. Measured, not calculated.
  ["src/main.js", 325],
  // The object right-click menu's nine row labels ("Wrap text", "Alt text…",
  // "Shape fill", "No fill", "Shape outline", "No outline", "Crop image",
  // "Properties…", "Delete"). They were nine of `main.js`'s 342 and moved here
  // with the menu builder, so this is the same debt in a new place, not a new
  // debt — main.js came down by more than nine in the same commit.
  ["src/object_context_menu.mjs", 9],
  ["src/pages_panel.mjs", 4],
  // The sixteen highlight labels ("Bright green", "Gray 50%", …). They were
  // sixteen of `main.js`'s 364 and moved here with the table, so this is the
  // same debt in a new place, not a new debt — main.js came down by exactly 16.
  // Routing them is a locale-key change across all eighteen languages, which is
  // its own piece of work; the ratchet is what keeps it from being forgotten.
  ["src/palettes.mjs", 16],
  ["src/shortcut_labels.mjs", 2],
  ["src/spell_check.mjs", 6],
]);

/** How far under its ceiling a file may sit before this test asks for the
 *  ceiling to be re-measured. Same reasoning as the `main.js` ratchet: a
 *  ceiling nobody lowers stops being a ratchet and becomes a comment. */
const SLACK = 12;

test("no file carries more unrouted strings than its ceiling", () => {
  const counts = scanTree(WEBAPP);
  const over = [];
  for (const [file, sites] of counts) {
    const ceiling = CEILINGS.get(file);
    if (ceiling === undefined) {
      over.push(`${file} is not in the table at all (${sites.length} sites) — add it at its
        measured count, or route its strings through the seam`);
      continue;
    }
    if (sites.length > ceiling) {
      const sample = sites
        .slice(ceiling)
        .slice(0, 5)
        .map((site) => `      line ${site.at}: ${JSON.stringify(site.text.slice(0, 60))}`)
        .join("\n");
      over.push(
        `${file} has ${sites.length} unrouted strings, ${sites.length - ceiling} over its ` +
          `ceiling of ${ceiling}. Route them through t() or data-i18n:\n${sample}`,
      );
    }
  }
  assert.deepEqual(over, []);
});

test("a ceiling nobody lowered is a ceiling to re-measure", () => {
  const counts = scanTree(WEBAPP);
  const stale = [];
  for (const [file, ceiling] of CEILINGS) {
    const actual = counts.get(file)?.length ?? 0;
    if (ceiling - actual > SLACK) {
      stale.push(`${file}: ceiling ${ceiling}, actually ${actual} — lower it to ${actual}`);
    }
  }
  assert.deepEqual(stale, []);
});

test("the total is published, so the remaining debt is a number and not a feeling", () => {
  const total = totalSites(scanTree(WEBAPP));
  const budget = [...CEILINGS.values()].reduce((sum, value) => sum + value, 0);
  assert.ok(
    total <= budget,
    `${total} unrouted strings against a budget of ${budget}`,
  );
});

// ---- The code-block exemption ---------------------------------------------
// The site pages embed extracted Rust, JavaScript and shell — most of it
// GENERATED from the code it documents by `tools/build-embed-docs.mjs` — and
// `npm pack` is not translated into eighteen languages. Left counted, that debt
// could never legitimately come down, and a ratchet nobody can turn gets deleted.
//
// So the exemption is structural rather than an allowlist entry: `<pre><code>`.
// The two tests below hold both halves of that — it covers code, and it cannot be
// stretched over prose — plus a measured bound on how much it suppresses, so
// wrapping a paragraph in `<pre><code>` to silence the gate shows up as a number
// that moved rather than as nothing at all.

test("the exemption covers code, and cannot be stretched over prose", () => {
  // Code inside `<pre><code>` is not a translation site.
  assert.deepEqual(scanMarkup('<pre><code>let greeting = "hello there";</code></pre>'), []);
  // Including the syntax-highlighting spans, whose text IS the code. Skipping only
  // the `<pre>` would have counted every keyword and string literal instead.
  assert.deepEqual(
    scanMarkup(
      '<pre><code><span class="kw">let</span> x = <span class="str">"hello there"</span>;</code></pre>',
    ),
    [],
  );
  // `<pre>` ALONE is not enough. Preformatted prose — a transcript, a poem, a
  // wrapped paragraph — keeps counting, which is what stops this covering a family
  // it was never argued for.
  assert.equal(scanMarkup("<pre>Once upon a time there was a document.</pre>").length, 1);
  // The `<pre>`'s own attributes are still read: a caption is prose.
  assert.equal(scanMarkup('<pre title="Shell commands"><code>npm pack</code></pre>').length, 1);
  // And the exemption ends where the block does.
  assert.equal(
    scanMarkup("<pre><code>npm pack</code></pre><p>Install the package first.</p>").length,
    1,
  );
  // An unterminated `<pre>` exempts nothing, rather than everything after it — so
  // BOTH the code and the prose count (2, not 1). Failing open is the right
  // direction for a scanner: a false positive costs one argued allowlist entry, a
  // false negative is a string that ships untranslated.
  assert.equal(scanMarkup("<pre><code>npm pack<p>Install the package first.</p>").length, 2);
});

test("how much the code-block exemption suppresses is measured, per file", () => {
  // MEASURED by scanning each block on its own. If somebody wraps real prose in
  // `<pre><code>` — the one way a structural exemption can be abused — this number
  // rises and the test fails, so the abuse has to be argued instead of being
  // invisible. Files absent from the expectation have no `<pre><code>` at all.
  const SUPPRESSED = {
    "docs.page.html": 2,
    "embedding.page.html": 1,
    "index.page.html": 2,
  };
  const measured = {};
  for (const file of [...CEILINGS.keys()].filter((name) => name.endsWith(".html"))) {
    const source = readFileSync(join(WEBAPP, file), "utf8");
    const sites = codeBlocks(source).reduce((sum, block) => sum + scanMarkup(block).length, 0);
    if (sites) measured[file] = sites;
  }
  assert.deepEqual(measured, SUPPRESSED);
});

// ---- The scanner itself, because a gate nobody trusts gets deleted ---------

test("markup: a human-readable attribute counts, and routing it silences the count", () => {
  assert.equal(scanMarkup('<button title="Bold">Go</button>').length, 2);
  // One letter is a symbol, not prose: `x`, `×`, `›` are not translated.
  assert.equal(scanMarkup('<button title="Bold">x</button>').length, 1);
  assert.equal(
    scanMarkup('<button data-i18n-title="a" title="Bold" data-i18n="b">Bold</button>').length,
    0,
  );
  // Routing only the attribute leaves the text node counted, and the reverse.
  assert.equal(scanMarkup('<button data-i18n-title="a" title="Bold">Bold</button>').length, 1);
  assert.equal(scanMarkup('<button data-i18n="b" title="Bold">Bold</button>').length, 1);
});

test("markup: machine-facing values are not translatable text", () => {
  assert.deepEqual(scanMarkup('<div role="dialog" data-command="file.save"></div>'), []);
  assert.deepEqual(scanMarkup('<span class="ms" aria-hidden="true">chevron_right</span>'), []);
  assert.deepEqual(scanMarkup('<div style="--c:#e2622a"></div>'), []);
  assert.deepEqual(scanMarkup('<a href="https://example.com/docs"></a>'), []);
});

test("script: a literal at a human sink counts; the same string via t() does not", () => {
  assert.equal(scanScript('el.textContent = "No unsaved work to recover";').length, 1);
  assert.equal(scanScript('el.textContent = t("draft.none");').length, 0);
  assert.equal(scanScript('setStatus("Saved");').length, 1);
  assert.equal(scanScript('setStatus(t("doc.saved"));').length, 0);
  assert.equal(scanScript('el.setAttribute("aria-label", "Close settings");').length, 1);
  assert.equal(scanScript('el.setAttribute("aria-label", t("settings.close"));').length, 0);
  assert.equal(scanScript('{ label: "Export as PDF...", id: "file.export.pdf" }').length, 1);
});

test("script: an id, a class or a css value at a human sink is not prose", () => {
  assert.deepEqual(scanScript('el.textContent = "chevron_right";'), []);
  assert.deepEqual(scanScript('const o = { label: "file.export.pdf" };'), []);
  assert.deepEqual(scanScript('el.title = "";'), []);
  assert.deepEqual(scanScript('el.textContent = "—";'), []);
});

test("every allowlist entry carries a reason, and the list stays tiny", () => {
  // `docs/124` §4 promised this list would be explicit and small, with a
  // reason on every entry. An entry is a claim that routing the string would
  // be WRONG — not that routing it is inconvenient — so the size bound is
  // part of the promise, and a reviewer should push back on any addition that
  // reads like the latter.
  const entries = unroutableStrings();
  assert.ok(entries.length <= 5, `${entries.length} exemptions is not "small"`);
  for (const entry of entries) {
    assert.ok(entry.text?.trim(), "an exemption with no string");
    assert.ok(
      (entry.reason ?? "").length > 60,
      `"${entry.text}" is exempt with no argued reason — say why routing it would be wrong`,
    );
  }
});

test("an allowlisted string is not counted, and nothing else is exempt", () => {
  // The exemption is by exact text, so it cannot quietly cover a family.
  assert.deepEqual(scanMarkup('<span>Loading engine…</span>'), []);
  assert.equal(scanMarkup('<span>Loading engines…</span>').length, 1);
  assert.equal(scanMarkup('<span title="Loading engine…">x</span>').length, 0);
});
