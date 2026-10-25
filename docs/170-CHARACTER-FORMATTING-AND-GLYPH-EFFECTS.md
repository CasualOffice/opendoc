# 170 — Character formatting and glyph effects

Implementation design for the owner's requested `docs/168` batch 3 (RM-18).
The implementation merged in #835. Acceptance and capability re-grading remain
open in `docs/109` and `docs/168`; the CI notes below record the state at the
time of that PR, not the current status of GitHub checks.
Uses the transaction and backend boundaries in docs/43, docs/58 and docs/108.

## Authoring and policy

Extend `FormatText` with an optional boxed character-property delta. Missing
fields preserve each selected run; explicit null clears direct formatting so
its named style can apply again. The default delta keeps its existing wire
shape. Apply a multi-paragraph selection as one undo action, preserving text,
paragraph structure, field markers, revision wrappers and every unrelated run
property. Viewing and document protection refuse mutation. Suggesting must
record the property change through the existing formatting-revision path.

Expose a bounded WASM character-format patch and a uniform/mixed-state query.
The Font dialog is a separate web module, with Font and Advanced tabs. Caps,
small caps and double strike also have individual commands. Collapsed-caret
changes arm the existing pending typing format and apply to the next insertion;
they do not rewrite letters or format an entire paragraph unexpectedly.

Spacing, position, scale, kerning, borders and emphasis use the existing model
and validation domains. Invalid values are refused before mutation, rather than
clamped. Applying unchanged or mixed controls must preserve imported values.

## Glyph ink and shaping

Carry outline, shadow, emboss and imprint through the backend-neutral glyph
run and the paragraph-cache signature. Effects change ink, not advances or
stored text. Default runs keep their existing serialization and paint path.
CPU rendering and PDF consume the same effect geometry; PDF retains real,
selectable text. Colored/bitmap glyphs must remain visible when effects apply.
Effect work and temporary buffers are bounded to visible glyph ink, not a full
page allocation for every run.

Kerning uses the authored half-point threshold against the vertically unscaled
font size. No threshold preserves existing shaping defaults. Add the Office
2010 ligature vocabulary from Microsoft's Open XML SDK `LigaturesValues`:
all sixteen combinations, including the XML spelling `discretional`. Map its
four groups to OpenType liga/clig/dlig/hlig; mandatory shaping features remain
untouched. Import/export and the style cascade preserve explicit none, inherited
settings and combined modes. Unsupported tokens remain reported.

## Evidence and scope

One browser spec covers the four tasks. Engine guards cover selected ranges,
undo, policies, DOCX round trips, mixed values, actual ink, kerning and ligature
shaping. Pure web guards cover patch validation and unchanged/mixed controls.
Drive guards red with targeted mutations. Run the shared gates once at the end
of the batch, then refresh docs/153 and docs/168 from evidence. A task with an
unresolved limitation stays open; do not mark the character family fully drawn
until the named rendering work is verified.

## UI and review status

The owner asked to maintain UX and the project UI guidelines. The Font surface
uses docs/63's standard modal, shared controls, token colors, Font/Advanced tabs,
Close/Cancel/Apply, roving tab focus and the shared focus trap. A Home Font-group
launcher and Format > Font menu entry complement the palette. The body scrolls
while actions stay reachable; numeric controls remain content-sized. Mixed
checkboxes and fields show mixed state, and untouched imported border values
survive unrelated edits. Ligatures can return to style inheritance.

The modal is registered in the shared roster, so desktop clipping, dismissal,
focus restoration and phone sheet checks run alongside the feature checks.
Final UX and shared gates will run in GitHub CI: the owner explicitly requested
CI monitoring instead of further local test execution. Prior to that request,
the five feature browser tests and the targeted engine checks passed. Mutation
runs proved range properties, ligature round trips, CPU/PDF ink, color masks,
OpenType shaping, concurrency and all five browser checks fail when their
behavior is removed. The subsequent UI refinement still needs CI validation.

Color glyph effects allocate only the visible glyph box, with a one-megapixel
ceiling and a four-pixel outline-mask radius. Above the allocation ceiling,
original colored artwork remains visible. Ordinary vector glyph effects do not
need that buffer and keep their outline path at large sizes.

CI reporting preserves the workspace command's exit status and now lists failed
Rust cases as check annotations. This makes failures inspectable through the
GitHub checks API even when the environment cannot download the separate
Actions blob-storage logs. The full workspace still runs with all features and
the frozen lockfile; no guard is disabled.

The plan guard correctly rejected re-graded capabilities while RM-18 was still
pending. Matrix re-grading and the batch's `done` marker will therefore land
together after feature and UX CI passes. The new editing module is included in
the exhaustive container audit. Web unit failures, like Rust failures, are now
reported by case name in GitHub annotations without changing their exit status.

Review cards include named advanced character deltas and show spacing, position,
scale and kerning in user units. The effective-format query aggregates one
paragraph at a time and serializes only character fields, so it does not clone
JSON or nested revision snapshots for every run in a large selection.

The shared select and ribbon-face guards caught missing declarations on the
new controls. Both selects now carry `dialog-select`, and the Font launcher
names `format.character.dialog` in the shared ribbon roster. The wasm artifact
crossed its existing size ceilings; CI reports the measured byte counts before
any budget decision. Optimization flags and the non-stub floor remain unchanged.

## Measured wasm budget decision

CI run 38308321551 on commit 42a70e6b measured 25,869,487 bytes total and
11,960,785 bytes of code. The merged collaboration baseline measured
25,711,106 / 11,820,216 bytes: this batch adds 158,381 / 140,569 bytes
(0.62% of the download). It adds the typed character transaction data, Word
ligature vocabulary and shaping, and CPU/PDF glyph effects. The release `z`,
hot-crate `3` overrides, Binaryen 117 and `-Oz` settings remain unchanged.

The existing budget policy allows measured feature growth with bounded
headroom. Set total/code ceilings to 26,000,000 / 12,100,000 bytes, leaving
130,513 / 139,215 bytes of headroom. The three-megabyte non-stub floor remains;
the code ceiling remains below even the earlier, smaller engine's measured
12,376,042-byte optimization regression. This is a recorded growth decision,
not a disabled check or a change to the main.js ratchet. Rust tests, lint,
docs, wasm check and benchmarks are green in CI; final browser checks remain.

The single-axis reachability roster records the Font launcher as the owner of
`format.character.*`, with a real control the guard verifies. Individual field
commands focus their corresponding input in the correct tab; the generic Font
and Advanced commands focus their tabs. The batch spec verifies that route.
