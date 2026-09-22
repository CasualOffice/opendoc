// No source file in `webapp/` may contain a raw control byte.
//
// This exists because one did, in this very branch: `spell_check.mjs` used
// U+0000 as a cache-key separator and five RAW NUL BYTES were written into the
// file instead of the `\u0000` escape. The code ran correctly — a NUL is a
// perfectly good character in a JavaScript string — which is exactly why it
// would have survived every other gate here.
//
// What it breaks is the repository. `tracker_counts.test.mjs` already records
// the same trap, in its own words:
//
//   > A sentinel that cannot occur in Markdown. Written as an escape, not as a
//   > literal NUL byte: a raw NUL makes git classify this file as binary and
//   > silently stop showing its diffs.
//
// A file git thinks is binary cannot be reviewed, cannot be merged by line, and
// hides every later change to it. That is a worse outcome than most bugs, and
// nothing would have said so.
//
// So this is the CLASS, not the instance (SKILL.md §10): every text file under
// `webapp/` that a person is expected to read, checked for any control
// character that has no business being there.

import assert from "node:assert/strict";
import test from "node:test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { extname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = fileURLToPath(new URL("..", import.meta.url));

/** Extensions whose contents are meant to be read by a human. */
const TEXT = new Set([".js", ".mjs", ".css", ".html", ".json", ".md", ".txt", ".py", ".sh", ".yml"]);

/** Directories that are not ours to police. */
const SKIP = new Set(["node_modules", "pkg", "test-results", "playwright-report", ".git"]);

/** Control characters that are never legitimate in these files. Tab, newline
 *  and carriage return are excluded: they are whitespace, not payload. */
const FORBIDDEN = new RegExp("[\\u0000-\\u0008\\u000B\\u000C\\u000E-\\u001F\\u007F]");

function* textFiles(dir) {
  for (const name of readdirSync(dir)) {
    if (SKIP.has(name)) continue;
    const path = join(dir, name);
    let info;
    try {
      info = statSync(path);
    } catch {
      continue;
    }
    if (info.isDirectory()) {
      yield* textFiles(path);
    } else if (TEXT.has(extname(name))) {
      yield path;
    }
  }
}

test("no source file carries a raw control byte", () => {
  const offenders = [];
  for (const path of textFiles(WEBAPP)) {
    const text = readFileSync(path, "latin1");
    const at = text.search(FORBIDDEN);
    if (at < 0) continue;
    const line = text.slice(0, at).split("\n").length;
    const code = text.charCodeAt(at).toString(16).padStart(4, "0");
    offenders.push(`${relative(WEBAPP, path)}:${line} contains U+${code.toUpperCase()}`);
  }
  assert.deepEqual(
    offenders,
    [],
    "write the escape (`\\u0000`), not the byte — a raw control character makes " +
      "git treat the file as binary and stop showing its diffs, so every later " +
      "change to it becomes unreviewable",
  );
});
