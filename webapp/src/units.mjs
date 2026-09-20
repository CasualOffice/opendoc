// The measurement arithmetic the dialogs and the ruler run on.
//
// Word's document model counts twips (1/1440 in) and OOXML's drawing model
// counts EMUs (1/914400 in); the person typing into a dialog counts inches.
// Every conversion between the three used to live inside `main.js` next to the
// element it read, so the rules that actually have edge cases — a blank field,
// a field holding "abc", the difference between "no value" and "zero", how many
// decimals a round-trip may lose — were only reachable through a DOM node
// (`109` HF-085). They are pure arithmetic and are unit-tested here instead.
//
// The parsers take the RAW field text rather than an element: reading `.value`
// is the caller's job, and it is the only part that needs a browser.

/** Twips per inch. The unit the engine reports page and paragraph geometry in. */
export const TWIPS_PER_INCH = 1440;

/** EMUs per twip. Object geometry (position, size) is EMUs on the engine side
 *  and twips on the painting side; this is the only factor between them. */
export const EMU_PER_TWIP = 635;

/** Two decimal places, for a readout a person reads while dragging. */
export function round2(n) {
  return Math.round(n * 100) / 100;
}

/** Twips → EMUs, for the object ops that take EMUs. */
export function twipsToEmu(twips) {
  return twips * EMU_PER_TWIP;
}

/** Twips → the shortest inches text that still round-trips at 2 dp: `1440` →
 *  `"1"`, `2160` → `"1.5"`, `360` → `"0.25"`. Zero is the empty string,
 *  because a ruler/indent field showing "0" reads as a value someone set. */
export function twipsToInchText(twips) {
  if (!twips) return "";
  return (twips / TWIPS_PER_INCH).toFixed(2).replace(/\.?0+$/, "");
}

/**
 * Twips → the inches text a DIALOG field shows.
 *
 * Differs from `twipsToInchText` in both directions, and both are deliberate:
 * a NEGATIVE value means "not set" (the engine's sentinel for an automatic row
 * height or an unset width) and shows as an empty field, while a genuine zero
 * shows as `"0"` — in a dialog, a blank box and a box holding 0 mean different
 * things, and `"0"` is the one the user set.
 */
export function twipsToDialogInches(twips) {
  return twips < 0 ? "" : (twips / TWIPS_PER_INCH).toFixed(2).replace(/\.?0+$/, "") || "0";
}

/** An inches field's text → twips, never negative. Blank or non-numeric → 0,
 *  which is what an empty margin/indent box means. */
export function inchesToTwips(text) {
  const raw = String(text ?? "").trim();
  if (raw === "" || !Number.isFinite(Number(raw))) return 0;
  return Math.max(0, Math.round(Number(raw) * TWIPS_PER_INCH));
}

/** An inches field's text → signed twips (a hanging indent or a negative
 *  offset is legitimate). Blank or non-numeric → 0. */
export function signedInchesToTwips(text) {
  const raw = String(text ?? "").trim();
  if (raw === "" || !Number.isFinite(Number(raw))) return 0;
  return Math.round(Number(raw) * TWIPS_PER_INCH);
}

/** An OPTIONAL inches field's text → twips, or `-1` for "leave it unset" —
 *  the engine's sentinel, and the reason a blank box cannot just mean 0. */
export function optionalInchesToTwips(text) {
  const raw = String(text ?? "").trim();
  return raw === "" ? -1 : Math.round(Number(raw) * TWIPS_PER_INCH);
}
