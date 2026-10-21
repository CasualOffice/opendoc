// Every claim on the landing page must have a committed source.
//
// The page this replaced published six claims that did not survive contact with
// the code, and they cut both ways:
//
//   - "three of five corpus documents match page counts exactly" — while the
//     fidelity page said 4/5, and docs/60 records 4/5 as the ONLY committed
//     figure and calls any other number the EV-001/EV-002 defect.
//   - a mock table row for `tables-nested.docx`, a file that is not in the corpus.
//   - "Sub-10 ms incremental repaint", with no repaint benchmark anywhere; docs/106
//     already records a fabricated "7 ms" reaching a public page the same way.
//   - "Not yet: text wrap around floats · inline math · multi-column layout" —
//     all three shipped. Understating is still false, and it costs adoption.
//
// So nothing numeric on the page is typed and trusted: each figure is tagged
// `data-claim` and recomputed here from the artifact it comes from. Buildless,
// in the existing `npm run test:unit` lane.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const page = read("../index.page.html");

// fidelity.js is a classic browser script; evaluate it the way
// fidelity_data.test.mjs does, with a fake `module` and no DOM.
const sandbox = { exports: {} };
new Function("module", read("../src/fidelity.js"))(sandbox);
const { FIDELITY } = sandbox.exports;

const count = (key, value) => FIDELITY.filter((family) => family[key] === value).length;

/** Every value tagged `data-claim="name"` on the page, as trimmed text. */
function claims(name) {
  const re = new RegExp(`data-claim="${name}"[^>]*>([^<]*)<`, "g");
  return [...page.matchAll(re)].map((m) => m[1].trim());
}

/** The single value of a claim; fails if it is missing or inconsistent. */
function claim(name) {
  const values = claims(name);
  assert.ok(values.length > 0, `the page must carry data-claim="${name}"`);
  assert.equal(
    new Set(values).size,
    1,
    `data-claim="${name}" appears with different values: ${values.join(", ")}`,
  );
  return values[0];
}

test("family counts come from fidelity.js, not from memory", () => {
  assert.equal(Number(claim("families")), FIDELITY.length, "graded families");
  assert.equal(Number(claim("roundtrip-full")), count("roundtrips", "full"), "full round-trips");
  for (const grade of ["full", "partial", "preserved", "none"]) {
    assert.equal(
      Number(claim(`rendered-${grade}`)),
      count("rendered", grade),
      `families that render "${grade}"`,
    );
  }
});

test("the render-grade bar is drawn from the same counts as its legend", () => {
  // The bar's segment weights are CSS custom properties; a legend that says 14
  // over a bar drawn at 13 is a chart lying about its own label.
  const bar = page.match(/<div class="home-dist-bar"[^>]*>([\s\S]*?)<\/div>/);
  assert.ok(bar, "the page must carry the render-grade bar");
  const weights = Object.fromEntries(
    [...bar[1].matchAll(/class="is-([a-z]+)" style="--n: (\d+)"/g)].map((m) => [m[1], Number(m[2])]),
  );
  for (const grade of ["full", "partial", "preserved", "none"]) {
    assert.equal(weights[grade], count("rendered", grade), `bar segment "${grade}"`);
  }
});

test("page-count parity is the one figure docs/60 records, with its delta", () => {
  // docs/60 is the only committed source, and says so in as many words.
  const source = read("../../docs/60-FIDELITY-CORPUS-RENDERING-AUDIT.md");
  const recorded = source.match(/\*\*(\d+)\/(\d+) exact parity, worst delta ([+-]\d+)\*\*/);
  assert.ok(recorded, "docs/60 must still record the parity figure");
  const [, hit, of, delta] = recorded;

  assert.equal(claim("page-parity").replace(/\s/g, ""), `${hit}/${of}`, "parity on the page");
  assert.equal(claim("page-delta"), delta, "worst page delta on the page");
  // And the page must not present it as live: it was measured once, by hand.
  assert.match(page, /Measured 2026-07-27/, "the parity figure must carry its measurement date");
  assert.match(page, /not yet a CI gate/, "the parity figure must say it is not a CI gate");
});

test("the sampled matrix rows state the grades fidelity.js actually gives", () => {
  const byName = new Map(FIDELITY.map((family) => [family.family, family]));
  const rows = [...page.matchAll(/data-family="([^"]+)"[^>]*>([\s\S]*?)<\/div>/g)];
  assert.ok(rows.length >= 3, "the proof card must sample at least three families");
  const decode = (text) => text.replaceAll("&amp;", "&");
  for (const [, rawName, body] of rows) {
    const name = decode(rawName);
    const family = byName.get(name);
    assert.ok(family, `"${name}" is not a family in fidelity.js`);
    const cells = [...body.matchAll(/role="cell"[^>]*>([^<]*)</g)].map((m) => m[1].trim());
    assert.equal(cells[1].toLowerCase(), family.rendered, `${name}: rendered`);
    assert.equal(cells[2].toLowerCase(), family.editable, `${name}: editable`);
  }
});

test("the 'not yet' list names only what genuinely does not render", () => {
  const line = page.match(/<b[^>]*>Not yet:<\/b>([\s\S]*?)<a /);
  assert.ok(line, "the roadmap must carry a 'Not yet' line");

  // Every gap declares where the claim comes from, so the check is exact rather
  // than a word match: "floating tables" is a real open gap even though the
  // Tables family renders, and fuzzy matching cannot tell those apart.
  const items = [...line[1].matchAll(/<span ([^>]*)>([^<]*)<\/span>/g)];
  assert.ok(items.length > 0, "the gaps must be marked up individually");

  const byName = new Map(FIDELITY.map((family) => [family.family, family]));
  const tracker = read("../../docs/105-AUDIT-2026-09-TRACKER.md");
  const decode = (text) => text.replaceAll("&amp;", "&");

  for (const [, attributes, label] of items) {
    const family = attributes.match(/data-gap-family="([^"]+)"/);
    const row = attributes.match(/data-gap-row="([^"]+)"/);
    assert.ok(
      family || row,
      `"${label.trim()}" is listed as missing without naming a source ` +
        "(data-gap-family or data-gap-row)",
    );
    if (family) {
      const graded = byName.get(decode(family[1]));
      assert.ok(graded, `"${family[1]}" is not a family in fidelity.js`);
      assert.equal(
        graded.rendered,
        "none",
        `"${graded.family}" renders (${graded.rendered}) but is listed under Not yet`,
      );
    }
    if (row) {
      const found = tracker.match(new RegExp(`^\\| ${row[1]} \\|.*\\| ([^|]+) \\|\\s*$`, "m"));
      assert.ok(found, `${row[1]} is not a row in docs/105`);
      assert.match(
        found[1].trim().replace(/^[*_]+/, ""),
        /^(Open|Partly|In progress|Re-opened)/,
        `${row[1]} is closed in docs/105 but still listed under Not yet`,
      );
    }
  }

  // The previous page listed these as missing after all three had shipped.
  const text = line[1].toLowerCase();
  for (const shipped of ["multi-column", "inline math", "text wrap"]) {
    assert.ok(!text.includes(shipped), `"${shipped}" shipped and must not be listed as missing`);
  }
});

test("no performance figure without a benchmark behind it", () => {
  // There is no layout, render or repaint benchmark (docs/105 CQ-006: the four
  // committed cases are package open, part read, model load and typing).
  const visible = page.replace(/<!--[\s\S]*?-->/g, "").replace(/<[^>]+>/g, " ");
  const timings = visible.match(/\b\d+(\.\d+)?\s?(ms|milliseconds?|µs)\b/gi) ?? [];
  assert.deepEqual(
    timings,
    [],
    "a timing on a public page needs a committed benchmark case; add one to " +
      "benchmarks/baselines first, or do not publish the number",
  );
});

test("the embed snippet calls only functions the engine really exports", () => {
  // The design prototype showed `doc.transaction(...)`, `doc.layout(viewport)`
  // and `doc.writeDocx()`. None exists. A developer landing page that invents
  // its API fails the first person who copies it.
  const snippet = page.match(/<pre id="qsEmbed"[\s\S]*?<\/pre>/);
  assert.ok(snippet, "the embed quickstart panel must exist");
  const code = snippet[0].replace(/<[^>]+>/g, "");
  const calls = [...new Set([...code.matchAll(/doc\.([a-zA-Z]+)\(/g)].map((m) => m[1]))];
  assert.ok(calls.length >= 3, "the snippet should demonstrate real calls");

  const wasm = read("../../crates/casual-doc-wasm/src/lib.rs");
  for (const name of calls) {
    assert.match(
      wasm,
      new RegExp(`js_name = ${name}\\b`),
      `doc.${name}() is not a casual-doc-wasm export`,
    );
  }
  assert.match(code, /import init, \{ open \}/, "the snippet must import the real entry point");
  assert.match(wasm, /pub fn open\(/, "`open` must still be the wasm entry point");
});

test("the hero embed stays a static poster until a visitor asks for the editor", () => {
  // Booting the WASM editor on page load spends ~100 MB of a marketing tab
  // (tests/e2e/memory-budget.spec.mjs). The design prototype put a live iframe
  // straight into the hero; this keeps it from coming back through a redesign.
  const start = page.indexOf('id="homeEmbed"');
  const end = page.indexOf("<!-- Rail -->");
  // Both markers, explicitly: with the end marker gone `slice` would run to -1 and
  // quietly test most of the page instead of the hero.
  assert.ok(start > 0 && end > start, "the hero embed and the <!-- Rail --> marker after it must exist");
  const hero = page.slice(start, end);
  assert.ok(!/<iframe/i.test(hero), "the hero must not ship an <iframe>; home-embed.js mounts one on click");
  assert.match(hero, /class="home-embed-run"/, "the poster needs its Run control");
});

test("the 'draws today' strip lists only families that render, at the grade they render", () => {
  // The strip under the hero (`109` HF-293) is a list of capabilities, which is the
  // shape a marketing page overstates in. Every chip names its fidelity.js family
  // and carries that family's render grade as its class, so a chip for a family
  // graded "none" — or a "full" dot on a partial one — cannot ship.
  const byName = new Map(FIDELITY.map((family) => [family.family, family]));
  const decode = (text) => text.replaceAll("&amp;", "&");
  const chips = [...page.matchAll(/<li class="is-([a-z]+)" data-render-family="([^"]+)"/g)];
  assert.ok(chips.length >= 8, `the strip should carry its chips, found ${chips.length}`);
  for (const [, grade, rawName] of chips) {
    const family = byName.get(decode(rawName));
    assert.ok(family, `"${decode(rawName)}" is not a family in fidelity.js`);
    assert.notEqual(family.rendered, "none", `"${family.family}" does not render but is on the strip`);
    assert.equal(grade, family.rendered, `"${family.family}" renders ${family.rendered}, the chip says ${grade}`);
  }
  // The decorative loop copy (aria-hidden) must be the same list, or the strip
  // shows a sighted reader something the accessible list does not say.
  const copy = page.match(/<ul class="home-marquee-track home-marquee-track--copy"[^>]*>([\s\S]*?)<\/ul>/);
  const main = page.match(/<ul class="home-marquee-track">([\s\S]*?)<\/ul>/);
  assert.ok(copy && main, "the strip and its loop copy must both exist");
  const keys = (html) => [...html.matchAll(/class="is-([a-z]+)"[^>]*data-i18n="([^"]+)"/g)].map((m) => m.slice(1).join(" "));
  assert.deepEqual(keys(copy[1]), keys(main[1]), "the loop copy drifted from the list it repeats");
});

test("the embedding figure speaks the element's real vocabulary", async () => {
  // The figure shows a host writing `<opendoc-editor mode=… can=…>` and a row of
  // capability chips. A developer copies what a landing page shows, so every
  // attribute must be one the element reads, the mode a real role, and every chip
  // a real capability.
  const { CAPABILITIES, ROLES } = await import("../src/capabilities.mjs");
  const element = read("../src/embed_element.mjs");
  const mount = element.match(/const MOUNT_ATTRIBUTES = Object\.freeze\(\[([\s\S]*?)\]\)/);
  assert.ok(mount, "MOUNT_ATTRIBUTES must still be declared in embed_element.mjs");
  const attributes = [...mount[1].matchAll(/"([a-z-]+)"/g)].map((m) => m[1]);

  const tag = page.match(/<pre class="hh-tag"><code>([\s\S]*?)<\/code><\/pre>/);
  assert.ok(tag, "the home page's embedding figure must show the element");
  const code = tag[1].replace(/<[^>]+>/g, "").replaceAll("&lt;", "<").replaceAll("&gt;", ">");
  assert.match(code, /^<opendoc-editor\b/);
  const used = [...code.matchAll(/\s([a-z-]+)="([^"]*)"/g)];
  assert.ok(used.length >= 2, "the figure should show at least two attributes");
  for (const [, name, value] of used) {
    assert.ok(attributes.includes(name), `${name}="" is not an attribute <opendoc-editor> reads`);
    if (name === "mode") assert.ok(ROLES.includes(value), `mode="${value}" is not a role`);
    if (name === "can") {
      for (const item of value.split(/[\s,]+/).filter(Boolean)) {
        assert.ok(CAPABILITIES.includes(item.replace(/^[-+]/, "")), `can="${value}" names no capability`);
      }
    }
  }
  for (const source of [page, read("../embedding.page.html")]) {
    const chips = [...source.matchAll(/data-capability="([^"]+)"/g)].map((m) => m[1]);
    assert.ok(chips.length > 0, "the capability chips must be marked up with data-capability");
    for (const chip of chips) assert.ok(CAPABILITIES.includes(chip), `"${chip}" is not a capability`);
  }
});

test("collaboration is described as docs/169 describes it, not as planned and not as finished", () => {
  // The page said "Real-time collaboration — Planned" after #828 had shipped
  // presence, Share, safe recovery and the convergence fuzz: understating is also
  // false (SKILL §9.6). docs/169 calls it experimental, so the roadmap does too —
  // and the day that document drops the word, this asks for the page to follow.
  const doc = read("../../docs/169-SHARING-PRESENCE-AND-SNAPSHOT-RECOVERY.md");
  const status = doc.match(/^Status: (.*)$/m);
  assert.ok(status, "docs/169 must still carry a Status line");
  const phase = page.match(
    /<span class="home-state"[^>]*>([^<]*)<\/span>\s*<h3[^>]*>Real-time collaboration<\/h3>/,
  );
  assert.ok(phase, "the roadmap must carry a real-time collaboration phase");
  assert.notEqual(phase[1].trim(), "Planned", "collaboration shipped in #828; it is not planned");
  if (/experimental/i.test(status[1])) {
    assert.equal(phase[1].trim(), "Experimental", "docs/169 says experimental; so must the roadmap");
  }
  // Presence is labelled by participant NUMBER (docs/169 §Presence), so the
  // illustrations must not show a display name the product cannot draw.
  assert.match(doc, /Labels\s+currently show participant numbers/);
  const flags = [...page.matchAll(/class="od-peer-flag"[^>]*>([^<]*)</g)].map((m) => m[1]);
  assert.ok(flags.length > 0, "the hero must show the second participant's flag");
  for (const flag of flags) assert.match(flag, /^Participant \d+$/, "a presence flag names a participant number");
});
