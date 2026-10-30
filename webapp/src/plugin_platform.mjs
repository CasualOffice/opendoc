// SPDX-License-Identifier: Apache-2.0
// The plugin platform in the editor: catalogues, the grant prompt, Manage
// add-ins, the Add-ins menu, the side panel and the commands plugins contribute
// (`docs/183` §6.5, §8, §9; ADR-077).
//
// DESIGNED FROM THE PRODUCTS PEOPLE KNOW. Word puts one Add-ins button on its
// ribbon that opens the add-in list; Google Docs has one Extensions menu;
// ONLYOFFICE has a Plugins tab. A button per plugin is how a ribbon overflows, so
// this editor has ONE Add-ins menu button (Review ▸ Proofing, fixed width,
// whatever the number of plugins), a palette row per command, context-menu rows
// where a manifest asks for them, and one side panel at a time.
//
// Every plugin command is `plugin.<pluginId>.<commandId>` in the one registry,
// so it reaches the palette, the menus and the host contract (the `plugin.`
// family) like any other command. A command whose plugin is not running is
// PRESENT AND DISABLED with the reason (SKILL §10).
//
// Complexity: O(plugins) per registry build; nothing here reads the document.
import { t, activeLocale, direction } from "./i18n.mjs";
import { registerModal } from "./modal.mjs";
import { onButton, registerPopover, closePopover } from "./popover_manager.mjs";
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

/**
 * The command rows the plugins contribute. Pure over its inputs.
 *
 * @param {{plugins: object[], context?: object, openManager: () => void, invoke: (plugin: object, command: object) => void}} io
 *   each plugin: `{manifest, running, enabled, name, title(command)}`
 */
export function pluginCommands({ plugins, context = {}, openManager, invoke }) {
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
  /** id -> {manifest, entry, running: host|null, enabled, problem} */
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

  const nameOf = (manifest) => localised(manifest, "name", activeLocale(), manifest.name);
  const titleOf = (manifest) => (command) => localised(manifest, `commands.${command.id}`, activeLocale(), command.title);

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

  // ---- The side panel ------------------------------------------------------

  const panel = doc.getElementById("pluginPanel");
  const panelTitle = doc.getElementById("pluginPanelTitle");
  const panelBody = doc.getElementById("pluginPanelBody");
  const panelClose = doc.getElementById("pluginPanelClose");
  let panelOpener = null;

  function showPanel(record, open) {
    if (!panel || !record.running?.frame) return;
    if (!open) {
      if (activePanel !== record) return;
      activePanel = null;
      panel.hidden = true;
      record.running.frame.hidden = true;
      const back = panelOpener;
      panelOpener = null;
      (back && back.isConnected ? back : null)?.focus?.({ preventScroll: true }) ?? ed.focusEditorSurface?.();
      return;
    }
    panelOpener = doc.activeElement instanceof view.HTMLElement ? doc.activeElement : null;
    for (const other of plugins.values()) if (other.running?.frame) other.running.frame.hidden = other !== record;
    activePanel = record;
    panelTitle.textContent = localised(record.manifest, "panel.title", activeLocale(), record.manifest.contributes.panel.title);
    panel.style.setProperty("--plugin-panel-width", `${record.manifest.contributes.panel.width}px`);
    panelClose.setAttribute("aria-label", t("plugins.panel.close", { name: nameOf(record.manifest) }));
    panelClose.title = panelClose.getAttribute("aria-label");
    panel.hidden = false;
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
    if (record.entry.source === "host") return [...(record.entry.hostGrant ?? [])];
    return grantFor(record.manifest.id, view)?.permissions ?? [];
  }

  function run(record) {
    if (record.running || !panelBody) return;
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
        locale: () => activeLocale(),
        direction: () => direction(activeLocale()),
        theme: () => doc.documentElement.dataset.theme ?? (view.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light"),
        storage: storageFor(record.manifest.id),
        fetchEntry: (url) => fetchText(url, MANIFEST_BOUNDS.entryBytes),
        onClosed: (reason) => {
          if (activePanel === record) showPanel(record, false);
          record.running = null;
          if (reason === "navigated") ed.setStatus(t("plugins.navigated", { name: nameOf(record.manifest) }), "error");
          else if (reason === "entry-unavailable") ed.setStatus(t("plugins.unavailable", { name: nameOf(record.manifest) }), "error");
        },
      },
    });
  }

  function stop(record, reason = "disabled") {
    record.running?.close(reason);
    record.running = null;
  }

  async function invoke(record, command) {
    if (!record.running) return;
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
    if (record.entry.source !== "host") {
      const stored = grantFor(record.manifest.id, view);
      const asking = stored ? addedPermissions(stored.permissions, record.manifest.permissions) : [...record.manifest.permissions];
      if (asking.length || !stored) {
        const allowed = await askGrant(record.manifest, asking);
        if (!allowed) return false;
      }
      storeGrant(record.manifest.id, { version: record.manifest.version, permissions: [...record.manifest.permissions], enabled: true }, view);
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
      if (stored) storeGrant(record.manifest.id, { ...stored, enabled: false }, view);
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
      record.enabled = false;
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
    const record = {
      manifest,
      entry,
      running: null,
      enabled: entry.source === "host" ? entry.enabled : stored?.enabled === true && covered,
    };
    plugins.set(manifest.id, record);
    if (record.enabled) run(record);
    return { ok: true, record };
  }

  async function load() {
    // The editor's own events, forwarded within each plugin's grant: a revision
    // handle and a flag, never document text (`docs/183` §5).
    let selectionTimer = null;
    session()?.on("change", ({ detail }) => platform.notifyAll("document/changed", { revision: detail.revision }));
    session()?.on("selection", ({ detail }) => {
      view.clearTimeout(selectionTimer);
      selectionTimer = view.setTimeout(() => platform.notifyAll("selection/changed", { hasRange: !!detail.hasRange }), 250);
    });
    catalogue = await loadCatalogues({ config, baseUrl: view.location.href, fetchJson, view });
    for (const entry of catalogue.entries) await admit(entry);
    renderManager();
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
    ? registerModal(manager, { initialFocus: () => doc.getElementById("pluginsClose"), fallbackFocus: () => shown(doc.getElementById("reviewAddinsBtn")) ?? ed.pagesEl })
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
      if (record.entry.source !== "host" && (record.entry.source === "installed" || grantFor(manifest.id, view))) {
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
    const admitted = await admit({ manifestUrl: url, source: "installed", hostGrant: null, enabled: false });
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
  doc.getElementById("pluginsDone")?.addEventListener("click", () => managerController?.close());

  // ---- The Add-ins menu ----------------------------------------------------

  const menuButton = doc.getElementById("reviewAddinsBtn");
  const menu = doc.getElementById("addinsMenu");
  const menuItems = doc.getElementById("addinsMenuItems");
  let popover = null;
  if (menuButton && menu && menuItems) {
    popover = registerPopover(
      menuButton,
      menu,
      () => {
        const rows = [];
        for (const record of plugins.values()) {
          if (!record.enabled) continue;
          for (const item of record.manifest.contributes.ribbon) {
            const command = record.manifest.contributes.commands.find((c) => c.id === item.command);
            const row = doc.createElement("button");
            row.type = "button";
            row.className = "menu-item";
            row.setAttribute("role", "menuitem");
            row.dataset.pluginCommand = pluginCommandId(record.manifest.id, command.id);
            if (item.icon) {
              const icon = doc.createElement("span");
              icon.className = "ms";
              icon.setAttribute("aria-hidden", "true");
              icon.textContent = item.icon;
              row.append(icon);
            }
            const label = doc.createElement("span");
            label.textContent = t("plugins.commandLabel", { plugin: nameOf(record.manifest), command: titleOf(record.manifest)(command) });
            row.append(label);
            row.disabled = !record.running;
            if (row.disabled) row.title = t("plugins.notRunning", { name: nameOf(record.manifest) });
            onButton(row, () => {
              closePopover(popover);
              void invoke(record, command);
            });
            rows.push(row);
          }
        }
        if (!rows.length) {
          const empty = doc.createElement("p");
          empty.className = "menu-empty";
          empty.textContent = t("plugins.menuEmpty");
          rows.push(empty);
        }
        menuItems.replaceChildren(...rows);
      },
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

  // ---- The registry --------------------------------------------------------

  addCommandSource((context) =>
    pluginCommands({
      plugins: [...plugins.values()].map((record) => ({
        manifest: record.manifest,
        running: !!record.running && record.running.state === "running",
        enabled: record.enabled,
        name: nameOf(record.manifest),
        title: titleOf(record.manifest),
      })),
      context,
      openManager,
      invoke: (plugin, command) => void invoke(plugins.get(plugin.manifest.id), command),
    }),
  );

  // Everything stops when the page goes: a plugin must not outlive its editor.
  view.addEventListener?.("pagehide", () => {
    for (const record of plugins.values()) stop(record, "unmounted");
  }, { once: true });

  const platform = {
    plugins: () => [...plugins.values()].map((record) => ({ id: record.manifest.id, enabled: record.enabled, state: record.running?.state ?? "stopped", source: record.entry.source, grant: record.running?.grant() ?? null })),
    openManager,
    install: async (manifestUrl) => {
      const admitted = await admit({ manifestUrl, source: "installed", hostGrant: null, enabled: false });
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
