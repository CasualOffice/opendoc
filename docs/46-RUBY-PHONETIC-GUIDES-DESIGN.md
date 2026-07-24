# Ruby (Phonetic Guides) Design

**Status:** Accepted — 2026-07-25 (repository owner directive: complete the model)
**Tracker:** P1A-019 (schema v1 semantic extension), ruby slice
**Decision basis:** importer no-skip audit (`P1A-025`, ruby minor finding)

## Why

The no-skip audit found that a WordprocessingML ruby (`w:ruby`, an East-Asian
phonetic guide) had both its annotation (`w:rt`) and base (`w:rubyBase`) text
captured and flattened into the paragraph in raw document order — the annotation
appears *before* the base, so the reading text is reordered/merged with the
pronunciation guide.

## Model

None. The base text is the reading text and is captured as ordinary runs in its
document position. The annotation (a pronunciation aid, rendered above the base)
is not modeled in this slice; it is dispositioned in the compatibility report and,
in Retention, preserved. Full annotation modeling (a `Ruby { base, annotation }`
node) is a possible additive follow-up.

## Import

- Entering `w:rt` sets `in_ruby_annotation`; while set, a run's `w:t` text is
  **not** emitted as a run (the annotation is dropped). Leaving `w:rt` reports
  `rt` (the annotation is dispositioned, not silently lost) and clears the flag.
- `w:rubyBase` runs are captured normally, so the base reads in document order at
  the ruby's position — fixing the reorder/merge bug.
- The flag is saved/restored across text-box frames and reset at paragraph close,
  so a malformed unclosed `w:rt` cannot suppress a later paragraph's text.
- `w:ruby`/`w:rubyPr`/`w:rubyBase` are reported (structural, unmodeled).

## Out of scope

Modeling the annotation text (a `Ruby` node) and ruby alignment/positioning.
