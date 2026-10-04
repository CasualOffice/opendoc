// The deck viewer's command registry, and the rosters its surfaces draw from.
//
// # Why a registry at all, for a page with nine commands
//
// Because the editor already proved the alternative. `105` UX-004/UX-005 record
// what happens when a second surface hand-binds its own listeners: the ribbon and
// the menu drift, one of them keeps an enablement rule the other forgot, and a
// command ends up enabled in one chrome and dead in the other. Every control on
// this page therefore names a command ID, and the ID resolves here — so a
// command has one enablement path, one activation path and one label whichever
// surface is showing.
//
// It is also what makes `compact_toolbar.mjs` and `command_menu.mjs` reusable
// here at all. Both take their registry as a dependency rather than importing
// the editor's (`module_seams.test.mjs` enforces that neither reaches back into
// `main.js`), so supplying this is the whole of what a second page owes them.
//
// # What a VIEWER's commands are
//
// Not a reduced editor's. This facade exposes no operation set — `casual-pres-wasm`
// has no slide operations and no transaction envelope, and a surface that could
// type would repeat the defect `105` CQ-002 records on the document path — so
// there is nothing here that changes a deck. What is left is everything both
// reference products put in a viewer: open and save a copy, move through the
// deck, change what you are looking at and how big it is, and show or hide a
// panel. Google Slides and ONLYOFFICE agree on that roster almost exactly; where
// they differ, the row is absent rather than present-and-dead (`docs/63`).

import { keyboardPlatform } from "./keyboard.mjs";
import { shortcutForCommand } from "./keymap.mjs";

/** The viewer's keyboard map, in `keymap.mjs`'s own row shape.
 *
 *  Chords are DECLARED in Apple glyphs — "⌘S" — because that is what the
 *  registry stores and what `shortcut_labels.mjs` renders for the keyboard in
 *  front of the reader; a map written in Control would read wrongly on a Mac and
 *  there would be no single place to fix it.
 *
 *  Arrow keys are NOT here. `keymap.mjs` describes chords — a modifier plus a
 *  key — and paging a deck with an unmodified Arrow, Page or Space key is not a
 *  chord: it is the primary interaction of the surface that has focus, the same
 *  way Arrow moves a caret in the editor rather than running a command. Putting
 *  them here would make every Arrow press in the deck a global accelerator.
 */
export const SLIDE_KEYMAP = [
  // The EDITOR'S spellings, not new ones. `keymap.mjs` already binds `⌘=` and
  // `⌘-` to `view.zoomIn`/`view.zoomOut` for the document, and an unshifted `+`
  // is a key no keyboard produces — a first draft of this table wrote `⌘+` and
  // the chord simply never fired. Same chord, same command id, both surfaces.
  { chord: "⌘S", command: "file.save" },
  { chord: "⌘O", command: "file.open" },
  { chord: "⌘=", command: "view.zoomIn" },
  { chord: "⌘-", command: "view.zoomOut" },
  { chord: "⌘0", command: "view.fitSlide" },
];

/** The Material Symbols ligature each command shows, where a surface wants one. */
const ICONS = {
  "file.open": "folder_open",
  "file.save": "save",
  "slide.first": "first_page",
  "slide.previous": "chevron_left",
  "slide.next": "chevron_right",
  "slide.last": "last_page",
  "view.zoomOut": "zoom_out",
  "view.zoomIn": "zoom_in",
  "view.fitSlide": "fit_screen",
  "view.fitWidth": "width_full",
  "view.slides": "auto_stories",
  "view.fidelity": "fact_check",
};

/**
 * Builds the registry for one booted viewer.
 *
 * `actions` carries the things only the page can do — opening the file picker,
 * repainting, toggling a panel — so this module owns the command TABLE and the
 * page owns the side effects, which is the split that keeps this testable
 * without a DOM.
 *
 * Returns `editorCommands(context)`, the exact callable shape
 * `createCompactToolbar` and `createMenuBar` expect: an array of
 * `{ id, label, group, icon, enabled, disabledReason, checked, run }`. `enabled`
 * is a BOOLEAN and not a predicate, because every surface gates on
 * `enabled === false` — passing a function there made every click a no-op once.
 */
export function createSlideCommands({ viewer, actions, t }) {
  const open = () => viewer.slideCount() > 0;
  const noDeck = () => t("slides.noDeckReason");

  return function editorCommands() {
    const count = viewer.slideCount();
    const index = viewer.currentIndex();
    const fit = viewer.currentFit();
    const go = (to) => () => {
      viewer.goTo(to);
      actions.repaint();
    };
    const commands = [
      {
        id: "file.open",
        label: t("slides.chooseFile"),
        group: t("slides.menuFile"),
        kw: "open load pptx presentation deck",
        // Reachable with nothing open — it is the only command that can be, and
        // the one a reader arriving at an empty viewer needs.
        noDoc: true,
        enabled: true,
        run: () => actions.openPicker(),
      },
      {
        id: "file.save",
        label: t("slides.save"),
        group: t("slides.menuFile"),
        kw: "save copy download export pptx",
        enabled: open(),
        disabledReason: noDeck(),
        run: () => actions.save(),
      },
      {
        id: "slide.first",
        label: t("slides.firstSlide"),
        group: t("slides.menuSlide"),
        kw: "first start beginning home",
        enabled: open() && index > 0,
        disabledReason: noDeck(),
        run: go(0),
      },
      {
        id: "slide.previous",
        label: t("slides.previousSlide"),
        group: t("slides.menuSlide"),
        kw: "previous back prior up",
        enabled: open() && index > 0,
        disabledReason: noDeck(),
        run: go(index - 1),
      },
      {
        id: "slide.next",
        label: t("slides.nextSlide"),
        group: t("slides.menuSlide"),
        kw: "next forward advance down",
        enabled: open() && index < count - 1,
        disabledReason: noDeck(),
        run: go(index + 1),
      },
      {
        id: "slide.last",
        label: t("slides.lastSlide"),
        group: t("slides.menuSlide"),
        kw: "last end final",
        enabled: open() && index < count - 1,
        disabledReason: noDeck(),
        run: go(count - 1),
      },
      {
        id: "view.zoomOut",
        label: t("slides.zoomOut"),
        group: t("slides.menuView"),
        kw: "zoom out smaller reduce shrink",
        enabled: open() && viewer.canZoom(-1),
        disabledReason: noDeck(),
        run: () => {
          viewer.stepZoom(-1);
          actions.repaint();
        },
      },
      {
        id: "view.zoomIn",
        label: t("slides.zoomIn"),
        group: t("slides.menuView"),
        kw: "zoom in larger magnify enlarge",
        enabled: open() && viewer.canZoom(1),
        disabledReason: noDeck(),
        run: () => {
          viewer.stepZoom(1);
          actions.repaint();
        },
      },
      {
        id: "view.fitSlide",
        label: t("slides.fitSlide"),
        group: t("slides.menuView"),
        kw: "fit slide whole page screen",
        enabled: open(),
        disabledReason: noDeck(),
        // `checked` rather than a separate pressed list: the menu and the bar both
        // read it, so the two cannot disagree about which fit is in force.
        checked: fit === "slide",
        run: () => {
          viewer.setFit("slide");
          actions.repaint();
        },
      },
      {
        id: "view.fitWidth",
        label: t("slides.fitWidth"),
        group: t("slides.menuView"),
        kw: "fit width desk fill",
        enabled: open(),
        disabledReason: noDeck(),
        checked: fit === "width",
        run: () => {
          viewer.setFit("width");
          actions.repaint();
        },
      },
      {
        id: "view.slides",
        label: t("slides.sorter"),
        group: t("slides.menuView"),
        kw: "slides sorter thumbnails panel filmstrip",
        noDoc: true,
        enabled: true,
        checked: actions.panelShown("slides"),
        run: () => actions.togglePanel("slides"),
      },
      // The theme, through `appearance.mjs` and `prefs.mjs` — the modules that
      // already own "what `data-theme` means" and "where a preference lives".
      // Three rows rather than a toggle, because `system` is a real third state
      // and a two-way switch cannot say "follow the OS".
      ...["system", "light", "dark"].map((theme) => ({
        id: `view.theme.${theme}`,
        label: t(`slides.theme.${theme}`),
        group: t("slides.menuView"),
        kw: `theme appearance ${theme} dark light colour color scheme`,
        noDoc: true,
        enabled: true,
        checked: actions.theme() === theme,
        run: () => actions.setTheme(theme),
      })),
      {
        id: "view.fidelity",
        label: t("slides.fidelityPanel"),
        group: t("slides.menuView"),
        kw: "fidelity findings report lost unsupported",
        enabled: open(),
        disabledReason: noDeck(),
        checked: actions.panelShown("fidelity"),
        run: () => actions.togglePanel("fidelity"),
      },
    ];
    // Each command carries the chord its own keymap row declares, so a tooltip
    // and a menu row advertise exactly what the dispatcher binds. `109`
    // UX-006/UX-007 is what happens when those are two tables: the editor
    // advertised chords it had never bound and bound one it advertised nowhere.
    // `shortcutForCommand` is `keymap.mjs`'s, reading OUR rows.
    const platform = keyboardPlatform();
    return commands.map((command) => ({
      icon: ICONS[command.id],
      shortcut: shortcutForCommand(command.id, platform, SLIDE_KEYMAP),
      ...command,
    }));
  };
}

/** The compact bar's roster, in the shape `createCompactToolbar` reads.
 *
 *  Order follows what both reference products put in front of a reader of a
 *  deck: move through it, change how big it is, then the panels. File sits at
 *  the left because it is where every other surface in this product puts it.
 *
 *  Each group names a `labelKey` rather than carrying English: `compact_toolbar`
 *  resolves it through the catalogue, and a raw `label` would be an unrouted
 *  string the seam cannot reach.
 *
 *  Nothing here is `adopt`ed from a ribbon, because this page has no ribbon yet —
 *  every row is a plain command button, which is also why the bar cannot leave a
 *  hole in a band it borrowed from. */
export const SLIDE_TOOLBAR = [
  {
    group: "file",
    labelKey: "slides.toolbarGroupFile",
    pinned: true,
    items: [
      { id: "file.open", icon: ICONS["file.open"] },
      { id: "file.save", icon: ICONS["file.save"] },
    ],
  },
  {
    group: "navigate",
    labelKey: "slides.toolbarGroupSlide",
    divider: true,
    pinned: true,
    items: [
      { id: "slide.first", icon: ICONS["slide.first"] },
      { id: "slide.previous", icon: ICONS["slide.previous"] },
      { id: "slide.next", icon: ICONS["slide.next"] },
      { id: "slide.last", icon: ICONS["slide.last"] },
    ],
  },
  {
    group: "zoom",
    labelKey: "slides.toolbarGroupZoom",
    divider: true,
    pinned: true,
    items: [
      { id: "view.zoomOut", icon: ICONS["view.zoomOut"] },
      { id: "view.zoomIn", icon: ICONS["view.zoomIn"] },
      { id: "view.fitSlide", icon: ICONS["view.fitSlide"], toggle: true },
      { id: "view.fitWidth", icon: ICONS["view.fitWidth"], toggle: true },
    ],
  },
  {
    group: "panels",
    labelKey: "slides.toolbarGroupPanels",
    divider: true,
    items: [
      { id: "view.slides", icon: ICONS["view.slides"], toggle: true },
      { id: "view.fidelity", icon: ICONS["view.fidelity"], toggle: true },
    ],
  },
];

/** The menu bar's sections, in the shape `createMenuBar`'s `sectionsFor` returns.
 *
 *  Three menus, because a viewer has three kinds of command and inventing a
 *  fourth to look like a word processor would be the dead chrome `docs/63`
 *  forbids. The bands inside each follow the same rule the editor's taxonomy
 *  states: a band is a group of rows that answer the same question. */
export const SLIDE_MENU_SECTIONS = {
  file: [{ group: "slides.menuFile", ids: ["file.open", "file.save"] }],
  slide: [
    { group: "slides.menuGroupMove", ids: ["slide.previous", "slide.next"] },
    { group: "slides.menuGroupEnds", ids: ["slide.first", "slide.last"] },
  ],
  view: [
    { group: "slides.menuGroupZoom", ids: ["view.zoomOut", "view.zoomIn"] },
    { group: "slides.menuGroupFit", ids: ["view.fitSlide", "view.fitWidth"] },
    { group: "slides.menuGroupPanels", ids: ["view.slides", "view.fidelity"] },
    {
      group: "slides.menuGroupTheme",
      ids: ["view.theme.system", "view.theme.light", "view.theme.dark"],
    },
  ],
};
