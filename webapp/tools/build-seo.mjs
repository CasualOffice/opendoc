#!/usr/bin/env node
// Generates the site's two crawler manifests — `sitemap.xml` for search engines
// and `llms.txt` for language-model crawlers — from the pages `build-site.py`
// actually builds.
//
// Both files were hand-written. Nothing produced them, so a new page was
// invisible to search until somebody remembered to edit two files, and the repo's
// own rule (`docs/99` §9.1) is that a published artifact is GENERATED or it is
// not published. The fix is not "add the missing page", it is "make forgetting
// impossible": the page set is discovered, every field is copied from the page's
// own head, and `--check` fails the build when the committed files drift.
//
// Everything in here is derived from a committed artifact:
//
//   * the origin comes from `CNAME`, the file that actually decides the host, so
//     the manifests cannot name a domain the site is not served from;
//   * each URL is the page's own `<link rel="canonical">`, so the sitemap cannot
//     disagree with the page about where the page lives;
//   * each `llms.txt` entry's title and summary are the page's own `<title>` and
//     `<meta name="description">`, so a page's description is written once.
//
// WHAT IS DELIBERATELY NOT PUBLISHED, and why — a sitemap may carry `lastmod`,
// `changefreq` and `priority`, and this one carries none of them:
//
//   * `changefreq` and `priority` are documented by Google as ignored. Publishing
//     a number a consumer discards is noise that reads like information.
//   * `lastmod` cannot be derived here, so it cannot honestly be published. The
//     obvious source is git, and git is not available: `.github/workflows/pages.yml`
//     builds with `actions/checkout@v4` at its default depth of 1, so the deploy
//     has no history to read a date from. Worse, it is self-referential — the
//     generator runs BEFORE the commit that changes a page exists, so `--check`
//     would fail in CI on every page edit, the date it wanted being the date of
//     the commit under construction. The committed file previously claimed
//     `2026-08-06` for four of the five pages, every one of which had been edited
//     many times since, which is the same defect as an unverifiable number on the
//     fidelity page. An absent `lastmod` costs a weak crawl hint; a wrong one
//     teaches Google to ignore the element for the whole site.
//
// `editor.html` is NOT in the sitemap. See EXCLUDED below.
import { readFileSync, readdirSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");

/** Pages that link the site stylesheet but are deliberately kept out of the
 *  crawler manifests, each with the reason and the measurement behind it.
 *
 *  A named exclusion has to live somewhere; it lives here, in one place, with an
 *  argument, and `tests/seo.test.mjs` asserts the exclusion actually holds so it
 *  cannot be quietly reversed by an edit elsewhere. */
export const EXCLUDED = [
  {
    page: "editor.html",
    reason:
      "The application, not content. Measured: its crawler-visible text is 1,357 " +
      "words of application chrome — menu names, dialog captions and 'Not set' " +
      "repeated a dozen times — with no prose describing what OpenDoc is, because " +
      "everything a reader would want is in a document the crawler never opens. " +
      "Against that it is 217 KB of markup which declares `<link rel=preload " +
      "as=fetch>` for a 21 MB WebAssembly binary a rendering crawler will fetch. " +
      "A sitemap is a statement that these pages are worth crawling; this one " +
      "spends the crawl budget of the whole site on a page with nothing to rank " +
      "for. It stays crawlable and keeps its canonical — excluding it from the " +
      "sitemap is reversible and evidence-backed, whereas `noindex` would remove " +
      "the live editor from search entirely and is the owner's call, not this " +
      "generator's.",
  },
  {
    page: "embed.html",
    reason:
      "A host-page demo that already declares `<meta name=\"robots\" " +
      "content=\"noindex\">`. Listing a noindex page in a sitemap is a direct " +
      "contradiction, and Search Console reports it as one.",
  },
];

const EXCLUDED_PAGES = new Set(EXCLUDED.map((entry) => entry.page));

/** `https://<host>`, from the file that decides the host. */
export function origin() {
  const host = readFileSync(join(WEBAPP, "CNAME"), "utf8").trim();
  if (!/^[a-z0-9.-]+\.[a-z]{2,}$/.test(host)) {
    throw new Error(`CNAME does not hold a hostname: ${JSON.stringify(host)}`);
  }
  return `https://${host}`;
}

/** Every `<name …>` tag in a source, as attribute maps.
 *
 *  QUOTE-AWARE, and not optional: `embedding.page.html`'s own description reads
 *  "…the <opendoc-editor> custom element…", so an unescaped `>` sits inside an
 *  attribute value. The first draft matched `<meta\s[^>]*>` and stopped at that
 *  `>`, lost the `content`, and reported the page as having no description at all.
 *  A tag ends at the first `>` that is NOT inside quotes. */
export function tags(source, name) {
  const found = [];
  const pattern = new RegExp(`<${name}\\b((?:[^>"']|"[^"]*"|'[^']*')*)>`, "gs");
  for (const match of source.matchAll(pattern)) {
    const attributes = {};
    for (const pair of match[1].matchAll(/([\w:.-]+)\s*=\s*"([^"]*)"/gs)) {
      attributes[pair[1]] = pair[2].replace(/\s+/g, " ").trim();
    }
    found.push(attributes);
  }
  return found;
}

const metaContent = (source, key, value) =>
  tags(source, "meta").find((tag) => tag[key] === value)?.content;

/** What the manifests need to know about one page, all of it from the page. */
export function describePage(file, source) {
  return {
    file,
    canonical: tags(source, "link").find((tag) => tag.rel === "canonical")?.href,
    title: source.match(/<title[^>]*>([^<]*)<\/title>/)?.[1]?.trim(),
    description: metaContent(source, "name", "description"),
    robots: metaContent(source, "name", "robots") ?? "",
  };
}

/** Every page in the crawler manifests: what `build-site.py` builds, minus the
 *  argued exclusions, minus anything that declares `noindex` about itself.
 *
 *  Read from the TEMPLATES rather than the generated pages, so this does not
 *  depend on having run `build-site.py` first, and so a head edit reaches the
 *  manifests in the same pass that reaches the page. */
export function sitePages() {
  const pages = readdirSync(WEBAPP)
    .filter((name) => name.endsWith(".page.html"))
    .sort()
    .map((template) =>
      describePage(
        template.replace(/\.page\.html$/, ".html"),
        readFileSync(join(WEBAPP, template), "utf8"),
      ),
    );
  for (const page of pages) {
    for (const field of ["canonical", "title", "description"]) {
      if (!page[field]) {
        throw new Error(
          `${page.file} has no ${field}. Every page in the sitemap must say what it ` +
            `is and where it lives; add it rather than letting this page be published ` +
            `without it.`,
        );
      }
    }
    if (!page.canonical.startsWith(origin())) {
      throw new Error(
        `${page.file}'s canonical (${page.canonical}) is not on ${origin()}, the ` +
          `origin CNAME declares`,
      );
    }
  }
  // The home page first, then the rest by filename. Deterministic either way, but
  // both manifests are read top-down by something deciding what to look at first —
  // a crawler prioritising URLs, a model reading `llms.txt` — and the entry point
  // belongs at the top rather than wherever `index` sorts.
  const home = `${origin()}/`;
  return pages
    .filter((page) => !EXCLUDED_PAGES.has(page.file) && !/\bnoindex\b/.test(page.robots))
    .sort((a, b) => (a.canonical === home ? -1 : b.canonical === home ? 1 : 0));
}

const escapeXml = (value) =>
  value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

export function renderSitemap(pages) {
  const urls = pages
    .map((page) => `  <url>\n    <loc>${escapeXml(page.canonical)}</loc>\n  </url>`)
    .join("\n");
  return [
    '<?xml version="1.0" encoding="UTF-8"?>',
    "<!-- Generated by webapp/tools/build-seo.mjs from the pages build-site.py builds.",
    "     Do not edit: ./tools/build-seo.mjs --check fails the build when this drifts.",
    "     No lastmod/changefreq/priority — see that file for why none of the three can",
    "     be published honestly from here. -->",
    '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">',
    urls,
    "</urlset>",
    "",
  ].join("\n");
}

/** `llms.txt` — the authored prose from `llms.page.txt`, with the site's own page
 *  list substituted at the `<!-- @pages -->` marker.
 *
 *  Only the page list is generated. The prose above it is the project's own
 *  summary of itself and is written by a person; what a generator is for is the
 *  part that goes stale, which is the list of pages and what each one says. Each
 *  entry's title and summary are the page's own, so a description is authored in
 *  exactly one place. */
export function renderLlms(pages, template) {
  const marker = "<!-- @pages -->";
  if (!template.includes(marker)) {
    throw new Error(`llms.page.txt has no ${marker} marker to fill`);
  }
  const list = pages
    .map((page) => `- [${page.title}](${page.canonical}): ${page.description}`)
    .join("\n");
  return template.replace(
    marker,
    `<!-- Generated from each page's own <title> and meta description by\n` +
      `     webapp/tools/build-seo.mjs. Edit the page, not this list. -->\n${list}`,
  );
}

function build(check) {
  const pages = sitePages();
  const artifacts = [
    ["sitemap.xml", renderSitemap(pages)],
    ["llms.txt", renderLlms(pages, readFileSync(join(WEBAPP, "llms.page.txt"), "utf8"))],
  ];
  const drift = [];
  for (const [name, rendered] of artifacts) {
    const path = join(WEBAPP, name);
    if (check) {
      let current = null;
      try {
        current = readFileSync(path, "utf8");
      } catch {
        current = null;
      }
      if (current !== rendered) drift.push(name);
    } else {
      writeFileSync(path, rendered, "utf8");
      console.log(`build-seo: wrote ${name} (${pages.length} page(s))`);
    }
  }
  if (check) {
    if (drift.length) {
      console.error(
        `build-seo --check: ${drift.join(" and ")} ${
          drift.length > 1 ? "are" : "is"
        } stale.\n` +
          "A page was added, renamed, or had its title/description/canonical edited " +
          "without regenerating the crawler manifests — which is exactly how a new " +
          "page stays invisible to search.\n" +
          "Run ./tools/build-seo.mjs and commit the result.",
      );
      return 1;
    }
    console.log(`build-seo --check: sitemap.xml and llms.txt up to date (${pages.length} page(s)).`);
  }
  return 0;
}

// Run as a script, imported by `tests/seo.test.mjs` as a module. `realpath` on
// both sides, because the test imports by a relative URL and the build invokes it
// by an absolute path.
if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exit(build(process.argv.includes("--check")));
}
