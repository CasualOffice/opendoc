// Every parameter a catalogue string DECLARES is supplied by every call site.
//
// THE DEFECT THIS EXISTS FOR, in the product, on `main`. `pageSetup.dimensions`
// was `"{width} × {height} in"` and became `"{width} × {height} {unit}"` when the
// measurement preference shipped — necessarily, because all eighteen translated
// catalogues had rendered the word "in" as a word ("po", "Zoll", "インチ"), and
// that is a wrong unit presented as a right one the moment the unit can move.
// The key has TWO callers. Page setup's own preview label was updated. The size
// readout in `object_resize_drag.mjs` was not, and `interpolate` leaves an
// unsupplied placeholder standing rather than failing:
//
//   return text.replace(/\{(\w+)\}/g, (whole, name) =>
//     Object.hasOwn(params, name) ? String(params[name]) : whole,
//   );
//
// So every resize drag and every crop in the product painted the literal text
//
//   2.89 × 1.45 {unit}
//
// to the person doing the dragging, in every language, and it reached `main` and
// reddened `object-command-reach.spec.mjs` in CI there. No existing guard could
// see it: `locale_coverage` checks that catalogues ANSWER keys,
// `no_unrouted_strings` checks that strings GO THROUGH the seam, and neither asks
// whether a call site passes what the string it names asks for.
//
// This is the class fix (`SKILL` §10): not the one call site, every call site,
// and a guard that fails the build when the next key gains a parameter. It is a
// SOURCE SCAN for the same reason `no_unrouted_strings` is — the failure is a
// relationship between a catalogue entry and a line of code, which no amount of
// running the editor reliably reaches, because reaching it means performing every
// gesture in the product.
//
// MUTATION PROOF. Removing `unit: INCH_SUFFIX` from `sizeLabel` — the exact state
// `main` is in:
//
//   not ok 1 - every t() call supplies every parameter its English declares
//     error: |-
//       a catalogue string will paint its own placeholder to a reader
//       + actual - expected
//
//       + [
//       +   'src/object_resize_drag.mjs:57 t("pageSetup.dimensions") is missing {unit}'
//       + ]
//       - []
//
// WHAT IT DELIBERATELY DOES NOT DO. It does not complain about EXTRA parameters:
// an unused one is dead weight, not a defect a reader can see, and `t` already
// formats `count` for the plural machinery whether a string displays it or not.
// It skips a call whose parameters arrive in a variable rather than as a literal,
// and a call whose key is computed, because a scanner that guessed at either
// would be a scanner people silence. Those skips are COUNTED and asserted below,
// so the day the editor is written in a style this cannot read, the number moves
// and somebody is told — rather than the guard quietly checking nothing.
import assert from "node:assert/strict";
import test from "node:test";

import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const SRC = join(WEBAPP, "src");

/** The RUNTIME catalogue, which is the union of the markup's keys and the
 *  script's: `en.json` is generated from both, so it is the only table that can
 *  answer for a key wherever it was declared. */
const EN = JSON.parse(readFileSync(join(WEBAPP, "locales/en.json"), "utf8"));

const PLACEHOLDER = /\{(\w+)\}/g;

function sources(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) out.push(...sources(full));
    else if (/\.(mjs|js)$/.test(name)) out.push(full);
  }
  return out.sort();
}

/** The placeholders `key` declares, or null when no catalogue entry names it.
 *
 *  A PLURAL FAMILY is read through whichever category is present: the categories
 *  of one family all take the same parameters by construction, so any of them
 *  answers, and English has no `zero`/`few`/`many` to read. */
function declaredParams(key) {
  const direct = EN[key];
  if (typeof direct === "string") {
    return new Set([...direct.matchAll(PLACEHOLDER)].map((match) => match[1]));
  }
  const family = ["other", "one", "two", "few", "many", "zero"]
    .map((category) => EN[`${key}.${category}`])
    .find((value) => typeof value === "string");
  return typeof family === "string"
    ? new Set([...family.matchAll(PLACEHOLDER)].map((match) => match[1]))
    : null;
}

/** The body of the object literal that starts at `text[start]` (`"{"`), or null
 *  when the braces do not close. Skips string contents, so a `"}"` inside a
 *  message does not end the literal early. */
function objectLiteral(text, start) {
  let depth = 0;
  for (let i = start; i < text.length; i += 1) {
    const ch = text[i];
    if (ch === "{") depth += 1;
    else if (ch === "}") {
      depth -= 1;
      if (depth === 0) return text.slice(start + 1, i);
    } else if (ch === '"' || ch === "'" || ch === "`") {
      const quote = ch;
      i += 1;
      while (i < text.length && text[i] !== quote) i += text[i] === "\\" ? 2 : 1;
    }
  }
  return null;
}

/** The property names an object literal's body declares at DEPTH ONE — both
 *  `name: value` and the `{ count }` shorthand. Nested literals, calls and
 *  strings are skipped, so `{ mark: t(mark.labelKey) }` reports `mark` and not
 *  `labelKey`. */
function propertyNames(body) {
  const names = [];
  let depth = 0;
  let atName = true;
  let i = 0;
  while (i < body.length) {
    const ch = body[i];
    if (ch === "{" || ch === "[" || ch === "(") {
      depth += 1;
      atName = false;
      i += 1;
    } else if (ch === "}" || ch === "]" || ch === ")") {
      depth -= 1;
      i += 1;
    } else if (ch === '"' || ch === "'" || ch === "`") {
      const quote = ch;
      i += 1;
      while (i < body.length && body[i] !== quote) i += body[i] === "\\" ? 2 : 1;
      i += 1;
      atName = false;
    } else if (depth === 0 && ch === ",") {
      atName = true;
      i += 1;
    } else if (depth === 0 && atName && /[A-Za-z_$]/.test(ch)) {
      const identifier = /^[A-Za-z0-9_$]+/.exec(body.slice(i))[0];
      names.push(identifier);
      i += identifier.length;
      atName = false;
    } else {
      if (!/\s/.test(ch)) atName = false;
      i += 1;
    }
  }
  return names;
}

/** `t("key"` / `io.t("key"` / `host.t("key"`, with whether a second argument
 *  follows. `\b` before `t` matches after a `.` too, and does not match the `t`
 *  inside an identifier like `format(`. */
const CALL = /\bt\(\s*"([\w.]+)"\s*(,?)/g;

/** Walks every call site once. O(bytes of `src/`). */
function scan() {
  const missing = [];
  const skipped = { computedParams: 0, unknownKey: 0 };
  let checked = 0;
  for (const file of sources(SRC)) {
    const text = readFileSync(file, "utf8");
    const label = file.slice(SRC.length - 3);
    for (const match of text.matchAll(CALL)) {
      const key = match[1];
      const need = declaredParams(key);
      if (need === null) {
        // A key no catalogue declares is `locale_coverage`'s and
        // `no_unrouted_strings`' business, not this one's — and `t` returns the
        // key itself for one, which is visible in a screenshot by design.
        skipped.unknownKey += 1;
        continue;
      }
      checked += 1;
      if (need.size === 0) continue;
      if (match[2] !== ",") {
        missing.push(`${label}:${lineOf(text, match.index)} t("${key}") is passed no parameters`);
        continue;
      }
      const after = match.index + match[0].length;
      const brace = text.indexOf("{", after);
      const close = text.indexOf(")", after);
      if (brace === -1 || (close !== -1 && close < brace)) {
        skipped.computedParams += 1;
        continue;
      }
      const body = objectLiteral(text, brace);
      if (body === null) {
        skipped.computedParams += 1;
        continue;
      }
      const supplied = new Set(propertyNames(body));
      // A SPREAD gives up: `{ ...vars }` can supply anything, so the honest
      // answer is to say nothing rather than to guess.
      if (body.includes("...")) {
        skipped.computedParams += 1;
        continue;
      }
      const absent = [...need].filter((name) => !supplied.has(name));
      if (absent.length) {
        missing.push(
          `${label}:${lineOf(text, match.index)} t("${key}") is missing {${absent.join("}, {")}}`,
        );
      }
    }
  }
  return { missing, skipped, checked };
}

const lineOf = (text, index) => text.slice(0, index).split("\n").length;

test("every t() call supplies every parameter its English declares", () => {
  const { missing } = scan();
  assert.deepEqual(missing, [], "a catalogue string will paint its own placeholder to a reader");
});

test("the scan still reaches the call sites it claims to", () => {
  // A source scan that stops matching is a guard that reports success for a
  // reason nobody asked for, which is `SKILL` §4's whole subject. These are
  // floors on reach, not pins on a count: the number of call sites goes up as
  // the editor grows, and what must not happen is that it collapses.
  const { checked, skipped } = scan();
  assert.ok(checked > 250, `only ${checked} t() calls were checked`);
  assert.ok(
    skipped.computedParams <= 12,
    `${skipped.computedParams} call sites pass parameters this scan cannot read; ` +
      "if that is the house style now, this guard needs a parser rather than a raise",
  );
  assert.ok(
    skipped.unknownKey <= 8,
    `${skipped.unknownKey} t() calls name a key no catalogue declares: ${skipped.unknownKey}`,
  );
});

test("the parameter table is read from the catalogue, not from a list here", () => {
  // The one thing that would make the guard above vacuous is a `declaredParams`
  // that answered "nothing is required" for everything. These are the three
  // shapes it has to read: a plain string, a plural family, and a key with no
  // parameters at all.
  assert.deepEqual([...declaredParams("pageSetup.dimensions")].sort(), [
    "height",
    "unit",
    "width",
  ]);
  assert.deepEqual([...declaredParams("formattingMarks.markSwitch")].sort(), ["mark", "state"]);
  assert.equal(declaredParams("command.needsDocument").size, 0);
  assert.equal(declaredParams("no.such.key.anywhere"), null);
  // A family, through whichever category English carries.
  const family = Object.keys(EN).find((key) => key.endsWith(".other"));
  assert.ok(family, "no plural family in the catalogue to read");
  assert.notEqual(declaredParams(family.replace(/\.other$/, "")), null);
});

test("the literal reader survives the shapes this source actually contains", () => {
  assert.deepEqual(propertyNames("width: a, height: b"), ["width", "height"]);
  assert.deepEqual(propertyNames(" count "), ["count"], "the shorthand");
  assert.deepEqual(
    propertyNames('mark: t(mark.labelKey), state: t(on ? "a.b" : "c.d")'),
    ["mark", "state"],
    "a nested call must not contribute its own property names",
  );
  assert.deepEqual(
    propertyNames('name: "}", other: 1'),
    ["name", "other"],
    "a brace inside a string must not end the literal",
  );
  const sample = 't("k", { a: "}" })';
  assert.equal(objectLiteral(sample, sample.indexOf("{")), ' a: "}" ');
});
