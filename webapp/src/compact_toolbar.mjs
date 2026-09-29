// The compact (Docs-shaped) toolbar: its layout table, its grouping, and the
// reflow that keeps it from ever scrolling sideways.
//
// Extracted from `main.js` (HF-085 ratchet) so the layout is data a test can
// read, and so the grouping and overflow logic are not another 200 lines in the
// god file. The module owns DOM, but it owns only THIS bar: everything it needs
// from the editor — the command registry, the click binder, the enablement
// lists, the popover manager — arrives in `createCompactToolbar`'s dependency
// bag, so nothing here imports `main.js` back.
//
// ---- What this bar is, and where its shape comes from ----------------------
//
// Contents and order follow **Google Docs**, which is the chrome this imitates.
// Docs reads, left to right: undo · redo · print · spellcheck · paint format ‖
// zoom ‖ style ‖ font ‖ size −/+ ‖ B I U · text colour · highlight ‖ link ·
// comment · image ‖ align · line spacing · checklist · lists · indent ∓ · clear
// formatting. Where Docs has something this engine does not, the row is absent
// rather than present-and-dead (docs/63 forbids dead controls).
//
// The bar is DECLARED, not hand-bound. Each row names a command id that already
// exists in `editorCommands()`, so enablement, activation and pressed state all
// come from the one registry entry the ribbon and the palette use. The
// alternative — a second set of listeners — is exactly the drift docs/105
// UX-004/UX-005 records.
//
// `icon` is a Material Symbols ligature from the SELF-HOSTED face (docs: never
// add a CDN link, `chrome_fonts.test.mjs` forbids it).
//
// ---- Grouping (docs/115 §8) ------------------------------------------------
//
// Rows are grouped, and the GROUP is the unit of three separate things:
//
//   1. **Reading.** Related controls sit flush against each other with a gap —
//      and at the marked boundaries a rule — between groups, the way Docs
//      draws its toolbar. The bar used to be 26 equal-weight buttons in one
//      undifferentiated run.
//   2. **Announcement.** Each group is a `role="group"` with a name, so a
//      screen reader says "Lists, group" instead of three anonymous buttons.
//      This is the ribbon's `.rgroup` treatment, applied here.
//   3. **Folding.** When the bar cannot fit, whole groups move into the "⋯"
//      overflow menu, right to left, pinned groups last. That is
//      `updateRibbonOverflow`'s algorithm, and it is what Google Docs does at
//      narrow widths. The bar therefore NEVER scrolls sideways.
//
// ---- Alignment is one control, not four (Google Docs) ----------------------
//
// Docs puts a single align dropdown in its toolbar — "the align dropdown in the
// main toolbar will left align, center, right align, and justify selected
// text". This bar did the opposite: four equal buttons, 136px, none of which
// ever lit up, because `updateToolbar` reflects alignment onto the ribbon's
// four elements by id and the compact copies were not among them. One trigger
// whose icon reports the current alignment, opening a menu of the same four
// registry commands, is both the Docs shape and the smaller one.

// ---- Tables (TBL-18) -------------------------------------------------------
//
// The compact bar shipped with NO table command at all, so switching the chrome
// to the Docs-shaped bar took every structural table capability off the screen:
// the contextual Table BAND is a ribbon tab, and in compact mode there is no
// ribbon. Google Docs solves this with a Format ▸ Table submenu, which is a
// menu and not twenty buttons, and that is what this borrows — one trigger whose
// menu is `APP_MENU_SECTIONS.table`, the SAME declaration the Table menu and the
// palette render, so the three cannot disagree about which commands exist, what
// they are called, or when they refuse.
//
// Its two names come from the CATALOGUE (`appMenuBar.table`, already translated
// into all eighteen languages for the menu bar) rather than from a fresh English
// literal: the bar's older group labels are raw English under a ratchet, and the
// way to not make that worse is to not add to it.
//
// **It is CONTEXTUAL, and that is not a width dodge — it is the standard.** The
// ribbon's own Table tab appears only when the caret is in a table; Word's Table
// Layout tab is contextual too, and Docs shows its table toolbar only over a
// table. The width matters as well, and honestly: measured at the 1280px budget
// this bar is held to, the twelve resting groups need 1237px of an available
// 1256, so a permanent thirteenth group would have folded another one into the
// `⋯` menu at the default viewport. Outside a table the capability is not gone —
// the compact chrome's Table MENU carries every row, disabled with its reason,
// which is where "never a dead control" is met.
import { APP_MENU_SECTIONS, TABLE_MENU_LABELS, sectionCommandIds } from "./command_taxonomy.mjs";
import { t } from "./i18n.mjs";

/** The ribbon-owned controls this bar borrows, by row `control` key.
 *
 *  Adopted, never cloned: one element means one set of listeners, one
 *  reflected value and one disabled state, so the two chromes cannot drift
 *  apart. A clone would be a second answer to "what font is this?". */
export const ADOPTED_CONTROL_IDS = {
  zoom: "zoom",
  style: "stylesTrigger",
  font: "fontFamily",
  size: "fontSize",
};

/** The bar, as data.
 *
 *  `pinned` groups are the ones that stay inline longest when the bar folds:
 *  undo/redo, the style, font and size controls, and B/I/U are the controls a
 *  word processor is unusable without.
 *
 *  `divider` draws a rule before the group. The marked boundaries are the seven
 *  Docs already draws; the other group breaks read as a gap, which is also what
 *  Docs does. */
export const COMPACT_TOOLBAR = [
  {
    group: "history",
    label: "History",
    pinned: true,
    items: [
      { id: "edit.undo", icon: "undo" },
      { id: "edit.redo", icon: "redo" },
      { id: "file.print", icon: "print" },
      { id: "format.painter", icon: "format_paint", toggle: true },
    ],
  },
  { group: "zoom", label: "Zoom", divider: true, items: [{ kind: "adopt", control: "zoom" }] },
  {
    group: "style",
    label: "Paragraph style",
    divider: true,
    pinned: true,
    items: [{ kind: "adopt", control: "style" }],
  },
  {
    group: "font",
    label: "Font",
    divider: true,
    pinned: true,
    items: [{ kind: "adopt", control: "font" }],
  },
  {
    group: "size",
    label: "Font size",
    divider: true,
    pinned: true,
    items: [
      { id: "format.shrink", icon: "remove" },
      { kind: "adopt", control: "size" },
      { id: "format.grow", icon: "add" },
    ],
  },
  {
    group: "text",
    label: "Text formatting",
    divider: true,
    pinned: true,
    items: [
      { id: "format.bold", icon: "format_bold", toggle: true, fmt: "bold", needs: "run" },
      { id: "format.italic", icon: "format_italic", toggle: true, fmt: "italic", needs: "run" },
      { id: "format.underline", icon: "format_underlined", toggle: true, fmt: "underline", needs: "run" },
      {
        id: "format.color",
        icon: "format_color_text",
        needs: "run",
        popup: "dialog",
        controls: "textColorMenu",
      },
      {
        id: "format.highlight",
        icon: "ink_highlighter",
        needs: "run",
        popup: "dialog",
        controls: "highlightMenu",
      },
    ],
  },
  {
    group: "insert",
    label: "Insert",
    divider: true,
    items: [
      { id: "insert.link", icon: "link" },
      // `review.comment`, not the context menu's `comment.add`: the same
      // mistake that cost this bar its bulleted and numbered list buttons also
      // cost it Add comment, and nothing said so.
      { id: "review.comment", icon: "add_comment" },
      { id: "insert.image", icon: "image" },
    ],
  },
  {
    // Beside Insert, because inserting a table is the gesture that creates the
    // need for these, and unpinned so it folds before B/I/U do.
    group: "table",
    labelKey: "appMenuBar.table",
    contextual: true,
    items: [
      {
        kind: "menu",
        id: "compactTable",
        icon: "grid_on",
        labelKey: "appMenuBar.table",
        sections: APP_MENU_SECTIONS.table,
        labels: TABLE_MENU_LABELS,
      },
    ],
  },
  {
    group: "align",
    label: "Alignment",
    divider: true,
    items: [
      {
        kind: "align",
        // Every one of these stays individually reachable: from this menu, from
        // the ribbon's four buttons, from the Format menu, and from the command
        // palette by its own name (UX-004 — two surfaces minimum).
        ids: [
          "paragraph.align.start",
          "paragraph.align.center",
          "paragraph.align.end",
          "paragraph.align.justify",
        ],
        icons: {
          "paragraph.align.start": "format_align_left",
          "paragraph.align.center": "format_align_center",
          "paragraph.align.end": "format_align_right",
          "paragraph.align.justify": "format_align_justify",
        },
      },
    ],
  },
  {
    group: "spacing",
    label: "Paragraph spacing",
    items: [{ id: "layout.paragraph", icon: "format_line_spacing" }],
  },
  {
    group: "lists",
    label: "Lists",
    items: [
      { id: "paragraph.list.checklist", icon: "checklist", needs: "para" },
      { id: "paragraph.list.bullet", icon: "format_list_bulleted", needs: "para" },
      { id: "paragraph.list.numbered", icon: "format_list_numbered", needs: "para" },
    ],
  },
  {
    group: "indent",
    label: "Indent",
    items: [
      { id: "paragraph.indent.decrease", icon: "format_indent_decrease", needs: "para" },
      { id: "paragraph.indent.increase", icon: "format_indent_increase", needs: "para" },
    ],
  },
  {
    // No rule before this one: in Docs the run from align to clear-formatting
    // is one undivided stretch, and the rules sit further left.
    group: "clear",
    label: "Clear formatting",
    items: [{ id: "format.clear", icon: "format_clear" }],
  },
];

// ---- The phone roster: Aa and + (docs/148 §9 item 4) ------------------------
//
// `docs/148` §8.4 argued AGAINST splitting formatting and insertion into two
// sheets, on the grounds that this bar's own `⋯` fold already IS Google's "Aa"
// sheet — and §10 recorded that discovery honestly: at 390px the fold holds
// zoom, font, size, B/I/U, colour, highlight, link, comment, image, alignment,
// lists, indent and clear-formatting, "which is Google's Aa sheet, arrived at
// without writing one."
//
// **That argument is wrong and this roster is why.** What the `⋯` fold holds is
// whatever did not fit, so its contents are a function of the WIDTH: at 390px
// the style picker is inline and at 320px it folds. A sheet whose membership
// changes when the window changes is not a designed surface — you cannot tell
// someone where a command lives, and the two phones in `PHONES` disagree about
// it. It also arrives in fold order (right to left, pinned last), which is a
// layout accident rather than a reading order. The competitive standard is a
// sheet with a STABLE, NAMED roster:
//
//   * Google Docs' **Aa** opens a panel with a **Text** section (Style, Font,
//     Size, Text colour, Highlight) and a separate **Paragraph** tab
//     (alignment, line spacing, indentation) — support answer 1663349. Its
//     **+** is the insert sheet.
//   * ONLYOFFICE's **Edit** is a bottom Sheet on a phone and a Popover on
//     anything wider (`apps/documenteditor/mobile/src/view/edit/Edit.jsx:278-302`),
//     with a tab bar keyed to the SELECTED OBJECT TYPE — text, paragraph,
//     image, shape, table, chart, header, TOC (`EditingPage.jsx:68-190`). Their
//     **Add** is a full-screen Popup with the same tab-then-push shape
//     (`src/view/add/Add.jsx:101-127`).
//
// So two references, one shape. What this roster does NOT copy is ONLYOFFICE's
// second front end: the two sheets are `kind: "menu"` entries over
// `APP_MENU_SECTIONS.format` and `APP_MENU_SECTIONS.insert` — the SAME
// declarations the Format and Insert menus and the palette already render — so
// there is no second roster to keep in step and no second answer to "where does
// this command live". Google's Text/Paragraph tab split arrives as the existing
// named BANDS (`menuGroup.font`, `menuGroup.alignment`, `menuGroup.paragraph`,
// …), which are already `role="group"` with translated names, so a screen
// reader gets more structure than a tab bar would give it, not less.
//
// The inline run is Docs': undo · redo · B · I · U · + · Aa · comment. Nothing
// else is inline, because at 320px nothing else fits and a bar that folds
// differently on two phones is the defect above.
//
// ONLYOFFICE's object-type tab keying is deliberately NOT here. The mechanism
// exists — `contextual` already shows the Table group only inside a table, and
// the object chip (`object_bar.mjs`) is this shell's answer for a selected
// drawing — so an object-keyed sheet is a data change when it is wanted. It is
// named rather than half-built.
export const PHONE_TOOLBAR = [
  {
    // Every group name in THIS roster is a catalogue key, never an English
    // literal. The desktop roster above carries raw English under a ceiling
    // (`no_unrouted_strings.test.mjs`), and the way not to make that worse is
    // not to add to it — these keys are already translated into all nineteen
    // locales, because the menu bar and the taxonomy's bands use them.
    group: "history",
    labelKey: "menuGroup.undo",
    pinned: true,
    items: [
      { id: "edit.undo", icon: "undo" },
      { id: "edit.redo", icon: "redo" },
    ],
  },
  {
    // THE TWO SHEETS COME FIRST, AND THAT ORDER WAS FOUND BY LOOKING.
    //
    // The first version of this roster read undo · redo · B · I · U · + · Aa ·
    // comment, with all four leading groups pinned. Screenshotted at 390px, the
    // bar rendered undo · redo · B · I · U · + · ⋯ — **the Aa sheet had folded
    // and the + sheet had not**, because `reflow()` spends groups right to left
    // and Aa was the rightmost pinned one. The single most important control on
    // a phone was the one the fold paid with, and no assertion would have
    // caught it: the bar had not scrolled, nothing was off-window, and every
    // command was still reachable — through a `⋯` nobody would think to open
    // for formatting.
    //
    // So the sheets sit immediately after undo/redo and are the only groups
    // besides history that are pinned. Everything to their right is what the
    // fold is allowed to spend, in the order it should be spent: B/I/U (whose
    // three commands are inside the Aa sheet anyway, in `menuGroup.font`), then
    // zoom. Docs' left-to-right order is not a constraint here — `docs/148` §2
    // records that no Google page enumerates it, so this document does not
    // claim one either, and the fold order is a real constraint that does.
    group: "formatSheet",
    divider: true,
    pinned: true,
    labelKey: "appMenuBar.format",
    items: [
      {
        kind: "menu",
        id: "compactFormat",
        // Material Symbols' own name for the "Aa" glyph, verified present in
        // the SELF-HOSTED face (`assets/fonts/material-symbols-outlined.woff2`)
        // rather than assumed — a missing ligature renders as its own name in
        // text, which is worse than a wrong icon.
        icon: "text_format",
        labelKey: "appMenuBar.format",
        // Style, Font, Size at the top, which is where Docs' Aa panel opens.
        // See `adoptInto`: they are value pickers, not commands, and the ribbon
        // that owns them is hidden in compact chrome, so a phone without this
        // strip would have no font, no size and no paragraph style at all.
        adopt: ["style", "font", "size"],
        adoptLabelKey: "menuGroup.font",
        sections: APP_MENU_SECTIONS.format,
      },
    ],
  },
  {
    group: "insertSheet",
    divider: true,
    pinned: true,
    labelKey: "appMenuBar.insert",
    items: [
      {
        kind: "menu",
        id: "compactInsert",
        icon: "add",
        labelKey: "appMenuBar.insert",
        // `APP_MENU_SECTIONS.insert` already carries `review.comment` in its
        // `menuGroup.comments` band, which is why this roster has no separate
        // comment button: an inline one would be a second face of a command the
        // sheet beside it already offers, bought with 38px the fold needs.
        sections: APP_MENU_SECTIONS.insert,
      },
    ],
  },
  {
    // Not pinned. Every one of these three is inside the Aa sheet's
    // `menuGroup.font` band, so folding them costs a tap rather than a
    // capability — which is exactly what a fold should be allowed to cost.
    group: "text",
    labelKey: "menuGroup.font",
    divider: true,
    items: [
      { id: "format.bold", icon: "format_bold", toggle: true, fmt: "bold", needs: "run" },
      { id: "format.italic", icon: "format_italic", toggle: true, fmt: "italic", needs: "run" },
      { id: "format.underline", icon: "format_underlined", toggle: true, fmt: "underline", needs: "run" },
    ],
  },
  {
    // NO ZOOM GROUP, and the reason is worth keeping because it was got wrong
    // once. `#zoom` is a text field that lives in the STATUS BAR and that the
    // desktop roster adopts out of there. A first version of this roster left
    // it behind, and `phone-no-horizontal-scroll.spec.mjs` immediately reported
    // `footer.footer` at scrollWidth 344 against clientWidth 320 — the status
    // bar had been overflowing at 320px all along, and the desktop bar was
    // masking it by taking the control away. Adopting it here fixed that and
    // cost the bar its inline B/I/U at 390px, which is the worse trade: three
    // one-tap toggles for a percentage nobody types on a phone. So the readout
    // is hidden at this rung in `style.css` instead, which fixes the status
    // bar's own defect rather than relocating it, and zoom stays reachable from
    // the steppers beside it, from View ▸ Zoom and from pinch.
    //
    // Same contextual Table group the desktop bar carries, for the same reason:
    // in compact chrome there is no ribbon, so the Table band has no other home.
    group: "table",
    labelKey: "appMenuBar.table",
    contextual: true,
    divider: true,
    items: [
      {
        kind: "menu",
        id: "compactTable",
        icon: "grid_on",
        labelKey: "appMenuBar.table",
        sections: APP_MENU_SECTIONS.table,
        labels: TABLE_MENU_LABELS,
      },
    ],
  },
];

/** Which alignment key each align command applies, for the trigger's icon. */
const ALIGN_KEY = {
  "paragraph.align.start": "start",
  "paragraph.align.center": "center",
  "paragraph.align.end": "end",
  "paragraph.align.justify": "justify",
};

/** Every command id the bar declares, in order. The parity guard reads this:
 *  a declared id that the registry does not answer renders NOTHING, which is
 *  how `paragraph.bullets` and `paragraph.numbering` — context-menu ids that
 *  were never registry ids — sat in the table while the bar silently shipped
 *  without a bulleted or a numbered list button. */
export function compactCommandIds(table = COMPACT_TOOLBAR) {
  const ids = [];
  for (const group of table) {
    for (const item of group.items) {
      if (item.kind === "menu") {
        ids.push(...sectionCommandIds(item.sections));
        continue; // its own `id` names the TRIGGER, not a command
      }
      if (item.id) ids.push(item.id);
      if (item.kind === "align") ids.push(...item.ids);
    }
  }
  return ids;
}

/**
 * Builds the compact bar and keeps it fitting.
 *
 * Complexity: `render` is O(controls in the bar) — a fixed ~26 — and `reflow`
 * is O(groups). Neither touches the document, so both are O(1) in document
 * size, as every per-interaction path must be (docs/107 §4).
 */
export function createCompactToolbar({
  host,
  editorCommands,
  onButton,
  registerPopover,
  runControls,
  paraControls,
  formatToggleCache,
  // The registry stores chords in Apple glyphs and every other surface renders
  // them for the keyboard in front of the user (`109` HF-025 / UX-009). This bar
  // did not, so on a PC every one of its tooltips read "Undo (⌘Z)" — a key that
  // keyboard does not have. Injected rather than imported so this module stays
  // free of `main.js` (`module_seams.test.mjs`); the default keeps a caller that
  // does not pass it working, unlocalised, rather than throwing.
  localizeShortcut = (text) => text,
  table = COMPACT_TOOLBAR,
  // The phone's roster (see `PHONE_TOOLBAR`). Passed as a SECOND table rather
  // than as a different `createCompactToolbar` call, because one bar with two
  // rosters keeps one set of adopted controls, one overflow menu and one
  // enablement contribution — two instances would fight over `#fontFamily`.
  phoneTable = PHONE_TOOLBAR,
  // Resolved at RENDER time, never captured: crossing the rung re-renders, and
  // a bar that had captured the answer at construction would be a phone bar on
  // a desktop for the rest of the session. Defaults to "never a phone" so every
  // existing caller and every unit test is unchanged.
  isPhone = () => false,
}) {
  /** The roster in force. A function, not a value, for the reason above. */
  const roster = () => (isPhone() ? phoneTable : table);
  /** Where each adopted control came from, so leaving compact mode restores the
   *  ribbon exactly rather than leaving a hole in it. */
  const adoptedHome = new Map();
  /** What this bar pushed into the shared enablement lists last render. They
   *  are module-level arrays in main.js; pushing every render without removing
   *  the previous nodes leaks a growing list of DETACHED buttons that
   *  `updateToolbar` then writes `disabled` into forever. */
  const contributed = [];
  /** The rendered groups, in declaration order, for the fold. */
  let rendered = [];
  /** Whether the caret is in a table, which is what shows the contextual group.
   *  Kept here rather than asked per frame: the bar renders on mode entry and
   *  this is pushed in by the one toolbar sync that already knows. */
  let tableContext = false;
  let overflowBtn = null;
  let overflowMenu = null;
  let alignTrigger = null;
  let alignMenu = null;
  let alignItems = [];
  let currentAlign = "start";

  const registry = () =>
    new Map(editorCommands({ surface: "compact" }).map((command) => [command.id, command]));

  /** Runs a registry command by id, resolved LIVE. `enabled` is a BOOLEAN on
   *  this registry, not a predicate — every other surface gates on
   *  `enabled === false`. Calling it threw on every click, once. */
  function runCommand(id, anchor) {
    const live = editorCommands({ surface: "compact" }).find((c) => c.id === id);
    if (live && live.enabled !== false) live.run(anchor);
  }

  function iconSpan(name) {
    const icon = document.createElement("span");
    icon.className = "ms";
    icon.setAttribute("aria-hidden", "true");
    icon.textContent = name;
    return icon;
  }

  function releaseContributed() {
    for (const [list, el] of contributed) {
      const at = list.indexOf(el);
      if (at !== -1) list.splice(at, 1);
    }
    contributed.length = 0;
  }

  /** Returns every adopted control to its original parent and position. */
  function releaseAdoptedControls() {
    for (const [el, home] of adoptedHome) {
      el.classList.remove("cadopted");
      el.style.minWidth = "";
      if (home.parent) home.parent.insertBefore(el, home.next);
    }
    adoptedHome.clear();
  }

  function release() {
    releaseAdoptedControls();
    releaseContributed();
  }

  function button(command, entry) {
    const el = document.createElement("button");
    el.type = "button";
    el.className = "ctool";
    el.dataset.commandId = command.id;
    el.title = command.shortcut
      ? localizeShortcut(`${command.label} (${command.shortcut})`)
      : command.label;
    el.setAttribute("aria-label", command.label);
    if (entry.toggle) el.setAttribute("aria-pressed", "false");
    if (entry.popup) {
      el.setAttribute("aria-haspopup", entry.popup);
      el.setAttribute("aria-expanded", "false");
    }
    if (entry.controls) el.setAttribute("aria-controls", entry.controls);
    // `updateToolbar` reflects pressed state across EVERY surface by querying
    // `[data-fmt]`, and enablement via the `runControls`/`paraControls` lists.
    // Stamping the same attribute is what makes one sync serve both chromes.
    if (entry.fmt) el.dataset.fmt = entry.fmt;
    el.appendChild(iconSpan(entry.icon));
    if (entry.needs === "run") {
      runControls.push(el);
      contributed.push([runControls, el]);
    }
    if (entry.needs === "para") {
      paraControls.push(el);
      contributed.push([paraControls, el]);
    }
    onButton(el, () => runCommand(command.id, el));
    return el;
  }

  // ---- A declared dropdown of registry rows (the Table menu) ----------------
  // Trigger and surface are created ONCE and the ROWS are rebuilt every time the
  // menu opens, which is the only shape that can be honest here: whether Merge
  // cells is available depends on the selection at the moment of asking, and the
  // bar renders on mode entry. Rebuilding on open is also what keeps the bar off
  // the keystroke path — `editorCommands` walks the caret's table, so asking it
  // per keystroke is exactly the O(document) typing `main-thread-budget` guards.
  const menus = new Map();

  function ensureMenu(entry) {
    if (menus.has(entry.id)) return menus.get(entry.id);
    const name = t(entry.labelKey);
    const trigger = document.createElement("button");
    trigger.type = "button";
    trigger.id = `${entry.id}Btn`;
    trigger.className = "ctool ctool-menu";
    trigger.setAttribute("aria-haspopup", "menu");
    trigger.setAttribute("aria-expanded", "false");
    trigger.setAttribute("aria-controls", `${entry.id}Menu`);
    trigger.setAttribute("aria-label", name);
    trigger.title = name;
    trigger.appendChild(iconSpan(entry.icon));
    const caret = iconSpan("arrow_drop_down");
    caret.classList.add("ctool-caret");
    trigger.appendChild(caret);

    const surface = document.createElement("div");
    surface.id = `${entry.id}Menu`;
    surface.className = "context-menu compact-command-menu";
    surface.hidden = true;
    surface.setAttribute("role", "menu");
    surface.setAttribute("aria-label", name);
    // Hung off <body> for the reason the align menu is: the bar clips.
    document.body.appendChild(surface);
    registerPopover(trigger, surface, () => fillMenu(entry, surface));
    // Filled once here as well as on every open. Two reasons, and neither is
    // cosmetic: the rows are then in the DOM for anything that asks what this
    // bar offers — including the guard that fails the build when a declared
    // command id renders nothing, which is how the bar once shipped with no
    // bulleted and no numbered list button — and a surface that is empty until
    // it is opened cannot be told apart from one that is empty because the
    // command set vanished.
    fillMenu(entry, surface);
    const pair = { trigger, surface };
    menus.set(entry.id, pair);
    return pair;
  }

  /** Rebuilds one dropdown's rows from the LIVE registry.
   *
   *  A command the registry does not answer at all is skipped; one it answers
   *  DISABLED is rendered disabled carrying its reason, never dropped — the
   *  whole point of the surface is that a user browsing it learns the editor can
   *  do this and what is missing, which an absent row cannot say. */
  /** Moves the ribbon-owned value controls into a sheet's leading strip.
   *
   *  Google Docs' Aa panel opens with **Style, Font, Size** and only then the
   *  toggles (support answer 1663349), and those three are not commands — they
   *  are value pickers whose whole surface is the control itself, which is why
   *  `one-axis-navigation.spec.mjs` exempts `format.family.*`, `format.size.*`
   *  and `style.*` as VALUE_FAMILIES keyed to `#fontFamily`, `#fontSize` and
   *  `#stylesTrigger` rather than as rows.
   *
   *  So a phone that dropped them from the bar without putting them anywhere
   *  would have no font, no size and no paragraph style at all: the ribbon that
   *  owns those elements is hidden in compact chrome. They are ADOPTED here —
   *  moved, never cloned — for the reason `ADOPTED_CONTROL_IDS` already gives:
   *  one element means one set of listeners, one reflected value and one
   *  disabled state, so the sheet and the ribbon cannot disagree about what
   *  font this is. `adoptedHome` is recorded on the first move and
   *  `releaseAdoptedControls` puts every one of them back, which is what makes
   *  widening the window past the rung restore the ribbon intact. */
  function adoptInto(strip, controls) {
    for (const control of controls) {
      const el = document.getElementById(ADOPTED_CONTROL_IDS[control]);
      if (!el) continue;
      if (!adoptedHome.has(el)) {
        adoptedHome.set(el, { parent: el.parentNode, next: el.nextSibling });
      }
      el.classList.add("cadopted");
      strip.appendChild(el);
    }
  }

  function fillMenu(entry, surface) {
    const commands = registry();
    surface.replaceChildren();
    if (entry.adopt?.length) {
      const strip = document.createElement("div");
      strip.className = "menu-group menu-value-strip";
      strip.setAttribute("role", "group");
      strip.setAttribute("aria-label", t(entry.adoptLabelKey));
      strip.dataset.group = entry.adoptLabelKey;
      adoptInto(strip, entry.adopt);
      // Only if something actually arrived: a host that withheld the ribbon
      // leaves the ids unresolvable, and an empty named group announces a band
      // that is not there.
      if (strip.childElementCount) surface.appendChild(strip);
    }
    for (const section of entry.sections) {
      // The SAME named band the Table menu renders, from the same declaration:
      // `role="group"` with the band's translated name, and the rule between
      // bands drawn on the group's own top edge. A dropdown that showed the
      // taxonomy's bands as anonymous hairlines while the menu bar announced
      // them by name would be two answers to one question.
      let group = null;
      for (const id of section.ids) {
        const command = commands.get(id);
        if (!command) continue;
        if (!group) {
          group = document.createElement("div");
          group.className = "menu-group";
          group.setAttribute("role", "group");
          group.setAttribute("aria-label", t(section.nameKey));
          group.dataset.group = section.nameKey;
          surface.appendChild(group);
        }
        const item = document.createElement("button");
        item.type = "button";
        item.className = "menu-item";
        item.setAttribute("role", "menuitem");
        item.dataset.commandId = id;
        // The MENU label, not the palette's prefixed one: a row inside a menu
        // called Table already has that noun in front of it.
        item.textContent = entry.labels?.get(id) ?? command.label;
        if (command.enabled === false) {
          item.disabled = true;
          // `title` and not only the disabled attribute: the reason is the
          // sentence that turns a greyed row into an answer, and hover must not
          // erase it (`109` HF-118).
          item.title = command.disabledReason ?? "";
        } else {
          onButton(item, () => runCommand(id, item));
        }
        group.appendChild(item);
      }
    }
  }

  // ---- The align dropdown --------------------------------------------------
  // Trigger and menu are created ONCE and reused across renders: the popover
  // manager keeps a permanent reference to whatever is registered with it, so
  // registering a freshly built pair every render would leave the manager
  // holding detached nodes and dismissing the wrong surface.
  function ensureAlign(entry) {
    if (alignTrigger) return;
    alignTrigger = document.createElement("button");
    alignTrigger.type = "button";
    alignTrigger.id = "compactAlignBtn";
    alignTrigger.className = "ctool ctool-menu";
    alignTrigger.setAttribute("aria-haspopup", "menu");
    alignTrigger.setAttribute("aria-expanded", "false");
    alignTrigger.setAttribute("aria-controls", "compactAlignMenu");
    alignTrigger.appendChild(iconSpan(entry.icons[entry.ids[0]]));
    const caret = iconSpan("arrow_drop_down");
    caret.classList.add("ctool-caret");
    alignTrigger.appendChild(caret);

    alignMenu = document.createElement("div");
    alignMenu.id = "compactAlignMenu";
    alignMenu.className = "context-menu compact-align-menu";
    alignMenu.hidden = true;
    alignMenu.setAttribute("role", "menu");
    alignMenu.setAttribute("aria-label", "Alignment");
    // A fixed-position surface has to hang off <body>, or the bar's
    // `overflow: hidden` clips it — the same reason the ribbon's overflow menu
    // is moved there.
    document.body.appendChild(alignMenu);
    registerPopover(alignTrigger, alignMenu, () => reflectAlign(currentAlign));
  }

  function buildAlign(entry, commands) {
    ensureAlign(entry);
    alignItems = [];
    alignMenu.replaceChildren();
    for (const id of entry.ids) {
      const command = commands.get(id);
      if (!command) continue;
      const item = document.createElement("button");
      item.type = "button";
      item.className = "ctool compact-align-item";
      item.dataset.commandId = id;
      item.dataset.alignKey = ALIGN_KEY[id];
      item.setAttribute("role", "menuitemradio");
      item.setAttribute("aria-checked", "false");
      item.setAttribute("aria-label", command.label);
      item.title = command.shortcut
        ? localizeShortcut(`${command.label} (${command.shortcut})`)
        : command.label;
      item.appendChild(iconSpan(entry.icons[id]));
      onButton(item, () => runCommand(id));
      alignMenu.appendChild(item);
      alignItems.push({ id, el: item, icon: entry.icons[id], label: command.label });
    }
    paraControls.push(alignTrigger);
    contributed.push([paraControls, alignTrigger]);
    reflectAlign(currentAlign);
    return alignTrigger;
  }

  /** Points the trigger at the caret's alignment and checks the matching row.
   *  Docs' align button reports the current alignment rather than a fixed
   *  icon, which is the only thing that makes one button as informative as the
   *  four it replaces. */
  function reflectAlign(align) {
    currentAlign = align || "start";
    if (!alignTrigger || alignItems.length === 0) return;
    const active = alignItems.find((i) => ALIGN_KEY[i.id] === currentAlign) ?? alignItems[0];
    for (const item of alignItems) {
      item.el.setAttribute("aria-checked", String(item === active));
    }
    const glyph = alignTrigger.querySelector(".ms:not(.ctool-caret)");
    if (glyph) glyph.textContent = active.icon;
    alignTrigger.setAttribute("aria-label", `Alignment: ${active.label}`);
    alignTrigger.title = `Alignment: ${active.label}`;
  }

  // ---- Overflow ------------------------------------------------------------
  function ensureOverflow() {
    if (overflowBtn) return;
    overflowBtn = document.createElement("button");
    overflowBtn.type = "button";
    overflowBtn.id = "compactOverflowBtn";
    overflowBtn.className = "ctool ctool-overflow";
    overflowBtn.hidden = true;
    overflowBtn.setAttribute("aria-haspopup", "menu");
    overflowBtn.setAttribute("aria-expanded", "false");
    overflowBtn.setAttribute("aria-controls", "compactOverflowMenu");
    overflowBtn.setAttribute("aria-label", "More toolbar controls");
    overflowBtn.title = "More toolbar controls";
    overflowBtn.appendChild(iconSpan("more_horiz"));

    overflowMenu = document.createElement("div");
    overflowMenu.id = "compactOverflowMenu";
    overflowMenu.className = "context-menu compact-overflow-menu";
    overflowMenu.hidden = true;
    overflowMenu.setAttribute("role", "menu");
    overflowMenu.setAttribute("aria-label", "More toolbar controls");
    document.body.appendChild(overflowMenu);
    registerPopover(overflowBtn, overflowMenu, () => {});
  }

  /** Folds the groups that do not fit into the "⋯" menu, right to left, so the
   *  bar never shows a horizontal scrollbar. The pinned groups go last.
   *
   *  Deliberately synchronous and measured from the live boxes: a width model
   *  computed from declared sizes drifts the moment a font or a token changes,
   *  and the ribbon learned that the hard way (its first-paint fallback face
   *  measured wider than Inter and exiled a group permanently). */
  function reflow() {
    if (!host || host.hidden || rendered.length === 0) return;
    if (overflowMenu && !overflowMenu.hidden) {
      overflowMenu.hidden = true;
      overflowBtn.setAttribute("aria-expanded", "false");
    }
    for (const group of rendered) host.insertBefore(group.el, overflowBtn);
    overflowMenu.replaceChildren();
    overflowBtn.hidden = true;
    // A contextual group that is not showing takes part in nothing: it has no
    // width, and counting the GAP beside it would make the bar fold a real group
    // to make room for a group nobody can see.
    const visible = rendered.filter((group) => !group.el.hidden);
    if (visible.length === 0) return;
    const style = getComputedStyle(host);
    const avail =
      host.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
    // An unmeasurable bar is not a narrow bar. Hidden, zero-width or mid mode
    // switch, every group would "not fit" and be exiled on no evidence at all.
    if (!(avail > 0)) return;
    const gap = parseFloat(style.columnGap) || 0;
    // MARGINS COUNT. `offsetWidth` stops at the border box, and the dividered
    // groups carry a left margin; measuring without it under-reported the bar
    // by 2px per divider, which is how "everything fits" was decided at a width
    // where the last group was in fact 8px outside the bar.
    const widths = visible.map((group) => {
      const box = getComputedStyle(group.el);
      return (
        group.el.offsetWidth + (parseFloat(box.marginLeft) || 0) + (parseFloat(box.marginRight) || 0)
      );
    });
    let total = widths.reduce((sum, w) => sum + w, 0) + gap * (visible.length - 1);
    if (total <= avail + 0.5) return; // everything fits — no overflow control
    const reserve = 38; // room for the ⋯ button itself
    const moved = new Set();
    for (let pass = 0; pass < 2 && total > avail - reserve; pass++) {
      for (let i = visible.length - 1; i >= 1 && total > avail - reserve; i--) {
        if (moved.has(i)) continue;
        // First pass folds only the unpinned groups; the second is the last
        // resort at a width where even B/I/U cannot stay, and it still leaves
        // the first group (undo/redo) inline.
        if (pass === 0 && visible[i].pinned) continue;
        moved.add(i);
        total -= widths[i] + gap;
      }
    }
    for (let i = 0; i < visible.length; i++) {
      if (moved.has(i)) overflowMenu.appendChild(visible[i].el);
    }
    overflowBtn.hidden = moved.size === 0;
  }

  let frame = 0;
  function scheduleReflow() {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(reflow);
  }

  /** Rebuilt on mode entry rather than kept in sync, so it cannot hold a stale
   *  reference to a command that changed. */
  function render() {
    if (!host) return;
    const commands = registry();
    // Put any previously adopted control back in the ribbon FIRST. Clearing the
    // host detaches them, and a detached node is not findable by id — so a
    // second render (the reload path rebuilds once the document loads) would
    // silently drop zoom, style, font and size.
    release();
    ensureOverflow();
    host.replaceChildren();
    // The toggle-button cache holds the previous render's nodes; keeping it
    // would leave the new buttons unsynced and write state into detached ones.
    formatToggleCache.clear();
    rendered = [];
    for (const group of roster()) {
      const el = document.createElement("span");
      el.className = "cgroup";
      el.dataset.group = group.group;
      el.setAttribute("role", "group");
      el.setAttribute("aria-label", group.labelKey ? t(group.labelKey) : group.label);
      if (group.divider) el.dataset.divider = "true";
      for (const entry of group.items) {
        if (entry.kind === "adopt") {
          const adopted = document.getElementById(ADOPTED_CONTROL_IDS[entry.control]);
          if (!adopted) continue;
          if (!adoptedHome.has(adopted)) {
            adoptedHome.set(adopted, { parent: adopted.parentNode, next: adopted.nextSibling });
          }
          adopted.classList.add("cadopted");
          el.appendChild(adopted);
          continue;
        }
        if (entry.kind === "align") {
          const trigger = buildAlign(entry, commands);
          if (trigger) el.appendChild(trigger);
          continue;
        }
        if (entry.kind === "menu") {
          el.appendChild(ensureMenu(entry).trigger);
          continue;
        }
        const command = commands.get(entry.id);
        if (!command) continue; // the parity guard fails the build on this
        el.appendChild(button(command, entry));
      }
      if (el.childElementCount === 0) continue;
      if (group.contextual) el.hidden = !tableContext;
      host.appendChild(el);
      rendered.push({
        el,
        group: group.group,
        pinned: !!group.pinned,
        contextual: !!group.contextual,
      });
    }
    host.appendChild(overflowBtn);
    reflow();
  }

  if (host && typeof ResizeObserver === "function") {
    new ResizeObserver(scheduleReflow).observe(host);
  } else {
    window.addEventListener("resize", scheduleReflow);
  }
  // The bar is measured in whatever face is available at first paint. Until
  // Inter arrives that is a fallback with wider metrics, so the groups measure
  // wider than they will ever be drawn — enough to fold a group that in fact
  // fits. Re-decide when the real metrics are in. (The ResizeObserver cannot
  // save us: the bar stays exactly as wide as the window.)
  if (document.fonts?.ready) document.fonts.ready.then(scheduleReflow).catch(() => {});

  /** Shows or hides the contextual group(s). Called from the one toolbar sync,
   *  which already knows whether the caret is in a table; re-laying out only on
   *  a CHANGE keeps a caret move inside a table free. */
  function setTableContext(inTable) {
    if (tableContext === !!inTable) return;
    tableContext = !!inTable;
    let changed = false;
    for (const group of rendered) {
      if (!group.contextual) continue;
      group.el.hidden = !tableContext;
      changed = true;
    }
    if (changed) scheduleReflow();
  }

  return { render, release, reflectAlign, reflow, scheduleReflow, setTableContext };
}
