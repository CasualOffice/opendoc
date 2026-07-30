# Cursor and selection hit audit

Pointer-down now uses the same nearest-page resolution as drag continuation,
so clicks in page gaps and outer margins no longer get dropped before the
engine sees them. Page-local coordinates are clamped to the rendered sheet
before `hitTest`, allowing the layout engine to choose the nearest caret at an
edge. Existing pointer-cancel, blur, hidden-tab, backward-selection, and table
selection paths remain unchanged and are covered by the browser smoke suite.
