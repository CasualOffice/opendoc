import { spawn } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect, clickIntoFirstPage, moveCaretToDocStart } from './fixtures.mjs';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
let host, origin;
test.beforeAll(async () => {
  host = spawn(process.execPath, [resolve(root,'examples/share-host.mjs')], {cwd:root,env:{...process.env,OPENDOC_SHARE_PORT:'0'},stdio:['ignore','pipe','pipe']});
  origin = await new Promise((done,fail) => {
    const timeout = setTimeout(() => fail(new Error('Share host startup timed out')),10000);
    host.once('error',fail);
    host.once('exit',code => {clearTimeout(timeout);fail(new Error(`Share host exited: ${code}`));});
    host.stdout.on('data',bytes => {const match = /Share host listening on (\d+)/.exec(String(bytes));if(match){clearTimeout(timeout);done(`http://127.0.0.1:${match[1]}`);}});
  });
});
test.afterAll(async () => {if(host && host.exitCode === null) {const exited = new Promise(done => host.once('exit',done));host.kill();await exited;}});
async function ready(page) {
  await expect(page.locator('.page-wrap').first()).toBeVisible({timeout:45000});
  await page.waitForFunction(() => document.body.dataset.fontsReady === 'true');
}
async function contains(page, marker) {
  await expect(page.locator('#a11yDocument')).toContainText(marker);
}
test('Share opens the document from a link, catches up late guests, and paints remote selections', async ({page,browser,consoleErrors}) => {
  test.setTimeout(120000);
  await page.goto(`${origin}/editor.html?fixture=rich`);await ready(page);
  await clickIntoFirstPage(page);await moveCaretToDocStart(page);
  await page.keyboard.type('SHAREDSEEDMARKER');
  await page.locator('#shareBtn').click();
  const dialog = page.locator('.shared-link-dialog');await expect(dialog).toBeVisible();
  const link = await dialog.locator('input').inputValue();
  expect(new URL(link).searchParams.has('document')).toBe(true);
  const seedUrl = new URL(link).searchParams.get('document');
  const seed = await (await page.request.get(seedUrl)).json();
  expect(seed.collabSnapshotVersion).toBe(1);
  expect(seed.resources['word/media/image1.png'].length).toBeGreaterThan(0);
  await dialog.getByRole('button',{name:'Close',exact:true}).click();
  // Host edits AFTER the saved seed; the guest must replay the relay's tail.
  await clickIntoFirstPage(page);await moveCaretToDocStart(page);await page.keyboard.type('ROOMTAILMARKER');
  const context = await browser.newContext(); const guest = await context.newPage();
  try {
    const errors = [];guest.on('pageerror',error => errors.push(String(error)));
    await guest.goto(link);await ready(guest);
    await contains(guest,'SHAREDSEEDMARKER');await contains(guest,'ROOMTAILMARKER');
    await clickIntoFirstPage(guest);await moveCaretToDocStart(guest);
    await guest.keyboard.type('GUESTEDITMARKER');
    await expect(page.locator('.remote-caret')).toHaveCount(1);
    await guest.keyboard.press('Shift+ArrowRight');await guest.keyboard.press('Shift+ArrowRight');
    await expect(page.locator('.remote-selection').first()).toBeVisible();
    await contains(page,'GUESTEDITMARKER');
    const documentUrl = new URL(link).searchParams.get('document');
    const id = /\/documents\/([^/]+)\.json$/.exec(new URL(documentUrl).pathname)[1];
    await expect.poll(async () => {const response=await page.request.get(`${origin}/api/shares/${id}/snapshot`);const snapshot=await response.json();return snapshot.document;}).toContain('GUESTEDITMARKER');
    const checkpoint = await (await page.request.get(`${origin}/api/shares/${id}/snapshot`)).json();
    expect(checkpoint.resources['word/media/image1.png']).toEqual(seed.resources['word/media/image1.png']);
    expect(errors).toEqual([]);
    await guest.close();await expect(page.locator('.remote-caret')).toHaveCount(0);
  } finally {await context.close();}
  await page.locator('#file').setInputFiles({name:'another.txt',mimeType:'text/plain',buffer:Buffer.from('ANOTHERDOCUMENT')});
  await expect(page.locator('#confirmDialog')).toBeVisible();await page.locator('#confirmAccept').click();
  await expect(page.locator('#a11yDocument')).toContainText('ANOTHERDOCUMENT');
  expect(new URL(page.url()).searchParams.has('room')).toBe(false);
  expect(new URL(page.url()).searchParams.has('document')).toBe(false);
  expect(consoleErrors).toEqual([]);
});
