// SPDX-License-Identifier: Apache-2.0
// Where plugins come from, and what each was granted (`docs/183` §6.5, §9).
//
//   * A HOST lists its plugins in a catalogue (`?plugins=<URL>`), with what it
//     grants each one. The host is the administrator: no prompt.
//   * STANDALONE, the first-party catalogue (`plugins/catalogue.json`, beside
//     the editor) is offered in Manage add-ins, and a person may install one from
//     a manifest URL. Turning either on shows the grant prompt.
//   * `?plugins=none` withdraws every plugin, first-party included. A framed
//     editor whose host named no catalogue runs none: in someone else's product,
//     the host decides.
//
// Grants are stored per plugin id with the version and the permissions granted,
// so a new version that adds a permission asks again for the addition only.
// Code belongs to the installation, never to a document: nothing here is read
// from or written into a file (macros stay declined, `docs/149`).
//
// Complexity: O(plugins) per load; nothing here reads the document.

/** The first-party catalogue, relative to the editor page. */
export const FIRST_PARTY_CATALOGUE = "plugins/catalogue.json";
/** The catalogue format this editor reads. */
export const CATALOGUE_VERSION = 1;
/** Storage keys: installed manifest URLs, and grants by plugin id. */
export const INSTALLED_KEY = "opendoc.plugins.installed";
export const GRANTS_KEY = "opendoc.plugins.grants";
/** The most plugins one page runs. */
export const MAX_PLUGINS = 24;

const isObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);

function readJson(view, key, fallback) {
  try {
    const raw = view.localStorage.getItem(key);
    return raw ? JSON.parse(raw) : fallback;
  } catch {
    return fallback;
  }
}

function writeJson(view, key, value) {
  try {
    view.localStorage.setItem(key, JSON.stringify(value));
    return true;
  } catch {
    return false;
  }
}

/**
 * Checks a catalogue: `{catalogueVersion: 1, plugins: [{manifest, grant?,
 * enabled?}], allowInstall?, allowFirstParty?}`. Manifest URLs resolve against
 * the catalogue's own URL. Unknown fields are refused. Pure.
 *
 * @returns {{ok: true, entries: object[], allowInstall: boolean, allowFirstParty: boolean} | {ok: false, reason: string}}
 */
export function checkCatalogue(raw, catalogueUrl, { source }) {
  if (!isObject(raw) || raw.catalogueVersion !== CATALOGUE_VERSION || !Array.isArray(raw.plugins)) {
    return { ok: false, reason: "not-a-catalogue" };
  }
  for (const key of Object.keys(raw)) {
    if (!["catalogueVersion", "plugins", "allowInstall", "allowFirstParty"].includes(key)) return { ok: false, reason: "unknown-field", field: key };
  }
  if (raw.plugins.length > MAX_PLUGINS) return { ok: false, reason: "too-many-plugins" };
  const entries = [];
  for (const item of raw.plugins) {
    if (!isObject(item) || typeof item.manifest !== "string") return { ok: false, reason: "no-manifest-url" };
    for (const key of Object.keys(item)) if (!["manifest", "grant", "enabled"].includes(key)) return { ok: false, reason: "unknown-field", field: key };
    let manifestUrl;
    try {
      manifestUrl = new URL(item.manifest, catalogueUrl).href;
    } catch {
      return { ok: false, reason: "bad-manifest-url" };
    }
    const grant = Array.isArray(item.grant) ? item.grant.filter((p) => typeof p === "string") : null;
    entries.push(Object.freeze({ manifestUrl, source, hostGrant: source === "host" ? Object.freeze(grant ?? []) : null, enabled: item.enabled !== false }));
  }
  return { ok: true, entries, allowInstall: raw.allowInstall === true, allowFirstParty: raw.allowFirstParty === true };
}

/** What `?plugins=` says. Pure. */
export function catalogueMode(config) {
  const value = String(config?.plugins ?? "").trim();
  if (value.toLowerCase() === "none") return { mode: "none" };
  if (value) return { mode: "host", url: value };
  return { mode: config?.framed ? "none" : "standalone" };
}

/**
 * Every catalogue entry this page offers, and whether installs are allowed.
 *
 * @param {{config: object, baseUrl: string, fetchJson: (url: string) => Promise<unknown>, view?: object}} io
 */
export async function loadCatalogues({ config, baseUrl, fetchJson, view = globalThis }) {
  const { mode, url } = catalogueMode(config);
  if (mode === "none") return { mode, entries: [], allowInstall: false, problems: [] };
  const problems = [];
  const entries = [];
  let allowInstall = mode === "standalone";
  let allowFirstParty = mode === "standalone";
  if (mode === "host") {
    let hostUrl;
    try {
      hostUrl = new URL(url, baseUrl).href;
    } catch {
      return { mode, entries: [], allowInstall: false, problems: [{ source: "host", reason: "bad-catalogue-url" }] };
    }
    try {
      const checked = checkCatalogue(await fetchJson(hostUrl), hostUrl, { source: "host" });
      if (!checked.ok) problems.push({ source: "host", reason: checked.reason });
      else {
        entries.push(...checked.entries);
        allowInstall = checked.allowInstall;
        allowFirstParty = checked.allowFirstParty;
      }
    } catch {
      problems.push({ source: "host", reason: "catalogue-unavailable" });
    }
  }
  if (allowFirstParty) {
    const firstUrl = new URL(FIRST_PARTY_CATALOGUE, baseUrl).href;
    try {
      const checked = checkCatalogue(await fetchJson(firstUrl), firstUrl, { source: "first-party" });
      if (checked.ok) entries.push(...checked.entries.map((entry) => Object.freeze({ ...entry, enabled: false })));
      else problems.push({ source: "first-party", reason: checked.reason });
    } catch {
      problems.push({ source: "first-party", reason: "catalogue-unavailable" });
    }
  }
  if (allowInstall) {
    const installed = readJson(view, INSTALLED_KEY, []);
    for (const manifestUrl of Array.isArray(installed) ? installed : []) {
      if (typeof manifestUrl === "string") entries.push(Object.freeze({ manifestUrl, source: "installed", hostGrant: null, enabled: false }));
    }
  }
  // One entry per manifest URL, the strongest source first.
  const seen = new Set();
  const unique = entries.filter((entry) => (seen.has(entry.manifestUrl) ? false : seen.add(entry.manifestUrl)));
  return { mode, entries: unique.slice(0, MAX_PLUGINS), allowInstall, problems };
}

/** The installed manifest URLs, on this device. */
export function installedUrls(view = globalThis) {
  const list = readJson(view, INSTALLED_KEY, []);
  return Array.isArray(list) ? list.filter((url) => typeof url === "string") : [];
}

/** Records an install. Returns false when storage refused it. */
export function addInstalled(manifestUrl, view = globalThis) {
  const list = installedUrls(view).filter((url) => url !== manifestUrl);
  list.push(manifestUrl);
  return writeJson(view, INSTALLED_KEY, list.slice(-MAX_PLUGINS));
}

/** Removes an install. */
export function removeInstalled(manifestUrl, view = globalThis) {
  writeJson(view, INSTALLED_KEY, installedUrls(view).filter((url) => url !== manifestUrl));
}

/** Every stored grant: `{[id]: {version, permissions, enabled}}`. */
export function loadGrants(view = globalThis) {
  const grants = readJson(view, GRANTS_KEY, {});
  return isObject(grants) ? grants : {};
}

/** The stored grant for one plugin, or null. */
export function grantFor(id, view = globalThis) {
  const grant = loadGrants(view)[id];
  if (!isObject(grant) || !Array.isArray(grant.permissions)) return null;
  return { version: String(grant.version ?? ""), permissions: grant.permissions.filter((p) => typeof p === "string"), enabled: grant.enabled === true };
}

/** Stores one plugin's grant. */
export function storeGrant(id, grant, view = globalThis) {
  const all = loadGrants(view);
  all[id] = { version: grant.version, permissions: [...grant.permissions], enabled: grant.enabled === true };
  return writeJson(view, GRANTS_KEY, all);
}

/** Forgets one plugin's grant (revoking it). */
export function revokeGrant(id, view = globalThis) {
  const all = loadGrants(view);
  delete all[id];
  writeJson(view, GRANTS_KEY, all);
}

/** One plugin's own storage key (`storage/get`, `storage/set`). */
export const storageKeyFor = (id) => `opendoc.plugin.${id}`;
