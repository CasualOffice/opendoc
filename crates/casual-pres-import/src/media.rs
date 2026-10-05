// SPDX-License-Identifier: Apache-2.0

//! Media registration: an `a:blip@r:embed` into `Definitions::media`.
//!
//! # Why this is a resolver and not a pass over the media folder
//!
//! A picture's reference is a *relationship id*, scoped to the part that declares
//! it. Two slides can both say `r:embed="rId2"` and mean different images, and the
//! same image referenced from two slides must become one media entry — because
//! `ShapeTree::validate` resolves `GroupPicture::media` against one
//! presentation-wide table.
//!
//! So the resolver is keyed by the **resolved part name**, not by the
//! relationship id: the part name is what identifies the image, and the id is
//! only how one part spells it. Enumerating `ppt/media/` instead would register
//! images nothing references and miss images stored elsewhere, since OPC fixes no
//! media location.

use std::collections::BTreeMap;

use casual_doc_model::NodeId;
use casual_doc_model::v1::{Definitions, MediaId, MediaReference};

use crate::ImportError;
use crate::ids::Ids;
use crate::loss::Reporter;
use crate::opc::Relationships;

/// Registers the media one part references, deduplicating by part name.
#[derive(Debug)]
pub(crate) struct MediaResolver<'a> {
    /// The declaring part's relationships, for turning an `r:embed` into a part.
    relationships: &'a Relationships,
    /// The part the references are read from, for a bounded finding location.
    part: &'a str,
    /// Resolved part name to the media id already minted for it, shared across
    /// every part of the deck.
    registered: &'a mut BTreeMap<String, NodeId>,
    /// The table the entries land in.
    media: &'a mut Definitions,
    /// Every ADMITTED part, mapped to its declared content type.
    ///
    /// Admission is the point as much as the content type: a key that is absent
    /// means the package never carried that part, which is how a dangling
    /// `r:embed` is detected without the resolver holding the package. And the
    /// declared type is recorded rather than guessed from the extension, because
    /// `[Content_Types].xml` is authoritative and a `.png` override declaring
    /// JPEG is a package the author can produce.
    admitted: &'a BTreeMap<String, Option<String>>,
}

impl<'a> MediaResolver<'a> {
    /// A resolver over one part's relationships.
    pub(crate) fn new(
        relationships: &'a Relationships,
        part: &'a str,
        registered: &'a mut BTreeMap<String, NodeId>,
        media: &'a mut Definitions,
        admitted: &'a BTreeMap<String, Option<String>>,
    ) -> Self {
        Self {
            relationships,
            part,
            registered,
            media,
            admitted,
        }
    }

    /// The declaring part's relationships.
    ///
    /// A picture is not the only thing in a shape tree that spells a reference as
    /// an `r:id`: `a:hlinkClick` does too, and it is read from the same part with
    /// the same table. Handing the table out — rather than growing a second
    /// resolver, or threading `&Relationships` alongside this type through the
    /// whole recursion — keeps one answer to "which relationships is this part
    /// reading against", which is the question `MediaResolver`'s module note says
    /// is the easy one to get wrong.
    ///
    /// The returned borrow lives as long as the table, not as long as `self`, so
    /// a caller can hold it across a `&mut self` call to [`resolve`](Self::resolve).
    pub(crate) fn relationships(&self) -> &'a Relationships {
        self.relationships
    }

    /// Resolves an `r:embed` to a media id, registering the entry on first use.
    ///
    /// `None` when the id resolves to no relationship, to an external target, or
    /// to a part the package did not admit. Each is reported by the caller as an
    /// invalid `a:blip`, because a picture whose bytes are not in the package
    /// cannot be modelled at all — `GroupPicture::media` is not optional and
    /// `ShapeTree::validate` refuses a dangling reference.
    ///
    /// # Complexity
    ///
    /// O(log media) — one map lookup per reference, so a deck that uses one image
    /// on two hundred slides registers it once.
    pub(crate) fn resolve(
        &mut self,
        relationship_id: &str,
        reporter: &mut Reporter,
        ids: &mut Ids,
    ) -> Result<Option<NodeId>, ImportError> {
        let Some(relationship) = self.relationships.get(relationship_id) else {
            return Ok(None);
        };
        let Some(part_name) = relationship.resolved_part.as_deref() else {
            // An external (`TargetMode="External"`) image. Never fetched: a
            // network read at open time is an exfiltration channel and a hang.
            reporter.omitted(self.part, b"blip/link");
            return Ok(None);
        };
        let Some(declared) = self.admitted.get(part_name) else {
            // The relationship resolves to a part the package does not contain.
            // Registering it anyway would put a dangling reference in the media
            // table and `ShapeTree::validate` would refuse the whole deck for it.
            return Ok(None);
        };
        if let Some(existing) = self.registered.get(part_name) {
            return Ok(Some(*existing));
        }
        let id = ids.next()?;
        self.media.media.insert(
            MediaId::new(id),
            MediaReference {
                relationship_id: relationship_id.to_owned(),
                media_type: declared.clone().unwrap_or_default(),
                part_name: part_name.to_owned(),
            },
        );
        self.registered.insert(part_name.to_owned(), id);
        Ok(Some(id))
    }
}
