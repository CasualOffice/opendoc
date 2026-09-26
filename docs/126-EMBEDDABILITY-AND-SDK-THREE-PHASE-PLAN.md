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

---

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
