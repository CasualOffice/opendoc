import assert from "node:assert/strict";
import test from "node:test";

import {
  NAMED_WEB_FONT_FACES,
  SCRIPT_FALLBACK_FONTS,
  fallbackKeysFor,
  fetchFontBytes,
  packFontBytes,
} from "../src/web_fonts.mjs";

test("Roboto and Noto families are external, immutable OpenType assets", () => {
  assert.deepEqual(
    [...new Set(NAMED_WEB_FONT_FACES.map((face) => face.family))],
    ["Roboto", "Noto Sans", "Noto Serif"],
  );
  assert.equal(NAMED_WEB_FONT_FACES.length, 6);
  for (const face of NAMED_WEB_FONT_FACES) {
    assert.match(
      face.url,
      /^https:\/\/cdn\.jsdelivr\.net\/gh\/google\/fonts@[0-9a-f]{40}\//,
    );
    assert.doesNotMatch(face.url, /@main|@latest/);
    assert.match(face.url, /\.ttf$/);
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
});

test("every script fallback font resolves to a pinned, immutable Noto URL", () => {
  for (const [key, font] of Object.entries(SCRIPT_FALLBACK_FONTS)) {
    assert.ok(font.scripts.length > 0, `${key} declares no scripts`);
    assert.match(font.url, /^https:\/\/cdn\.jsdelivr\.net\/gh\/notofonts\//);
    assert.match(font.url, /@[0-9a-f]{40}\//);
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
