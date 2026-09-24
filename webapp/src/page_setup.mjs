// Word's Layout ▸ Page Setup group: the page-geometry dialog, and line numbers.
//
// Extracted from `main.js` under `109` HF-085, and for the reason the ratchet in
// `module_seams.test.mjs` exists — the file was AT its ceiling, and line
// numbering needed somewhere to live. It is the same candidate shape as
// `bookmark_manager.mjs`: a surface that owns its own markup and its own state,
// whose entire need of the application is a handful of verbs it can be handed.
//
// The two belong together rather than in two modules. In Word they are one
// group, and here they are additionally one QUESTION — "which section is the
// caret in" — answered by one engine call (`section_of`, shared behind
// `pageSetupSections` and `lineNumbering`). Two modules would have meant two
// copies of the section list and two chances to show one section's values while
// writing another's, which is the defect the engine side of this change fixed.
//
// What did NOT come across: `updateToolbar`'s enable/disable sweep and the
// review-mode gate. Both are about the application's state rather than about
// page setup, so they stay where the rest of that logic is and arrive here as
// `io.runEdit` (which gates) and `setEnabled`.
import { TWIPS_PER_INCH, inchesToTwips } from "./units.mjs";
import { t } from "./i18n.mjs";

/** The Line Numbers presets, in Word's order. `rule` is merged over the
 *  section's current numbering, so switching mode keeps Start at / Count by /
 *  From text — changing "restart each page" to "continuous" should not silently
 *  reset the other three fields.
 *
 *  `null` is None, and None is the EMPTY rule rather than a rule that numbers
 *  nothing: import, layout and export all read the empty value as "no line
 *  numbering" (`casual-doc-layout/src/line_number.rs`), so anything else would
 *  read as off here while exporting a `w:lnNumType` no other producer writes. */
const LINE_NUMBER_MODES = new Map([
  ["none", null],
  ["continuous", { restart: "continuous" }],
  ["newPage", { restart: "newPage" }],
  ["newSection", { restart: "newSection" }],
]);

/**
 * Mounts the Page Setup dialog and the Line Numbers popover over the markup
 * already in the page.
 *
 * `io` is its entire contact with the application:
 *
 *   `getDoc()`            the open document, or null
 *   `selectionNode()`     the focus node id, or "" when there is no selection
 *   `selectionEndpoints()` `[sNode, sOff, eNode, eOff]`, or null
 *   `runEdit(thunk, options)` Promise; applies an engine edit, repaints, and
 *                         refuses it in the review modes that forbid it
 *   `registerModal(dialog, options)`  the host's modal registry
 *   `registerPopover(button, menu, reflect)` the host's popover manager, which
 *                         is what gives a new menu the light-dismiss contract
 *                         for free (`light-dismiss-contract.spec.mjs`)
 */
export function createPageSetup(io) {
  const el = (id) => document.getElementById(id);

  // ---- Page setup (page size, margins, orientation, columns) ---------------
  const pageSetupBtn = el("pageSetupBtn");
  const pageSetupMenu = el("pageSetupMenu");
  const orientationSeg = el("pageOrientationSeg");
  const widthInput = el("pageWidth");
  const heightInput = el("pageHeight");
  const marginTop = el("pageMarginTop");
  const marginBottom = el("pageMarginBottom");
  const marginLeft = el("pageMarginLeft");
  const marginRight = el("pageMarginRight");
  const applyBtn = el("pageSetupApply");
  const cancelBtn = el("pageSetupCancel");
  const closeBtn = el("pageSetupClose");
  const previewSheet = el("pagePreviewSheet");
  const previewMargins = el("pagePreviewMargins");
  const previewLabel = el("pagePreviewLabel");
  const sectionSelect = el("pageSetupSection");
  const columnCount = el("pageColumnCount");
  const columnGap = el("pageColumnGap");
  const columnSeparator = el("pageColumnSeparator");

  // ---- Line numbers --------------------------------------------------------
  const lineNumbersBtn = el("lineNumbersBtn");
  const lineNumbersMenu = el("lineNumbersMenu");
  const lineNumberSuppress = el("lineNumberSuppress");
  const lineNumberStart = el("lineNumberStart");
  const lineNumberCountBy = el("lineNumberCountBy");
  const lineNumberDistance = el("lineNumberDistance");

  if (!pageSetupMenu) return { open: () => {}, setEnabled: () => {} };

  /** The section whose geometry the dialog is currently showing. */
  let current = null;
  /** Which control this opening should land on — Layout ▸ Margins and
   *  Layout ▸ Columns are the same dialog reached with a different intent, and
   *  both Word and Docs put you on the field you asked for. Landing everyone on
   *  Orientation would make three of the four buttons feel like the wrong one.
   *  Reset on every open, so a deep link cannot leak into the next plain one. */
  let focusIntent = null;

  /** Where each opening intent lands. */
  const INTENTS = new Map([
    ["orientation", () => orientationSeg.querySelector('button[aria-pressed="true"]')],
    ["margins", () => marginTop],
    ["size", () => widthInput],
    ["columns", () => columnCount],
  ]);

  /** An inches field's value as twips. */
  const fieldTwips = (input) => inchesToTwips(input.value);

  /** Twips → an inches string for a page-geometry field. Unlike the general
   *  helper, 0 shows as "0": a page dimension or margin is never meaningfully
   *  unset, so blanking one would read as "inherited" when it is not. */
  function inchText(twip) {
    return (twip / TWIPS_PER_INCH).toFixed(2).replace(/\.?0+$/, "") || "0";
  }

  function reflectColumns(columns) {
    const value = columns ?? { count: 1, spaceTwips: 0, separator: false };
    columnCount.value = String(Math.min(4, Math.max(1, value.count ?? 1)));
    columnGap.value = inchText(value.spaceTwips ?? 0);
    columnSeparator.checked = value.separator === true;
  }

  function columnsPayload() {
    const previous = current.columns;
    const count = Number(columnCount.value) || 1;
    const spaceTwips = fieldTwips(columnGap);
    const separator = columnSeparator.checked;
    // Opening Page Setup and changing only page size/margins must not erase
    // explicit unequal column widths. Normalize to equal columns only when a
    // column control itself actually changed.
    if (
      previous &&
      count === previous.count &&
      spaceTwips === (previous.spaceTwips ?? 0) &&
      separator === (previous.separator === true)
    ) {
      return previous;
    }
    return { ...(previous ?? {}), count, spaceTwips, separator, equalWidth: true, columns: [] };
  }

  function updatePreview() {
    const width = Math.max(1, Number(widthInput.value) || 1);
    const height = Math.max(1, Number(heightInput.value) || 1);
    const top = Math.max(0, Number(marginTop.value) || 0);
    const bottom = Math.max(0, Number(marginBottom.value) || 0);
    const left = Math.max(0, Number(marginLeft.value) || 0);
    const right = Math.max(0, Number(marginRight.value) || 0);
    const percent = (value, dimension) =>
      `${Math.min(38, Math.max(3, (value / dimension) * 100))}%`;

    previewSheet.dataset.orientation = width > height ? "landscape" : "portrait";
    previewSheet.style.setProperty("--page-ratio", `${width} / ${height}`);
    previewMargins.style.setProperty("--preview-margin-top", percent(top, height));
    previewMargins.style.setProperty("--preview-margin-bottom", percent(bottom, height));
    previewMargins.style.setProperty("--preview-margin-left", percent(left, width));
    previewMargins.style.setProperty("--preview-margin-right", percent(right, width));
    previewLabel.textContent = t("pageSetup.dimensions", {
      width: inchText(width * TWIPS_PER_INCH),
      height: inchText(height * TWIPS_PER_INCH),
    });
  }

  /** The section list from the engine, or null. */
  function sections() {
    const doc = io.getDoc();
    if (!doc) return null;
    const raw = doc.pageSetupSections(io.selectionNode());
    const list = raw === "null" ? null : JSON.parse(raw);
    return list?.sections?.length ? list : null;
  }

  /** Paints every field from one section's geometry. Both the initial reflect
   *  and the Section dropdown's change handler come through here; they were two
   *  copies of the same fourteen lines, which is how the column fields came to
   *  be repainted in one and not the other. */
  function paintSection(section) {
    current = section;
    const { pageSize, pageMargins, orientation } = section;
    widthInput.value = inchText(pageSize.widthTwips);
    heightInput.value = inchText(pageSize.heightTwips);
    marginTop.value = inchText(pageMargins.topTwips);
    marginBottom.value = inchText(pageMargins.bottomTwips);
    marginLeft.value = inchText(pageMargins.startTwips);
    marginRight.value = inchText(pageMargins.endTwips);
    reflectColumns(section.columns);
    const active =
      orientation ?? (pageSize.widthTwips > pageSize.heightTwips ? "landscape" : "portrait");
    for (const btn of orientationSeg.querySelectorAll("button")) {
      btn.setAttribute("aria-pressed", String(btn.dataset.orientation === active));
    }
    updatePreview();
  }

  /** Fills the dialog from the document. False when there is no section
   *  geometry to edit, which is what stops the dialog opening empty. */
  function reflect() {
    const list = sections();
    if (!list) return false;
    sectionSelect.replaceChildren();
    for (const [index, section] of list.sections.entries()) {
      const option = document.createElement("option");
      option.value = section.section;
      option.textContent = t("pageSetup.sectionNumber", { number: index + 1 });
      sectionSelect.appendChild(option);
    }
    sectionSelect.value = list.current;
    paintSection(
      list.sections.find((section) => section.section === list.current) ?? list.sections[0],
    );
    return true;
  }

  const modal = io.registerModal(pageSetupMenu, {
    initialFocus: () =>
      focusIntent?.() ?? orientationSeg.querySelector('button[aria-pressed="true"]'),
    fallbackFocus: () => pageSetupBtn,
  });

  /** Opens the dialog on the field `intent` asks for, or closes it.
   *  `intent` is a name rather than an element so that no caller needs to know
   *  which id holds which field. */
  function toggle(open, intent = null) {
    const show = open ?? !modal.isOpen;
    if (show === modal.isOpen) return;
    if (show && !reflect()) return; // no section geometry to edit
    focusIntent = show ? (INTENTS.get(intent) ?? null) : null;
    pageSetupBtn.setAttribute("aria-expanded", String(show));
    if (show) modal.open();
    else modal.close();
  }

  sectionSelect.addEventListener("change", () => {
    const list = sections();
    const picked = list?.sections?.find((section) => section.section === sectionSelect.value);
    if (picked) paintSection(picked);
  });

  pageSetupBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    toggle();
  });

  orientationSeg.addEventListener("click", (e) => {
    const btn = e.target.closest("button[data-orientation]");
    if (!btn) return;
    for (const b of orientationSeg.querySelectorAll("button")) {
      b.setAttribute("aria-pressed", String(b === btn));
    }
    // Swap width/height to match, mirroring Word's orientation toggle.
    const w = Number(widthInput.value) || 0;
    const h = Number(heightInput.value) || 0;
    if ((btn.dataset.orientation === "landscape") === w > h) return; // already matches
    const widthTwips = fieldTwips(widthInput);
    const heightTwips = fieldTwips(heightInput);
    widthInput.value = inchText(heightTwips);
    heightInput.value = inchText(widthTwips);
    updatePreview();
  });

  cancelBtn.addEventListener("click", () => toggle(false));
  closeBtn.addEventListener("click", () => toggle(false));
  for (const input of [widthInput, heightInput, marginTop, marginBottom, marginLeft, marginRight]) {
    input.addEventListener("input", updatePreview);
  }

  applyBtn.addEventListener("click", async () => {
    const doc = io.getDoc();
    if (!doc || !current) return;
    const payload = {
      section: current.section,
      pageSize: { widthTwips: fieldTwips(widthInput), heightTwips: fieldTwips(heightInput) },
      pageMargins: {
        ...current.pageMargins,
        topTwips: fieldTwips(marginTop),
        bottomTwips: fieldTwips(marginBottom),
        startTwips: fieldTwips(marginLeft),
        endTwips: fieldTwips(marginRight),
      },
      columns: columnsPayload(),
      orientation:
        orientationSeg.querySelector('button[aria-pressed="true"]')?.dataset.orientation ??
        "portrait",
    };
    await io.runEdit(() => doc.setPageSetup(JSON.stringify(payload)), { gate: true });
    toggle(false);
  });

  // ---- Line numbers -------------------------------------------------------
  // The whole control is one popover, presets above the fields, which is the
  // shape `spacingMenu` already established here for "four common answers, plus
  // the exact ones". Word splits it into a dropdown and a modal; copying that
  // would put a second dismissal contract on screen for four radio buttons.

  if (!lineNumbersMenu) {
    return { open: toggle, setEnabled: (on) => void (pageSetupBtn.disabled = !on) };
  }

  /** The section's line numbering as the engine reports it, or null. */
  function lineNumbering() {
    const doc = io.getDoc();
    if (!doc) return null;
    const raw = doc.lineNumbering(io.selectionNode());
    return raw === "null" ? null : JSON.parse(raw);
  }

  /** Which preset a rule reads as. A rule with no `countBy` is off, whatever
   *  else it carries: `countBy` is what the engine steps the count by, and the
   *  empty rule is how numbering is turned off. */
  function modeOf(rule) {
    if (!rule || rule.countBy === null || rule.countBy === undefined) return "none";
    return rule.restart ?? "newPage"; // the schema default when `@w:restart` is absent
  }

  function reflectLineNumbers() {
    const rule = lineNumbering();
    const mode = modeOf(rule);
    for (const item of lineNumbersMenu.querySelectorAll("[data-linenumber]")) {
      item.setAttribute("aria-checked", String(item.dataset.linenumber === mode));
    }
    lineNumberSuppress.checked = rule?.suppressed === true;
    lineNumberStart.value = String(rule?.start ?? 1);
    lineNumberCountBy.value = String(rule?.countBy ?? 1);
    // `w:distance` absent is Word's "Auto", which it resolves against the text;
    // layout uses 0.25in for it, so that is what an empty field shows.
    lineNumberDistance.value = inchText(rule?.distance ?? 360);
  }

  io.registerPopover(lineNumbersBtn, lineNumbersMenu, reflectLineNumbers);

  /** Writes the section's numbering, merging `patch` over what is there.
   *  `null` turns numbering off by installing the empty rule. */
  async function writeLineNumbering(patch) {
    const doc = io.getDoc();
    const rule = lineNumbering();
    if (!doc || !rule) return;
    const payload =
      patch === null
        ? { section: rule.section }
        : {
            section: rule.section,
            // A mode is only "on" if the count steps, so a preset that does not
            // say otherwise numbers every line.
            countBy: rule.countBy ?? 1,
            start: rule.start,
            distance: rule.distance,
            restart: rule.restart,
            ...patch,
          };
    await io.runEdit(() => doc.setLineNumbering(JSON.stringify(payload)), { gate: true });
    reflectLineNumbers();
  }

  lineNumbersMenu.addEventListener("click", async (e) => {
    const preset = e.target.closest("[data-linenumber]");
    if (!preset) return;
    await writeLineNumbering(LINE_NUMBER_MODES.get(preset.dataset.linenumber) ?? null);
  });

  lineNumberSuppress.addEventListener("change", async () => {
    const doc = io.getDoc();
    const range = io.selectionEndpoints();
    if (!doc || !range) return;
    const on = lineNumberSuppress.checked;
    await io.runEdit(() => doc.setSuppressLineNumbers(...range, on), { gate: true });
    reflectLineNumbers();
  });

  // The three numeric fields commit on change rather than on every keystroke: a
  // repagination per digit would make typing "120" repaginate at 1 and at 12.
  for (const [input, field, scale] of [
    [lineNumberStart, "start", 1],
    [lineNumberCountBy, "countBy", 1],
    [lineNumberDistance, "distance", TWIPS_PER_INCH],
  ]) {
    input.addEventListener("change", async () => {
      const rule = lineNumbering();
      if (modeOf(rule) === "none") return; // nothing to number yet
      const raw = scale === 1 ? Number(input.value) : fieldTwips(input);
      if (!Number.isFinite(raw)) return;
      // The engine refuses out-of-domain values and leaves the section alone;
      // clamping here means the field cannot silently do nothing instead.
      const max = field === "distance" ? 31_680 : 32_767;
      const min = field === "countBy" ? 1 : 0;
      await writeLineNumbering({ [field]: Math.min(max, Math.max(min, Math.round(raw))) });
    });
  }

  return {
    open: toggle,
    /** The palette's route to the popover. Clicking the trigger rather than
     *  opening directly keeps ONE path through the popover manager, so the
     *  light-dismiss contract and the `aria-expanded` state cannot diverge
     *  between the two ways in. */
    openLineNumbers: () => lineNumbersBtn.click(),
    /** Line numbers and page setup are both document-scoped, so they enable and
     *  disable together with the rest of the document-scoped chrome. */
    setEnabled(on) {
      pageSetupBtn.disabled = !on;
      lineNumbersBtn.disabled = !on;
    },
  };
}
