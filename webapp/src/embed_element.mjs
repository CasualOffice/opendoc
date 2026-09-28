// `<opendoc-editor>` — the thing a host puts on a page.
//
// Framework free on purpose. A custom element works in React, Vue, Svelte and
// plain HTML with no binding layer, so `docs/126` phase 1 ships one element
// rather than three wrappers; framework bindings are explicitly deferred.
//
// It mounts the editor in an iframe, which is the same shape ONLYOFFICE's
// `DocsAPI.DocEditor` uses — `createIframe` at
// `reference/web-apps/apps/api/documents/api.js:1278` — and for the same
// reason: the editor is a whole application with its own stylesheet, its own
// focus model and a WebAssembly instance, and dropping that into a host's
// document tree makes both of them fragile. The difference is that they set no
// `sandbox` attribute at all and we derive one from the capability set, and
// that their host-customization path early-returns when unlicensed
// (`reference/web-apps/apps/common/main/lib/controller/LayoutManager.js:68`,
// `if (!_licensed || !config) return;`) where ours is ungated — the permissive
// licence is the wedge, so gating it would surrender the only structural
// advantage we have.
//
// THE MODE IS IN THE URL BEFORE THE IFRAME IS ATTACHED. That is what "resolved
// before first paint" means here: the frame's first navigation already carries
// the decision, so there is no window in which the editor is standalone and is
// then told otherwise. A `mode` set afterwards therefore REMOUNTS rather than
// mutating a live editor — a permission handed over after the document is on
// screen was never a permission.
//
// No English in this file. The one user-facing string an embed needs is the
// frame's accessible name, and it is an INPUT (`frame-title`), because the host
// owns their page's language and their i18n already produced the rest of the
// sentence it sits in.
import {
  CAPABILITIES,
  REGIONS,
  editingModeFor,
  parseWithheld,
  resolveCapabilities,
  resolveRegions,
  sandboxTokensFor,
} from "./capabilities.mjs";

/** Where the editor lives relative to the host page, when the host does not say. */
export const DEFAULT_EDITOR_SRC = "./editor.html";

/** The element's tag. Exported so a host that must rename it still has one
 *  spelling to change, and so a guard can assert the name rather than repeat it. */
export const OPENDOC_EDITOR_TAG = "opendoc-editor";

/** Fired once per mount, after the capability set is resolved and the frame is
 *  attached. Deliberately NOT one of `docs/126` phase 2's editor events
 *  (`ready`, `change`, `selection`, `save`, `export`, `error`, `refusal`) — this
 *  is the capability contract announcing itself, not the editor reporting on a
 *  document, and the two must not be one channel. */
export const CAPABILITIES_EVENT = "opendoc-capabilities";

/** Attributes that change what the embed IS, so a change to any of them is a
 *  remount rather than a live update. */
const MOUNT_ATTRIBUTES = Object.freeze(["mode", "can", "chrome", "editor-src", "frame-title"]);

/** The two NARROWING attributes, each read against the authority's own vocabulary.
 *
 *  `can` and `chrome` exist as attributes because the ROLE was otherwise the only
 *  thing this element could express — which made the role the unit of the
 *  declarative API, and the role is explicitly not the unit of anything
 *  (`capabilities.mjs`: "a role is a name for a subset... nothing downstream
 *  should ever branch on a role name"). A host wanting "commentor, but without
 *  download" had to hand-write `editor-src="./editor.html?can=-download"` — to
 *  drop out of the declarative API in order to say the thing the model is actually
 *  built around.
 *
 *  The vocabularies come from `capabilities.mjs` rather than being listed here, so
 *  there is no second statement of what a capability or a region is, and
 *  `parseWithheld` does the reading — which is what makes these attributes
 *  NARROW-ONLY by construction rather than by review.
 *
 *  A list on the attribute is UNIONED with one already in `editor-src`'s query
 *  rather than replacing it, which is the one place this differs from `mode`.
 *  `mode` names a starting set, so the more specific spelling has to win; a
 *  withhold list only ever takes away, so the union of two lists is the narrower
 *  of the three readings and cannot hand back something either spelling removed.
 *  Replacing would have let `can="-print"` on an element whose `editor-src` said
 *  `?can=-download` quietly restore `download` — a widening, through the one
 *  channel the whole model rests on never widening. */
const NARROWING = Object.freeze([
  Object.freeze({ attribute: "can", known: CAPABILITIES }),
  Object.freeze({ attribute: "chrome", known: REGIONS }),
]);

/** What the element extends.
 *
 *  `HTMLElement` does not exist outside a browser, and `class X extends
 *  undefined` throws at MODULE EVALUATION — so importing this package at all
 *  would crash any host that server-renders, which is most of them. A host
 *  discovering that from a stack trace in their build is a worse first
 *  experience than anything this package does right, and it cost nothing to
 *  avoid: in a non-browser runtime the class extends an inert base, is exported
 *  as usual, and is simply never constructed because `defineOpenDocEditor`
 *  returns false with no `customElements` to register against. */
const ElementBase = typeof HTMLElement === "function" ? HTMLElement : class {};

const SHADOW_STYLE = `
  :host { display: block; position: relative; min-height: 240px; }
  :host([hidden]) { display: none; }
  iframe { display: block; width: 100%; height: 100%; min-height: inherit; border: 0; }
`;

/**
 * An embedded opendoc editor.
 *
 * ```html
 * <opendoc-editor mode="readonly" frame-title="…"></opendoc-editor>
 * ```
 *
 * Attributes (all optional):
 *
 *  * `mode` — one of the five roles (`preview`, `readonly`, `commentor`,
 *    `edit`, `owner`) or a legacy preset name. Absent means the framed default,
 *    which an element-mounted editor always is, so absent is never
 *    `standalone`. NOT called `role`: `role` is a global ARIA attribute, and
 *    `role="readonly"` would publish an invalid ARIA role to every assistive
 *    technology on the page. `mode` is also the spelling the URL parameter and
 *    `resolveCapabilities({ mode })` already use, so there is one vocabulary
 *    rather than two.
 *  * `can` — capabilities to WITHHOLD from the role: `"-download"`, or
 *    `"-print,-download"`. Narrows, never widens, whatever the role. This is the
 *    per-capability half of the policy the model rests on, and it is an attribute
 *    so that a host expressing "commentor, but without download" does not have to
 *    leave the declarative API and hand-write a query string.
 *  * `chrome` — chrome regions to withhold, by the same rule: `"-brand,-rail"`.
 *  * `editor-src` — where `editor.html` is. May carry the host's own query
 *    (`?blank=1`, `?fixture=rich`); `mode` is merged into it, never appended
 *    twice, and `can`/`chrome` are unioned with any the query already carries.
 *  * `frame-title` — the frame's accessible name, in the host's language.
 *
 * Properties: `mode`, `capabilities`, `regions`, `editingMode`, `sandbox`,
 * `frame`, `can(name)`. `can` and `chrome` have no reflecting property on
 * purpose — see the note in the class body.
 *
 * Complexity: O(1) per mount. Nothing here walks a document.
 */
export class OpenDocEditorElement extends ElementBase {
  static get observedAttributes() {
    return [...MOUNT_ATTRIBUTES];
  }

  #root = null;
  #frame = null;
  #capabilities = new Set();
  #regions = new Set();
  #remountQueued = false;

  /** The role or preset this embed resolved to, as the host asked for it. */
  get mode() {
    return this.getAttribute("mode");
  }

  set mode(value) {
    if (value === null || value === undefined) this.removeAttribute("mode");
    else this.setAttribute("mode", String(value));
  }

  // NO REFLECTING PROPERTY FOR `can` OR `chrome`, and stated rather than left as
  // an omission. `can(capability)` is already on this prototype and is published
  // in the package's types and README, so `get can()` would REPLACE it — a silent
  // break of the one method a host uses to ask what they were granted. Giving the
  // attribute a different property name to dodge that would put two spellings on
  // one input, which is the drift this element exists to remove. So the
  // attributes are set as attributes (`setAttribute`, or markup, in every
  // framework), and what they RESOLVED to is read back off `capabilities`,
  // `regions` and `can(name)` — which is the more useful question anyway, because
  // it is the one the editor was actually mounted with.

  /** What this embed is allowed to do, resolved. Frozen: a host that could
   *  mutate the returned set would be editing a permission after the fact. */
  get capabilities() {
    return Object.freeze([...this.#capabilities]);
  }

  /** The chrome regions this embed is offered, from the same inputs. The other
   *  half of what a role names, and frozen for the same reason. */
  get regions() {
    return Object.freeze([...this.#regions]);
  }

  /** Which of the editor's three review modes this capability set means. */
  get editingMode() {
    return editingModeFor(this.#capabilities);
  }

  /** The iframe sandbox tokens in force. */
  get sandbox() {
    return sandboxTokensFor(this.#capabilities);
  }

  /** The live frame, for a host that needs to size or focus it. Null before
   *  the first mount and after disconnection. */
  get frame() {
    return this.#frame;
  }

  /** Whether a named capability was granted. */
  can(capability) {
    return this.#capabilities.has(capability);
  }

  connectedCallback() {
    if (!this.#root) this.#root = this.attachShadow({ mode: "open" });
    this.#mount();
  }

  disconnectedCallback() {
    this.#teardown();
  }

  attributeChangedCallback(name, previous, next) {
    if (previous === next) return;
    if (!MOUNT_ATTRIBUTES.includes(name)) return;
    if (!this.isConnected || !this.#root) return;
    // Coalesce: a host setting `mode` and `editor-src` in the same turn means
    // one new embed, not two, and booting a WebAssembly instance to throw it
    // away a microtask later is ~100 MB of pointless work.
    if (this.#remountQueued) return;
    this.#remountQueued = true;
    Promise.resolve().then(() => {
      this.#remountQueued = false;
      if (this.isConnected) this.#mount();
    });
  }

  /** Builds the frame from scratch. Resolution happens here, before the frame
   *  is created, so the mode is in the very first URL the frame navigates to. */
  #mount() {
    this.#teardown();

    const url = this.#resolveUrl();
    const resolution = {
      // An element-mounted editor is framed by construction, so the default
      // when nobody asked is the framed one. This is the same rule the page
      // applies to itself from `window.self !== window.top`; stating it here
      // as `framed: true` is not a second mechanism, it is this call site
      // telling the one authority a fact it cannot read from a URL.
      framed: true,
      mode: url.searchParams.get("mode"),
    };
    this.#capabilities = resolveCapabilities({
      ...resolution,
      autosave: autosaveRequest(url.searchParams.get("autosave")),
      withhold: url.searchParams.get("can"),
    });
    // From the URL the frame is about to be given, not from the attributes, so the
    // element cannot resolve one thing and navigate to another. The same reason
    // `mode` is read back off the URL two lines up.
    this.#regions = resolveRegions({
      ...resolution,
      withhold: url.searchParams.get("chrome"),
      capabilities: this.#capabilities,
    });

    const frame = document.createElement("iframe");
    // `sandbox` and `src` are both set BEFORE the element enters the document,
    // so the frame never navigates once unsandboxed and again correctly.
    frame.setAttribute("sandbox", this.sandbox.join(" "));
    frame.setAttribute("allow", "clipboard-read; clipboard-write");
    frame.setAttribute("referrerpolicy", "same-origin");
    frame.loading = "eager";
    const title = this.getAttribute("frame-title") ?? this.getAttribute("aria-label");
    // An iframe with no accessible name is an axe `frame-title` violation, and
    // this module has no business inventing English for a host's page. So the
    // host is told, loudly and once, rather than silently shipped a label in a
    // language their page may not be in.
    if (title) frame.title = title;
    else reportMissingTitle();
    frame.src = url.href;

    const style = document.createElement("style");
    style.textContent = SHADOW_STYLE;
    this.#root.replaceChildren(style, frame);
    this.#frame = frame;

    this.dispatchEvent(
      new CustomEvent(CAPABILITIES_EVENT, {
        bubbles: true,
        detail: {
          mode: url.searchParams.get("mode"),
          // The NORMALISED lists, read back off the URL the frame was given —
          // so a host listening to this hears what was applied rather than what
          // they typed, and an unknown entry they wrote is visibly gone.
          can: url.searchParams.get("can"),
          chrome: url.searchParams.get("chrome"),
          capabilities: this.capabilities,
          regions: this.regions,
          editingMode: this.editingMode,
          sandbox: this.sandbox,
          src: url.href,
        },
      }),
    );
  }

  /** The frame's URL, with the resolved mode and both withhold lists written
   *  into it.
   *
   *  The `mode` ATTRIBUTE wins over a `mode` the host left in `editor-src`, and
   *  the effective value is written back so the URL and the capability set can
   *  never disagree — a frame whose query says one thing while the element
   *  resolved another is the drift this whole phase exists to remove.
   *
   *  `can` and `chrome` are UNIONED with the query's rather than replacing it,
   *  and normalised through `parseWithheld` before being written back, so the
   *  frame is navigated to a list the authority has already read: an unknown entry
   *  is dropped HERE, once, instead of being carried in the URL for the page to
   *  drop again. Written minus-prefixed because the sign is the part that tells a
   *  host reading their own URL which direction it goes.
   *
   *  Complexity: O(entries) per attribute — at most `CAPABILITIES + REGIONS`. */
  #resolveUrl() {
    const base = this.getAttribute("editor-src") || DEFAULT_EDITOR_SRC;
    // `document.baseURI` rather than `location.href`: a host page with a
    // `<base>` element means something by it.
    const url = new URL(base, document.baseURI);
    const asked = this.getAttribute("mode") ?? url.searchParams.get("mode");
    // No mode anywhere is a deliberate absence, not an empty string: it must
    // reach `resolveCapabilities` as "nobody asked", which with `framed: true`
    // is the embedded default.
    if (asked === null) url.searchParams.delete("mode");
    else url.searchParams.set("mode", asked);
    for (const { attribute, known } of NARROWING) {
      const merged = [url.searchParams.get(attribute) ?? "", this.getAttribute(attribute) ?? ""];
      const names = parseWithheld(merged.join(","), known);
      if (names.length) url.searchParams.set(attribute, names.map((name) => `-${name}`).join(","));
      else url.searchParams.delete(attribute);
    }
    return url;
  }

  /** Releases the frame. Blanked before removal so the WebAssembly instance,
   *  its canvases and its heap start being collected before the node is
   *  detached — the same order `home-embed.js` established for the marketing
   *  page's live demo, where a leaked instance is ~100 MB. */
  #teardown() {
    if (this.#frame) {
      try {
        this.#frame.src = "about:blank";
      } catch {
        /* a cross-origin editor refuses the write; removal still releases it */
      }
      this.#frame.remove();
      this.#frame = null;
    }
    this.#root?.replaceChildren();
    this.#capabilities = new Set();
    this.#regions = new Set();
  }
}

/** `?autosave=1` / `?autosave=0` as the tri-state the authority expects. */
function autosaveRequest(value) {
  return value === "1" ? true : value === "0" ? false : null;
}

let warnedAboutTitle = false;

/** Said once per page, not once per mount: a host that remounts on every render
 *  should not have their console filled. */
function reportMissingTitle() {
  if (warnedAboutTitle) return;
  warnedAboutTitle = true;
  console.warn(
    `<${OPENDOC_EDITOR_TAG}> has no frame-title or aria-label, so the embedded ` +
      "editor has no accessible name. Set one in your page's language.",
  );
}

/**
 * Registers the element.
 *
 * Idempotent, and tolerant of a second copy of this package on the page: two
 * bundles each calling `customElements.define` for one tag throws, and a host
 * whose build deduplicated badly should get a working editor rather than a
 * crash on import.
 *
 * @param {string} [tag]
 * @returns {boolean} whether this call was the one that registered it.
 */
export function defineOpenDocEditor(tag = OPENDOC_EDITOR_TAG) {
  if (typeof customElements === "undefined") return false;
  if (customElements.get(tag)) return false;
  customElements.define(tag, OpenDocEditorElement);
  return true;
}

export { editingModeFor, resolveCapabilities, resolveRegions, sandboxTokensFor };
export { CAPABILITIES, LEGACY_PRESETS, PRESET_NAMES, REGIONS, ROLES } from "./capabilities.mjs";
