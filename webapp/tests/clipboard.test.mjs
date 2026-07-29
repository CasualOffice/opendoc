import assert from "node:assert/strict";
import test from "node:test";

import { embedMarker, escapeHtml, extractMarker, runsToHtml } from "../src/clipboard.mjs";

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
