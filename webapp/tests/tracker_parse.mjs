// Reading the trackers, once.
//
// Two guards parse these documents — `tracker_counts.test.mjs` re-derives every
// summary cell, and `tracker_single_queue.test.mjs` enforces that `109` is the
// only queue. They were one file until the second was written, and the obvious
// move was to copy the parser. That is how two guards come to disagree about
// what a row IS: the duplicate-id check in `tracker_counts` was blind to
// seventeen rows for a fortnight because its row filter and the table's actual
// contents had drifted apart, and a second copy of that filter would have been a
// second place for the same drift to hide. So the parser lives here and both
// guards read the documents the same way, by construction.
//
// NOT a `*.test.mjs` file on purpose: `npm run test:unit` is
// `node --test tests/*.test.mjs`, so this module is imported and never run as a
// suite of its own.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

/** A document under `docs/`, as text. */
export const read = (name) => readFileSync(join(repoRoot, "docs", name), "utf8");

// A table cell may legally contain a pipe inside a `code span`. Splitting on
// every pipe miscounts exactly those rows, which is how a malformed row hides:
// it looks wide enough while its cells are off by one.
// A sentinel that cannot occur in Markdown. Written as an escape, not as a
// literal NUL byte: a raw NUL makes git classify this file as binary and
// silently stop showing its diffs.
const SENTINEL = "\u0000";

/** The cells of one Markdown table row, pipes inside code spans preserved. */
export function cells(row) {
  const masked = row.replace(/`[^`]*`/g, (m) => m.replaceAll("|", SENTINEL));
  return masked
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((c) => c.replaceAll(SENTINEL, "|").trim());
}

// "Still open" is a PREFIX test, not equality: real statuses qualify themselves
// ("Open (owner decision)", "Partly fixed (the declaration is now read…)").
// Anything else — Fixed, Re-opened, Closed — is not open.
const OPEN_PREFIXES = ["Open", "Partly fixed", "Partly", "In progress", "Re-opened"];
// A status may be emphasised (`**Partly fixed** (#541) — …`), so strip markdown
// emphasis before testing the prefix; otherwise a bolded status reads as closed.
export const normalise = (status) => status.replace(/^[*_\s]+/, "");
export const isOpen = (status) => OPEN_PREFIXES.some((p) => normalise(status).startsWith(p));

/**
 * Every `| ID | … |` row in a document, with its id, last cell, and the `##`/`###`
 * heading it sits under. The heading is what lets a summary be checked cell by
 * cell instead of only in total.
 */
export function rows(text, idPattern) {
  const out = [];
  let heading = "";
  for (const line of text.split("\n")) {
    const h = line.match(/^#{2,3} +(.*?)\s*$/);
    if (h) {
      heading = h[1];
      continue;
    }
    if (!line.startsWith("|")) continue;
    const c = cells(line);
    if (!c.length || !idPattern.test(c[0])) continue;
    out.push({ id: c[0], status: c[c.length - 1], cells: c, line, heading });
  }
  return out;
}

/** The lanes of `109`, in the order the owner set on 2026-09-20. */
export const LANES = ["Hotfix", "Audit", "Roadmap"];
/** The priority scale, worst first. */
export const PRIORITIES = ["P0", "P1", "P2", "P3"];

/** The queue rows of docs/109: the ones whose first cell is a position number. */
export function queueRows(text) {
  const lines = text.split("\n");
  const headerAt = lines.findIndex((l) => l.startsWith("| # | Id | Lane |"));
  assert.ok(headerAt > 0, "docs/109 must carry a queue table headed '| # | Id | Lane |'");
  const header = cells(lines[headerAt]);
  const col = (name) => {
    const i = header.findIndex((h) => h.toLowerCase().startsWith(name));
    assert.ok(i >= 0, `docs/109's queue table must carry a ${name} column`);
    return i;
  };
  const idx = {
    n: col("#"),
    id: col("id"),
    lane: col("lane"),
    priority: col("priority"),
    effort: col("effort"),
    status: col("status"),
    source: col("source"),
    supersedes: col("supersedes"),
  };
  const out = [];
  for (const line of lines.slice(headerAt + 1)) {
    // The queue table ends where the next section begins. This bound matters now
    // that a row is recognised by its Id rather than by a numeric first cell:
    // `Dropped, and on whose authority` and `What was merged` are also id-bearing
    // tables, and without the bound their rows would be read as queue rows.
    if (line.startsWith("## ")) break;
    if (!line.startsWith("|")) continue;
    const c = cells(line);
    // A queue row is one whose first cell is position-SHAPED — digits, optionally
    // with a letter suffix. It used to require a plain integer, and that
    // `continue` was a hole rather than a filter: rows had been inserted with a
    // suffixed position (`1b`, `2e`, `64a`) instead of renumbering, so every
    // assertion below skipped them — the 1..N enumeration, the duplicate-id
    // check, the lane order, the priority order and the derived counts alike.
    //
    // What that cost, measured rather than supposed: seventeen rows were
    // invisible, so the summary said 129 rows against an actual 142 (`docs/99`
    // §9 rule 6 — understating is also false); a P2 row sat inside the P1 block
    // and the ordering assertion could not see it; and two DIFFERENT defects were
    // both numbered HF-179, which the duplicate-id check would have caught except
    // that one of them was a skipped row.
    //
    // Accepting the suffix here is what makes it FAIL, loudly, in the enumeration
    // assertion instead of vanishing. The id is deliberately NOT the test: ids in
    // this queue take four shapes (`HF-011`, `FID-L-07b`, `RM-01`, `Q3`), and a
    // filter written to match ids silently dropped six rows the first time this
    // was attempted — including the five owner-decision rows.
    if (!/^\d+[a-z]*$/.test(c[0])) continue;
    assert.equal(
      c.length,
      header.length,
      `docs/109 row ${c[0]} has ${c.length} cells against the header's ${header.length} — ` +
        "a missing cell shifts every later column left and the derived counts go wrong",
    );
    out.push({
      // Kept verbatim as well as coerced: `Number("2e")` is NaN, and a position
      // that silently becomes NaN is how a malformed row hides.
      nRaw: c[idx.n],
      n: Number(c[idx.n]),
      id: c[idx.id],
      lane: c[idx.lane],
      priority: c[idx.priority],
      effort: c[idx.effort],
      status: c[idx.status],
      source: c[idx.source],
      supersedes: c[idx.supersedes],
    });
  }
  assert.ok(out.length > 0, "docs/109 must carry queue rows");
  return out;
}

/**
 * The first cells of the Markdown table whose header line starts with `prefix`,
 * with markdown emphasis stripped — how a hand-authored coverage table is read
 * back and compared against the archive it claims to cover.
 */
export function tableColumn(text, headerPrefix, column = 0) {
  const lines = text.split("\n");
  const headerAt = lines.findIndex((l) => l.startsWith(headerPrefix));
  assert.ok(headerAt > 0, `expected a table headed '${headerPrefix}'`);
  const out = [];
  for (const line of lines.slice(headerAt + 2)) {
    if (!line.startsWith("|")) break;
    const c = cells(line);
    out.push(c.map((x) => x.replaceAll("*", "").replaceAll("`", "").trim())[column]);
  }
  assert.ok(out.length > 0, `the table headed '${headerPrefix}' has no rows`);
  return out;
}

/**
 * Every row of `14-EXECUTION-TRACKER.md` that sits under a header carrying a
 * `Status` column, as `{ id, status, title }`.
 *
 * The recipe is stated because `14`'s counts are published — in its own banner
 * and in `109` CQ-011 — and `SKILL.md` §8 requires a published number to be
 * derived from a committed artifact. Rows under the Completed-work index, whose
 * header carries a `Completed` date and no status, are returned with a status of
 * `""` so that they can be counted as the closed class they are rather than
 * silently dropped.
 */
export function executionRows(text) {
  const out = [];
  let heading = "";
  let header = null;
  for (const line of text.split("\n")) {
    const h = line.match(/^#{2,4} +(.*?)\s*$/);
    if (h) {
      heading = h[1];
      header = null;
      continue;
    }
    if (!line.startsWith("|")) {
      header = null;
      continue;
    }
    const c = cells(line);
    if (/^(ID|Id|#)$/.test(c[0])) {
      header = c;
      continue;
    }
    if (/^[-: ]+$/.test(c[0])) continue;
    if (!header) continue;
    const si = header.findIndex((x) => /^status$/i.test(x));
    const ti = header.findIndex((x) => /^(title|workstream)$/i.test(x));
    out.push({
      heading,
      id: c[0],
      title: ti >= 0 ? c[ti] : "",
      status: si >= 0 ? c[si] : "",
    });
  }
  return out;
}

/**
 * `14`'s forward-work statuses, longest-distinguishing-prefix first.
 *
 * `In review` is deliberately NOT here: it is a sign-off state on a slice that
 * shipped, not remaining work, and treating 67 of them as open work would bury
 * the 48 rows that are. It is covered as a counted class instead — see `109`'s
 * Archive coverage section.
 */
export const FORWARD_STATUSES = [
  "In progress",
  "Not started",
  "Designing",
  "Pending",
  "Accepted",
  "Planned",
  "Designed",
  "Open",
  "Experimental design",
];

/** Which forward-work class a `14` status belongs to, or `null`. */
export const forwardClass = (status) =>
  FORWARD_STATUSES.find((p) => status.startsWith(p)) ?? null;
