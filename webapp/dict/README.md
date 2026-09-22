# Spelling dictionaries

Generated files. **Do not edit them by hand** — `webapp/tests/dictionary_artifact.test.mjs`
re-emits each one from its own contents and fails the build if the bytes differ, which a
hand edit always makes them.

To regenerate, from `webapp/`:

```sh
curl -LO https://downloads.sourceforge.net/wordlist/scowl-2020.12.07.tar.gz
shasum -a 256 scowl-2020.12.07.tar.gz   # must print the digest below
tar xzf scowl-2020.12.07.tar.gz
node tools/build-dictionary.mjs --scowl scowl-2020.12.07            # write
node tools/build-dictionary.mjs --scowl scowl-2020.12.07 --check    # verify only
```

## Provenance

| | |
| --- | --- |
| Release | `scowl-2020.12.07` |
| URL | `https://downloads.sourceforge.net/wordlist/scowl-2020.12.07.tar.gz` |
| Size | 2,569,810 bytes |
| SHA-256 | `5587667caa20c4891390c2d42dbb4d5c4c3f41bee77af1457ece3ba23fb859cc` |
| Level | ≤ 60 — the level the official Hunspell `en_US` dictionary uses |
| Common tier | ≤ 35 |
| Files used | `english-{words,upper,contractions,proper-names}` plus the dialect's own; see `LANGUAGE_SETS` in the generator |
| Filters | possessive `'s` forms dropped (handled by rule at check time) |

Note that a **git checkout of `en-wl/wordlist` does not contain `final/`** — those files
are produced by SCOWL's Makefile. The release tarball is the artifact.

## Licence

SCOWL is attribution-only and permissive; the full notice is in
[`LICENSE.SCOWL.txt`](LICENSE.SCOWL.txt), verbatim and unmodified. The clause that makes a
**generated** list redistributable under Apache-2.0 is this one:

> Permission to use, copy, modify, distribute and sell these word lists, the associated
> scripts, **the output created from the scripts**, and its documentation for any purpose
> is hereby granted without fee, provided that the above copyright notice appears in all
> copies and that both that copyright notice and this permission notice appear in
> supporting documentation.

`docs/114` §2.2 works through why that is sufficient, and why taking the same data from a
LibreOffice/Hunspell bundle instead would not be.

## Format

```
<common-tier words, one per line, codepoint-sorted>
---
<the rest, one per line, codepoint-sorted>
```

One trailing newline, no BOM, no header. The two tiers are disjoint. `parseDictionary` in
`webapp/src/spelling.mjs` is the only reader; the common tier is what makes a suggestion
for `teh` rank `the` above `ten`.
