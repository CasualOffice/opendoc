// SPDX-License-Identifier: Apache-2.0
// The plugin platform in the editor: catalogues, the grant prompt, Manage
// add-ins, the Add-ins menu, the ✦ selection menu, the side panel and the
// commands plugins contribute (`docs/183` §6.5, §8, §9; ADR-077).
//
// DESIGNED FROM THE PRODUCTS PEOPLE KNOW, and held to the owner's rule of
// 2026-10-31: a plugin adds NOTHING to the editor until it runs, and then only
// inside chrome that already exists.
//
//   * The Add-ins menu button (Review ▸ Proofing) is hidden until a running
//     plugin contributes a ribbon row. Word has one Add-ins button and Docs one
//     Extensions menu; a button per plugin is how a ribbon overflows.
//   * The ✦ item in the EXISTING selection toolbar is hidden until a running
//     plugin contributes a selection action (Docs' Refine bar, Word's
//     "Edit with Copilot" on the mini toolbar).
//   * A plugin's panel is an ordinary `.side-panel` in the workarea, the slot the
//     outline, pages and compare panels use: same frame, head, close button and
//     width. One at a time.
//   * Palette rows: every command of a running plugin.
//   * A first-party plugin marked `when: "ai"` (the assistant) does not exist at
//     all until an AI provider is configured: no row, no button, no panel.
//
// Every plugin command is `plugin.<pluginId>.<commandId>` in the one registry,
// so it reaches the palette and the host contract (the `plugin.` family) like
// any other command. A plugin that is turned off has no rows at all (Manage
// add-ins turns it on); one that is on but not running yet keeps its rows,
// disabled with the reason (SKILL §10).
//
// Complexity: O(plugins) per registry build; nothing here reads the document.
import { t, has, activeLocale, direction } from "./i18n.mjs";
import { registerModal } from "./modal.mjs";
import { onButton, registerPopover, closePopover, openPopover } from "./popover_manager.mjs";
import { commandContract } from "./host_contract.mjs";
import { fetchVocabulary } from "./ai_webmcp.mjs";
import { MANIFEST_BOUNDS, addedPermissions, checkManifest, localised } from "./plugin_manifest.mjs";
import { startPlugin } from "./plugin_host.mjs";
import { createAddinProofing } from "./addin_proofing.mjs";
import { attachPanelResize } from "./plugin_panel_width.mjs";
import { disclosureKey } from "./ai_settings.mjs";
import { createCoEditor } from "./ai_coeditor.mjs";
import { registerAiLive } from "./ai_live.mjs";
import { capabilityLine, loadAiEditSetting, saveAiEditSetting } from "./ai_edit_mode.mjs";
import { addCommandSource, shown } from "./plugin_command_sources.mjs";
import {
  addInstalled,
  grantFor,
  loadCatalogues,
  removeInstalled,
  revokeGrant,
  storageKeyFor,
  storeGrant,
} from "./plugin_catalogue.mjs";

/** The Manage add-ins command, the family's one fixed member. */
export const MANAGE_COMMAND = "plugin.manage";
/** ⌘/ (Ctrl+/): the ✦ actions on a selection, or the first AI plugin's panel
 *  with none. ONLYOFFICE's own chord for its AI panel. Present only while such a
 *  plugin runs. */
export const ACTIONS_COMMAND = "plugin.actions";

/** Next, Previous and Show issue over an add-in's writing issues (`docs/183`
 *  §19): ⌘' and ⌘; as in Google Docs, ⌥⏎ for the card at the caret. Present only
 *  while an add-in that proofreads runs. */
export const PROOFING_NEXT_COMMAND = "plugin.proofing.next";
export const PROOFING_PREVIOUS_COMMAND = "plugin.proofing.previous";
export const PROOFING_OPEN_COMMAND = "plugin.proofing.open";

/** The palette rows for add-in proofing, or none. Pure over `proofing`. */
export function proofingCommands(proofing) {
  if (!proofing?.running()) return [];
  const none = proofing.count() === 0;
  const reason = t("proofing.addin.none");
  return [
    { id: PROOFING_NEXT_COMMAND, label: t("proofing.addin.next"), group: "Review", kw: "next spelling grammar issue mistake languagetool readability", noDoc: false, enabled: !none, disabledReason: reason, run: () => proofing.next() },
    { id: PROOFING_PREVIOUS_COMMAND, label: t("proofing.addin.previous"), group: "Review", kw: "previous spelling grammar issue mistake languagetool readability", noDoc: false, enabled: !none, disabledReason: reason, run: () => proofing.previous() },
    { id: PROOFING_OPEN_COMMAND, label: t("proofing.addin.open"), group: "Review", kw: "show issue suggestion card spelling grammar", noDoc: false, enabled: !none, disabledReason: reason, run: () => proofing.openAtCaret() },
  ];
}

/** A plugin command's registry id. */
export const pluginCommandId = (pluginId, commandId) => `plugin.${pluginId}.${commandId}`;

/** The sentence key for each permission in the grant prompt (`docs/183` §6.1). */
export const PERMISSION_KEYS = Object.freeze({
  read: "plugins.permission.read",
  comment: "plugins.permission.comment",
  suggest: "plugins.permission.suggest",
  edit: "plugins.permission.edit",
  network: "plugins.permission.network",
  ai: "plugins.permission.ai",
});

/** The design tokens a plugin's page receives as values (owner rule 3: reuse the
 *  product's tokens, never invent). Read from the editor's own computed style, so
 *  both themes and a white-label `brand.css` reach the plugin unchanged. */
export const PLUGIN_TOKENS = Object.freeze([
  "--accent",
  "--accent-ink",
  "--accent-soft",
  "--accent-line",
  "--accent-text",
  "--bg",
  "--bg-2",
  "--ink",
  "--muted",
  "--surface",
  "--surface-alt",
  "--paper",
  "--paper-ink",
  "--paper-insert",
  "--paper-delete",
  "--line",
  "--line-strong",
  "--danger",
  "--danger-soft",
  "--primary-bg",
  "--primary-bg-hover",
  "--primary-ink",
  "--radius",
  "--radius-sm",
  "--radius-md",
  "--radius-popover",
  "--space-1",
  "--space-2",
  "--space-3",
  "--space-4",
  "--fs-body",
  "--fs-small",
  "--fs-caption",
  "--h-control",
  "--h-field",
  "--h-search",
  "--pnl-pad-x",
  "--pnl-pad-y",
  "--shadow-sm",
  "--shadow-popover",
  "--brand-font",
  // Design system v2 (`docs/63` §7, `109` UX-052). The pre-v2 names above stay
  // published as aliases of these, so a plugin written against either reads the
  // same values; new plugin CSS uses these.
  "--fs-xs",
  "--fs-sm",
  "--fs-md",
  "--fs-lg",
  "--fw-regular",
  "--fw-medium",
  "--fw-semibold",
  "--r-xs",
  "--r-sm",
  "--r-md",
  "--r-lg",
  "--r-pill",
  "--h-dense",
  "--h-tool",
  "--h-ctl",
  "--h-input",
  "--e1",
  "--e2",
  "--e3",
]);

/** A short, stable, opaque key for a string (FNV-1a, 32-bit, hex). Pure. */
export function opaqueKey(text) {
  let hash = 0x811c9dc5;
  for (const char of String(text ?? "")) {
    hash ^= char.codePointAt(0);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

/** The catalogue namespace a first-party plugin's strings live under:
 *  `plugins.<last id segment>.`, e.g. `plugins.assistant.`. Pure. */
export const stringNamespace = (pluginId) => `plugins.${String(pluginId).split(".").at(-1)}.`;

/**
 * The command rows the plugins contribute. Pure over its inputs.
 *
 * @param {{plugins: object[], context?: object, openManager: () => void,
 *   invoke: (plugin: object, command: object) => void, actions?: (() => void) | null}} io
 *   each plugin: `{manifest, running, enabled, name, title(command)}`
 */
export function pluginCommands({ plugins, context = {}, openManager, invoke, actions = null }) {
  const rows = [
    {
      id: MANAGE_COMMAND,
      label: t("plugins.manage.command"),
      group: "Tools",
      kw: "add-ins addins plugins extensions install manage sandbox",
      noDoc: true,
      run: openManager,
    },
  ];
  if (actions) {
    rows.push({
      id: ACTIONS_COMMAND,
      label: t("plugins.actions.command"),
      group: "Tools",
      kw: "ai assistant casual actions rewrite ask",
      noDoc: false,
      run: actions,
    });
  }
  for (const plugin of plugins) {
    const { manifest } = plugin;
    const inMenu = new Map(manifest.contributes.contextMenu.map((item) => [item.command, item.when ?? "always"]));
    for (const command of manifest.contributes.commands) {
      const when = inMenu.get(command.id);
      const hasRange = context.hasRange ?? false;
      rows.push({
        id: pluginCommandId(manifest.id, command.id),
        label: t("plugins.commandLabel", { plugin: plugin.name, command: plugin.title(command) }),
        group: "Add-ins",
        kw: `add-in plugin ${plugin.name} ${plugin.title(command)}`,
        noDoc: true,
        contextMenu: when !== undefined && (when === "always" || (when === "selection" ? hasRange : !hasRange)),
        enabled: plugin.running,
        disabledReason: t(plugin.enabled ? "plugins.notRunning" : "plugins.turnOnFirst", { name: plugin.name }),
        run: () => invoke(plugin, command),
      });
    }
  }
  return rows;
}

/**
 * Starts the platform.
 *
 * @param {object} ed the editor context
 * @param {object} io
 * @param {object} io.assistant `editor.ai`
 * @param {object} io.providers the AI provider service
 * @param {object} io.config `hostConfig()`
 * @param {() => void} [io.openAiSettings] Settings ▸ AI provider, for the ⋯ menu
 * @param {() => object} [io.aiAccess] the effective AI access (`ai_platform.mjs`)
 * @param {object} [io.view]
 */
export function startPluginPlatform(ed, { assistant, providers, config, openAiSettings = null, aiAccess = null, view = globalThis }) {
  const doc = view.document;
  // Casual AI as a live co-editor (`docs/183` §23): one per editor, painted from
  // the editor's overlay repaint (`editor_proofing.mjs`).
  const effective = () => (aiAccess ? aiAccess() : { mode: "chat", setting: "suggest", badge: "readonly", direct: { allowed: false, reason: null }, reason: null, protectedLevel: null });
  const coeditor = createCoEditor(ed, { t, effective, view });
  /** The access as an add-in reads it: the effective mode, the badge and the
   *  model-facing capability line (fixed English). */
  const describeAccess = () => {
    const access = effective();
    return {
      ...access,
      capability: capabilityLine(access),
      // Said in the person's language by the editor: the add-in shows these as
      // they are (its sandbox has no catalogue of the editor's own keys).
      badgeText: t(`ai.access.badge.${access.badge}`),
      reasonText: access.reason ? t(access.reason) : "",
      directReasonText: access.direct.allowed ? "" : t(access.direct.reason ?? "ai.access.reason.viewer"),
    };
  };
  let lastAccess = "";
  /** Tells running add-ins when what the AI may do changed (a mode switch, a
   *  protection, the setting). Compared as JSON, so an unchanged state is silent. */
  function accessChanged() {
    const now = describeAccess();
    const key = JSON.stringify(now);
    if (key === lastAccess) return;
    lastAccess = key;
    platform.notifyAll("access/changed", now);
  }
  registerAiLive({ editor: coeditor, accessChanged });
  /** id -> {manifest, entry, running: host|null, enabled} */
  const plugins = new Map();
  let catalogue = { mode: "none", entries: [], allowInstall: false, problems: [] };
  let activePanel = null;
  const session = () => ed.hostSession ?? null;
  // Writing issues in the text (`docs/183` §19): drawn by the host through the
  // built-in spell check's overlay, counted in the status bar.
  const proofing = createAddinProofing(ed, { view });
  const issuesChip = doc.getElementById("addinIssuesStatus");
  function reflectIssues() {
    if (!issuesChip) return;
    if (!proofing.running()) {
      issuesChip.hidden = true;
      return;
    }
    const count = proofing.count();
    const summaries = proofing.summaries();
    issuesChip.hidden = false;
    issuesChip.textContent = t("proofing.addin.count", { count });
    const lines = [t("proofing.addin.countTitle", { names: summaries.map((p) => p.name).join(", ") })];
    for (const { name, summary } of summaries) if (summary) lines.push(`${name}: ${summary}`);
    issuesChip.title = lines.join("\n");
  }
  proofing.subscribe(reflectIssues);
  issuesChip?.addEventListener("click", () => {
    if (proofing.count()) proofing.next();
  });

  const fetchText = async (url, maxBytes) => {
    const response = await view.fetch(url, { credentials: "omit", referrerPolicy: "no-referrer" });
    if (!response.ok) throw new Error(String(response.status));
    const text = await response.text();
    if (new TextEncoder().encode(text).length > maxBytes) throw new Error("too-large");
    return text;
  };
  const fetchJson = async (url) => JSON.parse(await fetchText(url, MANIFEST_BOUNDS.manifestBytes));

  /** A FIRST-PARTY plugin's words come from the product's own catalogue
   *  (`plugins.<name>.name`, `.command.<id>`, `.panel`), translated with the
   *  editor's; any other plugin's from its manifest's `locales`. */
  const firstParty = (manifest) => plugins.get(manifest.id)?.entry.source === "first-party";
  const catalogued = (manifest, key, fallback) => {
    const full = `${stringNamespace(manifest.id)}${key}`;
    return firstParty(manifest) && has(full) ? t(full) : fallback;
  };
  const nameOf = (manifest) => catalogued(manifest, "name", localised(manifest, "name", activeLocale(), manifest.name));
  const titleOf = (manifest) => (command) =>
    catalogued(manifest, `command.${command.id}`, localised(manifest, `commands.${command.id}`, activeLocale(), command.title));
  /** An AI-gated plugin exists only while a provider is configured. */
  const present = (record) => record.entry.when !== "ai" || providers.available();
  const isRunning = (record) => !!record.running && record.running.state === "running";

  function storageFor(id) {
    const key = storageKeyFor(id);
    return {
      get() {
        try {
          return JSON.parse(view.localStorage.getItem(key) ?? "null");
        } catch {
          return null;
        }
      },
      set(value) {
        try {
          view.localStorage.setItem(key, JSON.stringify(value));
        } catch {
          /* storage refused: the value lives as long as the call */
        }
      },
    };
  }

  function tokens() {
    const style = view.getComputedStyle?.(doc.documentElement);
    const out = {};
    if (!style) return out;
    for (const name of PLUGIN_TOKENS) {
      const value = style.getPropertyValue(name).trim();
      if (value) out[name] = value;
    }
    return out;
  }
  const theme = () => doc.documentElement.dataset.theme ?? (view.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light");

  // ---- The side panel (the existing `.side-panel` slot) --------------------

  const panel = doc.getElementById("pluginPanel");
  const panelTitle = doc.getElementById("pluginPanelTitle");
  const panelBody = doc.getElementById("pluginPanelBody");
  const panelClose = doc.getElementById("pluginPanelClose");
  const panelMore = doc.getElementById("pluginPanelMore");
  const panelMenu = doc.getElementById("pluginPanelMenu");
  const panelResize = doc.getElementById("pluginPanelResize");
  let panelOpener = null;

  // Docked on the right at full height, with a splitter on its inner edge
  // (`docs/183` §21). The width and the status bar's height are custom
  // properties on the root, because the footer and the private preview move in
  // beside the panel by the same amount.
  const root = doc.documentElement;
  let reanchorFrame = 0;
  const applyWidth = (width) => {
    root.style.setProperty("--plugin-panel-w", `${width}px`);
    const footer = doc.querySelector("body > .footer, .footer");
    const foot = footer && footer.getClientRects().length ? Math.round(footer.getBoundingClientRect().height) : 0;
    root.style.setProperty("--plugin-panel-foot", `${foot}px`);
    // The page column changed width: the overlays that follow the pages
    // re-anchor once per frame, not once per pointer event.
    if (!reanchorFrame) {
      reanchorFrame = view.requestAnimationFrame?.(() => {
        reanchorFrame = 0;
        ed.reanchorPageOverlays?.();
      }) ?? 0;
    }
  };
  const sizer = panel && panelResize
    ? attachPanelResize({ handle: panelResize, panel, apply: applyWidth, rtl: () => direction(activeLocale()) === "rtl" || doc.documentElement.dir === "rtl", view })
    : null;

  function showPanel(record, open) {
    if (!panel || !record?.running?.frame) return;
    if (!open) {
      if (activePanel !== record) return;
      activePanel = null;
      panel.hidden = true;
      doc.body.classList.remove("plugin-panel-open");
      if (panelMenuPopover) closePopover(panelMenuPopover);
      record.running.frame.hidden = true;
      ed.reanchorPageOverlays?.();
      const back = shown(panelOpener);
      panelOpener = null;
      if (back) back.focus({ preventScroll: true });
      else ed.focusEditorSurface?.();
      return;
    }
    if (activePanel !== record) panelOpener = doc.activeElement instanceof view.HTMLElement ? doc.activeElement : null;
    for (const other of plugins.values()) if (other.running?.frame) other.running.frame.hidden = other !== record;
    activePanel = record;
    panelTitle.textContent = catalogued(record.manifest, "panel", localised(record.manifest, "panel.title", activeLocale(), record.manifest.contributes.panel.title));
    panelClose.setAttribute("aria-label", t("plugins.panel.close", { name: nameOf(record.manifest) }));
    panelClose.title = panelClose.getAttribute("aria-label");
    if (panelMore) panelMore.hidden = panelMenuRows(record).length === 0;
    // One panel per side (approved design P1, 2026-11-02): this panel is on
    // the right, so it replaces the other right-hand one (the review sidebar)
    // and leaves a left-hand Outline or Pages open.
    if (ed.reviewSidebar && !ed.reviewSidebar.hidden) ed.toggleReview?.(false);
    panel.hidden = false;
    doc.body.classList.add("plugin-panel-open");
    sizer?.reclamp();
    ed.reanchorPageOverlays?.();
    record.running.frame.focus?.();
  }
  panelClose?.addEventListener("click", () => activePanel && showPanel(activePanel, false));

  // ---- The panel's ⋯ menu ----------------------------------------------------
  //
  // One header line (owner, 2026-10-31: "too crowded, redundant information"):
  // what is not the conversation lives behind ⋯ — the add-in's own panel
  // commands (`contributes.panel.menu`, Casual AI's Clear history) and, for an
  // add-in holding `ai`, where its text goes and the way to change that. The
  // disclosure is the HOST's sentence from the provider's own description
  // (`disclosureKey`), so an add-in cannot word it.

  /** The ⋯ rows for `record`: `[{kind: "command"|"info"|"settings", …}]`. */
  function panelMenuRows(record) {
    const rows = (record.manifest.contributes.panel?.menu ?? []).map((item) => ({ kind: "command", item }));
    if (record.running?.grant().permissions.includes("ai")) {
      const held = record.running.grant().permissions;
      if (record.entry.source === "first-party" && (held.includes("suggest") || held.includes("edit"))) rows.push({ kind: "editAs" });
      rows.push({ kind: "info" });
      if (openAiSettings) rows.push({ kind: "settings" });
    }
    return rows;
  }

  function panelMenuItems(record, close) {
    return panelMenuRows(record).map((row) => {
      if (row.kind === "command") return menuRow(record, row.item, close);
      const button = doc.createElement("button");
      button.type = "button";
      button.className = "menu-item";
      button.setAttribute("role", "menuitem");
      const icon = doc.createElement("span");
      icon.className = "ms";
      icon.setAttribute("aria-hidden", "true");
      const label = doc.createElement("span");
      if (row.kind === "editAs") {
        // The quick toggle for "AI edits as" (Settings ▸ AI provider is the full
        // control). A checked menu item: Direct edits, on or off. Greyed out WITH
        // the reason when the document or the role caps the AI at suggestions.
        const access = effective();
        const direct = loadAiEditSetting(view) === "direct";
        icon.textContent = direct ? "edit" : "rate_review";
        label.textContent = t("plugins.panel.editAs");
        button.setAttribute("role", "menuitemcheckbox");
        button.setAttribute("aria-checked", String(direct && access.direct.allowed));
        button.dataset.panelMenu = "editAs";
        if (!access.direct.allowed) {
          button.setAttribute("aria-disabled", "true");
          button.title = t(access.direct.reason ?? "ai.access.reason.viewer");
          const why = doc.createElement("small");
          why.className = "menu-item-reason";
          why.textContent = button.title;
          label.append(doc.createElement("br"), why);
        } else {
          onButton(button, () => {
            close();
            saveAiEditSetting(direct ? "suggest" : "direct", view);
            accessChanged();
            ed.setStatus(t(direct ? "ai.editAs.nowSuggest" : "ai.editAs.nowDirect"));
          });
        }
      } else if (row.kind === "info") {
        // Information, not an action: announced in the menu, never actionable.
        const [key, params] = disclosureKey(providers.describe());
        icon.textContent = "info";
        label.textContent = t(key, params);
        button.setAttribute("aria-disabled", "true");
        button.classList.add("menu-item-note");
        button.dataset.panelMenu = "disclosure";
      } else {
        icon.textContent = "settings";
        label.textContent = t("plugins.panel.aiSettings");
        button.dataset.panelMenu = "aiSettings";
        onButton(button, () => {
          close();
          openAiSettings();
        });
      }
      button.append(icon, label);
      return button;
    });
  }

  let panelMenuPopover = null;
  if (panelMore && panelMenu) {
    panelMenuPopover = registerPopover(
      panelMore,
      panelMenu,
      () => panelMenu.replaceChildren(...(activePanel ? panelMenuItems(activePanel, () => closePopover(panelMenuPopover)) : [])),
      { needsDocument: false },
    );
  }
  panel?.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && activePanel) {
      event.preventDefault();
      showPanel(activePanel, false);
    }
  });

  // ---- Running a plugin ------------------------------------------------------

  /** Plain text to the clipboard: the async API, else a hidden field and the
   *  copy command (an older engine, or a page without the permission). */
  async function copyText(text) {
    try {
      await view.navigator.clipboard.writeText(text);
      return true;
    } catch {
      const field = doc.createElement("textarea");
      field.value = text;
      field.setAttribute("readonly", "");
      field.className = "sr-only";
      doc.body.append(field);
      field.select();
      let done = false;
      try {
        done = doc.execCommand("copy");
      } catch {
        done = false;
      }
      field.remove();
      return done;
    }
  }

  function granted(record) {
    if (record.entry.hostGrant) return [...record.entry.hostGrant];
    return grantFor(record.manifest.id, view)?.permissions ?? [];
  }

  /** Puts the caret at the start of a paragraph the plugin read, and shows it. */
  function reveal(node) {
    if (!ed.doc) return false;
    try {
      ed.selection = { anchor: { node, offset: 0 }, focus: { node, offset: 0 } };
      ed.drawSelection?.();
      ed.scrollReviewSelectionIntoView?.();
      ed.focusEditorSurface?.();
      return true;
    } catch {
      return false;
    }
  }

  function run(record) {
    if (record.running || !panelBody || !record.enabled || !present(record)) return;
    const namespace = stringNamespace(record.manifest.id);
    record.running = startPlugin({
      manifest: record.manifest,
      granted: () => granted(record),
      container: panelBody,
      env: {
        access: () => assistant.access(),
        ai: assistant,
        execute: (id) => session()?.execute(id) ?? { ok: false, refusal: { code: "unavailable" } },
        contractRow: (id) => commandContract(id),
        commandIds: () => (session()?.describe().commands ?? []).map((command) => command.id),
        providers,
        loadVocabulary: fetchVocabulary,
        selection: () => {
          if (!ed.doc || !ed.selection) return null;
          const { anchor, focus } = ed.selection;
          let text = "";
          try {
            text = ed.hasRange() ? ed.doc.copyText(anchor.node, anchor.offset, focus.node, focus.offset) : "";
          } catch {
            text = "";
          }
          // The table the caret is in, if any: "Describe this table" offers itself there.
          let table = null;
          try {
            if (ed.doc.inTable(focus.node)) {
              const info = ed.doc.tableInfo(focus.node);
              table = info?.found ? String(info.table) : null;
              info?.free?.();
            }
          } catch {
            table = null;
          }
          return { hasRange: ed.hasRange(), text, anchor: { ...anchor }, focus: { ...focus }, table };
        },
        notify: (message, kind) => ed.setStatus(t("plugins.notify", { name: nameOf(record.manifest), message }), kind === "error" ? "error" : undefined),
        panel: (open) => showPanel(record, open),
        reveal,
        // Copy, for a FIRST-PARTY plugin's answer cards: the sandbox holds no
        // clipboard, so the editor writes the text (plain text, bounded).
        copy: record.entry.source === "first-party" ? (text) => copyText(text) : null,
        // Live co-editing (`docs/183` §23), for the product's own assistant only:
        // a third-party add-in still proposes through the private preview.
        coedit: record.entry.source === "first-party" && record.entry.when === "ai" ? {
          access: describeAccess,
          begin: (spec, options) => coeditor.begin(spec, options),
          write: (turn, text, options) => coeditor.write(turn, text, options),
          replace: (turn, spec) => coeditor.replace(turn, spec),
          comment: (turn, spec) => coeditor.comment(turn, spec),
          remove: (turn, spec) => coeditor.remove(turn, spec),
          end: (turn) => coeditor.end(turn),
          stop: (turn) => coeditor.stop(turn, "add-in"),
          abort: (turn, code) => coeditor.abort(turn, code),
          decide: (turn, accept) => coeditor.decide(turn, accept),
        } : null,
        strings: (keys) => {
          if (record.entry.source !== "first-party") return {};
          const out = {};
          for (const key of keys.slice(0, 400)) if (key.startsWith(namespace)) out[key] = t(key);
          return out;
        },
        documentKey: () => opaqueKey(`${ed.draftDocKey ?? ""}|${ed.currentName ?? ""}`),
        locale: () => activeLocale(),
        direction: () => direction(activeLocale()),
        theme,
        tokens,
        config: () => record.entry.config ?? {},
        // Who its proposals are signed by: the product's own assistant signs as
        // "Casual AI"; every other add-in as "<name> (add-in)".
        addInName: () => (record.entry.source === "first-party" && record.entry.when === "ai" ? "" : nameOf(record.manifest)),
        storage: storageFor(record.manifest.id),
        fetchEntry: (url) => fetchText(url, MANIFEST_BOUNDS.entryBytes),
        proofingReport: (params) => proofing.report(record.manifest.id, params),
        onClosed: (reason) => {
          proofing.unregister(record.manifest.id);
          if (activePanel === record) showPanel(record, false);
          record.running = null;
          reflectChrome();
          if (reason === "navigated") ed.setStatus(t("plugins.navigated", { name: nameOf(record.manifest) }), "error");
          else if (reason === "entry-unavailable") ed.setStatus(t("plugins.unavailable", { name: nameOf(record.manifest) }), "error");
        },
      },
    });
    // The frame connects asynchronously; the chrome follows once it is running.
    void waitRunning(record).then(() => {
      const running = record.running;
      if (running?.state === "running" && record.manifest.contributes.proofing) {
        const held = running.grant().permissions;
        proofing.register({
          id: record.manifest.id,
          name: nameOf(record.manifest),
          canReplace: held.includes("suggest") || held.includes("edit"),
          send: (paragraphs) => record.running?.proofingCheck(paragraphs),
        });
      }
      reflectChrome();
    });
  }

  async function waitRunning(record, tries = 100) {
    for (let i = 0; i < tries && record.running && record.running.state !== "running"; i += 1) {
      await new Promise((resolve) => view.setTimeout(resolve, 50));
    }
  }

  function stop(record, reason = "disabled") {
    proofing.unregister(record.manifest.id);
    if (activePanel === record) showPanel(record, false);
    record.running?.close(reason);
    record.running = null;
    reflectChrome();
  }

  /** Starts what should run and stops what should not: called when the provider
   *  may have changed (a saved key, a host provider set late). O(plugins). */
  function refresh() {
    for (const record of plugins.values()) {
      if (record.enabled && present(record)) run(record);
      else if (record.running && !present(record)) stop(record, "provider-gone");
    }
    reflectChrome();
  }

  async function invoke(record, command) {
    if (!isRunning(record)) return;
    // An action on the selection works where the selection is, as Google Docs'
    // Refine and Notion's Edit with AI do: it does not open the side panel. The
    // add-in opens it itself (`ui/panel`) when the action needs input there.
    const onSelection = record.manifest.contributes.selection.some((item) => item.command === command.id);
    if (record.manifest.contributes.panel && !onSelection) showPanel(record, true);
    record.running.invoke(command.id, { hasRange: ed.hasRange?.() ?? false });
  }

  // ---- The grant prompt ------------------------------------------------------

  const grantDialog = doc.getElementById("pluginGrantDialog");
  let grantAnswer = null;
  let grantStaged = false;
  const grantController = grantDialog
    ? registerModal(grantDialog, {
        initialFocus: () => doc.getElementById("pluginGrantDeny"),
        fallbackFocus: () => shown(doc.getElementById("reviewAddinsBtn")) ?? ed.pagesEl,
        onClose: () => {
          const resolve = grantAnswer;
          grantAnswer = null;
          const allowed = grantStaged;
          grantStaged = false;
          resolve?.(allowed);
        },
      })
    : null;
  doc.getElementById("pluginGrantAllow")?.addEventListener("click", () => {
    grantStaged = true;
    grantController.close();
  });
  doc.getElementById("pluginGrantDeny")?.addEventListener("click", () => {
    grantStaged = false;
    grantController.close();
  });

  /** Asks the person for `permissions`. Resolves true or false. */
  function askGrant(manifest, permissions) {
    if (!grantController) return Promise.resolve(false);
    doc.getElementById("pluginGrantTitle").textContent = t("plugins.grant.title", { name: nameOf(manifest) });
    doc.getElementById("pluginGrantFrom").textContent = t("plugins.grant.from", { origin: new URL(manifest.entryUrl).host, publisher: manifest.publisher || new URL(manifest.entryUrl).host });
    const list = doc.getElementById("pluginGrantList");
    const provider = providers.describe();
    list.replaceChildren(
      ...permissions.map((permission) => {
        const item = doc.createElement("li");
        item.textContent = t(PERMISSION_KEYS[permission], {
          origins: manifest.network.map((origin) => new URL(origin).host).join(", "),
          endpoint: provider.available ? provider.endpoint || t("plugins.permission.aiHost") : t("plugins.permission.aiNone"),
        });
        return item;
      }),
    );
    const warn = doc.getElementById("pluginGrantWarning");
    warn.hidden = !permissions.includes("read");
    return new Promise((resolve) => {
      grantAnswer = resolve;
      grantStaged = false;
      grantController.open();
    });
  }

  async function enable(record) {
    if (!record.entry.hostGrant) {
      const stored = grantFor(record.manifest.id, view);
      const asking = stored ? addedPermissions(stored.permissions, record.manifest.permissions) : [...record.manifest.permissions];
      if (asking.length || !stored) {
        const allowed = await askGrant(record.manifest, asking);
        if (!allowed) return false;
      }
      storeGrant(record.manifest.id, { version: record.manifest.version, permissions: [...record.manifest.permissions], enabled: true }, view);
    } else if (record.entry.source === "first-party") {
      // The installation granted it; remember only the person's on/off choice.
      storeGrant(record.manifest.id, { version: record.manifest.version, permissions: [...record.entry.hostGrant], enabled: true }, view);
    }
    record.enabled = true;
    run(record);
    renderManager();
    return true;
  }

  function disable(record) {
    stop(record);
    record.enabled = false;
    if (record.entry.source !== "host") {
      const stored = grantFor(record.manifest.id, view);
      storeGrant(record.manifest.id, { version: record.manifest.version, permissions: stored?.permissions ?? [], enabled: false }, view);
    }
    renderManager();
  }

  function remove(record) {
    stop(record);
    revokeGrant(record.manifest.id, view);
    if (record.entry.source === "installed") {
      removeInstalled(record.entry.manifestUrl, view);
      plugins.delete(record.manifest.id);
    } else {
      record.enabled = record.entry.source === "first-party" ? record.entry.enabled : false;
      if (record.enabled) run(record);
    }
    renderManager();
  }

  // ---- Loading -------------------------------------------------------------

  async function admit(entry) {
    let manifest;
    let raw;
    try {
      raw = await fetchJson(entry.manifestUrl);
      const checked = checkManifest(raw, { manifestUrl: entry.manifestUrl, selfOrigin: view.location.origin });
      if (!checked.ok) return { ok: false, reason: checked.reason };
      manifest = checked.manifest;
    } catch {
      return { ok: false, reason: "unavailable" };
    }
    if (plugins.has(manifest.id)) return { ok: true, record: plugins.get(manifest.id) };
    const stored = entry.source === "host" ? null : grantFor(manifest.id, view);
    // A stored grant runs again on reload only while it still covers what this
    // version declares; a version that added a permission waits for the prompt.
    const covered = stored ? addedPermissions(stored.permissions, manifest.permissions).length === 0 : false;
    let enabled;
    if (entry.source === "host") enabled = entry.enabled;
    else if (entry.source === "first-party" && entry.hostGrant) enabled = stored ? stored.enabled : entry.enabled;
    else enabled = stored?.enabled === true && covered;
    // An administrator's own origins replace the manifest's (checked as the
    // manifest's are: exact https origins, and only with the network permission).
    if (entry.network) {
      const checked = checkManifest({ ...raw, network: entry.network }, { manifestUrl: entry.manifestUrl, selfOrigin: view.location.origin });
      if (!checked.ok) return { ok: false, reason: checked.reason };
      manifest = checked.manifest;
    }
    const record = { manifest, entry, running: null, enabled };
    plugins.set(manifest.id, record);
    run(record);
    return { ok: true, record };
  }

  async function load() {
    // The editor's own events, forwarded within each plugin's grant: a revision
    // handle and a flag, never document text (`docs/183` §5).
    let selectionTimer = null;
    let lastKey = null;
    session()?.on("change", ({ detail }) => {
      platform.notifyAll("document/changed", { revision: detail.revision });
      accessChanged();
      const key = opaqueKey(`${ed.draftDocKey ?? ""}|${ed.currentName ?? ""}`);
      if (lastKey !== null && key !== lastKey) platform.notifyAll("document/opened", { documentKey: key });
      lastKey = key;
    });
    session()?.on("selection", ({ detail }) => {
      view.clearTimeout(selectionTimer);
      selectionTimer = view.setTimeout(() => {
        platform.notifyAll("selection/changed", { hasRange: !!detail.hasRange });
        accessChanged();
      }, 250);
    });
    // Theme and locale changes reach running plugins with the tokens' new values.
    const themeChanged = () => platform.notifyAll("theme/changed", { theme: theme(), tokens: tokens() }, { always: true });
    if (typeof view.MutationObserver === "function") {
      new view.MutationObserver(themeChanged).observe(doc.documentElement, { attributes: true, attributeFilter: ["data-theme", "style", "lang", "dir"] });
    }
    view.matchMedia?.("(prefers-color-scheme: dark)").addEventListener?.("change", themeChanged);
    catalogue = await loadCatalogues({ config, baseUrl: view.location.href, fetchJson, view });
    for (const entry of catalogue.entries) await admit(entry);
    renderManager();
    reflectChrome();
  }

  // ---- Manage add-ins ----------------------------------------------------------

  const manager = doc.getElementById("pluginsDialog");
  const managerList = doc.getElementById("pluginsList");
  const managerStatus = doc.getElementById("pluginsStatus");
  const installRow = doc.getElementById("pluginsInstall");
  const installUrl = doc.getElementById("pluginsInstallUrl");
  const installBtn = doc.getElementById("pluginsInstallBtn");
  const managedNote = doc.getElementById("pluginsManaged");
  const managerController = manager
    ? registerModal(manager, { initialFocus: () => doc.getElementById("pluginsClose"), fallbackFocus: () => shown(doc.getElementById("settingsAddinsBtn")) ?? shown(doc.getElementById("reviewAddinsBtn")) ?? ed.pagesEl })
    : null;

  function button(labelKey, params, onClick, primary = false) {
    const node = doc.createElement("button");
    node.type = "button";
    node.className = primary ? "dialog-button dialog-button-primary" : "dialog-button";
    node.textContent = t(labelKey, params);
    node.addEventListener("click", onClick);
    return node;
  }

  function renderManager() {
    if (!managerList || !manager || manager.hidden) return;
    managedNote.hidden = catalogue.mode !== "host";
    installRow.hidden = !catalogue.allowInstall;
    const rows = [];
    for (const record of plugins.values()) {
      const { manifest } = record;
      const item = doc.createElement("li");
      item.className = "plugins-row";
      const name = doc.createElement("strong");
      name.textContent = nameOf(manifest);
      const meta = doc.createElement("span");
      meta.className = "plugins-meta";
      meta.textContent = t(record.entry.source === "first-party" ? "plugins.source.firstParty" : record.entry.source === "host" ? "plugins.source.host" : "plugins.source.installed", { origin: new URL(manifest.entryUrl).host, version: manifest.version });
      const description = doc.createElement("p");
      description.className = "plugins-description";
      description.textContent = localised(manifest, "description", activeLocale(), manifest.description);
      // On the source line, not a line of its own: three add-ins each spent a
      // row on it and pushed Manage add-ins past a 1280x720 laptop (dialog-fit).
      const perms = doc.createElement("span");
      perms.className = "plugins-permissions";
      perms.textContent = manifest.permissions.length
        ? t("plugins.permissionsList", { list: manifest.permissions.map((p) => t(`plugins.permissionShort.${p}`)).join(", ") })
        : t("plugins.permissionsNone");
      const actions = doc.createElement("div");
      actions.className = "plugins-actions";
      if (record.entry.source === "host") {
        const state = doc.createElement("span");
        state.className = "plugins-meta";
        state.textContent = t(record.enabled ? "plugins.state.onByHost" : "plugins.state.offByHost");
        actions.append(state);
      } else if (record.enabled) {
        actions.append(button("plugins.turnOff", {}, () => disable(record)));
      } else {
        actions.append(button("plugins.turnOn", {}, () => void enable(record), true));
      }
      if (record.enabled && !present(record)) {
        const state = doc.createElement("span");
        state.className = "plugins-meta";
        state.textContent = t("plugins.state.needsProvider");
        actions.append(state);
      }
      if (record.entry.source === "installed" || (record.entry.source !== "host" && !record.entry.hostGrant && grantFor(manifest.id, view))) {
        actions.append(button(record.entry.source === "installed" ? "plugins.remove" : "plugins.forgetGrant", {}, () => remove(record)));
      }
      meta.append(" · ", perms);
      item.append(name, meta, description, actions);
      rows.push(item);
    }
    if (!rows.length) {
      const empty = doc.createElement("li");
      empty.className = "plugins-empty";
      empty.textContent = t(catalogue.mode === "none" ? "plugins.noneHere" : "plugins.empty");
      rows.push(empty);
    }
    managerList.replaceChildren(...rows);
  }

  installBtn?.addEventListener("click", async () => {
    const raw = installUrl.value.trim();
    let url;
    try {
      url = new URL(raw, view.location.href).href;
    } catch {
      managerStatus.textContent = t("plugins.install.badUrl");
      return;
    }
    managerStatus.textContent = t("plugins.install.checking");
    const admitted = await admit({ manifestUrl: url, source: "installed", hostGrant: null, enabled: false, when: null });
    if (!admitted.ok) {
      managerStatus.textContent = t("plugins.install.refused", { reason: admitted.reason });
      return;
    }
    addInstalled(url, view);
    installUrl.value = "";
    managerStatus.textContent = "";
    renderManager();
    await enable(admitted.record);
  });

  function openManager() {
    if (!managerController) return;
    managerStatus.textContent = catalogue.problems.length ? t("plugins.catalogueProblem") : "";
    managerController.open();
    renderManager();
  }
  doc.getElementById("pluginsClose")?.addEventListener("click", () => managerController?.close());
  doc.getElementById("settingsAddinsBtn")?.addEventListener("click", () => {
    // Replaces Settings instead of stacking a second dialog over it (owner,
    // 2026-11-02). Closed through its ✕, the same path as Escape.
    const settings = doc.getElementById("settingsPanel");
    if (settings && !settings.hidden) settings.querySelector(".dialog-close")?.click();
    openManager();
  });
  doc.getElementById("pluginsDone")?.addEventListener("click", () => managerController?.close());

  // ---- Menus: Add-ins (ribbon) and ✦ (selection toolbar) --------------------

  /** One menu row for a contributed command. */
  function menuRow(record, item, close) {
    const command = record.manifest.contributes.commands.find((c) => c.id === item.command);
    const row = doc.createElement("button");
    row.type = "button";
    row.className = "menu-item";
    row.setAttribute("role", "menuitem");
    row.dataset.pluginCommand = pluginCommandId(record.manifest.id, command.id);
    const icon = doc.createElement("span");
    icon.className = item.icon ? "ms" : "menu-check";
    icon.setAttribute("aria-hidden", "true");
    if (item.icon) icon.textContent = item.icon;
    const label = doc.createElement("span");
    label.textContent = titleOf(record.manifest)(command);
    row.append(icon, label);
    onButton(row, () => {
      close();
      void invoke(record, command);
    });
    return row;
  }

  /** Every running plugin's rows of one contribution kind. */
  function contributed(kind) {
    const out = [];
    for (const record of plugins.values()) {
      if (!isRunning(record) || !present(record)) continue;
      for (const item of record.manifest.contributes[kind] ?? []) out.push([record, item]);
    }
    return out;
  }

  const menuButton = doc.getElementById("reviewAddinsBtn");
  const menu = doc.getElementById("addinsMenu");
  const menuItems = doc.getElementById("addinsMenuItems");
  let popover = null;
  if (menuButton && menu && menuItems) {
    popover = registerPopover(
      menuButton,
      menu,
      () => menuItems.replaceChildren(...contributed("ribbon").map(([record, item]) => menuRow(record, item, () => closePopover(popover)))),
      { needsDocument: false },
    );
    const manage = doc.getElementById("addinsManage");
    if (manage) {
      onButton(manage, () => {
        closePopover(popover);
        openManager();
      });
    }
  }

  const selButton = doc.getElementById("selAiBtn");
  const selDivider = doc.getElementById("selAiDivider");
  const selMenu = doc.getElementById("selAiMenu");
  let selPopover = null;
  if (selButton && selMenu) {
    selPopover = registerPopover(selButton, selMenu, () => {
      selMenu.replaceChildren(...contributed("selection").map(([record, item]) => menuRow(record, item, () => closePopover(selPopover))));
    });
  }

  /** The ✦ actions on a selection, or the first AI plugin's panel with none. */
  function actions() {
    if (ed.hasRange?.() && selPopover && contributed("selection").length) {
      ed.positionSelToolbar?.();
      openPopover(selPopover, { keyboard: true, anchor: shown(selButton) ?? selButton });
      return;
    }
    const first = contributed("selection")[0]?.[0] ?? null;
    if (first?.manifest.contributes.panel) {
      showPanel(first, true);
      first.running.invoke("panel", { hasRange: false });
    }
  }

  /** Opens the first AI plugin's panel, whatever is selected (owner,
   *  2026-11-02: Casual AI must be reachable without a selection, the way
   *  Copilot and Gemini are). */
  function openAiPanel() {
    const first = contributed("selection")[0]?.[0] ?? null;
    if (!first?.manifest.contributes.panel) return;
    showPanel(first, true);
    first.running.invoke("panel", { hasRange: !!ed.hasRange?.() });
  }
  const aiHeaderBtn = doc.getElementById("aiHeaderBtn");
  const homeAiBtn = doc.getElementById("homeAiBtn");
  const homeAiGroup = doc.getElementById("homeAiGroup");
  aiHeaderBtn?.addEventListener("click", openAiPanel);
  homeAiBtn?.addEventListener("click", openAiPanel);

  // "Help me write" beside the caret on an empty line (Word's "Draft with
  // Copilot", Docs' "Help me write"). Opens the assistant in Draft mode, which
  // writes at the caret. Shown only while an AI plugin runs and the caret sits
  // collapsed in an empty paragraph; O(1) per check (paragraphLength reads one
  // paragraph).
  const helpWrite = doc.createElement("button");
  helpWrite.type = "button";
  helpWrite.id = "aiHelpWriteBtn";
  helpWrite.className = "ai-help-write";
  helpWrite.hidden = true;
  helpWrite.innerHTML = '<span class="ms" aria-hidden="true">edit_note</span><span class="ai-help-write-text"></span>';
  const helpWriteText = () => (has("aiHelpWrite.label") ? t("aiHelpWrite.label") : "Help me write");
  helpWrite.addEventListener("mousedown", (e) => e.preventDefault());
  helpWrite.addEventListener("click", () => {
    helpWrite.hidden = true;
    const first = contributed("selection")[0]?.[0] ?? null;
    if (!first?.manifest.contributes.panel) return;
    showPanel(first, true);
    first.running.invoke("draft", { hasRange: false });
  });
  doc.body.append(helpWrite);
  function placeHelpWrite() {
    const running = contributed("selection").length > 0;
    const sel = ed.selection;
    const collapsed = sel && !ed.hasRange?.();
    let empty = false;
    if (running && collapsed && ed.doc?.paragraphLength) {
      try { empty = ed.doc.paragraphLength(sel.anchor.node) === 0; } catch { empty = false; }
    }
    const caret = empty ? ed.pagesEl?.querySelector(".overlay .caret") : null;
    const r = caret?.getBoundingClientRect();
    if (!r || !r.height || doc.body.classList.contains("modal-open")) { helpWrite.hidden = true; return; }
    helpWrite.querySelector(".ai-help-write-text").textContent = helpWriteText();
    helpWrite.setAttribute("aria-label", helpWriteText());
    helpWrite.hidden = false;
    helpWrite.style.top = `${Math.round(r.top + r.height / 2 - helpWrite.offsetHeight / 2)}px`;
    helpWrite.style.left = `${Math.round(r.left + 8)}px`;
  }
  let helpWriteQueued = false;
  const queueHelpWrite = () => {
    if (helpWriteQueued) return;
    helpWriteQueued = true;
    requestAnimationFrame(() => { helpWriteQueued = false; placeHelpWrite(); });
  };
  for (const type of ["keyup", "pointerup", "focusin"]) doc.addEventListener(type, queueHelpWrite, true);
  ed.viewportEl?.addEventListener("scroll", queueHelpWrite, { passive: true });

  /** Shows only what running plugins contribute (owner rule 1). O(plugins). */
  function reflectChrome() {
    const ribbonRows = contributed("ribbon").length > 0;
    if (menuButton) menuButton.hidden = !ribbonRows;
    const selectionRows = contributed("selection").length > 0;
    if (selButton) selButton.hidden = !selectionRows;
    if (selDivider) selDivider.hidden = !selectionRows;
    // The always-there entry points follow the same rule: shown only while an
    // AI plugin runs (no provider, no AI chrome — owner rule, 2026-10-31).
    if (aiHeaderBtn) aiHeaderBtn.hidden = !selectionRows;
    if (homeAiGroup) homeAiGroup.hidden = !selectionRows;
    // Labelled here rather than in markup: the keys are declared in
    // en_strings.mjs, and t() is safe because the buttons only show now.
    if (selectionRows) {
      const ask = has("plugins.assistant.command.ask") ? t("plugins.assistant.command.ask") : "Ask Casual AI";
      const name = has("plugins.assistant.name") ? t("plugins.assistant.name") : "Casual AI";
      aiHeaderBtn?.querySelector(".ai-header-text")?.replaceChildren(ask);
      aiHeaderBtn?.setAttribute("title", ask);
      homeAiBtn?.querySelector(".fmt-big-label")?.replaceChildren(name);
      homeAiBtn?.setAttribute("title", ask);
      homeAiBtn?.setAttribute("aria-label", ask);
    }
  }

  // ---- The registry --------------------------------------------------------

  addCommandSource((context) => {
    // A provider can arrive at any time (a key saved, a host provider set late);
    // the registry is rebuilt often enough to notice, at O(plugins).
    let changed = false;
    for (const record of plugins.values()) {
      if ((record.enabled && present(record) && !record.running) || (record.running && !present(record))) changed = true;
    }
    if (changed) refresh();
    // Only an add-in that is turned on contributes rows (disabled with the reason
    // while it loads or after it stopped); one that is off adds nothing to the
    // palette — Manage add-ins is where it is turned on (owner rule, 2026-10-31).
    const visible = [...plugins.values()].filter((record) => present(record) && record.enabled);
    return [...proofingCommands(proofing), ...pluginCommands({
      plugins: visible.map((record) => ({
        manifest: record.manifest,
        running: isRunning(record),
        enabled: record.enabled,
        name: nameOf(record.manifest),
        title: titleOf(record.manifest),
      })),
      context,
      openManager,
      invoke: (plugin, command) => void invoke(plugins.get(plugin.manifest.id), command),
      actions: contributed("selection").length ? actions : null,
    })];
  });

  // Everything stops when the page goes: a plugin must not outlive its editor.
  view.addEventListener?.("pagehide", () => {
    for (const record of plugins.values()) stop(record, "unmounted");
  }, { once: true });

  const platform = {
    plugins: () => [...plugins.values()].map((record) => ({ id: record.manifest.id, enabled: record.enabled, present: present(record), state: record.running?.state ?? "stopped", source: record.entry.source, grant: record.running?.grant() ?? null })),
    openManager,
    refresh,
    actions,
    proofing,
    coeditor,
    accessChanged,
    install: async (manifestUrl) => {
      const admitted = await admit({ manifestUrl, source: "installed", hostGrant: null, enabled: false, when: null });
      if (!admitted.ok) return admitted;
      addInstalled(manifestUrl, view);
      return { ok: await enable(admitted.record) };
    },
    /** Forwards an editor change to every running plugin that may know it. */
    notifyAll(method, params) {
      for (const record of plugins.values()) record.running?.notify(method, params);
    },
  };
  // After the boot script has finished, so the host session exists to subscribe to.
  platform.ready = Promise.resolve()
    .then(load)
    .catch((err) => console.warn("opendoc: the plugin catalogues could not be read", err));
  return platform;
}
