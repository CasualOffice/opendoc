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
export function resolveCapabilities({ mode = null, framed = false, autosave = null } = {}) {
  const asked = typeof mode === "string" ? mode.trim().toLowerCase() : "";
  const preset = PRESETS[asked] ?? PRESETS[framed ? "embedded" : "standalone"];
  const granted = new Set(preset);
  if (autosave === true) granted.add("autosave");
  else if (autosave === false) granted.delete("autosave");
  return granted;
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

/** Reads the host inputs off a real page. Separated from `resolveCapabilities`
 *  so the decision itself stays pure and testable. */
export function hostCapabilities(view = globalThis) {
  const search = view?.location?.search ?? "";
  let params = null;
  try {
    params = new URLSearchParams(search);
  } catch {
    params = null;
  }
  const mode = params?.get("mode") ?? null;
  const asked = params?.get("autosave") ?? null;
  const autosave = asked === "1" ? true : asked === "0" ? false : null;
  // `window.self !== window.top` throws on a cross-origin parent in some
  // engines; a throw means we ARE framed, which is the safer answer anyway.
  let framed = false;
  try {
    framed = view.self !== view.top;
  } catch {
    framed = true;
  }
  return resolveCapabilities({ mode, framed, autosave });
}
