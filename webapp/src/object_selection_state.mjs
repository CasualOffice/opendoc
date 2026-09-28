// What `#pages` says about the selected object, so the host and the tests can
// read the object-selection grammar without reaching into the overlay.
//
// It moved out of `main.js` because the interesting question about it is not a
// browser question. "Does the dataset drop every key when the selection goes
// away" is exactly the shape of bug that leaves a stale `data-object-kind` on
// the element after a deselect, and it is answerable in node against a plain
// object with a `dataset` (`object_selection_state.test.mjs`). It also puts the
// selection keys and the shape-format keys in one place: they are written
// together, cleared together, and were drifting apart in two functions.
//
// Cost: O(capability keys), a fixed nine — no walk of anything.

/** The dataset keys that describe the selected object itself. Listed once so a
 *  clear cannot fall behind a write; the previous pair of functions spelled the
 *  same eight keys out twice, once to set and once to delete. */
const SELECTION_KEYS = [
  "objectSelected",
  "objectSurface",
  "objectRoot",
  "objectSubject",
  "objectPath",
  "objectKind",
  "objectMode",
  "objectCapabilities",
];

/** The dataset keys that describe a selected SHAPE's own fill and outline. */
const SHAPE_KEYS = ["shapeFill", "shapeOutline", "shapeOutlineWidth"];

/**
 * Writes the object-selection grammar onto `host.dataset`, and clears every key
 * it owns when there is no selection.
 *
 * O(1).
 *
 * @param {{dataset: Record<string, string>}} host normally `#pages`
 * @param {object|null} selection the live object selection, or null
 * @param {string[]} capabilityKeys the engine-declared capability field names
 */
export function reflectObjectSelection(host, selection, capabilityKeys) {
  if (!selection) {
    for (const key of SELECTION_KEYS) delete host.dataset[key];
    return;
  }
  host.dataset.objectSelected = selection.node;
  host.dataset.objectSurface = selection.ref.surface;
  host.dataset.objectRoot = selection.ref.root;
  host.dataset.objectSubject = selection.ref.subject;
  host.dataset.objectPath = selection.ref.path.join(".");
  host.dataset.objectKind = selection.kind;
  host.dataset.objectMode = selection.mode;
  host.dataset.objectCapabilities = capabilityKeys.filter((key) => selection[key]).join(",");
}

/**
 * Writes a selected shape's fill and outline onto `host.dataset`. It is what the
 * Fill / Outline swatches paint from, so what a test reads here is what the user
 * sees on the control — `none` when the shape has no fill or no outline, which
 * is a different answer from "not a shape" (every key gone).
 *
 * O(1).
 *
 * @param {{dataset: Record<string, string>}} host normally `#pages`
 * @param {{fill?: string|null, outline?: string|null, outlineWidthEmu?: number|null}|null} format
 */
export function reflectShapeFormat(host, format) {
  if (!format) {
    for (const key of SHAPE_KEYS) delete host.dataset[key];
    return;
  }
  host.dataset.shapeFill = format.fill ?? "none";
  host.dataset.shapeOutline = format.outline ?? "none";
  host.dataset.shapeOutlineWidth =
    format.outlineWidthEmu == null ? "none" : String(format.outlineWidthEmu);
}
