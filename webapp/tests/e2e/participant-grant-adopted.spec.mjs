// A room's grant actually reaches the engine.
//
// `adoptParticipantIdentity` had never succeeded. `session_access.mjs` passed a
// JavaScript **Number** to a Rust `u64`, and wasm-bindgen maps `u64` to
// **BigInt** — the generated typings say so outright
// (`webapp/pkg/casual_doc_wasm.d.ts:1068`:
// `adoptParticipantIdentity(participant: bigint): void`). Measured in the
// browser against a real opened document, by calling the method directly:
//
//     adoptParticipantIdentity(1)  →  TypeError: Cannot convert 1 to a BigInt
//     adoptParticipantIdentity(0)  →  the same
//     adoptParticipantCapabilities([...five valid names])  →  fine
//
// `SESSION.adopt` wraps both calls in one `try`, so the throw was caught and
// turned into `GRANT_UNREADABLE`: every shared document opened telling the
// reader "This session's permissions could not be read, so the document is open
// read-only", in red, while the chrome carried on offering every control the
// grant named. Two authorities and one of them wrong.
//
// Worse, the identity call is FIRST, so the capabilities call never ran either:
// the engine kept every capability it had. `session_access.mjs`'s own comment
// names that consequence precisely — "A refusal here is not a security hole —
// the relay keeps its own copy and is the authority — but it IS a lie: the
// chrome would be disabling controls the engine still permits."
//
// And it was INVISIBLE, because the render's status line used to wipe the error
// a moment after it appeared. It surfaced only once background progress stopped
// painting over a reader's message (`status-line-ownership.spec.mjs`), which is
// that change's argument in one example.
//
// This spec is deliberately a BROWSER spec and not only a unit test. The unit
// test for `adopt` has always been green while the real call threw, because it
// drove a hand-written fake that accepted anything — SKILL §4's "a test against
// an invented shape would not notice if it stopped matching". Only the real
// engine can answer whether the grant was adopted.
import { test, expect } from "./fixtures.mjs";

/** The five capabilities the roster-driven dialog specs ask for. */
const GRANT = "&granted=comment,edit,manageAccess,review,suggest&participant=1";

/** Readiness WITHOUT reading the status line, because the status line is what is
 *  under test here: an assertion that waits for the line to be empty cannot then
 *  discover that it is not. */
async function openGranted(page, query) {
  await page.goto(`/editor.html?fixture=rich${query}`);
  await page.waitForFunction(
    () =>
      document.querySelectorAll(".page-wrap").length > 0 &&
      document.body.dataset.fontsReady === "true",
    null,
    { timeout: 45_000 },
  );
}

test("a participant grant is adopted instead of being reported unreadable", async ({
  page,
  consoleErrors,
}) => {
  await openGranted(page, GRANT);

  const status = await page.locator("#status").textContent();
  const className = await page.locator("#status").getAttribute("class");

  expect(
    status ?? "",
    "the engine refused the grant, so the reader is told the document is read-only",
  ).not.toMatch(/permissions could not be read/i);
  expect(className ?? "", `the status line is in the error state reading ${JSON.stringify(status)}`)
    .not.toMatch(/\berror\b/);

  expect(consoleErrors).toEqual([]);
});

test("participant zero is adopted too, not treated as no room at all", async ({
  page,
  consoleErrors,
}) => {
  // Zero is a legitimate participant number — `parseParticipant` accepts any
  // non-negative safe integer, and `participantGrant` is explicit that testing
  // the parsed number instead of the raw input would read a malformed
  // participant as "no room at all and therefore every capability". It is also
  // the number `modal-roster.mjs` uses, so eight dialog specs depended on it.
  await openGranted(page, "&granted=comment,edit,manageAccess,review,suggest&participant=0");

  const status = await page.locator("#status").textContent();
  expect(status ?? "").not.toMatch(/permissions could not be read/i);

  expect(consoleErrors).toEqual([]);
});
