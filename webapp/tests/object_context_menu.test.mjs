// What the object right-click menu offers, and what it refuses.
//
// This menu was 156 lines inside `main.js` with no test of any kind: the only way
// to find out whether a text box offered Crop, or whether Viewing greyed the
// destructive rows, was to open a browser and right-click. It came out under the
// line ratchet for OO-005 and came out PURE — it builds plain command descriptors
// and reaches the application only through its `io` — so all of that is now a
// node question.
//
// The rows that matter most here are the negative ones. "Crop" on a text box and
// "Delete" enabled in Viewing are both silent: the menu still renders, the click
// still fires, and the engine's own gate is the only thing left between the user
// and a change the review mode refuses.
import assert from "node:assert/strict";
import test from "node:test";

const { buildObjectContextCommands } = await import("../src/object_context_menu.mjs");

const CAPTION_ROW = {
  id: "reference.caption",
  label: "Insert caption",
  group: "caption",
  enabled: true,
  disabledReason: "",
  run: () => {},
};

/** An `io` that records nothing and answers everything, with the review mode and
 *  the read-only reason as knobs. */
const ARRANGE = {
  wrappable: { available: true, reason: "" },
  positionable: { available: true, reason: "" },
  stackable: { enabled: true, reason: null },
  groupable: { can: true, reason: null },
  rotatable: true,
};

function host({
  reviewMode = "editing",
  readOnlyReason = "",
  documentRows = [CAPTION_ROW],
  arrange = ARRANGE,
  activeWrap = "square",
} = {}) {
  return {
    reviewMode: () => reviewMode,
    readOnlyReason: () => readOnlyReason,
    wrapModes: () => [
      ["inline", "In line with text"],
      ["square", "Square"],
    ],
    positionPresets: () => [["topLeft", "Top left", { h: "left", v: "top" }]],
    zOrderChoices: () => [["front", "Bring to front"]],
    rotateChoices: () => [["right90", "Rotate right 90\u00b0"]],
    text: (key) => key,
    arrangeState: () => arrange,
    activeWrap: () => activeWrap,
    applyPosition: () => {},
    setZOrder: () => {},
    group: () => {},
    ungroup: () => {},
    rotate: () => {},
    addText: () => {},
    shapeColors: () => ["#000000", "#ffffff"],
    objectWrap: () => "square",
    documentRows: () => documentRows,
    setObjectWrap: () => {},
    openAltText: () => {},
    applyShapeFill: () => {},
    applyShapeOutline: () => {},
    enterCrop: () => {},
    openProperties: () => {},
    deleteObject: () => {},
  };
}

const picture = {
  surface: "object",
  kind: "picture",
  ref: { root: "r1", subject: "r1" },
  canWrap: true,
  canAltText: true,
  canCrop: true,
  canDelete: true,
  canFill: false,
  canStroke: false,
};

const textBox = { ...picture, kind: "textbox", canCrop: false };

const shape = { ...picture, kind: "shape", canCrop: false, canFill: true, canStroke: true };

const ids = (context, io = host()) =>
  buildObjectContextCommands(context, io).map((command) => command.id);

const row = (context, id, io = host()) =>
  buildObjectContextCommands(context, io).find((command) => command.id === id);

// ---- Membership -------------------------------------------------------------

test("a picture offers caption, wrap, alt text, crop, properties and delete", () => {
  assert.deepEqual(ids(picture), [
    "reference.caption",
    "object.wrap",
    "object.position",
    "object.arrange",
    "object.rotate",
    "object.altText",
    "object.crop",
    "object.properties",
    "object.delete",
  ]);
});

test("Insert caption comes FIRST, on every kind of object", () => {
  // ONLYOFFICE puts it at the top of the picture, table and equation menus
  // (`DocumentHolderExt.js:46`, ahead of its own separator) because it is the
  // reason a reader right-clicks a figure, and Word's picture menu agrees.
  for (const context of [picture, textBox, shape]) {
    assert.equal(ids(context)[0], "reference.caption", context.kind);
  }
});

test("a text box offers no Crop: it has no source rectangle to crop", () => {
  assert.ok(!ids(textBox).includes("object.crop"));
  assert.ok(ids(textBox).includes("object.altText"));
});

test("only a shape offers Shape fill and Shape outline", () => {
  assert.ok(ids(shape).includes("object.fill"));
  assert.ok(ids(shape).includes("object.outline"));
  assert.ok(!ids(picture).includes("object.fill"));
  // A "shape" whose engine capabilities say it can do neither gets neither.
  assert.ok(!ids({ ...shape, canFill: false, canStroke: false }).includes("object.fill"));
});

test("an object the engine says cannot be deleted has no Delete row", () => {
  assert.ok(!ids({ ...picture, canDelete: false }).includes("object.delete"));
});

test("an INLINE object STILL gets the wrap row, with In line as the ticked mode", () => {
  // This is the defect the owner reported. `canWrap` is false for an inline
  // object — correctly: it has no anchor to set a wrap on — and the menu used to
  // be gated on that bit, so an inserted picture (which is inline) offered no
  // wrap control at all and "In line" existed nowhere in the product. The
  // control is now gated on whether the object's ANCHOR KIND can be rewritten,
  // which `objectPosition` answers, and in-line is the first mode in the row.
  const io = host({ activeWrap: "inline" });
  const wrap = row({ ...picture, canWrap: false }, "object.wrap", io);
  assert.ok(wrap, "an inline object must still be offered the wrap row");
  assert.equal(wrap.submenu[0].id, "object.wrap.inline");
  assert.equal(wrap.submenu[0].shortcut, "\u2713", "In line must read as the current mode");
  assert.equal(wrap.submenu[1].shortcut, "");
});

test("a group CHILD gets the wrap row disabled with the reason, never hidden", () => {
  // Its parent decides where it sits, and the engine says so. A control that
  // vanishes cannot be told from a bug (SKILL §10: never a dead control, and
  // never a silent one either).
  const io = host({
    arrange: {
      ...ARRANGE,
      wrappable: { available: false, reason: "A shape inside a group is positioned by its group" },
      positionable: { available: false, reason: "A shape inside a group is positioned by its group" },
    },
  });
  const wrap = row(shape, "object.wrap", io);
  assert.ok(wrap);
  for (const child of wrap.submenu) {
    assert.equal(child.enabled, false);
    assert.equal(child.disabledReason, "A shape inside a group is positioned by its group");
  }
});

test("Group carries the ENGINE's refusal, not one this host invented", () => {
  const io = host({
    arrange: {
      ...ARRANGE,
      groupable: { can: false, reason: "every object must be on the same page" },
    },
  });
  const arrange = row(picture, "object.arrange", io);
  const group = arrange.submenu.find((child) => child.id === "object.group");
  assert.equal(group.enabled, false);
  assert.equal(group.disabledReason, "every object must be on the same page");
});

test("Ungroup is only live on a group, and Rotate only where the model carries one", () => {
  const io = host({ arrange: { ...ARRANGE, rotatable: false } });
  const ungroup = row(picture, "object.arrange", io).submenu.find((c) => c.id === "object.ungroup");
  assert.equal(ungroup.enabled, false);
  assert.equal(ungroup.disabledReason, "object.ungroup.notAGroup");
  const rotate = row(picture, "object.rotate", io).submenu[0];
  assert.equal(rotate.enabled, false);
  assert.equal(rotate.disabledReason, "object.rotate.unsupported");
});

test("only a shape offers Add text", () => {
  assert.ok(ids(shape).includes("object.addText"));
  assert.ok(!ids(picture).includes("object.addText"));
  assert.ok(!ids(textBox).includes("object.addText"));
});

test("an object the engine does not describe gets no arrange rows at all", () => {
  // `objectPosition` answers "" for something that is not a top-level object.
  // Offering Position for it would be inventing a claim the document does not
  // make; the rows are absent rather than wrong.
  const io = host({ arrange: null });
  const offered = ids(picture, io);
  for (const id of ["object.wrap", "object.position", "object.arrange", "object.rotate"]) {
    assert.ok(!offered.includes(id), id);
  }
});

// ---- The caption row is the ribbon's row, not a second copy of it -----------

test("the caption row is whatever the one declaration says it is", () => {
  // The whole point of `documentRows`: the label the menu shows is the label the
  // ribbon button and the palette row show, because it is the same object. A
  // menu that restated it is how a command comes to be called two things — the
  // defect `105` UX-004 keeps finding.
  const io = host({
    documentRows: [{ ...CAPTION_ROW, label: "Beschriftung einfügen" }],
  });
  assert.equal(row(picture, "reference.caption", io).label, "Beschriftung einfügen");
});

test("a row the declaration says is unavailable stays unavailable and says why", () => {
  const io = host({
    documentRows: [{ ...CAPTION_ROW, enabled: false, disabledReason: "Place the caret in a paragraph" }],
  });
  const caption = row(picture, "reference.caption", io);
  assert.equal(caption.enabled, false);
  assert.equal(caption.disabledReason, "Place the caret in a paragraph");
});

test("no document row at all leaves the rest of the menu intact", () => {
  assert.deepEqual(ids(picture, host({ documentRows: [] })), [
    "object.wrap",
    "object.position",
    "object.arrange",
    "object.rotate",
    "object.altText",
    "object.crop",
    "object.properties",
    "object.delete",
  ]);
});

// ---- Review modes -----------------------------------------------------------

/** Every row and submenu row that claims to be enabled. */
function enabledIds(context, io) {
  const out = [];
  for (const command of buildObjectContextCommands(context, io)) {
    if (command.enabled !== false) out.push(command.id);
    for (const child of command.submenu ?? []) {
      if (child.enabled !== false) out.push(child.id);
    }
  }
  return out;
}

test("Viewing greys every mutating row, submenus included, and says why", () => {
  const io = host({ reviewMode: "viewing" });
  // Properties is the one ACTION that stays live: reading an object's geometry
  // is useful in Viewing and the panel's own Apply refuses the write. The Wrap
  // row stays openable because it is a container, not an action — a submenu
  // sealed shut is a refusal a keyboard user cannot read.
  assert.deepEqual(enabledIds(picture, io), [
    "object.wrap",
    "object.position",
    "object.arrange",
    "object.rotate",
    "object.properties",
  ]);
  for (const id of ["reference.caption", "object.altText", "object.crop", "object.delete"]) {
    assert.equal(row(picture, id, io).disabledReason, "Turn on Editing to change this object");
  }
  // The wrap submenu's rows carry the reason too — a disabled parent with live
  // children is the shape that lets a keyboard user reach the refusal.
  const wrap = row(picture, "object.wrap", io);
  for (const child of wrap.submenu) {
    assert.equal(child.enabled, false);
    assert.equal(child.disabledReason, "Turn on Editing to change this object");
  }
});

test("Suggesting refuses for a different reason: the change cannot be tracked", () => {
  const io = host({ reviewMode: "suggesting" });
  // Again: the three submenu PARENTS open, and everything inside them refuses.
  assert.deepEqual(enabledIds(shape, io), [
    "object.wrap",
    "object.position",
    "object.arrange",
    "object.rotate",
    "object.fill",
    "object.outline",
    "object.properties",
  ]);
  assert.equal(
    row(shape, "object.delete", io).disabledReason,
    "Object changes cannot be tracked in Suggesting mode",
  );
  // The fill swatches are refusals too, not decorations.
  const fill = row(shape, "object.fill", io);
  assert.equal(fill.submenu[0].enabled, false);
  assert.equal(fill.submenu[1].disabledReason, "Object changes cannot be tracked in Suggesting mode");
});

test("a read-only DOCUMENT outranks the review mode's advice", () => {
  // "Turn on Editing" is advice the reader cannot act on when the FILE is the
  // read-only thing, which is why the document's own reason wins.
  const io = host({ reviewMode: "viewing", readOnlyReason: "This file was opened read-only" });
  assert.equal(row(picture, "object.delete", io).disabledReason, "This file was opened read-only");
  assert.equal(row(picture, "reference.caption", io).disabledReason, "This file was opened read-only");
});

test("Editing enables everything the object can do", () => {
  assert.deepEqual(enabledIds(picture, host()), [
    "reference.caption",
    "object.wrap",
    "object.wrap.inline",
    "object.wrap.square",
    "object.position",
    "object.position.topLeft",
    "object.arrange",
    "object.z.front",
    "object.group",
    "object.rotate",
    "object.rotate.right90",
    "object.altText",
    "object.crop",
    "object.properties",
    "object.delete",
  ]);
});

// ---- Wrap submenu -----------------------------------------------------------

test("the wrap submenu ticks the mode the object is actually in", () => {
  const wrap = row(picture, "object.wrap", host());
  assert.deepEqual(
    wrap.submenu.map((child) => [child.id, child.shortcut]),
    [
      ["object.wrap.inline", ""],
      ["object.wrap.square", "✓"],
    ],
  );
});

test("Delete is marked destructive so the menu can paint it as such", () => {
  assert.equal(row(picture, "object.delete", host()).danger, true);
});
