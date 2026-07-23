# Governance

OpenDoc is maintained under the CasualOffice GitHub organization.

## Roles

**Maintainers** own repository administration, releases, security response, and
final compatibility decisions.

**Subsystem owners** review changes in their documented area and maintain its
design, tests, fixtures, and tracker state.

**Contributors** may propose designs and changes through the documented
contribution process.

Named maintainers and subsystem owners will be recorded before the first public
preview. Until then, repository write access is the authoritative maintainer
signal.

## Decision Process

Substantial decisions follow the design-first process:

1. define the required outcome and constraints;
2. record research and alternatives;
3. publish a design note or ADR;
4. discuss and resolve objections;
5. mark the decision accepted;
6. update the tracker;
7. implement and verify.

Maintainers seek technical consensus. When consensus is not available, the
maintainer responsible for the affected compatibility boundary records the
decision and consequences in an ADR.

## Protected Areas

Public APIs, normalized/operation schemas, parser/security policy, unsafe code,
rendering backends, DOCX preservation, release automation, and cryptographic
behavior require maintainer review.

## Releases

Releases require all gates in `docs/15-CI-AND-RELEASE-GATES.md`, an updated
changelog and tracker, compatibility notes, and reproducible artifacts. No
single contributor may silently weaken a release gate to publish.
