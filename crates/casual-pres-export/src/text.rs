// SPDX-License-Identifier: Apache-2.0

//! `a:txBody` and the list styles that share its grammar.
//!
//! # One serializer per grammar, not per element
//!
//! `a:lstStyle`, a master's three `p:txStyles` tiers and `p:defaultTextStyle` are
//! all `CT_TextListStyle` — the element name differs and the content model does
//! not. The importer reads all five through one reader for that reason, and this
//! writes all five through one writer. Two serializers of one grammar disagree the
//! first time either is edited.
//!
//! # Every token comes from the model
//!
//! Each enum in `casual-pres-model` carries a `token()` that returns the exact
//! `ST_*` string it was read from, so an alignment, an anchor, a bullet scheme or
//! an underline style round-trips as the token the file stated rather than one
//! this writer chose. That is what makes a reopen comparable: a deck rewritten
//! with a synonym token would import identically and diff on every line.
//!
//! # Units are carried, never converted
//!
//! `a:rPr@sz` is hundredths of a point and is written from
//! `size_hundredths_point` verbatim. `w:sz`'s half-points are a different unit on
//! a different element, and a value carried across is wrong by fifty and still
//! looks like a font size. Offsets, insets, margins and indents are EMU;
//! `a:bodyPr@rot` is 1/60000 degree; `a:spcPct`, the `a:normAutofit` scales and
//! `a:buSzPct` are thousandths of a percent; `a:spcPts` and `a:rPr@spc` are
//! hundredths of a point.

use casual_pres_model::{
    ListStyle, TextAutoFit, TextBody, TextBullet, TextCharacterProperties, TextParagraph,
    TextParagraphProperties, TextRun, TextSpacing, TextStyles, Typeface,
};

/// Escapes the five XML entities. Written out rather than taken from a helper
/// because a run's text is the one place in this crate that carries arbitrary
/// author content, and an unescaped `&` produces a package no reader will open.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

/// A master's `p:txStyles`, or an empty string when it states no tier.
///
/// Nothing is written for an empty tier set rather than an empty `p:txStyles`:
/// a master that carries none and one that carries an empty one inherit
/// identically, so writing the element would be a diff in every package for no
/// change in meaning.
pub(crate) fn text_styles_xml(styles: &TextStyles) -> String {
    if styles.is_empty() {
        return String::new();
    }
    let mut xml = String::from("<p:txStyles>");
    // Schema order is title, body, other, and all three are REQUIRED once
    // `p:txStyles` is present — so an empty tier is written as an empty element
    // rather than omitted, which is the one place in this writer where an empty
    // element is the correct output.
    for (tag, tier) in [
        ("p:titleStyle", &styles.title),
        ("p:bodyStyle", &styles.body),
        ("p:otherStyle", &styles.other),
    ] {
        let levels = list_style_xml(tier, "a:lvl");
        if levels.is_empty() {
            xml.push_str(&format!("<{tag}/>"));
        } else {
            xml.push_str(&format!("<{tag}>{levels}</{tag}>"));
        }
    }
    xml.push_str("</p:txStyles>");
    xml
}

/// The `a:lvl1pPr` … `a:lvl9pPr` children of a `CT_TextListStyle`.
///
/// `prefix` is the element prefix, always `a:lvl` — taken as a parameter so the
/// one-based element names are built in one place rather than at each call site.
/// The model's levels are ZERO-based (`a:lvl1pPr` is level 0), and the two forms
/// never share a variable here.
pub(crate) fn list_style_xml(style: &ListStyle, prefix: &str) -> String {
    let mut xml = String::new();
    for (zero_based, level) in style.levels.iter().enumerate() {
        let Some(properties) = level else {
            continue;
        };
        let tag = format!("{prefix}{}pPr", zero_based + 1);
        xml.push_str(&paragraph_properties_xml(&tag, properties, false));
    }
    xml
}

/// One `a:pPr` or `a:lvlNpPr`.
///
/// `with_level` decides whether `@lvl` is written: a tier's `a:lvlNpPr` states its
/// level in its own ELEMENT NAME, so writing `@lvl` as well would be redundant at
/// best and contradictory at worst. A paragraph's `a:pPr` is the only place the
/// attribute belongs.
fn paragraph_properties_xml(
    tag: &str,
    properties: &TextParagraphProperties,
    with_level: bool,
) -> String {
    let mut attributes = String::new();
    if let Some(level) = properties.level.filter(|_| with_level) {
        attributes.push_str(&format!(r#" lvl="{level}""#));
    }
    if let Some(margin) = properties.margin_left_emu {
        attributes.push_str(&format!(r#" marL="{margin}""#));
    }
    if let Some(margin) = properties.margin_right_emu {
        attributes.push_str(&format!(r#" marR="{margin}""#));
    }
    if let Some(indent) = properties.indent_emu {
        attributes.push_str(&format!(r#" indent="{indent}""#));
    }
    if let Some(alignment) = properties.alignment {
        attributes.push_str(&format!(r#" algn="{}""#, alignment.token()));
    }
    if let Some(tab) = properties.default_tab_emu {
        attributes.push_str(&format!(r#" defTabSz="{tab}""#));
    }
    if let Some(rtl) = properties.right_to_left {
        attributes.push_str(&format!(r#" rtl="{}""#, u8::from(rtl)));
    }

    // `CT_TextParagraphProperties` child order, which a reader may enforce:
    // a:lnSpc, a:spcBef, a:spcAft, the bullet colour/size/font group, the bullet
    // itself, a:tabLst, a:defRPr.
    let mut children = String::new();
    for (element, spacing) in [
        ("a:lnSpc", properties.line_spacing.as_ref()),
        ("a:spcBef", properties.space_before.as_ref()),
        ("a:spcAft", properties.space_after.as_ref()),
    ] {
        if let Some(spacing) = spacing {
            children.push_str(&format!("<{element}>{}</{element}>", spacing_xml(spacing)));
        }
    }
    if let Some(color) = properties.bullet_color.as_ref() {
        children.push_str(&format!(
            "<a:buClr>{}</a:buClr>",
            crate::shapes::style_color_xml(color)
        ));
    }
    if let Some(percent) = properties.bullet_size_percent {
        children.push_str(&format!(r#"<a:buSzPct val="{percent}"/>"#));
    }
    if let Some(font) = properties.bullet_font.as_ref() {
        children.push_str(&typeface_xml("a:buFont", font));
    }
    if let Some(bullet) = properties.bullet.as_ref() {
        children.push_str(&bullet_xml(bullet));
    }
    if !properties.tab_stops.is_empty() {
        children.push_str("<a:tabLst>");
        for stop in &properties.tab_stops {
            children.push_str(&format!(
                r#"<a:tab pos="{}" algn="{}"/>"#,
                stop.position_emu,
                stop.alignment.token()
            ));
        }
        children.push_str("</a:tabLst>");
    }
    if let Some(character) = properties.default_character.as_deref() {
        children.push_str(&character_properties_xml("a:defRPr", character));
    }

    if children.is_empty() {
        // Self-closing when there are no children, which is the form real files
        // use — and the form whose attributes this engine once dropped on read.
        format!("<{tag}{attributes}/>")
    } else {
        format!("<{tag}{attributes}>{children}</{tag}>")
    }
}

/// An `a:lnSpc`/`a:spcBef`/`a:spcAft` child: a percentage or a point value.
fn spacing_xml(spacing: &TextSpacing) -> String {
    match spacing {
        // Thousandths of a percent and hundredths of a point: two different units
        // on two different elements, which is why the model names the field after
        // the unit rather than calling both `value`.
        TextSpacing::Percent { thousandths } => format!(r#"<a:spcPct val="{thousandths}"/>"#),
        TextSpacing::Points { hundredths } => format!(r#"<a:spcPts val="{hundredths}"/>"#),
    }
}

/// An `a:buNone`/`a:buChar`/`a:buAutoNum`.
///
/// `a:buNone` is NOT the absence of a bullet: it is an explicit statement that
/// this level has none, which beats an inherited bullet from the tier above. The
/// model distinguishes the two and so does this.
fn bullet_xml(bullet: &TextBullet) -> String {
    match bullet {
        TextBullet::None => "<a:buNone/>".to_owned(),
        TextBullet::Character { character, font } => {
            let mut xml = String::new();
            if let Some(font) = font {
                xml.push_str(&typeface_xml("a:buFont", font));
            }
            xml.push_str(&format!(r#"<a:buChar char="{}"/>"#, escape(character)));
            xml
        }
        TextBullet::AutoNumber { scheme, start_at } => {
            let mut xml = format!(r#"<a:buAutoNum type="{}""#, scheme.token());
            if let Some(start) = start_at {
                xml.push_str(&format!(r#" startAt="{start}""#));
            }
            xml.push_str("/>");
            xml
        }
    }
}

/// An `a:latin`/`a:ea`/`a:cs`/`a:buFont` typeface reference.
///
/// A `+mj-lt`/`+mn-lt` name is written back verbatim: it names the theme's
/// major/minor font, and substituting a resolved family would turn a theme
/// reference into an authored font — which survives a reopen but no longer
/// follows the theme.
fn typeface_xml(tag: &str, typeface: &Typeface) -> String {
    let mut xml = format!(r#"<{tag} typeface="{}""#, escape(&typeface.name));
    if let Some(panose) = typeface.panose.as_deref() {
        xml.push_str(&format!(r#" panose="{}""#, escape(panose)));
    }
    xml.push_str("/>");
    xml
}

/// One `a:rPr`, `a:defRPr` or `a:endParaRPr`.
pub(crate) fn character_properties_xml(tag: &str, properties: &TextCharacterProperties) -> String {
    let mut attributes = String::new();
    if let Some(language) = properties.language.as_deref() {
        attributes.push_str(&format!(r#" lang="{}""#, escape(language)));
    }
    if let Some(size) = properties.size_hundredths_point {
        attributes.push_str(&format!(r#" sz="{size}""#));
    }
    if let Some(bold) = properties.bold {
        attributes.push_str(&format!(r#" b="{}""#, u8::from(bold)));
    }
    if let Some(italic) = properties.italic {
        attributes.push_str(&format!(r#" i="{}""#, u8::from(italic)));
    }
    if let Some(underline) = properties.underline {
        attributes.push_str(&format!(r#" u="{}""#, underline.token()));
    }
    if let Some(strike) = properties.strike {
        attributes.push_str(&format!(r#" strike="{}""#, strike.token()));
    }
    if let Some(caps) = properties.caps {
        attributes.push_str(&format!(r#" cap="{}""#, caps.token()));
    }
    if let Some(spacing) = properties.spacing_hundredths_point {
        attributes.push_str(&format!(r#" spc="{spacing}""#));
    }
    if let Some(baseline) = properties.baseline_percent {
        attributes.push_str(&format!(r#" baseline="{baseline}""#));
    }
    if properties.dirty {
        attributes.push_str(r#" dirty="1""#);
    }

    // `CT_TextCharacterProperties` child order: the fill group, then the three
    // typeface slots.
    let mut children = String::new();
    if let Some(fill) = properties.fill.as_ref() {
        children.push_str(&format!(
            "<a:solidFill>{}</a:solidFill>",
            crate::shapes::style_color_xml(fill)
        ));
    }
    for (element, typeface) in [
        ("a:latin", properties.latin.as_ref()),
        ("a:ea", properties.east_asian.as_ref()),
        ("a:cs", properties.complex_script.as_ref()),
    ] {
        if let Some(typeface) = typeface {
            children.push_str(&typeface_xml(element, typeface));
        }
    }

    if children.is_empty() {
        format!("<{tag}{attributes}/>")
    } else {
        format!("<{tag}{attributes}>{children}</{tag}>")
    }
}

/// A shape's `a:txBody`.
pub(crate) fn text_body_xml(body: &TextBody) -> String {
    let mut xml = String::from("<p:txBody>");
    xml.push_str(&body_properties_xml(body));
    // `a:lstStyle` is required by `CT_TextBody` and is written empty when the
    // shape states no level, which is what every real file does.
    let levels = list_style_xml(&body.list_style, "a:lvl");
    if levels.is_empty() {
        xml.push_str("<a:lstStyle/>");
    } else {
        xml.push_str(&format!("<a:lstStyle>{levels}</a:lstStyle>"));
    }
    for paragraph in &body.paragraphs {
        xml.push_str(&paragraph_xml(paragraph));
    }
    xml.push_str("</p:txBody>");
    xml
}

/// `a:bodyPr`.
///
/// The insets are written unconditionally rather than omitted at their schema
/// defaults, because the model stores them as plain `i64` and so cannot tell an
/// absent `lIns` from an authored `91440`. Writing them is the direction that
/// cannot lose an authored value; the cost is a wider element than the source had.
fn body_properties_xml(body: &TextBody) -> String {
    let properties = &body.body_properties;
    let mut attributes = format!(
        r#" lIns="{}" tIns="{}" rIns="{}" bIns="{}""#,
        properties.inset_left_emu,
        properties.inset_top_emu,
        properties.inset_right_emu,
        properties.inset_bottom_emu
    );
    if let Some(rotation) = properties.rotation {
        attributes.push_str(&format!(r#" rot="{rotation}""#));
    }
    attributes.push_str(&format!(r#" vert="{}""#, properties.vertical.token()));
    attributes.push_str(&format!(r#" wrap="{}""#, properties.wrap.token()));
    attributes.push_str(&format!(r#" anchor="{}""#, properties.anchor.token()));
    if properties.anchor_center {
        attributes.push_str(r#" anchorCtr="1""#);
    }

    // The autofit choice is a CHILD of `a:bodyPr`. `TextAutoFit::None` is the
    // model's default and stands for BOTH an absent choice and an explicit
    // `a:noAutofit`, which the model does not distinguish — so `a:noAutofit` is
    // written for it. That is the direction that cannot lose an authored
    // statement; the cost is that a body which said nothing comes back saying it
    // does not resize, which is also the behaviour it had.
    let autofit = match properties.auto_fit {
        TextAutoFit::None => "<a:noAutofit/>".to_owned(),
        TextAutoFit::Shape => "<a:spAutoFit/>".to_owned(),
        TextAutoFit::Normal {
            font_scale,
            line_space_reduction,
        } => {
            let mut xml = String::from("<a:normAutofit");
            if let Some(scale) = font_scale {
                xml.push_str(&format!(r#" fontScale="{scale}""#));
            }
            if let Some(reduction) = line_space_reduction {
                xml.push_str(&format!(r#" lnSpcReduction="{reduction}""#));
            }
            xml.push_str("/>");
            xml
        }
    };

    format!("<a:bodyPr{attributes}>{autofit}</a:bodyPr>")
}

/// One `a:p`.
fn paragraph_xml(paragraph: &TextParagraph) -> String {
    let mut xml = String::from("<a:p>");
    if let Some(properties) = paragraph.properties.as_deref() {
        xml.push_str(&paragraph_properties_xml("a:pPr", properties, true));
    }
    for run in &paragraph.runs {
        match run {
            TextRun::Run(text) => {
                xml.push_str("<a:r>");
                if let Some(properties) = text.properties.as_deref() {
                    xml.push_str(&character_properties_xml("a:rPr", properties));
                }
                // `a:t` is written even for empty text: `CT_TextRun` requires it,
                // and a run with no `a:t` is a package a reader rejects.
                xml.push_str(&format!("<a:t>{}</a:t>", escape(&text.text)));
                xml.push_str("</a:r>");
            }
            TextRun::LineBreak(line_break) => match line_break.properties.as_deref() {
                Some(properties) => xml.push_str(&format!(
                    "<a:br>{}</a:br>",
                    character_properties_xml("a:rPr", properties)
                )),
                None => xml.push_str("<a:br/>"),
            },
            TextRun::Field(field) => {
                xml.push_str(&format!(
                    r#"<a:fld id="{}" type="{}">"#,
                    escape(&field.field_id),
                    escape(&field.kind)
                ));
                if let Some(properties) = field.properties.as_deref() {
                    xml.push_str(&character_properties_xml("a:rPr", properties));
                }
                // The CACHED text, which is what every consumer but PowerPoint
                // shows: a slide number or a date field renders from this cache
                // outside the authoring application, so dropping it blanks the
                // field everywhere else.
                xml.push_str(&format!("<a:t>{}</a:t>", escape(&field.text)));
                xml.push_str("</a:fld>");
            }
        }
    }
    if let Some(properties) = paragraph.end_properties.as_deref() {
        xml.push_str(&character_properties_xml("a:endParaRPr", properties));
    }
    xml.push_str("</a:p>");
    xml
}
