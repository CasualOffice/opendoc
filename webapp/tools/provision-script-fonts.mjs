#!/usr/bin/env node
// Populates `webapp/assets/fonts/script/` with the faces the manifest marks
// `local: false`, so a deployment serves every document font from its own
// origin and never reaches a third party at runtime.
//
// Why this is a tool and not part of `build.sh`: the mirrored faces are ~59 MB
// (three ~16.4 MB CJK regions, the 4.99 MB colour-emoji face, and 9.28 MB of
// named Latin variable faces). Committing them would put that on every clone
// for ever, and downloading them on every CI build would put it on every build.
// So the repository carries the manifest, the hashes and the 2.39 MB of small
// script faces, and a deployment that wants to be fully local-first runs this
// once:
//
//     node webapp/tools/provision-script-fonts.mjs
//
// Every byte is verified against the SHA-256 in `src/web_fonts.mjs` before it
// is written, so this cannot quietly install a different face than the one the
// manifest and the tests describe. A file that is already present and already
// matches its hash is left alone, which makes the tool idempotent and cheap to
// re-run.
//
// `--check` verifies what is already on disk and writes nothing — the mode a
// deployment pipeline uses to assert its own origin is complete.
//
// Licensing: every face is SIL OFL 1.1 (Noto Sans CJK, Noto Color Emoji, Noto
// Sans/Serif) or Apache-2.0 (Roboto), both compatible with redistribution
// alongside this Apache-2.0 project. The OFL text for the Noto script faces is
// committed at `assets/fonts/script/OFL-1.1-Noto.txt`; a deployment that
// provisions the CJK and emoji faces is redistributing them and should ship
// that text with them.

import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { argv, exit } from "node:process";
import { fileURLToPath } from "node:url";

import { allManifestFaces } from "../src/web_fonts.mjs";

const DEST = new URL("../assets/fonts/script/", import.meta.url);

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

/** The on-disk state of one face: `"ok"`, `"missing"`, or a mismatch reason. */
async function inspect(face) {
  let bytes;
  try {
    bytes = await readFile(new URL(face.file, DEST));
  } catch {
    return { state: "missing" };
  }
  if (bytes.length !== face.bytes) {
    return {
      state: "wrong-size",
      detail: `${bytes.length} bytes on disk, manifest says ${face.bytes}`,
    };
  }
  const digest = sha256(bytes);
  if (digest !== face.sha256) {
    return { state: "wrong-hash", detail: `${digest} != ${face.sha256}` };
  }
  return { state: "ok" };
}

async function download(face) {
  const response = await fetch(face.mirror);
  if (!response.ok) throw new Error(`HTTP ${response.status} ${face.mirror}`);
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.length !== face.bytes) {
    throw new Error(
      `${face.file}: upstream returned ${bytes.length} bytes, manifest says ${face.bytes}`,
    );
  }
  const digest = sha256(bytes);
  if (digest !== face.sha256) {
    throw new Error(
      `${face.file}: upstream SHA-256 ${digest} does not match the manifest's ${face.sha256}. ` +
        `Nothing was written. Either the pinned revision moved (it should not — it is a commit ` +
        `sha) or the bytes were tampered with in transit.`,
    );
  }
  await writeFile(new URL(face.file, DEST), bytes);
  return bytes.length;
}

async function main() {
  const checkOnly = argv.includes("--check");
  await mkdir(DEST, { recursive: true });

  const faces = allManifestFaces();
  let fetched = 0;
  let bytes = 0;
  const problems = [];

  for (const face of faces) {
    const { state, detail } = await inspect(face);
    if (state === "ok") {
      console.log(`ok       ${face.file}`);
      continue;
    }
    if (face.local) {
      // A committed face is missing or corrupt: that is a repository problem,
      // not something to paper over by downloading it.
      problems.push(
        `${face.file} is committed to this repository but ${state}${detail ? ` (${detail})` : ""}`,
      );
      continue;
    }
    if (checkOnly) {
      problems.push(
        `${face.file} is not provisioned on this origin (${state}${detail ? `: ${detail}` : ""})`,
      );
      continue;
    }
    console.log(`fetch    ${face.file} (${face.bytes} bytes) from ${face.mirror}`);
    bytes += await download(face);
    fetched += 1;
  }

  if (problems.length > 0) {
    console.error(`\n${problems.length} problem(s):`);
    for (const problem of problems) console.error(`  - ${problem}`);
    if (checkOnly) {
      console.error(
        "\nRun `node webapp/tools/provision-script-fonts.mjs` to populate this origin.",
      );
    }
    exit(1);
  }

  console.log(
    `\n${faces.length} faces verified; ${fetched} newly fetched (${bytes} bytes).`,
  );
}

if (fileURLToPath(import.meta.url) === argv[1]) {
  await main();
}
