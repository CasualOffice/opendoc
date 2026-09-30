// The proofing SDK boundary: `configureProofing`, `installPack`, `removePack`,
// `checkRange`, `checkDocument`, `onFindings`, `dispose`.
//
// ADR-042 §6, from the owner on 2026-09-29: **proofing is a separate optional
// package, not a webapp feature**, with network and storage providers injected by
// the host. The package is not published yet, and building behind the boundary
// now is what makes publishing packaging work rather than a rewrite. This file is
// that boundary and nothing else — every decision it appears to make is made in
// `proof_packs.mjs` (pure), `proof_store.mjs` (storage) or the coordinator.
//
// ## The rule this file exists to honour
//
// **Nothing here may reach a webapp global, assume the editor's own `fetch`, or
// assume its storage.** An embedder must be able to omit proofing entirely or
// supply its own transport, and the only way to be sure of that is for the module
// to have no way to help itself. So `configureProofing` takes:
//
//   `network.fetchAsset(url, { signal, onProgress })`  → `{ bytes, text }`
//   `storage.open()`                                   → a `proof_store.mjs` store
//   `digest(bytes)`                                    → SHA-256 hex
//   `permit({ locale, packId, bytes })`                → the host's download policy
//   `catalogue()`                                      → the manifests on offer
//   `coordinator`                                      → the editor's checker, or null
//
// ## `checkDocument` has no honest implementation today, and says so
//
// This is the part of the surface it would be easiest to fake. The scan is
// WINDOWED and BODY-ONLY by construction (`spell_check.mjs`, load-bearing rule 1:
// the accessibility mirror cost 87% of the time to open a 16,000-paragraph file
// when it projected the whole document, `docs/104` HF-158), and whole-document
// enumeration needs an engine export that does not exist — `hitTest` walks body
// fragments only and footnote bodies have no host-reachable route at all
// (`docs/146` §8's P3 row, `crates/**` work owned by another lane).
//
// A `checkDocument` that resolved to an empty finding list would therefore be a
// function that reports a clean document by not looking at it, which is the exact
// failure `docs/146` §1 forbids and the one Increment A was written to remove. So
// it is a **named refusal with a translatable reason**, and `checkRange` refuses
// the same way for a range outside the scanned window rather than answering
// "nothing wrong there".

import { PACK_REFUSAL, createPackInstaller, packRefusal } from "./proof_packs.mjs";

/** Refusals that belong to the SDK surface rather than to an install.
 *
 *  `messageKey` is a catalogue key, not English: the module that decides must not
 *  own the sentence (`version_policy.mjs` records the same rule), and a refusal
 *  with no sentence is a control that does nothing (SKILL.md §10). */
export const PROOF_SDK_REFUSAL = Object.freeze({
  /** Whole-document checking. See the header: not available, not a no-op. */
  wholeDocument: "whole-document",
  /** The range is not in the page window the coordinator last scanned. */
  outsideWindow: "outside-window",
  /** No coordinator was configured, so there is nothing to check with. */
  notConfigured: "not-configured",
  /** `dispose()` has been called. */
  disposed: "disposed",
});

/** Each SDK refusal's catalogue key. Exported so a guard can assert the map is
 *  total over `PROOF_SDK_REFUSAL`; a code with no key would reach a user as
 *  nothing at all. */
export const PROOF_SDK_REFUSAL_KEYS = Object.freeze({
  [PROOF_SDK_REFUSAL.wholeDocument]: "proofDocument.notAvailable",
  [PROOF_SDK_REFUSAL.outsideWindow]: "proofDocument.outsideWindow",
  [PROOF_SDK_REFUSAL.notConfigured]: "proofDocument.notConfigured",
  [PROOF_SDK_REFUSAL.disposed]: "proofDocument.disposed",
});

function sdkRefusal(code, detail = {}) {
  return Object.freeze({
    ok: false,
    code,
    messageKey: PROOF_SDK_REFUSAL_KEYS[code] ?? "",
    detail: Object.freeze({ ...detail }),
  });
}

/**
 * Builds a proofing session over host-supplied providers.
 *
 * Returns the seven-function surface ADR-042 §6 names, plus `installedPacks`
 * and `rollbackPack` — which are not new capability, they are the install
 * lifecycle's own read and its undo, and hiding them would mean the dialog had
 * to reach past this boundary into the store.
 */
export function configureProofing(options = {}) {
  const {
    network = {},
    storage = {},
    digest = null,
    permit = null,
    catalogue = () => [],
    coordinator = null,
    origin = "",
    allowedOrigins = [],
    now = () => 0,
  } = options;

  /** Listeners for `onFindings`. A Set, so the same listener cannot be
   *  registered twice and `dispose` can drop them all at once. */
  const listeners = new Set();
  let unsubscribeCoordinator = null;
  let store = null;
  let disposed = false;
  /** locale -> AbortController, so an install in flight can be cancelled and a
   *  second install of the same locale cannot race the first. */
  const inFlight = new Map();

  if (coordinator?.subscribe) {
    unsubscribeCoordinator = coordinator.subscribe((findings) => {
      for (const listener of listeners) {
        try {
          listener(findings);
        } catch (error) {
          // A host's listener throwing must not break the checker's own repaint.
          console.warn("proofing listener", error?.message ?? error);
        }
      }
    });
  }

  /** The store, opened once and lazily. Storage can be absent (a private window,
   *  a partitioned third-party iframe — `docs/146` §6), and that is a refusal
   *  with a reason rather than a throw at import time. */
  async function openStore() {
    if (store) return store;
    if (!storage.open) return null;
    try {
      store = await storage.open();
    } catch (error) {
      console.warn("proofing store", error?.message ?? error);
      store = null;
    }
    return store;
  }

  /** The manifest offered for `locale`, or null. `catalogue()` is the host's, so
   *  an embed can publish its own packs without this module knowing how. */
  async function manifestFor(locale) {
    const offered = (await catalogue()) ?? [];
    return offered.find((entry) => entry?.locale === locale) ?? null;
  }

  return {
    /** Whether packs can be kept at all.
     *
     *  A separate question from "is anything installed", and the dialog asks it on
     *  open: storage is genuinely absent in a private window and in a partitioned
     *  third-party frame (`docs/146` §6), and a surface whose Install button fails
     *  for an unexplained reason is worse than one that said so first. */
    async storageAvailable() {
      return (await openStore()) !== null;
    },

    /** Every pack currently active, or `[]` when there is no storage. */
    async installedPacks() {
      const opened = await openStore();
      if (!opened) return [];
      try {
        return await opened.installed();
      } catch (error) {
        console.warn("proofing store", error?.message ?? error);
        return [];
      }
    },

    /** The word list of an active pack — what the coordinator hands the worker. */
    async packWords(packId) {
      const opened = await openStore();
      if (!opened) return [];
      try {
        return await opened.words(packId);
      } catch (error) {
        console.warn("proofing store", error?.message ?? error);
        return [];
      }
    },

    /**
     * Installs the pack published for `locale`, through the eight steps of
     * `docs/146` §6.
     *
     * Returns the installer's own result: `{ ok: true, pack }` or a refusal
     * naming its code and the step it stopped at. Progress arrives on
     * `onProgress`, which is the only reason this takes a callback at all — a
     * download with no byte progress is a spinner, and `docs/146` §3 asks for
     * size, progress and offline state to be explicit.
     */
    async installPack({ locale, manifest = null, onProgress = null } = {}) {
      if (disposed) return sdkRefusal(PROOF_SDK_REFUSAL.disposed);
      const resolved = manifest ?? (await manifestFor(locale));
      if (!resolved) {
        return packRefusal(PACK_REFUSAL.notPublished, "resolve", { locale });
      }
      const opened = await openStore();
      if (!opened) {
        return packRefusal(PACK_REFUSAL.storage, "prepare", { locale });
      }
      if (!network.fetchAsset || !digest) {
        return packRefusal(PACK_REFUSAL.network, "fetch", { locale });
      }
      inFlight.get(locale)?.abort();
      const controller = new AbortController();
      inFlight.set(locale, controller);
      const installer = createPackInstaller({
        permit,
        estimate: network.estimate,
        fetchAsset: network.fetchAsset,
        digest,
        store: {
          stage: (record) => opened.stage(record),
          activate: (record) => opened.activate(record, now()),
          discard: (packId) => opened.discard(packId),
        },
        onStep: (step, info) => onProgress?.(step, info),
      });
      try {
        return await installer.run({
          locale,
          manifest: resolved,
          origin,
          allowedOrigins,
          signal: controller.signal,
        });
      } finally {
        if (inFlight.get(locale) === controller) inFlight.delete(locale);
      }
    },

    /** Cancels an install in flight. Returns whether there was one — a caller
     *  that cannot tell "cancelled" from "there was nothing to cancel" reports
     *  the wrong thing. */
    cancelInstall(locale) {
      const controller = inFlight.get(locale);
      if (!controller) return false;
      controller.abort();
      inFlight.delete(locale);
      return true;
    },

    /** Removes an installed pack. A removal of something absent is a named
     *  refusal, not a quiet success: the dialog must not report a removal that
     *  did not happen. */
    async removePack(packId) {
      if (disposed) return sdkRefusal(PROOF_SDK_REFUSAL.disposed);
      const opened = await openStore();
      if (!opened) return packRefusal(PACK_REFUSAL.storage, "activate", { packId });
      let removed = false;
      try {
        removed = await opened.remove(packId);
      } catch (error) {
        return packRefusal(PACK_REFUSAL.storage, "activate", {
          packId,
          cause: String(error?.message ?? error ?? ""),
        });
      }
      return removed
        ? Object.freeze({ ok: true, packId })
        : packRefusal(PACK_REFUSAL.notInstalled, "resolve", { packId });
    },

    /** Puts a locale back on the version it was on before the last update. The
     *  rollback half of `docs/146` §9's pack lifecycle. */
    async rollbackPack(locale) {
      if (disposed) return sdkRefusal(PROOF_SDK_REFUSAL.disposed);
      const opened = await openStore();
      if (!opened) return packRefusal(PACK_REFUSAL.storage, "activate", { locale });
      const version = await opened.rollback(locale);
      return version
        ? Object.freeze({ ok: true, locale, packVersion: version })
        : packRefusal(PACK_REFUSAL.notInstalled, "resolve", { locale });
    },

    /**
     * The findings for one paragraph range, from the coordinator's current scan.
     *
     * HONEST BY CONSTRUCTION: the coordinator knows about the paragraphs in the
     * page window and no others, so a range it has not scanned gets a refusal
     * naming that, never an empty list. An empty list from this function means
     * "checked, nothing found"; it can never mean "not looked at".
     */
    async checkRange({ node = null } = {}) {
      if (disposed) return sdkRefusal(PROOF_SDK_REFUSAL.disposed);
      if (!coordinator) return sdkRefusal(PROOF_SDK_REFUSAL.notConfigured);
      coordinator.refresh?.();
      const scanned = coordinator.scannedNodes?.() ?? [];
      if (node !== null && !scanned.includes(node)) {
        return sdkRefusal(PROOF_SDK_REFUSAL.outsideWindow, { node });
      }
      return Object.freeze({ ok: true, findings: coordinator.findings?.(node) ?? [] });
    },

    /**
     * Whole-document checking — **not available, and it says which reason.**
     *
     * See the header. This is deliberately not a `checkRange` over every
     * paragraph: the enumeration that would need does not exist, and writing one
     * over `moveCaret` would be an O(document) walk on the main thread, which
     * `docs/107` §4 forbids and which `docs/146` ranks P3 in `crates/**`.
     */
    async checkDocument() {
      if (disposed) return sdkRefusal(PROOF_SDK_REFUSAL.disposed);
      return sdkRefusal(PROOF_SDK_REFUSAL.wholeDocument);
    },

    /** Subscribes to findings. Returns the unsubscribe, so a host never has to
     *  keep the function it passed in order to stop hearing from us. */
    onFindings(listener) {
      if (typeof listener !== "function") return () => {};
      listeners.add(listener);
      return () => listeners.delete(listener);
    },

    /** Releases everything: listeners, the coordinator subscription, any install
     *  in flight, and the database handle. Every later call is a `disposed`
     *  refusal rather than a throw — a host that disposes and then calls is
     *  making a mistake, and a refusal is how it finds out. */
    dispose() {
      disposed = true;
      listeners.clear();
      unsubscribeCoordinator?.();
      unsubscribeCoordinator = null;
      for (const controller of inFlight.values()) controller.abort();
      inFlight.clear();
      store?.close?.();
      store = null;
    },
  };
}
