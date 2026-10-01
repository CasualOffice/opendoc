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
// ---- NO PASSWORD FIELD, AND THAT IS A DECISION -----------------------------
//
// Both references offer one and this deliberately does not. `w:documentProtection`
// can carry `w:hash`, `w:salt`, `w:cryptProviderType`, `w:cryptAlgorithmSid` and
// `w:cryptSpinCount`; ADR-052 records that none of it is modelled, and that
// verifying one would advertise a boundary that does not exist — the legacy hash
// is removable by editing a single XML attribute, and a local-first engine hands
// the reader the bytes regardless. A password box here would promise protection
// against an adversary this product does not have.
//
// What the dialog does instead is SAY SO, in the pane, where the password box
// would have been: this is a policy and anyone who can open the document can lift
// it. That is literally how Word behaves with an unpassworded restriction, and
// ADR-052's "policy, not security" has to stay true in what the reader is told,
// not only in an ADR.
//
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
    /** Whether a restriction is actually in force, which is the ribbon button's
     *  pressed state. `edit="none"` restricts nothing however it is enforced. */
    active: edit !== null && edit !== "none" && parsed?.enforcement === true,
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
  const applyBtn = el("restrictEditingApply");
  const cancelBtn = el("restrictEditingCancel");
  const closeBtn = el("restrictEditingClose");

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
   *  disabled CARRYING THE REASON rather than greyed in silence. */
  function reflectEnforce() {
    if (!enforce) return;
    const off = (group.value() ?? "off") === "off";
    enforce.disabled = off;
    if (off) enforce.checked = false;
    if (enforceRow) enforceRow.title = off ? t("protect.enforceOff") : "";
  }

  function open() {
    if (!io.getDoc()) return;
    const state = current();
    group.reflect(state.value);
    if (enforce) enforce.checked = state.enforcement;
    reflectEnforce();
    modal.open();
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
    const wantedEdit = row?.edit ?? null;
    const wantedEnforcement = wantedEdit !== null && enforce?.checked === true;
    // Compared on what RESTRICTS, not on the raw token. `edit=null` (the element
    // absent) and `edit="none"` (a restriction an author set up and switched off)
    // are different states that both restrict nothing, and the radio group has one
    // row for the pair — so a reader who opens this dialog on a `none` document and
    // presses Apply has chosen nothing and must get no write. Comparing the tokens
    // directly made `null === "none"` false and rewrote the file, which is what
    // both this function's note above and `readProtection`'s said it did not do.
    const sameRestriction =
      wantedEdit === state.edit || (!restricts(wantedEdit) && !restricts(state.edit));
    if (sameRestriction && wantedEnforcement === state.enforcement) {
      modal.close();
      return;
    }
    // `formatting` (`w:formatting`, Word's "limit formatting to a selection of
    // styles") is carried through untouched rather than defaulted: this dialog does
    // not offer the style whitelist, so Apply must not switch a restriction the
    // document already carries off behind the reader's back.
    //
    // NO `gate`. See the ordering-trap note in this module's header — gating a
    // policy change behind the suggesting-mode check would rebuild the one-way door
    // ADR-059 removed in the engine.
    const applied = await io.runEdit(() =>
      doc.setDocumentProtection(wantedEdit, wantedEnforcement, state.formatting),
    );
    if (!applied) return;
    modal.close();
    reflect();
    io.onChanged();
    io.setStatus(
      wantedEdit === null || !wantedEnforcement
        ? t("protect.removed")
        : t("protect.applied", { level: t(row.labelKey) }),
    );
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
  enforce?.addEventListener("change", reflectEnforce);

  return {
    open,
    reflect,
    state: current,
    isActive: () => current().active,
    /** The one command row. The label is Word's own menu wording, and the row is
     *  live whenever a document is open — including on a document that is ALREADY
     *  read-only-protected, which is the whole point: a restriction nobody can
     *  reach the dialog to lift is worse than no restriction at all. */
    commands: () => {
      const reason = io.getDoc() ? "" : t("command.needsDocument");
      return [
        {
          id: "review.restrictEditing",
          label: t("protect.command"),
          group: "Review",
          kw: "restrict editing protect protection read only comments tracked changes forms lock permission policy",
          enabled: !reason,
          disabledReason: reason,
          run: open,
        },
      ];
    },
  };
}
