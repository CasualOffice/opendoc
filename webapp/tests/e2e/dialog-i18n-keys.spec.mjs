// No surface may paint an i18n KEY where its text belongs.
//
// Reported by the owner: "on dialog i can see labels instead of texts o UI like
// captionDialog.headingLevel". They were right, and the mechanism is worth
// stating because the existing guards could not see it.
//
// `t()` returns the key itself for a miss — deliberately, so it is visible in a
// screenshot rather than silently blank. Catalogues arrive ASYNCHRONOUSLY:
// `i18n.mjs`'s `setCatalogue` is "called by the loader once a locale's JSON
// arrives". The caption dialog built its nine heading-level options in its
// FACTORY BODY, which ran before that arrival, so all nine were stamped with the
// literal string `captionDialog.headingLevel` and never rebuilt — the raw key sat
// on screen for the life of the tab, in every language.
//
// WHY NOTHING CAUGHT IT. `locale_coverage.test.mjs` asserts every key exists, and
// it does — in all 19 catalogues. `no_unrouted_strings.test.mjs` asserts English
// literals go through the seam, and this one did. Both check the CATALOGUE.
// Neither checks that a key was resolved by the time it was PAINTED, which is a
// different question and the one that was failing.
//
// So this walks the rendered chrome and asks whether any painted string IS one of
// the catalogue's own keys. Deliberately not a list of dialogs-and-their-strings:
// a list has to be maintained, and the next dialog to make this mistake would not
// be on it.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

/** Every key the English catalogue actually defines.
 *
 *  An EXACT set, not a regex. The first version of this guard matched
 *  `dotted.camelCase` and duly flagged `sample.docx` — a heuristic that fires on
 *  file names is a heuristic somebody deletes. Comparing against the real key
 *  list cannot false-positive on ordinary text, and it also catches an
 *  all-lower-case key like `status.words` that a camelCase pattern would miss. */
const KEYS = new Set(
  Object.keys(
    JSON.parse(
      readFileSync(
        join(dirname(fileURLToPath(import.meta.url)), "..", "..", "locales", "en.json"),
        "utf8",
      ),
    ),
  ),
);

/** Every string a person can read in the given root, with where it came from. */
async function paintedStrings(page, root) {
  return page.evaluate((selector) => {
    const host = document.querySelector(selector);
    if (!host) return [];
    const out = [];
    const push = (text, where) => {
      const value = (text ?? "").trim();
      if (value) out.push({ value, where });
    };
    for (const el of host.querySelectorAll("*")) {
      const style = getComputedStyle(el);
      if (style.display === "none" || style.visibility === "hidden") continue;
      // A closed dialog's markup is still in the document, and a `<select>`'s
      // options enumerate even when an ANCESTOR is hidden — so the chrome sweep
      // was reporting a hidden dialog's strings as painted chrome.
      if (el.closest("[hidden]")) continue;
      // Leaf text only, so a container is not charged with its children's text.
      if (el.children.length === 0) push(el.textContent, `${el.tagName}#${el.id || ""}`);
      // Attributes a person reads.
      for (const attribute of ["title", "placeholder", "aria-label"]) {
        push(el.getAttribute(attribute), `${el.tagName}#${el.id || ""}[${attribute}]`);
      }
      // A select paints its options even though they are children.
      if (el.tagName === "SELECT") {
        for (const option of el.options) push(option.textContent, `${el.id}<option>`);
      }
    }
    return out;
  }, root);
}

/** The dialogs reachable from the References band, which is where the reported
 *  defect was. Each is opened through a real control, not by unhiding it: a
 *  dialog unhidden by script may never run the code that fills it. */
const REFERENCE_DIALOGS = [
  { name: "caption", button: "#refCaptionBtn", dialog: "#captionDialog" },
  { name: "cross-reference", button: "#refCrossRefBtn", dialog: "#crossRefDialog" },
];

test.describe("no surface paints an i18n key", () => {
  test("the key set is real, and ordinary text is not in it", () => {
    // The guard's own guard. Cheap, and it pins the thing that went wrong first:
    // a pattern-based version flagged `sample.docx`.
    expect(KEYS.size, "the English catalogue must load").toBeGreaterThan(500);
    for (const key of ["captionDialog.headingLevel", "capability.notGranted"]) {
      expect(KEYS.has(key), `${key} must be a known key`).toBe(true);
    }
    for (const real of ["Heading 1", "Insert caption", "1, 2, 3, …", "sample.docx", "⌘⇧P"]) {
      expect(KEYS.has(real), `${real} must NOT be treated as a key`).toBe(false);
    }
  });

  for (const { name, button, dialog } of REFERENCE_DIALOGS) {
    test(`${name}: every painted string is resolved, not a key`, async ({ page, consoleErrors }) => {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.locator('[data-tab="references"]').click();

      const control = page.locator(button);
      await expect(control).toBeVisible();
      // The caption button is disabled until something is captionable; the
      // cross-reference one is not. Skip rather than force it open — a dialog
      // opened by script may skip the very code that fills it, which is exactly
      // the bug being guarded.
      test.skip(!(await control.isEnabled()), `${name} is disabled in this fixture`);
      await control.click();
      await expect(page.locator(dialog)).toBeVisible();

      const strings = await paintedStrings(page, dialog);
      expect(strings.length, `${name} painted nothing, so this proves nothing`).toBeGreaterThan(5);

      const leaked = strings.filter((s) => KEYS.has(s.value));
      expect(
        leaked.map((s) => `${s.value}  (${s.where})`),
        `${name} is painting i18n keys instead of text — the catalogue had not answered ` +
          "by the time these were rendered",
      ).toEqual([]);

      expect(consoleErrors).toEqual([]);
    });
  }

  test("the editor chrome itself paints no keys", async ({ page }) => {
    // The same failure in the ribbon or the menus would be worse, since it is on
    // screen without opening anything.
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    const strings = await paintedStrings(page, "body > .app, body");
    expect(strings.length, "the chrome painted nothing, so this proves nothing").toBeGreaterThan(40);
    const leaked = strings.filter((s) => KEYS.has(s.value));
    expect(
      leaked.map((s) => `${s.value}  (${s.where})`),
      "the editor chrome is painting i18n keys instead of text",
    ).toEqual([]);
  });
});
