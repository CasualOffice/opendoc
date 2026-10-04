// The ROOM's grant, in the chrome (`152` §10 Q4/Q5, ADR-060).
//
// Everything here exists because the engine half shipped and the chrome half did
// not: `adoptParticipantCapabilities`, `participantCapabilities` and
// `adoptParticipantIdentity` each appeared exactly ONCE in the whole tree — at
// their own definition — with no caller in `webapp/src`, and not one of the five
// `session.*` refusal codes was routed anywhere. `SKILL` §9 rule 4.
//
// So the load-bearing tests are the two that are DERIVED from the engine rather
// than from a list kept here: the capability vocabulary comes out of
// `casual-doc-wasm`'s own match arms, and the refusal codes come out of
// `casual-doc-edit` and `docs/20`. A code the engine starts sending with no
// sentence in the chrome has to fail the build, or this whole change decays back
// into the state it was written to fix.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const ROOT = join(WEBAPP, "..");
const source = (...parts) => readFileSync(join(ROOT, ...parts), "utf8");

const {
  ACCESS_REFUSAL_KEYS,
  COLLABORATION_REFUSAL_KEYS,
  CONNECTION_LOST,
  GRANT_UNREADABLE,
  PARTICIPANT_CAPABILITIES,
  PARTICIPANT_GATED_COMMANDS,
  PROTECTION_REFUSAL_KEYS,
  REFUSAL_KEYS,
  capabilityForCommand,
  narrowCommandsToGrant,
  narrowedToWire,
  participantAllowsMode,
  participantGrant,
  participantModeCeiling,
  refusalKey,
  sessionAccess,
  withheldCode,
} = await import("../src/session_access.mjs");
const { EN_STRINGS } = await import("../src/en_strings.mjs");
const { reflectReviewModeAccess } = await import("../src/capabilities.mjs");

/** `(k) => k`: the identity catalogue, so an assertion can name the KEY a
 *  surface would show rather than the sentence of the day. Where a real sentence
 *  matters, the English catalogue is asked for it by name. */
const keys = (key) => key;

// ---- The two derived guards ------------------------------------------------

test("the capability vocabulary is exactly what the wasm facade accepts", () => {
  // Read out of the facade's own match arms. This is the guard that would have
  // caught `review`: the engine grew a fifth capability, `Capabilities::reviewer`
  // and `AccessRefusal::ReviewOnly` were built and tested, and the facade's
  // setter refused the NAME while its getter could not report it — so a host
  // handing over a verified reviewer grant got an error, and a chrome asking what
  // this participant may do was told "comment".
  const lib = source("crates", "casual-doc-wasm", "src", "lib.rs");
  const setter = lib.slice(
    lib.indexOf("fn adopt_participant_capabilities_internal"),
    lib.indexOf("fn comment_thread_inner"),
  );
  assert.ok(setter.includes("with_comment()"), "the setter has moved or been renamed");
  const accepted = [...setter.matchAll(/"([a-zA-Z]+)" => capabilities\./g)].map((m) => m[1]).sort();
  assert.deepEqual(
    [...PARTICIPANT_CAPABILITIES],
    accepted,
    "the chrome's capability names and the facade's differ, so a grant naming one " +
      "of them is either refused by the engine or invisible to the chrome",
  );

  const getter = lib.slice(
    lib.indexOf("pub fn participant_capabilities"),
    lib.indexOf("pub fn identity_space"),
  );
  const reported = [...getter.matchAll(/names\.push\("([a-zA-Z]+)"/g)].map((m) => m[1]).sort();
  assert.deepEqual(
    reported,
    accepted,
    "the facade accepts a capability it cannot report, or reports one it will not accept",
  );
});

test("every refusal code the engine can send has a sentence in every locale", () => {
  // DERIVED, from the three places a code is minted, rather than from a list in
  // this file: the point of the guard is to fail when the engine adds one.
  const access = source("crates", "casual-doc-edit", "src", "access.rs");
  const protection = source("crates", "casual-doc-edit", "src", "protection.rs");
  const wasm = source("crates", "casual-doc-wasm", "src", "lib.rs");
  const register = source("docs", "20-ERROR-CODE-REGISTRY.md");

  // No trailing hyphen: `protection.rs`'s own test asserts the family PREFIX
  // `"document.protected-"`, and a looser pattern reads that prefix as a code and
  // charges the chrome for failing to route it.
  const minted = (text, family) =>
    [...text.matchAll(new RegExp(`"(${family}\\.[a-z0-9]+(?:-[a-z0-9]+)*)"`, "g"))].map((m) => m[1]);

  const sessionCodes = new Set(minted(access, "session"));
  assert.ok(sessionCodes.size >= 5, `only found ${sessionCodes.size} session codes in access.rs`);
  assert.deepEqual(
    [...sessionCodes].sort(),
    Object.keys(ACCESS_REFUSAL_KEYS).sort(),
    "a `session.*` refusal the engine sends is not routed by the chrome",
  );

  // The document's own protection, which is the OTHER authority and reachable on
  // a document opened from a file with no room at all.
  //
  // `document.protection-unknown-level` is excluded ON PURPOSE and named here so
  // the exclusion cannot become an oversight: its sentence quotes the restriction
  // token the file actually carried, and a translated sentence with the token
  // dropped would be less useful than the English it replaced.
  const QUOTES_ITS_OWN_VALUE = "document.protection-unknown-level";
  const documentCodes = new Set(
    [...minted(protection, "document"), ...minted(wasm, "document")].filter(
      (code) => code.startsWith("document.protect") && code !== QUOTES_ITS_OWN_VALUE,
    ),
  );
  assert.ok(documentCodes.has(QUOTES_ITS_OWN_VALUE) === false);
  assert.deepEqual(
    [...documentCodes].sort(),
    Object.keys(PROTECTION_REFUSAL_KEYS).sort(),
    "a `document.protected-*` refusal the engine sends is not routed by the chrome",
  );
  assert.ok(
    minted(wasm, "document").includes(QUOTES_ITS_OWN_VALUE),
    "the excluded code is no longer sent, so the exclusion above is stale and should go",
  );

  // The collaboration family, from the register rather than from the Rust enum:
  // `docs/20` is already pinned to `protocol::Refusal` in both directions by
  // `every_refusal_code_has_a_row_in_the_register`, so chaining through it is
  // sound and does not need a second parser for a Rust match.
  const odc = new Set([...register.matchAll(/`(ODC-7\d{3})`/g)].map((m) => m[1]));
  assert.ok(odc.size >= 10, `only found ${odc.size} ODC-7xxx rows in docs/20`);
  assert.deepEqual(
    [...odc].sort(),
    Object.keys(COLLABORATION_REFUSAL_KEYS).sort(),
    "an `ODC-7xxx` refusal in the register is not routed by the chrome",
  );

  // And every routed code resolves to a declared, translated sentence — not to
  // its own key, which is what a reader would otherwise see in the status bar.
  const locales = readFileSync(join(WEBAPP, "locales", "de.json"), "utf8");
  const german = JSON.parse(locales);
  for (const [code, key] of Object.entries(REFUSAL_KEYS)) {
    assert.ok(EN_STRINGS[key], `${code} routes to ${key}, which EN_STRINGS does not declare`);
    assert.ok(german[key], `${code} routes to ${key}, which a shipped locale cannot answer`);
  }
});

// ---- The grant itself ------------------------------------------------------

test("a document with no room is standalone, not a participant granted nothing", () => {
  const standalone = participantGrant({});
  assert.equal(standalone.shared, false);
  assert.equal(standalone.participant, null);
  assert.equal(standalone.problem, null);
  // Says yes to everything, including a capability name nobody asked about: with
  // no relay there is no grant, nobody to issue one, and the local reader is the
  // only authority (`152` §2a, `Capabilities::local`).
  for (const capability of [...PARTICIPANT_CAPABILITIES, "nonsense"]) {
    assert.deepEqual(standalone.decide(capability), { allowed: true, code: null, key: null });
  }
  assert.equal(participantModeCeiling(standalone), "editing");
  assert.equal(sessionAccess({}, keys).modeAuthority, null);
});

test("an unreadable grant narrows to read-only AND says so", () => {
  // Fail closed, and never hand the engine a name it will refuse: the facade
  // rejects the whole call on an unknown capability, so a chrome that dropped the
  // bad name and kept the rest would be enforcing a different grant from the
  // engine's. Both halves are asserted, because narrowing silently is a bug that
  // looks like a working read-only mode.
  for (const inputs of [{ granted: "comment,nonsense" }, { participant: "-1" }, { participant: "4.5" }]) {
    const grant = participantGrant(inputs);
    assert.deepEqual([...grant.names], [], JSON.stringify(inputs));
    assert.equal(grant.participant, null);
    assert.equal(grant.problem, GRANT_UNREADABLE);
    assert.equal(grant.decide("comment").code, "session.read-only-access");
  }
  assert.equal(
    sessionAccess({ granted: "comment,nonsense" }, (k) => EN_STRINGS[k]).problemMessage(),
    EN_STRINGS["session.grantUnreadable"],
  );
  // And a grant that reads cleanly announces nothing at all.
  assert.equal(sessionAccess({ granted: "edit" }, keys).problemMessage(), "");
});

test("the withheld reason names what the participant MAY do, as the engine does", () => {
  // A MIRROR of `access::refusal_for`, driven against the same cases its own Rust
  // tests use. Naming the operation would tell the reader what the chrome
  // happened to send; naming the class tells them what they are allowed to do.
  assert.equal(withheldCode([], "edit"), "session.read-only-access");
  assert.equal(withheldCode(["comment"], "edit"), "session.comments-only-access");
  assert.equal(withheldCode(["comment", "suggest"], "edit"), "session.suggestions-only-access");
  assert.equal(withheldCode(["comment", "review"], "edit"), "session.review-only-access");
  // Widest write class held first, and `suggest` beats `review` — the Rust
  // function's own ordering, for the reason it records: suggesting's gestures are
  // the commoner ones and the sentence does not claim the other is absent.
  assert.equal(withheldCode(["comment", "review", "suggest"], "edit"), "session.suggestions-only-access");
  // Protection is its own answer whatever else is held, because the authority
  // question is about the policy rather than about content (ADR-059).
  for (const held of [[], ["comment"], ["suggest"], ["review"], ["edit"]]) {
    assert.equal(withheldCode(held, "manageProtection"), "session.no-protection-change", `${held}`);
  }
});

test("a mode a participant may not enter is refused with its own sentence", () => {
  const viewer = participantGrant({ granted: "" });
  assert.equal(participantAllowsMode(viewer, "viewing").allowed, true, "a reader may always read");
  assert.equal(participantAllowsMode(viewer, "editing").allowed, false);
  assert.equal(participantAllowsMode(viewer, "suggesting").allowed, false);
  assert.equal(participantModeCeiling(viewer), "viewing");

  // `comment` admits SUGGESTING, which is the decision `session_access.mjs`
  // records: suggesting is this editor's annotating mode, so a comment-only
  // participant placed in Viewing could not comment at all —
  // `blockMutationInViewing` refuses every mutation, a comment included.
  const commenter = participantGrant({ granted: "comment" });
  assert.equal(participantAllowsMode(commenter, "suggesting").allowed, true);
  assert.equal(participantModeCeiling(commenter), "suggesting");
  assert.equal(participantAllowsMode(commenter, "editing").code, "session.comments-only-access");

  const editor = participantGrant({ granted: "comment,suggest,review,edit" });
  assert.equal(participantModeCeiling(editor), "editing");
  // A grant never WIDENS the container's mode, only narrows it.
  assert.equal(sessionAccess({ granted: "edit" }, keys).openMode("viewing"), "viewing");
  assert.equal(sessionAccess({ granted: "comment" }, keys).openMode("editing"), "suggesting");
  assert.equal(sessionAccess({}, keys).openMode("suggesting"), "suggesting");
});

// ---- Reachability: the controls -------------------------------------------

test("every gated command resolves to a capability, and nothing else is captured", () => {
  assert.equal(capabilityForCommand("review.restrictEditing"), "manageProtection");
  assert.equal(capabilityForCommand("review.comment"), "comment");
  assert.equal(capabilityForCommand("review.comment.resolve"), "comment");
  assert.equal(capabilityForCommand("review.comment.delete"), "comment");
  for (const id of ["review.acceptNext", "review.acceptAll", "review.acceptAtCaret", "review.rejectAll"]) {
    assert.equal(capabilityForCommand(id), "review", id);
  }
  // Reads, and preferences: not the grant's business, and listing them would be a
  // second enforcement of the review MODE's rule.
  for (const id of ["review.compare", "review.toggle", "review.next", "tools.spellCheck", "format.bold"]) {
    assert.equal(capabilityForCommand(id), null, id);
  }
  // A camel boundary, not a loose prefix: a future `review.acceptance` is a
  // different command and must not inherit `review.accept`'s capability.
  assert.equal(capabilityForCommand("review.acceptance"), null);
  for (const capability of Object.values(PARTICIPANT_GATED_COMMANDS)) {
    assert.ok(PARTICIPANT_CAPABILITIES.includes(capability), capability);
  }
});

test("a withheld command is disabled WITH its reason, on every surface at once", () => {
  const rows = [
    { id: "review.restrictEditing", enabled: true, disabledReason: "" },
    { id: "review.acceptAll", enabled: true, disabledReason: "" },
    { id: "review.comment", enabled: false, disabledReason: "Select text to comment on" },
    { id: "format.bold", enabled: true, disabledReason: "" },
  ];
  const grant = participantGrant({ granted: "comment,suggest" });
  const narrowed = narrowCommandsToGrant(rows, grant, (k) => EN_STRINGS[k]);

  const row = (id) => narrowed.find((r) => r.id === id);
  // Disabled, and carrying the SAME sentence the engine would have refused with.
  assert.equal(row("review.restrictEditing").enabled, false);
  assert.equal(row("review.restrictEditing").disabledReason, EN_STRINGS["session.noProtectionChange"]);
  assert.equal(row("review.acceptAll").enabled, false);
  assert.equal(row("review.acceptAll").disabledReason, EN_STRINGS["session.suggestionsOnly"]);
  // The grant's sentence outranks a transient one — the single rule used wherever
  // a control has two reasons to refuse. Advice the reader can act on is worthless
  // when acting on it cannot help.
  assert.equal(row("review.comment").disabledReason, "Select text to comment on");
  // Untouched, and the input array is not mutated.
  assert.equal(row("format.bold").enabled, true);
  assert.equal(rows[0].enabled, true);
  // A standalone page is unchanged by the existence of the seam.
  assert.deepEqual(narrowCommandsToGrant(rows, participantGrant({}), keys), rows);
});

test("the mode control disables a mode the room withheld, with the room's reason", () => {
  // `reflectReviewModeAccess` is the DOM half, and this is the only place the
  // three authorities meet: the document the engine refuses, the mode the
  // container withheld, and the mode this participant was not granted. Driven on
  // plain objects, which is what makes the precedence answerable without a
  // browser.
  const button = (mode) => ({ dataset: { reviewMode: mode }, disabled: false, title: "", removeAttribute() { this.title = ""; }, setAttribute() {} });
  const buttons = [button("editing"), button("suggesting"), button("viewing")];
  const bannerEdit = { hidden: false };
  reflectReviewModeAccess({
    buttons,
    bannerEdit,
    capabilities: new Set(["edit", "comment"]),
    readOnlyReason: "",
    withheldReason: "the container withheld it",
    participant: sessionAccess({ granted: "comment,review" }, (k) => EN_STRINGS[k]).modeAuthority,
  });
  assert.equal(buttons[0].disabled, true, "a reviewer may not enter Editing");
  assert.equal(buttons[0].title, EN_STRINGS["session.reviewOnly"]);
  assert.equal(buttons[1].disabled, false, "and may enter Suggesting, which is where they comment");
  assert.equal(buttons[2].disabled, false, "a reader may always read");
  assert.equal(bannerEdit.hidden, true, "an offer that can never be taken is withdrawn, not disabled");

  // The document's own refusal is the more specific truth and still outranks both.
  const again = [button("editing")];
  reflectReviewModeAccess({
    buttons: again,
    capabilities: new Set(["edit"]),
    readOnlyReason: "this document is too large to edit",
    withheldReason: "the container withheld it",
    participant: sessionAccess({ granted: "" }, keys).modeAuthority,
  });
  assert.equal(again[0].title, "this document is too large to edit");
});

// ---- The engine seam ------------------------------------------------------

test("the grant is handed to the engine, identity before capabilities", () => {
  // The order is load-bearing: the identity call is what partitions this
  // replica's minting, and it has to land before the first edit the participant
  // intends to share (`152` §4.2).
  const calls = [];
  const doc = {
    adoptParticipantIdentity: (n) => calls.push(["identity", n]),
    adoptParticipantCapabilities: (names) => calls.push(["capabilities", [...names]]),
  };
  assert.equal(sessionAccess({ granted: "comment,edit", participant: "7" }, keys).adopt(doc), "");
  assert.deepEqual(calls, [
    ["identity", 7],
    ["capabilities", ["comment", "edit"]],
  ]);

  // Standalone adopts NOTHING. A document with no session must not call either —
  // the engine's own doc comments say so — and calling them with a default would
  // move minting into a space a real participant owns.
  const untouched = [];
  sessionAccess({}, keys).adopt({
    adoptParticipantIdentity: () => untouched.push("identity"),
    adoptParticipantCapabilities: () => untouched.push("capabilities"),
  });
  assert.deepEqual(untouched, []);
});

test("an engine that refuses the grant is reported, not swallowed", () => {
  // Not a security hole — the relay keeps its own copy and is the authority — but
  // a chrome disabling controls the engine still permits is a lie, and a silent
  // one is the worst kind.
  const refusing = {
    adoptParticipantIdentity() {},
    adoptParticipantCapabilities() {
      throw new Error('unknown capability "review"');
    },
  };
  assert.equal(
    sessionAccess({ granted: "review" }, (k) => EN_STRINGS[k]).adopt(refusing),
    EN_STRINGS["session.grantUnreadable"],
  );
});

test("an unroutable code keeps the engine's own sentence rather than a generic one", () => {
  assert.equal(refusalKey("table.unmerge-not-merged"), null);
  assert.equal(refusalKey(""), null);
  assert.equal(refusalKey(undefined), null);
  assert.equal(refusalKey("ODC-7010"), "collab.roomFull");
  assert.equal(sessionAccess({}, keys).sentenceFor("table.unmerge-not-merged"), "");
  assert.equal(sessionAccess({}, keys).sentenceFor("session.read-only-access"), "session.readOnly");
});

// ---- The grant arriving on the wire ---------------------------------------

/** **A grant that arrives on the wire narrows this participant and cannot widen
 *  them.**
 *
 *  Until the transport existed the grant arrived on the URL, which `152` §9
 *  recorded as safe-by-construction rather than trusted because
 *  `adoptParticipantCapabilities` intersects. `Welcome` and `Resumed` now carry
 *  what the RELAY decided (`143` §10: a resume REPLACES the grant, so a
 *  revocation cannot be undone by reconnecting), and this is the chrome's half
 *  of the same intersection — a control must disable itself for exactly the
 *  reason a submission would be refused.
 *
 *  The widening direction is the security property and the narrowing direction
 *  is the one that silently stops working, so both are driven. */
test("a grant arriving on the wire narrows the chrome and cannot widen it", () => {
  const editor = participantGrant({ granted: "comment,edit", participant: 3 });

  // Narrowing: the relay says comment-only, and the chrome agrees.
  const narrowed = sessionAccess(narrowedToWire(editor, ["comment"], 3), keys);
  assert.deepEqual([...narrowed.grant.names], ["comment"]);
  assert.equal(narrowed.grant.decide("edit").allowed, false);
  assert.equal(narrowed.grant.decide("edit").code, "session.comments-only-access");

  // Widening: the relay claims owner for a participant holding two capabilities.
  const widened = sessionAccess(
    narrowedToWire(narrowed.grant, ["comment", "edit", "review", "suggest", "manageProtection"], 3),
    keys,
  );
  assert.deepEqual(
    [...widened.grant.names],
    ["comment"],
    "a value arriving on the wire WIDENED the chrome's grant, so the wire is a way to " +
      "escalate and the intersection has become a replacement",
  );
});

/** **A standalone replica takes the wire's list whole, and that is the
 *  intersection rather than an exception to it.**
 *
 *  A document with no room holds everything, like `Capabilities::local` (`152`
 *  §2a mode 1), so intersecting with it is the identity — which is exactly what
 *  `adopt_participant_capabilities_internal` does in the engine when a first
 *  `Welcome` lands on a document opened from a file. Getting this wrong the
 *  other way is the subtle one: intersecting against an empty `names` would
 *  leave every shared document a viewer and look like a working read-only mode.
 */
test("a standalone replica adopts the wire's grant whole", () => {
  const standalone = participantGrant({});
  assert.equal(standalone.shared, false);
  const joined = sessionAccess(narrowedToWire(standalone, ["comment", "edit"], 7), keys);
  assert.equal(joined.grant.shared, true);
  assert.deepEqual([...joined.grant.names], ["comment", "edit"]);
  assert.equal(joined.grant.participant, 7, "and the participant number is the relay's");
});

/** **A capability name this build does not know fails closed, with a reason.**
 *
 *  Passed straight through to `participantGrant` rather than filtered out here,
 *  because `adoptParticipantCapabilities` refuses the WHOLE call on an unknown
 *  name: dropping it and keeping the rest would leave the chrome and the engine
 *  disagreeing about what this participant may do, with the chrome the more
 *  permissive of the two. */
test("an unknown capability on the wire narrows to a viewer and says so", () => {
  const editor = participantGrant({ granted: "comment,edit", participant: 3 });
  const access = sessionAccess(narrowedToWire(editor, ["edit", "teleport"], 3), keys);
  assert.deepEqual([...access.grant.names], []);
  assert.equal(access.grant.problem, GRANT_UNREADABLE);
  assert.equal(access.problemMessage(), "session.grantUnreadable");
});

/** **A frame carrying no capability list leaves the grant exactly as it was.**
 *
 *  An `Ack`, an `Apply` and an `Awareness` carry no grant, so the transport hands
 *  this `undefined` on every frame but two. Re-deriving the grant from nothing
 *  must therefore be the identity, or every acknowledgement silently demotes the
 *  reader to a viewer. */
test("a frame with no capability list does not change the grant", () => {
  for (const absent of [undefined, null, "edit"]) {
    const editor = participantGrant({ granted: "comment,edit", participant: 3 });
    const same = sessionAccess(narrowedToWire(editor, absent), keys);
    assert.deepEqual([...same.grant.names], ["comment", "edit"], `for ${JSON.stringify(absent)}`);
    assert.equal(same.grant.participant, 3);
  }
  // And a standalone document stays standalone rather than becoming a viewer.
  const alone = sessionAccess(narrowedToWire(participantGrant({}), undefined), keys);
  assert.equal(alone.grant.shared, false);
  assert.equal(alone.grant.decide("edit").allowed, true);
});

/** **The connection-lost code is in the one table, and the engine never sends
 *  it.**
 *
 *  The same shape as `session.grant-unreadable` and for a sharper reason: an
 *  eviction IS a failed write, so there is no socket left to carry a refusal and
 *  `protocol::Refusal` has no variant for it — which is also why it must NOT
 *  appear in `docs/20`, where `every_refusal_code_has_a_row_in_the_register`
 *  would fail the other way for a row no variant carries. Asserted in both
 *  directions here, because a code with no sentence is a status bar showing a
 *  key and a code the engine does send is a code this table should not invent.
 */
test("the connection-lost code is routed by the chrome and minted by nothing else", () => {
  assert.equal(refusalKey(CONNECTION_LOST), "session.connectionLost");
  assert.ok(EN_STRINGS["session.connectionLost"]);
  const access = source("crates", "casual-doc-edit", "src", "access.rs");
  const protection = source("crates", "casual-doc-edit", "src", "protection.rs");
  const wasm = source("crates", "casual-doc-wasm", "src", "lib.rs");
  const collab = source("crates", "casual-doc-wasm", "src", "collab.rs");
  const register = source("docs", "20-ERROR-CODE-REGISTRY.md");
  for (const [name, text] of [
    ["access.rs", access],
    ["protection.rs", protection],
    ["lib.rs", wasm],
    ["collab.rs", collab],
    ["docs/20", register],
  ]) {
    assert.ok(
      !text.includes(CONNECTION_LOST),
      `${name} mentions ${CONNECTION_LOST}; a code the engine can produce does not belong ` +
        "in the chrome-only half of this table, and one the wire can never carry does not " +
        "belong in the register",
    );
  }
  // The scan can see what it looks for.
  assert.ok(`a ${CONNECTION_LOST} b`.includes(CONNECTION_LOST));
});
