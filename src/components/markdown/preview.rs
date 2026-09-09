//! Persistent, virtualized Markdown for workspace files. Chat messages continue
//! to use the stateless renderer; both paths share block and table presentation.

use std::{cell::RefCell, rc::Rc};

use gpui::{
    App, Context, FocusHandle, Focusable, KeyDownEvent, ListAlignment, ListOffset, ListState,
    MouseButton, Render, ScrollHandle, Window, list,
};

use super::*;
use crate::theme::ThemeMode;

const ESTIMATED_ITEM_HEIGHT: f32 = 64.;

struct PreviewTable {
    widths: RefCell<Option<Vec<f32>>>,
    scroll: ScrollHandle,
}

enum PreviewItem {
    Block(usize),
    TableRow {
        block: usize,
        row: Option<usize>,
        table: Rc<PreviewTable>,
    },
}

impl PreviewItem {
    fn block(&self) -> usize {
        match self {
            Self::Block(block) | Self::TableRow { block, .. } => *block,
        }
    }
}

fn preview_items(document: &MarkdownDocument) -> Vec<PreviewItem> {
    let mut items = Vec::new();
    for (block, value) in document.blocks.iter().enumerate() {
        if let MarkdownBlock::Table { header, rows, .. } = value {
            // A protocol reference can be a single table with hundreds of rows.
            // Virtualizing only top-level blocks would still lay out every cell.
            let table = Rc::new(PreviewTable {
                widths: RefCell::new(None),
                scroll: ScrollHandle::new(),
            });
            if !header.is_empty() {
                items.push(PreviewItem::TableRow {
                    block,
                    row: None,
                    table: table.clone(),
                });
            }
            for row in 0..rows.len() {
                items.push(PreviewItem::TableRow {
                    block,
                    row: Some(row),
                    table: table.clone(),
                });
            }
        } else {
            items.push(PreviewItem::Block(block));
        }
    }
    items
}

pub struct MarkdownPreview {
    document: Rc<MarkdownDocument>,
    items: Rc<Vec<PreviewItem>>,
    scroll: ListState,
    mode: ThemeMode,
    focus: FocusHandle,
    identity: u64,
    selectable: bool,
}

impl MarkdownPreview {
    pub fn new(document: MarkdownDocument, mode: ThemeMode, cx: &mut Context<Self>) -> Self {
        let items = preview_items(&document);
        Self {
            scroll: ListState::new(items.len(), ListAlignment::Top, px(200.))
                .with_uniform_item_height(px(ESTIMATED_ITEM_HEIGHT)),
            items: Rc::new(items),
            document: Rc::new(document),
            mode,
            focus: cx.focus_handle(),
            identity: markdown_hash(&cx.entity_id()),
            selectable: false,
        }
    }

    pub fn enable_text_selection(&mut self) {
        self.selectable = true;
    }
    pub fn set_document(&mut self, document: MarkdownDocument, cx: &mut Context<Self>) {
        let anchor = self.scroll.logical_scroll_top();
        let items = preview_items(&document);
        self.scroll
            .reset_with_uniform_height(items.len(), px(ESTIMATED_ITEM_HEIGHT));
        self.scroll.scroll_to(ListOffset {
            item_ix: anchor.item_ix.min(items.len().saturating_sub(1)),
            offset_in_item: anchor.offset_in_item,
        });
        self.document = Rc::new(document);
        self.items = Rc::new(items);
        cx.notify();
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.mode = mode;
            self.scroll.remeasure();
            cx.notify();
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let page = (self.scroll.viewport_bounds().size.height - px(40.)).max(px(40.));
        match event.keystroke.key.as_str() {
            "home" => self.scroll.scroll_to(ListOffset::default()),
            "end" => self.scroll.scroll_to_end(),
            "up" if event.keystroke.modifiers.platform => {
                self.scroll.scroll_to(ListOffset::default())
            }
            "down" if event.keystroke.modifiers.platform => self.scroll.scroll_to_end(),
            "up" => self.scroll.scroll_by(px(-40.)),
            "down" => self.scroll.scroll_by(px(40.)),
            "pageup" => self.scroll.scroll_by(-page),
            "pagedown" => self.scroll.scroll_by(page),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Focusable for MarkdownPreview {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for MarkdownPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let document = self.document.clone();
        let items = self.items.clone();
        let mut style = MarkdownRenderStyle::new(Theme::for_mode(self.mode));
        style.selectable = self.selectable;
        let identity = self.identity;
        div()
            .id("file-markdown-preview")
            .debug_selector(|| "file-markdown-preview".into())
            .size_full()
            .min_w(px(0.))
            .min_h(px(0.))
            .overflow_hidden()
            .track_focus(&self.focus)
            .role(gpui::Role::List)
            .aria_label("Markdown 文件预览")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, _, window, cx| {
                    if !window.default_prevented() {
                        s.focus.focus(window, cx);
                    }
                }),
            )
            .on_key_down(cx.listener(Self::key_down))
            .text_size(px(style.layout.base_size))
            .line_height(px(style.layout.base_line_height))
            .font(ui_font())
            .font_weight(CHATGPT_MARKDOWN_BODY_WEIGHT)
            .text_color(style.palette.text)
            .child(
                list(self.scroll.clone(), move |index, window, cx| {
                    render_item(&items[index], &document, style, identity, window, cx)
                        .debug_selector(move || format!("file-preview-item-{index}"))
                        .into_any_element()
                })
                .size_full()
                .py(px(24.)),
            )
    }
}

fn render_item(
    item: &PreviewItem,
    document: &MarkdownDocument,
    style: MarkdownRenderStyle,
    identity: u64,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let index = item.block();
    let block = &document.blocks[index];
    let block_identity = markdown_hash(&(identity, index));
    let previous = index.checked_sub(1).map(|i| &document.blocks[i]);
    let (top, _) = block_margins(
        block,
        previous,
        index == 0,
        style.layout,
        SequenceContext::Root,
    );
    let bottom = previous.map_or(0., |block| {
        block_margins(
            block,
            index.checked_sub(2).map(|i| &document.blocks[i]),
            index == 1,
            style.layout,
            SequenceContext::Root,
        )
        .1
    });
    let first_row = match item {
        PreviewItem::TableRow { row: Some(row), .. } => {
            matches!(block, MarkdownBlock::Table { header, .. } if header.is_empty() && *row == 0)
        }
        _ => true,
    };
    let gap = if index > 0 && first_row {
        top.max(bottom)
    } else {
        0.
    };
    // Padding is part of a virtual item's measured height; root margins are not.
    // List applies vertical padding to its scroll geometry. Horizontal padding
    // belongs on the items so it also constrains text and nested scrollports.
    let outer = div().w_full().min_w(px(0.)).px(px(24.)).pt(px(gap));
    if let PreviewItem::TableRow { row, table, .. } = item
        && let MarkdownBlock::Table {
            alignments,
            header,
            rows,
        } = block
    {
        let mut cached_widths = table.widths.borrow_mut();
        let widths = cached_widths.get_or_insert_with(|| {
            table_column_widths(alignments, header, rows, style, block_identity, window, cx)
        });
        let width = widths.iter().sum::<f32>().max(style.layout.table_min_width);
        let row_identity = row.map_or_else(
            || markdown_hash(&(block_identity, "header")),
            |row| markdown_hash(&(block_identity, row)),
        );
        let grid = append_table_row(
            div()
                .w(px(width))
                .flex_none()
                .grid()
                .grid_cols_auto(widths.len() as u16)
                .text_size(px(style.layout.table_size))
                .line_height(px(style.layout.table_line_height)),
            row.map_or(header.as_slice(), |row| rows[row].as_slice()),
            style,
            TableRowPresentation {
                is_header: row.is_none(),
                is_last_row: row.is_some_and(|row| row + 1 == rows.len()),
                row_identity,
            },
            widths.len(),
            widths,
        );
        outer.child(
            div().w_full().min_w(px(0.)).flex().justify_center().child(
                div()
                    .id(markdown_element_id("preview-table-row", &row_identity))
                    .w(px(width))
                    .max_w_full()
                    .min_w(px(0.))
                    .flex_none()
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .scrollbar_width(px(0.))
                    // Every mounted row uses the table's single horizontal offset.
                    .track_scroll(&table.scroll)
                    .child(grid),
            ),
        )
    } else {
        outer.child(
            div()
                .w_full()
                .min_w(px(0.))
                .max_w(px(style.layout.table_min_width))
                .mx_auto()
                .child(render_block(
                    block,
                    style,
                    0,
                    SequenceContext::Root,
                    block_identity,
                )),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Bounds, MouseButton, point, size};

    fn window(
        source: &str,
        width: f32,
        height: f32,
    ) -> (gpui::TestApp, gpui::TestAppWindow<MarkdownPreview>) {
        let mut app = gpui::TestApp::new();
        let document = parse_markdown(source);
        let window = app.open_window_with_options(
            gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(width), px(height)),
                ))),
                ..Default::default()
            },
            |_, cx| MarkdownPreview::new(document, ThemeMode::Light, cx),
        );
        (app, window)
    }

    #[test]
    fn long_preview_measures_visible_items_and_reaches_both_ends() {
        let source = "A paragraph with **bold** and `code`.\n\n".repeat(500);
        let (_app, mut window) = window(&source, 360., 300.);
        window.draw();
        let document = window.read(|p, _| p.document.clone());
        window.read(|p, _| {
            assert!(p.scroll.bounds_for_item(0).is_some());
            assert!(
                p.scroll.bounds_for_item(499).is_none(),
                "offscreen paragraphs must not be laid out"
            );
        });
        window.simulate_scroll(point(px(180.), px(140.)), point(px(0.), px(-160.)));
        window.draw();
        window.read(|p, _| {
            assert!(Rc::ptr_eq(&document, &p.document));
            assert!(p.scroll.logical_scroll_top().item_ix > 0);
        });
        window.simulate_click(point(px(180.), px(140.)), MouseButton::Left);
        window.simulate_keystroke("cmd-down");
        window.draw();
        window.read(|p, _| assert!(p.scroll.bounds_for_item(499).is_some()));
        window.simulate_resize(size(px(200.), px(180.)));
        window.draw();
        window.simulate_keystroke("cmd-up");
        window.draw();
        window.read(|p, _| assert_eq!(p.scroll.logical_scroll_top().item_ix, 0));
        window.simulate_scroll(point(px(100.), px(100.)), point(px(0.), px(-3000.)));
        window.draw();
        window.read(|p, _| {
            assert!(
                p.scroll.logical_scroll_top().item_ix > 20,
                "large wheel deltas must pass beyond the measured prefix after resizing"
            )
        });
    }

    #[test]
    fn large_table_shares_horizontal_scroll_and_keeps_rows_virtual() {
        let source = "| Left | Long details | Right |\n| --- | --- | --- |\n".to_owned()
            + &"| A | 一个需要横向滚动才能看完的表格单元格 | RIGHT |\n".repeat(500);
        let (_app, mut window) = window(&source, 360., 300.);
        window.draw();
        let table = window.read(|p, _| {
            assert!(p.scroll.bounds_for_item(450).is_none());
            let PreviewItem::TableRow { table, .. } = &p.items[0] else {
                panic!()
            };
            table.clone()
        });
        assert!(table.widths.borrow().is_some());
        assert_eq!(table.scroll.bounds().left(), px(24.));
        assert_eq!(table.scroll.bounds().size.width, px(312.));
        window.simulate_scroll(point(px(160.), px(42.)), point(px(-10000.), px(0.)));
        window.draw();
        assert!(table.scroll.offset().x < px(-300.));
        // A vertical wheel over a horizontally scrollable row must reach the list.
        window.simulate_scroll(point(px(160.), px(110.)), point(px(0.), px(-150.)));
        window.draw();
        window.read(|p, _| assert!(p.scroll.logical_scroll_top().item_ix > 0));
        let offset = table.scroll.offset().x;
        window.simulate_resize(size(px(260.), px(300.)));
        window.draw();
        assert_eq!(table.scroll.offset().x, offset);
        window.simulate_scroll(point(px(140.), px(100.)), point(px(10000.), px(0.)));
        window.draw();
        assert_eq!(table.scroll.offset().x, px(0.));
    }

    #[test]
    fn long_inline_code_and_file_links_wrap_without_overlapping_following_blocks() {
        let source = format!(
            "`{}`\n\n[{}](/tmp/file.rs)\n\nEND",
            "long_identifier_".repeat(20),
            "长链接".repeat(40)
        );
        let (_app, mut window) = window(&source, 200., 900.);
        window.draw();
        window.read(|p, _| {
            let code = p.scroll.bounds_for_item(0).unwrap();
            let link = p.scroll.bounds_for_item(1).unwrap();
            assert!(
                code.size.height > px(100.),
                "long inline code needs multiple lines: {code:?}"
            );
            assert!(
                link.size.height > px(100.),
                "long file links need multiple lines: {link:?}"
            );
            assert!(link.top() >= code.bottom());
        });
    }
}
