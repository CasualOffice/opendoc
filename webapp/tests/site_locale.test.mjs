// The public site's localisation seam (`109` HF-198).
//
// The site used to be the one part of this product with no way to say anything
// in a second language, which `no_unrouted_strings.test.mjs` had to carry as a
// declared exception. `src/site_locale.mjs` is the seam that closed it, and it
// reuses the editor's — `i18n.mjs` resolves, `localize.mjs` applies, the markup
// declares its keys with the English beside them. What is worth testing here is
// therefore not the lookup (that is `i18n.test.mjs`) but the four things this
// module is the only owner of: which locale a reader gets, that a fetched
// catalogue actually reaches the page, that the picker can be used and
// remembers, and that every page on the site loads the thing at all.
//
// No DOM library. The repository has none, and the shapes this module touches
// are small enough to stand up by hand — which also keeps each test honest
// about exactly what it depends on.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");

// Imported BEFORE any fake `document` exists, so the module's auto-start does
// not fire: it is guarded on `typeof document !== "undefined"`, and a test that
// booted the page would be testing the guard rather than the module.
const { buildLanguagePicker, preferredLocale, resetSiteLocales, startSiteLocalisation, useLocale } =
  await import("../src/site_locale.mjs");
const { resetI18n } = await import("../src/i18n.mjs");
const { LOCALES } = await import("../src/locales.mjs");

/** An element carrying one text node, the shape `localizeTree` relabels. */
function labelled(key, english) {
  return {
    dataset: { i18n: key },
    childNodes: [{ nodeType: 3, nodeValue: english }],
    childElementCount: 0,
    get text() {
      return this.childNodes[0].nodeValue;
    },
  };
}

/** Enough of a document for `localizeTree` and `applyDocumentLocale`. */
function fakeDocument(byKeyed = []) {
  return {
    documentElement: {},
    querySelectorAll(selector) {
      return selector === "[data-i18n]" ? byKeyed : [];
    },
    createElement() {
      return { set textContent(value) {}, get textContent() {} };
    },
  };
}

function withDocument(document, run) {
  const hadNode = "Node" in globalThis;
  globalThis.document = document;
  // `localize.mjs` reads `Node.TEXT_NODE` to find an element's own words.
  if (!hadNode) globalThis.Node = { TEXT_NODE: 3 };
  const restore = () => {
    delete globalThis.document;
    if (!hadNode) delete globalThis.Node;
  };
  // `finally` on the PROMISE, not on the try block: a `try { return run() }
  // finally { restore() }` tears the document down the moment the async body
  // first awaits, and every assertion after that await then runs against no
  // document at all. It failed exactly that way the first time.
  let result;
  try {
    result = run();
  } catch (error) {
    restore();
    throw error;
  }
  if (result && typeof result.then === "function") return result.finally(restore);
  restore();
  return result;
}

test("the locale a reader gets: an explicit ?lang= first, then their choice, then the browser", () => {
  assert.equal(preferredLocale({ search: "?lang=ja", saved: "de", browser: ["fr"] }), "ja");
  assert.equal(preferredLocale({ search: "", saved: "de", browser: ["fr"] }), "de");
  assert.equal(preferredLocale({ search: "", saved: "", browser: ["fr-CA"] }), "fr");
  // The same negotiation the editor uses, so `zh-TW` is Traditional rather than
  // truncated to `zh` and handed Simplified.
  assert.equal(preferredLocale({ search: "", saved: "", browser: ["zh-TW"] }), "zh-Hant");
  // Nothing recognisable is English, and so is nothing at all.
  assert.equal(preferredLocale({ search: "?lang=xx", saved: "", browser: ["qq"] }), "en");
  assert.equal(preferredLocale(), "en");
});

test("a fetched catalogue reaches the page, and a failed fetch leaves the English in the markup", async () => {
  resetI18n();
  resetSiteLocales();
  const nav = labelled("site.header.overview", "Overview");
  const document = fakeDocument([nav]);
  await withDocument(document, async () => {
    const inForce = await useLocale("de", {
      fetcher: async (tag) => (tag === "de" ? { "site.header.overview": "Überblick" } : null),
    });
    assert.equal(inForce, "de");
    assert.equal(nav.text, "Überblick");
    assert.equal(document.documentElement.lang, "de");
    assert.equal(document.documentElement.dir, "ltr");
  });

  // A catalogue that cannot be fetched falls back to English, and English is
  // whatever the markup already says — never a dotted key. This is the whole
  // reason the English stays beside the key.
  resetI18n();
  resetSiteLocales();
  const heading = labelled("site.index.buildDocuments", "Build documents on an engine");
  await withDocument(fakeDocument([heading]), async () => {
    const inForce = await useLocale("ja", { fetcher: async () => null });
    assert.equal(inForce, "en");
    assert.equal(heading.text, "Build documents on an engine");
  });
});

test("a right-to-left catalogue turns the page around", async () => {
  resetI18n();
  resetSiteLocales();
  const document = fakeDocument([]);
  await withDocument(document, async () => {
    await useLocale("ar", { fetcher: async () => ({ "@@direction": "rtl" }) });
    assert.equal(document.documentElement.dir, "rtl");
    assert.equal(document.documentElement.lang, "ar");
  });
});

/** A `<select>` with only the automatic option, as the header partial ships it. */
function fakeSelect() {
  const automatic = { value: "", textContent: "Automatic", lang: undefined };
  const options = [automatic];
  const label = { hidden: true };
  return {
    options,
    value: "",
    listeners: {},
    querySelector: (selector) => (selector === 'option[value=""]' ? automatic : null),
    appendChild: (option) => {
      // A real `<option>` can take itself out again, and `buildLanguagePicker`
      // uses that to rebuild the list when the language changes.
      option.remove = () => {
        const at = options.indexOf(option);
        if (at >= 0) options.splice(at, 1);
      };
      options.push(option);
    },
    closest: () => ({ removeAttribute: (name) => delete label[name] }),
    addEventListener(type, handler) {
      this.listeners[type] = handler;
    },
    label,
  };
}

test("the picker offers every shipped language by its own name, and shows itself", () => {
  resetI18n();
  const select = fakeSelect();
  const document = {
    createElement: () => ({ value: "", textContent: "", lang: "" }),
  };
  withDocument(document, () => {
    buildLanguagePicker(select, { saved: "de", browser: ["fr"] });
  });
  // Nineteen languages plus the automatic entry.
  assert.equal(select.options.length, LOCALES.length + 1);
  const endonyms = select.options.slice(1).map((option) => option.textContent);
  assert.deepEqual(
    endonyms,
    LOCALES.map((locale) => locale.endonym),
    "a picker that lists languages in a language you cannot read is one you cannot use to escape",
  );
  // The automatic entry names the language it would pick, because "Automatic"
  // alone tells a reader nothing about what they are choosing.
  assert.match(select.options[0].textContent, /Français$/);
  assert.equal(select.value, "de", "the saved choice is the one shown as selected");
  // And the control is revealed. It ships `hidden` so a page whose scripts did
  // not run never shows a picker with one option in it.
  assert.equal(select.label.hidden, undefined);
});

test("choosing a language persists it where the EDITOR reads it, not beside it", async () => {
  resetI18n();
  resetSiteLocales();
  const store = new Map();
  const view = {
    localStorage: {
      getItem: (key) => store.get(key) ?? null,
      setItem: (key, value) => store.set(key, value),
    },
  };
  const select = fakeSelect();
  const document = { ...fakeDocument([]), createElement: () => ({ value: "", textContent: "", lang: "" }) };
  await withDocument(document, async () => {
    await startSiteLocalisation({
      select,
      view,
      search: "",
      browser: ["en"],
      fetcher: async () => ({}),
    });
    select.value = "ja";
    await select.listeners.change();
  });
  // ONE settings object, shared with the editor: a reader who sets the site to
  // Japanese and opens the editor expects Japanese, and the site is not a
  // different product from the thing it advertises.
  assert.equal(JSON.parse(store.get("opendoc.settings")).language, "ja");
  assert.deepEqual([...store.keys()], ["opendoc.settings"], "no second preference key");
});

test("storage being unavailable costs the session persistence and nothing else", async () => {
  resetI18n();
  resetSiteLocales();
  const view = {
    get localStorage() {
      throw new Error("site data blocked");
    },
  };
  const document = { ...fakeDocument([]), createElement: () => ({ value: "", textContent: "", lang: "" }) };
  await withDocument(document, async () => {
    const inForce = await startSiteLocalisation({
      select: fakeSelect(),
      view,
      search: "?lang=de",
      browser: [],
      fetcher: async () => ({}),
    });
    assert.equal(inForce, "de", "the page still gets its language when storage throws");
  });
});

test("every page the site serves loads the seam, including the ones a directory down", () => {
  // The module is loaded once, from the shared footer partial, so no page can
  // be forgotten — but "no page can be forgotten" is a claim about the built
  // output, so it is checked against the built output. `build-doc-pages.mjs`
  // rebases the partial's `./` to `../` for the reference pages; a reference
  // page asking for `./src/site_locale.mjs` would 404 and be silently English.
  const pages = readdirSync(WEBAPP)
    .filter((name) => name.endsWith(".page.html"))
    .map((name) => name.replace(/\.page\.html$/, ".html"));
  assert.ok(pages.length >= 5, `expected the site's pages, found ${pages.length}`);
  for (const page of pages) {
    assert.match(
      readFileSync(join(WEBAPP, page), "utf8"),
      /<script type="module" src="\.\/src\/site_locale\.mjs"><\/script>/,
      `${page} does not load site_locale.mjs, so it is English whatever the reader chose`,
    );
  }
  const reference = readdirSync(join(WEBAPP, "reference")).filter((name) => name.endsWith(".html"));
  assert.ok(reference.length >= 10, `expected the reference pages, found ${reference.length}`);
  for (const page of reference) {
    assert.match(
      readFileSync(join(WEBAPP, "reference", page), "utf8"),
      /<script type="module" src="\.\.\/src\/site_locale\.mjs"><\/script>/,
      `reference/${page} loads the seam from the wrong directory, so the request 404s`,
    );
  }
});

test("the site's keys are extracted from the site, not hand-written into the catalogue", async () => {
  // `locales/en.json` is DERIVED. What is worth asserting here — beyond the
  // staleness check `locale_coverage.test.mjs` already runs — is that the site's
  // keys really come from the site's own markup, so a key cannot be invented in
  // the catalogue and quietly answered by nothing.
  const { keysFromSite } = await import("../tools/build-locale.mjs");
  const fromMarkup = keysFromSite();
  const catalogue = JSON.parse(readFileSync(join(WEBAPP, "locales/en.json"), "utf8"));
  const declared = Object.keys(catalogue).filter((key) => key.startsWith("site."));
  assert.ok(declared.length > 300, `only ${declared.length} site keys — did the extraction stop?`);
  assert.deepEqual(
    declared.filter((key) => !fromMarkup.has(key)),
    [],
    "a site.* key in the catalogue that no page declares answers nothing",
  );
  for (const [key, english] of fromMarkup) {
    assert.equal(catalogue[key], english, `${key} in the catalogue is not what the markup says`);
  }
});
