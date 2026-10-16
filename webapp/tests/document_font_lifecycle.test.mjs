// A registered font belongs to ONE engine instance, and an answer that arrives
// after its document was replaced touches nothing. `109` HF-292.
//
// Every open builds a new engine (`open(bytes)`), and a new engine holds none of
// the faces the last one was given. The editor recorded "Korean is registered"
// in session-wide Sets, so the second document to need Korean was told it
// already had it and painted every Hangul syllable as ▯ — reproduced on the
// deployed build with a real government form, where re-registering on the SAME
// second engine restored the first open's page byte for byte.
//
// The host below is `main.js`'s font path, statement for statement: `openBytes`
// retires the old engine, frees it, swaps `doc`, and starts the open pass only if
// `doc` is still the engine it opened; its completion repaints only if `doc` is
// still that engine; `ensureGlyphCoverage` acts only for the engine it started
// on. `main.js` cannot be imported in node (it binds ~360 DOM ids at import), so
// the last test holds its source to exactly this shape.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { createFontProvisioning } from "../src/font_provisioning.mjs";
import { NAMED_WEB_FONT_FACES, SCRIPT_FALLBACK_FONTS } from "../src/web_fonts.mjs";

const HANGUL = 0xd55c;
const KANA = 0x3042;
const KR = SCRIPT_FALLBACK_FONTS.kr.url;
const JP = SCRIPT_FALLBACK_FONTS.jp.url;
const isNamed = (url) => NAMED_WEB_FONT_FACES.some((face) => face.url === url);

const flush = () => new Promise((resolve) => setImmediate(resolve));

/** An engine whose every call after `free()` throws, as the wasm-bindgen wrapper
 *  does ("null pointer passed to rust"), and counts the attempt so a test can
 *  assert on it even where the throw is swallowed. */
function fakeEngine(name, missing = []) {
  let freed = false;
  const engine = {
    name,
    missing,
    namedBatches: 0,
    fallbackScripts: [],
    touchedAfterFree: 0,
  };
  const live = (fn) => (...args) => {
    if (freed) {
      engine.touchedAfterFree += 1;
      throw new Error("null pointer passed to rust");
    }
    return fn(...args);
  };
  engine.missingCoverage = live(() => engine.missing);
  engine.registerFonts = live(() => {
    engine.namedBatches += 1;
  });
  engine.registerFallbackFont = live((_bytes, scripts) => {
    engine.fallbackScripts.push([...scripts]);
  });
  engine.free = live(() => {
    freed = true;
  });
  engine.isFreed = () => freed;
  return engine;
}

/** `url → bytes`, where any URL can be held and later released or failed. */
function gatedFetcher({ hold = () => false, fail = () => false } = {}) {
  const started = [];
  let pending = [];
  return {
    started,
    hold,
    fail,
    fetchBytes(url) {
      started.push(url);
      if (this.fail(url)) return Promise.reject(new Error(`${url}: blocked`));
      if (!this.hold(url)) return Promise.resolve(new Uint8Array(4));
      return new Promise((resolve, reject) => pending.push({ url, resolve, reject }));
    },
    release(match = () => true) {
      const now = pending.filter((p) => match(p.url));
      pending = pending.filter((p) => !match(p.url));
      for (const p of now) p.resolve(new Uint8Array(4));
    },
    reject(match = () => true) {
      const now = pending.filter((p) => match(p.url));
      pending = pending.filter((p) => !match(p.url));
      for (const p of now) p.reject(new Error(`${p.url}: blocked`));
    },
  };
}

/** `main.js`'s font path over a fake engine and a fake network. */
function host(fetcher) {
  const screen = { doc: null, preview: null };
  /** `document.body.dataset`, for `fontsReady`. */
  const dataset = {};
  const painted = [];
  const statuses = [];
  const progress = [];
  const logged = [];
  const fontProvisioning = createFontProvisioning({
    engine: () => screen.doc,
    fetchBytes: (url) => fetcher.fetchBytes(url),
    releaseBytes: () => {},
    progress: (text) => progress.push({ doc: screen.doc?.name, text }),
    log: (message) => logged.push(message),
  });

  /** `openBytes`: parse (which may refuse), retire, free, swap, clear the ready
   *  flag — then the background open pass. Returns the pass's completion so a
   *  test can await it. */
  function openBytes(name, missing, { unparseable = false } = {}) {
    try {
      if (unparseable) throw new Error(`${name}: not a document`);
      const next = fakeEngine(name, missing);
      fontProvisioning.retire(screen.doc);
      if (screen.doc) screen.doc.free();
      screen.doc = next;
      delete dataset.fontsReady;
      const done =
        screen.doc === next
          ? fontProvisioning.provisionOnOpen(name).then((fontLoss) => {
              if (screen.doc !== next) return "stale";
              painted.push(next);
              if (fontLoss) statuses.push({ doc: next, text: fontLoss.text });
              dataset.fontsReady = "true";
              return "painted";
            })
          : Promise.resolve("stale");
      return { engine: next, done };
    } catch (err) {
      // `openBytes`' catch: the open document is kept, and so is its state.
      statuses.push({ doc: screen.doc, text: `Could not open ${name}: ${err.message}` });
      return { engine: null, done: Promise.resolve("refused") };
    }
  }

  async function ensureGlyphCoverage(label) {
    const target = screen.doc;
    const { fetched, report } = await fontProvisioning.provisionForEdit(label);
    if (!target || screen.doc !== target) return "stale";
    if (report) statuses.push({ doc: target, text: report.text });
    if (fetched) painted.push(target);
    return fetched ? "painted" : "unchanged";
  }

  /** `showVersionPreview`: the live engine stays alive behind another. */
  function preview(on) {
    if (on) {
      screen.preview = screen.doc;
      screen.doc = fakeEngine("preview");
    } else {
      screen.doc = screen.preview;
      screen.preview = null;
    }
  }

  return { screen, dataset, painted, statuses, progress, logged, openBytes, ensureGlyphCoverage, preview };
}

const scriptsOf = (key) => [...SCRIPT_FALLBACK_FONTS[key].scripts];

test("A then B: the second document to need Korean is given Korean", async () => {
  // THE REGRESSION. With the session-wide record, B's coverage check filtered
  // `kr` out as "already provisioned" and B never received the face.
  const h = host(gatedFetcher());
  const a = h.openBytes("a.docx", [HANGUL]);
  assert.equal(await a.done, "painted");
  assert.deepEqual(a.engine.fallbackScripts, [scriptsOf("kr")]);

  const b = h.openBytes("b.docx", [HANGUL]);
  assert.equal(await b.done, "painted");
  assert.deepEqual(b.engine.fallbackScripts, [scriptsOf("kr")], "B's own engine must receive Korean");
  assert.equal(b.engine.namedBatches, 1, "B's own engine must receive the named faces");
  assert.equal(a.engine.touchedAfterFree, 0);
  assert.deepEqual(h.statuses, []);
});

test("reopening the same file is a new engine under the same name, and is provisioned again", async () => {
  const h = host(gatedFetcher());
  const first = h.openBytes("form.docx", [HANGUL]);
  await first.done;
  const second = h.openBytes("form.docx", [HANGUL]);
  assert.equal(await second.done, "painted");
  assert.deepEqual(second.engine.fallbackScripts, [scriptsOf("kr")]);
  assert.equal(first.engine.touchedAfterFree, 0);
});

test("switching documents while the NAMED faces download: the old engine is never touched", async () => {
  const fetcher = gatedFetcher({ hold: isNamed });
  const h = host(fetcher);
  const a = h.openBytes("a.docx", [HANGUL]);
  await flush();
  const b = h.openBytes("b.docx", [HANGUL]);
  await flush();
  fetcher.release();
  assert.equal(await a.done, "stale", "A's completion must not repaint B's screen");
  assert.equal(await b.done, "painted");

  assert.equal(a.engine.touchedAfterFree, 0, "a late answer reached a freed engine");
  assert.equal(a.engine.namedBatches, 0);
  assert.equal(b.engine.namedBatches, 1, "B gets exactly one named batch, not A's as well");
  assert.deepEqual(b.engine.fallbackScripts, [scriptsOf("kr")]);
  // One download per face, shared by both engines.
  for (const face of NAMED_WEB_FONT_FACES) {
    assert.equal(fetcher.started.filter((url) => url === face.url).length, 1, face.url);
  }
  assert.deepEqual(h.painted, [b.engine]);
});

test("switching documents while a FALLBACK downloads: one download, registered only into the new engine", async () => {
  const fetcher = gatedFetcher({ hold: (url) => url === KR });
  const h = host(fetcher);
  const a = h.openBytes("a.docx", [HANGUL]);
  await flush();
  assert.equal(a.engine.namedBatches, 1, "the named faces landed; Korean is still arriving");
  const b = h.openBytes("b.docx", [HANGUL]);
  await flush();
  fetcher.release();
  assert.equal(await a.done, "stale");
  assert.equal(await b.done, "painted");

  assert.equal(a.engine.touchedAfterFree, 0, "Korean was registered into a freed engine");
  assert.deepEqual(a.engine.fallbackScripts, []);
  assert.deepEqual(b.engine.fallbackScripts, [scriptsOf("kr")], "B must not be told Korean is still in flight for ever");
  assert.equal(fetcher.started.filter((url) => url === KR).length, 1, "the 16 MB face was downloaded twice");
});

test("a same-name reopen DURING a download is told apart from the original", async () => {
  // The completion used to compare `currentName !== name`, which cannot tell
  // these two apart: A's late completion would have repainted B's screen.
  const fetcher = gatedFetcher({ hold: () => true });
  const h = host(fetcher);
  const a = h.openBytes("form.docx", [HANGUL]);
  await flush();
  const b = h.openBytes("form.docx", [HANGUL]);
  await flush();
  fetcher.release();
  await flush();
  fetcher.release(); // the fallback wave starts after the named one lands
  assert.equal(await a.done, "stale");
  assert.equal(await b.done, "painted");
  assert.equal(a.engine.touchedAfterFree, 0);
  assert.deepEqual(b.engine.fallbackScripts, [scriptsOf("kr")]);
  assert.deepEqual(h.painted, [b.engine]);
});

test("progress is painted only for the document on screen", async () => {
  const fetcher = gatedFetcher({ hold: () => true });
  const h = host(fetcher);
  h.openBytes("a.docx", [HANGUL]);
  await flush();
  const b = h.openBytes("b.docx", []);
  await flush();
  const before = h.progress.length;
  fetcher.release();
  await b.done;
  await flush();
  const after = h.progress.slice(before);
  // The final line of B's own pass is the clear; a stale pass from A must not
  // follow it with one of its own (clearing B's line, or naming A).
  assert.deepEqual(after.at(-1), { doc: "b.docx", text: "" });
  assert.equal(after.filter((line) => line.text === "").length, 1, "A's stale pass cleared B's line");
  assert.ok(after.every((line) => !line.text.includes("a.docx")), "A's stale pass painted over B");
});

test("overlapping coverage checks on one document share one download and one registration", async () => {
  const fetcher = gatedFetcher();
  const h = host(fetcher);
  const a = h.openBytes("a.docx", []);
  await a.done;
  fetcher.hold = (url) => url === JP;
  a.engine.missing = [KANA];
  const first = h.ensureGlyphCoverage("edit 1");
  const second = h.ensureGlyphCoverage("edit 2");
  await flush();
  fetcher.release();
  assert.deepEqual(await Promise.all([first, second]), ["painted", "unchanged"]);
  assert.equal(fetcher.started.filter((url) => url === JP).length, 1);
  assert.deepEqual(a.engine.fallbackScripts, [scriptsOf("jp")]);
});

test("a coverage check that outlives its document neither registers, reports nor repaints", async () => {
  const fetcher = gatedFetcher();
  const h = host(fetcher);
  const a = h.openBytes("a.docx", []);
  await a.done;
  fetcher.hold = (url) => url === JP;
  a.engine.missing = [KANA];
  const edit = h.ensureGlyphCoverage("typing");
  await flush();
  const b = h.openBytes("b.docx", [KANA]);
  await flush();
  fetcher.release();
  assert.equal(await edit, "stale");
  assert.equal(await b.done, "painted");
  assert.equal(a.engine.touchedAfterFree, 0);
  assert.deepEqual(a.engine.fallbackScripts, []);
  assert.deepEqual(b.engine.fallbackScripts, [scriptsOf("jp")]);
  assert.equal(fetcher.started.filter((url) => url === JP).length, 1);
  assert.deepEqual(h.painted, [a.engine, b.engine], "only each document's own open repainted");
});

test("a failure is retried for the next document, and a stale failure is never reported", async () => {
  const fetcher = gatedFetcher({ hold: (url) => url === KR });
  const h = host(fetcher);
  const a = h.openBytes("a.docx", [HANGUL]);
  await flush();
  const b = h.openBytes("b.docx", [HANGUL]);
  await flush();
  fetcher.reject();
  assert.equal(await a.done, "stale");
  assert.equal(await b.done, "painted");
  // B's own loss is reported once, against B; A's is not reported at all.
  assert.equal(h.statuses.length, 1);
  assert.equal(h.statuses[0].doc, b.engine);
  assert.match(h.statuses[0].text, /Korean font/);

  // The network comes back: the next edit retries, on B, and repaints B.
  fetcher.hold = () => false;
  assert.equal(await h.ensureGlyphCoverage("typing"), "painted");
  assert.deepEqual(b.engine.fallbackScripts, [scriptsOf("kr")]);
  // And a third document is not told B's success is its own.
  const c = h.openBytes("c.docx", [HANGUL]);
  assert.equal(await c.done, "painted");
  assert.deepEqual(c.engine.fallbackScripts, [scriptsOf("kr")]);
  assert.equal(a.engine.touchedAfterFree + b.engine.touchedAfterFree, 0);
});

test("a live document behind a version preview still receives its faces, and nothing repaints the preview", async () => {
  const fetcher = gatedFetcher({ hold: (url) => url === KR });
  const h = host(fetcher);
  const a = h.openBytes("a.docx", [HANGUL]);
  await flush();
  h.preview(true);
  const before = h.progress.length;
  fetcher.release();
  assert.equal(await a.done, "stale", "the open's repaint belongs to the live document, not the preview");
  assert.deepEqual(a.engine.fallbackScripts, [scriptsOf("kr")], "a preview is not a retirement");
  assert.deepEqual(h.progress.slice(before), [], "no progress painted over the preview");
  h.preview(false);
  // Back on the live document: it already holds Korean, so nothing re-downloads.
  assert.equal(await h.ensureGlyphCoverage("typing"), "unchanged");
  assert.equal(fetcher.started.filter((url) => url === KR).length, 1);
});

test("fontsReady belongs to the document on screen: cleared when a new one is adopted", async () => {
  // Every e2e spec waits on `fontsReady === "true"` before measuring. Left set
  // from the previous document, a wait after opening B returned at once and
  // measured B on its pre-upgrade paint.
  const fetcher = gatedFetcher();
  const h = host(fetcher);
  assert.equal(h.dataset.fontsReady, undefined, "a first load starts not ready");
  const a = h.openBytes("a.docx", [HANGUL]);
  await a.done;
  assert.equal(h.dataset.fontsReady, "true");

  fetcher.hold = () => true;
  const b = h.openBytes("b.docx", [HANGUL]);
  assert.notEqual(h.dataset.fontsReady, "true", "B was reported ready on A's upgrade");
  await flush();
  fetcher.release();
  await flush();
  fetcher.release();
  assert.equal(await b.done, "painted");
  assert.equal(h.dataset.fontsReady, "true", "B's own upgrade marks it ready");
});

test("a same-name reopen is not ready until its own upgrade, whatever its predecessor did", async () => {
  const fetcher = gatedFetcher({ hold: () => true });
  const h = host(fetcher);
  const first = h.openBytes("form.docx", [HANGUL]);
  await flush();
  const second = h.openBytes("form.docx", [HANGUL]);
  await flush();
  // The named faces land for both (downloads are shared); the first engine's
  // pass then completes, stale, while Korean — prefetched with them — is held.
  fetcher.release(isNamed);
  assert.equal(await first.done, "stale");
  await flush();
  assert.notEqual(h.dataset.fontsReady, "true", "the predecessor's completion marked the reopen ready");
  fetcher.release();
  assert.equal(await second.done, "painted");
  assert.equal(h.dataset.fontsReady, "true");
});

test("a failed open keeps the open document AND its ready flag", async () => {
  const h = host(gatedFetcher());
  const a = h.openBytes("a.docx", [HANGUL]);
  await a.done;
  assert.equal(h.dataset.fontsReady, "true");
  const refused = h.openBytes("broken.docx", [], { unparseable: true });
  assert.equal(await refused.done, "refused");
  assert.equal(h.screen.doc, a.engine, "a refused parse must not replace the open document");
  assert.equal(h.dataset.fontsReady, "true", "the document still on screen is still upgraded");
  assert.equal(a.engine.isFreed(), false);
});

// ---------------------------------------------------------------------------
// The host is the host above
// ---------------------------------------------------------------------------

test("main.js wires the per-engine programme, and nothing session-wide", () => {
  const source = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  // One mechanism: the programme, not a second copy in main.js.
  assert.match(source, /import \{ createFontProvisioning \} from "\.\/font_provisioning\.mjs";/);
  assert.doesNotMatch(source, /provisionedFallbackKeys|inFlightFallbackKeys/);
  assert.doesNotMatch(source, /registerFallbackFont|registerFonts\(/, "fonts are registered only by the programme");
  // Retired before freed, in the open path, so no answer lands in a freed wrapper.
  assert.match(source, /\n[ \t]*fontProvisioning\.retire\(doc\);[^\n]*\n[ \t]*if \(doc\) doc\.free\(\);\n[ \t]*doc = next;/);
  // The ready flag is cleared at adoption — after the parse that may refuse,
  // so a failed open keeps it — and set only by the adopted engine's completion.
  const openBody = source.slice(
    source.indexOf("async function openBytes("),
    source.indexOf("// ---- Document rename"),
  );
  const parsed = openBody.indexOf("const next = open(bytes);");
  const adopted = openBody.search(/\n[ \t]*doc = next;\n(?:[ \t]*\/\/[^\n]*\n)*[ \t]*delete document\.body\.dataset\.fontsReady;\n/);
  assert.ok(parsed >= 0 && adopted > parsed, "fontsReady must be cleared when, and only when, a parsed document is adopted");
  assert.equal(source.match(/document\.body\.dataset\.fontsReady = "true"/g)?.length, 1);
  assert.match(openBody, /if \(doc !== next\) return;\n[ \t]*buildOutline\(\);[\s\S]*?document\.body\.dataset\.fontsReady = "true";/);
  // The open pass starts and finishes against the engine it opened, by identity.
  assert.match(source, /if \(doc === next\) void fontProvisioning\.provisionOnOpen\(name\)\.then\(async \(fontLoss\) => \{\n\s*if \(doc !== next\) return;/);
  assert.doesNotMatch(source, /currentName !== name/, "a name cannot tell a same-name reopen apart");
  // The edit pass acts only for the engine it started on.
  assert.match(
    source,
    /async function ensureGlyphCoverage\(label\) \{\n\s*const target = doc;\n\s*const \{ fetched, report \} = await fontProvisioning\.provisionForEdit\(label\);\n\s*if \(!target \|\| doc !== target\) return;/,
  );
});
