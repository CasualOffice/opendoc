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
  hasOverrides,
  resetI18n,
  setCatalogue,
  setLocale,
  setOverrides,
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

// ---- Host string overrides (`docs/126` phase 3) ------------------------------
// The layer a white-label installs, and `docs/126` is explicit that it must be a
// LAYER on this seam rather than a second mechanism: "One mechanism, not two." The
// cases below are the ones a second mechanism would get wrong — and the ones
// registering the host's words as a twentieth CATALOGUE would get wrong too, which
// is why `setOverrides` exists instead.

test("a host override beats every catalogue, in every language", () => {
  // Entirely first, not interleaved per tag. An override that lost to one of the
  // nineteen catalogues would white-label the editor in eighteen languages and not
  // the nineteenth — the kind of almost-working somebody finds in a screenshot.
  setCatalogue("de", { "file.save": "Speichern" });
  setOverrides({ "*": { "file.save": "Keep" } });
  assert.equal(t("file.save"), "Keep");
  setLocale("de");
  assert.equal(t("file.save"), "Keep", "the German catalogue beat the host's override");
});

test("the reader's own language outranks the every-language override", () => {
  // `*` means "in every language", so it must beat a FALLBACK language: a German
  // reader gets the host's every-language word, not the host's English one. The
  // reader's OWN language still wins, because a host who wrote a German override
  // meant it for German readers.
  setOverrides({
    "*": { "file.save": "Keep" },
    de: { "file.save": "Behalten" },
    en: { "file.save": "Retain" },
  });
  setLocale("de");
  assert.equal(t("file.save"), "Behalten");
  setLocale("en");
  assert.equal(t("file.save"), "Retain");
  setLocale("fr");
  assert.equal(t("file.save"), "Keep", "French fell through to the host's ENGLISH, not its wildcard");
});

test("a region falls back to its base language before the wildcard", () => {
  setOverrides({ "*": { "file.save": "Keep" }, de: { "file.save": "Behalten" } });
  setLocale("de-AT");
  assert.equal(t("file.save"), "Behalten");
});

test("an override only covers the keys it names, and `has` answers for it", () => {
  setOverrides({ "*": { "file.save": "Keep" } });
  assert.equal(t("app.greeting", { who: "world" }), "Hello world");
  // `has()` must answer for an override, or `localizeTree` skips the element and
  // the markup keeps its authored English — a white-label that relabels the script
  // and not the page.
  setOverrides({ "*": { "brand.only": "Acme" } });
  assert.equal(has("brand.only"), true);
  assert.equal(t("brand.only"), "Acme");
});

test("an override interpolates like any other string", () => {
  setOverrides({ "*": { "app.greeting": "Welcome to Acme, {who}" } });
  assert.equal(t("app.greeting", { who: "world" }), "Welcome to Acme, world");
});

test("an override may carry a plural family, and answers in its own words", () => {
  // One layer at a time, the same rule the catalogues follow: a host who supplied
  // only `other` answers in their own words for every count rather than losing
  // `one` to ours, which would read as two different products in one sentence.
  setOverrides({ "*": { "count.words.other": "{count} Acme words" } });
  assert.equal(t("count.words", { count: 3 }), "3 Acme words");
  assert.equal(t("count.words", { count: 1 }), "1 Acme words");
});

test("overrides are announced, and cleared by the reset every test relies on", () => {
  assert.equal(hasOverrides(), false);
  setOverrides({ "*": { "file.save": "Keep" } });
  assert.equal(hasOverrides(), true);
  setOverrides(null);
  assert.equal(hasOverrides(), false);
  assert.equal(t("file.save"), "Save");
  // And `resetI18n` clears them. A test that installed a white-label and did not
  // clear it would white-label every test after it, which is the hardest kind of
  // green-for-the-wrong-reason to find.
  setOverrides({ "*": { "file.save": "Keep" } });
  resetI18n();
  setCatalogue("en", EN);
  assert.equal(t("file.save"), "Save");
});

test("a malformed override set is ignored rather than thrown", () => {
  // A host's configuration reaching `t()` must not be able to take the editor
  // down: every label in the chrome resolves through here.
  setOverrides({ "*": null, de: "not an object", en: { "file.save": "Retain" } });
  assert.equal(t("file.save"), "Retain");
  setLocale("de");
  assert.equal(t("file.save"), "Retain", "German fell through to the host's English");
});
