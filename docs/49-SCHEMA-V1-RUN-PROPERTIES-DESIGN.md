# Normalized Schema v1: Run-Property Long Tail Design

**Status:** Accepted — 2026-07-25 (repository owner directive: model everything,
data-driven priority from the multi-agent coverage audit)
**Tracker:** P1A-033 (first property-long-tail slice)
**Decision basis:** the model-coverage audit (rFonts ranked #1 by corpus
frequency, highlight #4, vertAlign #5, caps #8), comments (`47-…`), tracked
changes (`48-…`)

## Why

`RunProperties` mapped only bold/italic/underline/strike/color/size. The
coverage audit measured the reported-not-modeled run properties across the
fixture corpus and ranked the run-property tail as the highest-value, lowest-risk
remaining work: all additive `Option` fields, no new structural node, no import
state-machine hazard. A key finding: `font_ref` (and the `FontRef`/`FontName`/
`ThemeFont`/`ThemeFontRef` types) already existed and were validated, but
`apply_run_property` never populated them — so `w:rFonts` was reported-only
despite the model field existing.

## Scope (this slice: A + D + B)

Three cohesive groups shipped together:

- **A — toggle marks** (`w:caps`, `w:smallCaps`, `w:vanish`, `w:webHidden`,
  `w:dstrike`): additive `Option<bool>` fields (`all_caps`, `small_caps`,
  `hidden`, `web_hidden`, `double_strike`), mapped with the existing `is_true`
  toggle helper.
- **D — fonts** (`w:rFonts`): populate the existing `font_ref` as the `ascii`
  slot and add three sibling slots `font_ref_h_ansi`/`font_ref_cs`/
  `font_ref_east_asia`. Each slot prefers its `*Theme` attribute
  (`major*`→`ThemeFontRef::Major`, `minor*`→`Minor`) then its named attribute
  (bounded 255 bytes). `rFonts` is consumed only when a slot resolves; an
  `rFonts` carrying only unmodeled detail (e.g. just `@hint`) resolves nothing
  and is reported — **no silent loss**.
- **B — named vocabularies** (`w:vertAlign`, `w:highlight`, `w:em`): three closed
  enums (`VerticalAlignment`, `HighlightColor`, `EmphasisMark`) and their fields.
  An unknown `@val` is reported (not mapped), mirroring `sz`/`color`.

Deferred to follow-on slices (design captured, lower audit priority): **C —
typographic metrics** (`w:spacing` char / `w:kern` / `w:position`), **E —
language** (`w:lang`). Everything else in the run-property tail (underline
style/color, run shading, text effects, color theme tint/shade) stays
reported-only.

## Validation (additive)

- Every new named font slot is bounded like `font_ref` (`run.font_ref.name`,
  non-empty ≤255) — `check_run_property_refs` iterates all four slots.
- Enums are type-safe (no runtime domain check), matching `Alignment`.
- Toggles/enums need no numeric domain.

## Backward-compatibility

Every new field is `Option<_>` with `#[serde(skip_serializing_if =
"Option::is_none")]` (matching the existing fields — no `#[serde(default)]`, since
serde auto-defaults a missing `Option` to `None`). A default `RunProperties`
still serializes to `{}` (guarded by a unit test). The v0→v1 migration only sets
the four legacy toggles; `run_properties_from_marks` now ends with
`..RunProperties::default()` so adding fields cannot break its total-struct
literal and the byte-exact migration golden is unchanged.

## Review fixes folded (adversarial review, sound-with-fixes)

- `migration.rs` total-struct-literal compile break → `..RunProperties::default()`.
- Convention: new fields use only `skip_serializing_if` (no `#[serde(default)]`).
- `rFonts` returns `false` (reported) when no slot resolves — no silent swallow.

## Tests

Import: toggles (incl. `val=0` clear); named + theme font slots; `rFonts` with
only `@hint` reported; vocabularies mapped + unknown `@val` reported. Model:
long-tail round-trip; empty font-name rejected (`run.font_ref.name`); default
serializes to `{}`. All gates green.

## What stays reported-only (no silent loss)

Underline style/color (`w:u@val`/`@color` — the bool stays), run shading
(`w:shd`), text effects (`w:outline`/`emboss`/`imprint`/`shadow`/`effect`),
color theme tint/shade, `w:rtl`, `w:bdr`, `w:fitText`, and the deferred metrics
and language groups — all continue through `reporter.report(local)`.
