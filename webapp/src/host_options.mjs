// SPDX-License-Identifier: Apache-2.0
// THE COMPETITOR'S CONFIGURATION, ACCEPTED VERBATIM — and answered honestly.
//
// ONLYOFFICE's host surface is one object: `permissions`, `editorConfig` and
// `editorConfig.customization`, documented in their own source at
// `reference/web-apps/apps/api/documents/api.js:94-320`. It is the thing an
// integrator has already written, and it is the thing their licence gates: every
// `customization` key funnels through `Common.UI.LayoutManager._applyCustomization`,
// whose second line is
//
//     if (!_licensed || !config) return;
//
// (`reference/web-apps/apps/common/main/lib/controller/LayoutManager.js:68`, and
// `_isElementVisible` and `_getInitValue` beside it carry the same test). So a
// host who configured the editor and did not pay gets an editor that ignores
// them — silently, because an early return says nothing.
//
// Ours is Apache-2.0 and UNGATED, and this module is where that becomes a
// product rather than a licence file. It does two things and nothing else:
//
//   1. **Translates.** A host hands us the object they already have; it is
//      lowered onto the three axes `capabilities.mjs` owns — capabilities,
//      regions, preferences — plus the editor's own `mode` and `lang`.
//   2. **Reports.** Every option we cannot honour comes back as a machine
//      readable note saying which option, what we did instead, and why. Nothing
//      is silently ignored — the same rule loss reporting lives by, applied to
//      configuration. That is the half ONLYOFFICE cannot offer at all, because
//      an early return has nothing to say.
//
// WHAT THIS MODULE IS NOT. It is not a second statement of what a capability or
// a region is: every name it emits is read back out of `CAPABILITIES`,
// `REGIONS` and `PREFERENCES`, and `host_options.test.mjs` fails on a row naming
// anything those three do not contain. It is a translation table between two
// vocabularies, and a translation table that invented words would be a bug.
//
// NARROWING IS THE WHOLE MODEL, AND SURVIVES TRANSLATION. ONLYOFFICE's
// permissions are booleans in both directions, and `permissions.edit: true`
// against a `readonly` container would be a widening — the one thing the
// capability model refuses. So only `false` produces anything here. A `true`
// that the resolved role does not grant is REPORTED (`cannot-widen`) rather
// than honoured or dropped: the host asked for something, and the honest answer
// is "no, and here is where to say it instead" — `mode`, decided at boot.
//
// Pure: no DOM, no engine, no `t()`. Every sentence here is developer-facing
// English in a console note, which is the one audience `docs/124` exempts —
// the same reason `embed_element.mjs` warns in English about a missing
// `frame-title`.
import { CAPABILITIES, PREFERENCES, PRESET_NAMES, REGIONS } from "./capabilities.mjs";

/** How an option was answered. A host branches on this, so it is a code.
 *
 *    mapped      honoured, by lowering it onto one of our axes
 *    narrowed    honoured in part; the note says what was dropped
 *    cannot-widen  the option would have GRANTED something; say where to ask
 *    unsupported the editor has no such state or surface yet — engine or chrome
 *                work, named in the note so it is a row rather than a shrug
 *    declined    we deliberately do not want it, and the note says why
 *    elsewhere   it belongs to a product this one is not (spreadsheets,
 *                presentations, PDF forms) — `SKILL` §1's scope table
 *    unknown     not an option ONLYOFFICE has either, so probably a typo
 */
export const NOTE_CODES = Object.freeze([
  "mapped",
  "narrowed",
  "cannot-widen",
  "unsupported",
  "declined",
  "elsewhere",
  "unknown",
]);

/** A capability withhold: `permissions.print: false` → `can=-print`. */
const cap = (name) => Object.freeze({ kind: "capability", target: name });
/** A region withhold: `customization.hideRulers: true` → `chrome=-ruler`. */
const region = (name) => Object.freeze({ kind: "region", target: name });
/** A preference: `editorConfig.user.name` → `prefs={"user":…}`. */
const pref = (name) => Object.freeze({ kind: "preference", target: name });

/**
 * Every option in ONLYOFFICE's host configuration, against ours.
 *
 * THE MAPPING TABLE, as code rather than as prose, so the published one is
 * generated from it and the two cannot drift — `docs/99` §9 rule 1, and the
 * reason every count on the embedding page is derived.
 *
 * `option` is their dotted path, exactly as their source spells it. `ours` names
 * what a host says to us instead, in the vocabulary we publish. `via` is how the
 * translator lowers it, or `null` where there is nothing to lower.
 *
 * `invert: true` marks the options whose TRUE is the restrictive direction —
 * `hideRulers`, `hideRightMenu`, `toolbarHideFileName`. Getting that backwards
 * would turn a host's "show the rulers" into "hide them", which is the class of
 * bug a boolean table exists to make impossible to write twice.
 */
export const OPTIONS = Object.freeze([
  // ---- permissions ---------------------------------------------------------
  Object.freeze({
    option: "permissions.edit",
    ours: "mode / can=-edit",
    code: "mapped",
    via: cap("edit"),
    note: "A container that may not edit is the `readonly` role, or `can=-edit` over any role.",
  }),
  Object.freeze({
    option: "permissions.download",
    ours: "can=-download",
    code: "mapped",
    via: cap("download"),
  }),
  Object.freeze({ option: "permissions.print", ours: "can=-print", code: "mapped", via: cap("print") }),
  Object.freeze({
    option: "permissions.comment",
    ours: "can=-comment",
    code: "mapped",
    via: cap("comment"),
    note: "Withholding it also closes Suggesting mode: `editingModeFor` reads the same grant.",
  }),
  Object.freeze({
    option: "permissions.reader",
    ours: "mode=readonly",
    code: "mapped",
    via: null,
    note: "A role, not a flag. Composition is decided at boot, so it travels in `mode`.",
  }),
  Object.freeze({
    option: "permissions.review",
    ours: "mode=commentor",
    code: "narrowed",
    via: null,
    note:
      "`commentor` is the role: annotate and suggest, never commit. There is no separate " +
      "switch for 'may edit but may not review', because a tracked change is an edit.",
  }),
  Object.freeze({
    option: "permissions.copy",
    ours: null,
    code: "unsupported",
    via: null,
    note:
      "No clipboard gate exists. `edit.copy` is deliberately ungated today — nothing in the " +
      "capability set covers reading, and a `preview` is a usable reader. Adding one is a " +
      "capability plus a choke point in the copy path (webapp/src/clipboard.mjs), not chrome.",
  }),
  Object.freeze({
    option: "permissions.protect",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Document protection is not implemented; there is no Protect surface to show or hide.",
  }),
  Object.freeze({
    option: "permissions.fillForms",
    ours: null,
    code: "elsewhere",
    via: null,
    note: "PDF forms are out of scope (SKILL §1).",
  }),
  Object.freeze({
    option: "permissions.modifyContentControl",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Structured document tags are modelled and round-tripped, not separately gated.",
  }),
  Object.freeze({
    option: "permissions.modifyFilter",
    ours: null,
    code: "elsewhere",
    via: null,
    note: "Spreadsheets are opencalc's (SKILL §1).",
  }),
  Object.freeze({
    option: "permissions.chat",
    ours: null,
    code: "declined",
    via: null,
    note:
      "There is no chat and there is not going to be one in the editor: a chat panel is a " +
      "messaging product wearing an editor's chrome, and the host already has one. " +
      "Collaboration here is documents — presence, comments, suggestions (ADR-033).",
  }),
  Object.freeze({
    option: "permissions.editCommentAuthorOnly",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Needs per-comment authorship enforcement, which lands with collaboration identity.",
  }),
  Object.freeze({
    option: "permissions.deleteCommentAuthorOnly",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Same identity work as `editCommentAuthorOnly`.",
  }),
  Object.freeze({
    option: "permissions.reviewGroups",
    ours: null,
    code: "unsupported",
    via: null,
    note:
      "Per-GROUP review rights need multi-user identity, which arrives with collaboration " +
      "(ADR-033, docs/107). `mode=commentor` is the single-user shape of the same intent.",
  }),
  Object.freeze({
    option: "permissions.commentGroups",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Same identity work as `reviewGroups`.",
  }),
  Object.freeze({
    option: "permissions.userInfoGroups",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Presence is not shipped, so there is no user list to filter.",
  }),

  // ---- editorConfig --------------------------------------------------------
  Object.freeze({
    option: "editorConfig.mode",
    ours: "mode",
    code: "mapped",
    via: null,
    note: `Their two values become one of ${PRESET_NAMES.length} names; "view" is \`readonly\`.`,
  }),
  Object.freeze({ option: "editorConfig.lang", ours: "lang", code: "mapped", via: null }),
  Object.freeze({
    option: "editorConfig.user.name",
    ours: 'prefs={"user":…}',
    code: "mapped",
    via: pref("user"),
    note: "Signs this visitor's comments and tracked changes.",
  }),
  Object.freeze({
    option: "editorConfig.user.id",
    ours: null,
    code: "unsupported",
    via: null,
    note: "A stable identity matters when there are several; it lands with collaboration.",
  }),
  Object.freeze({
    option: "editorConfig.user.group",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Only meaningful with `reviewGroups`/`commentGroups`, which need identity.",
  }),
  Object.freeze({
    option: "editorConfig.user.image",
    ours: null,
    code: "unsupported",
    via: null,
    note: "No avatar surface: presence is not shipped.",
  }),
  Object.freeze({
    option: "editorConfig.region",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Number and date formatting follow the document; there is no per-host override yet.",
  }),
  Object.freeze({
    option: "editorConfig.coEditing",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Co-editing is designed (ADR-033, OT over transactions) and not shipped.",
  }),
  Object.freeze({
    option: "editorConfig.canCoAuthoring",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Same as `coEditing`.",
  }),
  Object.freeze({
    option: "editorConfig.callbackUrl",
    ours: null,
    code: "declined",
    via: null,
    note:
      "There is no mandatory server (SKILL §12). The editor hands bytes to the host through " +
      "the `save` and `export` events and the host decides where they go; a URL the editor " +
      "posts to would put a server in the middle of a local-first product.",
  }),
  Object.freeze({
    option: "editorConfig.createUrl",
    ours: null,
    code: "declined",
    via: null,
    note: "Host navigation belongs to the host's page, which owns the chrome around the frame.",
  }),
  Object.freeze({
    option: "editorConfig.saveAsUrl",
    ours: null,
    code: "declined",
    via: null,
    note: "Same: the `export` event carries the bytes and the host chooses the destination.",
  }),
  Object.freeze({
    option: "editorConfig.sharingSettingsUrl",
    ours: null,
    code: "declined",
    via: null,
    note: "Sharing is the host's product. Ours has no account model to share against.",
  }),
  Object.freeze({
    option: "editorConfig.fileChoiceUrl",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Insert-image-from-host-storage needs a request/response event pair; not shipped.",
  }),
  Object.freeze({
    option: "editorConfig.recent",
    ours: null,
    code: "declined",
    via: null,
    note:
      "A list of the host's other documents inside our File page is the host's navigation " +
      "drawn in our chrome. `can=-open` is how a host keeps the container on one document.",
  }),
  Object.freeze({
    option: "editorConfig.templates",
    ours: null,
    code: "unsupported",
    via: null,
    note: "There is no template gallery yet; `can=-new` withholds the blank document instead.",
  }),
  Object.freeze({
    option: "editorConfig.actionLink",
    ours: null,
    code: "unsupported",
    via: null,
    note:
      "Open-and-scroll-to-a-bookmark exists as a command (`reference.goToHeading`, bookmarks) " +
      "but not as a boot parameter. A real gap, and chrome work rather than engine work.",
  }),
  Object.freeze({
    option: "editorConfig.plugins",
    ours: null,
    code: "unsupported",
    via: null,
    note:
      "Deliberately after the command surface (docs/126): a plugin API built before the " +
      "command contract stabilises is a second addressing scheme. `host_contract.mjs` is that " +
      "surface, and a plugin registry layers on it.",
  }),
  Object.freeze({
    option: "editorConfig.wopi",
    ours: null,
    code: "declined",
    via: null,
    note: "WOPI is a server integration protocol; there is no mandatory server.",
  }),

  // ---- customization: identity ---------------------------------------------
  Object.freeze({
    option: "customization.logo.image",
    ours: "brand.json mark",
    code: "mapped",
    via: null,
    note: "A build-time artifact, not a runtime URL: the chrome must paint with the network off.",
  }),
  Object.freeze({ option: "customization.logo.imageDark", ours: "brand.json markDark", code: "mapped", via: null }),
  Object.freeze({
    option: "customization.logo.imageLight",
    ours: "brand.json mark",
    code: "narrowed",
    via: null,
    note: "Two variants, not three: ours are light and dark, and their light header is our light theme.",
  }),
  Object.freeze({ option: "customization.logo.url", ours: "brand.json markHref", code: "mapped", via: null }),
  Object.freeze({
    option: "customization.logo.visible",
    ours: "brand.json mark:false / can=-branding",
    code: "mapped",
    via: cap("branding"),
    note: "Two spellings for two questions: the deployment has no mark, or this container shows none.",
  }),
  Object.freeze({ option: "customization.customer.name", ours: "brand.json customer.name", code: "mapped", via: null }),
  Object.freeze({
    option: "customization.customer.address",
    ours: "brand.json customer.address",
    code: "mapped",
    via: null,
  }),
  Object.freeze({ option: "customization.customer.mail", ours: "brand.json customer.mail", code: "mapped", via: null }),
  Object.freeze({ option: "customization.customer.www", ours: "brand.json customer.www", code: "mapped", via: null }),
  Object.freeze({
    option: "customization.customer.phone",
    ours: "brand.json customer.phone",
    code: "mapped",
    via: null,
  }),
  Object.freeze({ option: "customization.customer.info", ours: "brand.json customer.info", code: "mapped", via: null }),
  Object.freeze({
    option: "customization.customer.logo",
    ours: "brand.json customer.logo",
    code: "mapped",
    via: null,
    note: "Relative, like every other asset: a remote logo is a network dependency in the chrome.",
  }),
  Object.freeze({
    option: "customization.customer.logoDark",
    ours: "brand.json customer.logoDark",
    code: "mapped",
    via: null,
  }),
  Object.freeze({
    option: "customization.about",
    ours: null,
    code: "unsupported",
    via: null,
    note:
      "About and the shortcut reference are COMMAND rows (`help.about`, `help.shortcuts`), " +
      "rendered from the command taxonomy into the File page and the menu bar. Withholding " +
      "them is registry work rather than chrome, and a region that hid two of three Help rows " +
      "would leave the third reachable from the palette — present-but-unreachable in reverse.",
  }),
  Object.freeze({
    option: "customization.feedback.visible",
    ours: "brand.json feedback",
    code: "mapped",
    via: null,
    note: "There is no feedback link unless the deployment configures one, so `false` is the default.",
  }),
  Object.freeze({
    option: "customization.feedback.url",
    ours: "brand.json feedback.url",
    code: "mapped",
    via: null,
    note: "Shown in About, with the host's own label — their language, not ours.",
  }),
  Object.freeze({
    option: "customization.help",
    ours: "brand.json help.url",
    code: "narrowed",
    via: null,
    note:
      "The host's help destination is configurable; hiding our own Help rows is not — see " +
      "`customization.about`.",
  }),

  // ---- customization: leaving the editor -----------------------------------
  Object.freeze({
    option: "customization.goback",
    ours: null,
    code: "declined",
    via: null,
    note:
      "Their back button exists because their editor REPLACES the host's page — it is a " +
      "full-window application that has to offer a way out. `<opendoc-editor>` is a region " +
      "inside the host's own page, where the host's chrome already owns navigation, so a " +
      "second Back inside the frame would be a control the host did not design pointing at a " +
      "page they are already on.",
  }),
  Object.freeze({
    option: "customization.close",
    ours: null,
    code: "unsupported",
    via: null,
    note:
      "The half of `goback` that is real for an embed is the opposite direction: a visitor " +
      "inside the frame asking to leave. That is an editor control plus a `close` host event, " +
      "and it is chrome work — reported rather than half-built.",
  }),

  // ---- customization: review ----------------------------------------------
  Object.freeze({
    option: "customization.review.hideReviewDisplay",
    ours: "chrome=-review",
    code: "mapped",
    via: region("review"),
    note: "The review-mode switcher is its own region, so a host can take the control without taking the band.",
  }),
  Object.freeze({
    option: "customization.review.trackChanges",
    ours: "mode=commentor",
    code: "narrowed",
    via: null,
    note:
      "A container opens in the strongest mode its grant allows (`editingModeFor`), so " +
      "`commentor` opens in Suggesting. Opening an `edit` container in Suggesting is a " +
      "preference the editor does not have a setting for yet.",
  }),
  Object.freeze({
    option: "customization.review.showReviewChanges",
    ours: null,
    code: "unsupported",
    via: null,
    note: "`view.showChanges` is a command, not a stored setting, so there is nothing to preset.",
  }),
  Object.freeze({
    option: "customization.review.reviewDisplay",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Markup/original/final projection is designed (docs/133) and has no boot parameter.",
  }),
  Object.freeze({
    option: "customization.review.hoverMode",
    ours: null,
    code: "declined",
    via: null,
    note: "Balloons on hover fire on every pointer move over text; ours open on the change.",
  }),
  Object.freeze({
    option: "customization.reviewPermissions",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Per-group accept/reject needs identity; see `permissions.reviewGroups`.",
  }),
  Object.freeze({
    option: "customization.anonymous",
    ours: 'prefs={"user":…}',
    code: "narrowed",
    via: pref("user"),
    note: "No anonymous-name prompt: a host that knows who is looking says so, and otherwise nobody is named.",
  }),

  // ---- customization: layout ----------------------------------------------
  Object.freeze({ option: "customization.layout.toolbar", ours: "chrome=-ribbon", code: "mapped", via: region("ribbon") }),
  Object.freeze({
    option: "customization.layout.toolbar.file",
    ours: "chrome=-band.file",
    code: "mapped",
    via: region("band.file"),
  }),
  Object.freeze({
    option: "customization.layout.toolbar.home",
    ours: "chrome=-band.home",
    code: "mapped",
    via: region("band.home"),
  }),
  Object.freeze({
    option: "customization.layout.toolbar.insert",
    ours: "chrome=-band.insert",
    code: "mapped",
    via: region("band.insert"),
  }),
  Object.freeze({
    option: "customization.layout.toolbar.layout",
    ours: "chrome=-band.layout",
    code: "mapped",
    via: region("band.layout"),
  }),
  Object.freeze({
    option: "customization.layout.toolbar.references",
    ours: "chrome=-band.references",
    code: "mapped",
    via: region("band.references"),
  }),
  Object.freeze({
    option: "customization.layout.toolbar.collaboration",
    ours: "chrome=-band.review",
    code: "mapped",
    via: region("band.review"),
    note: "Their Collaboration tab is our Review band.",
  }),
  Object.freeze({
    option: "customization.layout.toolbar.view",
    ours: "chrome=-band.view",
    code: "mapped",
    via: region("band.view"),
  }),
  Object.freeze({
    option: "customization.layout.toolbar.draw",
    ours: null,
    code: "unsupported",
    via: null,
    note: "There is no Draw band: inking is not implemented. Shapes are on Insert.",
  }),
  Object.freeze({
    option: "customization.layout.toolbar.protect",
    ours: null,
    code: "unsupported",
    via: null,
    note: "No Protect band; see `permissions.protect`.",
  }),
  Object.freeze({
    option: "customization.layout.toolbar.plugins",
    ours: null,
    code: "unsupported",
    via: null,
    note: "No plugin surface yet; see `editorConfig.plugins`.",
  }),
  Object.freeze({
    option: "customization.layout.header",
    ours: "chrome=-brand,-title,-state",
    code: "narrowed",
    via: null,
    note:
      "Our top bar is three regions rather than one switch, which is finer: a host can keep " +
      "the document name and drop the mark. There is no users list or avatar to hide.",
  }),
  Object.freeze({
    option: "customization.layout.leftMenu",
    ours: "chrome=-rail",
    code: "mapped",
    via: region("rail"),
  }),
  Object.freeze({
    option: "customization.layout.leftMenu.mode",
    ours: null,
    code: "unsupported",
    via: null,
    note: "The rail's open/closed state at boot is not a stored setting; `chrome=-rail` removes it entirely.",
  }),
  Object.freeze({
    option: "customization.layout.rightMenu",
    ours: null,
    code: "unsupported",
    via: null,
    note:
      "THERE IS NO RIGHT PANEL. Object and paragraph properties are dialogs and a floating " +
      "object bar, so there is no surface to hide — and hiding one that does not exist would " +
      "be a configuration option that does nothing, which is the dead control rule in " +
      "configuration form.",
  }),
  Object.freeze({
    option: "customization.hideRightMenu",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Same: no right panel exists to hide on first load.",
  }),
  Object.freeze({
    option: "customization.layout.statusBar",
    ours: "chrome=-status",
    code: "mapped",
    via: region("status"),
  }),
  Object.freeze({
    option: "customization.layout.statusBar.actionStatus",
    ours: null,
    code: "narrowed",
    via: null,
    note: "The status bar is one region; its counts and chips are not separately withholdable.",
  }),
  Object.freeze({ option: "customization.toolbar", ours: "chrome=-ribbon", code: "mapped", via: region("ribbon") }),
  Object.freeze({ option: "customization.leftMenu", ours: "chrome=-rail", code: "mapped", via: region("rail") }),
  Object.freeze({ option: "customization.rightMenu", ours: null, code: "unsupported", via: null, note: "No right panel." }),
  Object.freeze({ option: "customization.statusBar", ours: "chrome=-status", code: "mapped", via: region("status") }),
  Object.freeze({
    option: "customization.hideRulers",
    ours: "chrome=-ruler",
    code: "mapped",
    via: region("ruler"),
    invert: true,
  }),
  Object.freeze({
    option: "customization.toolbarHideFileName",
    ours: "chrome=-title",
    code: "mapped",
    via: region("title"),
    invert: true,
  }),
  Object.freeze({
    option: "customization.comments",
    ours: "can=-comment",
    code: "mapped",
    via: cap("comment"),
  }),
  Object.freeze({
    option: "customization.chat",
    ours: null,
    code: "declined",
    via: null,
    note: "See `permissions.chat`.",
  }),

  // ---- customization: opening positions ------------------------------------
  Object.freeze({
    option: "customization.spellcheck",
    ours: 'prefs={"spellcheck":…}',
    code: "mapped",
    via: pref("spellcheck"),
  }),
  Object.freeze({
    option: "customization.features.spellcheck.mode",
    ours: 'prefs={"spellcheck":…}',
    code: "mapped",
    via: pref("spellcheck"),
  }),
  Object.freeze({
    option: "customization.features.spellcheck.change",
    ours: null,
    code: "unsupported",
    via: null,
    note:
      "Hiding the proofing switches entirely would be a region; there is none, because " +
      "Settings is one region and splitting it per row is a redesign of the dialog.",
  }),
  Object.freeze({
    option: "customization.uiTheme",
    ours: 'prefs={"theme":…}',
    code: "narrowed",
    via: pref("theme"),
    note:
      "A DEFAULT, never a pin. ADR-039 refuses to let a deployment pin light/dark because " +
      "that is a reader's preference about their own eyes; an opening position they can " +
      "change in Settings takes nothing from them.",
  }),
  Object.freeze({
    option: "customization.autosave",
    ours: "can=-autosave / autosave=0",
    code: "mapped",
    via: cap("autosave"),
    note: "A capability rather than a preference, because it decides whether drafts are KEPT on this origin.",
  }),
  Object.freeze({
    option: "customization.forcesave",
    ours: null,
    code: "declined",
    via: null,
    note: "Forced save is a server round-trip; there is no mandatory server.",
  }),
  Object.freeze({
    option: "customization.compactToolbar",
    ours: null,
    code: "unsupported",
    via: null,
    note:
      "The compact (Docs-shaped) toolbar exists and `view.compactRibbon` toggles it, but the " +
      "choice is not a stored setting, so there is nothing for a boot parameter to write. " +
      "One setting key away, and reported rather than faked.",
  }),
  Object.freeze({
    option: "customization.compactHeader",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Same as `compactToolbar`.",
  }),
  Object.freeze({
    option: "customization.toolbarNoTabs",
    ours: null,
    code: "declined",
    via: null,
    note:
      "A tabless ribbon is a different information architecture, not a setting: `docs/122` " +
      "fixed the editor on ONE navigation axis at a time, and a ribbon with no tabs beside a " +
      "menu bar is two.",
  }),
  Object.freeze({
    option: "customization.features.tabStyle",
    ours: null,
    code: "declined",
    via: null,
    note:
      "Fill-versus-line tabs is a restyle, and `docs/63` says to propose visual changes rather " +
      "than make them. The white-label seam is tokens; geometry and chrome shape are not brand.",
  }),
  Object.freeze({
    option: "customization.features.tabBackground",
    ours: null,
    code: "declined",
    via: null,
    note: "Same as `tabStyle`.",
  }),
  Object.freeze({
    option: "customization.features.featuresTips",
    ours: null,
    code: "declined",
    via: null,
    note: "There are no what's-new tips to suppress, and adding some in order to suppress them is backwards.",
  }),
  Object.freeze({
    option: "customization.suggestFeature",
    ours: null,
    code: "declined",
    via: null,
    note: "No in-product feedback nag. `brand.json feedback.url` is a link the host owns.",
  }),
  Object.freeze({
    option: "customization.zoom",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Zoom is per session and not stored, so there is no default for a host to move.",
  }),
  Object.freeze({
    option: "customization.unit",
    ours: null,
    code: "unsupported",
    via: null,
    note:
      "The dialogs and the ruler are inches only (`webapp/src/units.mjs`: twips and EMUs " +
      "inside, inches at the field). Centimetres and points are a real gap and a real piece " +
      "of work — every dialog parser, the ruler readout and the status bar — not a flag.",
  }),
  Object.freeze({
    option: "customization.font",
    ours: null,
    code: "unsupported",
    via: null,
    note: "A default face and size for new documents is engine work: it is the Normal style.",
  }),
  Object.freeze({
    option: "customization.pointerMode",
    ours: null,
    code: "unsupported",
    via: null,
    note: "There is no hand/pan tool; `webapp/src/pointer_cursor.mjs` reflects state, it does not offer a mode.",
  }),
  Object.freeze({
    option: "customization.macros",
    ours: null,
    code: "declined",
    via: null,
    note:
      "There is no macro engine and there is not going to be one on this schedule. A document " +
      "format that can execute code in the reader's browser is the single largest attack " +
      "surface an office suite has; `.docm` is already refused at open. So there is nothing " +
      "to gate — which is a stronger answer than a policy flag.",
  }),
  Object.freeze({
    option: "customization.macrosMode",
    ours: null,
    code: "declined",
    via: null,
    note: "See `customization.macros`: no engine, so no warn/enable/disable policy.",
  }),
  Object.freeze({
    option: "customization.plugins",
    ours: null,
    code: "unsupported",
    via: null,
    note: "See `editorConfig.plugins`.",
  }),
  Object.freeze({
    option: "customization.mentionShare",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Mentions need a user directory, which arrives with collaboration.",
  }),
  Object.freeze({
    option: "customization.compatibleFeatures",
    ours: null,
    code: "declined",
    via: null,
    note:
      "We do not have a lowest-common-denominator mode to switch into: unsupported document " +
      "data is preserved or reported (SKILL §12), which is the same promise without a flag.",
  }),
  Object.freeze({
    option: "customization.integrationMode",
    ours: null,
    code: "declined",
    via: null,
    note: "Their flag turns off a scroll-into-view the embed does; ours never scrolls the host's page.",
  }),
  Object.freeze({
    option: "customization.forceWesternFontSize",
    ours: null,
    code: "unsupported",
    via: null,
    note: "Chinese-locale font-size presentation; not implemented.",
  }),
  Object.freeze({
    option: "customization.wordHeadingsColor",
    ours: null,
    code: "declined",
    via: null,
    note: "A document's heading colour belongs to its style table, not to the host's configuration.",
  }),
  Object.freeze({
    option: "customization.mobile",
    ours: null,
    code: "unsupported",
    via: null,
    note: "The phone layout is in flight (docs/148); its host switches are not designed yet.",
  }),
  Object.freeze({
    option: "customization.hideNotes",
    ours: null,
    code: "elsewhere",
    via: null,
    note: "Presentations.",
  }),
  Object.freeze({
    option: "customization.slidePlayerBackground",
    ours: null,
    code: "elsewhere",
    via: null,
    note: "Presentations.",
  }),
  Object.freeze({
    option: "customization.showVerticalScroll",
    ours: null,
    code: "elsewhere",
    via: null,
    note: "Spreadsheets.",
  }),
  Object.freeze({
    option: "customization.showHorizontalScroll",
    ours: null,
    code: "elsewhere",
    via: null,
    note: "Spreadsheets.",
  }),
  Object.freeze({
    option: "customization.submitForm",
    ours: null,
    code: "elsewhere",
    via: null,
    note: "PDF forms.",
  }),
  Object.freeze({
    option: "customization.startFillingForm",
    ours: null,
    code: "elsewhere",
    via: null,
    note: "PDF forms.",
  }),
  Object.freeze({
    option: "customization.features.roles",
    ours: null,
    code: "elsewhere",
    via: null,
    note: "PDF form roles.",
  }),
]);

/** Option path → row, for O(1) lookup during translation. */
const BY_OPTION = new Map(OPTIONS.map((row) => [row.option, row]));

/** Every option path, in source order. Exported so the published mapping table
 *  and its guard read the same list. */
export const OPTION_PATHS = Object.freeze(OPTIONS.map((row) => row.option));

/** How many of their options fall into each answer. Derived, never written down
 *  — `SKILL` §8: a hand-maintained count has drifted into a false public claim
 *  here twice. */
export function optionTally() {
  const tally = Object.fromEntries(NOTE_CODES.map((code) => [code, 0]));
  for (const row of OPTIONS) tally[row.code] += 1;
  return Object.freeze(tally);
}

/** The vocabularies a `via` may point into. One lookup per kind, read from the
 *  authority rather than restated. */
const VOCABULARY = Object.freeze({
  capability: CAPABILITIES,
  region: REGIONS,
  preference: Object.freeze(PREFERENCES.map((row) => row.name)),
});

/** Whether every `via` in the table names something that exists. Exported for
 *  the guard, which is the only thing standing between this table and a row that
 *  silently translates to nothing. */
export function unknownTargets() {
  const bad = [];
  for (const row of OPTIONS) {
    if (!row.via) continue;
    if (!VOCABULARY[row.via.kind]?.includes(row.via.target)) bad.push(`${row.option} → ${row.via.kind} ${row.via.target}`);
  }
  return Object.freeze(bad);
}

/** Walks a nested config object and yields `[dotted.path, value]` for every
 *  LEAF. Their config nests (`customization.layout.toolbar.home`) and their own
 *  `_applyCustomization` walks it recursively for the same reason.
 *
 *  A node that is BOTH an object and a documented option — `layout.toolbar` may
 *  be `false` or an object of tabs — yields nothing for itself and recurses,
 *  which is exactly their behaviour.
 *
 *  Complexity: O(nodes). Depth-bounded by the caller's object; a cycle would not
 *  terminate, so `seen` refuses one rather than hanging a host's page. */
function* leaves(node, prefix, seen) {
  if (!node || typeof node !== "object") return;
  if (seen.has(node)) return;
  seen.add(node);
  for (const [key, value] of Object.entries(node)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (value && typeof value === "object" && !Array.isArray(value)) yield* leaves(value, path, seen);
    else yield [path, value];
  }
}

/** Their `mode` vocabulary, as ours. Two values, and `edit` is not the default:
 *  an element-mounted editor is framed, so absent still means the framed preset. */
const MODE_ALIASES = Object.freeze({ view: "readonly", edit: "edit" });

/**
 * Lowers an ONLYOFFICE-shaped host configuration onto ours.
 *
 * Returns the four things the element needs to build a URL, plus the notes. It
 * does NOT resolve a capability set: resolution is `capabilities.mjs`'s, once,
 * at boot, and a translator that resolved as well would be the second answer
 * that eventually disagrees.
 *
 * Complexity: O(leaves in the config).
 *
 * @param {object|string|null|undefined} config their config, or its JSON text.
 * @returns {{mode: string|null, can: readonly string[], chrome: readonly string[],
 *            prefs: Readonly<Record<string, unknown>>, lang: string|null,
 *            notes: readonly {option: string, code: string, message: string}[]}}
 */
export function translateHostConfig(config) {
  const source = typeof config === "string" ? safeParse(config) : config;
  const notes = [];
  const can = [];
  const chrome = [];
  const prefs = {};
  let mode = null;
  let lang = null;
  if (!source || typeof source !== "object") {
    return Object.freeze({ mode, can: Object.freeze([]), chrome: Object.freeze([]), prefs: Object.freeze({}), lang, notes: Object.freeze([]) });
  }

  const add = (option, code, message) => notes.push(Object.freeze({ option, code, message }));

  for (const [path, value] of leaves(source, "", new Set())) {
    // Their config nests everything but `permissions` under `editorConfig`, and
    // `customization` under that again. The table spells the paths the way their
    // documentation does, so the one rewrite is dropping the `editorConfig.`
    // prefix in front of `customization`.
    const option = path.replace(/^editorConfig\.customization\./, "customization.");
    if (option === "editorConfig.mode" || option === "mode") {
      const asked = MODE_ALIASES[String(value)] ?? null;
      if (asked) {
        mode = asked;
        add(option, "mapped", `mode="${asked}"`);
      } else {
        add(option, "unknown", `"${value}" is not one of their two modes; ignored.`);
      }
      continue;
    }
    if (option === "editorConfig.lang" || option === "lang") {
      if (typeof value === "string" && value.trim()) {
        lang = value.trim();
        add(option, "mapped", `lang=${lang}`);
      }
      continue;
    }
    const row = BY_OPTION.get(option);
    if (!row) {
      add(option, "unknown", "Not an option ONLYOFFICE documents either — check the spelling.");
      continue;
    }
    if (!row.via) {
      add(option, row.code, row.note ?? `No equivalent: ours is ${row.ours ?? "nothing"}.`);
      continue;
    }
    // THE NARROWING RULE, and the one place it could be lost. Their booleans go
    // both ways; ours go one. `invert` marks the options whose TRUE is the
    // restrictive direction.
    const restrictive = row.invert === true ? value === true : value === false;
    if (!restrictive) {
      if (row.via.kind === "preference") {
        prefs[row.via.target] = value;
        add(option, "mapped", `prefs.${row.via.target}`);
      } else {
        add(
          option,
          "cannot-widen",
          `A host list may only narrow a role. Ask for the capability by choosing a "mode" instead.`,
        );
      }
      continue;
    }
    if (row.via.kind === "capability") {
      if (!can.includes(row.via.target)) can.push(row.via.target);
      add(option, "mapped", `can=-${row.via.target}`);
    } else if (row.via.kind === "region") {
      if (!chrome.includes(row.via.target)) chrome.push(row.via.target);
      add(option, "mapped", `chrome=-${row.via.target}`);
    } else {
      prefs[row.via.target] = value;
      add(option, "mapped", `prefs.${row.via.target}`);
    }
  }

  return Object.freeze({
    mode,
    can: Object.freeze(can),
    chrome: Object.freeze(chrome),
    prefs: Object.freeze(prefs),
    lang,
    notes: Object.freeze(notes),
  });
}

/** JSON that must not throw: a malformed `config` attribute leaves the editor
 *  with its own defaults, never with no editor. */
function safeParse(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

/** The notes worth saying out loud: everything we did not simply do.
 *
 *  `mapped` is left out on purpose — a console that reports success is a console
 *  nobody reads, and the full list is on the event detail for a host that wants
 *  it. Complexity: O(notes). */
export function unhandledNotes(notes) {
  return Object.freeze((notes ?? []).filter((note) => note.code !== "mapped"));
}
