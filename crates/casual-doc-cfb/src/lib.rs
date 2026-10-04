// SPDX-License-Identifier: Apache-2.0

//! Bounded MS-CFB compound-file **recognition**.
//!
//! An ECMA-376 document encrypted per [MS-OFFCRYPTO] is not a ZIP at all: it is
//! an OLE compound file ([MS-CFB]) whose streams include `\EncryptionInfo` and
//! `\EncryptedPackage`, and whose extension is still `.docx`. So recognising one
//! needs a container reader *before* any cryptography, and detecting one needs
//! magic bytes, because the extension and the MIME type both lie by construction
//! (`docs/163` §2.1).
//!
//! This crate is that container, and nothing else. It holds no cryptography, no
//! XML, and no document model, and it has **no dependencies at all**.
//!
//! # Scope, stated plainly
//!
//! This is phase 0 of `docs/163` §11: enough of [MS-CFB] to decide *what a file
//! is* and name it. [`has_signature`] answers the cheap question in eight byte
//! comparisons, and [`classify`] walks the header, the FAT and the directory
//! under explicit [`CfbLimits`] to report which well-known streams are present.
//!
//! It does **not** read stream contents, follow the mini-FAT, write compound
//! files, or decrypt anything. A reader is phase 1 and a writer phase 2. Nothing
//! here supports opening an encrypted document; it supports *saying* that a file
//! is one.
//!
//! # Why this is a separate crate
//!
//! `casual-doc-package` is documented as the ZIP substrate and its value is
//! being one small, fuzzed, limit-enforcing admission boundary. A compound file
//! is a second container with a different threat surface — a sector FAT is a
//! linked list, so cycles and truncation are the hazards, not path traversal —
//! and two containers in one crate dilutes both. `casual-doc-ooxml` is the OPC
//! profile, and CFB is also how legacy `.doc` and IRM files are shaped, so the
//! container is not OOXML-specific either (`docs/163` §7.1).
//!
//! # Why no `cfb` crate
//!
//! `docs/163` §9.3 recommends the third-party `cfb` crate for phase 1's full
//! reader, as a bounded wrapper in the relationship `casual-doc-package` has to
//! `zip`. Phase 0 does not need it: recognition needs the header, one FAT chain
//! and a flat directory scan, which is the code in this crate, and adding a
//! dependency to avoid 300 lines would buy a dependency-policy argument, a
//! lockfile entry, an MSRV to track and — per §9.3's own measurement — a
//! target-dependent `set_modified_time` signature to abstract away. When phase 1
//! needs streams, the FAT, the mini-FAT and a writer, that trade changes.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod container;
mod error;
mod limits;

#[cfg(test)]
mod tests;

pub use container::{CfbKind, SIGNATURE, classify, has_signature};
pub use error::CfbError;
pub use limits::CfbLimits;
