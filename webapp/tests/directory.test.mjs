// The @mention directory seam (`docs/175` §7, `docs/109` HF-361): what the editor
// asks, what it accepts back, and when it tells the host who was mentioned.
// The picker in a real composer, through a real host, is
// `tests/e2e/mention-directory.spec.mjs`.
import test from "node:test";
import assert from "node:assert/strict";
import {
  DIRECTORY_LIMIT,
  checkAnswer,
  checkEntry,
  createDirectoryChannel,
  excerptOf,
  mentionQuery,
  placeMentions,
} from "../src/directory.mjs";
import { createMentionNotices } from "../src/mention_notices.mjs";
import { createHostSession } from "../src/host_session.mjs";
import { HOST_EVENT_NAMES, PROTOCOL } from "../src/host_contract.mjs";

test("a bare @ asks nothing; one character after it is a query", () => {
  assert.equal(mentionQuery("Hello @", 7), null, "a bare @ must never ask for everyone");
  assert.deepEqual(mentionQuery("Hello @G", 8), { start: 6, query: "G" });
  assert.deepEqual(mentionQuery("@Gra", 4), { start: 0, query: "Gra" });
  assert.equal(mentionQuery("mail@example", 12), null, "an address is not a mention");
  assert.equal(mentionQuery("Hello @Grace Hopper", 19), null, "a space ends the query");
  assert.deepEqual(mentionQuery("Café @Gr", 8), { start: 5, query: "Gr" }, "offsets are UTF-16 units");
});

test("picked mentions are found by their label where they now sit, never overlapping", () => {
  const picked = [
    { id: "u_g", name: "Grace Hopper", link: "mailto:g@example.org" },
    { id: "u_g", name: "Grace Hopper", link: "mailto:g@example.org" },
    { id: "u_x", name: "Gone Person", link: null },
  ];
  const text = "Ask @Grace Hopper and again @Grace Hopper.";
  assert.deepEqual(placeMentions(text, picked), [
    { start: 4, end: 17, link: "mailto:g@example.org", id: "u_g" },
    { start: 28, end: 41, link: "mailto:g@example.org", id: "u_g" },
  ], "a deleted mention is simply not a mention");
});

test("an answer is bounded: over-limit and out-of-bounds entries are dropped AND counted", () => {
  const many = Array.from({ length: DIRECTORY_LIMIT + 5 }, (_, i) => ({ id: `u${i}`, name: `P ${i}` }));
  const answer = checkAnswer({ people: many, more: false });
  assert.equal(answer.people.length, DIRECTORY_LIMIT);
  assert.equal(answer.dropped, 5, "trimming is reported, never silent");
  assert.equal(answer.more, true);
  for (const bad of [
    { id: "", name: "No id" },
    { id: "x".repeat(129), name: "Long id" },
    { id: "u", name: "" },
    { id: "u", name: "Ada", link: "javascript:alert(1)" },
    { id: "u", name: "Ada", link: "http://example.org" },
    { id: "u", name: "Ada", avatar: { url: "http://cdn.example/a.png" } },
    { id: "u", name: "Ada", initials: "ABCDEFGHI" },
  ]) assert.equal(checkEntry(bad), null, JSON.stringify(bad));
  assert.deepEqual(checkEntry({ id: "u", name: "Ada", link: "mailto:a@example.org", email: "dropped" }),
    { id: "u", name: "Ada", link: "mailto:a@example.org" }, "unknown fields are not carried");
});

test("an excerpt is at most 280 characters of the comment", () => {
  assert.equal(excerptOf("short"), "short");
  assert.equal([...excerptOf("x".repeat(400))].length, 280);
});

test("with no directory nothing is offered and nothing is sent", async () => {
  const sent = [];
  const channel = createDirectoryChannel({ emit: (name, detail) => sent.push([name, detail]) });
  assert.equal(channel.available(), false);
  assert.deepEqual((await channel.search("Gr")).people, []);
  assert.equal(await channel.notify({ people: ["u"] }), null);
  assert.deepEqual(sent, [], "no event, no question, no message");
});

test("an in-process directory is asked the query and nothing else, with a bounded limit", async () => {
  const asked = [];
  const channel = createDirectoryChannel({ emit: () => assert.fail("in-process needs no event") });
  channel.directory = {
    search: (query, options) => { asked.push([query, options.limit, Object.keys(options).sort()]); return { people: [{ id: "u_g", name: "Grace" }] }; },
    notify: () => ({ status: "needs-access" }),
  };
  assert.equal(channel.available(), true);
  const answer = await channel.search("Gr", { limit: 500 });
  assert.deepEqual(asked, [["Gr", DIRECTORY_LIMIT, ["limit", "signal"]]]);
  assert.equal(answer.people[0].name, "Grace");
  assert.equal(await channel.notify({ people: ["u_g"] }), "needs-access");
});

test("over postMessage a query is an event answered by `directory.answer`, by qid", async () => {
  const events = [];
  const channel = createDirectoryChannel({ emit: (name, detail) => events.push({ name, detail }), remote: true });
  assert.equal(channel.available(), true);
  const pending = channel.search("Ad");
  assert.equal(events.length, 1);
  assert.equal(events[0].name, "directory-query");
  assert.deepEqual(Object.keys(events[0].detail).sort(), ["limit", "qid", "query"], "the query and the limit, nothing about the document");
  assert.equal(channel.answerDirectory({ qid: "nope", people: [] }), false, "an unknown qid answers nothing");
  assert.equal(channel.answerDirectory({ qid: events[0].detail.qid, people: [{ id: "u_a", name: "Ada" }], more: false }), true);
  assert.equal((await pending).people[0].name, "Ada");
});

test("a newer keystroke aborts the question before it", async () => {
  const channel = createDirectoryChannel({ emit: () => {}, remote: true });
  const controller = new AbortController();
  const first = channel.search("A", { signal: controller.signal });
  controller.abort();
  await assert.rejects(first, { name: "AbortError" });
});

test("the session serves both answer verbs and declares both events, as the contract does", () => {
  for (const verb of ["directory.answer", "mention.answer"]) assert.ok(PROTOCOL.requests.includes(verb), verb);
  for (const event of ["directory-query", "mention"]) assert.ok(HOST_EVENT_NAMES.includes(event), event);
  const session = createHostSession({ capabilities: new Set(), registry: () => [], directoryRemote: true });
  const heard = [];
  session.on("directory-query", ({ detail }) => heard.push(detail));
  const pending = session.mentions.search("Gr");
  assert.equal(heard.length, 1);
  assert.deepEqual(session["directory.answer"]({ qid: heard[0].qid, people: [{ id: "u", name: "Grace" }] }), { answered: true });
  return pending.then((answer) => assert.equal(answer.people[0].name, "Grace"));
});

test("a mention waits until its comment is ordered, and the host's answer is said", async () => {
  const notified = [];
  const said = [];
  const notices = createMentionNotices({
    mentions: () => ({ notify: async (notice) => { notified.push(notice); return notice.people[0] === "u_x" ? "needs-access" : "notified"; } }),
    status: (text, kind) => said.push([text, kind ?? ""]),
    t: (key, params) => `${key}:${params.names}`,
  });
  notices.queue({ scope: "room", people: [{ id: "u_g", name: "Grace" }], commentId: "c1", excerpt: "hi", room: "r" });
  notices.queue({ scope: "local", people: [{ id: "u_x", name: "Xan" }], commentId: "c2", excerpt: "yo" });
  assert.equal(notified.length, 0, "nothing is sent before the comment is ordered");
  await notices.ordered("local");
  assert.deepEqual(notified.map((n) => n.commentId), ["c2"], "only the ordered scope goes");
  assert.deepEqual(said, [["mention.needsAccess:Xan", "warn"]]);
  await notices.ordered("room");
  assert.deepEqual(notified[1], { people: ["u_g"], commentId: "c1", excerpt: "hi", room: "r" }, "ids travel, names do not");
  assert.equal(notices.pending(), 0);
});
