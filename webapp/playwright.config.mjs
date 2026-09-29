import { createHash } from "node:crypto";
import { defineConfig, devices } from "@playwright/test";

// Port allocation — the fix for shared-port e2e flakiness.
//
// Previously this config used a fixed port (8099) with reuseExistingServer:true
// locally. When several agents run `npm run test:e2e` concurrently from
// different git worktrees on one machine, they all target 8099 and Playwright
// happily *reuses* whichever serve.py already holds the port — serving a STALE
// bundle from another worktree's directory. That produced false failures and
// 404s that cost real debugging time across agents.
//
// Now each worktree gets its own port, derived deterministically from the
// working directory, so two concurrent worktrees never collide:
//   * PW_PORT env var wins if set (explicit override / manual runs);
//   * otherwise the port is a stable hash of process.cwd() — the worktree's
//     webapp dir, a unique absolute path per worktree — mapped into a high,
//     unprivileged range that avoids common dev ports.
// The port is threaded into baseURL, the serve.py command, and the readiness
// URL so all three always agree.
//
// reuseExistingServer is false: Playwright always starts its own server for the
// run and tears it down after, so a leftover/stale server is never reused. With
// per-worktree ports a same-worktree reuse would in fact be safe (serve.py
// reads from disk with no-cache), but false-reuse is the belt-and-suspenders
// that turns the vanishingly rare hash collision into a loud EADDRINUSE failure
// instead of a silent stale-bundle serve. CI already ran with reuse disabled
// (it sets CI), so this does not change CI behaviour.
function portFromCwd() {
  const digest = createHash("sha256").update(process.cwd()).digest();
  // 20000..59999 — above privileged/registered dev ports, below the 65535 ceiling.
  return 20000 + (digest.readUInt32BE(0) % 40000);
}

const port = Number(process.env.PW_PORT) || portFromCwd();
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: "tests/e2e",
  timeout: 60_000,
  fullyParallel: true,
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? [["html", { open: "never" }], ["list"]] : "list",
  use: {
    baseURL,
    trace: "retain-on-failure",
    // `--enable-precise-memory-info` makes `performance.memory.usedJSHeapSize`
    // return real (not quantized) bytes; `--expose-gc`/`--js-flags=--expose-gc`
    // lets the memory-budget spec force a deterministic collection before
    // measuring. Harmless to the other specs.
    launchOptions: { args: ["--enable-precise-memory-info", "--js-flags=--expose-gc"] },
  },
  // Two device classes, because this shell has two (ADR-044) and one of them
  // was being tested by hand.
  //
  // `docs/148` §9 item 5 records the state this replaces: the whole suite ran a
  // single `Desktop Chrome` project, five specs set a narrow viewport by hand,
  // and exactly ONE — `touch-targets.spec.mjs` — turned on touch emulation. So
  // every phone assertion was really an assertion about a *narrow desktop
  // window*: a fine pointer, a 1x backing store, no `isMobile`, and therefore no
  // meta-viewport processing at all. The two things the phone tier is built on —
  // `interactive-widget=resizes-content` and `viewport-fit=cover` — are meta
  // viewport directives, and a project without `isMobile` does not parse them.
  // Testing the phone rung in that project was testing the one thing it is not.
  //
  // The split is by FILENAME, and deliberately so rather than by `test.use`:
  //
  //   * `phone-*.spec.mjs` runs ONLY in the `phone` project, so a phone spec
  //     cannot be written that silently runs as a desktop. `testIgnore` on
  //     `chromium` is the other half — without it every phone spec would run
  //     twice and the suite would pay for a device class it is not testing.
  //   * everything else runs ONLY in `chromium`. `editor-narrow-chrome`,
  //     `responsive-shell` and `narrow-review-column` stay there on purpose:
  //     they test the 620px and 700px RUNGS, which a laptop reaches by being
  //     narrowed, and a rung is not a device. `touch-targets.spec.mjs` also
  //     stays, because its whole subject is that a coarse pointer can arrive at
  //     any width — it opts into touch with `test.use` and says so.
  //
  // Pixel 7 rather than an iPhone: Playwright's iPhone descriptors carry
  // `defaultBrowserType: "webkit"`, and this suite has no WebKit browser
  // installed, so an iPhone project would be a project that never runs. Pixel 7
  // is a Chromium descriptor at 412x915 with `isMobile`, `hasTouch` and a 2.625
  // device scale factor — a real phone's pixel budget, which is also what makes
  // the raster cost honest. Specs that care about an exact width still call
  // `setViewportSize` (390 and 320 are both narrower than the descriptor), and
  // that override keeps `isMobile` and `hasTouch`.
  //
  // What this still does NOT give us is a soft keyboard: Chromium's emulation
  // has none, so `visualViewport` never shrinks and the keyboard-inset path
  // remains reasoned rather than measured (`docs/148` §9 item 6). Naming that
  // here so the project is not read as evidence it covers it.
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
      testIgnore: /phone-.*\.spec\.mjs$/,
    },
    {
      name: "phone",
      use: { ...devices["Pixel 7"] },
      testMatch: /phone-.*\.spec\.mjs$/,
    },
  ],
  webServer: {
    command: `python3 serve.py ${port}`,
    url: `${baseURL}/`,
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
