// Shared version history in a room (`docs/175` §11, `docs/109` HF-362), in two real
// browsers joined through the repository's own `examples/share-host.mjs` in GRANTED mode.
//
// What it holds:
//   * a version B saves "for everyone" is listed on A, authored "Bob Stone" — the name the
//     HOST's session gives, since the client sends no author;
//   * A's own device versions appear only under "On this device", never in the shared grid,
//     and the shared grid holds only the room's;
//   * Restore is refused in the room, with the reason, on the row menu and on the preview bar
//     of a shared version that really opened (ADR-068, until HF-314).
import { spawn } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, expect } from './fixtures.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const TEXT = ['Alpha line for shared versions.', 'Bravo line both people see.', 'Charlie line at the end.'].join('\n');
const REFUSAL = "This document is shared, so restoring a version would change your copy and not everyone else's. Make a copy of this version or download it instead";

function startHost() {
  const host = spawn(process.execPath, [resolve(root, 'examples/share-host.mjs')], {
    cwd: root, env: { ...process.env, OPENDOC_SHARE_PORT: '0', OPENDOC_SHARE_ACCESS: 'granted' }, stdio: ['ignore', 'pipe', 'pipe'] });
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
async function person(browser, origin, cookie) {
  const context = await browser.newContext();
  await context.addCookies([{ name: 'opendoc_dev_user', value: encodeURIComponent(cookie), url: origin }]);
  return context;
}

const sharedRows = (page) => page.locator('#versionSharedList .version-item');
const deviceRows = (page) => page.locator('#versionPanelBody .version-item');

test.describe('shared version history in a granted room', () => {
  let host, origin;
  test.beforeAll(async () => { ({ host, origin } = startHost()); origin = await origin; });
  test.afterAll(async () => stopHost(host));

  test("lists B's version on A under B's attested name, keeps A's device versions apart, and refuses restore", async ({ browser }) => {
    test.setTimeout(180000);
    const ada = await person(browser, origin, 'u_ada|Ada Lovelace');
    const bob = await person(browser, origin, 'u_bob|Bob Stone');
    try {
      // A opens a document (a device version is captured on open) and shares it.
      const a = await ada.newPage();
      await a.goto(`${origin}/editor.html`); await ready(a);
      await identify(a, { id: 'u_ada', name: 'Ada Lovelace' });
      await a.locator('#file').setInputFiles({ name: 'room.txt', mimeType: 'text/plain', buffer: Buffer.from(TEXT) });
      if (await a.locator('#confirmDialog').isVisible().catch(() => false)) await a.locator('#confirmAccept').click();
      await expect(a.locator('#a11yDocument')).toContainText('Bravo line');
      await a.locator('#shareBtn').click();
      const dialog = a.locator('.shared-link-dialog'); await expect(dialog).toBeVisible();
      const link = await dialog.locator('input').inputValue();
      await dialog.getByRole('button', { name: 'Close', exact: true }).click();

      // B joins.
      const b = await bob.newPage();
      const url = new URL(link); url.searchParams.set('awaitIdentity', '1');
      await b.goto(url.href);
      await identify(b, { id: 'u_bob', name: 'Bob Stone' });
      await ready(b);
      await expect(b.locator('#collabSync')).toHaveAttribute('data-state', 'acknowledged');

      // B saves a version for everyone, named, through the panel's own control.
      await b.locator('#railVersions').click();
      await expect(b.locator('#versionSharedHeading')).toHaveText('Shared versions');
      await expect(b.locator('#versionSharedList')).toContainText('No shared versions yet');
      const save = b.locator('#versionSaveShared');
      await expect(save).toHaveText('Save a version for everyone');
      await save.click();
      await expect(b.locator('#versionNameTitle')).toHaveText('Save a version for everyone');
      await b.locator('#versionNameInput').fill('Checkpoint from Bob');
      await b.locator('#versionNameConfirm').click();
      await expect(sharedRows(b)).toHaveCount(1);
      // The local dialog's wording is put back once the shared prompt closes.
      await expect(b.locator('#versionNameTitle')).toHaveText('Name this version');

      // A sees it, authored by the name the HOST's session gave — not anything B's tab said.
      await a.locator('#railVersions').click();
      await expect(a.locator('#versionSharedHeading')).toHaveText('Shared versions');
      await expect(sharedRows(a)).toHaveCount(1);
      const row = sharedRows(a).first();
      await expect(row).toContainText('Checkpoint from Bob');
      await expect(row.locator('.version-item-detail')).toContainText('by Bob Stone');
      await expect(row.locator('.version-item-entry')).toHaveAttribute('aria-label', /by Bob Stone.*Shared versions$/);

      // A's device versions: present, under "On this device", and only there.
      await expect(a.locator('#versionDeviceHeading')).toBeVisible();
      await expect(a.locator('#versionDeviceHeading')).toHaveText('On this device');
      await expect(a.locator('#versionPanelBody')).toHaveAttribute('aria-labelledby', 'versionDeviceHeading');
      await expect(deviceRows(a).first()).toBeVisible();
      const deviceIds = await deviceRows(a).evaluateAll((els) => els.map((el) => el.dataset.versionId));
      const sharedIds = await sharedRows(a).evaluateAll((els) => els.map((el) => el.dataset.versionId));
      expect(deviceIds.length).toBeGreaterThan(0);
      expect(deviceIds.every((id) => !id.startsWith('shared:'))).toBe(true);
      expect(sharedIds.every((id) => id.startsWith('shared:'))).toBe(true);
      await expect(deviceRows(a).filter({ hasText: 'Checkpoint from Bob' })).toHaveCount(0);
      for (const label of await deviceRows(a).locator('.version-item-entry').evaluateAll((els) => els.map((el) => el.getAttribute('aria-label')))) {
        expect(label).toMatch(/On this device$/);
      }

      // Restore is refused in the room, with the reason: on the shared row's menu…
      await row.locator('.version-item-menu').click();
      const restore = a.locator('#versionRowMenu [data-command-id="version.restore"]');
      await expect(restore).toBeDisabled();
      await expect(restore).toHaveAttribute('title', REFUSAL);
      await a.keyboard.press('Escape');
      await expect(a.locator('#versionRowMenu')).toBeHidden();

      // …and on the preview bar of the shared version, which really opens (its bytes come
      // from the host's `versionBytes`).
      await row.locator('.version-item-entry').click();
      await expect(a.locator('#versionPreviewBanner')).toBeVisible();
      await expect(a.locator('#a11yDocument')).toContainText('Bravo line');
      const bannerRestore = a.locator('#versionPreviewRestore');
      await expect(bannerRestore).toBeDisabled();
      await expect(bannerRestore).toHaveAttribute('title', REFUSAL);
      await a.locator('#versionPreviewBack').click();
      await expect(a.locator('#versionPreviewBanner')).toBeHidden();
    } finally { await bob.close(); await ada.close(); }
  });
});
