// Twips, EMUs and the inches a person types — checked as arithmetic.
//
// Extracted from `main.js` (`109` HF-085) where each of these was welded to the
// `<input>` it read, so the cases that actually bite — a blank field, a field
// holding "abc", the difference between "unset" and "zero" — needed a browser.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  EMU_PER_TWIP,
  TWIPS_PER_INCH,
  inchesToTwips,
  optionalInchesToTwips,
  round2,
  signedInchesToTwips,
  twipsToDialogInches,
  twipsToEmu,
  twipsToInchText,
} from "../src/units.mjs";

// Both constants are fixed by OOXML, and they have to agree: an inch is 1440
// twips AND 914400 EMUs. Anything else silently misplaces every floating
// object on the page.
test("the unit constants agree with OOXML", () => {
  assert.equal(TWIPS_PER_INCH, 1440);
  assert.equal(EMU_PER_TWIP, 635);
  assert.equal(TWIPS_PER_INCH * EMU_PER_TWIP, 914400);
  assert.equal(twipsToEmu(TWIPS_PER_INCH), 914400);
});

test("round2 keeps two decimals and does not drift", () => {
  assert.equal(round2(1.005), 1.0); // the IEEE value of 1.005 is just under
  assert.equal(round2(2.346), 2.35);
  assert.equal(round2(-0.004), -0);
  assert.equal(round2(3), 3);
});

test("inches text round-trips whole, half and quarter inches without trailing zeros", () => {
  assert.equal(twipsToInchText(1440), "1");
  assert.equal(twipsToInchText(2160), "1.5");
  assert.equal(twipsToInchText(360), "0.25");
  assert.equal(twipsToInchText(14400), "10");
});

// An indent field showing "0" reads as a value somebody set. Zero is blank.
test("zero twips is an empty indent field, not \"0\"", () => {
  assert.equal(twipsToInchText(0), "");
});

// A dialog is the other way round: blank and 0 are different answers there, so
// the engine's "unset" sentinel (negative) is the one that blanks the box.
test("a dialog shows \"0\" for zero and an empty box for unset", () => {
  assert.equal(twipsToDialogInches(0), "0");
  assert.equal(twipsToDialogInches(-1), "");
  assert.equal(twipsToDialogInches(1440), "1");
  assert.equal(twipsToDialogInches(2160), "1.5");
});

test("a blank or nonsense measurement is zero, never NaN", () => {
  for (const raw of ["", "   ", "abc", "1.2.3", null, undefined]) {
    assert.equal(inchesToTwips(raw), 0, `"${raw}"`);
    assert.equal(signedInchesToTwips(raw), 0, `"${raw}"`);
  }
});

test("a margin cannot be negative, but an indent can", () => {
  assert.equal(inchesToTwips("-2"), 0, "a negative margin is clamped, not accepted");
  assert.equal(signedInchesToTwips("-2"), -2880, "a hanging indent is legitimately negative");
});

test("surrounding whitespace is not a typing error", () => {
  assert.equal(inchesToTwips("  1.5  "), 2160);
  assert.equal(signedInchesToTwips("\t-0.5\n"), -720);
  assert.equal(optionalInchesToTwips("   "), -1);
});

// -1 is the engine's "leave this alone", so a blank optional box must not
// become a real zero-width column or a zero-height row.
test("an empty optional field means unset, and 0 means zero", () => {
  assert.equal(optionalInchesToTwips(""), -1);
  assert.equal(optionalInchesToTwips("0"), 0);
  assert.equal(optionalInchesToTwips("1"), 1440);
});

test("inches and twips round-trip through the dialog text", () => {
  for (const twips of [0, 360, 720, 1440, 2160, 14400]) {
    assert.equal(inchesToTwips(twipsToDialogInches(twips)), twips);
  }
});
