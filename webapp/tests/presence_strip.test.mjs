// The avatar strip's seam and its rules (`docs/109` HF-330): what a participant
// looks like, how the room is ordered and folded, and when joins are said aloud.
// The DOM half — placement at the header's right end, keyboard, the jump to a
// caret — is `tests/e2e/presence-strip.spec.mjs`, in a real two-person room.
import test from "node:test";
import assert from "node:assert/strict";
import { EN_STRINGS } from "../src/en_strings.mjs";
import { chipLabel, contrast, createJoinAnnouncer, displayRoster, participantColor,
  participantDisplay, planStrip } from "../src/presence_strip.mjs";
import { createPresence } from "../src/collab_presence.mjs";

/** English, with `{param}` filled and `.one`/`.other` chosen, like the real seam. */
function t(key, params = {}) {
  const plural = EN_STRINGS[`${key}.${params.count === 1 ? "one" : "other"}`];
  const text = EN_STRINGS[key] ?? plural;
  assert.ok(text, `no English for ${key}`);
  return text.replace(/\{(\w+)\}/g, (_, name) => String(params[name]));
}

test("with no profile a participant is the relay's number, and says so", () => {
  const view = participantDisplay({ client: 1 }, { t });
  assert.equal(view.label, "Participant 2");
  assert.equal(view.initials, "P2");
  assert.equal(view.numbered, true);
  assert.equal(view.verified, false);
  assert.equal(view.avatarUrl, null);
  assert.match(chipLabel(view, t), /^Participant 2 — numbered by the room, because this host does not share names\. Go to their cursor$/);
  const mine = participantDisplay({ client: 0, isSelf: true }, { t });
  assert.equal(chipLabel(mine, t), "You (Participant 1 — numbered by the room, because this host does not share names)");
  assert.equal(chipLabel(participantDisplay({ client: null, isSelf: true }, { t }), t), "You");
});

test("a claimed name is shown, marked unverified, with the palette's colour and never a picture", () => {
  const view = participantDisplay({ client: 2, attested: false,
    profile: { id: "u", name: "Ada Lovelace", color: "#ff0000", avatarUrl: "https://x.example/a.png" } }, { t, avatars: "fetch" });
  assert.equal(view.label, "Ada Lovelace");
  assert.equal(view.initials, "AL");
  assert.equal(view.verified, false);
  assert.equal(view.numbered, false);
  assert.equal(view.avatarUrl, null, "a claimed identity never shows an avatar");
  assert.equal(view.color, participantColor(2).color, "a claimed colour could be anyone's");
  assert.match(chipLabel(view, t), /^Ada Lovelace — unverified: .*\. Go to their cursor$/);
});

test("an attested profile brings its colour, and its picture only with the mount's consent", () => {
  const grace = { client: 3, attested: true,
    profile: { id: "u", name: "Grace", initials: "GH", color: "#FFE600", avatarUrl: "https://x.example/g.png" } };
  assert.equal(participantDisplay(grace, { t }).avatarUrl, null, "no fetch without avatars: \"fetch\" (docs/175 §8.1)");
  const tiny = "data:image/png;base64,AAAA";
  assert.equal(participantDisplay({ ...grace, profile: { ...grace.profile, avatarUrl: tiny } }, { t }).avatarUrl, tiny,
    "a small data: URL fetches nothing and needs no consent");
  const view = participantDisplay(grace, { t, avatars: "fetch" });
  assert.equal(view.verified, true);
  assert.equal(view.initials, "GH");
  assert.equal(view.avatarUrl, "https://x.example/g.png");
  assert.equal(view.color, "#ffe600");
  assert.equal(view.ink, "#000000", "white on a yellow is unreadable");
  assert.equal(chipLabel(view, t), "Grace. Go to their cursor");
  // A colour this file cannot measure is not used.
  assert.equal(participantDisplay({ client: 3, attested: true, profile: { name: "G", color: "red" } }, { t }).color,
    participantColor(3).color);
});

test("two people with one name are told apart by a display-only suffix, in number order", () => {
  const views = displayRoster([
    { client: 5, attested: true, profile: { name: "Sam" } },
    { client: 1, attested: true, profile: { name: "Sam" } },
    { client: 2 }, { client: 3 },
    { client: 4, profile: { name: "sam" } },
  ], { t });
  assert.deepEqual(views.map((v) => v.label), ["Sam (3)", "Sam", "Participant 3", "Participant 4", "sam (2)"]);
});

test("every palette colour carries initials at AA or better", () => {
  for (let client = 0; client < 64; client += 1) {
    const { luminance, ink } = participantColor(client);
    const ratio = contrast(luminance, ink === "#ffffff" ? 1 : 0);
    assert.ok(ratio >= 4.5, `participant ${client}: ${ratio.toFixed(2)}:1`);
  }
});

test("the strip draws you first, then the room in number order, four chips and a count", () => {
  const plan = planStrip({ self: 2, others: [7, 0, 2, 5, 0, 9] });
  assert.deepEqual(plan.people.map((p) => [p.client, p.isSelf]), [[2, true], [0, false], [5, false], [7, false], [9, false]]);
  assert.deepEqual(plan.chips.map((p) => p.client), [2, 0, 5, 7]);
  assert.deepEqual(plan.overflow.map((p) => p.client), [9]);
  assert.equal(planStrip({ self: 0, others: [1, 2, 3] }).overflow.length, 0);
  // A seam input passes straight through, profile and all.
  const profile = { name: "Ada" };
  assert.equal(planStrip({ self: 0, others: [{ client: 4, profile, attested: true }] }).people[1].profile, profile);
});

function clock() {
  const timers = new Map(); let id = 0;
  return {
    setTimer: (fn, ms) => { timers.set(++id, { fn, ms }); return id; },
    clearTimer: (n) => timers.delete(n),
    fire(ms) { for (const [n, timer] of [...timers]) if (timer.ms === ms) { timers.delete(n); timer.fn(); } },
    pending: () => timers.size,
  };
}

test("joins and leaves are batched, rate-limited, and the room already there is not news", () => {
  const time = clock(), said = [];
  const announcer = createJoinAnnouncer({ ...time, settleMs: 10, windowMs: 20,
    name: (c) => `P${c + 1}`, join: (items) => items.join(" and "), announce: (s) => said.push(s) });
  announcer.open();
  announcer.update([1, 2]);
  time.fire(10); // settled
  assert.deepEqual(said, [], "people already in the room are not announced");
  announcer.update([1, 2, 3]);
  announcer.update([1, 2, 3, 4]);
  announcer.update([1, 3, 4]);
  announcer.update([1, 4]); // 3 joined and left inside the window: nothing to say
  assert.deepEqual(said, [], "nothing is said before the window closes");
  time.fire(20);
  assert.deepEqual(said, [[["joined", "P5"], ["left", "P3"]]]);
  announcer.update([1, 4]);
  assert.equal(time.pending(), 0, "an unchanged room schedules nothing");
  announcer.update([4]); // 1 drops and comes straight back: a blip, not news
  announcer.update([1, 4]);
  time.fire(20);
  assert.equal(said.length, 1, "a leave and a return inside one window cancel out");
  announcer.open(); // a reconnect: quiet again
  announcer.update([1, 4, 6]);
  time.fire(20);
  assert.equal(said.length, 1);
});

test("presence reports the roster's members, without this tab, to the strip", () => {
  const reports = [];
  const roster = { participants: JSON.stringify([{ client: 0 }, { client: 3 }, { client: 1 }]), receiveFrame() {},
    remoteSelections: () => "[]", clear() {}, free() {} };
  const doc = { collabState: JSON.stringify({ participant: 1 }) };
  const presence = createPresence({ newRoster: () => roster, document: () => doc, transport: () => null,
    selection: () => null, paint() {}, t, roster: (room) => reports.push(room) });
  presence.open(true);
  presence.frame(new Uint8Array([1]));
  assert.deepEqual(reports.at(-1), { self: 1, others: [0, 3], reconnected: false });
  presence.open(false);
  assert.equal(reports.at(-1), null, "leaving the room withdraws the strip");
});
