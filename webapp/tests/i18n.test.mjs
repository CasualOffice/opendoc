// The localisation seam, tested where it is hardest to get right (docs/124).
//
// The interesting cases are not "does it look up a string". They are the four
// things a hand-rolled approach gets wrong, and that `109` HF-081 exists
// because the editor gets wrong today: plural categories beyond one/other, the
// fallback chain, what a MISSING string does, and the fact that a document's
// direction is not the interface's.
import assert from "node:assert/strict";
import test from "node:test";

const {
  FALLBACK_LOCALE,
  activeLocale,
  d,
  direction,
  fallbackChain,
  has,
  list,
  n,
  resetI18n,
  setCatalogue,
  setLocale,
  t,
} = await import("../src/i18n.mjs");

const EN = {
  "app.greeting": "Hello {who}",
  "count.words.one": "{count} word",
  "count.words.other": "{count} words",
  "file.save": "Save",
};

test.beforeEach(() => {
  resetI18n();
  setCatalogue("en", EN);
});

test("the seam starts in English and says so", () => {
  assert.equal(activeLocale(), FALLBACK_LOCALE);
  assert.equal(t("file.save"), "Save");
});

test("placeholders interpolate, and a missing one stays VISIBLE", () => {
  assert.equal(t("app.greeting", { who: "world" }), "Hello world");
  // Not "Hello undefined": a visible `{who}` is a bug report, `undefined` is a
  // bug that ships.
  assert.equal(t("app.greeting"), "Hello {who}");
  assert.equal(t("app.greeting", { other: "x" }), "Hello {who}");
});

test("a language with more than two plural forms gets all of them", () => {
  // Russian: one / few / many. A `count === 1 ? a : b` ternary — which is what
  // the editor does today in six places — cannot express this, which is the
  // reason plural selection belongs in the seam and not at the call site.
  setCatalogue("ru", {
    "count.words.one": "{count} слово",
    "count.words.few": "{count} слова",
    "count.words.many": "{count} слов",
    "count.words.other": "{count} слова",
  });
  setLocale("ru");
  assert.equal(t("count.words", { count: 1 }), "1 слово");
  assert.equal(t("count.words", { count: 3 }), "3 слова");
  assert.equal(t("count.words", { count: 5 }), "5 слов");
  assert.equal(t("count.words", { count: 21 }), "21 слово");
});

test("English plurals still select one/other", () => {
  assert.equal(t("count.words", { count: 1 }), "1 word");
  assert.equal(t("count.words", { count: 0 }), "0 words");
  assert.equal(t("count.words", { count: 2 }), "2 words");
});

test("a language missing a plural category falls back to `other`, not to the key", () => {
  // A catalogue that carries only `other` must still answer `one`.
  setCatalogue("xx", { "count.words.other": "{count} sanaa" });
  setLocale("xx");
  assert.equal(t("count.words", { count: 1 }), "1 sanaa");
});

test("the fallback chain goes region, language, English", () => {
  assert.deepEqual(fallbackChain("pt-BR"), ["pt-BR", "pt", "en"]);
  assert.deepEqual(fallbackChain("zh-Hant-TW"), ["zh-Hant-TW", "zh-Hant", "zh", "en"]);
  assert.deepEqual(fallbackChain("en"), ["en"]);
});

test("a Brazilian user with a thin pt-BR gets Portuguese before English", () => {
  setCatalogue("pt", { "file.save": "Guardar", "app.greeting": "Olá {who}" });
  setCatalogue("pt-BR", { "file.save": "Salvar" });
  setLocale("pt-BR");
  assert.equal(t("file.save"), "Salvar", "the region catalogue wins where it answers");
  assert.equal(t("app.greeting", { who: "mundo" }), "Olá mundo", "and the language fills in");
  setCatalogue("pt-BR", { "file.save": "Salvar" });
  assert.equal(t("count.words", { count: 2 }), "2 words", "and English is the last resort");
});

test("an unknown key returns the key, loudly, and `has` reports it", () => {
  assert.equal(t("no.such.key"), "no.such.key");
  assert.equal(has("no.such.key"), false);
  assert.equal(has("file.save"), true);
});

test("direction is by language subtag, and only for the interface", () => {
  assert.equal(direction("ar"), "rtl");
  assert.equal(direction("ar-EG"), "rtl");
  assert.equal(direction("he"), "rtl");
  assert.equal(direction("ur-PK"), "rtl");
  assert.equal(direction("en"), "ltr");
  assert.equal(direction("zh-Hans"), "ltr");
  assert.equal(direction(""), "ltr");
  assert.equal(direction(undefined), "ltr");
});

test("numbers, dates and lists format in the active locale", () => {
  assert.equal(n(1024), "1,024");
  setLocale("de");
  assert.equal(n(1024), "1.024", "German groups with a full stop");
  setLocale("en");
  const day = new Date(Date.UTC(2026, 0, 2));
  assert.match(d(day, { dateStyle: "short", timeZone: "UTC" }), /1\/2\/26/);
  setLocale("en-GB");
  assert.match(d(day, { dateStyle: "short", timeZone: "UTC" }), /02\/01\/2026/);
  setLocale("en");
  assert.equal(list(["a", "b", "c"]), "a, b, and c");
});

test("switching locale switches every lookup at once", () => {
  setCatalogue("fr", { "file.save": "Enregistrer" });
  assert.equal(t("file.save"), "Save");
  setLocale("fr");
  assert.equal(t("file.save"), "Enregistrer");
  setLocale("en");
  assert.equal(t("file.save"), "Save");
});
