// SPDX-License-Identifier: Apache-2.0
// What THIS PARTICIPANT may do in a shared room, as the chrome has to know it.
//
// `152` §10 Q4/Q5 and ADR-060 built the engine half: `casual_doc_edit::access`
// enforces a participant's grant at the operation, `AccessRefusal` carries one
// `session.*` routing code per class, and `casual-doc-wasm` exposes
// `adoptParticipantCapabilities` / `participantCapabilities` so a host can hand
// over a verified grant and ask what it bought.
//
// NONE OF IT WAS REACHABLE. Before this module, `adoptParticipantCapabilities`,
// `participantCapabilities` and `adoptParticipantIdentity` each appeared exactly
// once in the whole tree — at their own definition — with zero callers in
// `webapp/src`; none of the five `session.*` codes was routed anywhere in the
// chrome; and `review.restrictEditing` was declared `requires: "doc"` with no
// reference to `manageProtection`, so it stayed enabled for a participant who
// may not change protection and the refusal arrived only after they pressed it.
// That is `SKILL` §9 rule 4 — built is not reachable — and §10's "never a dead
// control; say why it refuses" in the same place.
//
// ---- THE ESTABLISHED PATTERN, NAMED BEFORE ANY CODE ------------------------
//
// A **least-privilege capability grant**, composed by **intersection**. The host
// signs a token binding a subject to a document and a capability set, a boundary
// verifies it, and every layer below enforces what came out of verification —
// OAuth2 scopes, WOPI's access token, ONLYOFFICE's JWT. That is exactly what
// `access.rs` says, and this file is one more layer of the same thing rather than
// a new idea: `Capabilities::narrowed_to` is the engine's intersection,
// `capabilities.mjs` is the container's, and this is the room's.
//
// TWO AUTHORITIES, NEVER COLLAPSED. `capabilities.mjs` answers "what may this
// CONTAINER do" — a host's embed grant, resolved from the URL before first paint.
// This answers "what may THIS PARTICIPANT do in this room". They are different
// questions with different sources and different sentences, exactly as
// `access.rs` keeps `w:documentProtection` (what the document asks of everyone)
// apart from a participant's access level (what you may do). A control asks both
// and shows the reason belonging to whichever one refused.
//
// ---- THE TRAP THIS FILE IS SHAPED TO AVOID ---------------------------------
//
// `ObjectCapabilities` in `casual-doc-wasm` carries bare booleans with no reason
// string, and `main.js` gates on bare truthiness — which is how the inline-picture
// dead gesture shipped: the gesture was simply unavailable and nothing said why.
// So there is deliberately **no predicate** exported here. [`participantGrant`]
// returns an object whose only question-answering method is
// [`decide`](#decide) and it returns `{allowed, code, key}` — a withheld
// capability carries its routing code from the moment it is withheld, not from a
// later lookup somebody may forget. A caller cannot accidentally write
// `if (grant.has("review"))`, because there is nothing to call.
//
// ---- ONE TABLE FOR BOTH HALVES, WHICH IS THE POINT -------------------------
//
// The sentence shown on a control that is disabled BEFORE the gesture and the
// sentence shown when the engine refuses AFTER one are **the same string, from
// the same table**, keyed by the same `session.*` code. Two tables would drift,
// and a reader told two different things about one permission learns that neither
// is trustworthy. [`REFUSAL_KEYS`] is that table, and `session_access.test.mjs`
// derives its key set from `crates/casual-doc-edit/src/access.rs` and
// `docs/20-ERROR-CODE-REGISTRY.md` rather than from a list kept here by hand, so
// a code added in the engine with no sentence in the chrome fails the build.
//
// ---- WHAT A CLIENT MAY ASSUME, AND WHAT IT MAY NOT -------------------------
//
// That the relay will refuse what this refuses. **Not** that this is the
// authority: the relay keeps its own copy of the verified grant and judges every
// submission against it, so a chrome defeated from devtools still cannot write
// above its level. What this buys is a control that disables itself with a reason
// instead of offering a gesture the network will reject.
//
// Which is also why reading the grant off the URL is safe rather than forgeable.
// `adoptParticipantCapabilities` **intersects**, so any value this side supplies
// can only ever NARROW what the engine already holds — a visitor who edits the
// query string can take access away from themselves and cannot add any. The URL
// is the channel `capabilities.mjs` already uses for a host's configuration, for
// the reason given there: it is the only one decided before the frame's first
// navigation.
//
// Pure: no DOM, no engine, no strings of its own. Every sentence is a catalogue
// key the caller resolves, so this module adds no string debt (`docs/124`).

/** The capability names `WasmDocument::adoptParticipantCapabilities` accepts and
 *  `participantCapabilities` reports, sorted as the getter sorts them.
 *
 *  Kept in step with the facade by `session_access.test.mjs`, which reads the
 *  match arms out of `crates/casual-doc-wasm/src/lib.rs`: a sixth capability
 *  added to the engine is a capability this list has to learn, and the day it
 *  does not the build says so rather than a grant arriving narrowed by accident.
 *  (`review` was exactly that defect — the engine modelled it, the facade refused
 *  the name, and nothing noticed.) */
export const PARTICIPANT_CAPABILITIES = Object.freeze([
  "comment",
  "edit",
  "manageProtection",
  "review",
  "suggest",
]);

/** `AccessRefusal::reason`'s five `session.*` codes → catalogue key.
 *
 *  The engine writes an English fallback sentence beside each code; these are the
 *  translated ones, and they are what a reader actually sees in every locale. */
export const ACCESS_REFUSAL_KEYS = Object.freeze({
  "session.read-only-access": "session.readOnly",
  "session.comments-only-access": "session.commentsOnly",
  "session.suggestions-only-access": "session.suggestionsOnly",
  "session.review-only-access": "session.reviewOnly",
  "session.no-protection-change": "session.noProtectionChange",
});

/** `ProtectionRefusal::reason`'s codes, plus the forms-only one `casual-doc-wasm`
 *  raises → catalogue key.
 *
 *  THE OTHER AUTHORITY, and in the same table because it reaches the reader by
 *  the same route and has to be translated the same way. These are reachable on a
 *  document opened from a file with no room at all, which is why routing them is
 *  not collaboration work that can wait: every one of them was arriving in the
 *  engine's English in all nineteen locales.
 *
 *  `document.protection-unknown-level` is deliberately NOT here. Its sentence
 *  quotes the restriction token the file actually carried — "“scribble” isn't an
 *  editing restriction" — and a translated sentence with the token dropped would
 *  be less useful than the English one it replaced. A refusal that names the
 *  offending value stays as the engine wrote it until the routing seam can carry
 *  the value too, and `session_access.test.mjs` records the exclusion so it
 *  cannot become an oversight. */
export const PROTECTION_REFUSAL_KEYS = Object.freeze({
  "document.protected-read-only": "document.protectedReadOnly",
  "document.protected-comments-only": "document.protectedCommentsOnly",
  "document.protected-tracked-changes-only": "document.protectedTrackedChangesOnly",
  "document.protected-forms-only": "document.protectedFormsOnly",
});

/** The `ODC-7xxx` collaboration family (`docs/20`) → catalogue key.
 *
 *  Every row in the register, not the ones a lane happened to need: `docs/20`
 *  already pairs the family with `protocol::Refusal` in both directions, so
 *  routing the whole family is how "a refusal the reader can act on" stays true
 *  when the next variant lands. `ODC-7003` is deliberately vague in the register
 *  — detail is useful to an operator in a log and to an attacker in a response —
 *  and the sentence here is vague in the same way rather than inventing detail
 *  the engine refused to send. */
export const COLLABORATION_REFUSAL_KEYS = Object.freeze({
  "ODC-7001": "collab.conflict",
  "ODC-7002": "collab.protocolVersion",
  "ODC-7003": "collab.notAuthorised",
  "ODC-7004": "collab.readOnly",
  "ODC-7005": "collab.notSaving",
  "ODC-7006": "collab.tooFarBehind",
  "ODC-7007": "collab.malformed",
  "ODC-7008": "collab.idCollision",
  "ODC-7009": "collab.staleBase",
  "ODC-7010": "collab.roomFull",
});

/** The code a grant this side could not read reports.
 *
 *  Not an engine code: nothing in the engine produces it, because the engine is
 *  never handed the unreadable value — see [`participantGrant`]. It is in the
 *  same table because it reaches the reader by the same route and must be
 *  translated like any other. */
export const GRANT_UNREADABLE = "session.grant-unreadable";

/** Every refusal code the chrome can be asked to explain, in one table.
 *
 *  Derived from the three above rather than listed a fourth time. */
export const REFUSAL_KEYS = Object.freeze({
  ...ACCESS_REFUSAL_KEYS,
  ...PROTECTION_REFUSAL_KEYS,
  ...COLLABORATION_REFUSAL_KEYS,
  [GRANT_UNREADABLE]: "session.grantUnreadable",
});

/**
 * The catalogue key for a refusal code, or `null` for one this build cannot
 * route.
 *
 * `null` and not a fallback sentence: the caller already holds the engine's own
 * English, which is more specific than anything this module could invent, and
 * silently replacing a specific sentence with a generic one is the defect
 * `edit_errors.mjs` was written to stop.
 *
 * Complexity: O(1).
 *
 * @param {unknown} code
 * @returns {string|null}
 */
export function refusalKey(code) {
  if (typeof code !== "string" || code === "") return null;
  return Object.hasOwn(REFUSAL_KEYS, code) ? REFUSAL_KEYS[code] : null;
}

/**
 * Which `session.*` code explains a capability this participant was not granted.
 *
 * A MIRROR OF `access::refusal_for`, and deliberately the same rule: the reason
 * names what the participant MAY do, not which of fifty-eight operations their
 * gesture would have become. So the widest write class they hold is named first,
 * `manageProtection` is its own answer because the authority question is about
 * the policy rather than about content, and a participant holding nothing at all
 * is told they are read-only.
 *
 * `session_access.test.mjs` drives this against the Rust function's own test
 * table, because two implementations of one rule diverge.
 *
 * Complexity: O(1).
 *
 * @param {readonly string[]} held the capability names this participant holds
 * @param {string} wanted the capability a control needs
 * @returns {string} a code in [`ACCESS_REFUSAL_KEYS`]
 */
export function withheldCode(held, wanted) {
  const has = (name) => held.includes(name);
  if (wanted === "manageProtection") return "session.no-protection-change";
  if (has("suggest")) return "session.suggestions-only-access";
  if (has("review")) return "session.review-only-access";
  if (has("comment")) return "session.comments-only-access";
  return "session.read-only-access";
}

/** The frozen answer [`participantGrant`]'s `decide` returns. Always all three
 *  fields, so a caller destructuring it cannot end up with `undefined` where a
 *  reason belongs. */
const verdict = (allowed, code) =>
  Object.freeze({ allowed, code: code ?? null, key: code ? refusalKey(code) : null });

/** The grant a document with no room holds: everything, like `Capabilities::local`.
 *
 *  Standalone is not a weak session, it is the absence of one (`152` §2a), and the
 *  local reader is the only authority — so `decide` says yes to everything and
 *  nothing is disabled on this account. */
const STANDALONE = Object.freeze({
  shared: false,
  participant: null,
  names: Object.freeze([]),
  problem: null,
  decide: () => verdict(true, null),
});

/**
 * Reads a participant grant out of a host's inputs.
 *
 * `granted` is the capability list the HOST got back from verifying the room's
 * token — never something this editor decided. `participant` is the number the
 * room assigned, which is what `adoptParticipantIdentity` partitions the minting
 * space by (`152` §4.2).
 *
 * FAIL CLOSED, AND NEVER PASS THE ENGINE A NAME IT WILL REFUSE. An unreadable
 * grant — an unknown capability name, a participant number that is not a
 * non-negative integer — yields a VIEWER with `problem` set, rather than a
 * partial grant assembled from the names that happened to parse. The engine's own
 * reasoning applies here one layer up: "silently granting less is a bug that looks
 * like a working read-only mode", so the chrome narrows to the floor *and says so*.
 * Dropping the bad name and keeping the rest would also leave the two sides
 * disagreeing, because `adoptParticipantCapabilities` refuses the whole call.
 *
 * Absent inputs are the standalone mode, not an empty grant: a document opened
 * from a file has no room, nobody to issue a grant, and no control to disable.
 *
 * Complexity: O(names), once per open.
 *
 * @param {{granted?: string|readonly string[]|null, participant?: string|number|null}} [inputs]
 * @returns {{shared: boolean, participant: number|null, names: readonly string[],
 *            problem: string|null, decide: (capability: string) => {allowed: boolean, code: string|null, key: string|null}}}
 */
export function participantGrant(inputs = {}) {
  const raw = inputs?.granted ?? null;
  const asked = inputs?.participant ?? null;
  const number = parseParticipant(asked);
  // Standalone only when BOTH inputs are absent. Testing the PARSED number here
  // instead would read a malformed participant — `?participant=-1` — as "no room
  // at all" and hand the reader every capability, which is the inverse of failing
  // closed and the one mistake this function exists to avoid.
  if (raw === null && (asked === null || asked === "")) return STANDALONE;

  const listed = Array.isArray(raw) ? [...raw] : String(raw ?? "").split(",");
  const wanted = listed.map((name) => String(name).trim()).filter((name) => name !== "");
  const unknown = wanted.filter((name) => !PARTICIPANT_CAPABILITIES.includes(name));
  const unreadable = unknown.length > 0 || (asked !== null && asked !== "" && number === null);
  // Sorted and de-duplicated, so the list this hands the engine is exactly the
  // list the engine hands back and a host can compare the two.
  const names = Object.freeze(
    unreadable ? [] : PARTICIPANT_CAPABILITIES.filter((name) => wanted.includes(name)),
  );
  return Object.freeze({
    shared: true,
    participant: unreadable ? null : number,
    names,
    problem: unreadable ? GRANT_UNREADABLE : null,
    decide: (capability) =>
      names.includes(capability) ? verdict(true, null) : verdict(false, withheldCode(names, capability)),
  });
}

/** A participant number, or `null` for anything that is not one.
 *
 *  `null` rather than a default, because a wrong number is worse than none: it
 *  would move this replica's minting into a space another participant owns, which
 *  is the one failure `152` §4.2's partition exists to make impossible. */
function parseParticipant(value) {
  if (value === null || value === undefined || value === "") return null;
  const n = Number(value);
  return Number.isSafeInteger(n) && n >= 0 ? n : null;
}

/**
 * Whether this participant may enter a review MODE.
 *
 * The same shape as `capabilities.mjs`'s `allowsMode`, and the same reasoning:
 * Viewing is always allowed because a reader may always read, and a withheld mode
 * is DISABLED WITH A REASON rather than removed, so a reader can still see which
 * of three modes they are in.
 *
 * SUGGESTING IS ALLOWED BY `comment` AS WELL AS BY `suggest`, and that is a
 * decision rather than a slip. Suggesting is this editor's annotating mode —
 * `editingModeFor` already maps a container's `comment` grant onto it, which is
 * what the shipped `commentor` role is — so a comment-only participant placed in
 * Viewing would be unable to comment at all, because `blockMutationInViewing`
 * refuses every mutation including a comment. In Suggesting they can comment, and
 * a tracked body change is refused by the engine with
 * `session.comments-only-access`, routed through [`REFUSAL_KEYS`] to a sentence
 * naming what they MAY do. That is "say why it refuses" rather than a gesture
 * that does nothing, and it keeps one rule for the two grants instead of two.
 *
 * Complexity: O(names).
 *
 * @param {{names: readonly string[], shared: boolean}} grant
 * @param {string} mode
 * @returns {{allowed: boolean, code: string|null, key: string|null}}
 */
export function participantAllowsMode(grant, mode) {
  if (!grant?.shared) return verdict(true, null);
  if (mode === "editing") return grant.decide("edit");
  if (mode === "suggesting") {
    const suggest = grant.decide("suggest");
    return suggest.allowed ? suggest : grant.decide("comment");
  }
  return verdict(true, null);
}

/**
 * The strongest mode this participant's grant admits.
 *
 * The inverse of [`participantAllowsMode`], and the value a room should OPEN in:
 * a read-only guest arrives in Viewing rather than in an Editing chrome whose
 * every control refuses, exactly as a `readonly` container does.
 *
 * Complexity: O(names).
 *
 * @param {{names: readonly string[], shared: boolean}} grant
 * @returns {"editing"|"suggesting"|"viewing"}
 */
export function participantModeCeiling(grant) {
  if (!grant?.shared) return "editing";
  if (participantAllowsMode(grant, "editing").allowed) return "editing";
  return participantAllowsMode(grant, "suggesting").allowed ? "suggesting" : "viewing";
}

/** Which participant capability each gated command needs.
 *
 *  ONE TABLE, APPLIED ONCE, rather than a clause in every command row — the rule
 *  `SKILL` §10 states as fixing the pattern and not the instance. A review
 *  command added later is governed by the prefix it already has, instead of
 *  shipping ungated until somebody notices.
 *
 *  Matching is longest-prefix on the dotted id, so `review.comment.resolve` takes
 *  `review.comment`'s entry and `review.restrictEditing` its own.
 *
 *  NOT HERE: every command that merely changes content. Those are governed by the
 *  review MODE — `edit` is what admits Editing, and `blockMutationInViewing` is
 *  one fail-closed choke point rather than three hundred disabled buttons — and
 *  listing them here would be a second enforcement of one rule. Nor `review.compare`
 *  or `review.toggle`: both are reads. */
export const PARTICIPANT_GATED_COMMANDS = Object.freeze({
  "review.comment": "comment",
  "review.accept": "review",
  "review.reject": "review",
  "review.restrictEditing": "manageProtection",
});

/**
 * The capability a command id needs, or `null` when the grant does not gate it.
 *
 * Longest matching prefix wins, and a prefix only matches on a dotted boundary —
 * so `review.acceptNext` is governed by `review.accept` while a hypothetical
 * `review.acceptance` would not be, which is what stops a new command being
 * silently captured by a name it merely starts with.
 *
 * Complexity: O(table).
 *
 * @param {string} id
 * @returns {string|null}
 */
export function capabilityForCommand(id) {
  const name = String(id ?? "");
  let best = null;
  for (const prefix of Object.keys(PARTICIPANT_GATED_COMMANDS)) {
    if (name !== prefix && !name.startsWith(`${prefix}.`) && !startsWithCamelTail(name, prefix)) {
      continue;
    }
    if (best === null || prefix.length > best.length) best = prefix;
  }
  return best === null ? null : PARTICIPANT_GATED_COMMANDS[best];
}

/** `review.accept` matches `review.acceptNext` and `review.acceptAll`: the
 *  decision verbs are spelt as one camel-cased segment in this registry
 *  (`acceptNext`, `rejectAll`), so the boundary is the capital letter rather than
 *  a dot. Checked explicitly instead of with a loose `startsWith`, so
 *  `review.acceptance` — a different command — is not captured. */
function startsWithCamelTail(name, prefix) {
  if (!name.startsWith(prefix) || name.length === prefix.length) return false;
  const next = name[prefix.length];
  return next >= "A" && next <= "Z";
}

/**
 * Narrows a command list to a participant's grant, in place of its own reason.
 *
 * Returns a NEW array of new rows; nothing is mutated, so a caller can hold the
 * ungated list for a surface the grant does not govern. A row the grant withholds
 * comes back `enabled: false` with `disabledReason` set to the translated
 * sentence for its `session.*` code — the same sentence the engine would have
 * refused with, which is the whole reason there is one table.
 *
 * THE GRANT'S SENTENCE OUTRANKS A TRANSIENT ONE, and that is the single rule
 * used everywhere a control has two reasons to refuse — here, in
 * `ribbon_surface.mjs`'s review sweep, and in `document_protection.mjs`'s row.
 * "Select text to comment on" is actionable advice, which is exactly the problem
 * when the reader may never comment whatever they select: following it costs
 * them a gesture and teaches them nothing. The permanent truth is the one worth
 * saying, which is the same ordering `reflectReviewModeAccess` already applies
 * when it lets a document the engine refuses outrank a withheld mode.
 *
 * It only ever moves a row from enabled to disabled, which is the only direction
 * a narrowing may move anything.
 *
 * Complexity: O(commands).
 *
 * @param {readonly object[]} commands
 * @param {{shared: boolean, decide: (c: string) => object}} grant
 * @param {(key: string) => string} translate
 * @returns {object[]}
 */
export function narrowCommandsToGrant(commands, grant, translate) {
  const rows = [...(commands ?? [])];
  if (!grant?.shared) return rows;
  return rows.map((row) => {
    const capability = capabilityForCommand(row?.id);
    if (capability === null) return row;
    const decision = grant.decide(capability);
    if (decision.allowed) return row;
    return { ...row, enabled: false, disabledReason: translate(decision.key) };
  });
}

/**
 * The one object the chrome holds: a participant's grant, already bound to the
 * catalogue, with every answer it has to give.
 *
 * ONE SEAM AND NOT SIX, for the reason `capabilities.mjs` gives about host
 * inputs: a permission six call sites resolve independently is a permission six
 * call sites can disagree about. `main.js` builds this once and asks it.
 *
 * Complexity: O(names) at construction; every method is O(1) or O(its input).
 *
 * @param {{granted?: string|readonly string[]|null, participant?: string|number|null}} inputs
 *        the host's inputs, as `capabilities.mjs`'s `hostConfig` reads them
 * @param {(key: string) => string} translate the catalogue lookup
 */
export function sessionAccess(inputs, translate) {
  const grant = participantGrant(inputs);
  const sentence = (code) => {
    const key = refusalKey(code);
    return key ? translate(key) : "";
  };
  return Object.freeze({
    grant,
    /** The localised sentence for a refusal CODE the engine threw, or `""` for a
     *  code this build does not route — in which case the caller keeps the
     *  engine's own English, which is more specific than any replacement.
     *
     *  The same table as `refusalFor`, which is the whole point: the sentence on
     *  a disabled control and the sentence after a refused gesture are one
     *  string, so a reader is never told two different things about one
     *  permission. */
    sentenceFor: (code) => sentence(code),
    /** The sentence a control carries while this participant's grant withholds
     *  it, or `""` when the grant admits it (and when there is no room at all).
     *  A STRING and not a boolean, so a caller cannot gate on it and forget to
     *  say why — the `ObjectCapabilities` shape this module's header names. */
    refusalFor(command) {
      const capability = capabilityForCommand(command);
      if (capability === null || !grant.shared) return "";
      const decision = grant.decide(capability);
      return decision.allowed ? "" : sentence(decision.code);
    },
    /** The authority `reflectReviewModeAccess` asks about a review MODE, or
     *  `null` when there is no room — so a standalone page is unchanged by the
     *  existence of this seam rather than passing a permissive stub around. */
    modeAuthority: grant.shared
      ? Object.freeze({
          allows(mode) {
            const decision = participantAllowsMode(grant, mode);
            return Object.freeze({
              allowed: decision.allowed,
              reason: decision.allowed ? "" : sentence(decision.code),
            });
          },
        })
      : null,
    /** The palette's and the menus' rows, narrowed to this grant. */
    narrow: (commands) => narrowCommandsToGrant(commands, grant, (key) => translate(key)),
    /** The mode a document should OPEN in, given the mode the container asked
     *  for: the stronger of the two is never chosen, because a grant narrows. */
    openMode(containerMode) {
      const ceiling = participantModeCeiling(grant);
      if (ceiling === "viewing" || containerMode === "viewing") return "viewing";
      if (ceiling === "suggesting" || containerMode === "suggesting") return "suggesting";
      return "editing";
    },
    /**
     * Hands the grant to the engine, which is where it is ENFORCED.
     *
     * Identity first, then capabilities: the identity call is what partitions
     * this replica's minting (`152` §4.2) and it must land before the first edit
     * the participant intends to share. Both are idempotent —
     * `adoptParticipantCapabilities` intersects, and `adoptParticipantIdentity`
     * reserves above whatever the document already holds in the space it enters —
     * so re-adopting on each open is correct rather than merely harmless.
     *
     * Returns a sentence when the engine refused the grant, and `""` otherwise.
     * A refusal here is not a security hole — the relay keeps its own copy and is
     * the authority — but it IS a lie: the chrome would be disabling controls the
     * engine still permits, so it is reported rather than swallowed.
     */
    adopt(doc) {
      if (!grant.shared || !doc) return "";
      try {
        if (grant.participant !== null) doc.adoptParticipantIdentity(grant.participant);
        doc.adoptParticipantCapabilities([...grant.names]);
        return "";
      } catch {
        return sentence(GRANT_UNREADABLE);
      }
    },
    /** What to announce once at boot when the grant itself could not be read. */
    problemMessage: () => (grant.problem === null ? "" : sentence(grant.problem)),
  });
}
