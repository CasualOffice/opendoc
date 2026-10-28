// SPDX-License-Identifier: Apache-2.0

// The editor page carries Chrome's WebMCP origin-trial token (`docs/178` M4). A
// token silently stops working when it expires or names another origin, and the
// only symptom is that `document.modelContext` is absent — the editor then
// reports `unavailable` and nobody notices. So the token's own claims are checked
// against the deployment: the feature, the origin in `CNAME`, subdomain and
// third-party use (an embedded editor is a third-party frame), and the expiry.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const html = readFileSync(new URL("../editor.html", import.meta.url), "utf8");
const host = readFileSync(new URL("../CNAME", import.meta.url), "utf8").trim();

/** The decoded JSON claims of every origin-trial token on the page. */
function trialClaims() {
  return [...html.matchAll(/<meta\s+http-equiv="origin-trial"\s+content="([^"]+)"/g)].map(
    ([, token]) => {
      const bytes = Buffer.from(token, "base64");
      // Version byte, 64-byte signature, 4-byte big-endian payload length, payload.
      const length = bytes.readUInt32BE(65);
      return JSON.parse(bytes.subarray(69, 69 + length).toString("utf8"));
    },
  );
}

test("the editor carries a WebMCP origin-trial token for the deployed origin", () => {
  const webmcp = trialClaims().filter((claims) => claims.feature === "WebMCP");
  assert.equal(webmcp.length, 1, "exactly one WebMCP token on editor.html");
  const [claims] = webmcp;
  assert.equal(claims.origin, `https://${host}:443`, "the token names the CNAME origin");
  assert.equal(claims.isSubdomain, true, "subdomains are covered");
  assert.equal(claims.isThirdParty, true, "an embedded (third-party) editor frame is covered");
});

test("the WebMCP origin-trial token has not expired", () => {
  const [claims] = trialClaims().filter((c) => c.feature === "WebMCP");
  const expires = new Date(claims.expiry * 1000);
  assert.ok(
    expires.getTime() > Date.now(),
    `the WebMCP origin trial token expired on ${expires.toISOString().slice(0, 10)}; ` +
      "renew it at the Chrome origin-trials console and replace it in editor.html",
  );
});
