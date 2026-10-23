// The Version history timeline's rows and grids, moved out of `version_panel.mjs`
// unchanged when the room's shared grid (`docs/175` §11, HF-362) took that file past the
// source-file ceiling (`docs/106` standing rule 1). One renderer serves both grids — the
// room's shared versions and this device's — so a row reads, clicks and opens its menu
// the same way wherever it came from. It reaches the panel only through the callbacks it
// is handed; the panel's comments on the grid pattern (roving tabindex, the ⋮ as a real
// gridcell) still describe what these build.
import { t } from "./i18n.mjs";
import { reviewAuthorColor, reviewAuthorKey } from "./review_labels.mjs";
import { versionRowText } from "./version_policy.mjs";
import { ORIGIN } from "./shared_versions.mjs";

/** Row ids have to be stable within one render and unique in the document, and a
 *  `versionId` is already both. Named `versionOption-…` since the rows were
 *  options; the spelling is kept because it is what the specs and the panel's
 *  `aria` wiring already say, and renaming it buys nothing. */
export const optionId = (versionId) => `versionOption-${versionId}`;

/** A Material Symbols glyph, `aria-hidden` because it is decoration and the
 *  state it stands for is already in the option's accessible name.
 *
 *  A helper rather than three inline lines, for the same reason
 *  `compact_toolbar.mjs` and `review_chrome.mjs` have one: a ligature NAME is not
 *  prose, and writing it straight into `textContent` at the call site makes the
 *  unrouted-string scanner read a font instruction as English. */
function iconSpan(name, className = "") {
  const icon = document.createElement("span");
  icon.className = className ? `ms ${className}` : "ms";
  icon.setAttribute("aria-hidden", "true");
  icon.textContent = name;
  return icon;
}

/**
 * One section's day groups into its grid, or the sentence saying why it is empty.
 * `renderRow(row, heading)` builds each row. O(rows in the section).
 */
export function fillVersionGrid(grid, section, { namedOnly, empty: emptyText, renderRow }) {
  grid.replaceChildren();
  const groups = section.groups;
  if (groups.length === 0) {
    const empty = document.createElement("div");
    empty.className = "outline-empty";
    // A FILTERED timeline that comes back empty is not an empty timeline, and
    // saying "no versions yet" over a document that has nine of them is a lie
    // the reader can see through.
    empty.textContent = namedOnly && section.count > 0
      ? t("versionHistory.emptyNamed")
      : emptyText;
    grid.append(empty);
    // With no rows there is no cell to be the grid's tab stop, so the grid
    // itself takes the keyboard — otherwise opening an empty timeline would
    // drop focus on `<body>`.
    grid.tabIndex = 0;
    return;
  }
  grid.tabIndex = -1;
  for (const group of groups) {
    // `role="rowgroup"`, because `group` is not a permitted child of a grid.
    // The visible heading stays hidden from the accessibility tree and the day
    // stays inside every row's accessible name, which is where it was already
    // — so nothing is lost by `rowgroup` carrying no name of its own.
    const rowGroup = document.createElement("div");
    rowGroup.className = "version-group";
    rowGroup.setAttribute("role", "rowgroup");
    const day = document.createElement("div");
    day.className = "version-group-day";
    day.setAttribute("aria-hidden", "true");
    day.textContent = group.label;
    rowGroup.append(day);
    for (const row of group.rows) {
      rowGroup.append(renderRow(row, section.heading));
    }
    grid.append(rowGroup);
  }
}

/**
 * One row: a `div[role=row]` holding two `gridcell`s — the entry, and the ⋮
 * that carries this version's own actions.
 *
 * The entry cell is the one that carries the accessible name, because it is
 * the cell focus lands on; putting the same sentence on the row as well would
 * make a screen reader read the version twice for one arrow press.
 *
 * O(1) per row, and nothing here reads the document.
 *
 * @param {object} row a device or a shared timeline row.
 * @param {{sectionHeading: string, isHead: boolean, delta: object|null,
 *          open: () => void, menu: () => void, contextMenu: (x: number, y: number) => void}} context
 */
export function renderVersionRow(row, { sectionHeading = "", isHead, delta, open, menu, contextMenu }) {
  const text = versionRowText(row, { isHead, delta });
  // In a room the section is part of what a row IS — the room's, or this device's —
  // so it is in the accessible name, as the day already is: the grid's own label is
  // read on entry, not on every arrow press. Joined as `versionRowText` joins the rest.
  if (sectionHeading) text.label = [text.label, sectionHeading].join(", ");
  const item = document.createElement("div");
  item.className = row.origin === ORIGIN.SHARED ? "version-item version-item-shared" : "version-item";
  item.id = optionId(row.versionId);
  item.setAttribute("role", "row");
  item.setAttribute("aria-selected", "false");
  item.dataset.versionId = row.versionId;

  const entry = document.createElement("div");
  entry.className = "version-item-entry";
  entry.setAttribute("role", "gridcell");
  entry.tabIndex = -1;
  entry.setAttribute("aria-label", text.label);
  entry.title = text.timestamp;

  const head = document.createElement("div");
  head.className = "version-item-head";
  const title = document.createElement("span");
  title.className = "version-item-title";
  title.textContent = text.title;
  head.append(title);
  if (text.named) {
    // The pin is a STATE, and a colour-only or icon-only state is not a state
    // a screen reader can read — `versionRowText` puts "Named" into the
    // option's accessible name, so this glyph is decoration.
    head.append(iconSpan("push_pin", "version-item-pin"));
  }
  if (isHead) {
    const now = document.createElement("span");
    now.className = "version-item-current";
    now.setAttribute("aria-hidden", "true");
    now.textContent = t("versionHistory.current");
    head.append(now);
  }

  // `<time datetime>` beside the locale spelling: `docs/139` §14 requires the
  // exact machine-readable instant to be present, not implied by a friendly
  // label.
  const detail = document.createElement("time");
  detail.className = "version-item-detail";
  if (text.exact) detail.dateTime = text.exact;
  detail.textContent = text.named ? `${text.clock} · ${text.detail}` : text.detail;
  // The author's colour, as Google's list shows it: the same hue the
  // version's changes are painted in when it is opened (ADR-065), so the dot
  // and the marks on the page say the same thing. A version with no recorded
  // name was made in this browser and is painted as "You", so is its dot.
  const dot = document.createElement("span");
  dot.className = "version-item-author-dot";
  dot.setAttribute("aria-hidden", "true");
  const painter = text.author || t("diffCanvas.someone");
  dot.style.setProperty("--diff-author-color", reviewAuthorColor(reviewAuthorKey({ author: painter })));
  detail.prepend(dot);

  entry.append(head, detail);
  // A click is a decision already made, so it does not wait out the settle.
  entry.addEventListener("click", open);

  // The ⋮. Always painted, never revealed by hover: a hover affordance is no
  // affordance at all on a phone, and this list is one of the surfaces the
  // narrow-screen drawer carries.
  const actionCell = document.createElement("div");
  actionCell.className = "version-item-actions";
  actionCell.setAttribute("role", "gridcell");
  const menuBtn = document.createElement("button");
  menuBtn.type = "button";
  menuBtn.className = "version-item-menu";
  menuBtn.tabIndex = -1;
  menuBtn.setAttribute("aria-haspopup", "menu");
  menuBtn.setAttribute("aria-expanded", "false");
  menuBtn.setAttribute("aria-label", t("versionHistory.rowActions", { when: text.timestamp }));
  menuBtn.append(iconSpan("more_vert"));
  menuBtn.addEventListener("click", menu);
  actionCell.append(menuBtn);

  item.append(entry, actionCell);
  // The platform gesture, on the whole row: `docs/139` §14 asks for row menus
  // that work by keyboard, and a right-click is the pointer half of the same
  // affordance. Shift+F10 and the ContextMenu key are handled on the grid.
  item.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    contextMenu(event.clientX, event.clientY);
  });
  return item;
}
