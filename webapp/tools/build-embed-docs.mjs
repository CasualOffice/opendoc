#!/usr/bin/env node
// Generates the code blocks, tables and numbers on `webapp/embedding.page.html`
// from the code they document.
//
// WHY THIS TOOL EXISTS, rather than a page somebody typed: the public pages of
// this repository have carried fabricated claims twice, in both directions
// (`docs/99` §9, `docs/105` EV-007) — a parity figure contradicting the fidelity
// page, a corpus file that did not exist, a "sub-10 ms" repaint with no
// benchmark, and three shipped features listed as "Not yet". `docs/126` makes
// the rule for every SDK phase page explicit: "every example on the site is
// extracted from code that runs in CI, never hand-written into the page", and
// every number is generated from a committed artifact.
//
// So this tool is the same shape as `build-site.py --check` for the static pages
// and `build-embed-package.mjs --check` for the package: it writes the generated
// parts of the page and `--check` fails when the committed page is not what a
// fresh run produces. That makes BOTH directions of drift a build failure —
// editing the page by hand, and changing the code the page describes without
// re-generating it.
//
// What is generated, and what is not:
//
//   * `<!-- @generated NAME -->` … `<!-- @end NAME -->` regions hold the code
//     panels and the tables. Their contents are produced here.
//   * `data-claim="NAME"` elements hold the numbers. Their text is produced here.
//   * Everything else on the page is prose, written by a person, and this tool
//     never touches it.
//
// Nothing in a code panel is retyped. Each one is EXTRACTED from a file that is
// exercised in CI, and the panel's caption names that file, so a reader can go
// and check. `embedding_page.test.mjs` re-derives the numbers independently,
// proves each extraction still appears verbatim in its source, and proves the
// "does not do yet" list is still true — because understating is also false.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
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

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const REPO = join(WEBAPP, "..");
const PACKAGE = join(REPO, "packages", "opendoc-embed");
const PAGE = join(WEBAPP, "embedding.page.html");

const read = (path) => readFileSync(path, "utf8");
const rel = (path) => relative(REPO, path);

/** Sources the page quotes or measures. Named once so a failure can say which
 *  file moved rather than "extraction failed". */
const SOURCES = Object.freeze({
  demo: join(WEBAPP, "src", "embed_host_demo.js"),
  element: join(WEBAPP, "src", "embed_element.mjs"),
  capabilities: join(WEBAPP, "src", "capabilities.mjs"),
  main: join(WEBAPP, "src", "main.js"),
  readme: join(PACKAGE, "README.md"),
  manifest: join(PACKAGE, "package.json"),
  gate: join(WEBAPP, "tests", "e2e", "embedded-capability-gate.spec.mjs"),
  roles: join(WEBAPP, "tests", "roles.test.mjs"),
  packaging: join(WEBAPP, "tests", "embed_package.test.mjs"),
});

/** What each capability is, in one clause.
 *
 *  The only authored column in the capability table — everything else about a
 *  capability is derived below. Keyed by the authority's own names, and
 *  `capabilityRows()` FAILS if a capability exists without a clause or a clause
 *  exists without a capability, so adding a tenth capability cannot silently
 *  ship an unexplained row. */
const MEANINGS = Object.freeze({
  open: "Replace the loaded document from inside the editor.",
  new: "Start a blank document.",
  save: "Write the document back out.",
  download: "Export to a file the visitor keeps.",
  print: "Open the browser's print dialog.",
  edit: "Change the document at all.",
  comment: "Annotate, and change the document as tracked suggestions.",
  branding: "Show OpenDoc's own name and mark inside the frame.",
  autosave: "Keep drafts of the visitor's typing on this origin.",
});

/** For a capability the editor page consults, what it decides there.
 *
 *  Authored, because the answer is a sentence about product behaviour rather
 *  than a value in a table — but WHICH capabilities appear here is derived from
 *  `main.js`, and `chromeRows()` fails if a consulted capability has no note. */
const CHROME_NOTES = Object.freeze({
  open: "File ▸ Open is present and disabled, with the reason as its title.",
  new: "File ▸ New is present and disabled, with the reason as its title.",
  edit: "Decides the review mode the editor arrives in, and whether the Editing button is offered.",
  comment: "Decides whether Suggesting is offered as a mode.",
  autosave: "Decides whether this origin keeps drafts of the visitor's typing at all.",
});

// ── Extraction ─────────────────────────────────────────────────────────────

/** A named function's source, verbatim and dedented.
 *
 *  Brace-counted from the declaration, which is exact for the functions quoted
 *  here (no braces inside their strings or comments) and LOUD when it is not:
 *  an unbalanced or missing function throws with the file name, so a rename in
 *  the source fails this tool rather than quietly publishing the wrong lines. */
function functionSource(path, name) {
  const source = read(path);
  const start = source.search(new RegExp(`^[ \\t]*function ${name}\\(`, "m"));
  if (start < 0) throw new Error(`build-embed-docs: no function ${name}() in ${rel(path)}`);
  let depth = 0;
  let end = -1;
  for (let i = source.indexOf("{", start); i < source.length; i++) {
    if (source[i] === "{") depth += 1;
    else if (source[i] === "}") {
      depth -= 1;
      if (depth === 0) {
        end = i + 1;
        break;
      }
    }
  }
  if (end < 0) throw new Error(`build-embed-docs: unbalanced ${name}() in ${rel(path)}`);
  // Keep the doc comment above the declaration: it is the part that explains
  // why the code is shaped this way, and dropping it would publish the code
  // without its reasoning.
  let from = source.lastIndexOf("\n", start) + 1;
  const lines = source.slice(0, from).split("\n");
  let comment = lines.length - 1;
  while (comment > 0 && /^\s*(\/\*\*|\*|\*\/|\/\/)/.test(lines[comment - 1])) comment -= 1;
  if (comment < lines.length - 1) from = lines.slice(0, comment).join("\n").length + 1;
  return dedent(source.slice(from, end));
}

/** The nth fenced block of a language in a Markdown file, verbatim. */
function fencedBlock(path, language, index = 0) {
  const source = read(path);
  const blocks = [...source.matchAll(new RegExp("```" + language + "\\n([\\s\\S]*?)```", "g"))];
  const block = blocks[index];
  if (!block) throw new Error(`build-embed-docs: no \`\`\`${language} block #${index} in ${rel(path)}`);
  return block[1].replace(/\n+$/, "");
}

/** Removes the common leading indentation. */
function dedent(text) {
  const lines = text.replace(/\s+$/, "").split("\n");
  const widths = lines.filter((line) => line.trim()).map((line) => line.match(/^[ \t]*/)[0].length);
  const cut = widths.length ? Math.min(...widths) : 0;
  return lines.map((line) => line.slice(cut)).join("\n");
}

// ── Derivation ─────────────────────────────────────────────────────────────

/** The package as `npm pack` sees it: the file list and unpacked size of the
 *  tarball a host would install.
 *
 *  `--dry-run` writes nothing and reaches no network. The GZIPPED size is
 *  deliberately not published: it depends on the zlib the packer happens to
 *  ship, so a number derived from it would differ between a developer's machine
 *  and CI and the guard would fail for a reason that is not a drift. The
 *  unpacked size is a function of the committed bytes alone. */
function packed() {
  const out = execFileSync("npm", ["pack", "--dry-run", "--json"], {
    cwd: PACKAGE,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  const [result] = JSON.parse(out);
  return {
    filename: result.filename,
    files: result.files.map((file) => file.path).sort(),
    unpackedKb: Math.round(result.unpackedSize / 1024),
  };
}

/** Which capabilities the EDITOR PAGE consults, read out of `main.js`.
 *
 *  Derived rather than described, because "the chrome gates this" is exactly the
 *  kind of claim `docs/99` §9.4 warns about: built is not reachable. A
 *  capability nothing reads is withheld by the sandbox or by nothing at all, and
 *  the page has to say which — so it asks the code. */
function chromeConsulted() {
  const source = read(SOURCES.main);
  const consulted = new Map();
  for (const line of source.split("\n")) {
    const match = line.match(/(?:HOST_CAPS|hostCapabilities\(\))\.has\("([a-z]+)"\)/);
    if (!match) continue;
    const id = line.match(/id: "([a-z]+\.[a-z]+)"/);
    consulted.set(match[1], id ? id[1] : null);
  }
  // `editingModeFor(HOST_CAPS)` is the other reader: it turns the set into the
  // review mode the page starts in, and it branches on exactly these two.
  if (/editingModeFor\(HOST_CAPS\)/.test(source)) {
    for (const capability of ["edit", "comment"]) {
      if (!consulted.has(capability)) consulted.set(capability, null);
    }
  }
  return consulted;
}

/** The attributes the element actually reads, parsed out of its own
 *  `MOUNT_ATTRIBUTES` table.
 *
 *  Not exported by the module, and deliberately not retyped here: the page says
 *  how many attributes an embed takes, and that number has to come from the
 *  element or it is a claim about someone else's code. */
export function mountAttributes(source = read(SOURCES.element)) {
  const table = source.match(/const MOUNT_ATTRIBUTES = Object\.freeze\(\[([^\]]*)\]\)/);
  if (!table) throw new Error(`build-embed-docs: no MOUNT_ATTRIBUTES table in ${rel(SOURCES.element)}`);
  return [...table[1].matchAll(/"([\w-]+)"/g)].map((m) => m[1]);
}

/** How many commands the devtools-attack test fires at a reader's document.
 *
 *  Read out of the spec's own enumeration. The page describes the attack, and
 *  "seventeen commands" typed into a page is the next number that drifts — the
 *  spec enumerates them precisely so that leaving one out would be the one that
 *  got through, and this counts what it enumerates. */
export function attackCommandCount(source = read(SOURCES.gate)) {
  const list = source.match(/const controls = \[([\s\S]*?)\];/);
  if (!list) throw new Error(`build-embed-docs: no controls list in ${rel(SOURCES.gate)}`);
  return [...list[1].matchAll(/"([\w]+)"/g)].length;
}

/** Which sandbox token one capability unlocks on its own — the browser-enforced
 *  layer, derived by asking `sandboxTokensFor` rather than by repeating its
 *  rules here.
 *
 *  Asked of a set holding ONLY that capability, not of the full set minus it:
 *  `allow-downloads` is granted for `download` OR `save`, so removing either one
 *  from a full set loses nothing and the subtractive reading would report that
 *  neither is enforced by the browser. Two capabilities can unlock the same
 *  token, and the page has to say so for both. */
function sandboxTokenFor(capability) {
  const alone = sandboxTokensFor(new Set([capability]));
  const unlocked = alone.filter((token) => !unconditionalTokens().includes(token));
  return unlocked.length ? unlocked.join(" ") : null;
}

/** Tokens every embed gets whatever the role, derived from the emptiest set
 *  there is. */
function unconditionalTokens() {
  return sandboxTokensFor(new Set());
}

// ── HTML ───────────────────────────────────────────────────────────────────

const escape = (text) =>
  String(text).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");

/** A captioned code panel whose caption is the file the code came from. */
function codePanel(caption, code) {
  return [
    '<div class="code-panel">',
    `  <div class="code-panel-head"><span>${escape(caption)}</span></div>`,
    `  <pre><code>${escape(code)}</code></pre>`,
    "</div>",
  ].join("\n");
}

/** A three-column `disp-table`, the docs shell's existing table component. */
function table(head, rows) {
  const cells = (row) =>
    row
      .map((cell, index) => `    <div class="${index === 0 ? "disp-name" : "disp-cell"}">${cell}</div>`)
      .join("\n");
  return [
    '<div class="disp-table">',
    '  <div class="disp-row head">',
    ...head.map((label) => `    <div>${escape(label)}</div>`),
    "  </div>",
    ...rows.map((row) => ['  <div class="disp-row">', cells(row), "  </div>"].join("\n")),
    "</div>",
  ].join("\n");
}

/** A path or import specifier as inline code, with break opportunities.
 *
 *  A 50-character mono token is one unbreakable word, and one of them pushed the
 *  whole page 65px wider than a 390px phone before this existed. `<wbr />` adds
 *  the break the browser has no other way to find, costs no characters, and
 *  needs no stylesheet change — the tokens are deliberate (`docs/63`) and a
 *  `word-break` rule on a shared component would be a restyle. */
function pathCode(text) {
  const marked = text.length > 24 ? escape(text).replaceAll("/", "/<wbr />") : escape(text);
  return `<code>${marked}</code>`;
}

/** Capability names as inline code, or an em dash for none. */
function codeList(values) {
  const list = [...values];
  return list.length ? list.map((value) => `<code>${escape(value)}</code>`).join(" ") : "—";
}

// ── The regions ────────────────────────────────────────────────────────────

function capabilityRows() {
  const unknown = Object.keys(MEANINGS).filter((name) => !CAPABILITIES.includes(name));
  if (unknown.length) {
    throw new Error(`build-embed-docs: MEANINGS names capabilities that no longer exist: ${unknown}`);
  }
  const consulted = chromeConsulted();
  return CAPABILITIES.map((capability) => {
    const meaning = MEANINGS[capability];
    if (!meaning) {
      throw new Error(
        `build-embed-docs: capability "${capability}" has no entry in MEANINGS. ` +
          "Add one clause describing it, so the page cannot ship an unexplained row.",
      );
    }
    const layers = [];
    const token = sandboxTokenFor(capability);
    if (token) layers.push(`browser (<code>${escape(token)}</code>)`);
    if (["edit", "comment"].includes(capability)) layers.push("engine (review mode)");
    if (consulted.has(capability)) layers.push("chrome");
    return [
      `<code>${capability}</code>`,
      escape(meaning),
      layers.length ? layers.join(", ") : "<b>nothing yet</b>",
    ];
  });
}

function roleRows() {
  return ROLES.map((role) => {
    const capabilities = resolveCapabilities({ mode: role, framed: true });
    const extra = sandboxTokensFor(capabilities).filter(
      (token) => !unconditionalTokens().includes(token),
    );
    return [
      `<code>${role}</code>`,
      `<code>${editingModeFor(capabilities)}</code>`,
      `${capabilities.size ? codeList(capabilities) : "Nothing at all."}<br />` +
        `Sandbox: ${extra.length ? `adds ${codeList(extra)}` : "no tokens beyond the unconditional ones"}`,
    ];
  });
}

function legacyRows() {
  return Object.entries(LEGACY_PRESETS).map(([legacy, role]) => {
    const set = resolveCapabilities({ mode: legacy, framed: true });
    return [
      `<code>${legacy}</code>`,
      role ? `Exactly <code>${role}</code>.` : "Not an alias of any role.",
      codeList(set),
    ];
  });
}

function chromeRows() {
  const consulted = chromeConsulted();
  return [...consulted.keys()]
    .sort()
    .map((capability) => {
      const note = CHROME_NOTES[capability];
      if (!note) {
        throw new Error(
          `build-embed-docs: the editor page consults "${capability}" but CHROME_NOTES ` +
            "has no entry saying what it decides there.",
        );
      }
      const id = consulted.get(capability);
      return [
        `<code>${capability}</code>`,
        id ? `<code>${escape(id)}</code>` : "—",
        escape(note),
      ];
    });
}

/** The test titles that hold this page's claims up, read out of the test files.
 *
 *  `docs/99` §9.2: prose describing a gate must name it, and a test must assert
 *  the gate is armed. So the page lists real titles rather than saying "this is
 *  tested", and renaming or deleting one of them fails this tool. */
function evidenceRows() {
  // The gate spec names its role once, in a constant, and interpolates it into
  // two test titles — so the titles are read with that constant substituted.
  // Publishing `a ${READER} host cannot mutate…` would be publishing the source
  // of a sentence instead of the sentence.
  const reader = read(SOURCES.gate).match(/const READER = "([\w-]+)"/);
  if (!reader) throw new Error(`build-embed-docs: no READER constant in ${rel(SOURCES.gate)}`);
  const titles = (path) =>
    [...read(path).matchAll(/^test\(\s*(?:`|")([^`"]+)(?:`|")/gm)].map((m) =>
      m[1].replaceAll("${READER}", reader[1]),
    );
  const sets = [
    [SOURCES.roles, ["the role chain is monotone: a higher role never loses a lower one's grant"]],
    [SOURCES.packaging, ["the package packs, installs, and imports — not just claims to"]],
    [
      SOURCES.gate,
      [
        "both embeds resolve their role before first paint, and the chrome refuses with a reason",
        "the simplest possible embed is not a standalone editor",
      ],
    ],
  ];
  /** Which lane runs a test file — the two commands CI runs, not a label. */
  const lane = (path) =>
    rel(path).includes("/e2e/") ? "<code>npm run test:e2e</code>" : "<code>npm run test:unit</code>";
  const rows = [];
  for (const [path, wanted] of sets) {
    const present = titles(path);
    for (const title of wanted) {
      if (!present.includes(title)) {
        throw new Error(
          `build-embed-docs: ${rel(path)} no longer has the test "${title}". ` +
            "The page cites it as evidence, so either restore the title or cite the new one.",
        );
      }
      rows.push([pathCode(rel(path)), lane(path), escape(title)]);
    }
  }
  // Read from the spec rather than asserted: the strongest claim on the page is
  // that a reader is behind the engine's gate on arrival, and this is the test
  // that attacks it with the chrome stripped.
  const attack = titles(SOURCES.gate).find((title) => title.includes("through the engine"));
  if (!attack) {
    throw new Error(
      `build-embed-docs: ${rel(SOURCES.gate)} no longer has the devtools-attack test ` +
        '(a title containing "through the engine").',
    );
  }
  rows.push([pathCode(rel(SOURCES.gate)), lane(SOURCES.gate), escape(attack)]);
  return rows;
}

// ── The page ───────────────────────────────────────────────────────────────

/** Region name → generated HTML. */
function regions() {
  const pack = packed();
  const manifest = JSON.parse(read(SOURCES.manifest));
  return {
    "install-commands": codePanel(
      `${rel(SOURCES.packaging)} — what the packaging guard really runs`,
      [
        `# Nothing publishes ${manifest.name} to a registry yet, so a host`,
        "# installs the tarball `npm pack` produces from the checkout.",
        `cd ${rel(PACKAGE)}`,
        "npm pack                       # -> " + pack.filename,
        `npm install /path/to/${pack.filename}`,
      ].join("\n"),
    ),
    "package-files": table(
      ["File", "Ships as", "What it is"],
      [
        [
          pathCode(manifest.main),
          `<code>import ${pathCode(`"${manifest.name}"`).replace(/^<code>|<\/code>$/g, "")}</code>`,
          "The element class, the capability authority it re-exports, and <code>defineOpenDocEditor</code>.",
        ],
        [
          pathCode("./src/embed_define.js"),
          `<code>import ${pathCode(`"${manifest.name}/define"`).replace(/^<code>|<\/code>$/g, "")}</code>`,
          "The side-effect entry: importing it registers the tag and nothing else.",
        ],
        [
          pathCode("./src/capabilities.mjs"),
          `<code>import ${pathCode(`"${manifest.name}/capabilities"`).replace(/^<code>|<\/code>$/g, "")}</code>`,
          "Resolve a capability set without mounting anything — no DOM, so it answers in Node.",
        ],
        [
          pathCode(manifest.types),
          "Types",
          "Generated from the authority's real values, so a role cannot exist in the types without existing in the code.",
        ],
      ],
    ),
    "declarative-usage": codePanel(
      `${rel(SOURCES.readme)} — the package's own usage, checked against the element`,
      fencedBlock(SOURCES.readme, "html"),
    ),
    "mount-element": codePanel(
      `${rel(SOURCES.demo)} — mountElement()`,
      functionSource(SOURCES.demo, "mountElement"),
    ),
    "mount-iframe": codePanel(
      `${rel(SOURCES.demo)} — mountIframe()`,
      functionSource(SOURCES.demo, "mountIframe"),
    ),
    "resolve-first": codePanel(
      `${rel(SOURCES.demo)} — resolveFor()`,
      functionSource(SOURCES.demo, "resolveFor"),
    ),
    "capability-table": table(["Capability", "What it is", "Enforced by"], capabilityRows()),
    "role-table": table(["Role", "Review mode", "Grants"], roleRows()),
    "legacy-table": table(["Legacy name", "Relationship", "Grants"], legacyRows()),
    "chrome-table": table(["Capability", "Command id", "What it decides"], chromeRows()),
    "evidence-table": table(["File", "Lane", "The test"], evidenceRows()),
  };
}

/** Claim name → the value the page must show. */
function claims() {
  const pack = packed();
  const manifest = JSON.parse(read(SOURCES.manifest));
  const consulted = chromeConsulted();
  return {
    "capability-count": String(CAPABILITIES.length),
    "role-count": String(ROLES.length),
    "preset-count": String(PRESET_NAMES.length),
    "package-name": manifest.name,
    "package-version": manifest.version,
    "package-file-count": String(pack.files.length),
    "package-unpacked-kb": `${pack.unpackedKb} kB`,
    "package-dependencies": String(Object.keys(manifest.dependencies ?? {}).length),
    "chrome-consulted-count": String(consulted.size),
    "chrome-unconsulted-count": String(CAPABILITIES.length - consulted.size),
    "unconditional-sandbox": unconditionalTokens().join(" "),
    "entry-point-count": String(Object.keys(manifest.exports).length),
    "attribute-count": String(mountAttributes().length),
    "attack-command-count": String(attackCommandCount()),
  };
}

/** Indents a generated block to sit where its marker sits — except inside a
 *  `<pre>`, where whitespace is content. Indenting a code panel's body would
 *  publish every line of the extracted code with eight spaces of lie in front
 *  of it, and a reader copying it would paste the indentation too. */
function indentHtml(block, indent) {
  if (!indent) return block;
  let inPre = false;
  return block
    .split("\n")
    .map((line) => {
      const opens = line.includes("<pre") && !line.includes("</pre>");
      const closes = inPre && line.includes("</pre>");
      const indented = inPre || !line ? line : indent + line;
      if (opens) inPre = true;
      if (closes) inPre = false;
      return indented;
    })
    .join("\n");
}

/** Rewrites every generated region and every claim value in the page. */
function render(page) {
  const generated = regions();
  const values = claims();
  const seenRegions = new Set();
  const seenClaims = new Set();

  let out = page.replace(
    /^([ \t]*)<!-- @generated ([\w-]+) -->\n[\s\S]*?^[ \t]*<!-- @end \2 -->/gm,
    (whole, indent, name) => {
      const block = generated[name];
      if (block === undefined) {
        throw new Error(`build-embed-docs: the page has a region "${name}" this tool does not generate`);
      }
      seenRegions.add(name);
      const body = indentHtml(block, indent);
      return `${indent}<!-- @generated ${name} -->\n${body}\n${indent}<!-- @end ${name} -->`;
    },
  );

  out = out.replace(/(data-claim="([\w-]+)"[^>]*>)([^<]*)(<)/g, (whole, open, name, _old, close) => {
    const value = values[name];
    if (value === undefined) {
      throw new Error(`build-embed-docs: the page claims "${name}", which this tool cannot derive`);
    }
    seenClaims.add(name);
    return `${open}${escape(value)}${close}`;
  });

  // Both directions: a region or claim this tool derives but the page never
  // shows is dead weight that would rot unnoticed, and the next person would
  // trust it.
  const missing = [
    ...Object.keys(generated).filter((name) => !seenRegions.has(name)).map((n) => `region ${n}`),
    ...Object.keys(values).filter((name) => !seenClaims.has(name)).map((n) => `claim ${n}`),
  ];
  if (missing.length) {
    throw new Error(`build-embed-docs: generated but never used on the page: ${missing.join(", ")}`);
  }
  return out;
}

function main() {
  const check = process.argv.includes("--check");
  const current = read(PAGE);
  const fresh = render(current);
  if (current === fresh) {
    if (check) console.log(`build-embed-docs --check: ${rel(PAGE)} is up to date.`);
    return;
  }
  if (check) {
    console.error(
      `build-embed-docs --check: ${rel(PAGE)} is stale.\n` +
        "Its generated regions or claim values are not what the code it documents produces.\n" +
        "Run `node webapp/tools/build-embed-docs.mjs`, then `webapp/build-site.py`, and commit both.",
    );
    process.exit(1);
  }
  writeFileSync(PAGE, fresh);
  console.log(`build-embed-docs: wrote ${rel(PAGE)}`);
}

main();
