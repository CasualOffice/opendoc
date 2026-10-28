# 181 — One capability registry: every feature inherits its AI and MCP exposure

**Status:** Built 2026-10-29 (PR #897): R0–R2, the generated front doors of R3/R4 that the owner chose, `rewrite_block` and the private preview mode. §12 says what is built, what is not, and why. ADR-075 was accepted by the owner on 2026-10-29.
**Decides:** ADR-075 (accepted 2026-10-29). **Row:** `docs/109` RM-48.
**Refines:** `178` and ADR-072 (§3 rule 2 "one schema", §4.3 rule 3 "the vocabulary is
closed", and prerequisite A4). **Builds on:** `45` and ADR-030 (I1 to I4), `147` (transactions
as the one mutation path), `84` (the command registry), `149` and ADR-036 (the host contract),
`176` and ADR-071 (the Native SDK), A1 (`casual-doc-review`, the precedent this generalises).

**Owner request, 2026-10-28:** "MCP complete planning and architecture and pattern so
upcoming features can also inherit that."

---

## 0. The problem, measured at `a6a73192`

`178` designed the MCP server and its AI layer, and M1 + M2 are built (`178` §15). What it
did not design is **how the next feature gets there**. Today a feature reaches an agent only
if someone hand-wires it into five places, and nothing fails when nobody does.

| Fact | Evidence | Consequence |
| --- | --- | --- |
| The edit plan reaches **9 of 67** `Operation` variants | `casual-doc-ai/src/plan.rs:388-899` against `casual-doc-edit/src/lib.rs:459-1375` | Images, hyperlinks, breaks, fields, TOC, notes, bookmarks, sections, headers/footers, charts, styles, numbering, table insert/columns and every object edit are unreachable by an agent |
| The vocabulary is spelled **four times** in one file | `EDIT_KINDS` `plan.rs:45`, `enum Edit` `:142`, `kind()` `:201`, `needs()` `:219`, plus the `apply_edit` arm | A new edit is four edits that must agree, with no compiler help across the string list |
| The MCP schema for an edit is **an English sentence** | `tools/opendoc-mcp/src/catalog.rs:61-75`; each item is only `{type: object, required: [op]}` | A client cannot validate an edit before sending it, and the sentence drifts from the code silently |
| Reads cover the **body only** | `casual-doc-ai/src/read.rs:31` (`SURFACES = ["body"]`); unknown blocks become `"kind":"other"` (`read.rs:421`), unknown inlines are skipped (`text.rs:119`) | Headers, footers, notes, text boxes, drawings, charts, equations, fields, bookmarks and sections are invisible. A header paragraph is *editable* by id (`anchor.rs:91` resolves every surface) but its id can never be *discovered* |
| **Lowerings live inside wasm methods** | 391 `#[wasm_bindgen]` exports; e.g. `insert_image` `casual-doc-wasm/src/lib.rs:3149`, `set_hyperlink` `:4594`, `insert_footnote` `:4658`, `insert_field` `:5313`, `insert_table` `:7934`, `toc.rs:351` | An agent feature must re-implement the gesture, which is the drift `178` R1 names |
| **The re-implementations already diverge** | AI `table_insert_row` builds the row by hand and nulls `vertical_merge` (`plan.rs:716-780`); the editor uses `table_structure::insert_row` (`casual-doc-edit/src/table_structure.rs:162`, the `171` merged-cell rules). AI `table_delete_row` emits a raw `DeleteRow` (`plan.rs:781-796`); the editor repairs merges through `table_structure::delete_rows` (`:215`) | **A correctness defect, not only a design one:** an agent inserting or deleting a row through a vertically merged table produces a different (and, for delete, an unrepaired) grid from the person doing the same thing. Fixed by R1 below |
| Command ids and the AI vocabulary are **unrelated** | `host_contract.mjs` `COMMAND_CONTRACT` (216 exact + 11 family rows, parity-tested against the runtime registry by `host-contract.spec.mjs`); `casual-doc-sdk/src/host.rs:43-47` deliberately carries no command ids | There is no list from which "which editor features does an agent lack?" can even be computed |
| The `147` mutation guard sees **wasm only** | `every_document_mutation_is_a_transaction`, `casual-doc-wasm/src/lib.rs:41798`; `147` §5 admits other crates are invisible to it | `casual-doc-ai`'s own `Transaction::reserve` (`session.rs:125-155`) is unguarded |
| No schema generation exists | No `schemars`, `ts-rs`, `typeshare` or `specta` in the workspace; `Operation` derives serde only | `178` §3 rule 2 ("one schema, generated") is unbuilt (A4) |
| Capability enforcement **is** inherited | `access::admitted_by` defaults every operation to needing `edit` (`casual-doc-edit/src/access.rs:617-640`) | The one thing a new feature already gets for free. The design keeps it |

The pattern below makes every row except the last one structurally impossible to repeat.

## 1. The patterns, named before any design

| Problem | Known pattern | Precedent |
| --- | --- | --- |
| One capability, many front doors (ribbon, menu, palette, host contract, MCP, Python, JS, WebMCP) | **Single-source registry; every front door generated from it** | Google Docs `batchUpdate`: one endpoint, one `Request` union (53 kinds), a new feature is a new variant, never a new endpoint. VS Code is the counter-example: `languageModelTools` and `commands` are two registries with no link |
| A gesture expressed once and called from the editor and the agent | **Command pattern: a pure lowering `fn(&Document, Args) -> Result<Vec<Operation>>`** | A1 (`casual-doc-review/src/lower.rs`) and `table_structure.rs` already are this |
| "A new feature must not silently miss the AI surface" | **Exhaustiveness as a ratchet**: a `match` with no wildcard arm, so the compiler refuses an unclassified variant; an explicit `NotExposed { reason }` is the only other way out | SKILL §10 "never a dead control: an unbuilt command ships disabled **with a reason**", applied to the agent surface |
| Schemas that cannot drift | **Schema derivation from the type** (`schemars` `JsonSchema`) plus a **generated-file staleness check** | `webapp/build.sh --check` (six generators already work this way) |
| Tool count grows with features; agents choose worse from long lists | **A fixed, small tool set over a growing union, with schema discovery** (Google Docs); **toolsets filtered by profile** (GitHub MCP server: named toolsets, read-only override wins, exclusions win); **deferred loading** (Anthropic tool search: Opus 4.5 79.5% → 88.1% on MCP evaluations) | §5 |
| Agents patch badly by node id, rewrite well | **Rewrite-then-diff**: the agent rewrites a block, the engine computes the minimal edit (LCS) and lowers it | Liveblocks measured node-id patch operations failing and moved to rewrite + diff; §4.3 |
| What an agent may do | **Least-privilege capability**, enforced at the operation | `access.rs`, unchanged |

No pattern is invented here. What is new is applying exhaustiveness to the agent surface,
so that **coverage is a compile-time property, not a review-time hope**.

## 2. Architecture

```
                    editor (wasm)       opendoc-mcp       Python / JS / C ABI     WebMCP (editor.ai)
                         |                   |                   |                      |
                         +-------------------+---------+---------+----------------------+
                                                       v
                 casual-doc-command  — THE capability registry (new crate, Rust, pure)
       CommandSpec × N:  id · family · Args: JsonSchema · lower() · track() · capability · projection
                                                       |
             casual-doc-edit (Operation, apply, access.rs)   casual-doc-review (tracked lowering, A1)
                                                       |
                                           RevisionLog / Transaction (I1, 147)
```

### 2.1 `CommandSpec`: one declaration per feature

A new crate, `casual-doc-command`, depends on `casual-doc-model`, `casual-doc-edit` and
`casual-doc-review` and on nothing above them (no wasm, no layout, no I/O). It holds the
registry. Each entry is a type implementing one trait:

```rust
pub trait Command: DeserializeOwned + Serialize + JsonSchema {
    /// Stable, snake_case, never reused: the MCP edit `op`, the host-contract id suffix,
    /// the Python method name. Renaming one is a breaking API change.
    const ID: &'static str;
    /// The toolset it belongs to (§5.2): text, paragraph, styles, lists, tables, images,
    /// objects, links, fields, notes, sections, headers_footers, review, document.
    const FAMILY: Family;
    /// Editor command ids (COMMAND_CONTRACT) this is the lowering for. May be empty only
    /// for an agent-only convenience; the parity guard (§3, G3) checks both directions.
    const UI_COMMANDS: &'static [&'static str];
    /// Short, literal, model-facing. Generated into the MCP schema and the .d.ts.
    const SUMMARY: &'static str;

    /// Direct lowering. Pure: reads the document, returns operations, mints ids through
    /// `Mint`. O(affected nodes); never a document scan (SKILL §8).
    fn lower(&self, doc: &Document, at: &Resolved, mint: &mut Mint) -> Result<Vec<Operation>, Refusal>;

    /// How it is suggested. `Trackable::Lower` routes through casual-doc-review;
    /// `Trackable::No { reason }` makes the engine refuse it in suggest mode with that
    /// reason (today's `not-trackable`), and the reason is published in the schema.
    fn track(&self) -> Trackable;

    /// 1–5 worked examples, generated into MCP `input_examples` and the docs.
    fn examples() -> &'static [&'static str] { &[] }
}
```

The capability needed is **not** declared here: it is derived from the operations `lower()`
returns, by the existing `access::admitted_by`. A declaration that could disagree with the
engine would be a second source of truth; this keeps `178` §3 rule 4 ("the engine enforces;
the adapter declares") true by construction.

`Resolved` is the output of the anchor resolver (`casual-doc-ai/src/anchor.rs` moves down
into this crate): a `NodeId` plus a byte range, or a block position, never a quote. So the
editor (which has a caret, i.e. a `ModelPos`) and an agent (which has a quote) call the same
`lower()` with the same resolved input, and I3 holds.

The registry is a single `macro_rules!` list, not a runtime map and not a proc-macro:

```rust
commands! {
    // family     type                    
    text      => InsertText, ReplaceText, DeleteText, SetTextFormat, ClearFormatting,
    paragraph => InsertParagraphs, SetAlignment, SetLineSpacing, SetIndent, RewriteBlock,
    styles    => ApplyParagraphStyle, ApplyCharacterStyle,
    tables    => InsertTable, InsertRow, DeleteRow, InsertColumn, DeleteColumn, MergeCells, SetCellText,
    links     => SetHyperlink, RemoveHyperlink, CreateBookmark, ...
    // ...
}
```

It expands to the `enum Edit` (serde-tagged on `op`, `deny_unknown_fields`), its
`JsonSchema`, the `kind()` table, the dispatch, and `ALL: &[CommandMeta]`. The four
hand-spelled lists in `plan.rs` become one line per command.

### 2.2 The read side: `Projection`, by exhaustive match

Reads get the same treatment. `read.rs`'s block walk and `text.rs`'s run projection are
replaced by one function per model node family, reached through a `match` on `BlockNode` and
`InlineNode` **with no wildcard arm**:

```rust
fn project_block(b: &BlockNode, cx: &mut ProjectCx) -> Projected {
    match b {
        BlockNode::Paragraph(p) => paragraph(p, cx),
        BlockNode::Table(t)     => table(t, cx),
        BlockNode::Sdt(s)       => content_control(s, cx),      // tag, alias, type: not flattened
        BlockNode::AltChunk(a)  => Projected::opaque("alt-chunk", a.id, NotProjected::Opaque),
        // no `_ =>`: a new BlockNode variant does not compile until it is classified
    }
}
```

`Projected::opaque` is the read-side `NotExposed`: it names the node and its id, says *why*
the content is not shown, and is counted in the result's `omitted` field, so an agent knows
something is there. Today an unknown block silently becomes `"other"`.

Surfaces become a first-class argument (`Surface::{Body, Header(id), Footer(id),
Footnote(id), Endnote(id), TextBox(id), Comment(id)}`, the `85` *Location* concept) and the
outline enumerates every surface the document has. That closes "editable but undiscoverable".

### 2.3 Who calls the registry

| Caller | Today | After |
| --- | --- | --- |
| The editor's wasm methods | Build operations inline (391 exports) | A command's wasm method resolves the caret/selection to `Resolved` and calls `Command::lower` then `apply_group`. The method shrinks to binding glue, which is also the `177`/HF-109 direction for `main.js` |
| `casual-doc-ai` | Own `enum Edit` and lowerings | `EditPlan.edits: Vec<casual_doc_command::Edit>`; plan, preview, apply and the choke point are unchanged |
| `opendoc-mcp` | Hand-written `catalog.rs` | Schemas generated (§3 G4); the tool list stays fixed (§5) |
| Python / JS / C ABI | Not built | Generated from the same `JsonSchema` (§6) |
| WebMCP / `editor.ai` (`178` M4) | Not built | Same registry, filtered by the mount's role (`178` §9) |

## 3. The inheritance guards

Seven guards. Each is the mechanism by which "the next feature inherits", and each must be
driven red by a mutation when it lands (SKILL §4).

| # | Guard | Fails when | Where |
| --- | --- | --- | --- |
| **G1** | Every `Operation` variant is classified | A new variant is added and no command lowers to it **and** it is not in `NOT_AGENT_REACHABLE` with a reason | `casual-doc-command/tests/operation_coverage.rs`: an exhaustive `match` over `Operation` (compile-time) plus a check that the set reached by `ALL` ∪ the exclusion list is every variant |
| **G2** | Every model node is projected or explicitly opaque | A `BlockNode`/`InlineNode` variant is added | The no-wildcard `match` in §2.2 (compile-time) |
| **G3** | Editor and agent surfaces agree | A `COMMAND_CONTRACT` id that mutates the document has no `CommandSpec` and no `ai: "not-exposed: <reason>"` entry; or a `UI_COMMANDS` id names no contract row | `webapp/tests/command_registry_parity.test.mjs` over a generated `webapp/generated/commands.json` (the registry's `ALL`, emitted by a small bin, checked by `build.sh --check`) |
| **G4** | Generated schemas are current | `catalog` schemas, `.d.ts`, Python stubs or `commands.json` differ from what the registry emits | `--check` mode of the generator, run in `build.sh` and in the Rust `test` job |
| **G5** | Suggest parity | A command with `Trackable::Lower` does not round-trip: applied in suggest mode, saved as DOCX, reopened, it must read back as revisions by the session author and rejecting all must restore the original | One generic test iterating `ALL` with each command's first example, not one test per command |
| **G6** | One mutation path, every crate | A crate other than `casual-doc-edit` builds a `Transaction` or calls `apply` outside the one choke point | Extend `147`'s source scan from `casual-doc-wasm` to `casual-doc-ai` and `casual-doc-command` (it is a source scan; it can read three crates as easily as one) |
| **G7** | Editor/agent lowering parity | The wasm method for a `UI_COMMANDS` id produces operations different from `Command::lower` on the same resolved input | Per family, a test drives the wasm method and the command on the same fixture and compares the operation lists. This is the guard that would have caught the table-row divergence in §0 |

The exclusion lists (`NOT_AGENT_REACHABLE`, `Projected::opaque` reasons, `ai: not-exposed`)
are **ratchets**: a test asserts their length can only fall, the way the `main.js` line
ceiling works. Each entry is a sentence a user could read ("needs tracked table structure,
Q11"; "equations need the math offset space unified, `134`").

**What "a feature inherits MCP" therefore means, concretely.** A developer adding, say,
footnote insertion writes `InsertFootnote: Command` with its `lower()` (moved out of
`lib.rs:4658`), adds one line to `commands!`, and points the editor's method at it. With no
further work: the MCP `apply_edits`/`preview_edits` accept `{op: "insert_footnote", ...}`,
validated by a generated schema; `describe_edit_kind("insert_footnote")` documents it; the
Python and TS types have it; WebMCP offers it to roles whose capability admits its
operations; suggest mode either works (G5 proves it) or refuses with the published reason;
and the parity guards hold the editor and agent to one behaviour. Forgetting the line does
not compile (G1) or fails CI (G3).

## 4. The edit vocabulary, opened safely

`178` §4.3 rule 3 said the vocabulary is "versioned and closed". It stays **closed** in the
sense that matters (no unknown `op` is ever ignored, no raw escape hatch, `178` rule 4) but it
**grows by registration** instead of by editing a hand-kept table.

### 4.1 Versioning

- The registry carries `VOCABULARY_VERSION`. Adding a command is a minor bump; changing or
  removing one is major and needs an ADR.
- A plan may state `vocabulary` (optional). An `op` newer than the plan's stated version is
  still accepted; an `op` the server does not know is `unknown-edit` with the list of known
  ones in the refusal, as today.
- The generated schema for each `op` is published at
  `opendoc://schema/edits/{family}` and through `describe_edit_kind`, so a client learns
  new commands without a server release note.

### 4.2 Families that arrive first (R1 order)

Ordered by agent value per unit of work, and so that the divergence in §0 is fixed first:

1. **tables**: route `table_insert_row`/`table_delete_row` through `table_structure`, then
   add `insert_table`, columns, merge/split, cell properties. Fixes the defect.
2. **text + paragraph + styles**: unify the formatting deltas (`plan.rs:342` vs
   `lib.rs:6283/6449/6471` and `character_format.rs:133`), alignment, spacing, indent,
   character styles, clear formatting.
3. **lists**: numbering on/off, level, restart (`76`, `142`).
4. **links + bookmarks + fields + TOC**: `set_hyperlink`, bookmarks, cross-references,
   `insert_table_of_contents`, page-number fields.
5. **images + objects**: insert image (with the resource write through `SetMediaReference`),
   alt text, size, wrap, crop.
6. **notes + headers/footers + sections**: needs §2.2's surfaces, and `85`'s Location.
7. **charts, equations, content controls**: last, each behind the model work it needs
   (`155`, `134`, Q11-style trackability).

### 4.3 `rewrite_block`: the edit language models are good at

Evidence from Liveblocks' copilot: models rewrite well and patch by node id badly. Our quote
anchors already avoid offsets, but a multi-change edit to one paragraph still forces the
agent to emit several precise edits. Add one command:

`rewrite_block { target: block anchor, markdown: "…" }` — the engine parses the
anchored-Markdown dialect `read_blocks` already emits, computes a word-level LCS diff
against the block's current text and marks, and lowers the difference through the
ordinary text and format commands (so suggest mode yields minimal, reviewable revisions, not
a delete-all/insert-all). Bounded to one block (or one table cell) per edit; formatting the
dialect cannot express is preserved, not dropped (MF-I3). The diff is `casual-doc-diff`'s,
already budgeted.

### 4.4 A private preview mode

Gemini in Docs and Tiptap keep an AI suggestion private until the requester accepts it;
ours are durable tracked changes everyone sees. Both are legitimate. Add `mode: "preview"`
to `apply_edits` in the editor (`178` M4) only: the plan is applied to a copy-on-write
overlay shown to the requester alone and committed (as suggestions or direct, per role) on
accept. Headless sessions do not need it: `preview_edits` already is that.

## 5. The MCP surface stays small while coverage grows

### 5.1 Fixed tools, growing union

The tool list stays at `178` §6.2's twenty. New features add `op`s to `apply_edits` /
`preview_edits` and node kinds to the read tools, never new tools. Two tools are added once:

| Tool | Profile | What it does |
| --- | --- | --- |
| `list_edit_kinds` | read | The families and `op`s this session's profile admits, with one-line summaries and the vocabulary version |
| `describe_edit_kind` | read | One `op`'s generated JSON Schema, its trackability, and its examples |

`apply_edits`'s own `inputSchema` carries the full generated `oneOf` for clients that
validate up front; the two tools exist for clients and models that do better with
discovery than with one large schema.

### 5.2 Toolsets and profiles

- `--toolsets text,tables,…` limits the admitted families (GitHub MCP's model); `read` is
  always present; `--read-only` overrides everything else.
- The profile (`read`/`comment`/`suggest`/`edit`/`review`) still filters by capability, and
  the engine still enforces it.
- Tools rarely needed in a session (`compare_documents`, `validate_document`,
  `render_page`) are marked for deferred loading where the client supports it.

### 5.3 Protocol layer: reconsider `rmcp`

`178` Q4 chose a hand-rolled JSON-RPC layer. The official Rust SDK, `rmcp` 3.8.0
(2026-10-26), is now Apache-2.0 on crates.io, implements MCP 2026-07-28 statelessly and keeps
2025-11-25 compatibility, and derives input/output schemas from `schemars` types. With the
registry deriving `JsonSchema` anyway, `rmcp` would remove the protocol code we maintain.
The repository's licence file is mid-transition from MIT, which `deny.toml` admits either
way. **Q2** asks whether to adopt it; the registry does not depend on the answer.

## 6. Generated front doors

| Front door | Generated from the registry | Tool |
| --- | --- | --- |
| MCP `inputSchema`/`outputSchema`, `describe_edit_kind` | `JsonSchema` | `schemars` 1.x (MIT) |
| `webapp/generated/commands.json` (G3) | `ALL` | the registry's own bin |
| TypeScript `.d.ts` for `@casualoffice/opendoc-ai` and `editor.ai` | `JsonSchema` | `ts-rs` (MIT) or a JSON-Schema-to-TS step; chosen at R4 |
| Python stubs and bindings (`opendoc.ai`) | `JsonSchema` + `pyo3` (MIT/Apache) | `uniffi` is MPL-2.0 and is **not** in `deny.toml`'s allowlist; not proposed |
| The vocabulary table in this document and `178` §4.3 | `ALL` | the doc-page generator, so counts are derived (SKILL §8, §9) |

## 7. Delivery

Each phase is one PR with its guards driven red. Phases R0–R2 need no owner decision beyond
accepting this document; R3 waits on Q2.

| Phase | Delivers | Exit gate |
| --- | --- | --- |
| **R0** registry skeleton | `casual-doc-command` with the trait, `commands!`, the anchor resolver moved down, the 12 existing edits ported unchanged; G1, G2 (ratchet lists seeded from today's 58 unreachable operations and the opaque node kinds), G6 | `casual-doc-ai/tests/edits.rs` and the MCP conformance suite unchanged; G1 red when a test-only `Operation` variant is added; G2 red when a test-only node variant is added |
| **R1** families, in §4.2 order | Each family's lowering moved out of `casual-doc-wasm` into a `Command`; the wasm method calls it; G7 per family; G5 for every trackable command | The table-row divergence fixed and G7-guarded; `NOT_AGENT_REACHABLE` shrinks with each family and its ratchet holds |
| **R2** surfaces | `Surface` argument, outline of every surface, projections for headers/footers/notes/text boxes/drawings/fields/bookmarks/content controls | An agent reads and suggests into a footnote and a header, saved DOCX reopens in Word with the revision in that part |
| **R3** generated schemas + protocol | G4; catalog generated; `list_edit_kinds`/`describe_edit_kind`; toolsets; `rmcp` if Q2 says so | MCP conformance (`@modelcontextprotocol/conformance --spec-version 2026-07-28`, wrapped for stdio) green; G4 red on a hand-edited schema |
| **R4** bindings | Python and JS `opendoc.ai` generated; G3 | A Python agent and a JS agent apply the same plan to the same fixture and produce byte-identical DOCX |
| then | `178` M3 (HTTP/OAuth), M4 (WebMCP + preview mode §4.4), M5 (agent in a room) | as `178` §8 |

**Order against the rest of the queue.** R0 and the tables half of R1 are worth doing now:
they fix a live divergence and they make every later feature cheaper. The other R1 families
follow the editor's own feature work (a family moves when someone next touches it), with the
ratchet keeping the direction one-way.

## 8. Risks

| # | Risk | Mitigation |
| --- | --- | --- |
| R1 | Moving lowerings out of the 51k-line `lib.rs` breaks the editor | One family per PR; G7 compares old and new operation lists before the old code is deleted; the editor's e2e for that family is in the scoped run |
| R2 | The generated `oneOf` grows too large for some clients | Per-family discovery (§5.1), toolsets, deferred loading; measured at R3 with Claude and one other client |
| R3 | `rewrite_block` diffs produce noisy revisions | Word-level LCS on the block only; a test fixes the revision count for representative rewrites |
| R4 | Ratchet lists become a dumping ground | Each entry needs a reason sentence; the list length can only fall; additions need an ADR line |
| R5 | `schemars` output and the MCP 2026-07-28 schema dialect disagree | R3 conformance run; `schemars` targets JSON Schema 2020-12, which MCP uses |
| R6 | Prompt injection through new node kinds (field codes, alt text, hyperlink targets) | `178` §5.8 unchanged: projected text is data; new projections are added to the `132` §14 injection corpus |

## 9. Open questions for the owner

- **Q1. Accept the registry crate** (`casual-doc-command`) as the one place a feature's
  lowering lives, with wasm methods as glue over it?
- **Q2. Adopt `rmcp`** (Apache-2.0, MCP 2026-07-28) and retire the hand-rolled JSON-RPC
  layer, reversing `178` Q4?
- **Q3. `rewrite_block`** in the first vocabulary expansion, or after the families?
- **Q4. Private preview mode** (§4.4) for the in-page assistant?
- **Q5. Python bindings via `pyo3`** (and therefore no `uniffi`), confirming `deny.toml`
  stays MPL-free?
- **Q6. Tracked table structure** (`178` Q11) — the largest single block in the ratchet; it
  decides whether table edits can ever be suggestions.

## Owner decisions, 2026-10-28

- **Complete `181` in one PR (#897)**; the owner will not merge it before it is complete.
  PDF import (`182`) is not part of it and stays a proposal.
- **Q2 — protocol:** keep the hand-rolled JSON-RPC layer; no `rmcp`. Revisit with M3.
- **Q5 — bindings:** generated JSON Schema and TypeScript types now; the Python package
  (pyo3) in a later PR with its own CI job.
- **Q3 / Q4 — extras:** build `rewrite_block` (§4.3) and the private preview mode (§4.4).

## Owner decisions, 2026-10-29

- **ADR-075 accepted:** the registry is the permanent pattern; every new feature is declared
  once in `casual-doc-command`, and CI fails when one is neither offered to agents nor
  excluded with a reason.
- **The AI author is "Casual AI"** (`docs/178` Q3), whoever asked: `casual_doc_ai::AI_AUTHOR`,
  used by the editor's assistant, the AI layer's default and `opendoc-mcp`'s default (an
  operator may still pass `--author`). It is written into `w:author`.
- **A section break copies form protection**, as Word copies all section settings, so
  `SetSectionFormProtection` stays classified as reached; no command sets or lifts it.
- **PDF import (`182`) starts now, in its own PR,** with the recommended defaults.

## 10. Sources

- Google Docs API request union: <https://developers.google.com/workspace/docs/api/reference/rest/v1/documents/request>
- GitHub MCP server toolsets, read-only override: <https://github.com/github/github-mcp-server/blob/main/docs/server-configuration.md>
- VS Code language model tools: <https://code.visualstudio.com/api/extension-guides/ai/tools>
- Anthropic, advanced tool use (tool search, input examples): <https://anthropic.com/engineering/advanced-tool-use>;
  writing tools for agents: <https://www.anthropic.com/engineering/writing-tools-for-agents>;
  code execution with MCP: <https://www.anthropic.com/engineering/code-execution-with-mcp>
- Cloudflare code mode: <https://blog.cloudflare.com/code-mode-mcp/>
- Liveblocks, rewrite-then-diff: <https://liveblocks.io/blog/building-an-ai-copilot-inside-your-tiptap-text-editor>
- Tiptap AI Toolkit review modes: <https://tiptap.dev/docs/ai/ai-toolkit/client/agents/review-changes>
- Gemini suggestions in Docs: <https://workspaceupdates.googleblog.com/2026/07/streamline-collaboration-in-google-docs-with-Gemini-powered-comment-workflows.html>
- `rmcp`: <https://github.com/modelcontextprotocol/rust-sdk>; MCP conformance suite:
  <https://github.com/modelcontextprotocol/conformance>; MCP 2026-07-28 security best
  practices (handle hijacking): <https://modelcontextprotocol.io/specification/2026-07-28/basic/security_best_practices>
- Licences checked on crates.io 2026-10-28: `rmcp` 3.8.0 Apache-2.0, `schemars` 1.2.3 MIT,
  `ts-rs` 12.0.1 MIT, `pyo3` 0.30.1 MIT/Apache-2.0, `uniffi` 0.32.2 MPL-2.0.
- Not verified from a primary source, and not load-bearing here: the GitHub dynamic-toolsets
  meta-tool names; tool-count accuracy cliffs quoted in secondary posts.

## 11. What the first change was

Documentation only: this document, ADR-075 (proposed) and `docs/109` RM-48.

## 12. Built

### Where it stands (2026-10-29)

Counts are not repeated here: the registry is `casual_doc_command::COMMANDS`, its vocabulary
version is `schema::VOCABULARY_VERSION`, and the two ratchets are `coverage::NOT_EXPOSED_CEILING`
(engine operations no command emits) and `registry::UI_NOT_EXPOSED_CEILING` (editor commands
with no agent twin). `webapp/generated/edit-vocabulary.md` is the generated table of every
command, and a test fails when it is stale.

- **The pattern is in force.** A new engine operation does not compile until `coverage.rs`
  classifies it; a new model node does not compile until the read projection classifies it; a
  new editor command fails `command_registry_parity` until it has an agent twin or a reason; a
  command that emits an undeclared operation is refused at the AI choke point; every
  trackable command must round-trip as a suggestion under a real suggest-only profile (G5);
  every untracked example must apply, save and reopen; and editor/agent lowerings are compared
  per family (G7). Adding a feature for agents is one `Edit` variant, one `COMMANDS` entry,
  one shared lowering and one `apply_edit` arm; the guards name whatever is missing.
- **Families shared by the editor and agents:** tables, text, paragraph, styles, lists,
  links, fields, objects, notes, sections, headers and footers, charts, and references
  (contents, captions, cross-references), plus `rewrite_block`.
- **Surfaces:** body, headers, footers, footnotes, endnotes, text boxes and comments.
- **Front doors:** MCP (with `list_edit_kinds`, `describe_edit_kind`, schema resources,
  `--toolsets`, `--read-only`), generated JSON Schema and TypeScript types, and the editor's
  `editor.ai` with its private preview.
- **Deviation from §2.1, deliberate:** the registry is a `const` table beside a typed,
  `JsonSchema`-deriving `Edit` enum, held equal by a test, rather than a `Command` trait and a
  generating macro. It gives the same guarantees (exhaustive, generated, enforced) with
  ordinary Rust, and nothing a trait would add has been needed.

**Not built, and why.**

- *By owner decision (2026-10-28):* the Python package (later PR); `rmcp` (kept hand-rolled);
  the official conformance suite as a CI job (needs the owner's approval; it runs locally, see
  "Lane generation").
- *No editor gesture to share:* removing a field or a field range, inserting an equation or a
  content control, inserting shapes and text boxes and arranging objects, removing a note or a
  header/footer body, chart data editing beyond sample data.
- *Policy:* document-wide settings and protection are a person's decision.
- *Needs model or engine work first:* character-style application (no `FormatDelta` field
  for it), paragraph-number cross-references (list-number resolution), splitting a cell into
  a grid, whole-table properties, tracked table structure (`178` Q11).
- `178` M3 (HTTP/OAuth), M4 (WebMCP) and M5 (an agent in a room) stay as `178` plans them.

### R0 and R1 step 1 (2026-10-28)

**Built.**

- `crates/casual-doc-command`, depending only on `casual-doc-model` and `casual-doc-edit`:
  - `registry::COMMANDS`: the twelve existing edits, each with its id, family, summary,
    the operations it may emit, its trackability (with the published reason when it is not)
    and a worked example.
  - `coverage::OPERATIONS`: all 67 operations, 10 reached and 57 not exposed, each with a
    reason. One macro list expands into the list and into an exhaustive, wildcard-free
    `match`, so the two cannot disagree and an unclassified operation does not compile.
  - `table::{insert_row, insert_row_at, delete_row}`: the `docs/171` merged-cell rule plus
    the InsertRow/DeleteRow-or-ReplaceTable choice, moved out of the editor's wasm methods.
- The editor's `insertRow`/`deleteRow` and the AI layer's `table_insert_row`/
  `table_delete_row` both call `casual_doc_command::table`. **The divergence in §0 is fixed:**
  an agent's row inserted inside a vertical merge now continues it, and an agent deleting the
  top row of a merge keeps the merged cell's text on the next row.
- `casual-doc-ai` parses plans against the registry: `EDIT_KINDS` is gone and an `op` the
  registry does not name is `unknown-edit`. The suggest-mode refusal for rows quotes the
  registry's reason. **Declared emits are enforced:** the AI choke point refuses a lowering
  that emits an operation its command does not declare, so `coverage` is true of the code.
- `opendoc-mcp` publishes the registry's ids as the `op` enum of `preview_edits` and
  `apply_edits`.
- The AI layer's inline run walk (`text.rs`) is an exhaustive match (G2 for inlines; the
  block walk already was).

**Guards, each driven red by a mutation and restored:**

| Guard | Mutation | Red |
| --- | --- | --- |
| Agent row insert grows the merge | The old AI insert (fresh row, `vertical_merge` cleared, raw `InsertRow`) | `left: Null, right: "continue"` |
| Agent row delete keeps the merged text | The old raw `DeleteRow` | the new top row read `("", "continue")`, the text gone |
| Declared emits are enforced | Drop `ReplaceTable` from `table_delete_row`'s `emits` | `engine-refused`: "`table_delete_row` lowered to `ReplaceTable`, which the capability registry does not declare" |
| G1 reached ⇔ emitted | Mark `ClearFormatting` reached | "`ClearFormatting` is marked reached by agents, but no registry command emits it", and the ratchet count moved |
| G1 exhaustive | Delete `SetDocumentBackground`'s line | `E0004: non-exhaustive patterns` in `coverage.rs` |
| G2 inlines exhaustive | Delete the `Math` arm | `E0004` in `text.rs` |
| G6 one mutation path | A second `Transaction::reserve(` in `plan.rs` | `left: ["session.rs", "plan.rs"]` |

What this first step left open was built by the lanes below; "Where it stands" above is
the current account.

### Lane surfaces

**R2 built (`casual-doc-ai`, `opendoc-mcp` read tools).**

- `Surface::{Body, Header(id), Footer(id), Footnote(id), Endnote(id), TextBox(id),
  Comment(id)}` (`casual-doc-ai/src/surface.rs`), spelled `body` or `<kind>:<id>`. It is the
  engine's `casual_doc_edit::Surface` plus text boxes and comment bodies; `85` §8c.3 holds:
  ids are unique across stories, so an anchor needs no surface and a read does. `enumerate`
  walks the document once (O(document)); no per-node scan.
- `get_outline` lists every surface, paged by `surfaces_cursor`: id, kind, section and page
  type for a header or footer, the referencing paragraph for a note, the host paragraph for a
  text box, the author and `editable: false` for a comment, paragraph and table counts,
  estimated tokens and a preview. It also lists bookmarks (with their paragraph and surface)
  and counts links, fields, objects and content controls document-wide.
- `read_blocks` takes `surface`; without one it reads the cursor's surface, else the surface
  holding `start`, else the body. A cursor names its surface and cannot page another.
  `search` and `chunk_document` take `surface` (one, or `all`); the default stays the body
  (and, for chunks, comment threads). `read_table` names the table's surface. Every hit,
  chunk and result names its surface.
- Projections (`casual-doc-ai/src/project.rs`, one wildcard-free match on `InlineNode`):
  links with their URL or bookmark, fields with kind, instruction and result (and
  paragraph-spanning fields from `field_ranges`), bookmarks by name, note references with the
  note's surface, comment references, inline content controls, and objects: pictures and
  floating pictures with alt text and extent, text boxes and group text boxes with their
  surface, shape groups with their children, equations with their linear text. A block
  content control is its own entry (type, tag, alias, lock) and the blocks it wraps name it.
  Charts, SmartArt, OLE objects, other graphics, alt-chunks and equations without a linear
  form are named with a reason and counted in each result's `omitted`. All of it is
  structured data, bounded per paragraph and kind (`178` §5.8).
- Anchors resolve on every surface the read tools return ids from. A comment body's
  paragraph is refused with `unsupported-surface` and the reason: a comment changes through
  `comment` and `resolve_comment`, and the engine's paragraph operations do not reach it.
- **Exit gate passed, with no change to `plan.rs`:** an agent finds a header paragraph and a
  footnote paragraph through `get_outline` and `read_blocks`, suggests a replacement into
  each, saves DOCX; `word/header1.xml` and `word/footnotes.xml` each carry `w:ins` and `w:del`
  by the session author, `word/document.xml` carries none, and the reopened document lists
  exactly four revisions anchored in those two paragraphs
  (`casual-doc-ai/tests/surfaces.rs`). The check is the package XML, not Word itself.

### Lane generation (G3, G4, §5, the R3 conformance run)

**Built.**

- **G4, generated front doors.** `opendoc-command-gen` (a bin in `casual-doc-command`, no
  new dependency) writes `webapp/generated/`: `edit-schema.json`, one
  `edit-schema/<family>.json` per family, `commands.json`, `opendoc-edits.d.ts` and
  `edit-vocabulary.md` (the vocabulary table, every count derived from the registry). The
  TypeScript comes from a JSON-Schema-subset converter in `schema/typescript.rs`; a keyword
  it does not translate fails generation instead of publishing `unknown`. `--check` fails
  on a file that differs, is missing, or sits in a generated directory unproduced. It runs
  in `webapp/build.sh` (so in CI's `browser-smoke` and the Pages deploy) and, as
  `casual-doc-command/tests/generated.rs`, in CI's `test` job. `VOCABULARY_VERSION` is
  `1.0`; `VOCABULARY_COMMANDS` beside it makes adding a command decide the version bump.
- **G3, editor ↔ registry parity.** Each command's `ui_commands` names its editor twins
  from `COMMAND_CONTRACT` (an id ending in `.` is a value family). `UI_NOT_EXPOSED` in
  `registry.rs` lists, each with a reason, the contract ids that can change a document but
  have no agent twin; its length is the ratchet `UI_NOT_EXPOSED_CEILING`.
  `webapp/tests/command_registry_parity.test.mjs` holds both directions over the generated
  `commands.json`. **The rule for "can change the document"** is the contract's own data:
  a row whose `requires` is `mutate`, `edit` or `comment`. The units are every such exact
  row, every such family prefix, and every member of such a family the webapp enumerates
  statically (`TABLE_MENU_LABELS` for `table.`). The two rows the broad rule sweeps in
  that change the editor rather than the document (`review.mode.editing`,
  `review.mode.suggesting`) are in the list with that reason, so the rule has no
  exceptions of its own.
- **§5, discovery and toolsets.** `list_edit_kinds` and `describe_edit_kind` (read
  profile), the resource template `opendoc://schema/edits/{family}`, `--toolsets` and
  `--read-only` (the read profile, whatever order the flags come in). What a session
  admits is asked of `casual-doc-ai` itself: at start-up each command's example is
  previewed against a one-line document in each mode, so the listing cannot disagree with
  what `apply_edits` enforces. Toolsets are enforced in `opendoc-mcp` on `apply_edits` and
  `preview_edits` (`toolset-withheld`), the published plan schema narrows to the admitted
  families, and `add_comment`/`review_tracked_changes` are not offered when `review` is
  excluded. The plan schema's hand-kept English op list is gone.
- **R3, the official conformance suite.** `@modelcontextprotocol/conformance`
  0.2.0-alpha.12, `--spec-version 2026-07-28 --suite all`, through
  `tools/opendoc-mcp/conformance/stdio-http-bridge.mjs` (a development bridge, not a
  product feature). First run: 67 checks passed, 51 failed. It found real defects, all
  fixed and guarded in `tests/conformance.rs`: `ttlMs`/`cacheScope` missing on
  `server/discover`, `prompts/list` and `resources/templates/list`; no SEP-2575 stateless
  request validation (`_meta` version and client capabilities, `-32022` for an unknown
  version, `-32601` for the removed methods, `serverInfo` in the result `_meta`); SEP-2164
  resource-not-found as `-32002`; an unvalidated `inputResponses`. A client that opens with
  `initialize` keeps the older lifecycle (`178` Q12). Final run: 96 passed, 31 failed;
  every remaining failure drives the reference server's fixture tools, prompts or
  resources, or an optional feature this server does not declare (completions, progress).
  `tools/opendoc-mcp/conformance/expected-failures.yml` lists each with its reason, and a
  baseline run exits 0.

### Lane families-a

R1 steps 1 to 3 (tables, text + paragraph, lists), `rewrite_block` (§4.3) and guards G5 and
G7.

**Built.**

- One lowering per family in `casual-doc-command`, called by the editor's wasm method and by
  `casual-doc-ai` alike; selection and caret resolution stay in the wasm method:
  - `table`: `insert_column_beside`/`insert_column_at`, `delete_column_of_cell`/
    `delete_columns` (the InsertColumn/DeleteColumn-or-ReplaceTable choice and the caret
    cell), `merge` + `cell_rect` (the selection's grow-to-whole-merged-cells rule),
    `unmerge`, `delete_table`, `insert_table` + `body_index_after`, `range_cells` +
    `set_cell_properties`, `cell_vertical_alignment`. The editor's `insertColumn`,
    `deleteColumn`, `mergeTableSelection`/`mergeTableCellRange`, `splitMergedCell` (unmerge),
    `deleteTable`, `insertTable` and every cell-format method call them.
  - `format`: `delta` (an agent's `TextFormat`), `parse_color`, `parse_highlight`,
    `half_points`, `format_ranges`, `clear_ranges`. The editor's `formatSelection`, every
    run-format setter (`setHighlight`, `setFontSize`, colour, underline, baseline) and
    `clearFormatting` call them; the AI layer's private `delta`/`color`/`highlight` are gone.
  - `paragraph`: `ParagraphChange` (alignment, line spacing, space before/after, indents,
    style, numbering, list level) and `set_properties`, with the caller's tracker (the
    editor's Suggesting tracker, or `lower::track_paragraph_change` for an agent). Every
    editor paragraph setter and the ruler's indent setters call it.
  - `lists`: `level`, `install`, `new_definition`, `existing_definition`, `number_format`,
    `restart`, `continue_numbering`. The editor's `toggleList`/checklist definitions,
    `restartList`/`setListNumberingValue` and `continueList` call them.
  - `rewrite`: the `read_blocks` line dialect (`{#id}` prefix, ATX heading marker, text).
- Seventeen registry commands, each with `emits`, trackability, a working example and the
  editor command ids it twins (`ui_commands`; the existing rows, styles and text-format
  commands got theirs too): `clear_formatting`, `rewrite_block`, `set_alignment`,
  `set_line_spacing`, `set_paragraph_spacing`, `set_indent`, `insert_table`,
  `table_insert_column`, `table_delete_column`, `table_merge_cells`, `table_split_cell`,
  `table_delete`, `table_set_cell_format`, `set_list`, `change_list_level`,
  `restart_numbering`, `continue_numbering`. A new `lists` family.
- Coverage: `InsertColumn`, `DeleteColumn`, `InsertTable`, `DeleteTable`,
  `SetTableCellProperties`, `ClearFormatting`, `SetAbstractNumbering` and
  `SetNumberingInstance` are reached; **`NOT_EXPOSED_CEILING` 57 → 49**.
- `rewrite_block` diffs with `casual_doc_diff::inline::diff_text` (the comparison feature's
  word-level anchored aligner, not a second LCS) and lowers each edit right to left through
  `suggest_replace`/`suggest_delete`/`suggest_insert` (or `DeleteText`/`InsertText` direct);
  a heading marker that names another level applies that heading style through the
  paragraph lowering.
- Registry examples are written against `EXAMPLE_PARAGRAPH` and `EXAMPLE_IDS`, so G5 runs
  each one for real.

**Guards, each driven red by a mutation and restored:**

| Guard | Mutation | Red |
| --- | --- | --- |
| G2 inlines (projection) | Delete the `Math` arm in `project.rs` | `E0004: non-exhaustive patterns: InlineNode::Math(_) not covered` |
| G2 blocks | Delete the `AltChunk` arm in `structure::blocks_in` | `E0004: non-exhaustive patterns: BlockNode::AltChunk(_) not covered` |
| Every surface is listed | Drop the footnote loop from `surface::enumerate` | outline kinds `left: [body, header, footer, endnote, comment, text_box]`; the exit gate "one footnote surface, left: 0" |
| Text boxes are surfaces | `InlineNode::TextBox(_) => {}` in `text_boxes_in` | outline kinds lack `text_box`; search-all and anchor tests red |
| Content controls not flattened | Do not emit the control's own entry in `blocks_in` | projection test: `expect("the control")` |
| Omitted is counted | Drop the equation's `omitted.push` | `omitted.count left: 0, right: 1` |
| Hyperlinks carry their URL | Drop `value["url"]` | `left: Null, right: "https://example.com/terms"` |
| Comment bodies refuse with a reason | Remove the comment branch in `anchor::paragraph` | `left: "anchor-not-found", right: "unsupported-surface"` |
| Anchors resolve on every surface | `anchor::paragraph` looks in the body only | header/footnote anchors fail; the exit gate's plan is refused |

**Found, not fixed here (outside this lane's files).** An inline DrawingML text box is modeled
(its paragraphs are a surface) but the importer also reports the drawing as omitted, so a
DOCX save of a document holding one is refused by the loss gate unless `drawing` is
accepted. The exit-gate fixture therefore has no text box; the read tests use one.
Deletions are not in a paragraph's `text` or `runs` (the engine's final-with-markup
projection shows insertions only); `list_tracked_changes` is where deletions are read.

**Not built.** Chart series and data, SmartArt and OLE content (named and counted, not
read); `read_table` cells list text and the first paragraph id, not their links or objects;
the outline's heading tree covers the body only; the MCP conformance suite calls the read
tools without a `surface` argument (the library tests carry the surface cases).
| G4 generated files current | Hand-edit `edit-schema/text.json`, add a stray file | `--check` exit 1: "text.json differs from what the registry generates", "stray.json is not generated by anything"; `tests/generated.rs` the same |
| G4 source drift | Change a command summary, do not regenerate | `--check` names `commands.json`, `opendoc-edits.d.ts`, `edit-vocabulary.md` |
| Vocabulary version decided | `VOCABULARY_COMMANDS` 11 | "the registry has 12 commands but vocabulary 1.0 names 11" |
| TS converter refuses what it cannot translate | Accept any keyword | `an_unknown_keyword_is_an_error_not_unknown` fails |
| G3 ratchet | Drop `format.painter` from `UI_NOT_EXPOSED` | Rust `left: 136, right: 137`; parity test missing `['format.painter (exact)']` |
| G3 enumerated family members | Drop `table.merge` | missing `['table.merge (member)']` |
| G3 twin exists in the contract | `delete_text` names `edit.shred` | "names `edit.shred` as its editor twin, but the host contract has no such command or family" |
| G3 named once | `delete_text` names `format.painter` while it is excluded | both the Rust and the JS guard name the overlap |
| G3 new editor command | `exact("insert.signature", "mutate")` in the contract | missing `['insert.signature (exact)']` |
| Admission asked of the engine | Probe never matches `capability-withheld` | suggest profile lists text edits as `["suggest", "direct"]` |
| Toolsets enforced | `apply_edits` without the check | `table_insert_row` "was not refused" |
| `--read-only` wins | Remove the override | "apply_edits is hidden under --read-only" |
| 2026-07-28 cache fields | `prompts/list` without them | "prompts/list carries ttlMs" |
| Stateless `_meta` required | Accept meta-less requests without `initialize` | "no initialize and no _meta … left: Null, right: -32602" |
| Removed methods | `initialize` off the removed list | "initialize: … right: -32601" |
| SEP-2164 | Back to `-32002` | `left: Number(-32002)` |
| SEP-2322 | `inputResponses` checked only for null | "a malformed inputResponses is a protocol error … left: Null" |

**Not built, and why.**

- The toolset check lives in `opendoc-mcp` because `casual-doc-ai/src` belonged to another
  lane. It belongs beside the profile check in `casual-doc-ai`'s plan (one refusal path for
  every front door, WebMCP included); move it there with the next change to that file.
  The profile's per-op needs (`needs` in `plan.rs`) should likewise become registry data,
  which would retire the start-up probe.
- **Deferred loading:** the MCP 2026-07-28 `Tool` (name, title, description, icons,
  inputSchema, outputSchema, annotations, `_meta`) has no field for it, so none is invented.
- The conformance run is not a CI job: that is a new CI dependency and needs the owner.
- When a family lane registers a command that names an id now in `UI_NOT_EXPOSED`, both
  G3 guards fail until the entry is removed and the ceiling lowered; that is the ratchet
  working, not a conflict to silence.
- Python stubs: a later PR with its own CI job (owner decision Q5).

| G5: every `Trackable::Yes` command round-trips (suggest → DOCX → reopen → revisions by the author → reject-all restores) | Mark `clear_formatting` `Trackable::Yes` (the registry line only) | `clear_formatting: no revision survived the DOCX save` |
| G5, the restore half | `rewrite_block` suggest replacement lowered as a tracked delete plus an untracked `InsertText` | `rewrite_block: reject-all did not restore the document … "sixtythirty (6030) days"` |
| `rewrite_block` is minimal and keeps formatting | Replace the word diff with one whole-paragraph edit | `one word, one replacement` (count); `bold the Markdown cannot say survives …` `left: []`; an unchanged rewrite no longer `no-change` |
| G7 tables | `insertTable`'s content width off by one twip | `insert table: editor and command differ` |
| G7 text | The editor's highlight table reads `gray` as yellow | `character_formatting_is_one_lowering…` `left != right` |
| G7 paragraph | `setSpaceAfter` re-inlined, writing a negative value instead of clearing | `SpaceAfter(-1): editor and command differ` |
| G7 lists | The editor's list definition built with eight levels | `toggle list: editor and command differ` |
| G7 source half | `deleteTable` re-inlined as `vec![Operation::DeleteTable { table }]` | `lib.rs:7673: self.apply_action_caret(vec![Operation::DeleteTable {` |

**Behaviour changes, deliberate.**

- An agent's highlight now accepts `gray`/`grey` (the editor always did); an agent's font size
  must be a half-point step (the editor's rule; it used to be rounded).
- None for editor users: G7 holds the editor's operations equal to the command's on every
  moved path.

**Not built, and why.**

- **Character styles.** The editor has no character-style application and `FormatDelta` has
  no style field, so there is nothing to move; it needs an operation change in
  `casual-doc-edit` first.
- **Inline marks in `rewrite_block`.** `read_blocks` emits no bold/italic marks, so the
  dialect has none (a `*` is text). Formatting is never dropped: kept words keep their runs.
- **`splitMergedCell` with counts** (split one cell into an n×m grid) stays in the editor;
  agents get unmerge only.
- **Whole-table properties** (width, borders, style, alignment) stay not exposed.
- **Defect found, not fixed (outside this lane's files):** a session whose profile is
  `suggest` cannot suggest formatting or paragraph changes. `set_text_format` and
  `apply_paragraph_style` (pre-existing) and the new paragraph and list commands are refused
  by the engine with `session.suggestions-only-access`, because
  `casual-doc-edit/src/protection.rs` `is_tracked_only` recognises only text revisions: a
  run's `rPrChange` is not reverted by `before_projection`, and a `SetParagraphProperties`
  carrying a `pPrChange` is not considered at all. G5 therefore runs under an `edit` profile
  in suggest mode. The fix belongs in `is_tracked_only` (treat a property change whose
  recorded prior equals the current base as tracked-only) with its own access tests.

### Lane families-b

R1 steps 4 to 7 (§4.2) as far as the editor has a lowering: links, bookmarks and fields;
pictures and object formatting; notes, breaks, sections, page setup, headers and footers;
chart insertion. Plus the text-box save defect the surfaces lane found.

**Built.**

- One lowering per gesture in `casual-doc-command`, called by the editor's wasm method and
  by `casual-doc-ai`; caret, selection and page resolution stay in the wasm method:
  - `links`: `target` (`#name` is a bookmark, anything else a URL as written),
    `set_hyperlink`/`link_range`, `remove_hyperlink`, `create_bookmark`, `rename_bookmark`,
    `delete_bookmark`, `bookmark_named`, and `agent_target` (below). The editor's
    `setHyperlink`, `removeHyperlink`, the three bookmark methods and rich paste's links call
    them.
  - `fields`: `common_field` (the editor's `parse_common_field` table) and `insert_field`.
  - `notes`: `insert_note` (an agent may give the body's text in the same edit).
  - `breaks`: `insert_break` (line, page, column; the site refusal for page and column),
    `insert_blank_page`, `section_break` (site, inheritance, form protection, ids).
  - `sections`: `set_geometry`, `set_line_numbering`, `set_section_layout`, `text_watermark`,
    `set_watermark` (every section, as Word writes it), `set_title_page`, `set_even_and_odd`,
    `create_running_content`, `has_running_content`.
  - `objects`: `insert_picture` (registration, drawing and Word's aspect lock in one step;
    returns the part name the caller writes the bytes to), `set_extent`, `wrapped`/`set_wrap`
    and the float anchor walk (`anchor_of`, which the editor now calls for every object-anchor
    read), `crop`/`set_crop`, `set_alt_text`, `delete_object`, `set_shape_fill`,
    `stroke_base`/`set_shape_outline`, `image_part_type`.
  - `charts`: `insert_chart`, with `chart_group_for_kind`, `default_chart_projection` and
    the rest of the sample-projection code moved out of `lib.rs` whole; `chart.rs` reads them
    from here.
- Twenty-seven registry commands, all `Trackable::No` with a published reason, each with an
  example that runs for real: `set_hyperlink`, `remove_hyperlink`, `create_bookmark`,
  `rename_bookmark`, `delete_bookmark`, `insert_field`, `insert_image`, `set_alt_text`,
  `resize_object`, `set_wrap`, `crop_image`, `delete_object`, `set_shape_fill`,
  `set_shape_outline`, `insert_footnote`, `insert_endnote`, `insert_break`,
  `insert_section_break`, `set_page_setup`, `set_line_numbering`, `set_page_numbering`,
  `set_watermark`, `insert_header`, `insert_footer`, `set_different_first_page`,
  `set_different_odd_even`, `insert_chart`. Seven new families: `links`, `fields`,
  `objects`, `notes`, `sections`, `headers_footers`, `charts`. Vocabulary **1.2**, 56
  commands.
- Coverage: 28 operations moved to reached; **`NOT_EXPOSED_CEILING` 49 → 21**. G3:
  27 contract ids now have a twin; **`UI_NOT_EXPOSED_CEILING` 119 → 91**. The objects
  commands also name the `object.` family's members (`object.altText`, `object.crop`,
  `object.delete`, `object.fill`, `object.outline`, `object.wrap`); the family row stays
  listed, with that reason, because its arrangement members are not exposed.
- **How an agent supplies a picture:** base64 in the edit (`data`) with its MIME type,
  decoded strictly (`objects::decode_base64`, RFC 4648 alphabet, padding checked) and refused
  past 8 MiB decoded before decoding; the bytes must carry the signature of the type they
  claim; the size is given in points or read from the header (PNG, GIF, JPEG, BMP at 96 DPI;
  one side given keeps the proportions). There is no resource-reference scheme for MCP
  clients to point at yet, and base64 is what every client can send. The bytes ride on the
  plan's state (`State::media`) and join the session's resources only when the plan commits
  (`Session::adopt`), so a preview or a refused plan writes none; the picture's bytes are not
  counted against the 1 MiB plan text ceiling.
- **Agent-only bounds** (the editor trusts the person, an agent's input is untrusted):
  link targets are `http`, `https`, `mailto`, `ftp`, `tel` or `#bookmark`, never
  `javascript:`, `file:` or `data:` (R6); bookmark names follow Word's rule (a letter, then
  letters, digits or `_`, at most 40) and must be unique; a header or footer is refused when
  the section already shows one, its own or linked; page sides and margins are bounded and
  must leave room for text; an orientation change swaps the sides unless both are given, as
  Word's does; `set_page_numbering` keeps the section's vertical alignment (the dialog's
  lowering writes both).
- **The text-box save defect is fixed in the importer** (`casual-doc-import/src/body.rs`): a
  lone rectangular `wps:wsp` text box is pushed as its own segment when the shape closes, and
  `commit_drawing` then found no picture and reported the `w:drawing` omitted, which the DOCX
  loss gate turned into a refused save. A real false positive, not an artefact of minimal
  XML: three of the eight committed text-box fixtures (`floating-text-box`, `vml-shapes`,
  `wrap-text-sides`) refused to save through `casual-doc-ai`. No Word-authored text-box
  fixture exists under `fixtures/` (every one is generated, with no `docProps/app.xml`), so
  the guard uses the shape Word writes (`mc:AlternateContent` with the VML fallback). The
  false finding had been masking a real silent loss, which HF-243's attribute gate caught at
  once: a lone text box's own `a:xfrm` off/ext is not modelled; it is now reported when it
  is not the identity, as a lone picture's is. `picture-hyperlink` still reports one
  `drawing`, which is real (an `a:hlinkClick` to a missing relationship).
- `suggest_roundtrip.rs` gained a guard over every `Trackable::No` command: its example runs
  in direct mode against the G5 fixture (which now also holds a picture, a hyperlink and a
  bookmark, built through the registry's own commands), changes the document, saves to DOCX
  without loss and reopens. Five examples cannot change that fixture and are listed with the
  refusal they must still produce: `clear_formatting` (nothing to clear), `table_split_cell`
  (no merge), `continue_numbering` (no earlier list), `set_wrap` (the picture is inline) and
  `set_shape_fill` (no shape; agents cannot insert one).

**Guards, each driven red by a mutation and restored:**

| Guard | Mutation | Red |
| --- | --- | --- |
| Text box drawing reports nothing | the quiet arm in `commit_drawing` disabled | `a text box (floating false) that lost nothing reported: {"drawing"}` |
| Text box translation is named | the `off` report disabled | `a translated text box (floating false) was dropped in silence: {}`; the attribute gate names `off/@x`, `off/@y` on `floating-text-box.docx`, `vml-shapes.docx` |
| Every untracked example applies, saves, reopens | `sections::set_watermark` emits nothing | `set_watermark: refused in direct mode: … "no-change"` |
| An agent's picture reaches the package | `Session::adopt` drops the bytes | `the DOCX save loses nothing: … "docx.export.media.missing_bytes"` |
| Agent links refuse scripts | `agent_target` admits every scheme | the `javascript:` link applied (`"marks": ["underline", "link"]`) |
| A header is added once | `has_running_content` looks at no section | the second `insert_header` applied (`"refusal": Null`) |
| G7 links | the editor's `setHyperlink` drops the tooltip | `link https://example.com/: editor and command differ` (`tooltip: None`) |
| G7 notes | the editor's footnote reference one byte later | `Footnote: editor and command differ` (`offset: 2`) |
| G7 sections | the editor inverts the watermark's semi-transparency | `watermark: editor and command differ` |
| G7 objects | the editor swaps the picture's width and height | `picture: editor and command differ` (`width_emu: 457200, height_emu: 914400`) |
| G7 charts | the editor inserts a doughnut when asked for a pie | `pie chart: editor and command differ` |
| G7 source half | `setFirstPageVariant` re-inlined | `lib.rs:4690: vec![Operation::SetSectionTitlePage {` |

The source scan now also refuses a constructed `SetHyperlink`, `RenameBookmark`,
`DeleteBookmark`, `InsertNote`, `SpliceSectionBoundary`, section-property, header/footer,
crop, alt-text, fill or stroke operation, an inline `Break`, `insert_break_op(`,
`section_break_ops(`, and a re-defined `default_chart_projection`/`chart_group_for_kind`.
`CreateBookmark`, `InsertField`, `SetExtent`, `SetMediaReference` and `DeleteObject` are not
in it: cross-references, captions, group resizing, picture bullets, Change Picture and
group-member deletion build them for gestures that have not moved.

**Behaviour changes.** None for editor users: G7 holds every moved method's operations equal
to the command's. Rich paste's links now go through `links::link_range` (same operations).

**Left `NotExposed`, and why** (21 operations, 91 editor ids):

- **Table of contents, captions, cross-references, update fields and caption numbers**
  (`InsertFieldRange`, `RemoveFieldRange`, `RemoveField`, `InsertBlocks`, `DeleteBlocks`,
  `SetInlines`): the contents lowering reads the paginated layout for its page labels and the
  heading cascade inside `toc.rs`; captions and cross-references carry the caption-sequence
  and reference-bookmark logic in `references.rs`. Each can move (the page labels become an
  argument the editor and `casual-doc-ai`'s layout both supply), but not as a thin cut; it is
  the next step of this family.
- **Chart data editing** (`setChartData`, `chart.rs`'s patch model): agents insert charts
  with sample data and cannot yet change the data.
- **Equations and content controls:** the editor has no insertion gesture for either, so
  there is nothing to move; an agent command would be new engine work, not a shared lowering.
- **Shapes and text boxes inserted, and arrangement** (position, z-order, grouping,
  rotation, flips, inline/floating, Add Text; `SetGroupGeometry`, `InsertObjectNode`,
  `RemoveInlineObject`, `SetTextBoxBody`): not offered yet. A text box's *text* is reachable:
  its paragraphs are a surface (lane surfaces) and the text commands edit them.
- **Removing a note or a header/footer body**, mirrored margins, header/footer distances,
  and the document-wide and protection settings: unchanged reasons in `coverage.rs` and
  `UI_NOT_EXPOSED`.
- **Picture watermarks** (the `media` form of `setWatermark`): an agent stamps text only.

### Lane preview

The private preview mode of §4.4 (owner decision Q4), as `editor.ai` on the host contract.

**Designed from Docs and Word first.** Gemini in Google Docs shows a proposal in a card of its
own beside the page, untouched until the person picks Insert/Replace or throws it away; Word's
Copilot rewrite does the same with "Keep it"/"Replace" and a discard; Tiptap's AI Toolkit calls
it "review changes". One proposal is on screen at a time. So:

- `editor.ai.preview(plan)` compiles the plan and opens a **non-modal card** at the right of
  the editor (a bottom sheet under 600px): title, "Only you can see this until you accept it.",
  who Accept will author it as, one redline per affected paragraph (struck deletions,
  underlined insertions, "New paragraph" labels, on the paper colours in both themes), every
  refused edit with its reason, then **Discard** and **Accept**. Never light-dismissed: a stray
  click must not throw a proposal away. A new preview replaces the open one (reported as
  `discarded`, reason `replaced`).
- **Nothing is applied before Accept**: the document, the undo stack and a shared room see
  nothing (the compile runs on a copy-on-write clone; the operations wait in the engine under an
  id; JavaScript holds only the id).
- **Accept** commits exactly the previewed operations through `apply_group`, one transaction,
  one undo step. **Discard** drops them.
- **Deliberately unlike Word:** Word applies a Copilot rewrite directly unless Track Changes is
  on. Here the default is a tracked suggestion whatever the review mode (`178` §5.2): the
  assistant acts *for* the person and a reviewer must tell its words from theirs.

**One compiler, one mutation path.** `casual_doc_ai::editor::compile` is `plan::preview`'s run
with the operations kept (collected from the clone's log in commit order, so a later edit can
anchor in an earlier edit's paragraph). `Session::for_editor` builds the session over the
editor's document, its id and revision-number allocators and its document revision, so a plan
mints the ids the editor would mint next and the reads report the editor's revision.
`crates/casual-doc-wasm/src/ai.rs` (`aiRead`, `aiPreview`, `aiAccept`, `aiDiscard`) only routes:
it lowers nothing and builds no transaction; at preview it also asks the choke point's own
question (`refuse_if_protected`: the replica's grant, the document's protection) so Accept is not
offered when it would fail, and on Accept it moves the editor's allocators past the plan's before
calling `apply_group`. The redline now carries pending **deletions** too (`text::redline_runs`);
it carried only insertions, so a replacement read as a pure insertion in every preview.

**The API** (`webapp/src/ai_assistant.mjs`, `host_contract.mjs` `AI_METHODS`): `ai.read({tool:
outline | read | search | trackedChanges | comments})`, `ai.preview(plan)`, `ai.apply(plan)`
(compile and commit with no card: `opendoc.ai`'s apply), `ai.accept()`, `ai.discard()`,
`ai.access()`. In process as `window.opendoc.ai.*`; across a frame as one new verb, `ai`, with
`{method, args}` (`createHostClient(...).ai.preview(plan)`), so the assistant's surface grows
without new verbs. Every answer is a value, a refusal is also announced as a `refusal` event,
and the decision arrives as the new `ai-decision` event. The SDK's `HostRequest::Ai` and
`HostEvent::AiDecision` keep `host_parity.rs` exact.

**The role matrix as implemented** (from the grant, never a role name; `aiAccessFor`):

| Role (grant) | Access | Engine profile the plan compiles under | Direct edits |
| --- | --- | --- | --- |
| `preview` (nothing) | `none` | nothing; reads refused too | — |
| `readonly` | `read` | `read`: every edit refused `capability-withheld`, shown with its reason | never |
| `commentor` | `comment` | `comment`: comments only | never |
| `edit` | `suggest` | `suggest` | `edit` only with the host's `?ai=direct` |
| `owner` (every capability) | `review` | `review` for a plan that decides revisions, else `suggest` | `review` with `?ai=direct` |

The replica's own grant (a room's word, a host ceiling) and the document's protection still
decide at the choke point, and are asked at preview time too.

**Author label (`178` Q3, implemented pending the owner):** `"<host identity> (AI)"`, for
example "Ada Lovelace (AI)", the form `178` §9 gives and the MCP tests already use; "AI
assistant" when the host named nobody. A plan cannot name its author. **In a shared room the
relay's attested name wins at commit** (`docs/175` §5.2, `authorship::conform`), so the label
does not survive there until `178` §10's agent principal exists.

**Guards, each driven red:**

| Guard | Mutation | Red |
| --- | --- | --- |
| Preview's redline marks deletions (`editor_compile.rs`) | redline back to `text::runs` | the afterRuns carried only `inserted` |
| The compile hands back an allocator past every plan id | return the pre-run allocator | "the editor moves its allocator to 5001 and the plan minted 8194" |
| Accept commits exactly the previewed operations (`ai.rs`) | Accept drops the last operation | `left/right` differ: the commit is not the preview's operations |
| … and every one of them | compile keeps only the first commit | the new paragraph is missing |
| `readonly` is refused by the engine | `read` mapped to `Profile::Suggest` | `wouldApply` `true` |
| The replica's own grant is asked at preview | drop the `refuse_if_protected` call | a viewer replica offered an Accept (`wouldApply: true`) |
| No id collides after Accept | drop the allocator move | "the editor would mint counter 4096, which the plan already put in the document" |
| Discard leaves no trace | Discard keeps the plan | the pending plan survived |
| A stale preview is refused | drop the revision check | Accept applied over the person's later typing |
| Direct needs the host's grant | `suggest` always `Profile::Edit` | a direct plan was offered without the grant |
| `ai.rs` mutates only through `apply_group` | a `Transaction::reserve(` in `ai.rs` | "ai.rs contains `Transaction::reserve(`" |

**Not built, and why.** WebMCP (`178` M4) is out of scope. The `<opendoc-editor>` element has
no `ai-direct` attribute: a host grants direct edits with `?ai=direct` on the editor URL (the one
configuration channel, `hostConfig`). Rewriting a proposal ("Regenerate", "Make it shorter") is
the host assistant's job, not the editor's: the card offers Accept and Discard only. Per-edit
accept within one proposal (Docs' per-suggestion review) is not offered; accepting lands every
edit as its own tracked revision, which the review pane can then accept or reject one by one.

### Lane references

The last `NotExposed` editor lowerings: table of contents (insert, update page numbers,
update entire), captions (insert, update caption numbers) and cross-references.

**Built.**

- `casual_doc_command::references`, pure and layout-free. What a gesture needs from layout
  arrives as `LayoutFacts`: `heading_level(paragraph)` (the style cascade),
  `page_label(node)` (the label the page prints) and `whole_pagination()` (a windowed layout
  refuses instead of inventing numbers). Moved whole: the contents headings walk, `TOC n`
  style creation, entry blocks, both updates and `toc_field`; the one-walk reference scan
  (captions, headings, chapter ordinals, `_Ref` bookmark extents), caption insertion and
  renumbering, cross-reference insertion with *Include above/below*, bookmark and note
  targets. The editor's `toc.rs` and `references.rs` are glue: parse arguments, lend
  `EditorFacts` (cascade plus `first_page_labels` over the resident layout; for a page
  reference, the page the target is painted on) and apply.
- `casual-doc-ai` lends `AgentFacts` (`layout_facts.rs`): the same cascade, and page labels
  from an `opendoc-viewer` pagination of the plan's current state, so a contents inserted
  after other edits in one plan lists the pages those edits produce. Only the contents
  commands and a page reference pay for that pagination.
- One page map for both: `casual_doc_layout::paginate::first_page_labels`
  (`ViewerDocument::page_labels` calls it). Moved down so the command crate needs no layout:
  `node_plain_text` and its two siblings to `casual_doc_model::v1::plain_text` (layout
  re-exports), the paragraph visitors to `casual_doc_model::v1` and
  `visit_paragraphs_all_surfaces` to `casual-doc-edit` (review re-exports).
- Five commands in a new `references` family, all `Trackable::No` with a published reason:
  `insert_table_of_contents`, `update_table_of_contents`, `insert_caption`,
  `update_caption_numbers`, `insert_cross_reference`. Vocabulary **1.3**, 61 commands.
- Coverage: `InsertFieldRange`, `InsertBlocks`, `DeleteBlocks`, `SetInlines` and
  `SetStyleDefinition` reached; **`NOT_EXPOSED_CEILING` 21 → 16**. G3: the seven
  `reference.*` ids have twins; **`UI_NOT_EXPOSED_CEILING` 91 → 84**.
- G5 fixture: two headings, a table of contents made stale by renaming one, and a caption
  whose `SEQ` shows a stale `7` (as another producer's file arrives), so every new example
  changes the fixture in direct mode, saves and reopens.

**Defect found and fixed (editor behaviour change).** The contents page map read top-level
fragments only, so a heading inside a table cell had no page, its entry carried an empty
run, the model refused it and the whole table failed to insert (`InvalidField`) for any
document with a heading in a table. `first_page_labels` now descends table rows, and an
entry with no page (a heading in a text box) shows none instead of refusing.

**Guards, each driven red by a mutation and restored:**

| Guard | Mutation | Red |
| --- | --- | --- |
| Heading in a table cell | the code before the fix | `"InvalidField"` |
| Agent contents prints layout pages | `AgentFacts::paginated` lends no pages | `left: ["Chapter A\t", "Chapter B\t"] right: [... "\t1", ... "\t2"]` |
| Page cross-reference prints the label | `first_page_labels` uses `page.number` | `left: "1" right: "v"` |
| Chapter caption counts headings | `heading_level` returns `None` | `left: "Figure 1-1: Wiring" right: "Figure 2-1: Wiring"` |
| Windowed layout refuses | `whole_pagination()` ignored | `refused: Placed { operations: [...] }` |
| G7 contents | the editor's tab position off by one twip | `position_twips: 9361` vs `9360` |
| G7 captions | the editor's chapter separator forced to a hyphen | `caption Table Below: editor and command differ` (`"-"` vs `"."`) |
| G7 cross-references | the editor drops `hyperlink` | `cross-reference labelAndNumber: editor and command differ` |
| G7 source half | `InsertFieldRange` re-constructed in `toc.rs` | `toc.rs:228: if let Some(casual_doc_edit::Operation::InsertFieldRange {` |
| Declared emits enforced | drop `SetStyleDefinition` from the contents' `emits` | "`insert_table_of_contents` lowered to `SetStyleDefinition`, which the capability registry does not declare" |
| Direct-mode examples | the renumber reads the cached number | `update_caption_numbers: refused in direct mode … every caption already shows` |
| G1 | `SetInlines` marked not exposed | "`insert_caption` declares `SetInlines`, which coverage does not mark as reached"; ratchet `17 / 16` |
| G3 ratchet | `reference.caption` re-added to `UI_NOT_EXPOSED` | `left: 85 right: 84` |
| Vocabulary version | `VOCABULARY_COMMANDS` 60 | "the registry has 61 commands but vocabulary 1.3 names 60" |

The source scan now also refuses a constructed `CreateBookmark`, `InsertField` or
`InsertFieldRange`, and `caption_paragraph(`, `caption_style(`, `reference_field(`,
`above_below(` and re-defined contents or caption walks in the editor.

**Behaviour, otherwise unchanged for editor users.** G7 holds each moved gesture's
operations equal to the command's. Argument-validation order differs only when several
arguments are invalid at once (the label check now follows the enum parses).

**Left, and why.**

- `RemoveField`, `RemoveFieldRange`: no editor gesture unlinks a field yet, so there is
  nothing to share.
- Paragraph-number cross-references (`\r`, `\n`, `\w`) are not in the agent vocabulary: the
  shared lowering caches the paragraph's *text* as their result, which a reader sees until
  the field updates. A defect in the editor too; fixing it needs list-number resolution.
- ~~The editor's page cross-reference caches the **physical** page number, not the printed
  label.~~ **Fixed at integration:** `page_label_of` maps the painted page through
  `page_number_labels`, the table the contents reads, so an editor's `PAGEREF` and an
  agent's print the same label (`v` on the first page of a section numbered in roman from 5).
  Guard: `an_editor_page_cross_reference_prints_the_page_label`; with the physical number
  restored it reads `left: "1"`.
- ~~A one-entry contents reopens from DOCX as an inline `TOC` field that the update command
  cannot find.~~ **Fixed at integration**, in the command rather than the importer:
  `docs/128` §2c deliberately keeps a field whose markers share a paragraph inline (the
  stronger invariant), and two guards pin it. `update_table_of_contents` now also finds a
  contents held as an inline field and rebuilds it as a range, as Word does when an updated
  `TOC` result no longer fits in one paragraph. Guard:
  `a_one_entry_table_of_contents_still_updates_after_a_docx_round_trip`; without the inline
  path the reopened contents is refused. An importer-side promotion was tried first and
  reverted, because it broke `docs/128` §2c's two guards.

### Lane webmcp

`178` §9 item 2 (M4): a browser agent discovers the editor's tools through WebMCP. Owner
decision 2026-10-29: build it now, opt-in behind a flag, off by default, tracking the draft.

**The API, verified at the primary sources on 2026-10-29.** The Web Machine Learning CG's
Draft Community Group Report of 27 October 2026 (<https://webmachinelearning.github.io/webmcp/>,
not a W3C standard): `Document.modelContext` (secure contexts) with
`registerTool(tool, {exposedTo?, signal?})`, `getTools()` and `executeTool()`; a tool is
`{name, title?, description, inputSchema?, execute, annotations?}`; `execute(input, {signal})`
resolves to any value, which the browser serialises to JSON; the annotations are
`readOnlyHint`, `untrustedContentHint`, `consequentialHint` and `debugging`, all `false` by
default and advisory; a tool name is held to 128 characters. There is **no**
`unregisterTool`: the `signal` "unregisters the tool when aborted". Chrome's imperative API
page (<https://developer.chrome.com/docs/ai/webmcp/imperative-api>, updated 2026-10-16) shows
the same `document.modelContext.registerTool`, removal "with `AbortSignal`", and from Chrome 153
an abort that no longer breaks an execution in flight. `178` §11.3's citation said 20 October;
the draft has moved on once since, which is why this is opt-in and tracks it.

**Built** (`webapp/src/ai_webmcp.mjs`, started by `createAssistant`, so `main.js` is untouched).

- **Opt-in:** `ai-tools="webmcp"` on `<opendoc-editor>` (written into the frame's URL as
  `?aiTools=webmcp`; any other value is dropped and reported `unknown` on
  `opendoc-capabilities`), or `?aiTools=webmcp` on `editor.html`, read by `hostConfig()`, the one
  configuration channel. **Off by default.** Feature-detected: with no `document.modelContext`
  nothing runs and no polyfill is shipped. The host reads the state on `ai.access().webmcp`:
  `{state: off | unavailable | pending | registered | failed | unregistered, reason, tools}`.
- **Tools wrap `editor.ai` and nothing else.** Reads call `ai.read`; every edit tool calls
  `ai.preview`, which compiles in `casual-doc-ai`'s one compiler and opens the private card.
  No tool calls `apply` or `accept`, so an agent can only propose and the person decides with
  Accept or Discard. The tool result is `{status: "awaiting-person", previewId, wouldApply,
  preview}`.
- **The edit schema is selected, never written:** `generated/edit-schema.json`'s own branches,
  filtered by `generated/commands.json`'s trackability and `plan.rs`'s `needs` (comments need
  `comment`, `review` needs `review`). Both files are fetched only when a host opted in.
- **Gating, from `ai.access()` (the grant, never a role name):**

| Access (role) | Tools | Edit types in `propose_edits` |
| --- | --- | --- |
| `none` (`preview`) | none | — |
| `read` (`readonly`) | `get_outline`, `read_blocks`, `search`, `list_comments`, `list_tracked_changes` | — |
| `comment` (`commentor`) | + `add_comment`, `propose_edits` | `comment`, `resolve_comment` |
| `suggest` (`edit`) | same | + every trackable type; with `?ai=direct`, every type and `mode: direct` |
| `review` (`owner`) | + `review_tracked_changes` (the `review` type only) | as `suggest` |

- **Hints:** the five reads carry `readOnlyHint` and `untrustedContentHint` (document text is
  data, `178` §5.8); the three edit tools carry `untrustedContentHint` because the preview echoes
  document text; `review_tracked_changes`, and `propose_edits` under a direct grant, carry
  `consequentialHint`. Descriptions are fixed English in one `MODEL_TEXT` table: model-facing
  API text, like the tool names and `tools/opendoc-mcp`'s descriptions, so not routed through
  `t()`, and no document text ever reaches one.
- **Lifetime:** one `AbortController` per registration. `sync()` re-derives the list from the
  grant and, when it moved, aborts the old signal and registers what is left; every call
  re-asks the grant first, so a narrowed grant is never outrun by a stale registration. A
  failed `registerTool` aborts the half list. `pagehide`, which an `<opendoc-editor>` unmount
  (its frame blanked) and a closed tab both fire, unregisters everything.
- **Inside an embed** the tools are registered in the frame's document; the host page holds
  none and the host contract does not forward them.
- Documented in `packages/opendoc-embed/README.md` (attribute row and a WebMCP paragraph), the
  element's own attribute list, the generated typings (`WebMcpStatus`), and the regenerated
  embedding and playground pages (their attribute count is 11).

**Guards, each driven red** (`tests/ai_webmcp.test.mjs`, `tests/e2e/webmcp.spec.mjs` against a
fake `document.modelContext` shaped as the draft specifies):

| Guard | Mutation | Red |
| --- | --- | --- |
| Each role registers exactly its row | reads-only cut-off moved to `read` | the matrix differs: `readonly` gained `add_comment`, `propose_edits` |
| Reads are read-only and untrusted | `untrustedContentHint` dropped from reads | `get_outline` annotations differ |
| A direct grant is consequential | `consequentialHint: false` always | `false !== true` |
| Direct-only types need the grant | trackability ignored under `suggest` | `propose_edits` offered `Set(60)`, not the trackable set |
| Narrowing aborts the old registration | the old controller not aborted | the tool list differs after the grant narrowed |
| A call re-asks the grant | the `admits` check removed | the stale call answered `ok: true` |
| Unmount unregisters | `dispose` does not abort | `7 !== 0` tools left |
| A failed registration leaves nothing | the half list not aborted | `2 !== 0` tools left |
| Edit tools only propose | `propose_edits` calls `apply` | "a WebMCP tool must never call apply" |
| No WebMCP, nothing happens, and the host is told | no feature detection | state `failed`, not `unavailable` |
| An input cannot choose another read | `{tool, ...args}` order | the read went to `comments` |
| The element writes the opt-in | the `set` removed | `null !== 'webmcp'` |
| An unknown value turns it off | the `delete` removed | `'webmcp' !== null` |
| `hostConfig` reads it | `aiTools: null` | `null !== 'webmcp'` |
| e2e: opt-in registers | `createAssistant` never starts WebMCP | `Cannot read properties of null (reading 'state')` |
| e2e: default registers none | the opt-in check removed | 10 tools registered on the default page |
| e2e: readonly gets reads only | reads-only cut-off moved to `read` | one extra tool |
| e2e: the edit tool shows the card, the document waits for Accept | `propose_edits` calls `apply` | `Expected: true, Received: false` |
| e2e: unmount unregisters, in the frame | the `pagehide` listener removed | no `-name` entries after the element was removed |

**Not built, and why.**

- **No `mount({ ai: { webmcp: true } })` option:** there is no `mount()` API (`main.js` has no
  mount seam, HF-109); the element attribute and the URL are the two opt-in surfaces.
- **No forward to the host page** (out of scope by the brief): a host that wants an agent on
  its own page to reach a framed editor would need the host contract to re-register the tools
  there.
- **No runtime role change triggers a re-sync today:** the grant is resolved once per page
  (`HOST_CAPS`), and a remount is a new frame. `sync()` and the per-call check exist for the
  day a grant can narrow live.
- An edit tool does not wait for the person's decision; it returns at once with
  `awaiting-person`, and the agent reads the outcome back (`list_tracked_changes`). The
  `ai-decision` event tells the host.
- `pagehide` disposes for good, so a page restored from the back/forward cache offers no tools
  until reloaded.
- `exposedTo`, `title` and the `debugging` annotation are not used; `getTools`/`executeTool`
  are the agent's side and are not called.
- Tested against a fake: the browser in CI has no `document.modelContext` (Chrome ships it
  behind an origin trial), so no run here talks to a real browser agent.
- `178` §13's tool-name question (Q7) stands: `propose_edits` is a new name because it never
  applies, unlike MCP's `apply_edits`.
