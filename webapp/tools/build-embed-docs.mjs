#!/usr/bin/env node
// Generates the code blocks, tables, controls and numbers on the two SDK pages —
// `webapp/embedding.page.html` and `webapp/playground.page.html` — from the code
// they document.
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
//
// TWO PAGES, ONE TOOL, and that is the point rather than a convenience. The
// playground's CONTROLS are generated the same way its neighbour's tables are:
// one radio per role, one checkbox per capability, one per region, every label and
// every clause read from `capabilities.mjs` and from the description tables below.
// So adding a tenth capability puts a tenth switch on the playground with nobody
// editing the page — and forgetting to describe it fails the build rather than
// shipping an unexplained control. A second generator for the second page would
// have been a sixth `--check` in `build.sh` and, worse, a second place that knows
// what a capability is.
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
import { REGIONS, resolveRegions } from "../src/capabilities.mjs";
import { NOTE_CODES, OPTIONS, optionTally } from "../src/host_options.mjs";
import { TEXT_CONTRAST_FLOOR, UI_CONTRAST_FLOOR } from "../src/contrast.mjs";
import {
  BrandRefusal,
  OVERRIDABLE,
  PRODUCT,
  TAB_TITLE_POLICIES,
  auditBrand,
  normalize as normalizeBrand,
} from "./build-brand.mjs";
import { readPalettes } from "./palette_source.mjs";
import { RELEASE } from "../../packages/opendoc-embed/src/release.mjs";
import {
  COMMAND_CONTRACT,
  COMMAND_FAMILIES,
  CONTRACT_VERSION,
  HOST_EVENTS,
  PROTOCOL,
  REFUSAL_CODES,
} from "../src/host_contract.mjs";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const REPO = join(WEBAPP, "..");
const PACKAGE = join(REPO, "packages", "opendoc-embed");
const PAGE = join(WEBAPP, "embedding.page.html");
const OPTION_MAP_DOC = join(REPO, "docs", "149-ONLYOFFICE-HOST-CONFIGURATION-PARITY.md");
const PLAYGROUND = join(WEBAPP, "playground.page.html");

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
  contract: join(WEBAPP, "src", "host_contract.mjs"),
  contractTest: join(WEBAPP, "tests", "host_contract.test.mjs"),
  contractGate: join(WEBAPP, "tests", "e2e", "host-contract.spec.mjs"),
  // The Rust half of the contract. `casual-doc-sdk` declares the same events,
  // refusal codes, verbs and version, and `host_parity.rs` reads
  // `host_contract.mjs` and fails in both directions — which is the only guard on
  // this page that a browser cannot run.
  contractParity: join(REPO, "crates", "casual-doc-sdk", "src", "host_parity.rs"),
  // `docs/126` phase 3. The white-label configuration, the generator that validates
  // it, and the editor markup whose ribbon groups decide how fine a host's chrome
  // selection can be.
  brandExample: join(WEBAPP, "brand.example.json"),
  brandGenerator: join(WEBAPP, "tools", "build-brand.mjs"),
  brandTest: join(WEBAPP, "tests", "brand.test.mjs"),
  whiteLabelGate: join(WEBAPP, "tests", "e2e", "white-label.spec.mjs"),
  editorMarkup: join(WEBAPP, "editor.html"),
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

/** When each host event fires.
 *
 *  The only authored column of the event table. The NAMES and the payload FIELDS
 *  are read from `HOST_EVENTS`, and `eventRows()` fails if an event exists with no
 *  clause, so a new event cannot ship unexplained. */
const EVENT_WHEN = Object.freeze({
  ready: "Once, when the document is open and on screen.",
  change: "After every applied edit, whoever made it.",
  selection: "Whenever the caret or the selection moves.",
  save: "After File \u25B8 Save writes the document out.",
  export: "After an export writes another format out.",
  error: "When something failed that was not a command being refused.",
  refusal: "Whenever a command is refused, including one nobody asked for over the API.",
});

/** What each refusal code means, and which layer decided it.
 *
 *  Authored for the same reason, and the second column matters: a host reading
 *  \u201crefused\u201d needs to know whether to ask differently, ask later, or not ask at
 *  all. `refusalRows()` fails if a code exists with no entry. */
const REFUSAL_MEANINGS = Object.freeze({
  "unknown-command": ["No such command in this state.", "the contract"],
  "capability-withheld": [
    "The host did not grant what this command requires. Decided before dispatch, so the engine is never asked.",
    "the API",
  ],
  unavailable: [
    "The command exists and cannot run right now; the message is the chrome's own reason.",
    "the chrome",
  ],
  "engine-refused": [
    "It ran and the engine refused it \u2014 a read-only document, or Viewing mode.",
    "the engine",
  ],
  threw: ["It raised; the message is the exception's.", "the command"],
  "bad-request": ["The envelope or the arguments were not the contract's.", "the transport"],
  timeout: ["The transport gave up waiting for an answer.", "the transport"],
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
  comment: "Decides whether Suggesting is offered as a mode, and whether a comment can be resolved or deleted.",
  save: "File ▸ Save is present and disabled, with the reason as its title.",
  download: "Every File ▸ Export row is present and disabled, with the reason as its title.",
  print: "File ▸ Print is present and disabled, with the reason as its title.",
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
  // `async ` is allowed because the two-transport function is async: a panel that
  // could only quote synchronous code would silently exclude exactly the part of
  // the host contract worth showing.
  // `async` and `export` are both allowed: a panel that could only quote a private
  // synchronous function would silently exclude exactly the parts of the host
  // contract worth showing — the two-transport call and the origin policy.
  const start = source.search(
    new RegExp(`^[ \\t]*(?:export )?(?:async )?function ${name}\\(`, "m"),
  );
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
 *  unpacked size is a function of the committed bytes alone.
 *
 *  Memoized: it forks `npm`, and this tool now renders two pages that both ask. */
let packedOnce = null;
function packed() {
  if (packedOnce) return packedOnce;
  const out = execFileSync("npm", ["pack", "--dry-run", "--json"], {
    cwd: PACKAGE,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  const [result] = JSON.parse(out);
  packedOnce = {
    filename: result.filename,
    files: result.files.map((file) => file.path).sort(),
    unpackedKb: Math.round(result.unpackedSize / 1024),
  };
  return packedOnce;
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

// ── The localisation seam, inside generated markup (`109` HF-198 → HF-2xx) ──
//
// The site went through the editor's seam — `data-i18n` in the markup with the
// English beside it, `tools/build-locale.mjs` extracting it into the same
// `locales/en.json`, `i18n.mjs` resolving it — and these two pages were left out
// of that pass, correctly: everything between `@generated` and `@end` is written
// HERE, so hand-editing the pages to add attributes would have been undone by
// the next run and `--check` would have failed the build for exactly the right
// reason. Routing them means teaching THIS to emit the attribute, which is what
// the code below does.
//
// ONE KEY PER DATUM, NOT ONE PER SENTENCE. A capability's clause is keyed by the
// capability, a region's by the region, an event's by the event and a refusal's
// by its code — so renaming a sentence keeps its translations and adding a
// capability adds a key rather than silently reusing a neighbour's. The keys live
// under `site.*` rather than `site.embedding.*`/`site.playground.*` because
// the SAME clause serves both pages: `MEANINGS` is the embedding guide's "What it
// is" column and the playground's switch description, and two keys for one
// sentence is how the table and the control come to disagree about what
// `autosave` is. `keysFromSite` refuses one key with two different English
// strings, so sharing is checked rather than assumed.
//
// WHAT IS NOT ROUTED, deliberately:
//
//   * code — identifiers, paths, import specifiers, `npm run test:unit`. A
//     translated `package.json` path is a lie about someone else's repository.
//   * TEST TITLES in the evidence table. They are cited as evidence and a reader
//     is meant to be able to `grep` for them; a translated title names no test.
//   * role, capability and region NAMES. They are the API's own words, and a
//     host types them into a URL.
//   * numbers. Those are `data-claim` values, re-derived every run.
//
// So these two pages keep a floor of unroutable sites, and the ceilings in
// `no_unrouted_strings.test.mjs` are re-measured rather than driven to zero.

/** A cell or heading whose English goes through the seam.
 *
 *  `table()` and the control builders below understand this shape and emit the
 *  `data-i18n` attribute themselves, so a call site says WHAT the sentence is
 *  and never has to remember to route it. */
const say = (key, text) => ({ key, text });

/** An inline routed fragment, for a cell that is part prose and part code.
 *
 *  Used where a sentence and an identifier share a cell — "browser
 *  (`allow-downloads`)" — so the prose half is translatable and the token is
 *  left alone. Every fragment is a WHOLE clause, never half a sentence: splitting
 *  one sentence across two keys is the classic localisation defect, because word
 *  order is not a property a generator can assume. */
const said = (key, text) => `<span data-i18n="${key}">${escape(text)}</span>`;

/** Column heading → its key, or `null` for a heading that must not be
 *  translated.
 *
 *  Headings repeat across tables ("Grants" is the role table's and the legacy
 *  table's), so they are keyed by the WORDS rather than by the table: one column
 *  name, one translation, everywhere. `table()` fails on a heading that is not in
 *  here, which is what stops a new column shipping unrouted. */
const COLUMN_KEYS = Object.freeze({
  Capability: "site.column.capability",
  "What it is": "site.column.whatItIs",
  "Enforced by": "site.column.enforcedBy",
  Role: "site.column.role",
  "Review mode": "site.column.reviewMode",
  Grants: "site.column.grants",
  "Legacy name": "site.column.legacyName",
  Relationship: "site.column.relationship",
  "Command id": "site.column.commandId",
  "What it decides": "site.column.whatItDecides",
  Event: "site.column.event",
  Detail: "site.column.detail",
  "When it fires": "site.column.whenItFires",
  "Refusal code": "site.column.refusalCode",
  "What it means": "site.column.whatItMeans",
  "Decided by": "site.column.decidedBy",
  Region: "site.column.region",
  File: "site.column.file",
  "Ships as": "site.column.shipsAs",
  Lane: "site.column.lane",
  "The test": "site.column.theTest",
  // The two ROLE columns of the region table. A role name is API vocabulary — a
  // host types it into a URL — so it is the one heading that stays English on
  // purpose rather than by omission.
  readonly: null,
  preview: null,
});

/** A captioned code panel.
 *
 *  The caption is a file path and a clause about it. The PATH is code and stays
 *  as it is; the clause goes through the seam, which is why the two are separate
 *  elements rather than one string joined with an em dash. */
function codePanel(caption, code) {
  // Kept as `<span>`s rather than promoted to `<code>`: the head's typography is
  // the docs shell's and this change is about the seam, not about restyling
  // (SKILL §11 — design tokens are deliberate).
  const head = caption.key
    ? `<span>${escape(caption.path)}</span> — ${said(caption.key, caption.text)}`
    : `<span>${escape(caption.path)}</span>`;
  return [
    '<div class="code-panel">',
    `  <div class="code-panel-head">${head}</div>`,
    `  <pre><code>${escape(code)}</code></pre>`,
    "</div>",
  ].join("\n");
}

/** A three-column `disp-table`, the docs shell's existing table component.
 *
 *  A cell is either raw HTML (code, or prose already wrapped by `said`) or a
 *  `say(key, text)`, in which case the ROW ELEMENT carries the attribute and
 *  there is no extra span — a `disp-cell` that holds one sentence is exactly the
 *  element the seam wants. */
function table(head, rows) {
  const cells = (row) =>
    row
      .map((cell, index) => {
        const className = index === 0 ? "disp-name" : "disp-cell";
        if (cell && typeof cell === "object" && cell.key) {
          return `    <div class="${className}" data-i18n="${cell.key}">${escape(cell.text)}</div>`;
        }
        return `    <div class="${className}">${cell}</div>`;
      })
      .join("\n");
  const heading = (label) => {
    if (!Object.hasOwn(COLUMN_KEYS, label)) {
      throw new Error(
        `build-embed-docs: the column heading "${label}" has no entry in COLUMN_KEYS. ` +
          "Add a key so it goes through the localisation seam, or map it to null if it " +
          "is API vocabulary that must stay English.",
      );
    }
    const key = COLUMN_KEYS[label];
    return key
      ? `    <div data-i18n="${key}">${escape(label)}</div>`
      : `    <div>${escape(label)}</div>`;
  };
  return [
    '<div class="disp-table">',
    '  <div class="disp-row head">',
    ...head.map(heading),
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
    // Each LAYER is a whole clause with its own key, and the sandbox token
    // inside the browser one is left as code. Three clauses and a comma, not one
    // sentence cut into pieces.
    const layers = [];
    const token = sandboxTokenFor(capability);
    if (token) {
      layers.push(`${said("site.layer.browser", "browser")} (<code>${escape(token)}</code>)`);
    }
    if (["edit", "comment"].includes(capability)) {
      layers.push(said("site.layer.engine", "engine (review mode)"));
    }
    if (consulted.has(capability)) layers.push(said("site.layer.chrome", "chrome"));
    return [
      `<code>${capability}</code>`,
      say(`site.capability.${capability}`, meaning),
      layers.length
        ? layers.join(", ")
        : `<b data-i18n="site.layer.nothingYet">nothing yet</b>`,
    ];
  });
}

function roleRows() {
  return ROLES.map((role) => {
    const capabilities = resolveCapabilities({ mode: role, framed: true });
    const extra = sandboxTokensFor(capabilities).filter(
      (token) => !unconditionalTokens().includes(token),
    );
    // "Sandbox:" is a LABEL in front of a list of tokens, not the opening of a
    // sentence — so it routes as a label and the tokens stay as code.
    const sandbox = extra.length
      ? `${said("site.role.sandboxAdds", "Sandbox adds")} ${codeList(extra)}`
      : said("site.role.sandboxNone", "Sandbox: no tokens beyond the unconditional two");
    return [
      `<code>${role}</code>`,
      `<code>${editingModeFor(capabilities)}</code>`,
      `${
        capabilities.size
          ? codeList(capabilities)
          : said("site.role.grantsNothing", "Nothing at all.")
      }<br />${sandbox}`,
    ];
  });
}

function legacyRows() {
  return Object.entries(LEGACY_PRESETS).map(([legacy, role]) => {
    const set = resolveCapabilities({ mode: legacy, framed: true });
    return [
      `<code>${legacy}</code>`,
      role
        ? `${said("site.legacy.exactly", "Exactly")} <code>${role}</code>.`
        : say("site.legacy.notAnAlias", "Not an alias of any role."),
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
        say(`site.chrome.${capability}`, note),
      ];
    });
}

function eventRows() {
  return HOST_EVENTS.map((event) => {
    const when = EVENT_WHEN[event.name];
    if (!when) {
      throw new Error(
        `build-embed-docs: the host contract declares the "${event.name}" event but ` +
          "EVENT_WHEN has no clause saying when it fires.",
      );
    }
    return [
      `<code>${event.name}</code>`,
      codeList(event.detail),
      say(`site.event.${event.name}`, when),
    ];
  });
}

function refusalRows() {
  return REFUSAL_CODES.map((code) => {
    const entry = REFUSAL_MEANINGS[code];
    if (!entry) {
      throw new Error(
        `build-embed-docs: the host contract declares the "${code}" refusal code but ` +
          "REFUSAL_MEANINGS has no entry for it.",
      );
    }
    const [meaning, decided] = entry;
    return [
      `<code>${code}</code>`,
      say(`site.refusal.${code}.meaning`, meaning),
      say(`site.refusal.${code}.decidedBy`, decided),
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
  // A test's name, in the language the file is written in. A Rust `#[test] fn`
  // has no title string, and its function name IS the sentence — underscores and
  // all — so it is published as the identifier it is rather than a prose
  // paraphrase nothing could check.
  const titles = (path) =>
    path.endsWith(".rs")
      ? [...read(path).matchAll(/#\[test\]\s*\nfn ([a-z0-9_]+)\(/g)].map((m) => m[1])
      : [...read(path).matchAll(/^test\(\s*(?:`|")([^`"]+)(?:`|")/gm)].map((m) =>
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
    [
      SOURCES.contractTest,
      [
        "both transports return the identical result, for every command in the contract",
        "nothing is ever posted to a wildcard, in either direction",
        "a message from an origin the editor does not know is dropped in silence",
      ],
    ],
    [
      SOURCES.contractGate,
      [
        "the contract covers the command registry exactly, in every state a family needs",
        "a refusal the CHROME makes is a refusal the API makes",
        "a command the contract calls ungated really does not touch the document",
      ],
    ],
    [
      SOURCES.contractParity,
      [
        "the_editor_and_this_crate_declare_the_same_host_events",
        "the_editor_and_this_crate_declare_the_same_refusal_codes",
        "a_runtime_event_maps_onto_a_host_event_rather_than_a_new_name",
      ],
    ],
  ];
  /** Which lane runs a test file — the two commands CI runs, not a label. */
  const lane = (path) => {
    if (path.endsWith(".rs")) return "<code>cargo test</code>";
    return rel(path).includes("/e2e/")
      ? "<code>npm run test:e2e</code>"
      : "<code>npm run test:unit</code>";
  };
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

/** How many ribbon groups carry no `data-group`, which is what decides whether a
 *  host could address one.
 *
 *  Counted from the markup rather than stated, because the page says the number and
 *  a number typed onto a page is the next one that drifts. The day somebody labels
 *  the remaining groups, this falls and the page's "does not do yet" bullet has to
 *  come down with it — which is the whole point of deriving it. */
export function unnamedRibbonGroups(source = read(SOURCES.editorMarkup)) {
  const groups = [...source.matchAll(/<div class="rgroup[^"]*"([^>]*)>/g)];
  if (groups.length < 20) {
    throw new Error("build-embed-docs: too few .rgroup elements found; the scan is looking wrong");
  }
  return groups.filter(([, attrs]) => !/\bdata-group=/.test(attrs)).length;
}

/** The worked example, with its long explanatory notes folded away.
 *
 *  Verbatim values, trimmed comments. The `//`-keyed notes in that file are written
 *  for whoever edits it and run to several lines each; a code panel is for showing
 *  the SHAPE, and a reader who wants the reasoning follows the link in the caption.
 *  Nothing is re-typed: every line comes out of the committed file. */
function brandExampleShown() {
  const source = read(SOURCES.brandExample);
  const kept = source
    .split("\n")
    .filter((line) => !/^\s*"\/\/[^"]*":/.test(line))
    .join("\n")
    // Two blank-ish artefacts of dropping a note from the head of a block.
    .replace(/\{\n(\s*)\}/g, "{}");
  return kept.trimEnd();
}

/** A real refusal, produced by running the validator now.
 *
 *  A plausible first attempt rather than a contrived one: a bright brand orange with
 *  white text on it, and a grey that looks fine on white in a design tool. Generated
 *  rather than pasted, so the page cannot describe a message the code no longer
 *  emits — the defect `docs/99` §9 exists to prevent, in its prose form. */
function sampleRefusal() {
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
  if (!failures.length) {
    throw new Error("build-embed-docs: the sample palette no longer fails, so the page would lie");
  }
  return ["$ node webapp/tools/build-brand.mjs", "brand.json was refused:"]
    .concat(failures.map((reason) => `  - ${reason}`))
    .join("\n");
}

/** Region → what it is, and whether the two reading roles show it.
 *
 *  The two columns are asked of `resolveRegions`, so the page cannot disagree with
 *  the authority about what a `readonly` container has. The description is the only
 *  authored column, and a missing one FAILS rather than publishing a blank cell. */
const REGION_MEANINGS = Object.freeze({
  brand: "Our name and mark in the top bar.",
  title: "The document name, and renaming it.",
  state: "The document-state chips and Document properties in the top bar.",
  menu: "The application menu bar — one of the two navigation axes.",
  ribbon: "The whole tabbed ribbon, strip and bands together.",
  "band.file": "The File page: open, save, export, print, properties, settings.",
  "band.home": "Character and paragraph formatting, styles, clipboard, undo.",
  "band.insert": "Tables, pictures, shapes, links, comments, headers, symbols.",
  "band.layout": "Page setup, margins, columns, spacing, watermark, arrange.",
  "band.references": "Captions, cross-references, contents, footnotes, fields.",
  "band.review": "Tracked changes, comments, proofing, the review mode control.",
  "band.view": "Outline, zoom, page setup, showing changes.",
  "band.table": "The contextual table band, present when the caret is in a table.",
  rail: "The left navigation rail: outline and page thumbnails.",
  ruler: "The ruler strip above the page: indents and tab stops.",
  caret: "The text caret and the selection highlight on the page.",
  context: "The right-click menu on the document.",
  objects: "Object selection: the outline, its handles and the object bar.",
  history: "The version-history timeline, its ribbon entry and its preview bar.",
  review: "The Editing / Suggesting / Read-only switcher in the top bar.",
  status: "The status bar: counts, page number, language, mode.",
  zoom: "The zoom cluster in the status bar.",
  find: "The find and replace card.",
  selection: "The floating toolbar above a selection.",
  settings: "The settings dialog and the gear that opens it.",
});

function regionRows() {
  const readonly = resolveRegions({ mode: "readonly", framed: true });
  const preview = resolveRegions({ mode: "preview", framed: true });
  const unexplained = REGIONS.filter((id) => !REGION_MEANINGS[id]);
  if (unexplained.length) {
    throw new Error(`build-embed-docs: regions with no description: ${unexplained.join(", ")}`);
  }
  const extra = Object.keys(REGION_MEANINGS).filter((id) => !REGIONS.includes(id));
  if (extra.length) {
    throw new Error(`build-embed-docs: described regions that do not exist: ${extra.join(", ")}`);
  }
  // "yes" and "—" are not prose — a bare lowercase word and a dash — so the
  // scanner already leaves them alone and routing them would put "yes" in
  // eighteen catalogues for no reader's benefit.
  const mark = (on) => (on ? "yes" : "—");
  return REGIONS.map((id) => [
    `<code>${escape(id)}</code>`,
    say(regionKey(id), REGION_MEANINGS[id]),
    mark(readonly.has(id)),
    mark(preview.has(id)),
  ]);
}

/** A region's key.
 *
 *  The descriptions are written once in `REGION_MEANINGS`, keyed once here, and
 *  land in `locales/en.json` through `build-locale.mjs` like every other site
 *  string — which is also why ADDING a region now lowers these pages' unrouted
 *  count rather than raising it.
 *
 *  Keyed on the region id with `.` swapped for `-`: a catalogue key is a dotted
 *  path, so `band.file` would otherwise nest a level deeper than its siblings,
 *  and nothing downstream has to escape it. */
const regionKey = (id) => `site.region.${id.replaceAll(".", "-")}`;

// ── The playground's controls (`webapp/playground.page.html`) ──────────────
//
// A control per role, per capability and per region, generated from the authority
// rather than typed into the page. The rule this enforces is the one `docs/126`'s
// container policy states: "anything a role can express, an explicit capability
// list must also be able to express, and the two must resolve through the same
// code". A page whose switches were hand-listed would be a third table, and the
// first thing to drift.
//
// THE SHAPE OF THESE CONTROLS IS THE CONFIGURATOR PATTERN, not an invention:
// a compact option panel beside a preview that never leaves the screen, the way
// TinyMCE's configurator, CKEditor's online builder, the Monaco playground and
// Stripe's Elements demos are all built. Three consequences show up here rather
// than in the page, because this file writes the markup:
//
//   1. A ROLE IS A CARD, not a radio with an essay beside it. Name, one line of
//      what it is, and the three facts that differ between roles as pills. The
//      rest of the argument — why `preview` is not `readonly` minus print — moves
//      into a note that appears for the SELECTED role only, so it is one
//      paragraph on screen instead of five.
//   2. A CAPABILITY OR REGION IS A ONE-LINE TOGGLE, grouped. Nineteen flat rows
//      is a list nobody reads; five groups with counts is a list somebody scans.
//      The groups are asserted to be a partition of the authority's own order, so
//      a new region cannot quietly land outside one.
//   3. EVERY CLAUSE STILL COMES FROM ONE TABLE. `MEANINGS` and `REGION_MEANINGS`
//      serve the embedding guide's columns and these controls both, so the page
//      and the table cannot disagree about what `autosave` is.

/** What each role IS, in one line, and WHY, in the rest of it.
 *
 *  The only authored column; everything else in a role's card is asked of
 *  `resolveCapabilities` / `resolveRegions`. Split in two because the card shows
 *  `is` and the note shows `why`: one line is what a control can carry, and the
 *  argument still has to be somewhere. Not one string cut in half at render time —
 *  where a sentence ends is an editorial decision, and a generator that guessed it
 *  would produce a different card the day somebody added a comma.
 *
 *  `preview` and `readonly` are the two that matter and the two that get the most
 *  words, because `docs/126` is explicit that they are NOT the same thing and the
 *  playground is where a reader can watch the difference. */
const ROLE_MEANINGS = Object.freeze({
  preview: {
    is: "A picture of the document — a thumbnail, an attachment preview, a print preview.",
    why: "The runtime as a layout and rendering engine, and nearly replaceable by a static image.",
  },
  readonly: {
    is: "A published document, to read.",
    why: "Not preview-minus-print: a reader navigates, searches and gets to page 40, so this one keeps reading chrome.",
  },
  commentor: {
    is: "Google Docs' Commenter, Word's reviewer.",
    why: "Annotates and suggests; every body change is a tracked revision somebody else accepts.",
  },
  edit: {
    is: "The reason a host embeds an editor.",
    why: "Changes the document and keeps it; may not reach for a different one, and advertises nothing of ours.",
  },
  owner: {
    is: "The page is ours, or the host has said it may as well be.",
    why: "",
  },
});

/** An accent value that really does fail the AA floor, for the page's
 *  &ldquo;try a colour that fails&rdquo; button.
 *
 *  Published as a claim rather than typed into the markup, and ASSERTED to still
 *  fail below — a button that promises a refusal and produces none would be the
 *  page teaching the opposite of the lesson. A plausible first attempt, not a
 *  contrived one: a bright brand orange, the kind a design tool hands over. */
const FAILING_ACCENT = "#f5a524";

/** The role the playground opens on.
 *
 *  `edit` rather than `commentor` (which is where `embed.html` opens) because this
 *  page is subtractive: a visitor arrives at a normal editor with everything on and
 *  learns the contract by taking things away and watching them go. Starting narrow
 *  would make the first interaction "add something back", which the authority
 *  refuses by design — a list only ever narrows.
 *
 *  Declared here and nowhere else: `playground.js` reads which radio the markup
 *  has `checked` rather than carrying its own default, so this constant is the one
 *  statement of it — and it is also what the page's Reset returns to, because a
 *  reset reads `defaultChecked` off the same attribute. */
const DEFAULT_ROLE = "edit";

/** How the capability switches are grouped, and in what order.
 *
 *  A PARTITION OF `CAPABILITIES` IN THE AUTHORITY'S OWN ORDER, asserted below.
 *  That is stricter than "every capability appears somewhere" and it is the
 *  version worth having: the page's controls then read in the same order as the
 *  authority's list, the embedding guide's table and the readout chips, so three
 *  surfaces describing one contract cannot present it three ways. A tenth
 *  capability lands outside a group and fails the build. */
const CAPABILITY_GROUPS = Object.freeze([
  Object.freeze({
    title: say("site.group.inAndOut", "Getting documents in and out"),
    ids: ["open", "new", "save", "download", "print"],
  }),
  Object.freeze({
    title: say("site.group.changing", "Changing the document"),
    ids: ["edit", "comment"],
  }),
  Object.freeze({
    title: say("site.group.insideTheFrame", "Inside the frame"),
    ids: ["branding", "autosave"],
  }),
]);

/** How the chrome switches are grouped — the same partition rule, and one of the
 *  five folded shut.
 *
 *  The eight ribbon bands are the reason this page needed grouping at all:
 *  nineteen flat switches pushed the brand controls a screen and a half below the
 *  fold, and the previous fix — a 330px scroller — hid rows inside a box with no
 *  label saying how many were in there. A `<details>` says `8` on its summary, is
 *  a real keyboard control, and sits directly under the `ribbon` switch that
 *  contains it, so a reader who has not decided about `ribbon` has no business
 *  inside it. The other four groups are open, because folding a two-row group
 *  costs a click and saves nothing. */
const REGION_GROUPS = Object.freeze([
  Object.freeze({
    title: say("site.group.topBar", "Top bar"),
    ids: ["brand", "title", "state", "review", "menu", "ribbon"],
  }),
  Object.freeze({
    title: say("site.group.ribbonBands", "Ribbon bands"),
    ids: REGIONS.filter((id) => id.startsWith("band.")),
    fold: true,
  }),
  Object.freeze({
    title: say("site.group.besideTheDocument", "Beside the document"),
    ids: ["rail", "ruler", "caret", "context", "objects", "history"],
  }),
  Object.freeze({ title: say("site.group.statusBar", "Status bar"), ids: ["status", "zoom"] }),
  Object.freeze({
    title: say("site.group.overlays", "Overlays"),
    ids: ["find", "selection", "settings"],
  }),
]);

/** Fails unless the groups are exactly the authority's list, in order. */
function partition(groups, all, what) {
  const flat = groups.flatMap((group) => group.ids);
  if (flat.join(" ") !== all.join(" ")) {
    throw new Error(
      `build-embed-docs: the playground's ${what} groups are not a partition of the ` +
        `authority's list in its own order.\n  groups: ${flat.join(" ")}\n  ${what}: ${all.join(" ")}\n` +
        "Every one gets exactly one switch, and the switches read in the order the " +
        "authority declares — which is also the order of the table on the embedding " +
        "page and of the chips in the readout.",
    );
  }
  return groups;
}

/** The dom id a control gets. `.` is not legal in a `for=`-friendly id here only
 *  by convention, but `band.file` in a CSS selector would need escaping in every
 *  test that reaches for it, so the dots become dashes once, here. */
function domId(group, value) {
  return `pg-${group}-${value.replace(/\./g, "-")}`;
}

/** One capability or region switch: the control, its name, and one clause.
 *
 *  `<label for>` rather than a wrapping label, because the clause sits in a third
 *  grid cell and a label wrapping all three would read the whole paragraph as the
 *  control's name to a screen reader. */
function toggleRow({ group, value, what, attribute }) {
  const id = domId(group, value);
  return [
    '  <li class="pg-toggle">',
    `    <input type="checkbox" class="pg-check" id="${id}" value="${escape(value)}" ${attribute} />`,
    // The NAME is the capability or region id — API vocabulary a host types into
    // a URL — so it stays as it is; the clause beside it is the sentence.
    `    <label class="pg-toggle-name" for="${id}">${escape(value)}</label>`,
    `    <span class="pg-toggle-what" data-i18n="${what.key}">${escape(what.text)}</span>`,
    "  </li>",
  ].join("\n");
}

/** A group of switches, as a disclosure carrying its own count. */
function foldGroup({ title, count, open, body }) {
  return [
    `<details class="pg-fold"${open ? " open" : ""}>`,
    `  <summary class="pg-fold-head"><span class="pg-fold-title" data-i18n="${title.key}">` +
      `${escape(title.text)}</span>` +
      `<span class="pg-fold-count">${count}</span></summary>`,
    '  <ul class="pg-toggles">',
    body,
    "  </ul>",
    "</details>",
  ].join("\n");
}

/** The role cards.
 *
 *  Each card carries the three facts that actually differ between roles — how many
 *  capabilities it grants, how much chrome it is offered, and which review mode it
 *  lands in — derived from the authority rather than described. The full grant list
 *  goes in the note below, which is the one place a reader gets a paragraph. */
function roleControls() {
  const unexplained = ROLES.filter((role) => !ROLE_MEANINGS[role]?.is);
  if (unexplained.length) {
    throw new Error(
      `build-embed-docs: role(s) with no clause in ROLE_MEANINGS: ${unexplained.join(", ")}. ` +
        "The playground puts a control on screen for each one, and a control with no " +
        "explanation is a control a host has to guess at.",
    );
  }
  const cards = ROLES.map((role) => {
    const capabilities = resolveCapabilities({ mode: role, framed: true });
    const shown = resolveRegions({ mode: role, framed: true, capabilities });
    const id = domId("role", role);
    return [
      '  <li class="pg-choice">',
      `    <input type="radio" class="pg-check" name="role" id="${id}" value="${escape(role)}" data-role${role === DEFAULT_ROLE ? " checked" : ""} />`,
      `    <label class="pg-choice-name" for="${id}">${escape(role)}</label>`,
      `    <span class="pg-choice-what" data-i18n="site.role.${role}.is">${escape(ROLE_MEANINGS[role].is)}</span>`,
      '    <span class="pg-choice-meta">',
      // A COUNT AND A NOUN, not a sentence with numbers in it. "6 of 9
      // capabilities" reads as one string and would have to go into the
      // catalogue with its numbers baked in — so it is a ratio the generator
      // writes and a noun the seam translates, which is also how the numbers stay
      // derived rather than published inside a translation.
      `      <span class="pg-pill"><b class="pg-pill-n">${capabilities.size}/${CAPABILITIES.length}</b> ` +
        `${said("site.pill.capabilities", "capabilities")}</span>`,
      `      <span class="pg-pill"><b class="pg-pill-n">${shown.size}/${REGIONS.length}</b> ` +
        `${said("site.pill.regions", "regions")}</span>`,
      `      <span class="pg-pill pg-pill--mode">${editingModeFor(capabilities)}</span>`,
      "    </span>",
      "  </li>",
    ].join("\n");
  });
  // One note per role, all five present and four of them `hidden`. `playground.js`
  // flips which one is shown and writes nothing: the page's language stays in the
  // page, which is the contract that keeps that module out of the string table.
  // THREE LABELLED FACTS, not one sentence assembled from fragments.
  //
  // The note used to read "<why> Grants <a> <b>, and is offered 6 of 19 chrome
  // regions." — one English sentence with code and numbers spliced into it, which
  // cannot go through the seam whole (the extractor reads the text up to the
  // first tag) and must not go through it in pieces (word order is not a property
  // a generator may assume). So it becomes the argument, then a labelled grant
  // list, then a labelled ratio: each label is a complete clause a translator can
  // work with, and the derived parts stay derived.
  const notes = ROLES.map((role) => {
    const capabilities = resolveCapabilities({ mode: role, framed: true });
    const shown = resolveRegions({ mode: role, framed: true, capabilities });
    const why = ROLE_MEANINGS[role].why
      ? `<span data-i18n="site.role.${role}.why">${escape(ROLE_MEANINGS[role].why)}</span> `
      : "";
    const grants = capabilities.size
      ? codeList(capabilities)
      : said("site.role.grantsNothing", "Nothing at all.");
    return (
      `  <p class="pg-role-note" data-role-note="${escape(role)}"${role === DEFAULT_ROLE ? "" : " hidden"}>` +
      `${why}<span class="pg-role-fact">${said("site.role.grantsLabel", "Grants")} ${grants}</span> ` +
      `<span class="pg-role-fact">${said("site.role.chromeLabel", "Chrome regions offered")} ` +
      `<b class="pg-pill-n">${shown.size}/${REGIONS.length}</b></span></p>`
    );
  });
  return [
    '<ul class="pg-choices">',
    ...cards,
    "</ul>",
    '<div class="pg-role-notes" data-role-notes>',
    ...notes,
    "</div>",
  ].join("\n");
}

/** The capability checkboxes, described by the same `MEANINGS` table the
 *  embedding guide's capability column uses — one statement of what a capability
 *  is, serving a table and a control. */
function capabilityControls() {
  const unexplained = CAPABILITIES.filter((capability) => !MEANINGS[capability]);
  if (unexplained.length) {
    throw new Error(
      `build-embed-docs: capability(s) with no entry in MEANINGS: ${unexplained.join(", ")}. ` +
        "The playground puts a switch on screen for each one.",
    );
  }
  return partition(CAPABILITY_GROUPS, CAPABILITIES, "capability")
    .map((group) =>
      foldGroup({
        title: group.title,
        count: group.ids.length,
        open: true,
        body: group.ids
          .map((capability) =>
            toggleRow({
              group: "cap",
              value: capability,
              attribute: "data-capability",
              what: say(`site.capability.${capability}`, MEANINGS[capability]),
            }),
          )
          .join("\n"),
      }),
    )
    .join("\n");
}

/** The region checkboxes, described by `REGION_MEANINGS` — likewise shared with
 *  the region table on the embedding page. */
function regionControls() {
  const unexplained = REGIONS.filter((id) => !REGION_MEANINGS[id]);
  if (unexplained.length) {
    throw new Error(
      `build-embed-docs: region(s) with no description: ${unexplained.join(", ")}. ` +
        "The playground puts a switch on screen for each one.",
    );
  }
  return partition(REGION_GROUPS, REGIONS, "region")
    .map((group) =>
      foldGroup({
        title: group.title,
        count: group.ids.length,
        open: !group.fold,
        body: group.ids
          .map((id) =>
            toggleRow({
              group: "chrome",
              value: id,
              attribute: "data-region",
              what: say(regionKey(id), REGION_MEANINGS[id]),
            }),
          )
          .join("\n"),
      }),
    )
    .join("\n");
}

/** The brand fields. Generated because two of their values are facts about the
 *  code: the tab-title options are `TAB_TITLE_POLICIES`, and the accent the
 *  picker opens on is the editor's own `--accent`, read out of `style.css`. A
 *  swatch that opened on a colour the editor does not use would be the page's
 *  first small lie. */
function brandControls() {
  const { themes } = readPalettes();
  const accent = themes.get("light")?.["--accent"];
  if (!/^#[0-9a-f]{6}$/i.test(accent ?? "")) {
    throw new Error(
      `build-embed-docs: the editor's --accent reads ${JSON.stringify(accent)}, which a ` +
        "colour input cannot open on. The playground's picker needs a six-digit hex.",
    );
  }
  const options = TAB_TITLE_POLICIES.map(
    (policy) => `    <option value="${policy}">${escape(policy)}</option>`,
  ).join("\n");
  // The PLACEHOLDER is the product's own name and is not translated — it is a
  // proper noun, and the site header leaves it unrouted for the same reason.
  return [
    '<p class="pg-field">',
    '  <label for="pg-brand-name" data-i18n="site.brand.productName">Product name</label>',
    `  <input type="text" id="pg-brand-name" data-brand-name placeholder="${PRODUCT.name}" autocomplete="off" spellcheck="false" />`,
    "</p>",
    '<p class="pg-field">',
    '  <label for="pg-brand-accent-hex" data-i18n="site.brand.accent">Accent colour</label>',
    '  <span class="pg-colour-row">',
    `    <input type="color" id="pg-brand-accent" value="${accent}" data-brand-accent data-i18n-label="site.brand.accentSwatch" aria-label="Accent colour, as a swatch" />`,
    `    <input type="text" id="pg-brand-accent-hex" value="${accent}" data-brand-accent-hex autocomplete="off" spellcheck="false" data-i18n-label="site.brand.accentHex" aria-label="Accent colour, as hex" />`,
    "  </span>",
    "</p>",
    '<p class="pg-field">',
    '  <label for="pg-brand-tab-title" data-i18n="site.brand.tabTitle">The host browser tab says</label>',
    '  <select id="pg-brand-tab-title" data-brand-tab-title>',
    options,
    "  </select>",
    "</p>",
  ].join("\n");
}

/** The four host inputs that travel on the editor's URL, read out of
 *  `hostConfig()` rather than listed.
 *
 *  The playground's whole output is a URL, so the parameter names are the page's
 *  most load-bearing claim about someone else's code. Read in source order. */
export function urlParameters(source = read(SOURCES.capabilities)) {
  const body = source.match(/export function hostConfig\([\s\S]*?\n}/);
  if (!body) throw new Error(`build-embed-docs: no hostConfig() in ${rel(SOURCES.capabilities)}`);
  const names = [...body[0].matchAll(/params\??\.get\("([\w-]+)"\)/g)].map((m) => m[1]);
  const unique = [...new Set(names)];
  if (unique.length < 4) {
    throw new Error(
      `build-embed-docs: hostConfig() reads only ${unique.length} URL parameter(s) ` +
        `(${unique.join(", ")}); the playground documents four.`,
    );
  }
  return unique;
}

/** The refusal `FAILING_ACCENT` really produces, as the generator's own message.
 *
 *  Not published on the page — the page computes it live, in the browser, from the
 *  same `normalize`/`auditBrand` in `src/brand_contract.mjs`. What this does is
 *  PROVE the button's promise: a "try a colour that fails" control whose colour
 *  has quietly started passing would teach the opposite of the lesson, and the day
 *  the palette moves under it this throws instead. */
function failingAccentRefusal() {
  const { themes } = readPalettes();
  const config = normalizeBrand(
    { version: 1, theme: { tokens: { "--accent": FAILING_ACCENT }, light: {}, dark: {} } },
    themes,
  );
  const { failures } = auditBrand(config, themes);
  if (!failures.length) {
    throw new Error(
      `build-embed-docs: ${FAILING_ACCENT} no longer fails the AA audit, so the playground's ` +
        "“try an accent that fails” button would promise a refusal and produce none. " +
        "Pick a value that does fail, or take the button out.",
    );
  }
  return new BrandRefusal(failures).message;
}

/** Region name → generated HTML, for the playground. */
function playgroundRegions() {
  return {
    "playground-role-controls": roleControls(),
    "playground-capability-controls": capabilityControls(),
    "playground-region-controls": regionControls(),
    "playground-brand-controls": brandControls(),
  };
}

/** Claim name → value, for the playground. */
function playgroundClaims() {
  failingAccentRefusal(); // Throws if the button's promise has stopped being true.
  return {
    "role-count": String(ROLES.length),
    "capability-count": String(CAPABILITIES.length),
    "region-count": String(REGIONS.length),
    "band-count": String(REGIONS.filter((id) => id.startsWith("band.")).length),
    "overridable-token-count": String(OVERRIDABLE.length),
    "attribute-count": String(mountAttributes().length),
    "url-parameters": urlParameters().join(" "),
    "text-contrast-floor": String(TEXT_CONTRAST_FLOOR),
    "ui-contrast-floor": String(UI_CONTRAST_FLOOR),
    "failing-accent": FAILING_ACCENT,
  };
}

/** Region name → generated HTML, for the embedding guide. */
function regions() {
  const pack = packed();
  const manifest = JSON.parse(read(SOURCES.manifest));
  return {
    "install-commands": codePanel(
      {
        path: rel(SOURCES.packaging),
        key: "site.panel.installCommands",
        text: "what the packaging guard really runs",
      },
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
          // The clause is the sentence; `defineOpenDocEditor` is an export name
          // and stays as code, which is why this cell is a fragment and not a
          // `say()`.
          `${said("site.file.main", "The element class, the capability authority it re-exports, and")} <code>defineOpenDocEditor</code>.`,
        ],
        [
          pathCode("./src/embed_define.js"),
          `<code>import ${pathCode(`"${manifest.name}/define"`).replace(/^<code>|<\/code>$/g, "")}</code>`,
          say("site.file.define", "The side-effect entry: importing it registers the tag and nothing else."),
        ],
        [
          pathCode("./src/capabilities.mjs"),
          `<code>import ${pathCode(`"${manifest.name}/capabilities"`).replace(/^<code>|<\/code>$/g, "")}</code>`,
          say("site.file.capabilities", "Resolve a capability set without mounting anything — no DOM, so it answers in Node."),
        ],
        [
          pathCode("./src/host_contract.mjs"),
          `<code>import ${pathCode(`"${manifest.name}/contract"`).replace(/^<code>|<\/code>$/g, "")}</code>`,
          say("site.file.contract", "The host contract: what each command requires, which events exist, what a refusal can say, and the envelope. No DOM."),
        ],
        [
          pathCode("./src/host_client.mjs"),
          `<code>import ${pathCode(`"${manifest.name}/client"`).replace(/^<code>|<\/code>$/g, "")}</code>`,
          `<code>createHostClient</code> \u2014 ${said("site.file.client", "the postMessage transport, for a host that cannot reach into the frame.")}`,
        ],
        [
          pathCode(manifest.types),
          say("site.file.typesColumn", "Types"),
          say("site.file.types", "Generated from the authority's real values, so a role cannot exist in the types without existing in the code."),
        ],
      ],
    ),
    "declarative-usage": codePanel(
      {
        path: rel(SOURCES.readme),
        key: "site.panel.declarativeUsage",
        text: "the package's own usage, checked against the element",
      },
      fencedBlock(SOURCES.readme, "html"),
    ),
    "mount-element": codePanel(
      { path: `${rel(SOURCES.demo)} — mountElement()` },
      functionSource(SOURCES.demo, "mountElement"),
    ),
    "mount-iframe": codePanel(
      { path: `${rel(SOURCES.demo)} — mountIframe()` },
      functionSource(SOURCES.demo, "mountIframe"),
    ),
    "resolve-first": codePanel(
      { path: `${rel(SOURCES.demo)} — resolveFor()` },
      functionSource(SOURCES.demo, "resolveFor"),
    ),
    "command-transports": codePanel(
      { path: `${rel(SOURCES.demo)} \u2014 runCommand()` },
      functionSource(SOURCES.demo, "runCommand"),
    ),
    "origin-policy": codePanel(
      { path: `${rel(SOURCES.contract)} \u2014 parseOriginAllowlist()` },
      functionSource(SOURCES.contract, "parseOriginAllowlist"),
    ),
    "event-table": table(["Event", "Detail", "When it fires"], eventRows()),
    "refusal-table": table(["Refusal code", "What it means", "Decided by"], refusalRows()),
    "capability-table": table(["Capability", "What it is", "Enforced by"], capabilityRows()),
    "role-table": table(["Role", "Review mode", "Grants"], roleRows()),
    "legacy-table": table(["Legacy name", "Relationship", "Grants"], legacyRows()),
    "chrome-table": table(["Capability", "Command id", "What it decides"], chromeRows()),
    // ── docs/126 phase 3 ────────────────────────────────────────────────────
    // The worked example, verbatim from the committed file rather than retyped:
    // `tests/brand.test.mjs` generates from this exact JSON and audits the result,
    // and `tests/e2e/white-label.spec.mjs` serves what it produces into a real
    // editor. So the snippet on the page is the input to a test that runs in CI,
    // which is the only kind of example `docs/99` §9 permits.
    "brand-config": codePanel(
      {
        path: rel(SOURCES.brandExample),
        key: "site.panel.brandConfig",
        text: "a white-label, and the input to its own guard",
      },
      brandExampleShown(),
    ),
    // A REAL refusal, produced at generate time by running the validator on a
    // palette that fails. Not a transcript somebody pasted: the day the message
    // changes, this page changes with it, and the day the validator stops refusing,
    // `brand.test.mjs` goes red first.
    "brand-refusal": codePanel(
      {
        path: rel(SOURCES.brandGenerator),
        key: "site.panel.brandRefusal",
        text: "what a failing palette is told",
      },
      sampleRefusal(),
    ),
    "region-table": table(
      ["Region", "What it is", "readonly", "preview"],
      regionRows(),
    ),
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
    "contract-version": String(CONTRACT_VERSION),
    "command-count": String(COMMAND_CONTRACT.length),
    "family-count": String(COMMAND_FAMILIES.length),
    "event-count": String(HOST_EVENTS.length),
    "refusal-code-count": String(REFUSAL_CODES.length),
    "verb-count": String(PROTOCOL.requests.length),
    "verbs": PROTOCOL.requests.join(" "),
    // `docs/126` phase 3.
    "region-count": String(REGIONS.length),
    "band-count": String(REGIONS.filter((id) => id.startsWith("band.")).length),
    "unnamed-group-count": String(unnamedRibbonGroups()),
    "overridable-token-count": String(OVERRIDABLE.length),
    "reading-region-count": String(resolveRegions({ mode: "readonly", framed: true }).size),
    "preview-region-count": String(resolveRegions({ mode: "preview", framed: true }).size),
    "release-engine": RELEASE.engine,
    "release-licence": RELEASE.licence,
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

/** Markdown table cells may not contain a pipe, and a note that did would split
 *  a row in half silently. */
const cell = (text) => String(text ?? "").replaceAll("|", "\\|").replaceAll("\n", " ");

/** The verdict column's wording, one per code, so the document reads as prose
 *  rather than as an enum dump. Keyed off `NOTE_CODES`, and the generator throws
 *  on a code with no wording — a new answer cannot reach the published table
 *  unexplained. */
const VERDICT = Object.freeze({
  mapped: "Have it",
  narrowed: "Have it, differently",
  "cannot-widen": "Refused by design",
  unsupported: "Gap — work named",
  declined: "Declined",
  elsewhere: "Another product",
  unknown: "n/a",
});

/** The option map, as the two generated regions of `docs/149`. */
function optionMapRegions() {
  const missing = NOTE_CODES.filter((code) => !VERDICT[code]);
  if (missing.length) {
    throw new Error(`build-embed-docs: no verdict wording for: ${missing.join(", ")}`);
  }
  const tally = optionTally();
  const summary = [
    "| Verdict | Options | What it means |",
    "| --- | --- | --- |",
    ...NOTE_CODES.filter((code) => code !== "unknown").map(
      (code) => `| ${VERDICT[code]} | ${tally[code]} | ${cell(CODE_MEANING[code])} |`,
    ),
    `| **Total enumerated** | **${OPTIONS.length}** | every leaf of their \`permissions\`, \`editorConfig\` and \`customization\` |`,
  ];
  const rows = [
    "| Theirs | Ours | Verdict | Note |",
    "| --- | --- | --- | --- |",
    ...OPTIONS.map(
      (row) =>
        `| \`${cell(row.option)}\` | ${row.ours ? `\`${cell(row.ours)}\`` : "—"} | ${VERDICT[row.code]} | ${
          cell(row.note) || "—"
        } |`,
    ),
  ];
  return { "option-tally": summary.join("\n"), "option-map": rows.join("\n") };
}

/** What each verdict is, for the summary table. Separate from `VERDICT` because
 *  one is a label and the other is a definition, and a table whose two columns
 *  came from one string would be saying the same thing twice. */
const CODE_MEANING = Object.freeze({
  mapped: "lowered onto one of our three axes, and the resolved container really changes",
  narrowed: "honoured in part; the note says what is different and why",
  "cannot-widen": "it would GRANT something, and a host list may only narrow — reported, naming `mode`",
  unsupported: "the editor has no such state or surface yet; the note names the work",
  declined: "deliberately not wanted, with the argument",
  elsewhere: "belongs to spreadsheets, presentations or PDF forms (`SKILL` §1)",
});

/** The pages this tool owns, each with the region and claim maps it uses.
 *
 *  Two of them, and the region/claim namespaces are PER PAGE rather than shared:
 *  both pages publish a `capability-count`, derived the same way from the same
 *  authority, and merging the namespaces would have meant naming the second one
 *  `playground-capability-count` — a second name for one fact, which is the shape
 *  of defect this whole tool exists to prevent. The "generated but never used"
 *  check below therefore runs per page, so a region the playground defines and
 *  never shows fails even though the embedding guide is complete. */
const PAGES = Object.freeze([
  Object.freeze({ path: PAGE, regions, claims }),
  Object.freeze({ path: PLAYGROUND, regions: playgroundRegions, claims: playgroundClaims }),
  // The option map. A DOCUMENT rather than a site page, and deliberately: the two
  // pages above are localised — every authored clause carries a `data-i18n` key —
  // and the map is 121 rows of two text columns, so publishing it there would put
  // 240-odd developer-facing sentences in front of nineteen translators to no
  // benefit. `docs/` is where this repository keeps prose that is about the code
  // rather than in the product, and the table is generated into it for the same
  // reason the pages are generated: a hand-maintained count has drifted into a
  // false public claim here twice (`SKILL` §8).
  Object.freeze({ path: OPTION_MAP_DOC, regions: optionMapRegions, claims: () => ({}) }),
]);

/** Rewrites every generated region and every claim value in one page. */
function render(page, build) {
  const generated = build.regions();
  const values = build.claims();
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
  const stale = [];
  const written = [];
  for (const build of PAGES) {
    const current = read(build.path);
    const fresh = render(current, build);
    if (current === fresh) continue;
    if (check) {
      stale.push(rel(build.path));
      continue;
    }
    writeFileSync(build.path, fresh);
    written.push(rel(build.path));
  }
  if (check) {
    if (stale.length) {
      console.error(
        `build-embed-docs --check: ${stale.join(", ")} stale.\n` +
          "The generated regions or claim values are not what the code they document produce.\n" +
          "Run `node webapp/tools/build-embed-docs.mjs`, then `webapp/build-site.py`, and commit both.",
      );
      process.exit(1);
    }
    console.log(
      `build-embed-docs --check: ${PAGES.map((build) => rel(build.path)).join(", ")} up to date.`,
    );
    return;
  }
  for (const path of written) console.log(`build-embed-docs: wrote ${path}`);
  if (!written.length) console.log("build-embed-docs: both pages were already current.");
}

main();
