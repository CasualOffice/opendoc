// The picture tools ONLYOFFICE's Image settings panel carries beside Crop:
// Actual Size, Crop to Fill, Crop to Fit and Reset crop (`docs/109` RM-29).
//
// Competitive standard first, per the editing rule:
//
//   * ONLYOFFICE — `web-apps/apps/documenteditor/main/app/view/ImageSettings.js`:
//     `setOriginalSize` (:539) sets the picture to `asc_getCropOriginalImageSize`,
//     the natural size of the part of the image the crop leaves visible; the Crop
//     split button's menu (:666-678) offers Crop, Fill (`asc_cropFill`) and Fit
//     (`asc_cropFit`); `btnResetCrop` is disabled unless the picture is cropped
//     (:462).
//   * Word — Picture Format ▸ Crop ▸ Fill / Fit, and Reset Picture ▸ Reset
//     Picture & Size; the Size dialog's Reset button.
//   * Google Docs — Image options ▸ Size & rotation, and Reset image.
//
// Deliberate differences, stated here as the rule asks:
//
//   * Reset size changes only the size. ONLYOFFICE's Actual Size also zeroes the
//     rotation; Word's Reset Picture & Size also drops the crop and effects. Here
//     each of those has its own command (Rotate ▸ 0°, Reset crop), so no single
//     click undoes more than its name says.
//   * Fill and Fit apply at once as one undoable crop, instead of entering the
//     crop session pre-set. The reader sees the result and Ctrl+Z takes it back;
//     the manual crop grips are still one command away.
//
// The math is pure and the engine calls go through `io`, so the whole command
// policy is testable in node (`tests/picture_tools.test.mjs`).
//
// Cost: building the rows is three engine reads for the one selected picture —
// `pictureNaturalSize` and `objectCrop` each O(document) with one walk, and
// `objectExtent` the same — gathered once per menu build, never per row.

/** A crop edge the engine stores: at most 100% outset (`CROP_MIN`). */
const MIN_EDGE = -1;
/** …and at most 200% inset (`CROP_MAX`). */
const MAX_EDGE = 2;

const clampEdge = (value) => Math.min(MAX_EDGE, Math.max(MIN_EDGE, value));

/** The natural size of the part of the image `crop` leaves visible, in the
 *  units of `natural`. A crop that leaves no positive span (a corrupt or
 *  hand-written file) falls back to the whole image rather than to zero.
 *
 *  @param {[number, number]} natural  the whole image, `[width, height]`
 *  @param {number[] | null | undefined} crop  `[left, top, right, bottom]` fractions
 *  @returns {[number, number]}
 */
export function visibleNaturalSize(natural, crop) {
  const [l, t, r, b] = crop ?? [0, 0, 0, 0];
  const across = 1 - l - r;
  const down = 1 - t - b;
  return [
    Math.round(natural[0] * (across > 0 ? across : 1)),
    Math.round(natural[1] * (down > 0 ? down : 1)),
  ];
}

/** The crop that makes the image COVER `frame` at its own proportions, hiding
 *  the overflow equally on the two long sides — Crop to Fill.
 *
 *  @param {[number, number]} frame    the picture's box, `[width, height]`
 *  @param {[number, number]} natural  the whole image, `[width, height]`
 *  @returns {[number, number, number, number]} `[left, top, right, bottom]`
 */
export function cropToFill(frame, natural) {
  const frameAspect = frame[0] / frame[1];
  const imageAspect = natural[0] / natural[1];
  if (frameAspect > imageAspect) {
    // A wider frame: the whole width shows, the top and bottom are hidden.
    const hidden = (1 - imageAspect / frameAspect) / 2;
    return [0, clampEdge(hidden), 0, clampEdge(hidden)];
  }
  const hidden = (1 - frameAspect / imageAspect) / 2;
  return [clampEdge(hidden), 0, clampEdge(hidden), 0];
}

/** The crop that makes the WHOLE image fit inside `frame` at its own
 *  proportions, the spare space as an equal outset on the two short sides —
 *  Crop to Fit. Outsets are negative crop edges (`a:srcRect` allows them), so
 *  the frame does not move and nothing in the layout reflows.
 *
 *  @param {[number, number]} frame
 *  @param {[number, number]} natural
 *  @returns {[number, number, number, number]}
 */
export function cropToFit(frame, natural) {
  const frameAspect = frame[0] / frame[1];
  const imageAspect = natural[0] / natural[1];
  if (frameAspect > imageAspect) {
    // A wider frame: the height fills, the sides are padded.
    const outset = -(frameAspect / imageAspect - 1) / 2;
    return [clampEdge(outset), 0, clampEdge(outset), 0];
  }
  const outset = -(imageAspect / frameAspect - 1) / 2;
  return [0, clampEdge(outset), 0, clampEdge(outset)];
}

/** Whether `crop` hides or pads anything. */
export function isCropped(crop) {
  return Array.isArray(crop) && crop.some((edge) => Math.abs(edge) > 1e-9);
}

/** What the engine says about the picture `node`, read once.
 *
 *  @param {any} doc
 *  @param {string} node
 *  @returns {{natural: [number, number] | null, crop: number[] | null, frame: [number, number] | null}}
 */
export function readPicture(doc, node) {
  const pair = (value) =>
    Array.isArray(value) || ArrayBuffer.isView(value)
      ? value.length >= 2 && value[0] > 0 && value[1] > 0
        ? [Number(value[0]), Number(value[1])]
        : null
      : null;
  const crop = doc?.objectCrop?.(node);
  return {
    natural: pair(doc?.pictureNaturalSize?.(node)),
    crop: crop ? Array.from(crop) : null,
    frame: pair(doc?.objectExtent?.(node)),
  };
}

/**
 * The four picture-tool rows for the right-click menu (and, through its
 * flattening, the command palette), each enabled or disabled WITH the reason.
 *
 * @param {object} io
 * @param {any} io.doc                          the engine handle
 * @param {string} io.node                      the selected picture
 * @param {boolean} io.mutationEnabled          the review-mode gate
 * @param {string} io.mutationReason            why it is closed, when it is
 * @param {(key: string) => string} io.t
 * @param {(thunk: () => unknown, options: object) => unknown} io.runEdit  the gated edit path
 * @param {(text: string) => void} io.announce  the status line (read by screen readers)
 * @returns {object[]}
 */
export function pictureToolCommands(io) {
  const picture = readPicture(io.doc, io.node);
  const sizeReason = io.t("object.picture.reason.noNaturalSize");
  const known = !!picture.natural && !!picture.frame;
  const gate = (ready, reason) =>
    !io.mutationEnabled
      ? { enabled: false, disabledReason: io.mutationReason }
      : ready
        ? { enabled: true }
        : { enabled: false, disabledReason: reason };
  const apply = (thunk, doneKey) => {
    const result = io.runEdit(thunk, { gate: true });
    Promise.resolve(result).then(
      (ok) => ok !== false && io.announce(io.t(doneKey)),
      () => {},
    );
  };
  const node = io.node;
  return [
    {
      id: "object.cropFill",
      label: io.t("object.picture.cropFill"),
      group: "picture",
      icon: "crop",
      ...gate(known, sizeReason),
      run: () => {
        if (!known) return;
        apply(() => io.doc.setImageCrop(node, cropToFill(picture.frame, picture.natural)), "object.picture.cropFilled");
      },
    },
    {
      id: "object.cropFit",
      label: io.t("object.picture.cropFit"),
      group: "picture",
      icon: "crop",
      ...gate(known, sizeReason),
      run: () => {
        if (!known) return;
        apply(() => io.doc.setImageCrop(node, cropToFit(picture.frame, picture.natural)), "object.picture.cropFitted");
      },
    },
    {
      id: "object.resetCrop",
      label: io.t("object.picture.resetCrop"),
      group: "picture",
      icon: "crop",
      ...gate(isCropped(picture.crop), io.t("object.picture.reason.notCropped")),
      run: () => {
        if (!isCropped(picture.crop)) return;
        apply(() => io.doc.setImageCrop(node, null), "object.picture.cropReset");
      },
    },
    {
      id: "object.resetSize",
      label: io.t("object.picture.resetSize"),
      group: "picture",
      icon: "picture",
      ...gate(!!picture.natural, sizeReason),
      run: () => {
        if (!picture.natural) return;
        const [width, height] = visibleNaturalSize(picture.natural, picture.crop);
        apply(() => io.doc.setObjectExtent(node, width, height), "object.picture.sizeReset");
      },
    },
  ];
}
