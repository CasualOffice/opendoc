// Where focus goes when an anchored popover closes holding it (`docs/109`
// UX-055, audit DIALOGS-05). The audit measured the Insert table grid and the
// Shape gallery, opened from the palette, dropping focus on <body> on Escape:
// the popover manager sent focus to its trigger, a ribbon button on a tab that
// was not showing. The rule now: whoever had focus when it opened, else the
// trigger, else the document — never <body>.
//
// Driven against a stand-in DOM so the rule can fail here; the browser half is
// `e2e/panel-dock.spec.mjs` ("…opened from the palette…").
import { test } from "node:test";
import assert from "node:assert/strict";

const handlers = new Map();
const body = { id: "body", isConnected: true, getClientRects: () => [] };
globalThis.window = { innerWidth: 1280, innerHeight: 800 };
globalThis.document = {
  body,
  activeElement: body,
  addEventListener: (type, fn) => handlers.set(type, fn),
};

/** A focusable stand-in; `shown: false` is an element with no box (a hidden tab). */
function el(id, { shown = true } = {}) {
  const node = {
    id,
    shown,
    isConnected: true,
    disabled: false,
    hidden: true,
    style: { setProperty() {} },
    offsetWidth: 100,
    offsetHeight: 100,
    children: [],
    attrs: {},
    getClientRects: () => (node.shown ? [{}] : []),
    getBoundingClientRect: () => ({ left: 0, top: 0, bottom: 20, right: 20 }),
    setAttribute(k, v) {
      node.attrs[k] = v;
    },
    addEventListener() {},
    querySelectorAll: () => node.children,
    contains: (other) => other === node || node.children.includes(other),
    matches: () => false,
    focus() {
      document.activeElement = node;
    },
  };
  return node;
}

const { registerPopover, openPopover, closePopover, configurePopovers } = await import("../src/popover_manager.mjs");
let fellBack = 0;
const editor = el("editorTextInput");
configurePopovers({ focusFallback: () => (fellBack++, editor.focus()) });

function setup({ triggerShown = true } = {}) {
  const trigger = el("insertShapeBtn", { shown: triggerShown });
  const menu = el("shapeGalleryMenu");
  const cell = el("cell0");
  menu.children.push(cell);
  const p = registerPopover(trigger, menu, () => {});
  return { p, trigger, cell };
}

test("opened from the keyboard on its trigger: focus goes back to the trigger", () => {
  const { p, trigger, cell } = setup();
  trigger.focus();
  openPopover(p, { keyboard: true });
  cell.focus();
  closePopover(p);
  assert.equal(document.activeElement, trigger);
});

test("opened from the palette with its ribbon tab not showing: focus goes to the document, not <body>", () => {
  const { p, cell } = setup({ triggerShown: false });
  // The palette has closed and handed focus back to the document.
  editor.focus();
  openPopover(p, { keyboard: true });
  cell.focus();
  closePopover(p);
  assert.equal(document.activeElement, editor, `focus is on ${document.activeElement.id}`);
});

test("nothing to return to and the trigger off screen: the document's fallback, never the hidden trigger", () => {
  const { p, cell, trigger } = setup({ triggerShown: false });
  document.activeElement = body;
  fellBack = 0;
  openPopover(p, { keyboard: true });
  cell.focus();
  closePopover(p);
  assert.notEqual(document.activeElement, trigger, "focus went to a trigger with no box (a hidden tab), which the browser drops to <body>");
  assert.equal(fellBack, 1);
  assert.equal(document.activeElement, editor);
});

test("focus not inside the menu is left where it is", () => {
  const { p, trigger } = setup();
  editor.focus();
  openPopover(p, { keyboard: false });
  const other = el("somewhere");
  other.focus();
  closePopover(p);
  assert.equal(document.activeElement, other);
  assert.notEqual(document.activeElement, trigger);
});
