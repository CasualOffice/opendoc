import { test as base, expect } from "@playwright/test";

const MOD = process.platform === "darwin" ? "Meta" : "Control";
const WORD_MOD = process.platform === "darwin" ? "Alt" : "Control";

// Collects console errors/pageerrors for the duration of a test so specs can
// assert none occurred instead of only checking the behavior they triggered.
export const test = base.extend({
  consoleErrors: async ({ page }, use) => {
    const errors = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });
    page.on("pageerror", (err) => errors.push(String(err)));
    await use(errors);
  },
});

export { expect, MOD, WORD_MOD };

/** A laid-out element's box, retried until it is real.
 *
 *  `boundingBox()` returns null while an element is mid-re-render, so a spec that
 *  polls for a non-zero width and then calls it AGAIN can still get null — under
 *  parallel load that surfaced as `Cannot read properties of null (reading 'x')`,
 *  a flake that reads exactly like a broken canvas. */
export async function stableBox(locator) {
  let box = null;
  await expect
    .poll(async () => {
      box = await locator.boundingBox();
      return box?.width ?? 0;
    })
    .toBeGreaterThan(0);
  return box;
}

// Navigates to the demo document and waits for the WASM engine to boot, the
// sample to open, and every page to finish its first render (mirrors the
// manual "headless browser smoke" checks previously narrated in
// docs/14-EXECUTION-TRACKER.md — see docs/67-EDITOR-UX-GAP-ANALYSIS.md).
export async function gotoEditor(page) {
  await page.goto("/editor.html?fixture=rich");
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return (
        status !== null &&
        status.textContent === "" &&
        !status.classList.contains("error") &&
        document.querySelectorAll(".page-wrap").length > 0 &&
        // The document now paints before the ~9.5 MB of named web fonts arrive,
        // then re-renders once they register. Tests wait for that upgrade so a
        // geometry assertion can never race the repaint; the product does not.
        document.body.dataset.fontsReady === "true"
      );
    },
    null,
    { timeout: 45_000 },
  );
}

/**
 * How many pages the DOCUMENT has, read from the status bar the user reads.
 *
 * Not `.page-wrap` count: the viewer materializes a sheet only for the pages
 * near the viewport (`docs/113` §8.6), so counting sheets answers "how many
 * pages are on screen", which for a 25,556-page document is about five. A spec
 * that wants the document's own page count has to ask the document.
 *
 * A leading `~` means the total is an ESTIMATE: a document past the engine's
 * open budget opens on a measured prefix and converges as the rest is measured
 * between frames (`docs/116` §7). The marker is read and dropped here so a
 * caller gets a number either way; a caller that cares about exactness should
 * read `#statPages` itself rather than have this quietly decide for it.
 */
export async function documentPageCount(page) {
  const text = (await page.locator("#statPages").textContent()) ?? "";
  const match = text.match(/of\s+~?([\d,]+)/);
  if (!match)
    throw new Error(`the page indicator did not report a total: "${text}"`);
  return Number(match[1].replace(/,/g, ""));
}

/**
 * The sheet element for page `pageNumber` (1-based), scrolled into existence.
 *
 * `.page-wrap` used to be one element per page, so `nth(i)` was page `i + 1`
 * and every page of every document was in the DOM whether anyone had scrolled
 * to it or not. It is not any more (`docs/113` §8.6): a sheet exists only for
 * the pages near the viewport, so `nth(i)` is page `i + 1` only while the
 * reader is still at the top, and a spec that wants page 40 has to do what a
 * reader does — scroll there.
 *
 * Deliberately uses nothing but the scroller and the sheets' own
 * `data-page-number`, so it exercises the real scroll path rather than an app
 * internal: it bisects the scroll range, which works for a 6-page document and
 * for a 25,556-page one because the mapping from scroll position to document
 * position is monotonic by construction.
 */
export async function pageSheet(page, pageNumber) {
  const found = await page.evaluate(async (n) => {
    const viewport = document.getElementById("viewport");
    const sheet = () =>
      document.querySelector(`.page-wrap[data-page-number="${n}"]`);
    const frame = () =>
      new Promise((resolve) => requestAnimationFrame(resolve));
    const total = Number(
      (
        (document.getElementById("statPages")?.textContent ?? "").match(
          /of\s+([\d,]+)/,
        )?.[1] ?? "0"
      ).replace(/,/g, ""),
    );
    let lo = 0;
    let hi = viewport.scrollHeight - viewport.clientHeight;
    // One proportional guess first. Scroll position is linear in document
    // position, so for a document of roughly equal pages this lands on (or
    // beside) the page immediately, and the bisection below only has to clean
    // up — which is what keeps a jump to page 25,556 from costing 14 rounds of
    // rasterizing pages nobody asked for.
    if (total > 1) {
      viewport.scrollTop = (hi * (n - 1)) / (total - 1);
      await frame();
      await frame();
    }
    // Then bisect on the scroll position until the page is not merely in the
    // DOM but under the reader's eyes. Deliberately NOT `scrollIntoView`: for
    // a document tall enough to be compressed onto a bounded scroll range, a
    // delta measured on screen is `scale` times too large as a scroll delta,
    // so `scrollIntoView` overshoots — and above a factor of two it oscillates
    // instead of converging. Bisection needs only that page position is
    // monotonic in scroll position, which it is at every size.
    for (let i = 0; i < 60; i++) {
      const element = sheet();
      if (element) {
        const box = element.getBoundingClientRect();
        const view = viewport.getBoundingClientRect();
        // Aim the page's TOP just below the top of the viewport — what
        // `scrollIntoView({ block: "start" })` would do if it could be trusted
        // here. That shows the page's own top margin (where the header band
        // and its marker live) AND leaves the page covering the middle of the
        // viewport, which is what the editor's `pageInView()` answers with.
        if (
          box.top >= view.top - 2 &&
          box.top <= view.top + viewport.clientHeight / 2
        )
          break;
        // Scrolling further moves content UP, so a page whose top is too low
        // needs MORE scroll, and one whose top is off the top needs less.
        if (box.top > view.top) lo = viewport.scrollTop;
        else hi = viewport.scrollTop;
      } else {
        const numbers = [...document.querySelectorAll(".page-wrap")].map(
          (wrap) => Number(wrap.dataset.pageNumber),
        );
        if (numbers.length === 0) break;
        if (n < Math.min(...numbers)) hi = viewport.scrollTop;
        else lo = viewport.scrollTop;
      }
      const next = (lo + hi) / 2;
      // As close as scrolling gets: a document at either end, or a page the
      // scroll granularity cannot centre any better.
      if (Math.abs(next - viewport.scrollTop) < 0.5 && sheet()) break;
      viewport.scrollTop = next;
      // The viewer materializes its sheets from the scroll event, which the
      // browser fires at the next rendering opportunity, not on assignment.
      await frame();
      await frame();
    }
    return !!sheet();
  }, pageNumber);
  if (!found) throw new Error(`page ${pageNumber} could not be scrolled to`);
  return page.locator(`.page-wrap[data-page-number="${pageNumber}"]`);
}

// Clicks into the first rendered page to focus the editor surface and give
// the engine an initial hit-tested caret, independent of the demo's exact
// text layout.
export async function clickIntoFirstPage(page) {
  await page
    .locator(".page-wrap .page")
    .first()
    .click({ position: { x: 60, y: 60 } });
}

/**
 * Asserts the editing surface holds focus.
 *
 * Use this instead of `expect(page.locator("#pages")).toBeFocused()`. Focus is
 * owned by the editable proxy `#editorTextInput`, not by `#pages` — a
 * non-editable div cannot own text input, so it raises no soft keyboard and
 * fires no composition events (docs/105 UX-001). `#pages` is still focusable
 * and still the skip-link target; focus landing there is immediately handed to
 * the proxy.
 *
 * Asserting either element is correct, because the question these tests are
 * really asking is "can the editor receive text?" — which is why they should
 * not name a specific element at all.
 */
export async function expectEditorFocused(page) {
  const active = await page.evaluate(() => document.activeElement?.id ?? "");
  if (active !== "pages" && active !== "editorTextInput") {
    throw new Error(
      `expected the editing surface to hold focus, but focus is on "${active || "(none)"}"`,
    );
  }
}

// Moves the caret to the very start of the document (⌘/Ctrl+Home), so
// later assertions do not depend on where the initial click happened to land.
export async function moveCaretToDocStart(page) {
  await page.keyboard.press(`${MOD}+Home`);
}

// The three-state review-mode control (`#reviewModeControl`: Editing /
// Suggesting / Viewing) lives in the persistent footer. Selects `mode` and
// confirms its segment became the pressed one.
export async function setReviewMode(page, mode) {
  const button = page.locator(
    `#reviewModeControl [data-review-mode="${mode}"]`,
  );
  await button.click();
  await expect(button).toHaveAttribute("aria-pressed", "true");
}

// Types `marker` at the caret, rewinds to just before it, then proves the
// editor is still live by finding it via the real Find panel — the same
// "click, type, find" recovery check used for every focus-recovery scenario.
// Ends by undoing the insertion so specs stay independent of each other.
export async function typeMoveFindUndo(page, marker) {
  await page.keyboard.type(marker);
  await moveCaretToDocStart(page);
  await page.keyboard.press(`${MOD}+f`);
  const findInput = page.locator("#findInput");
  await findInput.fill(marker);
  await expect(page.locator("#findStatus")).toHaveText("1 match");
  await page.keyboard.press("Escape");
  await page.locator("#undoBtn").click();
}

// --- Reaching capabilities that left the top bar ------------------------------
// Open, Save and the Search box were removed from the header: they duplicated
// the File menu and the command palette, which is where Word and Docs put them.
// Specs that clicked `#openBtn` / `#save` / `#searchTrigger` were asserting that
// a BUTTON existed, not that the capability was reachable — so when the button
// went, they failed while the capability was fine. These helpers drive the
// surviving surface instead, which is the thing actually worth guarding: if a
// command stops being reachable from its menu, every spec using them fails.

/** Switches to the compact chrome, whose navigation axis IS the menu bar.
 *
 *  The menu bar used to show in both chromes, alongside the ribbon tab strip —
 *  two navigation systems at once, which is `109` UX-014 and what the owner
 *  asked to collapse. It is now compact-mode chrome only (`docs/122`), so a spec
 *  that wants a menu has to be in the chrome that has menus. Idempotent: already
 *  compact is a no-op. */
export async function useCompactChrome(page) {
  // Already showing the bar is enough — and it is showing in the ribbon chrome's
  // EMPTY state, where the band is hidden and the bar is the axis. Switching
  // chrome there would change the thing under test for no reason.
  if (await page.locator("#appMenuBar").isVisible()) return;
  await page.locator("#modeCompact").click();
  await expect(page.locator("#appMenuBar")).toBeVisible();
}

/** Opens one of the application menus and returns its popover locator.
 *
 *  Enters the compact chrome first. Specs calling this are asking "is the
 *  capability reachable from its menu", not "is the editor in ribbon mode" — the
 *  guarantee, not the mechanism, which is the shape `expectEditorFocused` fixed
 *  for focus. `one-axis-navigation.spec.mjs` is what guards the ribbon chrome's
 *  own axis. */
export async function openAppMenu(page, menu) {
  await useCompactChrome(page);
  await page.locator(`.app-menu-button[data-menu="${menu}"]`).click();
  const popover = page.locator("#appMenuPopover");
  await expect(popover).toBeVisible();
  return popover;
}

/** Opens the ribbon chrome's File page and returns its body locator. */
export async function openFilePage(page) {
  if (await page.locator("body.compact-mode").count()) await page.locator("#modeRibbon").click();
  await page.locator("#tabFile").click();
  const body = page.locator("#filePageBody");
  await expect(body).toBeVisible();
  return body;
}

/** Runs a command through its application-menu row, by command id. Asserts the
 *  row is actually present and enabled, so an unreachable command fails loudly
 *  rather than silently doing nothing. */
export async function runAppMenuCommand(page, menu, commandId) {
  await openAppMenu(page, menu);
  const row = page.locator(
    `#appMenuPopover .app-menu-item[data-command="${commandId}"]`,
  );
  await expect(
    row,
    `${commandId} should be reachable from the ${menu} menu`,
  ).toBeVisible();
  await expect(
    row,
    `${commandId} should be enabled in the ${menu} menu`,
  ).toBeEnabled();
  await row.click();
}

/** Opens the command palette through the File page — a real, clickable surface,
 *  not the keyboard chord, so this still proves a pointer user can get there.
 *  It used to go through a Help menu; Help is a File-page group now, as it is in
 *  ONLYOFFICE, and the RIBBON chrome is the default, so this drives the default. */
export async function openCommandPalette(page) {
  await runFilePageCommand(page, "help.commands");
  await expect(page.locator("#cmdInput")).toBeFocused();
}

/** Runs a command through the File surface of whichever chrome is showing — the
 *  ribbon chrome's File page, or the compact chrome's File dropdown. Both render
 *  the same roster from `FILE_SURFACE`, so a spec that only cares "File ▸ X is
 *  reachable and runs" must not have to know which chrome it is in, and must not
 *  silently switch the chrome under a spec that chose one. */
export async function runFilePageCommand(page, commandId) {
  if (await page.locator("body.compact-mode").count()) {
    await runAppMenuCommand(page, "file", commandId);
    return;
  }
  await openFilePage(page);
  const row = page.locator(`#filePageBody .file-page-item[data-command="${commandId}"]`);
  await expect(row, `${commandId} should be reachable from the File page`).toBeVisible();
  await expect(row, `${commandId} should be enabled on the File page`).toBeEnabled();
  await row.click();
}

/** Saves the open document through File ▸ Save. */
export async function saveDocument(page) {
  await runFilePageCommand(page, "file.save");
}

/** Asserts File ▸ Save is present and enabled without invoking it. */
export async function expectSaveEnabled(page) {
  if (await page.locator("body.compact-mode").count()) {
    await openAppMenu(page, "file");
    const row = page.locator('#appMenuPopover .app-menu-item[data-command="file.save"]');
    await expect(row).toBeVisible();
    await expect(row).toBeEnabled();
    await page.keyboard.press("Escape");
    return;
  }
  await openFilePage(page);
  const row = page.locator('#filePageBody .file-page-item[data-command="file.save"]');
  await expect(row).toBeVisible();
  await expect(row).toBeEnabled();
  await page.keyboard.press("Escape");
}

// --- Paragraph styles ---------------------------------------------------------
// Six specs read `#paragraphStyle` — a native `<select>` that listed every style in
// the document — to answer two different questions: "what style is the caret in?"
// and "what styles does this document define?". When that control was deleted
// (docs/115: the ribbon offers a SHORT list, one control) every one of them broke,
// which is the same pattern as the `expectEditorFocused` cleanup: a spec asserting a
// MECHANISM rather than the guarantee has to be patched each time the mechanism
// moves. These three helpers are the guarantee. If the Styles control changes shape
// again, this is the only place that moves.

/** The paragraph style at the caret. `#stylesTrigger`'s `data-active-style` is
 *  written straight from `doc.paragraphStyleAt(...)` on every toolbar refresh, and
 *  its visible label carries the same value. */
export async function reflectedParagraphStyle(page) {
  return page.locator("#stylesTrigger").getAttribute("data-active-style");
}

/** The styles the band currently OFFERS, in order — the short list, read by opening
 *  the control. Closes it again, so a caller can ask without changing what is open. */
export async function offeredParagraphStyles(page) {
  await page.locator("#stylesTrigger").click();
  const names = await page.$$eval("#stylesMenu .style-option", (els) =>
    els.map((el) => el.dataset.style),
  );
  await page.keyboard.press("Escape");
  return names;
}

/** Every paragraph style the open document defines. Read from Paragraph properties ▸
 *  Style, which carries the COMPLETE list — the band deliberately does not. */
export async function definedParagraphStyles(page) {
  return page.$$eval("#paraPanelStyle option", (opts) =>
    opts.map((o) => o.value).filter(Boolean),
  );
}

/** Applies a named paragraph style, from the band when it offers that style and from
 *  the command palette otherwise — so a caller does not have to know which of the two
 *  surfaces currently carries it. Waits for the change to be reflected back. */
export async function applyParagraphStyle(page, name) {
  await page.locator("#stylesTrigger").click();
  const option = page.locator(
    `#stylesMenu .style-option[data-style="${name}"]`,
  );
  if ((await option.count()) > 0) {
    await option.click();
  } else {
    await page.keyboard.press("Escape");
    await runAppMenuCommand(page, "help", "help.commands");
    await page.locator("#cmdInput").fill(`Style: ${name}`);
    const row = page.locator(
      `#cmdList .cmd-item[data-command-id="style.${name}"]`,
    );
    await expect(
      row,
      `"Style: ${name}" should be reachable from the palette`,
    ).toBeVisible();
    await row.click();
  }
  await expect.poll(() => reflectedParagraphStyle(page)).toBe(name);
}

// --- Platform-correct shortcut hints -----------------------------------------
// Specs asserted hint text as a literal "⌘P". That is the Mac rendering; on
// Linux the same command renders "Ctrl+P", so those assertions passed on the
// author's machine and failed in CI — the browser-smoke job has been red on
// `main` for exactly this. It is also docs/105 UX-009 showing through: the
// product hardcodes Mac glyphs in 71 places, so a test that hardcodes one is
// reproducing the defect rather than catching it.
//
// `formatShortcut` is the function the app itself renders hints with, so
// deriving the expectation from it means the spec cannot disagree with the UI
// about what a chord looks like on the platform it is running on.
import {
  formatShortcut,
  APPLE_PLATFORM,
  STANDARD_PLATFORM,
} from "../../src/keyboard.mjs";

const TEST_PLATFORM =
  process.platform === "darwin" ? APPLE_PLATFORM : STANDARD_PLATFORM;

/** The hint text the editor will render for a Mac-notation chord, here. */
export function shortcutHint(appleNotation) {
  return formatShortcut(appleNotation, TEST_PLATFORM);
}

/** The accessibility mirror's blocks, in reading order — one string per
 *  top-level node of `#a11yDocument` (a paragraph, a heading, a list, a
 *  table). */
export async function mirrorBlocks(page) {
  return page.locator("#a11yDocument > *").allTextContents();
}

/**
 * Asserts that `typed` reached exactly ONE block of the accessibility mirror,
 * and that every other block is unchanged.
 *
 * This replaces an idiom six specs shared:
 * `expect(await mirror.textContent()).toBe(bodyBefore)` after typing into a
 * text box — "the body is untouched, so the text went into the box". That was
 * only ever true because the mirror carried no text-box content AT ALL, which
 * is the defect `109` HF-169 names: a screen reader could not read a word of
 * any drawing on the page. Now that it can, an assertion that the mirror never
 * changes would forbid the fix.
 *
 * The guarantee those specs meant is unchanged, and is what this states: the
 * typing landed in one block, and the document body is not that block.
 */
export function expectTypedIntoOneBlock(before, after, typed) {
  const took = after.filter((text) => text.includes(typed));
  expect(
    took.length,
    `exactly one block must carry ${typed}: ${after.join(" | ")}`,
  ).toBe(1);
  // Every block that did NOT take the text must still be one of the blocks
  // that were there, matched one for one so a duplicate cannot cover for a
  // lost sibling.
  const remaining = [...before];
  for (const text of after.filter((t) => !t.includes(typed))) {
    const at = remaining.indexOf(text);
    expect(at, `the mirror lost a block: ${text}`).toBeGreaterThanOrEqual(0);
    remaining.splice(at, 1);
  }
  // At most one block of `before` is unaccounted for: the one the typing went
  // into. Zero means the text landed in a block that did not exist before —
  // an inserted text box, say — which is equally "not the body".
  expect(
    remaining.length,
    `more than one block changed: ${remaining.join(" | ")}`,
  ).toBeLessThanOrEqual(1);
}
