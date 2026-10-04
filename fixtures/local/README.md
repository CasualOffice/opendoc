# Local deck corpus — never committed

Drop real `.pptx`, `.potx`, `.ppsx` and `.odp` files here. Everything in this
directory is gitignored.

## Why this directory exists, and why it is not `fixtures/corpus/`

`fixtures/corpus/` is the COMMITTED corpus, and two rules govern it that a real
file usually cannot satisfy:

1. **Licence.** Every entry in `fixtures/manifest.json` declares a `license`, and
   the repository is Apache-2.0. A deck somebody downloaded carries its author's
   copyright in its text, its images and often its embedded fonts. It is not ours
   to redistribute, and a fixture nobody can relicense is a fixture nobody can
   fork the project with.
2. **Size.** The largest committed fixture is 64 KB. A real presentation is
   mostly pictures — the first deck tested against this engine was 9.3 MB, of
   which **92% was media and only 0.40 MB was the XML that exercises any code at
   all**. A binary that large is in the clone forever, for every contributor,
   and buys 0.40 MB of signal.

So real decks live here, untracked, and what gets committed is a **synthetic
fixture reproducing the STRUCTURE** a real file revealed: the elements, the
attribute spellings, the shapes of the loss. That is the same discipline
`fixtures/corpus/`'s synthetic DOCX entries already follow, and it is why a
fixture can be checksummed, reviewed and relicensed.

## How this directory is used

`cargo run -p casual-pres-import --example probe_real -- <file>` imports a deck
and prints its fidelity report and per-slide geometry, which is how a real file
turns into a structural finding worth building a fixture around.

A real file reveals what to build; it is never what a guard asserts against,
because a guard that depends on an untracked file fails for everyone else.
