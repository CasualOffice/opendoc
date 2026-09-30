// "Proofing languages" — the install surface for a language pack.
//
// `docs/146` §3 asks for "explicit first-use installation with progress, size,
// offline state, and a removal control", and §10 names the screen: *Manage
// languages*. This is that screen, and — per SKILL.md §8 and the editing-standard
// rule — the shape was taken from the competition before any of it was written,
// not from what the SDK happened to make easy.
//
// ## The competitive standard this is built from
//
// **Word — File ▸ Options ▸ Language.** A table, one row per authoring language,
// with a column for the language and a column headed *Proofing (Spelling,
// Grammar…)* whose CELL IS THE AFFORDANCE: it reads "Installed" where the
// proofing tools are present and is a link reading "Not installed" where they are
// not, which takes you to the language accessory pack's download. Word's second,
// narrower surface is Review ▸ Language ▸ Set Proofing Language…, which lists
// languages with a small spell-check mark against the ones that have proofing
// tools — the same fact, shown per selection.
//
// **Google Docs — Tools ▸ Spelling and grammar.** The switches plus *Personal
// dictionary…*; the document's language is File ▸ Language. Docs proofs on the
// server, so it has no installable pack at all: what it contributes to this design
// is where the proofing SWITCHES live (they already live there, in this editor's
// Review band) and nothing about installation.
//
// **So the shape is Word's Options ▸ Language table**, deliberately:
//
//   * one row per language, never a queue or a download manager — there is at most
//     one pack per language, and a queue would be chrome for a case that does not
//     exist;
//   * the STATE is a column and reads as a fact ("Built in", "Installed", "Not
//     installed", "Installing 42%"), because the first question is "does this
//     language get checked" and not "what can I click";
//   * the SIZE is its own column, before the download rather than during it.
//     Word's accessory-pack page leads with the size and `docs/146` §3 asks for it
//     to be explicit; a progress bar that is the first mention of 40 MB is a
//     download nobody agreed to;
//   * one action per row, whose label is the verb for the state it is in.
//
// **Where this deliberately differs from Word, and why** (SKILL.md §8: say so in
// the code rather than leaving it ambiguous):
//
//   * Word's "Not installed" link leaves the application for a web page and a
//     separate installer. We install in place, with byte progress and a cancel,
//     because there is no second application to run and local-first is the point.
//   * Word's table also carries a Keyboard Layout column. That is the operating
//     system's business, we cannot change it, and a column that only ever reports
//     is a column that only ever confuses.
//   * A language with no pack published shows its action DISABLED WITH THE REASON
//     rather than hidden (SKILL.md §10, never a dead control). Word hides nothing
//     here either, but it also never has to say "there is no pack for this yet",
//     and that sentence is the honest one for a build that publishes exactly one.
//
// ## Every refusal has a sentence
//
// `proof_packs.mjs` returns a CODE. The map below turns each into a catalogue key,
// and `tests/proof_languages.test.mjs` asserts the map is total over
// `PACK_REFUSAL_CODES` — because a refusal with no sentence is a dialog that
// closes having said nothing, which is the failure SKILL.md §10 names.

import { t } from "./i18n.mjs";
import { PACK_REFUSAL, PACK_REFUSAL_CODES, PACK_SIZE_CEILING_BYTES } from "./proof_packs.mjs";

/** Every pack refusal's sentence, by code. Total over `PACK_REFUSAL_CODES`, and
 *  guarded to stay that way. */
export const PACK_REFUSAL_KEYS = Object.freeze({
  [PACK_REFUSAL.hostRefused]: "proofPack.refusal.hostRefused",
  [PACK_REFUSAL.manifest]: "proofPack.refusal.manifest",
  [PACK_REFUSAL.schema]: "proofPack.refusal.schema",
  [PACK_REFUSAL.locale]: "proofPack.refusal.locale",
  [PACK_REFUSAL.engineApi]: "proofPack.refusal.schema",
  [PACK_REFUSAL.assetUrl]: "proofPack.refusal.assetUrl",
  [PACK_REFUSAL.tooLarge]: "proofPack.refusal.tooLarge",
  [PACK_REFUSAL.quota]: "proofPack.refusal.quota",
  [PACK_REFUSAL.network]: "proofPack.refusal.network",
  [PACK_REFUSAL.cancelled]: "proofPack.refusal.cancelled",
  [PACK_REFUSAL.digest]: "proofPack.refusal.digest",
  [PACK_REFUSAL.bounds]: "proofPack.refusal.bounds",
  [PACK_REFUSAL.format]: "proofPack.refusal.format",
  [PACK_REFUSAL.selfTest]: "proofPack.refusal.selfTest",
  [PACK_REFUSAL.storage]: "proofPack.refusal.storage",
  [PACK_REFUSAL.notPublished]: "proofPack.refusal.notPublished",
  [PACK_REFUSAL.notInstalled]: "proofPack.refusal.notInstalled",
});

/** Which refusal codes have no sentence. Exported so the guard reports the gap
 *  rather than only failing, and so this file cannot be the thing that has to be
 *  read to find out. */
export function refusalKeyGaps() {
  return PACK_REFUSAL_CODES.filter((code) => !PACK_REFUSAL_KEYS[code]);
}

/** A refusal's translated sentence. A code with no entry still says SOMETHING —
 *  the code — because an unexplained failure is worse than an ugly one. */
export function refusalSentence(refusal) {
  const key = PACK_REFUSAL_KEYS[refusal?.code] ?? refusal?.messageKey ?? "";
  if (!key) return String(refusal?.code ?? "");
  return t(key, {
    limit: Math.round(PACK_SIZE_CEILING_BYTES / (1024 * 1024)),
    ...(refusal?.detail ?? {}),
  });
}

/** Megabytes, to one decimal, as a NUMBER for the catalogue to format. A pack is
 *  measured in bytes and read in megabytes, and the rounding is here rather than
 *  in the sentence so every locale rounds the same way. */
export function megabytes(bytes) {
  return Math.max(0.1, Math.round((Number(bytes) || 0) / (1024 * 1024) * 10) / 10);
}

/**
 * A language's display name in the reader's own language.
 *
 * `Intl.DisplayNames` rather than 18 catalogues × N languages of hand-written
 * names: the platform already knows that `en-GB` is "British English" in English
 * and "englisch (Vereinigtes Königreich)" in German, and a hand-maintained table
 * of language names is a hand-maintained list — which this repository has already
 * learned to derive rather than keep (SKILL.md §8).
 *
 * Falls back to the tag, which is what an old engine or a locale `Intl` does not
 * know produces. A tag is a poor label; a blank cell is a bug.
 */
export function languageName(tag, locale) {
  try {
    return new Intl.DisplayNames([locale || "en"], { type: "language" }).of(tag) || tag;
  } catch {
    return tag;
  }
}

/**
 * Builds the dialog over the markup already in `editor.html`.
 *
 * It ADOPTS the markup rather than generating it, the rule `modal.mjs`'s header
 * states: the dialog is authored with its real labels, `aria-labelledby` and
 * `aria-describedby` wiring, and regenerating that from script would trade a
 * solved accessibility problem for an unsolved one.
 *
 * `io` is the whole of its contact with the application:
 *
 *   `sdk`                   the `proof_sdk.mjs` session
 *   `catalogue()`           the manifests on offer, as a promise
 *   `baseLanguages()`       the locales the BUILD already checks (the basic tier)
 *   `activeLocale()`        the reader's own locale, for language names
 *   `onPackChange(record)`  a pack became active (`record`) or was removed (null)
 *   `status(text, kind)`    the status line
 *   `registerModal(el, o)`  the host's modal registry
 *   `fallbackFocus()`       where focus goes when the opener has gone away
 */
export function createProofLanguages(io) {
  const dialog = document.getElementById("proofLanguagesDialog");
  if (!dialog) return { open: () => {}, close: () => {}, isOpen: () => false };
  const rowsBody = document.getElementById("proofLanguageRows");
  const note = document.getElementById("proofLanguagesNote");
  const closeBtn = document.getElementById("proofLanguagesClose");
  const doneBtn = document.getElementById("proofLanguagesDone");

  const modal = io.registerModal(dialog, {
    initialFocus: () => rowsBody?.querySelector("button:not(:disabled)") ?? doneBtn,
    fallbackFocus: io.fallbackFocus,
  });

  /** locale → the last progress fraction reported, while an install is running.
   *  Absent means no install is running for that locale. */
  const progress = new Map();
  /** The manifests on offer, once fetched. `null` until the first open, so the
   *  network is not touched by a dialog nobody opened. */
  let offered = null;

  /** Says something in the dialog AND on the status line.
   *
   *  Both, deliberately: the dialog's own note is where a reader looks after
   *  pressing a button in it, and the status line is what a screen reader
   *  announces and what survives the dialog being closed. A refusal that is only
   *  in a dialog the user then dismisses was never delivered. */
  function say(text, kind = "") {
    if (note) {
      note.textContent = text;
      note.classList.toggle("error", kind === "error");
    }
    io.status?.(text, kind);
  }

  /** The manifest offered for `locale`, or null. */
  function manifestFor(locale) {
    return (offered ?? []).find((entry) => entry?.locale === locale) ?? null;
  }

  /** One row of Word's table: the language, its proofing state, its size, and the
   *  one action that state admits. */
  function buildRow(locale, installed) {
    const manifest = manifestFor(locale);
    const running = progress.get(locale);
    const row = document.createElement("tr");
    row.className = "proof-lang-row";
    row.dataset.locale = locale;

    const name = document.createElement("th");
    name.scope = "row";
    name.className = "proof-lang-name";
    name.textContent = languageName(locale, io.activeLocale?.() ?? "en");
    row.append(name);

    const state = document.createElement("td");
    state.className = "proof-lang-state";
    if (running !== undefined) {
      state.textContent = t("proofLanguages.installing", { percent: Math.round(running * 100) });
    } else if (installed) {
      // The state and the EVIDENCE for it in one cell. "Installed" alone is a
      // claim; "Installed · 165 extra words" is the claim plus the number that
      // says the pack did something, and the number is derived from what was
      // actually stored rather than from the manifest that asked to be believed.
      state.textContent = t("proofLanguages.installedWords", { count: installed.words });
    } else if (manifest) {
      state.textContent = t("proofLanguages.notInstalled");
    } else {
      // The basic tier is not nothing, and a row that said only "Not installed"
      // about a language this build already checks would be false in the
      // direction that matters (SKILL.md §9: understating is also false).
      state.textContent = t("proofLanguages.builtIn");
    }
    row.append(state);

    const size = document.createElement("td");
    size.className = "proof-lang-size";
    const bytes = installed?.bytes ?? manifest?.assets?.reduce((sum, a) => sum + a.bytes, 0) ?? 0;
    size.textContent = bytes ? t("proofLanguages.megabytes", { size: megabytes(bytes) }) : "—";
    row.append(size);

    const actions = document.createElement("td");
    actions.className = "proof-lang-actions";
    const button = document.createElement("button");
    button.type = "button";
    button.className = "dialog-button proof-lang-action";
    if (running !== undefined) {
      button.textContent = t("proofLanguages.cancel");
      button.setAttribute("aria-label", t("proofLanguages.cancelLanguage", { language: name.textContent }));
      button.addEventListener("click", () => cancel(locale));
    } else if (installed) {
      button.textContent = t("proofLanguages.remove");
      button.setAttribute("aria-label", t("proofLanguages.removeLanguage", { language: name.textContent }));
      button.addEventListener("click", () => void remove(locale, installed.packId, name.textContent));
    } else {
      button.textContent = t("proofLanguages.install");
      button.setAttribute("aria-label", t("proofLanguages.installLanguage", { language: name.textContent }));
      if (!manifest) {
        // DISABLED WITH A REASON, never absent. The reason is the same sentence
        // the SDK's own refusal would produce, so the two surfaces cannot say
        // different things about one fact.
        button.disabled = true;
        button.title = t("proofPack.refusal.notPublished");
      } else {
        button.addEventListener("click", () => void install(locale, manifest, name.textContent));
      }
    }
    actions.append(button);
    row.append(actions);
    return row;
  }

  /** Repaints the table from what is actually installed. Called on open, after
   *  every install, removal and progress tick. O(languages). */
  async function refresh() {
    if (!rowsBody) return;
    const installed = await io.sdk.installedPacks();
    const byLocale = new Map(installed.map((pack) => [pack.locale, pack]));
    const locales = new Set([
      ...(io.baseLanguages?.() ?? []),
      ...(offered ?? []).map((entry) => entry.locale),
      ...byLocale.keys(),
    ]);
    rowsBody.replaceChildren();
    for (const locale of [...locales].sort()) {
      rowsBody.append(buildRow(locale, byLocale.get(locale) ?? null));
    }
  }

  async function install(locale, manifest, label) {
    progress.set(locale, 0);
    await refresh();
    const result = await io.sdk.installPack({
      locale,
      manifest,
      onProgress: (step, info) => {
        if (step !== "fetch" || !info?.total) return;
        progress.set(locale, Math.min(1, (info.loaded ?? 0) / info.total));
        void refresh();
      },
    });
    progress.delete(locale);
    if (!result.ok) {
      await refresh();
      say(refusalSentence(result), "error");
      return;
    }
    const words = await io.sdk.packWords(result.pack.packId);
    io.onPackChange?.({ ...result.pack, words });
    await refresh();
    say(t("proofLanguages.installedLanguage", { language: label }));
  }

  async function remove(locale, packId, label) {
    const result = await io.sdk.removePack(packId);
    if (!result.ok) {
      say(refusalSentence(result), "error");
      return;
    }
    io.onPackChange?.(null, locale);
    await refresh();
    say(t("proofLanguages.removedLanguage", { language: label }));
  }

  function cancel(locale) {
    // `cancelInstall` answers whether there was one. Reporting "cancelled" for an
    // install that had already finished would be a sentence about nothing.
    if (!io.sdk.cancelInstall(locale)) return;
    progress.delete(locale);
    void refresh();
    say(t("proofPack.refusal.cancelled"));
  }

  closeBtn?.addEventListener("click", () => modal.close("close"));
  doneBtn?.addEventListener("click", () => modal.close("done"));

  return {
    /** Opens the dialog, fetching the catalogue the first time only.
     *
     *  Every string is resolved HERE rather than in the factory body, which is the
     *  defect `dialog-i18n-keys.spec.mjs` exists for: a catalogue arrives
     *  asynchronously, and a row built at import time is stamped with the literal
     *  key and never rebuilt. */
    async open() {
      if (offered === null) {
        try {
          offered = (await io.catalogue?.()) ?? [];
        } catch (error) {
          offered = [];
          console.warn("proofing catalogue", error?.message ?? error);
        }
      }
      await refresh();
      if (note) {
        note.textContent = "";
        note.classList.remove("error");
      }
      // Storage can be absent — a private window, a partitioned third-party frame
      // (`docs/146` §6). Saying so on open is the difference between a dialog whose
      // Install button fails for an unexplained reason and one that told you first.
      if (!(await io.sdk.storageAvailable())) {
        say(t("proofLanguages.storageUnavailable"), "error");
      }
      modal.open();
    },
    close() {
      modal.close("close");
    },
    isOpen: () => modal.isOpen,
    /** Test/inspection seam: repaint from storage without opening. */
    refresh,
  };
}
