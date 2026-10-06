// SPDX-License-Identifier: Apache-2.0
// Key tips: press and release Alt, and every ribbon tab shows a letter; press a
// tab's letter and that tab opens with a letter on each of its controls; press a
// control's letter and it runs. Escape steps back a level.
//
// ---- THE PRIOR ART -----------------------------------------------------------
//
// Word's KeyTips (Office since 2007) and ONLYOFFICE's equivalent are the same
// mechanism, and it is a known one: a MODAL KEY SEQUENCE over a prefix-free code
// table, entered by a lone modifier tap. The parts that matter, all copied:
//
//   * Entry is Alt PRESSED AND RELEASED with nothing in between. Alt as a
//     modifier — Alt+Shift+Arrow resizing a table column here — never enters it,
//     because another key arrives before the release.
//   * Codes are PREFIX-FREE, so a two-letter code (Word's "JT" for Table Design)
//     can never be shadowed by a one-letter one. Typing filters the visible tips
//     to the codes that still match.
//   * Tab codes are FIXED and Word's own where we have the same tab (File F,
//     Home H, Insert N, Layout P, References S, Review R, View W, the contextual
//     Table tab JT), so the muscle memory of anyone arriving from Word works.
//   * Control codes are assigned PER BAND, deterministically: the same band in
//     the same state always gets the same letters. Word hand-assigns its; this
//     ribbon is localised into nineteen languages and changes with the product,
//     so a hand table would be stale on the day it shipped. The assignment is the
//     mnemonic rule Word's own letters mostly follow — a word's initial, then any
//     letter of the name, then the first free letter, then a digit — applied in
//     band order.
//
// ---- WHAT IT REFUSES TO DO ----------------------------------------------------
//
//   * It never fires while a person is TYPING somewhere other than the document:
//     a text field, a contenteditable, a select. Alt there belongs to the field.
//   * It never fires during IME composition, and leaves it the moment one starts.
//   * It never fires on an Apple keyboard. Option is macOS's character-composition
//     key, and Word for Mac has no key tips; an Option tap that took the keyboard
//     away would be a regression for exactly the people who use it most.
//   * It never fires over a modal dialog, in compact chrome (there is no ribbon to
//     label), on a phone, or with a pointer button held (Alt during a drag is a
//     modifier — `object_resize_drag.mjs` reads it to disable snapping).
//   * It takes no chord the application binds. While tips are showing, a key
//     held with Ctrl/Cmd/Alt leaves tip mode and is NOT consumed, so the chord
//     still runs; only bare letters and digits are taken, and only while the tips
//     are on screen.
//
// ---- HOW IT IS WIRED -----------------------------------------------------------
//
// No edit to `main.js`. A capturing `keydown` on `window` runs before every
// handler `main.js` and `modal.mjs` install, so a consumed letter never reaches
// the document's text input. Tabs and controls are activated with `click()` —
// exactly what a pointer does, so every command runs through the handler it
// already has, and `detail === 0` tells the popover manager it was the keyboard
// (a menu opened by a tip takes focus, as it should).
//
// Complexity: O(controls in one band) to show a level — about forty on Home — and
// O(codes) per key while tips are up. Nothing runs per keystroke otherwise
// beyond one key comparison.

import { APPLE_PLATFORM, keyboardPlatform } from "./keyboard.mjs";
import { shortcutForCommand } from "./keymap.mjs";

/** Word's own letters, for the tabs we share with it. */
export const TAB_KEYTIPS = Object.freeze({
  file: "F",
  home: "H",
  insert: "N",
  layout: "P",
  references: "S",
  review: "R",
  view: "W",
  table: "JT",
});

/** Single-key codes, in the order they are handed out once a name runs out of
 *  letters of its own. "0" is absent on purpose: it is the reserved prefix for
 *  the two-key overflow codes, and a prefix that were also a code would make the
 *  table ambiguous. */
const POOL = [..."ABCDEFGHIJKLMNOPQRSTUVWXYZ123456789"];
const OVERFLOW_PREFIX = "0";

/** The Latin letters of `name`, upper-cased, diacritics folded ("Édition" -> E,
 *  D, I, T, I, O, N). A name in a script with no Latin letters yields none, and
 *  the control then takes a letter from the pool — a code a person on any
 *  keyboard can still type. */
export function latinLetters(name) {
  return [...String(name ?? "").normalize("NFD").toUpperCase()].filter((c) => c >= "A" && c <= "Z");
}

function wordInitials(name) {
  return String(name ?? "")
    .split(/[\s\-–—/&,.:;()]+/u)
    .map((word) => latinLetters(word)[0])
    .filter(Boolean);
}

/** The letter or digit a chord ends in — "⌘B" -> B, "⌘⇧E" -> E — or null for
 *  a chord on a named key ("⌘⌥⏎", "⇧Tab"). Apple notation, as `keymap.mjs`
 *  declares every chord. */
export function chordLetter(chord) {
  const key = String(chord ?? "").replace(/[⌘⌃⌥⇧]/gu, "");
  return /^[A-Z0-9]$/i.test(key) ? key.toUpperCase() : null;
}

/**
 * Prefix-free codes for `entries`, in order, avoiding every code in `taken`.
 * An entry is a name, or `{name, chord}` where `chord` is the control's own
 * keyboard shortcut in Apple notation.
 *
 * Deterministic: the same entries in the same order always get the same codes.
 * Five passes, each only over the entries the previous one left without a code,
 * so an earlier control never loses a mnemonic letter to a later one's fallback:
 *
 *   0. the letter of the control's own chord — Bold is B because Ctrl+B is
 *      bold, Paste V, Undo Z; the letter a person already associates with the
 *      control, and stable however the name is translated;
 *   1. the initial of one of the name's words ("Track changes" -> T);
 *   2. any other letter of the name;
 *   3. the first free letter or digit;
 *   4. a two-key code under the reserved prefix — Home carries ~40 controls and
 *      there are 35 single keys.
 *
 * @param {Array<string|{name: string, chord?: string|null}>} entries
 * @param {{taken?: Iterable<string>}} [options]
 * @returns {string[]}
 */
export function assignKeyTips(entries, { taken = [] } = {}) {
  const names = entries.map((entry) => (typeof entry === "string" ? entry : entry?.name ?? ""));
  const chords = entries.map((entry) => (typeof entry === "string" ? null : chordLetter(entry?.chord)));
  const used = new Set(taken);
  const codes = names.map(() => null);
  const claim = (i, code) => {
    codes[i] = code;
    used.add(code);
  };
  // The plainest chord first: Left align is ⌘L and Bullets ⌘⇧L, and the letter
  // belongs to the one a person types without a second modifier.
  const modifiers = entries.map((entry) =>
    typeof entry === "string" ? 0 : [...String(entry?.chord ?? "")].filter((c) => "⌘⌃⌥⇧".includes(c)).length,
  );
  for (const round of [1, 2, 3, 4]) {
    chords.forEach((letter, i) => {
      if (!codes[i] && letter && modifiers[i] === round && !used.has(letter)) claim(i, letter);
    });
  }
  names.forEach((name, i) => {
    if (codes[i]) return;
    const code = wordInitials(name).find((c) => !used.has(c));
    if (code) claim(i, code);
  });
  names.forEach((name, i) => {
    if (codes[i]) return;
    const code = latinLetters(name).find((c) => !used.has(c));
    if (code) claim(i, code);
  });
  names.forEach((_, i) => {
    if (codes[i]) return;
    const code = POOL.find((c) => !used.has(c));
    if (code) claim(i, code);
  });
  // The n-th overflow code is the prefix, repeated once per 35, then a pool key:
  // "0A".."09", then "00A".."009". Prefix-free because the prefix alone is never
  // a code and is not in the pool, so no code can end where another continues.
  const overflow = (n) =>
    `${OVERFLOW_PREFIX.repeat(Math.floor(n / POOL.length) + 1)}${POOL[n % POOL.length]}`;
  let next = 0;
  names.forEach((_, i) => {
    if (codes[i]) return;
    while (used.has(overflow(next))) next += 1;
    claim(i, overflow(next));
    next += 1;
  });
  return codes;
}

/** The tab codes: Word's letter where the tab is one Word has, an assigned one
 *  for any other, never colliding. */
export function tabKeyTips(tabs) {
  const fixed = tabs.map((tab) => TAB_KEYTIPS[tab.key] ?? null);
  const taken = fixed.filter(Boolean);
  const free = assignKeyTips(
    tabs.filter((_, i) => !fixed[i]).map((tab) => tab.name),
    // A code that is a PREFIX of a fixed one ("J" of "JT") is taken as well.
    { taken: [...taken, ...taken.map((code) => code[0])] },
  );
  let at = 0;
  return fixed.map((code) => code ?? free[at++]);
}

/** Which codes are still live after `typed`, and whether one is complete. */
export function matchKeyTips(codes, typed) {
  const live = [];
  let exact = -1;
  codes.forEach((code, i) => {
    if (!code || !code.startsWith(typed)) return;
    live.push(i);
    if (code === typed) exact = i;
  });
  return { live, exact };
}

/** The character a keystroke types for tip purposes: its letter or digit,
 *  upper-cased — or, on a non-Latin layout, the physical key's Latin letter
 *  (`KeyR` -> R), so the printed codes work on every keyboard. */
export function keyTipChar(event) {
  const key = typeof event.key === "string" ? event.key : "";
  if (/^[a-z0-9]$/i.test(key)) return key.toUpperCase();
  const code = typeof event.code === "string" ? event.code : "";
  const fromCode = code.match(/^(?:Key([A-Z])|Digit([0-9])|Numpad([0-9]))$/);
  if (fromCode) return fromCode[1] ?? fromCode[2] ?? fromCode[3];
  return null;
}

/** Whether `el` is somewhere a person types that is NOT the document — where a
 *  bare Alt belongs to the field. The document's own text input is excluded:
 *  that is exactly where Word users press Alt. */
export function isForeignTextEntry(el) {
  if (!el || el === el.ownerDocument?.body) return false;
  if (el.classList?.contains("editor-text-input")) return false;
  if (el.isContentEditable) return true;
  const tag = el.tagName;
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag !== "INPUT") return false;
  const type = (el.getAttribute("type") || "text").toLowerCase();
  return !["button", "checkbox", "radio", "range", "color", "file", "submit", "reset", "image"].includes(type);
}

const CONTROL_SELECTOR = 'button, input:not([type="hidden"]), select, [role="combobox"]';

function rendered(el) {
  if (!el || el.getClientRects().length === 0) return false;
  if (el.closest("[hidden], [aria-hidden='true'], [inert]")) return false;
  const style = getComputedStyle(el);
  return style.visibility !== "hidden" && style.display !== "none";
}

function nameOf(el) {
  const labelledBy = el.getAttribute("aria-labelledby");
  if (labelledBy) {
    const text = labelledBy
      .split(/\s+/)
      .map((id) => el.ownerDocument.getElementById(id)?.textContent?.trim() ?? "")
      .join(" ")
      .trim();
    if (text) return text;
  }
  return (
    el.getAttribute("aria-label") ||
    el.textContent?.trim() ||
    el.getAttribute("title") ||
    el.getAttribute("placeholder") ||
    ""
  );
}

function unavailable(el) {
  return !!el.disabled || el.getAttribute("aria-disabled") === "true";
}

/**
 * Installs key tips over the ribbon. Returns a small controller, or `null` when
 * the page has no ribbon tab strip (an embedding shell that composed it away).
 *
 * @param {{doc?: Document, win?: Window, platform?: string}} [options]
 */
export function installKeyTips({ doc = document, win = window, platform = keyboardPlatform() } = {}) {
  const strip = doc.querySelector(".ribbon-tabs");
  if (!strip) return null;
  const layer = doc.createElement("div");
  layer.className = "key-tips";
  // Pictures of letters. Everything they say is already each control's
  // accessible name; the mechanism itself is a keyboard accelerator, which a
  // screen-reader user reaches through the same tab strip and band roving.
  layer.setAttribute("aria-hidden", "true");
  layer.hidden = true;
  doc.body.append(layer);

  /** "off" | "tabs" | "band" */
  let level = "off";
  let armed = false;
  let composing = false;
  let pointersDown = 0;
  let typed = "";
  /** @type {{el: Element, code: string, badge: HTMLElement}[]} */
  let tips = [];

  function eligible() {
    if (platform === APPLE_PLATFORM || composing || pointersDown > 0) return false;
    const body = doc.body;
    if (!body.classList.contains("ribbon-mode") || body.classList.contains("phone-mode")) return false;
    if (!rendered(strip)) return false;
    // A modal owns the keyboard. Any rendered `aria-modal` surface counts, so a
    // dialog added tomorrow is covered without being named here.
    for (const modal of doc.querySelectorAll('[aria-modal="true"]')) {
      if (modal.getClientRects().length > 0 && !modal.closest("[hidden]")) return false;
    }
    return !isForeignTextEntry(doc.activeElement);
  }

  function clear() {
    layer.replaceChildren();
    tips = [];
    typed = "";
  }

  function exit() {
    level = "off";
    armed = false;
    clear();
    layer.hidden = true;
  }

  function badgeFor(el, code, placement) {
    const rect = el.getBoundingClientRect();
    const badge = doc.createElement("span");
    badge.className = "key-tip";
    if (unavailable(el)) badge.classList.add("is-unavailable");
    badge.textContent = code;
    if (el.id) badge.dataset.target = el.id;
    // Centred under a tab's caption, and on the bottom edge of a control — where
    // Word draws them, and where a badge covers the least of the label it names.
    badge.style.left = `${Math.round(rect.left + rect.width / 2)}px`;
    badge.style.top = `${Math.round(placement === "tab" ? rect.bottom - 6 : rect.bottom - 9)}px`;
    layer.append(badge);
    return badge;
  }

  function show(entries, placement) {
    clear();
    // A control's chord comes from the ONE keymap, through the command id the
    // ribbon already stamps on it (`data-command`) — not from its title, which
    // a disabled control swaps for its reason, so a letter would move every time
    // availability changed.
    const codes = placement === "tab"
      ? tabKeyTips(entries.map((el) => ({ key: el.dataset.tab, name: nameOf(el) })))
      : assignKeyTips(
          entries.map((el) => ({
            name: nameOf(el),
            chord: el.dataset.command ? shortcutForCommand(el.dataset.command, platform) : null,
          })),
        );
    tips = entries.map((el, i) => ({ el, code: codes[i], badge: badgeFor(el, codes[i], placement) }));
    layer.hidden = false;
  }

  function showTabs() {
    level = "tabs";
    show([...strip.querySelectorAll('[role="tab"]')].filter(rendered), "tab");
  }

  function bandControls(tab) {
    const panel = doc.getElementById(tab.getAttribute("aria-controls") ?? "");
    if (!panel || !rendered(panel)) return [];
    const controls = [...panel.querySelectorAll(CONTROL_SELECTOR)].filter(rendered);
    // The band's "⋯" is a control of the band, wherever its markup sits.
    const overflow = doc.getElementById("ribbonOverflowBtn");
    if (panel.classList.contains("ribbon-panel") && overflow && rendered(overflow)) controls.push(overflow);
    return controls;
  }

  function showBand(tab) {
    const controls = bandControls(tab);
    if (controls.length === 0) {
      exit();
      return;
    }
    level = "band";
    show(controls, "control");
  }

  function filter() {
    const { live, exact } = matchKeyTips(tips.map((t) => t.code), typed);
    tips.forEach((tip, i) => {
      tip.badge.hidden = !live.includes(i);
    });
    return { live, exact };
  }

  function activate(tip) {
    const { el } = tip;
    if (level === "tabs") {
      if (unavailable(el)) return;
      el.click();
      // The tab's handler swaps the band synchronously; measuring on the next
      // frame reads the band the person is now looking at.
      win.requestAnimationFrame(() => {
        if (level === "tabs") showBand(el);
      });
      return;
    }
    if (unavailable(el)) return; // Word's answer too: the tip stays, nothing runs.
    exit();
    const field =
      el.tagName === "SELECT" ||
      el.getAttribute("role") === "combobox" ||
      (el.tagName === "INPUT" && isForeignTextEntry(el));
    if (field) {
      el.focus();
      if (typeof el.select === "function" && el.tagName === "INPUT") el.select();
    } else {
      el.click();
    }
  }

  function onKeyDown(event) {
    if (level === "off") {
      // Armed by a lone Alt; disarmed by ANY other key, so Alt as a modifier
      // never enters tip mode.
      armed =
        event.key === "Alt" &&
        !event.repeat &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.shiftKey &&
        eligible();
      return;
    }
    if (event.isComposing || event.key === "Process" || event.keyCode === 229) {
      exit();
      return;
    }
    const consume = () => {
      event.preventDefault();
      event.stopImmediatePropagation();
    };
    if (event.key === "Alt") {
      // A second tap puts them away, as in Word.
      consume();
      exit();
      return;
    }
    if (event.key === "Escape") {
      consume();
      if (level === "band") showTabs();
      else exit();
      return;
    }
    if (event.key === "Shift") return;
    if (event.ctrlKey || event.metaKey || event.altKey) {
      // A chord: leave, and let it through to whatever binds it.
      exit();
      return;
    }
    const char = keyTipChar(event);
    if (!char) {
      // Tab, the arrows, Enter: the person has moved on. Leave, and let the key
      // do what it does.
      exit();
      return;
    }
    consume();
    const candidate = typed + char;
    if (matchKeyTips(tips.map((t) => t.code), candidate).live.length === 0) return;
    typed = candidate;
    const { exact } = filter();
    if (exact < 0) return;
    const tip = tips[exact];
    typed = "";
    filter();
    activate(tip);
  }

  function onKeyUp(event) {
    if (event.key !== "Alt" || !armed || level !== "off") return;
    armed = false;
    if (!eligible()) return;
    // The browser's own Alt-tap (the window menu in Firefox and on Windows) is
    // the thing being replaced; it is suppressed only on the tap that shows tips.
    event.preventDefault();
    showTabs();
  }

  win.addEventListener("keydown", onKeyDown, true);
  win.addEventListener("keyup", onKeyUp, true);
  win.addEventListener(
    "pointerdown",
    () => {
      pointersDown += 1;
      armed = false;
      if (level !== "off") exit();
    },
    true,
  );
  const release = () => {
    pointersDown = Math.max(0, pointersDown - 1);
  };
  win.addEventListener("pointerup", release, true);
  win.addEventListener("pointercancel", release, true);
  win.addEventListener("blur", () => {
    pointersDown = 0;
    exit();
  });
  win.addEventListener("resize", () => level !== "off" && exit());
  // The document scrolling moves every control out from under its badge, so the
  // tips go. The chrome's own scrolling does not count: choosing a tab can scroll
  // the tab strip to reveal it, and that must not take the band's tips away.
  win.addEventListener(
    "scroll",
    (event) => {
      if (level === "off") return;
      const target = event.target;
      if (target && typeof target.closest === "function" && target.closest(".bar, .ribbon")) return;
      exit();
    },
    true,
  );
  doc.addEventListener(
    "compositionstart",
    () => {
      composing = true;
      exit();
    },
    true,
  );
  doc.addEventListener("compositionend", () => (composing = false), true);

  return {
    get level() {
      return level;
    },
    showTabs,
    exit,
  };
}
