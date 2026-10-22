// Two real browsers through opendoc's own relay: the four co-editing defects a review
// reproduced this way (`docs/109` HF-310…HF-313), as guards that run the same way.
//
// Each test drives real key events in two browser contexts joined to one room by the
// repository's own `examples/share-host.mjs`, and reads the document back from the
// accessibility mirror — the text a reader's screen reader hears, not an engine call.
import { spawn } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect, MOD } from './fixtures.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const PARAGRAPHS = [
  'Alpha paragraph one for co-editing tests.',
  'Bravo paragraph two shared by both users.',
  'Charlie paragraph three at the end.',
];
let host, origin;
test.beforeAll(async () => {
  host = spawn(process.execPath, [resolve(root, 'examples/share-host.mjs')], { cwd: root, env: { ...process.env, OPENDOC_SHARE_PORT: '0' }, stdio: ['ignore', 'pipe', 'pipe'] });
  origin = await new Promise((done, fail) => {
    const timeout = setTimeout(() => fail(new Error('Share host startup timed out')), 10000);
    host.once('error', fail);
    host.once('exit', (code) => { clearTimeout(timeout); fail(new Error(`Share host exited: ${code}`)); });
    host.stdout.on('data', (bytes) => { const match = /Share host listening on (\d+)/.exec(String(bytes)); if (match) { clearTimeout(timeout); done(`http://127.0.0.1:${match[1]}`); } });
  });
});
test.afterAll(async () => { if (host && host.exitCode === null) { const exited = new Promise((done) => host.once('exit', done)); host.kill(); await exited; } });

async function ready(page) {
  await expect(page.locator('.page-wrap').first()).toBeVisible({ timeout: 45000 });
  await page.waitForFunction(() => document.body.dataset.fontsReady === 'true');
}
const paragraphs = (page) => page.evaluate(() => [...document.querySelector('#a11yDocument').children].map((c) => c.textContent));

/** Opens the fixture in a host tab, shares it, and joins a guest. `beforeGuest` runs on
 *  the guest page before it navigates, so a socket route is in place for its first frame. */
async function room(browser, { beforeGuest } = {}) {
  const hostContext = await browser.newContext();
  const a = await hostContext.newPage();
  await a.goto(`${origin}/editor.html`); await ready(a);
  await a.locator('#file').setInputFiles({ name: 'co.txt', mimeType: 'text/plain', buffer: Buffer.from(PARAGRAPHS.join('\n')) });
  if (await a.locator('#confirmDialog').isVisible().catch(() => false)) await a.locator('#confirmAccept').click();
  await expect(a.locator('#a11yDocument')).toContainText('Bravo paragraph two');
  await a.locator('#shareBtn').click();
  const dialog = a.locator('.shared-link-dialog'); await expect(dialog).toBeVisible();
  const link = await dialog.locator('input').inputValue();
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  const guestContext = await browser.newContext();
  const b = await guestContext.newPage();
  await beforeGuest?.(b);
  await b.goto(link); await ready(b);
  await expect(b.locator('#a11yDocument')).toContainText('Bravo paragraph two');
  await expect(b.locator('#collabSync')).toHaveAttribute('data-state', 'acknowledged');
  return { a, b, close: async () => { await guestContext.close(); await hostContext.close(); } };
}

/** The caret at `offset` of paragraph `index`, by keyboard: start, down, right. */
async function caretAt(page, index, offset) {
  await page.locator('.page-wrap .page').first().click({ position: { x: 60, y: 60 } });
  await page.keyboard.press(`${MOD}+Home`);
  for (let i = 0; i < index; i += 1) await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Home');
  if (offset === 'end') await page.keyboard.press('End');
  else for (let i = 0; i < offset; i += 1) await page.keyboard.press('ArrowRight');
}
async function typeSlowly(page, text, delay) {
  for (const ch of text) { await page.keyboard.type(ch); await page.waitForTimeout(delay); }
}
async function converged(a, b) {
  await expect.poll(async () => JSON.stringify(await paragraphs(a)) === JSON.stringify(await paragraphs(b)), { timeout: 20000 }).toBe(true);
  return paragraphs(a);
}

test('two people typing in one paragraph keep both words in order (HF-310)', async ({ browser }) => {
  test.setTimeout(150000);
  const { a, b, close } = await room(browser);
  try {
    await caretAt(b, 1, 'end');
    await caretAt(a, 1, 0);
    // The review's own pacing, 60 ms a key, both at once: the later typist's word used to
    // come out reversed on both screens.
    await Promise.all([typeSlowly(a, 'abcdef', 60), typeSlowly(b, 'UVWXYZ', 60)]);
    expect((await converged(a, b))[1]).toBe(`abcdef${PARAGRAPHS[1]}UVWXYZ`);
  } finally { await close(); }
});

test('a remote insert before the caret carries the caret with the text (HF-310)', async ({ browser }) => {
  test.setTimeout(150000);
  const { a, b, close } = await room(browser);
  try {
    await caretAt(b, 1, 5); // after "Bravo"
    await caretAt(a, 1, 0);
    await a.keyboard.type('pp');
    await expect(b.locator('#a11yDocument')).toContainText('ppBravo');
    await b.keyboard.type('ww');
    expect((await converged(a, b))[1]).toBe('ppBravoww paragraph two shared by both users.');
  } finally { await close(); }
});

test('undo in a shared document reverts only the reader\'s own text, for everyone (HF-311)', async ({ browser }) => {
  test.setTimeout(150000);
  const { a, b, close } = await room(browser);
  try {
    await caretAt(a, 2, 7);
    await a.keyboard.type('111');
    await expect(b.locator('#a11yDocument')).toContainText('Charlie111');
    await caretAt(b, 2, 'end');
    await b.keyboard.type('222');
    await expect(a.locator('#a11yDocument')).toContainText('end.222');
    await a.keyboard.press(`${MOD}+z`);
    expect((await converged(a, b))[2]).toBe(`${PARAGRAPHS[2]}222`);
  } finally { await close(); }
});

test('restoring a version while shared is refused with the reason, never applied to one copy (HF-312)', async ({ browser }) => {
  test.setTimeout(150000);
  const { a, b, close } = await room(browser);
  try {
    await caretAt(a, 0, 'end');
    await a.keyboard.type(' v2');
    await a.keyboard.press(`${MOD}+s`);
    await a.locator('#railVersions').click();
    const rows = a.locator('#versionPanelBody .version-item');
    await expect(rows.first()).toBeVisible();
    await rows.last().click();
    const restore = a.locator('#versionPreviewRestore');
    await expect(restore).toBeVisible();
    await expect(restore).toBeDisabled();
    await expect(restore).toHaveAttribute('title', /shared/);
    // Still one document, and still in the room.
    await expect(a.locator('#collabSync')).toBeVisible();
    expect(new URL(a.url()).searchParams.has('room')).toBe(true);
    expect((await converged(a, b))[0]).toBe(`${PARAGRAPHS[0]} v2`);
  } finally { await close(); }
});

test('the sync chip waits for the relay\'s acknowledgement and says when it is stuck (HF-313)', async ({ browser }) => {
  test.setTimeout(150000);
  let frozen = false;
  const held = [];
  let toPage = null;
  const { b, close } = await room(browser, {
    beforeGuest: (page) => page.routeWebSocket(/\/rooms\//, (ws) => {
      const server = ws.connectToServer();
      toPage = ws;
      ws.onMessage((message) => server.send(message));
      server.onMessage((message) => { if (frozen) held.push(message); else ws.send(message); });
    }),
  });
  try {
    frozen = true; // the relay still takes the chunk; its acknowledgement never comes back
    await caretAt(b, 1, 'end');
    await b.keyboard.type('!');
    await expect(b.locator('#collabSync')).toHaveAttribute('data-state', 'sent');
    await expect(b.locator('#collabSync')).not.toContainText('reaching everyone');
    await expect(b.locator('#collabSync')).toHaveAttribute('data-state', 'stuck', { timeout: 10000 });
    await expect(b.locator('#collabSync')).toContainText(/not been confirmed for \d+ s/);
    frozen = false;
    for (const message of held.splice(0)) toPage.send(message);
    await expect(b.locator('#collabSync')).toHaveAttribute('data-state', 'acknowledged');
    await expect(b.locator('#collabSync')).toContainText('reaching everyone');
  } finally { await close(); }
});
