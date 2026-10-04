// The reference pages: the repository's own design docs, published as pages of
// this site rather than as links to raw Markdown on github.com (`109` HF-192).
//
// Three things need guarding here, and only the first is obvious.
//
//  1. THE PAGES ARE THE DOCUMENTS. Nothing on a reference page is written; the
//     title, the description and the whole article body are functions of the
//     committed `.md`. That is the strongest form of `docs/99` §9.1 available —
//     a fabricated number cannot reach one of these pages, because there is
//     nowhere on the page for a person to type. So this file asserts the
//     derivation twice: once byte-for-byte against a fresh render (the same
//     contract `build-site.py --check` has for the static pages), and once by
//     checking that every digit-bearing token the article shows a reader appears
//     in the source document. The second is the one that would survive somebody
//     replacing the generator.
//
//  2. THE SET IS A RULE, NOT A TASTE CALL. It is every repository Markdown file
//     the site links, minus an argued withheld list. A `.md` link left pointing
//     at github.com is therefore either a bug or a recorded decision, and this
//     file is what forces the choice — the owner's report ("many of these in site
//     still point to github md files") is a report about a set, so the fix has to
//     be a rule about the set and not eleven edits.
//
//  3. THE RATCHET EXCLUSION IS STRUCTURAL AND ITS SIZE IS PUBLISHED. These pages
//     carry two thousand sentences of English that `tools/string_sites.mjs` does
//     not count, because they live in `webapp/reference/` and `scanTree` reads
//     `editor.html`, root-level `*.page.html` and `_partials/*.html`. That is the
//     right exclusion — the English is in `docs/`, and no amount of `t()` in
//     `webapp/` can route a design document — but an exclusion nobody measured is
//     a hiding place. So the number is measured, published, and pinned below.
import assert from "node:assert/strict";
import test from "node:test";
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const {
  OUT_DIR,
  PROVENANCE,
  PUBLISHED,
  WITHHELD,
  artifacts,
  blocks,
  inline,
  openingIsProse,
  plain,
  renderBlocks,
  resolveTarget,
  section,
  slugify,
  summarise,
} = await import("../tools/build-doc-pages.mjs");
const { scanMarkup, scanTree } = await import("../tools/string_sites.mjs");

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const REPO = join(WEBAPP, "..");
const read = (path) => readFileSync(join(WEBAPP, path), "utf8");
const readRepo = (path) => readFileSync(join(REPO, path), "utf8");

/** The generated pages, as committed. */
const BUILT = artifacts().map(([file, rendered]) => ({
  file,
  rendered,
  committed: read(file),
}));

/** Link context for the renderer's unit tests: what the real generator passes. */
const CONTEXT = { published: new Map(PUBLISHED.map((entry) => [entry.source, entry.slug])) };

// ---- 1. The pages are the documents ---------------------------------------

test("every committed reference page is byte-identical to a fresh generator run", () => {
  // The `--check` mode, run in the unit lane so a stale page fails without a
  // wasm build — the same reason `build_site.test.mjs` exists. execFileSync
  // throws on a non-zero exit.
  execFileSync("node", [join(WEBAPP, "tools", "build-doc-pages.mjs"), "--check"], {
    cwd: WEBAPP,
    stdio: "pipe",
  });
  // And directly, so a failure names the page rather than an exit code.
  for (const page of BUILT) {
    assert.equal(page.committed, page.rendered, `${page.file} is not what the generator produces`);
  }
});

/** The article region of a page: what a reader reads, minus the shared chrome and
 *  minus the provenance line (which names the source file, and whose digits are
 *  therefore the document's number rather than one of its claims). */
function articleText(html) {
  const start = html.indexOf('<article class="doc-article">');
  const end = html.indexOf("</article>", start);
  assert.ok(start > 0 && end > start, "the page must have an article region");
  return html
    .slice(start, end)
    .replace(/<p class="doc-provenance">[\s\S]*?<\/p>/g, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&")
    .replace(/\s+/g, " ")
    .trim();
}

test("every number a reference page shows a reader is in the document it came from", () => {
  // The `site_claims.test.mjs` guarantee, in the form these pages can hold. The
  // landing page tags each figure `data-claim` and recomputes it; a reference page
  // has no figures of its own at all, so the assertion is the stronger one: NO
  // digit-bearing token on the page may be absent from the source Markdown.
  //
  // This is what would catch a hand-written "4/5" or "sub-10 ms" being slipped
  // into a generated page — including by a future change to the generator that
  // adds a computed figure to the chrome, which the byte-identity check above
  // would happily bless.
  const offenders = [];
  for (const entry of PUBLISHED) {
    const page = BUILT.find((built) => built.file === `${OUT_DIR}/${entry.slug}.html`);
    const source = readRepo(entry.source);
    for (const token of articleText(page.committed).split(" ")) {
      const bare = token.replace(/^[^\w<>&/.-]+|[^\w>%]+$/g, "");
      if (!/\d/.test(bare) || !bare) continue;
      if (!source.includes(bare)) offenders.push(`${page.file}: ${JSON.stringify(bare)}`);
    }
  }
  assert.deepEqual(
    offenders,
    [],
    "a number is on a published page and not in the document it is generated from, which " +
      "means somebody typed it",
  );
});

test("the one sentence these pages do not take from a document carries no claim", () => {
  // The provenance line. It is a constant rather than per-page prose precisely so
  // that there is one sentence to review, and it must stay a statement about where
  // the page comes from: no number (nothing to be wrong about), no adjective
  // standing in for a fact.
  assert.doesNotMatch(PROVENANCE, /\d/, "the provenance line must carry no number");
  for (const word of ["enterprise", "world-class", "best-in-class", "industry", "fastest"]) {
    assert.ok(
      !PROVENANCE.toLowerCase().includes(word),
      `"${word}" is an adjective standing in for a fact`,
    );
  }
  // And it is actually on every page, naming that page's own source.
  for (const entry of PUBLISHED) {
    const page = BUILT.find((built) => built.file === `${OUT_DIR}/${entry.slug}.html`);
    assert.ok(page.committed.includes(PROVENANCE), `${page.file} does not say where it came from`);
    assert.ok(
      page.committed.includes(
        `<a href="https://github.com/CasualOffice/opendoc/blob/main/${entry.source}"><code>${entry.source}</code></a>`,
      ),
      `${page.file} does not link its own source document`,
    );
  }
});

test("each page is one h1, a lede, and semantic headings under it", () => {
  for (const page of BUILT) {
    const h1s = [...page.committed.matchAll(/<h1[^>]*>/g)];
    assert.equal(h1s.length, 1, `${page.file} has ${h1s.length} <h1> elements`);
    assert.match(page.committed, /<p class="doc-lede">/, `${page.file} has no lede`);
    // No heading level is skipped downwards from the h1: the renderer clamps a
    // document's `##` to h2 and `###` to h3, so an h4 without an h3 above it would
    // mean the clamp broke.
    const levels = [...page.committed.matchAll(/<h([1-6])[^>]*class="doc-h/g)].map((m) =>
      Number(m[1]),
    );
    let deepest = 1;
    for (const level of levels) {
      assert.ok(level <= deepest + 1, `${page.file} jumps from h${deepest} to h${level}`);
      deepest = level;
    }
  }
});

test("a reference page never reaches a font CDN", () => {
  // `tests/chrome_fonts.test.mjs` proves this for `editor.html` and `index.html`.
  // Twelve new pages are twelve new places to break local-first, and they are
  // generated, so one bad line in the generator breaks all of them at once.
  for (const page of BUILT) {
    assert.doesNotMatch(
      page.committed,
      /fonts\.(?:googleapis|gstatic)\.com/,
      `${page.file} links a CDN font — the site self-hosts Inter and Material Symbols`,
    );
    assert.match(page.committed, /href="\.\.\/src\/fonts\.css"/, `${page.file} links no fonts.css`);
  }
});

// ---- 2. The set is a rule -------------------------------------------------

/** Every `blob/main/<path>` link on a hand-authored site surface. The templates
 *  and the partials, not the generated pages: a generated page's links are the
 *  generator's business and are covered by the renderer tests below. */
function markdownLinks() {
  const sources = [
    ...readdirSync(WEBAPP).filter((name) => name.endsWith(".page.html")),
    ...readdirSync(join(WEBAPP, "_partials")).map((name) => `_partials/${name}`),
    "llms.page.txt",
  ];
  const found = new Map();
  for (const name of sources) {
    for (const match of read(name).matchAll(
      /https:\/\/github\.com\/CasualOffice\/opendoc\/blob\/main\/([\w./-]+\.md)/g,
    )) {
      if (!found.has(match[1])) found.set(match[1], []);
      found.get(match[1]).push(name);
    }
  }
  return found;
}

test("no published document is still linked as raw Markdown on github.com", () => {
  const published = new Set(PUBLISHED.map((entry) => entry.source));
  const offenders = [...markdownLinks()]
    .filter(([path]) => published.has(path))
    .map(([path, pages]) => `${path} (linked from ${pages.join(", ")})`);
  assert.deepEqual(
    offenders,
    [],
    "this document has a page on this site, and a site page still sends the reader to " +
      `github.com for it. Point it at ./${OUT_DIR}/<slug>.html — an off-domain link to an ` +
      "unstyled page is the whole defect this work exists to fix",
  );
});

test("every Markdown document the site links is published or withheld with a reason", () => {
  const published = new Set(PUBLISHED.map((entry) => entry.source));
  const withheld = new Map(WITHHELD.map((entry) => [entry.source, entry.reason]));
  const unaccounted = [];
  for (const [path, pages] of markdownLinks()) {
    if (published.has(path) || withheld.has(path)) continue;
    unaccounted.push(`${path} (linked from ${pages.join(", ")})`);
  }
  assert.deepEqual(
    unaccounted,
    [],
    "a site page links a repository document that is neither published as a page nor " +
      "recorded as deliberately withheld. Decide: publish it, or add it to WITHHELD with " +
      "the evidence. An off-domain link nobody argued for is an oversight",
  );
  // The mirror of the rule in `seo.test.mjs` and `no_unrouted_strings.test.mjs`: a
  // deliberate omission that nothing checks is indistinguishable from a mistake, so
  // each one argues its case and the list stays short enough to read.
  assert.ok(WITHHELD.length <= 8, `${WITHHELD.length} withheld documents is not a short list`);
  for (const entry of WITHHELD) {
    assert.ok(
      entry.reason.length > 120,
      `${entry.source} is withheld with no argued reason — say why publishing it would be wrong`,
    );
    assert.ok(
      !published.has(entry.source),
      `${entry.source} is both published and withheld`,
    );
  }
});

test("the withheld list holds the decisions this work actually took", () => {
  // Named, because each of these is a judgement rather than a consequence of a
  // rule, and a judgement that nothing records gets quietly reversed.
  const withheld = new Map(WITHHELD.map((entry) => [entry.source, entry.reason]));

  // docs/60. Its cells were NOT re-derived and could not be: the audit's own
  // banner says it understates the engine, and its five probe documents are
  // untracked by policy. Both halves of that are checked here rather than
  // asserted in prose, so the day either changes, this decision is revisited.
  const audit = "docs/60-FIDELITY-CORPUS-RENDERING-AUDIT.md";
  assert.ok(withheld.has(audit), `${audit} must stay withheld until it is re-derived`);
  const source = readRepo(audit);
  assert.match(
    source,
    /systematically \*\*understates\*\* the engine/,
    "docs/60 no longer says it understates the engine — re-read it and revisit publishing it",
  );
  const ignored = readRepo(".gitignore");
  for (const probe of ["Class notes.docx", "Sample Document.docx", "demo.docx"]) {
    assert.ok(
      ignored.includes(probe),
      `${probe} is no longer excluded from the repository, so docs/60's figures may now be ` +
        "re-derivable — check before leaving it withheld",
    );
  }

  // The trackers, on the owner's instruction.
  for (const tracker of ["docs/14-EXECUTION-TRACKER.md", "docs/105-AUDIT-2026-09-TRACKER.md"]) {
    assert.ok(withheld.has(tracker), `${tracker} is a tracker and must not be published`);
  }

  // docs/83, because it tells a reader to install a package that does not exist.
  // If the package name in the document ever matches a package this repository
  // ships, the reason is spent and the document should be published.
  const sdk = "docs/83-SDK-PACKAGING-EMBEDDING-AND-EXTENSIBILITY-ARCHITECTURE.md";
  assert.ok(withheld.has(sdk), `${sdk} must stay withheld while its title is too long`);
  // The install-line defect that first withheld it is FIXED, so asserting it would
  // now be asserting a lie. What still withholds it is the head check: the heading
  // makes an 85-character page title against the site's 20-70 limit. Assert THAT,
  // so the day somebody shortens the heading this test demands publication — which
  // is exactly how the install-line fix surfaced.
  const sdkHeading = readRepo(sdk).split("\n", 1)[0].replace(/^#\s+\d+\s+—\s+/, "");
  assert.ok(
    `${sdkHeading} — OpenDoc`.length > 70,
    "docs/83's heading now makes a page title the site will accept — publish it, and " +
      "delete this withheld entry",
  );
  assert.doesNotMatch(
    readRepo(sdk),
    /npm install @casualoffice\/document-runtime/,
    "docs/83 tells a reader to install a package that does not exist again",
  );
  assert.equal(
    JSON.parse(readRepo("packages/opendoc-embed/package.json")).name,
    "@casualoffice/opendoc-embed",
    "the shipped package name moved; re-check what docs/83 tells a reader to install",
  );
});

// ---- Reachability ---------------------------------------------------------

test("every reference page is reachable, and links its way back", () => {
  const hub = `${OUT_DIR}/index.html`;
  const inbound = new Map(BUILT.map((page) => [page.file, []]));
  // From the hand-authored site: the templates and the shared partials, which is
  // what the primary navigation reaches.
  const authored = [
    ...readdirSync(WEBAPP).filter((name) => name.endsWith(".page.html")),
    ...readdirSync(join(WEBAPP, "_partials")).map((name) => `_partials/${name}`),
  ];
  for (const name of authored) {
    const html = read(name);
    for (const page of BUILT) {
      if (html.includes(`./${page.file}`)) inbound.get(page.file).push(name);
    }
  }
  // And from each other: every generated page carries the same rail, so the hub is
  // linked from all eleven pages and each page is linked from the hub. A link from
  // a crawlable page is an inbound link wherever it comes from — what this test
  // refuses is a page with NO inbound link at all.
  for (const from of BUILT) {
    for (const page of BUILT) {
      if (page === from) continue;
      const slug = page.file.slice(`${OUT_DIR}/`.length);
      if (from.committed.includes(`href="./${slug}"`)) inbound.get(page.file).push(from.file);
    }
  }

  const orphans = [...inbound].filter(([, from]) => from.length === 0).map(([file]) => file);
  assert.deepEqual(
    orphans,
    [],
    "a page nothing links to is a page nothing crawls: link it from the hub, or from a " +
      "site page the navigation reaches",
  );
  // The hub itself is reached from the pages, and each page is reached from the
  // hub — so the set is connected in both directions rather than a list of leaves.
  for (const page of BUILT) {
    assert.ok(
      page.committed.includes('href="./index.html"'),
      `${page.file} does not link the reference index`,
    );
  }
  // And the whole set hangs off the primary navigation, through Docs.
  const docs = read("docs.page.html");
  assert.ok(
    BUILT.some((page) => docs.includes(`./${page.file}`)),
    "docs.html links no reference page, so nothing in the navigation reaches the set",
  );
  assert.match(
    BUILT[0].committed,
    /<a href="\.\.\/docs\.html" data-nav="docs" aria-current="page"[^>]*>/,
    "a reference page must mark Docs as the active primary-nav item — it is where it lives",
  );
});

// ---- 3. The ratchet exclusion, and its size -------------------------------

test("the reference pages are outside the string ratchet by WHERE THEY LIVE", () => {
  // Structural, not an allowlist. `scanTree` reads `editor.html`, root-level
  // `*.page.html` and `_partials/*.html`; nothing under `reference/` can appear in
  // it, and nothing hand-written can reach the exclusion without being moved into
  // a directory the generator overwrites on every build.
  const counted = [...scanTree(WEBAPP).keys()];
  assert.deepEqual(
    counted.filter((file) => file.startsWith(`${OUT_DIR}/`)),
    [],
    "a reference page is being counted by the string ratchet",
  );
  // The exclusion has to be doing real work, or this test proves nothing: the
  // pages do carry prose the scanner would count.
  for (const page of BUILT) {
    assert.ok(
      scanMarkup(page.committed).length > 20,
      `${page.file} carries almost no prose, so the exclusion above is measuring nothing`,
    );
  }
  // And no generated page was smuggled into the scanned set under another name.
  for (const file of counted) {
    assert.ok(
      !BUILT.some((page) => page.file.endsWith(file)),
      `${file} is both generated and counted`,
    );
  }
});

test("how much English the reference pages put on the site, measured and published", () => {
  // ONE NUMBER, and it is not a ratchet. A ratchet is a debt somebody can pay
  // down; nobody can route a design document through `t()`, so a ceiling here
  // would be a ceiling nobody can ever lower — which `no_unrouted_strings.test.mjs`
  // says is a ratchet that gets deleted. What an exclusion needs instead is a
  // published size, so it cannot become a hiding place: if a hand-written page
  // appears under `reference/`, or a generated one grows a section of prose that
  // is not in a document, this number moves and the change has to be argued.
  //
  // MEASURED, per page, by running the scanner over the committed pages. Not
  // calculated: the total below is the sum this test computes, and the same rule
  // the string ceilings carry applies — a number here is a measurement.
  //
  // IT WILL MOVE, and that is the design. 2,029 when the pages landed; 2,052
  // after a rebase, because another branch added one bullet to
  // `docs/98-PDF-EXPORT-AND-PRINT-DESIGN.md`; 2,060 with `docs/126` phase 3, which
  // added a correction note to `docs/05` saying that its JavaScript example is the
  // DESIGNED surface rather than the shipped one. It had told a reader to
  // `import { DocumentEngine } from "@casualoffice/document-runtime"` — a package
  // that has never existed, importing a symbol nothing exports — from a page live on
  // this site. Eight sentences of English replacing an instruction nobody could
  // complete is a trade worth recording rather than hiding. 2,095 once `docs/126`
  // recorded phase 3's outcome in its own section, which is the whole point of that
  // document and is published as `reference/embeddability-plan.html`. 2109 once
  // `docs/35-DISPOSITION-TAXONOMY.md` recorded why `w:lvl@w:tplc` and
  // `w:lvl@w:tentative` are REPORTED where `w:nsid`/`w:tmpl` are excluded — the
  // two attributes the numbering importer had been dropping in silence, against
  // the rule that whole document exists to state (`docs/142` LST-31). 2161 once
  // `docs/126` recorded the configuration playground — the demo the owner asked
  // for, the page that makes all three SDK phases reachable — which is the same
  // document again and the same reason. 2185 once Playground and Embedding joined
  // the primary nav: two words each, on all twelve pages, because the shared
  // header is inlined into every one of them. Editing a published document
  // already fails `build-doc-pages --check` until the pages are regenerated, so
  // the same commit that regenerates them updates this figure — one number, in
  // the same pass, deliberately. A figure that updated itself would let the
  // excluded region grow silently, which is the one thing it exists to prevent.
  //
  // 2185 -> 2005 with the SITE LOCALISATION SEAM (`109` HF-198), and this is the
  // first time this figure has come DOWN. Nothing was deleted: these pages inline
  // the shared header and footer, and those fifteen chrome strings now carry
  // `data-i18n` keys, so twelve pages lost fifteen unrouted strings each. The
  // ARTICLE bodies are untouched and stay out of scope for exactly the reason the
  // paragraph above gives — a design document cannot be routed through `t()`, and
  // machine-translating twelve engineering documents into nineteen languages
  // would be a worse lie than leaving them in English. What the seam changes here
  // is only the chrome around them, which is the part a reader navigates with.
  // Measured, not calculated.
  //
  // 2005 -> 2005 when the footer gained the authorship and licence lines. The
  // number is RE-MEASURED rather than assumed: the footer is inlined into all
  // twelve of these pages, so a sentence added there costs twelve, and it cost
  // nothing because both lines carry `data-i18n` keys with the English beside
  // them. That is the figure earning its keep in the direction nobody watches —
  // it is here to notice unrouted prose arriving in the shared chrome, and this
  // is what it looks like when prose arrives routed.
  //
  // 2005 -> 2090 on the merge of two lanes that each moved it from 2005 on their
  // own tree: the collaboration lane to 2028 when `20` gained nine `ODC-7xxx`
  // codes (`error-code-registry.html`, 108 -> 131), and this one to 2067 when
  // `35-DISPOSITION-TAXONOMY.md` gained the loss-coverage-gate section
  // (`reference/disposition-taxonomy.html`, 290 -> 352). NEITHER branch's number
  // is this tree's — that is the whole trap this figure sets, and carrying either
  // side of the merge forward would publish a count the pages never had.
  // Re-measured from the regenerated pages on the merged tree.
  //
  // 2090 -> 2093 when `20-ERROR-CODES.md` gained `ODC-7010`
  // (`collaboration_room_full`), the refusal a relay returns when a room is at
  // its participant ceiling (`error-code-registry.html`, 131 -> 134). One row,
  // three strings: the code, its sentence and its cause column. Re-measured
  // from the regenerated page, not derived by adding three -- the figure's
  // whole purpose is that nobody predicts it.
  //
  // Both movements are ARTICLE BODY in published design documents, which is the
  // region this figure deliberately does not route: routing a design document's
  // prose through the string table, or machine translating it, would be a worse
  // lie than leaving it in English. The shared chrome cost nothing on either
  // side, which is the direction this figure actually watches.
  //
  // 2093 -> 2502 with `docs/162`, the deployment guide (`109` RM-11), published as
  // `reference/deployment-containers-and-configuration.html`. MEASURED per page
  // against `origin/main`'s committed bytes rather than calculated, and the
  // movement decomposes exactly:
  //
  //   the new page                                        0 -> 395
  //   reference/index.html (its row in the listing)      49 -> 52
  //   every other page, +1 each (11 pages)                     +11
  //
  // The +1 is the new entry in the shared reference navigation, which these pages
  // inline — an unrouted link label, like the eleven already there. The 392 is
  // ARTICLE BODY in a published design document, which is the region this figure
  // deliberately does not route, for the reason the paragraph above gives.
  //
  // RE-MEASURED FROM THE MERGED TREE, and the detour is worth recording because it
  // is `SKILL.md` §5a's trap in miniature. This branch first measured 2090 -> 2501
  // against `6f0c7efc`, where the committed pages actually totalled 2093 against
  // the 2090 then declared — the figure was 3 stale and the assertion was red on
  // `main`, which this branch found by re-measuring rather than carrying a number
  // forward. #759 then landed the same correction from the other side (2090 ->
  // 2093, ODC-7010's row in `20-ERROR-CODE-REGISTRY.md`), this branch rebased onto
  // it, and the figure was measured AGAIN on the merged tree. 2093 + 409 = 2502,
  // which is the same total either way — but only because the measurement was
  // retaken; arithmetic on this branch's own baseline would have published 2499.
  // 2502 -> 2601 with `docs/162` §1a, §3.1 and §3.5 (`109` RM-15): the compose
  // file's four services became two, and the document gained the one-container
  // section, the fenced competitor recollection, the one-image decision with its
  // measurements, the build-and-run verification paragraph and four guard
  // entries. MEASURED TWICE from the regenerated page — 2586 before the
  // verification paragraph, 2601 after — and not added up either time. The whole
  // movement is in one file (`deployment-containers-and-configuration.html`, 494
  // against 395) and it is ARTICLE BODY in a published design document, which is
  // the region this figure deliberately does not route.
  const sites = Object.fromEntries(
    BUILT.map((page) => [page.file, scanMarkup(page.committed).length]),
  );
  const total = Object.values(sites).reduce((sum, count) => sum + count, 0);
  assert.equal(
    total,
    2601,
    `the thirteen reference pages carry ${total} unrouted English strings (was 2601). That ` +
      `is not a failure — it is the number, and it moved: a published document gained or ` +
      `lost prose, or a page was published. Regenerate the pages, then MEASURE and record ` +
      `the new figure here — do not calculate it. Per page: ` +
      JSON.stringify(sites),
  );
  // The site's own hand-authored pages are still exactly what the string table
  // DECLARES them to be, which is the claim this work has to be able to make: the
  // reference pages cost the hand-authored site nothing.
  //
  // The numbers are read out of `no_unrouted_strings.test.mjs` rather than written
  // here, and that matters. This assertion first carried its own copy of six
  // figures, which made it a SECOND source of truth for the same fact — and it
  // broke the moment another branch legitimately documented the host contract and
  // took `embedding.page.html` from 204 to 310, declaring it properly in the one
  // table that owns it. A guard pinned to the value it happened to measure fails
  // on a change that took nothing away, which is the shape that has reddened
  // `main` twice: assert the GUARANTEE, not the circumstance.
  //
  // What each guard now owns, with no overlap: the string table owns whether a
  // count may move, and this owns whether the count matches what that table says.
  // A page that grows without declaring it fails there; prose smuggled out of a
  // document and into a hand-authored page fails here.
  const declared = new Map(
    [
      ...readFileSync(join(WEBAPP, "tests", "no_unrouted_strings.test.mjs"), "utf8").matchAll(
        /^\s*\["([^"]+\.html)",\s*(\d+)\]/gm,
      ),
    ].map((m) => [m[1], Number(m[2])]),
  );
  const HAND_AUTHORED = [
    "docs.page.html",
    "embedding.page.html",
    "fidelity.page.html",
    "index.page.html",
    "_partials/site-footer.html",
    "_partials/site-header.html",
  ];
  const counts = scanTree(WEBAPP);
  for (const file of HAND_AUTHORED) {
    const expected = declared.get(file);
    assert.ok(
      typeof expected === "number",
      `${file} is not declared in no_unrouted_strings.test.mjs, so this guard has nothing to ` +
        "compare against and would pass by failing to look",
    );
    // `?? 0`: `scanTree` lists a PARTIAL only when it still has sites, so a
    // partial routed all the way to zero is absent from the map rather than
    // present at zero. `site-footer.html` is the first file to reach that state
    // (`109` HF-198); without this, finishing the job would fail the guard that
    // exists to notice work being left undone.
    assert.equal(
      counts.get(file)?.length ?? 0,
      expected,
      `${file} measures ${counts.get(file)?.length ?? 0} but the string table declares ${expected}. ` +
        "Either a reference page put prose on a hand-authored page, or the page changed without " +
        "the table being updated",
    );
  }
});

// ---- The renderer, because a gate nobody trusts gets deleted --------------

test("markdown: a fenced block is code, with its language named", () => {
  const html = renderBlocks(blocks("```rust\nlet x = 1;\n```"), CONTEXT);
  assert.match(html, /<div class="code-panel">/);
  assert.match(html, /<div class="code-panel-head"><span>rust<\/span><\/div>/);
  assert.match(html, /<pre><code>let x = 1;<\/code><\/pre>/);
  // A fence with no language gets no caption rather than an empty one.
  assert.doesNotMatch(renderBlocks(blocks("```\nplain\n```"), CONTEXT), /code-panel-head/);
  // Markup inside a fence is escaped, not rendered: a `<div>` in an example is an
  // example, not a div.
  assert.match(
    renderBlocks(blocks("```\n<div>**bold**</div>\n```"), CONTEXT),
    /&lt;div&gt;\*\*bold\*\*&lt;\/div&gt;/,
  );
});

test("markdown: a pipe table becomes a table, with its alignment", () => {
  const html = renderBlocks(
    blocks("| A | B | C |\n| --- | :-: | ---: |\n| 1 | 2 | 3 |"),
    CONTEXT,
  );
  assert.match(html, /<div class="doc-table-scroll">/);
  assert.match(html, /<th>A<\/th>/);
  assert.match(html, /<th style="text-align:center">B<\/th>/);
  assert.match(html, /<th style="text-align:right">C<\/th>/);
  assert.match(html, /<td>1<\/td>/);
  // Three rows of pipes with no divider line is not a table — it is prose that
  // happens to contain pipes, and inventing a table out of it would be worse than
  // leaving it alone.
  assert.doesNotMatch(renderBlocks(blocks("| not | a table |\n| still | not |"), CONTEXT), /<table/);
  // An escaped pipe stays inside its cell.
  assert.match(
    renderBlocks(blocks("| A | B |\n| --- | --- |\n| a \\| b | c |"), CONTEXT),
    /<td>a \| b<\/td>/,
  );
});

test("markdown: lists nest by indentation, and ordered lists stay ordered", () => {
  const html = renderBlocks(blocks("- one\n- two\n  - nested\n- three"), CONTEXT);
  assert.equal((html.match(/<ul class="doc-list">/g) ?? []).length, 2);
  assert.equal((html.match(/<\/ul>/g) ?? []).length, 2);
  assert.match(html, /<li>nested<\/li>/);
  assert.match(renderBlocks(blocks("1. first\n2. second"), CONTEXT), /<ol class="doc-list">/);
  // A wrapped item is one item, not two.
  const wrapped = renderBlocks(blocks("- a sentence that\n  continues here"), CONTEXT);
  assert.equal((wrapped.match(/<li>/g) ?? []).length, 1);
  assert.match(wrapped, /<li>a sentence that continues here<\/li>/);
});

test("markdown: a block quote is a quote, and its contents are rendered", () => {
  const html = renderBlocks(blocks("> **Note.** Read `this` first."), CONTEXT);
  assert.match(html, /<blockquote class="doc-quote">/);
  assert.match(html, /<strong>Note\.<\/strong>/);
  assert.match(html, /<code>this<\/code>/);
});

test("inline: a code span is code, and nothing inside it is markup", () => {
  // The reason code spans are lifted out before anything else runs. These
  // documents are full of `w:rsid*`, `snake_case` and `**` inside backticks, and
  // every one of them would otherwise become emphasis.
  assert.equal(inline("`w:rsid*` and `a*b*c`", CONTEXT), "<code>w:rsid*</code> and <code>a*b*c</code>");
  assert.equal(inline("`**not bold**`", CONTEXT), "<code>**not bold**</code>");
  assert.equal(inline("`<div>`", CONTEXT), "<code>&lt;div&gt;</code>");
  // Underscores are never emphasis: an identifier is not italics.
  assert.equal(inline("font_table_id stays", CONTEXT), "font_table_id stays");
  // But asterisk emphasis and strong still work outside code.
  assert.equal(inline("**bold** and *italic*", CONTEXT), "<strong>bold</strong> and <em>italic</em>");
});

test("inline: a link to a published document becomes a link to its page", () => {
  // The whole exercise, at the level of one link.
  assert.equal(
    inline("see [the spec](docs/05-SDK-API-SPEC.md)", CONTEXT),
    'see <a href="./sdk-api-spec.html">the spec</a>',
  );
  // Doc-relative and repo-relative spellings of the same file both resolve.
  assert.equal(resolveTarget("05-SDK-API-SPEC.md", CONTEXT), "./sdk-api-spec.html");
  assert.equal(resolveTarget("docs/05-SDK-API-SPEC.md", CONTEXT), "./sdk-api-spec.html");
  assert.equal(resolveTarget("./CONTRIBUTING.md", CONTEXT), "./contributing.html");
  // A fragment survives.
  assert.equal(resolveTarget("docs/05-SDK-API-SPEC.md#errors", CONTEXT), "./sdk-api-spec.html#errors");
  // A document that is NOT published stays a link to GitHub, which is where it
  // actually is. Withholding a document does not mean hiding it.
  assert.equal(
    resolveTarget("docs/60-FIDELITY-CORPUS-RENDERING-AUDIT.md", CONTEXT),
    "https://github.com/CasualOffice/opendoc/blob/main/docs/60-FIDELITY-CORPUS-RENDERING-AUDIT.md",
  );
  // So does a source file.
  assert.equal(
    resolveTarget("crates/casual-doc-layout/src/lib.rs", CONTEXT),
    "https://github.com/CasualOffice/opendoc/blob/main/crates/casual-doc-layout/src/lib.rs",
  );
  // An absolute URL and a bare fragment are left exactly as they are.
  assert.equal(resolveTarget("https://example.org/a", CONTEXT), "https://example.org/a");
  assert.equal(resolveTarget("#a-section", CONTEXT), "#a-section");
});

test("headings get stable ids, and a repeated heading does not get a repeated id", () => {
  assert.equal(slugify("Axis A — model outcome"), "axis-a-model-outcome");
  const html = renderBlocks(blocks("## Goals\ntext\n\n## Goals\nmore"), CONTEXT);
  assert.match(html, /id="goals"/);
  assert.match(html, /id="goals-2"/);
});

test("the summary is the document's prose, never its filing information", () => {
  // Provenance lines are skipped: a search result made of "Status: Accepted for
  // Phase 0" describes nothing.
  const withPreamble = blocks(
    "**Status:** Accepted for Phase 0\n\nThe engine owns document state and layout, " +
      "and the host owns storage, network and policy decisions.",
  );
  assert.match(summarise(withPreamble), /^The engine owns document state/);

  // A paragraph that only introduces a list is skipped too: quoted on its own it
  // is a sentence with its predicate missing.
  const withStem = blocks(
    "Every error crossing the boundary has:\n\n- a code;\n\nError codes are never " +
      "recycled, and their meaning may not change without a breaking release.",
  );
  assert.match(summarise(withStem), /^Error codes are never recycled/);

  // A document with nothing but headings and lists has no summary, and says so
  // rather than being given one. That is a publishing decision — it is how
  // docs/40 came to be withheld — so it must not quietly return something.
  assert.equal(summarise(blocks("## Goals\n\n- G1: a goal;\n- G2: another goal;")), null);

  // And a named section narrows where the summary is read from, without changing
  // whose words it is.
  const doc = blocks("## First\n\nNot this one at all, though it is long enough to qualify.\n\n## Second\n\nThis paragraph is the one the manifest asked for, and it is long enough too.");
  assert.match(summarise(section(doc, "Second", "test.md")), /^This paragraph is the one/);
  assert.throws(
    () => section(doc, "Third", "test.md"),
    /does not have/,
    "naming a section that is not there must fail loudly, not fall back silently",
  );
});

test("a summary may continue past the opening, but may not start deep in a document", () => {
  // The rule that keeps a page's one quotable sentence a sentence ABOUT the page.
  //
  // It may continue: `docs/02`'s first section is one short sentence and an ASCII
  // diagram, and its description is that sentence plus the next one from the
  // section after it. Refusing that would have cost a good description.
  assert.ok(
    openingIsProse(blocks("## First\n\nA short opening sentence.\n\n## Second\n\nMore prose here.")),
  );
  // It may not start deep: a document whose first section is headings and lists
  // has nothing to summarise, and the next prose it reaches describes a subsection
  // rather than the page.
  assert.equal(
    openingIsProse(blocks("## First\n\n- a bullet;\n- another;\n\n## Second\n\nDeep prose here.")),
    false,
  );
  // A provenance line does not count as the opening prose, which is the case that
  // matters: almost every document in `docs/` starts with one.
  assert.equal(
    openingIsProse(blocks("**Status:** Accepted\n\n## First\n\n- a bullet;\n\n## Second\n\nDeep.")),
    false,
  );

  // And the real case, measured against the real document. `docs/40` is withheld
  // BECAUSE of this rule; if the document ever gains an opening paragraph, this
  // fails and the withheld entry has to be revisited rather than left to ossify.
  const fonts = blocks(readRepo("docs/40-FONT-MANAGEMENT-DESIGN.md")).slice(1);
  assert.equal(
    openingIsProse(fonts),
    false,
    "docs/40 now opens with prose, so it has a summary and can be published — remove " +
      "its WITHHELD entry",
  );
  // Verified, not assumed: the sentence the scan WOULD have reached is the
  // competitive claim named in that entry's reason.
  assert.match(summarise(fonts), /the embedded-font correctness that all three products lack/);
});

test("plain() strips markup so a title and a description are text", () => {
  assert.equal(plain("**Bold** `code` [label](url) *em*"), "Bold code label em");
  assert.equal(plain("a   multi\nline   run"), "a multi line run");
});
