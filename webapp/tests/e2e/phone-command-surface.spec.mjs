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
import {
  appMenuButton,
  clickIntoFirstPage,
  expect,
  gotoEditor,
  menuCommandRow,
  stableBox,
  test,
} from "./fixtures.mjs";
import { rawKeyLeaks } from "./raw-keys.mjs";

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
        const box = await stableBox(button);
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

    const sheet = await stableBox(page.locator("#compactFormatMenu"));
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

  test("the Aa sheet says what is on, and keeps saying it while it is open", async ({
    page,
    consoleErrors,
  }) => {
    // Six plain rows — Bold, Italic, Underline, Strikethrough, Superscript,
    // Subscript — and nothing to say whether the text was already bold, when
    // tapping Bold over bold text REMOVES it. Google Docs' Aa panel opens with
    // B / I / U / S as toggles that light up; this holds the sheet to that.
    await page.setViewportSize(PHONE);
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    // PRECONDITION, stated rather than assumed: the tap lands in the fixture's
    // bold "CASUALOFFICE" eyebrow, so the caret's text IS bold — the state under
    // test is the document's, as the editor's own ribbon reports it, and not
    // something this spec wrote into the DOM. If the fixture moves, this fails
    // here and says why, instead of the assertions below passing on "off".
    await expect(page.locator("#bold"), "the caret starts in bold text").toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect(page.locator("#italic")).toHaveAttribute("aria-pressed", "false");
    await openSheet(page, "#compactFormatBtn", "#compactFormatMenu");

    const strip = page.locator("#compactFormatMenu .menu-toggle-strip");
    await expect(strip).toBeVisible();
    const ids = await strip.locator("[data-command-id]").evaluateAll((els) => els.map((el) => el.dataset.commandId));
    expect(ids, "Docs' toggle row, in the band's own order").toEqual([
      "format.bold",
      "format.italic",
      "format.underline",
      "format.strike",
      "format.superscript",
      "format.subscript",
    ]);
    const bold = strip.locator('[data-command-id="format.bold"]');
    const italic = strip.locator('[data-command-id="format.italic"]');
    await expect(bold).toHaveAttribute("role", "menuitemcheckbox");
    await expect(bold, "the sheet opens saying bold is on").toHaveAttribute("aria-checked", "true");
    await expect(italic).toHaveAttribute("aria-checked", "false");

    // Drawn, not only announced: lit and unlit must not paint the same.
    const paint = (locator) => locator.evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(await paint(bold), "a checked toggle is drawn differently from an unchecked one").not.toBe(
      await paint(italic),
    );
    for (const toggle of await strip.locator("[data-command-id]").all()) {
      const box = await stableBox(toggle);
      expect(box.width).toBeGreaterThanOrEqual(MIN_TOUCH_TARGET_PX);
      expect(box.height).toBeGreaterThanOrEqual(MIN_TOUCH_TARGET_PX);
    }
    // Every icon is a GLYPH. A ligature the self-hosted face lacks renders as
    // its own name in text — "format_strikethrough", ~10x wider than one glyph —
    // which is worse than a wrong icon and passes every other check here.
    const icons = await strip.locator(".ms").evaluateAll((els) =>
      els.map((el) => ({
        name: el.textContent,
        width: el.getBoundingClientRect().width,
        size: Number.parseFloat(getComputedStyle(el).fontSize),
      })),
    );
    for (const icon of icons) {
      expect(icon.width, `${icon.name} renders as one glyph`).toBeLessThanOrEqual(icon.size * 1.5);
    }

    // LIVE: tapping a toggle keeps the sheet up (Docs' panel does) and the
    // toggle flips in place — no reopen, no stale answer.
    await bold.click();
    await expect(page.locator("#compactFormatMenu")).toBeVisible();
    await expect(bold, "the toggle follows the caret's state while the sheet is open").toHaveAttribute(
      "aria-checked",
      "false",
    );
    await italic.click();
    await expect(italic).toHaveAttribute("aria-checked", "true");

    // The one-of-four rows say which one: the caret's paragraph is left-aligned.
    const start = page.locator('#compactFormatMenu [data-command-id="paragraph.align.start"]');
    await expect(start).toHaveAttribute("role", "menuitemradio");
    await expect(start).toHaveAttribute("aria-checked", "true");
    await expect(page.locator('#compactFormatMenu [data-command-id="paragraph.align.center"]')).toHaveAttribute(
      "aria-checked",
      "false",
    );

    // The ORDER case. `updateToolbar` refills an open sheet before it writes
    // the list buttons' state, so a sheet that only refreshed on refill would
    // still say "not a list" after this tap; the row has to hear the write
    // itself.
    const bullets = page.locator('#compactFormatMenu [data-command-id="paragraph.list.bullet"]');
    await expect(bullets).toHaveAttribute("role", "menuitemcheckbox");
    await expect(bullets).toHaveAttribute("aria-checked", "false");
    await bullets.click();
    await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "true");
    await expect(page.locator("#compactFormatMenu")).toBeVisible();
    await expect(bullets, "the list row follows the paragraph it just changed").toHaveAttribute(
      "aria-checked",
      "true",
    );
    await expect(bullets.locator(".menu-check")).toHaveCSS("opacity", "1");
    expect(consoleErrors).toEqual([]);
  });

  test("a command run from a sheet closes the sheet first: the comment is not typed blind", async ({
    page,
    consoleErrors,
  }) => {
    // `phone-07a`: Add comment from the + sheet focused the comment composer
    // UNDER the sheet, which stayed open across it, so the comment was typed into
    // a box nobody could see. A menu closes before its command runs — the app
    // menu bar always did — and these sheets did not.
    await page.setViewportSize(PHONE);
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    // A comment needs text to hang off ("Select text to comment on").
    await page.keyboard.press("End");
    await page.keyboard.press("Shift+Home");
    await openSheet(page, "#compactInsertBtn", "#compactInsertMenu");
    const add = page.locator('#compactInsertMenu [data-command-id="review.comment"]');
    await expect(add).toBeEnabled();
    await add.click();

    await expect(page.locator("#compactInsertMenu"), "the + sheet is gone").toBeHidden();
    await expect(page.locator("#compactInsertBtn")).toHaveAttribute("aria-expanded", "false");
    const composer = page.locator('[data-testid="review-comment-composer"]:visible').first();
    await expect(composer).toBeVisible();
    await expect(composer).toBeFocused();
    // And it is what is ON TOP at its own centre — a sheet left open over it
    // would be the element a finger lands on.
    const covered = await composer.evaluate((el) => {
      const r = el.getBoundingClientRect();
      const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      return hit && !el.contains(hit) && !hit.contains(el) ? hit.id || hit.className : null;
    });
    expect(covered, "nothing paints over the focused composer").toBeNull();
    expect(consoleErrors).toEqual([]);
  });

  test("the table-size grid is a sheet a finger can use, not a 15px grid across the header", async ({
    page,
    consoleErrors,
  }) => {
    // `phone-08-insert-table.png`: Insert table from the + sheet put an 8x10
    // grid of 15x15px cells at y 4 — across the title and the menu bar — because
    // the row it was opened from had gone and the popover fell back to the
    // ribbon's hidden button for an anchor. Both halves are measured here: where
    // the grid is, and whether a finger can hit what is in it.
    for (const size of [PHONE, NARROW]) {
      await page.setViewportSize(size);
      await gotoEditor(page);
      await clickIntoFirstPage(page);
      await openSheet(page, "#compactInsertBtn", "#compactInsertMenu");
      await page.locator('#compactInsertMenu [data-command-id="insert.table"]').click();
      const grid = page.locator("#insertTableMenu");
      await expect(grid).toBeVisible();

      const box = await stableBox(grid);
      const header = await stableBox(page.locator("header.bar"));
      const bar = await stableBox(page.locator("#compactToolbar"));
      expect(box.x, `the grid starts inside the window at ${size.width}px`).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width, `the grid ends inside the window at ${size.width}px`).toBeLessThanOrEqual(
        size.width + 1,
      );
      expect(box.y, `the grid clears the header at ${size.width}px`).toBeGreaterThanOrEqual(
        header.y + header.height - 1,
      );
      expect(box.y + box.height, `the grid clears the command bar at ${size.width}px`).toBeLessThanOrEqual(
        bar.y + 1,
      );

      const cells = await grid.locator(".gc").evaluateAll((els) =>
        els.map((el) => {
          const r = el.getBoundingClientRect();
          return { w: r.width, h: r.height };
        }),
      );
      expect(cells.length, "the 8x10 grid").toBe(80);
      const smallest = cells.reduce((min, c) => Math.min(min, c.w, c.h), Infinity);
      expect(smallest, `every cell is a touch target at ${size.width}px`).toBeGreaterThanOrEqual(
        MIN_TOUCH_TARGET_PX,
      );

      // And it still inserts: three columns by two rows.
      await grid.locator('.gc[data-r="2"][data-c="3"]').click();
      await expect(grid).toBeHidden();
      await expect(page.locator("#tabTable"), "the caret is now in a table").toBeEnabled();
    }
    expect(consoleErrors).toEqual([]);
  });

  test("nothing on a phone is named with a catalogue key", async ({ page, consoleErrors }) => {
    // The defect this was written against: on every phone the Aa and + buttons
    // were announced as "appMenuBar.format" and "appMenuBar.insert" — their
    // `aria-label` AND `title` — because the bar rendered before the catalogue
    // that carries those keys had landed, and cached the miss. The sweep reads
    // every chrome element, not the two buttons, because the defect was a class:
    // `chrome-raw-keys.spec.mjs` holds the desktop project to the same sweep.
    for (const size of [PHONE, NARROW]) {
      await page.setViewportSize(size);
      await gotoEditor(page);
      await expect(page.locator("body")).toHaveClass(/phone-mode/);
      expect(await rawKeyLeaks(page), `at ${size.width}px`).toEqual([]);
      for (const [trigger, sheet] of [
        ["#compactFormatBtn", "#compactFormatMenu"],
        ["#compactInsertBtn", "#compactInsertMenu"],
      ]) {
        await openSheet(page, trigger, sheet);
        expect(await rawKeyLeaks(page), `at ${size.width}px with ${sheet} open`).toEqual([]);
        await page.keyboard.press("Escape");
      }
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
    await (await appMenuButton(page, "table")).click();
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

test.describe("the header", () => {
  /** The eight menus, in the bar's order — read from the markup rather than
   *  restated, so a ninth menu is covered on arrival. */
  const menuNames = (page) =>
    page.locator("#appMenuBar .app-menu-button").evaluateAll((els) => els.map((el) => el.dataset.menu));

  /** The header's budget, in CSS px. MEASURED, then given headroom, and the
   *  derivation is the point. The one row is set by the document-name field,
   *  which under a finger takes the 16px no-iOS-zoom type (`style.css`'s coarse
   *  block) and is 32px tall; with the bar's padding the header measured 38px at
   *  BOTH 390 and 320 on the Pixel 7 project (the 30px icon buttons sit inside
   *  that row). The budget is that plus 14px for a locale or a face with a
   *  taller line box — and it is deliberately smaller than one row MORE: the
   *  smallest thing a second row could hold is a 24px touch target
   *  (`MIN_TOUCH_TARGET_PX`), and 38 + 24 = 62 cannot fit under 52. The header
   *  this replaced measured 92px at 390 (`phone-09a`, a wrapped menu bar). */
  const HEADER_BUDGET_PX = 52;

  test("is one row at both phone widths, and spends no more than one row's height", async ({
    page,
    consoleErrors,
  }) => {
    for (const size of [PHONE, NARROW]) {
      await page.setViewportSize(size);
      await gotoEditor(page);
      await expect(page.locator("body")).toHaveClass(/phone-mode/);
      const header = await stableBox(page.locator("header.bar"));
      const rows = await page.evaluate(() => {
        // Every control the header paints, by its box. One row means every one
        // of them shares a horizontal band: the lowest top is above the highest
        // bottom. A second row fails that whatever the pixel numbers are.
        const shown = [...document.querySelectorAll("header.bar button, header.bar input, header.bar .document-state")]
          .filter((el) => el.getClientRects().length > 0)
          .map((el) => {
            const r = el.getBoundingClientRect();
            return { id: el.id || el.className, top: r.top, bottom: r.bottom };
          });
        return {
          shown,
          lowestTop: Math.max(...shown.map((c) => c.top)),
          highestBottom: Math.min(...shown.map((c) => c.bottom)),
          scrollWidth: document.documentElement.scrollWidth,
          clientWidth: document.documentElement.clientWidth,
          headerScroll: document.querySelector("header.bar").scrollWidth,
        };
      });
      const ids = rows.shown.map((c) => c.id);
      for (const id of ["docTitle", "appMenusBtn", "propertiesBtn", "settingsBtn"]) {
        expect(ids, `${id} is in the header at ${size.width}px`).toContain(id);
      }
      expect(
        rows.lowestTop,
        `the header's controls share one row at ${size.width}px: ${JSON.stringify(rows.shown)}`,
      ).toBeLessThan(rows.highestBottom);
      expect(header.height, `the header's height at ${size.width}px`).toBeLessThanOrEqual(HEADER_BUDGET_PX);
      expect(rows.scrollWidth, `no page scroll at ${size.width}px`).toBeLessThanOrEqual(rows.clientWidth);
      expect(rows.headerScroll, `no header scroll at ${size.width}px`).toBeLessThanOrEqual(rows.clientWidth);
      const door = await stableBox(page.locator("#appMenusBtn"));
      expect(door.width).toBeGreaterThanOrEqual(MIN_TOUCH_TARGET_PX);
      expect(door.height).toBeGreaterThanOrEqual(MIN_TOUCH_TARGET_PX);
    }
    expect(consoleErrors).toEqual([]);
  });

  test("every menu is still reachable, from a sheet of all eight names", async ({ page, consoleErrors }) => {
    for (const size of [PHONE, NARROW]) {
      await page.setViewportSize(size);
      await gotoEditor(page);
      const door = page.locator("#appMenusBtn");
      const list = page.locator("#appMenuBar");
      await expect(list, "the bar is not painted in the row").toBeHidden();
      await expect(door).toHaveAttribute("aria-expanded", "false");
      await expect(door).toHaveAttribute("aria-controls", "appMenuBar");

      const names = await menuNames(page);
      expect(names).toEqual(["file", "edit", "view", "insert", "format", "table", "references", "review"]);
      for (const name of names) {
        // Open on the first pass; on later passes Escape has already brought
        // the list back, which is part of what is being asserted.
        if ((await door.getAttribute("aria-expanded")) !== "true") await door.tap();
        await expect(list).toBeVisible();
        const button = page.locator(`#appMenuBar .app-menu-button[data-menu="${name}"]`);
        const box = await stableBox(button);
        expect(box.x, `${name} starts on screen at ${size.width}px`).toBeGreaterThanOrEqual(0);
        expect(box.x + box.width, `${name} ends on screen at ${size.width}px`).toBeLessThanOrEqual(size.width + 1);
        expect(box.y + box.height, `${name} is above the fold at ${size.width}px`).toBeLessThanOrEqual(size.height);
        expect(box.height, `${name} is a touch target`).toBeGreaterThanOrEqual(MIN_TOUCH_TARGET_PX);
        // `tap`, the phone's gesture, and not `click`: a click leaves an emulated
        // MOUSE resting where the name was, the menu sheet then opens under it,
        // and `createMenuBar`'s hover-to-open expands whichever submenu landed
        // under the pointer — a state a finger, which leaves no hovering pointer
        // behind, never produces.
        await button.tap();
        const menu = page.locator("#appMenuPopover");
        await expect(menu, `the ${name} menu opens from the sheet`).toBeVisible();
        expect(
          await menu.locator(".app-menu-item, .app-menu-item-parent").count(),
          `the ${name} menu has rows`,
        ).toBeGreaterThan(0);
        // The list steps aside while a menu is open, and Escape drills back
        // out to it with focus on the name that opened the menu.
        await expect(list).toBeHidden();
        await page.keyboard.press("Escape");
        await expect(menu).toBeHidden();
        await expect(list).toBeVisible();
        await expect(button).toBeFocused();
      }
      // A second Escape leaves the list, and focus goes back to the door.
      await page.keyboard.press("Escape");
      await expect(list).toBeHidden();
      await expect(door).toHaveAttribute("aria-expanded", "false");
      await expect(door).toBeFocused();
    }
    expect(consoleErrors).toEqual([]);
  });

  test("a command chosen from a menu closes the list it was chosen from, and a tap outside does too", async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize(PHONE);
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    const door = page.locator("#appMenusBtn");
    const list = page.locator("#appMenuBar");
    const row = await menuCommandRow(page, "view", "view.outline");
    await row.click();
    await expect(page.locator("#outlinePanel")).toBeVisible();
    await expect(page.locator("#appMenuPopover")).toBeHidden();
    await expect(list, "the list does not outlive the command chosen from it").toBeHidden();
    await expect(door).toHaveAttribute("aria-expanded", "false");

    // An outside press on chrome that takes no focus — the state chip — so the
    // press itself is what dismisses, not a focus change it happens to cause.
    await door.tap();
    await expect(list).toBeVisible();
    await page.locator("#documentState").tap();
    await expect(list, "an outside tap dismisses the list").toBeHidden();
    await expect(door).toHaveAttribute("aria-expanded", "false");

    // Focus leaving, with no press at all: from the keyboard the list takes
    // focus on File, Up walks it (wrapping to Review, the last name), and Tab
    // past the last name leaves it — which closes it.
    await door.focus();
    await page.keyboard.press("Enter");
    await expect(list).toBeVisible();
    await expect(page.locator('#appMenuBar [data-menu="file"]')).toBeFocused();
    await page.keyboard.press("ArrowUp");
    await expect(page.locator('#appMenuBar [data-menu="review"]')).toBeFocused();
    await expect(page.locator("#appMenuPopover"), "Up walks the list, it does not open a menu").toBeHidden();
    await page.keyboard.press("Tab");
    await expect(list, "focus leaving the list dismisses it").toBeHidden();
    await expect(door).toHaveAttribute("aria-expanded", "false");
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
    //
    // At this rung it arrives WITHHELD, and that is reflow's doing rather than
    // the phone's: reflow is on by default below the phone rung (ADR-046 §3.4)
    // and a page navigator over a document with no pages would card tiles
    // (`docs/151` §6.4). So the row is disabled and carries the reason — never a
    // dead control — and the capability is one toggle away, which is exactly
    // what this test exists to assert and is asserted by driving it.
    const withheld = await menuCommandRow(page, "view", "view.pages");
    await expect(withheld).toBeDisabled();
    expect(await withheld.getAttribute("title")).toMatch(/reflow/i);
    await page.keyboard.press("Escape");

    await (await menuCommandRow(page, "view", "view.reflow")).click();
    await expect
      .poll(async () =>
        page.evaluate(() => document.getElementById("viewport").classList.contains("is-reflow")),
      )
      .toBe(false);

    const row = await menuCommandRow(page, "view", "view.pages");
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

  const toast = await stableBox(page.locator(".toast"));
  const sheet = await stableBox(page.locator("#compactFormatMenu"));
  const header = await stableBox(page.locator("header.bar"));
  expect(toast, "the toast is on screen to be measured").not.toBeNull();
  expect(
    toast.y + toast.height,
    "the toast must sit clear above the open sheet, not across it",
  ).toBeLessThanOrEqual(sheet.y + 1);
  // And not under the header it moved up to avoid the sheet — which is the same
  // defect one row up, and is what the first version of the rule did, because it
  // used `--h-header: 63px` while this rung's header was then two rows at 390px.
  // It is one row now (`docs/148` §5.3b); the rule reads the MEASURED height,
  // which is why it did not have to change when the header did.
  expect(
    toast.y,
    "the toast must clear the header, not land across it",
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
  // `menuCommandRow`, not a direct click: Format's paragraph band folds into a
  // submenu, and `runAppMenuCommand` cannot be used at this rung because it
  // starts by clicking `#modeCompact`, which the phone withholds.
  await (await menuCommandRow(page, "format", "layout.tabStops")).click();
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
