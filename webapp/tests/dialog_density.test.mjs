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
  for (const selector of [".page-setup-dialog", ".field-dialog", ".about-dialog"]) {
    const body = rule(selector);
    assert.match(
      body,
      /width:\s*min\(fit-content/,
      `${selector} must be sized by its content, not by a width constant`,
    );
    assert.match(
      body,
      /max-inline-size:.*(ch|100%)/,
      `${selector} needs a ceiling so one long label cannot stretch it back out`,
    );
  }
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
