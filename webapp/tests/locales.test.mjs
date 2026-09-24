// Which language a browser gets, and why (docs/124 §5).
//
// The negotiation is where a locale system quietly goes wrong: `de-AT` must
// find German, `zh-TW` must find TRADITIONAL Chinese rather than being
// truncated to `zh` and handed Simplified, and an unknown preference must fall
// through to the next one the user listed rather than straight to English.
import assert from "node:assert/strict";
import test from "node:test";

const { LOCALES, bestLocale, isShipped } = await import("../src/locales.mjs");

test("the owner asked for at least fifteen, and every one names itself", () => {
  assert.ok(LOCALES.length - 1 >= 15, `${LOCALES.length - 1} translated locales`);
  for (const locale of LOCALES) {
    assert.ok(locale.endonym.trim(), `${locale.tag} has no endonym`);
    assert.ok(isShipped(locale.tag));
  }
  // The endonym is the language's name in ITSELF — a picker listing "German"
  // is unusable by the person who needs it most.
  assert.equal(LOCALES.find((l) => l.tag === "de").endonym, "Deutsch");
  assert.equal(LOCALES.find((l) => l.tag === "ja").endonym, "日本語");
});

test("a region falls back to its language", () => {
  assert.equal(bestLocale(["de-AT"]), "de");
  assert.equal(bestLocale(["fr-CA"]), "fr");
  assert.equal(bestLocale(["en-GB"]), "en");
  assert.equal(bestLocale(["es-419"]), "es");
});

test("Chinese resolves by SCRIPT, not by truncation", () => {
  // Truncating `zh-TW` to `zh` and serving Simplified is not a near miss; it
  // is the wrong writing system.
  assert.equal(bestLocale(["zh-TW"]), "zh-Hant");
  assert.equal(bestLocale(["zh-HK"]), "zh-Hant");
  assert.equal(bestLocale(["zh-Hant"]), "zh-Hant");
  assert.equal(bestLocale(["zh-CN"]), "zh-Hans");
  // A three-part tag must shed its REGION first and find the script tag it
  // already carries. Shedding from the front instead lands on bare `zh` and
  // hands a Taiwanese user Simplified — the same defect by a different route.
  assert.equal(bestLocale(["zh-Hant-TW"]), "zh-Hant");
  assert.equal(bestLocale(["zh"]), "zh-Hans");
});

test("European Portuguese gets Portuguese rather than English", () => {
  assert.equal(bestLocale(["pt-PT"]), "pt-BR");
  assert.equal(bestLocale(["pt"]), "pt-BR");
});

test("an unshipped language falls through to the NEXT preference", () => {
  assert.equal(bestLocale(["sv", "fr-CA"]), "fr");
  assert.equal(bestLocale(["is", "cy", "ja"]), "ja");
});

test("nothing recognisable is English, and so is nothing at all", () => {
  assert.equal(bestLocale(["xx"]), "en");
  assert.equal(bestLocale([]), "en");
  assert.equal(bestLocale(), "en");
  assert.equal(bestLocale([""]), "en");
  assert.equal(bestLocale([null, undefined]), "en");
});

test("an exact preference wins over any fallback", () => {
  assert.equal(bestLocale(["pt-BR", "en"]), "pt-BR");
  assert.equal(bestLocale(["uk", "ru"]), "uk");
});
