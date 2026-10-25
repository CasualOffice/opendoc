import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { queueRows, read } from "./tracker_parse.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

function viewIds(name) {
  return [...read(`tracker/${name}`).matchAll(/^\|\s*\d+\s*\|\s*([A-Z][A-Za-z0-9-]+)\s*\|/gm)]
    .map((match) => match[1]);
}

test("pending and in-progress views match the one open queue", () => {
  execFileSync(process.execPath, [join(root, "webapp/tools/build-tracker-views.mjs"), "--check"], { cwd: root });
  const queued = queueRows(read("109-BACKLOG.md")).map((row) => row.id);
  const pending = viewIds("pending.md");
  const active = viewIds("in-progress.md");
  assert.deepEqual([...new Set([...pending, ...active])].sort(), [...queued].sort());
  assert.equal(pending.length + active.length, queued.length, "a row appears in exactly one view");
});
