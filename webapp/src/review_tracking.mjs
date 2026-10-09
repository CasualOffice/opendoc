// Track Changes as the DOCUMENT states it (`w:trackRevisions`, `docs/165` M7,
// `docs/109` HF-283).
//
// Word saves a document with tracking on, and opens it that way. This editor's
// Suggesting mode is how tracking is presented here, so the two are joined in
// three places and nowhere else:
//
//   * OPEN — a document whose file says tracking is on opens in Suggesting
//     (never stronger than the mode the container and the grant allow: a mode
//     that would have been Viewing stays Viewing);
//   * the reader's own SWITCH between Editing and Suggesting writes the setting
//     (`setTrackRevisions`, one undoable edit), so a save says what the reader
//     last chose and a reopen honours it; a mode change that is not the
//     reader's gesture — a demo preset, a version preview, this module's own
//     follow — writes nothing;
//   * when an EDIT changes the setting — an Undo of that switch, a Redo, or a
//     co-author turning tracking on for everyone — the mode follows it, but only
//     on a change: a session in Suggesting over a document that does not track
//     (a demo, a reviewer's grant) is left alone on every keystroke.
//
// Viewing is read-only and orthogonal: entering or leaving it changes nothing,
// and a change of setting never pulls a reader out of it.
//
// No DOM and no engine of its own: the document and the edit runner are passed
// in, so the three rules are unit-testable as plain functions.

/**
 * @returns {{
 *   openMode: (mode: string, documentTracks: boolean) => string,
 *   apply: (at: {doc: any, mode: string, byUser: boolean, caret?: {node: string, offset: number}|null, runEdit: Function}) => void,
 *   follow: (documentTracks: boolean|undefined, mode: string, switchTo?: ((mode: string) => unknown)|null) => string|null,
 * }}
 */
export function createReviewTracking() {
  // The setting as last seen, so `follow` reacts to a CHANGE and not to a
  // standing difference between the mode and the document.
  let seen = null;
  return {
    /** The mode a document opens in, given the mode the session would open it
     *  in. O(1). */
    openMode(mode, documentTracks) {
      seen = documentTracks === true;
      return mode === "editing" && seen ? "suggesting" : mode;
    },

    /** Hands a mode change to the engine. O(1), plus one edit when the reader's
     *  own switch changes the document's setting. */
    apply({ doc, mode, byUser, caret, runEdit }) {
      if (!doc) return;
      // Paragraph formatting is tracked for as long as Suggesting is on (HF-131).
      // Every paragraph formatting command funnels through one engine choke point,
      // and review decisions build their own operations, so this cannot accidentally
      // track an Editing-mode change or a decision. Dated per command by the engine.
      doc.setParagraphTracking(mode === "suggesting", undefined);
      if (!byUser || mode === "viewing") return;
      const wanted = mode === "suggesting";
      if (doc.trackRevisions === wanted) return;
      // `seen` is NOT advanced here: the edit's own landing is what `follow`
      // sees, so a refused switch (a tracked-changes restriction locks the
      // setting) leaves nothing half-recorded.
      // `keepSelection`: a setting has no place in the text, so the reader's
      // selection survives the switch — a word selected before choosing
      // Suggesting is still selected after, and typing replaces it as a tracked
      // replacement rather than inserting beside a collapsed caret.
      void runEdit(() => doc.setTrackRevisions(wanted, caret?.node ?? "", caret?.offset ?? 0), { keepSelection: true });
    },

    /** After a landed edit: the mode to switch to when the edit changed the
     *  document's setting, or null to stay — handed to `switchTo` too, when
     *  given, which is how the host switches. O(1). */
    follow(documentTracks, mode, switchTo = null) {
      const now = documentTracks === true;
      if (now === seen) return null;
      seen = now;
      if (mode === "viewing") return null;
      const wanted = now ? "suggesting" : "editing";
      if (wanted === mode) return null;
      switchTo?.(wanted);
      return wanted;
    },
  };
}
