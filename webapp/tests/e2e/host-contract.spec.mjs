// `docs/126` phase 2's EXIT GATE: a host drives every command over both
// transports, in this repository, and the two are proved to be the same contract.
//
// Three claims, each a guarantee rather than a mechanism:
//
//   1. THE CONTRACT COVERS THE REGISTRY, exactly. Every command the editor
//      offers resolves to a schema row, and every schema row and every declared
//      family is really reachable. So adding a command to the registry without
//      declaring it — or declaring one that no longer exists — fails the build
//      rather than shipping a half-reachable API. The three editor states the
//      families need are DRIVEN here, because a family whose state is never
//      entered is a guard that passes having checked nothing.
//   2. BOTH TRANSPORTS ANSWER IDENTICALLY, for every command the editor offers,
//      in a real browser against a real engine. `host_contract.test.mjs` proves
//      this in node over fake windows; this proves it over a real iframe
//      boundary, which is the half a fake window cannot.
//   3. THE API IS NOT THE UNLOCKED DOOR. A `preview` host — granted nothing —
//      runs every command the editor offers, over both transports, and the
//      document is unchanged afterwards, read from the engine's own counts. Every
//      mutating command comes back `capability-withheld`, refused BEFORE
//      dispatch, so the engine is never even asked.
//
// THREE COMMANDS ARE DRIVEN WITH `query` RATHER THAN `execute`, and the reason is
// not squeamishness: `file.print` calls `window.print()`, and `file.open` and
// `insert.image` open a native file chooser. Each of those blocks the renderer
// until the operating system lets it go, which no test timeout can interrupt —
// measured here, by mutating the gate and watching the run wedge for four minutes
// and then fail on `frame.evaluate: Test timeout` rather than on an assertion. A
// guard whose failure mode is a hang is a guard that stops CI instead of failing
// it, so those three are asked about over both transports instead of run, and
// `query` is a real transport call with no side effect. `command-activation-
// contract.spec.mjs` keeps a POINTER_UNSAFE list for exactly the same three, for
// exactly the same reason.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  mountEmbedPanel,
  waitForFramedEditor,
} from "./fixtures.mjs";

// A framed editor plus a top-level one boot in here, and the sweep drives every
// command twice over. A real cost, not a slow environment — the tell for an
// environment problem is a wall of uniform ~45 s timeouts.
test.describe.configure({ timeout: 240_000 });

const { COMMAND_CONTRACT, COMMAND_FAMILIES, commandContract } = await import(
  "../../src/host_contract.mjs"
);

/** The role the sweep runs as: it grants nothing at all, which is what makes the
 *  gate's refusal the thing under test. Read as a name so the spec says which
 *  role it means rather than spelling one out four times. */
const NOBODY = "preview";

/** The commands that must be ASKED ABOUT rather than RUN, and why. Keyed by id so
 *  a rename makes the reason unreachable and the coverage assertion below fails,
 *  rather than leaving a skip that has outlived its command. */
const BLOCKS_THE_RENDERER = new Map([
  ["file.print", "window.print() holds the renderer until the OS dialog closes"],
  ["file.open", "opens a native file chooser"],
  ["insert.image", "opens a native file chooser"],
]);

/** Every command the editor offers right now, as the editor itself reports them
 *  through the in-process session. Not read off the palette DOM: `describe()` is
 *  the contract's own answer, and taking it from anywhere else would test a
 *  different surface from the one a host uses. */
async function offeredCommands(target) {
  return target.evaluate(() => window.opendoc.describe().commands.map((command) => command.id));
}

/** The engine's own view of the document. `#statWords` / `#statChars` are painted
 *  from `documentStats()`, so they move if and only if the document moved — where
 *  a DOM assertion on the canvas could be satisfied by a repaint. */
async function engineStats(target) {
  return target.evaluate(() => ({
    words: document.getElementById("statWords")?.textContent ?? "",
    chars: document.getElementById("statChars")?.textContent ?? "",
  }));
}

test("the contract covers the command registry exactly, in every state a family needs", async ({
  page,
}) => {
  // ── State: a document open, with a caret ──────────────────────────────────
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const seen = new Set(await offeredCommands(page));
  expect(seen.size, "the registry offered almost nothing, so this proves nothing").toBeGreaterThan(
    150,
  );

  // ── State: the caret inside a table ───────────────────────────────────────
  // Driven through the real Insert surface, because the table families exist only
  // while the caret is in a table and a declared family nothing enters is a
  // guard that checks nothing.
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  for (const id of await offeredCommands(page)) seen.add(id);
  expect(
    [...seen].some((id) => id.startsWith("table.")),
    "the caret never reached a table, so the table family was never exercised",
  ).toBe(true);

  // ── State: an object selected ─────────────────────────────────────────────
  // Through the API itself — `object.selectNext` is the keyboard route to the
  // first selection — on the fixture that has an anchored image.
  const float = await page.context().newPage();
  await float.goto("/editor.html?fixture=float");
  await waitForFramedEditor(float.mainFrame());
  const selected = await float.evaluate(() => window.opendoc.execute("object.selectNext"));
  expect(selected.ok, `selecting an object was refused: ${JSON.stringify(selected)}`).toBe(true);
  await expect(float.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  for (const id of await offeredCommands(float)) seen.add(id);
  await float.close();
  expect(
    [...seen].some((id) => id.startsWith("object.") && !id.startsWith("object.select")),
    "no object was selected, so the object family was never exercised",
  ).toBe(true);

  // ── Both directions ───────────────────────────────────────────────────────
  // A command the editor offers that the contract does not declare would be
  // addressable by a user and invisible to a host: half-reachable, which is the
  // failure this gate exists to make impossible.
  expect(
    [...seen].filter((id) => commandContract(id) === null).sort(),
    "the editor offers commands the host contract does not declare — add them to " +
      "COMMAND_CONTRACT (or to a family) in webapp/src/host_contract.mjs",
  ).toEqual([]);
  // And a declaration with nothing behind it is a promise to a host that the
  // editor cannot keep.
  expect(
    COMMAND_CONTRACT.map((row) => row.id).filter((id) => !seen.has(id)),
    "the host contract declares commands the editor never offers",
  ).toEqual([]);
  const exact = new Set(COMMAND_CONTRACT.map((row) => row.id));
  expect(
    COMMAND_FAMILIES.filter(
      (row) => ![...seen].some((id) => id.startsWith(row.prefix) && !exact.has(id)),
    ).map((row) => row.prefix),
    "a declared family has no members in any state this gate drives",
  ).toEqual([]);
});

test(`a ${NOBODY} host drives every command over BOTH transports and changes nothing`, async ({
  page,
}) => {
  await page.goto("/embed.html");
  const { frame } = await mountEmbedPanel(page, "element", NOBODY);

  // The host's own `postMessage` client, built from the shipped module — the same
  // one `embed_host_demo.js` uses for the console on this page, and the same one a
  // host installs from the package.
  await page.evaluate(async (origin) => {
    const { createHostClient } = await import("./src/host_client.mjs");
    const iframe = document.querySelector('[data-embed="element"] opendoc-editor').frame;
    window.__wire = createHostClient({ frame: iframe, editorOrigin: origin });
    window.__events = [];
    for (const name of (await window.__wire.describe()).events) {
      window.__wire.on(name, (event) => window.__events.push(event));
    }
  }, new URL(page.url()).origin);

  const ids = await offeredCommands(frame);
  expect(ids.length, "nothing was offered, so the sweep proves nothing").toBeGreaterThan(150);

  const before = await engineStats(frame);
  expect(before.words, "the document has no words to protect").not.toBe("0 words");

  const disagreements = [];
  const notWithheld = [];
  for (const id of ids) {
    const verb = BLOCKS_THE_RENDERER.has(id) ? "query" : "execute";
    // In process: a direct reference into the frame, which a host can hold
    // because `sandboxTokensFor` grants `allow-same-origin`.
    const direct = await frame.evaluate(
      ([command, how]) => window.opendoc[how](command),
      [id, verb],
    );
    // Over the wire: the same session, one envelope further away.
    const wire = await page.evaluate(
      ([command, how]) => window.__wire[how](command),
      [id, verb],
    );
    if (JSON.stringify(direct) !== JSON.stringify(wire)) {
      disagreements.push(`${id}: in-process ${JSON.stringify(direct)} vs wire ${JSON.stringify(wire)}`);
    }
    // Every command that can change the document must have been refused by the
    // GATE, not by the engine — which is the difference between the API being a
    // lock and the API being a hole with a lock behind it. For the three asked
    // about rather than run, `granted: false` is the same claim in `query`'s
    // vocabulary.
    const gated = verb === "query" ? direct.granted === false : direct.refusal?.code === "capability-withheld";
    if (commandContract(id).requires !== null && !gated) {
      notWithheld.push(`${id}: ${JSON.stringify(direct)}`);
    }
  }
  // Every skip is still a command the editor offers: a skip that outlives its
  // command is a hole in the sweep nobody notices.
  expect(
    [...BLOCKS_THE_RENDERER.keys()].filter((id) => !ids.includes(id)),
    "a command is skipped that the editor no longer offers",
  ).toEqual([]);
  expect(disagreements, "the two transports are not the same contract").toEqual([]);
  expect(
    notWithheld,
    "a mutating command reached past the API's capability gate for a host granted nothing",
  ).toEqual([]);

  // The document, from the engine, after every command in the product was fired
  // at it twice. Given a beat for any mutation that DID land to repaint the
  // counters — an assertion that ran before the repaint would pass either way.
  await page.waitForTimeout(400);
  expect(
    await engineStats(frame),
    "a host granted nothing changed the document through the API",
  ).toEqual(before);

  // A refusal is also an EVENT, and it crossed the boundary: a host that only
  // listens must learn the same thing as a host that awaits.
  const events = await page.evaluate(() => window.__events.map((event) => event.event));
  expect(events, "no refusal reached the host over postMessage").toContain("refusal");
  await page.evaluate(() => window.__wire.dispose());
});

test("an owner host succeeds over both transports, and the editor really moved", async ({
  page,
}) => {
  // The POSITIVE CONTROL for the sweep above. Without it, a contract that refused
  // absolutely everything would pass every assertion in this file — and this
  // repository has shipped exactly that kind of green-but-worthless guard (`105`
  // CQ-003).
  await page.goto("/embed.html");
  const { frame } = await mountEmbedPanel(page, "iframe", "owner");
  await page.evaluate(async (origin) => {
    const { createHostClient } = await import("./src/host_client.mjs");
    const iframe = document.querySelector('[data-embed="iframe"] iframe');
    window.__wire = createHostClient({ frame: iframe, editorOrigin: origin });
    window.__events = [];
    for (const name of (await window.__wire.describe()).events) {
      window.__wire.on(name, (event) => window.__events.push(event));
    }
  }, new URL(page.url()).origin);

  await frame.click(".page-wrap .page", { position: { x: 60, y: 60 } });
  await frame.locator("body").press("Home");
  await frame.locator("body").press("Shift+End");

  // In process, then over the wire, and both must actually apply.
  const direct = await frame.evaluate(() => window.opendoc.execute("format.bold"));
  expect(direct.ok, `bold was refused in process: ${JSON.stringify(direct)}`).toBe(true);
  const wire = await page.evaluate(() => window.__wire.execute("format.bold"));
  expect(wire.ok, `bold was refused over the wire: ${JSON.stringify(wire)}`).toBe(true);

  // The REVISION moved, which is the engine's own word that something landed, and
  // it is the handle the `change` event carries.
  expect(
    wire.revision,
    "the revision did not advance, so neither transport actually edited anything",
  ).toBeGreaterThan(0);
  const change = await page.evaluate(() =>
    window.__events.filter((event) => event.event === "change").map((event) => event.detail),
  );
  expect(change.length, "no change event reached the host").toBeGreaterThan(0);
  // A revision handle and a dirty flag, and nothing else: an event that carried a
  // snapshot would make every keystroke O(document) (`docs/107` §4).
  expect(Object.keys(change.at(-1)).sort()).toEqual(["dirty", "revision"]);
  expect(change.at(-1).dirty).toBe(true);
  await page.evaluate(() => window.__wire.dispose());
});

test("a refusal the CHROME makes is a refusal the API makes", async ({ page }) => {
  // The direction that matters. The API may be stricter than the chrome — it is,
  // deliberately, for `review.accept*` under `commentor` — but it must never be
  // looser, or a capability withheld in the chrome is available through the SDK
  // and the whole layering is theatre.
  await page.goto("/embed.html");
  const { frame } = await mountEmbedPanel(page, "element", "readonly");
  const leaked = await frame.evaluate(async () => {
    const out = [];
    for (const command of window.opendoc.describe().commands) {
      if (command.available) continue;
      // The chrome refuses it, either because the host withheld the capability or
      // because it cannot run right now. Either way the API must not run it.
      const result = await window.opendoc.execute(command.id);
      if (result.ok) out.push(command.id);
    }
    return out;
  });
  expect(leaked, "the API ran commands the chrome refuses").toEqual([]);
  // And the converse control: something the chrome DOES offer really does run,
  // so this test cannot pass on an editor where nothing works at all.
  const printable = await frame.evaluate(() => window.opendoc.query("file.print"));
  expect(printable.granted, "readonly must still be granted print").toBe(true);
});

test("a command the contract calls ungated really does not touch the document", async ({ page }) => {
  // THE MIS-DECLARATION HOLE, closed by derivation rather than by a second table.
  //
  // Every other assertion in this file reads `requires` from the contract, so a
  // command declared ungated that actually mutates is invisible to all of them:
  // measured, by re-declaring `format.bold` as `requires: null` and watching the
  // whole sweep stay green — the engine's Viewing gate caught the edit, the
  // document did not move, and the API's own claim was never checked.
  //
  // So the claim is checked against the engine. Run each ungated command as an
  // `owner`, where nothing is refused, and assert the revision watermark and the
  // engine's word and character counts do not move. A command that changes the
  // document cannot pass this while calling itself ungated, and the list is
  // DERIVED from the contract, so a new ungated row is covered the moment it is
  // added.
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const ungated = COMMAND_CONTRACT.filter((row) => row.requires === null).map((row) => row.id);
  expect(ungated.length, "no ungated commands, so this proves nothing").toBeGreaterThan(10);
  // `file.print` and `file.open` are NOT in this set — both require a capability —
  // so nothing here can hand control to the operating system.
  expect(ungated.filter((id) => BLOCKS_THE_RENDERER.has(id))).toEqual([]);

  const moved = await page.evaluate(async (ids) => {
    const stats = () => ({
      revision: window.opendoc.ping().revision,
      words: document.getElementById("statWords")?.textContent ?? "",
      chars: document.getElementById("statChars")?.textContent ?? "",
    });
    const out = [];
    for (const id of ids) {
      const before = stats();
      await window.opendoc.execute(id);
      const after = stats();
      if (JSON.stringify(before) !== JSON.stringify(after)) {
        out.push(`${id}: ${JSON.stringify(before)} -> ${JSON.stringify(after)}`);
      }
    }
    return out;
  }, ungated);
  expect(
    moved,
    "a command the host contract calls ungated changed the document — either it " +
      "needs a requirement in COMMAND_CONTRACT, or it should not be doing that",
  ).toEqual([]);

  // The positive control: a command the contract DOES gate really does move the
  // document for an owner, so this test cannot pass on an editor where nothing
  // works at all.
  const edited = await page.evaluate(async () => {
    const before = window.opendoc.ping().revision;
    await window.opendoc.execute("format.bold");
    return { before, after: window.opendoc.ping().revision };
  });
  expect(edited.after, "bold did not advance the revision for an owner").toBeGreaterThan(
    edited.before,
  );
});
