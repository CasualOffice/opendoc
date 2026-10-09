// Which object an insert just created.
//
// The engine's insert commands report the CARET, not the object they created,
// and every Word-standard insert has to land on that object: a text box is
// entered, a shape is left selected with Fill and Outline in reach, a chart is
// selected with its data open. Diffing the placed-object order before and after
// is how all three find it, so the diff lives once, here.
//
// Lifted out of `main.js` with the chart panel, to pay for that panel's wiring
// under the line ratchet.
//
// Cost: one `objectOrder` read — O(placed objects) — per insert, never per
// keystroke or pointer move.

/** The object that appeared since `before` (a set of node ids), optionally of a
 *  given kind, or `null`. */
export function newestObject(doc, before, kind) {
  let objects;
  try {
    objects = JSON.parse(doc.objectOrder());
  } catch {
    return null;
  }
  return objects.find((entry) => !before.has(entry.node) && (!kind || entry.kind === kind)) ?? null;
}

/** The node ids of every placed object right now — the "before" side of the diff. */
export function placedObjectIds(doc) {
  try {
    return new Set(JSON.parse(doc.objectOrder()).map((entry) => entry.node));
  } catch {
    return new Set();
  }
}
