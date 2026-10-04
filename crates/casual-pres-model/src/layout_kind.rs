// SPDX-License-Identifier: Apache-2.0

//! The named slide-layout kinds (`p:sldLayout@type`, `ST_SlideLayoutType`).

use serde::{Deserialize, Serialize};

/// What a slide layout is for (`p:sldLayout@type`).
///
/// A hint, not a behaviour: the layout's authority is its own shape tree and its
/// placeholder slots, and PowerPoint lays a slide out from those, not from this
/// token. It is modeled because export must write back what the file said, and
/// because the UI's "New Slide" gallery groups by it — a deck whose layouts all
/// read `cust` loses its gallery even though every slide still renders correctly.
///
/// These are `ST_SlideLayoutType`'s thirty-six values. An unrecognized token reads
/// as [`LayoutKind::Custom`], which is the lossless degradation: the shape tree
/// still carries everything that positions content.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "camelCase")]
pub enum LayoutKind {
    /// Title slide (`title`).
    Title,
    /// Title and body text (`tx`).
    #[serde(rename = "tx")]
    Text,
    /// Two-column text (`twoColTx`).
    TwoColumnText,
    /// Table (`tbl`).
    #[serde(rename = "tbl")]
    Table,
    /// Text and chart (`txAndChart`).
    TextAndChart,
    /// Chart and text (`chartAndTx`).
    ChartAndText,
    /// Diagram (`dgm`).
    #[serde(rename = "dgm")]
    Diagram,
    /// Chart (`chart`).
    Chart,
    /// Text and clip art (`txAndClipArt`).
    TextAndClipArt,
    /// Clip art and text (`clipArtAndTx`).
    ClipArtAndText,
    /// Title only (`titleOnly`).
    TitleOnly,
    /// Blank (`blank`).
    Blank,
    /// Text and object (`txAndObj`).
    TextAndObject,
    /// Object and text (`objAndTx`).
    ObjectAndText,
    /// Object only (`objOnly`).
    ObjectOnly,
    /// Title and object (`obj`).
    #[serde(rename = "obj")]
    Object,
    /// Text and media (`txAndMedia`).
    TextAndMedia,
    /// Media and text (`mediaAndTx`).
    MediaAndText,
    /// Object over text (`objOverTx`).
    ObjectOverText,
    /// Text over object (`txOverObj`).
    TextOverObject,
    /// Text and two objects (`txAndTwoObj`).
    TextAndTwoObjects,
    /// Two objects and text (`twoObjAndTx`).
    TwoObjectsAndText,
    /// Two objects over text (`twoObjOverTx`).
    TwoObjectsOverText,
    /// Four objects (`fourObj`).
    FourObjects,
    /// Vertical text (`vertTx`).
    VerticalText,
    /// Clip art and vertical text (`clipArtAndVertTx`).
    ClipArtAndVerticalText,
    /// Vertical title and text (`vertTitleAndTx`).
    VerticalTitleAndText,
    /// Vertical title and text over chart (`vertTitleAndTxOverChart`).
    VerticalTitleAndTextOverChart,
    /// Two objects (`twoObj`).
    TwoObjects,
    /// Object and two objects (`objAndTwoObj`).
    ObjectAndTwoObjects,
    /// Two objects and object (`twoObjAndObj`).
    TwoObjectsAndObject,
    /// Section header (`secHead`).
    SectionHeader,
    /// Two text blocks and two objects (`twoTxTwoObj`).
    TwoTextTwoObjects,
    /// Object and text, title-less variant (`objTx`).
    ObjectText,
    /// Picture and caption (`picTx`).
    PictureText,
    /// Custom (`cust`). The default, so an unknown token degrades here.
    #[default]
    #[serde(rename = "cust")]
    Custom,
}

impl LayoutKind {
    /// The `p:sldLayout@type` token, for export and diagnostics.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Text => "tx",
            Self::TwoColumnText => "twoColTx",
            Self::Table => "tbl",
            Self::TextAndChart => "txAndChart",
            Self::ChartAndText => "chartAndTx",
            Self::Diagram => "dgm",
            Self::Chart => "chart",
            Self::TextAndClipArt => "txAndClipArt",
            Self::ClipArtAndText => "clipArtAndTx",
            Self::TitleOnly => "titleOnly",
            Self::Blank => "blank",
            Self::TextAndObject => "txAndObj",
            Self::ObjectAndText => "objAndTx",
            Self::ObjectOnly => "objOnly",
            Self::Object => "obj",
            Self::TextAndMedia => "txAndMedia",
            Self::MediaAndText => "mediaAndTx",
            Self::ObjectOverText => "objOverTx",
            Self::TextOverObject => "txOverObj",
            Self::TextAndTwoObjects => "txAndTwoObj",
            Self::TwoObjectsAndText => "twoObjAndTx",
            Self::TwoObjectsOverText => "twoObjOverTx",
            Self::FourObjects => "fourObj",
            Self::VerticalText => "vertTx",
            Self::ClipArtAndVerticalText => "clipArtAndVertTx",
            Self::VerticalTitleAndText => "vertTitleAndTx",
            Self::VerticalTitleAndTextOverChart => "vertTitleAndTxOverChart",
            Self::TwoObjects => "twoObj",
            Self::ObjectAndTwoObjects => "objAndTwoObj",
            Self::TwoObjectsAndObject => "twoObjAndObj",
            Self::SectionHeader => "secHead",
            Self::TwoTextTwoObjects => "twoTxTwoObj",
            Self::ObjectText => "objTx",
            Self::PictureText => "picTx",
            Self::Custom => "cust",
        }
    }

    /// Reads a `p:sldLayout@type` token; anything unrecognized is
    /// [`LayoutKind::Custom`].
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| kind.token() == token)
            .unwrap_or(Self::Custom)
    }

    /// Every kind this build knows, so a guard derives the count rather than
    /// hand-maintaining it.
    pub const ALL: [Self; 36] = [
        Self::Title,
        Self::Text,
        Self::TwoColumnText,
        Self::Table,
        Self::TextAndChart,
        Self::ChartAndText,
        Self::Diagram,
        Self::Chart,
        Self::TextAndClipArt,
        Self::ClipArtAndText,
        Self::TitleOnly,
        Self::Blank,
        Self::TextAndObject,
        Self::ObjectAndText,
        Self::ObjectOnly,
        Self::Object,
        Self::TextAndMedia,
        Self::MediaAndText,
        Self::ObjectOverText,
        Self::TextOverObject,
        Self::TextAndTwoObjects,
        Self::TwoObjectsAndText,
        Self::TwoObjectsOverText,
        Self::FourObjects,
        Self::VerticalText,
        Self::ClipArtAndVerticalText,
        Self::VerticalTitleAndText,
        Self::VerticalTitleAndTextOverChart,
        Self::TwoObjects,
        Self::ObjectAndTwoObjects,
        Self::TwoObjectsAndObject,
        Self::SectionHeader,
        Self::TwoTextTwoObjects,
        Self::ObjectText,
        Self::PictureText,
        Self::Custom,
    ];
}
