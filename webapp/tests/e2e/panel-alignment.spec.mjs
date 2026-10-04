// The side panels share one shell, measured (HF-233 D1/D2).
//
// The owner's report was that the chrome "looks like stock HTML: top left and
// flow bottom". The dialogs were not the problem — all 23 go through
// `registerModal` and take their padding from `--dlg-pad-x`/`--dlg-pad-y`, and
// they measure identical left edges. The nine side panels went through no
// shared shell at all: `.side-panel` was a width plus a margin plus a border,
// `.panel-head` said `padding: 0 8px 0 14px`, `.panel-body` said
// `padding: 6px`, and five panels then said something else again.
//
// Measured on the parent of this branch, at 1280x720:
//
//   outlinePanel     title text 101.09   body content box  93.09   step  8
//   pagesPanel       title text 101.09   body content box  93.09   step  8
//   comparePanel     title text 101.09   body content box  93.09   step  8
//   versionPanel     title text 967      body content box 959      step  8
//   symbolDialog     title text 925      body content box 923      step  2
//   emojiDialog      title text 925      body content box 923      step  2
//   paragraph props  title text 1004     body content box 967      step 37
//   table props      title text 1004     body content box 967      step 37
//
// WHAT THIS ASSERTS, AND WHY IT IS THE GUARANTEE AND NOT THE MEASUREMENT.
//
// "The title's text starts where the body's content starts" is a property the
// design has to hold at every width, in every locale, at any panel width. "The
// left edge is 101px" is a number that moves when the rail gains a caption or
// the panel changes width, and a guard pinned to it reddens `main` on a change
// that removes nothing. So nothing here is a pixel: every assertion is one
// measured edge against another measured edge, in the same frame.
//
// A row INSIDE the body may still carry its own inset — the outline tree's 18px
// disclosure gutter, a version row's hit target, a fieldset's border. That is
// the row's business, exactly as a `.dialog-group`'s inset is inside a dialog
// body, and Word's Navigation pane and VS Code's explorer both indent a
// root-level label past the chevron column for the same reason. What the eye
// reads as "the panel is not aligned" is the SHELL — head against body — and
// that is what this measures.
import { readFileSync } from "node:fs";
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  MOD,
  runPaletteCommand,
} from "./fixtures.mjs";

const LAPTOP = { width: 1280, height: 720 };

/** Every `.side-panel` in the product, with the route that opens it.
 *
 *  Nine of them: eight declared in `editor.html` and the object inspector,
 *  which `object_inspector.mjs` builds at runtime with the same
 *  `.panel-head`/`.panel-body` markup — and which carried the same
 *  `.properties-panel-icon` the properties panels did. The last test below
 *  fails if a tenth appears and is not listed here, so this roster cannot fall
 *  behind the product the way `dialog-fit`'s did. */
const PANELS = [
  {
    id: "outlinePanel",
    name: "Outline",
    async open(page) {
      await page.locator("#railOutline").click();
    },
  },
  {
    id: "pagesPanel",
    name: "Pages",
    async open(page) {
      await page.locator("#railPages").click();
    },
  },
  {
    id: "comparePanel",
    name: "Compare",
    async open(page) {
      await page.locator("#railCompare").click();
    },
  },
  {
    id: "versionPanel",
    name: "Version history",
    async open(page) {
      await page.locator("#railVersions").click();
    },
  },
  {
    id: "symbolDialog",
    name: "Insert symbol",
    async open(page) {
      await page.locator('[data-tab="insert"]').click();
      await page.locator("#insertSymbolBtn").click();
    },
  },
  {
    id: "emojiDialog",
    name: "Insert emoji",
    async open(page) {
      await page.locator('[data-tab="insert"]').click();
      await page.locator("#insertEmojiBtn").click();
    },
  },
  {
    id: "paragraphPropertiesPanel",
    name: "Paragraph properties",
    async open(page) {
      await clickIntoFirstPage(page);
      await page.locator("#paraOptsBtn").click();
    },
  },
  {
    id: "tablePropertiesPanel",
    name: "Table properties",
    // The rich fixture has no table, so one is inserted — the same recipe
    // `dialog-contract` and `table-editing-ux` use.
    async open(page) {
      await clickIntoFirstPage(page);
      await page.locator('[data-tab="insert"]').click();
      await page.locator("#insertTableBtn").click();
      await expect(page.locator("#insertTableMenu")).toBeVisible();
      await page.locator('.gc[data-r="2"][data-c="2"]').click();
      await expect(page.locator("#tabTable")).toBeEnabled();
      await page.locator("#tabTable").click();
      await page.locator("#tablePropertiesBtn").click();
    },
  },
  {
    id: "objectInspector",
    name: "Object properties",
    selector: ".object-inspector",
    // The rich fixture carries a selectable inline image; `object.selectNext`
    // reaches it without a pointer, which is also the keyboard route.
    async open(page) {
      await clickIntoFirstPage(page);
      await runPaletteCommand(page, "object.selectNext", "select next object");
      await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
      // Layout > Wrap text lands on the inspector, which is how
      // `layout-references-surface.spec.mjs` reaches it.
      await page.locator("#tabLayout").click();
      await page.locator("#layoutWrapBtn").click();
    },
  },
];

/** The left edge of the first INK in `element`, taken from a Range over its
 *  first non-empty text node.
 *
 *  Not `getBoundingClientRect().left` on the element: a `.panel-title` is a
 *  `<span>` or a `<strong>` in a flex row, so its box can start where the flex
 *  item starts rather than where the glyphs do, and the question here is where
 *  the reader sees the text begin. */
const PROBE = ({ selector, titleSelector, bodySelector }) => {
  const root = document.querySelector(selector);
  if (!root) return { missing: selector };
  const textLeft = (el) => {
    if (!el) return null;
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, {
      acceptNode: (node) =>
        node.nodeValue.trim() ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT,
    });
    const node = walker.nextNode();
    if (!node) return null;
    const range = document.createRange();
    range.selectNodeContents(node);
    const rects = [...range.getClientRects()];
    return rects.length ? Math.round(rects[0].left * 100) / 100 : null;
  };
  /** The padding edge: where the body's content is allowed to start. */
  const contentLeft = (el) => {
    const style = getComputedStyle(el);
    return (
      Math.round(
        (el.getBoundingClientRect().left +
          parseFloat(style.borderLeftWidth) +
          parseFloat(style.paddingLeft)) *
          100,
      ) / 100
    );
  };
  const head = root.querySelector(titleSelector);
  const body = root.querySelector(bodySelector);
  return {
    headContent: contentLeft(head),
    headTitleText: textLeft(head.querySelector(".panel-title") ?? head),
    bodyContent: contentLeft(body),
    headPaddingLeft: getComputedStyle(head).paddingLeft,
    bodyPaddingLeft: getComputedStyle(body).paddingLeft,
    headPaddingRight: getComputedStyle(head).paddingRight,
    bodyPaddingRight: getComputedStyle(body).paddingRight,
  };
};

for (const panel of PANELS) {
  test(`${panel.name}: the title sits over the body's own content edge`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize(LAPTOP);
    await gotoEditor(page);
    await panel.open(page);
    const selector = panel.selector ?? `#${panel.id}`;
    await expect(page.locator(selector)).toBeVisible();

    const m = await page.evaluate(PROBE, {
      selector,
      titleSelector: ".panel-head",
      bodySelector: ".panel-body",
    });
    expect(m.missing, `${selector} should be in the document`).toBeUndefined();

    // 1. The SHELL takes one inset. This is the whole of D2: a head at 14px
    //    over a body at 6px is a panel whose title does not sit over its
    //    content, and it was true of all nine panels.
    expect(
      m.bodyContent,
      `${panel.name}: the head's content starts at ${m.headContent} and the body's at ` +
        `${m.bodyContent}. head padding-left ${m.headPaddingLeft}, body ` +
        `${m.bodyPaddingLeft} — one shell, one inset.`,
    ).toBe(m.headContent);

    // 2. And the TITLE'S TEXT lands on it, rather than being pushed off it by
    //    something inside the heading. This is D1: a 30px `--accent-soft` tile
    //    in the properties heading's flow put the title 37px inside its own
    //    body's first paragraph, which is four left edges in a 320px column.
    expect(
      m.headTitleText,
      `${panel.name}: the title's text starts at ${m.headTitleText}, the body's content at ` +
        `${m.bodyContent}. Anything in the heading's flow ahead of the title moves the ` +
        `one edge a reader actually sees.`,
    ).toBe(m.bodyContent);

    // 3. Symmetric, as the dialog heads are: the close button's side was 8px
    //    where the title's side was 14px, which is the same disagreement read
    //    from the other end.
    expect(m.headPaddingRight, `${panel.name}: the head's inset is not symmetric`).toBe(
      m.headPaddingLeft,
    );
    expect(m.bodyPaddingRight, `${panel.name}: the body's inset is not symmetric`).toBe(
      m.bodyPaddingLeft,
    );

    expect(consoleErrors).toEqual([]);
  });
}

// Find and replace is the one panel that was ALREADY right, and it is the only
// one that was padded from tokens — which is where `--pnl-pad-x`/`--pnl-pad-y`
// came from. Its GEOMETRY is deliberately not the shell's: it is fixed to the
// top right so it does not cover the text you are searching, which is Word's
// and Google Docs' model and not a defect. Its INSET is the shell's, and this
// is what keeps the two from drifting apart again.
test("Find and replace keeps its own geometry and the shell's inset", async ({ page }) => {
  await page.setViewportSize(LAPTOP);
  await gotoEditor(page);
  await page.keyboard.press(`${MOD}+f`);
  await expect(page.locator("#findPanel")).toBeVisible();
  const m = await page.evaluate(PROBE, {
    selector: "#findPanel",
    titleSelector: ".find-panel-head",
    bodySelector: ".find-panel-body",
  });
  expect(m.bodyContent).toBe(m.headContent);
  expect(m.headPaddingLeft).toBe(m.bodyPaddingLeft);
  // Fixed, and on the right-hand side of the window — the property that makes
  // it not cover the search target. Asserted as a relation to the viewport, not
  // as an x: the panel is `right: 18px` and its width is a `min()`.
  const geometry = await page.evaluate(() => {
    const el = document.getElementById("findPanel");
    const box = el.getBoundingClientRect();
    return {
      position: getComputedStyle(el).position,
      rightGap: Math.round(window.innerWidth - box.right),
      pastHalf: box.left > window.innerWidth / 2,
    };
  });
  expect(geometry.position).toBe("fixed");
  expect(geometry.pastHalf, "Find and replace lives in the right half of the window").toBe(true);
  expect(geometry.rightGap).toBeLessThan(40);
});

// The review column takes the PADDING convention and not the geometry one. It is
// absolutely positioned and rides the document scroll so each card stays pinned
// to the text it annotates — forcing it into the panel shell would break the one
// thing it is built to do. Its header and its cards both said `8px 10px`, which
// was already self-consistent and was two more raw pixel pairs in a family that
// now has a token.
test("the review column's header and its cards share the shell's inset", async ({ page }) => {
  await page.setViewportSize(LAPTOP);
  await gotoEditor(page);
  await page.locator("#railReview").click();
  await expect(page.locator(".review-sidebar")).toBeVisible();
  const m = await page.evaluate(() => {
    const head = document.querySelector(".review-sidebar-header");
    const root = getComputedStyle(document.documentElement);
    const want = `${root.getPropertyValue("--pnl-pad-y").trim()} ${root
      .getPropertyValue("--pnl-pad-x")
      .trim()}`;
    const style = getComputedStyle(head);
    return {
      position: getComputedStyle(document.querySelector(".review-sidebar")).position,
      padding: `${style.paddingTop} ${style.paddingLeft}`,
      want,
    };
  });
  // The geometry it keeps.
  expect(m.position).toBe("absolute");
  // The inset it shares. Resolved values on both sides, so this compares what
  // the browser computed from the token against the token — a header that goes
  // back to a literal fails even if the literal happens to be 8px 16px today,
  // because the token is a `clamp`-able pair and a literal does not follow it.
  expect(m.padding).toBe(m.want);
});

// The roster above cannot fall behind the product. `dialog-fit.spec.mjs` carried
// a hand-grown list of 8 of 24 for months with a comment promising it would
// grow; this is the half that stops the same thing happening here.
//
// `.side-panel` elements are in the DOM from the first frame (they are `hidden`,
// not absent), so nothing has to be opened to enumerate them — except the object
// inspector, which `object_inspector.mjs` creates on demand and which therefore
// gets the source read instead. Both halves, because a panel can be added either
// way and only checking one is how a roster goes stale.
test("every `.side-panel` in the shell is covered by this spec", async ({ page }) => {
  await page.setViewportSize(LAPTOP);
  await gotoEditor(page);
  // The object inspector carries no id (it is addressed by class), so it is
  // named by the roster key it has here; anything else without an id reports as
  // its class list, which is what a reader needs to go and find it.
  const declared = await page.evaluate(() =>
    [...document.querySelectorAll(".side-panel")].map(
      (el) =>
        el.id ||
        (el.classList.contains("object-inspector")
          ? "objectInspector"
          : `.${[...el.classList].join(".")}`),
    ),
  );
  const covered = new Set(PANELS.map((panel) => panel.id));
  expect(
    declared.filter((id) => !covered.has(id)),
    `these \`.side-panel\` surfaces are declared in editor.html and are not in this ` +
      `spec's roster, so nothing measures whether their head and body agree`,
  ).toEqual([]);

  // And the runtime-built one. The class string is what makes it a member of the
  // shell, so that is what is read.
  const inspector = readFileSync(
    new URL("../../src/object_inspector.mjs", import.meta.url),
    "utf8",
  );
  expect(
    /className\s*=\s*"object-inspector side-panel"/.test(inspector),
    "object_inspector.mjs no longer builds a `.side-panel`; if the shell gained or lost " +
      "a runtime-built panel, this spec's roster has to say so",
  ).toBe(true);
  expect(covered.has("objectInspector")).toBe(true);
});
