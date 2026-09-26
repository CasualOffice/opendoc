// The right-click menu for a SELECTED OBJECT (picture, shape, text box).
//
// The object counterpart to `buildContextCommands`: it emits object commands
// (Insert caption / Wrap / Alt text / Crop / Delete) instead of paragraph-text
// ones, reusing the exact same functions the floating object context bar wires
// up. Mutations are disabled — with the object review-mode reason — in Viewing
// and Suggesting, mirroring how the text menu greys its structural rows; the
// underlying functions still gate fail-closed, so the menu can never bypass a
// review mode.
//
// Extracted from `main.js` to pay for OO-005 under the line ratchet, and the
// extraction is worth more than the lines it bought: the builder returns plain
// command descriptors and reaches the application only through `io`, so it needs
// no DOM and `tests/object_context_menu.test.mjs` can assert what the menu
// offers — per object kind, per review mode — in node. 156 lines of menu policy
// had no test of any kind before this.
//
// `io` is its whole contact with the application:
//
//   `reviewMode()`         "editing" | "suggesting" | "viewing"
//   `readOnlyReason()`     why the DOCUMENT itself refuses writes, or ""
//   `wrapModes()`          `[[value, label]]`, the shared WRAP_MODES table
//   `shapeColors()`        the shape fill/outline swatch hexes
//   `objectWrap(root)`     the object's current wrap mode
//   `documentRows()`       the References rows Word puts on an object menu
//                          (Insert caption), derived from the one declaration
//                          that also makes the ribbon button and the palette row
//   `setObjectWrap(v)`     apply a wrap mode
//   `openAltText()`        open the alt-text dialog
//   `applyShapeFill(hex)`  `null` clears the fill
//   `applyShapeOutline(o)`
//   `enterCrop()`          enter crop mode on a picture
//   `openProperties()`     open the object inspector
//   `deleteObject()`       delete the selected object
export function buildObjectContextCommands(context, io) {
  // Object edits are untrackable, so they are read-only in Viewing and blocked
  // (untracked) in Suggesting — the same gate `runEdit({ gate:true })` applies.
  const mutationEnabled = io.reviewMode() === "editing";
  const mutationReason =
    // Same rule as `blockMutationInViewing`: "turn on Editing" is advice the
    // reader cannot act on when the DOCUMENT is the thing that is read-only.
    io.readOnlyReason() ||
    (io.reviewMode() === "viewing"
      ? "Turn on Editing to change this object"
      : "Object changes cannot be tracked in Suggesting mode");
  const commands = [];

  // Insert caption — FIRST, because it is the only row here that is about the
  // document rather than about the object's own geometry, and because it is the
  // reason a reader right-clicks a figure. ONLYOFFICE puts it at the top of the
  // picture, table and equation menus for the same reason
  // (`DocumentHolderExt.js:46`, `menuInsertCaption`, ahead of its own
  // separator). Word's picture menu carries it too.
  //
  // The ROW itself is handed in, not written here: it is the same declaration
  // the ribbon button and the palette row come from, and three copies of one
  // label is how a menu comes to call a command something the ribbon does not.
  // The review-mode gate is applied on top, because "this object cannot be
  // changed" is this menu's answer and not the ribbon's.
  for (const row of io.documentRows()) {
    commands.push({
      ...row,
      enabled: mutationEnabled && row.enabled !== false,
      disabledReason: mutationEnabled ? row.disabledReason : mutationReason,
    });
  }

  // Wrap text — a submenu of wrap modes, only for a floating (anchored) object,
  // exactly like the context bar. The active mode is checked on the right.
  if (context.canWrap) {
    const active = io.objectWrap(context.ref.root);
    commands.push({
      id: "object.wrap",
      label: "Wrap text",
      group: "arrange",
      icon: "wrap",
      submenu: io.wrapModes().map(([value, text]) => ({
        id: `object.wrap.${value}`,
        label: text,
        group: "wrap",
        shortcut: value === active ? "✓" : "",
        enabled: mutationEnabled,
        disabledReason: mutationReason,
        run: () => io.setObjectWrap(value),
      })),
    });
  }

  // Alt text — opens the shared alt-text dialog (its Apply pre-checks the gate).
  if (context.canAltText) {
    commands.push({
      id: "object.altText",
      label: "Alt text…",
      group: "arrange",
      icon: "altText",
      enabled: mutationEnabled,
      disabledReason: mutationReason,
      run: () => io.openAltText(),
    });
  }

  // Shape Fill / Shape Outline — the two live controls of Word's Shape Format
  // tab, reachable from the menu as well as the bar so neither surface is the
  // only way in.
  if (context.kind === "shape" && (context.canFill || context.canStroke)) {
    const swatch = (hex) => ({
      id: `object.fill.${hex}`,
      label: hex.toUpperCase(),
      group: "swatch",
      enabled: mutationEnabled,
      disabledReason: mutationReason,
    });
    if (context.canFill) {
      commands.push({
        id: "object.fill",
        label: "Shape fill",
        group: "arrange",
        icon: "format",
        submenu: [
          {
            id: "object.fill.none",
            label: "No fill",
            group: "reset",
            enabled: mutationEnabled,
            disabledReason: mutationReason,
            run: () => io.applyShapeFill(null),
          },
          ...io.shapeColors().map((hex) => ({
            ...swatch(hex),
            run: () => io.applyShapeFill(hex),
          })),
        ],
      });
    }
    if (context.canStroke) {
      commands.push({
        id: "object.outline",
        label: "Shape outline",
        group: "arrange",
        icon: "format",
        submenu: [
          {
            id: "object.outline.none",
            label: "No outline",
            group: "reset",
            enabled: mutationEnabled,
            disabledReason: mutationReason,
            run: () => io.applyShapeOutline({ color: null }),
          },
          ...io.shapeColors().map((hex) => ({
            ...swatch(hex),
            id: `object.outline.${hex}`,
            run: () => io.applyShapeOutline({ color: hex }),
          })),
        ],
      });
    }
  }

  // Crop — picture-only; a text box has no source rectangle to crop.
  if (context.canCrop) {
    commands.push({
      id: "object.crop",
      label: "Crop image",
      group: "arrange",
      icon: "crop",
      enabled: mutationEnabled,
      disabledReason: mutationReason,
      run: () => io.enterCrop(),
    });
  }

  // Properties — Word's "Size and Position…", Docs' "All image options". It had
  // exactly ONE route in the whole product: a button on the floating bar, which
  // a keyboard user cannot reach because Tab is bound to object traversal while
  // an object is selected. Adding it here puts it on the right-click menu and,
  // through the palette flattening in `editorCommands`, on the palette too.
  //
  // Not gated on `mutationEnabled`: reading an object's exact geometry is useful
  // in Viewing and Suggesting, and the panel's own Apply buttons already refuse
  // the write.
  commands.push({
    id: "object.properties",
    label: "Properties…",
    group: "arrange",
    icon: "tune",
    enabled: true,
    run: () => io.openProperties(),
  });

  // Delete — the destructive action, kept in its own trailing group.
  if (context.canDelete) {
    commands.push({
      id: "object.delete",
      label: "Delete",
      group: "delete",
      icon: "delete",
      danger: true,
      enabled: mutationEnabled,
      disabledReason: mutationReason,
      run: () => io.deleteObject(),
    });
  }
  return commands;
}
