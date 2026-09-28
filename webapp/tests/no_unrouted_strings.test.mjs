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
//
// THE EXCEPTION THAT USED TO LIVE HERE HAS EVAPORATED, WHICH IS THE POINT.
//
// For as long as the site had no seam, this file carried a deliberate exception:
// a ratchet only means "route the string" where there is something to route it
// through, and the `*.page.html` templates and the shared partials had zero
// `data-i18n` attributes, no per-language pages and no `hreflang`. Their numbers
// were therefore DECLARED MEASUREMENTS rather than ceilings — equality in both
// directions — because the only way to lower one was to delete English, and
// `docs/126` asks for site documentation as part of every SDK phase. A `SEAMLESS`
// set held those files, a test proved each one really had no seam with
// `editor.html` as the control, and the note said that the day site localisation
// landed those assertions would fail and the exception would disappear without
// anybody having to remember it.
//
// That day is `109` HF-198. The site now goes through the SAME seam the editor
// does — `data-i18n` in the markup with the English beside the key,
// `tools/build-locale.mjs` extracting it into the same `locales/en.json`,
// `i18n.mjs` resolving it and `localize.mjs` applying it, driven on the site by
// `src/site_locale.mjs`. So every file below is a RATCHET again, `SEAMLESS` is
// gone, and the test that bounded it has been turned around: it now asserts that
// every site template and partial HAS the seam, which is what stops the next page
// shipping without one.
//
// The numbers that moved, and why, are in the table.
import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";

const { exemptedSites, scanMarkup, scanScript, scanTree, totalSites, unroutableStrings } =
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
  // ---- The SITE (`109` HF-190, then HF-198) -------------------------------
  // HF-190 made the site's debt visible: `scanTree` read `editor.html` and
  // `src/*.{js,mjs}` and nothing else, so every `*.page.html` and every shared
  // partial sat outside the gate entirely — about five hundred unrouted strings,
  // against an `editor.html` ceiling of sixteen, with no number anywhere that
  // would have said so. They were measured, declared, and left, because there was
  // no seam to route them through.
  //
  // HF-198 built the seam, and these are what the same scanner measures now.
  // MEASURED, every one, by running it over the routed file — nothing here is
  // arithmetic, which is the rule that caught a report's approximate figures the
  // first time round.
  //
  //   docs.page.html       54 -> 0
  //   fidelity.page.html   84 -> 0
  //   index.page.html     157 -> 0
  //   _partials/site-footer.html  4 -> 0
  //   _partials/site-header.html 12 -> 1
  //   playground.page.html    109 -> 65
  //   embedding.page.html     353 -> 193
  //
  // Three pages and the footer are at ZERO: every sentence, heading, label,
  // `title` and `aria-label` on them now carries a `data-i18n*` key whose English
  // sits beside it in the markup, exactly as `editor.html` does.
  //
  // The header's remaining ONE is the product name, split across `brand-name` /
  // `brand-d` so the D can be styled. It is a proper noun, it is not translated in
  // any language, and a host renames it through `brand.json` and `BRAND_STRINGS`
  // (the override layer in `i18n.mjs`) rather than through a locale. Routing it
  // would put OpenDoc in nineteen catalogues and invite nineteen translations of
  // a word that has none.
  //
  // What is left on the two SDK pages is the GENERATED regions — everything
  // between `<!-- @generated NAME -->` and `<!-- @end NAME -->`, written by
  // `tools/build-embed-docs.mjs` from `MEANINGS`, `REGION_MEANINGS`, `HOST_EVENTS`
  // and `REFUSAL_CODES`. Routing those means giving the generator a key per
  // capability, per region, per event and per refusal code and teaching it to emit
  // `data-i18n`; it is real work, it belongs to whoever owns that generator, and
  // it is deliberately NOT bundled here. Hand-editing the pages instead is not an
  // option — `build-embed-docs --check` would fail the build, correctly. These two
  // numbers are ceilings like every other row: they may fall, never rise.
  ["docs.page.html", 0],
  ["embedding.page.html", 193],
  ["fidelity.page.html", 0],
  ["index.page.html", 0],
  ["playground.page.html", 65],
  ["_partials/site-footer.html", 0],
  ["_partials/site-header.html", 1],
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
  // 325 -> 312 in the cheap table round (`docs/141`). Out through the catalogue:
  // the four sentences that say why a table command is unavailable, which the
  // Table MENU already had and the Table BAND did not (TBL-03) and which are now
  // one entry each read by both; and `Selected table ${mode}`, the one table
  // status line with no catalogue entry at all. Out by deletion: `#mergeCellsBtn`'s
  // unreachable "Select a table row, column, or table first" — a second, divergent
  // copy of the menu's sentence on a branch the button's own `disabled` made
  // unreachable. Out by extraction: six command labels that moved with
  // `table_commands.mjs` (next entry), which is the same debt relocated and not
  // debt removed. In: nothing — every sentence this round adds is a `t()` key.
  // Measured, not calculated.
  ["src/main.js", 312],
  // The nine `label:`/`disabledReason:` literals that moved out of `main.js` with
  // the `table.*` command tree. Same debt in a new place, not a new debt: `main.js`
  // came down by more than nine in the same commit. They stay English because this
  // table's sibling labels do (`TABLE_MENU_LABELS` is read at import, before a
  // catalogue exists); routing the whole family is its own piece of work.
  // ONE literal, moved out of `main.js` with the line-spacing command rows and
  // not added by them: `"Line spacing: "`, the prefix in front of a preset's own
  // label, which is read from the popover markup the catalogue already covers.
  // `main.js` came down by more than one in the same commit. Routing it means
  // giving the four presets a second name in `EN_STRINGS` beside the one their
  // markup already carries, which is how two spellings of one control start to
  // drift; that is its own piece of work.
  ["src/spacing_menu.mjs", 1],
  ["src/table_commands.mjs", 9],
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

/** Every hand-authored site source, which must all carry the seam now (HF-198).
 *  The set that used to live here was the opposite list — the files with NO seam,
 *  whose numbers were measurements rather than ceilings. */
const SITE_SOURCES = [
  "docs.page.html",
  "embedding.page.html",
  "fidelity.page.html",
  "index.page.html",
  "playground.page.html",
  "_partials/site-footer.html",
  "_partials/site-header.html",
];

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

test("every site template and partial carries the localisation seam", () => {
  // THE TURNED-AROUND GUARD. This test used to prove the opposite — that each
  // file the table called `SEAMLESS` really had no way to route a string, so the
  // exception could not be claimed for convenience. HF-198 built the seam, so
  // what is worth holding now is that it is still there: a new site page, or a
  // rewrite of an old one, must not ship with English that no catalogue can
  // reach. The failure mode it prevents is the one that created the exception in
  // the first place.
  for (const file of SITE_SOURCES) {
    const source = readFileSync(join(WEBAPP, file), "utf8");
    assert.ok(
      /\bdata-i18n(-[a-z]+)?=/.test(source),
      `${file} carries no data-i18n attribute at all, so its English cannot be translated. ` +
        `Route it — every other site source does — rather than declaring a measurement.`,
    );
    assert.ok(CEILINGS.has(file), `${file} carries no declared number`);
  }
  // The CONTROL, without which the assertion above could pass on a detection that
  // is simply broken. `editor.html` is the chrome that was routed first (853 sites
  // down to 16); if the seam is not detectable there, it is the detection that is
  // wrong rather than the site that is routed.
  const editor = readFileSync(join(WEBAPP, "editor.html"), "utf8");
  assert.ok(
    /\bdata-i18n(-[a-z]+)?=/.test(editor),
    "editor.html has no data-i18n at all, so this test cannot tell a seam from its absence",
  );
  // And the NEGATIVE control: a source with no seam must fail the same check, or
  // the assertion above says nothing. `tools/string_sites.mjs` is the scanner
  // itself — prose in comments, not a single routed string.
  assert.equal(
    /\bdata-i18n(-[a-z]+)?=/.test(readFileSync(join(WEBAPP, "tools/palette_source.mjs"), "utf8")),
    false,
    "the negative control carries a data-i18n attribute, so this test cannot fail",
  );
  // Every site source is a page template or a shared partial, and every page
  // template and shared partial is a site source. A file added to the site and
  // forgotten here would otherwise be exempt by omission.
  const onDisk = [
    ...readdirSync(WEBAPP).filter((name) => name.endsWith(".page.html")),
    ...readdirSync(join(WEBAPP, "_partials"))
      .filter((name) => name.endsWith(".html"))
      .map((name) => `_partials/${name}`),
  ].sort();
  assert.deepEqual([...SITE_SOURCES].sort(), onDisk);
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

// ---- The code exemptions ---------------------------------------------------
// Two of them, both structural rather than allowlist entries, because a structural
// exemption cannot quietly cover prose:
//
//   * `<pre><code>…</code></pre>`. The site pages embed extracted Rust, JavaScript
//     and shell — most of it GENERATED from the code it documents by
//     `tools/build-embed-docs.mjs` — and `npm pack` is not translated into eighteen
//     languages. Left counted, that debt could never legitimately come down, and a
//     ratchet nobody can turn gets deleted.
//   * `<script>` and `<style>` BODIES. This one is a bug fix as much as an
//     exemption. `scanMarkup` skipped the script and style OPENING TAGS by name and
//     then walked straight into their contents, because it captures the text after
//     each tag up to the next `<` — so a script body containing anything
//     tag-SHAPED was read as markup. Putting the embedding guide's own description
//     ("…the `<opendoc-editor>` custom element…") into an `ld+json` block made the
//     scanner match `<opendoc-editor>` as an element and count the rest of the JSON
//     line as translatable prose. The ratchet caught it at one site over a ceiling.
//
// The tests below hold both halves — they cover code, and they cannot be stretched
// over prose — plus a measured bound on how much they suppress, so hiding a
// paragraph in either shows up as a number that moved rather than as nothing.

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

test("a script or style body is code, tag-shaped text inside it included", () => {
  // The regression this exists for, exactly as it was found. Without the
  // exemption, `<opendoc-editor>` in a JSON string is matched as an element and the
  // rest of the line is counted as prose — one site, in a file whose ceiling had
  // zero slack.
  const jsonld =
    '<script type="application/ld+json">\n' +
    '{ "description": "Embed the editor: the <opendoc-editor> custom element, and a contract." }\n' +
    "</script>";
  assert.deepEqual(scanMarkup(jsonld), []);
  assert.equal(scanMarkup(jsonld, { exemptCode: false }).length, 1);
  // A style body, same shape.
  assert.deepEqual(scanMarkup("<style>\n.a::after { content: \"<b>x</b> and some words\"; }\n</style>"), []);
  // And it ends at the closing tag: prose after the script still counts, and only
  // that prose.
  const terminated = '<script>var a = "<i>some words here</i>";</script><p>Real prose here.</p>';
  assert.equal(scanMarkup(terminated).length, 1);
  // An unterminated script exempts nothing, the same failing-open rule as `<pre>` —
  // so the tag-shaped text inside it is counted again alongside the real prose.
  const unterminated = '<script>var a = "<i>some words here</i>";<p>Real prose here.</p>';
  assert.equal(scanMarkup(unterminated).length, 2);
});

test("how much the code exemptions suppress is measured, per file", () => {
  // MEASURED by scanning each file with the exemptions on and off and taking the
  // difference. If somebody wraps real prose in `<pre><code>` or hides it in a
  // script body — the one way a structural exemption can be abused — this number
  // rises and the test fails, so the abuse has to be argued instead of being
  // invisible. Files absent from the expectation suppress nothing.
  //
  // The first version of this measurement scanned each exempt block on its own and
  // reported `embedding.page.html: 1`. That was wrong, and the way it was wrong is
  // worth keeping: `scanMarkup` captures the text after a tag only up to the next
  // `<`, and returns nothing when there is no next `<` at all, so the sentence it
  // really does read in the whole page reads as empty in a sliced-out block. The
  // real figure is 2. A measurement taken on a case that does not exercise the
  // behaviour is not a measurement.
  const SUPPRESSED = {
    "docs.page.html": 2,
    "embedding.page.html": 4,
    "index.page.html": 2,
    "playground.page.html": 2,
  };
  const measured = {};
  for (const file of [...CEILINGS.keys()].filter((name) => name.endsWith(".html"))) {
    const sites = exemptedSites(readFileSync(join(WEBAPP, file), "utf8"));
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
