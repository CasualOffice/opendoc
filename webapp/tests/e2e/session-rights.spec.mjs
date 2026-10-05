// SPDX-License-Identifier: Apache-2.0
// The rights surface, through the running app, in the three states that matter.
//
// The owner's decision is the specification: "a full rights-changing dialog for
// owner and editor — not viewer... not greyed, not present. And for SDK or single
// user, the role is pre-decided while loading the file."
//
// The node guards (`session_rights.test.mjs`) hold the DECISIONS. These hold the
// consequence a reader can see: whether a control is on screen, on every surface
// at once. That is the distinction `opendoc-command-surface-parity` records as
// the recurring defect — a capability reachable from one surface and not another
// — so every state here is asserted on the ribbon AND the menu AND the palette
// rather than on whichever one was convenient.
//
// The participant grant arrives on the URL, which is `session_access.mjs`'s
// documented channel for a document opened without a live `Welcome`, and is also
// the only way a spec can be a room's viewer without a relay. It cannot be used
// to widen anything: `adoptParticipantCapabilities` intersects, so a visitor who
// edits the query string can take access away from themselves and can add none.

import { expect, test } from "@playwright/test";

import { clickIntoFirstPage, gotoEditor } from "./fixtures.mjs";

/** The platform's command modifier, as every other spec in this directory
 *  derives it. Never a literal glyph: `formatShortcut` renders `⌘P` as `Ctrl+P`
 *  on the Linux runner, so a spec that asserted one would be asserting the
 *  runner (`105` UX-009). */
const MOD = process.platform === "darwin" ? "Meta" : "Control";

/** The query a participant with this capability list would arrive with. */
const asParticipant = (names) => `&granted=${names.join(",")}&participant=0`;

const OWNER = ["comment", "edit", "manageAccess", "manageProtection", "review", "suggest"];
const EDITOR = ["comment", "edit", "manageAccess", "review", "suggest"];
const VIEWER = [];

/** Opens the Review band and returns the Manage access button's locator.
 *
 *  Only usable where there IS a ribbon. A participant granted nothing opens in
 *  Viewing, and the Viewing chrome composes the whole ribbon away — measured:
 *  every `[data-tab]` has zero client rects and `.ribbon` computes to
 *  `display: none`. So the viewer case asserts the button is not on screen
 *  without trying to reach a band that does not exist. */
async function reviewBand(page) {
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="review"]').click();
  return page.locator("#reviewManageAccessBtn");
}

/** Whether an element is painted at all, which is the question "is this on
 *  screen" really asks. A `hidden` attribute an author `display` overrode is
 *  still painted, and that is exactly the defect `.fmt[hidden]` exists for. */
const onScreen = (page, selector) =>
  page.evaluate(
    (css) => (document.querySelector(css)?.getClientRects().length ?? 0) > 0,
    selector,
  );

/** Whether the command is in the palette's registry at all.
 *
 *  Asked of the WHOLE registry rather than of a filtered list, because a filter
 *  that matched nothing and a command that is absent look the same — which is the
 *  mistake a guard about absence is most likely to make. */
async function inPalette(page) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  const ids = await page
    .locator("#cmdList .cmd-item[data-command-id]")
    .evaluateAll((rows) => rows.map((row) => row.dataset.commandId));
  await page.keyboard.press("Escape");
  // The palette really did list the registry, or every absence below is vacuous.
  expect(ids.length).toBeGreaterThan(80);
  return ids.includes("review.manageAccess");
}

/** Whether the Review menu carries the command.
 *
 *  The menu bar is the COMPACT chrome's one axis and is entered by the chrome-mode
 *  control, not by narrowing the window — `one-axis-navigation.spec.mjs` is where
 *  that is established, and a first draft of this helper resized the viewport and
 *  found the bar hidden. It switches back afterwards, because the choice persists
 *  and a caller that looks at the ribbon next would otherwise find none. */
async function inMenu(page) {
  // Already the one axis in Viewing, where there is no chrome-mode control to
  // press — and pressing one that is composed away is how this helper failed
  // first time. Switch only when the bar is not already there, and switch back
  // only then too.
  const switched = !(await onScreen(page, "#appMenuBar"));
  if (switched) await page.locator("#modeCompact").click();
  await expect(page.locator("#appMenuBar")).toBeVisible();
  await page.locator('.app-menu-button[data-menu="review"]').click();
  await expect(page.locator("#appMenuPopover")).toBeVisible();
  const ids = await page
    .locator("#appMenuPopover .app-menu-item[data-command]")
    .evaluateAll((rows) => rows.map((row) => row.dataset.command));
  await page.keyboard.press("Escape");
  if (switched) await page.locator("#modeRibbon").click();
  // The Review menu really did render, so an absence is about this command.
  expect(ids).toContain("review.restrictEditing");
  return ids.includes("review.manageAccess");
}

test("a viewer in a shared document is offered the rights surface nowhere at all", async ({
  page,
}) => {
  // THE OWNER'S DECISION, asserted as absence on every surface. Absence is the
  // kind of property a spec passes by accident — a selector that matches nothing
  // because the id is misspelt looks exactly like a control that is correctly
  // absent — so the owner test below uses the SAME selectors and requires them to
  // match. Neither test is worth anything without the other.
  await gotoEditor(page, asParticipant(VIEWER));
  await clickIntoFirstPage(page);

  // Not on screen. Asserted as "is it painted" rather than on the `hidden`
  // attribute, because an attribute an author `display` overrode is still
  // painted — which is what this button did until `.fmt[hidden]` existed.
  expect(await onScreen(page, "#reviewManageAccessBtn")).toBe(false);
  expect(await inPalette(page)).toBe(false);
  expect(await inMenu(page)).toBe(false);
  // ...and the state is still legible, which is the half that makes absence
  // acceptable rather than silent.
  await expect(page.locator("#accessBadgeLevel")).toHaveText("Viewer");
});

test("an owner and an editor are offered it, live, on the ribbon and in the menu", async ({
  page,
}) => {
  // The other side of the same branch, with the same selectors, which is what
  // makes the absence above mean something.
  for (const grant of [OWNER, EDITOR]) {
    await gotoEditor(page, asParticipant(grant));
    const button = await reviewBand(page);
    await expect(button).toBeVisible();
    await expect(button).toBeEnabled();
    expect(await inPalette(page)).toBe(true);
    expect(await inMenu(page)).toBe(true);
  }
});

test("a commenter is a viewer for this purpose, and still sees what they MAY do", async ({
  page,
}) => {
  // `manageAccess` and not "can write something": a commenter, a suggester and a
  // reviewer all change the document and none of them manages the room. And the
  // state stays legible — the point of the badge is that a reader who is offered
  // nothing is still told what they have.
  await gotoEditor(page, asParticipant(["comment"]));
  await clickIntoFirstPage(page);
  expect(await onScreen(page, "#reviewManageAccessBtn")).toBe(false);
  await expect(page.locator("#accessBadge")).toBeVisible();
  await expect(page.locator("#accessBadgeLevel")).toHaveText("Commenter");
  // The second half names the authority, which is the question a reader asks
  // next: who decided this.
  await expect(page.locator("#accessBadgeSource")).toHaveText("in this shared document");
});

test("a document with no room offers the command DISABLED WITH A REASON, not absent", async ({
  page,
}) => {
  // The absent thing is a SESSION, not a permission, and the two must not look
  // alike: a reader who reached for Manage access on their own file deserves to
  // be told the document is not shared rather than to find nothing there.
  await gotoEditor(page);
  const button = await reviewBand(page);
  await expect(button).toBeVisible();
  await expect(button).toBeDisabled();
  await expect(button).toHaveAttribute("title", /not shared/i);

  // Present on the other surfaces too, so the reason is reachable wherever the
  // reader looked for the command.
  expect(await inPalette(page)).toBe(true);
  expect(await inMenu(page)).toBe(true);
});

test("the access indicator says what this reader may do from first paint, with no room", async ({
  page,
}) => {
  // THE NON-ROOM HALF, which is the defect the owner named: "a protected or
  // read-only document opens in the full editing chrome and the reader learns the
  // truth from a toast AFTER typing." Nothing is typed here and nothing is
  // clicked — the assertion is about what is on screen when the document arrives.
  await gotoEditor(page);
  await expect(page.locator("#accessBadge")).toBeVisible();
  await expect(page.locator("#accessBadgeLevel")).toHaveText("Full access");
  await expect(page.locator("#accessBadgeSource")).toHaveText(/on this device/);
  // One sentence for assistive technology, carrying both halves, so a narrow
  // footer that sheds the attribution does not cost a screen reader the answer.
  await expect(page.locator("#accessBadge")).toHaveAttribute(
    "aria-label",
    /Full access.*on this device/,
  );
});

test("an embedded read-only container says so in the status bar, not after a keystroke", async ({
  page,
}) => {
  // The SDK half of the owner's decision — "for SDK or single user, the role is
  // pre-decided while loading the file" — made legible. `capabilities.mjs`
  // resolves the container grant from the URL before first paint, and this is
  // that state being stated rather than discovered.
  await gotoEditor(page, "&mode=readonly");
  await expect(page.locator("#accessBadgeLevel")).toHaveText("Read only");
  await expect(page.locator("#accessBadgeSource")).toHaveText(/when this document was opened/);
  // The RIBBON is deliberately not asserted here, and the reason is the point: a
  // `readonly` container has its editing chrome composed away entirely
  // (`capabilities.mjs`'s regions), so there is no Review band to look in. That is
  // exactly why the indicator has to be in the status bar — it is the one surface
  // every container keeps, and it is the only thing in this mode that says what
  // the reader may do.
  await expect(page.locator(".ribbon")).toBeHidden();
});

test("the rights dialog lists the room and says so when the room is empty", async ({ page }) => {
  // A room of one is real and common — one doc, one room — and the dialog opens
  // and SAYS SO rather than refusing to open, because a control that will not
  // open cannot deliver the information that there is nobody to change.
  await gotoEditor(page, asParticipant(OWNER));
  await (await reviewBand(page)).click();
  await expect(page.locator("#sessionRightsDialog")).toBeVisible();
  await expect(page.locator("#sessionRightsEmpty")).toBeVisible();
  await expect(page.locator("#sessionRightsList .rights-row")).toHaveCount(0);
  // Apply is disabled rather than offering a gesture with no effect.
  await expect(page.locator("#sessionRightsApply")).toBeDisabled();
  await page.keyboard.press("Escape");
  await expect(page.locator("#sessionRightsDialog")).toBeHidden();
});
