#!/usr/bin/env node
// Publishes the repository's publishable design docs as real pages of this site.
//
// WHY THIS EXISTS. The site's own navigation used to send a reader — and a
// crawler — to `github.com/CasualOffice/opendoc/blob/main/docs/*.md`. That is an
// unstyled page on somebody else's domain: it ranks for github.com, it carries
// none of our chrome, an assistant quoting it cites GitHub, and the reader leaves
// the site to read our architecture. Fifteen such links sat in four page
// templates. Each one is now either a local page generated here, or a link to a
// doc this generator DELIBERATELY WITHHOLDS with the reason recorded below.
//
// WHAT IS GENERATED, AND FROM WHAT. Everything. Each page is
//
//     the shared chrome (`_partials/site-header.html`, `_partials/site-footer.html`)
//   + a head whose title and description are read out of the source Markdown
//   + the source Markdown, rendered
//
// and nothing else. There is no hand-written prose on these pages at all, which
// is the strongest available form of `docs/99` §9.1 ("a published number is
// generated from a committed artifact, or it is not published"): not one number,
// grade or claim on a reference page can be typed, because every word of the
// article body is a function of the committed `.md`. `--check` fails the build
// when a committed page is not what a fresh run produces, so drift is caught in
// BOTH directions — a page edited by hand, and a source doc that moved without
// the page being regenerated. Same contract as `build-site.py --check` and
// `build-embed-docs.mjs --check`.
//
// WHY A SEPARATE GENERATOR RATHER THAN `build-site.py`. Two reasons, and the
// second is a gate. `build-site.py` inlines partials into hand-authored
// `*.page.html` bodies; there is no Markdown here to inline. And these pages must
// live somewhere the string ratchet (`tools/string_sites.mjs`) does not read:
// `scanTree` counts `editor.html`, every root-level `*.page.html` and every
// `_partials/*.html`, and a generated page's English is not debt a person can pay
// down by routing a literal through `t()` — the English is in `docs/`. So the
// exclusion is STRUCTURAL and a property of WHERE THESE PAGES LIVE
// (`webapp/reference/`), not an allowlist: nothing hand-written can reach the
// exclusion without being moved into a directory this generator owns and
// overwrites. `tests/doc_pages.test.mjs` asserts the exclusion holds, and
// publishes — as one measured number — exactly how much English these pages put
// on the site, so the exclusion cannot hide the size of what it excludes.
import { mkdirSync, readFileSync, readdirSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const REPO = join(WEBAPP, "..");

/** The directory every generated page lives in, relative to `webapp/`.
 *
 *  Load-bearing, not cosmetic: it is what keeps these pages out of the string
 *  ratchet's scan tree and out of `build-site.py`'s template glob. */
export const OUT_DIR = "reference";

export const GITHUB = "https://github.com/CasualOffice/opendoc";
const BLOB = `${GITHUB}/blob/main`;

/** The docs published as pages, in reading order.
 *
 *  The SET IS A RULE, not a taste call: it is every repository Markdown document
 *  the site already linked to, minus the withheld list below. That is what makes
 *  it answerable — the owner's report was about the site's own links, and a guard
 *  (`tests/doc_pages.test.mjs`) asserts that every `.md` the site links is either
 *  in this list or in `WITHHELD` with a reason.
 *
 *  `group` is the rail heading a page files under; `nav` is its rail label, kept
 *  short because a 60-character `<h1>` is not a sidebar entry. Both are structure,
 *  not claims. */
export const PUBLISHED = [
  {
    source: "docs/02-ARCHITECTURE.md",
    slug: "architecture",
    group: "Architecture",
    nav: "Target architecture",
  },
  {
    source: "docs/28-DOCX-PACKAGE-READER.md",
    slug: "docx-package-reader",
    group: "Architecture",
    nav: "DOCX package reader",
  },
  {
    source: "docs/35-DISPOSITION-TAXONOMY.md",
    slug: "disposition-taxonomy",
    group: "Architecture",
    nav: "Import dispositions",
  },
  {
    source: "docs/98-PDF-EXPORT-AND-PRINT-DESIGN.md",
    slug: "pdf-export-and-print",
    group: "Architecture",
    nav: "PDF export and print",
  },
  {
    source: "docs/05-SDK-API-SPEC.md",
    slug: "sdk-api-spec",
    group: "SDK",
    nav: "SDK API specification",
  },
  {
    source: "docs/20-ERROR-CODE-REGISTRY.md",
    slug: "error-code-registry",
    group: "SDK",
    nav: "Error code registry",
  },
  {
    source: "docs/126-EMBEDDABILITY-AND-SDK-THREE-PHASE-PLAN.md",
    slug: "embeddability-plan",
    group: "SDK",
    nav: "Embeddability, in three phases",
    // Its first paragraph is a dated owner instruction and its second is a list of
    // cross-references: provenance, not a summary. This section is the document
    // saying what the work is for, in its own words.
    summaryFrom: "The through-line",
  },
  {
    source: "docs/84-CONTEXT-MENU-AND-COMMAND-REGISTRY-DESIGN.md",
    slug: "command-registry",
    group: "Editor",
    nav: "Command registry",
  },
  {
    source: "docs/17-GLOSSARY.md",
    slug: "glossary",
    group: "Project",
    nav: "Glossary",
  },
  {
    source: "CONTRIBUTING.md",
    slug: "contributing",
    group: "Project",
    nav: "Contributing",
  },
  {
    source: "SECURITY.md",
    slug: "security",
    group: "Project",
    nav: "Security policy",
  },
];

/** Docs the site links but this generator will NOT publish, each with the
 *  evidence.
 *
 *  An omission nothing argues for is indistinguishable from an oversight, so this
 *  list is the mirror of `build-seo.mjs`'s `EXCLUDED`: a reason on every entry,
 *  the list kept short, and `tests/doc_pages.test.mjs` asserting both that the
 *  reasons are argued and that the pages really are absent. A link to a withheld
 *  doc stays a link to GitHub — that is correct, not a leftover: GitHub is the
 *  source of record for a document we are deliberately not presenting as current
 *  site content. */
export const WITHHELD = [
  {
    source: "docs/60-FIDELITY-CORPUS-RENDERING-AUDIT.md",
    reason:
      "A dated audit that cannot be re-derived here, and that its own body says " +
      "not to trust. It carries a staleness banner added 2026-09-15 putting it " +
      "992 commits behind main, stating that it systematically UNDERSTATES the " +
      "engine because capability has since shipped for gaps it lists as open, and " +
      "instructing the reader not to cite it as evidence of a present gap without " +
      "re-checking the code. Re-deriving its cells is not possible in this " +
      "repository: all five probe documents it measures are deliberately untracked " +
      "under doc 23's rights policy and are named individually in `.gitignore`, so " +
      "the inputs are gone. Understating is as false as overstating (docs/99 §9.6), " +
      "and the current per-construct state is already published, from " +
      "`webapp/src/fidelity.js`, at /fidelity.html. Its one exception — the " +
      "page-count parity figure — is cited from that page by name and date, which " +
      "is the only use this document's own banner permits.",
  },
  {
    source: "docs/18-SUPPORT-MATRIX.md",
    reason:
      "A support matrix whose own staleness note (2026-09-15) says it had drifted " +
      "behind the code in one consistent direction, that several rows still read " +
      "'edit surface pending' for surfaces that now edit, and that any remaining " +
      "'pending' needs a code check before it is quoted — naming " +
      "`webapp/src/fidelity.js` as the artifact to trust when the two disagree. " +
      "That artifact is already published as /fidelity.html, under an honesty " +
      "guard. Publishing this document as a second indexable answer to the same " +
      "question would put two disagreeing support statements on one domain, which " +
      "is the shape of the EV-001/EV-002 defect docs/99 §9 exists to prevent.",
  },
  {
    source: "docs/106-ONLYOFFICE-ALTERNATIVE-ROADMAP.md",
    reason:
      "Self-declared an archive, closed to new rows since 2026-09-20, whose banner " +
      "says its phase tables still name rows that have since closed and must be " +
      "read as rationale rather than as a work list. A published roadmap is read as " +
      "a work list — that is what a roadmap is for — so publishing this one hands a " +
      "reader and an assistant a picture of what is open that the document itself " +
      "disclaims. It also routes the reader to `109-BACKLOG.md`, a queue withheld " +
      "on the owner's instruction.",
  },
  {
    source: "docs/06-ROADMAP-AND-DELIVERY.md",
    reason:
      "Contradicted by the current product order, in the part a reader would act " +
      "on. Its product-sequencing paragraph (2026-07-25) positions the Tauri " +
      "desktop application ahead of the public editing SDK and the browser " +
      "editor; the site is built web-first with Tauri deferred, and the owner's " +
      "delivery order set 2026-09-25 runs authoring, then mobile, then " +
      "embeddability, then the SDK, with collaboration last. Publishing a " +
      "superseded sequencing statement as current site content is a false claim " +
      "whichever direction it points.",
  },
  {
    source: "docs/83-SDK-PACKAGING-EMBEDDING-AND-EXTENSIBILITY-ARCHITECTURE.md",
    reason:
      "Tells the reader, in its overview and in present tense, that developers " +
      "install OpenDoc via `npm install @casualoffice/document-runtime`. No such " +
      "package exists: the package this repository ships is " +
      "`@casualoffice/opendoc-embed` (packages/opendoc-embed/package.json), and " +
      "this document's own Phase 3 task list records publishing the other name to " +
      "npm as future work. A self-contained paragraph that an assistant will quote " +
      "as an install command, and that fails for whoever runs it, is exactly the " +
      "defect `tests/site_claims.test.mjs` guards the landing page against. The " +
      "architecture is worth publishing once the install line names a package that " +
      "exists; reported for the owner to file rather than edited here.",
  },
  {
    source: "docs/40-FONT-MANAGEMENT-DESIGN.md",
    reason:
      "Refused by this generator's `openingIsProse` rule rather than by a judgement " +
      "call: its first section says nothing in prose, so it has no summary to " +
      "publish. Its opening paragraph is a provenance line (`Status: Draft for " +
      "review. Owner: … Depends on: … Feeds: …`) and §1 is headings and bulleted " +
      "implementation items carrying source coordinates like `definitions.rs l.235`. " +
      "MEASURED, and this is why the rule exists: publishing it anyway was tried, " +
      "and the description the scan reached was a sentence from a Synthesis " +
      "subsection of §2 — a three-way comparative claim about ONLYOFFICE, " +
      "LibreOffice and CSS Fonts 4 ending 'the embedded-font correctness that all " +
      "three products lack'. That would have been this page's search result, its " +
      "social card, and the one line an assistant quotes: a competitive assertion " +
      "nobody here re-derived, presented as what the page is about. Publish it when " +
      "the document gains an opening that says what font management does.",
  },
  {
    source: "docs/14-EXECUTION-TRACKER.md",
    reason:
      "An execution tracker, withheld on the owner's instruction together with " +
      "docs/104 (the hotfix queue), docs/105 (the September 2026 audit rows) and " +
      "docs/109 (the backlog). Publishing a ranked list of our own open defects and " +
      "in-flight rows as indexable pages is not useful to a reader evaluating the " +
      "project, and it is not what publishing the docs was asked to achieve. The " +
      "fidelity page already publishes the honest per-construct state, which is the " +
      "part of this material a reader needs.",
  },
  {
    source: "docs/105-AUDIT-2026-09-TRACKER.md",
    reason:
      "An audit tracker: ninety-three ranked rows of our own open defects, each with " +
      "a status. Withheld on the owner's instruction, with docs/14, 104 and 109. The " +
      "fidelity page cites individual rows from it by id where a gap is being " +
      "claimed, which is a citation of a source of record rather than publication of " +
      "the queue.",
  },
];

// ---------------------------------------------------------------------------
// Markdown → HTML
//
// A small, deterministic renderer for the subset these documents use, rather
// than a dependency: `webapp/package.json` has exactly one devDependency
// (Playwright) and the site ships no build step beyond these generators. The
// subset is not guessed — it is what a survey of the published set actually
// contains: ATX headings to depth 3, fenced code with a language, GFM pipe
// tables, nested bullet and ordered lists, block quotes, thematic breaks, and
// inline code / strong / emphasis / links. Anything outside it renders as plain
// text rather than as markup, so an unsupported construct degrades to something
// readable instead of to broken HTML. `tests/doc_pages.test.mjs` pins each rule.
// ---------------------------------------------------------------------------

const unescapeHtml = (text) =>
  text
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&");

const escapeHtml = (text) =>
  text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");

/** A heading's fragment id: lowercase words joined by hyphens. Deduplicated by
 *  the caller, because two `### Goals` in one document must not both be `#goals`. */
export function slugify(text) {
  return (
    text
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "section"
  );
}

/** Where a Markdown link target points once the page is on this site.
 *
 *  Three cases, in order: a published doc becomes the local page (this is the
 *  whole point of the exercise); any other repository path becomes a link to the
 *  file on GitHub, because that is where it actually is; anything already absolute
 *  is left alone. A bare `#fragment` stays a fragment. */
export function resolveTarget(href, { published }) {
  const raw = href.trim();
  if (/^[a-z][a-z0-9+.-]*:/i.test(raw) || raw.startsWith("//")) return raw;
  if (raw.startsWith("#")) return raw;
  const [path, fragment = ""] = raw.split("#");
  const hash = fragment ? `#${fragment}` : "";
  // `05-SDK-API-SPEC.md` (doc-relative) and `docs/05-SDK-API-SPEC.md` (repo-relative)
  // name the same file, and both spellings occur.
  const repoPath = path.replace(/^\.\//, "");
  const candidates = [repoPath, `docs/${repoPath}`];
  for (const candidate of candidates) {
    const local = published.get(candidate);
    if (local) return `./${local}.html${hash}`;
  }
  const known = candidates.find((candidate) => repoFileExists(candidate));
  if (known) return `${BLOB}/${known}${hash}`;
  return `${BLOB}/${repoPath}${hash}`;
}

let repoFiles = null;
function repoFileExists(path) {
  if (!repoFiles) {
    repoFiles = new Set([
      ...readdirSync(REPO).map((name) => name),
      ...readdirSync(join(REPO, "docs")).map((name) => `docs/${name}`),
    ]);
  }
  return repoFiles.has(path);
}

/** Inline markup. Code spans are extracted first and restored last, so nothing
 *  inside backticks is re-interpreted as emphasis or a link. */
export function inline(text, context) {
  const codes = [];
  let work = text.replace(/`([^`]+)`/g, (_, code) => {
    codes.push(code);
    return `\u0001${codes.length - 1}\u0002`;
  });
  work = escapeHtml(work);
  work = work.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, label, href) => {
    const target = resolveTarget(unescapeHtml(href), context);
    return `<a href="${escapeHtml(target)}">${label}</a>`;
  });
  work = work.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  // Emphasis is `*only*`. Underscores are left alone on purpose: these documents
  // are full of `snake_case` identifiers and `w:rFonts`-style names, and treating
  // `_` as emphasis turns `font_table_id` into markup.
  work = work.replace(/(^|[^*\w])\*([^*\n]+)\*(?=$|[^*\w])/g, "$1<em>$2</em>");
  return work.replace(/\u0001(\d+)\u0002/g, (_, index) => `<code>${escapeHtml(codes[Number(index)])}</code>`);
}

const FENCE = /^\s*```\s*([A-Za-z0-9_+-]*)\s*$/;
const HEADING = /^(#{1,6})\s+(.*)$/;
const RULE = /^(-{3,}|\*{3,}|_{3,})\s*$/;
const BULLET = /^(\s*)([-*+])\s+(.*)$/;
const ORDERED = /^(\s*)(\d+)[.)]\s+(.*)$/;
const QUOTE = /^\s*>\s?(.*)$/;
const TABLE_ROW = /^\s*\|/;
// One dash is enough, because `:-:` is a legal GFM centre marker and the first
// version of this required two — which silently rendered a centre-aligned table as
// three paragraphs of pipes. No document in the published set uses that spelling
// today; the guard in `tests/doc_pages.test.mjs` does, which is how it was found.
const TABLE_DIVIDER = /^\s*\|(\s*:?-+:?\s*\|)+\s*$/;

/** Splits a Markdown source into blocks. Returned as data rather than HTML so
 *  the same walk can produce the page body, the on-this-page list, and the
 *  description without three parsers disagreeing about what a paragraph is. */
export function blocks(source) {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const out = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i += 1;
      continue;
    }
    const fence = line.match(FENCE);
    if (fence) {
      const body = [];
      i += 1;
      while (i < lines.length && !FENCE.test(lines[i])) {
        body.push(lines[i]);
        i += 1;
      }
      i += 1; // the closing fence, or the end of the file
      out.push({ kind: "code", language: fence[1], text: body.join("\n") });
      continue;
    }
    const heading = line.match(HEADING);
    if (heading) {
      out.push({ kind: "heading", depth: heading[1].length, text: heading[2].trim() });
      i += 1;
      continue;
    }
    if (RULE.test(line)) {
      out.push({ kind: "rule" });
      i += 1;
      continue;
    }
    if (TABLE_ROW.test(line) && TABLE_DIVIDER.test(lines[i + 1] ?? "")) {
      const rows = [];
      while (i < lines.length && TABLE_ROW.test(lines[i])) {
        rows.push(lines[i]);
        i += 1;
      }
      out.push({ kind: "table", rows });
      continue;
    }
    if (QUOTE.test(line)) {
      const body = [];
      while (i < lines.length && (QUOTE.test(lines[i]) || (body.length && lines[i].trim()))) {
        body.push(lines[i].match(QUOTE)?.[1] ?? lines[i].trim());
        i += 1;
      }
      out.push({ kind: "quote", source: body.join("\n") });
      continue;
    }
    if (BULLET.test(line) || ORDERED.test(line)) {
      const items = [];
      while (i < lines.length && lines[i].trim()) {
        const bullet = lines[i].match(BULLET);
        const ordered = bullet ? null : lines[i].match(ORDERED);
        if (bullet || ordered) {
          const match = bullet ?? ordered;
          items.push({
            indent: match[1].length,
            ordered: Boolean(ordered),
            text: match[3],
          });
        } else if (items.length) {
          // A lazy continuation line belongs to the item above it.
          items[items.length - 1].text += ` ${lines[i].trim()}`;
        } else {
          break;
        }
        i += 1;
      }
      out.push({ kind: "list", items });
      continue;
    }
    const paragraph = [];
    while (
      i < lines.length &&
      lines[i].trim() &&
      !HEADING.test(lines[i]) &&
      !FENCE.test(lines[i]) &&
      !RULE.test(lines[i]) &&
      !QUOTE.test(lines[i]) &&
      !BULLET.test(lines[i]) &&
      !ORDERED.test(lines[i]) &&
      !TABLE_ROW.test(lines[i])
    ) {
      paragraph.push(lines[i].trim());
      i += 1;
    }
    if (!paragraph.length) {
      // Defensive: a line that starts a block shape none of the branches above
      // consumed would otherwise spin here forever.
      out.push({ kind: "paragraph", text: lines[i].trim() });
      i += 1;
      continue;
    }
    out.push({ kind: "paragraph", text: paragraph.join(" ") });
  }
  return out;
}

const cells = (row) =>
  row
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split(/(?<!\\)\|/)
    .map((cell) => cell.replace(/\\\|/g, "|").trim());

function renderTable(rows, context) {
  const head = cells(rows[0]);
  const alignments = cells(rows[1]).map((spec) => {
    if (/^:-+:$/.test(spec)) return "center";
    if (/^-+:$/.test(spec)) return "right";
    return null;
  });
  const align = (index) => (alignments[index] ? ` style="text-align:${alignments[index]}"` : "");
  const body = rows.slice(2).map((row) => cells(row));
  const lines = ['<div class="doc-table-scroll">', '<table class="doc-table">', "<thead>", "<tr>"];
  head.forEach((cell, index) => lines.push(`<th${align(index)}>${inline(cell, context)}</th>`));
  lines.push("</tr>", "</thead>", "<tbody>");
  for (const row of body) {
    lines.push("<tr>");
    row.forEach((cell, index) => lines.push(`<td${align(index)}>${inline(cell, context)}</td>`));
    lines.push("</tr>");
  }
  lines.push("</tbody>", "</table>", "</div>");
  return lines.join("\n");
}

function renderList(items, context) {
  // Indent-driven nesting. `depth` is the number of open lists, so an item
  // indented past the one above opens a list and an item indented less closes as
  // many as it has to.
  const html = [];
  const stack = [];
  for (const item of items) {
    while (stack.length && item.indent < stack[stack.length - 1].indent) {
      html.push(stack.pop().ordered ? "</ol>" : "</ul>");
    }
    if (!stack.length || item.indent > stack[stack.length - 1].indent) {
      stack.push({ indent: item.indent, ordered: item.ordered });
      html.push(item.ordered ? '<ol class="doc-list">' : '<ul class="doc-list">');
    }
    html.push(`<li>${inline(item.text, context)}</li>`);
  }
  while (stack.length) html.push(stack.pop().ordered ? "</ol>" : "</ul>");
  return html.join("\n");
}

/** Renders blocks to HTML, and collects the H2 outline on the way.
 *
 *  `headings` is an out-parameter so the on-this-page rail is built from the ids
 *  the body actually emitted, rather than from a second pass that could compute a
 *  different set. */
export function renderBlocks(list, context, headings = []) {
  const seen = new Map();
  const id = (text) => {
    const base = slugify(text);
    const count = (seen.get(base) ?? 0) + 1;
    seen.set(base, count);
    return count === 1 ? base : `${base}-${count}`;
  };
  const html = [];
  for (const block of list) {
    switch (block.kind) {
      case "heading": {
        const level = Math.min(Math.max(block.depth, 2), 4);
        const anchor = id(block.text);
        if (level === 2) headings.push({ id: anchor, text: block.text });
        html.push(
          `<h${level} class="doc-h${level}" id="${anchor}">${inline(block.text, context)}</h${level}>`,
        );
        break;
      }
      case "paragraph":
        html.push(`<p class="doc-body">${inline(block.text, context)}</p>`);
        break;
      case "code":
        html.push(
          '<div class="code-panel">',
          block.language
            ? `<div class="code-panel-head"><span>${escapeHtml(block.language)}</span></div>`
            : "",
          `<pre><code>${escapeHtml(block.text)}</code></pre>`,
          "</div>",
        );
        break;
      case "list":
        html.push(renderList(block.items, context));
        break;
      case "table":
        html.push(renderTable(block.rows, context));
        break;
      case "quote":
        html.push(
          '<blockquote class="doc-quote">',
          renderBlocks(blocks(block.source), context),
          "</blockquote>",
        );
        break;
      case "rule":
        html.push('<hr class="doc-rule" />');
        break;
      default:
        throw new Error(`build-doc-pages: no renderer for block kind ${block.kind}`);
    }
  }
  return html.filter(Boolean).join("\n");
}

/** Markdown inline markup removed, for a `<title>` or a meta description. */
export function plain(text) {
  return text
    .replace(/`([^`]*)`/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/\*([^*]+)\*/g, "$1")
    .replace(/\s+/g, " ")
    .trim();
}

/** A document's provenance line, not its summary.
 *
 *  `Status: Accepted for Phase 0`, `Depends on: docs 05, 14, 15`,
 *  `Owner instruction, 2026-09-27: …` — a short capitalised label, then a colon,
 *  then filing information. A reader skips it and a search snippet made of it says
 *  nothing about the page. Bounded to four words before the colon so it cannot
 *  swallow an ordinary sentence that happens to contain one. */
const PREAMBLE_LINE = /^\*{0,2}[A-Z][A-Za-z0-9/-]*(?:[ ,][A-Za-z0-9/-]+){0,3}:\*{0,2}(\s|$)/;
const isPreamble = (text) => PREAMBLE_LINE.test(plain(text));

/** A paragraph that introduces a list or a table rather than saying something.
 *
 *  "Every error crossing the SDK, WASM, C ABI, or serialized operation boundary
 *  has:" is true and useless on its own — the content is the six bullets under it,
 *  which a meta description cannot carry. Quoted by an assistant it is a sentence
 *  with its predicate missing, so it is skipped and the next real paragraph is
 *  used. This is the `docs/99` §9 "no claim that needs the rest of the page for
 *  context" rule applied to the one sentence search results actually show. */
const isStem = (text) => text.endsWith(":");

/** The page's own summary, in the document's words.
 *
 *  Derived, never written: the first paragraphs of real prose, joined until there
 *  is enough of them to be a search snippet, cut at a sentence boundary. Block
 *  quotes are skipped (they are staleness banners and admonitions, not summaries)
 *  and so is the metadata preamble.
 *
 *  Returns `null` when a document has no prose paragraph at all. That is a
 *  publishing decision, not an error to paper over: a document with no
 *  self-contained summary cannot be quoted by an assistant and will not rank, and
 *  the caller refuses to publish it rather than inventing one. */
/** The blocks a summary may be drawn from: the whole document, or one named
 *  section of it.
 *
 *  A manifest entry may name a section with `summaryFrom`. That is a choice of
 *  WHERE to read, never of WHAT to say: the text is still the document's own, the
 *  heading has to exist (this throws if it does not), and the alternative was
 *  worse — a document whose first paragraph is a dated owner instruction or a list
 *  of cross-references gets a search result that describes neither the document
 *  nor the project. Used sparingly; most pages take the default. */
export function section(list, heading, sourcePath) {
  if (!heading) return list;
  const start = list.findIndex(
    (block) => block.kind === "heading" && plain(block.text) === heading,
  );
  if (start < 0) {
    throw new Error(
      `${sourcePath}: summaryFrom names the section "${heading}", which this document does ` +
        `not have. The section was renamed or removed — pick one that exists, or drop the hint.`,
    );
  }
  const depth = list[start].depth;
  const end = list.findIndex(
    (block, index) => index > start && block.kind === "heading" && block.depth <= depth,
  );
  return list.slice(start + 1, end < 0 ? undefined : end);
}

/** The blocks a summary may be drawn from when no section is named: all of them,
 *  unless the document's FIRST SECTION says nothing in prose — in which case,
 *  none.
 *
 *  A summary may CONTINUE past the opening (`docs/02`'s first section is one
 *  sentence and a diagram, and its description is that sentence plus the next one),
 *  but it may not START deep in a document. That distinction was found by
 *  measurement, not chosen on principle: `docs/40` has no opening prose at all — a
 *  provenance line, then headings and bulleted implementation items — and the
 *  first eligible paragraph the scan reached was a three-way competitive claim
 *  inside a "Synthesis" subsection of section 2. Published, that sentence would
 *  have been the page's search result, its social card and the one line an
 *  assistant quotes: a comparative assertion about two other products, nobody
 *  re-derived, presented as what the page is about.
 *
 *  So a document whose first section has nothing to say has no summary, and the
 *  caller refuses to publish it rather than quoting it out of context. Naming a
 *  section with `summaryFrom` opts out, because there the choice is deliberate and
 *  reviewed. */
export function openingIsProse(list) {
  let sections = 0;
  for (const block of list) {
    if (block.kind === "heading" && block.depth === 2) {
      sections += 1;
      if (sections === 2) return false;
    }
    if (block.kind !== "paragraph") continue;
    const text = plain(block.text);
    if (text && !isPreamble(text) && !isStem(text)) return true;
  }
  return false;
}

export function summarise(list, { min = 110, max = 300 } = {}) {
  const sentences = [];
  let length = 0;
  for (const block of list) {
    if (block.kind !== "paragraph") continue;
    const text = plain(block.text);
    if (!text || isPreamble(text) || isStem(text)) continue;
    for (const sentence of text.split(/(?<=[.!?])\s+/)) {
      if (!sentence) continue;
      if (length && length + sentence.length + 1 > max) {
        return sentences.join(" ");
      }
      sentences.push(sentence);
      length += sentence.length + 1;
      if (length >= min) return sentences.join(" ");
    }
    if (length >= min) break;
  }
  const summary = sentences.join(" ");
  return summary.length >= 70 ? summary : null;
}

// ---------------------------------------------------------------------------
// The pages
// ---------------------------------------------------------------------------

const read = (path) => readFileSync(join(REPO, path), "utf8");
const readWebapp = (path) => readFileSync(join(WEBAPP, path), "utf8");

export function origin() {
  const host = readWebapp("CNAME").trim();
  if (!/^[a-z0-9.-]+\.[a-z]{2,}$/.test(host)) {
    throw new Error(`CNAME does not hold a hostname: ${JSON.stringify(host)}`);
  }
  return `https://${host}`;
}

/** The shared chrome, from the same partials `build-site.py` inlines.
 *
 *  Two transformations, both mechanical. The active primary-nav link is marked
 *  the way `build-site.py --check` marks it, because a reference page is reached
 *  through Docs and the nav has to say so. And every root-relative `./` in the
 *  partial is rebased to `../`, because these pages sit one directory down —
 *  which covers the nav hrefs, the brand link, and the footer script's prefetch
 *  of the engine. Anything else in the partial is inlined verbatim, so editing a
 *  partial reaches these pages in the same pass it reaches the four root pages,
 *  and `--check` fails until it does. */
function partial(name, { active } = {}) {
  let html = readWebapp(`_partials/${name}.html`).replace(/\n+$/, "");
  if (active) {
    const needle = `data-nav="${active}"`;
    if (!html.includes(needle)) {
      throw new Error(`build-doc-pages: no nav link ${needle} in partial '${name}'`);
    }
    html = html.replace(needle, `${needle} aria-current="page"`);
  }
  return html.replaceAll('"./', '"../');
}

const indent = (html, pad) =>
  html
    .split("\n")
    .map((line) => (line ? pad + line : line))
    .join("\n");

/** Everything a page needs to know about itself, all of it derived. */
export function describe(entry) {
  const text = read(entry.source);
  const list = blocks(text);
  const first = list[0];
  if (!first || first.kind !== "heading" || first.depth !== 1) {
    throw new Error(`${entry.source} does not open with a level-1 heading`);
  }
  // The document number is the repository's filing system, not a page title:
  // "98 — PDF Export and Print Design" becomes "PDF Export and Print Design".
  const heading = plain(first.text).replace(/^\d+\s*[—–-]\s*/, "");
  const body = list.slice(1);
  const summary = entry.summaryFrom
    ? summarise(section(body, entry.summaryFrom, entry.source))
    : openingIsProse(body)
      ? summarise(body)
      : null;
  if (!summary) {
    throw new Error(
      `${entry.source} has no prose paragraph to summarise, so it cannot be published as a ` +
        `page: a document with no self-contained summary has nothing for a search result or ` +
        `an assistant to quote. Add one to the document, or withhold the document.`,
    );
  }
  const page = {
    ...entry,
    sourcePath: entry.source,
    heading,
    summary,
    body,
    file: `${OUT_DIR}/${entry.slug}.html`,
  };
  checkHeadFields(page);
  return page;
}

/** Refuses a page whose head would fail the site's own discoverability guard.
 *
 *  `tests/seo.test.mjs` requires a 20-70 character title and a 70-340 character
 *  description of at least twelve words, both distinct across the site. Those are
 *  DERIVED here, from the document, so the failure mode is a document whose
 *  heading or opening paragraph does not make a search result — and the right
 *  place to find that out is at generation, naming the document, rather than in a
 *  test that names a URL.
 *
 *  The character ban is narrower than it looks and it is not cosmetic: a head
 *  string is published twice, once as an HTML attribute and once inside the
 *  JSON-LD graph, and `tests/seo.test.mjs` asserts the two agree. An `&` or a
 *  quote is escaped in the attribute and raw in the JSON, so the two copies would
 *  disagree by construction. Refusing the character keeps one published string
 *  with one spelling. */
function checkHeadFields(page) {
  const title = `${page.heading} — OpenDoc`;
  const refuse = (message) => {
    throw new Error(`${page.sourcePath}: ${message}`);
  };
  for (const [field, value] of [
    ["heading", page.heading],
    ["summary", page.summary],
  ]) {
    const offender = value.match(/[&<>"]/);
    if (offender) {
      refuse(
        `its ${field} contains ${JSON.stringify(offender[0])}, which is escaped in the ` +
          `page's meta tag and raw in its JSON-LD, so the two published copies of the ` +
          `same string would not match`,
      );
    }
  }
  if (title.length < 20 || title.length > 70) {
    refuse(`the page title "${title}" is ${title.length} characters; the site requires 20-70`);
  }
  if (page.summary.length < 70 || page.summary.length > 340) {
    refuse(`its derived summary is ${page.summary.length} characters; the site requires 70-340`);
  }
  if (page.summary.split(/\s+/).length < 12) {
    refuse("its derived summary is not a sentence (fewer than twelve words)");
  }
}

const HUB = {
  slug: "index",
  group: null,
  nav: "All reference docs",
  file: `${OUT_DIR}/index.html`,
};

const escapeAttribute = (value) => escapeHtml(value);

function railHtml(current) {
  const groups = new Map();
  for (const page of pages()) {
    if (!groups.has(page.group)) groups.set(page.group, []);
    groups.get(page.group).push(page);
  }
  const html = [
    '<aside class="doc-rail" aria-label="Reference documentation">',
    '  <div class="doc-rail-inner">',
    '    <p class="doc-rail-title">Reference</p>',
    '    <ul class="doc-rail-list">',
    `      <li><a${current === HUB.slug ? ' class="is-active"' : ""} href="./index.html">${
      HUB.nav
    }</a></li>`,
    "    </ul>",
  ];
  for (const [group, members] of groups) {
    html.push(`    <p class="doc-rail-title">${escapeHtml(group)}</p>`, '    <ul class="doc-rail-list">');
    for (const member of members) {
      html.push(
        `      <li><a${member.slug === current ? ' class="is-active"' : ""} href="./${
          member.slug
        }.html">${escapeHtml(member.nav)}</a></li>`,
      );
    }
    html.push("    </ul>");
  }
  html.push("  </div>", "</aside>");
  return html.join("\n");
}

function head({ title, description, canonical, headline, breadcrumb }) {
  const site = origin();
  const graph = {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "TechArticle",
        "@id": `${canonical}#webpage`,
        url: canonical,
        name: title,
        headline,
        description,
        inLanguage: "en",
        isPartOf: { "@id": `${site}/#website` },
        about: { "@id": `${site}/#software` },
        publisher: { "@id": `${site}/#organization` },
        license: "https://www.apache.org/licenses/LICENSE-2.0",
      },
      {
        "@type": "BreadcrumbList",
        "@id": `${canonical}#breadcrumb`,
        itemListElement: breadcrumb.map((step, index) => ({
          "@type": "ListItem",
          position: index + 1,
          name: step.name,
          item: step.item,
        })),
      },
    ],
  };
  return [
    "  <head>",
    '    <meta charset="utf-8" />',
    '    <meta name="viewport" content="width=device-width, initial-scale=1" />',
    `    <title>${escapeHtml(title)}</title>`,
    `    <meta name="description" content="${escapeAttribute(description)}" />`,
    '    <meta name="author" content="CasualOffice" />',
    '    <meta name="robots" content="index,follow,max-image-preview:large,max-snippet:-1" />',
    '    <meta name="theme-color" content="#fbfaf8" />',
    '    <link rel="icon" href="../opendoc-mark.svg" type="image/svg+xml" />',
    '    <link rel="apple-touch-icon" href="../apple-touch-icon.png" />',
    `    <link rel="canonical" href="${canonical}" />`,
    '    <link rel="stylesheet" href="../src/fonts.css" />',
    "",
    '    <meta property="og:type" content="article" />',
    '    <meta property="og:site_name" content="OpenDoc" />',
    `    <meta property="og:title" content="${escapeAttribute(headline)}" />`,
    `    <meta property="og:description" content="${escapeAttribute(description)}" />`,
    `    <meta property="og:url" content="${canonical}" />`,
    `    <meta property="og:image" content="${site}/assets/editor.jpg" />`,
    '    <meta property="og:image:alt" content="The OpenDoc browser editor rendering a DOCX document" />',
    "",
    '    <meta name="twitter:card" content="summary_large_image" />',
    `    <meta name="twitter:title" content="${escapeAttribute(headline)}" />`,
    `    <meta name="twitter:description" content="${escapeAttribute(description)}" />`,
    `    <meta name="twitter:image" content="${site}/assets/editor.jpg" />`,
    "",
    '    <link rel="stylesheet" href="../src/marketing.css" />',
    '    <script type="application/ld+json">',
    indent(JSON.stringify(graph, null, 2), "      "),
    "    </script>",
    "  </head>",
  ].join("\n");
}

function pageDocument({ head: headHtml, main }) {
  return [
    "<!doctype html>",
    '<html lang="en">',
    headHtml,
    "  <body>",
    indent(partial("site-header", { active: "docs" }), "    "),
    "",
    indent(main, "    "),
    "",
    indent(partial("site-footer"), "    "),
    "  </body>",
    "</html>",
    "",
  ].join("\n");
}

/** The provenance line every reference page carries.
 *
 *  The one piece of prose on these pages that is not in a `docs/` file, and it is
 *  a CONSTANT rather than per-page text for exactly that reason: there is one
 *  sentence to review, it states a checkable fact (where the page comes from), and
 *  it contains no number, grade or adjective standing in for one.
 *  `tests/doc_pages.test.mjs` asserts it carries no digit, so no claim can be
 *  smuggled into it. */
export const PROVENANCE = "Generated from this repository's own design document:";

function articleHead(page, sourceUrl) {
  return [
    '<p class="doc-eyebrow">Reference</p>',
    `<h1 id="${page.slug === HUB.slug ? "reference" : slugify(page.heading)}">${escapeHtml(
      page.heading,
    )}</h1>`,
    `<p class="doc-lede">${escapeHtml(page.summary)}</p>`,
    '<p class="doc-provenance">',
    `  ${PROVENANCE}`,
    `  <a href="${escapeAttribute(sourceUrl)}"><code>${escapeHtml(page.sourcePath)}</code></a>`,
    "</p>",
  ].join("\n");
}

export function renderPage(page) {
  const site = origin();
  const canonical = `${site}/${page.file}`;
  const headings = [];
  const body = renderBlocks(page.body, { published: publishedByPath() }, headings);
  const toc = [
    '<aside class="doc-toc" aria-label="On this page">',
    '  <div class="doc-toc-inner">',
    '    <p class="doc-toc-title">On this page</p>',
    '    <ul class="doc-toc-list">',
    ...headings.map((entry) => `      <li><a href="#${entry.id}">${escapeHtml(entry.text)}</a></li>`),
    "    </ul>",
    "  </div>",
    "</aside>",
  ].join("\n");
  // The ARTICLE comes first in the source, and the rail is ordered back into the
  // left column by CSS where there is a column for it. On a phone `.doc-reader`
  // collapses to one column, and a rail-first page would open with fifteen
  // navigation links and push the document below two screenfuls of them. The
  // reader came for the document.
  const main = [
    '<main class="doc-reader is-reference">',
    '  <article class="doc-article">',
    indent(articleHead(page, `${BLOB}/${page.sourcePath}`), "    "),
    indent(`<div class="doc-prose">\n${body}\n</div>`, "    "),
    "  </article>",
    "",
    indent(railHtml(page.slug), "  "),
    "",
    headings.length ? indent(toc, "  ") : "",
    "</main>",
  ]
    .filter((part) => part !== "")
    .join("\n");
  return pageDocument({
    head: head({
      title: `${page.heading} — OpenDoc`,
      description: page.summary,
      canonical,
      headline: page.heading,
      breadcrumb: [
        { name: "OpenDoc", item: `${site}/` },
        { name: "Documentation", item: `${site}/docs.html` },
        { name: "Reference", item: `${site}/${OUT_DIR}/index.html` },
        { name: page.heading, item: canonical },
      ],
    }),
    main,
  });
}

/** The hub. Generated from the same manifest as the pages it lists, so it can
 *  never be missing one — which is the failure a hand-maintained index has. */
export function renderHub() {
  const site = origin();
  const canonical = `${site}/${HUB.file}`;
  const groups = new Map();
  for (const page of pages()) {
    if (!groups.has(page.group)) groups.set(page.group, []);
    groups.get(page.group).push(page);
  }
  const cards = [];
  for (const [group, members] of groups) {
    cards.push(`<h2 class="doc-h2" id="${slugify(group)}">${escapeHtml(group)}</h2>`);
    cards.push('<ul class="doc-index-list">');
    for (const member of members) {
      cards.push(
        "  <li>",
        `    <a class="doc-index-title" href="./${member.slug}.html">${escapeHtml(member.heading)}</a>`,
        `    <p class="doc-index-summary">${escapeHtml(member.summary)}</p>`,
        "  </li>",
      );
    }
    cards.push("</ul>");
  }
  // Not "OpenDoc reference documentation": the page title appends " — OpenDoc",
  // and a search result reading "OpenDoc reference documentation — OpenDoc" says
  // the name twice and the subject once.
  const heading = "Reference documentation";
  const summary =
    "Every OpenDoc architecture, design and specification document published on this " +
    "site, generated from the repository's own Markdown so a page and its source can " +
    "never disagree. Documents deliberately not published here are named, with the " +
    "reason, in webapp/tools/build-doc-pages.mjs.";
  const main = [
    '<main class="doc-reader is-reference is-index">',
    '  <article class="doc-article">',
    '    <p class="doc-eyebrow">Reference</p>',
    `    <h1 id="reference">${escapeHtml(heading)}</h1>`,
    `    <p class="doc-lede">${escapeHtml(summary)}</p>`,
    indent(`<div class="doc-prose">\n${cards.join("\n")}\n</div>`, "    "),
    "  </article>",
    "",
    indent(railHtml(HUB.slug), "  "),
    "</main>",
  ].join("\n");
  return pageDocument({
    head: head({
      title: `${heading} — OpenDoc`,
      description: summary,
      canonical,
      headline: heading,
      breadcrumb: [
        { name: "OpenDoc", item: `${site}/` },
        { name: "Documentation", item: `${site}/docs.html` },
        { name: "Reference", item: canonical },
      ],
    }),
    main,
  });
}

let described = null;
/** Every published page, described. Memoised: `renderPage` asks for the set to
 *  build its rail, and re-reading eleven documents per page is pointless. */
export function pages() {
  if (!described) {
    described = PUBLISHED.map((entry) => describe(entry));
  }
  return described;
}

let byPath = null;
function publishedByPath() {
  if (!byPath) byPath = new Map(PUBLISHED.map((entry) => [entry.source, entry.slug]));
  return byPath;
}

/** `[file, html]` for every page this generator owns, hub included. */
export function artifacts() {
  return [
    ...pages().map((page) => [page.file, renderPage(page)]),
    [HUB.file, renderHub()],
  ].sort((a, b) => (a[0] < b[0] ? -1 : 1));
}

function build(check) {
  const built = artifacts();
  const drift = [];
  if (!check) mkdirSync(join(WEBAPP, OUT_DIR), { recursive: true });
  for (const [file, html] of built) {
    const path = join(WEBAPP, file);
    if (check) {
      let current = null;
      try {
        current = readFileSync(path, "utf8");
      } catch {
        current = null;
      }
      if (current !== html) drift.push(file);
    } else {
      writeFileSync(path, html, "utf8");
      console.log(`build-doc-pages: wrote ${file}`);
    }
  }
  if (check) {
    if (drift.length) {
      console.error(
        `build-doc-pages --check: ${drift.join(", ")} ${drift.length > 1 ? "are" : "is"} stale.\n` +
          "A published design document changed, or a generated page was edited by hand. " +
          "These pages are the documents — nothing on them is written here — so the fix is " +
          "always to regenerate.\nRun ./tools/build-doc-pages.mjs and commit the result.",
      );
      return 1;
    }
    console.log(`build-doc-pages --check: ${built.length} reference page(s) up to date.`);
  }
  return 0;
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exit(build(process.argv.includes("--check")));
}
