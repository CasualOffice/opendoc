import assert from "node:assert/strict";
import test from "node:test";

import { embedMarker, escapeHtml, extractMarker, htmlToRuns, runsToHtml } from "../src/clipboard.mjs";

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

test("htmlToRuns maps background-color to highlight but skips transparent", () => {
  const runs = htmlToRuns(
    root(
      el("span", { style: "background-color:#ffff00" }, txt("hi")),
      el("span", { style: "background-color:transparent" }, txt("no")),
    ),
  );
  assert.deepEqual(runs, [
    { text: "hi", highlight: "#ffff00" },
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
