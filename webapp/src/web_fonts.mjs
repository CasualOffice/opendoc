// Host-owned web font manifest.
//
// These immutable, commit-pinned OpenType files are fetched by the editor and
// registered into the document engine. They are deliberately not imported by
// Rust and therefore cannot become part of the WASM binary.

export const GOOGLE_FONTS_REVISION = "7ff85c87f93ea6cca5f41c69f2e4edcb90240f26";
export const NOTO_CJK_REVISION = "f8d157532fbfaeda587e826d4cd5b21a49186f7c";
export const NOTO_DISTRIBUTION_REVISION =
  "eaa1a5cf8cb83ea73941197e492d659e51bb11dd";

const GOOGLE_FONTS = `https://cdn.jsdelivr.net/gh/google/fonts@${GOOGLE_FONTS_REVISION}/ofl`;
const NOTO_CJK = `https://cdn.jsdelivr.net/gh/notofonts/noto-cjk@${NOTO_CJK_REVISION}/Sans/OTF`;
const NOTO = `https://cdn.jsdelivr.net/gh/notofonts/notofonts.github.io@${NOTO_DISTRIBUTION_REVISION}/fonts`;

const variableFace = (family, style, path) =>
  Object.freeze({ family, style, url: `${GOOGLE_FONTS}/${path}` });

/** Named Latin/Greek/Cyrillic families provisioned before the first paint.
 *
 * Two variable faces cover the complete upright/italic weight family without
 * downloading a separate static file per weight.
 */
export const NAMED_WEB_FONT_FACES = Object.freeze([
  variableFace("Roboto", "normal", "roboto/Roboto%5Bwdth,wght%5D.ttf"),
  variableFace("Roboto", "italic", "roboto/Roboto-Italic%5Bwdth,wght%5D.ttf"),
  variableFace("Noto Sans", "normal", "notosans/NotoSans%5Bwdth,wght%5D.ttf"),
  variableFace(
    "Noto Sans",
    "italic",
    "notosans/NotoSans-Italic%5Bwdth,wght%5D.ttf",
  ),
  variableFace(
    "Noto Serif",
    "normal",
    "notoserif/NotoSerif%5Bwdth,wght%5D.ttf",
  ),
  variableFace(
    "Noto Serif",
    "italic",
    "notoserif/NotoSerif-Italic%5Bwdth,wght%5D.ttf",
  ),
]);

/** Coverage-driven script fallbacks. CJK files are intentionally not eager:
 * each is large, and `missingCoverage()` tells us whether one is needed.
 */
export const SCRIPT_FALLBACK_FONTS = Object.freeze({
  jp: Object.freeze({
    url: `${NOTO_CJK}/Japanese/NotoSansCJKjp-Regular.otf`,
    scripts: Object.freeze(["Hani", "Hira", "Kana"]),
  }),
  kr: Object.freeze({
    url: `${NOTO_CJK}/Korean/NotoSansCJKkr-Regular.otf`,
    scripts: Object.freeze(["Hani", "Hang"]),
  }),
  sc: Object.freeze({
    url: `${NOTO_CJK}/SimplifiedChinese/NotoSansCJKsc-Regular.otf`,
    scripts: Object.freeze(["Hani"]),
  }),
  arabic: Object.freeze({
    url: `${NOTO}/NotoSansArabic/hinted/ttf/NotoSansArabic-Regular.ttf`,
    scripts: Object.freeze(["Arab"]),
  }),
  devanagari: Object.freeze({
    url: `${NOTO}/NotoSansDevanagari/hinted/ttf/NotoSansDevanagari-Regular.ttf`,
    scripts: Object.freeze(["Deva"]),
  }),
  bengali: Object.freeze({
    url: `${NOTO}/NotoSansBengali/hinted/ttf/NotoSansBengali-Regular.ttf`,
    scripts: Object.freeze(["Beng"]),
  }),
  gurmukhi: Object.freeze({
    url: `${NOTO}/NotoSansGurmukhi/hinted/ttf/NotoSansGurmukhi-Regular.ttf`,
    scripts: Object.freeze(["Guru"]),
  }),
  gujarati: Object.freeze({
    url: `${NOTO}/NotoSansGujarati/hinted/ttf/NotoSansGujarati-Regular.ttf`,
    scripts: Object.freeze(["Gujr"]),
  }),
  oriya: Object.freeze({
    url: `${NOTO}/NotoSansOriya/hinted/ttf/NotoSansOriya-Regular.ttf`,
    scripts: Object.freeze(["Orya"]),
  }),
  tamil: Object.freeze({
    url: `${NOTO}/NotoSansTamil/hinted/ttf/NotoSansTamil-Regular.ttf`,
    scripts: Object.freeze(["Taml"]),
  }),
  telugu: Object.freeze({
    url: `${NOTO}/NotoSansTelugu/hinted/ttf/NotoSansTelugu-Regular.ttf`,
    scripts: Object.freeze(["Telu"]),
  }),
  kannada: Object.freeze({
    url: `${NOTO}/NotoSansKannada/hinted/ttf/NotoSansKannada-Regular.ttf`,
    scripts: Object.freeze(["Knda"]),
  }),
  malayalam: Object.freeze({
    url: `${NOTO}/NotoSansMalayalam/hinted/ttf/NotoSansMalayalam-Regular.ttf`,
    scripts: Object.freeze(["Mlym"]),
  }),
  sinhala: Object.freeze({
    url: `${NOTO}/NotoSansSinhala/hinted/ttf/NotoSansSinhala-Regular.ttf`,
    scripts: Object.freeze(["Sinh"]),
  }),
  hebrew: Object.freeze({
    url: `${NOTO}/NotoSansHebrew/hinted/ttf/NotoSansHebrew-Regular.ttf`,
    scripts: Object.freeze(["Hebr"]),
  }),
  thai: Object.freeze({
    url: `${NOTO}/NotoSansThai/hinted/ttf/NotoSansThai-Regular.ttf`,
    scripts: Object.freeze(["Thai"]),
  }),
  // Bucket for the Geometric Shapes / Miscellaneous Symbols / Dingbats
  // blocks. These scalars have the Unicode Common script (`Zyyy`), but Parley
  // resolves Common characters beside Latin text into a Latin shaping run.
  // Register both keys so standalone symbols and symbols embedded in labels
  // can select the covering face. Noto Sans (the Calibri/system-font
  // substitute) doesn't cover these at all, so plain content like a "☐"/"□"
  // checklist placeholder
  // (as in the sample fixture's acceptance-checklist and Result-column
  // cells) tofu'd. Noto Sans Symbols 2 is the intended monochrome fallback
  // for exactly this range (full pictographic/color emoji are a separate,
  // much larger gap — see fontKeyForCodePoint's emoji comment).
  symbols: Object.freeze({
    url: `${NOTO}/NotoSansSymbols2/hinted/ttf/NotoSansSymbols2-Regular.ttf`,
    scripts: Object.freeze(["Zyyy", "Latn"]),
  }),
});

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
  // Geometric Shapes / Miscellaneous Symbols / Dingbats — not full emoji
  // (those need color-glyph rendering support the engine doesn't have yet).
  if (
    (cp >= 0x25a0 && cp <= 0x25ff) ||
    (cp >= 0x2600 && cp <= 0x26ff) ||
    (cp >= 0x2700 && cp <= 0x27bf)
  )
    return "symbols";
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

/** Fetches and memoizes one immutable font asset. */
export async function fetchFontBytes(url, cache, fetchImpl = fetch) {
  const cached = cache.get(url);
  if (cached) return cached;
  const response = await fetchImpl(url);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.length === 0) throw new Error("empty font response");
  cache.set(url, bytes);
  return bytes;
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
