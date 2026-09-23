// HF-025 / `105` UX-009 — no surface may show a Mac glyph to a keyboard that
// does not have one.
//
// This spec runs the editor with a WINDOWS navigator on purpose, so it means
// the same thing on a macOS developer machine and on the Linux CI runner. It
// then sweeps the rendered DOM — text and every user-visible attribute, hidden
// elements included — rather than checking the two labels that were known to
// be wrong. That is the point: the next raw ⌘ anyone adds to a menu, a chip, a
// tooltip or a dialog fails here without anyone updating this file.
//
// Document content is excluded. A ⌘ in a comment or a paragraph is the USER's
// text; localizing it would be corrupting the document.
import {
  test,
  expect,
  gotoEditor,
  openAppMenu,
  openCommandPalette,
  runAppMenuCommand,
  runFilePageCommand,
} from "./fixtures.mjs";
import { STANDARD_PLATFORM, formatShortcut } from "../../src/keyboard.mjs";

const WINDOWS_UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
const MAC_UA =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";

/** Makes `keyboardPlatform()` see the platform we are testing, whatever the
 *  machine running the test is. It reads `userAgentData.platform`, `platform`
 *  and `userAgent`, so all three have to agree or the join still matches
 *  /mac/. */
async function pretendPlatform(page, { platform, userAgentDataPlatform }) {
  await page.addInitScript(
    ([nav, uad]) => {
      Object.defineProperty(navigator, "platform", { get: () => nav });
      Object.defineProperty(navigator, "userAgentData", {
        get: () => ({ platform: uad, brands: [], mobile: false }),
      });
    },
    [platform, userAgentDataPlatform],
  );
}

/** Every Mac glyph currently rendered anywhere in the application chrome. */
function collectGlyphs(page) {
  return page.evaluate(() => {
    const GLYPH = /[⌘⌃⌥⇧⏎⌫⌦⎋⇥]/u;
    // Kept in step with `DOCUMENT_CONTENT_SELECTOR` in src/shortcut_labels.mjs.
    const CONTENT = "#pages, #a11yDocument, #reviewSidebarBody, script, style, template";
    const ATTRS = ["title", "aria-label", "aria-keyshortcuts", "placeholder", "alt", "data-tip-title"];
    const found = [];
    const describe = (el) =>
      `${el.tagName.toLowerCase()}${el.id ? `#${el.id}` : ""}${
        el.className && typeof el.className === "string"
          ? `.${el.className.trim().split(/\s+/).join(".")}`
          : ""
      }`;
    const visit = (node) => {
      if (node.nodeType === 3) {
        if (GLYPH.test(node.data)) {
          found.push(`text in ${describe(node.parentElement)}: ${node.data.trim()}`);
        }
        return;
      }
      if (node.nodeType !== 1) return;
      if (node.matches?.(CONTENT)) return;
      for (const name of ATTRS) {
        const value = node.getAttribute?.(name);
        if (value && GLYPH.test(value)) found.push(`${describe(node)}[${name}]: ${value}`);
      }
      for (const child of node.childNodes) visit(child);
    };
    visit(document.body);
    return found;
  });
}

test.describe("a Windows keyboard", () => {
  test.use({ userAgent: WINDOWS_UA });

  test.beforeEach(async ({ page }) => {
    await pretendPlatform(page, { platform: "Win32", userAgentDataPlatform: "Windows" });
  });

  test("no chrome surface shows a key this keyboard does not have", async ({
    page,
    consoleErrors,
  }) => {
    await gotoEditor(page);

    // Control: prove the collector can see a glyph at all. A sweep that returns
    // [] because it is walking nothing would otherwise pass forever.
    await page.evaluate(() => {
      const probe = document.createElement("button");
      probe.id = "glyphProbe";
      probe.title = "Probe (⌘J)";
      document.body.appendChild(probe);
    });
    expect(await collectGlyphs(page)).toEqual([
      "button#glyphProbe[title]: Probe (⌘J)",
    ]);
    await page.evaluate(() => document.getElementById("glyphProbe")?.remove());

    // The whole document, hidden elements included — the palette's chip and the
    // paste-options buttons are both offscreen until something summons them.
    expect(await collectGlyphs(page), "at rest").toEqual([]);

    for (const tab of ["insert", "layout", "references", "view", "review", "home"]) {
      await page.locator(`.ribbon-tab[data-tab="${tab}"]`).click();
      expect(await collectGlyphs(page), `${tab} ribbon tab`).toEqual([]);
    }

    for (const menu of ["file", "edit", "view", "insert", "format", "table", "review"]) {
      await openAppMenu(page, menu);
      expect(await collectGlyphs(page), `${menu} menu`).toEqual([]);
      await page.keyboard.press("Escape");
    }

    await openCommandPalette(page);
    await expect(page.locator("#cmdList .cmd-item").first()).toBeVisible();
    expect(await collectGlyphs(page), "command palette").toEqual([]);
    await page.keyboard.press("Escape");

    await runFilePageCommand(page, "help.shortcuts");
    expect(await collectGlyphs(page), "keyboard shortcuts dialog").toEqual([]);
    await page.keyboard.press("Escape");

    expect(consoleErrors).toEqual([]);
  });

  test("the palette advertises itself with the chord this keyboard can press", async ({
    page,
  }) => {
    await gotoEditor(page);
    // Derived, never hardcoded: the same formatter the product renders with,
    // asked for the platform this test is pretending to be.
    await expect(page.locator(".cmd-kbd")).toHaveText(
      formatShortcut("⌘⇧P", STANDARD_PLATFORM),
    );
    await expect(page.locator("#pasteOptionsTextOnly")).toHaveAttribute(
      "title",
      `Keep text only (${formatShortcut("⌘⇧V", STANDARD_PLATFORM)})`,
    );
  });

  test("a tooltip still shows the shortcut after the label is localized", async ({
    page,
  }) => {
    // The ribbon tooltip used to decide "is this parenthetical a shortcut?" by
    // looking for a glyph. Localizing the titles removes the glyph, so a
    // glyph-only test would silently drop the chord from the tooltip on exactly
    // the platform this work is for.
    await gotoEditor(page);
    await page.locator("#bold").hover();
    const tooltip = page.locator(".ribbon-tooltip");
    await expect(tooltip).toBeVisible({ timeout: 5_000 });
    await expect(tooltip.locator("kbd")).toHaveText(
      formatShortcut("⌘B", STANDARD_PLATFORM),
    );
  });
});

test.describe("an Apple keyboard", () => {
  test.use({ userAgent: MAC_UA });

  test.beforeEach(async ({ page }) => {
    await pretendPlatform(page, { platform: "MacIntel", userAgentDataPlatform: "macOS" });
  });

  test("keeps the glyphs, which are the platform convention", async ({ page }) => {
    // The other half of the guarantee: the sweep is conditional. If it ever
    // becomes unconditional, Mac users lose the notation their OS uses
    // everywhere else.
    await gotoEditor(page);
    await expect(page.locator(".cmd-kbd")).toHaveText("⌘⇧P");
    expect((await collectGlyphs(page)).length).toBeGreaterThan(0);
  });
});
