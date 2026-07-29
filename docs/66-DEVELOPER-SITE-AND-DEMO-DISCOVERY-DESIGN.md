# Developer Site and Demo Discovery Design

**Status:** Implemented
**Owner:** Codex
**Scope:** GitHub Pages entry point, live sample routing, README visual, search
and LLM discovery surfaces

## Outcome

`opendoc.casualoffice.org` becomes a developer-facing product and project entry
point instead of opening directly into an unexplained test harness:

- `/` explains the engine, its host boundary, current capabilities, and honest
  pre-release limits;
- `/editor.html` remains the complete browser editor;
- `/editor.html?demo=1` opens a small redistribution-safe repository fixture
  without requiring a file picker;
- the README shows the current editor shell and sends readers to the live site.

The site targets developers searching for a Rust DOCX engine, WebAssembly
document editor, embeddable DOCX SDK, deterministic document layout, or
loss-aware OOXML handling. It does not market unsupported Word-grade fidelity or
a stable SDK.

The roadmap story is explicit: present work is centered on DOCX fidelity; stable
SDK packaging, ODT/plain-text and other format adapters, and native PDF export
from the engine display list are future goals. They are labeled as directions,
not shipped capabilities.

## Selected structure

Keep the deployable unit as static files under `webapp/`. The existing editor
document moves from `index.html` to `editor.html`; its CSS, JavaScript, WASM
package, and host policy remain unchanged. A new semantic `index.html` and
`src/marketing.css` form the landing page with no framework or build-time
JavaScript dependency.

The build copies two repository-owned assets into ignored deploy outputs:

- `fixtures/corpus/real-producer-rich.docx` → `webapp/demo.docx`;
- `docs/assets/editor.jpg` → `webapp/assets/editor.jpg`.

The fixture manifest records the sample as Apache-2.0 and repository-generated.
The editor fetches it only for the explicit `?demo=1` route. Normal editor loads
perform no document fetch and continue to accept local files without uploading
them.

Rejected alternatives:

- a second hosting provider, because the requested production surface is the
  existing GitHub Pages/custom-domain deployment;
- a framework migration, because static HTML/CSS is sufficient and would add
  dependency/build risk unrelated to the document runtime;
- committing the demo document twice, because the corpus fixture remains the
  single source and the build can stage the deploy copy deterministically;
- moving the editor into a nested directory, because a sibling `editor.html`
  preserves all existing relative asset URLs and keeps rollback simple.

## Discovery contract

The landing page carries:

- a unique title and developer-intent meta description;
- canonical URL, Open Graph, and Twitter card metadata;
- `SoftwareSourceCode` and `WebApplication` JSON-LD with the Apache-2.0 license,
  Rust/WebAssembly runtime, repository, and live demo;
- semantic headings and crawlable technical copy rather than client-rendered
  content;
- `robots.txt` and `sitemap.xml` for the custom domain;
- `llms.txt` with a concise capability/limitation summary and durable project,
  architecture, demo, and source links.

GitHub Pages must serve the custom `CNAME` file and both discovery files from the
artifact root. Indexing remains controlled by search engines; deployment can
make the pages eligible and understandable but cannot guarantee ranking or a
particular crawl time.

## Interaction and safety

“Try the demo” opens the known local sample in the real editor. “Open your DOCX”
opens the editor without selecting or uploading a file. The editor continues to
own the statement “nothing is uploaded” because file bytes remain inside the
browser/WASM runtime. External fonts retain the documented host network policy;
the landing page must not imply that the editor makes no network requests.

Marketing claims follow the execution tracker and support matrix. Pre-release
status and the lack of pixel-perfect Word fidelity remain visible. The primary
actions are keyboard accessible, motion respects `prefers-reduced-motion`, and
the page remains usable without JavaScript.

## Acceptance gates

- root, editor, and demo routes load from one static Pages artifact;
- demo loading opens the expected repository fixture and still permits local
  Open/Save;
- root metadata, JSON-LD, canonical, robots, sitemap, and `llms.txt` validate
  syntactically and use the production custom domain;
- the landing page is responsive at narrow/mobile and desktop viewports;
- the README screenshot is current and its link resolves to the live sample;
- a headless-browser smoke verifies landing CTAs, demo open, outline width, and
  hyperlink/TOC chip behavior with no unexpected console errors.

## Rollout and rollback

The Pages workflow continues to deploy `webapp/` and now calls the repository
build script so WASM and staged assets cannot drift. Rolling back the landing
page requires restoring the editor as `index.html`; the runtime API and document
model do not change.

## Verification

Implemented and deployed on 2026-07-29. GitHub Pages workflow run
`30403402189` built and deployed commit `2a01853` successfully. A production
browser smoke verified the root and demo routes at desktop and mobile
viewports, automatic sample loading, the production WASM resources, no
horizontal overflow, and no console errors. `robots.txt`, `sitemap.xml`, and
`llms.txt` each returned HTTP 200 from the custom domain.

## 2026-07-30 visual-system refinement

The landing page now uses the editor's light paper/canvas visual language
instead of the earlier dark-glass treatment: graphite text, neutral grey
canvas, white bounded surfaces, one orange accent, ruler motifs, and dark
surfaces only where they communicate engine/code content. The information
architecture, capability claims, routes, metadata, and host-policy boundary
are unchanged. The editor preview asset was refreshed from the completed live
shell so the site no longer advertises stale chrome. Both site and editor use
the same self-hosted Inter face; the font asset and OFL text are checked in, so
the visual alignment adds no runtime Google Fonts dependency.

Permanent Playwright coverage in
`webapp/tests/e2e/site-visual-refresh.spec.mjs` checks the editor/demo routes,
preview load, console errors, and narrow-viewport overflow. The completed
refinement passed `webapp/build.sh`, 15 frontend unit tests, and the complete
28-test Playwright suite.
