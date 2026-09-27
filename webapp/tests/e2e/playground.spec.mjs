import { test, expect, waitForFramedEditor } from "./fixtures.mjs";

// The playground, driven the way a host drives it — because the whole claim of
// that page is that changing a control changes a RUNNING editor, and nothing in
// node can tell you whether it did.
//
// THE TRAP THIS SPEC IS WRITTEN AGAINST, twice recorded in `docs/126`: a guard
// that proves the DEFAULT configuration works proves nothing. Phase 2's
// `readonly` test stayed green when the API's capability gate was deleted,
// because a different layer caught the edits; phase 3's brand assertions stayed
// green when the generator's `throw` was deleted, because they called the audit
// helper instead of the command. So every test below CREATES THE CONDITION —
// withholds the thing, then looks in the live frame for its absence — and each one
// has been driven red by breaking the production code it covers.
//
// One document per mounted frame, and each frame is a ~100 MB WebAssembly
// instance, so tests mount once and reconfigure rather than mounting per
// assertion.

/** The live frame, after a mount or a remount.
 *
 *  Re-resolved every time rather than held: a role change REPLACES the iframe (the
 *  mode is decided before the frame's first navigation), so a handle taken before
 *  the change points at a detached document and every assertion after it would be
 *  about the editor that used to be there. */
async function liveFrame(page) {
  const iframe = page.locator("[data-stage] iframe");
  await expect(iframe).toHaveCount(1);
  const frame = await (await iframe.elementHandle()).contentFrame();
  await waitForFramedEditor(frame);
  return frame;
}

async function mount(page) {
  await page.goto("/playground.html");
  await page.getByRole("button", { name: /Start the editor/ }).click();
  return liveFrame(page);
}

/** Switches the role and waits for the editor that replaced the old one. */
async function chooseRole(page, role) {
  await page.locator(`#pg-role-${role}`).check();
  return liveFrame(page);
}

/** What the chrome of a live frame actually paints, measured rather than assumed.
 *
 *  `offsetParent` is null for a `display: none` ancestor, which is exactly how
 *  region composition takes a band away — so this reports what a person would see,
 *  not what is in the markup. */
async function painted(frame) {
  return frame.evaluate(() => {
    const shown = (selector) => {
      const el = document.querySelector(selector);
      if (!el) return false;
      const box = el.getBoundingClientRect();
      return box.width > 0 && box.height > 0;
    };
    return {
      ribbon: shown(".ribbon"),
      menu: shown("#appMenuBar"),
      rail: shown(".rail"),
      status: shown("footer.footer"),
      withheld: document.body.dataset.chromeWithheld ?? "",
      // Which review mode the container arrived in, and which ones it is allowed to
      // choose. A host-withheld mode is DISABLED WITH A REASON rather than removed
      // (`reflectReviewModeAccess`), so both halves are worth reading.
      mode:
        document.querySelector("[data-review-mode][aria-pressed='true']")?.dataset.reviewMode ?? "",
      editingOffered: !document.querySelector("[data-review-mode='editing']")?.disabled,
    };
  });
}

// ---- The owner's point: preview and readonly are not the same thing ---------

test("readonly is reading chrome with no ribbon; preview is neither, in the live frame", async ({
  page,
  consoleErrors,
}) => {
  await mount(page);

  const editing = await painted(await chooseRole(page, "edit"));
  expect(editing.ribbon, "an editing container has the ribbon").toBe(true);

  const readonly = await painted(await chooseRole(page, "readonly"));
  // The whole of `docs/126`'s container policy §2 in three assertions: a role with
  // no business with a surface does not get the surface, and a role that DOES get
  // one keeps the navigation axis its grant needs — `readonly` grants `print`, and
  // File ▸ Print lives on the menu bar once the ribbon is gone.
  expect(readonly.ribbon, "a readonly container must not have the editing ribbon").toBe(false);
  expect(readonly.menu, "a readonly container keeps the menu bar, or print is unreachable").toBe(
    true,
  );
  expect(readonly.rail, "a reader navigates, so the rail stays").toBe(true);
  expect(readonly.status, "the status bar carries the page count a reader scrolls by").toBe(true);

  const preview = await painted(await chooseRole(page, "preview"));
  expect(preview.ribbon).toBe(false);
  expect(preview.menu, "preview is the rendering engine, not a reading experience").toBe(false);
  expect(preview.rail).toBe(false);
  expect(preview.status).toBe(false);

  // And they are different SETS, not merely two states that happen to look alike
  // today. Collapsing them is the specific mistake `docs/126` names.
  expect(preview.withheld).not.toEqual(readonly.withheld);
  expect(preview.withheld.split(" ").length).toBeGreaterThan(
    readonly.withheld.split(" ").length,
  );

  expect(consoleErrors).toEqual([]);
});

// ---- Per capability, not per tier -------------------------------------------

test("withholding one capability from a role changes the live editor and the API", async ({
  page,
  consoleErrors,
}) => {
  // "Comments only, everything else off" is a real configuration (`docs/126`
  // container policy §1), so a role has to be narrowable one capability at a time —
  // and the narrowing has to reach the ENGINE, not just the buttons. Withholding
  // `edit` from `edit` leaves `comment`, so the editor must arrive in Suggesting
  // rather than Editing, and an editing command over the host API must come back
  // refused.
  const before = await mount(page);
  expect((await painted(before)).editingOffered, "an edit container may choose Editing").toBe(true);
  const allowed = await before.evaluate(async () => {
    const session = window.opendoc;
    return session ? (await session.execute("format.bold")).ok : null;
  });
  expect(allowed, "an edit-granted container runs an editing command").toBe(true);

  await page.locator("#pg-cap-edit").uncheck();
  const after = await liveFrame(page);

  // The URL the page resolved really carries the narrowing, minus sign and all.
  expect(after.url()).toContain("can=-edit");
  const refused = await after.evaluate(async () => {
    const session = window.opendoc;
    return session ? await session.execute("format.bold") : null;
  });
  expect(refused.ok, "an editing command must not run in a container without edit").toBe(false);
  expect(refused.refusal.code).toBe("capability-withheld");
  // The chrome agrees, and says why rather than hiding the control — the right half
  // of `docs/126`'s container policy for a surface this role still gets.
  const chrome = await painted(after);
  expect(chrome.editingOffered, "Editing is still offered after edit was withheld").toBe(false);
  expect(chrome.mode).toBe("suggesting");

  // And the readout on the HOST page says so, from the same resolution.
  await expect(page.locator("[data-caps] li[data-capability='edit']")).toHaveAttribute(
    "data-state",
    "withheld",
  );
  await expect(page.locator("[data-caps] li[data-capability='comment']")).toHaveAttribute(
    "data-state",
    "granted",
  );
  expect(consoleErrors).toEqual([]);
});

test("a capability a role never granted is disabled and says why, rather than missing", async ({
  page,
}) => {
  // "Never a dead control" applied to the configuration page itself, and the
  // sentence is the part that teaches: a list can only narrow, so there is exactly
  // one direction to audit. A control that vanished would leave a host wondering
  // whether the capability exists.
  await page.goto("/playground.html");
  await page.locator("#pg-role-readonly").check();
  const open = page.locator("#pg-cap-open");
  await expect(open).toBeDisabled();
  await expect(open).not.toBeChecked();
  await expect(open).toHaveAttribute("title", /can only narrow/i);
  // And one the role does grant is live.
  await expect(page.locator("#pg-cap-print")).toBeEnabled();
});

// ---- Regions ----------------------------------------------------------------

test("withholding a region takes it out of the live frame, and putting it back restores it", async ({
  page,
  consoleErrors,
}) => {
  await mount(page);
  expect((await painted(await liveFrame(page))).ribbon).toBe(true);

  await page.locator("#pg-chrome-ribbon").uncheck();
  const without = await liveFrame(page);
  expect(without.url()).toContain("chrome=-ribbon");
  const gone = await painted(without);
  expect(gone.ribbon, "the withheld region is still painted").toBe(false);
  // Containment, derived rather than asked of the host twice: the bands live inside
  // the ribbon, so dropping the ribbon drops all eight.
  expect(gone.withheld).toContain("band.home");

  await page.locator("#pg-chrome-ribbon").check();
  expect((await painted(await liveFrame(page))).ribbon).toBe(true);
  expect(consoleErrors).toEqual([]);
});

// ---- The brand, and the refusal ---------------------------------------------

test("a passing accent lands on the running editor with no reload", async ({ page }) => {
  const frame = await mount(page);
  const documentUrl = frame.url();
  const before = await frame.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue("--accent").trim(),
  );

  await page.locator("#pg-brand-accent-hex").fill("#0f766e");
  await expect(page.locator("[data-refusal]")).toBeHidden();

  // The SAME frame — not a remount. A palette is a stylesheet, and a white-labelled
  // build is a swapped static file; reloading the document to change a colour would
  // be the page teaching the opposite.
  const same = await liveFrame(page);
  expect(same.url(), "the document was re-opened to change a colour").toBe(documentUrl);
  await expect
    .poll(async () =>
      same.evaluate(() =>
        getComputedStyle(document.documentElement).getPropertyValue("--accent").trim(),
      ),
    )
    .toBe("#0f766e");
  expect(before).not.toBe("#0f766e");

  // And the copyable configuration carries it, so the state is reproducible.
  await expect(page.locator('[data-snippet="brand"]')).toContainText('"--accent": "#0f766e"');
});

test("an accent that fails AA is refused in the generator's own words, and is not applied", async ({
  page,
}) => {
  const frame = await mount(page);
  const before = await frame.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue("--accent").trim(),
  );

  const failing = (await page.locator("[data-claim='failing-accent']").textContent()).trim();
  await page.getByRole("button", { name: /Try an accent that fails AA/ }).click();

  const refusal = page.locator("[data-refusal]");
  await expect(refusal).toBeVisible();
  // The four things a host needs in order to act: which pair, what it measured,
  // what it must clear, and a value that would. This is `BrandRefusal`'s sentence,
  // produced by the code `node webapp/tools/build-brand.mjs` runs.
  await expect(refusal).toContainText("brand.json was refused");
  await expect(refusal).toContainText(failing);
  await expect(refusal).toContainText(/measures \d+\.\d\d:1 and must clear 4\.5:1/);
  await expect(refusal).toContainText(/would pass, on the same hue/);

  // REFUSED MEANS NOT APPLIED. The alternative — showing the message and painting
  // the colour anyway — is the nag-modal failure mode, and it would leave the
  // editor in a state nobody could ship.
  await page.waitForTimeout(250);
  expect(
    await (await liveFrame(page)).evaluate(() =>
      getComputedStyle(document.documentElement).getPropertyValue("--accent").trim(),
    ),
    "a refused colour reached the editor anyway",
  ).toBe(before);
});

// ---- The copyable configuration ---------------------------------------------

test("the snippet reproduces the state it was generated from", async ({ page, consoleErrors }) => {
  // The promise on the page is "paste it into your own page and get the same
  // result", so the test is to take what it printed and mount a SECOND editor from
  // it, with nothing else carried over, and compare the two.
  await mount(page);
  await page.locator("#pg-role-commentor").check();
  await page.locator("#pg-cap-download").uncheck();
  await page.locator("#pg-chrome-rail").uncheck();
  const configured = await painted(await liveFrame(page));

  const snippet = await page.locator('[data-snippet="element"]').textContent();
  const src = snippet.match(/editor-src="([^"]+)"/)?.[1].replaceAll("&amp;", "&");
  expect(src, `no editor-src in the snippet:\n${snippet}`).toBeTruthy();
  expect(snippet).toContain('mode="commentor"');

  const pasted = await page.context().newPage();
  await pasted.goto(`/${src.replace(/^\.\//, "")}`);
  await waitForFramedEditor(pasted.mainFrame());
  const reproduced = await painted(pasted.mainFrame());
  await pasted.close();

  expect(reproduced.withheld).toEqual(configured.withheld);
  expect(reproduced.ribbon).toEqual(configured.ribbon);
  expect(reproduced.rail).toEqual(configured.rail);
  expect(consoleErrors).toEqual([]);
});

// ---- The page itself ---------------------------------------------------------

test("every control has an accessible name and a keyboard path", async ({ page }) => {
  await page.goto("/playground.html");
  const unnamed = await page.evaluate(() => {
    const bad = [];
    for (const control of document.querySelectorAll(
      ".pg-controls input, .pg-controls select, .pg-controls button, .pg-out button",
    )) {
      const labelled =
        control.getAttribute("aria-label") ||
        control.labels?.length ||
        control.textContent.trim();
      if (!labelled) bad.push(control.id || control.outerHTML.slice(0, 60));
      if (control.tabIndex < 0) bad.push(`${control.id}: not reachable by Tab`);
    }
    return bad;
  });
  expect(unnamed).toEqual([]);

  // And the first switch really takes focus and toggles from the keyboard, rather
  // than merely being in the tab order.
  await page.locator("#pg-cap-save").focus();
  await expect(page.locator("#pg-cap-save")).toBeFocused();
  await page.keyboard.press("Space");
  await expect(page.locator("#pg-cap-save")).not.toBeChecked();
  await expect(page.locator("[data-caps] li[data-capability='save']")).toHaveAttribute(
    "data-state",
    "withheld",
  );
});

test("the page fits a phone without scrolling sideways", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/playground.html");
  const overflow = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
    widest: [...document.querySelectorAll(".pg *")]
      .filter((el) => el.getBoundingClientRect().right > document.documentElement.clientWidth + 1)
      .map((el) => `${el.tagName.toLowerCase()}.${el.className}`.slice(0, 60))
      .slice(0, 5),
  }));
  expect(overflow.widest).toEqual([]);
  expect(overflow.scrollWidth).toBeLessThanOrEqual(overflow.clientWidth + 1);
});

test("the two SDK pages point at each other", async ({ page }) => {
  // A page nobody can get to is the reachability defect one level up (`docs/99`
  // §9.4). The guide is where the tables are; the playground is where they move.
  await page.goto("/playground.html");
  await expect(page.locator('main a[href="./embedding.html"]').first()).toBeVisible();
  await page.goto("/embedding.html");
  await expect(page.locator('main a[href="./playground.html"]').first()).toBeVisible();
});
