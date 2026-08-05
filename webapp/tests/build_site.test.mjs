// build-site.py idempotency guard.
//
// The flat *.html pages GitHub Pages serves are generated from *.page.html
// templates + _partials/ by build-site.py and committed. This test runs the
// generator's `--check` mode, which re-renders every template in memory and
// diffs it against the committed HTML — so a template or partial edit that
// wasn't regenerated (or hand-edited generated HTML) fails here, the same guard
// build.sh and CI run. execFileSync throws on a non-zero exit, failing the test.
import { test } from "node:test";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const webapp = dirname(dirname(fileURLToPath(import.meta.url))); // tests/ -> webapp/
const script = join(webapp, "build-site.py");

test("committed static pages are byte-identical to a fresh generator run", () => {
  execFileSync("python3", [script, "--check"], { cwd: webapp, stdio: "pipe" });
});
