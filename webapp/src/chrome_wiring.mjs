// SPDX-License-Identifier: Apache-2.0
// Chrome behaviour that needs no editor state, booted from its own module.
//
// Each of these binds to markup and to attributes the editor already maintains
// and reads nothing of `main.js`'s — no document, no selection, no review-mode
// variable — so none of them has a reason to live in the 16,000-line module, and
// `main.js` is on a line ratchet. `editor.html` loads this beside `main.js` as a
// second `<script type="module">`; module scripts run after the document is
// parsed, so every element below exists, and the modules they import
// (`popover_manager.mjs`, `keyboard.mjs`) are the SAME instances `main.js` uses,
// so the popover manager's "one popover open at a time" holds across both.
//
//   * `header_mode.mjs`  — the editing-mode selector at the top right (UX-025),
//                          a second face of the status bar's segmented control.
//   * `status_line.mjs`  — the status line's full text on hover when it is cut.
//   * `key_tips.mjs`     — Alt key tips over the ribbon.

import { installHeaderModeMenu } from "./header_mode.mjs";
import { installKeyTips } from "./key_tips.mjs";
import { registerPopover } from "./popover_manager.mjs";
import { installStatusLineTitle } from "./status_line.mjs";

/** Wires all three into `doc`. Each is inert when its markup is absent, so a
 *  shell that composed a surface away gets nothing rather than a throw. */
export function wireChrome(doc) {
  return {
    headerMode: installHeaderModeMenu({
      button: doc.getElementById("headerModeBtn"),
      text: doc.getElementById("headerModeText"),
      icon: doc.getElementById("headerModeIcon"),
      menu: doc.getElementById("headerModeMenu"),
      segments: doc.getElementById("reviewModeControl"),
      registerPopover,
    }),
    statusLine: installStatusLineTitle(doc.getElementById("status")),
    keyTips: installKeyTips({ doc, win: doc.defaultView }),
  };
}

// The page's entry: `editor.html` loads this module for exactly this call.
if (typeof document !== "undefined") wireChrome(document);
