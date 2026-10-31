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

/** The page's provider service, once started; and the plugin platform's promise. */
let started = null;
let platformPromise = null;

/** The page's AI provider service, or null before `startAiPlatform`. */
export function aiProviders() {
  return started;
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
    });
    dialog.open();
  }
  const settingsRow = view.document?.getElementById("aiProviderSettingsBtn");
  settingsRow?.addEventListener("click", openSettings);
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
      startPluginPlatform(ed, { assistant, providers, config, openAiSettings: openSettings, view }),
    );
  }
  return providers;
}

/** The plugin platform once it has started, or null (no document, or not yet). */
export function pluginPlatform() {
  return platformPromise;
}
