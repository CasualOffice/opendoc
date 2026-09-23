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

test("every locale answers every plain key English has", () => {
  const source = shape(english);
  const gaps = [];
  for (const tag of tags) {
    if (tag === "en") continue;
    const local = shape(read(tag));
    for (const key of source.plain) {
      if (!local.plain.has(key)) gaps.push(`${tag}: missing ${key}`);
    }
    for (const key of local.plain) {
      if (!source.plain.has(key)) gaps.push(`${tag}: orphan ${key} — English no longer has it`);
    }
  }
  assert.deepEqual(gaps, []);
});

test("every locale carries every plural category ITS OWN language needs", () => {
  const source = shape(english);
  const gaps = [];
  for (const tag of tags) {
    const local = shape(read(tag));
    const needed = new Intl.PluralRules(tag).resolvedOptions().pluralCategories;
    for (const family of source.families.keys()) {
      const have = local.families.get(family);
      if (!have) {
        gaps.push(`${tag}: no plural family ${family}`);
        continue;
      }
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
