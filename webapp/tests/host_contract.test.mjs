// The host contract, asserted as a contract rather than as a table.
//
// `docs/126` phase 2. Every test here asks the question a host would ask, not
// "does the source still say what it said": what may this role run, does the
// wire answer the same thing the reference does, and where does a message go.
// The repository's own crop of failures in this area is the reason — a guard
// allowlisting the capabilities `main.js` happened to consult, a guard accepting
// one spelling of an authority, a round-trip guard that passed with the styling
// removed. `SKILL.md` §4: assert the guarantee, not the circumstance.
//
// The strongest test in the file is `both transports return the identical
// result`: the `postMessage` client and the in-process session are driven over
// the same session with the same command list, and their results are compared
// field by field. It is the whole of "one schema, two transports" in node, and it
// goes red the instant either transport grows an opinion of its own.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import { ROLES, resolveCapabilities } from "../src/capabilities.mjs";
import {
  COMMAND_CONTRACT,
  COMMAND_FAMILIES,
  CONTRACT_VERSION,
  FAMILY_STATES,
  HOST_EVENTS,
  HOST_EVENT_NAMES,
  PROTOCOL,
  REFUSAL_CODES,
  REQUIREMENTS,
  capabilityRefusal,
  commandContract,
  declaredRequirements,
  grantsRequirement,
  isEditorEnvelope,
  isRequestEnvelope,
  originAllowed,
  parseOriginAllowlist,
} from "../src/host_contract.mjs";
import { allowlistFor, attachHostBridge } from "../src/host_bridge.mjs";
import { createHostClient } from "../src/host_client.mjs";
import { createHostSession } from "../src/host_session.mjs";

const EDITOR_ORIGIN = "https://editor.example";
const OTHER_ORIGIN = "https://other.example";

/** Every contract id, exact rows and one synthetic member per family, so a sweep
 *  covers the family resolution path as well as the literal one. */
function everyContractId() {
  return [
    ...COMMAND_CONTRACT.map((row) => row.id),
    ...COMMAND_FAMILIES.map((row) => `${row.prefix}synthetic`),
  ];
}

/** A registry that offers every contract id, enabled, recording what ran. */
function fakeRegistry(ran, { disable = new Set(), throwOn = new Set(), status = null } = {}) {
  return () =>
    everyContractId().map((id) => ({
      id,
      label: id,
      enabled: !disable.has(id),
      disabledReason: disable.has(id) ? `no: ${id}` : "",
      run: () => {
        if (throwOn.has(id)) throw new Error(`boom ${id}`);
        ran.push(id);
        if (status) status(id);
      },
    }));
}

// ── The schema itself ───────────────────────────────────────────────────────

test("every requirement the contract uses is one the resolver can answer, both ways", () => {
  const used = declaredRequirements();
  assert.deepEqual(
    used.filter((name) => !REQUIREMENTS.includes(name)),
    [],
    "a requirement the resolver does not know falls through `grantsRequirement`'s " +
      "capability branch and is answered by `capabilities.has()` — which for a typo " +
      "is always false, so the command becomes unreachable rather than ungated. " +
      "Either spelling of that bug is a silent one.",
  );
  assert.deepEqual(
    REQUIREMENTS.filter((name) => !used.includes(name)),
    [],
    "a requirement nothing uses is dead vocabulary: the next reader will assume " +
      "something is gated by it",
  );
});

test("no command id is declared twice, and no family shadows an exact row", () => {
  const ids = COMMAND_CONTRACT.map((row) => row.id);
  assert.equal(new Set(ids).size, ids.length, "a duplicate row means one of the two is ignored");
  for (const row of COMMAND_FAMILIES) {
    assert.ok(row.prefix.endsWith("."), `family ${row.prefix} must end in a dot`);
    assert.ok(FAMILY_STATES.includes(row.state), `family ${row.prefix} names an unknown state`);
  }
  // The resolution ORDER, as a guarantee: `style.createFromSelection` defines a
  // style and `style.Heading 1` applies one, and they do not have the same
  // requirement path. If the family won, the exact row would be dead.
  const shadowed = COMMAND_CONTRACT.filter((row) =>
    COMMAND_FAMILIES.some((f) => row.id.startsWith(f.prefix)),
  );
  assert.ok(shadowed.length > 0, "no exact row sits under a family, so this proves nothing");
  for (const row of shadowed) {
    assert.equal(
      commandContract(row.id).family,
      undefined,
      `${row.id} resolved through a family, so its exact declaration is dead`,
    );
    assert.equal(commandContract(row.id).requires, row.requires);
  }
});

test("a family member resolves to its family, and an unknown id resolves to nothing", () => {
  assert.equal(commandContract("style.Heading 1").requires, "mutate");
  assert.equal(commandContract("style.Heading 1").family, "style.");
  for (const id of ["", null, undefined, "nope", "nope.nope", "format", "styleHeading"]) {
    assert.equal(commandContract(id), null, `${String(id)} must not resolve`);
  }
});

test("every declared event's detail is exactly what the session emits", async () => {
  const session = createHostSession({
    capabilities: resolveCapabilities({ mode: "owner", framed: true }),
    registry: fakeRegistry([]),
    revision: () => 7,
    dirty: () => true,
    selection: () => ({ anchor: { node: "n", offset: 0 }, focus: { node: "n", offset: 1 }, hasRange: true }),
  });
  const seen = new Map();
  for (const name of HOST_EVENT_NAMES) session.on(name, (e) => seen.set(e.event, e.detail));
  session.noteReady();
  session.noteChange();
  session.noteSelection();
  session.noteWrite("save", { format: "docx", name: "a.docx", bytes: 12 });
  session.noteWrite("export", { format: "pdf", name: "a.pdf", bytes: 34 });
  session.noteError("threw", "why", "edit.undo");
  await session.execute("nope.nope");
  for (const event of HOST_EVENTS) {
    const detail = seen.get(event.name);
    assert.ok(detail, `${event.name} was never emitted, so its shape is unchecked`);
    assert.deepEqual(
      Object.keys(detail).sort(),
      [...event.detail].sort(),
      `${event.name}'s payload and its declaration disagree — a host reading the ` +
        "schema would be reading a different event from the one it receives",
    );
  }
});

test("ready is emitted once, however often the editor says so", () => {
  const session = createHostSession({
    capabilities: resolveCapabilities({ mode: "owner", framed: true }),
    registry: fakeRegistry([]),
  });
  let count = 0;
  session.on("ready", () => (count += 1));
  session.noteReady();
  session.noteReady();
  session.noteReady();
  assert.equal(count, 1, "a host that hears ready twice cannot tell a reopen from a bug");
});

// ── The gate ───────────────────────────────────────────────────────────────

/** What a role may run, by contract row, through the gate the API uses. */
function allowedFor(role) {
  const capabilities = resolveCapabilities({ mode: role, framed: true });
  return new Set(
    everyContractId().filter((id) => grantsRequirement(commandContract(id).requires, capabilities)),
  );
}

test("each role may run exactly the requirements its capabilities cover", () => {
  /** role -> the requirements it may satisfy. Written out, because this is the
   *  PRODUCT claim: it is the table a host reads the roles from, and deriving it
   *  from `resolveCapabilities` would make the test a restatement of the code.
   *  `null` (ungated) is always allowed and is not listed. */
  const expected = {
    preview: [],
    readonly: ["print"],
    commentor: ["print", "download", "comment", "mutate"],
    edit: ["print", "download", "save", "comment", "edit", "mutate", "autosave"],
    owner: [...REQUIREMENTS],
  };
  for (const role of ROLES) {
    const capabilities = resolveCapabilities({ mode: role, framed: true });
    const satisfied = REQUIREMENTS.filter((name) => grantsRequirement(name, capabilities));
    assert.deepEqual(
      satisfied.sort(),
      [...expected[role]].sort(),
      `${role} satisfies the wrong set of requirements`,
    );
  }
});

test("a preview or readonly host cannot run ANY mutating command through the API", () => {
  for (const role of ["preview", "readonly"]) {
    const capabilities = resolveCapabilities({ mode: role, framed: true });
    const leaked = everyContractId().filter((id) => {
      const row = commandContract(id);
      return row.requires === "mutate" && grantsRequirement(row.requires, capabilities);
    });
    assert.deepEqual(leaked, [], `${role} reached a mutating command through the API`);
  }
  // And the converse, or the assertion above would pass on an empty contract.
  const owner = resolveCapabilities({ mode: "owner", framed: true });
  const mutating = everyContractId().filter((id) => commandContract(id).requires === "mutate");
  assert.ok(mutating.length > 50, `only ${mutating.length} mutating commands — the sweep is too thin`);
  for (const id of mutating) {
    assert.ok(grantsRequirement("mutate", owner), `owner cannot run ${id}`);
  }
});

test("a commentor may suggest but may not decide, and may not save", () => {
  const capabilities = resolveCapabilities({ mode: "commentor", framed: true });
  // Suggesting: a body change is recorded as a tracked revision, which is the
  // whole of the role. So `mutate` is granted.
  assert.ok(grantsRequirement(commandContract("format.bold").requires, capabilities));
  assert.ok(grantsRequirement(commandContract("review.comment").requires, capabilities));
  // Deciding a change writes it into the document outright — Google Docs'
  // Commenter cannot, and Word's reviewer cannot.
  for (const id of ["review.acceptAll", "review.rejectNext", "review.mode.editing"]) {
    assert.equal(
      grantsRequirement(commandContract(id).requires, capabilities),
      false,
      `a commentor reached ${id}`,
    );
  }
  assert.equal(grantsRequirement(commandContract("file.save").requires, capabilities), false);
});

test("the role chain is monotone through the API gate, not only through the capability set", () => {
  // `roles.test.mjs` asserts the capability sets are monotone. This asserts the
  // consequence a host actually experiences: a higher role never loses a command
  // a lower one could run. That is the classic permissions defect, and it can be
  // reintroduced by a `requires` change without touching a capability set at all.
  for (let i = 1; i < ROLES.length; i += 1) {
    const lower = allowedFor(ROLES[i - 1]);
    const higher = allowedFor(ROLES[i]);
    const lost = [...lower].filter((id) => !higher.has(id));
    assert.deepEqual(lost, [], `${ROLES[i]} lost what ${ROLES[i - 1]} could run`);
  }
  // Strictly increasing where the capability sets are: otherwise a chain of five
  // identical roles would satisfy the above.
  assert.ok(
    allowedFor("owner").size > allowedFor("preview").size,
    "owner and preview may run the same commands, so the gate does nothing",
  );
});

test("the gate fails closed on a capability set that is not a set", () => {
  for (const broken of [null, undefined, {}, { has: () => { throw new Error("no"); } }, "edit"]) {
    assert.equal(grantsRequirement("save", broken), false, "an exception on the permission path is a permission decided by accident");
    assert.equal(grantsRequirement("mutate", broken), false);
    // And an ungated command still runs: failing closed must not break reading.
    assert.equal(grantsRequirement(null, broken), true);
  }
});

test("a withheld capability produces a refusal that names the code and the requirement", () => {
  const refusal = capabilityRefusal(commandContract("file.save"), new Set(["print"]), "SENTENCE");
  assert.equal(refusal.code, "capability-withheld");
  assert.equal(refusal.command, "file.save");
  assert.equal(refusal.requires, "save");
  assert.equal(refusal.message, "SENTENCE");
  assert.ok(REFUSAL_CODES.includes(refusal.code));
  assert.equal(capabilityRefusal(commandContract("file.save"), new Set(["save"]), "x"), null);
});

// ── The result, and what a refusal says ────────────────────────────────────

test("a refused command says so in its result, with the chrome's own reason", async () => {
  const ran = [];
  const session = createHostSession({
    capabilities: resolveCapabilities({ mode: "owner", framed: true }),
    registry: fakeRegistry(ran, { disable: new Set(["edit.undo"]) }),
    revision: () => 3,
  });
  const refused = await session.execute("edit.undo");
  assert.equal(refused.ok, false);
  assert.equal(refused.refusal.code, "unavailable");
  assert.equal(refused.refusal.message, "no: edit.undo", "the reason must be the chrome's, already localised");
  assert.equal(refused.revision, 3);
  assert.deepEqual(ran, [], "a refused command must not have run");
});

test("a command refused BY THE ENGINE is reported from what the editor said", async () => {
  const ran = [];
  let session;
  session = createHostSession({
    capabilities: resolveCapabilities({ mode: "owner", framed: true }),
    // The editor refuses `format.bold` at its Viewing choke point, which reaches
    // the feedback channel as a status of kind "error" — exactly the shape
    // `blockMutationInViewing` produces.
    registry: fakeRegistry(ran, {
      status: (id) => {
        if (id === "format.bold") session.noteStatus("Viewing mode is read-only", "error");
        else session.noteStatus("Bolded", "");
      },
    }),
  });
  const refused = await session.execute("format.bold");
  assert.equal(refused.ok, false);
  assert.equal(refused.refusal.code, "engine-refused");
  assert.match(refused.refusal.message, /read-only/);
  // A CONFIRMATION during the command is not a refusal. This is the assertion
  // that stops the mechanism from reporting every command as refused, which would
  // pass the test above while being useless.
  const fine = await session.execute("format.italic");
  assert.equal(fine.ok, true, "a confirmation was read as a refusal");
  assert.equal(fine.refusal, undefined);
});

test("a command that throws is a refusal AND an error, and does not escape", async () => {
  const session = createHostSession({
    capabilities: resolveCapabilities({ mode: "owner", framed: true }),
    registry: fakeRegistry([], { throwOn: new Set(["edit.cut"]) }),
  });
  const errors = [];
  const refusals = [];
  session.on("error", (e) => errors.push(e.detail));
  session.on("refusal", (e) => refusals.push(e.detail));
  const result = await session.execute("edit.cut");
  assert.equal(result.ok, false);
  assert.equal(result.refusal.code, "threw");
  assert.match(result.refusal.message, /boom edit\.cut/);
  assert.equal(errors.length, 1, "a host watching only `error` must hear it");
  assert.equal(refusals.length, 1, "a host watching only `refusal` must hear it too");
});

test("every refusal a host can be handed is also announced on the refusal event", async () => {
  const session = createHostSession({
    capabilities: resolveCapabilities({ mode: "readonly", framed: true }),
    registry: fakeRegistry([], { disable: new Set(["edit.copy"]) }),
    withheldMessage: () => "WITHHELD",
  });
  const heard = [];
  session.on("refusal", (e) => heard.push(e.detail.code));
  const results = [
    await session.execute("format.bold"), // capability-withheld
    await session.execute("edit.copy"), // unavailable
    await session.execute("not.a.command"), // unknown-command
  ];
  assert.deepEqual(
    results.map((r) => r.refusal.code),
    ["capability-withheld", "unavailable", "unknown-command"],
  );
  assert.deepEqual(heard, ["capability-withheld", "unavailable", "unknown-command"]);
});

test("query reports the four facts separately, because they are four decisions", () => {
  const session = createHostSession({
    capabilities: resolveCapabilities({ mode: "readonly", framed: true }),
    registry: () => [{ id: "file.print", enabled: false, disabledReason: "Nothing to print", run: () => {} }],
  });
  const printable = session.query("file.print");
  assert.deepEqual(printable, {
    id: "file.print",
    known: true,
    present: true,
    requires: "print",
    granted: true,
    available: false,
    reason: "Nothing to print",
  });
  // Known but absent, which is what every table command is when the caret is not
  // in a table. A host must be able to tell that from "you may not".
  const absent = session.query("table.merge");
  assert.equal(absent.known, true);
  assert.equal(absent.present, false);
  assert.equal(absent.available, false);
  assert.equal(session.query("not.a.command").known, false);
});

// ── The origin contract ────────────────────────────────────────────────────

test("the allowlist always holds the editor's own origin and never holds anyone", () => {
  const list = parseOriginAllowlist("https://a.example, https://b.example", EDITOR_ORIGIN);
  assert.deepEqual([...list], [EDITOR_ORIGIN, "https://a.example", "https://b.example"]);
  // The two spellings of "anyone", and the shapes that are not an origin.
  for (const bad of ["*", "null", "", "   ", "https://c.example/path", "not a url", "example.com"]) {
    const guarded = parseOriginAllowlist(bad, EDITOR_ORIGIN);
    assert.deepEqual([...guarded], [EDITOR_ORIGIN], `${bad} was accepted into the allowlist`);
  }
  // A typo must NARROW, never widen.
  assert.deepEqual([...parseOriginAllowlist("htps://a.example", EDITOR_ORIGIN)], [EDITOR_ORIGIN]);
  assert.equal(originAllowed("*", list), false);
  assert.equal(originAllowed("null", list), false);
  assert.equal(originAllowed(null, list), false);
  assert.equal(originAllowed("https://a.example", list), true);
  assert.equal(originAllowed("https://a.example.evil", list), false);
});

test("an envelope is checked, not trusted", () => {
  const good = { opendoc: 1, kind: "request", rid: "r1", type: "ping" };
  assert.equal(isRequestEnvelope(good), true);
  for (const bad of [
    null,
    "opendoc",
    { ...good, opendoc: 2 },
    { ...good, opendoc: undefined },
    { ...good, kind: "event" },
    { ...good, rid: "" },
    { ...good, rid: 7 },
    { ...good, type: "evaluate" },
  ]) {
    assert.equal(isRequestEnvelope(bad), false, `${JSON.stringify(bad)} was accepted`);
  }
  assert.equal(isEditorEnvelope({ opendoc: 1, kind: "result" }), true);
  assert.equal(isEditorEnvelope({ opendoc: 1, kind: "event" }), true);
  assert.equal(isEditorEnvelope({ opendoc: 1, kind: "request" }), false);
});

// ── The two transports ─────────────────────────────────────────────────────

/** A window that records what was posted to it and delivers a message only when
 *  the target origin really is its own — which is the one rule that makes an
 *  explicit target origin worth anything. */
function makeWindow(origin, search = "") {
  const win = {
    location: { origin, search },
    listeners: new Set(),
    sent: [],
    addEventListener(type, fn) {
      if (type === "message") win.listeners.add(fn);
    },
    removeEventListener(type, fn) {
      win.listeners.delete(fn);
    },
    postMessage(data, targetOrigin) {
      win.sent.push({ data, targetOrigin });
      if (targetOrigin !== win.location.origin) return; // the browser drops it
      for (const fn of [...win.listeners]) fn({ data, origin: win.peerOrigin, source: win.peer });
    },
  };
  return win;
}

/** A host window and an editor window wired as each other's peer.
 *
 *  The default is SAME-ORIGIN, because that is what a real embed is:
 *  `sandboxTokensFor` grants `allow-same-origin` unconditionally — without it the
 *  frame cannot reach its own WebAssembly — so `<opendoc-editor>` produces a
 *  same-origin pair and the contract has to work with no configuration at all.
 *  The cross-origin cases are asked for explicitly, which is also how a
 *  deployment asks for them. */
function wire({ hostOrigin = EDITOR_ORIGIN, search = "" } = {}) {
  const host = makeWindow(hostOrigin);
  const editor = makeWindow(EDITOR_ORIGIN, search);
  host.peer = editor;
  host.peerOrigin = EDITOR_ORIGIN;
  editor.peer = host;
  editor.peerOrigin = hostOrigin;
  editor.parent = host;
  return { host, editor };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

/** A bridge plus a client over one session. */
function connect({ role = "owner", search = "", hostOrigin = EDITOR_ORIGIN, ran = [] } = {}) {
  const { host, editor } = wire({ hostOrigin, search });
  const session = createHostSession({
    capabilities: resolveCapabilities({ mode: role, framed: true }),
    registry: fakeRegistry(ran),
    withheldMessage: () => "WITHHELD",
    revision: () => 11,
    dirty: () => false,
  });
  const bridge = attachHostBridge({ session, view: editor });
  const client = createHostClient({
    frame: { contentWindow: editor },
    editorOrigin: EDITOR_ORIGIN,
    view: host,
    timeout: 250,
  });
  return { host, editor, session, bridge, client, ran };
}

test("the bridge serves every verb the schema declares, and no other", async () => {
  const { client } = connect();
  // Generated, not enumerated: a verb added to `PROTOCOL.requests` with no
  // handler fails here rather than being discovered by a host.
  for (const type of PROTOCOL.requests) {
    assert.equal(typeof client[type], "function", `the client has no ${type}`);
  }
  assert.equal((await client.ping()).contract, CONTRACT_VERSION);
  assert.equal((await client.describe()).protocol, PROTOCOL.version);
  assert.equal((await client.query("file.save")).requires, "save");
  assert.equal((await client.execute("edit.copy")).ok, true);
});

test("both transports return the identical result, for every command in the contract", async () => {
  const wire1 = [];
  const wire2 = [];
  const over = connect({ role: "commentor", ran: wire1 });
  const direct = createHostSession({
    capabilities: resolveCapabilities({ mode: "commentor", framed: true }),
    registry: fakeRegistry(wire2),
    withheldMessage: () => "WITHHELD",
    revision: () => 11,
    dirty: () => false,
  });
  const disagreements = [];
  for (const id of everyContractId()) {
    const a = await direct.execute(id);
    const b = await over.client.execute(id);
    if (JSON.stringify(a) !== JSON.stringify(b)) {
      disagreements.push(`${id}: in-process ${JSON.stringify(a)} vs wire ${JSON.stringify(b)}`);
    }
  }
  assert.deepEqual(disagreements, [], "the two transports are not the same contract");
  // Both must have actually done something, or "identical" is two empty sets.
  assert.ok(wire1.length > 40, `only ${wire1.length} commands ran in process`);
  assert.deepEqual(wire1, wire2, "the two transports ran a different set of commands");
  // And a commentor really was gated on the wire, not just in process.
  const decided = await over.client.execute("review.acceptAll");
  assert.equal(decided.ok, false);
  assert.equal(decided.refusal.code, "capability-withheld");
  over.client.dispose();
});

test("nothing is ever posted to a wildcard, in either direction", async () => {
  // Driven rather than grepped. A source scan for `"*"` would pass the moment the
  // wildcard arrived through a variable, and this repository has shipped guards
  // that checked a spelling instead of a behaviour. So both transports are run
  // and every target origin they actually named is inspected.
  for (const [label, options] of [
    ["same-origin", {}],
    [
      "cross-origin, named",
      { hostOrigin: OTHER_ORIGIN, search: `?hostOrigin=${OTHER_ORIGIN}` },
    ],
  ]) {
    const { host, editor, session, client, bridge } = connect(options);
    session.noteReady();
    session.noteChange();
    await client.execute("edit.copy");
    await flush();
    const targets = [...host.sent, ...editor.sent].map((m) => m.targetOrigin);
    assert.ok(targets.length > 3, `${label}: nothing was posted, so this proves nothing`);
    const permitted = new Set([...bridge.allowlist, EDITOR_ORIGIN]);
    assert.deepEqual(
      [...new Set(targets)].filter((origin) => !permitted.has(origin)),
      [],
      `${label}: a message was posted somewhere other than an allowed origin`,
    );
    // And a named cross-origin run really did address two distinct origins, so
    // the same-origin case cannot be the only thing this test ever sees.
    if (label !== "same-origin") {
      assert.deepEqual([...new Set(targets)].sort(), [EDITOR_ORIGIN, OTHER_ORIGIN].sort());
    }
    client.dispose();
  }
});

test("a message from an origin the editor does not know is dropped in silence", async () => {
  const { editor, ran } = connect();
  // The REPLY is recorded on the spoofed source, not on the editor's own window.
  // An earlier draft of this test watched `editor.sent`, and removing the origin
  // check left it green: the bridge answered the attacker, and the attacker's
  // window was not the one being measured. That is the "guard whose fixture
  // cannot exercise the path" failure, caught by mutating the code.
  const attacker = { sent: [], postMessage: (data, origin) => attacker.sent.push({ data, origin }) };
  for (const origin of ["https://evil.example", "null", "*", "https://editor.example.evil", ""]) {
    for (const fn of [...editor.listeners]) {
      fn({
        data: { opendoc: 1, kind: "request", rid: "x", type: "execute", id: "format.bold" },
        origin,
        source: attacker,
      });
    }
  }
  await flush();
  assert.deepEqual(
    attacker.sent,
    [],
    "an unknown origin got an answer — even an error reply tells a prober what this frame is",
  );
  assert.deepEqual(ran, [], "an unknown origin ran a command");
  // The positive control: the SAME envelope from an allowed origin is answered,
  // so this test cannot pass because the envelope was malformed.
  const peer = { sent: [], postMessage: (data, origin) => peer.sent.push({ data, origin }) };
  for (const fn of [...editor.listeners]) {
    fn({
      data: { opendoc: 1, kind: "request", rid: "x", type: "execute", id: "format.bold" },
      origin: EDITOR_ORIGIN,
      source: peer,
    });
  }
  await flush();
  assert.equal(peer.sent.length, 1, "an allowed origin was not answered either");
  assert.deepEqual(ran, ["format.bold"]);
});

test("a cross-origin host hears nothing until the deployment names it", async () => {
  // Default: the allowlist is the editor's own origin alone, so a cross-origin
  // parent's requests are dropped and its events never arrive. Fail closed.
  const closed = connect({ hostOrigin: OTHER_ORIGIN });
  assert.deepEqual([...closed.bridge.allowlist], [EDITOR_ORIGIN]);
  closed.session.noteReady();
  await flush();
  assert.equal(
    closed.host.sent.filter((m) => m.targetOrigin === OTHER_ORIGIN).length,
    0,
    "an unnamed cross-origin host received an event",
  );
  // And its REQUESTS go unanswered, which is the half that matters: an editor
  // that refuses to talk but still obeys is not fail-closed.
  const refusedResult = await closed.client.execute("format.bold");
  assert.equal(refusedResult.refusal.code, "timeout", "an unnamed cross-origin host was served");
  assert.deepEqual(closed.ran, [], "an unnamed cross-origin host ran a command");
  closed.client.dispose();

  // Named: `?hostOrigin=` opens exactly that one origin, and nothing else.
  const open = connect({
    hostOrigin: OTHER_ORIGIN,
    search: `?hostOrigin=${OTHER_ORIGIN}`,
  });
  assert.deepEqual([...open.bridge.allowlist], [EDITOR_ORIGIN, OTHER_ORIGIN]);
  const events = [];
  open.client.on("ready", (e) => events.push(e.detail));
  open.session.noteReady();
  await flush();
  assert.equal(events.length, 1, "a named cross-origin host heard nothing");
  assert.equal(events[0].contract, CONTRACT_VERSION);
  open.client.dispose();
});

test("allowlistFor reads the parameter off a real-shaped window and nothing else", () => {
  assert.deepEqual(
    [...allowlistFor({ location: { origin: EDITOR_ORIGIN, search: "?mode=readonly&hostOrigin=https://a.example" } })],
    [EDITOR_ORIGIN, "https://a.example"],
  );
  assert.deepEqual([...allowlistFor({ location: { origin: EDITOR_ORIGIN, search: "?mode=readonly" } })], [EDITOR_ORIGIN]);
  assert.deepEqual([...allowlistFor({})], []);
});

test("a host that connects after the editor was ready still hears ready", async () => {
  const { session, client } = connect();
  session.noteReady();
  await flush();
  const heard = [];
  client.on("ready", (e) => heard.push(e.detail));
  await client.describe();
  await flush();
  assert.equal(heard.length, 1, "a late host can never learn the editor is ready");
  client.dispose();
});

test("a client cannot be created without an explicit target origin", () => {
  const { editor, host } = wire();
  for (const origin of [undefined, null, "", "*"]) {
    assert.throws(
      () => createHostClient({ frame: { contentWindow: editor }, editorOrigin: origin, view: host }),
      /explicit editorOrigin/,
      `${String(origin)} was accepted as a target origin`,
    );
  }
});

test("a disposed client fails its in-flight requests instead of hanging", async () => {
  const { client, editor } = connect();
  editor.listeners.clear(); // the bridge is gone; nothing will answer
  const pending = client.describe();
  assert.equal(client.inFlight(), 1);
  client.dispose();
  await assert.rejects(pending, /disposed/);
});

test("an unanswered execute resolves as a timeout refusal, not a rejection", async () => {
  const { client, editor } = connect();
  editor.listeners.clear();
  const result = await client.execute("edit.copy");
  assert.equal(result.ok, false);
  assert.equal(result.refusal.code, "timeout", "a host's `if (!result.ok)` must cover a dead transport");
  client.dispose();
});

// ── Documentation of the contract ──────────────────────────────────────────

test("the contract's vocabulary is the one the Rust facade declares", () => {
  // `crates/casual-doc-sdk` is the designed host facade (`docs/05` §§4-5) and it
  // has no product consumer. Phase 2 does not close that — the live editing path
  // bypasses `casual-doc-transaction` entirely (`109` CQ-002), which is its own
  // row — but the two must not be allowed to invent separate vocabularies in the
  // meantime, or unifying them later becomes a migration instead of a wiring.
  // So the crate declares the same event kinds and refusal codes, and ITS OWN
  // test reads this file. This half asserts the file stays readable by that test:
  // a reshuffle that hid these lists from a line-oriented reader would turn a
  // cross-language guard into a silent pass.
  const source = readFileSync(new URL("../src/host_contract.mjs", import.meta.url), "utf8");
  for (const name of HOST_EVENT_NAMES) {
    assert.match(source, new RegExp(`name: "${name}"`), `${name} is not declared in a parseable form`);
  }
  for (const code of REFUSAL_CODES) {
    assert.match(source, new RegExp(`^  "${code}",$`, "m"), `${code} is not declared in a parseable form`);
  }
});
