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
});
