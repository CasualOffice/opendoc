// Deciding a tracked change: the chrome asks the host contract (HF-199).
//
// The API required `edit` for every `review.accept*` / `review.reject*` row
// while the chrome let a `commentor` decide. These hold the chrome to the
// contract's answer, read from the contract rather than restated.
import { test } from "node:test";
import assert from "node:assert/strict";

import { resolveCapabilities } from "../src/capabilities.mjs";
import { COMMAND_CONTRACT, grantsRequirement } from "../src/host_contract.mjs";
import {
  isReviewDecision,
  mayDecideReview,
  narrowDecisionsToContainer,
  reviewDecisionRefusal,
} from "../src/review_authority.mjs";

const commentor = resolveCapabilities({ mode: "commentor" });
const editor = resolveCapabilities({ mode: "edit" });

test("the decision ids are the accept and reject verbs, on a camel or dotted boundary", () => {
  for (const id of ["review.accept", "review.reject", "review.acceptAll", "review.rejectNext", "review.acceptAtCaret"]) {
    assert.equal(isReviewDecision(id), true, id);
  }
  for (const id of ["review.acceptance", "review.comment", "review.toggle", "edit.accept", "", null]) {
    assert.equal(isReviewDecision(id), false, String(id));
  }
});

test("every decision row the contract declares is refused to a commentor and admitted to an editor", () => {
  const decisions = COMMAND_CONTRACT.filter((row) => isReviewDecision(row.id));
  assert.ok(decisions.length >= 6, "the contract declares the six decision rows");
  for (const row of decisions) {
    // The chrome's answer is the contract's answer, whatever the row requires.
    assert.equal(reviewDecisionRefusal(row.id, commentor) !== "", !grantsRequirement(row.requires, commentor), row.id);
    assert.equal(reviewDecisionRefusal(row.id, commentor) !== "", true, `${row.id} for a commentor`);
    assert.equal(reviewDecisionRefusal(row.id, editor), "", `${row.id} for an editor`);
  }
  // The context menu's own ids, which are not contract rows, follow the same rule.
  assert.notEqual(reviewDecisionRefusal("review.accept", commentor), "");
  assert.equal(reviewDecisionRefusal("review.reject", editor), "");
});

test("a commentor still comments: only decisions are narrowed", () => {
  const rows = [
    { id: "review.comment", enabled: true },
    { id: "review.acceptAll", enabled: true },
    { id: "format.bold", enabled: true },
  ];
  const narrowed = narrowDecisionsToContainer(rows, commentor);
  assert.equal(narrowed[0], rows[0]);
  assert.equal(narrowed[2], rows[2]);
  assert.equal(narrowed[1].enabled, false);
  assert.notEqual(narrowed[1].disabledReason, "");
  assert.equal(rows[1].enabled, true, "the input is not mutated");
  assert.deepEqual(narrowDecisionsToContainer(rows, editor), rows);
});

test("the choke point refuses with the reason said, and lets an editor through", () => {
  const said = [];
  assert.equal(mayDecideReview(commentor, (text, kind) => said.push([text, kind])), false);
  assert.equal(said.length, 1);
  assert.equal(said[0][1], "error");
  assert.notEqual(said[0][0], "");
  assert.equal(mayDecideReview(editor, () => said.push("unexpected")), true);
  assert.equal(said.length, 1);
});
