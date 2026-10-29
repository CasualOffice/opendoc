//! The password on the editing restriction — Word's Restrict Editing ▸ "Yes,
//! Start Enforcing Protection" password, and Stop Protection asking for it
//! (`docs/153` `review.protection-password`, `docs/109` RM-32, ADR-052 as
//! amended 2026-10-30).
//!
//! # What is computed
//!
//! Exactly what Word writes, so a document protected here asks Word for the same
//! password and a document protected in Word is unlocked here by it:
//!
//! 1. The password, truncated to fifteen characters, is folded into Word's
//!    legacy 32-bit key (ECMA-376 Part 1 §17.15.1.29's `w:hash` algorithm: the
//!    high word from the initial-code table and the encryption matrix, the low
//!    word from the rotate-and-xor sequence). Each character contributes its low
//!    byte, or its high byte when the low byte is zero.
//! 2. The key's four bytes, little-endian, become eight upper-case hex digits,
//!    and THAT string — UTF-16LE — is what is hashed, after the salt:
//!    `H0 = H(salt ‖ key)`, then `Hn = H(Hn-1 ‖ LE32(n))` for `n` in
//!    `0..spinCount`.
//! 3. Word 2013 and later write SHA-512, 100,000 spins and a 16-byte salt, as the
//!    transitional attribute set `w:cryptProviderType="rsaAES"
//!    w:cryptAlgorithmClass="hash" w:cryptAlgorithmType="typeAny"
//!    w:cryptAlgorithmSid="14" w:cryptSpinCount="100000" w:hash w:salt`. That is
//!    what [`word_verifier`] produces.
//!
//! ONLYOFFICE computes the same thing (sdkjs `common/hash/hash.js:186-266`,
//! `prepareWordPassword`, whose reversed `fullWord` is step 2's byte order) and,
//! when it checks a password, tries both the folded key and the raw password
//! (`word/api.js:14467-14478`) because Word accepts either. [`verify`] does too.
//!
//! # What it is not
//!
//! Not encryption, and the dialog says so. The verifier sits in plain XML next to
//! the restriction it guards; anyone who can edit `settings.xml` can delete both.
//! ADR-052's "policy, not security" stands: what the password adds is that a
//! reader must know it to lift the restriction **through an editor that honours
//! it**, which is what Word and ONLYOFFICE do and now what this editor does.
//!
//! # Bounds
//!
//! A hostile file can name any spin count. One above [`MAX_SPIN_COUNT`] is not
//! attempted — the verifier is reported unverifiable, and the restriction is then
//! treated the way it was before this module existed (the dialog says changing it
//! removes the password). Hashing is O(spin count) and runs once per Apply, never
//! per keystroke.

use casual_doc_edit::Operation;
use casual_doc_model::v1::{DocumentProtection, PasswordAttribute, PasswordVerifier};
use sha2::{Digest, Sha256, Sha384, Sha512};
use wasm_bindgen::prelude::*;

use crate::{EditResult, HistoryKind, WasmDocument, protection_edit, to_js};

/// The spin count Word writes and this module writes.
pub const SPIN_COUNT: u32 = 100_000;

/// The largest spin count a verifier is checked with. Word writes 100,000; ten
/// million is two orders of magnitude of headroom and still bounded.
pub const MAX_SPIN_COUNT: u32 = 10_000_000;

/// The salt length Word writes, in bytes.
pub const SALT_LEN: usize = 16;

/// The refusal code for a password that does not match the document's verifier.
pub const WRONG_PASSWORD: &str = "document.protection-wrong-password";

/// Word's initial-code table (ECMA-376 Part 1 §17.15.1.29), indexed by password
/// length minus one.
const INITIAL_CODE: [u16; 15] = [
    0xE1F0, 0x1D0F, 0xCC9C, 0x84C0, 0x110C, 0x0E10, 0xF1CE, 0x313E, 0x1872, 0xE139, 0xD40F, 0x84F9,
    0x280C, 0xA96A, 0x4EC3,
];

/// Word's encryption matrix (ECMA-376 Part 1 §17.15.1.29): seven words per
/// character position, for the fifteen positions a password can fill.
const ENCRYPTION_MATRIX: [[u16; 7]; 15] = [
    [0xAEFC, 0x4DD9, 0x9BB2, 0x2745, 0x4E8A, 0x9D14, 0x2A09],
    [0x7B61, 0xF6C2, 0xFDA5, 0xEB6B, 0xC6F7, 0x9DCF, 0x2BBF],
    [0x4563, 0x8AC6, 0x05AD, 0x0B5A, 0x16B4, 0x2D68, 0x5AD0],
    [0x0375, 0x06EA, 0x0DD4, 0x1BA8, 0x3750, 0x6EA0, 0xDD40],
    [0xD849, 0xA0B3, 0x5147, 0xA28E, 0x553D, 0xAA7A, 0x44D5],
    [0x6F45, 0xDE8A, 0xAD35, 0x4A4B, 0x9496, 0x390D, 0x721A],
    [0xEB23, 0xC667, 0x9CEF, 0x29FF, 0x53FE, 0xA7FC, 0x5FD9],
    [0x47D3, 0x8FA6, 0x0F6D, 0x1EDA, 0x3DB4, 0x7B68, 0xF6D0],
    [0xB861, 0x60E3, 0xC1C6, 0x93AD, 0x377B, 0x6EF6, 0xDDEC],
    [0x45A0, 0x8B40, 0x06A1, 0x0D42, 0x1A84, 0x3508, 0x6A10],
    [0xAA51, 0x4483, 0x8906, 0x022D, 0x045A, 0x08B4, 0x1168],
    [0x76B4, 0xED68, 0xCAF1, 0x85C3, 0x1BA7, 0x374E, 0x6E9C],
    [0x3730, 0x6E60, 0xDCC0, 0xA9A1, 0x4363, 0x86C6, 0x1DAD],
    [0x3331, 0x6662, 0xCCC4, 0x89A9, 0x0373, 0x06E6, 0x0DCC],
    [0x1021, 0x2042, 0x4084, 0x8108, 0x1231, 0x2462, 0x48C4],
];

/// Word's legacy key for `password`, as the eight hex digits Office hashes in
/// the password's place. O(15).
#[must_use]
pub fn legacy_key_hex(password: &str) -> String {
    let bytes: Vec<u8> = password
        .encode_utf16()
        .take(15)
        .map(|unit| {
            let [high, low] = unit.to_be_bytes();
            if low == 0 { high } else { low }
        })
        .collect();
    if bytes.is_empty() {
        return "00000000".to_owned();
    }
    let len = bytes.len();
    let mut high = INITIAL_CODE[len - 1];
    for (index, byte) in bytes.iter().enumerate() {
        let row = &ENCRYPTION_MATRIX[15 - len + index];
        for (bit, word) in row.iter().enumerate() {
            if byte & (1 << bit) != 0 {
                high ^= word;
            }
        }
    }
    let rotate = |value: u16| ((value >> 14) & 0x0001) | ((value << 1) & 0x7FFF);
    let mut low: u16 = 0;
    for byte in bytes.iter().rev() {
        low = rotate(low) ^ u16::from(*byte);
    }
    // `len` is at most 15, so the cast cannot truncate.
    #[allow(clippy::cast_possible_truncation)]
    let length = len as u16;
    low = rotate(low) ^ length ^ 0xCE4B;
    let key = (u32::from(high) << 16) | u32::from(low);
    key.to_le_bytes()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

/// The hash a verifier names. SHA-1 is Word 2007 and 2010's, kept so their
/// documents can be unlocked; nothing here writes it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Algorithm {
    /// `cryptAlgorithmSid` 4, `algorithmName` `SHA-1`.
    Sha1,
    /// `cryptAlgorithmSid` 12, `SHA-256`.
    Sha256,
    /// `cryptAlgorithmSid` 13, `SHA-384`.
    Sha384,
    /// `cryptAlgorithmSid` 14, `SHA-512`.
    Sha512,
}

impl Algorithm {
    fn from_sid(sid: &str) -> Option<Self> {
        match sid.trim() {
            "4" => Some(Self::Sha1),
            "12" => Some(Self::Sha256),
            "13" => Some(Self::Sha384),
            "14" => Some(Self::Sha512),
            _ => None,
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_uppercase().as_str() {
            "SHA-1" | "SHA1" => Some(Self::Sha1),
            "SHA-256" | "SHA256" => Some(Self::Sha256),
            "SHA-384" | "SHA384" => Some(Self::Sha384),
            "SHA-512" | "SHA512" => Some(Self::Sha512),
            _ => None,
        }
    }

    fn digest(self, parts: &[&[u8]]) -> Vec<u8> {
        fn run<D: Digest>(parts: &[&[u8]]) -> Vec<u8> {
            let mut hasher = D::new();
            for part in parts {
                hasher.update(part);
            }
            hasher.finalize().to_vec()
        }
        match self {
            Self::Sha1 => {
                let joined: Vec<u8> = parts.concat();
                sha1(&joined).to_vec()
            }
            Self::Sha256 => run::<Sha256>(parts),
            Self::Sha384 => run::<Sha384>(parts),
            Self::Sha512 => run::<Sha512>(parts),
        }
    }
}

/// The iterated, salted hash of `text` (UTF-16LE). O(spin count).
#[must_use]
pub fn office_hash(algorithm: Algorithm, salt: &[u8], text: &str, spin_count: u32) -> Vec<u8> {
    let encoded: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut hash = algorithm.digest(&[salt, &encoded]);
    for iteration in 0..spin_count {
        hash = algorithm.digest(&[&hash, &iteration.to_le_bytes()]);
    }
    hash
}

/// The verifier Word 2013+ writes for `password` with `salt`.
///
/// # Errors
///
/// When the salt is not [`SALT_LEN`] bytes.
pub fn word_verifier(password: &str, salt: &[u8]) -> Result<PasswordVerifier, String> {
    if salt.len() != SALT_LEN {
        return Err(format!("a protection salt is {SALT_LEN} bytes"));
    }
    let hash = office_hash(
        Algorithm::Sha512,
        salt,
        &legacy_key_hex(password),
        SPIN_COUNT,
    );
    let mut verifier = PasswordVerifier::default();
    for (attribute, value) in [
        (PasswordAttribute::CryptProviderType, "rsaAES".to_owned()),
        (PasswordAttribute::CryptAlgorithmClass, "hash".to_owned()),
        (PasswordAttribute::CryptAlgorithmType, "typeAny".to_owned()),
        (PasswordAttribute::CryptAlgorithmSid, "14".to_owned()),
        (PasswordAttribute::CryptSpinCount, SPIN_COUNT.to_string()),
        (PasswordAttribute::Hash, base64_encode(&hash)),
        (PasswordAttribute::Salt, base64_encode(salt)),
    ] {
        verifier.set(attribute, value);
    }
    Ok(verifier)
}

/// What checking a password against a verifier found.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verification {
    /// The password is the one the verifier was made from.
    Match,
    /// It is not.
    Mismatch,
    /// The verifier names an algorithm, a spin count or an encoding this module
    /// cannot check, or is missing a part. Nothing can be said about any password.
    Unverifiable,
}

/// The parts of a verifier needed to check one, whichever attribute set states
/// them: the ISO four, else the transitional legacy set.
fn parts(verifier: &PasswordVerifier) -> Option<(Algorithm, Vec<u8>, Vec<u8>, u32)> {
    let iso = verifier
        .get(PasswordAttribute::HashValue)
        .zip(verifier.get(PasswordAttribute::SaltValue));
    let (algorithm, hash, salt, spin) = if let Some((hash, salt)) = iso {
        (
            Algorithm::from_name(verifier.get(PasswordAttribute::AlgorithmName)?)?,
            hash,
            salt,
            verifier.get(PasswordAttribute::SpinCount),
        )
    } else {
        (
            Algorithm::from_sid(verifier.get(PasswordAttribute::CryptAlgorithmSid)?)?,
            verifier.get(PasswordAttribute::Hash)?,
            verifier.get(PasswordAttribute::Salt)?,
            verifier.get(PasswordAttribute::CryptSpinCount),
        )
    };
    let spin = match spin {
        Some(text) => text.trim().parse::<u32>().ok()?,
        None => 0,
    };
    if spin > MAX_SPIN_COUNT {
        return None;
    }
    Some((algorithm, base64_decode(hash)?, base64_decode(salt)?, spin))
}

/// Whether `verifier` can be checked at all. O(16).
#[must_use]
pub fn is_verifiable(verifier: &PasswordVerifier) -> bool {
    parts(verifier).is_some()
}

/// Checks `password` against `verifier`, trying Word's folded key first and the
/// raw password second (Word accepts both). O(spin count), twice at most.
#[must_use]
pub fn verify(verifier: &PasswordVerifier, password: &str) -> Verification {
    let Some((algorithm, hash, salt, spin)) = parts(verifier) else {
        return Verification::Unverifiable;
    };
    for candidate in [legacy_key_hex(password), password.to_owned()] {
        if office_hash(algorithm, &salt, &candidate, spin) == hash {
            return Verification::Match;
        }
    }
    Verification::Mismatch
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard padded base64. O(n).
#[must_use]
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (index, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                out.push(char::from(BASE64[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodes standard base64, ignoring whitespace (ONLYOFFICE notes Word writes
/// stray whitespace into `w:hash`, `word/api.js:14402-14409`). `None` for
/// anything else that is not base64. O(n).
#[must_use]
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            byte if byte.is_ascii_whitespace() => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            // Masked to eight bits, so the cast cannot truncate anything kept.
            #[allow(clippy::cast_possible_truncation)]
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// SHA-1 (FIPS 180-4), for VERIFYING Word 2007 and 2010 verifiers only. Kept
/// here rather than as a dependency because nothing writes it and its one use is
/// a comparison a hostile file cannot make expensive (the spin count is bounded).
fn sha1(message: &[u8]) -> [u8; 20] {
    let mut state: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let bit_len = (message.len() as u64).wrapping_mul(8);
    let mut padded = message.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    for block in padded.chunks(64) {
        let mut w = [0u32; 80];
        for (index, word) in block.chunks(4).enumerate() {
            w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..80 {
            w[index] = (w[index - 3] ^ w[index - 8] ^ w[index - 14] ^ w[index - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (index, word) in w.iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut out = [0u8; 20];
    for (index, word) in state.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[wasm_bindgen]
impl WasmDocument {
    /// Whether the restriction carries a password, and whether this editor can
    /// check it: `{"present":bool,"verifiable":bool}`. A password that is present
    /// and not verifiable (an algorithm or spin count this build does not check)
    /// is the one case where a change here removes it without asking — the
    /// dialog says so before Apply. O(16).
    #[wasm_bindgen(js_name = protectionPassword)]
    #[must_use]
    pub fn protection_password(&self) -> String {
        let verifier = self
            .document
            .definitions()
            .settings
            .document_protection
            .as_ref()
            .and_then(|protection| protection.password.as_ref());
        format!(
            "{{\"present\":{},\"verifiable\":{}}}",
            verifier.is_some(),
            verifier.is_some_and(is_verifiable)
        )
    }

    /// Checks `password` against the restriction's verifier without changing
    /// anything: `"match"`, `"mismatch"`, `"unverifiable"`, or `"none"` when
    /// there is no password. The dialog asks first so a wrong password is said
    /// beside the field rather than after the dialog closes. O(spin count).
    #[wasm_bindgen(js_name = checkProtectionPassword)]
    #[must_use]
    pub fn check_protection_password(&self, password: &str) -> String {
        let verifier = self
            .document
            .definitions()
            .settings
            .document_protection
            .as_ref()
            .and_then(|protection| protection.password.as_ref());
        match verifier.map(|verifier| verify(verifier, password)) {
            None => "none",
            Some(Verification::Match) => "match",
            Some(Verification::Mismatch) => "mismatch",
            Some(Verification::Unverifiable) => "unverifiable",
        }
        .to_owned()
    }

    /// Installs, changes or lifts the editing restriction with Word's password
    /// semantics, as one undoable action (Word's Restrict Editing and Stop
    /// Protection; ONLYOFFICE's `asc_setDocumentProtection`).
    ///
    /// `edit`, `enforcement` and `formatting` are
    /// [`set_document_protection`](WasmDocument::set_document_protection)'s.
    ///
    /// * When the current restriction carries a password this editor can check,
    ///   `current_password` must match it, or the call is refused with
    ///   `document.protection-wrong-password` and nothing changes — that is Word's
    ///   Stop Protection asking for the password.
    /// * `new_password`, when non-empty and a restriction is being installed, is
    ///   stored as the verifier Word 2013+ writes, salted with `salt` (16 bytes,
    ///   which the host draws from `crypto.getRandomValues`). Empty means no
    ///   password.
    ///
    /// # Errors
    ///
    /// The refusal above, an unknown `edit` token, a salt of the wrong length, or
    /// the choke point refusing the operation. O(spin count) once.
    #[wasm_bindgen(js_name = protectDocument)]
    pub fn protect_document(
        &mut self,
        edit: Option<String>,
        enforcement: bool,
        formatting: bool,
        current_password: Option<String>,
        new_password: Option<String>,
        salt: Option<Vec<u8>>,
    ) -> Result<EditResult, JsValue> {
        self.protect_document_inner(
            edit.as_deref(),
            enforcement,
            formatting,
            current_password.as_deref(),
            new_password.as_deref(),
            salt.as_deref(),
        )
        .map_err(to_js)
    }
}

impl WasmDocument {
    /// [`WasmDocument::protect_document`] before the `JsValue` boundary, so native
    /// tests reach every refusal.
    pub(crate) fn protect_document_inner(
        &mut self,
        edit: Option<&str>,
        enforcement: bool,
        formatting: bool,
        current_password: Option<&str>,
        new_password: Option<&str>,
        salt: Option<&[u8]>,
    ) -> Result<EditResult, String> {
        let existing = self
            .document
            .definitions()
            .settings
            .document_protection
            .as_ref()
            .and_then(|protection| protection.password.as_ref());
        if let Some(verifier) = existing
            && verify(verifier, current_password.unwrap_or("")) == Verification::Mismatch
        {
            return Err(casual_doc_edit::refusal::marked(
                WRONG_PASSWORD,
                "That isn't the password this document's restriction was set with.",
            ));
        }
        let protection = match edit {
            None => None,
            Some(token) => {
                let edit = protection_edit(token).ok_or_else(|| {
                    casual_doc_edit::refusal::marked(
                        "document.protection-unknown-level",
                        &format!("\u{201c}{token}\u{201d} isn't an editing restriction."),
                    )
                })?;
                let password = match new_password.filter(|password| !password.is_empty()) {
                    Some(password) => Some(word_verifier(password, salt.unwrap_or(&[]))?),
                    None => None,
                };
                Some(DocumentProtection {
                    edit,
                    enforcement,
                    formatting,
                    password,
                })
            }
        };
        self.apply_action_as(
            vec![Operation::SetDocumentProtection { protection }],
            HistoryKind::DocumentProperties,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn sha1_matches_the_fips_vectors() {
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex(&sha1(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }

    #[test]
    fn base64_round_trips_and_ignores_whitespace() {
        for bytes in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            assert_eq!(base64_decode(&base64_encode(bytes)).as_deref(), Some(bytes));
        }
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_decode("Zm9v\n YmE=").as_deref(), Some(&b"fooba"[..]));
        assert_eq!(base64_decode("Zm9v*"), None);
    }

    /// The legacy key against the value Word's own algorithm gives "password"
    /// (the 16-bit `w:unprotectPassword` era's CE4B-seeded fold): the high word
    /// is the initial code for length 8 xored with the matrix rows the bits of
    /// p-a-s-s-w-o-r-d select, and the result is printed little-endian. The
    /// expected digits are computed independently in the test by the spec's
    /// steps, so a byte-order or table slip in the production fold goes red.
    #[test]
    fn the_legacy_key_folds_the_way_the_spec_describes() {
        let password = b"password";
        let mut high = INITIAL_CODE[password.len() - 1];
        for (index, byte) in password.iter().enumerate() {
            for bit in 0..7 {
                if byte & (1 << bit) != 0 {
                    high ^= ENCRYPTION_MATRIX[15 - password.len() + index][bit];
                }
            }
        }
        let mut low: u16 = 0;
        for byte in password.iter().rev() {
            low = (((low >> 14) & 1) | ((low << 1) & 0x7FFF)) ^ u16::from(*byte);
        }
        low = (((low >> 14) & 1) | ((low << 1) & 0x7FFF)) ^ 8 ^ 0xCE4B;
        let expected = format!(
            "{:02X}{:02X}{:02X}{:02X}",
            low & 0xFF,
            low >> 8,
            high & 0xFF,
            high >> 8
        );
        assert_eq!(legacy_key_hex("password"), expected);
        assert_eq!(legacy_key_hex("password").len(), 8);
        // Fifteen characters is the most Word folds.
        assert_eq!(
            legacy_key_hex("abcdefghijklmnopqrst"),
            legacy_key_hex("abcdefghijklmno")
        );
    }

    #[test]
    fn a_verifier_written_here_accepts_its_password_and_refuses_another() {
        let salt = [7u8; SALT_LEN];
        let verifier = word_verifier("Secret1", &salt).expect("verifier");
        assert_eq!(
            verifier.get(PasswordAttribute::CryptAlgorithmSid),
            Some("14")
        );
        assert_eq!(
            verifier.get(PasswordAttribute::CryptSpinCount),
            Some("100000")
        );
        assert_eq!(verify(&verifier, "Secret1"), Verification::Match);
        assert_eq!(verify(&verifier, "secret1"), Verification::Mismatch);
        assert_eq!(verify(&verifier, ""), Verification::Mismatch);
        assert!(word_verifier("x", &[1, 2, 3]).is_err());
    }

    /// A Word 2007-style verifier (SHA-1, legacy attributes) and an ISO one
    /// (`algorithmName`), both built by the independent `office_hash` path the
    /// way each producer writes them, are checked — and a hostile spin count is
    /// not attempted.
    #[test]
    fn older_and_iso_verifiers_are_checked_and_a_hostile_spin_count_is_not() {
        let salt = [3u8; SALT_LEN];
        let mut legacy = PasswordVerifier::default();
        legacy.set(PasswordAttribute::CryptAlgorithmSid, "4".to_owned());
        legacy.set(PasswordAttribute::CryptSpinCount, "1000".to_owned());
        legacy.set(
            PasswordAttribute::Hash,
            base64_encode(&office_hash(
                Algorithm::Sha1,
                &salt,
                &legacy_key_hex("open"),
                1000,
            )),
        );
        legacy.set(PasswordAttribute::Salt, base64_encode(&salt));
        assert_eq!(verify(&legacy, "open"), Verification::Match);
        assert_eq!(verify(&legacy, "shut"), Verification::Mismatch);

        let mut iso = PasswordVerifier::default();
        iso.set(PasswordAttribute::AlgorithmName, "SHA-256".to_owned());
        iso.set(PasswordAttribute::SpinCount, "10".to_owned());
        // The RAW password, which Word also accepts.
        iso.set(
            PasswordAttribute::HashValue,
            base64_encode(&office_hash(Algorithm::Sha256, &salt, "open", 10)),
        );
        iso.set(PasswordAttribute::SaltValue, base64_encode(&salt));
        assert_eq!(verify(&iso, "open"), Verification::Match);

        let mut hostile = iso.clone();
        hostile.set(PasswordAttribute::SpinCount, "4000000000".to_owned());
        assert_eq!(verify(&hostile, "open"), Verification::Unverifiable);
        assert!(!is_verifiable(&hostile));
    }

    /// The whole gesture through the facade: protect with a password, refuse
    /// Stop Protection without it and with a wrong one, accept the right one,
    /// and Undo puts the passworded restriction back.
    #[test]
    fn stop_protection_asks_for_the_password_it_was_set_with() {
        let mut document = crate::open_document(b"Alpha\nBeta").expect("opens");
        let salt = [9u8; SALT_LEN];
        document
            .protect_document_inner(
                Some("readOnly"),
                true,
                false,
                None,
                Some("hunter2"),
                Some(&salt[..]),
            )
            .expect("protects");
        assert_eq!(
            document.protection_password(),
            "{\"present\":true,\"verifiable\":true}"
        );
        assert_eq!(document.check_protection_password("hunter2"), "match");
        assert_eq!(document.check_protection_password("hunter3"), "mismatch");

        for wrong in [None, Some("hunter3")] {
            let refusal = document
                .protect_document_inner(None, false, false, wrong, None, None)
                .expect_err("a wrong password is refused");
            assert!(refusal.contains(WRONG_PASSWORD), "{refusal}");
            assert!(
                document
                    .document
                    .definitions()
                    .settings
                    .document_protection
                    .is_some(),
                "and nothing changed"
            );
        }
        document
            .protect_document_inner(None, false, false, Some("hunter2"), None, None)
            .expect("the right password stops protection");
        assert!(
            document
                .document
                .definitions()
                .settings
                .document_protection
                .is_none()
        );
        assert_eq!(document.check_protection_password(""), "none");

        document.undo_inner().expect("undo");
        assert_eq!(document.check_protection_password("hunter2"), "match");
    }
}
