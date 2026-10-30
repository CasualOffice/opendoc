// SPDX-License-Identifier: Apache-2.0
// WebMCP: a BROWSER agent discovers the editor's tools (`docs/178` §9 item 2, M4).
//
// WHAT IT IS. The page tells the browser "here are tools an agent may call" with
// `document.modelContext.registerTool(tool, { signal })`. The shape below was
// verified against the primary sources on 2026-10-29, not recalled:
//
//   * the WebMCP Draft Community Group Report of the Web Machine Learning CG,
//     27 October 2026 (<https://webmachinelearning.github.io/webmcp/>):
//     `Document.modelContext` (SecureContext), `registerTool(ModelContextTool,
//     {exposedTo?, signal?})` returning a promise; a tool is `{name, title?,
//     description, inputSchema?, execute, annotations?}`; `execute(input, {signal})`
//     resolves to any value, which the browser serialises to JSON; the
//     annotations are `readOnlyHint`, `untrustedContentHint`, `consequentialHint`
//     and `debugging`, all `false` by default; the `signal` "unregisters the tool
//     when aborted" — there is no `unregisterTool`.
//   * Chrome's imperative API page (<https://developer.chrome.com/docs/ai/webmcp/imperative-api>,
//     last updated 2026-10-16): the same `document.modelContext.registerTool`,
//     removal "with `AbortSignal`", and from Chrome 153 an abort no longer breaks
//     an execution already in flight.
//
// WHAT IT IS NOT. A second way into the document. Every tool here calls the
// EXISTING `editor.ai` (`ai_assistant.mjs`): reads through `ai.read`, and every edit
// through `ai.preview`, which compiles the plan in the engine's one compiler and
// shows it to the person in the private card. Nothing here can commit: the tools
// never call `apply` or `accept`, so a browser agent can only PROPOSE, and the
// person decides with Accept or Discard (`docs/181` §4.4). That is the guarantee,
// and `ai_webmcp.test.mjs` holds it.
//
// OFF BY DEFAULT, AND NEVER A POLYFILL (owner, 2026-10-24). The host opts in with
// `ai-tools="webmcp"` on `<opendoc-editor>` or `?aiTools=webmcp` on the editor URL
// (`hostConfig().aiTools`). With no `document.modelContext` this does nothing and
// says so in `ai.access().webmcp`, the status a host reads.
//
// GATED BY THE SAME GRANT `editor.ai` USES. The tool list is derived from
// `assistant.access()` — `aiAccessFor` over the host's capabilities — so this
// module never names a role. Each call asks again: a tool the current grant no
// longer admits is refused and the list is re-registered, so a narrowed grant is
// never outrun by a stale registration.
//
// THE EDIT SCHEMA IS GENERATED, never written here: `webapp/generated/edit-schema.json`
// (the capability registry's JSON Schema, `docs/181` G4) and `commands.json` (each
// command's trackability). This module only SELECTS from them, per access level.
//
// Complexity: building the tool list is O(vocabulary) (61 edit types today), done
// once per registration; every tool call is the cost of the `editor.ai` call it wraps.

/** The opt-in value, on `?aiTools=` and on the element's `ai-tools` attribute. */
export const WEBMCP_OPT_IN = "webmcp";

/** The access levels, weakest first (`AI_ACCESS` in `ai_assistant.mjs`). Repeated
 *  as a rank, not as a vocabulary: `ai_webmcp.test.mjs` holds the two equal. */
const RANK = Object.freeze({ none: 0, read: 1, comment: 2, suggest: 3, review: 4 });

/** The edit types whose profile is NOT decided by trackability: `plan.rs`'s
 *  `needs` gives comments the `comment` profile and `review` the `review` profile.
 *  Every other type needs `suggest` when it is trackable and a direct grant when it
 *  is not. `ai_webmcp.test.mjs` proves every `not-applicable` command in
 *  `commands.json` is named here, so a new one cannot fall through to `suggest`. */
export const EDIT_PROFILE = Object.freeze({ comment: "comment", resolve_comment: "comment", review: "review" });

/** The page's tool names, `docs/178` §6.2's where the tool is the same tool.
 *  `propose_edits` is not `apply_edits`: it never applies (the person does). */
export const WEBMCP_TOOLS = Object.freeze([
  "get_outline",
  "read_blocks",
  "search",
  "list_comments",
  "list_tracked_changes",
  "add_comment",
  "propose_edits",
  "review_tracked_changes",
]);

/** WHAT A MODEL READS, in one table: the tool and field descriptions. English
 *  on purpose and NOT routed through `t()`: they are the API's own words, read by
 *  an agent and never shown to a person, like the tool NAMES beside them and like
 *  `tools/opendoc-mcp`'s descriptions — a description that changed with the
 *  visitor's locale would change the tool. Short and literal (`docs/178` §6.2),
 *  and fixed text: document content never reaches a description (§5.8). */
const MODEL_TEXT = Object.freeze({
  cursor: "The `next` cursor a previous call returned.",
  limit: "At most this many items.",
  baseRevision: "The `revision` your last read returned. A plan written against an older revision is refused.",
  depth: "Deepest heading level.",
  start: "A paragraph id to start from; the beginning when absent.",
  maxTokens: "Stop near this many tokens.",
  mode: "Tracked suggestions unless the host granted direct edits.",
  get_outline:
    "The document's heading tree, sections, tables and review counts, with paragraph ids to anchor reads and edits on.",
  read_blocks: "Blocks of the document from an anchor, as JSON or anchored Markdown, up to a token ceiling.",
  search: "Find text in the document; each hit carries the paragraph id and context.",
  list_comments: "Comment threads with anchors, authors and state.",
  list_tracked_changes: "Tracked changes with kind, author, date and anchor.",
  add_comment:
    "Propose a comment on quoted text. The person sees it privately in the editor and decides; nothing is added until they accept.",
  propose_edits:
    "Propose an edit plan. The editor shows the person a private redline with Accept and Discard and changes nothing until they accept.",
  suggestOnly: "Edits land as tracked suggestions.",
  directGranted: "Direct edits are granted on this page.",
  review_tracked_changes:
    "Propose accepting or rejecting tracked changes. The person sees the proposal privately and decides; nothing is decided until they accept.",
});

/** A JSON Schema field described by `MODEL_TEXT[key]`. */
const field = (type, key, extra = {}) => Object.freeze({ type, ...extra, description: MODEL_TEXT[key] });

const cursor = field("string", "cursor");
const limit = field("integer", "limit", { minimum: 1 });
const baseRevision = field("integer", "baseRevision", { minimum: 0 });

/** Read annotations: nothing changes, and the answer carries document text, which
 *  is data and never instruction (`docs/178` §5.8). */
const READ = Object.freeze({ readOnlyHint: true, untrustedContentHint: true });

/** The five read tools: `[name, the `ai.read` tool, properties, required]`. */
const READS = Object.freeze([
  ["get_outline", "outline", { depth: field("integer", "depth", { minimum: 1, maximum: 9 }), cursor, limit }],
  [
    "read_blocks",
    "read",
    {
      start: field("string", "start"),
      cursor,
      maxTokens: field("integer", "maxTokens", { minimum: 1 }),
      format: { type: "string", enum: ["json", "markdown"] },
    },
  ],
  [
    "search",
    "search",
    {
      query: { type: "string", minLength: 1 },
      mode: { type: "string", enum: ["exact", "case-insensitive", "whole-word"] },
      cursor,
      limit,
    },
    ["query"],
  ],
  ["list_comments", "comments", { cursor, limit }],
  ["list_tracked_changes", "trackedChanges", { cursor, limit }],
]);

/** Each read tool's `ai.read` tool, for another transport over the same table
 *  (the plugin port, `plugin_host.mjs`, `docs/183` §5). */
export const READ_TOOL_OF = Object.freeze(Object.fromEntries(READS.map(([name, read]) => [name, read])));

/** The edit types an access level may put in a plan. Pure; O(vocabulary).
 *
 * @param {{access: string, direct: boolean}} grant what `editor.ai` derived
 * @param {{commands: {commands: {id: string, trackable: string}[]}}} vocabulary `commands.json` as `commands`
 * @returns {Set<string>}
 */
export function editTypesFor({ access, direct }, vocabulary) {
  const rank = RANK[access] ?? 0;
  const out = new Set();
  for (const { id, trackable } of vocabulary.commands?.commands ?? []) {
    const needs = EDIT_PROFILE[id] ?? (trackable === "yes" ? "suggest" : "direct");
    if (needs === "direct" ? rank >= RANK.suggest && direct === true : rank >= RANK[needs]) out.add(id);
  }
  return out;
}

/** The generated edit schema, narrowed to `types`. Each branch is the generated
 *  object itself, never rebuilt. O(vocabulary). */
function editItems(schema, types) {
  return { oneOf: (schema.oneOf ?? []).filter((branch) => types.has(branch?.properties?.op?.const)) };
}

/** The plan input: the vocabulary's own `EditPlan` fields. */
function planSchema(items, modes) {
  return {
    type: "object",
    additionalProperties: false,
    required: ["base_revision", "edits"],
    properties: {
      base_revision: baseRevision,
      edits: { type: "array", minItems: 1, items },
      mode: field("string", "mode", { enum: modes }),
    },
  };
}

/**
 * The tool descriptors one grant admits, without `execute`. Pure, so the gating
 * matrix is answerable in node. O(vocabulary).
 *
 *   none     nothing
 *   read     the five reads
 *   comment  + add_comment, + propose_edits holding the comment types only
 *   suggest  + propose_edits holding the trackable types (every type with `direct`)
 *   review   + review_tracked_changes
 *
 * @param {{access: string, direct: boolean}} grant
 * @param {{schema: object, commands: object}} vocabulary the two generated files
 */
export function toolsFor(grant, vocabulary) {
  const rank = RANK[grant.access] ?? 0;
  const direct = grant.direct === true && rank >= RANK.suggest;
  const tools = [];
  if (rank < RANK.read) return tools;
  for (const [name, , properties, required] of READS) {
    tools.push({
      name,
      description: MODEL_TEXT[name],
      inputSchema: { type: "object", additionalProperties: false, properties, ...(required ? { required } : {}) },
      annotations: READ,
    });
  }
  if (rank < RANK.comment) return tools;
  const types = editTypesFor({ access: grant.access, direct }, vocabulary);
  const comment = (vocabulary.schema.oneOf ?? []).find((branch) => branch?.properties?.op?.const === "comment");
  if (comment && types.has("comment")) {
    const { op: _op, ...fields } = comment.properties;
    tools.push({
      name: "add_comment",
      description: MODEL_TEXT.add_comment,
      inputSchema: {
        type: "object",
        additionalProperties: false,
        required: ["base_revision", ...(comment.required ?? []).filter((field) => field !== "op")],
        properties: { base_revision: baseRevision, ...fields },
      },
      annotations: Object.freeze({ untrustedContentHint: true }),
    });
  }
  const proposable = new Set([...types].filter((type) => type !== "review"));
  tools.push({
    name: "propose_edits",
    description: [MODEL_TEXT.propose_edits, direct ? MODEL_TEXT.directGranted : MODEL_TEXT.suggestOnly].join(" "),
    inputSchema: planSchema(editItems(vocabulary.schema, proposable), direct ? ["suggest", "direct"] : ["suggest"]),
    // Consequential only where an Accept would land untracked text: everywhere
    // else it is a suggestion a reviewer can still reject.
    annotations: Object.freeze({ untrustedContentHint: true, consequentialHint: direct }),
  });
  if (types.has("review")) {
    tools.push({
      name: "review_tracked_changes",
      description: MODEL_TEXT.review_tracked_changes,
      inputSchema: planSchema(editItems(vocabulary.schema, new Set(["review"])), ["suggest"]),
      annotations: Object.freeze({ untrustedContentHint: true, consequentialHint: true }),
    });
  }
  return tools;
}

/** A refusal as the tool's answer: a value, like every `editor.ai` refusal. */
function toolRefusal(code, message) {
  return Object.freeze({ ok: false, refusal: Object.freeze({ code, command: "webmcp", requires: null, message }) });
}

/** What a proposal hands back to the agent: the preview, and that the person
 *  decides. Never an Accept. */
export function proposal(answer) {
  if (!answer?.ok) return answer;
  return Object.freeze({
    ok: true,
    status: answer.previewId ? "awaiting-person" : "refused",
    previewId: answer.previewId ?? null,
    wouldApply: !!answer.wouldApply,
    preview: answer.preview ?? null,
  });
}

/**
 * Registers the tools for as long as the grant admits them.
 *
 * @param {{
 *   assistant: {access: () => {access: string, direct: boolean}, read: Function, preview: Function},
 *   modelContext: {registerTool: Function} | null | undefined,
 *   loadVocabulary: () => Promise<{schema: object, commands: object}>,
 * }} inputs
 */
export function createWebMcp({ assistant, modelContext, loadVocabulary }) {
  let state = modelContext && typeof modelContext.registerTool === "function" ? "pending" : "unavailable";
  let reason = state === "unavailable" ? "no-model-context" : null;
  let registered = [];
  let controller = null;
  /** The grant the registered list was built for, as `access/direct`. */
  let registeredFor = null;
  let vocabulary = null;
  /** One sync at a time: `registerTool` is asynchronous. */
  let queue = Promise.resolve();
  let disposed = false;

  const grant = () => {
    const { access = "none", direct = false } = assistant.access() ?? {};
    return { access, direct: direct === true };
  };
  const keyOf = ({ access, direct }) => `${access}/${direct}`;

  /** Whether `name` is admitted by the grant in force NOW. O(vocabulary). */
  function admits(name) {
    return vocabulary !== null && toolsFor(grant(), vocabulary).some((tool) => tool.name === name);
  }

  function execute(name, read) {
    return async (input = {}) => {
      if (!admits(name)) {
        void webmcp.sync();
        return toolRefusal("capability-withheld", "This page's grant no longer offers that tool.");
      }
      const args = input && typeof input === "object" ? input : {};
      if (read) return assistant.read({ ...args, tool: read });
      if (name === "add_comment") {
        const { base_revision, ...edit } = args;
        return proposal(assistant.preview({ base_revision, edits: [{ op: "comment", ...edit }] }));
      }
      // `propose_edits` and `review_tracked_changes`: the plan as the agent wrote it,
      // into the private preview. The engine compiles it under the grant's profile
      // and refuses anything the grant does not admit, with the reason on the card.
      if (args.mode === "direct" && !grant().direct) {
        return toolRefusal("capability-withheld", "This page has not granted direct edits.");
      }
      return proposal(assistant.preview(args));
    };
  }

  async function syncNow() {
    if (disposed || state === "unavailable") return;
    const wanted = grant();
    const key = keyOf(wanted);
    if (key === registeredFor && state === "registered") return;
    try {
      vocabulary ??= await loadVocabulary();
    } catch {
      state = "failed";
      reason = "vocabulary-unavailable";
      return;
    }
    if (disposed) return;
    controller?.abort();
    controller = null;
    registered = [];
    registeredFor = key;
    const tools = toolsFor(wanted, vocabulary);
    if (!tools.length) {
      state = "registered";
      reason = wanted.access === "none" ? "no-access" : null;
      return;
    }
    const mine = new AbortController();
    controller = mine;
    const reads = new Map(READS.map(([name, read]) => [name, read]));
    try {
      for (const tool of tools) {
        await modelContext.registerTool(
          { ...tool, execute: execute(tool.name, reads.get(tool.name) ?? null) },
          { signal: mine.signal },
        );
        if (mine.signal.aborted) return;
        registered.push(tool.name);
      }
      state = "registered";
      reason = null;
    } catch (error) {
      // Half a tool list is a list the grant did not choose: take it all back.
      mine.abort();
      if (controller === mine) controller = null;
      registered = [];
      registeredFor = null;
      state = "failed";
      reason = String(error?.name ?? "register-failed");
    }
  }

  const webmcp = {
    /** What a host reads (`ai.access().webmcp`): `state` is `unavailable` (no
     *  `document.modelContext`), `pending`, `registered`, `failed` or `unregistered`,
     *  with a `reason` code and the tool names registered now. O(tools). */
    status() {
      return Object.freeze({ state, reason, tools: Object.freeze([...registered]) });
    },
    /** Re-derives the list from the grant in force and re-registers when it moved.
     *  Resolves once this sync is done. */
    sync() {
      queue = queue.then(syncNow, syncNow);
      return queue;
    },
    /** Unregisters everything: the editor is going away. Idempotent. */
    dispose() {
      if (disposed) return;
      disposed = true;
      controller?.abort();
      controller = null;
      registered = [];
      registeredFor = null;
      if (state !== "unavailable") {
        state = "unregistered";
        reason = "unmounted";
      }
    },
  };
  return webmcp;
}

/** The two generated files, fetched only when a host opted in: ~210 KB nobody
 *  else downloads. Relative to the editor page, which is where `generated/` is
 *  served from (the Pages artifact is the whole `webapp/`). */
export async function fetchVocabulary() {
  const get = async (path) => {
    const response = await fetch(new URL(path, document.baseURI));
    if (!response.ok) throw new Error(`${path}: ${response.status}`);
    return response.json();
  };
  const [schema, commands] = await Promise.all([get("generated/edit-schema.json"), get("generated/commands.json")]);
  return { schema, commands };
}

/**
 * Starts WebMCP for an assistant when the host asked for it, and returns the
 * status a host reads. Called by `createAssistant`; unregisters on `pagehide`,
 * which is what an `<opendoc-editor>` unmount (its frame blanked) and a closed tab
 * both fire.
 *
 * @param {object} assistant `editor.ai`
 * @param {{aiTools?: string|null}} config `hostConfig()`
 * @param {object} [view] the editor's window
 * @returns {() => object} the status
 */
export function startWebMcp(assistant, config, view = globalThis) {
  if (String(config?.aiTools ?? "").toLowerCase() !== WEBMCP_OPT_IN) {
    const off = Object.freeze({ state: "off", reason: "not-requested", tools: Object.freeze([]) });
    return () => off;
  }
  const webmcp = createWebMcp({
    assistant,
    modelContext: view.document?.modelContext ?? null,
    loadVocabulary: fetchVocabulary,
  });
  void webmcp.sync();
  view.addEventListener?.("pagehide", () => webmcp.dispose(), { once: true });
  return () => webmcp.status();
}
