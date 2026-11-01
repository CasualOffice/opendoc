// The status-bar PDF layout chip (owner, 2026-11-01, `docs/109` UX-051).
//
// Switching a PDF between its flowing reconstruction and its exact page layout
// was reachable only from File ▸ PDF, the palette and the import summary: a
// reader looking at a PDF had no sign on screen of which of the two they were
// in, nor a short way to the other. Word shows nothing at all (it converts once
// and forgets the PDF); this is the status-bar readout-plus-menu shape the
// interface-language and zoom controls beside it already use.
//
// Shown ONLY while the open document is a PDF session (`pdfOrigin`); hidden
// otherwise, so a DOCX reader never sees it. The label names the layout in
// force and the button opens a two-row radio menu whose rows run the SAME
// registry rows as View ▸ Page display ▸ PDF layout (`pdf_commands.mjs`
// `layoutCommands`), so the chip, the menu and the palette cannot disagree.
//
// No new visual style: the chip is the text-language chip's class and the menu
// is the zoom menu's (`context-menu`, `menu-item`, `menu-check`). Keyboard:
// it is a button with `aria-haspopup="menu"`; Enter/Space opens the menu with
// focus on the checked row, arrows walk it (`menu_keys.mjs`), Escape closes
// and returns focus. Its label is written only when it is shown, after the
// catalogue has landed (`chrome-raw-keys.spec.mjs`).
import { t } from "./i18n.mjs";
import { menuArrowKeys } from "./menu_keys.mjs";
import { LAYOUT_EXACT_COMMAND, LAYOUT_FLOWING_COMMAND, pdfLayoutOf } from "./pdf_commands.mjs";

/** The glyph for each layout: text that wraps, or a page as drawn. Both are
 *  glyphs the chrome already uses, so the self-hosted icon subset has them. */
const ICON = { flowing: "wrap_text", exact: "fit_page" };

/**
 * Installs the chip on the editor context `ed` (docs/177).
 *
 * @param {object} ed reads `doc` and `pdfCommands`.
 * @param {object} io
 * @param {Function} io.registerPopover `popover_manager.mjs`'s.
 * @returns {{reflect: () => void}}
 */
export function createPdfLayoutChip(ed, { registerPopover }) {
  const chip = document.getElementById("pdfLayoutStatus");
  const label = document.getElementById("pdfLayoutStatusLabel");
  const icon = document.getElementById("pdfLayoutStatusIcon");
  const menu = document.getElementById("pdfLayoutMenu");
  if (!chip || !menu) return { reflect() {} };

  const rows = () => ed.pdfCommands?.layoutCommands?.() ?? [];

  /** Syncs the menu's checks from the registry rows, on every open. */
  function reflectMenu() {
    for (const row of rows()) {
      const item = menu.querySelector(`[data-command="${row.id}"]`);
      if (!item) continue;
      item.setAttribute("aria-checked", String(row.checked === true));
      item.disabled = row.enabled === false;
    }
  }

  const popover = registerPopover(chip, menu, reflectMenu);
  menu.addEventListener("click", (event) => {
    const item = event.target.closest("[data-command]");
    if (!item || item.disabled) return;
    const row = rows().find((candidate) => candidate.id === item.dataset.command);
    popover.close();
    // The checked row is the command's own refusal ("already open as …"), said
    // in the status line; anything else reopens the PDF in that layout.
    row?.run();
  });
  menuArrowKeys(menu, ".menu-item");

  /** Shows the chip for a PDF session, names its layout, and hides it for any
   *  other document. O(1); called on every open and every relabel. */
  function reflect() {
    const layout = pdfLayoutOf(ed.doc);
    chip.hidden = layout === "";
    if (!layout) {
      if (!menu.hidden) popover.close();
      return;
    }
    const name = t(layout === "exact" ? "pdfLayout.exact" : "pdfLayout.flowing");
    label.textContent = name;
    if (icon) icon.textContent = ICON[layout];
    const spoken = t("pdfLayout.status", { layout: name });
    chip.setAttribute("aria-label", spoken);
    chip.title = spoken;
    chip.dataset.layout = layout;
  }

  return { reflect };
}

/** The ids the chip's rows carry, for the parity test. */
export const PDF_LAYOUT_CHIP_COMMANDS = Object.freeze([LAYOUT_FLOWING_COMMAND, LAYOUT_EXACT_COMMAND]);
