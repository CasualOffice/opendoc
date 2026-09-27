# 126 — Experimental Document Assistance, Semantic Search, and MCP Architecture

**Status:** Proposed experimental future feature; documentation only. Not accepted,
implemented, supported, or committed to v1.

**Opened:** 2026-09-27.

**Decision record:** proposed ADR-035.

**Depends on:** docs 05, 24, 27, 45, 59, 68, 82, 83, 84, 107, and 116.

**Explicit separation:** this design is unrelated to PDF semantic reconstruction and
OCR in doc 131. PDF/OCR is an import problem. This document concerns assistance over
an already-open normalized document, regardless of its source format.

---

## 1. Outcome

Design OpenDoc's future AI and agent capability from user jobs first:

- understand, search, and summarize document content;
- improve, proofread, shorten, expand, or translate selected text;
- convert text, lists, and tables into other editable structures;
- apply deterministic formatting from natural-language intent;
- review a document and produce anchored comments or suggestions;
- generate bounded content and propose where to insert it;
- execute larger workflows only through an inspectable plan.

The resulting **Document Assistance Layer** is independent of any model vendor and
independent of MCP. The web UI, an embedded local model, a host-provided model, a
plugin, and an external MCP agent may all consume the same layer.

MCP is therefore an optional transport adapter at the outside boundary. It is not
the product capability model, a second mutation path, a vector database, or a model
runtime.

## 2. Research baseline

The dominant document-assistance interaction is selection- and review-oriented:

- Microsoft Word exposes selected-text rewrite, summarize, reorganize, format, and
  text-to-table actions, followed by Replace, Insert below, Regenerate, or review
  choices: <https://support.microsoft.com/en-us/word/copilot/edit-rewrite-content>.
- Google Docs exposes Rephrase, Shorten, Elaborate, Bulletize, Summarize, tone
  changes, natural-language formatting, and individual Accept/Reject decisions:
  <https://support.google.com/docs/answer/13447609>.
- Notion exposes selection skills such as Improve Writing, Proofread, Explain, and
  Reformat: <https://www.notion.com/help/create-and-manage-skills>.
- Word's improvement flow returns paragraph-anchored suggestion cards rather than
  silently replacing the whole document:
  <https://support.microsoft.com/en-us/word/copilot/improve-writing>.
- Word also provides deterministic text-to-table and table-to-text conversion,
  demonstrating that structural conversion is not inherently an LLM task:
  <https://support.microsoft.com/en-us/word/convert-text-to-a-table-or-a-table-to-text>.

These references are product-behaviour evidence, not fidelity claims or dependencies.
The common interaction contract is:

```
explicit scope -> requested outcome -> bounded proposal -> preview/diff
               -> accept/reject -> one transaction -> exact undo
```

## 3. Governing decisions

1. **User scenarios precede protocol design.** The domain API is named around
   document scopes, proposals, findings, and change sets. It is not named around
   MCP requests or a specific LLM SDK.
2. **Reasoning and mutation are separate.** A model may propose content or a typed
   intent. Only OpenDoc resolves anchors, validates structure, compiles operations,
   and mutates the document.
3. **Deterministic work stays deterministic.** Exact text/table conversion, style
   application, range replacement, insertion, and export use engine commands without
   requiring a model.
4. **Every semantic edit is proposed before it is authoritative.** Direct agent
   edits are opt-in host policy, not the default.
5. **One mutation choke point remains absolute.** Accepted proposals commit through
   the same command, operation, transaction, history, review, and event path as a
   human edit (docs 45 and 107).
6. **Derived data remains disposable.** Embeddings, chunks, cached summaries,
   findings, prompts, and unaccepted proposals live in a sidecar keyed by stable
   document anchors. None enters schema v1 or a saved DOCX unless a user accepts a
   corresponding ordinary document edit.
7. **Hosts own policy.** Model choice, network access, credentials, storage,
   retention, telemetry, external sources, permissions, and MCP exposure belong to
   the host.
8. **No mandatory server.** Local semantic indexing works in a browser. An external
   desktop MCP client may require an optional local companion, but document
   processing does not move to a remote service.
9. **No silent scope expansion.** Selection, table, section, document, and collection
   scopes are distinct. A request may not widen its scope implicitly.
10. **Source anchors accompany conclusions.** Search hits, answers, summaries,
    findings, and proposed edits retain `NodeId`/range provenance wherever the source
    document permits it.

## 4. User-scenario catalogue

### 4.1 Understand without editing

| Scenario | Default scope | Result |
| --- | --- | --- |
| Explain selected text | selection | answer with source anchor |
| Summarize selection | selection | answer or insertable proposal |
| Summarize section/document | explicit section/document | hierarchical anchored summary |
| Ask a document question | document or explicit collection | answer with cited ranges |
| Extract decisions/action items/risks | section/document | structured findings or proposed table |
| Find unclear or contradictory passages | document | anchored review findings |

These are the first experimental scenarios because they are read-only.

### 4.2 Improve selected writing

Supported intents may include proofread, improve clarity, shorten, elaborate,
simplify, change tone, adapt to an audience, remove repetition, and match a supplied
style. The default result is a diffable replacement proposal over the exact captured
selection. Available outcomes are Replace, Insert below, Add as suggestion,
Regenerate, or Discard.

The request must state what is invariant. For example, "make concise without changing
meaning, identifiers, citations, or numbers" is materially safer than "improve this."

### 4.3 Translate

Translation is a first-class recipe with:

- source and target language/locale;
- explicit scope;
- formal/informal preference where the language distinguishes it;
- optional terminology glossary and do-not-translate terms;
- output mode: replace, insert below, side-by-side, suggestion, or document copy;
- preservation rules for styles, links, fields, citations, footnotes, comments, and
  tables.

Only translatable text is given to the model. OpenDoc owns reconstruction around
non-text inline nodes. Translation must not flatten a table, remove a hyperlink,
rewrite a field code, or merge review attribution merely because a model returned a
plain string.

### 4.4 Transform structure

Examples include:

- paragraphs to bullets, numbered steps, or a checklist;
- notes to a Task/Owner/Due date/Status table;
- key/value text to a two-column table;
- bullets to narrative prose;
- table to delimited text;
- table to a narrative summary;
- selected blocks to headings and sections;
- meeting notes to decisions and action items.

There are two paths:

1. **Deterministic transform:** delimiters, row/column boundaries, and target structure
   are explicit. The engine performs the conversion without a model.
2. **Semantic transform:** columns, categories, or structure must be inferred. A model
   returns bounded structured content; OpenDoc validates it and constructs typed table,
   list, paragraph, and block operations.

Raw OOXML, internal node JSON, and arbitrary snapshot replacement are never accepted as
model output.

### 4.5 Format and standardize

Examples include formatting a selection, applying a paragraph style, normalizing all
Heading 2 paragraphs, fixing inconsistent list indentation, standardizing tables, or
applying an organization template without rewriting content.

Natural language may resolve into a typed format plan, but the plan executes as
deterministic format, paragraph, list, table, section, and style commands. If the
instruction is already structured (for example, "Heading 2: 16 pt, bold, navy"), no
generative model is required.

### 4.6 Review rather than rewrite

Review recipes produce findings, not direct edits:

- unclear wording;
- inconsistent terminology;
- undefined acronyms;
- claims needing citations;
- contradictions or duplication;
- missing sections against a checklist;
- template and style-guide deviations;
- accessibility findings such as missing object descriptions.

Each finding carries category, severity, explanation, source anchors, and an optional
proposed correction. A user may convert it to a comment, tracked suggestion, ordinary
edit, or dismissal. Doc 68's review model is the presentation and decision substrate.

### 4.7 Generate and insert

Examples include an executive summary, introduction, conclusion, FAQ, risk section,
cover email, or continuation at the caret. Generated content always has an explicit
insertion target and arrives as a proposal. It never replaces unrelated surrounding
content because the model inferred a better location.

### 4.8 Multi-step workflows

A request such as "improve the executive summary, standardize headings, and create a
risk table" becomes a visible ordered plan. Each step names scope, expected mutation,
and approval requirement. The user may approve the whole plan or individual steps.
Failure is atomic per step; it does not leave a partially constructed table or partially
rewritten selection.

## 5. Scope and result model

### 5.1 Scope

The assistance layer needs a versioned, public scope vocabulary:

- current caret;
- exact text range;
- selected block sequence;
- selected list;
- selected table, rows, columns, or cells;
- section;
- named structural query, such as all Heading 2 paragraphs;
- whole document;
- explicit host-approved document collection.

Every scope resolves to stable anchors plus a `base_revision`. Text captured for a
request also carries a hash. If the document changes before commit, OpenDoc either maps
the anchors safely or rejects the proposal as stale. It never searches for matching text
and edits the first coincidence.

### 5.2 Result modes

| Mode | Mutates the document? | Typical use |
| --- | --- | --- |
| Answer | No | explain, Q&A, summary in chat |
| Findings | No | review, compliance, ambiguity detection |
| Preview | No | deterministic format/structure plan |
| Suggestion | Yes, as review markup | AI-authored change awaiting decision |
| Insert | Yes after approval | summary, section, table |
| Replace | Yes after approval | rewrite or translation |
| Document copy | Creates a separate session/file | whole-document translation |

Answer and Findings must remain useful even when the current SDK cannot yet perform the
corresponding edit.

## 6. Logical architecture

```
OpenDoc UI       Embedded assistant       Plugin       External MCP agent
    |                    |                   |                 |
    +--------------------+-------------------+-----------------+
                             adapter boundary
                                      |
                                      v
                         Document Assistance Service
            +------------------------------------------------+
            | scope resolver and context builder             |
            | recipe/intent registry                         |
            | deterministic transform planner                |
            | model-provider port (optional)                 |
            | proposal/change-set compiler                   |
            | validation, policy, approval, and provenance   |
            +------------------------------------------------+
                     |                          |
                     v                          v
          Semantic Retrieval Service       Command/Transaction Service
          +----------------------+          +---------------------------+
          | projection/chunking  |          | preview and stale check   |
          | lexical index        |          | typed operation compile   |
          | embedding provider   |          | atomic commit + history   |
          | hybrid ranking       |          | events + exact undo       |
          +----------------------+          +---------------------------+
                     |                          |
                     v                          v
            Derived sidecar only       normalized document model
```

The assistance service is orchestration, not a second editor engine. It cannot write
model fields, serialize OOXML, or bypass validation.

## 7. Document Assistance Service

### 7.1 Recipe registry

A recipe describes a user job rather than a protocol call. It declares:

- stable recipe ID and version;
- allowed scope kinds;
- required inputs and clarifications;
- whether model reasoning is required, optional, or forbidden;
- maximum context and output bounds;
- content and structure preservation rules;
- permitted result modes;
- command capabilities needed to realize an accepted result;
- approval and audit classification;
- evaluation corpus and metrics.

Initial experimental recipes should be selection summary, selection explanation,
proofread, improve clarity, shorten, translate selection, text-to-list,
text-to-table, table-to-text, table-to-summary, format selection, extract action
items, and review document.

### 7.2 Context builder

The context builder projects only what a recipe needs:

- bounded plain text;
- structure and heading path;
- marks and paragraph style summaries;
- table headers and selected cells;
- neighbouring context explicitly allowed by the recipe;
- glossary, template, or external sources explicitly selected by the user;
- stable source anchors withheld from model-visible prose where a provider cannot
  round-trip opaque IDs reliably, but retained in the host-side mapping.

Unsupported or security-sensitive content is reported. It is not silently flattened.

### 7.3 Model-provider port

The host may provide:

- a remote model API;
- a browser-local model;
- a native local model;
- an already-running external MCP agent;
- no model, in which case deterministic recipes still work.

The core runtime never owns credentials or performs an implicit network request. A
provider advertises languages, context/output limits, structured-output support,
cancellation, privacy locality, and model/version identity.

### 7.4 Proposal and change set

Model output is first decoded into a bounded proposal. A proposal records:

- recipe and provider provenance;
- source document/session and base revision;
- exact target scopes and source hashes;
- human-readable explanation;
- typed replacement text, blocks, table cells, format specification, or findings;
- warnings and unsupported preservation requests;
- expected effect summary;
- expiry and idempotency identity.

Compilation converts a valid proposal to the closed OpenDoc operation set. Preview uses
the same compiler as commit; commit does not reinterpret natural language a second time.

## 8. Local semantic search and retrieval

### 8.1 Purpose

Semantic retrieval is one reusable OpenDoc capability consumed by search UI, Q&A,
query-focused summaries, review recipes, related-content discovery, and MCP. MCP does
not own the index.

### 8.2 Structural projection and chunks

Chunks follow document structure instead of arbitrary byte windows. Candidate chunk
kinds include:

- heading plus bounded following paragraphs;
- one long paragraph or a group of short paragraphs;
- list heading plus bounded items;
- table header plus bounded rows;
- one large table row;
- note plus its reference;
- comment thread plus its anchor when review content is in scope;
- text-box or running-content text when the requested surface includes it.

Each sidecar record carries a deterministic chunk ID, document ID, revision, ordered
`NodeId`/range anchors, structural kind, heading path, language, text hash, chunker
version, embedding model/version/dimension, vector, and policy tags.

### 8.3 Browser execution

Browser-local embedding inference is feasible through WebGPU with a WASM fallback:

- Transformers.js demonstrates feature extraction and normalized embeddings directly
  in the browser: <https://huggingface.co/docs/transformers.js/guides/webgpu>.
- ONNX Runtime Web supports WebGPU and the broader WASM execution provider:
  <https://onnxruntime.ai/docs/get-started/with-javascript/web.html>.

No dependency or model is accepted by this design. Evaluation candidates include a
small English model such as `mixedbread-ai/mxbai-embed-xsmall-v1` and a multilingual
profile such as `intfloat/multilingual-e5-small`; selection requires operator coverage,
licence, artifact, privacy, download, memory, quality, and cross-browser gates.

Embedding work runs in a dedicated Worker, is cancellable, and is scheduled below
editing, selection, layout, and rendering. WebGPU absence falls back to WASM when the
selected profile supports it; keyword search remains available if neither profile can
run. There is no silent cloud fallback.

### 8.4 Incremental lifecycle

Committed transactions identify affected anchors. The retrieval service recomputes only
affected chunks:

```
transaction -> affected NodeIds -> rebuild local chunk neighbourhood
            -> unchanged text hash: retain vector
            -> changed text hash: enqueue embedding
            -> removed anchor: delete chunk
```

A dropped event or unknown structural effect invalidates the relevant scope and requests
a bounded re-projection. It never trusts an incremental index after the event journal has
reported a gap.

### 8.5 Hybrid ranking

The default search combines:

- exact/substring or lexical relevance for names, identifiers, dates, and quotations;
- vector similarity for semantic relations and paraphrases;
- structural filters for scope, surface, heading, table, note, and review content;
- bounded neighbour expansion so an answer receives the context around a hit;
- deterministic score merging and tie-breaking for a fixed index/model version.

Modes are `exact`, `keyword`, `semantic`, `hybrid`, and `related`. Every result returns
source anchors, structural metadata, index completeness, model/version where relevant,
and the current revision. Raw embedding vectors are not exposed to ordinary clients or
MCP agents.

### 8.6 Storage

The browser host may persist the sidecar in IndexedDB or OPFS under a versioned schema.
Native hosts may choose another bounded store. Derived records are partitioned by host,
principal, collection, document identity, and model version. Deleting a document or
collection can purge its records without touching document bytes.

Embeddings are sensitive derived document data. They do not enter telemetry, sync, or a
shared collection unless host policy and user consent allow it.

### 8.7 Scale policy

For an ordinary single document, exact cosine scan over a compact typed vector array is
the initial baseline: it is simple, deterministic, and has no approximate-index
correctness debt. Approximate nearest-neighbour indexing is justified only after measured
single-document or explicit collection workloads exceed that baseline.

At 384 float32 values, vector payload alone is about 1.5 KiB per chunk: roughly 15 MiB
for 10,000 chunks and 150 MiB for 100,000 before metadata. Quantization is an optional
profile, not an excuse for unbounded indexing.

The 1.3-million-paragraph stress case in doc 116 forbids eager whole-document indexing at
open. Index headings/current section first, then extend in bounded background batches with
progress, pause, cancellation, memory ceilings, and honest partial-index status.

## 9. Summarization and retrieval

Summarization is not vector search:

- a selection summary reads the bounded selection directly;
- a section summary traverses that section structurally;
- a whole-document summary is hierarchical: bounded section summaries followed by a
  final summary with retained source coverage;
- a query-focused summary uses hybrid retrieval, neighbours, and explicit source ranges.

Cached section summaries are derived sidecar entries invalidated by the hashes of their
source chunks. A summary must reveal whether it covered the full requested scope or a
partial index/context window.

An external MCP agent can perform generation itself. Optional browser-only generation is
technically possible through engines such as WebLLM, which supports WebGPU and Worker-based
local inference (<https://github.com/mlc-ai/web-llm>), but it is a separate, heavier
profile. Local embeddings must not require shipping a local generative model.

## 10. MCP adapter

### 10.1 Role

The MCP adapter exposes approved assistance and document capabilities to external LLM
hosts. The current MCP specification defines resources, tools, prompts, notifications,
and elicitation, with stdio for local processes and Streamable HTTP for network services:
<https://modelcontextprotocol.io/docs/2026-07-28/learn/architecture>.

The adapter is model-independent by default. The external agent reasons over bounded
resources and calls deterministic proposal, review, and commit tools. OpenDoc does not
invoke a second model merely because the caller used MCP. The 2026-07-28 protocol also
deprecates client sampling, so the design does not depend on an MCP server asking its
client to run generation; generation remains an explicit host/provider responsibility.

### 10.2 Resources

Candidate versioned resources and templates include:

```
opendoc://sessions
opendoc://sessions/{session}/metadata
opendoc://sessions/{session}/outline
opendoc://sessions/{session}/selection
opendoc://sessions/{session}/ranges/{range}
opendoc://sessions/{session}/nodes/{node}
opendoc://sessions/{session}/styles
opendoc://sessions/{session}/review
opendoc://sessions/{session}/proposals/{proposal}
opendoc://sessions/{session}/semantic-index/status
```

Resources are bounded, paginated where necessary, and revision-labelled. A full raw
snapshot is not the default context mechanism.

### 10.3 Tools

The initial read-only tool family should be small:

- `document.get_scope`
- `document.search`
- `document.find_related`
- `document.read_ranges`
- `document.get_outline`
- `document.get_index_status`

Proposal tools may then add:

- `document.propose_text_replacement`
- `document.propose_block_replacement`
- `document.propose_table`
- `document.propose_format_plan`
- `document.add_review_comment`
- `document.preview_change_set`
- `document.commit_approved_change_set`
- `document.discard_change_set`

There is no unrestricted `apply_document_edits` tool in the first experimental
profile. Commit requires a change-set identity bound to document, revision, scopes,
policy, and—when required—an approval capability issued by the OpenDoc host UI.

### 10.4 User-oriented prompts

Optional MCP prompts can represent the researched workflows without moving policy into
the protocol adapter:

- improve selected writing;
- translate selection;
- summarize document;
- create a table from selection;
- convert table to narrative;
- review document;
- standardize formatting.

Prompts are conveniences. The domain recipes and enforcement remain authoritative.

### 10.5 Deployment topologies

**Native/headless document:** a local `opendoc-mcp` process may use stdio and the stable
Rust SDK. This is the preferred first MCP topology.

**Currently open browser/WASM document:** a browser cannot be an external desktop host's
stdio child. An optional local companion must expose MCP and pair with the active browser
session over authenticated loopback/IPC. The browser session remains authoritative and
all document processing remains in WASM.

**Assistant embedded in the OpenDoc web app:** use the Document Assistance API directly.
Running MCP inside the app solely to call itself adds protocol overhead and no capability.

**Remote Streamable HTTP:** host-owned optional deployment only. It requires explicit
authentication, authorization, origin/host validation, transport security, tenancy, and
data-residency policy. It is never required for local editing or local search.

## 11. Security, privacy, and approval

Document text, comments, fields, links, imported metadata, model output, MCP input, and
MCP tool descriptions are untrusted.

Required controls include:

- read-only by default; separate capabilities for search, external context, comment,
  suggest, edit, export, filesystem, and network;
- explicit scope and per-principal document authorization;
- bounded request, output, chunk, result, operation, time, memory, and model-download
  limits;
- `base_revision`, source hash, idempotency key, and atomic transaction on mutation;
- preview/diff and exact undo for every accepted semantic change;
- no macro execution, link following, shell execution, or instruction execution caused
  by document content;
- no raw OOXML or normalized snapshot replacement from a model;
- no automatic external-source lookup unless the user enabled that source;
- audit provenance without logging full document content by default;
- purge and retention controls for vectors, summaries, prompts, and proposals;
- stdio for a local MCP process when practical; authenticated restricted loopback/IPC
  otherwise.

MCP annotations are hints, not enforcement. The server and OpenDoc host enforce policy.
The MCP specification's consent and security requirements remain applicable:
<https://modelcontextprotocol.io/specification/2026-07-28> and
<https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices>.

## 12. Current repository fit and blockers

### Useful foundations already present

- stable `NodeId` identities and model positions;
- the closed, invertible `casual-doc-edit::Operation` set;
- comments, suggestions, accept/reject, and review UI (docs 68 and 82);
- deterministic normalized model import/export;
- browser/WASM command surfaces for a broad editing set;
- doc 45's derived sidecar invariant;
- a bounded event journal with explicit gap reporting (doc 27).

### Blocking gaps

1. **Two mutation paths:** doc 107 proves the live WASM editor does not use the
   revision-aware transaction envelope used by `casual-doc-sdk`. Agent commits cannot be
   production-safe until the paths converge.
2. **Narrow stable SDK:** doc 05's implemented public snapshot and commands cover only the
   Phase-0 paragraph/text subset. The broader document model and commands are target API.
3. **No public structural scope contract:** table/cell, block-sequence, section, review,
   running-content, and collection scopes need versioned SDK values.
4. **No proposal/change-set abstraction:** current commands apply immediately. Assistance
   needs preview, provenance, expiry, approval, stale checks, and commit identity.
5. **No semantic projection or sidecar store:** doc 45 preserves the seam but supplies no
   implementation.
6. **Event payload limitations:** transaction events carry revision, count, and position
   map, not a complete affected-anchor/change delta. Exact incremental indexing needs a
   stable affected-scope signal or safe bounded invalidation.
7. **Main-thread debt:** doc 116 forbids whole-document work on interaction paths. Search,
   indexing, review results, and proposal panels must be windowed/backgrounded and added to
   the complexity and long-task gates.

These are prerequisites, not reasons to bypass the SDK with direct WASM model access.

## 13. Experimental delivery order

### DAI-0 — Accept or reject the architecture

- finalize the user-scenario catalogue and priority;
- decide privacy profiles and supported deployment topologies;
- define proposal, scope, provenance, and capability schemas;
- select no model or dependency yet.

### DAI-1 — Read-only bounded assistance foundation

- stable structural scopes and projections;
- exact/keyword query API with anchored results;
- read-only answer/findings flows;
- virtualized result UI;
- no MCP and no generative dependency required.

### DAI-2 — Local semantic retrieval

- Worker-based chunk projection and embedding profile;
- versioned sidecar and incremental invalidation;
- hybrid search, related content, index status, and source navigation;
- keyword-only fallback and honest partial-index status.

### DAI-3 — Localized proposals

- proposal/change-set lifecycle;
- proofread, improve, shorten, translate selection, text/list/table recipes;
- tracked suggestion as the default mutation result;
- preview, granular accept/reject, stale rejection, and exact undo.

### DAI-4 — Read-only MCP profile

- local native/headless stdio companion first;
- resources plus search/read/outline/index-status tools;
- capability, authorization, audit, and conformance tests;
- no direct mutation tools.

### DAI-5 — Approved MCP proposals

- propose/preview/comment tools;
- commit only with host policy and approval capability;
- optional paired browser companion after the native topology is sound.

### DAI-6 — Document-wide and collection workflows

- hierarchical summaries;
- whole-document format/review plans;
- explicit local collections and cross-document retrieval;
- approximate vector index only if measurements justify it;
- optional browser-local generative profile evaluated separately.

## 14. Graduation gates

This feature remains experimental until all applicable gates pass:

- user-scenario acceptance tests for every declared recipe, scope, and result mode;
- full preservation tests for marks, paragraph properties, links, fields, notes,
  comments, revisions, tables, and unsupported inline objects across proposed edits;
- stale revision, moved anchor, deleted anchor, duplicate retry, cancellation, and
  partial-failure tests;
- exact undo and DOCX save/reopen semantic fixed points for accepted proposals;
- prompt-injection and malicious-document tests proving document text cannot widen
  capability, cause network access, execute code, or bypass approval;
- local-only network interception tests for the local semantic profile;
- deterministic chunk identity and lexical ranking; declared tolerance and golden
  evaluation for model-dependent vector ranking;
- multilingual retrieval and translation corpora for every claimed language/profile;
- retrieval recall, exact citation-anchor, summary coverage, hallucination, and
  unsupported-content reporting metrics;
- Worker cancellation, main-thread long-task, memory, storage, download, cold-start,
  incremental-update, and very-large-document gates;
- event-gap recovery that invalidates/rebuilds rather than returning stale search data;
- MCP protocol conformance, malformed request, resource bound, authorization,
  confused-deputy, state-handle, localhost, and approval tests;
- browser, native, offline, and fallback-profile matrices matching the support claim;
- dependency licences, immutable model/artifact hashes, SBOM, and update policy;
- support matrix, SDK specification, security documentation, and examples updated from
  measured behavior rather than intended architecture.

## 15. Non-goals

- PDF parsing, OCR, or PDF-to-DOCX reconstruction;
- training or fine-tuning foundation models;
- making an LLM or MCP mandatory for ordinary editing;
- embedding vectors or prompts in OOXML;
- allowing a model to emit raw OOXML or replace the normalized snapshot;
- treating semantic search as authoritative fact verification;
- indexing every document the host can access without an explicit collection policy;
- selecting a cloud provider, MCP SDK, embedding model, ANN library, or local LLM in
  this proposal.

## 16. Open questions

1. Which scenarios constitute the first experimental product slice: read-only search and
   summary only, or selected-text proposals as well?
2. Is tracked suggestion mandatory for all AI-authored edits, or may hosts pre-authorize
   direct edits within bounded recipes?
3. What is the canonical structural scope schema for table cells, block sequences,
   running content, notes, and text boxes?
4. How are source anchors represented to a model without depending on the model to return
   opaque identifiers correctly?
5. Which provider profiles may receive document text, and how is that policy communicated
   before a request?
6. What browser model-download ceiling and cold-start budget are acceptable on desktop,
   tablet, and mobile?
7. Is an English-small and multilingual-large embedding split acceptable, or must one
   profile cover every supported language?
8. When does exact scan cease to meet the collection latency/memory budget, and which ANN
   algorithm can meet deterministic rebuild and deletion requirements?
9. Should cached summaries persist across sessions, or are they session-only until a user
   explicitly saves one into the document?
10. Which MCP clients and protocol revision form the first interoperability matrix?
11. Does the optional browser companion use authenticated loopback HTTP, WebSocket, native
    messaging, or a host-specific bridge?
12. How are approvals represented so an MCP client cannot replay an old approval against a
    different revision or scope?
