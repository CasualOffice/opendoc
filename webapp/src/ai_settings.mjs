// SPDX-License-Identifier: Apache-2.0
// Bring your own key: the standalone editor's AI provider settings (`docs/183` §3.9).
//
// WHAT IS STORED, AND WHERE. One object — the service preset, the protocol, the
// endpoint, the model, the key — in THIS origin's `localStorage` under
// `opendoc.ai.provider`. Never in a document, a draft, a version, a room or a log;
// never sent anywhere but the endpoint it names, and only when the person asks an
// AI feature for something. "Forget" deletes it. Storage that throws (a private
// window) keeps it for this page only, and the dialog says so.
//
// WHAT IS SHOWN. The disclosure — "Text you send goes to api.openai.com" — before
// anything is sent; the key is never written back into the form (the field says it
// is saved, and stays empty unless the person types a new one).
//
// WHERE IT IS NOT OFFERED. A framed editor (someone else's product) and a host
// that said `?aiProvider=off` or serves its own model: the dialog says who manages
// AI instead, and nothing can be saved (`ai_provider_channel.mjs`'s resolution).
//
// Designed from what users already know: ONLYOFFICE's AI plugin (bring your own
// key per user, a provider list, a URL for local servers), and the LM Studio /
// Ollama convention that a local model needs only a URL. Unlike ONLYOFFICE's, the
// key is checked with one fixed, harmless request ("Test connection") before
// the person relies on it.
//
// Complexity: O(1) per interaction; nothing here reads the document.
import { endpointAllowed, endpointHost } from "./ai_provider.mjs";

/** The storage key. */
export const AI_SETTINGS_KEY = "opendoc.ai.provider";

/** The services the dialog offers, each a preset over one of three protocols.
 *  `local` presets need no key. `baseUrl` is the documented default endpoint;
 *  `model` is left for the person, because model names change faster than code. */
export const AI_PRESETS = Object.freeze([
  Object.freeze({ id: "openai", protocol: "openai", baseUrl: "https://api.openai.com/v1", local: false }),
  Object.freeze({ id: "azure", protocol: "azure", baseUrl: "https://YOUR-RESOURCE.openai.azure.com/openai/deployments/YOUR-DEPLOYMENT", local: false }),
  Object.freeze({ id: "anthropic", protocol: "anthropic", baseUrl: "https://api.anthropic.com", local: false }),
  Object.freeze({ id: "gemini", protocol: "openai", baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai", local: false }),
  Object.freeze({ id: "mistral", protocol: "openai", baseUrl: "https://api.mistral.ai/v1", local: false }),
  Object.freeze({ id: "openrouter", protocol: "openai", baseUrl: "https://openrouter.ai/api/v1", local: false }),
  Object.freeze({ id: "ollama", protocol: "openai", baseUrl: "http://localhost:11434/v1", local: true }),
  Object.freeze({ id: "lmstudio", protocol: "openai", baseUrl: "http://localhost:1234/v1", local: true }),
  Object.freeze({ id: "vllm", protocol: "openai", baseUrl: "http://localhost:8000/v1", local: true }),
  Object.freeze({ id: "custom", protocol: "openai", baseUrl: "", local: false }),
]);

const PRESET = new Map(AI_PRESETS.map((preset) => [preset.id, preset]));

/** This page's copy when storage refused it. */
let memory = null;

/** The stored settings, or null when none are. Never throws. */
export function loadAiSettings(view = globalThis) {
  try {
    const raw = view.localStorage.getItem(AI_SETTINGS_KEY);
    if (raw) return normaliseAiSettings(JSON.parse(raw));
  } catch {
    /* storage refused or the value is corrupt: fall through */
  }
  return memory;
}

/** Stores `settings`. Returns `"device"` or, when storage refused, `"page"`. */
export function saveAiSettings(settings, view = globalThis) {
  const clean = normaliseAiSettings(settings);
  memory = clean;
  try {
    view.localStorage.setItem(AI_SETTINGS_KEY, JSON.stringify(clean));
    return "device";
  } catch {
    return "page";
  }
}

/** Deletes the stored settings, key included. */
export function forgetAiSettings(view = globalThis) {
  memory = null;
  try {
    view.localStorage.removeItem(AI_SETTINGS_KEY);
  } catch {
    /* nothing stored */
  }
}

/** A settings object with exactly the known fields, bounded. Pure. */
export function normaliseAiSettings(raw) {
  if (!raw || typeof raw !== "object") return null;
  const preset = PRESET.get(raw.preset) ?? PRESET.get("custom");
  const text = (value, max) => String(value ?? "").trim().slice(0, max);
  const out = {
    preset: preset.id,
    protocol: ["openai", "azure", "anthropic"].includes(raw.protocol) ? raw.protocol : preset.protocol,
    baseUrl: text(raw.baseUrl, 2048),
    model: text(raw.model, 256),
    apiKey: text(raw.apiKey, 4096),
    legacyMaxTokens: raw.legacyMaxTokens === true,
  };
  if (out.protocol === "azure") out.apiVersion = text(raw.apiVersion || "2024-10-21", 32);
  return out.baseUrl ? Object.freeze(out) : null;
}

/** Why `draft` cannot be saved, as a catalogue key, or null. Pure. */
export function aiSettingsProblem(draft) {
  if (!draft?.baseUrl) return "aiProvider.problem.url";
  if (!endpointAllowed(draft.baseUrl)) return "aiProvider.problem.insecure";
  if (!draft.model) return "aiProvider.problem.model";
  const preset = PRESET.get(draft.preset);
  if (draft.protocol === "anthropic" && !draft.apiKey) return "aiProvider.problem.key";
  if (preset && !preset.local && preset.id !== "custom" && !draft.apiKey) return "aiProvider.problem.key";
  return null;
}

/** The catalogue key and parameters for a provider error code. Pure. */
export function aiErrorMessage(error) {
  const code = error?.code ?? "";
  if (code === "auth-failed") return ["aiProvider.error.auth", {}];
  if (code === "rate-limited") return ["aiProvider.error.rate", {}];
  if (code === "network") return ["aiProvider.error.network", {}];
  if (code === "timeout") return ["aiProvider.error.timeout", {}];
  if (code === "context-too-long") return ["aiProvider.error.tooLong", {}];
  if (code === "server-error") return ["aiProvider.error.server", {}];
  if (code === "over-budget") return ["aiProvider.error.budget", {}];
  if (code === "provider-unconfigured" || code === "provider-disabled") return ["aiProvider.none", {}];
  return ["aiProvider.error.unknown", { code }];
}

/** The sentence the disclosure shows for a provider description. Pure. */
export function disclosureKey(description) {
  if (!description?.available) {
    if (description?.reason === "off") return ["aiProvider.disclosure.off", {}];
    if (description?.reason === "framed") return ["aiProvider.disclosure.framed", {}];
    return ["aiProvider.none", {}];
  }
  if (description.kind === "host") return ["aiProvider.disclosure.host", {}];
  if (description.kind === "mcp-sampling") return ["aiProvider.disclosure.mcp", {}];
  return ["aiProvider.disclosure.endpoint", { endpoint: description.endpoint }];
}

/** The one fixed request "Test connection" sends: no document text, ever. */
export function testRequest(model) {
  return { model, messages: [{ role: "user", content: "Reply with the single word: ready" }], max_completion_tokens: 16 };
}

/**
 * Wires the dialog (markup in `editor.html`, `#aiProviderDialog`). Returns `open`.
 *
 * @param {object} deps
 * @param {(el: Element, options: object) => object} deps.registerModal
 * @param {() => Element} deps.fallbackFocus
 * @param {(key: string, params?: object) => string} deps.t
 * @param {object} deps.providers `createAiProviders`'s value
 * @param {boolean} deps.byokAllowed whether this page may hold a key at all
 * @param {() => void} [deps.onChange] told after a save or a forget
 */
export function createAiSettingsDialog({ registerModal, fallbackFocus, t, providers, byokAllowed, onChange = () => {} }) {
  const $ = (id) => document.getElementById(id);
  const dialog = $("aiProviderDialog");
  if (!dialog) return { open() {} };
  const preset = $("aiProviderPreset");
  const url = $("aiProviderUrl");
  const model = $("aiProviderModel");
  const key = $("aiProviderKey");
  const keyNote = $("aiProviderKeyNote");
  const legacy = $("aiProviderLegacy");
  const disclosure = $("aiProviderDisclosure");
  const status = $("aiProviderStatus");
  const managed = $("aiProviderManaged");
  const form = $("aiProviderForm");
  const save = $("aiProviderSave");
  const test = $("aiProviderTest");
  const forget = $("aiProviderForget");
  let testing = null;

  const controller = registerModal(dialog, {
    initialFocus: () => (byokAllowed ? preset : $("aiProviderClose")),
    fallbackFocus,
    onClose: () => {
      testing?.abort();
      testing = null;
    },
  });

  function draft() {
    const typedKey = key.value.trim();
    const stored = loadAiSettings();
    const chosen = PRESET.get(preset.value) ?? PRESET.get("custom");
    return {
      preset: chosen.id,
      protocol: chosen.protocol,
      baseUrl: url.value.trim(),
      model: model.value.trim(),
      // An empty key field keeps the saved key: it is never written back into the form.
      apiKey: typedKey || (stored && stored.baseUrl === url.value.trim() ? stored.apiKey : ""),
      legacyMaxTokens: legacy.checked,
    };
  }

  function showDisclosure() {
    const d = draft();
    if (byokAllowed && d.baseUrl && endpointAllowed(d.baseUrl)) {
      disclosure.textContent = t("aiProvider.disclosure.endpoint", { endpoint: endpointHost(d.baseUrl) });
    } else {
      const [k, params] = disclosureKey(providers.describe());
      disclosure.textContent = t(k, params);
    }
  }

  function refreshButtons() {
    const problem = aiSettingsProblem(draft());
    // Never a dead control (SKILL §10): disabled WITH the reason, said aloud.
    for (const button of [save, test]) {
      button.disabled = !byokAllowed || problem !== null;
      if (button.disabled) {
        button.title = byokAllowed ? t(problem) : t("aiProvider.disclosure.framed");
        button.setAttribute("aria-describedby", "aiProviderStatus");
      } else {
        button.removeAttribute("title");
        button.removeAttribute("aria-describedby");
      }
    }
    if (problem && byokAllowed && !status.dataset.result) status.textContent = t(problem);
    else if (!problem && !status.dataset.result) status.textContent = "";
    forget.disabled = !loadAiSettings();
  }

  function fill() {
    const stored = loadAiSettings();
    managed.hidden = byokAllowed;
    form.hidden = !byokAllowed;
    if (!byokAllowed) {
      const [k, params] = disclosureKey(providers.describe());
      managed.textContent = t(k, params);
    }
    preset.value = stored?.preset ?? "openai";
    url.value = stored?.baseUrl ?? PRESET.get(preset.value).baseUrl;
    model.value = stored?.model ?? "";
    key.value = "";
    legacy.checked = stored?.legacyMaxTokens === true;
    keyNote.textContent = stored?.apiKey ? t("aiProvider.key.saved") : PRESET.get(preset.value)?.local ? t("aiProvider.key.local") : t("aiProvider.key.device");
    delete status.dataset.result;
    status.textContent = "";
    showDisclosure();
    refreshButtons();
  }

  preset.addEventListener("change", () => {
    const chosen = PRESET.get(preset.value);
    if (chosen) url.value = chosen.baseUrl;
    keyNote.textContent = chosen?.local ? t("aiProvider.key.local") : t("aiProvider.key.device");
    delete status.dataset.result;
    showDisclosure();
    refreshButtons();
  });
  for (const input of [url, model, key, legacy]) {
    input.addEventListener("input", () => {
      delete status.dataset.result;
      showDisclosure();
      refreshButtons();
    });
  }

  save.addEventListener("click", () => {
    const d = draft();
    if (!byokAllowed || aiSettingsProblem(d)) return;
    const where = saveAiSettings(d);
    onChange();
    status.dataset.result = "saved";
    status.textContent = t(where === "device" ? "aiProvider.saved" : "aiProvider.savedPage");
    controller.close();
  });

  forget.addEventListener("click", () => {
    forgetAiSettings();
    onChange();
    fill();
    status.dataset.result = "forgot";
    status.textContent = t("aiProvider.forgotten");
    preset.focus();
  });

  test.addEventListener("click", async () => {
    const d = draft();
    if (!byokAllowed || aiSettingsProblem(d)) return;
    testing?.abort();
    testing = new AbortController();
    const mine = testing;
    status.dataset.result = "testing";
    status.textContent = t("aiProvider.testing", { endpoint: endpointHost(d.baseUrl) });
    test.disabled = true;
    // The test goes through the SAME contract, adapters and audit as any feature,
    // with the draft as a one-off configuration, so a passing test means the
    // saved settings will work.
    const { createAiProviders } = await import("./ai_provider_channel.mjs");
    const once = createAiProviders({ settings: () => normaliseAiSettings(d), budget: { requests: 1 } });
    const answer = await once.complete(testRequest(d.model), { signal: mine.signal, purpose: "test" });
    if (testing !== mine) return;
    testing = null;
    test.disabled = false;
    if (answer.ok) {
      status.dataset.result = "ok";
      status.textContent = t("aiProvider.testOk", { model: answer.response.model || d.model });
    } else {
      const [k, params] = aiErrorMessage(answer.error);
      status.dataset.result = "error";
      status.textContent = t(k, params);
    }
  });

  return {
    open() {
      fill();
      controller.open();
    },
  };
}
