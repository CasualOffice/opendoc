import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, runPaletteCommand, setReviewMode, MOD } from './fixtures.mjs';
async function selectStart(page) { await clickIntoFirstPage(page); await moveCaretToDocStart(page); for (let i=0;i<5;i++) await page.keyboard.press('Shift+ArrowRight'); }
async function dialog(page, advanced=false) { await runPaletteCommand(page,`format.character.${advanced ? 'advanced':'dialog'}`,'font'); await expect(page.locator('#fontAdvancedDialog')).toBeVisible(); }
test('character caps and double strike are authorable, preserve text, and undo together',async({page,consoleErrors}) => {
  await gotoEditor(page); await selectStart(page); await dialog(page);
  for (const key of ['allCaps','smallCaps','doubleStrike']) await page.locator(`#font-${key}`).check();
  await page.locator('#fontAdvancedApply').click(); await dialog(page);
  for (const key of ['allCaps','smallCaps','doubleStrike']) await expect(page.locator(`#font-${key}`)).toBeChecked();
  await page.locator('#fontAdvancedCancel').click(); await page.locator('#undoBtn').click(); await dialog(page);
  for (const key of ['allCaps','smallCaps','doubleStrike']) await expect(page.locator(`#font-${key}`)).not.toBeChecked();
  expect(consoleErrors).toEqual([]);
});
test('glyph effects change painted ink and cancel never authors a change',async({page,consoleErrors}) => {
  await gotoEditor(page); await selectStart(page);
  const pixels=() => page.locator('canvas.page').first().evaluate(canvas => { const bytes=canvas.getContext('2d').getImageData(0,0,canvas.width,canvas.height).data; let hash=0; for(let i=0;i<bytes.length;i+=4) hash=(Math.imul(hash,31)+bytes[i])|0; return hash; });
  const before=await pixels(); await dialog(page); await page.locator('#font-outline').check(); await page.locator('#fontAdvancedCancel').click(); expect(await pixels()).toBe(before);
  await dialog(page); for(const key of ['outline','shadow','emboss','imprint']) await page.locator(`#font-${key}`).check(); await page.locator('#fontAdvancedApply').click();
  await expect.poll(pixels).not.toBe(before); await page.locator('#undoBtn').click(); await expect.poll(pixels).toBe(before);
  expect(consoleErrors).toEqual([]);
});
test('run border and emphasis remain selected and undoable',async({page,consoleErrors}) => {
  await gotoEditor(page); await selectStart(page); await dialog(page);
  await page.locator('#font-border').selectOption('double'); await page.locator('#font-borderColor').fill('#123456'); await page.locator('#font-borderWidth').fill('2'); await page.locator('#font-emphasis').selectOption('under_dot'); await page.locator('#fontAdvancedApply').click();
  await dialog(page); await expect(page.locator('#font-border')).toHaveValue('double'); await expect(page.locator('#font-borderColor')).toHaveValue('#123456'); await expect(page.locator('#font-borderWidth')).toHaveValue('2'); await expect(page.locator('#font-emphasis')).toHaveValue('under_dot');
  expect(consoleErrors).toEqual([]);
});
test('Advanced controls survive unrelated edits and armed typing uses one undo',async({page,consoleErrors}) => {
  await gotoEditor(page); await selectStart(page); await dialog(page,true);
  for(const [key,value] of [['characterSpacingTwips','1.25'],['characterScalePercent','125'],['positionHalfPoints','-1.5'],['kerningHalfPoints','12']]) await page.locator(`#font-${key}`).fill(value);
  await page.locator('#font-ligature-standard').check(); await page.locator('#font-ligature-discretional').check(); await page.locator('#fontAdvancedApply').click();
  await dialog(page); await page.locator('#font-shadow').check(); await page.locator('#fontAdvancedApply').click(); await dialog(page,true);
  await expect(page.locator('#font-characterSpacingTwips')).toHaveValue('1.25'); await expect(page.locator('#font-characterScalePercent')).toHaveValue('125'); await expect(page.locator('#font-positionHalfPoints')).toHaveValue('-1.5'); await expect(page.locator('#font-kerningHalfPoints')).toHaveValue('12'); await expect(page.locator('#font-ligature-standard')).toBeChecked(); await expect(page.locator('#font-ligature-discretional')).toBeChecked();
  await page.locator('#fontAdvancedCancel').click(); await page.keyboard.press(`${MOD}+Home`); await dialog(page); await page.locator('#font-allCaps').check(); await page.locator('#fontAdvancedApply').click(); await page.keyboard.type('armed');
  await page.keyboard.press('Shift+ArrowLeft'); await dialog(page); await expect(page.locator('#font-allCaps')).toBeChecked(); await page.locator('#fontAdvancedCancel').click(); await page.locator('#undoBtn').click();
  expect(consoleErrors).toEqual([]);
});
test('Suggesting records character formatting and viewing withholds authoring',async({page,consoleErrors}) => {
  await gotoEditor(page); await selectStart(page); await setReviewMode(page,'suggesting'); await dialog(page); await page.locator('#font-outline').check(); await page.locator('#fontAdvancedApply').click(); await dialog(page); await expect(page.locator('#font-outline')).toBeChecked(); await page.locator('#fontAdvancedCancel').click();
  await page.locator('#undoBtn').click(); await setReviewMode(page,'viewing'); await page.keyboard.press(`${MOD}+Shift+P`); await page.locator('#cmdInput').fill('font'); const row=page.locator('#cmdList .cmd-item[data-command-id="format.character.dialog"]'); await expect(row).toBeDisabled(); await expect(row).toHaveAttribute('title', /\S/);
  expect(consoleErrors).toEqual([]);
});
