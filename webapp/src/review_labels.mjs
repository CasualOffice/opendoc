// The words a review item is shown and announced by.
//
// Attribution tooltips, the change-type vocabulary and the accessible name of a
// comment/suggestion card are all pure string building over the engine's review
// payload, and all three are read by a screen reader — so they are exactly the
// strings a localisation seam will need next (`105` UX-009/CQ-005), and exactly
// the ones that should be provable without booting a browser.

/** The human display name for an author, matching the sidebar's existing
 *  convention: name, else initials, else "You" (an unattributed local edit). */
export function reviewAuthorDisplay(item) {
  return (
    String(item?.author ?? "").trim() || String(item?.initials ?? "").trim() || "You"
  );
}

/** A short, locale-formatted date for an attribution line. Returns "" for a
 *  missing date and the raw value for one the platform cannot parse, so an
 *  unexpected engine value is shown rather than silently dropped. */
export function formatReviewDate(value) {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return String(value);
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date);
}

/** The change-type label used in an attribution tooltip. */
export function reviewChangeTypeLabel(kind) {
  switch (kind) {
    case "insertion": return "Insertion";
    case "deletion": return "Deletion";
    case "replacement": return "Replacement";
    case "formatting": return "Formatting change";
    case "move": return "Move";
    case "move_from": return "Move (source)";
    case "move_to": return "Move (destination)";
    // Paragraph-level revisions (docs/108). A paragraph mark is the pilcrow that
    // ends a paragraph, so inserting one is a new paragraph break and deleting
    // one joins the paragraph to the next when accepted.
    case "paragraph_mark_insertion": return "Paragraph break added";
    case "paragraph_mark_deletion": return "Paragraph break deleted";
    case "paragraph_format": return "Paragraph formatting";
    default: return "Change";
  }
}

/** The `author · type · date` attribution string shown on hover of a tracked
 *  change (its inline marker and its sidebar card). Omits empty segments. */
export function reviewRevisionTooltip(revision) {
  return [
    reviewAuthorDisplay(revision),
    reviewChangeTypeLabel(revision.kind),
    formatReviewDate(revision.date),
  ].filter(Boolean).join(" · ");
}

/** The `author · date` attribution string for a comment marker/card. */
export function reviewCommentTooltip(comment) {
  return [
    reviewAuthorDisplay(comment),
    comment.resolved ? "Resolved" : "Comment",
    formatReviewDate(comment.date),
  ].filter(Boolean).join(" · ");
}

/** A descriptive accessible name for a review card — who, what kind, and a short
 *  text snippet — so a screen reader announces the card's content and role
 *  rather than a nameless generic article (REVIEW-GAP-023). */
export function reviewCardAriaLabel(item) {
  const author = reviewAuthorDisplay(item.data) || "You";
  const snippet = String(item.data.text || "").replace(/\s+/g, " ").trim().slice(0, 80);
  const suffix = snippet ? `: ${snippet}` : "";
  if (item.type === "comment") {
    const kind = item.data.parentParaId
      ? "Reply"
      : item.data.resolved
        ? "Resolved comment"
        : "Comment";
    return `${kind} by ${author}${suffix}`;
  }
  const kind = (reviewChangeTypeLabel(item.data.kind) || "change").toLowerCase();
  return `Suggested ${kind} by ${author}${suffix}`;
}



/** One side of a tracked formatting change, and the whole sentence that
 *  describes one, in Word's words.
 *
 *  Moved out of the shell: between them they hold twenty-five property NAMES
 *  and every value's rendering, all of it pure string work over plain objects,
 *  and none of it anything the editor has to be running to be tested. */
export function reviewFormattingValue(property, value) {
  if (value == null) return "inherited";
  // The model's logical alignment names, in Word's words ("Formatted: Centered").
  if (property === "alignment") {
    return { start: "Left", end: "Right", center: "Centered", justify: "Justified" }[value] ?? String(value);
  }
  if (typeof value === "boolean") return value ? "on" : "off";
  if (property === "sizeHalfPoints" && Number.isFinite(Number(value))) {
    return `${Number(value) / 2} pt`;
  }
  if (typeof value === "object") {
    const scalar = Object.values(value).find((candidate) =>
      ["string", "number", "boolean"].includes(typeof candidate));
    return scalar == null ? "custom" : String(scalar);
  }
  return String(value);
}

export function reviewFormattingDescription(changes) {
  const labels = {
    bold: "Bold",
    italic: "Italic",
    underline: "Underline",
    strike: "Strikethrough",
    font: "Font",
    sizeHalfPoints: "Font size",
    color: "Text color",
    highlight: "Highlight",
    verticalAlignment: "Vertical alignment",
    // Paragraph properties, from a tracked `w:pPrChange` (docs/108).
    style: "Style",
    alignment: "Alignment",
    numbering: "List",
    indentation: "Indent",
    spacing: "Spacing",
    keepNext: "Keep with next",
    keepLines: "Keep lines together",
    pageBreakBefore: "Page break before",
    widowControl: "Widow/orphan control",
    outlineLevel: "Outline level",
    contextualSpacing: "Contextual spacing",
    borders: "Borders",
    shading: "Shading",
    tabs: "Tab stops",
    bidi: "Right-to-left",
  };
  return (changes ?? []).map((change) => {
    const property = String(change?.property || "");
    const label = labels[property] || property || "Formatting";
    return `${label}: ${reviewFormattingValue(property, change?.before)} → ${reviewFormattingValue(property, change?.after)}`;
  }).join("\n");
}
