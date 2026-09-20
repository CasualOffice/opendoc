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
