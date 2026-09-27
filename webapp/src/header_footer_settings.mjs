// Header and footer settings: where the running bands sit, which pages get their
// own, and how the section's pages are numbered.
//
// These are `w:sectPr` properties the engine has imported, laid out and
// round-tripped since P1F-36 and P1B-COV-SECT, with NOTHING in the product able
// to change one of them — `docs/99` §9.4's "modeled is not shipped, built is not
// reachable", which that audit calls the most expensive recurring pattern here. A
// document that arrived with a 0.3" header distance kept it perfectly and nobody
// could make it 0.5".
//
// ---- Why these four things are ONE surface -----------------------------------
//
// Because that is how ONLYOFFICE groups them, and the owner asked for their UX.
// Their live Header & Footer contextual ribbon tab
// (`apps/documenteditor/main/app/view/HeaderFooterTab.js`) reads, left to right:
//
//   [Header & Footer] | [Page Number, Date & Time, Field, Image] |
//   [Header from top / Footer from bottom] | [Different odd and even pages /
//   Different first page] [Link to previous] | [Close]
//
// — the distances at L63-72, the two switches at L74-81, "Link to previous" at
// L82-87 with no separator between the last two groups, so the three switches
// read as one block. The DISTANCES SIT BESIDE THE SWITCHES. Their earlier
// right-side panel had the same content in four titled sections — Position,
// Options, Insert Page Number, Page Numbering
// (`app/template/HeaderFooterSettings.template` L4/L20/L45/L84) — and that panel
// is the shape this dialog copies, because a dialog has titled sections and a
// ribbon band does not. It is dead code now (commented out of
// `app/view/RightMenu.js:56` and absent from both bundles) but it is still their
// own answer to "what shape is this", and the live tab agrees about the grouping.
//
// Word disagrees about ONE of the four: it files the distances under Page Setup ▸
// Layout ▸ "From edge", with the page geometry. ONLYOFFICE is right about which
// question the user is answering — you set a header distance while thinking about
// headers, not while thinking about paper — and following them also keeps the
// page-geometry dialog from growing a seventh group it has no room for.
//
// ---- What it deliberately does NOT offer -------------------------------------
//
//   * "Link to previous" is READ-ONLY, disabled, carrying its reason. See the
//     markup for why: in OOXML a section inherits by OMITTING a reference, so
//     unlinking has to materialise a deep copy of the inherited body with fresh
//     node ids, and the op that points a reference exists while the copy does
//     not. `docs/129` designs it, and records two things that audit found: the
//     only deep copy in the tree is the CLIPBOARD's, which drops 23 of the 27
//     inline kinds — the page number and the logo among them — and the
//     garbage-collection `docs/85` §8.4 assumed on re-link does not exist. A live
//     toggle without the copy would edit the
//     PREVIOUS section's header while claiming to edit this one, which is the
//     docs/104 T-01 cross-surface mutation class.
//   * the six page-number POSITION buttons ONLYOFFICE puts in the same panel
//     (top-left … bottom-right, `HeaderFooterSettings.template` L55-69). Those
//     insert a field into a band; this dialog sets section properties. The Insert
//     band's Page number button already owns insertion, and two controls for one
//     action is the drift the surface-parity guards exist to catch.
//   * `NumberFormat::Other` tokens, their `NumberInDash` format, their Russian
//     locale pair and their "More types" picker — see the markup.
import { TWIPS_PER_INCH, inchesToTwips } from "./units.mjs";
import { t } from "./i18n.mjs";

/** Word's default band distance, and the value layout resolves an absent
 *  `w:pgMar/@w:header`/`@w:footer` to (`casual-doc-layout/src/document_layout.rs`
 *  `DEFAULT_BAND_DISTANCE`). Shown rather than a blank field: the distance is
 *  never meaningfully unset — a band is always SOMEWHERE — so an empty box would
 *  read as "inherited" when nothing is inherited. */
const DEFAULT_BAND_TWIPS = 720;

/** The model's domain for a margin, a band distance and the gutter: 22 inches, the
 *  bound `check_section_domains` enforces. Clamping here rather than letting the
 *  engine refuse means an out-of-range field cannot look like a button that does
 *  nothing — the same reason the Line Numbers fields clamp. */
const MAX_DISTANCE_TWIPS = 31_680;

/** The page-number formats this dialog offers, as the `ST_NumberFormat` tokens the
 *  model's `NumberFormat` serialises to. The markup's `<option value>`s are these
 *  same tokens, so there is no mapping table to fall out of step. */
const NUMBER_FORMATS = new Set([
  "decimal",
  "lowerLetter",
  "upperLetter",
  "lowerRoman",
  "upperRoman",
]);

/**
 * Mounts the Header and footer settings dialog over the markup already in the
 * page, and owns the two running-content variant toggles the Insert band and the
 * palette both run.
 *
 * The toggles moved here from `main.js` because they are this dialog's Options
 * group: the same two engine calls now have one implementation behind two faces,
 * so a checkbox here and a pressed button there cannot disagree about what the
 * document says. That is also what paid for the lines this change adds to
 * `main.js`, which is at its ratchet.
 *
 * `io` is its entire contact with the application:
 *
 *   `getDoc()`            the open document, or null
 *   `selectionNode()`     the focus node id, or "" when there is no selection
 *   `runningPage()`       the page running-content commands act on — the page
 *                         whose band is open, else the page in view. Stays in
 *                         `main.js` because it reads `pages` and the open band.
 *   `runEdit(thunk, options)` Promise; applies an engine edit, repaints, and
 *                         refuses it in the review modes that forbid it
 *   `applyEditResult(result)` the raw-`EditResult` path, for the two variant
 *                         calls that predate `runEdit` and are gated by hand
 *   `blockedFromMutating()` true when the current review mode forbids an edit,
 *                         having already said so
 *   `setStatus(text, kind)`  the status line
 *   `registerModal(dialog, options)`  the host's modal registry
 */
export function createHeaderFooterSettings(io) {
  const el = (id) => document.getElementById(id);

  const dialog = el("headerFooterSettingsDialog");
  const openBtn = el("headerFooterSettingsBtn");

  /** Which running-content variants are on for the page the user is on, read from
   *  the engine rather than a local flag so no face can drift from the document.
   *
   *  `w:titlePg` is per SECTION, so this is a question about a PAGE: on a document
   *  whose page 5 changed orientation, the first section's answer is not the answer
   *  for the section the user is reading. */
  function variantState(page = io.runningPage()) {
    const doc = io.getDoc();
    if (!doc) return { firstPage: false, evenOdd: false };
    try {
      return JSON.parse(doc.runningVariants(page?.pageNumber ?? 1));
    } catch {
      return { firstPage: false, evenOdd: false };
    }
  }

  /** Toggles Word's "Different First Page" / "Different Odd & Even Pages".
   *
   *  Each only says a variant APPLIES; its content is a separate header/footer
   *  reference, so turning one on for a document that has no such variant shows an
   *  empty band until one is created — which is what Word does too.
   *
   *  "Different first page" is per section, so it applies to the section owning the
   *  page the user is on; "Different odd & even pages" is document-scoped, as OOXML
   *  carries it. */
  async function toggleVariant(which) {
    const doc = io.getDoc();
    if (!doc) return;
    if (io.blockedFromMutating()) return;
    const page = io.runningPage();
    const state = variantState(page);
    const next = !state[which];
    try {
      const result =
        which === "firstPage"
          ? doc.setFirstPageVariant(next, page?.pageNumber ?? 1)
          : doc.setEvenOddVariant(next);
      await io.applyEditResult(result);
    } catch (err) {
      io.setStatus(t("headerFooter.changeFailed", { message: err.message ?? err }), "error");
      return;
    }
    io.setStatus(
      which === "firstPage"
        ? next
          ? t("headerFooter.firstPageOn")
          : t("headerFooter.firstPageOff")
        : next
          ? t("headerFooter.evenOddOn")
          : t("headerFooter.evenOddOff"),
    );
  }

  // The variant toggles ship whether or not the dialog's markup is present, so an
  // embedding that trims the dialog does not lose the two commands that already
  // existed before it.
  //
  // `disabled` is deliberately NOT owned here: the button is declared in
  // `INSERT_SURFACE`, and that sweep already owns its enabled state and the reason
  // it carries while disabled. Two owners for one attribute is how a control ends
  // up enabled with the wrong tooltip — the note `page_setup.mjs` leaves about the
  // watermark button, for the same reason.
  const api = {
    variantState,
    toggleVariant,
    open: () => {},
  };
  if (!dialog || !openBtn) return api;

  const sectionSelect = el("headerFooterSection");
  const headerFromTop = el("headerFromTop");
  const footerFromBottom = el("footerFromBottom");
  const diffFirst = el("headerFooterDiffFirst");
  const diffOddEven = el("headerFooterDiffOddEven");
  const linkToPrevious = el("headerFooterLinkToPrevious");
  const numberContinue = el("pageNumberContinue");
  const numberRestart = el("pageNumberRestart");
  const numberStart = el("pageNumberStart");
  const numberFormat = el("pageNumberFormat");

  /** The section whose values the dialog is currently showing. `null` before the
   *  first reflect, and never read without one. */
  let current = null;

  /** Twips → an inches string. `0` shows as "0" rather than blank, for the reason
   *  `DEFAULT_BAND_TWIPS` records: none of these is meaningfully unset. */
  function inchText(twip) {
    return (twip / TWIPS_PER_INCH).toFixed(2).replace(/\.?0+$/, "") || "0";
  }

  /** An inches field as twips, clamped into the model's domain. */
  function distanceTwips(input) {
    const raw = inchesToTwips(input.value);
    if (!Number.isFinite(raw)) return 0;
    return Math.min(MAX_DISTANCE_TWIPS, Math.max(0, Math.round(raw)));
  }

  /** Every section's geometry plus the caret's, from the engine. `null` when there
   *  is nothing to edit.
   *
   *  ONE call answers "which section is the caret in" for this dialog, and it is
   *  the SAME call `page_setup.mjs` makes (`pageSetupSections`). Asking the
   *  question twice, two ways, is how a dialog came to show one section's margins
   *  while writing them to another — and the walk behind it is O(document), so a
   *  second answer would also cost a second document walk per opening. */
  function sections() {
    const doc = io.getDoc();
    if (!doc) return null;
    const raw = doc.pageSetupSections(io.selectionNode());
    const list = raw === "null" ? null : JSON.parse(raw);
    return list?.sections?.length ? list : null;
  }

  /** Every section's vertical alignment and page numbering. Kept separate from
   *  `sections()` because the engine keeps them in a separate payload —
   *  `sectionLayout` is the read side of `setSectionLayout`, and reflecting from
   *  anything else would let this dialog and Page setup hold two ideas of one
   *  value. A list, so the Section dropdown can paint any section. */
  function layoutSections() {
    const doc = io.getDoc();
    if (!doc) return null;
    const raw = doc.sectionLayout(io.selectionNode());
    const list = raw === "null" ? null : JSON.parse(raw);
    return list?.sections?.length ? list : null;
  }

  /** One section's layout entry, by id. */
  function layoutOf(sectionId) {
    return layoutSections()?.sections?.find((entry) => entry.section === sectionId) ?? null;
  }

  /** Which band distances one section carries, resolved the way layout resolves
   *  them: an absent attribute is Word's 720 twips, not zero. */
  function paintSection(section) {
    current = section;
    const margins = section.pageMargins ?? {};
    headerFromTop.value = inchText(margins.headerTwips ?? DEFAULT_BAND_TWIPS);
    footerFromBottom.value = inchText(margins.footerTwips ?? DEFAULT_BAND_TWIPS);
    // Numbering is per section too, so it repaints with the rest rather than only
    // on the first reflect — the two used to be one copy of these lines each,
    // which is how a Section dropdown comes to move some fields and not others.
    paintNumbering(layoutOf(section.section));
  }

  /** Paints the numbering controls from the engine's answer. An absent `w:start`
   *  is "continue from the previous section", which is the radio that is then
   *  chosen and the reason the spinner still shows a number: a user switching to
   *  "Start at" should not first have to think of one. */
  function paintNumbering(state) {
    const start = state?.pageNumberStart ?? null;
    numberContinue.checked = start === null;
    numberRestart.checked = start !== null;
    numberStart.value = String(start ?? 1);
    // A format the list does not hold — an imported `NumberFormat::Other`, or one
    // of the typed tokens this dialog deliberately does not offer — must not
    // silently become "decimal" the moment somebody presses Apply. The select is
    // left showing nothing and Apply preserves what was read (see `payload`).
    const token = state?.pageNumberFormat;
    numberFormat.value = typeof token === "string" && NUMBER_FORMATS.has(token) ? token : "";
  }

  /** Fills every control from the document. False when there is no section to
   *  edit, which is what stops the dialog opening on a form that cannot apply. */
  function reflect() {
    const list = sections();
    if (!list || !layoutSections()) return false;
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
    const variants = variantState();
    diffFirst.checked = variants.firstPage === true;
    diffOddEven.checked = variants.evenOdd === true;
    // Link to previous: the truth the engine already reports, per PAGE, because
    // inheritance is resolved per page and not per section. A section whose header
    // is inherited and whose footer is not is a real document, so the box is only
    // ticked when BOTH are — and it says so by staying disabled either way.
    const bands = runningBands();
    linkToPrevious.checked = bands !== null && bands.headerLinked && bands.footerLinked;
    return true;
  }

  /** One page's running-content context, for the Link-to-previous indicator. */
  function runningBands() {
    const doc = io.getDoc();
    const page = io.runningPage();
    if (!doc || !page) return null;
    try {
      return JSON.parse(doc.runningBands(page.pageNumber));
    } catch {
      return null;
    }
  }

  const modal = io.registerModal(dialog, {
    initialFocus: () => headerFromTop,
    fallbackFocus: () => openBtn,
    defaultAction: () => void apply(),
  });

  function toggle(open) {
    const show = open ?? !modal.isOpen;
    if (show === modal.isOpen) return;
    if (show && !reflect()) return; // nothing to edit
    if (show) modal.open();
    else modal.close();
  }

  /** Writes whatever the user actually changed, and NOTHING ELSE.
   *
   *  Two engine entry points, because these are two property families in two
   *  payloads and the engine has no single "write this whole section" call:
   *  `setPageSetup` carries the band distances (they live in `w:pgMar`) and
   *  `setSectionLayout` carries the numbering (`w:pgNumType`). So ONE CHANGE IS ONE
   *  UNDOABLE ACTION only if each call is skipped when its half is untouched —
   *  which is what the comparisons below are for. An Apply that issued both
   *  unconditionally made a header-distance change take TWO presses of undo to
   *  reverse, which is what this repository's undo guard caught.
   *
   *  A user who changes both halves in one Apply does make two changes, and undoes
   *  them in two steps. That is a deliberate departure from Word, recorded here
   *  rather than left to be discovered: closing it means one engine operation
   *  spanning two property families, which is a change to the op set and not to
   *  this dialog.
   *
   *  Both payloads are built from a FRESH read rather than from what the dialog
   *  opened on, so applying here cannot clobber a value Page setup changed while
   *  this dialog was up: `pageMargins` is spread from the engine's current answer
   *  and only the two distances are replaced. */
  async function apply() {
    const doc = io.getDoc();
    if (!doc || !current) return;
    const list = sections();
    const fresh = list?.sections?.find((section) => section.section === current.section);
    if (!fresh) return; // the section went away under us; say nothing, change nothing

    const headerTwips = distanceTwips(headerFromTop);
    const footerTwips = distanceTwips(footerFromBottom);
    const bandsMoved =
      headerTwips !== (fresh.pageMargins?.headerTwips ?? DEFAULT_BAND_TWIPS) ||
      footerTwips !== (fresh.pageMargins?.footerTwips ?? DEFAULT_BAND_TWIPS);
    if (bandsMoved) {
      await io.runEdit(
        () =>
          doc.setPageSetup(
            JSON.stringify({
              section: fresh.section,
              pageSize: fresh.pageSize,
              pageMargins: { ...fresh.pageMargins, headerTwips, footerTwips },
              columns: fresh.columns,
              // Required key, and `null` is a legal value the engine reads as
              // "infer from the page size" — so it is passed through rather than
              // defaulted, which would turn an inferred orientation into an
              // asserted one on every Apply.
              orientation: fresh.orientation ?? null,
            }),
          ),
        { gate: true },
      );
    }

    const state = layoutOf(fresh.section);
    // An off-list format is preserved rather than replaced: the select shows
    // nothing for one, so reading the select back would drop it.
    const format = numberFormat.value || (state?.pageNumberFormat ?? null);
    const start = numberRestart.checked
      ? Math.min(1_000_000, Math.max(0, Math.round(Number(numberStart.value) || 0)))
      : null;
    const numberingChanged =
      format !== (state?.pageNumberFormat ?? null) || start !== (state?.pageNumberStart ?? null);
    if (numberingChanged) {
      await io.runEdit(
        () =>
          doc.setSectionLayout(
            JSON.stringify({
              section: fresh.section,
              // Carried through untouched: vertical alignment is Page setup's
              // field, and this Apply must not invent a value for it.
              verticalAlignment: state?.verticalAlignment ?? null,
              pageNumberFormat: format,
              pageNumberStart: start,
            }),
          ),
        { gate: true },
      );
    }
    toggle(false);
  }

  sectionSelect.addEventListener("change", () => {
    const list = sections();
    const picked = list?.sections?.find((section) => section.section === sectionSelect.value);
    if (picked) paintSection(picked);
  });

  // The two switches are the SAME commands the Insert band's buttons run, so they
  // apply immediately rather than on Apply — a checkbox that needed Apply while
  // its twin button did not would be two behaviours for one command. Re-reflecting
  // afterwards is what keeps the box showing what the document says even when the
  // engine refuses the change.
  diffFirst.addEventListener("change", async () => {
    await toggleVariant("firstPage");
    const variants = variantState();
    diffFirst.checked = variants.firstPage === true;
  });
  diffOddEven.addEventListener("change", async () => {
    await toggleVariant("evenOdd");
    const variants = variantState();
    diffOddEven.checked = variants.evenOdd === true;
  });

  // Typing a number is choosing "Start at": leaving the radio on "Continue" while
  // a number sits beside it would make Apply quietly ignore what was typed.
  numberStart.addEventListener("input", () => {
    numberRestart.checked = true;
  });

  el("headerFooterSettingsCancel").addEventListener("click", () => toggle(false));
  el("headerFooterSettingsClose").addEventListener("click", () => toggle(false));
  el("headerFooterSettingsApply").addEventListener("click", () => void apply());

  api.open = (open = true) => toggle(open);
  return api;
}
