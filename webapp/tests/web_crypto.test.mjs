// The insecure-context fallbacks (HF-414): a plain-HTTP page has no
// `crypto.subtle` and no `crypto.randomUUID`, and the editor must still verify
// its downloads and mint ids there.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash, getRandomValues } from "node:crypto";
import { randomUuid, sha256Bytes, sha256Hex } from "../src/web_crypto.mjs";

const reference = (bytes) => createHash("sha256").update(bytes).digest("hex");
const hex = (bytes) => Buffer.from(bytes).toString("hex");

test("the JavaScript SHA-256 matches FIPS 180-4's vectors", () => {
  assert.equal(hex(sha256Bytes(new Uint8Array())), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
  assert.equal(hex(sha256Bytes(new TextEncoder().encode("abc"))), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
  assert.equal(
    hex(sha256Bytes(new TextEncoder().encode("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"))),
    "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
  );
});

test("the JavaScript SHA-256 agrees with node's at every padding boundary and on a model-sized input", () => {
  for (const length of [1, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 1000, 4097]) {
    const bytes = getRandomValues(new Uint8Array(length));
    assert.equal(hex(sha256Bytes(bytes)), reference(bytes), `length ${length}`);
  }
  const large = new Uint8Array(3_000_000);
  for (let i = 0; i < large.length; i += 65536) getRandomValues(large.subarray(i, i + 65536));
  assert.equal(hex(sha256Bytes(large)), reference(large));
  // An ArrayBuffer is accepted as the bytes it holds.
  assert.equal(hex(sha256Bytes(large.buffer)), reference(large));
});

test("sha256Hex uses Web Crypto when the page has it and the JavaScript digest when it does not", async () => {
  const bytes = new TextEncoder().encode("the recognition models");
  let asked = 0;
  const subtle = {
    digest: async (algorithm, data) => {
      asked += 1;
      assert.equal(algorithm, "SHA-256");
      return createHash("sha256").update(data).digest().buffer;
    },
  };
  assert.equal(await sha256Hex(bytes, subtle), reference(bytes));
  assert.equal(asked, 1);
  // A plain-HTTP page: `crypto.subtle` is undefined. Before this fallback the OCR
  // models threw "this browser cannot verify downloads" here.
  assert.equal(await sha256Hex(bytes, undefined), reference(bytes));
  assert.equal(await sha256Hex(bytes, null), reference(bytes));
});

test("randomUuid mints a version-4 UUID without crypto.randomUUID", () => {
  const insecure = { getRandomValues: (array) => getRandomValues(array) };
  const seen = new Set();
  for (let i = 0; i < 200; i += 1) {
    const id = randomUuid(insecure);
    assert.match(id, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    seen.add(id);
  }
  assert.equal(seen.size, 200);
  assert.equal(randomUuid({ randomUUID: () => "from-the-browser" }), "from-the-browser");
  assert.match(randomUuid({}), /^[0-9a-f]{8}-[0-9a-f]{4}-4/);
});
