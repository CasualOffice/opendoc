// The measurement-unit preference: Word's *Measurement units*, ONLYOFFICE's
// `cmbUnit` (`docs/153` `shell.measurement-units`).
//
// `SKILL` §9 rule 4 again. `casual_doc_layout::quantity` parses and formats cm,
// mm, in, pt and pica against the twip by EXACT integer arithmetic — `2.54 cm` is
// 1440 twips, not a float that rounds to it — and `casual-doc-wasm` publishes the
// whole of it across the JS boundary as five free functions. Nothing in `webapp/`
// called any of them: every dialog field in the product was hard-coded inches, so
// a reader outside the United States and Canada typed in a unit they do not use.
//
// ---- WHY THIS IS NOT A DOCUMENT SETTING ------------------------------------
//
// It belongs to the PERSON, not the file, and that is why the engine exposes it
// as free functions rather than as methods on a document: the same choice governs
// Page setup, the indent spinners and table properties, including with no
// document open at all. So it is stored with the other personal preferences and
// read from there, and this module is the one place that knows which unit is in
// force.
//
// ---- THE ONE RULE A HOST MUST NOT GET WRONG --------------------------------
//
// **Persist the id, never an ordinal.** ONLYOFFICE persists theirs as an integer
// — `Common.localStorage.setItem("de-settings-unit", this.cmbUnit.getValue())`
// read back through `parseInt` (`apps/documenteditor/main/app/view/FileMenuPanels.js:1032,1114`)
// — and they carry two disagreeing unit enums, which costs them a remap ternary
// copy-pasted across five files. The engine deliberately exposes no ordinal at
// all; what this module stores is the string id (`"cm"`, `"mm"`, `"inch"`,
// `"point"`, `"pica"`), and an unrecognised stored value falls back to the
// region default rather than to array position 0.
//
// ---- THE SEPARATOR, AND THE ONE PLACE IT CANNOT BE USED --------------------
//
// `decimalSeparatorForLanguage` answers what a locale writes, and `formatMeasurement`
// takes it — so a German reader sees `2,54`. But an `<input type="number">` only
// ever accepts a FULL STOP: the HTML floating-point literal is not localised, and a
// value of `2,54` is simply invalid and reads back empty. So a FIELD is formatted
// with `"."` and the prose a reader only reads — the Page setup preview's
// `8.27 × 11.69 in` — is formatted with the locale's own separator. Parsing accepts
// both regardless, which is why a German reader typing `2,54` into a field this
// module wrote as `2.54` is still understood.
//
// ---- WHAT THIS DOES NOT YET GOVERN, SAID WHERE A READER CAN SEE IT ---------
//
// The ruler keeps its own tick lattice: `ruler.mjs` draws an inch grid computed
// from `twipsPerInch`, and a centimetre preference does not move those ticks yet.
// That is stated in the Settings note beside the control rather than left for a
// reader to discover, because a preference that silently governs some surfaces and
// not others is worse than one that says which.
//
// Pure: every engine call and the preference store arrive through the caller, and
// the `<select>` is handed in, so none of this needs a browser and
// `measurement_units.test.mjs` can drive the whole policy in node.
import { t } from "./i18n.mjs";
import { readPref, writePref } from "./prefs.mjs";

/** Where the chosen unit is kept. One key, one string. */
export const UNIT_PREF_KEY = "opendoc.measurementUnit";

/** The label key for a unit id. Named rather than derived so a translator sees
 *  five declared strings instead of a template, and so an unknown id cannot
 *  produce a key no catalogue answers. */
const UNIT_LABEL_KEYS = Object.freeze({
  cm: "units.cm",
  mm: "units.mm",
  inch: "units.inch",
  point: "units.point",
  pica: "units.pica",
});

/** The unit name to show for `id`, or the id itself when the engine grows one
 *  this build has no name for — which reads as the engine's own token rather
 *  than as a blank row. */
export function unitLabel(id) {
  const key = UNIT_LABEL_KEYS[id];
  return key ? t(key) : id;
}

/**
 * The measurement preference, and the chooser that sets it.
 *
 * @param {object} io
 * @param {object} io.engine the four engine functions, injected so this module
 *        needs no wasm and no browser: `measurementUnits()`,
 *        `defaultMeasurementUnit(region)`, `decimalSeparatorForLanguage(tag)`,
 *        `parseMeasurement(text, unitId)`, `formatMeasurement(twips, unitId, sep)`.
 * @param {HTMLSelectElement|null} io.select the chooser in the Settings dialog.
 * @param {() => string} io.locale the active BCP-47 tag, for the separator and
 *        for the first-run region default.
 * @param {() => void} io.onChanged repaint whatever is showing a measurement.
 * @param {(text: string, kind?: string) => void} io.setStatus
 * @param {object} [io.view] the storage host, for a test with no `localStorage`.
 */
export function createMeasurementUnits(io) {
  const view = io.view ?? undefined;
  /** The engine's rows, the id index, and the unit in force.
   *
   *  Read ONCE, and deliberately NOT at construction. This factory is called
   *  while `main.js` is still being evaluated, which is before `boot()` has
   *  awaited `init()` — and a free function on an uninstantiated wasm module
   *  does not merely return nothing, it throws
   *  `Cannot read properties of undefined (reading
   *  '__wbindgen_add_to_stack_pointer')` and takes the whole editor down with
   *  it, because a throw during module evaluation means nothing after it runs.
   *  `main.js` already records this hazard where it reaches for
   *  `engineVersion()`; this module has to honour the same rule.
   *
   *  So the roster is faulted in on first use. `reflect()` is called once more
   *  from `boot()` after `init()` resolves, which is what fills the chooser; the
   *  roster still does not depend on a document, and it is still never rebuilt
   *  under a keyboard user. */
  let rows = null;
  let byId = null;
  let chosen = null;

  /** Faults the roster in. Returns whether the engine was ready.
   *
   *  `false` is only ever the pre-`init()` window: every caller below treats it
   *  as "nothing to show yet" rather than as an error, because at that point
   *  there is no document, no dialog and no field to format. */
  function ensure() {
    if (rows) return true;
    if (typeof io.engine?.measurementUnits !== "function") return false;
    let serialised;
    try {
      serialised = io.engine.measurementUnits();
    } catch {
      return false;
    }
    rows = JSON.parse(serialised);
    byId = new Map(rows.map((row) => [row.id, row]));
    /** A stored value that the engine does not recognise is DISCARDED rather
     *  than trusted: a build that drops a unit, or a value someone edited by
     *  hand, must not leave every dialog formatting against a unit that no
     *  longer exists. */
    chosen = resolve(readPref(UNIT_PREF_KEY, "", view));
    return true;
  }

  function resolve(stored) {
    if (byId.has(stored)) return stored;
    const fallback = io.engine.defaultMeasurementUnit(io.locale());
    return byId.has(fallback) ? fallback : rows[0].id;
  }

  /** The row for the unit in force: `{id, suffix, decimalPlaces, stepTwip}`. */
  function unit() {
    return ensure() ? byId.get(chosen) : undefined;
  }

  /** The locale's decimal separator, for prose only — see the header. */
  function separator() {
    if (typeof io.engine?.decimalSeparatorForLanguage !== "function") return ".";
    return io.engine.decimalSeparatorForLanguage(io.locale());
  }

  /** Twips → the text a `type="number"` FIELD shows, in the unit in force.
   *
   *  Always a full stop, because an HTML number input accepts nothing else.
   *  Trailing zeros are trimmed, which is what every distance field in this
   *  product has always shown — `1.00` reads as a precision nobody asked for —
   *  and it is safe because trimming never shows MORE digits than the unit's own
   *  `decimalPlaces`, so `format` → `parse` is still the engine's fixed point.
   *  A whole value keeps at least one digit: `"0"`, never `""`, because a blank
   *  box and a box holding 0 mean different things in a dialog. */
  function format(twips) {
    if (!ensure()) return "";
    const raw = io.engine.formatMeasurement(Math.round(twips ?? 0), chosen, ".");
    return raw.includes(".") ? raw.replace(/\.?0+$/, "") || "0" : raw;
  }

  /** Twips → prose a reader reads, with the locale's separator and the unit's
   *  own suffix. Used for the Page setup preview's caption. */
  function display(twips) {
    if (!ensure()) return "";
    return io.engine.formatMeasurement(Math.round(twips ?? 0), chosen, separator());
  }

  /** Field text → twips, or `null` when the engine refuses it.
   *
   *  **A refusal, never a zero.** The old helper read a blank or unparseable
   *  margin field as 0 and wrote a 0-inch margin nobody asked for; the engine
   *  refuses `""`, a group separator (`1,234.5` — guessing whether the comma
   *  groups or separates is not something a measurement field may do) and a value
   *  off the twip grid, each with its own sentence. The sentence is shown and the
   *  caller is told it has no number, so a dialog can decline to write rather than
   *  writing a value the reader never typed. */
  function parse(text) {
    if (!ensure()) return null;
    try {
      return io.engine.parseMeasurement(String(text ?? ""), chosen);
    } catch (error) {
      io.setStatus(String(error?.message ?? error), "error");
      return null;
    }
  }

  /** Installs `id`. Returns whether anything moved, so a caller can skip a
   *  repaint. An unknown id is refused rather than defaulted — a silently
   *  substituted unit is a wrong number presented as a right one. */
  function setUnit(id) {
    if (!ensure()) return false;
    if (!byId.has(id) || id === chosen) return false;
    chosen = id;
    writePref(UNIT_PREF_KEY, chosen, view);
    reflect();
    io.onChanged();
    return true;
  }

  /** Fills the chooser (once) and selects the unit in force. */
  function reflect() {
    const select = io.select;
    if (!select) return;
    if (!ensure()) return;
    if (select.options.length === 0) {
      for (const row of rows) {
        const option = select.ownerDocument.createElement("option");
        option.value = row.id;
        option.textContent = unitLabel(row.id);
        select.append(option);
      }
    } else {
      // A locale change renames the rows without changing the roster.
      for (const option of select.options) option.textContent = unitLabel(option.value);
    }
    select.value = chosen;
  }

  io.select?.addEventListener("change", () => setUnit(io.select.value));
  reflect();

  return {
    unitId: () => (ensure() ? chosen : null),
    unit,
    units: () => (ensure() ? rows : []),
    separator,
    format,
    display,
    parse,
    setUnit,
    reflect,
    /** Applies the unit in force to one numeric field: its spinner step, its
     *  bounds and the suffix beside it.
     *
     *  The bounds travel in the MARKUP as twips (`data-measure-min-twip` /
     *  `data-measure-max-twip`) rather than being converted from the inch
     *  attributes they used to carry: an inch `max="22"` means 31,680 twips
     *  whatever unit the field is labelled in, and re-deriving it from whatever
     *  happens to be in the attribute at the time would compound a rounding error
     *  every time the unit changed. `step` is the engine's own `stepTwip`, so a
     *  spinner never offers a digit the unit cannot show.
     *
     *  Complexity: O(1). */
    applyToField: (input) => {
      if (!input) return;
      const row = unit();
      if (!row) return;
      const min = Number(input.dataset.measureMinTwip ?? 0);
      const max = Number(input.dataset.measureMaxTwip ?? 0);
      input.step = format(row.stepTwip);
      input.min = format(min);
      if (max > 0) input.max = format(max);
      const suffix = input.parentElement?.querySelector("[data-measure-suffix]");
      if (suffix) suffix.textContent = row.suffix;
    },
    /** The palette / View-menu row. The label carries the unit in force, the same
     *  shape `view.compactRibbon` and `tools.smartQuotes` use, so a reader sees
     *  what is set without opening anything.
     *
     *  `noDoc`, and it is the point: the preference governs dialogs that open with
     *  no document — which is exactly why the engine made these free functions
     *  rather than methods on a document handle. */
    commands: () => [
      {
        id: "view.measurementUnits",
        label: t("units.command", { unit: unitLabel(ensure() ? chosen : "") }),
        group: "View",
        kw: "measurement units centimetres centimeters millimetres inches points picas ruler page setup margins indent",
        noDoc: true,
        run: () => io.openChooser(),
      },
    ],
  };
}
