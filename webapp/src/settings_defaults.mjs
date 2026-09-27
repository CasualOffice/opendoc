// What the editor believes before anyone has told it anything.
//
// Pure data, no DOM, no engine — which is why it can live here and be read by a
// unit test, and why `docs/125` names it as the layer a host configuration sits
// on top of: an embedded editor's defaults are the host's to choose, and a
// module is something a host can import and a 17,000-line browser script is not.
//
// Every value below is a decision with a reason attached. They are written down
// because "why is this on by default" is asked of each of them eventually, and
// the answer is otherwise in a commit message nobody will find.
export const DEFAULT_SETTINGS = Object.freeze({
  theme: "system",
  // "" means follow the browser. A person who has never touched this gets
  // their own language if we ship it, and a person who chose one keeps it even
  // on a machine whose browser disagrees (docs/124 §5).
  language: "",
  accent: "#3355c4",
  authorName: "",
  authorInitials: "",
  // On by default: the row this closes is a P0 data-safety row, and a safety
  // net nobody switches on is not one. Turning it off deletes what is stored.
  autosave: true,
  // Spelling. On by default because a plain <textarea> checks spelling and an
  // editor that does not is visibly behind one; remembered, because a user who
  // turned it off did not mean "until the next reload" (docs/114 §5.6).
  spellCheck: true,
  // Grammar. A SEPARATE switch from spelling, as in Word: the two checks are
  // independent, they mark differently, and the owner rates grammar the more
  // important of the two — so it must not be reachable only by leaving
  // spelling on.
  grammarCheck: true,
  // The language used where the document's own w:lang does not say. NOT
  // navigator.language: that would make every test non-deterministic and the
  // user could not see why the answer changed.
  spellLanguage: "en-US",

  // ── Version history (docs/139, docs/140, ADR-038; HF-068 / OO-004) ─────────
  //
  // The owner's ruling, verbatim in substance: "you can keep it in config.. or
  // in setting … like for version around 20-30 or retain for 7 days". So the
  // policy is configurable, here, with both a count cap and an age window — and
  // the two are NOT the same kind of rule, which is the part the ruling left
  // open and ADR-038 settles: the count and the byte budget are CEILINGS that
  // always apply, while the age window is a floor on how long history is kept
  // and never a licence to empty a timeline. `version_history.mjs` clamps every
  // value below to the engine's hard limits, because a stored preference is
  // untrusted input like any other.

  // On by default, for autosave's reason: a document-safety net nobody switches
  // on is not one. It rides on the autosave path, so turning autosave off turns
  // this off with it — one switch cannot promise what the other has stopped
  // doing. (docs/139 §18 question 2.)
  versionHistory: true,

  // The count ceiling, in the middle of the owner's 20–30 band. 25 checkpoints
  // of the 471-page worst case docs/112 measured (2.4 MB each) is ~60 MB, so
  // the count normally binds before the byte budget does.
  versionRetentionCount: 25,

  // The age window, in days. A version older than this is ELIGIBLE for pruning;
  // it is not thereby doomed — see the floor below.
  versionRetentionDays: 7,

  // How many recent unnamed versions survive the age window regardless of age.
  // Without a floor, "retain for 7 days" would delete the entire history of a
  // document nobody touched for a week, which is the opposite of what retaining
  // means. Three is the smallest floor that still answers "what did this look
  // like before my last session?" rather than only "what does it look like now".
  versionRetentionFloor: 3,

  // The byte budget, the bound docs/140 §13 and HF-068 ask for by name. Media
  // heavy documents hit bytes long before they hit the count, and the quota this
  // shares with drafts is the browser's, not ours.
  versionRetentionMegabytes: 120,

  // How often an unattended edit session lays down an automatic version. NOT
  // the autosave cadence: autosave writes one draft every 5 s of quiet and
  // overwrites it, while a version is kept, so one per keystroke pause would
  // spend the whole count ceiling in two minutes. Ten minutes puts ~4 hours of
  // editing inside 25 versions, which is the granularity Google Docs' session
  // grouping gives and the granularity "before lunch" needs.
  versionIntervalMinutes: 10,

  // How many versions may be NAMED. Named versions are never pruned
  // automatically (docs/139 §8.7, VH-006), so without a cap a user could pin
  // the store solid and then be refused every new version — the refusal being
  // correct and the wedge being avoidable. Google Docs allows 40 named
  // versions; 15 of 25 leaves ten slots that automatic capture can always use.
  versionNamedLimit: 15,
});
