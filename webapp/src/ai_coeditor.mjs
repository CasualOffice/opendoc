// SPDX-License-Identifier: Apache-2.0
// Casual AI as a live CO-EDITOR (`docs/183` §23): its edits stream into the
// document at a visible pointer, as a person's would.
//
// THE PATTERNS, NAMED (SKILL §8):
//
//   * STREAMING with BATCHING: the model's text arrives as many small deltas; the
//     co-editor buffers them and writes one batch every `FLUSH_MS` (80 ms, inside
//     the owner's 50-100 ms), so the engine sees ~12 writes a second whatever the
//     token rate. Each batch is one engine call (`aiTurnWrite`), O(1) in the
//     document's size (O(paragraph) for a suggestion, exactly like typing one).
//   * A PRESENCE PARTICIPANT that is not a room member: a "Casual AI" caret and
//     name flag painted by the same seam the room's carets use
//     (`collab_presence.mjs` `paintCaretFlag`, `presence_strip.mjs`
//     `participantDisplay`), plus a Figma-style POINTER on a layer above the
//     pages that glides (eased) to where the AI works. It never takes the
//     person's pointer, focus or scroll: `pointer-events: none`, `aria-hidden`
//     (it holds nothing focusable), and nothing here scrolls.
//   * ANCHORS, NOT OFFSETS (ADR-030 I3): the engine owns the AI's insertion point
//     and maps it through every edit that is not the turn's (`aiTurnAnchor`), so
//     the person keeps typing elsewhere — or before the AI in its own paragraph —
//     and both land where each meant. The person's selection is mapped forward
//     over the AI's writes here (`mapPersonSelection`).
//   * ONE UNDO STEP per turn: every commit of a turn joins one undo group in the
//     engine's log (`Coalesce::Join`), even with the person's own commits
//     interleaved; Accept all / Reject all act on that turn only.
//   * SAFETY AS A ROLLBACK, not a preview: the add-in validates each delta
//     (`assistant_core.js` `createGuard`) and this module enforces its own
//     ceiling (bytes, characters, the effective mode). A violation stops the
//     stream and REVERTS the turn (`aiTurnRevert(turn, true)`), which also drops
//     it from Undo, as if it never happened. This replaces the private preview
//     for the assistant's edit actions (ADR-077 as amended).
//
// Complexity: O(1) per delta and per batch beyond the engine's own write.
import { participantDisplay } from "./presence_strip.mjs";
import { paintCaretFlag } from "./collab_presence.mjs";
import { admits } from "./ai_edit_mode.mjs";
import { randomUuid } from "./web_crypto.mjs";
import { AI_COLOR, aiLiveMoved } from "./ai_live.mjs";

/** How long deltas are buffered before one batch is written. */
export const FLUSH_MS = 80;
/** The editor's own ceiling on one turn, whatever the add-in's guard allows. */
export const TURN_LIMITS = Object.freeze({ bytes: 256 * 1024, breaks: 400, cellBytes: 4096, rows: 50, columns: 20 });
/** How long a range to be replaced is highlighted before it changes. */
export const HIGHLIGHT_MS = 450;
/** The AI's colour: one fixed hue so it is recognisable across sessions, with
 *  the ink the presence seam measures for it (`participantDisplay`). */
export { AI_COLOR };

const CONTROL = /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F‪-‮⁦-⁩]/;

/** UTF-8 byte length. O(text). */
export function utf8Length(text) {
  let bytes = 0;
  const s = String(text ?? "");
  for (let i = 0; i < s.length; i += 1) {
    const c = s.charCodeAt(i);
    if (c < 0x80) bytes += 1;
    else if (c < 0x800) bytes += 2;
    else if (c >= 0xd800 && c <= 0xdbff) {
      bytes += 4;
      i += 1;
    } else bytes += 3;
  }
  return bytes;
}

/**
 * Where the person's selection goes after the AI wrote at `before` and its
 * anchor moved to `after`: a point in `before`'s paragraph at or past the AI's
 * point keeps its distance from the AI's text, which is now in `after`'s
 * paragraph (the same one, unless the write held a paragraph break). Anything
 * else is untouched. Pure, O(1).
 */
export function mapPersonSelection(selection, before, after) {
  if (!selection || !before || !after) return selection;
  const point = (p) => (p && p.node === before.node && p.offset >= before.offset ? { node: after.node, offset: after.offset + (p.offset - before.offset) } : p);
  const anchor = point(selection.anchor);
  const focus = point(selection.focus);
  return anchor === selection.anchor && focus === selection.focus ? selection : { ...selection, anchor, focus };
}

/** The editor's ceiling on one batch: a violation code, or null. Pure. */
export function hostViolation(text, written, { breaks = 0, cell = false } = {}) {
  if (CONTROL.test(text)) return "control";
  if (written + utf8Length(text) > (cell ? TURN_LIMITS.cellBytes : TURN_LIMITS.bytes)) return "length";
  if (cell && text.includes("\n")) return "plan";
  if (breaks + (text.match(/\n/g) ?? []).length > TURN_LIMITS.breaks) return "length";
  return null;
}

function isoNow() {
  return new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
}

/**
 * The co-editor.
 *
 * @param {object} ed the editor context (`docs/177`)
 * @param {object} io
 * @param {(key: string, params?: object) => string} io.t
 * @param {() => {mode: string}} io.effective the effective access (`ai_edit_mode.mjs`)
 * @param {Window} [io.view]
 */
export function createCoEditor(ed, { t, effective, view = globalThis }) {
  const doc = view.document;
  /** The live turn, or the last one still awaiting Accept all / Reject all. */
  let turn = null;
  let bar = null;
  let pointer = null;

  const display = () => participantDisplay({ client: null, profile: { name: "Casual AI", color: AI_COLOR }, attested: true }, { t });

  // ---- The pointer: one persistent element above the pages --------------------

  function pointerEl() {
    if (pointer) return pointer;
    const viewport = doc.getElementById("viewport");
    if (!viewport) return null;
    const view = display();
    pointer = doc.createElement("div");
    pointer.className = "ai-co-pointer";
    pointer.id = "aiCoPointer";
    // Decoration: what it shows is said in words by the turn bar's status line.
    pointer.setAttribute("aria-hidden", "true");
    pointer.hidden = true;
    pointer.style.setProperty("--ai-co-color", view.color);
    pointer.style.setProperty("--ai-co-ink", view.ink);
    const svgNs = "http://www.w3.org/2000/svg";
    const svg = doc.createElementNS(svgNs, "svg");
    svg.setAttribute("viewBox", "0 0 16 20");
    svg.setAttribute("width", "16");
    svg.setAttribute("height", "20");
    const path = doc.createElementNS(svgNs, "path");
    path.setAttribute("d", "M1 1 L1 16 L5 12 L8 19 L11 18 L8 11 L14 11 Z");
    svg.append(path);
    const tag = doc.createElement("span");
    tag.className = "ai-co-pointer-tag";
    tag.textContent = view.label;
    pointer.append(svg, tag);
    viewport.append(pointer);
    return pointer;
  }

  /** Viewport-content coordinates of the AI's caret, or null when its page is
   *  not on screen (then the pointer waits where it was, hidden). */
  function caretPoint(at) {
    if (!ed.doc || !at) return null;
    let flat = [];
    try {
      flat = ed.doc.caretRect(at.node, at.offset) ?? [];
    } catch {
      flat = [];
    }
    if (flat.length < 5) return null;
    const page = ed.pages?.[flat[0] - 1];
    const viewport = doc.getElementById("viewport");
    if (!page?.wrap || !viewport) return null;
    const { rect, sx, sy } = ed.scaleOf(page);
    const box = viewport.getBoundingClientRect();
    return {
      x: rect.left - box.left + viewport.scrollLeft + flat[1] * sx,
      y: rect.top - box.top + viewport.scrollTop + (flat[2] + flat[4]) * sy,
    };
  }

  /** Moves the pointer to the turn's anchor: gliding when the AI moved, jumping
   *  when the page moved under it (a zoom, a re-render). */
  function placePointer(glide) {
    const el = pointerEl();
    if (!el) return;
    const at = turn?.writing ? currentAnchor() : null;
    const point = at ? caretPoint(at) : null;
    if (!point) {
      el.hidden = true;
      return;
    }
    const wasHidden = el.hidden;
    el.classList.toggle("is-gliding", glide && !wasHidden);
    el.hidden = false;
    el.style.transform = `translate(${Math.round(point.x)}px, ${Math.round(point.y)}px)`;
    el.dataset.node = at.node;
    el.dataset.offset = String(at.offset);
    // The AI moved (not just the page under it): a shared room tells the others.
    if (glide) aiLiveMoved();
  }

  function hidePointer() {
    if (pointer) pointer.hidden = true;
    aiLiveMoved();
  }

  function currentAnchor() {
    if (!turn || !ed.doc) return null;
    try {
      const at = JSON.parse(ed.doc.aiTurnAnchor(turn.engineId));
      return at && at.node ? at : null;
    } catch {
      return null;
    }
  }

  // ---- The turn bar: "Casual AI is writing… · Stop · Accept all · Reject all" --

  function barEl() {
    if (bar) return bar;
    const root = doc.createElement("div");
    root.id = "aiTurnBar";
    root.className = "ai-turn-bar";
    root.setAttribute("role", "toolbar");
    root.hidden = true;
    root.style.setProperty("--ai-co-color", AI_COLOR);
    const who = doc.createElement("span");
    who.className = "ai-turn-who";
    const dot = doc.createElement("span");
    dot.className = "ai-turn-dot";
    dot.setAttribute("aria-hidden", "true");
    const status = doc.createElement("span");
    status.className = "ai-turn-status";
    status.id = "aiTurnStatus";
    status.setAttribute("role", "status");
    const mode = doc.createElement("span");
    mode.className = "ai-turn-mode";
    who.append(dot, status, mode);
    const button = (className, onClick) => {
      const b = doc.createElement("button");
      b.type = "button";
      b.className = `dialog-button ${className}`;
      b.addEventListener("click", onClick);
      return b;
    };
    const stop = button("ai-turn-stop", () => coeditor.stop(turn?.id, "person"));
    const accept = button("dialog-button-primary ai-turn-accept", () => void coeditor.decide(turn?.id, true));
    const reject = button("ai-turn-reject", () => void coeditor.decide(turn?.id, false));
    root.append(who, stop, accept, reject);
    root.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && turn?.writing) {
        event.preventDefault();
        event.stopPropagation();
        coeditor.stop(turn.id, "person");
      }
    });
    doc.body.append(root);
    bar = { root, status, mode, stop, accept, reject, dot };
    return bar;
  }

  /** Centres the bar over the page column, above the status bar. */
  function placeBar() {
    if (!bar || bar.root.hidden) return;
    const viewport = doc.getElementById("viewport");
    if (!viewport) return;
    const box = viewport.getBoundingClientRect();
    bar.root.style.left = `${Math.round(box.left + box.width / 2)}px`;
    bar.root.style.top = `${Math.round(box.bottom - 12)}px`;
  }

  function reflectBar() {
    const b = barEl();
    if (!turn || turn.decided) {
      b.root.hidden = true;
      return;
    }
    b.root.setAttribute("aria-label", t("ai.turn.label"));
    b.root.dataset.state = turn.writing ? "writing" : "done";
    b.root.dataset.mode = turn.mode;
    b.status.textContent = turn.writing ? t("ai.turn.writing") : turn.stopped ? t("ai.turn.stopped") : t("ai.turn.done");
    b.mode.textContent = t(turn.mode === "direct" ? "ai.turn.mode.direct" : "ai.turn.mode.suggest");
    b.stop.textContent = t("ai.turn.stop");
    b.stop.hidden = !turn.writing;
    b.stop.setAttribute("aria-keyshortcuts", "Escape");
    b.accept.textContent = t(turn.mode === "direct" ? "ai.turn.keep" : "ai.turn.acceptAll");
    b.reject.textContent = t(turn.mode === "direct" ? "ai.turn.undoAll" : "ai.turn.rejectAll");
    b.root.hidden = false;
    placeBar();
  }
  view.addEventListener?.("resize", placeBar);

  // ---- Applying an engine result without moving the person ---------------------

  async function applyAi(result, before) {
    const after = currentAnchor();
    if (before && after && ed.selection) ed.selection = mapPersonSelection(ed.selection, before, after);
    await ed.applyEditResult(result, { keepView: true, keepSelection: true });
    placePointer(true);
  }

  function engine(call) {
    try {
      return { ok: true, value: call() };
    } catch (err) {
      return { ok: false, error: String(err?.message ?? err) };
    }
  }

  /** Serialises every engine write of a turn: a batch never overtakes the last. */
  function queue(task) {
    const run = turn?.chain ?? Promise.resolve();
    const next = run.then(task, task);
    if (turn) turn.chain = next.catch(() => {});
    return next;
  }

  function tell(state, extra = {}) {
    const current = turn;
    if (!current) return;
    try {
      current.notify?.({ turn: current.id, state, mode: current.mode, ...extra });
    } catch {
      /* the add-in went away; the turn is the editor's */
    }
  }

  // ---- Batching ------------------------------------------------------------------

  function schedule() {
    if (!turn || turn.timer || !turn.pending) return;
    turn.timer = view.setTimeout(() => {
      if (!turn) return;
      turn.timer = 0;
      void flush();
    }, FLUSH_MS);
  }

  /** Writes what is buffered NOW as one batch, after every earlier batch. The
   *  text is taken at the call, so a later move (the next table cell) can never
   *  pull it along to the wrong place. */
  function flush() {
    const current = turn;
    if (!current || !current.writing || !current.pending) return queue(async () => {});
    const text = current.pending;
    current.pending = "";
    if (current.timer) view.clearTimeout(current.timer);
    current.timer = 0;
    return queue(async () => {
      if (turn !== current || !current.writing) return;
      // An IME composition owns the person's own text: wait it out, never cut in.
      while (ed.composingText && turn === current && current.writing) {
        await new Promise((resolve) => view.setTimeout(resolve, FLUSH_MS));
      }
      if (turn !== current || !current.writing) return;
      const before = currentAnchor();
      const result = engine(() => ed.doc.aiTurnWrite(current.engineId, text));
      if (!result.ok) {
        await rollback(current, "engine");
        return;
      }
      current.batches += 1;
      await applyAi(result.value, before);
    });
  }

  async function rollback(current, code) {
    if (!current || current.decided) return;
    current.pending = "";
    if (current.timer) view.clearTimeout(current.timer);
    current.timer = 0;
    current.writing = false;
    current.decided = true;
    engine(() => ed.doc.aiTurnEnd(current.engineId));
    const reverted = engine(() => ed.doc.aiTurnRevert(current.engineId, true));
    if (reverted.ok) await ed.applyEditResult(reverted.value, { keepView: true, keepSelection: true });
    hidePointer();
    reflectBar();
    ed.paintOverlayLayer?.();
    ed.setStatus(t("ai.turn.rolledBack", { reason: t(`ai.violation.${code}`) }), "error");
    tell("rolledBack", { code });
  }

  function refusal(code, extra = {}) {
    return Object.freeze({ ok: false, code, message: t(`ai.violation.${code}`), ...extra });
  }

  /** The paragraph text `[start, end)` holds now, or null. */
  function textAt(node, start, end) {
    try {
      return ed.doc.copyText(node, start, node, end);
    } catch {
      return null;
    }
  }

  const coeditor = {
    /** Where the AI is writing, `{node, offset}`, while a turn writes; else
     *  null. A shared room puts it in this participant's caret frame. */
    writingAnchor() {
      return turn?.writing ? currentAnchor() : null;
    },

    /** The live turn's id and state, or null. */
    current() {
      return turn ? { turn: turn.id, mode: turn.mode, writing: turn.writing, decided: turn.decided, batches: turn.batches, pending: turn.pending.length, written: turn.written, timer: !!turn.timer } : null;
    },

    /**
     * Opens a turn and moves the pointer to where it will work.
     *
     * `spec.kind`:
     *   `insert`  `{at: {node, offset}}`            text at a point
     *   `replace` `{node, start, end, exact}`      the range, re-checked, replaced
     *   `remove`  `{anchor, focus}`                 a cross-paragraph range removed
     *   `after`   `{block}`                         a new paragraph after a block
     *   `table`   `{after, rows, columns, header}`  a table after a paragraph
     *   `replaceTable` `{table}`                    a new paragraph, then the table removed
     *
     * Refused (`{ok: false, code}`) when the effective mode admits no edit — the
     * CODE's authority, whatever the model or the add-in believes.
     */
    async begin(spec, { notify = null, kind = "text" } = {}) {
      if (!ed.doc || typeof ed.doc.aiTurnBegin !== "function") return refusal("unavailable");
      const access = effective();
      if (!admits(access.mode, kind)) return refusal("access", { mode: access.mode });
      if (turn?.writing) coeditor.stop(turn.id, "replaced");
      const mode = access.mode === "direct" ? "direct" : "suggest";
      const opened = engine(() => ed.doc.aiTurnBegin(mode, isoNow()));
      if (!opened.ok) return refusal("engine", { detail: opened.error });
      turn = { id: `ai-turn-${randomUuid()}`, engineId: opened.value, mode, access: access.mode, writing: true, decided: false, stopped: false, pending: "", timer: 0, chain: Promise.resolve(), written: 0, breaks: 0, batches: 0, cells: null, cellBytes: 0, notify, highlight: null };
      const current = turn;
      const steps = [];
      const k = spec?.kind;
      if (k === "insert" || k === "at") {
        steps.push(() => ed.doc.aiTurnSetAnchor(current.engineId, String(spec.at?.node ?? ""), Number(spec.at?.offset) >>> 0));
      } else if (k === "replace") {
        const start = Number(spec.start) >>> 0;
        const end = Number(spec.end) >>> 0;
        if (end <= start || textAt(spec.node, start, end) !== String(spec.exact ?? "")) {
          turn = null;
          engine(() => ed.doc.aiTurnEnd(current.engineId));
          return refusal("stale");
        }
        // The range it replaces, marked briefly before it changes.
        current.highlight = { node: spec.node, start, end };
        ed.paintOverlayLayer?.();
        steps.push(() => ed.doc.aiTurnSetAnchor(current.engineId, String(spec.node), start));
        steps.push("wait");
        steps.push(() => ed.doc.aiTurnReplace(current.engineId, String(spec.node), start, end));
      } else if (k === "remove") {
        steps.push(() => ed.doc.aiTurnDeleteRange(current.engineId, String(spec.anchor.node), Number(spec.anchor.offset) >>> 0, String(spec.focus.node), Number(spec.focus.offset) >>> 0));
      } else if (k === "after") {
        steps.push(() => ed.doc.aiTurnParagraphAfter(current.engineId, String(spec.block ?? "")));
      } else if (k === "table") {
        const rows = Math.min(TURN_LIMITS.rows, Math.max(1, Number(spec.rows) | 0));
        const columns = Math.min(TURN_LIMITS.columns, Math.max(1, Number(spec.columns) | 0));
        steps.push(() => ed.doc.aiTurnInsertTable(current.engineId, String(spec.after ?? ""), rows, columns, spec.header !== false));
      } else if (k === "replaceTable") {
        steps.push(() => ed.doc.aiTurnParagraphAfter(current.engineId, String(spec.table ?? "")));
        steps.push(() => ed.doc.aiTurnRemoveTable(current.engineId, String(spec.table ?? "")));
      } else {
        turn = null;
        engine(() => ed.doc.aiTurnEnd(current.engineId));
        return refusal("plan");
      }
      placePointer(true);
      reflectBar();
      for (const step of steps) {
        if (step === "wait") {
          await new Promise((resolve) => view.setTimeout(resolve, HIGHLIGHT_MS));
          current.highlight = null;
          if (turn !== current || !current.writing) return refusal("stopped");
          continue;
        }
        const before = currentAnchor();
        const result = engine(step);
        if (!result.ok) {
          // Whatever the turn already did is undone; nothing is left half-built.
          await rollback(current, "engine");
          return refusal("engine", { detail: result.error });
        }
        if (result.value && typeof result.value === "object" && "free" in result.value) await applyAi(result.value, before);
      }
      if (k === "table") {
        const cells = engine(() => JSON.parse(ed.doc.aiTurnTableCells(current.engineId)));
        current.cells = cells.ok ? cells.value : null;
      }
      placePointer(true);
      tell("writing");
      return Object.freeze({ ok: true, turn: current.id, mode, cells: current.cells });
    },

    /** Mid-turn: the AI moves to `[start, end)` of `node`, marks it, and replaces
     *  it (a review finding, a paragraph of a multi-paragraph rewrite). The text
     *  that replaces it is the next `write`. */
    async replace(id, spec) {
      const current = turn;
      if (!current || current.id !== id || !current.writing) return refusal("gone");
      if (!admits(current.access, "text")) return refusal("access", { mode: current.access });
      await flush();
      return queue(async () => {
        if (turn !== current || !current.writing) return refusal("gone");
        const start = Number(spec?.start) >>> 0;
        const end = Number(spec?.end) >>> 0;
        if (end <= start || textAt(spec.node, start, end) !== String(spec.exact ?? "")) return refusal("stale");
        engine(() => ed.doc.aiTurnSetAnchor(current.engineId, String(spec.node), start));
        current.highlight = { node: spec.node, start, end };
        placePointer(true);
        ed.paintOverlayLayer?.();
        await new Promise((resolve) => view.setTimeout(resolve, HIGHLIGHT_MS));
        current.highlight = null;
        if (turn !== current || !current.writing) return refusal("stopped");
        const before = currentAnchor();
        const result = engine(() => ed.doc.aiTurnReplace(current.engineId, String(spec.node), start, end));
        if (!result.ok) return refusal("stale", { detail: result.error });
        await applyAi(result.value, before);
        return Object.freeze({ ok: true });
      });
    },

    /** Mid-turn: removes a range that may span paragraphs (the paragraphs a
     *  table was made from). Tracked as a deletion when suggesting. */
    async remove(id, spec) {
      const current = turn;
      if (!current || current.id !== id || !current.writing) return refusal("gone");
      if (!admits(current.access, "text")) return refusal("access", { mode: current.access });
      await flush();
      return queue(async () => {
        if (turn !== current || !current.writing) return refusal("gone");
        const a = spec?.anchor;
        const f = spec?.focus;
        if (!a?.node || !f?.node) return refusal("plan");
        const result = engine(() => ed.doc.aiTurnDeleteRange(current.engineId, String(a.node), Number(a.offset) >>> 0, String(f.node), Number(f.offset) >>> 0));
        if (!result.ok) return refusal("stale", { detail: result.error });
        await applyAi(result.value, null);
        return Object.freeze({ ok: true });
      });
    },

    /** Mid-turn: a comment by Casual AI on `[start, end)` of `node`, part of the
     *  turn (Reject all removes it with the rest). Admitted in comment-only mode. */
    async comment(id, spec) {
      const current = turn;
      if (!current || current.id !== id || !current.writing) return refusal("gone");
      if (!admits(current.access, "comment")) return refusal("access", { mode: current.access });
      const text = String(spec?.text ?? "").slice(0, 4000);
      if (!text || CONTROL.test(text)) return refusal("control");
      await flush();
      return queue(async () => {
        if (turn !== current || !current.writing) return refusal("gone");
        const start = Number(spec?.start) >>> 0;
        const end = Number(spec?.end) >>> 0;
        if (spec.exact !== undefined && textAt(spec.node, start, end) !== String(spec.exact)) return refusal("stale");
        engine(() => ed.doc.aiTurnSetAnchor(current.engineId, String(spec.node), end));
        placePointer(true);
        const result = engine(() => ed.doc.aiTurnComment(current.engineId, String(spec.node), start, end, text));
        if (!result.ok) return refusal("stale", { detail: result.error });
        await applyAi(result.value, null);
        return Object.freeze({ ok: true });
      });
    },

    /** Text for the turn, buffered into the next batch. `cell` `[row, column]`
     *  moves the AI to that cell of the table it inserted first. */
    write(id, text, { cell = null } = {}) {
      const current = turn;
      if (!current || current.id !== id || !current.writing) return refusal("gone");
      if (!admits(current.access, "text")) {
        void rollback(current, "access");
        return refusal("access", { mode: current.access });
      }
      const piece = String(text ?? "");
      if (cell) {
        const [r, c] = cell;
        const target = current.cells?.cells?.[r]?.[c];
        if (!target) return refusal("plan");
        void flush();
        current.cellBytes = 0;
        queue(async () => {
          if (turn !== current || !current.writing) return;
          engine(() => ed.doc.aiTurnSetAnchor(current.engineId, target, 0));
          placePointer(true);
        });
      }
      if (!piece) return Object.freeze({ ok: true });
      const inCell = !!current.cells;
      const violation = hostViolation(piece, inCell ? current.cellBytes : current.written, { breaks: current.breaks, cell: inCell })
        ?? (current.written + utf8Length(piece) > TURN_LIMITS.bytes ? "length" : null);
      if (violation) {
        void rollback(current, violation);
        return refusal(violation);
      }
      current.written += utf8Length(piece);
      if (current.cells) current.cellBytes += utf8Length(piece);
      current.breaks += (piece.match(/\n/g) ?? []).length;
      current.pending += piece;
      schedule();
      return Object.freeze({ ok: true });
    },

    /** The stream ended: the last batch is written and the turn awaits a decision. */
    async end(id) {
      const current = turn;
      if (!current || current.id !== id || !current.writing) return refusal("gone");
      if (current.timer) view.clearTimeout(current.timer);
      current.timer = 0;
      await flush();
      await queue(async () => {});
      if (turn !== current || current.decided) return refusal("gone");
      current.writing = false;
      const summary = engine(() => JSON.parse(ed.doc.aiTurnEnd(current.engineId)));
      hidePointer();
      reflectBar();
      ed.paintOverlayLayer?.();
      tell("done");
      return Object.freeze({ ok: true, summary: summary.ok ? summary.value : null });
    },

    /** Stop: halts at once and keeps what was already written (Docs' "Stop"). The
     *  buffered, not-yet-written text is dropped: it was never on the page. */
    stop(id, by = "person") {
      const current = turn;
      if (!current || current.id !== id || !current.writing) return false;
      current.pending = "";
      if (current.timer) view.clearTimeout(current.timer);
      current.timer = 0;
      current.writing = false;
      current.stopped = true;
      engine(() => ed.doc.aiTurnEnd(current.engineId));
      hidePointer();
      reflectBar();
      ed.paintOverlayLayer?.();
      tell("stopped", { by });
      return true;
    },

    /** A violation the add-in's guard found: the turn is rolled back. */
    async abort(id, code = "plan") {
      const current = turn;
      if (!current || current.id !== id) return false;
      await queue(async () => {});
      await rollback(current, code);
      return true;
    },

    /** Accept all (or Keep) / Reject all (or Undo all) for the turn. */
    async decide(id, accept) {
      const current = turn;
      if (!current || current.id !== id || current.decided) return refusal("gone");
      if (current.writing) coeditor.stop(id, "decision");
      await queue(async () => {});
      const result = engine(() => (accept ? ed.doc.aiTurnAccept(current.engineId) : ed.doc.aiTurnRevert(current.engineId, false)));
      current.decided = true;
      if (result.ok && result.value && typeof result.value === "object" && "free" in result.value) {
        await ed.applyEditResult(result.value, { keepView: true, keepSelection: true });
      }
      reflectBar();
      ed.paintOverlayLayer?.();
      if (!result.ok) {
        ed.setStatus(t("ai.violation.engine"), "error");
        return refusal("engine", { detail: result.error });
      }
      ed.setStatus(t(accept ? "ai.turn.accepted" : "ai.turn.rejected"));
      tell(accept ? "accepted" : "rejected");
      return Object.freeze({ ok: true });
    },

    /** Paints the AI's caret, flag and any range it is about to replace on the
     *  page overlays (called from the editor's one overlay repaint), and puts the
     *  pointer back where the page now is (a zoom or a scroll re-render). */
    paint(place) {
      if (!turn || !ed.doc) return;
      if (turn.highlight) {
        const { node, start, end } = turn.highlight;
        let rects = [];
        try {
          rects = ed.doc.selectionRects(node, start, node, end) ?? [];
        } catch {
          rects = [];
        }
        for (let i = 0; i + 4 < rects.length; i += 5) {
          const el = place(rects.slice(i, i + 5), "ai-co-range");
          if (el) el.style.setProperty("--ai-co-color", AI_COLOR);
        }
      }
      if (!turn.writing) return;
      const at = currentAnchor();
      if (at) {
        let rect = [];
        try {
          rect = ed.doc.caretRect(at.node, at.offset) ?? [];
        } catch {
          rect = [];
        }
        const el = paintCaretFlag(place, rect, display(), t);
        if (el) {
          el.classList.add("ai-co-caret");
          el.dataset.agent = "casual-ai";
        }
      }
      placePointer(false);
    },
  };
  return coeditor;
}
