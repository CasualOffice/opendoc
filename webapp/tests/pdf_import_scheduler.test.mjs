// The staged PDF import's scheduler (`docs/182` §6, P2): bounded slices,
// progress, Cancel between pages, and no livelock on a page heavier than the
// whole budget. A fake job and a fake clock, so every number here is exact.
import assert from "node:assert/strict";
import test from "node:test";
import {
  SLICE_BUDGET_MS,
  idleSlices,
  isPdfBytes,
  runStagedImport,
} from "../src/pdf_import_scheduler.mjs";

/** A job of `pages` pages, page i costing `cost(i)` ms on the fake clock. */
function fakeJob(pages, cost, clock) {
  return {
    pagesDone: 0,
    pageCount: pages,
    cancelledCalls: 0,
    steps: [],
    step(budget) {
      this.steps.push(budget);
      if (this.cancelledCalls) return "cancelled";
      clock.t += cost(this.pagesDone);
      this.pagesDone += 1;
      return this.pagesDone >= pages ? "done" : "pending";
    },
    cancel() {
      this.cancelledCalls += 1;
    },
  };
}

/** Runs slices synchronously through a queue, recording each slice's span. */
function harness({ cancelAfterSlices = Infinity } = {}) {
  const clock = { t: 0 };
  const queue = [];
  const spans = [];
  let slices = 0;
  return {
    clock,
    spans,
    io: {
      now: () => clock.t,
      cancelled: () => slices > cancelAfterSlices,
      nextSlice: (slice) =>
        queue.push(() => {
          slices += 1;
          const start = clock.t;
          slice(SLICE_BUDGET_MS);
          spans.push(clock.t - start);
        }),
    },
    drain() {
      while (queue.length) queue.shift()();
    },
  };
}

async function run(job, h, onProgress) {
  const pending = runStagedImport(job, { ...h.io, onProgress });
  h.drain();
  return pending;
}

test("every page is imported, one page per step, and no slice overruns its budget", async () => {
  const h = harness();
  const job = fakeJob(100, () => 7, h.clock);
  const progress = [];
  const result = await run(job, h, (done, total) => progress.push([done, total]));
  assert.equal(result.status, "done");
  assert.equal(job.pagesDone, 100);
  assert.ok(job.steps.every((budget) => budget === 1), `steps ${job.steps}`);
  const worst = Math.max(...h.spans);
  assert.ok(worst <= SLICE_BUDGET_MS, `a slice took ${worst} ms against ${SLICE_BUDGET_MS}`);
  // 7 ms pages under a 40 ms budget: five per slice, so twenty slices, not a
  // hundred (one page per task would be needlessly slow) and not one.
  assert.equal(result.slices, 20);
  // Progress is reported after every slice, monotonically, ending at the total.
  assert.deepEqual(progress[0], [0, 100]);
  assert.deepEqual(progress.at(-1), [100, 100]);
  for (let i = 1; i < progress.length; i += 1) assert.ok(progress[i][0] >= progress[i - 1][0]);
});

test("a page heavier than the whole budget is still imported, and later slices hold one page each", async () => {
  const h = harness();
  const job = fakeJob(6, (i) => (i === 2 ? 120 : 5), h.clock);
  const result = await run(job, h);
  assert.equal(result.status, "done");
  assert.equal(job.pagesDone, 6);
  // A heavy page nobody has seen yet cannot be predicted: its slice is the
  // pages before it plus itself, so at most one budget over the page's own
  // cost. After it the slowest page is 120 ms, so every later slice does
  // exactly one page.
  assert.ok(Math.max(...h.spans) <= 120 + SLICE_BUDGET_MS, `spans ${h.spans}`);
  assert.deepEqual(h.spans.slice(-3), [5, 5, 5]);
});

test("Cancel between slices stops the job before the next page and imports nothing more", async () => {
  const h = harness({ cancelAfterSlices: 2 });
  const job = fakeJob(100, () => 7, h.clock);
  const result = await run(job, h);
  assert.equal(result.status, "cancelled");
  assert.equal(job.cancelledCalls, 1, "the engine job is told to cancel");
  assert.equal(job.pagesDone, 10, "two slices of five pages, then nothing");
});

test("a failed page ends the run as failed", async () => {
  const h = harness();
  const job = fakeJob(10, () => 1, h.clock);
  const step = job.step.bind(job);
  job.step = (budget) => (job.pagesDone === 3 ? "failed" : step(budget));
  const result = await run(job, h);
  assert.equal(result.status, "failed");
});

test("the browser seam uses an idle callback with a timeout, and a task where there is none", () => {
  const calls = [];
  const withIdle = {
    requestIdleCallback: (cb, options) => {
      calls.push(options);
      cb({ didTimeout: false, timeRemaining: () => 49 });
    },
  };
  let budget = null;
  idleSlices(withIdle)((ms) => {
    budget = ms;
  });
  assert.deepEqual(calls, [{ timeout: 100 }]);
  assert.equal(budget, SLICE_BUDGET_MS, "never more than the slice budget, whatever idle time is offered");
  const withoutIdle = { setTimeout: (cb, delay) => (calls.push(delay), cb()) };
  idleSlices(withoutIdle)((ms) => {
    budget = ms;
  });
  assert.equal(calls.at(-1), 0);
  assert.equal(budget, SLICE_BUDGET_MS);
});

test("PDF bytes are told by their header, not their name", () => {
  assert.equal(isPdfBytes(new TextEncoder().encode("%PDF-1.7\n")), true);
  assert.equal(isPdfBytes(new TextEncoder().encode("PK\u0003\u0004")), false);
  assert.equal(isPdfBytes(new TextEncoder().encode("%PDF")), false);
  assert.equal(isPdfBytes(null), false);
});

test("a PDF whose header is not at byte 0 is still a PDF, as ISO 32000 allows", () => {
  const enc = (text) => new TextEncoder().encode(text);
  // A BOM, whitespace and mailer junk before the header: real files carry these.
  assert.equal(isPdfBytes(enc("﻿%PDF-1.7\n")), true);
  assert.equal(isPdfBytes(enc("\r\n  junk from a mailer\r\n%PDF-1.4\n")), true);
  // Past the first 1024 bytes the header no longer counts, and a DOCX is never a PDF.
  assert.equal(isPdfBytes(enc(`${" ".repeat(1100)}%PDF-1.4`)), false);
  assert.equal(isPdfBytes(enc("PK\u0003\u0004word/document.xml")), false);
  assert.equal(isPdfBytes(new Uint8Array()), false);
});
