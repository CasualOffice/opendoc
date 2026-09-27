// `docs/126` phase 1's exit gate: a host does it, in this repository.
//
// `embed.html` embeds the editor TWICE — once in an iframe the host wires
// itself, once through `<opendoc-editor>` — and this drives both. The gate is
// deliberately not "the button looks disabled": `docs/99` §9.4 records that
// "built" is not "reachable" and "modeled" is not "shipped", and a capability
// test that only checks a disabled attribute proves neither. So there are three
// separate claims here, each asserted as a guarantee rather than a mechanism:
//
//   1. Both embeds resolve their role BEFORE first paint, with a top-level
//      editor as the positive control — without it, a guard that finds
//      everything disabled everywhere passes for the wrong reason.
//   2. A withheld capability is DISABLED WITH A REASON, never hidden
//      (`SKILL.md` §10, "never a dead control"). A missing menu item teaches
//      nothing and reads as a broken product.
//   3. A reader cannot mutate the document THROUGH THE ENGINE even with the
//      chrome defeated from devtools. The attack is real: every `disabled` and
//      `aria-disabled` in the frame is stripped, every hidden control unhidden,
//      and then typing, deletion, paste and eleven formatting and structural
//      commands are fired at the document. The proof is the engine's own word
//      and character counts, read from `documentStats()`, not from the DOM the
//      command touched.
//
// What this canNOT yet prove, stated plainly rather than implied: the editor
// page does not YET map a role onto its review mode at startup, because that
// hook lives in `main.js` and a parallel lane owns that file. So claim 3 sets
// the mode the mapping DICTATES — read from `editingModeFor()`, not hardcoded —
// and then attacks. When the four-line startup hook lands, `setReviewMode` here
// becomes a no-op on an editor that is already in that mode and this spec keeps
// passing; the assertion to ADD at that point is marked below.
import { readFileSync } from "node:fs";
import {
  expect,
  test,
  mountEmbedPanel,
  openCommandPalette,
  waitForFramedEditor,
} from "./fixtures.mjs";

// Two WebAssembly editors boot in these tests, and the default 60 s is for one.
// This is a real cost, not a slow environment: a wall of UNIFORM ~45 s timeouts
// is the environment symptom, and this is a single spec that genuinely does
// twice the work.
test.describe.configure({ timeout: 240_000 });

const { editingModeFor, resolveCapabilities } = await import("../../src/capabilities.mjs");

/** The role under test, and the review mode the authority says it means. Read
 *  from the authority so this spec cannot disagree with the product about what
 *  `readonly` is — a spec holding its own copy of a table is the drift this
 *  phase exists to remove. */
const READER = "readonly";
const READER_MODE = editingModeFor(resolveCapabilities({ mode: READER, framed: true }));

// `waitForFramedEditor` and `mountEmbedPanel` were local to this file until
// `docs/126` phase 2's gate needed the same two, and two copies of "is this
// editor ready" is how one of them ends up waiting for less. They live in
// `fixtures.mjs` now, with every other shared driving helper.

/** The engine's own view of the document, which is the only thing that settles
 *  whether a mutation landed. `#statWords` / `#statChars` are painted from
 *  `documentStats()`, so they move if and only if the document moved — a DOM
 *  assertion on the page canvas could be satisfied by a repaint. */
async function engineStats(frame) {
  return frame.evaluate(() => ({
    words: document.getElementById("statWords")?.textContent ?? "",
    chars: document.getElementById("statChars")?.textContent ?? "",
  }));
}

test("both embeds resolve their role before first paint, and the chrome refuses with a reason", async ({
  page,
}) => {
  await page.goto("/embed.html");

  // The readout is derived from the capability authority, so the page cannot
  // display a table that is not the real one.
  for (const kind of ["iframe", "element"]) {
    const panel = page.locator(`[data-embed="${kind}"]`);
    await panel.locator("[data-role-select]").selectOption(READER);
    await expect(panel.locator("[data-editing-mode]")).toHaveText(READER_MODE);
    // Withheld capabilities are LISTED and struck through, not omitted — the
    // same rule as a disabled control, one level up.
    await expect(panel.locator('[data-capability="edit"]')).toHaveAttribute(
      "data-state",
      "withheld",
    );
    await expect(panel.locator('[data-capability="print"]')).toHaveAttribute(
      "data-state",
      "granted",
    );
  }

  // The element derives the frame's sandbox from the capability set, which is
  // the one layer the framed page cannot argue with: a frame without
  // `allow-downloads` cannot start a download however it is persuaded.
  const { frame: elementFrame, panel: elementPanel } = await mountEmbedPanel(page, "element", READER);
  const sandbox = await elementPanel.locator("iframe").getAttribute("sandbox");
  expect(sandbox.split(/\s+/)).not.toContain("allow-downloads");
  expect(sandbox.split(/\s+/)).toContain("allow-scripts");
  // The frame has an accessible name, from the HOST's markup — an iframe with
  // none is an axe `frame-title` violation, and this package has no business
  // inventing English for someone else's page.
  expect(await elementPanel.locator("iframe").getAttribute("title")).toBeTruthy();

  const { frame: iframeFrame, panel: iframePanel } = await mountEmbedPanel(page, "iframe", READER);

  // Both framed editors refuse the two commands that ARE the HF-109 defect —
  // File ▸ Open and File ▸ New are what let a visitor replace the host's
  // document from inside the host's own chrome. Disabled, present, and saying
  // why.
  for (const [panel, frame] of [
    [elementPanel, elementFrame],
    [iframePanel, iframeFrame],
  ]) {
    await panel.scrollIntoViewIfNeeded();
    const refused = await readPaletteRows(frame, ["file.open", "file.new"]);
    for (const row of refused) {
      expect(row.present, `${row.id} is missing from the palette`).toBe(true);
      expect(row.disabled, `${row.id} is offered as if it worked`).toBe(true);
      expect(row.reason.length, `${row.id} is disabled with no reason`).toBeGreaterThan(0);
    }
  }

  // THE POSITIVE CONTROL. Without this, a guard that found every command
  // disabled for an unrelated reason would pass, and this repository has shipped
  // exactly that kind of green-but-worthless assertion before (`105` CQ-003).
  const top = await page.context().newPage();
  await top.goto("/editor.html");
  await waitForFramedEditor(top.mainFrame());
  const standalone = await readPaletteRows(top.mainFrame(), ["file.open", "file.new"]);
  for (const row of standalone) {
    expect(row.present, `${row.id} is missing at top level`).toBe(true);
    expect(row.disabled, `${row.id} is refused to a standalone editor too`).toBe(false);
  }
  await top.close();
});

test(`a ${READER} host cannot mutate the document through the engine, chrome defeated`, async ({
  page,
}) => {
  await page.goto("/embed.html");
  const { frame } = await mountEmbedPanel(page, "element", READER);

  // THE ROLE ALONE, with nobody clicking anything.
  //
  // This is the assertion the phase turns on, and it is why the click that used
  // to be here is gone. Clicking the mode control first proved the engine
  // refuses mutation in Viewing — which was already true and already guarded by
  // `viewing-mode-gate`. It did NOT prove that a `readonly` HOST ends up behind
  // that gate, and while the startup hook was missing it did not: `?mode=readonly`
  // disabled `file.open`/`file.new` and left the page in Editing. So the strongest
  // claim in the suite was being made by the test's own click.
  //
  // Read from the authority, not hardcoded: if `editingModeFor` ever maps
  // `readonly` elsewhere, this follows it rather than passing on the wrong mode.
  await expect(
    frame.locator('#reviewModeControl [data-review-mode="' + READER_MODE + '"]'),
    'a ' + READER + ' embed must be in ' + READER_MODE + ' on arrival, not merely disabled',
  ).toHaveAttribute("aria-pressed", "true");
  // And the modes it was not granted are refused with a reason, still present —
  // a host reading a MISSING button cannot tell a permission from a bug.
  const editingButton = frame.locator('#reviewModeControl [data-review-mode="editing"]');
  await expect(editingButton, "a withheld mode must still be offered").toBeVisible();
  await expect(editingButton).toBeDisabled();
  await expect(editingButton).not.toHaveAttribute("title", "");

  const before = await engineStats(frame);
  expect(before.words, "the document has no words to protect").not.toBe("0 words");

  // ── The devtools attack ───────────────────────────────────────────────────
  // Strip every refusal the CHROME is making, so what is left is only what the
  // engine refuses. This is what someone with the element inspector open does
  // in ten seconds, and it is the whole reason a disabled attribute is not a
  // permission.
  const reEnabled = await frame.evaluate(() => {
    let count = 0;
    for (const element of document.querySelectorAll("[disabled], [aria-disabled='true']")) {
      if (element.hasAttribute("disabled")) element.removeAttribute("disabled");
      if (element.getAttribute("aria-disabled") === "true") {
        element.setAttribute("aria-disabled", "false");
      }
      count += 1;
    }
    return count;
  });
  expect(reEnabled, "nothing was disabled, so this proves nothing").toBeGreaterThan(0);

  // 1) Keyboard input, at a real caret.
  await frame.click(".page-wrap .page", { position: { x: 60, y: 60 } });
  await frame.locator("body").press("Home");
  await frame.locator("body").press("KeyX");
  await frame.locator("body").press("Backspace");
  await frame.locator("body").press("Delete");
  await frame.locator("body").press("Enter");

  // 2) Every formatting and structural control the ribbon offers, clicked
  //    directly. Enumerated rather than sampled: the point is that no ONE of
  //    them is the gate, so leaving one out would be the one that got through.
  const controls = [
    "bold",
    "italic",
    "underline",
    "strike",
    "subscript",
    "superscript",
    "bulletList",
    "numberedList",
    "checkList",
    "indentInc",
    "indentDec",
    "clearFormatting",
    "growFont",
    "shrinkFont",
    "changeCaseBtn",
    "undoBtn",
    "redoBtn",
  ];
  for (const id of controls) {
    const button = frame.locator(`#${id}`);
    if (await button.count()) await button.click({ force: true, timeout: 5_000 }).catch(() => {});
  }

  // 3) Paste, which is the path that does not go through a keystroke.
  await frame.evaluate(() => {
    const data = new DataTransfer();
    data.setData("text/plain", "PASTED-BY-A-READER");
    document
      .getElementById("editorTextInput")
      ?.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true }));
  });

  // ── The document is unchanged ─────────────────────────────────────────────
  // Read from the engine, and given a beat for any mutation that DID land to
  // repaint the counters — a stats assertion that runs before the repaint would
  // pass whether or not the edit applied.
  await page.waitForTimeout(400);
  const after = await engineStats(frame);
  expect(after, "a reader mutated the document through the engine").toEqual(before);

  // And it said why. A refusal with no sentence is the other half of the
  // defect: the reader is left unable to tell a permission from a bug.
  await expect(frame.locator("#status")).toContainText(/read-only/i);
});

test("the simplest possible embed is not a standalone editor", async ({ page }) => {
  // The case that matters most, because it is what a host writes before reading
  // anything: an element with NO mode at all. `109` HF-109 is that a framed
  // editor was a standalone one, so File ▸ Open and File ▸ New were live inside
  // someone else's page and a visitor could replace the host's document from
  // within the host's own chrome. A host must not have to discover a parameter
  // to avoid that.
  //
  // The markup is written here rather than added to `embed.html` on purpose: the
  // page's panels always carry a role, so nothing there ever exercises the
  // DEFAULT — and a default nothing exercises is one a later refactor can remove
  // with every test still green.
  await page.goto("/embed.html");
  await page.evaluate(() => {
    const element = document.createElement("opendoc-editor");
    element.setAttribute("editor-src", "./editor.html");
    element.setAttribute("frame-title", document.title);
    element.id = "bareEmbed";
    document.body.append(element);
  });
  const bare = page.locator("#bareEmbed");
  await expect(bare.locator("iframe")).toHaveCount(1);
  const frame = await (await bare.locator("iframe").elementHandle()).contentFrame();
  await waitForFramedEditor(frame);
  await bare.scrollIntoViewIfNeeded();

  const rows = await readPaletteRows(frame, ["file.open", "file.new"]);
  for (const row of rows) {
    expect(row.present, `${row.id} is missing from the palette`).toBe(true);
    expect(
      row.disabled,
      `${row.id} is live inside a host's page with no mode set — HF-109 itself`,
    ).toBe(true);
  }
});

test("the demo host is the page the package documents", async () => {
  // A README that shows an API the code does not have is the failure `docs/99`
  // §9.7 records about the design prototype, whose `doc.transaction()` and
  // `doc.writeDocx()` never existed. So the package's own attribute table is
  // checked against the element that reads them.
  const readme = readFileSync(new URL("../../../packages/opendoc-embed/README.md", import.meta.url), "utf8");
  const element = readFileSync(new URL("../../src/embed_element.mjs", import.meta.url), "utf8");
  for (const attribute of ["mode", "editor-src", "frame-title"]) {
    expect(readme, `the README does not document ${attribute}`).toContain(`\`${attribute}\``);
    expect(element, `the element does not read ${attribute}`).toContain(`"${attribute}"`);
  }
  // And the README must not promise `role`, which is the ARIA collision the
  // element deliberately avoids.
  expect(readme).not.toMatch(/<opendoc-editor\s+role=/);
});

/** Opens the command palette in `frame` and reports, for each command id,
 *  whether the row is present, whether it is disabled, and what reason it gives.
 *
 *  Three separate facts, because "absent" and "disabled" are different product
 *  behaviours and only one of them is acceptable. */
async function readPaletteRows(frame, ids) {
  // `openCommandPalette` goes through the File surface of whichever chrome is
  // showing — a real clickable path, not a chord — so this also proves a pointer
  // user can reach the palette inside an embed. It takes anything with
  // `.locator()`, which a Frame has, so the embedded editor is driven by exactly
  // the fixture every other spec uses rather than by a second copy of it.
  await openCommandPalette(frame);
  const rows = [];
  for (const id of ids) {
    await frame.locator("#cmdInput").fill(id.split(".").pop());
    const row = frame.locator(`#cmdList .cmd-item[data-command-id="${id}"]`);
    const present = (await row.count()) > 0;
    rows.push({
      id,
      present,
      disabled: present ? await row.first().isDisabled() : false,
      reason: present ? ((await row.first().getAttribute("title")) ?? "") : "",
    });
  }
  await frame.locator("#cmdInput").press("Escape");
  return rows;
}
