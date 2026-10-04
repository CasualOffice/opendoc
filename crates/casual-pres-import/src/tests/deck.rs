// SPDX-License-Identifier: Apache-2.0

//! The `.pptx` fixture generator.
//!
//! # Why a generated package and not hand-built XML strings
//!
//! Driving the importer from XML strings would skip the two layers that fail
//! first on a real file: the ZIP container and the OPC reference graph. The
//! fixture is therefore a genuine deflate-compressed ZIP with a real
//! `[Content_Types].xml`, real `_rels` parts and real relationship ids, assembled
//! the same way `casual-doc-ooxml/examples/generate_fixtures.rs` assembles its
//! `.docx` fixtures.
//!
//! # The one deliberately adversarial choice
//!
//! The slide parts are named `slide1.xml`, `slide2.xml` and `slide10.xml`, and
//! `p:sldIdLst` presents them in that order — 1, 2, 10. Every lexical ordering of
//! those part names gives 1, 10, 2 instead. So a reader that takes the deck's
//! order from the part names rather than from `p:sldIdLst` produces a *different,
//! plausible* order, and the order guard can actually fail. A fixture named
//! `slideA`/`slideB`/`slideC` would have let that bug through.
//!
//! # The second adversarial choice: colours that COMPETE
//!
//! The theme part is read now, so the fixture's job changed from "an unconsumed
//! part to report" to "a palette a wrong answer cannot hit by accident". Three
//! things are deliberate:
//!
//! * **The master's `p:clrMap` is the DARK mapping** (`bg1="dk1"`, `tx1="lt1"`),
//!   which is what PowerPoint writes for a dark design. So a reader that ignored
//!   the map and treated `tx1` as `a:dk1` — which is what every alias table does,
//!   including `casual-doc-import`'s, whose own comment admits it — resolves
//!   black where the file means white. A fixture mapping `tx1` to `dk1` could not
//!   tell the two readers apart.
//! * **Three tiers state three different maps for `accent2`.** The master maps it
//!   to `a:accent2` (`ED7D31`), `slideLayout2`'s `a:overrideClrMapping` maps it to
//!   `a:accent4` (`FFC000`), and `slide10`'s maps it to `a:accent5` (`5B9BD5`).
//!   The same four characters of markup therefore mean three different colours
//!   depending on which part they sit in, so the override CHAIN is what the guard
//!   measures rather than the mere presence of a map.
//! * **Every one of the twelve slots differs from every other**, and `dk1`/`lt1`
//!   are `a:sysClr` with a `lastClr` while the rest are `a:srgbClr`, so a reader
//!   that honoured only one of the two forms leaves a hole a guard can see.
//!
//! The `a:fmtScheme` is shaped the same way: entry 2 of `a:lnStyleLst` is a
//! gradient-filled outline this build cannot hold, and it sits AFTER an entry it
//! can, so a reader that dropped the unmodelled entry instead of keeping a `None`
//! in its place would resolve `idx="2"` to entry 1 and paint the wrong outline
//! with nothing reporting it.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// `[Content_Types].xml`. Note the `png` default, which is what makes the
/// picture's media type resolve to `image/png` rather than to nothing.
const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Default Extension="png" ContentType="image/png"/>
<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
<Override PartName="/ppt/slideLayouts/slideLayout2.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
<Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
<Override PartName="/ppt/slides/slide2.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
<Override PartName="/ppt/slides/slide10.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
<Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#;

/// `ppt/presentation.xml`.
///
/// `p:sldSz` is 13.333in x 7.5in in EMU, which is what `screen16x9` means in
/// practice. `p:sldIdLst` is the deck's order, and the `@id` values (256, 257,
/// 258) are producer-scoped numeric slide ids that name no part — only `r:id`
/// does.
const PRESENTATION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rIdMaster"/></p:sldMasterIdLst>
<p:sldIdLst>
<p:sldId id="256" r:id="rIdSlideOne"/>
<p:sldId id="257" r:id="rIdSlideTwo"/>
<p:sldId id="258" r:id="rIdSlideTen"/>
</p:sldIdLst>
<p:sldSz cx="12192000" cy="6858000" type="screen16x9"/>
<p:notesSz cx="6858000" cy="9144000"/>
<p:defaultTextStyle><a:lvl1pPr><a:defRPr sz="1800"/></a:lvl1pPr></p:defaultTextStyle>
</p:presentation>"#;

/// `ppt/_rels/presentation.xml.rels`.
///
/// The relationship ids are deliberately NOT in the order the slides appear, and
/// they are not `rId1`/`rId2`/`rId3` either. A reader that sorted its
/// relationships and used that order, or assumed the ids were sequential, would
/// be wrong here.
const PRESENTATION_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdSlideTen" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide10.xml"/>
<Relationship Id="rIdSlideOne" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
<Relationship Id="rIdSlideTwo" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide2.xml"/>
<Relationship Id="rIdMaster" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>
<Relationship Id="rIdTheme" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>
</Relationships>"#;

/// `ppt/slideMasters/slideMaster1.xml`.
///
/// The master is where the title and body slots get their real geometry: both
/// shapes here carry a full `a:xfrm`, and the slides' corresponding placeholders
/// carry none.
const MASTER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld name="Office Theme">
<p:bg><p:bgPr><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></p:bgPr></p:bg>
<p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
<p:sp>
<p:nvSpPr><p:cNvPr id="2" name="Title Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="838200" y="365126"/><a:ext cx="10515600" cy="1325563"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
<p:txBody><a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" anchor="ctr"><a:normAutofit/></a:bodyPr><a:lstStyle/><a:p><a:endParaRPr lang="en-US"/></a:p></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="3" name="Text Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="838200" y="1825625"/><a:ext cx="10515600" cy="4351338"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
<p:txBody><a:bodyPr vert="horz" anchor="t"/><a:lstStyle/><a:p><a:endParaRPr lang="en-US"/></a:p></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="4" name="Master Accent"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="0" y="6705600"/><a:ext cx="12192000" cy="152400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:schemeClr val="accent2"/></a:solidFill></p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody>
</p:sp>
</p:spTree>
</p:cSld>
<p:clrMap bg1="dk1" tx1="lt1" bg2="dk2" tx2="lt2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rIdLayout1"/><p:sldLayoutId id="2147483650" r:id="rIdLayout2"/></p:sldLayoutIdLst>
<p:txStyles><p:titleStyle><a:lvl1pPr algn="ctr"><a:defRPr sz="4400"/></a:lvl1pPr></p:titleStyle><p:bodyStyle><a:lvl1pPr marL="228600" indent="-228600"><a:buChar char="&#8226;"/><a:defRPr sz="2800"/></a:lvl1pPr></p:bodyStyle><p:otherStyle><a:lvl1pPr/></p:otherStyle></p:txStyles>
</p:sldMaster>"#;

const MASTER_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdLayout1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
<Relationship Id="rIdLayout2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout2.xml"/>
<Relationship Id="rIdTheme" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/>
</Relationships>"#;

/// `slideLayout1.xml` — the title-slide layout, with `ctrTitle` and `subTitle`.
///
/// `ctrTitle` and `title` are both titles, which is why the model folds them: a
/// layout uses one or the other, never both.
const LAYOUT_ONE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="title" preserve="1">
<p:cSld name="Title Slide">
<p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr/>
<p:sp>
<p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="ctrTitle"/></p:nvPr></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="1524000" y="1122363"/><a:ext cx="9144000" cy="2387600"/></a:xfrm></p:spPr>
<p:txBody><a:bodyPr anchor="b"/><a:lstStyle><a:lvl1pPr algn="ctr"><a:defRPr sz="6000"/></a:lvl1pPr></a:lstStyle><a:p><a:r><a:rPr lang="en-US"/><a:t>Click to edit Master title style</a:t></a:r></a:p></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="3" name="Subtitle 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="subTitle" idx="1"/></p:nvPr></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="1524000" y="3602038"/><a:ext cx="9144000" cy="1655762"/></a:xfrm></p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle><a:lvl1pPr marL="0" indent="0" algn="ctr"><a:buNone/><a:defRPr sz="2400"/></a:lvl1pPr></a:lstStyle><a:p><a:r><a:rPr lang="en-US"/><a:t>Click to edit Master subtitle style</a:t></a:r></a:p></p:txBody>
</p:sp>
</p:spTree>
</p:cSld>
<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sldLayout>"#;

/// `slideLayout2.xml` — title and content, with a `body` slot at `idx="1"`.
///
/// Its `p:clrMapOvr` is a REAL `a:overrideClrMapping`, not the usual
/// `<a:masterClrMapping/>`, and it rebinds `accent2` to `a:accent4`. Every slide
/// using this layout therefore resolves `accent2` differently from the master —
/// which is the only way a guard can tell the override chain from the master map.
const LAYOUT_TWO: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="obj" preserve="1">
<p:cSld name="Title and Content">
<p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr/>
<p:sp>
<p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="838200" y="365126"/><a:ext cx="10515600" cy="1325563"/></a:xfrm></p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="en-US"/></a:p></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="3" name="Content Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="838200" y="1825625"/><a:ext cx="10515600" cy="4351338"/></a:xfrm></p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="en-US"/></a:p></p:txBody>
</p:sp>
</p:spTree>
</p:cSld>
<p:clrMapOvr><a:overrideClrMapping bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent4" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/></p:clrMapOvr>
</p:sldLayout>"#;

const LAYOUT_ONE_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdMaster" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>"#;

const LAYOUT_TWO_RELS: &str = LAYOUT_ONE_RELS;

/// `slide1.xml` — the title slide.
///
/// Both placeholders carry **no `a:xfrm`**, which is what PowerPoint actually
/// writes: the geometry comes from `slideLayout1`'s matching slots. The explicit
/// rectangle below them does carry one, plus a solid fill, a dashed outline and a
/// rotation in 1/60000 degree.
///
/// The title's run states `sz="4400"` — 44 points in HUNDREDTHS of a point. A
/// reader that treated it as half-points would make it 2200 points.
const SLIDE_ONE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld name="Opening">
<p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
<p:sp>
<p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="ctrTitle"/></p:nvPr></p:nvSpPr>
<p:spPr/>
<p:txBody><a:bodyPr><a:normAutofit fontScale="92500" lnSpcReduction="10000"/></a:bodyPr><a:lstStyle/>
<a:p><a:r><a:rPr lang="en-US" sz="4400" b="1" dirty="0"><a:solidFill><a:srgbClr val="1F3864"/></a:solidFill><a:latin typeface="+mj-lt"/></a:rPr><a:t>One</a:t></a:r></a:p>
</p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="3" name="Subtitle 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="subTitle" idx="1"/></p:nvPr></p:nvSpPr>
<p:spPr/>
<p:txBody><a:bodyPr/><a:lstStyle/>
<a:p><a:pPr algn="ctr"/><a:r><a:rPr lang="en-US" sz="2400" i="1"/><a:t>First in presentation order</a:t></a:r><a:br><a:rPr lang="en-US" sz="1200"/></a:br><a:r><a:rPr lang="en-US" sz="2400"><a:latin typeface="+mn-lt"/></a:rPr><a:t>second line</a:t></a:r></a:p>
</p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="4" name="Accent Bar"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
<p:spPr>
<a:xfrm rot="1200000" flipH="1"><a:off x="1524000" y="5486400"/><a:ext cx="3048000" cy="152400"/></a:xfrm>
<a:prstGeom prst="roundRect"><a:avLst><a:gd name="adj" fmla="val 25000"/></a:avLst></a:prstGeom>
<a:solidFill><a:srgbClr val="C00000"/></a:solidFill>
<a:ln w="19050"><a:solidFill><a:srgbClr val="000000"/></a:solidFill><a:prstDash val="dash"/><a:tailEnd type="triangle" w="med" len="lg"/></a:ln>
<a:effectLst/>
</p:spPr>
<p:style><a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="50000"/></a:schemeClr></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef><a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></p:style>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="5" name="Themed Band"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="12192000" cy="76200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:schemeClr val="tx1"/></a:solidFill></p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="6" name="Tinted Band"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="0" y="152400"/><a:ext cx="12192000" cy="76200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:schemeClr val="accent1"><a:tint val="40000"/></a:schemeClr></a:solidFill><a:ln w="12700"><a:solidFill><a:schemeClr val="accent1"><a:lumMod val="75000"/><a:lumOff val="25000"/></a:schemeClr></a:solidFill></a:ln></p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody>
</p:sp>
</p:spTree>
</p:cSld>
<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
<p:transition spd="slow"><p:fade/></p:transition>
</p:sld>"#;

const SLIDE_ONE_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdLayout" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#;

/// `slide2.xml` — a picture, a nested group, and a bulleted body.
///
/// The body exercises three outline levels, an inline `a:buChar`, an
/// `a:buAutoNum` with a `startAt`, and an `a:fld` slide number with its cached
/// text. The group carries a child coordinate space whose extent differs from its
/// box, so a reader that ignored `a:chExt` would place the children wrongly.
const SLIDE_TWO: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld name="Detail">
<p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr/>
<p:sp>
<p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
<p:spPr/>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Two</a:t></a:r></a:p></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="3" name="Content Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr>
<p:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
<p:txBody>
<a:bodyPr><a:normAutofit/></a:bodyPr>
<a:lstStyle><a:lvl2pPr marL="742950" indent="-285750"><a:buFont typeface="Courier New"/><a:buChar char="o"/></a:lvl2pPr></a:lstStyle>
<a:p><a:pPr marL="228600" indent="-228600"><a:buFont typeface="Wingdings"/><a:buChar char="&#167;"/></a:pPr><a:r><a:rPr lang="en-US" sz="2800"/><a:t>Top level</a:t></a:r></a:p>
<a:p><a:pPr lvl="1" marL="742950" indent="-285750"><a:buAutoNum type="alphaLcParenR" startAt="3"/></a:pPr><a:r><a:rPr lang="en-US" sz="2400"/><a:t>Second level, numbered from c</a:t></a:r></a:p>
<a:p><a:pPr lvl="2" algn="r"><a:lnSpc><a:spcPct val="150000"/></a:lnSpc><a:spcBef><a:spcPts val="600"/></a:spcBef><a:buNone/><a:tabLst><a:tab pos="914400" algn="ctr"/></a:tabLst></a:pPr><a:r><a:rPr lang="en-US" sz="2000" u="sng" strike="sngStrike" spc="150" baseline="30000" cap="small"/><a:t>Third level, unbulleted</a:t></a:r><a:fld id="{B6F15528-21DE-4FAD-B9F8-3B2E2CFEF2C3}" type="slidenum"><a:rPr lang="en-US" sz="2000"/><a:t>2</a:t></a:fld></a:p>
<a:p><a:endParaRPr lang="en-US" sz="1400"/></a:p>
</p:txBody>
</p:sp>
<p:pic>
<p:nvPicPr><p:cNvPr id="4" name="Logo" descr="The company mark"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr>
<p:blipFill><a:blip r:embed="rIdImage"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>
<p:spPr><a:xfrm><a:off x="9144000" y="457200"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
</p:pic>
<p:grpSp>
<p:nvGrpSpPr><p:cNvPr id="5" name="Diagram"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr><a:xfrm><a:off x="6096000" y="4572000"/><a:ext cx="2743200" cy="1371600"/><a:chOff x="0" y="0"/><a:chExt cx="1371600" cy="685800"/></a:xfrm></p:grpSpPr>
<p:sp>
<p:nvSpPr><p:cNvPr id="6" name="Left Box"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="609600" cy="685800"/></a:xfrm><a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="4472C4"/></a:solidFill></p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="7" name="Right Box"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="762000" y="0"/><a:ext cx="609600" cy="685800"/></a:xfrm><a:prstGeom prst="triangle"><a:avLst/></a:prstGeom><a:noFill/><a:ln w="12700"><a:solidFill><a:srgbClr val="ED7D31"/></a:solidFill></a:ln></p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>in a group</a:t></a:r></a:p></p:txBody>
</p:sp>
</p:grpSp>
<p:sp>
<p:nvSpPr><p:cNvPr id="8" name="Themed Box"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="457200" y="6248400"/><a:ext cx="2743200" cy="304800"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:schemeClr val="accent2"/></a:solidFill></p:spPr>
<p:style><a:lnRef idx="1"><a:schemeClr val="accent3"/></a:lnRef><a:fillRef idx="3"><a:schemeClr val="accent3"/></a:fillRef><a:effectRef idx="2"><a:schemeClr val="accent3"/></a:effectRef><a:fontRef idx="major"><a:schemeClr val="dk1"/></a:fontRef></p:style>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody>
</p:sp>
</p:spTree>
</p:cSld>
<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sld>"#;

const SLIDE_TWO_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdLayout" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout2.xml"/>
<Relationship Id="rIdImage" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/>
</Relationships>"#;

/// `slide10.xml` — a custom geometry, an unmodelled gradient fill, a hidden
/// shape, and an unknown preset.
///
/// This is the slide whose purpose is the loss report: a `a:gradFill`, a
/// `a:custGeom`, a preset (`wedgeRoundRectCallout`) no typed primitive covers,
/// and a `p:graphicFrame` holding a table. Each must be *reported*, and the
/// slide must still import.
///
/// It also carries the deepest colour map in the deck: its own
/// `a:overrideClrMapping` rebinds `accent2` to `a:accent5`, over the layout
/// override that rebinds it to `a:accent4`, over the master map that leaves it at
/// `a:accent2`. The run below says `accent2` and must resolve to the slide's
/// answer, not the layout's and not the master's.
///
/// It is also the third slide in `p:sldIdLst` while sorting SECOND by part name.
const SLIDE_TEN: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" show="0">
<p:cSld name="Appendix">
<p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr/>
<p:sp>
<p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
<p:spPr/>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Ten</a:t></a:r></a:p></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="3" name="Freeform"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
<p:spPr>
<a:xfrm><a:off x="914400" y="2286000"/><a:ext cx="1828800" cy="1828800"/></a:xfrm>
<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l="0" t="0" r="r" b="b"/>
<a:pathLst><a:path w="1828800" h="1828800">
<a:moveTo><a:pt x="0" y="0"/></a:moveTo>
<a:lnTo><a:pt x="1828800" y="914400"/></a:lnTo>
<a:cubicBezTo><a:pt x="1524000" y="1524000"/><a:pt x="609600" y="1828800"/><a:pt x="0" y="1371600"/></a:cubicBezTo>
<a:close/>
</a:path></a:pathLst>
</a:custGeom>
<a:solidFill><a:srgbClr val="70AD47"/></a:solidFill>
</p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="4" name="Callout" hidden="1"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
<p:spPr>
<a:xfrm><a:off x="3657600" y="2286000"/><a:ext cx="2743200" cy="1371600"/></a:xfrm>
<a:prstGeom prst="wedgeRoundRectCallout"><a:avLst><a:gd name="adj1" fmla="val -20000"/><a:gd name="adj2" fmla="val 60000"/></a:avLst></a:prstGeom>
<a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FFFFFF"/></a:gs><a:gs pos="100000"><a:srgbClr val="BDD7EE"/></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill>
<a:effectLst><a:outerShdw blurRad="50800" dist="38100" dir="2700000"><a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw></a:effectLst>
</p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"><a:solidFill><a:schemeClr val="accent2"><a:alpha val="40000"/><a:satMod val="155000"/></a:schemeClr></a:solidFill></a:rPr><a:t>Themed colour, resolved through this slide own map</a:t></a:r></a:p></p:txBody>
</p:sp>
<p:graphicFrame>
<p:nvGraphicFramePr><p:cNvPr id="5" name="Table 4"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr>
<p:xfrm><a:off x="7315200" y="2286000"/><a:ext cx="3657600" cy="1371600"/></p:xfrm>
<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table">
<a:tbl><a:tblPr/><a:tblGrid><a:gridCol w="1828800"/><a:gridCol w="1828800"/></a:tblGrid>
<a:tr h="370840"><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Cell</a:t></a:r></a:p></a:txBody></a:tc><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p/></a:txBody></a:tc></a:tr>
</a:tbl>
</a:graphicData></a:graphic>
</p:graphicFrame>
</p:spTree>
</p:cSld>
<p:clrMapOvr><a:overrideClrMapping bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent5" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/></p:clrMapOvr>
<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"/></p:par></p:tnLst></p:timing>
</p:sld>"#;

const SLIDE_TEN_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdLayout" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout2.xml"/>
</Relationships>"#;

/// `ppt/theme/theme1.xml`.
///
/// All twelve `a:clrScheme` slots, with twelve DISTINCT colours — see the module
/// note on why that matters. `a:dk1`/`a:lt1` are `a:sysClr` with a `lastClr`,
/// which is what Office writes and what makes honouring `lastClr` observable.
///
/// The `a:fontScheme` gives `+mj-lt` and `+mn-lt` different families, so a reader
/// that resolved the wrong collection would be visible, and leaves `a:ea`/`a:cs`
/// with the empty `@typeface` that means "fall back to latin" — a case that must
/// resolve to nothing rather than to the empty string.
///
/// The `a:fmtScheme` carries one entry of each kind this build can hold and one
/// of each it cannot, in positions where confusing them would paint something:
/// `a:fillStyleLst` is solid, gradient, pattern; `a:lnStyleLst` is a solid outline
/// then a gradient one; `a:effectStyleLst` is an EMPTY `a:effectLst` then an
/// `a:outerShdw`, which is the shape that makes reporting on `a:effectRef@idx`
/// alone wrong.
const THEME: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Opendoc Fixture Theme">
<a:themeElements>
<a:clrScheme name="Fixture"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme>
<a:fontScheme name="Fixture"><a:majorFont><a:latin typeface="Calibri Light" panose="020F0302020204030204"/><a:ea typeface=""/><a:cs typeface=""/><a:font script="Hans" typeface="DengXian Light"/></a:majorFont><a:minorFont><a:latin typeface="Calibri" panose="020F0502020204030204"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>
<a:fmtScheme name="Fixture">
<a:fillStyleLst>
<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>
<a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:lumMod val="110000"/><a:satMod val="105000"/><a:tint val="67000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:lumMod val="105000"/><a:satMod val="103000"/><a:tint val="73000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:lumMod val="105000"/><a:satMod val="109000"/><a:tint val="81000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill>
<a:pattFill prst="pct25"><a:fgClr><a:schemeClr val="phClr"/></a:fgClr><a:bgClr><a:srgbClr val="FFFFFF"/></a:bgClr></a:pattFill>
</a:fillStyleLst>
<a:lnStyleLst>
<a:ln w="6350" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln>
<a:ln w="12700" cap="flat" cmpd="sng" algn="ctr"><a:gradFill><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"/></a:gs><a:gs pos="100000"><a:srgbClr val="FFFFFF"/></a:gs></a:gsLst><a:lin ang="0" scaled="0"/></a:gradFill><a:prstDash val="solid"/></a:ln>
</a:lnStyleLst>
<a:effectStyleLst>
<a:effectStyle><a:effectLst/></a:effectStyle>
<a:effectStyle><a:effectLst><a:outerShdw blurRad="57150" dist="19050" dir="5400000" algn="ctr" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="63000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>
</a:effectStyleLst>
<a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst>
</a:fmtScheme>
</a:themeElements>
</a:theme>"#;

/// The picture's bytes.
///
/// A real PNG signature followed by opaque filler. The importer never decodes
/// image bytes — `MediaReference` carries the pointer, not the pixels — so what
/// must hold of these is only that the part is admitted and resolves. Making it a
/// decodable image would invite a test that passes for the wrong reason.
const IMAGE: &[u8] = b"\x89PNG\r\n\x1a\nopendoc-pptx-fixture-opaque-image-bytes-padded-to-defeat-the-expansion-ratio-bound";

/// Every part of the fixture deck, as `(part name, bytes)`.
///
/// Returned as a list rather than written straight into a ZIP so a caller can
/// perturb one part — which is how the negative guards build a package that is
/// malformed in exactly one way.
pub fn deck_parts() -> Vec<(String, Vec<u8>)> {
    let text = |name: &str, body: &str| (name.to_owned(), body.as_bytes().to_vec());
    vec![
        text("[Content_Types].xml", CONTENT_TYPES),
        text("_rels/.rels", ROOT_RELS),
        text("ppt/presentation.xml", PRESENTATION),
        text("ppt/_rels/presentation.xml.rels", PRESENTATION_RELS),
        text("ppt/slideMasters/slideMaster1.xml", MASTER),
        text("ppt/slideMasters/_rels/slideMaster1.xml.rels", MASTER_RELS),
        text("ppt/slideLayouts/slideLayout1.xml", LAYOUT_ONE),
        text(
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
            LAYOUT_ONE_RELS,
        ),
        text("ppt/slideLayouts/slideLayout2.xml", LAYOUT_TWO),
        text(
            "ppt/slideLayouts/_rels/slideLayout2.xml.rels",
            LAYOUT_TWO_RELS,
        ),
        text("ppt/slides/slide1.xml", SLIDE_ONE),
        text("ppt/slides/_rels/slide1.xml.rels", SLIDE_ONE_RELS),
        text("ppt/slides/slide2.xml", SLIDE_TWO),
        text("ppt/slides/_rels/slide2.xml.rels", SLIDE_TWO_RELS),
        text("ppt/slides/slide10.xml", SLIDE_TEN),
        text("ppt/slides/_rels/slide10.xml.rels", SLIDE_TEN_RELS),
        text("ppt/theme/theme1.xml", THEME),
        ("ppt/media/image1.png".to_owned(), IMAGE.to_vec()),
    ]
}

/// Packs parts into a deflate-compressed ZIP.
///
/// Deflated rather than stored so the fixture exercises the same decompression
/// path and the same expansion-ratio bound a real package does.
///
/// # Panics
///
/// Panics if the in-memory ZIP write fails, which it cannot: the sink is a
/// `Vec`, so there is no I/O to fail, and a panic here would mean the fixture
/// builder itself is broken.
pub fn build_pptx(parts: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, bytes) in parts {
        writer
            .start_file(name.clone(), options)
            .expect("in-memory zip entry");
        writer.write_all(bytes).expect("in-memory zip write");
    }
    writer.finish().expect("in-memory zip finish").into_inner()
}

/// The fixture deck as `.pptx` bytes.
pub fn deck() -> Vec<u8> {
    build_pptx(&deck_parts())
}

/// The fixture deck with one part replaced, for a negative guard.
pub fn deck_with(part: &str, body: &[u8]) -> Vec<u8> {
    let mut parts = deck_parts();
    for entry in &mut parts {
        if entry.0 == part {
            entry.1 = body.to_vec();
        }
    }
    build_pptx(&parts)
}

/// The fixture deck with one part removed.
pub fn deck_without(part: &str) -> Vec<u8> {
    let mut parts = deck_parts();
    parts.retain(|entry| entry.0 != part);
    build_pptx(&parts)
}
