// `109-BACKLOG.md` is the only queue, and that is enforced here rather than
// promised there.
//
// The owner has asked for one tracker — everything pulled into it, organised,
// and not pivoted away from — repeatedly, and it was repeatedly not delivered.
// The failure was never a disagreement about the goal; it was that nothing
// checked. `109` declared itself the single working queue on 2026-09-20 and
// declared `104`/`105`/`106` closed to new rows, and on 2026-10-04 `14` and `99`
// were still outside the scheme altogether, `14` holding 48 forward-work rows
// for which no queue position had ever been derived. A document that says it
// holds everything, and is not checked, drifts to holding whatever its last
// editor remembered.
//
// The failure mode the one-queue decision creates is specific and quiet: a row
// that is still open in an archive silently stops being worked, because nobody
// reads the archive any more. So the load-bearing assertion in this file is
// COVERAGE — every row still open in `104`, `105`, `106`, `99` or `14` must be
// reachable from `109`. The three structural invariants beside it are the ones a
// hand-edited table gets wrong: a duplicated id (the work is queued twice, or one
// copy was silently overwritten), a `#` column that no longer enumerates the
// queue, and a row inserted in the wrong place — which matters here because the
// ordering IS this document's product.
//
// WHAT THIS DOES NOT CHECK, said plainly because a guard cited for more than it
// checks is the recurring defect in this repository:
//
//   * The effort tie-break. The `Effort` scale is not comparable across lanes —
//     `104` grades S as under an hour, `105` as under a day — so a monotonicity
//     assertion over the column would be comparing two scales.
//   * Blocked-by. It is prose in the Notes cell, not a column. Parsing English
//     for a dependency graph would make the guard's verdict less trustworthy than
//     the table it is guarding.
//   Both are stated in `109`'s own sort rule as human keys, so nothing here is
//   being quoted as enforced when it is not (`docs/99` §9 rule 2).
//
//   * Whether a row's status is TRUE. Nothing mechanical can read code and decide
//     that "Open" is still right. That is what `109`'s re-verification section is
//     for, and `109` CQ-011 is the row for `14`'s 115 unverified statuses.
//
// Buildless on purpose — it runs in the existing `npm run test:unit` lane
// (`node --test tests/*.test.mjs`), which CI already invokes as the `test` job.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  FORWARD_STATUSES,
  LANES,
  PRIORITIES,
  cells,
  executionRows,
  forwardClass,
  isOpen,
  queueRows,
  read,
  rows,
  tableColumn,
} from "./tracker_parse.mjs";

const BACKLOG = "109-BACKLOG.md";

// ---------------------------------------------------------------------------
// Invariant 1 — an id appears at most once.
//
// Not hypothetical twice over. Two unrelated defects were both numbered HF-179
// and nothing noticed for days; then PR #747 minted OO-020, OO-021, UX-020 and
// UX-021 for four new findings while `105` already owned all four ids for
// unrelated work — and UX-021 had already CLOSED there, so the queue was
// carrying a row under an id its own source records as fixed.
test("docs/109: no id is queued twice", () => {
  const queue = queueRows(read(BACKLOG));
  const seen = new Map();
  for (const r of queue) {
    assert.ok(
      !seen.has(r.id),
      `docs/109 lists ${r.id} twice (rows ${seen.get(r.id)} and ${r.n}) — the same work is ` +
        "queued twice, or one row was overwritten by a copy-paste and has silently left " +
        "the queue. The Id is the identity: if two rows are different work, one of them " +
        "needs the next free number, and the renumber goes in its Notes cell",
    );
    seen.set(r.id, r.n);
  }
});

// An id must be unique across the WHOLE document, not just the open queue.
//
// This exists because HF-263 was minted twice in one day for two unrelated
// pieces of work — a deferred Pages action bump, which landed in the queue, and
// a print hang, which was worked and filed under "Closed since the queue was
// opened". Both guards above passed: the uniqueness check reads `queueRows`, so
// a closed row reusing a queued id is invisible to it, and so is a closed row
// duplicated within the closed table.
//
// That is not cosmetic. The id is how a commit, a code comment and a doc cite a
// row; two rows sharing one means a citation resolves to whichever the reader
// finds first, and the other piece of work has no name at all. Two parallel
// lanes each taking "the next free number" from the queue is exactly how it
// happens, and it will happen again.
test("docs/109: no id is minted twice anywhere in the document", () => {
  const text = read(BACKLOG);
  const queued = new Map(queueRows(text).map((r) => [r.id, `queue row ${r.n}`]));

  // The closed table's rows start with the Id directly, having no `#`.
  const closedAt = text.indexOf("## Closed since the queue was opened");
  assert.ok(closedAt > 0, "docs/109 must carry a 'Closed since the queue was opened' section");
  const closed = new Map();
  const clashes = [];
  for (const line of text.slice(closedAt).split("\n")) {
    const row = line.match(/^\|\s*([A-Z]+[A-Z0-9-]*-[A-Z0-9]+)\s*\|/);
    if (!row) continue;
    const id = row[1];
    if (closed.has(id)) clashes.push(`${id}: listed twice among the closed rows`);
    else if (queued.has(id)) clashes.push(`${id}: ${queued.get(id)} AND a closed row`);
    closed.set(id, true);
  }

  assert.ok(
    closed.size >= 10,
    `expected to read the closed rows, found ${closed.size} — a guard that reads no rows ` +
      "passes for the wrong reason",
  );
  assert.deepEqual(
    clashes,
    [],
    "these ids name two different pieces of work; the one that has NOT already merged " +
      "takes the next free number, and the renumber is recorded in its Notes cell",
  );
});

// ---------------------------------------------------------------------------
// Invariant 2 — the `#` column enumerates the queue.
test("docs/109: the # column runs 1..N", () => {
  const queue = queueRows(read(BACKLOG));
  assert.deepEqual(
    queue.map((r) => r.nRaw),
    Array.from({ length: queue.length }, (_, i) => String(i + 1)),
    "docs/109's # column must run 1..N with no gaps, repeats or suffixes. The # is a " +
      "POSITION, not an identity (the Id is the identity), so inserting a row means " +
      "renumbering the column — a suffixed position such as `2e` is how sixteen rows once " +
      "escaped every assertion in this file",
  );
});

// ---------------------------------------------------------------------------
// Invariant 3 — the order is the one the document states.
//
// Lane first ("lets complete the hotfix.. target those than audit and than
// roadmap", the owner, 2026-09-20), then priority within the lane. The Roadmap
// lane runs its unprioritised rows — the owner decisions and the phase rows read
// out of `106`, which states no priority for them — before the rows minted in
// `109`, which are graded at minting and so invent nothing.
test("docs/109: lanes and priorities run the way the document says they do", () => {
  const queue = queueRows(read(BACKLOG));

  let lane = 0;
  for (const r of queue) {
    assert.ok(LANES.includes(r.lane), `docs/109 row ${r.n} has unknown lane ${r.lane}`);
    const at = LANES.indexOf(r.lane);
    assert.ok(
      at >= lane,
      `docs/109 row ${r.n} (${r.id}) is lane ${r.lane} after lane ${LANES[lane]} — ` +
        "the lanes must run Hotfix, then Audit, then Roadmap",
    );
    lane = at;
  }

  for (const name of ["Hotfix", "Audit"]) {
    let pri = 0;
    for (const r of queue.filter((x) => x.lane === name)) {
      const at = PRIORITIES.indexOf(r.priority);
      assert.ok(at >= 0, `docs/109 row ${r.n} (${r.id}) in lane ${name} needs a P0-P3 priority`);
      assert.ok(
        at >= pri,
        `docs/109 row ${r.n} (${r.id}) is ${r.priority} after ${PRIORITIES[pri]} — ` +
          `the ${name} lane must run P0 to P3`,
      );
      pri = at;
    }
  }

  // Roadmap: every unprioritised row, then every graded one in order. A
  // `106`-sourced row must stay unprioritised — grading it here would be
  // re-grading, which this document forbids — so the Source cell decides which
  // half of the lane a row may sit in.
  const roadmap = queue.filter((x) => x.lane === "Roadmap");
  let graded = false;
  let pri = 0;
  for (const r of roadmap) {
    const at = PRIORITIES.indexOf(r.priority);
    if (at < 0) {
      assert.ok(
        !graded,
        `docs/109 row ${r.n} (${r.id}) is an unprioritised Roadmap row after a graded one — ` +
          "the Roadmap lane runs its phase and decision rows first, then the rows minted here",
      );
      continue;
    }
    assert.ok(
      !/\b106\b/.test(r.source),
      `docs/109 row ${r.n} (${r.id}) carries priority ${r.priority} and cites 106 as its ` +
        "source, but 106 states no priority for its phase rows — inventing one here is " +
        "re-grading. A row minted in 109 may be graded; a row read out of 106 may not",
    );
    assert.ok(
      at >= pri,
      `docs/109 row ${r.n} (${r.id}) is ${r.priority} after ${PRIORITIES[pri]} in the ` +
        "Roadmap lane's graded block",
    );
    graded = true;
    pri = at;
  }

  // A queue of closed work is not a queue. A row that closes moves to "Closed
  // since the queue was opened"; it is not left in place with a Fixed status,
  // which is how #747's eleven closed rows came to occupy eleven queue positions.
  for (const r of queue) {
    assert.ok(
      isOpen(r.status),
      `docs/109 row ${r.n} (${r.id}) is not open: "${r.status}" — move it to "Closed since ` +
        'the queue was opened" rather than leaving it in the queue',
    );
  }
});

// ---------------------------------------------------------------------------
// Invariant 4 — nothing open in an archive has fallen out of the one queue.
//
// `104` and `105` are matched row by row, by id. `106`'s only id-bearing table
// is its owner decisions. `14` is matched over its forward-work rows, whose ids
// `109` must name one at a time. `99` carries NO ids at all, so it is matched by
// section heading — which is why `109`'s coverage tables are hand-authored: a
// generated coverage table would be satisfied by construction and could never go
// red, which is the `docs/105` CQ-003 class of guard this repository has shipped
// before and been burnt by.
test("docs/109: no open row of 104 or 105 has fallen out of the one queue", () => {
  const backlog = read(BACKLOG);
  const queue = queueRows(backlog);

  // A merged row keeps its id but gets no row of its own: it is DECLARED in the
  // "What was merged" table, and named in the Supersedes/see-also column of the
  // row that absorbed it. Both are required. An earlier draft of this guard
  // accepted a bare see-also mention as coverage, and deleting the OO-010 row
  // did not turn it red — OO-010 is cited by five other rows, so the queue could
  // lose the row that actually carries the work while the guard stayed green. So
  // coverage means "has a row, or is declared merged".
  const listed = new Set(queue.map((r) => r.id));
  const seeAlso = new Set();
  for (const r of queue) {
    for (const m of r.supersedes.match(/\b(?:HF|EV|UX|CQ|OO)-\d+\b|\bFID-[PLR]-\d+\b/g) ?? []) {
      seeAlso.add(m);
    }
  }

  const merged = new Set(
    rows(
      backlog.split("## What was merged")[1] ?? "",
      /^(?:HF|EV|UX|CQ|OO)-\d+$|^FID-[PLR]-\d+$/,
    ).map((r) => r.id),
  );
  assert.ok(merged.size > 0, "docs/109 must carry a 'What was merged' table");
  for (const id of merged) {
    assert.ok(
      !listed.has(id),
      `docs/109 declares ${id} merged away and also queues it as a row of its own`,
    );
    assert.ok(
      seeAlso.has(id),
      `docs/109 declares ${id} merged, but no row names it in Supersedes / see also — ` +
        "the work it stands for is now unreachable from the queue",
    );
  }
  const mentioned = new Set([...listed, ...merged]);

  const sources = [
    ["104-HOTFIX-TRACKER.md", [/^HF-\d+$/]],
    [
      "105-AUDIT-2026-09-TRACKER.md",
      [/^EV-\d+$/, /^UX-\d+$/, /^CQ-\d+$/, /^FID-[PLR]-\d+$/, /^OO-\d+$/],
    ],
  ];

  const missing = [];
  const closedButQueued = [];
  for (const [file, patterns] of sources) {
    const text = read(file);
    for (const pattern of patterns) {
      for (const r of rows(text, pattern)) {
        if (isOpen(r.status)) {
          if (!mentioned.has(r.id)) missing.push(`${r.id} (open in ${file.slice(0, 3)})`);
        } else if (listed.has(r.id)) {
          closedButQueued.push(
            `${r.id} (closed in ${file.slice(0, 3)}: "${r.status.slice(0, 60)}")`,
          );
        }
      }
    }
  }

  // Checked before the coverage list, so that mis-typing one row's id reports the
  // closed row it now points at rather than only the row it stopped being.
  assert.deepEqual(
    closedButQueued,
    [],
    "docs/109 queues rows their own source records as closed: " + closedButQueued.join(", "),
  );
  assert.deepEqual(
    missing,
    [],
    "these rows are open in an archive and appear nowhere in docs/109 — work has " +
      "fallen out of the one queue: " + missing.join(", "),
  );
});

test("docs/109: every open owner decision in 106 is queued", () => {
  const backlog = read(BACKLOG);
  const open = [];
  // `106` §9 has no Status column: a decision is closed when its row is struck
  // through, or when its recommendation opens with "Closed". Both are how Q4 and
  // Q5 are recorded there, so the predicate is read off the document rather than
  // invented.
  for (const line of read("106-ONLYOFFICE-ALTERNATIVE-ROADMAP.md").split("\n")) {
    if (!line.startsWith("|")) continue;
    const c = cells(line);
    if (!/^~*Q\d~*$/.test(c[0])) continue;
    const id = c[0].replaceAll("~", "");
    const struck = c.some((x) => x.includes("~~"));
    const closed = /^\*\*Closed\b/.test(c[c.length - 1]);
    if (!struck && !closed) open.push(id);
  }
  assert.ok(open.length >= 5, `expected 106 to carry open owner decisions, found ${open.length}`);
  const missing = open.filter((id) => !new RegExp(`\\b${id}\\b`).test(backlog));
  assert.deepEqual(
    missing,
    [],
    "these owner decisions are open in 106 and named nowhere in docs/109: " + missing.join(", ") +
      " — a decision is the cheapest unblocker in the queue and gates a whole phase",
  );
});

test("docs/14: every forward-work row is named in 109's archive coverage", () => {
  const all = executionRows(read("14-EXECUTION-TRACKER.md"));
  assert.ok(all.length > 400, `expected 14 to parse to hundreds of rows, got ${all.length}`);

  const forward = all.filter((r) => forwardClass(r.status));
  const covered = new Set(
    tableColumn(read(BACKLOG), "| `14` id | Status in `14` |"),
  );

  const missing = forward
    .filter((r) => !covered.has(r.id.replaceAll("`", "")))
    .map((r) => `${r.id} (${r.status})`);
  assert.deepEqual(
    missing,
    [],
    "these rows of docs/14 are in a forward-work state and docs/109's archive-coverage " +
      "table does not name them, so nothing says whether the work is queued, superseded " +
      "or merely unverified — which is how 14 came to sit outside the one-queue scheme " +
      "for a month: " + missing.join(", "),
  );

  // And the other direction: a coverage row for an id `14` no longer carries is a
  // claim about nothing, and it is also how a row gets quietly retired — close it
  // in `14` and the coverage row stops meaning anything, with no diff to notice.
  const ids = new Set(all.map((r) => r.id.replaceAll("`", "")));
  const stale = [...covered].filter((id) => !ids.has(id));
  assert.deepEqual(
    stale,
    [],
    "docs/109's archive-coverage table names rows that docs/14 does not have: " +
      stale.join(", "),
  );
});

test("docs/99: every section is named in 109's archive coverage", () => {
  // `99` carries no ids, so there is nothing to match row by row. Its sections
  // are the unit, and a new section is exactly how a page of unfinished
  // capability would arrive outside the queue again.
  const headings = read("99-REMAINING-WORK-AUDIT.md")
    .split("\n")
    .filter((l) => l.startsWith("## "))
    .map((l) => l.slice(3).trim());
  assert.ok(headings.length >= 6, `expected 99 to carry its sections, found ${headings.length}`);

  const covered = tableColumn(read(BACKLOG), "| `99` section | Where it is covered here |");
  const plain = (s) => s.replaceAll("*", "").replaceAll("`", "").trim();
  const missing = headings.map(plain).filter((h) => !covered.includes(h));
  assert.deepEqual(
    missing,
    [],
    "these sections of docs/99 are not named in docs/109's archive coverage, so nothing " +
      "says where their open work is queued: " + missing.join(" / "),
  );
  const stale = covered.filter((h) => !headings.map(plain).includes(h));
  assert.deepEqual(
    stale,
    [],
    "docs/109's archive coverage names sections docs/99 does not have: " + stale.join(" / "),
  );
});

// ---------------------------------------------------------------------------
// The published counts, re-derived.
//
// `SKILL.md` §8: counts in docs are derived, never hand-maintained. These ones
// are published twice over — in `14`'s own banner and in `109` CQ-011 — and
// `104`'s hand-maintained summary read 114/47 against an actual 146/54 for as
// long as nothing re-derived it.
test("docs/14's class counts are what its rows actually say", () => {
  const all = executionRows(read("14-EXECUTION-TRACKER.md"));
  const derived = {
    Done: all.filter((r) => /^Done\b/.test(r.status)).length,
    "Completed-work index": all.filter((r) => r.status === "").length,
    "In review": all.filter((r) => /^In review\b/.test(r.status)).length,
    "Closed by their own words": all.filter(
      (r) => /^(Superseded|Fixed by)\b/.test(r.status),
    ).length,
    "Forward work": all.filter((r) => forwardClass(r.status)).length,
  };
  // Every row must land in exactly one class, or a class count is a subset
  // dressed as a partition — the shape of `104`'s drift, where two equal and
  // opposite per-section errors still summed to the stated total.
  const sum = Object.values(derived).reduce((a, b) => a + b, 0);
  assert.equal(
    sum,
    all.length,
    `docs/14's five classes account for ${sum} of ${all.length} rows — some status matches ` +
      "no class and is therefore invisible in every published count",
  );

  const backlog = read(BACKLOG);
  const labels = tableColumn(backlog, "| Class | Rows |");
  const counts = tableColumn(backlog, "| Class | Rows |", 1);
  for (const [label, n] of Object.entries(derived)) {
    const i = labels.findIndex((l) => l.startsWith(label));
    assert.ok(i >= 0, `docs/109's class table must carry a "${label}" row`);
    assert.equal(
      Number(counts[i]),
      n,
      `docs/109's class table says ${counts[i]} for "${label}"; docs/14's rows say ${n}`,
    );
  }

  // `14`'s banner publishes the same numbers in prose, which is the form a reader
  // meets first. Whitespace is normalised so rewrapping the paragraph cannot
  // break the check — a guard that fails on a line break teaches people to delete
  // guards.
  const banner = read("14-EXECUTION-TRACKER.md")
    .split("\n")
    .filter((l) => l.startsWith(">"))
    .map((l) => l.replace(/^>\s?/, ""))
    .join(" ")
    .replace(/\s+/g, " ");
  const m = banner.match(
    /(\d+) rows, of which (\d+) `Done`, (\d+) in the completed-work index, \*\*(\d+) `In review`\*\*[^,]*, (\d+) closed by their own words, and \*\*(\d+) in a forward-work state\*\*/,
  );
  assert.ok(m, "docs/14's banner must publish its derived class counts in that sentence");
  assert.equal(Number(m[1]), all.length, "docs/14 banner: total rows");
  assert.equal(Number(m[2]), derived.Done, "docs/14 banner: Done");
  assert.equal(Number(m[3]), derived["Completed-work index"], "docs/14 banner: completed index");
  assert.equal(Number(m[4]), derived["In review"], "docs/14 banner: In review");
  assert.equal(
    Number(m[5]),
    derived["Closed by their own words"],
    "docs/14 banner: closed by their own words",
  );
  assert.equal(Number(m[6]), derived["Forward work"], "docs/14 banner: forward work");

  // And CQ-011's own cell, which breaks the forward-work class down by status.
  // The breakdown is the number a reader would quote to argue about scope.
  const LABELS = {
    "In progress": "In progress",
    "Not started": "Not started",
    Designing: "Designing",
    Pending: "Pending",
    Accepted: "Accepted",
    Planned: "Planned",
    Designed: "Designed",
    Open: "Open",
    "Experimental design": "Experimental design; not implemented",
  };
  for (const prefix of FORWARD_STATUSES) {
    const n = all.filter((r) => forwardClass(r.status) === prefix).length;
    assert.ok(
      backlog.includes(`\`${LABELS[prefix]}\` ${n}`),
      `docs/109 CQ-011 must state "\`${LABELS[prefix]}\` ${n}" — that is what docs/14's ` +
        "rows say, and the breakdown is the figure a reader quotes to argue about scope",
    );
  }
});
