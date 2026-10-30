#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
//
// Stages the deployment's OCR models (docs/182 §19-§20) into webapp/ocr/ for the
// Pages build, so scanned PDFs can be recognised on opendoc.casualoffice.org.
//
// The models are PaddlePaddle's PP-OCRv5 mobile graphs (Apache-2.0): detection and
// Latin recognition are committed in docs/assets/ocr (about 13 MB); a GitHub
// release is the fallback source. tools/ocr-models.json holds every SHA-256. Release downloads carry no CORS
// headers a browser can use, so they are fetched HERE, at build time, verified,
// and served same-origin. Only after every digest matches does this write
// webapp/ocr/manifest.json and the <meta name="opendoc-ocr-manifest"> tag the
// editor reads (src/ocr_models.mjs). A missing release or a digest mismatch
// leaves the site without models, and says so, rather than shipping bad bytes.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const webapp = join(here, "..");
const spec = JSON.parse(readFileSync(join(webapp, "..", "tools", "ocr-models.json"), "utf8"));
const out = join(webapp, "ocr");
const base = `https://github.com/${spec.repository}/releases/download/${spec.release}/`;

// The models committed in the repository (docs/assets/ocr) are the first source:
// no release, no network. The release is the fallback for a checkout without them.
const committed = join(webapp, "..", "docs", "assets", "ocr");

async function fetchVerified(entry) {
  let bytes;
  if (existsSync(join(committed, entry.file))) {
    bytes = new Uint8Array(readFileSync(join(committed, entry.file)));
  } else {
    const response = await fetch(base + entry.file);
    if (!response.ok) throw new Error(`${entry.file}: HTTP ${response.status}`);
    bytes = new Uint8Array(await response.arrayBuffer());
  }
  const sha = createHash("sha256").update(bytes).digest("hex");
  if (sha !== entry.sha256 || bytes.length !== entry.bytes) {
    throw new Error(`${entry.file}: digest or size mismatch (${sha}, ${bytes.length} B)`);
  }
  writeFileSync(join(out, entry.file), bytes);
  return { url: entry.file, sha256: entry.sha256, bytes: entry.bytes };
}

try {
  mkdirSync(out, { recursive: true });
  const detection = await fetchVerified(spec.detection);
  const packs = [];
  for (const pack of spec.packs) {
    packs.push({ name: pack.name, model: await fetchVerified(pack.model), dictionary: await fetchVerified(pack.dictionary) });
  }
  writeFileSync(join(out, "manifest.json"), `${JSON.stringify({ name: spec.name, detection, packs }, null, 2)}\n`);
  const editor = join(webapp, "editor.html");
  const html = readFileSync(editor, "utf8");
  const tag = '<meta name="opendoc-ocr-manifest" content="./ocr/manifest.json" />';
  if (!html.includes('name="opendoc-ocr-manifest"')) {
    writeFileSync(editor, html.replace("</head>", `    ${tag}\n  </head>`));
  }
  console.log(`stage-ocr-models: ${spec.release} verified and staged in webapp/ocr/`);
} catch (error) {
  // Not fatal: the editor without the tag behaves exactly as before (no models).
  console.warn(`stage-ocr-models: skipped, scanned-PDF recognition stays unavailable: ${error.message}`);
}
