// Word's Layout ▸ Page Setup group: the page-geometry dialog, line numbers, and
// the watermark.
//
// Extracted from `main.js` under `109` HF-085, and for the reason the ratchet in
// `module_seams.test.mjs` exists — the file was AT its ceiling, and line
// numbering needed somewhere to live. It is the same candidate shape as
// `bookmark_manager.mjs`: a surface that owns its own markup and its own state,
// whose entire need of the application is a handful of verbs it can be handed.
//
// The three belong together rather than in three modules. In Word they are one
// group (bar the watermark, which Word keeps on a Design tab this product does
// not have), and here they are additionally one QUESTION — "which section is the
// caret in" — answered by one engine call (`section_of`, shared behind
// `pageSetupSections`, `lineNumbering` and `watermark`). Separate modules would
// have meant separate copies of the section list and as many chances to show one
// section's values while writing another's, which is the defect the engine side
// of the line-numbering change fixed.
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
 *   `fontInventory()`     the font names the editor offers anywhere else
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
    return {
      open: toggle,
      openWatermark: () => {},
      setEnabled: (on) => void (pageSetupBtn.disabled = !on),
    };
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

  const api = {
    open: toggle,
    /** Fill the controls from the caret's section WITHOUT opening the dialog.
     *  The File page shows this panel as a PANE rather than raising a modal over
     *  itself, so it needs the half of `toggle` that reads the document — the
     *  same preparation every other File pane does (`file_pane.mjs`). Returns
     *  false when there is no section geometry to edit, exactly as `toggle`
     *  treats that case. */
    reflect,
    /** The palette's route to the popover. Clicking the trigger rather than
     *  opening directly keeps ONE path through the popover manager, so the
     *  light-dismiss contract and the `aria-expanded` state cannot diverge
     *  between the two ways in. */
    openLineNumbers: () => lineNumbersBtn.click(),
    openWatermark: () => {},
    /** Line numbers and page setup are both document-scoped, so they enable and
     *  disable together with the rest of the document-scoped chrome. The
     *  watermark button is NOT here: it is declared in `LAYOUT_SURFACE`, and
     *  that sweep already owns its `disabled` state and the reason it carries
     *  while disabled. Two owners for one attribute is how a control ends up
     *  enabled with the wrong tooltip. */
    setEnabled(on) {
      pageSetupBtn.disabled = !on;
      lineNumbersBtn.disabled = !on;
    },
  };

  // ---- Watermark (`109` OO-006) -------------------------------------------
  // Word's "Printed Watermark": a form that is filled in and then committed, so
  // a MODAL like the page-geometry dialog above rather than a popover like line
  // numbers. The engine has painted `Section::watermark` since the layout half of
  // OO-006 (`casual-doc-layout/src/watermark.rs`) and nothing could ask for one.
  //
  // Two deliberate departures from Word, recorded here rather than left to be
  // discovered:
  //
  //   * it applies to the CARET'S SECTION, not to every section. Word's dialog
  //     is document-wide; the engine's watermark is a section property, and the
  //     honest surface for a section property is the one the rest of this module
  //     already presents (the dialog's own note says so). A document-wide apply
  //     wants its own "Apply to" control, which is a bigger change than this;
  //   * the picture half is offered as a DISABLED choice carrying its reason.
  //     A picture watermark needs a media id the document already holds and the
  //     host cannot register one (`insertImage` places a drawing in the flow and
  //     returns no id), so a live control would be a dead control. It is still
  //     what an imported picture watermark reads back as, and Apply round-trips
  //     it untouched, so opening this dialog can never silently rewrite one.
  const watermarkBtn = el("watermarkBtn");
  const watermarkDialog = el("watermarkDialog");
  if (!watermarkDialog || !watermarkBtn) return api;

  const watermarkText = el("watermarkText");
  const watermarkFont = el("watermarkFont");
  const watermarkSize = el("watermarkSize");
  const watermarkColor = el("watermarkColor");
  const watermarkSemi = el("watermarkSemiTransparent");
  const watermarkLayoutSeg = el("watermarkLayoutSeg");
  const watermarkTextFields = el("watermarkTextFields");
  const watermarkKinds = [...watermarkDialog.querySelectorAll('input[name="watermarkKind"]')];

  /** The watermark the dialog is currently showing, as the engine reported it.
   *  Kept because Apply has to write back the fields this form does NOT offer —
   *  bold, italic, and a picture's media/scale/washout. */
  let currentWatermark = null;

  /** Word's own watermark size list, in points. Appended from script rather than
   *  authored in the markup because a number is not a string a translator has to
   *  see; "Auto" is in the markup precisely because it is. */
  for (const points of [8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72, 96, 120, 144]) {
    const option = document.createElement("option");
    option.value = String(points * 2); // half-points, the engine's unit
    option.textContent = String(points);
    watermarkSize.appendChild(option);
  }

  /** The section's watermark as the engine reports it, or null when the document
   *  has no sections at all. A section with no watermark comes back as
   *  `kind: "none"` carrying the defaults for a NEW one, so this form has no
   *  separate "new watermark" state that could drift from the engine's idea of
   *  one. */
  function watermarkState() {
    const doc = io.getDoc();
    if (!doc) return null;
    const raw = doc.watermark(io.selectionNode());
    return raw === "null" ? null : JSON.parse(raw);
  }

  /** Which kind the radios are set to. */
  function watermarkKind() {
    return watermarkKinds.find((input) => input.checked)?.value ?? "none";
  }

  /** Which way the stamp runs. */
  function watermarkLayout() {
    return (
      watermarkLayoutSeg.querySelector('button[aria-pressed="true"]')?.dataset.watermarkLayout ??
      "diagonal"
    );
  }

  /** Fills the Font list from the editor's own inventory, so a watermark cannot
   *  be given a face no other control here can name. The first entry is the
   *  document's default, which is what an absent `w:rFonts` resolves to. A face
   *  the inventory does not hold — an imported watermark's own — is added rather
   *  than dropped, for the reason the size list has the same rule below. */
  function paintWatermarkFonts(selected) {
    const names = io.fontInventory?.() ?? [];
    watermarkFont.replaceChildren();
    const fallback = document.createElement("option");
    fallback.value = "";
    fallback.textContent = t("watermark.fontDefault");
    watermarkFont.appendChild(fallback);
    for (const name of selected && !names.includes(selected) ? [selected, ...names] : names) {
      const option = document.createElement("option");
      option.value = name;
      option.textContent = name;
      watermarkFont.appendChild(option);
    }
    watermarkFont.value = selected ?? "";
  }

  /** The chosen kind decides which half of the form is live. Word greys the text
   *  fields under "No watermark" rather than hiding them, so the dialog does not
   *  change shape as you choose. */
  function reflectWatermarkKind() {
    watermarkTextFields.disabled = watermarkKind() !== "text";
  }

  /** Paints every control from the engine's answer. False when there is no
   *  section to carry a watermark, which is what stops the dialog opening on a
   *  form that cannot be applied. */
  function reflectWatermark() {
    const state = watermarkState();
    if (!state) return false;
    currentWatermark = state;
    for (const input of watermarkKinds) input.checked = input.value === state.kind;
    watermarkText.value = state.text ?? "";
    paintWatermarkFonts(state.font ?? "");
    // An imported watermark can carry any size at all, and a <select> silently
    // falls back to its first option for a value it does not hold — which would
    // read as "Auto" and turn a 37pt stamp into an auto-fitted one the moment
    // anybody pressed Apply. An off-list size joins the list instead.
    const wanted = state.sizeHalfPoints == null ? "" : String(state.sizeHalfPoints);
    if (wanted && ![...watermarkSize.options].some((option) => option.value === wanted)) {
      const option = document.createElement("option");
      option.value = wanted;
      option.textContent = String(state.sizeHalfPoints / 2);
      watermarkSize.appendChild(option);
    }
    watermarkSize.value = wanted;
    // A picture watermark reports no colour at all, and `<input type=color>`
    // resolves anything it cannot parse to black — which would read as a black
    // stamp that nobody asked for.
    watermarkColor.value = /^#[0-9a-f]{6}$/i.test(state.color ?? "") ? state.color : "#c0c0c0";
    watermarkSemi.checked = state.semiTransparent === true;
    for (const button of watermarkLayoutSeg.querySelectorAll("button")) {
      button.setAttribute(
        "aria-pressed",
        String(button.dataset.watermarkLayout === (state.layout || "diagonal")),
      );
    }
    reflectWatermarkKind();
    return true;
  }

  const watermarkModal = io.registerModal(watermarkDialog, {
    initialFocus: () => watermarkKinds.find((input) => input.checked) ?? watermarkKinds[0],
    fallbackFocus: () => watermarkBtn,
    defaultAction: () => void applyWatermark(),
  });

  /** Opens the dialog on the engine's current answer, or closes it. */
  function toggleWatermark(open) {
    const show = open ?? !watermarkModal.isOpen;
    if (show === watermarkModal.isOpen) return;
    if (show && !reflectWatermark()) return; // no section to carry one
    if (show) watermarkModal.open();
    else watermarkModal.close();
  }

  /** Writes the section's watermark. One undoable action, whichever branch. */
  async function applyWatermark() {
    const doc = io.getDoc();
    if (!doc || !currentWatermark) return;
    const kind = watermarkKind();
    // The engine refuses an empty stamp and leaves the section exactly as it
    // was, so Apply would look like a button that does nothing. Refuse out loud,
    // at the field, and keep the dialog open.
    //
    // The field is blanked first because `required` refuses only an EMPTY value:
    // whitespace passes it, so a field holding three spaces would have been
    // refused here in silence — the very defect this branch exists to avoid. The
    // message is then the browser's own, in the browser's language, and costs no
    // key of ours.
    if (kind === "text" && !watermarkText.value.trim()) {
      watermarkText.value = "";
      watermarkText.reportValidity();
      return;
    }
    const payload =
      kind === "none"
        ? { section: currentWatermark.section, kind: "none" }
        : kind === "picture"
          ? // Round-tripped verbatim: none of a picture's fields is editable
            // here, so Apply must not invent values for them.
            { ...currentWatermark }
          : {
              section: currentWatermark.section,
              kind: "text",
              text: watermarkText.value.trim(),
              font: watermarkFont.value || null,
              sizeHalfPoints: watermarkSize.value ? Number(watermarkSize.value) : null,
              color: watermarkColor.value,
              // Not in Word's dialog, and not invented here either: an imported
              // watermark can be bold or italic, and dropping that on Apply
              // would be a silent rewrite of the document.
              bold: currentWatermark.bold === true,
              italic: currentWatermark.italic === true,
              layout: watermarkLayout(),
              semiTransparent: watermarkSemi.checked,
            };
    await io.runEdit(() => doc.setWatermark(JSON.stringify(payload)), { gate: true });
    watermarkModal.close();
  }

  for (const input of watermarkKinds) {
    input.addEventListener("change", () => {
      reflectWatermarkKind();
      // Choosing "Text watermark" is choosing to type one, so the caret goes
      // where the words go — the same courtesy the page-setup dialog's opening
      // intents pay.
      if (watermarkKind() === "text") watermarkText.focus();
    });
  }

  watermarkLayoutSeg.addEventListener("click", (e) => {
    const button = e.target.closest("button[data-watermark-layout]");
    if (!button) return;
    for (const other of watermarkLayoutSeg.querySelectorAll("button")) {
      other.setAttribute("aria-pressed", String(other === button));
    }
  });

  el("watermarkCancel").addEventListener("click", () => toggleWatermark(false));
  el("watermarkClose").addEventListener("click", () => toggleWatermark(false));
  el("watermarkApply").addEventListener("click", () => void applyWatermark());

  // The ribbon button's own click is wired by `LAYOUT_SURFACE`, which is also
  // what gives the command its palette row; this is only the palette's and the
  // ribbon's shared way in.
  api.openWatermark = (open = true) => toggleWatermark(open);
  return api;
}
