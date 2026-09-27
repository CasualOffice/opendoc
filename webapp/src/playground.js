// `playground.html` — host configuration with a live editor attached to it.
//
// WHY THIS PAGE EXISTS. All three `docs/126` phases shipped and none of them was
// reachable: a prospective host could read the embedding guide's tables but could
// not try the thing. `docs/99` §9.4 records that as the most expensive recurring
// pattern here — "built is not reachable" — and a capability contract is the worst
// possible subject for it, because the only convincing argument that `readonly`
// and `preview` are different presentations is watching them be different.
//
// WHY IT IS A SEPARATE PAGE FROM `embed.html`, WHICH ALREADY EMBEDS THE EDITOR.
// Three reasons, in order of weight. `embed.html` is the fixture
// `host-contract.spec.mjs` drives, and growing a test fixture into a public
// teaching page couples the two: a control added for a reader would be a control
// that spec has to keep working around. It declares `noindex` and carries no site
// chrome, because it is a host page pretending to be somebody else's site — this
// one is a page OF the site, in the sitemap, with the shared header. And it mounts
// the editor TWICE, on purpose, to show one contract over two transports; this
// page mounts it once and spends the room on configuration instead. They link to
// each other and neither duplicates the other's job.
//
// WHAT IS LIVE, AND WHAT CANNOT BE.
//
//   * The brand palette is live: it is a stylesheet, and a white-labelled build is
//     a swapped stylesheet. No reload.
//   * Role, capabilities and regions REMOUNT the frame, and that is the design
//     rather than a shortcut. `capabilities.mjs` resolves them before the frame's
//     first navigation precisely so there is no window in which the editor was
//     something else, and `docs/104` is explicit that gating must apply before the
//     first frame. A `postMessage` that could widen a live container would be the
//     second channel that file refuses to add.
//
// COMPLEXITY. Every interaction is O(controls) — five roles, nine capabilities,
// nineteen regions — plus one string build, and independent of document size. A
// brand change touches no document at all. A capability change reloads the frame
// only when the resolved URL actually differs, so a repeated click costs nothing.
//
// NO ENGLISH IN THIS FILE. Every word a person reads is in `playground.page.html`,
// where the page's language lives, and reaches this module as markup or as a
// `data-` attribute — the same contract `embed_host_demo.js` holds, and the reason
// neither module appears in `no_unrouted_strings.test.mjs`'s table. A module that
// takes its labels as input has none to route.
import {
  CAPABILITIES,
  REGIONS,
  editingModeFor,
  resolveCapabilities,
  resolveRegions,
  sandboxTokensFor,
} from "./capabilities.mjs";
import {
  BrandRefusal,
  PRODUCT,
  auditBrand,
  brandCss,
  markPaths,
  normalize,
} from "./brand_contract.mjs";
import { parsePalettes } from "./palette_parse.mjs";
import { documentTabTitle } from "./status_policy.mjs";

/** Where the editor is, and the document it opens: the shipped sample, because a
 *  capability demonstration on an empty document proves nothing — a reader has to
 *  have something they are being refused the editing of. */
const EDITOR_SRC = "./editor.html";

const form = document.querySelector("[data-controls]");
const live = document.querySelector("[data-stage]");
const idle = live?.querySelector("[data-idle]");
const mountButton = document.querySelector("[data-mount]");
const releaseButton = document.querySelector("[data-release]");

/** The visitor's WITHHOLDING intent, which outlives a role change.
 *
 *  Kept as intent rather than read back off the checkboxes, because a role change
 *  rewrites what is checkable: switching `edit` → `commentor` takes `save` away
 *  whatever the box said, and switching back should not silently restore a
 *  capability the visitor had turned off. So the sets below are what the visitor
 *  asked to remove, and a checkbox's state is derived from the role's grant minus
 *  them. One direction, always — the same rule `parseWithheld` enforces on the URL,
 *  for the same reason: a configuration channel that can widen a role is a
 *  configuration channel an attacker fills in. */
const withheldCapabilities = new Set();
const withheldRegions = new Set();

/** The editor's own palettes, fetched once from the stylesheet that decides them.
 *
 *  `src/style.css` is the source of truth for token values (`docs/63`), so the
 *  live AA audit reads it rather than a copy. Null until it arrives; the brand
 *  panel says nothing until it can say something true. */
let palettes = null;

/** The frame's `<style>` element carrying the previewed brand, so a change
 *  replaces rather than stacks. */
let brandStyle = null;

/** The `src` the mounted frame was given, so a change that resolves to the same
 *  URL does not reload the document for nothing. */
let mountedSrc = "";

/** Coalesces a burst of `input` events — dragging a colour picker fires one per
 *  frame — into one repaint. */
let queued = false;

// ---- Reading the controls --------------------------------------------------

/** The role the radios are on. Read from the markup rather than defaulted here:
 *  the generator writes `checked` onto one row, so the default is declared once. */
function role() {
  return form?.querySelector("[data-role]:checked")?.value ?? "";
}

/** `-a,-b` for a withhold list, or null when nothing is withheld.
 *
 *  Minus-prefixed even though `parseWithheld` accepts bare names, because the sign
 *  is the part that tells a host reading their own URL which direction it goes. */
function withholdParam(names) {
  return names.length ? names.map((name) => `-${name}`).join(",") : null;
}

/**
 * Everything the page currently describes: the two resolved sets, the URL that
 * reproduces them, and the brand configuration.
 *
 * Resolved through `resolveCapabilities` / `resolveRegions` — the authority — and
 * never recomputed by hand, so the readout cannot disagree with the editor it is
 * describing.
 *
 * Complexity: O(capabilities + regions).
 */
function readState() {
  const mode = role();
  const granted = resolveCapabilities({ mode, framed: true });
  const shownByRole = resolveRegions({ mode, framed: true, capabilities: granted });
  const can = withholdParam(CAPABILITIES.filter((name) => granted.has(name) && withheldCapabilities.has(name)));
  const chrome = withholdParam(REGIONS.filter((id) => shownByRole.has(id) && withheldRegions.has(id)));
  const capabilities = resolveCapabilities({ mode, framed: true, withhold: can });
  const regions = resolveRegions({ mode, framed: true, withhold: chrome, capabilities });
  const url = new URL(EDITOR_SRC, document.baseURI);
  url.searchParams.set("mode", mode);
  if (can) url.searchParams.set("can", can);
  if (chrome) url.searchParams.set("chrome", chrome);
  return {
    mode,
    can,
    chrome,
    granted,
    shownByRole,
    capabilities,
    regions,
    src: url.href,
    // What a host writes, which is a path and a query rather than an absolute URL.
    editorSrc: `${EDITOR_SRC}${url.search}`,
    brand: readBrand(),
  };
}

/** The host's `brand.json`, as the fields the panel offers.
 *
 *  Only DELTAS: a field the visitor left alone is absent, because `brand.json` is
 *  a file of differences from the product's own identity and a generated snippet
 *  full of our own defaults would teach a host to write them out. */
function readBrand() {
  const name = form?.querySelector("[data-brand-name]")?.value.trim() ?? "";
  const accent = form?.querySelector("[data-brand-accent-hex]")?.value.trim() ?? "";
  const tabTitle = form?.querySelector("[data-brand-tab-title]")?.value ?? PRODUCT.tabTitle;
  const raw = { version: 1 };
  if (name && name !== PRODUCT.name) raw.name = name;
  if (tabTitle !== PRODUCT.tabTitle) raw.tabTitle = tabTitle;
  // The swatch opens on the editor's own accent, so "unchanged" means no override
  // at all rather than an override that happens to equal ours — which is also the
  // difference between a build that pins the accent and one that does not.
  if (accent && accent.toLowerCase() !== defaultAccent()) {
    raw.theme = { tokens: { "--accent": accent }, light: {}, dark: {} };
  }
  return raw;
}

/** The accent the picker was generated to open on, read back off the control. */
function defaultAccent() {
  const swatch = form?.querySelector("[data-brand-accent]");
  return (swatch?.getAttribute("value") ?? "").toLowerCase();
}

// ---- Painting the controls -------------------------------------------------

/** Reflects a role's grant onto the switches.
 *
 *  A capability or region the role never granted is DISABLED AND SAYS WHY, not
 *  removed: a control that vanishes cannot be told from a bug, and the sentence is
 *  the one thing that teaches the rule — a list narrows, so there is nothing to
 *  check. The reason comes from the fieldset's own markup, in the page's language.
 *
 *  Complexity: O(switches). */
function reflectSwitches(state) {
  const pairs = [
    ["[data-capability]", state.granted, withheldCapabilities],
    ["[data-region]", state.shownByRole, withheldRegions],
  ];
  for (const [selector, offered, withheld] of pairs) {
    for (const box of form?.querySelectorAll(selector) ?? []) {
      const reason = box.closest("[data-narrow-reason]")?.dataset.narrowReason ?? "";
      const available = offered.has(box.value);
      box.disabled = !available;
      box.checked = available && !withheld.has(box.value);
      if (available) box.removeAttribute("title");
      else box.title = reason;
    }
  }
}

/** Paints the resolved readout: review mode, sandbox, capability chips, the
 *  regions composed away, and the URL. Every value is derived. */
function paintReadout(state) {
  set("[data-editing-mode]", editingModeFor(state.capabilities));
  set("[data-sandbox]", sandboxTokensFor(state.capabilities).join(" "));
  const withheld = REGIONS.filter((id) => !state.regions.has(id));
  const regionsOut = document.querySelector("[data-withheld-regions]");
  if (regionsOut) {
    regionsOut.textContent = withheld.length
      ? withheld.join(" ")
      : (regionsOut.dataset.noneLabel ?? "");
  }
  const chips = document.querySelector("[data-caps]");
  if (!chips) return;
  chips.replaceChildren(
    ...CAPABILITIES.map((capability) => {
      const item = document.createElement("li");
      const granted = state.capabilities.has(capability);
      item.dataset.capability = capability;
      item.dataset.state = granted ? "granted" : "withheld";
      item.textContent = capability;
      // The strikethrough is decoration; the sentence is what a screen reader
      // gets, because a capability name read out twice with no difference is not
      // a readout. Both words come from the MARKUP.
      const word = granted ? chips.dataset.grantedLabel : chips.dataset.withheldLabel;
      item.setAttribute("aria-label", [capability, word].filter(Boolean).join(": "));
      return item;
    }),
  );
}

function set(selector, value) {
  const node = document.querySelector(selector);
  if (node) node.textContent = value;
}

// ---- The two snippets ------------------------------------------------------

/** The embed a host writes, and the `brand.json` beside it.
 *
 *  Generated from the SAME state the frame was mounted from, which is what makes
 *  the promise on the page true: paste it and you get what is on screen. The
 *  element's shape is the one its README publishes — this page is not a second
 *  opinion about how to mount an editor. */
function paintSnippets(state) {
  const title = live?.dataset.frameTitle ?? "";
  const element = [
    '<script type="module">',
    '  import "@casualoffice/opendoc-embed/define";',
    "</script>",
    "",
    "<opendoc-editor",
    `  mode="${state.mode}"`,
    `  editor-src="${state.editorSrc.replaceAll("&", "&amp;")}"`,
    // `JSON.stringify` rather than an interpolated `"…"`, and not for elegance:
    // the string-site scanner reads `frame-title="…"` as a `title` sink and would
    // count this module's one interpolation as unrouted English. It quotes and
    // escapes the same way an attribute needs, so the snippet is unchanged.
    `  frame-title=${JSON.stringify(title)}`,
    "></opendoc-editor>",
  ].join("\n");
  set('[data-snippet="element"]', element);
  set('[data-snippet="brand"]', JSON.stringify(state.brand, null, 2));
}

// ---- The brand, live -------------------------------------------------------

/**
 * Validates the brand fields and, if they pass, paints them onto the frame.
 *
 * THE REFUSAL IS THE GENERATOR'S OWN. `normalize` and `auditBrand` come from
 * `src/brand_contract.mjs`, which is the code `node webapp/tools/build-brand.mjs`
 * runs, and the message is `BrandRefusal`'s — the same pair, ratio, floor and
 * nearest passing value a host would read on their terminal. Re-implementing the
 * sentence here would have been a second version of it to keep in step, and this
 * page's whole job is to show what really happens.
 *
 * A refused palette is NOT applied. That is the point of refusing rather than
 * correcting: the editor keeps the last palette that passed, so what is on screen
 * is always a configuration somebody could ship.
 *
 * Complexity: O(tokens × themes) — three palettes of about forty tokens.
 */
function applyBrand(state) {
  const refusalOut = document.querySelector("[data-refusal]");
  if (!palettes) return;
  let config = null;
  let reasons = [];
  try {
    config = normalize(state.brand, palettes.themes);
    reasons = auditBrand(config, palettes.themes).failures;
  } catch (err) {
    reasons = err instanceof BrandRefusal ? err.reasons : [String(err?.message ?? err)];
  }
  if (refusalOut) {
    refusalOut.hidden = reasons.length === 0;
    refusalOut.textContent = reasons.length ? new BrandRefusal(reasons).message : "";
  }
  if (reasons.length || !config) return;
  paintFrameBrand(config);
}

/** Puts a validated brand onto the live frame, if there is one.
 *
 *  Same-origin, so the host reaches into the frame's document — which is exactly
 *  what a white-labelled DEPLOYMENT does not have to do, because it serves the
 *  generated `src/brand.css` instead. Two adjustments are needed to preview a
 *  file's contents from outside the file, and both are transformations rather than
 *  fudges:
 *
 *    * `--brand-mark`'s `url()` is written relative to `src/brand.css`. A `<style>`
 *      element in the page resolves against the PAGE, so the `../` becomes `./`.
 *    * `applySettings()` has already written an inline `--accent` from the
 *      visitor's stored preference, and an inline declaration beats any
 *      stylesheet. In a real build the generator's `--brand-accent-pinned: 1`
 *      stops it being written at all (`src/appearance.mjs`); here it is already
 *      written, so it is removed — the same end state, reached after the fact. */
function paintFrameBrand(config) {
  const frame = live?.querySelector("iframe");
  const doc = frame?.contentDocument ?? null;
  if (!doc?.head) return;
  const css = brandCss(config, markPaths(config)).replace('url("../', 'url("./');
  if (!brandStyle || !brandStyle.isConnected || brandStyle.ownerDocument !== doc) {
    brandStyle = doc.createElement("style");
    brandStyle.dataset.playgroundBrand = "true";
    doc.head.append(brandStyle);
  }
  brandStyle.textContent = css;
  doc.documentElement.style.removeProperty("--accent");
  // What the HOST's browser tab would say. The frame's own title is the thing
  // `tabTitle` actually governs and `documentTabTitle` is the rule that composes
  // it, imported rather than paraphrased.
  const nameField = doc.getElementById("docTitle");
  const name = nameField?.value ?? "";
  if (name) {
    doc.title = documentTabTitle({
      name,
      fallback: doc.title,
      product: (config.tabTitle ?? PRODUCT.tabTitle) === "document" ? "" : (config.name ?? PRODUCT.name),
    });
  }
}

// ---- Mounting --------------------------------------------------------------

/** Mounts the editor at the resolved URL, replacing any frame already there.
 *
 *  A hand-wired iframe rather than `<opendoc-editor>`, because the element keeps
 *  its frame in a shadow root and this page has to reach the frame's document to
 *  preview a stylesheet. The element is what the SNIPPET tells a host to use, and
 *  `embed.html` is where both paths are driven side by side; here the point is the
 *  configuration, not the mounting. */
function mount(state) {
  release({ keepButton: true });
  const frame = document.createElement("iframe");
  frame.setAttribute("sandbox", sandboxTokensFor(state.capabilities).join(" "));
  frame.setAttribute("allow", "clipboard-read; clipboard-write");
  frame.title = live?.dataset.frameTitle ?? "";
  frame.src = state.src;
  frame.addEventListener("load", () => {
    // The brand is painted on LOAD as well as on change: a remount throws the
    // previewed stylesheet away with the old document, and a host colour that
    // survived only until the next role change would be a worse demonstration
    // than none.
    applyBrand(readState());
  });
  mountedSrc = state.src;
  if (idle) idle.hidden = true;
  live?.append(frame);
  if (releaseButton) releaseButton.disabled = false;
}

function release({ keepButton = false } = {}) {
  const frame = live?.querySelector("iframe");
  if (frame) {
    // Blank before detaching so the WebAssembly instance starts being released
    // before the node goes — the order `home-embed.js` established.
    try {
      frame.src = "about:blank";
    } catch {
      /* ignore */
    }
    frame.remove();
  }
  brandStyle = null;
  mountedSrc = "";
  if (keepButton) return;
  if (idle) idle.hidden = false;
  if (releaseButton) releaseButton.disabled = true;
}

// ---- The loop --------------------------------------------------------------

/** Repaints everything the controls decide, and remounts only if it has to. */
function sync() {
  const state = readState();
  reflectSwitches(state);
  paintReadout(state);
  paintSnippets(state);
  applyBrand(state);
  if (mountedSrc && state.src !== mountedSrc) mount(state);
}

function schedule() {
  if (queued) return;
  queued = true;
  requestAnimationFrame(() => {
    queued = false;
    sync();
  });
}

form?.addEventListener("submit", (event) => event.preventDefault());

form?.addEventListener("change", (event) => {
  const target = event.target;
  if (target.matches?.("[data-capability]")) toggle(withheldCapabilities, target);
  else if (target.matches?.("[data-region]")) toggle(withheldRegions, target);
  sync();
});

// `input` as well as `change`, so the colour picker and the name field are live
// while they are being used rather than on blur.
form?.addEventListener("input", (event) => {
  const target = event.target;
  if (target.matches?.("[data-brand-accent]")) mirrorAccent(target.value, "[data-brand-accent-hex]");
  else if (target.matches?.("[data-brand-accent-hex]")) mirrorAccent(target.value, "[data-brand-accent]");
  schedule();
});

/** A checkbox's state is the visitor's intent, inverted: checked means "keep". */
function toggle(withheld, box) {
  if (box.checked) withheld.delete(box.value);
  else withheld.add(box.value);
}

/** Keeps the swatch and the hex field showing one value.
 *
 *  Only when the text really is a colour the swatch can hold: `<input type=color>`
 *  silently keeps its previous value for anything else, and a swatch showing a
 *  colour the hex field does not say is the control lying about the state. A value
 *  the swatch cannot take still reaches the audit, which is what refuses it with a
 *  sentence. */
function mirrorAccent(value, selector) {
  const other = form?.querySelector(selector);
  if (!other) return;
  if (other.type === "color" && !/^#[0-9a-f]{6}$/i.test(value.trim())) return;
  other.value = value.trim();
}

document.querySelector("[data-try-failing]")?.addEventListener("click", () => {
  // The value is on the button, as a generated claim, so it cannot drift from the
  // one `build-embed-docs.mjs` proves still fails.
  const value = document.querySelector("[data-claim='failing-accent']")?.textContent.trim() ?? "";
  mirrorAccent(value, "[data-brand-accent]");
  const hex = form?.querySelector("[data-brand-accent-hex]");
  if (hex) hex.value = value;
  sync();
});

mountButton?.addEventListener("click", () => mount(readState()));
releaseButton?.addEventListener("click", () => release());

for (const button of document.querySelectorAll("[data-copy]")) {
  button.addEventListener("click", async () => {
    const text =
      document.querySelector(`[data-snippet="${button.dataset.copy}"]`)?.textContent ?? "";
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // A browser or a permission that refuses the clipboard: the panel is
      // selectable, so there is still a way through, and a thrown error here
      // would be a broken button rather than an unavailable one.
      return;
    }
    button.textContent = button.dataset.doneLabel ?? "";
    setTimeout(() => {
      button.textContent = button.dataset.idleLabel ?? "";
    }, 1500);
  });
}

// The palette, then the first paint. Fetched rather than declared: `style.css` is
// where a token's value is decided, so the live audit measures the palette the
// editor will actually render — and a brand panel that guessed would be the second
// source of truth `palette_parse.mjs` exists to avoid.
sync();
fetch(new URL("./src/style.css", document.baseURI))
  .then((response) => (response.ok ? response.text() : Promise.reject(response.status)))
  .then((css) => {
    palettes = parsePalettes(css);
    sync();
  })
  .catch(() => {
    // No palette means no honest audit, so the panel says nothing rather than
    // saying something it cannot check. The rest of the page is unaffected.
    const refusalOut = document.querySelector("[data-refusal]");
    if (refusalOut) refusalOut.hidden = true;
  });
