// SPDX-License-Identifier: Apache-2.0
// Header and footer commands that act on ONE band of the page in view: Link to
// Previous, Remove, and the six page-number positions (`docs/109` HF-192 and
// RM-23, `docs/168` batch 8).
//
// ---- THE SHAPE, FROM THE REFERENCES ----------------------------------------
//
//   * ONLYOFFICE's Header & Footer tab (`apps/documenteditor/main/app/view/
//     HeaderFooterTab.js`) carries "Link to previous" (`chSameAs`, L219) beside
//     the two variant switches, disabled — not hidden — on the first section
//     (`controller/HeaderFooterTab.js:247`). Its Insert ▸ Page Number gallery is
//     six positions, top and bottom by left, centre and right, plus "To current
//     position" (`DE.Views.HeaderFooterTab.capCurrentPos`); the toolbar's
//     header menu has Remove header / Remove footer (`mniRemoveHeader`).
//   * Word: Header & Footer ▸ Link to Previous on the contextual tab; Insert ▸
//     Page Number ▸ Top / Bottom of Page; Header ▸ Remove Header.
//   * Google Docs: the band carries its own small chrome while it is open — a
//     "Different first page" checkbox and an Options menu with Page numbers and
//     Remove header — rather than a ribbon tab.
//
// This editor has no contextual Header & Footer tab to spend (the ribbon must
// fit 1280px, `ribbon-width-budget.spec.mjs`), so it takes Docs' shape for the
// in-place chrome (`editor_header_footer.mjs` draws it on the open band) and
// ONLYOFFICE's and Word's command set. Every command here is ALSO an Insert-menu
// row and a palette row, from this one table, so none is reachable from one
// surface only (SKILL §10).
//
// The ids are band-explicit (`layout.header.linkToPrevious`,
// `layout.footer.linkToPrevious`) because linkage is per band and per variant
// (`docs/85` §8d Q7): a section whose header is inherited while its footer is
// its own is a real document, and one command for "this band" would be
// meaningless from the palette, where no band is open.
//
// Pure: no DOM, no document. Complexity O(1) per row.
import { t } from "./i18n.mjs";

/** The six page-number positions, in the order ONLYOFFICE's gallery and Word's
 *  Top of Page / Bottom of Page submenus list them. `band` and `align` are the
 *  engine's own `insertRunningPageNumber` vocabulary. */
export const PAGE_NUMBER_POSITIONS = Object.freeze([
  Object.freeze({ command: "layout.pageNumber.topLeft", band: "header", align: "left", labelKey: "pageNumber.topLeft" }),
  Object.freeze({ command: "layout.pageNumber.topCenter", band: "header", align: "center", labelKey: "pageNumber.topCenter" }),
  Object.freeze({ command: "layout.pageNumber.topRight", band: "header", align: "right", labelKey: "pageNumber.topRight" }),
  Object.freeze({ command: "layout.pageNumber.bottomLeft", band: "footer", align: "left", labelKey: "pageNumber.bottomLeft" }),
  Object.freeze({ command: "layout.pageNumber.bottomCenter", band: "footer", align: "center", labelKey: "pageNumber.bottomCenter" }),
  Object.freeze({ command: "layout.pageNumber.bottomRight", band: "footer", align: "right", labelKey: "pageNumber.bottomRight" }),
]);

/** Link to Previous and Remove, one row per band. */
export const BAND_COMMANDS = Object.freeze([
  Object.freeze({ command: "layout.header.linkToPrevious", band: "header", action: "link" }),
  Object.freeze({ command: "layout.footer.linkToPrevious", band: "footer", action: "link" }),
  Object.freeze({ command: "layout.header.remove", band: "header", action: "remove" }),
  Object.freeze({ command: "layout.footer.remove", band: "footer", action: "remove" }),
]);

/** Every command id this module declares, for the host contract and the guards. */
export const HEADER_FOOTER_COMMAND_IDS = Object.freeze([
  ...BAND_COMMANDS.map((row) => row.command),
  ...PAGE_NUMBER_POSITIONS.map((row) => row.command),
]);

/** The engine's `runningLinkState` answer, parsed. `null` when there is no
 *  document or the answer cannot be read; `linked: null` on the first section's
 *  pages, which have no previous section. */
export function parseLinkState(raw) {
  if (typeof raw !== "string" || raw === "null") return null;
  try {
    const state = JSON.parse(raw);
    if (!state || typeof state !== "object") return null;
    return {
      section: Number(state.section) || 1,
      kind: state.kind === "first" || state.kind === "even" ? state.kind : "default",
      linked: state.linked === true ? true : state.linked === false ? false : null,
      shown: state.shown === true,
    };
  } catch {
    return null;
  }
}

/** The palette label of one row. A link row reads as a SWITCH with its state,
 *  the way the two variant toggles do ("Different first page: on"), because the
 *  palette has no checkbox to show it. Whole sentences per state, for the reason
 *  `en_strings.mjs` gives at `headerFooter.firstPageOn`. */
export function bandCommandLabel(row, state) {
  if (row.action === "remove") {
    return row.band === "header" ? t("headerFooter.removeHeader") : t("headerFooter.removeFooter");
  }
  const on = state?.linked === true;
  if (row.band === "header") {
    return on ? t("headerFooter.linkHeaderOn") : t("headerFooter.linkHeaderOff");
  }
  return on ? t("headerFooter.linkFooterOn") : t("headerFooter.linkFooterOff");
}

/** Why a row cannot run on this page, or `""` when it can. Shared by the
 *  palette's `disabledReason`, the menu row and the band checkbox's title, so
 *  the three faces give one reason. */
export function bandCommandReason(row, state) {
  if (!state) return t("command.needsDocument");
  if (row.action === "link" && state.linked === null) return t("headerFooter.firstSectionNoPrevious");
  if (row.action === "remove" && !state.shown) {
    return row.band === "header" ? t("headerFooter.noHeaderToRemove") : t("headerFooter.noFooterToRemove");
  }
  return "";
}

/** The band label Word draws ("Header -Section 2-", "Same as Previous"), as
 *  data: one sentence per case, because word order around the number differs by
 *  language. One section shows the bare name, as Word does. */
export function bandLabel(band, bands) {
  const header = band === "header";
  if (!bands || (bands.sectionCount ?? 1) <= 1) {
    return header ? t("runningBand.header") : t("runningBand.footer");
  }
  const linked = header ? bands.headerLinked : bands.footerLinked;
  const number = bands.section;
  if (linked) {
    return header
      ? t("runningBand.headerSectionLinked", { number })
      : t("runningBand.footerSectionLinked", { number });
  }
  return header ? t("runningBand.headerSection", { number }) : t("runningBand.footerSection", { number });
}
