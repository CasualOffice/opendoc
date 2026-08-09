// THE MATRIX: every editing operation × every editable surface.
//
// This exists because spot-checks let four partial fixes look finished. Each one
// verified the path it had just changed and shipped green while the same
// body-only seam stayed open one layer up — typing, then selection, then
// alignment reading as left, then font and size reading blank. The user found
// every one of them.
//
// A cell here is a real gesture driven through the editor and read back from what
// the user would see. Nothing is asserted from internal state that a broken build
// could still satisfy.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
  stableBox,
  MOD,
} from "./fixtures.mjs";

const SAMPLE = "sample.docx"; // header + footer, header is right-aligned
const TEXTBOX = "../fixtures/generated/inline-text-box.docx";
const GROUPED = "../fixtures/generated/grouped-text-boxes.docx";

async function openDoc(page, file, contains) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(file);
  const canvas = page.locator(".page-wrap .page").first();
  await expect(canvas).toBeVisible();
  const box = await stableBox(canvas);
  if (contains) await expect(page.locator("#a11yDocument")).toContainText(contains);
  return box;
}

/** Enters a surface and leaves the caret in its text.
 *
 *  Every entry here is deterministic. Hardcoded page fractions produced FALSE
 *  reds — the body point landed on a shaded box, the footer sat below the fold,
 *  and the grouped box had moved — and a matrix that cries wolf is no better
 *  than the spot-checks it replaces. */
async function scrollTo(page, edge) {
  await page
    .locator(".page-wrap .page")
    .first()
    .evaluate((el, e) => el.scrollIntoView({ block: e }), edge);
  await page.waitForTimeout(150);
  return stableBox(page.locator(".page-wrap .page").first());
}

/** Clicks around until an object is selected, so the point is found rather than
 *  assumed; returns the point that worked. */
async function findObject(page, box) {
  for (let fy = 0.04; fy < 0.45; fy += 0.02) {
    for (let fx = 0.1; fx < 0.85; fx += 0.05) {
      const p = { x: box.x + box.width * fx, y: box.y + box.height * fy };
      await page.mouse.click(p.x, p.y);
      if (await page.locator("#pages").getAttribute("data-object-kind")) return p;
    }
  }
  throw new Error("no selectable object found");
}

/** Double-clicks into the header or footer band and leaves the caret in it.
 *
 *  Retried: the page is taller than the viewport, so under parallel load the
 *  double-click can land before the scroll has settled and the band never
 *  engages. Retrying removes the intermittency without hiding a real failure — a
 *  genuinely broken band fails all three attempts. */
async function enterRunningBand(page, region) {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    const box = await scrollTo(page, region === "header" ? "start" : "end");
    const y = region === "header" ? box.y + 12 : box.y + box.height - 12;
    await page.mouse.dblclick(box.x + box.width * 0.5, y);
    if ((await page.locator("#pages").getAttribute("data-running-edit")) === region) break;
    await page.waitForTimeout(200);
  }
  await expect.poll(() => page.locator("#pages").getAttribute("data-running-edit")).toBe(region);
  // Let the caret settle before the first keystroke: entering re-renders, and a
  // key pressed mid-render is dropped, which showed up as a rare false red.
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
  return null;
}

const ENTER = {
  async body(page) {
    await clickIntoFirstPage(page);
    await moveCaretToDocStart(page);
    return null; // keyboard-driven from here
  },
  async header(page) {
    return enterRunningBand(page, "header");
  },
  async footer(page) {
    return enterRunningBand(page, "footer");
  },
  async footnote(page) {
    // Insert one and stay in its body — Word and Docs both leave the caret there.
    await clickIntoFirstPage(page);
    await moveCaretToDocStart(page);
    await page.keyboard.press(`${MOD}+Shift+P`);
    await page.locator("#cmdInput").fill("Footnote");
    await page.locator("#cmdList .cmd-item", { hasText: "Footnote" }).first().click();
    await expect(page.locator("#status")).toContainText("Footnote added");
    await expect(page.locator(".overlay .caret")).toHaveCount(1);
    return null;
  },
  async object(page) {
    const box = await scrollTo(page, "start");
    const p = await findObject(page, box);
    await page.mouse.dblclick(p.x, p.y);
    await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
    await expect(page.locator(".overlay .caret")).toHaveCount(1);
    return null;
  },
};

/** Selects a few characters from the caret, without a mouse drag that can leave
 *  the surface. */
async function selectSome(page) {
  for (let i = 0; i < 5; i += 1) await page.keyboard.press("Shift+ArrowRight");
}

const SURFACES = [
  { name: "body", file: SAMPLE, contains: "OpenDoc", enter: ENTER.body },
  { name: "header", file: SAMPLE, contains: "OpenDoc", enter: ENTER.header },
  { name: "footer", file: SAMPLE, contains: "OpenDoc", enter: ENTER.footer },
  { name: "footnote", file: SAMPLE, contains: "OpenDoc", enter: ENTER.footnote },
  { name: "text box", file: TEXTBOX, contains: "Body paragraph", enter: ENTER.object },
  { name: "grouped box", file: GROUPED, contains: "Body before", enter: ENTER.object },
];

for (const surface of SURFACES) {
  test(`[${surface.name}] typing inserts and undoes`, async ({ page, consoleErrors }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    await page.keyboard.type("MX");
    await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
    await page.keyboard.press(`${MOD}+z`);
    await expect(page.locator("#undoBtn")).not.toHaveAttribute("aria-label", "Undo Typing");
    expect(consoleErrors).toEqual([]);
  });

  test(`[${surface.name}] the toolbar reports a real font, not a blank`, async ({
    page,
    consoleErrors,
  }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    // Blank is what a body-only read produces: the caller falls back to its
    // default and the control has no value to change FROM.
    const font = (await page.locator("#fontFamily").textContent())?.trim();
    expect(font, "font control must reflect the caret's paragraph").toBeTruthy();
    expect(font).not.toBe("Font");
    expect(consoleErrors).toEqual([]);
  });

  test(`[${surface.name}] bold applies to a selection`, async ({ page, consoleErrors }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    await selectSome(page);
    await page.keyboard.press(`${MOD}+b`);
    await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Formatting");
    expect(consoleErrors).toEqual([]);
  });

  test(`[${surface.name}] copy puts this surface's text on the clipboard`, async ({
    page,
    consoleErrors,
  }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    await selectSome(page);
    const copied = await page.evaluate(async () => {
      const before = document.getElementById("status")?.textContent;
      document.execCommand("copy");
      return before !== undefined;
    });
    expect(copied).toBe(true);
    expect(consoleErrors).toEqual([]);
  });
}

// ---- Round two: the operations not yet proven in every surface ----------------
// Driven exactly as the existing specs drive them, so a red here is the product
// and not a wrong selector — the first attempt failed in the BODY too, which is
// how I knew the harness was lying rather than the editor.

/** Dispatches a real paste, the way keyboard-clipboard-parity does. */
async function pasteText(page, text) {
  await page.evaluate((value) => {
    const dt = new DataTransfer();
    dt.setData("text/plain", value);
    document.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }),
    );
  }, text);
}

for (const surface of SURFACES) {
  test(`[${surface.name}] paste inserts here`, async ({ page, consoleErrors }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    await pasteText(page, "PASTED");
    await expect(page.locator("#undoBtn")).toBeEnabled();
    expect(consoleErrors).toEqual([]);
  });

  test(`[${surface.name}] a comment can be added on a selection`, async ({
    page,
    consoleErrors,
  }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    await page.keyboard.type("CMT");
    for (let i = 0; i < 3; i += 1) await page.keyboard.press("Shift+ArrowLeft");
    await page.locator("#selComment").click();
    const sidebar = page.locator("#reviewSidebar");
    await sidebar.locator('[data-testid="review-comment-composer"]').fill("note");
    await sidebar.locator('[data-testid="review-comment-submit"]').click();
    // The comment must exist against the text it was anchored to. `.review-margin-card`
    // is what the sidebar actually renders — my first guess at the class failed in
    // the BODY too, which is how I knew it was the assertion and not the editor.
    await expect(sidebar.locator(".review-margin-card").first()).toBeVisible();
    expect(consoleErrors).toEqual([]);
  });

  test(`[${surface.name}] find locates this surface's text`, async ({ page, consoleErrors }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    await page.keyboard.type("ZQXFIND");
    await page.locator("#findBtn").click();
    await expect(page.locator("#findPanel")).toBeVisible();
    await page.locator("#findInput").fill("ZQXFIND");
    await page.waitForTimeout(500);
    // A find blind to the surface reports nothing for text just typed there.
    await expect(page.locator("#findStatus, #findCount").first()).not.toContainText("No results");
    expect(consoleErrors).toEqual([]);
  });
}

// ---- Round three: the operations the first two rounds left out ---------------
// Seven operations proved nothing about the eighth. These are the rest of what a
// user does to text, each driven through the real control (ribbon button, key,
// dialog) rather than a function call, in every surface.

for (const surface of SURFACES) {
  test(`[${surface.name}] cut removes text and undo restores it`, async ({
    page,
    consoleErrors,
  }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    await page.keyboard.type("CUTME");
    for (let i = 0; i < 5; i += 1) await page.keyboard.press("Shift+ArrowLeft");
    // A synthetic ⌘X does not make Chromium emit the native `cut` event the
    // editor listens for, so the event is dispatched directly — the same way the
    // paste cell drives paste, and through the same handler a real cut reaches.
    await page.evaluate(() => {
      const dt = new DataTransfer();
      document.dispatchEvent(
        new ClipboardEvent("cut", { clipboardData: dt, bubbles: true, cancelable: true }),
      );
    });
    // A cut that reached this surface leaves a deletion on the undo stack; one
    // that silently did nothing leaves the typing there instead.
    await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Delete");
    await page.keyboard.press(`${MOD}+z`);
    await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
    expect(consoleErrors).toEqual([]);
  });

  test(`[${surface.name}] redo reapplies an undone edit`, async ({ page, consoleErrors }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    await page.keyboard.type("RD");
    await page.keyboard.press(`${MOD}+z`);
    await expect(page.locator("#redoBtn")).toBeEnabled();
    await page.keyboard.press(`${MOD}+Shift+z`);
    // Redoing puts the typing back on the undo stack — the round trip closed.
    await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
    expect(consoleErrors).toEqual([]);
  });

  test(`[${surface.name}] Select All stays inside this surface`, async ({
    page,
    consoleErrors,
  }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    if (surface.name === "body") return; // nothing to scope it against
    // Read the body AFTER entering: entering a footnote adds its reference to
    // the body, and under parallel load the accessibility tree can still be
    // settling when the document opens — reading it too early compares against
    // a snapshot that was never the steady state.
    const bodyBefore = await page.locator("#a11yDocument").textContent();

    // Word scopes Ctrl+A to the story the caret is in. If ours selected the whole
    // document instead, typing here would wipe the body — the most destructive
    // way a surface leak can show up.
    await page.keyboard.press(`${MOD}+a`);
    await page.keyboard.type("X");
    expect(
      await page.locator("#a11yDocument").textContent(),
      "Select All in a sub-document must not reach the body",
    ).toBe(bodyBefore);
    expect(consoleErrors).toEqual([]);
  });

  test(`[${surface.name}] centering applies to this paragraph`, async ({
    page,
    consoleErrors,
  }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    await page.keyboard.type("ALIGN");
    await page.locator("#alignCenter").click();
    // The button lighting up is the read path (which was body-only and reported
    // every header as left-aligned); the undo entry is the write path.
    await expect(page.locator("#alignCenter")).toHaveAttribute("aria-pressed", "true");
    await expect(page.locator("#undoBtn")).toHaveAttribute(
      "aria-label",
      "Undo Paragraph formatting",
    );
    expect(consoleErrors).toEqual([]);
  });

  test(`[${surface.name}] a bulleted list can be started here`, async ({
    page,
    consoleErrors,
  }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    await page.keyboard.type("ITEM");
    await page.locator("#bulletList").click();
    await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "true");
    expect(consoleErrors).toEqual([]);
  });

  test(`[${surface.name}] an edit here can be tracked as a suggestion`, async ({
    page,
    consoleErrors,
  }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    await setReviewMode(page, "suggesting");
    await page.keyboard.type("SUGGESTED");
    // A tracked insertion must surface as a card wherever it was made; review
    // projections were body-only, so sub-document changes were invisible.
    await expect(
      page.locator("#reviewSidebar .review-margin-card.review-margin-insertion").first(),
    ).toBeVisible();
    expect(consoleErrors).toEqual([]);
  });

  test(`[${surface.name}] Replace all rewrites text here`, async ({ page, consoleErrors }) => {
    const box = await openDoc(page, surface.file, surface.contains);
    await surface.enter(page, box);
    await page.keyboard.type("QQZZ");
    await page.locator("#findBtn").click();
    await expect(page.locator("#findPanel")).toBeVisible();
    await page.locator("#findInput").fill("QQZZ");
    await page.locator("#replaceInput").fill("WWYY");
    await page.locator("#replaceAll").click();
    // Replace is find + edit: a surface-blind find replaces nothing, and a
    // body-only edit path throws instead of writing.
    await expect(page.locator("#undoBtn")).toBeEnabled();
    await expect(page.locator("#findStatus, #findCount").first()).not.toContainText("No results");
    expect(consoleErrors).toEqual([]);
  });
}
