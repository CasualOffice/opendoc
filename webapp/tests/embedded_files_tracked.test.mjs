// Every file a Rust crate embeds at compile time must be IN THE REPOSITORY.
//
// `include_bytes!`/`include_str!` read whatever is on disk where the build
// runs. `webapp/sample.docx` is on disk on any machine that has run
// `webapp/build.sh` — it stages a copy of the committed `sample.docx` there —
// and is in `webapp/.gitignore`, so a test that embeds it compiles here and
// fails on CI's clean checkout, before a single test runs. That happened three
// times: `casual-doc-wasm`'s surface-walk test (the comment beside it in
// `lib.rs` records it), `casual-doc-edit`'s field-result tests, and then three
// test modules at once on #819, where `lint` went red on
// "couldn't read `…/webapp/sample.docx`".
//
// So this resolves every literal embed path against the file that contains it
// and asks git whether that path is tracked. It catches the whole class — any
// staged, generated or ignored file — not the one name that bit last.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const REPO = fileURLToPath(new URL("../../", import.meta.url));
const ROOTS = ["crates", "tools"];
const EMBED = /include_(?:bytes|str)!\(\s*"([^"]+)"\s*\)/g;

/** Every `.rs` file under `dir`, skipping build output. O(files). */
function rustFiles(dir) {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === "target" || entry.name === "node_modules") continue;
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...rustFiles(path));
    else if (entry.name.endsWith(".rs")) out.push(path);
  }
  return out;
}

/** Each literal embed as `{ file, path }`, both repository-relative. */
export function embeds(files, read = (file) => readFileSync(file, "utf8")) {
  const found = [];
  for (const file of files) {
    for (const match of read(file).matchAll(EMBED)) {
      const target = relative(REPO, resolve(dirname(file), match[1])).split("\\").join("/");
      found.push({ file: relative(REPO, file).split("\\").join("/"), path: target });
    }
  }
  return found;
}

function trackedFiles() {
  const listed = execFileSync("git", ["ls-files", "-z"], {
    cwd: REPO,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  return new Set(listed.split("\0").filter(Boolean));
}

test("every file a Rust crate embeds at compile time is tracked by git", () => {
  const files = ROOTS.flatMap((root) => rustFiles(join(REPO, root)));
  const found = embeds(files);
  assert.ok(found.length > 50, `the scan must find the embeds it guards (found ${found.length})`);
  const tracked = trackedFiles();
  const untracked = found.filter(({ path }) => !tracked.has(path));
  assert.deepEqual(
    untracked,
    [],
    "these embeds read a file that is not in the repository, so they compile only where it happens to exist " +
      "(embed the committed copy instead — `sample.docx` at the root, not `webapp/sample.docx`)",
  );
});

test("the scan resolves an embed against the file that contains it", () => {
  const file = join(REPO, "crates/casual-doc-wasm/src/example_tests.rs");
  const [one, two] = embeds([file], () =>
    'const A: &[u8] = include_bytes!("../../../webapp/sample.docx");\nconst B: &str = include_str!( "lib.rs" );',
  );
  assert.deepEqual(one, { file: "crates/casual-doc-wasm/src/example_tests.rs", path: "webapp/sample.docx" });
  assert.deepEqual(two, { file: "crates/casual-doc-wasm/src/example_tests.rs", path: "crates/casual-doc-wasm/src/lib.rs" });
});
