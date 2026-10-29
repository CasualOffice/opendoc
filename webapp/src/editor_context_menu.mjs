// The context-aware editor menu: what a right-click on text, a table, a
// revision or an object offers, built from the command registry. Moved out of
// `main.js` unchanged (docs/177); it reaches the rest of the editor only
// through the context `ed`.
import { buildObjectContextCommands } from "./object_context_menu.mjs";
import { pictureToolCommands } from "./picture_tools.mjs";
import { createObjectAlign } from "./object_align.mjs";
import { outlineStyleCommands, readOutlineStyle } from "./shape_outline_style.mjs";
import { gradientCommands } from "./shape_gradient.mjs";
import { editRefusalMessage } from "./edit_errors.mjs";
import { formatShortcut } from "./keyboard.mjs";
import { spellingContextCommands } from "./spell_check.mjs";
import { shortcutForCommand } from "./keymap.mjs";
import { POSITION_PRESETS, ROTATE_CHOICES, WRAP_CHOICES, Z_ORDER_CHOICES, activeWrapChoice, positionAvailability, wrapAvailability } from "./object_arrange.mjs";
import { t } from "./i18n.mjs";
import { followExternalTarget } from "./link_targets.mjs";
import { tableToolCommands as buildTableToolCommands } from "./table_commands.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createContextMenu(ed) {
  // ---- Context-aware editor menu ---------------------------------------------
  // One surface serves prose, links, review ranges, lists, and tables. Commands
  // call the same transaction-backed actions as the ribbon and palette.
  const editorContextMenu = document.createElement("div");
  editorContextMenu.className = "context-menu editor-context-menu";
  editorContextMenu.hidden = true;
  editorContextMenu.setAttribute("role", "menu");
  editorContextMenu.setAttribute("aria-label", "Editor commands");
  document.body.appendChild(editorContextMenu);


  function selectionContainsClientPoint(clientX, clientY) {
    if (!ed.doc || !ed.hasRange()) return false;
    const rects = ed.doc.selectionRects(
      ed.selection.anchor.node,
      ed.selection.anchor.offset,
      ed.selection.focus.node,
      ed.selection.focus.offset,
    );
    for (let i = 0; i + 4 < rects.length; i += 5) {
      const [pageNumber, x, y, width, height] = rects.slice(i, i + 5);
      const page = ed.pages[pageNumber - 1];
      if (!page) continue;
      const { rect, sx, sy } = ed.scaleOf(page);
      if (
        clientX >= rect.left + x * sx &&
        clientX <= rect.left + (x + width) * sx &&
        clientY >= rect.top + y * sy &&
        clientY <= rect.top + (y + height) * sy
      ) {
        return true;
      }
    }
    return false;
  }


  function anchorInsideRange(anchor, range) {
    return (
      anchor &&
      range &&
      range.startNode === anchor.node &&
      range.endNode === anchor.node &&
      anchor.offset >= range.startOffset &&
      anchor.offset <= range.endOffset
    );
  }

  function reviewContextAt(anchor) {
    if (!ed.doc || !anchor) return { comment: null, revision: null };
    const summary = ed.readReviewData(ed.doc);
    const comment = (summary.comments ?? []).find((item) =>
      item.anchor?.node === anchor.node &&
      anchor.offset >= (Number(item.anchor.start) || 0) &&
      anchor.offset <= (Number(item.anchor.end) || Number(item.anchor.start) || 0),
    ) ?? null;
    // The most specific revision at the caret wins. A paragraph formatting change
    // spans its whole paragraph and lists first, so a plain `find` resolved a caret
    // inside an inline insertion to the paragraph change instead, and Accept at the
    // caret decided the wrong suggestion (docs/108). Inline revisions first, then
    // the paragraph mark, then the paragraph formatting change.
    const specificity = (item) =>
      item.kind === "paragraph_format" ? 2 : item.kind?.startsWith("paragraph_mark_") ? 1 : 0;
    const revision = (summary.revisions ?? [])
      .filter((item) => anchorInsideRange(anchor, ed.revisionRange(item)))
      .sort((a, b) => specificity(a) - specificity(b))[0] ?? null;
    return { comment, revision };
  }

  function plainTableInfo(node) {
    if (!ed.doc?.inTable(node)) return null;
    const info = ed.doc.tableInfo(node);
    const value = info?.found ? {
      found: true,
      regular: info.regular,
      merged: info.merged,
      rowHeightRule: info.rowHeightRule,
      table: info.table,
      column: info.column,
    } : null;
    info?.free();
    return value;
  }

  // 0-based index of the column containing the caret, or -1 when the node is not
  // inside a table. Used so table sort keys off the caret's own column (Docs/Word
  // standard) rather than always the first column.
  function caretTableColumn(node) {
    if (!ed.doc?.inTable(node)) return -1;
    const info = ed.doc.tableInfo(node);
    const column = info?.found ? info.column : -1;
    info?.free();
    return column;
  }

  function contextAt(anchor, link = null) {
    const review = reviewContextAt(anchor);
    return {
      surface: "context",
      anchor,
      link,
      comment: review.comment,
      revision: review.revision,
      table: plainTableInfo(anchor.node),
      listKind: ed.doc.listStyleAt(anchor.node),
      hasRange: ed.hasRange(),
      misspelling: ed.spellChecker.misspellingAt(anchor),
      sameParagraphRange:
        ed.hasRange() && ed.selection.anchor.node === ed.selection.focus.node,
      suggesting: ed.reviewMode === "suggesting",
    };
  }

  function decideContextRevision(revision, accept) {
    if (!revision) return;
    return ed.runEdit(() =>
      revision.movePair?.fromStart && revision.movePair?.toStart
        ? ed.doc.decideMovePair(
          revision.movePair.fromStart,
          revision.movePair.toStart,
          accept,
        )
        : revision.groupId
          ? ed.doc.decideRevisionGroup(revision.groupId, accept)
          : ed.doc.decideRevision(revision.id, accept),
    );
  }

  /** Selects a whole row, column or table around `node` — the command twin of a
   *  click on the gutter strip. The endpoints come from the engine's own anchor
   *  list, so the menu route and the pointer route build the same rectangle
   *  (`table_range.mjs`). A mode the table cannot express — a column of a merged
   *  table — refuses with the catalogue's sentence instead of painting nothing. */
  function selectTableContext(node, mode) {
    const refusal = ed.tableRange.selectMode(node, mode);
    ed.drawSelection();
    if (refusal) ed.setStatus(t(refusal), "warn");
    else ed.tableRange.announce();
    ed.focusEditorSurface();
  }

  function editContextLink(link) {
    if (!link || link.startNode !== link.endNode) return;
    ed.selection = {
      anchor: { node: link.startNode, offset: link.startOffset },
      focus: { node: link.endNode, offset: link.endOffset },
    };
    ed.drawSelection();
    const text = ed.doc.copyText(link.startNode, link.startOffset, link.endNode, link.endOffset);
    ed.openLinkDialog({ node: link.startNode, start: link.startOffset, end: link.endOffset, link, text });
  }

  function removeContextLink(link) {
    if (!link || link.startNode !== link.endNode) return;
    ed.selection = {
      anchor: { node: link.startNode, offset: link.startOffset },
      focus: { node: link.endNode, offset: link.endOffset },
    };
    ed.drawSelection();
    ed.runToolbarEdit(() =>
      ed.doc.removeHyperlink(link.startNode, link.startOffset, link.endOffset),
    );
  }

  function openContextLink(link) {
    if (!link) return;
    if (link.url) {
      followExternalTarget(link.url, ed.setStatus);
      return;
    }
    if (link.targetNode != null) {
      ed.selection = {
        anchor: { node: link.targetNode, offset: link.targetOffset ?? 0 },
        focus: { node: link.targetNode, offset: link.targetOffset ?? 0 },
      };
      ed.drawSelection();
      ed.scrollCaretIntoView("center");
      ed.focusEditorSurface();
    }
  }

  // Builds the right-click menu as a short, grouped, contextual set. Primary
  // actions (clipboard, link, comment, review decisions) stay at the top level;
  // the long tail (text styling, list/indent, table row/column operations) is
  // tucked into submenus so no single target dumps a 30-row list. Every entry
  // still routes through the same transaction-backed actions as the ribbon and
  // command palette, so availability and mutation gates never drift.
  // The table command tree (`table_commands.mjs`, which says why it is shared by the menu
  // and the palette) gets the editor's bindings here, each read at CALL time.
  const TABLE_COMMAND_HOST = {
    doc: () => ed.doc,
    tableSelection: () => ed.tableRange.descriptor(),
    clearTableSelection: () => ed.tableRange.clear(),
    status: (text) => ed.setStatus(text),
    plainTableInfo,
    runEdit: (thunk, options) => ed.runEdit(thunk, options),
    selectTableContext,
    openSplitCellDialog: () => ed.toggleSplitCellDialog(true),
    // `focus` names a control INSIDE the popover to land on, which is how
    // `table.borderStyle` reaches the line-style pen from the Table menu and the
    // palette without a second copy of the control: the same shape `layout.indent`
    // uses to reach the indent field. Focus is taken after the click, because the
    // popover is hidden until then and `.focus()` on a hidden element is a no-op.
    openCellFormat: (focus) => {
      ed.selectRibbonTab("table");
      ed.tableBtn.click();
      if (focus) document.getElementById(focus)?.focus();
    },
    openTableProperties: () => ed.toggleTableProperties(true),
    stepTableBand: (axis, sign) => ed.tableChrome.stepCaretBand(ed.selection?.focus?.node, ed.pages, axis, sign),
    moveTableBand: (axis, sign) => ed.tableGutter.moveBand(ed.selection?.focus?.node, axis, sign),
  };

  const tableToolCommands = (context) => buildTableToolCommands(context, TABLE_COMMAND_HOST);

  function buildContextCommands(context) {
    // Right-clicking a selected drawing/image/text box shows OBJECT commands, not
    // the paragraph-text menu (docs/85 §4.1; Word/Google Docs image menu). The
    // object was already selected by the handler that resolved this context.
    if (context.surface === "object") return buildObjectContextCommands(context, objectContextMenuHost);
    const registry = new Map(ed.editorCommands(context).map((command) => [command.id, command]));
    const pick = (id, extra = {}) => {
      const base = registry.get(id);
      return base ? { ...base, ...extra } : null;
    };
    const structuralEnabled = !context.suggesting;
    const structuralReason = structuralEnabled
      ? ""
      : "This structural change cannot be tracked in Suggesting mode";
    const inTable = !!context.table;
    const commands = [];

    // 0 — Spelling, ABOVE the clipboard block. Word and Docs both put the
    // suggestions first because that is the reason the user right-clicked a
    // squiggled word; anything else at the top makes them read past it. The rows
    // themselves are declared in `spell_check.mjs` so the "no suggestions" case
    // cannot be dropped on the way here.
    if (context.misspelling) {
      commands.push(
        ...spellingContextCommands(
          context.misspelling,
          ed.spellChecker.suggestions(context.misspelling),
          {
            replace: ed.replaceMisspelling,
            ignoreOnce: (flagged) => ed.spellChecker.ignoreOnce(flagged),
            ignoreAll: (word) => ed.spellChecker.ignoreAll(word),
            ignoreRule: (rule) => ed.spellChecker.ignoreRule(rule),
            addToDictionary: (word) => void ed.addWordToDictionary(word),
          },
          ed.reviewMode === "viewing"
            ? "Switch to Editing mode to correct the spelling"
            : "",
        ),
      );
    }

    // 1 — The universal primary actions: history, then clipboard, in the order
    // every text field on the platform offers them.
    //
    // Membership is READ from the command's own `contextMenu: true` declaration
    // rather than hand-picked here. Seven commands declared the flag and this
    // function looked at none of them, so "Paste without formatting" — a top-five
    // action in both Word and Docs — and "Select all" were declared for the
    // right-click menu and absent from it (docs/104 HF-076). A capability that is
    // reachable from one surface only is this repo's recurring defect; the cure is
    // one declaration feeding every surface instead of three hand-wired copies.
    const CONTEXT_MENU_ICONS = { "edit.cut": "cut", "edit.copy": "copy", "edit.paste": "paste" };
    commands.push(
      ...[...registry.values()]
        .filter((command) => command.contextMenu)
        .map((command) => ({
          ...command,
          icon: CONTEXT_MENU_ICONS[command.id],
          // The palette's own grouping decides the divider: "Edit" (undo/redo)
          // and "Clipboard" become two blocks, as in the platform's text menus.
          group: command.group === "Clipboard" ? "clipboard" : "history",
        })),
    );

    // 2 — Review decisions: only over a tracked change, kept near the top since
    // they are the most specific thing a right-click can land on.
    if (context.revision) {
      commands.push(
        {
          id: "review.accept",
          label: "Accept suggestion",
          group: "review",
          icon: "accept",
          run: () => decideContextRevision(context.revision, true),
        },
        {
          id: "review.reject",
          label: "Reject suggestion",
          group: "review",
          icon: "reject",
          danger: true,
          run: () => decideContextRevision(context.revision, false),
        },
      );
    }

    // Annotations (link + comment) — contextual to the text/selection under the
    // pointer. Assembled here, placed after the primary group for prose and after
    // the table tools inside a cell.
    const annotate = [];
    // A contents entry the author's field never hyperlinked. Beside Open link
    // because it IS Open link for that case, reached by the same gesture — a
    // right-click on the entry. O(1); see `toc_navigation.mjs`.
    if (!context.link && ed.tocEntryTargetAt(ed.selection?.focus?.node)) {
      annotate.push({
        id: "reference.goToHeading",
        label: "Go to the heading this entry names",
        group: "annotate",
        icon: "linkOpen",
        run: () => void ed.followTocEntry(ed.selection?.focus?.node),
      });
    }
    if (context.link) {
      const linkReason = context.suggesting
        ? "Link changes cannot be tracked in Suggesting mode"
        : "";
      if (context.link.url || context.link.targetNode != null) {
        annotate.push({
          id: "link.open",
          label: "Open link",
          group: "annotate",
          icon: "linkOpen",
          run: () => openContextLink(context.link),
        });
      }
      annotate.push(
        {
          id: "link.edit",
          label: "Edit link…",
          group: "annotate",
          icon: "link",
          enabled: !context.suggesting,
          disabledReason: linkReason,
          run: () => editContextLink(context.link),
        },
        {
          id: "link.remove",
          label: "Remove link",
          group: "annotate",
          enabled: !context.suggesting,
          disabledReason: linkReason,
          run: () => removeContextLink(context.link),
        },
      );
    } else {
      annotate.push({
        id: "link.add",
        label: "Add link…",
        group: "annotate",
        icon: "link",
        // The same capability as `insert.link` under the annotate surface's own id,
        // so it reads its chord from the one table rather than restating it.
        shortcut: shortcutForCommand("insert.link", ed.EDITOR_KEYBOARD_PLATFORM),
        enabled: context.sameParagraphRange && !context.suggesting,
        disabledReason: context.suggesting
          ? "Link changes cannot be tracked in Suggesting mode"
          : context.hasRange
            ? "Links must stay within one paragraph"
            : t("insert.reason.linkNeedsText"),
        run: () => ed.editSelectionLink(),
      });
    }
    if (context.comment) {
      annotate.push({
        id: "comment.open",
        label: "Open comment",
        group: "annotate",
        icon: "comment",
        run: () => ed.focusReviewComment(context.comment),
      });
    } else {
      annotate.push({
        id: "comment.add",
        label: "Add comment",
        group: "annotate",
        icon: "comment",
        shortcut: shortcutForCommand("review.comment", ed.EDITOR_KEYBOARD_PLATFORM),
        enabled: context.hasRange,
        disabledReason: "Select text to add a comment",
        run: () => ed.openReviewComposer(),
      });
    }

    // Text styling, list/indentation, and the paragraph dialog. Shared building
    // blocks: on prose they are three top-level rows; inside a table cell they
    // collapse into a single trailing "Format ▸" submenu so the table actions
    // lead and the cell menu stays compact.
    const formatSubmenu = [
      pick("format.bold", { group: "style" }),
      pick("format.italic", { group: "style" }),
      pick("format.underline", { group: "style" }),
      pick("format.strike", { group: "style" }),
      pick("format.superscript", { group: "script" }),
      pick("format.subscript", { group: "script" }),
      pick("format.clear", { group: "clear" }),
    ].filter(Boolean);
    const listSubmenu = [
      {
        id: "paragraph.bullets",
        label: context.listKind === "bullet" ? "Remove bullets" : "Bulleted list",
        group: "list",
        enabled: structuralEnabled,
        disabledReason: structuralReason,
        run: () => ed.runToolbarEdit((a, b, c, d) => ed.doc.toggleList(a, b, c, d, "bullet"), { paragraphLevel: true }),
      },
      {
        id: "paragraph.numbering",
        label: context.listKind === "numbered" ? "Remove numbering" : "Numbered list",
        group: "list",
        enabled: structuralEnabled,
        disabledReason: structuralReason,
        run: () => ed.runToolbarEdit((a, b, c, d) => ed.doc.toggleList(a, b, c, d, "numbered"), { paragraphLevel: true }),
      },
      {
        // The engine has had checklists as long as the ribbon button has, but the
        // list submenu offered only Bulleted and Numbered — a user browsing the
        // menus concluded the editor had none (docs/104 HF-076).
        id: "paragraph.list.checklist",
        label: context.listKind === "checklist" ? "Remove checklist" : "Checklist",
        group: "list",
        enabled: structuralEnabled,
        disabledReason: structuralReason,
        run: () => ed.toggleChecklistCommand(),
      },
      {
        id: "paragraph.restart",
        label: "Restart numbering",
        group: "list",
        visible: context.listKind === "numbered",
        enabled: structuralEnabled,
        disabledReason: structuralReason,
        run: () => ed.runNodeEdit(() => ed.doc.restartList(context.anchor.node)),
      },
      {
        id: "paragraph.continue",
        label: "Continue numbering",
        group: "list",
        visible: context.listKind === "numbered" && ed.doc.canContinueList(context.anchor.node),
        enabled: structuralEnabled,
        disabledReason: structuralReason,
        run: () => ed.runNodeEdit(() => ed.doc.continueList(context.anchor.node)),
      },
      {
        id: "paragraph.indent.increase",
        label: t("panelHome.increaseIndent.label"),
        group: "indent",
        enabled: structuralEnabled,
        disabledReason: structuralReason,
        run: () => ed.adjustIndentCommand(360),
      },
      {
        id: "paragraph.indent.decrease",
        label: t("panelHome.decreaseIndent.label"),
        group: "indent",
        enabled: structuralEnabled,
        disabledReason: structuralReason,
        run: () => ed.adjustIndentCommand(-360),
      },
    ];
    const paragraphProperties = {
      id: "paragraph.properties",
      label: "Paragraph properties…",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => ed.toggleParagraphProperties(true),
    };

    // A container that may NEVER change the document gets reading rows only: copy,
    // select all, search, and the two ways of following a link. Not the editing menu
    // greyed out — "never, for you" is composition and is silent (`docs/126`) — and
    // the owner's report was exactly this: a read-only container still offering a
    // table's structure commands and an image's properties. The review MODE is the
    // other question, and still says "not right now" with a reason.
    if (!ed.containerCanEdit) {
      return [
        ...commands.filter((row) => row.id === "edit.copy" || row.id === "edit.selectAll"),
        pick("edit.find", { group: "clipboard" }),
        ...annotate.filter((row) => row.id === "link.open" || row.id === "reference.goToHeading"),
      ].filter(Boolean);
    }

    if (!inTable) {
      // Prose menu: annotations, then the text-arrangement rows.
      commands.push(...annotate);
      commands.push({
        id: "format.menu",
        label: "Format text",
        group: "arrange",
        icon: "format",
        submenu: formatSubmenu,
      });
      commands.push({
        id: "paragraph.list",
        label: "List & indentation",
        group: "arrange",
        icon: "list",
        submenu: listSubmenu,
      });
      commands.push({ ...paragraphProperties, group: "arrange", icon: "paragraph" });
      return commands.filter(Boolean);
    }

    // Insert caption, on the TABLE menu as well as the picture menu: a table is
    // the second thing Word captions, and right-clicking it is how a reader
    // reaches it (`DocumentHolderExt.js:46`).
    commands.push(...ed.referenceObjectMenuRows());

    // 3 — Table cell: lead with the table tools (Insert / Delete / Merge / Split,
    // then Select / Autofit & sort, then the property dialogs), matching Word's
    // and Google Docs' table menus. The generic text-format rows are demoted to a
    // single trailing "Format ▸" submenu.
    commands.push(...tableToolCommands(context));

    // Annotations sit below the table tools, then the demoted text-format submenu.
    commands.push(...annotate);
    commands.push({
      id: "format.menu",
      label: "Format",
      group: "format",
      icon: "format",
      submenu: [
        ...formatSubmenu.map((entry) => ({ ...entry, group: "type" })),
        ...listSubmenu.map((entry) => ({ ...entry, group: "list" })),
        { ...paragraphProperties, group: "para" },
      ],
    });
    return commands.filter(Boolean);
  }

  // The object right-click menu's ROWS are `object_context_menu.mjs`; this is the
  // application state and the verbs it needs. Extracted to pay for the captions
  // work under the line ratchet, and to give 156 lines of menu policy its first
  // test (`tests/object_context_menu.test.mjs`).
  /** Arrange ▸ Align and Distribute (`object_align.mjs`, RM-28). One instance,
   *  so the target the reader picks stays chosen, as Word's does. */
  const objectAlign = createObjectAlign({
    doc: () => ed.doc,
    nodes: () => ed.objectArrange.candidates(),
    runEdit: (thunk, options) => ed.runEdit(thunk, options),
    t,
    explain: (raw) => editRefusalMessage(raw, { routeRefusal: ed.SESSION.sentenceFor }),
    announce: (text) => ed.setStatus(text),
    modifier: formatShortcut("⌘"),
  });
  const objectContextMenuHost = {
    reviewMode: () => ed.reviewMode,
    alignRows: (mutationEnabled, mutationReason) => objectAlign.rows(mutationEnabled, mutationReason),
    changeShape: (token) => {
      const node = ed.objectSelection?.node;
      if (!node) return;
      void ed.runEdit(() => ed.doc.changeShape(node, token), { gate: true }).then((applied) => {
        if (applied) ed.setStatus(t("object.changeShape.done"));
      });
    },
    gradientCommands: (mutationEnabled, mutationReason) => {
      const node = ed.objectSelection?.node;
      const format = node ? ed.doc.shapeFormat?.(node) : null;
      const fill = format?.fill ?? null;
      format?.free?.();
      return gradientCommands({
        fill,
        mutationEnabled,
        mutationReason,
        t,
        apply: (from, to, angle) => {
          if (!node) return;
          void ed.runEdit(() => ed.doc.setShapeGradient(node, from, to, angle), { gate: true }).then(() => ed.reflectShapeSwatches());
        },
      });
    },
    outlineStyleCommands: (picture, mutationEnabled, mutationReason) => {
      const node = ed.objectSelection?.node;
      return outlineStyleCommands({
        style: node ? readOutlineStyle(ed.doc.shapeFormat?.(node)) : null,
        picture,
        mutationEnabled,
        mutationReason,
        t,
        apply: (dash, head, tail) => {
          if (!node) return;
          void ed.runEdit(() => ed.doc.setShapeOutlineStyle(node, dash, head, tail), { gate: true }).then(() => ed.reflectShapeSwatches());
        },
      });
    },
    readOnlyReason: () => ed.readOnlyReason,
    wrapModes: () => WRAP_CHOICES.map((choice) => [choice.value, t(choice.key)]),
    positionPresets: () => POSITION_PRESETS.map((preset) => [preset.id, t(preset.key), preset]),
    zOrderChoices: () => Z_ORDER_CHOICES.map((choice) => [choice.value, t(choice.key)]),
    rotateChoices: () => ROTATE_CHOICES.map((choice) => [choice.value, t(choice.key)]),
    text: (key) => t(key),
    // ONE gather per menu build. `objectPosition` and `objectTransform` are each
    // O(document) with a single walk, so asking them once here — rather than once
    // per row, of which there are twenty-odd — is the difference between a menu
    // that opens in constant work and one that does not.
    arrangeState: () => {
      const state = ed.arrangeState();
      if (!state) return null;
      const wrappable = wrapAvailability(state.read);
      const positionable = positionAvailability(state.read);
      return {
        wrappable: { available: wrappable.available, reason: wrappable.reasonKey ? t(wrappable.reasonKey) : "" },
        positionable: {
          available: positionable.available,
          reason: positionable.reasonKey ? t(positionable.reasonKey) : "",
        },
        stackable: ed.objectArrange.zOrderAvailability(state),
        groupable: ed.objectArrange.groupability(),
        rotatable: !!state.transform,
      };
    },
    activeWrap: () => {
      const state = ed.arrangeState();
      return state ? activeWrapChoice(state.read, state.wrap) : null;
    },
    applyPosition: (preset) => ed.objectArrange.applyPosition(preset),
    setZOrder: (value) => ed.objectArrange.setZOrder(value),
    group: () => void ed.objectArrange.group(),
    ungroup: () => void ed.objectArrange.ungroup(),
    rotate: (value) => ed.objectArrange.rotate(value),
    addText: () => void ed.objectArrange.addText(),
    shapeColors: () => ed.SHAPE_MENU_COLORS,
    objectWrap: (root) => ed.doc.objectWrap(root),
    documentRows: () => ed.referenceObjectMenuRows(),
    setObjectWrap: ed.setObjectWrap,
    openAltText: () => ed.openAltTextDialog(),
    chartCommands: () => ed.chartSurface.commandsFor(),
    applyShapeFill: ed.applyShapeFill,
    applyShapeOutline: ed.applyShapeOutline,
    enterCrop: () => ed.enterCropMode(),
    changePicture: () => ed.pictureReplace.choose(),
    pictureToolCommands: (mutationEnabled, mutationReason) => pictureToolCommands({
      doc: ed.doc,
      node: ed.objectSelection?.node,
      mutationEnabled,
      mutationReason,
      t,
      runEdit: (thunk, options) => ed.runEdit(thunk, options),
      announce: (text) => ed.setStatus(text),
    }),
    openProperties: () => ed.toggleObjectInspector(true),
    deleteObject: () => ed.deleteSelectedObject(),
    moveInText: () => ed.keyboardObjectMove.begin(false),
  };

  // Resolves a pointer event to an OBJECT context (or null). Prefers a fresh
  // object hit-test at the point; falls back to the already-selected object when
  // the click lands on it (e.g. on a resize handle the hit-test skips). The
  // caller selects the object before showing the menu.
  function objectContextAtEvent(page, event) {
    const { x, y } = ed.pointToTwip(page, event);
    let object = ed.doc.objectAt(page.pageNumber, x, y);
    // A right-click on the selected MEMBER of a group is about that member. The
    // fresh hit answers with the group's root, which put the GROUP's menu — no
    // alt text, no crop, no border — over the picture just selected (HF-214).
    const selectedHere = ed.objectSelection?.mode === "selected" && ed.pointInsideObject(ed.objectSelection.node, page, x, y);
    if (object && selectedHere && object.root === ed.objectSelection.ref.root) object = void object.free?.();
    if (object) {
      const capabilities = ed.objectCapabilities(object);
      const ref = ed.objectReference(object, object.node);
      const ctx = {
        surface: "object",
        node: object.node,
        ref,
        kind: object.kind,
        anchored: object.anchored,
        ...capabilities,
      };
      object.free?.();
      return ctx;
    }
    // No fresh hit (or the selected member's), and the point is inside the
    // selected object's box: the menu is about what is selected.
    return selectedHere ? ed.selectedObjectContext() : null;
  }

  return {
    buildContextCommands, caretTableColumn, contextAt, decideContextRevision, editorContextMenu,
    objectContextAtEvent, objectContextMenuHost, plainTableInfo, reviewContextAt,
    selectTableContext, selectionContainsClientPoint, tableToolCommands,
  };
}
