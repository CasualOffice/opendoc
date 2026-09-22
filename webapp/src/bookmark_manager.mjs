// The bookmark manager dialog.
//
// A Word/Docs-style surface over the engine's `bookmarkEntries` /
// `createBookmark` / `renameBookmark` / `deleteBookmark` operations plus the
// existing `bookmarkPosition` navigation. Each mutation is one undoable action,
// grouped by the engine as a "Bookmark change".
//
// Extracted from `main.js` under `109` HF-085 — and, immediately, to pay for
// the spelling and grammar wiring under the line ratchet (`docs/114` §6). It is
// a good candidate for the same reason `glyph_picker.mjs` was: the dialog owns
// its own markup and its own list state, and everything it needs from the
// application is a verb it can be handed. Nothing about bookmarks is decided in
// `main.js` any more; what stays there is `applyBookmarkEdit`, which is about
// repainting pages, and the review-mode gate, which is about review mode.
//
// `bookmark-manager.spec.mjs` exercises the whole of this through the real
// dialog, so the extraction is covered end to end rather than by inspection.

/**
 * Builds the manager over the dialog already in the page.
 *
 * `io` is its entire contact with the application:
 *
 *   `getDoc()`             the open document, or null
 *   `applyEdit(res)`       Promise; repaints after an engine EditResult and
 *                          marks the document edited, WITHOUT moving the caret
 *   `mutationBlocked()`    true (having said why) when the current review mode
 *                          refuses a structural change
 *   `hasRange()`           whether there is a text selection to bookmark
 *   `selectionEndpoints()` `[sNode, sOff, eNode, eOff]` or null
 *   `navigateTo(position)` put the caret at `{ node, offset }` and show it
 *   `status(text, kind)`   the status line
 *   `registerModal(dialog, options)` the host's modal registry
 *   `fallbackFocus()`      where focus goes when the dialog closes
 */
export function createBookmarkManager(io) {
  const dialog = document.getElementById("bookmarkDialog");
  const nameInput = document.getElementById("bookmarkNameInput");
  const addForm = document.getElementById("bookmarkAddForm");
  const addNote = document.getElementById("bookmarkAddNote");
  const list = document.getElementById("bookmarkList");
  const empty = document.getElementById("bookmarkEmpty");
  const sortBtn = document.getElementById("bookmarkSortBtn");
  const sortLabel = document.getElementById("bookmarkSortLabel");
  const closeBtn = document.getElementById("bookmarkClose");
  const doneBtn = document.getElementById("bookmarkDone");
  if (!dialog) return { open: () => {}, close: () => {} };

  const ADD_HINT = "Select text in the document, then add a bookmark for it.";
  let sortAscending = true;

  const modal = io.registerModal(dialog, {
    initialFocus: () => nameInput,
    fallbackFocus: io.fallbackFocus,
  });

  /** The bookmark name's byte length under the engine's UTF-8 255-byte bound. */
  function nameByteLength(name) {
    return new TextEncoder().encode(name).length;
  }

  /** A clean, user-facing validation message for `name`, or "" when valid. The
   *  engine enforces the same non-empty + 255-byte bound (its raw error name is
   *  internal vocabulary, so it is never shown directly). */
  function validateName(name) {
    if (!name) return "Enter a name for the bookmark.";
    if (nameByteLength(name) > 255) return "That name is too long (max 255 characters).";
    return "";
  }

  function setAddNote(message, isError) {
    addNote.textContent = message || ADD_HINT;
    addNote.classList.toggle("error", !!isError && !!message);
  }

  /** The current bookmarks as `[{ id, name }]`, parsed from the engine's
   *  `"{id}\t{name}"` entries and ordered by the active sort direction. */
  function entries() {
    const doc = io.getDoc();
    if (!doc) return [];
    const rows = (doc.bookmarkEntries?.() || []).map((row) => {
      const tab = row.indexOf("\t");
      return { id: row.slice(0, tab), name: row.slice(tab + 1) };
    });
    rows.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
    if (!sortAscending) rows.reverse();
    return rows;
  }

  function refresh() {
    const rows = entries();
    list.replaceChildren();
    empty.hidden = rows.length > 0;
    list.hidden = rows.length === 0;
    for (const { id, name } of rows) list.append(buildRow(id, name));
  }

  /** One bookmark row: a Go-to button (name) plus Rename and Delete actions. */
  function buildRow(id, name) {
    const li = document.createElement("li");
    li.className = "bookmark-row";
    li.dataset.id = id;

    const goto = document.createElement("button");
    goto.type = "button";
    goto.className = "bookmark-goto";
    goto.title = `Go to “${name}”`;
    goto.innerHTML = `<span class="ms" aria-hidden="true">arrow_forward</span><span class="bookmark-name"></span>`;
    goto.querySelector(".bookmark-name").textContent = name;
    goto.addEventListener("click", () => navigate(name));

    const actions = document.createElement("div");
    actions.className = "bookmark-row-actions";

    const rename = document.createElement("button");
    rename.type = "button";
    rename.className = "bookmark-action";
    rename.title = "Rename";
    rename.setAttribute("aria-label", `Rename “${name}”`);
    rename.innerHTML = `<span class="ms" aria-hidden="true">edit</span>`;
    rename.addEventListener("click", () => beginRename(li, id, name));

    const del = document.createElement("button");
    del.type = "button";
    del.className = "bookmark-action danger";
    del.title = "Delete";
    del.setAttribute("aria-label", `Delete “${name}”`);
    del.innerHTML = `<span class="ms" aria-hidden="true">delete</span>`;
    del.addEventListener("click", () => remove(id, name));

    actions.append(rename, del);
    li.append(goto, actions);
    return li;
  }

  /** Navigates to `name` (reusing the existing bookmarkPosition resolver) and
   *  places the caret there, closing the dialog so focus returns to the canvas. */
  function navigate(name) {
    const doc = io.getDoc();
    if (!doc) return;
    const encoded = doc.bookmarkPosition(name);
    const [node, offset] = encoded.split("\t");
    if (!node || !offset) {
      io.status(`Bookmark “${name}” was not found`, "error");
      return;
    }
    close();
    io.navigateTo({ node, offset: Number(offset) });
  }

  /** Swaps a row's name for an inline editor. Enter confirms via rename;
   *  Esc (or blur) cancels and restores the row. */
  function beginRename(li, id, currentName) {
    if (li.classList.contains("editing")) return;
    li.classList.add("editing");
    const goto = li.querySelector(".bookmark-goto");
    const input = document.createElement("input");
    input.type = "text";
    input.className = "bookmark-rename-input";
    input.maxLength = 255;
    input.value = currentName;
    input.setAttribute("aria-label", "New bookmark name");
    li.replaceChild(input, goto);
    input.focus();
    input.select();

    let settled = false;
    const cancel = () => {
      if (settled) return;
      settled = true;
      refresh();
    };
    const commit = () => {
      if (settled) return;
      const next = input.value.trim();
      if (next === currentName) return cancel();
      const problem = validateName(next);
      if (problem) {
        io.status(problem, "error");
        input.focus();
        input.select();
        return;
      }
      if (io.mutationBlocked()) {
        settled = true;
        close();
        return;
      }
      settled = true;
      applyRename(id, next);
    };

    input.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        commit();
      } else if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        cancel();
      }
    });
    input.addEventListener("blur", cancel);
  }

  function createFromSelection() {
    const doc = io.getDoc();
    if (!doc) return;
    const name = nameInput.value.trim();
    const problem = validateName(name);
    if (problem) {
      setAddNote(problem, true);
      nameInput.focus();
      return;
    }
    if (!io.hasRange()) {
      setAddNote("Select some text in the document first.", true);
      return;
    }
    const ends = io.selectionEndpoints();
    if (!ends) return;
    if (io.mutationBlocked()) {
      close();
      return;
    }
    let res;
    try {
      res = doc.createBookmark(ends[0], ends[1], ends[2], ends[3], name);
    } catch (err) {
      console.warn("createBookmark ignored:", err?.message ?? err);
      setAddNote("That bookmark couldn't be created for this selection.", true);
      return;
    }
    io.applyEdit(res).then(() => {
      nameInput.value = "";
      setAddNote("", false);
      refresh();
      io.status(`Bookmark “${name}” added`);
      nameInput.focus();
    });
  }

  function applyRename(id, name) {
    const doc = io.getDoc();
    if (!doc) return;
    let res;
    try {
      res = doc.renameBookmark(id, name);
    } catch (err) {
      console.warn("renameBookmark ignored:", err?.message ?? err);
      io.status("That bookmark couldn't be renamed", "error");
      refresh();
      return;
    }
    io.applyEdit(res).then(() => {
      refresh();
      io.status(`Bookmark renamed to “${name}”`);
    });
  }

  function remove(id, name) {
    const doc = io.getDoc();
    if (!doc) return;
    if (io.mutationBlocked()) {
      close();
      return;
    }
    let res;
    try {
      res = doc.deleteBookmark(id);
    } catch (err) {
      console.warn("deleteBookmark ignored:", err?.message ?? err);
      io.status("That bookmark couldn't be deleted", "error");
      refresh();
      return;
    }
    io.applyEdit(res).then(() => {
      refresh();
      io.status(`Bookmark “${name}” deleted`);
    });
  }

  function open() {
    if (!io.getDoc() || !modal) return;
    nameInput.value = "";
    setAddNote("", false);
    refresh();
    modal.open();
  }

  function close() {
    modal?.close();
  }

  addForm.addEventListener("submit", (event) => {
    event.preventDefault();
    createFromSelection();
  });
  nameInput.addEventListener("input", () => {
    if (addNote.classList.contains("error")) setAddNote("", false);
  });
  sortBtn.addEventListener("click", () => {
    sortAscending = !sortAscending;
    sortLabel.textContent = sortAscending ? "A–Z" : "Z–A";
    refresh();
  });
  closeBtn.addEventListener("click", () => close());
  doneBtn.addEventListener("click", () => close());

  return {
    open,
    close,
    /** The document's bookmarks, `[{ id, name }]`, in the manager's current
     *  sort order. Exposed because the link dialog offers them as internal link
     *  targets — a READ of the document, not part of this dialog, and the
     *  parsing of the engine's `"{id}\t{name}"` rows belongs in one place.
     *  Missing it was a real regression: the extraction left
     *  `populateLinkPlaces` calling a function that had moved, so the link
     *  dialog threw and never opened. */
    entries,
  };
}
