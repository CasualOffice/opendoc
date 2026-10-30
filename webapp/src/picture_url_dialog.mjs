// SPDX-License-Identifier: Apache-2.0
// Insert ▸ Picture from URL: the dialog (`picture_url.mjs` is the fetch).
//
// The interaction is Google Docs' Image ▸ By URL and ONLYOFFICE's Image ▸ From
// URL, which agree: one address field, an Insert button, and the picture lands
// at the insertion point. Two things are added because neither of them is a
// nicety once the download is real. PROGRESS, because a 15 MB photo over a slow
// link is many seconds of nothing otherwise; and CANCEL that really cancels —
// Cancel, Escape, the close button and the backdrop all abort the request
// through one `AbortController`, because every route out of a modal converges
// on `onClose` (the same staging `name_dialog.mjs` explains).
//
// A refusal keeps the dialog open with the reason under the field, so the
// person can fix a typo instead of retyping the address. Success closes it and
// inserts through `insertImageFromBlob`, the path a picked file takes — one
// list of formats, one decode, one undoable `insertImage`, the picture left
// selected (OL-03).
import { modalIsOpen, registerModal } from "./modal.mjs";
import { hostConfig } from "./capabilities.mjs";
import { t } from "./i18n.mjs";
import {
  PictureUrlError,
  fetchPictureFromUrl,
  resolveRemoteImagePolicy,
  validatePictureUrl,
} from "./picture_url.mjs";

/** One literal catalogue key per refusal code, so every key is visible to the
 *  string tooling (a template-built key is not). */
export const PICTURE_URL_ERROR_KEYS = Object.freeze({
  invalid: "pictureUrl.error.invalid",
  credentials: "pictureUrl.error.credentials",
  denied: "pictureUrl.error.denied",
  network: "pictureUrl.error.network",
  http: "pictureUrl.error.http",
  notImage: "pictureUrl.error.notImage",
  unsupported: "pictureUrl.error.unsupported",
  tooLarge: "pictureUrl.error.tooLarge",
  empty: "pictureUrl.error.empty",
  aborted: "pictureUrl.error.aborted",
});

/**
 * Wires the dialog and returns `{ open }`.
 *
 * @param {object} ed the editor context (`docs/177`): `doc`, `selection`,
 *        `reviewMode`, `blockMutationInViewing()`, `blockUntrackedInSuggesting()`,
 *        `insertImageFromBlob(blob)`, `setStatus(text, kind)`, `pagesEl`.
 * @param {object} [io] seams: `fetchImpl` (the network), `policy` (overrides the
 *        host's `?remoteImages=`).
 */
export function createPictureUrlDialog(ed, io = {}) {
  const dialog = document.getElementById("pictureUrlDialog");
  if (!dialog) return { open() {} };
  const input = document.getElementById("pictureUrlInput");
  const status = document.getElementById("pictureUrlStatus");
  const progress = document.getElementById("pictureUrlProgress");
  const insertBtn = document.getElementById("pictureUrlInsert");
  const cancelBtn = document.getElementById("pictureUrlCancel");
  const closeBtn = document.getElementById("pictureUrlClose");
  const policy = io.policy ?? resolveRemoteImagePolicy(hostConfig().remoteImages);
  /** The download in flight, or null. */
  let inFlight = null;

  const controller = registerModal(dialog, {
    initialFocus: () => input,
    fallbackFocus: () => ed.pagesEl,
    defaultAction: () => void submit(),
    onClose: () => {
      inFlight?.abort();
      inFlight = null;
      setBusy(false);
    },
  });

  function say(text, kind = "") {
    status.textContent = text;
    status.dataset.kind = kind;
  }

  function setBusy(busy) {
    input.disabled = busy;
    insertBtn.disabled = busy;
    progress.hidden = !busy;
    if (busy) progress.removeAttribute("value");
  }

  function sentenceFor(err) {
    const code = err instanceof PictureUrlError ? err.code : "network";
    return t(PICTURE_URL_ERROR_KEYS[code] ?? PICTURE_URL_ERROR_KEYS.network, { status: String(err?.detail?.status ?? "") });
  }

  async function submit() {
    if (inFlight) return;
    const checked = validatePictureUrl(input.value);
    if (!checked.ok) {
      say(t(PICTURE_URL_ERROR_KEYS[checked.code]), "error");
      input.focus();
      return;
    }
    const abort = new AbortController();
    inFlight = abort;
    setBusy(true);
    say(t("pictureUrl.downloading"));
    let fetched;
    try {
      fetched = await fetchPictureFromUrl(checked.url, {
        policy,
        fetchImpl: io.fetchImpl,
        signal: abort.signal,
        onProgress: (received, total) => {
          if (total) progress.value = Math.min(1, received / total);
          say(t("pictureUrl.received", { size: formatSize(received) }));
        },
      });
    } catch (err) {
      if (inFlight !== abort) return; // closed meanwhile: nobody to tell
      inFlight = null;
      setBusy(false);
      say(sentenceFor(err), "error");
      input.focus();
      return;
    }
    if (inFlight !== abort) return;
    inFlight = null;
    controller.close("inserted");
    await ed.insertImageFromBlob(new Blob([fetched.bytes], { type: fetched.mime }));
  }

  insertBtn.addEventListener("click", () => void submit());
  cancelBtn.addEventListener("click", () => controller.close("cancel"));
  closeBtn.addEventListener("click", () => controller.close("cancel"));

  return {
    /** Opens the dialog for the insertion point. Refuses, saying why, in
     *  Viewing, in Suggesting (a picture insert cannot be tracked yet), and when
     *  the host's policy forbids downloads — before asking for an address the
     *  editor would then refuse. O(1). */
    open() {
      if (!ed.doc || !ed.selection || modalIsOpen()) return;
      if (ed.blockMutationInViewing()) return;
      if (ed.blockUntrackedInSuggesting()) return;
      if (policy.mode === "deny") {
        ed.setStatus(t(PICTURE_URL_ERROR_KEYS.denied), "error");
        return;
      }
      setBusy(false);
      say("");
      progress.value = 0;
      controller.open();
    },
  };
}

/** "1.2 MB" / "340 KB" — sizes as a file manager writes them. */
function formatSize(bytes) {
  if (bytes >= 1024 * 1024) return t("pictureUrl.sizeMb", { size: (bytes / (1024 * 1024)).toFixed(1) });
  return t("pictureUrl.sizeKb", { size: String(Math.max(1, Math.round(bytes / 1024))) });
}
