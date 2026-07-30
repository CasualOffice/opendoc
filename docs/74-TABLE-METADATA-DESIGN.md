# Table caption and accessibility description

Table caption and description are exposed in the existing live Table
properties inspector. Values are trimmed, bounded to the model's 255-byte
limit, and empty values clear the authored metadata. They use the existing
`ReplaceTable` property transaction, so each completed field interaction is
undoable and DOCX export preserves `w:tblCaption` and `w:tblDescription`.
