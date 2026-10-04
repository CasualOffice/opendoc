// SPDX-License-Identifier: Apache-2.0

//! Presentation part identifiers.
//!
//! One newtype per referenced part kind, over the shared [`NodeId`] so a
//! presentation's identities live in the same space as a document's and the
//! duplicate-id walk can compare them directly. A bare `NodeId` would let a slide
//! name a master where a layout is required, and that mistake does not show up until
//! inheritance silently resolves nothing.

use casual_doc_model::NodeId;
use serde::{Deserialize, Serialize};

macro_rules! part_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(NodeId);

        impl $name {
            /// Wraps a node ID as this part identifier.
            #[must_use]
            pub const fn new(id: NodeId) -> Self {
                Self(id)
            }

            /// Returns the underlying node ID.
            #[must_use]
            pub const fn node_id(self) -> NodeId {
                self.0
            }
        }
    };
}

part_id!(
    /// Stable identity of a slide (`p:sld`, entered in `p:sldIdLst`).
    SlideId
);
part_id!(
    /// Stable identity of a slide layout (`p:sldLayout`).
    SlideLayoutId
);
part_id!(
    /// Stable identity of a slide master (`p:sldMaster`).
    SlideMasterId
);
