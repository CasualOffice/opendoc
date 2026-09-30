// The SDK boundary (`docs/146` §8's "keep the public SDK surface small", ADR-042 §6).
//
// Two things are worth testing here and the rest is delegation:
//
//   1. **`checkDocument` refuses, by name, with a translatable reason.** It has no
//      honest implementation today — the scan is windowed and body-only, and
//      whole-document enumeration is an engine export that does not exist — and the
//      easiest wrong thing to do would have been to resolve it to an empty finding
//      list. That function would report a clean document by not looking at it,
//      which is the failure `docs/146` §1 forbids and the one Increment A existed to
//      remove. A guard that only checked "it does not throw" would pass on the
//      empty-list version, so what is asserted is that the result is a refusal, that
//      it names its code, and that the code has a catalogue key.
//   2. **Nothing here reaches a global.** Every provider is injected, so the whole
//      surface is drivable in node — which is also the proof that an embedder can
//      supply its own (ADR-042 §6).

import assert from "node:assert/strict";
import test from "node:test";

import { PACK_REFUSAL } from "../src/proof_packs.mjs";
import {
  PROOF_SDK_REFUSAL,
  PROOF_SDK_REFUSAL_KEYS,
  configureProofing,
} from "../src/proof_sdk.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";

const WORDS = "alpha\nbravo\n";
const BYTES = new TextEncoder().encode(WORDS);
const DIGEST = "d".repeat(64);

const MANIFEST = {
  schemaVersion: 1,
  packId: "casual-proof-en-US-test",
  locale: "en-US",
  packVersion: "1.abc",
  engineApiRange: "1",
  capabilities: ["spelling"],
  assets: [
    {
      id: "words",
      url: "packs/test/words.txt",
      bytes: BYTES.byteLength,
      sha256: DIGEST,
      format: "words-plain-1",
    },
  ],
  provenance: [{ name: "t", version: "1", license: "Apache-2.0", url: "https://x.test" }],
  selfTest: { known: ["alpha"], unknown: ["qzxbletch"] },
};

/** A store that is an in-memory object rather than a fake IndexedDB: the storage
 *  LAYER is `proof_store.test.mjs`'s subject, and what this file tests is the SDK's
 *  own behaviour over whatever storage it is handed. */
function memoryStore() {
  const staged = new Map();
  const active = new Map();
  const closed = [];
  return {
    closed,
    async installed() {
      return [...active.values()].map(({ words, ...rest }) => ({ ...rest, words: words.length }));
    },
    async words(packId) {
      return [...active.values()].find((pack) => pack.packId === packId)?.words ?? [];
    },
    async stage(record) {
      staged.set(record.packVersion, record);
    },
    async activate(record, now) {
      active.set(record.locale, { ...record, installedAt: now });
    },
    async discard() {},
    async remove(packId) {
      for (const [locale, pack] of active) {
        if (pack.packId === packId) {
          active.delete(locale);
          return true;
        }
      }
      return false;
    },
    async rollback() {
      return "";
    },
    close() {
      closed.push(true);
    },
  };
}

function session(overrides = {}) {
  const store = memoryStore();
  return {
    store,
    sdk: configureProofing({
      network: {
        fetchAsset: async () => ({ bytes: BYTES, text: WORDS }),
        estimate: async () => ({ quota: 10 ** 9, usage: 0 }),
      },
      storage: { open: async () => store },
      digest: async () => DIGEST,
      permit: () => true,
      catalogue: () => [MANIFEST],
      now: () => 7,
      ...overrides,
    }),
  };
}

test("checkDocument is a NAMED refusal, never an empty result", async () => {
  const { sdk } = session();
  const result = await sdk.checkDocument();
  assert.equal(result.ok, false, "an empty finding list here would report a clean document");
  assert.equal(result.code, PROOF_SDK_REFUSAL.wholeDocument);
  assert.equal(result.messageKey, "proofDocument.notAvailable");
  assert.equal(
    Object.hasOwn(result, "findings"),
    false,
    "and it carries no findings field at all, so a caller cannot read it as a result",
  );
});

test("every SDK refusal has a sentence, and the sentence exists in the catalogue", () => {
  // SKILL.md §10: a control that refuses has to be able to SAY something. A code
  // with no key would reach a user as nothing, and a key the catalogue does not
  // declare would reach them as the key itself.
  for (const code of Object.values(PROOF_SDK_REFUSAL)) {
    const key = PROOF_SDK_REFUSAL_KEYS[code];
    assert.ok(key, `${code} has no catalogue key`);
    assert.ok(EN_STRINGS[key], `${key} is not declared in EN_STRINGS`);
  }
});

test("checkRange answers for a scanned paragraph and REFUSES for one outside the window", async () => {
  // The honesty boundary. The coordinator knows the page window and nothing else,
  // so an empty answer for an unscanned range would be a lie shaped like a result.
  const coordinator = {
    refresh: () => {},
    scannedNodes: () => ["p1a", "p1b"],
    findings: (node) => (node === "p1b" ? [{ word: "qzxtypo" }] : []),
  };
  const { sdk } = session({ coordinator });
  const inside = await sdk.checkRange({ node: "p1b" });
  assert.equal(inside.ok, true);
  assert.equal(inside.findings.length, 1);
  const clean = await sdk.checkRange({ node: "p1a" });
  assert.equal(clean.ok, true, "a scanned paragraph with nothing wrong IS a result");
  assert.deepEqual(clean.findings, []);
  const outside = await sdk.checkRange({ node: "p9z" });
  assert.equal(outside.ok, false, "an unscanned paragraph is not a clean one");
  assert.equal(outside.code, PROOF_SDK_REFUSAL.outsideWindow);
});

test("with no coordinator, checking says so instead of answering nothing", async () => {
  const { sdk } = session({ coordinator: null });
  const result = await sdk.checkRange({ node: "p1a" });
  assert.equal(result.code, PROOF_SDK_REFUSAL.notConfigured);
});

test("installPack drives the eight steps and reports progress", async () => {
  const { sdk } = session();
  const steps = [];
  const result = await sdk.installPack({
    locale: "en-US",
    onProgress: (step) => steps.push(step),
  });
  assert.equal(result.ok, true, JSON.stringify(result));
  assert.deepEqual(await sdk.packWords("casual-proof-en-US-test"), ["alpha", "bravo"]);
  const installed = await sdk.installedPacks();
  assert.equal(installed.length, 1);
  assert.equal(installed[0].installedAt, 7, "the host's clock, injected");
  assert.ok(steps.includes("fetch") && steps.includes("activate"));
});

test("a locale nobody publishes a pack for is a named refusal", async () => {
  const { sdk } = session();
  const result = await sdk.installPack({ locale: "fr-FR" });
  assert.equal(result.code, PACK_REFUSAL.notPublished);
});

test("with no storage, installing refuses instead of appearing to work", async () => {
  const sdk = configureProofing({
    network: { fetchAsset: async () => ({ bytes: BYTES, text: WORDS }) },
    storage: {},
    digest: async () => DIGEST,
    catalogue: () => [MANIFEST],
  });
  assert.equal(await sdk.storageAvailable(), false);
  const result = await sdk.installPack({ locale: "en-US" });
  assert.equal(result.code, PACK_REFUSAL.storage);
  assert.deepEqual(await sdk.installedPacks(), []);
});

test("a store that throws on open is reported as absent, not as a crash", async () => {
  const sdk = configureProofing({
    storage: {
      open: async () => {
        throw new Error("the proofing database is blocked by another tab");
      },
    },
  });
  assert.equal(await sdk.storageAvailable(), false);
  assert.deepEqual(await sdk.installedPacks(), []);
});

test("removing a pack that is not installed refuses rather than reporting success", async () => {
  const { sdk } = session();
  const absent = await sdk.removePack("casual-proof-en-US-test");
  assert.equal(absent.ok, false);
  assert.equal(absent.code, PACK_REFUSAL.notInstalled);
  await sdk.installPack({ locale: "en-US" });
  const removed = await sdk.removePack("casual-proof-en-US-test");
  assert.equal(removed.ok, true);
  assert.deepEqual(await sdk.installedPacks(), []);
});

test("cancelInstall says whether there was anything to cancel", async () => {
  const { sdk } = session();
  assert.equal(sdk.cancelInstall("en-US"), false, "nothing in flight");
  let release = null;
  const slow = configureProofing({
    network: {
      fetchAsset: () =>
        new Promise((resolve, reject) => {
          release = reject;
        }),
    },
    storage: { open: async () => memoryStore() },
    digest: async () => DIGEST,
    catalogue: () => [MANIFEST],
  });
  const running = slow.installPack({ locale: "en-US" });
  // Let the machine reach the fetch step before cancelling it.
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(slow.cancelInstall("en-US"), true);
  release?.(new Error("aborted"));
  const result = await running;
  assert.equal(result.code, PACK_REFUSAL.cancelled);
});

test("onFindings delivers, returns its own unsubscribe, and one listener registers once", async () => {
  let notify = null;
  const coordinator = {
    subscribe: (fn) => {
      notify = fn;
      return () => {
        notify = null;
      };
    },
    refresh: () => {},
    scannedNodes: () => [],
    findings: () => [],
  };
  const { sdk } = session({ coordinator });
  const heard = [];
  const listener = (findings) => heard.push(findings);
  const off = sdk.onFindings(listener);
  sdk.onFindings(listener);
  notify([{ word: "qzxtypo" }]);
  assert.equal(heard.length, 1, "a Set, so the same listener is registered once");
  off();
  notify([{ word: "qzxtypo" }]);
  assert.equal(heard.length, 1);
  assert.equal(typeof sdk.onFindings("not a function"), "function", "and a non-function is inert");
});

test("a listener that throws does not break the coordinator's repaint", () => {
  let notify = null;
  const coordinator = {
    subscribe: (fn) => {
      notify = fn;
      return () => {};
    },
  };
  const { sdk } = session({ coordinator });
  sdk.onFindings(() => {
    throw new Error("host bug");
  });
  const heard = [];
  sdk.onFindings((findings) => heard.push(findings));
  notify([]);
  assert.equal(heard.length, 1, "the second listener still heard it");
});

test("dispose releases everything and every later call is a refusal, not a throw", async () => {
  const { sdk, store } = session();
  let unsubscribed = false;
  const coordinated = configureProofing({
    coordinator: {
      subscribe: () => () => {
        unsubscribed = true;
      },
    },
  });
  coordinated.dispose();
  assert.equal(unsubscribed, true);

  await sdk.installPack({ locale: "en-US" });
  sdk.dispose();
  assert.deepEqual(store.closed, [true], "the database handle was closed");
  for (const call of [
    () => sdk.checkDocument(),
    () => sdk.checkRange({ node: "p1a" }),
    () => sdk.installPack({ locale: "en-US" }),
    () => sdk.removePack("casual-proof-en-US-test"),
    () => sdk.rollbackPack("en-US"),
  ]) {
    const result = await call();
    assert.equal(result.ok, false);
    assert.equal(result.code, PROOF_SDK_REFUSAL.disposed, "a host that disposes then calls");
  }
});
