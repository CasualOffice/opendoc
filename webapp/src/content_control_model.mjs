// Content controls: the vocabulary and the arithmetic, with no DOM and no engine
// (`docs/109` RM-34, `docs/153` page.contentControls / shell.form-fields).
//
// What lives here is every decision about a content control that can be stated
// without a browser: which seven kinds Word's Developer ▸ Controls group inserts
// and in what order, which command each one is, what filling a control of each
// kind means, and how a date is written in a control's own date format. The DOM
// half — the Insert dropdown, the properties dialog and the fill popover — is
// `content_controls.mjs`; the engine half is `crates/casual-doc-wasm/src/
// content_controls.rs`.

/** Word's Developer ▸ Controls, in Word's order. `kind` is the engine's token,
 *  `command` the editor's, and `placeholderKey` the localised prompt an empty
 *  control is inserted holding — Word's own words ("Click or tap here to enter
 *  text.", "Choose an item.", "Click or tap to enter a date."). */
export const CONTROL_KINDS = Object.freeze([
  Object.freeze({ kind: "richText", command: "controls.insert.richText", labelKey: "controls.kind.richText", icon: "text_snippet", placeholderKey: "controls.placeholder.text", kw: "rich text content control form field formatted" }),
  Object.freeze({ kind: "plainText", command: "controls.insert.plainText", labelKey: "controls.kind.plainText", icon: "short_text", placeholderKey: "controls.placeholder.text", kw: "plain text content control form field input box" }),
  Object.freeze({ kind: "picture", command: "controls.insert.picture", labelKey: "controls.kind.picture", icon: "image", placeholderKey: "", kw: "picture image photo content control form field" }),
  Object.freeze({ kind: "checkbox", command: "controls.insert.checkbox", labelKey: "controls.kind.checkbox", icon: "check_box", placeholderKey: "", kw: "check box checkbox tick content control form field" }),
  Object.freeze({ kind: "comboBox", command: "controls.insert.comboBox", labelKey: "controls.kind.comboBox", icon: "arrow_drop_down_circle", placeholderKey: "controls.placeholder.choose", kw: "combo box combobox list choose type content control form field" }),
  Object.freeze({ kind: "dropDownList", command: "controls.insert.dropDownList", labelKey: "controls.kind.dropDownList", icon: "list", placeholderKey: "controls.placeholder.choose", kw: "drop down dropdown list choose select content control form field" }),
  Object.freeze({ kind: "date", command: "controls.insert.date", labelKey: "controls.kind.date", icon: "event", placeholderKey: "controls.placeholder.date", kw: "date picker calendar day content control form field" }),
]);

/** Every insert command id, for the contract and the guards. */
export const CONTROL_INSERT_COMMANDS = Object.freeze(CONTROL_KINDS.map((row) => row.command));

/** The date formats the properties dialog offers, Word's list for English. */
export const DATE_FORMATS = Object.freeze([
  "M/d/yyyy",
  "dddd, MMMM d, yyyy",
  "MMMM d, yyyy",
  "M/d/yy",
  "yyyy-MM-dd",
  "d-MMM-yy",
  "M.d.yyyy",
  "MMM. d, yy",
  "d MMMM yyyy",
  "MMMM yy",
  "MMM-yy",
  "dd/MM/yyyy",
]);

/** How a control is filled: `list` (choose an entry), `combo` (choose or
 *  type), `date`, `picture`, `checkbox`, `text` (type), or `null` for a
 *  container that is not a field (group, repeating section, gallery). */
export function fillModeOf(control) {
  switch (control?.kind) {
    case "dropDownList":
      return "list";
    case "comboBox":
      return "combo";
    case "date":
      return "date";
    case "picture":
      return "picture";
    case "checkbox":
      return "checkbox";
    case "richText":
    case "plainText":
      return "text";
    default:
      return null;
  }
}

/** Whether a click on the control opens a chooser rather than placing a caret:
 *  a list, a date and a picture are chosen, not typed. A combo box is typed
 *  into as well, so a click places the caret and Alt+Down opens its list. */
export function clickOpensChooser(control) {
  const mode = fillModeOf(control);
  return mode === "list" || mode === "date" || mode === "picture";
}

/**
 * The list entries the dialog edits, normalised the way Word stores them: an
 * entry with no display text shows its value, an empty value is kept (Word's
 * own "Choose an item." entry has one), and an entry with neither is dropped.
 * Order is kept.
 *
 * @param {ReadonlyArray<{display?: string, value?: string}>} rows
 * @returns {Array<{display: string, value: string}>}
 */
export function normaliseItems(rows) {
  const out = [];
  for (const row of rows ?? []) {
    const display = String(row?.display ?? "").trim();
    const value = String(row?.value ?? "").trim();
    if (!display && !value) continue;
    out.push({ display: display || value, value });
  }
  return out;
}

/** Two digits. */
const pad = (n) => String(n).padStart(2, "0");

/**
 * A date written in a Word date picture (`w:dateFormat`), in `locale`.
 *
 * The tokens are Word's: `d`/`dd` the day, `ddd`/`dddd` the weekday's short and
 * full name, `M`/`MM` the month number, `MMM`/`MMMM` its short and full name,
 * `yy`/`yyyy` the year, `h`/`hh`/`H`/`HH`/`mm`/`ss` the time and `am/pm` or `AM/PM`
 * the half of the day. Text in single quotes is literal, as is every character
 * that is not a token. Names come from `Intl`, so a French reader's control says
 * "mars" where an English one says "March".
 *
 * The date is read in UTC: a date picker stores a calendar day
 * (`2026-03-05T00:00:00Z`), and reading it in local time would show the 4th to
 * a reader west of Greenwich.
 *
 * @param {Date} date
 * @param {string} picture
 * @param {string} [locale]
 * @returns {string}
 */
export function formatWordDate(date, picture, locale = "en") {
  const name = (options) => new Intl.DateTimeFormat(locale, { ...options, timeZone: "UTC" }).format(date);
  const day = date.getUTCDate();
  const month = date.getUTCMonth() + 1;
  const year = date.getUTCFullYear();
  const hours = date.getUTCHours();
  const tokens = [
    ["dddd", () => name({ weekday: "long" })],
    ["ddd", () => name({ weekday: "short" })],
    ["dd", () => pad(day)],
    ["d", () => String(day)],
    ["MMMM", () => name({ month: "long" })],
    ["MMM", () => name({ month: "short" })],
    ["MM", () => pad(month)],
    ["M", () => String(month)],
    ["yyyy", () => String(year).padStart(4, "0")],
    ["yy", () => pad(year % 100)],
    ["HH", () => pad(hours)],
    ["H", () => String(hours)],
    ["hh", () => pad(hours % 12 || 12)],
    ["h", () => String(hours % 12 || 12)],
    ["mm", () => pad(date.getUTCMinutes())],
    ["ss", () => pad(date.getUTCSeconds())],
    ["AM/PM", () => (hours < 12 ? "AM" : "PM")],
    ["am/pm", () => (hours < 12 ? "am" : "pm")],
  ];
  let out = "";
  let i = 0;
  const text = String(picture || "M/d/yyyy");
  while (i < text.length) {
    if (text[i] === "'") {
      const close = text.indexOf("'", i + 1);
      const end = close === -1 ? text.length : close;
      out += text.slice(i + 1, end);
      i = end + 1;
      continue;
    }
    const hit = tokens.find(([token]) => text.startsWith(token, i));
    if (hit) {
      out += hit[1]();
      i += hit[0].length;
    } else {
      out += text[i];
      i += 1;
    }
  }
  return out;
}

/** `YYYY-MM-DD` from an `<input type="date">`, as the ISO date-time Word
 *  stores in `w:fullDate`; `null` for anything else. */
export function isoFromDateInput(value) {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(value ?? ""));
  if (!match) return null;
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])));
  if (Number.isNaN(date.getTime()) || date.getUTCDate() !== Number(match[3])) return null;
  return `${match[1]}-${match[2]}-${match[3]}T00:00:00Z`;
}

/** The `<input type="date">` value for a stored `w:fullDate`, or "". */
export function dateInputFromIso(iso) {
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(String(iso ?? ""));
  return match ? `${match[1]}-${match[2]}-${match[3]}` : "";
}

/** The properties-dialog patch for the engine, from the dialog's fields. Only
 *  the fields the control's kind has are sent: a list for a list, a date format
 *  for a date picker. */
export function propertiesPatch(control, fields) {
  const patch = {
    title: String(fields.title ?? ""),
    tag: String(fields.tag ?? ""),
    color: fields.useColor ? String(fields.color ?? "").replace(/^#/, "") : "",
    temporary: !!fields.temporary,
    lockControl: !!fields.lockControl,
    lockContent: !!fields.lockContent,
    role: String(fields.role ?? ""),
  };
  if (fillModeOf(control) === "text" || fillModeOf(control) === "list" || fillModeOf(control) === "combo" || fillModeOf(control) === "date") {
    const placeholder = String(fields.placeholder ?? "").trim();
    if (placeholder) patch.placeholder = placeholder;
  }
  if (control.kind === "dropDownList" || control.kind === "comboBox") patch.items = normaliseItems(fields.items);
  if (control.kind === "date") patch.dateFormat = String(fields.dateFormat ?? "").trim() || "M/d/yyyy";
  return patch;
}
