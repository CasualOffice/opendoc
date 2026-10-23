// Host identity in a shared room (`docs/175`, `docs/109` HF-360), in real browsers joined
// through the repository's own `examples/share-host.mjs` and its relay.
//
// Two rooms: a GRANTED one, where the relay verifies a host-signed JWT and shows the grant's
// name, and an OPEN one, where a name is the participant's own claim and is shown marked
// unverified. Identity reaches the editor the in-process way (`window.opendoc.identify`),
// never through a URL.
import { spawn } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect } from './fixtures.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const TEXT = ['Alpha line for identity tests.', 'Bravo line shared by both people.', 'Charlie line at the end.'].join('\n');

function startHost(access) {
  const host = spawn(process.execPath, [resolve(root, 'examples/share-host.mjs')], {
    cwd: root, env: { ...process.env, OPENDOC_SHARE_PORT: '0', OPENDOC_SHARE_ACCESS: access }, stdio: ['ignore', 'pipe', 'pipe'] });
  const origin = new Promise((done, fail) => {
    const timeout = setTimeout(() => fail(new Error('Share host startup timed out')), 10000);
    host.once('error', fail);
    host.once('exit', (code) => { clearTimeout(timeout); fail(new Error(`Share host exited: ${code}`)); });
    host.stdout.on('data', (bytes) => { const match = /Share host listening on (\d+)/.exec(String(bytes)); if (match) { clearTimeout(timeout); done(`http://127.0.0.1:${match[1]}`); } });
  });
  return { host, origin };
}
async function stopHost(host) {
  if (host && host.exitCode === null) { const exited = new Promise((done) => host.once('exit', done)); host.kill(); await exited; }
}

async function ready(page) {
  await expect(page.locator('.page-wrap').first()).toBeVisible({ timeout: 45000 });
  await page.waitForFunction(() => document.body.dataset.fontsReady === 'true');
}
async function identify(page, identity) {
  await page.waitForFunction(() => typeof window.opendoc?.identify === 'function');
  const answer = await page.evaluate((value) => window.opendoc.identify(value), identity);
  expect(answer.ok, JSON.stringify(answer)).toBe(true);
}
/** A context whose dev-host session says who this is (the granted host signs for it). */
async function person(browser, origin, cookie) {
  const context = await browser.newContext();
  if (cookie) await context.addCookies([{ name: 'opendoc_dev_user', value: encodeURIComponent(cookie), url: origin }]);
  return context;
}
async function share(context, origin, identity) {
  const a = await context.newPage();
  await a.goto(`${origin}/editor.html`); await ready(a);
  await identify(a, identity);
  await a.locator('#file').setInputFiles({ name: 'room.txt', mimeType: 'text/plain', buffer: Buffer.from(TEXT) });
  if (await a.locator('#confirmDialog').isVisible().catch(() => false)) await a.locator('#confirmAccept').click();
  await expect(a.locator('#a11yDocument')).toContainText('Bravo line');
  await a.locator('#shareBtn').click();
  const dialog = a.locator('.shared-link-dialog'); await expect(dialog).toBeVisible();
  const link = await dialog.locator('input').inputValue();
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  return { a, link };
}
/** Joins `link` as `identity`, which the page is told before its room opens. */
async function join(context, link, identity) {
  const page = await context.newPage();
  const url = new URL(link); url.searchParams.set('awaitIdentity', '1');
  await page.goto(url.href);
  await identify(page, identity);
  await ready(page);
  return page;
}
const chips = (page) => page.locator('#presenceStrip .presence-chips > .presence-chip');

test.describe('a granted room', () => {
  let host, origin;
  test.beforeAll(async () => { ({ host, origin } = startHost('granted')); origin = await origin; });
  test.afterAll(async () => stopHost(host));

  test('shows the names the host signed, on chips and on carets', async ({ browser }) => {
    test.setTimeout(150000);
    const ada = await person(browser, origin, 'u_ada|Ada Lovelace');
    const bob = await person(browser, origin, 'u_bob|Bob Stone');
    try {
      const { a, link } = await share(ada, origin, { id: 'u_ada', name: 'Ada Lovelace' });
      const b = await join(bob, link, { id: 'u_bob', name: 'Bob Stone' });
      await expect(b.locator('#collabSync')).toHaveAttribute('data-state', 'acknowledged');
      // Attested names: no "unverified" mark, no "numbered by the room".
      await expect(a.getByRole('button', { name: 'Bob Stone. Go to their cursor', exact: true })).toHaveCount(1);
      await expect(b.getByRole('button', { name: 'Ada Lovelace. Go to their cursor', exact: true })).toHaveCount(1);
      await expect(chips(b).first()).toHaveAttribute('aria-label', 'You (Bob Stone)');
      // The caret label reads the same seam.
      await b.locator('.page-wrap').first().click({ position: { x: 80, y: 100 } });
      await expect(a.locator('.remote-caret span').first()).toHaveText('Bob Stone', { timeout: 20000 });
    } finally { await bob.close(); await ada.close(); }
  });

  test('refuses a participant presenting somebody else\'s identity', async ({ browser }) => {
    test.setTimeout(150000);
    const ada = await person(browser, origin, 'u_ada|Ada Lovelace');
    // Mallory's host session is Mallory's, so the host signs a grant for Mallory; the editor is
    // then told it is Ada. The relay compares the two and refuses.
    const mallory = await person(browser, origin, 'u_mallory|Mallory');
    try {
      const { a, link } = await share(ada, origin, { id: 'u_ada', name: 'Ada Lovelace' });
      const m = await join(mallory, link, { id: 'u_ada', name: 'Ada Lovelace' });
      // Refused with the terminal "not authorised": the session stops rather than retrying.
      await expect(m.locator('#collabSync')).toHaveAttribute('data-state', 'stopped', { timeout: 20000 });
      // And Ada's room never showed an impostor: she is alone in it.
      await expect(chips(a)).toHaveCount(1);
    } finally { await mallory.close(); await ada.close(); }
  });
});

test.describe('an open room', () => {
  let host, origin;
  test.beforeAll(async () => { ({ host, origin } = startHost('open')); origin = await origin; });
  test.afterAll(async () => stopHost(host));

  test('marks a claimed name unverified and suffixes a duplicate on screen only', async ({ browser }) => {
    test.setTimeout(150000);
    const first = await person(browser, origin);
    const second = await person(browser, origin);
    try {
      const { a, link } = await share(first, origin, { id: 'u_ada', name: 'Ada Lovelace' });
      const b = await join(second, link, { id: 'u_other', name: 'Ada Lovelace' });
      await expect(b.locator('#collabSync')).toHaveAttribute('data-state', 'acknowledged');
      // On A's screen the other "Ada Lovelace" is a claim, and a second one: both said.
      const other = a.getByRole('button', {
        name: 'Ada Lovelace (2) — unverified: this name was given by the person, not confirmed by the host. Go to their cursor', exact: true });
      await expect(other).toHaveCount(1);

      // B comments. The stored author is the name exactly as given — no "(2)" in the document.
      await b.locator('.page-wrap').first().click({ position: { x: 80, y: 100 } });
      await b.keyboard.press('ControlOrMeta+Home');
      for (let i = 0; i < 5; i += 1) await b.keyboard.press('Shift+ArrowRight');
      await b.locator('#selComment').click();
      const composer = b.locator('#reviewSidebar [data-testid="review-comment-composer"]');
      await expect(composer).toBeVisible();
      await composer.fill('From the second Ada');
      await b.locator('#reviewSidebar [data-testid="review-comment-submit"]').click();
      const card = a.locator('#reviewSidebar .review-margin-card.review-margin-comment').filter({ hasText: 'From the second Ada' });
      await expect(card).toBeVisible({ timeout: 20000 });
      await expect(card.locator('.review-margin-card-head strong')).toHaveText('Ada Lovelace');
    } finally { await second.close(); await first.close(); }
  });
});
