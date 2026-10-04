// The measurement preference, checked as the thing it governs.
//
// `docs/153` `shell.measurement-units`. The engine has carried the conversion
// layer for a while — `casual_doc_layout::quantity` parses and formats `cm`,
// `mm`, `in`, `pt` and `pica` against the twip by exact integer arithmetic — and
// nothing in the chrome chose a unit, so Page setup was inches whatever a reader
// wanted (`SKILL` §9 rule 4: built is not reachable).
//
// WHAT THIS GUARDS, and why it is this and not "the select has five options":
// the whole capability is the claim that **the unit in force is the unit used**.
// A chooser that stores a preference which `format` and `parse` then ignore is
// the exact shape of a control that lies, and it is the shape this module had to
// be written carefully to avoid, because `format`/`parse` close over `chosen`
// rather than taking a unit argument. So every assertion here is about the
// OUTPUT for a chosen unit, not about which arguments the engine was handed —
// the guarantee, not the mechanism (`SKILL` §10).
//
// MUTATION PROOF. Making `format` and `parse` ignore `chosen` and always pass
// "inch" — the single most likely way for this to break, since it is what the
// code did before the preference existed — reddens five of the nine tests here:
//
//   the unit in force is the unit the fields are formatted in
//     AssertionError: 1440 twips in centimetres
//     + actual - expected   + '1'   - '2.54'
//   a value typed in the unit in force is read in that unit
//     AssertionError: 2.54 cm is an inch
//     + actual - expected   + 3658  - 1440
//   choosing a unit changes what every distance reads as
//   the preview caption carries the locale separator and the unit's own suffix
//   a field takes the unit's own spinner step and suffix
//
// The fake engine below is NOT a stub that echoes: it implements the same
// integer ratios and display precisions as `quantity.rs` (`per_unit`,
// `decimal_places`), because a fake that returned a constant would let the
// mutation pass and the guard would be decoration.
import assert from "node:assert/strict";
import test from "node:test";

import { EN_STRINGS } from "../src/en_strings.mjs";
import { setCatalogue, setLocale } from "../src/i18n.mjs";
import { UNIT_PREF_KEY, createMeasurementUnits } from "../src/measurement_units.mjs";

setCatalogue("en", { ...EN_STRINGS });
setLocale("en");

/** `quantity.rs`'s own table: (numerator, denominator) twips per unit, the
 *  display precision, and the suffix. Kept as the engine's integers rather than
 *  as floats so `2.54 cm` is 1440 twips exactly and not an approximation. */
const UNITS = {
  cm: { num: 72_000, den: 127, places: 2, suffix: "cm" },
  mm: { num: 7_200, den: 127, places: 1, suffix: "mm" },
  inch: { num: 1_440, den: 1, places: 2, suffix: "in" },
  point: { num: 20, den: 1, places: 1, suffix: "pt" },
  pica: { num: 240, den: 1, places: 2, suffix: "pi" },
};

const ORDER = ["cm", "mm", "inch", "point", "pica"];

/** `MeasurementUnit::step`: one display step in whole twips, never below one. */
function stepTwip(id) {
  const { num, den, places } = UNITS[id];
  return Math.max(1, Math.round(num / (den * 10 ** places)));
}

/** A faithful stand-in for the five free functions the wasm facade exports. */
function fakeEngine({ region = "inch", separator = "." } = {}) {
  return {
    measurementUnits: () =>
      JSON.stringify(
        ORDER.map((id) => ({
          id,
          suffix: UNITS[id].suffix,
          decimalPlaces: UNITS[id].places,
          stepTwip: stepTwip(id),
        })),
      ),
    defaultMeasurementUnit: () => region,
    decimalSeparatorForLanguage: () => separator,
    formatMeasurement(twips, unitId, sep) {
      const { num, den, places } = UNITS[unitId];
      const value = (Number(twips) * den) / num;
      return value.toFixed(places).replace(".", sep);
    },
    parseMeasurement(text, unitId) {
      const { num, den } = UNITS[unitId];
      const raw = String(text).trim();
      // The engine refuses a blank, a group separator and anything non-numeric
      // with its own sentence. The refusals matter here because `parse` must
      // return null rather than zero for each of them.
      if (raw === "") throw new Error("a measurement is required");
      if (/,\d{3}/.test(raw)) throw new Error("remove the group separator");
      if (!/^-?\d+(\.\d+)?$/.test(raw)) throw new Error(`${raw} is not a measurement`);
      return Math.round((Number(raw) * num) / den);
    },
  };
}

/** A `localStorage` that is a plain object, so a test needs no browser. */
function fakeView(seed = {}) {
  const store = new Map(Object.entries(seed));
  return {
    localStorage: {
      getItem: (k) => (store.has(k) ? store.get(k) : null),
      setItem: (k, v) => void store.set(k, String(v)),
      removeItem: (k) => void store.delete(k),
    },
    _store: store,
  };
}

/** The two properties of a `<select>` this module actually uses: an options
 *  list it appends to, and a `value` it writes. Not a DOM — the point of the
 *  module being pure is that this is all it needs. */
function fakeSelect() {
  const options = [];
  return {
    options,
    value: "",
    ownerDocument: {
      createElement: () => ({ value: "", textContent: "" }),
    },
    append: (option) => void options.push(option),
    addEventListener: () => {},
  };
}

function build({ seed = {}, region = "inch", separator = ".", locale = "en-US" } = {}) {
  const view = fakeView(seed);
  const status = [];
  let changed = 0;
  const units = createMeasurementUnits({
    engine: fakeEngine({ region, separator }),
    select: null,
    locale: () => locale,
    onChanged: () => void (changed += 1),
    setStatus: (text, kind) => void status.push({ text, kind }),
    openChooser: () => {},
    view,
  });
  return { units, status, view, changed: () => changed };
}

test("the unit in force is the unit the fields are formatted in", () => {
  const { units } = build({ seed: { [UNIT_PREF_KEY]: "cm" } });
  assert.equal(units.unitId(), "cm");
  // An inch IS 2.54 cm. If `format` reached for inches instead of the unit in
  // force this would read "1".
  assert.equal(units.format(1440), "2.54", "1440 twips in centimetres");
  assert.equal(units.format(0), "0", "a whole value keeps one digit, never ''");
});

test("a value typed in the unit in force is read in that unit", () => {
  const { units } = build({ seed: { [UNIT_PREF_KEY]: "cm" } });
  // 2.54 cm is exactly one inch. Parsing it as inches would give 3658 twips —
  // a page two and a half times too wide, written without a word to the reader.
  assert.equal(units.parse("2.54"), 1440, "2.54 cm is an inch");
  assert.equal(units.parse("1"), 567, "1 cm, on the twip grid");
});

test("choosing a unit changes what every distance reads as", () => {
  const { units, changed } = build({ seed: { [UNIT_PREF_KEY]: "inch" } });
  assert.equal(units.format(1440), "1");
  assert.equal(units.setUnit("point"), true, "a real change is reported as one");
  assert.equal(units.unitId(), "point");
  assert.equal(units.format(1440), "72", "an inch is 72 points");
  assert.equal(units.parse("72"), 1440);
  assert.equal(changed(), 1, "the caller is told once, so it can repaint");
  assert.equal(units.setUnit("point"), false, "choosing the same unit moves nothing");
  assert.equal(changed(), 1);
});

test("the chosen unit outlives the session", () => {
  const first = build({ seed: {} });
  first.units.setUnit("mm");
  const carried = Object.fromEntries(first.view._store);
  assert.equal(carried[UNIT_PREF_KEY], "mm", "the preference is written, not held");
  const second = build({ seed: carried });
  assert.equal(second.units.unitId(), "mm");
});

test("the preview caption carries the locale separator and the unit's own suffix", () => {
  // `display` is the one that speaks to a reader, so it takes the locale's
  // separator — "2,54 cm" in French — while `format` must not, because an
  // <input type="number"> cannot hold a comma.
  const { units } = build({ seed: { [UNIT_PREF_KEY]: "cm" }, separator: ",", locale: "fr-FR" });
  assert.equal(units.display(1440), "2,54", "prose takes the comma");
  assert.equal(units.format(1440), "2.54", "the field never does");
  assert.equal(units.unit().suffix, "cm");
});

test("a field takes the unit's own spinner step and suffix", () => {
  const { units } = build({ seed: { [UNIT_PREF_KEY]: "cm" } });
  const suffix = { textContent: "" };
  const input = {
    dataset: { measureMinTwip: "0", measureMaxTwip: "31680" },
    step: "",
    min: "",
    max: "",
    parentElement: { querySelector: () => suffix },
  };
  units.applyToField(input);
  // 6 twips is one display step of a centimetre. A spinner stepping by an
  // inch's 14 twips in a field labelled cm offers digits the unit cannot show.
  assert.equal(input.step, units.format(6));
  assert.equal(input.max, "55.88", "22 inches of bound, read in centimetres");
  assert.equal(suffix.textContent, "cm");
});

test("a refusal is a refusal and never a silent zero", () => {
  const { units, status } = build({ seed: { [UNIT_PREF_KEY]: "inch" } });
  // The old helper read a blank margin field as 0 and wrote a 0-inch margin
  // nobody asked for. `null` is what lets a dialog decline to write at all.
  assert.equal(units.parse(""), null, "a blank field");
  assert.equal(units.parse("abc"), null, "not a measurement");
  assert.equal(units.parse("1,234.5"), null, "a group separator is not guessed at");
  assert.equal(status.length, 3, "each refusal says something");
  assert.ok(
    status.every((entry) => entry.kind === "error" && entry.text.length > 0),
    "and says it as an error carrying the engine's own sentence",
  );
});

test("a stored unit the engine does not recognise is discarded, not trusted", () => {
  // A build that drops a unit, or a hand-edited value, must not leave every
  // dialog formatting against a unit that no longer exists.
  const { units } = build({ seed: { [UNIT_PREF_KEY]: "furlong" }, region: "cm" });
  assert.equal(units.unitId(), "cm", "the region default answers instead");
  assert.equal(units.setUnit("furlong"), false, "and an unknown id cannot be installed");
  assert.equal(units.unitId(), "cm");
});

test("the command row says which unit is in force", () => {
  // The same shape `view.compactRibbon` and `tools.smartQuotes` use: a reader
  // sees the state without opening anything.
  const { units } = build({ seed: { [UNIT_PREF_KEY]: "point" } });
  const [row] = units.commands();
  assert.equal(row.id, "view.measurementUnits");
  assert.equal(row.noDoc, true, "the preference governs dialogs that open with no document");
  assert.match(row.label, /point/i, `the label carried no unit: ${row.label}`);
});

// THE BOOT ORDER, which took the editor down on `main` at 5cbbaf12.
//
// `createMeasurementUnits` is called while `main.js` is still being EVALUATED.
// `boot()` is the last statement in that file, so `await init()` has not run
// yet and the wasm module is not instantiated. A free function on an
// uninstantiated module does not return nothing — it throws
//
//   Uncaught TypeError: Cannot read properties of undefined
//     (reading '__wbindgen_add_to_stack_pointer')
//       at Object.measurementUnits (casual_doc_wasm.js:12257)
//       at createMeasurementUnits (measurement_units.mjs:97)
//       at main.js:10486
//
// and a throw during module evaluation means every statement after it never
// runs. The editor did not degrade; it did not open at all.
//
// So this asserts the GUARANTEE — constructing before the engine exists is
// survivable, and the roster arrives when the engine does — rather than the
// mechanism, which is a `let` and a fault-in function and could be rewritten.
test("constructing before the engine is instantiated does not throw", () => {
  const view = fakeView({});
  // Exactly what `main.js` hands it pre-`init()`: the bindings are imported and
  // callable, and calling one throws.
  const uninstantiated = {
    measurementUnits: () => {
      throw new TypeError(
        "Cannot read properties of undefined (reading '__wbindgen_add_to_stack_pointer')",
      );
    },
    defaultMeasurementUnit: () => {
      throw new TypeError("Cannot read properties of undefined");
    },
    decimalSeparatorForLanguage: () => {
      throw new TypeError("Cannot read properties of undefined");
    },
    formatMeasurement: () => {
      throw new TypeError("Cannot read properties of undefined");
    },
    parseMeasurement: () => {
      throw new TypeError("Cannot read properties of undefined");
    },
  };
  const select = fakeSelect();
  let units;
  assert.doesNotThrow(() => {
    units = createMeasurementUnits({
      engine: uninstantiated,
      select,
      locale: () => "en-US",
      onChanged: () => {},
      setStatus: () => {},
      openChooser: () => {},
      view,
    });
  }, "the factory must survive being called before init()");

  // Nothing to show yet, and nothing that crashes a caller who asks early.
  assert.doesNotThrow(() => units.reflect(), "reflect() before init() is a no-op");
  assert.deepEqual(units.units(), [], "no roster before the engine has one");
  assert.equal(units.unitId(), null);
  assert.doesNotThrow(() => units.applyToField(null));
  assert.equal(select.options.length, 0, "the chooser stays empty until primed");

  // Now the engine arrives, which is `boot()` reaching `measurement.reflect()`
  // after `await init()`. The roster faults in and the chooser fills.
  const ready = fakeEngine({ region: "inch", separator: "." });
  for (const key of Object.keys(uninstantiated)) uninstantiated[key] = ready[key];
  units.reflect();
  assert.ok(units.units().length > 0, "the roster arrives with the engine");
  assert.equal(units.unitId(), "inch");
  assert.equal(
    select.options.length,
    units.units().length,
    "the chooser is filled once, by the priming call",
  );
  assert.equal(units.format(1440), "1", "and the unit in force is usable");
});
