// Authorship and licensing, asserted rather than asserted-in-prose.
//
// The licence is this project's competitive position, not paperwork: the
// alternative it exists to be is AGPL-3.0-only with commercial tiers, so
// "Apache-2.0, developed by the CasualOffice Team, contributions welcome under
// the same terms" is a product claim. `docs/99` §9 rule 1 applies to it exactly
// as it applies to a fidelity number — a published claim is derived from a
// committed artifact or it is not published.
//
// So there are four things worth failing a build over, and this file is each of
// them:
//
//   1. The repository has a NOTICE, and it says what Apache-2.0 §4(d) notices
//      say.
//   2. Every third-party licence file committed here is ATTRIBUTED in it, and
//      every attribution names a file that exists. Both directions, because a
//      bundled font whose licence nobody mentioned and an attribution for a font
//      that was removed are the same defect seen from opposite sides — and the
//      first one is what happens the next time somebody adds a face.
//   3. The PUBLISHED package carries the licence and the notice. `"license":
//      "Apache-2.0"` in a manifest is an identifier; a host who installs the
//      package receives files, and until this landed they received neither.
//   4. A source file that is a licensing entry point declares SPDX, and the list
//      is ENUMERATED rather than written out here, so a new crate fails this
//      instead of quietly joining the ones without a header.
//
// WHAT THIS DELIBERATELY DOES NOT COVER, so the boundary is stated rather than
// implied: the sweep is crate roots, the workspace's three binaries, and the
// modules the npm package publishes. The rest of `webapp/src` has no header, and
// `webapp/src/main.js` least of all — it is under a line ratchet
// (`module_seams.test.mjs`) that a header line would spend for nothing. Widening
// the sweep is its own change; what this file guarantees is that the boundary
// cannot silently shrink.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const REPO = dirname(WEBAPP);
const PACKAGE = join(REPO, "packages", "opendoc-embed");

const notice = readFileSync(join(REPO, "NOTICE"), "utf8");
const manifest = JSON.parse(readFileSync(join(PACKAGE, "package.json"), "utf8"));

/** Directories that are not this repository's to account for. */
const SKIP = new Set([
  ".git",
  ".claude",
  "node_modules",
  "target",
  "pkg",
  "test-results",
  "playwright-report",
]);

/** A file whose name says it carries licence terms.
 *
 *  The extension matters as much as the stem. Without it this matched its own
 *  source file — `licence_metadata.test.mjs` begins with "licence" — which is a
 *  fair warning that "name starts with LICENSE" catches code as readily as
 *  terms. Licence texts here are extensionless (`LICENSE`), `.txt`
 *  (`LICENSE-Inter.txt`, `OFL-1.1-Carlito.txt`) or `.md`; nothing executable
 *  ever is. */
function isLicenceFile(name) {
  if (!/^(licen[sc]e|copying|ofl)\b/i.test(name.replace(/[-_.]/g, " "))) return false;
  const dot = name.lastIndexOf(".");
  const extension = dot === -1 ? "" : name.slice(dot).toLowerCase();
  return extension === "" || extension === ".txt" || extension === ".md";
}

function* filesUnder(dir) {
  for (const name of readdirSync(dir).sort()) {
    if (SKIP.has(name)) continue;
    const path = join(dir, name);
    let info;
    try {
      info = statSync(path);
    } catch {
      continue;
    }
    if (info.isDirectory()) yield* filesUnder(path);
    else yield path;
  }
}

/** Every committed licence file that belongs to somebody else.
 *
 *  The root LICENSE is OUR terms, and the package's copies of the root pair are
 *  generated from them and checked by their own test below, so neither is a
 *  third-party attribution and neither is expected in the notices. */
function thirdPartyLicenceFiles() {
  const ours = new Set([
    "LICENSE",
    "NOTICE",
    "packages/opendoc-embed/LICENSE",
    "packages/opendoc-embed/NOTICE",
  ]);
  const found = [];
  for (const path of filesUnder(REPO)) {
    const rel = relative(REPO, path).split("\\").join("/");
    if (ours.has(rel)) continue;
    if (isLicenceFile(rel.slice(rel.lastIndexOf("/") + 1))) found.push(rel);
  }
  return found;
}

test("the repository states its authorship and its licence in a NOTICE", () => {
  assert.match(notice, /Copyright 2026 CasualOffice/);
  assert.match(notice, /Developed by the CasualOffice Team/);
  assert.match(notice, /Apache License, Version 2\.0/);
  assert.match(notice, /Contributions are welcome under the same licence/);
  // The notice points at the terms rather than restating them.
  assert.match(notice, /http:\/\/www\.apache\.org\/licenses\/LICENSE-2\.0/);
});

test("every third-party licence file committed here is attributed in NOTICE", () => {
  // The guard that makes the next bundled font a build failure rather than a
  // discovery. A licence file lands in the tree with the asset it covers; this
  // fails until a human writes the attribution beside it, and the attribution is
  // then traceable to the file that forced it.
  const files = thirdPartyLicenceFiles();
  assert.ok(
    files.length >= 9,
    `only ${files.length} third-party licence file(s) found — the walk is broken`,
  );
  const unattributed = files.filter((rel) => !notice.includes(rel));
  assert.deepEqual(
    unattributed,
    [],
    "these licence files are committed but named nowhere in NOTICE. Add the attribution — " +
      "copyright line, licence, and this path — derived from the file itself.",
  );
});

test("every licence file NOTICE cites is really there", () => {
  // The other direction. An attribution for a font that has since been removed
  // is a notice that misdescribes what ships, which is the failure mode
  // `docs/99` §9.3 calls an overstatement.
  const cited = [...notice.matchAll(/[\w./-]*LICEN[SC]ES?[\w.-]*\.txt|[\w./-]*OFL-[\w.-]*\.txt/g)].map(
    (match) => match[0],
  );
  assert.ok(cited.length >= 9, `NOTICE cites only ${cited.length} licence file(s)`);
  const missing = cited.filter((rel) => {
    try {
      return !statSync(join(REPO, rel)).isFile();
    } catch {
      return true;
    }
  });
  assert.deepEqual(missing, [], "NOTICE cites licence files that do not exist");
});

test("the published package ships the licence and the notice, not just the identifier", () => {
  // npm cannot publish a file from outside the package directory, so these are
  // generated copies — and they are compared BYTE FOR BYTE with the root here,
  // because a stale copy is a notice that describes a different build.
  for (const name of ["LICENSE", "NOTICE"]) {
    assert.equal(
      readFileSync(join(PACKAGE, name), "utf8"),
      readFileSync(join(REPO, name), "utf8"),
      `packages/opendoc-embed/${name} is not the repository's ${name}`,
    );
    assert.ok(
      manifest.files.includes(name),
      `${name} is in the package directory but "files" does not publish it`,
    );
  }
  assert.equal(manifest.license, "Apache-2.0");
  assert.match(manifest.author, /CasualOffice/);
});

test("the webapp manifest declares the licence a consumer would look for", () => {
  const webapp = JSON.parse(readFileSync(join(WEBAPP, "package.json"), "utf8"));
  assert.equal(webapp.license, "Apache-2.0");
  assert.match(webapp.author, /CasualOffice/);
});

/** The files that must declare SPDX, ENUMERATED rather than listed.
 *
 *  Every crate's root, every workspace binary's root, the modules `webapp/src`
 *  is the authority for in the published package, and everything the package
 *  actually publishes. A crate added tomorrow is in this list the moment its
 *  `src/lib.rs` exists, which is the whole point: a list written out by hand is
 *  a list the next crate is missing from. */
function licensingEntryPoints() {
  const paths = [];
  // Directories only. A macOS checkout leaves `.DS_Store` beside the crates, and
  // reading `crates/.DS_Store/src/lib.rs` throws ENOTDIR — the guard then fails
  // for everyone on a Mac while passing in CI, which is the least useful way for
  // a guard to fail. `withFileTypes` asks the directory rather than guessing
  // from the name, so an editor's stray file or a symlinked crate behaves.
  const directories = (where) =>
    readdirSync(join(REPO, where), { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
      .sort();
  for (const crate of directories("crates")) {
    paths.push(join(REPO, "crates", crate, "src", "lib.rs"));
  }
  for (const tool of directories("tools")) {
    paths.push(join(REPO, "tools", tool, "src", "main.rs"));
  }
  for (const name of [
    "capabilities.mjs",
    "embed_element.mjs",
    "embed_define.js",
    "host_contract.mjs",
    "host_client.mjs",
  ]) {
    paths.push(join(WEBAPP, "src", name));
  }
  for (const name of readdirSync(join(PACKAGE, "src")).sort()) {
    if (/\.(mjs|js)$/.test(name)) paths.push(join(PACKAGE, "src", name));
  }
  paths.push(join(PACKAGE, "types", "index.d.ts"));
  return paths;
}

test("every licensing entry point declares SPDX-License-Identifier: Apache-2.0", () => {
  // Within the first twelve lines rather than on line one: a generated module in
  // the package opens with the banner saying where it came from, and
  // `embed_package.test.mjs` requires that banner to be first. What matters is
  // that the file says which licence it is under, not which line says it.
  const bare = [];
  for (const path of licensingEntryPoints()) {
    const head = readFileSync(path, "utf8").split("\n").slice(0, 12).join("\n");
    if (!head.includes("SPDX-License-Identifier: Apache-2.0")) bare.push(relative(REPO, path));
  }
  assert.deepEqual(
    bare,
    [],
    "these files are licensing entry points with no SPDX header. Add " +
      "`// SPDX-License-Identifier: Apache-2.0` as the first line.",
  );
});

test("the SPDX sweep covers every crate in the workspace, not a subset", () => {
  // The sweep is only worth anything if it is complete for the thing it claims
  // to cover. This reads the workspace members out of `Cargo.toml` and checks
  // the enumeration above reached all of them, so a crate that lives outside
  // `crates/` or `tools/` cannot slip past the glob.
  const cargo = readFileSync(join(REPO, "Cargo.toml"), "utf8");
  const members = [
    ...(cargo.match(/^members = \[([\s\S]*?)^\]/m)?.[1] ?? "").matchAll(/"([^"]+)"/g),
  ].map((match) => match[1]);
  assert.ok(members.length >= 20, `only ${members.length} workspace member(s) parsed`);
  const covered = new Set(licensingEntryPoints().map((path) => relative(REPO, path)));
  const uncovered = members.filter(
    (member) => !covered.has(`${member}/src/lib.rs`) && !covered.has(`${member}/src/main.rs`),
  );
  assert.deepEqual(uncovered, [], "these workspace members have no root in the SPDX sweep");
});
