// SPDX-License-Identifier: Apache-2.0

//! Opening a damaged package: a rebuilt ZIP directory, and inferred OPC
//! plumbing.
//!
//! # Why the plumbing is the easy half
//!
//! `35-DISPOSITION-TAXONOMY.md` already places the OPC plumbing —
//! `[Content_Types].xml` and `_rels/*` — **outside** the disposition taxonomy,
//! because it "is regenerated deterministically from the model. Not data." That
//! sentence was written about writing, and it settles reading too: if the engine
//! can regenerate the manifest from the model, it can infer the manifest from the
//! parts. A package whose manifest is missing is not a package with missing
//! content; it is a package with a missing index, and refusing it over the index
//! discards content that is entirely intact.
//!
//! So every OPC-level refusal becomes a repair with a report, and the only
//! genuinely unopenable cases left are the ones where there is no document to
//! find at all.
//!
//! # The ZIP directory
//!
//! A truncated `.docx` — the overwhelmingly common real-world damage, from an
//! interrupted download, a full disk, or a mail gateway — has lost its central
//! directory, which lives at the *end* of the file. Every byte of every part
//! before the cut is still there, and each is still introduced by its own local
//! file header.
//!
//! The known pattern is **file carving**, and `zip -FF` is its reference
//! implementation: scan for local headers, and rebuild the directory from what
//! they say. [`repair_archive`] does exactly that and nothing more — it does not
//! decompress, re-compress, or move a single byte of part data, so a repaired
//! archive's parts are bit-identical to the original's. That matters: it means the
//! repair cannot corrupt a part, only fail to find one.
//!
//! One mechanism, deliberately: the repaired bytes go back through the same
//! [`crate::DocxPackage::open`] path, with the same limits and the same checks. A
//! second, laxer archive reader would be a second set of bounds to get wrong.
//!
//! # Bounds
//!
//! The scan is one forward pass with a strictly increasing cursor, so it is
//! `O(bytes)` and cannot loop. Output is bounded by the input plus 46 bytes and a
//! name per recovered entry. Encrypted entries, entries using a compression
//! method outside the DOCX profile, and the entry whose data the truncation cut
//! are not recovered — a salvaged entry is one whose bytes are all present.

const LOCAL_FILE_SIGNATURE: &[u8; 4] = b"PK\x03\x04";
const CENTRAL_FILE_SIGNATURE: &[u8; 4] = b"PK\x01\x02";
const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
const DATA_DESCRIPTOR_SIGNATURE: &[u8; 4] = b"PK\x07\x08";

/// Fixed size of a ZIP local file header, before its name and extra field.
const LOCAL_HEADER_BYTES: usize = 30;

/// What opening a damaged package had to repair before a document could be read.
///
/// Each variant is reported, never applied silently: a reader who is handed a
/// document out of a file whose index was rebuilt needs to know that parts after
/// the damage may simply not be there.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageRepair {
    /// The ZIP directory at the end of the file was missing or unusable, and the
    /// entry list was rebuilt by scanning for part headers.
    ArchiveDirectoryRebuilt {
        /// Entries recovered by the scan.
        entries: u32,
    },
    /// `[Content_Types].xml` was missing, and part types were inferred from
    /// their extensions.
    ContentTypeManifestMissing,
    /// `[Content_Types].xml` was present but unreadable, and part types were
    /// inferred from their extensions.
    ContentTypeManifestUnreadable,
    /// `_rels/.rels` was missing, so nothing in the file said which part is the
    /// document.
    PackageRelationshipsMissing,
    /// `_rels/.rels` was present but unreadable.
    PackageRelationshipsUnreadable,
    /// No usable `officeDocument` relationship resolved, so the main document
    /// was located by name.
    MainDocumentLocatedByName {
        /// The part that was used as the main document.
        part: String,
    },
    /// The main document's declared content type was absent or not a
    /// WordprocessingML type, and was ignored.
    MainDocumentContentTypeIgnored {
        /// The declared type, when the manifest declared one.
        declared: Option<String>,
    },
    /// More than one `officeDocument` relationship was present; the first in
    /// relationship order was used.
    MainDocumentAmbiguous {
        /// The part that was used as the main document.
        part: String,
    },
    /// A part's own relationships were unreadable, so references out of that part
    /// (its images, its hyperlinks) could not be resolved.
    PartRelationshipsUnreadable {
        /// The part whose relationships could not be read.
        part: String,
    },
}

impl PackageRepair {
    /// A stable token a host can key a translation off.
    #[must_use]
    pub const fn token(&self) -> &'static str {
        match self {
            Self::ArchiveDirectoryRebuilt { .. } => "archive-directory-rebuilt",
            Self::ContentTypeManifestMissing => "content-type-manifest-missing",
            Self::ContentTypeManifestUnreadable => "content-type-manifest-unreadable",
            Self::PackageRelationshipsMissing => "package-relationships-missing",
            Self::PackageRelationshipsUnreadable => "package-relationships-unreadable",
            Self::MainDocumentLocatedByName { .. } => "main-document-located-by-name",
            Self::MainDocumentContentTypeIgnored { .. } => "main-document-content-type-ignored",
            Self::MainDocumentAmbiguous { .. } => "main-document-ambiguous",
            Self::PartRelationshipsUnreadable { .. } => "part-relationships-unreadable",
        }
    }

    /// One sentence a reader can act on, with no internal vocabulary in it.
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::ArchiveDirectoryRebuilt { entries } => format!(
                "The index at the end of this file is damaged, so its contents were found by \
                 scanning it; {entries} parts were recovered. Anything after the damage is \
                 not in this document."
            ),
            Self::ContentTypeManifestMissing => {
                "This file is missing its list of content types. Each part's type was worked \
                 out from its name instead."
                    .to_owned()
            }
            Self::ContentTypeManifestUnreadable => {
                "This file's list of content types is damaged. Each part's type was worked \
                 out from its name instead."
                    .to_owned()
            }
            Self::PackageRelationshipsMissing => {
                "This file is missing the index that says which part holds the document.".to_owned()
            }
            Self::PackageRelationshipsUnreadable => {
                "The index that says which part holds the document is damaged.".to_owned()
            }
            Self::MainDocumentLocatedByName { part } => format!(
                "Nothing in this file points at the document text, so it was located by its \
                 usual name ({part})."
            ),
            Self::MainDocumentContentTypeIgnored { declared } => match declared {
                Some(declared) => format!(
                    "This file says its main part is \u{201c}{declared}\u{201d} rather than a \
                     Word document. It was opened as a Word document anyway."
                ),
                None => "This file does not say that its main part is a Word document. It was \
                         opened as one anyway."
                    .to_owned(),
            },
            Self::MainDocumentAmbiguous { part } => {
                format!("This file names more than one main document. The first ({part}) was used.")
            }
            Self::PartRelationshipsUnreadable { part } => format!(
                "The list of what {part} refers to is damaged, so images and links inside it \
                 are not shown."
            ),
        }
    }
}

/// Rebuilds a usable ZIP container from `bytes` by scanning for local file
/// headers, or returns `None` when there is nothing to salvage.
///
/// The result is the recovered entries' original bytes, verbatim, followed by a
/// freshly written central directory and end-of-directory record. It is meant to
/// be handed straight back to [`crate::DocxPackage::open`], which applies every
/// bound and check it normally would — this function widens what can be *found*,
/// never what is *admitted*.
///
/// `None` means no entry could be salvaged: either the bytes hold no local file
/// header at all (not a ZIP), or the first header's own data is already past the
/// end of the file.
#[must_use]
pub fn repair_archive(bytes: &[u8]) -> Option<Vec<u8>> {
    let first = find(bytes, LOCAL_FILE_SIGNATURE, 0)?;
    let source = &bytes[first..];
    let mut out: Vec<u8> = Vec::with_capacity(source.len() + 1024);
    let mut central: Vec<u8> = Vec::new();
    let mut entries = 0_u16;
    let mut cursor = 0_usize;

    while let Some(at) = find(source, LOCAL_FILE_SIGNATURE, cursor) {
        let Some(entry) = read_local_entry(source, at) else {
            break;
        };
        cursor = entry.next;
        if !entry.admissible {
            continue;
        }
        let local_offset = u32::try_from(out.len()).ok()?;
        let header_start = out.len();
        out.extend_from_slice(&source[at..entry.data_end]);
        // An entry written with a trailing data descriptor carries zeroes in its
        // own header. The descriptor is dropped (its bytes are not copied), so
        // the flag that promises one must be cleared and the real figures written
        // into the copied header — otherwise a reader waits for a descriptor that
        // is no longer there.
        if entry.had_descriptor {
            write_u16(&mut out, header_start + 6, entry.flags & !8);
            write_u32(&mut out, header_start + 14, entry.crc);
            write_u32(&mut out, header_start + 18, entry.compressed);
            write_u32(&mut out, header_start + 22, entry.uncompressed);
        }
        central.extend_from_slice(CENTRAL_FILE_SIGNATURE);
        push_u16(&mut central, 20); // version made by
        push_u16(&mut central, 20); // version needed to extract
        push_u16(&mut central, entry.flags & !8);
        push_u16(&mut central, entry.method);
        central.extend_from_slice(&source[at + 10..at + 14]); // modification time/date
        push_u32(&mut central, entry.crc);
        push_u32(&mut central, entry.compressed);
        push_u32(&mut central, entry.uncompressed);
        push_u16(&mut central, u16::try_from(entry.name.len()).ok()?);
        push_u16(&mut central, 0); // extra field length
        push_u16(&mut central, 0); // comment length
        push_u16(&mut central, 0); // disk number start
        push_u16(&mut central, 0); // internal attributes
        push_u32(&mut central, 0); // external attributes
        push_u32(&mut central, local_offset);
        central.extend_from_slice(entry.name);
        entries = entries.checked_add(1)?;
    }

    if entries == 0 {
        return None;
    }
    let central_offset = u32::try_from(out.len()).ok()?;
    let central_size = u32::try_from(central.len()).ok()?;
    out.extend_from_slice(&central);
    out.extend_from_slice(EOCD_SIGNATURE);
    push_u16(&mut out, 0); // this disk
    push_u16(&mut out, 0); // disk with the central directory
    push_u16(&mut out, entries);
    push_u16(&mut out, entries);
    push_u32(&mut out, central_size);
    push_u32(&mut out, central_offset);
    push_u16(&mut out, 0); // comment length
    Some(out)
}

/// One local file header the scan read, and where the next one can start.
struct LocalEntry<'a> {
    name: &'a [u8],
    flags: u16,
    method: u16,
    crc: u32,
    compressed: u32,
    uncompressed: u32,
    data_end: usize,
    next: usize,
    had_descriptor: bool,
    /// Whether this entry is inside the DOCX profile and fully present.
    admissible: bool,
}

/// Reads the local file header at `at`, resolving a trailing data descriptor.
fn read_local_entry(source: &[u8], at: usize) -> Option<LocalEntry<'_>> {
    let fixed_end = at.checked_add(LOCAL_HEADER_BYTES)?;
    if fixed_end > source.len() {
        return None;
    }
    let flags = read_u16(source, at + 6)?;
    let method = read_u16(source, at + 8)?;
    let mut crc = read_u32(source, at + 14)?;
    let mut compressed = read_u32(source, at + 18)?;
    let mut uncompressed = read_u32(source, at + 22)?;
    let name_length = usize::from(read_u16(source, at + 26)?);
    let extra_length = usize::from(read_u16(source, at + 28)?);
    let name_end = fixed_end.checked_add(name_length)?;
    let data_start = name_end.checked_add(extra_length)?;
    if name_length == 0 || data_start > source.len() {
        return None;
    }
    let name = &source[fixed_end..name_end];

    let mut had_descriptor = false;
    if flags & 8 != 0 && compressed == 0 {
        // The sizes live in a descriptor after the data. Its signature is
        // optional in the specification but written by every producer in
        // practice, so a scan for it is the only way to bound the data.
        let descriptor = find(source, DATA_DESCRIPTOR_SIGNATURE, data_start)?;
        crc = read_u32(source, descriptor + 4)?;
        compressed = read_u32(source, descriptor + 8)?;
        uncompressed = read_u32(source, descriptor + 12)?;
        let measured = u32::try_from(descriptor - data_start).ok()?;
        if compressed != measured {
            compressed = measured;
        }
        had_descriptor = true;
    }

    let data_end = data_start.checked_add(usize::try_from(compressed).ok()?)?;
    if data_end > source.len() {
        // The truncation fell inside this entry's data. Everything before it is
        // still whole, so the scan stops rather than salvaging a short part.
        return None;
    }
    let next = if had_descriptor {
        data_end.checked_add(16)?.min(source.len())
    } else {
        data_end
    };
    // Monotonic progress: a zero-length entry would otherwise rescan its own
    // header forever.
    let next = next.max(at + 1);
    let encrypted = flags & 1 != 0;
    let profile = method == 0 || method == 8;
    Some(LocalEntry {
        name,
        flags,
        method,
        crc,
        compressed,
        uncompressed,
        data_end,
        next,
        had_descriptor,
        admissible: !encrypted && profile,
    })
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u16(out: &mut [u8], offset: usize, value: u16) {
    if let Some(slot) = out.get_mut(offset..offset + 2) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn write_u32(out: &mut [u8], offset: usize, value: u32) {
    if let Some(slot) = out.get_mut(offset..offset + 4) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn find(haystack: &[u8], needle: &[u8; 4], from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at + from)
}
