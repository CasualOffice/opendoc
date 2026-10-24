// Telling the host who was mentioned — only once the comment is ORDERED
// (`docs/175` §7.2, `docs/109` HF-361).
//
// "Ordered" means: acknowledged by the relay in a shared room, or saved when the
// document is standalone. A mention in a comment that is later lost — a refused
// submission, a session that never reconnects, a document closed unsaved — must
// not already have e-mailed somebody. So a notice waits here, and is sent when
// `ordered(scope)` says its scope has been made durable:
//
//   * `"room"`  — `collab_shell.mjs`, when an acknowledgement leaves nothing of
//     this replica's unacknowledged;
//   * `"local"` — the host session's `save` event.
//
// What the host answered is said to the reader: "notified", "needs-access" (the
// host's own word that this person cannot open the document — sharing is the
// host's product, so the editor offers no share dialog), or "declined".
//
// Complexity: O(pending notices) per flush; the queue is bounded by what a
// reader can type between two acknowledgements, and capped below anyway.

/** At most this many notices wait at once; past it the oldest is said to be dropped. */
export const MAX_PENDING_NOTICES = 64;

/**
 * @param {object} io
 * @param {() => object|null} io.mentions the session's `mentions` seam.
 * @param {(text: string, kind?: string) => void} io.status the status line.
 * @param {(key: string, params?: object) => string} io.t
 */
export function createMentionNotices({ mentions, status, t, list = (names) => names.join(", ") }) {
  /** {scope, notice, names} in the order they were queued. */
  const pending = [];

  function say(answer, names) {
    const joined = list(names);
    if (answer === "notified") status(t("mention.notified", { names: joined }));
    else if (answer === "needs-access") status(t("mention.needsAccess", { names: joined }), "warn");
    else if (answer === "declined") status(t("mention.declined", { names: joined }), "warn");
  }

  return {
    /** Holds a notice until its scope is ordered. `people` are directory entries
     *  `{id, name}`; only the ids travel, the names are for the reader's sentence. */
    queue({ scope, people, commentId, excerpt, room = null }) {
      if (!people?.length) return;
      if (pending.length >= MAX_PENDING_NOTICES) {
        const dropped = pending.shift();
        status(t("mention.declined", { names: list(dropped.names) }), "warn");
      }
      pending.push({
        scope,
        names: people.map((person) => person.name),
        notice: Object.freeze({
          people: Object.freeze(people.map((person) => person.id)),
          commentId: String(commentId),
          excerpt: String(excerpt),
          room,
        }),
      });
    },
    /** Sends every notice waiting on `scope`. */
    async ordered(scope) {
      const seam = mentions();
      const due = [];
      for (let i = pending.length - 1; i >= 0; i -= 1) {
        if (pending[i].scope === scope) due.unshift(...pending.splice(i, 1));
      }
      for (const { notice, names } of due) {
        let answer = null;
        try {
          answer = await seam?.notify(notice);
        } catch {
          answer = "declined";
        }
        say(answer, names);
      }
    },
    /** How many notices are waiting, for a scope or in all. */
    pending: (scope) => pending.filter((entry) => !scope || entry.scope === scope).length,
  };
}

/** The page's one queue: one editor, one reader, one place that knows what is
 *  still waiting. Created by the first composer that needs it. */
let shared = null;

/** The page's queue, made by `make` the first time. */
export function pageMentionNotices(make) {
  if (!shared) shared = make();
  return shared;
}

/** Says that `scope` (`"room"` or `"local"`) has been made durable. A no-op
 *  when nothing was ever mentioned, which is every page with no directory. */
export function mentionsOrdered(scope) {
  return shared?.ordered(scope);
}
