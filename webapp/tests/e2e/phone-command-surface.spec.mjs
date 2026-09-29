// What a phone can reach, and how (docs/148 §5.3a and §9 items 3, 4 and 8).
//
// This rung withholds two regions that the first version of the phone tier kept
// against all three references — the rail and the ruler — and it replaces the
// desktop's thirteen-group compact bar with Docs' row over two named sheets. Both
// changes trade a visible control for a menu home, so both are only safe while the
// menu home exists. That is what this file measures.
//
// It runs in the `phone` Playwright project (it is a `phone-*.spec.mjs`), so it
// gets `isMobile`, `hasTouch` and a real device scale factor rather than a narrow
// desktop window — which, before that project existed, is all any "phone" spec in
// this suite was ever getting (docs/148 §9 item 5).
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

const PHONE = { width: 390, height: 844 };
const NARROW = { width: 320, height: 568 };

/** WCAG 2.5.8 Target Size (Minimum), Level AA — the same 24 `phone_chrome.mjs`
 *  exports as `MIN_TOUCH_TARGET_PX`. Restated here because a spec asserting a
 *  floor it imports from the thing under test proves nothing. */
const MIN_TOUCH_TARGET_PX = 24;

const openSheet = async (page, trigger, surface) => {
  await page.locator(trigger).click();
  await expect(page.locator(surface)).toBeVisible();
};

/** Puts the caret inside the fixture's table, by looking for it rather than by
 *  a hard-coded offset — see the call site for why that matters at this rung.
 *  The contextual Table group appearing IS the signal that the caret arrived. */

test.describe("the two sheets", () => {
  test("Aa and + are on the bar at both phone widths, and neither is what the fold spends", async ({
    page,
    consoleErrors,
  }) => {
    // This is the guard the screenshot earned. The first roster put B/I/U before
    // the two sheets and pinned all four; `reflow()` spends groups RIGHT TO LEFT,
    // so at 390px the bar rendered undo · redo · B · I · U · + · ⋯ with **Aa
    // folded and + inline** — the most important control on a phone paid for the
    // fold. Nothing could have failed: the bar had not scrolled, nothing was off
    // window, and every command was still reachable through a `⋯` nobody would
    // open looking for formatting.
    for (const size of [PHONE, NARROW]) {
      await page.setViewportSize(size);
      await gotoEditor(page);
      await expect(page.locator("body")).toHaveClass(/phone-mode/);

      for (const id of ["#compactFormatBtn", "#compactInsertBtn"]) {
        const button = page.locator(id);
        await expect(button, `${id} is on the bar at ${size.width}px`).toBeVisible();
        const box = await button.boundingBox();
        expect(box.height, `${id} is a touch target at ${size.width}px`).toBeGreaterThanOrEqual(
          MIN_TOUCH_TARGET_PX,
        );
        expect(box.width, `${id} is a touch target at ${size.width}px`).toBeGreaterThanOrEqual(
          MIN_TOUCH_TARGET_PX,
        );
        // Inline on the BAR, not inside the overflow menu.
        const inBar = await button.evaluate((el) => !!el.closest("#compactToolbar"));
        expect(inBar, `${id} is inline at ${size.width}px, not folded into ⋯`).toBe(true);
      }
    }

    // …and the STRUCTURAL half, which is the half that can still fail.
    //
    // Measured honestly: with the rail, the ruler and the zoom readout
    // withheld, this bar has slack at both widths, so today nothing folds and
    // the three assertions above hold however the roster is ordered — I tried,
    // and reordering the table red-handed left them green. An assertion that
    // cannot fail is worth less than none (`SKILL.md` §4), so the invariant
    // that actually prevented the defect is asserted directly: `reflow()`
    // spends groups RIGHT TO LEFT and pinned ones last, so a sheet is safe from
    // the fold exactly while it is pinned and declared before everything
    // unpinned. Break that and this goes red on the next line of data anyone
    // writes, instead of on the next phone narrow enough to notice.
    const order = await page.evaluate(async () => {
      const mod = await import("/src/compact_toolbar.mjs");
      return mod.PHONE_TOOLBAR.map((group) => ({ group: group.group, pinned: !!group.pinned }));
    });
    const sheets = ["formatSheet", "insertSheet"];
    const firstLoose = order.findIndex((g) => !g.pinned);
    for (const name of sheets) {
      const at = order.findIndex((g) => g.group === name);
      expect(at, `${name} is in PHONE_TOOLBAR`).toBeGreaterThanOrEqual(0);
      expect(order[at].pinned, `${name} is pinned, so the fold reaches it last`).toBe(true);
      expect(
        at,
        `${name} is declared before every unpinned group, so the fold reaches those first`,
      ).toBeLessThan(firstLoose === -1 ? order.length : firstLoose);
    }

    expect(consoleErrors).toEqual([]);
  });

  test("the Aa sheet opens at the bottom, full width, with Style, Font and Size at the top", async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize(PHONE);
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    await openSheet(page, "#compactFormatBtn", "#compactFormatMenu");

    const sheet = await page.locator("#compactFormatMenu").boundingBox();
    // A sheet, not a popover anchored mid-screen: the first version of this rung
    // styled `.app-menu-popover` and `.compact-overflow-menu` as sheets and left
    // `.compact-command-menu` a floating card in the middle of the document.
    expect(sheet.x, "the sheet starts at the window's edge").toBeLessThanOrEqual(1);
    expect(sheet.width, "the sheet spans the window").toBeGreaterThanOrEqual(PHONE.width - 1);
    expect(sheet.y + sheet.height, "the sheet is at the bottom").toBeGreaterThan(PHONE.height / 2);

    // Google Docs' Aa panel opens with Style, Font and Size (support answer
    // 1663349). They are value pickers rather than commands — the ribbon that
    // owns them is hidden in compact chrome — so without this strip a phone has
    // no font, no size and no paragraph style at all.
    for (const id of ["#stylesTrigger", "#fontFamily", "#fontSize"]) {
      const inSheet = await page
        .locator(id)
        .evaluate((el) => !!el.closest("#compactFormatMenu"));
      expect(inSheet, `${id} is in the Aa sheet`).toBe(true);
    }

    // Adopted, never cloned: exactly one of each in the whole document, which is
    // what keeps the sheet and the ribbon from disagreeing about the font.
    for (const id of ["#stylesTrigger", "#fontFamily", "#fontSize"]) {
      expect(await page.locator(id).count(), `${id} exists once`).toBe(1);
    }
    expect(consoleErrors).toEqual([]);
  });

  test("the + sheet offers insertion, and the comment command with it", async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize(PHONE);
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    await openSheet(page, "#compactInsertBtn", "#compactInsertMenu");

    const ids = await page
      .locator("#compactInsertMenu [data-command-id]")
      .evaluateAll((els) => els.map((el) => el.dataset.commandId));
    expect(ids).toContain("insert.table");
    expect(ids).toContain("insert.image");
    expect(ids).toContain("insert.link");
    // The roster carries no separate comment button precisely because this sheet
    // already holds the command; if that stops being true the button has to come
    // back rather than the capability quietly leaving the bar.
    expect(ids, "the + sheet carries Comment, which is why the bar has no comment button").toContain(
      "review.comment",
    );
    expect(consoleErrors).toEqual([]);
  });

  test("every command the phone roster declares actually renders", async ({
    page,
    consoleErrors,
  }) => {
    // `compact-toolbar.spec.mjs` holds this property for the DESKTOP roster at
    // 1280px and cannot see this one: a phone draws a different table. Read the
    // declaration from the module rather than restating it, so a row added to
    // `PHONE_TOOLBAR` is covered on arrival — which is how the desktop bar once
    // shipped with no bulleted and no numbered list button.
    await page.setViewportSize(PHONE);
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    // Open each sheet once so its rows are in the DOM under this selection.
    //
    // They have to be OPENED rather than trusted to be pre-filled, and that is
    // worth recording: `ensureMenu` fills a menu once at construction as well as
    // on every open, so that "anything that asks what this bar offers" can read
    // it — but on a phone the bar first renders on `doc-loaded`, BEFORE there is
    // a caret, so at that moment the registry answers no `table.*` command and
    // the Table sheet is built empty. The desktop spec never sees this because
    // its helper switches chrome after clicking into the page. A user is
    // unaffected (every open refills from the live registry); a guard that read
    // the stale state would have reported a phony absence.
    await openSheet(page, "#compactFormatBtn", "#compactFormatMenu");
    await page.keyboard.press("Escape");
    await openSheet(page, "#compactInsertBtn", "#compactInsertMenu");
    await page.keyboard.press("Escape");
    // The Table sheet is the interesting one and it is NOT on the bar here.
    // It is contextual (the caret must be in a table, exactly as the ribbon's
    // Table tab is), and on a phone the bar has no room for a sixth group even
    // then, so it folds into `⋯`. That is not a hole: the compact chrome's
    // Table MENU carries every `table.*` row, present and disabled with its
    // reason (`docs/105` UX-012), and on a phone the menu bar is the navigation
    // axis (doc 122). So the menu is where this guard looks for them — which is
    // the reachability question it is actually asking, rather than a question
    // about which of two surfaces happened to render first.
    await page.locator('.app-menu-button[data-menu="table"]').click();
    await expect(page.locator("#appMenuPopover")).toBeVisible();

    const missing = await page.evaluate(async () => {
      const mod = await import("/src/compact_toolbar.mjs");
      const declared = mod.compactCommandIds(mod.PHONE_TOOLBAR);
      const present = new Set(
        [
          ...document.querySelectorAll(
            "#compactToolbar [data-command-id], #compactFormatMenu [data-command-id], " +
              "#compactInsertMenu [data-command-id], #compactTableMenu [data-command-id], " +
              "#compactOverflowMenu [data-command-id]",
          ),
        ].map((el) => el.dataset.commandId),
      );
      // The menu bar's rows use `data-command`, not `data-command-id`.
      for (const el of document.querySelectorAll("#appMenuPopover .app-menu-item[data-command]")) {
        present.add(el.dataset.command);
      }
      return declared.filter((id) => !present.has(id));
    });
    expect(
      missing,
      "a command id in PHONE_TOOLBAR that the registry does not answer renders NOTHING and says nothing",
    ).toEqual([]);
    expect(consoleErrors).toEqual([]);
  });
});

test.describe("the regions a phone withholds", () => {
  test("the rail and the ruler are not painted, and their capabilities still have homes", async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize(PHONE);
    await gotoEditor(page);
    await expect(page.locator("body")).toHaveClass(/phone-mode/);

    // Withheld. `docs/148` §5.3a: both survived the first version of this rung
    // only because `#railPages` and `setTabStop` each had exactly one surface.
    await expect(page.locator(".rail")).toBeHidden();
    await expect(page.locator(".ruler")).toBeHidden();

    // And the panels they used to open are still HERE — this hides the rail's
    // tiles, not its destinations. A host that withholds the `rail` REGION is a
    // different thing and takes the panels with it.
    await expect(page.locator("#pagesPanel")).toHaveCount(1);
    await expect(page.locator("#outlinePanel")).toHaveCount(1);

    // Pages, from the View menu, which is the home that made withholding safe.
    await page.locator('.app-menu-button[data-menu="view"]').click();
    const row = page.locator('#appMenuPopover .app-menu-item[data-command="view.pages"]');
    await expect(row, "View ▸ Pages is the Pages panel's home on a phone").toBeVisible();
    await row.click();
    await expect(page.locator("#pagesPanel")).toBeVisible();

    expect(consoleErrors).toEqual([]);
  });

  test("the status bar does not overflow once the zoom readout is withheld", async ({
    page,
    consoleErrors,
  }) => {
    // Found by `phone-no-horizontal-scroll.spec.mjs` rather than by design: the
    // status bar had been overflowing at 320px all along, and the desktop compact
    // bar was masking it by adopting `#zoom` out of there. A phone roster that
    // did not adopt it exposed `footer.footer` at scrollWidth 344 against
    // clientWidth 320.
    for (const size of [PHONE, NARROW]) {
      await page.setViewportSize(size);
      await gotoEditor(page);
      const footer = await page.locator(".footer").evaluate((el) => ({
        scrollWidth: el.scrollWidth,
        clientWidth: el.clientWidth,
      }));
      expect(
        footer.scrollWidth,
        `the status bar fits at ${size.width}px`,
      ).toBeLessThanOrEqual(footer.clientWidth + 1);
      await expect(page.locator("#zoom")).toBeHidden();
    }
    expect(consoleErrors).toEqual([]);
  });
});

test("the toast clears an open bottom sheet as well as the command bar", async ({
  page,
  consoleErrors,
}) => {
  // `docs/148` §9 item 8. The toast owns the bottom-start corner and `--z-toast`
  // sits above `--z-modal` on purpose, so a sheet and a toast share a corner and
  // the toast wins — invisibly, because the toast is `pointer-events: none` and
  // nothing about clicking the sheet can fail. §9 recorded it unfixed, with the
  // reason that reserving a band above a sheet needs the sheet's height and that
  // is content-dependent. True, and an argument against one answer rather than
  // all of them: where the sheet is NOT is knowable, so while a sheet is open the
  // toast moves to the top of the screen, under the measured header height.
  await page.setViewportSize(PHONE);
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openSheet(page, "#compactFormatBtn", "#compactFormatMenu");

  await page.evaluate(() => {
    const toast = document.querySelector(".toast");
    toast.textContent = "A status message long enough to be a real card";
    toast.classList.add("is-shown");
    toast.hidden = false;
  });

  const toast = await page.locator(".toast").boundingBox();
  const sheet = await page.locator("#compactFormatMenu").boundingBox();
  const header = await page.locator("header.bar").boundingBox();
  expect(toast, "the toast is on screen to be measured").not.toBeNull();
  expect(
    toast.y + toast.height,
    "the toast must sit clear above the open sheet, not across it",
  ).toBeLessThanOrEqual(sheet.y + 1);
  // And not under the header it moved up to avoid the sheet — which is the same
  // defect one row up, and is what the first version of the rule did, because it
  // used `--h-header: 63px` while this rung's header is two rows at 390px.
  expect(
    toast.y,
    "the toast must clear the wrapped menu bar, not land across it",
  ).toBeGreaterThanOrEqual(header.y + header.height - 1);

  expect(consoleErrors).toEqual([]);
});

test("the toast clears an open dialog's pinned actions too", async ({ page, consoleErrors }) => {
  // The screenshot half of the rule above, and it is the case that was WRONG on
  // the first pass: a modal was excluded on the theory that an 86vh cap leaves
  // no band above the card. That is the cap, not the height — the Tab stops
  // sheet stands about 60% tall at 390px — and "Rendering 1 page at 100%…" lay
  // straight across its Set and Clear all buttons, which is `109` HF-233
  // exactly: the drawer's pinned action must stay readable.
  await page.setViewportSize({ width: 390, height: 844 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('.app-menu-button[data-menu="format"]').click();
  await page.locator('#appMenuPopover .app-menu-item[data-command="layout.tabStops"]').click();
  await expect(page.locator("#tabStopsDialog")).toBeVisible();

  await page.evaluate(() => {
    const toast = document.querySelector(".toast");
    toast.textContent = "A status message long enough to be a real card";
    toast.classList.add("is-shown");
    toast.hidden = false;
  });

  const overlap = await page.evaluate(() => {
    const box = (selector) => document.querySelector(selector).getBoundingClientRect();
    const hits = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
    const toast = box(".toast");
    return [...document.querySelectorAll("#tabStopsDialog .dialog-button")]
      .filter((el) => hits(toast, el.getBoundingClientRect()))
      .map((el) => el.textContent.trim());
  });
  expect(overlap, "the toast must not land on a dialog's buttons").toEqual([]);

  expect(consoleErrors).toEqual([]);
});
