// The trackers tell the reader four separate times to "re-derive these counts,
// do not edit them by hand" — and nothing checked that anyone did. Both
// documents drifted anyway: `104`'s P3 cell claimed 14 rows still open against
// 13 actually open, so the Still-open column no longer summed to its own Total,
// and `105` carried two §3.2 rows with a missing cell, which silently shifted
// their Pri, Eff and Status one column to the left.
//
// This is docs/105 CQ-007. A tracker whose summary is hand-maintained is a
// tracker that will eventually lie about how much work is left, which is the
// one thing it exists to be trusted about. So: parse the rows, recompute every
// summary cell, and fail on any disagreement.
//
// Buildless on purpose — it runs in the existing `npm run test:unit` lane
// (`node --test tests/*.test.mjs`), which CI already invokes.
// The parser lives in `tracker_parse.mjs` and is shared with
// `tracker_single_queue.test.mjs`. Two copies of a row filter is how two guards
// come to disagree about what a row IS — see that module's opening comment.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  LANES,
  PRIORITIES,
  cells,
  isOpen,
  queueRows,
  read,
  rows,
} from "./tracker_parse.mjs";


test("docs/104: every summary cell is what the rows actually say", () => {
  const text = read("104-HOTFIX-TRACKER.md");
  const all = rows(text, /^HF-\d+$/);

  // Ids are the spine of the table: a duplicate double-counts and a gap means a
  // row was dropped without anyone noticing.
  const ids = all.map((r) => Number(r.id.slice(3)));
  assert.equal(new Set(ids).size, ids.length, "duplicate HF ids");
  const sorted = [...ids].sort((a, b) => a - b);
  assert.deepEqual(
    sorted,
    Array.from({ length: sorted.length }, (_, i) => i + 1),
    "HF ids must run 1..N with no gaps",
  );

  // The summary table's own Total must equal the sum of its own section rows —
  // the invariant that broke, and that no amount of careful editing preserves.
  const summary = all.length;
  const open = all.filter((r) => isOpen(r.status)).length;

  const totalRow = text
    .split("\n")
    .find((l) => l.startsWith("| **Total** |"));
  assert.ok(totalRow, "docs/104 must carry a Total row");
  const [, totalRows, totalOpen] = cells(totalRow).map((c) => c.replaceAll("*", "").trim());
  assert.equal(
    Number(totalRows),
    summary,
    `Total rows says ${totalRows}, the table holds ${summary}`,
  );
  assert.equal(
    Number(totalOpen),
    open,
    `Total still-open says ${totalOpen}, the rows say ${open}`,
  );

  // And each section cell must sum to that Total.
  // Every row of the summary table between its header and its Total — named by
  // whatever sections the document actually has, so appending a new audit
  // section without adding it here fails instead of quietly vanishing from the
  // total. That is precisely how 7 rows went missing.
  const summaryLines = text.split("\n");
  const headerAt = summaryLines.findIndex((l) => l.startsWith("| Section | Rows |"));
  assert.ok(headerAt > 0, "docs/104 must carry a Section/Rows summary table");
  const sectionOpen = [];
  const sectionRows = [];
  const stated = [];
  for (let i = headerAt + 2; i < summaryLines.length; i++) {
    const line = summaryLines[i];
    if (!line.startsWith("|")) break;
    if (line.startsWith("| **Total**")) break;
    const c = cells(line);
    sectionRows.push(Number(c[1]));
    sectionOpen.push(Number(c[2]));
    stated.push({ label: c[0], rows: Number(c[1]), open: Number(c[2]) });
  }
  assert.ok(
    sectionOpen.every((n) => Number.isFinite(n)),
    "every summary section must carry a numeric Still-open cell",
  );

  assert.equal(
    sectionRows.reduce((a, b) => a + b, 0),
    summary,
    `the per-section Rows column sums to ${sectionRows.reduce((a, b) => a + b, 0)}, ` +
      `but the document holds ${summary} rows — a section is missing from the summary`,
  );
  assert.equal(
    sectionOpen.reduce((a, b) => a + b, 0),
    open,
    `the per-section Still-open column sums to ${sectionOpen.reduce((a, b) => a + b, 0)}, ` +
      `but the rows say ${open} — this is exactly the P3-cell drift`,
  );

  // Per-section, not only the column total. The two assertions ABOVE only check
  // that the Rows and Still-open columns SUM to the Total — which two
  // equal-and-opposite errors sail straight through. That is not hypothetical:
  // on 2026-09-20 the P2 cell read 21 against 23 rows actually open while the
  // behavioural-audit cell read 6 against 4, the column still summed to the
  // stated Total of 50, and the guard stayed green through both. They are
  // deliberately left above this block so that the failure of a cancelling pair
  // is charged to THIS assertion and not to theirs. So: charge every row to the
  // heading it sits under and compare the cells one at a time.
  const bySection = new Map();
  for (const r of all) {
    if (!bySection.has(r.heading)) bySection.set(r.heading, []);
    bySection.get(r.heading).push(r);
  }
  // A summary label names its section either verbatim ("Behavioural audit —
  // 2026-09-04") or as the stem of a counted heading ("P2" -> "P2 — 52 items").
  const matchesLabel = (heading, label) => heading === label || heading.startsWith(`${label} —`);
  const resolved = stated.map((s) => ({
    ...s,
    hits: [...bySection.keys()].filter((h) => matchesLabel(h, s.label)),
  }));

  // Checked BEFORE the per-cell comparisons, and it has to be: a section that
  // splits in two (a heading inserted mid-table) leaves the rows below it
  // charged to a heading no summary row names, and the first thing the reader
  // would otherwise see is a count mismatch on the section that shrank, which
  // is the symptom rather than the cause.
  const claimed = new Set(resolved.flatMap((s) => s.hits));
  const unclaimed = [...bySection.keys()].filter((h) => !claimed.has(h));
  assert.deepEqual(
    unclaimed,
    [],
    "these sections of docs/104 hold HF rows but no summary row counts them, so their " +
      `open work is invisible in the summary: ${unclaimed.join(" / ")}`,
  );

  for (const s of resolved) {
    const hits = s.hits;
    assert.equal(
      hits.length,
      1,
      `docs/104's summary row "${s.label}" matches ${hits.length} sections holding HF rows ` +
        `(${hits.join(" / ") || "none"}) — a summary cell that names no section counts nothing`,
    );
    const rs = bySection.get(hits[0]);
    const openHere = rs.filter((r) => isOpen(r.status));
    assert.equal(
      s.rows,
      rs.length,
      `docs/104 summary "${s.label}" Rows says ${s.rows}, section "${hits[0]}" holds ${rs.length}`,
    );
    assert.equal(
      s.open,
      openHere.length,
      `docs/104 summary "${s.label}" Still-open says ${s.open}, but section "${hits[0]}" has ` +
        `${openHere.length} open: ${openHere.map((r) => r.id).join(", ") || "none"}`,
    );
    // The `— N items` in the heading itself is a hand-maintained number too,
    // and it is the one a reader sees before the table.
    const inHeading = hits[0].match(/— (\d+) items$/);
    if (inHeading) {
      assert.equal(
        Number(inHeading[1]),
        rs.length,
        `docs/104 heading "${hits[0]}" claims ${inHeading[1]} items, the section holds ${rs.length}`,
      );
    }
  }

  const prose = text.match(/\*\*(\d+) of (\d+) rows remain open/);
  assert.ok(prose, "the progress line must state 'N of M rows remain open'");
  assert.equal(Number(prose[1]), open, "progress line's open count");
  assert.equal(Number(prose[2]), summary, "progress line's total");
});

test("docs/105: every class count is derived, and no row is malformed", () => {
  const text = read("105-AUDIT-2026-09-TRACKER.md");
  const classes = {
    EV: /^EV-\d+$/,
    UX: /^UX-\d+$/,
    CQ: /^CQ-\d+$/,
    FID: /^FID-[PLR]-\d+$/,
    OO: /^OO-\d+$/,
  };

  const counts = {};
  for (const [name, pattern] of Object.entries(classes)) {
    const rs = rows(text, pattern);
    assert.ok(rs.length > 0, `${name} rows must parse`);
    counts[name] = { total: rs.length, open: rs.filter((r) => isOpen(r.status)).length };

    // Uniform width within a section. A row missing a cell shifts every later
    // column left, so its Status is read out of the Evidence position and the
    // derived counts silently go wrong while the table still renders.
    const widths = new Map();
    for (const r of rs) widths.set(r.cells.length, (widths.get(r.cells.length) ?? 0) + 1);
    if (name === "FID") {
      // FID spans three sub-tables with different shapes; group by sub-class.
      for (const sub of ["FID-P", "FID-L", "FID-R"]) {
        const w = new Set(rs.filter((r) => r.id.startsWith(sub)).map((r) => r.cells.length));
        assert.equal(w.size, 1, `${sub} rows disagree on column count: ${[...w].join(", ")}`);
      }
    } else {
      assert.equal(
        widths.size,
        1,
        `${name} rows disagree on column count: ${[...widths.keys()].join(", ")}`,
      );
    }
  }

  // Every class must carry a real Status cell. OO used to have none and its
  // "21 open" rested on a sentence of prose, which would go false the moment one
  // OO row closed.
  for (const [name, c] of Object.entries(counts)) {
    assert.ok(
      c.open <= c.total,
      `${name} cannot have more open rows (${c.open}) than rows (${c.total})`,
    );
  }

  const summaryRows = text
    .split("\n")
    .filter((l) => /^\| (EV|UX|CQ|FID|OO) [—-]/.test(l));
  assert.equal(summaryRows.length, 5, "the summary table must carry all five classes");
  for (const line of summaryRows) {
    const c = cells(line);
    const name = c[0].split(" ")[0];
    assert.equal(
      Number(c[1]),
      counts[name].total,
      `${name} row count: summary says ${c[1]}, rows say ${counts[name].total}`,
    );
    assert.equal(
      Number(c[2]),
      counts[name].open,
      `${name} open count: summary says ${c[2]}, rows say ${counts[name].open}`,
    );
  }

  const totalLine = text.split("\n").find((l) => l.startsWith("| **Total**"));
  assert.ok(totalLine, "docs/105 must carry a Total row");
  const t = cells(totalLine).map((x) => x.replaceAll("*", "").trim());
  const allRows = Object.values(counts).reduce((a, c) => a + c.total, 0);
  const allOpen = Object.values(counts).reduce((a, c) => a + c.open, 0);
  assert.equal(Number(t[1]), allRows, `Total rows: says ${t[1]}, derived ${allRows}`);
  assert.equal(Number(t[2]), allOpen, `Total open: says ${t[2]}, derived ${allOpen}`);
});

// ---------------------------------------------------------------------------
// docs/109 — the one queue, arithmetic only.
//
// The owner's decision on 2026-09-20 was that there is exactly ONE tracker to
// work from. 104, 105 and 106 became archives and 109 holds the order; 14 and 99
// joined the scheme on 2026-10-04. The STRUCTURE of that arrangement — the
// coverage of every archive, the duplicate-id check, the 1..N enumeration and
// the lane and priority order — now lives in `tracker_single_queue.test.mjs`,
// because it is one set of invariants about one decision and splitting it across
// two files is how a guard ends up half-armed. What remains here is what this
// file was written for: every published COUNT re-derived from the rows that
// justify it, in 104, 105 and 109 alike.

test("docs/109: every summary cell is what the rows actually say", () => {
  const text = read("109-BACKLOG.md");
  const queue = queueRows(text);

  const tally = (rs) => ({
    Rows: rs.length,
    P0: rs.filter((r) => r.priority === "P0").length,
    P1: rs.filter((r) => r.priority === "P1").length,
    P2: rs.filter((r) => r.priority === "P2").length,
    P3: rs.filter((r) => r.priority === "P3").length,
    Unprioritised: rs.filter((r) => !PRIORITIES.includes(r.priority)).length,
  });

  const lines = text.split("\n");
  const headerAt = lines.findIndex((l) => l.startsWith("| Lane | Rows |"));
  assert.ok(headerAt > 0, "docs/109 must carry a Lane/Rows summary table");
  const columns = cells(lines[headerAt]).slice(1);
  assert.deepEqual(
    columns,
    ["Rows", "P0", "P1", "P2", "P3", "Unprioritised"],
    "docs/109's summary columns changed — derive the new ones here too",
  );

  const stated = new Map();
  const wrongWidth = [];
  for (const line of lines.slice(headerAt + 2)) {
    if (!line.startsWith("|")) break;
    const c = cells(line).map((x) => x.replaceAll("*", "").trim());
    // A row must be exactly as wide as the header it sits under.
    //
    // This is here because the Total row carried TWENTY-ONE value cells against
    // a six-column header: the six real ones followed by fifteen numbers left
    // over from earlier edits. Every assertion below reads by column INDEX, so
    // the extras were unreachable and the table published stale figures while
    // this file stayed green — drift, in the summary of the row about drift.
    // Checking the shape is what makes the value checks trustworthy, because a
    // guard that indexes into a row it never measured can only ever see the
    // prefix somebody happened to keep correct.
    if (c.length - 1 !== columns.length) {
      wrongWidth.push(
        `${c[0] || "(unnamed)"} has ${c.length - 1} value cells, the header declares ` +
          `${columns.length}: ${c.slice(1).join(" | ")}`,
      );
    }
    stated.set(c[0], c.slice(1).map(Number));
  }
  assert.deepEqual(
    wrongWidth,
    [],
    "docs/109's summary rows must be as wide as its header — extra cells are never read, " +
      "so they are stale numbers nothing can correct",
  );

  for (const name of LANES) {
    const derived = tally(queue.filter((r) => r.lane === name));
    const row = stated.get(name);
    assert.ok(row, `docs/109's summary must carry a ${name} row`);
    columns.forEach((colName, i) => {
      assert.equal(
        row[i],
        derived[colName],
        `docs/109 summary ${name}/${colName}: says ${row[i]}, the rows say ${derived[colName]}`,
      );
    });
  }

  const total = tally(queue);
  const totalRow = stated.get("Total");
  assert.ok(totalRow, "docs/109's summary must carry a Total row");
  columns.forEach((colName, i) => {
    assert.equal(
      totalRow[i],
      total[colName],
      `docs/109 summary Total/${colName}: says ${totalRow[i]}, the rows say ${total[colName]}`,
    );
  });

  // The prose headline is a published number too, and it is the one a reader
  // quotes without opening the table.
  const prose = text.match(
    /\*\*(\d+) rows in the one queue: (\d+) Hotfix, (\d+) Audit, (\d+) Roadmap\.\*\*/,
  );
  assert.ok(
    prose,
    "docs/109 must state '**N rows in the one queue: A Hotfix, B Audit, C Roadmap.**'",
  );
  assert.equal(Number(prose[1]), total.Rows, "docs/109 headline total");
  LANES.forEach((name, i) => {
    assert.equal(
      Number(prose[i + 2]),
      queue.filter((r) => r.lane === name).length,
      `docs/109 headline ${name} count`,
    );
  });
});
