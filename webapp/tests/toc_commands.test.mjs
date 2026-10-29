// The table-of-contents settings dialog, Remove, Add text and the table of
// figures (`docs/153` RM-25): the pure halves, held without a browser.
//
// `tocSettingsFromForm` is the one place the dialog's controls become the
// engine's `insertTableOfContentsWith` settings, so a control that is read but
// never sent — the defect this guards — shows up here as a missing field.
import assert from "node:assert/strict";
import test from "node:test";

// `popover_manager.mjs` installs document-level listeners when it is imported.
globalThis.document = {
  getElementById: () => null,
  addEventListener: () => {},
  activeElement: null,
};

const {
  ADD_TEXT_LEVELS,
  TOC_FORMATS,
  TOC_LEADERS,
  addTextCommand,
  addTextLabel,
  tocSettingsFromForm,
  tocSurfaceRows,
} = await import("../src/toc_commands.mjs");
const { commandContract } = await import("../src/host_contract.mjs");
const { t } = await import("../src/i18n.mjs");

const FORM = Object.freeze({
  mode: "contents",
  pageNumbers: true,
  rightAlign: true,
  leader: "dot",
  hyperlinks: false,
  source: "outline",
  levels: 3,
  styles: [],
  format: "current",
  caption: "Figure",
  includeLabel: true,
});

test("every control the contents dialog shows reaches the engine's settings", () => {
  // Mutation: drop `hyperlinks` (or `leader`) from the object `tocSettingsFromForm`
  // builds and this deepEqual goes red.
  assert.deepEqual(
    tocSettingsFromForm({ ...FORM, pageNumbers: false, rightAlign: false, leader: "hyphen", hyperlinks: true, levels: 5, format: "modern" }),
    { pageNumbers: false, rightAlign: false, leader: "hyphen", hyperlinks: true, format: "modern", levels: 5 },
  );
  assert.equal(tocSettingsFromForm({ ...FORM, levels: 40 }).levels, 9, "levels clamp to Word's 1..9");
  assert.equal(tocSettingsFromForm({ ...FORM, levels: Number.NaN }).levels, 3, "an unreadable box is Word's 3");
  assert.equal(tocSettingsFromForm({ ...FORM, leader: "stars" }).leader, "dot", "an unknown leader is the default");
});

test("building from styles sends only the styles given a level, and refuses none", () => {
  const settings = tocSettingsFromForm({
    ...FORM,
    source: "styles",
    styles: [
      ["Heading 1", "1"],
      ["Quote", ""],
      ["Callout", "2"],
      ["Bad", "12"],
    ],
  });
  assert.deepEqual(settings.styles, [
    ["Heading 1", 1],
    ["Callout", 2],
  ]);
  assert.equal(settings.levels, undefined, "a style table carries no outline-level range");
  assert.equal(tocSettingsFromForm({ ...FORM, source: "styles", styles: [["Quote", ""]] }), null);
});

test("the figures mode sends a caption label and the include-label switch, not the outline", () => {
  assert.deepEqual(tocSettingsFromForm({ ...FORM, mode: "figures", caption: " Table ", includeLabel: false }), {
    pageNumbers: true,
    rightAlign: true,
    leader: "dot",
    hyperlinks: false,
    caption: "Table",
    includeLabel: false,
  });
});

test("the gallery and leader lists are the ones the engine reads", () => {
  // The engine's tokens (`casual-doc-wasm` `toc.rs` `TocSettings`, `TocStyle::token`).
  assert.deepEqual([...TOC_LEADERS], ["none", "dot", "hyphen", "underscore"]);
  assert.deepEqual([...TOC_FORMATS], ["current", "simple", "standard", "modern", "classic", "online"]);
});

test("every surface row this module declares is a host-contract command with a routed label", () => {
  const rows = tocSurfaceRows(() => ({}));
  const ids = rows.map((row) => row.command);
  assert.deepEqual(ids, [
    "reference.removeToc",
    "reference.tableOfFigures",
    ...ADD_TEXT_LEVELS.map(addTextCommand),
  ]);
  assert.equal(addTextCommand(-1), "reference.addText.none");
  assert.equal(addTextCommand(4), "reference.addText.4");
  for (const row of rows) {
    const contract = commandContract(row.command);
    assert.ok(contract, `${row.command} is not in the host contract`);
    assert.equal(contract.requires, "mutate", `${row.command} changes the document`);
    // A key that is missing from the catalogue comes back as the key itself.
    assert.ok(!row.label.startsWith("toc."), `${row.command}'s label is a raw key: ${row.label}`);
    assert.equal(typeof row.run, "function");
  }
  assert.equal(addTextLabel(-1), t("toc.addText.none"));
  assert.equal(addTextLabel(2), t("toc.addText.level", { level: 2 }));
});
