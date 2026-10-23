// Shared version history in a room (`docs/175` §11, `docs/109` HF-362).
//
// Before this, version history was per browser even in a shared room: each participant
// saw only the versions their own tab had captured, under no name (`docs/174` RC-09). A
// room's history needs the HOST, because only the host can keep something every
// participant reads and only the host can say who made it. So the `docs/169` provider
// gains three optional methods, and this module is the editor's whole contact with them:
//
//   listVersions({ room, before, limit })  -> { versions: [{ id, revision, createdAt,
//                                               author: { id, name }, name?, kind }], more }
//   saveVersion({ room, revision, name, bytes })   -> { id }
//   versionBytes({ room, id })                     -> { bytes, revision }
//
// Four rules, each of which a unit test in `shared_versions.test.mjs` holds:
//
//  1. **The client never states an author.** `saveVersion` is called with exactly
//     `room`, `revision`, `name` and `bytes`. A version author the client could state is
//     one it could forge; the host attests it from its own session (`docs/175` §11).
//  2. **Bounded.** A page is at most `SHARED_PAGE_MAX` rows whatever the caller asks, a
//     page the host over-fills is cut to what was asked, and the pager holds at most
//     `SHARED_LOADED_MAX` rows. Older pages load only on an explicit request (`before` is
//     the id of the oldest row already held), so a room with ten thousand versions costs
//     one page to open.
//  3. **Cancellable.** Every read carries an `AbortSignal` (an extra field a host may
//     honour or ignore), and a superseded read's answer is dropped by token whether or
//     not the host stopped, so closing the panel or leaving the room never paints a stale
//     page.
//  4. **Shared and device rows are never mixed.** `versionSections` partitions by each
//     row's ORIGIN, not by which list it arrived in, so a device row handed to the shared
//     list still lands under "On this device".
//
// Version bytes are the `collabSnapshot()` package of `docs/169`, not a second format:
// the engine's `open` reads them, a preview parses them, and a download hands them over
// as the JSON snapshot they are.
//
// No English here: every sentence is a catalogue key the panel resolves.
import { t } from "./i18n.mjs";

/** Rows asked for when the caller names no limit. */
export const SHARED_PAGE = 25;
/** The most rows one page may carry, whatever the caller or the host says. */
export const SHARED_PAGE_MAX = 50;
/** The most shared rows the panel holds at once. Past it, "Load older" is disabled with
 *  the reason rather than growing the list without end. */
export const SHARED_LOADED_MAX = 500;

/** Where the timeline's rows come from (`docs/175` §11). */
export const HISTORY_SCOPE = Object.freeze({
  /** No room: IndexedDB only, unchanged (ADR-038). */
  STANDALONE: "standalone",
  /** A room whose provider has `listVersions`: the room's shared, authored versions,
   *  and this device's capture underneath in a group of its own. */
  SHARED: "shared",
  /** A room whose provider cannot list versions: today's panel, headed honestly. */
  DEVICE_ONLY: "deviceOnly",
});

/** Which rows a section holds. */
export const ORIGIN = Object.freeze({ SHARED: "shared", DEVICE: "device" });

const ID = /^[\w.:-]{1,128}$/;
const MAX_TEXT = 200;

/** The scope for a session: standalone without a room; in a room, shared only when the
 *  host can list. O(1). */
export function historyScope({ room, provider }) {
  if (!room) return HISTORY_SCOPE.STANDALONE;
  return typeof provider?.listVersions === "function" ? HISTORY_SCOPE.SHARED : HISTORY_SCOPE.DEVICE_ONLY;
}

/** A bounded, trimmed string, or "". Control and format characters go, because a bidi
 *  override in an author's name is a spoofing vector and a newline makes one row look
 *  like two (the rule `sanitiseVersionName` applies to local names). O(length). */
function text(value) {
  if (typeof value !== "string") return "";
  return value.replace(/[\p{Cc}\p{Cf}]/gu, " ").trim().replace(/\s+/g, " ").slice(0, MAX_TEXT);
}

/** A host time as epoch milliseconds: a number, or an ISO string. NaN when neither. */
function epoch(value) {
  if (typeof value === "number") return value;
  if (typeof value === "string") return Date.parse(value);
  return Number.NaN;
}

/**
 * One host row as a timeline row, or `null` when it is not a version.
 *
 * The shape is the device rows' (`versionId`, `createdAt`, `actor`, `name`, `kind`,
 * `revision`), so the row renderer, the day grouping and the change deltas serve both;
 * `origin` is what keeps them apart. `versionId` is prefixed so a host id can never
 * collide with a device version id in one panel. O(1).
 */
export function sharedRow(raw) {
  if (!raw || typeof raw !== "object") return null;
  const id = typeof raw.id === "string" ? raw.id : "";
  const createdAt = epoch(raw.createdAt);
  const revision = Number(raw.revision);
  const author = raw.author && typeof raw.author === "object" ? raw.author : null;
  const authorName = text(author?.name);
  if (!ID.test(id) || !Number.isFinite(createdAt) || !Number.isSafeInteger(revision) || revision < 0) return null;
  // An unattested version is not a shared version: the whole point of the host is the
  // author, and a row without one would be painted as somebody.
  if (!authorName || typeof author.id !== "string" || !author.id) return null;
  return Object.freeze({
    versionId: `shared:${id}`,
    sharedId: id,
    origin: ORIGIN.SHARED,
    createdAt,
    revision,
    actor: authorName,
    authorId: text(author.id),
    name: text(raw.name),
    kind: text(raw.kind) || (text(raw.name) ? "named" : "manual"),
    // The bytes are a `collabSnapshot()` package: the JSON snapshot format.
    formatId: "json",
  });
}

/** Why a host answer was refused, as a sentence. */
function refused(reason) {
  const error = new Error(reason);
  error.sharedVersions = true;
  return error;
}

/**
 * The provider's three methods, bound to one room, validated and bounded. `null` when the
 * provider cannot list, which is the "device only" scope.
 *
 * Each method throws an `Error` whose message is said to the reader as it is; nothing here
 * swallows a provider failure.
 */
export function sharedVersionSource(provider, room) {
  if (!room || typeof provider?.listVersions !== "function") return null;
  return Object.freeze({
    room,
    canSave: typeof provider.saveVersion === "function",
    canRead: typeof provider.versionBytes === "function",

    /** One page, newest first, of at most `limit` (clamped to 1..SHARED_PAGE_MAX). */
    async list({ before = null, limit = SHARED_PAGE, signal } = {}) {
      const bounded = Math.min(SHARED_PAGE_MAX, Math.max(1, Math.floor(Number(limit)) || SHARED_PAGE));
      const answer = await provider.listVersions({ room, before: before ?? null, limit: bounded, signal });
      if (!answer || !Array.isArray(answer.versions)) throw refused(t("versionHistory.shared.malformed"));
      // A host that sends more than it was asked for is cut to the page: the bound is
      // the editor's, not the host's courtesy.
      const page = answer.versions.slice(0, bounded);
      const rows = [];
      for (const raw of page) {
        const row = sharedRow(raw);
        if (!row) throw refused(t("versionHistory.shared.malformed"));
        rows.push(row);
      }
      return { rows, more: Boolean(answer.more) || answer.versions.length > bounded };
    },

    /** Saves the live document for everyone. NO author field: see rule 1. */
    async save({ revision, name, bytes }) {
      if (typeof provider.saveVersion !== "function") throw refused(t("versionHistory.shared.cannotSave"));
      const answer = await provider.saveVersion({ room, revision, name, bytes });
      if (!answer || typeof answer.id !== "string" || !ID.test(answer.id)) throw refused(t("versionHistory.shared.malformed"));
      return { id: answer.id };
    },

    /** A version's `collabSnapshot()` package. */
    async bytes({ id, signal }) {
      if (typeof provider.versionBytes !== "function") throw refused(t("versionHistory.shared.cannotRead"));
      const answer = await provider.versionBytes({ room, id, signal });
      const raw = answer?.bytes;
      const bytes = raw instanceof Uint8Array ? raw : raw instanceof ArrayBuffer ? new Uint8Array(raw) : null;
      if (!bytes || bytes.length === 0) throw refused(t("versionHistory.shared.malformed"));
      return { bytes, revision: Number(answer.revision) };
    },
  });
}

/**
 * The shared rows of one room, a page at a time.
 *
 * `first()` replaces what is held with the newest page; `older()` appends the next page
 * before the oldest row held. Each read supersedes the last: its token is bumped and its
 * signal aborted, and a superseded answer is dropped. `cancel()` does the same without a
 * new read. O(page) per read; O(rows held) to de-duplicate.
 */
export function createSharedPager(source, { pageSize = SHARED_PAGE, maxRows = SHARED_LOADED_MAX } = {}) {
  let rows = [];
  let more = false;
  let loading = false;
  let error = "";
  let token = 0;
  let controller = null;

  function cancel() {
    token += 1;
    controller?.abort();
    controller = null;
    loading = false;
  }

  async function read(before) {
    cancel();
    const mine = token;
    controller = typeof AbortController === "function" ? new AbortController() : null;
    loading = true;
    error = "";
    try {
      const page = await source.list({ before, limit: pageSize, signal: controller?.signal });
      if (mine !== token) return false;
      const held = before ? rows : [];
      const seen = new Set(held.map((row) => row.versionId));
      const fresh = page.rows.filter((row) => !seen.has(row.versionId));
      rows = held.concat(fresh).slice(0, maxRows);
      more = page.more;
      return true;
    } catch (err) {
      if (mine !== token) return false;
      error = String(err?.message ?? err);
      return true;
    } finally {
      if (mine === token) {
        loading = false;
        controller = null;
      }
    }
  }

  return {
    get rows() { return rows; },
    get more() { return more; },
    get loading() { return loading; },
    get error() { return error; },
    /** Whether the bound stops another page. */
    get full() { return rows.length >= maxRows; },
    room: source.room,
    first: () => read(null),
    older: () => (rows.length && !(rows.length >= maxRows) ? read(rows[rows.length - 1].sharedId) : Promise.resolve(false)),
    cancel,
  };
}

/**
 * The timeline as sections, each with its heading and its own day groups.
 *
 * - standalone: one section, no heading — the panel as it always was;
 * - shared: "Shared versions" (the room's), then "On this device" (this tab's capture);
 * - device only: one section headed "Versions on this device only".
 *
 * Rows are PARTITIONED BY ORIGIN, never by the list they arrived in, so shared and device
 * rows cannot share a section whatever a caller passes. Outside the shared scope a shared
 * row has no section at all. O(v log v) for the day grouping.
 *
 * @param {{scope: string, rows: object[], group: (rows: object[]) => object[]}} input
 *        `group` is the day grouping (`groupVersions` with its options bound).
 */
export function versionSections({ scope, rows, group }) {
  const shared = [];
  const device = [];
  for (const row of rows ?? []) {
    if (!row) continue;
    if (row.origin === ORIGIN.SHARED) shared.push(row);
    else device.push(row);
  }
  if (scope === HISTORY_SCOPE.SHARED) {
    return [
      { origin: ORIGIN.SHARED, heading: t("versionHistory.section.shared"), groups: group(shared), count: shared.length },
      { origin: ORIGIN.DEVICE, heading: t("versionHistory.section.device"), groups: group(device), count: device.length },
    ];
  }
  const heading = scope === HISTORY_SCOPE.DEVICE_ONLY ? t("versionHistory.section.deviceOnly") : "";
  return [{ origin: ORIGIN.DEVICE, heading, groups: group(device), count: device.length }];
}
