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
//   `wrapModes()`          `[[value, label]]`, the shared wrap table (in-line first)
//   `arrangeState()`       `{wrappable, positionable, stackable, groupable,
//                           rotatable}` — what is offered and, when it is not,
//                           WHY. Gathered ONCE per menu build by the caller,
//                           never re-derived per row.
//   `activeWrap()`         the wrap chip that reads as pressed ("inline" for an
//                          object in the run flow)
//   `positionPresets()`    `[[id, label, preset]]`, Word's nine cells
//   `zOrderChoices()`      `[[value, label]]`
//   `rotateChoices()`      `[[value, label]]`
//   `text(key)`            one localised string. Deliberately not spelled with
//                          the word the unrouted-string scanner treats as a
//                          human sink, which would count the KEY as English.
//   `applyPosition(preset)` / `setZOrder(v)` / `group()` / `ungroup()`
//   `rotate(v)` / `addText()`
//   `shapeColors()`        the shape fill/outline swatch hexes
//   `objectWrap(root)`     the object's current wrap mode
//   `documentRows()`       the References rows Word puts on an object menu
//                          (Insert caption), derived from the one declaration
//                          that also makes the ribbon button and the palette row
//   `setObjectWrap(v)`     apply a wrap mode
//   `openAltText()`        open the alt-text dialog
//   `chartCommands()`      the selected chart's command tree (`chart_commands.mjs`)
//   `applyShapeFill(hex)`  `null` clears the fill
//   `applyShapeOutline(o)`
//   `enterCrop()`          enter crop mode on a picture
//   `changePicture()`      Word's Change Picture: pick a file for the picture
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

  // Wrap text — a submenu of wrap modes, for EVERY top-level object, exactly
  // like the object chip. Not gated on `canWrap`: the engine sets that bit only
  // for an object that already floats, so gating on it is what hid the control
  // in the one case a user meets first (an inserted picture is inline). "In
  // line" is the first row, as it is in Word's Layout Options and in Docs' image
  // chip, and choosing it rewrites the anchor rather than setting a wrap.
  const arrange = io.arrangeState();
  if (arrange) {
    const active = io.activeWrap();
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
        enabled: mutationEnabled && arrange.wrappable.available,
        disabledReason: mutationEnabled ? arrange.wrappable.reason : mutationReason,
        run: () => io.setObjectWrap(value),
      })),
    });

    // Position — Word's nine-cell gallery, as a submenu here because a
    // right-click menu is a list. Same nine cells, same commit.
    commands.push({
      id: "object.position",
      label: io.text("object.position"),
      group: "arrange",
      icon: "position",
      submenu: io.positionPresets().map(([id, text, preset]) => ({
        id: `object.position.${id}`,
        label: text,
        group: "position",
        enabled: mutationEnabled && arrange.positionable.available,
        disabledReason: mutationEnabled ? arrange.positionable.reason : mutationReason,
        run: () => io.applyPosition(preset),
      })),
    });

    // Arrange — the four stacking commands, then Group and Ungroup. Every
    // disabled row carries the ENGINE's own reason, which is the whole point of
    // `canGroupObjects` answering `{can, reason}` rather than a bare boolean.
    commands.push({
      id: "object.arrange",
      label: io.text("object.arrange"),
      group: "arrange",
      icon: "layers",
      submenu: [
        ...io.zOrderChoices().map(([value, text]) => ({
          id: `object.z.${value}`,
          label: text,
          group: "stack",
          enabled: mutationEnabled && arrange.stackable.enabled,
          disabledReason: mutationEnabled ? arrange.stackable.reason : mutationReason,
          run: () => io.setZOrder(value),
        })),
        {
          id: "object.group",
          label: io.text("object.group"),
          group: "group",
          enabled: mutationEnabled && arrange.groupable.can,
          disabledReason: mutationEnabled ? arrange.groupable.reason : mutationReason,
          run: () => io.group(),
        },
        {
          id: "object.ungroup",
          label: io.text("object.ungroup"),
          group: "group",
          enabled: mutationEnabled && context.kind === "group",
          disabledReason: mutationEnabled ? io.text("object.ungroup.notAGroup") : mutationReason,
          run: () => io.ungroup(),
        },
      ],
    });

    // Rotate — Word's four rows. A top-level text box models no rotation, and
    // the refusal SAYS so rather than the row silently doing nothing.
    commands.push({
      id: "object.rotate",
      label: io.text("object.rotate"),
      group: "arrange",
      icon: "rotate",
      submenu: io.rotateChoices().map(([value, text]) => ({
        id: `object.rotate.${value}`,
        label: text,
        group: "rotate",
        enabled: mutationEnabled && arrange.rotatable,
        disabledReason: mutationEnabled ? io.text("object.rotate.unsupported") : mutationReason,
        run: () => io.rotate(value),
      })),
    });
  }

  // Add text — Word's and Docs' shape command. The double-click is the gesture;
  // this is the surface that tells you the gesture exists.
  if (context.kind === "shape") {
    commands.push({
      id: "object.addText",
      label: io.text("object.addText"),
      group: "arrange",
      icon: "text",
      enabled: mutationEnabled,
      disabledReason: mutationReason,
      run: () => io.addText(),
    });
  }

  // A chart's own commands — Edit data, Type, Elements, Style, Settings — the
  // SAME tree the contextual Chart tab and the settings panel run
  // (`chart_commands.mjs`), so the right-click menu and the ribbon cannot drift.
  // Each row carries its own enablement and reason: the data is readable in
  // Viewing even though nothing in it can change.
  if (context.kind === "chart") {
    commands.push(...(io.chartCommands?.() ?? []));
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
  // only way in. A PICTURE has the outline half, under Word's own name for it,
  // Picture Border (`docs/109` HF-254): the same `a:ln`, the same command.
  const picture = context.kind === "image";
  if ((context.kind === "shape" || picture) && (context.canFill || context.canStroke)) {
    const swatch = (hex) => ({
      id: `object.fill.${hex}`,
      label: hex.toUpperCase(),
      group: "swatch",
      enabled: mutationEnabled,
      disabledReason: mutationReason,
    });
    if (context.canFill && !picture) {
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
        label: picture ? io.text("object.pictureBorder") : "Shape outline",
        group: "arrange",
        icon: "format",
        submenu: [
          {
            id: "object.outline.none",
            // Word's Picture Border menu says "No Outline" too.
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

  // Change Picture — Word's right-click row, Docs' Replace image. A picture
  // only: a chart or an embedded object has no image of its own to swap.
  if (picture && typeof io.changePicture === "function") {
    commands.push({
      id: "object.changePicture",
      label: io.text("object.changePicture.menu"),
      group: "arrange",
      icon: "picture",
      enabled: mutationEnabled,
      disabledReason: mutationReason,
      run: () => io.changePicture(),
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
