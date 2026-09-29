#!/usr/bin/env node
// The white-label generator: `webapp/brand.json` → three committed artifacts.
//
// `docs/126` phase 3's exit gate is that "a build is white-labelled with NO CODE
// CHANGES — configuration only — and still passes the contrast sweep". That
// sentence decides the whole shape of this file, in three ways.
//
// 1. CONFIGURATION IS DATA, SO THE SEAM IS A GENERATOR. A host edits one JSON
//    file and runs one command. Everything the editor reads is produced from it:
//    `src/brand.css` (the palette, the mark, and one marker token the runtime
//    reads), `src/brand.mjs` (the name, the tab-title policy, and the string
//    overrides), and two generated regions in `editor.html` (the icons and the
//    brand element). `--check` fails the build when a committed artifact is not
//    what a fresh run produces, in BOTH directions — the same contract as
//    `build-embed-docs.mjs`, `build-site.py`, `build-doc-pages.mjs` and
//    `build-seo.mjs`, and deliberately not a fifth convention.
//
// 2. THE PALETTE IS VALIDATED, AND A FAILURE IS A REFUSAL. The contract itself —
//    which tokens a host may set, what a refusal says, and the stylesheet their
//    choices produce — is `src/brand_contract.mjs`, which is pure so that the SDK
//    configuration playground can run the SAME validator in a browser and show a
//    host the refusal this command line would give them. See §AA there.
//
// 3. NOTHING FETCHED. A mark or a font face must be a same-origin, relative
//    path, and an absolute or cross-origin one is refused. `src/fonts.css`
//    self-hosts Inter and Material Symbols and `tests/chrome_fonts.test.mjs`
//    asserts no `fonts.googleapis.com` or `gstatic.com` reference, because
//    local-first means the chrome paints with the network off. A white-label
//    that pulls the host's webfont off a CDN would break that for every
//    deployment, so a host supplies a FILE next to the editor and names it.
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import {
  BANNER,
  BrandRefusal,
  PRODUCT,
  auditBrand,
  brandCss,
  markPaths,
  normalize,
} from "../src/brand_contract.mjs";
import { readPalettes } from "./palette_source.mjs";

// The contract is `src/brand_contract.mjs` and every name it declares is
// re-exported here, so this module's callers — `build-embed-docs.mjs`,
// `tests/brand.test.mjs` — did not move when it split. Nothing below is a second
// copy: what moved is the pure half (what a host may set, what a refusal says,
// and the stylesheet the choices produce), because the SDK configuration
// playground has to run exactly that code in a browser, and a browser cannot
// import a module whose first line is `node:fs`.
export {
  BANNER,
  BrandRefusal,
  INDEPENDENT_ONLY,
  LOCKED_TOKEN_PREFIXES,
  OVERRIDABLE,
  PRODUCT,
  TAB_TITLE_POLICIES,
  auditBrand,
  brandCss,
  markPaths,
  normalize,
} from "../src/brand_contract.mjs";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const REPO = join(WEBAPP, "..");

/**
 * The host's string overrides, derived from the catalogues rather than written.
 *
 * A host renames the product once, in one field, and gets all nineteen
 * languages. The mechanism is a substitution over the committed catalogues: any
 * value containing the product's name yields an override with the name replaced,
 * under that locale's own tag. So "Über OpenDoc" becomes "Über Acme Docs" and a
 * German reader is not shown an English sentence because their host renamed
 * something.
 *
 * DERIVED, NOT HAND-WRITTEN, for the reason `docs/99` §9 gives about every other
 * published fact: a hand-written per-locale table would be nineteen chances to
 * forget one, and the one forgotten is the one a reader sees. A host who wants
 * different words in a given language still says so in `strings`, which is
 * merged over this and wins.
 *
 * Complexity: O(catalogue entries) once, at generate time.
 */
export function deriveStrings(config, catalogues) {
  const byLocale = {};
  if (config.name && config.name !== PRODUCT.name) {
    for (const [tag, entries] of catalogues) {
      const overrides = {};
      for (const [key, value] of Object.entries(entries)) {
        if (key.startsWith("@@") || typeof value !== "string") continue;
        if (!value.includes(PRODUCT.name)) continue;
        overrides[key] = value.split(PRODUCT.name).join(config.name);
      }
      if (Object.keys(overrides).length) byLocale[tag] = overrides;
    }
  }
  for (const [tag, entries] of Object.entries(config.strings)) {
    byLocale[tag] = { ...(byLocale[tag] ?? {}), ...entries };
  }
  return byLocale;
}

/** `src/brand.mjs` — the half a script needs: the name, the tab-title policy,
 *  and the derived string overrides. Data only; the module that installs it is
 *  `locale_boot.mjs`, and the module that reads the name is `status_policy.mjs`. */
export function brandModule(config, strings) {
  const brand = {
    name: config.name ?? PRODUCT.name,
    named: config.name !== null,
    tabTitle: config.tabTitle ?? PRODUCT.tabTitle,
    mark: markPaths(config).pageMark,
    // ONLYOFFICE `customization.logo.url`: the product mark links back to the
    // host's own product. `null` — the default — is a mark that is not a link,
    // which is what it has always been.
    markHref: config.markHref,
    // Their `customization.customer` and `customization.feedback`, and the
    // destination half of their `customization.help`. All three are host DATA in
    // the host's own language, which is why they are values here rather than
    // catalogue keys: the editor renders what it is given and translates none of
    // it (`docs/124` — a string the host wrote is not a string we route).
    customer: config.customer,
    feedback: config.feedback,
    help: config.help,
    themed:
      Object.keys(config.theme.tokens).length +
        Object.keys(config.theme.light).length +
        Object.keys(config.theme.dark).length >
      0,
    accentPinned: Boolean(config.theme.tokens["--accent"]),
  };
  return `// ${BANNER("this file")}
//
// The white-label identity, as data. \`brand.json\` is the source; \`brand.css\` is
// the other half of the same generation and carries the palette, the mark and the
// marker \`applySettings()\` reads.
export const BRAND = Object.freeze(${JSON.stringify(brand, null, 2)});

/** Host string overrides, by locale tag, with \`*\` meaning every language.
 *
 *  Installed through \`setOverrides\` in \`i18n.mjs\` — the SAME lookup the
 *  nineteen catalogues resolve through, one layer earlier. There is deliberately
 *  no second string mechanism for hosts (\`docs/126\` phase 3). */
export const BRAND_STRINGS = Object.freeze(${JSON.stringify(strings, null, 2)});
`;
}

/** Host text going into generated MARKUP. Every value here came out of a
 *  `brand.json` somebody else wrote, so it is untrusted input like any other:
 *  `normalize` refuses an executable scheme in a link, and this refuses the
 *  characters that would end an attribute or open a tag. Two layers, because a
 *  white-label seam that could inject markup into the editor's own origin would
 *  be a hole opened by configuration. */
const escapeHtml = (text) =>
  String(text)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");

/** The generated regions in `editor.html`, by marker name.
 *
 *  Two of them, and both are head-or-brand markup a host cannot otherwise reach
 *  from configuration: the icons (a favicon is a file reference, not a token) and
 *  the brand element (absent entirely when a host asked for no product mark, so
 *  "no name and no mark anywhere" is expressible rather than approximated by an
 *  invisible image). */
export function editorRegions(config, marks) {
  let icons;
  if (!marks.showMark) {
    icons = ['    <!-- No favicon: brand.json set "mark": false. -->'];
  } else if (config.mark === null) {
    // The product's own pair, including the raster touch icon, which only exists
    // for our mark; a host who supplies an SVG gets the one link and adds their
    // own raster by naming it — stated in the docs rather than guessed at.
    icons = [
      `    <link rel="icon" href="${PRODUCT.mark}" type="image/svg+xml" />`,
      `    <link rel="apple-touch-icon" href="${PRODUCT.touchIcon}" />`,
    ];
  } else {
    icons = [`    <link rel="icon" href="${marks.pageMark}" />`];
  }
  let brand;
  if (!marks.showMark) {
    brand = ['      <!-- No product mark: brand.json set "mark": false. -->'];
  } else if (config.markHref) {
    // ONLYOFFICE `customization.logo.url`. The mark itself stays `aria-hidden` —
    // it is decoration painted from a CSS token — so the anchor carries the name
    // in text: an `aria-hidden` wrapper around a focusable link is the axe
    // `aria-hidden-focus` violation `SKILL` §10 forbids outright, and a link
    // announced as "link" with no destination is the same failure by hand.
    const name = escapeHtml(config.name ?? PRODUCT.name);
    brand = [
      `      <a class="brand-logo-link" href="${escapeHtml(config.markHref)}" rel="noreferrer noopener" target="_blank">`,
      '        <span class="brand-logo" aria-hidden="true"></span>',
      `        <span class="sr-only">${name}</span>`,
      "      </a>",
    ];
  } else {
    brand = ['      <span class="brand-logo" aria-hidden="true"></span>'];
  }
  return new Map([
    ["brand-icons", icons.join("\n")],
    ["brand-mark", brand.join("\n")],
  ]);
}

/** Replaces every `<!-- @generated NAME -->…<!-- @end NAME -->` region, and
 *  fails when a declared region is missing from the file — both directions, the
 *  same rule `build-embed-docs.mjs` holds. */
export function applyRegions(source, regions) {
  let out = source;
  const missing = [];
  for (const [name, body] of regions) {
    const pattern = new RegExp(
      `([ \\t]*<!-- @generated ${name} -->\\n)[\\s\\S]*?([ \\t]*<!-- @end ${name} -->)`,
    );
    if (!pattern.test(out)) {
      missing.push(name);
      continue;
    }
    out = out.replace(pattern, (whole, open, close) => `${open}${body}\n${close}`);
  }
  if (missing.length) {
    throw new Error(
      `editor.html is missing generated region(s): ${missing.join(", ")}. A generator that ` +
        "silently writes nothing is a generator whose output nobody notices is stale.",
    );
  }
  return out;
}

/** Everything this generator owns, as `{path, content}`. */
export function artifacts({ configPath = join(WEBAPP, "brand.json") } = {}) {
  const raw = JSON.parse(readFileSync(configPath, "utf8"));
  const { themes } = readPalettes();
  const config = normalize(raw, themes);
  const { failures } = auditBrand(config, themes);
  if (failures.length) throw new BrandRefusal(failures);

  const marks = markPaths(config);
  const catalogues = readCatalogues();
  const strings = deriveStrings(config, catalogues);

  const editor = applyRegions(
    readFileSync(join(WEBAPP, "editor.html"), "utf8"),
    editorRegions(config, marks),
  );
  return [
    { path: join(WEBAPP, "src", "brand.css"), content: brandCss(config, marks) },
    { path: join(WEBAPP, "src", "brand.mjs"), content: brandModule(config, strings) },
    { path: join(WEBAPP, "editor.html"), content: editor },
  ];
}

/** The nineteen committed catalogues, by tag. */
export function readCatalogues() {
  const dir = join(WEBAPP, "locales");
  const out = new Map();
  for (const file of readdirSync(dir).sort()) {
    if (!file.endsWith(".json")) continue;
    out.set(file.replace(/\.json$/, ""), JSON.parse(readFileSync(join(dir, file), "utf8")));
  }
  return out;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const check = process.argv.includes("--check");
  // `--config <path>` names a different configuration, and it exists because of a
  // MUTATION THAT DID NOT GO RED. Deleting the `throw new BrandRefusal(failures)`
  // below left every assertion in `brand.test.mjs` green, because the test called
  // `auditBrand` itself: it proved the audit could see a failing palette and never
  // proved the generator refuses one. That is exactly phase 2's finding — a door
  // standing open with a lock behind it — so the guard now drives this command line
  // with a failing palette and asserts the exit status, which is the only thing
  // that can tell "refuses" from "computes a refusal and carries on".
  const configArg = process.argv.indexOf("--config");
  const configPath = configArg > 0 ? process.argv[configArg + 1] : undefined;
  // A `--config` run is a check and never a write: it must not be able to
  // overwrite a deployment's committed artifacts with somebody's experiment.
  if (configPath && !check) {
    console.error("build-brand: --config is only valid with --check.");
    process.exit(2);
  }
  let produced;
  try {
    produced = artifacts(configPath ? { configPath } : undefined);
  } catch (err) {
    console.error(err instanceof BrandRefusal ? err.message : err);
    process.exit(1);
  }
  const stale = produced.filter((file) => {
    let current = null;
    try {
      current = readFileSync(file.path, "utf8");
    } catch {
      current = null;
    }
    return current !== file.content;
  });
  if (configPath) {
    // Nothing is compared: a foreign configuration produces different artifacts by
    // definition, and the only question asked of it is whether it was ACCEPTED.
    console.log(`build-brand --check --config ${configPath}: accepted.`);
  } else if (check) {
    if (stale.length) {
      console.error(
        `build-brand --check: ${stale.length} generated file(s) are stale:\n` +
          stale.map((f) => `  ${relative(REPO, f.path)}`).join("\n") +
          "\nRun `node webapp/tools/build-brand.mjs` and commit the result.",
      );
      process.exit(1);
    }
    console.log(`build-brand --check: ${produced.length} file(s) up to date.`);
  } else {
    for (const file of produced) {
      writeFileSync(file.path, file.content);
      console.log(`build-brand: wrote ${relative(REPO, file.path)}`);
    }
  }
}
