// SCRATCH probe (branch feat/image-direct-manipulation) — deleted before the PR.
import { test, expect, gotoEditor } from "./fixtures.mjs";

const SHOTS =
  "/tmp/claude-501/-Users-sachin-Desktop-melp-services-opendoc/4d1217fc-ff39-40ad-aea9-7434a5864b68/scratchpad/shots";
const IMAGE_POS = { fx: 0.32, fy: 0.1 };
const FLOAT_POS = { fx: 0.14, fy: 0.11 };

async function gotoFloat(page) {
  await page.goto("/editor.html?fixture=float");
  await page.waitForFunction(
    () => {
      const s = document.getElementById("status");
      return s && s.textContent === "" && document.querySelectorAll(".page-wrap").length > 0;
    },
    null,
    { timeout: 45_000 },
  );
}

async function pointOn(page, pos) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  return { x: box.x + box.width * pos.fx, y: box.y + box.height * pos.fy };
}

async function outlineBox(page) {
  const n = await page.locator(".overlay .object-outline").count();
  if (!n) return null;
  return page.locator(".overlay .object-outline").first().evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height) };
  });
}

async function guides(page) {
  return page.locator(".object-align-guide, .align-guide, .object-guide, .overlay [class*=guide]").count();
}

test("G1-G3 inline image: click, corner, edge, crop", async ({ page }) => {
  test.setTimeout(180_000);
  await gotoEditor(page);
  const p = await pointOn(page, IMAGE_POS);
  const r = {};

  // hover FIRST, so the cursor router has run
  await page.mouse.move(p.x, p.y);
  await page.waitForTimeout(400);
  r.cursorHoverUnselected = await page.evaluate(
    () => getComputedStyle(document.querySelector(".page")).cursor,
  );

  await page.mouse.click(p.x, p.y);
  await page.waitForTimeout(300);
  r.handles = await page.locator(".overlay .object-handle").evaluateAll((n) => n.map((e) => e.dataset.handle));
  r.bar = (await page.locator(".object-context-bar").textContent()) ?? "(none)";
  r.wrapBtnCount = await page.locator(".object-wrap-btn").count();
  await page.mouse.move(p.x + 2, p.y + 2);
  await page.waitForTimeout(400);
  r.cursorHoverSelected = await page.evaluate(
    () => getComputedStyle(document.querySelector(".page")).cursor,
  );
  await page.screenshot({ path: `${SHOTS}/A1-inline-selected.png` });
  r.boxAtSelect = await outlineBox(page);

  // corner drag (SE = 4)
  const se = page.locator('.overlay .object-handle[data-handle="4"]');
  const b = await se.boundingBox();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + 90, b.y + 10, { steps: 8 });
  r.readoutDuringResize = (await page.locator(".object-resize-readout").count())
    ? await page.locator(".object-resize-readout").textContent()
    : "(none)";
  r.guidesDuringResize = await guides(page);
  await page.screenshot({ path: `${SHOTS}/A2-corner-drag.png` });
  await page.mouse.up();
  await page.waitForTimeout(400);
  r.boxAfterCorner = await outlineBox(page);
  r.stillSelectedAfterCorner = await page.locator("#pages").getAttribute("data-object-mode");
  await page.screenshot({ path: `${SHOTS}/A3-after-corner.png` });

  // crop: double-click, then the button
  await page.mouse.dblclick(p.x, p.y);
  await page.waitForTimeout(500);
  r.cropHandlesAfterDblclick = await page.locator(".overlay .object-crop-handle").count();
  r.modeAfterDblclick = await page.locator("#pages").getAttribute("data-object-mode");
  r.barAfterDblclick = (await page.locator(".object-context-bar").count())
    ? await page.locator(".object-context-bar").textContent()
    : "(no bar)";
  r.barVisibleAfterDblclick = await page.locator(".object-context-bar").isVisible().catch(() => false);
  await page.screenshot({ path: `${SHOTS}/A4-dblclick.png` });

  // re-select and use the Crop button
  await page.mouse.click(p.x, p.y);
  await page.waitForTimeout(300);
  const cropBtn = page.locator(".object-bar-actions button", { hasText: /Crop/ });
  r.cropBtnCount = await cropBtn.count();
  if (r.cropBtnCount) {
    await cropBtn.first().click();
    await page.waitForTimeout(400);
    r.cropHandles = await page.locator(".overlay .object-crop-handle").count();
    r.cropStatus = await page.locator("#status").textContent();
    await page.screenshot({ path: `${SHOTS}/A5-crop-mode.png` });
    const ch = page.locator('.overlay .object-crop-handle[data-handle="3"]');
    const cb = await ch.boundingBox();
    await page.mouse.move(cb.x + 6, cb.y + 6);
    await page.mouse.down();
    await page.mouse.move(cb.x - 30, cb.y + 6, { steps: 6 });
    r.cropReadout = (await page.locator(".object-crop-readout, .object-resize-readout").count())
      ? await page.locator(".object-crop-readout, .object-resize-readout").first().textContent()
      : "(none)";
    await page.screenshot({ path: `${SHOTS}/A6-crop-drag.png` });
    await page.mouse.up();
    await page.keyboard.press("Escape");
  }

  console.log("PROBE_A " + JSON.stringify(r, null, 2));
  expect(true).toBe(true);
});

test("G4-G7 floating image: wrap, move, arrows, alt text", async ({ page }) => {
  test.setTimeout(180_000);
  await gotoFloat(page);
  const p = await pointOn(page, FLOAT_POS);
  const r = {};

  await page.mouse.move(p.x, p.y);
  await page.waitForTimeout(400);
  r.cursorHover = await page.evaluate(() => getComputedStyle(document.querySelector(".page")).cursor);
  await page.mouse.click(p.x, p.y);
  await page.waitForTimeout(300);
  r.handles = await page.locator(".overlay .object-handle").evaluateAll((n) => n.map((e) => e.dataset.handle));
  r.bar = await page.locator(".object-context-bar").textContent();
  r.wrapBtns = await page.locator(".object-wrap-btn").evaluateAll((n) =>
    n.map((e) => `${e.textContent}${e.getAttribute("aria-pressed") === "true" ? "*" : ""}`),
  );
  await page.screenshot({ path: `${SHOTS}/B1-float-selected.png` });

  // G5: drag to move
  const before = await outlineBox(page);
  await page.mouse.move(p.x, p.y);
  await page.mouse.down();
  await page.mouse.move(p.x + 100, p.y + 70, { steps: 10 });
  r.previewDuringMove = await page.locator(".overlay .object-resize-preview").count();
  r.readoutDuringMove = (await page.locator(".object-resize-readout").count())
    ? await page.locator(".object-resize-readout").textContent()
    : "(none)";
  r.guidesDuringMove = await guides(page);
  r.cursorDuringMove = await page.evaluate(
    () => getComputedStyle(document.querySelector(".page")).cursor,
  );
  await page.screenshot({ path: `${SHOTS}/B2-move-drag.png` });
  await page.mouse.up();
  await page.waitForTimeout(400);
  const after = await outlineBox(page);
  r.moveDelta = { dx: after.x - before.x, dy: after.y - before.y };
  await page.screenshot({ path: `${SHOTS}/B3-after-move.png` });

  // does it snap to page centre? drag so the image centre is ~4px off page centre
  const canvas = page.locator(".page-wrap .page").first();
  const cbox = await canvas.boundingBox();
  const cur = await outlineBox(page);
  const wantCx = cbox.x + cbox.width / 2 + 5;
  const grabX = cur.x + cur.w / 2;
  const grabY = cur.y + cur.h / 2;
  await page.mouse.move(grabX, grabY);
  await page.mouse.down();
  await page.mouse.move(wantCx, grabY, { steps: 10 });
  r.guidesNearCentre = await guides(page);
  await page.screenshot({ path: `${SHOTS}/B4-near-centre.png` });
  await page.mouse.up();
  await page.waitForTimeout(400);
  const snapped = await outlineBox(page);
  r.centreOffsetAfterDrag = Math.round(snapped.x + snapped.w / 2 - (cbox.x + cbox.width / 2));

  // G6: arrow nudge
  const preArrow = await outlineBox(page);
  await page.keyboard.press("ArrowRight");
  await page.waitForTimeout(300);
  const postArrow = await outlineBox(page);
  r.arrowDx = postArrow ? postArrow.x - preArrow.x : "selection lost";
  r.modeAfterArrow = await page.locator("#pages").getAttribute("data-object-mode");
  await page.screenshot({ path: `${SHOTS}/B5-after-arrow.png` });

  // G7: alt text
  const altBtn = page.locator(".object-bar-actions button", { hasText: /Alt text/ });
  r.altBtn = await altBtn.count();
  if (r.altBtn) {
    await altBtn.first().click();
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${SHOTS}/B6-alt-text.png` });
    r.altDialog = await page.locator("dialog[open], .dialog[open]").count();
    await page.keyboard.press("Escape");
  }

  console.log("PROBE_B " + JSON.stringify(r, null, 2));
  expect(true).toBe(true);
});

test("COST: engine calls per pointer move during a move drag and a resize drag", async ({ page }) => {
  test.setTimeout(180_000);
  await gotoFloat(page);
  const p = await pointOn(page, FLOAT_POS);
  await page.mouse.click(p.x, p.y);
  await page.waitForTimeout(300);

  // Instrument every method on the live engine handle reachable from the overlay:
  // patch the wasm Document prototype, which main.js calls through.
  const installed = await page.evaluate(() => {
    // find the engine object by walking a known DOM-bound closure is impossible;
    // instead count wasm export entries via performance marks is also unavailable.
    // Use the wasm module's exported class prototype on the module namespace.
    const mod = window.__casualDocModule;
    return !!mod;
  });

  // Fall back to a DOM-work proxy: count overlay child mutations per pointermove,
  // which is the observable per-gesture host cost.
  await page.evaluate(() => {
    window.__probe = { moves: 0, mutations: 0 };
    const ov = document.querySelector(".page-wrap .overlay");
    const mo = new MutationObserver((recs) => {
      window.__probe.mutations += recs.length;
    });
    mo.observe(ov, { childList: true, subtree: true, attributes: true });
    window.addEventListener("pointermove", () => { window.__probe.moves += 1; }, true);
    window.__probeObserver = mo;
  });

  const box = await outlineBox(page);
  await page.mouse.move(box.x + box.w / 2, box.y + box.h / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.w / 2 + 120, box.y + box.h / 2 + 90, { steps: 30 });
  const moveCost = await page.evaluate(() => ({ ...window.__probe }));
  await page.mouse.up();

  console.log("PROBE_COST installed=" + installed + " " + JSON.stringify(moveCost));
  expect(true).toBe(true);
});
