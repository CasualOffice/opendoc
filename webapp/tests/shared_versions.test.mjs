// Shared version history in a room — `shared_versions.mjs` (`docs/175` §11, HF-362).
//
// Four guarantees, each driven RED by a mutation of the module before it was trusted (the
// mutations and their output are in the commit message):
//
//   1. the client never states a version's author: `saveVersion` is called with exactly
//      `room`, `revision`, `name` and `bytes`;
//   2. paging is bounded: a page never exceeds `SHARED_PAGE_MAX`, an over-full host answer
//      is cut, and the pager stops at `SHARED_LOADED_MAX`;
//   3. shared and device rows are never mixed: sections partition by ORIGIN;
//   4. the headings say which history this is: none standalone, "Shared versions" and
//      "On this device" in a room whose host lists, "Versions on this device only" in one
//      whose host does not.
import assert from "node:assert/strict";
import test from "node:test";

import { EN_STRINGS } from "../src/en_strings.mjs";
import { resetI18n, setCatalogue, setLocale } from "../src/i18n.mjs";
import {
  HISTORY_SCOPE,
  ORIGIN,
  SHARED_LOADED_MAX,
  SHARED_PAGE,
  SHARED_PAGE_MAX,
  createSharedPager,
  historyScope,
  sharedRow,
  sharedVersionSource,
  versionSections,
} from "../src/shared_versions.mjs";

resetI18n();
setCatalogue("en", EN_STRINGS);
setLocale("en");

const ROOM = "ws://127.0.0.1:1/rooms/r1";

/** A host row as `listVersions` returns one. */
const hostRow = (i, overrides = {}) => ({
  id: `v${i}`,
  revision: i,
  createdAt: Date.UTC(2026, 9, 24, 12, 0, i),
  author: { id: "u_bob", name: "Bob Stone" },
  kind: "manual",
  ...overrides,
});

/** A provider over `count` versions, newest first, recording every call. */
function fakeProvider(count, { overfill = 0 } = {}) {
  const all = Array.from({ length: count }, (_, i) => hostRow(count - i));
  const calls = [];
  return {
    calls,
    async listVersions(args) {
      calls.push(["listVersions", args]);
      const start = args.before ? all.findIndex((row) => row.id === args.before) + 1 : 0;
      const page = all.slice(start, start + args.limit + overfill);
      return { versions: page, more: start + args.limit < all.length };
    },
    async saveVersion(args) {
      calls.push(["saveVersion", args]);
      return { id: "saved1" };
    },
    async versionBytes(args) {
      calls.push(["versionBytes", args]);
      return { bytes: new Uint8Array([123, 125]), revision: 3 };
    },
  };
}

// ── 1. No author from the client ─────────────────────────────────────────────

test("saving a shared version sends no author: exactly room, revision, name and bytes", async () => {
  // Catches: the adapter adding `author` (or anything else) to the save — a version
  // author the client can state is one it can forge (`docs/175` §11).
  const provider = fakeProvider(0);
  const source = sharedVersionSource(provider, ROOM);
  const bytes = new Uint8Array([1, 2, 3]);
  const saved = await source.save({ revision: 7, name: "Draft", bytes, author: { id: "u_evil", name: "Forged" } });
  assert.deepEqual(saved, { id: "saved1" });
  const [[method, args]] = provider.calls;
  assert.equal(method, "saveVersion");
  assert.deepEqual(Object.keys(args).sort(), ["bytes", "name", "revision", "room"]);
  assert.equal(args.room, ROOM);
  assert.equal(args.revision, 7);
  assert.equal(args.name, "Draft");
  assert.equal(args.bytes, bytes);
  assert.ok(!("author" in args));
});

test("a host answer to save that is not an id is refused, not trusted", async () => {
  const source = sharedVersionSource({ listVersions() {}, saveVersion: async () => ({}) }, ROOM);
  await assert.rejects(source.save({ revision: 1, name: "", bytes: new Uint8Array([1]) }),
    /could not be read/);
});

// ── 2. Bounded paging ────────────────────────────────────────────────────────

test("a page asks for at most SHARED_PAGE_MAX, whatever the caller asks", async () => {
  // Catches: dropping the clamp, which would let one call ask the host for everything.
  const provider = fakeProvider(500);
  const source = sharedVersionSource(provider, ROOM);
  const page = await source.list({ limit: 10_000 });
  assert.equal(provider.calls[0][1].limit, SHARED_PAGE_MAX);
  assert.equal(page.rows.length, SHARED_PAGE_MAX);
  await source.list({});
  assert.equal(provider.calls[1][1].limit, SHARED_PAGE);
  await source.list({ limit: 0 });
  assert.equal(provider.calls[2][1].limit, SHARED_PAGE, "a nonsense limit falls back to the default page");
});

test("a host that over-fills a page is cut to the page, and more is reported", async () => {
  // Catches: trusting the host's page length — the bound is the editor's, not the host's courtesy.
  const provider = fakeProvider(200, { overfill: 40 });
  const page = await sharedVersionSource(provider, ROOM).list({ limit: 10 });
  assert.equal(page.rows.length, 10);
  assert.equal(page.more, true);
});

test("the pager reads one page at a time, older pages only on request, and stops at its bound", async () => {
  // Catches: a pager that fetches everything on open, or grows without limit.
  const provider = fakeProvider(SHARED_LOADED_MAX + 100);
  const pager = createSharedPager(sharedVersionSource(provider, ROOM), { pageSize: SHARED_PAGE_MAX });
  await pager.first();
  assert.equal(provider.calls.length, 1, "opening reads one page");
  assert.equal(pager.rows.length, SHARED_PAGE_MAX);
  assert.equal(pager.more, true);
  await pager.older();
  assert.equal(provider.calls[1][1].before, pager.rows[SHARED_PAGE_MAX - 1].sharedId, "`before` is the oldest row held");
  while (!pager.full) await pager.older();
  assert.equal(pager.rows.length, SHARED_LOADED_MAX);
  const reads = provider.calls.length;
  assert.equal(await pager.older(), false, "a full pager reads nothing more");
  assert.equal(provider.calls.length, reads);
  assert.equal(new Set(pager.rows.map((row) => row.versionId)).size, pager.rows.length, "no row twice");
});

test("a superseded read is dropped and its signal aborted", async () => {
  // Catches: a cancelled page painting over a newer one (closing the panel, leaving the room).
  let release;
  const seen = [];
  const provider = {
    listVersions: (args) => {
      seen.push(args.signal);
      return new Promise((done) => { release = () => done({ versions: [hostRow(1)], more: false }); });
    },
  };
  const pager = createSharedPager(sharedVersionSource(provider, ROOM));
  const first = pager.first();
  pager.cancel();
  assert.equal(seen[0].aborted, true);
  release();
  assert.equal(await first, false);
  assert.equal(pager.rows.length, 0);
});

test("a provider failure is kept as the pager's error, never an empty success", async () => {
  // Catches: swallowing a host error into an empty list (the silent failure §11 forbids).
  const provider = { listVersions: async () => { throw new Error("host is down"); } };
  const pager = createSharedPager(sharedVersionSource(provider, ROOM));
  assert.equal(await pager.first(), true);
  assert.equal(pager.error, "host is down");
  const malformed = createSharedPager(sharedVersionSource({ listVersions: async () => ({ versions: [{ id: "x" }] }) }, ROOM));
  await malformed.first();
  assert.match(malformed.error, /could not be read/);
});

test("a host row with no attested author is not a shared version", () => {
  assert.equal(sharedRow(hostRow(1, { author: null })), null);
  assert.equal(sharedRow(hostRow(1, { author: { id: "u", name: "  " } })), null);
  assert.equal(sharedRow(hostRow(1, { id: "has spaces" })), null);
  const row = sharedRow(hostRow(4, { name: "Draft‮" }));
  assert.equal(row.origin, ORIGIN.SHARED);
  assert.equal(row.actor, "Bob Stone");
  assert.equal(row.name, "Draft", "a bidi override does not survive into a row");
  assert.equal(row.versionId, "shared:v4");
});

// ── 3 and 4. Sections and headings ───────────────────────────────────────────

const group = (rows) => (rows.length ? [{ key: "day", label: "Today", rows }] : []);
const deviceRow = (id) => ({ versionId: id, createdAt: 1, kind: "auto" });

test("in a room whose host lists, shared and device rows land in separate sections, headed", () => {
  // Catches: sectioning by the list a row arrived in (or merging the two), which would
  // let a device row read as the room's or the room's as this browser's.
  const shared = [sharedRow(hostRow(1)), sharedRow(hostRow(2))];
  const device = [deviceRow("ver_a"), deviceRow("ver_b")];
  // Handed in deliberately interleaved.
  const sections = versionSections({ scope: HISTORY_SCOPE.SHARED, rows: [device[0], shared[0], device[1], shared[1]], group });
  assert.deepEqual(sections.map((s) => [s.origin, s.heading]), [
    [ORIGIN.SHARED, "Shared versions"],
    [ORIGIN.DEVICE, "On this device"],
  ]);
  const ids = (section) => section.groups.flatMap((g) => g.rows.map((r) => r.versionId));
  assert.deepEqual(ids(sections[0]).sort(), ["shared:v1", "shared:v2"]);
  assert.deepEqual(ids(sections[1]).sort(), ["ver_a", "ver_b"]);
  for (const id of ids(sections[0])) assert.ok(!ids(sections[1]).includes(id));
});

test("a room whose host cannot list is headed 'Versions on this device only'", () => {
  const sections = versionSections({ scope: HISTORY_SCOPE.DEVICE_ONLY, rows: [deviceRow("ver_a")], group });
  assert.deepEqual(sections.map((s) => [s.origin, s.heading]), [[ORIGIN.DEVICE, "Versions on this device only"]]);
});

test("standalone is one unheaded section, and no shared row can reach it", () => {
  const sections = versionSections({ scope: HISTORY_SCOPE.STANDALONE, rows: [deviceRow("ver_a"), sharedRow(hostRow(1))], group });
  assert.equal(sections.length, 1);
  assert.equal(sections[0].heading, "");
  assert.deepEqual(sections[0].groups[0].rows.map((r) => r.versionId), ["ver_a"]);
});

test("the scope follows the room and the provider", () => {
  assert.equal(historyScope({ room: "", provider: fakeProvider(0) }), HISTORY_SCOPE.STANDALONE);
  assert.equal(historyScope({ room: ROOM, provider: {} }), HISTORY_SCOPE.DEVICE_ONLY);
  assert.equal(historyScope({ room: ROOM, provider: undefined }), HISTORY_SCOPE.DEVICE_ONLY);
  assert.equal(historyScope({ room: ROOM, provider: fakeProvider(0) }), HISTORY_SCOPE.SHARED);
  assert.equal(sharedVersionSource({}, ROOM), null);
  assert.equal(sharedVersionSource(fakeProvider(0), ""), null);
});
