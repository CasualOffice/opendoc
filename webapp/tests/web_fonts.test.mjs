import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  FONT_ORIGIN,
  NAMED_WEB_FONT_FACES,
  SCRIPT_FALLBACK_FONTS,
  allManifestFaces,
  eagerBytes,
  fallbackKeysFor,
  fetchFontBytes,
  localUrlFor,
  packFontBytes,
  primaryUrlFor,
  selfHostingBytes,
} from "../src/web_fonts.mjs";

const REPO = new URL("../../", import.meta.url);
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

// ---------------------------------------------------------------------------
// Origin. `109` HF-176.
//
// Every document font used to be fetched from `cdn.jsdelivr.net`, and the test
// that used to live here *asserted* that — so the guard pinned the defect in
// place and a reviewer reading it would have concluded the CDN was intentional.
// It was: `tests/chrome_fonts.test.mjs` bans `fonts.googleapis.com` and
// `fonts.gstatic.com` for the editor chrome on local-first grounds, and the same
// reasoning applies with more force to the faces a *document's own text* needs.
// ---------------------------------------------------------------------------

test("every COMMITTED document font is served from our own origin, never a third party", () => {
  const faces = allManifestFaces().filter((face) => face.local);
  assert.ok(faces.length >= 20, `only ${faces.length} committed faces found`);
  for (const face of faces) {
    assert.equal(
      face.url,
      localUrlFor(face),
      `${face.key}: primary URL is not this origin's copy of ${face.file}`,
    );
    assert.ok(
      face.url.startsWith(FONT_ORIGIN),
      `${face.key}: ${face.url} is outside FONT_ORIGIN (${FONT_ORIGIN})`,
    );
    // The hosts the chrome guard bans, plus the CDN this row removed. A
    // document must never need any of them to draw text in these scripts.
    assert.doesNotMatch(
      face.url,
      /fonts\.(?:googleapis|gstatic)\.com|cdn\.jsdelivr\.net|unpkg\.com|cdnjs\.cloudflare\.com/,
      `${face.key}: primary URL reaches a third-party host`,
    );
  }
});

// The honest half. A face this repository does not contain is NOT asked for on
// our origin by default, because that request can only 404.
//
// The first version of this module did ask, on the reasoning that "our origin
// first, always" was the stronger property. It is not: on an unprovisioned
// checkout — which is CI, and every fresh clone — it produced one failed request
// and one console error per mirrored face. Measured against an unprovisioned
// tree it reddened `emoji-rendering.spec.mjs`, whose `consoleErrors` guard
// caught it. "Works on the machine that happens to have the fonts" is exactly
// the class of bug this lane was sent to remove, so the fix must not add it.
test("a face this origin does not have is fetched from its mirror, not 404'd against us", () => {
  assert.equal(
    globalThis.OPENDOC_PROVISIONED_FONTS,
    undefined,
    "this test describes the DEFAULT, so the provisioning flag must be unset",
  );
  const mirroredFaces = allManifestFaces().filter((face) => !face.local);
  assert.equal(mirroredFaces.length, 4);
  for (const face of mirroredFaces) {
    assert.equal(
      face.url,
      face.mirror,
      `${face.key} is not committed and this deployment has not declared it provisioned, so asking our own origin for it could only 404`,
    );
  }
});

// And the other direction: a deployment that HAS provisioned them says so, and
// then nothing leaves its own origin. This is what makes the local-first claim
// reachable rather than aspirational.
test("a deployment that declares the faces provisioned serves every one of them itself", () => {
  const previous = globalThis.OPENDOC_PROVISIONED_FONTS;
  try {
    globalThis.OPENDOC_PROVISIONED_FONTS = true;
    for (const face of allManifestFaces()) {
      assert.equal(
        primaryUrlFor(face),
        localUrlFor(face),
        `${face.key} still reaches a third party on a fully provisioned deployment`,
      );
    }
    // A filename list, not just `true`, so a partial provisioning stays honest.
    globalThis.OPENDOC_PROVISIONED_FONTS = ["Noto-COLRv1.ttf"];
    assert.equal(
      primaryUrlFor(SCRIPT_FALLBACK_FONTS.emoji),
      localUrlFor(SCRIPT_FALLBACK_FONTS.emoji),
    );
    assert.equal(
      primaryUrlFor(SCRIPT_FALLBACK_FONTS.sc),
      SCRIPT_FALLBACK_FONTS.sc.mirror,
      "a face absent from the declared list must not be asked of our origin",
    );
  } finally {
    if (previous === undefined) delete globalThis.OPENDOC_PROVISIONED_FONTS;
    else globalThis.OPENDOC_PROVISIONED_FONTS = previous;
  }
});

test("a mirrored face declares a commit-pinned upstream and a committed one declares none", () => {
  for (const face of allManifestFaces()) {
    if (face.local) {
      assert.equal(
        face.mirror,
        null,
        `${face.key} is committed, so it must not carry a mirror — a committed face \
that silently falls back to a third party is the defect wearing a disguise`,
      );
      continue;
    }
    assert.match(
      face.mirror,
      /^https:\/\/cdn\.jsdelivr\.net\/gh\/(notofonts|google\/fonts|googlefonts\/noto-emoji)/,
      `${face.key} is not mirrored from a known upstream`,
    );
    assert.match(
      face.mirror,
      /@[0-9a-f]{40}\//,
      `${face.key}'s mirror is not pinned to a commit`,
    );
    assert.doesNotMatch(face.mirror, /@main|@latest/);
  }
});

// ---------------------------------------------------------------------------
// Integrity. Pinning a URL fixes which revision is asked for; it says nothing
// about what bytes come back. Before this there was no check anywhere.
// ---------------------------------------------------------------------------

test("every face declares an exact byte count and a SHA-256", () => {
  for (const face of allManifestFaces()) {
    assert.ok(
      Number.isInteger(face.bytes) && face.bytes > 0,
      `${face.key} declares no byte count`,
    );
    assert.match(
      face.sha256,
      /^[0-9a-f]{64}$/,
      `${face.key} declares no SHA-256`,
    );
  }
});

test("every committed face is on disk and matches its declared bytes and hash", async () => {
  const committed = allManifestFaces().filter((face) => face.local);
  assert.ok(committed.length >= 14, "the committed set shrank unexpectedly");
  for (const face of committed) {
    const path = new URL(`webapp/assets/fonts/script/${face.file}`, REPO);
    const bytes = await readFile(path);
    assert.equal(
      bytes.length,
      face.bytes,
      `${face.file}: on-disk size disagrees with the manifest`,
    );
    assert.equal(
      sha256(bytes),
      face.sha256,
      `${face.file}: on-disk SHA-256 disagrees with the manifest`,
    );
    // A real OpenType/TrueType file, not an HTML error page a CDN returned.
    const tag = bytes.subarray(0, 4);
    const sfnt = tag.toString("ascii") === "OTTO" || tag.readUInt32BE(0) === 0x00010000;
    assert.ok(sfnt, `${face.file} is not an sfnt font file`);
  }
});

test("fetchFontBytes refuses bytes whose hash does not match the manifest", async () => {
  const face = SCRIPT_FALLBACK_FONTS.hebrew;
  const wrong = new Uint8Array(face.bytes).fill(0x41);
  const fakeFetch = async () => ({
    ok: true,
    arrayBuffer: async () => wrong.buffer,
  });
  await assert.rejects(
    () => fetchFontBytes(face.url, new Map(), fakeFetch),
    /SHA-256 mismatch/,
    "unverified font bytes reached the caller",
  );
});

test("fetchFontBytes refuses a truncated download before hashing it", async () => {
  const face = SCRIPT_FALLBACK_FONTS.hebrew;
  const short = new Uint8Array(16).fill(0x41);
  const fakeFetch = async () => ({
    ok: true,
    arrayBuffer: async () => short.buffer,
  });
  await assert.rejects(
    () => fetchFontBytes(face.url, new Map(), fakeFetch),
    /expected \d+ bytes, got 16/,
  );
});

test("a mirrored face is verified on the mirror path too, then falls back to us", async () => {
  const face = SCRIPT_FALLBACK_FONTS.emoji;
  const good = new Uint8Array(face.bytes);
  // Real bytes are not available offline, so stub the digest boundary instead:
  // assert the *candidate order* and that the mirror's bytes go through the same
  // verification. They fail here, which is the point — the mirror does not get
  // to skip the check just because it is the primary.
  const asked = [];
  const fakeFetch = async (url) => {
    asked.push(url);
    return { ok: true, arrayBuffer: async () => good.buffer };
  };
  await assert.rejects(
    () => fetchFontBytes(face.url, new Map(), fakeFetch),
    /SHA-256 mismatch/,
    "the mirror path skipped verification",
  );
  assert.deepEqual(
    asked,
    [face.mirror, localUrlFor(face)],
    "an unprovisioned deployment asks the mirror, then tries our origin in case the file is there after all — never the other way round, which would 404 first",
  );
});

test("a committed face does not fall back to anywhere", async () => {
  const face = SCRIPT_FALLBACK_FONTS.thai;
  const asked = [];
  const fakeFetch = async (url) => {
    asked.push(url);
    throw new Error("unreachable");
  };
  await assert.rejects(() => fetchFontBytes(face.url, new Map(), fakeFetch));
  assert.deepEqual(
    asked,
    [face.url],
    "a committed face reached for a second candidate; there is nowhere legitimate to go",
  );
});

// ---------------------------------------------------------------------------
// Repository hygiene: the mirrored faces are 51.8 MB and must not be committable
// by accident. The brief for this work named exactly this risk — "a font binary
// added by a blanket add is exactly the kind of thing nobody notices".
// ---------------------------------------------------------------------------

test("every mirrored face is named in .gitignore and no committed face is", async () => {
  const ignore = await readFile(new URL(".gitignore", REPO), "utf8");
  const lines = new Set(
    ignore
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line && !line.startsWith("#")),
  );
  for (const face of allManifestFaces()) {
    const entry = `/webapp/assets/fonts/script/${face.file}`;
    if (face.local) {
      assert.ok(
        !lines.has(entry),
        `${face.file} is committed but .gitignore excludes it`,
      );
    } else {
      assert.ok(
        lines.has(entry),
        `${face.file} is ${face.bytes} bytes and not committed, but .gitignore does \
not name it — a blanket \`git add\` would commit it`,
      );
    }
  }
});

test("the self-hosting byte totals are derived, not typed", () => {
  const derived = selfHostingBytes();
  let committed = 0;
  let mirrored = 0;
  for (const face of allManifestFaces()) {
    if (face.local) committed += face.bytes;
    else mirrored += face.bytes;
  }
  assert.equal(derived.committed, committed);
  assert.equal(derived.mirrored, mirrored);
  assert.equal(derived.total, committed + mirrored);
  // A ceiling, so growth in what every clone carries is a decision somebody
  // makes rather than a drift nobody measures. 11.66 MiB today: the 14 small
  // script faces plus the 6 named Latin variable faces.
  assert.ok(
    derived.committed < 13 * 1024 * 1024,
    `the committed font set has grown to ${derived.committed} B. That is repository \
weight on every clone for ever — raise this ceiling deliberately, with the owner's \
font-provisioning decision in hand, or mirror the new face instead`,
  );
});

// What a reader waits on, as a ratchet.
//
// The committed-bytes ceiling above answers "how heavy is a clone". It cannot
// answer the question the owner asked — "does a reader actually need 9.28 MB
// before they can work" — because a face can be committed and never fetched
// (the 14 small script faces are coverage-driven and most documents ask for
// none of them). This is the other number: the bytes EVERY reader transfers on
// EVERY editor load, whatever their document contains.
//
// Measured in Chromium on 2026-10-06, the default editor page transfers
// 74.14 MB in total. These six faces are 9.28 MB of it, the coverage-driven CJK
// and colour-emoji buckets are 31.40 MB, and the engine is 26.07 MB. So this is
// a ceiling on the one part of that which is paid unconditionally — and the
// reason a ceiling is the right instrument rather than a smaller set is in
// `NAMED_WEB_FONT_FACES`'s own comment: there is no engine seam that can tell
// whether the open document names one of these three families, so a deferral
// would have to guess, and guessing wrong renders a document in the wrong face
// and says nothing.
test("the eagerly-fetched bytes are derived, and capped", () => {
  assert.equal(
    eagerBytes(),
    NAMED_WEB_FONT_FACES.reduce((sum, face) => sum + face.bytes, 0),
  );
  // 9.28 MB today. A seventh eager face, or a bigger build of one of the six,
  // is a decision about what every reader waits for — not a diff.
  assert.ok(
    eagerBytes() <= 9_727_000,
    `the eager set has grown to ${eagerBytes()} B — that is what EVERY reader \
transfers on EVERY editor load before the editor has finished settling, whatever \
their document contains. Make it coverage- or demand-driven like \
SCRIPT_FALLBACK_FONTS, or raise this ceiling deliberately with the measurement \
that justifies it`,
  );
  // And the eager set must stay a strict subset of the committed set: a
  // mirrored eager face would cost a third-party round trip on every load.
  assert.ok(eagerBytes() < selfHostingBytes().committed);
});

// The design rule behind which faces are committed and which are mirrored, as
// an assertion rather than as a paragraph of prose that can go stale.
test("every eagerly-fetched face is committed; only coverage-driven ones may be mirrored", () => {
  for (const face of NAMED_WEB_FONT_FACES) {
    assert.equal(
      face.local,
      true,
      `${face.family}/${face.style} is fetched before the first paint on EVERY editor \
load, so mirroring it costs a 404 against our origin plus a third-party round trip \
every time, for every reader, whatever script their document is in`,
    );
  }
  const mirrored = Object.entries(SCRIPT_FALLBACK_FONTS)
    .filter(([, font]) => !font.local)
    .map(([key]) => key)
    .sort();
  // Only the four genuinely large faces. If a small one appears here, it should
  // have been committed instead.
  assert.deepEqual(mirrored, ["emoji", "jp", "kr", "sc"]);
  for (const key of mirrored) {
    assert.ok(
      SCRIPT_FALLBACK_FONTS[key].bytes > 4 * 1024 * 1024,
      `${key} is mirrored but only ${SCRIPT_FALLBACK_FONTS[key].bytes} B — commit it`,
    );
  }
});

// ---------------------------------------------------------------------------
// Coverage routing. Unchanged behaviour, kept because each case is a real
// regression that was reported.
// ---------------------------------------------------------------------------

test("Roboto and Noto families are the named, eagerly provisioned faces", () => {
  assert.deepEqual(
    [...new Set(NAMED_WEB_FONT_FACES.map((face) => face.family))],
    ["Roboto", "Noto Sans", "Noto Serif"],
  );
  assert.equal(NAMED_WEB_FONT_FACES.length, 6);
  for (const face of NAMED_WEB_FONT_FACES) {
    assert.match(face.file, /\.ttf$/);
  }
});

test("script fallback routing is coverage-driven and coalesces Han", () => {
  assert.deepEqual(fallbackKeysFor([0x0041]), []);
  assert.deepEqual(fallbackKeysFor([0x65e5, 0x3042, 0x4e2d]), ["jp"]);
  assert.deepEqual(fallbackKeysFor([0xd55c, 0x4e2d]), ["kr"]);
  assert.deepEqual(fallbackKeysFor([0x0627, 0x0915, 0x05d0, 0x0e01]), [
    "arabic",
    "devanagari",
    "hebrew",
    "thai",
  ]);
});

// Regression: only Devanagari was covered among the Indic scripts, so a
// document mixing Hindi with e.g. Bengali or Tamil (as the sample.docx
// fixture does) silently tofu'd every non-Devanagari Indic run.
test("every common Indic script has its own fallback bucket, not just Devanagari", () => {
  assert.deepEqual(
    fallbackKeysFor([
      0x0995, // Bengali KA
      0x0a15, // Gurmukhi KA
      0x0a95, // Gujarati KA
      0x0b15, // Oriya KA
      0x0b95, // Tamil UU (first letter after vowels)
      0x0c15, // Telugu KA
      0x0c95, // Kannada KA
      0x0d15, // Malayalam KA
      0x0d9a, // Sinhala KAYANNA
    ]),
    [
      "bengali",
      "gurmukhi",
      "gujarati",
      "oriya",
      "tamil",
      "telugu",
      "kannada",
      "malayalam",
      "sinhala",
    ],
  );
});

test("checkbox/dingbat symbols outside Noto Sans's coverage fall back too", () => {
  assert.deepEqual(
    fallbackKeysFor([
      0x25a1, // □ WHITE SQUARE — the sample fixture's table "Result" column
      0x2610, // ☐ BALLOT BOX — unchecked sample checklist rows
      0x2612, // ☒ BALLOT BOX WITH X — checked sample checklist rows
    ]),
    ["symbols"],
  );
  assert.deepEqual(SCRIPT_FALLBACK_FONTS.symbols.scripts, ["Zyyy", "Latn"]);
  // The demo fixture's own checklist needs this face, so it is committed: the
  // editor's first screen must not ask anyone else for its checkbox glyphs.
  assert.equal(SCRIPT_FALLBACK_FONTS.symbols.local, true);
});

// Pictographic emoji are ordinary document content — a .docx authored anywhere
// else can carry 😀 in a heading — so they must resolve to a covering face like
// any other script. Before the `emoji` bucket existed every one of these scalars
// mapped to no font and painted as a notdef box, whatever produced the document.
test("pictographic emoji use the pinned color emoji face", () => {
  assert.deepEqual(
    fallbackKeysFor([
      0x1f600, // 😀 GRINNING FACE — Emoticons
      0x1f525, // 🔥 FIRE — Misc Symbols and Pictographs
      0x1f680, // 🚀 ROCKET — Transport and Map
      0x1f9ea, // 🧪 TEST TUBE — Supplemental Symbols and Pictographs
      0x1fa79, // 🩹 ADHESIVE BANDAGE — Extended-A
      0x1f1ee, // 🇮 REGIONAL INDICATOR I — flag pairs
    ]),
    ["emoji"],
  );
  assert.deepEqual(SCRIPT_FALLBACK_FONTS.emoji.scripts, ["Zyyy", "Latn", "Zinh"]);
  // COLRv1, not CBDT: the same official Noto Color Emoji, 4.99 MB of vector
  // paint graphs (2.47 MB brotli) instead of 10.87 MB of PNG strikes (9.24 MB
  // brotli), and resolution-independent at every zoom. The engine renders
  // either, so only this file decides which one a reader downloads.
  assert.match(SCRIPT_FALLBACK_FONTS.emoji.file, /^Noto-COLRv1\.ttf$/);
});

// An emoji-presentation sequence asks for the COLOUR face, not the monochrome
// symbols face. U+2764 alone is a dingbat by Unicode default and stays with
// `symbols`; U+2764 U+FE0F is a red heart. Reading only the base scalar painted
// the heart grey and never asked for a colour face at all — which is exactly
// what was reported.
test("a variation selector or ZWJ routes a sequence to the colour emoji face", () => {
  assert.deepEqual(fallbackKeysFor([0x2764, 0xfe0f]).sort(), ["emoji", "symbols"]);
  assert.deepEqual(fallbackKeysFor([0x1f469, 0x200d, 0x1f4bb]), ["emoji"]);
  assert.deepEqual(fallbackKeysFor([0x2764]), ["symbols"]);
});

// The monochrome symbol blocks must keep resolving to Noto Sans Symbols 2 rather
// than being swallowed by the new emoji bucket: ☐/☒ checklist markers sit at
// U+2610/U+2612, immediately below the emoji ranges.
test("monochrome symbol blocks still resolve to the symbols face, not emoji", () => {
  assert.deepEqual(fallbackKeysFor([0x2610, 0x2612, 0x25a1, 0x2764]), ["symbols"]);
});

test("every script fallback bucket declares the scripts it covers", () => {
  for (const [key, font] of Object.entries(SCRIPT_FALLBACK_FONTS)) {
    assert.ok(font.scripts.length > 0, `${key} declares no scripts`);
  }
});

test("font fetches are cached and blobs pack without loss", async () => {
  const cache = new Map();
  let calls = 0;
  const fakeFetch = async () => {
    calls += 1;
    return {
      ok: true,
      arrayBuffer: async () => Uint8Array.from([1, 2, 3]).buffer,
    };
  };
  // A URL the manifest knows nothing about: there is nothing to verify against,
  // so a host passing its own face through this helper is not blocked.
  const first = await fetchFontBytes(
    "https://fonts.invalid/a.ttf",
    cache,
    fakeFetch,
  );
  const second = await fetchFontBytes(
    "https://fonts.invalid/a.ttf",
    cache,
    fakeFetch,
  );
  assert.equal(calls, 1);
  assert.equal(first, second);

  const packed = packFontBytes([first, Uint8Array.from([4, 5])]);
  assert.deepEqual(packed.lengths, [3, 2]);
  assert.deepEqual([...packed.bytes], [1, 2, 3, 4, 5]);
});
