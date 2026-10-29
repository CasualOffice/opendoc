// Link to Previous, Remove and the page-number positions (`docs/109` HF-192,
// RM-23): the pure rules behind the band bar, the Insert menu and the palette.
//
// The engine half — that unlinking copies the band with its page number, that
// Remove never writes through to an earlier section, that a page number keeps
// the band's content — is guarded in `crates/casual-doc-wasm/src/running_tests.rs`
// against the committed `sample.docx`. What is held here is the part a person
// reads: the reason a control is disabled, the label a switch shows, and that
// every command reaches the host contract and a menu home.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import { setCatalogue } from "../src/i18n.mjs";
import {
  BAND_COMMANDS,
  HEADER_FOOTER_COMMAND_IDS,
  PAGE_NUMBER_POSITIONS,
  PAGE_OF_TOTAL,
  bandCommandLabel,
  bandCommandReason,
  bandLabel,
  parseLinkState,
  splitPageOfTotal,
} from "../src/header_footer_commands.mjs";
import { commandContract } from "../src/host_contract.mjs";
import { menuCommandIds } from "../src/command_taxonomy.mjs";

const english = JSON.parse(readFileSync(new URL("../locales/en.json", import.meta.url), "utf8"));
setCatalogue("en", english);

const link = (band) => BAND_COMMANDS.find((row) => row.band === band && row.action === "link");
const remove = (band) => BAND_COMMANDS.find((row) => row.band === band && row.action === "remove");

test("the engine's link state parses, and the first section reads as null, not false", () => {
  assert.equal(parseLinkState("null"), null);
  assert.equal(parseLinkState("{not json"), null);
  assert.deepEqual(parseLinkState('{"section":1,"kind":"first","linked":null,"shown":true}'), {
    section: 1,
    kind: "first",
    linked: null,
    shown: true,
  });
  assert.equal(parseLinkState('{"section":2,"kind":"default","linked":false,"shown":false}').linked, false);
  // An unknown kind is the default variant, never undefined.
  assert.equal(parseLinkState('{"section":2,"kind":"odd","linked":true,"shown":true}').kind, "default");
});

// Mutation: make `bandCommandReason` return "" for `linked === null` — the first
// section's Link to previous is then offered live, and the first assertion fails.
test("Link to previous is disabled WITH a reason on the first section, and only there", () => {
  const first = parseLinkState('{"section":1,"kind":"default","linked":null,"shown":true}');
  const second = parseLinkState('{"section":2,"kind":"default","linked":true,"shown":true}');
  assert.equal(bandCommandReason(link("header"), first), english["headerFooter.firstSectionNoPrevious"]);
  assert.equal(bandCommandReason(link("footer"), first), english["headerFooter.firstSectionNoPrevious"]);
  assert.equal(bandCommandReason(link("header"), second), "");
  // No document: the reason every other command gives.
  assert.equal(bandCommandReason(link("header"), null), english["command.needsDocument"]);
});

// Mutation: drop the `!state.shown` branch — Remove is then offered on a page with
// no band and the assertion on the empty page fails.
test("Remove is disabled with a reason where the page shows no such band", () => {
  const empty = parseLinkState('{"section":1,"kind":"default","linked":null,"shown":false}');
  const shown = parseLinkState('{"section":1,"kind":"default","linked":null,"shown":true}');
  assert.equal(bandCommandReason(remove("header"), empty), english["headerFooter.noHeaderToRemove"]);
  assert.equal(bandCommandReason(remove("footer"), empty), english["headerFooter.noFooterToRemove"]);
  // Remove does NOT need a previous section: removing the first section's header is legitimate.
  assert.equal(bandCommandReason(remove("header"), shown), "");
});

test("a link row reads as a switch with its state, per band", () => {
  const on = parseLinkState('{"section":2,"kind":"default","linked":true,"shown":true}');
  const off = parseLinkState('{"section":2,"kind":"default","linked":false,"shown":true}');
  assert.equal(bandCommandLabel(link("header"), on), english["headerFooter.linkHeaderOn"]);
  assert.equal(bandCommandLabel(link("header"), off), english["headerFooter.linkHeaderOff"]);
  assert.equal(bandCommandLabel(link("footer"), on), english["headerFooter.linkFooterOn"]);
  assert.equal(bandCommandLabel(remove("footer"), on), english["headerFooter.removeFooter"]);
});

test("the band label names the section once there are two, and says Same as Previous", () => {
  assert.equal(bandLabel("header", { sectionCount: 1, section: 1 }), "Header");
  assert.equal(bandLabel("footer", null), "Footer");
  assert.equal(bandLabel("header", { sectionCount: 2, section: 2, headerLinked: false }), "Header -Section 2-");
  assert.equal(
    bandLabel("footer", { sectionCount: 3, section: 2, footerLinked: true, headerLinked: false }),
    "Footer -Section 2- · Same as Previous",
  );
  // Per band: the header's state never labels the footer.
  assert.equal(bandLabel("header", { sectionCount: 2, section: 2, headerLinked: false, footerLinked: true }), "Header -Section 2-");
});

test("the six positions are top and bottom by left, center and right", () => {
  assert.deepEqual(
    PAGE_NUMBER_POSITIONS.map((row) => `${row.band}:${row.align}`),
    ["header:left", "header:center", "header:right", "footer:left", "footer:center", "footer:right"],
  );
  for (const row of PAGE_NUMBER_POSITIONS) assert.ok(english[row.labelKey], `${row.labelKey} has English`);
});

// Mutation: delete one id from `host_contract.mjs` or from the Insert menu in
// `command_taxonomy.mjs` — that id then fails one of the two assertions.
test("every band command is a host-contract command with an Insert-menu home", () => {
  const insertIds = new Set(menuCommandIds("insert"));
  for (const id of HEADER_FOOTER_COMMAND_IDS) {
    assert.equal(commandContract(id)?.requires, "mutate", `${id} is a mutate command in the host contract`);
    assert.ok(insertIds.has(id), `${id} has a row in the Insert menu`);
  }
});

// Mutation: return the English shape unconditionally from `splitPageOfTotal` —
// the Chinese pattern then loses its words after the count and the second
// assertion fails.
test("Page X of Y splits the catalogue's pattern around its two fields, in every shape", () => {
  assert.deepEqual(splitPageOfTotal(english["pageNumber.ofTotalPattern"]), {
    before: "Page ",
    between: " of ",
    after: "",
  });
  assert.deepEqual(splitPageOfTotal("第 {page} 页，共 {pages} 页"), {
    before: "第 ",
    between: " 页，共 ",
    after: " 页",
  });
  // Fields are always PAGE then NUMPAGES: a pattern that reverses or drops a
  // placeholder falls back rather than writing them in the wrong order.
  assert.deepEqual(splitPageOfTotal("{pages} / {page}"), { before: "Page ", between: " of ", after: "" });
  assert.deepEqual(splitPageOfTotal(undefined), { before: "Page ", between: " of ", after: "" });
  assert.equal(PAGE_OF_TOTAL.band, "footer");
  assert.ok(HEADER_FOOTER_COMMAND_IDS.includes(PAGE_OF_TOTAL.command));
});

test("every locale's Page X of Y pattern keeps {page} before {pages}", () => {
  for (const tag of ["ar", "de", "es", "fr", "hi", "id", "it", "ja", "ko", "nl", "pl", "pt-BR", "ru", "tr", "uk", "vi", "zh-Hans", "zh-Hant"]) {
    const catalogue = JSON.parse(readFileSync(new URL(`../locales/${tag}.json`, import.meta.url), "utf8"));
    const pattern = catalogue["pageNumber.ofTotalPattern"];
    assert.ok(pattern.indexOf("{page}") >= 0 && pattern.indexOf("{page}") < pattern.indexOf("{pages}"), `${tag}: ${pattern}`);
  }
});
