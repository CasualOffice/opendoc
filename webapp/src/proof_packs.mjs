// Language packs: the manifest, the policy, and the eight-step install, as a
// state machine over injected effects.
//
// Designed in `docs/146` §5/§6 (the owner's architecture of 2026-09-28) and
// decided in **ADR-042**. This is Increment B — installation — and it is the
// half of the pack system that DECIDES things: what a manifest may say, where
// an asset may be fetched from, how much room a pack may take, what each
// refusal is called, and in what order the eight steps of `docs/146` §6 run.
// Where the bytes actually come from and where they are kept is
// `proof_store.mjs` and `proof_sdk.mjs`.
//
// ## Why this file is pure, and what that costs the caller
//
// It is in `PURE_MODULES`, for the same three reasons `proof_protocol.mjs` is,
// plus one that is specific to installation:
//
//   * **ADR-042 §6 — network and storage are injected by the host.** A pack may
//     be served from the editor's own origin, from a host's CDN, or handed over
//     as bytes by an embedder that allows no network at all. A module that
//     reached for `fetch` could not serve the third case, and the third case is
//     the one an enterprise embed actually asks for.
//   * **`digest` is injected too**, and that is not only about purity: a module
//     that reached a crypto global would trip the `BROWSER_GLOBALS` guard in
//     `module_seams.test.mjs`, and — more usefully — a test could not then drive
//     a checksum MISMATCH without corrupting a real file. Injecting it makes the
//     corrupted-download path (`docs/146` §9, "corrupted hash") a two-line test
//     rather than a fixture nobody writes.
//   * A refusal that only a browser can produce is a refusal nobody unit-tests.
//     Every one of the codes below is reachable from node.
//
// ## The known pattern this is, before any of it was invented
//
// Nothing here is new (SKILL.md §8): it is a **staged, atomically-promoted,
// content-addressed cache** — the shape a package manager, a service worker's
// install/activate pair, and an atomic file rename all use. Fetch into a
// temporary namespace, verify against a digest declared out of band, prove the
// result works, flip ONE pointer, then sweep. The properties that matters come
// from the shape and not from care: a failure before the flip cannot be
// observed, and the previous version stays usable throughout.
//
// The one place it departs from a package manager is the SELF-TEST between
// prepare and activate. A package manager trusts its checksum; a proofing pack
// cannot, because a pack that is byte-perfect and semantically empty is
// indistinguishable, to a reader, from a pack that works — the same failure
// `docs/146` §1 forbids ("a document that is silently unchecked is
// indistinguishable from a clean one"). So a pack carries examples it must get
// right, and getting them wrong is a refusal rather than an activation.

import { BASIC_PACK_VERSION } from "./proof_protocol.mjs";
import { skipReason } from "./spelling.mjs";

/** The manifest shape this build understands (`docs/146` §5). A pack declaring
 *  anything else is refused by NAME rather than parsed hopefully. */
export const PACK_SCHEMA_VERSION = 1;

/** The proofing engine API a pack declares it works against. A pack's
 *  `engineApiRange` is `"N"` or `"N-M"` over this integer; it is deliberately
 *  not semver, because there is exactly one number that matters — the shape of
 *  the resources message the worker will be handed — and a range grammar nobody
 *  needs is a range grammar with bugs in it. */
export const PACK_ENGINE_API = 1;

/** The ceiling on one language's installed pack, in bytes.
 *
 *  ADR-042 §8, from the owner on 2026-09-29: **~50 MB per language on the
 *  lowest supported device**. That is a CAP to design tiers against, not a
 *  target, and it does not remove the measurement obligation — cold load and
 *  worker memory are still instrumented per pack (`docs/146` §9). It is
 *  enforced here, before a single byte is fetched, from the manifest's own
 *  declared sizes: refusing a 400 MB pack after downloading it is not a
 *  refusal, it is a bill. */
export const PACK_SIZE_CEILING_BYTES = 50 * 1024 * 1024;

/** Room left unspent in the origin's quota after an install.
 *
 *  Browser storage is quota-bound and evictable (`docs/146` §6), and the data
 *  that must survive an eviction is the USER's — their own words, their
 *  settings, their unsaved drafts. A pack is disposable and reconstructible by
 *  construction, so an install that filled the quota to the brim would be
 *  trading valuable data for replaceable data. This is what stops it. */
export const PACK_QUOTA_HEADROOM_BYTES = 8 * 1024 * 1024;

/** Asset formats this build can parse. An unknown format is refused BEFORE the
 *  bytes are looked at, which is the "bounded asset parsing" of `docs/146` §9.
 *
 *  `words-plain-1` is one NFC word per line, newline-separated, sorted, no
 *  sections — the shape `dict/glossary.txt` already has, deliberately, so the
 *  supplement tier needs no second parser. */
export const PACK_ASSET_FORMATS = Object.freeze(["words-plain-1"]);

/** The capabilities a pack may claim (`docs/146` §5). Increment B ships only
 *  `spelling`; the other three are listed because the manifest may declare them
 *  and a pack that claims one this build cannot honour must be refused rather
 *  than silently half-installed. */
export const PACK_CAPABILITIES = Object.freeze(["spelling", "grammar", "context", "consistency"]);

/** Capabilities this build can actually act on. */
export const PACK_SUPPORTED_CAPABILITIES = Object.freeze(["spelling"]);

/**
 * Every way an install can refuse, named.
 *
 * A code and not a sentence, for the reason `version_policy.mjs` already
 * records: the module that DECIDES must not own the English, or the decision
 * cannot be translated and the sentence cannot be tested. `proof_languages.mjs`
 * maps each of these to a catalogue key, and a guard asserts the map is total —
 * a refusal with no sentence is a dialog that closes with nothing said, which is
 * the failure SKILL.md §10 forbids.
 */
export const PACK_REFUSAL = Object.freeze({
  /** The host's policy adapter said no to downloading. */
  hostRefused: "host-refused",
  /** The manifest is not an object, or a required field is missing or the wrong
   *  type. Nothing was fetched. */
  manifest: "manifest",
  /** `schemaVersion` is not this build's. */
  schema: "schema",
  /** The pack is for a different locale than the one being installed. */
  locale: "locale",
  /** `engineApiRange` does not admit `PACK_ENGINE_API`. */
  engineApi: "engine-api",
  /** An asset URL is not one the URL policy allows. */
  assetUrl: "asset-url",
  /** The declared total is over `PACK_SIZE_CEILING_BYTES`. */
  tooLarge: "too-large",
  /** The origin's quota cannot hold it with headroom to spare. */
  quota: "quota",
  /** A fetch failed or returned nothing. */
  network: "network",
  /** The caller's `AbortSignal` fired. */
  cancelled: "cancelled",
  /** SHA-256 of the fetched bytes is not what the manifest declared. */
  digest: "digest",
  /** The fetched bytes are not the declared length, or exceed the maximum
   *  uncompressed size for their format. */
  bounds: "bounds",
  /** The asset format, or a claimed capability, is not one this build reads. */
  format: "format",
  /** The pack's own positive/negative examples did not come out right. */
  selfTest: "self-test",
  /** Storage refused the write. The previously active version is untouched. */
  storage: "storage",
  /** No pack is published for the requested locale. */
  notPublished: "not-published",
  /** Nothing is installed for the locale a removal names. */
  notInstalled: "not-installed",
});

/** The refusal codes, as a set, so a guard can assert a map over them is total. */
export const PACK_REFUSAL_CODES = Object.freeze(Object.values(PACK_REFUSAL));

/**
 * The eight steps of `docs/146` §6, in order, as the state machine's states.
 *
 * Named and exported because the ORDER is the contract, not an implementation
 * detail: verification must precede parsing, the self-test must precede
 * activation, and cleanup must run whichever way the machine leaves. A test
 * asserts a failure at step *k* ran steps 1..k and no more, which is the only
 * way to show that a refused install never touched the active pointer.
 *
 *   1. `resolve`   locale, manifest, and host download permission
 *   2. `budget`    declared bytes against the ceiling and the quota
 *   3. `fetch`     the assets, abortable, with byte progress
 *   4. `verify`    digest, declared length, format, bounds
 *   5. `prepare`   parse into the lookup structure, in a temp namespace
 *   6. `selfTest`  the pack's own positive and negative examples
 *   7. `activate`  flip the active-version record atomically
 *   8. `cleanup`   drop temp data; on failure keep the previous version
 */
export const PACK_STEPS = Object.freeze([
  "resolve",
  "budget",
  "fetch",
  "verify",
  "prepare",
  "selfTest",
  "activate",
  "cleanup",
]);

/** A refusal, in the shape every entry point here returns. `detail` is data for
 *  the sentence (a limit, a language name) and never itself English. */
export function packRefusal(code, step = "", detail = {}) {
  return Object.freeze({ ok: false, code, step, detail: Object.freeze({ ...detail }) });
}

/** Whether `value` is a plain finite non-negative integer. Manifests arrive
 *  from a host and are DATA, not a promise. */
function isByteCount(value) {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function isNonEmptyString(value) {
  return typeof value === "string" && value.trim().length > 0;
}

/** SHA-256 hex, as a shape: 64 lower-case hex digits. A manifest that declares
 *  a truncated or upper-cased digest is refused here rather than compared
 *  case-sensitively against a real one later and reported as corruption. */
const SHA256_HEX = /^[0-9a-f]{64}$/;

/**
 * Whether `range` admits `api`.
 *
 * `"1"` is exactly 1; `"1-3"` is 1 through 3 inclusive. Anything else is not a
 * range this build reads, and an unreadable range is a refusal rather than an
 * optimistic yes — a pack whose compatibility statement cannot be understood is
 * precisely the pack that should not be activated.
 */
export function engineApiAdmits(range, api = PACK_ENGINE_API) {
  if (!isNonEmptyString(range)) return false;
  const exact = /^(\d+)$/.exec(range.trim());
  if (exact) return Number(exact[1]) === api;
  const span = /^(\d+)\s*-\s*(\d+)$/.exec(range.trim());
  if (!span) return false;
  const low = Number(span[1]);
  const high = Number(span[2]);
  return low <= high && api >= low && api <= high;
}

/**
 * Whether an asset URL is one this editor may fetch (`docs/146` §6: "resolved
 * by host policy, same-origin by default").
 *
 * The rule, and why each half of it is here:
 *
 *   * a RELATIVE url resolves against the editor's own origin and is allowed —
 *     that is the shipped pack, served beside the page;
 *   * an ABSOLUTE url is allowed only when its origin is the editor's own or one
 *     the host explicitly named. A pack is a file this editor parses and then
 *     believes about a user's language; "wherever the manifest says" is not a
 *     policy;
 *   * `http:` is refused outright, along with every other scheme. `data:` and
 *     `blob:` would let a manifest carry its own payload past the digest
 *     declaration; `javascript:` needs no explanation; and an `http:` pack on an
 *     `https:` page is a downgrade the browser would block anyway, so refusing
 *     it here turns a silent failure into a sentence.
 *
 * `origin` and `allowedOrigins` are supplied by the caller — this module does
 * not know what page it is on, and that is ADR-042 §6 rather than fastidiousness.
 */
export function assetUrlAllowed(url, { origin = "", allowedOrigins = [] } = {}) {
  if (!isNonEmptyString(url)) return false;
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(url)) {
    let parsed = null;
    try {
      parsed = new URL(url);
    } catch {
      return false;
    }
    if (parsed.protocol !== "https:") return false;
    const permitted = new Set([origin, ...allowedOrigins].filter(Boolean));
    return permitted.has(parsed.origin);
  }
  // A protocol-relative `//host/path` is an absolute URL wearing a disguise.
  if (url.startsWith("//")) return false;
  return true;
}

/**
 * Validates a manifest and returns a normalized copy, or a refusal.
 *
 * Everything is checked BEFORE anything is fetched, which is what makes the
 * refusals cheap and the bad cases testable. The normalized copy is frozen and
 * is what every later step reads, so a host cannot mutate the manifest out from
 * under an install in flight.
 */
export function validateManifest(manifest, { locale = "", origin = "", allowedOrigins = [] } = {}) {
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest)) {
    return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: "manifest" });
  }
  if (manifest.schemaVersion !== PACK_SCHEMA_VERSION) {
    return packRefusal(PACK_REFUSAL.schema, "resolve", {
      declared: manifest.schemaVersion,
      supported: PACK_SCHEMA_VERSION,
    });
  }
  for (const field of ["packId", "locale", "packVersion", "engineApiRange"]) {
    if (!isNonEmptyString(manifest[field])) {
      return packRefusal(PACK_REFUSAL.manifest, "resolve", { field });
    }
  }
  if (locale && manifest.locale !== locale) {
    return packRefusal(PACK_REFUSAL.locale, "resolve", {
      declared: manifest.locale,
      wanted: locale,
    });
  }
  if (!engineApiAdmits(manifest.engineApiRange)) {
    return packRefusal(PACK_REFUSAL.engineApi, "resolve", {
      declared: manifest.engineApiRange,
      api: PACK_ENGINE_API,
    });
  }
  const capabilities = manifest.capabilities;
  if (!Array.isArray(capabilities) || capabilities.length === 0) {
    return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: "capabilities" });
  }
  for (const capability of capabilities) {
    if (!PACK_CAPABILITIES.includes(capability)) {
      return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: "capabilities" });
    }
    // A pack claiming a capability this build has no code for would be installed,
    // reported as installed, and then do less than it said. Refused instead.
    if (!PACK_SUPPORTED_CAPABILITIES.includes(capability)) {
      return packRefusal(PACK_REFUSAL.format, "resolve", { capability });
    }
  }
  if (!Array.isArray(manifest.assets) || manifest.assets.length === 0) {
    return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: "assets" });
  }
  const seen = new Set();
  for (const asset of manifest.assets) {
    if (!asset || typeof asset !== "object") {
      return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: "assets" });
    }
    if (!isNonEmptyString(asset.id) || seen.has(asset.id)) {
      return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: "assets.id" });
    }
    seen.add(asset.id);
    if (!isByteCount(asset.bytes) || asset.bytes === 0) {
      return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: "assets.bytes" });
    }
    if (typeof asset.sha256 !== "string" || !SHA256_HEX.test(asset.sha256)) {
      return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: "assets.sha256" });
    }
    if (!PACK_ASSET_FORMATS.includes(asset.format)) {
      return packRefusal(PACK_REFUSAL.format, "resolve", { format: asset.format });
    }
    if (!assetUrlAllowed(asset.url, { origin, allowedOrigins })) {
      return packRefusal(PACK_REFUSAL.assetUrl, "resolve", { url: String(asset.url ?? "") });
    }
  }
  // Provenance is REQUIRED, with a licence per entry. `docs/146` §6: "Use
  // reproducible inputs, pinned versions, checksums, and provenance", and
  // ADR-042 leaves the redistributable-corpus question open precisely because a
  // dataset's licence is not inferable from an engine's. A pack that does not
  // say where its words came from is a pack nobody can answer that question
  // about, so it does not install.
  if (!Array.isArray(manifest.provenance) || manifest.provenance.length === 0) {
    return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: "provenance" });
  }
  for (const entry of manifest.provenance) {
    if (!entry || typeof entry !== "object") {
      return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: "provenance" });
    }
    for (const field of ["name", "version", "license", "url"]) {
      if (!isNonEmptyString(entry[field])) {
        return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: `provenance.${field}` });
      }
    }
  }
  const selfTest = manifest.selfTest;
  if (!selfTest || typeof selfTest !== "object") {
    return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: "selfTest" });
  }
  if (!Array.isArray(selfTest.known) || selfTest.known.length === 0) {
    return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: "selfTest.known" });
  }
  if (!Array.isArray(selfTest.unknown) || selfTest.unknown.length === 0) {
    return packRefusal(PACK_REFUSAL.manifest, "resolve", { field: "selfTest.unknown" });
  }
  return Object.freeze({
    ok: true,
    manifest: Object.freeze({
      schemaVersion: manifest.schemaVersion,
      packId: manifest.packId,
      locale: manifest.locale,
      packVersion: manifest.packVersion,
      engineApiRange: manifest.engineApiRange,
      capabilities: Object.freeze([...capabilities]),
      assets: Object.freeze(
        manifest.assets.map((asset) =>
          Object.freeze({
            id: asset.id,
            url: asset.url,
            bytes: asset.bytes,
            sha256: asset.sha256,
            format: asset.format,
          }),
        ),
      ),
      provenance: Object.freeze(
        manifest.provenance.map((entry) =>
          Object.freeze({
            name: entry.name,
            version: entry.version,
            license: entry.license,
            url: entry.url,
          }),
        ),
      ),
      selfTest: Object.freeze({
        known: Object.freeze([...selfTest.known]),
        unknown: Object.freeze([...selfTest.unknown]),
      }),
    }),
  });
}

/** The declared size of a validated manifest, in bytes. */
export function packBytes(manifest) {
  return manifest.assets.reduce((sum, asset) => sum + asset.bytes, 0);
}

/**
 * Whether there is room for `bytes`, given what the origin reports.
 *
 * `estimate` is `{ quota, usage }` or null — `StorageManager.estimate()` is not
 * everywhere, and where it is, it is an ESTIMATE. A missing estimate is not a
 * refusal: the install goes ahead and storage refuses it if it must, which is
 * the honest order (a browser that will not tell us the quota has not said the
 * quota is zero). What is refused is an estimate that says, in its own numbers,
 * that this will not fit.
 */
export function quotaPlan(bytes, estimate, headroom = PACK_QUOTA_HEADROOM_BYTES) {
  if (bytes > PACK_SIZE_CEILING_BYTES) {
    return packRefusal(PACK_REFUSAL.tooLarge, "budget", {
      bytes,
      limitBytes: PACK_SIZE_CEILING_BYTES,
    });
  }
  if (!estimate || !isByteCount(estimate.quota) || estimate.quota === 0) {
    return Object.freeze({ ok: true, measured: false, bytes });
  }
  const usage = isByteCount(estimate.usage) ? estimate.usage : 0;
  const free = estimate.quota - usage;
  if (free < bytes + headroom) {
    return packRefusal(PACK_REFUSAL.quota, "budget", { bytes, freeBytes: Math.max(free, 0) });
  }
  return Object.freeze({ ok: true, measured: true, bytes, freeBytes: free });
}

/**
 * Parses a `words-plain-1` asset into the supplement set.
 *
 * NFC-normalized on the way in, for the same reason `isKnownWord` normalizes
 * before comparing: a `.docx` can carry NFD, where `café` is `cafe` plus U+0301
 * — the same word, a different string, and `Set.has` says no. Normalizing the
 * PACK once at parse time is strictly cheaper than normalizing every token, and
 * `isKnownWord` already normalizes the token, so the two meet.
 *
 * O(bytes). Called once per install and once per session load.
 */
export function parsePackWords(text) {
  const out = new Set();
  for (const line of String(text ?? "").replace(/\r\n/g, "\n").split("\n")) {
    const word = line.trim();
    if (word.length > 0) out.add(word.normalize("NFC"));
  }
  return out;
}

/**
 * The pack's own examples, run against what was actually parsed.
 *
 * `docs/146` §6 step 6: "Run a small pack self-test (known positive and negative
 * examples)." The NEGATIVE half is the half that matters and the half a careless
 * implementation drops: a pack containing every string in the language would
 * pass every positive example and would turn spell checking off. So a pack
 * declares words it must NOT claim, and claiming one is a refusal.
 */
export function packSelfTest(words, selfTest) {
  const missing = selfTest.known.filter((word) => !words.has(String(word).normalize("NFC")));
  const overreach = selfTest.unknown.filter((word) => words.has(String(word).normalize("NFC")));
  if (missing.length || overreach.length) {
    return packRefusal(PACK_REFUSAL.selfTest, "selfTest", {
      missing: missing.slice(0, 4),
      overreach: overreach.slice(0, 4),
    });
  }
  return Object.freeze({ ok: true });
}

/**
 * The `activePackVersion` of `docs/146` §4, for real this time.
 *
 * Increment A's seam was `` `${BASIC_PACK_VERSION}.${assetsCounter}` `` — the
 * base data plus how much of it had arrived. That was right for a build with one
 * pack and is wrong the moment a pack can be installed or removed, because the
 * cache key has to change when the RESOURCES change and installing a pack
 * changes them without touching a single paragraph. So the token now names every
 * active pack and its immutable version, sorted so two sessions that installed
 * the same packs in a different order compute the same key.
 *
 * O(packs). There is one pack per locale and a handful of locales.
 */
export function activePackVersion({ base = BASIC_PACK_VERSION, assets = 0, packs = [] } = {}) {
  const active = [...packs]
    .filter((pack) => pack && isNonEmptyString(pack.packId) && isNonEmptyString(pack.packVersion))
    .map((pack) => `${pack.packId}@${pack.packVersion}`)
    .sort();
  return active.length ? `${base}.${assets}+${active.join("+")}` : `${base}.${assets}`;
}

/** Whether `word` is one a pack may legitimately carry.
 *
 *  A pack is a mechanism for accepting words, so the same rule the glossary
 *  generator follows applies with more force here: a token the checker would
 *  skip anyway needs no entry, and a token that is not word-shaped is a way to
 *  accept typos. Used by `tools/build-pack.mjs` AND by the artifact guard, so
 *  the generator and the test cannot disagree about it. */
export function packWordAdmissible(word) {
  if (typeof word !== "string") return false;
  if (word.length < 3 || word.length > 40) return false;
  if (!/^\p{L}[\p{L}\p{M}'’-]*$/u.test(word)) return false;
  return skipReason(word, { ignoreNumbers: true, ignoreUpper: false }) === "";
}

/**
 * The eight-step installer, as a state machine over injected effects.
 *
 * `effects` is the whole of its contact with the world, and every one of these
 * is a seam a host may own (ADR-042 §6):
 *
 *   `permit({ locale, bytes, packId })`   → boolean. The host's download policy.
 *   `estimate()`                          → `{ quota, usage }` or null.
 *   `fetchAsset(url, { signal, onProgress })` → `{ bytes, text }`.
 *   `digest(bytes)`                       → lower-case SHA-256 hex. INJECTED —
 *                                           see the header for both reasons.
 *   `store`                               → `stage`/`activate`/`discard`, the
 *                                           `proof_store.mjs` shape.
 *   `onStep(step, info)`                  → optional progress/trace hook.
 *
 * `run(request)` resolves to `{ ok: true, pack, words, trace }` or a refusal
 * with the step it happened at. It never throws for a refusal: a thrown error is
 * a bug in an effect, and a caller that has to tell an expected refusal from a
 * bug by catching cannot.
 */
export function createPackInstaller(effects) {
  const trace = [];

  function note(step, info = {}) {
    trace.push(Object.freeze({ step, ...info }));
    effects.onStep?.(step, info);
  }

  /** The state machine's states. Each returns a refusal or the state it hands
   *  to the next step; the driver below walks `PACK_STEPS` in order. */
  const states = {
    async resolve(state, request) {
      const validated = validateManifest(request.manifest, {
        locale: request.locale,
        origin: request.origin,
        allowedOrigins: request.allowedOrigins,
      });
      if (!validated.ok) return validated;
      let permitted = true;
      try {
        permitted = (await effects.permit?.({
          locale: validated.manifest.locale,
          packId: validated.manifest.packId,
          bytes: packBytes(validated.manifest),
        })) ?? true;
      } catch {
        permitted = false;
      }
      if (!permitted) {
        return packRefusal(PACK_REFUSAL.hostRefused, "resolve", {
          locale: validated.manifest.locale,
        });
      }
      return { ...state, manifest: validated.manifest };
    },

    async budget(state) {
      const bytes = packBytes(state.manifest);
      let estimate = null;
      try {
        estimate = (await effects.estimate?.()) ?? null;
      } catch {
        estimate = null;
      }
      const plan = quotaPlan(bytes, estimate);
      if (!plan.ok) return plan;
      return { ...state, bytes, plan };
    },

    async fetch(state, request) {
      const fetched = [];
      let done = 0;
      for (const asset of state.manifest.assets) {
        if (request.signal?.aborted) return packRefusal(PACK_REFUSAL.cancelled, "fetch");
        let result = null;
        try {
          result = await effects.fetchAsset(asset.url, {
            signal: request.signal,
            onProgress: (loaded) =>
              effects.onStep?.("fetch", { loaded: done + loaded, total: state.bytes }),
          });
        } catch (error) {
          if (request.signal?.aborted) return packRefusal(PACK_REFUSAL.cancelled, "fetch");
          return packRefusal(PACK_REFUSAL.network, "fetch", {
            url: asset.url,
            cause: String(error?.message ?? error ?? ""),
          });
        }
        if (!result || !result.bytes) {
          return packRefusal(PACK_REFUSAL.network, "fetch", { url: asset.url });
        }
        done += result.bytes.byteLength ?? result.bytes.length ?? 0;
        fetched.push({ asset, ...result });
        // Per-asset progress, reported whether or not the host's `fetchAsset`
        // streams. `docs/146` §3 asks for byte progress to be explicit, and the
        // granularity an effect that cannot stream can honestly offer is one tick
        // per asset — which is stated rather than dressed up as a smooth bar.
        effects.onStep?.("fetch", { loaded: done, total: state.bytes });
      }
      if (request.signal?.aborted) return packRefusal(PACK_REFUSAL.cancelled, "fetch");
      return { ...state, fetched };
    },

    async verify(state) {
      for (const entry of state.fetched) {
        const length = entry.bytes.byteLength ?? entry.bytes.length ?? 0;
        if (length !== entry.asset.bytes) {
          return packRefusal(PACK_REFUSAL.bounds, "verify", {
            id: entry.asset.id,
            declared: entry.asset.bytes,
            actual: length,
          });
        }
        if (length > PACK_SIZE_CEILING_BYTES) {
          return packRefusal(PACK_REFUSAL.bounds, "verify", { id: entry.asset.id });
        }
        let hex = "";
        try {
          hex = String(await effects.digest(entry.bytes));
        } catch (error) {
          return packRefusal(PACK_REFUSAL.digest, "verify", {
            id: entry.asset.id,
            cause: String(error?.message ?? error ?? ""),
          });
        }
        // Compared case-insensitively on OUR side only: the manifest's own
        // digest was already required to be lower-case hex by `validateManifest`,
        // so this tolerates an effect that returns upper case without tolerating
        // a manifest that declares one.
        if (hex.toLowerCase() !== entry.asset.sha256) {
          return packRefusal(PACK_REFUSAL.digest, "verify", { id: entry.asset.id });
        }
      }
      return state;
    },

    async prepare(state) {
      const words = new Set();
      for (const entry of state.fetched) {
        if (entry.asset.format !== "words-plain-1") {
          return packRefusal(PACK_REFUSAL.format, "prepare", { format: entry.asset.format });
        }
        for (const word of parsePackWords(entry.text)) words.add(word);
      }
      if (words.size === 0) {
        return packRefusal(PACK_REFUSAL.bounds, "prepare", { words: 0 });
      }
      const record = Object.freeze({
        packId: state.manifest.packId,
        locale: state.manifest.locale,
        packVersion: state.manifest.packVersion,
        bytes: state.bytes,
        capabilities: state.manifest.capabilities,
        provenance: state.manifest.provenance,
        words: Object.freeze([...words].sort()),
      });
      // The TEMPORARY namespace of `docs/146` §6 step 5. Staging can fail, and a
      // staging failure has to be a refusal that leaves the previous version
      // exactly where it was — which is why it is a separate call from
      // `activate` rather than one write with a flag.
      try {
        await effects.store?.stage?.(record);
      } catch (error) {
        return packRefusal(PACK_REFUSAL.storage, "prepare", {
          cause: String(error?.message ?? error ?? ""),
        });
      }
      return { ...state, words, record };
    },

    async selfTest(state) {
      const result = packSelfTest(state.words, state.manifest.selfTest);
      if (!result.ok) return result;
      return state;
    },

    async activate(state) {
      try {
        await effects.store?.activate?.(state.record);
      } catch (error) {
        return packRefusal(PACK_REFUSAL.storage, "activate", {
          cause: String(error?.message ?? error ?? ""),
        });
      }
      return state;
    },

    async cleanup(state) {
      // Runs on the way OUT, success or failure — which is why it is a step and
      // not a `finally` somewhere: a cleanup that only runs on one path is how
      // a half-written temp namespace survives a refusal and gets promoted by
      // the next attempt.
      try {
        await effects.store?.discard?.(state.manifest?.packId ?? "");
      } catch {
        // A temp namespace that will not clear is not worth failing an install
        // that otherwise succeeded: the records are keyed by version, so the
        // leftovers are inert. They are reported, not fatal.
        note("cleanup", { swept: false });
        return state;
      }
      return state;
    },
  };

  return {
    /** The steps this machine will run, in order. */
    steps: PACK_STEPS,
    /** What actually happened, for a guard to assert against. */
    trace,

    async run(request) {
      let state = { manifest: null };
      for (const step of PACK_STEPS) {
        if (step === "cleanup") break;
        note(step);
        const next = await states[step](state, request);
        if (next && next.ok === false) {
          note("cleanup", { after: step });
          await states.cleanup(state);
          return Object.freeze({ ...next, trace: Object.freeze([...trace]) });
        }
        state = next;
      }
      note("cleanup");
      await states.cleanup(state);
      return Object.freeze({
        ok: true,
        pack: state.record,
        words: state.words,
        trace: Object.freeze([...trace]),
      });
    },
  };
}
