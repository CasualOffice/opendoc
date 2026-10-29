//! CommonMark + GFM import: `pulldown-cmark`'s events into the
//! [`FlowBuilder`](crate::flow_builder::FlowBuilder).
//!
//! # Why this exists
//!
//! `docs/153` `shell.import-markdown`: ONLYOFFICE opens Markdown (its
//! `apps/documenteditor/main/app/view/FileMenuPanels.js` lists `MD` among the
//! formats it converts on open), and a person handed a README expects a
//! document, not a text file full of hashes. The writer beside this module has
//! existed since `shell.export-markdown`; this is the reader.
//!
//! # What maps to what
//!
//! | Markdown | Model |
//! | --- | --- |
//! | `#`..`######` | "Heading 1".."Heading 6" |
//! | `-`, `*`, `1.` lists | numbered / bulleted paragraphs at their level |
//! | `- [ ]` / `- [x]` | a checkbox content control (`w14:checkbox`) |
//! | GFM table | a table, header row marked, column alignment on its cells |
//! | fenced / indented code | "HTML Preformatted" paragraphs, one per line |
//! | `` `code` `` | the "HTML Code" character style |
//! | `>` | the "Quote" style |
//! | `---` | an empty paragraph with a bottom border |
//! | `[text](url)` | a hyperlink (safe schemes only) |
//! | `![alt](data:...)` | an inline picture |
//! | `*`, `**`, `~~` | italic, bold, strike |
//! | `\` / two-space break | a line break |
//! | `[^1]` | a footnote |
//!
//! Everything else is **reported**, never silently dropped: raw HTML (never
//! executed, never parsed as markup here), a remote picture (never fetched:
//! the alt text stays as a link to it), a local picture no host resolved,
//! front matter, an unsafe link scheme.
//!
//! # Bounds
//!
//! The input is refused over [`MarkdownLimits::max_input_bytes`] before the
//! parser sees it, and the walk refuses past
//! [`MarkdownLimits::max_nesting_depth`] container levels (lists, items,
//! quotes, footnotes, tables), the same refuse-don't-truncate rule the other
//! importers follow. O(input).

use pulldown_cmark::{
    Alignment as MdAlignment, CodeBlockKind, Event, HeadingLevel, LinkType, Options, Parser, Tag,
    TagEnd,
};

use casual_doc_model::v1::Alignment;

use crate::flow_builder::{
    BlockKind, FlowBuilder, Imported, Link, UrlKind, classify_url, decode_data_uri, is_remote,
};
use crate::{AdapterError, MarkdownLimits, ModelOutcome};

/// A host's lookup for a picture a Markdown file names by a relative path
/// (`![](images/a.png)`): the bytes, or `None` when it has none. The importer
/// never touches a filesystem or a network itself.
pub type LocalImageResolver = dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync;

struct ImageState {
    destination: String,
    alt: String,
}

/// Imports `text` as CommonMark with the GFM extensions. O(text).
pub(crate) fn import_markdown(
    text: &str,
    limits: MarkdownLimits,
    resolver: Option<&LocalImageResolver>,
    namespace: u64,
) -> Result<Imported, AdapterError> {
    let mut options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    options.insert(Options::ENABLE_GFM);
    let mut builder = FlowBuilder::new(namespace, "markdown.import");
    let mut depth = 0usize;
    let mut image: Option<ImageState> = None;
    let mut code: Option<String> = None;
    let mut html_block = false;
    let mut metadata = false;
    let mut links: Vec<bool> = Vec::new();
    let mut in_head = false;

    for event in Parser::new_ext(text, options) {
        // Inside a picture, everything is its alternative text.
        if let Some(state) = image.as_mut() {
            match event {
                Event::Text(value) | Event::Code(value) => state.alt.push_str(&value),
                Event::SoftBreak | Event::HardBreak => state.alt.push(' '),
                Event::End(TagEnd::Image) => {
                    let state = image.take().expect("an open picture");
                    place_image(&mut builder, state, resolver)?;
                }
                _ => {}
            }
            continue;
        }
        if let Some(buffer) = code.as_mut() {
            match event {
                Event::Text(value) => buffer.push_str(&value),
                Event::End(TagEnd::CodeBlock) => {
                    let mut content = code.take().expect("an open code block");
                    if content.ends_with('\n') {
                        content.pop();
                    }
                    builder.text(&content)?;
                    builder.end_block()?;
                }
                _ => {}
            }
            continue;
        }
        match event {
            Event::Start(tag) => {
                if is_container(&tag) {
                    depth += 1;
                    if depth > limits.max_nesting_depth {
                        return Err(AdapterError::new(format!(
                            "limit markdown_nesting_depth observed {depth} exceeds allowed {}",
                            limits.max_nesting_depth
                        )));
                    }
                }
                start(
                    &mut builder,
                    tag,
                    &mut StartState {
                        image: &mut image,
                        code: &mut code,
                        html_block: &mut html_block,
                        metadata: &mut metadata,
                        links: &mut links,
                        in_head: &mut in_head,
                    },
                )?;
            }
            Event::End(tag) => {
                if is_container_end(tag) {
                    depth = depth.saturating_sub(1);
                }
                end(
                    &mut builder,
                    tag,
                    &mut links,
                    &mut html_block,
                    &mut metadata,
                    &mut in_head,
                )?;
            }
            Event::Text(value) => {
                if metadata {
                    continue;
                }
                if html_block {
                    continue;
                }
                builder.text(&value)?;
            }
            Event::Code(value) => {
                builder.push_format(|format| format.code = true);
                builder.text(&value)?;
                builder.pop_format();
            }
            Event::InlineMath(value) | Event::DisplayMath(value) => builder.text(&value)?,
            Event::Html(_) => {
                // An HTML block arrives as one `Html` event per line; it is
                // counted once, at its start tag.
            }
            Event::InlineHtml(value) => {
                let tag = value.trim().to_ascii_lowercase();
                if matches!(tag.as_str(), "<br>" | "<br/>" | "<br />") {
                    builder.line_break()?;
                } else {
                    builder.report("raw_html_inline", ModelOutcome::Omitted);
                }
            }
            Event::FootnoteReference(label) => builder.note_reference(&label)?,
            Event::SoftBreak => builder.text(" ")?,
            Event::HardBreak => builder.line_break()?,
            Event::Rule => builder.rule()?,
            Event::TaskListMarker(checked) => builder.checkbox(checked)?,
        }
    }
    builder.finish()
}

fn is_container(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::List(_) | Tag::Item | Tag::BlockQuote(_) | Tag::FootnoteDefinition(_) | Tag::Table(_)
    )
}

fn is_container_end(tag: TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::BlockQuote(_)
            | TagEnd::FootnoteDefinition
            | TagEnd::Table
    )
}

struct StartState<'a> {
    image: &'a mut Option<ImageState>,
    code: &'a mut Option<String>,
    html_block: &'a mut bool,
    metadata: &'a mut bool,
    links: &'a mut Vec<bool>,
    in_head: &'a mut bool,
}

fn start(
    builder: &mut FlowBuilder,
    tag: Tag<'_>,
    state: &mut StartState<'_>,
) -> Result<(), AdapterError> {
    match tag {
        Tag::Paragraph => builder.paragraph_break()?,
        Tag::Heading { level, .. } => builder.begin_block(BlockKind::Heading(heading(level)))?,
        Tag::BlockQuote(kind) => {
            if kind.is_some() {
                // GFM's `> [!NOTE]` alert: a quote here, its colour and icon
                // are a renderer's.
                builder.report("alert_kind", ModelOutcome::Degraded);
            }
            builder.begin_quote()?;
        }
        Tag::CodeBlock(kind) => {
            if let CodeBlockKind::Fenced(info) = &kind
                && !info.trim().is_empty()
            {
                // The info string (`rust`, `json`) names a highlighter; the
                // model has no syntax highlighting to hand it to.
                builder.report("code_language", ModelOutcome::Omitted);
            }
            builder.begin_block(BlockKind::Code)?;
            *state.code = Some(String::new());
        }
        Tag::HtmlBlock => {
            builder.report("raw_html_block", ModelOutcome::Omitted);
            *state.html_block = true;
        }
        Tag::List(start) => builder.begin_list(
            start.is_some(),
            start.map_or(1, |value| u32::try_from(value).unwrap_or(u32::MAX)),
        )?,
        Tag::Item => builder.begin_item()?,
        Tag::FootnoteDefinition(label) => builder.begin_note(&label)?,
        Tag::DefinitionList | Tag::DefinitionListTitle | Tag::DefinitionListDefinition => {
            builder.paragraph_break()?;
        }
        Tag::Table(alignments) => builder.begin_table(
            alignments
                .into_iter()
                .map(|alignment| match alignment {
                    MdAlignment::None => None,
                    MdAlignment::Left => Some(Alignment::Start),
                    MdAlignment::Center => Some(Alignment::Center),
                    MdAlignment::Right => Some(Alignment::End),
                })
                .collect(),
        )?,
        Tag::TableHead => {
            *state.in_head = true;
            builder.begin_row(true)?;
        }
        Tag::TableRow => builder.begin_row(false)?,
        Tag::TableCell => builder.begin_cell(None, 1)?,
        Tag::Emphasis => builder.push_format(|format| format.italic = true),
        Tag::Strong => builder.push_format(|format| format.bold = true),
        Tag::Strikethrough => builder.push_format(|format| format.strike = true),
        Tag::Superscript => builder.push_format(|format| format.superscript = true),
        Tag::Subscript => builder.push_format(|format| format.subscript = true),
        Tag::Link {
            link_type,
            dest_url,
            ..
        } => {
            let destination = if link_type == LinkType::Email && !dest_url.contains(':') {
                format!("mailto:{dest_url}")
            } else {
                dest_url.to_string()
            };
            match classify_url(&destination) {
                UrlKind::Internal(anchor) => {
                    builder.begin_link(Link::Internal(anchor));
                    state.links.push(true);
                }
                UrlKind::Safe(url) => {
                    builder.begin_link(Link::External(url));
                    state.links.push(true);
                }
                UrlKind::Unsafe => {
                    builder.report("link_unsafe_scheme", ModelOutcome::Degraded);
                    state.links.push(false);
                }
            }
        }
        Tag::Image { dest_url, .. } => {
            *state.image = Some(ImageState {
                destination: dest_url.to_string(),
                alt: String::new(),
            });
        }
        Tag::MetadataBlock(_) => {
            builder.report("front_matter", ModelOutcome::Omitted);
            *state.metadata = true;
        }
    }
    Ok(())
}

fn end(
    builder: &mut FlowBuilder,
    tag: TagEnd,
    links: &mut Vec<bool>,
    html_block: &mut bool,
    metadata: &mut bool,
    in_head: &mut bool,
) -> Result<(), AdapterError> {
    match tag {
        TagEnd::Paragraph => builder.paragraph_break()?,
        TagEnd::Heading(_) | TagEnd::CodeBlock => builder.end_block()?,
        TagEnd::BlockQuote(_) => builder.end_quote()?,
        TagEnd::HtmlBlock => *html_block = false,
        TagEnd::List(_) => builder.end_list()?,
        TagEnd::Item => builder.end_item()?,
        TagEnd::FootnoteDefinition => builder.end_note()?,
        TagEnd::DefinitionList | TagEnd::DefinitionListTitle | TagEnd::DefinitionListDefinition => {
            builder.paragraph_break()?;
        }
        TagEnd::Table => builder.end_table()?,
        TagEnd::TableHead => {
            *in_head = false;
            builder.end_row()?;
        }
        TagEnd::TableRow => builder.end_row()?,
        TagEnd::TableCell => builder.end_cell()?,
        TagEnd::Emphasis
        | TagEnd::Strong
        | TagEnd::Strikethrough
        | TagEnd::Superscript
        | TagEnd::Subscript => builder.pop_format(),
        TagEnd::Link => {
            if links.pop() == Some(true) {
                builder.end_link()?;
            }
        }
        TagEnd::Image => {}
        TagEnd::MetadataBlock(_) => *metadata = false,
    }
    Ok(())
}

fn heading(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// A picture: embedded from a `data:` URI or the host's resolver; a remote
/// one is never fetched, and its alt text is kept as a link to it so the
/// reference survives.
fn place_image(
    builder: &mut FlowBuilder,
    state: ImageState,
    resolver: Option<&LocalImageResolver>,
) -> Result<(), AdapterError> {
    let ImageState { destination, alt } = state;
    let destination = destination.trim().to_owned();
    if destination.to_ascii_lowercase().starts_with("data:") {
        if let Some((_, bytes)) = decode_data_uri(&destination)
            && builder.picture(bytes, &alt)?
        {
            return Ok(());
        }
        builder.report("image_unsupported", ModelOutcome::Degraded);
        return builder.text(&alt);
    }
    if is_remote(&destination) {
        builder.report("image_remote", ModelOutcome::Degraded);
        return alt_as_link(builder, &alt, &destination);
    }
    if let Some(resolve) = resolver
        && let Some(bytes) = resolve(&destination)
        && builder.picture(bytes, &alt)?
    {
        return Ok(());
    }
    builder.report("image_unresolved", ModelOutcome::Degraded);
    alt_as_link(builder, &alt, &destination)
}

fn alt_as_link(
    builder: &mut FlowBuilder,
    alt: &str,
    destination: &str,
) -> Result<(), AdapterError> {
    let label = if alt.trim().is_empty() {
        destination
    } else {
        alt
    };
    match classify_url(destination) {
        UrlKind::Safe(url) => {
            builder.begin_link(Link::External(url));
            builder.text(label)?;
            builder.end_link()
        }
        _ => builder.text(label),
    }
}

#[cfg(test)]
#[path = "markdown_import_tests.rs"]
mod tests;
