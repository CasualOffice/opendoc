// Host-owned web font manifest — served from OUR OWN ORIGIN, hash-verified.
//
// These are the faces the editor fetches at runtime and registers into the
// document engine through `registerFont`/`registerFallbackFont`. They are
// deliberately not imported by Rust and therefore cannot become part of the
// WASM binary; the engine's bundled base (Roboto, Caladea, Carlito, the
// Liberation families and the monochrome Noto Emoji) stays small, which is the
// owner's font-provisioning decision: desktop uses OS system fonts, the browser
// fetches faces over the network, a bundled base is always there, and ONE
// dynamic registry seam serves both.
//
// ## Why the origin matters (`109` HF-176)
//
// Every face here used to be fetched from `cdn.jsdelivr.net`. That is a
// third-party runtime dependency for *document rendering*, and it cost three
// things at once:
//
//   1. **Local-first.** A deployment behind a firewall, on an air-gapped
//      network, or simply with jsdelivr blocked could not render a CJK, Arabic,
//      Indic or Thai document at all — and the demo fixture's own ☐/☒ checklist
//      markers needed a third-party request to draw.
//   2. **Supply chain.** The URLs were pinned to an upstream commit, which
//      fixes *which revision* is asked for but not *what bytes come back*. There
//      was no integrity check anywhere: whatever the CDN returned was handed
//      straight to the shaper.
//   3. **Privacy.** Every reader's IP, and a profile of which scripts their
//      documents contain, went to a third party on open.
//
// `tests/chrome_fonts.test.mjs` has long asserted that the *chrome* fonts carry
// no `fonts.googleapis.com`/`fonts.gstatic.com` reference. It did not catch this
// because jsdelivr is neither of those hosts and because document fonts are not
// chrome fonts. `tests/web_fonts.test.mjs` went further and actively *asserted*
// the jsdelivr URLs, so the guard pinned the defect in place.
//
// ## The shape now
//
// Each face declares a `file` under `FONT_ORIGIN` — this module's own origin, so
// it resolves correctly for the editor, for the GitHub Pages site, and for an
// embedder serving the directory from anywhere — plus its exact `bytes` and
// `sha256`.
//
// `local: true` means the file is committed to this repository and served from
// our origin with no network third party involved at all — 20 of the 24 faces,
// covering every script but CJK and colour emoji. `local: false` means the file
// is too large to commit (`selfHostingBytes()` has the figures) and is fetched
// from a pinned upstream `mirror` until a deployment provisions it locally with
// `tools/provision-script-fonts.mjs` and declares it through
// `OPENDOC_PROVISIONED_FONTS`.
//
// Either way the bytes are verified against `sha256` before they reach the
// engine, so a mirror cannot substitute a different face and a truncated
// download is refused rather than registered.
//
// A face is asked of our own origin whenever we actually have it, and of its
// mirror otherwise — never speculatively. An earlier version of this module
// asked our origin first for everything and fell back on failure, which reads
// as the stronger rule and is not: on an unprovisioned checkout it cost one
// failed request and one console error per mirrored face. See
// `declaredProvisioned`.

/** Pinned upstream revisions. Used to build `mirror` URLs for the faces that
 *  are not committed, and kept here so the provisioning tool and the manifest
 *  cannot drift apart.
 *
 * `GOOGLE_FONTS_REVISION` no longer builds a mirror — the named Latin faces it
 * used to point at are committed now — but it is the provenance of those files
 * and is kept so a re-fetch is reproducible. */
export const GOOGLE_FONTS_REVISION = "7ff85c87f93ea6cca5f41c69f2e4edcb90240f26";
export const NOTO_EMOJI_REVISION = "8998f5dd683424a73e2314a8c1f1e359c19e8742";
export const NOTO_CJK_REVISION = "f8d157532fbfaeda587e826d4cd5b21a49186f7c";
export const NOTO_DISTRIBUTION_REVISION =
  "eaa1a5cf8cb83ea73941197e492d659e51bb11dd";

const NOTO_EMOJI = `https://cdn.jsdelivr.net/gh/googlefonts/noto-emoji@${NOTO_EMOJI_REVISION}/fonts`;
const NOTO_CJK = `https://cdn.jsdelivr.net/gh/notofonts/noto-cjk@${NOTO_CJK_REVISION}/Sans/OTF`;

/** Where our own copies live, resolved against this module so it is correct for
 *  `editor.html`, for the site, and for an embedder at any path depth. */
export const FONT_ORIGIN = new URL("../assets/fonts/script/", import.meta.url)
  .href;

/** Whether this deployment has provisioned the non-committed faces into
 *  `assets/fonts/script/`, declared by the host as either `true` (all of them)
 *  or an array of filenames.
 *
 * This exists because asking our own origin for a file we know is not in the
 * repository is a guaranteed 404. The first version of this module did exactly
 * that — primary URL always our origin, fall back to the mirror on failure —
 * and it cost one failed request per mirrored face per session plus a console
 * error, which reddened `emoji-rendering.spec.mjs`'s `consoleErrors` guard the
 * moment it was run against an UNPROVISIONED checkout. That is the state CI and
 * a fresh clone are in, so the regression would have shipped as "works on the
 * machine that happened to have the fonts".
 *
 * So the rule is: our origin is asked first for every face this repository
 * actually has, and for the rest only when the deployment says it has them.
 * Nothing speculates.
 *
 *     <script>window.OPENDOC_PROVISIONED_FONTS = true;</script>
 *
 * before the editor's module graph, on a deployment that has run
 * `tools/provision-script-fonts.mjs`. See `assets/fonts/script/README.md`.
 */
function declaredProvisioned(file) {
  const declared = globalThis.OPENDOC_PROVISIONED_FONTS;
  if (declared === true) return true;
  return Array.isArray(declared) && declared.includes(file);
}

/** The PRIMARY url for a face — the one `main.js` reads to fetch, and to name a
 *  face in a warning.
 *
 * A committed face: always our own origin. A mirrored one: our origin when the
 * deployment has declared it provisioned, and the pinned upstream otherwise,
 * because that is where the file truthfully is.
 *
 * Exported so a test can exercise both branches without reloading the module.
 */
export function primaryUrlFor({ file, mirror }) {
  return mirror === null || mirror === undefined || declaredProvisioned(file)
    ? new URL(file, FONT_ORIGIN).href
    : mirror;
}

// `url` is STORED, not an accessor: every entry below spreads its face into a
// bucket object, and a spread copies an accessor's value rather than carrying
// the accessor. So the global is read once, at module evaluation. A host sets it
// from a classic `<script>`, which always runs before a module graph evaluates,
// so that ordering is the normal case rather than a constraint.
const face = (file, bytes, sha256, mirror) =>
  Object.freeze({
    file,
    bytes,
    sha256,
    local: mirror === null,
    mirror,
    url: primaryUrlFor({ file, mirror }),
  });

const local = (file, bytes, sha256) => face(file, bytes, sha256, null);

const mirrored = (file, bytes, sha256, mirror) =>
  face(file, bytes, sha256, mirror);

/** Named Latin/Greek/Cyrillic families provisioned before the first paint.
 *
 * Two variable faces cover the complete upright/italic weight family without
 * downloading a separate static file per weight.
 *
 * These are the only faces fetched eagerly, so they are the ones a reader waits
 * on — which is exactly why they are COMMITTED (9.28 MB) rather than mirrored.
 * A mirrored face costs a 404 against our own origin and then a third-party
 * round trip, on every single editor load, for every reader, whatever script
 * their document is in. The CJK faces are only fetched by CJK documents; these
 * are fetched by all of them.
 *
 * The editor still paints from the bundled metric-compatible substitutes first
 * and upgrades when these land, which is what `tests/e2e/first-paint.spec.mjs`
 * holds. Provenance: `google/fonts@${GOOGLE_FONTS_REVISION}/ofl`. All three
 * families are SIL OFL 1.1 — including Roboto, which Google Fonts relicensed
 * from Apache-2.0; the engine's BUNDLED Roboto in
 * `crates/casual-doc-layout/fonts/` is the older Apache-2.0 build and a
 * different file. Licence texts sit beside the fonts in
 * `assets/fonts/script/`.
 */
export const NAMED_WEB_FONT_FACES = Object.freeze([
  Object.freeze({
    family: "Roboto",
    style: "normal",
    ...local(
      "Roboto-Variable.ttf",
      488_584,
      "d7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134",
    ),
  }),
  Object.freeze({
    family: "Roboto",
    style: "italic",
    ...local(
      "Roboto-Italic-Variable.ttf",
      530_944,
      "9725a847af6b460ffca162ae66d20dad48b01876137947180b42d7dcd7887182",
    ),
  }),
  Object.freeze({
    family: "Noto Sans",
    style: "normal",
    ...local(
      "NotoSans-Variable.ttf",
      2_049_096,
      "bfb7bb691513f12e734dc346c03a03f784912432d7e3fa8e56efcf906fe86b3d",
    ),
  }),
  Object.freeze({
    family: "Noto Sans",
    style: "italic",
    ...local(
      "NotoSans-Italic-Variable.ttf",
      2_322_640,
      "58e6e0ebd1931b29a365aa2d3e2ee9a9e831a3af7cf3ad1462d4e72154f0b291",
    ),
  }),
  Object.freeze({
    family: "Noto Serif",
    style: "normal",
    ...local(
      "NotoSerif-Variable.ttf",
      1_887_192,
      "4d8e6761424656867019081a1a01336f3cb086982682698714054fc33f782713",
    ),
  }),
  Object.freeze({
    family: "Noto Serif",
    style: "italic",
    ...local(
      "NotoSerif-Italic-Variable.ttf",
      2_448_496,
      "e87acbc6c0efd0d9a20d6a8cbbda2b266c14be3a3a6f5af8ec9d7b2460570ad1",
    ),
  }),
]);

/** Coverage-driven script fallbacks. CJK files are intentionally not eager:
 * each is large, and `missingCoverage()` tells us whether one is needed.
 *
 * `scripts` are ISO 15924 codes handed to `registerFallbackFont`, which wires
 * the face into `fontique`'s per-script fallback chain. Selection is therefore
 * by **coverage**, not by family name: the shaper asks the chain for a face that
 * covers the scalar, and `casual-doc-layout/tests/cjk_coverage.rs` proves the
 * chosen face's own `cmap` maps it.
 */
export const SCRIPT_FALLBACK_FONTS = Object.freeze({
  // --- CJK ---------------------------------------------------------------
  // ~16.4 MB each, so mirrored rather than committed. One of these is what
  // HF-176 is about: two corpus documents carry 9,576 CJK characters, 53% of
  // each document's text.
  jp: Object.freeze({
    ...mirrored(
      "NotoSansCJKjp-Regular.otf",
      16_467_736,
      "68a3fc98800b2a27b371f2fb79991daf3633bd89309d4ffaa6946fd587f375b5",
      `${NOTO_CJK}/Japanese/NotoSansCJKjp-Regular.otf`,
    ),
    scripts: Object.freeze(["Hani", "Hira", "Kana"]),
  }),
  kr: Object.freeze({
    ...mirrored(
      "NotoSansCJKkr-Regular.otf",
      16_433_112,
      "6bcb2a0703aa137e874fc2dffa85f6c21ba9a67fa329e81b8c801663af7e992a",
      `${NOTO_CJK}/Korean/NotoSansCJKkr-Regular.otf`,
    ),
    scripts: Object.freeze(["Hani", "Hang"]),
  }),
  sc: Object.freeze({
    ...mirrored(
      "NotoSansCJKsc-Regular.otf",
      16_437_364,
      "2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b",
      `${NOTO_CJK}/SimplifiedChinese/NotoSansCJKsc-Regular.otf`,
    ),
    scripts: Object.freeze(["Hani"]),
  }),

  // --- Committed, fully local-first ---------------------------------------
  // All 14 of these together are 2.39 MB, so there is no reason for any of them
  // to involve a third party. Arabic and Hebrew render right-to-left and the
  // Indic faces carry the GSUB/GPOS tables their scripts need; see the
  // SHAPING note at the bottom of this file for what that does and does not
  // mean for correctness.
  arabic: Object.freeze({
    ...local(
      "NotoSansArabic-Regular.ttf",
      234_892,
      "bdff3e5659d67e67def05b33f749683b9376ae819d65d3dd62ac4640b3aaef48",
    ),
    scripts: Object.freeze(["Arab"]),
  }),
  devanagari: Object.freeze({
    ...local(
      "NotoSansDevanagari-Regular.ttf",
      244_284,
      "306b53ecfb182a504dd8a7446093c316387d2fd8dc350d0792ed1753fe0996cd",
    ),
    scripts: Object.freeze(["Deva"]),
  }),
  bengali: Object.freeze({
    ...local(
      "NotoSansBengali-Regular.ttf",
      143_072,
      "b55c62ee531e3214da6c0701daecea89a52ba42db7d8206b92e6b51f397a3193",
    ),
    scripts: Object.freeze(["Beng"]),
  }),
  gurmukhi: Object.freeze({
    ...local(
      "NotoSansGurmukhi-Regular.ttf",
      55_172,
      "658d0207da305a1411c539a8b0bbeda64d4146e54fb4827facddb890b6b90d74",
    ),
    scripts: Object.freeze(["Guru"]),
  }),
  gujarati: Object.freeze({
    ...local(
      "NotoSansGujarati-Regular.ttf",
      200_704,
      "9b5a7aaeeb649a2e75a49d8b006a1f87db1b61c0df3b001609f4e0725d88dbf6",
    ),
    scripts: Object.freeze(["Gujr"]),
  }),
  oriya: Object.freeze({
    ...local(
      "NotoSansOriya-Regular.ttf",
      131_216,
      "a16645d056017927406546aa78e4ce15e782fd8783467267b75450453d007415",
    ),
    scripts: Object.freeze(["Orya"]),
  }),
  tamil: Object.freeze({
    ...local(
      "NotoSansTamil-Regular.ttf",
      73_992,
      "3c0a186feb3c63c7f6d63e1511dcdc144e745ae09b98e217c83f3e317974f6f9",
    ),
    scripts: Object.freeze(["Taml"]),
  }),
  telugu: Object.freeze({
    ...local(
      "NotoSansTelugu-Regular.ttf",
      235_176,
      "b274780b69d1d23fe84b55e809a152cb2ac5306d33864b1f87622f6971871aae",
    ),
    scripts: Object.freeze(["Telu"]),
  }),
  kannada: Object.freeze({
    ...local(
      "NotoSansKannada-Regular.ttf",
      182_416,
      "9ad74dc64838c6855b96f671fc08e425a58921b9d0c71712ea79c328a27e6e38",
    ),
    scripts: Object.freeze(["Knda"]),
  }),
  malayalam: Object.freeze({
    ...local(
      "NotoSansMalayalam-Regular.ttf",
      112_936,
      "c08de7fa8d032a5d6a4d120fb82c78cec60b362a4e73fa26360d89759ff2a7f9",
    ),
    scripts: Object.freeze(["Mlym"]),
  }),
  sinhala: Object.freeze({
    ...local(
      "NotoSansSinhala-Regular.ttf",
      154_912,
      "9e32612d47004552f3125e78648a9e2e7899a216ccd3cefbb93a9b5f4c809feb",
    ),
    scripts: Object.freeze(["Sinh"]),
  }),
  hebrew: Object.freeze({
    ...local(
      "NotoSansHebrew-Regular.ttf",
      26_860,
      "cdefaf8efd47045f6820928eba84db5bed7557539328952b5f828315485e02ee",
    ),
    scripts: Object.freeze(["Hebr"]),
  }),
  thai: Object.freeze({
    ...local(
      "NotoSansThai-Regular.ttf",
      37_780,
      "61cf814eec46b294d6ea4401ac295d0cecd5207bd2331dcc5a15e7301d30ee44",
    ),
    scripts: Object.freeze(["Thai"]),
  }),
  // Bucket for the Geometric Shapes / Miscellaneous Symbols / Dingbats
  // blocks. These scalars have the Unicode Common script (`Zyyy`), but Parley
  // resolves Common characters beside Latin text into a Latin shaping run.
  // Register both keys so standalone symbols and symbols embedded in labels
  // can select the covering face. Noto Sans, the network-fetched face this
  // table registers for Latin COVERAGE (it is not a metric substitute for
  // anything — Calibri's metric partner is the bundled Carlito, chosen in
  // `font_substitution.rs`; calling it "the Calibri substitute" here cost one
  // investigation an afternoon), doesn't cover these at all, so plain content
  // like a "☐"/"□" checklist placeholder
  // (as in the sample fixture's acceptance-checklist and Result-column
  // cells) tofu'd. Noto Sans Symbols 2 is the intended monochrome fallback
  // for exactly this range (full pictographic/color emoji are a separate,
  // much larger gap — see fontKeyForCodePoint's emoji comment).
  //
  // Committed, because the demo fixture's own checklist needs it: before this,
  // the editor's first screen asked a third party for its checkbox glyphs.
  symbols: Object.freeze({
    ...local(
      "NotoSansSymbols2-Regular.ttf",
      671_568,
      "c4a0a80f0041ce4be81e2478faad22776d23edb98ae3f0d19bd37044820ecf9d",
    ),
    scripts: Object.freeze(["Zyyy", "Latn"]),
  }),

  // --- Colour emoji -------------------------------------------------------
  // Pictographic emoji, IN COLOUR. The engine bundles the monochrome Noto
  // Emoji base, so an emoji is never tofu and never waits on the network; this
  // bucket is the colour upgrade on top of it.
  //
  // It is the COLRv1 build, not the CBDT one. Both are official Noto Color
  // Emoji and the renderer paints either (`render_colr_glyph` for COLR,
  // `render_bitmap_glyph` for CBDT/sbix strikes), but they are not the same
  // download: the CBDT file is 10.87 MB of PNG strikes that compress to 9.24 MB
  // brotli, while the COLRv1 file is 4.99 MB of vector paint graphs that
  // compress to 2.47 MB — a 6.8 MB saving on every document with an emoji in
  // it, and resolution-independent instead of a 109 px bitmap scaled to body
  // size. Do NOT substitute `ofl/notoemoji/NotoEmoji[wght].ttf` here: that is
  // the monochrome face, which is the thing we already bundle.
  //
  // Registered through the same host seam as every other bucket. The engine
  // puts a face carrying colour glyphs at the FRONT of the fallback chain, so
  // this outranks both the bundled monochrome base and the monochrome symbols
  // face — which is what stops an emoji-presentation heart painting as a
  // dingbat because the symbols bucket happened to be fetched first.
  //
  // Like every bucket here it is coverage-driven: `missingCoverage()` only asks
  // for it when the open document actually contains these scalars (including
  // ones the monochrome base is standing in for), so documents without emoji
  // never pay for it.
  emoji: Object.freeze({
    ...mirrored(
      "Noto-COLRv1.ttf",
      4_991_984,
      "0ae57fe58645638523ba35f388d93739d292539a9acb84df5700c81b1e1a28d2",
      `${NOTO_EMOJI}/Noto-COLRv1.ttf`,
    ),
    scripts: Object.freeze(["Zyyy", "Latn", "Zinh"]),
  }),
});

/** Every face in the manifest, named faces and script fallbacks together. */
export function allManifestFaces() {
  return [
    ...NAMED_WEB_FONT_FACES.map((face) => ({
      key: `named:${face.family}/${face.style}`,
      ...face,
    })),
    ...Object.entries(SCRIPT_FALLBACK_FONTS).map(([key, font]) => ({
      key,
      ...font,
    })),
  ];
}

/** The URL this face is served from on our own origin. */
export function localUrlFor(face) {
  return new URL(face.file, FONT_ORIGIN).href;
}

/** The cost of each provisioning group, **derived from the manifest** so the
 *  "bundled base stays small" decision can be re-made with real numbers.
 *
 * Hand-writing these was wrong the first time it was tried here: the mirrored
 * total was typed as 59,065,736 against an actual 64,057,148, and nothing would
 * have caught it. Counts in this repository are derived, not maintained.
 *
 * None of these bytes are in the bundle. Every face in the manifest is fetched
 * on demand — the named Latin faces once per editor load, the script fallbacks
 * only when `missingCoverage()` reports a scalar that needs one — so these are
 * repository and deployed-origin weight, never first-paint weight.
 */
export function selfHostingBytes() {
  let committed = 0;
  let mirrored = 0;
  let committedFaces = 0;
  let mirroredFaces = 0;
  for (const face of allManifestFaces()) {
    if (face.local) {
      committed += face.bytes;
      committedFaces += 1;
    } else {
      mirrored += face.bytes;
      mirroredFaces += 1;
    }
  }
  return Object.freeze({
    committed,
    mirrored,
    committedFaces,
    mirroredFaces,
    total: committed + mirrored,
  });
}

/** Every URL a face may legitimately be served from → its manifest entry, so a
 *  fetch is verified against what the manifest says about *that* face whichever
 *  origin answered. */
const BY_URL = new Map();
for (const entry of allManifestFaces()) {
  BY_URL.set(localUrlFor(entry), entry);
  if (entry.mirror) BY_URL.set(entry.mirror, entry);
}

/** Which script fallback bucket (if any) covers a Unicode scalar. */
export function fontKeyForCodePoint(cp) {
  if ((cp >= 0x3040 && cp <= 0x30ff) || (cp >= 0x31f0 && cp <= 0x31ff))
    return "jp";
  if (
    (cp >= 0xac00 && cp <= 0xd7a3) ||
    (cp >= 0x1100 && cp <= 0x11ff) ||
    (cp >= 0x3130 && cp <= 0x318f)
  )
    return "kr";
  if (
    (cp >= 0x4e00 && cp <= 0x9fff) ||
    (cp >= 0x3400 && cp <= 0x4dbf) ||
    (cp >= 0xf900 && cp <= 0xfaff)
  )
    return "sc";
  if (cp >= 0x0600 && cp <= 0x06ff) return "arabic";
  if (cp >= 0x0900 && cp <= 0x097f) return "devanagari";
  if (cp >= 0x0980 && cp <= 0x09ff) return "bengali";
  if (cp >= 0x0a00 && cp <= 0x0a7f) return "gurmukhi";
  if (cp >= 0x0a80 && cp <= 0x0aff) return "gujarati";
  if (cp >= 0x0b00 && cp <= 0x0b7f) return "oriya";
  if (cp >= 0x0b80 && cp <= 0x0bff) return "tamil";
  if (cp >= 0x0c00 && cp <= 0x0c7f) return "telugu";
  if (cp >= 0x0c80 && cp <= 0x0cff) return "kannada";
  if (cp >= 0x0d00 && cp <= 0x0d7f) return "malayalam";
  if (cp >= 0x0d80 && cp <= 0x0dff) return "sinhala";
  if (cp >= 0x0590 && cp <= 0x05ff) return "hebrew";
  if (cp >= 0x0e00 && cp <= 0x0e7f) return "thai";
  // The two joiners that turn other scalars into an emoji: U+FE0F asks for
  // emoji presentation and U+200D joins a sequence. They are reported as part
  // of the cluster they sit in, so seeing either means the document holds an
  // emoji-presentation sequence or a ZWJ sequence and wants the colour face.
  //
  // Without this the emoji-presentation heart (U+2764 U+FE0F) fell to the
  // `symbols` bucket below on its base scalar alone — Noto Sans Symbols 2
  // covers U+2764 as a monochrome dingbat, so a heart painted grey, the colour
  // face was never asked for, and nothing said why. U+2764 on its OWN is a
  // dingbat by Unicode default and still belongs to `symbols`; it is the
  // selector that changes the answer.
  if (cp === 0xfe0f || cp === 0x200d) return "emoji";
  // Geometric Shapes / Miscellaneous Symbols / Dingbats. Noto Sans Symbols 2
  // covers these monochrome shapes, and they are what a checklist's ☐/☒ markers
  // and similar plain content use.
  if (
    (cp >= 0x25a0 && cp <= 0x25ff) ||
    (cp >= 0x2600 && cp <= 0x26ff) ||
    (cp >= 0x2700 && cp <= 0x27bf)
  )
    return "symbols";
  // Pictographic emoji, in the astral plane, plus the two BMP blocks that only
  // carry emoji in practice. Every block here is covered by the colour face
  // registered above; before the bucket existed these all resolved to no font
  // at all and rendered as notdef boxes in any document that contained them.
  if (
    (cp >= 0x1f300 && cp <= 0x1f5ff) || // Misc Symbols and Pictographs
    (cp >= 0x1f600 && cp <= 0x1f64f) || // Emoticons
    (cp >= 0x1f680 && cp <= 0x1f6ff) || // Transport and Map
    (cp >= 0x1f700 && cp <= 0x1f77f) || // Alchemical Symbols
    (cp >= 0x1f780 && cp <= 0x1f7ff) || // Geometric Shapes Extended
    (cp >= 0x1f900 && cp <= 0x1f9ff) || // Supplemental Symbols and Pictographs
    (cp >= 0x1fa70 && cp <= 0x1faff) || // Symbols and Pictographs Extended-A
    (cp >= 0x1f1e6 && cp <= 0x1f1ff) || // Regional indicators (flag pairs)
    (cp >= 0x1f000 && cp <= 0x1f0ff) || // Mahjong / dominoes / playing cards
    (cp >= 0x1f200 && cp <= 0x1f2ff) // Enclosed ideographic supplement
  )
    return "emoji";
  return null;
}

/** Unique fallback buckets needed for the reported scalars. */
export function fallbackKeysFor(codePoints) {
  const keys = new Set();
  for (const cp of codePoints) {
    const key = fontKeyForCodePoint(cp);
    if (key) keys.add(key);
  }
  // Japanese and Korean region faces both include Han.
  if (keys.has("jp") || keys.has("kr")) keys.delete("sc");
  return [...keys];
}

/** Lowercase hex SHA-256 of `bytes`, or `null` where WebCrypto is unavailable
 *  (a page served over plain HTTP to something other than localhost has no
 *  `crypto.subtle` at all). */
async function sha256Hex(bytes) {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) return null;
  const digest = await subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)]
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/** Verifies `bytes` against the manifest entry for `url`.
 *
 * Throws rather than returning a flag: unverified font bytes must never reach
 * `registerFont`, and a caller that forgets to check a boolean is exactly how
 * that would happen.
 *
 * Where the manifest has no entry for the URL (a host passing its own face
 * through this helper) there is nothing to verify against and the bytes pass.
 * Where WebCrypto is missing, a **mirrored** face is refused — those are the
 * bytes a third party supplies, so they are the ones that must be checked —
 * while a face served from our own origin is allowed through, because its
 * integrity already rests on the same origin that served the page.
 */
async function verifyFontBytes(url, bytes) {
  const face = BY_URL.get(url);
  if (!face) return;
  if (bytes.length !== face.bytes) {
    throw new Error(
      `${face.file}: expected ${face.bytes} bytes, got ${bytes.length}`,
    );
  }
  const actual = await sha256Hex(bytes);
  if (actual === null) {
    if (face.local) return;
    throw new Error(
      `${face.file}: cannot verify a mirrored face without WebCrypto`,
    );
  }
  if (actual !== face.sha256) {
    throw new Error(
      `${face.file}: SHA-256 mismatch (expected ${face.sha256}, got ${actual})`,
    );
  }
}

/** Fetches, verifies and memoizes one immutable font asset.
 *
 * `url` is the manifest's primary URL. Our own origin is always tried first;
 * for a face that is not committed, a failure there falls back to the pinned
 * upstream mirror, so a deployment that has run the provisioning tool never
 * reaches a third party and one that has not still renders. Both paths are
 * hash-verified before the bytes are returned.
 */
export async function fetchFontBytes(url, cache, fetchImpl = fetch) {
  const cached = cache.get(url);
  if (cached) return cached;

  const entry = BY_URL.get(url);
  const candidates = [url];
  // The other legitimate candidate, if there is one. A mirrored face asked for
  // on our origin falls back to the mirror; one asked for on the mirror (the
  // default, when this deployment has not provisioned it) falls back to our
  // origin, in case it is there after all. Both are hash-verified.
  if (entry?.mirror) {
    const other = url === entry.mirror ? localUrlFor(entry) : entry.mirror;
    if (other !== url) candidates.push(other);
  }

  let lastError;
  for (const candidate of candidates) {
    try {
      const response = await fetchImpl(candidate);
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const bytes = new Uint8Array(await response.arrayBuffer());
      if (bytes.length === 0) throw new Error("empty font response");
      await verifyFontBytes(candidate, bytes);
      cache.set(url, bytes);
      return bytes;
    } catch (err) {
      lastError = err;
    }
  }
  throw lastError ?? new Error(`${url}: no candidate succeeded`);
}

/** Packs separate blobs for the bounded `registerFonts(bytes, lengths)` ABI. */
export function packFontBytes(blobs) {
  const total = blobs.reduce((sum, bytes) => sum + bytes.length, 0);
  const bytes = new Uint8Array(total);
  const lengths = [];
  let offset = 0;
  for (const blob of blobs) {
    bytes.set(blob, offset);
    lengths.push(blob.length);
    offset += blob.length;
  }
  return { bytes, lengths };
}

// --- SHAPING: what provisioning a face does and does not fix ---------------
//
// Registering a covering face fixes *coverage* — the scalar draws as itself
// instead of as a notdef box. That is the whole of the CJK problem, because CJK
// is not a complex script: each ideograph, kana and precomposed Hangul syllable
// is one cluster with one glyph and no reordering.
//
// It is NOT the whole of the Arabic or Indic problem. Those scripts need
// *shaping*: contextual joining forms for Arabic, reordering and conjunct
// formation for the Indic scripts. The engine shapes through `harfrust` by way
// of `parley`, and the Noto faces committed here carry the GSUB/GPOS tables
// that shaping reads, so the mechanism is present rather than stubbed — but
// this lane measured coverage, not shaping correctness, and there is no
// shaped-output guard for either family in the tree. Treat Arabic and Indic as
// "draws real glyphs, shaping unverified" and not as "supported", and do not
// quote this file as evidence that they are.
