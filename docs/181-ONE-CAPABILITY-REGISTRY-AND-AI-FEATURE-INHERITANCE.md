# 181 — One capability registry: every feature inherits its AI and MCP exposure

**Status:** Design, proposed 2026-10-28. R0 and the tables step of R1 are built; what is and is not is in §12.
**Decides:** ADR-075 (proposed). **Row:** `docs/109` RM-48.
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

## 11. What the design change was

Documentation only: this document, ADR-075 (proposed) and `docs/109` RM-48.

## 12. Built, 2026-10-28 (R0 and R1 step 1)

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

**Not built yet, and where it goes.**

- The registry is a `const` table, not yet the `Command` trait of §2.1. The serde `Edit`
  enum and its lowerings stay in `casual-doc-ai/src/plan.rs`; a test holds them to the
  registry (every registry example parses as its own `op`). The trait and the macro that
  generates the enum arrive with the next family moved in.
- `UI_COMMANDS` and G3 (editor command ids ⇔ registry), G4 (generated schemas), G5 (suggest
  round-trip over every command) and G7 beyond tables: R3 and later R1 steps.
- The anchor resolver has not moved down; surfaces (R2) are not built.
