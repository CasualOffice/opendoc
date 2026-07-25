# Normalized Schema v1: Paragraph-Property Long Tail Design

**Status:** Designed — 2026-07-25 (multi-agent coverage workflow; adversarially reviewed, verdict sound-with-fixes). Pending implementation.
**Tracker:** P1A-034

> Produced by the parallel model-coverage design workflow. The adversarial review flagged concrete implementation fixes (see the tracker entry); fold them in at implementation time.



**Status:** Proposed — 2026-07-25
**Tracker:** P1A-033 … P1A-036 (follow-on to P1A-032 tracked changes, the most recent analog)
**Decision basis:** ADR-027, schema v1 (`38-…`), tables (`39-…`), tracked changes (`48-…`); importer no-skip audit
**Target files:** `crates/casual-doc-model/src/v1/properties.rs`, `.../document.rs`, `crates/casual-doc-import/src/properties.rs`, `.../body.rs`

## Why

`ParagraphProperties` (`crates/casual-doc-model/src/v1/properties.rs:185`) models only `style_ref`, `numbering`, `alignment`, `indentation`, `spacing`. Everything else in `w:pPr` reaches the report arm at `body.rs:1005` (`_ if self.ppr_depth > 0 => … reporter.report(local)`) — byte-preserved in Retention, dispositioned in the compatibility report, but **not editable in the model**. This slice family promotes the high-value Western long tail: keep/break flow control, outline level, contextual spacing, line-number suppression, vertical text alignment, bidi, paragraph shading, paragraph borders, and custom tab stops.

All additions follow the established shapes exactly:
- `Option<…>` / `Vec<…>` fields with `#[serde(default, skip_serializing_if = …)]`, so an empty `ParagraphProperties` still serializes to `{}` and every existing snapshot + the v0→v1 migration golden stay **byte-identical** (v0 carries none of these).
- Bounded values validated in `check_paragraph_property_refs` (`document.rs:333`) via `check_domain(cond, "paragraph.…")`, reusing `ModelError::PropertyValueOutOfDomain` — **no new `ModelError` variant, no `error.rs` change**.
- Producer-specific token vocabularies (border art styles, shading patterns) are **retained as bounded strings**, matching the "opaque/retained where semantics are producer-specific" convention — not exploded into 40–180-variant enums. Structurally load-bearing vocabularies (tab alignment/leader, vertical text alignment) are typed closed enums like `Alignment`.
- Anything still not mapped returns `false` from `apply_paragraph_property` and is reported — never silently dropped.

## Slicing (cheap → expensive; each independently shippable)

| Slice | Tracker | Constructs | Import surface |
|---|---|---|---|
| **A — Flow control & levels** | P1A-033 | `keepNext`, `keepLines`, `pageBreakBefore`, `widowControl`, `suppressLineNumbers`, `contextualSpacing`, `bidi`, `outlineLvl`, `textAlignment` | `properties.rs` only — all flat elements; **no `body.rs` change** |
| **B — Paragraph shading** | P1A-034 | `w:shd` | `properties.rs` only — flat element; **no `body.rs` change**; introduces reusable `Shading` |
| **C — Paragraph borders** | P1A-035 | `w:pBdr` (`top`/`bottom`/`left`/`right`/`between`/`bar`) | container → new `pbdr_depth` in `body.rs`; introduces reusable `BorderSide` |
| **D — Custom tab stops** | P1A-036 | `w:tabs` → repeated `w:tab` | container → new `tabs_depth` in `body.rs` |

Slices A and B touch only `properties.rs` + `document.rs` (validation) because every element is a single self-closing tag with attributes, exactly like the already-mapped `jc`/`ind`/`spacing`. Slices C and D introduce **container** elements (children carry the data), so they add a depth counter to the flat quick-xml state machine, mirroring the existing `numPr`/`numpr_depth` pattern (`body.rs:670`). `Shading` (B) and `BorderSide` (C) are designed producer-neutral so the later table-property tail (gap #5: `tblBorders`/`shd`/`tcBorders`) and run-property tail (gap #3: run `w:shd`) reuse them unchanged.

---

## Slice A — Flow control & levels

### Model (`properties.rs`)

```rust
/// Vertical alignment of characters on each line (`w:textAlignment`, ST_TextAlignment).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlignment {
    /// `auto` — implementation default.
    Auto,
    /// `baseline`.
    Baseline,
    /// `bottom`.
    Bottom,
    /// `center`.
    Center,
    /// `top`.
    Top,
}
```

Fields appended to `ParagraphProperties`:

```rust
    /// Keep this paragraph on the same page as the next (`w:keepNext`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_next: Option<bool>,
    /// Keep all lines of this paragraph on one page (`w:keepLines`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_lines: Option<bool>,
    /// Start this paragraph on a new page (`w:pageBreakBefore`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_break_before: Option<bool>,
    /// Suppress widow/orphan control (`w:widowControl`; `false` disables it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widow_control: Option<bool>,
    /// Exclude this paragraph from line numbering (`w:suppressLineNumbers`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suppress_line_numbers: Option<bool>,
    /// Suppress spacing between paragraphs of the same style (`w:contextualSpacing`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contextual_spacing: Option<bool>,
    /// Right-to-left paragraph direction (`w:bidi`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bidi: Option<bool>,
    /// Outline (heading) level, `0..=9` (`w:outlineLvl`; 9 = body text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline_level: Option<u8>,
    /// Vertical alignment of text on the line (`w:textAlignment`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_alignment: Option<TextAlignment>,
```

| Field | Type | OOXML source | Domain |
|---|---|---|---|
| `keep_next` … `bidi` | `Option<bool>` | `CT_OnOff` toggles | none (bool is total); absent `w:val`→`true`, `0`/`false`/`off`→`false` |
| `outline_level` | `Option<u8>` | `w:outlineLvl w:val` (ST_DecimalNumber) | `0..=9` → `"paragraph.outline_level"` |
| `text_alignment` | `Option<TextAlignment>` | `w:textAlignment w:val` | closed enum; unknown token → report |

### Import (`apply_paragraph_property`)

Add arms; reuse the existing `is_true` helper (`properties.rs:124`) for toggles:

```rust
b"keepNext"            => properties.keep_next = Some(is_true(value.as_deref())),
b"keepLines"           => properties.keep_lines = Some(is_true(value.as_deref())),
b"pageBreakBefore"     => properties.page_break_before = Some(is_true(value.as_deref())),
b"widowControl"        => properties.widow_control = Some(is_true(value.as_deref())),
b"suppressLineNumbers" => properties.suppress_line_numbers = Some(is_true(value.as_deref())),
b"contextualSpacing"   => properties.contextual_spacing = Some(is_true(value.as_deref())),
b"bidi"                => properties.bidi = Some(is_true(value.as_deref())),
b"outlineLvl" => match value.as_deref().and_then(|v| v.parse::<u8>().ok()).filter(|l| *l <= 9) {
    Some(level) => properties.outline_level = Some(level),
    None => return false,          // out-of-domain/unparseable → reported
},
b"textAlignment" => match value.as_deref().and_then(text_alignment_from) {
    Some(alignment) => properties.text_alignment = Some(alignment),
    None => return false,
},
```

`value` is already bound at the top of `apply_paragraph_property`? — currently only `apply_run_property` binds `value`; add `let value = attribute_value(element, b"val");` at the top of `apply_paragraph_property` (harmless; `jc`/`ind`/`spacing` ignore it). New free fn `text_alignment_from(&str) -> Option<TextAlignment>` beside `alignment_from`.

### Validation (`check_paragraph_property_refs`)

```rust
if let Some(level) = properties.outline_level {
    check_domain(level <= 9, "paragraph.outline_level")?;
}
```
Toggles and the enum are self-bounding.

---

## Slice B — Paragraph shading (`w:shd`)

`w:shd` is a flat element: `<w:shd w:val="clear" w:color="auto" w:fill="D9D9D9"/>`. The dominant real-world use is a solid background fill; the pattern token (`ST_Shd`: `clear`, `solid`, `pct5…pct95`, `horzStripe`, …, ~40 values) is producer-specific, so it is **retained as a bounded token string** rather than enumerated — this keeps the *fill color modeled* regardless of pattern (no data loss for the common highlight case).

### Model (`properties.rs`) — reusable `Shading`

```rust
/// Background shading (`w:shd`). Reused by paragraph, and (future) table/run tails.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Shading {
    /// ST_Shd pattern token as written (`clear`, `solid`, `pct25`, …); `<= 32` bytes.
    pub pattern: ShadingPattern,
    /// Background fill color (`w:fill`); `auto`/absent omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<RgbColor>,
    /// Pattern foreground color (`w:color`); `auto`/absent omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<RgbColor>,
}

/// A shading pattern: the two structural cases typed, the ST_Shd long tail retained.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadingPattern {
    /// `clear` — no pattern; the fill shows through.
    Clear,
    /// `solid` — the foreground color fills the cell.
    Solid,
    /// Any other ST_Shd token (`pct25`, `horzStripe`, …), retained verbatim (`<= 32` bytes).
    Other(String),
}
```

`ShadingPattern` mirrors `Color`'s "typed common cases + retained tail" spirit. `Clear`/`Solid` serialize as `"clear"`/`"solid"`; `Other` as `{"other":"pct25"}` (externally tagged, consistent with serde defaults elsewhere). `Shading` is `Copy`-incompatible because of `Other(String)` → drop `Copy` (make it `Clone` only); if `Copy` is desired for later table reuse, model `pattern` as a bounded `String` instead. **Recommended:** keep the `Clone`-only enum for type safety.

Field appended to `ParagraphProperties`:
```rust
    /// Paragraph background shading (`w:shd`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shading: Option<Shading>,
```

### Import (`apply_paragraph_property`)

```rust
b"shd" => {
    let shading = Shading {
        pattern: shading_pattern_from(value.as_deref()),        // default Clear if absent
        fill:  attribute_value(element, b"fill").as_deref().and_then(parse_rgb),
        color: attribute_value(element, b"color").as_deref().and_then(parse_rgb),
    };
    // A no-op shd (clear pattern, no colors) carries nothing → report, don't model.
    if matches!(shading.pattern, ShadingPattern::Clear) && shading.fill.is_none() && shading.color.is_none() {
        return false;
    }
    properties.shading = Some(shading);
}
```
`parse_rgb` already returns `None` for `auto`/malformed (`properties.rs:157`), so `auto` colors omit cleanly. New free fn `shading_pattern_from` maps `clear`/`solid`/absent to the typed variants, else `Other(token)`.

### Validation
```rust
if let Some(shading) = &properties.shading {
    if let ShadingPattern::Other(token) = &shading.pattern {
        check_domain(!token.is_empty() && token.len() <= 32, "paragraph.shading.pattern")?;
    }
}
```
`RgbColor` channels are inherently bounded.

---

## Slice C — Paragraph borders (`w:pBdr`)

`w:pBdr` is a **container**; each side (`w:top`, `w:left`/`w:start`, `w:bottom`, `w:right`/`w:end`, `w:between`, `w:bar`) is a `CT_Border` child with `w:val` (ST_Border, ~180 art-border tokens → retained), `w:sz` (eighths of a point), `w:space` (points), `w:color`.

### Model (`properties.rs`) — reusable `BorderSide`

```rust
/// One border edge (`CT_Border`). Reused by paragraph, and (future) table/cell tails.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BorderSide {
    /// ST_Border line-style token as written (`single`, `thick`, `dashed`, …); `<= 32` bytes.
    pub style: String,
    /// Line width in eighths of a point (`w:sz`); `0..=1020`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_eighths: Option<u16>,
    /// Padding from text in points (`w:space`); `0..=31`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_points: Option<u16>,
    /// Line color (`w:color`); `auto`/absent omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<RgbColor>,
}

/// Paragraph borders (`w:pBdr`). Each edge is optional.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParagraphBorders {
    #[serde(skip_serializing_if = "Option::is_none")] pub top: Option<BorderSide>,
    #[serde(skip_serializing_if = "Option::is_none")] pub bottom: Option<BorderSide>,
    /// Leading edge (`w:left`/`w:start`).
    #[serde(skip_serializing_if = "Option::is_none")] pub start: Option<BorderSide>,
    /// Trailing edge (`w:right`/`w:end`).
    #[serde(skip_serializing_if = "Option::is_none")] pub end: Option<BorderSide>,
    /// Between consecutive same-property paragraphs (`w:between`).
    #[serde(skip_serializing_if = "Option::is_none")] pub between: Option<BorderSide>,
    /// Vertical bar to the left of the paragraph (`w:bar`).
    #[serde(skip_serializing_if = "Option::is_none")] pub bar: Option<BorderSide>,
}
```
Field appended to `ParagraphProperties`:
```rust
    /// Paragraph borders (`w:pBdr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub borders: Option<ParagraphBorders>,
```

Domains: `style` token ≤ 32 bytes; `size_eighths` `0..=1020` (≈127 pt, well above any real border; over-range → the side is skipped and reported); `space_points` `0..=31` (spec cap). Sides map `left→start`, `right→end` (transitional/strict alias, mirroring `indent_attr`'s `&[b"start", b"left"]` at `properties.rs:128`).

### Import (`body.rs` — container depth, mirrors `numPr`)

Add `pbdr_depth: u32` to `BodyParser` and to `ContentFrame` save/restore (same places `ppr_depth`/`numpr_depth` live). `on_start`:
```rust
b"pBdr" if self.ppr_depth > 0 => self.pbdr_depth += 1,
b"top" | b"bottom" | b"left" | b"right" | b"start" | b"end" | b"between" | b"bar"
    if self.pbdr_depth > 0 =>
{
    match border_side(element) {                       // -> Option<BorderSide>
        Some(side) => set_paragraph_border(&mut self.paragraph_properties, local, side),
        None => self.reporter.report(local),           // no line-style / bad → reported
    }
}
```
`on_end`: `b"pBdr" => self.pbdr_depth = self.pbdr_depth.saturating_sub(1)`.

The `pbdr_depth > 0` guard is **load-bearing**: `top`/`bottom`/`left`/`right` also name table-cell-margin and cell-border children, but those occur under `tcPr`/`tblPr` (never inside a `pPr>pBdr`), so scoping by `pbdr_depth` prevents cross-family misrouting. These arms must sit **above** the generic `_ if self.ppr_depth > 0` arm (`body.rs:1005`) in match order. `border_side`/`set_paragraph_border` are new helpers in `properties.rs` (parse `val`/`sz`/`space`/`color`; drop-and-report a side with no usable `w:val`).

### Validation
```rust
if let Some(borders) = &properties.borders {
    for side in [&borders.top, &borders.bottom, &borders.start,
                 &borders.end, &borders.between, &borders.bar].into_iter().flatten() {
        check_domain(!side.style.is_empty() && side.style.len() <= 32, "paragraph.border.style")?;
        if let Some(sz) = side.size_eighths  { check_domain(sz <= 1020, "paragraph.border.size")?; }
        if let Some(sp) = side.space_points  { check_domain(sp <= 31,   "paragraph.border.space")?; }
    }
}
```

---

## Slice D — Custom tab stops (`w:tabs`)

`w:tabs` is a **container** of repeated `w:tab` (`CT_TabStop`): `w:val` (ST_TabJc alignment), `w:pos` (signed twips), `w:leader` (ST_TabTlc). Both vocabularies are small and structurally load-bearing → typed closed enums.

### Model (`properties.rs`)

```rust
/// A custom tab stop's alignment (`w:tab@w:val`, ST_TabJc).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TabAlignment { Start, Center, End, Decimal, Bar, Clear }

/// A tab-stop leader fill (`w:tab@w:leader`, ST_TabTlc).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TabLeader { None, Dot, Hyphen, Underscore, Heavy, MiddleDot }

/// One custom tab stop (`w:tab`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TabStop {
    /// Alignment at the stop.
    pub alignment: TabAlignment,
    /// Position from the leading margin in twips (`w:pos`); `-31_680..=31_680`.
    pub position_twips: i32,
    /// Leader fill; `none`/absent omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leader: Option<TabLeader>,
}
```
Field appended to `ParagraphProperties`:
```rust
    /// Custom tab stops (`w:tabs`), in document order; `<= 64`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<TabStop>,
```
`val="left"→Start`, `right→End` (transitional aliases). `val="num"` (legacy list tab) → mapped to `Clear` is wrong; treat `num`/unknown → skip-and-report that stop.

### Import (`body.rs` — container depth)

Add `tabs_depth: u32` (BodyParser + ContentFrame). `on_start`:
```rust
b"tabs" if self.ppr_depth > 0 => self.tabs_depth += 1,
b"tab" if self.tabs_depth > 0 => match tab_stop(element) {   // -> Option<TabStop>
    Some(stop) => self.paragraph_properties.tabs.push(stop),
    None => self.reporter.report(b"tab"),
},
```
`on_end`: `b"tabs" => self.tabs_depth = self.tabs_depth.saturating_sub(1)`.

**Disambiguation hazard:** `w:tab` is overloaded — inside a run it is the *tab character* already mapped to `InlineNode::Tab` (handled under `run_open`). The property `w:tab` occurs under `pPr>tabs` with the run closed. The two contexts are disjoint (`tabs_depth > 0` vs `run_open`); the `tabs_depth` arm must be ordered before any `run_open` tab arm and both above the generic ppr report arm.

### Validation
```rust
check_domain(properties.tabs.len() <= 64, "paragraph.tabs.count")?;
for stop in &properties.tabs {
    check_domain((-31_680..=31_680).contains(&stop.position_twips), "paragraph.tab.position")?;
}
```

---

## Backward-compatibility (all four slices)

- **Byte-identical existing snapshots.** Every new field is `Option`/`Vec` with `#[serde(default, skip_serializing_if = …)]`; `ParagraphProperties` keeps `deny_unknown_fields` + `camelCase`, and an empty value still serializes to `{}`. No existing key's bytes change; no already-authored snapshot ever emits a new key.
- **v0→v1 migration golden unchanged.** v0 carries none of these constructs (`migration.rs` produces empty long-tail fields → all skipped). The byte-exact migration golden is untouched.
- **No `error.rs` change.** All out-of-domain cases reuse `ModelError::PropertyValueOutOfDomain { property }` with new stable strings (`paragraph.outline_level`, `paragraph.shading.pattern`, `paragraph.border.style|size|space`, `paragraph.tabs.count`, `paragraph.tab.position`).
- **No `ids.rs`/`definitions.rs` change.** These are inline property values, not cross-referenced definitions — no `NodeId`, no `DefinitionMap`.
- **No silent loss preserved.** Unmapped tokens/out-of-domain values return `false` (flat elements) or hit `reporter.report(local)` (container children), so they still surface in the compatibility report exactly as today.
- **Determinism.** `tabs`/border sides are captured in document order; no id allocation involved.

## Explicitly deferred (reported, not modeled)

Each still reaches the `_ if self.ppr_depth > 0 => report` arm (or a container's child report), so it is dispositioned, never silently dropped. Follow-up slices:

- **Paragraph-mark run properties** (`w:pPr>w:rPr`) — formatting of the paragraph mark itself; needs a paragraph-node field (also noted deferred by the tracked-changes slice `48-…`).
- **Section break in `pPr`** (`w:pPr>w:sectPr`) — belongs to section handling, not this tail.
- **Frames** (`w:framePr`) — text-frame geometry, a distinct feature.
- **East-Asian / typographic toggle tail** — `w:snapToGrid`, `w:wordWrap`, `w:overflowPunct`, `w:topLinePunct`, `w:autoSpaceDE`/`DN`, `w:kinsoku`, `w:suppressAutoHyphens`, `w:mirrorIndents`, `w:adjustRightInd`, `w:suppressOverlap`. Cheap to fold into a future extension of Slice A when a corpus needs them.
- **Conditional table formatting** (`w:cnfStyle`) and **`w:divId`** — table/HTML-import provenance.
- **`w:pPrChange`** (property-change revision) — already deferred by the tracked-changes slice.
- **Tab `w:val="num"`** legacy list tabs — reported per-stop.

## Test plan

Follow the tracked-changes slice's model+import+walker structure.

- **Model (`v1/tests.rs`):** round-trip each new field (toggles true/false, `outline_level` 0/5/9, `text_alignment` each variant, `shading` clear-with-fill / solid / `Other("pct25")`, all six border sides with size/space/color, `tabs` with each alignment+leader); reject `outline_level = 10`, oversized shading token/border style (>32), `border.size > 1020`, `border.space > 31`, `tab.position` beyond ±31680, `> 64` tabs; confirm empty `ParagraphProperties` still serializes to `{}`; assert a fixed pre-existing snapshot is byte-identical (guards `skip_serializing_if`).
- **Import (`import/tests.rs`):** a `w:pPr` exercising every construct maps to the expected model; `keepNext`/`widowControl`/etc. with and without `w:val`; `w:shd` with `auto` fill omits the color but still models a solid pattern; a no-op `clear` `w:shd` is reported not modeled; `w:left`/`w:start` alias to `start`; a `w:pBdr` side with no `w:val` is reported; a run-level `w:tab` (tab character) still yields `InlineNode::Tab` while a `pPr>tabs>tab` yields a `TabStop` (the disambiguation regression guard); `num`/unknown tab val reported; over-domain `outlineLvl`/border size reported not modeled; verify each reaches the Reporter (no silent drop).
- **Migration:** re-run the v0→v1 byte-exact golden — unchanged.
- **Walkers:** extend the fidelity walker (`tools/opendoc-fidelity`) and export-presence walker to descend `ParagraphProperties` long-tail fields.
- **Gates:** fmt, clippy, unit, doctest, wasm, MSRV 1.85, doc — as in prior slices.

## CHANGELOG (Unreleased → Added)

> Paragraph-property long tail in schema v1: keepNext/keepLines/pageBreakBefore/widowControl/suppressLineNumbers/contextualSpacing/bidi toggles, outline level, and vertical text alignment (Slice A); paragraph shading `w:shd` via a reusable `Shading` value (Slice B); paragraph borders `w:pBdr` via a reusable `BorderSide` value (Slice C); custom tab stops `w:tabs` (Slice D). Producer-specific border/shading vocabularies are retained as bounded tokens; the remaining `pPr` tail stays reported-not-modeled. Additive: existing snapshots and the v0→v1 migration golden are byte-identical.

---

**Design notes surfaced for owner discussion before implementation:**
1. `ShadingPattern::Other(String)` forces `Shading` to be `Clone`-only (not `Copy`). If the future table tail wants `Copy` table props, model `pattern` as a bounded `String` instead — flagged where it matters.
2. Border `size_eighths` bound `0..=1020` is deliberately generous (Word UI caps ~6 pt); tighten to `0..=255` if the corpus shows nothing larger, at the cost of possibly reporting exotic art borders.
3. Slices C/D each add one `u32` depth field to `BodyParser` **and** `ContentFrame` (save/restore) — the only state-machine surface touched; A/B touch none.