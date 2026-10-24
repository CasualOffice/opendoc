// The room's half of the Version history panel (`docs/175` §11, `docs/109` HF-362).
//
// `version_panel.mjs` owns the timeline: one row renderer, one row menu, one preview, one
// keyboard contract. This module owns what only a room has — the shared grid's chrome,
// the pager behind it, saving a version for everyone, a shared row's menu and its bytes —
// and reaches the panel only through the callbacks it is handed. Split out so the panel
// stays under the source-file ceiling (`docs/106` standing rule 1) and so the room's
// rules sit in one place:
//
//   * the shared grid lists the ROOM'S versions, a page at a time, and a provider failure
//     is said (status channel and panel), never an empty list;
//   * "Save a version for everyone" sends the live document's `collabSnapshot()` package
//     and NO author (`shared_versions.mjs` rule 1);
//   * a shared row can be viewed, compared, copied and downloaded; Restore is refused with
//     the ADR-068 reason until HF-314, and rename, keep and delete are the host's.
//
// No English here except through the catalogue.
import { t } from "./i18n.mjs";
import { sanitiseVersionName } from "./version_history.mjs";
import { HISTORY_SCOPE, createSharedPager } from "./shared_versions.mjs";

/**
 * @param {object} deps
 * @param {() => {scope: string, source: object|null, current: (() => {revision: number, bytes: Uint8Array})|null}} deps.shared
 * @param {(text: string, kind?: string) => void} deps.publish the status channel.
 * @param {(result: object) => void} deps.report a store status, said.
 * @param {(capability: string) => boolean} deps.allows the host's grants.
 * @param {() => object} deps.documentInfo `{hasDocument, …}`.
 * @param {(current: string) => Promise<string|null>} deps.promptName the shared-worded name prompt.
 * @param {() => string} deps.restoreRefusal ADR-068's sentence while in a room.
 * @param {() => boolean} deps.isOpen whether the panel is showing.
 * @param {() => void} deps.repaint re-renders the timeline, keeping the keyboard in it.
 * @param {() => Promise<void>} deps.closePreview back to the live document.
 * @param {(versionId: string) => void} deps.select selects a row.
 * @param {() => boolean} deps.canRedline whether Show changes is wired.
 */
export function createSharedHistory({
  shared,
  publish,
  report,
  allows,
  documentInfo,
  promptName,
  restoreRefusal,
  isOpen,
  repaint,
  closePreview,
  select,
  canRedline,
}) {
  const section = document.getElementById("versionSharedSection");
  const heading = document.getElementById("versionSharedHeading");
  const list = document.getElementById("versionSharedList");
  const statusEl = document.getElementById("versionSharedStatus");
  const moreBtn = document.getElementById("versionSharedMore");
  const saveBtn = document.getElementById("versionSaveShared");
  const deviceHeading = document.getElementById("versionDeviceHeading");

  /** The room's rows, a page at a time, or null outside the shared scope. */
  let pager = null;
  let saving = false;

  /** The timeline's scope now. Read each time: a room opens and closes without the
   *  panel being rebuilt. O(1). */
  function scope() {
    return shared()?.scope ?? HISTORY_SCOPE.STANDALONE;
  }

  /** The bound source, in the shared scope only. O(1). */
  function source() {
    const now = shared();
    return now?.scope === HISTORY_SCOPE.SHARED ? now.source ?? null : null;
  }

  /** The pager for the room in force: kept while the room is the same, replaced (and the
   *  old read cancelled) when it changes, dropped outside the shared scope. O(1). */
  function sync() {
    const bound = source();
    if (!bound) {
      pager?.cancel();
      pager = null;
      return null;
    }
    if (!pager || pager.room !== bound.room) {
      pager?.cancel();
      pager = createSharedPager(bound);
    }
    return pager;
  }

  /**
   * Reads a page — the newest, or the next older — and repaints when it lands. NOT
   * awaited by the panel's open: a slow host must not hold the keyboard out of the
   * panel, so the grid says "Loading" until the page arrives. O(page).
   */
  function load(which = "first") {
    const current = sync();
    if (!current) return Promise.resolve(false);
    const read = which === "older" ? current.older() : current.first();
    if (isOpen()) repaint();
    return read.then((changed) => {
      if (!changed || current !== pager) return false;
      if (current.error) publish(t("versionHistory.shared.listFailed", { message: current.error }), "error");
      if (isOpen()) repaint();
      return true;
    });
  }

  /** The room's rows held, newest first; none outside the shared scope. */
  function rows() {
    return scope() === HISTORY_SCOPE.SHARED && pager ? pager.rows : [];
  }

  /** The room's version immediately older than `versionId` among those held. O(rows). */
  function predecessorOf(versionId) {
    const held = pager?.rows ?? [];
    const index = held.findIndex((candidate) => candidate.versionId === versionId);
    return index < 0 ? null : held[index + 1] ?? null;
  }

  /** What the shared grid says when it has no rows to show. O(1). */
  function emptyText() {
    if (pager?.loading) return t("versionHistory.shared.loading");
    if (pager?.error) return t("versionHistory.shared.unavailable");
    return t("versionHistory.shared.empty");
  }

  /** Shows or hides the shared section and sets the two headings. The device grid's is
   *  "On this device" beside the room's versions, "Versions on this device only" in a
   *  room whose host keeps none, and nothing standalone, where the grid keeps its own
   *  label. O(1). */
  function paintHeadings(sharedPart, devicePart, deviceGrid) {
    if (section) section.hidden = !sharedPart;
    if (sharedPart && heading) heading.textContent = sharedPart.heading;
    if (!sharedPart) list?.replaceChildren();
    if (deviceHeading) {
      deviceHeading.hidden = !devicePart.heading;
      deviceHeading.textContent = devicePart.heading;
    }
    if (devicePart.heading) deviceGrid.setAttribute("aria-labelledby", "versionDeviceHeading");
    else deviceGrid.removeAttribute("aria-labelledby");
  }

  /** Whether the shared grid is on screen. O(1). */
  function shown() {
    return Boolean(list) && Boolean(section) && !section.hidden;
  }

  /**
   * The shared grid's two controls and its status line. Neither is ever dead: each is
   * disabled with its reason (SKILL §10), and a provider failure is said here as well as
   * on the status channel, because the toast fades and the panel is where the reader is
   * looking for the versions that did not arrive. O(1).
   */
  function reflect() {
    const bound = source();
    const inShared = Boolean(bound) && shown();
    if (statusEl) {
      const said = !inShared ? ""
        : pager?.error ? t("versionHistory.shared.listFailed", { message: pager.error })
          : pager?.full ? t("versionHistory.shared.limit", { count: pager.rows.length })
            : "";
      statusEl.textContent = said;
      statusEl.hidden = !said;
    }
    if (moreBtn) {
      // "Try again" after a failure, "Load older shared versions" while the host says
      // there are more; absent otherwise, because there is nothing it could do.
      const retry = Boolean(pager?.error);
      moreBtn.hidden = !inShared || !(retry || pager?.more);
      if (!moreBtn.hidden) {
        moreBtn.textContent = retry ? t("versionHistory.shared.retry") : t("versionHistory.shared.loadOlder");
        const why = pager?.loading ? t("versionHistory.shared.loading")
          : !retry && pager?.full ? t("versionHistory.shared.limit", { count: pager.rows.length })
            : "";
        disable(moreBtn, why);
      }
    }
    if (saveBtn) {
      saveBtn.hidden = !inShared;
      if (inShared) {
        saveBtn.textContent = t("versionHistory.shared.save");
        disable(saveBtn, saveRefusal() || (saving ? t("versionHistory.shared.saving") : ""));
      }
    }
  }

  /** Disabled with the reason as its tooltip, or enabled with none. O(1). */
  function disable(button, why) {
    button.disabled = Boolean(why);
    if (why) button.title = why;
    else button.removeAttribute("title");
  }

  /** Why "Save a version for everyone" cannot run, or "". O(1). */
  function saveRefusal() {
    const bound = source();
    if (!bound) return t("versionHistory.shared.noRoom");
    if (!bound.canSave) return t("versionHistory.shared.cannotSave");
    if (!documentInfo().hasDocument) return t("versionHistory.disabled.noDocument");
    if (!allows("save")) return t("capability.notGranted");
    return "";
  }

  /**
   * "Save a version for everyone" (`docs/175` §11): the live document's
   * `collabSnapshot()` package, saved through the host with an optional name. NO author
   * is sent — the host attests it from its own session — so the row that comes back reads
   * under the name the host knows, not one this tab typed.
   *
   * Back to the live document first, for restore's reason: while a preview is up the
   * canvas is the preview, and saving it would save an old version as new. O(document)
   * for the package, once, on an explicit act.
   */
  async function save() {
    const refusal = saveRefusal();
    if (refusal) return void publish(refusal, "error");
    if (saving) return void publish(t("versionHistory.shared.saving"), "warn");
    const bound = source();
    const typed = await promptName("");
    if (typed === null) return;
    let name = "";
    if (typed.trim()) {
      const clean = sanitiseVersionName(typed);
      if (!clean.ok) return void report({ status: clean.reason });
      name = clean.name;
    }
    saving = true;
    reflect();
    try {
      await closePreview();
      const current = shared()?.current?.();
      if (!current?.bytes?.length) throw new Error(t("versionHistory.shared.nothingToSave"));
      const saved = await bound.save({ revision: current.revision, name, bytes: current.bytes });
      publish(name ? t("versionHistory.shared.savedNamed", { name }) : t("versionHistory.shared.saved"), "");
      if (isOpen()) {
        await load("first");
        select(`shared:${saved.id}`);
      }
    } catch (err) {
      publish(t("versionHistory.shared.saveFailed", { message: String(err?.message ?? err) }), "error");
    } finally {
      saving = false;
      reflect();
    }
  }

  /** A shared version's `collabSnapshot()` package from the host's `versionBytes`, as
   *  `{ok: true, bytes}` or `{ok: false, message}`; never thrown. O(version bytes). */
  async function bytes(row) {
    const bound = source();
    if (!bound) return { ok: false, message: t("versionHistory.shared.left") };
    try {
      const got = await bound.bytes({ id: row.sharedId });
      return { ok: true, bytes: got.bytes };
    } catch (err) {
      return { ok: false, message: t("versionHistory.shared.readFailed", { message: String(err?.message ?? err) }) };
    }
  }

  /**
   * The actions for one of the ROOM'S versions: view, compare, copy and download, as for
   * a device version; Restore refused with the reason until HF-314 (ADR-068); and the
   * three that would change what the host keeps — rename, keep, delete — disabled with
   * theirs, because the provider offers none of them. Naming happens at save. The same
   * seven rows in the same order as a device row, so a reader's hand learns one menu.
   * O(rows held), for the predecessor.
   */
  function menuEntries(row, { keys, makeCopy, download, showChanges }) {
    const refusal = restoreRefusal() || t("versionHistory.restore.sharedRefused");
    const hostKeeps = t("versionHistory.shared.hostKeeps");
    const older = predecessorOf(row.versionId);
    return [
      { id: "version.restore", group: "restore", label: t("versionPanel.restoreThisVersion"),
        enabled: false, disabledReason: refusal, run: () => publish(refusal, "warn") },
      { id: "version.copy", group: "copy", label: t("versionPanel.makeACopy"),
        enabled: allows("open"), disabledReason: t("capability.notGranted"), run: makeCopy },
      { id: "version.download", group: "copy", label: t("versionPanel.downloadThisVersion"),
        enabled: allows("download"), disabledReason: t("capability.notGranted"), run: download },
      { id: "version.name", group: "edit", label: t("versionPanel.nameThisVersion"), shortcut: keys.name,
        enabled: false, disabledReason: t("versionHistory.shared.nameAtSave"), run: () => {} },
      { id: "version.keep", group: "edit", label: t("versionPanel.keepThisVersion"),
        enabled: false, disabledReason: hostKeeps, run: () => {} },
      { id: "version.changes", group: "edit", label: t("versionPanel.showChanges"),
        enabled: older !== null && canRedline(),
        disabledReason: older !== null ? t("versionHistory.action.showChangesUnavailable")
          : pager?.more ? t("versionHistory.shared.loadOlderToCompare") : t("versionHistory.earliestNotComparable"),
        run: showChanges },
      { id: "version.delete", group: "danger", label: t("versionPanel.deleteThisVersion"), shortcut: keys.delete,
        danger: true, enabled: false, disabledReason: hostKeeps, run: () => {} },
    ];
  }

  return {
    list,
    scope,
    rows,
    shown,
    emptyText,
    paintHeadings,
    predecessorOf,
    reflect,
    bytes,
    menuEntries,
    save,
    saveRefusal,
    sync,
    /** "Load older" pages on; "Try again" after a failed FIRST page re-reads it. */
    more: () => load(pager?.rows.length ? "older" : "first"),
    first: () => load("first"),
    /** A page in flight is for a panel nobody is looking at. */
    cancel: () => pager?.cancel(),
    /** Whether the room's history is reachable: a room whose host lists, with a document. */
    available: () => Boolean(source()) && Boolean(documentInfo().hasDocument),
    find: (versionId) => pager?.rows.find((candidate) => candidate.versionId === versionId) ?? null,
    hasMore: () => Boolean(pager?.more),
    moreButton: moreBtn,
    saveButton: saveBtn,
  };
}
