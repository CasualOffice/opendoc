// EVERY RIBBON CONTROL NAMES A COMMAND, AND REACHES IT (`109` UX-005, `105` CQ-004).
//
// Re-measured in this browser before anything was changed: 52 of 120 visible
// ribbon controls carried a command id. Insert, Layout, References and Review are
// declarative and stamp one; Home (0 of 41), View (0 of 8) and Table (0 of 19)
// were bound by hand. The row's own "~23 of ~90" and its 2026-09-20 recount of
// "39 of 109" were both stale — which is why this file MEASURES rather than
// pinning a number: it asserts that nothing is unclassified, so it stays true as
// the ribbon grows and cannot be satisfied by editing a count.
//
// Three things are asserted, in increasing strength:
//
//   1. Nothing is unclassified. A visible ribbon control carries `data-command`
//      (it runs that command) or `data-command-family` (it opens a chooser over
//      that family). A control with neither is a ribbon-only capability, which is
//      this repo's recurring defect (SKILL.md §10, every capability from ≥2
//      surfaces).
//
//   2. The name is LIVE. A stamped id must be one `editorCommands()` actually
//      returns, and a family must have at least one live member. #623 is why:
//      `⌘⌥M` was bound to `comment.add`, an id the registry does not return, and
//      a check that only read the table passed it. The registry is read from the
//      command palette's own rows, which carry `data-command-id`.
//
//   3. The control REACHES it. For every `data-command` face, the control is
//      driven with a pointer and the command is then run by id from the palette on
//      a freshly loaded document, and the two are required to have the same
//      observable effect. That is the guarantee — "these are two faces of one
//      command" — asserted without naming a mechanism, which matters because
//      several registry commands are implemented BY these controls
//      (`format.clear` runs `clearFormattingBtn.click()`) and others share a
//      helper. A test that compared function identity would pass for the first
//      kind and fail for the second while both are correct.
import {
  MOD,
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
} from "./fixtures.mjs";

const TABS = ["home", "insert", "layout", "references", "view", "review", "table"];

const panelOf = (tab) => `#panel${tab[0].toUpperCase()}${tab.slice(1)}`;

/** Hands control to the operating system and never gives it back to the test.
 *  Each is covered by its own spec; the ids are asserted to still be offered, so
 *  a skip cannot outlive its command. */
const POINTER_UNSAFE = new Map([
  ["file.print", "window.print() blocks the renderer until the OS dialog closes"],
  ["file.open", "opens a native file chooser"],
  ["insert.image", "opens a native file chooser"],
]);

/** A document with a caret in a table and a range inside that cell — the state
 *  that makes the most of the ribbon reachable at once: the contextual Table tab
 *  is present, and the commands that need a range are enabled. */
async function prepare(page, { table = true, range = true } = {}) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  if (table) {
    await page.locator('.ribbon-tab[data-tab="insert"]').click();
    await page.locator("#insertTableBtn").click();
    await page.locator('.gc[data-r="2"][data-c="3"]').click();
    await expect(page.locator("#tabTable")).toBeEnabled();
    // Type into the cell so there is something to select, and something to undo.
    await page.keyboard.type("parity");
  } else {
    await page.keyboard.type("parity");
  }
  if (range) for (let i = 0; i < 6; i += 1) await page.keyboard.press("Shift+ArrowLeft");
  // Let the preparation's own announcements finish. The toast is transient, so a
  // baseline taken while one is still up would attribute its disappearance to the
  // activation — and the two halves of a comparison are timed slightly
  // differently, which is exactly how a guard becomes flaky.
  await expect(page.locator("#statusToast")).toBeHidden();
}

/** Every command id the editor is offering right now, read from the palette's own
 *  rows rather than from any table — the registry as the user meets it. */
async function liveRegistry(page) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  const ids = await page.$$eval("#cmdList .cmd-item", (items) =>
    items.map((item) => ({ id: item.dataset.commandId, disabled: item.disabled })),
  );
  await page.keyboard.press("Escape");
  await expect(page.locator("#cmdPalette")).toBeHidden();
  return ids;
}

/** Every visible control in a tab's panel, with what it claims to be. */
async function controlsOf(page, tab) {
  await page.locator(`.ribbon-tab[data-tab="${tab}"]`).click();
  return page.$$eval(panelOf(tab), ([panel]) =>
    [...panel.querySelectorAll("button, input, select")]
      .filter((el) => el.offsetParent !== null)
      .map((el, index) => ({
        index,
        name: el.id || el.className,
        command: el.dataset.command ?? "",
        family: el.dataset.commandFamily ?? "",
        disabled: !!el.disabled,
      })),
  );
}

test.describe("every ribbon control names a live command", () => {
  test("nothing on any tab is unclassified, and every name is one the registry returns", async ({
    page,
    consoleErrors,
  }) => {
    test.setTimeout(240_000);
    await prepare(page);
    const registry = await liveRegistry(page);
    const ids = new Set(registry.map((row) => row.id));
    expect(ids.size, "the palette must offer commands, or this test checks nothing")
      .toBeGreaterThan(100);

    const unclassified = [];
    const dead = [];
    const emptyFamilies = [];
    let classified = 0;

    for (const tab of TABS) {
      for (const control of await controlsOf(page, tab)) {
        if (!control.command && !control.family) {
          unclassified.push(`${tab} ▸ ${control.name}`);
          continue;
        }
        classified += 1;
        if (control.command && !ids.has(control.command)) {
          dead.push(`${tab} ▸ ${control.name} names ${control.command}`);
        }
        if (control.family && ![...ids].some((id) => id.startsWith(control.family))) {
          emptyFamilies.push(`${tab} ▸ ${control.name} discloses ${control.family}`);
        }
      }
    }

    expect(
      unclassified,
      "these ribbon controls name no command: they are reachable from the ribbon and " +
        "nowhere else. Declare each one in ribbon_faces.mjs (or in the tab's own " +
        "surface table) as the command it runs or the family it opens",
    ).toEqual([]);
    expect(
      dead,
      "these controls name a command id editorCommands() does not return — the #623 " +
        "defect: a name that looks wired and is not",
    ).toEqual([]);
    expect(
      emptyFamilies,
      "these controls open a chooser over a command family with no live members, so " +
        "the capability they disclose is not reachable from anywhere else",
    ).toEqual([]);
    // A lower bound, not a pinned count: adding a control must not turn this red.
    expect(classified, "the whole ribbon should be classified").toBeGreaterThanOrEqual(115);
    expect(consoleErrors).toEqual([]);
  });

  test("the boot-time stamping pass resolved every selector it declares", async ({ page }) => {
    // `stampRibbonFaces` warns about a face whose control it could not find,
    // because a renamed control silently losing its command id is exactly the
    // state UX-005 describes. The warning is the app's own report; this asserts it
    // is silent.
    const warnings = [];
    page.on("console", (message) => {
      if (message.type() === "warning") warnings.push(message.text());
    });
    await gotoEditor(page);
    expect(warnings.filter((text) => text.includes("ribbon faces with no control"))).toEqual([]);
  });

  test("every skipped command is still offered, so a skip cannot outlive its command", async ({
    page,
  }) => {
    await prepare(page);
    const ids = new Set((await liveRegistry(page)).map((row) => row.id));
    expect([...POINTER_UNSAFE.keys()].filter((id) => !ids.has(id))).toEqual([]);
  });
});

// ---- Reach: the control and the id do the same thing ------------------------

/** What a user could perceive, as one comparable value.
 *
 *  Deliberately not a list of things a particular command does: this has to be
 *  the same question for a toggle, an edit, a dialog and a zoom change. So it is
 *  "what became visible, what stopped being visible, and what the chrome now
 *  says". Mechanism-free — a command that changed its implementation but not its
 *  effect keeps this green, and one that quietly stopped doing half of what its
 *  ribbon face does cannot. */
const observe = () => {
  const visible = (el) => el.offsetParent !== null || el.getClientRects().length > 0;
  return {
    shown: [...document.querySelectorAll("[id]")]
      .filter(visible)
      // The toast is a transient announcement on its own timer, not a surface an
      // activation opened; its TEXT is compared below, through `status`.
      .filter((el) => el.id !== "statusToast")
      .map((el) => el.id)
      .sort(),
    pressed: [...document.querySelectorAll("[aria-pressed]")]
      .map((el) => `${el.id || el.dataset.command || el.className}=${el.getAttribute("aria-pressed")}`)
      .sort(),
    undo: document.getElementById("undoBtn")?.getAttribute("aria-label") ?? "",
    redo: document.getElementById("redoBtn")?.getAttribute("aria-label") ?? "",
    words: document.getElementById("statWords")?.textContent ?? "",
    pages: document.getElementById("statPages")?.textContent ?? "",
    zoom: document.getElementById("zoom")?.value ?? "",
    status: document.getElementById("status")?.textContent ?? "",
  };
};

/** The difference one activation made. Sets, not sequences, so the order the DOM
 *  happened to mutate in is not part of the claim. */
function difference(before, after) {
  const appeared = after.shown.filter((id) => !before.shown.includes(id));
  const vanished = before.shown.filter((id) => !after.shown.includes(id));
  const pressed = after.pressed.filter((entry) => !before.pressed.includes(entry));
  const changed = {};
  for (const key of ["undo", "redo", "words", "pages", "zoom", "status"]) {
    if (before[key] !== after[key]) changed[key] = after[key];
  }
  return { appeared, vanished, pressed, changed };
}

async function activationEffect(page, activate) {
  const before = await page.evaluate(observe);
  await activate();
  // Give the effect a frame to land, then read. A command that does nothing
  // perceivable is the activation contract's business, not this test's.
  await page.waitForTimeout(400);
  const after = await page.evaluate(observe);
  return difference(before, after);
}

/** Runs `id` from the command palette — the second surface, with no knowledge of
 *  which control is supposed to match it. */
async function runFromPaletteById(page, id) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator(`#cmdList .cmd-item[data-command-id="${id}"]`).first().click();
  await expect(page.locator("#cmdPalette")).toBeHidden();
}

for (const tab of ["home", "view", "table"]) {
  test(`every ${tab} control reaches the command it names`, async ({ page }) => {
    // Two document loads per control: one to drive the ribbon, one to run the id.
    // A shared document would let the control before this one decide the answer.
    test.setTimeout(900_000);

    await prepare(page);
    const faces = (await controlsOf(page, tab)).filter(
      (control) => control.command && !control.disabled && !POINTER_UNSAFE.has(control.command),
    );
    const registry = await liveRegistry(page);
    const runnable = new Set(registry.filter((row) => !row.disabled).map((row) => row.id));
    const pairs = faces.filter((face) => runnable.has(face.command));

    // A sweep that covers nothing passes every assertion below.
    expect(pairs.length, `${tab} must offer comparable faces`).toBeGreaterThan(
      { home: 20, view: 5, table: 10 }[tab],
    );

    const divergent = [];
    for (const face of pairs) {
      await prepare(page);
      await page.locator(`.ribbon-tab[data-tab="${tab}"]`).click();
      const fromRibbon = await activationEffect(page, () =>
        page.locator(`${panelOf(tab)} [data-command="${face.command}"]`).first().click(),
      );

      await prepare(page);
      // The same panel is showing in both halves, so the baseline is the same and
      // "the Table tab appeared" is not counted as an effect of one and not the
      // other.
      await page.locator(`.ribbon-tab[data-tab="${tab}"]`).click();
      const fromPalette = await activationEffect(page, () =>
        runFromPaletteById(page, face.command),
      );

      if (JSON.stringify(fromRibbon) !== JSON.stringify(fromPalette)) {
        divergent.push({
          control: face.name,
          command: face.command,
          ribbon: fromRibbon,
          palette: fromPalette,
        });
      }
    }

    expect(
      divergent.map((row) => `${row.control} vs ${row.command}: ` + JSON.stringify(row)),
      `a ${tab} control and the command it names must do the same thing; ` +
        "either the control is stamped with the wrong id, or the two surfaces have " +
        "drifted into two implementations of one command",
    ).toEqual([]);
  });
}
