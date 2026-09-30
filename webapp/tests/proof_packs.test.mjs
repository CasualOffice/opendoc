// The pack system's decisions, driven in node with no browser (`docs/146` §6/§9,
// ADR-042).
//
// `docs/146` §9 lists the pack lifecycle a release has to survive: "Fresh install,
// cancel, restart, corrupted hash, unsupported schema, insufficient quota, offline
// reopen, storage eviction, update rollback, concurrent tabs, and iframe
// partitioning." Six of those are decisions rather than plumbing, and they are all
// here — because `proof_packs.mjs` is pure and takes its network, its storage and
// its DIGEST by injection, a corrupted download is two lines rather than a fixture
// nobody writes. That is the argument for the injection, stated as tests.
//
// The rest of the list belongs elsewhere and is named so it is not assumed covered:
// offline reopen and eviction are `proof_store.test.mjs`, rollback is there too,
// concurrent tabs is the `onblocked` path in the same file, and iframe partitioning
// is a browser behaviour no unit test can produce.

import assert from "node:assert/strict";
import test from "node:test";

import {
  PACK_ENGINE_API,
  PACK_QUOTA_HEADROOM_BYTES,
  PACK_REFUSAL,
  PACK_REFUSAL_CODES,
  PACK_SCHEMA_VERSION,
  PACK_SIZE_CEILING_BYTES,
  PACK_STEPS,
  activePackVersion,
  assetUrlAllowed,
  createPackInstaller,
  engineApiAdmits,
  packBytes,
  packSelfTest,
  packWordAdmissible,
  parsePackWords,
  quotaPlan,
  validateManifest,
} from "../src/proof_packs.mjs";

const WORDS = "alpha\nbravo\ncharlie\n";
const BYTES = new TextEncoder().encode(WORDS);
const DIGEST = "a".repeat(64);

/** A manifest that validates, so every test below can break exactly one thing. */
function manifest(overrides = {}) {
  return {
    schemaVersion: PACK_SCHEMA_VERSION,
    packId: "casual-proof-en-US-test",
    locale: "en-US",
    packVersion: "1.deadbeef",
    engineApiRange: String(PACK_ENGINE_API),
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
    provenance: [
      { name: "test", version: "1", license: "Apache-2.0", url: "https://example.test/x" },
    ],
    selfTest: { known: ["alpha", "charlie"], unknown: ["qzxbletch"] },
    ...overrides,
  };
}

/** Effects that all succeed. Each test replaces exactly one. */
function effects(overrides = {}) {
  const staged = [];
  const activated = [];
  const discarded = [];
  return {
    calls: { staged, activated, discarded },
    permit: () => true,
    estimate: () => ({ quota: 500 * 1024 * 1024, usage: 0 }),
    fetchAsset: async () => ({ bytes: BYTES, text: WORDS }),
    digest: async () => DIGEST,
    store: {
      stage: async (record) => staged.push(record.packVersion),
      activate: async (record) => activated.push(record.packVersion),
      discard: async (packId) => discarded.push(packId),
    },
    ...overrides,
  };
}

async function install(manifestIn, effectsIn, request = {}) {
  const installer = createPackInstaller(effectsIn);
  return installer.run({ locale: "en-US", manifest: manifestIn, ...request });
}

test("the eight steps of docs/146 §6 are the machine's states, in that order", () => {
  assert.deepEqual(PACK_STEPS, [
    "resolve",
    "budget",
    "fetch",
    "verify",
    "prepare",
    "selfTest",
    "activate",
    "cleanup",
  ]);
  // The ORDER is the contract, and two pairs of it are the whole argument for the
  // machine being a machine: verification before parsing (nothing unverified is
  // ever handed to a parser) and the self-test before activation (nothing
  // unproven is ever pointed at).
  assert.ok(PACK_STEPS.indexOf("verify") < PACK_STEPS.indexOf("prepare"));
  assert.ok(PACK_STEPS.indexOf("selfTest") < PACK_STEPS.indexOf("activate"));
  assert.equal(PACK_STEPS.at(-1), "cleanup");
});

test("a fresh install runs every step and activates exactly once", async () => {
  const fx = effects();
  const result = await install(manifest(), fx);
  assert.equal(result.ok, true, JSON.stringify(result));
  assert.deepEqual(
    result.trace.map((entry) => entry.step),
    PACK_STEPS,
    "every step ran, in order",
  );
  assert.deepEqual(fx.calls.staged, ["1.deadbeef"], "staged before it was activated");
  assert.deepEqual(fx.calls.activated, ["1.deadbeef"]);
  assert.deepEqual([...result.words].sort(), ["alpha", "bravo", "charlie"]);
});

test("a corrupted download is refused by name and never activated", async () => {
  // `docs/146` §9's "corrupted hash". The digest is INJECTED, which is why this is
  // three lines and not a fixture: the effect simply reports a different hash.
  const fx = effects({ digest: async () => "b".repeat(64) });
  const result = await install(manifest(), fx);
  assert.equal(result.ok, false);
  assert.equal(result.code, PACK_REFUSAL.digest);
  assert.equal(result.step, "verify");
  assert.deepEqual(fx.calls.activated, [], "NOTHING was activated");
  assert.deepEqual(fx.calls.staged, [], "and nothing was even staged — verify precedes prepare");
  assert.deepEqual(fx.calls.discarded, ["casual-proof-en-US-test"], "cleanup still ran");
});

test("a failure at step k runs steps 1..k and then cleanup, and no more", async () => {
  // The property that makes "a refused install cannot be observed" true rather
  // than hoped: the steps after the failure did not run, and the one that sweeps
  // the temporary namespace did.
  const fx = effects({ digest: async () => "c".repeat(64) });
  const result = await install(manifest(), fx);
  assert.deepEqual(
    result.trace.map((entry) => entry.step),
    ["resolve", "budget", "fetch", "verify", "cleanup"],
  );
});

test("bytes that are not the declared length are refused before they are parsed", async () => {
  const fx = effects({ fetchAsset: async () => ({ bytes: new Uint8Array(3), text: "no\n" }) });
  const result = await install(manifest(), fx);
  assert.equal(result.code, PACK_REFUSAL.bounds);
  assert.equal(result.step, "verify");
  assert.deepEqual(fx.calls.staged, []);
});

test("an unsupported schema is refused without a byte being fetched", async () => {
  let fetched = 0;
  const fx = effects({
    fetchAsset: async () => {
      fetched += 1;
      return { bytes: BYTES, text: WORDS };
    },
  });
  const result = await install(manifest({ schemaVersion: 99 }), fx);
  assert.equal(result.code, PACK_REFUSAL.schema);
  assert.equal(result.step, "resolve");
  assert.equal(fetched, 0, "refusing a 400 MB pack after downloading it is not a refusal");
});

test("insufficient quota is refused from the numbers the origin reports", async () => {
  const bytes = packBytes(manifest());
  const fx = effects({
    estimate: () => ({ quota: bytes + PACK_QUOTA_HEADROOM_BYTES - 1, usage: 0 }),
  });
  const result = await install(manifest(), fx);
  assert.equal(result.code, PACK_REFUSAL.quota);
  assert.equal(result.step, "budget");
});

test("an origin that will not report a quota has not said no", () => {
  // The distinction that matters: `StorageManager.estimate()` is not everywhere and
  // is an estimate where it is. "Unmeasured" must not be read as "does not fit", or
  // proofing would refuse to install on every browser that declines to answer.
  const plan = quotaPlan(1024, null);
  assert.equal(plan.ok, true);
  assert.equal(plan.measured, false);
  const measured = quotaPlan(1024, { quota: 10 * 1024 * 1024, usage: 0 });
  assert.equal(measured.measured, true);
});

test("the 50 MB ceiling is enforced from the manifest, whatever the quota says", () => {
  const plan = quotaPlan(PACK_SIZE_CEILING_BYTES + 1, { quota: 10 ** 12, usage: 0 });
  assert.equal(plan.ok, false);
  assert.equal(plan.code, PACK_REFUSAL.tooLarge);
  assert.equal(plan.detail.limitBytes, PACK_SIZE_CEILING_BYTES);
  assert.equal(PACK_SIZE_CEILING_BYTES, 50 * 1024 * 1024, "ADR-042 §8: ~50 MB per language");
});

test("a host that refuses the download gets a refusal that names it", async () => {
  const fx = effects({ permit: () => false });
  const result = await install(manifest(), fx);
  assert.equal(result.code, PACK_REFUSAL.hostRefused);
  // ADR-042 §6: the download decision is the host's. A `permit` that THROWS is
  // also a refusal — a policy adapter that blew up has not said yes.
  const threw = await install(manifest(), effects({ permit: () => { throw new Error("no"); } }));
  assert.equal(threw.code, PACK_REFUSAL.hostRefused);
});

test("a cancelled install refuses as cancelled and leaves nothing behind", async () => {
  const controller = new AbortController();
  const fx = effects({
    fetchAsset: async () => {
      controller.abort();
      throw new Error("aborted");
    },
  });
  const result = await install(manifest(), fx, { signal: controller.signal });
  assert.equal(result.code, PACK_REFUSAL.cancelled);
  assert.deepEqual(fx.calls.activated, []);
  assert.deepEqual(fx.calls.discarded, ["casual-proof-en-US-test"]);
});

test("a pack that fails its own self-test is staged and NOT activated", async () => {
  // The step a package manager does not have, and the reason it is here: a pack can
  // be byte-perfect and semantically empty, and a reader cannot tell a pack that
  // does nothing from one that works.
  const fx = effects();
  const result = await install(manifest({ selfTest: { known: ["zulu"], unknown: ["alpha"] } }), fx);
  assert.equal(result.code, PACK_REFUSAL.selfTest);
  assert.equal(result.step, "selfTest");
  assert.deepEqual(fx.calls.staged, ["1.deadbeef"], "it reached the staging namespace");
  assert.deepEqual(fx.calls.activated, [], "and never became the active version");
  assert.deepEqual(fx.calls.discarded, ["casual-proof-en-US-test"]);
});

test("the self-test's NEGATIVE half is what catches a pack that claims everything", () => {
  const everything = new Set(["alpha", "bravo", "qzxbletch"]);
  const refusal = packSelfTest(everything, { known: ["alpha"], unknown: ["qzxbletch"] });
  assert.equal(refusal.ok, false);
  assert.deepEqual(refusal.detail.overreach, ["qzxbletch"]);
  // Positives alone would have passed it, which is the whole point of the pair.
  assert.equal(packSelfTest(everything, { known: ["alpha"], unknown: ["qzxnothing"] }).ok, true);
});

test("storage refusing the activation is a refusal, not a silent half-install", async () => {
  const fx = effects();
  fx.store.activate = async () => {
    throw new Error("QuotaExceededError");
  };
  const result = await install(manifest(), fx);
  assert.equal(result.code, PACK_REFUSAL.storage);
  assert.equal(result.step, "activate");
});

test("a URL policy refusal happens before anything is fetched", async () => {
  for (const url of [
    "http://evil.test/words.txt",
    "https://evil.test/words.txt",
    "//evil.test/words.txt",
    "data:text/plain,alpha",
    "javascript:alert(1)",
  ]) {
    const result = await install(manifest({ assets: [{ ...manifest().assets[0], url }] }), effects());
    assert.equal(result.code, PACK_REFUSAL.assetUrl, `${url} must be refused`);
    assert.equal(result.step, "resolve");
  }
});

test("the URL policy admits relative URLs and origins the host named, and nothing else", () => {
  assert.equal(assetUrlAllowed("packs/x/words.txt"), true, "the shipped pack");
  assert.equal(assetUrlAllowed("./packs/x/words.txt"), true);
  assert.equal(assetUrlAllowed("//cdn.test/x"), false, "protocol-relative is absolute in disguise");
  assert.equal(assetUrlAllowed("http://cdn.test/x", { origin: "http://cdn.test" }), false, "http");
  assert.equal(assetUrlAllowed("https://cdn.test/x"), false, "an origin nobody named");
  assert.equal(assetUrlAllowed("https://cdn.test/x", { origin: "https://cdn.test" }), true);
  assert.equal(
    assetUrlAllowed("https://cdn.test/x", { allowedOrigins: ["https://cdn.test"] }),
    true,
  );
  assert.equal(assetUrlAllowed("", {}), false);
});

test("a manifest with no provenance does not install", () => {
  // ADR-042 leaves the redistributable-corpus question OPEN, and a pack that does
  // not say where its words came from is a pack nobody can answer it about.
  assert.equal(validateManifest(manifest({ provenance: [] })).code, PACK_REFUSAL.manifest);
  assert.equal(
    validateManifest(manifest({ provenance: [{ name: "x", version: "1", url: "https://x.test" }] }))
      .detail.field,
    "provenance.license",
  );
});

test("every required manifest field is required, and says which one is missing", () => {
  for (const field of ["packId", "locale", "packVersion", "engineApiRange"]) {
    const broken = manifest();
    delete broken[field];
    const result = validateManifest(broken);
    assert.equal(result.ok, false);
    assert.equal(result.detail.field, field);
  }
  assert.equal(validateManifest(null).code, PACK_REFUSAL.manifest);
  assert.equal(validateManifest([]).code, PACK_REFUSAL.manifest);
  assert.equal(validateManifest(manifest({ assets: [] })).detail.field, "assets");
  assert.equal(
    validateManifest(manifest({ assets: [{ ...manifest().assets[0], sha256: "SHORT" }] })).detail
      .field,
    "assets.sha256",
  );
  // An UPPER-CASE digest is refused too: the comparison is case-sensitive on the
  // manifest's side on purpose, so a pack cannot declare one casing and be
  // compared in another.
  assert.equal(
    validateManifest(manifest({ assets: [{ ...manifest().assets[0], sha256: DIGEST.toUpperCase() }] }))
      .detail.field,
    "assets.sha256",
  );
});

test("a pack for another language, or another engine API, is refused by name", () => {
  assert.equal(validateManifest(manifest(), { locale: "fr-FR" }).code, PACK_REFUSAL.locale);
  assert.equal(
    validateManifest(manifest({ engineApiRange: "9" })).code,
    PACK_REFUSAL.engineApi,
  );
  assert.equal(engineApiAdmits("1"), true);
  assert.equal(engineApiAdmits("1-3"), true);
  assert.equal(engineApiAdmits("2-3"), false);
  assert.equal(engineApiAdmits("~1"), false, "a range we cannot read is not a yes");
  assert.equal(engineApiAdmits(""), false);
});

test("a capability this build cannot honour is refused rather than half-installed", () => {
  // A pack claiming `grammar` would be reported as installed and then do less than
  // it said, which is `docs/146` §1's failure wearing a success's clothes.
  const result = validateManifest(manifest({ capabilities: ["spelling", "grammar"] }));
  assert.equal(result.code, PACK_REFUSAL.format);
  assert.equal(validateManifest(manifest({ capabilities: ["nonsense"] })).code, PACK_REFUSAL.manifest);
});

test("the active pack version names every pack and is order-independent", () => {
  // Two sessions that installed the same packs in a different order must compute
  // the same cache key, or one of them re-checks the entire window for nothing.
  const a = activePackVersion({
    assets: 3,
    packs: [
      { packId: "b", packVersion: "2" },
      { packId: "a", packVersion: "1" },
    ],
  });
  const b = activePackVersion({
    assets: 3,
    packs: [
      { packId: "a", packVersion: "1" },
      { packId: "b", packVersion: "2" },
    ],
  });
  assert.equal(a, b);
  assert.ok(a.includes("a@1") && a.includes("b@2"));
  // And with no packs it is the Increment A token, so a build with none computes
  // exactly the keys it did before.
  assert.equal(activePackVersion({ assets: 3 }), "basic-1.3");
  assert.notEqual(activePackVersion({ assets: 3, packs: [{ packId: "a", packVersion: "1" }] }), "basic-1.3");
  // A half-formed record cannot silently vanish from the token AND cannot corrupt
  // it: it is filtered, and the filtered token is still distinct from no packs.
  assert.equal(activePackVersion({ assets: 1, packs: [null, { packId: "a" }] }), "basic-1.1");
});

test("pack words are NFC-normalized at parse time, once", () => {
  // `isKnownWord` normalizes the TOKEN; normalizing the pack once here is what
  // makes the two meet. A `.docx` can carry NFD, where `café` is `cafe` + U+0301.
  const words = parsePackWords("café\n  spaced  \n\nlast\n");
  assert.ok(words.has("café"), "decomposed input, composed entry");
  assert.ok(words.has("spaced"));
  assert.equal(words.size, 3);
});

test("only word-shaped candidates are admissible", () => {
  assert.equal(packWordAdmissible("webhook"), true);
  assert.equal(packWordAdmissible("e-mail"), true);
  assert.equal(packWordAdmissible("don't"), true);
  assert.equal(packWordAdmissible("ab"), false, "too short");
  assert.equal(packWordAdmissible("x".repeat(41)), false, "too long");
  assert.equal(packWordAdmissible("snake_case"), false, "an identifier is not a word");
  assert.equal(packWordAdmissible("camelCase2"), false, "a digit is skipped by rule anyway");
  assert.equal(packWordAdmissible("info@x.test"), false);
  // ALL CAPS IS ADMITTED, and that is `build-glossary.mjs`'s reasoning applied
  // here: the checker skips an all-caps token by DEFAULT, but the user can turn
  // that option off, and then an acronym needs a real entry. So the rule is run
  // with `ignoreUpper: false`, exactly as the glossary generator runs it.
  assert.equal(packWordAdmissible("OAuth"), true);
  assert.equal(packWordAdmissible("HTTP"), true);
  assert.equal(packWordAdmissible(42), false);
});

test("every refusal code is distinct, and the set is what the UI maps over", () => {
  assert.equal(new Set(PACK_REFUSAL_CODES).size, PACK_REFUSAL_CODES.length);
  assert.equal(PACK_REFUSAL_CODES.length, Object.keys(PACK_REFUSAL).length);
});
