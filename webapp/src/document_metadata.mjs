// The read-only half of Document properties: `docProps/app.xml` and the parts of
// `docProps/core.xml` nobody edits, projected onto the dialog's definition list.
//
// Extracted from `main.js` to pay for the version history panel under the line
// ratchet (`module_seams.test.mjs`) — the file was at its ceiling with zero
// slack — and it is a clean thing to take out because it is a PROJECTION and
// nothing else: one engine getter's JSON in, seventeen `<dd>` elements out, no
// state, no commands, no transactions. The EDITABLE half (the six fields, Apply,
// and the transaction it runs) stays in `main.js` with the edit path it belongs
// to.
//
// The English here is the English that was in `main.js`; this is a move, not a
// rewrite. Routing "Not set" and the four saved-count words through the
// catalogue is a separate change, and mixing it in would have made a pure move
// unreviewable.

/** Every element the read-out fills, once. Ids rather than arguments because the
 *  dialog's markup is fixed and seventeen parameters would be worse. */
function readoutElements() {
  return {
    created: document.getElementById("metaCreated"),
    modified: document.getElementById("metaModified"),
    lastModifiedBy: document.getElementById("metaLastModifiedBy"),
    lastPrinted: document.getElementById("metaLastPrinted"),
    revision: document.getElementById("metaRevision"),
    language: document.getElementById("metaLanguage"),
    contentStatus: document.getElementById("metaContentStatus"),
    version: document.getElementById("metaVersion"),
    application: document.getElementById("metaApplication"),
    appVersion: document.getElementById("metaAppVersion"),
    template: document.getElementById("metaTemplate"),
    company: document.getElementById("metaCompany"),
    manager: document.getElementById("metaManager"),
    totalTime: document.getElementById("metaTotalTime"),
    savedStats: document.getElementById("metaSavedStats"),
    customSection: document.getElementById("metaCustomSection"),
    customList: document.getElementById("metaCustomList"),
  };
}

/** One `<dd>`: the value, or "Not set" marked as empty. An absent value is
 *  stated rather than left blank — a blank cell is indistinguishable from a
 *  cell that failed to fill. */
function displayMetadataValue(element, value, formatter = String) {
  if (!element) return;
  const hasValue = value !== null && value !== undefined && value !== "";
  element.textContent = hasValue ? formatter(value) : "Not set";
  element.classList.toggle("metadata-empty", !hasValue);
  if (hasValue) element.title = String(value);
  else element.removeAttribute("title");
}

/** A metadata date in the reader's locale, or the raw string when the producer
 *  wrote something `Date` cannot parse — which happens, and showing it is more
 *  use than showing "Invalid Date". */
function formatMetadataDate(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return String(value);
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

/** A custom property's value. `bool` is the one typed kind the engine reports
 *  that has no useful string form of its own. */
function customMetadataValue(value) {
  if (!value || typeof value !== "object") return "";
  if (value.type === "bool") return value.value ? "True" : "False";
  return value.value ?? "";
}

/**
 * Builds the read-out and returns the one function that fills it.
 *
 * Complexity: O(custom properties) — the fixed fields are a constant number of
 * assignments, and `documentMetadata()` is one engine call that does not walk the
 * document body.
 *
 * @returns {{reflect: (doc: object) => void}}
 */
export function createMetadataReadout() {
  const el = readoutElements();
  return {
    reflect(doc) {
      const metadata = JSON.parse(doc.documentMetadata());
      const core = metadata.core ?? {};
      const app = metadata.app ?? {};

      displayMetadataValue(el.created, core.created, formatMetadataDate);
      displayMetadataValue(el.modified, core.modified, formatMetadataDate);
      displayMetadataValue(el.lastModifiedBy, core.lastModifiedBy);
      displayMetadataValue(el.lastPrinted, core.lastPrinted, formatMetadataDate);
      displayMetadataValue(el.revision, core.revision);
      displayMetadataValue(el.language, core.language);
      displayMetadataValue(el.contentStatus, core.contentStatus);
      displayMetadataValue(el.version, core.version);

      displayMetadataValue(el.application, app.application);
      displayMetadataValue(el.appVersion, app.appVersion);
      displayMetadataValue(el.template, app.template);
      displayMetadataValue(el.company, app.company);
      displayMetadataValue(el.manager, app.manager);
      displayMetadataValue(
        el.totalTime,
        app.totalTime,
        (minutes) => `${Number(minutes).toLocaleString()} min`,
      );

      const savedCounts = [
        ["pages", app.pages],
        ["words", app.words],
        ["characters", app.characters],
        ["paragraphs", app.paragraphs],
      ]
        .filter(([, value]) => value !== null && value !== undefined)
        .map(([label, value]) => `${Number(value).toLocaleString()} ${label}`)
        .join(" · ");
      displayMetadataValue(el.savedStats, savedCounts);

      el.customList.replaceChildren();
      const custom = Array.isArray(metadata.custom) ? metadata.custom : [];
      for (const property of custom) {
        const row = document.createElement("div");
        const name = document.createElement("dt");
        const value = document.createElement("dd");
        name.textContent = property.name;
        value.textContent = customMetadataValue(property.value) || "Not set";
        row.append(name, value);
        el.customList.append(row);
      }
      el.customSection.hidden = custom.length === 0;
    },
  };
}

/** `docProps/core.xml` field → the input that edits it. Word's own Properties
 *  dialog offers exactly these six; the rest of `core.xml` is producer-written
 *  and read-only above. */
const EDITABLE_FIELDS = Object.freeze([
  ["title", "propTitle"],
  ["creator", "propCreator"],
  ["subject", "propSubject"],
  ["category", "propCategory"],
  ["keywords", "propKeywords"],
  ["description", "propDescription"],
]);

/**
 * The Document properties dialog: the six editable fields, the read-out beside
 * them, and the one transaction Apply runs.
 *
 * Moved out of `main.js` with the read-out, because splitting one dialog across
 * two files is worse than moving it whole — and because the pane the File page
 * shows and the dialog are the SAME element (`file_pane.mjs`), so "fill this from
 * the document" has to be one function with one caller shape, or the pane shows
 * the previous document's values.
 *
 * Complexity: O(fields) to fill, plus the read-out's O(custom properties). Apply
 * is one engine operation.
 *
 * @param {object} deps
 * @param {(element: Element, options: object) => object} deps.registerModal
 * @param {() => object|null} deps.getDoc
 * @param {(run: () => object, options?: object) => Promise<unknown>} deps.runEdit
 *        the editor's one gated edit path, so Apply is an ordinary undoable
 *        transaction rather than a second write route.
 */
export function createPropertiesDialog({ registerModal, getDoc, runEdit }) {
  const readout = createMetadataReadout();
  const button = document.getElementById("propertiesBtn");
  const panel = document.getElementById("propertiesPanel");
  const fields = EDITABLE_FIELDS.map(([key, id]) => [key, document.getElementById(id)]);
  const modal = registerModal(panel, {
    initialFocus: () => fields[0]?.[1] ?? null,
    fallbackFocus: () => button,
  });

  /** Loads the dialog from the open document. Also the File page's pane hook:
   *  the pane fills itself when it is shown, because the element is shared. */
  function fill() {
    const doc = getDoc();
    if (!doc) return;
    const current = JSON.parse(doc.documentProperties());
    for (const [key, input] of fields) {
      if (input) input.value = current[key] ?? "";
    }
    readout.reflect(doc);
  }

  function toggle(open) {
    const show = open ?? !modal.isOpen;
    if (show === modal.isOpen) return;
    if (show) fill();
    button?.setAttribute("aria-expanded", String(show));
    if (show) modal.open();
    else modal.close();
  }

  button?.addEventListener("click", (event) => {
    event.stopPropagation();
    toggle();
  });
  document.getElementById("propertiesCancel")?.addEventListener("click", () => toggle(false));
  document.getElementById("propertiesClose")?.addEventListener("click", () => toggle(false));
  document.getElementById("propertiesApply")?.addEventListener("click", async () => {
    const doc = getDoc();
    if (!doc) return;
    const current = JSON.parse(doc.documentProperties());
    for (const [key, input] of fields) {
      const value = (input?.value ?? "").trim();
      current[key] = value ? value : null;
    }
    await runEdit(() => doc.setDocumentProperties(JSON.stringify(current)), { gate: true });
    toggle(false);
  });

  return { toggle, fill };
}
