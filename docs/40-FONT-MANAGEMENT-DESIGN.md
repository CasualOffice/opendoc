# 40 — Font Management Design

Status: Draft for review. Owner: OpenDoc core. Depends on: 28 (Package Reader), 34 (Fidelity Architecture), 35 (Disposition Taxonomy), 38 (Schema v1), 39 (Semantic Writer). Feeds: Phase 1B layout/resolution and the `FontProvider` interface named in 03-HLD.md l.178 and 06-ROADMAP l.97/101/290.

## 1. Goals and non-goals

### Goals
- **G1 (Phase 1A — model + round-trip, no loss).** Represent every OOXML font construct as a first-class typed value so a semantic re-serialize reproduces it: the four `w:rFonts` slots with the direct-vs-theme distinction and the full 8-value theme enum, `w:hint`, the `fontTable.xml` descriptors (`panose1`, `charset`, `family`, `pitch`, `sig`, `altName`, `notTrueType`), the theme `a:fontScheme` (major/minor × latin/ea/cs + per-script overrides), embedded-font relations (`embedRegular/Bold/Italic/BoldItalic` + `fontKey` + `subsetted`), the de-obfuscated `.odttf` bytes, and the three `settings.xml` embedding flags.
- **G2 (Phase 1B+ — resolution).** From the modeled data plus an explicit, host-supplied face index, deterministically resolve a run to a concrete face and per-glyph metrics, with CSS-Fonts-4-style substitution/fallback when a requested family is unavailable — identical output on native and `wasm32-unknown-unknown`.
- **G3.** Host-independence: no ambient OS font database, no fontconfig, no browser DOM on the deterministic path. System enumeration is an opt-in, native-feature-gated input, never the core.
- **G4.** Security-bounded: `.odttf` and embedded fonts are attacker-controlled OpenType; every parse is size/structure-bounded and non-panicking.

### Non-goals
- No layout, line-breaking, shaping, or rasterization in this document (Phase 1B/1C). This design only makes fonts *modelable, round-trippable, and resolvable-to-a-face-and-metrics*.
- No glyph subsetting/rewriting on export in Phase 1A (embedded bytes are preserved verbatim; `write-fonts` deferred).
- No font UI, no font-substitution editor UI.

## 2. Prior-art synthesis

| System | Resolution stance | Embedded fonts | Determinism | Lesson for OpenDoc |
|---|---|---|---|---|
| **Word / OOXML** | Runtime PANOSE-1 match + `altName` + Windows `FontSubstitutes` registry | `.odttf`, reversed-GUID XOR over first 32 bytes, gated by `settings.xml` + OS/2 `fsType` | Non-deterministic (per-machine substitutes) | The file carries only *hints* (`altName`, `panose1`, `sig`, `family`, `pitch`); the resolved substitute is never serialized. Model the hints, own the matcher. |
| **LibreOffice (VCL)** | Layered: user Font Replacement Table → fontconfig → `VCL.xcu` lists → per-glyph fallback | Lossy DOCX interop, embedding is an internal toggle not a pass-through | Environment-dependent (fontconfig) | Anti-pattern for G3: never route through an ambient OS DB. But *adopt* its two-tier split — whole-face substitution vs per-glyph coverage fallback — and its metric-compatible replacement policy (Liberation/Carlito/Caladea). |
| **ONLYOFFICE** | Bundled `core-fonts` + precompiled `font_selection.bin` metrics index via offline `allfontsgen` | Historically *ignored* embedded fonts (issue #228) | Deterministic for a given bundle | *Adopt* the offline precompiled-metrics-index pattern for a versioned bundled fallback set. *Reject* ignoring embedded fonts — that is silent loss by our standard. Always prefer the document's own faces. |
| **CSS Fonts 4 §5** | Fixed 3-axis nearest match (width→style→weight), per-family then per-character fall-through, generics + last-resort tail | `@font-face src` ordered list (`local()`/`url()`/`format()`/`tech()`), `unicode-range` | Fully deterministic given the face set (integer/enum axes, specified tie-breaks) | The canonical algorithm to mirror verbatim. Integer axes → bit-exact on native and wasm. The one non-portable input is *the set of available faces*, which we make an explicit argument. |

**Synthesis.** The correct OpenDoc design combines: ONLYOFFICE's precompiled-deterministic bundled metrics index + LibreOffice's two-tier substitution/fallback shape + CSS Fonts 4's exact matching algorithm + the embedded-font correctness that all three products lack. Resolution becomes a pure function `fn(request, &FaceIndex) -> FaceMatch` where `FaceIndex = document-embedded ∪ bundled-fallback ∪ (native-only) host-enumerated`, and document-embedded faces always win.

## 3. The OpenDoc font data model (Phase 1A)

The current model (`crates/casual-doc-model/src/v1/properties.rs`) understands fonts only as a run-level reference layer, and collapses too much: `FontRef = Theme(ThemeFont{Major|Minor}) | Named(FontName)` drops (a) which of the 4 slots a theme value came from, (b) the Ascii/HAnsi/EastAsia/Bidi sub-axis of the theme enum, and (c) `w:hint`. `ThemeReferences` (definitions.rs l.167) exists but is orphaned — not a field of `Definitions` or `Document`, never populated. There is no `fontTable.xml`, no `theme1.xml` `fontScheme`, no embedded-font model. This section closes those gaps. All additions are **additive** (new fields `#[serde(skip_serializing_if=...)]` / new `DefinitionMap`s omitted when empty) so existing v1 snapshots serialize byte-identically, matching the established schema-v1 evolution rule.

### 3.1 Run-level: fix the `w:rFonts` collapse

Replace the 2-variant theme enum with the full OOXML vocabulary and add `hint`:

```
enum ThemeFontRef {                    // ECMA-376 §17.3.2.26 theme enum (8 values)
    MajorAscii, MajorHAnsi, MajorEastAsia, MajorBidi,
    MinorAscii, MinorHAnsi, MinorEastAsia, MinorBidi,
}
enum RFontHint { Default, EastAsia, Cs }   // w:hint

// Per-slot, direct XOR theme are mutually exclusive per the spec:
enum SlotFont { Direct(FontName), Theme(ThemeFontRef) }

struct RunFonts {                       // replaces the 4 flat Option<FontRef> fields
    ascii:      Option<SlotFont>,       // w:ascii / w:asciiTheme
    h_ansi:     Option<SlotFont>,       // w:hAnsi / w:hAnsiTheme
    east_asia:  Option<SlotFont>,       // w:eastAsia / w:eastAsiaTheme
    cs:         Option<SlotFont>,       // w:cs / w:cstheme
    hint:       Option<RFontHint>,      // w:hint  (was reported-only)
}
```

This makes `rFonts@hint`-only elements (previously routed to `CompatibilityReport`, import/properties.rs l.46-56) losslessly modeled, and preserves per-script theme granularity that the resolver needs. Importer `theme_font_ref()` (properties.rs l.135) must map the exact suffix instead of the `major`/`minor`-prefix collapse; a token outside the 8-value enum still falls through to `Direct` (keep the `rfonts_bogus_theme_falls_through` regression, tests.rs:280).

### 3.2 Definition-level: the font table

Add a name-keyed `FontTable` to `Definitions` (definitions.rs l.235). Keyed by `w:font/@w:name`; **preserve entries even when unreferenced** (Word emits stale entries). Store PANOSE and sig as **opaque fixed-width bytes**, never a parsed struct that could drop unknown bits.

```
struct FontDescriptor {                 // one w:font, ECMA-376 §17.8
    name: FontName,                     // @w:name (map key)
    alt_name: Option<FontName>,         // w:altName@w:val  (authored substitution hint)
    panose1: Option<[u8;10]>,           // w:panose1  (raw 20-hex → 10 bytes, opaque)
    charset: Option<u8>,                // w:charset  (Windows charset byte, hex)
    family: Option<FontFamilyKind>,     // auto|decorative|modern|roman|script|swiss
    pitch: Option<FontPitch>,           // default|fixed|variable
    sig: Option<FontSig>,               // usb0..3 + csb0..1 (six u32, OS/2 coverage bits)
    not_true_type: bool,                // w:notTrueType
    embed: Option<EmbeddedFontSet>,     // §3.3
}
struct FontSig { usb0:u32, usb1:u32, usb2:u32, usb3:u32, csb0:u32, csb1:u32 }
```

### 3.3 Embedded fonts (de-obfuscation)

```
struct EmbeddedFontSet {                // up to 4 faces under one w:font
    regular:      Option<EmbeddedFace>, // w:embedRegular
    bold:         Option<EmbeddedFace>,
    italic:       Option<EmbeddedFace>,
    bold_italic:  Option<EmbeddedFace>,
}
struct EmbeddedFace {
    font_key: FontKey,        // {GUID} de-obfuscation key (parsed + original string kept)
    subsetted: bool,          // w:subsetted
    media_id: MediaId,        // → MediaReference-like entry holding the OpenType bytes
}
```

**De-obfuscation (own it, no crate).** Per ECMA-376 Part 4 / MS-OE376 §15.2.12: strip braces/dashes from `fontKey` → 32 hex chars → 16 bytes `b[0..15]` in string order; reverse to `r[k]=b[15-k]`; for `i in 0..32`, `font[i] ^= r[i mod 16]`; bytes 32..end untouched; the transform is its own inverse. ~10 lines, no_std, deterministic, zero deps. The worked example (GUID `001B70DC-AA60-4AD5-90EC-18A0948E1EAE`) reverses to `AE 1E 8E 94 A0 18 EC 90 D5 4A 60 AA DC 70 1B 00`. **Use plain hex-string reversal, not .NET `Guid.ToByteArray()` mixed-endian** — the c-rex example and python-docx/POI agree; validate against a real Word `.odttf` fixture in Phase 1B (open question). Reject malformed GUIDs (no panic). After de-obfuscation, validate the sfnt version (`0x00010000`, `OTTO`, `true`, `ttcf`) before trusting the buffer — G4.

**Storage / round-trip decision.** Store the **de-obfuscated OpenType bytes** in the model (so the resolver can parse them directly) plus the original `fontKey`. On semantic write, **re-obfuscate with the preserved `fontKey`** → byte-identical `.odttf`. Do **not** regenerate a GUID (would break byte-identical round-trip). This keeps decode-in-core (not handed raw to the host), satisfying G2/G4 and avoiding a host round-trip. The bytes are bounded by the existing package size/ratio limits (28-DOCX-PACKAGE-READER, 21-PARSER-LIMITS).

**Package plumbing (a known-bug guard).** Embedded-font `r:id`s resolve through `word/_rels/fontTable.xml.rels`, **not** the document `.rels`. Content type `application/vnd.openxmlformats-officedocument.obfuscatedFont`, usually a `<Default Extension="odttf">`. The importer must add `/fontTable` and `/theme` to its relationship-type resolution (currently absent: import/lib.rs l.80-88 handles only styles/notes/comments/image/header/footer).

### 3.4 Theme font scheme

Wire the orphaned `ThemeReferences` into `Definitions` and **extend it** to hold the full scheme so `Theme(ThemeFontRef)` slots are resolvable:

```
struct FontScheme {                       // theme1.xml a:fontScheme, §20.1.4.1.18
    major: FontCollection,                // a:majorFont
    minor: FontCollection,                // a:minorFont
}
struct FontCollection {
    latin: ThemeFontEntry,                // a:latin  (@typeface + optional @panose/@pitchFamily/@charset)
    ea:    ThemeFontEntry,                // a:ea     (empty typeface ⇒ fall back to latin)
    cs:    ThemeFontEntry,                // a:cs
    script_overrides: Vec<ScriptFont>,    // <a:font script="Hans" typeface="..."/> (unbounded map)
}
struct ScriptFont { script: String, typeface: String }   // ISO-15924 tag → family
```

Resolution matrix: `asciiTheme=minorHAnsi → minor.latin.typeface`, `eastAsiaTheme=minorEastAsia → minor.ea.typeface`, `cstheme=minorBidi → minor.cs.typeface`; `major*` analogous. Empty `ea/cs` typeface ⇒ use `latin`. Preserve `script_overrides` verbatim (needed to turn a theme reference into a concrete family before any face lookup).

### 3.5 Settings-level embedding flags

Round-trip the three `settings.xml` booleans (§17.15.1): `w:embedTrueTypeFonts`, `w:embedSystemFonts`, `w:saveSubsetFonts`. They are document fidelity and govern what a faithful writer may re-embed. Store on the existing settings/defaults surface (`document_defaults`, definitions.rs l.272).

## 4. Resolution / matching / fallback (Phase 1B+)

A single pure entry point, host-independent, wasm-safe:

```
fn resolve(request: FaceRequest, index: &FaceIndex) -> FaceMatch
```

- **`FaceRequest`** = theme-resolved family list + weight (u16, OS/2 `usWeightClass` 1..1000) + width (u16, `usWidthClass` 1..9) + style (enum italic/oblique(angle)/normal from `fsSelection` bits) + the codepoint being rendered + `altName` + `panose1`/`sig` hints.
- **`FaceIndex`** = an explicit, content-addressed list of available faces built from three tiers, in priority order:
  1. **document-embedded** faces (from §3.3) — *always preferred* (the correctness both LO/ONLYOFFICE lack);
  2. **bundled fallback set** — a versioned, pinned set with a precompiled metrics/coverage index (ONLYOFFICE `allfontsgen` pattern), shipped for wasm where no system fonts exist;
  3. **host-enumerated** system faces — **native-only, feature-gated**, an *opt-in, explicitly non-deterministic* layer, excluded from golden tests and from the wasm build.

**Two-tier fallback (from LibreOffice), both deterministic given the index:**
- **Whole-face substitution** when a named family is absent: try `altName` as a family candidate first, then PANOSE-distance nearest-match (classify via PANOSE `FamilyType`/`SerifStyle`/`Proportion` into a serif/sans/mono/script bucket) filtered by `sig` script coverage, then a bundled metric-compatible default (Liberation Sans/Serif/Mono ↔ Arial/Times/Courier; Carlito ↔ Calibri; Caladea ↔ Cambria). Record whether the chosen substitution was **metric-compatible** (layout-preserving) vs **visual-only** on the disposition surface (35-DISPOSITION-TAXONOMY) — loss-aware honesty, not silent swap.
- **Per-glyph coverage fallback** when a present face lacks a glyph: fall through to the next candidate face that covers the codepoint. Replicate the concrete correctness rules: exclude PUA codepoints from generic glyph fallback; check the glyph actually exists (cmap) before substituting.

**Per-family style match = CSS Fonts 4 §5.2 verbatim**, three cascaded *filters* (never a blended distance): width first, then style, then weight. Encode the weight rule exactly (the 400/500 special case): target in `400..=500` searches up to 500, then down, then above; `<400` down then up; `>500` up then down. Because all axes are small integers/enums, two implementations produce bit-identical choices → G2 determinism on native and wasm with no float/locale drift. Terminate the family cascade with a generic bucket then a last-resort face so `resolve` is total.

**Metrics.** Read `units-per-em`, ascender/descender/line-gap (honor OS/2 `fsSelection` bit7 USE_TYPO_METRICS), and per-glyph advances from the resolved face via the parsing/metrics crate (§5). These feed Phase 1C layout, not this doc.

## 5. Recommended Rust crate stack (minimal defensible)

Workspace currently has **zero** font crates (only `quick-xml 0.41`, `zip =7.2.0`) — clean slate. Add a new **`casual-doc-font`** crate (or a feature-gated module in `casual-doc-model`) so the pure-model path stays dependency-light.

**Phase 1A (model + round-trip) — parsing/metrics core, no system deps:**
- **`ttf-parser`** (MIT/Apache-2.0, harfbuzz org, no_std, zero-alloc, wasm-clean) as the **baseline** descriptor/metrics reader: `Face::names()`, `tables().os2` (weight/width/`fsSelection`/PANOSE), cmap coverage, metrics — minimal transitive deps, auditable, security-bounded. Sufficient to parse de-obfuscated embedded fonts and read descriptors.
- **De-obfuscation + re-obfuscation: no crate** — ~10 lines in `casual-doc-font`, no_std, deterministic (§3.3).

**Phase 1B+ (resolution/metrics depth) — upgrade only if needed:**
- **`read-fonts` + `skrifa`** (MIT/Apache-2.0, Google fontations, no_std, wasm-safe, now shipping in Chrome) if variable-font / color-font / richer localized-name depth is required beyond `ttf-parser`. `skrifa`'s `MetadataProvider` exposes attributes (stretch/style/weight), localized names, units-per-em, global + per-glyph metrics with variation support. Prefer as the metrics substrate for layout; keep `ttf-parser` if depth is unneeded.
- **`fontdb`** (MIT, RazrFalcon/resvg, wasm-compilable, does **not** call OS APIs) as the lightweight face-index + CSS-like query layer for the bundled/embedded tiers. Its matching is CSS-*inspired*, **not** verbatim §5.2 — verify the weight tie-break or wrap it with our own §5.2 comparator to guarantee determinism.
- **`fontique`** (MIT/Apache-2.0, Linebender/parley) *only when* true script/codepoint fallback is needed; its system enumeration backends (Core Text/DirectWrite/`fontconfig-dlopen`) must be **feature-gated native-only** and never enter the wasm build. Still 0.x (API churn).

**Native-only, feature-gated (Tauri/desktop tier-3 enumeration):**
- **`font-kit`** (MIT/Apache-2.0, servo) *only* if Word-parity OS-DB matching is wanted on desktop. Binds Core Text/DirectWrite/FreeType+fontconfig — **not wasm-viable**, violates G3 on the core path; strictly an optional native backend behind a cargo feature.

**Explicitly avoided:** `harfbuzz_rs` (C FFI, breaks pure-Rust/wasm). Shaping (`rustybuzz` vs the fontations-native `HarfRust`) is a Phase 1C decision, out of scope here — but if/when needed prefer `HarfRust` (cosmic-text already migrated) to avoid a later port.

**License posture:** all recommended crates are MIT and/or Apache-2.0 (permissive, pass the repo `deny.toml`). `allsorts` (Apache-2.0-only; would only be pulled for WOFF2 — not expected in `.odttf`) and `harfbuzz_rs` are the only entries needing policy review, and neither is in the recommended set.

## 6. Phased implementation plan

**Phase 1A.1 — run-slot fidelity (smallest, highest-leverage).** Replace the 2-variant theme enum with the 8-value `ThemeFontRef`, introduce `RunFonts` with `hint`, update importer `theme_font_ref()` + `font_slot()`, and **emit `w:rFonts` in the semantic writer** (`semantic.rs` `write_run_properties` l.569 currently emits none). Fixture: mixed-script run with `asciiTheme`+`eastAsia` direct+`hint` round-trips.

**Phase 1A.2 — fontTable + theme parts.** Add `FontTable`/`FontDescriptor` and `FontScheme`; wire `ThemeReferences`/scheme into `Definitions`; add `/fontTable` + `/theme` relationship resolution to the importer (resolving embed `r:id`s through `fontTable.xml.rels`); write the `fontTable.xml` + `theme1.xml` parts + content-types + relationships in the semantic writer. Fixtures: descriptor with `panose1`/`sig`/`altName`/`family`/`pitch` survives; theme scheme with per-script overrides survives.

**Phase 1A.3 — embedded fonts.** De-obfuscation/re-obfuscation routine + sfnt validation; `EmbeddedFontSet`/`EmbeddedFace`; store de-obfuscated bytes + `fontKey`; round-trip all four faces byte-identically (the LO/ONLYOFFICE interop gap turned into a passing fixture). Round-trip the three `settings.xml` embedding flags.

Until 1A.2/1A.3 land, Retention (byte-floor) mode already preserves `fontTable.xml`/`theme1.xml`/`.odttf` verbatim (package.rs admits every non-macro part) — so the *no-edit* case is safe today; the work closes the *semantic-write* gap.

**Phase 1B — resolution.** `FontProvider` trait (the boundary named in 03-HLD.md l.178) + `FaceIndex` (three tiers) + `resolve()` implementing §5.2 verbatim + PANOSE/altName substitution + per-glyph fallback. Bundled fallback set + precompiled metrics index (Liberation/Carlito/Caladea). Golden tests snapshot a fixed `FaceIndex` (embedded+bundled only); host enumeration excluded.

**Phase 1C+ (out of scope here).** Shaping + layout consume the resolved faces/metrics.

## 7. Determinism, security, loss-awareness

- **Determinism (G2/G3).** Resolution is `fn(request, &FaceIndex)`; the only non-portable input is the face *set*, made an explicit argument. Golden/headless runs pin a `FaceIndex` of embedded+bundled faces; system enumeration is an opt-in native layer excluded from tests. All match axes are integers/enums → bit-exact across native and wasm. Bundled set + metrics index are **versioned/pinned**.
- **Security (G4).** `.odttf` and embedded fonts are untrusted OpenType: bounded by existing package size/ratio limits before decode; GUID parse rejects malformed keys without panic; sfnt version validated post-de-obfuscation; `ttf-parser`'s zero-alloc bounded parsing is the trust boundary. OS/2 `fsType` of a resolved/embedded face governs whether OpenDoc may legally use it for print vs edit — surfaced, not silently ignored, for headless export.
- **Loss-awareness (G1/no silent loss).** Every construct in §3 is modeled first-class; nothing font-related should reach `CompatibilityReport` as "unmapped" once 1A lands. Substitution outcomes (which face chosen, metric-compatible vs visual-only, whether an embedded face was used vs a fallback) are reported on the disposition surface (35-DISPOSITION-TAXONOMY), mirroring ONLYOFFICE's honesty rather than swapping silently. Embedded bytes preserved verbatim via `fontKey` re-obfuscation.

## 8. Open questions
1. `fontKey` parsing: confirm plain hex-string reversal (vs .NET mixed-endian) against a real Word-produced `.odttf` fixture in 1A.3. Evidence favors hex-string reversal.
2. Re-obfuscation policy is *preserve `fontKey` verbatim* (recommended) — confirm no scenario requires GUID regeneration.
3. Exact Unicode-range→slot boundaries Word uses for ascii/hAnsi/eastAsia/cs selection and how `hint` alters them — extract a precise table from §17.3.2.26 for the resolver (needed for correct per-character slot selection, Phase 1B).
4. Deterministic PANOSE/OS-2 → serif/sans/mono/script bucket heuristic — no spec dictates it; needs a documented, testable mapping.
5. `altName` priority in the CSS-style cascade: recommended as a *higher-priority family candidate* before PANOSE/generic fallback — confirm.
6. Bundled fallback set contents (size vs coverage) for the wasm binary; whether to ship metrics-only descriptors (no outlines) to keep wasm small.
7. Whether to standardize on `fontdb` alone or add `fontique` for script fallback; and whether `ttf-parser` suffices for byte-exact descriptor round-trip or `read-fonts` low-level access is needed for every OS/2/name field.
8. Confirm current ONLYOFFICE (8.x) has/has-not added embedded-font rendering (sources predate); does not change our design (we always prefer embedded) but worth tracking.
