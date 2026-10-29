//! Owned per-page evidence: everything the importer needs, nothing it does
//! not (PDFR-I1: evidence only, never editor state).
//!
//! Coordinates are PDF points in **page space with a top-left origin and y
//! growing down**, after the page's CropBox offset and `/Rotate` are applied —
//! the frame a word processor places objects in. Paint order is the order of
//! [`PageEvidence::items`].

/// A colour with alpha, 8 bits per channel.
pub type Rgba8 = [u8; 4];

/// A 2-D point in page space (points, y down).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// Horizontal position.
    pub x: f64,
    /// Vertical position (down is positive).
    pub y: f64,
}

/// An axis-aligned rectangle in page space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    /// Left edge.
    pub x0: f64,
    /// Top edge.
    pub y0: f64,
    /// Right edge.
    pub x1: f64,
    /// Bottom edge.
    pub y1: f64,
}

impl Bounds {
    /// The width.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    /// The height.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    /// The intersection, or `None` when the two do not overlap.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        let out = Self {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        };
        (out.x1 > out.x0 && out.y1 > out.y0).then_some(out)
    }

    /// Whether `other` lies inside this rectangle within `slack` points.
    #[must_use]
    pub fn contains(&self, other: &Self, slack: f64) -> bool {
        other.x0 >= self.x0 - slack
            && other.y0 >= self.y0 - slack
            && other.x1 <= self.x1 + slack
            && other.y1 <= self.y1 + slack
    }
}

/// How a glyph run or path is painted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderMode {
    /// Filled.
    Fill,
    /// Stroked.
    Stroke,
    /// Filled and stroked.
    FillStroke,
    /// Not painted (text render mode 3: an OCR layer, usually).
    Invisible,
}

/// One positioned glyph.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphEvidence {
    /// The Unicode text the glyph maps to (`ToUnicode`, then the glyph list,
    /// then `uniXXXX` names), or `None` when no mapping exists.
    pub text: Option<String>,
    /// Baseline origin.
    pub origin: Point,
    /// Advance along the baseline, in points (from the PDF's widths).
    pub advance: f64,
    /// The glyph's index in its font program (hayro's `glyph_id`), when the
    /// font is an outline font; `None` for Type3 glyphs and for synthesized
    /// evidence (OCR). It is what maps a CID-keyed program's glyphs to the
    /// characters the PDF drew with them (`docs/182` §18.5).
    pub glyph: Option<u32>,
}

/// A run of glyphs sharing font, size, colour, mode and orientation.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRunEvidence {
    /// The font, an index into [`DocumentFonts`].
    pub font: usize,
    /// The em size in points (the text matrix x CTM vertical scale).
    pub size: f64,
    /// Clockwise rotation of the baseline in degrees (0 = upright).
    pub rotation_degrees: f64,
    /// Fill colour (`None` when painted with a pattern).
    pub fill: Option<Rgba8>,
    /// Paint mode.
    pub mode: RenderMode,
    /// The glyphs, in show order.
    pub glyphs: Vec<GlyphEvidence>,
    /// Marked-content id (`MCID`) the run sits in, if any.
    pub mcid: Option<i32>,
    /// Whether the run sits in `/Artifact` marked content (page furniture).
    pub artifact: bool,
    /// The page content's show operator that painted this run (its index
    /// among the page stream's `Tj`/`TJ`/`'`/`"`, `docs/182` P4), or `None`
    /// when a form XObject painted it or the page's operators could not be
    /// numbered. A byte-preserving save removes exactly these operators for an
    /// edited frame.
    pub show: Option<u32>,
}

/// One path command, in page space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathCommand {
    /// Start a subpath.
    MoveTo(Point),
    /// A straight segment.
    LineTo(Point),
    /// A quadratic Bezier.
    QuadTo(Point, Point),
    /// A cubic Bezier.
    CubicTo(Point, Point, Point),
    /// Close the subpath.
    Close,
}

/// A stroke.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeEvidence {
    /// Colour (`None` for a pattern).
    pub color: Option<Rgba8>,
    /// Line width in points (scaled by the CTM).
    pub width: f64,
    /// Whether a dash array is set.
    pub dashed: bool,
}

/// A painted path.
#[derive(Clone, Debug, PartialEq)]
pub struct PathEvidence {
    /// The commands.
    pub commands: Vec<PathCommand>,
    /// Fill colour, when filled (`Some(None)` for a pattern fill).
    pub fill: Option<Option<Rgba8>>,
    /// Whether the fill uses the even-odd rule.
    pub even_odd: bool,
    /// The stroke, when stroked.
    pub stroke: Option<StrokeEvidence>,
    /// The path's bounds.
    pub bounds: Bounds,
    /// Whether an active clip cuts into the path's bounds.
    pub clipped: bool,
}

/// The encoding an image's bytes use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageFormat {
    /// PNG, re-encoded from the decoded raster.
    Png,
    /// JPEG, carried through from a `DCTDecode` stream unchanged.
    Jpeg,
}

/// A painted image.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageEvidence {
    /// Encoded bytes.
    pub bytes: Vec<u8>,
    /// Their encoding.
    pub format: ImageFormat,
    /// Pixel width and height.
    pub pixels: (u32, u32),
    /// The PDF filters the source stream used (for reporting).
    pub codecs: Vec<&'static str>,
    /// The centre of the placed image.
    pub center: Point,
    /// Displayed width and height in points (before rotation).
    pub size: (f64, f64),
    /// Clockwise rotation in degrees.
    pub rotation_degrees: f64,
    /// Mirrored vertically relative to the PDF's upright placement.
    pub flip_v: bool,
    /// The visible part after the active clip, when the clip cuts the image.
    pub clip: Option<Bounds>,
    /// The unclipped, unrotated bounds.
    pub bounds: Bounds,
}

/// One painted object, in paint order.
#[derive(Clone, Debug, PartialEq)]
pub enum PaintEvidence {
    /// Text.
    Glyphs(GlyphRunEvidence),
    /// A vector path.
    Path(PathEvidence),
    /// A raster image.
    Image(ImageEvidence),
}

/// What the importer must report about a page that is not paint.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PageNotes {
    /// Paints with a pattern or shading (approximated as a flat colour).
    pub pattern_paints: u32,
    /// Transparency groups, soft masks and non-normal blend modes (flattened:
    /// the model paints every object opaque and normal).
    pub transparency: u32,
    /// Glyph runs a clip cuts into.
    pub clipped_text: u32,
    /// Type3 glyphs drawn (their outlines are not kept as text).
    pub type3_glyphs: u32,
    /// Images that failed to decode.
    pub undecodable_images: u32,
    /// Interpreter warnings (unsupported font kinds, decode failures).
    pub warnings: u32,
}

/// One page's evidence.
#[derive(Clone, Debug, PartialEq)]
pub struct PageEvidence {
    /// Zero-based page index.
    pub index: usize,
    /// Displayed width in points (CropBox, rotation applied).
    pub width: f64,
    /// Displayed height in points.
    pub height: f64,
    /// `/Rotate` in degrees (0, 90, 180, 270).
    pub rotation: u16,
    /// Painted objects in paint order.
    pub items: Vec<PaintEvidence>,
    /// Non-paint findings.
    pub notes: PageNotes,
    /// Operators in the page's own content stream.
    pub operators: u64,
    /// The page object's number and generation (`docs/182` P4).
    pub object: Option<(u32, u16)>,
    /// How many show operators the page stream has, when every glyph run of
    /// the page stream carries its [`GlyphRunEvidence::show`]; `None` when the
    /// operators could not be numbered (the two tokenizers disagreed, or the
    /// stream already used the marker tag).
    pub shows: Option<u32>,
}

/// What an embedded font program is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontProgramKind {
    /// An sfnt with TrueType outlines.
    TrueType,
    /// An sfnt with CFF outlines (`OTTO`).
    OpenTypeCff,
    /// A bare CFF program (`FontFile3`/`Type1C`), not an sfnt.
    BareCff,
    /// Something else.
    Other,
}

/// A font's identity and, when embedded and extractable, its program.
#[derive(Clone, Debug, PartialEq)]
pub struct FontEvidence {
    /// The `BaseFont` name with any six-letter subset prefix removed.
    pub name: String,
    /// The family name the font descriptor declares, if any.
    pub family: Option<String>,
    /// Bold, from the descriptor weight, flags or name.
    pub bold: bool,
    /// Italic, from the descriptor flags, angle or name.
    pub italic: bool,
    /// Serif, from the descriptor flags.
    pub serif: bool,
    /// Fixed pitch, from the descriptor flags.
    pub monospace: bool,
    /// The font kind (`Type0`, `TrueType`, `Type1`, `Type3`, ...).
    pub subtype: String,
    /// The embedded program, when the font embeds one hayro can extract.
    pub program: Option<(FontProgramKind, Vec<u8>)>,
}

/// The fonts seen so far, in first-use order. Indices are stable.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentFonts {
    /// The fonts.
    pub fonts: Vec<FontEvidence>,
}

/// Document-level content that is never executed or extracted, only counted
/// for the report (MF-I6).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ActiveContent {
    /// JavaScript (open action, name tree, or an action on an annotation).
    pub javascript: u32,
    /// Launch, `GoToR`, `ImportData`, `SubmitForm` and other non-navigation
    /// actions.
    pub actions: u32,
    /// Embedded files and file attachment annotations.
    pub embedded_files: u32,
    /// Interactive form fields (`AcroForm`) or XFA.
    pub form_fields: u32,
    /// Link annotations with a URI (kept only as a count in P1).
    pub links: u32,
    /// Other annotations (comments, highlights, stamps, widgets).
    pub annotations: u32,
    /// Whether a structure tree is present (tagged PDF).
    pub tagged: bool,
    /// Whether the document carries optional content (layers).
    pub optional_content: bool,
}
