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
// any other command. An installed plugin that is turned off keeps its rows,
// disabled with the reason (SKILL §10); an AI-gated one has none.
//
// Complexity: O(plugins) per registry build; nothing here reads the document.
import { t, has, activeLocale, direction } from "./i18n.mjs";
import { registerModal } from "./modal.mjs";
import { onButton, registerPopover, closePopover, openPopover } from "./popover_manager.mjs";
import { commandContract } from "./host_contract.mjs";
import { fetchVocabulary } from "./ai_webmcp.mjs";
import { MANIFEST_BOUNDS, addedPermissions, checkManifest, localised } from "./plugin_manifest.mjs";
import { startPlugin } from "./plugin_host.mjs";
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
 * @param {object} [io.view]
 */
export function startPluginPlatform(ed, { assistant, providers, config, view = globalThis }) {
  const doc = view.document;
  /** id -> {manifest, entry, running: host|null, enabled} */
  const plugins = new Map();
  let catalogue = { mode: "none", entries: [], allowInstall: false, problems: [] };
  let activePanel = null;
  const session = () => ed.hostSession ?? null;

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
  let panelOpener = null;

  function showPanel(record, open) {
    if (!panel || !record?.running?.frame) return;
    if (!open) {
      if (activePanel !== record) return;
      activePanel = null;
      panel.hidden = true;
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
    // One side panel at a time, as the outline and the review sidebar already
    // agree between themselves (`editor_outline.mjs`).
    if (ed.reviewSidebar && !ed.reviewSidebar.hidden) ed.toggleReview?.(false);
    ed.closeFlankingPanel?.(ed.outlinePanel, ed.railOutline);
    ed.closeFlankingPanel?.(ed.pagesPanel, ed.railPages);
    panel.hidden = false;
    ed.reanchorPageOverlays?.();
    record.running.frame.focus?.();
  }
  panelClose?.addEventListener("click", () => activePanel && showPanel(activePanel, false));
  panel?.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && activePanel) {
      event.preventDefault();
      showPanel(activePanel, false);
    }
  });

  // ---- Running a plugin ------------------------------------------------------

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
          return { hasRange: ed.hasRange(), text, anchor: { ...anchor }, focus: { ...focus } };
        },
        notify: (message, kind) => ed.setStatus(t("plugins.notify", { name: nameOf(record.manifest), message }), kind === "error" ? "error" : undefined),
        panel: (open) => showPanel(record, open),
        reveal,
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
        storage: storageFor(record.manifest.id),
        fetchEntry: (url) => fetchText(url, MANIFEST_BOUNDS.entryBytes),
        onClosed: (reason) => {
          if (activePanel === record) showPanel(record, false);
          record.running = null;
          reflectChrome();
          if (reason === "navigated") ed.setStatus(t("plugins.navigated", { name: nameOf(record.manifest) }), "error");
          else if (reason === "entry-unavailable") ed.setStatus(t("plugins.unavailable", { name: nameOf(record.manifest) }), "error");
        },
      },
    });
    // The frame connects asynchronously; the chrome follows once it is running.
    void waitRunning(record).then(reflectChrome);
  }

  async function waitRunning(record, tries = 100) {
    for (let i = 0; i < tries && record.running && record.running.state !== "running"; i += 1) {
      await new Promise((resolve) => view.setTimeout(resolve, 50));
    }
  }

  function stop(record, reason = "disabled") {
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
    if (record.manifest.contributes.panel) showPanel(record, true);
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
    try {
      const checked = checkManifest(await fetchJson(entry.manifestUrl), { manifestUrl: entry.manifestUrl, selfOrigin: view.location.origin });
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
      const key = opaqueKey(`${ed.draftDocKey ?? ""}|${ed.currentName ?? ""}`);
      if (lastKey !== null && key !== lastKey) platform.notifyAll("document/opened", { documentKey: key });
      lastKey = key;
    });
    session()?.on("selection", ({ detail }) => {
      view.clearTimeout(selectionTimer);
      selectionTimer = view.setTimeout(() => platform.notifyAll("selection/changed", { hasRange: !!detail.hasRange }), 250);
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
      const perms = doc.createElement("p");
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
      item.append(name, meta, description, perms, actions);
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
  doc.getElementById("settingsAddinsBtn")?.addEventListener("click", openManager);
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

  /** Shows only what running plugins contribute (owner rule 1). O(plugins). */
  function reflectChrome() {
    const ribbonRows = contributed("ribbon").length > 0;
    if (menuButton) menuButton.hidden = !ribbonRows;
    const selectionRows = contributed("selection").length > 0;
    if (selButton) selButton.hidden = !selectionRows;
    if (selDivider) selDivider.hidden = !selectionRows;
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
    const visible = [...plugins.values()].filter(present);
    return pluginCommands({
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
    });
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
