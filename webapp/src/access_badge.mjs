// SPDX-License-Identifier: Apache-2.0
// What THIS reader may do with THIS document, said out loud, from first paint.
//
// ---- THE DEFECT THIS EXISTS TO CLOSE --------------------------------------
//
// A protected or read-only document opened in the full editing chrome and the
// reader learnt the truth from a TOAST, after typing. Measured rather than
// recalled: `WasmDocument::editingUnavailableReason` answers only the windowed
// case (`if self.layout.is_windowed() { … } else { String::new() }`), and
// `document_protection.mjs`'s own header records the deliberate decision that a
// `trackedChanges`-protected document "does not force the editor into Suggesting
// mode on open, as Word does". Both of those are defensible. Together they meant
// nothing on screen said what the reader could do until they tried something and
// were refused.
//
// The two review-mode banners are not that answer either, and the reason is the
// shape of the problem rather than their content: `createReviewBanners` shows a
// banner only in Suggesting and Viewing, so the mode that most needs to be
// distinguished from a lie — "Editing, but the document restricts you" — is the
// one with nothing on screen.
//
// ---- WHY A STATUS INDICATOR AND NOT A DIALOG OR A TOAST -------------------
//
// Because the question is PERSISTENT and the answer never changes on its own. A
// toast is for an event; a dialog is for a decision; a reader's own access is
// neither — it is state, and Word and Google Docs both put document state in the
// status bar. This is that control, in that place.
//
// It is also, deliberately, **not a control**. It carries no `data-command` and
// nothing happens when it is clicked. `SKILL` §10 forbids a dead control, and the
// way not to ship one is not to ship a control: a viewer must see what they may
// do and must NOT be offered a rights surface (the owner's decision — see
// `session_rights.mjs`), and an indicator satisfies both at once where a button
// would have to satisfy one by breaking the other.
//
// ---- WHERE IT LIVES, AND WHY NOT THE OTHER HALF OF THE FOOTER -------------
//
// `.foot-right`, measured rather than assumed: `style.css`'s narrow breakpoint
// sets `.footer .foot-left { display: none }` and gives `.foot-right` the full
// width. An indicator in the left half would be invisible on a phone, which is
// the width at which a reader is least likely to have discovered their own
// permissions by other means. It also carries no `data-status-priority`, because
// every priority is shed at that same breakpoint.
//
// ---- THE RULE: THE NARROWEST TRUE ANSWER ----------------------------------
//
// Three authorities can each restrict this reader, and they are the three the
// engine already keeps apart:
//
//   * the ENGINE's own refusal (`editingUnavailableReason`) — a document it
//     cannot edit at all;
//   * the CONTAINER's grant (`capabilities.mjs`) — the host's embed role,
//     resolved from the URL before first paint. "For SDK or single user, the
//     role is pre-decided while loading the file", which is this one;
//   * the PARTICIPANT's grant (`session_access.mjs`) — the room's, on the
//     `Welcome`;
//   * and the DOCUMENT's own policy (`w:documentProtection`), which asks the
//     same of everyone.
//
// The badge reports the **narrowest** of them and names which one decided, and
// those are two separate fields rather than one sentence, because they answer two
// questions a reader asks in sequence: what can I do, and who said so. Naming the
// wrong authority is the exact failure `AccessRefusal`'s `session.*` /
// `document.*` split exists to prevent one layer down — "this document is
// protected" sends a read-only guest to look at the wrong thing.
//
// Pure, apart from one binder at the bottom. Every sentence is a catalogue key.

import { roleKey } from "./session_rights.mjs";

/** Where a restriction came from → catalogue key for the badge's second half.
 *
 *  Ordered as [`accessState`] considers them: narrowest authority first. */
export const ACCESS_SOURCE_KEYS = Object.freeze({
  /** The engine will not edit this document at all. */
  engine: "access.source.engine",
  /** `w:documentProtection` — a property of the FILE, asked of everyone. */
  document: "access.source.document",
  /** The room's grant, verified by the relay. */
  shared: "access.source.shared",
  /** The host's embed grant, decided before first paint. */
  host: "access.source.host",
  /** No grant, no room, no restriction: the reader is the only authority. */
  local: "access.source.local",
});

/** The level the badge names when no role matches, because the question is not
 *  "which role" but "what can I do".
 *
 *  Kept apart from `session_rights.mjs`'s role vocabulary on purpose. A ROLE is
 *  something somebody was given in a room; these are what a reader can do with a
 *  document that has no room, and calling a standalone reader "Owner" would
 *  invent a grant nobody issued. */
export const ACCESS_LEVEL_KEYS = Object.freeze({
  full: "access.level.full",
  suggest: "access.level.suggest",
  comment: "access.level.comment",
  read: "access.level.read",
  /** `w:formatting` with no editing restriction — Word's pure formatting
   *  restriction, which is now enforced at the operation
   *  (`casual_doc_edit::protection`). It is the one state where a reader may write
   *  whatever they like and may not change how it looks, so neither `full` nor
   *  `read` is true of them: `full` would be the lie this badge exists to stop,
   *  and `read` would stop them typing in a document they can type in. The two
   *  axes are independent in the file and the badge has to be able to say so. */
  noFormatting: "access.level.noFormatting",
});

/** The container role, as `capabilities.mjs` grades it, reduced to a level — or
 *  `null` when no container grant was supplied at all.
 *
 *  Reads the capability set rather than the role NAME, because a host may
 *  withhold a capability from a named preset (`parseWithheld`) and the name would
 *  then overstate what is left.
 *
 *  **`null` for an absent grant, and NOT "read only", which is the fail-closed
 *  answer and is wrong here.** Fail-closed is the rule for a boundary, and this is
 *  not one — it is a statement, and the statement would be false: a reader on a
 *  page that supplied no container grant is not restricted by one, so reporting
 *  "Read only — set when this document was opened" names an authority that did not
 *  speak. That is the exact failure the `session.*` / `document.*` split exists to
 *  prevent, and it is worse than the overstatement it would be avoiding, because a
 *  reader who is told they may not edit does not try.
 *
 *  An object that is PRESENT and withholds `edit` is a different fact and does
 *  grade to a level — `{has: () => false}` is a host saying no, and it is reported.
 *
 *  O(1). */
function containerLevel(capabilities) {
  if (typeof capabilities?.has !== "function") return null;
  if (capabilities.has("edit") === true) return "full";
  if (capabilities.has("comment") === true) return "comment";
  return "read";
}

/**
 * What this reader may do, and which authority decided it.
 *
 * `levelKey` is always a key in [`ACCESS_LEVEL_KEYS`] or a role key from
 * `session_rights.mjs`; `sourceKey` is always a key in [`ACCESS_SOURCE_KEYS`];
 * `detail` is the engine's own sentence when it has one and `""` otherwise. All
 * three fields always present, so a caller destructuring cannot end up with
 * `undefined` where a sentence belongs.
 *
 * # The order of the branches is the rule
 *
 * Narrowest authority first, and each branch returns — so a read-only guest in a
 * protected document is told about their own access rather than about the file,
 * which is the one of the two they can do something about (ask somebody) and the
 * one that is true of them specifically.
 *
 * Complexity: O(1).
 *
 * @param {{
 *   readOnlyReason?: string,
 *   protection?: {active?: boolean, value?: string}|null,
 *   grant?: {shared?: boolean, names?: readonly string[]}|null,
 *   capabilities?: {has?: (name: string) => boolean}|null,
 * }} authorities
 * @returns {{levelKey: string, sourceKey: string, detail: string}}
 */
export function accessState({
  readOnlyReason = "",
  protection = null,
  grant = null,
  capabilities = null,
} = {}) {
  const frozen = (levelKey, sourceKey, detail = "") =>
    Object.freeze({ levelKey, sourceKey, detail });

  // 1. The engine refuses outright. Its sentence is more specific than anything
  //    this module could say, so it is carried verbatim rather than replaced —
  //    the rule `edit_errors.mjs` was written to hold.
  const refusal = String(readOnlyReason ?? "");
  if (refusal !== "") return frozen(ACCESS_LEVEL_KEYS.read, ACCESS_SOURCE_KEYS.engine, refusal);

  // 2. The room, when there is one. A participant grant is a ROLE — somebody
  //    issued it — so the badge names the role rather than a level, and
  //    `rights.role.custom` is a real answer for a grant the host narrowed by
  //    hand.
  if (grant?.shared === true) {
    return frozen(roleKey(grant.names ?? []), ACCESS_SOURCE_KEYS.shared);
  }

  // 3. The document's own policy, which is the case the toast used to carry. It
  //    is checked AFTER the room, because in a room the participant's own grant
  //    is the more specific statement about this reader; it is checked BEFORE the
  //    container because a protected file restricts a reader whose host grant
  //    says `edit`, and that is exactly the combination that opened in the full
  //    editing chrome with nothing on screen.
  if (protection?.active === true) {
    const level = String(protection.value ?? "");
    if (level === "comments") {
      return frozen(ACCESS_LEVEL_KEYS.comment, ACCESS_SOURCE_KEYS.document);
    }
    if (level === "trackedChanges") {
      return frozen(ACCESS_LEVEL_KEYS.suggest, ACCESS_SOURCE_KEYS.document);
    }
    // The SECOND axis, and it is tested before the fall-through rather than after
    // it, because the fall-through says `read` and a formatting-restricted reader
    // is not a reader — they may type. `w:documentProtection` carries two
    // independent restrictions (`w:edit` and `w:formatting`) and a document whose
    // editing axis restricts NOTHING while its formatting axis restricts
    // everything is a state Word writes and this engine now enforces. Reported
    // here rather than left to the `local` branch, which would have named no
    // authority for a refusal the file is about to hand the reader.
    if (level === "off") {
      return frozen(ACCESS_LEVEL_KEYS.noFormatting, ACCESS_SOURCE_KEYS.document);
    }
    // `readOnly` and `forms` both leave the body alone; `forms` admits a field
    // and the badge does not promise one, because "you may type in the form
    // fields" is a sentence about where rather than about what, and the reader
    // finds the fields by looking. Both absorb the formatting axis into their own
    // sentence: a reader told the document is read-only does not also need to be
    // told its formatting is.
    return frozen(ACCESS_LEVEL_KEYS.read, ACCESS_SOURCE_KEYS.document);
  }

  // 4. The container's grant — the host's, from the URL, before first paint.
  //    `null` is "no container said anything", which falls through to (5); `full`
  //    is a container that said yes, which is the same outcome by a different
  //    route and falls through for the same reason — naming the host as the
  //    authority for an unrestricted reader attributes a decision nobody made.
  const level = containerLevel(capabilities);
  if (level !== null && level !== "full") {
    return frozen(ACCESS_LEVEL_KEYS[level], ACCESS_SOURCE_KEYS.host);
  }

  // 5. Nothing restricts anything. `Capabilities::local` says the reader on their
  //    own machine is the only authority, and the badge says so rather than going
  //    blank — an empty indicator is indistinguishable from a broken one, and the
  //    owner asked for this state to be LEGIBLE rather than invisible.
  return frozen(ACCESS_LEVEL_KEYS.full, ACCESS_SOURCE_KEYS.local);
}

/**
 * Binds the badge over the markup in `editor.html`, and returns one reflector.
 *
 * A closure over the two elements rather than assignments at the call site, for
 * `createReviewBanners`'s reason: the thing worth getting right is not the text
 * assignment, it is that the sentence a reader sees and the sentence assistive
 * technology reads are composed in ONE place from ONE state.
 *
 * The returned function takes the same authorities [`accessState`] does, so a
 * caller has one thing to call and no state of its own to keep.
 *
 * Returns an inert reflector when the markup is absent, so a host embedding a
 * cut-down shell gets a no-op rather than a throw — `document_protection.mjs`'s
 * pattern.
 *
 * Complexity: O(1) per call.
 *
 * @param {{badge: Element|null, level: Element|null, source: Element|null,
 *          t: (key: string) => string}} io
 * @returns {(authorities: object) => void}
 */
export function createAccessBadge({ badge, level, source, t }) {
  if (!badge || !level || !source) return () => {};
  return (authorities) => {
    const state = accessState(authorities);
    const levelText = t(state.levelKey);
    const sourceText = t(state.sourceKey);
    level.textContent = levelText;
    source.textContent = sourceText;
    // The two halves are two elements so CSS can shed the second one on a narrow
    // footer without the first losing its meaning, and ONE `title`/`aria-label`
    // carries the whole statement — including the engine's own sentence when it
    // has one, which is the detail a shed second half would otherwise take away.
    const whole = state.detail === "" ? `${levelText} — ${sourceText}` : state.detail;
    badge.setAttribute("title", whole);
    badge.setAttribute("aria-label", whole);
  };
}
