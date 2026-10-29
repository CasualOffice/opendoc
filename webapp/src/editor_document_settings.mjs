// Document-level settings dialogs: document properties (docProps/core.xml),
// page setup with line numbers and watermark, and header and footer settings.
// Moved out of `main.js` unchanged (docs/177); it reaches the rest of the
// editor only through the context `ed`.
import { createHeaderFooterSettings } from "./header_footer_settings.mjs";
import { createPageSetup } from "./page_setup.mjs";
import { createPropertiesDialog } from "./document_metadata.mjs";
import { registerModal, setModalHooks } from "./modal.mjs";
import { registerPopover } from "./popover_manager.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createDocumentSettings(ed) {
  // ---- Document properties (docProps/core.xml — title, author, subject, …) ----
  // The whole dialog is `document_metadata.mjs`: the six editable fields, the
  // seventeen read-only `docProps` cells beside them, and the one transaction Apply
  // runs. Splitting one dialog across two files would have been worse than moving
  // it whole, and the File page shows the same element as a pane.
  const propertiesUi = createPropertiesDialog({ registerModal, getDoc: () => ed.doc, runEdit: ed.runEdit });

  // Chrome that must never survive into a modal's foreground (docs/104 HF-089):
  // a pinned tracked-change card used to paint above the scrim with live
  // Accept/Reject buttons, so a change could be accepted while a blocking dialog
  // was supposedly in control. The z-index ladder now puts modals on top, and
  // this hook makes the floating review chrome go away entirely — one place,
  // rather than teaching every dialog's open path about every popover.
  setModalHooks({
    firstOpen: () => {
      ed.closeReviewPopover();
      ed.closeReviewInlineCard();
      ed.closeAppMenu();
    },
  });

  // ---- Page setup, line numbers, watermark -----------------------------------
  // Word's Layout ▸ Page Setup group, in `page_setup.mjs`: the geometry dialog, the
  // Line Numbers popover and the Watermark dialog, which share one answer to
  // "which section is the caret in". None of the three is decided here any more.
  const pageSetup = createPageSetup({
    getDoc: () => ed.doc,
    // The same inventory the Home band's font menu offers, so the watermark's Font
    // list cannot name a face no other control here can.
    fontInventory: ed.fontInventory,
    selectionNode: () => ed.selection?.focus?.node ?? "",
    selectionEndpoints: ed.selEndpoints,
    runEdit: ed.runEdit,
    registerModal,
    registerPopover,
    // Word's *Measurement units* setting governs exactly this dialog, so the
    // preference arrives as a dependency rather than being read from a global.
    measure: ed.measurement,
  });

  // ---- Header and footer settings --------------------------------------------
  // The band distances, the two running-content variants and the section's page
  // numbering, in `header_footer_settings.mjs`: ONLYOFFICE's own grouping of the
  // four (`view/HeaderFooterTab.js` L63-87). The two variant toggles moved there
  // from here, so the checkbox faces in the dialog and the button faces on the
  // Insert band are one implementation rather than two that can disagree.
  const headerFooterSettings = createHeaderFooterSettings({
    getDoc: () => ed.doc,
    selectionNode: () => ed.selection?.focus?.node ?? "",
    runningPage: ed.runningEditPageOrView,
    runEdit: ed.runEdit,
    applyEditResult: ed.applyEditResult,
    blockedFromMutating: ed.blockedFromPageSetup,
    setStatus: ed.setStatus,
    registerModal,
    // Link to previous, per band (HF-192): the same commands as the open band's bar.
    band: () => ed.headerFooterBand,
  });

  return { headerFooterSettings, pageSetup, propertiesUi };
}
