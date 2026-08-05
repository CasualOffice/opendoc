// Home hero live-editor embed — memory-safe by construction.
//
// The marketing page must NEVER auto-boot a WASM editor on load: each editor
// tab uses ~100 MB, and the tab-memory budget is guarded end-to-end in
// tests/e2e/memory-budget.spec.mjs. So the hero ships a STATIC styled poster
// and boots exactly ONE live <iframe src="editor.html?demo=1"> only when the
// visitor clicks Run — the same single-instance, click-to-activate pattern the
// examples gallery uses (src/examples.js). Closing it blanks and removes the
// iframe, releasing the WASM instance, canvases, and JS heap it held.

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
