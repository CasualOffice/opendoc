// Home hero live-editor embed — memory-safe by construction.
//
// The marketing page must NEVER auto-boot a WASM editor on load: each editor
// tab uses ~100 MB, and the tab-memory budget is guarded end-to-end in
// tests/e2e/memory-budget.spec.mjs. So the hero ships a STATIC styled poster
// and boots exactly ONE live <iframe src="editor.html?demo=1"> only when the
// visitor clicks Run — a single-instance, click-to-activate pattern. Closing it
// blanks and removes the iframe, releasing the WASM instance, canvases, and JS
// heap it held.

const embed = document.getElementById("homeEmbed");

if (embed) {
  const stage = embed.querySelector(".home-embed-stage");
  const runBtn = embed.querySelector(".home-embed-run");
  const SRC = "./editor.html?demo=1";

  function boot() {
    if (embed.classList.contains("is-live")) return; // never double-boot

    const frame = document.createElement("iframe");
    frame.className = "home-embed-frame";
    frame.title = "Live OpenDoc editor — demo document";
    frame.loading = "eager";
    // Same-origin editor: allow clipboard + downloads for a real Save flow, but
    // keep it out of top-level navigation.
    frame.setAttribute("allow", "clipboard-read; clipboard-write; downloads");
    frame.src = SRC;
    stage.appendChild(frame);

    const controls = document.createElement("div");
    controls.className = "home-embed-live-controls";

    const badge = document.createElement("span");
    badge.className = "home-embed-live-badge";
    badge.innerHTML = '<span class="home-embed-live-dot" aria-hidden="true"></span> Live';

    const caption = document.createElement("span");
    caption.className = "home-embed-live-caption";
    caption.textContent =
      "Running the real Rust + WebAssembly engine — your document stays in the browser";

    const open = document.createElement("a");
    open.className = "home-embed-live-open";
    open.href = SRC;
    open.target = "_blank";
    open.rel = "noopener";
    open.innerHTML = 'Full-screen <span aria-hidden="true">↗</span>';

    const close = document.createElement("button");
    close.type = "button";
    close.className = "home-embed-live-close";
    close.textContent = "Close";
    close.setAttribute("aria-label", "Close the live editor demo");
    close.addEventListener("click", teardown);

    controls.append(badge, caption, open, close);
    embed.appendChild(controls);

    embed.classList.add("is-live");
    if (runBtn) runBtn.setAttribute("aria-pressed", "true");
  }

  function teardown() {
    const frame = stage.querySelector(".home-embed-frame");
    if (frame) {
      // Blank first so the WASM tab starts releasing before detachment.
      try {
        frame.src = "about:blank";
      } catch {
        /* ignore */
      }
      frame.remove();
    }
    const controls = embed.querySelector(".home-embed-live-controls");
    if (controls) controls.remove();
    embed.classList.remove("is-live");
    if (runBtn) {
      runBtn.setAttribute("aria-pressed", "false");
      runBtn.focus();
    }
  }

  runBtn?.addEventListener("click", boot);
}

// Quickstart tabs — "shell" and "embed". Both panels are in the markup and are
// shown or hidden, rather than swapped in through innerHTML the way the design
// prototype did it: the embed snippet then stays greppable, which is how
// tests/site_claims.test.mjs holds it to the engine's real exports. Arrow keys
// rove between tabs, as the WAI-ARIA tabs pattern requires.
const quickstartTabs = [...document.querySelectorAll('.home-terminal-tabs [role="tab"]')];

function selectQuickstartTab(tab) {
  for (const other of quickstartTabs) {
    const selected = other === tab;
    other.setAttribute("aria-selected", String(selected));
    other.tabIndex = selected ? 0 : -1;
    const panel = document.getElementById(other.getAttribute("aria-controls"));
    if (panel) panel.hidden = !selected;
  }
}

for (const tab of quickstartTabs) {
  tab.addEventListener("click", () => selectQuickstartTab(tab));
  tab.addEventListener("keydown", (event) => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const at = quickstartTabs.indexOf(tab);
    const last = quickstartTabs.length - 1;
    const next =
      event.key === "Home" ? 0
      : event.key === "End" ? last
      : event.key === "ArrowRight" ? (at === last ? 0 : at + 1)
      : at === 0 ? last : at - 1;
    selectQuickstartTab(quickstartTabs[next]);
    quickstartTabs[next].focus();
  });
}
