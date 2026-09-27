// The Version history panel — the surface HF-068 / `105` OO-004 has been
// missing, and the half `#653` deliberately left out.
//
// `version_history.mjs` landed the whole durable store: checkpoints, naming,
// pinning, retention, atomic restore. None of it was reachable. A subsystem that
// is built and unreachable from the product is the most expensive recurring
// pattern in this repository (SKILL §9.4), so this module is the reachable half:
// the panel, the timeline, the actions, and the orchestration that turns a
// stored checkpoint back into the document on screen.
//
// ## Designed from the Docs standard first, not from the store's API
//
// The house rule is that every editing interaction is designed from the
// Word/Docs standard before a line is written (SKILL §8, and the owner has
// enforced it repeatedly — the crop dialog that was a numeric form because that
// was what the engine offered). Google Docs' version history is the better of
// the two references and `docs/139` §3 already names it as the model, so this is
// what it does and what this panel does:
//
// | Google Docs | Here | Difference |
// | --- | --- | --- |
// | A named surface from File ▸ Version history, not a modal | `file.versionHistory` opens a right-hand panel | Docs takes over the whole window; a panel keeps the document visible beside the timeline, which is this editor's established shape for Outline, Pages and Comments — one mechanism, not a second full-window mode |
// | Right-hand list grouped by date, newest first | same (`groupVersions`) | — |
// | Time and author on every entry | time, origin and size; author when known | version-level actor only. Per-CHANGE authorship needs the durable commit log (`docs/140` H4), and claiming it from a snapshot would be a guess |
// | "Name this version", and a named-only filter | same | — |
// | Selecting an entry previews it in the main canvas, read-only | same | arrow-key navigation previews after a short settle rather than on every keypress, because a preview parses a document and holding ↓ must not parse ten |
// | Restore this version, current state kept as a version | same | Docs restores without confirming; this asks once, because the confirmation is where the reader is TOLD their current work is kept — see `confirmRestore` |
// | "Show changes" diff toggle | present and disabled, with the reason | the structural diff is `docs/140` H3 and is not built. SKILL §10: a command that does not exist yet ships disabled with a reason, never as a button that does nothing |
// | Make a copy / Download this version | not built | needs a format→MIME answer the export registry owns; `docs/139` VH-007 stays open, and the report says so rather than the panel implying it |
//
// ## What stays out of `main.js`
//
// All of it. `main.js` is at its line ratchet with zero slack and every line
// there is paid for by an extraction, so this module owns the store handle, the
// panel DOM, the keyboard contract, the retention disclosure and the restore
// state machine, and reaches the application only through the callbacks in
// `createVersionHistory`. That is also what makes the panel's rules testable:
// the decisions live in `version_policy.mjs` and are driven in node.
//
// ## Complexity
//
// Opening the panel is O(versions in this lineage) and reads METADATA ONLY
// (`docs/139` VH-001) — the artifact bytes are never touched until a preview or
// a restore asks for one. Nothing here runs on a keystroke: the editing path's
// entire contribution to version history is `VersionCapturePolicy.shouldCapture`,
// which is O(1) in both document size and stored-version count. Selecting an
// entry is O(1) in the LIVE document; the preview it opens is O(target
// document), exactly as opening a file is, and is serialised behind one token so
// a fast reader cannot stack two parses.

import { describeDraftSize } from "./drafts.mjs";
import { t } from "./i18n.mjs";
import {
  CAPTURE_REASON,
  HISTORY_STATUS,
  VERSION_KIND,
  VersionCapturePolicy,
  openHistoryStore,
  resolveRetention,
  sanitiseVersionName,
} from "./version_history.mjs";
import {
  groupVersions,
  historyMessage,
  historyMessageKind,
  historyUnavailableReason,
  retentionSummary,
  versionRowText,
} from "./version_policy.mjs";

/** How long arrow-key navigation settles before it opens a preview.
 *
 *  A preview parses a document, so previewing on every keypress would parse one
 *  per arrow press — the `docs/116` defect in miniature, from a keyboard user
 *  doing nothing unusual. 220 ms is long enough that holding ↓ through nine rows
 *  costs one parse and short enough that a single press feels immediate. A click
 *  does not wait: the reader has already chosen. */
const PREVIEW_SETTLE_MS = 220;

/** `role="option"` ids have to be stable within one render and unique in the
 *  document, and a `versionId` is already both. */
const optionId = (versionId) => `versionOption-${versionId}`;

/**
 * Builds the version history panel and everything behind it.
 *
 * Every argument is a seam into the application, so this module never reaches
 * for editor state. The DOM ids it owns are its own and are queried here rather
 * than passed in, which is the convention `file_pane.mjs` already follows.
 *
 * @param {object} deps
 * @param {(bytes: Uint8Array) => object} deps.parse the engine's `open`; throws
 *        on a document it will not admit, which is how a corrupt checkpoint is
 *        refused before anything on screen changes.
 * @param {(doc: object|null, version: object|null) => Promise<void>} deps.showPreview
 *        swaps the canvas onto a preview document, or back to the live one when
 *        given `null`. Read-only enforcement is the caller's, because the caller
 *        already owns the one fail-closed mutation choke point.
 * @param {(bytes: Uint8Array, name: string) => Promise<boolean>} deps.activateRestored
 *        opens restored bytes through the ordinary open path.
 * @param {() => object|null} deps.snapshot the same fidelity-complete export the
 *        autosave path takes, or null when the document cannot be exported.
 * @param {() => object} deps.documentInfo `{name, docKey, revision, engine, actor, hasDocument}`.
 * @param {() => object} deps.settings the live settings object.
 * @param {() => boolean} deps.hostAllows whether this page may keep local data at all.
 * @param {(text: string, kind?: string) => void} deps.publish the status channel.
 * @param {(options: object) => Promise<boolean>} deps.confirm the application's one yes/no card.
 * @param {(current: string) => Promise<string|null>} deps.promptName the name dialog.
 * @param {(open: boolean) => void} [deps.onOpenChange] lets the chrome reflect
 *        the panel's state and close whatever it is mutually exclusive with.
 * @param {object} [deps.storeOptions] injected into `openHistoryStore` (tests).
 */
export function createVersionHistory({
  parse,
  showPreview,
  activateRestored,
  snapshot,
  documentInfo,
  settings,
  hostAllows,
  publish,
  confirm,
  promptName,
  onOpenChange = () => {},
  storeOptions = {},
}) {
  const panel = document.getElementById("versionPanel");
  const body = document.getElementById("versionPanelBody");
  const summaryEl = document.getElementById("versionPanelSummary");
  const detailEl = document.getElementById("versionPanelDetail");
  const policyEl = document.getElementById("versionPanelPolicy");
  const namedOnlyBox = document.getElementById("versionNamedOnly");
  const closeBtn = document.getElementById("versionPanelClose");
  const banner = document.getElementById("versionPreviewBanner");
  const bannerText = document.getElementById("versionPreviewBannerText");
  const bannerBack = document.getElementById("versionPreviewBack");
  const actions = {
    restore: document.getElementById("versionRestoreBtn"),
    name: document.getElementById("versionNameBtn"),
    pin: document.getElementById("versionPinBtn"),
    remove: document.getElementById("versionDeleteBtn"),
    changes: document.getElementById("versionChangesBtn"),
    clear: document.getElementById("versionClearBtn"),
  };

  /** The store, once. `null` until asked for; `storeReason` is non-empty once
   *  the browser has refused it, which is what the disabled entry point says. */
  let store = null;
  let opening = null;
  let storeReason = "";
  /** This document's timeline identity, minted by the store (`docs/140` §4.1). */
  let lineageId = "";
  let headVersionId = null;
  let rows = [];
  let selectedId = "";
  let previewVersionId = "";
  let previewDoc = null;
  /** Serialises preview work: two overlapping parses would race on `previewDoc`
   *  and leak the loser's wasm allocation. */
  let previewToken = 0;
  let settleTimer = 0;
  let returnFocus = null;
  /** One capture at a time. Autosave, Save and Open can all ask at once, and two
   *  overlapping captures would race on the lineage head. */
  let work = Promise.resolve();
  const capturePolicy = new VersionCapturePolicy({ intervalMs: retention().intervalMs });

  /** The effective retention policy. Read at call time because Settings can
   *  change it without anything being rebuilt. O(1). */
  function retention() {
    return resolveRetention(settings());
  }

  /** Runs store work one job at a time. Same posture as `queueDraftWork`: the
   *  two writers share one database and one lineage head. */
  function queue(job) {
    work = work.then(job).catch((err) => {
      console.error("version history", err);
      return null;
    });
    return work;
  }

  /** Opens the store once, lazily, and says so when it cannot be opened — a
   *  history that is not being recorded is a promise the editor is not keeping. */
  async function ensureStore() {
    if (store) return store;
    if (!hostAllows() || storeReason) return null;
    if (!opening) {
      opening = openHistoryStore(storeOptions).then(
        (opened) => {
          store = opened;
          return opened;
        },
        (err) => {
          storeReason = t("versionHistory.disabled.noStore", { message: String(err?.message ?? err) });
          return null;
        },
      );
    }
    return opening;
  }

  /** Whether the user's settings and the host's policy allow history at all. */
  function enabled() {
    const s = settings();
    return hostAllows() && s.autosave !== false && s.versionHistory !== false;
  }

  /** Why the entry point is disabled, or "". Never a silent absence (SKILL §10). */
  function unavailableReason() {
    const s = settings();
    return historyUnavailableReason({
      hasDocument: Boolean(documentInfo().hasDocument),
      hostAllows: hostAllows(),
      autosaveOn: s.autosave !== false,
      historyOn: s.versionHistory !== false,
      storeReason,
    });
  }

  /** Reports a store result through the single feedback channel, with the
   *  refusal/confirmation distinction the channel already makes — a refusal
   *  reaches the assertive live region and the viewport toast even while the
   *  footer is visible, because the footer's pill is the first thing a narrow
   *  window sheds. Never prints a code. */
  function report(result) {
    if (!result?.status) return result;
    publish(historyMessage(result.status), historyMessageKind(result.status));
    return result;
  }

  // ── The timeline ───────────────────────────────────────────────────────────

  /**
   * Rebuilds the list from `rows`.
   *
   * O(rows): one pass, and no lookup-by-id inside it. The rows are metadata —
   * a few hundred bytes each — so this never touches a checkpoint artifact and
   * never walks the document (`docs/139` VH-001, SKILL §8).
   */
  function renderList() {
    body.replaceChildren();
    const namedOnly = Boolean(namedOnlyBox?.checked);
    const groups = groupVersions(rows, { now: Date.now(), namedOnly });
    if (groups.length === 0) {
      const empty = document.createElement("div");
      empty.className = "outline-empty";
      // A FILTERED timeline that comes back empty is not an empty timeline, and
      // saying "no versions yet" over a document that has nine of them is a lie
      // the reader can see through.
      empty.textContent = namedOnly && rows.length > 0
        ? t("versionHistory.emptyNamed")
        : t("versionHistory.empty");
      body.append(empty);
      body.removeAttribute("aria-activedescendant");
      reflectActions();
      return;
    }
    for (const group of groups) {
      // `role="group"` with `aria-label`, and the visible heading hidden from
      // the accessibility tree: a heading element is not a permitted child of a
      // listbox, and the day is already in every option's accessible name.
      const section = document.createElement("div");
      section.className = "version-group";
      section.setAttribute("role", "group");
      section.setAttribute("aria-label", group.label);
      const day = document.createElement("div");
      day.className = "version-group-day";
      day.setAttribute("aria-hidden", "true");
      day.textContent = group.label;
      section.append(day);
      for (const row of group.rows) {
        section.append(renderRow(row));
      }
      body.append(section);
    }
    reflectSelection();
  }

  /** One row. A `div[role=option]` rather than a button, because the list is a
   *  single-select listbox and an interactive child inside an option is a shape
   *  screen readers cannot describe — the actions live in the panel's action bar
   *  instead, where they are also reachable by touch without hovering. */
  function renderRow(row) {
    const text = versionRowText(row, {
      isHead: row.versionId === headVersionId,
      sizeText: describeDraftSize(row.bytes ?? 0),
    });
    const item = document.createElement("div");
    item.className = "version-item";
    item.id = optionId(row.versionId);
    item.setAttribute("role", "option");
    item.setAttribute("aria-selected", "false");
    item.dataset.versionId = row.versionId;
    item.setAttribute("aria-label", text.label);
    item.title = text.timestamp;

    const head = document.createElement("div");
    head.className = "version-item-head";
    const title = document.createElement("span");
    title.className = "version-item-title";
    title.textContent = text.title;
    head.append(title);
    if (text.named) {
      // The pin is a STATE, and a colour-only or icon-only state is not a state
      // a screen reader can read — `versionRowText` puts "Named" into the
      // option's accessible name, so this glyph is decoration.
      const pin = document.createElement("span");
      pin.className = "ms version-item-pin";
      pin.setAttribute("aria-hidden", "true");
      pin.textContent = "push_pin";
      head.append(pin);
    }
    if (row.versionId === headVersionId) {
      const now = document.createElement("span");
      now.className = "version-item-current";
      now.setAttribute("aria-hidden", "true");
      now.textContent = t("versionHistory.current");
      head.append(now);
    }

    // `<time datetime>` beside the locale spelling: `docs/139` §14 requires the
    // exact machine-readable instant to be present, not implied by a friendly
    // label.
    const detail = document.createElement("time");
    detail.className = "version-item-detail";
    if (text.exact) detail.dateTime = text.exact;
    detail.textContent = text.named ? `${text.clock} · ${text.detail}` : text.detail;

    item.append(head, detail);
    item.addEventListener("click", () => {
      select(row.versionId);
      // A click is a decision already made, so it does not wait out the settle.
      void openPreview(row.versionId);
    });
    return item;
  }

  /** Marks the selected option and points `aria-activedescendant` at it, which
   *  is how a listbox tells a screen reader where it is without moving focus off
   *  the list. O(rows). */
  function reflectSelection() {
    let found = false;
    for (const item of body.querySelectorAll(".version-item")) {
      const active = item.dataset.versionId === selectedId;
      item.setAttribute("aria-selected", String(active));
      item.classList.toggle("is-active", active);
      item.classList.toggle("is-previewing", item.dataset.versionId === previewVersionId);
      if (active) found = true;
    }
    if (found) body.setAttribute("aria-activedescendant", optionId(selectedId));
    else body.removeAttribute("aria-activedescendant");
    reflectActions();
  }

  /** The visible rows, in list order — what the arrow keys walk. O(rows). */
  function visibleIds() {
    return [...body.querySelectorAll(".version-item")].map((item) => item.dataset.versionId);
  }

  function selectedRow() {
    return rows.find((row) => row.versionId === selectedId) ?? null;
  }

  /**
   * Enables each action, or disables it with the reason.
   *
   * Every branch here produces a REASON. The alternative — hiding what cannot be
   * done — leaves a reader unable to tell a permission from a bug, which SKILL
   * §10 rules out and `capabilities.mjs` already refuses to do for review modes.
   * O(1).
   */
  function reflectActions() {
    const row = selectedRow();
    const previewing = Boolean(previewVersionId);
    const needs = t("versionHistory.needsSelection");
    const set = (button, allowed, reason) => {
      if (!button) return;
      button.disabled = !allowed;
      if (allowed) button.removeAttribute("title");
      else button.title = reason;
    };
    set(
      actions.restore,
      Boolean(row) && row.versionId !== headVersionId,
      row ? t("versionHistory.headNotRestorable") : needs,
    );
    set(actions.name, Boolean(row), needs);
    set(actions.pin, Boolean(row), needs);
    set(actions.remove, Boolean(row) && row.versionId !== headVersionId, row ? t("versionHistory.headNotDeletable") : needs);
    // Present, disabled, and honest about why: the structural diff is `docs/140`
    // H3 and is not built.
    set(actions.changes, false, t("versionHistory.action.showChangesUnavailable"));
    set(actions.clear, rows.length > 0, t("versionHistory.empty"));
    // A toggle button, `aria-pressed` and a FIXED label: "Keep this version"
    // that becomes "Stop keeping this version" would be a control whose name
    // changes under a screen-reader user's cursor, and the applier that walks
    // `data-i18n` on a locale change would overwrite a script-set label anyway.
    // The state belongs in `aria-pressed`, which is what it is for.
    actions.pin?.setAttribute("aria-pressed", String(Boolean(row?.pinned)));
    if (bannerBack) bannerBack.hidden = !previewing;
    if (banner) banner.hidden = !previewing;
  }

  /** The disclosure line: how much history there is and what the policy does
   *  with it (`docs/139` §12, §8.7). O(versions in the store). */
  async function reflectSummary() {
    if (!summaryEl) return;
    const ready = await ensureStore();
    if (!ready) {
      summaryEl.textContent = unavailableReason();
      if (detailEl) detailEl.textContent = "";
      if (policyEl) policyEl.textContent = "";
      return;
    }
    // `expectVersions` is what turns a browser that dropped the origin's
    // IndexedDB into a reported loss rather than an empty timeline.
    const status = await ready.storageStatus({ expectVersions: rows.length });
    if (status.status === HISTORY_STATUS.EVICTED) report(status);
    const words = retentionSummary(status, retention(), describeDraftSize);
    summaryEl.textContent = words.kept;
    if (detailEl) detailEl.textContent = words.detail;
    if (policyEl) policyEl.textContent = words.policy;
  }

  /** Re-reads the timeline from the store and repaints. O(versions). */
  async function refresh() {
    const ready = await ensureStore();
    if (!ready || !lineageId) {
      rows = [];
      renderList();
      await reflectSummary();
      return;
    }
    rows = await ready.listVersions(lineageId);
    headVersionId = await ready.head(lineageId);
    if (selectedId && !rows.some((row) => row.versionId === selectedId)) selectedId = "";
    renderList();
    await reflectSummary();
  }

  // ── Preview ────────────────────────────────────────────────────────────────

  function select(versionId) {
    selectedId = versionId;
    reflectSelection();
  }

  /** Moves the active option and schedules its preview. Arrow keys settle; see
   *  `PREVIEW_SETTLE_MS`. O(rows). */
  function moveActive(delta, absolute = null) {
    const ids = visibleIds();
    if (ids.length === 0) return;
    const at = ids.indexOf(selectedId);
    const next =
      absolute === null
        ? Math.min(ids.length - 1, Math.max(0, (at === -1 ? 0 : at) + delta))
        : absolute < 0
          ? ids.length - 1
          : 0;
    select(ids[next]);
    body.querySelector(`#${CSS.escape(optionId(ids[next]))}`)?.scrollIntoView({ block: "nearest" });
    clearTimeout(settleTimer);
    settleTimer = setTimeout(() => void openPreview(ids[next]), PREVIEW_SETTLE_MS);
  }

  /**
   * Opens a version in an isolated read-only preview (`docs/139` §8.4).
   *
   * The checkpoint is read and hash-verified by the store, then parsed through
   * the ORDINARY format path — the same admission limits as opening a file — and
   * only a document that survives both reaches the canvas. A corrupt or missing
   * artifact is reported and the live document is untouched, which is the
   * difference between a preview and a hazard.
   *
   * Complexity: O(target document) for the read, the hash and the parse, once
   * per preview. It never walks the live document, and nothing here runs on an
   * interaction with the document itself.
   */
  async function openPreview(versionId) {
    if (versionId === previewVersionId) return;
    if (versionId === headVersionId) {
      // The head IS the document on screen. Previewing it would swap the live
      // session for a byte-identical copy and throw away the caret for nothing.
      await closePreview();
      return;
    }
    const row = rows.find((candidate) => candidate.versionId === versionId);
    const ready = await ensureStore();
    if (!row || !ready) return;
    const token = ++previewToken;
    const loaded = await ready.readCheckpoint(row.checkpointId);
    if (token !== previewToken) return;
    if (!loaded.ok) {
      report(loaded);
      return;
    }
    let next = null;
    try {
      next = parse(loaded.bytes);
    } catch (err) {
      // `docs/139` §8.4: an unavailable or partial version is stated, never
      // shown as an empty preview.
      publish(t("versionHistory.preview.failed", { message: String(err?.message ?? err) }), "error");
      return;
    }
    if (token !== previewToken) {
      next.free();
      return;
    }
    const previous = previewDoc;
    previewDoc = next;
    previewVersionId = versionId;
    const words = versionRowText(row, { isHead: false });
    if (bannerText) {
      bannerText.textContent = t("versionHistory.preview.banner", { when: words.timestamp });
    }
    await showPreview(next, row);
    // Freed AFTER the swap: the canvas held the old preview until `showPreview`
    // returned, and freeing before that would hand the renderer a dead wrapper.
    previous?.free();
    reflectSelection();
  }

  /** Returns to the live document. Idempotent, because Escape, "Back to
   *  current", closing the panel and a restore all reach it. */
  async function closePreview() {
    clearTimeout(settleTimer);
    previewToken += 1;
    if (!previewVersionId) return;
    previewVersionId = "";
    const previous = previewDoc;
    previewDoc = null;
    await showPreview(null, null);
    previous?.free();
    reflectSelection();
  }

  // ── Actions ────────────────────────────────────────────────────────────────

  /**
   * Restore, as `docs/140` §9 specifies it, and in that order.
   *
   * 1. `prepareRestore` validates the target's bytes against the hash its key
   *    claims and **captures the current document as a version first**. If that
   *    capture is refused — quota, a wedged budget — the restore is refused with
   *    it and nothing has changed. That is `docs/112`'s "never let a restore
   *    leave the work in neither place", one level up.
   * 2. The bytes are parsed in isolation. A parse failure ABANDONS the prepared
   *    record rather than committing it; the pre-restore version it already took
   *    stays, because it is a real point in the document's past.
   * 3. `commitRestore` advances the head only if it is still the head the
   *    prepare saw (compare-and-set), in ONE IndexedDB transaction.
   * 4. Only then is the canvas activated, through the ordinary open path.
   *
   * Nothing is deleted at any step: the target, the pre-restore capture and
   * every version in between all remain (append, never rewind).
   *
   * Complexity: O(target bytes) twice — once to validate, once to activate —
   * plus O(current document) for the pre-restore export. All three are
   * document-sized and none is on an interaction path.
   */
  async function restore(versionId) {
    const ready = await ensureStore();
    const row = rows.find((candidate) => candidate.versionId === versionId);
    if (!ready || !row) return;
    if (!(await confirmRestore(row))) return;
    const info = documentInfo();
    let current = null;
    try {
      current = snapshot();
    } catch (err) {
      // The current document cannot be exported, so it cannot be kept as a
      // version — and a restore that cannot keep it is a destructive restore.
      publish(t("versionHistory.restore.cannotKeepCurrent", { message: String(err?.message ?? err) }), "error");
      return;
    }
    const prepared = await ready.prepareRestore({
      lineageId,
      versionId,
      idempotencyKey: `restore-${versionId}-${Date.now()}`,
      current: current
        ? {
            bytes: current.bytes,
            formatId: current.formatId,
            exportMode: current.mode,
            findings: current.findings,
            revision: info.revision,
            engine: info.engine,
            actor: info.actor,
          }
        : null,
      retention: retention(),
      actor: info.actor,
    });
    if (prepared.status !== HISTORY_STATUS.RESTORE_PREPARED) return void report(prepared);

    let validated = null;
    try {
      validated = parse(prepared.bytes);
    } catch (err) {
      await ready.abandonRestore(prepared.operation.opId, { reason: "parse_failed" });
      publish(t("versionHistory.restore.failed", { message: String(err?.message ?? err) }), "error");
      await refresh();
      return;
    }
    // Validated, and the validation document has served its purpose: the canvas
    // is activated from the BYTES through the ordinary open path, so a restored
    // document is indistinguishable from an opened one.
    validated.free();

    const committed = await ready.commitRestore({ opId: prepared.operation.opId, retention: retention() });
    if (committed.status !== HISTORY_STATUS.RESTORE_COMMITTED) return void report(committed);

    await closePreview();
    const words = versionRowText(row, { isHead: false });
    await activateRestored(committed.bytes, info.name);
    await refresh();
    selectedId = committed.version?.versionId ?? "";
    reflectSelection();
    publish(t("versionHistory.restored", { when: words.timestamp }), "");
  }

  /**
   * The one confirmation, and why there is one.
   *
   * `docs/139` §18 question 6 asked whether Restore must always confirm. It
   * must, and the reason is not caution: the confirmation is the only place the
   * reader is TOLD that their current work becomes a version of its own. Google
   * Docs restores without asking, and can — its restore is an edit to a
   * server-side document the user can undo. Here it replaces the document in the
   * tab, so the sentence that makes restore legible as non-destructive has to be
   * read before it happens, not after.
   */
  function confirmRestore(row) {
    const words = versionRowText(row, { isHead: false });
    return confirm({
      title: t("versionHistory.restore.title"),
      message: t("versionHistory.restore.message", {
        name: row.name || words.timestamp,
        when: words.timestamp,
      }),
      confirmLabel: t("versionHistory.restore.confirm"),
      cancelLabel: t("versionHistory.restore.cancel"),
      note: t("versionHistory.restore.note"),
      icon: "history",
    });
  }

  /** Name or rename. Naming pins (`docs/139` §8.3): the label makes a version
   *  findable, the pin makes it durable, and the store treats them as separable
   *  operations — which is what `setPinned` below is for. */
  async function nameVersion(versionId) {
    const ready = await ensureStore();
    const row = rows.find((candidate) => candidate.versionId === versionId);
    if (!ready || !row) return;
    const typed = await promptName(row.name ?? "");
    if (typed === null) return;
    const clean = sanitiseVersionName(typed);
    if (!clean.ok) return void report({ status: clean.reason });
    const result = await ready.nameVersion(versionId, typed, { pinLimit: retention().pinLimit });
    await refresh();
    if (result.ok) publish(t("versionHistory.named", { name: clean.name }), "");
    else report(result);
  }

  async function setPinned(versionId, pinned) {
    const ready = await ensureStore();
    if (!ready) return;
    const result = await ready.setPinned(versionId, pinned, { pinLimit: retention().pinLimit });
    await refresh();
    if (result.ok) publish(pinned ? t("versionHistory.pinned") : t("versionHistory.unpinned"), "");
    else report(result);
  }

  /** Deletes one version, after asking: its contents go with it. */
  async function removeVersion(versionId) {
    const ready = await ensureStore();
    const row = rows.find((candidate) => candidate.versionId === versionId);
    if (!ready || !row) return;
    const words = versionRowText(row, { isHead: false });
    const ok = await confirm({
      title: t("versionHistory.delete.title"),
      message: t("versionHistory.delete.message", { name: row.name || words.timestamp }),
      confirmLabel: t("versionHistory.delete.confirm"),
      cancelLabel: t("versionHistory.delete.cancel"),
      icon: "warning",
    });
    if (!ok) return;
    if (versionId === previewVersionId) await closePreview();
    const result = await ready.deleteVersion(versionId);
    await refresh();
    if (result.ok) publish(t("versionHistory.deleted"), "");
    else report(result);
  }

  /** `docs/139` §8.7's visible **Clear version history** control, which reports
   *  the size it freed for the same reason autosave's retention posture is
   *  written next to its switch. */
  async function clearHistory() {
    const ready = await ensureStore();
    if (!ready || !lineageId) return;
    const ok = await confirm({
      title: t("versionHistory.clear.title"),
      message: t("versionHistory.clear.message"),
      confirmLabel: t("versionHistory.clear.confirm"),
      cancelLabel: t("versionHistory.clear.cancel"),
      icon: "warning",
    });
    if (!ok) return;
    await closePreview();
    const result = await ready.clearHistory({ lineageId });
    await refresh();
    publish(
      t("versionHistory.clear.done", {
        count: result.versions ?? 0,
        size: describeDraftSize(result.freed ?? 0),
      }),
      "",
    );
  }

  // ── The panel ──────────────────────────────────────────────────────────────

  function isOpen() {
    return Boolean(panel) && !panel.hidden;
  }

  /** Opens the panel and puts the keyboard in the list. The panel is a real
   *  surface, so it takes focus (`docs/139` §14) and remembers where to give it
   *  back. */
  async function open() {
    if (!panel || isOpen()) return;
    returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    panel.hidden = false;
    onOpenChange(true);
    await refresh();
    body?.focus();
  }

  /** Closes the panel, leaves the preview behind, and gives the keyboard back to
   *  whatever opened it — or to the editing surface when that element is gone. */
  async function close() {
    if (!panel || !isOpen()) return;
    await closePreview();
    panel.hidden = true;
    onOpenChange(false);
    const back = returnFocus;
    returnFocus = null;
    if (back?.isConnected) back.focus();
  }

  async function toggle() {
    if (isOpen()) await close();
    else await open();
  }

  body?.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown") moveActive(1);
    else if (event.key === "ArrowUp") moveActive(-1);
    else if (event.key === "Home") moveActive(0, 1);
    else if (event.key === "End") moveActive(0, -1);
    else if (event.key === "Enter" || event.key === " ") {
      // Enter and Space open the selection NOW, without the settle: a keyboard
      // reader who has arrowed to a row and pressed Enter has decided.
      clearTimeout(settleTimer);
      if (selectedId) void openPreview(selectedId);
    } else if (event.key === "Escape") {
      void close();
    } else {
      return;
    }
    event.preventDefault();
  });

  closeBtn?.addEventListener("click", () => void close());
  bannerBack?.addEventListener("click", () => void closePreview());
  namedOnlyBox?.addEventListener("change", () => renderList());
  actions.restore?.addEventListener("click", () => void queue(() => restore(selectedId)));
  actions.name?.addEventListener("click", () => void queue(() => nameVersion(selectedId)));
  actions.pin?.addEventListener("click", () =>
    void queue(() => setPinned(selectedId, !selectedRow()?.pinned)),
  );
  actions.remove?.addEventListener("click", () => void queue(() => removeVersion(selectedId)));
  actions.clear?.addEventListener("click", () => void queue(() => clearHistory()));

  return {
    isOpen,
    open,
    close,
    toggle,
    available: enabled,
    unavailableReason,
    previewing: () => Boolean(previewVersionId),

    /**
     * Joins a freshly opened document to its own timeline and records the import
     * baseline (`docs/140` §7.5).
     *
     * The lineage is found by the `documentKey` hint the draft path already
     * computes — O(1) in document size — and the lineage ID itself is MINTED and
     * stored, because identity may not be derived from a filename or a byte hash.
     */
    async adopt() {
      capturePolicy.intervalMs = retention().intervalMs;
      if (!enabled()) return null;
      return queue(async () => {
        const ready = await ensureStore();
        if (!ready) return null;
        const info = documentInfo();
        const lineage = await ready.openLineage({ docKey: info.docKey, name: info.name });
        lineageId = lineage.lineageId;
        headVersionId = lineage.headVersionId;
        rows = lineage.versions;
        selectedId = "";
        if (isOpen()) {
          renderList();
          await reflectSummary();
        }
        return captureNow(CAPTURE_REASON.OPEN);
      });
    },

    /**
     * The autosave path's hook. Called AFTER the draft has been written, with
     * the same bytes, because the artifact is already exported and already
     * verified at that point — capturing anywhere else would export the document
     * twice (`docs/140` §7.5).
     *
     * `shouldCapture` is the only thing this costs on the editing path, and it
     * is O(1) in both document size and stored-version count.
     */
    capture(reason, artifact) {
      if (!enabled() || !lineageId) return null;
      const info = documentInfo();
      const decision = capturePolicy.shouldCapture({
        reason,
        revision: info.revision,
        now: Date.now(),
        enabled: true,
      });
      if (!decision.capture) return null;
      return queue(() => captureNow(reason, artifact, decision.kind));
    },

    /** Boot work: finish a restore a killed tab left prepared, then apply the
     *  age window. Both are boot-time by nature (`docs/140` §7.5). */
    resume() {
      if (!enabled()) return null;
      return queue(async () => {
        const ready = await ensureStore();
        if (!ready) return null;
        await ready.resolvePendingRestores();
        if (lineageId) await ready.sweep({ lineageId, retention: retention() });
        return null;
      });
    },

    /** Settings or the locale changed: the capture interval, the row words and
     *  the disclosure all follow. The rows are rebuilt rather than left stale
     *  because their day headings, origins and timestamps are all locale-shaped
     *  — a panel that stayed in the previous language would be the one surface
     *  the relabel missed. O(rows). */
    reflect() {
      capturePolicy.intervalMs = retention().intervalMs;
      if (!isOpen()) return;
      renderList();
      void reflectSummary();
    },
  };

  /**
   * Writes one version. Always through `queue`, never directly.
   *
   * `artifact` is the export the caller already has; when it is absent this
   * takes one, which is the Save and Open paths where no draft was written.
   */
  async function captureNow(reason, artifact = null, kind = null) {
    const ready = await ensureStore();
    if (!ready || !lineageId) return null;
    const info = documentInfo();
    let taken = artifact;
    if (!taken) {
      try {
        taken = snapshot();
      } catch {
        // A document that cannot be exported cannot be versioned. Autosave
        // reports that failure through its own channel; repeating it here would
        // be two sentences about one problem.
        return null;
      }
    }
    if (!taken?.bytes?.length) return null;
    const result = await ready.captureVersion({
      lineageId,
      bytes: taken.bytes,
      formatId: taken.formatId,
      exportMode: taken.mode,
      findings: taken.findings,
      revision: info.revision,
      engine: info.engine,
      actor: info.actor,
      kind: kind ?? kindFor(reason),
      retention: retention(),
    });
    if (result.ok) {
      capturePolicy.noteCaptured(Date.now(), info.revision);
      rows = await ready.listVersions(lineageId);
      headVersionId = result.version?.versionId ?? headVersionId;
      if (isOpen()) {
        renderList();
        await reflectSummary();
      }
    } else {
      // A version that could not be kept is REPORTED. A quota-exhausted store
      // and a budget wedged by named versions have different ways out, so they
      // get different sentences (`version_policy.mjs`).
      report(result);
    }
    return result;
  }
}

/** The version kind a capture reason records, for the explicit reasons the
 *  autosave path does not own. O(1). */
function kindFor(reason) {
  if (reason === CAPTURE_REASON.OPEN) return VERSION_KIND.IMPORT;
  if (reason === CAPTURE_REASON.SAVE) return VERSION_KIND.SAVED;
  if (reason === CAPTURE_REASON.MANUAL) return VERSION_KIND.MANUAL;
  return VERSION_KIND.AUTO;
}
