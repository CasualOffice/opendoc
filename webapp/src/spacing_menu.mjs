// The Home band's Line & paragraph spacing popover.
//
// Lifted out of `main.js` whole (it was ~90 lines of wiring in the middle of the
// toolbar section) so the capability this adds is paid for rather than charged
// to the module ratchet, and so the rule below can be stated once in the place
// that implements it.
//
// ## The competitive standard, and which one this follows
//
// * **Word** — Home ▸ Line and Paragraph Spacing is one dropdown: the multiples
//   1.0 / 1.15 / 1.5 / 2.0 / 2.5 / 3.0, then "Line Spacing Options…" (the
//   Paragraph dialog), then **Add Space Before Paragraph / Remove Space Before
//   Paragraph** and the same pair for After. Add writes 12 pt; Remove writes 0.
//   The two chords ⌘1 / ⌘5 / ⌘2 set single / 1.5 / double directly.
// * **Google Docs** — Format ▸ Line & paragraph spacing is the same shape:
//   Single / 1.15 / 1.5 / Double, "Custom spacing…", then **Add space before
//   paragraph / Add space after paragraph**, which flip to "Remove …" once the
//   paragraph has that space. Docs shows one item whose verb tells you the
//   current state instead of two items where one is always a no-op.
//
// **Followed: Docs for the shape (one item per side, verb flipped from the
// current state), Word for the amount (12 pt).** Two items per side would put a
// permanently dead row in a four-row menu; a flipping verb also makes the menu
// report what the paragraph currently has, which is the half the popover was
// missing. The presets stay at Word's and Docs' common four — 2.5 and 3.0 are
// reachable by typing them into the exact field, which is a path Docs does not
// even offer.
//
// ## What the popover shows, and how it tells inherited from directly-set
//
// `paragraphSpacing()` is STYLE-RESOLVED: it answers the value actually in
// effect, and `lineFromStyle` / `beforeFromStyle` / `afterFromStyle` say whether
// the paragraph wrote it or inherited it. Before that it reported direct
// formatting only, so on every paragraph of the shipped sample — which takes its
// line spacing from `Normal`, as most paragraphs in most documents do — no
// preset was ticked, the value box was empty, and a note said in words that the
// paragraph followed its style. A blank box is what neither competitor shows:
//
// * **Word** puts the RESOLVED number in the Paragraph dialog's line-spacing
//   fields, whether the paragraph set it or its style did.
// * **Docs** ticks the RESOLVED preset in Format ▸ Line & paragraph spacing.
//
// **Followed: both.** The preset is ticked (Docs) and the number is shown
// (Word), and the distinction Word's dialog drops is kept — a note under the
// field, plus `data-from-style` on every control whose value was inherited — so
// "1.5, from the style" and "1.5, set here" stay two different readings, which
// is what makes "clear the override" a gesture that means something.
//
// `ParagraphSpacing::default()` answers `-1` for before/after, not `0`, because
// `0` is a real spacing value and the old default was claiming "unset" with it.
// Every read here tests `>= 0` rather than truthiness for that reason.

/** Twips per typographic point. */
const TWIPS_PER_POINT = 20;

/** What Word's "Add Space Before/After Paragraph" writes, in points. */
const ADDED_SPACE_POINTS = 12;

const round2 = (n) => Math.round(n * 100) / 100;

/**
 * Wires the spacing popover and returns its controller.
 *
 * Every read is one `paragraphSpacing` call on the caret's paragraph and every
 * write is one toolbar edit, so the popover is **O(1) in document size** — it
 * never enumerates paragraphs.
 *
 * @param {object} host
 * @param {() => object | null} host.doc the engine handle, or null
 * @param {() => string | null} host.focusNode the caret paragraph's node id
 * @param {(thunk: Function, options?: object) => unknown} host.runToolbarEdit
 * @param {(btn: Element, menu: Element, reflect: Function) => unknown} host.registerPopover
 * @param {(el: Element, handler: Function) => void} host.onButton
 * @param {(key: string) => string} host.t the localised-string lookup
 */
export function createSpacingMenu(host) {
  const btn = document.getElementById("spacingBtn");
  const menu = document.getElementById("spacingMenu");
  const modeSelect = document.getElementById("lineSpacingMode");
  const valueInput = document.getElementById("lineSpacingValue");
  const unitLabel = document.getElementById("lineSpacingUnit");
  const beforeInput = document.getElementById("spaceBefore");
  const afterInput = document.getElementById("spaceAfter");
  const beforeToggle = document.getElementById("spaceBeforeToggle");
  const afterToggle = document.getElementById("spaceAfterToggle");
  const fromStyleNote = document.getElementById("lineSpacingFromStyle");

  /** The caret paragraph's spacing, or null when there is nothing to ask about. */
  function spacingNow() {
    const doc = host.doc();
    const node = host.focusNode();
    if (!doc || !node) return null;
    return doc.paragraphSpacing(node);
  }

  /** Sync the value field's unit label + step to the current mode (× for a
   *  multiple, pt for atLeast/exact). */
  function reflectUnit() {
    const multiple = modeSelect.value === "multiple";
    unitLabel.textContent = multiple ? "×" : "pt";
    valueInput.step = multiple ? "0.05" : "1";
  }

  /** Reflect the caret paragraph's spacing into the menu: the line preset check,
   *  the mode/value fields, the space before/after fields, and the verb on each
   *  one-gesture row. */
  function reflect() {
    const s = spacingNow();
    if (!s) return;
    const percent = s.lineRule === 0 ? s.linePercent : 0; // presets are `auto` multiples
    for (const b of menu.querySelectorAll(".spacing-line")) {
      const ticked = Number(b.dataset.percent) === percent;
      b.setAttribute("aria-checked", String(ticked));
      // The tick is the same tick whether the value was written here or
      // inherited — Docs ticks the resolved preset — and the flag is what keeps
      // the two readable apart without a second, quieter tick nobody would find.
      setFromStyle(b, ticked && s.lineFromStyle);
    }
    // Don't overwrite a field the user is mid-edit in.
    const editingCustom =
      document.activeElement === modeSelect || document.activeElement === valueInput;
    if (!editingCustom) {
      // `lineRule`: 0 auto/multiple, 1 atLeast, 2 exact.
      if (s.lineRule === 1 || s.lineRule === 2) {
        modeSelect.value = s.lineRule === 1 ? "atLeast" : "exact";
        valueInput.value = s.lineTwip > 0 ? String(round2(s.lineTwip / TWIPS_PER_POINT)) : "";
      } else {
        modeSelect.value = "multiple";
        valueInput.value = s.linePercent > 0 ? String(round2(s.linePercent / 100)) : "";
      }
      // Three states, not two. The value box now carries the EFFECTIVE number,
      // so "empty" no longer means "inherited" — it means nothing in the whole
      // cascade sets line spacing and the layout default applies. Inherited is
      // the case with a real number and `lineFromStyle`, and it gets the
      // sentence that says where the number came from: in a row of its own,
      // because the box is 56px wide and a sentence put in it is three
      // characters. The box keeps it as a tooltip for a pointer that lands there.
      const inherited = s.lineFromStyle;
      const unset = valueInput.value === "";
      const note = inherited ? "spacing.fromStyleHint" : unset ? "spacing.lineUnset" : "";
      setFromStyle(valueInput, inherited);
      valueInput.title = note ? host.t(note) : "";
      if (fromStyleNote) {
        fromStyleNote.textContent = note ? host.t(note) : "";
        fromStyleNote.hidden = !note;
      }
      reflectUnit();
    }
    reflectSpaceField(beforeInput, s.beforeTwip, s.beforeFromStyle);
    reflectSpaceField(afterInput, s.afterTwip, s.afterFromStyle);
    reflectToggle(beforeToggle, s.beforeTwip, "spacing.addSpaceBefore", "spacing.removeSpaceBefore");
    reflectToggle(afterToggle, s.afterTwip, "spacing.addSpaceAfter", "spacing.removeSpaceAfter");
  }

  /** Marks one control as showing a value it INHERITED rather than one the
   *  paragraph set. An attribute rather than a class: the stylesheet and a
   *  guard both read it, and there is nothing to keep in sync. */
  function setFromStyle(el, inherited) {
    if (!el) return;
    if (inherited) el.dataset.fromStyle = "true";
    else delete el.dataset.fromStyle;
  }

  /** One space-before/after field: the EFFECTIVE value in points, the inherited
   *  flag, and the sentence saying so. `-1` is the engine's "unset anywhere in
   *  the cascade" and is the only value that leaves the box empty — `0` is a
   *  real answer ("no space before this paragraph") and now prints as one. */
  function reflectSpaceField(input, twip, fromStyle) {
    if (!input || document.activeElement === input) return;
    input.value = twip >= 0 ? String(Math.round(twip / TWIPS_PER_POINT)) : "";
    setFromStyle(input, twip >= 0 && fromStyle);
    input.title = twip >= 0 && fromStyle ? host.t("spacing.spaceFromStyleHint") : "";
  }

  /** Docs' flipping verb: the row offers whichever of add/remove would change
   *  this paragraph, and `aria-pressed` says which state it is reporting. */
  function reflectToggle(el, twip, addKey, removeKey) {
    if (!el) return;
    const has = twip > 0;
    el.querySelector(".menu-item-label").textContent = host.t(has ? removeKey : addKey);
    el.dataset.spaceState = has ? "on" : "off";
  }

  /** Commit a space-before/after value in twips (`-1` clears it back to the
   *  style default). */
  function setSpace(which, twips) {
    host.runToolbarEdit(
      (a, b, c, d) =>
        which === "before"
          ? host.doc().setSpaceBefore(a, b, c, d, twips)
          : host.doc().setSpaceAfter(a, b, c, d, twips),
      { paragraphLevel: true },
    );
    reflect();
  }

  /** The one-gesture rows: add 12 pt where there is none, clear it where there
   *  is. Word's amount, Docs' single flipping row. */
  function toggleSpace(which) {
    const s = spacingNow();
    if (!s) return;
    const has = (which === "before" ? s.beforeTwip : s.afterTwip) > 0;
    setSpace(which, has ? 0 : ADDED_SPACE_POINTS * TWIPS_PER_POINT);
  }

  /** Applies a line-spacing multiple as a percentage (100 = single). Shared by
   *  the preset rows, the palette commands generated from them, and the ⌘1 /
   *  ⌘5 / ⌘2 chords. */
  function applyLinePercent(percent) {
    host.runToolbarEdit((a, b, c, d) => host.doc().setLineSpacing(a, b, c, d, percent), {
      paragraphLevel: true,
    });
    reflect();
  }

  /** Commit the custom line-spacing mode + value. Multiple rides
   *  `setLineSpacing` (the `auto` percent rule); At least / Exactly ride
   *  `setLineSpacingExact` (twips + `at_least`). Blank/non-numeric is ignored. */
  function applyCustom() {
    const raw = valueInput.value.trim();
    if (raw === "" || !Number.isFinite(Number(raw))) return;
    const v = Number(raw);
    if (v <= 0) return;
    if (modeSelect.value === "multiple") {
      applyLinePercent(Math.round(v * 100));
      return;
    }
    const twips = Math.max(0, Math.round(v * TWIPS_PER_POINT));
    const atLeast = modeSelect.value === "atLeast";
    host.runToolbarEdit(
      (a, b, c, d) => host.doc().setLineSpacingExact(a, b, c, d, twips, atLeast),
      { paragraphLevel: true },
    );
    reflect();
  }

  /** Commit a typed space-before/after field: blank clears (back to the style
   *  default), otherwise points → twips (clamped ≥ 0). Non-numeric is ignored. */
  function applyTypedSpace(input, which) {
    const raw = input.value.trim();
    if (raw !== "" && !Number.isFinite(Number(raw))) return;
    setSpace(which, raw === "" ? -1 : Math.max(0, Math.round(Number(raw) * TWIPS_PER_POINT)));
  }

  host.registerPopover(btn, menu, reflect);
  for (const b of menu.querySelectorAll(".spacing-line")) {
    host.onButton(b, () => applyLinePercent(Number(b.dataset.percent)));
  }
  if (beforeToggle) host.onButton(beforeToggle, () => toggleSpace("before"));
  if (afterToggle) host.onButton(afterToggle, () => toggleSpace("after"));
  // Switching mode only reinterprets the value's unit; it never auto-applies (a
  // multiple typed as "1.5" must not be re-read as 1.5 pt). Commit on change.
  modeSelect.addEventListener("change", reflectUnit);
  valueInput.addEventListener("change", applyCustom);
  beforeInput.addEventListener("change", () => applyTypedSpace(beforeInput, "before"));
  afterInput.addEventListener("change", () => applyTypedSpace(afterInput, "after"));

  /** Whether the caret paragraph currently has space on `which` side. */
  function hasSpace(which) {
    const s = spacingNow();
    return !!s && (which === "before" ? s.beforeTwip : s.afterTwip) > 0;
  }

  /** Every spacing capability as a command row, for the palette, the app menus,
   *  the keymap and a host driving the editor.
   *
   *  Generated from the popover's own markup rather than restated, so a preset
   *  added there gets its command — and its chord's meaning — for free, and no
   *  label can drift from the control it mirrors. Declared whether or not a
   *  document is open: a chord bound to a command the registry only sometimes
   *  returns is a chord that sometimes does nothing, with no error anywhere. With
   *  no caret the rows ship DISABLED with their reason.
   *
   *  O(presets) — six rows read off the DOM, nothing walked. */
  function commands({ enabled }) {
    const rows = [];
    for (const preset of menu?.querySelectorAll(".spacing-line") ?? []) {
      const percent = Number(preset.dataset.percent);
      rows.push({
        id: `paragraph.spacing.${percent}`,
        label: `Line spacing: ${preset.textContent.trim()}`,
        group: "Paragraph",
        kw: "line spacing leading single double space",
        enabled,
        disabledReason: host.t("paragraph.caretRequired"),
        run: () => applyLinePercent(percent),
      });
    }
    // Literal keys, never built ones: the catalogue is EXTRACTED from this
    // source, so a key assembled at runtime is a key no catalogue has.
    const label = {
      before: () =>
        enabled && hasSpace("before")
          ? host.t("spacing.removeSpaceBefore")
          : host.t("spacing.addSpaceBefore"),
      after: () =>
        enabled && hasSpace("after")
          ? host.t("spacing.removeSpaceAfter")
          : host.t("spacing.addSpaceAfter"),
    };
    for (const side of ["before", "after"]) {
      rows.push({
        id: `paragraph.space.${side}`,
        label: label[side](),
        group: "Paragraph",
        kw: `space ${side} paragraph add remove spacing padding gap margin`,
        enabled,
        disabledReason: host.t("paragraph.caretRequired"),
        run: () => toggleSpace(side),
      });
    }
    return rows;
  }

  return {
    /** Re-read the caret paragraph into the popover's controls. */
    reflect,
    /** Apply a line-spacing multiple, as a percentage (100 = single). */
    applyLinePercent,
    /** Add or remove the 12 pt of space on one side of the caret paragraph. */
    toggleSpace,
    hasSpace,
    commands,
  };
}
