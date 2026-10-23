// Host identity, the fourth host input (`docs/175` §3, ADR-070, `docs/109` HF-360).
//
// What is held here, each by a guard that was driven red by mutating the code it
// guards (the mutations are recorded in the commit that added this file):
//
//   * ONE validator, whose bounds are REFUSALS naming the field, never truncations;
//     `group` is reported `not-honoured`, not refused;
//   * an identity is decided ONCE: a second `identify`, or one after a room was
//     joined, is refused `identity-fixed`, on both transports alike;
//   * the element NEVER puts the identity in the frame's URL — it says only that
//     one is coming, then hands it over in a one-shot message to an explicit origin;
//   * when it is given it is the only identity: Settings' author fields are
//     disabled WITH the reason, `prefs.user`/`prefs.initials` are superseded;
//   * a room join carries the host's id and profile, and asks for a grant PER join;
//   * the people strip and the caret label read profile-bearing rows through one
//     seam: no avatar without consent and nothing fetched, a claimed name marked
//     unverified on the caret as on the chip, and a duplicate suffix that never
//     renames the profile it was handed.
import test from "node:test";
import assert from "node:assert/strict";
import { EN_STRINGS } from "../src/en_strings.mjs";
import { IDENTITY_BOUNDS, PROTOCOL, REFUSAL_CODES, validateHostIdentity } from "../src/host_contract.mjs";
import {
  activeAuthor,
  avatarPolicy,
  createIdentityHolder,
  identityPromised,
  joinIdentity,
  reflectAuthorIdentity,
} from "../src/host_identity.mjs";
import { createHostSession } from "../src/host_session.mjs";
import { attachHostBridge } from "../src/host_bridge.mjs";
import { createHostClient } from "../src/host_client.mjs";
import { resolveCapabilities } from "../src/capabilities.mjs";
import { collabTransport } from "../src/collab_transport.mjs";
import { PRESENCE_OUTCOMES, createPresence } from "../src/collab_presence.mjs";
import { displayRoster, participantDisplay } from "../src/presence_strip.mjs";

function t(key, params = {}) {
  const plural = EN_STRINGS[`${key}.${params.count === 1 ? "one" : "other"}`];
  const text = EN_STRINGS[key] ?? plural;
  assert.ok(text, `no English for ${key}`);
  return text.replace(/\{(\w+)\}/g, (_, name) => String(params[name]));
}

const ADA = Object.freeze({ id: "u_8f2c", name: "Ada Lovelace" });

// ---- The validator -----------------------------------------------------------

test("every bound is a refusal naming its field, never a truncation", () => {
  const over = (bytes) => "a".repeat(bytes + 1);
  const cases = [
    [null, "identity"],
    ["{not json", "identity"],
    [[], "identity"],
    [{ name: "Ada" }, "identity.id"],
    [{ ...ADA, id: over(IDENTITY_BOUNDS.idBytes) }, "identity.id"],
    [{ ...ADA, id: "   " }, "identity.id"],
    [{ id: "u" }, "identity.name"],
    [{ ...ADA, name: over(IDENTITY_BOUNDS.nameBytes) }, "identity.name"],
    // 128 two-byte characters: 256 bytes, though only 128 characters.
    [{ ...ADA, name: "é".repeat(128) }, "identity.name"],
    [{ ...ADA, initials: "ABCDEFGHI" }, "identity.initials"],
    [{ ...ADA, color: "red" }, "identity.color"],
    [{ ...ADA, avatar: "https://cdn.example/a.png" }, "identity.avatar"],
    [{ ...ADA, avatar: { url: "http://cdn.example/a.png" } }, "identity.avatar.url"],
    [{ ...ADA, avatar: { url: "javascript:alert(1)" } }, "identity.avatar.url"],
    [{ ...ADA, avatar: { url: "https://user:pw@cdn.example/a.png" } }, "identity.avatar.url"],
    [{ ...ADA, avatar: { url: `https://cdn.example/${"a".repeat(1024)}` } }, "identity.avatar.url"],
    [{ ...ADA, avatar: { url: `data:image/png;base64,${"A".repeat(1024)}` } }, "identity.avatar.url"],
    [{ ...ADA, avatar: { url: "data:image/svg+xml;base64,AAAA" } }, "identity.avatar.url"],
    [{ ...ADA, persist: { providerId: "AD" } }, "identity.persist.userId"],
    [{ ...ADA, persist: { providerId: over(255), userId: "x" } }, "identity.persist.providerId"],
  ];
  for (const [raw, field] of cases) {
    const out = validateHostIdentity(raw);
    assert.equal(out.ok, false, `${JSON.stringify(raw)?.slice(0, 60)} must be refused`);
    assert.equal(out.refusal.code, "identity-invalid");
    assert.equal(out.refusal.field, field, `${JSON.stringify(raw)?.slice(0, 60)} names the wrong field`);
    assert.ok(REFUSAL_CODES.includes(out.refusal.code), "the code is one the contract declares");
  }
  // The refusal never repeats the value: an identity is personal data.
  const secret = validateHostIdentity({ ...ADA, color: "not-a-colour-but-a-secret" });
  assert.ok(!JSON.stringify(secret).includes("secret"));
});

test("an identity at its bounds is accepted exactly as given", () => {
  const out = validateHostIdentity({
    id: "i".repeat(IDENTITY_BOUNDS.idBytes),
    name: "n".repeat(IDENTITY_BOUNDS.nameBytes),
    // Eight GRAPHEMES, though many more code units: a family emoji is one.
    initials: "👨‍👩‍👧ABCDEFG",
    color: "#3355C4",
    avatar: { url: "https://cdn.example/a.png" },
    persist: { providerId: "AD", userId: "S-1-5-21" },
  });
  assert.equal(out.ok, true, JSON.stringify(out.refusal));
  assert.equal(out.identity.name.length, IDENTITY_BOUNDS.nameBytes, "never truncated");
  assert.equal(out.identity.color, "#3355c4");
  assert.deepEqual(out.identity.persist, { providerId: "AD", userId: "S-1-5-21" });
  assert.ok(Object.isFrozen(out.identity));
  // The element's attribute is text; the same function reads it.
  assert.deepEqual(validateHostIdentity(JSON.stringify(ADA)).identity, ADA);
  assert.equal(validateHostIdentity({ ...ADA, avatar: { url: "data:image/png;base64,AAAA" } }).ok, true,
    "a small data: avatar fetches nothing and is allowed");
});

test("group is reserved: reported not-honoured, never refused and never kept", () => {
  const out = validateHostIdentity({ ...ADA, group: "Legal" });
  assert.equal(out.ok, true);
  assert.equal(out.identity.group, undefined);
  assert.deepEqual(out.notes.map((n) => [n.option, n.code]), [["identity.group", "not-honoured"]]);
});

// ---- Decided once ------------------------------------------------------------

test("an identity is accepted once, and never after a room was joined", () => {
  const holder = createIdentityHolder();
  const seen = [];
  holder.onChange((who) => seen.push(who.name));
  assert.equal(holder.identify({ id: "u" }).refusal.code, "identity-invalid", "a refused one does not use up the one call");
  assert.equal(holder.identity, null);
  assert.equal(holder.identify(ADA).ok, true);
  assert.equal(holder.identity.name, "Ada Lovelace");
  const again = holder.identify({ id: "u_other", name: "Mallory" });
  assert.equal(again.ok, false);
  assert.equal(again.refusal.code, "identity-fixed");
  assert.equal(holder.identity.name, "Ada Lovelace", "the second call changed nothing");
  assert.deepEqual(seen, ["Ada Lovelace"]);

  const late = createIdentityHolder();
  late.markJoined();
  assert.equal(late.identify(ADA).refusal.code, "identity-fixed", "a room is never re-joined as somebody else");
  assert.equal(late.identity, null);
});

test("a promised identity holds the startup until it lands, and a broken promise costs only a wait", async () => {
  assert.equal(identityPromised("?awaitIdentity=1&mode=edit"), true);
  assert.equal(identityPromised("?mode=edit"), false);
  const holder = createIdentityHolder();
  const waiting = holder.settled(60_000);
  holder.identify(ADA);
  assert.equal((await waiting).name, "Ada Lovelace");
  let fire;
  const never = createIdentityHolder({ setTimer: (fn) => { fire = fn; return 1; }, clearTimer() {} });
  const gave = never.settled(5);
  fire();
  assert.equal(await gave, null);
});

/** A host window and an editor window wired as each other's peer (same origin). */
function wire() {
  const ORIGIN = "https://docs.example";
  const make = () => {
    const win = {
      location: { origin: ORIGIN, search: "" },
      listeners: new Set(),
      sent: [],
      addEventListener(type, fn) { if (type === "message") win.listeners.add(fn); },
      removeEventListener(type, fn) { win.listeners.delete(fn); },
      postMessage(data, targetOrigin) {
        win.sent.push({ data, targetOrigin });
        if (targetOrigin !== ORIGIN) return;
        for (const fn of [...win.listeners]) fn({ data, origin: ORIGIN, source: win.peer });
      },
    };
    return win;
  };
  const host = make(), editor = make();
  host.peer = editor; editor.peer = host; editor.parent = host;
  return { host, editor, ORIGIN };
}

test("both transports answer identify alike: once, then identity-fixed, as a value and an event", async () => {
  for (const transport of ["in-process", "postMessage"]) {
    const holder = createIdentityHolder();
    const session = createHostSession({
      capabilities: resolveCapabilities({ mode: "edit", framed: true }),
      registry: () => [],
      identify: (raw) => holder.identify(raw),
    });
    const refusals = [];
    session.on("refusal", (e) => refusals.push(e.detail));
    let call = (raw) => session.identify(raw);
    if (transport === "postMessage") {
      const { host, editor, ORIGIN } = wire();
      attachHostBridge({ session, view: editor });
      const client = createHostClient({ frame: { contentWindow: editor }, editorOrigin: ORIGIN, view: host, timeout: 500 });
      call = (raw) => client.identify(raw);
    }
    assert.ok(PROTOCOL.requests.includes("identify"));
    const first = await call({ ...ADA, group: "Legal" });
    assert.equal(first.ok, true, transport);
    assert.equal(first.notes[0].code, "not-honoured");
    assert.equal(JSON.stringify(first).includes("Lovelace"), false, "the identity is never echoed back");
    const second = await call({ id: "u2", name: "Mallory" });
    assert.equal(second.ok, false, transport);
    assert.equal(second.refusal.code, "identity-fixed");
    assert.equal(holder.identity.name, "Ada Lovelace");
    assert.deepEqual(Object.keys(refusals.at(-1)).sort(), ["code", "command", "message", "requires"],
      "the refusal EVENT keeps the contract's declared shape");
    assert.equal(refusals.at(-1).code, "identity-fixed");
  }
});

// ---- When identity is given it is the only identity ----------------------------

function fakeInput(value = "") {
  const attributes = new Map();
  return {
    value, disabled: false,
    get title() { return attributes.get("title") ?? ""; },
    set title(v) { attributes.set("title", v); },
    setAttribute: (k, v) => attributes.set(k, String(v)),
    removeAttribute: (k) => attributes.delete(k),
    getAttribute: (k) => attributes.get(k) ?? null,
  };
}

test("with a host identity the Settings author fields show it, disabled, and say why", () => {
  const nameInput = fakeInput(), initialsInput = fakeInput();
  const reason = { id: "authorIdentityReason", hidden: true, textContent: "Your name is set by the application you opened this in" };
  const settings = { authorName: "Typed By Visitor", authorInitials: "TV" };

  reflectAuthorIdentity({ nameInput, initialsInput, reason, settings, identity: { ...ADA, initials: "AL" } });
  for (const input of [nameInput, initialsInput]) {
    assert.equal(input.disabled, true, "the visitor cannot overwrite a host identity");
    assert.equal(input.title, reason.textContent, "a disabled control says why (SKILL §10)");
    assert.equal(input.getAttribute("aria-describedby"), "authorIdentityReason");
  }
  assert.equal(reason.hidden, false);
  assert.equal(nameInput.value, "Ada Lovelace");
  assert.equal(initialsInput.value, "AL");

  // And without one, the fields are the visitor's again, with no stale reason.
  reflectAuthorIdentity({ nameInput, initialsInput, reason, settings, identity: null });
  assert.equal(nameInput.disabled, false);
  assert.equal(nameInput.value, "Typed By Visitor");
  assert.equal(nameInput.getAttribute("aria-describedby"), null, "a hidden description still referenced is still read");
  assert.equal(reason.hidden, true);
});

test("the active author is the host identity, which a Settings name cannot replace", () => {
  const settings = { authorName: "Typed By Visitor", authorInitials: "TV" };
  const who = activeAuthor(settings, { ...ADA, persist: { providerId: "AD", userId: "S-1" } });
  assert.deepEqual([who.name, who.id, who.fixed], ["Ada Lovelace", "u_8f2c", true]);
  assert.deepEqual(who.persist, { providerId: "AD", userId: "S-1" });
  const typed = activeAuthor(settings, null);
  assert.deepEqual([typed.name, typed.initials, typed.id, typed.persist], ["Typed By Visitor", "TV", undefined, null],
    "a typed name is never an id and never persisted");
  assert.equal(activeAuthor({ authorName: " " }, null).name, "You", "the owner's anonymous author (docs/175 §10.1)");
});

// ---- Joining a room -------------------------------------------------------------

test("a join carries the host's id and profile, or a claimed profile from Settings, or nothing", () => {
  const host = joinIdentity({ authorName: "Typed" },
    { ...ADA, initials: "AL", color: "#3355c4", avatar: { url: "https://cdn.example/a.png" }, persist: { providerId: "AD", userId: "S-1" } });
  assert.equal(host.identity, "u_8f2c");
  assert.deepEqual(JSON.parse(host.profile),
    { id: "u_8f2c", name: "Ada Lovelace", initials: "AL", color: "#3355c4", avatarUrl: "https://cdn.example/a.png" });
  assert.ok(!host.profile.includes("S-1"), "persist is file data and never goes on the wire (docs/175 §4.1)");
  const claimed = joinIdentity({ authorName: " Grace " }, null);
  assert.deepEqual([claimed.identity, JSON.parse(claimed.profile)], ["Grace", { id: "Grace", name: "Grace" }]);
  assert.deepEqual(joinIdentity({ authorName: "" }, null), { identity: "You", profile: undefined });
});

function fakeSocket() {
  const socket = { sent: [], closed: false, send(b) { socket.sent.push(b); }, close() { socket.closed = true; } };
  return socket;
}

function fakeEngine() {
  const engine = {
    joins: [],
    get collabState() { return JSON.stringify({ joined: engine.joins.length > 0, revision: 0 }); },
    collabJoinFrame(...args) { engine.joins.push(args); return new Uint8Array([1]); },
    collabReceiveFrame() { return "{}"; },
    collabNextChunk() { return undefined; },
    collabLeaveFrame() { return new Uint8Array([2]); },
  };
  return engine;
}

test("the grant is asked for on every join, held only for the call, and the join waits for it", async () => {
  const session = fakeEngine();
  const sockets = [];
  const timers = [];
  const states = [];
  let asked = 0;
  const pipe = collabTransport({
    session, url: "wss://relay.example/r", identity: "u_8f2c", resumeKey: "k",
    profile: JSON.stringify({ id: "u_8f2c", name: "Ada" }),
    credentials: async () => ({ grant: `jwt-${(asked += 1)}` }),
    open: () => { const s = fakeSocket(); sockets.push(s); return s; },
    setTimer: (fn) => { timers.push(fn); return timers.length; }, clearTimer() {}, random: () => 0,
    onState: (s) => states.push(s),
  });
  pipe.start();
  sockets[0].onopen();
  assert.equal(session.joins.length, 0, "no join before the grant: a granted relay would refuse it");
  assert.equal(pipe.state().name, "reconnecting", "nothing is shared while the join waits");
  assert.equal(pipe.awareness(new Uint8Array([9])), false, "and nothing else is written ahead of the join");
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(session.joins[0], ["u_8f2c", "k", 0, "jwt-1", JSON.stringify({ id: "u_8f2c", name: "Ada" })]);
  assert.equal(pipe.state().name, "connected");
  // A reconnect asks again: the relay re-verifies every join (docs/152 §10 Q4).
  sockets[0].onclose();
  timers.shift()();
  sockets[1].onopen();
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(session.joins[1][3], "jwt-2", "a fresh grant per join, never a remembered one");
  assert.equal(asked, 2);
  assert.ok(!JSON.stringify([...states, pipe.state()]).includes("jwt"), "the grant is in no state");
});

test("a provider that cannot produce a grant is a dropped connection, retried, never a grantless join", async () => {
  const session = fakeEngine();
  const sockets = [];
  const timers = [];
  const pipe = collabTransport({
    session, url: "wss://relay.example/r", identity: "u", resumeKey: "k",
    credentials: () => Promise.reject(new Error("token service down")),
    open: () => { const s = fakeSocket(); sockets.push(s); return s; },
    setTimer: (fn) => { timers.push(fn); return timers.length; }, clearTimer() {}, random: () => 0,
  });
  pipe.start();
  sockets[0].onopen();
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(session.joins.length, 0);
  assert.equal(sockets[0].closed, true);
  assert.equal(pipe.state().attempt, 1, "counted once, not twice");
  assert.equal(timers.length, 1, "a retry is scheduled");
  assert.ok(!pipe.state().problem.includes("token service"), "the provider's error is not carried in the state");
});

test("with no credentials provider the join is written at once, as before", () => {
  const session = fakeEngine();
  const sockets = [];
  const pipe = collabTransport({
    session, url: "wss://relay.example/r", identity: "You", resumeKey: "k",
    open: () => { const s = fakeSocket(); sockets.push(s); return s; },
  });
  pipe.start();
  sockets[0].onopen();
  assert.deepEqual(session.joins[0], ["You", "k", 0, undefined, undefined]);
});

// ---- The strip and the caret read profile rows through one seam ----------------

test("participant joins and leaves repaint the room like caret moves do", () => {
  assert.ok(PRESENCE_OUTCOMES.includes("participantJoined"));
  assert.ok(PRESENCE_OUTCOMES.includes("participantLeft"));
});

test("no avatar URL without the mount's consent, and nothing is fetched to decide", () => {
  const realFetch = globalThis.fetch;
  const fetched = [];
  globalThis.fetch = (...args) => { fetched.push(args); return Promise.reject(new Error("no")); };
  try {
    const grace = { client: 3, attested: true, profile: { id: "g", name: "Grace", avatarUrl: "https://cdn.example/g.png" } };
    assert.equal(participantDisplay(grace, { t }).avatarUrl, null);
    assert.equal(displayRoster([grace], { t })[0].avatarUrl, null, "the default policy is never");
    assert.equal(displayRoster([grace], { t, avatars: "never" })[0].avatarUrl, null);
    assert.equal(displayRoster([grace], { t, avatars: "fetch" })[0].avatarUrl, "https://cdn.example/g.png");
    assert.equal(fetched.length, 0, "deciding what to draw fetches nothing");
  } finally {
    globalThis.fetch = realFetch;
  }
  assert.equal(avatarPolicy("?avatars=fetch"), "fetch");
  assert.equal(avatarPolicy("?avatars=yes"), "never");
  assert.equal(avatarPolicy(""), "never");
});

test("a duplicate name is suffixed on the view and never on the profile it was handed", () => {
  const first = { id: "a", name: "Sam" }, second = { id: "b", name: "Sam" };
  const views = displayRoster([
    { client: 1, attested: true, profile: first },
    { client: 2, attested: true, profile: second },
  ], { t });
  assert.deepEqual(views.map((v) => v.label), ["Sam", "Sam (2)"]);
  assert.equal(second.name, "Sam", "the suffix is display-only: the stored author keeps the name as given");
  assert.equal(first.name, "Sam");
});

/** A roster and a document as the engine reports them, and an overlay `place`. */
function room({ participants, me, remotes }) {
  const roster = {
    participants: JSON.stringify(participants),
    me: JSON.stringify(me),
    receiveFrame() {}, clear() {}, free() {},
    remoteSelections: () => JSON.stringify(remotes),
  };
  const doc = { collabState: JSON.stringify({ participant: me?.client ?? null }) };
  const placed = [];
  const element = () => {
    const el = {
      style: {}, dataset: {}, children: [], classList: { set: new Set(), add(c) { this.set.add(c); } },
      title: "",
      ownerDocument: { createElement: () => element() },
      append(child) { el.children.push(child); },
    };
    return el;
  };
  const place = (_rect, kind) => { const el = element(); el.kind = kind; placed.push(el); return el; };
  return { roster, doc, placed, place };
}

test("the caret label names the person from the roster, and marks a claimed name unverified", () => {
  const { roster, doc, placed, place } = room({
    me: { client: 0, attested: true, profile: { id: "u0", name: "Ada" } },
    participants: [
      { client: 0, attested: true, profile: { id: "u0", name: "Ada" } },
      { client: 1, attested: false, profile: { id: "Ada", name: "Ada" } },
      { client: 2, attested: true, profile: { id: "u2", name: "Grace" } },
      { client: 3, attested: false },
    ],
    remotes: [
      { client: 1, caret: [0, 0, 1, 10], selection: [] },
      { client: 2, caret: [0, 0, 1, 10], selection: [] },
      { client: 3, caret: [0, 0, 1, 10], selection: [] },
    ],
  });
  const reports = [];
  const presence = createPresence({ newRoster: () => roster, document: () => doc, transport: () => null,
    selection: () => null, paint() {}, t, roster: (r) => reports.push(r) });
  presence.open(true);
  assert.deepEqual(presence.participants().map((p) => p.client), [1, 2, 3], "everyone present, this tab excluded");
  assert.equal(reports.at(-1).self.profile.name, "Ada", "the self entry carries roster.me's profile");
  presence.paint(place);
  const labels = placed.filter((el) => el.kind === "remote-caret").map((el) => el.children[0]);
  assert.deepEqual(labels.map((l) => l.textContent), ["Ada (2)", "Grace", "Participant 4"],
    "names on carets, the same (2) the strip draws, and the honest number where nobody said");
  assert.match(labels[0].title, /unverified/, "a claimed name is marked on the caret tooltip");
  assert.ok(labels[0].classList.set.has("is-unverified"));
  assert.equal(labels[1].title, "", "an attested name carries no mark");
  assert.equal(labels[2].title, "", "a number is not a claim");
});

// ---- The element never puts the identity in a URL --------------------------------

/** The smallest DOM the element touches, installed before it is imported (its
 *  base class is chosen at module evaluation). */
function installDom() {
  class FakeElement {
    constructor() { this.attributes = new Map(); this.isConnected = false; this.events = []; }
    getAttribute(k) { return this.attributes.has(k) ? this.attributes.get(k) : null; }
    hasAttribute(k) { return this.attributes.has(k); }
    setAttribute(k, v) { this.attributes.set(k, String(v)); }
    removeAttribute(k) { this.attributes.delete(k); }
    attachShadow() { return { replaceChildren() {} }; }
    dispatchEvent(e) { this.events.push(e); return true; }
  }
  const frames = [];
  globalThis.HTMLElement = FakeElement;
  globalThis.CustomEvent = class { constructor(type, init) { this.type = type; this.detail = init?.detail; } };
  globalThis.window = { addEventListener() {}, removeEventListener() {} };
  globalThis.document = {
    baseURI: "https://host.example/app/",
    createElement(tag) {
      if (tag !== "iframe") return { textContent: "" };
      const sent = [];
      const frame = {
        attributes: new Map(), listeners: new Map(), src: "",
        setAttribute(k, v) { frame.attributes.set(k, v); },
        addEventListener(type, fn) { frame.listeners.set(type, fn); },
        remove() {},
        contentWindow: { postMessage: (data, origin) => sent.push({ data, origin }) },
        sent,
      };
      frames.push(frame);
      return frame;
    },
  };
  return frames;
}

test("the element hands the identity over by message to an explicit origin, never in the frame URL", async () => {
  const frames = installDom();
  const warn = console.warn;
  console.warn = () => {};
  try {
    const { OpenDocEditorElement } = await import("../src/embed_element.mjs");
    const mount = (setup) => {
      const element = new OpenDocEditorElement();
      setup(element);
      element.isConnected = true;
      element.setAttribute("frame-title", "Editor");
      element.connectedCallback();
      return { element, frame: frames.at(-1), detail: element.events.at(-1).detail };
    };

    const attr = mount((el) => {
      el.setAttribute("identity", JSON.stringify({ ...ADA, group: "Legal" }));
      el.setAttribute("prefs", JSON.stringify({ user: "Typed", initials: "TY", spellcheck: false }));
      el.setAttribute("avatars", "fetch");
    });
    const src = decodeURIComponent(attr.frame.src);
    for (const secret of ["u_8f2c", "Ada", "Lovelace", "Legal", "Typed"]) {
      assert.ok(!src.includes(secret), `the frame URL must not carry "${secret}": ${src}`);
    }
    const url = new URL(attr.frame.src);
    assert.equal(url.searchParams.get("awaitIdentity"), "1", "the URL says only that one is coming");
    assert.equal(url.searchParams.get("avatars"), "fetch");
    assert.deepEqual(JSON.parse(url.searchParams.get("prefs")), { spellcheck: false }, "prefs.user and prefs.initials are superseded");
    const codes = attr.detail.notes.map((n) => `${n.option}:${n.code}`);
    assert.ok(codes.includes("prefs.user:superseded-by-identity"));
    assert.ok(codes.includes("prefs.initials:superseded-by-identity"));
    assert.ok(codes.includes("identity.group:not-honoured"));
    assert.equal(attr.element.identity.name, "Ada Lovelace");

    assert.equal(attr.frame.sent.length, 0, "nothing is posted before the frame's scripts have run");
    attr.frame.listeners.get("load")();
    assert.equal(attr.frame.sent.length, 1);
    const [{ data, origin }] = attr.frame.sent;
    assert.equal(origin, "https://host.example", "an explicit target origin, never \"*\"");
    assert.equal(data.type, "identify");
    assert.deepEqual(data.identity, ADA, "the validated identity, without the reserved group");

    // The property, for a host that would rather not put a name in the DOM, wins.
    const prop = mount((el) => {
      el.setAttribute("identity", JSON.stringify({ id: "attr", name: "Attribute" }));
      el.identity = { id: "prop", name: "Property" };
    });
    assert.equal(prop.element.identity.name, "Property");
    assert.ok(!decodeURIComponent(prop.frame.src).includes("Property"));

    // ONLYOFFICE's user arrives the same way, and never as prefs.user.
    const config = mount((el) => el.setAttribute("config", JSON.stringify({ editorConfig: { user: { id: "oo_1", name: "Grace" } } })));
    assert.equal(config.element.identity.name, "Grace");
    assert.ok(!decodeURIComponent(config.frame.src).includes("Grace"));

    // An invalid identity is not used at all, and the refusal names the field.
    const bad = mount((el) => el.setAttribute("identity", JSON.stringify({ id: "u", name: "x".repeat(256) })));
    assert.equal(bad.element.identity, null);
    assert.equal(new URL(bad.frame.src).searchParams.get("awaitIdentity"), null);
    assert.ok(bad.detail.notes.some((n) => n.option === "identity.name" && n.code === "identity-invalid"));
    assert.equal(bad.frame.listeners.has("load"), false, "nothing to hand over");
  } finally {
    console.warn = warn;
  }
});
