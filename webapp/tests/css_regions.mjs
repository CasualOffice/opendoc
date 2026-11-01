// The chrome / document split the editor's stylesheet guards share.
//
// `chrome_units.test.mjs` (px is refused in chrome rules) and
// `design_scale.test.mjs` (only the v2 scale in chrome rules) ask the same
// question first: is this rule drawing CHROME, or the DOCUMENT? One answer, so
// the two guards cannot disagree about which side of the line a selector is on.
// Not a test file: it has no `.test.` in its name, so the unit runner does not
// execute it on its own.

/** A selector's SUBJECT (its last compound) naming one of these is document
 *  region; so is any selector under `.overlay`. Keep this list in step with
 *  the prose at the top of the file and with `style.css`'s HF-413 block. */
export const DOCUMENT_SUBJECT = new RegExp(
  [
    String.raw`\.viewport\b`,
    String.raw`#viewport\b`,
    String.raw`#pages\b`,
    String.raw`\.pages(?![\w-])`,
    String.raw`\.page(?![\w-])`,
    String.raw`\.page-wrap\b`,
    String.raw`\.page-band\b`,
    String.raw`\.ruler`,
    // Edit Points grips are drawn over the page at the document's scale, like
    // the crop and resize grips under .overlay.
    String.raw`\.shape-points-`,
    String.raw`\.tab-corner\b`,
    String.raw`\.tab-glyph\b`,
    String.raw`\.caret(?![\w-])`,
    String.raw`\.remote-(?:caret|selection)\b`,
    String.raw`\.ime-preedit\b`,
    String.raw`\.editor-text-input\b`,
    String.raw`\.running-(?:band|marker)`,
    String.raw`\.fold-body-chevron\b`,
    String.raw`\.touch-(?:handle|magnifier)\b`,
    String.raw`\.reflow-table-`,
    String.raw`\.is-reflow\b`,
    String.raw`\.table-(?:gutter|col-|row-|column-strip|insert-target|move-indicator|cell-selection)`,
    String.raw`\.cell-outline\b`,
    String.raw`\.object-(?:align-guide|crop-|drop-caret|handle|move-pad|outline|resize-preview|rotate-handle|rotate-preview|rotate-tether)`,
    String.raw`\.review-margin-add\b`,
    String.raw`\.review-[\w-]*-marker\b`,
    String.raw`\.checklist-marker\b`,
  ].join("|"),
);


/** Blank out comments and quoted strings, keeping offsets, so prose and
 *  `content:` text are never read as lengths. */
export function mask(css) {
  return css.replace(/\/\*[\s\S]*?\*\/|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/g, (m) =>
    m.replace(/[^\n]/g, " "),
  );
}

/** Split on `seps` at bracket depth zero. */
export function splitTop(text, seps) {
  const out = [];
  let depth = 0;
  let cur = "";
  for (const ch of text) {
    if (ch === "(" || ch === "[") depth += 1;
    else if (ch === ")" || ch === "]") depth -= 1;
    if (depth === 0 && seps.includes(ch)) {
      out.push(cur);
      cur = "";
    } else cur += ch;
  }
  out.push(cur);
  return out;
}

export function isDocumentSelector(prelude) {
  return splitTop(prelude, ",").some((selector) => {
    if (selector.includes(".overlay")) return true;
    const parts = splitTop(selector.trim(), " >+~\n\t").filter((p) => p.trim());
    return parts.length > 0 && DOCUMENT_SUBJECT.test(parts[parts.length - 1]);
  });
}

/** Every declaration in `source`, with the rule it sits in. `document` is true
 *  when the rule, or any rule enclosing it, draws on the document. Comments and
 *  strings are masked first, so prose and `content:` text are never read.
 *  Complexity: O(length of the sheet). */
export function declarations(source) {
  const css = mask(source);
  const out = [];
  const stack = [];
  let segment = 0;
  const lineOf = (at) => css.slice(0, at).split("\n").length;
  for (let i = 0; i < css.length; i++) {
    const ch = css[i];
    if (ch === "{") {
      const prelude = css.slice(segment, i).split(";").pop().trim();
      const inherited = stack.some((s) => s.document);
      stack.push({ prelude, document: inherited || (!prelude.startsWith("@") && isDocumentSelector(prelude)) });
      segment = i + 1;
    } else if (ch === "}") {
      const top = stack[stack.length - 1];
      let offset = segment;
      for (const decl of css.slice(segment, i).split(";")) {
        const at = offset;
        offset += decl.length + 1;
        const colon = decl.indexOf(":");
        if (colon < 0 || !top || top.prelude.startsWith("@")) continue;
        out.push({
          selector: top.prelude.replace(/\s+/g, " "),
          property: decl.slice(0, colon).trim().toLowerCase(),
          value: source.slice(at + colon + 1, at + decl.length).trim(),
          line: lineOf(at),
          document: top.document,
        });
      }
      stack.pop();
      segment = i + 1;
    }
  }
  return out;
}
