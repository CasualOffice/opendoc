# Cursor and selection hit audit

Pointer-down only resolves a concrete rendered page, so ambiguous page-gap
clicks are not turned into caret jumps. Page-local coordinates are clamped to the
rendered sheet before `hitTest`, allowing the layout engine to choose the nearest
caret at an edge. The layout hit tester also chooses the geometrically nearest
line in whitespace, using table-cell column membership only as a tie-breaker;
this prevents a wide table from capturing a click closer to a body paragraph.
Existing pointer-cancel, blur, hidden-tab, backward-selection, and table-selection
paths remain unchanged and are covered by the browser smoke suite.

Pointer-down deliberately does not map page-gap clicks to the nearest page:
doing so can place the caret in a nearby table when the user intended no
fragment at all. Nearest-page resolution remains enabled only after a real
drag begins, where it is needed for cross-page selection.
