// SPDX-License-Identifier: Apache-2.0
// One running plugin: its sandboxed frame, its port and the methods it may call
// (`docs/183` §5–§7, ADR-077).
//
// THE SAME REGISTRY AGENTS USE. `tools/list` is `ai_webmcp.toolsFor` for the
// plugin's effective grant, and `tools/call` reaches `editor.ai` exactly as a
// WebMCP tool does: reads through `ai.read`, every edit through `ai.preview`,
// which opens the PRIVATE card the person accepts or discards. A plugin can never
// call `apply` or `accept`. Commands by id go through the host session's
// `execute`, so a plugin can never do more than a host could. The model, if the
// plugin holds `ai`, is reached through the provider service: the plugin never
// sees the key or the endpoint URL, only the disclosure the person sees.
//
// THE GRANT IS ASKED ON EVERY CALL (`plugin_manifest.mjs`'s `effectiveGrant`):
// what the manifest declared, intersected with what the person or host granted,
// capped by this mount's `editor.ai` access. The engine then decides again at the
// choke point (`docs/178` §5.1).
//
// Complexity: O(1) per call beyond the work the call itself does.
import { RpcRefusal, servePort } from "./plugin_rpc.mjs";
import { SANDBOX_TOKENS, effectiveGrant, sandboxDocument } from "./plugin_manifest.mjs";
import { READ_TOOL_OF, proposal, toolsFor } from "./ai_webmcp.mjs";

/** The protocol version `initialize` reports. */
export const PLUGIN_PROTOCOL_VERSION = 1;
/** The handshake message that carries the port into the frame. */
export const CONNECT_MESSAGE = Object.freeze({ opendocPlugin: PLUGIN_PROTOCOL_VERSION, type: "connect" });
/** Bounds a plugin's calls keep. */
export const PLUGIN_LIMITS = Object.freeze({
  notifyChars: 200,
  selectionBytes: 16 * 1024,
  storageBytes: 256 * 1024,
  copyBytes: 64 * 1024,
  concurrentAi: 4,
  closingMs: 2000,
});

/** Contract requirements a plugin can never run, whatever it holds: they move
 *  the file or the session, which is a person's or a host's business. */
const NEVER = new Set(["save", "download", "print", "open", "new", "autosave"]);

/** What a command id needs of a plugin's grant, or null when it is never
 *  offered (`docs/183` §6.4). Pure. */
export function commandPermission(id, row) {
  if (!row || id.startsWith("plugin.") || id.startsWith("plugins.") || id.startsWith("ai.")) return null;
  if (row.requires === null) return "none";
  if (NEVER.has(row.requires)) return null;
  if (row.requires === "comment") return "comment";
  return "edit";
}

/** Whether `grant` admits a command needing `need`. Pure. */
export function grantAdmits(need, permissions) {
  if (need === null) return false;
  if (need === "none") return true;
  if (need === "comment") return permissions.includes("comment") || permissions.includes("edit");
  return permissions.includes(need);
}

const truncate = (text, max) => {
  const chars = [...String(text ?? "")];
  return chars.length <= max ? chars.join("") : `${chars.slice(0, max - 1).join("")}…`;
};

/**
 * Starts one plugin.
 *
 * @param {object} io
 * @param {object} io.manifest a checked manifest (`checkManifest`)
 * @param {() => string[]} io.granted what the person or host granted, read per call
 * @param {object} io.env the editor's side:
 *   `access()` → `{access, direct}`; `ai` → `{read, preview}` (editor.ai);
 *   `execute(id)`; `contractRow(id)`; `commandIds()`; `providers`;
 *   `loadVocabulary()`; `selection()`; `notify(text, kind)`; `panel(open)`;
 *   `locale()`; `direction()`; `theme()`; `storage` → `{get(), set(value)}`;
 *   `copy(text)` → Promise<boolean>, or null when the plugin may not copy;
 *   `fetchEntry(url)` → the entry page's text; `onClosed(reason)`
 * @param {HTMLElement} io.container where the frame lives (the side panel's body)
 */
export function startPlugin({ manifest, granted, env, container }) {
  let frame = null;
  let rpc = null;
  let loads = 0;
  let closed = false;
  let vocabulary = null;
  let aiInFlight = 0;
  let state = "starting";

  const grant = () => effectiveGrant(manifest.permissions, granted(), env.access());
  /** The private preview, signed by this add-in unless the platform says it is
   *  the product's own assistant. */
  const previewAs = (plan) => {
    const name = env.addInName?.() ?? "";
    return name && typeof env.ai.previewFrom === "function" ? env.ai.previewFrom(plan, name) : env.ai.preview(plan);
  };
  const need = (permission) => {
    if (!grant().permissions.includes(permission)) throw new RpcRefusal("permission-denied", `This add-in does not hold the ${permission} permission.`, { permission });
  };
  async function tools() {
    vocabulary ??= await env.loadVocabulary();
    const { access, direct } = grant();
    return toolsFor({ access, direct }, vocabulary);
  }

  const handlers = {
    initialize() {
      const held = grant();
      const provider = env.providers.describe();
      return {
        protocolVersion: PLUGIN_PROTOCOL_VERSION,
        plugin: { id: manifest.id, version: manifest.version },
        permissions: held.permissions,
        access: held.access,
        direct: held.direct,
        locale: env.locale(),
        direction: env.direction(),
        theme: env.theme(),
        // The editor's own design tokens (`--accent`, `--ink`, `--radius`, …) as
        // values, so a plugin's page draws with the product's colours and spacing in
        // both themes rather than inventing its own (owner rule, 2026-10-31).
        tokens: env.tokens?.() ?? {},
        // What the administrator configured for this plugin (a server URL, say).
        config: env.config?.() ?? {},
        // A stable, opaque key for the open document on this device, so a plugin may
        // keep per-document state (the assistant's history) without learning its name.
        documentKey: held.permissions.includes("read") ? (env.documentKey?.() ?? null) : null,
        // The disclosure the person sees, never the key or the URL's path.
        provider: held.permissions.includes("ai") ? { available: provider.available, kind: provider.kind, endpoint: provider.endpoint } : { available: false },
      };
    },
    async "tools/list"() {
      need("read");
      return { tools: await tools() };
    },
    async "tools/call"(params) {
      need("read");
      const name = String(params?.name ?? "");
      const offered = await tools();
      if (!offered.some((tool) => tool.name === name)) throw new RpcRefusal("permission-denied", `The tool ${name} is not offered to this add-in.`, { tool: name });
      const args = params?.arguments && typeof params.arguments === "object" ? params.arguments : {};
      const read = READ_TOOL_OF[name];
      if (read) return env.ai.read({ ...args, tool: read });
      if (name === "add_comment") {
        const { base_revision, ...edit } = args;
        return proposal(previewAs({ base_revision, edits: [{ op: "comment", ...edit }] }));
      }
      if (args.mode === "direct" && !grant().direct) throw new RpcRefusal("permission-denied", "Direct edits are not granted to this add-in.", { permission: "edit" });
      // `propose_edits` / `review_tracked_changes`: the private card. Never apply.
      return proposal(previewAs(args));
    },
    "selection/get"() {
      need("read");
      const selection = env.selection();
      if (!selection) return { hasRange: false, text: "", anchor: null, focus: null, truncated: false };
      const encoded = new TextEncoder().encode(selection.text ?? "");
      const truncated = encoded.length > PLUGIN_LIMITS.selectionBytes;
      const text = truncated ? new TextDecoder().decode(encoded.slice(0, PLUGIN_LIMITS.selectionBytes)).replace(/�$/, "") : String(selection.text ?? "");
      return { hasRange: !!selection.hasRange, text, anchor: selection.anchor ?? null, focus: selection.focus ?? null, truncated };
    },
    "commands/list"() {
      const { permissions } = grant();
      return { commands: env.commandIds().filter((id) => grantAdmits(commandPermission(id, env.contractRow(id)), permissions)) };
    },
    async "commands/run"(params) {
      const id = String(params?.id ?? "");
      const permission = commandPermission(id, env.contractRow(id));
      if (permission === null) throw new RpcRefusal("permission-denied", `Add-ins cannot run ${id}.`, { command: id });
      if (!grantAdmits(permission, grant().permissions)) throw new RpcRefusal("permission-denied", `${id} needs the ${permission} permission.`, { permission, command: id });
      return env.execute(id);
    },
    async "ai/complete"(params, { id, signal }) {
      need("ai");
      if (aiInFlight >= PLUGIN_LIMITS.concurrentAi) throw new RpcRefusal("rate-limited", "Too many AI requests at once.");
      aiInFlight += 1;
      try {
        const answer = await env.providers.complete(params?.request, {
          signal,
          purpose: "plugin",
          onChunk: params?.request?.stream ? (chunk) => rpc?.notify("ai/chunk", { requestId: id, chunk }) : undefined,
        });
        if (!answer.ok) throw new RpcRefusal(answer.error.code, answer.error.message, answer.error.partial ? { partial: answer.error.partial } : {});
        return { response: answer.response, estimate: answer.estimate };
      } finally {
        aiInFlight -= 1;
      }
    },
    "ai/cancel"(params, { cancel }) {
      need("ai");
      return { cancelled: cancel(params?.requestId) };
    },
    /** Scrolls to and selects a paragraph the plugin read (a citation chip). */
    "ui/reveal"(params) {
      need("read");
      const node = String(params?.node ?? "");
      if (!node) throw new RpcRefusal("bad-request", "ui/reveal needs a paragraph id.");
      return { revealed: env.reveal?.(node) === true };
    },
    /** Strings from the editor's own catalogue, for a FIRST-PARTY plugin only: its
     *  words are translated with the product's (19 locales). Keys outside the
     *  plugin's namespace are refused. */
    "ui/strings"(params) {
      const keys = Array.isArray(params?.keys) ? params.keys.map(String) : [];
      return { strings: env.strings?.(keys) ?? {} };
    },
    /** Writing issues for paragraphs the host sent in `proofing/check`
     *  (`docs/183` §19). The host verifies and draws them; nothing here edits. */
    "proofing/report"(params) {
      need("read");
      if (!manifest.contributes.proofing) throw new RpcRefusal("bad-request", "This add-in declares no proofing contribution.");
      return env.proofingReport?.(params) ?? { accepted: 0, dropped: 0 };
    },
    /** Plain text to the clipboard, for a FIRST-PARTY plugin only (its answer
     *  cards' Copy): the sandbox holds no clipboard of its own. At most 64 KiB. */
    async "ui/copy"(params) {
      if (typeof env.copy !== "function") throw new RpcRefusal("permission-denied", "Only the product's own add-ins may copy.");
      const text = String(params?.text ?? "");
      if (!text) throw new RpcRefusal("bad-request", "ui/copy needs text.");
      if (new TextEncoder().encode(text).length > PLUGIN_LIMITS.copyBytes) throw new RpcRefusal("bad-request", "ui/copy takes at most 64 KiB.");
      return { copied: (await env.copy(text)) === true };
    },
    "ui/notify"(params) {
      const message = truncate(params?.message, PLUGIN_LIMITS.notifyChars);
      if (!message) throw new RpcRefusal("bad-request", "ui/notify needs a message.");
      env.notify(message, params?.kind === "error" ? "error" : "info");
      return {};
    },
    "ui/panel"(params) {
      if (!manifest.contributes.panel) throw new RpcRefusal("bad-request", "This add-in declares no panel.");
      env.panel(params?.open !== false);
      return {};
    },
    "storage/get"() {
      return { value: env.storage.get() };
    },
    "storage/set"(params) {
      const value = params?.value ?? null;
      let size = 0;
      try {
        size = new TextEncoder().encode(JSON.stringify(value)).length;
      } catch {
        throw new RpcRefusal("bad-request", "storage/set takes JSON.");
      }
      if (size > PLUGIN_LIMITS.storageBytes) throw new RpcRefusal("bad-request", "Add-in storage holds at most 256 KiB.");
      env.storage.set(value);
      return {};
    },
  };

  function close(reason) {
    if (closed) return;
    closed = true;
    state = "closed";
    try {
      rpc?.notify("plugin/closing", { reason });
    } catch {
      /* the port may already be gone */
    }
    rpc?.close();
    rpc = null;
    if (frame) {
      try {
        frame.srcdoc = "";
      } catch {
        /* nothing to release */
      }
      frame.remove();
      frame = null;
    }
    env.onClosed?.(reason);
  }

  async function boot() {
    let html;
    try {
      html = await env.fetchEntry(manifest.entryUrl);
    } catch (err) {
      state = "failed";
      env.onClosed?.("entry-unavailable", err);
      return;
    }
    if (closed) return;
    frame = document.createElement("iframe");
    // Scripts and nothing else: an opaque origin with no popups, forms,
    // navigation of the top window, modals or downloads (`docs/183` §7).
    frame.setAttribute("sandbox", SANDBOX_TOKENS.join(" "));
    frame.setAttribute("allow", "");
    frame.setAttribute("referrerpolicy", "no-referrer");
    frame.className = "plugin-frame";
    // Shown only while its panel is the open one; a hidden frame still runs.
    frame.hidden = true;
    frame.title = manifest.name;
    frame.dataset.plugin = manifest.id;
    frame.addEventListener("load", () => {
      loads += 1;
      if (loads > 1) {
        // The plugin navigated its own frame: whatever it loaded now is not the
        // page that was granted. Close it and say so (`docs/183` §10 T2).
        close("navigated");
        return;
      }
      const channel = new MessageChannel();
      rpc = servePort(channel.port1, handlers);
      state = "running";
      // An opaque origin has no origin to name, so the target is "*"; what is
      // sent is only the port, and only to this frame, at its first load.
      frame.contentWindow?.postMessage(CONNECT_MESSAGE, "*", [channel.port2]);
    });
    frame.srcdoc = sandboxDocument(html, manifest, grant().permissions);
    container.append(frame);
  }
  void boot();

  return {
    id: manifest.id,
    get state() {
      return state;
    },
    get frame() {
      return frame;
    },
    grant,
    /** A contributed command was chosen. */
    invoke(command, context = {}) {
      if (state !== "running") return false;
      rpc?.notify("contribution/invoked", { command, context });
      return true;
    },
    /** Asks a proofing add-in to check `paragraphs` (`[{node, text, language}]`).
     *  Only while it holds `read`. */
    proofingCheck(paragraphs) {
      if (state !== "running" || !manifest.contributes.proofing || !grant().permissions.includes("read")) return false;
      rpc?.notify("proofing/check", { paragraphs });
      return true;
    },
    /** Tells the plugin something changed, within what its grant may know. */
    notify(method, params) {
      if (state !== "running") return;
      if ((method === "document/changed" || method === "selection/changed") && !grant().permissions.includes("read")) return;
      rpc?.notify(method, params);
    },
    close,
  };
}
