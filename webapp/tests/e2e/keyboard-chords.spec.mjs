// `109` UX-006/UX-007 — the half only a browser can answer.
//
// `tests/keymap.test.mjs` proves that a chord resolves to a command id. That is
// not the same claim as "the chord does something": an id that resolves to a
// descriptor whose `run` is broken, or to a command the dispatcher never reaches
// because a guard above it returns first, would satisfy the unit test and leave a
// dead chord. SKILL.md §10 — a control that does nothing is worse than none. So
// every assertion here DRIVES the keystroke and reads the result back out of the
// document or the chrome that reflects it.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  mirrorBlocks,
  moveCaretToDocStart,
  MOD,
} from "./fixtures.mjs";

/** Which alignment the ribbon is showing as active — the engine's answer, round
 *  tripped through the toolbar's own reflection rather than through the keymap. */
function reflected(page) {
  return page.evaluate(() => {
    const pressed = (id) => document.getElementById(id)?.getAttribute("aria-pressed");
    return {
      start: pressed("alignStart"),
      center: pressed("alignCenter"),
      end: pressed("alignEnd"),
      justify: pressed("alignJustify"),
      bullets: pressed("bulletList"),
    };
  });
}

test("the alignment chords change the paragraph, not just the table", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Each of the four, and each one EXCLUSIVELY — Word's alignment is one-of-four,
  // so a chord that set center without clearing left would be a different bug
  // passing a weaker test.
  for (const [chord, expected] of [
    [`${MOD}+e`, "center"],
    [`${MOD}+r`, "end"],
    [`${MOD}+j`, "justify"],
    [`${MOD}+l`, "start"],
  ]) {
    await page.keyboard.press(chord);
    await expect
      .poll(async () => (await reflected(page))[expected], { message: `${chord} → ${expected}` })
      .toBe("true");
    const state = await reflected(page);
    for (const other of ["start", "center", "end", "justify"]) {
      if (other === expected) continue;
      expect(state[other], `${chord} left ${other} pressed as well`).not.toBe("true");
    }
  }
  expect(consoleErrors).toEqual([]);
});

test("the list and indent chords reach the engine", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.keyboard.press(`${MOD}+Shift+l`);
  await expect.poll(async () => (await reflected(page)).bullets).toBe("true");
  await page.keyboard.press(`${MOD}+Shift+l`);
  await expect.poll(async () => (await reflected(page)).bullets).toBe("false");

  // Indent is read back from the Paragraph properties panel, which fills its
  // fields from the ENGINE when it opens. Opening it each time rather than
  // assuming it reflects live is deliberate: a test that depended on live
  // reflection would be asserting the panel's refresh policy, not the chord.
  const indentAfterOpening = async () => {
    await page.locator("#paraOptsBtn").click();
    await expect(page.locator("#paragraphPropertiesPanel")).toBeVisible();
    const value = await page.locator("#indentLeft").inputValue();
    await page.keyboard.press("Escape");
    await clickIntoFirstPage(page);
    return value;
  };

  // A round trip rather than an absolute value: the field renders no indent as an
  // empty string, so "is it 0" cannot tell a cleared indent from a blank field.
  // What the two chords have to guarantee is that one moves the indent and the
  // other puts it back.
  const start = await indentAfterOpening();
  await page.keyboard.press(`${MOD}+m`);
  const indented = await indentAfterOpening();
  expect(indented, "⌘M did not indent the paragraph").not.toBe(start);
  await page.keyboard.press(`${MOD}+Shift+m`);
  expect(await indentAfterOpening(), "⌘⇧M did not undo the indent").toBe(start);
  expect(consoleErrors).toEqual([]);
});

test("the font-size chords change the size the toolbar reports", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  // A CARET, not select-all: over a whole rich document the size is mixed and the
  // field goes blank, which would make this test read 0 and prove nothing. That
  // version was written first and failed for exactly that reason.
  const size = () => page.locator("#fontSize").inputValue();
  const before = Number(await size());
  expect(before, "no font size reported, so this test cannot see a change").toBeGreaterThan(0);
  await page.keyboard.press(`${MOD}+]`);
  await expect.poll(async () => Number(await size())).toBeGreaterThan(before);
  const grown = Number(await size());
  await page.keyboard.press(`${MOD}+[`);
  await expect.poll(async () => Number(await size())).toBeLessThan(grown);
});

test("an app chord works from the editor and an editor chord does not work from chrome", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // App scope: Find opens with the caret in the document.
  await page.keyboard.press(`${MOD}+f`);
  await expect(page.locator("#findPanel")).toBeVisible();
  await page.keyboard.press("Escape");

  // The palette's own chord closes it as well as opening it, or the chord would
  // strand the surface it opened.
  await page.keyboard.press(`${MOD}+Shift+p`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.keyboard.press(`${MOD}+Shift+p`);
  await expect(page.locator("#cmdPalette")).toBeHidden();

  // Editor scope: ⌘B must not reach the document while a chrome search box has
  // the keyboard. This is what `scope` exists for.
  await page.locator("#fontFamily").click();
  await expect(page.locator("#fontMenuInput")).toBeVisible();
  await page.locator("#fontMenuInput").focus();
  const boldBefore = await page.locator("#bold").getAttribute("aria-pressed");
  await page.keyboard.press(`${MOD}+b`);
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", boldBefore ?? "false");
  expect(consoleErrors).toEqual([]);
});

test("the review-mode chord cycles all three modes", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const mode = () =>
    page.evaluate(
      () => document.querySelector('.review-mode-seg[aria-pressed="true"]')?.dataset.reviewMode,
    );
  // UX-007's example: this chord worked and was advertised nowhere. It is a named
  // command now, so it is both discoverable and still has to actually cycle.
  expect(await mode()).toBe("editing");
  for (const expected of ["suggesting", "viewing", "editing"]) {
    await page.keyboard.press(`${MOD}+Shift+e`);
    await expect.poll(mode).toBe(expected);
  }
  expect(consoleErrors).toEqual([]);
});

test("a chord whose command cannot run says why instead of doing nothing", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  // ⌘K needs a text range; with a collapsed caret the command is disabled. A
  // chord that refused in silence would be indistinguishable from a chord that
  // was never bound — which is the defect this row is about.
  await page.keyboard.press(`${MOD}+k`);
  await expect(page.locator("#status")).toContainText("Select text to add a link");
  await expect(page.locator("#linkDialog")).toBeHidden();
});

test("every chord the palette advertises is one the editor can run", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Shift+p`);
  await expect(page.locator("#cmdPalette")).toBeVisible();

  // The surfaces and the dispatcher read ONE table, so what the palette prints
  // must be exactly what the dispatcher would match. Asked of the running app
  // rather than of the module, so a build that shipped a stale copy of either
  // side fails here.
  const mismatches = await page.evaluate(async () => {
    const { KEYMAP, chordCommand } = await import("/src/keymap.mjs");
    const { keyboardPlatform, parseShortcut } = await import("/src/keyboard.mjs");
    const platform = keyboardPlatform(navigator);
    const advertised = [...document.querySelectorAll("#cmdList [data-command-shortcut]")].map(
      (el) => el.dataset.commandShortcut,
    );
    const out = [];
    for (const label of new Set(advertised)) {
      const row = KEYMAP.find((r) => r.chord === label && (!r.platform || r.platform === platform));
      if (!row) {
        out.push(`${label} is printed by the palette but is in no keymap row`);
        continue;
      }
      const wanted = parseShortcut(row.chord);
      if (!wanted.mod && !wanted.control && !wanted.alt) continue;
      const apple = platform === "apple";
      const event = {
        key: wanted.key,
        metaKey: apple ? wanted.mod : false,
        ctrlKey: apple ? wanted.control : wanted.mod || wanted.control,
        altKey: wanted.alt,
        shiftKey: wanted.shift,
      };
      if (chordCommand(event, platform, { inEditor: true }) !== row.command) {
        out.push(`${label} is printed for ${row.command} but does not dispatch to it`);
      }
    }
    return { mismatches: out, count: advertised.length };
  });
  expect(mismatches.count, "the palette advertises no chords at all").toBeGreaterThan(25);
  expect(mismatches.mismatches).toEqual([]);
});

// ---- History through the chord ----------------------------------------------
//
// Undo and redo had no direct chord guard. When the keymap rewrite moved ⌘Z off
// an inline `keydown` branch onto the dispatcher, the only things that noticed a
// broken undo were `line-numbers` and `shape-editing` — two specs about
// something else entirely, which caught it by measuring ink in a margin and a
// hex fill. That is an accident, not coverage: the diagnosis had to start from
// "the numbers are still painted" and work backwards to the keyboard.
//
// So these two assert the GUARANTEE — the document is back in the state it was
// in — rather than the mechanism, and they assert it for the two families the
// history stack has to serve: an edit that changes TEXT, and an edit that
// changes a paragraph's FORMATTING without changing a character.

test("the history chords put the text back, and forward again", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // The a11y mirror is the document as the engine has it, one string per block,
  // so "the document returned to its prior state" is a comparison of the whole
  // reading order rather than of one element's text. `#undoBtn` is deliberately
  // NOT the subject here — that button is the other surface, and this test is
  // about the keyboard.
  const before = await mirrorBlocks(page);
  const marker = "CHORDUNDOMARKER";
  await page.keyboard.type(marker);
  await expect
    .poll(async () => (await mirrorBlocks(page)).join("\n").includes(marker), {
      message: "the typing never reached the document, so undo cannot be measured",
    })
    .toBe(true);

  await page.keyboard.press(`${MOD}+z`);
  await expect
    .poll(async () => (await mirrorBlocks(page)).join("\n"), {
      message: "the undo chord did not put the document back",
    })
    .toBe(before.join("\n"));

  // Redo, both spellings Word binds. ⌘⇧Z first; then undo again so ⌘Y has
  // something to put forward, because a chord that fires on an empty redo stack
  // proves nothing.
  await page.keyboard.press(`${MOD}+Shift+z`);
  await expect
    .poll(async () => (await mirrorBlocks(page)).join("\n").includes(marker), {
      message: "the redo chord did not put the typing back",
    })
    .toBe(true);

  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(async () => (await mirrorBlocks(page)).join("\n")).toBe(before.join("\n"));
  await page.keyboard.press(`${MOD}+y`);
  await expect
    .poll(async () => (await mirrorBlocks(page)).join("\n").includes(marker), {
      message: "⌘Y is bound to redo as well and must reach the same command",
    })
    .toBe(true);

  // Leave the document as it was found.
  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(async () => (await mirrorBlocks(page)).join("\n")).toBe(before.join("\n"));
  expect(consoleErrors).toEqual([]);
});

test("the undo chord puts back a formatting change too", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Not a text edit: alignment leaves every character in place, and it is the
  // family `line-numbers` and `shape-editing` were incidentally covering — a
  // property change whose undo has to travel the same chord to the same command.
  const before = await reflected(page);
  const wasCentered = before.center === "true";
  const chord = wasCentered ? `${MOD}+l` : `${MOD}+e`;
  const changed = wasCentered ? "start" : "center";

  await page.keyboard.press(chord);
  await expect
    .poll(async () => (await reflected(page))[changed], {
      message: `${chord} did not change the alignment, so undo cannot be measured`,
    })
    .toBe("true");

  await page.keyboard.press(`${MOD}+z`);
  await expect
    .poll(async () => (await reflected(page))[changed], {
      message: "the undo chord left the alignment applied",
    })
    .not.toBe("true");
  expect(await reflected(page), "undo restored something other than the prior state").toEqual(
    before,
  );
  expect(consoleErrors).toEqual([]);
});
