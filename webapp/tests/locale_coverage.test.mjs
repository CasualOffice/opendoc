// The catalogues (docs/124 §4).
//
// Two properties, and the second is the owner's "nothing should be left"
// applied to the languages rather than to the source:
//
//   1. `en.json` is what the extractor produces from the current source. It is
//      DERIVED, like `dict/glossary.txt`, so a hand edit or a stale checkout is
//      a build failure rather than a discovery six weeks later.
//   2. Every shipped locale answers every key English has — and for a plural
//      family, every category ITS OWN language needs, which is not the same set
//      English needs. A locale that is 90% translated is not shipped 90% lit;
//      it fails the build, because a UI that switches to German and then shows
//      English in the places nobody got to is worse than one that stayed
//      English on purpose.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const LOCALES = join(WEBAPP, "locales");

const { buildCatalogue, serialise } = await import("../tools/build-locale.mjs");
const { direction } = await import("../src/i18n.mjs");
const { EN_STRINGS } = await import("../src/en_strings.mjs");

const read = (tag) => JSON.parse(readFileSync(join(LOCALES, `${tag}.json`), "utf8"));
const tags = readdirSync(LOCALES)
  .filter((name) => name.endsWith(".json"))
  .map((name) => name.replace(/\.json$/, ""))
  .sort();
const english = read("en");

/** `status.words.one` -> family `status.words`, category `one`. Anything whose
 *  last segment is not a CLDR category is a plain key. */
const CATEGORIES = new Set(["zero", "one", "two", "few", "many", "other"]);
function split(key) {
  const cut = key.lastIndexOf(".");
  const tail = key.slice(cut + 1);
  return cut > 0 && CATEGORIES.has(tail)
    ? { family: key.slice(0, cut), category: tail }
    : { family: null, key };
}

function shape(catalogue) {
  const plain = new Set();
  const families = new Map();
  for (const key of Object.keys(catalogue)) {
    if (key.startsWith("@@")) continue;
    const parsed = split(key);
    if (parsed.family === null) plain.add(parsed.key);
    else families.set(parsed.family, (families.get(parsed.family) ?? new Set()).add(parsed.category));
  }
  return { plain, families };
}

test("en.json is exactly what the extractor produces from today's source", async () => {
  const built = serialise(await buildCatalogue());
  const onDisk = readFileSync(join(LOCALES, "en.json"), "utf8");
  assert.equal(
    onDisk,
    built,
    "locales/en.json is stale or hand-edited. Run `node webapp/tools/build-locale.mjs`.",
  );
});

test("the owner asked for at least fifteen languages, and they are here", () => {
  const translated = tags.filter((tag) => tag !== "en");
  assert.ok(
    translated.length >= 15,
    `${translated.length} translated locales: ${translated.join(", ")}`,
  );
});

/** Keys a locale MUST answer, whatever else it lags on: the ones with no
 *  English sitting in the markup to fall back to. A script-side string that a
 *  catalogue cannot answer renders as a dotted key, which is the one outcome
 *  worth failing a build over. */
const SCRIPT_KEYS = new Set(Object.keys(EN_STRINGS));

/** Translation coverage per locale, measured on 2026-09-25 after the third
 *  translation pass. Only one direction is legal — up.
 *
 *  The first design failed a locale that answered fewer keys than English. It
 *  was the right instinct and the wrong mechanism: routing a surface through
 *  the seam and translating it are two different days' work, and a gate that
 *  refuses the first until the second is done means the routing never starts —
 *  or means 15,000 machine translations land unreviewed in one commit. The
 *  markup carries its English beside every key, so an untranslated string
 *  reads as English rather than as an identifier, and what has to be true is
 *  that coverage never FALLS. Same ratchet as the unrouted-string count it
 *  faces across the seam: one number goes down, the other goes up. */
const COVERAGE = new Map([
  ["ar", 902],
  ["de", 902],
  ["es", 902],
  ["fr", 902],
  ["hi", 902],
  ["id", 902],
  ["it", 902],
  ["ja", 902],
  ["ko", 902],
  ["nl", 902],
  ["pl", 902],
  ["pt-BR", 902],
  ["ru", 902],
  ["tr", 902],
  ["uk", 902],
  ["vi", 902],
  ["zh-Hans", 902],
  ["zh-Hant", 902],
]);

/** English defines 902 keys today, and every locale answers all 902 — 100.0%,
 *  up from the 301 (35.4%) the previous pass had reached. That is the whole
 *  routed surface: every ribbon tab's groups and controls, the menu bar, every
 *  dialog title and button, the field labels, the File page, and the status and
 *  error sentences, not only the chrome a person reads first.
 *
 *  876 → 902 is the watermark dialog (`109` OO-006), translated in the same
 *  change that routed it: every locale answers its twenty-six keys, six of which
 *  are copied from the string the same catalogue already uses for the Page setup
 *  dialog's Apply, Cancel, Close, Font, Size and "applies to this section" rather
 *  than translated a second time — two words for one button is how a dialog
 *  starts reading as a different product from the one beside it.
 *
 *  The number in each row is still a floor, not a target, and at parity its
 *  job changes rather than ending: it is now what refuses a NEWLY routed
 *  surface that ships untranslated. Whoever adds keys to `en.json` raises this
 *  number in the same PR or the gate says so. */

test("every locale answers every SCRIPT-side key, where English is not in the markup", () => {
  const gaps = [];
  for (const tag of tags) {
    if (tag === "en") continue;
    const catalogue = read(tag);
    const local = shape(catalogue);
    for (const key of SCRIPT_KEYS) {
      const parsed = split(key);
      const answered = parsed.family
        ? local.families.has(parsed.family)
        : Object.hasOwn(catalogue, key);
      if (!answered) gaps.push(`${tag}: missing ${key}`);
    }
  }
  assert.deepEqual(gaps, []);
});

test("no locale carries a key English no longer has", () => {
  const source = shape(english);
  const orphans = [];
  for (const tag of tags) {
    if (tag === "en") continue;
    const local = shape(read(tag));
    for (const key of local.plain) {
      if (!source.plain.has(key)) orphans.push(`${tag}: ${key}`);
    }
    for (const family of local.families.keys()) {
      if (!source.families.has(family)) orphans.push(`${tag}: ${family}.*`);
    }
  }
  assert.deepEqual(orphans, []);
});

test("translation coverage never falls", () => {
  const source = shape(english);
  const total = source.plain.size + source.families.size;
  const fell = [];
  for (const [tag, floor] of COVERAGE) {
    const local = shape(read(tag));
    const answered =
      [...source.plain].filter((key) => local.plain.has(key)).length +
      [...source.families.keys()].filter((family) => local.families.has(family)).length;
    if (answered < floor) {
      fell.push(`${tag}: answers ${answered} of ${total}, was ${floor}`);
    }
  }
  assert.deepEqual(fell, [], `of ${total} keys English defines`);
});

test("every locale carries every plural category ITS OWN language needs", () => {
  const source = shape(english);
  const gaps = [];
  for (const tag of tags) {
    const local = shape(read(tag));
    const needed = new Intl.PluralRules(tag).resolvedOptions().pluralCategories;
    for (const family of source.families.keys()) {
      const have = local.families.get(family);
      // A family this locale has not reached yet is the coverage ratchet's
      // business, not this test's. What this one holds is that a family a
      // locale DOES carry is complete for that language.
      if (!have) continue;
      for (const category of needed) {
        // Russian needs one/few/many/other and Arabic six. English declares
        // two. A locale that only copied English's two is broken for most of
        // its own numbers, and this is the only place that can notice.
        if (!have.has(category)) gaps.push(`${tag}: ${family} has no "${category}" form`);
      }
      for (const category of have) {
        if (!needed.includes(category)) {
          gaps.push(`${tag}: ${family}.${category} is not a category ${tag} uses`);
        }
      }
    }
  }
  assert.deepEqual(gaps, []);
});

test("no catalogue value carries an HTML entity", () => {
  // The applier writes TEXT, not HTML — it has to, or a translation could
  // inject markup — so an entity that survives extraction reaches the screen
  // literally. The review sidebar read "Comments &amp; suggestions" the first
  // time the markup went through the seam.
  const leaked = [];
  for (const tag of tags) {
    for (const [key, value] of Object.entries(read(tag))) {
      if (typeof value !== "string") continue;
      if (/&(#\d+|#x[0-9a-f]+|[a-z]+);/i.test(value)) leaked.push(`${tag} ${key}: ${value}`);
    }
  }
  assert.deepEqual(leaked, []);
});

test("every catalogue declares its own locale, direction and review state", () => {
  const wrong = [];
  for (const tag of tags) {
    const catalogue = read(tag);
    if (catalogue["@@locale"] !== tag) wrong.push(`${tag}: @@locale is ${catalogue["@@locale"]}`);
    if (catalogue["@@direction"] !== direction(tag)) {
      wrong.push(`${tag}: @@direction is ${catalogue["@@direction"]}, should be ${direction(tag)}`);
    }
    if (typeof catalogue["@@reviewed"] !== "boolean") wrong.push(`${tag}: no @@reviewed flag`);
  }
  assert.deepEqual(wrong, []);
});

test("Arabic is right to left and nothing else shipped is", () => {
  const rtl = tags.filter((tag) => read(tag)["@@direction"] === "rtl");
  assert.deepEqual(rtl, ["ar"]);
});

test("the unreviewed locales are unreviewed, and say so rather than implying otherwise", () => {
  // docs/124 §6: the first delivery's translations are machine-produced. That
  // is recorded in the artifact, not only in a commit message, so the day a
  // native speaker reviews one the flag is what changes.
  assert.equal(english["@@reviewed"], true, "English is the source, so it is reviewed by construction");
  const claimed = tags.filter((tag) => tag !== "en" && read(tag)["@@reviewed"] === true);
  assert.deepEqual(
    claimed,
    [],
    `${claimed.join(", ")} claim native review. Flip a flag only when a native speaker has actually read it.`,
  );
});

test("every string interpolates the placeholders its English does, and no others", () => {
  const placeholders = (text) => new Set([...String(text).matchAll(/\{(\w+)\}/g)].map((m) => m[1]));
  const wrong = [];
  for (const tag of tags) {
    if (tag === "en") continue;
    const catalogue = read(tag);
    for (const [key, value] of Object.entries(catalogue)) {
      if (key.startsWith("@@")) continue;
      const parsed = split(key);
      // A plural form's English is whichever category English has; compare
      // against the family's `other`, which every language declares.
      const source = parsed.family
        ? english[`${parsed.family}.other`] ?? english[key]
        : english[key];
      if (source === undefined) continue;
      const want = placeholders(source);
      const got = placeholders(value);
      for (const name of want) {
        if (got.has(name)) continue;
        // `{count}` may be absent from the zero, one and two forms, and only
        // those: in Arabic "كلمة واحدة" and "كلمتان" ARE "one word" and "two
        // words" — the numeral is carried by the grammar, and printing "1" in
        // front of them is what would read wrong. Every other placeholder, and
        // `{count}` in every other category, must survive translation.
        const grammatical =
          name === "count" && ["zero", "one", "two"].includes(parsed.category);
        if (!grammatical) wrong.push(`${tag} ${key}: dropped {${name}}`);
      }
      for (const name of got) {
        if (!want.has(name)) wrong.push(`${tag} ${key}: invented {${name}}`);
      }
    }
  }
  assert.deepEqual(wrong, []);
});

