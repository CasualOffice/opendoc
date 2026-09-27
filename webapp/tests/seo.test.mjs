// Discoverability, guarded — because every tag on these pages is a published
// claim (`109` HF-191).
//
// Three separate things had no gate at all. A page could ship with no canonical,
// no description or a title indistinguishable from another page's, and nothing
// would say so. A new page could be added and stay out of `sitemap.xml` and
// `llms.txt` indefinitely, because both were hand-written. And structured data —
// which is prose a machine reads and a search engine repeats — had no check that
// any of its fields matched the page it described.
//
// That last one is the reason this file is strict. `docs/99` §9 exists because the
// public pages carried fabricated claims twice, in both directions, and a
// `datePublished` nobody derived or an `aggregateRating` nobody earned is the same
// defect in a format a reviewer does not read. So:
//
//   * every field in the structured data is re-read from the page it sits on and
//     must agree with it — `url` with the canonical, `name` with the `<title>`,
//     `description` with the meta description, `headline` with the `<h1>`,
//     `inLanguage` with `<html lang>`;
//   * a short list of fields is REFUSED outright, because nothing in this repo
//     could derive them honestly;
//   * every image URL a social card advertises must resolve to a file that is
//     either committed or copied in by `build.sh`;
//   * every `@id` a page points at must be a node some page actually defines.
//
// The pages are read as BUILT (`*.html`), not as templates, because what a crawler
// receives is the built page. `build-site.py --check` already guarantees the two
// agree.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const { EXCLUDED, describePage, origin, renderLlms, renderSitemap, sitePages, tags } =
  await import("../tools/build-seo.mjs");
const { scanMarkup } = await import("../tools/string_sites.mjs");

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (name) => readFileSync(join(WEBAPP, name), "utf8");

const ORIGIN = origin();
const PAGES = sitePages();
/** The built page for each entry, plus its head fields read back independently of
 *  the generator, so a bug in the generator cannot make both sides agree. */
const BUILT = PAGES.map((page) => {
  const source = read(page.file);
  return { ...describePage(page.file, source), source };
});

/** Structured-data nodes on a page, as one flat list. */
function nodes(source) {
  const blocks = [...source.matchAll(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g)];
  return blocks.flatMap((block) => {
    const parsed = JSON.parse(block[1]);
    return parsed["@graph"] ?? [parsed];
  });
}

/** Every `{ "@id": … }` reference in a value, at any depth. */
function references(value, found = []) {
  if (Array.isArray(value)) for (const item of value) references(item, found);
  else if (value && typeof value === "object") {
    const keys = Object.keys(value);
    if (keys.length === 1 && keys[0] === "@id") found.push(value["@id"]);
    else for (const key of keys) references(value[key], found);
  }
  return found;
}

/** Every key used anywhere in a node, at any depth. */
function keysOf(value, found = new Set()) {
  if (Array.isArray(value)) for (const item of value) keysOf(item, found);
  else if (value && typeof value === "object") {
    for (const key of Object.keys(value)) {
      found.add(key);
      keysOf(value[key], found);
    }
  }
  return found;
}

// ---- The head every indexable page must have -------------------------------

test("every indexable page says what it is, where it lives, and in what language", () => {
  assert.ok(BUILT.length >= 4, `only ${BUILT.length} indexable pages found`);
  for (const page of BUILT) {
    assert.match(page.source, /<html lang="en">/, `${page.file} declares no language`);
    assert.ok(page.canonical, `${page.file} has no rel=canonical`);
    assert.ok(page.title, `${page.file} has no <title>`);
    assert.ok(page.description, `${page.file} has no meta description`);
    assert.match(
      page.robots,
      /\bindex\b/,
      `${page.file} is in the sitemap but does not tell a crawler to index it`,
    );
    // The canonical is the page's own URL, not a neighbour's. A copy-pasted head
    // that points four pages at one canonical is the classic way a site
    // de-indexes three of them.
    const expected = page.file === "index.html" ? `${ORIGIN}/` : `${ORIGIN}/${page.file}`;
    assert.equal(page.canonical, expected, `${page.file}'s canonical points elsewhere`);
  }
});

test("titles and descriptions are distinct, and long enough to be a search result", () => {
  // MEASURED: titles 23-50 characters, descriptions 165-316. The bounds are
  // generous either side of that — this is here to catch an empty, duplicated or
  // placeholder value, not to police wording.
  //
  // `fidelity.html`'s 316 characters deliberately exceed the ~160 a search snippet
  // shows: the tail is the caveat that the oracle geometry gate is manual and
  // currently inert. Truncated in a snippet is better than absent from the page.
  const titles = new Map();
  const descriptions = new Map();
  for (const page of BUILT) {
    assert.ok(
      page.title.length >= 20 && page.title.length <= 70,
      `${page.file}'s title is ${page.title.length} characters: ${JSON.stringify(page.title)}`,
    );
    assert.ok(
      page.description.length >= 70 && page.description.length <= 340,
      `${page.file}'s description is ${page.description.length} characters`,
    );
    // "OpenDoc" alone is not a description of anything.
    assert.ok(
      page.description.split(/\s+/).length >= 12,
      `${page.file}'s description is not a sentence`,
    );
    for (const [seen, value] of [
      [titles, page.title],
      [descriptions, page.description],
    ]) {
      assert.ok(
        !seen.has(value),
        `${page.file} and ${seen.get(value)} publish the same text: ${JSON.stringify(
          value.slice(0, 60),
        )}`,
      );
      seen.set(value, page.file);
    }
  }
});

// ---- Structured data -------------------------------------------------------

test("every page carries structured data, and it is valid JSON", () => {
  for (const page of BUILT) {
    const graph = nodes(page.source); // throws on invalid JSON
    assert.ok(
      graph.length > 0,
      `${page.file} publishes no structured data — a crawler has to infer what the ` +
        `page is about from the prose alone`,
    );
    for (const node of graph) {
      assert.ok(node["@type"], `a node on ${page.file} has no @type`);
    }
  }
});

test("no structured-data field claims something this repository cannot derive", () => {
  // Each of these is a claim with no source in the repository. A date cannot be
  // derived: the Pages deploy checks out at depth 1, so there is no git history at
  // build time, and the value would in any case be the date of the commit being
  // written. Ratings and reviews do not exist at all. `docs/99` §9 does not stop
  // applying because the reader is a machine.
  const REFUSED = [
    "datePublished",
    "dateModified",
    "dateCreated",
    "aggregateRating",
    "ratingValue",
    "reviewCount",
    "review",
    "interactionCount",
    "interactionStatistic",
  ];
  for (const page of BUILT) {
    const used = keysOf(nodes(page.source));
    const claimed = REFUSED.filter((field) => used.has(field));
    assert.deepEqual(
      claimed,
      [],
      `${page.file} publishes ${claimed.join(", ")} in its structured data. Nothing in ` +
        `this repository derives that, so it is a fabricated claim in a format no ` +
        `reviewer reads. Remove it, or generate it from a committed artifact.`,
    );
  }
});

test("structured data agrees with the page it describes", () => {
  for (const page of BUILT) {
    const h1 = page.source
      .match(/<h1[^>]*>([\s\S]*?)<\/h1>/)?.[1]
      .replace(/<[^>]+>/g, "")
      .replace(/\s+/g, " ")
      .trim();
    for (const node of nodes(page.source)) {
      // Only the node that IS this page. A WebSite or Organization node describes
      // the site, and a WebApplication node describes the editor, so their `url`
      // is legitimately not this page's.
      if (node["@id"] !== `${page.canonical}#webpage`) continue;
      assert.equal(node.url, page.canonical, `${page.file}: schema url is not the canonical`);
      assert.equal(node.name, page.title, `${page.file}: schema name is not the <title>`);
      assert.equal(
        node.description,
        page.description,
        `${page.file}: schema description is not the meta description`,
      );
      assert.equal(node.inLanguage, "en", `${page.file}: schema language is not <html lang>`);
      if (node.headline !== undefined) {
        assert.equal(node.headline, h1, `${page.file}: schema headline is not the <h1>`);
      }
    }
  }
});

test("every @id a page points at is a node the site defines", () => {
  const defined = new Set();
  for (const page of BUILT) {
    for (const node of nodes(page.source)) if (node["@id"]) defined.add(node["@id"]);
  }
  assert.ok(defined.size >= 6, `only ${defined.size} identified nodes across the site`);
  const dangling = [];
  for (const page of BUILT) {
    for (const id of references(nodes(page.source))) {
      if (!defined.has(id)) dangling.push(`${page.file} → ${id}`);
    }
  }
  assert.deepEqual(
    dangling,
    [],
    "a structured-data reference resolves to nothing, so the node it claims to " +
      "describe does not exist anywhere on the site",
  );
});

test("every breadcrumb is a real path through real pages", () => {
  const urls = new Set(BUILT.map((page) => page.canonical));
  let found = 0;
  for (const page of BUILT) {
    for (const node of nodes(page.source)) {
      if (node["@type"] !== "BreadcrumbList") continue;
      found += 1;
      const items = node.itemListElement;
      assert.ok(items.length >= 2, `${page.file}'s breadcrumb has ${items.length} item(s)`);
      items.forEach((item, index) => {
        assert.equal(item.position, index + 1, `${page.file}'s breadcrumb skips a position`);
        assert.ok(
          urls.has(item.item),
          `${page.file}'s breadcrumb points at ${item.item}, which is not a page of this site`,
        );
        assert.ok(item.name, `${page.file}'s breadcrumb has an unnamed step`);
      });
      // The trail ends at the page itself, and starts at the home page.
      assert.equal(items[0].item, `${ORIGIN}/`, `${page.file}'s breadcrumb does not start at home`);
      assert.equal(
        items.at(-1).item,
        page.canonical,
        `${page.file}'s breadcrumb does not end at ${page.file}`,
      );
    }
  }
  assert.ok(found >= 3, `only ${found} breadcrumb trails — the docs pages each need one`);
});

// ---- Social cards ----------------------------------------------------------

test("every page a link preview can reach advertises an image that exists", () => {
  // An `og:image` pointing at a file that is not there is the same defect as an
  // unverifiable number: the claim is checkable and wrong. `webapp/assets/` is
  // gitignored build output, so a path counts as real if the file is committed OR
  // `build.sh` copies it in — read from `build.sh` rather than listed here.
  const buildScript = read("build.sh");
  let checked = 0;
  for (const page of BUILT) {
    const meta = tags(page.source, "meta");
    const images = meta
      .filter((tag) => tag.property === "og:image" || tag.name === "twitter:image")
      .map((tag) => tag.content);
    assert.ok(images.length >= 1, `${page.file} has no social card image`);
    for (const url of images) {
      assert.ok(url.startsWith(`${ORIGIN}/`), `${page.file}'s card image is off-origin: ${url}`);
      const path = url.slice(`${ORIGIN}/`.length);
      const committed = existsSync(join(WEBAPP, path));
      const copiedIn = buildScript.includes(`"$here/${path}"`);
      assert.ok(
        committed || copiedIn,
        `${page.file} advertises ${url}, but webapp/${path} is neither committed nor ` +
          `copied in by build.sh — a link preview would show a broken image`,
      );
      checked += 1;
    }
    // A card with a title and no description is a worse preview than none.
    for (const key of ["og:title", "og:description", "og:url", "twitter:card"]) {
      assert.ok(
        meta.some((tag) => tag.property === key || tag.name === key),
        `${page.file} has no ${key}`,
      );
    }
  }
  assert.ok(checked >= 8, `only ${checked} card images checked`);
});

// ---- The crawler manifests -------------------------------------------------

test("the crawler manifests are what the generator produces", () => {
  // The same check `build.sh` runs, repeated here so `npm run test:unit` catches a
  // stale sitemap without a full wasm build.
  assert.equal(
    read("sitemap.xml"),
    renderSitemap(PAGES),
    "sitemap.xml is not what webapp/tools/build-seo.mjs produces — run it and commit",
  );
  assert.equal(
    read("llms.txt"),
    renderLlms(PAGES, read("llms.page.txt")),
    "llms.txt is not what webapp/tools/build-seo.mjs produces — run it and commit",
  );
});

test("every indexable page is in the sitemap and llms.txt, and nothing else is", () => {
  const sitemap = [...read("sitemap.xml").matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => m[1]);
  const llms = read("llms.txt");
  const expected = PAGES.map((page) => page.canonical).sort();
  assert.deepEqual([...sitemap].sort(), expected, "the sitemap is not the set of indexable pages");
  for (const page of PAGES) {
    assert.ok(
      llms.includes(`(${page.canonical})`),
      `${page.file} is missing from llms.txt, so a model crawler has no entry for it`,
    );
    assert.ok(
      llms.includes(page.description),
      `${page.file}'s description in llms.txt is not the page's own`,
    );
  }
});

test("the pages kept out of the sitemap are kept out, each for an argued reason", () => {
  // The mirror of the allowlist rule in `no_unrouted_strings.test.mjs`: an
  // exclusion is a claim that listing the page would be WRONG, so it carries a
  // reason, the list stays small, and the exclusion is asserted to hold — a
  // deliberate omission that nothing checks is indistinguishable from an oversight.
  const sitemap = read("sitemap.xml");
  const llms = read("llms.txt");
  assert.ok(EXCLUDED.length <= 4, `${EXCLUDED.length} exclusions is not a short list`);
  for (const entry of EXCLUDED) {
    assert.ok(
      entry.reason.length > 120,
      `${entry.page} is excluded with no argued reason — say why listing it would be wrong`,
    );
    assert.ok(
      !sitemap.includes(`/${entry.page}<`),
      `${entry.page} is in the sitemap despite being excluded: ${entry.reason.slice(0, 80)}…`,
    );
    assert.ok(
      !llms.includes(`(${ORIGIN}/${entry.page})`),
      `${entry.page} is listed as a site page in llms.txt despite being excluded`,
    );
  }
  // Named, because this one is a judgement rather than a consequence of a rule:
  // `editor.html` is a WebAssembly application, and the sitemap is a statement
  // about content. If it is ever put back, that has to be a decision.
  assert.ok(
    EXCLUDED.some((entry) => entry.page === "editor.html"),
    "editor.html's exclusion is the one this work decided; keep it argued in one place",
  );
});

test("robots.txt allows the crawl and points at the sitemap on this origin", () => {
  const robots = read("robots.txt");
  assert.match(robots, /^User-agent: \*$/m);
  assert.match(robots, /^Allow: \/$/m);
  assert.ok(
    robots.includes(`Sitemap: ${ORIGIN}/sitemap.xml`),
    `robots.txt does not point at ${ORIGIN}/sitemap.xml — a sitemap nothing announces ` +
      `is found only if it is submitted by hand`,
  );
});

// ---- Where this meets the string ratchet ------------------------------------

test("page metadata is outside the i18n seam, by measurement and on purpose", () => {
  // `no_unrouted_strings.test.mjs` sets every page ceiling at its exact measured
  // count, so anything the scanner counts fails the ratchet on arrival. This work
  // adds structured data and social cards, both English prose, so the question had
  // to be settled rather than assumed.
  //
  // IT WAS ASSUMED FIRST, AND THE ASSUMPTION WAS WRONG, which is the part worth
  // keeping. A check of "does `scanMarkup` read `ld+json` bodies?" used a JSON block
  // with no tag-shaped text in it and answered no. It does read them: the scanner
  // skips the `<script>` OPENING TAG by name and then walks into the body, because
  // it captures the text after each tag up to the next `<` — and the embedding
  // guide's own description contains "…the `<opendoc-editor>` custom element…", which
  // matches as an element. Adding the JSON-LD put `embedding.page.html` one site over
  // its ceiling and the ratchet refused the commit. `string_sites.mjs` now exempts
  // script and style BODIES structurally, which is a bug fix as much as an exemption,
  // and `no_unrouted_strings.test.mjs` publishes how many sites that suppresses.
  //
  // Where that leaves the three ways out: no site-i18n project, no raised ceiling,
  // and the exemption is structural with a measured bound rather than an allowlist.
  const jsonld =
    '<script type="application/ld+json">\n' +
    '{ "description": "the <opendoc-editor> custom element, and a contract." }\n' +
    "</script>";
  assert.deepEqual(scanMarkup(jsonld), [], "ld+json bodies must not be counted as prose");
  assert.equal(
    scanMarkup(jsonld, { exemptCode: false }).length,
    1,
    "the exemption must be doing real work here — if this is 0 the regression it " +
      "exists for is not being reproduced, and the test above proves nothing",
  );
  // `<meta … content="…">` is not counted: `content` is not one of the scanner's
  // HUMAN_ATTRIBUTES. `<title>` IS counted, as a text node, and always has been —
  // one site per page, inside the existing ceilings.
  assert.deepEqual(scanMarkup('<meta name="description" content="A whole readable sentence." />'), []);
  assert.deepEqual(scanMarkup('<meta property="og:title" content="A readable card title" />'), []);
  assert.equal(scanMarkup("<title>A readable page title</title>").length, 1);

  // MEASURED: human-readable metadata strings the i18n seam does not reach — every
  // meta description/og/twitter value, plus every schema `name`/`headline`/
  // `description` and breadcrumb step name, across the indexable pages.
  //
  // Not a ratchet, a published number: it makes the debt visible so the site-i18n
  // decision is taken with it in hand, and it moves only deliberately. 46, and the
  // first attempt at it was 62 — this assertion rejected that, which is the same
  // lesson the string ceilings carry: a number here is a measurement, never a
  // calculation.
  const READABLE_META = [
    "description",
    "og:title",
    "og:description",
    "og:image:alt",
    "twitter:title",
    "twitter:description",
  ];
  let strings = 0;
  for (const page of BUILT) {
    strings += tags(page.source, "meta").filter(
      (tag) => READABLE_META.includes(tag.name) || READABLE_META.includes(tag.property),
    ).length;
    for (const node of nodes(page.source)) {
      for (const field of ["name", "headline", "description"]) {
        if (typeof node[field] === "string") strings += 1;
      }
      for (const item of node.itemListElement ?? []) if (item.name) strings += 1;
    }
  }
  assert.equal(
    strings,
    46,
    `the site publishes ${strings} human-readable metadata strings outside the i18n ` +
      `seam (was 46). That is not a failure — it is the number, and it moved. Update ` +
      `it deliberately, and note that localising these needs per-language pages with ` +
      `hreflang, which does not exist yet.`,
  );
});
