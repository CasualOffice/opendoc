# Glyph picker panel UX

## Decision

Symbol and emoji insertion use a contextual supporting pane, not a dialog.
The pane is part of the editor shell, sits after the document viewport in the
right contextual column, and keeps the document visible while the user inserts
multiple glyphs.

## Design rules

- The pane is coplanar with the editor: no backdrop, modal lock, centered card,
  dialog shadow, or `aria-modal`.
- Header hierarchy matches the existing inspector panels: icon, title, short
  description, and a compact close button.
- Search and category tabs stay in the scrollable panel body; the repeated-
  insertion affordance stays in a small footer.
- Opening Symbol closes Emoji and vice versa. The editor caret remains the
  insertion target; opening the pane must not create a second editing surface.
- At compact widths the pane becomes a fixed right overlay with the same panel
  chrome, not a centered modal.
- Escape and Close dismiss the pane and return focus to the prior editor target.

This follows established supporting-pane patterns: Carbon describes right
panels as optional content associated with a shell action, while Material's
standard side sheet keeps the primary content visible and lays out beside it.
References: https://carbondesignsystem.com/components/UI-shell-right-panel/usage/
and https://m2.material.io/components/sheets-side.

## Verification contract

Browser coverage must assert the pane has `panel-head` and `panel-body`, has no
`dialog-card` or `aria-modal`, follows the viewport in the shell, remains open
after insertion, and closes cleanly with Escape.
