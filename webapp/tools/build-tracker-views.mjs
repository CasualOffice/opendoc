#!/usr/bin/env node
// Read-only status views of the one open queue. Edit docs/109, then regenerate.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { queueRows } from "../tests/tracker_parse.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const queue = queueRows(readFileSync(join(root, "docs/109-BACKLOG.md"), "utf8"));
const check = process.argv.includes("--check");

function inProgress(status) {
  return /^(?:In progress|Partly|Open \(partly delivered\)|Open — (?:implementation merged|reproduction in flight|fixed on)|Open, (?:designed|measured and designed))/.test(status);
}

function table(rows) {
  return [
    "| # | Id | Priority | Work | Status |",
    "| ---: | --- | --- | --- | --- |",
    ...rows.map((row) => `| ${row.nRaw} | ${row.id} | ${row.priority} | ${row.what.replaceAll("|", "\\|")} | ${row.status.replaceAll("|", "\\|")} |`),
  ].join("\n");
}

const views = [
  ["pending.md", "Pending work", queue.filter((row) => !inProgress(row.status))],
  ["in-progress.md", "In progress or partly delivered", queue.filter((row) => inProgress(row.status))],
];
for (const [name, title, rows] of views) {
  const target = join(root, "docs/tracker", name);
  const content = `# ${title}\n\nGenerated from the single open queue in [docs/109](../109-BACKLOG.md).\nEdit the source row there, then run \`node webapp/tools/build-tracker-views.mjs\`.\nA partial status records delivered work; it does not claim that a person is\ncurrently assigned. Rows stay in their queue order.\n\n${table(rows)}\n`;
  if (check) {
    if (readFileSync(target, "utf8") !== content) {
      throw new Error(`${name} is stale; run node webapp/tools/build-tracker-views.mjs`);
    }
  } else {
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, content);
  }
}
