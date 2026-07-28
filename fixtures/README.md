# OpenDoc Fixture Corpus

This directory contains repository-owned synthetic fixtures. Do not copy
documents from the sibling study repository, customer data, or third-party
vendor suites into this directory without the rights review required by
`docs/23-DOCX-FIXTURE-CORPUS.md`.

Regenerate the package fixtures from the repository root:

```sh
cargo run -p casual-doc-ooxml --example generate_fixtures --locked
```

After regeneration, verify every SHA-256 value in `manifest.json`. Fixture
changes require a behavior/design explanation and review; checksums are not
updated merely to make tests pass.

`generated/visual-containment.docx` also carries a deterministic render
baseline. Run its collision and five-page RGBA check with:

```sh
cargo test -p casual-doc-render --test visual_containment --locked
```

The baseline always uses the bundled font source. Do not regenerate its hash
with system fonts or browser-loaded web fonts enabled.
