// SPDX-License-Identifier: Apache-2.0
// The status line's full text, on hover, when the line has to cut it short.
//
// The status bar gives the engine's message its whole width before anything
// else in the left half (`style.css`, `.status`), so at 1280px a 60-character
// message is painted whole. A longer one — a file name, a refusal with its way
// out in it — still has to end in an ellipsis somewhere, and an ellipsis with
// nothing behind it is a sentence the reader cannot finish. So when, and only
// when, the line is truncated, its `title` carries the full text; Word and
// Google Docs both answer a clipped status the same way.
//
// Bound to the DOM rather than to `setStatus`, because the line has more than
// one writer (`main.js`'s `setStatus`, its own timed clear, and
// `status_channel.mjs`'s background progress) and the title has to follow all
// of them. A `MutationObserver` sees every one; a `ResizeObserver` catches the
// other way the answer changes — the window, and with it the half, resizing.
//
// Complexity: O(1) per message and per resize — one layout read of one element.

/** Whether `line` is painting less than its whole text. */
export function isTruncated(line) {
  return line.textContent.trim() !== "" && line.scrollWidth > line.clientWidth + 1;
}

/**
 * Keeps `line`'s `title` equal to its text while it is truncated, and absent
 * otherwise — so a short message does not grow a tooltip that repeats it.
 *
 * Returns the update function (so a caller or a test can force one), or `null`
 * when there is no line to watch.
 *
 * @param {HTMLElement|null} line
 */
export function installStatusLineTitle(line) {
  if (!line) return null;
  const update = () => {
    const text = line.textContent.trim();
    if (isTruncated(line)) {
      if (line.getAttribute("title") !== text) line.setAttribute("title", text);
    } else if (line.hasAttribute("title")) {
      line.removeAttribute("title");
    }
  };
  // Writing `title` is an attribute change, which neither observer watches, so
  // the update cannot feed itself.
  new MutationObserver(update).observe(line, { childList: true, characterData: true, subtree: true });
  if (typeof ResizeObserver === "function") {
    const resize = new ResizeObserver(update);
    resize.observe(line);
    if (line.parentElement) resize.observe(line.parentElement);
  }
  update();
  return update;
}
