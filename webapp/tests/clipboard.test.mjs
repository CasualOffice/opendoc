import assert from "node:assert/strict";
import test from "node:test";

import {
  embedMarker,
  escapeHtml,
  extractMarker,
  htmlToRuns,
  htmlToStructured,
  runsToHtml,
} from "../src/clipboard.mjs";

// `htmlToRuns` walks a browser DOM. The unit runner has no DOM, so build a
// minimal tree faithful to the node interface it reads: `childNodes`,
// `nodeType` (against the `Node` constants), `tagName` (uppercase, as HTML
// DOM reports), `textContent`, and `getAttribute` (returns `null` when
// absent). This mirrors what `DOMParser` produces for the same fragment.
globalThis.Node ??= { ELEMENT_NODE: 1, TEXT_NODE: 3 };
function txt(text) {
  return { nodeType: Node.TEXT_NODE, textContent: text, childNodes: [] };
}
function el(tag, attrs, ...children) {
  const map = attrs ?? {};
  return {
    nodeType: Node.ELEMENT_NODE,
    tagName: tag.toUpperCase(),
    childNodes: children,
    getAttribute: (name) => (name in map ? map[name] : null),
  };
}
function root(...children) {
  return { childNodes: children };
}

test("escapeHtml escapes the five HTML-significant characters", () => {
  assert.equal(escapeHtml(`<b>"Bold" & "Italic"</b>`), "&lt;b&gt;&quot;Bold&quot; &amp; &quot;Italic&quot;&lt;/b&gt;");
});

test("runsToHtml wraps paragraphs, formatting, and links", () => {
  const html = runsToHtml([
    { text: "Bold ", bold: true },
    { text: "Link", href: "https://example.com" },
    { paragraphBreak: true },
    { text: "plain" },
  ]);
  assert.equal(
    html,
    '<p><b>Bold </b><a href="https://example.com">Link</a></p><p>plain</p>',
  );
});

test("runsToHtml escapes text and renders an empty paragraph as a line break", () => {
  const html = runsToHtml([{ text: "<script>" }, { paragraphBreak: true }]);
  assert.equal(html, "<p>&lt;script&gt;</p><p><br></p>");
});

test("runsToHtml carries size/color/highlight/vertAlign", () => {
  const html = runsToHtml([
    { text: "x", sizeHalfPoints: 40, color: "#ff0000", highlight: "yellow", vertAlign: "super" },
  ]);
  assert.equal(
    html,
    '<p><span style="color:#ff0000;font-size:20pt;background-color:yellow"><sup>x</sup></span></p>',
  );
});

test("typed underline style/color survive visible HTML and external CSS parsing", () => {
  const html = runsToHtml([
    {
      text: "typed",
      underline: true,
      underlineStyle: "wavy",
      underlineColor: "#cc1122",
    },
  ]);
  assert.equal(
    html,
    '<p><span style="text-decoration-line:underline;text-decoration-style:wavy;text-decoration-color:#cc1122">typed</span></p>',
  );

  const runs = htmlToRuns(
    root(
      el(
        "span",
        {
          style:
            "text-decoration-line:underline;text-decoration-style:double;text-decoration-color:rgb(204, 17, 34)",
        },
        txt("typed"),
      ),
    ),
  );
  assert.deepEqual(runs, [
    {
      text: "typed",
      underline: true,
      underlineStyle: "double",
      underlineColor: "#cc1122",
    },
  ]);
});

test("embedMarker/extractMarker round-trip arbitrary JSON, including non-ASCII text", () => {
  const runsJson = JSON.stringify([{ text: "café ☕" }, { paragraphBreak: true }]);
  const html = `${embedMarker(runsJson)}<p>café ☕</p>`;
  assert.equal(extractMarker(html), runsJson);
});

test("extractMarker returns null for HTML with no marker", () => {
  assert.equal(extractMarker("<p>from Word</p>"), null);
});

test("extractMarker returns null for a malformed marker", () => {
  assert.equal(extractMarker("<!--opendoc-clipboard-runs:not-base64!!!-->"), null);
});

test("htmlToRuns reads Google-Docs-style inline CSS spans (no tags)", () => {
  // Google Docs wraps every run in a styled <span> with NO <b>/<i>/<u> tags.
  const runs = htmlToRuns(
    root(
      el(
        "p",
        null,
        el("span", { style: "font-weight:700" }, txt("bold")),
        el("span", { style: "font-style:italic" }, txt("it")),
        el("span", { style: "text-decoration:underline" }, txt("und")),
        el("span", { style: "text-decoration:line-through" }, txt("strk")),
        el("span", { style: "font-size:14pt;color:#ff0000" }, txt("big")),
        el("span", { style: "vertical-align:super" }, txt("sup")),
        el("span", { style: "vertical-align:sub" }, txt("sub")),
      ),
    ),
  );
  assert.deepEqual(runs, [
    { text: "bold", bold: true },
    { text: "it", italic: true },
    { text: "und", underline: true },
    { text: "strk", strike: true },
    { text: "big", sizeHalfPoints: 28, color: "#ff0000" },
    { text: "sup", vertAlign: "super" },
    { text: "sub", vertAlign: "sub" },
  ]);
});

test("htmlToRuns accumulates nested inline styles down the tree", () => {
  // Docs nests styled spans; a deeper span's size overrides an ancestor's,
  // while inherited bold/italic still apply.
  const runs = htmlToRuns(
    root(
      el(
        "span",
        { style: "font-weight:bold" },
        txt("a"),
        el("span", { style: "font-style:italic;font-size:20px" }, txt("b")),
      ),
    ),
  );
  assert.deepEqual(runs, [
    { text: "a", bold: true },
    // 20px -> 15pt -> 30 half-points; bold inherited, italic added.
    { text: "b", bold: true, italic: true, sizeHalfPoints: 30 },
  ]);
});

test("htmlToRuns merges tag and inline-style signals (either wins)", () => {
  // Word sometimes uses <b>; a style-only <span> must also register.
  const runs = htmlToRuns(
    root(
      el("b", { style: "font-style:italic" }, txt("x")),
      el("span", { style: "font-weight:600" }, txt("y")),
    ),
  );
  assert.deepEqual(runs, [
    { text: "x", bold: true, italic: true },
    { text: "y", bold: true },
  ]);
});

test("htmlToRuns snaps a bright background-color to a named highlight, and drops shading/transparent", () => {
  const runs = htmlToRuns(
    root(
      el("span", { style: "background-color:#ffff00" }, txt("hi")), // bright yellow -> "yellow"
      el("span", { style: "background-color:#00ff05" }, txt("gr")), // near-green -> "green"
      el("span", { style: "background-color:#f3f3f3" }, txt("sh")), // subtle gray shading -> dropped
      el("span", { style: "background-color:transparent" }, txt("no")),
    ),
  );
  // Highlight is a NAMED enum, not hex — so a real highlighter color maps to its
  // name, while ordinary low-saturation shading never becomes a phantom highlight.
  assert.deepEqual(runs, [
    { text: "hi", highlight: "yellow" },
    { text: "gr", highlight: "green" },
    { text: "sh" },
    { text: "no" },
  ]);
});

test("htmlToRuns still honors legacy formatting tags with no inline style", () => {
  const runs = htmlToRuns(
    root(el("b", null, txt("B")), el("sup", null, txt("2")), el("a", { href: "https://x.test" }, txt("L"))),
  );
  assert.deepEqual(runs, [
    { text: "B", bold: true },
    { text: "2", vertAlign: "super" },
    { text: "L", href: "https://x.test" },
  ]);
});

// The three ways the HTML importer corrupted the most common paste sources.
// All three shared one shape: it could only ever add — formatting on, a break on
// entering a container, a block from an element — and never account for content
// that was turned off, absent, or not an element at all.

test("an explicit font-weight:normal turns bold OFF, as CSS says it does", () => {
  // Google Docs wraps its ENTIRE clipboard payload in `<b style="font-weight:normal">`.
  // Reading only "does this turn bold on" saw the <b>, never the normal, and
  // delivered every paste from Docs in bold.
  const runs = htmlToRuns(
    root(
      el("b", { style: "font-weight:normal" }, txt("Not actually bold")),
    ),
  );
  assert.deepEqual(runs, [{ text: "Not actually bold", bold: false }]);

  // A real bold is still bold, from the tag or from the style.
  assert.equal(htmlToRuns(root(el("b", null, txt("x"))))[0].bold, true);
  assert.equal(
    htmlToRuns(root(el("span", { style: "font-weight:700" }, txt("x"))))[0].bold,
    true,
  );
  // And italic behaves the same way, for the same CSS reason.
  assert.equal(
    htmlToRuns(root(el("i", { style: "font-style:normal" }, txt("x"))))[0].italic,
    false,
  );
});

test("a wrapper div does not produce a leading blank paragraph", () => {
  // Word's outer WordSection1 div, and the wrapper div a browser selection
  // carries, are containers — not content. Treating entering one as content put
  // a paragraph break before the first word of every such paste.
  const runs = htmlToRuns(
    root(el("div", null, el("p", null, txt("First")), el("p", null, txt("Second")))),
  );
  assert.deepEqual(runs, [
    { text: "First" },
    { paragraphBreak: true },
    { text: "Second" },
  ]);
  assert.notEqual(runs[0].paragraphBreak, true, "nothing precedes the first word");
});

test("text decoration still propagates from an ancestor", () => {
  // `text-decoration` is NOT authoritative in both directions: it propagates from
  // an ancestor and a descendant cannot switch it off. Honouring `none` here
  // would be wrong CSS, so this pins the asymmetry deliberately.
  const runs = htmlToRuns(
    root(el("u", null, el("span", { style: "text-decoration:none" }, txt("still underlined")))),
  );
  assert.equal(runs[0].underline, true);
});

test("the sentences either side of a pasted table survive", () => {
  // "Intro sentence <table>…</table> Closing sentence" is the ordinary shape of a
  // copy from a web page or Word. The top-level text branch called `htmlToRuns`
  // on the TEXT NODE itself, and that walks `childNodes` — a text node has none —
  // so it returned nothing and both sentences were filtered away as empty
  // blocks. The table arrived alone, with no warning, against this module's own
  // no-silent-loss contract.
  const table = el(
    "table",
    null,
    el("tr", null, el("td", null, txt("cell"))),
  );
  // Only enough of the element interface for the table branch to run; the
  // table's own contents are not what this test is about.
  table.querySelector = () => null;
  table.querySelectorAll = () => [];
  const { blocks } = htmlToStructured(
    root(txt("Intro sentence"), table, txt("Closing sentence")),
  );

  const text = JSON.stringify(blocks);
  assert.ok(text.includes("Intro sentence"), `intro sentence dropped: ${text}`);
  assert.ok(text.includes("Closing sentence"), `closing sentence dropped: ${text}`);
});
