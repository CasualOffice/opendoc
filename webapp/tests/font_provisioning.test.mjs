// What the editor tells a reader when a font cannot be had, and what it costs
// them to be told it. `109` FONT-04, and the loss half of it.
//
// Every case here needed a browser, a route interception and in two of them a
// 14 MB request before `font_provisioning.mjs` existed, which is why the
// decision it makes was wrong for as long as it was: the editor raised a
// persistent red line naming three font families whose absence changed nothing
// about the document, over whatever the reader had just been told.
import assert from "node:assert/strict";
import test from "node:test";

import {
  createFontProvisioning,
  fallbackLabel,
  fontLossReport,
  joinNames,
  provisioningProgress,
} from "../src/font_provisioning.mjs";
import { NAMED_WEB_FONT_FACES, SCRIPT_FALLBACK_FONTS } from "../src/web_fonts.mjs";

// ---------------------------------------------------------------------------
// The loss decision
// ---------------------------------------------------------------------------

test("a failed face that left coverage complete is not reported as a loss", () => {
  // THE REGRESSION. Measured in Chromium on 2026-10-06: abort the six named
  // faces on the default editor page and the reader got
  //   "Opened sample.docx; unavailable web fonts: Roboto, Noto Sans, Noto Serif"
  // in red, while the page count, the accessibility text and `caretRect` at
  // eight probe points were identical to the unblocked load. Those faces are
  // registered BY FAMILY NAME with no coverage wiring, and the fixture names
  // none of the three, so nothing could have changed.
  assert.equal(
    fontLossReport({
      failedNamedFamilies: ["Roboto", "Noto Sans", "Noto Serif"],
      uncoveredKeys: [],
    }),
    null,
  );
  // Same rule for a BUCKET that failed and is covered anyway — which is the
  // colour-emoji case exactly: the engine bundles the monochrome Noto Emoji
  // base, so a failed colour upgrade costs the reader a colour, not a glyph.
  assert.equal(fontLossReport({ failedFallbackKeys: ["emoji"], uncoveredKeys: [] }), null);
});

test("a face whose absence leaves text as ▯ is reported, named by script", () => {
  const report = fontLossReport({ failedFallbackKeys: ["jp"], uncoveredKeys: ["jp"] });
  assert.equal(report.kind, "error");
  assert.match(report.text, /Japanese font/);
  assert.match(report.text, /▯/);
  // A bucket key is a row in a manifest, not a thing a reader has.
  assert.doesNotMatch(report.text, /\bjp\b/);
});

test("the report names every uncovered script, not just the first", () => {
  const report = fontLossReport({
    failedFallbackKeys: ["jp", "kr", "emoji"],
    uncoveredKeys: ["kr", "jp", "emoji"],
  });
  assert.match(report.text, /Japanese, Korean and emoji fonts/);
});

test("uncovered scalars are reported even when no fetch failed", () => {
  // A deployment with no mirror reachable and nothing provisioned: nothing was
  // attempted this pass, and the text is still going to paint as ▯. Silence
  // here would be silent data loss (`AGENTS.md`).
  const report = fontLossReport({ uncoveredKeys: ["sc"] });
  assert.equal(report.kind, "error");
  assert.match(report.text, /Chinese font/);
  assert.match(report.text, /provisioned/);
});

test("every fallback bucket in the manifest has a sentence a reader can read", () => {
  for (const key of Object.keys(SCRIPT_FALLBACK_FONTS)) {
    const label = fallbackLabel(key);
    assert.ok(label.length > 0, `${key} has no reader-facing label`);
    const report = fontLossReport({ failedFallbackKeys: [key], uncoveredKeys: [key] });
    assert.ok(report.text.includes(label), `${key}'s refusal does not name it`);
  }
});

test("joinNames reads as a sentence at one, two and three items", () => {
  assert.equal(joinNames([]), "");
  assert.equal(joinNames(["Japanese"]), "Japanese");
  assert.equal(joinNames(["Japanese", "Korean"]), "Japanese and Korean");
  assert.equal(joinNames(["Japanese", "Korean", "emoji"]), "Japanese, Korean and emoji");
});

test("progress names the scripts it is fetching, in reader terms", () => {
  assert.equal(provisioningProgress("sample.docx"), "Fetching fonts for sample.docx…");
  assert.equal(
    provisioningProgress("sample.docx", ["jp", "kr"]),
    "Fetching fonts for sample.docx (Japanese, Korean)…",
  );
});

// ---------------------------------------------------------------------------
// The programme
// ---------------------------------------------------------------------------

/** A stand-in engine that records what was registered and in what order. */
function fakeEngine(missing = []) {
  return {
    missing,
    namedBatches: [],
    fallbacks: [],
    missingCoverage() {
      return this.missing;
    },
    registerFonts(bytes, lengths) {
      this.namedBatches.push({ bytes: bytes.length, lengths: [...lengths] });
    },
    registerFallbackFont(bytes, scripts) {
      this.fallbacks.push({ bytes: bytes.length, scripts: [...scripts] });
    },
  };
}

/** `url → Uint8Array`, with a deferred mode so overlap can be observed. */
function fakeFetcher({ fail = new Set(), hold = false } = {}) {
  const started = [];
  let resolvers = [];
  let held = hold;
  const fetchBytes = (url) => {
    started.push(url);
    if (fail.has(url)) return Promise.reject(new Error(`${url}: blocked`));
    const bytes = new Uint8Array(started.length);
    if (!held) return Promise.resolve(bytes);
    return new Promise((resolve) => resolvers.push(() => resolve(bytes)));
  };
  return {
    fetchBytes,
    started,
    /** Opens the latch: everything already waiting resolves, and so does
     *  anything asked for afterwards. A release that only drained the current
     *  queue deadlocked on the second wave. */
    release() {
      held = false;
      const pending = resolvers;
      resolvers = [];
      for (const r of pending) r();
    },
  };
}

function harness(engine, fetcher) {
  const released = [];
  const progressLines = [];
  const logged = [];
  // A BOX, not the value: every engine call in the module happens after an await
  // on the network, and the host can replace the document while those bytes are
  // in flight. A harness that could not swap the engine mid-flight could not
  // express the one failure that crashes the tab (see
  // `a document replaced mid-fetch…` below).
  const held = { engine };
  return {
    released,
    progressLines,
    logged,
    /** Stands in for `main.js` opening a second document: the old
     *  `WasmDocument` is freed and a new one takes its place. */
    replaceEngine(next) {
      held.engine = next;
    },
    provisioning: createFontProvisioning({
      engine: () => held.engine,
      fetchBytes: fetcher.fetchBytes,
      releaseBytes: (url) => released.push(url),
      progress: (text) => progressLines.push(text),
      log: (message) => logged.push(message),
    }),
  };
}

/** Frees `engine` the way `main.js` frees a `WasmDocument` it is replacing:
 *  every method then throws wasm-bindgen's own error, so a pass that still
 *  touches it fails here, by name, instead of in a browser six specs later.
 *
 *  Modelled this way round on purpose. After a second document opens, `engine()`
 *  answers with the NEW document, which is alive — the handle that is dead is
 *  the one the in-flight pass captured before its await. A fake that made
 *  `engine()` itself throw would be testing something that cannot happen. */
function freeEngine(engine) {
  const die = () => {
    throw new Error("null pointer passed to rust");
  };
  engine.missingCoverage = die;
  engine.registerFonts = die;
  engine.registerFallbackFont = die;
  return engine;
}

test("the named faces go in as ONE batch and one repagination", async () => {
  const engine = fakeEngine();
  const fetcher = fakeFetcher();
  const h = harness(engine, fetcher);
  const loss = await h.provisioning.provisionOnOpen("doc.docx");
  assert.equal(loss, null);
  assert.equal(engine.namedBatches.length, 1);
  assert.equal(engine.namedBatches[0].lengths.length, NAMED_WEB_FONT_FACES.length);
  // The JS copy is released once WASM holds it; ~9 MB double-held for a tab's
  // life is the defect this prevents.
  for (const face of NAMED_WEB_FONT_FACES) assert.ok(h.released.includes(face.url));
});

test("the coverage fetches run CONCURRENTLY and register in manifest order", async () => {
  // Three buckets for the default editor page's own scalars: Japanese
  // (14.27 MB), Korean (14.24 MB) and colour emoji (2.88 MB). Serially that is
  // three round trips end to end for downloads that do not depend on each
  // other. Only the REGISTRATION has to be ordered, because each call
  // repaginates and the fallback chain must not depend on which request
  // happened to answer first — the order is the document's own scalar order,
  // which is the same on every load of the same document.
  const engine = fakeEngine([0x3042, 0xd55c, 0x1f680]); // kana, Hangul, emoji
  const fetcher = fakeFetcher({ hold: true });
  const h = harness(engine, fetcher);
  // The edit pass, not the open pass, so the named batch is not in the way and
  // what is observed is only the coverage fetches.
  const done = h.provisioning.provisionForEdit("this edit");
  // Nothing has resolved yet. With a serial loop only the FIRST bucket would
  // have been asked for by now.
  await new Promise((r) => setTimeout(r, 0));
  const askedFor = [...fetcher.started];
  fetcher.release();
  await done;
  assert.deepEqual(
    askedFor,
    [SCRIPT_FALLBACK_FONTS.jp.url, SCRIPT_FALLBACK_FONTS.kr.url, SCRIPT_FALLBACK_FONTS.emoji.url],
    "every needed bucket must be in flight together, not one after the last",
  );
  assert.deepEqual(
    engine.fallbacks.map((f) => f.scripts),
    [
      [...SCRIPT_FALLBACK_FONTS.jp.scripts],
      [...SCRIPT_FALLBACK_FONTS.kr.scripts],
      [...SCRIPT_FALLBACK_FONTS.emoji.scripts],
    ],
  );
});

test("a successful pass reports nothing, though the engine still lists the scalars", async () => {
  // `missingCoverage()` is a CUMULATIVE set: `FontRegistry::note_missing`
  // inserts and nothing clears it, so a registered face does not take its
  // scalars back out. The fake engine keeps reporting them for exactly that
  // reason. Reading that set as a current state made a wholly successful load
  // say "Could not load the Arabic, Bengali, Devanagari, Japanese… fonts" in
  // red — the same overstated-loss defect in a new coat.
  const engine = fakeEngine([0x3042, 0xd55c, 0x1f680, 0x0915]);
  const h = harness(engine, fakeFetcher());
  assert.equal(await h.provisioning.provisionOnOpen("doc.docx"), null);
  assert.equal(engine.fallbacks.length, 4, "every needed bucket must have registered");
  assert.deepEqual(engine.missingCoverage(), [0x3042, 0xd55c, 0x1f680, 0x0915]);
});

test("a bucket already provisioned is never fetched again", async () => {
  const engine = fakeEngine([0x3042]);
  const fetcher = fakeFetcher();
  const h = harness(engine, fetcher);
  await h.provisioning.provisionOnOpen("doc.docx");
  const first = fetcher.started.length;
  const again = await h.provisioning.provisionForEdit("this edit");
  assert.equal(again.fetched, false);
  assert.equal(fetcher.started.length, first, "a second coverage check re-downloaded the face");
});

test("two overlapping coverage checks share one download", async () => {
  // Coverage is checked after every edit, so typing three emoji fires three
  // checks. Without the in-flight set each would see nothing provisioned and
  // start its own copy of the same multi-megabyte face.
  const engine = fakeEngine([0x1f680]);
  const fetcher = fakeFetcher({ hold: true });
  const h = harness(engine, fetcher);
  const a = h.provisioning.provisionForEdit("edit a");
  const b = h.provisioning.provisionForEdit("edit b");
  await new Promise((r) => setTimeout(r, 0));
  fetcher.release();
  await Promise.all([a, b]);
  assert.equal(fetcher.started.length, 1);
});

test("a failed bucket is retried by the next edit, and the failure is logged", async () => {
  const engine = fakeEngine([0x1f680]);
  const failing = fakeFetcher({ fail: new Set([SCRIPT_FALLBACK_FONTS.emoji.url]) });
  const h = harness(engine, failing);
  const first = await h.provisioning.provisionForEdit("edit a");
  assert.equal(first.fetched, false);
  assert.equal(h.logged.length, 1, "a failed face must reach the console whatever the reader is told");
  // Still uncovered, so this one IS a loss the reader suffers.
  engine.missing = [0x1f680];
  assert.match((await h.provisioning.provisionForEdit("edit b")).report.text, /emoji/);
  assert.equal(failing.started.length, 2, "a network error must not poison the bucket for the session");
});

test("provisioning with no document open does nothing and says nothing", async () => {
  const provisioning = createFontProvisioning({
    engine: () => null,
    fetchBytes: () => assert.fail("no document, so nothing may be fetched"),
    releaseBytes: () => {},
    progress: () => assert.fail("no document, so nothing may be announced"),
    log: () => {},
  });
  assert.equal(await provisioning.provisionOnOpen("doc.docx"), null);
  assert.deepEqual(await provisioning.provisionForEdit("edit"), { fetched: false, report: null });
});

test("progress retires its own line when the pass is done", async () => {
  const engine = fakeEngine();
  const h = harness(engine, fakeFetcher());
  await h.provisioning.provisionOnOpen("doc.docx");
  assert.equal(
    h.progressLines.at(-1),
    "",
    "a finished background pass must clear the line it took, or it reads as still working",
  );
});

// ---------------------------------------------------------------------------
// The document can be replaced while the bytes are in flight
// ---------------------------------------------------------------------------

test("a document replaced mid-fetch is abandoned, not written to after it was freed", async () => {
  // THE CRASH. `main.js` FREES the old `WasmDocument` when it opens the next one
  // — drop a second .docx onto the viewport, or pick one from the file dialog
  // while the first is still fetching its faces — so an engine handle captured
  // before the network await is a freed pointer, and `registerFonts` on it
  // throws `null pointer passed to rust`.
  //
  // Measured: `caret-alignment.spec.mjs`, which opens a second document while
  // the first is provisioning, failed 5 of 5 with that error against 5 of 5
  // passing on `origin/main` — where provisioning read the module-global `doc`
  // fresh at each use and so could only ever be wrong about WHICH document,
  // never about whether it still existed.
  const first = fakeEngine([0x3042]); // kana: one fallback bucket to fetch
  const fetcher = fakeFetcher({ hold: true });
  const h = harness(first, fetcher);

  const pass = h.provisioning.provisionOnOpen("first.docx");
  // The fetches are latched open, so this is exactly the window a second open
  // lands in: a NEW live document takes over, and the one this pass captured is
  // freed. Every method on the freed one now throws, so touching it at all fails
  // this test by name.
  h.replaceEngine(fakeEngine());
  freeEngine(first);
  fetcher.release();
  const loss = await pass;

  assert.equal(
    loss,
    null,
    "a superseded pass must report NO loss: the newer document runs its own, and a " +
      "refusal about a document nobody is looking at is worse than silence",
  );
  assert.deepEqual(first.namedBatches, [], "the freed document was written to");
  assert.deepEqual(first.fallbacks, [], "the freed document was written to");
});

test("an edit-time pass is abandoned the same way", async () => {
  // The other entry point, and the one that runs most often — coverage is
  // re-checked after every edit — so a document replaced during one of those is
  // the same hazard with more chances to happen.
  const first = fakeEngine([0x3042]); // kana: one fallback bucket to fetch
  const fetcher = fakeFetcher({ hold: true });
  const h = harness(first, fetcher);

  const pass = h.provisioning.provisionForEdit("this edit");
  h.replaceEngine(fakeEngine());
  freeEngine(first);
  fetcher.release();
  const { fetched, report } = await pass;

  assert.equal(fetched, false, "nothing registered, so there is nothing to re-render for");
  assert.equal(report, null);
  assert.deepEqual(first.fallbacks, []);
});

test("a document that is still the same one is written to as before", async () => {
  // The precondition, and the reason the two guards above are not satisfied by
  // simply never registering anything: the ordinary case must still work.
  const engine = fakeEngine([0x3042]); // kana: one fallback bucket to fetch
  const fetcher = fakeFetcher({ hold: true });
  const h = harness(engine, fetcher);

  const pass = h.provisioning.provisionForEdit("this edit");
  fetcher.release();
  const { fetched } = await pass;

  assert.equal(fetched, true);
  assert.equal(engine.fallbacks.length, 1, "the live document must still be registered into");
});
