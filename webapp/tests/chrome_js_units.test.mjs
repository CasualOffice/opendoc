// The chrome is rem from SCRIPT too (`docs/109` UX-056, `docs/184` D8,
// approved by the owner 2026-11-01).
//
// `chrome_units.test.mjs` holds the stylesheets to rem. The audit then counted
// 165 px styles written from JavaScript, and a px written into `el.style` is
// exactly as fixed as one written into `style.css`: it stays the same size while
// the fluid root grows or shrinks the chrome around it. Most of those writes are
// legitimately px — positions and sizes MEASURED from the pointer, the layout
// engine or `getBoundingClientRect()`, which are px by nature — so this guard
// does not refuse px from script in general. It refuses the two shapes that are
// never measured:
//
//   * a TYPOGRAPHY or SPACING property (font size, padding, margin, gap, line
//     height, radius, letter spacing) set to a px value from script; chrome type
//     and spacing come from the stylesheet's tokens, and
//   * a px LITERAL (`"12px"`) for any property — a literal is a design length,
//     not a measurement, whichever property it is written to.
//
// Allowed by name, each with its reason: modules that draw the DOCUMENT (a page
// thumbnail laid out in points, a style preview at the style's own size,
// overlay grips), and the zero / off-screen literals that hide or pin an
// element rather than size it.
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import test from "node:test";

const SRC = new URL("../src/", import.meta.url);

const TYPE_OR_SPACE =
  /^(?:fontSize|font-size|padding|padding[A-Z-]\w*|margin|margin[A-Z-]\w*|gap|rowGap|columnGap|row-gap|column-gap|lineHeight|line-height|borderRadius|border-radius|letterSpacing|letter-spacing)$/;

/** file → why its px writes are document geometry, not chrome. */
const DOCUMENT_MODULES = new Map([
  ["command_menu.mjs", "the page thumbnail lays the document out in points"],
  ["style_preview.mjs", "the Styles gallery previews each style at its own size"],
  ["table_chrome.mjs", "table grips are drawn on the page at the document's scale"],
  ["editor_header_footer.mjs", "the running band marker sits on the page"],
]);

/** A literal that hides or pins rather than sizes. */
const NEUTRAL_LITERAL = /^-?(?:0|99999)px$/;

/** Every px write in one module's source, as offenders. */
export function scriptPxOffenders(source, file) {
  const out = [];
  const lines = source.split("\n");
  const at = (index) => source.slice(0, index).split("\n").length;
  const patterns = [
    // el.style.fontSize = `${n}px` | "12px"
    /\.style\.([A-Za-z]+)\s*=\s*([^;\n]+)/g,
    // el.style.setProperty("padding", "12px")
    /\.style\.setProperty\(\s*["'`]([\w-]+)["'`]\s*,\s*([^;\n]+)\)/g,
  ];
  for (const re of patterns) {
    for (const m of source.matchAll(re)) {
      const [, property, value] = m;
      if (!/px/.test(value)) continue;
      const line = at(m.index);
      if (/^\s*\/\//.test(lines[line - 1])) continue;
      const literal = value.trim().match(/^["'`](-?[\d.]+px)["'`]$/);
      const where = `${file}:${line} ${m[0].trim().slice(0, 100)}`;
      if (literal && !NEUTRAL_LITERAL.test(literal[1])) {
        out.push(`${where} — a px literal is a design length: put it in the stylesheet in rem`);
      } else if (TYPE_OR_SPACE.test(property) && !property.startsWith("--")) {
        out.push(`${where} — chrome type and spacing come from the stylesheet's tokens`);
      }
    }
  }
  return out;
}

test("no chrome module writes a px type/spacing length or a px literal from script", () => {
  const files = readdirSync(SRC).filter((f) => /\.(?:mjs|js)$/.test(f));
  assert.ok(files.length > 150, `expected the editor's modules, found ${files.length}`);
  const offenders = [];
  for (const file of files) {
    if (DOCUMENT_MODULES.has(file)) continue;
    offenders.push(...scriptPxOffenders(readFileSync(new URL(file, SRC), "utf8"), file));
  }
  assert.deepEqual(
    offenders,
    [],
    "the chrome is rem (docs/184 D8): set the length in style.css in rem, or — if the module " +
      "draws the DOCUMENT — add it to DOCUMENT_MODULES with the reason:\n  " +
      offenders.join("\n  "),
  );
});

test("the script guard tells a measured position from a design length", () => {
  const mutated = [
    "chip.style.padding = `${4}px`;",
    'label.style.fontSize = "11px";',
    'menu.style.width = "240px";',
    'panel.style.setProperty("gap", "6px");',
    "menu.style.left = `${rect.left}px`;",
    "box.style.height = `${h * sy}px`;",
    'el.style.top = "-99999px";',
    'root.style.setProperty("--plugin-panel-w", `${width}px`);',
  ].join("\n");
  const found = scriptPxOffenders(mutated, "mutation.mjs");
  assert.equal(found.length, 4, found.join("\n"));
});
