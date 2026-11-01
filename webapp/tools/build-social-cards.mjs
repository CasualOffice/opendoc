#!/usr/bin/env node
// Generates the site's link-preview images (og:image / twitter:image) and the web
// app manifest's PNG icons, from committed sources.
//
// WHY. Every page advertised `assets/editor.jpg`: a 1456x829 screenshot staged by
// `build.sh`, which is not the 1.91:1 a link preview crops to, and which no page
// described with a width, a height or (on four pages) any alt text at all. A card
// that is cropped differently by every consumer and announced with no dimensions
// is a preview nobody designed. This makes the cards a build artifact with a
// source, the way every other published artifact here is (`docs/99` §9.1).
//
// SOURCES, all committed:
//
//   * `social/card.template.svg` — the card's design, with `{{…}}` slots;
//   * `CARDS` below — the words on each card, and the alt text that describes it;
//   * `assets/fonts/instrument-sans.woff2` — the site's own type, embedded;
//   * `opendoc-mark.svg` — the mark, for the two manifest icons.
//
// TWO MODES, because rasterizing needs a browser and the deploy has none.
//
//   --render   Fills the template, rasterizes each card and icon with Playwright's
//              Chromium (already the webapp's one devDependency), writes the PNGs
//              and `social/cards.json`: for every image its pixel size, the SHA-256
//              of the exact source text it was rendered from, and the SHA-256 of
//              the PNG bytes. It also measures every line of text in the browser
//              and refuses a card whose words overflow the safe area, so a long
//              title fails here instead of being clipped in somebody's feed.
//   --check    (what `build.sh` runs; pure Node, no browser) re-derives each
//              source hash and compares it with the manifest, re-hashes each PNG,
//              and reads each PNG's own IHDR header for its size. So the build
//              fails when the template, a card's words, the font or the mark change
//              without the images being regenerated, AND when a PNG is replaced by
//              hand — both directions, the same contract as `build-seo --check`.
//
// Re-rasterizing is NOT part of `--check`, deliberately: a PNG is not a pure
// function of its source across Chromium releases and platforms (anti-aliasing
// and hinting move), so a byte comparison against a fresh render would fail for
// reasons that are not drift. The source hash is the deterministic half; the
// committed PNG is the artifact it produced.
//
// WHAT A CARD MAY SAY. These images are published claims in a format no reviewer
// reads closely, so `tests/seo.test.mjs` refuses any digit on a card (no number
// can be smuggled into a picture), and every phrase here is one the site's pages
// already make and back.
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");

export const CARD_WIDTH = 1200;
export const CARD_HEIGHT = 630;
export const MANIFEST = "social/cards.json";
const TEMPLATE = "social/card.template.svg";
const FONT = "assets/fonts/instrument-sans.woff2";
const MARK = "opendoc-mark.svg";

/** The text block may not cross this right edge (the template's 80px margin). */
const SAFE_RIGHT = CARD_WIDTH - 80;

/** Every card, keyed by the file a page's `og:image` names.
 *
 *  `alt` is what `og:image:alt` and `twitter:image:alt` publish for the card, so a
 *  screen-reader user of a feed hears what a sighted one sees: it is the card's own
 *  words, not a description of the product. `tests/seo.test.mjs` asserts every page
 *  that advertises a card publishes exactly this alt. */
export const CARDS = [
  {
    file: "social/opendoc.png",
    kicker: "OPEN SOURCE · LOCAL-FIRST · EMBEDDABLE",
    title: ["An open alternative", "to ONLYOFFICE Docs"],
    subtitle: [
      "Word documents edited in the browser on a Rust engine,",
      "with experimental real-time co-editing.",
    ],
  },
  {
    file: "social/editor.png",
    kicker: "OPENDOC EDITOR",
    title: ["Open and edit Word", "documents in your browser"],
    subtitle: [
      "The engine runs on your device in WebAssembly,",
      "so opening or editing a file needs no server.",
    ],
  },
  {
    file: "social/embedding.png",
    kicker: "EMBED THE EDITOR",
    title: ["A document editor", "inside your product"],
    subtitle: [
      "A framework-free custom element, and a capability",
      "contract that the host page decides.",
    ],
  },
  {
    file: "social/sdk.png",
    kicker: "OPENDOC SDK",
    title: ["Documents in code,", "laid out by the engine"],
    subtitle: [
      "Create, read, edit, convert and render",
      "from JavaScript, Python, Java and Rust.",
    ],
  },
  {
    file: "social/fidelity.png",
    kicker: "DOCX FIDELITY",
    title: ["Graded by construct,", "not by feature list"],
    subtitle: [
      "Per-construct support grades that say",
      "what is not there yet.",
    ],
  },
  {
    file: "social/reference.png",
    kicker: "DOCUMENTATION",
    title: ["Architecture, SDK and", "design documents"],
    subtitle: [
      "Generated from the repository's own Markdown,",
      "so a page and its source cannot disagree.",
    ],
  },
].map((card) => ({
  ...card,
  alt: `OpenDoc: ${card.title.join(" ")}. ${card.subtitle.join(" ")}`,
}));

/** The web app manifest's raster icons, drawn from the mark. */
export const ICONS = [
  { file: "icon-192.png", size: 192 },
  { file: "icon-512.png", size: 512 },
];

const readWebapp = (path) => readFileSync(join(WEBAPP, path));
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

const escapeXml = (value) =>
  value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** The filled SVG for one card — a pure function of the template and the card. */
export function cardSvg(card) {
  const template = readWebapp(TEMPLATE).toString("utf8");
  const tspans = (lines, step) =>
    lines
      .map((line, index) => `<tspan x="80" dy="${index ? step : 0}">${escapeXml(line)}</tspan>`)
      .join("");
  // The subtitle sits a fixed gap under the title's last baseline, so a one-line
  // and a two-line title both read as one block.
  const subtitleY = 316 + (card.title.length - 1) * 74 + 66;
  const filled = template
    .replace("{{KICKER}}", escapeXml(card.kicker))
    .replace("{{TITLE}}", tspans(card.title, 74))
    .replace("{{SUBTITLE_Y}}", String(subtitleY))
    .replace("{{SUBTITLE}}", tspans(card.subtitle, 38));
  const left = filled.match(/\{\{[A-Z_]+\}\}/);
  if (left) throw new Error(`${TEMPLATE}: slot ${left[0]} has no value`);
  return filled;
}

/** The hash a manifest entry is checked against: everything the PNG depends on. */
export function sourceHash(entry) {
  const parts = [];
  if (entry.kind === "card") {
    parts.push(cardSvg(entry.card), readWebapp(FONT));
  } else {
    parts.push(readWebapp(MARK), String(entry.size));
  }
  const hash = createHash("sha256");
  for (const part of parts) hash.update(part).update("\0");
  return hash.digest("hex");
}

/** Every image this generator owns, in manifest order. */
export function entries() {
  return [
    ...CARDS.map((card) => ({
      kind: "card",
      file: card.file,
      width: CARD_WIDTH,
      height: CARD_HEIGHT,
      card,
    })),
    ...ICONS.map((icon) => ({ kind: "icon", file: icon.file, width: icon.size, height: icon.size, size: icon.size })),
  ];
}

/** A PNG's pixel size, from its own IHDR chunk — not from the manifest. */
export function pngSize(bytes) {
  const signature = "89504e470d0a1a0a";
  if (bytes.subarray(0, 8).toString("hex") !== signature || bytes.toString("latin1", 12, 16) !== "IHDR") {
    throw new Error("not a PNG");
  }
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

function fontFace() {
  const data = readWebapp(FONT).toString("base64");
  return (
    `@font-face{font-family:"Instrument Sans";font-weight:400 700;` +
    `src:url(data:font/woff2;base64,${data}) format("woff2");}`
  );
}

async function render() {
  const { chromium } = await import("@playwright/test");
  const browser = await chromium.launch();
  const manifest = [];
  try {
    const page = await browser.newPage({ deviceScaleFactor: 1 });
    for (const entry of entries()) {
      const svg = entry.kind === "card" ? cardSvg(entry.card) : readWebapp(MARK).toString("utf8");
      await page.setViewportSize({ width: entry.width, height: entry.height });
      await page.setContent(
        `<!doctype html><html><head><style>${fontFace()}` +
          `html,body{margin:0;background:transparent}` +
          `svg.card{display:block;width:${entry.width}px;height:${entry.height}px}</style></head>` +
          `<body>${svg.replace("<svg ", '<svg class="card" ')}</body></html>`,
      );
      await page.evaluate(() => document.fonts.ready);
      if (entry.kind === "card") {
        const used = await page.evaluate(() => document.fonts.check('600 64px "Instrument Sans"'));
        if (!used) throw new Error(`${entry.file}: Instrument Sans did not load; refusing a fallback-font card`);
        const overflow = await page.evaluate((right) => {
          const bad = [];
          for (const node of document.querySelectorAll("text[data-slot] tspan, text[data-slot]")) {
            const box = node.getBoundingClientRect();
            if (box.right > right) bad.push(`${node.textContent.trim()} (${Math.round(box.right)}px)`);
          }
          return bad;
        }, SAFE_RIGHT);
        if (overflow.length) {
          throw new Error(`${entry.file}: text crosses the ${SAFE_RIGHT}px safe edge: ${overflow.join("; ")}`);
        }
      }
      const png = await page.screenshot({
        clip: { x: 0, y: 0, width: entry.width, height: entry.height },
        omitBackground: entry.kind === "icon",
      });
      mkdirSync(dirname(join(WEBAPP, entry.file)), { recursive: true });
      writeFileSync(join(WEBAPP, entry.file), png);
      manifest.push({
        file: entry.file,
        width: entry.width,
        height: entry.height,
        source: sourceHash(entry),
        sha256: sha256(png),
      });
      console.log(`build-social-cards: wrote ${entry.file} (${entry.width}x${entry.height}, ${png.length} B)`);
    }
  } finally {
    await browser.close();
  }
  writeFileSync(join(WEBAPP, MANIFEST), `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  console.log(`build-social-cards: wrote ${MANIFEST}`);
  return 0;
}

/** Every reason the committed images are not what the sources say, as sentences. */
export function problems() {
  let manifest;
  try {
    manifest = JSON.parse(readWebapp(MANIFEST).toString("utf8"));
  } catch (error) {
    return [`${MANIFEST} is missing or unreadable (${error.message})`];
  }
  const found = [];
  const expected = entries();
  const files = (list) => list.map((entry) => entry.file).join(", ");
  if (files(manifest) !== files(expected)) {
    found.push(`${MANIFEST} lists [${files(manifest)}] but the generator owns [${files(expected)}]`);
  }
  for (const entry of expected) {
    const recorded = manifest.find((item) => item.file === entry.file);
    if (!recorded) continue;
    if (recorded.source !== sourceHash(entry)) {
      found.push(`${entry.file} was rendered from a different source than the committed one`);
    }
    let bytes;
    try {
      bytes = readWebapp(entry.file);
    } catch {
      found.push(`${entry.file} is not committed`);
      continue;
    }
    if (sha256(bytes) !== recorded.sha256) found.push(`${entry.file} is not the PNG the generator wrote`);
    const size = pngSize(bytes);
    if (size.width !== entry.width || size.height !== entry.height) {
      found.push(`${entry.file} is ${size.width}x${size.height}, not ${entry.width}x${entry.height}`);
    }
    if (recorded.width !== size.width || recorded.height !== size.height) {
      found.push(`${MANIFEST} records the wrong size for ${entry.file}`);
    }
  }
  return found;
}

function check() {
  const found = problems();
  if (found.length) {
    console.error(
      `build-social-cards --check: the link-preview images are stale.\n  - ${found.join("\n  - ")}\n` +
        "A card's words, its template, the font or the mark changed without the images " +
        "being regenerated, or a PNG was replaced by hand.\n" +
        "Run `node tools/build-social-cards.mjs --render` (needs Playwright's Chromium) and commit the result.",
    );
    return 1;
  }
  console.log(`build-social-cards --check: ${entries().length} image(s) match their sources.`);
  return 0;
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.includes("--render")) process.exit(await render());
  process.exit(check());
}
