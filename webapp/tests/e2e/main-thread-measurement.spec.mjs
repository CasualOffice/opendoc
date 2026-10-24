// The committed probe behind `docs/180` §2's tables (RM-13).
//
// A published number is generated from a committed artifact or it is not
// published (`SKILL.md` §9). This is that artifact for the main-thread blocking
// figures: the longest tasks while a document OPENS (import, first layout,
// first paint), on the first and second KEYSTROKE after it, during a large
// PASTE, during the re-layout after a STYLE change, and after select-all +
// bold, on `sample.docx` and on the generated large fixture.
//
// It is **not** part of the normal suite — it prints a table rather than
// asserting a budget (the budget is `main-thread-budget.spec.mjs`). Run it
// deliberately:
//
//     cd webapp && ./build.sh
//     MEASURE_MAIN_THREAD=1 npx playwright test main-thread-measurement --project=chromium
//
// `MAIN_THREAD_PARAGRAPHS=2000,20000` overrides the large-fixture sizes, and
// `MAIN_THREAD_PROFILE=1` adds a sampled CPU profile of one keystroke per fixture.
//
// What is measured, and why each one:
//
// - **longest task** — the longest single main-thread task, via
//   `PerformanceObserver` `longtask` entries. While it runs nothing paints, no
//   click lands and no Cancel can be pressed.
// - **tasks > 50 ms** — every long task, so a 900 ms open that is one task can
//   be told from one that is eighteen 50 ms slices.
// - **time to interactive** — from handing the file to the picker until the
//   document reports open AND the main thread has then gone 500 ms without a
//   long task: the point a click would land promptly, not the point the open
//   "completed".
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  applyParagraphStyle,
  expect,
  gotoEditor,
  offeredParagraphStyles,
  reflectedParagraphStyle,
  test,
} from "./fixtures.mjs";
import { largeDocx } from "./large_docx_fixture.mjs";

const SIZES = (process.env.MAIN_THREAD_PARAGRAPHS ?? "2000,20000")
  .split(",")
  .map((v) => Number(v.trim()))
  .filter((v) => Number.isFinite(v) && v > 0);

test.skip(!process.env.MEASURE_MAIN_THREAD, "measurement probe: set MEASURE_MAIN_THREAD=1");

async function observe(page) {
  await page.evaluate(() => {
    window.__tasks = [];
    window.__taskObserver?.disconnect();
    window.__taskObserver = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        window.__tasks.push({ start: entry.startTime, duration: Math.round(entry.duration) });
      }
    });
    window.__taskObserver.observe({ entryTypes: ["longtask"] });
    window.__t0 = performance.now();
  });
}

/** Waits for 500 ms with no long task, then reports. */
async function settle(page) {
  return page.evaluate(
    () =>
      new Promise((resolve) => {
        const check = () => {
          const tasks = window.__tasks ?? [];
          const last = tasks.length ? tasks[tasks.length - 1] : null;
          const quietSince = last ? last.start + last.duration : window.__t0;
          if (performance.now() - quietSince >= 500) {
            resolve({
              worst: tasks.length ? Math.max(...tasks.map((t) => t.duration)) : 0,
              tasks: tasks.map((t) => t.duration),
              tti: Math.round(quietSince - window.__t0),
            });
          } else setTimeout(check, 100);
        };
        setTimeout(check, 300);
      }),
  );
}

const statParas = (page) =>
  page.evaluate(() =>
    Number((document.getElementById("statParas")?.textContent ?? "").replace(/[^0-9]/g, "")),
  );

async function openFile(page, file, paragraphs) {
  await observe(page);
  await page.locator("#file").setInputFiles(file);
  await page.waitForFunction(
    (want) =>
      document.body.classList.contains("doc-loaded") &&
      document.querySelectorAll(".page-wrap").length > 0 &&
      (want === 0 ||
        Number((document.getElementById("statParas")?.textContent ?? "").replace(/[^0-9]/g, "")) >=
          want),
    paragraphs,
    { timeout: 180_000 },
  );
  return settle(page);
}

/**
 * `MAIN_THREAD_PROFILE=1`: a sampled CPU profile of one more keystroke, printed
 * as the JavaScript frames with the most inclusive time — which host code the
 * keystroke's time is spent under (`docs/180` §2.3). wasm frames are anonymous
 * in a release build, so they are attributed to the JS binding that called them.
 */
async function profileKeystroke(page, name) {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Profiler.enable");
  await cdp.send("Profiler.setSamplingInterval", { interval: 200 });
  await cdp.send("Profiler.start");
  await page.evaluate(async () => {
    document.activeElement.dispatchEvent(
      new InputEvent("beforeinput", { inputType: "insertText", data: "q", bubbles: true, cancelable: true }),
    );
    await new Promise((resolve) => setTimeout(resolve, 1200));
  });
  const { profile } = await cdp.send("Profiler.stop");
  await cdp.detach();
  const byId = new Map(profile.nodes.map((n) => [n.id, n]));
  const parent = new Map();
  for (const n of profile.nodes) for (const child of n.children ?? []) parent.set(child, n.id);
  const label = (n) =>
    `${n.callFrame.functionName || "(anonymous)"} ${n.callFrame.url.split("/").pop()}:${n.callFrame.lineNumber + 1}`;
  const inclusive = new Map();
  profile.samples.forEach((id, i) => {
    const seen = new Set();
    for (let cur = id; cur !== undefined; cur = parent.get(cur)) {
      const key = label(byId.get(cur));
      if (seen.has(key)) continue;
      seen.add(key);
      inclusive.set(key, (inclusive.get(key) ?? 0) + (profile.timeDeltas[i] ?? 0) / 1000);
    }
  });
  const top = [...inclusive]
    .filter(([key]) => /\.m?js:/.test(key))
    .sort((a, b) => b[1] - a[1])
    .slice(0, 15)
    .map(([key, ms]) => `  ${Math.round(ms)} ms  ${key}`);
  console.log(`one keystroke in ${name}, inclusive JS frames:\n${top.join("\n")}`);
}

function row(label, m) {
  const line = rowText(label, m);
  console.log(line); // as it is measured, so a later step's failure still leaves the earlier rows
  return line;
}

function rowText(label, m) {
  const over = m.tasks.filter((t) => t > 50);
  return `| ${label} | ${m.worst} ms | ${m.tti} ms | ${over.length} | [${over.slice(0, 12).join(", ")}${over.length > 12 ? ", …" : ""}] |`;
}

test.describe("main-thread blocking, measured (docs/180 §2)", () => {
  test.setTimeout(600_000);

  test("open, paste and style change on sample.docx and the large fixture", async ({ page }) => {
    const scratch = mkdtempSync(join(tmpdir(), "opendoc-mt-"));
    const lines = ["| operation | longest task | time to interactive | tasks > 50 ms | those tasks |", "| --- | ---: | ---: | ---: | --- |"];
    try {
      const fixtures = [{ name: "sample.docx", path: join(process.cwd(), "..", "sample.docx"), paragraphs: 0 }];
      for (const n of SIZES) {
        const path = join(scratch, `large-${n}.docx`);
        writeFileSync(path, largeDocx(n));
        fixtures.push({ name: `large-${n}.docx`, path, paragraphs: n });
      }
      for (const fixture of fixtures) {
        // A fresh editor per fixture: the previous one is edited, and opening over
        // it would put the discard prompt inside the measurement.
        await gotoEditor(page);
        lines.push(row(`open ${fixture.name}`, await openFile(page, fixture.path, fixture.paragraphs)));

        // The first keystroke after the open, then a second one. Through
        // `beforeinput`, the event the editor's text input path consumes, so the
        // measurement is of the edit and not of Playwright's typing. The first
        // is measured apart because it is the one that pays for anything the
        // open left undone (`docs/180` §2.2).
        await page.locator(".page-wrap .page").first().click({ position: { x: 120, y: 140 } });
        for (const [ordinal, key] of [["first", "x"], ["second", "y"]]) {
          await observe(page);
          await page.evaluate((data) => {
            document.activeElement.dispatchEvent(
              new InputEvent("beforeinput", { inputType: "insertText", data, bubbles: true, cancelable: true }),
            );
          }, key);
          lines.push(row(`${ordinal} keystroke in ${fixture.name}`, await settle(page)));
        }
        if (process.env.MAIN_THREAD_PROFILE) await profileKeystroke(page, fixture.name);
        const paras = await statParas(page);

        // A large paste: 2,000 lines of plain text at the caret, through the
        // editor's own paste handler (the same synthesis the paste suites use).
        await page.locator(".page-wrap .page").first().click({ position: { x: 120, y: 140 } });
        await observe(page);
        await page.evaluate(() => {
          const dt = new DataTransfer();
          dt.setData("text/plain", Array.from({ length: 2000 }, (_, i) => `Pasted line ${i} of the measurement.`).join("\n"));
          document.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }));
        });
        await expect.poll(() => statParas(page), { timeout: 120_000 }).toBeGreaterThan(paras);
        lines.push(row(`paste 2,000 lines into ${fixture.name}`, await settle(page)));

        // A style change: Heading 1 onto the caret's paragraph, then Normal back.
        await page.locator(".page-wrap .page").first().click({ position: { x: 120, y: 140 } });
        await observe(page);
        // Whichever offered style is not the caret's: `sample.docx` and the blank
        // package do not name their headings alike.
        const current = await reflectedParagraphStyle(page);
        const target = (await offeredParagraphStyles(page)).find((s) => s !== current);
        await observe(page);
        await applyParagraphStyle(page, target);
        lines.push(row(`apply style "${target}" in ${fixture.name}`, await settle(page)));

        // A whole-document re-layout: select all, then bold.
        await observe(page);
        await page.keyboard.press("ControlOrMeta+a");
        await page.keyboard.press("ControlOrMeta+b");
        lines.push(row(`select all + bold in ${fixture.name}`, await settle(page)));
      }
    } finally {
      rmSync(scratch, { recursive: true, force: true });
    }
    console.log(`\n${lines.join("\n")}\n`);
  });
});
