// Page borders: Word's Borders and Shading ▸ Page Border, as a dialog (HF-193).
//
// The whole stack below the chrome existed — `PageBorders` in the model, import
// of `w:pgBorders`, the layout's page-border module, the painter, the writer —
// and nothing could add, change or remove one. `setPageBorders` is the one
// facade write (`SetSectionPageBorders`, one operation per section, one undo
// step), `pageBorders` the read.
//
// THE SHAPE, FROM THE REFERENCES.
//
//   * Word: Design ▸ Page Borders opens Borders and Shading on its Page Border
//     tab — Setting (None, Box, Shadow, 3-D, Custom), Style, Color, Width, Art,
//     a preview whose four edge buttons switch each edge, "Apply to" (Whole
//     document, This section, first page only, all except first page), and
//     Options (measure from Edge of page or Text, and four margins).
//   * ONLYOFFICE ships no page-border dialog at all: no view, no locale key, no
//     API method; sdkjs touches `pgBorders` only in serialisation
//     (`docs/153` row `page.borders`). So this is ahead of them, not parity.
//   * Google Docs has no page borders.
//
// So: Word's controls that the model carries. The four edges are checkboxes
// (Word's preview buttons, as a keyboard-reachable list), "Show on" is Word's
// first-page / all-but-first split of Apply to, and "Apply to" keeps Word's
// Whole document / This section. NOT offered, and said here rather than
// pretended: Art borders (`w:pgBorders` art values need a glyph set the painter
// does not draw), Shadow and 3-D (two-tone edges), and per-edge margins — one
// spacing applies to all four, because the dialog writes the four edges alike.
//
// The dialog writes what it shows. An imported border whose edges differ opens
// on its top edge's values, and the edge checkboxes reflect which edges exist.
import { n, t } from "./i18n.mjs";
import { registerModal } from "./modal.mjs";

/** Word's line widths, in eighth-points (`w:sz`). */
export const BORDER_WIDTHS = Object.freeze([2, 4, 6, 8, 12, 18, 24, 36, 48]);

/** The line styles offered, as `ST_Border` tokens the painter draws. */
export const BORDER_STYLES = Object.freeze(["single", "double", "dotted", "dashed", "dotDash", "triple", "thick", "wave"]);

/** The four edges, in the model's names, with the control that switches each. */
const EDGES = Object.freeze([
  ["top", "pageBordersTop"],
  ["bottom", "pageBordersBottom"],
  ["start", "pageBordersLeft"],
  ["end", "pageBordersRight"],
]);

/** Word's defaults for a new border: 24pt from the page edge, 4pt from text. */
const DEFAULT_SPACE = Object.freeze({ page: 24, text: 4 });

/** `#rrggbb` → the model's `{r, g, b}`; black for anything unreadable. */
export function rgbOf(hex) {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(String(hex ?? ""));
  return m ? { r: parseInt(m[1], 16), g: parseInt(m[2], 16), b: parseInt(m[3], 16) } : { r: 0, g: 0, b: 0 };
}

/** The model's `{r, g, b}` → `#rrggbb`, black when absent (`w:color="auto"`). */
export function hexOf(color) {
  if (!color) return "#000000";
  return `#${[color.r, color.g, color.b].map((c) => Number(c).toString(16).padStart(2, "0")).join("")}`;
}

/**
 * The `PageBorders` value the dialog's fields describe, in the model's camelCase
 * shape. No edge checked is the empty value, which removes the borders — and
 * then `display` and `offsetFrom` go too, so "None" leaves nothing behind.
 *
 * Pure, so the mapping the dialog applies is a unit test's business.
 *
 * @param {{edges: string[], style: string, sizeEighthPoints: number, color: string,
 *          spacePoints: number, display: string, offsetFrom: string}} form
 */
export function bordersFromForm(form) {
  const edges = new Set(form.edges ?? []);
  if (edges.size === 0) return {};
  const space = Math.max(0, Math.min(31, Math.round(Number(form.spacePoints) || 0)));
  const edge = {
    style: form.style || "single",
    sizeEighthPoints: Number(form.sizeEighthPoints) || 4,
    color: rgbOf(form.color),
    spacePoints: space,
  };
  const out = { display: form.display || "allPages", offsetFrom: form.offsetFrom || "page" };
  for (const [name] of EDGES) if (edges.has(name)) out[name] = { ...edge };
  return out;
}

/**
 * Wires the Page borders dialog.
 *
 * @param {object} io
 * @param {() => object|null} io.getDoc
 * @param {() => string|null} io.selectionNode the caret's paragraph, or null
 * @param {(thunk: () => unknown, options?: object) => Promise<boolean>} io.runEdit
 * @param {(text: string, kind?: string) => void} io.setStatus
 * @param {() => Element|null} io.fallbackFocus
 * @returns {{open: () => void}}
 */
export function createPageBorders(io) {
  const el = (id) => document.getElementById(id);
  const dialog = el("pageBordersDialog");
  if (!dialog) return { open() {} };
  const style = el("pageBordersStyle");
  const width = el("pageBordersWidth");
  const color = el("pageBordersColor");
  const spacing = el("pageBordersSpacing");
  const display = el("pageBordersDisplay");
  const offsetFrom = el("pageBordersOffsetFrom");
  const applyTo = el("pageBordersApplyTo");
  const applyToField = el("pageBordersApplyToField");
  const edgeBoxes = EDGES.map(([name, id]) => [name, el(id)]);

  // Width options are numbers and a unit symbol, identical in every language
  // but for the digits, so they are written from script in the locale's own.
  for (const size of BORDER_WIDTHS) {
    const option = document.createElement("option");
    option.value = String(size);
    option.textContent = `${n(size / 8, { maximumFractionDigits: 2 })} pt`;
    width.appendChild(option);
  }

  let state = null;
  const modal = registerModal(dialog, {
    initialFocus: () => edgeBoxes[0][1],
    fallbackFocus: io.fallbackFocus,
    defaultAction: () => void apply(),
  });

  /** Paints the fields from the engine's answer for the caret's section. */
  function reflect(read) {
    state = read;
    const b = read.borders ?? {};
    const first = b.top ?? b.bottom ?? b.start ?? b.end ?? null;
    // A section with no border opens on a box, all four edges on: whoever opened
    // Page borders came to add one. One that has a border shows its own edges.
    for (const [name, box] of edgeBoxes) box.checked = first ? !!b[name] : true;
    style.value = first && BORDER_STYLES.includes(first.style) ? first.style : "single";
    if (first && !BORDER_STYLES.includes(first.style)) {
      // An imported token this list does not offer joins it rather than being
      // silently rewritten to "single" on Apply.
      const option = document.createElement("option");
      option.value = first.style;
      option.textContent = first.style;
      style.appendChild(option);
      style.value = first.style;
    }
    const size = first?.sizeEighthPoints ?? 4;
    if (![...width.options].some((o) => o.value === String(size))) {
      const option = document.createElement("option");
      option.value = String(size);
      option.textContent = `${n(size / 8, { maximumFractionDigits: 2 })} pt`;
      width.appendChild(option);
    }
    width.value = String(size);
    // Relabelled on every open, in the locale active NOW: the options were built
    // at boot, before a catalogue or a language change could reach them.
    for (const option of width.options) option.textContent = `${n(Number(option.value) / 8, { maximumFractionDigits: 2 })} pt`;
    color.value = hexOf(first?.color);
    offsetFrom.value = b.offsetFrom === "text" ? "text" : "page";
    spacing.value = String(first?.spacePoints ?? DEFAULT_SPACE[offsetFrom.value]);
    display.value = ["allPages", "firstPage", "notFirstPage"].includes(b.display) ? b.display : "allPages";
    // "Apply to" only means something with more than one section.
    const several = (read.sections?.length ?? 0) > 1;
    applyToField.hidden = !several;
    applyTo.value = "document";
  }

  function open() {
    const doc = io.getDoc();
    if (!doc) {
      io.setStatus(t("command.needsDocument"), "error");
      return;
    }
    const raw = doc.pageBorders(io.selectionNode() ?? "");
    const read = raw && raw !== "null" ? JSON.parse(raw) : null;
    if (!read) {
      io.setStatus(t("pageBorders.noSection"), "error");
      return;
    }
    reflect(read);
    modal.open();
  }

  async function apply() {
    const doc = io.getDoc();
    if (!doc || !state) return;
    // Out of range is refused at the field, in the browser's own words, and the
    // dialog stays open — never clamped in silence (`docs/141` TBL-25).
    if (!spacing.checkValidity()) {
      spacing.reportValidity();
      return;
    }
    const borders = bordersFromForm({
      edges: edgeBoxes.filter(([, box]) => box.checked).map(([name]) => name),
      style: style.value,
      sizeEighthPoints: Number(width.value),
      color: color.value,
      spacePoints: Number(spacing.value),
      display: display.value,
      offsetFrom: offsetFrom.value,
    });
    const sections = applyTo.value === "section" ? [state.section] : state.sections;
    const landed = await io.runEdit(() => doc.setPageBorders(JSON.stringify({ sections, borders })), { gate: true });
    modal.close("apply");
    if (landed) io.setStatus(Object.keys(borders).length ? t("pageBorders.applied") : t("pageBorders.removed"));
  }

  // Switching what the spacing is measured from moves it to that origin's
  // default, as Word's Options dialog does, unless the reader already typed one.
  let spacingTouched = false;
  spacing.addEventListener("input", () => {
    spacingTouched = true;
  });
  offsetFrom.addEventListener("change", () => {
    if (!spacingTouched) spacing.value = String(DEFAULT_SPACE[offsetFrom.value] ?? 24);
  });
  for (const id of ["pageBordersCancel", "pageBordersClose"]) {
    el(id)?.addEventListener("click", () => modal.close("cancel"));
  }
  el("pageBordersApply")?.addEventListener("click", () => void apply());

  return {
    open() {
      spacingTouched = false;
      open();
    },
  };
}
