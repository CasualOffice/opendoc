# Design-First Delivery Process

## Purpose

The project uses design-first delivery because document editors fail when implementation outruns semantics. Layout, transactions, DOCX fidelity, selection, collaboration, and API design all require explicit decisions before code becomes expensive to unwind.

## Standard Flow

### 1. Problem Definition

Document:

- user or integrator outcome;
- affected subsystem;
- current known constraints;
- compatibility expectations;
- security and performance concerns.

### 2. Research

Use:

- project docs;
- existing Casual Docs behavior as reference only;
- relevant specifications;
- competitor behavior;
- current dependency/platform documentation.

Record links and conclusions when they affect design.

### 3. Design Note

For substantial work, create a design note under `docs/design/` once that directory exists. Until then, add a section to the closest numbered doc or tracker item.

The design should include:

- proposed API or module boundary;
- data model changes;
- algorithms or state machines;
- failure behavior;
- testing strategy;
- migration plan;
- open questions.

### 4. Discussion and Finalization

Do not begin major implementation until the approach is accepted.

Accepted designs should state:

- selected approach;
- rejected alternatives;
- required tests;
- documentation updates;
- rollout order.

### 5. Tracker Update

Update `docs/14-EXECUTION-TRACKER.md` with:

- status;
- owner;
- scope;
- acceptance gates;
- links to design/ADR/docs.

### 6. Implementation

Implementation should be sliced so each change is reviewable and testable. Avoid mixing architecture, formatting, refactoring, and feature work unless the coupling is unavoidable.

### 7. Verification

Run the relevant local checks and ensure the future CI gate is represented in `docs/15-CI-AND-RELEASE-GATES.md`.

### 8. Documentation

Update docs in the same change when behavior, API, workflow, compatibility, or decisions change.

## ADR Triggers

Create or update an ADR for:

- public API boundaries;
- crate/module boundaries;
- serialization formats;
- transaction semantics;
- layout units or algorithms;
- renderer backends;
- parser/security policy;
- dependency choices with long-term impact;
- collaboration operation model;
- plugin trust or sandbox model.
