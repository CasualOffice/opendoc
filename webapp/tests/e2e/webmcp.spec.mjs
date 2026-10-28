// WebMCP (`docs/178` §9 item 2, M4): when the host opts in, the editor registers
// `editor.ai`'s tools with `document.modelContext`, filtered by the mount's grant,
// and every edit tool opens the person's private preview instead of committing.
//
// Chrome ships WebMCP behind an origin trial, so the browser here has no
// `document.modelContext`. Each spec injects a FAKE before any page script runs
// (`addInitScript` reaches every frame, so an `<opendoc-editor>`'s too), shaped as
// the draft specifies: `registerTool(tool, {signal})`, unregistered when the signal
// aborts. It keeps the registered tools on the frame's `document.__webmcpTools`
// and logs `+name` / `-name` to the TOP window, so an unmount — whose frame and its
// document are gone afterwards — can still be observed.
import { test, expect, gotoEditor, waitForFramedEditor } from "./fixtures.mjs";

test.describe.configure({ timeout: 120_000 });

const READS = ["get_outline", "read_blocks", "search", "list_comments", "list_tracked_changes"];
const MARKER = " (agent)";

function installFakeModelContext() {
  const tools = new Map();
  const top = (() => {
    try {
      return window.top;
    } catch {
      return window;
    }
  })();
  top.__webmcpLog ??= [];
  Object.defineProperty(document, "__webmcpTools", { value: tools });
  Object.defineProperty(document, "modelContext", {
    configurable: true,
    value: {
      async registerTool(tool, { signal } = {}) {
        if (tools.has(tool.name)) throw new DOMException("duplicate", "InvalidStateError");
        if (signal?.aborted) return;
        tools.set(tool.name, tool);
        top.__webmcpLog.push(`+${tool.name}`);
        signal?.addEventListener("abort", () => {
          tools.delete(tool.name);
          top.__webmcpLog.push(`-${tool.name}`);
        });
      },
    },
  });
}

const registered = (target) => target.evaluate(() => [...(document.__webmcpTools?.keys() ?? [])]);
const status = (target) => target.evaluate(async () => (await window.opendoc.ai.access()).webmcp);
/** Calls a registered tool the way a browser agent would: `execute(input, {signal})`. */
const call = (target, name, input) =>
  target.evaluate(
    async ([tool, args]) => document.__webmcpTools.get(tool).execute(args, { signal: new AbortController().signal }),
    [name, input],
  );

test("opt-in registers the grant's tools with their hints; default and a browser without WebMCP register none", async ({ page, consoleErrors }) => {
  await page.addInitScript(installFakeModelContext);
  await gotoEditor(page, "&aiTools=webmcp");
  await expect.poll(() => status(page).then((s) => s.state)).toBe("registered");
  // A standalone page is the owner: every tool, `review` included.
  expect(await registered(page)).toEqual([...READS, "add_comment", "propose_edits", "review_tracked_changes"]);
  const hints = await page.evaluate(() =>
    Object.fromEntries([...document.__webmcpTools.values()].map((tool) => [tool.name, { ...tool.annotations }])),
  );
  for (const name of READS) expect(hints[name]).toEqual({ readOnlyHint: true, untrustedContentHint: true });
  expect(hints.review_tracked_changes.consequentialHint).toBe(true);
  expect(hints.propose_edits.consequentialHint).toBe(false);
  // The edit schema is the generated vocabulary's, not a hand-written one.
  const ops = await page.evaluate(() =>
    document.__webmcpTools.get("propose_edits").inputSchema.properties.edits.items.oneOf.map((b) => b.properties.op.const),
  );
  expect(ops).toContain("insert_text");
  expect(ops).not.toContain("review");
  expect(consoleErrors).toEqual([]);

  // Default: the same fake is present, and nothing is registered.
  await gotoEditor(page);
  expect(await registered(page)).toEqual([]);
  expect((await status(page)).state).toBe("off");
});

test("a browser without document.modelContext gets nothing, and the host is told", async ({ page, consoleErrors }) => {
  await gotoEditor(page, "&aiTools=webmcp");
  expect(await page.evaluate(() => "modelContext" in document)).toBe(false);
  expect(await status(page)).toEqual({ state: "unavailable", reason: "no-model-context", tools: [] });
  expect(consoleErrors).toEqual([]);
});

test("a readonly mount gets the read tools only", async ({ page, consoleErrors }) => {
  await page.addInitScript(installFakeModelContext);
  await gotoEditor(page, "&mode=readonly&aiTools=webmcp");
  await expect.poll(() => status(page).then((s) => s.state)).toBe("registered");
  expect(await registered(page)).toEqual(READS);
  const outline = await call(page, "get_outline", {});
  expect(outline.ok).toBe(true);
  expect(typeof outline.value.revision).toBe("number");
  expect(consoleErrors).toEqual([]);
});

test("the edit tool shows the private card and the document is unchanged until the person accepts", async ({ page, consoleErrors }) => {
  await page.addInitScript(installFakeModelContext);
  await gotoEditor(page, "&aiTools=webmcp");
  await expect.poll(() => registered(page)).toContain("propose_edits");
  const read = await call(page, "read_blocks", { maxTokens: 4000 });
  const block = read.value.blocks.find((b) => b.kind === "paragraph" && /\w{3,}/.test(b.text ?? ""));
  const word = block.text.match(/\w{3,}/)[0];
  const hits = async () => (await call(page, "search", { query: MARKER.trim() })).value.hits.length;
  expect(await hits()).toBe(0);

  const proposed = await call(page, "propose_edits", {
    base_revision: read.value.revision,
    edits: [{ op: "insert_text", at: { node: block.id, exact: word, occurrence: 1, side: "after" }, text: MARKER }],
  });
  expect(proposed.ok).toBe(true);
  expect(proposed.status).toBe("awaiting-person");
  expect(proposed.wouldApply).toBe(true);

  const panel = page.locator("#aiPreviewPanel");
  await expect(panel).toBeVisible();
  await expect(panel.locator("ins")).toContainText(MARKER.trim());
  // Nothing reached the document, and no tool can accept on the person's behalf.
  expect(await hits()).toBe(0);
  expect(await registered(page)).not.toContain("accept");

  await panel.getByRole("button", { name: "Accept" }).click();
  await expect(panel).toBeHidden();
  await expect.poll(hits).toBeGreaterThan(0);
  const changes = await call(page, "list_tracked_changes", {});
  expect(new Set(changes.value.trackedChanges.map((change) => change.author))).toEqual(new Set(["Casual AI"]));
  expect(consoleErrors).toEqual([]);
});

test("an <opendoc-editor ai-tools=webmcp> registers in its own frame and unregisters on unmount", async ({ page }) => {
  await page.addInitScript(installFakeModelContext);
  await page.goto("/embed.html");
  await page.evaluate(() => {
    const element = document.createElement("opendoc-editor");
    element.dataset.webmcpDemo = "";
    element.setAttribute("mode", "commentor");
    element.setAttribute("ai-tools", "webmcp");
    element.setAttribute("editor-src", "./editor.html?fixture=rich");
    element.setAttribute("frame-title", "WebMCP through the element");
    document.querySelector('[data-embed="element"] .embed-stage').append(element);
  });
  const frame = await (await page.locator("[data-webmcp-demo] iframe").elementHandle()).contentFrame();
  await waitForFramedEditor(frame);
  expect(new URL(frame.url()).searchParams.get("aiTools")).toBe("webmcp");
  const commentor = [...READS, "add_comment", "propose_edits"];
  await expect.poll(() => registered(frame)).toEqual(commentor);
  // Registered in the FRAME's document: the host page holds no tool.
  expect(await registered(page)).toEqual([]);

  await page.evaluate(() => document.querySelector("[data-webmcp-demo]").remove());
  await expect
    .poll(() => page.evaluate(() => window.__webmcpLog.filter((entry) => entry.startsWith("-")).sort()))
    .toEqual(commentor.map((name) => `-${name}`).sort());
});
