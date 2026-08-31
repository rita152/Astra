//! Native file-approval, file-change activity, and diff-review surfaces.
//!
//! The presentation models in this module deliberately do not depend on the
//! app-server protocol. Geometry, copy, and colors are based on the ChatGPT
//! desktop CDP captures in
//! `artifacts/chatgpt-p0-ui-cdp-audit-2026-08-30/{24..45}-*.{json,png}`.

use std::{
    path::{Component, Path, PathBuf},
    rc::Rc,
};

use gpui::{
    App, BoxShadow, Div, FontWeight, Role, ScrollHandle, SharedString, Stateful, Window, div,
    point, prelude::*, px, rgba,
};

use crate::{
    components::icons::icon,
    theme::{Theme, ThemeMode, UI_FONT_FAMILY, UI_MONOSPACE_FONT_FAMILY},
};

pub const FILE_APPROVAL_HEADER_HEIGHT: f32 = 76.0;
pub const FILE_APPROVAL_ACTIONS_HEIGHT: f32 = 52.0;
pub const FILE_APPROVAL_BUTTON_HEIGHT: f32 = 28.0;
pub const FILE_APPROVAL_MENU_TOP: f32 = 64.0;
pub const FILE_APPROVAL_MENU_WIDTH: f32 = 168.0;
pub const FILE_APPROVAL_MENU_HEIGHT: f32 = 67.125;
pub const FILE_APPROVAL_MENU_ROW_HEIGHT: f32 = 28.5625;
pub const FILE_APPROVAL_FILE_ROW_HEIGHT: f32 = 33.0;
pub const FILE_APPROVAL_FILES_VERTICAL_PADDING: f32 = 16.0;
pub const FILE_APPROVAL_FILES_MAX_HEIGHT: f32 = 200.0;

pub const FILE_CHANGE_ACTIVITY_HEIGHT: f32 = 64.5;
pub const FILE_CHANGE_ACTIVITY_ICON_SIZE: f32 = 40.0;
pub const FILE_CHANGE_FILE_ROW_HEIGHT: f32 = 36.0;
pub const FILE_CHANGE_FILE_LIST_BORDER_HEIGHT: f32 = 1.0;

pub const REVIEW_TAB_BAR_HEIGHT: f32 = 46.0;
pub const REVIEW_TOOLBAR_HEIGHT: f32 = 40.0;
pub const REVIEW_FILE_HEADER_HEIGHT: f32 = 32.0;
pub const REVIEW_FILE_COLLAPSED_HEIGHT: f32 = 34.0;
pub const REVIEW_DIFF_TOP_PADDING: f32 = 2.0;
pub const REVIEW_DIFF_LINE_HEIGHT: f32 = 21.59375;
/// The real `diffs-container` reserves a 15px horizontal scrollbar and the
/// surrounding full-review file adds 2px bottom padding.
pub const REVIEW_DIFF_BOTTOM_PADDING: f32 = 17.0;
pub const REVIEW_SUMMARY_WIDTH: f32 = 244.0;
pub const REVIEW_FILE_TREE_WIDTH: f32 = 250.0;
// GPUI floors the repeated files' final 0.5px when resolving scroll content.
// A 13px tail therefore reproduces the measured integer `scrollHeight=2533`
// and `maxScrollTop=1209` without changing any visible diff row.
pub const REVIEW_VIEWPORT_BOTTOM_PADDING: f32 = 13.0;
pub const REVIEW_GUTTER_WIDTH: f32 = 52.546875;
pub const REVIEW_GUTTER_BAR_WIDTH: f32 = 4.0;
pub const REVIEW_LINE_HORIZONTAL_PADDING: f32 = 7.2246094;

fn review_diff_row_top(index: usize) -> f32 {
    REVIEW_DIFF_TOP_PADDING + index as f32 * REVIEW_DIFF_LINE_HEIGHT
}

fn review_diff_rows_height(line_count: usize) -> f32 {
    REVIEW_DIFF_TOP_PADDING
        + line_count.max(1) as f32 * REVIEW_DIFF_LINE_HEIGHT
        + REVIEW_DIFF_BOTTOM_PADDING
}

fn element_id(prefix: &str, id: &str) -> SharedString {
    format!("{prefix}-{id}").into()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileApprovalStatus {
    #[default]
    Pending,
    Resolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileApprovalDecision {
    AllowOnce,
    AllowAllEdits,
    Decline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileApprovalMenuItem {
    AllowOnce,
    AllowAllEdits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileApprovalKeyboardFocus {
    Decline,
    AllowOnce,
    MenuToggle,
    MenuAllowOnce,
    MenuAllowAllEdits,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileApprovalVisualState {
    #[default]
    Default,
    AllowHovered,
    DeclineHovered,
    SplitMenu {
        focused: Option<FileApprovalMenuItem>,
    },
}

impl FileApprovalVisualState {
    pub fn menu_open(self) -> bool {
        matches!(self, Self::SplitMenu { .. })
    }

    #[cfg(test)]
    pub fn focused_menu_item(self) -> Option<FileApprovalMenuItem> {
        match self {
            Self::SplitMenu { focused } => focused,
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileApprovalPathPresentation {
    pub path: String,
    pub additions: u32,
    pub deletions: u32,
}

impl FileApprovalPathPresentation {
    pub fn new(path: impl Into<String>, additions: u32, deletions: u32) -> Self {
        Self {
            path: path.into(),
            additions,
            deletions,
        }
    }

    pub fn directory_and_name(&self) -> (&str, &str) {
        split_directory_and_name(&self.path)
    }
}

#[derive(Clone, Debug)]
pub struct FileApprovalPresentation {
    pub request_id: String,
    pub files: Vec<FileApprovalPathPresentation>,
    pub reason: Option<String>,
    pub status: FileApprovalStatus,
    pub visual_state: FileApprovalVisualState,
    pub keyboard_focus: Option<FileApprovalKeyboardFocus>,
    /// Stable native scroll state. Keeping the handle with the presentation
    /// prevents an unrelated rerender from snapping an eight-file request
    /// back to its first row.
    file_list_scroll: ScrollHandle,
}

impl PartialEq for FileApprovalPresentation {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id
            && self.files == other.files
            && self.reason == other.reason
            && self.status == other.status
            && self.visual_state == other.visual_state
            && self.keyboard_focus == other.keyboard_focus
    }
}

impl Eq for FileApprovalPresentation {}

impl FileApprovalPresentation {
    pub fn pending(
        request_id: impl Into<String>,
        files: Vec<FileApprovalPathPresentation>,
        reason: Option<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            files,
            reason,
            status: FileApprovalStatus::Pending,
            visual_state: FileApprovalVisualState::Default,
            keyboard_focus: None,
            file_list_scroll: ScrollHandle::new(),
        }
    }

    pub fn question(&self) -> &str {
        self.reason
            .as_deref()
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
            .unwrap_or("是否允许 ChatGPT 编辑以下文件？")
    }

    pub fn should_render(&self) -> bool {
        self.status == FileApprovalStatus::Pending
    }

    pub fn files_height(&self) -> f32 {
        FILE_APPROVAL_FILES_VERTICAL_PADDING + self.file_list_viewport_height()
    }

    pub fn file_list_content_height(&self) -> f32 {
        self.files.len().max(1) as f32 * FILE_APPROVAL_FILE_ROW_HEIGHT
    }

    pub fn file_list_viewport_height(&self) -> f32 {
        self.file_list_content_height()
            .min(FILE_APPROVAL_FILES_MAX_HEIGHT)
    }

    #[cfg(test)]
    pub fn file_list_scroll_handle(&self) -> ScrollHandle {
        self.file_list_scroll.clone()
    }

    pub fn scroll_file_list_to_bottom(&self) {
        self.file_list_scroll.scroll_to_bottom();
    }

    pub fn card_height(&self) -> f32 {
        FILE_APPROVAL_HEADER_HEIGHT + self.files_height() + FILE_APPROVAL_ACTIONS_HEIGHT
    }

    pub fn keyboard_event(&self, key: &str, shift: bool) -> Option<FileApprovalEvent> {
        match key {
            "tab" => {
                let next = if self.visual_state.menu_open() {
                    match (self.keyboard_focus, shift) {
                        (Some(FileApprovalKeyboardFocus::MenuAllowOnce), false) => {
                            FileApprovalKeyboardFocus::MenuAllowAllEdits
                        }
                        (Some(FileApprovalKeyboardFocus::MenuAllowAllEdits), false) => {
                            FileApprovalKeyboardFocus::MenuAllowOnce
                        }
                        (Some(FileApprovalKeyboardFocus::MenuAllowOnce), true) => {
                            FileApprovalKeyboardFocus::MenuAllowAllEdits
                        }
                        (Some(FileApprovalKeyboardFocus::MenuAllowAllEdits), true) => {
                            FileApprovalKeyboardFocus::MenuAllowOnce
                        }
                        (_, true) => FileApprovalKeyboardFocus::MenuAllowAllEdits,
                        _ => FileApprovalKeyboardFocus::MenuAllowOnce,
                    }
                } else {
                    match (self.keyboard_focus, shift) {
                        (Some(FileApprovalKeyboardFocus::Decline), false) => {
                            FileApprovalKeyboardFocus::AllowOnce
                        }
                        (Some(FileApprovalKeyboardFocus::AllowOnce), false) => {
                            FileApprovalKeyboardFocus::MenuToggle
                        }
                        (Some(FileApprovalKeyboardFocus::MenuToggle), false) => {
                            FileApprovalKeyboardFocus::Decline
                        }
                        (Some(FileApprovalKeyboardFocus::Decline), true) => {
                            FileApprovalKeyboardFocus::MenuToggle
                        }
                        (Some(FileApprovalKeyboardFocus::AllowOnce), true) => {
                            FileApprovalKeyboardFocus::Decline
                        }
                        (Some(FileApprovalKeyboardFocus::MenuToggle), true) => {
                            FileApprovalKeyboardFocus::AllowOnce
                        }
                        (_, true) => FileApprovalKeyboardFocus::MenuToggle,
                        _ => FileApprovalKeyboardFocus::Decline,
                    }
                };
                Some(FileApprovalEvent::KeyboardFocusChanged(Some(next)))
            }
            "escape" if self.visual_state.menu_open() => Some(FileApprovalEvent::ToggleMenu),
            "escape" => Some(FileApprovalEvent::Decision(FileApprovalDecision::Decline)),
            "enter" | "space" => match self.keyboard_focus {
                Some(FileApprovalKeyboardFocus::Decline) => {
                    Some(FileApprovalEvent::Decision(FileApprovalDecision::Decline))
                }
                Some(FileApprovalKeyboardFocus::MenuToggle) => Some(FileApprovalEvent::ToggleMenu),
                Some(FileApprovalKeyboardFocus::MenuAllowOnce) => {
                    Some(FileApprovalEvent::Decision(FileApprovalDecision::AllowOnce))
                }
                Some(FileApprovalKeyboardFocus::MenuAllowAllEdits) => Some(
                    FileApprovalEvent::Decision(FileApprovalDecision::AllowAllEdits),
                ),
                Some(FileApprovalKeyboardFocus::AllowOnce) | None
                    if !self.visual_state.menu_open() =>
                {
                    Some(FileApprovalEvent::Decision(FileApprovalDecision::AllowOnce))
                }
                _ => None,
            },
            "down" if self.visual_state.menu_open() => {
                let next = match self.keyboard_focus {
                    Some(FileApprovalKeyboardFocus::MenuAllowOnce) => {
                        FileApprovalKeyboardFocus::MenuAllowAllEdits
                    }
                    _ => FileApprovalKeyboardFocus::MenuAllowOnce,
                };
                Some(FileApprovalEvent::KeyboardFocusChanged(Some(next)))
            }
            "up" if self.visual_state.menu_open() => {
                let next = match self.keyboard_focus {
                    Some(FileApprovalKeyboardFocus::MenuAllowAllEdits) => {
                        FileApprovalKeyboardFocus::MenuAllowOnce
                    }
                    _ => FileApprovalKeyboardFocus::MenuAllowAllEdits,
                };
                Some(FileApprovalEvent::KeyboardFocusChanged(Some(next)))
            }
            _ => None,
        }
    }
}

/// Screenshot fixture backed by the original one-file captures plus the
/// naturally triggered two-file (08-22) and eight-file (43-46) captures in
/// `artifacts/chatgpt-multifile-diff-cdp-audit-2026-08-30`.
pub fn captured_file_approval_fixture(mode: ThemeMode, state: &str) -> FileApprovalPresentation {
    let files = if state.starts_with("multifile-") {
        vec![
            FileApprovalPathPresentation::new(
                "/Users/zp/Desktop/codex-cdp-multifile-approval-20260830-a.txt",
                1,
                0,
            ),
            FileApprovalPathPresentation::new(
                "/Users/zp/Desktop/codex-cdp-multifile-approval-20260830-b.txt",
                1,
                0,
            ),
        ]
    } else if state.starts_with("many-") {
        (1..=8)
            .map(|index| {
                FileApprovalPathPresentation::new(
                    format!("/Users/zp/Desktop/codex-cdp-multifile-scroll-20260830-{index:02}.txt"),
                    1,
                    0,
                )
            })
            .collect()
    } else {
        let path = match mode {
            ThemeMode::Light => "/Users/zp/Desktop/codex-cdp-file-approval-probe.txt",
            ThemeMode::Dark => "/Users/zp/Desktop/codex-cdp-file-approval-dark-probe.txt",
        };
        vec![FileApprovalPathPresentation::new(path, 1, 0)]
    };
    let mut model = FileApprovalPresentation::pending("file-approval-ui-capture", files, None);
    model.visual_state = if state.ends_with("allow-hover") {
        FileApprovalVisualState::AllowHovered
    } else if state.ends_with("decline-hover") {
        FileApprovalVisualState::DeclineHovered
    } else if state.ends_with("options-focus") {
        FileApprovalVisualState::SplitMenu {
            focused: Some(FileApprovalMenuItem::AllowOnce),
        }
    } else if state.ends_with("options") {
        FileApprovalVisualState::SplitMenu { focused: None }
    } else {
        FileApprovalVisualState::Default
    };
    if [
        "approved",
        "declined",
        "declined-immediate",
        "declined-resolved",
        "resolved",
        "timeout",
    ]
    .iter()
    .any(|suffix| state == *suffix || state.ends_with(&format!("-{suffix}")))
    {
        model.status = FileApprovalStatus::Resolved;
    }
    if state.ends_with("list-bottom") {
        model.scroll_file_list_to_bottom();
    }
    model
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileApprovalEvent {
    Decision(FileApprovalDecision),
    ToggleMenu,
    MenuFocusChanged(Option<FileApprovalMenuItem>),
    KeyboardFocusChanged(Option<FileApprovalKeyboardFocus>),
}

#[derive(Clone)]
pub struct FileApprovalCallback(Rc<dyn Fn(FileApprovalEvent, &mut Window, &mut App) + 'static>);

impl FileApprovalCallback {
    pub fn new(callback: impl Fn(FileApprovalEvent, &mut Window, &mut App) + 'static) -> Self {
        Self(Rc::new(callback))
    }

    fn emit(&self, event: FileApprovalEvent, window: &mut Window, cx: &mut App) {
        (self.0)(event, window, cx);
    }
}

#[derive(Clone, Copy)]
struct FilePalette {
    mode: ThemeMode,
    surface: gpui::Rgba,
    elevated: gpui::Rgba,
    card: gpui::Rgba,
    text: gpui::Rgba,
    tertiary: gpui::Rgba,
    approval_secondary: gpui::Rgba,
    approval_icon: gpui::Rgba,
    approval_decline_text: gpui::Rgba,
    approval_button_border: gpui::Rgba,
    approval_primary_text: gpui::Rgba,
    outline: gpui::Rgba,
    soft: gpui::Rgba,
    soft_hover: gpui::Rgba,
    primary: gpui::Rgba,
    primary_hover: gpui::Rgba,
    primary_text: gpui::Rgba,
    menu: gpui::Rgba,
    added: gpui::Rgba,
    deleted: gpui::Rgba,
}

impl FilePalette {
    fn for_theme(theme: Theme) -> Self {
        if theme.surface == rgba(0x181818ff) {
            Self {
                mode: ThemeMode::Dark,
                surface: rgba(0x181818ff),
                elevated: rgba(0x232323ff),
                // CDP 39-43 rasterize the approval surface to a flat #2c2c2c.
                // Keep this opaque: GPUI's premultiplication otherwise rounds
                // the captured card up to #2d2d2d.
                card: rgba(0x2c2c2cff),
                text: rgba(0xdfdfdfff),
                tertiary: rgba(0xffffff7f),
                approval_secondary: rgba(0xdfdfdf80),
                approval_icon: rgba(0xdfdfdfa6),
                approval_decline_text: rgba(0xdfdfdfca),
                approval_button_border: rgba(0xffffff15),
                approval_primary_text: rgba(0x2d2d2ddb),
                // `border-default` is rgba(255, 255, 255, .082) in the CDP
                // captures. `Rgba::alpha` replaces (rather than multiplies)
                // alpha, so callers must use this token directly.
                outline: rgba(0xffffff15),
                soft: rgba(0xffffff08),
                soft_hover: rgba(0xffffff14),
                primary: rgba(0xdfdfdfff),
                primary_hover: rgba(0xdfdfdfcc),
                primary_text: rgba(0x2d2d2dff),
                menu: rgba(0x2d2d2dff),
                // The CDP text tokens are already composited in the capture;
                // use the measured full-stroke pixels so CoreText does not
                // render the counters over-saturated.
                added: rgba(0x6bc67fff),
                deleted: rgba(0xe75248ff),
            }
        } else {
            Self {
                mode: ThemeMode::Light,
                surface: rgba(0xffffffff),
                elevated: rgba(0xffffffff),
                card: rgba(0xffffffff),
                text: rgba(0x1a1c1fff),
                tertiary: rgba(0x1a1c1f7e),
                approval_secondary: rgba(0x1a1c1f74),
                approval_icon: rgba(0x1a1c1fa6),
                approval_decline_text: rgba(0x1a1c1fdb),
                approval_button_border: rgba(0x1a1c1f14),
                approval_primary_text: rgba(0xffffffb3),
                outline: rgba(0x1a1c1f14),
                soft: rgba(0xfffffff5),
                soft_hover: rgba(0x1a1c1f0e),
                primary: rgba(0x1a1c1fff),
                primary_hover: rgba(0x1a1c1fcc),
                primary_text: rgba(0xffffffff),
                menu: rgba(0xffffffff),
                added: rgba(0x48a04dff),
                deleted: rgba(0xab352cff),
            }
        }
    }

    fn approval_shadows(self) -> Vec<BoxShadow> {
        let (short_shadow, ambient_shadow, outline) = match self.mode {
            ThemeMode::Light => (rgba(0x0000000d), rgba(0x00000007), rgba(0x1a1c1f10)),
            ThemeMode::Dark => (rgba(0x0000000a), rgba(0x0000000d), rgba(0xffffff14)),
        };
        vec![
            BoxShadow::new(px(0.0), px(0.0), outline.into()).spread_radius(px(0.5)),
            BoxShadow::new(px(0.0), px(3.0), short_shadow.into()).blur_radius(px(7.5)),
            BoxShadow::new(px(0.0), px(0.0), ambient_shadow.into()).blur_radius(px(20.0)),
        ]
    }
}

/// Render the captured Edit files approval card. Resolved requests are
/// intentionally unmounted, matching both immediate decisions and
/// `serverRequest/resolved` in captures 29/30/44/45.
pub fn render_file_approval_card(
    model: &FileApprovalPresentation,
    theme: Theme,
    callback: FileApprovalCallback,
) -> Option<Stateful<Div>> {
    if !model.should_render() {
        return None;
    }

    let palette = FilePalette::for_theme(theme);
    let header = div()
        .id(element_id("file-approval-header", &model.request_id))
        .role(Role::Alert)
        .aria_label(format!("编辑文件，{}", model.question()))
        .h(px(FILE_APPROVAL_HEADER_HEIGHT))
        .px(px(16.0))
        .pt(px(16.0))
        .pb(px(12.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .h(px(20.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(12.95))
                .line_height(px(20.0))
                .font_weight(FontWeight::NORMAL)
                .text_color(palette.approval_secondary)
                .child(icon("file-approval-edit", palette.approval_icon.into()).size(px(18.0)))
                .child("编辑文件"),
        )
        .child(
            div()
                .h(px(20.0))
                .min_w(px(0.0))
                .overflow_hidden()
                .text_size(px(13.95))
                .line_height(px(20.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(palette.text)
                .child(model.question().to_owned()),
        );

    let files = render_approval_files(model, palette);
    let actions = render_file_approval_actions(model, palette, callback.clone());

    let card_height = model.card_height();
    let card = div()
        .h(px(card_height))
        .w_full()
        .overflow_hidden()
        .rounded(px(25.0))
        .bg(palette.card)
        .shadow(palette.approval_shadows())
        .child(header)
        .child(files)
        .child(actions);

    let mut result = div()
        .id(element_id("file-approval-card", &model.request_id))
        .relative()
        // CDP's conversation column starts at x=.671875. Match that phase so
        // native text and icons land on the same device pixels as Chromium.
        .left(px(0.671875))
        .h(px(card_height))
        .w_full()
        .child(card);

    if let FileApprovalVisualState::SplitMenu { focused } = model.visual_state {
        result = result.child(render_file_approval_menu(
            &model.request_id,
            focused,
            palette,
            callback,
        ));
    }
    Some(result)
}

fn render_approval_files(model: &FileApprovalPresentation, palette: FilePalette) -> Stateful<Div> {
    let mut list = div()
        .id(element_id("file-approval-files-scroll", &model.request_id))
        .h(px(model.file_list_viewport_height()))
        // The real renderer caps the repeated file list at 200px and keeps
        // every file reachable through native scrolling.
        .max_h(px(FILE_APPROVAL_FILES_MAX_HEIGHT))
        .overflow_y_scroll()
        .track_scroll(&model.file_list_scroll)
        .flex()
        .flex_col();

    for (index, file) in model.files.iter().enumerate() {
        let (directory, name) = file.directory_and_name();
        list = list.child(
            div()
                .id(element_id(
                    "file-approval-file",
                    &format!("{}-{index}", model.request_id),
                ))
                .h(px(FILE_APPROVAL_FILE_ROW_HEIGHT))
                .flex_none()
                .min_w(px(0.0))
                .role(Role::Group)
                .aria_label(format!(
                    "{}，增加 {} 行，删除 {} 行",
                    file.path, file.additions, file.deletions
                ))
                .px(px(6.0))
                .flex()
                .items_center()
                .rounded(px(11.0))
                // The computed fill is transparent in CDP 24/39. Paint the
                // identical opaque card color here so GPUI's shadow primitive
                // cannot show through the row interior.
                .bg(palette.card)
                // Electron applies `elevation-stroke` as a shadow, so it does
                // not consume a layout pixel like a CSS border would.
                .shadow(vec![
                    BoxShadow::new(px(0.0), px(0.0), palette.outline.into()).spread_radius(px(0.5)),
                ])
                .text_size(px(14.35))
                .line_height(px(21.0))
                .font_family(UI_FONT_FAMILY)
                .text_color(palette.text)
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .items_center()
                        .font_weight(FontWeight::NORMAL)
                        .truncate()
                        .child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
                                .text_color(palette.tertiary)
                                .child(directory.to_owned()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .font_weight(FontWeight::MEDIUM)
                                .child(name.to_owned()),
                        ),
                )
                .child(approval_change_counts(
                    file.additions,
                    file.deletions,
                    palette,
                )),
        );
    }
    div()
        .id(element_id("file-approval-files", &model.request_id))
        .h(px(model.files_height()))
        .px(px(16.0))
        .py(px(8.0))
        .child(list)
}

fn render_file_approval_actions(
    model: &FileApprovalPresentation,
    palette: FilePalette,
    callback: FileApprovalCallback,
) -> Div {
    let decline_callback = callback.clone();
    let decline = div()
        .id(element_id("file-approval-decline", &model.request_id))
        .role(Role::Button)
        .aria_label("拒绝文件修改")
        .h(px(FILE_APPROVAL_BUTTON_HEIGHT))
        .w(px(80.156_25))
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded(px(9999.0))
        .border_1()
        .border_color(palette.approval_button_border)
        .bg(
            if model.visual_state == FileApprovalVisualState::DeclineHovered {
                palette.soft_hover
            } else {
                palette.soft
            },
        )
        .text_size(px(12.95))
        .line_height(px(18.0))
        .text_color(palette.approval_decline_text)
        .cursor_pointer()
        .hover(move |button| button.bg(palette.soft_hover))
        .on_click(move |_, window, cx| {
            decline_callback.emit(
                FileApprovalEvent::Decision(FileApprovalDecision::Decline),
                window,
                cx,
            );
        })
        .child("拒绝")
        .child(keycap("Esc", palette.approval_decline_text));

    let primary_fill = if matches!(
        model.visual_state,
        FileApprovalVisualState::AllowHovered | FileApprovalVisualState::SplitMenu { .. }
    ) {
        palette.primary_hover
    } else {
        palette.primary
    };
    let allow_callback = callback.clone();
    let allow = div()
        .id(element_id("file-approval-once", &model.request_id))
        .role(Role::Button)
        .aria_label("允许一次")
        .h(px(FILE_APPROVAL_BUTTON_HEIGHT))
        .w(px(93.140_625))
        .pl(px(8.0))
        .pr(px(4.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded_l(px(9999.0))
        .border_t_1()
        .border_b_1()
        .border_l_1()
        .border_color(palette.approval_button_border)
        .bg(primary_fill)
        .text_size(px(12.95))
        .line_height(px(18.0))
        .text_color(palette.approval_primary_text)
        .cursor_pointer()
        .hover(move |button| button.bg(palette.primary_hover))
        .on_click(move |_, window, cx| {
            allow_callback.emit(
                FileApprovalEvent::Decision(FileApprovalDecision::AllowOnce),
                window,
                cx,
            );
        })
        .child("允许一次")
        .child(keycap("⏎", palette.approval_primary_text));

    let menu_callback = callback;
    let menu_toggle = div()
        .id(element_id("file-approval-menu-toggle", &model.request_id))
        .role(Role::Button)
        .aria_label("审批选项")
        .h(px(FILE_APPROVAL_BUTTON_HEIGHT))
        .w(px(23.0))
        .pl(px(2.0))
        .pr(px(6.0))
        .flex()
        .items_center()
        .rounded_r(px(9999.0))
        .border_t_1()
        .border_r_1()
        .border_b_1()
        .border_color(palette.approval_button_border)
        .bg(primary_fill)
        .cursor_pointer()
        .hover(move |button| button.bg(palette.primary_hover))
        .on_click(move |_, window, cx| {
            menu_callback.emit(FileApprovalEvent::ToggleMenu, window, cx);
        })
        .child(
            icon(
                "chevron-down",
                palette.approval_primary_text.alpha(0.50).into(),
            )
            .size(px(14.0)),
        );

    div()
        .h(px(FILE_APPROVAL_ACTIONS_HEIGHT))
        .px(px(16.0))
        .pt(px(8.0))
        .pb(px(16.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(div().flex_1())
        .child(decline)
        .child(
            div()
                .min_w(px(0.0))
                .flex()
                .items_stretch()
                .overflow_hidden()
                .rounded(px(9999.0))
                .child(allow)
                .child(menu_toggle),
        )
}

fn render_file_approval_menu(
    request_id: &str,
    focused: Option<FileApprovalMenuItem>,
    palette: FilePalette,
    callback: FileApprovalCallback,
) -> Stateful<Div> {
    div()
        .id(element_id("file-approval-menu", request_id))
        .role(Role::Menu)
        .aria_label("审批选项")
        .absolute()
        .top(px(FILE_APPROVAL_MENU_TOP))
        .right(px(16.0))
        .w(px(FILE_APPROVAL_MENU_WIDTH))
        .h(px(FILE_APPROVAL_MENU_HEIGHT))
        .p(px(4.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .rounded(px(15.0))
        .bg(palette.menu)
        .font_family(UI_FONT_FAMILY)
        .shadow(vec![
            BoxShadow::new(px(0.0), px(0.0), palette.outline.into()).spread_radius(px(0.5)),
            BoxShadow::new(px(0.0), px(8.0), rgba(0x0000001f).into())
                .blur_radius(px(16.0))
                .spread_radius(px(-4.0)),
        ])
        .child(file_approval_menu_row(
            element_id("file-approval-menu-once", request_id),
            "允许一次",
            FileApprovalMenuItem::AllowOnce,
            focused == Some(FileApprovalMenuItem::AllowOnce),
            palette,
            callback.clone(),
        ))
        .child(file_approval_menu_row(
            element_id("file-approval-menu-all", request_id),
            "允许所有修改",
            FileApprovalMenuItem::AllowAllEdits,
            focused == Some(FileApprovalMenuItem::AllowAllEdits),
            palette,
            callback,
        ))
}

fn file_approval_menu_row(
    id: SharedString,
    label: &'static str,
    item: FileApprovalMenuItem,
    focused: bool,
    palette: FilePalette,
    callback: FileApprovalCallback,
) -> Stateful<Div> {
    let hover_callback = callback.clone();
    div()
        .id(id)
        .role(Role::MenuItem)
        .h(px(FILE_APPROVAL_MENU_ROW_HEIGHT))
        .w_full()
        .px(px(8.0))
        .py(px(5.0))
        .flex()
        .items_center()
        .rounded(px(12.5))
        .when(focused, |row| row.bg(palette.soft_hover))
        .text_size(px(13.0))
        .line_height(px(18.5714))
        .text_color(palette.text)
        .cursor_pointer()
        .hover(move |row| row.bg(palette.soft_hover))
        .on_hover(move |hovered, window, cx| {
            hover_callback.emit(
                FileApprovalEvent::MenuFocusChanged((*hovered).then_some(item)),
                window,
                cx,
            );
        })
        .on_click(move |_, window, cx| {
            let decision = match item {
                FileApprovalMenuItem::AllowOnce => FileApprovalDecision::AllowOnce,
                FileApprovalMenuItem::AllowAllEdits => FileApprovalDecision::AllowAllEdits,
            };
            callback.emit(FileApprovalEvent::Decision(decision), window, cx);
        })
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(div().min_w(px(0.0)).flex_1().truncate().child(label))
                .when(item == FileApprovalMenuItem::AllowAllEdits, |content| {
                    content.child(
                        icon("file-approval-info", palette.text.alpha(0.75).into()).size(px(16.0)),
                    )
                }),
        )
}

fn keycap(label: &'static str, color: gpui::Rgba) -> Div {
    div()
        .h(px(16.0))
        .min_w(px(16.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .bg(color.alpha(0.10))
        .text_size(px(12.0))
        .line_height(px(16.0))
        .text_color(color)
        .child(label)
}

fn change_counts(additions: u32, deletions: u32, palette: FilePalette) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(4.0))
        .text_size(px(13.0))
        .line_height(px(18.0))
        .child(
            div()
                .text_color(palette.added)
                .child(format!("+{additions}")),
        )
        .child(
            div()
                .text_color(palette.deleted)
                .child(format!("-{deletions}")),
        )
}

fn approval_change_counts(additions: u32, deletions: u32, palette: FilePalette) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(4.0))
        .text_size(px(14.0))
        .line_height(px(21.0))
        .child(
            div()
                .text_color(palette.added)
                .child(format!("+{additions}")),
        )
        .child(
            div()
                .text_color(palette.deleted)
                .child(format!("-{deletions}")),
        )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileChangeActivityVisualState {
    #[default]
    Default,
    HeaderHovered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChangeActivityPresentation {
    pub item_id: String,
    pub path: String,
    pub additions: u32,
    pub deletions: u32,
    pub visual_state: FileChangeActivityVisualState,
    pub details_expanded: bool,
    pub review: DiffReviewPresentation,
}

impl FileChangeActivityPresentation {
    pub fn edited(
        item_id: impl Into<String>,
        path: impl Into<String>,
        additions: u32,
        deletions: u32,
    ) -> Self {
        let item_id = item_id.into();
        let path = path.into();
        let review = DiffReviewPresentation::new(
            format!("file-change-{item_id}"),
            "上一轮",
            vec![DiffFilePresentation {
                path: path.clone(),
                resolved_path: Some(path.clone()),
                additions,
                deletions,
                lines: Vec::new(),
                visual_state: DiffFileVisualState::Expanded,
            }],
        );
        Self {
            item_id,
            path,
            additions,
            deletions,
            visual_state: FileChangeActivityVisualState::Default,
            details_expanded: false,
            review,
        }
    }

    pub fn with_review(mut self, review: DiffReviewPresentation) -> Self {
        // `path` is only the legacy single-file title fallback. Multi-file
        // activities render every entry from `review.files` below and must not
        // silently collapse their identity to the first file.
        if review.files.len() == 1 {
            self.path = review.files[0].path.clone();
        }
        self.additions = review.total_additions();
        self.deletions = review.total_deletions();
        self.details_expanded = review.files.len() > 1;
        self.review = review;
        self
    }

    pub fn title(&self) -> String {
        if self.review.files.len() > 1 {
            return format!("已编辑 {} 个文件", self.review.files.len());
        }
        let (_, name) = split_directory_and_name(&self.path);
        format!("已编辑 {name}")
    }

    pub fn geometry_height(&self) -> f32 {
        FILE_CHANGE_ACTIVITY_HEIGHT
            + if self.details_expanded {
                FILE_CHANGE_FILE_LIST_BORDER_HEIGHT
                    + self.review.files.len() as f32 * FILE_CHANGE_FILE_ROW_HEIGHT
            } else {
                0.0
            }
    }
}

/// Completed two-file fixture from natural captures 26/27. Each file and the
/// +2/-0 aggregate come directly from the correlated `item/completed` item.
pub fn captured_file_change_activity_fixture(state: &str) -> FileChangeActivityPresentation {
    let review = DiffReviewPresentation::new(
        "file-change-ui-capture-review",
        "上一轮",
        vec![
            DiffFilePresentation {
                path: "/Users/zp/Desktop/codex-cdp-multifile-approval-20260830-a.txt".to_owned(),
                resolved_path: Some(
                    "/Users/zp/Desktop/codex-cdp-multifile-approval-20260830-a.txt".to_owned(),
                ),
                additions: 1,
                deletions: 0,
                lines: vec![DiffLinePresentation::added(1, "APPROVAL_A")],
                visual_state: DiffFileVisualState::Expanded,
            },
            DiffFilePresentation {
                path: "/Users/zp/Desktop/codex-cdp-multifile-approval-20260830-b.txt".to_owned(),
                resolved_path: Some(
                    "/Users/zp/Desktop/codex-cdp-multifile-approval-20260830-b.txt".to_owned(),
                ),
                additions: 1,
                deletions: 0,
                lines: vec![DiffLinePresentation::added(1, "APPROVAL_B")],
                visual_state: DiffFileVisualState::Expanded,
            },
        ],
    );
    let mut model = FileChangeActivityPresentation::edited(
        "file-change-ui-capture",
        review.files[0].path.clone(),
        review.total_additions(),
        review.total_deletions(),
    )
    .with_review(review);
    if matches!(state, "hover" | "header-hover" | "completed-hover") {
        model.visual_state = FileChangeActivityVisualState::HeaderHovered;
    }
    if matches!(state, "collapsed" | "completed-collapsed") {
        model.details_expanded = false;
    }
    model
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileChangeActivityEvent {
    OpenReview(DiffReviewPresentation),
}

#[derive(Clone)]
pub struct FileChangeActivityCallback(
    Rc<dyn Fn(FileChangeActivityEvent, &mut Window, &mut App) + 'static>,
);

impl FileChangeActivityCallback {
    pub fn new(
        callback: impl Fn(FileChangeActivityEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self(Rc::new(callback))
    }

    fn emit(&self, event: FileChangeActivityEvent, window: &mut Window, cx: &mut App) {
        (self.0)(event, window, cx);
    }
}

pub fn render_file_change_activity(
    model: &FileChangeActivityPresentation,
    theme: Theme,
    callback: FileChangeActivityCallback,
) -> Stateful<Div> {
    let palette = FilePalette::for_theme(theme);
    let (activity_outline, icon_tile, icon_color) = match palette.mode {
        // The light CDP strict crop starts just inside the 0.5px Electron
        // elevation stroke. Painting that stroke at GPUI's different vertical
        // raster phase leaves a false full-width grey row in the local crop.
        ThemeMode::Light => (rgba(0x1a1c1f00), rgba(0xf7f7f7ff), rgba(0x8b8c8eff)),
        ThemeMode::Dark => (
            // GPUI covers the half-pixel spread more strongly than Chromium;
            // 0x0b produces the captured #2a edge over the #232323 card.
            rgba(0xffffff0b),
            rgba(0x151515ff),
            rgba(0x777777ff),
        ),
    };
    let hovered = model.visual_state == FileChangeActivityVisualState::HeaderHovered;
    let review_callback = callback.clone();
    let review = model.review.clone();

    let header = div()
        .h(px(FILE_CHANGE_ACTIVITY_HEIGHT))
        .flex_none()
        .px(px(12.0))
        .py(px(12.0))
        .flex()
        .items_center()
        .gap(px(10.0))
        .when(hovered, |header| header.bg(palette.soft_hover))
        .hover(move |header| header.bg(palette.soft_hover))
        .child(
            div()
                .size(px(FILE_CHANGE_ACTIVITY_ICON_SIZE))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(12.5))
                .bg(icon_tile)
                .child(icon("panel-review", icon_color.into()).size(px(24.0))),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_size(px(14.0))
                        .line_height(px(21.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(palette.text)
                        .child(model.title()),
                )
                .child(div().h(px(19.5)).flex().items_center().child(change_counts(
                    model.additions,
                    model.deletions,
                    palette,
                ))),
        )
        .child(
            // The captured app also exposes Undo here, but GPUI does not yet
            // own a reversible file operation or a confirmed server-side
            // revert. Keep the action absent instead of presenting a
            // destructive-looking dismiss-only control.
            div().flex_none().flex().items_center().child(
                div()
                    .id(element_id("file-change-review", &model.item_id))
                    .role(Role::Button)
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(12.5))
                    .border_1()
                    .border_color(palette.outline)
                    .bg(palette.soft)
                    .text_size(px(14.0))
                    .line_height(px(18.0))
                    .text_color(palette.text)
                    .cursor_pointer()
                    .hover(move |button| button.bg(palette.soft_hover))
                    .on_click(move |_, window, cx| {
                        review_callback.emit(
                            FileChangeActivityEvent::OpenReview(review.clone()),
                            window,
                            cx,
                        );
                    })
                    .child("审核"),
            ),
        );

    let mut card = div()
        .id(element_id("file-change-activity", &model.item_id))
        .role(Role::Group)
        .aria_label(format!(
            "{}，增加 {} 行，删除 {} 行",
            model.title(),
            model.additions,
            model.deletions
        ))
        .relative()
        .h(px(model.geometry_height()))
        .w_full()
        .overflow_hidden()
        .flex()
        .flex_col()
        .rounded(px(12.5))
        .bg(palette.elevated)
        .shadow(vec![
            BoxShadow::new(px(0.0), px(0.0), activity_outline.into()).spread_radius(px(0.5)),
        ])
        .child(header);

    if model.details_expanded {
        let mut details = div()
            .h(px(FILE_CHANGE_FILE_LIST_BORDER_HEIGHT
                + model.review.files.len() as f32
                    * FILE_CHANGE_FILE_ROW_HEIGHT))
            .flex_none()
            .border_t_1()
            .border_color(palette.outline)
            .flex()
            .flex_col();
        for (index, file) in model.review.files.iter().enumerate() {
            let row_callback = callback.clone();
            let row_review = model.review.clone();
            let (directory, name) = split_directory_and_name(&file.path);
            details = details.child(
                div()
                    .id(element_id(
                        "file-change-file-row",
                        &format!("{}-{index}", model.item_id),
                    ))
                    .role(Role::Button)
                    .h(px(FILE_CHANGE_FILE_ROW_HEIGHT))
                    .w_full()
                    .flex_none()
                    .px(px(12.0))
                    .py(px(4.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .bg(palette.surface.alpha(0.70))
                    .cursor_pointer()
                    .hover(move |row| row.bg(palette.soft_hover))
                    .on_click(move |_, window, cx| {
                        row_callback.emit(
                            FileChangeActivityEvent::OpenReview(row_review.clone()),
                            window,
                            cx,
                        );
                    })
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .items_center()
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_color(palette.tertiary)
                                    .child(directory.to_owned()),
                            )
                            .child(
                                div()
                                    .max_w_full()
                                    .flex_none()
                                    .truncate()
                                    .text_color(palette.text)
                                    .child(name.to_owned()),
                            ),
                    )
                    .child(approval_change_counts(
                        file.additions,
                        file.deletions,
                        palette,
                    )),
            );
        }
        card = card.child(details);
    }

    card
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Added,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLinePresentation {
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub content: String,
    pub kind: DiffLineKind,
}

impl DiffLinePresentation {
    pub fn added(new_line: u32, content: impl Into<String>) -> Self {
        Self {
            old_line: None,
            new_line: Some(new_line),
            content: content.into(),
            kind: DiffLineKind::Added,
        }
    }

    pub fn deleted(old_line: u32, content: impl Into<String>) -> Self {
        Self {
            old_line: Some(old_line),
            new_line: None,
            content: content.into(),
            kind: DiffLineKind::Deleted,
        }
    }

    pub fn context(old_line: u32, new_line: u32, content: impl Into<String>) -> Self {
        Self {
            old_line: Some(old_line),
            new_line: Some(new_line),
            content: content.into(),
            kind: DiffLineKind::Context,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffFileVisualState {
    Collapsed,
    #[default]
    Expanded,
    HeaderHovered,
}

impl DiffFileVisualState {
    pub fn is_expanded(self) -> bool {
        !matches!(self, Self::Collapsed)
    }

    pub fn header_hovered(self) -> bool {
        matches!(self, Self::HeaderHovered)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffFilePresentation {
    pub path: String,
    /// Canonical filesystem target used by native Open/Reveal actions. The
    /// display path can remain relative exactly as emitted by the review UI.
    pub resolved_path: Option<String>,
    pub additions: u32,
    pub deletions: u32,
    pub lines: Vec<DiffLinePresentation>,
    pub visual_state: DiffFileVisualState,
}

impl DiffFilePresentation {
    pub fn geometry_height(&self) -> f32 {
        if self.visual_state.is_expanded() {
            REVIEW_FILE_HEADER_HEIGHT + review_diff_rows_height(self.lines.len())
        } else {
            REVIEW_FILE_COLLAPSED_HEIGHT
        }
    }
}

#[derive(Clone, Debug)]
pub struct DiffReviewPresentation {
    pub review_id: String,
    pub turn_label: String,
    pub files: Vec<DiffFilePresentation>,
    pub show_summary: bool,
    pub show_file_tree: bool,
    /// Stable vertical scroll state for the full Review viewport.
    review_scroll: ScrollHandle,
}

impl PartialEq for DiffReviewPresentation {
    fn eq(&self, other: &Self) -> bool {
        self.review_id == other.review_id
            && self.turn_label == other.turn_label
            && self.files == other.files
            && self.show_summary == other.show_summary
            && self.show_file_tree == other.show_file_tree
    }
}

impl Eq for DiffReviewPresentation {}

impl DiffReviewPresentation {
    pub fn new(
        review_id: impl Into<String>,
        turn_label: impl Into<String>,
        files: Vec<DiffFilePresentation>,
    ) -> Self {
        Self {
            review_id: review_id.into(),
            turn_label: turn_label.into(),
            files,
            show_summary: false,
            show_file_tree: true,
            review_scroll: ScrollHandle::new(),
        }
    }

    pub fn total_additions(&self) -> u32 {
        self.files.iter().map(|file| file.additions).sum()
    }

    pub fn total_deletions(&self) -> u32 {
        self.files.iter().map(|file| file.deletions).sum()
    }

    #[cfg(test)]
    pub fn review_scroll_handle(&self) -> ScrollHandle {
        self.review_scroll.clone()
    }

    pub fn set_review_scroll_top(&self, scroll_top: f32) {
        self.review_scroll
            .set_offset(point(px(0.0), px(-scroll_top.max(0.0))));
    }

    pub fn from_unified_diff(
        review_id: impl Into<String>,
        turn_label: impl Into<String>,
        diff: &str,
        cwd: Option<&Path>,
    ) -> Self {
        let mut files = Vec::<DiffFilePresentation>::new();
        let mut current: Option<DiffFilePresentation> = None;
        let mut old_line = 0_u32;
        let mut new_line = 0_u32;

        let finish_file = |current: &mut Option<DiffFilePresentation>,
                           files: &mut Vec<DiffFilePresentation>| {
            if let Some(mut file) = current.take() {
                file.additions = file
                    .lines
                    .iter()
                    .filter(|line| line.kind == DiffLineKind::Added)
                    .count() as u32;
                file.deletions = file
                    .lines
                    .iter()
                    .filter(|line| line.kind == DiffLineKind::Deleted)
                    .count() as u32;
                files.push(file);
            }
        };

        for raw_line in diff.lines() {
            if let Some(rest) = raw_line.strip_prefix("diff --git ") {
                finish_file(&mut current, &mut files);
                let protocol_path = rest
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .trim_start_matches("b/");
                let resolved_path = resolve_review_path(protocol_path, cwd);
                let display_path =
                    review_display_path(protocol_path, resolved_path.as_deref(), cwd);
                current = Some(DiffFilePresentation {
                    path: display_path,
                    resolved_path,
                    additions: 0,
                    deletions: 0,
                    lines: Vec::new(),
                    visual_state: DiffFileVisualState::Expanded,
                });
                continue;
            }
            if current.is_none() && raw_line.starts_with("@@ ") {
                current = Some(DiffFilePresentation {
                    path: "变更".to_owned(),
                    resolved_path: None,
                    additions: 0,
                    deletions: 0,
                    lines: Vec::new(),
                    visual_state: DiffFileVisualState::Expanded,
                });
            }
            if let Some((old_start, new_start)) = parse_hunk_header(raw_line) {
                old_line = old_start;
                new_line = new_start;
                continue;
            }
            let Some(file) = current.as_mut() else {
                continue;
            };
            if raw_line.starts_with("--- ") || raw_line.starts_with("+++ ") {
                continue;
            }
            if let Some(content) = raw_line.strip_prefix('+') {
                file.lines
                    .push(DiffLinePresentation::added(new_line, content));
                new_line = new_line.saturating_add(1);
            } else if let Some(content) = raw_line.strip_prefix('-') {
                file.lines
                    .push(DiffLinePresentation::deleted(old_line, content));
                old_line = old_line.saturating_add(1);
            } else if let Some(content) = raw_line.strip_prefix(' ') {
                file.lines
                    .push(DiffLinePresentation::context(old_line, new_line, content));
                old_line = old_line.saturating_add(1);
                new_line = new_line.saturating_add(1);
            }
        }
        finish_file(&mut current, &mut files);

        Self::new(review_id, turn_label, files)
    }
}

fn review_display_path(path: &str, resolved_path: Option<&str>, cwd: Option<&Path>) -> String {
    let protocol_path = Path::new(path);
    if !protocol_path.is_absolute() {
        return path.to_owned();
    }
    let Some(cwd) = cwd else {
        return path.to_owned();
    };
    let Some(resolved_path) = resolved_path.map(Path::new) else {
        return path.to_owned();
    };
    lexical_relative_path(resolved_path, cwd)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned())
}

fn lexical_relative_path(target: &Path, base: &Path) -> Option<PathBuf> {
    let target = target.components().collect::<Vec<_>>();
    let base = base.components().collect::<Vec<_>>();
    let common = target
        .iter()
        .zip(&base)
        .take_while(|(target, base)| target == base)
        .count();
    if common == 0 {
        return None;
    }

    let mut relative = PathBuf::new();
    for component in &base[common..] {
        if matches!(component, Component::Normal(_) | Component::ParentDir) {
            relative.push("..");
        }
    }
    for component in &target[common..] {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

fn resolve_review_path(path: &str, cwd: Option<&Path>) -> Option<String> {
    let path = Path::new(path);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd?.join(path)
    };
    Some(resolved.to_string_lossy().into_owned())
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    let header = line.strip_prefix("@@ -")?;
    let (old, remainder) = header.split_once(" +")?;
    let (new, _) = remainder.split_once(" @@")?;
    let parse_start = |range: &str| range.split(',').next()?.parse::<u32>().ok();
    Some((parse_start(old)?, parse_start(new)?))
}

/// Exact `turn/diff/updated` payload captured from the natural two-file
/// workflow. The file's SHA-256 is recorded in the audit as
/// `9808146c...6801b5`; `split('\n')` contains the captured 123 entries.
pub const CAPTURED_LONG_TWO_FILE_TURN_DIFF: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/cdp/turn-diff-updated-two-file-123-entries.diff"
));

/// Review fixture derived by the production parser from the exact natural
/// `turn/diff/updated` payload, rather than a hand-authored one-line stand-in.
pub fn captured_diff_review_fixture(state: &str) -> DiffReviewPresentation {
    let mut review = DiffReviewPresentation::from_unified_diff(
        "turn-diff-ui-capture",
        "上一轮",
        CAPTURED_LONG_TWO_FILE_TURN_DIFF,
        Some(Path::new("/Users/zp/Desktop/GPUI")),
    );
    if let Some(first) = review.files.first_mut() {
        first.visual_state = match state {
            "file-collapsed" | "first-file-collapsed" | "collapsed" => {
                DiffFileVisualState::Collapsed
            }
            "file-header-hover" | "first-header-hover" | "header-hover" | "hover" => {
                DiffFileVisualState::HeaderHovered
            }
            _ => DiffFileVisualState::Expanded,
        };
    }
    if matches!(state, "cross-file-scroll" | "scroll-1100") {
        review.set_review_scroll_top(1_100.0);
    } else if matches!(state, "bottom" | "second-file-bottom") {
        // The natural renderer reports a 1209px maximum. Request that measured
        // offset explicitly because this deterministic fixture is configured
        // before layout can resolve a generic `scroll_to_bottom` operation.
        review.set_review_scroll_top(1_209.0);
    }
    review
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffReviewEvent {
    Close,
    ToggleFile(usize),
    HeaderHoverChanged { index: usize, hovered: bool },
    CopyPath(String),
    OpenLocation(String),
}

#[derive(Clone)]
pub struct DiffReviewCallback(Rc<dyn Fn(DiffReviewEvent, &mut Window, &mut App) + 'static>);

impl DiffReviewCallback {
    pub fn new(callback: impl Fn(DiffReviewEvent, &mut Window, &mut App) + 'static) -> Self {
        Self(Rc::new(callback))
    }

    fn emit(&self, event: DiffReviewEvent, window: &mut Window, cx: &mut App) {
        (self.0)(event, window, cx);
    }
}

/// Render the native Review panel contents. The parent App shell owns panel
/// placement; this component owns the captured tab, review toolbar, summary,
/// file header, line, and file-tree states.
pub fn render_diff_review_panel(
    model: &DiffReviewPresentation,
    theme: Theme,
    callback: DiffReviewCallback,
) -> Stateful<Div> {
    let palette = FilePalette::for_theme(theme);
    let close_callback = callback.clone();
    let tab = div()
        .h(px(REVIEW_TAB_BAR_HEIGHT))
        .px(px(8.0))
        .flex()
        .items_center()
        .border_b_1()
        .border_color(palette.outline)
        .child(
            div()
                .h(px(28.0))
                .w(px(156.0))
                .px(px(8.0))
                .py(px(4.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(12.5))
                .bg(palette.surface)
                .text_size(px(13.0))
                .line_height(px(18.5714))
                .text_color(palette.text)
                .child(icon("panel-review", palette.text.into()).size(px(16.0)))
                .child(div().min_w(px(0.0)).flex_1().truncate().child("审查"))
                .child(
                    div()
                        .id(element_id("review-close", &model.review_id))
                        .role(Role::Button)
                        .size(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(10.0))
                        .text_color(palette.tertiary)
                        .cursor_pointer()
                        .hover(move |button| button.bg(palette.soft_hover))
                        .on_click(move |_, window, cx| {
                            close_callback.emit(DiffReviewEvent::Close, window, cx);
                        })
                        .child("×"),
                ),
        )
        .child(div().flex_1())
        .child("+");

    let toolbar = div()
        .h(px(REVIEW_TOOLBAR_HEIGHT))
        .px(px(8.0))
        .flex()
        .items_center()
        .border_b_1()
        .border_color(palette.outline)
        .text_size(px(14.0))
        .line_height(px(18.0))
        .text_color(palette.text)
        .child(
            div()
                .h(px(28.0))
                .px(px(6.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .rounded(px(12.5))
                .child(model.turn_label.clone())
                .child("⌄"),
        )
        .child(div().flex_1())
        .child(change_counts(
            model.total_additions(),
            model.total_deletions(),
            palette,
        ));

    let mut body = div().min_h(px(0.0)).flex_1().flex();
    if model.show_summary {
        body = body.child(render_review_summary(model, palette));
    }

    let mut diff_column = div()
        .id(element_id("review-diff-scroll", &model.review_id))
        .min_w(px(0.0))
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .restrict_scroll_to_axis()
        .track_scroll(&model.review_scroll)
        .pb(px(REVIEW_VIEWPORT_BOTTOM_PADDING))
        .flex()
        .flex_col()
        .bg(palette.surface);
    for (index, file) in model.files.iter().enumerate() {
        diff_column = diff_column.child(render_review_file(
            &model.review_id,
            index,
            file,
            palette,
            callback.clone(),
        ));
    }
    body = body.child(diff_column);

    if model.show_file_tree {
        body = body.child(render_review_file_tree(model, palette));
    }

    div()
        .id(element_id("diff-review", &model.review_id))
        .role(Role::Region)
        .aria_label("审查文件更改")
        .h_full()
        .w_full()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .bg(palette.surface)
        .text_color(palette.text)
        .child(tab)
        .child(toolbar)
        .child(body)
}

fn render_review_summary(model: &DiffReviewPresentation, palette: FilePalette) -> Div {
    div()
        .w(px(REVIEW_SUMMARY_WIDTH))
        .flex_none()
        .border_r_1()
        .border_color(palette.outline)
        .px(px(12.0))
        .py(px(8.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .text_size(px(14.0))
        .line_height(px(21.0))
        .child(
            div()
                .h(px(24.0))
                .flex()
                .items_center()
                .child("变更")
                .child(div().flex_1())
                .child(change_counts(
                    model.total_additions(),
                    model.total_deletions(),
                    palette,
                )),
        )
        .child("本地")
        .child("main")
        .child("提交或推送")
}

fn render_review_file(
    review_id: &str,
    index: usize,
    file: &DiffFilePresentation,
    palette: FilePalette,
    callback: DiffReviewCallback,
) -> Div {
    let expanded = file.visual_state.is_expanded();
    let header_hovered = file.visual_state.header_hovered();
    let header_toggle_callback = callback.clone();
    let toggle_callback = callback.clone();
    let hover_callback = callback.clone();
    let copy_callback = callback.clone();
    let open_callback = callback;
    let path_for_copy = file.path.clone();
    let path_for_open = file
        .resolved_path
        .clone()
        .unwrap_or_else(|| file.path.clone());

    let (_, name) = split_directory_and_name(&file.path);
    let header_background = if header_hovered {
        palette.soft_hover
    } else if palette.mode == ThemeMode::Dark {
        // The dark full-review sticky header composites the soft-active token
        // to #232323.
        rgba(0x232323ff)
    } else {
        // The light sticky header is `surface` mixed to 88% over the same
        // white Review surface, so it remains white until hovered.
        rgba(0xffffffff)
    };
    let (directory, _) = split_directory_and_name(&file.path);
    let header_action_opacity = if header_hovered { 1.0 } else { 0.0 };
    let header = div()
        .id(element_id(
            "review-file-header",
            &format!("{review_id}-{index}"),
        ))
        .h(px(REVIEW_FILE_HEADER_HEIGHT))
        .px(px(8.0))
        .pl(px(12.0))
        .flex()
        .items_center()
        .gap(px(2.0))
        .bg(header_background)
        .cursor_pointer()
        .hover(move |header| header.bg(palette.soft_hover))
        .on_hover(move |hovered, window, cx| {
            hover_callback.emit(
                DiffReviewEvent::HeaderHoverChanged {
                    index,
                    hovered: *hovered,
                },
                window,
                cx,
            );
        })
        .on_click(move |_, window, cx| {
            header_toggle_callback.emit(DiffReviewEvent::ToggleFile(index), window, cx);
        })
        .child(
            div()
                .min_w(px(0.0))
                .pl(px(4.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(14.0))
                .line_height(px(21.0))
                .child(icon("panel-files", palette.tertiary.into()).size(px(16.0)))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .truncate()
                        .text_color(palette.tertiary)
                        .child(directory.to_owned())
                        .child(div().text_color(palette.text).child(name.to_owned())),
                ),
        )
        .child(change_counts(file.additions, file.deletions, palette))
        .child(
            div()
                .id(element_id(
                    "review-copy-path",
                    &format!("{review_id}-{index}"),
                ))
                .role(Role::Button)
                .size(px(24.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.0))
                .opacity(header_action_opacity)
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    copy_callback.emit(
                        DiffReviewEvent::CopyPath(path_for_copy.clone()),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                })
                .child(icon("message-copy", palette.tertiary.into()).size(px(16.0))),
        )
        .child(
            div()
                .id(element_id(
                    "review-toggle-file",
                    &format!("{review_id}-{index}"),
                ))
                .role(Role::Button)
                .size(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.0))
                .text_color(palette.tertiary)
                .opacity(header_action_opacity)
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    toggle_callback.emit(DiffReviewEvent::ToggleFile(index), window, cx);
                    cx.stop_propagation();
                })
                .child(
                    icon(
                        if expanded {
                            "chevron-down"
                        } else {
                            "settings-chevron-right"
                        },
                        palette.tertiary.into(),
                    )
                    .size(px(16.0)),
                ),
        )
        .child(
            div()
                .id(element_id(
                    "review-open-location",
                    &format!("{review_id}-{index}"),
                ))
                .role(Role::Button)
                .size(px(20.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.0))
                .opacity(header_action_opacity)
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    open_callback.emit(
                        DiffReviewEvent::OpenLocation(path_for_open.clone()),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                })
                .child(icon("settings-external", palette.tertiary.into()).size(px(14.0))),
        )
        .child(div().flex_1());

    let mut result = div()
        .h(px(file.geometry_height()))
        .w_full()
        .flex_none()
        .overflow_hidden()
        .bg(palette.surface)
        .child(header);

    if expanded {
        let lines = if file.lines.is_empty() {
            vec![DiffLinePresentation::added(
                1,
                if name.is_empty() { "" } else { " " },
            )]
        } else {
            file.lines.clone()
        };
        // Chromium positions repeated 21.59375px diff rows against the
        // cumulative fractional offset, producing the measured 21/22px
        // raster cadence. A GPUI flex column snaps every child independently
        // to 22px and drifts several pixels before the file boundary. Keep the
        // exact logical height, but place every row from its cumulative top so
        // the native raster follows the captured cadence without dropping or
        // duplicating any line.
        let rows_height = review_diff_rows_height(lines.len());
        let mut rows = div().relative().w_full().h(px(rows_height)).flex_none();
        for (line_index, line) in lines.iter().enumerate() {
            rows = rows.child(
                render_diff_line(line, palette)
                    .absolute()
                    .top(px(review_diff_row_top(line_index)))
                    .left_0()
                    .right_0(),
            );
        }
        result = result.child(rows);
    }
    result
}

fn render_diff_line(line: &DiffLinePresentation, palette: FilePalette) -> Div {
    let (background, gutter_background, accent, number_color) = match (palette.mode, line.kind) {
        (ThemeMode::Light, DiffLineKind::Added) => (
            rgba(0xe9f4e8ff),
            rgba(0xeff7eeff),
            Some(rgba(0x00a240ff)),
            rgba(0x00a240ff),
        ),
        (ThemeMode::Dark, DiffLineKind::Added) => (
            rgba(0x233125ff),
            rgba(0x162017ff),
            Some(rgba(0x40c977ff)),
            rgba(0x40c977ff),
        ),
        (ThemeMode::Light, DiffLineKind::Deleted) => (
            rgba(0xf8e7e3ff),
            rgba(0xfaedeaff),
            Some(rgba(0xba2623ff)),
            rgba(0xba2623ff),
        ),
        (ThemeMode::Dark, DiffLineKind::Deleted) => (
            rgba(0x38201cff),
            rgba(0x25140fff),
            Some(rgba(0xfa423eff)),
            rgba(0xfa423eff),
        ),
        (_, DiffLineKind::Context) => (palette.surface, palette.surface, None, palette.tertiary),
    };
    let line_number = match line.kind {
        DiffLineKind::Added => line.new_line,
        DiffLineKind::Deleted => line.old_line,
        DiffLineKind::Context => line.new_line.or(line.old_line),
    }
    .map(|line| line.to_string())
    .unwrap_or_default();

    let mut gutter = div()
        .h_full()
        .w(px(REVIEW_GUTTER_WIDTH))
        .flex_none()
        .flex()
        .items_center()
        .bg(gutter_background);
    if let Some(accent) = accent {
        gutter = gutter.child(
            div()
                .h_full()
                .w(px(REVIEW_GUTTER_BAR_WIDTH))
                .flex_none()
                .bg(accent),
        );
    } else {
        gutter = gutter.child(div().h_full().w(px(REVIEW_GUTTER_BAR_WIDTH)).flex_none());
    }
    gutter = gutter.child(
        div()
            .h_full()
            .min_w(px(0.0))
            .flex_1()
            .pr(px(REVIEW_LINE_HORIZONTAL_PADDING))
            .flex()
            .items_center()
            .justify_end()
            .text_color(number_color)
            .child(line_number),
    );

    div()
        .h(px(REVIEW_DIFF_LINE_HEIGHT))
        .w_full()
        .flex()
        .items_center()
        .font_family(UI_MONOSPACE_FONT_FAMILY)
        .text_size(px(12.0))
        .line_height(px(21.6))
        .text_color(palette.text)
        .child(gutter)
        .child(
            div()
                .h_full()
                .min_w(px(0.0))
                .flex_1()
                .px(px(REVIEW_LINE_HORIZONTAL_PADDING))
                .flex()
                .items_center()
                .bg(background)
                .child(line.content.clone()),
        )
}

fn render_review_file_tree(model: &DiffReviewPresentation, palette: FilePalette) -> Div {
    let mut tree = div()
        .w(px(REVIEW_FILE_TREE_WIDTH))
        .flex_none()
        .border_l_1()
        .border_color(palette.outline)
        .px(px(8.0))
        .pt(px(8.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .text_size(px(13.0))
        .line_height(px(18.0))
        .text_color(palette.text)
        .child(
            div()
                .h(px(28.0))
                .w_full()
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(12.5))
                .border_1()
                .border_color(palette.outline)
                .bg(palette.soft)
                .text_color(palette.tertiary)
                .child("⌕  筛选文件…"),
        );
    for (index, file) in model.files.iter().enumerate() {
        let (directory, name) = split_directory_and_name(&file.path);
        tree = tree
            .child(
                div()
                    .id(element_id(
                        "review-tree-directory",
                        &format!("{}-{index}", model.review_id),
                    ))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child("⌄")
                    .child(directory.trim_end_matches('/').to_owned()),
            )
            .child(
                div()
                    .id(element_id(
                        "review-tree-file",
                        &format!("{}-{index}", model.review_id),
                    ))
                    .h(px(28.0))
                    .pl(px(20.0))
                    .pr(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(7.5))
                    .bg(palette.soft_hover)
                    .child(icon("panel-files", palette.tertiary.into()).size(px(16.0)))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .child(name.to_owned()),
                    )
                    .child(
                        div()
                            .size(px(14.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(3.0))
                            .bg(palette.added)
                            .text_color(palette.primary_text)
                            .child("+"),
                    ),
            );
    }
    tree
}

fn split_directory_and_name(path: &str) -> (&str, &str) {
    let name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    let split = path.len().saturating_sub(name.len());
    path.split_at(split)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use gpui::{
        Bounds, Context, IntoElement, MouseButton, Render, ScrollDelta, ScrollWheelEvent, TestApp,
        TouchPhase, Window, WindowBounds, WindowOptions, point, px, size,
    };

    use super::*;

    struct FileChangeActivityHarness {
        model: FileChangeActivityPresentation,
        events: Rc<RefCell<Vec<FileChangeActivityEvent>>>,
    }

    impl Render for FileChangeActivityHarness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let events = self.events.clone();
            render_file_change_activity(
                &self.model,
                Theme::for_mode(ThemeMode::Dark),
                FileChangeActivityCallback::new(move |event, _, _| {
                    events.borrow_mut().push(event);
                }),
            )
        }
    }

    struct FileApprovalHarness {
        model: FileApprovalPresentation,
    }

    impl Render for FileApprovalHarness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            render_file_approval_card(
                &self.model,
                Theme::for_mode(ThemeMode::Dark),
                FileApprovalCallback::new(|_, _, _| {}),
            )
            .expect("pending approval remains mounted")
        }
    }

    struct DiffReviewHarness {
        model: DiffReviewPresentation,
        events: Rc<RefCell<Vec<DiffReviewEvent>>>,
    }

    impl Render for DiffReviewHarness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let events = self.events.clone();
            render_diff_review_panel(
                &self.model,
                Theme::for_mode(ThemeMode::Dark),
                DiffReviewCallback::new(move |event, _, _| {
                    events.borrow_mut().push(event);
                }),
            )
        }
    }

    fn approval() -> FileApprovalPresentation {
        FileApprovalPresentation::pending(
            "file-request-1",
            vec![FileApprovalPathPresentation::new(
                "/Users/zp/Desktop/codex-cdp-file-approval-probe.txt",
                1,
                0,
            )],
            None,
        )
    }

    fn review_file(state: DiffFileVisualState) -> DiffFilePresentation {
        DiffFilePresentation {
            path: "../../../../tmp/chatgpt-cdp-file-approval-approved.txt".to_owned(),
            resolved_path: Some("/tmp/chatgpt-cdp-file-approval-approved.txt".to_owned()),
            additions: 1,
            deletions: 0,
            lines: vec![DiffLinePresentation::added(1, "APPROVED_PROBE")],
            visual_state: state,
        }
    }

    #[test]
    fn file_approval_geometry_matches_cdp() {
        let model = approval();
        assert_eq!(model.files_height(), 49.0);
        assert_eq!(model.card_height(), 177.0);
        assert_eq!(FILE_APPROVAL_MENU_TOP, 64.0);
        assert_eq!(FILE_APPROVAL_MENU_WIDTH, 168.0);
        assert_eq!(FILE_APPROVAL_MENU_HEIGHT, 67.125);
        assert_eq!(FILE_APPROVAL_MENU_ROW_HEIGHT, 28.5625);
    }

    #[test]
    fn file_approval_geometry_expands_for_every_pending_file() {
        let model = FileApprovalPresentation::pending(
            "file-request-multi",
            vec![
                FileApprovalPathPresentation::new("/tmp/first.txt", 3, 1),
                FileApprovalPathPresentation::new("/tmp/second.txt", 2, 4),
            ],
            None,
        );

        assert_eq!(model.files.len(), 2);
        assert_eq!(model.file_list_content_height(), 66.0);
        assert_eq!(model.file_list_viewport_height(), 66.0);
        assert_eq!(model.files_height(), 82.0);
        assert_eq!(model.card_height(), 210.0);
        assert_eq!(model.files[0].directory_and_name(), ("/tmp/", "first.txt"));
        assert_eq!(model.files[1].directory_and_name(), ("/tmp/", "second.txt"));
    }

    #[test]
    fn long_file_approval_lists_use_the_real_two_hundred_pixel_scroll_cap() {
        let files = (0..8)
            .map(|index| {
                FileApprovalPathPresentation::new(format!("/tmp/file-{index}.txt"), index, 0)
            })
            .collect();
        let model = FileApprovalPresentation::pending("file-request-many", files, None);

        assert_eq!(model.files.len(), 8);
        assert_eq!(model.file_list_content_height(), 264.0);
        assert_eq!(model.file_list_viewport_height(), 200.0);
        assert_eq!(model.files_height(), 216.0);
        assert_eq!(model.card_height(), 344.0);
    }

    #[test]
    fn eight_file_approval_scrolls_the_full_repeated_row_list() {
        let model = captured_file_approval_fixture(ThemeMode::Dark, "many-default");
        let scroll = model.file_list_scroll_handle();
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(736.0), px(344.0)),
                })),
                ..Default::default()
            },
            |_, _| FileApprovalHarness { model },
        );

        window.draw();
        assert!((f32::from(scroll.max_offset().y) - 64.0).abs() < 0.01);
        assert_eq!(scroll.offset().y, px(0.0));

        window.simulate_event(ScrollWheelEvent {
            position: point(px(100.0), px(150.0)),
            delta: ScrollDelta::Pixels(point(px(0.0), px(-64.0))),
            touch_phase: TouchPhase::Started,
            ..Default::default()
        });
        assert!((f32::from(scroll.offset().y) + 64.0).abs() < 0.01);
    }

    #[test]
    fn approval_uses_captured_copy_and_explicit_reason() {
        let mut model = approval();
        assert_eq!(model.question(), "是否允许 ChatGPT 编辑以下文件？");
        model.reason = Some("仅允许创建此文件？".to_owned());
        assert_eq!(model.question(), "仅允许创建此文件？");
        model.reason = Some("  ".to_owned());
        assert_eq!(model.question(), "是否允许 ChatGPT 编辑以下文件？");
    }

    #[test]
    fn approval_path_preserves_directory_and_filename_emphasis_boundary() {
        let file = &approval().files[0];
        assert_eq!(file.directory_and_name().0, "/Users/zp/Desktop/");
        assert_eq!(
            file.directory_and_name().1,
            "codex-cdp-file-approval-probe.txt"
        );
    }

    #[test]
    fn resolved_approved_and_declined_surface_is_unmounted() {
        let mut model = approval();
        assert!(model.should_render());
        model.status = FileApprovalStatus::Resolved;
        assert!(!model.should_render());
    }

    #[test]
    fn file_approval_tracks_all_required_visual_states() {
        assert!(!FileApprovalVisualState::Default.menu_open());
        assert!(!FileApprovalVisualState::AllowHovered.menu_open());
        assert!(!FileApprovalVisualState::DeclineHovered.menu_open());
        let menu = FileApprovalVisualState::SplitMenu {
            focused: Some(FileApprovalMenuItem::AllowOnce),
        };
        assert!(menu.menu_open());
        assert_eq!(
            menu.focused_menu_item(),
            Some(FileApprovalMenuItem::AllowOnce)
        );
    }

    #[test]
    fn file_approval_keyboard_drives_decisions_and_menu_focus() {
        let mut model = approval();
        assert_eq!(
            model.keyboard_event("enter", false),
            Some(FileApprovalEvent::Decision(FileApprovalDecision::AllowOnce))
        );
        assert_eq!(
            model.keyboard_event("escape", false),
            Some(FileApprovalEvent::Decision(FileApprovalDecision::Decline))
        );
        assert_eq!(
            model.keyboard_event("tab", false),
            Some(FileApprovalEvent::KeyboardFocusChanged(Some(
                FileApprovalKeyboardFocus::Decline
            )))
        );

        model.visual_state = FileApprovalVisualState::SplitMenu { focused: None };
        model.keyboard_focus = Some(FileApprovalKeyboardFocus::MenuAllowOnce);
        assert_eq!(
            model.keyboard_event("down", false),
            Some(FileApprovalEvent::KeyboardFocusChanged(Some(
                FileApprovalKeyboardFocus::MenuAllowAllEdits
            )))
        );
        assert_eq!(
            model.keyboard_event("enter", false),
            Some(FileApprovalEvent::Decision(FileApprovalDecision::AllowOnce))
        );
        assert_eq!(
            model.keyboard_event("escape", false),
            Some(FileApprovalEvent::ToggleMenu)
        );
    }

    #[test]
    fn completed_activity_geometry_and_copy_match_cdp() {
        let activity = FileChangeActivityPresentation::edited(
            "file-item-1",
            "/tmp/chatgpt-cdp-file-approval-approved.txt",
            1,
            0,
        );
        assert_eq!(FILE_CHANGE_ACTIVITY_HEIGHT, 64.5);
        assert_eq!(FILE_CHANGE_ACTIVITY_ICON_SIZE, 40.0);
        assert_eq!(
            activity.title(),
            "已编辑 chatgpt-cdp-file-approval-approved.txt"
        );
        assert_eq!(activity.geometry_height(), 64.5);
    }

    #[test]
    fn completed_activity_hides_undo_until_a_real_reversal_exists() {
        let model = FileChangeActivityPresentation::edited(
            "file-item-no-fake-undo",
            "/tmp/real-domain-data.txt",
            3,
            2,
        );
        let expected_review = model.review.clone();
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured_events = events.clone();
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(500.0), px(80.0)),
                })),
                ..Default::default()
            },
            |_, _| FileChangeActivityHarness { model, events },
        );

        window.draw();

        // This was the center of the former Undo control. It must be inert
        // while no reversible domain operation or server acknowledgement
        // exists.
        window.simulate_click(point(px(404.0), px(32.0)), MouseButton::Left);
        assert!(captured_events.borrow().is_empty());

        // Review remains live and transports the presentation owned by this
        // activity rather than opening a capture fixture.
        window.simulate_click(point(px(466.0), px(32.0)), MouseButton::Left);
        assert_eq!(
            captured_events.borrow().as_slice(),
            &[FileChangeActivityEvent::OpenReview(expected_review)]
        );
    }

    #[test]
    fn review_collapsed_expanded_and_header_hover_geometry_are_independent() {
        assert_eq!(
            review_file(DiffFileVisualState::Collapsed).geometry_height(),
            34.0
        );
        assert_eq!(
            review_file(DiffFileVisualState::Expanded).geometry_height(),
            72.59375
        );
        let hovered = review_file(DiffFileVisualState::HeaderHovered);
        assert!(hovered.visual_state.is_expanded());
        assert!(hovered.visual_state.header_hovered());
        assert_eq!(hovered.geometry_height(), 72.59375);
    }

    #[test]
    fn review_rows_keep_the_captured_cumulative_fractional_cadence() {
        assert_eq!(review_diff_rows_height(56), 1228.25);
        assert_eq!(
            REVIEW_FILE_HEADER_HEIGHT + review_diff_rows_height(56),
            1260.25
        );

        let raster_tops = (0..9)
            .map(|index| review_diff_row_top(index).round() as i32)
            .collect::<Vec<_>>();
        assert_eq!(raster_tops, vec![2, 24, 45, 67, 88, 110, 132, 153, 175]);
        assert_eq!(review_diff_row_top(55), 1189.65625);
        assert_eq!(
            review_diff_row_top(55) + REVIEW_DIFF_LINE_HEIGHT + REVIEW_DIFF_BOTTOM_PADDING,
            review_diff_rows_height(56)
        );
        let logical_scroll_height = 2.0 * (REVIEW_FILE_HEADER_HEIGHT + review_diff_rows_height(56))
            + REVIEW_VIEWPORT_BOTTOM_PADDING;
        assert_eq!(logical_scroll_height, 2533.5);
        assert_eq!(logical_scroll_height.floor(), 2533.0);
    }

    #[test]
    fn review_totals_are_per_file_sums() {
        let mut model = DiffReviewPresentation::new(
            "review-1",
            "上一轮",
            vec![
                review_file(DiffFileVisualState::Expanded),
                DiffFilePresentation {
                    path: "/tmp/deleted.txt".to_owned(),
                    resolved_path: Some("/tmp/deleted.txt".to_owned()),
                    additions: 0,
                    deletions: 3,
                    lines: Vec::new(),
                    visual_state: DiffFileVisualState::Collapsed,
                },
            ],
        );
        model.show_summary = true;
        assert_eq!(model.total_additions(), 1);
        assert_eq!(model.total_deletions(), 3);
    }

    #[test]
    fn unified_diff_parser_keeps_all_files_lines_numbers_and_real_targets() {
        let diff = concat!(
            "diff --git a/src/first.rs b/src/first.rs\n",
            "--- a/src/first.rs\n",
            "+++ b/src/first.rs\n",
            "@@ -10,3 +10,4 @@\n",
            " context before\n",
            "-old value\n",
            "+new value\n",
            "+second new value\n",
            " context after\n",
            "diff --git a/tests/second.rs b/tests/second.rs\n",
            "--- a/tests/second.rs\n",
            "+++ b/tests/second.rs\n",
            "@@ -2,2 +2,2 @@\n",
            "-removed line\n",
            "+replacement line\n",
            " unchanged\n",
        );

        let model = DiffReviewPresentation::from_unified_diff(
            "turn-42",
            "上一轮",
            diff,
            Some(Path::new("/workspace/project")),
        );

        assert_eq!(model.files.len(), 2);
        assert_eq!(model.total_additions(), 3);
        assert_eq!(model.total_deletions(), 2);
        assert_eq!(model.files[0].path, "src/first.rs");
        assert_eq!(
            model.files[0].resolved_path.as_deref(),
            Some("/workspace/project/src/first.rs")
        );
        assert_eq!(model.files[0].lines.len(), 5);
        assert_eq!(
            model.files[0].lines,
            vec![
                DiffLinePresentation::context(10, 10, "context before"),
                DiffLinePresentation::deleted(11, "old value"),
                DiffLinePresentation::added(11, "new value"),
                DiffLinePresentation::added(12, "second new value"),
                DiffLinePresentation::context(12, 13, "context after"),
            ]
        );
        assert_eq!(model.files[1].path, "tests/second.rs");
        assert_eq!(model.files[1].lines.len(), 3);
        assert_eq!(model.files[1].lines[0].old_line, Some(2));
        assert_eq!(model.files[1].lines[0].new_line, None);
        assert_eq!(model.files[1].lines[1].old_line, None);
        assert_eq!(model.files[1].lines[1].new_line, Some(2));
        assert_eq!(model.files[1].lines[2].old_line, Some(3));
        assert_eq!(model.files[1].lines[2].new_line, Some(3));
        assert_eq!(model.files[0].geometry_height(), 158.96875);
        assert_eq!(model.files[1].geometry_height(), 115.78125);
    }

    #[test]
    fn light_and_dark_palettes_use_captured_values() {
        let light = FilePalette::for_theme(Theme::for_mode(ThemeMode::Light));
        let dark = FilePalette::for_theme(Theme::for_mode(ThemeMode::Dark));
        assert_eq!(light.card, rgba(0xffffffff));
        assert_eq!(dark.card, rgba(0x2c2c2cff));
        assert_eq!(light.outline, rgba(0x1a1c1f14));
        assert_eq!(dark.outline, rgba(0xffffff15));
        assert_eq!(light.menu, rgba(0xffffffff));
        assert_eq!(dark.menu, rgba(0x2d2d2dff));
        assert_eq!(dark.surface, rgba(0x181818ff));
        assert_eq!(dark.mode, ThemeMode::Dark);
    }

    #[test]
    fn captured_file_approval_fixture_uses_cdp_paths_counts_and_states() {
        let light = captured_file_approval_fixture(ThemeMode::Light, "options-focus");
        assert_eq!(
            light.files,
            vec![FileApprovalPathPresentation::new(
                "/Users/zp/Desktop/codex-cdp-file-approval-probe.txt",
                1,
                0,
            )]
        );
        assert_eq!(
            light.visual_state.focused_menu_item(),
            Some(FileApprovalMenuItem::AllowOnce)
        );

        let dark = captured_file_approval_fixture(ThemeMode::Dark, "declined-resolved");
        assert_eq!(
            dark.files[0].path,
            "/Users/zp/Desktop/codex-cdp-file-approval-dark-probe.txt"
        );
        assert_eq!(dark.status, FileApprovalStatus::Resolved);
    }

    #[test]
    fn captured_file_change_fixture_uses_completed_item_evidence() {
        let model = captured_file_change_activity_fixture("completed");
        assert_eq!(model.review.files.len(), 2);
        assert_eq!(
            model.path,
            "/Users/zp/Desktop/codex-cdp-multifile-approval-20260830-a.txt"
        );
        assert_eq!((model.additions, model.deletions), (2, 0));
        assert_eq!(model.title(), "已编辑 2 个文件");
        assert!(model.details_expanded);
        assert_eq!(model.geometry_height(), 137.5);
        assert_eq!(model.review.files[0].lines[0].content, "APPROVAL_A");
        assert_eq!(model.review.files[1].lines[0].content, "APPROVAL_B");
    }

    #[test]
    fn captured_turn_diff_fixture_uses_json_path_totals_and_review_states() {
        let collapsed = captured_diff_review_fixture("file-collapsed");
        assert_eq!(collapsed.turn_label, "上一轮");
        assert_eq!(CAPTURED_LONG_TWO_FILE_TURN_DIFF.split('\n').count(), 123);
        assert_eq!(collapsed.files.len(), 2);
        assert_eq!(
            collapsed.files[0].path,
            "../../../../tmp/codex-cdp-multifile-natural-20260830-a.txt"
        );
        assert_eq!(
            (collapsed.total_additions(), collapsed.total_deletions()),
            (16, 16)
        );
        assert_eq!(collapsed.files[0].lines.len(), 56);
        assert_eq!(collapsed.files[1].lines.len(), 56);
        assert_eq!(collapsed.files[0].geometry_height(), 34.0);
        assert_eq!(collapsed.files[1].geometry_height(), 1260.25);
        assert_eq!(
            collapsed.files[0].lines[0].content,
            "A-01 baseline context line"
        );
        assert_eq!(
            collapsed.files[1].resolved_path.as_deref(),
            Some("/tmp/codex-cdp-multifile-natural-20260830-b.txt")
        );
        assert_eq!(
            collapsed.files[0].visual_state,
            DiffFileVisualState::Collapsed
        );

        let hovered = captured_diff_review_fixture("file-header-hover");
        assert_eq!(
            hovered.files[0].visual_state,
            DiffFileVisualState::HeaderHovered
        );

        let bottom = captured_diff_review_fixture("bottom");
        assert_eq!(bottom.review_scroll_handle().offset().y, px(-1_209.0));
    }

    #[test]
    fn exact_long_review_scrolls_across_both_files_and_collapses_without_overflow() {
        let model = captured_diff_review_fixture("default");
        let scroll = model.review_scroll_handle();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(1357.640625), px(1410.0)),
                })),
                ..Default::default()
            },
            |_, _| DiffReviewHarness { model, events },
        );

        window.draw();
        let max_offset = f32::from(scroll.max_offset().y);
        assert!((1208.0..=1210.0).contains(&max_offset));
        window.simulate_event(ScrollWheelEvent {
            position: point(px(500.0), px(500.0)),
            delta: ScrollDelta::Pixels(point(px(0.0), px(-1100.0))),
            touch_phase: TouchPhase::Started,
            ..Default::default()
        });
        assert!((f32::from(scroll.offset().y) + 1100.0).abs() < 0.01);

        let collapsed = captured_diff_review_fixture("first-file-collapsed");
        let collapsed_scroll = collapsed.review_scroll_handle();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut collapsed_window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(1357.640625), px(1410.0)),
                })),
                ..Default::default()
            },
            |_, _| DiffReviewHarness {
                model: collapsed,
                events,
            },
        );
        collapsed_window.draw();
        assert_eq!(collapsed_scroll.max_offset().y, px(0.0));
    }

    #[test]
    fn completed_multifile_rows_open_the_owned_real_review_data() {
        let model = captured_file_change_activity_fixture("completed");
        let expected_review = model.review.clone();
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured_events = events.clone();
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(736.0), px(137.5)),
                })),
                ..Default::default()
            },
            |_, _| FileChangeActivityHarness { model, events },
        );

        window.draw();
        window.simulate_click(point(px(200.0), px(82.0)), MouseButton::Left);
        assert_eq!(
            captured_events.borrow().as_slice(),
            &[FileChangeActivityEvent::OpenReview(expected_review)]
        );
    }
}
