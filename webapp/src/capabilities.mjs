// What this editor is allowed to be, decided before the first paint.
//
// A framed editor is currently a STANDALONE editor: File ▸ New and File ▸ Open
// are live inside someone else's page, so a visitor can replace the host's
// document from within the host's own chrome. opencalc measured and fixed
// exactly this defect (`docs/104` §HF-109), and `docs/125` §2 F3 records it
// here — along with the two places that already know about framing and simply
// never generalised: autosave defaults off when framed, and one command already
// says "Autosave is off in an embedded editor".
//
// `docs/126` phase 1 makes this file the SINGLE authority for both of those
// questions rather than one of two places that know about framing. So the
// `autosave` capability now lives here beside the other seven, and the
// `?autosave=0` / `?autosave=1` contract (`docs/112` O-2) is resolved here too.
// There is deliberately no second mechanism: a caller that wants to know
// whether this page may keep drafts asks the capability set.
//
// It also carries the FIVE ROLES the owner asked for — `preview`, `readonly`,
// `commentor`, `edit`, `owner` — as presets over the same eight capabilities,
// not as a parallel concept. A role is a name for a subset; it is not a new
// kind of thing, and nothing downstream should ever branch on a role name.
//
// Pure: no DOM, no engine. The host inputs arrive as arguments so the whole
// resolution is answerable in node.

/** Everything a host can grant or withhold. Deliberately small — this is the
 *  v1 set `docs/125` §5.2 names, not a wish list. */
export const CAPABILITIES = Object.freeze([
  "open", // replace the loaded document from inside the editor
  "new", // start a blank document
  "save", // write the document back out
  "download", // export to a file the visitor keeps
  "print",
  "edit", // mutate the document at all
  "comment",
  "branding", // show OUR name and mark
  // Keep drafts on this origin. Was `autosaveAllowedHere()` in `main.js`, which
  // is the second place that knew about framing (`docs/126` phase 1). A framed
  // editor must not leave the visitor's typing in the host origin's store for a
  // later real session to be offered, so every framed preset withholds it.
  "autosave",
]);

/** The five roles, in strictly increasing order of what they grant.
 *
 *  The ordering is not decoration: `roles.test.mjs` asserts the chain is
 *  monotone, which is what stops the classic permissions defect where a
 *  "higher" role quietly loses something a lower one had. Add a role only where
 *  it belongs in the chain, or the guard will say so. */
export const ROLES = Object.freeze(["preview", "readonly", "commentor", "edit", "owner"]);

/** Role → capability set, and the three legacy preset names that predate the
 *  roles. The legacy names are kept because they are a published URL contract
 *  (`?mode=embedded`), and two of them are exact aliases; `embedded` is not,
 *  and the difference is the point — see its comment. */
const PRESETS = Object.freeze({
  // ── The five roles ────────────────────────────────────────────────────────
  //
  // Look, and nothing else. `docs/83` §2 describes read-only/preview as the
  // runtime acting "strictly as a layout and rendering engine"; scrolling,
  // zooming, selecting, copying and searching are not in the capability set at
  // all because nobody gates them, so a preview grants none of the eight and
  // is still a usable reader.
  preview: Object.freeze([]),
  // A published document: read it, and take a paper copy. `download` is
  // withheld deliberately — a reader who can download holds the file, which is
  // a different grant from being allowed to look at it, and a host that wants
  // to hand the file over says `commentor` or wires the file itself.
  readonly: Object.freeze(["print"]),
  // Google Docs' "Commenter" and Word's review reviewer: may annotate and may
  // suggest, may not change the document outright. That maps onto the editor's
  // EXISTING Suggesting mode (see `editingModeFor`), where every body change is
  // recorded as a tracked revision the owner accepts or rejects.
  commentor: Object.freeze(["print", "download", "comment"]),
  // The reason a host embeds an editor. It may change and keep the document; it
  // may not reach for a DIFFERENT one, and it does not advertise us inside
  // their product.
  edit: Object.freeze(["save", "download", "print", "edit", "comment", "autosave"]),
  // The page is ours, or the host has said it may as well be.
  owner: Object.freeze([...CAPABILITIES]),

  // ── The legacy preset names ───────────────────────────────────────────────
  //
  // `standalone` is `owner`: a page opened directly, with no host and no
  // question to answer.
  standalone: Object.freeze([...CAPABILITIES]),
  // `embedded` is the `edit` role MINUS `autosave`, and that is exactly what it
  // has always meant: it is the default for a page that turned out to be framed
  // without anyone asking for a mode, and a framed page must not leave drafts
  // in the host's origin. A host that has thought about it and wants drafts
  // says `?mode=edit`, or `?autosave=1`.
  embedded: Object.freeze(["save", "download", "print", "edit", "comment"]),
  // `viewer` is `readonly`.
  viewer: Object.freeze(["print"]),
});

/** The legacy name → role it is an alias of, or `null` where it is not an exact
 *  alias of any role. Exported so a guard can prove the aliases really are
 *  equal sets rather than two tables that drifted. */
export const LEGACY_PRESETS = Object.freeze({
  standalone: "owner",
  viewer: "readonly",
  embedded: null, // `edit` minus `autosave` — see the comment above
});

/** The preset names a host may ask for, for validation and for tests. */
export const PRESET_NAMES = Object.freeze(Object.keys(PRESETS));

// ---- Surface composition ---------------------------------------------------
//
// `docs/126`'s container policy, owner notes §2, draws a line this file did not
// have: **surface composition is a different question from command gating.**
//
//   * Within a surface a role DOES get, a command that cannot run right now is
//     disabled and says why. That is "never a dead control" (`SKILL` §10), it is
//     what `reflectReviewModeAccess` below already does, and it is correct.
//   * A role with no business with a WHOLE SURFACE does not get the surface. A
//     `readonly` container has no editing ribbon — not a ribbon full of greyed
//     buttons, no ribbon. Word, Google Docs and ONLYOFFICE all present read-only
//     as different chrome, not the editing chrome dimmed.
//
// The distinction is "never, for you" versus "not right now", and a wall of
// greyed controls is not honesty: it tells a reader about capabilities they will
// never have and buries the one or two things they can do.
//
// SO THE TWO AXES ARE SEPARATE, AND BOTH RESOLVE HERE. A region is not a
// capability and must never be made one: a withheld region is a presentation
// decision the host made about their page, and a withheld capability is a
// permission. Collapsing them would make every hidden band read as a refusal —
// and, worse, would let a host defeat a permission by showing a surface.
//
// NAMING. Not "surface": in this codebase `surface` already means which command
// menu a row appears on (`editorCommands({ surface: "palette" | "menu" | … })`),
// and overloading it would make "is this capability reachable from two surfaces"
// (`105` UX-004) ambiguous. `docs/126` says surfaces; the code says REGIONS.

/** Every region of the chrome a host can withhold.
 *
 *  Each one is a whole presentation, not a control: the unit is "a reader has no
 *  business with this at all". Deliberately not one entry per button — that is
 *  what the capability set and the command registry are for.
 *
 *  The eight `band.*` entries are the ribbon's tabs, which is what this project
 *  calls a band ("the Home band must fit 1280px"). A band is the right grain for
 *  a host: "show Home and Insert only" is a real request, where "hide the third
 *  group inside Home" is a request to redesign the ribbon. */
export const REGIONS = Object.freeze([
  "brand", // the product mark in the top bar
  "title", // the document name, and renaming it
  "menu", // the application menu bar — one of the two navigation axes
  "ribbon", // the whole tabbed ribbon, strip and bands together
  "band.file",
  "band.home",
  "band.insert",
  "band.layout",
  "band.references",
  "band.review",
  "band.view",
  "band.table",
  "rail", // the left navigation rail and its panels: outline, pages
  // The version-history timeline: the panel, its View-band button, its preview
  // bar, and the command that opens them (`docs/139`, `docs/140`).
  //
  // A REGION and not merely a capability, decided rather than assumed. Version
  // history is already gated on the `autosave` capability — one switch must not
  // promise what the other has stopped doing (ADR-038) — so the permission axis
  // was covered. But a container's entry points are only half of reachability:
  // the command palette and the ⌘⌥⇧H chord belong to no region, so a host that
  // withheld `band.file` and `band.view` would still have a visitor one chord
  // away from a timeline of a document's past. That is a whole presentation a
  // host can legitimately have no business with — the test this file sets for a
  // region — so it is one, and withholding it takes the command out of the
  // registry rather than hiding a button.
  //
  // Not in `READING_REGIONS`: no preset below `edit` grants `autosave`, so a
  // reader's timeline could only ever be empty and disabled. A presentation that
  // can never say anything is a dead control, and leaving it out is the honest
  // answer rather than an oversight.
  "history",
  "status", // the status bar
  "zoom", // the zoom cluster in the status bar
  "find", // the find/replace card
  "selection", // the floating selection toolbar
  "settings", // the settings dialog and its trigger
]);

/** Regions that are only meaningful inside the ribbon, so withholding `ribbon`
 *  withholds them too. Derived, not listed twice. */
const BAND_REGIONS = Object.freeze(REGIONS.filter((id) => id.startsWith("band.")));

/** THE EDITING CHROME: the regions whose whole purpose is changing the document.
 *
 *  They are named as a set because a container sometimes has a document on screen
 *  that **cannot be edited at all** — one the engine refuses (`docs/113` §8.7), or
 *  a stored version being previewed (`docs/139` §8.4) — and that is the same
 *  "never, for you" the container policy above describes, arriving from the
 *  document rather than from the host. The honest chrome for it is no editing
 *  ribbon, not a ribbon of greyed bands.
 *
 *  The ribbon and its eight bands, because the ribbon IS the editing surface; and
 *  the floating selection toolbar, because it offers formatting and nothing else
 *  — which is exactly why `READING_REGIONS` leaves it out too.
 *
 *  NOT the menu bar: withholding the ribbon reveals it (`style.css`), so a reader
 *  keeps one navigation axis and can still reach File ▸ Print. NOT `settings`
 *  (appearance and identity are not document edits, and a reader who cannot
 *  change the theme for the duration of a preview has lost something for no
 *  reason), NOT `brand` (it is still our page), NOT `history` (the timeline is how
 *  a preview is left again — withholding it would strand the reader in the
 *  preview, since `#versionPanel` and `#versionPreviewBanner` are both in it).
 *
 *  A subtraction and not a second whitelist: one list of what editing chrome IS
 *  cannot drift from `READING_REGIONS`, where two overlapping whitelists would. */
export const EDITING_REGIONS = Object.freeze(["ribbon", ...BAND_REGIONS, "selection"]);

/** Every region — the presentation a page with no host gets. */
const ALL_REGIONS = Object.freeze([...REGIONS]);

/** READING CHROME, for `readonly`.
 *
 *  `docs/126` spells out what it is: "navigation, outline, find, zoom, page
 *  controls, print" — a reading experience, where the reader navigates and
 *  searches and a static image could not replace it. So: the menu bar rather
 *  than the ribbon, because that is where File ▸ Print lives and because a
 *  reader needs exactly one navigation axis, never two (`109` UX-014, `docs/122`,
 *  and the invariant `style.css` states above its compact/ribbon rules). The
 *  rail carries the outline and the page thumbnails; the status bar carries the
 *  page count and the zoom; the find card is how a reader searches.
 *
 *  Not here: the ribbon and every band, the floating selection toolbar (it offers
 *  formatting), and Settings (appearance, reviewer identity, autosave and
 *  proofing are an author's preferences).
 *
 *  And no `brand`: a reader is not somewhere we advertise, and no preset below
 *  `owner` grants `branding` anyway, so listing it would be a dead entry. */
const READING_REGIONS = Object.freeze(["title", "menu", "rail", "status", "zoom", "find"]);

/** PREVIEW CHROME.
 *
 *  Empty, and that is the point. `docs/126` is explicit that `preview` and
 *  `readonly` are NOT the same thing and must not be collapsed: `preview` is the
 *  runtime as a layout and rendering engine — a picture of the document, for a
 *  thumbnail, an attachment preview, a search result — and its own test of the
 *  difference is "could a static image replace it? For `preview`, nearly."
 *  Giving it reading chrome would hand every attachment preview a navigation UI
 *  it does not want; giving `readonly` a bare canvas would leave a published
 *  document with no way to reach page 40. */
const PREVIEW_REGIONS = Object.freeze([]);

/** Preset → regions. Same table shape as the capability presets, and the same
 *  rule: nothing downstream branches on a role NAME, it asks for the set. */
const REGION_PRESETS = Object.freeze({
  preview: PREVIEW_REGIONS,
  readonly: READING_REGIONS,
  // A commentor suggests, and a suggestion is a document change recorded as a
  // tracked revision — so they need the formatting chrome an author needs. The
  // capability set is what stops them committing it outright.
  commentor: ALL_REGIONS,
  edit: ALL_REGIONS,
  owner: ALL_REGIONS,
  standalone: ALL_REGIONS,
  embedded: ALL_REGIONS,
  viewer: READING_REGIONS,
});

/** Where a granted capability has a VISIBLE affordance, by region.
 *
 *  THE JOINT BETWEEN THE TWO AXES, and the thing that makes region composition
 *  safe rather than merely possible. Composition removes surfaces; a permission
 *  grants an action. Nothing mechanically stops a host from being handed a
 *  capability and no way to use it — which is the recurring defect `105` UX-004
 *  records one level down ("every capability must be reachable from ≥2 surfaces")
 *  arriving at the container level. `roles.test.mjs` walks every preset and
 *  refuses a preset that grants something its own chrome cannot reach.
 *
 *  It is also what settled the shape of reading chrome. `readonly` grants `print`
 *  and nothing else; hiding the ribbon without revealing the MENU BAR would have
 *  left that grant unreachable, and this table is what said so before any of it
 *  was written.
 *
 *  An empty list means the capability needs no chrome at all, and each one says
 *  why — an unexplained empty list would be a hole in the guard rather than a
 *  fact about the product. */
export const CAPABILITY_AFFORDANCES = Object.freeze({
  // File-page or menu-bar commands, both of which exist in either navigation
  // axis: File is the ribbon's first band and the menu bar's first menu.
  open: Object.freeze(["menu", "band.file"]),
  new: Object.freeze(["menu", "band.file"]),
  save: Object.freeze(["menu", "band.file"]),
  download: Object.freeze(["menu", "band.file"]),
  print: Object.freeze(["menu", "band.file"]),
  // Typing IS the affordance. A container with no chrome at all can still be
  // edited, which is why `preview` withholds `edit` rather than relying on having
  // hidden the ribbon — a permission enforced by hiding a button is not enforced
  // (`docs/125` §3.2, and phase 1's devtools attack).
  edit: Object.freeze([]),
  // The ribbon's Review band, the floating selection toolbar, and the right-click
  // menu — which belongs to no region, because a context menu is raised on the
  // document rather than painted in the chrome.
  comment: Object.freeze([]),
  // Autosave is a preference, and Settings is where a preference lives.
  autosave: Object.freeze(["settings"]),
  // `branding` IS the brand region: showing our name and mark is the whole of it.
  branding: Object.freeze(["brand"]),
});

/**
 * Parses a withhold list: `"-print,-download"` or `"print,download"`.
 *
 * ONE DIRECTION, ALWAYS. A list can only take things away, whether or not the
 * entries carry their minus sign, because a configuration channel that can widen
 * a role is a configuration channel an attacker fills in. `docs/125` §5.2 states
 * the rule for capabilities — "`can` may only narrow the role. A host cannot
 * grant `edit` to `preview` by accident, and there is exactly one direction to
 * audit" — and it holds for regions for the same reason plus one more: a
 * `preview` with a ribbon is not a presentation anybody asked for.
 *
 * An unknown entry is DROPPED rather than refused, and dropping narrows nothing,
 * so a typo in a host's URL can never widen the result. The same reasoning as an
 * unrecognised `mode` falling back to the framed default.
 *
 * Complexity: O(entries).
 *
 * @param {string|null|undefined} raw
 * @param {readonly string[]} known the vocabulary; anything else is dropped.
 * @returns {readonly string[]}
 */
export function parseWithheld(raw, known) {
  const out = [];
  for (const entry of String(raw ?? "").split(",")) {
    const name = entry.trim().replace(/^-/, "");
    if (!name) continue;
    if (!known.includes(name)) continue;
    if (!out.includes(name)) out.push(name);
  }
  return Object.freeze(out);
}

/**
 * Resolves the capability set for this page load.
 *
 * `mode` is the host's explicit request (`?mode=embedded`, `?mode=readonly`) —
 * a role name or a legacy preset name. When absent, being FRAMED decides it: a
 * page that is not the top window was put there by someone else, and the safe
 * reading of that is `embedded`, not `standalone`. That default is the whole
 * point — it closes the defect without a host having to know this parameter
 * exists.
 *
 * An unrecognised `mode` falls back to the framed-derived default rather than
 * throwing: a typo in a host's URL must not leave the editor with no chrome at
 * all, and it must not silently grant MORE than the default either.
 *
 * `autosave` is the one per-capability override in v1, because it is an already
 * published contract (`docs/112` O-2) and absorbing it here is what makes this
 * file the single authority instead of the second place that knows about
 * framing. It can only move `autosave`, never anything else, so a host cannot
 * widen a role through it.
 *
 * Complexity: O(1).
 *
 * @param {{mode?: string|null, framed?: boolean, autosave?: boolean|null}} [input]
 * @returns {Set<string>}
 */
export function resolveCapabilities({ mode = null, framed = false, autosave = null, withhold = null } = {}) {
  const asked = typeof mode === "string" ? mode.trim().toLowerCase() : "";
  const preset = PRESETS[asked] ?? PRESETS[framed ? "embedded" : "standalone"];
  const granted = new Set(preset);
  if (autosave === true) granted.add("autosave");
  else if (autosave === false) granted.delete("autosave");
  // THE PER-CAPABILITY POLICY (`docs/126` container policy §1). "Print,
  // download, save, edit and comment withholdable independently" — "comments
  // only, everything else off" is a real configuration, not a rung on a ladder.
  //
  // It is applied HERE, after the preset, which is what makes the roles presets
  // over the capability set rather than a parallel mechanism: a role is the
  // starting set, a withhold list narrows it, and there is one function that
  // answers "what may this page do". Two code paths — one for roles, one for
  // explicit lists — would disagree exactly the way the `shortcut:` labels and
  // the key bindings did.
  for (const name of parseWithheld(withhold, CAPABILITIES)) granted.delete(name);
  return granted;
}

/**
 * The chrome regions this page load gets.
 *
 * The same shape as `resolveCapabilities`, from the same `mode`, in the same
 * file, because "one authority" has to mean one place that knows what a role is.
 * A role names two sets, and a caller asks for whichever it needs.
 *
 * Complexity: O(1) plus O(withheld).
 *
 * @param {{mode?: string|null, framed?: boolean, withhold?: string|null}} [input]
 * @returns {Set<string>}
 */
export function resolveRegions({ mode = null, framed = false, withhold = null, capabilities = null } = {}) {
  const asked = typeof mode === "string" ? mode.trim().toLowerCase() : "";
  const preset = REGION_PRESETS[asked] ?? REGION_PRESETS[framed ? "embedded" : "standalone"];
  const shown = new Set(preset);
  // The resolved grant, so a `?can=-branding` reaches this too. Resolved here only
  // when a caller did not already have it — `hostRegions` does, and resolving the
  // same inputs twice is how the two answers start to differ.
  const granted = capabilities ?? resolveCapabilities({ mode, framed });
  // `branding` — "show OUR name and mark" — was declared in phase 1 and consulted
  // NOWHERE, with a test asserting so and the embedding page saying in prose that
  // withholding it does nothing. A capability that does nothing is the API-level
  // form of a dead control.
  //
  // It is wired here, to the `brand` region, because that is what it always meant:
  // the `edit` preset's own comment says an embedded editor "does not advertise us
  // inside their product", and the only presets that grant `branding` are `owner`
  // and `standalone` — a page that is ours. So a host embedding the editor in their
  // product gets no mark of ours by default, and does not have to discover a
  // parameter to stop advertising us.
  //
  // THIS IS NOT A SECOND ENFORCEMENT, and the distinction matters after all the
  // above: the capability set is an INPUT to composition, the same way
  // `editingModeFor` maps it onto a review mode. It decides what the chrome IS, once,
  // at boot. It does not gate a command, and no command consults it.
  if (!grants(granted, "branding")) shown.delete("brand");
  for (const id of parseWithheld(withhold, REGIONS)) shown.delete(id);
  // The ribbon's bands are inside the ribbon, so withholding the ribbon
  // withholds them. Derived rather than asked of the host twice: a host who
  // said `-ribbon` and still saw a band would have found a hole, and a host who
  // had to say `-ribbon,-band.home,-band.insert,…` would be maintaining our
  // containment rules for us.
  if (!shown.has("ribbon")) for (const id of BAND_REGIONS) shown.delete(id);
  return shown;
}

/**
 * Which of the editor's three review modes a capability set means.
 *
 * This is the whole of how a role is enforced, and it deliberately invents
 * nothing: Editing / Suggesting / Viewing already exist, and Viewing is already
 * fully read-only at one fail-closed choke point — `blockMutationInViewing()`
 * refuses every mutation path (typing, deletion, paste, toolbar formatting,
 * table ops, comment and revision decisions) rather than relying on any
 * individual control being disabled (`docs/68` §"Suggesting mode",
 * REVIEW-GAP-014). So a role does not need a gate of its own; it needs to pick
 * the right existing mode, and a chrome defeated from devtools still meets that
 * choke point.
 *
 *   `edit`    → Editing     — change the document outright.
 *   `comment` → Suggesting  — annotate, and change it as tracked revisions.
 *   neither   → Viewing     — no Operation reaches apply.
 *
 * Complexity: O(1).
 *
 * @param {{has: (name: string) => boolean}} capabilities
 * @returns {"editing"|"suggesting"|"viewing"}
 */
export function editingModeFor(capabilities) {
  if (grants(capabilities, "edit")) return "editing";
  if (grants(capabilities, "comment")) return "suggesting";
  return "viewing";
}

/**
 * Whether a capability set allows a review MODE to be chosen.
 *
 * Viewing is always allowed — a reader may always read. Suggesting needs
 * `comment`; Editing needs `edit`. The inverse of [`editingModeFor`]: that one
 * picks the strongest mode a host permits, this one answers whether a
 * particular mode is among them, which is what a mode CONTROL has to know to
 * disable itself with a reason rather than vanish.
 *
 * Complexity: O(1).
 *
 * @param {{has: (name: string) => boolean}} capabilities
 * @param {string} mode
 * @returns {boolean}
 */
export function allowsMode(capabilities, mode) {
  if (mode === "suggesting") return grants(capabilities, "comment");
  if (mode === "editing") return grants(capabilities, "edit");
  return true;
}

/**
 * Reflects a host's grant onto the review-mode buttons: a mode the host
 * withheld is DISABLED WITH A REASON, never removed.
 *
 * Here rather than in `main.js` because it is the same question as
 * `allowsMode` asked of the DOM, and because putting it here makes "a withheld
 * mode still appears, and says why" answerable without a browser. A control
 * that silently vanishes is worse than one that explains itself, and a host
 * reading a missing button cannot tell a permission from a bug.
 *
 * `readOnlyReason` (the engine refusing the whole document) outranks a
 * host-withheld mode: it is the more specific truth and names the document.
 *
 * Complexity: O(buttons).
 */
export function reflectReviewModeAccess({ buttons, capabilities, readOnlyReason, withheldReason }) {
  for (const button of buttons ?? []) {
    const withheld = !allowsMode(capabilities, button.dataset?.reviewMode);
    button.disabled = !!readOnlyReason || withheld;
    if (readOnlyReason) button.title = readOnlyReason;
    else if (withheld) button.title = withheldReason;
    else button.removeAttribute("title");
  }
}

/** Whether a capability set grants `name`, for any value at all.
 *
 *  Fail closed on a shape that is not a set. A caller that has not resolved
 *  anything yet — or has been handed a plain object by a host — must get "no",
 *  not a `TypeError` that the caller's own `catch` then turns into whatever its
 *  fallback happens to be. An exception on the permission path is a permission
 *  decided by accident. */
function grants(capabilities, name) {
  try {
    return capabilities?.has?.(name) === true;
  } catch {
    return false;
  }
}

/** The iframe `sandbox` tokens an embedding host should give this capability
 *  set.
 *
 *  A third enforcement layer, and the only one the page cannot argue with: the
 *  BROWSER refuses a download the sandbox did not allow, whatever the chrome or
 *  the engine think. ONLYOFFICE's `createIframe` sets no `sandbox` attribute at
 *  all (`reference/web-apps/apps/api/documents/api.js:1278`), so this is a
 *  layer they do not have.
 *
 *  `allow-scripts` and `allow-same-origin` are unconditional: without them the
 *  editor cannot run its own WebAssembly or read its own origin, and an embed
 *  that cannot boot is not a permission decision. Together they do mean the
 *  frame can reach out of the sandbox, which is why the sandbox is a LAYER and
 *  not the enforcement.
 *
 *  Complexity: O(1).
 *
 *  @param {{has: (name: string) => boolean}} capabilities
 *  @returns {readonly string[]}
 */
export function sandboxTokensFor(capabilities) {
  const tokens = ["allow-scripts", "allow-same-origin"];
  // Saving and exporting both end in a file the visitor keeps.
  if (grants(capabilities, "download") || grants(capabilities, "save")) tokens.push("allow-downloads");
  // The print dialog is a modal the frame opens.
  if (grants(capabilities, "print")) tokens.push("allow-modals");
  return Object.freeze(tokens);
}

/** The host inputs carried on the page's URL, read once.
 *
 *  `mode` and `autosave` were already a published contract; `can` and `chrome`
 *  join them, spelled the same way and read in the same place, because `?lang=`
 *  already set the precedent that a host's configuration travels in the URL and
 *  because that is the only channel that is decided BEFORE the frame's first
 *  navigation (`embed_element.mjs`). A second channel — a `postMessage` after
 *  load — would mean a window in which the editor was something else, and
 *  `docs/104` is explicit that gating must apply before the first frame. */
export function hostConfig(view = globalThis) {
  const search = view?.location?.search ?? "";
  let params = null;
  try {
    params = new URLSearchParams(search);
  } catch {
    params = null;
  }
  const asked = params?.get("autosave") ?? null;
  // `window.self !== window.top` throws on a cross-origin parent in some
  // engines; a throw means we ARE framed, which is the safer answer anyway.
  let framed = false;
  try {
    framed = view.self !== view.top;
  } catch {
    framed = true;
  }
  return {
    mode: params?.get("mode") ?? null,
    autosave: asked === "1" ? true : asked === "0" ? false : null,
    withhold: params?.get("can") ?? null,
    chrome: params?.get("chrome") ?? null,
    framed,
  };
}

/** Reads the host inputs off a real page. Separated from `resolveCapabilities`
 *  so the decision itself stays pure and testable. */
export function hostCapabilities(view = globalThis) {
  return resolveCapabilities(hostConfig(view));
}

/** The regions a real page shows, from the same inputs. The `editing` half of
 *  [`hostChrome`], and delegated rather than computed a second time: two functions
 *  reading the same URL are two answers waiting to disagree. */
export function hostRegions(view = globalThis) {
  return hostChrome(view).editing;
}

/**
 * The TWO region sets a real page needs, from the one set of host inputs.
 *
 * `editing` is what `hostRegions` has always returned: the chrome this container
 * paints. `reading` is the same container with its editing chrome composed away,
 * for as long as the document on screen cannot be edited at all — a document the
 * engine refuses, and a version preview.
 *
 * BOTH COME FROM `resolveRegions`, and `reading` is produced by appending
 * `EDITING_REGIONS` to the host's own `?chrome=` withhold list rather than by
 * filtering the result. That is not a stylistic choice: `parseWithheld` can only
 * ever take regions away, so `reading ⊆ editing` holds structurally instead of by
 * inspection, and a `preview` or `readonly` container — which already has no
 * ribbon — is unchanged by it. There is no second mechanism and nothing is hidden
 * by hand; a mode cannot widen what a host composed.
 *
 * Complexity: O(regions) twice, once at boot.
 *
 * @param {object} [view]
 * @returns {{editing: Set<string>, reading: Set<string>}}
 */
export function hostChrome(view = globalThis) {
  const config = hostConfig(view);
  const shared = {
    mode: config.mode,
    framed: config.framed,
    // The same grant `hostCapabilities` resolves, handed over rather than resolved
    // again: `branding` is one of the capabilities a `?can=` list can withhold, and
    // the `brand` region follows it.
    capabilities: resolveCapabilities(config),
  };
  const withheld = [config.chrome ?? "", ...EDITING_REGIONS.map((id) => `-${id}`)].join(",");
  return {
    editing: resolveRegions({ ...shared, withhold: config.chrome }),
    reading: resolveRegions({ ...shared, withhold: withheld }),
  };
}
