# Work tracker

[The single open queue](../109-BACKLOG.md) is the authoritative order and status
of work. Change a row there in the same change as the implementation.

- [Pending](pending.md) lists open rows without a partial or active status.
- [In progress or partly delivered](in-progress.md) lists rows with work or
  acceptance already underway. A partial status does not imply an assigned owner.
- [Completed](completed.md) holds the closed-row evidence previously kept at the
  end of `109`.
- [Retired plans and research](completed/retired-documents.md) records the
  replacement for each removed historical file and how to recover its text.

The pending and in-progress files are generated views. Regenerate them with
`node webapp/tools/build-tracker-views.mjs`; CI checks they match `109`. Do not
edit those two files to change a work item. Older trackers `14`, `99`, `104`,
`105`, and `106` remain dated evidence archives, closed to new rows.
