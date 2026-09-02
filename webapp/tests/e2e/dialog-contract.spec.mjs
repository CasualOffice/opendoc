// One modal contract, asserted against every modal (docs/104 HF-043, HF-062,
// HF-063, HF-070, HF-089).
//
// The tracker's finding was not "Split cell is broken" — it was that nine
// dialogs had three different dismissal contracts, so there was no rule for a
// user to learn and no rule for a reviewer to check. `webapp/tests/e2e` had no
// dialog-dismissal spec at all, which is why a dialog could ship without
// Escape, without backdrop dismissal, and without a focus trap, and nothing
// turned red.
//
// So this spec is written against the CONTRACT rather than against any one
// dialog: a table of every `aria-modal` surface in the editor, driven through
// the same four assertions. The last test closes the loop — if a new
// `aria-modal` element appears in editor.html and is not in the table, this
// spec fails, so the next dialog cannot ship without the contract either.
import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";

async function openPalette(page) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
}

async function runFromPalette(page, query, label) {
  await openPalette(page);
  await page.locator("#cmdInput").fill(query);
  await page.locator(".cmd-item", { hasText: label }).first().click();
}

async function insertTwoByTwoTable(page) {
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  await page.locator("#tabTable").click();
}

// Every modal surface, with the real user route that reaches it and the control
// that must hold focus once it is open. `opener` is the chrome focus has to
// come back to; null means the route leaves no focusable opener on screen (the
// palette runs a command and closes itself), in which case the contract only
// requires that focus lands somewhere inside the editor rather than on <body>.
const MODALS = [
  {
    id: "propertiesPanel",
    name: "Document properties",
    opener: "#propertiesBtn",
    focus: "#propTitle",
    async open(page) {
      await gotoEditor(page);
      await page.locator("#propertiesBtn").click();
    },
  },
  {
    id: "pageSetupMenu",
    name: "Page setup",
    opener: "#pageSetupBtn",
    focus: '#pageOrientationSeg button[aria-pressed="true"]',
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.locator("#tabView").click();
      await page.locator("#pageSetupBtn").click();
    },
  },
  {
    id: "splitCellDialog",
    name: "Split cell",
    opener: null,
    restore: "#pages",
    focus: "#splitCellColumns",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await insertTwoByTwoTable(page);
      await page.locator("#splitCellBtn").click();
    },
  },
  {
    id: "styleNameDialog",
    name: "Create a style",
    opener: null,
    focus: "#styleNameInput",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.keyboard.press(`${MOD}+Home`);
      await page.keyboard.press("Shift+End");
      await runFromPalette(page, "Create style from selection", "Create style from selection");
    },
  },
  {
    id: "bookmarkDialog",
    name: "Bookmark manager",
    opener: null,
    focus: "#bookmarkNameInput",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await runFromPalette(page, "Bookmark", "Bookmark…");
    },
  },
  {
    id: "fieldDialog",
    name: "Insert field",
    opener: null,
    focus: ".field-choice",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.locator('.app-menu-button[data-menu="insert"]').click();
      await page.locator('#appMenuPopover .app-menu-item[data-command="insert.field"]').click();
    },
  },
  {
    id: "linkDialog",
    name: "Insert link",
    opener: null,
    focus: "#linkUrlInput",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.keyboard.press(`${MOD}+Home`);
      for (let i = 0; i < 4; i += 1) await page.keyboard.press("Shift+ArrowRight");
      await page.keyboard.press(`${MOD}+k`);
    },
  },
  {
    id: "altTextDialog",
    name: "Alt text",
    // The object context bar is rebuilt on every repaint, so the button that
    // opened this is a detached node by the time it closes.
    opener: null,
    restore: "#pages",
    focus: "#altTextInput",
    async open(page) {
      await page.goto("/editor.html?fixture=float");
      await page.waitForFunction(
        () => {
          const s = document.getElementById("status");
          return s && s.textContent === "" && document.querySelectorAll(".page-wrap").length > 0;
        },
        null,
        { timeout: 45_000 },
      );
      const canvas = page.locator(".page-wrap .page").first();
      const box = await canvas.boundingBox();
      await canvas.click({ position: { x: box.width * 0.14, y: box.height * 0.11 } });
      await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
      await page.locator('.object-bar-btn[aria-label="Edit alt text"]').click();
    },
  },
  {
    id: "confirmDialog",
    name: "Discard confirmation",
    opener: null,
    focus: "#confirmCancel",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await page.keyboard.type("dirty");
      await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");
      await page.locator("#file").setInputFiles("sample.docx");
    },
  },
  {
    id: "cmdPalette",
    name: "Command palette",
    opener: null,
    focus: "#cmdInput",
    async open(page) {
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await openPalette(page);
    },
  },
];

for (const modal of MODALS) {
  const dialog = (page) => page.locator(`#${modal.id}`);

  test(`${modal.name}: opening moves focus in, Escape closes and restores it`, async ({
    page,
    consoleErrors,
  }) => {
    await modal.open(page);
    await expect(dialog(page)).toBeVisible();
    await expect(page.locator(modal.focus).first()).toBeFocused();

    await page.keyboard.press("Escape");
    await expect(dialog(page)).toBeHidden();
    if (modal.opener) {
      await expect(page.locator(modal.opener).first()).toBeFocused();
    } else if (modal.restore) {
      await expect(page.locator(modal.restore)).toBeFocused();
    } else {
      // No opener survives the route, so the requirement is the weaker but
      // still load-bearing one: the keyboard must not be left on <body>, which
      // is what made the editor look frozen after Split cell (HF-062).
      const active = await page.evaluate(() => document.activeElement?.tagName ?? "");
      expect(active).not.toBe("BODY");
    }
    expect(consoleErrors).toEqual([]);
  });

  test(`${modal.name}: a backdrop click dismisses it`, async ({ page, consoleErrors }) => {
    await modal.open(page);
    await expect(dialog(page)).toBeVisible();
    // Press and release on the scrim itself — the top-left corner is outside
    // every dialog card, which is centred.
    await dialog(page).click({ position: { x: 4, y: 4 } });
    await expect(dialog(page)).toBeHidden();
    expect(consoleErrors).toEqual([]);
  });

  test(`${modal.name}: Tab cycles inside it and cannot leave`, async ({
    page,
    consoleErrors,
  }) => {
    await modal.open(page);
    await expect(dialog(page)).toBeVisible();
    // Enough presses to walk past the end of any of these dialogs and wrap.
    const visited = new Set();
    for (let i = 0; i < 30; i += 1) {
      await page.keyboard.press("Tab");
      const seen = await page.evaluate((id) => {
        const active = document.activeElement;
        return {
          inside: document.getElementById(id).contains(active),
          // Identity of the focused control, so the test can tell "Tab cycles
          // through the dialog" from "Tab escapes and something drags focus
          // back to the first control every time".
          at: active?.id || active?.getAttribute("aria-label") || active?.textContent?.trim().slice(0, 24) || "?",
        };
      }, modal.id);
      expect(seen.inside, `focus left #${modal.id} after ${i + 1} Tab presses`).toBe(true);
      visited.add(seen.at);
    }
    const focusableCount = await page.evaluate(
      (id) =>
        [
          ...document
            .getElementById(id)
            .querySelectorAll(
              "a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])",
            ),
        ].filter((node) => node.getClientRects().length > 0).length,
      modal.id,
    );
    if (focusableCount > 1) {
      expect(visited.size, `Tab never moved off one control in #${modal.id}`).toBeGreaterThan(1);

      // Backwards off the front must wrap to the BACK. Containment alone would
      // put focus on the first control again, which reads as Shift+Tab being
      // broken; the cycle has to run both ways to be a cycle.
      const wrapped = await page.evaluate((id) => {
        const items = [
          ...document
            .getElementById(id)
            .querySelectorAll(
              "a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])",
            ),
        ].filter((node) => node.getClientRects().length > 0);
        items[0].focus();
        return { first: items[0] === document.activeElement, last: items[items.length - 1] };
      }, modal.id);
      expect(wrapped.first).toBe(true);
      await page.keyboard.press("Shift+Tab");
      const atLast = await page.evaluate((id) => {
        const items = [
          ...document
            .getElementById(id)
            .querySelectorAll(
              "a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])",
            ),
        ].filter((node) => node.getClientRects().length > 0);
        return items[items.length - 1] === document.activeElement;
      }, modal.id);
      expect(atLast, `Shift+Tab off the first control did not wrap to the last in #${modal.id}`).toBe(true);
    }
    expect(consoleErrors).toEqual([]);
  });

  test(`${modal.name}: application shortcuts do not fire behind it`, async ({
    page,
    consoleErrors,
  }) => {
    await modal.open(page);
    await expect(dialog(page)).toBeVisible();
    // HF-063: ⌘F used to open Find behind the scrim and take focus with it.
    await page.keyboard.press(`${MOD}+f`);
    await expect(page.locator("#findPanel")).toBeHidden();
    const inside = await page.evaluate(
      (id) => document.getElementById(id).contains(document.activeElement),
      modal.id,
    );
    expect(inside).toBe(true);
    expect(consoleErrors).toEqual([]);
  });
}

test("every aria-modal surface in the editor is covered by this spec", async ({ page }) => {
  await gotoEditor(page);
  const declared = await page.evaluate(() =>
    [...document.querySelectorAll('[aria-modal="true"]')].map(
      // The palette's aria-modal lives on the inner .cmd-box; the overlay it
      // sits in is the element the contract is registered against.
      (element) => (element.id || element.closest("[id]")?.id) ?? "",
    ),
  );
  const covered = MODALS.map((modal) => modal.id).sort();
  expect(declared.filter(Boolean).sort()).toEqual(covered);
});

test("body scroll is locked while a modal is open and released after", async ({ page }) => {
  await gotoEditor(page);
  await expect(page.locator("body")).not.toHaveClass(/modal-open/);
  await page.locator("#propertiesBtn").click();
  await expect(page.locator("body")).toHaveClass(/modal-open/);
  await page.keyboard.press("Escape");
  await expect(page.locator("body")).not.toHaveClass(/modal-open/);
});

test("a modal paints above the review chrome, and takes it off screen", async ({ page }) => {
  await gotoEditor(page);
  const ladder = await page.evaluate(() => {
    const value = (name) =>
      Number(getComputedStyle(document.documentElement).getPropertyValue(name).trim());
    return {
      modal: value("--z-modal"),
      palette: value("--z-palette"),
      reviewPopover: value("--z-review-popover"),
      reviewCard: value("--z-review-card"),
      inspector: value("--z-inspector"),
    };
  });
  // HF-089: the pinned tracked-change card used to sit at 85 over a dialog at
  // 70, so Accept/Reject stayed live above a blocking modal.
  expect(ladder.modal).toBeGreaterThan(ladder.palette);
  expect(ladder.palette).toBeGreaterThan(ladder.reviewCard);
  expect(ladder.reviewCard).toBeGreaterThan(ladder.reviewPopover);
  expect(ladder.reviewPopover).toBeGreaterThan(ladder.inspector);
});
