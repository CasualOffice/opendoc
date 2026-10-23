// Mentions in the two comment composers — a new comment and a reply — wired to
// the engine and to the notice queue (`docs/175` §7, `docs/109` HF-361).
//
// One helper for both composers, so a reply cannot mention differently from a
// comment. It decides three things and nothing else:
//   * which engine call posts the text (the mentioning variant only when the
//     reader picked somebody, so a comment with no mention is byte-for-byte what
//     it always was);
//   * which comment the call created, read from the engine's own list rather
//     than predicted — O(comments), and only when somebody was mentioned;
//   * whose notice waits for what: the relay's acknowledgement in a room, the
//     save when standalone (`mention_notices.mjs`).
import { excerptOf } from "./directory.mjs";
import { attachMentionPicker } from "./mention_picker.mjs";
import { createMentionNotices, pageMentionNotices } from "./mention_notices.mjs";

/** The room this page joined, as its link names it (the host's own room id). */
function roomOf(href) {
  try {
    return new URL(href).searchParams.get("room");
  } catch {
    return null;
  }
}

function commentIds(doc) {
  try {
    return new Set(JSON.parse(doc.listComments()).map((comment) => String(comment.id)));
  } catch {
    return new Set();
  }
}

/** The page's notice queue, made once, flushed on every standalone save. */
function notices(ed, t) {
  return pageMentionNotices(() => {
    const queue = createMentionNotices({ mentions: () => ed.hostSession?.mentions ?? null, status: (text, kind) => ed.setStatus(text, kind), t });
    ed.hostSession?.on?.("save", () => void queue.ordered("local"));
    return queue;
  });
}

/**
 * Attaches the picker to a composer and returns how to post through it.
 *
 * @param {object} ed the editor facade (`doc`, `runEdit`, `hostSession`, `collab`, `setStatus`).
 * @param {HTMLTextAreaElement} textarea
 * @param {(key: string, params?: object) => string} t
 */
export function attachCommentMentions(ed, textarea, t) {
  const picker = attachMentionPicker({ textarea, mentions: ed.hostSession?.mentions, t });
  return {
    picker,
    /**
     * Posts `text`. `plain(doc)` is the existing call; `mentioning(doc, json)` the
     * same call with the mention spans. Returns the edit's own result.
     */
    async post(text, { plain, mentioning }) {
      const placed = picker.placed();
      if (!placed.length) return ed.runEdit(() => plain(ed.doc), { keepView: true });
      const before = commentIds(ed.doc);
      const spans = JSON.stringify(placed.map(({ start, end, link }) => ({ start, end, link })));
      const result = await ed.runEdit(() => mentioning(ed.doc, spans), { keepView: true });
      const created = [...commentIds(ed.doc)].find((id) => !before.has(id));
      if (created) {
        const people = [];
        for (const { id } of placed) {
          const person = picker.picked().find((entry) => entry.id === id);
          if (person && !people.some((p) => p.id === id)) people.push({ id, name: person.name });
        }
        // `ed.collab` is the shared session's pipe: present exactly when in a room.
        const inRoom = Boolean(ed.collab);
        notices(ed, t).queue({
          scope: inRoom ? "room" : "local",
          people,
          commentId: created,
          excerpt: excerptOf(text),
          room: inRoom ? roomOf(globalThis.location?.href) : null,
        });
      }
      return result;
    },
  };
}
