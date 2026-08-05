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
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: `python3 serve.py ${port}`,
    url: `${baseURL}/`,
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
