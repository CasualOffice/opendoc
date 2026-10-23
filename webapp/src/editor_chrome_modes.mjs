// Chrome modes: the compact toolbar and the ribbon, the persisted choice, and
// the phone layout's override. Moved out of `main.js` unchanged (docs/177); it
// reaches the rest of the editor only through the context `ed`.
import { createCompactToolbar } from "./compact_toolbar.mjs";
import { localizeShortcutText } from "./shortcut_labels.mjs";
import { bindRadioGroup } from "./radio_group.mjs";
import { onButton, registerPopover } from "./popover_manager.mjs";
import { readPref, writePref } from "./prefs.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createChromeModes(ed) {
  // ---- Chrome modes: compact vs ribbon (owner prototype) ---------------------
  //
  // Two mutually exclusive toolbars. The ribbon is the Word-shaped tabbed chrome;
  // compact is the Docs-shaped single dense bar. The bar's layout table, its
  // grouping and its fold live in `compact_toolbar.mjs` (HF-085: new logic goes
  // in a module); what stays here is the mode switch and the dependency bag.
  const CHROME_MODE_PREF = "opendoc.chromeMode";
  let chromeMode = readPref(CHROME_MODE_PREF, "ribbon") === "compact" ? "compact" : "ribbon";

  ed.compactToolbarUi = createCompactToolbar({
    host: document.getElementById("compactToolbar"),
    editorCommands: ed.editorCommands,
    onButton,
    registerPopover,
    runControls: ed.runControls,
    paraControls: ed.paraControls,
    formatToggleCache: ed.formatToggleCache,
    localizeShortcut: (text) => localizeShortcutText(text, ed.EDITOR_KEYBOARD_PLATFORM),
    // Which roster the bar draws. A phone gets Docs' row over two named sheets
    // (`PHONE_TOOLBAR`) instead of the desktop's thirteen groups over a fold whose
    // membership changes with the window width — see `compact_toolbar.mjs`.
    // Resolved at render time, not captured, so crossing the rung switches it.
    isPhone: () => ed.phoneChrome.isPhone(),
  });

  /** Switches chrome. Mutually exclusive by construction — `body` carries exactly
   *  one mode class — and the caret is never disturbed, because neither chrome
   *  owns the editing surface. */
  function setChromeMode(mode, { persist = true, announce = true } = {}) {
    chromeMode = mode === "compact" ? "compact" : "ribbon";
    // A phone has room for one navigation axis and the ribbon is not it (docs/148
    // §5). The preference is untouched, so growing back past the rung restores it.
    const compact = chromeMode === "compact" || ed.phoneChrome.isPhone();
    // The File PAGE is ribbon chrome: compact mode hides the whole ribbon, so
    // switching modes with it open would leave `.file-page-open` set over a page
    // nobody can see or leave. Compact mode answers File with a dropdown instead.
    if (compact) ed.closeFilePage();
    document.body.classList.toggle("compact-mode", compact);
    document.body.classList.toggle("ribbon-mode", !compact);
    // The toolbar IS the chrome — the wrapper it used to sit in was a second
    // nested box, which is what read as a doubled outline on both sides and the
    // bottom.
    const bar = document.getElementById("compactToolbar");
    if (bar) bar.hidden = !compact;
    chromeModeGroup?.reflect(chromeMode);
    // With no document open the registry holds only `noDoc` commands, so the bar
    // would render nearly empty; it is rebuilt on `doc-loaded`. Rendering here
    // anyway keeps the adopted controls in place so the bar is never a bare strip.
    if (compact) ed.compactToolbarUi.render();
    else ed.compactToolbarUi.release();
    if (persist) writePref(CHROME_MODE_PREF, chromeMode);
    if (announce) ed.setStatus(compact ? "Compact toolbar" : "Ribbon toolbar", "", { timeout: 1800 });
  }

  const chromeModeGroup = bindRadioGroup(document.querySelector(".chrome-mode"), {
    attr: "data-chrome-mode",
    onSelect: (mode) => setChromeMode(mode),
  });
  setChromeMode(chromeMode, { persist: false });
  // Silently: nobody asked to cross the rung, so a toast on every rotation would
  // be a notification about the window rather than about the document.
  ed.phoneChrome.onPhoneChange(() => setChromeMode(chromeMode, { persist: false, announce: false }));

}
