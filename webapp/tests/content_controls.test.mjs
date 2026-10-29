// Content controls, form roles and the accessibility checker's merge, in node
// (`docs/109` RM-34, HF-064). The engine side is `content_controls_tests.rs`.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import {
  CONTROL_KINDS,
  CONTROL_INSERT_COMMANDS,
  clickOpensChooser,
  dateInputFromIso,
  fillModeOf,
  formatWordDate,
  isoFromDateInput,
  normaliseItems,
  propertiesPatch,
} from "../src/content_control_model.mjs";
import { controlsForRole, nextRoles, rolesStorageKey } from "../src/form_roles.mjs";
import { checkAccessibility, contrastFindings, mergeFindings } from "../src/accessibility_checker.mjs";
import { commandContract } from "../src/host_contract.mjs";
import { APP_MENU_SECTIONS, sectionCommandIds } from "../src/command_taxonomy.mjs";

const html = readFileSync(new URL("../editor.html", import.meta.url), "utf8");

test("Word's seven kinds, in Word's order, each a contract command", () => {
  assert.deepEqual(
    CONTROL_KINDS.map((row) => row.kind),
    ["richText", "plainText", "picture", "checkbox", "comboBox", "dropDownList", "date"],
  );
  for (const id of CONTROL_INSERT_COMMANDS) {
    const row = commandContract(id);
    assert.ok(row, `${id} is in the host contract`);
    assert.equal(row.requires, "mutate");
  }
  for (const id of ["controls.properties", "controls.remove", "controls.fill"]) assert.equal(commandContract(id)?.requires, "mutate");
  for (const id of ["review.protectForm", "forms.roles"]) assert.equal(commandContract(id)?.requires, "edit");
});

test("the Controls dropdown, the Insert menu and the kind table name the same seven inserts", () => {
  const menu = html.slice(html.indexOf('id="insertControlsMenu"'));
  const rows = [...menu.slice(0, menu.indexOf("</div>")).matchAll(/data-command="(controls\.insert\.[^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual(rows, CONTROL_INSERT_COMMANDS);
  const insertMenu = sectionCommandIds(APP_MENU_SECTIONS.insert).filter((id) => id.startsWith("controls.insert."));
  assert.deepEqual(insertMenu, CONTROL_INSERT_COMMANDS);
  const review = sectionCommandIds(APP_MENU_SECTIONS.review);
  assert.ok(review.includes("review.protectForm") && review.includes("forms.roles"), "Protect form and Form roles have a Review menu home");
});

test("how each kind is filled, and which ones a click opens a chooser for", () => {
  assert.equal(fillModeOf({ kind: "dropDownList" }), "list");
  assert.equal(fillModeOf({ kind: "comboBox" }), "combo");
  assert.equal(fillModeOf({ kind: "date" }), "date");
  assert.equal(fillModeOf({ kind: "picture" }), "picture");
  assert.equal(fillModeOf({ kind: "checkbox" }), "checkbox");
  assert.equal(fillModeOf({ kind: "plainText" }), "text");
  assert.equal(fillModeOf({ kind: "group" }), null);
  assert.equal(clickOpensChooser({ kind: "dropDownList" }), true);
  assert.equal(clickOpensChooser({ kind: "date" }), true);
  assert.equal(clickOpensChooser({ kind: "picture" }), true);
  // A combo box is typed into too: the click places the caret.
  assert.equal(clickOpensChooser({ kind: "comboBox" }), false);
  assert.equal(clickOpensChooser({ kind: "richText" }), false);
});

test("a date is written in the control's own Word date picture, in UTC", () => {
  const day = new Date(Date.UTC(2026, 2, 5));
  assert.equal(formatWordDate(day, "M/d/yyyy", "en"), "3/5/2026");
  assert.equal(formatWordDate(day, "dd/MM/yy", "en"), "05/03/26");
  assert.equal(formatWordDate(day, "dddd, MMMM d, yyyy", "en"), "Thursday, March 5, 2026");
  assert.equal(formatWordDate(day, "d MMMM yyyy", "fr"), "5 mars 2026");
  assert.equal(formatWordDate(day, "'Due' d-MMM-yy", "en"), "Due 5-Mar-26");
  assert.equal(isoFromDateInput("2026-03-05"), "2026-03-05T00:00:00Z");
  assert.equal(isoFromDateInput("2026-02-30"), null, "a day that does not exist is refused");
  assert.equal(isoFromDateInput("tomorrow"), null);
  assert.equal(dateInputFromIso("2026-03-05T00:00:00Z"), "2026-03-05");
  assert.equal(dateInputFromIso(undefined), "");
});

test("list entries are normalised the way Word stores them", () => {
  assert.deepEqual(
    normaliseItems([{ display: " Yes ", value: "" }, { display: "", value: "n" }, { display: "", value: "" }, { display: "Maybe", value: "m" }]),
    [
      { display: "Yes", value: "" },
      { display: "n", value: "n" },
      { display: "Maybe", value: "m" },
    ],
  );
});

test("the properties patch carries only what the control's kind has", () => {
  const fields = { title: "T", tag: "t", useColor: true, color: "#AA3300", placeholder: "Pick", temporary: false, lockControl: true, lockContent: false, role: "Manager", items: [{ display: "A" }], dateFormat: "d MMM" };
  const list = propertiesPatch({ kind: "dropDownList" }, fields);
  assert.deepEqual(list.items, [{ display: "A", value: "" }]);
  assert.equal(list.dateFormat, undefined);
  assert.equal(list.color, "AA3300");
  assert.equal(list.placeholder, "Pick");
  const date = propertiesPatch({ kind: "date" }, fields);
  assert.equal(date.items, undefined);
  assert.equal(date.dateFormat, "d MMM");
  const box = propertiesPatch({ kind: "checkbox" }, { ...fields, useColor: false });
  assert.equal(box.placeholder, undefined, "a check box has no placeholder");
  assert.equal(box.color, "", "no colour clears it");
  assert.equal(box.role, "Manager");
});

test("a renamed role keeps its controls, a removed one releases them", () => {
  const previous = {
    roles: [{ name: "Employee", color: "3355C4" }, { name: "Manager", color: "AA3300" }],
    assignments: { 11: "Employee", 12: "Manager", 13: "Manager" },
    fillingAs: "Manager",
  };
  const next = nextRoles(
    previous,
    [
      { original: "Manager", name: "Approver", color: "#aa3300" },
      { original: "", name: "Witness", color: "#00aa00" },
    ],
    "Manager",
  );
  assert.deepEqual(next.roles, [
    { name: "Approver", color: "AA3300" },
    { name: "Witness", color: "00AA00" },
  ]);
  assert.deepEqual(next.assignments, { 12: "Approver", 13: "Approver" }, "Employee's control is anyone's now");
  assert.equal(next.fillingAs, "Approver", "the reader follows their role's rename");
  assert.equal(nextRoles(previous, [], "Manager").fillingAs, undefined);
  assert.equal(rolesStorageKey("Form.docx"), "opendoc.formRoles.v1:Form.docx");
});

test("the roles dialog filters the fields a role fills", () => {
  const controls = [{ id: "a", role: "Manager" }, { id: "b" }, { id: "c", role: "Employee" }];
  assert.deepEqual(controlsForRole(controls, "Manager").map((c) => c.id), ["a"]);
  assert.deepEqual(controlsForRole(controls, "").map((c) => c.id), ["b"]);
  assert.deepEqual(controlsForRole(controls, "*").map((c) => c.id), ["a", "b", "c"]);
});

test("the checker reports every rule, contrast included, errors first", () => {
  const blocks = [
    { kind: "heading", level: 1, text: "Title" },
    { kind: "heading", level: 3, text: "Deep" },
    { kind: "image", alt: "" },
    { kind: "table", rows: [[{ text: "a", colSpan: 2 }], [{ text: "" }]], headerRows: [] },
  ];
  const nodes = ["h1", "h3", "img", "tbl"];
  const findings = mergeFindings(
    checkAccessibility(blocks, nodes),
    contrastFindings([{ node: "p9", offset: 4, text: "faint", ratio: 1.6 }]),
  );
  assert.deepEqual(
    findings.map((f) => [f.severity, f.rule, f.node]),
    [
      ["error", "missingAlt", "img"],
      ["error", "tableHeader", "tbl"],
      ["warning", "headingSkip", "h3"],
      ["warning", "mergedCells", "tbl"],
      ["warning", "emptyRow", "tbl"],
      ["warning", "lowContrast", "p9"],
    ],
  );
  assert.equal(findings.at(-1).offset, 4, "Go to lands on the run, not the paragraph start");
  assert.deepEqual(checkAccessibility([{ kind: "heading", level: 2 }, { kind: "heading", level: 1 }, { kind: "image", alt: "A cat" }]), [], "going up any number of levels and described pictures are fine");
});
