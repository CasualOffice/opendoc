// `embed.html` — a host driving the editor two ways at once.
//
// This is the page `docs/126` phase 1's exit gate asks for: the editor embedded
// TWICE in this repository, once in an iframe a host wires itself and once
// through `<opendoc-editor>`, with both driven through the five roles. It exists
// so "a host can do this" is demonstrable rather than asserted — `docs/99` §9.4
// records that "built" is not "reachable", and an embed surface with no host
// exercising it is exactly that failure.
//
// `docs/126` phase 2 adds the other half: each panel also DRIVES the editor, over
// BOTH transports, from one command list the editor itself hands over. In process
// is `frame.contentWindow.opendoc` — a direct reference, which a host can only
// hold same-origin — and over the wire is `createHostClient`, which is what a
// cross-origin host uses once the deployment names its origin. The point of
// having both on this page is that a reader can watch them answer identically;
// the point of the guard in `host-contract.spec.mjs` is that they must.
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
import { createHostClient } from "./host_client.mjs";

defineOpenDocEditor();

/** The document each embed opens: the shipped sample, because a capability
 *  demonstration on an empty document proves nothing — a reader has to have
 *  something to be refused the editing of. */
const EDITOR_SRC = "./editor.html";

/** The default role. `commentor` rather than `edit`, so the page opens on the
 *  interesting case — a mode that is neither fully open nor fully closed. */
const DEFAULT_ROLE = "commentor";

const panels = [...document.querySelectorAll("[data-embed]")];

for (const panel of panels) {
  wirePanel(panel);
}

// The header strip: DIGITS ONLY, from the authority and from the page's own
// structure. Two of these are facts about `capabilities.mjs` and two are facts
// about this page — how many times it mounts the editor, and how many transports
// each mount offers — counted rather than typed, because a number typed into a
// page is a claim about someone else's code (`docs/99` §9.1). The words beside
// them live in the markup, so this module still puts no English on screen.
setStat("roles", ROLES.length);
setStat("capabilities", CAPABILITIES.length);
setStat("mounts", panels.length);
setStat("transports", document.querySelector("[data-transport]")?.options.length ?? 0);

function setStat(name, value) {
  const node = document.querySelector(`[data-stat="${name}"]`);
  if (node) node.textContent = String(value);
}

/** One panel: a role picker, a mount/release pair, a readout and a stage. Both
 *  panels are the same controller because they answer the same question two
 *  ways; writing it twice is how the two would drift into disagreeing. */
function wirePanel(panel) {
  const kind = panel.dataset.embed;
  const select = panel.querySelector("[data-role-select]");
  const withholdBox = panel.querySelector("[data-withhold]");
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
  reflect();

  for (const control of [select, withholdBox]) {
    control?.addEventListener("change", () => {
      reflect();
      // A live embed is REMOUNTED rather than retuned. Both the mode and the
      // withhold list are resolved before the frame's first navigation, so
      // changing either on a running editor would be a permission decided after
      // the document was already on screen.
      if (stage.querySelector("[data-live]")) mount();
    });
  }

  mountBtn.addEventListener("click", mount);
  releaseBtn.addEventListener("click", release);

  /** The host's two inputs. `can` is null rather than `""` when nothing is
   *  withheld: an empty list and no list are the same decision, and writing the
   *  empty one into a URL or an attribute would teach a host to carry it. */
  function asked() {
    const withheld = withholdBox?.checked ? `-${withholdBox.value}` : null;
    return { mode: select.value, can: withheld };
  }

  /** Paints the readout and the code panel from the CONTROLS, without mounting
   *  anything. */
  function reflect() {
    const request = asked();
    render(panel, resolveFor(request));
    writeSource(panel, kind, request);
  }

  function mount() {
    release();
    const request = asked();
    const live = kind === "element" ? mountElement(request) : mountIframe(request);
    live.dataset.live = "true";
    idle.hidden = true;
    stage.append(live);
    releaseBtn.disabled = false;
    reflect();
    void wireConsole(panel);
  }

  /** The custom element's own path: the host DECLARES the configuration and the
   *  element resolves, sandboxes and reports. `can` is an attribute here, which
   *  is the whole difference from the panel above — `mountIframe` has to build
   *  the same list into a query string by hand. The readout is taken from the
   *  element's event rather than recomputed here: if the page and the element
   *  disagreed, the page would be hiding it. */
  function mountElement({ mode, can }) {
    const element = document.createElement("opendoc-editor");
    element.setAttribute("mode", mode);
    if (can) element.setAttribute("can", can);
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
   *  and the withhold list in the URL before the frame is attached, derive the
   *  sandbox from the capability set, and give the frame an accessible name. */
  function mountIframe(request) {
    const frame = document.createElement("iframe");
    frame.setAttribute("sandbox", resolveFor(request).sandbox.join(" "));
    frame.setAttribute("allow", "clipboard-read; clipboard-write");
    frame.title = panel.dataset.frameTitle ?? "";
    frame.src = editorUrl(request).href;
    return frame;
  }

  function release() {
    releaseConsole(panel);
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

/** The capability set, review mode and sandbox for a host's request, as an embed
 *  would see them: `framed: true`, because every embed on this page is framed.
 *
 *  `can` goes to the same `resolveCapabilities` the element and the editor page
 *  both call, so the readout cannot describe a narrowing the frame did not get. */
function resolveFor({ mode, can }) {
  const capabilities = resolveCapabilities({ mode, framed: true, withhold: can });
  return {
    capabilities,
    editingMode: editingModeFor(capabilities),
    sandbox: sandboxTokensFor(capabilities),
  };
}

/** The URL a hand-wiring host has to build. One function, used both to mount the
 *  raw iframe and to print what that host wrote — a panel showing a URL it did
 *  not navigate to would be the page's own version of the drift it is about. */
function editorUrl({ mode, can }) {
  const url = new URL(EDITOR_SRC, document.baseURI);
  url.searchParams.set("mode", mode);
  if (can) url.searchParams.set("can", can);
  return url;
}

/**
 * Prints the code THIS host writes, for this panel's mounting style.
 *
 * The two panels differ in exactly one thing and it was the hardest thing on the
 * page to see. So each one shows its own source, generated from the same request
 * that mounts it: a URL and a hand-derived sandbox on one side, and the markup on
 * the other. `can="-download"` beside `?can=-download` is the whole argument for
 * the element in two lines.
 *
 * No English: both strings are code, the way the event log is an event name and
 * its own JSON. Complexity: O(1).
 */
function writeSource(panel, kind, request) {
  const out = panel.querySelector("[data-wrote]");
  if (!out) return;
  const title = panel.dataset.frameTitle ?? "";
  if (kind === "element") {
    out.textContent = [
      "<opendoc-editor",
      `  mode="${request.mode}"`,
      ...(request.can ? [`  can="${request.can}"`] : []),
      `  editor-src=${JSON.stringify(EDITOR_SRC)}`,
      `  frame-title=${JSON.stringify(title)}`,
      "></opendoc-editor>",
    ].join("\n");
    return;
  }
  const url = editorUrl(request);
  out.textContent = [
    "<iframe",
    `  src=${JSON.stringify(`${EDITOR_SRC}${url.search}`)}`,
    `  sandbox=${JSON.stringify(resolveFor(request).sandbox.join(" "))}`,
    `  title=${JSON.stringify(title)}`,
    "></iframe>",
  ].join("\n");
}

/** Paints a panel's readout.
 *
 *  Withheld capabilities are LISTED, struck through, not omitted. The same rule
 *  the chrome follows for a disabled control: an absent capability a host cannot
 *  see is a capability they will file a bug about. */
function render(panel, { capabilities, editingMode, sandbox }) {
  panel.querySelector("[data-editing-mode]").textContent = editingMode;
  panel.querySelector("[data-sandbox]").textContent = [...sandbox].join(" ");
  // Digits into the middle of a sentence the MARKUP owns, the same contract the
  // playground's readout holds: "6 of 9 capabilities" is three nodes and this
  // writes the two numbers.
  set(panel, "[data-granted-count]", capabilities.size);
  set(panel, "[data-capability-count]", CAPABILITIES.length);
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

function set(panel, selector, value) {
  const node = panel.querySelector(selector);
  if (node) node.textContent = String(value);
}

// ---- The host contract console (`docs/126` phase 2) ------------------------
//
// One controller for both panels and both transports, for the same reason the
// panels already share one: two copies of "ask the editor to do something" would
// be two things to keep in step, which is the drift this phase exists to remove.

/** panel -> its live `postMessage` client, so releasing an embed disposes it and
 *  a re-mount never talks to a frame that is gone. */
const clients = new WeakMap();

/** The iframe a panel is showing, whichever way it was mounted. The custom
 *  element keeps its frame in a shadow root and exposes it as `.frame`; the
 *  hand-wired panel IS the iframe. */
function liveFrame(panel) {
  const live = panel.querySelector("[data-stage] [data-live]");
  if (!live) return null;
  return live.tagName === "IFRAME" ? live : (live.frame ?? null);
}

/** Waits for the frame's own session to exist.
 *
 *  The editor assigns `window.opendoc` at the end of its module evaluation, so a
 *  host reaching for it the instant the frame is attached finds nothing. That is
 *  not a race to paper over — it is why the contract also has a `ready` event and
 *  a `ping`, and a host that would rather not poll listens for `ready` instead. */
async function waitForSession(frame) {
  for (let attempt = 0; attempt < 400; attempt += 1) {
    const session = frame?.contentWindow?.opendoc ?? null;
    if (session) return session;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  return null;
}

/** Fills the command picker from the editor's OWN description of itself, and
 *  starts logging its events. Nothing about the command list is written on this
 *  page: a command added to the editor appears here with nobody editing it. */
async function wireConsole(panel) {
  const surface = panel.querySelector("[data-console]");
  if (!surface) return;
  const frame = liveFrame(panel);
  if (!(await waitForSession(frame))) return;
  const client = createHostClient({ frame, editorOrigin: window.location.origin });
  clients.set(panel, client);
  const described = await client.describe();
  const events = surface.querySelector("[data-events]");
  // Over the WIRE, deliberately: the in-process session delivers the same events,
  // and taking them from the transport that has further to travel is what shows
  // that it does.
  for (const name of described.events) client.on(name, (event) => logEvent(events, event));
  surface.querySelector("[data-command-select]").replaceChildren(
    ...described.commands.map((command) => {
      const option = document.createElement("option");
      option.value = command.id;
      // The id, not a label: the ids ARE the addressable surface, and giving them
      // labels here would put a second name on every command.
      option.textContent = command.id;
      return option;
    }),
  );
  surface.hidden = false;
}

/** Disposes a panel's client and empties its readouts. */
function releaseConsole(panel) {
  clients.get(panel)?.dispose();
  clients.delete(panel);
  const surface = panel.querySelector("[data-console]");
  if (!surface) return;
  surface.hidden = true;
  surface.querySelector("[data-events]").replaceChildren();
  surface.querySelector("[data-result]").textContent = "";
}

/** One event, newest last, bounded so a long session does not grow without end. */
function logEvent(list, event) {
  const item = document.createElement("li");
  item.dataset.event = event.event;
  // Joined rather than interpolated: the string-site scanner reads a template
  // assigned to `textContent` as a user-facing literal, and this one carries no
  // words at all — an event name and its own JSON.
  item.textContent = [event.event, JSON.stringify(event.detail)].join(" ");
  list.append(item);
  while (list.children.length > 40) list.firstElementChild.remove();
  list.scrollTop = list.scrollHeight;
}

for (const surface of document.querySelectorAll("[data-console]")) {
  const panel = surface.closest("[data-embed]");
  surface.querySelector("[data-run]").addEventListener("click", async () => {
    const id = surface.querySelector("[data-command-select]").value;
    const transport = surface.querySelector("[data-transport]").value;
    const result = await runCommand(panel, transport, id);
    // The RESULT, verbatim. A refusal is a value a host branches on, so the thing
    // to show is the value — not a sentence this page wrote about it.
    surface.querySelector("[data-result]").textContent = JSON.stringify(result, null, 2);
  });
}

/**
 * Runs one command over one transport, and returns exactly what came back.
 *
 * The two branches are the whole demonstration, and they are four lines apart so a
 * reader can see that the only difference between them is how the call travels.
 */
async function runCommand(panel, transport, id) {
  if (transport === "post-message") {
    const client = clients.get(panel);
    return client ? client.execute(id) : null;
  }
  const session = liveFrame(panel)?.contentWindow?.opendoc ?? null;
  return session ? session.execute(id) : null;
}
