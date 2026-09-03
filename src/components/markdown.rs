use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    ops::Range,
    path::Path,
    str::FromStr,
    sync::OnceLock,
};

use gpui::{
    ClipboardItem, Div, FontStyle, FontWeight, Rgba, SharedString, StrikethroughStyle, StyledText,
    TextAlign, TextRun, UnderlineStyle, div, prelude::*, px,
};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag};
use two_face::{
    re_exports::syntect::{
        easy::ScopeRegionIterator,
        highlighting::ScopeSelectors,
        parsing::{MatchPower, ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet},
        util::LinesWithEndings,
    },
    syntax::extra_newlines,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    components::icons::icon,
    theme::{Theme, UI_MONOSPACE_FONT_FAMILY, ui_font},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MarkdownDocument {
    pub blocks: Vec<MarkdownBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MarkdownBlock {
    Paragraph(Vec<MarkdownInline>),
    Heading {
        level: u8,
        content: Vec<MarkdownInline>,
    },
    List {
        start: Option<u64>,
        items: Vec<MarkdownListItem>,
    },
    BlockQuote(Vec<MarkdownBlock>),
    HorizontalRule,
    Table {
        alignments: Vec<MarkdownAlignment>,
        header: Vec<MarkdownTableCell>,
        rows: Vec<Vec<MarkdownTableCell>>,
    },
    CodeBlock {
        language: Option<String>,
        code: String,
        fenced: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MarkdownListItem {
    pub checked: Option<bool>,
    pub blocks: Vec<MarkdownBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MarkdownTableCell {
    pub content: Vec<MarkdownInline>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarkdownAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MarkdownInline {
    Text(String),
    Strong(Vec<MarkdownInline>),
    Emphasis(Vec<MarkdownInline>),
    Strikethrough(Vec<MarkdownInline>),
    Code(String),
    Link {
        destination: String,
        title: String,
        content: Vec<MarkdownInline>,
    },
    SoftBreak,
    HardBreak,
}

struct RawFrame {
    tag: Tag<'static>,
    children: Vec<RawNode>,
}

enum RawNode {
    Element {
        tag: Tag<'static>,
        children: Vec<RawNode>,
    },
    Text(String),
    Code(String),
    SoftBreak,
    HardBreak,
    Rule,
    TaskListMarker(bool),
}

pub fn parse_markdown(source: &str) -> MarkdownDocument {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    let mut roots = Vec::new();
    let mut stack: Vec<RawFrame> = Vec::new();

    for event in Parser::new_ext(source, options).map(Event::into_static) {
        match event {
            Event::Start(tag) => stack.push(RawFrame {
                tag,
                children: Vec::new(),
            }),
            Event::End(end) => {
                if let Some(frame) = stack.pop() {
                    debug_assert_eq!(frame.tag.to_end(), end);
                    push_raw(
                        &mut roots,
                        &mut stack,
                        RawNode::Element {
                            tag: frame.tag,
                            children: frame.children,
                        },
                    );
                }
            }
            Event::Text(text)
            | Event::Html(text)
            | Event::InlineHtml(text)
            | Event::InlineMath(text)
            | Event::DisplayMath(text)
            | Event::FootnoteReference(text) => {
                push_raw(&mut roots, &mut stack, RawNode::Text(text.into_string()));
            }
            Event::Code(code) => {
                push_raw(&mut roots, &mut stack, RawNode::Code(code.into_string()));
            }
            Event::SoftBreak => push_raw(&mut roots, &mut stack, RawNode::SoftBreak),
            Event::HardBreak => push_raw(&mut roots, &mut stack, RawNode::HardBreak),
            Event::Rule => push_raw(&mut roots, &mut stack, RawNode::Rule),
            Event::TaskListMarker(checked) => {
                push_raw(&mut roots, &mut stack, RawNode::TaskListMarker(checked));
            }
        }
    }

    while let Some(frame) = stack.pop() {
        push_raw(
            &mut roots,
            &mut stack,
            RawNode::Element {
                tag: frame.tag,
                children: frame.children,
            },
        );
    }

    MarkdownDocument {
        blocks: raw_nodes_to_blocks(roots),
    }
}

fn push_raw(roots: &mut Vec<RawNode>, stack: &mut [RawFrame], node: RawNode) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        roots.push(node);
    }
}

fn raw_nodes_to_blocks(nodes: Vec<RawNode>) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let mut pending_inline = Vec::new();

    for node in nodes {
        if is_block_node(&node) {
            flush_pending_paragraph(&mut pending_inline, &mut blocks);
            append_block(node, &mut blocks);
        } else {
            pending_inline.push(node);
        }
    }
    flush_pending_paragraph(&mut pending_inline, &mut blocks);
    blocks
}

fn flush_pending_paragraph(pending: &mut Vec<RawNode>, blocks: &mut Vec<MarkdownBlock>) {
    if !pending.is_empty() {
        let content = raw_nodes_to_inlines(std::mem::take(pending));
        if !content.is_empty() {
            blocks.push(MarkdownBlock::Paragraph(content));
        }
    }
}

fn is_block_node(node: &RawNode) -> bool {
    match node {
        RawNode::Rule => true,
        RawNode::Element { tag, .. } => matches!(
            tag,
            Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::BlockQuote(_)
                | Tag::CodeBlock(_)
                | Tag::HtmlBlock
                | Tag::List(_)
                | Tag::Table(_)
                | Tag::FootnoteDefinition(_)
                | Tag::DefinitionList
                | Tag::DefinitionListTitle
                | Tag::DefinitionListDefinition
                | Tag::MetadataBlock(_)
        ),
        _ => false,
    }
}

fn append_block(node: RawNode, blocks: &mut Vec<MarkdownBlock>) {
    match node {
        RawNode::Rule => blocks.push(MarkdownBlock::HorizontalRule),
        RawNode::Element { tag, children } => match tag {
            Tag::Paragraph => blocks.push(MarkdownBlock::Paragraph(raw_nodes_to_inlines(children))),
            Tag::Heading { level, .. } => blocks.push(MarkdownBlock::Heading {
                level: level as u8,
                content: raw_nodes_to_inlines(children),
            }),
            Tag::BlockQuote(_) => {
                blocks.push(MarkdownBlock::BlockQuote(raw_nodes_to_blocks(children)))
            }
            Tag::CodeBlock(kind) => {
                let (language, fenced) = match kind {
                    CodeBlockKind::Indented => (None, false),
                    CodeBlockKind::Fenced(info) => (
                        info.split_whitespace().next().and_then(|language| {
                            (!language.is_empty()).then(|| language.to_owned())
                        }),
                        true,
                    ),
                };
                blocks.push(MarkdownBlock::CodeBlock {
                    language,
                    code: collect_raw_text(children),
                    fenced,
                });
            }
            Tag::List(start) => {
                let items = children
                    .into_iter()
                    .filter_map(|child| match child {
                        RawNode::Element {
                            tag: Tag::Item,
                            mut children,
                        } => {
                            let checked = take_task_marker(&mut children);
                            Some(MarkdownListItem {
                                checked,
                                blocks: raw_nodes_to_blocks(children),
                            })
                        }
                        _ => None,
                    })
                    .collect();
                blocks.push(MarkdownBlock::List { start, items });
            }
            Tag::Table(alignments) => {
                let alignments = alignments.into_iter().map(map_alignment).collect();
                let mut header = Vec::new();
                let mut rows = Vec::new();
                for child in children {
                    if let RawNode::Element { tag, children } = child {
                        match tag {
                            Tag::TableHead => header = table_cells(children),
                            Tag::TableRow => rows.push(table_cells(children)),
                            _ => {}
                        }
                    }
                }
                blocks.push(MarkdownBlock::Table {
                    alignments,
                    header,
                    rows,
                });
            }
            _ => blocks.extend(raw_nodes_to_blocks(children)),
        },
        other => {
            let content = raw_nodes_to_inlines(vec![other]);
            if !content.is_empty() {
                blocks.push(MarkdownBlock::Paragraph(content));
            }
        }
    }
}

fn map_alignment(alignment: Alignment) -> MarkdownAlignment {
    match alignment {
        Alignment::None => MarkdownAlignment::None,
        Alignment::Left => MarkdownAlignment::Left,
        Alignment::Center => MarkdownAlignment::Center,
        Alignment::Right => MarkdownAlignment::Right,
    }
}

fn table_cells(nodes: Vec<RawNode>) -> Vec<MarkdownTableCell> {
    nodes
        .into_iter()
        .filter_map(|node| match node {
            RawNode::Element {
                tag: Tag::TableCell,
                children,
            } => Some(MarkdownTableCell {
                content: raw_nodes_to_inlines(children),
            }),
            _ => None,
        })
        .collect()
}

fn take_task_marker(nodes: &mut Vec<RawNode>) -> Option<bool> {
    let mut index = 0;
    while index < nodes.len() {
        if matches!(nodes[index], RawNode::TaskListMarker(_)) {
            if let RawNode::TaskListMarker(checked) = nodes.remove(index) {
                return Some(checked);
            }
        }
        if let RawNode::Element { tag, children } = &mut nodes[index]
            && matches!(
                tag,
                Tag::Paragraph
                    | Tag::Strong
                    | Tag::Emphasis
                    | Tag::Strikethrough
                    | Tag::Link { .. }
            )
            && let Some(checked) = take_task_marker(children)
        {
            return Some(checked);
        }
        index += 1;
    }
    None
}

fn collect_raw_text(nodes: Vec<RawNode>) -> String {
    let mut text = String::new();
    for node in nodes {
        match node {
            RawNode::Text(value) | RawNode::Code(value) => text.push_str(&value),
            RawNode::SoftBreak | RawNode::HardBreak => text.push('\n'),
            RawNode::Element { children, .. } => text.push_str(&collect_raw_text(children)),
            RawNode::Rule => text.push_str("---"),
            RawNode::TaskListMarker(checked) => {
                text.push_str(if checked { "[x] " } else { "[ ] " });
            }
        }
    }
    text
}

fn raw_nodes_to_inlines(nodes: Vec<RawNode>) -> Vec<MarkdownInline> {
    let mut inlines = Vec::new();
    for node in nodes {
        match node {
            RawNode::Text(text) => push_inline(&mut inlines, MarkdownInline::Text(text)),
            RawNode::Code(code) => inlines.push(MarkdownInline::Code(code)),
            RawNode::SoftBreak => inlines.push(MarkdownInline::SoftBreak),
            RawNode::HardBreak => inlines.push(MarkdownInline::HardBreak),
            RawNode::Rule => push_inline(&mut inlines, MarkdownInline::Text("—".to_owned())),
            RawNode::TaskListMarker(checked) => push_inline(
                &mut inlines,
                MarkdownInline::Text(if checked { "[x] " } else { "[ ] " }.to_owned()),
            ),
            RawNode::Element { tag, children } => match tag {
                Tag::Strong => inlines.push(MarkdownInline::Strong(raw_nodes_to_inlines(children))),
                Tag::Emphasis => {
                    inlines.push(MarkdownInline::Emphasis(raw_nodes_to_inlines(children)))
                }
                Tag::Strikethrough => inlines.push(MarkdownInline::Strikethrough(
                    raw_nodes_to_inlines(children),
                )),
                Tag::Link {
                    dest_url, title, ..
                } => inlines.push(MarkdownInline::Link {
                    destination: dest_url.into_string(),
                    title: title.into_string(),
                    content: raw_nodes_to_inlines(children),
                }),
                Tag::Image {
                    dest_url, title, ..
                } => inlines.push(MarkdownInline::Link {
                    destination: dest_url.into_string(),
                    title: title.into_string(),
                    content: raw_nodes_to_inlines(children),
                }),
                _ => {
                    for inline in raw_nodes_to_inlines(children) {
                        push_inline(&mut inlines, inline);
                    }
                }
            },
        }
    }
    inlines
}

fn push_inline(inlines: &mut Vec<MarkdownInline>, inline: MarkdownInline) {
    if let MarkdownInline::Text(text) = inline {
        if let Some(MarkdownInline::Text(previous)) = inlines.last_mut() {
            previous.push_str(&text);
        } else if !text.is_empty() {
            inlines.push(MarkdownInline::Text(text));
        }
    } else {
        inlines.push(inline);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MarkdownLayout {
    base_size: f32,
    base_line_height: f32,
    paragraph_space: f32,
    heading_top: f32,
    list_padding: f32,
    list_item_padding: f32,
    quote_bottom: f32,
    quote_padding_y: f32,
    quote_padding_left: f32,
    quote_line_height: f32,
    quote_bar_width: f32,
    rule_margin: f32,
    inline_code_size: f32,
    inline_code_line_height: f32,
    inline_code_padding_x: f32,
    inline_code_padding_y: f32,
    inline_code_radius: f32,
    code_margin: f32,
    code_radius: f32,
    code_header_size: f32,
    code_header_line_height: f32,
    code_header_padding_x: f32,
    code_header_padding_right: f32,
    code_header_padding_y: f32,
    code_body_padding_x: f32,
    code_body_padding_bottom: f32,
    code_size: f32,
    code_line_height: f32,
    table_size: f32,
    table_line_height: f32,
    table_header_line_height: f32,
    table_header_padding_y: f32,
    table_cell_padding_y: f32,
    table_cell_padding_right: f32,
    table_header_last_padding_right: f32,
    table_body_last_padding_bottom: f32,
    table_min_width: f32,
    table_breakout_width: f32,
    table_cell_max_width: f32,
}

const CHATGPT_MARKDOWN_LAYOUT: MarkdownLayout = MarkdownLayout {
    base_size: 14.0,
    base_line_height: 22.75,
    paragraph_space: 3.5,
    heading_top: 14.0,
    list_padding: 22.75,
    list_item_padding: 5.25,
    quote_bottom: 7.0,
    quote_padding_y: 7.0,
    quote_padding_left: 21.0,
    quote_line_height: 21.0,
    quote_bar_width: 3.5,
    rule_margin: 24.5,
    inline_code_size: 12.25,
    // The browser's inline box is 18.6875px tall. GPUI boxes include
    // padding in layout, so this keeps the visual box at that height while
    // the containing line remains 22.75px.
    inline_code_line_height: 14.5,
    inline_code_padding_x: 4.2,
    inline_code_padding_y: 2.1,
    inline_code_radius: 7.5,
    code_margin: 17.5,
    code_radius: 20.0,
    code_header_size: 13.0,
    code_header_line_height: 18.5714,
    code_header_padding_x: 20.0,
    code_header_padding_right: 6.0,
    code_header_padding_y: 6.0,
    code_body_padding_x: 20.0,
    code_body_padding_bottom: 12.0,
    code_size: 12.0,
    code_line_height: 20.0,
    table_size: 12.25,
    table_line_height: 22.75,
    table_header_line_height: 14.0,
    table_header_padding_y: 7.0,
    table_cell_padding_y: 8.75,
    table_cell_padding_right: 21.0,
    table_header_last_padding_right: 35.0,
    table_body_last_padding_bottom: 21.0,
    table_min_width: 736.0,
    table_breakout_width: 1024.0,
    table_cell_max_width: 576.0,
};

#[derive(Clone, Copy, Debug, PartialEq)]
struct MarkdownPalette {
    text: Rgba,
    link: Rgba,
    file_link: Rgba,
    inline_code_text: Rgba,
    inline_code_surface: Rgba,
    code_surface: Rgba,
    code_header_surface: Rgba,
    code_border: Rgba,
    syntax_comment: Rgba,
    syntax_keyword: Rgba,
    syntax_literal: Rgba,
    syntax_string: Rgba,
    syntax_variable: Rgba,
    syntax_attribute: Rgba,
    syntax_name: Rgba,
    syntax_error: Rgba,
    action_hover: Rgba,
    blockquote_border: Rgba,
    table_border_strong: Rgba,
    table_border_subtle: Rgba,
    table_header_surface: Rgba,
    rule: Rgba,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MarkdownRenderStyle {
    layout: MarkdownLayout,
    palette: MarkdownPalette,
}

impl MarkdownRenderStyle {
    fn new(theme: Theme) -> Self {
        Self {
            layout: CHATGPT_MARKDOWN_LAYOUT,
            palette: MarkdownPalette {
                text: theme.markdown_text,
                link: theme.markdown_link,
                file_link: theme.markdown_file_link,
                inline_code_text: theme.markdown_inline_code_text,
                inline_code_surface: theme.markdown_inline_code_surface,
                code_surface: theme.markdown_code_surface,
                code_header_surface: theme.markdown_code_header_surface,
                code_border: theme.markdown_code_border,
                syntax_comment: theme.markdown_syntax_comment,
                syntax_keyword: theme.markdown_syntax_keyword,
                syntax_literal: theme.markdown_syntax_literal,
                syntax_string: theme.markdown_syntax_string,
                syntax_variable: theme.markdown_syntax_variable,
                syntax_attribute: theme.markdown_syntax_attribute,
                syntax_name: theme.markdown_syntax_name,
                syntax_error: theme.markdown_syntax_error,
                action_hover: theme.markdown_action_hover,
                blockquote_border: theme.markdown_blockquote_border,
                table_border_strong: theme.markdown_table_border_strong,
                table_border_subtle: theme.markdown_table_border_subtle,
                table_header_surface: theme.markdown_table_header_surface,
                rule: theme.markdown_rule,
            },
        }
    }
}

#[derive(Clone, Copy)]
enum SequenceContext {
    Root,
    BlockQuote,
    ListItem,
}

pub fn render_assistant_markdown(source: &str, theme: Theme, message_scope: &str) -> Div {
    let document = parse_markdown(source);
    render_markdown_document(&document, theme, markdown_hash(message_scope))
}

fn render_markdown_document(document: &MarkdownDocument, theme: Theme, identity_seed: u64) -> Div {
    let style = MarkdownRenderStyle::new(theme);
    render_block_sequence(
        &document.blocks,
        style,
        0,
        SequenceContext::Root,
        identity_seed,
    )
    .w_full()
    .min_w(px(0.0))
    .text_size(px(style.layout.base_size))
    .line_height(px(style.layout.base_line_height))
    .font(ui_font())
    .font_weight(FontWeight::NORMAL)
    .text_color(style.palette.text)
}

fn render_block_sequence(
    blocks: &[MarkdownBlock],
    style: MarkdownRenderStyle,
    list_depth: usize,
    context: SequenceContext,
    identity_seed: u64,
) -> Div {
    let mut sequence = div().w_full().min_w(px(0.0)).flex().flex_col();
    let mut previous: Option<&MarkdownBlock> = None;
    let mut previous_bottom = 0.0_f32;

    for (index, block) in blocks.iter().enumerate() {
        let block_identity = markdown_hash(&(identity_seed, index));
        let (top, bottom) = block_margins(block, previous, index == 0, style.layout, context);
        let collapsed_gap = if index == 0 {
            0.0
        } else {
            previous_bottom.max(top)
        };
        sequence = sequence.child(
            div()
                .w_full()
                .min_w(px(0.0))
                .when(collapsed_gap > 0.0, |element| element.mt(px(collapsed_gap)))
                .child(render_block(
                    block,
                    style,
                    list_depth,
                    context,
                    block_identity,
                )),
        );
        previous = Some(block);
        previous_bottom = bottom;
    }
    sequence
}

fn block_margins(
    block: &MarkdownBlock,
    previous: Option<&MarkdownBlock>,
    is_first: bool,
    layout: MarkdownLayout,
    context: SequenceContext,
) -> (f32, f32) {
    let default = match block {
        MarkdownBlock::Paragraph(_) => (0.0, layout.paragraph_space),
        MarkdownBlock::Heading { level: 1, .. } => (0.0, 7.0),
        MarkdownBlock::Heading { level: 2 | 3, .. } => (layout.heading_top, 3.5),
        MarkdownBlock::Heading { level: 4, .. } => (layout.heading_top, 0.0),
        MarkdownBlock::Heading { .. } => (0.0, 0.0),
        MarkdownBlock::List { .. } | MarkdownBlock::Table { .. } => (0.0, 0.0),
        MarkdownBlock::BlockQuote(_) => (0.0, layout.quote_bottom),
        MarkdownBlock::HorizontalRule => (layout.rule_margin, layout.rule_margin),
        MarkdownBlock::CodeBlock { .. } => (layout.code_margin, layout.code_margin),
    };

    let (mut top, mut bottom) = match context {
        SequenceContext::Root => match block {
            MarkdownBlock::Paragraph(_)
                if matches!(previous, Some(MarkdownBlock::Paragraph(_))) =>
            {
                (14.0, 14.0)
            }
            MarkdownBlock::Paragraph(_)
                if matches!(previous, Some(MarkdownBlock::Heading { level: 4, .. })) =>
            {
                (0.0, default.1)
            }
            MarkdownBlock::Paragraph(_) if !is_first => (7.0, default.1),
            _ => default,
        },
        SequenceContext::BlockQuote => match block {
            MarkdownBlock::Paragraph(_) => (0.0, 0.0),
            _ => default,
        },
        SequenceContext::ListItem => match block {
            MarkdownBlock::Paragraph(_)
                if matches!(previous, Some(MarkdownBlock::Paragraph(_))) =>
            {
                (14.0, 0.0)
            }
            MarkdownBlock::Paragraph(_) | MarkdownBlock::List { .. } => (0.0, 0.0),
            _ => default,
        },
    };
    if is_first {
        top = 0.0;
    }
    if !bottom.is_finite() {
        bottom = 0.0;
    }
    (top, bottom)
}

fn render_block(
    block: &MarkdownBlock,
    style: MarkdownRenderStyle,
    list_depth: usize,
    context: SequenceContext,
    block_identity: u64,
) -> Div {
    match block {
        MarkdownBlock::Paragraph(content) => render_inline_block(
            content,
            style,
            style.layout.base_size,
            if matches!(context, SequenceContext::BlockQuote) {
                style.layout.quote_line_height
            } else {
                style.layout.base_line_height
            },
            FontWeight::NORMAL,
            block_identity,
        ),
        MarkdownBlock::Heading { level, content } => {
            let (size, line_height) = match level {
                1 => (21.0, 28.0),
                2 => (17.5, 24.5),
                3 => (15.75, 24.5),
                4 => (14.0, 21.0),
                _ => (14.0, 22.75),
            };
            render_inline_block(
                content,
                style,
                size,
                line_height,
                FontWeight::SEMIBOLD,
                block_identity,
            )
        }
        MarkdownBlock::List { start, items } => {
            render_list(*start, items, style, list_depth, block_identity)
        }
        MarkdownBlock::BlockQuote(blocks) => div()
            .relative()
            .w_full()
            .min_w(px(0.0))
            .pl(px(style.layout.quote_padding_left))
            .py(px(style.layout.quote_padding_y))
            .line_height(px(style.layout.quote_line_height))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(style.layout.quote_padding_y))
                    .bottom(px(style.layout.quote_padding_y))
                    .w(px(style.layout.quote_bar_width))
                    .rounded(px(style.layout.quote_bar_width / 2.0))
                    .bg(style.palette.blockquote_border),
            )
            .child(render_block_sequence(
                blocks,
                style,
                list_depth,
                SequenceContext::BlockQuote,
                block_identity,
            )),
        MarkdownBlock::HorizontalRule => div()
            .w_full()
            .h_0()
            .border_t_1()
            .border_color(style.palette.rule),
        MarkdownBlock::CodeBlock { language, code, .. } => {
            render_code_block(language.as_deref(), code, style, block_identity)
        }
        MarkdownBlock::Table {
            alignments,
            header,
            rows,
        } => render_table(alignments, header, rows, style, block_identity),
    }
}

fn render_list(
    start: Option<u64>,
    items: &[MarkdownListItem],
    style: MarkdownRenderStyle,
    depth: usize,
    block_identity: u64,
) -> Div {
    let is_task_list = list_uses_task_layout(start, items);
    let mut list = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .when(!is_task_list, |list| list.pl(px(style.layout.list_padding)));

    for (index, item) in items.iter().enumerate() {
        let marker = if let Some(checked) = item.checked {
            if checked {
                "☑".to_owned()
            } else {
                "☐".to_owned()
            }
        } else if let Some(start) = start {
            format!("{}.", start.saturating_add(index as u64))
        } else {
            match depth % 3 {
                0 => "•".to_owned(),
                1 => "◦".to_owned(),
                _ => "▪".to_owned(),
            }
        };
        let marker_left = if is_task_list {
            0.0
        } else {
            -style.layout.list_padding
        };
        let content_padding = if is_task_list {
            style.layout.list_padding
        } else {
            style.layout.list_item_padding
        };
        list = list.child(
            div()
                .relative()
                .w_full()
                .min_w(px(0.0))
                .pl(px(content_padding))
                .child(
                    div()
                        .absolute()
                        .left(px(marker_left))
                        .top(px(if item.checked.is_some() {
                            style.layout.paragraph_space
                        } else {
                            0.0
                        }))
                        .w(px(style.layout.list_padding))
                        .pr(px(style.layout.list_item_padding))
                        .text_right()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(style.layout.base_size))
                        .line_height(px(style.layout.base_line_height))
                        .child(marker),
                )
                .child(render_block_sequence(
                    &item.blocks,
                    style,
                    depth + 1,
                    SequenceContext::ListItem,
                    markdown_hash(&(block_identity, index)),
                )),
        );
    }
    list
}

fn list_uses_task_layout(start: Option<u64>, items: &[MarkdownListItem]) -> bool {
    if start.is_none() {
        items.iter().any(|item| item.checked.is_some())
    } else {
        !items.is_empty() && items.iter().all(|item| item.checked.is_some())
    }
}

fn render_inline_block(
    content: &[MarkdownInline],
    style: MarkdownRenderStyle,
    font_size: f32,
    line_height: f32,
    font_weight: FontWeight,
    inline_identity: u64,
) -> Div {
    if requires_inline_boxes(content) {
        render_inline_boxes(
            content,
            style,
            font_size,
            line_height,
            font_weight,
            inline_identity,
        )
    } else {
        div()
            .w_full()
            .min_w(px(0.0))
            .text_size(px(font_size))
            .line_height(px(line_height))
            .font_weight(font_weight)
            .child(render_styled_text(content, style, font_weight))
    }
}

fn requires_inline_boxes(inlines: &[MarkdownInline]) -> bool {
    inlines.iter().any(|inline| match inline {
        MarkdownInline::Code(_) | MarkdownInline::Link { .. } => true,
        MarkdownInline::Strong(children)
        | MarkdownInline::Emphasis(children)
        | MarkdownInline::Strikethrough(children) => requires_inline_boxes(children),
        _ => false,
    })
}

#[derive(Clone, Copy, Default)]
struct InlineState {
    strong: bool,
    emphasis: bool,
    strikethrough: bool,
    code: bool,
    link: bool,
}

fn render_styled_text(
    inlines: &[MarkdownInline],
    style: MarkdownRenderStyle,
    base_weight: FontWeight,
) -> StyledText {
    let mut text = String::new();
    let mut runs = Vec::new();
    append_inline_runs(
        inlines,
        InlineState::default(),
        style,
        base_weight,
        &mut text,
        &mut runs,
    );
    StyledText::new(text).with_runs(runs)
}

fn append_inline_runs(
    inlines: &[MarkdownInline],
    state: InlineState,
    style: MarkdownRenderStyle,
    base_weight: FontWeight,
    text: &mut String,
    runs: &mut Vec<TextRun>,
) {
    for inline in inlines {
        match inline {
            MarkdownInline::Text(value) => {
                append_text_run(value, state, style, base_weight, text, runs)
            }
            MarkdownInline::Code(value) => {
                let mut next = state;
                next.code = true;
                append_text_run(value, next, style, base_weight, text, runs);
            }
            MarkdownInline::SoftBreak => {
                append_text_run(" ", state, style, base_weight, text, runs)
            }
            MarkdownInline::HardBreak => {
                append_text_run("\n", state, style, base_weight, text, runs)
            }
            MarkdownInline::Strong(children) => {
                let mut next = state;
                next.strong = true;
                append_inline_runs(children, next, style, base_weight, text, runs);
            }
            MarkdownInline::Emphasis(children) => {
                let mut next = state;
                next.emphasis = true;
                append_inline_runs(children, next, style, base_weight, text, runs);
            }
            MarkdownInline::Strikethrough(children) => {
                let mut next = state;
                next.strikethrough = true;
                append_inline_runs(children, next, style, base_weight, text, runs);
            }
            MarkdownInline::Link { content, .. } => {
                let mut next = state;
                next.link = true;
                append_inline_runs(content, next, style, base_weight, text, runs);
            }
        }
    }
}

fn append_text_run(
    value: &str,
    state: InlineState,
    style: MarkdownRenderStyle,
    base_weight: FontWeight,
    text: &mut String,
    runs: &mut Vec<TextRun>,
) {
    if value.is_empty() {
        return;
    }
    text.push_str(value);
    let mut font = ui_font();
    if state.code {
        font.family = UI_MONOSPACE_FONT_FAMILY.into();
    }
    font.weight = if state.strong {
        FontWeight::SEMIBOLD
    } else {
        base_weight
    };
    font.style = if state.emphasis {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    let color = if state.link {
        style.palette.link.into()
    } else if state.code {
        style.palette.inline_code_text.into()
    } else {
        style.palette.text.into()
    };
    runs.push(TextRun {
        len: value.len(),
        font,
        color,
        background_color: state.code.then(|| style.palette.inline_code_surface.into()),
        underline: None,
        strikethrough: state.strikethrough.then_some(StrikethroughStyle {
            thickness: px(1.0),
            color: None,
        }),
    });
}

struct InlineFragment {
    text: String,
    state: InlineState,
    hard_break: bool,
    trailing_space: bool,
    link_destination: Option<String>,
    link_leading: bool,
}

fn render_inline_boxes(
    inlines: &[MarkdownInline],
    style: MarkdownRenderStyle,
    font_size: f32,
    line_height: f32,
    base_weight: FontWeight,
    inline_identity: u64,
) -> Div {
    let mut fragments = Vec::new();
    append_inline_fragments(inlines, InlineState::default(), None, &mut fragments);
    fragments.into_iter().enumerate().fold(
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center(),
        |line, (index, fragment)| {
            if fragment.hard_break {
                return line.child(div().w_full().h_0());
            }
            let file_reference = fragment
                .link_destination
                .as_deref()
                .and_then(markdown_file_reference_path);
            let color = if file_reference.is_some() {
                style.palette.file_link
            } else if fragment.state.link {
                style.palette.link
            } else if fragment.state.code {
                style.palette.inline_code_text
            } else {
                style.palette.text
            };
            let weight = if fragment.state.strong {
                FontWeight::SEMIBOLD
            } else if fragment.state.code || fragment.state.link {
                FontWeight::MEDIUM
            } else {
                base_weight
            };
            let mut element = div()
                .flex_none()
                .max_w_full()
                .font_weight(weight)
                .text_color(color)
                .when(fragment.state.emphasis, |element| element.italic())
                .when(fragment.state.strikethrough, |element| {
                    element.line_through()
                })
                .when(fragment.state.code, |element| {
                    element
                        .font_family(UI_MONOSPACE_FONT_FAMILY)
                        .text_size(px(style.layout.inline_code_size))
                        .line_height(px(style.layout.inline_code_line_height))
                        .px(px(style.layout.inline_code_padding_x))
                        .py(px(style.layout.inline_code_padding_y))
                        .rounded(px(style.layout.inline_code_radius))
                        .bg(style.palette.inline_code_surface)
                })
                .when(!fragment.state.code, |element| {
                    element
                        .text_size(px(font_size))
                        .line_height(px(line_height))
                })
                .when(file_reference.is_some(), |element| {
                    element.px(px(2.0)).flex().items_center()
                })
                .when(fragment.trailing_space, |element| {
                    element.mr(px(style.layout.paragraph_space))
                });
            if let Some(file_reference) = file_reference
                && fragment.link_leading
            {
                element = element.child(
                    icon(markdown_file_reference_icon(file_reference), color.into())
                        .size(px(16.0))
                        .flex_none()
                        .mr(px(3.0)),
                );
            }
            let element = element.child(StyledText::new(fragment.text));
            if let Some(destination) = fragment.link_destination {
                let link_id = markdown_element_id(
                    "markdown-link",
                    &(inline_identity, index, destination.as_str()),
                );
                let file_reference = markdown_file_reference_path(&destination).map(str::to_owned);
                line.child(
                    element
                        .id(link_id)
                        .cursor_pointer()
                        .hover(|element| element.underline())
                        .on_click(move |_, _, cx| {
                            if let Some(file_reference) = &file_reference {
                                cx.open_with_system(Path::new(file_reference));
                            } else {
                                cx.open_url(&destination);
                            }
                        }),
                )
            } else {
                line.child(element)
            }
        },
    )
}

fn append_inline_fragments(
    inlines: &[MarkdownInline],
    state: InlineState,
    link_destination: Option<&str>,
    fragments: &mut Vec<InlineFragment>,
) {
    for inline in inlines {
        match inline {
            MarkdownInline::Text(text) => {
                for word in UnicodeSegmentation::split_word_bounds(text.as_str()) {
                    if word.chars().all(char::is_whitespace) {
                        if let Some(previous) = fragments.last_mut() {
                            previous.trailing_space = true;
                        }
                    } else {
                        fragments.push(InlineFragment {
                            text: word.to_owned(),
                            state,
                            hard_break: false,
                            trailing_space: false,
                            link_destination: link_destination.map(ToOwned::to_owned),
                            link_leading: false,
                        });
                    }
                }
            }
            MarkdownInline::Code(text) => {
                let mut next = state;
                next.code = true;
                fragments.push(InlineFragment {
                    text: text.clone(),
                    state: next,
                    hard_break: false,
                    trailing_space: false,
                    link_destination: link_destination.map(ToOwned::to_owned),
                    link_leading: false,
                });
            }
            MarkdownInline::SoftBreak => {
                if let Some(previous) = fragments.last_mut() {
                    previous.trailing_space = true;
                }
            }
            MarkdownInline::HardBreak => fragments.push(InlineFragment {
                text: String::new(),
                state,
                hard_break: true,
                trailing_space: false,
                link_destination: None,
                link_leading: false,
            }),
            MarkdownInline::Strong(children) => {
                let mut next = state;
                next.strong = true;
                append_inline_fragments(children, next, link_destination, fragments);
            }
            MarkdownInline::Emphasis(children) => {
                let mut next = state;
                next.emphasis = true;
                append_inline_fragments(children, next, link_destination, fragments);
            }
            MarkdownInline::Strikethrough(children) => {
                let mut next = state;
                next.strikethrough = true;
                append_inline_fragments(children, next, link_destination, fragments);
            }
            MarkdownInline::Link {
                destination,
                content,
                ..
            } => {
                let mut next = state;
                next.link = true;
                let first_fragment = fragments.len();
                append_inline_fragments(content, next, Some(destination), fragments);
                if let Some(fragment) = fragments.get_mut(first_fragment) {
                    fragment.link_leading = true;
                }
            }
        }
    }
}

fn markdown_file_reference_path(destination: &str) -> Option<&str> {
    if !Path::new(destination).is_absolute() {
        return None;
    }
    Some(
        destination
            .rsplit_once(':')
            .filter(|(_, line)| line.bytes().all(|byte| byte.is_ascii_digit()))
            .map_or(destination, |(path, _)| path),
    )
}

fn markdown_file_reference_icon(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("py" | "pyi" | "pyw") => "markdown-file-python",
        _ => "markdown-file-document",
    }
}

fn render_code_block(
    language: Option<&str>,
    code: &str,
    style: MarkdownRenderStyle,
    block_identity: u64,
) -> Div {
    let code = code.strip_suffix('\n').unwrap_or(code).to_owned();
    let code_for_copy = code.clone();
    let code_text = highlighted_code_text(&code, language, style);
    let scroll_id = markdown_element_id("markdown-code-scroll", &block_identity);
    let copy_id = markdown_element_id("markdown-code-copy", &block_identity);
    let wrap_id = markdown_element_id("markdown-code-wrap", &block_identity);
    let language_label = code_language_label(language);

    div()
        .w_full()
        .min_w(px(0.0))
        .overflow_hidden()
        .rounded(px(style.layout.code_radius))
        .border(px(1.0))
        .border_color(style.palette.code_border)
        .bg(style.palette.code_surface)
        .child(
            div()
                .w_full()
                .min_h(px(48.0))
                .relative()
                .pl(px(style.layout.code_header_padding_x))
                .pr(px(style.layout.code_header_padding_right))
                .py(px(style.layout.code_header_padding_y))
                .bg(style.palette.code_header_surface)
                .flex()
                .items_center()
                .justify_between()
                .font_weight(FontWeight::MEDIUM)
                .text_size(px(style.layout.code_header_size))
                .line_height(px(style.layout.code_header_line_height))
                // The live header paints a solid theme surface plus the same
                // translucent gradient used by the code body. Modeling both
                // layers avoids the misleading computed background-color and
                // reproduces the final #f4f4f4 / #242424 pixels.
                .child(div().absolute().inset_0().bg(style.palette.code_surface))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(icon("markdown-code", style.palette.text.into()).size(px(20.0)))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(language_label),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(1.0))
                        // ChatGPT always reserves this 36px action before copy.
                        // Wrapping is intentionally left disabled because this
                        // stateless display node mirrors its default CDP state.
                        .child(
                            div()
                                .id(wrap_id)
                                .size(px(36.0))
                                .rounded(px(10.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .hover(move |button| button.bg(style.palette.action_hover))
                                .child(
                                    icon("markdown-wrap", style.palette.text.into()).size(px(20.0)),
                                ),
                        )
                        .child(
                            div()
                                .id(copy_id)
                                .size(px(36.0))
                                .rounded(px(10.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(move |button| button.bg(style.palette.action_hover))
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        code_for_copy.clone(),
                                    ));
                                })
                                .child(
                                    icon("markdown-copy", style.palette.text.into()).size(px(20.0)),
                                ),
                        ),
                ),
        )
        .child(
            div()
                .id(scroll_id)
                .w_full()
                .min_w(px(0.0))
                .px(px(style.layout.code_body_padding_x))
                .pb(px(style.layout.code_body_padding_bottom))
                .flex()
                .overflow_x_scroll()
                .restrict_scroll_to_axis()
                .scrollbar_width(px(0.0))
                .text_size(px(style.layout.code_size))
                .line_height(px(style.layout.code_line_height))
                .font_family(UI_MONOSPACE_FONT_FAMILY)
                .whitespace_nowrap()
                .child(div().flex_none().child(code_text)),
        )
}

fn code_language(language: Option<&str>) -> Option<String> {
    language
        .and_then(|language| language.split_ascii_whitespace().next())
        .map(str::trim)
        .filter(|language| !language.is_empty())
        .map(|language| language.to_ascii_lowercase())
}

fn code_language_label(language: Option<&str>) -> String {
    let Some(raw_language) = language
        .and_then(|language| language.split_ascii_whitespace().next())
        .map(str::trim)
        .filter(|language| !language.is_empty())
    else {
        return "纯文本".to_owned();
    };
    match raw_language.to_ascii_lowercase().as_str() {
        "text" | "txt" | "plaintext" | "plain" => "纯文本".to_owned(),
        "bash" | "sh" | "zsh" => "Bash".to_owned(),
        "fish" => "Fish".to_owned(),
        "arduino" => "Arduino".to_owned(),
        "c" => "C".to_owned(),
        "cpp" | "c++" => "C++".to_owned(),
        "csharp" | "c#" | "cs" => "C#".to_owned(),
        "diff" => "Diff".to_owned(),
        "dart" => "Dart".to_owned(),
        "dockerfile" | "docker" => "Dockerfile".to_owned(),
        "elixir" => "Elixir".to_owned(),
        "erlang" => "Erlang".to_owned(),
        "go" | "golang" => "Go".to_owned(),
        "graphql" => "GraphQL".to_owned(),
        "haskell" => "Haskell".to_owned(),
        "ini" => "INI".to_owned(),
        "java" => "Java".to_owned(),
        "js" | "javascript" | "jsx" => "JavaScript".to_owned(),
        "ts" | "typescript" | "tsx" => "TypeScript".to_owned(),
        "kotlin" | "kt" => "Kotlin".to_owned(),
        "latex" | "tex" => "LaTeX".to_owned(),
        "less" => "Less".to_owned(),
        "lua" => "Lua".to_owned(),
        "makefile" | "make" => "Makefile".to_owned(),
        "objectivec" | "objective-c" | "objc" => "Objective-C".to_owned(),
        "perl" => "Perl".to_owned(),
        "php" => "PHP".to_owned(),
        "php-template" => "PHP template".to_owned(),
        "powershell" | "ps1" => "PowerShell".to_owned(),
        "py" | "python" => "Python".to_owned(),
        "python-repl" | "pycon" => "Python REPL".to_owned(),
        "r" => "R".to_owned(),
        "rb" | "ruby" => "Ruby".to_owned(),
        "rs" | "rust" => "Rust".to_owned(),
        "scala" => "Scala".to_owned(),
        "scss" => "SCSS".to_owned(),
        "shell" => "Shell".to_owned(),
        "swift" => "Swift".to_owned(),
        "json" => "JSON".to_owned(),
        "yaml" | "yml" => "YAML".to_owned(),
        "toml" => "TOML".to_owned(),
        "css" => "CSS".to_owned(),
        "sql" => "SQL".to_owned(),
        "html" | "xml" => "XML".to_owned(),
        "md" | "markdown" => "Markdown".to_owned(),
        "vbnet" | "vb" => "Visual Basic .NET".to_owned(),
        "wasm" | "webassembly" => "WebAssembly".to_owned(),
        _ => raw_language.to_owned(),
    }
}

const MAX_HIGHLIGHTED_CODE_BYTES: usize = 256 * 1024;
const MAX_HIGHLIGHTED_LINE_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CodeSyntaxToken {
    Plain,
    Comment,
    Keyword,
    Literal,
    String,
    Variable,
    Attribute,
    Name,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CodeSyntaxStyle {
    token: CodeSyntaxToken,
    italic: bool,
    bold: bool,
    underline: bool,
}

impl CodeSyntaxStyle {
    const PLAIN: Self = Self {
        token: CodeSyntaxToken::Plain,
        italic: false,
        bold: false,
        underline: false,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CodeHighlightSpan {
    range: Range<usize>,
    style: CodeSyntaxStyle,
}

struct CodeScopeClassifiers {
    tokens: Vec<(ScopeSelectors, CodeSyntaxToken)>,
    shell_plain: [Scope; 2],
    italic: ScopeSelectors,
    bold: ScopeSelectors,
    underline: ScopeSelectors,
}

fn code_syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(extra_newlines)
}

fn code_scope_classifiers() -> &'static CodeScopeClassifiers {
    static CLASSIFIERS: OnceLock<CodeScopeClassifiers> = OnceLock::new();
    CLASSIFIERS.get_or_init(|| CodeScopeClassifiers {
        // Sublime/TextMate scopes do not have highlight.js's exact class names.
        // These groups preserve ChatGPT's eight semantic theme buckets while
        // allowing the grammar's more-specific nested scope to win.
        tokens: [
            ("comment", CodeSyntaxToken::Comment),
            (
                "keyword, storage, punctuation.definition.keyword",
                CodeSyntaxToken::Keyword,
            ),
            (
                "constant.numeric, constant.language, support.function, support.class, support.type, entity.name.type.class",
                CodeSyntaxToken::Literal,
            ),
            ("string, regexp, markup.inserted", CodeSyntaxToken::String),
            (
                "variable, support.variable, entity.name.function, entity.name.class, entity.other.inherited-class",
                CodeSyntaxToken::Variable,
            ),
            (
                "entity.other.attribute-name, support.type.property-name, markup.heading",
                CodeSyntaxToken::Attribute,
            ),
            (
                "entity.name.tag, constant.other.symbol, markup.list, meta.preprocessor",
                CodeSyntaxToken::Name,
            ),
            ("invalid, markup.deleted", CodeSyntaxToken::Error),
        ]
        .into_iter()
        .map(|(selector, token)| {
            (
                ScopeSelectors::from_str(selector).expect("valid Markdown syntax selector"),
                token,
            )
        })
        .collect(),
        // TextMate treats every shell command name and option as a variable.
        // highlight.js (and the live ChatGPT Bash block) leaves ordinary
        // commands such as `ssh -t` in the base foreground instead.
        shell_plain: [
            Scope::new("variable.function.shell").expect("valid shell command scope"),
            Scope::new("variable.parameter.option.shell").expect("valid shell option scope"),
        ],
        italic: ScopeSelectors::from_str("comment, markup.italic")
            .expect("valid Markdown italic selector"),
        bold: ScopeSelectors::from_str("markup.bold").expect("valid Markdown bold selector"),
        underline: ScopeSelectors::from_str("markup.underline.link")
            .expect("valid Markdown underline selector"),
    })
}

fn code_syntax(language: Option<&str>) -> Option<&'static SyntaxReference> {
    let language = code_language(language)?;
    let token = match language.as_str() {
        "text" | "txt" | "plaintext" | "plain" => return None,
        "arduino" => "cpp",
        "bash" | "sh" | "zsh" => "sh",
        "c#" | "csharp" => "cs",
        "c++" => "cpp",
        "docker" => "Dockerfile",
        "gql" => "graphql",
        "golang" => "go",
        "html" => "xml",
        "javascript" => "js",
        "kt" => "kotlin",
        "make" => "Makefile",
        "markdown" => "md",
        "objectivec" | "objective-c" | "objc" => "Objective-C",
        "patch" => "diff",
        "php-template" => "php",
        "python-repl" | "pycon" => "python",
        "shell" => "Shell-Unix-Generic",
        "typescript" => "ts",
        "vb" | "vbnet" | "wasm" | "webassembly" => return None,
        "yml" => "yaml",
        _ => language.as_str(),
    };
    code_syntax_set().find_syntax_by_token(token)
}

fn code_scope_style(stack: &ScopeStack) -> CodeSyntaxStyle {
    let classifiers = code_scope_classifiers();
    let mut strongest: Option<(MatchPower, usize, CodeSyntaxToken)> = None;
    for (index, (selector, token)) in classifiers.tokens.iter().enumerate() {
        let Some(power) = selector.does_match(stack.as_slice()) else {
            continue;
        };
        if strongest
            .as_ref()
            .is_none_or(|(best_power, best_index, _)| {
                power > *best_power || (power == *best_power && index > *best_index)
            })
        {
            strongest = Some((power, index, *token));
        }
    }

    let token = if stack.as_slice().iter().any(|scope| {
        classifiers
            .shell_plain
            .iter()
            .any(|plain| plain.is_prefix_of(*scope))
    }) {
        CodeSyntaxToken::Plain
    } else {
        strongest
            .map(|(_, _, token)| token)
            .unwrap_or(CodeSyntaxToken::Plain)
    };
    CodeSyntaxStyle {
        token,
        italic: token == CodeSyntaxToken::Comment
            || classifiers.italic.does_match(stack.as_slice()).is_some(),
        bold: classifiers.bold.does_match(stack.as_slice()).is_some(),
        underline: classifiers.underline.does_match(stack.as_slice()).is_some(),
    }
}

fn push_code_span(spans: &mut Vec<CodeHighlightSpan>, range: Range<usize>, style: CodeSyntaxStyle) {
    if range.is_empty() {
        return;
    }
    if let Some(previous) = spans.last_mut()
        && previous.range.end == range.start
        && previous.style == style
    {
        previous.range.end = range.end;
    } else {
        spans.push(CodeHighlightSpan { range, style });
    }
}

fn highlighted_code_spans(code: &str, language: Option<&str>) -> Option<Vec<CodeHighlightSpan>> {
    if code.len() > MAX_HIGHLIGHTED_CODE_BYTES {
        return None;
    }
    let syntax = code_syntax(language)?;
    let syntax_set = code_syntax_set();
    let mut parse_state = ParseState::new(syntax);
    let mut scope_stack = ScopeStack::new();
    let mut spans = Vec::new();
    let mut line_base = 0;

    for line in LinesWithEndings::from(code) {
        if line.len() > MAX_HIGHLIGHTED_LINE_BYTES {
            return None;
        }
        let operations = parse_state.parse_line(line, syntax_set).ok()?;
        let mut line_cursor = 0;
        for (segment, operation) in ScopeRegionIterator::new(&operations, line) {
            scope_stack.apply(operation).ok()?;
            let start = line_base + line_cursor;
            line_cursor += segment.len();
            push_code_span(
                &mut spans,
                start..line_base + line_cursor,
                code_scope_style(&scope_stack),
            );
        }
        if line_cursor != line.len() {
            return None;
        }
        line_base += line.len();
    }

    if line_base != code.len()
        || spans.iter().any(|span| {
            !code.is_char_boundary(span.range.start) || !code.is_char_boundary(span.range.end)
        })
        || spans
            .windows(2)
            .any(|spans| spans[0].range.end != spans[1].range.start)
        || spans.first().is_some_and(|span| span.range.start != 0)
        || spans
            .last()
            .is_some_and(|span| span.range.end != code.len())
    {
        return None;
    }
    Some(spans)
}

fn highlighted_code_text(
    code: &str,
    language: Option<&str>,
    style: MarkdownRenderStyle,
) -> StyledText {
    let mut base_font = ui_font();
    base_font.family = UI_MONOSPACE_FONT_FAMILY.into();
    base_font.weight = FontWeight::NORMAL;
    let spans = highlighted_code_spans(code, language).unwrap_or_else(|| {
        (!code.is_empty())
            .then(|| CodeHighlightSpan {
                range: 0..code.len(),
                style: CodeSyntaxStyle::PLAIN,
            })
            .into_iter()
            .collect()
    });
    let runs = spans
        .into_iter()
        .map(|span| {
            code_text_run(
                span.range.len(),
                base_font.clone(),
                span.style,
                style.palette,
            )
        })
        .collect();
    StyledText::new(code.to_owned()).with_runs(runs)
}

fn code_syntax_color(token: CodeSyntaxToken, palette: MarkdownPalette) -> Rgba {
    match token {
        CodeSyntaxToken::Plain => palette.text,
        CodeSyntaxToken::Comment => palette.syntax_comment,
        CodeSyntaxToken::Keyword => palette.syntax_keyword,
        CodeSyntaxToken::Literal => palette.syntax_literal,
        CodeSyntaxToken::String => palette.syntax_string,
        CodeSyntaxToken::Variable => palette.syntax_variable,
        CodeSyntaxToken::Attribute => palette.syntax_attribute,
        CodeSyntaxToken::Name => palette.syntax_name,
        CodeSyntaxToken::Error => palette.syntax_error,
    }
}

fn code_text_run(
    len: usize,
    mut font: gpui::Font,
    style: CodeSyntaxStyle,
    palette: MarkdownPalette,
) -> TextRun {
    let color = code_syntax_color(style.token, palette);
    if style.italic {
        font.style = FontStyle::Italic;
    }
    if style.bold {
        font.weight = FontWeight::BOLD;
    }
    TextRun {
        len,
        font,
        color: color.into(),
        background_color: None,
        underline: style.underline.then(|| UnderlineStyle {
            thickness: px(1.0),
            color: Some(color.into()),
            wavy: false,
        }),
        strikethrough: None,
    }
}

fn render_table(
    alignments: &[MarkdownAlignment],
    header: &[MarkdownTableCell],
    rows: &[Vec<MarkdownTableCell>],
    style: MarkdownRenderStyle,
    block_identity: u64,
) -> Div {
    let column_count = alignments
        .len()
        .max(header.len())
        .max(rows.iter().map(Vec::len).max().unwrap_or(0))
        .max(1)
        .min(u16::MAX as usize);
    let scroll_id = markdown_element_id("markdown-table-scroll", &block_identity);
    let mut table = div()
        .min_w(px(style.layout.table_min_width))
        .flex_none()
        .grid()
        .grid_cols_max_content(column_count as u16)
        .text_size(px(style.layout.table_size))
        .line_height(px(style.layout.table_line_height));

    if !header.is_empty() {
        table = append_table_row(
            table,
            header,
            alignments,
            style,
            true,
            false,
            markdown_hash(&(block_identity, "header")),
            column_count,
        );
    }
    for (index, row) in rows.iter().enumerate() {
        table = append_table_row(
            table,
            row,
            alignments,
            style,
            false,
            index + 1 == rows.len(),
            markdown_hash(&(block_identity, index)),
            column_count,
        );
    }

    // ChatGPT lets tables break out beyond the prose column into a centered
    // 1024px scroller. The table itself is intrinsic-width with a prose-width
    // floor, so short columns stay compact while wide content scrolls.
    div().w_full().min_w(px(0.0)).flex().justify_center().child(
        div()
            .id(scroll_id)
            .w(px(style.layout.table_breakout_width))
            .flex_none()
            .min_w(px(0.0))
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .scrollbar_width(px(0.0))
            .child(div().w_full().flex().child(table.mx_auto())),
    )
}

fn append_table_row(
    mut table: Div,
    cells: &[MarkdownTableCell],
    alignments: &[MarkdownAlignment],
    style: MarkdownRenderStyle,
    is_header: bool,
    is_last_row: bool,
    row_identity: u64,
    column_count: usize,
) -> Div {
    for index in 0..column_count {
        let cell = cells.get(index);
        let alignment = alignments
            .get(index)
            .copied()
            .unwrap_or(MarkdownAlignment::None);
        let mut element = div()
            .min_w(px(0.0))
            .max_w(px(style.layout.table_cell_max_width))
            .h_full()
            .pr(px(if index + 1 == column_count {
                if is_header {
                    style.layout.table_header_last_padding_right
                } else {
                    0.0
                }
            } else {
                style.layout.table_cell_padding_right
            }))
            .bg(if is_header {
                style.palette.table_header_surface
            } else {
                rgba_transparent()
            })
            .when(is_header, |element| {
                element
                    .py(px(style.layout.table_header_padding_y))
                    .border_b_1()
                    .border_color(style.palette.table_border_strong)
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(px(style.layout.table_header_line_height))
            })
            .when(!is_header, |element| {
                element
                    .pt(px(style.layout.table_cell_padding_y))
                    .pb(px(if is_last_row {
                        style.layout.table_body_last_padding_bottom
                    } else {
                        style.layout.table_cell_padding_y
                    }))
                    .when(!is_last_row, |element| {
                        element
                            .border_b_1()
                            .border_color(style.palette.table_border_subtle)
                    })
            });
        element = match alignment {
            MarkdownAlignment::Center => element.text_align(TextAlign::Center),
            MarkdownAlignment::Right => element.text_align(TextAlign::Right),
            MarkdownAlignment::None | MarkdownAlignment::Left => {
                element.text_align(TextAlign::Left)
            }
        };
        if let Some(cell) = cell {
            element = element.child(render_inline_block(
                &cell.content,
                style,
                style.layout.table_size,
                if is_header {
                    style.layout.table_header_line_height
                } else {
                    style.layout.table_line_height
                },
                if is_header {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                },
                markdown_hash(&(row_identity, index)),
            ));
        }
        table = table.child(element);
    }
    table
}

fn rgba_transparent() -> Rgba {
    Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    }
}

fn markdown_element_id(prefix: &str, value: &impl Hash) -> SharedString {
    format!("{prefix}-{:016x}", markdown_hash(value)).into()
}

fn markdown_hash(value: &(impl Hash + ?Sized)) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeMode;
    use gpui::rgba;

    #[test]
    fn parses_commonmark_and_gfm_nodes() {
        let markdown = r#"# H1
## H2
### H3
#### H4
##### H5
###### H6

Text **strong** *emphasis* ~~strike~~ `inline` [link](https://example.com "title")
soft
line\
hard

> quote **body**

---

1. ordered
   - nested
2. next

| Left | Right |
| :--- | ---: |
| a | b |

```rust
fn main() {}
```
"#;
        let document = parse_markdown(markdown);

        let levels = document
            .blocks
            .iter()
            .filter_map(|block| match block {
                MarkdownBlock::Heading { level, .. } => Some(*level),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(levels, vec![1, 2, 3, 4, 5, 6]);
        assert!(
            document
                .blocks
                .iter()
                .any(|block| matches!(block, MarkdownBlock::BlockQuote(_)))
        );
        assert!(
            document
                .blocks
                .iter()
                .any(|block| matches!(block, MarkdownBlock::HorizontalRule))
        );
        assert!(document.blocks.iter().any(|block| matches!(
            block,
            MarkdownBlock::Table { header, rows, .. }
                if header.len() == 2 && rows.len() == 1
        )));
        assert!(document.blocks.iter().any(|block| matches!(
            block,
            MarkdownBlock::CodeBlock { language, fenced: true, code }
                if language.as_deref() == Some("rust") && code.contains("fn main")
        )));

        let rich_paragraph = document.blocks.iter().find_map(|block| match block {
            MarkdownBlock::Paragraph(content)
                if content
                    .iter()
                    .any(|inline| matches!(inline, MarkdownInline::Strong(_))) =>
            {
                Some(content)
            }
            _ => None,
        });
        let rich_paragraph = rich_paragraph.expect("rich paragraph");
        assert!(
            rich_paragraph
                .iter()
                .any(|inline| matches!(inline, MarkdownInline::Emphasis(_)))
        );
        assert!(
            rich_paragraph
                .iter()
                .any(|inline| matches!(inline, MarkdownInline::Strikethrough(_)))
        );
        assert!(
            rich_paragraph
                .iter()
                .any(|inline| matches!(inline, MarkdownInline::Code(code) if code == "inline"))
        );
        assert!(rich_paragraph.iter().any(|inline| matches!(
            inline,
            MarkdownInline::Link { destination, .. } if destination == "https://example.com"
        )));
        assert!(
            rich_paragraph
                .iter()
                .any(|inline| matches!(inline, MarkdownInline::SoftBreak))
        );
        assert!(
            rich_paragraph
                .iter()
                .any(|inline| matches!(inline, MarkdownInline::HardBreak))
        );
    }

    #[test]
    fn preserves_ordered_start_nested_lists_and_tasks() {
        let document = parse_markdown("3. outer\n   - [x] nested task\n4. next\n");
        let MarkdownBlock::List { start, items } = &document.blocks[0] else {
            panic!("expected list");
        };
        assert_eq!(*start, Some(3));
        assert_eq!(items.len(), 2);
        let nested = items[0]
            .blocks
            .iter()
            .find_map(|block| match block {
                MarkdownBlock::List { items, .. } => Some(items),
                _ => None,
            })
            .expect("nested list");
        assert_eq!(nested[0].checked, Some(true));
    }

    #[test]
    fn incomplete_streaming_markdown_stays_renderable() {
        let document = parse_markdown("before **open\n\n```rust\nfn main(");
        assert!(!document.blocks.is_empty());
        assert!(document.blocks.iter().any(|block| matches!(
            block,
            MarkdownBlock::CodeBlock { fenced: true, code, .. } if code.contains("fn main(")
        )));
    }

    #[test]
    fn light_and_dark_share_structure_and_geometry() {
        let source = "## Title\n\n- **item** with `code`\n\n| a | b |\n|---|---|\n| 1 | 2 |";
        let light_document = parse_markdown(source);
        let dark_document = parse_markdown(source);
        assert_eq!(light_document, dark_document);

        let light = MarkdownRenderStyle::new(Theme::for_mode(ThemeMode::Light));
        let dark = MarkdownRenderStyle::new(Theme::for_mode(ThemeMode::Dark));
        assert_eq!(light.layout, dark.layout);
        assert_ne!(light.palette, dark.palette);
        assert_eq!(light.palette.link, dark.palette.link);
    }

    #[test]
    fn code_toolbar_labels_match_chatgpt_language_aliases() {
        assert_eq!(code_language_label(None), "纯文本");
        assert_eq!(code_language_label(Some("text")), "纯文本");
        assert_eq!(code_language_label(Some("bash")), "Bash");
        assert_eq!(code_language_label(Some("sh")), "Bash");
        assert_eq!(code_language_label(Some("zsh")), "Bash");
        assert_eq!(code_language_label(Some("jsx")), "JavaScript");
        assert_eq!(code_language_label(Some("tsx")), "TypeScript");
        assert_eq!(code_language_label(Some("html")), "XML");
        assert_eq!(code_language_label(Some("rust title=sample")), "Rust");
    }

    #[test]
    fn absolute_markdown_links_use_chatgpt_file_mentions() {
        assert_eq!(
            markdown_file_reference_path("/Users/example/source.py:18"),
            Some("/Users/example/source.py")
        );
        assert_eq!(
            markdown_file_reference_path("/Users/example/README.md"),
            Some("/Users/example/README.md")
        );
        assert_eq!(
            markdown_file_reference_path("https://example.com/source.py"),
            None
        );
        assert_eq!(
            markdown_file_reference_icon("/Users/example/source.py"),
            "markdown-file-python"
        );
        assert_eq!(
            markdown_file_reference_icon("/Users/example/README.md"),
            "markdown-file-document"
        );

        let light = MarkdownRenderStyle::new(Theme::for_mode(ThemeMode::Light)).palette;
        let dark = MarkdownRenderStyle::new(Theme::for_mode(ThemeMode::Dark)).palette;
        assert_eq!(light.file_link, rgba(0x2858a4ff));
        assert_eq!(dark.file_link, rgba(0x5685d1ff));
    }

    fn assert_code_span_coverage(code: &str, spans: &[CodeHighlightSpan]) {
        if code.is_empty() {
            assert!(spans.is_empty());
            return;
        }
        assert_eq!(spans.first().map(|span| span.range.start), Some(0));
        assert_eq!(spans.last().map(|span| span.range.end), Some(code.len()));
        assert!(
            spans
                .windows(2)
                .all(|spans| spans[0].range.end == spans[1].range.start)
        );
        assert!(spans.iter().all(|span| {
            span.range.start < span.range.end
                && code.is_char_boundary(span.range.start)
                && code.is_char_boundary(span.range.end)
        }));
        assert_eq!(
            spans.iter().map(|span| span.range.len()).sum::<usize>(),
            code.len()
        );
    }

    fn text_for_token<'a>(
        code: &'a str,
        spans: &'a [CodeHighlightSpan],
        token: CodeSyntaxToken,
    ) -> Vec<&'a str> {
        spans
            .iter()
            .filter(|span| span.style.token == token)
            .map(|span| &code[span.range.clone()])
            .collect()
    }

    #[test]
    fn syntax_highlighting_covers_utf8_bytes_and_multiline_scopes() {
        let rust = "fn greet() {\n    let value = \"中文🙂\"; // comment\n}\n";
        let rust_spans = highlighted_code_spans(rust, Some("rs")).expect("Rust syntax");
        assert_code_span_coverage(rust, &rust_spans);
        assert!(
            text_for_token(rust, &rust_spans, CodeSyntaxToken::Keyword)
                .iter()
                .any(|text| text.contains("fn"))
        );
        assert!(
            text_for_token(rust, &rust_spans, CodeSyntaxToken::String)
                .iter()
                .any(|text| text.contains("中文🙂"))
        );
        assert!(
            text_for_token(rust, &rust_spans, CodeSyntaxToken::Comment)
                .iter()
                .any(|text| text.contains("comment"))
        );
        assert!(
            rust_spans
                .iter()
                .filter(|span| span.style.token == CodeSyntaxToken::Comment)
                .all(|span| span.style.italic)
        );

        let python = "message = \"\"\"first\n第二行🙂\nthird\"\"\"\nprint(message)";
        let python_spans = highlighted_code_spans(python, Some("python")).expect("Python syntax");
        assert_code_span_coverage(python, &python_spans);
        let highlighted_strings = text_for_token(python, &python_spans, CodeSyntaxToken::String)
            .into_iter()
            .collect::<String>();
        assert!(highlighted_strings.contains("第二行🙂"));

        let shell = "ssh -t host \"value\" # a \"quote\" in a comment\n";
        let shell_spans = highlighted_code_spans(shell, Some("bash")).expect("Bash syntax");
        assert_code_span_coverage(shell, &shell_spans);
        assert!(
            shell_spans
                .iter()
                .any(|span| span.style.token == CodeSyntaxToken::Comment
                    && shell[span.range.clone()].contains("quote"))
        );
        assert!(shell_spans.iter().any(|span| {
            span.style.token == CodeSyntaxToken::Plain && shell[span.range.clone()].contains("ssh")
        }));
        assert!(!shell_spans.iter().any(|span| {
            span.style.token == CodeSyntaxToken::Variable
                && (shell[span.range.clone()].contains("ssh")
                    || shell[span.range.clone()].contains(" -"))
        }));
    }

    #[test]
    fn syntax_highlighting_falls_back_for_unknown_plaintext_and_limits() {
        assert!(highlighted_code_spans("text", None).is_none());
        assert!(highlighted_code_spans("text", Some("plaintext")).is_none());
        assert!(highlighted_code_spans("text", Some("not-a-real-language")).is_none());
        assert!(highlighted_code_spans("", Some("rust")).unwrap().is_empty());

        let oversized_code = "x".repeat(MAX_HIGHLIGHTED_CODE_BYTES + 1);
        assert!(highlighted_code_spans(&oversized_code, Some("rust")).is_none());
        let oversized_line = "x".repeat(MAX_HIGHLIGHTED_LINE_BYTES + 1);
        assert!(highlighted_code_spans(&oversized_line, Some("rust")).is_none());
    }

    #[test]
    fn syntax_lookup_covers_chatgpt_languages_with_explicit_fallbacks() {
        for language in [
            "arduino",
            "bash",
            "c",
            "cpp",
            "csharp",
            "css",
            "diff",
            "go",
            "graphql",
            "ini",
            "java",
            "javascript",
            "json",
            "kotlin",
            "less",
            "lua",
            "makefile",
            "markdown",
            "objectivec",
            "perl",
            "php",
            "php-template",
            "python",
            "python-repl",
            "r",
            "ruby",
            "rust",
            "scss",
            "shell",
            "sql",
            "swift",
            "typescript",
            "xml",
            "yaml",
        ] {
            assert!(code_syntax(Some(language)).is_some(), "missing {language}");
        }
        for language in ["plaintext", "vbnet", "wasm"] {
            assert!(
                code_syntax(Some(language)).is_none(),
                "{language} should use the safe plaintext fallback"
            );
        }
    }

    #[test]
    fn chatgpt_code_theme_exposes_all_semantic_tokens() {
        let light = MarkdownRenderStyle::new(Theme::for_mode(ThemeMode::Light)).palette;
        assert_eq!(light.syntax_comment, rgba(0x4f4f4fff));
        assert_eq!(light.syntax_keyword, rgba(0xab4f7aff));
        assert_eq!(light.syntax_literal, rgba(0xac4f23ff));
        assert_eq!(light.syntax_string, rgba(0x3a843fff));
        assert_eq!(light.syntax_variable, rgba(0x643caeff));
        assert_eq!(light.syntax_attribute, rgba(0xb8802bff));
        assert_eq!(light.syntax_name, rgba(0x1f4e94ff));
        assert_eq!(light.syntax_error, rgba(0xba2623ff));

        let dark = MarkdownRenderStyle::new(Theme::for_mode(ThemeMode::Dark)).palette;
        assert_eq!(dark.syntax_comment, rgba(0xb9b9b9ff));
        assert_eq!(dark.syntax_keyword, rgba(0xf8a6c8ff));
        assert_eq!(dark.syntax_literal, rgba(0xf1a275ff));
        assert_eq!(dark.syntax_string, rgba(0x83d197ff));
        assert_eq!(dark.syntax_variable, rgba(0xb897f4ff));
        assert_eq!(dark.syntax_attribute, rgba(0xf9dc78ff));
        assert_eq!(dark.syntax_name, rgba(0x63a8f8ff));
        assert_eq!(dark.syntax_error, rgba(0xff8583ff));
    }
}
