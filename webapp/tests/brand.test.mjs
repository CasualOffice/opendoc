// White-labelling: the exit gate, asserted rather than described.
//
// `docs/126` phase 3's gate is "a build is white-labelled with NO CODE CHANGES —
// configuration only — and still passes the contrast sweep". That is two claims
// and this file holds both, plus the refusals that make the second one true.
//
// THE CONDITION IS CREATED, NOT ASSUMED. The lesson phase 2 paid for is that its
// `readonly` test stayed green when the API's capability gate was removed
// entirely, because a different layer caught the edits. A theme guard is wide open
// to the same failure: a test that boots the default build and finds legible text
// has proved that OUR palette passes, which was already true and is already
// guarded by `style_tokens.test.mjs`. So everything below generates from
// `brand.example.json` — a real host configuration, a deliberately different
// palette — and asserts against THAT.
//
// The browser half is `tests/e2e/white-label.spec.mjs`, which installs the
// stylesheet this generator produces into a real editor and runs the full
// `contrast-audit.mjs` sweep over both themes. Node can prove the palette is
// sound; only a browser can prove the cascade delivers it.
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import {
  BrandRefusal,
  LOCKED_TOKEN_PREFIXES,
  OVERRIDABLE,
  PRODUCT,
  TAB_TITLE_POLICIES,
  auditBrand,
  brandCss,
  brandModule,
  deriveStrings,
  editorRegions,
  markPaths,
  normalize,
  readCatalogues,
} from "../tools/build-brand.mjs";
import { readPalettes } from "../tools/palette_source.mjs";
import { TEXT_CONTRAST_FLOOR, auditPalette, contrastRatio, parseHex } from "../src/contrast.mjs";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const GENERATOR = join(WEBAPP, "tools", "build-brand.mjs");
const { themes } = readPalettes();
const read = (name) => readFileSync(join(WEBAPP, name), "utf8");

/** The worked example, normalized. The thing every claim below is about. */
const EXAMPLE = normalize(JSON.parse(read("brand.example.json")), themes);
/** What the shipped build carries: all-null, so the product is the product. */
const SHIPPED = normalize(JSON.parse(read("brand.json")), themes);

test("the shipped configuration overrides nothing, so a white-label is a decision", () => {
  // Not a formality. If `brand.json` carried our own name and palette as data,
  // every assertion below could pass while the mechanism was only ever exercised
  // with the values it was written against, and "a host supplies a DIFFERENT
  // palette" would be untested. All-null also means the default build behaves
  // exactly as it did before this phase.
  assert.equal(SHIPPED.name, null);
  assert.equal(SHIPPED.mark, null);
  assert.equal(SHIPPED.tabTitle, null);
  assert.deepEqual(SHIPPED.theme, { tokens: {}, light: {}, dark: {} });
  assert.deepEqual(SHIPPED.strings, {});
  assert.equal(brandModule(SHIPPED, {}).includes(`"name": ${JSON.stringify(PRODUCT.name)}`), true);
  assert.equal(brandModule(SHIPPED, {}).includes('"accentPinned": false'), true);
});

test("the example is a REAL white-label: a different name, and a different palette", () => {
  // The guard against a guard that proves nothing. If the example's palette were
  // ours, every contrast assertion below would be re-asserting
  // `style_tokens.test.mjs`.
  assert.notEqual(EXAMPLE.name, PRODUCT.name);
  const moved = [];
  for (const [theme, palette] of themes) {
    const own = theme === "light" ? EXAMPLE.theme.light : EXAMPLE.theme.dark;
    for (const [token, value] of Object.entries({ ...EXAMPLE.theme.tokens, ...own })) {
      if (palette[token] !== value) moved.push(`${theme} ${token}`);
    }
  }
  assert.ok(
    moved.length >= 20,
    `the example moves only ${moved.length} token values; a palette that is nearly ours cannot ` +
      "show that a host's palette is checked",
  );
  assert.equal(EXAMPLE.theme.tokens["--accent"], "#0f6d6a");
});

test("a white-labelled palette still clears every WCAG floor, in all three themes", () => {
  // THE EXIT GATE'S second half, in node. The palette is measured AS IT WILL
  // RENDER — our tokens with the host's written over them — because a brand
  // colour measured in isolation passes when it is unreadable only against the
  // surface it will actually be painted on.
  const { palettes, failures } = auditBrand(EXAMPLE, themes);
  assert.deepEqual(failures, []);
  assert.equal(palettes.size, 3);
  for (const [theme, palette] of palettes) {
    const { failures: bad, unreadable } = auditPalette(palette);
    assert.deepEqual(unreadable, [], `${theme} carries a value nothing can measure`);
    assert.deepEqual(bad.map((f) => f.role), [], `${theme} fails its floors`);
  }
});

test("a palette that fails AA is REFUSED, with the ratio and a value that would pass", () => {
  // WHAT HAPPENS WHEN A HOST'S COLOUR FAILS AA, asserted rather than left to a
  // comment. Not corrected — a product shipped in an accent nobody approved is a
  // lie about the brand, which is the same reason `previewInkIsLegible` returns a
  // boolean instead of a nudged colour. Not warned about — that is ONLYOFFICE's
  // nag-modal failure mode (`docs/125` §1.1), where the integrator ships it and
  // then discovers the problem.
  //
  // A plausible first attempt, not a contrived one: a bright brand orange with
  // white text on it, and a light grey that looks fine on white in a design tool.
  const raw = {
    version: 1,
    name: "Northwind Docs",
    theme: {
      tokens: { "--accent": "#f5a524", "--accent-ink": "#ffffff" },
      light: { "--muted": "#9aa0a6" },
      dark: {},
    },
  };
  const { failures } = auditBrand(normalize(raw, themes), themes);
  assert.ok(failures.length >= 4, `expected several refusals, got ${failures.length}`);
  const accentInk = failures.find((f) => f.includes("--accent-ink") && f.startsWith("light:"));
  assert.match(accentInk, /2\.04:1/, "the refusal must carry the MEASURED ratio");
  assert.match(accentInk, /must clear 4\.5:1/, "the refusal must name the floor");
  assert.match(accentInk, /#[0-9a-f]{6} there would pass/, "the refusal must offer a value that works");
  // This host set BOTH ends, so the ink is the one named — the conventional
  // reading, and the one the palette's own guard uses. The case where they set
  // only one is the test below.
  assert.match(accentInk, /You set --accent-ink:/, "a host who set the ink is told about the ink");
  // And every theme is reported, not just the first: a host fixing one theme and
  // discovering the other on the next run is five builds for one mistake.
  for (const theme of ["light", "system dark", "explicit dark"]) {
    assert.ok(
      failures.some((f) => f.startsWith(`${theme}:`)),
      `${theme} was not reported`,
    );
  }
});

test("the refusal names the value the host supplied, not the other end of the pair", () => {
  // THE DEFECT. A host sets ONE token — `--accent`, the thing a brand actually
  // is — and the pair that fails is `--accent-ink` on `--accent`. The refusal used
  // to read "`--accent-ink` (#ffffff) … #484848 would pass", which tells them to
  // darken a value that is not in their file, that they did not choose, and that
  // they would have to go and look up to even find. The actionable half — what to
  // make `--accent` — was missing entirely, and refusing rather than warning is
  // only the better choice if the failure lands on the person who can fix it WITH
  // the fix.
  //
  // Both halves are asserted, because naming the right token while suggesting the
  // wrong end's number would be the same defect with better wording.
  const raw = { version: 1, theme: { tokens: { "--accent": "#f5a524" }, light: {}, dark: {} } };
  const { failures } = auditBrand(normalize(raw, themes), themes);
  const pair = failures.find((f) => f.startsWith("light:") && f.includes("--accent-ink"));
  assert.ok(pair, `no light --accent-ink failure: ${failures.join(" | ")}`);
  assert.match(pair, /--accent-ink \(#ffffff\) on --accent \(#f5a524\)/, "the PAIR is still the fact");
  assert.match(pair, /You set --accent:/, "the refusal must name the token the host wrote");
  assert.doesNotMatch(pair, /You set --accent-ink:/, "the host never set --accent-ink");

  // And the number really is a value for THAT token: white on it clears the floor.
  const suggested = pair.match(/You set --accent: (#[0-9a-f]{6})/)?.[1];
  assert.ok(suggested, `no suggestion in: ${pair}`);
  assert.ok(
    contrastRatio(parseHex("#ffffff"), parseHex(suggested)) >= TEXT_CONTRAST_FLOOR,
    `${suggested} as --accent still fails under #ffffff ink — the refusal offered a fix that is not one`,
  );

  // The derived role too: `--accent-text` is `var(--accent)` in light, so its
  // failure is the host's `--accent` as well and must say so rather than sending
  // them after a token whose value is literally a reference to theirs.
  const derived = failures.find((f) => f.startsWith("light:") && f.includes("--accent-text"));
  assert.ok(derived, `no light --accent-text failure: ${failures.join(" | ")}`);
  assert.match(derived, /You set --accent:/, "a plain var() indirection is followed to the host's token");
});

test("the refusal reaches the command line, on its real exit status", () => {
  // THE TEST THAT HAD TO EXIST, and the reason it does.
  //
  // Everything above calls `auditBrand` directly. Deleting the generator's
  // `throw new BrandRefusal(failures)` left all of it GREEN: the assertions proved
  // the audit can see a failing palette, and never proved the generator refuses
  // one. A host would have got a stylesheet full of unreadable text and a build
  // that exited 0. That is phase 2's finding exactly — a door standing open with a
  // lock behind it — and it is why this drives the real command line and reads the
  // real exit status.
  //
  // Never piped into grep or tail: `a | tail && echo OK` tests TAIL's status, which
  // is how a failing gate printed PASS here once (#552).
  const run = (args) => {
    try {
      return { code: 0, out: String(execFileSync("node", [GENERATOR, ...args], { cwd: WEBAPP, stdio: "pipe" })) };
    } catch (err) {
      return { code: err.status ?? 1, out: `${err.stdout ?? ""}${err.stderr ?? ""}` };
    }
  };

  // The committed tree passes, which is the positive control: without it a
  // generator that refused everything would satisfy the negative case below.
  assert.equal(run(["--check"]).code, 0, "--check must pass on a committed, up-to-date tree");
  assert.equal(run(["--check", "--config", "brand.example.json"]).code, 0, "the example must be accepted");

  // And a failing palette is refused, at the command line, non-zero.
  const failing = join(tmpdir(), `opendoc-brand-fail-${process.pid}.json`);
  writeFileSync(
    failing,
    JSON.stringify({
      version: 1,
      theme: { tokens: { "--accent": "#f5a524", "--accent-ink": "#ffffff" }, light: {}, dark: {} },
    }),
  );
  try {
    const refused = run(["--check", "--config", failing]);
    assert.notEqual(refused.code, 0, "a palette that fails AA must exit non-zero");
    assert.match(refused.out, /brand\.json was refused/);
    assert.match(refused.out, /--accent-ink/);
    assert.match(refused.out, /2\.04:1/);
    assert.match(refused.out, /would pass/);
    // `--config` without `--check` must not be able to WRITE somebody's experiment
    // over a deployment's committed artifacts.
    assert.equal(run(["--config", failing]).code, 2);
  } finally {
    rmSync(failing, { force: true });
  }
});

test("a host may not reach off-origin for a mark or a face", () => {
  // Never a CDN font link. `src/fonts.css` self-hosts Inter and Material Symbols
  // and `chrome_fonts.test.mjs` asserts no remote reference, because local-first
  // means the chrome paints with the network off. A white-label seam that let a
  // host name `https://fonts.example/...` would break that for every deployment
  // that used it, and the breakage would be invisible until the network was.
  for (const mark of ["https://cdn.example/logo.svg", "//cdn.example/logo.svg", "data:image/svg+xml,x"]) {
    assert.throws(
      () => normalize({ version: 1, mark }, themes),
      (err) => err instanceof BrandRefusal && /absolute or protocol-relative|start with/.test(err.message),
      `${mark} was accepted`,
    );
  }
  assert.throws(
    () => normalize({ version: 1, font: { family: "Acme", src: "https://fonts.example/a.woff2" } }, themes),
    (err) => err instanceof BrandRefusal && /network off/.test(err.message),
  );
  // And a relative one is accepted, or the assertions above would pass for a
  // generator that refused everything.
  assert.equal(normalize({ version: 1, mark: "./acme.svg" }, themes).mark, "./acme.svg");
  assert.equal(
    normalize({ version: 1, font: { family: "Acme", src: "./acme.woff2" } }, themes).font.family,
    "Acme",
  );
});

test("the paper layer is not a host's to move", () => {
  // The sheet is white in both themes on purpose, so everything drawn onto the
  // raster has a fixed contrast partner. A host who darkened `--paper` would put
  // the dark theme's insertion and deletion marks on a dark page at roughly 2:1,
  // and NO existing guard would see it: the paper tokens are in neither role
  // list, deliberately, because they are not a theme.
  for (const group of ["tokens", "light", "dark"]) {
    assert.throws(
      () => normalize({ version: 1, theme: { [group]: { "--paper": "#101214" } } }, themes),
      (err) => err instanceof BrandRefusal && /paper layer is the same in both themes/.test(err.message),
      `theme.${group} accepted a paper token`,
    );
  }
  assert.ok(LOCKED_TOKEN_PREFIXES.includes("--paper"));
});

test("structure is not brand: geometry and z-order are refused", () => {
  // `docs/63` calls the flattened radii "a considered flat redesign" and says to
  // propose visual changes rather than make them. A seam that let a host round
  // every corner would be making them on the owner's behalf, for every
  // deployment, without the owner ever seeing the proposal.
  for (const token of ["--radius", "--space-3", "--z-dialog", "--fs-body", "--h-ribbon"]) {
    assert.throws(
      () => normalize({ version: 1, theme: { light: { [token]: "9px" } } }, themes),
      (err) => err instanceof BrandRefusal && /Overridable tokens are brand and palette only/.test(err.message),
      `${token} was accepted`,
    );
  }
  assert.ok(OVERRIDABLE.every((t) => t.startsWith("--")));
  assert.equal(OVERRIDABLE.includes("--radius"), false);
});

test("a per-theme token in the shared block is refused, and the other way round", () => {
  // `--accent` is one colour in both themes by design and `--surface` is not, so
  // declaring either in the wrong group is a mistake whose symptom is the dark
  // theme painted with the light one's values — the exact shape of HF-092, where
  // three patches landed in one dark entry point and not the other.
  assert.throws(
    () => normalize({ version: 1, theme: { light: { "--accent": "#0f6d6a" } } }, themes),
    (err) => err instanceof BrandRefusal && /same in both themes by design/.test(err.message),
  );
  assert.throws(
    () => normalize({ version: 1, theme: { tokens: { "--surface": "#ffffff" } } }, themes),
    (err) => err instanceof BrandRefusal && /belongs in theme\.light and theme\.dark/.test(err.message),
  );
});

test("an unknown config version is refused rather than coerced", () => {
  for (const version of [undefined, 0, 2, "1", null]) {
    assert.throws(
      () => normalize({ version }, themes),
      (err) => err instanceof BrandRefusal && /version must be 1/.test(err.message),
      `version ${JSON.stringify(version)} was accepted`,
    );
  }
});

test("renaming the product renames it in all nineteen languages, derived", () => {
  // A host writes ONE field. If they had to write nineteen overrides, the one they
  // forgot is the one a reader sees — which is `docs/99` §9's argument about every
  // other published fact, applied to a product name.
  const catalogues = readCatalogues();
  assert.equal(catalogues.size, 19, "nineteen catalogues is the claim this depends on");
  const strings = deriveStrings(EXAMPLE, catalogues);
  const tags = Object.keys(strings).filter((tag) => tag !== "*");
  assert.ok(tags.length >= 19, `only ${tags.length} locales got an override`);
  for (const [tag, entries] of catalogues) {
    const ours = Object.entries(entries).filter(
      ([key, value]) => !key.startsWith("@@") && typeof value === "string" && value.includes(PRODUCT.name),
    );
    assert.ok(ours.length > 0, `${tag} carries no product name, so nothing proves the substitution`);
    for (const [key, value] of ours) {
      const replaced = strings[tag]?.[key];
      assert.ok(replaced !== undefined, `${tag} ${key} still reads our name`);
      assert.equal(replaced.includes(PRODUCT.name), false, `${tag} ${key} still contains our name`);
      assert.ok(replaced.includes(EXAMPLE.name), `${tag} ${key} does not carry the host's name`);
      // The sentence around the name is untouched: a German reader gets German.
      assert.equal(replaced, value.split(PRODUCT.name).join(EXAMPLE.name));
    }
  }
  // The product name really is in the catalogues, so this constant is not
  // silently substituting nothing after a rename.
  assert.ok(
    JSON.stringify([...catalogues.values()]).includes(PRODUCT.name),
    "PRODUCT.name is not in any catalogue, so the substitution above is a no-op",
  );
});

test("a host's own wording wins over the derived override", () => {
  const strings = deriveStrings(EXAMPLE, readCatalogues());
  assert.equal(strings.de["branding.themeSetByHost"], "Die Farbe wird von Northwind Docs vorgegeben.");
  // And the rest of German is still the derived substitution, not replaced wholesale.
  assert.ok(Object.keys(strings.de).length > 1, "an explicit override wiped the derived ones");
});

test("the shipped build derives no overrides at all", () => {
  assert.deepEqual(deriveStrings(SHIPPED, readCatalogues()), {});
});

test("the generated stylesheet declares the host palette in all three entry points", () => {
  const css = brandCss(EXAMPLE, markPaths(EXAMPLE));
  // A host who overrode only `:root` would white-label the light theme and leave
  // the dark one ours; and a reader who picked Dark on a light OS only ever sees
  // the explicit block, which is HF-092's whole story.
  assert.match(css, /:root,\n:root\[data-theme="light"\] \{/);
  assert.match(css, /@media \(prefers-color-scheme: dark\) \{\n {2}:root:not\(\[data-theme\]\) \{/);
  assert.match(css, /:root\[data-theme="dark"\] \{/);
  const dark = css.slice(css.indexOf("@media"));
  for (const [token, value] of Object.entries(EXAMPLE.theme.dark)) {
    const occurrences = dark.split(`${token}: ${value};`).length - 1;
    assert.equal(occurrences, 2, `${token} must be declared in BOTH dark entry points, not ${occurrences}`);
  }
  // The pin marker, which is how the runtime learns the accent is not the
  // visitor's to change. Without it `applySettings()` writes an inline `--accent`
  // that beats this whole stylesheet.
  assert.match(css, /--brand-accent-pinned: 1;/);
  // And the shipped build declares no such marker, or the visitor could never
  // change the accent in the product's own deployment.
  assert.equal(brandCss(SHIPPED, markPaths(SHIPPED)).includes("--brand-accent-pinned"), false);
});

test("the mark is referenced correctly from two different bases", () => {
  // `url()` in a stylesheet resolves against the STYLESHEET; an `href` in the page
  // resolves against the page. `src/brand.css` is one directory down, so the same
  // configured path has two spellings and getting one wrong is a silently missing
  // logo.
  const paths = markPaths(EXAMPLE);
  assert.equal(paths.pageMark, "./opendoc-mark.svg");
  assert.equal(paths.mark, "../opendoc-mark.svg");
  assert.match(brandCss(EXAMPLE, paths), /--brand-mark: url\("\.\.\/opendoc-mark\.svg"\)/);
  const noMark = markPaths(normalize({ version: 1, mark: false }, themes));
  assert.equal(noMark.showMark, false);
  assert.equal(noMark.pageMark, null);
  // "No mark anywhere" is expressible, not approximated by an invisible image:
  // both the favicon and the header element are gone.
  const regions = editorRegions(normalize({ version: 1, mark: false }, themes), noMark);
  assert.match(regions.get("brand-icons"), /No favicon/);
  assert.equal(regions.get("brand-mark").includes("<span"), false);
});

test("tab-title policy is the one place our brand left the editor", () => {
  assert.deepEqual([...TAB_TITLE_POLICIES], ["document+product", "document"]);
  assert.equal(EXAMPLE.tabTitle, "document");
  assert.match(brandModule(EXAMPLE, {}), /"tabTitle": "document"/);
  assert.match(brandModule(SHIPPED, {}), /"tabTitle": "document\+product"/);
});

test("editor.html carries every region the generator writes, and only generated content", () => {
  const editor = read("editor.html");
  for (const name of editorRegions(EXAMPLE, markPaths(EXAMPLE)).keys()) {
    assert.ok(
      editor.includes(`<!-- @generated ${name} -->`) && editor.includes(`<!-- @end ${name} -->`),
      `editor.html has no ${name} region, so the generator writes nothing and nobody notices`,
    );
  }
  // The brand mark is no longer a hardcoded `<img src>`: that was the thing a host
  // could not change from configuration.
  assert.equal(/<img[^>]*class="brand-logo"/.test(editor), false);
});

test("the white-labelled build changes NO code — only generated artifacts", () => {
  // THE EXIT GATE'S first half. Generate from the example configuration and prove
  // that everything that moves is a file this generator owns. Anything else moving
  // would mean white-labelling required a code change, which is the one thing the
  // gate forbids.
  const paths = markPaths(EXAMPLE);
  const produced = new Map([
    ["src/brand.css", brandCss(EXAMPLE, paths)],
    ["src/brand.mjs", brandModule(EXAMPLE, deriveStrings(EXAMPLE, readCatalogues()))],
  ]);
  // Both differ from the shipped ones — otherwise this proves nothing.
  for (const [name, content] of produced) {
    assert.notEqual(content, read(name), `${name} is unchanged by the example configuration`);
  }
  // And the three files the generator owns are the ONLY ones it writes. Asserted
  // against the generator's own manifest, so a FOURTH output cannot appear without
  // this list saying so — which is the half that keeps "no code changes" honest as
  // the generator grows. `editor.html` is on the list deliberately: it carries two
  // generated regions, and a host changing the favicon must not have to edit it.
  const source = readFileSync(GENERATOR, "utf8");
  const owned = [...source.matchAll(/\{ path: join\(WEBAPP, ([^)]+)\), content:/g)].map((m) => m[1]);
  assert.deepEqual(owned, ['"src", "brand.css"', '"src", "brand.mjs"', '"editor.html"']);
});
