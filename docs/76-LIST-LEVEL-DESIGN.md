# List level promotion and demotion

Tab and Shift+Tab on list paragraphs now change the numbering level instead of
only changing paragraph indentation. Levels are clamped to `0..=8`, preserve
the existing numbering instance, and apply to every selected paragraph in one
undoable `SetParagraphProperties` transaction. Non-list paragraphs retain the
existing 360-twip indent behavior.
