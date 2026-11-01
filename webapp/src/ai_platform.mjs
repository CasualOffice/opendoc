// SPDX-License-Identifier: Apache-2.0
// Boots the AI provider service and the plugin platform on the editor context
// (`docs/183`). Started by `createAssistant` (`ai_assistant.mjs`), so `main.js`
// carries no line for it — the same seam WebMCP used.
//
// What it installs:
//
//   * the provider service (`aiProviders()`): `createAiProviders`'s value, the
//     one way anything in the editor reaches a model (`complete`, `describe`);
//   * the BYOK dialog and its row in Settings, its one home: with no provider the
//     editor shows nothing new anywhere else (owner rule, 2026-10-31);
//   * the plugin platform (`plugin_platform.mjs`), loaded on demand.
//
// Nothing here runs a request at boot: with no provider configured, no network
// call is made by this module or by anything it installs (owner, 2026-10-30).
//
// Complexity: O(1) at boot; the command list is O(plugins) per registry build.
import { hostConfig } from "./capabilities.mjs";
import { t } from "./i18n.mjs";
import { registerModal } from "./modal.mjs";
import { createAiProviders } from "./ai_provider_channel.mjs";
import { createAiSettingsDialog, disclosureKey, loadAiSettings } from "./ai_settings.mjs";
import { shown } from "./plugin_command_sources.mjs";
import { effectiveAiAccess, loadAiEditSetting } from "./ai_edit_mode.mjs";
import { readProtection } from "./document_protection.mjs";

/** The page's provider service, once started; and the plugin platform's promise. */
let started = null;
let platformPromise = null;

/** The page's AI provider service, or null before `startAiPlatform`. */
export function aiProviders() {
  return started;
}

/**
 * What Casual AI may do HERE, now (`docs/183` §23.3): the person's "AI edits as"
 * setting capped by the editor's mode, the document's protection, the room's
 * grant and the host's (`effectiveAiAccess`). Asked at every turn, never cached:
 * a mode switch or a protection change applies to the next decision.
 * O(1) beyond reading the protection's small JSON.
 */
export function effectiveAccessFor(ed, assistant, config = {}, view = globalThis) {
  const grant = assistant?.access?.() ?? { access: "none" };
  let protection = null;
  try {
    protection = ed.doc && typeof ed.doc.documentProtection === "function" ? readProtection(ed.doc.documentProtection()) : null;
  } catch {
    protection = null;
  }
  const authority = ed.SESSION?.modeAuthority;
  const editingRefused = authority?.allows?.("editing")?.allowed === false;
  const suggestingAllowed = authority?.allows?.("suggesting")?.allowed !== false;
  return effectiveAiAccess({
    access: grant.access,
    setting: loadAiEditSetting(view),
    reviewMode: ed.reviewMode ?? "editing",
    protection,
    suggestOnly: editingRefused && suggestingAllowed,
    hostSuggestOnly: String(config?.ai ?? "").toLowerCase() === "suggest",
  });
}

/** Whether this page may hold a key: standalone, and the host did not choose a
 *  provider for it or switch AI off (`docs/183` §3.8). Pure. */
export function byokAllowedFor(config) {
  const mode = String(config?.aiProvider ?? "").toLowerCase();
  return !config?.framed && mode !== "off" && mode !== "host";
}

/**
 * Starts the layer once per page.
 *
 * @param {object} ed the editor context
 * @param {{assistant?: object, view?: object}} [options] `assistant` is `editor.ai`,
 *   which the plugin platform's tools wrap
 */
export function startAiPlatform(ed, { assistant = null, view = globalThis } = {}) {
  if (started) return started;
  const config = hostConfig(view);
  const byokAllowed = byokAllowedFor(config);
  const session = () => ed.hostSession ?? null;
  const providers = createAiProviders({
    config,
    framed: !!config.framed,
    settings: () => (byokAllowed ? loadAiSettings(view) : null),
    remote: {
      ask: (request, options) => {
        const channel = session()?.aiProviderChannel;
        if (!channel) return Promise.reject(new Error("The host session is not ready."));
        return channel.ask(request, options);
      },
    },
    hostFn: () => session()?.aiProvider ?? null,
  });
  started = providers;
  const aiAccess = () => effectiveAccessFor(ed, assistant, config, view);

  let dialog = null;
  const summary = () => view.document?.getElementById("aiProviderSummary");
  function refreshSummary() {
    const node = summary();
    if (!node) return;
    const [key, params] = disclosureKey(providers.describe());
    node.textContent = t(key, params);
  }
  function openSettings() {
    dialog ??= createAiSettingsDialog({
      registerModal,
      fallbackFocus: () => shown(view.document.getElementById("aiProviderSettingsBtn")) ?? ed.pagesEl,
      t,
      providers,
      byokAllowed,
      onChange: () => {
        refreshSummary();
        // A saved or forgotten key starts or stops the AI plugins at once.
        void platformPromise?.then((platform) => platform.refresh());
      },
      editAccess: aiAccess,
      onEditAs: () => void platformPromise?.then((platform) => platform.accessChanged()),
    });
    dialog.open();
    // The provider form is a section of Settings, not a dialog of its own:
    // open Settings if it is not already showing, then bring the section into
    // view and put the caret on its first control.
    const doc = view.document;
    const settings = doc?.getElementById("settingsPanel");
    if (settings?.hidden) doc.getElementById("settingsBtn")?.click();
    doc?.querySelector('#settingsPanel [data-settings-tab="ai"]')?.click();
    const first = doc?.getElementById("aiProviderPreset");
    if (first && !first.disabled) first.focus({ preventScroll: true });
  }
  const settingsRow = view.document?.getElementById("aiProviderSettingsBtn");
  settingsRow?.addEventListener("click", openSettings);
  // Fill the form whenever Settings opens, so the section is never stale.
  view.document?.getElementById("settingsBtn")?.addEventListener("click", () => {
    queueMicrotask(() => { if (!view.document.getElementById("settingsPanel")?.hidden) openSettingsQuietly(); });
  });
  function openSettingsQuietly() {
    dialog ??= createAiSettingsDialog({
      registerModal,
      fallbackFocus: () => shown(view.document.getElementById("aiProviderSettingsBtn")) ?? ed.pagesEl,
      t,
      providers,
      byokAllowed,
      onChange: () => {
        refreshSummary();
        void platformPromise?.then((platform) => platform.refresh());
      },
      editAccess: aiAccess,
      onEditAs: () => void platformPromise?.then((platform) => platform.accessChanged()),
    });
    dialog.open();
  }
  // The Settings row's sentence follows the provider, computed when Settings is
  // opened — never at boot, when the catalogue may not have landed yet.
  view.document?.getElementById("settingsBtn")?.addEventListener("click", refreshSummary);

  // No palette row and no menu row for the settings (owner rule, 2026-10-31: with
  // no provider the editor looks exactly as it does today). Settings is its home.
  // The plugin platform (`docs/183` §4–§9), over the same assistant and provider.
  // Imported on demand: it builds chrome, and a headless import of the assistant
  // (its unit tests) must not need a DOM.
  if (assistant && view.document) {
    platformPromise = import("./plugin_platform.mjs").then(({ startPluginPlatform }) =>
      startPluginPlatform(ed, { assistant, providers, config, openAiSettings: openSettings, aiAccess, view }),
    );
  }
  return providers;
}

/** The plugin platform once it has started, or null (no document, or not yet). */
export function pluginPlatform() {
  return platformPromise;
}
