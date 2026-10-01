// The ¶ button: Word's Show/Hide formatting marks, and its five individual
// switches.
//
// `SKILL` §9 rule 4 — "built" is not "reachable". `casual_doc_layout`'s
// `FormattingMarks` paints the pilcrow, the tab arrow, the space dot, the return
// arrow at a hard line break and the rule at a page or column break; the wasm
// facade exposes the whole of it as ONE patch call plus a getter; and no control
// anywhere in `webapp/` ever called either, so the most-pressed button in a word
// processor did not exist.
//
// ---- THE SHAPE, FROM THE THREE REFERENCES ----------------------------------
//
//   * Word 365: Home ▸ Paragraph carries the ¶ button (Show/Hide ¶), and the five
//     marks are individually switchable in File ▸ Options ▸ Display ("Always show
//     these formatting marks on the screen": Tab characters, Spaces, Paragraph
//     marks, …). One gesture for all of them, the individual choices buried three
//     dialogs deep. The Mac chord is ⌘8.
//   * ONLYOFFICE, read from their source rather than their documentation
//     (`apps/documenteditor/main/app/view/Toolbar.js:785-808`): `btnShowHidenChars`
//     is a SPLIT toggle in their Home toolbar's paragraph controls —
//     `enableToggle: true, split: true, action: 'hidden-chars'` — whose menu holds
//     two checkable rows (`mniHiddenChars`, `mniHiddenBorders`). They register it
//     in `shortcutHints.ShowAll`, so they advertise a chord for it too.
//   * Google Docs: View ▸ Show non-printing characters. One switch, no parts.
//
// So: a SPLIT control on Home ▸ Paragraph, which is Word's placement and
// ONLYOFFICE's mechanism — the main half toggles everything, the caret opens the
// five switches. Word's parts are real and are worth having; what is not worth
// copying is burying them in an Options backstage this editor does not have. Both
// halves are one engine call, because `setFormattingMarks` takes a PATCH: `all`
// resolves first, so `{"all":true}` is the ¶ button and `{"tab":false}` is one row
// of the menu, and the state comes back from the same call.
//
// ---- WHAT THIS DELIBERATELY DOES NOT OFFER ---------------------------------
//
// `color`. The engine's default is `null`, meaning each mark takes the LINE'S OWN
// INK, and that is the setting to keep rather than expose: it is precisely the
// ONLYOFFICE defect this seam exists not to reproduce — they hard-code black for
// the tab and break marks, so those vanish on a dark page. A colour picker here
// would be a control whose only use is to break the dark theme.
//
// Word's other three Display marks — hidden text, optional hyphens and object
// anchors — are not offered because the engine paints no mark for them. A switch
// for a mark that cannot be drawn is the dead control the working contract
// forbids; the honest form of that gap is its absence plus this sentence.
//
// ---- IT IS A REPAINT, NOT A REPAGINATION -----------------------------------
//
// The setter bumps a view epoch and issues no `Operation`: no page boundary
// moves, the caret does not move, the document revision does not move and undo is
// untouched. So the chrome owes it exactly what it owes a zoom change —
// re-raster the pages on screen — and nothing else. `repaint` is injected for
// that reason and is O(the materialized page window), never O(document).
// `view-formatting-marks.spec.mjs` asserts the page count is unchanged, which is
// the half a "the marks appeared" test cannot see.
import { t } from "./i18n.mjs";
// `authoredTitle` and the keyboard platform, imported rather than injected for the
// same reason the popover registry is: there is one implementation of "what does
// this control's title say when it is live", every band already uses it, and
// threading it through `main.js` as two host members would be two more lines in
// the file the ratchet protects for no reader.
import { authoredTitle } from "./localize.mjs";
import { keyboardPlatform } from "./keyboard.mjs";
import { loadPrefObject, savePrefObject } from "./prefs.mjs";
// Imported rather than injected, the `break_commands.mjs` shape: ESM gives one
// popover registry, so a popover registered here joins the same list and gets the
// same outside-click dismissal, Escape handling, `aria-expanded` bookkeeping and
// focus restoration. Injecting them would buy nothing but lines in the file the
// ratchet protects.
import { onButton, registerPopover } from "./popover_manager.mjs";

/** The five marks the engine can actually paint, in Word's *Display* order.
 *
 *  `key` is the engine's own patch field, so nothing here maps one spelling onto
 *  another — a second vocabulary for five booleans is how two tables start
 *  disagreeing about which switch means which mark. */
export const FORMATTING_MARKS = Object.freeze([
  Object.freeze({ key: "tab", labelKey: "formattingMarks.tab" }),
  Object.freeze({ key: "space", labelKey: "formattingMarks.space" }),
  Object.freeze({ key: "paragraph", labelKey: "formattingMarks.paragraph" }),
  Object.freeze({ key: "lineBreak", labelKey: "formattingMarks.lineBreak" }),
  Object.freeze({ key: "pageBreak", labelKey: "formattingMarks.pageBreak" }),
]);

/** Every command id this control owns: the ¶ toggle and the five switches. */
export const FORMATTING_MARK_COMMANDS = Object.freeze([
  "view.formattingMarks",
  ...FORMATTING_MARKS.map((mark) => `view.formattingMarks.${mark.key}`),
]);

/** Where the preference is kept, and what it is before anyone has chosen.
 *
 *  Marks OFF on a first run, which is Word's default and Docs' and ONLYOFFICE's:
 *  a document should look like the document. */
const PREF_KEY = "opendoc.formattingMarks";
const PREF_DEFAULTS = Object.freeze(
  Object.fromEntries(FORMATTING_MARKS.map((mark) => [mark.key, false])),
);

/** The control's two halves and its popover, in `editor.html`.
 *
 *  Named here because this module is the only thing that reads them, and three
 *  element ids travelling through `main.js` as host members would be three more
 *  lines in the file the ratchet protects, for no reader. */
const TOGGLE_ID = "formattingMarksBtn";
const MENU_BUTTON_ID = "formattingMarksMenuBtn";
const MENU_ID = "formattingMarksMenu";

/**
 * The ¶ control: its two ribbon halves, its popover, and its six command rows.
 *
 * The PREFERENCE is the person's and outlives the document — Word remembers
 * Show/Hide across sessions and ONLYOFFICE persists `de-settings-showhiddenchars`
 * — but the STATE lives on the document handle, because that is where the
 * renderer reads it. So `adopt()` re-applies the preference to a freshly opened
 * document, and it is the setter's own idempotence that makes that safe: a patch
 * resolving to the state already held bumps no revision at all.
 *
 * @param {object} io
 * @param {() => object|null} io.getDoc the live document, or null.
 * @param {() => void} io.repaint re-raster the pages on screen. O(window).
 * @param {(text: string, kind?: string) => void} io.setStatus
 * @returns {{commands: () => Array<object>, reflect: () => void, adopt: () => void,
 *            state: () => object, isOn: () => boolean}}
 */
export function createFormattingMarks(io) {
  const toggle = document.getElementById(TOGGLE_ID);
  const menuButton = document.getElementById(MENU_BUTTON_ID);
  const menu = document.getElementById(MENU_ID);
  /** The person's choice, which is what `adopt` replays onto a new document. */
  let preference = loadPrefObject(PREF_KEY, PREF_DEFAULTS);

  /** What the ENGINE is painting, read back rather than remembered.
   *
   *  The getter is the authority on purpose: binding the ¶ button to what the
   *  chrome asked for rather than to what took effect is how a control starts
   *  lying about the page. `any` comes from the engine too — it is the ¶ button's
   *  own state, and OR-ing five fields here would be a second implementation of
   *  one rule. */
  function engineState() {
    const doc = io.getDoc();
    if (!doc) return { ...PREF_DEFAULTS, any: false };
    try {
      return JSON.parse(doc.formattingMarks);
    } catch {
      // A getter that cannot be parsed is a broken build, not a user error. The
      // control reads as "off" rather than throwing out of a render pass.
      return { ...PREF_DEFAULTS, any: false };
    }
  }

  /** Sends one patch and repaints. Returns whether it was accepted.
   *
   *  The setter THROWS for a patch it does not understand, and that refusal is
   *  shown rather than swallowed: this chrome only ever sends keys the engine
   *  declares, so a throw here means the two have drifted, and silence would make
   *  the ¶ button a control that sometimes does nothing. */
  function apply(patch) {
    const doc = io.getDoc();
    if (!doc) return false;
    let next;
    try {
      next = JSON.parse(doc.setFormattingMarks(JSON.stringify(patch)));
    } catch (error) {
      io.setStatus(String(error?.message ?? error), "error");
      return false;
    }
    for (const mark of FORMATTING_MARKS) preference[mark.key] = next[mark.key];
    savePrefObject(PREF_KEY, preference);
    io.repaint();
    reflect();
    return true;
  }

  /** The ¶ button. OFF when anything is showing, and ON means every mark.
   *
   *  Word's button is exactly this: it does not restore a remembered subset, it
   *  shows all of them. `any` decides the direction, so a reader with only the
   *  space dots on gets "off" from one press rather than having to find which of
   *  five rows is still checked. */
  function toggleAll() {
    apply({ all: !engineState().any });
  }

  /** One row of the popover. A patch of one key leaves the other four alone,
   *  which is the whole reason the setter takes a patch. */
  function toggleOne(key) {
    apply({ [key]: !engineState()[key] });
  }

  /** Puts the preference back onto a document that has just been opened.
   *
   *  Idempotent and O(1): when the new handle already agrees — the common case,
   *  because the default is off and so is a fresh document's state — the setter
   *  bumps no revision and this costs one JSON round trip. The repaint it would
   *  otherwise schedule is skipped, since the open path is about to render
   *  anyway. */
  function adopt() {
    const doc = io.getDoc();
    if (!doc) return;
    const wanted = FORMATTING_MARKS.some((mark) => preference[mark.key]);
    if (!wanted && !engineState().any) {
      reflect();
      return;
    }
    apply(Object.fromEntries(FORMATTING_MARKS.map((mark) => [mark.key, !!preference[mark.key]])));
  }

  /** Reflects the engine's state onto both halves and every popover row.
   *
   *  The ¶ button carries `aria-pressed` from `any`, each row carries
   *  `aria-checked` from its own field, and with no document open both halves are
   *  disabled CARRYING THE REASON rather than greyed in silence. */
  function reflect() {
    const state = engineState();
    const reason = io.getDoc() ? "" : t("command.needsDocument");
    for (const half of [toggle, menuButton]) {
      if (!half) continue;
      half.disabled = !!reason;
      // The AUTHORED title when live, the reason when not — through the host's own
      // `authoredTitle`, which is the one implementation the Layout, References and
      // Table bands already use. Writing a second sentence here is how a control
      // ends up carrying a title that disagrees with its own markup after a locale
      // change, and `ribbon_tooltip.mjs` reads `title` as the control's name.
      half.title = reason || authoredTitle(half, keyboardPlatform(navigator));
    }
    toggle?.setAttribute("aria-pressed", String(!!state.any));
    for (const row of menu?.querySelectorAll("[data-mark]") ?? []) {
      row.setAttribute("aria-checked", String(!!state[row.dataset.mark]));
    }
  }

  if (toggle) onButton(toggle, toggleAll);
  if (menuButton && menu) {
    registerPopover(menuButton, menu, reflect);
    for (const row of menu.querySelectorAll("[data-mark]")) {
      // The popover STAYS OPEN, unlike the Breaks menu: these are five switches a
      // reader flips in a row while watching the page, and Word's Display pane
      // and ONLYOFFICE's own split menu both keep theirs open for that reason. A
      // menu that closed after each tick would cost four extra journeys.
      onButton(row, () => toggleOne(row.dataset.mark));
    }
  }

  return {
    /** The six palette / View-menu rows, generated from the one mark table.
     *
     *  Labels are read when the rows are BUILT, which is after a catalogue is
     *  installed and again after a locale change — the reason `break_commands.mjs`
     *  uses a getter for the same job and the reason neither module carries an
     *  English literal.
     *
     *  Every row requires NOTHING of a host's grant and says so in the contract:
     *  a mark is a view, not an edit. It issues no operation, bumps no document
     *  revision and cannot reach the export path, so a container that granted no
     *  mutation capability at all may still offer it — and a reader proof-reading
     *  someone else's document is exactly who wants it. */
    commands: () => {
      const state = engineState();
      const reason = io.getDoc() ? "" : t("command.needsDocument");
      return [
        {
          id: "view.formattingMarks",
          label: t(state.any ? "formattingMarks.commandOn" : "formattingMarks.commandOff"),
          group: "View",
          kw: "formatting marks pilcrow paragraph mark show hide nonprinting non-printing invisible whitespace tab space dot return arrow",
          enabled: !reason,
          disabledReason: reason,
          run: toggleAll,
        },
        // ONE composed key for all five rows rather than five sentences. A palette
        // row has to read as a switch — "Tab characters: on" — and the mark's own
        // name is already a declared string, so composing them here costs one
        // catalogue entry instead of five, and a translator sees the pattern once.
        ...FORMATTING_MARKS.map((mark) => ({
          id: `view.formattingMarks.${mark.key}`,
          label: t("formattingMarks.markSwitch", {
            mark: t(mark.labelKey),
            state: t(state[mark.key] ? "formattingMarks.stateOn" : "formattingMarks.stateOff"),
          }),
          group: "View",
          kw: `formatting marks nonprinting non-printing invisible ${mark.key}`,
          enabled: !reason,
          disabledReason: reason,
          run: () => toggleOne(mark.key),
        })),
      ];
    },
    reflect,
    adopt,
    state: engineState,
    isOn: () => engineState().any,
  };
}
