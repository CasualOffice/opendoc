import { test, expect, gotoEditor, clickIntoFirstPage, runPaletteCommand, documentPageCount, setReviewMode } from './fixtures.mjs';
async function listPanel(page) {
  await runPaletteCommand(page, 'paragraph.list.settings', 'list settings');
  await expect(page.locator('#paragraphPropertiesPanel')).toBeVisible();
}
test('list definition settings change the caret list and undo restores its saved values', async ({page, consoleErrors}) => {
  await gotoEditor(page); await clickIntoFirstPage(page);
  await page.locator('#numberedList').click();
  await listPanel(page);
  const original = await page.locator('#listAdvancedStart').inputValue();
  await page.locator('#listAdvancedStart').fill('7');
  await page.locator('#listAdvancedApply').click();
  await page.locator('#paragraphPropertiesClose').click();
  await listPanel(page);
  await expect(page.locator('#listAdvancedStart')).toHaveValue('7');
  await page.locator('#paragraphPropertiesClose').click();
  await page.locator('#undoBtn').click();
  await listPanel(page);
  await expect(page.locator('#listAdvancedStart')).toHaveValue(original);
  expect(consoleErrors).toEqual([]);
});
test('page presets, unequal columns, mirror and color remain undoable through page setup', async ({page, consoleErrors}) => {
  await gotoEditor(page); await clickIntoFirstPage(page);
  await runPaletteCommand(page, 'layout.pageSetup', 'page setup');
  await page.locator('#pagePaperPreset').selectOption('legal');
  await page.locator('#pageMarginsPreset').selectOption('narrow');
  await page.locator('#pageColumnCount').selectOption('2');
  await page.locator('#pageColumnEqual').uncheck();
  await page.locator('#pageColumnWidth0').fill('2');
  await page.locator('#pageColumnWidth1').fill('5');
  await page.locator('#pageColumnSpace0').fill('0.5');
  await page.locator('#pageSetupApply').click();
  await runPaletteCommand(page, 'layout.pageSetup', 'page setup');
  await expect(page.locator('#pageColumnEqual')).not.toBeChecked();
  await expect(page.locator('#pageColumnWidth0')).toHaveValue('2');
  await expect(page.locator('#pageColumnWidth1')).toHaveValue('5');
  await page.locator('#pageMirrorMargins').check(); await page.locator('#pageMirrorApply').click();
  await page.locator('#pageBackgroundColor').fill('#123456'); await page.locator('#pageBackgroundApply').click();
  await page.locator('#pageSetupClose').click();
  await runPaletteCommand(page, 'layout.pageSetup', 'page setup');
  await expect(page.locator('#pageMirrorMargins')).toBeChecked();
  await expect(page.locator('#pageBackgroundColor')).toHaveValue('#123456');
  const corner = () => page.locator('canvas.page').first().evaluate(canvas => [...canvas.getContext('2d').getImageData(10, 10, 1, 1).data]);
  await expect.poll(corner).toEqual([18, 52, 86, 255]);
  await page.locator('#pageSetupClose').click();
  await page.locator('#undoBtn').click();
  await expect.poll(corner).toEqual([255, 255, 255, 255]);
  await page.locator('#redoBtn').click();
  await expect.poll(corner).toEqual([18, 52, 86, 255]);
  expect(consoleErrors).toEqual([]);
});
test('blank page is a single undo action and list definitions refuse suggesting edits', async ({page, consoleErrors}) => {
  await gotoEditor(page); await clickIntoFirstPage(page);
  const count = await documentPageCount(page);
  await runPaletteCommand(page, 'layout.blankPage', 'blank page');
  await expect.poll(() => documentPageCount(page)).toBe(count + 2);
  await page.locator('#undoBtn').click();
  await expect.poll(() => documentPageCount(page)).toBe(count);
  await page.locator('#numberedList').click(); await listPanel(page);
  const original = await page.locator('#listAdvancedStart').inputValue();
  await page.locator('#paragraphPropertiesClose').click(); await setReviewMode(page, 'suggesting');
  await listPanel(page); await page.locator('#listAdvancedStart').fill('9'); await page.locator('#listAdvancedApply').click();
  await page.locator('#paragraphPropertiesClose').click(); await listPanel(page);
  await expect(page.locator('#listAdvancedStart')).toHaveValue(original);
  expect(consoleErrors).toEqual([]);
});

test('a custom picture bullet paints its image and disappears with one undo', async ({page, consoleErrors}) => {
  await gotoEditor(page); await clickIntoFirstPage(page); await page.locator('#numberedList').click();
  await listPanel(page);
  await page.locator('#listAdvancedPicture').setInputFiles({ name: 'red-bullet.png', mimeType: 'image/png', buffer: Buffer.from('iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAIAAACQkWg2AAAAF0lEQVR4nGP4z8BAEiJN9aiGUQ1DSgMAkPn/Afnh+ngAAAAASUVORK5CYII=', 'base64') });
  await page.locator('#listAdvancedPictureApply').click();
  const redPixels = () => page.locator('canvas.page').first().evaluate(canvas => {
    const {data} = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height);
    let count = 0; for (let i = 0; i < data.length; i += 4) if (data[i] > 240 && data[i+1] < 20 && data[i+2] < 20) count++;
    return count;
  });
  await expect.poll(redPixels).toBeGreaterThan(0);
  await page.locator('#paragraphPropertiesClose').click(); await page.locator('#undoBtn').click();
  await expect.poll(redPixels).toBe(0);
  expect(consoleErrors).toEqual([]);
});
