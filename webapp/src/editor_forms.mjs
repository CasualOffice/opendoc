// Content controls, Protect form and form roles (`docs/109` RM-34), wired to
// the editor context `ed` (docs/177). The surfaces are `content_controls.mjs`
// and `form_roles.mjs`; this module only hands them the editor's seams.
import { createContentControls } from "./content_controls.mjs";
import { createFormRoles } from "./form_roles.mjs";

/** Installs the forms domain on `ed` and returns what the rest of the editor
 *  calls. Runs once, from `main.js`. */
export function createForms(ed) {
  /** Puts a collapsed caret at `node`/`offset` and shows it — Go to, and the
   *  press that opens a chooser. */
  const select = (node, start, end = start) => {
    ed.selection = { anchor: { node, offset: start }, focus: { node, offset: end } };
    ed.drawSelection();
    ed.scrollCaretIntoView();
    ed.updateToolbar();
    ed.focusEditorSurface();
  };
  let contentControls = null;
  const formRoles = createFormRoles({
    getDoc: () => ed.doc,
    fileName: () => ed.currentName,
    runEdit: (thunk, options) => ed.runEdit(thunk, options),
    openRestrictEditing: () => ed.documentProtection.open(),
    select,
    setStatus: (text, kind) => ed.setStatus(text, kind),
    participantRefusal: () => ed.SESSION.refusalFor("review.protectForm"),
    changed: () => {
      contentControls?.forget();
      ed.updateToolbar();
    },
    fallbackFocus: () => document.getElementById("insertControlsBtn") ?? ed.pagesEl,
    kindLabel: (kind) => contentControls?.kindLabel({ kind }) ?? kind,
  });
  contentControls = createContentControls({
    getDoc: () => ed.doc,
    getSelection: () => ed.selection,
    select,
    runEdit: (thunk, options) => ed.runEdit(thunk, options),
    revision: () => ed.currentRevision,
    setStatus: (text, kind) => ed.setStatus(text, kind),
    roles: () => formRoles.roles(),
    protectForm: () => void formRoles.protectForm(),
    openRoles: () => formRoles.open(),
    fallbackFocus: () => document.getElementById("insertControlsBtn") ?? ed.pagesEl,
  });
  return { contentControls, formRoles };
}
