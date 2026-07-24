# DOCX Engine Competitor Research

**Status:** Complete
**Completed:** 2026-07-24
**Scope:** Source-architecture study
**Tracker:** P1A-002
**Decision output:** `34-OOXML-FIDELITY-ARCHITECTURE.md`

## Purpose

This study examines how established open-source document systems cross the
OOXML boundary. It is not a feature comparison and does not treat source code
availability as permission to copy implementation. The questions are:

1. Do mature editors use raw OOXML as their live editing model?
2. Where do they preserve source information that their semantic model does not
   fully represent?
3. How are import, editing, export, and compatibility tests separated?
4. What should OpenDoc design before semantic DOCX import begins?

The earlier product-level comparison remains in
`12-COMPETITIVE-ANALYSIS.md`.

## Method

The review used official repositories and specification pages at pinned
revisions. Paths and behavior below describe those revisions, not an evergreen
claim about each project's latest branch.

| Project | Revision reviewed | Role in study | License observed |
| --- | --- | --- | --- |
| LibreOffice core | [`bdb27b2`](https://github.com/LibreOffice/core/commit/bdb27b21bfe44a3a0bbc7df115fd91dffde30cd4) | Full editor, OOXML import/export, layout and compatibility tests | MPL-2.0/LGPL-3.0-or-later project policy; file-specific notices apply |
| ONLYOFFICE core | [`3250a84`](https://github.com/ONLYOFFICE/core/commit/3250a848ee4ef20c2fb8c38dc86350ec579124b8) | OOXML object model and converter | AGPL-3.0-only source headers |
| ONLYOFFICE sdkjs | [`72b0421`](https://github.com/ONLYOFFICE/sdkjs/commit/72b0421c0bbf9d01eed9cf14834ae47eb2df1b50) | Live editor model, pagination and interaction state | AGPL-3.0-only source headers |
| Open XML SDK | [`cd2b359`](https://github.com/dotnet/Open-XML-SDK/commit/cd2b359ef824737edb93f1c6157c19551aae1e52) | Typed OPC/OOXML library control | MIT |
| Apache POI | [`0d6d487`](https://github.com/apache/poi/commit/0d6d4872c491b1f230f51c6878e57407c60ae697) | Typed OOXML library control | Apache-2.0 |

No implementation code, generated schemas, tests, or fixtures were copied.
Research notes record architectural facts and independently derived design
requirements only. Any future implementation must be original and reviewed
under OpenDoc's Apache-2.0 contribution policy.

License references:

- [LibreOffice core licensing](https://github.com/LibreOffice/core/blob/bdb27b21bfe44a3a0bbc7df115fd91dffde30cd4/COPYING);
- [ONLYOFFICE core license](https://github.com/ONLYOFFICE/core/blob/3250a848ee4ef20c2fb8c38dc86350ec579124b8/LICENSE);
- [Open XML SDK license](https://github.com/dotnet/Open-XML-SDK/blob/cd2b359ef824737edb93f1c6157c19551aae1e52/LICENSE);
- [Apache POI legal information](https://poi.apache.org/legal.html).

Collabora Online was not treated as a separate DOCX engine in this pass because
its document core is LibreOffice technology. Its deployment and WOPI product
boundary remain covered by the product-level study.

## LibreOffice

### Import boundary

LibreOffice's Writer DOCX importer does not expose WordprocessingML as the live
Writer document. `WriterFilter.cxx` creates an OOXML stream and document,
constructs a `DomainMapper` for the Writer model, and resolves the OOXML stream
through that mapper.

The mapper layer is feature-oriented. Its directory contains handlers for
styles, themes, numbering, sections, tables, graphics, fields, tracked changes,
and related document concepts. This is a source-shaped decoding layer feeding a
separate editor model.

Evidence:

- [`WriterFilter.cxx`](https://github.com/LibreOffice/core/blob/bdb27b21bfe44a3a0bbc7df115fd91dffde30cd4/sw/source/writerfilter/filter/WriterFilter.cxx)
- [`DomainMapper.hxx`](https://github.com/LibreOffice/core/blob/bdb27b21bfe44a3a0bbc7df115fd91dffde30cd4/sw/source/writerfilter/dmapper/DomainMapper.hxx)
- [`writerfilter/dmapper`](https://github.com/LibreOffice/core/tree/bdb27b21bfe44a3a0bbc7df115fd91dffde30cd4/sw/source/writerfilter/dmapper)

### Export boundary

DOCX output is a separate subsystem. `DocxExport` and
`DocxAttributeOutput` traverse Writer state and generate package parts and
WordprocessingML. Import and export therefore share compatibility knowledge but
are not one mutable OOXML tree.

Evidence:

- [`docxexport.cxx`](https://github.com/LibreOffice/core/blob/bdb27b21bfe44a3a0bbc7df115fd91dffde30cd4/sw/source/filter/ww8/docxexport.cxx)
- [`docxattributeoutput.cxx`](https://github.com/LibreOffice/core/blob/bdb27b21bfe44a3a0bbc7df115fd91dffde30cd4/sw/source/filter/ww8/docxattributeoutput.cxx)

### Interoperability preservation

LibreOffice supplements its semantic model with interoperability "grab bags."
The public document API describes `InteropGrabBag` as storage for properties
needed to preserve interoperability information. Writer defines grab-bag
properties at document, paragraph, character, style, frame, cell, row, and
table scopes. Import paths retain items such as theme DOM and embedding
information, and export paths consume preserved data where supported.

This is evidence for scoped source preservation. It does not prove that every
unsupported construct round-trips or that arbitrary XML remains byte-identical.

Evidence:

- [`OfficeDocument.idl`](https://github.com/LibreOffice/core/blob/bdb27b21bfe44a3a0bbc7df115fd91dffde30cd4/offapi/com/sun/star/document/OfficeDocument.idl)
- [`unoprnms.hxx`](https://github.com/LibreOffice/core/blob/bdb27b21bfe44a3a0bbc7df115fd91dffde30cd4/sw/inc/unoprnms.hxx)
- [`docxtablestyleexport.cxx`](https://github.com/LibreOffice/core/blob/bdb27b21bfe44a3a0bbc7df115fd91dffde30cd4/sw/source/filter/ww8/docxtablestyleexport.cxx)

### Test separation

LibreOffice keeps distinct Writer suites for OOXML import, OOXML export/reload,
and layout behavior. The source tree contains hundreds of fixtures and tests;
the export fixture listing alone reaches GitHub's 1,000-entry contents limit.
This organization matters more than the count:

- import assertions inspect semantic model state and selected layout effects;
- export assertions save, reload, and inspect generated package XML;
- layout tests inspect pagination and geometry independently.

Evidence:

- [`sw/qa/extras/ooxmlimport`](https://github.com/LibreOffice/core/tree/bdb27b21bfe44a3a0bbc7df115fd91dffde30cd4/sw/qa/extras/ooxmlimport)
- [`sw/qa/extras/ooxmlexport`](https://github.com/LibreOffice/core/tree/bdb27b21bfe44a3a0bbc7df115fd91dffde30cd4/sw/qa/extras/ooxmlexport)
- [`sw/qa/extras/layout`](https://github.com/LibreOffice/core/tree/bdb27b21bfe44a3a0bbc7df115fd91dffde30cd4/sw/qa/extras/layout)

## ONLYOFFICE

### System boundary

ONLYOFFICE Document Server separates conversion, the browser editor, and the
application shell. Its documented deployment exchanges editing data through
client and server services; this topology is a product choice, not a
requirement OpenDoc should inherit.

Evidence:

- [Document Server repository](https://github.com/ONLYOFFICE/DocumentServer)
- [ONLYOFFICE Docs API architecture](https://api.onlyoffice.com/docs/docs-api/get-started/how-it-works/)

### OOXML and conversion layers

The core repository has a typed DOCX-format layer with package parts for the
document, styles, numbering, relationships, media, settings, comments, custom
XML, and other OOXML structures.

The converter's `docx.h` explicitly bridges a DOCX directory and an internal
`doct`/Editor.bin representation through `CDocxSerializer`. Feature-specific
binary readers and writers handle document structures, styles, numbering,
relationships, and media.

Evidence:

- [`OOXML/DocxFormat`](https://github.com/ONLYOFFICE/core/tree/3250a848ee4ef20c2fb8c38dc86350ec579124b8/OOXML/DocxFormat)
- [`X2tConverter/src/lib/docx.h`](https://github.com/ONLYOFFICE/core/blob/3250a848ee4ef20c2fb8c38dc86350ec579124b8/X2tConverter/src/lib/docx.h)
- [`OOXML/Binary/Document`](https://github.com/ONLYOFFICE/core/tree/3250a848ee4ef20c2fb8c38dc86350ec579124b8/OOXML/Binary/Document)

### Live editor model

The sdkjs word editor has its own `CDocument` and feature modules for
paragraphs, runs, tables, styles, numbering, sections, history, actions,
recalculation, and pages. This is the interactive source of truth after
conversion; it is not the package's raw XML tree.

Evidence:

- [`word/Editor/Document.js`](https://github.com/ONLYOFFICE/sdkjs/blob/72b0421c0bbf9d01eed9cf14834ae47eb2df1b50/word/Editor/Document.js)
- [`word/Editor`](https://github.com/ONLYOFFICE/sdkjs/tree/72b0421c0bbf9d01eed9cf14834ae47eb2df1b50/word/Editor)

### Source retention

The reviewed DOCX converter includes a conditional `needConvertToOrigin` path
that can copy the original OOXML package to `origin.docx`. This demonstrates a
source-retention mechanism. The inspected code alone does not establish when
it is selected, how export reconciles it with edits, or which features achieve
lossless round-trip behavior.

Evidence:

- [`X2tConverter/src/lib/docx.h`](https://github.com/ONLYOFFICE/core/blob/3250a848ee4ef20c2fb8c38dc86350ec579124b8/X2tConverter/src/lib/docx.h)

## Typed OOXML Library Controls

### Microsoft Open XML SDK

The Open XML SDK provides an OPC package abstraction, generated typed elements,
and LINQ-style XML access. Its own README states that it is not a high-level
abstraction for productivity documents. It is useful evidence that a complete
source-shaped OOXML layer and a WYSIWYG editor model solve different problems.

Evidence:

- [Open XML SDK README](https://github.com/dotnet/Open-XML-SDK/blob/cd2b359ef824737edb93f1c6157c19551aae1e52/README.md)

### Apache POI XWPF

POI's `XWPFDocument` builds an object graph over OPC parts while retaining
schema-derived `CT*` objects. The official XWPF guide warns that the high-level
API is incomplete and some operations require low-level XMLBeans access. This
is a practical example of the maintenance cost when semantic convenience and
source-shaped coverage share one public object model.

Evidence:

- [XWPF guide](https://poi.apache.org/components/document/quick-guide-xwpf.html)
- [`XWPFDocument.java`](https://github.com/apache/poi/blob/0d6d4872c491b1f230f51c6878e57407c60ae697/poi-ooxml/src/main/java/org/apache/poi/xwpf/usermodel/XWPFDocument.java)

## Comparative Matrix

| System | Source-shaped OOXML layer | Separate live editor model | Separate export path | Explicit preservation evidence | Layout engine |
| --- | --- | --- | --- | --- | --- |
| LibreOffice Writer | Yes | Yes | Yes | Scoped interoperability grab bags | Yes |
| ONLYOFFICE | Yes | Yes, internal binary plus sdkjs model | Yes | Original-package retention path | Yes |
| Open XML SDK | Yes | No editor model | Package/XML mutation APIs | Source tree itself | No |
| Apache POI XWPF | Yes | High-level wrapper over schema objects, not a WYSIWYG engine | Package/XML mutation APIs | Schema objects remain accessible | No |

## Findings

### 1. Raw OOXML is not a sufficient live editing model

The full editors map source-shaped OOXML into models organized around document
semantics, editing, history, pagination, and interaction. Typed OOXML libraries
provide broad package access but do not supply those editor behaviors.

OpenDoc should keep a normalized model as its live runtime source of truth.
Making the XML DOM or generated OOXML classes the editor model would couple
commands, selection, layout, and public APIs to interchange syntax.

### 2. Normalization alone is not a fidelity strategy

A semantic model deliberately removes aliases, inheritance forms, ordering
details, extension markup, alternate content, and producer-specific choices.
Those details may be required to write a compatible document later.

OpenDoc therefore needs bounded source provenance and preservation data beside
the normalized model. A generic JSON extension bag is not enough because it
cannot define ownership, ordering, edit invalidation, conflicts, or export
disposition.

### 3. Import and export must be designed together

Phase 1A does not implement a writer, but every imported feature needs a
declared reverse-mapping strategy before its normalized representation is
accepted. Otherwise the importer can discard distinctions that Phase 2 cannot
recover.

### 4. Fidelity needs separate test oracles

One "round-trip passed" result hides different failures. OpenDoc needs
independent:

- package and relationship validation;
- semantic snapshot assertions;
- compatibility and preservation-ledger assertions;
- generated OOXML assertions when writing exists;
- save/reopen semantic assertions;
- fixed-font layout and visual assertions when layout/rendering exist.

### 5. Preservation must be scoped and typed

Preserved content needs a source part, semantic owner, stable anchor, namespace,
order, byte budget, edit-invalidation rule, conflict policy, and planned save
disposition. Whole-package retention can support inspection or exact no-op
return, but cannot by itself define edited saves.

### 6. ECMA-376 is necessary but not sufficient

The implementation baseline includes OPC, WordprocessingML, markup
compatibility, and extensibility from ECMA-376. Real Microsoft-produced DOCX
files also require review against Microsoft's current DOCX and ISO/IEC 29500
implementation notes. Producer quirks must be fixture-backed rather than
encoded from assumptions.

Specification references:

- [ECMA-376](https://ecma-international.org/publications-and-standards/standards/ecma-376/)
- [Microsoft MS-DOCX](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-docx/b839fe1f-e1ca-4fa6-8c26-5954d0abbccd)

## What This Pass Does Not Prove

- It does not rank visual fidelity or performance.
- It does not establish feature-by-feature round-trip success.
- It does not claim byte-identical output from any competitor.
- It does not test malformed-input behavior or resource limits.
- It does not select OpenDoc's XML parser or schema-generation strategy.
- It does not make competitor fixtures safe to redistribute.

## Required Measured Follow-up

Pass 3 must use rights-reviewed fixtures and record tool versions, input hashes,
commands, outputs, and known environment limitations. At minimum:

1. no-edit open/save package and XML diffs;
2. targeted edits near supported and unsupported content;
3. alternate-content and unknown-namespace retention;
4. relationships, media, custom XML, fields, tracked changes, and section cases;
5. save/reopen semantic comparison;
6. fixed-font page, line, and visual comparison when Phase 1D exists;
7. malformed package and external-relationship handling.

The output must distinguish observed behavior from source-code inference and
must not add unlicensed documents to the repository.

## OpenDoc Design Consequence

The accepted direction should be a dual-representation import result:

- normalized semantic model for commands, selection, layout, and SDK use;
- immutable bounded source snapshot, provenance, and typed preservation ledger
  for compatibility analysis and future save planning.

The proposed contract, alternatives, and acceptance decisions are documented in
`34-OOXML-FIDELITY-ARCHITECTURE.md`. No Phase 1A parser implementation begins
until that design and normalized schema v1 are accepted.
