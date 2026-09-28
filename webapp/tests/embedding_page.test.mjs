// `webapp/embedding.page.html` — the guard that makes the embedding page honest.
//
// The public pages of this repository have carried fabricated claims twice, in
// both directions (`docs/99` §9, `docs/105` EV-007): a parity figure that
// contradicted the fidelity page, a corpus file that did not exist, a "sub-10 ms"
// repaint with no benchmark, and three shipped features listed as "Not yet".
// `docs/126` therefore binds every SDK phase page to the same rule — examples
// extracted from code that runs in CI, numbers generated from a committed
// artifact, and an honest "what this does not do yet".
//
// So this file checks the four ways the page could be false:
//
//   1. It has drifted from the code it documents. `build-embed-docs.mjs --check`
//      answers that, and this runs it.
//   2. Its numbers are wrong. They are RE-DERIVED here from the authority and
//      from the package's own bytes — deliberately NOT by calling the generator,
//      because a guard that re-runs the code under test agrees with its bugs.
//   3. Its examples were typed rather than extracted. Every code panel names its
//      source file, and every panel's body is checked to appear VERBATIM in that
//      file, with the chain from that file to a test that runs it.
//   4. Its "does not do yet" list has gone stale. Understating is also false —
//      `docs/99` §9.6 records the landing page listing three shipped features as
//      "Not yet" — so every gap the page publishes is asserted to still be a gap.
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  CAPABILITIES,
  LEGACY_PRESETS,
  PRESET_NAMES,
  ROLES,
  editingModeFor,
  resolveCapabilities,
  sandboxTokensFor,
} from "../src/capabilities.mjs";
import { OPENDOC_EDITOR_TAG, OpenDocEditorElement } from "../src/embed_element.mjs";
import { REGIONS, resolveRegions } from "../src/capabilities.mjs";
import { auditBrand, normalize as normalizeBrand } from "../tools/build-brand.mjs";
import { readPalettes } from "../tools/palette_source.mjs";
import {
  COMMAND_CONTRACT,
  HOST_EVENTS,
  PROTOCOL,
  REFUSAL_CODES,
  commandContract,
} from "../src/host_contract.mjs";

const WEBAPP = dirname(dirname(fileURLToPath(import.meta.url)));
const REPO = dirname(WEBAPP);
const PACKAGE = join(REPO, "packages", "opendoc-embed");

const read = (path) => readFileSync(path, "utf8");
const page = read(join(WEBAPP, "embedding.page.html"));
const built = read(join(WEBAPP, "embedding.html"));
const manifest = JSON.parse(read(join(PACKAGE, "package.json")));

/** Undoes the escaping the generator applies, so a panel's text can be compared
 *  with the file it was taken from. */
const unescape = (html) =>
  html.replaceAll("&lt;", "<").replaceAll("&gt;", ">").replaceAll("&amp;", "&");

/** The single value of `data-claim="name"`, the way `site_claims.test.mjs` reads
 *  the landing page's. */
function claim(name) {
  const values = [...page.matchAll(new RegExp(`data-claim="${name}"[^>]*>([^<]*)<`, "g"))].map((m) =>
    m[1].trim(),
  );
  assert.ok(values.length > 0, `the page must carry data-claim="${name}"`);
  assert.equal(new Set(values).size, 1, `data-claim="${name}" appears with different values`);
  return values[0];
}

/** One generated region of the BUILT page, so an assertion about the role table
 *  cannot accidentally match the capability table — both list capability names,
 *  and the first draft of this file read `edit` out of the wrong one. */
function region(name) {
  const open = `<!-- @generated ${name} -->`;
  const close = `<!-- @end ${name} -->`;
  const from = built.indexOf(open);
  const to = built.indexOf(close);
  assert.ok(from >= 0 && to > from, `the built page must carry the "${name}" region`);
  return built.slice(from + open.length, to);
}

/** Every generated code panel: its caption and its code, unescaped. */
function panels() {
  const found = [
    ...built.matchAll(
      /<div class="code-panel-head"><span>([^<]*)<\/span><\/div>\s*<pre><code>([\s\S]*?)<\/code><\/pre>/g,
    ),
  ];
  assert.ok(found.length >= 5, "the page must carry the extracted code panels");
  return found.map(([, caption, code]) => ({ caption, code: unescape(code) }));
}

/** The file a panel's caption names, as a repo-relative path. */
function captionedFile(caption) {
  const path = caption.split(" — ")[0].trim();
  assert.match(path, /^[\w./-]+$/, `a panel caption must start with its source path: "${caption}"`);
  return path;
}

test("the page is what a fresh generation produces", () => {
  // The same shape `build_site.test.mjs` and `embed_package.test.mjs` use: run
  // the generator's `--check` and let a non-zero exit fail the test. NOT piped
  // into anything — a gate read through a pipe reports the pipe's status, which
  // is how a formatting failure once reached CI looking green (#552).
  execFileSync("node", [join(WEBAPP, "tools", "build-embed-docs.mjs"), "--check"], {
    cwd: WEBAPP,
    stdio: "pipe",
  });
});

test("every number on the page is re-derived, not taken from the generator", () => {
  assert.equal(Number(claim("capability-count")), CAPABILITIES.length);
  assert.equal(Number(claim("role-count")), ROLES.length);
  assert.equal(Number(claim("preset-count")), PRESET_NAMES.length);
  assert.equal(claim("package-name"), manifest.name);
  assert.equal(claim("package-version"), manifest.version);
  assert.equal(Number(claim("package-dependencies")), Object.keys(manifest.dependencies ?? {}).length);
  assert.equal(Number(claim("entry-point-count")), Object.keys(manifest.exports).length);
  assert.equal(claim("unconditional-sandbox"), sandboxTokensFor(new Set()).join(" "));

  // The package's size and file count, measured off the committed bytes rather
  // than read back from `npm pack`: two ways of counting the same thing, so a
  // mistake in either is visible. `files` in the manifest is what ships, plus
  // the two npm always includes.
  const shipped = [];
  const walk = (entry) => {
    const absolute = join(PACKAGE, entry);
    if (statSync(absolute).isDirectory()) {
      for (const child of readdirSync(absolute)) walk(join(entry, child));
    } else {
      shipped.push(statSync(absolute).size);
    }
  };
  for (const entry of [...manifest.files, "package.json"]) walk(entry);
  assert.equal(Number(claim("package-file-count")), shipped.length, "files in the tarball");
  const kb = Math.round(shipped.reduce((sum, size) => sum + size, 0) / 1024);
  assert.equal(claim("package-unpacked-kb"), `${kb} kB`, "unpacked size");
});

test("the capability and role tables state what the authority resolves", () => {
  // Read out of the built page as a reader sees it, and compared with the
  // authority — so a table that is internally consistent but wrong still fails.
  const roles = region("role-table");
  for (const role of ROLES) {
    const row = roles.match(
      new RegExp(`<div class="disp-name"><code>${role}</code></div>\\s*([\\s\\S]*?)</div>\\s*</div>`),
    );
    assert.ok(row, `the role table must carry a row for ${role}`);
    const capabilities = resolveCapabilities({ mode: role, framed: true });
    const granted = [...row[1].matchAll(/<code>([\w-]+)<\/code>/g)].map((m) => m[1]);
    assert.equal(
      granted[0],
      editingModeFor(capabilities),
      `${role}: the review mode on the page is not the one the authority resolves`,
    );
    for (const capability of capabilities) {
      assert.ok(granted.includes(capability), `${role}: the page omits its "${capability}" grant`);
    }
    for (const capability of CAPABILITIES) {
      if (capabilities.has(capability)) continue;
      assert.ok(
        !granted.includes(capability),
        `${role}: the page shows "${capability}" as granted, and it is not`,
      );
    }
    for (const token of sandboxTokensFor(capabilities)) {
      if (sandboxTokensFor(new Set()).includes(token)) continue;
      assert.ok(granted.includes(token), `${role}: the page omits its "${token}" sandbox token`);
    }
  }
  // Every capability there is has a row: absence from a support matrix is an
  // overstatement by omission (`docs/99` §9.3).
  const capabilityTable = region("capability-table");
  for (const capability of CAPABILITIES) {
    assert.match(
      capabilityTable,
      new RegExp(`<div class="disp-name"><code>${capability}</code></div>`),
      `the capability table omits "${capability}"`,
    );
  }
  const legacyTable = region("legacy-table");
  for (const [legacy, role] of Object.entries(LEGACY_PRESETS)) {
    assert.match(legacyTable, new RegExp(`<code>${legacy}</code>`), `the page omits the "${legacy}" preset`);
    if (!role) continue;
    const same =
      [...resolveCapabilities({ mode: legacy })].sort().join(" ") ===
      [...resolveCapabilities({ mode: role })].sort().join(" ");
    assert.ok(same, `the page calls ${legacy} an alias of ${role}, and the sets differ`);
  }
});

test("every code panel was extracted from the file its caption names", () => {
  // The rule `docs/126` states for every phase page: an example nobody executes
  // is the next false claim. So no panel may contain a line that is not in the
  // file it says it came from.
  for (const { caption, code } of panels()) {
    const path = captionedFile(caption);
    if (path.endsWith(".test.mjs")) continue; // the shell panel, checked below
    // The refusal panel is OUTPUT, not source: it is what `build-brand.mjs` prints
    // when a host's palette fails AA, produced by running the validator at generate
    // time. Exempted the same way the install panel is, and checked the other way
    // round in its own test below — every line must be a refusal the validator
    // really emits, re-derived there without going through the generator.
    if (path.endsWith("build-brand.mjs") && /failing palette/.test(caption)) continue;
    const source = read(join(REPO, path));
    const [first] = code.split("\n");
    assert.ok(
      source.includes(first.trim()),
      `"${caption}" shows a line that is not in ${path}: ${first.trim()}`,
    );
    for (const line of code.split("\n")) {
      if (!line.trim()) continue;
      assert.ok(
        source.includes(line.trim()),
        `"${caption}" shows a line that is not in ${path}: ${line.trim()}`,
      );
    }
  }
});

test("the commands the install panel shows are the ones the packaging guard runs", () => {
  // The one panel that is composed rather than quoted, because a test's
  // `execFileSync` arguments are not a shell transcript. So the claim is checked
  // the other way round: every command shown must be one the guard really runs.
  const panel = panels().find(({ caption }) => captionedFile(caption).endsWith(".test.mjs"));
  assert.ok(panel, "the install panel must name the packaging guard");
  const guard = read(join(WEBAPP, "tests", "embed_package.test.mjs"));
  assert.match(guard, /"npm", \["pack"/, "the guard no longer packs the package");
  assert.match(guard, /"npm", \["install"/, "the guard no longer installs the tarball");
  assert.match(panel.code, /npm pack/);
  assert.match(panel.code, /npm install/);
  // And the tarball name is npm's, not a guess.
  const packed = JSON.parse(
    execFileSync("npm", ["pack", "--dry-run", "--json"], {
      cwd: PACKAGE,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }),
  )[0];
  assert.ok(
    panel.code.includes(packed.filename),
    `the install panel must name the tarball npm produces (${packed.filename})`,
  );
  // The page must not show a registry install, because nothing publishes it.
  assert.ok(
    !new RegExp(`npm i(nstall)? ${manifest.name.replace("/", "\\/")}\\s*$`, "m").test(panel.code),
    "the page must not show a registry install while nothing publishes the package",
  );
});

test("the declarative example only uses attributes and modes the element accepts", () => {
  // The README's own snippet is published, so it is checked against the element
  // that reads it. `docs/99` §9.7: the design prototype published
  // `doc.transaction()` and `doc.writeDocx()`, neither of which existed.
  const element = read(join(WEBAPP, "src", "embed_element.mjs"));
  const attributes = [
    ...element.match(/const MOUNT_ATTRIBUTES = Object\.freeze\(\[([^\]]*)\]\)/)[1].matchAll(
      /"([\w-]+)"/g,
    ),
  ].map((m) => m[1]);
  assert.equal(Number(claim("attribute-count")), attributes.length);

  const panel = panels().find(({ caption }) => captionedFile(caption).endsWith("README.md"));
  assert.ok(panel, "the page must carry the package's own declarative usage");
  const tag = panel.code.match(new RegExp(`<(${OPENDOC_EDITOR_TAG})\\b([\\s\\S]*?)>`));
  assert.ok(tag, `the snippet must mount <${OPENDOC_EDITOR_TAG}>`);
  for (const [, name, value] of tag[2].matchAll(/([\w-]+)="([^"]*)"/g)) {
    assert.ok(attributes.includes(name), `the snippet sets "${name}", which the element never reads`);
    if (name === "mode") {
      assert.ok(PRESET_NAMES.includes(value), `the snippet asks for mode="${value}", which is not a preset`);
    }
  }
  // Import specifiers must be ones the manifest actually exports.
  for (const [, specifier] of panel.code.matchAll(/from "([^"]+)"|import "([^"]+)"/g)) {
    if (!specifier?.startsWith(manifest.name)) continue;
    const subpath = specifier.slice(manifest.name.length) || ".";
    assert.ok(
      manifest.exports[subpath === "." ? "." : `.${subpath}`],
      `the snippet imports "${specifier}", which the package does not export`,
    );
  }
});

test("the attack the page describes is the one the spec performs", () => {
  const spec = read(join(WEBAPP, "tests", "e2e", "embedded-capability-gate.spec.mjs"));
  const controls = [...spec.match(/const controls = \[([\s\S]*?)\];/)[1].matchAll(/"(\w+)"/g)];
  assert.equal(Number(claim("attack-command-count")), controls.length);
  // And it is still an attack: the chrome's refusals are stripped first, and the
  // verdict is read from the engine rather than from the DOM.
  assert.match(spec, /removeAttribute\("disabled"\)/, "the spec no longer defeats the chrome");
  assert.match(spec, /documentStats\(\)|#statWords/, "the spec no longer reads the engine's counts");
});

test("the chrome table names only capabilities the editor page really consults", () => {
  const main = read(join(WEBAPP, "src", "main.js"));
  const consulted = new Set(
    [...main.matchAll(/(?:HOST_CAPS|hostCapabilities\(\))\.has\("([a-z]+)"\)/g)].map((m) => m[1]),
  );
  if (/editingModeFor\(HOST_CAPS\)/.test(main)) {
    consulted.add("edit");
    consulted.add("comment");
  }
  assert.equal(Number(claim("chrome-consulted-count")), consulted.size);
  assert.equal(
    Number(claim("chrome-unconsulted-count")),
    CAPABILITIES.length - consulted.size,
    "the count of capabilities the chrome does not consult",
  );
  // Command ids the page prints must be commands that exist and be gated there.
  for (const [, id] of region("chrome-table").matchAll(/<div class="disp-cell"><code>([a-z]+\.[a-z]+)<\/code><\/div>/g)) {
    assert.match(
      main,
      // Either spelling of the ONE authority: `HOST_CAPS` is `hostCapabilities()`
      // hoisted once at startup, so accepting only the call form made this fail on
      // a command gated through the hoisted constant — a guard pinned to a spelling
      // rather than to the guarantee.
      new RegExp(`id: "${id}"[^\\n]*(?:HOST_CAPS|hostCapabilities\\(\\))\\.has`),
      `the page says ${id} is capability-gated, and main.js does not gate it`,
    );
  }
});

test("everything under 'does not do yet' is still not done", () => {
  // Understating is also false. Each assertion here fails when the gap closes,
  // which is the point: the page must not outlive it.
  const packageSources = ["capabilities.mjs", "embed_element.mjs", "embed_define.js"]
    .map((name) => read(join(PACKAGE, "src", name)))
    .join("\n");

  // The ELEMENT has no command methods: the contract is reached through
  // `window.opendoc` or through `createHostClient`, and a host holds the contract
  // rather than the element. Asserted against the class, not against its source,
  // so adding a method is caught however it is spelled.
  for (const method of ["execute", "query", "describe", "ping", "on", "off"]) {
    assert.equal(
      typeof OpenDocEditorElement.prototype[method],
      "undefined",
      `the element now has a ${method}() method, so the page must stop saying it does not`,
    );
  }
  const events = [...packageSources.matchAll(/new CustomEvent\(\s*([A-Z_]+|"[^"]+")/g)].map(
    (m) => m[1],
  );
  assert.deepEqual(events, ["CAPABILITIES_EVENT"], "the element now fires more than one event");

  // No arguments for most commands: the schema declares an argument list and
  // exactly one row uses it.
  const withArgs = COMMAND_CONTRACT.filter((row) => row.args.length > 0).map((row) => row.id);
  assert.deepEqual(withArgs, ["view.zoom"], "more commands take arguments now");

  // No document I/O through the contract: none of the four verbs moves bytes.
  assert.deepEqual([...PROTOCOL.requests], ["describe", "execute", "query", "ping"]);
  const write = HOST_EVENTS.find((event) => event.name === "export");
  assert.deepEqual(
    [...write.detail],
    ["format", "name", "bytes"],
    "the export event's payload changed — if it now carries the document, the page " +
      "must stop saying a host cannot have the bytes",
  );

  // The context menu's word-level commands are outside the contract. Both halves
  // are asserted: that the ids are NOT in the contract, and that they really exist
  // — a gap stated about a command that does not exist is not a gap.
  const wordLevel = ["spell.ignoreOnce", "spell.addToDictionary", "grammar.explain"];
  const spellSource = read(join(WEBAPP, "src", "spell_check.mjs"));
  for (const id of wordLevel) {
    assert.equal(commandContract(id), null, `${id} is in the contract now`);
    assert.ok(spellSource.includes(`id: "${id}"`), `${id} no longer exists, so the gap is stale`);
  }
  assert.ok(REFUSAL_CODES.length > 0);

  // No host-supplied document: three mount attributes, and none of them is one.
  const attributes = [
    ...read(join(WEBAPP, "src", "embed_element.mjs"))
      .match(/const MOUNT_ATTRIBUTES = Object\.freeze\(\[([^\]]*)\]\)/)[1]
      .matchAll(/"([\w-]+)"/g),
  ].map((m) => m[1]);
  assert.deepEqual(attributes, ["mode", "editor-src", "frame-title"]);

  // No host capability list: the only override that moves anything is autosave.
  const commentor = resolveCapabilities({ mode: "commentor", framed: true });
  const withPrintWithheld = resolveCapabilities({ mode: "commentor", framed: true, print: false });
  assert.deepEqual(
    [...withPrintWithheld].sort(),
    [...commentor].sort(),
    "a per-capability override now works, so the page must stop saying it does not",
  );
  assert.ok(resolveCapabilities({ mode: "readonly", autosave: true }).has("autosave"));

  // No reading or preview chrome: both roles still resolve to the same mode, and
  // nothing in the editor page consults a capability in order to drop a surface.
  assert.equal(editingModeFor(resolveCapabilities({ mode: "preview" })), "viewing");
  assert.equal(editingModeFor(resolveCapabilities({ mode: "readonly" })), "viewing");
  const main = read(join(WEBAPP, "src", "main.js"));
  const consulted = new Set(
    [...main.matchAll(/(?:HOST_CAPS|hostCapabilities\(\))\.has\("([a-z]+)"\)/g)].map((m) => m[1]),
  );
  // The claim is "no reading or preview chrome": no SURFACE is composed out for a
  // role. So the thing to assert is not WHICH capabilities are consulted — that
  // list grows every time another command is gated, and an allowlist of today's
  // three made this guard fail on a change that only disabled more controls,
  // which is the "pinned to a circumstance" shape that reddened `main` twice on
  // 2026-09-26. What must hold is HOW they are consulted: a capability may
  // disable a control, and may not hide or remove one.
  // The hiding mechanisms this codebase actually uses. A bare `.remove()` is
  // deliberately NOT here: `reviewCommentActions.remove()` removes a COMMENT, and
  // matching it made this guard fail on a line that hides nothing. The narrower
  // pattern can be evaded by a DOM `element.remove()`, so this is a tripwire for
  // the ordinary case rather than a proof — which is worth saying out loud instead
  // of implying a completeness it does not have.
  const COMPOSES_OUT = /\.hidden\s*=|style\.display|removeChild|replaceChildren\(\s*\)/;
  for (const line of main.split("\n")) {
    if (!/(?:HOST_CAPS|hostCapabilities\(\))\.has\("/.test(line)) continue;
    assert.ok(
      !COMPOSES_OUT.test(line),
      "a capability now HIDES a surface rather than disabling it: " +
        `${line.trim().slice(0, 140)} — surface composition is real and welcome, but the ` +
        "page's 'no reading or preview chrome' claim and its chrome table must change with it",
    );
  }

  // Withholding `branding` still does nothing.
  assert.ok(!consulted.has("branding"), "branding is now consulted; the page says it is not");
  assert.equal(
    sandboxTokensFor(new Set(["branding"])).join(" "),
    sandboxTokensFor(new Set()).join(" "),
    "branding now changes the sandbox",
  );

  // The RUST facade declares the contract and does not run the editor. Both
  // halves are asserted, because either one alone is a different claim: that the
  // crate really does declare the vocabulary (its parity test exists and names
  // this file), and that it really is not the editor's runtime — the engine the
  // browser loads references it zero times, and the only workspace member that
  // depends on it is the benchmark tool. The day convergence lands, this fails and
  // the page has to stop saying it.
  const parity = read(join(REPO, "crates", "casual-doc-sdk", "src", "host_parity.rs"));
  assert.ok(
    parity.includes("host_contract.mjs"),
    "the Rust parity test no longer reads the editor's schema, so the page's claim that one " +
      "vocabulary is shared is unbacked",
  );
  const wasmCrate = read(join(REPO, "crates", "casual-doc-wasm", "src", "lib.rs"));
  assert.equal(
    wasmCrate.includes("casual_doc_sdk"),
    false,
    "casual-doc-wasm now uses casual-doc-sdk: the facade IS becoming the runtime, so the page " +
      "must stop saying it is not",
  );
  // Directories only, and only ones that really are crates. Reading
  // `<entry>/Cargo.toml` for every DIRECTORY ENTRY threw `ENOTDIR` on a stray
  // file — macOS drops `.DS_Store` into any folder someone opens in Finder — so
  // the guard crashed instead of asserting. A guard that throws on the
  // developer's filesystem and passes in CI is a guard people learn to ignore.
  const dependants = readdirSync(join(REPO, "crates"), { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && entry.name !== "casual-doc-sdk")
    .map((entry) => entry.name)
    .filter((name) => existsSync(join(REPO, "crates", name, "Cargo.toml")))
    .filter((name) => read(join(REPO, "crates", name, "Cargo.toml")).includes("casual-doc-sdk"));
  assert.deepEqual(
    dependants,
    [],
    "a crate now depends on casual-doc-sdk; the page's claim about its reach is stale",
  );

  // No framework bindings, and nothing published.
  for (const subpath of Object.keys(manifest.exports)) {
    assert.ok(
      !/react|vue|svelte|angular/i.test(subpath),
      `the package now ships a framework entry point (${subpath})`,
    );
  }
  const workflows = join(REPO, ".github", "workflows");
  for (const file of readdirSync(workflows)) {
    assert.ok(
      !/npm publish|npm_publish|NODE_AUTH_TOKEN/.test(read(join(workflows, file))),
      `${file} now publishes something; the page says nothing is published`,
    );
  }
});

test("nothing in the package consults a licence", () => {
  // The page's competitive claim, asserted rather than asserted-at: ONLYOFFICE's
  // host-customization path early-returns when unlicensed
  // (`reference/web-apps/apps/common/main/lib/controller/LayoutManager.js:68`).
  // Ours must stay ungated, because the permissive licence is the wedge.
  for (const name of ["capabilities.mjs", "embed_element.mjs", "embed_define.js"]) {
    const source = read(join(PACKAGE, "src", name));
    assert.ok(
      !/licen[cs]e/i.test(source.replace(/^\s*(\/\/|\*|\/\*).*$/gm, "")),
      `${name} now mentions a licence outside its comments`,
    );
  }
});

test("the page adds no styling of its own", () => {
  // `docs/63`: the design tokens are deliberate — 140 tokens, one raw hex in
  // 6,481 CSS lines, and an AA contrast sweep. A documentation page that brings
  // its own colours is how that becomes 141 tokens and two sweeps, so this page
  // uses the docs shell's existing components and nothing else. A visual change
  // is a proposal to the owner, not an inline style.
  assert.ok(!/<style[\s>]/.test(page), "the page must not carry its own stylesheet");
  assert.ok(!/\sstyle="/.test(page), "the page must not carry inline styles");
  assert.match(page, /marketing\.css/, "the page must use the shared stylesheet");
});

test("the page is navigable, and the chrome stays DRY", () => {
  // Authored from the shared partials like every other page, and marked with its
  // OWN nav key. It used to say `active=docs`: the guide lit up somebody else's
  // tab and was reachable only from a rail on a page a reader had to find first.
  // That was the reasoning this comment used to carry — "reached from the docs
  // rail rather than by widening the header" — and it is why embedding, and then
  // the playground, shipped unfindable. Widening the header was the right move;
  // `site_nav_reach.test.mjs` now fails the build for a page without its own entry.
  assert.match(page, /<!-- @include site-header active=embedding -->/);
  assert.match(page, /<!-- @include site-footer -->/);
  assert.match(built, /class="site-header"/, "the built page must carry the inlined header");
  assert.match(built, /data-nav="embedding" aria-current="page"/);
  assert.ok(
    !/data-nav="docs" aria-current="page"/.test(built),
    "the embedding guide must not mark the Docs tab active — it has a tab of its own",
  );
  // Reachable: the docs page's rail and this page's own rail both link it.
  for (const template of ["docs.page.html", "embedding.page.html"]) {
    assert.match(
      read(join(WEBAPP, template)),
      /href="\.\/embedding\.html"/,
      `${template} must link the embedding guide`,
    );
  }
  assert.match(read(join(WEBAPP, "sitemap.xml")), /embedding\.html/, "the sitemap must list the page");
});

test("the gates the page names are armed", () => {
  // `docs/99` §9.2: prose describing a CI gate must name the workflow, and a
  // test must assert the gate is armed — a gate claimed as "CI-enforced" here had
  // never executed.
  const ci = read(join(REPO, ".github", "workflows", "ci.yml"));
  const job = ci.slice(ci.indexOf("browser-smoke:"), ci.indexOf("platform:"));
  assert.match(job, /\.\/webapp\/build\.sh/, "browser-smoke must build the webapp");
  assert.match(job, /npm run --prefix webapp test:unit/, "browser-smoke must run the unit lane");
  assert.match(job, /npm run --prefix webapp test:e2e/, "browser-smoke must run the browser lane");
  assert.match(built, /browser-smoke/, "the page must name the job that runs its guards");
  // `build.sh` is "the build", and a stale page has to fail there too.
  assert.match(
    read(join(WEBAPP, "build.sh")),
    /build-embed-docs\.mjs" --check|build-embed-docs\.mjs --check/,
    "build.sh must run the embedding-page generator's --check",
  );
  // And this file is in the unit lane's glob.
  assert.match(
    JSON.parse(read(join(WEBAPP, "package.json"))).scripts["test:unit"],
    /tests\/\*\.test\.mjs/,
    "test:unit must still collect tests/*.test.mjs",
  );
});

// ---- docs/126 phase 3: white-labelling ---------------------------------------

test("the refusal the page shows is one the validator really emits", () => {
  // The panel the extraction rule exempts, checked the other way round — the same
  // shape as the install panel above. The page shows OUTPUT, so "every line is in
  // the source file" is the wrong question; "the validator really says this" is the
  // right one, and it is asked by running the validator here rather than by trusting
  // the generator that wrote the panel.
  const panel = panels().find(({ caption }) => /failing palette/.test(caption));
  assert.ok(panel, "the page must show what a failing palette is told");

  const { themes } = readPalettes();
  const config = normalizeBrand(
    {
      version: 1,
      name: "Northwind Docs",
      theme: {
        tokens: { "--accent": "#f5a524", "--accent-ink": "#ffffff" },
        light: { "--muted": "#9aa0a6" },
        dark: {},
      },
    },
    themes,
  );
  const { failures } = auditBrand(config, themes);
  assert.ok(failures.length >= 4, "the sample palette no longer fails, so the page would lie");
  for (const reason of failures) {
    assert.ok(panel.code.includes(reason), `the page omits a refusal the validator emits: ${reason}`);
  }
  for (const line of panel.code.split("\n")) {
    const reason = line.replace(/^\s*-\s*/, "").trim();
    if (!reason || reason.startsWith("$") || reason.endsWith("refused:")) continue;
    assert.ok(failures.includes(reason), `the page shows a refusal the validator does not emit: ${reason}`);
  }
  // And it carries the three things a refusal has to carry to be actionable.
  assert.match(panel.code, /measures \d+\.\d+:1/, "no measured ratio");
  assert.match(panel.code, /must clear 4\.5:1/, "no floor");
  assert.match(panel.code, /#[0-9a-f]{6} would pass/, "no value that would work");
});

test("the configuration the page shows is the committed example, values intact", () => {
  // Verbatim values. The panel folds away the long `//`-keyed notes because a code
  // panel is for showing the shape, but every VALUE must be the committed one — a
  // white-label example that does not match the file the guards generate from is an
  // example nobody executes.
  const panel = panels().find(({ caption }) => /brand\.example\.json/.test(caption));
  assert.ok(panel, "the page must show the worked example");
  const source = readFileSync(join(WEBAPP, "brand.example.json"), "utf8");
  for (const line of panel.code.split("\n")) {
    if (!line.trim()) continue;
    assert.ok(source.includes(line.trim()), `the panel shows a line not in the file: ${line.trim()}`);
  }
  // The notes are folded away, and nothing else is: every non-note line survives.
  const kept = source
    .split("\n")
    .filter((line) => line.trim() && !/^\s*"\/\//.test(line))
    .map((line) => line.trim());
  const shown = new Set(panel.code.split("\n").map((line) => line.trim()));
  const dropped = kept.filter((line) => !shown.has(line) && line !== "{}");
  assert.deepEqual(dropped, [], "the panel drops configuration a host would need");
});

test("the region table states what the authority resolves, not what the page says", () => {
  const regions = REGIONS;
  const table = page.slice(page.indexOf("@generated region-table"), page.indexOf("@end region-table"));
  const readonly = resolveRegions({ mode: "readonly", framed: true });
  const preview = resolveRegions({ mode: "preview", framed: true });
  for (const id of regions) {
    const at = table.indexOf(`<code>${id}</code>`);
    assert.ok(at > 0, `${id} has no row`);
    const row = table.slice(at, table.indexOf('<div class="disp-row"', at + 1) >>> 0 || undefined);
    const cells = [...row.matchAll(/<div class="disp-cell">([^<]*)</g)].map((m) => m[1].trim());
    assert.equal(cells.length, 3, `${id}'s row has ${cells.length} cells, not 3`);
    // The last two cells are the two reading roles, in that order.
    assert.equal(cells[1] === "yes", readonly.has(id), `${id}'s readonly cell disagrees`);
    assert.equal(cells[2] === "yes", preview.has(id), `${id}'s preview cell disagrees`);
  }
  // `readonly` has no ribbon and no band, on the page as in the authority.
  assert.equal(readonly.has("ribbon"), false);
  assert.ok(regions.filter((id) => id.startsWith("band.")).every((id) => !readonly.has(id)));
});
