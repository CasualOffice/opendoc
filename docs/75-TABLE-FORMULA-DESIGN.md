# Table formula calculation

The first formula slice is intentionally bounded to deterministic calculated
values. It accepts `=SUM(ABOVE)`, `=SUM(LEFT)`, `=AVERAGE(...)`, `=MIN(...)`,
and `=MAX(...)` over numeric cells in the active regular table. The result is
written as a normal cell run through one undoable table replacement. Formula
field-code preservation and automatic dependency recalculation remain deferred
until the model has a first-class OOXML field representation; the UI labels this
as “Calculate formula” rather than claiming live spreadsheet semantics.
