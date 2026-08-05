// Examples gallery — memory-safe, click-to-activate live editor demos.
//
// Each card ships a lightweight STATIC poster (a still image + caption) and
// nothing heavier. The real editor is a full WebAssembly app that uses ~100 MB
// per tab, so booting one iframe per card would blow the tab memory budget the
// editor is guarded against (webapp/tests/e2e/memory-budget.spec.mjs) and jank
// the page. Instead we boot exactly ONE live editor at a time: activating a card
// mounts a single <iframe src="editor.html?demo=<name>"> in that card and tears
// down whichever card was previously live (dropping its iframe frees the WASM
// instance, canvases, and JS heap that lived in it). This mirrors how live-demo
// SDK sites keep a gallery interactive without N heavy instances alive at once.

const grid = document.getElementById("exampleGrid");

// The single live card, or null when the whole gallery is static.
let activeCard = null;

// Human-readable labels for the status line / a11y announcements.
const DEMO_LABELS = {
  tables: "Tables",
  changes: "Tracked changes",
  comments: "Comments",
  find: "Find & replace",
  formatting: "Formatting",
  export: "Export",
};

// Tears the given card back down to its static poster, disposing the live
// editor iframe it held (the point of the single-instance rule).
function deactivate(card) {
  if (!card) return;
  const frame = card.querySelector(".example-frame");
  if (frame) {
    // Blank the document first so the WASM tab starts releasing before the
    // element is detached, then remove the node so it can be collected.
    try {
      frame.src = "about:blank";
    } catch {
      /* ignore */
    }
    frame.remove();
  }
  card.classList.remove("is-live");
  const runBtn = card.querySelector(".example-run");
  if (runBtn) runBtn.setAttribute("aria-pressed", "false");
  const controls = card.querySelector(".example-live-controls");
  if (controls) controls.remove();
  if (activeCard === card) activeCard = null;
}

// Mounts the one live editor into `card`, tearing down any previously live card
// first so only a single WASM editor instance is ever alive on the page.
function activate(card) {
  const demo = card.dataset.demo;
  if (!demo) return;

  // Enforce the single-live-instance invariant: drop the previous editor first.
  if (activeCard && activeCard !== card) deactivate(activeCard);
  if (card.classList.contains("is-live")) return; // already live — no double-boot

  const stage = card.querySelector(".example-stage");
  const label = DEMO_LABELS[demo] ?? demo;

  const frame = document.createElement("iframe");
  frame.className = "example-frame";
  frame.title = `Live OpenDoc editor — ${label} demo`;
  frame.loading = "eager";
  // Same-origin editor: allow clipboard + downloads so Export etc. work, but
  // keep the demo sandboxed from top-level navigation.
  frame.setAttribute(
    "allow",
    "clipboard-read; clipboard-write; downloads",
  );
  frame.src = `./editor.html?demo=${encodeURIComponent(demo)}`;
  stage.appendChild(frame);

  // A small live-controls bar with a Close action so activation is reversible
  // (and the single live instance can be dropped back to a static page).
  const controls = document.createElement("div");
  controls.className = "example-live-controls";
  const live = document.createElement("span");
  live.className = "example-live-badge";
  live.innerHTML = '<span class="example-live-dot" aria-hidden="true"></span> Live';
  const spacer = document.createElement("span");
  spacer.className = "example-live-caption";
  spacer.textContent = `${label} · running the real editor`;
  const openBtn = document.createElement("a");
  openBtn.className = "example-live-open";
  openBtn.href = `./editor.html?demo=${encodeURIComponent(demo)}`;
  openBtn.target = "_blank";
  openBtn.rel = "noopener";
  openBtn.innerHTML = 'Open full-screen <span aria-hidden="true">↗</span>';
  const closeBtn = document.createElement("button");
  closeBtn.type = "button";
  closeBtn.className = "example-live-close";
  closeBtn.textContent = "Close";
  closeBtn.setAttribute("aria-label", `Close the ${label} live demo`);
  closeBtn.addEventListener("click", () => {
    deactivate(card);
    const runBtn = card.querySelector(".example-run");
    if (runBtn) runBtn.focus();
  });
  controls.append(live, spacer, openBtn, closeBtn);
  stage.appendChild(controls);

  card.classList.add("is-live");
  const runBtn = card.querySelector(".example-run");
  if (runBtn) runBtn.setAttribute("aria-pressed", "true");
  activeCard = card;
}

// One delegated listener for every run button — keyboard-activatable because
// each control is a real <button>.
grid?.addEventListener("click", (event) => {
  const runBtn = event.target.closest(".example-run");
  if (!runBtn || !grid.contains(runBtn)) return;
  const card = runBtn.closest(".example-card");
  if (card) activate(card);
});
