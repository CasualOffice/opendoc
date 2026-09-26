// `embed.html` — a host driving the editor two ways at once.
//
// This is the page `docs/126` phase 1's exit gate asks for: the editor embedded
// TWICE in this repository, once in an iframe a host wires itself and once
// through `<opendoc-editor>`, with both driven through the five roles. It exists
// so "a host can do this" is demonstrable rather than asserted — `docs/99` §9.4
// records that "built" is not "reachable", and an embed surface with no host
// exercising it is exactly that failure.
//
// Nothing here is fixture code. The capability readout is DERIVED from the same
// authority the editor resolves, so this page cannot show a table that is not
// the real one; and neither editor boots until asked, because each holds a
// ~100 MB WebAssembly instance and the memory budget is guarded end to end
// (`memory-budget.spec.mjs`).
import {
  CAPABILITIES,
  ROLES,
  editingModeFor,
  resolveCapabilities,
  sandboxTokensFor,
} from "./capabilities.mjs";
import { defineOpenDocEditor } from "./embed_element.mjs";

defineOpenDocEditor();

/** The document each embed opens: the shipped sample, because a capability
 *  demonstration on an empty document proves nothing — a reader has to have
 *  something to be refused the editing of. */
const EDITOR_SRC = "./editor.html";

/** The default role. `commentor` rather than `edit`, so the page opens on the
 *  interesting case — a mode that is neither fully open nor fully closed. */
const DEFAULT_ROLE = "commentor";

for (const panel of document.querySelectorAll("[data-embed]")) {
  wirePanel(panel);
}

/** One panel: a role picker, a mount/release pair, a readout and a stage. Both
 *  panels are the same controller because they answer the same question two
 *  ways; writing it twice is how the two would drift into disagreeing. */
function wirePanel(panel) {
  const kind = panel.dataset.embed;
  const select = panel.querySelector("[data-role-select]");
  const mountBtn = panel.querySelector("[data-mount]");
  const releaseBtn = panel.querySelector("[data-release]");
  const stage = panel.querySelector("[data-stage]");
  const idle = panel.querySelector("[data-idle]");

  for (const role of ROLES) {
    const option = document.createElement("option");
    option.value = role;
    option.textContent = role;
    option.selected = role === DEFAULT_ROLE;
    select.append(option);
  }

  // The readout is shown before anything is mounted, and that is deliberate: a
  // host deciding which role to hand out should be able to see what each one
  // grants without paying 100 MB to find out.
  render(panel, resolveFor(select.value));

  select.addEventListener("change", () => {
    render(panel, resolveFor(select.value));
    // A live embed is REMOUNTED rather than retuned. The mode is resolved before
    // the frame's first navigation, so changing it on a running editor would be
    // a permission granted after the document was already on screen.
    if (stage.querySelector("[data-live]")) mount();
  });

  mountBtn.addEventListener("click", mount);
  releaseBtn.addEventListener("click", release);

  function mount() {
    release();
    const role = select.value;
    const live = kind === "element" ? mountElement(role) : mountIframe(role);
    live.dataset.live = "true";
    idle.hidden = true;
    stage.append(live);
    releaseBtn.disabled = false;
    render(panel, resolveFor(role));
  }

  /** The custom element's own path: the host sets `mode` and nothing else, and
   *  the element resolves, sandboxes and reports. The readout is taken from the
   *  element's event rather than recomputed here — if the page and the element
   *  disagreed, the page would be hiding it. */
  function mountElement(role) {
    const element = document.createElement("opendoc-editor");
    element.setAttribute("mode", role);
    element.setAttribute("editor-src", EDITOR_SRC);
    element.setAttribute("frame-title", panel.dataset.frameTitle ?? "");
    element.addEventListener("opendoc-capabilities", (event) => {
      render(panel, {
        capabilities: new Set(event.detail.capabilities),
        editingMode: event.detail.editingMode,
        sandbox: event.detail.sandbox,
      });
    });
    return element;
  }

  /** What a host writes with no package at all — and why they should not have
   *  to. Every line below is a decision the element already made: put the mode
   *  in the URL before the frame is attached, derive the sandbox from the
   *  capability set, and give the frame an accessible name. */
  function mountIframe(role) {
    const url = new URL(EDITOR_SRC, document.baseURI);
    url.searchParams.set("mode", role);
    const resolved = resolveFor(role);
    const frame = document.createElement("iframe");
    frame.setAttribute("sandbox", resolved.sandbox.join(" "));
    frame.setAttribute("allow", "clipboard-read; clipboard-write");
    frame.title = panel.dataset.frameTitle ?? "";
    frame.src = url.href;
    return frame;
  }

  function release() {
    const live = stage.querySelector("[data-live]");
    if (live) {
      // Blank before detaching so the WebAssembly instance starts being released
      // before the node goes, the order `home-embed.js` established. The element
      // does this for itself in `disconnectedCallback`; the raw iframe is the
      // host's job, which is one more thing the element saves them.
      const frame = live.tagName === "IFRAME" ? live : null;
      if (frame) {
        try {
          frame.src = "about:blank";
        } catch {
          /* ignore */
        }
      }
      live.remove();
    }
    idle.hidden = false;
    releaseBtn.disabled = true;
  }
}

/** The capability set, review mode and sandbox for a role, as an embed would
 *  see them: `framed: true`, because every embed on this page is framed. */
function resolveFor(role) {
  const capabilities = resolveCapabilities({ mode: role, framed: true });
  return {
    capabilities,
    editingMode: editingModeFor(capabilities),
    sandbox: sandboxTokensFor(capabilities),
  };
}

/** Paints a panel's readout.
 *
 *  Withheld capabilities are LISTED, struck through, not omitted. The same rule
 *  the chrome follows for a disabled control: an absent capability a host cannot
 *  see is a capability they will file a bug about. */
function render(panel, { capabilities, editingMode, sandbox }) {
  panel.querySelector("[data-editing-mode]").textContent = editingMode;
  panel.querySelector("[data-sandbox]").textContent = [...sandbox].join(" ");
  const list = panel.querySelector("[data-caps]");
  list.replaceChildren(
    ...CAPABILITIES.map((capability) => {
      const item = document.createElement("li");
      const granted = capabilities.has(capability);
      item.dataset.capability = capability;
      item.dataset.state = granted ? "granted" : "withheld";
      item.textContent = capability;
      // The strikethrough is decoration; the sentence is what a screen reader
      // gets, because a capability name read out twice with no difference is not
      // a readout. Both words come from the MARKUP, where this page's language
      // is, so this module carries no English of its own and adds no string debt
      // (`docs/124`: a module that takes its labels as input has none to route).
      const word = granted ? list.dataset.grantedLabel : list.dataset.withheldLabel;
      const label = [capability, word].filter(Boolean).join(": ");
      item.setAttribute("aria-label", label);
      return item;
    }),
  );
}
