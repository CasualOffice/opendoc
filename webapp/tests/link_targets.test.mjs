// The link-target allowlist, tested directly.
//
// It is the one predicate standing between an imported document and a
// capability: a `.docx` can contain `javascript:` in a hyperlink, and every
// follow path — the chip's Open, a click on linked text, a click on a linked
// PICTURE, the context menu — resolves through here. It used to live inside
// `activateLink`, and the context menu opened `javascript:` unchecked, which is
// the same capability guarded on one surface only (`109` HF-085).
import assert from "node:assert/strict";
import test from "node:test";

// The module reads `window.location.href` to resolve a relative target, which
// is the whole reason a relative URL can be judged at all.
globalThis.window = { location: { href: "https://editor.example/app/editor.html" } };
const { EXTERNAL_LINK_SCHEMES, resolveExternalTarget } = await import(
  "../src/link_targets.mjs"
);

test("the allowlist is exactly the three schemes a document may reach", () => {
  assert.deepEqual([...EXTERNAL_LINK_SCHEMES].sort(), ["http:", "https:", "mailto:"]);
});

test("an allowlisted target resolves", () => {
  for (const url of [
    "https://example.org/a",
    "http://example.org/a",
    "mailto:someone@example.org",
    "/relative/page.html",
    "  https://example.org/padded  ",
  ]) {
    const target = resolveExternalTarget(url);
    assert.ok(target, `${url} must resolve`);
    assert.ok(EXTERNAL_LINK_SCHEMES.has(target.protocol), `${url} -> ${target.protocol}`);
  }
});

test("every other scheme is refused, not sanitised", () => {
  // Refused rather than rewritten: a target we do not understand is not a
  // target we should be guessing the safe form of.
  for (const url of [
    "javascript:alert(1)",
    "JavaScript:alert(1)",
    "  javascript:alert(1)",
    "data:text/html,<script>alert(1)</script>",
    "file:///etc/passwd",
    "vbscript:msgbox(1)",
    "myapp://do-something",
    "blob:https://example.org/abc",
  ]) {
    assert.equal(resolveExternalTarget(url), null, `${url} must be refused`);
  }
});

test("a target that is not a usable string is refused", () => {
  for (const url of ["", "   ", null, undefined, 42, {}, []]) {
    assert.equal(resolveExternalTarget(url), null, `${JSON.stringify(url)} must be refused`);
  }
});
