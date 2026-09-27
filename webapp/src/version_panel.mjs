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
// | A ⋮ menu on every entry carrying that entry's actions | same | — (see "The row actions" below: this row USED to read "an action bar below the list", and that difference is what the owner rejected) |
// | Restore also offered prominently while previewing | on the preview bar, beside "Back to current" | — |
// | Restore this version, current state kept as a version | same | Docs restores without confirming; this asks once, because the confirmation is where the reader is TOLD their current work is kept — see `confirmRestore` |
// | "Show changes" diff toggle | present and disabled, with the reason | the structural diff is `docs/140` H3 and is not built. SKILL §10: a command that does not exist yet ships disabled with a reason, never as a button that does nothing |
// | Make a copy / Download this version | not built | needs a format→MIME answer the export registry owns; `docs/139` VH-007 stays open, and the report says so rather than the panel implying it |
//
// ## The row actions, and why the list had to stop being a listbox
//
// The first round of this panel put all five actions in a bar under the list and
// wrote down two reasons: a `role="option"` may not hold an interactive child,
// and a bar has no hover affordance. Both are true. Neither survives contact with
// the result, which the owner saw and rejected: with one or two versions the bar
// was most of the panel, four of its five buttons were disabled until a row was
// selected, and every one of them was detached from the row it acted on.
//
// The ARIA constraint was real, so the answer is the structure that does not have
// it. The established pattern for "a list of selectable rows, each carrying its
// own actions" is the **grid** — Gmail's message list, Drive's list view, the APG
// grid pattern — a composite widget whose cells may be interactive and whose
// focus is managed by a ROVING TABINDEX rather than `aria-activedescendant`. So:
//
//   `role="grid"`     the list                (`aria-selected` still on the row)
//   `role="rowgroup"` one per day heading     (`group` is not a legal grid child)
//   `role="row"`      one per version
//   `role="gridcell"` the entry, and the ⋮
//
// A screen reader reads the row it lands on, and Right/Left step to the ⋮ and
// back. The ⋮ takes REAL focus, so Enter opens its menu; Shift+F10 and the
// ContextMenu key open the same menu without reaching for the ⋮ at all, and so
// does a right-click. Nothing is hover-only: the ⋮ is always painted and is a
// 44px target.
//
// The day heading stays `aria-hidden` and the day stays inside every row's
// accessible name, as before. It lost its `aria-label` in the move, because
// `rowgroup`'s support for a name is not something to bet a screen reader on when
// the information is already in every row.
//
// ## Where each action's second surface is (SKILL §10: never one surface)
//
//   Restore         the row menu, and the preview bar's "Restore this version"
//   Name            the row menu, and **F2** on the focused row
//   Delete          the row menu, and **Delete** on the focused row
//   Keep            the row menu, and the row's right-click / Shift+F10 menu
//   Show changes    the same pair, both disabled with the reason
//   Clear history   the panel footer (it acts on the timeline, not on a row)
//
// F2 and Delete are the two keys a list carries everywhere, and they are
// ADVERTISED in the menu's shortcut column rather than left to be discovered —
// an undiscoverable chord is not a second surface.
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

import { clampContextMenuPosition, moveMenuIndex, normalizeMenuEntries } from "./context_menu.mjs";
import { describeDraftSize } from "./drafts.mjs";
import { t } from "./i18n.mjs";
import { formatShortcut, matchesShortcut } from "./keyboard.mjs";
import { focusMenuIndex, renderMenuLevel } from "./menu_render.mjs";
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

/** Row ids have to be stable within one render and unique in the document, and a
 *  `versionId` is already both. Named `versionOption-…` since the rows were
 *  options; the spelling is kept because it is what the specs and the panel's
 *  `aria` wiring already say, and renaming it buys nothing. */
const optionId = (versionId) => `versionOption-${versionId}`;

/** The two row keys, declared in the same Apple notation every other chord in
 *  this editor is declared in, so `matchesShortcut` binds exactly what
 *  `formatShortcut` prints in the menu — one declaration, never two tables that
 *  drift (`109` UX-006/UX-007).
 *
 *  They are PANEL-SCOPED and deliberately not in `keymap.mjs`: F2 and Delete are
 *  the keys a list carries, not chords the application owns, and binding Delete
 *  globally would be a document edit. */
const NAME_KEY = "F2";
const DELETE_KEY = "⌦";

/** Which of a row's two cells the keyboard is on: the entry, or its ⋮. */
const CELL_ENTRY = 0;
const CELL_MENU = 1;

/** A Material Symbols glyph, `aria-hidden` because it is decoration and the
 *  state it stands for is already in the option's accessible name.
 *
 *  A helper rather than three inline lines, for the same reason
 *  `compact_toolbar.mjs` and `review_chrome.mjs` have one: a ligature NAME is not
 *  prose, and writing it straight into `textContent` at the call site makes the
 *  unrouted-string scanner read a font instruction as English. */
function iconSpan(name, className = "") {
  const icon = document.createElement("span");
  icon.className = className ? `ms ${className}` : "ms";
  icon.setAttribute("aria-hidden", "true");
  icon.textContent = name;
  return icon;
}

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
  // The two durable toggles: the View band's button and the left rail's entry
  // below Comments. This module owns both pressed states and both disabled
  // reasons, so it owns both elements — two owners of one button is how a control
  // comes to say one thing and do another, and two buttons for one command with
  // two owners is that defect twice.
  const entryPoints = [
    document.getElementById("viewVersionsBtn"),
    document.getElementById("railVersions"),
  ].filter(Boolean);
  const banner = document.getElementById("versionPreviewBanner");
  const bannerText = document.getElementById("versionPreviewBannerText");
  const bannerBack = document.getElementById("versionPreviewBack");
  const bannerRestore = document.getElementById("versionPreviewRestore");
  const clearBtn = document.getElementById("versionClearBtn");

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
  /** The cell the roving tabindex is on. A grid has exactly one tab stop. */
  let activeCell = CELL_ENTRY;
  /** The open row menu: the version it acts on, its DOM, its ⋮, and the level
   *  record `menu_render.mjs` moves the keyboard through. */
  let menuVersionId = "";
  let rowMenuEl = null;
  let menuButton = null;
  let menuLevel = null;
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
    // Every row about to be replaced, including the one an open menu is anchored
    // to. A menu left pointing at a detached button is a menu whose Escape has
    // nowhere to give the keyboard back to.
    closeRowMenu();
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
      // With no rows there is no cell to be the grid's tab stop, so the grid
      // itself takes the keyboard — otherwise opening an empty timeline would
      // drop focus on `<body>`.
      body.tabIndex = 0;
      reflectActions();
      return;
    }
    body.tabIndex = -1;
    for (const group of groups) {
      // `role="rowgroup"`, because `group` is not a permitted child of a grid.
      // The visible heading stays hidden from the accessibility tree and the day
      // stays inside every row's accessible name, which is where it was already
      // — so nothing is lost by `rowgroup` carrying no name of its own.
      const section = document.createElement("div");
      section.className = "version-group";
      section.setAttribute("role", "rowgroup");
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

  /**
   * One row: a `div[role=row]` holding two `gridcell`s — the entry, and the ⋮
   * that carries this version's own actions.
   *
   * The entry cell is the one that carries the accessible name, because it is
   * the cell focus lands on; putting the same sentence on the row as well would
   * make a screen reader read the version twice for one arrow press.
   *
   * O(1) per row, and nothing here reads the document.
   */
  function renderRow(row) {
    const text = versionRowText(row, {
      isHead: row.versionId === headVersionId,
      sizeText: describeDraftSize(row.bytes ?? 0),
    });
    const item = document.createElement("div");
    item.className = "version-item";
    item.id = optionId(row.versionId);
    item.setAttribute("role", "row");
    item.setAttribute("aria-selected", "false");
    item.dataset.versionId = row.versionId;

    const entry = document.createElement("div");
    entry.className = "version-item-entry";
    entry.setAttribute("role", "gridcell");
    entry.tabIndex = -1;
    entry.setAttribute("aria-label", text.label);
    entry.title = text.timestamp;

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
      head.append(iconSpan("push_pin", "version-item-pin"));
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

    entry.append(head, detail);
    entry.addEventListener("click", () => {
      select(row.versionId, CELL_ENTRY);
      // A click is a decision already made, so it does not wait out the settle.
      void openPreview(row.versionId);
    });

    // The ⋮. Always painted, never revealed by hover: a hover affordance is no
    // affordance at all on a phone, and this list is one of the surfaces the
    // narrow-screen drawer carries.
    const actionCell = document.createElement("div");
    actionCell.className = "version-item-actions";
    actionCell.setAttribute("role", "gridcell");
    const menuBtn = document.createElement("button");
    menuBtn.type = "button";
    menuBtn.className = "version-item-menu";
    menuBtn.tabIndex = -1;
    menuBtn.setAttribute("aria-haspopup", "menu");
    menuBtn.setAttribute("aria-expanded", "false");
    menuBtn.setAttribute("aria-label", t("versionHistory.rowActions", { when: text.timestamp }));
    menuBtn.append(iconSpan("more_vert"));
    menuBtn.addEventListener("click", () => {
      // A second press on the same ⋮ closes it, which is what every menu button
      // in this editor does and what a reader expects of one.
      if (menuVersionId === row.versionId) return void closeRowMenu({ restoreFocus: true });
      select(row.versionId, CELL_MENU);
      openRowMenu(row.versionId);
    });
    actionCell.append(menuBtn);

    item.append(entry, actionCell);
    // The platform gesture, on the whole row: `docs/139` §14 asks for row menus
    // that work by keyboard, and a right-click is the pointer half of the same
    // affordance. Shift+F10 and the ContextMenu key are handled on the grid.
    item.addEventListener("contextmenu", (event) => {
      event.preventDefault();
      select(row.versionId, CELL_MENU);
      openRowMenu(row.versionId, { x: event.clientX, y: event.clientY });
    });
    return item;
  }

  /**
   * Marks the selected row and places the grid's ONE tab stop.
   *
   * A grid manages focus with a roving tabindex rather than
   * `aria-activedescendant`, because its cells are really focusable — that is
   * the whole reason this is a grid and not a listbox. Exactly one cell carries
   * `tabindex="0"`: the active cell of the selected row, or the first row's
   * entry when nothing is selected yet, so Tab always lands somewhere sensible.
   *
   * It never MOVES focus. Moving is the business of `moveActive`/`moveCell`,
   * which the user drove; this runs on refreshes too, and a refresh that stole
   * the keyboard would take it off whatever the user was doing.
   *
   * O(rows).
   */
  function reflectSelection() {
    const items = [...body.querySelectorAll(".version-item")];
    const selected = items.find((item) => item.dataset.versionId === selectedId) ?? null;
    const stop = selected ?? items[0] ?? null;
    for (const item of items) {
      const active = item === selected;
      item.setAttribute("aria-selected", String(active));
      item.classList.toggle("is-active", active);
      item.classList.toggle("is-previewing", item.dataset.versionId === previewVersionId);
      const cells = cellsOf(item);
      const wanted = item === stop ? (selected ? activeCell : CELL_ENTRY) : -1;
      cells.forEach((cell, index) => {
        if (cell) cell.tabIndex = index === wanted ? 0 : -1;
      });
    }
    reflectActions();
  }

  /** A row's two cells, in keyboard order. O(1). */
  function cellsOf(item) {
    return [item.querySelector(".version-item-entry"), item.querySelector(".version-item-menu")];
  }

  /** The grid's single tab stop, or the grid itself when it is empty. */
  function focusGrid() {
    const cell = body.querySelector('.version-item [tabindex="0"]');
    if (cell instanceof HTMLElement) cell.focus({ preventScroll: true });
    else body.focus();
  }

  /**
   * Runs work that swaps the document, and gives the panel its keyboard back.
   *
   * Swapping the document sets the review mode, and setting the review mode ends
   * by focusing the editing surface — deliberately, because every other caller of
   * that path has just changed how the document may be edited and wants the
   * keyboard there. A reader arrowing through the timeline does not: the preview
   * settles 220 ms after the last arrow press, so the NEXT ArrowDown would scroll
   * the document instead of walking to the next version, and a keyboard user
   * would be quietly ejected from the panel by the panel's own feature.
   *
   * Only when the panel HAD the keyboard, so a preview opened while the reader
   * is typing in the document does not snatch it back.
   *
   * The panel's keyboard territory includes the ROW MENU, which is parented to
   * `document.body` so it can paint over the canvas. Counting only
   * `#versionPanel` fixed the arrow-key case and left the identical one a step
   * further on: open a row's menu with the keyboard, and the preview the same
   * arrow press scheduled settles 220 ms later and empties the menu of focus, so
   * the next ArrowDown walks the document. Same defect, second doorway — so the
   * question asked here is "does the timeline own the keyboard", not "is focus
   * inside one element".
   *
   * Found by asserting which element really holds focus rather than
   * `aria-activedescendant`, which does not move with it — the shipped listbox
   * had the same defect and no guard that could see it.
   */
  async function keepingFocus(work) {
    const owns = () => {
      const el = document.activeElement;
      if (!(el instanceof Node)) return false;
      if (panel.contains(el)) return true;
      return Boolean(rowMenuEl) && !rowMenuEl.hidden && rowMenuEl.contains(el);
    };
    const had = isOpen() && owns();
    await work();
    if (!had || owns()) return;
    // Back where it was: into the menu when one is still open, onto the grid's
    // tab stop otherwise.
    if (menuLevel) focusMenuIndex(menuLevel, menuLevel.index);
    else focusGrid();
  }

  /** The visible rows, in list order — what the arrow keys walk. O(rows). */
  function visibleIds() {
    return [...body.querySelectorAll(".version-item")].map((item) => item.dataset.versionId);
  }

  /**
   * The two controls that are not on a row: Clear version history, and the
   * preview bar.
   *
   * Everything a SINGLE version can have done to it moved onto that version's
   * own row menu, where it is built from the row that is really there — so there
   * is no longer a bar of controls that has to be told, five times, that nothing
   * is selected. Clear acts on the whole timeline and stays in the footer; it is
   * still disabled WITH A REASON when there is no timeline to clear (SKILL §10).
   *
   * The preview bar's Restore needs no enabled/disabled branch at all: the head
   * cannot be previewed (`openPreview` returns early), so whenever this bar is on
   * screen the version behind it is restorable.
   *
   * O(1).
   */
  function reflectActions() {
    const previewing = Boolean(previewVersionId);
    if (clearBtn) {
      clearBtn.disabled = rows.length === 0;
      if (rows.length > 0) clearBtn.removeAttribute("title");
      else clearBtn.title = t("versionHistory.empty");
    }
    if (bannerBack) bannerBack.hidden = !previewing;
    if (bannerRestore) bannerRestore.hidden = !previewing;
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

  function select(versionId, cell = activeCell) {
    selectedId = versionId;
    activeCell = cell;
    reflectSelection();
  }

  /** Left/Right across a row's two cells, which is how a grid's keyboard reaches
   *  the ⋮ without a pointer. Selects the first row when nothing is selected, so
   *  a first ArrowRight is not silently ignored. O(rows).
   *
   *  NOT mirrored for right-to-left locales, and that is a recorded gap rather
   *  than an oversight: no keyboard navigation in this chrome mirrors — the
   *  command menu, the glyph picker and the radio groups all read ArrowRight as
   *  "next" — so mirroring only here would make this one widget the outlier. It
   *  is one rule for the whole chrome or none, and that rule is its own piece of
   *  work. */
  function moveCell(delta) {
    const ids = visibleIds();
    if (ids.length === 0) return;
    const target = selectedId || ids[0];
    select(target, Math.min(CELL_MENU, Math.max(CELL_ENTRY, activeCell + delta)));
    focusGrid();
  }

  /** Moves the active row and schedules its preview. Arrow keys settle; see
   *  `PREVIEW_SETTLE_MS`. O(rows). */
  function moveActive(delta, absolute = null) {
    const ids = visibleIds();
    if (ids.length === 0) return;
    const at = ids.indexOf(selectedId);
    // With nothing selected yet, the FIRST press lands on an end rather than
    // stepping from an imagined position: ArrowDown selects the newest version,
    // ArrowUp the oldest. Stepping from a notional index 0 made the first
    // ArrowDown select the SECOND row and skip the newest version entirely —
    // found by `version-history-panel.spec.mjs`, which reads which cell really
    // holds the keyboard rather than trusting the list looked right.
    const next =
      absolute !== null
        ? absolute < 0
          ? ids.length - 1
          : 0
        : at === -1
          ? delta > 0
            ? 0
            : ids.length - 1
          : Math.min(ids.length - 1, Math.max(0, at + delta));
    select(ids[next]);
    body.querySelector(`#${CSS.escape(optionId(ids[next]))}`)?.scrollIntoView({ block: "nearest" });
    // A grid moves REAL focus, so the cell the tab stop just landed on has to
    // take it — `aria-activedescendant` is what a listbox uses, and this list
    // stopped being one when its rows gained a menu button.
    focusGrid();
    clearTimeout(settleTimer);
    settleTimer = setTimeout(() => void openPreview(ids[next]), PREVIEW_SETTLE_MS);
  }

  // ── The row menu ───────────────────────────────────────────────────────────

  /**
   * The actions for ONE version, as menu descriptors.
   *
   * Built from the row record the panel already holds, so opening a menu is
   * O(1) in document size and O(1) in stored-version count — it reads no
   * checkpoint and walks no document (`docs/139` VH-001, SKILL §8).
   *
   * Every row that cannot run says WHY, exactly as the bar this replaced did:
   * the head is not restorable and not deletable because it is the document,
   * and Show changes is `docs/140` H3 and is not built.
   */
  function rowMenuEntries(row) {
    const id = row.versionId;
    const isHead = id === headVersionId;
    return [
      {
        id: "version.restore",
        group: "restore",
        label: t("versionPanel.restoreThisVersion"),
        enabled: !isHead,
        disabledReason: t("versionHistory.headNotRestorable"),
        run: () => void queue(() => restore(id)),
      },
      {
        id: "version.name",
        group: "edit",
        label: t("versionPanel.nameThisVersion"),
        shortcut: NAME_KEY,
        run: () => void queue(() => nameVersion(id)),
      },
      {
        id: "version.keep",
        group: "edit",
        // A checked row rather than a label that flips between keeping and not
        // keeping: a menu states the STATE and Word and Docs both do it this
        // way, and a control whose name changes under a screen-reader user is
        // the thing the action bar's note was avoiding. `aria-checked` is set on
        // the rendered row below.
        label: t("versionPanel.keepThisVersion"),
        // The tick in the icon gutter every row already reserves, so the state
        // is visible as well as announced — `aria-checked` alone is a state only
        // a screen reader can read.
        icon: row.pinned ? "accept" : undefined,
        run: () => void queue(() => setPinned(id, !row.pinned)),
      },
      {
        id: "version.changes",
        group: "edit",
        label: t("versionPanel.showChanges"),
        enabled: false,
        disabledReason: t("versionHistory.action.showChangesUnavailable"),
        run: () => {},
      },
      {
        id: "version.delete",
        group: "danger",
        label: t("versionPanel.deleteThisVersion"),
        shortcut: DELETE_KEY,
        danger: true,
        enabled: !isHead,
        disabledReason: t("versionHistory.headNotDeletable"),
        run: () => void queue(() => removeVersion(id)),
      },
    ];
  }

  /** The one menu element, made once and reused: a menu per row would be one
   *  detached popup per version in the timeline. */
  function ensureRowMenu() {
    if (rowMenuEl) return rowMenuEl;
    rowMenuEl = document.createElement("div");
    rowMenuEl.id = "versionRowMenu";
    rowMenuEl.className = "context-menu version-row-menu";
    rowMenuEl.hidden = true;
    rowMenuEl.setAttribute("role", "menu");
    rowMenuEl.addEventListener("keydown", onRowMenuKey);
    document.body.append(rowMenuEl);
    return rowMenuEl;
  }

  /**
   * Opens a row's menu, anchored under its ⋮ or at a right-click point.
   *
   * Reuses the editor's context-menu machinery whole — `normalizeMenuEntries`
   * for the separators, `renderMenuLevel` for the rows, `focusMenuIndex` for the
   * roving focus and `clampContextMenuPosition` so it never opens off-screen —
   * rather than growing a second menu implementation beside it.
   *
   * O(entries): five rows, no document access.
   */
  function openRowMenu(versionId, at = null) {
    const row = rows.find((candidate) => candidate.versionId === versionId);
    if (!row) return;
    closeRowMenu();
    const el = ensureRowMenu();
    const entries = normalizeMenuEntries(rowMenuEntries(row));
    el.setAttribute("aria-label", t("versionPanel.actionsSelectedVersion.label"));
    el.hidden = false;
    renderMenuLevel(el, entries, 0, MENU_HOOKS);
    for (const item of el.querySelectorAll('[data-command-id="version.keep"]')) {
      item.setAttribute("role", "menuitemcheckbox");
      item.setAttribute("aria-checked", String(Boolean(row.pinned)));
    }
    menuVersionId = versionId;
    menuButton =
      body.querySelector(`#${CSS.escape(optionId(versionId))} .version-item-menu`) ?? null;
    menuButton?.setAttribute("aria-expanded", "true");
    // Right-aligned under the ⋮, which is where a menu button's menu belongs;
    // a right-click uses the pointer instead.
    const box = menuButton?.getBoundingClientRect();
    const anchor = at ?? {
      x: (box?.right ?? 0) - el.offsetWidth,
      y: (box?.bottom ?? 0) + 2,
    };
    const position = clampContextMenuPosition(
      anchor.x,
      anchor.y,
      el.offsetWidth,
      el.offsetHeight,
      window.innerWidth,
      window.innerHeight,
    );
    el.style.left = `${position.left}px`;
    el.style.top = `${position.top}px`;
    menuLevel = { el, entries, index: -1, parentButton: null };
    focusMenuIndex(menuLevel, moveMenuIndex(entries, -1, 1));
  }

  /** Closes it. Idempotent, because a re-render, a light dismiss, an activation,
   *  Escape and closing the panel all reach it. */
  function closeRowMenu({ restoreFocus = false } = {}) {
    if (!rowMenuEl || rowMenuEl.hidden) return;
    rowMenuEl.hidden = true;
    rowMenuEl.replaceChildren();
    menuLevel = null;
    menuVersionId = "";
    const button = menuButton;
    menuButton = null;
    button?.setAttribute("aria-expanded", "false");
    if (!restoreFocus) return;
    if (button?.isConnected) button.focus({ preventScroll: true });
    else focusGrid();
  }

  /** The keyboard inside the menu, the same contract the editor's context menu
   *  keeps: arrows move, Home/End jump, Escape and Tab close and give the
   *  keyboard back to the ⋮ it came from. */
  function onRowMenuKey(event) {
    if (!menuLevel) return;
    const { entries, index } = menuLevel;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      focusMenuIndex(menuLevel, moveMenuIndex(entries, index, event.key === "ArrowDown" ? 1 : -1));
    } else if (event.key === "Home" || event.key === "End") {
      focusMenuIndex(menuLevel, moveMenuIndex(entries, index, event.key === "Home" ? "first" : "last"));
    } else if (event.key === "Escape" || event.key === "ArrowLeft" || event.key === "Tab") {
      closeRowMenu({ restoreFocus: true });
    } else {
      return;
    }
    event.preventDefault();
  }

  /** What `menu_render.mjs` needs from its host. No submenus here, so the hover
   *  hook only moves the highlight. */
  const MENU_HOOKS = {
    shortcutText: (spec) => formatShortcut(spec),
    onHover: (_depth, index) => {
      if (menuLevel) focusMenuIndex(menuLevel, index);
    },
    onActivate: (_depth, _button, entry) => {
      if (!entry || entry.enabled === false) return;
      closeRowMenu({ restoreFocus: true });
      entry.run();
    },
  };

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
    await keepingFocus(() => showPreview(next, row));
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
    await keepingFocus(() => showPreview(null, null));
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
    for (const entry of entryPoints) entry.setAttribute("aria-pressed", "true");
    onOpenChange(true);
    await refresh();
    // The grid's tab stop, not the grid: a grid manages focus with a roving
    // tabindex, and landing on the container would leave a screen reader
    // announcing a list with nothing in it.
    focusGrid();
  }

  /** Closes the panel, leaves the preview behind, and gives the keyboard back to
   *  whatever opened it — or to the editing surface when that element is gone. */
  async function close() {
    if (!panel || !isOpen()) return;
    closeRowMenu();
    await closePreview();
    panel.hidden = true;
    for (const entry of entryPoints) entry.setAttribute("aria-pressed", "false");
    onOpenChange(false);
    const back = returnFocus;
    returnFocus = null;
    if (back?.isConnected) back.focus();
  }

  async function toggle() {
    if (isOpen()) await close();
    else await open();
  }

  // The grid's keyboard. Up/Down walk rows and select (which previews),
  // Left/Right walk the two cells, and the three gestures that open a row's menu
  // — Shift+F10, the ContextMenu key and Enter on the ⋮ — all reach the same
  // menu the pointer does.
  body?.addEventListener("keydown", (event) => {
    const onMenuButton =
      event.target instanceof HTMLElement && event.target.closest(".version-item-menu") !== null;
    if (event.key === "ArrowDown") moveActive(1);
    else if (event.key === "ArrowUp") moveActive(-1);
    else if (event.key === "Home") moveActive(0, 1);
    else if (event.key === "End") moveActive(0, -1);
    else if (event.key === "ArrowRight") moveCell(1);
    else if (event.key === "ArrowLeft") moveCell(-1);
    else if (event.key === "Enter" || event.key === " ") {
      // On the ⋮ the browser's own click does it; handling it here as well would
      // open the menu and then immediately toggle it shut.
      if (onMenuButton) return;
      // Enter and Space open the selection NOW, without the settle: a keyboard
      // reader who has arrowed to a row and pressed Enter has decided.
      clearTimeout(settleTimer);
      if (selectedId) void openPreview(selectedId);
    } else if ((event.key === "F10" && event.shiftKey) || event.key === "ContextMenu") {
      if (selectedId) openRowMenu(selectedId);
    } else if (matchesShortcut(NAME_KEY, event)) {
      if (selectedId) void queue(() => nameVersion(selectedId));
    } else if (matchesShortcut(DELETE_KEY, event)) {
      // The head is the document; deleting it is refused for the same reason its
      // menu row is disabled, and silently doing nothing would be worse than the
      // menu row that says so.
      if (selectedId && selectedId !== headVersionId) void queue(() => removeVersion(selectedId));
    } else if (event.key === "Escape") {
      void close();
    } else {
      return;
    }
    event.preventDefault();
  });

  // Light dismiss, on `pointerdown` — the phase every dismissable surface in this
  // editor uses, because `mousedown` never fires for a pen or a consumed touch
  // contact (`light-dismiss-contract.spec.mjs`). The ⋮ itself is excluded so its
  // own click can close a menu it already opened rather than reopening it.
  document.addEventListener(
    "pointerdown",
    (event) => {
      if (!rowMenuEl || rowMenuEl.hidden) return;
      const target = event.target instanceof Node ? event.target : null;
      if (target && (rowMenuEl.contains(target) || menuButton?.contains(target))) return;
      closeRowMenu();
    },
    true,
  );

  closeBtn?.addEventListener("click", () => void close());
  for (const entry of entryPoints) entry.addEventListener("click", () => void toggle());
  bannerBack?.addEventListener("click", () => void closePreview());
  bannerRestore?.addEventListener("click", () => void queue(() => restore(previewVersionId)));
  namedOnlyBox?.addEventListener("change", () => renderList());
  clearBtn?.addEventListener("click", () => void queue(() => clearHistory()));

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

    /**
     * A document opened, Settings changed, or the locale changed: the capture
     * interval, the ribbon entry, the row words and the disclosure all follow.
     *
     * The two ribbon/rail entries are reflected HERE rather than in `main.js`
     * because this module already owns their pressed state, and "enabled, or
     * disabled with the reason" is the same question `unavailableReason` answers
     * for the File row and the palette — one function, so the four surfaces
     * cannot come to disagree about why version history is unavailable.
     *
     * The rows are rebuilt rather than left stale because their day headings,
     * origins and timestamps are all locale-shaped: a panel that stayed in the
     * previous language would be the one surface a relabel missed. O(rows).
     */
    reflect() {
      capturePolicy.intervalMs = retention().intervalMs;
      const reason = unavailableReason();
      for (const entry of entryPoints) {
        entry.disabled = Boolean(reason);
        entry.title = reason || t("versionHistory.command");
      }
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
