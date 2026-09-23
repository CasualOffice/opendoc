// One navigation axis per chrome, and nothing lost on the way there.
//
// `109` UX-014: the editor showed an application menu bar (File / Edit / View /
// …) and a ribbon tab strip (Home / Insert / Layout / …) AT THE SAME TIME, so
// finding a command meant first guessing which of two systems owned it. The
// owner's words: "two menues option … lets adopt [ONLYOFFICE's] file, home, … —
// much cleaner and friendly … and less cognitive burden".
//
// The restructure (docs/122) gives each chrome exactly one axis: the ribbon
// chrome's is the tab strip with File as its first tab, and the compact chrome's
// is the menu bar with File as a dropdown. Three menus were dissolved into those
// — Tools, Help, and the second copy of File — and the whole risk of a
// restructure like this is a command quietly losing its home.
//
// So the load-bearing test here is the LAST one: every command the registry
// offers must be reachable from the palette AND from at least one DURABLE
// surface (a ribbon control, the File surface, a menu row, the compact toolbar,
// the context menu, or a keyboard chord). `109` UX-004 is open precisely because
// the two tests named for command-surface parity assert frozen `toContain`
// lists and cannot detect an omission. This one derives both sides from the
// running application, so it can.
import { test, expect, gotoEditor, clickIntoFirstPage, openAppMenu, MOD } from "./fixtures.mjs";

/** The tab strip, in rendered order, with each tab's enabled state. */
async function tabStrip(page) {
  return page.locator(".ribbon-tabs .ribbon-tab").evaluateAll((tabs) =>
    tabs.map((t) => ({ tab: t.dataset.tab, label: t.textContent.trim(), disabled: t.disabled })),
  );
}

test("the ribbon chrome shows ONE axis: the tab strip, File first, no menu bar", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);

  // Ribbon is the default chrome.
  await expect(page.locator("body")).toHaveClass(/ribbon-mode/);
  await expect(page.locator(".ribbon-tabs")).toBeVisible();

  // The second navigation system must not be on screen. This is the whole
  // defect: two bars of names, one document.
  await expect(page.locator("#appMenuBar")).toBeHidden();

  expect((await tabStrip(page)).map((t) => t.label)).toEqual([
    "File",
    "Home",
    "Insert",
    "Layout",
    "References",
    "Review",
    "View",
    "Table",
  ]);
  expect(consoleErrors).toEqual([]);
});

test("the compact chrome shows ONE axis: the menu bar, and no ribbon", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await page.locator("#modeCompact").click();

  await expect(page.locator("#appMenuBar")).toBeVisible();
  // `.ribbon` is `display: none` in compact mode, which takes the tab strip with
  // it — so the bar is the only axis, exactly as the ribbon chrome's strip is
  // the only axis there.
  await expect(page.locator(".ribbon")).toBeHidden();
  await expect(page.locator(".ribbon-tabs")).toBeHidden();

  // Seven names, and neither Tools nor Help among them: their rows moved to the
  // File surface and the Review band. Two names fewer is two names that cannot
  // scroll off the end of the bar behind a hidden scrollbar (`109` HF-097).
  const menus = await page
    .locator("#appMenuBar .app-menu-button")
    .evaluateAll((b) => b.map((x) => x.dataset.menu));
  expect(menus).toEqual(["file", "edit", "view", "insert", "format", "table", "review"]);
  expect(consoleErrors).toEqual([]);
});

// The regression this restructure nearly shipped. With no document open the
// empty-state CSS hides the whole `.ribbon` — bands of disabled controls around
// a drop card read as a broken shell — so hiding the menu bar in ribbon mode as
// well left a freshly loaded editor with NO route to Open: the drop card and
// nothing else. Caught by running the suite, not by reading the rule.
test("with no document open there is still exactly one axis, and it offers Open", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/editor.html?blank=1");
  await expect(page.locator("body")).not.toHaveClass(/doc-loaded/);

  // The band is not on screen, so the bar is the axis here — still one, not two.
  await expect(page.locator(".ribbon-tabs")).toBeHidden();
  await expect(page.locator("#appMenuBar")).toBeVisible();

  await page.locator('.app-menu-button[data-menu="file"]').click();
  const open = page.locator('#appMenuPopover .app-menu-item[data-command="file.open"]');
  await expect(open, "with no document, Open must be reachable from somewhere").toBeEnabled();
  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

test("File is a PAGE in the ribbon chrome and a DROPDOWN in the compact chrome", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);

  // Ribbon: ONLYOFFICE's arrangement — the File tab is declared
  // `haspanel: false` there and opens a full-window route, not a band.
  await page.locator("#tabFile").click();
  const filePage = page.locator("#panelFile");
  await expect(filePage).toBeVisible();
  await expect(page.locator("body")).toHaveClass(/file-page-open/);
  // It covers the work area rather than sitting in the band's 70-odd pixels.
  const box = await filePage.boundingBox();
  const viewport = page.viewportSize();
  expect(box.height, "a File PAGE fills the work area").toBeGreaterThan(viewport.height / 2);
  expect(box.width).toBeGreaterThan(viewport.width - 4);

  // A full-window route needs a way out that is not "pick another tab".
  await page.keyboard.press("Escape");
  await expect(filePage).toBeHidden();
  await page.locator("#tabFile").click();
  await page.locator("#filePageBack").click();
  await expect(filePage).toBeHidden();
  await expect(page.locator("#panelHome")).toBeVisible();

  // Compact: Google Docs' and Drive's arrangement — an anchored dropdown. The
  // researched distinction that settles this: in Docs the arrow keys traverse
  // from File to its sibling menus, which is structurally impossible for a
  // view-replacing page.
  await openAppMenu(page, "file");
  const popover = page.locator("#appMenuPopover");
  await expect(popover).toBeVisible();
  const menuBox = await popover.boundingBox();
  expect(menuBox.width, "a dropdown is anchored, not full-width").toBeLessThan(
    viewport.width / 2,
  );
  await expect(page.locator("#panelFile")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("both File surfaces offer the same rows — one roster, two renderings", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.locator("#tabFile").click();
  // The page renders some of the roster as CATEGORY rows that open a pane
  // instead of running — one `Export` for the six formats, a pane for each row
  // that used to open a dialog over the page. A category row records the ids it
  // stands in for, so this still compares rosters rather than renderings: same
  // commands, same order, two chromes.
  const onPage = await page
    .locator("#filePageBody .file-page-item")
    .evaluateAll((rows) =>
      rows.flatMap((r) => (r.dataset.command ? [r.dataset.command] : r.dataset.covers.split(" "))),
    );
  await page.keyboard.press("Escape");

  await openAppMenu(page, "file");
  const inMenu = await page
    .locator("#appMenuPopover .app-menu-item")
    .evaluateAll((rows) => rows.map((r) => r.dataset.command));
  await page.keyboard.press("Escape");

  expect(onPage.length).toBeGreaterThan(10);
  expect(
    onPage,
    "the page and the dropdown render the SAME declaration; a difference here " +
      "means one chrome is offering something the other is not",
  ).toEqual(inMenu);

  // And the rows that arrived from the dissolved menus are really there.
  for (const id of ["view.settings", "help.about", "help.shortcuts", "file.save", "file.print"]) {
    expect(onPage, `${id} must be on the File surface`).toContain(id);
  }
  expect(consoleErrors).toEqual([]);
});

test("the proofing switches have a Review band face that reflects their state", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await page.locator("#tabReview").click();

  // Both were reachable from a `Tools` menu and the palette only. A chrome with
  // no menu bar would have left them palette-only, which is the single-surface
  // defect `109` UX-015 records.
  const smartQuotes = page.locator("#reviewSmartQuotesBtn");
  await expect(smartQuotes).toHaveAttribute("data-command", "tools.smartQuotes");
  const before = await smartQuotes.getAttribute("aria-pressed");
  await smartQuotes.click();
  await expect(smartQuotes).not.toHaveAttribute("aria-pressed", before);

  for (const [selector, command] of [
    ["#reviewSpellCheckBtn", "tools.spellCheck"],
    ["#reviewGrammarCheckBtn", "tools.grammarCheck"],
  ]) {
    const button = page.locator(selector);
    await expect(button).toHaveAttribute("data-command", command);
    const was = await button.getAttribute("aria-pressed");
    await button.click();
    await expect(button).not.toHaveAttribute("aria-pressed", was);
  }
  expect(consoleErrors).toEqual([]);
});

test("the running-content variants have an Insert band face that reflects their state", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator("#tabInsert").click();

  const first = page.locator("#insertFirstPageVariantBtn");
  await expect(first).toHaveAttribute("data-command", "layout.firstPageVariant");
  await expect(first).toBeEnabled();
  await expect(first).toHaveAttribute("aria-pressed", "false");
  await first.click();
  await page.locator("#tabInsert").click();
  await expect(first).toHaveAttribute("aria-pressed", "true");
  expect(consoleErrors).toEqual([]);
});

// ---------------------------------------------------------------------------
// The load-bearing guard.
//
// Two navigation systems became one. The claim that nothing was lost is only
// worth something if a test can fail when something IS lost — which is exactly
// what `109` UX-004 says the existing parity tests cannot do, because they
// assert frozen `toContain` lists.
//
// This derives BOTH sides from the running application:
//
//   the registry   the palette lists every command with an empty query, and each
//                  row carries `data-command-id`, so this is the complete set
//                  rather than a sample.
//   the surfaces   every `[data-command]` control in the ribbon (all panels, on
//                  every tab, hidden or not), every File-page row, every menu
//                  row across all seven menus, every compact-toolbar button, and
//                  every context-menu row. Plus a keyboard chord, read off the
//                  palette row's own shortcut hint.
//
// A command reachable ONLY from the palette fails. That is the rule SKILL.md §10
// states ("every capability must be reachable from ≥2 surfaces") made checkable
// over the whole registry instead of family by family.
//
// It carries an explicit exemption list, which is the honest part: three
// families genuinely have no durable surface yet, each for a stated reason, and
// hiding that behind a looser assertion would make the guard worthless. Adding
// to that list is a deliberate act with a reason attached — which is the
// difference between a recorded gap and a silent narrowing.
// VALUE rows: a command id that names one VALUE of a control which is itself on
// a durable surface. `format.family.Georgia` exists so a keyboard user can ask
// for Georgia by name (`104` HF-147/149/150) — the durable affordance is the
// Font control on the Home band, and putting twenty typefaces on a band is not
// an improvement on that.
//
// The exemption is VERIFIED, not asserted: each family names the control that
// owns its list, and the guard fails if that control is not in the chrome. Delete
// `#fontFamily` and every `format.family.*` row becomes a real orphan, which is
// the behaviour an exemption list has to have to be worth keeping.
const VALUE_FAMILIES = [
  ["format.family.", "#fontFamily", "the Font control on the Home band opens this list"],
  ["format.size.", "#fontSize", "the Font size control on the Home band"],
  ["format.underline.", "#underlineMenuBtn", "the underline-style menu on the Home band"],
  ["paragraph.listFormat.", "#bulletListMenuBtn", "the bullet and numbering galleries on Home"],
  ["paragraph.spacing.", "#spacingBtn", "the line-and-paragraph-spacing menu on Home"],
  ["style.", "#stylesTrigger", "the Styles gallery on the Home band"],
  ["view.zoom.", "#zoom", "the zoom control, on the View band and in the footer"],
  ["insert.field.", "#insertFieldBtn", "the field dialog, on both Insert and References"],
];

// Commands with no durable BUTTON, of two kinds, kept apart on purpose.
//
// The first kind is by design: a capability whose affordance is the key itself.
// The second kind is a recorded GAP — the fix is real design work rather than a
// placement decision, so it stays visible to whoever picks the row up instead of
// being dissolved into a looser assertion.
const PALETTE_ONLY = new Map([
  // By design. Object traversal is the Tab key walking the anchored objects of
  // the page. No button can point at it: there is nothing to act on until an
  // object is selected, and once one is, the object bar and the context menu
  // take over. It is deliberately NOT given a `shortcut` in the registry either,
  // because Tab means "insert a tab" in a paragraph and "next cell" in a table —
  // declaring it as this command's global chord would be a false claim on a
  // user-facing surface.
  ["object.selectNext", "by design: the Tab key IS the affordance"],
  ["object.selectPrevious", "by design: Shift+Tab IS the affordance"],
  // Word's Review band puts these on a SPLIT button: "Accept and Move to Next"
  // is the face, "Accept This Change" is the first dropdown row. We have the
  // face (`#reviewAcceptBtn` = review.acceptNext) and no dropdown, so the
  // at-caret pair is palette-only. Pre-existing, found by this guard rather
  // than caused by the restructure; queued in `109` as HF-179.
  ["review.acceptAtCaret", "needs the Review band's Accept split button (109 HF-179)"],
  ["review.rejectAtCaret", "needs the Review band's Reject split button (109 HF-179)"],
]);

test("no command is reachable from the command palette alone", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // --- the registry, complete ---
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  const registry = await page
    .locator("#cmdList .cmd-item[data-command-id]")
    .evaluateAll((rows) =>
      // `data-command-shortcut`, NOT the visible hint: the hint box shows a
      // chord, or the command's group, or its refusal reason, depending on
      // state — so reading it would count "Format" as a keyboard surface and
      // this guard would pass for every command in the registry.
      rows.map((r) => ({ id: r.dataset.commandId, shortcut: r.dataset.commandShortcut ?? "" })),
    );
  await page.keyboard.press("Escape");
  expect(registry.length, "the palette must list the whole registry").toBeGreaterThan(80);

  // --- the durable surfaces ---
  const durable = new Map();
  const claim = (ids, surface) => {
    for (const id of ids) if (id && !durable.has(id)) durable.set(id, surface);
  };

  // Every ribbon control on every tab. Hidden panels keep their buttons, so no
  // tab switching is needed — and reading them all at once is what makes this
  // ribbon-WIDE rather than per-panel, so a command may legitimately move tab
  // without this guard dictating the information architecture.
  claim(
    await page
      .locator(".ribbon-panel [data-command]")
      .evaluateAll((els) => els.map((e) => e.dataset.command)),
    "ribbon",
  );

  // The File page.
  await page.locator("#tabFile").click();
  claim(
    await page
      .locator("#filePageBody .file-page-item")
      .evaluateAll((rows) => rows.map((r) => r.dataset.command)),
    "file page",
  );
  await page.keyboard.press("Escape");

  // Every menu of the compact chrome, and its toolbar.
  const MENUS = ["file", "edit", "view", "insert", "format", "table", "review"];
  for (const menu of MENUS) {
    await openAppMenu(page, menu);
    claim(
      await page
        .locator("#appMenuPopover .app-menu-item")
        .evaluateAll((rows) => rows.map((r) => r.dataset.command)),
      `${menu} menu`,
    );
    await page.keyboard.press("Escape");
  }
  claim(
    await page
      .locator("#compactToolbar [data-command-id]")
      .evaluateAll((els) => els.map((e) => e.dataset.commandId)),
    "compact toolbar",
  );
  await page.locator("#modeRibbon").click();

  // The context menu, with a range so the formatting rows are live.
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+a`);
  await page.locator("#pages").click({ button: "right" });
  claim(
    await page
      .locator(".editor-context-menu .menu-item")
      .evaluateAll((rows) => rows.map((r) => r.dataset.commandId)),
    "context menu",
  );
  await page.keyboard.press("Escape");

  // A chord is a surface: it is how most people actually run Undo.
  claim(
    registry.filter((r) => r.shortcut).map((r) => r.id),
    "keyboard",
  );

  // Every value family's owning control must really be in the chrome, or the
  // exemption it grants is a fiction.
  for (const [prefix, selector, why] of VALUE_FAMILIES) {
    await expect(
      page.locator(selector),
      `${prefix}* is exempt because ${why} — and ${selector} is not there`,
    ).toHaveCount(1);
  }
  // A family covers its own PARENT id too: `format.family` is "open the Font
  // control's list", which is the control the family names — the same durable
  // surface, reached by the same click.
  const isValueRow = (id) =>
    VALUE_FAMILIES.some(([prefix]) => id.startsWith(prefix) || id === prefix.slice(0, -1));

  const orphans = registry
    .map((r) => r.id)
    .filter((id) => !durable.has(id) && !PALETTE_ONLY.has(id) && !isValueRow(id));
  expect(
    orphans,
    "these commands are reachable from the command palette and nowhere else. " +
      "Either give each one a durable surface — a ribbon control, a File-page " +
      "row, a menu row, a context-menu row or a chord — or add it to " +
      "PALETTE_ONLY with the reason it has none. A restructure that quietly " +
      "strands a command is the thing this guard exists to catch.",
  ).toEqual([]);

  // The exemption list must not rot into cover for commands that since gained a
  // surface: an exemption nobody needs is an exemption nobody re-reads.
  const needless = [...PALETTE_ONLY.keys()].filter((id) => durable.has(id));
  expect(
    needless,
    "these ids are exempted from the reachability rule and no longer need to be",
  ).toEqual([]);

  expect(consoleErrors).toEqual([]);
});
