// Dialogs waste space in two ways, and both are structural rather than
// aesthetic, so both can be checked without a browser.
//
// 1. DEAD TOKENS. `--dlg-pad-x` and `--dlg-pad-y` were declared with responsive
//    `clamp()` values and referenced NOWHERE — every dialog hardcoded
//    `padding: 18px var(--space-5) 14px` instead. The design's responsive
//    rhythm existed in the stylesheet and never reached a pixel. That is the
//    same shape as `GlyphRun::is_marker`, which was declared, read in two
//    places, and never once set: a declaration nobody honours reads as a
//    feature and behaves as nothing.
//
// 2. CONSTANT WIDTHS. Every card took one of three hand-picked pixel widths,
//    so a six-row list and a twenty-field form were the same size and the
//    smaller one was mostly padding.
//
// The measured geometry — spinner widths, gap sizes, preview boxes — is guarded
// in `webapp/tests/e2e/dialog-density.spec.mjs`, which needs a real layout.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("../src/style.css", import.meta.url), "utf8");

/** The body of the first rule whose selector list matches `selector` exactly. */
function rule(selector) {
  const re = new RegExp(`(^|\\n)${selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\{([^}]*)\\}`);
  const m = css.match(re);
  assert.ok(m, `expected a \`${selector}\` rule`);
  return m[2];
}

test("the dialog padding tokens are actually used, not merely declared", () => {
  for (const token of ["--dlg-pad-x", "--dlg-pad-y"]) {
    const declarations = [...css.matchAll(new RegExp(`${token}\\s*:`, "g"))].length;
    const uses = [...css.matchAll(new RegExp(`var\\(${token}`, "g"))].length;
    assert.ok(declarations > 0, `${token} must be declared`);
    assert.ok(
      uses > 0,
      `${token} is declared and never used. A responsive token that no rule ` +
        `references is a design intention the product does not have.`,
    );
  }
});

test("dialog chrome takes its padding from the tokens, not from pixels", () => {
  for (const selector of [".dialog-head", ".dialog-body", ".dialog-foot"]) {
    const body = rule(selector);
    const padding = body.match(/(?:^|\n)\s*padding:\s*([^;]+);/);
    assert.ok(padding, `${selector} must set padding`);
    assert.match(
      padding[1],
      /--dlg-pad-[xy]/,
      `${selector} hardcodes its padding (${padding[1].trim()}). Every dialog ` +
        `did this, which is why the clamp() tokens never took effect.`,
    );
  }
});

test("a dialog that shrank its contents is allowed to shrink", () => {
  // Page setup's preview and spinners were cut to their content; if the card
  // stays pinned to the "wide" constant the dialog is still as big and no more
  // useful, which is the complaint in one sentence.
  for (const selector of [
    ".field-dialog",
    ".about-dialog",
    ".bookmark-dialog",
    "#splitCellDialog .dialog-card",
    "#confirmDialog .dialog-card",
  ]) {
    const body = rule(selector);
    assert.match(
      body,
      /width:\s*fit-content\s*;/,
      `${selector} must be sized by its content, not by a width constant`,
    );
    assert.match(
      body,
      /max-inline-size:.*(ch|100%)/,
      `${selector} needs a ceiling so one long label cannot stretch it back out`,
    );
  }
});

// `min()` takes <length-percentage> terms. `fit-content` is a sizing KEYWORD,
// so `width: min(fit-content, 100%)` does not parse and the whole declaration
// is thrown away — `CSS.supports("width", "min(fit-content, 100%)")` is false
// in Chromium. It reads as correct, it is the obvious way to write "size to the
// content but never overflow", and it shipped on all three cards the previous
// pass was written to make content-sized. Measured in the browser afterwards:
// every one of them still computed `.dialog-card`'s `min(640px, 100%)` and was
// being clamped by its `max-inline-size` ch ceiling instead — a constant, which
// is the defect that pass set out to remove. Insert field measured 426px,
// exactly its 52ch ceiling, against the 357px its rows add up to.
//
// The keyword alone is both correct and sufficient: `fit-content` is defined as
// `min(max-content, max(min-content, stretch))`, so it already cannot exceed
// the space available to it and needs no clamp.
// A RATCHET rather than a zero, for the same reason as the `main.js` one.
// Three rules still carry the broken spelling and none of them is this change's
// to correct:
//
//   `.watermark-dialog`, `.drop-cap-dialog` — a change running in parallel with
//     this one owns both, so fixing them here would collide rather than help.
//   `.page-setup-dialog` — correcting the spelling changes that card's rendered
//     width for the first time, and redesigning Page setup is not this change's
//     job. (It also already fails the clipping contract on the commit this
//     branched from, with and without the fix, for an unrelated reason.)
//
// The ratchet arms the rule for everything else immediately: no NEW card can be
// written this way, and when those three are corrected this number comes down
// with them. A ceiling nobody lowers stops being a ratchet and becomes a
// comment, so the test also fails if the count drops below it.
const MIN_FIT_CONTENT_RATCHET = 3;

test("no new card asks for `min(fit-content, …)`, which is not valid CSS", () => {
  // Comments first: the rules above explain this defect in prose, and a guard
  // that reads its own explanation as a violation would be unfixable.
  const code = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const offenders = [...code.matchAll(/^.*\bmin\(\s*fit-content\b.*$/gm)].map((m) => m[0].trim());
  assert.ok(
    offenders.length <= MIN_FIT_CONTENT_RATCHET,
    `\`min(fit-content, …)\` does not parse, so the declaration is dropped and ` +
      `the element keeps whatever width it inherited. Write \`width: fit-content\`, ` +
      `which cannot exceed the available space anyway. ${offenders.length} uses, ` +
      `ratchet ${MIN_FIT_CONTENT_RATCHET}:\n  ${offenders.join("\n  ")}`,
  );
  assert.equal(
    offenders.length,
    MIN_FIT_CONTENT_RATCHET,
    `the ratchet is above the real count (${offenders.length}). Lower ` +
      `MIN_FIT_CONTENT_RATCHET to it, or the guard stops guarding.`,
  );
});

test("the narrow-width override no longer stacks paired number fields", () => {
  // Forcing `.dialog-field-grid` to one column at 620px turned a pair of
  // five-character fields into two 302px boxes stacked — worse on the screen
  // with the least room. Word and Docs keep Top/Bottom paired on a phone.
  // Brace-match the media block rather than guessing where it ends: an
  // eyeballed slice silently stops covering the rule it was written for, and
  // a guard that stops looking is a guard that passes for free.
  // Anchored to the start of a line: the phrase also appears inside the
  // explanatory comment on `.dialog-field-grid`, and matching THAT made this
  // guard brace-match a comment and pass without ever reading the rule.
  const at = css.search(/\n@media \(max-width: 620px\)/);
  assert.ok(at > 0, "the 620px breakpoint must exist");
  const open = css.indexOf("{", at);
  let depth = 0;
  let close = open;
  for (let i = open; i < css.length; i++) {
    if (css[i] === "{") depth += 1;
    else if (css[i] === "}") {
      depth -= 1;
      if (depth === 0) {
        close = i;
        break;
      }
    }
  }
  assert.ok(close > open, "the 620px block must be balanced");
  const block = css.slice(open, close);
  assert.ok(
    !/\.dialog-field-grid\s*[,{]/.test(block),
    "`.dialog-field-grid` must not be collapsed to one column at narrow " +
      "widths; its auto-fit track already drops to one column when one no " +
      "longer fits, and forcing it stretches five-character inputs to 302px",
  );
});

test("number inputs are sized by their content", () => {
  const body = rule(".number-control");
  assert.match(
    body,
    /inline-size:\s*fit-content/,
    "a margin or column-count spinner holds at most five characters; " +
      "stretching it to `1fr` made it 182px at 1280 and 302px at 390",
  );
  assert.ok(
    !/grid-template-columns:\s*minmax\(0,\s*1fr\)/.test(body),
    "the value column must not be a stretching `1fr` track",
  );
});
