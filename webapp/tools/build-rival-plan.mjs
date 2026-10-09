#!/usr/bin/env node
// Generates the two score blocks in `docs/168-WORD-AND-DOCS-RIVAL-PLAN.md` — the
// scores as they stand and the projection after each batch — from committed
// sources, so no number in that plan is typed by hand (SKILL.md §9).
//
// The four stage scores come from the public fidelity matrix
// (`webapp/src/fidelity.js`): every construct family is graded on reading,
// drawing, editing and saving, and a stage's score is the mean of its cells,
// with full = 1, partial = 0.5, preserved or placeholder = 0.25 and none = 0.
// The editing-parity score comes from the capability matrix
// (`docs/153-ONLYOFFICE-CAPABILITY-PARITY-MATRIX.md`): (Parity + Partial / 2)
// over Parity + Partial + Gap. "Ours only" and "Theirs, gated" rows are
// outside the denominator, as the matrix's own tally keeps them.
//
// The projection replays the plan's batches in order. A batch moves the
// fidelity cells its `Moves:` line names and closes the docs/153 capabilities
// its task tables name, and a closed capability counts as Parity. That is the
// claim each batch makes; the batch is not done until both files say so.
//
// Usage, from `webapp/`:
//   node tools/build-rival-plan.mjs            # rewrite the two blocks
//   node tools/build-rival-plan.mjs --check    # verify only, exit 1 on drift

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const ROOT = new URL("../../", import.meta.url);
const PLAN = "docs/168-WORD-AND-DOCS-RIVAL-PLAN.md";
const MATRIX = "docs/153-ONLYOFFICE-CAPABILITY-PARITY-MATRIX.md";
const CAPABILITIES = "tools/opendoc-parity/data/capabilities.json";
const FIDELITY = "webapp/src/fidelity.js";

export const STAGES = ["modeled", "rendered", "editable", "roundtrips"];
/** The plan's words for the four stages, in the same order. */
export const STAGE_WORDS = ["read", "drawn", "editable", "saved"];
export const WEIGHT = { full: 1, partial: 0.5, preserved: 0.25, placeholder: 0.25, none: 0 };
export const TARGET = 85;
/** Verdicts a batch can close; the rest are already closed or outside the score. */
export const OPEN = new Set(["Gap", "Partial", "Theirs, gated"]);

const read = (path) => readFileSync(new URL(path, ROOT), "utf8");

/** The fidelity rows, read the way `fidelity_data.test.mjs` reads them. */
export function fidelityRows(source = read(FIDELITY)) {
  const sandbox = { exports: {} };
  new Function("module", source)(sandbox);
  return sandbox.exports.FIDELITY;
}

/**
 * Every capability with its docs/153 verdict, keyed by id. The matrix prints
 * titles, not ids, grouped by area in the capabilities file's area order and
 * sorted by id within an area, so the two are zipped in that order and every
 * title is compared; a mismatch is an error, not a guess.
 */
export function capabilityVerdicts(matrix = read(MATRIX), data = JSON.parse(read(CAPABILITIES))) {
  const body = matrix.split("<!-- @generated parity-matrix -->")[1]?.split("<!-- @end parity-matrix -->")[0];
  if (!body) throw new Error(`${MATRIX}: no parity-matrix block`);
  const rows = [];
  for (const line of body.split("\n")) {
    if (!line.startsWith("| ") || line.startsWith("| Capability") || line.startsWith("| ---")) continue;
    const cells = line.split(/(?<!\\)\|/).slice(1, -1).map((cell) => cell.trim());
    rows.push({ title: cells[0], verdict: cells[1] });
  }
  const ordered = data.areas.flatMap((area) =>
    data.capabilities
      .filter((capability) => capability.area === area.id)
      .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)),
  );
  if (ordered.length !== rows.length) {
    throw new Error(`${MATRIX} prints ${rows.length} rows for ${ordered.length} capabilities`);
  }
  const verdicts = new Map();
  ordered.forEach((capability, i) => {
    if (rows[i].title !== capability.title) {
      throw new Error(`${MATRIX} row ${i + 1} is "${rows[i].title}", expected "${capability.title}" (${capability.id})`);
    }
    verdicts.set(capability.id, { title: capability.title, verdict: rows[i].verdict });
  });
  return verdicts;
}

/**
 * The batches, the deferred rows and the tracker ids the plan names. A batch
 * whose `**State:**` line says done keeps its moves and ids as a record; they
 * are already true of the matrices, so replaying them changes nothing.
 */
export function parsePlan(text = read(PLAN)) {
  const batches = [];
  const deferred = [];
  let batch = null;
  let section = "";
  for (const line of text.split("\n")) {
    if (line.startsWith("## ")) section = line.slice(3).trim();
    const heading = line.match(/^### (\d+)\. (.+) \(([^)]+)\)$/);
    if (heading) {
      batch = {
        order: Number(heading[1]),
        title: heading[2],
        ids: heading[3].split(", "),
        done: false,
        moves: [],
        closes: [],
      };
      batches.push(batch);
      continue;
    }
    if (section === "Deferred" && /^\| `/.test(line)) {
      deferred.push(line.match(/^\| `([^`]+)`/)[1]);
      continue;
    }
    if (!batch || !section.startsWith("The batches")) continue;
    if (line.startsWith("**State:** done")) {
      batch.done = true;
    } else if (line.startsWith("**Moves:**")) {
      for (const [, family, word, value] of line.matchAll(/`([^`]+)` (read|drawn|editable|saved) → (full|partial)/g)) {
        batch.moves.push({ family, stage: STAGES[STAGE_WORDS.indexOf(word)], value });
      }
    } else if (line.startsWith("| ") && !line.startsWith("| Task") && !line.startsWith("| ---")) {
      const closes = line.split(/(?<!\\)\|/).slice(1, -1)[1] ?? "";
      for (const [, id] of closes.matchAll(/`([a-z]+\.[A-Za-z0-9.-]+)`/g)) batch.closes.push(id);
    }
  }
  return { batches, deferred };
}

const percent = (x) => Math.round(x * 100);

/** The five scores for a set of fidelity cells and a verdict tally. */
function score(cells, tally) {
  const stages = STAGES.map((stage) => percent(cells.reduce((sum, row) => sum + WEIGHT[row[stage]], 0) / cells.length));
  const parity = percent((tally.Parity + tally.Partial / 2) / (tally.Parity + tally.Partial + tally.Gap));
  return [...stages, parity];
}

/** The scores now and after each batch, in the plan's order. */
export function project(fidelity, verdicts, plan) {
  const cells = fidelity.map((row) => ({ family: row.family, ...Object.fromEntries(STAGES.map((s) => [s, row[s]])) }));
  const byFamily = new Map(cells.map((row) => [row.family, row]));
  const tally = { Parity: 0, Partial: 0, Gap: 0 };
  const open = new Map();
  for (const [id, { verdict }] of verdicts) {
    if (verdict in tally) tally[verdict] += 1;
    if (OPEN.has(verdict)) open.set(id, verdict);
  }
  const steps = [{ order: 0, label: "Now", scores: score(cells, tally) }];
  for (const batch of [...plan.batches].sort((a, b) => a.order - b.order)) {
    for (const { family, stage, value } of batch.moves) {
      const row = byFamily.get(family);
      if (!row) throw new Error(`batch ${batch.order} moves "${family}", which ${FIDELITY} does not grade`);
      if (WEIGHT[value] > WEIGHT[row[stage]]) row[stage] = value;
    }
    for (const id of batch.closes) {
      const verdict = open.get(id);
      if (verdict === "Gap") tally.Gap -= 1;
      if (verdict === "Partial") tally.Partial -= 1;
      if (verdict === "Gap" || verdict === "Partial") tally.Parity += 1;
      open.delete(id);
    }
    steps.push({ order: batch.order, label: `${batch.title} (${batch.ids.join(", ")})`, scores: score(cells, tally) });
  }
  return { steps, counts: tally };
}

const HEAD = "| Read | Drawn | Editable | Saved | Editing parity |";

function scoresBlock(fidelity, verdicts, projection) {
  const now = projection.steps[0].scores;
  const graded = (stage, value) => fidelity.filter((row) => row[stage] === value).length;
  const lines = [
    `Generated by \`webapp/tools/build-rival-plan.mjs\` from \`${FIDELITY}\` (${fidelity.length} families × 4 stages) and \`${MATRIX}\` (${verdicts.size} capabilities). Target: ${TARGET} on every column.`,
    "",
    "| Measure | Score | Full | Partial | Kept or placeholder | None |",
    "| --- | ---: | ---: | ---: | ---: | ---: |",
    ...STAGES.map(
      (stage, i) =>
        `| ${STAGE_WORDS[i][0].toUpperCase()}${STAGE_WORDS[i].slice(1)} | ${now[i]} | ${graded(stage, "full")} | ${graded(stage, "partial")} | ${graded(stage, "preserved") + graded(stage, "placeholder")} | ${graded(stage, "none")} |`,
    ),
  ];
  const tally = { Parity: 0, Partial: 0, Gap: 0 };
  for (const { verdict } of verdicts.values()) if (verdict in tally) tally[verdict] += 1;
  lines.push(`| Editing parity | ${now[4]} | ${tally.Parity} at parity | ${tally.Partial} partial | — | ${tally.Gap} gaps |`);
  return lines.join("\n");
}

/** When each measure first reaches the target, in words. */
function milestones(projection) {
  const names = [...STAGE_WORDS.map((w) => `${w[0].toUpperCase()}${w.slice(1)}`), "Editing parity"];
  return names
    .map((name, i) => {
      const step = projection.steps.find((s) => s.scores[i] >= TARGET);
      if (!step) return `${name} does not reach it.`;
      return step.order === 0 ? `${name} is there now.` : `${name} reaches it after batch ${step.order}.`;
    })
    .join(" ");
}

function projectionBlock(projection) {
  const mark = (x) => (x >= TARGET ? `**${x}**` : `${x}`);
  return [
    `| # | After | ${HEAD.slice(2)}`,
    "| ---: | --- | ---: | ---: | ---: | ---: | ---: |",
    ...projection.steps.map((step) => `| ${step.order || "—"} | ${step.label} | ${step.scores.map(mark).join(" | ")} |`),
    "",
    `Bold is at or above the target of ${TARGET}. ${milestones(projection)}`,
  ].join("\n");
}

function splice(text, name, content) {
  const open = `<!-- @generated ${name} -->`;
  const close = `<!-- @end ${name} -->`;
  const start = text.indexOf(open);
  const end = text.indexOf(close);
  if (start < 0 || end < start) throw new Error(`${PLAN}: no ${name} block`);
  return `${text.slice(0, start + open.length)}\n${content}\n${text.slice(end)}`;
}

/** The plan as it should read, from the committed inputs. */
export function build(text = read(PLAN)) {
  const fidelity = fidelityRows();
  const verdicts = capabilityVerdicts();
  const projection = project(fidelity, verdicts, parsePlan(text));
  let out = splice(text, "rival-scores", scoresBlock(fidelity, verdicts, projection));
  out = splice(out, "rival-projection", projectionBlock(projection));
  return { text: out, projection, verdicts };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const current = read(PLAN);
  const { text } = build(current);
  if (process.argv.includes("--check")) {
    if (text !== current) {
      console.error(`build-rival-plan --check: ${PLAN} is stale; run node tools/build-rival-plan.mjs`);
      process.exitCode = 1;
    } else {
      console.log(`build-rival-plan --check: ${PLAN} up to date.`);
    }
  } else {
    writeFileSync(new URL(PLAN, ROOT), text);
    console.log(`build-rival-plan: wrote ${PLAN}`);
  }
}
