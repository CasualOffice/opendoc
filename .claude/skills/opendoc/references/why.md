# Why the rules exist — evidence behind `SKILL.md`

Keyed by the `SKILL.md` section numbers. Load only when you need to argue for a rule,
extend one, or check whether a past incident applies.

## §1 Competitive position

**Why Apache-2.0 is the whole wedge.** ONLYOFFICE is AGPL-3.0-only with assets under
CC-BY-SA-4.0, commercial tiers from $1,500/$3,500, Developer Edition billed per concurrent
browser tab, and the host-customisation API gated in code (`LayoutManager._applyCustomization`
early-returns when `!_licensed`). Permissive licence + real DOCX fidelity + embeddability
is an unoccupied position.

**The three structural advantages:**

1. Local-first. Their web client cannot open a file offline: format I/O is `x2t`, and
   `core/X2tConverter/build/` has only `Android/` and `Qt/`, no WASM build.
2. Direct OOXML with verbatim retention. They convert `DOCX → Editor.bin → DOCX`, so
   whatever the intermediate model lacks is dropped. Ours is only real if loss is detected
   and reported, so loss-reporting is competitive work, not hygiene.
3. No mandatory server. Any relay is additive and optional.

**Facts commonly reported wrong:**

- Their co-editing is neither OT nor a CRDT (zero `transform` hits in `DocsCoServer.js`):
  a server-ordered change log + pessimistic object locks + client rollback/replay over
  socket.io.
- Spell check is client-side WASM (`spell.wasm`); their server service is retired.
- Text input is a hidden `<textarea>` (id matches `/area_id/`); zero functional
  `contenteditable` in 171 KLOC.
- Gaps they have: no table sorting, no decimal/bar tab stops, only 14 field codes, no
  page-borders dialog, no accessibility checker, no native citation manager, SmartArt is
  formatting-only. We ship table sorting and decimal tabs.

## §2 OT

`casual-doc-wasm` references `casual_doc_transaction` zero times and applies
`casual-doc-edit`'s 47 ops directly with a flat undo stack and no revision chain.

## §3 Gates

- A formatting failure reached CI looking green locally because `fmt | tail && echo OK`
  printed OK (#552).
- Plain `cargo fmt` passes locally and fails CI; the pinned toolchain formats differently.
- Public doc links to private items failed `cargo doc` on three branches in one session.
- `build.sh` was assumed to be a superset of every generator's check. Measured 2026-10-04,
  it is not: `grep -n glossary webapp/build.sh` returns nothing, and `build.sh` passed while
  the committed glossary was stale. The glossary derives from every `docs/*.md`, so any
  prose sentence in a numbered doc moves it.

## §4 Green-but-wrong tests (`docs/105` CQ-003)

- A spec asserted `not.toHaveText(/no results/i)` while the app says `"No match"`.
- A spec used `keyboard.type`, which `preventDefault`s printable characters, so the element
  under test never received input.
- The IME spec dispatched synthetic `CompositionEvent`s at `document`, a shape no real IME
  produces, and stayed green for months while the feature was unreachable.
- A guard passed because two sections inherited the same header, so it could not tell which
  section the result was charged to.

## §5a Combination failures (three on 2026-09-27)

1. #646 added a field to `Field`/`FieldRange`; #645 wrote fixtures constructing both in a
   crate #646 stayed out of. No text conflict; `main` failed with three `E0063`.
2. A published document's page is generated, so editing `docs/126` on one branch staled a
   page committed on another and `build.sh --check` failed on `main`.
3. Two ADR-034s met, and the merge resolution deleted two published ADRs while five
   documents still cited them. Uniqueness guards for doc and ADR numbers exist now.

`main.js` went 16,616 → 16,589 → 16,579 across three branches in one day, so arithmetic
on two branches' numbers is always wrong. Several lanes reached for `HF-190`/`HF-191`
within an hour. In the fixture proving a paste carries every inline kind, `Default` would
have satisfied `cargo check` while testing nothing about the new field.

## §6 Flakiness numbers

Roughly 5 of 15 contention-flaky specs fail under `--repeat-each=3 --workers=4` on clean
`main`. `an_absurdly_large_input_is_refused_quickly` has a 2 s budget: 4.9 s in a full
workspace sweep, 0.21 s alone.

## §6a The six repeated mistakes

The owner named each more than once in a single session, one of them fifteen times.

1. Co-editing stopped four times because a lane's report was relayed and the PR raised with
   no relaunch.
2. ADR-061 named the one function Compare needed; it sat unbuilt for hours while its
   analysis was reported twice.
3. A CI verdict was called twice from a run still in flight, once reporting one failure
   where there were eight across three shards.
4. The glossary reddened `main` five times in one day; the `main.js` line ceiling churned
   four times the same way.
5. #738 added struct literals and #739 added the field and swept 33 of them. Each was green;
   the merge did not compile, in either order.
6. The disk filled seven times with eight Rust lanes on one volume, losing work three times.
   One lane was recovered only by diffing a dead worktree against its own commit.

## §7 Tracker edits

The rule once read "do NOT edit `docs/105` or any tracker" to stop lanes conflicting in one
table. It silently cancelled the standing instruction: eighteen pieces of work, eight
already merged, existed only in commit messages and PR bodies.

## §8 Naming the pattern

- **Model memory.** Every `Paragraph` and `Run` stored its properties by value, so a
  1.3M-paragraph plain-text document held 1.3 million copies of one `RunProperties`. Three
  rounds boxed rarely-populated fields, each measuring less than projected. The owner named
  it: normalization. Flyweight/interning (side table + handle + copy-on-write) takes
  ~1,000 B/paragraph to ~150–200 B.
- The same session raised a viewer ceiling three times by measurement alone before asking
  what shape the cost had.
- The windowed layout kept one paginator generic over a trait rather than a second
  height-only one, because two implementations of one rule diverge.
- A dropdown holding every style in the document fails the Word/Docs interaction test
  before any code is written.
- **Performance.** `documentOutline` called `paragraph_properties` per node, and that helper
  walks the document linearly: 1.3M × 1.3M block visits on a real customer file, for a
  panel that returned an empty list. The owner found it by waiting minutes for a panel.
  `MAX_VIEWER_BLOCKS` was raised to 1,800,000 on a Playwright run that merely finished,
  admitting documents that freeze the tab for 30–110 s.
- Hand-maintained counts drifted into false public claims twice (`104` read 114/47 against
  an actual 146/54).

## §9 The public page lied three times

`webapp/fidelity.html` carried fabricated claims twice. The landing page lied a third time,
in both directions (`105` EV-007): a "three of five" parity figure contradicting the sourced
4/5, a corpus file that did not exist, a "sub-10 ms" repaint claim with no benchmark, and
three shipped features listed as "Not yet". `fidelity_data.test.mjs` once pinned two grades
at values the code had already outgrown. A design prototype's `doc.transaction()` and
`doc.writeDocx()` APIs did not exist, and it booted a live editor iframe on page load.

## §10 Fix the class

Six specs asserted `expect(#pages).toBeFocused()` and were patched one CI failure at a time
until the shared `expectEditorFocused()` and `tests/focus_contract.test.mjs` replaced them.
Single-surface capability is a recurring defect (`docs/105` UX-004).

## §11 Traps

- Design tokens: the flat redesign is `docs/63`; ~140 tokens, one raw hex in ~6,500 CSS
  lines.
- Ribbon: this file once claimed "~55px of slack" and `ribbon-home.spec.mjs` "10px". Measured
  in Chromium, the Home band had ~288px of headroom at 1280 and fit down to 1017px; the other
  bands fit down to 702–743px. The under-estimate was being used to reject additions.
- `node_modules` symlink: `.gitignore`'s `node_modules/` matches a directory, so a symlink is
  committed as a `120000` blob. CI stayed green and the Pages deploy died with `tar:
  ./node_modules: File removed before we read it`. `repository-policy` now refuses tracked
  symlinks.
- A spec failed for no reason but a stale `webapp/pkg`.
- `formatShortcut` renders `⌘P` as `Ctrl+P` on the Linux runner (`105` UX-009).
