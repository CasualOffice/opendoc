// Why a selected object will not do what was asked of it.
//
// The engine decides, per carrier, which of ten structural operations an object
// supports, and since `docs/109` HF-259 it publishes a REASON beside every `false`
// one (`capabilityReasons` on a hit payload, the same key on every `objectOrder`
// entry). Nothing read it: the chrome gated on the bare booleans, so a refused
// gesture did nothing and said nothing. The live case is the commonest picture
// there is — an INLINE one. `canMove` is false (it sits in the line of text), so
// dragging it was simply inert: no move, no message, the reader left to guess
// whether the editor was broken.
//
// This module is the host half: carrying the reasons with the selection, turning
// one into a sentence, and the drag that says it.
//
// Pure apart from `createRefusedDrag`, which owns no DOM either — it is handed
// pointer events and a status sink.

/** The marker and separator `casual_doc_edit::refusal` writes: a reason is
 *  `"refused: <sentence>\u001f<code>"`. */
const MARKER = "refused: ";
const CODE_SEPARATOR = "\u001f";

/**
 * The capability reasons a payload carries, as a plain `{name: reason}` record.
 *
 * Accepts both shapes the engine publishes — a JSON string (the hit payload's
 * getter, because `wasm_bindgen` cannot hand back a map) and an object (an
 * `objectOrder` entry, already parsed) — and copies, because a hit payload is
 * freed right after it is read.
 *
 * O(capabilities).
 *
 * @param {unknown} source a hit payload, an order entry, or anything else
 * @returns {Record<string, string>}
 */
export function readCapabilityReasons(source) {
  let raw = null;
  try {
    raw = source?.capabilityReasons ?? null;
  } catch {
    return {};
  }
  if (typeof raw === "string") {
    try {
      raw = JSON.parse(raw);
    } catch {
      return {};
    }
  }
  if (!raw || typeof raw !== "object") return {};
  return Object.fromEntries(
    Object.entries(raw).filter(([, reason]) => typeof reason === "string" && reason),
  );
}

/**
 * The sentence for one unavailable capability of the selected object, in the
 * reader's language when the host can route its code — the same contract
 * `editRefusalMessage` keeps for a refused command, so a control and the command
 * behind it never explain one fact two ways.
 *
 * Returns `""` when the capability is available, or when the engine published no
 * reason (a stale bridge): a caller then says nothing rather than inventing one.
 *
 * O(1).
 *
 * @param {{capabilityReasons?: Record<string, string>} | null | undefined} selection
 * @param {string} capability e.g. `"canMove"`
 * @param {(code: string) => string} [route] the host's code -> localised sentence
 * @returns {string}
 */
export function capabilityRefusal(selection, capability, route) {
  const reason = selection?.capabilityReasons?.[capability];
  if (typeof reason !== "string" || !reason) return "";
  const [sentence, code = ""] = reason.split(CODE_SEPARATOR);
  const routed = code && typeof route === "function" ? String(route(code) ?? "") : "";
  if (routed) return routed;
  return sentence.startsWith(MARKER) ? sentence.slice(MARKER.length) : sentence;
}

/** How far the pointer must travel, in CSS pixels, before a press on an object
 *  that cannot move counts as a DRAG rather than a click that selected it. */
export const REFUSED_DRAG_THRESHOLD_PX = 6;

/**
 * A press on an object that cannot move, watched until it becomes a drag — at
 * which point the reason is said, once.
 *
 * A click must stay silent: the click selected the object, which is exactly
 * what was asked. Only the DRAG is refused, so only the drag explains itself.
 *
 * O(1) per event.
 *
 * @param {{setStatus: (text: string, kind?: string) => void}} io
 */
export function createRefusedDrag(io) {
  let armed = null;
  return {
    /** Starts watching a press whose object will not move, with the sentence
     *  that says why. A press with no sentence is not watched. */
    arm(event, sentence) {
      armed = sentence ? { x: event.clientX, y: event.clientY, sentence, said: false } : null;
    },
    /** Says the reason the first time the pointer travels far enough to mean a
     *  drag. Returns whether it is watching a press at all. */
    move(event) {
      if (!armed) return false;
      if (event.buttons === 0) {
        armed = null;
        return false;
      }
      const far =
        Math.abs(event.clientX - armed.x) + Math.abs(event.clientY - armed.y) >=
        REFUSED_DRAG_THRESHOLD_PX;
      if (far && !armed.said) {
        armed.said = true;
        io.setStatus(armed.sentence, "error");
      }
      return true;
    },
    /** The press is over, however it ended. */
    end() {
      armed = null;
    },
  };
}
