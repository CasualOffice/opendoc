import assert from "node:assert/strict";
import test from "node:test";

import {
  NAMED_WEB_FONT_FACES,
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
