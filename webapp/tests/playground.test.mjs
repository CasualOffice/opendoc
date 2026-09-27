// The playground's page, checked against the authority it claims to be generated
// from — and its refusal, checked against the command line it claims to quote.
//
// `docs/99` §9.1: a published number is generated from a committed artifact or it
// is not published. `build-embed-docs.mjs` generates this page, so the obvious
// test — call the generator and compare — would prove nothing at all: it would
// compare the generator with itself. Everything below re-derives from
// `capabilities.mjs` and reads the COMMITTED page, so a bug in the generator
// fails here rather than being confirmed here.
//
// And the sharper half, which is `docs/126` phase 3's own recorded mistake:
// deleting the brand generator's `throw` left all eighteen of `brand.test.mjs`'s
// assertions green, because they called the audit helper instead of the real CLI —
// "the door was open with a lock behind it". This page publishes a refusal, so the
// guard here drives `node webapp/tools/build-brand.mjs` and reads its real exit
// status and its real stderr.
import assert from "node:assert/strict";
import test from "node:test";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { CAPABILITIES, REGIONS, ROLES, resolveCapabilities, resolveRegions } from "../src/capabilities.mjs";
import { BrandRefusal, auditBrand, normalize } from "../src/brand_contract.mjs";
import { TEXT_CONTRAST_FLOOR, UI_CONTRAST_FLOOR } from "../src/contrast.mjs";
import { parsePalettes } from "../src/palette_parse.mjs";
import { OVERRIDABLE } from "../tools/build-brand.mjs";
import { scanScript } from "../tools/string_sites.mjs";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const REPO = join(WEBAPP, "..");
const read = (name) => readFileSync(join(WEBAPP, name), "utf8");

const PAGE = read("playground.page.html");
const MODULE = read("src/playground.js");
const THEMES = parsePalettes(read("src/style.css")).themes;

/** Every `value="…"` carried by an element with the given data attribute. */
function controlValues(attribute) {
  return [...PAGE.matchAll(new RegExp(`<input[^>]*\\bvalue="([^"]*)"[^>]*\\b${attribute}\\b`, "g"))].map(
    (match) => match[1],
  );
}

/** A `data-claim` value as the committed page publishes it. */
function claim(name) {
  const found = [...PAGE.matchAll(new RegExp(`data-claim="${name}"[^>]*>([^<]*)<`, "g"))].map((m) =>
    m[1].trim(),
  );
  assert.ok(found.length, `the page publishes no data-claim="${name}"`);
  assert.equal(
    new Set(found).size,
    1,
    `the page publishes data-claim="${name}" with more than one value: ${[...new Set(found)].join(", ")}`,
  );
  return found[0];
}

// ---- The controls are the authority's, not a list somebody typed -------------

test("every role, capability and region has a control, and nothing else does", () => {
  // BOTH DIRECTIONS. A missing control is a configuration a host cannot reach from
  // the page that exists to teach it; a control for something the authority does
  // not declare is a promise the editor will not keep. The page is generated, so
  // the day a tenth capability lands this is what says the page moved with it.
  assert.deepEqual(controlValues("data-role"), [...ROLES]);
  assert.deepEqual(controlValues("data-capability"), [...CAPABILITIES]);
  assert.deepEqual(controlValues("data-region"), [...REGIONS]);
});

test("every switch is labelled, and the label points at the control", () => {
  // An `<input>` with no `<label for>` has no accessible name, and a checkbox with
  // no name is a checkbox a screen reader reads as "checkbox". Thirty-three of them
  // would be a page nobody could use without a mouse.
  // Both halves, because the first version of this counted `id="pg-…"` inputs and
  // asserted the total was 33 — which passed only while the brand fields happened
  // to be named in camelCase, and went red the moment they were renamed to match
  // their neighbours. A count is not the guarantee; being labelled is.
  const switches = [...PAGE.matchAll(/<input[^>]*\bid="(pg-[\w.-]+)"[^>]*\bdata-(?:role|capability|region)\b/g)]
    .map((m) => m[1]);
  assert.equal(
    switches.length,
    ROLES.length + CAPABILITIES.length + REGIONS.length,
    "the page does not carry one switch per role, capability and region",
  );
  const named = [...PAGE.matchAll(/<(?:input|select)[^>]*\bid="(pg-[\w.-]+)"[^>]*>/g)].map(
    (m) => m[1],
  );
  assert.ok(named.length >= switches.length, "no controls found at all");
  for (const id of named) {
    const labelled =
      PAGE.includes(`for="${id}"`) ||
      new RegExp(`<(?:input|select)[^>]*\\bid="${id}"[^>]*aria-label="[^"]+"`).test(PAGE);
    assert.ok(labelled, `#${id} has neither a <label for> nor an aria-label`);
  }
});

test("exactly one role is checked on arrival, and it is a role", () => {
  const checked = [...PAGE.matchAll(/<input[^>]*\bvalue="([^"]*)"[^>]*data-role checked/g)].map(
    (m) => m[1],
  );
  assert.equal(checked.length, 1, `${checked.length} roles are checked in the markup`);
  assert.ok(ROLES.includes(checked[0]), `the default role ${checked[0]} is not a role`);
});

test("a role's row says what it resolves to, and says it correctly", () => {
  // The clause on each radio ends with the grant, the region count and the review
  // mode. That is the page's most load-bearing claim — it is what a host compares
  // `preview` and `readonly` with before mounting anything — so it is re-derived
  // here rather than trusted.
  for (const role of ROLES) {
    const row = PAGE.match(
      new RegExp(`for="pg-role-${role}"[\\s\\S]*?<span class="pg-switch-what">([\\s\\S]*?)</span>`),
    );
    assert.ok(row, `no control row for the ${role} role`);
    const capabilities = resolveCapabilities({ mode: role, framed: true });
    const shown = resolveRegions({ mode: role, framed: true, capabilities });
    assert.ok(
      row[1].includes(`${shown.size} of ${REGIONS.length} regions`),
      `the ${role} row does not say it gets ${shown.size} of ${REGIONS.length} regions: ${row[1]}`,
    );
    for (const capability of capabilities) {
      assert.ok(
        row[1].includes(`<code>${capability}</code>`),
        `the ${role} row does not list its ${capability} grant`,
      );
    }
  }
});

test("the two reading roles are described as different, because they are", () => {
  // The owner's point, and the reason this page exists. `docs/126`: "`preview` is
  // not `readonly` minus print." A page that let them collapse would be teaching
  // the thing the container policy exists to prevent.
  const preview = resolveRegions({ mode: "preview", framed: true });
  const readonly = resolveRegions({ mode: "readonly", framed: true });
  assert.notDeepEqual([...preview].sort(), [...readonly].sort());
  assert.equal(preview.size, 0, "preview is supposed to be offered no region at all");
  assert.ok(readonly.size > 0, "readonly is supposed to keep reading chrome");
  assert.ok(
    PAGE.includes(`${preview.size} of ${REGIONS.length} regions`) &&
      PAGE.includes(`${readonly.size} of ${REGIONS.length} regions`),
    "the page does not publish both region counts, so a reader cannot see the difference",
  );
});

// ---- The numbers ------------------------------------------------------------

test("every number on the page is the authority's, re-derived here", () => {
  assert.equal(claim("role-count"), String(ROLES.length));
  assert.equal(claim("capability-count"), String(CAPABILITIES.length));
  assert.equal(claim("region-count"), String(REGIONS.length));
  assert.equal(claim("band-count"), String(REGIONS.filter((id) => id.startsWith("band.")).length));
  assert.equal(claim("overridable-token-count"), String(OVERRIDABLE.length));
  assert.equal(claim("text-contrast-floor"), String(TEXT_CONTRAST_FLOOR));
  assert.equal(claim("ui-contrast-floor"), String(UI_CONTRAST_FLOOR));
});

test("the URL parameters the page names are the ones the editor reads", () => {
  // The page's whole output is a URL, so this is its most load-bearing claim about
  // somebody else's code. Read out of `hostConfig()` independently of the
  // generator's own regex.
  const source = read("src/capabilities.mjs");
  const body = source.slice(source.indexOf("export function hostConfig("));
  const named = claim("url-parameters").split(" ");
  assert.ok(named.length >= 4, `the page names only ${named.length} URL parameters`);
  for (const name of named) {
    assert.ok(
      body.includes(`params?.get("${name}")`),
      `the page tells a host about ?${name}=, which hostConfig() never reads`,
    );
  }
});

test("the element attribute count is the element's own", () => {
  const element = read("src/embed_element.mjs");
  const table = element.match(/const MOUNT_ATTRIBUTES = Object\.freeze\(\[([^\]]*)\]\)/);
  assert.ok(table, "embed_element.mjs no longer declares MOUNT_ATTRIBUTES");
  const attributes = [...table[1].matchAll(/"([\w-]+)"/g)].map((m) => m[1]);
  assert.equal(claim("attribute-count"), String(attributes.length));
  // And the page's "does not do yet" bullet is still TRUE. Understating is as
  // false as overstating (`docs/99` §9.6): the day the element grows a `can`
  // attribute, that bullet is a lie and this is what says so.
  for (const name of ["can", "chrome"]) {
    assert.ok(
      !attributes.includes(name),
      `<opendoc-editor> now reads a "${name}" attribute, so the page's "no attribute of ` +
        `their own" bullet is false — say the new truth and generate the snippet with it`,
    );
  }
});

// ---- The refusal ------------------------------------------------------------

test("the accent the page offers as a failure really does fail", () => {
  // A "try a colour that fails" button whose colour has quietly started passing
  // would teach the opposite of the lesson, in the one place a reader is being
  // invited to press something.
  const accent = claim("failing-accent");
  assert.match(accent, /^#[0-9a-f]{6}$/i);
  const config = normalize(
    { version: 1, theme: { tokens: { "--accent": accent }, light: {}, dark: {} } },
    THEMES,
  );
  const { failures } = auditBrand(config, THEMES);
  assert.ok(failures.length, `${accent} passes the AA audit, so the button promises nothing`);
  assert.ok(
    failures.some((reason) => reason.includes(`must clear ${TEXT_CONTRAST_FLOOR}:1`)),
    `no failure names the text floor: ${failures.join(" | ")}`,
  );
});

test("the refusal the page shows is the one the command line gives, exit status and all", () => {
  // THE PHASE-3 LESSON, APPLIED. `docs/126`: deleting the generator's `throw` left
  // every assertion in `brand.test.mjs` green, because they called `auditBrand`
  // directly and never proved the GENERATOR refuses. So this drives the real
  // command with a real file and reads its real exit status — and then compares
  // what it printed with what `src/brand_contract.mjs` computes, which is the code
  // the playground runs in the browser. One message, two callers, proven.
  const accent = claim("failing-accent");
  const dir = mkdtempSync(join(tmpdir(), "opendoc-playground-"));
  const configPath = join(dir, "brand.json");
  writeFileSync(
    configPath,
    JSON.stringify({ version: 1, theme: { tokens: { "--accent": accent } } }, null, 2),
  );
  let status = 0;
  let output = "";
  try {
    execFileSync("node", [join(WEBAPP, "tools", "build-brand.mjs"), "--check", "--config", configPath], {
      cwd: REPO,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
  } catch (err) {
    status = err.status;
    output = `${err.stdout ?? ""}${err.stderr ?? ""}`;
  }
  assert.equal(status, 1, "the generator accepted a palette that fails AA");
  const config = normalize(
    { version: 1, theme: { tokens: { "--accent": accent }, light: {}, dark: {} } },
    THEMES,
  );
  const expected = new BrandRefusal(auditBrand(config, THEMES).failures).message;
  assert.equal(
    output.trim(),
    expected.trim(),
    "the command line says something the module the browser runs does not — which is the " +
      "two-implementations defect in the one place a host is told what to change",
  );
});

test("the playground writes no refusal sentence of its own", () => {
  // The shape guard behind the equality above. The page's refusal has to BE the
  // generator's, so the module imports it; a hand-written sentence here would be a
  // second version to keep in step, and the first thing to drift.
  assert.match(
    MODULE,
    /import \{[\s\S]*?\bauditBrand\b[\s\S]*?\} from "\.\/brand_contract\.mjs"/,
    "playground.js does not take the audit from the white-label contract",
  );
  for (const phrase of ["was refused", "must clear", "would pass"]) {
    assert.ok(
      !MODULE.includes(phrase),
      `playground.js contains "${phrase}", so it is writing its own version of the refusal`,
    );
  }
});

// ---- The module's own discipline --------------------------------------------

test("playground.js carries no English of its own", () => {
  // The page is a SITE template with no `t()` seam, so every word on it is a
  // declared measurement in `no_unrouted_strings.test.mjs`. That only works if the
  // script contributes none: a literal at a human sink here would be a string with
  // nowhere to go and no number tracking it. Same contract `embed_host_demo.js`
  // holds, which is why neither module is in that table.
  assert.deepEqual(
    scanScript(MODULE).map((site) => `line ${site.at}: ${site.text}`),
    [],
    "route it through the markup as a data- attribute, the way the chips' granted/withheld " +
      "labels already are",
  );
});

test("the module reads its configuration from the authority, not from a copy", () => {
  // The failure this prevents is the one the whole page is about: a demo that
  // resolves capabilities its own way shows a host something the editor will not
  // do. Every set on screen has to come through these two functions.
  for (const name of ["resolveCapabilities", "resolveRegions", "sandboxTokensFor", "editingModeFor"]) {
    assert.ok(MODULE.includes(name), `playground.js does not use ${name}`);
  }
  assert.ok(
    !/\bPRESETS\b|\bREGION_PRESETS\b/.test(MODULE),
    "playground.js reaches for a preset table instead of resolving through the authority",
  );
});
