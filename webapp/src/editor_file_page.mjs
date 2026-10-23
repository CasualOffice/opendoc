// One navigation axis: the compact menu bar and the ribbon's File page. Moved
// out of `main.js` unchanged (docs/177); it reaches the rest of the editor only
// through the context `ed`.
import { EXPORT_COMMANDS } from "./export_commands.mjs";
import { formatInfo } from "./format_io.mjs";
import { formatShortcut } from "./keyboard.mjs";
import { onButton } from "./popover_manager.mjs";
import { APP_MENU_SECTIONS, fileMenuSections } from "./command_taxonomy.mjs";
import { createMenuBar, renderCommandRows } from "./command_menu.mjs";
import { fileCategoryRowFor, renderFilePane, setFilePane } from "./file_pane.mjs";
import { DOCUMENT_TEMPLATES, templateBytes, templateName, templateThumbnail } from "./blank_document.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createFilePage(ed) {
  // ---- One navigation axis: the compact menu bar and the ribbon's File page ---
  // The editor used to show TWO navigation systems at once — this menu bar and the
  // ribbon tab strip — so a command's home was a guess between two places (`109`
  // UX-014). Each chrome now has exactly one axis (docs/122):
  //
  //   compact mode  the menu bar IS the axis; File is a dropdown, as in Google
  //                 Docs and Drive. `style.css` hides the bar in ribbon mode.
  //   ribbon mode   the tab strip is the axis; File is its first tab and opens a
  //                 PAGE, as in ONLYOFFICE (`app/view/Toolbar.js:182`, the only
  //                 tab declared `haspanel: false`).
  //
  // Both read `FILE_SURFACE` and both render through `command_menu.mjs`, so the
  // two File rosters cannot drift and neither can invent its own gating: the rows
  // come from the same `editorCommands()` descriptors the palette and the context
  // menu use.
  const appMenuBar = document.getElementById("appMenuBar");
  const appMenuPopover = document.getElementById("appMenuPopover");
  const menuRegistry = () =>
    new Map(
      ed.editorCommands({ surface: "menu", hasRange: ed.hasRange() }).map((command) => [command.id, command]),
    );
  const appMenu = createMenuBar({
    bar: appMenuBar,
    popover: appMenuPopover,
    sectionsFor: (name) => APP_MENU_SECTIONS[name] ?? [],
    registry: menuRegistry,
    formatShortcut,
  });
  const closeAppMenu = (options) => appMenu.close(options);

  // The File page. Same rows, rendered as headed groups instead of a dropdown,
  // because a full-window route has the room to say what a group is and a dropdown
  // does not. Rebuilt on every open rather than kept in sync — the same reason the
  // compact toolbar is — so a row can never hold a stale `enabled`.
  function renderFilePage() {
    if (!ed.filePageBody) return 0;
    renderFilePane({
      menuRegistry,
      exportRows: EXPORT_COMMANDS,
      formatInfoOf: formatInfo,
      closeFilePage: ed.closeFilePage,
      templates: DOCUMENT_TEMPLATES,
      templateThumbnailFor: templateThumbnail,
      onTemplate: async (id) => {
        ed.closeFilePage();
        if (!(await ed.confirmDiscardIfEdited())) return;
        await ed.openBytes(templateBytes(id), templateName(id));
      },
      // What each panel's dialog does on open, so the pane shows the same thing.
      prepare: (pane) => {
        if (pane === "shortcuts") ed.buildShortcutsReference();
        if (pane === "commands") {
          // The palette renders its rows from the live registry when it opens;
          // shown as a pane nothing had asked it to, so it came up as an empty
          // search box. Same call, and the query starts clean.
          const input = document.getElementById("cmdInput");
          if (input) input.value = "";
          ed.renderCommands("");
          // You came to this pane to type a command name. Both entry points —
          // opening the File tab on this pane, and selecting it in the rail —
          // are moments the person chose it, so taking the keyboard cannot
          // interrupt anything they were doing.
          input?.focus();
        }
        if (pane === "about") ed.toggleAbout.stampVersion?.(); else if (pane === "pageSetup") ed.pageSetup.reflect?.();
        if (pane === "properties") ed.propertiesUi.fill();
      },
    });
    // The rail lists every File command, but three kinds of them are rendered as
    // a CATEGORY row that opens a pane instead of a row that runs: the six export
    // formats collapse into one `Export`, and the rows that used to open a dialog
    // over the page — Settings, Document properties, the Help rows, New — open
    // panes. The owner asked for exactly this — "basically in this view replace
    // dialogs with this space", and for export, "one export on left and right
    // list of them with icons, like in onlyoffice".
    //
    // `fileMenuSections()` itself is untouched, so the compact chrome's File
    // DROPDOWN — which has no pane to open — still runs all of them directly, and
    // each category row records the ids it stands in for so the two surfaces can
    // be proved to offer the same roster.
    const count = renderCommandRows(
      ed.filePageBody,
      fileMenuSections(),
      menuRegistry(),
      {
        itemClass: "file-page-item",
        headings: true,
        formatShortcut,
        rowFor: (id, ids) =>
          fileCategoryRowFor(id, ids, {
            itemClass: "file-page-item",
            onSelect: (pane) => {
              setFilePane(pane);
              renderFilePage();
            },
          }),
        onRun: (command) => {
          // Every File row returns to the document first, which is what a dropdown
          // does implicitly by closing and what ONLYOFFICE's page does through its
          // own "Back to Document". One rule, no exception list: Settings and the
          // Help rows open dialogs, and a modal over a full-window page is two
          // layers of chrome between the user and the document they came for.
          //
          // Through `closeFilePage` rather than `selectRibbonTab` alone, because
          // the row the click landed on is about to be hidden: without moving the
          // keyboard somewhere real first, focus falls to <body> and a dialog
          // opened from here has nowhere to hand it back to on Escape — the
          // HF-062 failure, where the editor looked frozen after a dialog closed.
          ed.closeFilePage();
          command.run();
        },
      },
    );
    return count;
  }


  {
    const back = document.getElementById("filePageBack");
    if (back) onButton(back, () => ed.closeFilePage());
  }

  function buildCommands() {
    return ed.editorCommands({ surface: "palette" });
  }

  return { buildCommands, closeAppMenu, renderFilePage };
}
