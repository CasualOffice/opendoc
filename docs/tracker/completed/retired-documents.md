# Retired planning and research files

These completed plans and studies no longer need separate files in the working
documentation. Their original text remains recoverable from commit
`24c8e684181f7b8ed031a293cfdb87b2325d8422` with
`git show <commit>:<old path>`. Find an old path with
`git ls-tree -r --name-only <commit> docs` and its document number. Current work
belongs in the [single queue](../../109-BACKLOG.md).

| Former doc | Why it was retired | Current source |
| --- | --- | --- |
| <a id="doc-3"></a>03 — high-level design | Proposed workspace and component map predates the implemented workspace | [Architecture](../../02-ARCHITECTURE.md), [workspace](../../../Cargo.toml) |
| <a id="doc-4"></a>04 — low-level design | Proposed model and API types predate the implemented schema | [Schema v1](../../38-SCHEMA-V1-DESIGN-REFERENCE.md), [SDK API](../../05-SDK-API-SPEC.md) |
| <a id="doc-6"></a>06 — roadmap | Historical phases and product order have been superseded | [Current batch plan](../../168-WORD-AND-DOCS-RIVAL-PLAN.md), [backlog](../../109-BACKLOG.md) |
| <a id="doc-9"></a>09 — repository and contribution | Repeats the live contributor workflow | [Contributing](../../../CONTRIBUTING.md) |
| <a id="doc-10"></a>10 — project goal and standards | Repeats the repository mission and rules | [Agent instructions](../../../AGENTS.md), [outcomes](../../01-ORD.md) |
| <a id="doc-12"></a>12 — competitive analysis | Early comparison predates the measured capability matrix | [Parity matrix](../../153-ONLYOFFICE-CAPABILITY-PARITY-MATRIX.md), [rival plan](../../168-WORD-AND-DOCS-RIVAL-PLAN.md) |
| <a id="doc-13"></a>13 — UX and bug hunting | General review checklist; no feature contract | [Quality and security](../../07-QUALITY-SECURITY-AND-COMPATIBILITY.md), [backlog](../../109-BACKLOG.md) |
| <a id="doc-19"></a>19 — workspace scaffold | Phase 0 implementation plan is complete | [Repository guide](../../../CONTRIBUTING.md) |
| <a id="doc-22"></a>22 — schema v0 | Superseded by the typed v1 design | [Schema v1](../../38-SCHEMA-V1-DESIGN-REFERENCE.md) |
| <a id="doc-23"></a>23 — fixture corpus plan | The manifest and fixture guide now own the corpus | [Fixture guide](../../../fixtures/README.md), [manifest](../../../fixtures/manifest.json) |
| <a id="doc-29"></a>29 — benchmark harness plan | The runnable benchmark guide and CI gate own the procedure | [Benchmark guide](../../../benchmarks/README.md), [CI gates](../../15-CI-AND-RELEASE-GATES.md) |
| <a id="doc-30"></a>30 — Phase 0 closure design | Superseded by the completed execution record | [Execution archive](../../14-EXECUTION-TRACKER.md) |
| <a id="doc-31"></a>31 — Phase 0 exit report | Completion is recorded in the execution archive | [Execution archive](../../14-EXECUTION-TRACKER.md) |
| <a id="doc-33"></a>33 — DOCX engine competitor research | The adopted fidelity rules live in the architecture and decision record | [OOXML architecture](../../34-OOXML-FIDELITY-ARCHITECTURE.md), [ADR-027 acceptance](../../36-ADR-027-ACCEPTANCE-RECORD.md) |
| <a id="doc-37"></a>37 — Phase 1A decision research | Its decisions were resolved in the acceptance record | [ADR-027 acceptance](../../36-ADR-027-ACCEPTANCE-RECORD.md) |
| <a id="doc-66"></a>66 — developer site design | Site and demo are implemented; source is the current contract | [Site entry](../../../webapp/index.html), [repository README](../../../README.md) |
| <a id="doc-69"></a>69 — editor shell competitive guide | Completed visual direction is embodied in the shell and design system | [Editor design system](../../63-EDITOR-UI-UX-DESIGN-SYSTEM.md), [ribbon design](../../64-EDITOR-TOOLBAR-RIBBON-DESIGN.md) |
| <a id="doc-71"></a>71 — history and formatting inspector | Bounded editor slice is implemented | [Editor architecture](../../56-EDITOR-SHELL-AND-RENDER-ARCHITECTURE.md), [editing operations](../../59-V1-EDITING-OP-SET.md) |
| <a id="doc-80"></a>80 — cursor hit audit | Short completed observation; pointer and selection code own behavior | [Selection architecture](../../58-INTERACTION-SELECTION-EDITING-ARCHITECTURE.md), [selection code](../../../webapp/src/editor_selection.mjs) |
| <a id="doc-145"></a>145 — experimental portfolio roadmap | Proposed umbrella checklist duplicates the active batch plan and feature contracts | [Rival plan](../../168-WORD-AND-DOCS-RIVAL-PLAN.md), [feature architecture](../../architecture/README.md) |

For an exact historical claim, read its original file at the commit above. Do
not treat the old plan or research as a current support statement.
