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
// The measured geometry — what a card actually renders at, and whether it hides
// content below its own fold — needs a real layout, so it is guarded in
// `webapp/tests/e2e/dialog-fit.spec.mjs`. (This line named
// `e2e/dialog-density.spec.mjs` for several commits. That file has never
// existed: the citation was a claim that geometry was covered when nothing
// covered it.)
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("../src/style.css", import.meta.url), "utf8");

/** EVERY rule body whose selector list matches `selector`, in source order.
 *
 *  This helper used to return the FIRST one, anchored on `(^|\n)` plus the bare
 *  selector — so it matched only an unindented, top-level rule and could not see
 *  an override inside a media block, which is indented. That is not a missed
 *  edge case, it is the guard being structurally unable to read the thing it
 *  exists to check: `.dialog-head`, `.dialog-body` and `.dialog-foot` all went
 *  back to `padding: 16px` inside `@media (max-width: 620px)`, and the token
 *  test below passed the whole time (HF-265 D7).
 *
 *  A guard that can only read one of several declarations reports on whichever
 *  one happens to come first, which is worse than not checking: it is cited as
 *  evidence. So every match is returned and every caller asserts over all of
 *  them. */
function rules(selector) {
  const quoted = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`(?:^|[\\n{])[ \\t]*${quoted}\\s*\\{([^}]*)\\}`, "g");
  const found = [...css.matchAll(re)].map((m) => m[1]);
  assert.ok(found.length > 0, `expected at least one \`${selector}\` rule`);
  return found;
}

/** The first matching rule body, for the callers that are asking about one. */
function rule(selector) {
  return rules(selector)[0];
}

// The subject is the FAMILY, derived, not a list of names. The previous version
// of this test named `--dlg-pad-x` and `--dlg-pad-y` as two literals — so
// `--dlg-gap`, declared beside them and referenced nowhere, and
// `--dialog-width-wide`, the widest rung of a four-rung scale, both sat dead for
// as long as they liked while the guard reported the family clean (HF-265 D8).
// A guard whose subject is a list is a guard that only covers what somebody
// remembered to add to it.
const SCALE_FAMILY = /^--(?:dlg-|dialog-width-|pnl-|file-page-)/;

test("every token in the dialog and panel scale is actually used, not merely declared", () => {
  const declared = [...new Set([...css.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]))];
  const family = declared.filter((token) => SCALE_FAMILY.test(token));
  assert.ok(
    family.length >= 7,
    `expected the dialog/panel scale to have its rungs; found ${family.length}: ${family}`,
  );
  const dead = family.filter((token) => !new RegExp(`var\\(${token}[,)]`).test(css));
  assert.deepEqual(
    dead,
    [],
    `these tokens are declared and never read. A responsive token that no rule ` +
      `references is a design intention the product does not have — and a width ` +
      `scale with a dead rung is how a dialog comes to state a fourth width in raw ` +
      `pixels (\`.settings-dialog\`, 960px) beside an unused \`--dialog-width-wide\`. ` +
      `Either read the token where it belongs, or delete it.`,
  );
});

test("dialog and panel chrome take their padding from the tokens, not from pixels", () => {
  // `.panel-head`/`.panel-body` are here because the nine side panels had no
  // shared shell at all: `0 8px 0 14px` on the head over `padding: 6px` on the
  // body, and five panels overriding even that (HF-265 D2).
  const SUBJECTS = [
    [".dialog-head", /--dlg-pad-[xy]/],
    [".dialog-body", /--dlg-pad-[xy]/],
    [".dialog-foot", /--dlg-pad-[xy]/],
    [".panel-head", /--pnl-pad-[xy]/],
    [".panel-body", /--pnl-pad-[xy]/],
    [".find-panel-head", /--pnl-pad-[xy]/],
    [".find-panel-body", /--pnl-pad-[xy]/],
  ];
  for (const [selector, token] of SUBJECTS) {
    // EVERY rule for the selector, not the first. The three dialog bands each
    // had a second rule inside `@media (max-width: 620px)` restating
    // `padding: 16px`, and the old helper could not see it.
    const bodies = rules(selector);
    let sawPadding = false;
    for (const body of bodies) {
      const padding = body.match(/(?:^|\n)\s*padding:\s*([^;]+);/);
      if (!padding) continue;
      sawPadding = true;
      assert.match(
        padding[1],
        token,
        `${selector} hardcodes its padding (${padding[1].trim()}) in one of its ` +
          `${bodies.length} rules. Every dialog did this, which is why the clamp() ` +
          `tokens never took effect; a narrow-width rung belongs in the token, not ` +
          `in a second copy of the rule.`,
      );
    }
    assert.ok(sawPadding, `${selector} must set padding`);
  }
});

test("the panel shell's inset is symmetric, as the dialog heads' are", () => {
  // The head said `0 8px 0 14px`: the close button's side was 8px where the
  // title's was 14px, which is the same head/body disagreement read from the
  // other end. Two values at most — `y x` — so neither end can drift again.
  for (const selector of [".panel-head", ".panel-body"]) {
    for (const body of rules(selector)) {
      const padding = body.match(/(?:^|\n)\s*padding:\s*([^;]+);/);
      if (!padding) continue;
      const terms = padding[1].trim().split(/\s+(?![^(]*\))/);
      assert.ok(
        terms.length <= 2,
        `${selector} states ${terms.length} padding terms (${padding[1].trim()}). ` +
          `The shell's inset is one pair; a third or fourth term is a side being ` +
          `tuned on its own, which is the defect.`,
      );
    }
  }
});

test("no panel re-states the shell's padding for itself", () => {
  // The five that did: `.glyph-body` 10px, `.version-list` 4px 6px, and
  // `.table-properties-body`/`.properties-panel-body` 13px 14px 16px. Each was
  // a panel deciding its own inset, which is what made nine panels read as nine
  // unrelated surfaces. Scoped to the classes that sit ON a `.panel-body` or a
  // `.panel-head`, so an ordinary rule inside a panel is unaffected.
  const PANEL_PARTS = [
    "outline-list",
    "pages-grid",
    "compare-body",
    "version-list",
    "glyph-body",
    "properties-panel-body",
    "table-properties-body",
    "properties-panel-head",
    "table-properties-head",
    // The BANDS between head and body, which are part of the same shell: a
    // filter row, two footers. All three were 12px under a 16px head.
    "version-filter",
    "version-foot",
    "glyph-panel-foot",
  ];
  const offenders = [];
  for (const part of PANEL_PARTS) {
    const re = new RegExp(`(?:^|[\\n{])[ \\t]*[^\\n{}]*\\.${part}\\b[^\\n{}]*\\{([^}]*)\\}`, "g");
    for (const match of css.matchAll(re)) {
      const padding = match[1].match(/(?:^|\n)\s*padding(?:-inline|-left|-right)?:\s*([^;]+);/);
      if (padding && !/--pnl-pad-[xy]/.test(padding[1])) {
        offenders.push(`.${part} { padding: ${padding[1].trim()} }`);
      }
    }
  }
  assert.deepEqual(
    offenders,
    [],
    `a panel part must not state its own inset — that is what made the head and the ` +
      `body of every side panel disagree, and then made four of the nine disagree ` +
      `differently from the other five:\n  ${offenders.join("\n  ")}`,
  );
});

test("the review column reads the panel pair rather than a literal that matches it", () => {
  // It is deliberately NOT in the `.panel-head`/`.panel-body` shell — it is
  // absolutely positioned and rides the document scroll so each card stays
  // pinned to its anchor — but it takes the shell's INSET, and its header and
  // its cards both said `8px 10px`.
  //
  // This is a SOURCE assertion and has to be: `getComputedStyle` cannot tell
  // `8px 16px` from `var(--pnl-pad-y) var(--pnl-pad-x)`, so the browser-side
  // guard in `e2e/panel-alignment.spec.mjs` measures what a browser can answer
  // (the geometry, and header-against-card agreement) and this one reads the
  // stylesheet. Written down because the first version of that e2e test claimed
  // to catch a literal and was measured not to.
  for (const selector of [".review-sidebar-header", ".review-margin-card"]) {
    for (const body of rules(selector)) {
      const padding = body.match(/(?:^|\n)\s*padding:\s*([^;]+);/);
      if (!padding) continue;
      assert.match(
        padding[1],
        /--pnl-pad-[xy]/,
        `${selector} states its inset as ${padding[1].trim()}. A literal that happens ` +
          `to equal the token today is not the token: it does not follow it, and no ` +
          `computed-style guard can see the difference.`,
      );
    }
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
    // Desktop-qualified so the phone bottom-sheet rung, which is (0,2,0), is
    // not outranked by an ID selector (HF-265 D5).
    "body:not(.phone-mode) #splitCellDialog .dialog-card",
    "body:not(.phone-mode) #confirmDialog .dialog-card",
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
// A RATCHET rather than a zero, because one rule still carries the broken
// spelling:
//
//   `.page-setup-dialog` — correcting the spelling changes that card's rendered
//     width for the first time, and Page setup wants a layout pass of its own
//     rather than a width change made in passing.
//
// `.watermark-dialog` and `.drop-cap-dialog` were the other two. They were left
// at 3 while a parallel change owned them; both are corrected in this same
// commit, so the number comes down with them — which is what a ratchet is for.
//
// The ratchet arms the rule for everything else immediately: no NEW card can be
// written this way. A ceiling nobody lowers stops being a ratchet and becomes a
// comment, so the test also fails if the count drops below it.
const MIN_FIT_CONTENT_RATCHET = 1;

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
