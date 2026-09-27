// One contrast sweep, used by every surface that has one.
//
// This started life inside `theme-contrast.spec.mjs` (the editor chrome, both
// themes) and was then COPIED into `embedding-page.spec.mjs`, whose own comment
// admitted the copy and said that a third caller should make it shared. HF-189
// is that third caller — the site pages needed a sweep of their own — so the
// machinery moved here instead of being written a third time. Three
// implementations of one WCAG formula would drift, and the one that drifted
// would still be green.
//
// It is deliberately a MEASUREMENT of the rendered page rather than an audit of
// hex codes, because the defects it exists for were pairing failures: a token
// that is readable on the surface it was chosen against and fails on a quieter
// one also painted behind it. No amount of reading either token alone reveals
// that. It is also deliberately a WALK of every text-bearing element rather than
// a check of a list of known selectors — a sweep that only visits the selectors
// somebody already wrote down cannot find the next one.

/** Contrast ratio of every text-bearing element under `selector`, against the
 *  background actually composited behind it.
 *
 *  Runs in the page (`page.evaluate`), so it is self-contained by construction.
 *
 *  Returns `{ examined, failures }`:
 *    * `examined` — how many elements carried their own text and were measured.
 *      A caller MUST assert a floor on this. Every failure mode of a page — a
 *      404, a stylesheet that did not load, a render that has not happened —
 *      produces a page with little or no text, and a sweep over nothing returns
 *      no failures and reads exactly like a pass.
 *    * `unresolved` — paints whose colours cannot be read out of CSS (an image
 *      fill). A caller MUST assert this is empty. The alternative is a sweep that
 *      quietly walks past an unknown background and scores the text against
 *      whatever happens to be below it, which is how the old version charged
 *      white-on-graphite text with 1.04:1.
 *    * `failures` — one entry per element below its floor, each with the ratio,
 *      the floor it needed, the text, the font size and the nearest classes.
 *
 *  There is deliberately no allow-list parameter. One used to exist, so a caller
 *  could name the components already known to fail and assert only that it had
 *  added none; the list then became the thing the gate protected. A sweep either
 *  finds nothing or names what it found.
 *
 *  @param {{selector: string}} options
 */
export const auditRegion = ({ selector }) => {
  // Colours are resolved by the canvas parser, not by regex. Chrome hands back
  // whatever form the cascade produced — `rgb()`, `color(srgb …)`, `color-mix`,
  // and, mid-transition, `oklab()` — and a regex that assumes one of those reads
  // another's components as RGB. That is not a hypothetical: an earlier draft
  // scraped `oklab(0.49 -0.01 -0.18)` into a near-black colour and reported two
  // confident, entirely fictional failures. Canvas understands every CSS colour
  // syntax there will ever be and answers in sRGB bytes.
  const ctx = document
    .createElement("canvas")
    .getContext("2d", { willReadFrequently: true });
  const parse = (value) => {
    // An unparseable value leaves fillStyle untouched, so a sentinel is the only
    // way to tell "transparent black" from "Chrome rejected this string".
    ctx.fillStyle = "#ff00ff";
    ctx.fillStyle = value;
    if (ctx.fillStyle === "#ff00ff" && !/f0f|ff00ff|magenta/i.test(value)) {
      throw new Error(`unparseable colour: ${value}`);
    }
    ctx.clearRect(0, 0, 1, 1);
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
    return { r, g, b, a: a / 255 };
  };
  const luminance = (c) => {
    const channel = (v) => {
      v /= 255;
      return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b);
  };
  const composite = (fg, bg) => ({
    r: fg.r * fg.a + bg.r * (1 - fg.a),
    g: fg.g * fg.a + bg.g * (1 - fg.a),
    b: fg.b * fg.a + bg.b * (1 - fg.a),
    a: 1,
  });
  const contrast = (a, b) => {
    const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
    return (hi + 0.05) / (lo + 0.05);
  };

  // Every colour serialization a computed `background-image` can contain. Chrome
  // resolves `var()` and `color-mix()` away by computed-value time, so what is
  // left is concrete stops.
  const COLOUR =
    /#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\([^()]*\)|\btransparent\b/g;

  /** Candidate backdrops for `el` — the colours that can end up behind its text.
   *
   *  Plural, because a gradient is a SET of colours and text has to be readable
   *  on all of them. Reading only `backgroundColor` is how this sweep used to
   *  report fiction: `.home-cta-box` is
   *  `linear-gradient(135deg, #111318, #191d26)`, whose `backgroundColor` is
   *  transparent, so the walk sailed past the dark panel to the page's off-white
   *  and charged white heading text with 1.04:1 — a failure that does not exist
   *  and three more like it, on a page that reads perfectly well. A sweep that
   *  invents failures gets its exemption list grown until it stops finding real
   *  ones, so this has to be right before the rest of it means anything.
   *
   *  Returns `{ colours, unresolved }`. `unresolved` names a paint whose colours
   *  cannot be read out of CSS at all — an image fill — because guessing there is
   *  the same mistake in the other direction. */
  const backdrops = (el) => {
    // Top-down: `layers[0]` is painted last (nearest the text).
    const layers = [];
    let unresolved = null;
    for (let n = el; n && n.nodeType === 1; n = n.parentElement) {
      const cs = getComputedStyle(n);
      const own = parse(cs.backgroundColor);
      const image = cs.backgroundImage;
      let stops = [];
      if (image && image !== "none") {
        if (/(^|[\s,])url\(/.test(image) && !unresolved) {
          unresolved = `<${n.tagName.toLowerCase()}.${n.className}> paints ${image.slice(0, 60)}`;
        }
        stops = (image.match(COLOUR) ?? []).map(parse).filter((c) => c.a > 0);
      }
      // A gradient with an opaque stop covers whatever is under it, so it both
      // contributes its stops and ends the walk. One with only translucent stops
      // is a scrim (`.example-poster-scrim`, `.home-card::before`) and is skipped
      // rather than modelled: it never sits behind this site's text, and pretending
      // to composite an unknown coverage area would be a guess.
      const opaqueStops = stops.filter((c) => c.a === 1);
      if (opaqueStops.length) {
        layers.push(opaqueStops);
        break;
      }
      if (own.a > 0) layers.push([own]);
      if (own.a === 1) break;
    }
    const white = { r: 255, g: 255, b: 255, a: 1 };
    const bottom = layers.length ? layers[layers.length - 1] : [white];
    // Only the bottom layer can hold more than one colour: a multi-colour layer
    // is an opaque gradient, and that ends the walk.
    const above = layers.slice(0, Math.max(0, layers.length - 1)).map((layer) => layer[0]);
    const colours = bottom.map((base) => {
      let acc = base.a === 1 ? base : composite(base, white);
      for (let i = above.length - 1; i >= 0; i--) acc = composite(above[i], acc);
      return acc;
    });
    return { colours, unresolved };
  };

  const root = document.querySelector(selector);
  if (!root) throw new Error(`no element matches ${selector}`);
  const failures = [];
  const unresolved = new Set();
  let examined = 0;
  for (const el of root.querySelectorAll("*")) {
    const own = [...el.childNodes]
      .filter((n) => n.nodeType === 3)
      .map((n) => n.textContent.trim())
      .join("");
    if (!own) continue;
    const cs = getComputedStyle(el);
    if (cs.visibility === "hidden" || cs.display === "none") continue;
    const box = el.getBoundingClientRect();
    if (box.width < 1 || box.height < 1) continue;
    // Disabled controls are dimmed deliberately, and AA exempts them.
    const control = el.closest("button,select,input,textarea");
    if (control && control.disabled) continue;

    // An ICON drawn with a glyph is not text. `<span aria-hidden="true">✓</span>`,
    // `●`, `→`, `○` — one non-alphanumeric character, hidden from assistive
    // technology — is a graphical object, and WCAG holds those to 1.4.11's 3:1
    // rather than 1.4.3's 4.5:1. The three conditions are all required, so this
    // cannot reach prose: a word has letters, and real text is not aria-hidden.
    //
    // And when the icon's parent ALSO presents real text of its own, the icon is
    // decoration for a label that says the same thing — the fidelity legend's `●`
    // sits beside "Full", the home page's `✓` beside "Apache-2.0" — so it is
    // exempt outright, by 1.4.11's "not required to understand the content". That
    // redundancy is MEASURED from the DOM, not assumed: an icon whose parent
    // carries no text of its own (`.home-arrow`, alone in its box) stays held to
    // 3:1, which is how its 2.81:1 was found.
    const glyph = own.length <= 2 && !/[\p{L}\p{N}]/u.test(own);
    const icon = glyph && el.getAttribute("aria-hidden") === "true";
    const labelled =
      icon &&
      [...(el.parentElement?.childNodes ?? [])].some(
        (n) => n.nodeType === 3 && n.textContent.trim() !== "",
      );
    if (labelled) continue;

    examined += 1;
    const behind = backdrops(el);
    if (behind.unresolved) unresolved.add(behind.unresolved);
    const ink = parse(cs.color);
    // The worst backdrop this text can land on, not an average of them.
    let ratio = Infinity;
    let bg = behind.colours[0];
    for (const candidate of behind.colours) {
      const r = contrast(composite(ink, candidate), candidate);
      if (r < ratio) {
        ratio = r;
        bg = candidate;
      }
    }
    const size = parseFloat(cs.fontSize);
    const isLarge =
      size >= 24 || (size >= 18.66 && parseInt(cs.fontWeight, 10) >= 700);


    const required = icon || isLarge ? 3 : 4.5;
    if (ratio < required) {
      // The nearest classes in the stylesheet, so a failure can be charged to a
      // component rather than to a paragraph.
      const classes = [el, el.parentElement, el.parentElement?.parentElement]
        .filter(Boolean)
        .flatMap((n) => [...n.classList])
        .join(" ");
      const hex = (c) =>
        "#" + [c.r, c.g, c.b].map((v) => Math.round(v).toString(16).padStart(2, "0")).join("");
      failures.push({
        text: own.slice(0, 40),
        ratio: Number(ratio.toFixed(2)),
        required,
        size,
        classes,
        describe:
          `${ratio.toFixed(2)}:1 (needs ${required}:1) — "${own.slice(0, 32)}" ` +
          `<${el.tagName.toLowerCase()}.${el.className}> at ${size}px ` +
          `(${hex(parse(cs.color))} on ${hex(bg)})`,
      });
    }
  }
  return { examined, failures, unresolved: [...unresolved] };
};
