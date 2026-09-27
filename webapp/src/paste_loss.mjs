// What the editor says when a paste carried the content but not a reference.
//
// The engine holds no prose. `EditResult.pasteLoss` hands back stable family
// keys — `bookmark`, `comment`, `note`, `fieldRange`, `trackedMove` — for the
// five kinds of reference a same-document copy cannot duplicate (a bookmark name
// is unique per document; one comment has one anchored range; Word duplicates a
// footnote rather than re-pointing at it; a duplicate field-range marker is
// rejected by the model outright; one tracked move has one destination).
// `casual-doc-edit/src/clone.rs` records the reason for each.
//
// This module is the host half: keys in, one localized sentence out. It exists
// as its own pure module for the reason `status_policy.mjs` and `edit_errors.mjs`
// do — the decision ("which families, in what order, phrased how") is worth
// testing, and inside `main.js` it would only be reachable through a DOM write.
//
// It is also the thing that makes the loss NOT silent, which `AGENTS.md` lists as
// a hard rule. Before this, 25 of `InlineNode`'s 29 variants were dropped by the
// structured paste with nothing said; the 19 that carry ink are now carried, and
// these five are the documented remainder.

import { list, t } from "./i18n.mjs";

/** The families the engine can report, in the order a sentence should name them.
 *
 *  Declared here rather than derived from the engine's output so an unknown key
 *  — a future family this host has no wording for — cannot reach the screen as a
 *  raw identifier. It is left out of the sentence and the sentence still names
 *  what it does know, which is better than printing `trackedMove` at a user.
 */
// Every key is written out LITERALLY. `t(`paste.loss.${key}`)` reads identically
// and is wrong: `build-locale.mjs` extracts the catalogue by scanning for `t("…")`
// and cannot see an interpolated key, so the five nouns would never reach
// `locales/en.json` and would print as raw identifiers in all nineteen languages.
// The thunk is what keeps the lookup lazy — this table is built at import, before
// any catalogue is installed.
const FAMILIES = [
  { key: "bookmark", label: () => t("paste.loss.bookmark") },
  { key: "comment", label: () => t("paste.loss.comment") },
  { key: "note", label: () => t("paste.loss.note") },
  { key: "fieldRange", label: () => t("paste.loss.fieldRange") },
  { key: "trackedMove", label: () => t("paste.loss.trackedMove") },
];

/**
 * The sentence for a paste that landed but could not carry every reference, or
 * `null` when it carried everything — which is the common case, and a message
 * that fires on every paste is a message nobody reads.
 *
 * @param {readonly string[] | undefined | null} kinds `EditResult.pasteLoss`.
 * @returns {string | null}
 */
export function pasteLossMessage(kinds) {
  if (!Array.isArray(kinds) || kinds.length === 0) return null;
  const wanted = new Set(kinds);
  const named = FAMILIES.filter((family) => wanted.has(family.key));
  if (!named.length) return null;
  // `Intl.ListFormat` rather than `", "`: "bookmarks, comments and field codes"
  // is punctuated differently in the eighteen other languages, and no call site
  // can be written correctly for all of them.
  return t("paste.loss", { items: list(named.map((family) => family.label())) });
}

/**
 * The known families in `kinds`, de-duplicated and in declaration order.
 *
 * Separate from the sentence so a test can assert the ORDER and the filtering
 * without depending on a catalogue being installed.
 *
 * @param {readonly string[] | undefined | null} kinds
 * @returns {string[]}
 */
export function pasteLossFamilies(kinds) {
  if (!Array.isArray(kinds) || kinds.length === 0) return [];
  const wanted = new Set(kinds);
  return FAMILIES.filter((family) => wanted.has(family.key)).map((family) => family.key);
}
