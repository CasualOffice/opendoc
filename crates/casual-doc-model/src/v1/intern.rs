//! Shared, copy-on-write property sets — the flyweight half of the document
//! model.
//!
//! # Why this type exists
//!
//! A document stores its formatting **per node**: every [`super::Paragraph`]
//! owns a [`super::ParagraphProperties`] and every [`super::Run`] owns a
//! [`super::RunProperties`]. Those two structs are 304 and 352 bytes, and they
//! are *identical* across almost every node of almost every document — a
//! plain-text file of 1,303,306 paragraphs has exactly **one** distinct value
//! of each, and used to store 1,303,306 copies of both (`docs/111` §4).
//!
//! That is textbook redundancy, and the textbook answer is normalization:
//! store each distinct value once and let the nodes reference it. [`Shared`]
//! is that reference. It is an [`Arc`], so:
//!
//! - a node costs **8 bytes** of formatting rather than 304 or 352;
//! - the shared value is freed when the last node referencing it drops, so
//!   nothing accumulates and there is no table to compact;
//! - reads are unchanged, because [`Shared`] dereferences to the value —
//!   `paragraph.properties.alignment` still compiles and still reads a field;
//! - **writes are copy-on-write**, because mutation goes through
//!   `Arc::make_mut`: a node that shares its properties with a million others
//!   gets its own copy the moment it is edited, and the million are untouched.
//!
//! # What is actually shared
//!
//! Two mechanisms, both cheap, applied in [`Shared::new`]:
//!
//! 1. **The default.** Each [`Shareable`] type has one process-wide default
//!    entry. Constructing a slot from a value equal to the default hands back
//!    a reference to that one entry. This is the whole win on a plain-text or
//!    lightly-formatted document, and it costs one comparison.
//! 2. **A bounded most-recently-interned cache**, [`Shareable::with_recent`],
//!    holding at most [`RECENT_ENTRIES`] entries per type per thread. Real
//!    documents have enormous locality — consecutive runs in a paragraph, and
//!    consecutive paragraphs in a section, almost always carry identical
//!    formatting — so a small cache captures most of the repetition that is
//!    not the default, without a hash of a 352-byte struct per node and
//!    without a table that grows with the document.
//!
//! Neither mechanism is observable through the API: two slots holding equal
//! values behave identically whether or not they share an entry. Sharing is a
//! memory property, and [`Shared::share_count`] exists so a test can assert it.
//!
//! # Serialization is unchanged
//!
//! [`Shared<T>`] serializes exactly as `T` does and deserializes from exactly
//! what `T` accepts, so no on-disk or on-wire form moved. Deserialization goes
//! through [`Shared::new`], so reopening a snapshot re-shares.

use std::cell::RefCell;
use std::fmt;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::OnceLock;

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;

/// How many distinct non-default entries per type stay eligible for reuse.
///
/// Small on purpose. The cache exists to catch locality (a run of bold runs, a
/// section of indented paragraphs), not to be a complete index of the
/// document's formatting: a complete index would cost a hash of a 352-byte
/// struct per node and would hold every distinct value alive for as long as
/// the process runs. Eight entries is scanned by equality in a few dozen
/// nanoseconds and holds at most a few kilobytes.
pub const RECENT_ENTRIES: usize = 8;

/// A property set that can be shared between nodes by [`Shared`].
///
/// Implemented for [`super::ParagraphProperties`] and
/// [`super::RunProperties`]. The two methods give [`Shared`] the per-type
/// storage it needs without a type-keyed registry: one process-wide default
/// entry, and one bounded per-thread reuse cache.
pub trait Shareable: Clone + Default + PartialEq + Sized + 'static {
    /// The one process-wide entry holding this type's default value.
    fn shared_default() -> &'static Arc<Self>;

    /// Runs `action` against this type's bounded reuse cache for the current
    /// thread.
    fn with_recent<R>(action: impl FnOnce(&mut Vec<Arc<Self>>) -> R) -> R;
}

/// A shared, copy-on-write property set.
///
/// Reads go straight through ([`Deref`]); writes copy first if the value is
/// shared ([`DerefMut`], which calls `Arc::make_mut`). See the module
/// documentation for why the model stores formatting this way.
pub struct Shared<T: Shareable>(Arc<T>);

impl<T: Shareable> Shared<T> {
    /// Interns `value`, reusing an existing entry when one holds an equal
    /// value.
    ///
    /// Checks the type's default entry first (one comparison, and the hit for
    /// the overwhelming majority of nodes in the overwhelming majority of
    /// documents), then the bounded recent-entry cache.
    #[must_use]
    pub fn new(value: T) -> Self {
        let default_entry = T::shared_default();
        if value == **default_entry {
            return Self(Arc::clone(default_entry));
        }
        T::with_recent(|recent| {
            if let Some(entry) = recent.iter().find(|entry| ***entry == value) {
                return Self(Arc::clone(entry));
            }
            let entry = Arc::new(value);
            if recent.len() == RECENT_ENTRIES {
                recent.remove(0);
            }
            recent.push(Arc::clone(&entry));
            Self(entry)
        })
    }

    /// Borrows the shared value.
    ///
    /// Equivalent to dereferencing; named so a caller that needs an explicit
    /// `&T` (a turbofished generic, a trait object) does not have to write
    /// `&**slot`.
    #[must_use]
    pub fn get(&self) -> &T {
        &self.0
    }

    /// Mutably borrows the value, copying it first if it is shared.
    ///
    /// **This is the copy-on-write seam.** Every mutation of a node's
    /// properties goes through here (directly, or through [`DerefMut`]), so a
    /// node can never write through a reference another node also holds.
    pub fn make_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.0)
    }

    /// Consumes the slot and returns the value, cloning it if it is shared.
    #[must_use]
    pub fn into_inner(self) -> T {
        Arc::try_unwrap(self.0).unwrap_or_else(|entry| (*entry).clone())
    }

    /// Whether two slots reference the same entry.
    ///
    /// A memory property, not an equality: two slots holding equal values may
    /// or may not share. Exists for the guards in `v1::tests`.
    #[must_use]
    pub fn shares_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// How many slots (plus any cache entry) reference this value.
    ///
    /// Exists for the guards in `v1::tests`: it is how a test asserts that a
    /// million paragraphs hold one property set rather than a million.
    #[must_use]
    pub fn share_count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

impl<T: Shareable> Deref for Shared<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Shareable> DerefMut for Shared<T> {
    /// Copy-on-write: if the value is shared, this clones it first and
    /// repoints only this slot. See `make_mut`.
    fn deref_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.0)
    }
}

impl<T: Shareable> Default for Shared<T> {
    fn default() -> Self {
        Self(Arc::clone(T::shared_default()))
    }
}

impl<T: Shareable> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T: Shareable + fmt::Debug> fmt::Debug for Shared<T> {
    /// Forwards to the value, so a slot is indistinguishable from the value it
    /// replaced in any diagnostic, assertion message or snapshot.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl<T: Shareable> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
    }
}

impl<T: Shareable + Eq> Eq for Shared<T> {}

impl<T: Shareable> PartialEq<T> for Shared<T> {
    fn eq(&self, other: &T) -> bool {
        *self.0 == *other
    }
}

impl<T: Shareable> From<T> for Shared<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: Shareable> AsRef<T> for Shared<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T: Shareable + Serialize> Serialize for Shared<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        T::serialize(&self.0, serializer)
    }
}

impl<'de, T: Shareable + Deserialize<'de>> Deserialize<'de> for Shared<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::new)
    }
}

/// Implements [`Shareable`] for one property type, giving it a process-wide
/// default entry and a per-thread bounded reuse cache.
macro_rules! shareable {
    ($type:ty, $default:ident, $recent:ident) => {
        impl $crate::v1::Shareable for $type {
            fn shared_default() -> &'static ::std::sync::Arc<Self> {
                static $default: OnceLock<Arc<$type>> = OnceLock::new();
                $default.get_or_init(|| Arc::new(<$type>::default()))
            }

            fn with_recent<R>(action: impl FnOnce(&mut Vec<Arc<Self>>) -> R) -> R {
                thread_local! {
                    static $recent: RefCell<Vec<Arc<$type>>> = const { RefCell::new(Vec::new()) };
                }
                $recent.with(|recent| action(&mut recent.borrow_mut()))
            }
        }
    };
}

shareable!(
    super::ParagraphProperties,
    DEFAULT_PARAGRAPH_PROPERTIES,
    RECENT_PARAGRAPH_PROPERTIES
);
shareable!(
    super::RunProperties,
    DEFAULT_RUN_PROPERTIES,
    RECENT_RUN_PROPERTIES
);

/// A [`super::ParagraphProperties`] stored once and referenced by every
/// paragraph that carries the same formatting.
pub type SharedParagraphProperties = Shared<super::ParagraphProperties>;

/// A [`super::RunProperties`] stored once and referenced by every run that
/// carries the same formatting.
pub type SharedRunProperties = Shared<super::RunProperties>;
