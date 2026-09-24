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
import { dirname, join } from "node:path";

const { scanMarkup, scanScript, scanTree, totalSites, unroutableStrings } = await import(
  "../tools/string_sites.mjs"
);

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
  ["src/a11y_mirror.mjs", 1],
  ["src/blank_document.mjs", 8],
  ["src/bookmark_manager.mjs", 6],
  ["src/command_menu.mjs", 6],
  ["src/command_taxonomy.mjs", 8],
  ["src/compact_toolbar.mjs", 18],
  ["src/export_commands.mjs", 6],
  ["src/fidelity.js", 5],
  ["src/file_pane.mjs", 12],
  ["src/format_io.mjs", 6],
  ["src/home-embed.js", 4],
  ["src/keyboard.mjs", 7],
  ["src/main.js", 364],
  ["src/pages_panel.mjs", 4],
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
