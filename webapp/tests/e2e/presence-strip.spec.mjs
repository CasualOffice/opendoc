// Share and the people in the room, at the right end of the header (`docs/109`
// HF-330), in real browsers joined to one room through `examples/share-host.mjs`.
//
// The owner's report: Share sat in the middle of the header, beside the file
// name, and nobody else in the room was shown anywhere but on their caret. The
// guarantees held here are the ones a reader sees: Share is the last thing in the
// document row with the people immediately before it; each other person is a
// chip that names them honestly and takes the reader to their cursor; arrivals
// are said aloud; and at 390px the same controls are on screen.
import { spawn } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect, stableBox } from "./fixtures.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const TEXT = Array.from({ length: 90 }, (_, i) => `Line ${i + 1} of a document long enough to scroll.`).join('\n');
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

/** Opens the fixture in a host tab at `viewport`, shares it, and returns the link. */
async function shared(browser, viewport) {
  const context = await browser.newContext({ viewport });
  const a = await context.newPage();
  await a.goto(`${origin}/editor.html`); await ready(a);
  await a.locator('#file').setInputFiles({ name: 'room.txt', mimeType: 'text/plain', buffer: Buffer.from(TEXT) });
  if (await a.locator('#confirmDialog').isVisible().catch(() => false)) await a.locator('#confirmAccept').click();
  await expect(a.locator('#a11yDocument')).toContainText('Line 90');
  await expect(a.locator('#presenceStrip')).toBeHidden();
  await a.locator('#shareBtn').click();
  const dialog = a.locator('.shared-link-dialog'); await expect(dialog).toBeVisible();
  const link = await dialog.locator('input').inputValue();
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  return { a, link, context };
}
async function guest(browser, link) {
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto(link); await ready(page);
  await expect(page.locator('#collabSync')).toHaveAttribute('data-state', 'acknowledged');
  return { page, context };
}
/** The guest's caret, by keyboard, at the END of the document — far below A's view. */
async function caretAtEnd(page) {
  await page.locator('.page-wrap').first().click({ position: { x: 80, y: 120 } });
  await page.keyboard.press('ControlOrMeta+End');
}

test('Share is the last control in the header, the room is beside it, and a chip finds its person', async ({ browser }) => {
  test.setTimeout(150000);
  const { a, link, context } = await shared(browser, { width: 1440, height: 900 });
  const b = await guest(browser, link);
  try {
    // Placement: the last child of the document row, the strip immediately before it,
    // and nowhere inside the title row it used to be appended to.
    const order = await a.evaluate(() => {
      const row = document.querySelector('#shareBtn').parentElement;
      return { last: row.lastElementChild?.id, before: document.querySelector('#shareBtn').previousElementSibling?.id,
        inTitle: Boolean(document.querySelector('.document-title-row #shareBtn')) };
    });
    expect(order).toEqual({ last: 'shareBtn', before: 'presenceStrip', inTitle: false });
    const share = await stableBox(a.locator('#shareBtn'));
    const state = await stableBox(a.locator('#documentState'));
    expect(share.x).toBeGreaterThan(state.x + state.width);
    expect(1440 - (share.x + share.width)).toBeLessThan(40);

    // Two chips: yours first, then the other person's, named honestly.
    const chips = a.locator('#presenceStrip .presence-chips > .presence-chip');
    await expect(chips).toHaveCount(2);
    await expect(chips.first()).toHaveAttribute('role', 'img');
    await expect(chips.first()).toHaveAttribute('aria-label', /^You \(Participant \d+ — numbered by the room/);
    const other = a.getByRole('button', { name: /^Participant \d+ — numbered by the room, because this host does not share names\. Go to their cursor$/ });
    await expect(other).toHaveCount(1);
    await expect(other).toHaveText(/^P\d+$/);
    // The chip is the caret's colour.
    const participant = await other.getAttribute('data-participant');

    // The guest goes to the end; the host is at the top. Enter on the chip brings the
    // guest's caret into view.
    await caretAtEnd(b.page);
    const caret = a.locator(`.remote-caret[data-participant="${participant}"]`);
    await expect(caret).toHaveCount(1, { timeout: 15000 });
    await expect(caret).not.toBeInViewport();
    expect(await other.evaluate((el) => getComputedStyle(el).backgroundColor))
      .toBe(await caret.evaluate((el) => getComputedStyle(el).borderLeftColor));
    await other.focus();
    await a.keyboard.press('Enter');
    await expect(caret).toBeInViewport();

    // A third person arrives after the room has settled: said once, in the strip's
    // own live region; and said again when they go.
    // B's own arrival may already have been said (B loads after A's room settled), so
    // the sentence is pinned to the THIRD person's number, read off their new chip.
    // And the leave waits for the join to be said: a join and a leave inside one
    // window cancel out by design, which would say nothing at all.
    const before = await chips.evaluateAll((els) => els.map((el) => el.dataset.participant));
    const c = await guest(browser, link);
    await expect(chips).toHaveCount(3, { timeout: 15000 });
    const third = (await chips.evaluateAll((els) => els.map((el) => el.dataset.participant))).find((p) => !before.includes(p));
    const name = `Participant ${Number(third) + 1}`;
    await expect(a.locator('#presenceLive')).toHaveText(`${name} joined.`, { timeout: 15000 });
    await c.context.close();
    await expect(a.locator('#presenceLive')).toHaveText(`${name} left.`, { timeout: 15000 });
    await expect(chips).toHaveCount(2);
  } finally { await b.context.close(); await context.close(); }
});

test('at 390px the room is one button with a count, and Share keeps its place at the end', async ({ browser }) => {
  test.setTimeout(150000);
  const { a, link, context } = await shared(browser, { width: 390, height: 844 });
  const b = await guest(browser, link);
  try {
    await expect(a.locator('body')).toHaveClass(/phone-mode/);
    const people = a.locator('#presencePeople');
    await expect(people).toBeVisible();
    await expect(people).toHaveAccessibleName('2 people in this document');
    await expect(a.locator('#presenceStrip .presence-chips')).toBeHidden();
    const share = a.getByRole('button', { name: 'Share', exact: true });
    await expect(share).toBeVisible();
    await expect(share).toBeInViewport({ ratio: 1 });
    await expect(a.locator('#docTitle')).toBeInViewport({ ratio: 1 });
    expect(await a.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
    // The list it opens names everyone and is left with Escape, focus restored.
    await people.click();
    await expect(a.locator('#presenceList')).toBeVisible();
    await expect(a.locator('#presenceList')).toContainText('You (Participant');
    await expect(a.locator('#presenceList button')).toHaveCount(1);
    await a.keyboard.press('Escape');
    await expect(a.locator('#presenceList')).toBeHidden();
    await expect(people).toBeFocused();
  } finally { await b.context.close(); await context.close(); }
});
