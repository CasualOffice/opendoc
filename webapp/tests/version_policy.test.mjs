// What the version timeline says — `version_policy.mjs` (docs/139, docs/140).
//
// The store returns CODES and no English, which is the right split and also a
// hazard: the moment one code has no sentence, a reader gets `history.pinLimit`
// in their status bar, or — worse — the generic fallback, which is a specific
// refusal quietly turned into an unhelpful one. Half of this file exists to make
// that unrepresentable rather than merely discouraged.
//
// Every test here was driven RED before it was trusted. The mutations and their
// output are in the commit message; each test's comment says what mutation it
// catches, because a guard whose failure mode nobody wrote down is a guard
// nobody can tell is still working.
import assert from "node:assert/strict";
import test from "node:test";

import { EN_STRINGS } from "../src/en_strings.mjs";
import { resetI18n, setCatalogue, setLocale } from "../src/i18n.mjs";
import {
  HISTORY_STATUS,
  VERSION_KIND,
  historyStatusCodes,
  isHistoryRefusal,
  resolveRetention,
} from "../src/version_history.mjs";
import {
  dayLabel,
  describedKinds,
  describedStatusCodes,
  groupVersions,
  historyMessage,
  historyMessageKind,
  historyStatusKeys,
  historyUnavailableReason,
  retentionSummary,
  versionKindLabel,
  versionRowText,
} from "../src/version_policy.mjs";

resetI18n();
setCatalogue("en", EN_STRINGS);
setLocale("en");

/** A version row, as the store writes one. Only the fields the words read. */
const row = (overrides = {}) => ({
  versionId: "ver-1",
  createdAt: Date.UTC(2026, 8, 28, 12, 0, 0),
  kind: VERSION_KIND.AUTO,
  name: "",
  pinned: false,
  bytes: 16_095,
  ...overrides,
});

// ── One sentence per code ────────────────────────────────────────────────────

test("every status code the store can return has a sentence of its own", () => {
  // Catches: deleting an entry from `STATUS_TEXT`, or adding a code to
  // `HISTORY_STATUS` without wording it. Both are the same defect — a refusal
  // that falls through to the generic sentence — and the second is the one that
  // happens by accident.
  const described = new Set(describedStatusCodes());
  const missing = historyStatusCodes().filter((code) => !described.has(code));
  assert.deepEqual(missing, [], "these codes would fall through to the generic refusal");
  const extra = [...described].filter((code) => !historyStatusCodes().includes(code));
  assert.deepEqual(extra, [], "these described codes are not codes the store returns");
});

test("every status sentence is declared in the catalogue, so none renders as a key", () => {
  // Catches: a `t("history.status.…")` key that `en_strings.mjs` does not carry.
  // `t()` returns the KEY for an unknown one, which is visible in a screenshot
  // and invisible in a diff; this is the test that reads it.
  const undeclared = historyStatusKeys().filter((key) => !Object.hasOwn(EN_STRINGS, key));
  assert.deepEqual(undeclared, []);
  for (const code of historyStatusCodes()) {
    const text = historyMessage(code);
    assert.ok(text.length > 12, `${code} has no real sentence: ${JSON.stringify(text)}`);
    // The failure this catches is the one that matters: a raw code reaching a
    // reader. A dotted identifier with no space in it is what that looks like.
    assert.ok(!/^[a-z]+(\.[a-zA-Z]+)+$/.test(text), `${code} renders as a key: ${text}`);
    assert.ok(text !== historyMessage("history.noSuchCodeAtAll"), `${code} uses the fallback`);
  }
});

test("a code nobody described falls back to a refusal, never to silence", () => {
  // Deliberate: the cost of an unnecessary toast is small, and the cost of
  // silence about somebody's lost work is not. Catches a fallback changed to ""
  // or to a confirmation.
  const fallback = historyMessage("history.somethingNobodyWroteDown");
  assert.ok(fallback.length > 12);
  assert.equal(historyMessageKind("history.somethingNobodyWroteDown"), "error");
});

test("a refusal is published as an error and a confirmation is not", () => {
  // Catches: inverting `historyMessageKind`, which would route a quota failure
  // to the polite live region and the footer pill — the pill being the first
  // thing a narrow window sheds, so the message would be invisible AND
  // unspeakable at 390px. That exact failure is why `status_channel.mjs` exists.
  let refusals = 0;
  for (const code of historyStatusCodes()) {
    const kind = historyMessageKind(code);
    if (isHistoryRefusal(code)) {
      refusals += 1;
      assert.equal(kind, "error", `${code} is a refusal but publishes as "${kind}"`);
    } else {
      assert.equal(kind, "", `${code} is not a refusal but publishes as "${kind}"`);
    }
  }
  // The half that fails when the guard breaks rather than when the code does: a
  // classifier that called nothing a refusal would satisfy the loop above.
  assert.ok(refusals >= 10, `only ${refusals} refusals found, so the loop proves little`);
});

test("naming is described as a promise, because the store keeps it as one", () => {
  // `planRetention` never prunes a pinned row: when a ceiling cannot be met
  // without deleting one, the capture is REFUSED with `fullPinned`. A UI that
  // implied naming were best-effort would be wrong about the store, so the two
  // sentences that carry that promise are checked rather than trusted.
  assert.match(historyMessage(HISTORY_STATUS.FULL_PINNED), /named/i);
  assert.match(EN_STRINGS["versionHistory.retention"], /[Nn]amed versions are kept until/);
});

test("every version kind has a word a reader recognises", () => {
  // Catches: a kind added to the store with no label, which would render as the
  // generic "Autosaved" and quietly mislabel a restore.
  const described = new Set(describedKinds());
  const missing = Object.values(VERSION_KIND).filter((kind) => !described.has(kind));
  assert.deepEqual(missing, []);
  const labels = Object.values(VERSION_KIND).map((kind) => versionKindLabel(kind));
  assert.equal(new Set(labels).size, labels.length, "two kinds read as the same word");
  // An unrecognised kind is an ordinary automatic version rather than an empty
  // cell: a stored row is untrusted input on reopen (`docs/140` §14).
  assert.equal(versionKindLabel("whatever-a-host-wrote"), versionKindLabel(VERSION_KIND.AUTO));
});

// ── Grouping ────────────────────────────────────────────────────────────────

test("grouping is by LOCAL day, so a version written at 23:30 is today's", () => {
  // Catches: grouping on the UTC date. Everywhere east or west of UTC that puts
  // a late-evening or early-morning version under the wrong heading, which is
  // the one thing a timeline must not get wrong. Built from local-time
  // constructors so the test says the same thing in every time zone.
  const now = new Date(2026, 8, 28, 14, 0, 0).getTime();
  const late = new Date(2026, 8, 28, 23, 30, 0).getTime();
  const early = new Date(2026, 8, 28, 0, 15, 0).getTime();
  const groups = groupVersions(
    [row({ versionId: "a", createdAt: late }), row({ versionId: "b", createdAt: early })],
    { now },
  );
  assert.equal(groups.length, 1, "two versions on the same local day made two groups");
  assert.equal(groups[0].label, "Today");
  assert.deepEqual(groups[0].rows.map((r) => r.versionId), ["a", "b"]);
});

test("yesterday is named, and older days carry their date", () => {
  const now = new Date(2026, 8, 28, 14, 0, 0).getTime();
  assert.equal(dayLabel(new Date(2026, 8, 27, 9, 0, 0).getTime(), now), "Yesterday");
  const older = dayLabel(new Date(2026, 8, 20, 9, 0, 0).getTime(), now);
  assert.ok(!/yesterday|today/i.test(older), `a week ago read as "${older}"`);
  // `docs/139` §14: a relative label never REPLACES the date. Four days ago must
  // read as a date, not as "4 days ago".
  assert.match(older, /2026/);
});

test("grouping loses nothing: every version comes back in exactly one group", () => {
  // Catches: grouping that drops a row. `docs/139` §8.2 — grouping is
  // presentation and never deletes a commit.
  const now = Date.now();
  const day = 24 * 60 * 60 * 1000;
  const rows = Array.from({ length: 9 }, (_, i) =>
    row({ versionId: `ver-${i}`, createdAt: now - i * (day / 2) }),
  );
  const groups = groupVersions(rows, { now });
  const seen = groups.flatMap((group) => group.rows.map((r) => r.versionId));
  assert.equal(seen.length, rows.length);
  assert.equal(new Set(seen).size, rows.length);
});

test("the list is newest first, within groups and across them", () => {
  const now = Date.now();
  const rows = [
    row({ versionId: "old", createdAt: now - 400_000 }),
    row({ versionId: "new", createdAt: now - 1_000 }),
    row({ versionId: "mid", createdAt: now - 60_000 }),
  ];
  const order = groupVersions(rows, { now }).flatMap((g) => g.rows.map((r) => r.versionId));
  assert.deepEqual(order, ["new", "mid", "old"]);
});

test("the named-only filter hides rows and never claims the timeline is empty", () => {
  // Catches: a filter that also empties `rows`, and the subtler defect the panel
  // guards against — reporting a FILTERED empty list as "no versions yet" over a
  // document that has nine.
  const now = Date.now();
  const rows = [row({ versionId: "a" }), row({ versionId: "b", name: "Before the review" })];
  const all = groupVersions(rows, { now });
  const named = groupVersions(rows, { now, namedOnly: true });
  assert.equal(all.flatMap((g) => g.rows).length, 2);
  assert.deepEqual(named.flatMap((g) => g.rows).map((r) => r.versionId), ["b"]);
  assert.equal(rows.length, 2, "the filter mutated its input");
});

// ── A row's words ───────────────────────────────────────────────────────────

test("a row carries the exact instant as well as the friendly one", () => {
  // `docs/139` §14: dates use locale formatting but expose an exact
  // machine-readable timestamp, and a relative label never replaces it. Catches
  // dropping the `<time datetime>` value.
  const at = Date.UTC(2026, 8, 28, 9, 41, 30);
  const text = versionRowText(row({ createdAt: at }));
  assert.equal(text.exact, new Date(at).toISOString());
  assert.ok(text.timestamp.length > text.clock.length, "the full timestamp is not fuller");
  assert.match(text.timestamp, /2026/);
});

test("a named version is titled by its name; an unnamed one by its time", () => {
  const at = Date.UTC(2026, 8, 28, 9, 41, 30);
  assert.equal(versionRowText(row({ createdAt: at, name: "Before legal" })).title, "Before legal");
  const plain = versionRowText(row({ createdAt: at }));
  assert.equal(plain.title, plain.clock);
  assert.ok(plain.clock.length > 0);
});

test("a screen reader hears which row it is, not just a time", () => {
  // Catches: an accessible name that is only the visible title. "14:32" tells a
  // non-sighted reader nothing about which of nine rows they are on, so the
  // label has to carry the full timestamp, the origin, and the two states.
  const at = Date.UTC(2026, 8, 28, 9, 41, 30);
  const text = versionRowText(row({ createdAt: at, name: "Before legal", kind: VERSION_KIND.SAVED }), {
    isHead: true,
  });
  assert.ok(text.label.includes("Before legal"));
  assert.ok(text.label.includes(text.timestamp), "the label omits the exact timestamp");
  assert.ok(text.label.includes(versionKindLabel(VERSION_KIND.SAVED)), "the label omits the origin");
  assert.ok(text.label.includes("Current version"), "the head does not say so");
  assert.ok(text.label.includes("Named"), "a named version does not say so");
});

// ── Disclosure ──────────────────────────────────────────────────────────────

test("the disclosure states the counts, the size and the policy", () => {
  // `docs/139` §12 asks for this by name: a person who cannot see the bound
  // cannot trust the promise. Catches a summary that stopped naming the days or
  // the ceiling.
  const retention = resolveRetention({});
  const words = retentionSummary(
    { versions: 7, bytes: 2_400_000, pinned: 2 },
    retention,
    (bytes) => `${Math.round(bytes / 1024)} KB`,
  );
  assert.match(words.kept, /7 versions/);
  assert.match(words.detail, /2344 KB/);
  assert.match(words.detail, /2 of 15/);
  assert.match(words.policy, /7 days/);
  assert.match(words.policy, /25/);
  // One version is not "1 versions": the plural family is a family, not an "(s)".
  assert.match(retentionSummary({ versions: 1, bytes: 0, pinned: 0 }, retention, () => "0 B").kept, /1 version kept/);
});

// ── Never a dead control ────────────────────────────────────────────────────

const READY = Object.freeze({
  hasDocument: true,
  hostAllows: true,
  autosaveOn: true,
  historyOn: true,
  storeReason: "",
});

test("every way version history can be unavailable has its own sentence", () => {
  // SKILL §10 and `docs/139` §12: the entry ships disabled WITH A REASON, and a
  // single generic reason is its own defect because the way out of each of these
  // is different. Catches collapsing any two of them.
  const reasons = [
    historyUnavailableReason({ ...READY, hostAllows: false }),
    historyUnavailableReason({ ...READY, storeReason: "the browser said no" }),
    historyUnavailableReason({ ...READY, historyOn: false }),
    historyUnavailableReason({ ...READY, autosaveOn: false }),
    historyUnavailableReason({ ...READY, hasDocument: false }),
  ];
  for (const reason of reasons) assert.ok(reason.length > 12, `weak reason: ${reason}`);
  assert.equal(new Set(reasons).size, reasons.length, "two states give the same reason");
  assert.equal(historyUnavailableReason(READY), "", "a ready editor still reports a reason");
});

test("the most specific truth wins, and the host's outranks the user's", () => {
  // A framed editor that has also turned the preference off must read as the
  // HOST's decision: that is the one the reader cannot change from here, and
  // sending them to Settings to fix something Settings cannot fix is worse than
  // saying nothing. Catches reordering the cascade.
  const both = historyUnavailableReason({ ...READY, hostAllows: false, historyOn: false, autosaveOn: false });
  assert.equal(both, historyUnavailableReason({ ...READY, hostAllows: false }));
  // A refused store outranks a preference for the same reason.
  const store = historyUnavailableReason({ ...READY, storeReason: "no room", historyOn: false });
  assert.equal(store, "no room");
});

test("an absent state object is unavailable, not available", () => {
  // Fail closed. A caller that has resolved nothing yet must get a reason, not
  // an empty string that reads as "everything is fine".
  assert.ok(historyUnavailableReason(undefined).length > 12);
  assert.ok(historyUnavailableReason({}).length > 12);
});
