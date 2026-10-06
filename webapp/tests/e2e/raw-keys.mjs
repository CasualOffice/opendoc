// A catalogue key is never a word anybody reads.
//
// `t(key)` returns the KEY when no catalogue in the chain answers it — on purpose,
// because a visible `appMenuBar.format` is a bug report and an empty label is a
// bug that ships (`i18n.mjs`). That only works if something LOOKS. Nothing did:
// on a phone the Aa and + buttons were announced as "appMenuBar.format" and
// "appMenuBar.insert" — their `aria-label` and `title` — for as long as anyone
// had used the phone chrome, while both keys sat translated in every catalogue.
// The cause was a class rather than a typo (see `compact_toolbar.mjs`'s
// `nameFromKey`): a script-built element resolved a key the MARKUP declares
// before the catalogue that carries it had arrived, and cached the miss.
//
// So this is a sweep, not a check on two buttons. Every element in the chrome
// is read for the four strings a person meets — `aria-label`, `title`,
// `placeholder`, and its own visible text — and anything shaped like a dotted
// key fails, wherever it came from. Attributes are read on HIDDEN elements too:
// the menus a bar hangs off `<body>` are hidden until opened and carry their
// names the whole time, which is exactly where the phone's two leaked.
//
// Shared rather than written into one spec because the defect is not one
// device's: the phone spec and the desktop spec run in different Playwright
// projects (`playwright.config.mjs` splits them by filename), and a guard that
// lived in only one of them would watch only one of the chromes.

/** The shape of a catalogue key: a lower-case head and at least one dotted
 *  segment. `sample.docx` has the shape too — which is why the DOCUMENT is out
 *  of scope below, rather than why this pattern is looser than it looks. */
export const RAW_KEY_SHAPE = /^[a-z][A-Za-z0-9]*(\.[A-Za-z0-9]+)+$/;

/** Surfaces whose words belong to the document or to the person, not to the
 *  chrome: the pages, the screen-reader mirror of the pages, and the document's
 *  own name. A file called `notes.txt` is not a leaked key, and a guard that
 *  could not tell the two apart would be loosened the first time a fixture's
 *  name tripped it. Named, not pattern-matched, so the exemption cannot grow. */
export const DOCUMENT_SURFACES = ["#pages", "#a11yDocument", "#docTitle"];

/**
 * Every chrome string on the page that is shaped like a catalogue key.
 *
 * O(elements in the page) — a test sweep, run a handful of times per spec.
 *
 * @param {import("@playwright/test").Page} page
 * @returns {Promise<string[]>} one line per leak: where, which string, what it read
 */
export async function rawKeyLeaks(page) {
  return page.evaluate(
    ({ shape, skip }) => {
      const pattern = new RegExp(shape);
      const describe = (el) =>
        el.id ? `#${el.id}` : `${el.tagName.toLowerCase()}.${[...el.classList].join(".")}`;
      const leaks = [];
      for (const el of document.body.querySelectorAll("*")) {
        if (skip.some((selector) => el.closest(selector))) continue;
        for (const attribute of ["aria-label", "title", "placeholder"]) {
          const value = el.getAttribute(attribute)?.trim();
          if (value && pattern.test(value)) leaks.push(`${describe(el)} [${attribute}] "${value}"`);
        }
        // Visible text only: a hidden template's text is not read by anyone,
        // and its attributes were already swept above.
        if (el.getClientRects().length === 0) continue;
        for (const node of el.childNodes) {
          if (node.nodeType !== Node.TEXT_NODE) continue;
          const text = node.nodeValue.trim();
          if (text && pattern.test(text)) leaks.push(`${describe(el)} [text] "${text}"`);
        }
      }
      return leaks;
    },
    { shape: RAW_KEY_SHAPE.source, skip: DOCUMENT_SURFACES },
  );
}
