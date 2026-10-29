// Review ▸ Restrict Editing — `w:documentProtection`, from the chrome (ADR-052,
// ADR-059; `docs/153` `review.restrict-editing`).
//
// The sharpest instance of `SKILL` §9 rule 4 in the tree, pointing BOTH ways.
// ADR-052 made the restriction *enforced* at the operation for `readOnly`,
// `comments` and `trackedChanges`; ADR-059 added the one operation that can
// install or lift it. Neither was reachable, so a document that arrived protected
// was permanently read-only in this editor — the restriction could be obeyed and
// never changed.
//
// ---- THE SHAPE, FROM THE TWO REFERENCES ------------------------------------
//
//   * Word 365: Review ▸ Protect ▸ Restrict Editing opens a task pane with
//     "Allow only this type of editing in the document" over a dropdown of four —
//     No changes (Read only) / Tracked changes / Comments / Filling in forms —
//     and a "Yes, Start Enforcing Protection" button that offers a password.
//   * ONLYOFFICE, read from their source
//     (`apps/documenteditor/main/app/view/ProtectDialog.js:121-148,230-233`):
//     exactly four radios — `rbView` / `rbForms` / `rbReview` / `rbComments`,
//     carrying `Asc.c_oAscEDocProtect.ReadOnly` / `Forms` / `TrackedChanges` /
//     `Comments` — plus `inputPwd` and `repeatPwd` password fields.
//
// So: a radio group, which is ONLYOFFICE's presentation, in Word's order
// (strictest first), with Word's own wording for each level, and a fifth row for
// "no restriction" because Word's Stop Protection has to be reachable from the
// same control rather than from a second button that appears conditionally. One
// mechanism, not two.
//
// ---- THE PASSWORD (ADR-052 as amended 2026-10-30) ---------------------------
//
// Both references offer one and, since `docs/153` `review.protection-password`,
// so does this. Installing a restriction takes an optional new password and its
// confirmation (ONLYOFFICE's `inputPwd`/`repeatPwd`); changing or stopping one
// that carries a password this editor can check asks for it first (Word's Stop
// Protection). The verifier is Word's own — `casual-doc-wasm`
// `review_protection.rs` writes the SHA-512, 100,000-spin, legacy-key-folded
// hash Word 2013+ writes — so Word asks a reader for the same password.
//
// What the dialog says beside the fields is what it is NOT: encryption. The
// verifier sits in plain XML next to the restriction, so "policy, not security"
// stays true in what the reader is told. A verifier this build cannot check (an
// algorithm or a spin count it does not take) is the one case left where a change
// removes the password, and the dialog says so before Apply.

// ---- THE ORDERING TRAP, ON THE CHROME'S SIDE OF THE LINE -------------------
//
// ADR-059's hard part was that an operation which LIFTS a restriction would
// otherwise be refused by the restriction it is lifting, and the engine answers it
// with `casual_doc_edit::protection::exempt_from_protection`. The chrome has the
// same trap one layer up and it is answered the same way: the write goes through
// `runEdit` WITHOUT `gate`. `blockUntrackedInSuggesting` exists to stop an
// untracked CONTENT edit slipping past review; it is not an authority check, and a
// policy change cannot be expressed as a tracked suggestion at all — so gating it
// would rebuild, in `webapp/`, exactly the one-way door ADR-059 removed in
// `casual-doc-edit`. Viewing mode still refuses, through `runEdit`'s own
// `blockMutationInViewing`, because a reader the HOST made a viewer is not being
// asked about the document's policy.
//
// Deliberately NOT done, and recorded rather than left ambiguous: a
// `trackedChanges`-protected document does not force the editor into Suggesting
// mode on open, as Word does. The mode is reversible and the restriction is
// enforced at the operation either way, and forcing the mode without first making
// the unprotect path mode-exempt is the one-way door again.
import { t } from "./i18n.mjs";

/** The levels, in Word's dropdown order with "no restriction" first.
 *
 *  `edit` is the engine's own `w:edit` token, or `null` for Word's Stop
 *  Protection — which removes `w:documentProtection` outright rather than writing
 *  `w:edit="none"`. Those two are different states and the engine keeps them
 *  apart, so this table does too. */
export const PROTECTION_LEVELS = Object.freeze([
  Object.freeze({ value: "off", edit: null, labelKey: "protect.level.none" }),
  Object.freeze({ value: "readOnly", edit: "readOnly", labelKey: "protect.level.readOnly" }),
  Object.freeze({
    value: "trackedChanges",
    edit: "trackedChanges",
    labelKey: "protect.level.trackedChanges",
  }),
  Object.freeze({ value: "comments", edit: "comments", labelKey: "protect.level.comments" }),
  Object.freeze({ value: "forms", edit: "forms", labelKey: "protect.level.forms" }),
]);

/** Whether an `edit` token restricts anything at all.
 *
 *  `null` is the element being absent and `"none"` is a restriction switched off;
 *  neither restricts, and the two are deliberately not collapsed anywhere else,
 *  because only the EXPORTER may decide which of them to write back.
 *
 *  O(1). */
const restricts = (edit) => edit !== null && edit !== "none";

/**
 * Reads `documentProtection()`'s JSON into the two things the dialog shows.
 *
 * Exported and pure so the five states a document can arrive in — absent,
 * `edit="none"`, an enforced level, an UNENFORCED level, and an `edit` token this
 * build does not know — are answerable in node rather than only through a
 * browser and five fixtures.
 *
 * `edit: "none"` with `enforcement: false` is a real state Word round-trips (a
 * restriction an author set up and switched off) and it is deliberately NOT the
 * same as `edit: null`, which is the element being absent. Both show as "no
 * restriction" in the radio group because neither restricts anything; what keeps
 * them apart is that Apply only writes when something the reader chose actually
 * differs, so opening the dialog on an `edit="none"` document and pressing Cancel
 * — or Apply — does not quietly rewrite the file.
 */
export function readProtection(json) {
  let parsed = null;
  try {
    parsed = typeof json === "string" ? JSON.parse(json) : json;
  } catch {
    parsed = null;
  }
  const edit = parsed?.edit ?? null;
  const known = PROTECTION_LEVELS.find((level) => level.edit === edit);
  return {
    /** The engine's token, carried through even when it is one this build has no
     *  row for — so Apply can leave an unknown restriction alone instead of
     *  silently downgrading it to "no restriction". */
    edit,
    /** The radio row to check. An unknown token falls back to the strictest row
     *  rather than to "off": reading a restriction nobody can name as "none" is
     *  the one answer that could lose a protection. */
    value: edit === null || edit === "none" ? "off" : (known?.value ?? "readOnly"),
    enforcement: parsed?.enforcement === true,
    formatting: parsed?.formatting === true,
    /** Whether the file carries a Word password with the restriction. Kept and
     *  saved, never checked (ADR-052); a change made here removes it, which the
     *  dialog says before Apply and the status line after. */
    password: parsed?.password === true,
    /** Whether a restriction is actually in force, which is the ribbon button's
     *  pressed state and the access badge's document authority. `edit="none"`
     *  restricts nothing however it is enforced — **unless `w:formatting` is on**,
     *  which is Word's pure formatting restriction and is a restriction in force by
     *  any reading: a document that refuses every formatting gesture was reporting
     *  itself unprotected, so the lock button showed unpressed and the badge named
     *  no authority at all. Both axes, one predicate, because "is anything in
     *  force" is one question and two predicates would answer it differently. */
    active:
      parsed?.enforcement === true && (restricts(edit) || parsed?.formatting === true),
  };
}

/**
 * Mounts Restrict Editing over the markup in `editor.html`.
 *
 * @param {object} io
 * @param {() => object|null} io.getDoc
 * @param {(thunk: () => unknown, options?: object) => Promise<boolean>} io.runEdit
 * @param {(dialog: Element, options: object) => object} io.registerModal
 * @param {() => Element|null} io.fallbackFocus
 * @param {(container: Element, options: object) => object} io.bindRadioGroup
 * @param {(text: string, kind?: string) => void} io.setStatus
 * @param {() => void} io.onChanged
 */
export function createDocumentProtection(io) {
  const el = (id) => document.getElementById(id);
  const dialog = el("restrictEditingDialog");
  const levels = el("restrictEditingLevels");
  const enforce = el("restrictEditingEnforce");
  const enforceRow = el("restrictEditingEnforceRow");
  const formatting = el("restrictEditingFormatting");
  const applyBtn = el("restrictEditingApply");
  const cancelBtn = el("restrictEditingCancel");
  const closeBtn = el("restrictEditingClose");
  const passwordNote = el("restrictEditingPassword");
  const currentRow = el("restrictEditingCurrentRow");
  const currentInput = el("restrictEditingCurrentPassword");
  const newRow = el("restrictEditingNewRow");
  const newInput = el("restrictEditingNewPassword");
  const confirmInput = el("restrictEditingConfirmPassword");
  const passwordError = el("restrictEditingPasswordError");
  const stopBtn = el("restrictEditingStop");

  /** What the document says right now. `{edit:null,…}` with no document open. */
  function current() {
    const doc = io.getDoc();
    return readProtection(doc ? doc.documentProtection() : null);
  }

  if (!dialog || !levels) {
    // The markup is not in this chrome. Return the same shape so no caller needs
    // a null check, and so a host that composed the Review region away does not
    // get a crash instead of a missing button.
    return { open() {}, reflect() {}, state: current, commands: () => [], isActive: () => false };
  }

  const group = io.bindRadioGroup(levels, {
    attr: "data-protect-level",
    onSelect: () => reflectEnforce(),
  });

  const modal = io.registerModal(dialog, {
    initialFocus: () => group.selected() ?? levels.querySelector("[data-protect-level]"),
    fallbackFocus: io.fallbackFocus,
    defaultAction: () => void apply(),
  });

  /** The enforcement checkbox is meaningless with no restriction chosen, so it is
   *  disabled CARRYING THE REASON rather than greyed in silence.
   *
   *  "No restriction chosen" means NEITHER axis, and that is the whole difference
   *  the formatting checkbox makes here: Word writes
   *  `w:edit="none" w:formatting="1" w:enforcement="1"` when an author ticks the
   *  formatting box and no editing box, so a reader who has ticked only the
   *  formatting box has chosen something and must be able to apply it. Reading the
   *  radio group alone would disable Apply's own precondition on exactly the state
   *  the new control exists to reach. */
  function reflectEnforce() {
    const off = (group.value() ?? "off") === "off" && formatting?.checked !== true;
    // A new password belongs to a restriction being installed; with none chosen
    // there is nothing for it to guard, so the fields go rather than sit inert.
    if (newRow) newRow.hidden = off;
    if (!enforce) return;
    enforce.disabled = off;
    if (off) enforce.checked = false;
    if (enforceRow) enforceRow.title = off ? t("protect.enforceOff") : "";
  }

  /** Says a password problem beside the fields — `role="alert"`, so a screen
   *  reader hears it — and puts focus where the reader has to act. */
  function refusePassword(message, field) {
    if (passwordError) {
      passwordError.textContent = message;
      passwordError.hidden = false;
    }
    field?.focus?.();
  }

  /** The restriction's password as the engine reads it. A build without the
   *  password methods answers "no password", which is what it could check. */
  function passwordState() {
    const doc = io.getDoc();
    try {
      const parsed = JSON.parse(doc?.protectionPassword?.() ?? "null");
      return { present: parsed?.present === true, verifiable: parsed?.verifiable === true };
    } catch {
      return { present: false, verifiable: false };
    }
  }

  function open() {
    if (!io.getDoc()) return;
    const state = current();
    group.reflect(state.value);
    if (enforce) enforce.checked = state.enforcement;
    if (formatting) formatting.checked = state.formatting;
    const password = passwordState();
    // The "removes the password" note is for the one verifier this editor cannot
    // check; one it can check is asked for instead.
    if (passwordNote) passwordNote.hidden = !(state.password && !password.verifiable);
    if (currentRow) currentRow.hidden = !password.verifiable;
    for (const input of [currentInput, newInput, confirmInput]) if (input) input.value = "";
    if (passwordError) {
      passwordError.hidden = true;
      passwordError.textContent = "";
    }
    if (stopBtn) stopBtn.hidden = !state.active;
    reflectEnforce();
    modal.open();
  }

  /** Word's Stop Protection: the "No restriction" row, applied. */
  function stop() {
    group.reflect("off");
    if (formatting) formatting.checked = false;
    reflectEnforce();
    void apply();
  }

  /** Writes the chosen level, as ONE undoable action, and says what happened.
   *
   *  Says something on success as well as on refusal: a restriction is invisible
   *  until someone tries to type, so silence reads as "nothing happened" — the one
   *  outcome `docs/67` forbids. Nothing is written when nothing the reader can see
   *  has changed, which is what keeps opening the dialog on a document carrying
   *  `edit="none"` from rewriting it. */
  async function apply() {
    const doc = io.getDoc();
    if (!doc) return;
    const state = current();
    const row = PROTECTION_LEVELS.find((level) => level.value === (group.value() ?? "off"));
    // `formatting ? … : state.formatting` and NOT `formatting?.checked === true`.
    // The control is optional markup — a host may compose this dialog down, and
    // `createDocumentProtection` returns the same shape when the whole dialog is
    // absent — so an absent checkbox must mean "the reader was not asked", which
    // carries the document's own flag through. Reading `?.checked` would answer
    // `false` for "not asked" and switch a restriction the file carries off behind
    // the reader, which is the silent loss this dialog's previous note promised it
    // did not do and `w:formatting is carried through…` has guarded since.
    const wantedFormatting = formatting ? formatting.checked === true : state.formatting;
    // **The element has to EXIST to carry a formatting restriction.** A reader who
    // ticks only the formatting box has chosen Word's pure formatting restriction,
    // which Word writes as `w:edit="none" w:formatting="1" w:enforcement="1"` — so
    // `edit` becomes the explicit `"none"` token rather than `null`, because `null`
    // removes `w:documentProtection` outright and there is then nowhere for
    // `w:formatting` to live. The two are different states and this module already
    // keeps them apart everywhere else; this is the one place the SECOND axis
    // decides which of them is meant.
    const wantedEdit = row?.edit ?? (wantedFormatting ? "none" : null);
    const wantedEnforcement = wantedEdit !== null && enforce?.checked === true;
    // Compared on what RESTRICTS, not on the raw token. `edit=null` (the element
    // absent) and `edit="none"` (a restriction an author set up and switched off)
    // are different states that both restrict nothing, and the radio group has one
    // row for the pair — so a reader who opens this dialog on a `none` document and
    // presses Apply has chosen nothing and must get no write. Comparing the tokens
    // directly made `null === "none"` false and rewrote the file, which is what
    // both this function's note above and `readProtection`'s said it did not do.
    //
    // `wantedFormatting` joins the comparison rather than riding past it: without
    // it, ticking the formatting box on a document whose editing level is unchanged
    // would be read as "nothing the reader can see has changed" and silently
    // discarded — which is the same defect the token comparison had, in the new axis.
    const sameRestriction =
      wantedEdit === state.edit || (!restricts(wantedEdit) && !restricts(state.edit));
    const newPassword = wantedEdit !== null ? (newInput?.value ?? "") : "";
    if (
      sameRestriction &&
      wantedEnforcement === state.enforcement &&
      wantedFormatting === state.formatting &&
      newPassword === ""
    ) {
      modal.close();
      return;
    }
    if (newPassword !== (wantedEdit !== null ? (confirmInput?.value ?? "") : "")) {
      refusePassword(t("protect.passwordMismatch"), confirmInput);
      return;
    }
    // Word's Stop Protection asks for the password before anything changes, and
    // so does this — checked here first so the refusal is said beside the field
    // and the dialog stays open, then checked again by the engine at the write.
    const password = passwordState();
    const currentPassword = currentInput?.value ?? "";
    if (password.verifiable) {
      if (currentPassword === "") {
        refusePassword(t("protect.passwordRequired"), currentInput);
        return;
      }
      if (doc.checkProtectionPassword(currentPassword) === "mismatch") {
        refusePassword(t("protect.wrongPassword"), currentInput);
        return;
      }
    }
    // NO `gate`. See the ordering-trap note in this module's header — gating a
    // policy change behind the suggesting-mode check would rebuild the one-way door
    // ADR-059 removed in the engine.
    const applied = await io.runEdit(() =>
      typeof doc.protectDocument === "function"
        ? doc.protectDocument(
            wantedEdit,
            wantedEnforcement,
            wantedFormatting,
            currentPassword || null,
            newPassword || null,
            newPassword ? globalThis.crypto.getRandomValues(new Uint8Array(16)) : null,
          )
        : doc.setDocumentProtection(wantedEdit, wantedEnforcement, wantedFormatting),
    );
    if (!applied) return;
    modal.close();
    reflect();
    io.onChanged();
    // Three outcomes rather than two, because the formatting axis can be the ONLY
    // thing in force and "Editing restricted to: No restriction" would be a
    // sentence that contradicts itself. What is said is what will now be refused.
    const status = statusFor(wantedEdit, wantedEnforcement, wantedFormatting, row);
    if (newPassword && wantedEnforcement && restricts(wantedEdit)) {
      io.setStatus(t("protect.appliedWithPassword", { level: t(row.labelKey) }));
    } else {
      // Only an UNCHECKABLE password is removed silently by a change, and that is
      // the case the status names; a checked one was asked for and the reader knows.
      io.setStatus(state.password && !password.verifiable ? t("protect.withPasswordRemoved", { status }) : status);
    }
  }

  /** What the status bar says after Apply.
   *
   *  Pure and separate so the three outcomes can be read — and tested — without a
   *  dialog: nothing in force, the formatting axis alone, and an editing level
   *  (which absorbs the formatting axis into its own sentence, because a reader who
   *  has just been told the document is read-only does not also need to be told its
   *  formatting is). O(1). */
  function statusFor(edit, enforcement, formattingWanted, row) {
    if (edit === null || !enforcement) return t("protect.removed");
    if (!restricts(edit)) {
      return formattingWanted ? t("protect.appliedFormatting") : t("protect.removed");
    }
    return t("protect.applied", { level: t(row.labelKey) });
  }

  /** Nothing to reflect HERE, deliberately.
   *
   *  The ribbon button's `disabled`, the reason it carries while disabled and its
   *  pressed state are all declared in `main.js`'s `REVIEW_SURFACE` row, which is
   *  the one owner of those three attributes for every control on that band. A
   *  second writer here is how a control ends up enabled with the wrong tooltip —
   *  the mistake that table's own comments record. What this module owns is the
   *  dialog, the engine call and what the reader is told.
   *
   *  Kept as a no-op function so the returned shape is the same whether the markup
   *  is present or composed away, and so a caller never needs a null check. */
  function reflect() {}

  applyBtn?.addEventListener("click", () => void apply());
  cancelBtn?.addEventListener("click", () => modal.close());
  closeBtn?.addEventListener("click", () => modal.close());
  stopBtn?.addEventListener("click", stop);
  enforce?.addEventListener("change", reflectEnforce);
  // The formatting box changes whether ANYTHING is restricted, so it changes the
  // enforcement checkbox's own precondition. Without this, ticking it left Apply's
  // enforcement control disabled until some other event reflected, and Word's pure
  // formatting restriction was unreachable through this dialog even with the engine
  // enforcing it. Found by the guard, not by reading the code.
  formatting?.addEventListener("change", reflectEnforce);

  return {
    open,
    reflect,
    state: current,
    isActive: () => current().active,
    /** The one command row. The label is Word's own menu wording, and the row is
     *  live whenever a document is open — including on a document that is ALREADY
     *  read-only-protected, which is the whole point: a restriction nobody can
     *  reach the dialog to lift is worse than no restriction at all.
     *
     *  EXCEPT for a participant the room did not grant `manageProtection`
     *  (ADR-060, `152` §10 Q5). That row was `requires: "doc"` with no reference
     *  to the grant, so it stayed enabled for someone who may not change the
     *  policy and the refusal arrived only after they had chosen a level and
     *  pressed Apply. The reason comes in as `io.participantRefusal` — a sentence
     *  this module does not write, like every other sentence it shows — and it
     *  outranks "open a document first" by the one rule `session_access.mjs`
     *  states for every control with two reasons to refuse: the PERMANENT truth
     *  wins, because advice the reader can follow is worthless when following it
     *  cannot help. */
    commands: () => {
      const reason = io.participantRefusal?.() || (io.getDoc() ? "" : t("command.needsDocument"));
      return [
        {
          id: "review.restrictEditing",
          label: t("protect.command"),
          group: "Review",
          kw: "restrict editing protect protection password stop read only comments tracked changes forms lock permission policy",
          enabled: !reason,
          disabledReason: reason,
          run: open,
        },
      ];
    },
  };
}
