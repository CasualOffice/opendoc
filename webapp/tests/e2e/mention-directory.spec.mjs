// @mention over the host's directory (`docs/175` §7, `docs/109` HF-361), in the real
// comment composer. The host is the in-process one (`window.opendoc.directory`), so
// the spec can see exactly what the editor asked it and when.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD, waitForFramedEditor } from "./fixtures.mjs";

/** Installs a directory that answers from a small list and records every call. */
async function installDirectory(page, { status = "needs-access" } = {}) {
  await page.evaluate((answer) => {
    const people = [
      { id: "u_grace", name: "Grace Hopper", link: "mailto:grace@example.org" },
      { id: "u_greta", name: "Greta Garbo" },
      { id: "u_ada", name: "Ada Lovelace", link: "mailto:ada@example.org" },
    ];
    window.__directory = { queries: [], notices: [] };
    window.opendoc.directory = {
      search(query, { limit }) {
        window.__directory.queries.push({ query, limit });
        const q = query.toLowerCase();
        return { people: people.filter((p) => p.name.toLowerCase().includes(q)).slice(0, limit), more: false };
      },
      notify(notice) {
        window.__directory.notices.push(notice);
        return { status: answer };
      },
    };
  }, status);
}

async function openComposer(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  for (let i = 0; i < 5; i += 1) await page.keyboard.press("Shift+ArrowRight");
  await page.locator("#selComment").click();
  const composer = page.locator('#reviewSidebar [data-testid="review-comment-composer"]');
  await expect(composer).toBeVisible();
  await composer.focus();
  return composer;
}

test("@ asks the host's directory, a pick links the person, and the host is told only once the comment is saved", async ({ page }) => {
  await gotoEditor(page);
  await installDirectory(page);
  const composer = await openComposer(page);
  await composer.pressSequentially("Please check @Gr");
  const list = page.locator(".mention-list");
  await expect(list).toBeVisible();
  await expect(list.getByRole("option")).toHaveText([/Grace Hopper/, /Greta Garbo/]);
  // The query and nothing else, after typing settled, bounded to 20.
  const asked = await page.evaluate(() => window.__directory.queries);
  expect(asked.every((q) => q.limit === 20 && q.query.length >= 1 && q.query.length <= 2)).toBe(true);
  expect(asked.at(-1).query).toBe("Gr");
  await page.keyboard.press("Enter"); // picks, does not submit
  await expect(composer).toHaveValue("Please check @Grace Hopper ");
  await expect(list).toBeHidden();
  await composer.pressSequentially("today");
  await page.locator('#reviewSidebar [data-testid="review-comment-submit"]').click();
  const card = page.locator("#reviewSidebar .review-margin-card.review-margin-comment").filter({ hasText: "@Grace Hopper today" });
  await expect(card).toBeVisible();
  // Not yet ordered: a standalone comment is ordered when it is saved.
  expect(await page.evaluate(() => window.__directory.notices.length)).toBe(0);
  const download = page.waitForEvent("download");
  await page.keyboard.press(`${MOD}+s`);
  await download;
  await expect.poll(() => page.evaluate(() => window.__directory.notices)).toEqual([
    expect.objectContaining({ people: ["u_grace"], excerpt: "Please check @Grace Hopper today", room: null }),
  ]);
  // The host's answer is said to the reader.
  await expect(page.locator("#status")).toContainText("Grace Hopper can’t open this document");
});

test("with no directory, @ is just text: no list, and nothing is asked", async ({ page }) => {
  await gotoEditor(page);
  const composer = await openComposer(page);
  await composer.pressSequentially("Ping @Gr");
  await page.waitForTimeout(400); // longer than the debounce, so a query would have gone out
  await expect(page.locator(".mention-list")).toHaveCount(0);
  await expect(composer).not.toHaveAttribute("aria-autocomplete", "list");
});

test("Escape closes the list and leaves the composer open", async ({ page }) => {
  await gotoEditor(page);
  await installDirectory(page);
  const composer = await openComposer(page);
  await composer.pressSequentially("Hi @Ad");
  await expect(page.locator(".mention-list").getByRole("option")).toHaveText([/Ada Lovelace/]);
  await expect(composer).toHaveAttribute("aria-expanded", "true");
  await page.keyboard.press("Escape");
  await expect(page.locator(".mention-list")).toBeHidden();
  await expect(composer).toBeVisible();
  await expect(composer).toHaveValue("Hi @Ad");
});

test("an <opendoc-editor>'s directory property answers the framed editor over postMessage", async ({ page }) => {
  await page.goto("/embed.html");
  await page.evaluate(() => {
    window.__asked = [];
    window.__told = [];
    const element = document.createElement("opendoc-editor");
    element.dataset.mentionDemo = "";
    element.setAttribute("editor-src", "./editor.html");
    element.setAttribute("frame-title", "Mentions through the element");
    element.directory = {
      search(query, { limit }) {
        window.__asked.push({ query, limit });
        return { people: [{ id: "u_grace", name: "Grace Hopper", link: "mailto:grace@example.org" }], more: false };
      },
      notify(notice) { window.__told.push(notice); return { status: "notified" }; },
    };
    document.querySelector('[data-embed="element"] .embed-stage').append(element);
  });
  const frame = await (await page.locator("[data-mention-demo] iframe").elementHandle()).contentFrame();
  await waitForFramedEditor(frame);
  // The frame was told a directory exists — and nothing else about it — in its URL.
  expect(new URL(frame.url()).searchParams.get("directory")).toBe("1");
  // The framed editor's own seam asks; the element's directory answers, by qid.
  const answer = await frame.evaluate(() => window.opendoc.mentions.search("Gr"));
  expect(answer.people).toEqual([{ id: "u_grace", name: "Grace Hopper", link: "mailto:grace@example.org" }]);
  expect(await page.evaluate(() => window.__asked)).toEqual([{ query: "Gr", limit: 20 }]);
  const status = await frame.evaluate(() => window.opendoc.mentions.notify({ people: ["u_grace"], commentId: "c", excerpt: "hi", room: null }));
  expect(status).toBe("notified");
  expect(await page.evaluate(() => window.__told.map((n) => n.people))).toEqual([["u_grace"]]);
});
