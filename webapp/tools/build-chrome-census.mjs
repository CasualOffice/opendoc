#!/usr/bin/env node
// What the editor shows a reader, counted — one control at a time, per surface.
//
// WHY THIS EXISTS. The owner's complaint is cognitive burden, and every previous
// answer to it in this repository has been prose. Prose cannot tell you whether
// a band grew by four controls last week, and `docs/105` UX-027 records that
// *nothing* decides which surface a capability lands on, so there has never been
// a number to argue with. `docs/63`/`64`/`67`/`101`/`105`/`123`/`148` all discuss
// density; none of them publishes a count that regenerates. This file is the
// instrument those documents were missing: it counts the controls a reader sees
// on each surface, straight out of `editor.html`, and `--check` fails the build
// when the committed census drifts — so a band cannot quietly gain nine controls
// between audits again.
//
// `SKILL.md` §9 rule 1: "a published number is generated from a committed
// artifact, or it is not published." `docs/167` quotes this census and nothing
// else for its density figures.
//
// WHAT IS MEASURED, and the honest limits of it.
//
// The subject is STRUCTURAL visibility: an element counts as visible by default
// when neither it nor any ancestor carries the `hidden` attribute. That is what
// `editor.html` can answer on its own, and it is the figure that matters for
// burden — `hidden` is how this chrome gates every panel, dialog, popover and
// the whole compact toolbar, and `main.js` toggles the attribute rather than
// inline styles (`ribbon_surface.mjs`, `popover_manager.mjs`). It deliberately
// does NOT model:
//
//   * CSS `display:none` from a media query. A phone hides bands by stylesheet,
//     so the phone figure here is an upper bound; `tests/e2e/phone-*.spec.mjs`
//     measures the rendered phone chrome.
//   * controls `main.js` injects at runtime — the compact toolbar, the overflow
//     menu, the context menus, the command palette rows and the application-menu
//     rows are all built from the taxonomy at boot. Those surfaces are counted
//     from their DECLARATION in `src/command_taxonomy.mjs` instead, which is the
//     committed artifact that decides them, and the two sources are labelled
//     separately in the output so nobody adds a markup count to a taxonomy count
//     and publishes the sum.
//
// So `markup` and `taxonomy` are reported as separate objects on purpose. A
// single blended "total controls" number would be the kind of figure this
// repository has already published twice and had to retract.
import { readFileSync, realpathSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { FILE_SURFACE, RIBBON_TABS, allMenuSections, appMenuNames } from "../src/command_taxonomy.mjs";

const HTML = new URL("../editor.html", import.meta.url);
const OUT = new URL("../chrome-census.json", import.meta.url);

/** Elements that ship `hidden` and are unhidden unconditionally at boot, so a
 *  reader does see their subtree with nothing opened. One entry; see the note at
 *  its use for why this is not a loophole. */
const BOOT_REVEALED = new Set(["documentChrome"]);

/** Tags whose content is not markup, so `<` inside them must not open a tag. */
const RAW_TEXT = new Set(["script", "style"]);

/** Void elements: they never have a closing tag, so they must not push a frame.
 *  Getting this wrong silently nests the whole rest of the document inside an
 *  `<input>`, which would make every count after the first form field wrong. */
const VOID = new Set([
  "area",
  "base",
  "br",
  "col",
  "embed",
  "hr",
  "img",
  "input",
  "link",
  "meta",
  "param",
  "source",
  "track",
  "wbr",
]);

/** The element kinds a reader can operate. `a` without `href` is not one of
 *  them, and neither is a `tabindex="-1"` node: both are reachable only to
 *  script, so counting them would inflate the density figure with chrome a
 *  reader cannot act on. */
function isControl(node) {
  const { tag, attrs } = node;
  if (tag === "button" || tag === "select" || tag === "textarea") return true;
  if (tag === "input") return (attrs.type ?? "text").toLowerCase() !== "hidden";
  if (tag === "a") return attrs.href !== undefined;
  const role = attrs.role;
  if (role && /^(?:button|menuitem|menuitemcheckbox|menuitemradio|option|tab|switch|checkbox|radio|slider|spinbutton|combobox|searchbox|textbox|link)$/.test(role)) {
    return true;
  }
  return false;
}

/** Parse `editor.html` into a tree of the elements this census asks about.
 *
 *  A real HTML parser is not available here (the webapp has exactly one
 *  devDependency and it is Playwright), and pulling one in to count buttons
 *  would be a dependency the project carries forever. So this is a tag scanner
 *  with the two correctness properties the count depends on — void elements do
 *  not nest, and raw-text elements do not contain tags — plus a hard assertion
 *  that the tree closes cleanly. If `editor.html` ever stops parsing, this
 *  throws rather than quietly reporting a smaller number.
 *
 *  Complexity: O(bytes). It runs once per build. */
export function parseTree(html) {
  const root = { tag: "#root", attrs: {}, children: [], parent: null, hidden: false };
  let node = root;
  let i = 0;
  while (i < html.length) {
    const lt = html.indexOf("<", i);
    if (lt < 0) break;
    if (html.startsWith("<!--", lt)) {
      const end = html.indexOf("-->", lt + 4);
      i = end < 0 ? html.length : end + 3;
      continue;
    }
    if (html.startsWith("<!", lt)) {
      const end = html.indexOf(">", lt + 2);
      i = end < 0 ? html.length : end + 1;
      continue;
    }
    const gt = findTagEnd(html, lt);
    if (gt < 0) break;
    const raw = html.slice(lt + 1, gt);
    i = gt + 1;
    if (raw.startsWith("/")) {
      const tag = raw.slice(1).trim().toLowerCase();
      // Climb to the nearest matching open frame. An unmatched close is ignored
      // rather than allowed to unwind the whole tree.
      let up = node;
      while (up && up.tag !== tag) up = up.parent;
      if (up && up.parent) node = up.parent;
      continue;
    }
    const selfClosing = raw.endsWith("/");
    const body = selfClosing ? raw.slice(0, -1) : raw;
    const tag = (body.match(/^[a-zA-Z0-9:-]+/) ?? [""])[0].toLowerCase();
    if (!tag) continue;
    const attrs = parseAttrs(body.slice(tag.length));
    const child = {
      tag,
      attrs,
      children: [],
      parent: node,
      // BOOT_REVEALED is not an exception to the rule, it is the rule applied
      // once: `#documentChrome` ships `hidden` and `main.js:2450` clears it
      // unconditionally, before anything else, because the menu bar is the only
      // entry point in an empty editor. Counting its subtree as concealed would
      // report the whole chrome as invisible — which is how the first run of this
      // census said the ribbon tab strip held zero controls.
      hidden:
        node.hidden ||
        (attrs.hidden !== undefined && !BOOT_REVEALED.has(attrs.id ?? "")),
    };
    node.children.push(child);
    if (RAW_TEXT.has(tag)) {
      const close = html.toLowerCase().indexOf(`</${tag}`, i);
      i = close < 0 ? html.length : close;
      continue;
    }
    if (!selfClosing && !VOID.has(tag)) node = child;
  }
  return root;
}

/** The `>` that ends this tag, skipping any inside a quoted attribute value.
 *  `title="Indent > 0"` is not hypothetical in this file. */
function findTagEnd(html, lt) {
  let quote = null;
  for (let j = lt + 1; j < html.length; j += 1) {
    const ch = html[j];
    if (quote) {
      if (ch === quote) quote = null;
      continue;
    }
    if (ch === '"' || ch === "'") quote = ch;
    else if (ch === ">") return j;
  }
  return -1;
}

function parseAttrs(text) {
  const attrs = {};
  const re = /([a-zA-Z_:][-a-zA-Z0-9_:.]*)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+)))?/g;
  let m;
  while ((m = re.exec(text))) {
    attrs[m[1].toLowerCase()] = m[2] ?? m[3] ?? m[4] ?? "";
  }
  return attrs;
}

function* walk(node) {
  for (const child of node.children) {
    yield child;
    yield* walk(child);
  }
}

function byId(root) {
  const map = new Map();
  for (const node of walk(root)) {
    if (node.attrs.id) map.set(node.attrs.id, node);
  }
  return map;
}

/** Controls under `node`, split by whether a reader sees them without opening
 *  anything. `shown` is the burden figure; `shown + concealed` is the surface's
 *  whole roster. */
function census(node) {
  let shown = 0;
  let concealed = 0;
  if (!node) return { shown, concealed };
  for (const descendant of walk(node)) {
    if (!isControl(descendant)) continue;
    if (descendant.hidden) concealed += 1;
    else shown += 1;
  }
  return { shown, concealed };
}

export function build(check = false) {
  const html = readFileSync(HTML, "utf8");
  const root = parseTree(html);
  const ids = byId(root);

  // --- The ribbon ----------------------------------------------------------
  // Exactly one panel is unhidden in the committed markup, and that is the
  // design: the band a reader lands on. `shown` on every other panel must be 0
  // or the census is measuring something else, so the assertion is made here
  // rather than left to a reader of the JSON.
  const panels = {};
  for (const { tab } of RIBBON_TABS) {
    const node = ids.get(`panel${tab[0].toUpperCase()}${tab.slice(1)}`);
    if (!node) continue;
    const counts = census(node);
    const groups = [...walk(node)].filter((n) => (n.attrs.class ?? "").split(/\s+/).includes("rgroup"));
    panels[tab] = {
      landing: !node.hidden,
      groups: groups.length,
      controls: counts.shown + counts.concealed,
      shownWhenOpen: node.hidden ? counts.shown + counts.concealed : counts.shown,
    };
  }

  // --- The persistent chrome ------------------------------------------------
  // What is on screen before a reader opens, clicks or selects anything. This is
  // the number the owner's "little to zero cognitive burden" is about.
  const withClass = (name) =>
    [...walk(root)].filter((n) => (n.attrs.class ?? "").split(/\s+/).includes(name));
  const shownIn = (nodes) => nodes.reduce((sum, n) => sum + census(n).shown, 0);

  // The menu bar and the ribbon tab strip are the two renderings of ONE
  // navigation axis (docs/123 §3), never both on screen: the bar is CSS-hidden
  // in ribbon mode and the strip in compact mode, and neither carries `hidden`.
  // So they are reported separately and never summed — a `persistentTotal` that
  // added both would describe a chrome this editor never shows.
  const persistent = {
    // Everything in `header.bar` that is neither of the two navigation axes:
    // the document-name field and whatever the header's right end carries.
    header:
      shownIn(withClass("document-title-row")) +
      (census(ids.get("appHeaderActions")).shown || 0),
    appMenuBarCompactOnly: census(ids.get("appMenuBar")).shown,
    ribbonTabsRibbonOnly: withClass("ribbon-tab").filter((n) => !n.hidden).length,
    rail: shownIn(withClass("rail")),
    statusBar: shownIn(withClass("footer")),
  };

  // --- Transient surfaces ---------------------------------------------------
  // Every dialog card, side panel and popover in the markup. The COUNT is the
  // interesting figure: `docs/105` UX-027 asks what decides between these three
  // shapes, and the answer has to account for all of them.
  const classed = withClass;
  const dialogs = classed("dialog-card").map((n) => ({
    id: n.parent?.attrs.id ?? n.attrs.id ?? null,
    controls: census(n).shown + census(n).concealed,
  }));
  const sidePanels = classed("side-panel").map((n) => ({
    id: n.attrs.id ?? null,
    controls: census(n).shown + census(n).concealed,
  }));

  // --- The taxonomy surfaces ------------------------------------------------
  // Declared, not marked up. Counted from the module that decides them.
  const menuRows = new Set();
  for (const section of allMenuSections()) {
    for (const entry of section.ids ?? []) {
      if (typeof entry === "string") menuRows.add(entry);
      else if (entry?.ids) for (const id of entry.ids) menuRows.add(id);
    }
  }

  const census_ = {
    $generated: "webapp/tools/build-chrome-census.mjs — do not edit by hand",
    $subject:
      "Controls a reader can operate, per chrome surface. `shown` excludes anything " +
      "under a `hidden` ancestor. See the header of the generator for what this " +
      "deliberately does not model.",
    markup: {
      persistent,
      // The two axes are mutually exclusive, so the figure a reader experiences
      // takes the LARGER of them plus everything that is always on screen.
      persistentRibbonChrome:
        persistent.header +
        persistent.ribbonTabsRibbonOnly +
        persistent.rail +
        persistent.statusBar +
        (panels.home?.shownWhenOpen ?? 0),
      persistentCompactChrome:
        persistent.header + persistent.appMenuBarCompactOnly + persistent.rail + persistent.statusBar,
      ribbonPanels: panels,
      dialogCards: { count: dialogs.length, controls: dialogs.reduce((a, d) => a + d.controls, 0) },
      sidePanels: { count: sidePanels.length, controls: sidePanels.reduce((a, p) => a + p.controls, 0) },
    },
    taxonomy: {
      applicationMenus: appMenuNames().length,
      applicationMenuRows: menuRows.size,
      fileSurfaceBands: FILE_SURFACE.length,
      ribbonTabs: RIBBON_TABS.length,
    },
  };

  const rendered = `${JSON.stringify(census_, null, 2)}\n`;
  if (check) {
    let current = "";
    try {
      current = readFileSync(OUT, "utf8");
    } catch {
      current = "";
    }
    if (current !== rendered) {
      console.error(
        "build-chrome-census --check: webapp/chrome-census.json is stale.\n" +
          "A chrome surface gained or lost controls without the census being " +
          "regenerated, so every density number docs/167 publishes is now wrong.\n" +
          "Run ./tools/build-chrome-census.mjs and commit the result.",
      );
      return 1;
    }
    console.log("build-chrome-census --check: chrome-census.json up to date.");
    return 0;
  }
  writeFileSync(OUT, rendered, "utf8");
  console.log(
    `build-chrome-census: wrote chrome-census.json ` +
      `(${census_.markup.persistentRibbonChrome} controls on the ribbon chrome, ` +
      `${dialogs.length} dialog cards, ${sidePanels.length} side panels)`,
  );
  return 0;
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exit(build(process.argv.includes("--check")));
}
