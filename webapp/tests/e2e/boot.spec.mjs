// The designed boot (PROPOSAL D-16; docs/109 UX-064): SHELL-08, FILE-01/02/03,
// SYSTEM-01, MOBILE-07.
//
// What the audit measured, and what each test here refuses to let come back:
//
//   * the compact menu bar and the ribbon tab strip painted on top of each other
//     for 2-6 s of every cold load, and FOREVER when the engine failed, because
//     the chrome mode was decided by a module that ran seconds after first paint;
//   * a CLS of 0.062, most of it an empty 40px compact-toolbar slot the ribbon
//     then jumped up over;
//   * a phone painted the desktop ribbon for ~3 s before turning into a phone;
//   * a failed engine download was one 13px red status line, with no way to
//     retry and a ribbon that still looked usable;
//   * the ribbon looked (and was) clickable before the engine could run anything.
//
// The page is sampled every 50 ms FROM COMMIT by an init script, so the frames a
// person sees between first paint and `doc-loaded` are what is asserted, not the
// settled page a normal spec waits for.
import { test, expect } from "./fixtures.mjs";

const WASM = "**/casual_doc_wasm_bg.wasm*";

/** Installed before any document script: a 50 ms sampler of which navigation
 *  axes are visible, plus a layout-shift accumulator. Both run from the moment
 *  the document is created, which is before the first paint. */
async function instrument(page) {
  await page.addInitScript(() => {
    const samples = [];
    let cls = 0;
    const shifts = [];
    try {
      new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          if (entry.hadRecentInput) continue;
          cls += entry.value;
          shifts.push({
            t: Math.round(entry.startTime),
            value: entry.value,
            sources: (entry.sources || []).map((s) => {
              const n = s.node;
              return n ? `${n.nodeName}#${n.id || ""}.${(n.className && n.className.baseVal === undefined ? n.className : "") || ""}` : "?";
            }),
          });
        }
      }).observe({ type: "layout-shift", buffered: true });
    } catch {
      /* no layout-shift support: the CLS assertion below then fails loudly */
    }
    const visible = (el) => {
      if (!el) return false;
      if (typeof el.checkVisibility === "function" && !el.checkVisibility({ visibilityProperty: true, opacityProperty: true })) return false;
      const r = el.getBoundingClientRect();
      return r.width > 0 && r.height > 0;
    };
    const visibleIn = (selector) => [...document.querySelectorAll(selector)].some(visible);
    const sample = () => {
      if (!document.body) return;
      samples.push({
        t: Math.round(performance.now()),
        menuBar: visibleIn("#appMenuBar .app-menu-button"),
        ribbonTabs: visibleIn(".ribbon-tabs [role=tab]"),
        ribbonBand: visibleIn(".ribbon .ribbon-body"),
        loaded: document.body.classList.contains("doc-loaded"),
      });
    };
    const timer = setInterval(sample, 50);
    document.addEventListener("readystatechange", sample);
    window.__boot = {
      samples,
      shifts,
      get cls() {
        return cls;
      },
      stop: () => clearInterval(timer),
    };
  });
}

async function waitForDocument(page) {
  await page.waitForFunction(
    () =>
      document.body.classList.contains("doc-loaded") &&
      document.querySelectorAll(".page-wrap").length > 0 &&
      document.body.dataset.fontsReady === "true",
    null,
    { timeout: 45_000 },
  );
  // A beat past the font upgrade, so a late shift is still inside the window.
  await page.waitForTimeout(300);
  const result = await page.evaluate(() => {
    window.__boot?.stop();
    return window.__boot ? { samples: window.__boot.samples, shifts: window.__boot.shifts, cls: window.__boot.cls } : null;
  });
  if (result) test.info().annotations.push({ type: "cls", description: result.cls.toFixed(4) });
  return result;
}

function twoNavFrames(samples) {
  return samples.filter((s) => s.menuBar && s.ribbonTabs);
}

test.describe("designed boot", () => {
  test("ribbon chrome: one navigation axis in every frame, and no layout shift", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await instrument(page);
    await page.goto("/editor.html?fixture=rich");
    const { samples, shifts, cls } = await waitForDocument(page);
    expect(samples.length, "the sampler ran").toBeGreaterThan(5);
    expect(twoNavFrames(samples), "frames with the menu bar AND the ribbon tabs visible").toEqual([]);
    expect(samples.filter((s) => s.menuBar), "the menu bar never paints in ribbon chrome").toEqual([]);
    expect(cls, `CLS ${cls.toFixed(4)} — shifts: ${JSON.stringify(shifts)}`).toBeLessThan(0.01);
  });

  test("compact chrome: the ribbon never paints, and no layout shift", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.addInitScript(() => {
      try {
        localStorage.setItem("opendoc.chromeMode", "compact");
      } catch {
        /* storage blocked: the test then exercises the default */
      }
    });
    await instrument(page);
    await page.goto("/editor.html?fixture=rich");
    const { samples, shifts, cls } = await waitForDocument(page);
    expect(twoNavFrames(samples)).toEqual([]);
    expect(samples.filter((s) => s.ribbonTabs || s.ribbonBand), "ribbon frames in compact chrome").toEqual([]);
    expect(cls, `CLS ${cls.toFixed(4)} — shifts: ${JSON.stringify(shifts)}`).toBeLessThan(0.01);
  });

  test("phone width: the desktop ribbon never flashes before the phone chrome", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await instrument(page);
    await page.goto("/editor.html?fixture=rich");
    const { samples } = await waitForDocument(page);
    expect(samples.filter((s) => s.ribbonTabs || s.ribbonBand), "ribbon frames on a phone").toEqual([]);
    // The skeleton says the editor is loading while it is (MOBILE-07 counted 0
    // loading indicators); it is gone once the document is in.
    await expect(page.locator("#bootSplash")).toBeHidden();
  });

  test("chrome controls are inert until the engine is ready, with a progress bar meanwhile", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    let release;
    const gate = new Promise((resolve) => (release = resolve));
    await page.route(WASM, async (route) => {
      await gate;
      await route.continue();
    });
    await page.goto("/editor.html?fixture=rich");
    await page.waitForFunction(() => document.getElementById("tabInsert") !== null && document.readyState !== "loading");
    // The guarantee, not the mechanism: a control nobody can focus is a control
    // nobody can activate from the keyboard or the pointer. EVERY control in the
    // chrome is tried, not a hand-picked few, so a region that lost its `inert`
    // is caught whichever control in it happens to be enabled early.
    const sweep = () =>
      page.evaluate(() => {
        const regions = "header.bar, .ribbon, #compactToolbar, .workarea > .rail, footer.footer";
        const controls = [...document.querySelectorAll(regions)].flatMap((r) => [
          ...r.querySelectorAll("button, input, select, textarea, a[href], [tabindex]"),
        ]);
        const tried = controls.filter((el) => !el.disabled && el.getClientRects().length > 0);
        const focusable = tried.filter((el) => {
          el.focus();
          return document.activeElement === el;
        });
        document.activeElement?.blur?.();
        return { tried: tried.length, focusable: focusable.map((el) => el.id || el.className || el.tagName) };
      });
    const before = await sweep();
    expect(before.tried, "controls drawn in the chrome before the engine is ready").toBeGreaterThan(5);
    expect(before.focusable, "controls focusable before the engine is ready").toEqual([]);
    const named = () =>
      page.evaluate(() =>
        ["tabInsert", "tabHome", "settingsBtn", "headerModeBtn"]
          .map((id) => document.getElementById(id))
          .filter(Boolean)
          .map((el) => {
            el.focus();
            return { id: el.id, focused: document.activeElement === el };
          }),
      );
    await expect(page.locator("#bootSplash")).toBeVisible();
    await expect(page.locator("#bootSplash [role=progressbar]")).toBeVisible();

    release();
    await waitForDocument(page);
    const after = await named();
    expect(after.length).toBeGreaterThan(2);
    // Every region the boot made inert is live again: nothing is left holding
    // the attribute that marks it for lifting.
    expect(await page.locator("[data-boot-inert], [inert]").count(), "regions still inert once ready").toBe(0);
    expect(after.filter((c) => !c.focused), "controls still inert once ready").toEqual([]);
  });

  test("the progress bar is determinate, fed by the engine's bytes, when their total is known", async ({ page }) => {
    // The dev server gzips the wasm, which hides the total (the deploy stamps it
    // as `data-bytes`, `stamp_assets.test.mjs`). Served uncompressed here, so the
    // stream's own length is the denominator, and the bar must use it.
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.route(WASM, async (route) => {
      const response = await route.fetch();
      const body = await response.body();
      await route.fulfill({ status: 200, body, headers: { "content-type": "application/wasm", "content-length": String(body.length) } });
    });
    await page.addInitScript(() => {
      window.__progress = [];
      new MutationObserver(() => {
        const bar = document.getElementById("bootProgress");
        if (bar) window.__progress.push({ state: bar.dataset.state, now: bar.getAttribute("aria-valuenow") });
      }).observe(document, { subtree: true, attributes: true, attributeFilter: ["aria-valuenow", "data-state"] });
    });
    await page.goto("/editor.html?fixture=rich");
    await waitForDocument(page);
    const progress = await page.evaluate(() => window.__progress);
    const determinate = progress.filter((p) => p.state === "determinate");
    expect(determinate.length, `progress ${JSON.stringify(progress)}`).toBeGreaterThan(0);
    expect(determinate.at(-1).now).toBe("100");
  });

  test("a startup document that fails to open gives way to the upload card, not an endless skeleton", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.route("**/demo.docx*", (route) => route.fulfill({ status: 404, body: "" }));
    await page.goto("/editor.html?fixture=rich");
    await expect(page.locator(".drop-card")).toBeVisible({ timeout: 30_000 });
    await expect(page.locator("#bootSplash")).toBeHidden();
  });

  test("a failed engine download shows a failure card with Try again, and Try again recovers", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await instrument(page);
    let blocked = true;
    await page.route(WASM, (route) => (blocked ? route.abort("failed") : route.continue()));
    await page.goto("/editor.html?fixture=rich");

    const card = page.locator("#bootFailure");
    await expect(card).toBeVisible({ timeout: 30_000 });
    await expect(card).toHaveAttribute("role", "alert");
    const retry = card.getByRole("button", { name: "Try again" });
    await expect(retry).toBeVisible();
    await expect(retry).toBeFocused();
    // The technical string is there for whoever reports it, behind Details, not
    // in the status bar and not in a toast.
    await expect(card.locator("details")).toBeVisible();
    await expect(page.locator("#status")).not.toHaveClass(/error/);
    await expect(page.locator(".toast.error, .toast[data-kind=error]")).toHaveCount(0);
    // The chrome stays inert behind the card, and still shows ONE navigation axis.
    expect(await page.evaluate(() => {
      const el = document.getElementById("tabInsert");
      el.focus();
      return document.activeElement === el;
    })).toBe(false);
    const failed = await page.evaluate(() => window.__boot.samples);
    expect(twoNavFrames(failed), "frames with two navigation axes on a failed boot").toEqual([]);

    blocked = false;
    await retry.click();
    await waitForDocument(page);
    await expect(card).toBeHidden();
    await expect(page.locator(".page-wrap")).not.toHaveCount(0);
  });
});
