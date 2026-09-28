import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";

// A site page that ships with no nav entry of its own is unreachable in
// practice, whatever its URL returns. `playground.html` deployed green for a
// day, answered 200, was listed in the sitemap — and could not be found from
// the site, because the header nav held four links and the playground was not
// one of them. It borrowed `active=docs`, so it lit up somebody else's tab.
// `embedding.html` had the same shape.
//
// So: every generated site page states its own nav key, and the header nav
// carries a link to it. A page that should deliberately stay out of the nav is
// declared below WITH a reason — the decision is then visible in review rather
// than absent.
const WEBAPP = new URL("../", import.meta.url);

// Pages intentionally absent from the primary nav. The nav is for destinations
// a reader chooses; these are reached by doing something, not by browsing.
const NOT_IN_NAV = new Map([
  // editor.html is the product surface. It is reached from the header CTA
  // ("Open the editor") and from every page's call to action, which is a
  // stronger placement than a nav link, and it has no *.page.html template.
  ["editor.html", "reached by the header CTA, which outranks a nav link"],
  // embed.html is the iframe body an embedding host loads. It is not a page a
  // reader visits; the playground and the embedding docs drive it.
  ["embed.html", "the embedded frame, loaded by a host and by the playground"],
]);

async function read(relativePath) {
  return String(await readFile(new URL(relativePath, WEBAPP)));
}

test("every site page has a nav entry of its own", async () => {
  const header = await read("_partials/site-header.html");
  const navKeys = new Set([...header.matchAll(/data-nav="([\w-]+)"/g)].map((m) => m[1]));
  const navHrefs = new Set([...header.matchAll(/<a href="([^"]+)" data-nav="([\w-]+)"/g)].map((m) => m[1]));

  const templates = (await readdir(new URL(".", WEBAPP))).filter((f) => f.endsWith(".page.html"));
  assert.ok(templates.length >= 5, `expected the site's page templates, found ${templates.length}`);

  for (const template of templates) {
    const page = template.replace(/\.page\.html$/, ".html");
    if (NOT_IN_NAV.has(page)) continue;

    const source = await read(template);
    const active = source.match(/@include site-header active=([\w-]+)/)?.[1];
    assert.ok(active, `${template} includes the header without an active= key`);
    assert.ok(navKeys.has(active), `${template} claims data-nav="${active}", which the nav does not define`);

    // The key must be the page's OWN key, not a neighbour's: the nav link
    // carrying it has to point back at this page. This is the assertion that
    // playground.page.html failed while passing every other site guard.
    const expectedHref = page === "index.html" ? "./" : `./${page}`;
    assert.ok(
      navHrefs.has(expectedHref),
      `the nav has no link to ${page} — it ships unreachable from the header`,
    );
    const linkForKey = header.match(new RegExp(`<a href="([^"]+)" data-nav="${active}"`))?.[1];
    assert.equal(
      linkForKey,
      expectedHref,
      `${template} marks data-nav="${active}" active, but that link points at ${linkForKey}, not ${expectedHref}`,
    );
  }
});

test("each nav key is defined once, and every nav link resolves to a built page", async () => {
  const header = await read("_partials/site-header.html");
  const links = [...header.matchAll(/<a href="([^"]+)" data-nav="([\w-]+)"/g)].map((m) => ({
    href: m[1],
    key: m[2],
  }));
  assert.ok(links.length >= 4, `expected the primary nav, found ${links.length} links`);

  const keys = links.map((l) => l.key);
  assert.deepEqual([...new Set(keys)], keys, "a nav key is defined twice; active marking would hit the first");

  const built = new Set((await readdir(new URL(".", WEBAPP))).filter((f) => f.endsWith(".html")));
  for (const { href, key } of links) {
    const file = href === "./" ? "index.html" : href.replace(/^\.\//, "");
    assert.ok(built.has(file), `nav link ${key} points at ${file}, which the site does not build`);
  }
});
