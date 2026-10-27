# 178 — MCP and the AI-native SDK: agents read by anchor and edit through the closed op set

**Status:** Design, proposed 2026-10-24. A1 (shared review lowering), M1 and M2 over stdio are
built; what is built and what is not is in §15.
**Decides:** ADR-072 (proposed). **Row:** `docs/109` RM-47.
**Refined by:** `181` and ADR-075 (proposed 2026-10-28): how every later feature reaches
this surface through one capability registry, with generated schemas (A4) and exhaustive
coverage guards.
**Builds on:** `45` and ADR-030 (the four seams, I1 to I4), `132` and ADR-035 (the document
assistance layer, which this refines for MCP), `176` and ADR-071 (the Native/headless SDK
this is built on), `05` (SDK API), `175` and ADR-070 (host-attested identity, JWT grants),
`169` and `152` (sharing, relay, participants), `107` and `150` (OT over the closed op set),
`149` and ADR-036 (roles, capabilities, one host contract), ADR-052 (enforcement at the
operation).

**Owner request, 2026-10-24:** "plan and design MCP for this product as well as with SDK,
for AI tools".

---

## 0. Where this sits next to `docs/132`

`132` designed a vendor-neutral **Document Assistance Layer** (recipes, scopes, proposals,
local semantic retrieval) and put MCP at its outer edge as an optional adapter, read-only
first, with no direct-edit tool. It predates two facts:

1. `176` (2026-10-23) made a **Native/headless SDK** a product, with modification through
   `casual_doc_edit::apply` and no UI. That is the natural host for an MCP server, and it
   exists to be called by programs, which is what an agent is.
2. The owner now asks for agents that **edit**: insert and replace text, apply styles,
   change tables, comment, and suggest.

This document is therefore the concrete design of three things `132` left abstract: the
`opendoc-mcp` server, the AI-facing SDK layer it wraps, and the edit vocabulary both share.
It keeps every `132` governing decision (§3 there): reasoning and mutation are separate, the
model never emits OOXML or a snapshot, deterministic work stays deterministic, derived data
lives in the I4 sidecar, the host owns policy, and no server is mandatory. It changes one:
`132` §10.3 had no edit tool in its first MCP profile. Here, edits exist from the second
phase, but **as tracked suggestions by default**, compiled to the closed op set, previewable,
and refused by the engine when the session's capability does not allow them. That is the
same safety goal reached through enforcement instead of absence.

Semantic (vector) retrieval, recipes and the in-editor assistant UI stay in `132`
(DAI-2, DAI-3). This document gives them the chunker and the anchors they need (§4.4).

## 1. The patterns, named before any design

| Problem | Known pattern | Where it is used here |
| --- | --- | --- |
| One capability, several front doors (MCP, Python, JS, the editor page) | **Adapter over one façade; one schema, N transports** (ADR-036's precedent) | §3. MCP, Python, JS and WebMCP are adapters over one Rust layer, and their schemas are generated from one set of Rust types |
| An agent must change a document safely | **Command pattern compiled to a closed instruction set** (I2) | §4.3. A small, versioned *edit-plan* vocabulary lowers to `casual_doc_edit::Operation`; it is not a second op set |
| Pointing at text robustly | **Stable identity plus a content selector**: the W3C Web Annotation `TextQuoteSelector` (`exact`, `prefix`, `suffix`) scoped to a `NodeId` | §4.2. Never a document-global offset, never a paragraph index |
| Edits against a document that may have changed | **Optimistic concurrency** (HTTP `If-Match`, an ETag) | Every read returns a `revision`; every edit carries `base_revision` |
| "Show me what you would do" | **Plan / apply** (Terraform's `plan`, a dry run) | §4.3. `preview_edits` runs the same compiler as `apply_edits` and commits nothing |
| What an agent may do | **Least-privilege capability grant** (OAuth scope, WOPI token; already `casual-doc-edit/src/access.rs`) | §5.1. The server's profile becomes an engine `Capabilities` value, and the engine refuses |
| Large documents through a small context window | **Cursor pagination and bounded projection** | §5.3. Every list is paged, every text result has a token ceiling, and truncation is stated |
| Files from strangers | **Sandboxing with an explicit allowlist** (chroot-like roots set by the operator) | §5.6 |
| State across stateless calls | **Server-minted handle** (a file descriptor; MCP 2026-07-28's own "stateful tools" advice) | §6.1. `open_document` returns a handle every later call names |

No pattern is invented here. The one choice with no precedent in this repository is the
anchor format, and it is borrowed from a W3C recommendation rather than designed.

## 2. What exists today, measured at `289c90f8`

| Fact | Evidence | Consequence |
| --- | --- | --- |
| The closed op set has **67** variants, each invertible | `crates/casual-doc-edit/src/lib.rs:454` | The edit plan compiles to it; nothing is added to it for agents |
| There is no comment or tracked-change *operation*; both are written by `UpdateReviewState` | `lib.rs:780` | Comments and suggestions are lowered into that variant, as the editor does |
| **The lowering of a suggestion lives in the editor's WASM crate**, not in a shared one | `crates/casual-doc-wasm/src/lib.rs:11596` (`suggestInsert`), `:11724` (`suggestStyledInsert`), in a 51,724-line file | Prerequisite A1 (§8): move it to a shared crate, or the headless SDK grows a second lowering that drifts |
| A participant's capabilities are enforced **at the operation**: `comment`, `suggest`, `review`, `edit`, `manage_protection`, `manage_access` | `crates/casual-doc-edit/src/access.rs:138`, `refuse_if_not_permitted` at `:662`; ADR-052 projection equality | A suggest-only agent is refused by the engine, not by the adapter's good behaviour |
| `ModelPos` is a `NodeId` plus a **UTF-8 byte offset** | `crates/casual-doc-layout/src/model.rs:14` | A program can pass back a position it was given. A language model cannot count bytes, so it gets quote selectors (§4.2) |
| `NodeId`s are minted by a deterministic namespace-and-counter generator | `crates/casual-doc-model/src/ids.rs` (`IdGenerator`) | The same bytes on the same engine version yield the same ids. A re-save by another program does not; the quote selector covers that |
| A content digest of the whole document exists | `crates/casual-doc-diff/src/identity.rs:131` (`content_digest`, O(document)) | It is the cross-session identity of a document state, computed at open and save, not per edit |
| Comparison is a resumable, budgeted job | `crates/casual-doc-diff/src/job.rs:217` (`step(sides, budget)`), `:195` (`cancel`) | `compare_documents` can report progress and be cancelled |
| Render, PNG and PDF exist without any editing crate | `crates/casual-doc-viewer` (`176` §3.1) | The render and convert tools need no new engine work |
| Writers exist for DOCX, ODT, HTML, Markdown, TXT, RTF and PDF | `crates/casual-doc-io/src/` | `export` is a registry call |
| `CompatibilityReport` and `PreservationLedger` exist and are **not exposed** by any SDK | `casual-doc-io/src/report.rs:66`, `casual-doc-import/src/report.rs:271` (`176` P7) | The loss report is the edge (§7), and it is the first prerequisite |
| The Native SDK's edit API is **4** operations today | `176` §3.3 (`casual-doc-sdk`) | Everything in §4.3 beyond insert, delete, split and join is new surface |

## 3. Architecture

```
 Claude Desktop / Code, IDEs,            Python program        Node / browser program      <opendoc-editor> page
 any MCP client                           (agent framework)     (agent framework)           (in-page assistant)
        |  stdio | Streamable HTTP              |                      |                            |
        v                                       v                      v                            v
   opendoc-mcp  (Rust binary, thin)     opendoc.ai (PyPI)      @casualoffice/opendoc-ai    editor.ai + WebMCP (opt-in)
        |                                       |  C ABI (176 §6)      |  WASM                       |  host contract (ADR-036)
        +---------------------------------------+----------------------+-----------------------------+
                                                v
                         opendoc::ai  — the AI-native layer (Rust, in the `opendoc` façade crate)
              outline · read · chunk · search · anchors · edit plan → Operation · preview · report · diff
                                                |
                         opendoc  — the Native SDK (176 §3.3): open, read view, edit(), save, render
                                                |
            casual-doc-edit::apply (I1) · access.rs capabilities · importers/writers · layout · render
```

Rules:

1. **One implementation.** `opendoc::ai` is Rust. The MCP server, the Python package and the
   JS package are adapters over it, and none of them re-implements an anchor, a lowering or a
   bound. A Python MCP server written over python bindings would be a second
   implementation; it is allowed as an example, never as the product.
2. **One schema.** Every request and result type derives a JSON Schema from its Rust type.
   The MCP `inputSchema`/`outputSchema`, the Python type stubs and the TypeScript `.d.ts` are
   generated from it, and a test fails if a generated file is stale (the `build.sh --check`
   pattern). A field cannot exist in one front door and be missing from another.
3. **The server is model-independent.** It never calls a model, holds no model credential,
   and does not use MCP sampling (§6.4). The agent reasons; OpenDoc resolves, validates,
   compiles and commits (`132` §3.2).
4. **The engine enforces; the adapter declares.** MCP tool annotations are hints (§6.1).
   The capability check is `refuse_if_not_permitted`, the same function a relay replica runs.

## 4. The AI-native SDK surface (`opendoc.ai`)

Small, JSON-in and JSON-out, and shaped for a language model: stable ids it can quote back,
text it can read, and refusals it can act on. Python and JavaScript first, because that is
where agent frameworks are; Java and .NET get it through the same C ABI later (`176` N1).

```python
import opendoc
from opendoc import ai

doc = opendoc.open("contract.docx")                     # 176 §3.3; refuses past the limits in §5.7
outline = ai.outline(doc, depth=3)                      # headings, sections, tables, comments; anchors + token estimates
page = ai.read(doc, start=outline.sections[2].anchor, max_tokens=3000, format="markdown")
for chunk in ai.chunks(doc, max_tokens=400):             # structure-aware, deterministic ids, for RAG
    index.add(chunk.id, chunk.text, meta=chunk.anchors)
hits = ai.search(doc, "termination for convenience", mode="words", limit=20)

plan = ai.EditPlan(base_revision=doc.revision, edits=[
    ai.ReplaceText(target=ai.Anchor(node=hits[0].node, exact="thirty (30) days"), text="sixty (60) days"),
    ai.Comment(target=ai.Anchor(node=hits[0].node, exact="termination"), text="Aligned with the MSA, clause 9."),
])
preview = ai.preview(doc, plan)                          # same compiler, commits nothing
result = ai.apply(doc, plan, mode="suggest")             # tracked changes by the session's author
doc.save("contract.reviewed.docx", report="refuse-on-loss")
```

### 4.1 Read: outline, blocks, tables, review content

- **`outline(doc, depth, cursor)`**: the heading tree, sections, tables (with dimensions and
  header row text), comment and tracked-change counts, and for every node its `NodeId`, its
  heading path, and an estimate of its size in tokens. This is the map an agent reads first.
- **`read(doc, start, max_tokens, format)`**: blocks in document order from an anchor, as
  `json` (typed blocks: paragraph with id, style, list level, text, and runs as
  `{text, marks}`; table; content control; `other` for anything preserved but not
  projected) or as **anchored Markdown** (each block prefixed with its id, `{#n:…}`, so a
  model can cite and target it). It stops at the token ceiling and returns a cursor.
- **`tables`, `table(id, rows)`**, **`comments(cursor)`**, **`revisions(cursor)`**,
  **`styles()`**, **`metadata()`**: the `176` §3.3 read view, projected. Comments and
  revisions reuse the shapes `docs/05` §"Phase 1G" already publishes for `listComments` and
  `listRevisions`, so the editor and the SDK say the same thing.

Reading covers the body first. Headers, footers, notes and text boxes are separate
surfaces with their own anchors (`docs/85`'s location model), and a result says which
surfaces it covered. `176` §3.1 records that `plainText` today covers the body only; the
same honesty applies here.

### 4.2 Anchors: `NodeId` plus a quote, never a global offset

```ts
type Anchor = {
  node: string;                 // a NodeId returned by outline/read/search. Required (I3)
  exact?: string;               // W3C TextQuoteSelector, resolved INSIDE `node` only
  prefix?: string; suffix?: string;
  occurrence?: number;          // 1-based, when `exact` repeats inside the node
  pos?: { start: number; end: number };  // ModelPos offsets, for programs only (see below)
  side?: "before" | "after";    // for insertion points
};
```

Resolution is deterministic and **refuses rather than guesses**:

- The node must exist in the current revision, or `anchor-not-found`.
- `exact` is matched inside that node only. Zero matches: `anchor-not-found`, with the
  node's current text (bounded) so the agent can retry. Several and no `occurrence` or
  prefix/suffix to disambiguate: `anchor-ambiguous`, with the candidates. **It never
  searches the document and edits the first coincidence** (`132` §5.1).
- `pos` is a `ModelPos` pair as `read` and `search` returned it. It is for programs that
  pass back what they were given. Offsets are never computed by the caller, and a model is
  never asked to produce one: they are UTF-8 bytes (§2), which a model cannot count.
- No paragraph index, no character index over the whole document. Those are what
  python-docx and the Google Docs API address by (§7), and they shift under every edit.

**Why the quote, and not only the id.** The id is stable within a session and reproducible
for the same bytes on the same engine version (§2). A file re-saved by another program gets
new ids. A quote lets an agent that cached an outline re-find its target, and it lets the
server confirm the agent is pointing at the text it thinks it is: the quote is checked even
when the id resolves.

### 4.3 Edit plans: a closed vocabulary compiled to the closed op set

An **edit plan** is an ordered list of typed edits against one `base_revision`. The
vocabulary is deliberately smaller than `Operation`'s 67 variants. It covers what an agent
is asked to do, and each entry lowers to operations through the same code the editor's
commands use (A1 in §8):

| Edit | Lowers to | Capability in `suggest` mode | in `direct` mode |
| --- | --- | --- | --- |
| `insert_text { at, text }` | `InsertText`, or a tracked insertion via `UpdateReviewState` | `suggest` | `edit` |
| `replace_text { target, text }` | `DeleteText` + `InsertText`, or a tracked replacement (delete + insert in one revision group) | `suggest` | `edit` |
| `delete_text { target }` | `DeleteText`, or a tracked deletion | `suggest` | `edit` |
| `insert_paragraphs { after, paragraphs: [{ style, text }] }` | `SplitParagraph`/`InsertBlocks`, tracked when suggesting | `suggest` | `edit` |
| `apply_paragraph_style { target, style }` | `SetParagraphProperties`; a tracked property change when suggesting | `suggest` | `edit` |
| `set_text_format { target, format }` (bold, italic, underline, size, colour, font, highlight) | `FormatText`; a tracked formatting revision when suggesting | `suggest` | `edit` |
| `insert_table { after, rows, header }` | `InsertTable`, cells filled by `InsertText` | `suggest` | `edit` |
| `table_insert_row` / `table_delete_row` / `table_insert_column` / `table_delete_column` | `InsertRow`, `DeleteRow`, `InsertColumn`, `DeleteColumn` | `edit` until tracked table structure exists (Q11) | `edit` |
| `set_cell_text { table, row, column, text }` | a `replace_text` inside the cell's paragraph | `suggest` | `edit` |
| `comment { target, text, reply_to? }` | `UpdateReviewState` (comments map) | `comment` | `comment` |
| `resolve_comment { id }` | `UpdateReviewState` | `comment` | `comment` |
| `review { accept \| reject, revisions }` | the editor's accept/reject lowering | `review` | `review` |

Rules:

1. **Mode is a property of the call, default `suggest`.** `direct` is accepted only when the
   session's profile grants `edit` (§5.1). A suggestion is authored by the session's
   identity, never by a field in the plan (ADR-070's rule: a participant never states who
   wrote an operation).
2. **All or nothing, one undo step.** A plan applies as one batch of operations; any refusal
   leaves the document unchanged and returns every refusal with its edit index. The batch's
   inverse is kept as one `change_id` (§5.4).
3. **The vocabulary is versioned and closed.** An unknown `op` is refused with
   `unknown-edit`, never ignored. A new entry is added in this table, with its lowering, its
   capability and its OT tier (`150`), or not at all.
4. **No raw escape hatch.** There is no `set_xml`, no `replace_document`, no
   `apply_operations(raw)`. `132` §15's non-goal stands.
5. **Bounded.** At most 500 edits and 1 MiB of inserted text per plan (proposed defaults,
   server-configurable downwards). A redline of a whole contract is several plans, which is
   also several reviewable groups in Word's Reviewing pane.
6. **Stale plans are refused, not rebased.** A plan whose `base_revision` is behind the
   session's is refused with `stale-revision` and the current revision. In a single-writer
   headless session the server serialises calls, so this only happens when the agent read
   before its own last edit; it re-reads. Rebasing across a concurrent writer is the live
   co-editing case (§10), where OT already does it.

`preview(doc, plan)` runs the anchor resolution, the capability check, the lowering and the
apply on a **copy-on-write clone**, and returns: the per-edit result, the refusals, a
redline of every affected block (before and after text, with insertions and deletions
marked), the operations' count, and whether any edit would change what the compatibility
report says on save. It commits nothing and costs O(affected blocks) plus the clone.

### 4.4 Chunks for retrieval, with anchors

`chunks(doc, max_tokens, overlap, surfaces)` implements `132` §8.2's structure-aware
projection: heading plus following paragraphs up to the ceiling, a list with its lead-in,
a table header plus a bounded run of rows, a comment thread with its anchor. Every chunk
carries:

- a **deterministic id**: a hash of the chunker version, its ordered anchors and its text
  hash, so an unchanged chunk keeps its id and its vector across re-indexing;
- the ordered `NodeId` anchors it covers and its heading path;
- its text, its token estimate, and the surfaces it covers.

OpenDoc does not embed, store vectors or call a model. The host's RAG pipeline does, and a
hit maps back to live text through the anchors. Vectors stay in the I4 sidecar, outside the
document.

### 4.5 Token estimates are labelled estimates

A token count depends on the model's own vocabulary, which the server does not know. Every
`max_tokens` bound and every estimate uses one declared heuristic (UTF-8 bytes ÷ 4, rounded
up, with a floor per block), named in the result as `estimator: "bytes/4"`. A host that
knows its model's token count can pass a tighter ceiling. The bound errs towards returning less.

## 5. Safety

### 5.1 Capability profiles, enforced by the engine

The server starts with exactly one profile, chosen by whoever launches it, never by the
agent:

| Profile | Engine `Capabilities` | Tools offered |
| --- | --- | --- |
| `read` (**default**) | `viewer()` | open, inspect, search, chunk, render, export to a non-overwriting path, compare, report |
| `comment` | `comment` | + `comment`, `resolve_comment` |
| `suggest` | `comment` + `suggest` | + edit plans in `suggest` mode, `undo_change` |
| `edit` | + `edit` | + `direct` mode, table structure edits |
| `review` | + `review` | + accept or reject tracked changes |

Tools the profile does not grant are **not listed** in `tools/list`, and if called anyway
they are refused with `capability-withheld` (ADR-036's code). Listing them disabled would
spend the model's context on tools it cannot use. The capability check that actually
protects the document is the engine's `refuse_if_not_permitted` on the lowered operations,
so a bug in the adapter cannot widen what the profile allows. A document's own protection
(`w:documentProtection`, ADR-052) still applies on top: a document restricted to tracked
changes refuses a `direct` plan even under `edit`.

### 5.2 Tracked changes by default, so a person decides

In `suggest` mode every textual and formatting edit becomes a Word revision (`w:ins`,
`w:del`, `w:rPrChange`, `w:pPrChange`) authored by the session's configured identity, for
example `Claude (AI)`. A reviewer sees them in Word, LibreOffice or OpenDoc and accepts or
rejects each one. All edits of one plan share a revision group, so "reject everything the
agent did in that step" is one action in OpenDoc. The author string is set at launch
(`--author`), or by the host's attested identity in the editor and in a room (§9, §10); the
agent cannot change it.

### 5.3 Bounded outputs

- Every list (`outline` levels, blocks, hits, comments, revisions, tables, diffs, chunks)
  uses MCP's opaque-cursor pagination (§6.1), with a server page ceiling.
- Every text-bearing result honours `max_tokens` (default 4,000, server ceiling 20,000,
  both configurable) and reports `truncated: true` with a cursor. **Truncation is never
  silent.**
- A page image is capped by DPI (default 110, ceiling 200) and by pixel count, and is
  returned as an MCP image content block or a resource link, never inlined in text.
- A search returns at most `limit` hits with a bounded context window around each.

### 5.4 Every edit is undoable

`apply` returns a `change_id` that names the inverse batch (`176` §3.3: `edit` returns
the inverse). `undo_change(change_id)` applies it **only if it is the latest change** in the
session; otherwise it refuses with `undo-not-latest` and names the later changes, because
undoing an older batch under newer ones needs the selective undo of ADR-069, which is a
co-editing mechanism. In `suggest` mode there is a second undo that needs no server: a
human rejecting the revisions in any editor.

### 5.5 No silent data loss

- `open` returns the import summary: what was degraded or preserved opaquely
  (`CompatibilityReport`, `PreservationLedger`; prerequisite `176` P7).
- `export` and `save` to DOCX default to `preserve_when_safe` and **refuse** when the report
  says the target would lose content, naming each loss. The caller may pass
  `accept_loss: [codes]` naming the exact losses it accepts; a blanket "ignore" does not
  exist. Exporting to a format that is lossy by nature (TXT, Markdown) states that in its
  result rather than refusing.
- `compatibility_report(doc, target)` answers the question before anything is written.

### 5.6 Files: an allowlist, and nothing outside it

- `--root DIR` (repeatable) names the directories the server may read. `--write-root DIR`
  names where it may write; **the default is none**, so out of the box results come back as
  MCP resources and blobs, and the server writes nothing to disk.
- Every path is canonicalised (symlinks resolved) and then prefix-checked against the roots.
  A path that resolves outside is refused with `path-outside-roots`; the message does not
  echo the resolved target.
- The allowlist is **server configuration only**. MCP's *Roots* feature is deprecated in
  2026-07-28 (migration: "pass directories or files via tool parameters, resource URIs, or
  server configuration"), so the server does not adopt it. A path in a tool argument is
  always checked against the operator's roots; a client can never widen them.
- `save` never overwrites the file it opened unless the launch flag `--allow-overwrite` is
  set **and** the call says `overwrite: true`. The default target is a new sibling name.
- Only the allowlisted extensions open (`.docx`, `.dotx`, `.odt`, `.rtf`, `.txt`, `.json`).
  `.docm` is refused, as it is in the editor today (`SKILL` §11).

### 5.7 Untrusted files: resource limits

Every document an agent opens is untrusted. The server reuses the engine's existing
ceilings (`package_limits` in `casual-doc-viewer`, the importer's limits) and adds:

- a maximum file size and decompressed package size per open;
- a maximum number of open documents per session and an idle timeout that closes them;
- a wall-clock budget per call, with the comparison and layout jobs run in their budgeted,
  cancellable form (`casual-doc-diff` `job.rs`); a call over budget is refused with
  `over-budget`, not left running;
- a ceiling on pages rendered per call;
- no network access from the engine: linked images, fonts and external references are never
  fetched, and fields are not updated from outside sources;
- for a hosted deployment (§6.3), one worker process per session with OS limits on memory
  and CPU, so one hostile file cannot take down other sessions.

### 5.8 Document text is data, never instruction

Document content, comments, metadata and file names are untrusted text that may try to
instruct the agent. The server cannot stop a model from reading it, but it keeps three lines:
content is returned in structured fields, never concatenated into tool descriptions or
prompts' fixed text; nothing in a document can change the profile, the roots or the author;
and the dangerous outcomes (overwriting files, direct edits, accepting changes, network
access) are either absent or need a launch-time grant that no document can supply.
`132` §14's injection test corpus is the gate.

### 5.9 Audit

Each call is logged with its tool name, the document's digest, the revision before and after,
the number of operations and the refusal codes. **Never** the document's text, the inserted
text, or the agent's arguments verbatim (`175` §4.5's logging rule).

## 6. The MCP server: `opendoc-mcp`

### 6.1 Protocol baseline

Target revision: **MCP 2026-07-28**, the current one (checked 2026-10-24; §11.1). It is a
stateless revision, and that shapes the server:

- **No protocol session.** The `initialize` handshake and `Mcp-Session-Id` are gone; each
  request carries its protocol version and client capabilities in `_meta`, and the server
  implements `server/discover`. Cross-call state is what the revision itself recommends: an
  explicit, server-minted **document handle** returned by `open_document` and passed as an
  ordinary argument to every later tool. A handle is opaque, unguessable, scoped to the
  server process (stdio) or to the authenticated principal (HTTP), and expires when idle.
- **Results** carry `resultType`; list and read results carry `ttlMs` and
  `cacheScope: "private"` (a document is never cacheable by a shared intermediary).
  `tools/list` is returned in a fixed order.
- **Features used:** tools (input and output schemas, `structuredContent`, annotations),
  resources and resource templates, prompts, cursor pagination, progress, cancellation,
  `subscriptions/listen` for resource updates, elicitation through the multi-round-trip
  pattern, and the `io.modelcontextprotocol/tasks` extension for long work.
- **Features not used, because 2026-07-28 deprecates them:** Sampling, Roots and Logging.
  Diagnostics go to `stderr` on stdio and to OpenTelemetry when hosted.
- **Older clients.** Clients that still speak 2025-11-25 are common at the time of writing.
  Whether the server also answers the 2025-11-25 handshake for them is Q12.

### 6.2 Tools

Grouped by family. Every tool has an `inputSchema` and an `outputSchema`, returns
`structuredContent` plus a short text summary, and carries the annotations shown
(`R` = `readOnlyHint: true`; `D` = `destructiveHint: true`; `I` = `idempotentHint: true`).

| Family | Tool | Profile | Hints | What it does |
| --- | --- | --- | --- | --- |
| Session | `open_document` | read | R, I | Open a file under the roots (or a resource the client supplies) and return a handle, revision, digest, page count and the import summary |
| | `close_document` | read | I | Release a handle |
| Inspect | `get_outline` | read | R, I | Heading tree, sections, tables, review counts, with anchors and token estimates |
| | `read_blocks` | read | R, I | Blocks from an anchor, as JSON or anchored Markdown, to a token ceiling |
| | `get_styles` | read | R, I | Paragraph and character styles in use, with their effective formatting |
| | `read_table` | read | R, I | One table's cells, row-paged, with merged-cell geometry |
| | `list_comments` | read | R, I | Threads with anchors, authors and state |
| | `list_tracked_changes` | read | R, I | Revisions with kind, author, date, group and anchor |
| | `search` | read | R, I | Exact, whole-word or regular-expression search with anchored hits and context |
| | `chunk_document` | read | R, I | §4.4's anchored chunks, paged |
| Edit | `preview_edits` | read | R, I | Dry-run an edit plan: redline, refusals, report impact; commits nothing |
| | `apply_edits` | suggest | — | Apply an edit plan (`suggest` by default; `direct` only under `edit`) and return a `change_id` |
| | `add_comment` | comment | — | A one-edit convenience for the most common review action |
| | `undo_change` | suggest | — | Undo the latest change by its id |
| | `review_tracked_changes` | review | D | Accept or reject named revisions or a whole revision group |
| Convert | `export_document` | read | — | DOCX, ODT, PDF, HTML, Markdown or TXT; to a resource, or to a path under a write root; the loss gate of §5.5 |
| | `render_page` | read | R, I | One page as PNG, DPI-capped |
| Compare | `compare_documents` | read | R, I | Structured differences between two open documents or two revisions, paged, with progress |
| Validate | `compatibility_report` | read | R, I | What a save to a given format would keep, degrade or lose |
| | `validate_document` | read | R, I | Model invariants, broken references (comments, notes, bookmarks, numbering), protection state |

Twenty tools at the widest profile and sixteen under `read` (`preview_edits` is offered there
too: it shows what a plan would do and which edits the profile would refuse). Agents choose worse
from long tool lists, so the profile trims the list (§5.1) and the descriptions are short
and literal. The tool names are proposals (Q7).

Not tools, on purpose: `get_text` (it is `export_document` to TXT or `read_blocks`),
`set_xml` and any raw operation (§4.3 rule 4), `semantic_search` (the host owns vectors,
§4.4), `run_macro` (never).

### 6.3 Resources and prompts

Resources (URIs are proposals):

| URI template | Content |
| --- | --- |
| `opendoc://documents` | The open handles |
| `opendoc://documents/{doc}/outline` | The outline, as JSON |
| `opendoc://documents/{doc}/blocks/{node}` | One block and its anchors |
| `opendoc://documents/{doc}/pages/{n}.png` | One rendered page (`image/png`) |
| `opendoc://documents/{doc}/report` | The import and save compatibility report |
| `opendoc://documents/{doc}/exports/{name}` | An export produced by `export_document` with no write root |
| `file:///…` | `.docx`/`.odt` files under the read roots, listed |

A client that opts in to resource subscriptions through `subscriptions/listen` is told
after each committed change to a document it reads. Resources are paged and revision-labelled, as `132` §10.2
requires.

Prompts are conveniences over the tools, never policy (`132` §10.4):

- **`summarise`** `{doc, length, audience}`: read the outline, then sections to a budget,
  and summarise with section anchors cited.
- **`review`** `{doc, checklist?}`: produce findings as comments, never edits.
- **`redline`** `{doc, instructions}`: propose changes as tracked suggestions, always calling
  `preview_edits` before `apply_edits`, one plan per section.
- **`compare_and_explain`** `{a, b}`: compare two versions and explain what changed, citing
  anchors.

### 6.4 Elicitation and sampling

- **Elicitation** (form mode) asks the human, through the client, before the few actions
  that a launch flag permits but a person should still confirm: an overwrite, an
  `accept_loss` export, a `direct` plan over a threshold of edits. In 2026-07-28 this is the
  multi-round-trip pattern: the tool returns `resultType: "input_required"` with the
  question, and the client retries the same call with the answer. The pending plan is bound
  to the retry by the server's own `requestState`, which carries the plan's digest and base
  revision, so an answer cannot be replayed against a different plan or revision (`132`
  §16 Q12). A client that does not support elicitation gets a refusal with the reason
  (`confirmation-unavailable`), not a silent yes.
- **Sampling is not used**, and 2026-07-28 deprecates it ("integrate directly with LLM
  provider APIs"). The server never asks the client's model to generate. The agent
  is already the model; a server that called back into it would hide reasoning inside a
  tool and make the server's behaviour depend on a model the operator did not choose.

### 6.5 Transports and authorisation

- **stdio first**, local, launched by the client with the operator's `--root` and profile flags
  on its command line. This is `132` §10.5's preferred first topology and covers Claude Desktop, Claude
  Code, IDE agents and agent frameworks.
- **Streamable HTTP later, host-operated.** One POST endpoint; `Origin` validated with a
  403 on mismatch; bound to 127.0.0.1 when local; the `MCP-Protocol-Version`, `Mcp-Method`
  and `Mcp-Name` headers checked. MCP authorisation: the server is an OAuth 2.1 protected
  resource that publishes RFC 9728 metadata, validates the token's audience (clients send
  the RFC 8707 `resource` parameter), and **never accepts or passes on any other token**
  to storage. A dropped stream loses the in-flight call (resumability was removed), so every
  mutating tool is safe to re-issue: `apply_edits` carries an idempotency key and a
  `base_revision`, and a retry of a committed plan returns the original `change_id`. In a host that already issues OpenDoc grants, the token's claims
  map onto §5.1's profile the same way a JWT grant maps onto room capabilities (`175`,
  ADR-070). Documents then come from the host's storage by id, not from paths.
- **Long work** (compare, export to PDF of a long document, chunking a large file) reports
  progress and honours cancellation. Where the client supports the tasks extension it
  returns a task handle (`tasks/get` to poll, `tasks/cancel` to stop); otherwise it stays
  synchronous within the budget of §5.7. Closing the HTTP stream cancels the call.

### 6.6 Distribution

One Rust binary, `opendoc-mcp`, released beside the `opendoc` CLI (`176` §5.2), with
launchers so it installs the way agent users expect: `pipx run opendoc-mcp` / `uvx` (a
wheel that carries the binary) and `npx @casualoffice/opendoc-mcp`. Configuration is flags
and an optional TOML file; no environment variable can widen a root or a profile.

## 7. Competitive context

Every row is from the cited public page (§11.2), read on 2026-10-24. *Not stated* means the
page does not say, which is unverified, not absent.

| | Engine | Addresses text by | Tracked changes | Comments | Layout, page images, PDF | Reports what a save lost | Licence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **OpenDoc MCP** (this design) | Own engine, the editor's | `NodeId` + quote, checked on every edit | Write, accept, reject; suggest by default; engine-enforced | Write, reply, resolve | **Yes**: the editor's layout, PNG, real-text PDF | **Yes**: `CompatibilityReport`, refuse on loss | Apache-2.0 |
| GongRzhe Office-Word-MCP-Server | python-docx + FastMCP | `paragraph_index`, `target_text`, `table_index`, offsets inside a paragraph | No | Read only | Not stated | Not stated | MIT; **archived** 2026-03-03 |
| SecurityRonin docx-mcp | Direct OOXML (unpack, edit XML, repack) | `para_id` plus matched text | **Yes**, tracked by default, `tracked=False` to write directly; accept and reject | Add, reply, list | Not stated | Not stated; validates against OOXML before save | MIT |
| UseJunior safe-docx | TypeScript `docx-core` | Not stated | Clean or tracked output | Not stated | "Not a visual editor"; no pagination guarantee | Not stated | Apache-2.0 |
| ONLYOFFICE DocSpace MCP | Their hosted DocSpace API | Files, folders, rooms (not document content, as far as read) | Not stated | Not stated | Not stated | Not stated | MIT server; the editor is AGPL |
| Microsoft Word MCP tools (Copilot Studio, preview) | Microsoft 365 | Whole document by sharing URL | No | Create, reply | Not stated | Not stated | Proprietary; Microsoft says "shouldn't be used in production" |
| Office.js Word API (add-ins inside Word) | Word itself | Ranges in a running Word | Yes (`Word.TrackedChange`, `changeTrackingMode`) | Yes | Word's | Word's | Proprietary; needs Word |
| Google Docs API | Google Docs | Segment-relative **UTF-16 index** (`startIndex`/`endIndex`), shifting with each earlier edit; `requiredRevisionId` for concurrency | Not stated in the cited pages | Not stated in the cited pages | Google's | Not applicable | Proprietary; needs Google's servers |

Two corrections to the easy story. python-docx is not the whole field: **docx-mcp already
edits as tracked changes by default and addresses by paragraph id plus text**, which is the
shape §4.2 and §5.2 choose. So tracked-by-default is table stakes, not our edge. And
Microsoft and Google have Office.js, Copilot and Gemini inside their own editors, which no
third party can embed or run offline.

The edge, stated as narrowly as the evidence allows:

1. **The same engine renders, converts and edits.** The PNG the agent looks at and the PDF
   it exports are the editor's own layout, offline. None of the open-source servers above
   claims a layout engine.
2. **Loss is reported before it happens.** No server above states that it reports what a
   save would drop; OpenDoc refuses a lossy save unless the loss is named (§5.5).
3. **Capabilities are enforced by the engine, not by the adapter**, and they are the same
   capabilities a person has in a room (`access.rs`, ADR-052). A suggest-only agent cannot
   be talked into a direct edit by a prompt, because the operation itself is refused.
4. **One path for a keystroke, a collaborator and an agent (I1)**, so an agent can join a
   live room (§10) with no second mutation path, and every change has an exact inverse.
5. **Licence and embedding.** Apache-2.0, no Microsoft 365 or Google account, no server.

None of this is shipped until the phases in §8 close. Until then it is a design advantage,
not a product claim, and the website must not say otherwise (`SKILL` §9).

## 8. Phased plan

Read-only first, then suggestions, then everything that needs a grant from a person.

| Phase | Delivers | Exit gate (proven in this repository) |
| --- | --- | --- |
| **A**: prerequisites | A1: move the suggestion and comment lowering (`suggestInsert`, `suggestStyledInsert`, reply, resolve, accept/reject) out of `casual-doc-wasm` into a shared crate both the editor and the Native SDK call. A2: `176` P7 (the report as a stable value) and the read view of `176` N1. A3: the anchor resolver and its refusals. A4: the schema crate and the generated-schema staleness guard | The editor's track-changes suite is unchanged after A1; a mutation that bypasses the shared lowering in either caller turns a test red; the anchor resolver refuses ambiguity in a test that would otherwise edit the wrong occurrence |
| **M1**: read-only MCP, stdio | The `read` profile: open, outline, read, styles, tables, comments, tracked changes, search, chunk, render, export (to resources), compare, report, validate; resources; the `summarise` and `compare_and_explain` prompts; `opendoc.ai` read half in Python and JS | An MCP conformance run (the official inspector, or a scripted client) against every tool; bound tests (a 1,000-page fixture returns paged, truncated-and-stated results); path-escape tests (`..`, symlinks, absolute paths) refused; a hostile-package corpus refused within budget |
| **M2**: suggestions and comments | Edit plans in `suggest` mode, `preview_edits`, `add_comment`, `undo_change`, DOCX save with the loss gate; the `review` and `redline` prompts | A plan applied by the server opens in Word with the expected revisions by the configured author (the `docs/94` oracle harness, or a measured Word-authored fixture); preview equals apply on the same plan; a suggest-profile session is refused a direct edit **by the engine** with the adapter's check removed (the §4 mutation proof) |
| **M3**: grants a person gives | `edit` and `review` profiles, elicitation for overwrite and accepted loss, Streamable HTTP with OAuth for host deployments | Authorisation tests: wrong audience, expired token, token passthrough attempt; a direct plan on a tracked-changes-protected document refused |
| **M4**: in the editor | `editor.ai` and opt-in WebMCP tools (§9) | The capability matrix in §9 asserted per role; an in-page agent's edit lands as a suggestion attributed per `175` |
| **M5**: an agent in a live room | §10 | Two humans and one agent converge under the OT suite; the agent's ops are suggestions; the relay refuses its direct edits |

M1 is read-only because it has no mutation contract to get wrong, it exercises the anchors,
bounds and limits on which everything later depends, and most of it is already engine code
(`casual-doc-viewer`, `casual-doc-diff`, `casual-doc-io`). It is not a substitute for M2:
the owner asked for editing, and M2 follows immediately on A1.

## 9. The editor: `<opendoc-editor>` and an assistant in the page

There are three ways an assistant meets an open editor, and only one of them is MCP:

1. **A host's own assistant in the page** calls `editor.ai`, a JS object on the host
   contract (ADR-036) with the same functions and schemas as `@casualoffice/opendoc-ai`,
   bound to the live session, plus what only an editor has: the current selection as an
   anchor. Its edits are commands through the same choke point as keystrokes, in `suggest`
   mode unless the role allows otherwise. No MCP is involved: running MCP inside the page to
   call itself adds a protocol and no capability (`132` §10.5).
2. **A browser agent** (an agent built into the browser or an extension) discovers the
   page's tools through **WebMCP** if the host opts in (`ai-tools="webmcp"` on the element,
   or `mount({ ai: { webmcp: true } })`). The editor then calls
   `document.modelContext.registerTool()` for each §6.2 tool the mount's role allows, with
   an `AbortSignal` it aborts when the role narrows or the editor unmounts. Read tools carry
   `readOnlyHint` and `untrustedContentHint` (document text is untrusted, §5.8);
   `review_tracked_changes` and a `direct` plan carry `consequentialHint`. WebMCP is a
   Community Group draft behind a Chrome origin trial (§11.3); the integration is
   feature-detected, off by default, and never ships a polyfill. Inside an embed, the tools
   are registered by the editor's own document (the iframe), so the host page cannot call
   them unless the host contract forwards them.
3. **A desktop agent driving a document open in a browser tab** needs a local companion
   that pairs with the tab (`132` §10.5). Deferred; not designed here.

Capability gating follows `149`'s roles, narrowed further by the host and never widened:

| Role (`capabilities.mjs`) | Tools an in-page agent may see |
| --- | --- |
| `preview` | none (a preview grants nothing, `149`) |
| `readonly` | read tools only (outline, read, search, styles, tables, review lists) |
| `commentor` | + `add_comment`, edit plans limited to `comment` |
| `edit` | + edit plans in `suggest` mode; `direct` only if the host passes `ai.direct: true` |
| `owner` | + `review_tracked_changes` |

The agent acts **on behalf of** the signed-in person. Its suggestions carry an author built
from the host identity (`175` §3), for example "Ada Lovelace (AI)", so a reviewer can tell
the person's own edits from the assistant's. The exact label format is Q3.

## 10. Live co-editing with an agent (later: M5)

An agent can join a shared room as a **participant**, with no new mutation path:

- **Identity.** The host mints a grant (JWT, `175` §12 Q6) whose subject is an agent
  principal, with a profile whose name carries the AI label and an `on_behalf_of` claim
  naming the person who invoked it. The relay attests it exactly as for a person (ADR-070):
  the agent cannot state its own name, and the people list and carets show it with an "AI"
  badge.
- **Capabilities.** `comment` and `suggest`, never `edit`, unless the room's owner grants
  more (Q9). The relay holds the write/no-write line and every replica holds the suggest
  line by projection equality (`access.rs`); a modified agent cannot widen either.
- **Edits arrive as suggestions** through OT like any participant's (`107`, `150`). People
  accept or reject them in the review pane while they keep typing.
- **Bounded.** The relay's back-pressure and participant ceiling (`152` §2c) apply. An agent
  also gets an operation budget per minute, so a runaway loop cannot flood a room with
  revisions.
- **What it needs that does not exist.** A headless replica that speaks the relay
  protocol: the collaboration client lives in `casual-doc-wasm` today, and `176` keeps it
  out of the Native SDK. M5 therefore needs either a native collaboration client crate or a
  WASM replica run by the agent's host. That is `176` Q4's question in another form, and it
  is why this phase comes last. It also waits on HF-360 (`175`) for attested identity.

## 11. Evidence and citations

### 11.1 MCP specification (revision 2026-07-28)

Read 2026-10-24. Load-bearing facts were re-read by this lane, not only by the research pass.

- Current revision and versioning: <https://modelcontextprotocol.io/specification/versioning>.
- Changes since 2025-11-25 (stateless requests, `server/discover`, no `Mcp-Session-Id`,
  server-minted handles, `subscriptions/listen`, multi-round-trip requests, `resultType`,
  `ttlMs`/`cacheScope`, tasks moved to an extension, resumability removed):
  <https://modelcontextprotocol.io/specification/2026-07-28/changelog>.
- Deprecated (Roots, Sampling, Logging, Dynamic Client Registration; earliest removal in the
  first revision on or after 2027-07-28):
  <https://modelcontextprotocol.io/specification/2026-07-28/deprecated>.
- Tools, annotations (`readOnlyHint` default false, `destructiveHint` default true,
  `idempotentHint` default false, `openWorldHint` default true; annotations from untrusted
  servers are untrusted), `outputSchema` and `structuredContent`:
  <https://modelcontextprotocol.io/specification/2026-07-28/server/tools> and
  <https://modelcontextprotocol.io/specification/2026-07-28/schema>.
- Streamable HTTP (single POST endpoint, `Origin` validation with 403, bind to 127.0.0.1
  locally, required headers):
  <https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http>.
- Authorization (OAuth 2.1 with PKCE, RFC 9728 protected resource metadata, RFC 8707
  resource indicators, audience validation, no token passthrough, RFC 9207 `iss`; stdio
  takes credentials from the environment):
  <https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization>.
- Tasks extension: <https://modelcontextprotocol.io/extensions/tasks/overview>.
- **Not read in full:** the prompts and stdio pages of 2026-07-28. §6.3's prompt use is the
  long-standing `prompts/list` and `prompts/get` shape and is to be re-checked at M1.

### 11.2 Competitors

- GongRzhe Office-Word-MCP-Server: <https://github.com/GongRzhe/Office-Word-MCP-Server>.
- SecurityRonin docx-mcp: <https://github.com/SecurityRonin/docx-mcp> (re-read by this lane).
- UseJunior safe-docx: <https://github.com/UseJunior/safe-docx>. Its addressing scheme is
  **unverified**: the README does not state it.
- ONLYOFFICE DocSpace MCP: <https://github.com/ONLYOFFICE/docspace-mcp>. That it does not
  edit document content is **unverified**: its tool reference was not read in full.
- python-docx tracked changes, open since 2016: <https://github.com/python-openxml/python-docx/issues/340>;
  comments arrived in 1.2.0: <https://github.com/python-openxml/python-docx/blob/master/HISTORY.rst>.
  Whether python-docx keeps unknown parts on save is **unverified** here.
- Microsoft Word MCP tools: <https://learn.microsoft.com/en-us/microsoft-copilot-studio/mcp-word-tools>;
  Work IQ MCP tool reference: <https://learn.microsoft.com/en-us/microsoft-365/copilot/extensibility/work-iq/mcp/tool-reference>;
  Office.js tracked changes: <https://learn.microsoft.com/en-us/javascript/api/word/word.trackedchange>.
  Copilot in Word's agent mode: **unverified** against a primary Microsoft source.
- Google Docs API structure (UTF-16 indexes per segment):
  <https://developers.google.com/workspace/docs/api/concepts/structure>; batch updates and
  `requiredRevisionId`: <https://developers.google.com/workspace/docs/api/how-tos/batch>;
  Workspace MCP (no Docs-editing server at the time of reading):
  <https://workspaceupdates.googleblog.com/2026/05/agent-tools-and-security-updates-for-workspace-developers.html>.
- No MCP server built on Aspose.Words, docx4j, the Open XML SDK or LibreOffice was found.
  That is a search result, **not** a verified absence.

### 11.3 WebMCP

- Specification: <https://webmachinelearning.github.io/webmcp/>, a Draft Community Group
  Report of the Web Machine Learning CG dated 20 October 2026, not on the W3C standards
  track (re-read by this lane). The API is `document.modelContext` with `registerTool()`,
  `getTools()` and `executeTool()`; annotations `readOnlyHint`, `untrustedContentHint`,
  `consequentialHint` and `debugging`.
- Chrome: origin trial from Chrome 149, <https://developer.chrome.com/docs/ai/webmcp>, and
  <https://developer.chrome.com/docs/ai/webmcp/imperative-api>. The trial's end date is
  **unverified**.

## 12. Risks

| # | Risk | Mitigation |
| --- | --- | --- |
| R1 | **Two lowerings drift**: the editor's suggestion code and the SDK's diverge, and an agent's tracked change differs from a person's | A1 moves the lowering to one crate before any agent edit ships; a guard fails if `casual-doc-wasm` lowers a suggestion itself |
| R2 | A model invents or misquotes an anchor | The quote is verified even when the id resolves; ambiguity and absence are refusals with candidates, never a guess |
| R3 | Context blow-up on large documents | Pagination, token ceilings with stated truncation, outline-first prompts |
| R4 | Prompt injection through document text | §5.8: no document content can change profile, roots or author; dangerous actions need launch-time grants; `132` §14 corpus |
| R5 | Path traversal or overwrite of the user's files | §5.6: canonicalised allowlist, no write root by default, no overwrite by default |
| R6 | Hostile files (zip bombs, deep nesting, giant images) | §5.7: engine ceilings, per-call budgets, worker process per hosted session |
| R7 | A flood of agent revisions nobody can review | Plan size ceiling, one revision group per plan, reject-group in one action |
| R8 | MCP revision churn | Pin one revision, negotiate the version, and keep the protocol layer thin over the schema crate |
| R9 | A dependency for the protocol (an MCP SDK crate) with its own licence and maintenance | Evaluate under the dependency policy (Q4); the protocol surface is small enough to hand-roll, as the PDF writer was (ADR-031) |
| R10 | Overclaiming against python-docx servers or Copilot | §7 marks every competitor fact with a source, and the site claims nothing until M1/M2 ship (`SKILL` §9) |
| R11 | Tracked table-structure changes are not modelled yet, so row and column edits cannot be suggestions | Those edits require `edit` until they are (Q11) |

## 13. Open questions for the owner

- **Q1. Order.** Read-only M1 first and then M2, or M1 and M2 in one release?
- **Q2. Direct edits.** May an agent ever edit without tracked changes? If yes, only when the
  operator launches with `edit`, or also per call after an elicitation?
- **Q3. The agent's author label.** "Claude (AI)", "Ada Lovelace (AI)", or "Ada Lovelace via
  Claude"? It is written into `w:author` and survives into Word.
- **Q4. Protocol dependency.** Use an MCP SDK crate (subject to the dependency policy), or
  hand-roll the JSON-RPC layer?
- **Q5. Hosted MCP.** Is a Streamable HTTP deployment with OAuth in scope (M3), or local
  stdio only for now?
- **Q6. WebMCP.** Build the opt-in in-page integration while WebMCP is a draft, or wait for
  it to stabilise?
- **Q7. Names.** `opendoc-mcp` for the binary, `opendoc.ai` (PyPI module) and
  `@casualoffice/opendoc-ai` and `@casualoffice/opendoc-mcp` (npm), and the tool names in
  §6.2?
- **Q8. Semantic search.** Confirm that OpenDoc provides anchored chunks and the host
  brings embeddings (this document), rather than OpenDoc shipping a vector index (`132`
  DAI-2) in the MCP server.
- **Q9. Agents in rooms.** Suggest-only always, or may a room owner grant an agent `edit`?
- **Q10. Model provider terms.** The server never calls a model, so document text reaches a
  provider only through the agent the user runs. Should the docs state that the operator is
  responsible for that provider's data terms, and nothing more?
- **Q11. Tracked table structure.** Model tracked row and column insertion and deletion
  (Word has `w:ins`/`w:del` on rows) so table edits can be suggestions too?
- **Q12. Older MCP clients.** Also answer the 2025-11-25 handshake for clients that have not
  moved to 2026-07-28, or support 2026-07-28 only?

## 14. What this change is

Documentation only: this document, ADR-072 (proposed), and `docs/109` RM-47. No code,
no dependency, no support claim.

## Owner decisions, 2026-10-24

- **First release is M1 + M2 together:** read, search, render, convert and compare, plus tracked suggestions and comments. Moving suggestion editing out of `casual-doc-wasm` into a shared crate is therefore on the critical path.
- **Direct (untracked) edits only with an explicit grant.** The default is tracked suggestions. A host or operator may grant a session edit rights; those edits are still undoable and loss-checked.
- **Transport: local stdio first.** Hosted streamable HTTP with OAuth comes later (M3), after the host identity contract (HF-360, JWT grants) is built.
- **WebMCP (M4): opt-in behind a flag**, off by default, tracking the draft specification.

## 15. Built, 2026-10-24

What exists, measured by the tests named, and what does not.

**Built.**

- A1: `crates/casual-doc-review` holds the one lowering of every review command; the
  editor's methods wrap it (#879). Guard: `casual-doc-review/tests/one_lowering.rs`.
- The Rust core: `crates/casual-doc-ai` (`Session`, `read`, `anchor`, `plan`, `compare`).
  Every plan runs on a copy-on-write clone of the session state and commits through ONE
  choke point that calls the engine's `refuse_if_not_permitted` and `refuse_if_protected`
  before the transaction log applies it; preview runs the same code and discards the clone.
- `tools/opendoc-mcp`: stdio, newline-delimited JSON-RPC; answers `server/discover` and,
  for older clients, `initialize` (2025-11-25, 2025-06-18, 2025-03-26: Q12 answered yes);
  the 20 tools of §6.2 filtered by profile (16 under `read`), the resource templates and the
  four prompts of §6.3; `--root`, `--write-root` (default none), `--profile` (default `read`),
  `--author`, `--allow-overwrite`; an audit line per call on stderr with no document text.
- Guards, each driven red by a mutation in its PR: MCP conformance over the real binary
  (list, then call every tool); a suggestion saved as DOCX carries `w:ins`, `w:del`,
  `w:delText` and the configured `w:author`, and reopens as two tracked changes; a direct
  plan is refused without an edit grant (and refused **by the engine** with the adapter's
  check removed); paths outside the roots (`..`, absolute, `file://`, a symlink) are refused
  without echoing the target; `.docm` is refused.

**An engine defect the first headless suggester found.** `is_tracked_only` and
`is_comment_only` compared paragraphs by full equality, ids included, so a suggestion or a
comment that split a run (any mark not on a run boundary) was refused as an untracked edit
for every suggest-only or comment-only participant. `protection.rs` now rejoins only runs the
split minted fresh onto the run before them before comparing; a held run that comes back
re-minted still differs, so ADR-052's identity convention is unchanged.

**Not built, stated.**

- Every surface is read (`docs/181` §12 "Lane surfaces"); charts, SmartArt, OLE objects, alt-chunks and equations without a linear form are named and counted as `omitted`, not projected.
- `search` has exact, case-insensitive and whole-word modes; regular expressions are not
  supported (no regex dependency), and case folding is ASCII-only.
- Paragraphs holding an equation, a note reference or a preview-less embedded object are
  refused for edits (`unsupported-content`): the engine's two offset spaces disagree there.
- Table structure (`table_insert_row`/`table_delete_row`) is direct-only (Q11); `insert_table`
  and column edits are not in vocabulary version 1.
- Schemas are hand-written in `tools/opendoc-mcp/src/catalog.rs`; A4 (generated schemas and
  the staleness guard) is not built. Resource subscriptions, the tasks extension,
  elicitation, Streamable HTTP and OAuth (M3), WebMCP (M4), rooms (M5), and the Python and
  JavaScript bindings are not built.
- The preview does not compute whether a plan changes the compatibility report; it says so.
- `get_styles` reports what each style declares, not the cascaded effective formatting.
- `undo_change` applies the log's exact inverse of the latest change without re-judging it
  as a new authoring act (withdrawing one's own suggestion removes a revision, which the
  suggest class does not admit as fresh input).
