---
name: opendoc
description: The working contract for the opendoc repository — an Apache-2.0 document runtime and editor aiming to replace ONLYOFFICE for documents. Load this BEFORE touching anything in this repo: it carries the product goal, the non-negotiable engineering gates (including the two CI gates a normal test run misses), the PR and branching rules, how to parallelise with agents, the known-flaky tests, and the specific mistakes that have already been made here so they are not repeated. Use it for any task in this repo — fixing a bug, adding a feature, auditing, reviewing, or answering a question about direction.
---

# opendoc — working contract

Read this first. It exists so the owner does not have to repeat the same instructions
every session. If something here conflicts with a stale doc in the repo, this file and
`AGENTS.md` win; if it conflicts with something the owner says now, **the owner wins** —
then update this file.

## 1. The goal, in one sentence

> opendoc is the **Apache-2.0 alternative to ONLYOFFICE Docs for documents and document
> collaboration**.

| In scope | Out of scope |
| --- | --- |
| Word-processing documents: DOCX, ODT, TXT, JSON snapshot, and remaining document interchange formats | **Spreadsheets** — the sibling `opencalc` (`../sheets`) owns these |
| Document **collaboration** — multi-user editing, presence, review, versions, roles | **Presentations** — a future sibling |
| Embedding as a library; local-first; no mandatory server | PDF *editing* and PDF forms |
| Desktop, browser, mobile browser, headless | Native mobile app shells |

**Never describe this project as an MVP, prototype, or side project.** Production and
enterprise grade is the baseline, not an aspiration. Phasing is delivery order only.

The roadmap is `docs/106-ONLYOFFICE-ALTERNATIVE-ROADMAP.md` (8 phases + a cross-cutting
quality track). Ranked rows live in `docs/105-AUDIT-2026-09-TRACKER.md`. The defect queue
is `docs/104-HOTFIX-TRACKER.md`. Cite row ids (UX-001, FID-L-03, CQ-002, HF-011) in
commits and PRs.

### Why Apache-2.0 is the whole wedge

ONLYOFFICE is **AGPL-3.0-only** with assets under CC-BY-SA-4.0, commercial tiers from
$1,500/$3,500, Developer Edition billed **per concurrent browser tab**, and the
host-customization API gated *in code* (`LayoutManager._applyCustomization` early-returns
when `!_licensed`). Permissive licence + real DOCX fidelity + embeddability is an
unoccupied position — which is why **embeddability is the product**, not a late-phase
nicety.

### Three structural advantages to protect

Do not close a parity row in a way that costs any of these.

1. **Local-first.** ONLYOFFICE's web client *cannot* open a file offline: format I/O is
   `x2t`, and `core/X2tConverter/build/` has only `Android/` and `Qt/` — **no WASM build**.
   They do not ship the native core as an embeddable library. We are local by construction.
2. **Direct OOXML with verbatim retention.** They convert `DOCX → Editor.bin → DOCX`, so
   whatever the intermediate model lacks is dropped. **But this advantage is only real if
   loss is detected and reported** — which is why loss-reporting work is competitive work,
   not hygiene.
3. **No mandatory server.** Any collaboration relay is additive and optional.

### Competitive facts that are commonly reported wrong

Use these instead of ONLYOFFICE marketing pages:

- Their co-editing is **neither OT nor a CRDT** — zero `transform` hits in
  `DocsCoServer.js`. It is a server-ordered change log + pessimistic object locks +
  client rollback/replay. Transport is socket.io.
- Spell check is **client-side WASM** now (`spell.wasm`); their server service is retired.
  So spell check is provably a client-side problem for us too.
- Their text input is a **hidden `<textarea>`** whose id matches `/area_id/` — zero
  functional `contenteditable` in 171 KLOC.
- **Gaps they have:** no table sorting, no decimal/bar tab stops, only 14 field codes, no
  page-borders dialog, no accessibility checker, no native citation manager, SmartArt
  editing is formatting-only. We already ship table sorting and decimal tabs.

## 2. Collaboration is decided: OT (ADR-033)

Operational transformation, carried by transactions, with snapshot-plus-replay versioning.
Designed in `docs/107`. **Do not reopen the OT-vs-CRDT question.**

The blocker is not the algorithm — it is that **the live editing path bypasses the
transaction engine**: `casual-doc-wasm` references `casual_doc_transaction` zero times and
applies `casual-doc-edit`'s 47 ops directly with a flat undo stack and no revision chain.
So **ADR-005 is not honoured in practice**, and unifying the two op sets comes first.

Owner constraint: **editing must stay light** — seven measurable budgets in `docs/107` §4.
Per-keystroke work is O(1) in document size.

## 3. Non-negotiable engineering gates

Run **all** of these locally before pushing. Two of them are missed by a normal
`cargo test` sweep and have broken CI on three separate branches:

```sh
cargo +1.96.0 fmt --check                                      # PINNED toolchain
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked
cargo test --workspace --all-features --locked
cargo test --doc --workspace --all-features --locked
cargo check --workspace --all-features --locked --target wasm32-unknown-unknown

cd webapp && ./build.sh                                        # needed before any e2e
npm run test:unit                                              # node --test tests/*.test.mjs
npm run test:e2e                                               # playwright
```

**The two that bite:**

- **Never pipe a gate into `tail`/`grep` and then `&& echo OK`.** `a | tail -2 && echo OK`
  tests TAIL's exit code, not the gate's, so a failing gate prints OK. That is how a
  formatting failure reached CI green-looking locally (#552). Run each gate as its own
  command and branch on its real status: `if cargo +1.96.0 fmt --all --check; then …`.
- **`cargo +1.96.0 fmt`, not plain `cargo fmt`.** Plain fmt passes locally and fails CI —
  the pinned toolchain formats differently.
- **`RUSTDOCFLAGS="-D warnings" cargo doc`.** A public doc comment must **not** link to a
  private item (`[\`Self::private_fn\`]`). This failed three branches in one session. Write
  the name as plain code text instead, or make the item `pub`.

Full CI job list: `format lint test benchmark-smoke fuzz-build docs wasm browser-smoke
platform dependency-policy repository-policy`.

## 4. Tests must be able to fail — this is the house rule

A guard that cannot fail is worse than no guard, because it gets cited as evidence.
This repo has shipped green-but-wrong tests more than once (`docs/105` CQ-003).

**The procedure, every time:**

1. Write the guard.
2. **Mutate the production code** to reintroduce the bug.
3. **Run it and SEE IT GO RED.** Record the actual failure output.
4. Restore, confirm green.
5. Report the mutation and the red output in the commit/PR.

If a guard cannot be driven red, rewrite it or drop it. **A test that passes on arrival
tells you less than you think.** Real examples from this repo:

- A spec asserted `not.toHaveText(/no results/i)` while the app's string is `"No match"` —
  it passed no matter what happened.
- A spec used `keyboard.type`, which `preventDefault`s printable characters, so the element
  under test never received input.
- The IME spec dispatched synthetic `CompositionEvent`s at `document` — a shape no real IME
  produces — and stayed green for months while the feature was unreachable.
- An agent's guard passed because two sections inherited the same header, so the assertion
  could not tell which section it was charged to.

## 5. Branching, PRs, and commits

- **Always a feature branch + PR. Never commit or merge directly to `main`.**
- **One combined PR for related work.** Do not raise a stream of small PRs. Batch the
  lanes, verify them together, open one.
- **Never revert to get green when you can fix forward.** If a gate goes red, diagnose and
  fix it; reverting hides the finding. (Asked once, told clearly: *fix it, don't revert.*)
- **No Claude/AI attribution** in commit messages or PR bodies. No `Co-Authored-By`
  trailers, no "Generated with" footers.
- Commit messages: prose explaining **why**, the mechanism, the evidence, and the mutation
  proof. Cite row ids. State what you deliberately did **not** do.
- **Never dispatch a workflow against `main` that can turn CI red.** Run it on a branch,
  verify the gate is green with its output, *then* let it land. This was learned the hard
  way: arming a gate on `main` broke CI for everyone.
- **Never hand-edit a blessed reference or golden** to make a test pass, and never inflate
  a tolerance to green a red edge. Both make the gate worthless.
- If a golden (`geometry_snapshot.golden`) moves, the diff must be intentional and
  explained.

## 6. Verify before you claim

- **Never bisect with single runs of a possibly-flaky test.** Repeat 5× per ref. A false
  "this PR broke it" wastes more time than the bug.
- **Known flaky under worker contention** — re-run in isolation before calling a failure a
  regression: `context-menu.spec.mjs`, `header-footer-editing.spec.mjs`,
  `object-command-reach.spec.mjs`, `table-editing-ux.spec.mjs`. Roughly 5 of 15
  fail under `--repeat-each=3 --workers=4` on *clean main*.
- **Clock-bound tests are a different failure, and retries do not help.** These wait on a
  wall clock rather than on a state change, so they degrade under load no matter how
  sound the code is: `draft-recovery.spec.mjs` (a 5 s autosave quiesce) and the Rust test
  `an_absurdly_large_input_is_refused_quickly` (a 2 s budget — measured at 4.9 s in a full
  workspace sweep and 0.21 s alone). Both failed once in a full parallel run and passed
  3/3 isolated. Prefer waiting on an observable state change when writing a new one.
- Distinguish *your* regression from pre-existing failure with evidence: stash, rebuild,
  run the same specs on `main`, compare.
- Do not take a subagent's report at face value. Re-verify its load-bearing claim yourself.
  Agents in this repo have been right when I was wrong, and wrong when confident.

## 7. Parallelise with agents — by default, not on request

The owner expects parallel work and should not have to ask. When there is more than one
independent piece, fan out.

**Scope agents by FILE DOMAIN, not by feature**, or they collide. Domains that don't
overlap:

| Lane | Files |
| --- | --- |
| layout engine | `crates/casual-doc-layout/**` |
| import | `crates/casual-doc-import/**` |
| export / io | `crates/casual-doc-export/**`, `crates/casual-doc-io/**` |
| wasm facade | `crates/casual-doc-wasm/**` |
| webapp | `webapp/**` |

**Only ONE agent may own `webapp/src/main.js`** — it is a 15,951-line module and two agents
in it will conflict badly. Same for `casual-doc-wasm/src/lib.rs` (26,374 lines).

**Always use `isolation: "worktree"`.** Each worktree carries its own `target/` (~2–7 GB),
so remove finished ones (`git worktree remove --force`) — but **push any unmerged branch to
origin first**, because removing the worktree is easy to do before noticing the work was
never merged.

**Every agent prompt must say:** commit your work on your branch; do NOT merge; do NOT open
a PR; do NOT edit `docs/105` or any tracker. Agents have left work uncommitted and have
taken unrequested actions. Also tell them the mutation rule and the full gate list — they
will otherwise skip `cargo doc` and the browser suite.

## 8. Design first, and document as you go

Per `AGENTS.md`: read the docs, design, discuss substantial designs, update
`docs/14-EXECUTION-TRACKER.md`, implement in reviewable increments, test, keep docs and
ADRs current.

- Durable knowledge goes in a **numbered doc** under `docs/`. Decisions go in an **ADR**
  (`docs/08-ADR-REGISTER.md`). Execution state goes in the **tracker**.
- **Record open questions rather than hiding uncertainty.** Where behaviour deliberately
  differs from Word, say so in the code — do not leave it ambiguous.
- Counts in docs must be **derived, not hand-maintained**. Hand-maintained numbers have
  drifted into false public claims twice (`104` read 114/47 against an actual 146/54).

## 9. Evidence rules — these exist because the public page lied twice

`webapp/fidelity.html` carried fabricated claims on two separate occasions.

1. **A published number is generated from a committed artifact, or it is not published.**
2. **Prose describing a CI gate must name the workflow, and a test must assert the gate is
   armed.** A gate claimed as "CI-enforced" had never executed.
3. **Absence from a support matrix is an overstatement by omission** — enumerate families,
   not successes.
4. **"Modeled" is not "shipped", and "built" is not "reachable."** Constructs are typed and
   round-tripped with no layout consumer; a whole subsystem is built and unreachable from
   the product. This is the most expensive recurring pattern here — when you mark something
   done, check a user can reach it.
5. Dated audits (`44`/`46`/`55`/`60`) are ~1000 commits stale and **understate** the engine.
   `webapp/src/fidelity.js` is the current artifact and is under an honesty guard.
6. **The landing page lied a third time, in both directions** (`105` EV-007): a "three of
   five" parity figure contradicting the fidelity page's sourced 4/5, a corpus file that does
   not exist, a "sub-10 ms" repaint claim with no benchmark, and three shipped features
   listed as "Not yet". Every number on `index.page.html` is now tagged `data-claim` and
   re-derived by `tests/site_claims.test.mjs`; every "Not yet" item must cite a family graded
   `none` or an open `105` row. **Understating is also false** — check a gap is still open
   before you publish it. And a guard can pin a lie: `fidelity_data.test.mjs` was holding two
   grades at values the code had already outgrown, so re-verify a pinned cell before
   trusting it.
7. **Design prototypes are not evidence.** A prototype's numbers, API snippets and
   "live application" labels are placeholders. Its `doc.transaction()`/`doc.writeDocx()`
   APIs did not exist, and it booted a live editor iframe on page load. Adopt the visual
   system; re-derive every claim.

## 10. Enterprise-grade means fixing the class, not the instance

The owner asked for production/enterprise quality in code **and** UI/UX. Concretely:

- When the same failure appears in several places, **fix the pattern and add a guard that
  fails the build if it returns.** Six specs asserted `expect(#pages).toBeFocused()` and
  were being patched one CI failure at a time; the fix was a shared
  `expectEditorFocused()` plus `tests/focus_contract.test.mjs` to prevent drift.
- Tests should assert the **guarantee**, not the mechanism. "The editor can receive text",
  not "element X has focus."
- **Every capability must be reachable from ≥2 surfaces** (ribbon / menu / context menu /
  palette / shortcut). Single-surface capability is a recurring defect (`docs/105` UX-004).
- **Never a dead control.** A command that does not exist yet ships **disabled with a
  reason**, never as a button that does nothing.
- UI floor per phase: keyboard-operable, screen-reader-operable, localised, themed,
  touch-usable, and able to **say something** when it refuses.
- No `aria-hidden` on anything focusable (axe `aria-hidden-focus`).
- No file should exceed ~2,000 lines without a recorded exception.

## 11. Repo-specific traps

- **Design tokens are deliberate. Do not restyle.** `--radius: 3px` is a considered flat
  redesign (`docs/63`), `--accent: #3355c4`, 140 tokens, one raw hex in 6,481 CSS lines, and
  an AA contrast sweep over every text node in both themes. Adopt structure; propose visual
  changes to the owner rather than making them.
- **Fonts are self-hosted and guarded.** `webapp/src/fonts.css` self-hosts **Inter** and
  **Material Symbols Outlined**; `tests/chrome_fonts.test.mjs` asserts no
  `fonts.googleapis.com`/`gstatic.com` reference. Never add a CDN font link — it breaks
  local-first.
- **The ribbon must fit 1280px.** There is ~55px of slack on the Home band; widening one
  control exiles a whole group into the `⋯` overflow.
- `webapp/src/main.js` has **zero exports** and binds ~360 fixed DOM ids at import. There is
  no mount seam yet (HF-109).
- **`pages[]` holds page RECORDS, not elements.** The sheet element is `page.wrap`, and
  `scaleOf(page)` already returns its rect.
- The oracle geometry gate compares the **text region** (pen extents + baseline ±
  ascent/descent), and **only Latin-only fixtures** may be in it — substituted text of a
  different width changes where the rest of the paragraph *wraps*, so excluding the
  offending line is not enough.
- `.docm` is rejected at open; that policy is undecided, not an oversight.
- **Never symlink `node_modules` into a worktree.** `.gitignore`'s `node_modules/` matches a
  *directory*; git treats a symlink as a file, so `git add -A` commits it as a `120000` blob
  pointing at one machine's absolute path. CI stays green, and the Pages deploy dies in
  `tar: ./node_modules: File removed before we read it`. Worse, checking out a branch that
  carries the blob replaces the real `node_modules` with a self-referential link. Run
  `npm ci` in the worktree instead. `repository-policy` now refuses any tracked symlink.
- **`webapp/pkg` is not committed.** The browser suite silently runs against whatever engine
  was last built; after a rebase or a Rust change, `./webapp/build.sh` before trusting an
  e2e result. A spec failed here for no reason but a stale wasm.
- **Run Playwright from `webapp/`.** From the repo root it picks up no config and fails to
  collect with "did not expect test() to be called here".
- **Specs must not assert Mac glyphs.** `formatShortcut` renders `⌘P` as `Ctrl+P` on the
  Linux runner; derive expectations with the `shortcutHint` fixture (`105` UX-009).
- **Relative worktree paths land inside the repo.** `git -C <repo> worktree add name` creates
  `<repo>/name`. Use absolute scratchpad paths.

## 12. Engineering priority order

From `AGENTS.md`, in order: correctness and document safety → deterministic behaviour →
security and resource bounds → compatibility and round-trip fidelity → performance → API
stability → UX quality → maintainability.

Hard rules: public mutation goes through commands and transactions; the runtime must not
depend on the browser DOM as source of truth; unsupported document data is preserved where
safe or reported explicitly; **no silent data loss**; no mandatory server, React, or
collaboration provider dependency.

## 13. Communication

Be direct and factual. Surface risks early. **Do not overstate support or fidelity.** If a
feature is partial, say so. When you get something wrong, correct it plainly with the
evidence and move on — do not bury it or over-apologise.
