// SPDX-License-Identifier: Apache-2.0
// THE HOST CONTRACT: one schema, from which both transports are built.
//
// `docs/126` phase 2. A host gets a typed way to command the editor and to be
// told what happened, and it must be IDENTICAL whether they hold a reference to
// the editor or are talking to it across an iframe boundary. Two
// hand-maintained surfaces drift, and this repository has the receipts: the
// `shortcut:` labels and the key bindings were two tables that disagreed until
// `109` UX-006 made them one, and `one-axis-navigation.spec.mjs`'s
// `VALUE_FAMILIES` is *currently* a second copy of the `data-command-family`
// declarations in `ribbon_faces.mjs`.
//
// So this file is the only place that knows:
//
//   * what a command REQUIRES of the host's grant (`COMMAND_CONTRACT`,
//     `COMMAND_FAMILIES`);
//   * which events exist and what each one carries (`HOST_EVENTS`);
//   * what a refusal can say (`REFUSAL_CODES`);
//   * the shape of a `postMessage` envelope (`PROTOCOL`);
//   * which origins may speak to the editor (`originAllowed`).
//
// `host_session.mjs` (the in-process transport) gates and dispatches from it.
// `host_bridge.mjs` (the editor side of `postMessage`) and `host_client.mjs`
// (the host side) build their whole message surface from it. Neither transport
// carries a command list, an event list or a refusal code of its own — which is
// what makes "adding a command to one without the other" a build failure rather
// than a half-reachable API.
//
// WHY A COMMAND ID IS THE UNIT. The editor already has a command registry
// reached by id: `runCommandById` and `keymap.mjs` landed with `109` UX-006, and
// #626 gave 111 ribbon controls a `data-command`. A command id is therefore
// already the unit of reachability in this product, and inventing a second
// addressing scheme for hosts would be the drift above in a new costume.
//
// THE ADDRESSING RULE, and why families exist. Most command ids are literals.
// Some name a VALUE of a control — `format.family.Georgia`,
// `paragraph.listFormat.decimal`, `style.Heading 1`, `view.zoom.150` — and those
// sets are generated from the document, from the font inventory or from markup,
// so they are not enumerable in a table at all. Measured in a browser on the
// `rich` fixture, the registry offered 214 ids, of which 93 were values of ten
// such families, and the font inventory itself differed between two reads of the
// same document. A table of 214 literals would therefore be wrong on arrival.
// So a command resolves EXACTLY first and by FAMILY PREFIX second — the same two
// kinds `ribbon_faces.mjs` already distinguishes as `data-command` and
// `data-command-family`, rather than a third vocabulary.
//
// Pure: no DOM, no engine, no `t()`. Every user-facing sentence a refusal
// carries is an INPUT, because a module that takes its labels as arguments adds
// no string debt (`docs/124`), and because the reasons a command is unavailable
// are the ones the chrome already localised.
import { editingModeFor } from "./capabilities.mjs";

/** The contract's version, carried on every `ready` event and every envelope.
 *
 *  A host pins against this. It is bumped when a command's `requires` changes
 *  meaning, an event's payload loses a field, or the envelope shape changes —
 *  never for an added command, an added event or an added refusal code, which
 *  are all additive by `docs/05` §12. */
export const CONTRACT_VERSION = 1;

/** What a command can ask of the host's grant.
 *
 *  `mutate` is not a capability: it is the question "may this page change the
 *  document at all", and `capabilities.mjs` already answers it — `editingModeFor`
 *  returns `viewing` exactly when neither `edit` nor `comment` was granted. That
 *  matters because a `commentor` MUST be able to run `format.bold`: the editor
 *  records it as a tracked suggestion, which is the whole point of the role. So
 *  `mutate` resolves through the Phase 1 authority rather than through a second
 *  copy of its rule. Everything else is a capability name, checked directly. */
export const REQUIREMENTS = Object.freeze([
  "mutate",
  "edit",
  "comment",
  "save",
  "download",
  "print",
  "open",
  "new",
  "autosave",
]);

/** Everything a refusal can say, as a code a host can branch on.
 *
 *  A code, not a sentence: the sentence is localised and belongs to the chrome,
 *  and a host writing `if (result.refusal.code === "capability-withheld")` must
 *  not be matching on English. `109` UX-017 gave the editor one feedback channel
 *  and `status_channel.mjs` already distinguishes a refusal from a confirmation;
 *  these are the machine-readable half of the same distinction.
 *
 *    unknown-command      no such id in the registry, in this state
 *    capability-withheld  the host did not grant what the command requires
 *    unavailable          the command exists but cannot run right now, and the
 *                         chrome's own reason says why ("Nothing to undo")
 *    engine-refused       it ran and the engine refused it — Viewing mode, a
 *                         read-only document — reported through the status
 *                         channel rather than guessed at
 *    threw                the command raised; the message is the exception's
 *    bad-request          the envelope or the arguments were not the contract's
 *    timeout              the transport gave up waiting (client side only)
 */
export const REFUSAL_CODES = Object.freeze([
  "unknown-command",
  "capability-withheld",
  "unavailable",
  "engine-refused",
  "threw",
  "bad-request",
  "timeout",
]);

/** Every event a host can hear, and the exact fields its detail carries.
 *
 *  `docs/126` phase 2 names the minimum set; `refusal` is in it because the
 *  editor's whole feedback design funnels through one channel and a host that
 *  cannot hear a refusal will re-issue it forever.
 *
 *  `change` carries a REVISION HANDLE, never a snapshot. `docs/107` §4 makes
 *  per-interaction work O(1) in document size an owner constraint, and an event
 *  that serialises the document on every keystroke is a defect however fast it
 *  feels on a fixture. A host that wants the bytes asks for them; a host that
 *  wants to know something changed reads two integers. */
export const HOST_EVENTS = Object.freeze([
  Object.freeze({
    name: "ready",
    detail: Object.freeze(["contract", "capabilities", "editingMode", "commands"]),
  }),
  Object.freeze({ name: "change", detail: Object.freeze(["revision", "dirty"]) }),
  Object.freeze({ name: "selection", detail: Object.freeze(["anchor", "focus", "hasRange"]) }),
  Object.freeze({ name: "save", detail: Object.freeze(["format", "name", "bytes"]) }),
  Object.freeze({ name: "export", detail: Object.freeze(["format", "name", "bytes"]) }),
  Object.freeze({ name: "error", detail: Object.freeze(["code", "message", "command"]) }),
  Object.freeze({
    name: "refusal",
    detail: Object.freeze(["code", "command", "requires", "message"]),
  }),
]);

/** Event names alone, for validation and for generating a client's surface. */
export const HOST_EVENT_NAMES = Object.freeze(HOST_EVENTS.map((event) => event.name));

/** The `postMessage` protocol, in one object.
 *
 *  ONLYOFFICE's host API is the shape to learn from and the shape to improve on.
 *  Read from their source rather than their docs: `DocsAPI.DocEditor` sends
 *  `{command, data}` one way (`_sendCommand`,
 *  `reference/web-apps/apps/api/documents/api.js:649-651`) and receives
 *  `{event, frameEditorId, data}` the other (`_onMessage`, `:470-500`). There is
 *  NO correlation id anywhere in it, so a command is fire-and-forget: a host
 *  cannot learn whether what it asked for happened, or was refused. That is the
 *  gap `rid` closes here, and it is why a refusal is part of a RESULT and not
 *  only an event.
 *
 *  Their transport also posts to a wildcard target — `wnd.postMessage(msg, "*")`
 *  at `:1303-1306`, under a literal `// TODO: specify explicit origin` — and
 *  their inbound filter carries `// TODO: check message origin` above the one
 *  real check they do make (`:1014-1018`, `_scope.frameOrigin == msg.origin`).
 *  Ours never posts to `"*"`; see `originAllowed`. */
export const PROTOCOL = Object.freeze({
  /** Present on every envelope, and equal to the protocol version. A message
   *  without it is not ours: a page may host many frames and many libraries, and
   *  a bridge that answers anything shaped vaguely right is an attack surface. */
  marker: "opendoc",
  version: 1,
  /** Envelope kinds, one per direction of travel. */
  kinds: Object.freeze(["request", "result", "event"]),
  /** What a host may ask for. Deliberately four, and deliberately the same four
   *  the in-process session exposes as methods — `describe`, `execute`, `query`,
   *  `ping` — so neither transport has a verb the other lacks. */
  requests: Object.freeze(["describe", "execute", "query", "ping"]),
});

/** A command whose id is a literal: `{ id, requires, args }`.
 *
 *  `requires` is the ONE fact this table adds to the registry, and it is the
 *  fact the registry cannot supply: the registry knows whether a command can run
 *  *right now* (`enabled`, `disabledReason`), which is state; the contract knows
 *  whether this host may *ever* run it, which is composition. `docs/126`'s
 *  container policy draws exactly that line — "never, for you" versus "not right
 *  now".
 *
 *  `args` is declared only where the registry's `run` actually takes one. Three
 *  do: `view.zoom` takes a percentage, and the two colour commands take an
 *  anchor element that no host can send across a frame boundary, so they are
 *  declared with no args and a host gets the popover at its default position. */
const exact = (id, requires, args = []) => Object.freeze({ id, requires, args: Object.freeze(args) });

/** A family of value commands: `{ prefix, requires }`.
 *
 *  A family's members are generated from the document, the font inventory or
 *  markup, so the contract declares the PREFIX and what it requires. `state`
 *  names the editor state in which the family has members at all, so the parity
 *  guard can drive that state rather than skipping the family and passing having
 *  checked nothing — which is how a guard whose fixture cannot exercise its path
 *  ships green. */
const family = (prefix, requires, state) => Object.freeze({ prefix, requires, state });

/** Editor states a family can need. The guard drives every one of them. */
export const FAMILY_STATES = Object.freeze(["always", "caret", "table", "object"]);

/** Every command id the contract knows literally.
 *
 *  Grouped by namespace, in the order `docs/05` §6 declares the namespaces. A
 *  row's `requires` is a claim about the product, so the interesting ones carry
 *  their reasoning; the repetitive ones do not, because a comment per bold
 *  button is noise.
 */
export const COMMAND_CONTRACT = Object.freeze([
  // ---- file ---------------------------------------------------------------
  // The five file capabilities are the ones a host most often wants to withhold
  // one at a time — `docs/126`'s container policy: "comments only, everything
  // else off" is a real configuration, not a rung on a ladder.
  exact("file.new", "new"),
  exact("file.open", "open"),
  exact("file.save", "save"),
  exact("file.export.pdf", "download"),
  exact("file.export.docx", "download"),
  exact("file.export.odt", "download"),
  exact("file.export.rtf", "download"),
  exact("file.export.text", "download"),
  exact("file.export.json", "download"),
  exact("file.print", "print"),
  // Reading the metadata is not a grant: the document is already on screen.
  exact("file.properties", null),
  // Recovering a draft requires the capability that WROTE it. A host that
  // withheld `autosave` has no drafts of this visitor's typing, and offering to
  // restore some would be offering work from a session the host refused to keep.
  exact("file.recoverDrafts", "autosave"),
  // Version history, for the same reason and the same capability: the timeline is
  // written by the autosave path (ADR-038), so a host that withheld `autosave`
  // has no versions of this visitor's work and opening a panel over an empty
  // store would promise a past that was never kept. The panel it opens can
  // RESTORE, which replaces the document — but that is gated where it belongs,
  // on the capability that wrote the versions in the first place; a host who
  // granted `autosave` and not `edit` gets a timeline whose restore lands in a
  // Viewing-mode session, which is the same answer every other mutation gets.
  exact("file.versionHistory", "autosave"),

  // ---- edit ---------------------------------------------------------------
  exact("edit.undo", "mutate"),
  exact("edit.redo", "mutate"),
  exact("edit.cut", "mutate"),
  // Copy is not gated, and neither is Select all or Find. Nothing in the
  // capability set covers reading, and `capabilities.mjs` says so in as many
  // words: a `preview` grants none of the nine and is still a usable reader.
  exact("edit.copy", null),
  exact("edit.paste", "mutate"),
  exact("edit.pasteText", "mutate"),
  exact("edit.selectAll", null),
  exact("edit.find", null),

  // ---- format -------------------------------------------------------------
  exact("format.bold", "mutate"),
  exact("format.italic", "mutate"),
  exact("format.underline", "mutate"),
  exact("format.strike", "mutate"),
  exact("format.subscript", "mutate"),
  exact("format.superscript", "mutate"),
  exact("format.clear", "mutate"),
  exact("format.grow", "mutate"),
  exact("format.shrink", "mutate"),
  exact("format.painter", "mutate"),
  // The two split colour controls. Their `run` takes an anchor ELEMENT so the
  // popover can be positioned, which nothing can cross a frame boundary; a host
  // calling them gets the popover where the editor would have put it anyway.
  exact("format.color", "mutate"),
  exact("format.highlight", "mutate"),
  exact("format.family", "mutate"),
  exact("format.size", "mutate"),
  exact("format.case.upper", "mutate"),
  exact("format.case.lower", "mutate"),
  exact("format.case.title", "mutate"),
  exact("format.case.sentence", "mutate"),
  exact("format.case.toggle", "mutate"),

  // ---- paragraph ----------------------------------------------------------
  exact("paragraph.align.start", "mutate"),
  exact("paragraph.align.center", "mutate"),
  exact("paragraph.align.end", "mutate"),
  exact("paragraph.align.justify", "mutate"),
  exact("paragraph.indent.increase", "mutate"),
  exact("paragraph.indent.decrease", "mutate"),
  exact("paragraph.list.bullet", "mutate"),
  exact("paragraph.list.numbered", "mutate"),
  exact("paragraph.list.checklist", "mutate"),
  exact("paragraph.list.continue", "mutate"),
  exact("paragraph.list.restart", "mutate"),

  // ---- insert -------------------------------------------------------------
  exact("insert.link", "mutate"),
  exact("insert.bookmark", "mutate"),
  exact("insert.field", "mutate"),
  exact("insert.dropCap", "mutate"),
  exact("insert.image", "mutate"),
  exact("insert.shape", "mutate"),
  exact("insert.textbox", "mutate"),
  exact("insert.symbol", "mutate"),
  exact("insert.emoji", "mutate"),
  exact("insert.table", "mutate"),
  exact("insert.header", "mutate"),
  exact("insert.footer", "mutate"),
  exact("insert.footnote", "mutate"),
  exact("insert.endnote", "mutate"),
  exact("insert.lineBreak", "mutate"),

  // ---- layout / section ---------------------------------------------------
  exact("layout.pageSetup", "mutate"),
  exact("layout.paragraph", "mutate"),
  exact("layout.margins", "mutate"),
  exact("layout.orientation", "mutate"),
  exact("layout.size", "mutate"),
  exact("layout.columns", "mutate"),
  exact("layout.indent", "mutate"),
  exact("layout.spacing", "mutate"),
  // Tab stops mutate the paragraph, so they need the same grant the rest of this
  // group needs — declared here because a capability the editor offers and the
  // contract does not name is invisible to a host, which is the one audience
  // this file exists for.
  exact("layout.tabStops", "mutate"),
  exact("layout.lineNumbers", "mutate"),
  exact("layout.watermark", "mutate"),
  exact("layout.firstPageVariant", "mutate"),
  exact("layout.evenOddVariant", "mutate"),
  // Word's Header & Footer ▸ Options, plus the position and page-numbering fields.
  // A dialog, and still a mutation: what it writes is `w:titlePg`,
  // `w:evenAndOddHeaders`, the header/footer distances and the section's page
  // numbering — the same switches the two variant toggles above write, which is why
  // they share one implementation (`header_footer_settings.mjs`).
  exact("layout.headerFooterSettings", "mutate"),
  exact("layout.arrange.position", "mutate"),
  exact("layout.arrange.wrap", "mutate"),
  // Word's Arrange group, all of it. `bringForward` was the only one declared
  // because it was the only one that existed — it shipped permanently disabled
  // carrying "the engine does not expose a z-order operation yet", and the rest
  // of the group arrived with #677 when `setObjectZOrder`, `groupObjects`,
  // `ungroupObject`, `setObjectRotation`/`setObjectFlip`, `setObjectAnchorKind`
  // and `addTextToShape` were finally called from the editor.
  //
  // Every one is a MUTATION: each writes the document and each is refused by the
  // same fail-closed gate in Viewing and Suggesting. None takes an argument —
  // the value is in the command id, as it is for `paragraph.indent.increase`,
  // because a host sending "bring forward" is not choosing a magnitude.
  exact("layout.arrange.bringForward", "mutate"),
  exact("layout.arrange.sendBackward", "mutate"),
  exact("layout.arrange.bringToFront", "mutate"),
  exact("layout.arrange.sendToBack", "mutate"),
  exact("layout.arrange.group", "mutate"),
  exact("layout.arrange.ungroup", "mutate"),
  exact("layout.arrange.rotateRight", "mutate"),
  exact("layout.arrange.rotateLeft", "mutate"),
  exact("layout.arrange.flipHorizontal", "mutate"),
  exact("layout.arrange.flipVertical", "mutate"),
  // "In line with text" is a wrap MODE, not a separate capability — it rewrites
  // the anchor (`wp:inline` vs `wp:anchor`) rather than setting a wrap, which is
  // why it is its own command beside `layout.arrange.wrap` rather than a value of
  // it.
  exact("layout.arrange.inLine", "mutate"),
  // Word's and Docs' Add Text on a shape. A `wps:wsp` with a `wps:txbx` IS a
  // shape with text, so this writes a text body into the shape it names.
  exact("layout.arrange.addText", "mutate"),

  // ---- references ---------------------------------------------------------
  exact("reference.caption", "mutate"),
  exact("reference.crossReference", "mutate"),
  exact("reference.tableOfContents", "mutate"),
  // Word's Update Table: one button, a two-radio dialog, two commands. Both
  // rewrite the contents field's own content, so both are mutations. Declared
  // here with the Arrange group because they arrived in the same window and were
  // missing for the same reason — the registry gained them and the contract did
  // not, which this guard is the only thing that notices.
  exact("reference.updateToc.pageNumbers", "mutate"),
  exact("reference.updateToc.entire", "mutate"),
  // Navigation, not mutation: it moves the caret to the heading a contents entry
  // names and changes nothing. `null` is the same grade `view.zoom.` carries.
  exact("reference.goToHeading", null),
  exact("reference.updateFields", "mutate"),
  exact("reference.updateCaptionNumbers", "mutate"),

  // ---- review -------------------------------------------------------------
  // Deciding a tracked change is EDITING, not commenting: accepting a suggestion
  // writes it into the document outright. So these require `edit` and not
  // `mutate`, which means a `commentor` cannot accept their own suggestions —
  // Google Docs' Commenter cannot either, and Word's reviewer cannot. This is
  // the one place the API is deliberately STRICTER than the chrome currently is;
  // see the PR, which reports it rather than widening the API to match a gap.
  exact("review.acceptAtCaret", "edit"),
  exact("review.rejectAtCaret", "edit"),
  exact("review.acceptNext", "edit"),
  exact("review.rejectNext", "edit"),
  exact("review.acceptAll", "edit"),
  exact("review.rejectAll", "edit"),
  // Commenting, and the three commands that are only about comments.
  exact("review.comment", "comment"),
  exact("review.comment.resolve", "comment"),
  exact("review.comment.delete", "comment"),
  // Choosing a review mode is gated by the mode itself, which is the inverse of
  // `editingModeFor` and exactly what `allowsMode` answers: Viewing is always
  // allowed because a reader may always read, Suggesting needs `comment`, and
  // Editing needs `edit`. `cycle` needs nothing because it walks the modes the
  // host allowed.
  exact("review.mode.editing", "edit"),
  exact("review.mode.suggesting", "comment"),
  exact("review.mode.viewing", null),
  exact("review.mode.cycle", null),
  // Reading and navigating other people's comments is not a grant.
  exact("review.next", null),
  exact("review.previous", null),
  exact("review.toggle", null),

  // ---- style --------------------------------------------------------------
  // Both write to the document's style table, so both are mutations. Declared
  // exactly, ahead of the `style.` family, because `style.Heading 1` applies a
  // style and these two DEFINE one.
  exact("style.updateFromSelection", "mutate"),
  exact("style.createFromSelection", "mutate"),

  // ---- table --------------------------------------------------------------
  // Selecting is navigation; everything else in the band writes.
  exact("table.select.row", null),
  exact("table.select.column", null),
  exact("table.select.table", null),

  // ---- object -------------------------------------------------------------
  // Walking the anchored objects with Tab changes nothing.
  exact("object.selectNext", null),
  exact("object.selectPrevious", null),

  // ---- tools / view -------------------------------------------------------
  // Proofing and chrome preferences. None of them touches the document, and all
  // three tools rows are `noDoc` commands in the registry for the same reason.
  exact("tools.spellCheck", null),
  exact("tools.grammarCheck", null),
  exact("tools.smartQuotes", null),
  exact("view.outline", null),
  // `view.pages` is a panel toggle and changes no document state, so it requires
  // nothing — the same shape as `view.outline`, which the taxonomy already puts
  // beside it because the two are the same kind of thing.
  exact("view.pages", null),
  exact("view.settings", null),
  exact("view.showChanges", null),
  exact("view.compactRibbon", null),
  exact("view.zoomIn", null),
  exact("view.zoomOut", null),
  exact("view.zoom", null, [
    Object.freeze({ name: "percent", type: "number", required: false }),
  ]),

  // ---- help ---------------------------------------------------------------
  exact("help.commands", null),
  exact("help.shortcuts", null),
  exact("help.about", null),
]);

/** The value families, and the state each one has members in.
 *
 *  Ten of them, covering 93 of the 214 ids the registry offered on the `rich`
 *  fixture. Each prefix ends in `.` so `style.` cannot swallow
 *  `style.createFromSelection` by accident — and even if it could, exact rows
 *  resolve first. */
export const COMMAND_FAMILIES = Object.freeze([
  // Generated from the font inventory, which is not a fixed list: two reads of
  // the same document returned different inventories when a fallback face
  // registered between them. A table of literals here would be wrong on arrival.
  family("format.family.", "mutate", "always"),
  family("format.size.", "mutate", "always"),
  family("format.underline.", "mutate", "always"),
  family("paragraph.spacing.", "mutate", "always"),
  // The two one-gesture space rows, generated beside the spacing presets.
  family("paragraph.space.", "mutate", "always"),
  family("paragraph.listFormat.", "mutate", "always"),
  // Generated from the field-kind table, and from the document's own style list.
  family("insert.field.", "mutate", "always"),
  family("style.", "mutate", "always"),
  // Generated from the zoom menu's markup.
  family("view.zoom.", null, "always"),
  // The table band and the object menus, which exist only when the caret is in a
  // table or an object is selected. Both write; `table.select.*` and
  // `object.selectNext/Previous` are the exceptions and are declared exactly.
  family("table.", "mutate", "table"),
  family("object.", "mutate", "object"),
]);

/** Exact ids, as a Set, for O(1) resolution. */
const EXACT = new Map(COMMAND_CONTRACT.map((row) => [row.id, row]));

/**
 * The contract row for `id`, or `null` when the id is outside the contract.
 *
 * Exact first, then the longest matching family prefix — longest so that a
 * future `table.style.` family could refine `table.` without the order of the
 * list deciding the answer.
 *
 * Complexity: O(1) for an exact hit, O(families) — ten — otherwise. Called once
 * per host command, never per keystroke.
 *
 * @param {string} id
 * @returns {{id: string, requires: string|null, args: readonly object[], family?: string}|null}
 */
export function commandContract(id) {
  if (typeof id !== "string" || !id) return null;
  const hit = EXACT.get(id);
  if (hit) return hit;
  let best = null;
  for (const row of COMMAND_FAMILIES) {
    if (!id.startsWith(row.prefix)) continue;
    if (!best || row.prefix.length > best.prefix.length) best = row;
  }
  if (!best) return null;
  return Object.freeze({ id, requires: best.requires, args: Object.freeze([]), family: best.prefix });
}

/**
 * Whether a capability set may run a command with this requirement, and the
 * refusal if not.
 *
 * THE FOURTH DOOR. Phase 1 layered three enforcements — the browser's iframe
 * sandbox, the engine's Viewing-mode choke point, and the chrome disabling a
 * control with a reason. The API is a fourth, and it must not be the unlocked
 * one: a host that was not granted `edit` must not be able to run an editing
 * command THROUGH THE SDK either. So this runs BEFORE dispatch, and a refusal
 * here means `run()` is never called at all.
 *
 * `mutate` resolves through `editingModeFor`, so the rule that a `commentor` may
 * change the document as tracked suggestions lives in exactly one place.
 *
 * Complexity: O(1).
 *
 * @param {string|null} requires
 * @param {{has: (name: string) => boolean}} capabilities
 * @returns {boolean}
 */
export function grantsRequirement(requires, capabilities) {
  if (requires === null || requires === undefined) return true;
  if (requires === "mutate") return editingModeFor(capabilities) !== "viewing";
  try {
    return capabilities?.has?.(requires) === true;
  } catch {
    // Fail closed. An exception on the permission path is a permission decided
    // by accident — the same rule `capabilities.mjs` states for `grants`.
    return false;
  }
}

/**
 * Parses a host-declared origin allowlist.
 *
 * THE ORIGIN CONTRACT, stated rather than defaulted. A wildcard target origin in
 * an editor that can be embedded anywhere is a security decision, not a default:
 * `postMessage(documentContent, "*")` hands the bytes to whatever page happens
 * to be the parent. ONLYOFFICE does exactly that, with a TODO where the decision
 * should be (`reference/web-apps/apps/api/documents/api.js:1303-1306`).
 *
 * WHAT WE ACCEPT, AND WHY:
 *
 *   * The editor's OWN origin, always and implicitly. A same-origin embed — the
 *     shape `<opendoc-editor>` produces, because `sandboxTokensFor` grants
 *     `allow-same-origin` so the frame can reach its own WebAssembly — is the
 *     common case, and it needs no configuration to work.
 *   * Any origin the host names explicitly, as `?hostOrigin=https://app.example`
 *     (comma-separated for several). Explicit, so a cross-origin host is a
 *     deliberate act by whoever deployed the editor rather than whoever framed
 *     it.
 *   * Nothing else. `*` and `null` are REJECTED rather than honoured: they are
 *     the two spellings of "anyone", and an editor that accepts them has no
 *     origin contract at all. A rejected entry is dropped, so a typo narrows the
 *     allowlist and never widens it.
 *
 * Complexity: O(entries).
 *
 * @param {string|null|undefined} raw the `hostOrigin` parameter, if any.
 * @param {string} selfOrigin the editor's own origin.
 * @returns {readonly string[]} absolute origins, deduplicated, `selfOrigin` first.
 */
export function parseOriginAllowlist(raw, selfOrigin) {
  const allowed = [];
  const add = (value) => {
    if (typeof value !== "string") return;
    const origin = value.trim();
    // "null" is the origin a sandboxed or `data:` document reports, and "*" is
    // the wildcard. Neither identifies anybody.
    if (!origin || origin === "*" || origin === "null") return;
    let parsed = null;
    try {
      parsed = new URL(origin);
    } catch {
      return;
    }
    // An entry must BE an origin, not a URL with a path: `https://x/y` and
    // `https://x` are the same origin, and accepting the first spelling invites
    // a host to believe the path is part of the check.
    if (parsed.origin !== origin) return;
    if (!allowed.includes(parsed.origin)) allowed.push(parsed.origin);
  };
  add(selfOrigin);
  for (const entry of String(raw ?? "").split(",")) add(entry);
  return Object.freeze(allowed);
}

/**
 * Whether a message from `origin` may be acted on.
 *
 * Complexity: O(allowlist).
 *
 * @param {string|null|undefined} origin `MessageEvent.origin`.
 * @param {readonly string[]} allowlist
 * @returns {boolean}
 */
export function originAllowed(origin, allowlist) {
  if (typeof origin !== "string" || !origin || origin === "null" || origin === "*") return false;
  return (allowlist ?? []).includes(origin);
}

/**
 * Whether `message` is a well-formed request envelope for this protocol.
 *
 * Checked rather than trusted, and checked here rather than in the bridge, so
 * the shape of a request is part of the schema and not part of a transport.
 *
 * Complexity: O(1).
 *
 * @param {unknown} message
 * @returns {boolean}
 */
export function isRequestEnvelope(message) {
  if (!message || typeof message !== "object") return false;
  if (message[PROTOCOL.marker] !== PROTOCOL.version) return false;
  if (message.kind !== "request") return false;
  if (typeof message.rid !== "string" || !message.rid) return false;
  return PROTOCOL.requests.includes(message.type);
}

/**
 * Whether `message` is a well-formed envelope the HOST side should read.
 *
 * Complexity: O(1).
 *
 * @param {unknown} message
 * @returns {boolean}
 */
export function isEditorEnvelope(message) {
  if (!message || typeof message !== "object") return false;
  if (message[PROTOCOL.marker] !== PROTOCOL.version) return false;
  return message.kind === "result" || message.kind === "event";
}

/**
 * The refusal a contract row produces for a capability set, or `null`.
 *
 * The message is an INPUT: the sentence that explains a withheld capability is
 * localised and already exists in the chrome (`capability.notGranted`), so this
 * module neither invents English nor routes a nineteenth translation.
 *
 * Complexity: O(1).
 *
 * @returns {{code: string, command: string, requires: string, message: string}|null}
 */
export function capabilityRefusal(row, capabilities, message = "") {
  if (!row || grantsRequirement(row.requires, capabilities)) return null;
  return Object.freeze({
    code: "capability-withheld",
    command: row.id,
    requires: row.requires,
    message,
  });
}

/** Every requirement the contract actually uses, derived. Exported so a guard
 *  can prove the table uses no requirement the resolver cannot answer, in both
 *  directions — a `requires: "eddit"` typo would otherwise grant everything. */
export function declaredRequirements() {
  const used = new Set();
  for (const row of COMMAND_CONTRACT) if (row.requires !== null) used.add(row.requires);
  for (const row of COMMAND_FAMILIES) if (row.requires !== null) used.add(row.requires);
  return Object.freeze([...used].sort());
}
