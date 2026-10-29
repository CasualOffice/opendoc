//! Embedding a non-DOCX font program as an obfuscated `.odttf` (`docs/182`
//! gap G-1).
//!
//! A PDF import carries its embedded faces as [`EmbeddedFontProgram`]s: plain
//! font bytes in the resource map plus the family and style they serve. A DOCX
//! can carry a face only as a `w:font/w:embed*` entry in `fontTable.xml`, whose
//! relationship points at an **obfuscated** part (ECMA-376 Part 1 §17.8.1): the
//! first 32 bytes XORed with the 16 bytes of a `w:fontKey` GUID, read in
//! hex-string order and reversed. This module decides which programs the package
//! can carry and produces their parts; every one it cannot carry is reported.
//!
//! # What is carried, and what is reported
//!
//! - A **TrueType** program (sfnt `0x00010000` or `true`) is embedded. The
//!   extension `.odttf` is literally "obfuscated TrueType", and that is the face
//!   Word embeds.
//! - A CFF-flavoured OpenType (`OTTO`), a bare CFF table, or a collection is
//!   **not** embedded and is reported `docx.export.font_program.not_carried`:
//!   this writer does not claim that a consumer reads CFF outlines from an
//!   `.odttf`, so it does not write one.
//! - A program whose bytes the caller did not supply is reported
//!   `docx.export.font_program.missing_bytes`, as an `.odttf` face is.
//! - A program whose family and style are already embedded (by the document's
//!   own `fontTable.xml`, or by an earlier program — a PDF routinely carries
//!   several subsets of one face) is reported
//!   `docx.export.font_program.slot_taken`: a `w:font` has one slot per style.
//!
//! # Determinism
//!
//! The `w:fontKey` is derived from the program (family, style, bytes) by a fixed
//! hash, not drawn at random, so writing the same document twice produces the
//! same package. The key's only job is the obfuscation, which is not a security
//! boundary.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::v1::{Definitions, EmbeddedFontProgram, FontProgramFormat, FontProgramStyle};

use crate::report::{Disposition, Reporter};

/// The bytes ECMA-376 §17.8.1 obfuscates.
const OBFUSCATED_PREFIX_BYTES: usize = 32;

/// One program the package carries, ready to write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProgramEmbed {
    /// The `w:font/@w:name` it is filed under.
    pub family: String,
    /// The `w:embed*` slot.
    pub style: FontProgramStyle,
    /// The `fontTable.xml.rels` relationship id.
    pub relationship_id: String,
    /// The `w:fontKey` GUID, braced.
    pub font_key: String,
    /// The `.odttf` part name.
    pub part_name: String,
    /// The obfuscated part bytes.
    pub bytes: Vec<u8>,
    /// `w:subsetted`.
    pub subsetted: bool,
}

/// The `w:embed*` element name to its style.
fn slot_style(element: &str) -> FontProgramStyle {
    match element {
        "w:embedBold" => FontProgramStyle::Bold,
        "w:embedItalic" => FontProgramStyle::Italic,
        "w:embedBoldItalic" => FontProgramStyle::BoldItalic,
        _ => FontProgramStyle::Regular,
    }
}

/// Which of `definitions.font_programs` the package carries, with their parts;
/// every other program is named in `reporter`.
///
/// Complexity: O(p · log p + f) in programs and font-table faces, plus one pass
/// over each carried program's bytes (hash and obfuscation).
pub(crate) fn plan(
    definitions: &Definitions,
    media: &BTreeMap<String, Vec<u8>>,
    reporter: &mut Reporter,
) -> Vec<ProgramEmbed> {
    let mut taken: BTreeSet<(String, FontProgramStyle)> = BTreeSet::new();
    let mut relationship_ids: BTreeSet<String> = BTreeSet::new();
    let mut part_names: BTreeSet<String> = media.keys().cloned().collect();
    for font in &definitions.font_table {
        for (element, face) in font.embedded.faces() {
            relationship_ids.insert(face.relationship_id.clone());
            part_names.insert(face.part_name.clone());
            // The writer emits a face only when its bytes are supplied, so only
            // those occupy the slot.
            if media.contains_key(&face.part_name)
                && crate::semantic::is_writable_part_name(&face.part_name, true)
            {
                taken.insert((font.name.clone(), slot_style(element)));
            }
        }
    }
    let mut out = Vec::new();
    let mut counter = 0_usize;
    for program in &definitions.font_programs {
        let Some(bytes) = media.get(&program.part_name) else {
            reporter.record_part(
                "docx.export.font_program.missing_bytes",
                &program.part_name,
                Disposition::OmittedNotRetained,
            );
            continue;
        };
        if !is_truetype(program, bytes) {
            reporter.record_part(
                "docx.export.font_program.not_carried",
                &program.part_name,
                Disposition::OmittedNotRetained,
            );
            continue;
        }
        if !taken.insert((program.family.clone(), program.style)) {
            reporter.record_part(
                "docx.export.font_program.slot_taken",
                &program.part_name,
                Disposition::OmittedNotRetained,
            );
            continue;
        }
        let key = font_key_bytes(program, bytes);
        let (relationship_id, part_name) = loop {
            counter += 1;
            let id = format!("rIdFontProgram{counter}");
            let part = format!("word/fonts/program{counter}.odttf");
            if !relationship_ids.contains(&id) && !part_names.contains(&part) {
                break (id, part);
            }
        };
        relationship_ids.insert(relationship_id.clone());
        part_names.insert(part_name.clone());
        out.push(ProgramEmbed {
            family: program.family.clone(),
            style: program.style,
            relationship_id,
            font_key: guid_string(&key),
            part_name,
            bytes: obfuscate(&key, bytes),
            subsetted: program.subsetted,
        });
    }
    out
}

/// A TrueType-outline sfnt, as the model says and as the bytes confirm.
fn is_truetype(program: &EmbeddedFontProgram, bytes: &[u8]) -> bool {
    program.format == FontProgramFormat::TrueType
        && bytes.len() >= OBFUSCATED_PREFIX_BYTES
        && matches!(
            bytes.first_chunk::<4>(),
            Some(b"\x00\x01\x00\x00" | b"true")
        )
}

/// 64-bit FNV-1a, seeded.
fn fnv1a(seed: u64, parts: &[&[u8]]) -> u64 {
    let mut hash = seed;
    for part in parts {
        for byte in *part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        // A separator, so ("ab","c") and ("a","bc") differ.
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// The 16 GUID bytes, in the order its hex string is written.
fn font_key_bytes(program: &EmbeddedFontProgram, bytes: &[u8]) -> [u8; 16] {
    let style = [program.style as u8];
    let parts: [&[u8]; 3] = [program.family.as_bytes(), &style, bytes];
    let high = fnv1a(0xcbf2_9ce4_8422_2325, &parts);
    let low = fnv1a(0x6c62_272e_07bb_0142, &parts);
    let mut key = [0_u8; 16];
    key[..8].copy_from_slice(&high.to_be_bytes());
    key[8..].copy_from_slice(&low.to_be_bytes());
    key
}

/// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`, uppercase, in byte order.
fn guid_string(key: &[u8; 16]) -> String {
    let hex: String = key.iter().map(|byte| format!("{byte:02X}")).collect();
    format!(
        "{{{}-{}-{}-{}-{}}}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// ECMA-376 Part 1 §17.8.1: XOR the first 32 bytes with the key's bytes in
/// reverse string order (`key[15 - i % 16]`). Its own inverse.
fn obfuscate(key: &[u8; 16], plain: &[u8]) -> Vec<u8> {
    let mut out = plain.to_vec();
    for (index, byte) in out.iter_mut().take(OBFUSCATED_PREFIX_BYTES).enumerate() {
        *byte ^= key[15 - index % 16];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worked example of `40-FONT-MANAGEMENT-DESIGN.md` §3.3: GUID
    /// `001B70DC-AA60-4AD5-90EC-18A0948E1EAE` XORs with
    /// `AE 1E 8E 94 A0 18 EC 90 D5 4A 60 AA DC 70 1B 00`, twice.
    #[test]
    fn obfuscation_uses_the_reversed_hex_string_bytes() {
        let key = [
            0x00, 0x1B, 0x70, 0xDC, 0xAA, 0x60, 0x4A, 0xD5, 0x90, 0xEC, 0x18, 0xA0, 0x94, 0x8E,
            0x1E, 0xAE,
        ];
        assert_eq!(guid_string(&key), "{001B70DC-AA60-4AD5-90EC-18A0948E1EAE}");
        let applied = [
            0xAE, 0x1E, 0x8E, 0x94, 0xA0, 0x18, 0xEC, 0x90, 0xD5, 0x4A, 0x60, 0xAA, 0xDC, 0x70,
            0x1B, 0x00,
        ];
        let out = obfuscate(&key, &[0_u8; 40]);
        assert_eq!(&out[..16], &applied);
        assert_eq!(&out[16..32], &applied);
        assert_eq!(&out[32..], &[0_u8; 8], "bytes 32.. are untouched");
    }
}
