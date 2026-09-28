//! A deterministic 128-bit content hash.
//!
//! # Why not `std::hash::DefaultHasher`
//!
//! The alignment keys in this crate are compared **across two documents** and
//! are asserted by golden tests, so the hash has to be the same value today,
//! tomorrow, and on another target. `DefaultHasher` is explicitly documented as
//! unspecified and free to change between releases, which is fine for a
//! `HashMap` and wrong for a key a test pins.
//!
//! # Why 128 bits
//!
//! Alignment treats equal keys as the same block, so a collision is not a slow
//! path — it is a **wrong diff**. FNV-1a 128 over the content of two documents
//! (at most a few million blocks) leaves a collision probability below 2^-80,
//! which is far below the probability of the storage layer handing back the
//! wrong bytes. A 64-bit key would reach a 50% collision chance at 2^32 blocks
//! and a one-in-a-million chance at ~6×10^6 — inside the document sizes this
//! engine admits (`MAX_VIEWER_BLOCKS` is 1,800,000).
//!
//! All hashing here is **O(bytes hashed)** with no allocation.

/// FNV-1a 128-bit offset basis.
const OFFSET_BASIS: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;

/// FNV-1a 128-bit prime.
const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

/// An incremental FNV-1a 128 hasher.
///
/// Every `write_*` is **O(bytes)**; `finish` is O(1).
#[derive(Clone, Copy, Debug)]
pub struct ContentHasher(u128);

impl Default for ContentHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentHasher {
    /// Starts a hash at the FNV offset basis.
    #[must_use]
    pub const fn new() -> Self {
        Self(OFFSET_BASIS)
    }

    /// Mixes raw bytes in.
    pub fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u128::from(*byte);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    /// Mixes a string in, length-prefixed so `("ab", "c")` and `("a", "bc")`
    /// hash differently. Without the prefix a paragraph's inline boundaries
    /// would be invisible to the hash and two different documents could share a
    /// key.
    pub fn write_str(&mut self, value: &str) {
        self.write_u64(value.len() as u64);
        self.write(value.as_bytes());
    }

    /// Mixes a `u64` in, little-endian.
    pub fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    /// Mixes a `u128` in, little-endian. Used to fold a child's subtree hash
    /// into its parent's, which is what makes an unchanged subtree one
    /// comparison instead of a walk.
    pub fn write_u128(&mut self, value: u128) {
        self.write(&value.to_le_bytes());
    }

    /// Mixes a discriminant tag in. Distinct tags keep a paragraph holding the
    /// text "x" from sharing a key with a table cell holding the text "x".
    pub fn write_tag(&mut self, tag: u8) {
        self.write(&[tag]);
    }

    /// The hash so far.
    #[must_use]
    pub const fn finish(self) -> u128 {
        self.0
    }
}

/// The hash of one string, as a one-liner.
#[must_use]
pub fn hash_str(value: &str) -> u128 {
    let mut hasher = ContentHasher::new();
    hasher.write_str(value);
    hasher.finish()
}
