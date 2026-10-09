---
name: opendoc
description: Working contract for the opendoc repo (Apache-2.0 document editor and runtime replacing ONLYOFFICE for documents). Load BEFORE any task here (bug, feature, audit, review, direction question). Carries the goal, the CI gates a plain `cargo test` misses, PR and branch rules, agent fan-out rules, flaky tests, and repo traps.
---

# opendoc — working contract

Precedence: what the owner says now > this file > `AGENTS.md` > other docs. When a rule
changes, edit this file. Code cites these sections (`SKILL §8`), so **keep the numbers
stable**. The *why* behind each rule (incidents, measurements, competitive evidence) is in
`references/why.md`, keyed by the same §. Read it only to argue for, or extend, a rule.

## 1. Goal

Apache-2.0 alternative to ONLYOFFICE Docs for **documents and document collaboration**.

- **In:** DOCX, ODT, TXT, JSON snapshot; multi-user editing, presence, review, versions,
  roles; an embeddable library; local-first; desktop, browser, mobile browser, headless.
- **Out:** spreadsheets (sibling `opencalc`), presentations, PDF editing and forms, native
  mobile shells.
- Production and enterprise grade is the baseline. Never call it an MVP or a prototype.
- **Embeddability is the product.** ONLYOFFICE is AGPL and gates host customisation
  behind a licence.
- Never close a parity row at the cost of: **local-first** (no server to open or edit),
  **direct OOXML with verbatim retention** (only real if loss is *detected and reported*),
  **no mandatory server**.
- **`docs/109-BACKLOG.md` is the only queue**, in fix-first order. `docs/104`, `105`, `106`,
  `99` and `14` are archives closed to new rows (`tracker_single_queue.test.mjs` enforces
  this). Cite row ids (UX-001, HF-011) in commits and PRs. Roadmap: `docs/106`.
- Competitive facts (their co-editing, spell check, text input, gaps) are in
  `references/why.md` §1. Use those, not their marketing pages.

## 2. Collaboration is OT (ADR-033) — decided

Transactions carry OT; versioning is snapshot plus replay (`docs/107`). Do not reopen OT
versus CRDT. First blocker: the live editing path in `casual-doc-wasm` bypasses
`casual_doc_transaction` (ADR-005 is not honoured), so unify the two op sets first.
**Editing stays light:** per-keystroke work is O(1) in document size (seven budgets in
`docs/107` §4).

## 3. Gates — run every one, each as its own command

```sh
cargo +1.96.0 fmt --all --check        # PINNED toolchain; plain `cargo fmt` differs from CI
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked
cargo test --workspace --all-features --locked
cargo test --doc --workspace --all-features --locked
cargo check --workspace --all-features --locked --target wasm32-unknown-unknown
node webapp/tools/build-glossary.mjs   # whenever docs/ or any string changed; NOT in build.sh
cd webapp && ./build.sh                # pages, SEO, embed docs, site + their --check; before e2e
npm run test:unit                      # from webapp/
npm run test:e2e                       # from webapp/ (Playwright)
```

- Never `gate | tail -2 && echo OK`: that tests `tail`'s exit code. Branch on the gate's own
  status (`if cargo +1.96.0 fmt --all --check; then …`).
- A public doc comment must not link to a private item (`[`Self::private_fn`]`). Write the
  name as plain code text, or make the item `pub`.
- CI jobs: `format lint test benchmark-smoke fuzz-build docs wasm browser-smoke platform
  dependency-policy repository-policy`.

## 4. Every guard must be seen to fail

Write the guard → mutate the production code to reintroduce the bug → run it and **see it
go red** (keep the output) → restore and confirm green → put the mutation and the red output
in the commit or PR. If it cannot be driven red, rewrite it or drop it. Known false greens:
an expected string the app never shows, `keyboard.type` swallowing printable input,
synthetic events no real user or IME produces, an assertion that cannot tell two candidates
apart.

## 5. Branches, PRs, commits

- Always a feature branch and a PR; never commit or merge to `main`. **One combined PR** for
  related work, not a stream of small ones.
- Fix forward; never revert to get green. Never hand-edit a golden or blessed reference and
  never inflate a tolerance. A moved golden (`geometry_snapshot.golden`) must be intentional
  and explained.
- **No AI attribution** in commits or PRs: no `Co-Authored-By`, no "Generated with" footer.
- A commit message is prose: why, the mechanism, the evidence, the mutation proof, the row
  ids, and what you deliberately did not do.
- Never dispatch a workflow against `main` that can turn CI red. Prove it green on a branch.

## 5a. Check the combination, not just your branch

Two green PRs can merge into a red `main`: a field added to a struct whose literals live on
another branch, a generated page staled by a doc edit, two branches minting the same number.

- **Rebase last**, immediately before the PR, then re-run the gates.
- Run `cargo check --workspace --all-targets --all-features`, not just your crate's tests.
- Re-measure every ratchet (for example the `main.js` line ceiling) from the merged file.
- Editing a published doc ⇒ run `webapp/build.sh` and commit the regenerated pages in the
  same commit (the list is in `webapp/tools/build-doc-pages.mjs`).
- Claim a number (doc, ADR, row id) by writing it into the tree in the commit that cites it.
- When the compiler demands a new field, do not reflexively reach for `Default::default()`:
  set it where the fixture's purpose needs it. A widely constructed model struct should be
  `#[non_exhaustive]` with a builder.

## 6. Verify before you claim

- Bisect a possibly flaky test with 5 runs per ref, never one.
- Flaky under worker contention, so re-run in isolation first: `context-menu`,
  `header-footer-editing`, `object-command-reach`, `table-editing-ux` specs.
- Clock-bound, where retries do not help: `draft-recovery.spec.mjs` and the Rust test
  `an_absurdly_large_input_is_refused_quickly`. New tests wait on a state change, not time.
- Prove a failure is pre-existing with evidence: stash, rebuild, run the same specs on `main`.
- Re-verify a subagent's load-bearing claim yourself.

## 6a. Habits with mechanical closes

1. A lane's hand-back is not the end of its programme: raise the PR **and** relaunch the lane
   in the same message.
2. A verified gap is a work item: a finding ships with its change, or names the lane that
   will build it.
3. "Fixed" means merged; otherwise say "the fix is in #N". No CI verdict until every check
   has reported.
4. A diff touching `docs/` or any string ⇒ run the glossary **and** `build.sh` (§3). Turn a
   hand-maintained number into a generated artifact.
5. Adding a field to a model struct ⇒ `cargo check --workspace --all-targets` first, and
   expect a sibling-branch collision.
6. Check free disk before fanning out, cap concurrency to what the disk holds, and tell the
   owner which lane is paused and why.

## 7. Parallelise by default

Independent pieces ⇒ fan out without being asked. Scope lanes by **file domain**, not by
feature: layout `crates/casual-doc-layout/**` · import `crates/casual-doc-import/**` ·
export/io `crates/casual-doc-export/**`, `crates/casual-doc-io/**` · wasm
`crates/casual-doc-wasm/**` · webapp `webapp/**`. Only **one** agent may own
`webapp/src/main.js`, and only one `crates/casual-doc-wasm/src/lib.rs`.

- Always `isolation: "worktree"` with absolute scratchpad paths. Each worktree carries a
  multi-GB `target/`; push an unmerged branch before `git worktree remove --force`.
- Every lane prompt says: commit on your branch; do not merge; do not open a PR; **add your
  row to `docs/109-BACKLOG.md` in the implementation commit** (resolve a conflict by
  rebasing; never impose a no-tracker-edits rule); follow the §4 mutation rule and the full
  §3 gate list including `cargo doc` and the browser suite; "report, and I will raise it
  and relaunch you".

## 8. Design first, and name the pattern

Per `AGENTS.md`: read the docs → design → discuss a substantial design → add the `109` row →
implement in small increments → test → update docs and ADRs.

- **Name the known pattern before inventing one** (caching, interning/flyweight,
  virtualization, copy-on-write, normalization, indexing, streaming, back-pressure), or say
  why none applies. Increments that each buy less than the last mean the wrong axis.
- Design every interaction from Word and Google Docs first. Prefer one mechanism over two
  parallel paths.
- Durable knowledge → a numbered doc in `docs/`. Decisions → an ADR in
  `docs/08-ADR-REGISTER.md`. Execution state → `109`. Record open questions. Where behaviour
  deliberately differs from Word, say so in the code. Counts in docs are derived, never
  hand-maintained.
- **Performance (there is no CI perf job yet):** state the complexity of anything that
  touches the document in its doc comment; never call a lookup-by-id (`paragraph_properties`,
  `find_paragraph`, which are linear scans) inside a loop; per-interaction work is O(1) in
  document size; anything O(document) runs off the main thread with progress and cancel;
  guard complexity with n-versus-2n doubling tests, not millisecond thresholds; measure time
  to interactive, not "completes in a harness".

## 9. Evidence

1. A published number is generated from a committed artifact, or it is not published.
2. Prose calling a gate "CI-enforced" names the workflow, and a test asserts it is armed.
3. Support matrices enumerate families, not successes. Absence is overstatement.
4. Modeled is not shipped; built is not reachable. Before marking done, check a user can
   reach it.
5. Dated audits (`44`/`46`/`55`/`60`) understate the engine. `webapp/src/fidelity.js` is
   current and honesty-guarded.
6. Every number on `index.page.html` is `data-claim` tagged and re-derived by
   `tests/site_claims.test.mjs`. A "Not yet" cites a family graded `none` or an open row.
   Understating is also false; re-verify a pinned cell before trusting it.
7. Design prototypes are not evidence: adopt the visuals, re-derive every claim.

## 10. Enterprise grade means fixing the class

- The same failure in several places ⇒ fix the pattern and add a guard that fails the build
  if it returns (for example `expectEditorFocused()` plus `focus_contract.test.mjs`).
- Assert the guarantee, not the mechanism.
- Every capability is reachable from ≥2 surfaces (ribbon, menu, context menu, palette,
  shortcut).
- Never a dead control: an unbuilt command ships disabled **with a reason**.
- UI floor: keyboard and screen-reader operable, localised, themed, touch-usable, and it
  says something when it refuses. No `aria-hidden` on anything focusable.
- A file over ~2,000 lines needs a recorded exception.

## 11. Repo traps

- Design tokens are deliberate (`--radius: 3px`, `--accent: #3355c4`, an AA sweep in both
  themes). Adopt the structure; propose visual changes to the owner instead of making them.
- Fonts are self-hosted (Inter, Material Symbols Outlined) and `chrome_fonts.test.mjs`
  forbids the Google Fonts CDN. Never add a CDN font.
- The ribbon must fit 1280px. Do not quote a headroom figure: `ribbon-width-budget.spec.mjs`
  derives it and holds a 120px floor. Overflow shows up as a horizontal scrollbar on the
  band, not as the `⋯` button.
- `main.js` has zero exports and binds ~360 fixed DOM ids at import; no mount seam (HF-109).
- `pages[]` holds page records; the sheet element is `page.wrap`; `scaleOf(page)` gives its
  rect.
- The oracle geometry gate compares the text region only, and only Latin-only fixtures may
  be in it.
- `.docm` is rejected at open: the policy is undecided, not an oversight.
- Never symlink `node_modules` into a worktree (git commits the link as a blob and the Pages
  deploy dies). Run `npm ci` there instead.
- `webapp/pkg` is not committed: run `./webapp/build.sh` after a rebase or a Rust change
  before trusting an e2e result.
- `webapp/sample.docx` is a gitignored copy `build.sh` stages. A Rust test embeds the
  committed root `sample.docx`; the copy compiles locally and fails CI's fresh checkout.
- Chrome built at boot must not call `t()` until it is shown: the catalogue lands later,
  and `chrome-raw-keys.spec.mjs` reads `title`/`aria-label` on HIDDEN elements too. A
  scoped e2e run missed this and turned `main` red (#815 → HF-281); before pushing UI, run
  the whole browser suite, not just the specs named after the feature.
- Run Playwright from `webapp/`. Specs never assert Mac glyphs; use the `shortcutHint`
  fixture.
- A relative `git worktree add name` lands inside the repo. Use absolute paths.

## 12. Priority order

Correctness and document safety → determinism → security and resource bounds →
compatibility and round-trip fidelity → performance → API stability → UX → maintainability.
Public mutation goes through commands and transactions; the DOM is never the source of
truth; unsupported data is preserved or reported; **no silent data loss**; no mandatory
server, React or collaboration provider.

## 13. Communication

Direct and factual. Surface risks early. Never overstate support or fidelity: a partial
feature is described as partial. Correct a mistake plainly with the evidence and move on.
