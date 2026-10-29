// Content controls and forms (`docs/109` RM-34; `docs/153` page.contentControls,
// shell.form-fields, review.protect-form, shell.form-roles), driven the way a
// person drives them: the Insert band's Controls dropdown, typing, Alt+Down,
// the properties dialog and Protect form.
//
// Every assertion reads the DOCUMENT (the accessibility mirror, which is a
// projection of the engine's model) or a refusal the editor says out loud — a
// dialog that opened, or a menu row that was enabled, would pass a spec that
// only looked for chrome.
import { test, expect, clickIntoFirstPage, runAppMenuCommand, mirrorBlocks } from "./fixtures.mjs";

/** A blank document holding exactly `text`, caret at its end. */
async function blankDocumentWithText(page, text) {
  await page.goto("/editor.html?blank=1");
  await page.waitForFunction(
    () => document.getElementById("status")?.textContent.startsWith("Ready — open"),
    null,
    { timeout: 45_000 },
  );
  await runAppMenuCommand(page, "file", "file.new");
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 45_000 });
  await clickIntoFirstPage(page);
  await page.locator("#pages").pressSequentially(text);
  await expect.poll(async () => (await mirrorBlocks(page)).join("|")).toContain(text.trim());
}

async function insertControl(page, kind) {
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertControlsBtn").click();
  await expect(page.locator("#insertControlsMenu")).toBeVisible();
  await page.locator(`#insertControlsMenu [data-command="controls.insert.${kind}"]`).click();
  await expect(page.locator("#insertControlsMenu")).toBeHidden();
}

async function runControlsMenu(page, command) {
  await page.locator("#insertControlsBtn").click();
  await page.locator(`#insertControlsMenu [data-command="${command}"]`).click();
}

const bodyText = async (page) => (await mirrorBlocks(page)).join("\n");

test("a plain-text control holds Word's placeholder and the first keystroke replaces it", async ({
  page,
  consoleErrors,
}) => {
  await blankDocumentWithText(page, "Name: ");
  await insertControl(page, "plainText");
  await expect.poll(() => bodyText(page)).toContain("Name: Click or tap here to enter text.");

  await page.locator("#pages").pressSequentially("Ada");
  await expect.poll(() => bodyText(page)).toContain("Name: Ada");
  expect(await bodyText(page)).not.toContain("Click or tap");

  // Remove Content Control keeps the content, as Word's does.
  await page.keyboard.press("ArrowLeft");
  await runControlsMenu(page, "controls.remove");
  await expect(page.locator("#status")).toContainText("its contents stay");
  expect(await bodyText(page)).toContain("Name: Ada");

  expect(consoleErrors).toEqual([]);
});

test("a drop-down list is configured in its properties and filled with Alt+Down", async ({
  page,
  consoleErrors,
}) => {
  await blankDocumentWithText(page, "Answer: ");
  await insertControl(page, "dropDownList");
  await expect.poll(() => bodyText(page)).toContain("Answer: Choose an item.");

  await runControlsMenu(page, "controls.properties");
  const dialog = page.locator("#controlPropsDialog");
  await expect(dialog).toBeVisible();
  await page.locator("#controlPropsTitle").fill("Answer");
  await expect(page.locator("#controlPropsListRow")).toBeVisible();
  for (const entry of ["Yes", "No"]) {
    await page.locator("#controlPropsAddItem").click();
    await page.locator("#controlPropsList .control-item-row").last().locator(".control-item-display").fill(entry);
  }
  await page.locator("#controlPropsOk").click();
  await expect(dialog).toBeHidden();

  await page.locator("#pages").focus();
  await page.keyboard.press("Alt+ArrowDown");
  const fill = page.locator("#controlFillDialog");
  await expect(fill).toBeVisible();
  await expect(page.locator("#controlFillTitle")).toHaveText("Answer");
  await expect(page.locator("#controlFillList [role=option]")).toHaveText(["Yes", "No"]);
  await page.locator("#controlFillList [role=option]", { hasText: "No" }).click();
  await expect(fill).toBeHidden();
  await expect.poll(() => bodyText(page)).toContain("Answer: No");

  expect(consoleErrors).toEqual([]);
});

test("Protect form leaves only the fields editable, and says so", async ({ page, consoleErrors }) => {
  await blankDocumentWithText(page, "Name: ");
  await insertControl(page, "plainText");
  await page.locator("#pages").pressSequentially("Ada");
  await expect.poll(() => bodyText(page)).toContain("Name: Ada");

  await runControlsMenu(page, "review.protectForm");
  await expect(page.locator("#status")).toContainText("only its fields can be filled");

  // Outside the control: refused, out loud, and the document is unchanged.
  await page.locator("#pages").focus();
  await page.keyboard.press("Home");
  await page.locator("#pages").pressSequentially("Z");
  await expect(page.locator("#status")).toContainText("only its form fields can be edited");
  expect(await bodyText(page)).toContain("Name: Ada");
  expect(await bodyText(page)).not.toContain("ZName");

  // Inside it — the caret at its end is inside — filling still works.
  await page.keyboard.press("End");
  await page.locator("#pages").pressSequentially("x");
  await expect.poll(() => bodyText(page)).toContain("Name: Adax");

  // And it is a toggle: pressing it again lifts the protection.
  await runControlsMenu(page, "review.protectForm");
  await expect(page.locator("#status")).toContainText("Form protection removed");
  await page.locator("#pages").focus();
  await page.keyboard.press("Home");
  await page.locator("#pages").pressSequentially("Z");
  await expect.poll(() => bodyText(page)).toContain("ZName: Adax");

  expect(consoleErrors).toEqual([]);
});

test("a form role fills only its own fields while the form is protected", async ({ page, consoleErrors }) => {
  await blankDocumentWithText(page, "Approved by: ");
  await insertControl(page, "plainText");
  await page.locator("#pages").pressSequentially("Pat");

  // Two roles, and the control assigned to the second.
  await runControlsMenu(page, "forms.roles");
  const roles = page.locator("#formRolesDialog");
  await expect(roles).toBeVisible();
  await expect(roles).toContainText("not saved inside the .docx");
  await page.locator("#formRolesAdd").click();
  await page.locator("#formRolesList .form-role-name").last().fill("Employee");
  await page.locator("#formRolesAdd").click();
  await page.locator("#formRolesList .form-role-name").last().fill("Manager");
  await page.locator("#formRolesSave").click();
  await expect(roles).toBeHidden();

  await page.locator("#pages").focus();
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowLeft");
  await runControlsMenu(page, "controls.properties");
  await page.locator("#controlPropsRole").selectOption("Manager");
  await page.locator("#controlPropsOk").click();

  // The roles dialog filters the fields by role.
  await runControlsMenu(page, "forms.roles");
  await page.locator("#formRolesFilter").selectOption("Manager");
  await expect(page.locator("#formRolesFields .form-role-field")).toHaveCount(1);
  await page.locator("#formRolesFilter").selectOption("Employee");
  await expect(page.locator("#formRolesFields .form-role-field")).toHaveCount(0);
  // Fill as Employee, and protect the form.
  await page.locator("#formRolesFillAs").selectOption("Employee");
  await page.locator("#formRolesSave").click();
  await expect(page.locator("#status")).toContainText("Filling the form as Employee");
  await runControlsMenu(page, "review.protectForm");

  await page.locator("#pages").focus();
  await page.keyboard.press("End");
  await page.locator("#pages").pressSequentially("q");
  await expect(page.locator("#status")).toContainText("only its form fields can be edited");
  expect(await bodyText(page)).not.toContain("Patq");

  expect(consoleErrors).toEqual([]);
});
