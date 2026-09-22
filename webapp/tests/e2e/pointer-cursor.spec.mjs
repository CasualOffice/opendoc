// The pointer shape says what the thing under it will do (`docs/109` HF-179).
//
// Reported as "cursor is always in editor (I) but i think it should change based
// on where it is.. like for dragging, changing side, check box and other .. im
// just giving example". Measured before the fix, on a grid of 160 points across
// the demo document's first sheet: the computed cursor was `text` at EVERY point
// that was over paper. A page is one `<canvas>`, so CSS had nothing to key on
// and the only variation in the whole surface was a hyperlink class.
//
// The table and the cascade are unit-tested without a browser
// (`tests/pointer_cursor.test.mjs`). What can only be proven here is that the
// router is actually WIRED: that a real pointer over a real rendered picture, a
// real checkbox and a real resize grip gets the real cursor. So every assertion
// below reads `getComputedStyle(...).cursor` — the value the browser will
// actually draw — and, where the router decided it, the `data-pointer-target`
// the router stamped, so a passing cursor cannot be the right shape reached for
// the wrong reason.
import {
  test,
  expect,
  gotoEditor,
  stableBox,
  setReviewMode,
  MOD,
} from "./fixtures.mjs";

const FORM = "../fixtures/generated/form-checkbox.docx";

/** The floating image sits near the top-left of page 1 in the float fixture
 *  (the same point `object-edit.spec.mjs` and `object-anchor.spec.mjs` click). */
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

/** What the browser will draw at a screen point, and why.
 *
 *  `cursor` is the computed value on whatever element is actually under the
 *  point — not on a locator we hoped was there. `target` is the router's own
 *  verdict, present only for the canvas rows it decides; CSS-owned chrome
 *  leaves it empty, which is itself the distinction being asserted. */
async function read(page, x, y) {
  return page.evaluate(
    ([px, py]) => {
      const el = document.elementFromPoint(px, py);
      if (!el) return { cursor: "", target: "", element: "none" };
      return {
        cursor: getComputedStyle(el).cursor,
        target: el.dataset?.pointerTarget ?? "",
        element: `${el.tagName.toLowerCase()}.${el.className}`,
      };
    },
    [x, y],
  );
}

/** Moves the pointer and waits for the router's frame to land. The router is
 *  throttled to one animation frame, so reading immediately after the move
 *  races it — and a race here would read the PREVIOUS target and pass for the
 *  wrong reason. Polling on the stamped target is what closes that. */
async function routed(page, x, y) {
  // Two moves: a pointermove only fires on a change of position, and a spec that
  // re-checks the same point would otherwise never wake the router at all.
  await page.mouse.move(x - 1, y - 1);
  await page.mouse.move(x, y);
  let seen = { cursor: "", target: "", element: "" };
  await expect
    .poll(async () => {
      seen = await read(page, x, y);
      return seen.target;
    })
    .not.toBe("");
  return seen;
}

test.describe("the pointer says what the thing under it will do", () => {
  test("body text, the desk around it, and the header band", async ({ page, consoleErrors }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await gotoEditor(page);
    const wrap = page.locator(".page-wrap").first();
    const box = await stableBox(wrap);

    // Body text: the I-beam, and reached as body text rather than by falling
    // through from somewhere else.
    const body = await routed(page, box.x + box.width * 0.5, box.y + box.height * 0.45);
    expect(body).toMatchObject({ cursor: "text", target: "body-text" });

    // The grey surround is the desk, not the paper: a press there resolves to no
    // page, so the arrow is the honest shape.
    const desk = await read(page, box.x - 30, box.y + 200);
    expect(desk.cursor, `over ${desk.element}`).toBe("default");

    // The top margin band. Deliberately still an I-beam — a single click there
    // does place a caret, in the nearest body line — but it must be REACHED as
    // the band, because that is what makes the decision a decision.
    const band = await routed(page, box.x + box.width * 0.5, box.y + 8);
    expect(band).toMatchObject({ cursor: "text", target: "running-content-band" });

    // …and the band's own affordance, the button that enters it, is a hand.
    const marker = page.locator(".running-marker");
    await expect(marker).toBeVisible();
    const markerBox = await stableBox(marker);
    const onMarker = await read(page, markerBox.x + 4, markerBox.y + markerBox.height / 2);
    expect(onMarker.cursor).toBe("pointer");

    expect(consoleErrors).toEqual([]);
  });

  test("a picture: move to drag it, an axis on each grip, move while dragging", async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await gotoFloat(page);
    const canvas = page.locator(".page-wrap .page").first();
    const box = await stableBox(canvas);
    const at = {
      x: box.x + box.width * FLOAT_POS.fx,
      y: box.y + box.height * FLOAT_POS.fy,
    };

    // Over the picture, unselected: it is movable, so the pointer offers a move.
    // This is the single most visible half of the report.
    const onImage = await routed(page, at.x, at.y);
    expect(onImage).toMatchObject({ cursor: "move", target: "object-movable" });

    // Select it, then check each of the eight grips carries its own axis. The
    // grips are overlay DOM, so these are CSS-owned and stamp no target.
    await canvas.click({
      position: { x: box.width * FLOAT_POS.fx, y: box.height * FLOAT_POS.fy },
    });
    await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
    const axes = {
      0: "nwse-resize",
      1: "ns-resize",
      2: "nesw-resize",
      3: "ew-resize",
      4: "nwse-resize",
      5: "ns-resize",
      6: "nesw-resize",
      7: "ew-resize",
    };
    for (const [handle, axis] of Object.entries(axes)) {
      const grip = page.locator(`.overlay .object-handle[data-handle="${handle}"]`);
      if ((await grip.count()) === 0) continue; // the engine omits unsupported grips
      await expect(grip).toHaveCSS("cursor", axis);
    }

    // And a drag in flight keeps the move cursor even where the pointer has run
    // past the object — it is `body` that carries it off the sheet.
    await page.mouse.move(at.x, at.y);
    await page.mouse.down();
    await page.mouse.move(at.x + 60, at.y + 40);
    await page.mouse.move(at.x + 120, at.y + 80);
    await expect
      .poll(() => page.evaluate(() => document.body.style.cursor))
      .toBe("move");
    await page.mouse.up();
    // Released: the gesture gives the cursor back without needing a move.
    expect(await page.evaluate(() => document.body.style.cursor)).toBe("");

    expect(consoleErrors).toEqual([]);
  });

  test("a picture whose geometry is locked does not offer a move", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await gotoFloat(page);
    const canvas = page.locator(".page-wrap .page").first();
    // Suggesting refuses resize, crop and the move commit — a tracked revision
    // cannot represent them — so promising a move here would be a lie the
    // release then breaks. The box is measured AFTER the switch: the mode banner
    // pushes the sheet down, and coordinates taken before it land in the header.
    await setReviewMode(page, "suggesting");
    const box = await stableBox(canvas);
    const locked = await routed(
      page,
      box.x + box.width * FLOAT_POS.fx,
      box.y + box.height * FLOAT_POS.fy,
    );
    expect(locked).toMatchObject({ cursor: "default", target: "object-locked" });
  });

  test("a form checkbox is a control, until the document refuses edits", async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await gotoEditor(page);
    await page.locator("#file").setInputFiles(FORM);
    await expect(page.locator("#a11yDocument")).toContainText("Tick one");

    // Find the control by keyboard so no pixel is guessed: the caret the editor
    // paints at the start of the control's paragraph sits on the glyph.
    await page.locator("#pages").click({ position: { x: 300, y: 40 } });
    await page.keyboard.press(`${MOD}+Home`);
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Home");
    const caret = await stableBox(page.locator(".overlay .caret").first());
    const at = { x: caret.x + 3, y: caret.y + caret.height / 2 };

    const onBox = await routed(page, at.x, at.y);
    expect(onBox).toMatchObject({ cursor: "pointer", target: "form-checkbox" });

    // In Viewing the box cannot be ticked, so it is not a control any more —
    // only text you can still select and copy.
    await setReviewMode(page, "viewing");
    const viewing = await routed(page, at.x, at.y);
    expect(viewing).toMatchObject({ cursor: "text", target: "read-only-text" });

    expect(consoleErrors).toEqual([]);
  });

  test("a hyperlink offers the hand, and the format painter keeps its brush", async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await gotoEditor(page);

    // Make a link rather than hunt for one, so the point under the pointer is
    // known exactly: select the first word and apply a web address.
    await page.locator("#pages").click({ position: { x: 300, y: 40 } });
    await page.keyboard.press(`${MOD}+Home`);
    for (let i = 0; i < 6; i += 1) await page.keyboard.press("Shift+ArrowRight");
    const highlight = await stableBox(page.locator(".overlay .highlight").first());
    await page.keyboard.press(`${MOD}+k`);
    await expect(page.locator("#linkDialog")).toBeVisible();
    await page.locator("#linkUrlInput").fill("https://example.com");
    await page.locator("#linkUrlInput").press("Enter");
    await expect(page.locator("#linkDialog")).toBeHidden();

    const onLink = await routed(
      page,
      highlight.x + highlight.width / 2,
      highlight.y + highlight.height / 2,
    );
    expect(onLink).toMatchObject({ cursor: "pointer", target: "hyperlink" });

    // Arming the format painter is a whole-surface mode, and its brush must win
    // over whatever the router would otherwise have written here — including the
    // hand it just wrote. The router's job in this state is to write nothing.
    await page.locator("#formatPainter").click();
    await page.mouse.move(highlight.x + 4, highlight.y + 2);
    await page.mouse.move(highlight.x + highlight.width / 2, highlight.y + highlight.height / 2);
    await expect
      .poll(async () =>
        (await read(page, highlight.x + highlight.width / 2, highlight.y + highlight.height / 2))
          .cursor,
      )
      .toMatch(/^url\(/);

    expect(consoleErrors).toEqual([]);
  });

  test("the surface is no longer one cursor everywhere", async ({ page }) => {
    // The report in one assertion. Sweep the sheet and require the router to
    // have produced MORE THAN ONE answer — the state before this change would
    // fail here with a single `text`, whatever else was on the page.
    await page.setViewportSize({ width: 1280, height: 900 });
    await gotoFloat(page);
    const box = await stableBox(page.locator(".page-wrap").first());
    const seen = new Set();
    // Every point has to be inside the WINDOW, not merely inside the sheet: a
    // sheet is taller than the viewport, so a fraction near 1 moves the pointer
    // off-screen, where no pointermove is delivered and the router never runs.
    for (const [fx, fy] of [
      [FLOAT_POS.fx, FLOAT_POS.fy], // the picture
      [0.5, 0.01], // the header band
      [0.5, 0.5], // body text
    ]) {
      const at = await routed(page, box.x + box.width * fx, box.y + box.height * fy);
      seen.add(`${at.target}:${at.cursor}`);
    }
    expect(
      [...seen].sort(),
      "the pointer must distinguish what is under it; one entry here is the defect",
    ).toEqual([
      "body-text:text",
      "object-movable:move",
      "running-content-band:text",
    ]);
  });
});
