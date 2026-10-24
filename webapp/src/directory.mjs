// The host directory seam for @mention (`docs/175` §7, `docs/109` HF-361).
//
// THE RULE. The editor never enumerates people. It asks the host one bounded
// question — "who matches what the reader typed after `@`" — and shows the
// answer. With no host directory there is no picker, no fetch and no message:
// `@Ada` is just text.
//
// The named pattern is TYPEAHEAD OVER A REMOTE INDEX: query-driven, bounded,
// cancellable, never enumerated. Contrast ONLYOFFICE's `onRequestUsers`, which
// asks the host for its whole list and filters it on the client (`docs/175` §9).
//
// Two transports, one schema (ADR-036):
//   * in-process — `window.opendoc.directory = { search, notify }` (and the
//     `<opendoc-editor>` element's `directory` property, which it relays);
//   * postMessage — with `?directory=1`, `search` is the `directory-query`
//     event answered by the `directory.answer` request, and `notify` is the
//     `mention` event answered by `mention.answer`, correlated by `qid`.
//
// What is sent is the query and nothing else: no document text, no title,
// nothing about other participants. Nothing is cached past the open picker.
//
// Complexity: O(limit) per answer; nothing here reads the document.

/** The most answers the editor reads for one query (`docs/175` §7.2). */
export const DIRECTORY_LIMIT = 20;
/** Typing settles this long before a query goes out. */
export const DIRECTORY_DEBOUNCE_MS = 150;
/** How long a `postMessage` host has to answer before the editor stops waiting. */
export const DIRECTORY_TIMEOUT_MS = 5000;
/** The URL flag a `postMessage` host (or the element) sets to say it serves a directory. */
export const DIRECTORY_PARAM = "directory";
/** The longest mention excerpt sent with a notice, in characters. */
export const EXCERPT_CHARS = 280;
/** Bounds one entry must keep, the identity's (`docs/175` §4.4). */
export const ENTRY_BOUNDS = Object.freeze({ idBytes: 128, nameBytes: 255, linkBytes: 2048, avatarUrlBytes: 1024, initialsChars: 8 });
/** What a `notify` may answer. */
export const MENTION_STATUSES = Object.freeze(["notified", "needs-access", "declined"]);

const bytes = (value) => new TextEncoder().encode(value).length;

function linkAllowed(link) {
  if (typeof link !== "string" || !link) return false;
  if (bytes(link) > ENTRY_BOUNDS.linkBytes || /\s/u.test(link)) return false;
  return link.startsWith("mailto:") || link.startsWith("https://");
}

function avatarAllowed(url) {
  if (typeof url !== "string" || !url || bytes(url) > ENTRY_BOUNDS.avatarUrlBytes) return false;
  if (url.startsWith("https://")) {
    const authority = url.slice(8).split(/[/?#]/u)[0];
    return Boolean(authority) && !authority.includes("@");
  }
  return /^data:image\/(png|jpeg|webp)[;,]/u.test(url);
}

/** One directory entry, checked: `{id, name, initials?, avatar?, link?}` back,
 *  or null for an entry outside its bounds. Unknown fields are dropped. */
export function checkEntry(raw) {
  if (!raw || typeof raw !== "object") return null;
  const id = typeof raw.id === "string" ? raw.id : "";
  const name = typeof raw.name === "string" ? raw.name.trim() : "";
  if (!id || bytes(id) > ENTRY_BOUNDS.idBytes || !name || bytes(name) > ENTRY_BOUNDS.nameBytes) return null;
  const entry = { id, name };
  if (raw.initials != null) {
    const initials = String(raw.initials).trim();
    if (!initials || [...initials].length > ENTRY_BOUNDS.initialsChars) return null;
    entry.initials = initials;
  }
  if (raw.link != null) {
    if (!linkAllowed(raw.link)) return null;
    entry.link = raw.link;
  }
  if (raw.avatar != null) {
    if (!avatarAllowed(raw.avatar?.url)) return null;
    entry.avatar = { url: raw.avatar.url };
  }
  return Object.freeze(entry);
}

/** A host's answer, bounded: at most `limit` entries, each in bounds. What was
 *  dropped is COUNTED and returned so the caller reports it — over-limit
 *  results and out-of-bounds entries are never trimmed silently. */
export function checkAnswer(raw, limit = DIRECTORY_LIMIT) {
  const list = Array.isArray(raw?.people) ? raw.people : [];
  const people = [];
  let dropped = 0;
  for (const candidate of list) {
    if (people.length >= limit) { dropped += 1; continue; }
    const entry = checkEntry(candidate);
    if (entry) people.push(entry); else dropped += 1;
  }
  return Object.freeze({ people: Object.freeze(people), more: Boolean(raw?.more) || dropped > 0, dropped });
}

/** The query the text before the caret is asking, or null when there is none.
 *
 *  A query is `@` at the start of the text or after whitespace, then at least ONE
 *  character that is not whitespace or `@`: a bare `@` sends nothing, so the
 *  host is never asked for "everyone". Returns `{start, query}`, where `start`
 *  is the `@`'s offset in UTF-16 units (what `selectionStart` measures). */
export function mentionQuery(text, caret) {
  const before = String(text ?? "").slice(0, Math.max(0, caret ?? 0));
  const match = /(^|\s)@([^\s@]{1,64})$/u.exec(before);
  if (!match) return null;
  return { start: before.length - match[2].length - 1, query: match[2] };
}

/** Where each picked mention sits in `text` when the comment is posted.
 *
 *  Picked mentions are remembered by how they read (`@Name`), not by an offset,
 *  because the reader keeps typing after picking one. Each is found left to
 *  right, never overlapping the one before, so two mentions of one person are
 *  two spans and a label the reader has since deleted is simply not a mention.
 *  Offsets are UTF-16 units, which is what the engine's `addCommentMentioning`
 *  takes. O(text × mentions). */
export function placeMentions(text, picked) {
  const out = [];
  let from = 0;
  for (const pick of picked) {
    const spelled = `@${pick.name}`;
    const at = text.indexOf(spelled, from);
    if (at < 0) continue;
    out.push({ start: at, end: at + spelled.length, link: pick.link ?? null, id: pick.id });
    from = at + spelled.length;
  }
  return out;
}

/** A notice's excerpt: the comment's text, at most `EXCERPT_CHARS` characters. */
export function excerptOf(text) {
  const chars = [...String(text ?? "")];
  return chars.length <= EXCERPT_CHARS ? chars.join("") : `${chars.slice(0, EXCERPT_CHARS - 1).join("")}…`;
}

/**
 * The editor's side of the seam: one object the chrome asks, whichever
 * transport the host chose.
 *
 * @param {object} io
 * @param {(name: string, detail: object) => void} io.emit the session's event emitter.
 * @param {boolean} [io.remote] whether a `postMessage` host serves the directory.
 * @param {(fn: () => void, ms: number) => unknown} [io.setTimer]
 * @param {(handle: unknown) => void} [io.clearTimer]
 */
export function createDirectoryChannel({ emit, remote = false, setTimer = setTimeout, clearTimer = clearTimeout, timeoutMs = DIRECTORY_TIMEOUT_MS }) {
  let local = null;
  let counter = 0;
  /** qid -> {resolve, timer}. Bounded by the picker: one query in flight per keystroke, each aborted by the next. */
  const waiting = new Map();

  function ask(event, detail, signal) {
    const qid = `${event}:${(counter += 1)}`;
    return new Promise((resolve, reject) => {
      const timer = setTimer(() => { waiting.delete(qid); reject(new Error("timeout")); }, timeoutMs);
      waiting.set(qid, { resolve, timer });
      signal?.addEventListener?.("abort", () => {
        if (!waiting.delete(qid)) return;
        clearTimer(timer);
        reject(Object.assign(new Error("aborted"), { name: "AbortError" }));
      }, { once: true });
      emit(event, { qid, ...detail });
    });
  }

  function settle(qid, value) {
    const waiter = waiting.get(String(qid ?? ""));
    if (!waiter) return false;
    waiting.delete(String(qid));
    clearTimer(waiter.timer);
    waiter.resolve(value);
    return true;
  }

  return {
    /** Whether a host serves a directory at all. False means: no picker. */
    available: () => Boolean(local) || remote,
    /** The in-process directory (`window.opendoc.directory = {...}`). */
    get directory() { return local; },
    set directory(value) {
      local = value && typeof value.search === "function" ? value : null;
    },
    /** Who matches `query`. Resolves to a checked answer; rejects on abort. */
    async search(query, { limit = DIRECTORY_LIMIT, signal } = {}) {
      const bounded = Math.min(DIRECTORY_LIMIT, Math.max(1, Number(limit) || DIRECTORY_LIMIT));
      if (local) return checkAnswer(await local.search(String(query), { limit: bounded, signal }), bounded);
      if (!remote) return checkAnswer(null, bounded);
      return checkAnswer(await ask("directory-query", { query: String(query), limit: bounded }, signal), bounded);
    },
    /** Tells the host who was mentioned. Resolves to one of `MENTION_STATUSES`, or
     *  `null` when the host offers no notifier (the mention is then text only). */
    async notify(notice) {
      if (local) {
        if (typeof local.notify !== "function") return null;
        const answer = await local.notify(notice);
        return MENTION_STATUSES.includes(answer?.status) ? answer.status : "declined";
      }
      if (!remote) return null;
      const answer = await ask("mention", notice);
      return MENTION_STATUSES.includes(answer?.status) ? answer.status : "declined";
    },
    /** The `directory.answer` request: `{qid, people, more}`. */
    answerDirectory(message) {
      return settle(message?.qid, { people: message?.people, more: message?.more });
    },
    /** The `mention.answer` request: `{qid, status}`. */
    answerMention(message) {
      return settle(message?.qid, { status: message?.status });
    },
  };
}
