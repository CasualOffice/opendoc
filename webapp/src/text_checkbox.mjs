// DOCX text checklists (including sample.docx) have no form-control state.
// Only a leading ballot box followed by whitespace is a click target.
export function textCheckboxReplacement(text, offset) {
  if (!Number.isInteger(offset) || offset < 0 || offset > 3) return null;
  if (!/^[☐☑☒](?:\s|$)/u.test(text)) return null;
  return { start: 0, end: 3, text: text[0] === "☐" ? "☒" : "☐" };
}

export function toggleTextCheckboxAt(doc, node, offset, runEdit) {
  if ((offset ?? 0) > 3) return false;
  const text = doc.copyText(node, 0, node, doc.paragraphLength(node));
  const replacement = textCheckboxReplacement(text, offset ?? 0);
  if (!replacement) return false;
  // Use the normal transaction for formatting, review mode, export and undo.
  runEdit(() => doc.replaceSelection(node, replacement.start, node, replacement.end, replacement.text));
  return true;
}
