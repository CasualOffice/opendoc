// SPDX-License-Identifier: Apache-2.0

//! What a session may do, chosen by whoever launches it and never by the agent
//! (`docs/178` §5.1).

use casual_doc_edit::access::Capabilities;
use serde::Serialize;

/// A capability profile. Ordered: each grants everything the one before it
/// does, plus its own row of `docs/178` §5.1.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    /// Open, inspect, search, chunk, render, export, compare, report. The default.
    #[default]
    Read,
    /// Plus comments.
    Comment,
    /// Plus edit plans as tracked suggestions, and undoing them.
    Suggest,
    /// Plus direct (untracked) edits and table structure: the explicit grant
    /// the owner requires for anything that is not a suggestion.
    Edit,
    /// Plus accepting and rejecting tracked changes.
    Review,
}

impl Profile {
    /// Parses a launch flag value.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "read" => Self::Read,
            "comment" => Self::Comment,
            "suggest" => Self::Suggest,
            "edit" => Self::Edit,
            "review" => Self::Review,
            _ => return None,
        })
    }

    /// The flag spelling.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Comment => "comment",
            Self::Suggest => "suggest",
            Self::Edit => "edit",
            Self::Review => "review",
        }
    }

    /// The engine capabilities this profile becomes. The engine's
    /// `refuse_if_not_permitted` judges every lowered operation against this
    /// value, so an adapter bug cannot widen what the profile allows.
    #[must_use]
    pub const fn capabilities(self) -> Capabilities {
        match self {
            Self::Read => Capabilities::viewer(),
            Self::Comment => Capabilities::commenter(),
            Self::Suggest => Capabilities::suggester(),
            // `editor()` carries `review` as well (an editor can produce any state
            // a reviewer can). The review TOOL is still offered only from the
            // `review` profile: the adapter narrows, the engine bounds.
            Self::Edit | Self::Review => Capabilities::editor(),
        }
    }

    /// Whether this profile includes `needed`.
    #[must_use]
    pub fn grants(self, needed: Self) -> bool {
        self >= needed
    }
}
