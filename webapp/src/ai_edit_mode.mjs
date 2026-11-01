// SPDX-License-Identifier: Apache-2.0
// "AI edits as: Suggestions | Direct edits", and what the AI may ACTUALLY do here
// (`docs/183` §23.3).
//
// THE RULE: the effective mode is the MOST RESTRICTIVE of three authorities —
//
//   * the person's own setting (Settings ▸ AI provider, the panel's ⋯ menu):
//     Suggestions by default, Direct edits on request;
//   * the DOCUMENT's state: the editor in Suggesting mode, a `trackedChanges`
//     protection, or a participant grant that only allows suggesting forces
//     Suggestions; a `comments` protection allows comments only; `readOnly`
//     and `forms` protection, and Viewing mode, allow nothing but chat;
//   * the PERSON's role, from the grant `aiAccessFor` derives: a commenter's AI
//     comments only, a viewer's only chats. The AI never exceeds the human.
//
// Word's Copilot follows Track Changes and nothing else; Google Docs' Gemini
// inserts as the person. Neither caps the assistant by a protection or a role
// because neither has an assistant that edits on its own; this one does, so the
// cap is explicit, said to the model (`capabilityLine`), shown in the composer
// (`badge`) and ENFORCED in code: the co-editor refuses a turn the mode does not
// admit, and every write still meets the engine's own protection check.
//
// Pure apart from the two storage helpers, which never throw.

/** Where the person's choice is remembered, per device. Not a document key. */
export const AI_EDIT_MODE_KEY = "opendoc.ai.editAs";

/** The two settings, default first. */
export const AI_EDIT_SETTINGS = Object.freeze(["suggest", "direct"]);

/** The four effective modes, most permissive first. */
export const AI_MODES = Object.freeze(["direct", "suggest", "comment", "chat"]);

let memory = null;

/** The stored setting: "suggest" (the default) or "direct". Never throws. */
export function loadAiEditSetting(view = globalThis) {
  try {
    const raw = view.localStorage?.getItem(AI_EDIT_MODE_KEY);
    if (raw === "direct" || raw === "suggest") return raw;
  } catch {
    /* storage refused: the page's copy, else the default */
  }
  return memory ?? "suggest";
}

/** Remembers the setting. Returns "device", or "page" when storage refused. */
export function saveAiEditSetting(value, view = globalThis) {
  const clean = value === "direct" ? "direct" : "suggest";
  memory = clean;
  try {
    view.localStorage.setItem(AI_EDIT_MODE_KEY, clean);
    return "device";
  } catch {
    return "page";
  }
}

/** The weaker of two modes. Pure. */
export function narrower(a, b) {
  return AI_MODES[Math.max(AI_MODES.indexOf(a), AI_MODES.indexOf(b))] ?? "chat";
}

/**
 * The effective access. Every input is plain data, so the whole matrix is
 * answerable in node (`tests/ai_edit_mode.test.mjs`).
 *
 * @param {object} inputs
 * @param {string} inputs.access the grant's level (`aiAccessFor`): none | read | comment | suggest | review
 * @param {string} [inputs.setting] "suggest" | "direct"
 * @param {string} [inputs.reviewMode] the editor's mode: editing | suggesting | viewing
 * @param {{active?: boolean, edit?: string|null}} [inputs.protection] `readProtection`'s answer
 * @param {boolean} [inputs.suggestOnly] a participant grant that allows Suggesting but not Editing
 * @param {boolean} [inputs.hostSuggestOnly] the host said `?ai=suggest`
 * @returns {{mode: string, setting: string, badge: string, direct: {allowed: boolean, reason: string|null}, reason: string|null, protectedLevel: string|null}}
 *   `reason` is the catalogue key naming the authority that narrowed the
 *   person's setting (null when nothing did); `direct.reason` is why Direct
 *   edits cannot be chosen, for the greyed-out option.
 */
export function effectiveAiAccess({ access, setting = "suggest", reviewMode = "editing", protection = null, suggestOnly = false, hostSuggestOnly = false } = {}) {
  const wanted = setting === "direct" ? "direct" : "suggest";
  let ceiling = "direct";
  let because = null;
  const cap = (mode, key) => {
    const next = narrower(ceiling, mode);
    if (next !== ceiling) {
      ceiling = next;
      because = key;
    }
  };
  // The person's role first: the AI never exceeds the human.
  if (access === "none" || access === "read" || !access) cap("chat", "ai.access.reason.viewer");
  else if (access === "comment") cap("comment", "ai.access.reason.commenter");
  // The document's own policy, asked of everyone.
  const level = protection?.active ? String(protection.edit ?? "readOnly") : null;
  if (level === "readOnly" || level === "forms") cap("chat", "ai.access.reason.protected");
  else if (level === "comments") cap("comment", "ai.access.reason.protected");
  else if (level === "trackedChanges") cap("suggest", "ai.access.reason.protected");
  else if (level && level !== "none") cap("chat", "ai.access.reason.protected");
  // The editor's mode and the room's grant.
  if (reviewMode === "viewing") cap("chat", "ai.access.reason.viewing");
  else if (reviewMode === "suggesting") cap("suggest", "ai.access.reason.suggesting");
  if (suggestOnly) cap("suggest", "ai.access.reason.suggestOnly");
  if (hostSuggestOnly) cap("suggest", "ai.access.reason.host");
  const mode = narrower(wanted, ceiling);
  const badge = mode === "direct" ? "editing" : mode === "suggest" ? "suggesting" : mode === "comment" ? "commenting" : "readonly";
  return Object.freeze({
    mode,
    setting: wanted,
    badge,
    direct: Object.freeze({ allowed: ceiling === "direct", reason: ceiling === "direct" ? null : because }),
    reason: mode === wanted ? null : because,
    protectedLevel: level,
  });
}

/**
 * The capability line the model is told, in fixed English (what a model reads
 * must not change with the reader's locale, `docs/181`). It says what the AI may
 * do, what it may not, and what to do when asked for more. The code, not this
 * sentence, is the authority: the co-editor refuses what the mode does not
 * admit whatever the model says. Pure.
 */
export function capabilityLine(effective) {
  const mode = effective?.mode ?? "chat";
  const protectedNote = effective?.protectedLevel
    ? ` The document is protected (${effective.protectedLevel}): its owner restricted editing for everyone.`
    : "";
  const ask = " When the user asks for more than this, say so plainly in one sentence and offer what you can do instead.";
  if (mode === "direct") {
    return "Your access: you may edit this document directly; your edits appear as ordinary changes written by Casual AI." + protectedNote;
  }
  if (mode === "suggest") {
    return "Your access: you may only SUGGEST changes. Everything you write becomes a tracked suggestion by Casual AI that the user accepts or rejects; you cannot make direct edits." + protectedNote + ask;
  }
  if (mode === "comment") {
    return "Your access: you may read the document and add comments, but you cannot change its text, not even as a suggestion." + protectedNote + ask;
  }
  return "Your access: read-only. You may read the document and answer, but you cannot change it or comment on it. If the user asks for a change, answer with the text they could use, so they can copy it." + protectedNote + ask;
}

/** Whether an edit KIND is admitted by an effective mode: the code's authority.
 *  `text` covers insert, replace, paragraphs and tables; `comment` a comment. */
export function admits(mode, kind) {
  if (kind === "comment") return mode === "direct" || mode === "suggest" || mode === "comment";
  return mode === "direct" || mode === "suggest";
}
