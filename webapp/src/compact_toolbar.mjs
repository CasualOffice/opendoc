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
}) {
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
    const widths = rendered.map((group) => {
      const box = getComputedStyle(group.el);
      return (
        group.el.offsetWidth + (parseFloat(box.marginLeft) || 0) + (parseFloat(box.marginRight) || 0)
      );
    });
    let total = widths.reduce((sum, w) => sum + w, 0) + gap * (rendered.length - 1);
    if (total <= avail + 0.5) return; // everything fits — no overflow control
    const reserve = 38; // room for the ⋯ button itself
    const moved = new Set();
    for (let pass = 0; pass < 2 && total > avail - reserve; pass++) {
      for (let i = rendered.length - 1; i >= 1 && total > avail - reserve; i--) {
        if (moved.has(i)) continue;
        // First pass folds only the unpinned groups; the second is the last
        // resort at a width where even B/I/U cannot stay, and it still leaves
        // the first group (undo/redo) inline.
        if (pass === 0 && rendered[i].pinned) continue;
        moved.add(i);
        total -= widths[i] + gap;
      }
    }
    for (let i = 0; i < rendered.length; i++) {
      if (moved.has(i)) overflowMenu.appendChild(rendered[i].el);
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
    for (const group of table) {
      const el = document.createElement("span");
      el.className = "cgroup";
      el.dataset.group = group.group;
      el.setAttribute("role", "group");
      el.setAttribute("aria-label", group.label);
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
        const command = commands.get(entry.id);
        if (!command) continue; // the parity guard fails the build on this
        el.appendChild(button(command, entry));
      }
      if (el.childElementCount === 0) continue;
      host.appendChild(el);
      rendered.push({ el, group: group.group, pinned: !!group.pinned });
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

  return { render, release, reflectAlign, reflow, scheduleReflow };
}
