// `.github/workflows/npm-publish.yml` — the one workflow that can put a package
// on npm (docs/176 §8.3).
//
// A publish cannot be taken back: npm refuses to reuse a version even after an
// unpublish, and a stable version under `latest` is what every `npm install`
// without a range resolves to. So the properties that keep a dispatch harmless
// are asserted here rather than trusted to review:
//
//   * it runs only when somebody dispatches it, never on a push or a schedule;
//   * `dry_run` defaults to true;
//   * the version input must carry a pre-release part — exercised against real
//     versions, so the check is the shell pattern the job runs, not a copy of it;
//   * every `npm publish` is `--tag next --access public --provenance`, and
//     `latest` is never named;
//   * the npm token reaches the publish steps only, and nothing echoes it;
//   * the package names are the ones docs/176 §10 decided.
//
// A YAML parser is not a dependency here, so the file is read as text and each
// assertion is pinned to a shape a GitHub workflow cannot express two ways.
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const WORKFLOW = readFileSync(join(REPO, ".github", "workflows", "npm-publish.yml"), "utf8");
/** The workflow with comments stripped, so prose cannot satisfy or trip a guard. */
const CODE = WORKFLOW.split("\n")
  .filter((line) => !/^\s*#/.test(line))
  .join("\n");

/** The top-level block that starts at `key:` and runs to the next top-level key. */
function block(key) {
  const match = CODE.match(new RegExp(`^${key}:\\n((?:[ \\t]+.*\\n|\\n)*)`, "m"));
  assert.ok(match, `the workflow has no top-level \`${key}:\` block`);
  return match[1];
}

/** Each step, as its text, from `- name:` to the next one. */
const STEPS = CODE.split(/\n(?=\s+- name: )/).slice(1);

test("it runs on a manual dispatch and on nothing else", () => {
  const on = block("on");
  const triggers = [...on.matchAll(/^ {2}([a-z_]+):/gm)].map((m) => m[1]);
  assert.deepEqual(triggers, ["workflow_dispatch"]);
});

test("its inputs are package, version and a dry run that defaults to true", () => {
  const on = block("on");
  for (const option of ["viewer", "editor", "both"]) {
    assert.match(on, new RegExp(`^\\s+- ${option}$`, "m"), `package option ${option}`);
  }
  assert.match(on, /^ {6}version:\n(?: {8}.*\n)*? {8}required: true$/m);
  assert.match(
    on,
    /^ {6}dry_run:\n(?: {8}.*\n)*? {8}type: boolean\n(?: {8}.*\n)*? {8}default: true$/m,
    "dry_run must be a boolean that defaults to true, so a dispatch publishes nothing unless asked",
  );
});

test("it holds only the permissions trusted publishing needs, in the npm environment", () => {
  assert.deepEqual(
    block("permissions")
      .trim()
      .split("\n")
      .map((l) => l.trim()),
    ["contents: read", "id-token: write"],
  );
  assert.match(CODE, /^ {4}environment: github-pages$/m);
});

test("the version check accepts pre-releases only", () => {
  const ident = CODE.match(/^\s*ident='([^']+)'$/m)?.[1];
  const pattern = CODE.match(/^\s*pattern="([^"]+)"$/m)?.[1];
  assert.ok(ident && pattern, "the job no longer validates the version with `ident` and `pattern`");
  assert.match(CODE, /\[\[ ! "\$\{RELEASE_VERSION\}" =~ \$\{pattern\} \]\]/);
  const regex = pattern.replaceAll("${ident}", ident);
  // Run the job's own pattern through bash, the interpreter that runs it in CI.
  const accepts = (version) => {
    try {
      execFileSync("bash", ["-c", `[[ "$1" =~ $2 ]]`, "bash", version, regex], { stdio: "pipe" });
      return true;
    } catch {
      return false;
    }
  };
  for (const good of ["0.1.0-alpha.1", "0.1.0-rc.0", "1.0.0-beta", "0.2.3-alpha.1.x-y"]) {
    assert.ok(accepts(good), `rejects the pre-release ${good}`);
  }
  for (const bad of [
    "0.1.0",
    "1.0.0",
    "v0.1.0-alpha.1",
    "0.1-alpha.1",
    "01.0.0-alpha",
    "0.1.0-alpha.01",
    "0.1.0-",
    "0.1.0-alpha..1",
    "0.1.0-alpha.1+build",
    "0.1.0-alpha.1; rm -rf /",
    "",
  ]) {
    assert.ok(!accepts(bad), `accepts ${JSON.stringify(bad)}, which is not a pre-release`);
  }
  // The inputs reach the shell through env, never interpolated into a script.
  for (const step of STEPS) {
    const run = step.split(/^\s+run: /m)[1] ?? "";
    assert.doesNotMatch(run, /\$\{\{\s*inputs\./, "an input is interpolated into a run script");
  }
});

test("every publish is next-tagged, public and provenanced, and latest is never named", () => {
  const publishing = STEPS.filter((s) => /npm publish/.test(s));
  assert.equal(publishing.length, 2, "one publish step per package");
  for (const step of publishing) {
    assert.match(step, /args=\(--tag next --access public --provenance\)/);
    assert.match(step, /if \[\[ "\$\{DRY_RUN\}" == "true" \]\]; then\n\s+args\+=\(--dry-run\)/);
    assert.match(step, /npm publish "\$\{args\[@\]\}"/);
  }
  assert.doesNotMatch(CODE, /\blatest\b/, "the workflow names the latest tag");
  assert.doesNotMatch(CODE, /npm dist-tag/, "the workflow moves dist-tags");
  assert.match(CODE, /npm install --global [^\n]*npm@11\.(?:[6-9]|[1-9]\d)\.\d+/, "npm >= 11.5.1");
});

test("each package is version-bumped, packed and tested before it publishes", () => {
  const order = (needles) => {
    const at = needles.map((n) => STEPS.findIndex((s) => s.includes(n)));
    assert.ok(
      at.every((i, k) => i >= 0 && (k === 0 || i > at[k - 1])),
      `out of order: ${needles.join(" -> ")}`,
    );
  };
  order([
    "./packages/opendoc-viewer/build.sh",
    "npm test",
    "name: Set the viewer version",
    "name: Inspect the viewer tarball",
    "name: Publish the viewer package",
  ]);
  order([
    "webapp/tests/embed_package.test.mjs",
    "name: Set the editor version",
    "name: Inspect the editor tarball",
    "name: Publish the editor package",
  ]);
  for (const step of STEPS.filter((s) => /npm version/.test(s))) {
    assert.match(step, /npm version "\$\{RELEASE_VERSION\}" --no-git-tag-version/);
  }
  assert.equal(STEPS.filter((s) => /npm pack --dry-run/.test(s)).length, 2);
});

test("the npm token reaches the publish steps only, and nothing prints it", () => {
  const lines = CODE.split("\n").filter((l) => l.includes("secrets."));
  for (const line of lines) {
    assert.match(
      line.trim(),
      /^(NODE_AUTH_TOKEN: \$\{\{ secrets\.NPM_PASSWORD \}\}|HAS_NPM_TOKEN: \$\{\{ secrets\.NPM_PASSWORD != '' \}\})$/,
      `an unexpected use of a secret: ${line.trim()}`,
    );
  }
  for (const step of STEPS) {
    if (step.includes("NODE_AUTH_TOKEN: ${{ secrets.NPM_PASSWORD }}")) {
      assert.match(step, /npm publish/, "the token is handed to a step that does not publish");
    }
    assert.doesNotMatch(
      step,
      /(echo|printf)[^\n]*(NODE_AUTH_TOKEN|NPM_PASSWORD)/,
      "a step prints the npm token",
    );
  }
});

test("the package names are the ones docs/176 decided", () => {
  const name = (dir) =>
    JSON.parse(readFileSync(join(REPO, "packages", dir, "package.json"), "utf8")).name;
  assert.equal(name("opendoc-viewer"), "@casualoffice/opendoc-viewer");
  assert.equal(name("opendoc-embed"), "@casualoffice/opendoc-editor");
  const decided = readFileSync(join(REPO, "docs", "176-THREE-SDK-PRODUCTS.md"), "utf8");
  assert.match(decided, /`@casualoffice\/opendoc-viewer` \(Viewer\), `@casualoffice\/opendoc-editor` \(Editor/);
});
