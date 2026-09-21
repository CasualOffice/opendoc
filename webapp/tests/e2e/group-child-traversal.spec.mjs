// Reaching every shape in a group — including the ones inside a nested group.
//
// The owner's Medical Incident Report form is one anchored group holding four
// drawings, one pair of them inside a nested group. Every one of them RENDERED
// and exactly one of them could be selected: clicking picked the group, Enter
// descended to its first child, and Tab from there jumped straight back out to
// the group and stayed there. So three of the four shapes were unreachable by
// any gesture at all, keyboard or pointer, which is what "they're all grouped
// and I can't edit them" describes.
//
// The cause was one list. `traverseObjects` always walked `objectOrder()`, the
// TOP-LEVEL objects; a group child is not in that list, so `findIndex` returned
// -1, which the wrap-around reads as "nothing is selected" and restarts from
// index 0 — the group. `objectDescendants(root)` already existed and was used
// only by Enter.
//
// `nested-object-editing.spec.mjs` covers descending into a group and editing
// the child it lands on. What was missing, and is here, is that the OTHER
// children exist as far as the chrome is concerned.
import { test, expect, gotoEditor, stableBox } from "./fixtures.mjs";

const NESTED = "../fixtures/generated/nested-group.docx";

async function openNested(page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(NESTED);
  await expect(page.locator("#a11yDocument")).toContainText(
    "Body after the nested group",
  );
  return stableBox(page.locator(".page-wrap .page").first());
}

/** Clicks around the sheet until something selectable answers. */
async function selectAnObject(page, box) {
  for (let fy = 0.03; fy < 0.5; fy += 0.02) {
    for (let fx = 0.05; fx < 0.9; fx += 0.05) {
      await page.mouse.click(box.x + box.width * fx, box.y + box.height * fy);
      if (await page.locator("#pages").getAttribute("data-object-kind"))
        return true;
    }
  }
  return false;
}

const state = (page) =>
  page.locator("#pages").evaluate((el) => ({
    kind: el.dataset.objectKind,
    subject: el.dataset.objectSubject,
    path: el.dataset.objectPath,
    mode: el.dataset.objectMode,
  }));

test("Tab reaches every shape in a group, nested ones included", async ({
  page,
  consoleErrors,
}) => {
  const box = await openNested(page);
  expect(
    await selectAnObject(page, box),
    "the group should be selectable",
  ).toBe(true);
  expect((await state(page)).kind).toBe("group");
  const root = await page.locator("#pages").getAttribute("data-object-root");

  // Enter descends to the first child; Tab must then walk the REST, not leave.
  await page.keyboard.press("Enter");
  const visited = [];
  for (let i = 0; i < 8; i++) {
    const now = await state(page);
    expect(now.mode, "traversal must stay in selected mode").toBe("selected");
    expect(now.subject, "Tab must not leave the group").not.toBe(root);
    if (visited.includes(now.path)) break;
    visited.push(now.path);
    await page.keyboard.press("Tab");
  }

  // Three shapes: one direct child, and two inside the nested group. A nested
  // child's path has a dot in it, which is the only structural evidence the
  // chrome ever sees that the nesting exists.
  expect(visited.sort(), `reached: ${visited.join(", ")}`).toEqual([
    "0",
    "1.0",
    "1.1",
  ]);

  expect(consoleErrors).toEqual([]);
});

test("Shift+Tab walks the same group backwards", async ({
  page,
  consoleErrors,
}) => {
  const box = await openNested(page);
  expect(await selectAnObject(page, box)).toBe(true);
  await page.keyboard.press("Enter");
  const first = (await state(page)).path;

  await page.keyboard.press("Shift+Tab");
  const back = await state(page);
  // Backwards from the first child wraps to the LAST child, not out of the
  // group — the same rule as forwards, which is what makes it navigation
  // rather than two different gestures.
  expect(back.path).toBe("1.1");
  expect(back.mode).toBe("selected");

  await page.keyboard.press("Tab");
  expect((await state(page)).path).toBe(first);

  expect(consoleErrors).toEqual([]);
});

test("a nested child can be edited, and Escape climbs back out", async ({
  page,
  consoleErrors,
}) => {
  // Reaching a shape is only worth anything if it can then be edited, which is
  // the half the owner actually asked for.
  const box = await openNested(page);
  expect(await selectAnObject(page, box)).toBe(true);
  await page.keyboard.press("Enter");
  await page.keyboard.press("Tab"); // into the nested group
  expect((await state(page)).path).toBe("1.0");

  const bodyBefore = await page.locator("#a11yDocument").textContent();
  await page.keyboard.press("Enter");
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
  await page.keyboard.type("EDITED");
  // The edit is a real, undoable one. Read from the undo button rather than the
  // accessibility mirror, because the mirror does not carry grouped text-box
  // content at all — a separate, real defect, filed rather than papered over by
  // asserting something weaker here.
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
  // And it went into the shape, not the document body.
  expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);

  await page.keyboard.press("Escape");
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "selected",
  );

  expect(consoleErrors).toEqual([]);
});

test("at the top level Tab still walks the document's objects", async ({
  page,
  consoleErrors,
}) => {
  // The other half of the rule, which a fix that simply always traversed
  // descendants would break: a SELECTED group is its own root, so Tab there
  // must move between top-level objects rather than descend. Descending is
  // Enter, and conflating them would make a group impossible to step over.
  const box = await openNested(page);
  expect(await selectAnObject(page, box)).toBe(true);
  const before = await state(page);
  expect(before.kind).toBe("group");
  expect(before.path).toBe("");

  await page.keyboard.press("Tab");
  const after = await state(page);
  // This document has exactly one top-level object, so a correct top-level
  // traversal wraps back to it — and, crucially, does NOT land on a child.
  expect(after.path, "Tab on a selected group must not descend into it").toBe(
    "",
  );
  expect(after.kind).toBe("group");

  expect(consoleErrors).toEqual([]);
});
