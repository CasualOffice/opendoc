import {
  SKIP_ROW,
  categoryRow,
  renderExportPane,
  renderFilePageInfo,
  renderTemplatePane,
} from "./command_menu.mjs";

// The File page's content pane.
//
// ONLYOFFICE's File page is a rail plus a pane, and the pane is never empty:
// `FileMenu.js:390` opens it on `saveas` when the document can be downloaded
// and on `info` otherwise. Ours had a rail and three quarters of a blank
// window. This is the pane.
//
// It takes its collaborators rather than reaching for them, so which pane is
// showing and what each one renders can be tested without an engine or a
// document behind it.

/** Which pane the File page is showing. `export` by default, because that is
 *  the one ONLYOFFICE opens on whenever the document can be downloaded
 *  (`FileMenu.js:390`: `canDownload ? 'saveas' : 'info'`), and ours always can. */
let filePane = "export";

/** The File page's content pane.
 *
 *  ONLYOFFICE's page is a rail plus a pane and the pane is never empty; ours
 *  had a rail and three quarters of a blank window. The rail's category rows
 *  choose what shows here.
 */
export function renderFilePane(deps) {
  const {
    menuRegistry,
    exportRows,
    formatInfoOf,
    closeFilePage,
    prepare,
    templates,
    onTemplate,
    templateThumbnailFor,
  } = deps;
  const host = document.getElementById("filePageDetail");
  if (!host) return;
  // ALWAYS return whatever panel is currently borrowed, before anything else
  // writes into the pane. Without this, switching from one panel pane straight
  // to another — Document properties to Settings, say — left the first still
  // parented here when `replaceChildren` ran, and `replaceChildren` does not
  // move a node out, it DROPS it: the settings form vanished from the document
  // permanently and every later attempt to show it found nothing. Releasing
  // and immediately re-borrowing the same panel is harmless, which is why this
  // is unconditional rather than a comparison that has to stay correct.
  releaseSettingsPanel();
  if (filePane === "new") {
    return renderTemplatePane(host, templates, onTemplate, templateThumbnailFor);
  }
  const panelPane = PANE_BY_ID[filePane];
  if (panelPane) {
    // Every one of these panels fills itself when its DIALOG opens — the
    // shortcuts reference is built on open, the properties form is loaded from
    // the document on open, the About version is stamped on open. Shown as a
    // pane none of that ran, so the shortcuts pane came up empty and the
    // properties pane would have shown the previous document's values. The
    // host passes the same preparation in, so a pane and a dialog show the
    // same thing.
    // Placed FIRST, prepared second: `showPanelInFilePane` moves the panel
    // element into the pane, and moving a node blurs whatever inside it had
    // the keyboard — so a `prepare` that focuses (the command pane's search
    // field) has to run after the move, not before it.
    showPanelInFilePane(host, panelPane.panel, panelPane);
    prepare?.(panelPane.id);
    return;
  }
  const registry = menuRegistry();
  renderExportPane(
    host,
    exportRows.map((row) => ({
      ...formatInfoOf(row.format),
      id: row.id,
      icon: row.icon,
      enabled: registry.get(row.id)?.enabled !== false,
      reason: registry.get(row.id)?.reason,
    })),
    (format) => {
      closeFilePage();
      registry.get(format.id)?.run();
    },
  );
}

/** The File rows that become PANES instead of opening a dialog over the page.
 *
 *  The owner's rule, in their words: "basically in this view replace dialogs
 *  with this space". It is also ONLYOFFICE's — their File page has no dialogs
 *  at all; Advanced Settings, document Info and Help are panes
 *  (`FileMenu.js:412-415`). A modal over a full-window page is two layers of
 *  chrome between a person and the document they came for, and the Settings
 *  one was taller than the window, so scrolling it scrolled the page behind.
 *
 *  Keyed by the command the rail row would otherwise run, so a row and its
 *  pane cannot drift apart. `panel` is the element MOVED into the pane and put
 *  back on close — one form in the document, never a copy. */
export const PANEL_PANES = Object.freeze({
  "view.settings": {
    id: "settings",
    label: "Settings",
    blurb: "Appearance, your reviewer identity, autosave and proofing.",
    panel: "settingsPanel",
  },
  "file.properties": {
    id: "properties",
    label: "Document properties",
    blurb: "Title, author and the other metadata saved with the file.",
    panel: "propertiesPanel",
  },
  "help.shortcuts": {
    id: "shortcuts",
    label: "Keyboard shortcuts",
    blurb: "Every command that has one.",
    panel: "shortcutsDialog",
  },
  "help.about": {
    id: "about",
    label: "About OpenDoc",
    blurb: "Version, licence and where the source lives.",
    panel: "aboutDialog",
  },
  "file.new": { id: "new", label: "New document", panel: null },
  "help.commands": {
    id: "commands",
    label: "Find a command",
    blurb: "Search everything the editor can do.",
    panel: "cmdPalette",
  },
});

const PANE_BY_ID = Object.fromEntries(
  Object.values(PANEL_PANES)
    .filter((pane) => pane.panel)
    .map((pane) => [pane.id, pane]),
);

/** A panel, shown IN the page rather than as a dialog over it.
 *
 *  The owner's report: the dialog is taller than the window and scrolling it
 *  scrolls the whole page behind it. ONLYOFFICE has no settings dialog at all —
 *  Advanced Settings is a File-page pane (`FileMenu.js:412`). The panel element
 *  is MOVED here rather than duplicated, and put back when the page closes, so
 *  there is still exactly one settings form in the document. */
function showPanelInFilePane(host, panelId, { label, blurb } = {}) {
  const panel = document.getElementById(panelId);
  if (!panel) return;
  if (!settingsHome) {
    settingsHome = { panel, parent: panel.parentElement, next: panel.nextSibling };
  }
  panel.hidden = false;
  panel.classList.add("panel-in-page");
  // Borrowed, it is a section of a page, not a modal dialog: a `role="dialog"`
  // with `aria-modal` inside the page would tell a screen reader the rest of
  // the window is unavailable, and its own `aria-labelledby` points at the
  // title the pane hides. The pane's heading becomes the accessible name.
  if (!paneRoles) {
    paneRoles = {
      role: panel.getAttribute("role"),
      modal: panel.getAttribute("aria-modal"),
      labelledBy: panel.getAttribute("aria-labelledby"),
      label: panel.getAttribute("aria-label"),
    };
  }
  panel.setAttribute("role", "group");
  panel.removeAttribute("aria-modal");
  panel.removeAttribute("aria-label");
  panel.setAttribute("aria-labelledby", FILE_PANE_HEADING_ID);
  // The pane's own header, not the panel's. Every pane — Export, New, and each
  // borrowed panel — opens with the same title and the same measure beneath
  // it, so the right side reads as one surface instead of four dialogs that
  // happen to be parked in the same place.
  const heading = document.createElement("h2");
  heading.id = FILE_PANE_HEADING_ID;
  heading.textContent = label ?? "";
  const sub = document.createElement("p");
  sub.className = "file-detail-sub";
  sub.textContent = blurb ?? "";
  const body = document.createElement("div");
  body.className = "file-pane-body";
  body.appendChild(panel);
  host.replaceChildren(heading, sub, body);
}

/** Where the relocated panel came from, so closing the page puts it back. */
let settingsHome = null;

/** The dialog attributes a borrowed panel had before the pane took it over. */
let paneRoles = null;

/** The pane heading a borrowed panel is named by while it is in the page. */
const FILE_PANE_HEADING_ID = "filePaneHeading";

/** Returns the settings panel to its own place in the document. */
export function releaseSettingsPanel() {
  if (!settingsHome) return;
  const panel = settingsHome.panel;
  if (!panel) return;
  panel.classList.remove("panel-in-page");
  panel.hidden = true;
  if (paneRoles) {
    const restore = (name, value) =>
      value === null ? panel.removeAttribute(name) : panel.setAttribute(name, value);
    restore("role", paneRoles.role);
    restore("aria-modal", paneRoles.modal);
    restore("aria-labelledby", paneRoles.labelledBy);
    restore("aria-label", paneRoles.label);
    paneRoles = null;
  }
  settingsHome.parent?.insertBefore(panel, settingsHome.next);
  settingsHome = null;
}


/** Which pane is showing, and how to change it. */
export function currentFilePane() {
  return filePane;
}

export function setFilePane(next) {
  filePane = next;
}

/** The rail row for one File command, when the page renders that command as a
 *  PANE instead of as a row that runs.
 *
 *  Positional, not appended: the row stands exactly where the command it
 *  replaces stood, so the page and the compact chrome's File dropdown list the
 *  same things in the same order. Export is the one row that speaks for
 *  several commands — ONLYOFFICE's shape, one `Save as` opening a pane of
 *  format tiles rather than six rows in the rail — so it claims the first
 *  export id in its section and skips the rest.
 */
export function fileCategoryRowFor(id, ids, { itemClass, onSelect }) {
  if (id.startsWith("file.export.")) {
    const formats = (ids ?? []).filter((candidate) => candidate.startsWith("file.export."));
    if (id !== formats[0]) return SKIP_ROW;
    return categoryRow({
      id: "export",
      label: "Export",
      itemClass,
      selected: filePane === "export",
      onSelect,
      covers: formats,
    });
  }
  const pane = PANEL_PANES[id];
  if (!pane) return null;
  return categoryRow({
    id: pane.id,
    label: pane.label,
    itemClass,
    selected: filePane === pane.id,
    onSelect,
    covers: [id],
  });
}
