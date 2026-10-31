// SPDX-License-Identifier: Apache-2.0
// The plugin manifest, the sandbox it runs in, and the grant it holds
// (`docs/183` §4, §6, §7; ADR-077).
//
// A plugin is DATA here until a person or a host grants it: a manifest checked
// field by field, unknown fields refused (a manifest that says something we do
// not understand may be asking for something we would not grant), and an entry
// page that is fetched and wrapped — never imported — so its code runs only in
// an opaque-origin iframe behind a Content-Security-Policy it cannot loosen.
//
// Pure: no DOM, no fetch. `plugin_host.mjs` does the I/O.
//
// Complexity: O(manifest size).
import { endpointAllowed } from "./ai_provider.mjs";

/** The manifest format this editor reads. */
export const MANIFEST_VERSION = 1;

/** Every permission a manifest may declare, weakest first (`docs/183` §6.1). */
export const PLUGIN_PERMISSIONS = Object.freeze(["read", "comment", "suggest", "edit", "network", "ai"]);

/** The editing permissions as an `editor.ai` access level (`ai_assistant.mjs`'s
 *  `AI_ACCESS`), strongest a plugin can hold: `review` is never a plugin's —
 *  deciding tracked changes is a person's (`docs/183` §6.2). */
const ACCESS_RANK = Object.freeze({ none: 0, read: 1, comment: 2, suggest: 3, review: 4 });

/** Bounds a manifest must keep. */
export const MANIFEST_BOUNDS = Object.freeze({
  idBytes: 128,
  nameChars: 64,
  descriptionChars: 300,
  publisherChars: 64,
  commands: 32,
  ribbon: 8,
  contextMenu: 8,
  selection: 12,
  network: 8,
  locales: 32,
  panelWidth: Object.freeze([240, 480]),
  panelMenu: 6,
  entryBytes: 1024 * 1024,
  manifestBytes: 64 * 1024,
});

const ID = /^[a-z0-9]+(\.[a-z0-9-]+)+$/;
const COMMAND_ID = /^[a-z][a-zA-Z0-9-]{0,47}$/;
const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;
const CONTEXT_WHEN = Object.freeze(["always", "selection", "caret"]);
const TOP_FIELDS = Object.freeze(["manifestVersion", "id", "name", "version", "description", "publisher", "entry", "icon", "permissions", "network", "contributes", "locales"]);
const CONTRIBUTES = Object.freeze(["commands", "ribbon", "panel", "contextMenu", "palette", "selection", "proofing"]);

const isObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const chars = (text) => [...String(text)].length;
const bytes = (text) => new TextEncoder().encode(String(text)).length;

function fail(reason, field) {
  return Object.freeze({ ok: false, reason, field });
}

function unknownField(object, allowed, where) {
  for (const key of Object.keys(object)) if (!allowed.includes(key)) return fail(`${where}${key} is not a manifest field`, `${where}${key}`);
  return null;
}

function text(value, max, field, { required = true } = {}) {
  if (value === undefined && !required) return null;
  if (typeof value !== "string" || !value.trim() || chars(value) > max) return fail(`${field} must be text of 1 to ${max} characters`, field);
  return null;
}

/** An `http(s)` URL resolved against `base`, or null. Credentials refused. */
function resolveUrl(raw, base) {
  try {
    const url = new URL(String(raw), base);
    if (url.username || url.password) return null;
    if (url.protocol !== "https:" && url.protocol !== "http:") return null;
    return url;
  } catch {
    return null;
  }
}

/** Whether `url` may serve plugin code: https, or http on this computer, or the
 *  editor's own origin (the first-party catalogue on a local preview). */
function codeOriginAllowed(url, selfOrigin) {
  return url.origin === selfOrigin || endpointAllowed(url.href);
}

/**
 * Checks a manifest and returns `{ok: true, manifest}` with resolved URLs
 * (`entryUrl`, `entryOrigin`, `iconUrl`) and frozen contributions, or `{ok:
 * false, reason, field}`.
 *
 * @param {unknown} raw the parsed JSON
 * @param {{manifestUrl: string, selfOrigin: string}} where
 */
export function checkManifest(raw, { manifestUrl, selfOrigin }) {
  if (!isObject(raw)) return fail("the manifest is not a JSON object", "");
  const unknown = unknownField(raw, TOP_FIELDS, "");
  if (unknown) return unknown;
  if (raw.manifestVersion !== MANIFEST_VERSION) return fail(`manifestVersion must be ${MANIFEST_VERSION}`, "manifestVersion");
  if (typeof raw.id !== "string" || !ID.test(raw.id) || bytes(raw.id) > MANIFEST_BOUNDS.idBytes) {
    return fail("id must be reverse-DNS, like org.example.tool", "id");
  }
  for (const [field, max, required] of [["name", MANIFEST_BOUNDS.nameChars, true], ["description", MANIFEST_BOUNDS.descriptionChars, false], ["publisher", MANIFEST_BOUNDS.publisherChars, false]]) {
    const problem = text(raw[field], max, field, { required });
    if (problem) return problem;
  }
  if (typeof raw.version !== "string" || !SEMVER.test(raw.version)) return fail("version must be SemVer, like 1.0.0", "version");
  const entryUrl = resolveUrl(raw.entry, manifestUrl);
  if (!entryUrl || !codeOriginAllowed(entryUrl, selfOrigin)) return fail("entry must be an https page (or http on this computer)", "entry");
  let iconUrl = null;
  if (raw.icon !== undefined) {
    iconUrl = resolveUrl(raw.icon, manifestUrl);
    if (!iconUrl || !codeOriginAllowed(iconUrl, selfOrigin)) return fail("icon must be an https URL", "icon");
  }
  const permissions = raw.permissions ?? [];
  if (!Array.isArray(permissions) || permissions.some((p) => !PLUGIN_PERMISSIONS.includes(p)) || new Set(permissions).size !== permissions.length) {
    return fail(`permissions must be distinct values of ${PLUGIN_PERMISSIONS.join(", ")}`, "permissions");
  }
  const network = raw.network ?? [];
  if (!Array.isArray(network) || network.length > MANIFEST_BOUNDS.network) return fail(`network lists at most ${MANIFEST_BOUNDS.network} origins`, "network");
  const origins = [];
  for (const entry of network) {
    const url = resolveUrl(entry, undefined);
    // An ORIGIN, exactly: no path, no wildcard, https (or a local server).
    if (!url || !endpointAllowed(url.href) || url.origin !== String(entry).replace(/\/$/, "") || String(entry).includes("*")) {
      return fail("network entries must be origins like https://api.example.org, without wildcards or paths", "network");
    }
    origins.push(url.origin);
  }
  if (permissions.includes("network") !== origins.length > 0) {
    return fail("network origins and the network permission come together", "network");
  }
  const contributes = raw.contributes ?? {};
  if (!isObject(contributes)) return fail("contributes must be an object", "contributes");
  const unknownContribution = unknownField(contributes, CONTRIBUTES, "contributes.");
  if (unknownContribution) return unknownContribution;
  const commands = contributes.commands ?? [];
  if (!Array.isArray(commands) || commands.length > MANIFEST_BOUNDS.commands) return fail(`at most ${MANIFEST_BOUNDS.commands} commands`, "contributes.commands");
  const commandIds = new Set();
  for (const command of commands) {
    if (!isObject(command) || !COMMAND_ID.test(command.id ?? "") || commandIds.has(command.id)) return fail("each command needs a distinct id like check-grammar", "contributes.commands");
    const unknownKey = unknownField(command, ["id", "title"], "contributes.commands.");
    if (unknownKey) return unknownKey;
    const problem = text(command.title, MANIFEST_BOUNDS.nameChars, "contributes.commands.title");
    if (problem) return problem;
    commandIds.add(command.id);
  }
  const refs = (list, max, field, extra = []) => {
    if (list === undefined) return { ok: true, items: [] };
    if (!Array.isArray(list) || list.length > max) return { ok: false, problem: fail(`${field} lists at most ${max} items`, field) };
    const items = [];
    for (const item of list) {
      if (!isObject(item) || !commandIds.has(item.command)) return { ok: false, problem: fail(`${field} names a command the manifest does not declare`, field) };
      const unknownKey = unknownField(item, ["command", ...extra], `${field}.`);
      if (unknownKey) return { ok: false, problem: unknownKey };
      if (item.when !== undefined && !CONTEXT_WHEN.includes(item.when)) return { ok: false, problem: fail(`${field}.when must be ${CONTEXT_WHEN.join(", ")}`, field) };
      if (item.icon !== undefined && !/^[a-z0-9_]{1,40}$/.test(item.icon)) return { ok: false, problem: fail(`${field}.icon must be a Material Symbols name`, field) };
      items.push(Object.freeze({ ...item }));
    }
    return { ok: true, items };
  };
  const ribbon = refs(contributes.ribbon, MANIFEST_BOUNDS.ribbon, "contributes.ribbon", ["icon"]);
  if (!ribbon.ok) return ribbon.problem;
  const contextMenu = refs(contributes.contextMenu, MANIFEST_BOUNDS.contextMenu, "contributes.contextMenu", ["when"]);
  if (!contextMenu.ok) return contextMenu.problem;
  const palette = refs(contributes.palette, MANIFEST_BOUNDS.commands, "contributes.palette");
  // Rows in the one ✦ menu of the selection toolbar (`docs/183` §8): actions on
  // the selected text, shown only while the plugin runs.
  const selection = refs(contributes.selection, MANIFEST_BOUNDS.selection, "contributes.selection", ["icon"]);
  if (!selection.ok) return selection.problem;
  if (!palette.ok) return palette.problem;
  let panel = null;
  if (contributes.panel !== undefined) {
    if (!isObject(contributes.panel)) return fail("contributes.panel must be an object", "contributes.panel");
    const unknownKey = unknownField(contributes.panel, ["title", "width", "menu"], "contributes.panel.");
    if (unknownKey) return unknownKey;
    const problem = text(contributes.panel.title, MANIFEST_BOUNDS.nameChars, "contributes.panel.title");
    if (problem) return problem;
    const [min, max] = MANIFEST_BOUNDS.panelWidth;
    const width = contributes.panel.width ?? 320;
    if (!Number.isInteger(width) || width < min || width > max) return fail(`contributes.panel.width must be ${min} to ${max}`, "contributes.panel.width");
    // Rows of the panel head's ⋯ menu (`docs/183` §21): the plugin's own
    // commands, drawn and announced by the editor.
    const menu = refs(contributes.panel.menu, MANIFEST_BOUNDS.panelMenu, "contributes.panel.menu", ["icon"]);
    if (!menu.ok) return menu.problem;
    panel = Object.freeze({ title: contributes.panel.title, width, menu: Object.freeze(menu.items) });
  }
  // Writing issues drawn in the text by the host (`docs/183` §19). Reading the
  // paragraphs to check is reading the document, so it needs `read`.
  let proofing = null;
  if (contributes.proofing !== undefined) {
    if (!isObject(contributes.proofing)) return fail("contributes.proofing must be an object", "contributes.proofing");
    const unknownKey = unknownField(contributes.proofing, [], "contributes.proofing.");
    if (unknownKey) return unknownKey;
    if (!permissions.includes("read")) return fail("contributes.proofing needs the read permission", "contributes.proofing");
    proofing = Object.freeze({});
  }
  const locales = raw.locales ?? {};
  if (!isObject(locales) || Object.keys(locales).length > MANIFEST_BOUNDS.locales) return fail("locales must be an object of at most 32 tags", "locales");
  for (const [tag, table] of Object.entries(locales)) {
    if (!/^[a-zA-Z]{2,3}(-[a-zA-Z0-9]{2,8})*$/.test(tag) || !isObject(table)) return fail("locales maps a language tag to strings", "locales");
    for (const [key, value] of Object.entries(table)) {
      const allowed = key === "name" || key === "description" || key === "panel.title" || (key.startsWith("commands.") && commandIds.has(key.slice(9)));
      if (!allowed || typeof value !== "string" || chars(value) > MANIFEST_BOUNDS.descriptionChars) return fail(`locales.${tag}.${key} is not a translatable manifest string`, "locales");
    }
  }
  return Object.freeze({
    ok: true,
    manifest: Object.freeze({
      id: raw.id,
      name: raw.name.trim(),
      version: raw.version,
      description: raw.description?.trim() ?? "",
      publisher: raw.publisher?.trim() ?? "",
      manifestUrl: String(manifestUrl),
      entryUrl: entryUrl.href,
      entryOrigin: entryUrl.origin,
      iconUrl: iconUrl?.href ?? null,
      permissions: Object.freeze([...permissions]),
      network: Object.freeze(origins),
      contributes: Object.freeze({
        commands: Object.freeze(commands.map((c) => Object.freeze({ id: c.id, title: c.title.trim() }))),
        ribbon: Object.freeze(ribbon.items),
        contextMenu: Object.freeze(contextMenu.items),
        selection: Object.freeze(selection.items),
        palette: Object.freeze(palette.items),
        panel,
        proofing,
      }),
      locales: Object.freeze(locales),
    }),
  });
}

/** A manifest string in the visitor's language, the manifest's English otherwise. */
export function localised(manifest, key, locale, fallback) {
  const tags = [String(locale ?? ""), String(locale ?? "").split("-")[0]];
  for (const tag of tags) {
    const value = manifest.locales?.[tag]?.[key];
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return fallback;
}

/**
 * The grant a plugin holds: `declared ∩ granted`, then capped by the mount's
 * `editor.ai` access (`docs/183` §6.3). Returns the permission set and the
 * access level its tools are built for. Pure; O(permissions).
 *
 * @param {string[]} declared the manifest's permissions
 * @param {string[]} granted what the person or the host granted
 * @param {{access: string, direct: boolean}} mount `editor.ai`'s `access()`
 */
export function effectiveGrant(declared, granted, mount) {
  const held = new Set(declared.filter((p) => granted.includes(p)));
  const mountRank = ACCESS_RANK[mount?.access] ?? 0;
  // The editing permissions as an access level, strongest held.
  const wanted = held.has("edit") || held.has("suggest") ? "suggest" : held.has("comment") ? "comment" : held.has("read") ? "read" : "none";
  const rank = Math.min(ACCESS_RANK[wanted], Math.min(mountRank, ACCESS_RANK.suggest));
  const access = Object.keys(ACCESS_RANK).find((name) => ACCESS_RANK[name] === rank) ?? "none";
  // Direct edits need the `edit` permission AND the host's own direct grant.
  const direct = held.has("edit") && mount?.direct === true && rank >= ACCESS_RANK.suggest;
  // A plugin that may not read gets no editing access either: every edit tool
  // anchors on text it would have to read first.
  if (!held.has("read")) {
    return Object.freeze({ permissions: Object.freeze([...held].filter((p) => p === "network" || p === "ai")), access: "none", direct: false });
  }
  const permissions = [...held].filter((p) => {
    if (p === "edit") return direct || rank >= ACCESS_RANK.suggest;
    if (p === "suggest") return rank >= ACCESS_RANK.suggest;
    if (p === "comment") return rank >= ACCESS_RANK.comment;
    if (p === "read") return rank >= ACCESS_RANK.read;
    return true;
  });
  return Object.freeze({ permissions: Object.freeze(permissions), access, direct });
}

/** The permissions `next` adds over `previous`: what a new version must ask
 *  again (`docs/183` §6.5). Pure. */
export function addedPermissions(previous, next) {
  return next.filter((p) => !previous.includes(p));
}

/**
 * The CSP the sandbox enforces (`docs/183` §7). A meta policy a later policy can
 * only tighten: the plugin may add its own, never loosen this one.
 *
 * @param {{entryOrigin: string, network: string[]}} manifest
 * @param {string[]} permissions the effective permissions
 */
export function sandboxCsp(manifest, permissions) {
  const own = manifest.entryOrigin;
  const connect = permissions.includes("network") && manifest.network.length ? manifest.network.join(" ") : "'none'";
  return [
    "default-src 'none'",
    `script-src ${own} 'unsafe-inline'`,
    `style-src ${own} 'unsafe-inline'`,
    `img-src ${own} data: blob:`,
    `font-src ${own}`,
    `connect-src ${connect}`,
    "media-src 'none'",
    "object-src 'none'",
    "frame-src 'none'",
    "worker-src 'none'",
    "form-action 'none'",
    `base-uri ${own}`,
  ].join("; ");
}

const escapeAttribute = (value) => String(value).replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/**
 * The `srcdoc` a plugin frame loads: the CSP and a `<base>` FIRST, then the
 * plugin's own page. Placing them before the plugin's markup is what makes the
 * policy govern every script the page has. Pure; O(entry size).
 *
 * @param {string} html the fetched entry page
 * @param {{entryUrl: string, entryOrigin: string, network: string[]}} manifest
 * @param {string[]} permissions the effective permissions
 */
export function sandboxDocument(html, manifest, permissions) {
  const head = `<meta http-equiv="Content-Security-Policy" content="${escapeAttribute(sandboxCsp(manifest, permissions))}"><base href="${escapeAttribute(manifest.entryUrl)}">`;
  // A doctype stays first so the plugin's page keeps standards mode.
  const doctype = /^\s*<!doctype[^>]*>/i.exec(html);
  const rest = doctype ? html.slice(doctype[0].length) : html;
  return `${doctype ? doctype[0] : "<!doctype html>"}<html><head>${head}</head>${rest}</html>`;
}

/** The iframe `sandbox` tokens: scripts, and nothing else (`docs/183` §7). */
export const SANDBOX_TOKENS = Object.freeze(["allow-scripts"]);
