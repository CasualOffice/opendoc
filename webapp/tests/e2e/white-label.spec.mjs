// `docs/126` phase 3's exit gate, in a browser: a build white-labelled with NO
// CODE CHANGES that still passes the contrast sweep, and chrome composed per role.
//
// HOW A WHITE-LABELLED BUILD IS PRODUCED HERE, and why this is faithful rather
// than a simulation. White-labelling is a file swap: a host edits `brand.json`,
// runs `build-brand.mjs`, and their deployment serves a different `src/brand.css`
// and `src/brand.mjs`. So this spec ROUTES those two requests and fulfils them with
// the artifacts the generator produces from `brand.example.json` — the same bytes a
// white-labelled deployment would serve, arriving at the same moment in the page's
// life. Nothing is injected after load, which matters: `main.js` reads the pinned
// marker out of the computed palette at module evaluation, so a stylesheet added
// afterwards would prove nothing about the ordering that actually decides.
//
// THE CONDITION IS CREATED, NOT ASSUMED — the lesson phase 2 paid for, where a
// `readonly` test stayed green with the API's capability gate removed entirely. A
// contrast sweep over the DEFAULT build proves our palette passes, which was
// already true and already guarded by `theme-contrast.spec.mjs`. Every assertion
// below runs against a palette that is demonstrably not ours, and there is a
// positive control asserting so before the sweeps run.
import { readFileSync } from "node:fs";
import { expect, test, gotoEditor, runPaletteCommand } from "./fixtures.mjs";
import { auditRegion } from "./contrast-audit.mjs";

const { normalize, brandCss, brandModule, deriveStrings, markPaths, readCatalogues, PRODUCT } =
  await import("../../tools/build-brand.mjs");
const { readPalettes } = await import("../../tools/palette_source.mjs");
const { REGIONS, resolveRegions } = await import("../../src/capabilities.mjs");
const { regionClass } = await import("../../src/chrome_regions.mjs");

const CONFIG = normalize(
  JSON.parse(readFileSync(new URL("../../brand.example.json", import.meta.url), "utf8")),
  readPalettes().themes,
);
const BRAND_CSS = brandCss(CONFIG, markPaths(CONFIG));
const BRAND_MJS = brandModule(CONFIG, deriveStrings(CONFIG, readCatalogues()));

// Two editors boot in some of these tests and the default 60 s is for one.
test.describe.configure({ timeout: 180_000 });

/** Serves the white-labelled artifacts in place of the shipped ones, which is
 *  exactly what a white-labelled deployment does. */
async function whiteLabel(page) {
  await page.route("**/src/brand.css", (route) =>
    route.fulfill({ status: 200, contentType: "text/css", body: BRAND_CSS }),
  );
  await page.route("**/src/brand.mjs", (route) =>
    route.fulfill({ status: 200, contentType: "text/javascript", body: BRAND_MJS }),
  );
}

test("a white-labelled build carries the host's palette, and ours is nowhere in it", async ({ page }) => {
  await whiteLabel(page);
  await gotoEditor(page);

  // THE POSITIVE CONTROL, first: the palette really is the host's. Without this,
  // every sweep below could be measuring our own tokens and passing for the wrong
  // reason — which is the exact trap this phase was warned about.
  const palette = await page.evaluate(() => {
    const read = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    return {
      accent: read("--accent"),
      pinned: read("--brand-accent-pinned"),
      surface: read("--surface"),
      ink: read("--ink"),
      inlineAccent: document.documentElement.style.getPropertyValue("--accent"),
    };
  });
  expect(palette.accent, "the host's accent did not reach the page").toBe("#0f6d6a");
  expect(palette.accent).not.toBe("#3355c4");
  expect(palette.pinned).toBe("1");
  expect(palette.surface).toBe("#ffffff");
  expect(palette.ink).toBe("#1b1a18");

  // `docs/125` §2 F4, in a browser. `applySettings()` runs at import and used to
  // write an inline `--accent` from `localStorage`, which beats the host's
  // stylesheet; the pinned marker is how it learns not to.
  expect(
    palette.inlineAccent,
    "an inline --accent was written over the host's brand, which is F4 reintroduced",
  ).toBe("");
});

test("a host's stored accent cannot un-brand a white-labelled build", async ({ page }) => {
  // The sharp version of the case above: a visitor who HAS picked an accent. That
  // is the common case in a real deployment — the preference is remembered — and it
  // is what made F4 fire for everyone rather than for nobody.
  await whiteLabel(page);
  await page.addInitScript(() => {
    try {
      window.localStorage.setItem("opendoc.settings", JSON.stringify({ accent: "#d64562", theme: "light" }));
    } catch {
      /* storage blocked: the test below still asserts the host's colour */
    }
  });
  await gotoEditor(page);
  const state = await page.evaluate(() => ({
    accent: getComputedStyle(document.documentElement).getPropertyValue("--accent").trim(),
    inline: document.documentElement.style.getPropertyValue("--accent"),
    customValue: document.getElementById("accentCustom")?.value,
    customDisabled: document.getElementById("accentCustom")?.disabled,
    swatchDisabled: [...document.querySelectorAll("#accentSwatches .acc[data-accent]")].map((b) => b.disabled),
    swatchTitle: document.querySelector("#accentSwatches .acc[data-accent]")?.title ?? "",
    pressed: [...document.querySelectorAll("#accentSwatches .acc[data-accent]")].map((b) =>
      b.getAttribute("aria-pressed"),
    ),
  }));
  expect(state.accent, "the visitor's stored accent won").toBe("#0f6d6a");
  expect(state.inline).toBe("");
  // Disabled WITH A REASON, and showing the colour that is actually on screen.
  expect(state.customDisabled).toBe(true);
  expect(state.customValue).toBe("#0f6d6a");
  expect(state.swatchDisabled.every(Boolean)).toBe(true);
  expect(state.swatchTitle.length).toBeGreaterThan(0);
  expect(state.pressed.every((v) => v === "false")).toBe(true);
});

for (const theme of ["light", "dark"]) {
  test(`a host's ${theme} palette still passes the contrast sweep`, async ({ page, consoleErrors }) => {
    // THE EXIT GATE'S second half, measured rather than argued. The same
    // `contrast-audit.mjs` the editor's own theme sweep uses, over the same region,
    // on a palette that is not ours. Chrome resolves `var()` and `color-mix()` away
    // by computed-value time, so whatever chain the host installed is flattened
    // before this reads it.
    await whiteLabel(page);
    await gotoEditor(page);
    await page.addStyleTag({ content: "*,*::before,*::after{transition:none!important;animation:none!important}" });
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    // And the host's palette is in force for THIS theme, not just the other one —
    // the mistake HF-092 was: a patch in one dark entry point and not the other.
    const ground = await page.evaluate(() =>
      getComputedStyle(document.documentElement).getPropertyValue("--surface").trim(),
    );
    expect(ground).toBe(theme === "dark" ? "#202426" : "#ffffff");

    const result = await page.evaluate(auditRegion, { selector: "body" });
    expect(result.unresolved, "a backdrop the sweep could not resolve").toEqual([]);
    expect(result.examined, "too few text nodes examined for this to mean anything").toBeGreaterThan(60);
    expect(result.failures.map((f) => f.describe)).toEqual([]);
    expect(consoleErrors).toEqual([]);
  });
}

test("renaming the product renames it everywhere it was routed, and nowhere else", async ({ page }) => {
  await whiteLabel(page);
  await gotoEditor(page);
  // The host's name reached the chrome, through the SAME `t()` seam the nineteen
  // catalogues resolve through — no second mechanism.
  // `#aboutTitle` is a MARKUP key (`data-i18n="aboutDialog.aboutOpendoc"`), which is
  // the interesting half: it proves the override reached `localizeTree`'s sweep over
  // the page rather than only the strings a script formats. Read from a hidden
  // dialog on purpose — `textContent` does not care, and opening a modal to read one
  // heading would make this test about the modal.
  const chrome = await page.evaluate(() => ({
    about: document.getElementById("aboutTitle")?.textContent?.trim() ?? "",
    title: document.title,
  }));
  expect(chrome.about).toContain("Northwind Docs");
  expect(chrome.about).not.toContain(PRODUCT.name);
  // `tabTitle: "document"` — the host's browser tab is theirs. This was the single
  // most visible place our brand left the editor (`docs/125` §2 F4).
  expect(chrome.title).not.toContain(PRODUCT.name);
  expect(chrome.title.length).toBeGreaterThan(0);

  // And the POSITIVE CONTROL: the default build still says our name, so the
  // assertions above are about the white-label and not about a string that vanished.
  const plain = await page.context().newPage();
  await gotoEditor(plain);
  const ours = await plain.evaluate(() => document.getElementById("aboutTitle")?.textContent?.trim() ?? "");
  expect(ours).toContain(PRODUCT.name);
  await plain.close();
});

test("a deployment can say who runs it, and where to ask for help", async ({ page }) => {
  // ONLYOFFICE `customization.customer` and `customization.feedback`, which they
  // gate with everything else in that block. About is the one surface in an editor
  // whose job is saying who made and who runs this thing, and a white-label whose
  // deployment cannot say "this is Northwind, here is how to reach them" is a
  // white-label in name only.
  await whiteLabel(page);
  await gotoEditor(page);
  await runPaletteCommand(page, "help.about", "about");

  const block = page.locator("#aboutCustomer");
  await expect(block, "the host's identity never reached About").toBeVisible();
  await expect(block).toContainText("Northwind Trading Co.");
  await expect(block).toContainText("14 Harbour Road");
  // The LABEL is the host's too, which is the one place this improves on theirs:
  // their feedback button shows their own English, and a white-label cannot
  // afford a word it did not choose.
  const feedback = block.locator('a[href="https://example.invalid/northwind/support"]');
  await expect(feedback).toHaveText("Tell us what broke");
  await expect(feedback).toHaveAttribute("rel", /noopener/);
  await expect(block.locator('a[href="https://example.invalid/northwind/help"]')).toHaveText(
    "Northwind help centre",
  );

  // THE POSITIVE CONTROL. The shipped `brand.json` is all-null on purpose — the
  // default build is the product — so this block must not exist at all without a
  // host, and an empty bordered section on every deployment would be chrome
  // nobody asked for.
  const plain = await page.context().newPage();
  await gotoEditor(plain);
  await runPaletteCommand(plain, "help.about", "about");
  await expect(plain.locator("#aboutDialog")).toBeVisible();
  await expect(
    plain.locator("#aboutCustomer"),
    "the default build grew a customer block with nothing in it",
  ).toHaveCount(0);
  await plain.close();
});

test("a readonly container has no ribbon, and can still print", async ({ page }) => {
  // The owner's sentence in a browser: "not a ribbon full of greyed buttons — no
  // ribbon". And the other half, which is what makes it a reading experience rather
  // than a mutilated editor: the capability it IS granted stays reachable.
  await page.goto("/editor.html?fixture=rich&mode=readonly");
  await page.waitForFunction(() => document.body.dataset.chromeWithheld !== undefined);
  await expect(page.locator(".ribbon")).toBeHidden();
  await expect(page.locator(".ribbon-nav")).toBeHidden();
  // The menu bar is the reading container's one navigation axis, and where
  // File ▸ Print lives.
  await expect(page.locator("#appMenuBar")).toBeVisible();
  await page.locator('#appMenuBar [data-menu="file"]').click();
  const print = page.locator('#appMenuPopover [data-command="file.print"]');
  await expect(print, "a readonly reader must be able to reach Print").toBeVisible();
  await expect(print).toBeEnabled();
  await page.keyboard.press("Escape");

  // Reading chrome: navigate, search, see where you are.
  await expect(page.locator(".rail")).toBeVisible();
  await expect(page.locator(".footer")).toBeVisible();
  await expect(page.locator(".zoom")).toBeVisible();
  // Not an author's surfaces.
  await expect(page.locator("#settingsBtn")).toBeHidden();

  // THE POSITIVE CONTROL. Without it, a page that failed to boot would satisfy
  // every `toBeHidden` above.
  const top = await page.context().newPage();
  await gotoEditor(top);
  await expect(top.locator(".ribbon")).toBeVisible();
  await expect(top.locator("#settingsBtn")).toBeVisible();
  await top.close();
});

test("a preview container is the runtime as a rendering engine", async ({ page }) => {
  // `docs/126`: "Minimal to none. No ribbon, no menu bar. Possibly only the pages."
  // Its own test of the difference from `readonly` is "could a static image replace
  // it? For `preview`, nearly."
  await page.goto("/editor.html?fixture=rich&mode=preview");
  await page.waitForFunction(() => document.body.dataset.chromeWithheld !== undefined);
  for (const selector of [".ribbon", ".ribbon-nav", "#appMenuBar", ".rail", ".footer", "#settingsBtn"]) {
    await expect(page.locator(selector), `${selector} must be absent from a preview`).toBeHidden();
  }
  // The document is still there — that is the whole point of a preview.
  await expect(page.locator("#pages")).toBeVisible();
  // And every region is withheld, read from what the page itself published rather
  // than re-derived here.
  const withheld = await page.evaluate(() => document.body.dataset.chromeWithheld.split(" ").filter(Boolean));
  expect(withheld.sort()).toEqual([...REGIONS].sort());
});

test("a host can withhold one band, and the ribbon opens on a surviving one", async ({ page }) => {
  // Home carries `aria-selected="true"` in the markup, so the default band is
  // exactly the one a host is most likely to withhold — and without the
  // re-selection this container would open onto nothing.
  await page.goto("/editor.html?fixture=rich&mode=owner&chrome=-band.home,-band.table");
  await page.waitForFunction(() => document.body.dataset.chromeWithheld !== undefined);
  await expect(page.locator("#tabHome")).toBeHidden();
  await expect(page.locator("#panelHome")).toBeHidden();
  await expect(page.locator("#tabInsert")).toBeVisible();
  await expect(page.locator("#tabInsert")).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#panelInsert")).toBeVisible();
  // The ribbon itself survives: withholding a band is not withholding the ribbon.
  await expect(page.locator(".ribbon")).toBeVisible();

  // And the arrow keys skip the withheld band rather than walking onto an invisible
  // tab where `focus()` does nothing — the bug region composition would otherwise
  // have introduced.
  await page.locator("#tabInsert").focus();
  await page.keyboard.press("ArrowLeft");
  const focused = await page.evaluate(() => document.activeElement?.id ?? "");
  expect(focused, "the arrow keys landed on a withheld band").not.toBe("tabHome");
  await expect(page.locator(`#${focused}`)).toBeVisible();
});

test("the body classes a host's composition sets are the ones the authority decided", async ({ page }) => {
  // Read from the page and compared with the authority, so the chrome cannot paint
  // a composition that is not the resolved one.
  await page.goto("/editor.html?fixture=rich&mode=readonly");
  await page.waitForFunction(() => document.body.dataset.chromeWithheld !== undefined);
  const classes = await page.evaluate(() => [...document.body.classList]);
  const shown = resolveRegions({ mode: "readonly", framed: false });
  for (const id of REGIONS) {
    expect(classes.includes(regionClass(id)), `${id} class disagrees with the authority`).toBe(!shown.has(id));
  }
});
