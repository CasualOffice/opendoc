// Proofing's wiring: the coordinator, the two switches, the replacement, and the
// pack lifecycle (`docs/114`, `docs/146` §3/§6, ADR-042).
//
// Extracted from `main.js` under `109` HF-085 — and, immediately, to pay for
// Increment B's own wiring under the line ratchet. It is a good candidate for the
// same reason `bookmark_manager.mjs` and `object_bar.mjs` were: everything here is
// a verb this module can be HANDED, nothing about proofing is decided in `main.js`
// any more, and what stays there is repainting and review mode.
//
// What this file is, in one line each:
//
//   * the `io` the coordinator asks for, answered from the editor's state;
//   * the two remembered switches, which are preference + reflect + say so;
//   * ONE replacement path — `doc.replaceRanges` through the editor's own
//     `runEdit`, which is what Replace All uses. Proofing adds no second mutation
//     path, now or in a later increment (ADR-005, ADR-030 I2, ADR-042 §5);
//   * the HOST PROVIDERS for `proof_sdk.mjs`: this build's network, this build's
//     storage, this build's digest. They are here rather than inside the SDK
//     because ADR-042 §6 makes them the host's — an embedder supplies its own, and
//     a module that reached for `fetch` could not be given one.
//
// ## Why the providers are three tiny functions and not a class
//
// Each of them is the ONE place a browser API is touched, and each is touched
// once. `fetch` + `arrayBuffer` for the bytes, `crypto.subtle.digest` for the
// checksum, `indexedDB` for the store. Everything that decides anything about them
// is in `proof_packs.mjs`, which is pure and has no way to reach them — which is
// what lets a test drive a corrupted download, an over-quota origin and a refused
// host in node, with no browser and no fixture (`docs/146` §9's pack-lifecycle
// list).

import { openProofStore } from "./proof_store.mjs";
import { configureProofing } from "./proof_sdk.mjs";
import { createProofLanguages } from "./proof_languages.mjs";
import { createSpellChecker } from "./spell_check.mjs";
import { SPELL_LANGUAGES } from "./spelling.mjs";
import { t } from "./i18n.mjs";

/** Bytes → lower-case SHA-256 hex.
 *
 *  `docs/146` §6 step 4 verifies the digest before parsing, and this is the only
 *  place in the feature that touches a crypto API. It is INJECTED into the
 *  installer rather than reached for there, for two reasons that both matter: a
 *  pure module cannot hold it (`BROWSER_GLOBALS`), and a test cannot drive a
 *  checksum MISMATCH without corrupting a real file unless it can substitute this.
 */
async function sha256Hex(bytes) {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

/**
 * Builds everything proofing needs and hands `main.js` back four bindings.
 *
 * `io` is its entire contact with the editor:
 *
 *   `getDoc()`            the open document, or null
 *   `pages()`             the page records array (`page.overlay` is the seam)
 *   `pageWindow()`        `{ first, last }` — the materialized page window
 *   `place(flat, kind)`   puts one twip rect on its page's overlay
 *   `caret()`             `{ node, offset }` or null
 *   `preference(key)`     one remembered setting
 *   `setPreference(k, v)` writes one and persists it
 *   `toggles()`           `{ spell, grammar }` — the Settings checkboxes, or null
 *   `status(text, kind)`  the status line
 *   `repaint()`           repaint the page overlays
 *   `redraw()`            redraw the selection chrome
 *   `reflect()`           "state changed, re-reflect every surface"
 *   `openWords()`         resolves the personal-dictionary store
 *   `runEdit(thunk, o)`   the editor's one undoable-edit path
 *   `registerModal(el,o)` the modal registry
 *   `fallbackFocus()`     where focus goes when a dialog's opener has gone
 *   `activeLocale()`      the reader's locale, for language names
 *   `indexedDB`           the storage factory, injected (ADR-042 §6)
 *   `stamp`               the build's `?v=` query, so a pack is not served from a
 *                         four-hour cache paired with a newer build
 */
export function createProofingChrome(io) {
  const checker = createSpellChecker({
    getDoc: io.getDoc,
    windowPages: () => {
      const window_ = io.pageWindow();
      const all = io.pages();
      const inWindow = [];
      for (let i = window_.first; i <= window_.last; i += 1) {
        const page = all[i];
        if (page?.overlay) inWindow.push(page);
      }
      return inWindow;
    },
    place: io.place,
    caret: io.caret,
    enabled: () => io.preference("spellCheck") !== false,
    grammarEnabled: () => io.preference("grammarCheck") !== false,
    defaultLanguage: () => io.preference("spellLanguage") || "en-US",
    status: io.status,
    repaint: io.repaint,
    openWords: io.openWords,
  });

  /** A pack asset's URL: relative in the manifest, resolved against this module
   *  and re-stamped with the build's own query.
   *
   *  The stamping is the same trick the dictionary fetch uses and it is here for
   *  the same measured reason: a URL resolved against `import.meta.url` drops the
   *  `?v=<build>` the import map put on this module, so the asset would be served
   *  from a fixed URL with a four-hour cache and a deploy could pair a new build
   *  with an old pack. The DIGEST would then refuse the install — correctly, and
   *  confusingly, because nothing would be wrong with either file. */
  function packUrl(url) {
    if (/^https?:/.test(url)) return url;
    return `${new URL(`../${url}`, import.meta.url)}${io.stamp ?? ""}`;
  }

  const sdk = configureProofing({
    network: {
      async fetchAsset(url, { signal } = {}) {
        const response = await fetch(packUrl(url), { signal });
        if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
        const buffer = await response.arrayBuffer();
        return { bytes: new Uint8Array(buffer), text: new TextDecoder().decode(buffer) };
      },
      async estimate() {
        // `StorageManager.estimate()` is not everywhere, and where it is it is an
        // ESTIMATE (`docs/146` §6). A missing one is not a refusal: `quotaPlan`
        // treats "unmeasured" and "measured and fits" differently from "measured
        // and does not fit", so an origin that will not say has not said no.
        try {
          return (await navigator.storage?.estimate?.()) ?? null;
        } catch {
          return null;
        }
      },
    },
    storage: { open: () => openProofStore({ indexedDB: io.indexedDB }) },
    digest: sha256Hex,
    // This build's own policy: it serves its own packs from its own origin, so it
    // permits them. An EMBEDDER replaces this function, which is the whole point of
    // the seam — `docs/146` §6 puts the download decision with the host, and a host
    // that forbids network access entirely refuses here and gets a named refusal
    // in the dialog rather than a failed fetch.
    permit: () => true,
    catalogue: () => fetchCatalogue(),
    coordinator: checker,
    origin: io.origin?.() ?? "",
    now: () => Date.now(),
  });

  /** Hands the checker a pack's words, or takes them away. ONE function for both
   *  directions, so the two cannot disagree about what has to happen either way:
   *  the supplement tier changes AND the active pack version changes, and the
   *  second is what stops the cache serving pre-install answers. */
  function applyPack(record, locale = "") {
    if (record) checker.setPack(record);
    else checker.removePack(locale);
    io.repaint?.();
  }

  const languages = createProofLanguages({
    sdk,
    catalogue: () => fetchCatalogue(),
    baseLanguages: () => SPELL_LANGUAGES,
    activeLocale: io.activeLocale,
    onPackChange: applyPack,
    status: io.status,
    registerModal: io.registerModal,
    fallbackFocus: io.fallbackFocus,
  });

  /** The catalogue, fetched once. Held here rather than in the dialog so a future
   *  second surface (a first-use prompt, `docs/146` §3's setup state machine) reads
   *  the same list rather than fetching a second copy. */
  let catalogueOnce = null;
  function fetchCatalogue() {
    catalogueOnce ??= (async () => {
      const response = await fetch(packUrl("packs/index.json"));
      if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
      return (await response.json()).packs ?? [];
    })();
    return catalogueOnce;
  }

  /**
   * Re-installs what is already stored, at boot.
   *
   * Without this an installed pack would be inert until the dialog was opened,
   * which is "offline reopen" from `docs/146` §9's lifecycle list — the case that
   * matters most, because a reader who installed a pack on a train expects it to
   * still be there when they open the file with no network. Failure is not fatal:
   * proofing runs on the basic tier, which is what it did before any pack existed.
   */
  async function loadInstalledPacks() {
    try {
      for (const pack of await sdk.installedPacks()) {
        const words = await sdk.packWords(pack.packId);
        if (words.length) checker.setPack({ ...pack, words });
      }
    } catch (error) {
      console.warn("proofing packs", error?.message ?? error);
    }
  }
  void loadInstalledPacks();

  return {
    checker,
    sdk,

    /** Replaces a flagged word with a suggestion through the SAME path Replace All
     *  uses — one undoable action, closed in Viewing, tracked in Suggesting. */
    replaceMisspelling(flagged, word) {
      const doc = io.getDoc();
      if (!doc) return;
      void io.runEdit(
        () => doc.replaceRanges([flagged.node], [flagged.start], [flagged.node], [flagged.end], word),
        { gate: true },
      );
    },

    /** Spelling on or off, remembered with the other preferences. */
    setSpellCheckEnabled(enabled) {
      io.setPreference("spellCheck", enabled);
      const toggle = io.toggles?.().spell;
      if (toggle) toggle.checked = enabled;
      checker.setEnabled(enabled);
      io.status(enabled ? t("proofing.spellCheckOn") : t("proofing.spellCheckOff"));
      io.reflect();
    },

    /** Grammar on or off, remembered beside spelling and independent of it. */
    setGrammarCheckEnabled(enabled) {
      io.setPreference("grammarCheck", enabled);
      const toggle = io.toggles?.().grammar;
      if (toggle) toggle.checked = enabled;
      checker.refresh();
      io.redraw();
      io.status(enabled ? t("proofing.grammarCheckOn") : t("proofing.grammarCheckOff"));
      io.reflect();
    },

    /** Opens "Proofing languages". Reachable from the Review band, the Review
     *  menu and the command palette — three surfaces, because one is the recurring
     *  defect this repository keeps finding (`docs/105` UX-004). */
    openLanguages() {
      void languages.open();
    },

    /** Test/inspection seam: the dialog controller. */
    languages,
  };
}
