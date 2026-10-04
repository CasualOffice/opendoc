// One modal contract, asserted against every modal (docs/104 HF-043, HF-062,
// HF-063, HF-070, HF-089).
//
// The tracker's finding was not "Split cell is broken" — it was that nine
// dialogs had three different dismissal contracts, so there was no rule for a
// user to learn and no rule for a reviewer to check. `webapp/tests/e2e` had no
// dialog-dismissal spec at all, which is why a dialog could ship without
// Escape, without backdrop dismissal, and without a focus trap, and nothing
// turned red.
//
// So this spec is written against the CONTRACT rather than against any one
// dialog: a table of every `aria-modal` surface in the editor, driven through
// the same four assertions. The last test closes the loop — if a new
// `aria-modal` element appears in editor.html and is not in the table, this
// spec fails, so the next dialog cannot ship without the contract either.
import { test, expect, gotoEditor, MOD, expectEditorFocused } from "./fixtures.mjs";
// The table moved to `modal-roster.mjs` so the fit and the phone guards can ask
// their questions of the same 23 dialogs this one asks its four of, instead of
// each keeping a hand-grown subset — which is how `dialog-fit` came to cover 8.
import { MODALS, EDITOR_SURFACE } from "./modal-roster.mjs";

// An icon-font glyph's CONTENT AREA is ascent+descent, and for Material Symbols
// that is taller than the 1em line box `.ms` gives it — about two pixels at an
// 18px glyph. `.ms` clips that on purpose (`overflow: hidden` in fonts.css):
// the excess is the font's line gap, not ink. So the height rule is relaxed for
// icons by this much and no more, and two checks that DO bite are asserted in
// its place: the box must still be at least one em tall, and the glyph must
// never overflow horizontally. Shrink an icon's box and this still fails.
// The constant is declared INSIDE `measureClipping`: that function is
// serialised into the page, where nothing in this module's scope exists.
//
/** Every element inside `id` whose content is larger than the box it is painted
 *  in, measured rather than inspected.
 *
 *  This is the measurable form of "the label does not fit". `#confirmAccept`
 *  reading "Discard and open" wrapped to a second line inside a fixed 32px box
 *  and reported `scrollHeight` 59 against `clientHeight` 32 — the second line
 *  simply was not painted, and the overflow propagated up through
 *  `.dialog-actions`, `.dialog-foot` and the card itself. Every assertion that
 *  the older specs make about that dialog — it is visible, it has the right
 *  text, it is focused, it dismisses — passes in exactly that state. Only the
 *  geometry says anything, so the geometry is what this asserts. */
const measureClipping = (id) => {
  const ICON_LINE_GAP = 4;
  const root = document.getElementById(id);
  const out = [];
  for (const el of [root, ...root.querySelectorAll("*")]) {
    if (el.getClientRects().length === 0) continue; // not painted: nothing to clip
    const style = getComputedStyle(el);
    const where =
      (el.id && `#${el.id}`) ||
      (typeof el.className === "string" && el.className.trim()
        ? `${el.tagName.toLowerCase()}.${el.className.trim().split(/\s+/)[0]}`
        : el.tagName.toLowerCase());
    const box = `${el.clientWidth}x${el.clientHeight}`;
    const content = `${el.scrollWidth}x${el.scrollHeight}`;
    if (el.classList.contains("ms")) {
      const em = parseFloat(style.fontSize) || 0;
      if (el.scrollWidth > el.clientWidth + 1) {
        out.push(`${where}: icon is ${content} wide in a ${box} box`);
      }
      if (el.clientHeight + 1 < em) {
        out.push(`${where}: icon box is ${el.clientHeight}px tall for a ${em}px glyph`);
      }
      if (el.scrollHeight > el.clientHeight + ICON_LINE_GAP) {
        out.push(`${where}: icon content ${content} exceeds its ${box} box by more than the line gap`);
      }
      continue;
    }
    // An element that scrolls ON PURPOSE is allowed more content than box in
    // that axis — that is what the scrollbar is for. `.dialog-body` is the one
    // that actually uses it.
    const scrollsX = /auto|scroll/.test(style.overflowX);
    const scrollsY = /auto|scroll/.test(style.overflowY);
    if (!scrollsX && el.scrollWidth > el.clientWidth + 1) {
      out.push(`${where}: content ${content} is wider than its ${box} box`);
    }
    if (!scrollsY && el.scrollHeight > el.clientHeight + 1) {
      out.push(`${where}: content ${content} is taller than its ${box} box`);
    }
  }
  return out;
};

for (const modal of MODALS) {
  const dialog = (page) => page.locator(`#${modal.id}`);

  test(`${modal.name}: nothing in it is clipped`, async ({ page, consoleErrors }) => {
    await modal.open(page);
    await expect(dialog(page)).toBeVisible();
    const clipped = await page.evaluate(measureClipping, modal.id);
    expect(
      clipped,
      `#${modal.id} paints content outside the boxes that hold it:\n  ${clipped.join("\n  ")}`,
    ).toEqual([]);
    expect(consoleErrors).toEqual([]);
  });

  // `hidden` is the weakest promise in the platform: it is a UA `display: none`,
  // and ANY author `display` on the element beats the entire UA sheet. So a
  // component rule as ordinary as `display: flex` silently un-hides everything
  // it matches, with no warning anywhere — not in the markup, which still says
  // `hidden`, and not in the JS, which still sets it.
  //
  // That shipped. `.link-mode-panel { display: flex }` was painting the INACTIVE
  // tab panel of the link dialog: with "Web address" selected, the dialog showed
  // the URL box and, twelve pixels under it, the bookmark picker reading "No
  // bookmarks in this document" plus its explanatory note — 52px of a 509px
  // dialog that the user had not chosen and could not act on, which also made
  // the two tabs look like they did nothing. Found by screenshot; no test in the
  // suite could see it, because every assertion about that panel asked the
  // locator whether it was visible AFTER clicking its own tab.
  //
  // The stylesheet answers this per component in about a dozen places
  // (`.ribbon-panel[hidden]`, `.side-panel[hidden]`, `.context-menu[hidden]`, …)
  // and every one of those is a rule someone had to remember. This is the guard
  // that means nobody has to: whatever the app has decided to hide inside a
  // modal, the modal must not be painting it.
  test(`${modal.name}: nothing it has hidden is painted`, async ({ page, consoleErrors }) => {
    await modal.open(page);
    await expect(dialog(page)).toBeVisible();
    const painted = await page.evaluate((id) => {
      const out = [];
      for (const el of document.getElementById(id).querySelectorAll("[hidden]")) {
        // `getClientRects()` is empty for anything `display: none` anywhere up
        // the chain, so a hidden element inside a hidden ancestor is correctly
        // silent here rather than double-reported.
        const rects = el.getClientRects();
        if (rects.length === 0) continue;
        const { width, height } = rects[0];
        if (width === 0 && height === 0) continue;
        out.push(
          `${el.tagName.toLowerCase()}${el.id ? `#${el.id}` : ""}` +
            `${el.className ? `.${String(el.className).trim().split(/\s+/).join(".")}` : ""}` +
            ` is [hidden] and painting ${Math.round(width)}x${Math.round(height)}` +
            ` (display: ${getComputedStyle(el).display})`,
        );
      }
      return out;
    }, modal.id);
    expect(
      painted,
      `#${modal.id} paints elements it has marked hidden — an author \`display\` ` +
        `beats the UA sheet's \`[hidden] { display: none }\`:\n  ${painted.join("\n  ")}`,
    ).toEqual([]);
    expect(consoleErrors).toEqual([]);
  });

  // Chrome text interpolates user data, and the measured guard above only sees
  // whatever string the fixture happens to supply. It passed on `opendoc-demo.docx`
  // while the owner's `General_Loan_On_lend_and_loan_from_SMSF_Agreement.docx` was
  // painted past the card's right edge and clipped mid-word: CSS does not break on
  // an underscore, so that filename is ONE 48-character token and a `fit-content`
  // card cannot shrink below it.
  //
  // So the fixture must be hostile. A dialog that renders no interpolated text is
  // skipped rather than asserted vacuously.
  test(`${modal.name}: an unbreakable 60-character token cannot escape the card`, async ({
    page,
    consoleErrors,
  }) => {
    await modal.open(page);
    await expect(dialog(page)).toBeVisible();
    const TOKEN = "A_really_long_unbreakable_filename_token_with_no_spaces_1234.docx";
    const planted = await page.evaluate(
      ({ id, token }) => {
        const root = document.getElementById(id);
        const target = [...root.querySelectorAll("p, h2, .dialog-body, .dialog-note")].find(
          (n) => (n.textContent || "").trim().length > 0,
        );
        if (!target) return false;
        target.textContent = `${target.textContent.trim()} ${token}`;
        return true;
      },
      { id: modal.id, token: TOKEN },
    );
    test.skip(!planted, "this dialog renders no interpolated text");
    const clipped = await page.evaluate(measureClipping, modal.id);
    expect(
      clipped,
      `#${modal.id} clips an unbreakable token instead of wrapping it:\n  ${clipped.join("\n  ")}`,
    ).toEqual([]);
    expect(consoleErrors).toEqual([]);
  });

  test(`${modal.name}: opening moves focus in, Escape closes and restores it`, async ({
    page,
    consoleErrors,
  }) => {
    await modal.open(page);
    await expect(dialog(page)).toBeVisible();
    await expect(page.locator(modal.focus).first()).toBeFocused();

    await page.keyboard.press("Escape");
    await expect(dialog(page)).toBeHidden();
    if (modal.opener) {
      await expect(page.locator(modal.opener).first()).toBeFocused();
    } else if (modal.restore === EDITOR_SURFACE) {
      // The requirement is "focus returns to the editing surface", not "to
      // `#pages`". Focus is owned by the editable proxy, since a non-editable
      // div raises no soft keyboard and fires no composition events
      // (docs/105 UX-001); naming the element here made the contract describe a
      // mechanism instead of the guarantee.
      await expectEditorFocused(page);
    } else if (modal.restore) {
      await expect(page.locator(modal.restore)).toBeFocused();
    } else {
      // No opener survives the route, so the requirement is the weaker but
      // still load-bearing one: the keyboard must not be left on <body>, which
      // is what made the editor look frozen after Split cell (HF-062).
      const active = await page.evaluate(() => document.activeElement?.tagName ?? "");
      expect(active).not.toBe("BODY");
    }
    expect(consoleErrors).toEqual([]);
  });

  test(`${modal.name}: a backdrop click dismisses it`, async ({ page, consoleErrors }) => {
    await modal.open(page);
    await expect(dialog(page)).toBeVisible();
    // Press and release on the scrim itself — the top-left corner is outside
    // every dialog card, which is centred.
    await dialog(page).click({ position: { x: 4, y: 4 } });
    await expect(dialog(page)).toBeHidden();
    expect(consoleErrors).toEqual([]);
  });

  test(`${modal.name}: Tab cycles inside it and cannot leave`, async ({
    page,
    consoleErrors,
  }) => {
    await modal.open(page);
    await expect(dialog(page)).toBeVisible();
    // Enough presses to walk past the end of any of these dialogs and wrap.
    const visited = new Set();
    for (let i = 0; i < 30; i += 1) {
      await page.keyboard.press("Tab");
      const seen = await page.evaluate((id) => {
        const active = document.activeElement;
        return {
          inside: document.getElementById(id).contains(active),
          // Identity of the focused control, so the test can tell "Tab cycles
          // through the dialog" from "Tab escapes and something drags focus
          // back to the first control every time".
          at: active?.id || active?.getAttribute("aria-label") || active?.textContent?.trim().slice(0, 24) || "?",
        };
      }, modal.id);
      expect(seen.inside, `focus left #${modal.id} after ${i + 1} Tab presses`).toBe(true);
      visited.add(seen.at);
    }
    const focusableCount = await page.evaluate(
      (id) =>
        [
          ...document
            .getElementById(id)
            .querySelectorAll(
              "a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])",
            ),
        ].filter((node) => node.getClientRects().length > 0).length,
      modal.id,
    );
    if (focusableCount > 1) {
      expect(visited.size, `Tab never moved off one control in #${modal.id}`).toBeGreaterThan(1);

      // Backwards off the front must wrap to the BACK. Containment alone would
      // put focus on the first control again, which reads as Shift+Tab being
      // broken; the cycle has to run both ways to be a cycle.
      const wrapped = await page.evaluate((id) => {
        const items = [
          ...document
            .getElementById(id)
            .querySelectorAll(
              "a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])",
            ),
        ].filter((node) => node.getClientRects().length > 0);
        items[0].focus();
        return { first: items[0] === document.activeElement, last: items[items.length - 1] };
      }, modal.id);
      expect(wrapped.first).toBe(true);
      await page.keyboard.press("Shift+Tab");
      const atLast = await page.evaluate((id) => {
        const items = [
          ...document
            .getElementById(id)
            .querySelectorAll(
              "a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])",
            ),
        ].filter((node) => node.getClientRects().length > 0);
        return items[items.length - 1] === document.activeElement;
      }, modal.id);
      expect(atLast, `Shift+Tab off the first control did not wrap to the last in #${modal.id}`).toBe(true);
    }
    expect(consoleErrors).toEqual([]);
  });

  test(`${modal.name}: application shortcuts do not fire behind it`, async ({
    page,
    consoleErrors,
  }) => {
    await modal.open(page);
    await expect(dialog(page)).toBeVisible();
    // HF-063: ⌘F used to open Find behind the scrim and take focus with it.
    await page.keyboard.press(`${MOD}+f`);
    await expect(page.locator("#findPanel")).toBeHidden();
    const inside = await page.evaluate(
      (id) => document.getElementById(id).contains(document.activeElement),
      modal.id,
    );
    expect(inside).toBe(true);
    expect(consoleErrors).toEqual([]);
  });
}

test("every aria-modal surface in the editor is covered by this spec", async ({ page }) => {
  await gotoEditor(page);
  const declared = await page.evaluate(() =>
    [...document.querySelectorAll('[aria-modal="true"]')].map(
      // The palette's aria-modal lives on the inner .cmd-box; the overlay it
      // sits in is the element the contract is registered against.
      (element) => (element.id || element.closest("[id]")?.id) ?? "",
    ),
  );
  const covered = MODALS.map((modal) => modal.id).sort();
  expect(declared.filter(Boolean).sort()).toEqual(covered);
});

test("body scroll is locked while a modal is open and released after", async ({ page }) => {
  await gotoEditor(page);
  await expect(page.locator("body")).not.toHaveClass(/modal-open/);
  await page.locator("#propertiesBtn").click();
  await expect(page.locator("body")).toHaveClass(/modal-open/);
  await page.keyboard.press("Escape");
  await expect(page.locator("body")).not.toHaveClass(/modal-open/);
});

test("a modal paints above the review chrome, and takes it off screen", async ({ page }) => {
  await gotoEditor(page);
  const ladder = await page.evaluate(() => {
    const value = (name) =>
      Number(getComputedStyle(document.documentElement).getPropertyValue(name).trim());
    return {
      modal: value("--z-modal"),
      palette: value("--z-palette"),
      reviewPopover: value("--z-review-popover"),
      reviewCard: value("--z-review-card"),
      inspector: value("--z-inspector"),
    };
  });
  // HF-089: the pinned tracked-change card used to sit at 85 over a dialog at
  // 70, so Accept/Reject stayed live above a blocking modal.
  expect(ladder.modal).toBeGreaterThan(ladder.palette);
  expect(ladder.palette).toBeGreaterThan(ladder.reviewCard);
  expect(ladder.reviewCard).toBeGreaterThan(ladder.reviewPopover);
  expect(ladder.reviewPopover).toBeGreaterThan(ladder.inspector);
});

test("a primary dialog button keeps its text legible while the pointer is on it", async ({
  page,
}) => {
  // Found by hovering one and looking, and it was in EVERY dialog in the
  // product. `.dialog-button:hover:not(:disabled)` is (0,3,0) and
  // `.dialog-button-primary:hover` was (0,2,0), so the generic hover won on a
  // primary button: it repainted the background to `--bg-2` and left the white
  // primary text on it — measured rgb(251,250,248) on rgb(244,246,251), about
  // 1.03:1, against `docs/63`'s AA floor of 4.5:1 for text.
  //
  // No existing guard could see it. The contrast sweep reads RESTING state, and
  // every behavioural test passed because the button still worked — the text
  // was simply not there any more. So this one hovers, and measures.
  await gotoEditor(page);
  await page.locator("#propertiesBtn").click();
  const button = page.locator(".dialog-card:not([hidden]) .dialog-button-primary").first();
  await expect(button).toBeVisible();
  await button.hover();

  const { ratio, color, background } = await button.evaluate(async (el) => {
    const mod = await import("/src/contrast.mjs");
    const parse = (value) => {
      const [r, g, b] = value.match(/[\d.]+/g).map(Number);
      return { r, g, b };
    };
    const style = getComputedStyle(el);
    // Walk out for the painted background, exactly as a reader's eye does: a
    // transparent button shows whatever is behind it.
    let node = el;
    let painted = parse(style.backgroundColor);
    while (node && getComputedStyle(node).backgroundColor.startsWith("rgba(0, 0, 0, 0)")) {
      node = node.parentElement;
      if (node) painted = parse(getComputedStyle(node).backgroundColor);
    }
    return {
      ratio: mod.contrastRatio(parse(style.color), painted),
      color: style.color,
      background: `rgb(${painted.r}, ${painted.g}, ${painted.b})`,
    };
  });

  expect(
    ratio,
    `a hovered primary dialog button reads ${color} on ${background} — AA needs 4.5:1`,
  ).toBeGreaterThanOrEqual(4.5);
});
