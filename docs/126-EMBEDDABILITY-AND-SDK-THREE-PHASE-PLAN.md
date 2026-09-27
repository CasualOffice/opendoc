# 126 — Embeddability and the SDK, in three phases

Owner instruction, 2026-09-27: take the SDK and all the embeddability/integration
planning, **divide it into three phases, and work one phase at a time** — each phase
in its own worktree, rebased on `main` before it is raised, one PR per phase.

This document is the division. It does not restate the designs: the API surface is
`docs/05-SDK-API-SPEC.md`, the architecture is
`docs/83-SDK-PACKAGING-EMBEDDING-AND-EXTENSIBILITY-ARCHITECTURE.md`, the extensibility
invariants are ADR-030 / `docs/45`, and the defect being closed is `109` **HF-109**
("nothing is embeddable: no host-capability modes, no custom element, no package").

## Why three, when `83` already has seven

`83` §6 is a 24-week calendar in seven phases. It is not wrong, but it is not a
delivery plan for this repo as it stands, for three reasons:

1. **Two of its seven are deliberately out of scope for now.** Collaboration is
   LAST by the owner's own ordering (authoring + i18n → mobile/touch →
   embeddability → SDK/release → collaboration), and `109` carries it separately as
   HF-114. MCP/AI agent tooling sits behind that. Phases 4 and 6 of `83` therefore do
   not belong in this sequence at all.
2. **Several of its phases are already substantially done** and would read as work
   that has not started. Its Phase 2 (virtualised multi-page scroll, zoom, selection,
   outline, search) and Phase 3 (IME/keyboard input, caret geometry, command
   dispatcher, undo/redo, DOCX export) are shipped in the editor. What is missing is
   not the capability; it is that **none of it is reachable by a host** — which is
   exactly what HF-109 says and what §9.4 of `docs/99` warns about: "built" is not
   "reachable".
3. **Its exit gates are mostly performance numbers** (60 FPS, <50 ms, <100 MB). Those
   are worth having, but a phase gate that is a frame rate does not tell you whether a
   host can do the thing. The gates below are "a host can do X, proven by a host doing
   X in this repository".

So the three phases below re-cut the same material by **what a host gets at the end
of each**, and each is independently shippable.

## The through-line

Embeddability is the product (`SKILL.md` §1). ONLYOFFICE is AGPL-3.0-only, bills the
Developer Edition per concurrent browser tab, and gates host customization *in code*
— `LayoutManager._applyCustomization` early-returns when `!_licensed`. Our position
is the same capability, permissively licensed and **ungated**. That is the wedge, and
it is what Phase 3 exists to make true rather than claimed.

---

## Phase 1 — One artifact, and a capability contract that holds

**What a host gets:** an installable thing they can put on a page, in a mode they
choose, that cannot be talked out of that mode.

* A single published artifact and a **custom element** (`<opendoc-editor>`), framework
  free. Framework bindings are explicitly *not* in this phase; a custom element works
  in React, Vue and plain HTML without any of them.
* **Capability modes resolved before first paint.** `webapp/src/capabilities.mjs`
  already exists and already does the load-bearing part: a page that is not the top
  window defaults to `embedded`, so a framed editor stops being a standalone one
  without the host having to discover a parameter. This phase makes it the single
  authority rather than one of two places that know about framing.
* **The roles the owner asked for**: `readonly`, `preview`, `commentor`, `edit`,
  `owner` — as presets over the capability set, not as a parallel mechanism.
* **Layered enforcement.** The chrome disables with a reason (never hides — "never a
  dead control", `SKILL.md` §10) *and* the engine refuses independently, so a
  permission cannot be defeated from devtools. A capability absent from the set must
  produce a disabled control with an explanation, not a missing one.

**Exit gate — a host does it, in this repository:** a page in the repo embeds the
editor twice, once in an iframe and once as the custom element, and drives both. A
test proves a `viewer`/`readonly` host cannot mutate the document **through the engine**,
not merely that the button looks disabled. Passwords are out of scope here and stay
out until the format work that would make them meaningful exists.

## Phase 2 — The host contract: one schema, two transports

**What a host gets:** a typed way to command the editor and to be told what happened,
identical whether they hold a reference or are talking across an iframe boundary.

* **`crates/casual-doc-sdk` exists and has no product consumer.** That is the thing to
  fix, not to duplicate: it already has `command`, `config`, `event`, `selection`,
  `session`, `snapshot`, `value`. Phase 2 wires the real editor to that facade, or
  states in the PR why the facade's shape has to change and changes it once.
* **Events**: at minimum `ready`, `change`, `selection`, `save`, `export`, `error`,
  and `refusal` — refusal because the editor's whole feedback design (`109` UX-017)
  now has one channel, and a host that cannot hear a refusal will re-issue it forever.
* **Commands**: the catalog already enumerated in `docs/05` §7, reached through the
  command registry the editor already has — `runCommandById` and `keymap.mjs` landed
  with UX-006, so every chord-bearing capability is already addressable by id.
* **One contract, not two.** The in-process API and the `postMessage` protocol are
  generated from, or checked against, **one schema**. Two hand-maintained surfaces
  drift, and this repository has the receipts: the `shortcut:` labels and the key
  bindings were two tables that disagreed until UX-006 made them one.

**Exit gate:** a host in this repo drives every command over **both** transports, and a
guard proves the two surfaces are the same contract — so adding a command to one
without the other fails the build rather than shipping a half-reachable API.

### Phase 2 status — landed 2026-09-27

The schema is `webapp/src/host_contract.mjs`: commands and what each requires of
the host's grant, the events and their payloads, the refusal codes, the
`postMessage` envelope, and the origin rule. `host_session.mjs` (in process) gates
and dispatches from it; `host_bridge.mjs` is an envelope and an origin check over
that same session object, not a second implementation; `host_client.mjs` generates
a host's verbs from the same schema. The decision is recorded as **ADR-036** (it was drafted as ADR-034, a number two already-published experimental ADRs held; see the register).

The exit gate is `webapp/tests/e2e/host-contract.spec.mjs`, driven from
`webapp/embed.html`, which mounts the editor twice and drives both panels. It
proves the contract covers the registry exactly in the three states the families
need, that both transports answer identically for every command the editor offers,
that a host granted nothing changes nothing, that a refusal the chrome makes is a
refusal the API makes, and — by derivation from the engine rather than from the
table — that a command the contract calls ungated really does not touch the
document.

Three things worth knowing, because they are decisions rather than details:

* **The event set is this document's, not `docs/125` §8's.** `125` sketched ten
  events derived from `casual_doc_sdk::RuntimeEvent` with `ErrorCode` as the
  refusal vocabulary. What shipped is the seven named above, because that is what a
  host needs to drive an editor rather than to read a transaction log. The refusal
  codes are complementary to `ErrorCode`, not a rename of it: they say why a *host
  command* was refused, and `HostRefusal::for_error` maps every `ErrorCode` onto
  one, so a native and a browser host share one vocabulary.
* **`crates/casual-doc-sdk` now declares that vocabulary and still does not run
  the editor.** `src/host.rs` holds the events, codes, verbs and version, with
  `HostEvent::from(&RuntimeEvent)` as the derivation `125` §8 asked for, and
  `src/host_parity.rs` reads the editor's schema and fails in both directions. The
  runtime convergence is `docs/125` §9 row 5 (**L**) behind `109` CQ-002, and it is
  ADR-005's debt regardless — doing it inside this phase would have hidden a
  runtime migration inside an API change. The embedding page says so in its "does
  not do yet" list, with a guard that fails when it stops being true.
* **The site page's unrouted-string count ROSE, deliberately.** The four site
  templates have no localisation seam at all, so the only way to lower one of their
  numbers is to delete English, and every site ceiling sat exactly at its
  measurement. `no_unrouted_strings.test.mjs` now reads those files as declared
  measurements held to equality, with a guard proving they really have no seam
  (`editor.html` as the control) so the exception evaporates when site
  localisation lands. That is a policy change and it is the one thing in this
  phase an owner might want to reverse.

Deliberately not in phase 2: document I/O through the contract (no `open(bytes)`,
no `export()` that returns bytes), arguments for more than the one command that
takes one, a per-capability host list, reading/preview chrome composition, and any
Rust in-process transport.

## Phase 3 — White-labelling, customization, and release

**What a host gets:** their product, not ours, and a way to install it.

* **White-labelling**: name, mark, and theme. The design tokens are deliberate
  (`docs/63`: `--radius: 3px`, 140 tokens, one raw hex in 6,481 CSS lines, an AA
  contrast sweep over both themes) so this is a *token* seam, not a restyle. Any
  host theme must still pass the contrast sweep; a white-label that ships unreadable
  text is a worse outcome than no white-labelling.
* **Chrome selection**: which bands, menus and surfaces a host shows. This is the part
  ONLYOFFICE gates behind a licence check, and ours must be **ungated** — the licence
  is the wedge, so gating it would surrender the only structural advantage.
* **String overrides**: i18n already ships 19 catalogues behind `t()`, so host
  overrides layer on the existing seam rather than introducing a second one.
* **Release**: versioning, provenance, and a developer page whose every claim is
  **generated from a committed artifact** (`docs/99` §9 — that page has carried
  fabricated numbers twice, in both directions, so examples must be extracted from
  code that runs in CI rather than written by hand).

**Exit gate:** a build is white-labelled with **no code changes** — configuration only
— and still passes the contrast sweep; and every published claim is re-derived by a
guard rather than asserted.

### Phase 3 status — landed 2026-09-27

The configuration is `webapp/brand.json` and the seam is a generator,
`webapp/tools/build-brand.mjs`, which turns it into three committed artifacts:
`src/brand.css` (the palette, the mark, and one marker the runtime reads),
`src/brand.mjs` (the name, the tab-title policy, the string overrides), and two
generated regions in `editor.html` (the icons and the brand element). `--check` runs in
`build.sh` and fails in both directions. So white-labelling is one JSON edit and one
command, the shipped configuration is all-null — the default build is the product — and
the decision is recorded as **ADR-039**.

**The exit gate, both halves.** `webapp/brand.example.json` is a committed worked
example (a fictional "Northwind Docs": teal accent, warmer greys, twenty-odd token
values moved, `tabTitle: "document"`). `tests/brand.test.mjs` generates from it and
audits the result in node; `tests/e2e/white-label.spec.mjs` serves the artifacts in
place of the shipped ones — which is exactly what a white-labelled deployment does — and
runs the same `contrast-audit.mjs` sweep the editor's own theme sweep uses, over both
themes, with a positive control asserting the palette really is not ours before any of
it. A sweep over the DEFAULT build would have proved only that our palette passes, which
was already true and already guarded.

**What happens when a host's colour fails AA: it is refused**, at generate time, naming
the pair, the measured ratio, the floor, and the nearest value on the same hue that
would pass. Not corrected (that ships a brand nobody approved, which is the lie
`contrast.mjs` already refuses to tell about a document's own colours) and not warned
about (that is ONLYOFFICE's nag-modal failure mode, `docs/125` §1.1). The floors are
WCAG's split, so a hairline is not held to the prose bar.

Five things worth knowing, because they are decisions rather than details.

* **The overridable token set is a public contract; the rest are refused.** This answers
  `docs/125` Q-B. Eighteen names are frozen — the accent family, surfaces, foregrounds,
  lines, primary action. Geometry and z-order are refused because `docs/63` says to
  propose visual changes rather than make them, and the `--paper*` layer is refused
  because the sheet is white in both themes on purpose; a host who darkened it would put
  the dark theme's markers on a dark page at roughly 2:1, and no existing guard would
  see it.
* **`docs/125` §2 F4 is closed, and the sharper half was the one F4 did not mention.**
  `applySettings()` wrote an inline `--accent` from `localStorage` — which beats any
  stylesheet a host ships — and REMOVED a host's `data-theme` whenever the stored theme
  was `system`, the default. The fix travels in the stylesheet that pinned the value
  (`--brand-accent-pinned`) rather than through a second configuration channel, which is
  what keeps a white-labelled build a swapped static file. A host may pin the accent; it
  may not pin light/dark, because that is a reader's preference about their own eyes.
* **Chrome composition is a second axis in the same authority, and `CAPABILITY_AFFORDANCES`
  is the joint.** Every capability a preset grants must have an affordance in chrome that
  preset shows. That guard is what SHAPED reading chrome rather than being written after
  it: hiding the ribbon for `readonly` without revealing the menu bar would have made its
  only grant — `print` — unreachable, and the guard said so before the code existed.
  `readonly` gets reading chrome; `preview` gets none of the eighteen regions.
* **Per capability, not per tier, is now real.** `?can=-print,-download` narrows any
  preset through the same `resolveCapabilities`, and a guard proves every role is
  reachable as a narrowing of the role above it. Phase 1 had only the `autosave`
  override.
* **A mutation that did NOT go red found the phase's own version of phase 2's finding.**
  Deleting the generator's AA refusal left all eighteen assertions in `brand.test.mjs`
  green, because they called the audit directly and never proved the GENERATOR refuses.
  The door was open with a lock behind it. The fix is a test that drives the real command
  line and reads the real exit status, which is why the generator has
  `--check --config <path>`.

**Per-GROUP selection inside a band is deliberately not here.** Eleven ribbon groups
carry no `data-group`, so that roster would be half-expressible; the honest grain for a
host is the band, and all eight are selectable.

**Also deliberately not here, and each for a reason:**

* **Publishing to npm.** Nothing publishes the package, and the embedding page still says
  so. A publish workflow needs an owner-held registry credential and a decided version
  line, and shipping one that cannot be exercised would arm a gate nobody has run — which
  is `docs/99` §9.2's defect. The packaging side is done: `release.mjs` is generated and
  guarded, the `./release` subpath is typed, and `npm pack`'s file count and size are
  re-derived on the embedding page.
* **A consumer type-check job.** `docs/104` recommends one and `embed_package.test.mjs`
  still checks the `.d.ts` by string rather than by compiling it. Adding `tsc` to a
  repository with no TypeScript toolchain is its own decision.
* **Publishing `docs/83` as a site page.** The install-line defect that withheld it is
  fixed, but this generator's head-field rules still refuse it: an `&` in the H1 and an
  85-character `<title>` against a 20-70 rule. Both are fixed by shortening the
  document's own heading, which is its identity and belongs to whoever owns it — filed as
  `109` HF-202 rather than done in passing. The withheld entry now carries the new reason
  and a guard asserts the new reason is a fact.
* **Site localisation.** The four site templates still have no `t()` seam, so the
  measured-not-ratcheted exception `docs/126` phase 2 introduced still stands.
* **A second mechanism for anything.** Strings layer on `t()`, the palette layers on the
  cascade, chrome layers on the body-class shape `body:not(.doc-loaded)` already uses, and
  the mark is a token.

---

## Container policy — owner notes, 2026-09-27

Three constraints the owner gave after Phase 1 landed. They are here rather than in a
phase because they cut across all three.

### 1. The policies are per capability, not per tier

A host must be able to withhold **print, download, save, edit and comment
independently** — "comments only, everything else off" is a real configuration, not a
step on a ladder. The five roles stay, because most hosts want a name rather than a
checklist, but they are **presets over the capability set, never the unit of
enforcement**. Anything a role can express, an explicit capability list must also be
able to express, and the two must resolve through the same code — one authority, or the
roles and the fine-grained list will disagree the way the `shortcut:` labels and the
key bindings did (`109` UX-006/UX-007).

### 2. Surface composition is a different question from command gating

"Never a dead control" (`SKILL.md` §10) says a command that cannot run **now** ships
disabled **with a reason**, never hidden. That rule is about a *command inside a surface
the user was offered*. It does NOT say every role gets every surface.

So the two rules compose:

* **A role that has no business with a whole surface does not get the surface.** A
  `readonly` container has no editing ribbon. Not a ribbon full of greyed buttons — no
  ribbon. Word, Google Docs and ONLYOFFICE all do this: their read-only/preview
  presentations are a different chrome, not the editing chrome with everything dimmed.
* **Within a surface a role DOES get, a command that cannot run right now is disabled
  and says why.** That is where "never a dead control" applies, and Phase 1's
  disabled-with-a-reason work is correct there.

A wall of greyed controls is not honesty, it is noise: it tells a reader about
capabilities they will never have, and it buries the one or two things they *can* do.
The distinction to hold onto is **"never, for you" versus "not right now"** — the first
is composition, the second is state.

This refines what Phase 1 shipped. Phase 1 gates commands and disables review-mode
buttons with a reason, which is right for `edit`/`commentor`, and wrong for `readonly`
and `preview`, where the band should be absent.

### 3. `preview` and `readonly` are NOT the same thing

They were correctly given different capability sets in Phase 1 (`preview` grants
nothing, `readonly` grants `print`), but the difference is bigger than one capability
and the chrome has to reflect it:

| | `preview` | `readonly` |
| --- | --- | --- |
| What it is | The runtime as a **layout and rendering engine** (`docs/83` §2) — a picture of the document | A **reading experience** of the document |
| Chrome | Minimal to none. No ribbon, no menu bar. Possibly only the pages | Reading chrome: navigation, outline, find, zoom, page controls, print |
| Who embeds it | A host showing a thumbnail, an attachment preview, a search result, a print preview | A host publishing a document for people to read |
| The test of the difference | Could a static image replace it? For `preview`, nearly | For `readonly`, no — the reader navigates and searches |

Consequence: `preview` is not "`readonly` minus print". Collapsing them would give every
attachment preview a reading UI it does not want, or every published document a bare
canvas with no way to get to page 40. They are separate presentations and Phase 2/3 must
keep them so.

## Site documentation is part of every phase, including Phase 1

Owner instruction: the SDK documentation goes **on the site**, attached to the phases,
and Phase 1's own page is owed now rather than at the end.

The binding constraint is `docs/99` §9: the fidelity and landing pages have carried
**fabricated claims twice, in both directions**. So for every phase:

* **Every example on the site is extracted from code that runs in CI**, never
  hand-written into the page. A snippet nobody executes is the next false claim.
* Every number is generated from a committed artifact and re-derived by a guard, as
  `site_claims.test.mjs` already does for the landing page.
* Each page carries an honest **"what this does not do yet"** section. Phase 1's must
  say there is no command/event API and nothing published to a registry.

Phase 1's page has real material to point at: an installable package, `<opendoc-editor>`,
the five roles, the three enforcement layers (browser sandbox / engine mode / chrome
reason), and `webapp/embed.html` as a working demo — so it can link a thing that runs
rather than describe one.

## Explicitly not in these three

* **Collaboration, presence, sharing, roles-at-a-server** — `109` HF-114, and last by
  the owner's ordering. ADR-033 settles the algorithm (OT over transactions); the real
  blocker is that the live editing path bypasses the transaction engine (`109` CQ-002),
  which is its own row and must close first.
* **MCP / AI agent tooling** — additive by ADR-030's invariants, and behind
  collaboration.
* **Plugin registration and framework bindings** — a custom element covers the hosts
  that matter now; plugins want a stable command surface first, which is Phase 2.
* **Passwords** — scoped out until there is format work to make them mean anything.

## Sequencing rule

One phase at a time, each in its own worktree, **rebased on `main` immediately before
the PR is raised**, one PR per phase. That last part is not ceremony: four merges went
in on 2026-09-26/27 while no `main` CI run had completed, and the `main.js` ratchet
collided twice because two branches each measured honestly against a file the other was
about to change. Re-measuring from the merged file is part of raising the PR.
