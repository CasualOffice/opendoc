// WebMCP's decisions that need no browser (`docs/178` §9 item 2, M4): which tools
// each grant registers, with which hints and which slice of the GENERATED edit
// schema, and that registration follows the grant and the editor's lifetime
// through the `AbortSignal` the draft specifies. Driven against a fake
// `document.modelContext` that keeps what was registered and drops a tool when its
// signal aborts, which is the draft's only unregister mechanism.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { ROLES, resolveCapabilities } from "../src/capabilities.mjs";
import { AI_ACCESS, aiAccessFor } from "../src/ai_assistant.mjs";
import {
  EDIT_PROFILE,
  WEBMCP_OPT_IN,
  WEBMCP_TOOLS,
  createWebMcp,
  editTypesFor,
  startWebMcp,
  toolsFor,
} from "../src/ai_webmcp.mjs";

const VOCABULARY = Object.freeze({
  schema: JSON.parse(readFileSync(new URL("../generated/edit-schema.json", import.meta.url), "utf8")),
  commands: JSON.parse(readFileSync(new URL("../generated/commands.json", import.meta.url), "utf8")),
});
const READS = ["get_outline", "read_blocks", "search", "list_comments", "list_tracked_changes", "read_table"];

const grantOf = (role, direct = false) => aiAccessFor(resolveCapabilities({ mode: role, framed: true }), { direct });
const names = (grant) => toolsFor(grant, VOCABULARY).map((tool) => tool.name);
const tool = (grant, name) => toolsFor(grant, VOCABULARY).find((entry) => entry.name === name);
const opsOf = (descriptor) => descriptor.inputSchema.properties.edits.items.oneOf.map((branch) => branch.properties.op.const);

/** A fake `document.modelContext`: the draft's `registerTool(tool, {signal})`. */
function fakeModelContext({ failOn = null } = {}) {
  const tools = new Map();
  const log = [];
  return {
    tools,
    log,
    async registerTool(descriptor, { signal } = {}) {
      if (descriptor.name === failOn) throw Object.assign(new Error("refused"), { name: "InvalidStateError" });
      if (tools.has(descriptor.name)) throw Object.assign(new Error("duplicate"), { name: "InvalidStateError" });
      if (signal?.aborted) return;
      tools.set(descriptor.name, descriptor);
      log.push(`+${descriptor.name}`);
      signal?.addEventListener("abort", () => {
        tools.delete(descriptor.name);
        log.push(`-${descriptor.name}`);
      });
    },
  };
}

/** A fake `editor.ai`: the grant it reports can be changed, and every call is kept. */
function fakeAssistant(role, direct = false) {
  const calls = [];
  const assistant = {
    calls,
    grant: grantOf(role, direct),
    access: () => Object.freeze({ ok: true, ...assistant.grant }),
    read: (request) => {
      calls.push(["read", request]);
      return { ok: true, value: { revision: 3 } };
    },
    preview: (plan) => {
      calls.push(["preview", plan]);
      return { ok: true, previewId: 7, wouldApply: true, preview: { redline: [] } };
    },
    apply: () => {
      calls.push(["apply"]);
      throw new Error("a WebMCP tool must never call apply");
    },
    accept: () => {
      calls.push(["accept"]);
      throw new Error("a WebMCP tool must never call accept");
    },
  };
  return assistant;
}

const mount = (assistant, modelContext) =>
  createWebMcp({ assistant, modelContext, loadVocabulary: async () => VOCABULARY });

test("each role's grant registers exactly its row of the gating matrix", () => {
  assert.deepEqual(
    Object.fromEntries(ROLES.map((role) => [role, names(grantOf(role))])),
    {
      preview: [],
      readonly: READS,
      commentor: [...READS, "add_comment", "propose_edits"],
      edit: [...READS, "add_comment", "propose_edits"],
      owner: [...READS, "add_comment", "propose_edits", "review_tracked_changes"],
    },
  );
  for (const role of ROLES) {
    for (const name of names(grantOf(role, true))) assert.ok(WEBMCP_TOOLS.includes(name), name);
  }
  // The rank this module keeps is `editor.ai`'s own list, in its order.
  assert.deepEqual(AI_ACCESS, ["none", "read", "comment", "suggest", "review"]);
});

test("read tools are read-only and untrusted; only a direct grant or a review is consequential", () => {
  for (const name of READS) {
    assert.deepEqual({ ...tool(grantOf("readonly"), name).annotations }, { readOnlyHint: true, untrustedContentHint: true }, name);
  }
  for (const role of ["commentor", "edit", "owner"]) {
    for (const name of ["add_comment", "propose_edits", "review_tracked_changes"]) {
      const descriptor = tool(grantOf(role), name);
      if (!descriptor) continue;
      assert.notEqual(descriptor.annotations.readOnlyHint, true, `${role} ${name} is not read-only`);
      assert.equal(descriptor.annotations.untrustedContentHint, true, `${role} ${name} echoes document text`);
    }
  }
  assert.equal(tool(grantOf("edit"), "propose_edits").annotations.consequentialHint, false);
  assert.equal(tool(grantOf("edit", true), "propose_edits").annotations.consequentialHint, true);
  assert.equal(tool(grantOf("owner"), "review_tracked_changes").annotations.consequentialHint, true);
});

test("the edit schema is the generated one, narrowed per grant and never rebuilt", () => {
  const generated = new Map(VOCABULARY.schema.oneOf.map((branch) => [branch.properties.op.const, branch]));
  const comment = opsOf(tool(grantOf("commentor"), "propose_edits"));
  assert.deepEqual(new Set(comment), new Set(["comment", "resolve_comment"]));
  const suggest = opsOf(tool(grantOf("edit"), "propose_edits"));
  const trackable = VOCABULARY.commands.commands.filter((row) => row.trackable === "yes").map((row) => row.id);
  assert.deepEqual(new Set(suggest), new Set([...trackable, "comment", "resolve_comment"]));
  const direct = opsOf(tool(grantOf("edit", true), "propose_edits"));
  assert.deepEqual(new Set(direct), new Set([...generated.keys()].filter((op) => op !== "review")));
  assert.deepEqual(opsOf(tool(grantOf("owner"), "review_tracked_changes")), ["review"]);
  // The same objects the generator wrote, not copies someone could edit.
  for (const branch of tool(grantOf("edit", true), "propose_edits").inputSchema.properties.edits.items.oneOf) {
    assert.equal(branch, generated.get(branch.properties.op.const));
  }
  assert.deepEqual(tool(grantOf("edit"), "propose_edits").inputSchema.properties.mode.enum, ["suggest"]);
  assert.deepEqual(tool(grantOf("edit", true), "propose_edits").inputSchema.properties.mode.enum, ["suggest", "direct"]);
  // `add_comment` is the generated `comment` branch's own fields.
  assert.deepEqual(
    Object.keys(tool(grantOf("commentor"), "add_comment").inputSchema.properties).sort(),
    ["base_revision", ...Object.keys(generated.get("comment").properties).filter((key) => key !== "op")].sort(),
  );
});

test("every edit type the registry marks not-applicable has a profile here", () => {
  // A new comment- or review-like command would otherwise fall through to `direct`.
  const exempt = VOCABULARY.commands.commands.filter((row) => row.trackable === "not-applicable").map((row) => row.id);
  assert.deepEqual(new Set(exempt), new Set(Object.keys(EDIT_PROFILE)));
  for (const op of Object.keys(EDIT_PROFILE)) assert.ok(editTypesFor(grantOf("owner"), VOCABULARY).has(op), op);
});

test("opt-in registers the grant's tools; default and a missing modelContext register none and say so", async () => {
  const context = fakeModelContext();
  const webmcp = mount(fakeAssistant("readonly"), context);
  assert.equal(webmcp.status().state, "pending");
  await webmcp.sync();
  assert.deepEqual([...context.tools.keys()], READS);
  assert.deepEqual({ ...webmcp.status(), tools: [...webmcp.status().tools] }, { state: "registered", reason: null, tools: READS });

  const absent = mount(fakeAssistant("owner"), undefined);
  await absent.sync();
  assert.deepEqual(absent.status().state, "unavailable");
  assert.deepEqual(absent.status().reason, "no-model-context");

  // Not opted in: nothing is touched, even with a modelContext present.
  const untouched = fakeModelContext();
  const view = { document: { modelContext: untouched }, addEventListener() {} };
  const off = startWebMcp(fakeAssistant("owner"), { aiTools: null }, view);
  await Promise.resolve();
  assert.equal(untouched.tools.size, 0);
  assert.equal(off().state, "off");
  const typo = startWebMcp(fakeAssistant("owner"), { aiTools: "webmcp-polyfill" }, view);
  assert.equal(typo().state, "off");
  assert.equal(WEBMCP_OPT_IN, "webmcp");
});

test("a narrowed grant aborts the old registration and registers only what is left", async () => {
  const context = fakeModelContext();
  const assistant = fakeAssistant("owner");
  const webmcp = mount(assistant, context);
  await webmcp.sync();
  assert.ok(context.tools.has("review_tracked_changes"));
  assistant.grant = grantOf("readonly");
  // A call under the narrowed grant is refused even before a sync catches up.
  const stale = await context.tools.get("propose_edits").execute({ base_revision: 1, edits: [] }, {});
  assert.equal(stale.ok, false);
  assert.equal(stale.refusal.code, "capability-withheld");
  await webmcp.sync();
  assert.deepEqual([...context.tools.keys()], READS);
  assert.ok(context.log.includes("-review_tracked_changes"));
  assert.ok(!assistant.calls.some(([method]) => method === "preview"), "the refused call reached the preview");
});

test("unmount unregisters every tool, and a failed registration takes the half list back", async () => {
  const context = fakeModelContext();
  const webmcp = mount(fakeAssistant("edit"), context);
  await webmcp.sync();
  assert.equal(context.tools.size, 8);
  webmcp.dispose();
  assert.equal(context.tools.size, 0);
  assert.equal(webmcp.status().state, "unregistered");
  await webmcp.sync();
  assert.equal(context.tools.size, 0, "a disposed editor registered again");

  // `pagehide` is the unmount: an `<opendoc-editor>` blanks its frame.
  const listeners = {};
  const view = { document: { modelContext: fakeModelContext() }, addEventListener: (type, fn) => (listeners[type] = fn) };
  const status = startWebMcp(fakeAssistant("readonly"), { aiTools: "webmcp" }, view);
  assert.equal(typeof listeners.pagehide, "function");
  listeners.pagehide();
  assert.equal(status().state, "unregistered");

  const failing = fakeModelContext({ failOn: "search" });
  const half = mount(fakeAssistant("readonly"), failing);
  await half.sync();
  assert.equal(failing.tools.size, 0);
  assert.equal(half.status().state, "failed");
  assert.equal(half.status().reason, "InvalidStateError");
});

test("edit tools only ever PROPOSE: they reach `preview`, never `apply` or `accept`", async () => {
  const context = fakeModelContext();
  const assistant = fakeAssistant("owner");
  const webmcp = mount(assistant, context);
  await webmcp.sync();
  const signal = new AbortController().signal;
  const plan = { base_revision: 3, edits: [{ op: "insert_text", at: { node: "1:2" }, text: "x" }] };
  const proposed = await context.tools.get("propose_edits").execute(plan, { signal });
  assert.deepEqual({ ...proposed, preview: null }, { ok: true, status: "awaiting-person", previewId: 7, wouldApply: true, preview: null });
  await context.tools.get("add_comment").execute({ base_revision: 3, target: { node: "1:2" }, text: "why?" }, { signal });
  await context.tools.get("review_tracked_changes").execute({ base_revision: 3, edits: [{ op: "review", decision: "accept", all: true }] }, { signal });
  const read = await context.tools.get("search").execute({ query: "days", tool: "comments" }, { signal });
  assert.equal(read.ok, true);
  assert.deepEqual(assistant.calls.map(([method]) => method), ["preview", "preview", "preview", "read"]);
  assert.deepEqual(assistant.calls[1][1], { base_revision: 3, edits: [{ op: "comment", target: { node: "1:2" }, text: "why?" }] });
  // An input cannot pick a different read than the tool's own.
  assert.equal(assistant.calls[3][1].tool, "search");
  // A direct plan without the grant is refused before the compile.
  const direct = await context.tools.get("propose_edits").execute({ ...plan, mode: "direct" }, { signal });
  assert.equal(direct.refusal.code, "capability-withheld");
  assert.equal(assistant.calls.length, 4);
});

test("the opt-in reaches the editor as `?aiTools=webmcp`, from the URL and from the element", async () => {
  const { hostConfig } = await import("../src/capabilities.mjs");
  const view = (search) => ({ location: { search }, self: 1, top: 1 });
  assert.equal(hostConfig(view("?aiTools=webmcp")).aiTools, WEBMCP_OPT_IN);
  assert.equal(hostConfig(view("")).aiTools, null);

  // The smallest DOM `<opendoc-editor>` touches, installed before it is imported
  // (its base class is chosen at module evaluation). Last in this file for that reason.
  const frames = [];
  globalThis.HTMLElement = class {
    constructor() { this.attributes = new Map(); this.isConnected = false; this.events = []; }
    getAttribute(k) { return this.attributes.has(k) ? this.attributes.get(k) : null; }
    hasAttribute(k) { return this.attributes.has(k); }
    setAttribute(k, v) { this.attributes.set(k, String(v)); }
    removeAttribute(k) { this.attributes.delete(k); }
    attachShadow() { return { replaceChildren() {} }; }
    dispatchEvent(e) { this.events.push(e); return true; }
  };
  globalThis.CustomEvent = class { constructor(type, init) { this.type = type; this.detail = init?.detail; } };
  globalThis.window = { addEventListener() {}, removeEventListener() {} };
  globalThis.document = {
    baseURI: "https://host.example/app/",
    createElement(tag) {
      if (tag !== "iframe") return { textContent: "" };
      const frame = { attributes: new Map(), src: "", setAttribute(k, v) { frame.attributes.set(k, v); }, addEventListener() {}, remove() {} };
      frames.push(frame);
      return frame;
    },
  };
  const warn = console.warn;
  console.warn = () => {};
  try {
    const { OpenDocEditorElement } = await import("../src/embed_element.mjs");
    assert.ok(OpenDocEditorElement.observedAttributes.includes("ai-tools"), "a changed ai-tools must remount");
    const mountElement = (value, src = null) => {
      const element = new OpenDocEditorElement();
      if (value !== null) element.setAttribute("ai-tools", value);
      if (src) element.setAttribute("editor-src", src);
      element.setAttribute("frame-title", "Editor");
      element.isConnected = true;
      element.connectedCallback();
      return { url: new URL(frames.at(-1).src), notes: element.events.at(-1).detail.notes };
    };
    assert.equal(mountElement("webmcp").url.searchParams.get("aiTools"), "webmcp");
    assert.equal(mountElement(null).url.searchParams.get("aiTools"), null, "off by default");
    const typo = mountElement("webmcp-polyfill", "./editor.html?aiTools=webmcp");
    assert.equal(typo.url.searchParams.get("aiTools"), null, "an unknown value turns it off");
    assert.ok(typo.notes.some((note) => note.option === "ai-tools" && note.code === "unknown"), "and is reported");
  } finally {
    console.warn = warn;
  }
});
