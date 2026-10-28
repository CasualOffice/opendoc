// SPDX-License-Identifier: Apache-2.0

// The editor's address options are matched without regard to the NAME's case
// (owner, 2026-10-29): `?aitools=webmcp` was silently ignored, and a person typing
// an address cannot see why. Values stay verbatim; the enumerated opt-in values
// are compared case-insensitively where they are read.

import { test } from "node:test";
import assert from "node:assert/strict";
import { hostConfig, hostParams } from "../src/capabilities.mjs";
import { startWebMcp } from "../src/ai_webmcp.mjs";

const view = (search) => ({ location: { search }, self: 1, top: 1 });

test("option names match in any case, and the first of a name wins", () => {
  assert.equal(hostConfig(view("?aitools=webmcp")).aiTools, "webmcp");
  assert.equal(hostConfig(view("?AITOOLS=webmcp")).aiTools, "webmcp");
  assert.equal(hostConfig(view("?MODE=readonly&Room=wss://r/x")).mode, "readonly");
  assert.equal(hostConfig(view("?Room=wss://r/X")).room, "wss://r/X", "values stay verbatim");
  assert.equal(hostParams("?Fixture=rich&fixture=styled").get("fixture"), "rich");
});

test("a WebMCP opt-in written in any case turns WebMCP on", () => {
  for (const search of ["?aiTools=webmcp", "?aitools=WebMCP", "?AITOOLS=WEBMCP"]) {
    const status = startWebMcp({}, hostConfig(view(search)), {});
    assert.notEqual(status().reason, "not-requested", search);
  }
  assert.equal(startWebMcp({}, hostConfig(view("?other=1")), {})().reason, "not-requested");
});
