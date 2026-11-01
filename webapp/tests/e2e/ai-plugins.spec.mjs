// The AI provider settings and the plugin platform in a real browser (`docs/183`).
//
// No real model and no real key: the "provider" is a Playwright route on this
// test server that answers OpenAI Chat Completions JSON, and the plugin is the
// `tests/fixtures/plugins/probe` page served from here too. What is asserted is
// what a person would see and what the engine holds.
import { test, expect, gotoEditor } from "./fixtures.mjs";

test.describe.configure({ timeout: 120_000 });

const MARKER = " (probe)";

/** Every http(s) request the page makes; filter by origin once the page is known. */
function requests(page) {
  const seen = [];
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (url.protocol.startsWith("http")) seen.push(url);
  });
  return seen;
}

async function openSettingsRow(page) {
  await page.locator("#settingsBtn").click();
  await expect(page.locator("#settingsPanel")).toBeVisible();
  await page.locator("#aiProviderSettingsBtn").click();
  await expect(page.locator("#aiProviderDialog")).toBeVisible();
}

test("with no provider set up, nothing is sent and Settings says so", async ({ page }) => {
  const seen = requests(page);
  await gotoEditor(page);
  await page.locator("#settingsBtn").click();
  await expect(page.locator("#aiProviderSummary")).toHaveText("No AI provider is set up, so no text is sent anywhere.");
  const origin = new URL(page.url()).origin;
  const outside = seen.filter((url) => url.origin !== origin).map((url) => url.href);
  expect(outside, "the editor reached another origin with no provider configured").toEqual([]);
  // And nothing on our own origin looks like a model call.
  expect(seen.filter((url) => /chat\/completions|v1\/messages/.test(url.pathname))).toEqual([]);
});

test("bring your own key: test, save, the disclosure names the endpoint, and the key is never shown again", async ({ page }) => {
  const asked = [];
  await page.route("**/mock-ai/v1/chat/completions", async (route) => {
    const body = route.request().postDataJSON();
    asked.push({ body, auth: route.request().headers().authorization ?? null });
    await route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ id: "x", object: "chat.completion", created: 1, model: body.model, choices: [{ index: 0, message: { role: "assistant", content: "ready" }, finish_reason: "stop" }] }),
    });
  });
  await gotoEditor(page);
  const origin = new URL(page.url()).origin;
  const host = new URL(page.url()).host;
  await openSettingsRow(page);
  await page.locator("#aiProviderPreset").selectOption("custom");
  await page.locator("#aiProviderUrl").fill(`${origin}/mock-ai/v1`);
  await expect(page.locator("#aiProviderDisclosure")).toHaveText(`Text you send to AI features goes to ${host}.`);
  // Disabled WITH a reason until a model is named (SKILL §10).
  await expect(page.locator("#aiProviderSave")).toBeDisabled();
  await expect(page.locator("#aiProviderStatus")).toHaveText("Enter the model name.");
  await page.locator("#aiProviderModel").fill("mock-model");
  await page.locator("#aiProviderKey").fill("sk-test-not-real");
  await page.locator("#aiProviderTest").click();
  await expect(page.locator("#aiProviderStatus")).toHaveText("Connected: mock-model answered.");
  expect(asked).toHaveLength(1);
  expect(asked[0].auth).toBe("Bearer sk-test-not-real");
  // The test prompt is fixed: no document text.
  expect(JSON.stringify(asked[0].body.messages)).not.toContain("Lorem");
  expect(asked[0].body.max_completion_tokens).toBe(16);
  await page.locator("#aiProviderSave").click();
  await expect(page.locator("#aiProviderDialog")).toBeHidden();
  await expect(page.locator("#aiProviderSummary")).toHaveText(`Text you send to AI features goes to ${host}.`);

  // Reopened, the key is not written back into the form.
  await page.locator("#aiProviderSettingsBtn").click();
  await expect(page.locator("#aiProviderKey")).toHaveValue("");
  await expect(page.locator("#aiProviderKeyNote")).toHaveText("A key is saved on this device. Leave the field empty to keep it.");
  // Forget removes it.
  await page.locator("#aiProviderForget").click();
  await expect(page.locator("#aiProviderStatus")).toHaveText("The AI provider and its key were removed from this device.");
  expect(await page.evaluate(() => localStorage.getItem("opendoc.ai.provider"))).toBeNull();
});

test("?aiProvider=off: Settings says AI is off and the dialog holds no form", async ({ page }) => {
  await gotoEditor(page, "&aiProvider=off");
  await openSettingsRow(page);
  await expect(page.locator("#aiProviderForm")).toBeHidden();
  await expect(page.locator("#aiProviderManaged")).toHaveText("AI features are turned off for this editor.");
});

/** Installs the probe plugin through Manage add-ins and allows it. */
async function installProbe(page) {
  await page.keyboard.press(process.platform === "darwin" ? "Meta+Shift+P" : "Control+Shift+P");
  await page.locator("#cmdInput").fill("manage add-ins");
  await page.locator(".cmd-item", { hasText: "Manage add-ins" }).first().click();
  await expect(page.locator("#pluginsDialog")).toBeVisible();
  await page.locator("#pluginsInstallUrl").fill("tests/fixtures/plugins/probe/manifest.json");
  await page.locator("#pluginsInstallBtn").click();
  await expect(page.locator("#pluginGrantDialog")).toBeVisible();
  await expect(page.locator("#pluginGrantTitle")).toHaveText("Allow Probe?");
  await expect(page.locator("#pluginGrantList li")).toHaveText(["Read this document", "Propose changes as suggestions"]);
  await expect(page.locator("#pluginGrantWarning")).toBeVisible();
  await page.locator("#pluginGrantAllow").click();
  await expect(page.locator("#pluginGrantDialog")).toBeHidden();
  await page.keyboard.press("Escape");
  await expect(page.locator("#pluginsDialog")).toBeHidden();
  const frame = page.frameLocator('iframe[data-plugin="org.opendoc.test.probe"]');
  await expect(frame.locator("#state")).toHaveText("connected");
  return page.frames().find((f) => f.url() === "about:srcdoc" && f !== page.mainFrame());
}

test("a plugin installed with a grant runs sandboxed, appears in the Add-ins menu and its panel", async ({ page }) => {
  await gotoEditor(page);
  const frame = await installProbe(page);
  // The sandbox is scripts only: the frame has an opaque origin and cannot read
  // this page's storage, where a BYOK key would live.
  expect(await page.locator('iframe[data-plugin="org.opendoc.test.probe"]').getAttribute("sandbox")).toBe("allow-scripts");
  expect(await frame.evaluate(() => { try { return String(localStorage.length); } catch (err) { return err.name; } })).toBe("SecurityError");
  expect(await frame.evaluate(() => { try { return String(window.top.document.title); } catch (err) { return err.name; } })).toBe("SecurityError");
  // The grant it holds is what was declared and allowed.
  expect(await frame.evaluate(() => window.probeInfo.permissions)).toEqual(["read", "suggest"]);

  await page.locator('[data-tab="review"]').click();
  await page.locator("#reviewAddinsBtn").click();
  const row = page.locator("#addinsMenu .menu-item", { hasText: "Probe the editor" });
  await expect(row).toBeEnabled();
  await row.click();
  await expect(page.locator("#pluginPanel")).toBeVisible();
  await expect(page.locator("#pluginPanelTitle")).toHaveText("Probe");
  await expect(page.frameLocator('iframe[data-plugin="org.opendoc.test.probe"]').locator("#invoked")).toHaveText("1");
  await page.locator("#pluginPanelClose").click();
  await expect(page.locator("#pluginPanel")).toBeHidden();
});

test("a plugin's edit opens the private preview and changes nothing until the person accepts", async ({ page }) => {
  await gotoEditor(page);
  const frame = await installProbe(page);
  const proposal = await frame.evaluate(async (marker) => {
    const read = await window.probe.tools.call("read_blocks", { maxTokens: 4000 });
    const block = read.value.blocks.find((b) => b.kind === "paragraph" && /\w{3,}/.test(b.text ?? ""));
    const word = block.text.match(/\w{3,}/)[0];
    return window.probe.tools.call("propose_edits", {
      base_revision: read.value.revision,
      edits: [{ op: "insert_text", at: { node: block.id, exact: word, occurrence: 1, side: "after" }, text: marker }],
    });
  }, MARKER);
  expect(proposal.status).toBe("awaiting-person");
  await expect(page.locator("#aiPreviewPanel")).toBeVisible();
  const search = () => page.evaluate(async (m) => (await window.opendoc.ai.read({ tool: "search", query: m.trim() })).value.hits.length, MARKER);
  expect(await search(), "nothing lands before Accept").toBe(0);
  await page.locator("#aiPreviewPanel .ai-preview-accept").click();
  await expect.poll(search).toBeGreaterThan(0);
  // As a suggestion, never a direct edit: the plugin held `suggest`.
  const changes = await page.evaluate(async () => (await window.opendoc.ai.read({ tool: "trackedChanges" })).value.trackedChanges.length);
  expect(changes).toBeGreaterThan(0);
});

test("a plugin may not run commands outside its grant or apply without the person", async ({ page }) => {
  await gotoEditor(page);
  const frame = await installProbe(page);
  const answers = await frame.evaluate(async () => {
    const tryIt = (fn) => fn().then(() => "ok", (err) => err.code);
    return {
      bold: await tryIt(() => window.probe.commands.run("format.bold")),
      save: await tryIt(() => window.probe.commands.run("file.save")),
      ai: await tryIt(() => window.probe.ai.complete({ messages: [{ role: "user", content: "hi" }] })),
      review: await tryIt(() => window.probe.tools.call("review_tracked_changes", { base_revision: 0, edits: [] })),
      // A no-cors request to the test server itself: it resolves (opaquely) unless
      // the CSP refuses it, so "blocked" can only mean connect-src said no.
      network: await fetch(new URL("/editor.html", document.baseURI), { mode: "no-cors" }).then(() => "fetched", () => "blocked"),
      // The control: an image from the plugin's own origin is allowed by img-src,
      // so the frame CAN reach the server — only fetch is refused.
      image: await new Promise((resolve) => {
        const img = new Image();
        img.onload = () => resolve("loaded");
        img.onerror = () => resolve("refused");
        img.src = new URL("/opendoc-mark.svg", document.baseURI).href;
      }),
    };
  });
  expect(answers).toEqual({ bold: "permission-denied", save: "permission-denied", ai: "permission-denied", review: "permission-denied", network: "blocked", image: "loaded" });
});

test("a plugin that navigates its own frame is closed and the person is told", async ({ page }) => {
  await gotoEditor(page);
  const frame = await installProbe(page);
  // The frame's own context goes away with the navigation, so the call may not return.
  await frame
    .evaluate(() => {
      location.href = "about:blank";
    })
    .catch(() => {});
  await expect(page.locator('iframe[data-plugin="org.opendoc.test.probe"]')).toHaveCount(0);
  await expect(page.locator("#status")).toHaveText("Probe was closed because it tried to leave its sandbox.");
});
