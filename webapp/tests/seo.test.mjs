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

const {
  EXCLUDED,
  describePage,
  origin,
  renderLlms,
  renderLlmsFull,
  renderSitemap,
  selectPages,
  sitePages,
  tags,
} = await import("../tools/build-seo.mjs");
const { scanMarkup } = await import("../tools/string_sites.mjs");
const { DESCRIPTION_MAX } = await import("../tools/build-doc-pages.mjs");

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

test("titles and descriptions are distinct, and fit a search result", () => {
  // The LOWER bounds catch an empty, duplicated or placeholder value. The UPPER
  // bound on the description is the one that changed (`109` HF-292): it was 340,
  // set generously above the measured 165-316, and so it guarded nothing — eight
  // of the eighteen pages published descriptions a search result cuts off, the
  // fidelity page at 316 with its caveat in the half nobody sees. A caveat that is
  // truncated away is not a caveat, so the rule is now that the description FITS,
  // and the caveat is written inside the bound. 160 is `DESCRIPTION_MAX`, the same
  // number `build-doc-pages.mjs` enforces where reference descriptions are written.
  const titles = new Map();
  const descriptions = new Map();
  for (const page of BUILT) {
    assert.ok(
      page.title.length >= 20 && page.title.length <= 70,
      `${page.file}'s title is ${page.title.length} characters: ${JSON.stringify(page.title)}`,
    );
    assert.ok(
      page.description.length >= 70 && page.description.length <= DESCRIPTION_MAX,
      `${page.file}'s description is ${page.description.length} characters; a search result ` +
        `shows about ${DESCRIPTION_MAX}, so the rest is cut off: ${JSON.stringify(page.description)}`,
    );
    // The card copies of it may run a little longer — a preview shows two or three
    // lines — but not unboundedly.
    for (const tag of tags(page.source, "meta")) {
      if (tag.property === "og:description" || tag.name === "twitter:description") {
        assert.ok(
          tag.content.length <= 200,
          `${page.file}'s ${tag.property ?? tag.name} is ${tag.content.length} characters`,
        );
      }
    }
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
  assert.equal(
    read("llms-full.txt"),
    renderLlmsFull(read("llms.page.txt")),
    "llms-full.txt is not what webapp/tools/build-seo.mjs produces — run it and commit",
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
  //
  // 46 → 201 when the reference pages landed (`109` HF-192): the site went from
  // four indexable pages to sixteen, and twelve of them are the repository's own
  // design documents published as pages instead of as links to raw Markdown on
  // github.com. Two things are worth saying about the jump rather than letting the
  // number speak for itself.
  //
  // It is DERIVED, not authored. Every one of the 155 new strings is a reference
  // page's `<title>`, its meta description, or a copy of one of those inside its
  // social card and its JSON-LD — and the title and the description are read out
  // of the source `.md` by `tools/build-doc-pages.mjs` (the heading, and the first
  // prose paragraphs). Nobody typed any of them, and nobody can: the page is
  // regenerated and `--check` fails the build if it is edited. So this is not 155
  // new sentences somebody has to translate one day; it is twelve documents'
  // headings and opening paragraphs, counted once per place they are published.
  //
  // And it does not change the site-i18n position. Localising these still needs
  // per-language pages with `hreflang`, which does not exist — and for these
  // pages it would additionally need translated design documents, which is a
  // different project from translating a product surface.
  //
  // MEASURED by running this test, not by arithmetic: 46 + 12 × 13 would give 202.
  //
  // 201 → 212 with the configuration playground (`docs/126`): one indexable page,
  // carrying the six readable meta values above, its schema `name`, `headline` and
  // `description`, and a three-step breadcrumb whose middle step names the
  // embedding guide. MEASURED, and the arithmetic would have been wrong in the
  // usual direction — six plus three is nine, and the breadcrumb is the other
  // three.
  //
  // 212 → 225 with `docs/162`, the deployment guide (`109` RM-11), published as a
  // reference page. MEASURED by running this test: one more indexable page
  // carrying the six readable meta values above, its schema `name`, `headline`
  // and `description`, and a four-step breadcrumb. The arithmetic would again
  // have been wrong in the usual direction — six plus three is nine, and the
  // breadcrumb is the other four.
  //
  // 225 -> 246 with the social cards and the description pass (`109` HF-292).
  // MEASURED by running this test (a guess of 284 was wrong, as the first guess
  // here always is). `twitter:image:alt` joined the list, because it is a sentence
  // a screen reader announces exactly as `og:image:alt` is: eighteen pages each
  // gained one, and three root pages (docs, embedding, playground) gained the
  // `og:image:alt` they never had — 18 + 3 = 21. Every one of the
  // new strings is a copy of one of the five cards' alt text, which is written
  // once, in `tools/build-social-cards.mjs`.
  //
  // 246 -> 273 with the SDK (`docs/176` §12): two new indexable pages, `sdk.html`
  // and `reference/three-sdk-products.html`, each with the seven readable meta
  // values, its schema `name`, `headline` and `description`, and a three- or
  // four-step breadcrumb. MEASURED by running this test.
  const READABLE_META = [
    "description",
    "og:title",
    "og:description",
    "og:image:alt",
    "twitter:title",
    "twitter:description",
    "twitter:image:alt",
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
    273,
    `the site publishes ${strings} human-readable metadata strings outside the i18n ` +
      `seam (was 273). That is not a failure — it is the number, and it moved. Update ` +
      `it deliberately, and note that localising these needs per-language pages with ` +
      `hreflang, which does not exist yet.`,
  );
});

// ---- The link-preview cards (`109` HF-292) ---------------------------------

const { CARDS, CARD_HEIGHT, CARD_WIDTH, ICONS, pngSize, problems: cardProblems } = await import(
  "../tools/build-social-cards.mjs"
);

/** The pages a link preview can be made from: every indexable page, plus the
 *  editor — kept out of the sitemap, but the URL people actually paste. */
const SHAREABLE = [...BUILT, { file: "editor.html", source: read("editor.html") }];

const metaValue = (source, key) =>
  tags(source, "meta").find((tag) => tag.property === key || tag.name === key)?.content;

test("every shareable page advertises a generated card, with its true size and its alt", () => {
  const byUrl = new Map(CARDS.map((card) => [`${ORIGIN}/${card.file}`, card]));
  for (const page of SHAREABLE) {
    const image = metaValue(page.source, "og:image");
    const card = byUrl.get(image);
    assert.ok(
      card,
      `${page.file}'s og:image is ${image}, which is not one of the generated 1200x630 cards ` +
        `(tools/build-social-cards.mjs). A screenshot is cropped differently by every consumer.`,
    );
    // The size a page announces is read back from the PNG's own header, so a
    // page cannot claim 1200x630 for an image that is not.
    const size = pngSize(readFileSync(join(WEBAPP, card.file)));
    assert.deepEqual(size, { width: CARD_WIDTH, height: CARD_HEIGHT }, `${card.file} is not 1200x630`);
    assert.equal(metaValue(page.source, "og:image:width"), String(size.width), `${page.file}: og:image:width`);
    assert.equal(metaValue(page.source, "og:image:height"), String(size.height), `${page.file}: og:image:height`);
    assert.equal(metaValue(page.source, "og:image:type"), "image/png", `${page.file}: og:image:type`);
    assert.equal(metaValue(page.source, "og:locale"), "en_US", `${page.file}: og:locale`);
    assert.equal(metaValue(page.source, "twitter:image"), image, `${page.file}: twitter:image is another image`);
    // The alt is the card's own words, written once in the generator.
    assert.equal(metaValue(page.source, "og:image:alt"), card.alt, `${page.file}: og:image:alt is not the card's`);
    assert.equal(
      metaValue(page.source, "twitter:image:alt"),
      card.alt,
      `${page.file}: twitter:image:alt is not the card's`,
    );
    for (const key of ["og:title", "og:description", "og:url", "twitter:card", "twitter:title"]) {
      assert.ok(metaValue(page.source, key), `${page.file} has no ${key}`);
    }
  }
});

test("the committed cards and icons are what their committed sources render", () => {
  // The same check `build.sh` runs, in the unit lane so a stale card fails without
  // a wasm build.
  assert.deepEqual(cardProblems(), [], "run `node tools/build-social-cards.mjs --render` and commit");
});

test("a card says nothing a page does not, and carries no number", () => {
  // A picture is a claim no reviewer reads closely, so no figure may be smuggled
  // into one: a number on a card has no `data-claim` and no derivation.
  for (const card of CARDS) {
    const words = [card.kicker, ...card.title, ...card.subtitle].join(" ");
    assert.doesNotMatch(words, /\d/, `${card.file} carries a number: ${words}`);
  }
});

// ---- The editor's own head ------------------------------------------------

test("the editor's head is shareable, indexable, and stays out of the sitemap", () => {
  const editor = describePage("editor.html", read("editor.html"));
  const source = read("editor.html");
  assert.equal(editor.canonical, `${ORIGIN}/editor.html`);
  assert.doesNotMatch(editor.robots, /noindex/, "the editor must not be noindexed — that is the owner's call");
  assert.ok(!read("sitemap.xml").includes(`${ORIGIN}/editor.html<`), "editor.html is in the sitemap");
  assert.ok(editor.description.length >= 70 && editor.description.length <= DESCRIPTION_MAX);
  const themes = tags(source, "meta").filter((tag) => tag.name === "theme-color");
  assert.deepEqual(
    themes.map((tag) => tag.media).sort(),
    ["(prefers-color-scheme: dark)", "(prefers-color-scheme: light)"],
    "the editor needs a theme-color for each colour scheme",
  );
  // Its structured data: valid, nothing refused, its page node agreeing with its
  // head, and the application node IDENTICAL to the one the home page defines —
  // one @id described two ways is two claims about one thing.
  const graph = nodes(source);
  const page = graph.find((node) => node["@id"] === `${ORIGIN}/editor.html#webpage`);
  assert.equal(page?.name, editor.title, "editor schema name is not its <title>");
  assert.equal(page?.description, editor.description, "editor schema description is not its meta description");
  const app = graph.find((node) => node["@id"] === `${ORIGIN}/#editor`);
  const home = nodes(read("index.html")).find((node) => node["@id"] === `${ORIGIN}/#editor`);
  assert.deepEqual(app, home, "editor.html and index.html describe the editor differently");
  const defined = new Set(SHAREABLE.flatMap((built) => nodes(built.source).map((node) => node["@id"])));
  for (const id of references(graph)) assert.ok(defined.has(id), `editor.html points at ${id}, defined nowhere`);
  for (const field of ["aggregateRating", "review", "ratingValue", "reviewCount", "datePublished", "dateModified"]) {
    assert.ok(!keysOf(graph).has(field), `editor.html publishes ${field}`);
  }
});

// ---- Pages that are not in the manifests ------------------------------------

test("a noindex page needs no canonical, and never reaches the manifests", () => {
  // The 404 page is the case: served at every missing URL, it has no URL of its
  // own, so it has no canonical — and the generator used to REQUIRE one before it
  // filtered noindex pages out, so the page's mere existence broke the build.
  const notFound = describePage(
    "404.html",
    '<title>Page not found — OpenDoc</title><meta name="robots" content="noindex" />',
  );
  const indexable = describePage("x.html", "<title>A page with no canonical at all</title>");
  assert.deepEqual(selectPages([notFound]), [], "a noindex page must be dropped, not validated");
  assert.throws(() => selectPages([indexable]), /has no canonical/, "an indexable page must still need one");
  // And the real one, whenever it lands.
  if (existsSync(join(WEBAPP, "404.page.html"))) {
    assert.match(read("404.page.html"), /<meta name="robots" content="noindex/, "404.page.html must be noindex");
    assert.ok(!PAGES.some((page) => page.file === "404.html"), "404.html reached the sitemap");
  }
});

// ---- The web app manifest ----------------------------------------------------

test("the web app manifest is valid, every icon is real, and every page links it", () => {
  const manifest = JSON.parse(read("site.webmanifest"));
  for (const key of ["name", "short_name", "start_url", "scope", "display", "icons", "theme_color"]) {
    assert.ok(manifest[key], `site.webmanifest has no ${key}`);
  }
  assert.ok(existsSync(join(WEBAPP, manifest.start_url)), `start_url ${manifest.start_url} does not exist`);
  const sizes = [];
  for (const icon of manifest.icons) {
    const path = join(WEBAPP, icon.src);
    assert.ok(existsSync(path), `manifest icon ${icon.src} does not exist`);
    if (icon.type === "image/png") {
      const { width, height } = pngSize(readFileSync(path));
      assert.equal(icon.sizes, `${width}x${height}`, `${icon.src} is ${width}x${height}, not ${icon.sizes}`);
      sizes.push(width);
    }
  }
  // The two sizes a browser asks for before it will offer an install or a
  // home-screen tile; both generated from the mark by the card generator.
  for (const { size } of ICONS) assert.ok(sizes.includes(size), `no ${size}px icon in the manifest`);
  for (const page of SHAREABLE) {
    const link = tags(page.source, "link").find((tag) => tag.rel === "manifest");
    const depth = page.file.split("/").length - 1;
    assert.equal(
      link?.href,
      `${depth ? "../".repeat(depth) : "./"}site.webmanifest`,
      `${page.file} does not link the web app manifest`,
    );
  }
});

// ---- security.txt (RFC 9116) --------------------------------------------------

test("security.txt names the policy's own reporting channel, and has not expired", () => {
  const text = read(".well-known/security.txt");
  const field = (name) => text.match(new RegExp(`^${name}: (.+)$`, "m"))?.[1];
  // RFC 9116 §2.5: Contact and Expires are REQUIRED.
  const contact = field("Contact");
  const policy = readFileSync(join(WEBAPP, "..", "SECURITY.md"), "utf8");
  assert.ok(contact && policy.includes(`(${contact})`), `Contact ${contact} is not the link SECURITY.md gives`);
  const expires = Date.parse(field("Expires"));
  assert.ok(Number.isFinite(expires), "Expires is not an RFC 3339 date");
  const DAY = 86_400_000;
  // RED 30 DAYS BEFORE IT LAPSES, so renewing it is a scheduled commit and never a
  // reporter finding a stale file. RFC 9116 recommends under a year ahead.
  assert.ok(
    expires - Date.now() > 30 * DAY,
    `security.txt expires ${field("Expires")} — renew it (move Expires up to a year ahead) and commit`,
  );
  assert.ok(expires - Date.now() < 366 * DAY, "Expires is more than a year ahead (RFC 9116 §2.5.5)");
  assert.equal(field("Canonical"), `${ORIGIN}/.well-known/security.txt`);
  assert.ok(PAGES.some((page) => page.canonical === field("Policy")), `Policy ${field("Policy")} is not a site page`);
});

test("the deploy publishes .well-known, and nothing else hidden", async () => {
  // `actions/upload-pages-artifact` tars with `--exclude=.[^/]*` unless told
  // otherwise, which silently drops `.well-known/` — the file would be committed
  // and 404. The flag that fixes that also publishes every other dot-entry, so the
  // tracked ones are pinned: the build-output `.gitignore` and `.well-known/`.
  const workflow = readFileSync(join(WEBAPP, "..", ".github", "workflows", "pages.yml"), "utf8");
  assert.match(workflow, /upload-pages-artifact@[\s\S]*?include-hidden-files: true/, "pages.yml drops .well-known");
  const { execFileSync } = await import("node:child_process");
  const tracked = execFileSync("git", ["ls-files", "--", "."], { cwd: WEBAPP, encoding: "utf8" })
    .split("\n")
    .filter((path) => path.split("/").some((part) => part.startsWith(".")));
  assert.deepEqual(
    [...new Set(tracked.map((path) => path.split("/").find((part) => part.startsWith("."))))].sort(),
    [".gitignore", ".well-known"],
    "a new hidden file under webapp/ would now be published by the Pages deploy",
  );
});
