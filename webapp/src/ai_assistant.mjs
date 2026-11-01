// SPDX-License-Identifier: Apache-2.0
// `editor.ai`: a host's own assistant, in the page, proposing edits the person
// sees privately before anything reaches the document (`docs/178` §9,
// `docs/181` §4.4, the owner's 2026-10-28 decision).
//
// THE MODEL, AND WHOSE IT IS. Gemini in Google Docs and Copilot in Word both
// show an assistant's proposal to the person who asked for it, in a card of its
// own beside the document, with the document untouched until they choose:
// Docs' "Insert" / "Replace" against a discard, Word's "Keep it" (or "Replace")
// against a discard, and Tiptap's AI Toolkit calls the same thing "review
// changes" before they are applied. Our durable alternative — the agent writes
// tracked changes everyone sees — stays available as `apply`. This module is the
// private one:
//
//   * The plan is compiled by the ENGINE (`aiPreview`, `casual-doc-ai`'s one
//     compiler) into a redline and a set of operations the engine keeps under an
//     id. Nothing is applied, nothing is sent to a room, nothing joins the undo
//     stack.
//   * The panel shows the redline: what each affected paragraph becomes, struck
//     deletions and underlined insertions on paper colours (the document's own
//     surface, in both themes), new paragraphs labelled, and every edit the
//     engine refused WITH its reason.
//   * Accept commits exactly what was shown, through the editor's one choke point,
//     as one undo step — as tracked suggestions authored "Casual AI", or as
//     direct edits only when the host granted them. Discard leaves no trace.
//
// WHERE WE DIFFER FROM WORD, deliberately: Word applies a Copilot rewrite as a
// direct edit unless Track Changes is on. Here the default is a suggestion
// whatever the review mode, because the assistant is acting for the person, and
// a reviewer must be able to tell its words from theirs (`docs/178` §5.2).
//
// No role names (`capabilities.mjs`: "nothing downstream should ever branch on a
// role name"): the access level is derived from the GRANT, in `aiAccessFor`.
import { CAPABILITIES, hostConfig } from "./capabilities.mjs";
import { t } from "./i18n.mjs";
import { startWebMcp } from "./ai_webmcp.mjs";
import { startAiPlatform } from "./ai_platform.mjs";

/** The access levels the engine's `aiRead`/`aiPreview` accept, weakest first. */
export const AI_ACCESS = Object.freeze(["none", "read", "comment", "suggest", "review"]);

/**
 * What an in-page assistant may do under a grant (`docs/178` §9's table).
 *
 *   nothing granted (a preview) → `none`: not even reads
 *   no `edit`, no `comment`      → `read`
 *   `comment`, no `edit`         → `comment`: comments only
 *   `edit`                       → `suggest`; direct edits only with the host's
 *                                  explicit grant (`?ai=direct`)
 *   every capability (the owner) → `review`: may also accept or reject changes
 *
 * The ENGINE enforces each level (the profile the plan compiles under, then the
 * choke point's own grant and protection checks); this only chooses it.
 *
 * Complexity: O(capabilities).
 *
 * @param {{has: (name: string) => boolean, size?: number}} capabilities
 * @param {{direct?: boolean}} [options]
 * @returns {{access: string, direct: boolean}}
 */
export function aiAccessFor(capabilities, { direct = false } = {}) {
  const has = (name) => capabilities.has(name);
  if (!CAPABILITIES.some(has)) return { access: "none", direct: false };
  if (has("edit")) {
    return { access: CAPABILITIES.every(has) ? "review" : "suggest", direct: direct === true };
  }
  if (has("comment")) return { access: "comment", direct: false };
  return { access: "read", direct: false };
}

/** The host refusal code (`host_contract.mjs` REFUSAL_CODES) an engine or AI-layer
 *  code becomes. The AI code itself travels beside it as `aiCode`, so a host can
 *  still tell `anchor-ambiguous` from `stale-revision`. O(1). */
export function hostRefusalCode(code) {
  if (code === "capability-withheld") return "capability-withheld";
  if (code === "invalid-argument" || code === "unknown-edit" || code === "over-limit") return "bad-request";
  if (code === "no-preview" || code === "no-document" || code === "ai.preview-gone") return "unavailable";
  return "engine-refused";
}

/** The catalogue key for the sentence the panel shows for a refusal code. O(1). */
export function refusalKey(code) {
  if (code === "capability-withheld") return "ai.refusal.access";
  if (code === "document-protected" || /^(document|session)\./.test(code ?? "")) return "ai.refusal.protected";
  if (code === "anchor-not-found" || code === "anchor-ambiguous") return "ai.refusal.anchor";
  if (code === "stale-revision" || code === "ai.preview-stale") return "ai.refusal.stale";
  if (code === "ai.preview-gone") return "ai.refusal.gone";
  return "ai.refusal.generic";
}

/**
 * One redline block as segments `{text, mark}`, `mark` being `"ins"`, `"del"` or
 * `null`. A suggestion's runs already carry their marks; a direct edit's do not,
 * so its paragraph is shown as what it was, struck, then what it becomes —
 * honest about the change without inventing a diff the engine did not make.
 *
 * Pure. Complexity: O(runs).
 */
export function redlineSegments(block) {
  const runs = Array.isArray(block?.afterRuns) ? block.afterRuns : [];
  const marked = runs.some((run) => (run.marks ?? []).some((mark) => mark === "inserted" || mark === "deleted"));
  if (marked || block?.before === null || block?.before === undefined) {
    return runs.map((run) => {
      const marks = run.marks ?? [];
      const mark = marks.includes("deleted") ? "del" : marks.includes("inserted") || block?.before == null ? "ins" : null;
      return { text: String(run.text ?? ""), mark };
    });
  }
  const before = String(block.before ?? "");
  const after = block.after === null || block.after === undefined ? "" : String(block.after);
  if (before === after) return [{ text: after, mark: null }];
  const out = [];
  if (before) out.push({ text: before, mark: "del" });
  if (after) out.push({ text: after, mark: "ins" });
  return out;
}

/** Suggestion dates are the HOST's clock, in Word's ISO form, to the second. */
function isoNow() {
  return new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
}

/** A frozen refusal value, the shape `execute`'s refusals have. */
function refused(method, code, message, extra = {}) {
  return Object.freeze({
    ok: false,
    refusal: Object.freeze({
      code: hostRefusalCode(code),
      command: `ai.${method}`,
      requires: null,
      message,
      aiCode: code,
      ...extra,
    }),
  });
}

/**
 * Installs the assistant on the editor context and returns the host-facing API.
 *
 * @param {object} ed the editor context (`docs/177`).
 * @param {{onDecision?: (detail: object) => void}} [hooks] told when the person
 *   accepts or discards, for the host contract's `ai-decision` event.
 */
export function createAssistant(ed, { onDecision = () => {} } = {}) {
  /** The open preview: `{previewId, preview}`, or null. One at a time. */
  let open = null;
  /** The panel, built on first use: chrome built at boot must not call `t()`
   *  before the catalogue lands (`chrome-raw-keys.spec.mjs`). */
  let panel = null;
  /** The WebMCP status, until `startWebMcp` below replaces it. */
  let webmcpStatus = () => null;

  function access() {
    return aiAccessFor(ed.HOST_CAPS, { direct: String(hostConfig().ai ?? "").toLowerCase() === "direct" });
  }

  function decide(previewId, decision, reason = null) {
    try {
      onDecision({ previewId, decision, revision: ed.currentRevision ?? null, reason });
    } catch (err) {
      console.warn("opendoc: an ai-decision listener threw", err);
    }
  }

  // ---- The panel ------------------------------------------------------------

  function element(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function buildPanel() {
    const root = element("section", "ai-preview");
    root.id = "aiPreviewPanel";
    root.setAttribute("role", "dialog");
    root.setAttribute("aria-labelledby", "aiPreviewTitle");
    root.setAttribute("aria-describedby", "aiPreviewPrivate aiPreviewMode");
    root.hidden = true;
    const title = element("h2", "ai-preview-title");
    title.id = "aiPreviewTitle";
    const privacy = element("p", "ai-preview-private");
    privacy.id = "aiPreviewPrivate";
    const mode = element("p", "ai-preview-mode");
    mode.id = "aiPreviewMode";
    const blocks = element("ol", "ai-preview-blocks");
    const refusals = element("ul", "ai-preview-refusals");
    const actions = element("div", "ai-preview-actions");
    const discard = element("button", "dialog-button ai-preview-discard");
    discard.type = "button";
    discard.setAttribute("aria-keyshortcuts", "Escape");
    const accept = element("button", "dialog-button dialog-button-primary ai-preview-accept");
    accept.type = "button";
    accept.setAttribute("aria-keyshortcuts", "Control+Enter Meta+Enter");
    const blocked = element("p", "ai-preview-blocked");
    blocked.id = "aiPreviewBlocked";
    actions.append(discard, accept);
    root.append(title, privacy, mode, blocks, refusals, blocked, actions);
    discard.addEventListener("click", () => void assistant.discard());
    accept.addEventListener("click", () => void assistant.accept());
    root.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        void assistant.discard();
      } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey) && !accept.disabled) {
        event.preventDefault();
        event.stopPropagation();
        void assistant.accept();
      }
    });
    document.body.append(root);
    return { root, title, privacy, mode, blocks, refusals, blocked, discard, accept };
  }

  function renderBlock(block) {
    const item = element("li", "ai-preview-block");
    if (block.before === null || block.before === undefined) {
      item.append(element("span", "ai-preview-new", t("ai.panel.newParagraph")));
    }
    const paper = element("p", "ai-preview-text");
    for (const segment of redlineSegments(block)) {
      if (!segment.text) continue;
      if (segment.mark === null) {
        paper.append(document.createTextNode(segment.text));
        continue;
      }
      // `<ins>`/`<del>` carry the meaning; the hidden word says it aloud, because
      // screen readers do not reliably announce either element.
      const node = element(segment.mark);
      node.append(
        element("span", "sr-only", `${t(segment.mark === "ins" ? "ai.panel.inserted" : "ai.panel.deleted")} `),
        document.createTextNode(segment.text),
      );
      paper.append(node);
    }
    item.append(paper);
    return item;
  }

  function showPanel(preview) {
    panel ??= buildPanel();
    const { root, title, privacy, mode, blocks, refusals, blocked, discard, accept } = panel;
    title.textContent = t("ai.panel.title");
    privacy.textContent = t("ai.panel.private");
    mode.textContent = t(preview.mode === "direct" ? "ai.panel.direct" : "ai.panel.suggest", {
      author: String(preview.author ?? ""),
    });
    blocks.setAttribute("aria-label", t("ai.panel.changes"));
    blocks.replaceChildren(...(preview.redline ?? []).map(renderBlock));
    const reasons = [];
    for (const edit of preview.edits ?? []) {
      if (edit.refusal) reasons.push(t(refusalKey(edit.refusal.code)));
    }
    if (preview.editorRefusal) reasons.push(t(refusalKey(preview.editorRefusal.details?.engineCode ?? preview.editorRefusal.code)));
    const unique = [...new Set(reasons)];
    refusals.replaceChildren(...unique.map((reason) => element("li", "", reason)));
    refusals.hidden = unique.length === 0;
    discard.textContent = t("ai.panel.discard");
    accept.textContent = t("ai.panel.accept");
    // Never a dead control (SKILL §10): Accept is disabled WITH its reason, in
    // text the reader sees and the button's description says.
    accept.disabled = !preview.wouldApply;
    blocked.textContent = preview.wouldApply ? "" : t("ai.panel.cannotApply");
    blocked.hidden = !!preview.wouldApply;
    if (preview.wouldApply) {
      accept.removeAttribute("aria-describedby");
      accept.removeAttribute("title");
    } else {
      accept.setAttribute("aria-describedby", "aiPreviewBlocked");
      accept.title = t("ai.panel.cannotApply");
    }
    root.hidden = false;
    (accept.disabled ? discard : accept).focus({ preventScroll: true });
  }

  function hidePanel() {
    if (!panel || panel.root.hidden) return;
    const hadFocus = panel.root.contains(document.activeElement);
    panel.root.hidden = true;
    if (hadFocus) ed.focusEditorSurface();
  }

  // ---- The API ----------------------------------------------------------------

  /** Withdraws an open card without a status line: the host replaced it. */
  function withdraw() {
    if (!open) return;
    const { previewId } = open;
    open = null;
    if (previewId) {
      ed.doc?.aiDiscard(previewId);
      decide(previewId, "discarded", "replaced");
    }
    hidePanel();
  }

  function noDocument(method) {
    return refused(method, "no-document", t("ai.refusal.noDocument"));
  }

  /** Parses the engine's `{ok, value}` / `{ok: false, error}` answer. */
  function engineAnswer(method, json) {
    const answer = JSON.parse(json);
    if (answer.ok) return answer;
    const error = answer.error ?? {};
    return refused(method, error.code ?? "engine-refused", t(refusalKey(error.code)), {
      detail: error.message ?? "",
      details: error.details ?? null,
    });
  }

  function compile(method, plan, addIn = "") {
    const { access: level, direct } = access();
    if (level === "none") return refused(method, "capability-withheld", t("ai.refusal.access"));
    let text;
    try {
      text = typeof plan === "string" ? plan : JSON.stringify(plan);
    } catch {
      return refused(method, "invalid-argument", t("ai.refusal.generic"));
    }
    // An add-in's proposal is signed "<name> (add-in)" by the engine
    // (`aiPreviewFrom`, `docs/183` §17); the assistant's own is "Casual AI".
    const answer = addIn && typeof ed.doc.aiPreviewFrom === "function"
      ? ed.doc.aiPreviewFrom(text, level, direct, isoNow(), addIn)
      : ed.doc.aiPreview(text, level, direct, isoNow());
    return engineAnswer(method, answer);
  }

  /** Commits the preview `previewId`; the engine refuses anything stale. */
  async function commit(method, previewId) {
    let result;
    try {
      result = ed.doc.aiAccept(previewId);
    } catch (err) {
      const code = err?.code ?? "engine-refused";
      return refused(method, code, t(refusalKey(code)), { detail: String(err?.message ?? err) });
    }
    await ed.applyEditResult(result, { keepView: true, keepSelection: true });
    return Object.freeze({ ok: true, previewId, revision: ed.currentRevision ?? null });
  }

  const assistant = {
    /** What this mount lets an assistant do, and whether its tools are offered to
     *  a browser agent (`webmcp`, `ai_webmcp.mjs`). O(capabilities). */
    access() {
      return Object.freeze({ ok: true, ...access(), webmcp: webmcpStatus() });
    },

    /** The assistant's reads, over the live document. O(document). */
    read(request = {}) {
      if (!ed.doc) return noDocument("read");
      const { access: level } = access();
      if (level === "none") return refused("read", "capability-withheld", t("ai.refusal.access"));
      return engineAnswer("read", ed.doc.aiRead(JSON.stringify(request ?? {}), level));
    },

    /** Compiles `plan` and shows it to the person, privately. Resolves at once
     *  with the preview; the decision arrives as the `ai-decision` event (and as
     *  `accept`/`discard`'s own results). O(document) for the compile. */
    preview(plan) {
      return assistant.previewFrom(plan, "");
    },

    /** `preview`, signed by an add-in (`plugin_host.mjs`). Not on the host
     *  contract: `AI_METHODS` is the host's list, and a host signs as itself. */
    previewFrom(plan, addIn) {
      if (!ed.doc) return noDocument("preview");
      // One proposal at a time, as Docs and Word show one card: a new one
      // replaces the last, which is reported discarded.
      withdraw();
      const compiled = compile("preview", plan, String(addIn ?? ""));
      if (!compiled.ok) {
        ed.setStatus(compiled.refusal.message, "error");
        return compiled;
      }
      const preview = compiled.value;
      open = { previewId: preview.previewId ?? null, preview };
      showPanel(preview);
      return Object.freeze({ ok: true, previewId: open.previewId, wouldApply: !!preview.wouldApply, preview });
    },

    /** Compiles and commits at once, with no panel: `opendoc.ai`'s `apply`. */
    async apply(plan) {
      if (!ed.doc) return noDocument("apply");
      // The engine holds one compiled plan at a time, so an open card is
      // withdrawn first rather than left offering an Accept that would fail.
      withdraw();
      const compiled = compile("apply", plan);
      if (!compiled.ok) return compiled;
      const preview = compiled.value;
      if (!preview.wouldApply || !preview.previewId) {
        const first = (preview.edits ?? []).find((edit) => edit.refusal)?.refusal ?? preview.editorRefusal ?? {};
        const code = first.details?.engineCode ?? first.code ?? "engine-refused";
        return refused("apply", code, t(refusalKey(code)), { preview });
      }
      const committed = await commit("apply", preview.previewId);
      return committed.ok ? Object.freeze({ ...committed, preview }) : committed;
    },

    /** The person's Accept. */
    async accept() {
      if (!open?.previewId) {
        return refused("accept", "no-preview", t("ai.refusal.gone"));
      }
      const { previewId } = open;
      open = null;
      hidePanel();
      const committed = await commit("accept", previewId);
      if (committed.ok) {
        ed.setStatus(t("ai.accepted"));
        decide(previewId, "accepted");
      } else {
        ed.setStatus(committed.refusal.message, "error");
        decide(previewId, "refused", committed.refusal.aiCode);
      }
      return committed;
    },

    /** The person's Discard. Leaves the document and the undo stack as they were. */
    discard() {
      if (!open) return refused("discard", "no-preview", t("ai.refusal.gone"));
      const { previewId } = open;
      open = null;
      if (previewId) ed.doc?.aiDiscard(previewId);
      hidePanel();
      ed.setStatus(t("ai.discarded"));
      decide(previewId, "discarded");
      return Object.freeze({ ok: true, previewId });
    },
  };
  // WebMCP (`docs/178` §9 item 2): only when the host asked, and only over the
  // methods above, so a browser agent reaches nothing this object does not offer.
  webmcpStatus = startWebMcp(assistant, hostConfig());
  // The AI provider service and the plugin platform (`docs/183`), started here for
  // WebMCP's reason: `main.js` carries no line for it. Nothing is requested at boot.
  startAiPlatform(ed, { assistant });
  return assistant;
}
